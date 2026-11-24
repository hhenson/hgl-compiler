//! GEN-6 declaration checks for a candidate's result and its operator's
//! requirements. The signature pass runs these before bodies are typed.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use hgl_diagnostics::{Category, Diagnostics, Location, Severity};
use hgl_parser::{HasSpan, SyntaxKind};
use hgl_source::FileId;

use crate::constant::{Constants, Scope};
use crate::declared::{
    names_one_of, result_types_differ, same_type, substitute_with_members, substitute_with_packs,
    substitute_within, weight,
};
use crate::program::{Declaration, DeclarationKind, Program, Role};
use crate::read::type_child;
use crate::syntax::child;
use crate::types::{
    Argument, Constant, ConstantValue, Detail, GenericKind, Pack, Parameter, ReflectedPack,
    Signature, Type,
};
use crate::{codes, error};

use super::inference::{infer, pack_bounds, type_pack};
use super::requirements::declared_requirement;

/// Report contradictions that follow from the candidate's declared types.
pub(crate) fn check_candidate_contracts(program: &Program, diagnostics: &mut Diagnostics) {
    let uses: HashMap<_, _> = program
        .modules
        .iter()
        .flat_map(|module| &module.uses)
        .map(|used| ((used.at.file, used.at.span.start), &used.binding))
        .collect();
    let constants = Constants::default();
    for module in &program.modules {
        for (candidate, detail) in module.declarations.iter().zip(&module.details) {
            let (Some((role, Some(operator))), Detail::Callable(signature)) =
                (candidate.kind.candidate(), detail)
            else {
                continue;
            };
            let declared_in = &program.modules[operator.module];
            let declared = &declared_in.declarations[operator.declaration];
            let Some(Detail::Callable(public)) = declared_in.details.get(operator.declaration)
            else {
                continue;
            };
            let shadowed = candidate.test_owner.is_some()
                && module.declarations.iter().any(|other| {
                    other.kind == DeclarationKind::Operator
                        && other.test_owner == candidate.test_owner
                        && other.name == candidate.name
                        && diagnostics.as_slice().iter().any(|diagnostic| {
                            diagnostic.primary == other.at
                                && diagnostic.code.as_str() == codes::NAME_DUPLICATE
                        })
                });
            if shadowed
                || declaration_has_error(candidate, diagnostics)
                || declaration_has_error(declared, diagnostics)
            {
                continue;
            }
            let Some(parameters) = parameter_types(public, signature, role) else {
                continue;
            };
            let bindings: Vec<_> = public
                .generics
                .iter()
                .zip(&parameters.arguments)
                .map(|(parameter, argument)| (parameter.at, argument))
                .collect();
            let expected = public.result.as_ref().map(|result| {
                result_in_candidate(result, public, signature, &bindings, &parameters)
            });
            if let Some(message) = result_error(&expected, &signature.result) {
                let at = child(&candidate.syntax, SyntaxKind::ReturnType)
                    .as_ref()
                    .and_then(type_child)
                    .map_or(candidate.at, |ty| {
                        Location::new(candidate.at.file, ty.span())
                    });
                diagnostics.emit(
                    error(Category::Operator, codes::IMPL_RESULT, at, message, "GEN-6")
                        .with_related(declared.at, "the operator's declaration"),
                );
            }
            let binding = |file: FileId, at: u32| uses.get(&(file, at)).map(|binding| &**binding);
            let parameter = |at| {
                let generic = public.generics.iter().find(|generic| generic.at == at)?;
                match &generic.kind {
                    GenericKind::Value(ty) => Some(substitute_within(ty, &bindings)),
                    _ => None,
                }
            };
            let scope = Scope {
                program,
                file: declared.at.file,
                binding: &binding,
                parameter: &parameter,
                arguments: &bindings,
                type_packs: &[],
                constants: &constants,
            };
            if declared_requirement(
                &scope,
                (operator.module, operator.declaration),
                &parameters.arguments,
            ) == Some(false)
            {
                diagnostics.emit(
                    error(
                        Category::Operator,
                        codes::IMPL_REQUIRES,
                        candidate.at,
                        "the declared parameter types lie outside the operator's `requires` set"
                            .to_string(),
                        "GEN-6",
                    )
                    .with_related(declared.at, "the operator's declaration"),
                );
            }
        }
    }
}

/// GEN-6: a candidate declares the operator's result, when this
/// declaration determines it, and declares one only if the operator does.
fn result_error(expected: &Option<Type>, actual: &Option<Type>) -> Option<String> {
    match (expected, actual) {
        (Some(expected), Some(actual)) => mismatched_result(expected, actual).then(|| {
            format!(
                "this candidate declares result `{actual}` where the operator gives these parameters result `{expected}`"
            )
        }),
        (Some(expected), None) => Some(format!(
            "this candidate declares no result where the operator gives these parameters result `{expected}`"
        )),
        (None, Some(actual)) => Some(format!(
            "this candidate declares result `{actual}` where the operator declares no result"
        )),
        _ => None,
    }
}

/// GEN-6 and GEN-17: a reflected result uses the corresponding pack
/// in the candidate's callback parameter, including a shared option key.
/// Without the pack or key, only that member's type is still open.
/// GEN-10: an exact homogeneous pack supplies its tuple elements.
fn result_in_candidate(
    result: &Type,
    public: &Signature,
    candidate: &Signature,
    bindings: &[(Location, &Argument)],
    parameters: &ParameterTypes<'_>,
) -> Type {
    let member = |at, _: &str, key: &Constant| {
        Some(
            parameters
                .packs
                .get(&at)
                .and_then(|pack| reflected_member(public, candidate, pack, key))
                .unwrap_or(Type::Unknown),
        )
    };
    let sequence = |at| {
        let pack = parameters
            .tuple_pack
            .as_ref()
            .filter(|pack| pack.at == at)?;
        Some(vec![pack.element.clone(); pack.count])
    };
    // Bound the expansion before allocating it. A pack the result does
    // not refer to cannot affect the weight of the result.
    let pack_weight = parameters
        .tuple_pack
        .as_ref()
        .filter(|pack| names_one_of(result, &[pack.at]))
        .map_or(1, |pack| pack.count.saturating_mul(weight(pack.element)));
    substitute_with_packs(result, bindings, &member, &sequence, pack_weight)
}

/// A reflected member written in the candidate's pack and option scope.
fn reflected_member(
    public: &Signature,
    candidate: &Signature,
    pack: &ReflectedPack,
    key: &Constant,
) -> Option<Type> {
    let mut key = key.clone();
    if let ConstantValue::Parameter { at, name } = &key.value
        && let Some(option) = public.generics.iter().find(|option| option.at == *at)
    {
        let given = candidate.generics.iter().find(|given| {
            given.name == *name
                && matches!((&option.kind, &given.kind),
                    (GenericKind::Value(ty), GenericKind::Value(given_ty))
                    if same_type(ty, given_ty) == Some(true))
        })?;
        key.value = ConstantValue::Parameter {
            name: given.name.clone(),
            at: given.at,
        };
    }
    if matches!(
        key.value,
        ConstantValue::Unknown | ConstantValue::Unevaluated
    ) {
        return None;
    }
    Some(Type::PackMember {
        pack: pack.at,
        name: pack.name.clone(),
        key,
    })
}

/// An earlier error in either declaration can invalidate the comparison.
fn declaration_has_error(declaration: &Declaration, diagnostics: &Diagnostics) -> bool {
    let span = declaration.syntax.span();
    diagnostics.as_slice().iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic.primary.file == declaration.at.file
            && (span.start..span.end).contains(&diagnostic.primary.span.start)
    })
}

/// Bind the operator's generics from the corresponding candidate parameters.
/// Conflicting bindings leave the contract unresolved for this declaration.
fn parameter_types<'a>(
    public: &Signature,
    candidate: &'a Signature,
    role: Role,
) -> Option<ParameterTypes<'a>> {
    let mut arguments = vec![None; public.generics.len()];
    let mut from_parameter = arguments.clone();
    let mut inputs = Vec::new();
    let mut packs = HashMap::new();
    let mut tuple_pack = None;
    for (parameter, slot) in public
        .parameters
        .iter()
        .zip(public.candidate_parameters(candidate))
    {
        let given = &candidate.parameters[slot?];
        if parameter.pack != given.pack {
            return None;
        }
        if parameter.pack == Some(Pack::Positional) {
            tuple_pack = homogeneous_candidate_pack(public, candidate, parameter, given);
        }
        // The type reader has reported this non-ordinary input (TYPE-12).
        if matches!(given.ty, Type::Signal) && (role == Role::Value || given.configuration) {
            continue;
        }
        // Both templates reflect the same suffix of this callback.
        if let (Type::Function(parameter), Type::Function(given)) = (&parameter.ty, &given.ty)
            && parameter.parameters.len() == given.parameters.len()
            && let (Some(expected), Some(actual)) = (&parameter.named_pack, &given.named_pack)
            && packs
                .insert(expected.at, actual)
                .is_some_and(|previous: &ReflectedPack| previous.at != actual.at)
        {
            return None;
        }
        infer(
            &parameter.ty,
            &given.ty,
            &public.generics,
            &mut from_parameter,
        );
        for (known, next) in arguments.iter_mut().zip(&mut from_parameter) {
            if let Some(next) = next.take() {
                match (&*known, &next) {
                    (Some(Argument::Type(left)), Argument::Type(right))
                        if same_type(left, right) == Some(true) => {}
                    (Some(Argument::Constant(left)), Argument::Constant(right))
                        if left.value.same(&right.value) == Some(true) => {}
                    (None, _) => *known = Some(next),
                    _ => return None,
                }
            }
        }
        inputs.push((&parameter.ty, &given.ty));
    }
    let bindings: Vec<_> = public
        .generics
        .iter()
        .zip(&arguments)
        .filter_map(|(generic, argument)| Some((generic.at, argument.as_ref()?)))
        .collect();
    let values: Vec<_> = bindings
        .iter()
        .filter_map(|&(at, argument)| matches!(argument, Argument::Constant(_)).then_some(at))
        .collect();
    let member = |at, _: &str, key: &Constant| {
        packs
            .get(&at)
            .and_then(|pack| reflected_member(public, candidate, pack, key))
    };
    // Inference retains the first binding within an input and does not
    // solve equations. Once all inputs have contributed, check repeats
    // and expressions before trusting the bindings for diagnostics.
    for (pattern, given) in inputs {
        if !needs_binding_check(pattern, &bindings, &values) {
            continue;
        }
        let mut bound = substitute_with_members(pattern, &bindings, &member, 1);
        // GEN-17: the corresponding callback packs have different source
        // locations but come from the same reflected suffix. Keep the
        // template's own shape and bounds when checking repeated bindings.
        if let Type::Function(callback) = &mut bound
            && let Some(pack) = &mut callback.named_pack
            && let Some(actual) = packs.get(&pack.at)
        {
            pack.at = actual.at;
            pack.name.clone_from(&actual.name);
        }
        if same_type(&bound, given) != Some(true) {
            return None;
        }
    }
    Some(ParameterTypes {
        arguments: arguments
            .into_iter()
            .map(|argument| argument.unwrap_or(Argument::Type(Type::Unknown)))
            .collect(),
        packs,
        tuple_pack,
    })
}

struct ParameterTypes<'a> {
    arguments: Vec<Argument>,
    packs: HashMap<Location, &'a ReflectedPack>,
    tuple_pack: Option<HomogeneousPack<'a>>,
}

struct HomogeneousPack<'a> {
    at: Location,
    count: usize,
    element: &'a Type,
}

/// A nonempty positional candidate pack of known cardinality can give
/// one concrete sequence of element types to a public type pack (GEN-10).
fn homogeneous_candidate_pack<'a>(
    public: &Signature,
    candidate: &Signature,
    parameter: &Parameter,
    given: &'a Parameter,
) -> Option<HomogeneousPack<'a>> {
    let at = public.generics[type_pack(parameter, &public.generics)?].at;
    let (count, maximum) = given.pack_bounds;
    let (public_minimum, public_maximum) = pack_bounds(parameter, &public.generics, None);
    if count == 0
        || maximum != Some(count)
        || count < public_minimum
        || public_maximum.is_some_and(|maximum| count > maximum)
        || type_pack(given, &candidate.generics).is_some()
    {
        return None;
    }
    Some(HomogeneousPack {
        at,
        count: usize::try_from(count).ok()?,
        element: &given.ty,
    })
}

/// True where an input repeats an inferred binding or computes a constant
/// from a bound value parameter (GEN-2, GEN-6).
fn needs_binding_check(ty: &Type, inferred: &[(Location, &Argument)], values: &[Location]) -> bool {
    let seen = RefCell::new(HashSet::new());
    let repeated =
        |at| inferred.iter().any(|&(known, _)| known == at) && !seen.borrow_mut().insert(at);
    ty.any_part(
        &|part| matches!(part, Type::Parameter { at, .. } if repeated(*at)),
        &|constant| constant_needs_binding_check(&constant.value, &repeated, values),
    )
}

/// A literal composite can contain direct bindings as well as expressions
/// that use a binding inferred elsewhere.
fn constant_needs_binding_check(
    value: &ConstantValue,
    repeated: &dyn Fn(Location) -> bool,
    values: &[Location],
) -> bool {
    match value {
        ConstantValue::Parameter { at, .. } => repeated(*at),
        ConstantValue::List(elements) | ConstantValue::Tuple(elements) => elements
            .iter()
            .any(|element| constant_needs_binding_check(element, repeated, values)),
        _ => value.names_one_of(values),
    }
}

fn mismatched_result(expected: &Type, actual: &Type) -> bool {
    result_types_differ(expected, actual)
        // GEN-6 leaves wrapper-only differences open. The payload still
        // establishes a contradiction when it is known to differ.
        && result_types_differ(result_payload(expected), result_payload(actual))
}

fn result_payload(mut ty: &Type) -> &Type {
    loop {
        ty = match ty {
            Type::Nullable(value) => value,
            Type::Result { value, error } if **error == Type::Error => value,
            _ => return ty,
        }
    }
}
