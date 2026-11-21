//! What a value is in a graph: a connection, or an ordinary value known
//! while the graph is built, and the places that take no connection
//! (FN-3, FN-6, CTRL-14, EXPR-14). Part of the typing pass ([`super`]).

use hgl_diagnostics::{Category, Diagnostic, Diagnostics, Location};

use super::Typer;
use super::control::{bound_by_loop, no_else_type, while_connection};
use super::fit::{open, whole};
use crate::hir::{
    Body, Callee, Expr, ExprKind, Local, LocalId, Node, NodeMut, Run, Stmt, ValueCategory,
    walk_statement_nodes, walk_statement_nodes_mut, walk_statements_mut,
};
use crate::program::{DeclarationKind, Program, Role};
use crate::types::{Detail, Type};
use crate::{codes, error};

impl<'a> Typer<'a> {
    /// The category of a graph's `var` against that of a value assigned
    /// to it.
    ///
    /// CTRL-14: "A `var` keeps the category of its first value
    /// (EXPR-3): a connection assigned to one that holds an ordinary
    /// value, or the reverse, rejects (EXPR-4). Under a temporal
    /// condition a `var` declared before the `if` is assigned in a
    /// branch only where it is a connection: one that holds an ordinary
    /// value would have its value chosen at runtime, and the assignment
    /// rejects." A `var` declared with no value takes the category of
    /// the first value it is assigned, and what is not settled is not
    /// judged.
    ///
    /// `given` is the category of what is assigned, and where it is
    /// written. `inside` is true for an assignment into a place of the
    /// `var` (EXPR-15). The `var` is then rebound to itself with one
    /// place another value: a connection stays one whatever the place
    /// is given, an ordinary value wired there as a constant (FN-3),
    /// and an ordinary value given a connection, or a place a
    /// connection selects, would become one.
    pub(super) fn assigned_category(
        &mut self,
        local: LocalId,
        (given, at): (ValueCategory, Location),
        inside: bool,
    ) {
        if self.locals[local].category == ValueCategory::Unsettled && !inside {
            self.locals[local].category = given;
        }
        // Declared before the nearest `if` selected at runtime.
        let chosen_at_runtime = self.temporal.is_some_and(|outside| local < outside);
        if let Some(report) =
            assignment_category(&self.locals[local], (given, at), inside, chosen_at_runtime)
        {
            self.diagnostics.emit(report);
        }
    }

    /// What a value read in a graph is (FN-3), as [`category`] says,
    /// with the locals read so far.
    pub(super) fn category(&self, value: &Expr) -> ValueCategory {
        category(self.program, &self.locals, value)
    }

    /// FN-3: "Where a constant is required ... a `const` parameter's
    /// argument ... a connection rejects." A connection has no value
    /// while the graph is built. TIME-2: "Composition describes a typed
    /// graph without reading live payloads."
    pub(super) fn constants_of(&mut self, kind: &ExprKind) {
        check_const_arguments(self.program, &self.locals, kind, self.diagnostics);
    }

    /// An entry of the configuration of the symbol written `form` in a
    /// graph, `value` given to the option `option` of the value
    /// candidate selected, which the graph lifts (FN-6).
    ///
    /// EXPR-14: "Scalar operator configuration binds during graph
    /// construction in temporal composition ... Configuration must not
    /// silently introduce a temporal input: time-varying temporal
    /// configuration requires a signature explicitly admitting that
    /// input." A value candidate's parameter is a scalar one: a
    /// connection there would be an input the configuration brought
    /// in. A temporal candidate's parameter admits one, and a call by
    /// name, `eq_(a, b, abs: tolerance)`, gives it explicitly (FN-6).
    /// TIME-2: "Composition describes a typed graph without reading
    /// live payloads." So the entry is given what is known where the
    /// graph is built (`connection.as_constant`).
    pub(super) fn configuration_input(&mut self, operator: &str, option: &str, value: &Expr) {
        if self.category(value) != ValueCategory::Connection {
            return;
        }
        self.diagnostics.emit(
            error(
                Category::Phase,
                codes::CONNECTION_AS_CONSTANT,
                value.at,
                format!(
                    "`{option}` configures `{operator}`, which binds it where the graph is built, \
                     and this is a connection"
                ),
                "EXPR-14",
            )
            .with_note("a constant or a `const` parameter is known where the graph is built"),
        );
    }

    /// True when `value` is a connection of a graph's body: what a
    /// temporal input or a graph's result is given in composition.
    pub(super) fn composes(&self, value: &Expr) -> bool {
        self.graph
            && !self.testing
            && self.hook.is_none()
            && self.category(value) == ValueCategory::Connection
    }
}

/// The same CTRL-14 comparison, once the categories of both sides are known.
pub(super) fn assignment_category(
    binding: &Local,
    (given, at): (ValueCategory, Location),
    inside: bool,
    chosen_at_runtime: bool,
) -> Option<Diagnostic> {
    use ValueCategory::{Connection, Value};
    let name = &binding.name;
    let message = match (binding.category, given) {
        (Value, Connection) => format!(
            "`{name}` holds an ordinary value, and this is a connection: a `var` keeps \
             the category of its first value"
        ),
        (Connection, Value) if !inside => format!(
            "`{name}` is a connection, and this is an ordinary value: a `var` keeps the \
             category of its first value"
        ),
        (Value, Value) if chosen_at_runtime => format!(
            "`{name}` holds an ordinary value, and this branch is selected at runtime: \
             its value would not be known while the graph is built"
        ),
        _ => return None,
    };
    Some(
        error(
            Category::Type,
            codes::BINDING_CATEGORY,
            at,
            message,
            "CTRL-14",
        )
        .with_related(binding.at, "declared here"),
    )
}

/// FN-3: checks the const inputs of a call using its current graph categories.
fn check_const_arguments(
    program: &Program,
    locals: &[Local],
    kind: &ExprKind,
    diagnostics: &mut Diagnostics,
) {
    let ExprKind::Call {
        callee, arguments, ..
    } = kind
    else {
        return;
    };
    let (module, declaration) = match callee {
        Callee::Function {
            module,
            declaration,
            ..
        }
        | Callee::Candidate {
            module,
            declaration,
            ..
        } => (*module, *declaration),
        // An operation the body requires: the operator's own
        // signature says which of its parameters is `const`
        // (GEN-4).
        Callee::Required {
            operator: (module, declaration),
            ..
        } => (*module, *declaration),
        Callee::Parameter { .. } | Callee::Anonymous(_) | Callee::Unread => return,
    };
    let Some(Detail::Callable(signature)) = program.modules[module].details.get(declaration) else {
        return;
    };
    for argument in arguments {
        let parameter = argument
            .parameter
            .and_then(|index| signature.parameters.get(index));
        let Some(parameter) = parameter else {
            continue;
        };
        // What is not settled is not judged: it may be a value.
        if parameter.constant
            && category(program, locals, &argument.value) == ValueCategory::Connection
        {
            diagnostics.emit(error(
                Category::Phase,
                codes::CONNECTION_AS_CONSTANT,
                argument.value.at,
                format!(
                    "`{}` is a `const` parameter, and this is a connection: it has no \
                     value while the graph is built",
                    parameter.name
                ),
                "FN-3",
            ));
        }
    }
}

/// GEN-7: a selected temporal candidate can make a formerly unsettled
/// argument or a loop condition a connection. Check the graph's
/// constant places after recording the instance.
pub(super) fn check_instance_constants(
    program: &Program,
    body: &Body,
    from: &[Location],
    diagnostics: &mut Diagnostics,
) {
    let mut reports = Diagnostics::new();
    walk_statement_nodes(&body.statements, &mut |node| match node {
        Node::Expression(value) => {
            check_const_arguments(program, &body.locals, &value.kind, &mut reports);
        }
        Node::Statement(Stmt::While {
            condition: Some(condition),
            ..
        }) => {
            if let Some(report) = while_connection(condition, condition.category) {
                reports.emit(report);
            }
        }
        _ => {}
    });
    report_instance_checks(
        &reports,
        from,
        diagnostics,
        "this place in the generic body needs a value",
    );
}

pub(super) fn report_instance_checks(
    reports: &Diagnostics,
    from: &[Location],
    diagnostics: &mut Diagnostics,
    message: &str,
) {
    for report in reports.as_slice() {
        if diagnostics.as_slice().contains(report) {
            continue;
        }
        for &root in from {
            let mut here = report.clone().with_related(report.primary, message);
            here.primary = root;
            if !diagnostics.as_slice().contains(&here) {
                diagnostics.emit(here);
            }
        }
    }
}

/// True when an expression gives a value where it stands. A block gives
/// the one it ends in (CTRL-10), and none when it ends in a `return` or
/// any other statement; an `if` gives one where a branch does.
fn gives_a_value(value: &Expr) -> bool {
    match value.kind.branch_values() {
        Some(values) => values.into_iter().any(gives_a_value),
        None => true,
    }
}

/// What `value` is (FN-3), its locals being `locals`: "There a temporal
/// parameter, and the result of a temporal or lifted call, is a
/// connection: the argument of a call, the operand of a symbol, a
/// condition, a bound local, or what is returned." So a connection is a
/// temporal parameter, a local given one, the result of a temporal
/// function or candidate, and what a call given one gives, which is
/// lifted (FN-6); a symbol is the call of its identity, `&&`, `||` and
/// `!` among them (the generics chapter's table), and a construction,
/// a result form, an element or a field of a connection is one too.
/// Any other is an ordinary value, known while the graph is built, but
/// for what an operation the graph requires gives (GEN-7): which
/// candidate that is is selected where the generic parameters are
/// known, and in composition it may be a temporal one (GEN-5), so with
/// no connection among its arguments what it gives is not settled here.
///
/// This is the one place the rule is written: the typing pass asks it
/// as it reads ([`Typer::category`]), and [`record`] writes its answer
/// on each expression for the stages after the analyser.
pub(crate) fn category(program: &Program, locals: &[Local], value: &Expr) -> ValueCategory {
    use ValueCategory::{Connection, Unsettled, Value};
    let category = |value: &Expr| category(program, locals, value);
    // Several values taken together: a connection where one is, and
    // not settled where one is not and none is a connection.
    let join = |one: ValueCategory, other: ValueCategory| match (one, other) {
        (Connection, _) | (_, Connection) => Connection,
        (Unsettled, _) | (_, Unsettled) => Unsettled,
        _ => Value,
    };
    let all = |values: &mut dyn Iterator<Item = &Expr>| values.map(category).fold(Value, join);
    match &value.kind {
        // A default and a constant are read with no local in scope
        // (UNIT-7, UNIT-13).
        ExprKind::Local(local) => locals.get(*local).map_or(Value, |local| local.category),
        ExprKind::StaticCallable(callee) => match &**callee {
            Callee::Anonymous(function) => function
                .captures
                .iter()
                .map(|&local| locals[local].category)
                .fold(Value, join),
            _ => Value,
        },
        ExprKind::Call {
            callee, arguments, ..
        } => {
            let given = all(&mut arguments.iter().map(|argument| &argument.value));
            match callee {
                Callee::Function {
                    module,
                    declaration,
                    ..
                } if matches!(
                    program.modules[*module].declarations[*declaration].kind,
                    DeclarationKind::Function(Role::Temporal)
                        | DeclarationKind::NativeFunction(Role::Temporal)
                ) =>
                {
                    Connection
                }
                Callee::Candidate {
                    module,
                    declaration,
                    ..
                } if matches!(
                    program.modules[*module].declarations[*declaration]
                        .kind
                        .candidate(),
                    Some((Role::Temporal, _))
                ) =>
                {
                    Connection
                }
                Callee::Anonymous(function) if !function.template.value => Connection,
                Callee::Anonymous(function) => function
                    .captures
                    .iter()
                    .map(|&local| locals[local].category)
                    .fold(given, join),
                Callee::Parameter { temporal: true, .. } => Connection,
                // A callback may capture a connection. A call awaiting
                // instance selection may choose a temporal candidate
                // (GEN-7), even with only ordinary arguments.
                Callee::Parameter { .. } | Callee::Required { .. } | Callee::Unread => {
                    join(given, Unsettled)
                }
                _ => given,
            }
        }
        ExprKind::Construct { arguments, .. } | ExprKind::Delta { arguments } => {
            all(&mut arguments.iter().map(|argument| &argument.value))
        }
        ExprKind::Unary { operand, .. } => category(operand),
        ExprKind::Binary { left, right, .. } => join(category(left), category(right)),
        ExprKind::Widen(inner) => category(inner),
        // OPS-6: the list converted is what it is given, lifted
        // over a connection as a conversion is (FN-6).
        ExprKind::ParentList(inner) => category(inner),
        ExprKind::Field { base, .. } => category(base),
        // An enum constructed from a connection is lifted (FN-6).
        ExprKind::EnumFrom { value, .. } => category(value),
        // A result form over a connection is lifted, as a value
        // function's call is (FN-6, ERR-3): what it gives is what it
        // is given.
        ExprKind::Tagged { payload, .. } => category(payload),
        ExprKind::TagTest { value, .. } => category(value),
        ExprKind::Payload { result, .. } => category(result),
        ExprKind::ErrorPart { base, .. } => category(base),
        // ERR-15, ERR-12: a result's form and a `T?` read are what the
        // value they take is: a connection of the form where it is one.
        ExprKind::Raised { result: inner, .. }
        | ExprKind::OrNull { result: inner, .. }
        | ExprKind::NullRead { value: inner, .. } => category(inner),
        // ERR-7: a capture is a temporal boundary, and what is read
        // of it a connection.
        ExprKind::Capture(_) | ExprKind::Captured { .. } => Connection,
        ExprKind::Error { message, code } => code
            .iter()
            .fold(category(message), |given, code| join(given, category(code))),
        ExprKind::Index { base, index } => join(category(base), category(index)),
        ExprKind::Tuple(entries) | ExprKind::Sequence(entries) => all(&mut entries.iter()),
        ExprKind::Pairs(entries) => all(&mut entries.iter().flat_map(|(key, value)| [key, value])),
        ExprKind::Block {
            value: Some(value), ..
        } => category(value),
        // CTRL-13: "It is a connection when its condition is one or
        // either branch's value is". CTRL-10: "A branch that returns
        // from the function gives the `if` no value and takes no
        // part in its type (CTRL-5)", and so none in what it is. An
        // `if` no branch of which gives a value has none itself.
        ExprKind::If { condition, .. } => {
            let mut branches = value
                .kind
                .branch_values()
                .into_iter()
                .flatten()
                .filter(|branch| gives_a_value(branch))
                .map(category);
            match branches.next() {
                Some(first) => join(category(condition), branches.fold(first, join)),
                None => Value,
            }
        }
        _ => Value,
    }
}

/// True where `callee`, given a connection in composition, is lifted
/// (FN-6): "A selected value helper, a `const fn` or a value candidate
/// of an operator", a native one among them, that marks no parameter
/// `var` (FN-15: "A function or a candidate that declares one is not
/// lifted"). An operation the declaration requires is not known here
/// (GEN-7), and a temporal function or candidate is the node or the
/// graph itself.
fn lifts(program: &Program, callee: &Callee) -> bool {
    if let Callee::Anonymous(function) = callee {
        return function.template.value;
    }
    let Some((module, declaration)) = callee.declaration() else {
        return false;
    };
    let declared = &program.modules[module];
    let places = match declared.details.get(declaration) {
        Some(Detail::Callable(signature)) => {
            signature.parameters.iter().any(|parameter| parameter.var)
        }
        _ => false,
    };
    declared.declarations[declaration].kind.role() == Some(Role::Value) && !places
}

/// Records on every expression of `body` what it is ([`Expr::category`])
/// and on every call whether it is lifted (the `lifted` of
/// [`ExprKind::Call`]), as [`category`] says with the body's locals.
/// `composition` is true for a graph's body, where FN-6 lifts: "A
/// selected value helper ... with temporal arguments in composition is
/// lifted". A node's hooks, a value function and a test lift nothing
/// (a test's eval wires its target itself, TST-8).
///
/// A local that is not settled waits for its first value (EXPR-3), or
/// for the sequence of the `for` that binds it (CTRL-15). In an
/// instance ([`Program::instance`]), each operation the declaration
/// requires is the call of the candidate that answers it, a temporal
/// one or a value one, so what it gives is known. A value may read a
/// local declared inside it, `let s = { let t = add_(k, k); t }`, which
/// a walk meets after the declaration it is in, so the walk is made
/// again until it settles no more: each walk but the last settles one
/// category or fills a hole in an inferred type. In a graph this
/// includes a no-else `if` whose type depends on the categories (CTRL-13).
pub(crate) fn record(program: &Program, body: &mut Body, composition: bool) {
    record_with(program, body, composition, |_, _| Type::Unknown);
}

/// Settle a concrete graph with the source branch-typing check available.
pub(super) fn record_with(
    program: &Program,
    body: &mut Body,
    composition: bool,
    mut join: impl FnMut(&mut Expr, &mut Expr) -> Type,
) {
    use ValueCategory::{Connection, Unsettled};
    let Body { locals, statements } = body;
    let first_values = first_values(locals, statements);
    let mut settling = true;
    while settling {
        settling = false;
        walk_statement_nodes_mut(statements, &mut |node| {
            let (local, value, at, declares) = match node {
                NodeMut::Statement(Stmt::Declare {
                    local,
                    value: Some(value),
                }) => (*local, value, locals[*local].at, true),
                NodeMut::Statement(Stmt::Assign {
                    local, value, at, ..
                }) => (*local, value, *at, false),
                NodeMut::Statement(Stmt::For {
                    names, sequence, ..
                }) if composition => {
                    if names
                        .iter()
                        .all(|&local| locals[local].category != Unsettled)
                    {
                        return;
                    }
                    let over = category(program, locals, sequence);
                    // A connection still needs its collection kind to
                    // decide whether each iteration binding is one.
                    if over == Unsettled || over == Connection && open(&sequence.ty) {
                        return;
                    }
                    let (categories, _) = bound_by_loop(over, &sequence.ty, names.len());
                    for (&local, given) in names.iter().zip(categories) {
                        if locals[local].category == Unsettled && given != Unsettled {
                            locals[local].category = given;
                            settling = true;
                        }
                    }
                    return;
                }
                NodeMut::Expression(value) if composition && !whole(&value.ty) => {
                    if let Some(ty) = settled_type(program, locals, value, &mut join) {
                        settling |= fill_type(&mut value.ty, ty);
                    }
                    return;
                }
                _ => return,
            };
            if first_values[local] == Some(at) && locals[local].category == Unsettled {
                let given = category(program, locals, value);
                if given != Unsettled {
                    locals[local].category = given;
                    settling = true;
                }
            }
            if composition && declares && !whole(&locals[local].ty) {
                settling |= fill_type(&mut locals[local].ty, value.ty.clone());
            }
        });
    }
    walk_statements_mut(statements, &mut |value| {
        recorded(program, locals, composition, value);
    });
}

/// EXPR-3: the initializer takes precedence, or the first assignment
/// of a `var` with no initializer. A write inside an assignment's
/// right-hand side completes before the enclosing write.
fn first_values(locals: &[Local], statements: &[Stmt]) -> Vec<Option<Location>> {
    let mut first: Vec<Option<Location>> = vec![None; locals.len()];
    walk_statement_nodes(statements, &mut |node| match node {
        Node::Statement(Stmt::Declare {
            local,
            value: Some(_),
        }) => {
            first[*local] = Some(locals[*local].at);
        }
        Node::Statement(Stmt::Assign { local, at, .. })
            if first[*local].is_none_or(|before| at.span.end < before.span.end) =>
        {
            first[*local] = Some(*at);
        }
        _ => {}
    });
    first
}

/// EXPR-3 and EXPR-9: each pass fills holes in a type, preserving all
/// previously known positions, including members typed by an annotation.
fn fill_type(current: &mut Type, inferred: Type) -> bool {
    match (current, inferred) {
        (current @ Type::Unknown, inferred) if inferred != Type::Unknown => {
            *current = inferred;
            true
        }
        (Type::Nullable(current), Type::Nullable(inferred)) => fill_type(current, *inferred),
        (Type::Tuple(current), Type::Tuple(inferred)) if current.len() == inferred.len() => {
            let mut changed = false;
            for (current, inferred) in current.iter_mut().zip(inferred) {
                changed |= fill_type(current, inferred);
            }
            changed
        }
        _ => false,
    }
}

/// CTRL-13: decide a no-else expression's type once its categories
/// settle. A block tail or a read of an inferred local also takes its
/// type from the value once that type is known.
fn settled_type(
    program: &Program,
    locals: &[Local],
    value: &mut Expr,
    join: &mut dyn FnMut(&mut Expr, &mut Expr) -> Type,
) -> Option<Type> {
    let ty = match &mut value.kind {
        ExprKind::If {
            then,
            otherwise: Some(otherwise),
            join_branches: true,
            ..
        } if whole(&then.ty) && whole(&otherwise.ty) => join(then, otherwise),
        ExprKind::If {
            condition,
            then,
            otherwise: None,
            ..
        } if gives_a_value(then) => no_else_type(
            then.ty.clone(),
            category(program, locals, condition),
            category(program, locals, then),
        ),
        ExprKind::Block {
            value: Some(tail), ..
        } => tail.ty.clone(),
        // Tuple entries each have their own position and type (EXPR-9).
        ExprKind::Tuple(entries) => {
            Type::Tuple(entries.iter().map(|entry| entry.ty.clone()).collect())
        }
        ExprKind::Local(local) => locals.get(*local)?.ty.clone(),
        _ => return None,
    };
    (ty != Type::Unknown).then_some(ty)
}

/// Records what `value` is and, for a call, whether it is lifted, as
/// [`record`] does for one expression of a body with `locals`.
fn recorded(program: &Program, locals: &[Local], composition: bool, value: &mut Expr) {
    if let ExprKind::Call {
        callee, arguments, ..
    } = &mut value.kind
        && let Some((module, declaration)) = callee.declaration()
        && let Some(Detail::Callable(signature)) = program.modules[module].details.get(declaration)
    {
        function_capture_arguments(locals, arguments, signature.parameters.len());
    }
    value.category = category(program, locals, value);
    if composition {
        formed_in_composition(program, locals, value);
    }
    if let ExprKind::Call { callee, lifted, .. } = &mut value.kind {
        *lifted =
            composition && lifts(program, callee) && value.category == ValueCategory::Connection;
    }
}

/// A static anonymous argument contributes runtime parameters only for
/// its captures. Reserve their slots in declaration order and pass their
/// values alongside the static argument in source order.
fn function_capture_arguments(
    locals: &[Local],
    arguments: &mut Vec<crate::hir::Argument>,
    original: usize,
) {
    let captures = |argument: &crate::hir::Argument| -> Option<(usize, Vec<LocalId>)> {
        let ExprKind::StaticCallable(callee) = &argument.value.kind else {
            return None;
        };
        let Callee::Anonymous(function) = &**callee else {
            return None;
        };
        Some((argument.parameter?, function.captures.clone()))
    };
    let mut captured: Vec<_> = arguments.iter().filter_map(captures).collect();
    if captured.iter().all(|(_, captures)| captures.is_empty()) {
        return;
    }
    captured.sort_by_key(|(parameter, _)| *parameter);
    let mut next = original;
    let mut ranges = vec![None; original];
    for (parameter, captures) in captured {
        ranges[parameter] = Some(next);
        next += captures.len();
    }
    // An instance may already carry captures from its uninstantiated
    // body. Rebuild them from the now concrete arguments.
    let written = std::mem::take(arguments);
    for argument in written {
        if argument
            .parameter
            .is_some_and(|parameter| parameter >= original)
        {
            continue;
        }
        let holds = captures(&argument);
        arguments.push(argument);
        let Some((parameter, captured)) = holds else {
            continue;
        };
        let start = ranges[parameter].expect("capture slots were reserved");
        for (offset, local) in captured.into_iter().enumerate() {
            let binding = &locals[local];
            arguments.push(crate::hir::Argument {
                parameter: Some(start + offset),
                pack: None,
                value: Expr {
                    kind: ExprKind::Local(local),
                    ty: binding.ty.clone(),
                    at: binding.at,
                    category: binding.category,
                },
            });
        }
    }
}

/// Where the form a place in a graph's body takes of `value` is made
/// (ERR-15, ERR-12), now that what `value` holds is known to be a
/// connection or an ordinary value. ERR-15: "In composition a call that
/// gives a connection, of a temporal function or one that is lifted
/// (FN-6), takes its form as the connection's: ... of T, where a success
/// publishes its value and a failure fails the call's node in that cycle
/// ... A call that is evaluated where the graph is built and gives no
/// connection (FN-6, FN-9) takes its form as value code does. A
/// connection of `result<T, error>` that a binding holds, given where a
/// connection of T or of `T?` is taken, takes that form as a call's
/// does, in the node that reads it." ERR-12: "A connection of `T?` given
/// where a connection of T is taken ... is read so too, the node that
/// reads it failing in a cycle that publishes a null."
fn formed_in_composition(program: &Program, locals: &[Local], value: &mut Expr) {
    use crate::hir::Raise;
    let connection = |inner: &Expr| category(program, locals, inner) == ValueCategory::Connection;
    let of_call = |inner: &Expr| matches!(inner.kind, ExprKind::Call { .. });
    match &mut value.kind {
        ExprKind::Raised { result, raise } => {
            *raise = match (connection(result), of_call(result)) {
                (true, true) => Raise::CallNode,
                (true, false) => Raise::Reader,
                (false, _) => Raise::Build,
            };
        }
        ExprKind::OrNull { result, reader } => {
            *reader = connection(result) && !of_call(result);
        }
        ExprKind::NullRead { value, raise, .. } => {
            *raise = if connection(value) {
                Raise::Reader
            } else {
                Raise::Build
            };
        }
        _ => {}
    }
}

/// [`record`] for a module constant's expression (UNIT-13). It has
/// no enclosing body and does not compose a graph.
pub(crate) fn record_value(program: &Program, value: &mut Expr) {
    value.walk_mut(&mut |inner| recorded(program, &[], false, inner));
}

/// An application's entry arguments are value code with `run_config`
/// in scope when injected (UNIT-14, RUN-6).
pub(super) fn record_run(program: &Program, run: &mut Run) {
    run.call
        .walk_mut(&mut |inner| recorded(program, &run.locals, false, inner));
}
