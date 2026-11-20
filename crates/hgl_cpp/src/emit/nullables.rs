//! The temporal form of a nullable scalar: the family of the dispatch
//! point (`forms`) that writes a temporal value of `i64?`, `datetime?`
//! and the other nullables of a scalar or an enum. Part of the
//! transpiler, the last stage.
//!
//! | HGL | C++ |
//! | --- | --- |
//! | A temporal value of `T?`, T a scalar or an enum (TYPE-14) | hgraph's `TS<Optional<T>>`, whose published null is a value that holds nothing: valid and modified (TYPE-15: "A present null is ordinary data, distinct from an invalid temporal input or silence") |
//! | A read of its payload in a hook (TIME-1) | `::hglrt::nullable_value(view)`, a `std::optional<T>`, checked when it runs |
//! | A read of it where a test against null has proved it is not null (ERR-12) | `::hglrt::nullable_payload(view)`, the T |
//! | `value == null`, `value != null` of it (ERR-12) | whether that read holds nothing, or something |
//! | `return value`, `out = value` (TIME-7) | `::hglrt::publish_nullable(view, value);`: "`return null` for an `i64?` result a null, as `return delta<i64?>()` does" |
//! | `delta_value(view)` (END-3, ERR-14) | `::hglrt::fn_delta_value(view)`, the optional that is `delta<T?>`, or `fn_delta_value_checked` where its place takes a `result` |
//! | A cell of a trace of it (TST-5) | a `std::optional<T>`: a nullable publishes whole, a published null among it |
//!
//! A nullable of any other type, and a nullable where a connection is
//! expected (FN-3), are not this family's, and are reported. A `T?` of
//! a type that is no ordinary value marks a parameter that may be left
//! out (TYPE-14), and is the type under it (`Type::omission_removed`).

use hgl_analyser::hir::{BinaryOperator, Callee, Expr, ExprKind, Value};
use hgl_analyser::{Argument as GenericArgument, DeclarationKind, Detail, Scalar, Type};

use super::Emitter;
use super::forms::Family;
use super::names::identifier;
use super::nodes::Role;
use super::statements::Scope;
use crate::runtime;

/// The nullables' family.
pub(super) struct Nullables;

/// The C++ type of the payload of `ty`, where `ty` is a nullable this
/// family writes: of a scalar that has a temporal form (`scalars`), or of
/// an enum with a member, noted where it is another module's. TYPE-14:
/// "`T?` denotes an ordinary value that is either a T or null".
fn payload(emitter: &mut Emitter, ty: &Type) -> Option<String> {
    let Type::Nullable(inner) = ty else {
        return None;
    };
    match &**inner {
        Type::Scalar(Scalar::Timezone | Scalar::ZonedTime | Scalar::Any) => None,
        Type::Scalar(_) => emitter.named_type(inner),
        Type::Named {
            module,
            declaration,
            ..
        } => {
            let declared = &emitter.program.modules[*module];
            if declared.declarations[*declaration].kind != DeclarationKind::Enum {
                return None;
            }
            match declared.details.get(*declaration) {
                Some(Detail::Enum(read)) if !read.members.is_empty() => emitter.named_type(inner),
                _ => None,
            }
        }
        _ => None,
    }
}

/// The view of `value` where it names a temporal input of the node whose
/// type is a nullable this family writes, with that type.
fn nullable_input<'v>(
    emitter: &Emitter,
    scope: &Scope,
    value: &'v Expr,
) -> Option<(String, &'v Type)> {
    let ExprKind::Local(local) = value.kind else {
        return None;
    };
    if emitter.role(local) != Role::Input {
        return None;
    }
    let declared = &scope.body.locals[local];
    matches!(declared.ty, Type::Nullable(_)).then(|| (identifier(&declared.name), &value.ty))
}

/// The name of the declaration the call `value` calls, where it is one.
fn callee_name<'p>(emitter: &Emitter<'p>, value: &Expr) -> Option<&'p str> {
    let ExprKind::Call { callee, .. } = &value.kind else {
        return None;
    };
    let (module, declaration) = callee.declaration()?;
    let program = emitter.program;
    Some(
        program.modules[module].declarations[declaration]
            .name
            .as_str(),
    )
}

impl Family for Nullables {
    fn schema(&self, emitter: &mut Emitter<'_>, ty: &Type) -> Option<String> {
        payload(emitter, ty).map(|written| format!("::hgraph::TS<::hgraph::Optional<{written}>>"))
    }

    // TIME-1: "A payload read requires admission/readability", checked
    // again when it runs; the payload of a nullable is the optional,
    // which holds nothing for a published null.
    fn read(&self, emitter: &mut Emitter<'_>, ty: &Type, view: &str) -> Option<String> {
        payload(emitter, ty)?;
        Some(format!("::{}::nullable_value({view})", runtime::NAMESPACE))
    }

    // TIME-7: "a value that is a T is published whole ... `return null`
    // for an `i64?` result a null, as `return delta<i64?>()` does": a
    // nullable's delta is the optional of what was published.
    fn publish(
        &self,
        emitter: &mut Emitter<'_>,
        ty: &Type,
        view: &str,
        value: &str,
    ) -> Option<String> {
        payload(emitter, ty)?;
        Some(format!(
            "::{}::publish_nullable({view}, {value});",
            runtime::NAMESPACE
        ))
    }

    // END-3: "For a nullable scalar it is the optional that type gives
    // it, which holds the value published or the null"; ERR-14 has the
    // read take its form from its place. The library reads it
    // (`endpoint_read<TS<Optional<T>>>`).
    fn delta(
        &self,
        emitter: &mut Emitter<'_>,
        ty: &Type,
        view: &str,
        checked: bool,
    ) -> Option<String> {
        payload(emitter, ty)?;
        let native = if checked {
            "fn_delta_value_checked"
        } else {
            "fn_delta_value"
        };
        Some(format!("::{}::{native}({view})", runtime::NAMESPACE))
    }

    // TST-5: a cell is "written as a `return` is": a nullable publishes
    // whole, so its cell is the optional, a null that holds nothing.
    fn cell(
        &self,
        emitter: &mut Emitter<'_>,
        scope: &mut Scope<'_>,
        ty: &Type,
        value: &Expr,
    ) -> Option<String> {
        let written = payload(emitter, ty)?;
        let value = emitter.expression(scope, value);
        Some(format!("std::optional<{written}>{{{value}}}"))
    }

    fn expression(
        &self,
        emitter: &mut Emitter<'_>,
        scope: &mut Scope<'_>,
        value: &Expr,
    ) -> Option<String> {
        match &value.kind {
            // ERR-12: "a binding a test has proved is not null is read as
            // its payload's type there": a nullable input so read is its
            // payload, which the test has proved is there.
            ExprKind::Local(_) => {
                let (view, read) = nullable_input(emitter, scope, value)?;
                if matches!(read, Type::Nullable(_)) {
                    return None;
                }
                scope.read = true;
                scope.acted = true;
                Some(format!(
                    "::{}::nullable_payload({view})",
                    runtime::NAMESPACE
                ))
            }
            // ERR-12: a test of a nullable input against `null` reads its
            // payload, and asks whether it holds a value.
            ExprKind::Binary {
                operator: operator @ (BinaryOperator::Equal | BinaryOperator::NotEqual),
                left,
                right,
            } => {
                let null = |side: &Expr| matches!(side.kind, ExprKind::Literal(Value::Null));
                let tested = match (null(left), null(right)) {
                    (true, false) => right,
                    (false, true) => left,
                    _ => return None,
                };
                let (view, _) = nullable_input(emitter, scope, tested)?;
                scope.read = true;
                scope.acted = true;
                let read = format!("::{}::nullable_value({view})", runtime::NAMESPACE);
                Some(if *operator == BinaryOperator::Equal {
                    format!("(!{read}.has_value())")
                } else {
                    format!("{read}.has_value()")
                })
            }
            // CAP-6: a nullable put in the run's store is read back by a
            // `get` whose `V` the analyser gives as the payload's type,
            // `i64` for `let saved: i64? = get(...)` and for
            // `get<i64?>(...)` alike, so that a stored null would be read
            // as a value of another type, which fails. Until the two
            // agree, such a `put` is not written.
            ExprKind::Call {
                callee:
                    Callee::Candidate {
                        module, generics, ..
                    },
                ..
            } if Some(*module) == emitter.program.core()
                && callee_name(emitter, value) == Some("put")
                && generics.iter().any(|argument| {
                    matches!(argument, GenericArgument::Type(Type::Nullable(_)))
                }) =>
            {
                Some(emitter.missing(value.at, "a nullable value put in the run's store"))
            }
            _ => None,
        }
    }
}
