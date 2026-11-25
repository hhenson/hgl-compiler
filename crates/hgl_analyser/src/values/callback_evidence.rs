//! Type evidence a positive membership gives a bare callback (GEN-3, FN-17).
//! Part of the analyser's typing pass for static function arguments.

use hgl_diagnostics::Location;
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode};

use super::Candidate;
use super::candidates::alternative_outranks;
use super::dependent_packs::dependent_slot;
use super::fit::whole;
use super::inference::{infer, type_pack};
use super::requirements::{conjoined_terms, pack_view};
use crate::declared::{
    same_type, substitute_with_members, substitute_with_packs, substitute_within, weight,
};
use crate::syntax::{child, tokens};
use crate::types::{Argument, ConstantValue, GenericKind, GenericParameter, Type};
use crate::{Program, ReflectedPack};

/// Selection evidence only. The enclosing call still receives the original argument.
#[derive(Default)]
pub(super) struct CallbackEvidence(Vec<Membership>);

impl CallbackEvidence {
    /// Does any positive membership give a type to this callback?
    pub(super) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(super) fn from_callables<'a, 'b>(
        program: &Program,
        callables: impl IntoIterator<Item = (Candidate<'a>, &'b [(Location, &'b Argument)])>,
    ) -> Self {
        let mut evidence = Self::default();
        for ((module, declaration, signature), bindings) in callables {
            let fixed: Vec<_> = bindings
                .iter()
                .copied()
                .filter(|(_, value)| match value {
                    Argument::Type(ty) => whole(ty),
                    Argument::Constant(constant) => {
                        !matches!(constant.value, ConstantValue::Unknown)
                    }
                    Argument::NoResult => true,
                })
                .collect();
            let unit = &program.modules[module];
            let Some(slot) = dependent_slot(unit, signature) else {
                continue;
            };
            let supplied = &signature.parameters[slot];
            let Some(source) = type_pack(supplied, &signature.generics) else {
                continue;
            };
            let source = &signature.generics[source];
            let callback = unit.dependent_packs[&supplied.at].function;
            let Some(target) = signature.parameters.iter().find_map(|parameter| {
                if parameter.at == callback
                    && let Type::Function(template) = &parameter.ty
                {
                    template.named_pack.as_ref()
                } else {
                    None
                }
            }) else {
                continue;
            };
            let declared = &unit.declarations[declaration];
            let Some(condition) = child(&declared.syntax, SyntaxKind::RequiresClause)
                .and_then(|clause| clause.children().next())
            else {
                continue;
            };
            let type_at = |node: &SyntaxNode| {
                let at = Location::new(declared.at.file, node.span());
                unit.written
                    .iter()
                    .find(|ty| ty.at == at)
                    .map(|ty| substitute_within(&ty.ty, &fixed))
            };
            let mut terms = Vec::new();
            conjoined_terms(
                &condition,
                &|kind| {
                    matches!(
                        kind,
                        SyntaxKind::ConstraintRelation | SyntaxKind::ConstraintEach
                    )
                },
                &mut terms,
            );
            let mut relations = Vec::new();
            for term in terms {
                if term.kind() == SyntaxKind::ConstraintRelation {
                    relations.push((term, None));
                    continue;
                }
                let mut parts = term.children();
                let (Some((view, name)), Some(body)) =
                    (parts.next().and_then(pack_view), parts.next())
                else {
                    continue;
                };
                if view.text() != "arg_keys" || name.text() != source.name {
                    continue;
                }
                let Some(key) = tokens(&term)
                    .filter(|token| token.kind() == SyntaxKind::Ident)
                    .nth(1)
                else {
                    continue;
                };
                let key = Location::new(declared.at.file, key.span());
                let mut members = Vec::new();
                conjoined_terms(
                    &body,
                    &|kind| kind == SyntaxKind::ConstraintRelation,
                    &mut members,
                );
                relations.extend(members.into_iter().map(|relation| (relation, Some(key))));
            }
            let Some(mut variable) = signature
                .generics
                .iter()
                .find(|parameter| parameter.at == target.at)
                .cloned()
            else {
                continue;
            };
            // One pack member is the type variable we solve for in this pattern.
            // Give it a separate identity from the declaration of the whole pack.
            variable.at = Location::new(declared.at.file, condition.span());
            variable.kind = GenericKind::Type;
            let mut generics = signature.generics.clone();
            generics.push(variable);
            evidence
                .0
                .extend(relations.iter().filter_map(|(relation, bound)| {
                    Membership::read(relation, *bound, source, target, &generics, &type_at)
                }));
        }
        evidence
    }

    /// Select among compatible alternatives within each positive
    /// membership (GEN-3). A winner that gives no binding leaves the
    /// input open for other type evidence.
    pub(super) fn resolve(
        &self,
        name: Option<&str>,
        position: Option<usize>,
        actual: &Type,
        compatible: impl Fn(&Type) -> bool,
    ) -> Result<Option<Type>, ()> {
        let groups: Vec<Vec<_>> = self
            .0
            .iter()
            .filter(|membership| membership.applies(name, position))
            .map(|membership| {
                membership
                    .patterns
                    .iter()
                    .filter_map(|pattern| {
                        let bindings = membership.fit(pattern, actual)?;
                        if selected_type(&bindings, membership.target)
                            .is_some_and(|ty| !compatible(ty))
                        {
                            return None;
                        }
                        Some((membership, pattern, bindings))
                    })
                    .collect()
            })
            .collect();
        let permitted = |ty: &Type| {
            groups.iter().all(|group| {
                group.iter().any(|(membership, _, bindings)| {
                    selected_type(bindings, membership.target)
                        .is_none_or(|other| same_type(ty, other) != Some(false))
                })
            })
        };
        let mut resolved = None;
        let mut nonbinding = false;
        for group in &groups {
            let admitted: Vec<_> = group
                .iter()
                .filter(|(membership, _, bindings)| {
                    selected_type(bindings, membership.target).is_none_or(permitted)
                })
                .collect();
            let mut binding = None;
            let mut open = false;
            let mut conflicting = false;
            for (membership, pattern, bindings) in &admitted {
                if admitted.iter().any(|(other, candidate, found)| {
                    alternative_outranks(
                        (candidate, &other.generics, found),
                        (pattern, &membership.generics, bindings),
                    )
                }) {
                    continue;
                }
                let Some(ty) = selected_type(bindings, membership.target) else {
                    open = true;
                    continue;
                };
                conflicting |=
                    binding.is_some_and(|previous| same_type(previous, ty) != Some(true));
                binding = Some(ty);
            }
            // This requirement supplies no binding where one of its
            // best alternatives is independent of the callback input.
            if open {
                nonbinding = true;
            } else if conflicting {
                return Err(());
            } else if let Some(ty) = binding {
                if resolved.is_some_and(|previous| same_type(previous, ty) != Some(true)) {
                    return Err(());
                }
                resolved = Some(ty);
            }
        }
        Ok(resolved
            .or_else(|| (!nonbinding).then_some(actual))
            .cloned())
    }
}

fn selected_type(bindings: &[Option<Argument>], target: usize) -> Option<&Type> {
    match &bindings[target] {
        Some(Argument::Type(ty)) => Some(ty),
        _ => None,
    }
}

struct Membership {
    key: Option<ConstantValue>,
    target: usize,
    generics: Vec<GenericParameter>,
    patterns: Vec<Type>,
}

impl Membership {
    fn read(
        relation: &SyntaxNode,
        bound: Option<Location>,
        source: &GenericParameter,
        target: &ReflectedPack,
        generics: &[GenericParameter],
        type_at: &impl Fn(&SyntaxNode) -> Option<Type>,
    ) -> Option<Self> {
        let mut operands = relation.children();
        let left = operands.next()?;
        let right = operands.next()?;
        let target_index = generics.len().checked_sub(1)?;
        let variable = generics[target_index].at;
        if right.kind() != SyntaxKind::ConstraintSet
            || !tokens(relation)
                .any(|token| token.kind() == SyntaxKind::Ident && token.text() == "in")
        {
            return None;
        }
        let Type::PackMember { pack, key, .. } = type_at(&left)? else {
            return None;
        };
        if pack != source.at
            || bound.is_some_and(
                |bound| !matches!(key.value, ConstantValue::Parameter { at, .. } if at == bound),
            )
        {
            return None;
        }
        let mut patterns = Vec::new();
        for alternative in right.children() {
            let ty = type_at(&alternative)?;
            let pattern = substitute_with_members(
                &ty,
                &[],
                &|pack, _, member| {
                    (pack == target.at && member == &key).then(|| Type::Parameter {
                        at: variable,
                        name: target.name.clone(),
                    })
                },
                1,
            );
            if whole(&pattern) {
                patterns.push(pattern);
            }
        }
        patterns
            .iter()
            .any(|pattern| {
                pattern.any_part(
                    &|ty| matches!(ty, Type::Parameter { at, .. } if *at == variable),
                    &|_| false,
                )
            })
            .then(|| Self {
                key: bound.is_none().then(|| key.value.clone()),
                target: target_index,
                generics: generics.to_vec(),
                patterns,
            })
    }

    fn applies(&self, name: Option<&str>, position: Option<usize>) -> bool {
        match &self.key {
            None => true,
            Some(ConstantValue::Str(key)) => name == Some(key),
            Some(ConstantValue::Int(key)) => {
                usize::try_from(*key).is_ok_and(|key| position == Some(key))
            }
            Some(_) => false,
        }
    }

    fn fit(&self, pattern: &Type, actual: &Type) -> Option<Vec<Option<Argument>>> {
        let mut found = vec![None; self.generics.len()];
        infer(pattern, actual, &self.generics, &mut found);
        let mut bindings = Vec::new();
        let mut packs = Vec::new();
        for (generic, value) in self.generics.iter().zip(&found) {
            let Some(value) = value else { continue };
            if let GenericKind::Pack { minimum, maximum } = generic.kind {
                let Argument::Type(Type::Tuple(types)) = value else {
                    return None;
                };
                let count = types.len() as u64;
                if count < minimum || maximum.is_some_and(|maximum| count > maximum) {
                    return None;
                }
                packs.push((generic.at, types));
            } else {
                bindings.push((generic.at, value));
            }
        }
        let pack_weight = packs
            .iter()
            .map(|(_, types)| {
                types
                    .iter()
                    .fold(1usize, |sum, ty| sum.saturating_add(weight(ty)))
            })
            .max()
            .unwrap_or(1);
        let sequence = |at| {
            packs
                .iter()
                .find_map(|(declared, types)| (*declared == at).then(|| (*types).clone()))
        };
        // Inference keeps its first binding. Verify the entire pattern so
        // repeated parameters and all fixed parts also agree.
        let fitted =
            substitute_with_packs(pattern, &bindings, &|_, _, _| None, &sequence, pack_weight);
        (same_type(&fitted, actual) == Some(true)).then_some(found)
    }
}
