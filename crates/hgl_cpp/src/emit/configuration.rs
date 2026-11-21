//! How a `const` parameter's value is held as the configuration of the
//! node it is given to: a node's, a source's, a lifted call's, a child
//! graph's capture. Part of the transpiler, the last stage.
//!
//! FN-1: "a `const` parameter takes what is fixed when the graph is
//! built. In composition that is any ordinary value." hgraph holds a
//! node's configuration as `hgraph::Scalar<"name", T>`, and needs a scalar
//! of its type registry for T, with `std::hash` and `==`, which its
//! interning of identical nodes asks of it. It has one for its own
//! scalars, and the runtime library registers an enum, a list and a
//! nullable whose payload has a hash. Every other value, a tuple, a
//! fixed list, a set, a dictionary, a struct, a nullable of a tuple, is
//! held as `::hglrt::configured<T>` (`values.h`): the value whole, an
//! atomic of hgraph's under the HGL name of its type, with the hash and
//! the equality of the value. So two nodes whose configurations differ
//! stay two, and two whose configurations are equal are one, as for any
//! other configuration; one that holds an `f64` is a node of its own
//! (hgraph issue 1866, `nodes::unique`).
//!
//! | Held | The `Scalar`'s type | Given where the node is wired | Read in a hook |
//! | --- | --- | --- | --- |
//! | as it is | `T` | `value` | `name.value()` |
//! | wrapped | `::hglrt::configured<T>` | `::hglrt::configured<T>{value}` | `name.value().held` |

use hgl_analyser::{DeclarationKind, Program, Type};

use crate::runtime;

/// True where hgraph holds a value of `ty` as configuration as the value
/// itself, a scalar of its own or one the runtime library registers: a
/// scalar, an enum, an unsized list, and a nullable whose payload
/// `std::hash` hashes (`hashed`).
pub(super) fn direct(program: &Program, ty: &Type) -> bool {
    match ty {
        Type::Scalar(_) | Type::List { size: None, .. } => true,
        Type::Named { .. } => is_enum(program, ty),
        Type::Nullable(payload) => hashed(payload),
        _ => false,
    }
}

/// True where `std::hash` hashes a value of `ty`, as hgraph's interning
/// asks of a nullable's payload through `std::hash<std::optional<T>>`: a
/// scalar, an enum, an unsized list, a set, a dictionary and a struct,
/// to each of which the runtime library or the compiler gives one. A
/// tuple and a fixed list have none.
fn hashed(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Scalar(_)
            | Type::List { size: None, .. }
            | Type::Set(_)
            | Type::Dict { .. }
            | Type::Named { .. }
    )
}

/// True where `ty` names an enum.
fn is_enum(program: &Program, ty: &Type) -> bool {
    matches!(ty, Type::Named { module, declaration, .. }
        if program.modules[*module].declarations[*declaration].kind == DeclarationKind::Enum)
}

/// The type an `hgraph::Scalar` holds a configuration of `ty` as, whose
/// C++ type is `written`.
pub(super) fn held(program: &Program, ty: &Type, written: &str) -> String {
    if direct(program, ty) {
        written.to_string()
    } else {
        format!("::{}::configured<{written}>", runtime::NAMESPACE)
    }
}

/// `value`, a value of `ty` whose C++ type is `written`, as the node is
/// given it where it is wired.
pub(super) fn given(program: &Program, ty: &Type, written: &str, value: &str) -> String {
    if direct(program, ty) {
        value.to_string()
    } else {
        format!("::{}::configured<{written}>{{{value}}}", runtime::NAMESPACE)
    }
}

/// The value of the configuration `name`, of `ty`, as a hook reads it
/// from its `hgraph::Scalar`.
pub(super) fn read(program: &Program, ty: &Type, name: &str) -> String {
    if direct(program, ty) {
        format!("{name}.value()")
    } else {
        format!("{name}.value().held")
    }
}
