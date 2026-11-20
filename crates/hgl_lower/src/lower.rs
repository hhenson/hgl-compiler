//! Select and inline the reachable graph, resolving argument order and child scope once.
use super::*;
use hgl_analyser::hir::{self, Argument, Body, Callee, Expr, ExprKind as E, Form, Hook, Stmt};
use hgl_analyser::{
    DeclarationKind, Detail, LocalKind, Module, Program, Role, Scalar, Signature, Type,
};
use std::collections::BTreeMap;

type R<T> = Result<T, Unsupported>;
type Id = (usize, usize);
#[derive(Clone)]
enum Binding {
    Wire(Port),
    Constant(Literal),
    Store,
    Value(Value),
}
type Env = BTreeMap<usize, Binding>;

fn schema(ty: &Type, at: Location) -> R<Schema> {
    match ty {
        Type::Scalar(Scalar::I64) => Ok(Schema::Integer),
        Type::Scalar(Scalar::Str) => Ok(Schema::Text),
        Type::Scalar(Scalar::Bool) => Ok(Schema::Flag),
        Type::Dict { key, value }
            if **key == Type::Scalar(Scalar::Str) && **value == Type::Scalar(Scalar::I64) =>
        {
            Ok(Schema::Dictionary)
        }
        _ => Err(Unsupported(at, "this payload schema")),
    }
}
struct Lower<'a> {
    program: &'a Program,
    stack: Vec<Id>,
    next_local: usize,
}
impl<'a> Lower<'a> {
    fn new(program: &'a Program) -> Self {
        Self {
            program,
            stack: vec![],
            next_local: 0,
        }
    }
    fn callable(&self, id: Id, at: Location) -> R<(&'a Module, &'a Signature, &'a Body)> {
        let m = &self.program.modules[id.0];
        let Detail::Callable(sig) = &m.details[id.1] else {
            return Err(Unsupported(at, "this callable"));
        };
        if !sig.generics.is_empty() || sig.parameters.iter().any(|p| p.var || p.pack.is_some()) {
            return Err(Unsupported(at, "a generic, var or packed callable"));
        }
        Ok((
            m,
            sig,
            m.bodies[id.1]
                .as_ref()
                .ok_or(Unsupported(at, "a missing implementation"))?,
        ))
    }
    fn native(&self, c: &Callee) -> Option<&str> {
        let (m, d) = c.declaration()?;
        let decl = &self.program.modules[m].declarations[d];
        (Some(m) == self.program.core()
            && matches!(decl.kind, DeclarationKind::NativeCandidate(Role::Value, _)))
        .then_some(decl.name.as_str())
    }
    fn literal(&self, e: &Expr) -> R<Literal> {
        match &e.kind {
            E::Literal(hir::Value::Int(i)) => Ok(Literal::Int(*i)),
            E::Literal(hir::Value::Str(s)) => Ok(Literal::Text(s.clone())),
            E::Literal(hir::Value::Bool(v)) => Ok(Literal::Bool(*v)),
            E::Call {
                callee,
                arguments,
                form: Form::Unary(hir::UnaryOperator::Negate),
            } if self.native(callee) == Some("neg_") => {
                if let [a] = arguments.as_slice()
                    && let Literal::Int(v) = self.literal(&a.value)?
                    && let Some(i) = v.checked_neg()
                {
                    return Ok(Literal::Int(i));
                }
                Err(Unsupported(e.at, "this negative literal"))
            }
            _ => Err(Unsupported(
                e.at,
                "a non-literal replay or configuration value",
            )),
        }
    }
    fn trace(&self, e: &Expr) -> R<Vec<(i64, Literal)>> {
        let E::Trace(cells) = &e.kind else {
            return Err(Unsupported(e.at, "this trace"));
        };
        let mut events = vec![];
        for (step, c) in cells.iter().enumerate() {
            if c.time.is_some() {
                return Err(Unsupported(e.at, "a timed test trace"));
            }
            if let Some(v) = &c.value {
                events.push((step as i64, self.literal(v)?))
            }
        }
        Ok(events)
    }
    fn bind(&mut self, e: &Expr, env: &Env, g: &mut Graph) -> R<Binding> {
        match &e.kind {
            E::Local(id) => env
                .get(id)
                .cloned()
                .ok_or(Unsupported(e.at, "an unbound graph local")),
            E::Call { callee, .. } if self.native(callee).is_none() => self
                .call(e, env, g)?
                .map(Binding::Wire)
                .ok_or(Unsupported(e.at, "an outputless graph argument")),
            _ => Ok(Binding::Constant(self.literal(e)?)),
        }
    }
    fn call(&mut self, e: &Expr, outer: &Env, g: &mut Graph) -> R<Option<Port>> {
        let E::Call {
            callee, arguments, ..
        } = &e.kind
        else {
            return Err(Unsupported(e.at, "this graph expression"));
        };
        let id = callee
            .declaration()
            .ok_or(Unsupported(e.at, "an unresolved call"))?;
        let (m, sig, body) = self.callable(id, e.at)?;
        if m.declarations[id.1].kind != DeclarationKind::Function(Role::Temporal) {
            return Err(Unsupported(e.at, "a lifted or native graph call"));
        }
        let mut env = Env::new();
        for a in arguments {
            let i = a
                .parameter
                .ok_or(Unsupported(e.at, "this argument binding"))?;
            let binding = if matches!(a.value.kind, E::Trace(_)) {
                let events = self.trace(&a.value)?;
                Binding::Wire(append(
                    g,
                    Node::Replay(schema(&sig.parameters[i].ty, e.at)?, events),
                ))
            } else {
                self.bind(&a.value, outer, g)?
            };
            env.insert(i, binding);
        }
        if env.len() != sig.parameters.len() {
            return Err(Unsupported(e.at, "an omitted argument"));
        }
        if self.stack.contains(&id) {
            return Err(Unsupported(e.at, "a recursive graph"));
        }
        self.stack.push(id);
        let result = if body
            .statements
            .iter()
            .any(|s| matches!(s, Stmt::Hook { .. }))
        {
            let mut ports = vec![];
            let mut types = vec![];
            for (i, p) in sig.parameters.iter().enumerate() {
                match (&env[&i], p.constant) {
                    (Binding::Wire(port), false) => {
                        let ty = schema(&p.ty, p.at)?;
                        let slot = ports.len();
                        ports.push(*port);
                        types.push(ty);
                        env.insert(i, Binding::Value(Value::Input(slot, ty)));
                    }
                    (Binding::Constant(_), true) => {}
                    _ => return Err(Unsupported(p.at, "a lifted scalar argument")),
                }
            }
            let output = sig.result.as_ref().map(|t| schema(t, e.at)).transpose()?;
            let mut k = Kernel {
                name: format!("{}::{}", m.path, m.declarations[id.1].name),
                inputs: types,
                output,
                state: false,
                start: vec![],
                eval: vec![],
                stop: vec![],
            };
            // A source and a lifecycle-only node need different scheduling builders.
            if k.inputs.is_empty() {
                return Err(Unsupported(e.at, "a callback node without temporal inputs"));
            }
            self.kernel(body, &mut env, &mut k, e.at)?;
            let p = append(g, Node::Call(k, ports));
            output.map(|_| p)
        } else if body
            .statements
            .iter()
            .all(|s| matches!(s, Stmt::Yield { .. }))
            && !body.statements.is_empty()
        {
            let mut events = vec![];
            let mut step = 0i64;
            for s in &body.statements {
                let Stmt::Yield { time, value, .. } = s else {
                    unreachable!()
                };
                let E::Literal(hir::Value::Duration(delay)) = time.kind else {
                    return Err(Unsupported(time.at, "a non-literal yield delay"));
                };
                if delay < 0 || (delay == 0 && !events.is_empty()) {
                    return Err(Unsupported(time.at, "non-increasing yield times"));
                }
                step = step
                    .checked_add(delay)
                    .ok_or(Unsupported(time.at, "a yield past the clock range"))?;
                let Binding::Constant(v) = self.bind(value, &env, g)? else {
                    return Err(Unsupported(value.at, "a non-constant source payload"));
                };
                events.push((step, v));
            }
            let ty = schema(
                sig.result
                    .as_ref()
                    .ok_or(Unsupported(e.at, "an outputless source"))?,
                e.at,
            )?;
            Some(append(g, Node::Replay(ty, events)))
        } else {
            self.wire(&body.statements, &mut env, g, e.at)?
        };
        self.stack.pop();
        if sig.result.is_some() != result.is_some() {
            return Err(Unsupported(
                e.at,
                "a graph result without an explicit connection",
            ));
        }
        Ok(result)
    }
    fn wire(
        &mut self,
        stmts: &[Stmt],
        env: &mut Env,
        g: &mut Graph,
        at: Location,
    ) -> R<Option<Port>> {
        for s in stmts {
            match s {
                Stmt::For {
                    names,
                    sequence,
                    body,
                    ..
                } if names.len() == 2
                    && schema(&sequence.ty, sequence.at)? == Schema::Dictionary =>
                {
                    let Binding::Wire(over) = self.bind(sequence, env, g)? else {
                        return Err(Unsupported(sequence.at, "ordinary iteration"));
                    };
                    let mut child = Graph::default();
                    // CTRL-15: only ordinary outer values cross this slice's child boundary.
                    let mut nested: Env = env
                        .iter()
                        .filter(|(_, v)| matches!(v, Binding::Constant(_)))
                        .map(|(i, v)| (*i, v.clone()))
                        .collect();
                    nested.insert(names[0], Binding::Wire(Port::Key));
                    nested.insert(names[1], Binding::Wire(Port::Element));
                    if self.wire(body, &mut nested, &mut child, at)?.is_some() {
                        return Err(Unsupported(at, "a returning map child"));
                    }
                    let [Node::Call(k, ports)] = child.nodes.as_slice() else {
                        return Err(Unsupported(at, "a multi-node dictionary child"));
                    };
                    if k.output.is_some()
                        || ports.iter().filter(|p| **p == Port::Key).count() != 1
                        || ports.iter().filter(|p| **p == Port::Element).count() != 1
                    {
                        return Err(Unsupported(at, "this child binding"));
                    }
                    append(g, Node::Each(over, child));
                }
                Stmt::Declare {
                    local,
                    value: Some(v),
                } => {
                    let b = self.bind(v, env, g)?;
                    env.insert(*local, b);
                }
                Stmt::Return { value: Some(v), .. } => {
                    let Binding::Wire(port) = self.bind(v, env, g)? else {
                        return Err(Unsupported(v.at, "a constant graph output"));
                    };
                    return Ok(Some(port));
                }
                Stmt::Expr(e) => {
                    self.call(e, env, g)?;
                }
                _ => return Err(Unsupported(at, "this graph statement")),
            }
        }
        Ok(None)
    }
    fn kernel(&mut self, b: &Body, env: &mut Env, k: &mut Kernel, at: Location) -> R<()> {
        for s in &b.statements {
            match s {
                Stmt::Inject { local, .. } if b.locals[*local].name == "global_state" => {
                    env.insert(*local, Binding::Store);
                }
                Stmt::Declare {
                    local,
                    value: Some(v),
                } if !k.state
                    && b.locals[*local].kind == LocalKind::State
                    && b.locals[*local].ty == Type::Scalar(Scalar::I64) =>
                {
                    k.start.push(Action::Set(self.value(v, env)?));
                    k.state = true;
                    env.insert(*local, Binding::Value(Value::State));
                }
                Stmt::Hook {
                    hook,
                    body,
                    condition,
                    header,
                    ..
                } => {
                    let actions = self.actions(body, &mut env.clone(), at)?;
                    match hook {
                        Hook::Start => k.start.extend(actions),
                        Hook::Stop => k.stop.extend(actions),
                        Hook::When => {
                            let h = header
                                .as_ref()
                                .ok_or(Unsupported(at, "an unresolved handler header"))?;
                            let supported_condition = condition.as_ref().is_none_or(|c| matches!(
                                &c.kind,
                                E::Endpoint {operation: hir::EndpointOperation::Modified, inputs: Some(ids)}
                                    if !ids.is_empty()
                            ));
                            if h.scheduled
                                || h.beside.is_some()
                                || !supported_condition
                                || !k.eval.is_empty()
                            {
                                return Err(Unsupported(
                                    at,
                                    "this handler condition or scheduling pattern",
                                ));
                            }
                            let slots = |ids: &[usize]| {
                                ids.iter()
                                    .map(|i| match env.get(i) {
                                        Some(Binding::Value(Value::Input(s, _))) => Ok(*s),
                                        _ => Err(Unsupported(at, "a non-input selector")),
                                    })
                                    .collect::<R<Vec<_>>>()
                            };
                            k.eval.push(Action::Admit(
                                slots(&h.required)?,
                                slots(&h.subscribed)?,
                                actions,
                            ));
                        }
                    }
                }
                _ => return Err(Unsupported(at, "this node declaration")),
            }
        }
        Ok(())
    }
    fn value(&self, e: &Expr, env: &Env) -> R<Value> {
        let boxed = |e: &Expr| self.value(e, env).map(Box::new);
        match &e.kind {
            E::Local(i) => match env.get(i) {
                Some(Binding::Constant(v)) => Ok(Value::Literal(v.clone())),
                Some(Binding::Value(Value::Input(_, Schema::Dictionary))) => {
                    Err(Unsupported(e.at, "a whole dictionary read"))
                }
                Some(Binding::Value(v)) => Ok(v.clone()),
                _ => Err(Unsupported(e.at, "this value binding")),
            },
            E::Endpoint {
                operation: hir::EndpointOperation::DeltaValue,
                inputs: Some(ids),
            } if ids.len() == 1 => {
                if let Some(Binding::Value(Value::Input(i, t))) = env.get(&ids[0])
                    && *t != Schema::Dictionary
                {
                    return Ok(Value::Delta(*i, *t));
                }
                Err(Unsupported(e.at, "a composite delta read"))
            }
            E::If {
                condition,
                then,
                otherwise: Some(other),
            } => Ok(Value::Choose(
                boxed(condition)?,
                boxed(then)?,
                boxed(other)?,
            )),
            E::Block {
                statements,
                value: Some(v),
            } if statements.is_empty() => self.value(v, env),
            E::Call {
                callee, arguments, ..
            } => {
                let args = ordered(arguments, e.at)?;
                match (self.native(callee), args.as_slice()) {
                    (Some("has" | "get"), [store, key])
                        if store_arg(store, env)
                            && (self.native(callee) == Some("has")
                                || e.ty == Type::Scalar(Scalar::I64)) =>
                    {
                        let key = boxed(key)?;
                        Ok(if self.native(callee) == Some("has") {
                            Value::Has(key)
                        } else {
                            Value::Get(key)
                        })
                    }
                    (Some("add_"), [l, r]) if e.ty == Type::Scalar(Scalar::I64) => {
                        Ok(Value::Add(boxed(l)?, boxed(r)?))
                    }
                    (Some("eq_"), [l, r]) => Ok(Value::Equal(boxed(l)?, boxed(r)?)),
                    (Some("lt_"), [l, r]) => Ok(Value::Less(boxed(l)?, boxed(r)?)),
                    _ => self.literal(e).map(Value::Literal),
                }
            }
            _ => self.literal(e).map(Value::Literal),
        }
    }
    fn actions(&mut self, stmts: &[Stmt], env: &mut Env, at: Location) -> R<Vec<Action>> {
        let mut out = vec![];
        for s in stmts {
            match s {
                Stmt::Assign { local, value, .. }
                    if matches!(env.get(local), Some(Binding::Value(Value::State))) =>
                {
                    out.push(Action::Set(self.value(value, env)?))
                }
                Stmt::Declare {
                    local,
                    value: Some(v),
                } => {
                    let val = self.value(v, env)?;
                    let n = self.next_local;
                    self.next_local += 1;
                    out.push(Action::Let(n, val));
                    env.insert(*local, Binding::Value(Value::Local(n)));
                }
                Stmt::Expr(e) => self.effect(e, env, &mut out)?,
                Stmt::Return { value: Some(e), .. } => {
                    if let E::Delta { arguments } = &e.kind {
                        let [a] = arguments.as_slice() else {
                            return Err(Unsupported(e.at, "a multi-part delta"));
                        };
                        let op = match (a.parameter, &a.value.kind) {
                            (Some(0), E::Pairs(items)) if items.len() == 1 => Action::Update(
                                self.value(&items[0].0, env)?,
                                self.value(&items[0].1, env)?,
                            ),
                            (Some(1), E::Sequence(items)) if items.len() == 1 => {
                                Action::Remove(self.value(&items[0], env)?)
                            }
                            _ => return Err(Unsupported(e.at, "this dictionary delta")),
                        };
                        out.push(op);
                    } else {
                        if !matches!(
                            schema(&e.ty, e.at)?,
                            Schema::Integer | Schema::Text | Schema::Flag
                        ) {
                            return Err(Unsupported(e.at, "a composite publication"));
                        }
                        out.push(Action::Publish(self.value(e, env)?))
                    }
                }
                _ => return Err(Unsupported(at, "this callback statement")),
            }
        }
        Ok(out)
    }
    fn effect(&mut self, e: &Expr, env: &Env, out: &mut Vec<Action>) -> R<()> {
        match &e.kind {
            E::If {
                condition,
                then,
                otherwise: None,
            } => {
                let mut branch = vec![];
                self.effect(then, env, &mut branch)?;
                out.push(Action::If(self.value(condition, env)?, branch));
                Ok(())
            }
            E::Block { statements, value } => {
                let mut scope = env.clone();
                out.extend(self.actions(statements, &mut scope, e.at)?);
                if let Some(v) = value {
                    self.effect(v, &scope, out)?
                }
                Ok(())
            }
            E::Call {
                callee, arguments, ..
            } => {
                let args = ordered(arguments, e.at)?;
                if self.native(callee) == Some("put")
                    && let [store, key, v] = args.as_slice()
                    && store_arg(store, env)
                    && v.ty == Type::Scalar(Scalar::I64)
                {
                    out.push(Action::Put(self.value(key, env)?, self.value(v, env)?));
                    return Ok(());
                }
                let id = callee
                    .declaration()
                    .ok_or(Unsupported(e.at, "this callback call"))?;
                let (m, sig, b) = self.callable(id, e.at)?;
                if m.declarations[id.1].kind != DeclarationKind::Function(Role::Value)
                    || sig.result.is_some()
                    || self.stack.contains(&id)
                    || args.len() != sig.parameters.len()
                {
                    return Err(Unsupported(e.at, "this value helper"));
                }
                let mut scope = Env::new();
                for (i, a) in args.iter().enumerate() {
                    scope.insert(
                        i,
                        if store_arg(a, env) {
                            Binding::Store
                        } else {
                            let local = self.next_local;
                            self.next_local += 1;
                            out.push(Action::Let(local, self.value(a, env)?));
                            Binding::Value(Value::Local(local))
                        },
                    );
                }
                self.stack.push(id);
                out.extend(self.actions(&b.statements, &mut scope, e.at)?);
                self.stack.pop();
                Ok(())
            }
            _ => Err(Unsupported(e.at, "this effect")),
        }
    }
}
fn append(g: &mut Graph, n: Node) -> Port {
    let p = Port::Node(g.nodes.len());
    g.nodes.push(n);
    p
}
fn store_arg(e: &Expr, env: &Env) -> bool {
    matches!(&e.kind,E::Local(id) if matches!(env.get(id),Some(Binding::Store)))
}
fn ordered(args: &[Argument], at: Location) -> R<Vec<&Expr>> {
    let mut out = vec![None; args.len()];
    for a in args {
        let slot = a
            .parameter
            .and_then(|i| out.get_mut(i))
            .ok_or(Unsupported(at, "this argument binding"))?;
        *slot = Some(&a.value)
    }
    out.into_iter()
        .map(|v| v.ok_or(Unsupported(at, "a missing argument")))
        .collect()
}
/// Lower a selected ordinary HGL test, resolving only the declarations it reaches.
pub fn test(program: &Program, module: &Module, index: usize) -> R<Vec<Case>> {
    let t = &module.tests[index];
    let body = module.test_bodies[index]
        .as_ref()
        .ok_or(Unsupported(t.at, "a missing test body"))?;
    if t.integration || body.statements.is_empty() {
        return Err(Unsupported(t.at, "this test kind"));
    }
    body.statements
        .iter()
        .map(|s| {
            let Stmt::Assert { condition, at } = s else {
                return Err(Unsupported(t.at, "this test statement"));
            };
            let E::TraceCompare {
                left,
                right,
                negated: false,
            } = &condition.kind
            else {
                return Err(Unsupported(*at, "this assertion"));
            };
            let E::Eval {
                call,
                real_time: false,
                config: None,
            } = &left.kind
            else {
                return Err(Unsupported(*at, "this evaluation"));
            };
            let mut l = Lower::new(program);
            let mut graph = Graph::default();
            graph.result = l.call(call, &Env::new(), &mut graph)?;
            if graph.result.is_none() || call.ty != Type::Scalar(Scalar::I64) {
                return Err(Unsupported(*at, "a non-integer recording"));
            }
            let expected = l
                .trace(right)?
                .into_iter()
                .map(|(s, v)| {
                    if let Literal::Int(i) = v {
                        Ok((s, i))
                    } else {
                        Err(Unsupported(right.at, "a non-integer expectation"))
                    }
                })
                .collect::<R<_>>()?;
            Ok(Case {
                graph,
                expected,
                at: *at,
            })
        })
        .collect()
}
/// Lower a finite simulation entry after application argument replacement (RUN-1).
pub fn application(program: &Program, module: &Module) -> R<Graph> {
    let at = module.application_at().expect("application module");
    let r = module
        .run_body
        .as_ref()
        .ok_or(Unsupported(at, "a missing application entry"))?;
    if r.real_time || r.config.is_some() || module.store_body.is_some() {
        return Err(Unsupported(
            r.at,
            "a configured, store-seeded or real-time application",
        ));
    }
    let mut g = Graph::default();
    g.result = Lower::new(program).call(&r.call, &Env::new(), &mut g)?;
    if g.result.is_some() && r.call.ty != Type::Scalar(Scalar::I64) {
        return Err(Unsupported(r.at, "a non-integer recording"));
    }
    Ok(g)
}
