//! Writes HGL types as C++ types, and the declarations of a module's
//! types: its enums, and the structs and native types it does not write
//! yet. Part of the transpiler, the last stage.
//!
//! | HGL | C++ |
//! | --- | --- |
//! | `i64`, `f64`, `bool`, `str`, `bytes` | hgraph's `Int`, `Float`, `Bool`, `Str`, `Bytes` (`primitive_types.h`) |
//! | `date`, `time`, `datetime`, `duration` | hgraph's `Date`, `Time`, `DateTime`, `TimeDelta` (`util/date_time.h`) |
//! | `civil_datetime`, `zoned_datetime` | hgraph's `CivilDateTime`, `ZonedDateTime` (`types/temporal.h`) |
//! | `enum Side { buy, sell }` | `enum class Side { buy, sell }` of the module's namespace; an enum of core's the runtime library declares, `ZeroPolicy`, is the library's |
//! | `T?` | `std::optional<T>` |
//! | `tuple<A, B>` | `std::tuple<A, B>` |
//! | `list<T>`, `list<T, N>` | the library's `hglrt::list<T>`, a `std::vector<T>` hgraph can hash; `std::array<T, N>` |
//! | `error`, `result<T, error>` | the library's `hglrt::error`, hgraph's `CanonicalError`; `hglrt::result<T>`, a `std::expected<T, error>` (ERR-2, ERR-4) |
//!
//! Every other type is not written yet: a struct, a set, a dictionary, a
//! window, a delta, a trace, `tenor`, `timezone` and `zoned_time`.

use hgl_analyser::{ConstantValue, DeclarationKind, Detail, Program, Scalar, Type};
use hgl_diagnostics::Location;

use super::Emitter;
use super::names::{self, identifier};
use crate::runtime;

/// The C++ type of `ty`, qualified from the global namespace, or `None`
/// for a type the target does not write yet.
pub(super) fn cpp_type(program: &Program, ty: &Type) -> Option<String> {
    Some(match ty {
        Type::Scalar(scalar) => scalar_type(*scalar)?.to_string(),
        Type::Named {
            module,
            declaration,
            ..
        } => match program.modules[*module].declarations[*declaration].kind {
            DeclarationKind::Enum => enum_name(program, *module, *declaration),
            _ => return None,
        },
        Type::Nullable(inner) => format!("std::optional<{}>", cpp_type(program, inner)?),
        Type::Tuple(positions) => {
            let positions: Option<Vec<String>> = positions
                .iter()
                .map(|position| cpp_type(program, position))
                .collect();
            format!("std::tuple<{}>", positions?.join(", "))
        }
        Type::List {
            element,
            size: None,
        } => format!(
            "::{}::list<{}>",
            runtime::NAMESPACE,
            cpp_type(program, element)?
        ),
        // TYPE-4: a fixed list holds exactly its size.
        Type::List {
            element,
            size: Some(size),
        } => match size.value {
            ConstantValue::Int(count) if count > 0 => {
                format!("std::array<{}, {count}>", cpp_type(program, element)?)
            }
            _ => return None,
        },
        Type::Error => format!("::{}::error", runtime::NAMESPACE),
        Type::Result { value, error } if **error == Type::Error => format!(
            "::{}::result<{}>",
            runtime::NAMESPACE,
            cpp_type(program, value)?
        ),
        Type::Result { value, error } => format!(
            "std::expected<{}, {}>",
            cpp_type(program, value)?,
            cpp_type(program, error)?
        ),
        _ => return None,
    })
}

/// The C++ type of a scalar, or `None` for one not written yet.
fn scalar_type(scalar: Scalar) -> Option<&'static str> {
    Some(match scalar {
        Scalar::I64 => "::hgraph::Int",
        Scalar::F64 => "::hgraph::Float",
        Scalar::Bool => "::hgraph::Bool",
        Scalar::Str => "::hgraph::Str",
        Scalar::Bytes => "::hgraph::Bytes",
        Scalar::Date => "::hgraph::Date",
        Scalar::Time => "::hgraph::Time",
        Scalar::Datetime => "::hgraph::DateTime",
        Scalar::Duration => "::hgraph::TimeDelta",
        Scalar::CivilDatetime => "::hgraph::CivilDateTime",
        Scalar::ZonedDatetime => "::hgraph::ZonedDateTime",
        Scalar::Tenor | Scalar::Timezone | Scalar::ZonedTime | Scalar::Any => return None,
    })
}

/// True where a value of `ty` is passed and bound by value rather than
/// by reference: it is small and holds nothing on the heap. A scalar but
/// `str`, `bytes` and `zoned_datetime`, an enum, and an optional of one.
/// What is passed by value is a copy, which is the value itself: no
/// change of a place can reach it (the copy rule, `hgl_lower::copies`).
pub(super) fn by_value(program: &Program, ty: &Type) -> bool {
    match ty {
        Type::Scalar(scalar) => {
            !matches!(scalar, Scalar::Str | Scalar::Bytes | Scalar::ZonedDatetime)
        }
        Type::Named {
            module,
            declaration,
            ..
        } => program.modules[*module].declarations[*declaration].kind == DeclarationKind::Enum,
        Type::Nullable(inner) => by_value(program, inner),
        _ => false,
    }
}

/// The C++ name of enum `declaration` of `module`, qualified from the
/// global namespace. An enum of core's that the runtime library
/// declares, `ZeroPolicy` among them, is the library's: its natives take
/// it, and it has core's members in core's order.
pub(super) fn enum_name(program: &Program, module: usize, declaration: usize) -> String {
    let name = &program.modules[module].declarations[declaration].name;
    if Some(module) == program.core() && runtime::declares_enum(name) {
        return format!("::{}::{name}", runtime::NAMESPACE);
    }
    format!(
        "{}::{}",
        names::module_namespace(&program.modules[module].path),
        identifier(name)
    )
}

impl Emitter<'_> {
    /// The C++ type of `ty`, or, for one not written yet, the reason
    /// reported at `at` and `None`.
    pub(super) fn written_type(&mut self, ty: &Type, at: Location) -> Option<String> {
        let written = cpp_type(self.program, ty);
        if written.is_none() {
            self.unsupported(at, &format!("the type `{ty}`"));
        }
        written
    }

    /// Writes the declaration of a struct, an enum or a native type
    /// (TYPE-3, TYPE-7). An enum is an `enum class` of its members, in
    /// the order declared: its members are its values, and nothing else
    /// is (TYPE-7). One of core's that the runtime library declares is
    /// the library's, and is not written again.
    pub(super) fn type_declaration(&mut self, index: usize) {
        let declared = &self.program.modules[self.module];
        let declaration = &declared.declarations[index];
        let at = declaration.at;
        match declaration.kind {
            DeclarationKind::Enum => {
                let Some(Detail::Enum(read)) = declared.details.get(index) else {
                    self.unsupported(at, "an enum");
                    return;
                };
                if Some(self.module) == self.program.core()
                    && runtime::declares_enum(&declaration.name)
                {
                    return;
                }
                let members: Vec<String> = read
                    .members
                    .iter()
                    .map(|member| identifier(&member.name))
                    .collect();
                let name = identifier(&declaration.name);
                self.piece.declaration = format!(
                    "// enum {} (TYPE-7)\nenum class {name}\n{{\n    {},\n}};\n",
                    declaration.name,
                    members.join(",\n    ")
                );
            }
            DeclarationKind::Struct => self.unsupported(at, "a struct"),
            _ => self.unsupported(at, "a native type"),
        }
    }
}
