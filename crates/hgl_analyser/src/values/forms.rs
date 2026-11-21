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
use super::fit::{open, whole};
use crate::hir::{Expr, ExprKind, Hook, NULL_READ_CODE, Raise, ValueCategory};
use crate::program::Role;
use crate::types::Type;
use crate::{codes, error};

/// What a place that expects another type takes of a value given to it
/// (ERR-12, ERR-15).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Taken {
    /// A result's T: a success's value, a failure raised.
    Value,
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
    /// is expected is read.
    pub(super) fn taken(&self, expected: &Type, ty: &Type) -> Option<Taken> {
        if self.admits(expected, ty) || open(expected) || !whole(ty) {
            return None;
        }
        match ty {
            Type::Result { value, error } => {
                let nullable_form = **error == Type::Error
                    && !matches!(**value, Type::Nullable(_))
                    && matches!(expected, Type::Nullable(inner) if self.admits(inner, value));
                if nullable_form {
                    Some(Taken::Nullable)
                } else if self.admits(expected, value) {
                    Some(Taken::Value)
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
        let at = value.at;
        let (kind, ty) = match (taken, value.ty.clone()) {
            (Taken::Value, Type::Result { value: success, .. }) => {
                let raise = self.raise_here();
                self.raisable(&value, raise);
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
                let raise = self.null_raise_here();
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
        match &base.ty {
            Type::Result { value, .. } if whole(&base.ty) && !open(value) => {
                self.take(base, Taken::Value)
            }
            Type::Nullable(payload) if reads(payload) => self.take(base, Taken::Read),
            _ => base,
        }
    }

    /// A result compared with `null` (ERR-15: "Compared with `null`, where
    /// T is not nullable, it expects the `T?`: `checked(value) == null`
    /// tests for a failure. Where T is nullable that place expects T, and
    /// takes the T form: the comparison is of a success's value, and a
    /// failure is raised"); any other value as it is.
    pub(super) fn compared_with_null(&mut self, value: Expr) -> Expr {
        match &value.ty {
            Type::Result {
                value: success,
                error,
            } if whole(&value.ty) && !open(success) => {
                if matches!(**success, Type::Nullable(_)) || **error != Type::Error {
                    self.take(value, Taken::Value)
                } else {
                    self.take(value, Taken::Nullable)
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

    /// [`Typer::raise_here`], for the failure of a null read (ERR-12: "an
    /// execution failure raised as a result's failure is (ERR-5)"): out of
    /// a value function that gives a `result<U, error>`, and as an
    /// execution failure of one whose failure is of another type, which
    /// a null's `error` is not.
    fn null_raise_here(&self) -> Raise {
        match (self.raise_here(), &self.signature.result) {
            (Raise::Return, Some(Type::Result { error, .. })) if **error != Type::Error => {
                Raise::Execution
            }
            (raise, _) => raise,
        }
    }

    /// Reports `result`, given where its T is expected and raised as
    /// `raise` says, where its error may not be raised there (ERR-5): "In
    /// a value function that gives a `result<U,E>`, a `result<T,E>` of
    /// that same E raises its failure as the function's ... Any other
    /// result given where its T is expected rejects
    /// (`result.propagation_type`): in a function that gives a
    /// `result<U,E>`, one of another E, and elsewhere one whose E is not
    /// `error`."
    fn raisable(&mut self, result: &Expr, raise: Raise) {
        let Type::Result { error: failure, .. } = &result.ty else {
            return;
        };
        let (allowed, place) = match (raise, &self.signature.result) {
            (Raise::Return, Some(Type::Result { error: own, .. })) => {
                if !whole(own) || open(own) {
                    return;
                }
                (
                    (**own).clone(),
                    format!(
                        "`{}` gives `{}`, whose failure is `{own}`",
                        self.declaration.name,
                        self.signature.result.as_ref().expect("matched")
                    ),
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
