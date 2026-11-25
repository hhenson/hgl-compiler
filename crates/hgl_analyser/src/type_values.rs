//! Type identities held as ordinary constants (TYPE-18, GEN-13).
//! Part of analysis; display spelling is separate from identity so
//! declarations with the same name in different modules stay distinct.

use crate::{Argument, ConstantValue, Type};

impl Type {
    /// A compilation-local identity for a fully resolved type. This is
    /// shared by Type equality and hashing, never by display conversion.
    /// Named types include their declaration, not their written alias.
    pub fn identity(&self) -> Option<String> {
        let one = Type::identity;
        let argument = |argument: &Argument| match argument {
            Argument::Type(ty) => one(ty),
            Argument::Constant(value) => value.value.key(),
            Argument::NoResult => Some("no-result".to_string()),
        };
        Some(match self {
            Type::TypeValue => "Type".to_string(),
            Type::Scalar(scalar) => scalar.as_str().to_string(),
            Type::Named {
                module,
                declaration,
                arguments,
                ..
            } => {
                let arguments = arguments.iter().map(argument).collect::<Option<Vec<_>>>()?;
                format!(
                    "declaration({module},{declaration})<{}>",
                    arguments.join(",")
                )
            }
            Type::Tuple(values) => {
                let values = values.iter().map(one).collect::<Option<Vec<_>>>()?;
                format!("tuple<{}>", values.join(","))
            }
            Type::List { element, size } => match size {
                None => format!("list<{}>", one(element)?),
                Some(size) if matches!(size.value, ConstantValue::Unbounded) => {
                    format!("list<{}>", one(element)?)
                }
                Some(size) => format!("list<{},{}>", one(element)?, size.value.key()?),
            },
            Type::Set(element) => format!("set<{}>", one(element)?),
            Type::Dict { key, value } => format!("dict<{},{}>", one(key)?, one(value)?),
            Type::RollingWindow {
                value,
                maximum,
                minimum,
            } => {
                let maximum = maximum.value.key()?;
                let minimum = minimum
                    .as_ref()
                    .map_or_else(|| Some(maximum.clone()), |v| v.value.key())?;
                format!("rolling_window<{},{maximum},{minimum}>", one(value)?)
            }
            Type::Atomic(payload) => {
                let normalized = Type::atomic((**payload).clone());
                if &normalized != self {
                    return one(&normalized);
                }
                format!("atomic<{}>", one(payload)?)
            }
            Type::Delta(payload) => {
                let normalized = Type::delta((**payload).clone());
                if &normalized != self {
                    return one(&normalized);
                }
                format!("delta<{}>", one(payload)?)
            }
            Type::Nullable(payload) => format!("{}?", one(payload)?),
            Type::Endpoint(payload) => format!("endpoint<{}>", one(payload)?),
            Type::Result { value, error } => format!("result<{},{}>", one(value)?, one(error)?),
            Type::Signal => "signal".to_string(),
            Type::Error => "error".to_string(),
            Type::ErrorContext => "error-context".to_string(),
            Type::Trace { cell, style } => format!("trace<{},{style:?}>", one(cell)?),
            Type::Function(function) => {
                let inputs = function
                    .parameters
                    .iter()
                    .map(|p| one(p.ty()))
                    .collect::<Option<Vec<_>>>()?;
                let result = function
                    .result
                    .as_ref()
                    .map_or_else(|| Some("no-result".to_string()), one)?;
                format!(
                    "fn({:?},{})->{result};ref={}",
                    function.value,
                    inputs.join(","),
                    function.reference
                )
            }
            Type::Parameter { .. } | Type::PackMember { .. } | Type::Unknown => return None,
        })
    }
}
