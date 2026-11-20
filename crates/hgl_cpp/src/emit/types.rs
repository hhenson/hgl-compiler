//! Writes HGL types as C++ types, and the declarations of a module's
//! types: structs, enums and native types. Part of the transpiler, the
//! last stage.
//!
//! The scalars are hgraph's own (`primitive_types.h`): `i64` is
//! `hgraph::Int`, `f64` `hgraph::Float`, `bool` `hgraph::Bool`. Every
//! other type, and every declaration of one, is not written yet.

use hgl_analyser::{DeclarationKind, Scalar, Type};
use hgl_diagnostics::Location;

use super::Emitter;

/// The C++ type of `ty`, qualified from the global namespace, or `None`
/// for a type the target does not write yet.
pub(super) fn cpp_type(ty: &Type) -> Option<&'static str> {
    match ty {
        Type::Scalar(Scalar::I64) => Some("::hgraph::Int"),
        Type::Scalar(Scalar::F64) => Some("::hgraph::Float"),
        Type::Scalar(Scalar::Bool) => Some("::hgraph::Bool"),
        _ => None,
    }
}

impl Emitter<'_> {
    /// The C++ type of `ty`, or, for one not written yet, the reason
    /// reported at `at` and `None`.
    pub(super) fn written_type(&mut self, ty: &Type, at: Location) -> Option<&'static str> {
        let written = cpp_type(ty);
        if written.is_none() {
            self.unsupported(at, &format!("the type `{ty}`"));
        }
        written
    }

    /// Writes the declaration of a struct, an enum or a native type
    /// (TYPE-3, TYPE-7): none is written yet.
    pub(super) fn type_declaration(&mut self, kind: DeclarationKind, at: Location) {
        let what = match kind {
            DeclarationKind::Struct => "a struct",
            DeclarationKind::Enum => "an enum",
            _ => "a native type",
        };
        self.unsupported(at, what);
    }
}
