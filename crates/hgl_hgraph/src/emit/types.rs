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
    /// A list's: `updated`, `removed` and `size_modified`. `fixed` for
    /// a list with a size.
    List { fixed: bool },
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
            Type::List { size, .. } => Some(DeltaOf::List {
                fixed: size.is_some(),
            }),
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
/// the trace's cell is the `delta<T>` of a list, a set, a dictionary or
/// a struct, a type of its own, and the value is of T (TST-5).
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
/// which this target writes as a `TS` of a whole tuple.
pub(super) fn growing_list(ty: &Type) -> bool {
    matches!(ty, Type::List { size: None, .. })
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
