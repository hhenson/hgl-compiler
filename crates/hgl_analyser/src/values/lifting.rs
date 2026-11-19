//! What a value is in a graph: a connection, or an ordinary value known
//! while the graph is built, and the places that take no connection
//! (FN-3, FN-6, CTRL-14, EXPR-14). Part of the typing pass ([`super`]).

use hgl_diagnostics::{Category, Location};

use super::Typer;
use crate::hir::{Callee, Expr, ExprKind, Form, LocalId, ValueCategory};
use crate::program::{DeclarationKind, Role};
use crate::types::Detail;
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
        use ValueCategory::{Connection, Unsettled, Value};
        if self.locals[local].category == Unsettled && !inside {
            self.locals[local].category = given;
        }
        let held = self.locals[local].category;
        // Declared before the nearest `if` selected at runtime.
        let chosen_at_runtime = self.temporal.is_some_and(|outside| local < outside);
        let name = self.locals[local].name.clone();
        let message = match (held, given) {
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
            _ => return,
        };
        self.diagnostics.emit(
            error(
                Category::Type,
                codes::BINDING_CATEGORY,
                at,
                message,
                "CTRL-14",
            )
            .with_related(self.locals[local].at, "declared here"),
        );
    }

    /// What a value read in a graph is (FN-3). A connection: a
    /// temporal parameter, a local given one, the result of a temporal
    /// function or candidate, or of a call given one, which is lifted
    /// (FN-6). Any other is an ordinary value, known while the graph is
    /// built, but for what an operation the graph requires gives
    /// (GEN-7): which candidate that is is selected where the generic
    /// parameters are known, and in composition it may be a temporal
    /// one (GEN-5), so with no connection among its arguments what it
    /// gives is not settled here.
    pub(super) fn category(&self, value: &Expr) -> ValueCategory {
        use ValueCategory::{Connection, Unsettled, Value};
        let program = self.program;
        // Several values taken together: a connection where one is, and
        // not settled where one is not and none is a connection.
        let join = |one: ValueCategory, other: ValueCategory| match (one, other) {
            (Connection, _) | (_, Connection) => Connection,
            (Unsettled, _) | (_, Unsettled) => Unsettled,
            _ => Value,
        };
        let all = |values: &mut dyn Iterator<Item = &Expr>| {
            values.map(|value| self.category(value)).fold(Value, join)
        };
        match &value.kind {
            ExprKind::Local(local) => self.locals[*local].category,
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
                    Callee::Required { .. } => join(given, Unsettled),
                    _ => given,
                }
            }
            ExprKind::Construct { arguments, .. } | ExprKind::Delta { arguments } => {
                all(&mut arguments.iter().map(|argument| &argument.value))
            }
            ExprKind::Unary { operand, .. } => self.category(operand),
            ExprKind::Binary { left, right, .. } => join(self.category(left), self.category(right)),
            ExprKind::Widen(inner) => self.category(inner),
            // OPS-6: the list converted is what it is given, lifted
            // over a connection as a conversion is (FN-6).
            ExprKind::ParentList(inner) => self.category(inner),
            ExprKind::Field { base, .. } => self.category(base),
            // An enum constructed from a connection is lifted (FN-6).
            ExprKind::EnumFrom { value, .. } => self.category(value),
            // A result form over a connection is lifted, as a value
            // function's call is (FN-6, ERR-3): what it gives is what it
            // is given.
            ExprKind::Tagged { payload, .. } => self.category(payload),
            ExprKind::TagTest { value, .. } => self.category(value),
            ExprKind::Payload { result, .. } => self.category(result),
            ExprKind::ErrorPart { base, .. } => self.category(base),
            ExprKind::Propagate(operand) => self.category(operand),
            // ERR-7: a capture is a temporal boundary, and what is read
            // of it a connection.
            ExprKind::Capture(_) | ExprKind::Captured { .. } => Connection,
            ExprKind::Error { message, code } => {
                code.iter().fold(self.category(message), |given, code| {
                    join(given, self.category(code))
                })
            }
            ExprKind::Index { base, index } => join(self.category(base), self.category(index)),
            ExprKind::Tuple(entries) | ExprKind::Sequence(entries) => all(&mut entries.iter()),
            ExprKind::Pairs(entries) => {
                all(&mut entries.iter().flat_map(|(key, value)| [key, value]))
            }
            ExprKind::Block {
                value: Some(value), ..
            } => self.category(value),
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
                    .map(|branch| self.category(branch));
                match branches.next() {
                    Some(first) => join(self.category(condition), branches.fold(first, join)),
                    None => Value,
                }
            }
            _ => Value,
        }
    }

    /// FN-3: "Where a constant is required ... a `const` parameter's
    /// argument ... a connection rejects." A connection has no value
    /// while the graph is built. TIME-2: "Composition describes a typed
    /// graph without reading live payloads."
    pub(super) fn constants_of(&mut self, kind: &ExprKind) {
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
            Callee::Unread => return,
        };
        let Some(Detail::Callable(signature)) =
            self.program.modules[module].details.get(declaration)
        else {
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
            if parameter.constant && self.category(&argument.value) == ValueCategory::Connection {
                self.diagnostics.emit(error(
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
    pub(super) fn configuration_input(&mut self, form: Form, option: &str, value: &Expr) {
        if self.category(value) != ValueCategory::Connection {
            return;
        }
        let symbol = match form {
            Form::Binary(operator) => operator.symbol(),
            Form::Unary(operator) => operator.symbol(),
            Form::Named => return,
        };
        self.diagnostics.emit(
            error(
                Category::Phase,
                codes::CONNECTION_AS_CONSTANT,
                value.at,
                format!(
                    "`{option}` configures `{symbol}`, which binds it where the graph is built, \
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

/// True when an expression gives a value where it stands. A block gives
/// the one it ends in (CTRL-10), and none when it ends in a `return` or
/// any other statement; an `if` gives one where a branch does.
fn gives_a_value(value: &Expr) -> bool {
    match value.kind.branch_values() {
        Some(values) => values.into_iter().any(gives_a_value),
        None => true,
    }
}
