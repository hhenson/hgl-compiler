//! Writes temporal functions whose body is a graph's (FN-2: typed
//! connections and no payload read) as structs whose static `compose`
//! wires hgraph's graph, and an application's run statement (UNIT-14,
//! RUN-1). Part of the transpiler, the last stage; a family of the
//! dispatch point (`forms`) for the statements and the connections of a
//! graph.
//!
//! | HGL | C++ |
//! | --- | --- |
//! | `fn add_one(value: i64) -> i64 { return value + 1 }` | a struct `n_::add_one` with `static ::hgraph::Port<TS<Int>> compose(::hgraph::Wiring& wiring_, ::hgraph::Port<TS<Int>> value)`, and a wiring function `w_::add_one` that calls it |
//! | A temporal parameter; a `const` one | an `hgraph::Port` of its temporal form; the value, as a value function takes it |
//! | `let x = f(a)`, a connection | `const ::hgraph::Port<S> x = ...;` |
//! | `return x` (FN-7) | `return x;`: the connection the graph gives |
//! | A call of a temporal function | its wiring function, `w_::f(wiring_, a)`; a generic one's instance's |
//! | A lifted call, `value + 1` (FN-6); `capture` (ERR-7) | the lifting family's (`lifting`) |
//! | An ordinary value where a connection is expected (FN-3) | its temporal form's constant, `::hglrt::constant<T>(wiring_, value)` |
//!
//! Not written yet, and reported: `if`, `switch` and `for` over
//! connections (CTRL-7), a `var` connection, a call that leaves out a
//! temporal input (UNIT-8), `inject global_state` in a graph (FN-9), and
//! an application's run statement.

use hgl_analyser::hir::{Body, Callee, Expr, ExprKind, Stmt, ValueCategory};
use hgl_analyser::{DeclarationKind, Detail, LocalKind, Role as Phase, Signature, Type};
use hgl_lower::calls::{LeftOut, left_out};

use super::Emitter;
use super::calls::{changes, may_fail};
use super::forms::Family;
use super::names::{self, identifier};
use super::nodes::{Context, Kind, Placed, Role, nested};
use super::statements::Scope;
use super::values::INDENT;

/// The name of the wiring a graph's `compose`, a wiring function and an
/// eval's entry are given: hgraph's `Wiring`. No HGL name gives it
/// (`names::identifier`).
pub(super) const WIRING: &str = "wiring_";

impl Emitter<'_> {
    /// Writes the graph `index`, or its instance `instance`, of
    /// `signature` and `body`, which `shown` describes.
    pub(super) fn graph(
        &mut self,
        index: usize,
        instance: Option<&str>,
        signature: &Signature,
        body: &Body,
        shown: &str,
    ) {
        let program = self.program;
        let mut context = Context::new(Kind::Graph, body.locals.len());
        for (local, declared) in body.locals.iter().enumerate() {
            let temporal_parameter = signature
                .parameters
                .get(local)
                .is_some_and(|parameter| !parameter.constant);
            if temporal_parameter || declared.category == ValueCategory::Connection {
                context.roles[local] = Role::Connection;
            }
        }
        context.result = signature.result.clone();
        let mut parameters = vec![format!("::hgraph::Wiring& {WIRING}")];
        for (local, parameter) in signature.parameters.iter().enumerate() {
            let name = identifier(&body.locals[local].name);
            if parameter.constant {
                let Some(ty) = self.written_type(&parameter.ty, parameter.at) else {
                    return;
                };
                let taken = super::values::parameter_type(program, &parameter.ty, false, &ty);
                parameters.push(format!("[[maybe_unused]] {taken} {name}"));
            } else {
                let Some(schema) = self.temporal_schema(&parameter.ty, parameter.at) else {
                    return;
                };
                parameters.push(format!("[[maybe_unused]] ::hgraph::Port<{schema}> {name}"));
            }
        }
        let at = program.modules[self.module].declarations[index].at;
        let result = match &signature.result {
            Some(result) => match self.temporal_schema(result, at) {
                Some(schema) => format!("::hgraph::Port<{schema}>"),
                None => return,
            },
            None => "void".to_string(),
        };
        let mut scope = Scope::cold(program, body);
        let mut statements = String::new();
        self.in_context(&mut context, |emitter| {
            emitter.statements(&mut scope, &body.statements, 2, &mut statements);
        });
        let placed = Placed::of(program, self.module, index, instance);
        let qualified = placed.qualified(program, self.module);
        let mut text = format!("// {shown}\nstruct {}\n{{\n", placed.name);
        for lifted in &context.lifted {
            for line in lifted.lines() {
                text.push_str(&format!("{INDENT}{line}\n"));
            }
            text.push('\n');
        }
        text.push_str(&format!(
            "{INDENT}static {result} compose({})\n{INDENT}{{\n{statements}{INDENT}}}\n}};\n",
            parameters.join(", ")
        ));
        let mut whole = nested(&placed.structs, &text);
        let Some((prototype, wiring)) = self.wiring_function(
            signature,
            &placed,
            shown,
            &format!("{qualified}::compose"),
            body,
        ) else {
            return;
        };
        whole.push_str(&wiring);
        self.piece.declaration = prototype;
        self.piece.text = whole;
    }

    /// Writes the module's run statement, for an application (RUN-1).
    pub(super) fn application(&mut self) {
        let declared = &self.program.modules[self.module];
        let at = declared
            .run_body
            .as_ref()
            .map(|run| run.at)
            .or_else(|| declared.run.as_ref().map(|run| run.at))
            .or_else(|| declared.application_at());
        match at {
            Some(at) => self.unsupported(at, "an application's run statement"),
            None => {
                // An application with no run statement to read was
                // rejected; there is nothing to write.
            }
        }
    }

    /// `value` where a connection of `ty` is expected: the port of a
    /// connection (`forms::wiring_form`), or an ordinary value wired as a
    /// constant (FN-3: "An ordinary value of type T is accepted where
    /// composition expects a connection of T").
    pub(super) fn connection(&mut self, scope: &mut Scope, value: &Expr, ty: &Type) -> String {
        if value.is_connection() {
            return self.wiring_form(scope, value);
        }
        let written = self.expression(scope, value);
        self.temporal_constant(ty, &written, value.at)
    }

    /// The call `value` of a temporal function, wired: its wiring
    /// function, given the wiring, a connection for each temporal
    /// parameter and a value for each `const` one. `given` are the ports
    /// already wired for some of its arguments, by their positions among
    /// the call's arguments: an eval's replays.
    pub(super) fn temporal_call(
        &mut self,
        scope: &mut Scope,
        value: &Expr,
        given: &[(usize, String)],
    ) -> String {
        let ExprKind::Call {
            callee:
                Callee::Function {
                    module,
                    declaration,
                    generics,
                    ..
                }
                | Callee::Candidate {
                    module,
                    declaration,
                    generics,
                    ..
                },
            arguments,
            ..
        } = &value.kind
        else {
            return self.missing(value.at, "this call");
        };
        let program = self.program;
        let (module, declaration) = (*module, *declaration);
        let Some(Detail::Callable(declared)) = program.modules[module].details.get(declaration)
        else {
            return self.missing(value.at, "this call");
        };
        let (signature, suffix): (&Signature, Option<String>) = if declared.generics.is_empty() {
            (declared, None)
        } else {
            let instances = self.instances;
            match instances.find(program, (module, declaration), generics) {
                Some(instance) => (&instance.signature, Some(instance.suffix.clone())),
                None => return self.missing(value.at, "a call of an instance that is not written"),
            }
        };
        self.piece
            .reach
            .uses
            .insert((module, declaration, suffix.clone()));
        let placed = Placed::of(program, module, declaration, suffix.as_deref());
        let mut written: Vec<Option<String>> = vec![None; signature.parameters.len()];
        // The parameters in the order their arguments are written, and
        // what may fail or run statements among those (EXPR-2).
        let mut order: Vec<usize> = Vec::new();
        let mut failing = 0;
        let mut changing = false;
        for (position, argument) in arguments.iter().enumerate() {
            let Some(index) = argument
                .parameter
                .filter(|&index| index < signature.parameters.len())
            else {
                return self.missing(argument.value.at, "an argument given to a pack");
            };
            let parameter = &signature.parameters[index];
            let port = given
                .iter()
                .find(|(at, _)| *at == position)
                .map(|(_, port)| port.clone());
            written[index] = Some(match port {
                Some(port) => port,
                None if parameter.constant => self.expression(scope, &argument.value),
                None => self.connection(scope, &argument.value, &parameter.ty),
            });
            order.push(index);
            failing += usize::from(may_fail(&argument.value));
            changing |= changes(&argument.value);
        }
        for (index, parameter) in signature.parameters.iter().enumerate() {
            if written[index].is_some() {
                continue;
            }
            if !parameter.constant {
                // UNIT-8: "an omitted temporal input is never valid".
                return self.missing(value.at, "a call that leaves out a temporal input");
            }
            written[index] = Some(
                match left_out(program, (module, declaration), index, parameter) {
                    Some(LeftOut::Default(default)) => {
                        let cold = Body {
                            locals: Vec::new(),
                            statements: Vec::new(),
                        };
                        let mut defaults = Scope::cold(program, &cold);
                        let written = self.expression(&mut defaults, default);
                        if !defaults.before.is_empty() {
                            return self.missing(default.at, "a default that propagates a failure");
                        }
                        written
                    }
                    Some(LeftOut::Null) => "std::nullopt".to_string(),
                    None => {
                        return self.missing(
                            value.at,
                            &format!("a call that leaves out `{}`", parameter.name),
                        );
                    }
                },
            );
        }
        let called = placed.wiring_qualified(program, module);
        // EXPR-2: the arguments in the order written, then the defaults of
        // those left out. C++ evaluates a call's arguments in no order it
        // promises, so where more than one may fail, or one runs
        // statements, each is held first, in that order.
        if failing > 1 || changing {
            let rest: Vec<usize> = (0..written.len())
                .filter(|index| !order.contains(index))
                .collect();
            order.extend(rest);
            let mut steps = String::new();
            for index in order {
                let name = names::temporary("argument", self.temporary());
                let held = written[index].replace(name.clone()).unwrap_or_default();
                steps.push_str(&format!("const auto {name} = {held}; "));
            }
            let mut all = vec![WIRING.to_string()];
            all.extend(written.into_iter().flatten());
            return format!("[&] {{ {steps}return {called}({}); }}()", all.join(", "));
        }
        let mut all = vec![WIRING.to_string()];
        all.extend(written.into_iter().flatten());
        format!("{called}({})", all.join(", "))
    }
}

/// The graphs' family: the statements of a graph, and its connections.
pub(super) struct Graphs;

impl Family for Graphs {
    fn statement(
        &self,
        emitter: &mut Emitter<'_>,
        scope: &mut Scope<'_>,
        statement: &Stmt,
        depth: usize,
        text: &mut String,
    ) -> bool {
        if emitter.kind() != Some(Kind::Graph) {
            return false;
        }
        let indent = INDENT.repeat(depth);
        match statement {
            Stmt::Declare {
                local,
                value: Some(value),
            } if emitter.role(*local) == Role::Connection => {
                let declared = &scope.body.locals[*local];
                if declared.kind == LocalKind::Var {
                    emitter.unsupported(declared.at, "a `var` connection");
                    return true;
                }
                let (ty, at, name) = (declared.ty.clone(), declared.at, identifier(&declared.name));
                let port = emitter.connection(scope, value, &ty);
                if let Some(schema) = emitter.temporal_schema(&ty, at) {
                    text.push_str(&format!(
                        "{indent}[[maybe_unused]] const ::hgraph::Port<{schema}> {name} = {port};\n"
                    ));
                }
                true
            }
            // FN-7: "A composition return selects a connection".
            Stmt::Return {
                value: Some(value),
                at,
            } => {
                let Some(result) = emitter
                    .temporal
                    .as_ref()
                    .and_then(|context| context.result.clone())
                else {
                    emitter.unsupported(*at, "a `return` with a value here");
                    return true;
                };
                let port = emitter.connection(scope, value, &result);
                text.push_str(&format!("{indent}return {port};\n"));
                true
            }
            // A call that stands as a statement: a node with no result,
            // or one whose result is not taken.
            Stmt::Expr(value) if wires(emitter, value) => {
                let port = emitter.wiring_form(scope, value);
                if gives_port(emitter, value) {
                    text.push_str(&format!("{indent}static_cast<void>({port});\n"));
                } else {
                    text.push_str(&format!("{indent}{port};\n"));
                }
                true
            }
            Stmt::Inject { at, .. } => {
                emitter.unsupported(*at, "an `inject` in a graph");
                true
            }
            _ => false,
        }
    }

    // FN-3: "A composition body cannot read current input payloads": a
    // connection that is not where a graph's statements take one is not
    // written yet.
    fn expression(
        &self,
        emitter: &mut Emitter<'_>,
        _scope: &mut Scope<'_>,
        value: &Expr,
    ) -> Option<String> {
        (emitter.kind() == Some(Kind::Graph) && value.is_connection())
            .then(|| emitter.missing(value.at, "a connection here"))
    }

    fn wiring(
        &self,
        emitter: &mut Emitter<'_>,
        scope: &mut Scope<'_>,
        value: &Expr,
    ) -> Option<String> {
        match &value.kind {
            ExprKind::Local(local) if emitter.role(*local) == Role::Connection => {
                Some(identifier(&scope.body.locals[*local].name))
            }
            ExprKind::Call { callee, .. } => {
                let (module, declaration) = callee.declaration()?;
                let declared = &emitter.program.modules[module].declarations[declaration];
                match declared.kind {
                    DeclarationKind::Function(Phase::Temporal) => {
                        Some(emitter.temporal_call(scope, value, &[]))
                    }
                    // GEN-5: a temporal candidate is the node or the graph
                    // its call selects, wired as a temporal function is.
                    DeclarationKind::Implementation(Phase::Temporal, _) => {
                        Some(emitter.temporal_call(scope, value, &[]))
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

/// True where `value`, a statement of a graph, is a call that is wired:
/// of a temporal function, or lifted.
fn wires(emitter: &Emitter, value: &Expr) -> bool {
    let ExprKind::Call { callee, .. } = &value.kind else {
        return false;
    };
    value.is_lifted()
        || callee.declaration().is_some_and(|(module, declaration)| {
            emitter.program.modules[module].declarations[declaration]
                .kind
                .role()
                == Some(Phase::Temporal)
        })
}

/// True where the call `value` gives a connection: its function has a
/// result.
fn gives_port(emitter: &Emitter, value: &Expr) -> bool {
    let ExprKind::Call { callee, .. } = &value.kind else {
        return false;
    };
    callee.declaration().is_some_and(|(module, declaration)| {
        matches!(
            emitter.program.modules[module].details.get(declaration),
            Some(Detail::Callable(signature)) if signature.result.is_some()
        )
    })
}
