//! Writes the activity of a node's inputs, and the temporal form of an
//! enum: the family of the dispatch point (`forms`) for `is_active`,
//! `activate` and `passivate` (TIME-9, END-5), and for a temporal value
//! of an enum, which publishes whole as a scalar does (TYPE-16). Part of
//! the transpiler, the last stage.
//!
//! | HGL | C++ |
//! | --- | --- |
//! | `is_active(data)` (END-5: "the current activity of the receiving node's direct input") | `::hglrt::fn_is_active(data)`, of the input's hgraph view |
//! | `activate(data)`, `passivate(data)` in a `when` (TIME-3, END-5) | `::hglrt::fn_activate(data);`, `::hglrt::fn_passivate(data);`: hgraph's own subscription of that input binding, which the input's declared activity (`nodes`) starts from |
//! | A temporal value of an enum `E` (TYPE-7) | hgraph's `TS<E>`: the library registers an enum with hgraph as an atomic (`values.h`) |
//! | A read of its payload, a publication, its `delta_value`, a constant connection, a trace cell | as a scalar's (`scalars`): an enum's delta is the enum itself (TYPE-16) |
//!
//! What a handler subscribes to and requires is `hgl_lower::nodes`'s:
//! `is_active` sets neither (TIME-9: "It reads the binding's current
//! activity without setting a subscription or validity requirement"),
//! and is a condition of its handler like any other. `activate` and
//! `passivate` change the binding while the node runs; the handlers'
//! guards read `modified` of the view as it is, so a passive input that
//! published in the cycle is modified (TIME-3). `activate` and
//! `passivate` in `stop` are not written, and are reported: hgraph makes
//! passive at stop only the inputs a node declares active.

use hgl_analyser::hir::{EndpointOperation, Expr, ExprKind, Hook, Stmt};
use hgl_analyser::{DeclarationKind, Detail, Type};

use super::Emitter;
use super::forms::Family;
use super::graphs::WIRING;
use super::nodes::{Kind, Role};
use super::statements::Scope;
use super::values::INDENT;
use crate::runtime;

/// The family of input activity, and of the enums' temporal form.
pub(super) struct Activity;

/// The library's native of `operation`, where it is one of an input's
/// activity: `is_active`, `activate` or `passivate` (TIME-9).
fn activity_native(operation: EndpointOperation) -> Option<&'static str> {
    Some(match operation {
        EndpointOperation::IsActive => "fn_is_active",
        EndpointOperation::Activate => "fn_activate",
        EndpointOperation::Passivate => "fn_passivate",
        _ => return None,
    })
}

/// The C++ type of `ty` where it is an enum with a member, noting it
/// where it is another module's (`types`). An enum of no member has no
/// value to publish, and the library cannot name it (`types` writes its
/// `hgl_enum` only where it has one).
fn enumeration(emitter: &mut Emitter, ty: &Type) -> Option<String> {
    let Type::Named {
        module,
        declaration,
        ..
    } = ty
    else {
        return None;
    };
    let declared = &emitter.program.modules[*module];
    if declared.declarations[*declaration].kind != DeclarationKind::Enum {
        return None;
    }
    let Some(Detail::Enum(read)) = declared.details.get(*declaration) else {
        return None;
    };
    if read.members.is_empty() {
        return None;
    }
    emitter.named_type(ty)
}

impl Family for Activity {
    // TYPE-7: an enum is a value type of its members, which hgraph holds
    // as an atomic.
    fn schema(&self, emitter: &mut Emitter<'_>, ty: &Type) -> Option<String> {
        enumeration(emitter, ty).map(|written| format!("::hgraph::TS<{written}>"))
    }

    // TIME-1: "A payload read requires admission/readability", checked
    // again when it runs, as a scalar's is.
    fn read(&self, emitter: &mut Emitter<'_>, ty: &Type, view: &str) -> Option<String> {
        enumeration(emitter, ty)?;
        Some(format!("::{}::checked_value({view})", runtime::NAMESPACE))
    }

    // TIME-7: "Where T publishes whole, as a scalar does, `delta<T>` is T
    // and the two are one"; TYPE-16 has an enum's delta the enum itself.
    fn publish(
        &self,
        emitter: &mut Emitter<'_>,
        ty: &Type,
        view: &str,
        value: &str,
    ) -> Option<String> {
        enumeration(emitter, ty)?;
        Some(format!("{view}.set({value});"))
    }

    // END-3, ERR-14: the delta read in the form its place takes, which
    // the library's scalar form gives (`endpoint_read<TS<T>>`).
    fn delta(
        &self,
        emitter: &mut Emitter<'_>,
        ty: &Type,
        view: &str,
        checked: bool,
    ) -> Option<String> {
        enumeration(emitter, ty)?;
        let native = if checked {
            "fn_delta_value_checked"
        } else {
            "fn_delta_value"
        };
        Some(format!("::{}::{native}({view})", runtime::NAMESPACE))
    }

    // FN-3: a value where a connection is expected is wired as a
    // constant, one publication when the graph starts.
    fn constant(&self, emitter: &mut Emitter<'_>, ty: &Type, value: &str) -> Option<String> {
        let written = enumeration(emitter, ty)?;
        Some(format!(
            "::{}::constant<{written}>({WIRING}, {value})",
            runtime::NAMESPACE
        ))
    }

    // TST-5: a cell is "written as a `return` is": an enum's is its value.
    fn cell(
        &self,
        emitter: &mut Emitter<'_>,
        scope: &mut Scope<'_>,
        ty: &Type,
        value: &Expr,
    ) -> Option<String> {
        enumeration(emitter, ty)?;
        Some(emitter.expression(scope, value))
    }

    // TIME-9: `is_active`, `activate` and `passivate` take a direct
    // temporal input of the node, given as its hgraph view.
    fn expression(
        &self,
        emitter: &mut Emitter<'_>,
        scope: &mut Scope<'_>,
        value: &Expr,
    ) -> Option<String> {
        let ExprKind::Endpoint { operation, inputs } = &value.kind else {
            return None;
        };
        let native = activity_native(*operation)?;
        let changes = *operation != EndpointOperation::IsActive;
        // TIME-5: "`start` ... gives none to an endpoint operation", which
        // the analyser rejects. A change in `stop` would outlive the node's
        // run: hgraph makes passive at stop only what the node declares
        // active.
        let admitted = match emitter.kind() {
            Some(Kind::Hook(Hook::When)) => true,
            Some(Kind::Hook(Hook::Stop)) => !changes,
            _ => false,
        };
        if !admitted {
            return Some(emitter.missing(value.at, &format!("`{}` here", operation.name())));
        }
        let view = match inputs.as_deref() {
            Some([input]) if emitter.role(*input) == Role::Input => {
                emitter.view(&scope.body.locals, *input)
            }
            _ => None,
        };
        let Some(view) = view else {
            return Some(emitter.missing(value.at, &format!("`{}` of this", operation.name())));
        };
        // A change of activity does something: a `?` after it in its
        // statement is not run before it (EXPR-13).
        if changes {
            scope.acted = true;
        }
        Some(format!("::{}::{native}({view})", runtime::NAMESPACE))
    }

    // END-5: `activate(data)` and `passivate(data)` stand as statements
    // of their own, and give nothing.
    fn statement(
        &self,
        emitter: &mut Emitter<'_>,
        scope: &mut Scope<'_>,
        statement: &Stmt,
        depth: usize,
        text: &mut String,
    ) -> bool {
        let Stmt::Expr(
            value @ Expr {
                kind:
                    ExprKind::Endpoint {
                        operation: EndpointOperation::Activate | EndpointOperation::Passivate,
                        ..
                    },
                ..
            },
        ) = statement
        else {
            return false;
        };
        let written = emitter.expression(scope, value);
        text.push_str(&format!("{}{written};\n", INDENT.repeat(depth)));
        true
    }
}
