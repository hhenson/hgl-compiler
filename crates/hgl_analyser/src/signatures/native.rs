//! NATIVE-2 checks that need the completed types of native signatures.
//! Declaration collection has already checked name and role.

use hgl_diagnostics::{Category, Diagnostics};

use crate::declared::{same_type, substitute_within};
use crate::program::{DeclarationKind, Program};
use crate::types::{Argument, Detail, GenericKind, Signature, Type};
use crate::{codes, error};

/// A completing part must match one of the signatures of its name and role.
pub(super) fn check(program: &Program, diagnostics: &mut Diagnostics) {
    for module in &program.modules {
        for (part, detail) in module.declarations.iter().zip(&module.details) {
            let Detail::Callable(written) = detail else {
                continue;
            };
            if !matches!(
                part.kind,
                DeclarationKind::NativeImplementation(_)
                    | DeclarationKind::NativeCandidateImplementation(_)
            ) {
                continue;
            }
            let declarations = module.declarations.iter().zip(&module.details).enumerate();
            let mut mismatch = None;
            for (index, (public, detail)) in declarations {
                if part.name != public.name {
                    continue;
                }
                let completes = match (part.kind, public.kind) {
                    (
                        DeclarationKind::NativeImplementation(role),
                        DeclarationKind::NativeFunction(of),
                    ) => {
                        role == of
                            && module
                                .scope
                                .get(&part.name)
                                .is_some_and(|bound| bound.contains(&index))
                    }
                    (
                        DeclarationKind::NativeCandidateImplementation(role),
                        DeclarationKind::NativeCandidate(of, _),
                    ) => role == of,
                    _ => false,
                };
                let (true, Detail::Callable(contract)) = (completes, detail) else {
                    continue;
                };
                let Some(reason) = difference(contract, written) else {
                    mismatch = None;
                    break;
                };
                mismatch.get_or_insert((public.at, reason));
            }
            if let Some((at, reason)) = mismatch {
                diagnostics.emit(
                    error(
                        Category::Name,
                        codes::NATIVE_MISSING_DECLARATION,
                        part.at,
                        format!(
                            "native implementation `{}` completes no matching declaration",
                            part.name
                        ),
                        "NATIVE-2",
                    )
                    .with_related(at, reason),
                );
            }
        }
    }
}

/// A known difference between the implementation and a public signature.
/// Constraints still need their own equivalence check. Defaults belong to the
/// public declaration and are outside this comparison.
fn difference(contract: &Signature, written: &Signature) -> Option<String> {
    if contract.generics.len() != written.generics.len() {
        return Some("this declaration takes a different number of generic parameters".into());
    }
    // GEN-8 supplies type arguments by position. Compare their uses in
    // the declaration's names and locations, independent of how the
    // provider names a type parameter.
    let generics = contract.generics.iter().zip(&written.generics);
    let renamed: Vec<_> = generics
        .clone()
        .filter(|(public, part)| {
            !matches!(public.kind, GenericKind::Value(_))
                && !matches!(part.kind, GenericKind::Value(_))
        })
        .map(|(public, part)| {
            (
                part.at,
                Argument::Type(Type::Parameter {
                    name: public.name.clone(),
                    at: public.at,
                }),
            )
        })
        .collect();
    let map: Vec<_> = renamed
        .iter()
        .map(|(at, argument)| (*at, argument))
        .collect();
    let types_differ = |public: &Type, part: &Type| {
        let dependent = |ty: &Type| {
            ty.any_part(
                &|ty| {
                    matches!(ty, Type::PackMember { .. })
                        || matches!(ty, Type::Function(template) if template.named_pack.is_some())
                },
                &|_| false,
            )
        };
        if dependent(public) || dependent(part) {
            return false;
        }
        let same = if map.is_empty() {
            same_type(public, part)
        } else {
            same_type(public, &substitute_within(part, &map))
        };
        same == Some(false)
    };
    for (public, part) in generics {
        let differs = match (&public.kind, &part.kind) {
            (GenericKind::Value(public), GenericKind::Value(part)) => types_differ(public, part),
            (GenericKind::Plain | GenericKind::Type, GenericKind::Plain | GenericKind::Type) => {
                false
            }
            (public, part) => public != part,
        };
        if differs {
            return Some(format!(
                "generic parameter `{}` has a different kind",
                public.name
            ));
        }
    }
    if contract.parameters.len() != written.parameters.len() {
        return Some("this declaration takes a different number of parameters".into());
    }
    for (public, part) in contract.parameters.iter().zip(&written.parameters) {
        if public.name != part.name {
            return Some(format!("this parameter is named `{}` here", public.name));
        }
        if public.var != part.var || public.reference != part.reference {
            return Some(format!(
                "parameter `{}` has a different `var` or `ref` mark",
                public.name
            ));
        }
        if public.configuration != part.configuration
            || public.constant != part.constant
            || public.pack != part.pack
            || public.pack_bounds != part.pack_bounds
        {
            return Some(format!("parameter `{}` has a different form", public.name));
        }
        if types_differ(&public.ty, &part.ty) {
            return Some(format!(
                "parameter `{}` has type `{}` here",
                public.name, public.ty
            ));
        }
    }
    match (&contract.result, &written.result) {
        _ if contract.reference != written.reference => {
            Some("this result has a different `ref` mark".into())
        }
        (Some(public), Some(part)) if types_differ(public, part) => {
            Some(format!("this declaration returns `{public}`"))
        }
        (None, Some(_)) => Some("this declaration has no result".into()),
        (Some(public), None) => Some(format!("this declaration returns `{public}`")),
        _ => None,
    }
}
