//! The temporal form of a list: the family of the dispatch point
//! (`forms`) that writes a temporal value of `list<V>` and `list<V, N>`, a
//! child of one read or wired, and what one publishes. Part of the
//! transpiler, the last stage. A tuple is the `structs` family's.
//!
//! | HGL | C++ |
//! | --- | --- |
//! | A temporal `list<V>` (TYPE-9, TYPE-10) | hgraph's growing `TSL<S>`, S the temporal form of V |
//! | A temporal `list<V, N>` | `TSL<S, N>` |
//! | A read of one in a hook (TIME-1); what a lifted helper is given (FN-12) | `::hglrt::checked_whole(view)`: a growing list needs `valid`, a gap read as TIME-1's table has it; a fixed list needs `all_valid` |
//! | `return value`, `out = value` (TIME-7) | `::hglrt::publish(view, value);`, a whole value or a delta, as its C++ type says |
//! | `delta_value(view)` (END-3, TYPE-16) | `::hglrt::fn_delta_value(view)`, `fn_delta_value_checked` where its place takes a `result` (ERR-14), the delta read from hgraph's |
//! | `all_valid(view)` (TIME-9, END-7) | `::hglrt::fn_all_valid(view)` |
//! | `values[i]`, a child named in a hook (TIME-1, END-8) | `::hglrt::child(view, i)`: an endpoint whether or not it is valid, given to `valid`, `modified`, `delta_value`, `last_modified`, `all_valid` |
//! | `values[i]` read where it is checked when it runs (ERR-14) | `::hglrt::checked_whole(child)`, or `checked_whole_result` where its place takes a `result` |
//! | `pair[0]`, a child of a connection of a graph (TIME-9) | `::hglrt::element_port(port, 0)` |
//! | A cell of a trace of one (TST-5) | `::hglrt::testing::Cell<T>`, of a `Whole<T>` or of a delta |
//!
//! A part is read and published through its own form (TYPE-9): a scalar,
//! a nullable, a list, and a struct or a tuple, the `structs` family's. A
//! part the library does not read and publish as a part yet, a set, a
//! dictionary, a window, an atomic struct, is not this family's, and is
//! reported. FN-11 gives a lifted helper a fixed list only where it
//! is `all_valid`, which the lifted call's node asks first (`lifting`).

use hgl_analyser::hir::{EndpointOperation, Expr, ExprKind};
use hgl_analyser::{ConstantValue, NamedKind, Type};

use super::Emitter;
use super::forms::Family;
use super::graphs::WIRING;
use super::names;
use super::nodes::{Kind, Role};
use super::statements::Scope;
use crate::runtime;

/// The lists' family.
pub(super) struct Lists;

/// What a type this family writes the temporal form of is.
enum Shape<'t> {
    /// `list<V>`, of its element.
    Growing(&'t Type),
    /// `list<V, N>`, of its element and size.
    Fixed(&'t Type, i64),
}

/// Keep the fixed container conversion guarded even when the analyser
/// could not compare the extents. The loop itself always visits its source.
fn check_extent(items: &str, count: i64) -> String {
    format!(
        "if ({items}.size() != ::std::size_t{{{count}}}) throw ::std::logic_error(\"list comprehension extent does not match its result type\");\n"
    )
}

/// The shape of `ty`, where it is a list this family writes: a fixed list
/// of a positive size (TYPE-10 allows none of no position that hgraph's
/// `TSL` takes).
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
        _ => None,
    }
}

/// True where the library reads and publishes a part of type `ty` of a
/// list through the part's own form (TYPE-9; endpoints.h, `whole` and
/// `publisher`): a scalar or an enum, which publishes whole, a nullable
/// one, a list of such parts, and a struct or a tuple whose form the
/// `structs` family writes, a bundle (structs.h). The part's form is the
/// dispatch point's to give, asked of its own family.
fn supported(emitter: &mut Emitter, ty: &Type) -> bool {
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
        Type::Tuple(_)
        | Type::Named {
            kind: NamedKind::Struct,
            ..
        } => emitter
            .temporal_schema_of(ty)
            .is_some_and(|schema| bundle(&schema)),
        _ => match shape(ty) {
            Some(Shape::Growing(element) | Shape::Fixed(element, _)) => supported(emitter, element),
            None => false,
        },
    }
}

/// True where `schema` is hgraph's bundle, a struct's or a tuple's form
/// (`structs`): `NativeTSB`, `NominalTSB` or `UnNamedTSB`.
fn bundle(schema: &str) -> bool {
    [
        "::hgraph::NativeTSB<",
        "::hgraph::NominalTSB<",
        "::hgraph::UnNamedTSB<",
    ]
    .iter()
    .any(|form| schema.starts_with(form))
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
    /// Write the evaluation of an ordinary list comprehension (EXPR-17).
    /// Collect its entries before making a fixed array so even its
    /// elements with no default construction can be built in order.
    pub(super) fn list_comprehension(
        &mut self,
        scope: &mut Scope,
        value: &Expr,
        (binding, sequence, entry): (usize, &Expr, &Expr),
    ) -> String {
        let Type::List { element, size } = &value.ty else {
            return self.missing(value.at, "this list comprehension");
        };
        let Some(result) = self.written_type(&value.ty, value.at) else {
            return "0".to_string();
        };
        let fixed = if let Some(size) = size {
            let ConstantValue::Int(count) = size.value else {
                return self.missing(
                    value.at,
                    "a list comprehension with an unresolved fixed size",
                );
            };
            let Some(element) = self.written_type(element, value.at) else {
                return "0".to_string();
            };
            Some((count, element))
        } else {
            None
        };
        let collection = fixed.as_ref().map_or_else(
            || result.clone(),
            |(_, element)| format!("::std::vector<{element}>"),
        );
        let items = names::temporary("comprehension_items", self.temporary());
        let name = names::identifier(&scope.body.locals[binding].name);
        let saved = std::mem::take(&mut scope.before);
        scope.conditional += 1;
        let header = if let ExprKind::Range { lower, upper } = &sequence.kind {
            self.range_header(scope, lower, upper, &name)
        } else {
            let source = self.expression(scope, sequence);
            let source_value = names::temporary("comprehension_source", self.temporary());
            scope
                .before
                .push(format!("const auto {source_value} = {source};"));
            format!("for ([[maybe_unused]] const auto& {name} : {source_value}) {{")
        };
        let begin = std::mem::take(&mut scope.before).join("\n");
        let entry = self.expression(scope, entry);
        let before_entry = std::mem::replace(&mut scope.before, saved).join("\n");
        scope.conditional -= 1;
        scope.acted = true;
        let (check, output) = if let Some((count, _)) = fixed {
            let check = check_extent(&items, count);
            let output = format!(
                "[&]<::std::size_t... I>(::std::index_sequence<I...>) {{ return {result}{{::std::move({items}[I])...}}; }}(::std::make_index_sequence<{count}>{{}})"
            );
            (check, output)
        } else {
            (String::new(), items.clone())
        };
        format!(
            "[&]() -> {result} {{\n{begin}\n{collection} {items};\n{header}\n{before_entry}\n{items}.emplace_back({entry});\n}}\n{check}return {output};\n}}()"
        )
    }

    /// Compose one connection per pass and combine them into the fixed
    /// list's structural port (EXPR-17). A range carries ordinary values
    /// and a fixed input list carries child connections.
    fn list_comprehension_port(
        &mut self,
        scope: &mut Scope,
        value: &Expr,
        (binding, sequence, entry): (usize, &Expr, &Expr),
    ) -> String {
        let Some(Shape::Fixed(element, count)) = shape(&value.ty) else {
            return self.missing(
                value.at,
                "a composed list comprehension without a supported fixed size",
            );
        };
        let Some(schema) = self.temporal_schema(&value.ty, value.at) else {
            return "0".to_string();
        };
        let input_count = match &sequence.kind {
            ExprKind::Range { .. } => count,
            _ => match shape(&sequence.ty) {
                Some(Shape::Fixed(_, count)) => count,
                _ => {
                    return self.missing(
                        sequence.at,
                        "a comprehension over a list without a fixed size",
                    );
                }
            },
        };
        let name = names::identifier(&scope.body.locals[binding].name);
        let children = names::temporary("comprehension_children", self.temporary());
        let cursor = names::temporary("comprehension_position", self.temporary());
        let saved = std::mem::take(&mut scope.before);
        scope.conditional += 1;
        let header = if let ExprKind::Range { lower, upper } = &sequence.kind {
            self.range_header(scope, lower, upper, &name)
        } else {
            let source = self.wiring_form(scope, sequence);
            let source_port = names::temporary("comprehension_source", self.temporary());
            scope
                .before
                .push(format!("const auto {source_port} = {source};"));
            format!(
                "for (::hgraph::Int {cursor} = 0; {cursor} < {input_count}; ++{cursor}) {{ [[maybe_unused]] const auto {name} = ::{}::element_port({source_port}, {cursor});",
                runtime::NAMESPACE
            )
        };
        let begin = std::mem::take(&mut scope.before).join("\n");
        let entry = self.connection(scope, entry, element);
        let before_entry = std::mem::replace(&mut scope.before, saved).join("\n");
        scope.conditional -= 1;
        scope.acted = true;
        let check = check_extent(&children, count);
        format!(
            "[&]() -> ::hgraph::Port<{schema}> {{\n{begin}\n::std::vector<::hgraph::WiringPortRef> {children};\n{children}.reserve({count});\n{header}\n{before_entry}\n{children}.push_back(({entry}).erased());\n}}\n{check}return {{&{WIRING}, ::hgraph::WiringPortRef::structural_source(::hgraph::schema_descriptor<{schema}>::ts_meta(), ::std::move({children}))}};\n}}()"
        )
    }

    /// The endpoint of `child`, a child of a temporal input that is a
    /// list, an element at any depth (TIME-1: "A
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
        let shape = shape(&base.ty)?;
        match shape {
            // EXPR-2: the parent is chosen before the next index is
            // evaluated, as C++ evaluates a call's arguments in no order it
            // promises: where either runs statements, or both may fail, as
            // a list's element read has it (`expressions::held_read`), the
            // parent is held first.
            Shape::Growing(_) | Shape::Fixed(..) if self.held_read(base, index) => {
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
    // hgraph's `TSL` of a child for each position, each of a form of its
    // own; a tuple's bundle is the `structs` family's.
    fn schema(&self, emitter: &mut Emitter<'_>, ty: &Type) -> Option<String> {
        // A list's form alone: a part's is asked of the dispatch point,
        // which asks this family again of no other type (`supported`).
        shape(ty)?;
        if !supported(emitter, ty) {
            return None;
        }
        match shape(ty)? {
            Shape::Growing(element) => {
                let child = emitter.temporal_schema_of(element)?;
                Some(format!("::hgraph::TSL<{child}>"))
            }
            Shape::Fixed(element, size) => {
                let child = emitter.temporal_schema_of(element)?;
                Some(format!("::hgraph::TSL<{child}, {size}>"))
            }
        }
    }

    // TIME-1: "A read of a struct, a tuple or a fixed list whole ...
    // requires `all_valid` of it ... A dictionary and an unsized list hold
    // what they hold: a read of one whole requires its `valid` and no
    // more", checked when it runs.
    fn read(&self, emitter: &mut Emitter<'_>, ty: &Type, view: &str) -> Option<String> {
        // A list's form alone: a part's is asked of the dispatch point,
        // which asks this family again of no other type (`supported`).
        shape(ty)?;
        if !supported(emitter, ty) {
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
        emitter: &mut Emitter<'_>,
        ty: &Type,
        view: &str,
        value: &str,
    ) -> Option<String> {
        // A list's form alone: a part's is asked of the dispatch point,
        // which asks this family again of no other type (`supported`).
        shape(ty)?;
        if !supported(emitter, ty) {
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
        emitter: &mut Emitter<'_>,
        ty: &Type,
        view: &str,
        checked: bool,
    ) -> Option<String> {
        // A list's form alone: a part's is asked of the dispatch point,
        // which asks this family again of no other type (`supported`).
        shape(ty)?;
        if !supported(emitter, ty) {
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
        // A list's form alone: a part's is asked of the dispatch point,
        // which asks this family again of no other type (`supported`).
        shape(ty)?;
        if !supported(emitter, ty) {
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
                "{library}::Cell<{written}>{{::std::in_place_index<0>, {cell}}}"
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
        if let ExprKind::ListComprehension {
            composition: true,
            binding,
            sequence,
            entry,
        } = &value.kind
        {
            return Some(emitter.list_comprehension_port(
                scope,
                value,
                (*binding, sequence, entry),
            ));
        }
        let ExprKind::Index { base, index } = &value.kind else {
            return None;
        };
        if !base.is_connection() || shape(&base.ty).is_none() || !supported(emitter, &base.ty) {
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
        if emitter.held_read(base, index) {
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
