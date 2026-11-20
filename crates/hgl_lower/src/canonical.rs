//! Canonical text of a type and of a generic argument: what tells one
//! generic instance from another. Part of lowering: [`crate::instances`]
//! keys each instance by it, and a transpiler names what it writes for a
//! type's application with it.

use hgl_analyser::{Argument as GenericArgument, Constant, Program, Type};

/// A type as text that no other type is written as: part of the key of
/// a generic instance (GEN-2) and of a generic struct's application,
/// whose values TYPE-3 tells apart by "complete canonical arguments". A named
/// type has its module's path before it: two modules may each declare
/// a `Quote`. `None` for a type that is not whole as the module is
/// written: one that names a generic parameter, or is not known.
pub fn canonical(program: &Program, ty: &Type) -> Option<String> {
    let one = |ty: &Type| canonical(program, ty);
    Some(match ty {
        Type::Scalar(scalar) => scalar.as_str().to_string(),
        Type::Named {
            name,
            module,
            arguments,
            ..
        } => {
            let path = &program.modules[*module].path;
            if arguments.is_empty() {
                format!("{path}::{name}")
            } else {
                let arguments: Option<Vec<String>> = arguments
                    .iter()
                    .map(|argument| canonical_argument(program, argument))
                    .collect();
                format!("{path}::{name}<{}>", arguments?.join(", "))
            }
        }
        Type::Tuple(elements) => {
            let elements: Option<Vec<String>> = elements.iter().map(one).collect();
            format!("tuple<{}>", elements?.join(", "))
        }
        Type::List { element, size } => match size {
            Some(size) => format!("list<{}, {}>", one(element)?, canonical_constant(size)?),
            None => format!("list<{}>", one(element)?),
        },
        Type::Set(element) => format!("set<{}>", one(element)?),
        Type::Dict { key, value } => format!("dict<{}, {}>", one(key)?, one(value)?),
        Type::RollingWindow {
            value,
            maximum,
            minimum,
        } => match minimum {
            Some(minimum) => format!(
                "rolling_window<{}, {}, {}>",
                one(value)?,
                canonical_constant(maximum)?,
                canonical_constant(minimum)?
            ),
            None => format!(
                "rolling_window<{}, {}>",
                one(value)?,
                canonical_constant(maximum)?
            ),
        },
        Type::Endpoint(target) => format!("endpoint<{}>", one(target)?),
        Type::Signal => "signal".to_string(),
        Type::Atomic(payload) => format!("atomic<{}>", one(payload)?),
        Type::Result { value, error } => format!("result<{}, {}>", one(value)?, one(error)?),
        Type::Error => "error".to_string(),
        Type::ErrorContext => "error context".to_string(),
        Type::Delta(shape) => format!("delta<{}>", one(shape)?),
        Type::Nullable(inner) => format!("{}?", one(inner)?),
        Type::Parameter { .. } | Type::Trace { .. } | Type::Unknown => return None,
    })
}

/// A generic argument as [`canonical`] text.
pub fn canonical_argument(program: &Program, argument: &GenericArgument) -> Option<String> {
    match argument {
        GenericArgument::Type(ty) => canonical(program, ty),
        GenericArgument::Constant(constant) => canonical_constant(constant),
    }
}

/// A constant inside a type as [`canonical`] text: what it comes to,
/// however it was written, as the analyser keys it
/// (`ConstantValue::key`): two constants it holds the same have one
/// text. `None` for one whose value is not known as the module is
/// written.
fn canonical_constant(constant: &Constant) -> Option<String> {
    constant.value.key()
}
