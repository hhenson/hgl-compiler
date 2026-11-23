//! GEN-6 declaration checks for a candidate's result and its operator's
//! requirements. The signature pass runs these before bodies are typed.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use hgl_diagnostics::{Category, Diagnostics, Location, Severity};
use hgl_parser::{HasSpan, SyntaxKind};
use hgl_source::FileId;

use crate::constant::{Constants, Scope};
use crate::declared::{names_one_of, same_type, substitute_within};
use crate::program::{Declaration, DeclarationKind, Program, Role};
use crate::read::type_child;
use crate::syntax::child;
use crate::types::{Argument, ConstantValue, Detail, GenericKind, Signature, Type};
use crate::{codes, error};

use super::inference::infer;
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
            let Some(arguments) = parameter_types(public, signature, role) else {
                continue;
            };
            let bindings: Vec<_> = public
                .generics
                .iter()
                .zip(&arguments)
                .map(|(parameter, argument)| (parameter.at, argument))
                .collect();
            if let Some(message) = result_error(&public.result, &signature.result, &bindings) {
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
                constants: &constants,
            };
            if declared_requirement(&scope, (operator.module, operator.declaration), &arguments)
                == Some(false)
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
fn result_error(
    expected: &Option<Type>,
    actual: &Option<Type>,
    bindings: &[(Location, &Argument)],
) -> Option<String> {
    match (expected, actual) {
        (Some(expected), Some(actual)) if result_is_bound(expected, bindings) => {
            let expected = substitute_within(expected, bindings);
            mismatched_result(&expected, actual).then(|| {
                format!(
                    "this candidate declares result `{actual}` where the operator gives these parameters result `{expected}`"
                )
            })
        }
        (Some(expected), None) => {
            let expected = substitute_within(expected, bindings);
            Some(format!(
                "this candidate declares no result where the operator gives these parameters result `{expected}`"
            ))
        }
        (None, Some(actual)) => Some(format!(
            "this candidate declares result `{actual}` where the operator declares no result"
        )),
        _ => None,
    }
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

/// The operator's generic result can still depend on an argument the
/// candidate's inputs do not establish, such as an unsized list's extent.
fn result_is_bound(ty: &Type, bindings: &[(Location, &Argument)]) -> bool {
    let unbound: Vec<_> = bindings
        .iter()
        .filter_map(|&(at, argument)| match argument {
            Argument::Type(Type::Unknown) => Some(at),
            Argument::Constant(constant) if constant.value.key().is_none() => Some(at),
            _ => None,
        })
        .collect();
    !names_one_of(ty, &unbound)
        && !ty.any_part(&|_| false, &|constant| {
            constant.value.names_one_of(&unbound)
        })
}

/// Bind the operator's generics from the corresponding candidate parameters.
/// Conflicting bindings leave the contract unresolved for this declaration.
fn parameter_types(public: &Signature, candidate: &Signature, role: Role) -> Option<Vec<Argument>> {
    let mut arguments = vec![None; public.generics.len()];
    let mut from_parameter = arguments.clone();
    let mut inputs = Vec::new();
    for (parameter, slot) in public
        .parameters
        .iter()
        .zip(public.candidate_parameters(candidate))
    {
        let given = &candidate.parameters[slot?];
        if parameter.pack != given.pack {
            return None;
        }
        // The type reader has reported this non-ordinary input (TYPE-12).
        if matches!(given.ty, Type::Signal) && (role == Role::Value || given.configuration) {
            continue;
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
    // Inference retains the first binding within an input and does not
    // solve equations. Once all inputs have contributed, check repeats
    // and expressions before trusting the bindings for diagnostics.
    for (pattern, given) in inputs {
        if needs_binding_check(pattern, &bindings, &values)
            && same_type(&substitute_within(pattern, &bindings), given) != Some(true)
        {
            return None;
        }
    }
    Some(
        arguments
            .into_iter()
            .map(|argument| argument.unwrap_or(Argument::Type(Type::Unknown)))
            .collect(),
    )
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
    same_type(expected, actual) == Some(false)
        // GEN-6 leaves wrapper-only differences open. The payload still
        // establishes a contradiction when it is known to differ.
        && same_type(result_payload(expected), result_payload(actual)) == Some(false)
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
