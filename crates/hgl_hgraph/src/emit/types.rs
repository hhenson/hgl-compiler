//! What the hgraph target asks of a type: a struct's fields, whether
//! a value of it is published whole or as a delta, what a delta is the
//! delta of, and what of it hgraph holds as a window. Part of the
//! transpiler, the last stage: `emit` writes a module with the answers.

use super::*;

/// The provider's function that reads an element of a value of type
/// `ty` in brackets: `item` for a list and a tuple, by position, and
/// `entry` for a dictionary, by key. `None` for a type brackets do not
/// read.
pub(super) fn element_read(ty: &Type) -> Option<&'static str> {
    match ty {
        Type::List { .. } | Type::Tuple(_) => Some("item"),
        Type::Dict { .. } => Some("entry"),
        _ => None,
    }
}

/// True for a type of configuration a node is given inside an object
/// hgraph keeps as it is: a list, a tuple or a dictionary, nullable or
/// not. hgraph makes a value of its own of a node's configuration, and
/// has none for some of what one of these may hold, a set's delta say.
pub(super) fn held(ty: &Type) -> bool {
    match ty {
        Type::Nullable(inner) => held(inner),
        Type::List { .. } | Type::Tuple(_) | Type::Dict { .. } => true,
        _ => false,
    }
}

/// True for a struct whose value `port_value` makes anew from its
/// bundle, every part of it: each field is of a type nothing changes
/// where it is, or such a struct itself. Any other type is not.
pub(super) fn built_anew(program: &Program, ty: &Type, depth: usize) -> bool {
    let Type::Named {
        module,
        declaration,
        kind: NamedKind::Struct,
        ..
    } = ty
    else {
        return false;
    };
    let Some(Detail::Struct(read)) = program.modules[*module].details.get(*declaration) else {
        return false;
    };
    depth < 8
        && read.generics.is_empty()
        && !inherited(program, *module, *declaration)
        && read.members.iter().all(|member| {
            !changed_in_place(program, &member.ty, 0) || built_anew(program, &member.ty, depth + 1)
        })
}

/// What a delta is the delta of, where the delta is a type of its own
/// (TYPE-16, `Type::delta`). Each kind has properties of its own,
/// which `Emitter::delta` makes a delta of and a read of the delta's
/// field reads.
#[derive(Clone, Copy)]
pub(super) enum DeltaOf<'t> {
    /// A nullable's: `value`, null for a published null.
    Nullable,
    /// A struct's, of this struct type: a property for each field.
    Struct(&'t Type),
    /// A dictionary's: `updated` and `removed`.
    Dict,
    /// A list's, fixed or unsized: `updated`, `removed` and
    /// `size_modified`.
    List,
    /// A set's, of this element type: `added` and `removed`.
    Set(&'t Type),
}

impl<'t> DeltaOf<'t> {
    /// What `ty` is the delta of. `None` for a type that is no
    /// `delta<T>` of these.
    pub(super) fn of(ty: &'t Type) -> Option<DeltaOf<'t>> {
        let Type::Delta(of) = ty else {
            return None;
        };
        match &**of {
            Type::Nullable(_) => Some(DeltaOf::Nullable),
            Type::Named {
                kind: NamedKind::Struct,
                ..
            } => Some(DeltaOf::Struct(of)),
            Type::Dict { .. } => Some(DeltaOf::Dict),
            Type::List { .. } => Some(DeltaOf::List),
            Type::Set(element) => Some(DeltaOf::Set(element)),
            _ => None,
        }
    }
}

/// True for a type hgraph holds as one value under `TS[...]`, so that a
/// tick of it is the whole value: a scalar, an enum, and a nullable one.
pub(super) fn publishes_whole(ty: &Type) -> bool {
    match ty {
        Type::Nullable(inner) => publishes_whole(inner),
        Type::Named {
            kind: NamedKind::Enum,
            ..
        } => true,
        other => scalar(other).is_some(),
    }
}

/// True for a type hgraph holds as one value under `TS[...]` that is
/// not nullable: a scalar or an enum. It looks at the top of the type
/// only.
pub(super) fn whole_scalar(ty: &Type) -> bool {
    publishes_whole(ty) && !is_nullable(ty)
}

/// True where a cell of a trace of `trace` holds a value of `ty` whole:
/// the trace's cell is the `delta<T>` of a list, a set, a dictionary, a
/// struct or a tuple, and the value is of T (TST-5: "A list, a tuple, a
/// struct and a dictionary are each written either way"). A tuple's
/// trace names its T, `delta<tuple<A, B>>`, where the delta is a tuple
/// of its own (`Type::trace_cell`).
pub(super) fn whole_cell(trace: &Type, ty: &Type) -> bool {
    matches!(
        trace,
        Type::Trace { cell, .. } if matches!(
            &**cell,
            Type::Delta(of) if **of == *ty && matches!(
                **of,
                Type::List { .. }
                    | Type::Set(_)
                    | Type::Dict { .. }
                    | Type::Tuple(_)
                    | Type::Named {
                        kind: NamedKind::Struct,
                        ..
                    }
            )
        )
    )
}

/// True where a cell of a trace of `trace`, of a struct's delta, holds a
/// whole value of another struct, one below it (TYPE-3).
pub(super) fn descendant_cell(trace: &Type, ty: &Type) -> bool {
    let struct_type = |ty: &Type| {
        matches!(
            ty,
            Type::Named {
                kind: NamedKind::Struct,
                ..
            }
        )
    };
    matches!(
        trace,
        Type::Trace { cell, .. } if matches!(
            &**cell,
            Type::Delta(of) if struct_type(of) && struct_type(ty) && **of != *ty
        )
    )
}

/// True for a dictionary whose entries each publish whole and are not
/// null, which a whole value of it is published as (see `dict_replaced`).
pub(super) fn whole_dictionary(ty: &Type) -> bool {
    matches!(ty, Type::Dict { value, .. } if publishes_whole(value))
}

/// True for a window of a duration, `rolling_window<f64, 3s, 2s>`
/// (TYPE-11), which a value leaves as it ages, though nothing arrives.
pub(super) fn duration_window(ty: &Type) -> bool {
    matches!(
        ty,
        Type::RollingWindow { maximum, .. } if matches!(maximum.value, ConstantValue::Duration(_))
    )
}

/// Where a temporal value of `ty`, an unsized list, a struct or a tuple,
/// holds an unsized list through its own fields and positions, the
/// provider's shape of it for `published_into` and `invalidated`:
/// `"list"` for the list, a `dict` of the fields that hold one for a
/// struct, and a tuple of each position's, `None` where it holds none,
/// for a tuple. `None` where `ty` holds none so: a fixed list's and a
/// dictionary's are each hgraph's child that a delta may add, and a
/// delta of one that holds an unsized list's is reported where it is
/// made (`nested_list_delta`).
pub(super) fn list_parts_shape(program: &Program, ty: &Type) -> Option<String> {
    fn within(program: &Program, ty: &Type, depth: usize) -> Option<String> {
        if depth > HELD_DEPTH {
            return None;
        }
        match ty {
            Type::List { size: None, .. } => Some("\"list\"".to_string()),
            Type::Tuple(elements) => {
                let parts: Vec<Option<String>> = elements
                    .iter()
                    .map(|element| within(program, element, depth + 1))
                    .collect();
                if parts.iter().all(Option::is_none) {
                    return None;
                }
                let written: Vec<String> = parts
                    .into_iter()
                    .map(|part| part.unwrap_or_else(|| "None".to_string()))
                    .collect();
                Some(format!("({},)", written.join(", ")))
            }
            Type::Named {
                kind: NamedKind::Struct,
                ..
            } => {
                let written: Vec<String> = fields_of(program, ty)?
                    .into_iter()
                    .filter_map(|(member, field)| {
                        let part = within(program, &field, depth + 1)?;
                        Some(format!("{}: {part}", string(&name(&member.name))))
                    })
                    .collect();
                (!written.is_empty()).then(|| format!("{{{}}}", written.join(", ")))
            }
            _ => None,
        }
    }
    within(program, ty, 0)
}

/// Where a temporal value of `ty` holds what hgraph keeps when it is
/// invalidated, the provider's shape of it for `invalidated`: `"list"`
/// for an unsized list, `"entries"` for a dictionary or a set, a `dict`
/// of the fields that hold one for a struct, a tuple of each position's,
/// `None` where it holds none, for a tuple, and `[part]` for a fixed
/// list whose elements hold one. `None` where `ty` holds none.
///
/// END-4: "Invalidation clears validity, modification, payload, and
/// delta." hgraph 0.9.8's unbounded `TSL` keeps its positions, and its
/// `TSD` and `TSS` their keys and members, which the next delta read
/// would name removed, and which a delta that names no change would leave
/// in what it makes valid (TIME-7: "it makes the value valid and empty").
/// A struct, a tuple and a fixed list keep their children, so each is
/// looked into; what a dictionary or an unsized list holds goes with the
/// key or the position, so neither is.
pub(super) fn emptied_shape(program: &Program, ty: &Type) -> Option<String> {
    fn within(program: &Program, ty: &Type, depth: usize) -> Option<String> {
        if depth > HELD_DEPTH {
            return None;
        }
        match ty {
            Type::List { size: None, .. } => Some("\"list\"".to_string()),
            Type::Dict { .. } | Type::Set(_) => Some("\"entries\"".to_string()),
            Type::List {
                element,
                size: Some(_),
            } => within(program, element, depth + 1).map(|part| format!("[{part}]")),
            Type::Tuple(elements) => {
                let parts: Vec<Option<String>> = elements
                    .iter()
                    .map(|element| within(program, element, depth + 1))
                    .collect();
                if parts.iter().all(Option::is_none) {
                    return None;
                }
                let written: Vec<String> = parts
                    .into_iter()
                    .map(|part| part.unwrap_or_else(|| "None".to_string()))
                    .collect();
                Some(format!("({},)", written.join(", ")))
            }
            Type::Named {
                kind: NamedKind::Struct,
                ..
            } => {
                let written: Vec<String> = fields_of(program, ty)?
                    .into_iter()
                    .filter_map(|(member, field)| {
                        let part = within(program, &field, depth + 1)?;
                        Some(format!("{}: {part}", string(&name(&member.name))))
                    })
                    .collect();
                (!written.is_empty()).then(|| format!("{{{}}}", written.join(", ")))
            }
            _ => None,
        }
    }
    within(program, ty, 0)
}

/// Where a temporal value of `ty` holds a dictionary or a set, itself
/// among them, through dictionaries' values, structs' fields, tuples'
/// positions and fixed lists' elements: the provider's shape of it for
/// `entries_published`, `"entries"` for one whose own parts hold none,
/// `("entries", part)` for a dictionary whose values hold one,
/// `("fields", {name: part})` for a struct, `("positions", (part, ...))`
/// for a tuple and `("list", part)` for a fixed list, `None` for a part
/// that holds none. `None` where `ty` holds none so: hgraph's `TSD`
/// fails the run for a `REMOVE` of a key it does not hold, and neither
/// it nor the `TSS` is made valid by a delta that names no change. An
/// unsized list's is published by its size (`list_parts_shape`), and one
/// in it is not looked for.
///
/// Within a dictionary's values a struct, a tuple and a fixed list are
/// each written out whether or not they hold one, `("fields", {})` for a
/// struct of fields that publish whole: TIME-7, "A struct, a tuple and
/// a `list<V, N>` are valid where a child is (TIME-9), and such a delta
/// of one, `delta<Quote>()`, sets no child: it changes no validity, and
/// is never a tick". hgraph's `TSD` ticks for a key given one, where its
/// bundle and its `TSL` do not, and the provider leaves it out of the
/// dictionary's delta (`_pruned`).
pub(super) fn entries_shape(program: &Program, ty: &Type) -> Option<String> {
    /// `entry` is true within a dictionary's value.
    fn within(program: &Program, ty: &Type, depth: usize, entry: bool) -> Option<String> {
        if depth > HELD_DEPTH {
            return None;
        }
        let part = |ty: &Type| within(program, ty, depth + 1, entry);
        let written = |part: Option<String>| part.unwrap_or_else(|| "None".to_string());
        match ty {
            Type::Set(_) => Some("\"entries\"".to_string()),
            Type::Dict { value, .. } => Some(match within(program, value, depth + 1, true) {
                Some(inner) => format!("(\"entries\", {inner})"),
                None => "\"entries\"".to_string(),
            }),
            Type::List {
                element,
                size: Some(_),
            } => match part(element) {
                Some(inner) => Some(format!("(\"list\", {inner})")),
                None => entry.then(|| "(\"list\", None)".to_string()),
            },
            Type::Tuple(elements) => {
                let parts: Vec<Option<String>> = elements.iter().map(part).collect();
                if !entry && parts.iter().all(Option::is_none) {
                    return None;
                }
                let parts: Vec<String> = parts.into_iter().map(written).collect();
                Some(format!("(\"positions\", ({},))", parts.join(", ")))
            }
            Type::Named {
                kind: NamedKind::Struct,
                ..
            } => {
                let fields: Vec<String> = fields_of(program, ty)?
                    .into_iter()
                    .filter_map(|(member, field)| {
                        let inner = part(&field)?;
                        Some(format!("{}: {inner}", string(&name(&member.name))))
                    })
                    .collect();
                (entry || !fields.is_empty())
                    .then(|| format!("(\"fields\", {{{}}})", fields.join(", ")))
            }
            _ => None,
        }
    }
    within(program, ty, 0, false)
}

/// Where a temporal value of `ty` holds a list, fixed or unsized, as a
/// part at any depth, itself among them: the provider's shape of it for
/// `valid_of` and `all_valid_of`, `("list", part)`, `("fields", {name:
/// part})` for a struct or a tuple, and `("entries", part)` for a
/// dictionary, with `None` for a part that holds no list. `None` where
/// `ty` holds none.
///
/// TIME-9: "`valid` of a collection is true where any element, field or
/// entry is valid". hgraph's `TSL` stays valid once it has ticked where
/// each position has since lost its value (TYPE-16's `removed`), and so
/// does a `TSB` that holds one: only where a list is in the value does
/// hgraph's `valid` differ from the language's.
pub(super) fn validity_shape(program: &Program, ty: &Type) -> Option<String> {
    fn within(program: &Program, ty: &Type, depth: usize) -> Option<String> {
        if depth > HELD_DEPTH {
            return None;
        }
        let fields = |named: Vec<(String, Type)>| -> Option<String> {
            let parts: Vec<(String, Option<String>)> = named
                .into_iter()
                .map(|(field, part)| (field, within(program, &part, depth + 1)))
                .collect();
            if parts.iter().all(|(_, part)| part.is_none()) {
                return None;
            }
            let written: Vec<String> = parts
                .into_iter()
                .map(|(field, part)| {
                    format!(
                        "{}: {}",
                        string(&field),
                        part.unwrap_or_else(|| "None".to_string())
                    )
                })
                .collect();
            Some(format!("(\"fields\", {{{}}})", written.join(", ")))
        };
        match ty {
            Type::List { element, .. } => Some(format!(
                "(\"list\", {})",
                within(program, element, depth + 1).unwrap_or_else(|| "None".to_string())
            )),
            Type::Dict { value, .. } => {
                within(program, value, depth + 1).map(|part| format!("(\"entries\", {part})"))
            }
            Type::Tuple(elements) => fields(
                elements
                    .iter()
                    .enumerate()
                    .map(|(index, element)| (position(index), element.clone()))
                    .collect(),
            ),
            Type::Named {
                kind: NamedKind::Struct,
                ..
            } => fields(
                fields_of(program, ty)?
                    .into_iter()
                    .map(|(member, field)| (name(&member.name), field))
                    .collect(),
            ),
            _ => None,
        }
    }
    within(program, ty, 0)
}

/// True for a temporal type whose null is published whole, as hgraph's
/// `Null`: a `T?`, and an `atomic<T?>` (TYPE-9: "`atomic<V>` stops
/// expansion at a complete ordinary V").
pub(super) fn publishes_whole_null(ty: &Type) -> bool {
    match ty {
        Type::Atomic(payload) => is_nullable(payload),
        other => is_nullable(other),
    }
}
