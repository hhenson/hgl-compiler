//! NATIVE-2 checks that need the completed types of native signatures.
//! Declaration collection has already checked name and role.

use hgl_diagnostics::{Category, Diagnostics};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode};

use crate::declared::{names_parameter, same_type, substitute_within};
use crate::program::{Declaration, DeclarationKind, Module, Program};
use crate::syntax::{child, tokens};
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
                let reason = difference(contract, written).or_else(|| {
                    finite_constraints_differ(module, (public, contract), (part, written)).then(
                        || "this declaration and its implementation admit different types".into(),
                    )
                });
                let Some(reason) = reason else {
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
/// Defaults belong to the public declaration and are outside this comparison.
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
                &|ty| match ty {
                    Type::PackMember { .. } => true,
                    // `same_type` uses structural equality for callbacks.
                    // It cannot compare symbolic values bound in different
                    // declarations, so defer those template comparisons.
                    Type::Function(template) => {
                        template.named_pack.is_some()
                            || template.types().any(|ty| {
                                ty.any_part(&|ty| matches!(ty, Type::Unknown), &|constant| {
                                    constant.value.key().is_none()
                                })
                            })
                    }
                    _ => false,
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

/// NATIVE-2: compare finite type memberships by generic parameter position
/// (GEN-8) and the types each set admits (GEN-3).
fn finite_constraints_differ(
    module: &Module,
    public: (&Declaration, &Signature),
    part: (&Declaration, &Signature),
) -> bool {
    let public_requires = child(&public.0.syntax, SyntaxKind::RequiresClause);
    let part_requires = child(&part.0.syntax, SyntaxKind::RequiresClause);
    let public_bound = public_requires
        .as_ref()
        .and_then(|requires| finite_membership(module, public, requires));
    let part_bound = part_requires
        .as_ref()
        .and_then(|requires| finite_membership(module, part, requires));
    let excludes_a_type = |a: &[&Type], b: &[&Type]| {
        a.iter()
            .any(|ty| b.iter().all(|other| same_type(ty, other) == Some(false)))
    };
    match (public_bound, part_bound) {
        (Some((public_slot, public_types)), Some((part_slot, part_types))) => {
            public_slot != part_slot
                || excludes_a_type(&public_types, &part_types)
                || excludes_a_type(&part_types, &public_types)
        }
        (Some(_), None) => part_requires.is_none(),
        (None, Some(_)) => public_requires.is_none(),
        (None, None) => false,
    }
}

/// Read the simplest finite constraint: one generic type parameter in a
/// set of concrete types. Other constraint shapes stay with the general
/// equivalence check.
fn finite_membership<'a>(
    module: &'a Module,
    (declaration, signature): (&Declaration, &Signature),
    requires: &SyntaxNode,
) -> Option<(usize, Vec<&'a Type>)> {
    let relation = requires.children().next()?;
    if relation.kind() != SyntaxKind::ConstraintRelation
        || !tokens(&relation).any(|token| token.text() == "in")
    {
        return None;
    }
    let file = module.files[declaration.file].file;
    let written_at = |node: &SyntaxNode| {
        let start = node.span().start;
        module
            .written
            .iter()
            .find(|written| written.at.file == file && written.at.span.start == start)
            .map(|written| &written.ty)
    };
    let mut operands = relation.children();
    let Type::Parameter { at, .. } = written_at(&operands.next()?)? else {
        return None;
    };
    let slot = signature
        .generics
        .iter()
        .position(|generic| generic.at == *at)?;
    let set = operands.next()?;
    if set.kind() != SyntaxKind::ConstraintSet {
        return None;
    }
    let types = set
        .children()
        .map(|member| {
            let ty = written_at(&member)?;
            (!names_parameter(ty)
                && !ty.any_part(&|part| matches!(part, Type::Unknown), &|c| {
                    c.value.key().is_none()
                }))
            .then_some(ty)
        })
        .collect::<Option<Vec<_>>>()?;
    (!types.is_empty()).then_some((slot, types))
}
