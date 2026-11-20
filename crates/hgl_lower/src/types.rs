//! What a type of the program holds, as the language says it: a
//! struct's fields and its descendants, the windows and the unsized
//! lists inside it, which of its children `all_valid` asks, and whether
//! a value of it is whole or a delta. Part of lowering: a transpiler
//! asks it what a type is before it writes the type's form.

use hgl_analyser::hir::Expr;
use hgl_analyser::{Detail, Member, NamedKind, Program, Type};

/// True where another struct of the program inherits from the struct
/// `declaration` of `module`: a value of that type may be of the other
/// struct, with fields this one has not.
pub fn inherited(program: &Program, module: usize, declaration: usize) -> bool {
    program.modules.iter().any(|module_of| {
        module_of.details.iter().any(|detail| {
            matches!(detail, Detail::Struct(other) if other.parents.iter().any(|parent| {
                matches!(parent, Type::Named { module: parent_module, declaration: parent_declaration, .. }
                    if *parent_module == module && *parent_declaration == declaration)
            }))
        })
    })
}

/// The field at `member` among those a struct has, for a value of the
/// type `ty`. `None` where the type is no struct: a native type has
/// properties, and a delta its own.
pub fn field_of<'p>(program: &'p Program, ty: &Type, member: usize) -> Option<&'p Member> {
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

/// True for a type written `T?` (TYPE-14).
pub fn is_nullable(ty: &Type) -> bool {
    matches!(ty, Type::Nullable(_))
}

/// True for a temporal value of `ty` that is read whole only where it is
/// all valid (TIME-1: "A read of a struct, a tuple or a fixed list whole
/// ... requires `all_valid` of it"): a struct with a field, a tuple and a
/// list of a size. A struct with none publishes its one value whole
/// (`empty_struct`).
pub fn read_whole_all_valid(program: &Program, ty: &Type) -> bool {
    match ty {
        Type::Named {
            kind: NamedKind::Struct,
            ..
        } => !empty_struct(program, ty),
        Type::Tuple(_) | Type::List { size: Some(_), .. } => true,
        _ => false,
    }
}

/// True for a concrete struct with no field, `struct Empty {}`, that no
/// struct inherits from: a temporal value of it has no child, and
/// publishes its one value whole (TIME-1, TYPE-9).
pub fn empty_struct(program: &Program, ty: &Type) -> bool {
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

/// True where `value` is published to a place of the type `ty` as a
/// whole value, and false where it is a `delta<ty>`. TIME-7: "The place
/// expects T (EXPR-5), so a value that is a T is published whole ... A
/// value that is no T and is a `delta<T>` is published as that change."
/// A value of a struct is a T where T is an abstract struct it inherits
/// from (TYPE-3), and where T publishes whole `delta<T>` is T itself.
///
/// It asks whether `value` is `ty`'s delta, and not whether it is a
/// nullable's `delta<T?>`: the two part at an `atomic<T?>`, whose delta
/// is the `T?` a whole value has, and which this calls a delta.
///
/// A window is never published whole. TIME-7: "A result of
/// `rolling_window<V, Max, Min>` takes a V, which arrives, or a delta of
/// the window (TYPE-11)": each is a change to what the window holds, and
/// this is false for both.
pub fn whole_value(ty: &Type, value: &Expr) -> bool {
    if matches!(ty, Type::RollingWindow { .. }) {
        return false;
    }
    value.ty == *ty || value.ty != Type::delta(ty.clone())
}

/// The fields of the struct `ty`, each its member and its type with
/// `ty`'s generic arguments given (TYPE-4: a `Box<i64>`'s `value: T`
/// is an `i64`), a parent's first. `None` where `ty` is no struct of
/// the program.
pub fn fields_of<'p>(program: &'p Program, ty: &Type) -> Option<Vec<(&'p Member, Type)>> {
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
pub fn payload_of(ty: &Type) -> &Type {
    match ty {
        Type::Atomic(payload) => payload,
        other => other,
    }
}

/// Each window `ty` holds: itself, and one at any depth inside it, as a
/// struct's field, a tuple's position, a list's element, a dictionary's
/// value, or under a `?` (TYPE-9: temporalization maps each to a child).
/// A struct's fields are those of its application (`field_types`).
pub fn windows_in(program: &Program, ty: &Type) -> Vec<Type> {
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

/// True for an unsized list, `list<T>` (TYPE-10: "a growing shape").
pub fn growing_list(ty: &Type) -> bool {
    matches!(ty, Type::List { size: None, .. })
}

/// How deep a type is looked into for what it holds: a struct holds
/// itself only through an `atomic`, which is not looked into (TYPE-6).
pub const HELD_DEPTH: usize = 32;

/// True where a temporal value of `ty` holds an unsized list as a part
/// at any depth, itself among them: a struct's field, a tuple's
/// position, a list's element or a dictionary's value (TYPE-9), and
/// what a delta is of. An `atomic<V>` publishes whole, and holds no
/// part of its own.
pub fn holds_growing_list(program: &Program, ty: &Type) -> bool {
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

/// Which children of a temporal value of `ty` its `all_valid` asks
/// (TIME-9): "every one ... one level deep, an optional child aside".
pub enum Asked {
    /// Every child: no child is optional, or `ty` is no collection.
    Every,
    /// None: every child is optional, a list's element or a dictionary's
    /// value of a `T?`, or each field of a struct or position of a tuple.
    None,
    /// A struct's fields or a tuple's positions that are not optional,
    /// beside one that is: each by its position among the struct's
    /// fields (`fields_of`) or the tuple's positions, and its type.
    Some(Vec<(usize, Type)>),
}

/// The children of `ty` its `all_valid` asks. TIME-9: "an optional
/// child aside: one whose type is `T?`, or a field that TYPE-4 makes
/// optional, is not asked", and TYPE-4: "A field is optional when its
/// type is nullable, `T?`, or an `atomic<...>` of a nullable type", with
/// the struct's own arguments given (`fields_of`): the `value: T` of a
/// `Box<i64?>` is optional.
pub fn asked_children(program: &Program, ty: &Type) -> Asked {
    let field = |child: &Type| match child {
        Type::Nullable(_) => true,
        Type::Atomic(inner) => matches!(**inner, Type::Nullable(_)),
        _ => false,
    };
    let parts: Vec<(usize, Type, bool)> = match ty {
        Type::Named {
            kind: NamedKind::Struct,
            ..
        } => match fields_of(program, ty) {
            Some(fields) => fields
                .into_iter()
                .enumerate()
                .map(|(index, (_, part))| {
                    let optional = field(&part);
                    (index, part, optional)
                })
                .collect(),
            None => return Asked::Every,
        },
        Type::Tuple(elements) => elements
            .iter()
            .enumerate()
            .map(|(index, element)| {
                let optional = matches!(element, Type::Nullable(_));
                (index, element.clone(), optional)
            })
            .collect(),
        Type::List { element: child, .. } | Type::Dict { value: child, .. } => {
            return if matches!(**child, Type::Nullable(_)) {
                Asked::None
            } else {
                Asked::Every
            };
        }
        _ => return Asked::Every,
    };
    if parts.iter().all(|(_, _, optional)| !optional) {
        return Asked::Every;
    }
    let required: Vec<(usize, Type)> = parts
        .into_iter()
        .filter(|(_, _, optional)| !optional)
        .map(|(key, part, _)| (key, part))
        .collect();
    if required.is_empty() {
        Asked::None
    } else {
        Asked::Some(required)
    }
}
