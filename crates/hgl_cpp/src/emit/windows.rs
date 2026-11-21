//! The temporal form of a window, `rolling_window<V, Max, Min>` (TYPE-11;
//! core's END-9): the family of the dispatch point (`forms`) that writes a
//! window of a tick count as hgraph's `TSW` and one of a duration as
//! hgraph's `TSWDuration`, and the reads of one that the runtime library's
//! `windows.h` makes. Part of the transpiler, the last stage.
//!
//! | HGL | C++ |
//! | --- | --- |
//! | `rolling_window<i64, 3, 2>`; `rolling_window<i64, 3us, 1us>` | `::hgraph::TSW<::hgraph::Int, 3, 2>`; `::hgraph::TSWDuration<::hgraph::Int, 3, 1>`, its bounds in microseconds |
//! | `return value`, `out = value` to a window result in a node's `when` (TIME-7, END-5) | `arrival_.hold(value);`: the node's `eval` runs its handlers in `::hglrt::WindowArrival::run`, which holds the last value written in the evaluation and then pushes it, the cycle's one arrival (TYPE-11) |
//! | A source's `yield` to a window (TIME-7) | `::hglrt::window_push(view, value);`: the V arrives |
//! | A read of a window's value in a hook or a lifted call's node (TYPE-11) | `::hglrt::window_value(view)`, a `list<V>` oldest first, which fails with `value.unset_read` before the window is all valid; where its place takes a `result` ([`ExprKind::CheckedWindow`]), `::hglrt::window_value_checked(view)` |
//! | `all_valid(window)` (END-9) | `::hglrt::fn_all_valid(view)` |
//! | `delta_value(window)` (END-3, END-9, ERR-14) | `::hglrt::fn_delta_value(view)`, or `fn_delta_value_checked` where its place takes a `result`: the value that arrived |
//! | `removed_value(window)`, read only where its place takes a `result` (END-9, ERR-14) | `::hglrt::fn_removed_value_checked(view)` |
//! | A window given to a lifted call (FN-11, FN-12) | the helper given the window's `list<V>` (`read_type`), and not called where the window is valid and not all valid (`lifting`) |
//! | A trace of a window, a source of one (TST-16, TIME-7) | a trace of its V, each cell the V that arrives; a source's payload a V |
//!
//! `valid`, `modified` and `last_modified` of a window are the endpoints'
//! family's, which hgraph's views answer for a window as for any value.
//! A window whose V is no scalar, or whose bounds are not known,
//! `invalidate` of a window result (END-4), and a node's read of its own
//! window result in a `when`, which would not see what the evaluation
//! holds to publish, are not written yet, and are reported.

use hgl_analyser::hir::{EndpointOperation, Expr, ExprKind, Hook};
use hgl_analyser::{ConstantValue, DeclarationKind, Scalar, Type};

use super::Emitter;
use super::forms::Family;
use super::nodes::{Kind, OUT, Role};
use super::statements::Scope;
use super::types::cpp_type;
use super::values::INDENT;
use crate::runtime;

/// The name of what a node's `eval` holds of the arrival it makes at its
/// window result (`::hglrt::WindowArrival`).
const ARRIVAL: &str = "arrival_";

/// The windows' family.
pub(super) struct Windows;

/// The bounds of a window, as hgraph's schema of it takes them.
enum Bounds {
    /// A tick count: the most it holds and the least it is all valid at.
    Ticks(i64, i64),
    /// A duration, in microseconds: the oldest age it holds a value at,
    /// and the span its values make it all valid at.
    Duration(i64, i64),
}

/// A window this family writes: the C++ type of its V and its bounds.
struct Window {
    /// The C++ type of what it holds.
    element: String,
    /// Its bounds.
    bounds: Bounds,
}

/// `ty` as a window this family writes, where it is one: a window of a
/// scalar the scalars' family writes, with bounds known when the program
/// is compiled. TYPE-11: "Omitted minimum equals maximum".
fn window(emitter: &Emitter, ty: &Type) -> Option<Window> {
    let Type::RollingWindow {
        value,
        maximum,
        minimum,
    } = ty
    else {
        return None;
    };
    // A window of a nullable V publishes and reads its V's delta
    // (TYPE-11), and one of an enum is not written as a temporal value
    // yet: only a scalar's is.
    let element = match value.as_ref() {
        Type::Scalar(Scalar::Timezone | Scalar::ZonedTime | Scalar::Any) => return None,
        Type::Scalar(_) => cpp_type(emitter.program, value)?,
        _ => return None,
    };
    let minimum = minimum.as_ref().unwrap_or(maximum);
    let bounds = match (&maximum.value, &minimum.value) {
        // TYPE-11: "Tick sizes are positive".
        (ConstantValue::Int(most), ConstantValue::Int(least)) if *least > 0 && least <= most => {
            Bounds::Ticks(*most, *least)
        }
        // TYPE-11: "duration maximum is positive and minimum may be
        // zero".
        (ConstantValue::Duration(most), ConstantValue::Duration(least))
            if *most > 0 && *least >= 0 && least <= most =>
        {
            Bounds::Duration(*most, *least)
        }
        _ => return None,
    };
    Some(Window { element, bounds })
}

/// The type of the endpoint `local` of `locals` names, where it is a
/// window: a temporal input's own, and `out`'s the node's result
/// (TIME-7).
fn window_type(
    emitter: &Emitter,
    locals: &[hgl_analyser::hir::Local],
    local: usize,
) -> Option<Type> {
    let ty = match emitter.role(local) {
        Role::Input => locals.get(local)?.ty.clone(),
        Role::Out => emitter
            .temporal
            .as_ref()
            .and_then(|context| context.result.clone())?,
        _ => return None,
    };
    matches!(ty, Type::RollingWindow { .. }).then_some(ty)
}

/// True where `kind` reads endpoints: a node's hook, or a lifted call's
/// node.
fn reads_endpoints(kind: Option<Kind>) -> bool {
    matches!(kind, Some(Kind::Hook(_) | Kind::Lifted))
}

/// True where what is written is a node's `when`, whose writes to its
/// window result `out` are held until its evaluation ends (END-5).
fn holding(emitter: &Emitter, view: &str) -> bool {
    emitter.kind() == Some(Kind::Hook(Hook::When)) && view == OUT
}

/// True where `local` of `locals` is the window result of the node whose
/// `when` is being written: what a read of it gives would not hold what
/// the evaluation has written of it yet.
fn own_window(emitter: &Emitter, locals: &[hgl_analyser::hir::Local], local: usize) -> bool {
    emitter.role(local) == Role::Out
        && emitter.kind() == Some(Kind::Hook(Hook::When))
        && window_type(emitter, locals, local).is_some()
}

/// True where `value` reads the window result of the node whose `when`
/// is being written: an endpoint operation on it but `invalidate`, which
/// is reported of its own, its `delta_value` or `removed_value` into a
/// `result`, or its value into a `result`. (A plain read of `out` is the
/// endpoints' family's to report.)
fn own_read(emitter: &Emitter, locals: &[hgl_analyser::hir::Local], value: &Expr) -> bool {
    let own = |local: usize| own_window(emitter, locals, local);
    match &value.kind {
        ExprKind::Endpoint {
            operation,
            inputs: Some(inputs),
        } => *operation != EndpointOperation::Invalidate && inputs.iter().any(|&local| own(local)),
        ExprKind::CheckedWindow(read) => matches!(read.kind, ExprKind::Local(local) if own(local)),
        ExprKind::CheckedDelta(read) => match &read.kind {
            ExprKind::Endpoint {
                inputs: Some(inputs),
                ..
            } => inputs.iter().any(|&local| own(local)),
            ExprKind::Call { arguments, .. } => arguments.iter().any(
                |argument| matches!(argument.value.kind, ExprKind::EndpointOf(local) if own(local)),
            ),
            _ => false,
        },
        ExprKind::Call { arguments, .. } => arguments.iter().any(
            |argument| matches!(argument.value.kind, ExprKind::EndpointOf(local) if own(local)),
        ),
        _ => false,
    }
}

/// The library's function `name`, qualified.
fn library(name: &str) -> String {
    format!("::{}::{name}", runtime::NAMESPACE)
}

impl Family for Windows {
    // TYPE-11: "Its maximum and minimum have one constant kind: tick count
    // or duration"; hgraph's `TSW` takes a tick count and `TSWDuration`
    // microseconds.
    fn schema(&self, emitter: &mut Emitter<'_>, ty: &Type) -> Option<String> {
        let Window { element, bounds } = window(emitter, ty)?;
        Some(match bounds {
            Bounds::Ticks(most, least) => format!("::hgraph::TSW<{element}, {most}, {least}>"),
            Bounds::Duration(most, least) => {
                format!("::hgraph::TSWDuration<{element}, {most}, {least}>")
            }
        })
    }

    // TYPE-11: "A window's value, read in a hook or given to a lifted
    // helper (FN-12), is a `list<V>` of the values held, oldest first. A
    // read of it before the window is `all_valid` fails with
    // `value.unset_read`".
    fn read(&self, emitter: &mut Emitter<'_>, ty: &Type, view: &str) -> Option<String> {
        window(emitter, ty)?;
        Some(format!("{}({view})", library("window_value")))
    }

    // TIME-7: "A result of `rolling_window<V, Max, Min>` takes a V, which
    // arrives, and nothing else (TYPE-11)".
    fn publish(
        &self,
        emitter: &mut Emitter<'_>,
        ty: &Type,
        view: &str,
        value: &str,
    ) -> Option<String> {
        window(emitter, ty)?;
        // END-5: "Further writes to the already modified endpoint in the
        // same evaluation update its publication"; TYPE-11: "A window
        // takes at most one value a cycle". A node's `when` holds the
        // value, which arrives once its evaluation ends.
        if holding(emitter, view) {
            return Some(format!("{ARRIVAL}.hold({value});"));
        }
        Some(format!("{}({view}, {value});", library("window_push")))
    }

    // END-5, TYPE-11: the handlers of a node whose result is a window run
    // while the evaluation holds its one arrival, which is pushed when
    // they have run, a `return` among them.
    fn evaluation(&self, emitter: &mut Emitter<'_>, ty: &Type, handlers: &str) -> Option<String> {
        window(emitter, ty)?;
        let indent = INDENT.repeat(2);
        let inner: String = handlers
            .lines()
            .map(|line| {
                if line.is_empty() {
                    "\n".to_string()
                } else {
                    format!("{INDENT}{line}\n")
                }
            })
            .collect();
        Some(format!(
            "{indent}// The cycle's one arrival at the window (TYPE-11, END-5): each write holds\n\
             {indent}// its value in place of the one before, and the last arrives when the\n\
             {indent}// handlers have run.\n\
             {indent}::{}::WindowArrival<decltype({OUT})> {ARRIVAL}{{{OUT}}};\n\
             {indent}{ARRIVAL}.run([&] {{\n{inner}{indent}}});\n",
            runtime::NAMESPACE
        ))
    }

    // END-9: "`delta_value(x)` of a window is the value that arrived in
    // the cycle ... In a cycle in which a value only leaves a window of a
    // duration, the window ticks and has no delta": the library's
    // `endpoint_read` of a window says so, and ERR-14 has the read take
    // its form from its place.
    fn delta(
        &self,
        emitter: &mut Emitter<'_>,
        ty: &Type,
        view: &str,
        checked: bool,
    ) -> Option<String> {
        window(emitter, ty)?;
        let native = if checked {
            "fn_delta_value_checked"
        } else {
            "fn_delta_value"
        };
        Some(format!("{}({view})", library(native)))
    }

    // TST-16: a trace of a window "holds the window's arrivals"; TIME-7:
    // a result of one "takes a V".
    fn payload(&self, emitter: &mut Emitter<'_>, ty: &Type) -> Option<String> {
        window(emitter, ty)?;
        let Type::RollingWindow { value, .. } = ty else {
            return None;
        };
        emitter.named_type(value)
    }

    // TYPE-11: "A window's value, read in a hook or given to a lifted
    // helper (FN-12), is a `list<V>` of the values held".
    fn read_type(&self, emitter: &mut Emitter<'_>, ty: &Type) -> Option<Type> {
        window(emitter, ty)?;
        let Type::RollingWindow { value, .. } = ty else {
            return None;
        };
        Some(Type::List {
            element: value.clone(),
            size: None,
        })
    }

    // TST-5: a cell is "written as a `return` is": a window's is the V
    // that arrives.
    fn cell(
        &self,
        emitter: &mut Emitter<'_>,
        scope: &mut Scope<'_>,
        ty: &Type,
        value: &Expr,
    ) -> Option<String> {
        window(emitter, ty)?;
        Some(emitter.expression(scope, value))
    }

    fn expression(
        &self,
        emitter: &mut Emitter<'_>,
        scope: &mut Scope<'_>,
        value: &Expr,
    ) -> Option<String> {
        let locals = &scope.body.locals;
        // END-5: what a node's `when` writes of its window result is held
        // until its evaluation ends, so a read of that result there would
        // not see it.
        if own_read(emitter, locals, value) {
            return Some(emitter.missing(
                value.at,
                "a read of a node's own window result in its `when`",
            ));
        }
        match &value.kind {
            // TYPE-11: a read of a window input's value in a hook, as its
            // temporal form reads it. The read is typed as the `list<V>`
            // it gives.
            ExprKind::Local(local) if emitter.role(*local) == Role::Input => {
                let ty = window_type(emitter, locals, *local)?;
                scope.read = true;
                scope.acted = true;
                let view = emitter.view(locals, *local)?;
                Some(emitter.temporal_read(&ty, &view, value.at))
            }
            // END-9: "`all_valid(x)` where it holds at least its minimum".
            ExprKind::Endpoint {
                operation: EndpointOperation::AllValid,
                inputs: Some(inputs),
            } => {
                let [local] = inputs.as_slice() else {
                    return None;
                };
                let ty = window_type(emitter, locals, *local)?;
                if !reads_endpoints(emitter.kind()) {
                    return Some(emitter.missing(value.at, "`all_valid` here"));
                }
                if window(emitter, &ty).is_none() {
                    return Some(emitter.missing(value.at, &format!("a temporal value of `{ty}`")));
                }
                let view = emitter.view(locals, *local)?;
                Some(format!("{}({view})", library("fn_all_valid")))
            }
            // END-4 of a window result: hgraph's window has no
            // invalidation of its own, which the library does not stand
            // in for yet.
            ExprKind::Endpoint {
                operation: EndpointOperation::Invalidate,
                inputs: Some(inputs),
            } => {
                let [local] = inputs.as_slice() else {
                    return None;
                };
                window_type(emitter, locals, *local)?;
                Some(emitter.missing(value.at, "`invalidate` of a window"))
            }
            // TYPE-11, ERR-14: the read of a window's value where its place
            // expects `result<list<V>, error>`.
            ExprKind::CheckedWindow(read) => {
                let ExprKind::Local(local) = read.kind else {
                    return Some(
                        emitter.missing(value.at, "this read of a window into a `result`"),
                    );
                };
                let Some(ty) = window_type(emitter, locals, local) else {
                    return Some(
                        emitter.missing(value.at, "this read of a window into a `result`"),
                    );
                };
                if !reads_endpoints(emitter.kind()) || window(emitter, &ty).is_none() {
                    return Some(
                        emitter.missing(value.at, "this read of a window into a `result`"),
                    );
                }
                let view = emitter.view(locals, local)?;
                scope.read = true;
                scope.acted = true;
                Some(format!("{}({view})", library("window_value_checked")))
            }
            // END-9: "`removed_value(x)` ... is read only where its place
            // expects a `result<V, error>`, and gives `ok` of that value,
            // or an `err` coded `value.unset_read` where none left".
            ExprKind::CheckedDelta(read) => {
                let ExprKind::Call {
                    callee, arguments, ..
                } = &read.kind
                else {
                    return None;
                };
                let (module, declaration) = callee.declaration()?;
                let program = emitter.program;
                let declared = &program.modules[module].declarations[declaration];
                if Some(module) != program.core()
                    || declared.name != "removed_value"
                    || !matches!(declared.kind, DeclarationKind::NativeFunction(_))
                {
                    return None;
                }
                let [argument] = arguments.as_slice() else {
                    return Some(emitter.missing(value.at, "this `removed_value`"));
                };
                let ExprKind::EndpointOf(local) = argument.value.kind else {
                    return Some(emitter.missing(value.at, "this `removed_value`"));
                };
                let ty = window_type(emitter, locals, local);
                if !reads_endpoints(emitter.kind())
                    || ty.is_none_or(|ty| window(emitter, &ty).is_none())
                {
                    return Some(emitter.missing(value.at, "this `removed_value`"));
                }
                let view = emitter.view(locals, local)?;
                scope.acted = true;
                Some(format!("{}({view})", library("fn_removed_value_checked")))
            }
            _ => None,
        }
    }
}
