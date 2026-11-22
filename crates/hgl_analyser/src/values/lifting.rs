//! What a value is in a graph: a connection, or an ordinary value known
//! while the graph is built, and the places that take no connection
//! (FN-3, FN-6, CTRL-14, EXPR-14). Part of the typing pass ([`super`]).

use std::collections::HashMap;

use hgl_diagnostics::{Category, Diagnostic, Diagnostics, Location};
use hgl_parser::{SyntaxKind, SyntaxNode};

use super::calls::pending_function_inference;
use super::control::{bound_by_loop, iterates, iteration_types, no_else_type, while_connection};
use super::expressions::sequence_element;
use super::fit::{open, whole};
use super::forms::received_type;
use super::members::{index_result, indexed_payload};
use super::results::{captured_binding, captured_output};
use super::{CapturedOutput, Typer};
use crate::hir::{
    Body, Callee, Expr, ExprKind, Form, IfInference, Local, LocalId, Node, NodeMut, Run, Stmt,
    Value, ValueCategory, walk_statement_nodes, walk_statement_nodes_mut, walk_statements_mut,
};
use crate::program::{DeclarationKind, OperatorRef, Program, Role};
use crate::types::{Argument, Detail, Parameter, Type};
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
    pub(super) fn configuration_input(
        &mut self,
        (operator, form): (OperatorRef, Form),
        (module, declaration): (usize, usize),
        parameter: &Parameter,
        value: &Expr,
        category: ValueCategory,
    ) {
        if !self.graph
            || parameter.constant
            || category != ValueCategory::Connection
            || !matches!(
                self.program.modules[module].declarations[declaration]
                    .kind
                    .candidate(),
                Some((Role::Value, _))
            )
        {
            return;
        }
        let operator = match form {
            Form::Named => self.program.modules[operator.module].declarations[operator.declaration]
                .name
                .as_str(),
            Form::Binary(operator) => operator.symbol(),
            Form::Unary(operator) => operator.symbol(),
        };
        let option = &parameter.name;
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
        Callee::Parameter { .. }
        | Callee::Anonymous(_)
        | Callee::DeferredOperator(_)
        | Callee::DeferredValue
        | Callee::Unread => {
            return;
        }
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
        ExprKind::Local(local)
        | ExprKind::DeferredPresent(local)
        | ExprKind::PackEntry { pack: local, .. } => {
            locals.get(*local).map_or(Value, |local| local.category)
        }
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
                Callee::Parameter { .. }
                | Callee::Required { .. }
                | Callee::DeferredOperator(_)
                | Callee::Unread => join(given, Unsettled),
                _ => given,
            }
        }
        ExprKind::Construct { arguments, .. } | ExprKind::Delta { arguments } => {
            all(&mut arguments.iter().map(|argument| &argument.value))
        }
        ExprKind::Unary { operand, .. } => category(operand),
        ExprKind::DeferredNegation {
            configuration,
            operand,
            ..
        } => {
            let given = all(&mut configuration
                .iter()
                .filter_map(|entry| entry.value.as_ref()));
            join(join(given, category(operand)), Unsettled)
        }
        ExprKind::Binary { left, right, .. } => join(category(left), category(right)),
        ExprKind::DeferredBinary {
            left,
            configuration,
            right,
            ..
        } => {
            let given = all(&mut configuration
                .iter()
                .filter_map(|entry| entry.value.as_ref()));
            join(
                join(join(category(left), given), category(right)),
                Unsettled,
            )
        }
        ExprKind::Widen(inner) => category(inner),
        // OPS-6: the list converted is what it is given, lifted
        // over a connection as a conversion is (FN-6).
        ExprKind::ParentList(inner) => category(inner),
        ExprKind::Field { base, .. } | ExprKind::DeferredMember { base, .. } => category(base),
        // An enum constructed from a connection is lifted (FN-6).
        ExprKind::EnumFrom { value, .. } | ExprKind::DeferredEnumFrom(value) => category(value),
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
        | ExprKind::NullRead { value: inner, .. }
        | ExprKind::PresentChild(inner) => category(inner),
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
    record_with(program, body, composition, None, None);
}

/// Settle a concrete graph with source branch and field inference available.
pub(super) fn record_with(
    program: &Program,
    body: &mut Body,
    composition: bool,
    mut typer: Option<&mut Typer<'_>>,
    place_required: Option<&dyn Fn(&mut Expr)>,
) {
    use ValueCategory::{Connection, Unsettled};
    let Body { locals, statements } = body;
    let mut first = first_values(locals, statements);
    let mut capture_types = HashMap::new();
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
                    let ty = received_type(&sequence.ty, iterates);
                    if let Some(types) = iteration_types(ty, names.len()) {
                        for (&local, given) in names.iter().zip(types) {
                            if !whole(&locals[local].ty) {
                                settling |= fill_type(&mut locals[local].ty, given.clone());
                            }
                        }
                    }
                    if names
                        .iter()
                        .all(|&local| locals[local].category != Unsettled)
                    {
                        return;
                    }
                    let over = category(program, locals, sequence);
                    // A connection still needs its collection kind to
                    // decide whether each iteration binding is one.
                    if over == Unsettled || over == Connection && open(ty) {
                        return;
                    }
                    let (categories, _) = bound_by_loop(over, ty, names.len());
                    for (&local, given) in names.iter().zip(categories) {
                        if locals[local].category == Unsettled && given != Unsettled {
                            locals[local].category = given;
                            settling = true;
                        }
                    }
                    return;
                }
                NodeMut::Expression(value)
                    if composition
                        && matches!(
                            value.kind,
                            ExprKind::DeferredBinary { .. } | ExprKind::DeferredNegation { .. }
                        ) =>
                {
                    if let Some(typer) = typer.as_deref_mut() {
                        // The normal call checks read the concrete local
                        // scope, including iteration bindings settled here.
                        std::mem::swap(&mut typer.locals, locals);
                        let selected = typer.settle_symbol(&value.kind);
                        std::mem::swap(&mut typer.locals, locals);
                        if let Some((kind, ty)) = selected {
                            value.kind = kind;
                            value.ty = ty;
                            settling = true;
                        }
                    }
                    return;
                }
                NodeMut::Expression(Expr { kind, ty, .. })
                    if composition && matches!(kind, ExprKind::DeferredEnumFrom(_)) =>
                {
                    if let ExprKind::DeferredEnumFrom(value) = kind
                        && let Some(typer) = typer.as_deref_mut()
                        && let Some(constructor) = typer.settle_enum_from(ty, value)
                    {
                        *kind = constructor;
                        settling = true;
                    }
                    return;
                }
                NodeMut::Expression(value)
                    if composition && pending_function_inference(&value.kind) =>
                {
                    if let Some(typer) = typer.as_deref_mut() {
                        // Inference and admission use the concrete locals
                        // of the graph, as ordinary call checking does.
                        std::mem::swap(&mut typer.locals, locals);
                        settling |= typer.settle_function_call(value);
                        std::mem::swap(&mut typer.locals, locals);
                    }
                    return;
                }
                NodeMut::Expression(value)
                    if composition
                        && matches!(
                            value.kind,
                            ExprKind::Call {
                                callee: Callee::DeferredOperator(_),
                                ..
                            }
                        ) =>
                {
                    if let Some(typer) = typer.as_deref_mut() {
                        std::mem::swap(&mut typer.locals, locals);
                        let selected = typer.settle_operator(value);
                        std::mem::swap(&mut typer.locals, locals);
                        if let Some((kind, ty)) = selected {
                            value.kind = kind;
                            value.ty = ty;
                            // GEN-16: a requirement found on replay uses
                            // the evidence supplied to the enclosing instance.
                            if let Some(place) = place_required
                                && matches!(
                                    value.kind,
                                    ExprKind::Call {
                                        callee: Callee::Required { .. },
                                        ..
                                    }
                                )
                            {
                                place(value);
                            }
                            settling = true;
                        }
                    }
                    return;
                }
                NodeMut::Expression(value) if composition && !whole(&value.ty) => {
                    if let ExprKind::DeferredPresent(local) = value.kind {
                        let binding = &locals[local];
                        if binding.category != Unsettled && whole(&binding.ty) {
                            value.ty = match (&binding.ty, binding.category) {
                                (Type::Nullable(payload), ValueCategory::Value) => {
                                    (**payload).clone()
                                }
                                (ty, _) => ty.clone(),
                            };
                            value.kind = ExprKind::Local(local);
                            settling = true;
                        }
                        return;
                    }
                    if let ExprKind::DeferredMember {
                        base,
                        name,
                        at,
                        variant_proven,
                    } = &value.kind
                        && let Some(typer) = typer.as_deref_mut()
                    {
                        std::mem::swap(&mut typer.locals, locals);
                        let member = typer.settle_member(base, name, *at, *variant_proven);
                        std::mem::swap(&mut typer.locals, locals);
                        if let Some((kind, ty)) = member {
                            value.kind = kind;
                            value.ty = ty;
                            settling = true;
                        }
                        return;
                    }
                    if let Some(ty) =
                        settled_type(program, locals, &capture_types, value, typer.as_deref_mut())
                    {
                        settling |= fill_type(&mut value.ty, ty);
                    }
                    return;
                }
                _ => return,
            };
            if first.get(local).copied().flatten() == Some(at)
                && locals[local].category == Unsettled
            {
                let given = category(program, locals, value);
                if given != Unsettled {
                    locals[local].category = given;
                    settling = true;
                }
            }
            if composition && declares && !whole(&locals[local].ty) {
                settling |= fill_type(&mut locals[local].ty, value.ty.clone());
            }
            // ERR-7: the boundary stays separate from its output. Keep
            // the output only for the definite capture bindings source recognizes.
            if composition
                && declares
                && let Some(call) = captured_binding(locals[local].kind, value)
                && let CapturedOutput::Of(ty) = captured_output(program, call)
                && whole(&ty)
                && capture_types.get(&local) != Some(&ty)
            {
                capture_types.insert(local, ty);
                settling = true;
            }
        });
        // A contextual symbol option can introduce locals in its block
        // when its candidate is selected. Include them in the next pass.
        if first.len() != locals.len() {
            first = first_values(locals, statements);
        }
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
pub(super) fn fill_type(current: &mut Type, inferred: Type) -> bool {
    match (current, inferred) {
        (current @ Type::Unknown, inferred) if inferred != Type::Unknown => {
            *current = inferred;
            true
        }
        (Type::Nullable(current), Type::Nullable(inferred))
        | (Type::Set(current), Type::Set(inferred))
        | (Type::Atomic(current), Type::Atomic(inferred))
        | (Type::Delta(current), Type::Delta(inferred)) => fill_type(current, *inferred),
        // TYPE-9: a payload that settles to a whole publication can
        // lose its provisional atomic wrapper.
        (current @ Type::Atomic(_), inferred) => {
            let Type::Atomic(payload) = current else {
                unreachable!();
            };
            if !fill_type(payload, inferred) {
                return false;
            }
            *current = Type::atomic((**payload).clone());
            true
        }
        (
            Type::Result { value, error },
            Type::Result {
                value: inferred_value,
                error: inferred_error,
            },
        ) => {
            let value_changed = fill_type(value, *inferred_value);
            let error_changed = fill_type(error, *inferred_error);
            value_changed || error_changed
        }
        (
            Type::Dict { key, value },
            Type::Dict {
                key: inferred_key,
                value: inferred_value,
            },
        ) => {
            let key_changed = fill_type(key, *inferred_key);
            let value_changed = fill_type(value, *inferred_value);
            key_changed || value_changed
        }
        (
            Type::List {
                element: current,
                size,
            },
            Type::List {
                element: inferred,
                size: inferred_size,
            },
        ) if *size == inferred_size => fill_type(current, *inferred),
        (Type::Tuple(current), Type::Tuple(inferred)) if current.len() == inferred.len() => {
            let mut changed = false;
            for (current, inferred) in current.iter_mut().zip(inferred) {
                changed |= fill_type(current, inferred);
            }
            changed
        }
        (
            Type::Named {
                module,
                declaration,
                arguments,
                ..
            },
            Type::Named {
                module: new_module,
                declaration: new_declaration,
                arguments: inferred,
                ..
            },
        ) if (*module, *declaration) == (new_module, new_declaration)
            && arguments.len() == inferred.len() =>
        {
            let mut changed = false;
            for (current, inferred) in arguments.iter_mut().zip(inferred) {
                if let (Argument::Type(current), Argument::Type(inferred)) = (current, inferred) {
                    changed |= fill_type(current, inferred);
                }
            }
            changed
        }
        _ => false,
    }
}

/// True when a contextual operand's children have more type information
/// than its stored type. Settlement visits the outer call before these
/// children, so its replay must wait for the next type update.
pub(super) fn contextual_type_is_pending(typer: &Typer<'_>, value: &Expr) -> bool {
    if whole(&value.ty) {
        return false;
    }
    let Some(inferred) = contextual_type(value, Some(typer)) else {
        return false;
    };
    fill_type(&mut value.ty.clone(), inferred)
}

fn contextual_type(value: &Expr, typer: Option<&Typer<'_>>) -> Option<Type> {
    let inferred = match &value.kind {
        ExprKind::Block {
            value: Some(tail), ..
        } => tail.ty.clone(),
        // Tuple entries each have their own position and type (EXPR-9).
        ExprKind::Tuple(entries) => {
            Type::Tuple(entries.iter().map(|entry| entry.ty.clone()).collect())
        }
        ExprKind::Sequence(entries) => {
            let Type::List { element, size } = &value.ty else {
                return None;
            };
            let typed: Vec<&Type> = entries
                .iter()
                .filter(|entry| {
                    let tail = block_tail(entry);
                    !matches!(tail.kind, ExprKind::Literal(Value::Null))
                        && !typer.is_some_and(|typer| contextual_member(typer, tail).is_some())
                })
                .map(|entry| &entry.ty)
                .collect();
            // Another unsettled entry could change even the first known
            // element's type (for instance by numeric widening).
            if !typed.iter().all(|ty| whole(ty)) {
                return None;
            }
            let inferred = sequence_element(&typed, matches!(**element, Type::Nullable(_)))?;
            Type::List {
                element: Box::new(inferred),
                size: size.clone(),
            }
        }
        _ => return None,
    };
    Some(inferred)
}

/// CTRL-10: nested blocks take their value and type from their tail.
pub(super) fn block_tail(mut entry: &Expr) -> &Expr {
    while let ExprKind::Block {
        value: Some(tail), ..
    } = &entry.kind
    {
        entry = tail;
    }
    entry
}

/// TYPE-7: an unread `.member` takes its type from the list. Other
/// unread expressions may still need to settle before the list has a type.
fn contextual_member(typer: &Typer<'_>, tail: &Expr) -> Option<SyntaxNode> {
    if !matches!(tail.kind, ExprKind::Unread) || whole(&tail.ty) {
        return None;
    }
    let node = contextual_tail_syntax(typer, tail)?;
    (node.kind() == SyntaxKind::ImpliedMember).then_some(node)
}

/// The expression written at a contextual tail's source location.
pub(super) fn contextual_tail_syntax(typer: &Typer<'_>, tail: &Expr) -> Option<SyntaxNode> {
    let node = typer.syntax_of(tail)?;
    // A block's tail and its expression statement share a span.
    if node.kind() == SyntaxKind::ExprStmt {
        node.children().next()
    } else {
        Some(node)
    }
}

/// Replace only the contextual tail, keeping any statements already
/// checked in the block's own scope.
pub(super) fn resolve_contextual_tail(entry: &mut Expr, resolved: (ExprKind, Type)) {
    if let ExprKind::Block {
        value: Some(tail), ..
    } = &mut entry.kind
    {
        resolve_contextual_tail(tail, resolved);
        entry.ty = tail.ty.clone();
    } else {
        (entry.kind, entry.ty) = resolved;
    }
}

/// CTRL-13: decide a no-else expression's type once its categories
/// settle. A block tail or a read of an inferred local also takes its
/// type from the value once that type is known.
fn settled_type(
    program: &Program,
    locals: &[Local],
    capture_types: &HashMap<LocalId, Type>,
    value: &mut Expr,
    typer: Option<&mut Typer<'_>>,
) -> Option<Type> {
    let ty = match &mut value.kind {
        ExprKind::If {
            then,
            otherwise: Some(otherwise),
            inference: IfInference::Both,
            ..
        } if whole(&then.ty) && whole(&otherwise.ty) => typer?.one_type(then, otherwise),
        ExprKind::If {
            then,
            inference: IfInference::Then,
            ..
        } => then.ty.clone(),
        ExprKind::If {
            otherwise: Some(otherwise),
            inference: IfInference::Otherwise,
            ..
        } => otherwise.ty.clone(),
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
        ExprKind::Sequence(_) => {
            let mut typer = typer;
            let inferred = contextual_type(value, typer.as_deref())?;
            if let (Type::List { element, .. }, ExprKind::Sequence(entries)) =
                (&inferred, &mut value.kind)
            {
                for entry in entries {
                    let tail = block_tail(entry);
                    if matches!(tail.kind, ExprKind::Literal(Value::Null))
                        && matches!(**element, Type::Nullable(_))
                    {
                        // EXPR-9 and TYPE-14: a null takes the list's
                        // inferred nullable element type through its blocks.
                        resolve_contextual_tail(
                            entry,
                            (ExprKind::Literal(Value::Null), (**element).clone()),
                        );
                    } else if let Some(typer) = typer.as_deref_mut()
                        && let Some(node) = contextual_member(typer, tail)
                    {
                        resolve_contextual_tail(entry, typer.implied_member(&node, Some(element)));
                    }
                }
            }
            inferred
        }
        ExprKind::Block { .. } | ExprKind::Tuple(_) => contextual_type(value, None)?,
        ExprKind::Local(local) => locals.get(*local)?.ty.clone(),
        // ERR-12: a read keeps the operand's payload type as it settles,
        // with its original null failure still recorded on the read.
        ExprKind::NullRead { value, .. } => match &value.ty {
            Type::Nullable(payload) => (**payload).clone(),
            _ => return None,
        },
        // ERR-7: capture already checked the field. The call's output
        // may have settled after that read, directly or through a `let`.
        ExprKind::Captured {
            capture,
            error: false,
        } => match &capture.kind {
            ExprKind::Local(local) | ExprKind::DeferredPresent(local) => {
                capture_types.get(local)?.clone()
            }
            ExprKind::Capture(call) => match captured_output(program, call) {
                CapturedOutput::Of(ty) => ty,
                CapturedOutput::None => return None,
            },
            _ => return None,
        },
        // The base may have acquired an element type after a conditional
        // inside a list or tuple settled. Keep the existing projection.
        ExprKind::Index { base, index } => {
            let ty = indexed_payload(&base.ty);
            if matches!(ty, Type::Tuple(_))
                && whole(&index.ty)
                && let Some(typer) = typer
            {
                typer.tuple_index(ty, index, None, false)
            } else {
                let position = match index.kind {
                    ExprKind::Literal(Value::Int(position)) => Some(position),
                    _ => None,
                };
                index_result(ty, position)?.clone()
            }
        }
        ExprKind::Construct {
            arguments,
            infer_types: true,
            ..
        } => typer?.infer_settled_fields(value.ty.clone(), arguments)?,
        // EXPR-7: the member's position was checked where it was read.
        // Recover its type using the settled base, keeping TYPE-9's
        // atomic normalization and TYPE-16's delta properties.
        ExprKind::Field { base, member } => match &base.ty {
            Type::Atomic(of) => Type::atomic(program.field_types(of)?.get(*member)?.clone()),
            Type::Delta(of) => program.delta_properties(of)?.into_iter().nth(*member)?.1,
            ty => program.field_types(ty)?.get(*member)?.clone(),
        },
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
    fn captures(
        argument: &crate::hir::Argument,
    ) -> Option<(usize, &crate::hir::AnonymousFunction)> {
        let ExprKind::StaticCallable(callee) = &argument.value.kind else {
            return None;
        };
        let Callee::Anonymous(function) = &**callee else {
            return None;
        };
        Some((argument.parameter?, function))
    }
    let mut captured: Vec<_> = arguments
        .iter()
        .filter_map(captures)
        .map(|(parameter, function)| (parameter, function.captures.len()))
        .collect();
    if captured.iter().all(|&(_, count)| count == 0) {
        return;
    }
    captured.sort_by_key(|(parameter, _)| *parameter);
    let mut next = original;
    let mut ranges = vec![None; original];
    for (parameter, count) in captured {
        ranges[parameter] = Some(next);
        next += count;
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
        let holds = captures(&argument).map(|(parameter, function)| {
            let start = ranges[parameter].expect("capture slots were reserved");
            function
                .captures
                .iter()
                .enumerate()
                .map(|(offset, &local)| {
                    let binding = &locals[local];
                    crate::hir::Argument {
                        parameter: Some(start + offset),
                        pack: None,
                        value: function.capture_value(offset, binding, binding.at),
                    }
                })
                .collect::<Vec<_>>()
        });
        arguments.push(argument);
        arguments.extend(holds.into_iter().flatten());
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
