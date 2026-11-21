//! The temporal form of a set and of a dictionary: the family of the
//! dispatch point (`forms`) that writes a temporal value of `set<K>` and
//! of `dict<K, V>` (TYPE-9: "dictionaries to keyed children, and sets to
//! membership"). Part of the transpiler, the last stage.
//!
//! | HGL | C++ |
//! | --- | --- |
//! | A temporal value of `set<K>`, of `dict<K, V>` | hgraph's `TSS<K>`, `TSD<K, V'>`, `V'` the form of `V` |
//! | A read of one whole in a hook (TIME-1) | `::hglrt::keyed_value(view)`: an `::hglrt::set` or `::hglrt::dict`, which fails with `value.unset_read` where it is not valid |
//! | `return value`, `out = value`, of a whole value or a delta (TIME-7) | `::hglrt::keyed_publish(view, value);`: hgraph's whole write of the one, its write of a delta of the other |
//! | `delta_value(view)` (END-3, TYPE-16, ERR-14) | `::hglrt::fn_delta_value(view)`, or `fn_delta_value_checked`: an `::hglrt::set_delta`, `::hglrt::dict_delta` |
//! | `all_valid(book)` (TIME-9) | `::hglrt::fn_all_valid(book)` |
//! | `book[key]` read in a hook, as a value or into a `result` (TIME-1, EXPR-7, ERR-14) | `::hglrt::keyed_entry_value(book, key)`, `keyed_entry_result`: an entry the dictionary does not hold, or that is not valid, fails with `value.unset_read` or gives an `err` of it |
//! | `valid(book[key])`, `modified`, `last_modified`, `delta_value` of an entry (TIME-9) | `::hglrt::keyed_entry_valid(book, key)` and the like: an entry the dictionary does not hold is not valid |
//! | `invalidate(out)` (END-4) | the endpoints' `::hglrt::fn_invalidate(out_)`, which the library writes for these forms |
//! | A cell of a trace of one (TST-5) | `::hglrt::testing::Whole<T>{value}` for a value written whole; the delta itself for a delta |
//!
//! A set's element and a dictionary's key are a scalar, which hgraph
//! holds as a value of its own; a dictionary's value is a scalar, a set,
//! a dictionary or a tuple of such positions, whose form the library
//! converts, a tuple's an entry read whole as a tuple is (TIME-1). Every
//! other one, a constant connection of a set or a dictionary (FN-3) and an
//! entry of an entry (`book["a"]["b"]`), are not written yet, and are
//! reported. A key's child as a connection of a graph (EXPR-7) is the
//! `references` family's, a designation of the entry.

use hgl_analyser::hir::{EndpointOperation, Expr, ExprKind};
use hgl_analyser::{Scalar, Type};

use super::Emitter;
use super::forms::Family;
use super::nodes::{Kind, Role};
use super::statements::Scope;
use super::types::cpp_type;
use crate::runtime;

/// The family of sets and dictionaries.
pub(super) struct Keyed;

/// The C++ type of `ty` where it is a set's element or a dictionary's key
/// this family writes: a scalar, but `timezone`, `zoned_time` and `any`
/// (TYPE-14: "A set's element and a dictionary's key are never null"),
/// which hgraph holds as a value of its own.
fn key(emitter: &Emitter, ty: &Type) -> Option<String> {
    match ty {
        Type::Scalar(Scalar::Timezone | Scalar::ZonedTime | Scalar::Any) => None,
        Type::Scalar(_) => cpp_type(emitter.program, ty),
        _ => None,
    }
}

/// True where `ty` is a dictionary's value whose form the library
/// converts (`keyed_form`): a scalar's, a set's, a dictionary's, and a
/// tuple's of such positions, whose form is the `structs` family's (the
/// dispatch point gives it).
fn entry_form(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Scalar(scalar) if !matches!(scalar, Scalar::Timezone | Scalar::ZonedTime | Scalar::Any)
    ) || matches!(ty, Type::Set(_) | Type::Dict { .. })
        || matches!(ty, Type::Tuple(positions) if !positions.is_empty() && positions.iter().all(entry_form))
}

/// hgraph's schema of a temporal value of `ty`, where it is a set or a
/// dictionary this family writes.
fn schema(emitter: &mut Emitter, ty: &Type) -> Option<String> {
    match ty {
        Type::Set(element) => Some(format!("::hgraph::TSS<{}>", key(emitter, element)?)),
        Type::Dict { key: keys, value } if entry_form(value) => {
            let keys = key(emitter, keys)?;
            let value = emitter.temporal_schema_of(value)?;
            Some(format!("::hgraph::TSD<{keys}, {value}>"))
        }
        _ => None,
    }
}

impl Family for Keyed {
    fn schema(&self, emitter: &mut Emitter<'_>, ty: &Type) -> Option<String> {
        schema(emitter, ty)
    }

    // TIME-1: "A dictionary and an unsized list hold what they hold: a
    // read of one whole requires its `valid` and no more", as a set's does.
    fn read(&self, emitter: &mut Emitter<'_>, ty: &Type, view: &str) -> Option<String> {
        schema(emitter, ty)?;
        Some(format!("::{}::keyed_value({view})", runtime::NAMESPACE))
    }

    // TIME-7: "a complete value of the result's type T, published whole,
    // or a `delta<T>` (TYPE-16), published as that change": the library
    // tells the two apart by their C++ types.
    fn publish(
        &self,
        emitter: &mut Emitter<'_>,
        ty: &Type,
        view: &str,
        value: &str,
    ) -> Option<String> {
        schema(emitter, ty)?;
        Some(format!(
            "::{}::keyed_publish({view}, {value});",
            runtime::NAMESPACE
        ))
    }

    // END-3, TYPE-16: a set's delta "with the properties `added` and
    // `removed`", a dictionary's with `updated` and `removed`; ERR-14 has
    // the read take its form from its place.
    fn delta(
        &self,
        emitter: &mut Emitter<'_>,
        ty: &Type,
        view: &str,
        checked: bool,
    ) -> Option<String> {
        schema(emitter, ty)?;
        let native = if checked {
            "fn_delta_value_checked"
        } else {
            "fn_delta_value"
        };
        Some(format!("::{}::{native}({view})", runtime::NAMESPACE))
    }

    // TST-5: a cell is "written as a `return` is": a set or a dictionary
    // whole, or a delta of one.
    fn cell(
        &self,
        emitter: &mut Emitter<'_>,
        scope: &mut Scope<'_>,
        ty: &Type,
        value: &Expr,
    ) -> Option<String> {
        schema(emitter, ty)?;
        let written = emitter.expression(scope, value);
        if value.ty != *ty {
            return Some(written);
        }
        let whole = cpp_type(emitter.program, ty)?;
        Some(format!(
            "{}<{whole}>{{{written}}}",
            super::names::testing("Whole")
        ))
    }

    // TIME-9: "`all_valid` is true where every one is, by its own
    // `valid`, one level deep": of a dictionary in a hook. TIME-1: an
    // entry of a dictionary input, `book[key]`, is an endpoint whether or
    // not it is valid, read checked when it runs (EXPR-7).
    fn expression(
        &self,
        emitter: &mut Emitter<'_>,
        scope: &mut Scope<'_>,
        value: &Expr,
    ) -> Option<String> {
        if !matches!(emitter.kind(), Some(Kind::Hook(_))) {
            return None;
        }
        let library = runtime::NAMESPACE;
        match &value.kind {
            ExprKind::Endpoint {
                operation: EndpointOperation::AllValid,
                inputs: Some(inputs),
            } => {
                let [local] = inputs.as_slice() else {
                    return None;
                };
                let ty = match emitter.role(*local) {
                    Role::Out => emitter.temporal.as_ref()?.result.clone()?,
                    Role::Input => scope.body.locals[*local].ty.clone(),
                    _ => return None,
                };
                if !matches!(ty, Type::Dict { .. }) || schema(emitter, &ty).is_none() {
                    return None;
                }
                let view = emitter.view(&scope.body.locals, *local)?;
                Some(format!("::{library}::fn_all_valid({view})"))
            }
            // END-1 to END-3, END-6 of an entry (TIME-9).
            ExprKind::ChildEndpoint { operation, child } => {
                let (view, key) = match entry_of(emitter, scope, child)? {
                    Ok(entry) => entry,
                    Err(reported) => return Some(reported),
                };
                let native = match operation {
                    EndpointOperation::Valid => "keyed_entry_valid",
                    EndpointOperation::Modified => "keyed_entry_modified",
                    EndpointOperation::LastModified => "keyed_entry_last_modified",
                    EndpointOperation::DeltaValue => "keyed_entry_delta",
                    _ => {
                        return Some(emitter.missing(
                            value.at,
                            &format!("`{}` of a dictionary's entry", operation.name()),
                        ));
                    }
                };
                scope.acted = true;
                Some(format!("::{library}::{native}({view}, {key})"))
            }
            // ERR-14: the delta of an entry where its place takes a
            // `result`.
            ExprKind::CheckedDelta(read) => {
                let ExprKind::ChildEndpoint {
                    operation: EndpointOperation::DeltaValue,
                    child,
                } = &read.kind
                else {
                    return None;
                };
                let (view, key) = match entry_of(emitter, scope, child)? {
                    Ok(entry) => entry,
                    Err(reported) => return Some(reported),
                };
                scope.acted = true;
                Some(format!(
                    "::{library}::keyed_entry_delta_checked({view}, {key})"
                ))
            }
            // TIME-1: "a child that is not valid fails it
            // (`value.unset_read`), or gives an `err` where its place takes
            // a `result`": a read named when compiled is checked too.
            ExprKind::CheckedChild { read, result } => {
                let (view, key) = match entry_of(emitter, scope, read)? {
                    Ok(entry) => entry,
                    Err(reported) => return Some(reported),
                };
                scope.read = true;
                scope.acted = true;
                let native = if *result {
                    "keyed_entry_result"
                } else {
                    "keyed_entry_value"
                };
                Some(format!("::{library}::{native}({view}, {key})"))
            }
            ExprKind::Index { .. } => {
                let (view, key) = match entry_of(emitter, scope, value)? {
                    Ok(entry) => entry,
                    Err(reported) => return Some(reported),
                };
                scope.read = true;
                scope.acted = true;
                Some(format!("::{library}::keyed_entry_value({view}, {key})"))
            }
            _ => None,
        }
    }
}

/// The view of the dictionary input and the C++ of the key, where `read`
/// is the entry `book[key]` of a temporal input of a dictionary this
/// family writes; `None` where it is not. An entry of an entry,
/// `book[a][b]`, is reported, and its stand-in given as the error.
fn entry_of(
    emitter: &mut Emitter,
    scope: &mut Scope,
    read: &Expr,
) -> Option<Result<(String, String), String>> {
    let ExprKind::Index { base, index } = &read.kind else {
        return None;
    };
    let (local, nested) = root(base)?;
    if emitter.role(local) != Role::Input {
        return None;
    }
    let declared = &scope.body.locals[local].ty;
    if !matches!(declared, Type::Dict { .. }) || schema(emitter, declared).is_none() {
        return None;
    }
    if nested {
        return Some(Err(
            emitter.missing(read.at, "an entry of a dictionary's entry")
        ));
    }
    let view = emitter.view(&scope.body.locals, local)?;
    let key = emitter.expression(scope, index);
    Some(Ok((view, key)))
}

/// The local a chain of indices is rooted at, and whether `base` is
/// itself an index of it.
fn root(base: &Expr) -> Option<(usize, bool)> {
    match &base.kind {
        ExprKind::Local(local) => Some((*local, false)),
        ExprKind::Index { base, .. } => root(base).map(|(local, _)| (local, true)),
        _ => None,
    }
}
