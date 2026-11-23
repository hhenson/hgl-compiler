//! GEN-6 declaration checks for a candidate's result and its operator's
//! type requirements. The signature pass runs these before bodies are typed.

use std::cell::RefCell;
use std::collections::HashSet;

use hgl_diagnostics::{Category, Diagnostics, Location};
use hgl_parser::{HasSpan, SyntaxKind};

use crate::declared::{same_type, substitute_within};
use crate::program::{Program, Role};
use crate::read::type_child;
use crate::syntax::child;
use crate::types::{Argument, ConstantValue, Detail, Signature, Type};
use crate::{codes, error};

use super::inference::infer;
use super::requirements::declared_type_requirement;

/// Report contradictions that follow from the candidate's declared types.
pub(crate) fn check_candidate_contracts(program: &Program, diagnostics: &mut Diagnostics) {
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
            let Some(arguments) = parameter_types(public, signature, role) else {
                continue;
            };
            if let (Some(expected), Some(actual), Some(written)) = (
                &public.result,
                &signature.result,
                child(&candidate.syntax, SyntaxKind::ReturnType)
                    .as_ref()
                    .and_then(type_child),
            ) {
                let bindings: Vec<_> = public
                    .generics
                    .iter()
                    .zip(&arguments)
                    .map(|(parameter, argument)| (parameter.at, argument))
                    .collect();
                let expected = substitute_within(expected, &bindings);
                if mismatched_result(&expected, actual) {
                    diagnostics.emit(
                        error(
                            Category::Operator,
                            codes::IMPL_RESULT,
                            Location::new(candidate.at.file, written.span()),
                            format!(
                                "this candidate declares result `{actual}` where the operator gives these parameters result `{expected}`"
                            ),
                            "GEN-6",
                        )
                        .with_related(declared.at, "the operator's declaration"),
                    );
                }
            }
            if declared_type_requirement(
                program,
                (operator.module, operator.declaration),
                &arguments,
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

/// Bind the operator's generics from the corresponding candidate parameters.
/// Conflicting bindings leave the contract unresolved for this declaration.
fn parameter_types(public: &Signature, candidate: &Signature, role: Role) -> Option<Vec<Argument>> {
    let mut arguments = vec![None; public.generics.len()];
    let mut from_parameter = arguments.clone();
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
        let inferred: Vec<_> = public
            .generics
            .iter()
            .zip(&from_parameter)
            .filter_map(|(generic, argument)| Some((generic.at, argument.as_ref()?)))
            .collect();
        // Inference retains the first binding within a parameter. Check
        // that the known parts agree when a binding is used more than
        // once before using it for a declaration diagnostic.
        if repeated_binding(&parameter.ty, &inferred)
            && same_type(&substitute_within(&parameter.ty, &inferred), &given.ty) != Some(true)
        {
            return None;
        }
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
    }
    Some(
        arguments
            .into_iter()
            .map(|argument| argument.unwrap_or(Argument::Type(Type::Unknown)))
            .collect(),
    )
}

fn repeated_binding(ty: &Type, inferred: &[(Location, &Argument)]) -> bool {
    let seen = RefCell::new(HashSet::new());
    let repeated =
        |at| inferred.iter().any(|&(known, _)| known == at) && !seen.borrow_mut().insert(at);
    ty.any_part(
        &|part| matches!(part, Type::Parameter { at, .. } if repeated(*at)),
        &|constant| matches!(&constant.value, ConstantValue::Parameter { at, .. } if repeated(*at)),
    )
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
