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

/// The field at `member` among those a struct has, for a value of the
/// type `ty`. `None` where the type is no struct: a native type has
/// properties, and a delta its own.
pub(super) fn field_of<'p>(program: &'p Program, ty: &Type, member: usize) -> Option<&'p Member> {
    let Type::Named {
        module,
        declaration,
        kind: NamedKind::Struct,
        ..
    } = ty
    else {
        return None;
    };
    match program.modules[*module].details.get(*declaration) {
        Some(Detail::Struct(read)) => read.members.get(member),
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

/// True where another struct of the program inherits from the struct
/// `declaration` of `module`: a value of that type may be of the other
/// struct, with fields this one has not.
pub(crate) fn inherited(program: &Program, module: usize, declaration: usize) -> bool {
    program.modules.iter().any(|module_of| {
        module_of.details.iter().any(|detail| {
            matches!(detail, Detail::Struct(other) if other.parents.iter().any(|parent| {
                matches!(parent, Type::Named { module: parent_module, declaration: parent_declaration, .. }
                    if *parent_module == module && *parent_declaration == declaration)
            }))
        })
    })
}

/// True for a type written `T?` (TYPE-14).
pub(super) fn is_nullable(ty: &Type) -> bool {
    matches!(ty, Type::Nullable(_))
}

/// True for a temporal value of `ty` that is read whole only where it is
/// all valid (TIME-1: "A read of a struct, a tuple or a fixed list whole
/// ... requires `all_valid` of it"): a struct with a field, a tuple and a
/// list of a size. A struct with none publishes its one value whole
/// (`empty_struct`).
pub(super) fn read_whole_all_valid(program: &Program, ty: &Type) -> bool {
    match ty {
        Type::Named {
            kind: NamedKind::Struct,
            ..
        } => !empty_struct(program, ty),
        Type::Tuple(_) | Type::List { size: Some(_), .. } => true,
        _ => false,
    }
}

/// True for a concrete struct with no field, `struct Empty {}`, which
/// this target writes as a temporal `TS` of its class
/// (`Emitter::struct_annotation`).
pub(super) fn empty_struct(program: &Program, ty: &Type) -> bool {
    let Type::Named {
        module,
        declaration,
        kind: NamedKind::Struct,
        ..
    } = ty
    else {
        return false;
    };
    matches!(program.modules[*module].details.get(*declaration),
        Some(Detail::Struct(read)) if read.members.is_empty())
        && !inherited(program, *module, *declaration)
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

/// True where `value` is published to a place of the type `ty` as a
/// whole value, and false where it is a `delta<ty>`. TIME-7: "The place
/// expects T (EXPR-5), so a value that is a T is published whole ... A
/// value that is no T and is a `delta<T>` is published as that change."
/// A value of a struct is a T where T is an abstract struct it inherits
/// from (TYPE-3), and where T publishes whole `delta<T>` is T itself.
///
/// It asks whether `value` is `ty`'s delta, and is not the test of
/// `Function::publishes_null`, which asks whether it is a nullable's
/// `delta<T?>`: the two part at an `atomic<T?>`, whose delta is the
/// `T?` a whole value has, and which this calls a delta.
pub(super) fn whole_value(ty: &Type, value: &Expr) -> bool {
    value.ty == *ty || value.ty != Type::delta(ty.clone())
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

/// The fields of the struct `ty`, each its member and its type with
/// `ty`'s generic arguments given (TYPE-4: a `Box<i64>`'s `value: T`
/// is an `i64`), a parent's first. `None` where `ty` is no struct of
/// the program.
pub(super) fn fields_of<'p>(program: &'p Program, ty: &Type) -> Option<Vec<(&'p Member, Type)>> {
    let Type::Named {
        module,
        declaration,
        kind: NamedKind::Struct,
        ..
    } = ty
    else {
        return None;
    };
    let Some(Detail::Struct(read)) = program.modules[*module].details.get(*declaration) else {
        return None;
    };
    Some(read.members.iter().zip(program.field_types(ty)?).collect())
}

/// What a value of `ty` holds, with the `atomic` taken off: TYPE-9's
/// `atomic<V>` holds the whole V, whose fields and parts are V's. Any
/// other type is itself.
pub(super) fn payload_of(ty: &Type) -> &Type {
    match ty {
        Type::Atomic(payload) => payload,
        other => other,
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

/// Each window `ty` holds: itself, and one at any depth inside it, as a
/// struct's field, a tuple's position, a list's element, a dictionary's
/// value, or under a `?` (TYPE-9: temporalization maps each to a child).
/// A struct's fields are those of its application (`field_types`).
pub(super) fn windows_in(program: &Program, ty: &Type) -> Vec<Type> {
    /// How deep a type is looked into: a struct does not hold itself
    /// but through an `atomic`, which holds no window (TYPE-12).
    const DEPTH: usize = 32;
    fn within(program: &Program, ty: &Type, depth: usize, found: &mut Vec<Type>) {
        if depth > DEPTH {
            return;
        }
        match ty {
            Type::RollingWindow { .. } => found.push(ty.clone()),
            Type::Nullable(inner) => within(program, inner, depth + 1, found),
            Type::List { element, .. } => within(program, element, depth + 1, found),
            Type::Dict { value, .. } => within(program, value, depth + 1, found),
            Type::Tuple(elements) => {
                for element in elements {
                    within(program, element, depth + 1, found);
                }
            }
            Type::Named {
                kind: NamedKind::Struct,
                ..
            } => {
                for field in program.field_types(ty).unwrap_or_default() {
                    within(program, &field, depth + 1, found);
                }
            }
            _ => {}
        }
    }
    let mut found = Vec::new();
    within(program, ty, 0, &mut found);
    found
}

/// True for an unsized list, `list<T>` (TYPE-10: "a growing shape"),
/// which this target writes as hgraph's unbounded `TSL`.
pub(super) fn growing_list(ty: &Type) -> bool {
    matches!(ty, Type::List { size: None, .. })
}

/// How deep a type is looked into for what it holds: a struct holds
/// itself only through an `atomic`, which is not looked into (TYPE-6).
const HELD_DEPTH: usize = 32;

/// True where a temporal value of `ty` holds an unsized list as a part
/// at any depth, itself among them: a struct's field, a tuple's
/// position, a list's element or a dictionary's value (TYPE-9), and
/// what a delta is of. An `atomic<V>` publishes whole, and holds no
/// part of its own.
pub(super) fn holds_growing_list(program: &Program, ty: &Type) -> bool {
    fn within(program: &Program, ty: &Type, depth: usize) -> bool {
        if depth > HELD_DEPTH {
            return false;
        }
        match ty {
            Type::List { size: None, .. } => true,
            Type::Nullable(inner) | Type::Delta(inner) => within(program, inner, depth + 1),
            Type::List { element, .. } => within(program, element, depth + 1),
            Type::Dict { value, .. } => within(program, value, depth + 1),
            Type::Tuple(elements) => elements
                .iter()
                .any(|element| within(program, element, depth + 1)),
            Type::Named {
                kind: NamedKind::Struct,
                ..
            } => program
                .field_types(ty)
                .unwrap_or_default()
                .iter()
                .any(|field| within(program, field, depth + 1)),
            _ => false,
        }
    }
    within(program, ty, 0)
}

/// True where a whole value of `ty` is published through the port of
/// the output it goes to: where it holds an unsized list or a set as a
/// part at any depth, itself among them (`holds_growing_list`). TIME-7,
/// as the specification's `main` has it (hhenson/hgl_spec#124): "A value
/// published whole, the same again or empty, is no delta, and ticks as
/// any publication does", which hgraph's unbounded `TSL` does not for an
/// empty list and its `TSS` does not for a set that keeps its members:
/// the provider makes each tick through its own port (`_whole_list`,
/// `_whole_set`).
pub(super) fn published_through_port(program: &Program, ty: &Type) -> bool {
    fn holds_set(program: &Program, ty: &Type, depth: usize) -> bool {
        if depth > HELD_DEPTH {
            return false;
        }
        match ty {
            Type::Set(_) => true,
            Type::Nullable(inner) | Type::Delta(inner) => holds_set(program, inner, depth + 1),
            Type::List { element, .. } => holds_set(program, element, depth + 1),
            Type::Dict { value, .. } => holds_set(program, value, depth + 1),
            Type::Tuple(elements) => elements
                .iter()
                .any(|element| holds_set(program, element, depth + 1)),
            Type::Named {
                kind: NamedKind::Struct,
                ..
            } => program
                .field_types(ty)
                .unwrap_or_default()
                .iter()
                .any(|field| holds_set(program, field, depth + 1)),
            _ => false,
        }
    }
    holds_growing_list(program, ty) || holds_set(program, ty, 0)
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
/// delta." hgraph 0.9.5's unbounded `TSL` keeps its positions, and its
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
/// struct of fields that publish whole: TIME-7, as the specification's
/// `main` has it (hhenson/hgl_spec#124), "A struct, a tuple and a
/// `list<V, N>` are valid where a child is (TIME-9), and such a delta of
/// one, `delta<Quote>()`, sets no child: it changes no validity, and is
/// never a tick". hgraph's `TSD` ticks for a key given one, where its
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
