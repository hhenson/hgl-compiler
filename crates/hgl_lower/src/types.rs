//! What a type of the program holds, as the language says it: a
//! struct's fields and its descendants, and whether a value of it is
//! read whole only where it is all valid. Part of lowering: a transpiler
//! asks it what a type is before it writes the type's form.

use hgl_analyser::{Constant, ConstantValue, Detail, Member, NamedKind, Program, Type};

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

/// The size a fixed list supplies to lowering, possibly still unresolved.
/// An omitted size and TYPE-10's `unbounded` sentinel both grow.
pub fn fixed_list_size(size: Option<&Constant>) -> Option<&Constant> {
    size.filter(|size| !matches!(size.value, ConstantValue::Unbounded))
}

/// True for a temporal value of `ty` that is read whole only where it is
/// all valid (TIME-1: "A read of a struct, a tuple or a fixed list whole
/// ... requires `all_valid` of it"): a struct with a field, a tuple and a
/// fixed list. A struct with no fields publishes its one value whole
/// (`empty_struct`).
pub fn read_whole_all_valid(program: &Program, ty: &Type) -> bool {
    match ty {
        Type::Named {
            kind: NamedKind::Struct,
            ..
        } => !empty_struct(program, ty),
        Type::Tuple(_) => true,
        Type::List { size, .. } => fixed_list_size(size.as_ref()).is_some(),
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
