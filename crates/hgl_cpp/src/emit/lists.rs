//! The temporal form of a list and of a tuple: the family of the dispatch
//! point (`forms`) that writes a temporal value of `list<V>`, `list<V, N>`
//! and `tuple<A, B>`, a child of one read or wired, and what one publishes.
//! Part of the transpiler, the last stage.
//!
//! | HGL | C++ |
//! | --- | --- |
//! | A temporal `list<V>` (TYPE-9, TYPE-10) | hgraph's growing `TSL<S>`, S the temporal form of V |
//! | A temporal `list<V, N>` | `TSL<S, N>` |
//! | A temporal `tuple<A, B>` | `UnNamedTSB<Field<"_0", SA>, Field<"_1", SB>>`, a field for each position |
//! | A read of one in a hook (TIME-1); what a lifted helper is given (FN-12) | `::hglrt::checked_whole(view)`: a growing list needs `valid`, a gap read as TIME-1's table has it; a fixed list or a tuple needs `all_valid` |
//! | `return value`, `out = value` (TIME-7) | `::hglrt::publish(view, value);`, a whole value or a delta, as its C++ type says |
//! | `delta_value(view)` (END-3, TYPE-16) | `::hglrt::fn_delta_value(view)`, `fn_delta_value_checked` where its place takes a `result` (ERR-14), the delta read from hgraph's |
//! | `all_valid(view)` (TIME-9, END-7) | `::hglrt::fn_all_valid(view)` |
//! | `values[i]`, a child named in a hook (TIME-1, END-8) | `::hglrt::child(view, i)`, `::hglrt::position<0>(view)` for a tuple's: an endpoint whether or not it is valid, given to `valid`, `modified`, `delta_value`, `last_modified`, `all_valid` |
//! | `values[i]` read where it is checked when it runs (ERR-14) | `::hglrt::checked_whole(child)`, or `checked_whole_result` where its place takes a `result` |
//! | `pair[0]`, a child of a connection of a graph (TIME-9) | `::hglrt::element_port(port, 0)` |
//! | A cell of a trace of one (TST-5) | `::hglrt::testing::Cell<T>`, of a `Whole<T>` or of a delta |
//!
//! A part the library does not read and publish yet (`list_endpoints.h`),
//! a struct, a set, a dictionary, a window, is not this family's, and is
//! reported. FN-11 gives a lifted helper a fixed list or a tuple only
//! where it is `all_valid`, which the lifted call's node asks first
//! (`lifting`).

use hgl_analyser::hir::{EndpointOperation, Expr, ExprKind, Value};
use hgl_analyser::{ConstantValue, NamedKind, Type};

use super::Emitter;
use super::expressions::held_read;
use super::forms::Family;
use super::names;
use super::nodes::{Kind, Role};
use super::statements::Scope;
use crate::runtime;

/// The lists' and tuples' family.
pub(super) struct Lists;

/// What a type this family writes the temporal form of is.
enum Shape<'t> {
    /// `list<V>`, of its element.
    Growing(&'t Type),
    /// `list<V, N>`, of its element and size.
    Fixed(&'t Type, i64),
    /// `tuple<A, B>`, of its positions.
    Tuple(&'t [Type]),
}

/// The shape of `ty`, where it is a list or a tuple this family writes:
/// a fixed list of a positive size (TYPE-10 allows none of no position
/// that hgraph's `TSL` takes), and a tuple of a position or more.
fn shape(ty: &Type) -> Option<Shape<'_>> {
    match ty {
        Type::List {
            element,
            size: None,
        } => Some(Shape::Growing(element)),
        Type::List {
            element,
            size: Some(size),
        } => match size.value {
            ConstantValue::Int(count) if count > 0 => Some(Shape::Fixed(element, count)),
            _ => None,
        },
        Type::Tuple(positions) if !positions.is_empty() => Some(Shape::Tuple(positions)),
        _ => None,
    }
}

/// True where the library reads and publishes a part of type `ty` of a
/// list or a tuple (`list_endpoints.h`): a scalar or an enum, which
/// publishes whole, a nullable one, and a list or a tuple of such parts.
/// Its form is still the dispatch point's to give.
fn supported(ty: &Type) -> bool {
    match ty {
        Type::Scalar(_)
        | Type::Named {
            kind: NamedKind::Enum,
            ..
        } => true,
        Type::Nullable(inner) => matches!(
            **inner,
            Type::Scalar(_)
                | Type::Named {
                    kind: NamedKind::Enum,
                    ..
                }
        ),
        _ => match shape(ty) {
            Some(Shape::Growing(element) | Shape::Fixed(element, _)) => supported(element),
            Some(Shape::Tuple(positions)) => positions.iter().all(supported),
            None => false,
        },
    }
}

/// The library's function of the operation `operation` on an endpoint,
/// as `endpoints.h` names it, where it is one a child is given.
fn native(operation: EndpointOperation) -> Option<&'static str> {
    Some(match operation {
        EndpointOperation::Valid => "fn_valid",
        EndpointOperation::AllValid => "fn_all_valid",
        EndpointOperation::Modified => "fn_modified",
        EndpointOperation::DeltaValue => "fn_delta_value",
        EndpointOperation::LastModified => "fn_last_modified",
        _ => return None,
    })
}

impl Emitter<'_> {
    /// The endpoint of `child`, a child of a temporal input that is a
    /// list or a tuple, an element or a position at any depth (TIME-1: "A
    /// child of a collection ... is an endpoint whether or not it is
    /// valid"), as the library's `Child` of it: `None` where it is no
    /// child of an input of a form of this family's. A child of `out` is
    /// not written yet: a read of `out` is reported (`endpoints`).
    fn child_endpoint(&mut self, scope: &mut Scope, child: &Expr) -> Option<String> {
        let ExprKind::Index { base, index } = &child.kind else {
            return None;
        };
        let parent = match &base.kind {
            ExprKind::Local(local) if self.role(*local) == Role::Input => {
                self.view(&scope.body.locals, *local)?
            }
            ExprKind::Index { .. } => self.child_endpoint(scope, base)?,
            _ => return None,
        };
        let namespace = runtime::NAMESPACE;
        match shape(&base.ty)? {
            // EXPR-2: the parent is chosen before the next index is
            // evaluated, as C++ evaluates a call's arguments in no order it
            // promises: where either runs statements, or both may fail, as
            // a list's element read has it (`expressions::held_read`), the
            // parent is held first.
            Shape::Growing(_) | Shape::Fixed(..) if held_read(base, index) => {
                let held = names::temporary("parent", self.temporary());
                let at = names::temporary("index", self.temporary());
                let index = self.expression(scope, index);
                // An input's view, which hgraph does not copy, is named by
                // reference; a child of one is the value the library made.
                let kept = if matches!(base.kind, ExprKind::Local(_)) {
                    "const auto&"
                } else {
                    "const auto"
                };
                Some(format!(
                    "[&]() {{ {kept} {held} = {parent}; const ::hgraph::Int {at} = {index}; return ::{namespace}::child({held}, {at}); }}()"
                ))
            }
            Shape::Growing(_) | Shape::Fixed(..) => {
                let index = self.expression(scope, index);
                Some(format!("::{namespace}::child({parent}, {index})"))
            }
            // TYPE-10: a tuple's position is a constant within it.
            Shape::Tuple(_) => match index.kind {
                ExprKind::Literal(Value::Int(position)) => {
                    Some(format!("::{namespace}::position<{position}>({parent})"))
                }
                _ => Some(self.missing(index.at, "a tuple's position that is no literal")),
            },
        }
    }

    /// The type of the endpoint `local` of a hook: an input's own, or the
    /// node's result for `out`.
    fn endpoint_type(&self, scope: &Scope, local: usize) -> Option<Type> {
        match self.role(local) {
            Role::Input => Some(scope.body.locals[local].ty.clone()),
            Role::Out => self
                .temporal
                .as_ref()
                .and_then(|context| context.result.clone()),
            _ => None,
        }
    }
}

impl Family for Lists {
    // TYPE-9: "Temporalization recursively maps tuples/structs to child
    // fields, lists to child positions". TYPE-10: "`list<T>` denotes a
    // growing shape; fixed sizes are nonnegative integer constants ... A
    // tuple retains positional identity and is not a fixed list alias":
    // hgraph's `TSL` of a child for each position, and its bundle with no
    // name of a field for each position, each of a form of its own.
    fn schema(&self, emitter: &mut Emitter<'_>, ty: &Type) -> Option<String> {
        if !supported(ty) {
            return None;
        }
        match shape(ty)? {
            Shape::Growing(element) => {
                let child = emitter.part_schema(element)?;
                Some(format!("::hgraph::TSL<{child}>"))
            }
            Shape::Fixed(element, size) => {
                let child = emitter.part_schema(element)?;
                Some(format!("::hgraph::TSL<{child}, {size}>"))
            }
            Shape::Tuple(positions) => {
                let mut fields = Vec::new();
                for (index, position) in positions.iter().enumerate() {
                    let child = emitter.part_schema(position)?;
                    fields.push(format!("::hgraph::Field<\"_{index}\", {child}>"));
                }
                Some(format!("::hgraph::UnNamedTSB<{}>", fields.join(", ")))
            }
        }
    }

    // TIME-1: "A read of a struct, a tuple or a fixed list whole ...
    // requires `all_valid` of it ... A dictionary and an unsized list hold
    // what they hold: a read of one whole requires its `valid` and no
    // more", checked when it runs.
    fn read(&self, _emitter: &mut Emitter<'_>, ty: &Type, view: &str) -> Option<String> {
        if !supported(ty) {
            return None;
        }
        shape(ty)?;
        Some(format!("::{}::checked_whole({view})", runtime::NAMESPACE))
    }

    // TIME-7: "`return value` and `out = value` take a complete value of
    // the result's type T, published whole, or a `delta<T>` (TYPE-16),
    // published as that change": the library tells the two by their C++
    // types.
    fn publish(
        &self,
        _emitter: &mut Emitter<'_>,
        ty: &Type,
        view: &str,
        value: &str,
    ) -> Option<String> {
        if !supported(ty) {
            return None;
        }
        shape(ty)?;
        Some(format!(
            "::{}::publish({view}, {value});",
            runtime::NAMESPACE
        ))
    }

    // END-3: "`delta_value(x)` gives what `x` published in the current
    // cycle"; its form taken from its place (ERR-14).
    fn delta(
        &self,
        _emitter: &mut Emitter<'_>,
        ty: &Type,
        view: &str,
        checked: bool,
    ) -> Option<String> {
        if !supported(ty) {
            return None;
        }
        shape(ty)?;
        let native = if checked {
            "fn_delta_value_checked"
        } else {
            "fn_delta_value"
        };
        Some(format!("::{}::{native}({view})", runtime::NAMESPACE))
    }

    // TST-5: a cell is "written as a `return` is (TIME-7) ... a T,
    // published whole, or a `delta<T>`", the one compared with the value
    // the result holds once its cycle has published, the other with the
    // delta it published.
    fn cell(
        &self,
        emitter: &mut Emitter<'_>,
        scope: &mut Scope<'_>,
        ty: &Type,
        value: &Expr,
    ) -> Option<String> {
        if !supported(ty) {
            return None;
        }
        shape(ty)?;
        let Some(written) = emitter.written_type(ty, value.at) else {
            return Some("0".to_string());
        };
        let cell = emitter.expression(scope, value);
        let library = super::names::testing("");
        let library = library.trim_end_matches("::");
        if value.ty == *ty {
            Some(format!(
                "{library}::Cell<{written}>{{{library}::Whole<{written}>{{{cell}}}}}"
            ))
        } else {
            Some(format!(
                "{library}::Cell<{written}>{{std::in_place_index<0>, {cell}}}"
            ))
        }
    }

    fn expression(
        &self,
        emitter: &mut Emitter<'_>,
        scope: &mut Scope<'_>,
        value: &Expr,
    ) -> Option<String> {
        if !matches!(emitter.kind(), Some(Kind::Hook(_) | Kind::Lifted)) {
            return None;
        }
        let namespace = runtime::NAMESPACE;
        match &value.kind {
            // END-7: `all_valid` of an input or of `out` of this family's
            // form.
            ExprKind::Endpoint {
                operation: EndpointOperation::AllValid,
                inputs: Some(inputs),
            } if inputs.len() == 1 => {
                let ty = emitter.endpoint_type(scope, inputs[0])?;
                shape(&ty)?;
                let view = emitter.view(&scope.body.locals, inputs[0])?;
                Some(format!("::{namespace}::fn_all_valid({view})"))
            }
            // TIME-9: an operation on a child, `valid(values[2])`.
            ExprKind::ChildEndpoint { operation, child } => {
                let endpoint = emitter.child_endpoint(scope, child)?;
                scope.read = true;
                scope.acted = true;
                match native(*operation) {
                    Some(native) => Some(format!("::{namespace}::{native}({endpoint})")),
                    None => Some(
                        emitter.missing(value.at, &format!("`{}` of a child", operation.name())),
                    ),
                }
            }
            // TIME-9: a child given to an `endpoint<T>` parameter.
            ExprKind::ChildEndpointOf(child) => {
                let endpoint = emitter.child_endpoint(scope, child)?;
                scope.acted = true;
                Some(endpoint)
            }
            // ERR-14: "a child that is not valid fails the read with
            // `value.unset_read`. Where `result` is true the place expects
            // a `result<T, error>`".
            ExprKind::CheckedChild { read, result } => {
                let endpoint = emitter.child_endpoint(scope, read)?;
                scope.read = true;
                scope.acted = true;
                let native = if *result {
                    "checked_whole_result"
                } else {
                    "checked_whole"
                };
                Some(format!("::{namespace}::{native}({endpoint})"))
            }
            // ERR-14: `delta_value` of a child where its place takes a
            // `result`.
            ExprKind::CheckedDelta(read) => match &read.kind {
                ExprKind::ChildEndpoint {
                    operation: EndpointOperation::DeltaValue,
                    child,
                } => {
                    let endpoint = emitter.child_endpoint(scope, child)?;
                    scope.acted = true;
                    Some(format!("::{namespace}::fn_delta_value_checked({endpoint})"))
                }
                _ => None,
            },
            // TIME-1: a child read at a constant index, which the analyser
            // has proved admitted, read as its form reads it, and checked
            // again when it runs.
            ExprKind::Index { .. } => {
                let endpoint = emitter.child_endpoint(scope, value)?;
                scope.read = true;
                scope.acted = true;
                Some(format!("::{namespace}::checked_whole({endpoint})"))
            }
            _ => None,
        }
    }

    // TIME-9: "a child of one, `parts[1]` ... is given ... as a
    // connection", a connection of its own of the element's form.
    fn wiring(
        &self,
        emitter: &mut Emitter<'_>,
        scope: &mut Scope<'_>,
        value: &Expr,
    ) -> Option<String> {
        let ExprKind::Index { base, index } = &value.kind else {
            return None;
        };
        if !base.is_connection() || !supported(&base.ty) {
            return None;
        }
        if !matches!(shape(&base.ty)?, Shape::Growing(_) | Shape::Fixed(..)) {
            return None;
        }
        if index.is_connection() {
            return Some(emitter.missing(index.at, "a child of a connection at a connection"));
        }
        let port = emitter.wiring_form(scope, base);
        // EXPR-2: the connection is wired before its index is evaluated:
        // where either runs statements, or both may fail
        // (`expressions::held_read`), the port is held first, as C++
        // evaluates a call's arguments in no order it promises.
        if held_read(base, index) {
            let held = names::temporary("list", emitter.temporary());
            let at = names::temporary("index", emitter.temporary());
            let index = emitter.expression(scope, index);
            return Some(format!(
                "[&]() {{ const auto {held} = {port}; const ::hgraph::Int {at} = {index}; return ::{}::element_port({held}, {at}); }}()",
                runtime::NAMESPACE
            ));
        }
        let index = emitter.expression(scope, index);
        Some(format!(
            "::{}::element_port({port}, {index})",
            runtime::NAMESPACE
        ))
    }
}
