//! Type identities held as ordinary constants (TYPE-18, GEN-13).
//! Part of analysis; display spelling is separate from identity so
//! declarations with the same name in different modules stay distinct.

use std::collections::{HashMap, HashSet};

use hgl_diagnostics::{Category, Diagnostics, Location};

use crate::{Argument, ConstantValue, Detail, GenericKind, NamedKind, Program, Type};

/// TYPE-16 and TYPE-18, Proposed: a Type-containing value has no
/// temporal form and therefore no delta. Struct fields are resolved
/// before this check so declaration order does not affect it.
pub(crate) fn check_deltas(program: &Program, diagnostics: &mut Diagnostics) {
    for module in &program.modules {
        // TYPE-16/18: atomic, result and window normalization may have
        // erased the Delta node. Check its saved operand after fields resolve.
        let mut rejected: Vec<_> = module
            .delta_inputs
            .iter()
            .filter(|input| program.holds_type_value(&input.payload))
            .map(|input| input.at)
            .collect();
        for written in &module.written {
            if written.ty.any_part(
                &|part| matches!(part, Type::Delta(payload) if program.holds_type_value(payload)),
                &|_| false,
            ) && !rejected.iter().any(|at| {
                at.file == written.at.file
                    && written.at.span.start <= at.span.start
                    && at.span.end <= written.at.span.end
            }) {
                // Keep checking normalized types too, without reporting a
                // containing type again for an explicit application above.
                rejected.push(written.at);
            }
        }
        let files: HashMap<_, _> = module
            .files
            .iter()
            .enumerate()
            .map(|(index, file)| (file.file, index))
            .collect();
        rejected.sort_by_key(|at| (files[&at.file], at.span.start, at.span.end));
        rejected.dedup();
        for at in rejected {
            diagnostics.emit(crate::error(
                Category::Shape,
                crate::codes::DELTA_UNSUPPORTED_SHAPE,
                at,
                "a value that holds a `Type` has no temporal form and no delta".to_string(),
                "TYPE-16",
            ));
        }
    }
}

/// Which generic input a struct stores, directly or through another struct.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Stored {
    Parameter(Location),
    PackMember(Location, usize),
}

#[derive(Clone, Default, PartialEq, Eq)]
struct StoredTypes {
    type_value: bool,
    parameters: HashSet<Stored>,
}

impl StoredTypes {
    fn include(&mut self, other: Self) {
        self.type_value |= other.type_value;
        self.parameters.extend(other.parameters);
    }

    fn of(ty: &Type, program: &Program, structs: &HashMap<(usize, usize), Self>) -> Self {
        let mut result = Self::default();
        let mut add = |ty: &Type| result.include(Self::of(ty, program, structs));
        match ty {
            Type::TypeValue => result.type_value = true,
            Type::Parameter { at, .. } => {
                result.parameters.insert(Stored::Parameter(*at));
            }
            Type::PackMember { pack, key, .. } => {
                if let ConstantValue::Int(index) = key.value
                    && let Ok(index) = usize::try_from(index)
                {
                    result.parameters.insert(Stored::PackMember(*pack, index));
                }
            }
            Type::Named {
                module,
                declaration,
                arguments,
                kind: NamedKind::Struct,
                ..
            } => {
                let Some(info) = structs.get(&(*module, *declaration)) else {
                    return result;
                };
                result.type_value = info.type_value;
                let Some(Detail::Struct(read)) = program.modules[*module].details.get(*declaration)
                else {
                    return result;
                };
                for stored in &info.parameters {
                    let at = match stored {
                        Stored::Parameter(at) | Stored::PackMember(at, _) => at,
                    };
                    let Some(slot) = read.generics.iter().position(|p| p.at == *at) else {
                        continue;
                    };
                    let range = match stored {
                        Stored::PackMember(_, index) => slot + index..slot + index + 1,
                        Stored::Parameter(_)
                            if matches!(read.generics[slot].kind, GenericKind::Pack { .. }) =>
                        {
                            slot..arguments.len()
                        }
                        Stored::Parameter(_) => slot..slot + 1,
                    };
                    for argument in arguments.get(range).unwrap_or(&[]) {
                        if let Argument::Type(ty) = argument {
                            result.include(Self::of(ty, program, structs));
                        }
                    }
                }
            }
            Type::Tuple(elements) => elements.iter().for_each(&mut add),
            Type::List { element, .. }
            | Type::Set(element)
            | Type::Nullable(element)
            | Type::Atomic(element)
            | Type::Delta(element)
            | Type::Endpoint(element)
            | Type::Trace { cell: element, .. }
            | Type::RollingWindow { value: element, .. } => add(element),
            Type::Dict { key, value } => {
                add(key);
                add(value);
            }
            Type::Result { value, error } => {
                add(value);
                add(error);
            }
            Type::Function(function) => function.types().for_each(&mut add),
            Type::Named { .. }
            | Type::Scalar(_)
            | Type::Signal
            | Type::Error
            | Type::ErrorContext
            | Type::Unknown => {}
        }
        result
    }
}

impl Program {
    /// Whether an ordinary value of `ty` holds a Type (TYPE-18,
    /// Proposed). Struct storage dependencies reach a fixed point,
    /// rather than expanding recursive generic applications.
    pub fn holds_type_value(&self, ty: &Type) -> bool {
        let direct = |ty: &Type| ty.any_part(&|part| *part == Type::TypeValue, &|_| false);
        if !direct(ty)
            && !self.modules.iter().any(|m| {
                m.details.iter().any(
                    |d| matches!(d, Detail::Struct(s) if s.members.iter().any(|m| direct(&m.ty))),
                )
            })
        {
            return false;
        }
        let mut structs = HashMap::new();
        loop {
            let mut changed = false;
            for (module, unit) in self.modules.iter().enumerate() {
                for (declaration, detail) in unit.details.iter().enumerate() {
                    let Detail::Struct(read) = detail else {
                        continue;
                    };
                    let mut next = StoredTypes::default();
                    for field in &read.members {
                        next.include(StoredTypes::of(&field.ty, self, &structs));
                    }
                    if structs.get(&(module, declaration)) != Some(&next) {
                        structs.insert((module, declaration), next);
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        StoredTypes::of(ty, self, &structs).type_value
    }
}

impl Type {
    /// The canonical diagnostic spelling of a Type value (TYPE-18,
    /// FMT-5). Resolved constants are written by value rather than by
    /// the expression that supplied them.
    pub fn canonical_spelling(&self) -> String {
        let one = Type::canonical_spelling;
        let constant = |value: &crate::Constant| match &value.value {
            ConstantValue::Type(ty) => one(ty),
            value_ => value_.key().unwrap_or_else(|| value.text.clone()),
        };
        match self {
            Type::Named {
                name, arguments, ..
            } if !arguments.is_empty() => {
                let arguments = arguments
                    .iter()
                    .map(|argument| match argument {
                        Argument::Type(ty) => one(ty),
                        Argument::Constant(value) => constant(value),
                        Argument::NoResult => "none".to_string(),
                    })
                    .collect::<Vec<_>>();
                format!("{name}<{}>", arguments.join(", "))
            }
            Type::Tuple(values) => format!(
                "tuple<{}>",
                values.iter().map(one).collect::<Vec<_>>().join(", ")
            ),
            Type::List { element, size } => match size {
                Some(size) if !matches!(size.value, ConstantValue::Unbounded) => {
                    format!("list<{}, {}>", one(element), constant(size))
                }
                _ => format!("list<{}>", one(element)),
            },
            Type::Set(element) => format!("set<{}>", one(element)),
            Type::Dict { key, value } => format!("dict<{}, {}>", one(key), one(value)),
            Type::RollingWindow {
                value,
                maximum,
                minimum,
            } => {
                let maximum_text = constant(maximum);
                match minimum {
                    Some(minimum) if minimum.value.same(&maximum.value) != Some(true) => {
                        format!(
                            "rolling_window<{}, {maximum_text}, {}>",
                            one(value),
                            constant(minimum)
                        )
                    }
                    _ => format!("rolling_window<{}, {maximum_text}>", one(value)),
                }
            }
            Type::Atomic(payload) => {
                let normalized = Type::atomic((**payload).clone());
                if &normalized != self {
                    one(&normalized)
                } else {
                    format!("atomic<{}>", one(payload))
                }
            }
            Type::Delta(payload) => {
                let normalized = Type::delta((**payload).clone());
                if &normalized != self {
                    one(&normalized)
                } else {
                    format!("delta<{}>", one(payload))
                }
            }
            Type::Nullable(payload) => match **payload {
                Type::Nullable(_) => one(payload),
                _ => format!("{}?", one(payload)),
            },
            Type::Endpoint(payload) => format!("endpoint<{}>", one(payload)),
            Type::Result { value, error } => format!("result<{}, {}>", one(value), one(error)),
            _ => self.to_string(),
        }
    }

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
