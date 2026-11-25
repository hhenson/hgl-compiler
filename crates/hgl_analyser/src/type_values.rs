//! Type identities held as ordinary constants (TYPE-18, GEN-13).
//! Part of analysis; display spelling is separate from identity so
//! declarations with the same name in different modules stay distinct.

use std::collections::{HashMap, HashSet};

use hgl_diagnostics::{Category, Diagnostics, Location};
use hgl_parser::HasSpan;

use crate::{
    Argument, ConstantValue, Detail, GenericKind, GenericParameter, NamedKind, Program, Type,
};

/// Generic parameters in the declaration's written order.
fn parameters(program: &Program, (module, declaration): (usize, usize)) -> &[GenericParameter] {
    match &program.modules[module].details[declaration] {
        Detail::Callable(signature) => &signature.generics,
        Detail::Struct(read) => &read.generics,
        _ => &[],
    }
}

/// TYPE-18 and GEN-13: an unmarked function parameter may be a value named
/// Type. When its making call supplies a type kind, reject at its name.
pub(crate) fn instance_parameter_names(
    program: &Program,
    declaration: (usize, usize),
    arguments: &[Argument],
) -> Vec<hgl_diagnostics::Diagnostic> {
    parameters(program, declaration)
        .iter()
        .zip(arguments)
        .filter(|(generic, argument)| {
            generic.name == "Type"
                && generic.kind == GenericKind::Plain
                && (matches!(argument, Argument::Type(ty) if !matches!(ty, Type::Unknown))
                    || matches!(argument, Argument::NoResult))
        })
        .map(|(generic, _)| {
            crate::error(
                Category::Name,
                crate::codes::NAME_RESERVED,
                generic.at,
                "the type parameter name `Type` is reserved for the descriptor type".to_string(),
                "TYPE-18",
            )
        })
        .collect()
}

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

/// Raw delta applications belonging to this declaration, including local types.
fn declaration_deltas(
    program: &Program,
    (module, declaration): (usize, usize),
) -> impl Iterator<Item = &crate::types::DeltaInput> {
    let unit = &program.modules[module];
    let declared = &unit.declarations[declaration];
    let span = declared.syntax.span();
    unit.delta_inputs.iter().filter(move |input| {
        input.at.file == declared.at.file
            && span.start <= input.at.span.start
            && input.at.span.end <= span.end
    })
}

/// Declarations whose saved delta operands or deferred Type parameter names
/// need checking once a making call supplies their kinds (GEN-7/13, TYPE-16/18).
/// Feed the existing bounded instance traversal rather than walking calls again.
pub(crate) fn instance_declarations(program: &Program) -> Vec<(usize, usize)> {
    let mut declarations = Vec::new();
    for (module, unit) in program.modules.iter().enumerate() {
        for declaration in 0..unit.declarations.len() {
            if parameters(program, (module, declaration))
                .iter()
                .any(|generic| generic.name == "Type" && generic.kind == GenericKind::Plain)
                || declaration_deltas(program, (module, declaration)).any(|input| {
                    input.payload.any_part(
                        &|part| matches!(part, Type::Parameter { .. } | Type::PackMember { .. }),
                        &|_| false,
                    )
                })
            {
                declarations.push((module, declaration));
            }
        }
    }
    declarations
}

/// TYPE-16/18, Proposed: substitution cannot create a delta of a Type-containing
/// value, even where normalization erased the Delta node at its declaration.
/// `project` is the instance traversal's ordinary generic/pack substitution.
pub(crate) fn instance_deltas(
    program: &Program,
    declaration: (usize, usize),
    project: impl Fn(&Type) -> Option<Type>,
    diagnostics: &mut Diagnostics,
) {
    for input in declaration_deltas(program, declaration) {
        if program.holds_type_value(&input.payload) {
            // The declaration already reported this concrete forbidden operand.
            continue;
        }
        if project(&input.payload).is_some_and(|payload| program.holds_type_value(&payload)) {
            diagnostics.emit(crate::error(
                Category::Shape,
                crate::codes::DELTA_UNSUPPORTED_SHAPE,
                input.at,
                "this instance gives a delta operand that holds a `Type`, which has no temporal form or delta".to_string(),
                "TYPE-16",
            ));
            // ERR-9: one invalid type suffices to reject this instance.
            break;
        }
    }
}

/// Which generic input a struct stores, directly or through another struct.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Stored {
    Parameter(Location),
    PackMember(Location, usize),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
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

/// One immutable summary of the struct fields resolved by the declaration pass.
/// Compute even generic-only storage: `Box<Type>` can hold a Type although
/// no declared field directly names Type (TYPE-18, Proposed).
#[derive(Debug, Clone)]
pub(crate) struct TypeStorage {
    structs: HashMap<(usize, usize), StoredTypes>,
}

impl TypeStorage {
    fn new(program: &Program) -> Self {
        let mut structs = HashMap::new();
        loop {
            let mut changed = false;
            for (module, unit) in program.modules.iter().enumerate() {
                for (declaration, detail) in unit.details.iter().enumerate() {
                    let Detail::Struct(read) = detail else {
                        continue;
                    };
                    let mut next = StoredTypes::default();
                    for field in &read.members {
                        next.include(StoredTypes::of(&field.ty, program, &structs));
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
        Self { structs }
    }
}

impl Program {
    /// Whether an ordinary value of `ty` holds a Type (TYPE-18,
    /// Proposed). The declared fields are complete before the first query;
    /// reuse their fixed point for every type and every emitted module.
    pub fn holds_type_value(&self, ty: &Type) -> bool {
        let storage = self
            .type_value_storage
            .get_or_init(|| TypeStorage::new(self));
        StoredTypes::of(ty, self, &storage.structs).type_value
    }
}

impl Type {
    /// The canonical diagnostic spelling of a Type value (TYPE-18,
    /// FMT-5). Resolved constants are written by value rather than by
    /// the expression that supplied them.
    pub fn canonical_spelling(&self) -> String {
        let one = Type::canonical_spelling;
        // FMT-5: display names are not identity keys. In particular an enum
        // member has its declared type's name, just as a named type does;
        // the module identity remains in ConstantValue::key for equality.
        fn value_spelling(value: &ConstantValue) -> Option<String> {
            Some(match value {
                ConstantValue::Type(ty) => ty.canonical_spelling(),
                ConstantValue::Member(member) => match &member.of {
                    Some(of) => format!(
                        "{}.{}",
                        of.written.rsplit("::").next().unwrap_or(&of.written),
                        member.name
                    ),
                    None => format!(".{}", member.name),
                },
                ConstantValue::List(values) | ConstantValue::Tuple(values) => {
                    let text = values
                        .iter()
                        .map(value_spelling)
                        .collect::<Option<Vec<_>>>()?
                        .join(", ");
                    if matches!(value, ConstantValue::List(_)) {
                        format!("[{text}]")
                    } else {
                        format!("({text})")
                    }
                }
                _ => value.key()?,
            })
        }
        let constant = |value: &crate::Constant| {
            value_spelling(&value.value).unwrap_or_else(|| value.text.clone())
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
