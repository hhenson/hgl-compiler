//! Type evidence a positive membership gives a bare callback (GEN-3, FN-17).

use hgl_diagnostics::Location;
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode};

use super::Candidate;
use super::dependent_packs::dependent_slot;
use super::fit::whole;
use super::inference::{infer, type_pack};
use super::requirements::{conjoined_terms, pack_view};
use crate::declared::{same_type, substitute_with_members, substitute_within};
use crate::syntax::{child, tokens};
use crate::types::{Argument, ConstantValue, GenericKind, GenericParameter, Type};
use crate::{Program, ReflectedPack};

/// Selection evidence only. The enclosing call still receives the original argument.
#[derive(Default)]
pub(super) struct CallbackEvidence(Vec<Membership>);

impl CallbackEvidence {
    pub(super) fn from_callables<'a>(
        program: &Program,
        callables: impl IntoIterator<Item = Candidate<'a>>,
    ) -> Self {
        let mut evidence = Self::default();
        for (module, declaration, signature) in callables {
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
                unit.written.iter().find(|ty| ty.at == at).map(|ty| &ty.ty)
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
            let mut generics = signature.generics.clone();
            let Some(variable) = generics
                .iter_mut()
                .find(|parameter| parameter.at == target.at)
            else {
                continue;
            };
            // One pack member is the type variable we solve for in this pattern.
            variable.kind = GenericKind::Type;
            evidence
                .0
                .extend(relations.iter().filter_map(|(relation, bound)| {
                    Membership::read(relation, *bound, source, target, &generics, &type_at)
                }));
        }
        evidence
    }

    /// Read the most specific alternatives that fit this argument. Tied alternatives
    /// may give the same type. A conflicting binding cannot select a bare callback.
    pub(super) fn resolve(
        &self,
        name: Option<&str>,
        position: Option<usize>,
        actual: &Type,
    ) -> Result<Type, ()> {
        let mut matches = Vec::new();
        for membership in &self.0 {
            if membership.applies(name, position) {
                matches.extend(membership.patterns.iter().filter_map(|pattern| {
                    Some((membership, pattern, membership.fit(pattern, actual)??))
                }));
            }
        }
        let mut resolved = None;
        for (membership, pattern, ty) in &matches {
            let outranked = matches.iter().any(|(other, candidate, _)| {
                membership.fit(pattern, candidate).is_some()
                    && other.fit(candidate, pattern).is_none()
            });
            if outranked {
                continue;
            }
            if resolved.is_some_and(|previous| same_type(previous, ty) != Some(true)) {
                return Err(());
            }
            resolved = Some(ty);
        }
        Ok(resolved.unwrap_or(actual).clone())
    }
}

struct Membership {
    key: Option<ConstantValue>,
    target: Location,
    generics: Vec<GenericParameter>,
    patterns: Vec<Type>,
}

impl Membership {
    fn read<'a>(
        relation: &SyntaxNode,
        bound: Option<Location>,
        source: &GenericParameter,
        target: &ReflectedPack,
        generics: &[GenericParameter],
        type_at: &impl Fn(&SyntaxNode) -> Option<&'a Type>,
    ) -> Option<Self> {
        let mut operands = relation.children();
        let left = operands.next()?;
        let right = operands.next()?;
        if right.kind() != SyntaxKind::ConstraintSet
            || !tokens(relation)
                .any(|token| token.kind() == SyntaxKind::Ident && token.text() == "in")
        {
            return None;
        }
        let Type::PackMember { pack, key, .. } = type_at(&left)? else {
            return None;
        };
        if *pack != source.at
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
                ty,
                &[],
                &|pack, _, member| {
                    (pack == target.at && member == key).then(|| Type::Parameter {
                        at: target.at,
                        name: target.name.clone(),
                    })
                },
                1,
            );
            if whole(&pattern)
                && pattern.any_part(
                    &|ty| matches!(ty, Type::Parameter { at, .. } if *at == target.at),
                    &|_| false,
                )
            {
                patterns.push(pattern);
            }
        }
        (!patterns.is_empty()).then(|| Self {
            key: bound.is_none().then(|| key.value.clone()),
            target: target.at,
            generics: generics.to_vec(),
            patterns,
        })
    }

    fn applies(&self, name: Option<&str>, position: Option<usize>) -> bool {
        match &self.key {
            None => true,
            Some(ConstantValue::Str(key)) => name == Some(key),
            Some(ConstantValue::Int(key)) => position == usize::try_from(*key).ok(),
            Some(_) => false,
        }
    }

    fn fit(&self, pattern: &Type, actual: &Type) -> Option<Option<Type>> {
        let mut found = vec![None; self.generics.len()];
        infer(pattern, actual, &self.generics, &mut found);
        let bindings: Vec<_> = self
            .generics
            .iter()
            .zip(&found)
            .filter_map(|(parameter, argument)| Some((parameter.at, argument.as_ref()?)))
            .collect();
        // Inference keeps its first binding. Verify the entire pattern so
        // repeated parameters and all fixed parts also agree.
        (same_type(&substitute_within(pattern, &bindings), actual) == Some(true)).then(|| {
            bindings
                .into_iter()
                .find_map(|(at, argument)| match argument {
                    Argument::Type(ty) if at == self.target => Some(ty.clone()),
                    _ => None,
                })
        })
    }
}
