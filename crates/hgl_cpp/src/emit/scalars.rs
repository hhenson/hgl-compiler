//! The temporal form of a scalar: the family of the dispatch point
//! (`forms`) that writes a temporal value of `i64`, `f64`, `bool`, `str`
//! and the other scalars. Part of the transpiler, the last stage.
//!
//! | HGL | C++ |
//! | --- | --- |
//! | A temporal value of a scalar `T` | hgraph's `TS<T>` |
//! | A read of its payload in a hook (TIME-1) | `::hglrt::checked_value(view)` |
//! | `return value`, `out = value` (TIME-7) | `view.set(value);`: a scalar publishes whole, and its delta is itself (TYPE-16) |
//! | `delta_value(view)` (END-3, ERR-14) | `::hglrt::fn_delta_value(view)`, or `fn_delta_value_checked` where its place takes a `result` |
//! | An ordinary value where a connection is expected (FN-3) | `::hglrt::constant<T>(wiring_, value)` |
//! | A cell of a trace of it (TST-5) | the value |
//!
//! An enum, a nullable and every composite type are not this family's.

use hgl_analyser::hir::Expr;
use hgl_analyser::{Scalar, Type};

use super::Emitter;
use super::forms::Family;
use super::graphs::WIRING;
use super::statements::Scope;
use super::types::cpp_type;
use crate::runtime;

/// The scalars' family.
pub(super) struct Scalars;

/// The C++ type of `ty` where it is a scalar this family writes: every
/// one but `timezone`, `zoned_time` and `any`, which have no C++ type
/// yet.
fn scalar(emitter: &Emitter, ty: &Type) -> Option<String> {
    match ty {
        Type::Scalar(Scalar::Timezone | Scalar::ZonedTime | Scalar::Any) => None,
        Type::Scalar(_) => cpp_type(emitter.program, ty),
        _ => None,
    }
}

impl Family for Scalars {
    fn schema(&self, emitter: &mut Emitter<'_>, ty: &Type) -> Option<String> {
        scalar(emitter, ty).map(|written| format!("::hgraph::TS<{written}>"))
    }

    // TIME-1: "A payload read requires admission/readability". The
    // analyser has proved each read admitted; it is checked again when it
    // runs, and fails with `value.unset_read` where it is not.
    fn read(&self, emitter: &mut Emitter<'_>, ty: &Type, view: &str) -> Option<String> {
        scalar(emitter, ty)?;
        Some(format!("::{}::checked_value({view})", runtime::NAMESPACE))
    }

    // TIME-7: "Where T publishes whole, as a scalar does, `delta<T>` is T
    // and the two are one."
    fn publish(
        &self,
        emitter: &mut Emitter<'_>,
        ty: &Type,
        view: &str,
        value: &str,
    ) -> Option<String> {
        scalar(emitter, ty)?;
        Some(format!("{view}.set({value});"))
    }

    // END-3: "For a scalar it is the value published"; ERR-14 has the read
    // take its form from its place.
    fn delta(
        &self,
        emitter: &mut Emitter<'_>,
        ty: &Type,
        view: &str,
        checked: bool,
    ) -> Option<String> {
        scalar(emitter, ty)?;
        let native = if checked {
            "fn_delta_value_checked"
        } else {
            "fn_delta_value"
        };
        Some(format!("::{}::{native}({view})", runtime::NAMESPACE))
    }

    // FN-3: "An ordinary value of type T is accepted where composition
    // expects a connection of T, and is wired as a constant, one
    // publication when the graph starts". The type is given, so that a
    // literal is the scalar it is in HGL.
    fn constant(&self, emitter: &mut Emitter<'_>, ty: &Type, value: &str) -> Option<String> {
        let written = scalar(emitter, ty)?;
        Some(format!(
            "::{}::constant<{written}>({WIRING}, {value})",
            runtime::NAMESPACE
        ))
    }

    // TST-5: a cell is "written as a `return` is": a scalar's is its value.
    fn cell(
        &self,
        emitter: &mut Emitter<'_>,
        scope: &mut Scope<'_>,
        ty: &Type,
        value: &Expr,
    ) -> Option<String> {
        scalar(emitter, ty)?;
        Some(emitter.expression(scope, value))
    }
}
