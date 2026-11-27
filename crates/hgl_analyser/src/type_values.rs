//! Type identities held as ordinary constants (TYPE-18, GEN-13).
//! Part of analysis; display spelling is separate from identity so
//! declarations with the same name in different modules stay distinct.

use std::collections::{HashMap, HashSet, VecDeque};

use hgl_diagnostics::{Category, Diagnostics, Location};
use hgl_parser::{HasSpan, SyntaxKind};

use crate::{
    Argument, ConstantValue, Detail, GenericKind, GenericParameter, NamedKind, Program, Type,
};

/// The existing generic-instance or type-size bound stopped a storage query.
/// This is a compiler limit, never evidence that a type stores no Type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypeStorageLimit;

/// Parameters in a deferred integer key, using the ordinary constant walker.
fn key_parameters(key: &ConstantValue) -> HashSet<Location> {
    let found = std::cell::RefCell::new(HashSet::new());
    key.substituted(&|at| {
        found.borrow_mut().insert(at);
        None
    });
    found.into_inner()
}

fn holds_or_report(
    program: &Program,
    ty: &Type,
    at: Location,
    diagnostics: &mut Diagnostics,
) -> bool {
    match program.holds_type_value(ty) {
        Ok(holds) => holds,
        Err(_) => {
            if !diagnostics.as_slice().iter().any(|report| {
                report.primary == at
                    && report.code
                        == hgl_diagnostics::Code::Catalogued(crate::codes::INSTANCE_LIMIT)
            }) {
                diagnostics.emit(crate::error(Category::Type, crate::codes::INSTANCE_LIMIT, at,
                    format!("checking stored generic types exceeds the compiler's {}-instance or type-size bound", crate::INSTANCES), "GEN-13"));
            }
            false
        }
    }
}

/// Generic parameters in the declaration's written order.
fn parameters(program: &Program, (module, declaration): (usize, usize)) -> &[GenericParameter] {
    match &program.modules[module].details[declaration] {
        Detail::Callable(signature) => &signature.generics,
        Detail::Struct(read) => &read.generics,
        _ => &[],
    }
}

/// TYPE-18 and GEN-7: a generic cold query needs the ordinary source reader
/// again with its concrete inputs. Its operand's calls never enter runtime
/// HIR, so placement cannot recover a newly selected result from those calls.
pub(crate) fn has_instance_query(program: &Program, declaration: (usize, usize)) -> bool {
    // A field, formal or result can carry another declaration's query into a
    // fit even where this body writes no query syntax of its own (TYPE-18).
    if program.deferred_type_fits.contains(&declaration) {
        return true;
    }
    matches!(
        &program.modules[declaration.0].details[declaration.1],
        Detail::Callable(signature) if !signature.generics.is_empty()
    ) && program.modules[declaration.0].declarations[declaration.1]
        .syntax
        .descendants()
        .any(|node| node.kind() == SyntaxKind::TypeOfExpr)
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
            .filter(|input| holds_or_report(program, &input.payload, input.at, diagnostics))
            .map(|input| input.at)
            .collect();
        for written in &module.written {
            let limits = std::cell::RefCell::new(Diagnostics::new());
            let forbidden = written.ty.any_part(
                &|part| matches!(part, Type::Delta(payload) if holds_or_report(program, payload, written.at, &mut limits.borrow_mut())),
                &|_| false,
            );
            for report in limits.into_inner().as_slice() {
                if !diagnostics.as_slice().contains(report) {
                    diagnostics.emit(report.clone());
                }
            }
            if forbidden
                && !rejected.iter().any(|at| {
                    at.file == written.at.file
                        && written.at.span.start <= at.span.start
                        && at.span.end <= written.at.span.end
                })
            {
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

/// Declarations whose deltas, cold queries or deferred Type parameter names
/// need checking once a making call supplies their kinds (GEN-7/13, TYPE-16/18).
/// Feed the existing bounded instance traversal rather than walking calls again.
pub(crate) fn instance_declarations(program: &Program) -> Vec<(usize, usize)> {
    let mut declarations = Vec::new();
    for (module, unit) in program.modules.iter().enumerate() {
        for declaration in 0..unit.declarations.len() {
            if has_instance_query(program, (module, declaration))
                || parameters(program, (module, declaration))
                    .iter()
                    .any(|generic| generic.name == "Type" && generic.kind == GenericKind::Plain)
                || declaration_deltas(program, (module, declaration)).any(|input| {
                    input.payload.any_part(
                        &|part| matches!(part, Type::Parameter { .. } | Type::PackMember { .. }),
                        &|constant| constant.value.key().is_none(),
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
        if holds_or_report(program, &input.payload, input.at, diagnostics) {
            // The declaration already reported this concrete forbidden operand.
            continue;
        }
        if project(&input.payload)
            .is_some_and(|payload| holds_or_report(program, &payload, input.at, diagnostics))
        {
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct StoredTypes {
    type_value: bool,
    parameters: HashSet<Location>,
    indexed: bool,
    keys: HashSet<Location>,
}

impl StoredTypes {
    fn include(&mut self, other: Self) {
        self.type_value |= other.type_value;
        self.parameters.extend(other.parameters);
        self.indexed |= other.indexed;
        self.keys.extend(other.keys);
    }

    fn of(ty: &Type, program: &Program, structs: &HashMap<(usize, usize), Self>) -> Self {
        let mut result = Self::default();
        let mut add = |ty: &Type| result.include(Self::of(ty, program, structs));
        match ty {
            Type::TypeValue => result.type_value = true,
            Type::Parameter { at, .. } => {
                result.parameters.insert(*at);
            }
            Type::PackMember { key, .. } => {
                // Retain conditional storage instead of throwing away a key
                // whose generic value is supplied by a later application.
                result.indexed = true;
                result.keys.extend(key_parameters(&key.value));
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
                result.indexed = info.indexed;
                let Some(Detail::Struct(read)) = program.modules[*module].details.get(*declaration)
                else {
                    return result;
                };
                for key in &info.keys {
                    if let Some(slot) = read.generics.iter().position(|generic| generic.at == *key)
                        && let Some(Argument::Constant(argument)) = arguments.get(slot)
                    {
                        result.keys.extend(key_parameters(&argument.value));
                    }
                }
                for at in &info.parameters {
                    let Some(slot) = read.generics.iter().position(|p| p.at == *at) else {
                        continue;
                    };
                    let range = if matches!(read.generics[slot].kind, GenericKind::Pack { .. }) {
                        slot..arguments.len()
                    } else {
                        slot..slot + 1
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

    /// Check indexed dependencies at the application, retaining source key
    /// expressions in the cached fields rather than growing them in a fixed point.
    fn holds(&self, program: &Program, ty: &Type) -> Result<bool, TypeStorageLimit> {
        let mut pending = VecDeque::from([ty.clone()]);
        let mut seen = HashSet::new();
        let mut unbound = HashSet::new();
        let mut instances = 0;
        while let Some(ty) = pending.pop_front() {
            let summary = StoredTypes::of(&ty, program, &self.structs);
            // This fact comes only from unconditional fields. Indexed
            // members never turn an entire forwarded pack into stored data.
            if summary.type_value {
                return Ok(true);
            }
            if !summary.indexed {
                continue;
            }
            match ty {
                Type::Named {
                    name,
                    module,
                    declaration,
                    arguments,
                    kind: NamedKind::Struct,
                } => {
                    let Detail::Struct(read) = &program.modules[module].details[declaration] else {
                        continue;
                    };
                    // Only values that choose stored pack members distinguish
                    // storage states. An unused identity tag cannot grow a cycle.
                    let mut state = arguments.clone();
                    for (generic, argument) in read.generics.iter().zip(&mut state) {
                        if matches!(generic.kind, GenericKind::Value(_))
                            && !self.structs[&(module, declaration)]
                                .keys
                                .contains(&generic.at)
                        {
                            *argument = Argument::Constant(crate::Constant {
                                text: "0".to_string(),
                                value: ConstantValue::Int(0),
                            });
                        }
                    }
                    // Reuse the existing GEN-13 depth and written-size
                    // guard before computing or following another state.
                    let key = crate::values::bounded_instance_key(
                        (module, declaration),
                        &state,
                        &[],
                        &[],
                    )
                    .ok_or(TypeStorageLimit)?;
                    let state = Type::Named {
                        name,
                        module,
                        declaration,
                        arguments: state,
                        kind: NamedKind::Struct,
                    };
                    if state.identity().is_some() {
                        if !seen.insert(key) {
                            continue;
                        }
                    } else if !unbound.insert((module, declaration)) {
                        // GEN-7: unresolved generic applications are checked
                        // again by the making instance with concrete arguments.
                        continue;
                    }
                    instances += 1;
                    if instances > crate::INSTANCES {
                        return Err(TypeStorageLimit);
                    }
                    let map: Vec<_> = read
                        .generics
                        .iter()
                        .take_while(|generic| !matches!(generic.kind, GenericKind::Pack { .. }))
                        .map(|generic| generic.at)
                        .zip(&arguments)
                        .collect();
                    let slot = |at| {
                        read.generics.iter().position(|generic| {
                            generic.at == at && matches!(generic.kind, GenericKind::Pack { .. })
                        })
                    };
                    let pack = |at| -> Option<Vec<Type>> {
                        arguments
                            .get(slot(at)?..)?
                            .iter()
                            .map(|argument| match argument {
                                Argument::Type(ty) => Some(ty.clone()),
                                _ => None,
                            })
                            .collect()
                    };
                    let member = |at, _: &str, key: &crate::Constant| {
                        let ConstantValue::Int(index) = key.value else {
                            return None;
                        };
                        let index = slot(at)?.checked_add(usize::try_from(index).ok()?)?;
                        match arguments.get(index)? {
                            Argument::Type(ty) => Some(ty.clone()),
                            _ => None,
                        }
                    };
                    let weight = arguments
                        .iter()
                        .map(|argument| match argument {
                            Argument::Type(ty) => crate::declared::weight(ty),
                            _ => 1,
                        })
                        .sum::<usize>()
                        .max(1);
                    for field in &read.members {
                        pending.push_back(
                            crate::declared::substitute_storage(
                                &field.ty, &map, &member, &pack, weight,
                            )
                            .ok_or(TypeStorageLimit)?,
                        );
                    }
                }
                Type::Tuple(types) => pending.extend(types),
                Type::List { element, .. }
                | Type::Set(element)
                | Type::Nullable(element)
                | Type::Atomic(element)
                | Type::Delta(element)
                | Type::Endpoint(element)
                | Type::Trace { cell: element, .. }
                | Type::RollingWindow { value: element, .. } => pending.push_back(*element),
                Type::Dict { key, value } => {
                    pending.push_back(*key);
                    pending.push_back(*value);
                }
                Type::Result { value, error } => {
                    pending.push_back(*value);
                    pending.push_back(*error);
                }
                Type::Function(function) => pending.extend(function.types().cloned()),
                _ => {}
            }
        }
        Ok(false)
    }
}

impl Program {
    /// Whether an ordinary value of `ty` is known to hold a Type (TYPE-18,
    /// Proposed). The declared fields are complete before the first query;
    /// reuse their fixed point for every type and every emitted module.
    /// Unresolved generic keys are deferred to the making instance.
    /// Returns an error if the existing generic-instance or type-size limit
    /// prevents a decision. Callers must report or decline that case.
    pub fn holds_type_value(&self, ty: &Type) -> Result<bool, TypeStorageLimit> {
        let storage = self
            .type_value_storage
            .get_or_init(|| TypeStorage::new(self));
        storage.holds(self, ty)
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
                    Some(minimum) if minimum.same_value(maximum) != Some(true) => {
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
