//! Writes HGL types as C++ types, and the declarations of a module's
//! types: its enums here, its structs in `composites`, and the native
//! types it does not write yet. Part of the transpiler, the last stage.
//!
//! | HGL | C++ |
//! | --- | --- |
//! | `i64`, `f64`, `bool`, `str`, `bytes` | hgraph's `Int`, `Float`, `Bool`, `Str`, `Bytes` (`primitive_types.h`) |
//! | `date`, `time`, `datetime`, `duration` | hgraph's `Date`, `Time`, `DateTime`, `TimeDelta` (`util/date_time.h`) |
//! | `civil_datetime`, `zoned_datetime`, `tenor` | hgraph's `CivilDateTime`, `ZonedDateTime`, `Tenor` (`types/temporal.h`) |
//! | `timezone`, `zoned_time` | hgraph's `ZoneId` and `ZonedTime` (`types/temporal.h`) |
//! | `enum Side { buy, sell }` | `enum class Side { buy, sell }` of the module's namespace; an enum of core's the runtime library declares, `ZeroPolicy`, is the library's |
//! | `T?` | `std::optional<T>` |
//! | `tuple<A, B>` | `std::tuple<A, B>` |
//! | `list<T>`, `list<T, unbounded>`, `list<T, N>` | growing lists use `hglrt::list<T>`, a `std::vector<T>` hgraph can hash; fixed lists use `std::array<T, N>` |
//! | `error`, `result<T, error>` | the library's `hglrt::error`, hgraph's `CanonicalError`; `hglrt::result<T>`, a `std::expected<T, error>` (ERR-2, ERR-4) |
//! | An error's `.context` (ERR-4) | hgraph's `ErrorContext` |
//! | A struct, a set, a dictionary, a delta | `composites`' |
//!
//! Every other type is not written yet: a window, a trace and anything
//! temporal.

use hgl_analyser::{ConstantValue, DeclarationKind, Detail, Program, Scalar, Type};
use hgl_diagnostics::Location;
use hgl_lower::types::fixed_list_size;

use super::Emitter;
use super::composites;
use super::expressions::integer_literal;
use super::names::{self, identifier};
use crate::runtime;

/// The C++ type of `ty`, qualified from the global namespace, or `None`
/// for a type the target does not write yet.
pub(super) fn cpp_type(program: &Program, ty: &Type) -> Option<String> {
    cpp_type_in(program, ty, None)
}

/// [`cpp_type`], where `queries` is present when `ty` is written inside the
/// declaration of a generic struct, where a generic parameter of it is
/// the C++ template's parameter of its name (`composites`). Anywhere else
/// a generic parameter is not written: a generic function is written for
/// each instance, its parameters given. The slice names reflected type identities
/// forwarded by this layout (TYPE-18).
pub(super) fn cpp_type_in(
    program: &Program,
    ty: &Type,
    queries: Option<&[Type]>,
) -> Option<String> {
    Some(match ty {
        Type::TypeValue => "::hglrt::type_value".to_string(),
        Type::Scalar(scalar) => scalar_type(*scalar)?.to_string(),
        Type::Named {
            module,
            declaration,
            arguments,
            ..
        } => match program.modules[*module].declarations[*declaration].kind {
            DeclarationKind::Enum => enum_name(program, *module, *declaration),
            DeclarationKind::Struct => {
                composites::struct_type(program, (*module, *declaration), arguments, queries)?
            }
            // CAP-1: a capability is the runtime library's (`endpoints`).
            DeclarationKind::NativeType => {
                super::endpoints::capability_type(program, *module, *declaration)?
            }
            _ => return None,
        },
        // TYPE-16: "`delta<f64>` is `f64`": a scalar publishes whole, and
        // its delta is itself.
        Type::Delta(inner) if matches!(**inner, Type::Scalar(_)) => cpp_type(program, inner)?,
        Type::Nullable(inner) => {
            format!("::std::optional<{}>", cpp_type_in(program, inner, queries)?)
        }
        Type::Tuple(positions) => {
            let positions: Option<Vec<String>> = positions
                .iter()
                .map(|position| cpp_type_in(program, position, queries))
                .collect();
            format!("::std::tuple<{}>", positions?.join(", "))
        }
        Type::List { element, size } => {
            match fixed_list_size(size.as_ref()).map(|size| &size.value) {
                None => format!(
                    "::{}::list<{}>",
                    runtime::NAMESPACE,
                    cpp_type_in(program, element, queries)?
                ),
                // TYPE-4: a fixed list holds exactly its size.
                Some(ConstantValue::Int(count)) if *count >= 0 => {
                    format!(
                        "::std::array<{}, {count}>",
                        cpp_type_in(program, element, queries)?
                    )
                }
                // The size of a generic struct's own (GEN-13).
                Some(ConstantValue::Parameter { name, .. }) if queries.is_some() => format!(
                    "::std::array<{}, {}>",
                    cpp_type_in(program, element, queries)?,
                    names::template_parameter(name)
                ),
                _ => return None,
            }
        }
        Type::Error => format!("::{}::error", runtime::NAMESPACE),
        // ERR-4: an error's `.context`, "a read-only nullable record of
        // `.node: str`, `.phase: str` ... and `.evaluation_time`", is
        // hgraph's `ErrorContext`, which `CanonicalError` holds.
        Type::ErrorContext => "::hgraph::ErrorContext".to_string(),
        Type::Result { value, error } if **error == Type::Error => format!(
            "::{}::result<{}>",
            runtime::NAMESPACE,
            cpp_type_in(program, value, queries)?
        ),
        Type::Result { value, error } => format!(
            "::std::expected<{}, {}>",
            cpp_type_in(program, value, queries)?,
            cpp_type_in(program, error, queries)?
        ),
        Type::Set(_) | Type::Dict { .. } | Type::Delta(_) | Type::Atomic(_) => {
            composites::composite_type(program, ty, queries)?
        }
        Type::Parameter { name, .. } if queries.is_some() => names::template_parameter(name),
        _ => return None,
    })
}

/// The distinct C++ value types of HGL scalars (TYPE-1), or `None`
/// for one not written yet.
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
        // TYPE-17: hgraph's tenor, of its two forms.
        Scalar::Tenor => "::hgraph::Tenor",
        // TYPE-2: "named `timezone`" and "date-independent `zoned_time`",
        // a zone's name and a time of day in it.
        Scalar::Timezone => "::hgraph::ZoneId",
        Scalar::ZonedTime => "::hgraph::ZonedTime",
        Scalar::Any => return None,
    })
}

/// True where a value of `ty` is passed and bound by value rather than
/// by reference: it is small and holds nothing on the heap. A scalar but
/// `str`, `bytes` and `zoned_datetime`, an enum, a struct that the copy
/// rule takes as a value (`hgl_lower::copies::by_value`: at most two
/// fields of such scalars), and an optional of one.
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
        } => match program.modules[*module].declarations[*declaration].kind {
            DeclarationKind::Enum => true,
            DeclarationKind::Struct => hgl_lower::copies::by_value(program, *module, *declaration),
            _ => false,
        },
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
        names::declared_type(
            &program.modules[module].declarations[declaration],
            declaration
        )
    )
}

impl Emitter<'_> {
    /// The C++ type of `ty`, or, for one not written yet, the reason
    /// reported at `at` and `None`.
    pub(super) fn written_type(&mut self, ty: &Type, at: Location) -> Option<String> {
        let written = self.named_type(ty);
        if written.is_none() {
            self.unsupported(at, &format!("the type `{ty}`"));
        }
        written
    }

    /// The C++ type of `ty`, as [`cpp_type`] gives it, noting each type
    /// of the program it names ([`Emitter::note_types`]).
    pub(super) fn named_type(&mut self, ty: &Type) -> Option<String> {
        let written = cpp_type(self.program, ty)?;
        self.note_types(ty);
        Some(written)
    }

    /// Notes each enum of a module other than this one that `ty` names,
    /// at any depth, and that the runtime library does not declare, and
    /// each struct it names (`composites::note_struct`).
    pub(super) fn note_types(&mut self, ty: &Type) {
        match ty {
            Type::Named {
                module,
                declaration,
                arguments,
                ..
            } => {
                if self.program.modules[*module].declarations[*declaration].kind
                    == DeclarationKind::Struct
                {
                    self.note_struct(*module, *declaration);
                } else {
                    self.note_enum(*module, *declaration);
                }
                for argument in arguments {
                    if let hgl_analyser::Argument::Type(argument) = argument {
                        self.note_types(argument);
                    }
                }
            }
            Type::Nullable(inner)
            | Type::List { element: inner, .. }
            | Type::Set(inner)
            | Type::Delta(inner)
            | Type::Atomic(inner) => self.note_types(inner),
            Type::Dict { key, value } => {
                self.note_types(key);
                self.note_types(value);
            }
            Type::Tuple(positions) => {
                for position in positions {
                    self.note_types(position);
                }
            }
            Type::Result { value, error } => {
                self.note_types(value);
                self.note_types(error);
            }
            _ => {}
        }
    }

    /// Notes enum `declaration` of `module`, where it is another module's
    /// and declared by no library.
    pub(super) fn note_enum(&mut self, module: usize, declaration: usize) {
        let declared = &self.program.modules[module].declarations[declaration];
        // A capability, core's native type, is the library's, and no enum
        // to declare (`endpoints::capability_type`).
        if declared.kind != DeclarationKind::Enum {
            return;
        }
        let name = &declared.name;
        let library = Some(module) == self.program.core() && runtime::declares_enum(name);
        if module != self.module && !library {
            self.piece.types.insert((module, declaration));
        }
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
                let name = names::declared_type(declaration, index);
                self.piece.is_type = true;
                let enumerators = if members.is_empty() {
                    String::new()
                } else {
                    format!("    {},\n", members.join(",\n    "))
                };
                self.piece.declaration = format!(
                    "// enum {} (TYPE-7)\nenum class {name}\n{{\n{enumerators}}};\n",
                    declaration.name,
                );
                let qualified = match declaration.test_owner {
                    Some(owner) => format!(
                        "{}::{}::{}",
                        declared.path, declared.tests[owner].name, declaration.name
                    ),
                    None => format!("{}::{}", declared.path, declaration.name),
                };
                let qualified = names::string(&qualified);
                // TYPE-7: the enum's name also identifies its port and
                // result types when there are no members to publish.
                let named: Vec<String> = read
                    .members
                    .iter()
                    .zip(&members)
                    .map(|(member, written)| {
                        format!("{{{name}::{written}, {}}}", names::string(&member.name))
                    })
                    .collect();
                let Some(numbers) = read
                    .members
                    .iter()
                    .map(|member| member.value)
                    .collect::<Option<Vec<_>>>()
                else {
                    self.unsupported(at, "an enum with unresolved member numbers");
                    return;
                };
                // TYPE-7: an enum's C++ ordinal stays separate from
                // the signed number an HGL conversion uses.
                let numbers = if numbers.iter().copied().eq((0..).take(numbers.len())) {
                    String::new()
                } else {
                    format!(
                        ", {{{}}}",
                        numbers
                            .into_iter()
                            .map(integer_literal)
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                };
                self.piece.declaration.push_str(&format!(
                    "constexpr ::{}::EnumNames<{name}, {}> hgl_enum({name}) noexcept\n{{\n    return {{{qualified}, {{{}}}{numbers}}};\n}}\n",
                    runtime::NAMESPACE,
                    members.len(),
                    named.join(", ")
                ));
            }
            DeclarationKind::Struct => self.struct_declaration(index),
            _ => self.unsupported(at, "a native type"),
        }
    }
}
