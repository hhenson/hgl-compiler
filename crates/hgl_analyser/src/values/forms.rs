//! The form a result takes from the place it is given to, and the read of
//! a `T?` given where a T is expected (ERR-5, ERR-12, ERR-15). Part of the
//! typing pass ([`super`]).
//!
//! ERR-15: "A function that can fail declares `result<T, error>` as its
//! result. That result takes its form from its place wherever it is
//! given". Where the place expects the result it is given itself; where
//! it expects T, a success's value, a failure's own error raised; where
//! it expects `T?`, a success's value, or null for a failure. ERR-12: "A
//! `T?` value given where a T is expected is read as its value: the place
//! tries it, and a null fails the read with the code `value.null`".
//!
//! Each is recorded as an expression of its own, [`ExprKind::Raised`],
//! [`ExprKind::OrNull`] or [`ExprKind::NullRead`], with where a failure
//! is raised ([`Raise`]), so that a target reads what the place takes and
//! decides none of it. In composition, where the value is a connection,
//! which node raises is settled once the categories are
//! (`lifting::record`).

use hgl_diagnostics::Category;

use super::Typer;
use super::fit::{checked_child_result, open, whole};
use crate::hir::{ConfigurationRead, Expr, ExprKind, Hook, NULL_READ_CODE, Raise, ValueCategory};
use crate::program::Role;
use crate::types::Type;
use crate::{codes, error};

/// The type after the same result and nullable reads as `Typer::received`.
pub(super) fn received_type(ty: &Type, reads: impl Fn(&Type) -> bool) -> &Type {
    let received = match ty {
        Type::Result { value, .. } if whole(ty) && !open(value) => value,
        _ => ty,
    };
    match received {
        Type::Nullable(payload) if reads(payload) => payload,
        _ => received,
    }
}

/// What a place that expects another type takes of a value given to it
/// (ERR-12, ERR-15).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Taken {
    /// A result's T: a success's value, a failure raised.
    Value,
    /// A result's nullable success read as its value, raising the result's
    /// error or `value.null` where applicable.
    ValueRead,
    /// A result's `T?`: a success's value, or null for a failure.
    Nullable,
    /// A `T?`'s T: its value, a null failing the read.
    Read,
}

impl<'a> Typer<'a> {
    /// What a place that expects `expected` takes of a value of type
    /// `ty`, where it does not take it as it is: `None` where the place
    /// takes the value itself, or where it takes no form of it.
    ///
    /// ERR-15: "Where the place expects `result<T, error>`, or
    /// `result<T, error>?` ... the result is given itself. Where it
    /// expects T, it gives a success's value, and a failure's own error
    /// is raised ... Where it expects `T?`, it gives a success's value,
    /// or null for a failure ... Where T is itself nullable, a place that
    /// expects T takes the T form: a null there is a success's value and
    /// never a failure." ERR-5: "In a value function that gives a
    /// `result<U,E>`, a `result<T,E>` of that same E raises its failure
    /// as the function's"; a result of another E given where its T is
    /// expected rejects there (`result.propagation_type`), and is taken
    /// as the T form for [`Typer::take`] to say so. A result of an E
    /// that is not `error` has no `T?` form. ERR-12: a `T?` where its T
    /// is expected is read. The owner confirmed in hhenson/hgl-compiler#345
    /// that a `result<U?, error>` given where a U is taken first takes the
    /// result's value and then reads the nullable success.
    pub(super) fn taken(&self, expected: &Type, ty: &Type) -> Option<Taken> {
        if self.admits(expected, ty) || open(expected) || !whole(ty) {
            return None;
        }
        match ty {
            Type::Result { value, error } => {
                let nullable_success = matches!(**value, Type::Nullable(_));
                let nullable_form = !nullable_success
                    && matches!(expected, Type::Nullable(inner) if self.admits(inner, value));
                if nullable_form {
                    // EXPR-5 lets another E raise where its T is expected.
                    // ERR-15 gives only result<T, error> the T? form.
                    (**error == Type::Error).then_some(Taken::Nullable)
                } else if self.admits(expected, value) {
                    Some(Taken::Value)
                } else if **error == Type::Error
                    && matches!(value.as_ref(), Type::Nullable(payload) if self.admits(expected, payload))
                {
                    Some(Taken::ValueRead)
                } else {
                    None
                }
            }
            Type::Nullable(payload) if self.admits(expected, payload) => Some(Taken::Read),
            _ => None,
        }
    }

    /// `value` in the form `taken` says its place takes ([`Typer::taken`]).
    /// A result raised where its error may not be is reported
    /// (`result.propagation_type`), and is taken all the same, so that
    /// nothing more is reported of it (ERR-9).
    pub(super) fn take(&mut self, value: Expr, taken: Taken) -> Expr {
        self.take_with(value, taken, None)
    }

    /// Takes a value's form either in the current scope or in the
    /// scope where a required operator's configuration was written.
    pub(super) fn take_with(
        &mut self,
        value: Expr,
        taken: Taken,
        context: Option<&ConfigurationRead>,
    ) -> Expr {
        if taken == Taken::ValueRead {
            let success = self.take_with(value, Taken::Value, context);
            return self.take_with(success, Taken::Read, context);
        }
        let at = value.at;
        let (kind, ty) = match (taken, value.ty.clone()) {
            (Taken::Value, Type::Result { value: success, .. }) => {
                let raise = context.map_or_else(|| self.raise_here(), |context| context.raise);
                self.raisable(&value, raise, context);
                let kind = ExprKind::Raised {
                    result: Box::new(value),
                    raise,
                };
                (kind, *success)
            }
            (Taken::Nullable, Type::Result { value: success, .. }) => {
                let kind = ExprKind::OrNull {
                    result: Box::new(value),
                    reader: false,
                };
                (kind, Type::Nullable(success))
            }
            (Taken::Read, Type::Nullable(payload)) => {
                let raise = context.map_or_else(|| self.raise_here(), |context| context.raise);
                let result = self.result_in(context);
                // An execution failure of a function with another E
                // is not returned as its failure (ERR-5, ERR-12).
                let raise = match (raise, result) {
                    (Raise::Return, Some(Type::Result { error, .. })) if **error != Type::Error => {
                        Raise::Execution
                    }
                    _ => raise,
                };
                let kind = ExprKind::NullRead {
                    value: Box::new(value),
                    raise,
                    code: NULL_READ_CODE,
                };
                (kind, *payload)
            }
            _ => return value,
        };
        Expr {
            kind,
            ty,
            at,
            category: ValueCategory::Unsettled,
        }
    }

    /// `base`, what a member or an index is read from, as that place
    /// takes it (ERR-15: "What a member or an index is read from ...
    /// expect[s] the T too, but for `.value` and `.error` read from a
    /// result, which are the result's own (ERR-3)"; ERR-12): a result's
    /// T, the value of a `T?`. `reads` says whether a `T?`'s payload is
    /// one this place reads, a struct's for a member, a list's for an
    /// index; what is not is left as it is.
    pub(super) fn received(&mut self, base: Expr, reads: impl Fn(&Type) -> bool) -> Expr {
        let base = match &base.ty {
            Type::Result { value, .. } if whole(&base.ty) && !open(value) => {
                self.take(base, Taken::Value)
            }
            _ => base,
        };
        match &base.ty {
            Type::Nullable(payload) if reads(payload) => self.take(base, Taken::Read),
            _ => base,
        }
    }

    /// A result compared with `null` (ERR-15: "Compared with `null`, where
    /// T is not nullable, it expects the `T?`: `checked(value) == null`
    /// tests for a failure. Where T is nullable that place expects T, and
    /// takes the T form: the comparison is of a success's value, and a
    /// failure is raised"). A runtime-checked child offers the same
    /// forms (ERR-14).
    pub(super) fn compared_with_null(
        &mut self,
        value: Expr,
        context: Option<&ConfigurationRead>,
    ) -> Expr {
        if context.is_none() {
            self.record_result_read(&value, None);
        }
        // A nullable child already represents the payload read. Keeping
        // it here also lets a null guard refine the original bound child.
        let value = if matches!(value.ty, Type::Nullable(_)) {
            value
        } else {
            checked_child_result(&value).unwrap_or(value)
        };
        match &value.ty {
            Type::Result {
                value: success,
                error,
            } if whole(&value.ty) && !open(success) => {
                if matches!(**success, Type::Nullable(_)) || **error != Type::Error {
                    self.take_with(value, Taken::Value, context)
                } else {
                    self.take_with(value, Taken::Nullable, context)
                }
            }
            _ => value,
        }
    }

    /// Where a failure that the place a value stands in raises goes
    /// (ERR-5): out of a value function that gives a `result`, as an
    /// execution failure of one that gives no result, the node's in a
    /// `when`, the source's in its body, the phase's in a lifecycle hook,
    /// where a graph is built, and among a test's or an application's
    /// statements. In composition a connection's is settled once it is
    /// known to be one (`lifting::record`): it is written here as the
    /// graph's building's.
    pub(super) fn raise_here(&self) -> Raise {
        // ERR-5: failure propagation targets the function doing the
        // read, including one written inside a hook or a source.
        if let Some(scope) = &self.anonymous {
            return match &scope.result {
                Some(Type::Result { .. }) => Raise::Return,
                _ => Raise::Execution,
            };
        }
        if self.testing {
            return Raise::Test;
        }
        if self.applying || self.value_code && !self.graph {
            return Raise::Application;
        }
        if self.graph {
            return Raise::Build;
        }
        // TIME-5: a `state`'s and a `cache`'s initializer are evaluated
        // as the node starts.
        if self.initializing || self.caching {
            return Raise::Lifecycle(Hook::Start);
        }
        match self.hook {
            Some(Hook::When) => return Raise::Node,
            Some(hook) => return Raise::Lifecycle(hook),
            None => {}
        }
        if self.generating {
            return Raise::Source;
        }
        let value_function = self.declaration.kind.role() == Some(Role::Value);
        match &self.signature.result {
            Some(Type::Result { .. }) if value_function => Raise::Return,
            _ => Raise::Execution,
        }
    }

    /// The function's result at the payload read.
    fn result_in<'r>(&'r self, context: Option<&'r ConfigurationRead>) -> Option<&'r Type> {
        context.map_or_else(|| self.result_type(), |context| context.result.as_ref())
    }

    /// Reports `result`, given where its T is expected and raised as
    /// `raise` says, where its error may not be raised there (ERR-5): "In
    /// a value function that gives a `result<U,E>`, a `result<T,E>` of
    /// that same E raises its failure as the function's ... Any other
    /// result given where its T is expected rejects
    /// (`result.propagation_type`): in a function that gives a
    /// `result<U,E>`, one of another E, and elsewhere one whose E is not
    /// `error`."
    fn raisable(&mut self, result: &Expr, raise: Raise, context: Option<&ConfigurationRead>) {
        let Type::Result { error: failure, .. } = &result.ty else {
            return;
        };
        let anonymous = context.map_or(self.anonymous.is_some(), |context| context.anonymous);
        let (allowed, place) = match (raise, self.result_in(context)) {
            (_, Some(result)) if anonymous && open(result) => return,
            (Raise::Return, Some(result @ Type::Result { error: own, .. })) => {
                if !whole(own) || open(own) {
                    return;
                }
                let who = if anonymous {
                    "the anonymous function".to_string()
                } else {
                    format!("`{}`", self.declaration.name)
                };
                (
                    (**own).clone(),
                    format!("{who} gives `{result}`, whose failure is `{own}`"),
                )
            }
            (raise, _) => (
                Type::Error,
                format!("{} raises an `error`", raise_place(raise)),
            ),
        };
        if **failure == allowed {
            return;
        }
        self.diagnostics.emit(
            error(
                Category::Type,
                codes::RESULT_PROPAGATION_TYPE,
                result.at,
                format!(
                    "this is `{}`, given where its value is expected, and {place}: its failure \
                     of `{failure}` is not raised here",
                    result.ty
                ),
                "ERR-5",
            )
            .with_note(
                "hold the result and test it, `if is_err(outcome) { ... }`, or give it where \
                 a result is taken",
            ),
        );
    }
}

/// What raises a failure where `raise` says it goes, for a report.
fn raise_place(raise: Raise) -> &'static str {
    match raise {
        Raise::Return | Raise::Execution => "a value function",
        Raise::Node => "a `when`",
        Raise::Source => "a source",
        Raise::Lifecycle(Hook::Start) => "`start`",
        Raise::Lifecycle(_) => "`stop`",
        Raise::Build => "a graph",
        Raise::Test => "a test",
        Raise::Application => "an application",
        Raise::CallNode | Raise::Reader => "a node",
    }
}
