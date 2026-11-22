//! Binding equalities and existential pack constraints (GEN-3, GEN-10).
//!
//! Only positive conjuncts can contribute a binding. A trial is private until
//! both sides are known equal, so a failed earlier pack entry cannot leak
//! its tentative inference into the entry that follows.

use hgl_diagnostics::Location;
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode};

use super::dependent_packs::PackTypes;
use super::inference::infer;
use crate::declared::{names_parameter, same_type};
use crate::hir::{PackKey, TypePackArgument};
use crate::program::{Declaration, Module};
use crate::syntax::{child, has_token, tokens};
use crate::types::{Argument, Constant, ConstantValue, GenericParameter, Type};

/// Resolve the single unknown generic of a positive equality or `any`.
/// Revisit conjuncts to a fixed point, without changing supplied bindings.
pub(super) fn bind(
    unit: &Module,
    declaration: &Declaration,
    generics: &[GenericParameter],
    packs: &[TypePackArgument],
    found: &mut [Option<Argument>],
) {
    let Some(condition) = child(&declaration.syntax, SyntaxKind::RequiresClause)
        .and_then(|clause| clause.children().next())
    else {
        return;
    };
    let mut conjuncts = Vec::new();
    positive(&condition, &mut conjuncts);
    let types = PackTypes::new(generics, packs);
    for _ in 0..=generics.len() {
        let before = found.iter().filter(|argument| argument.is_some()).count();
        for condition in &conjuncts {
            if condition.kind() == SyntaxKind::ConstraintRelation {
                equality(unit, declaration, condition, generics, &types, &[], found);
                continue;
            }
            let mut parts = condition.children();
            let (Some(view), Some(body)) = (parts.next(), parts.next()) else {
                continue;
            };
            let Some((callee, pack_name)) = super::requirements::pack_view(view) else {
                continue;
            };
            if unit
                .uses
                .iter()
                .any(|used| used.at == Location::new(declaration.at.file, callee.span()))
            {
                continue;
            }
            let Some(parameter) = generics
                .iter()
                .position(|generic| generic.name == pack_name.text())
            else {
                continue;
            };
            let Some(pack) = packs.iter().find(|pack| pack.parameter == parameter) else {
                continue;
            };
            // `any` is a scalar keyword; only the bound name is an Ident.
            let Some(name) = tokens(condition).find(|token| token.kind() == SyntaxKind::Ident)
            else {
                continue;
            };
            let at = Location::new(declaration.at.file, name.span());
            for (index, ty) in pack.types.iter().enumerate() {
                let argument = if callee.text() == "types" {
                    Argument::Type(ty.clone())
                } else {
                    let value = match &pack.keys[index] {
                        PackKey::Name(name) => ConstantValue::Str(name.clone()),
                        PackKey::Position(position) => {
                            let Ok(position) = i64::try_from(*position) else {
                                continue;
                            };
                            ConstantValue::Int(position)
                        }
                    };
                    Argument::Constant(Constant {
                        text: name.text().to_string(),
                        value,
                    })
                };
                if equality(
                    unit,
                    declaration,
                    &body,
                    generics,
                    &types,
                    &[(at, &argument)],
                    found,
                ) {
                    break;
                }
            }
        }
        if found.iter().filter(|argument| argument.is_some()).count() == before {
            break;
        }
    }
}

/// GEN-3: only positive conjuncts may contribute an inferred binding.
pub(super) fn positive(node: &SyntaxNode, conjuncts: &mut Vec<SyntaxNode>) {
    match node.kind() {
        SyntaxKind::ConstraintParen => {
            if let Some(inside) = node.children().next() {
                positive(&inside, conjuncts);
            }
        }
        SyntaxKind::ConstraintBinary if has_token(node, SyntaxKind::AmpAmp) => {
            for side in node.children() {
                positive(&side, conjuncts);
            }
        }
        SyntaxKind::ConstraintRelation | SyntaxKind::ConstraintAny => conjuncts.push(node.clone()),
        _ => {}
    }
}

fn equality(
    unit: &Module,
    declaration: &Declaration,
    relation: &SyntaxNode,
    generics: &[GenericParameter],
    packs: &PackTypes<'_>,
    keys: &[(Location, &Argument)],
    found: &mut [Option<Argument>],
) -> bool {
    if relation.kind() == SyntaxKind::ConstraintParen {
        return relation.children().next().is_some_and(|inside| {
            equality(unit, declaration, &inside, generics, packs, keys, found)
        });
    }
    if relation.kind() != SyntaxKind::ConstraintRelation || !has_token(relation, SyntaxKind::EqEq) {
        return false;
    }
    let sides: Vec<_> = relation.children().collect();
    let [left, right] = sides.as_slice() else {
        return false;
    };
    let written = |node: &SyntaxNode, values: &[Option<Argument>]| -> Option<Type> {
        let ty = &unit
            .written
            .iter()
            .find(|written| {
                written.at.file == declaration.at.file && written.at.span.start == node.span().start
            })?
            .ty;
        let mut map: Vec<_> = generics
            .iter()
            .zip(values)
            .filter_map(|(generic, value)| Some((generic.at, value.as_ref()?)))
            .collect();
        map.extend_from_slice(keys);
        packs.project(ty, &map).ok()
    };
    let (Some(left), Some(right)) = (written(left, found), written(right, found)) else {
        return false;
    };
    if same_type(&left, &right) == Some(true) && !names_parameter(&left) && !names_parameter(&right)
    {
        return true;
    }
    // GEN-3 permits one unresolved generic, with one side known. It
    // does not solve two simultaneous unknowns or bind a foreign generic.
    let unresolved: Vec<_> = generics
        .iter()
        .zip(found.iter())
        .filter(|(_, value)| value.is_none())
        .map(|(generic, _)| generic.at)
        .filter(|at| {
            super::inference::mentions(&left, &[*at]) || super::inference::mentions(&right, &[*at])
        })
        .collect();
    if unresolved.len() != 1 {
        return false;
    }
    let mut trial = found.to_vec();
    // GEN-3: an unresolved const generic inside a type is not known
    // data either, even though names_parameter only finds type parameters.
    let known = |ty: &Type| !names_parameter(ty) && !super::inference::mentions(ty, &unresolved);
    if known(&left) {
        infer(&right, &left, generics, &mut trial);
    } else if known(&right) {
        infer(&left, &right, generics, &mut trial);
    } else {
        return false;
    }
    let map: Vec<_> = generics
        .iter()
        .zip(&trial)
        .filter_map(|(generic, value)| Some((generic.at, value.as_ref()?)))
        .collect();
    let left = crate::declared::substitute_within(&left, &map);
    let right = crate::declared::substitute_within(&right, &map);
    if names_parameter(&left) || names_parameter(&right) || same_type(&left, &right) != Some(true) {
        return false;
    }
    found.clone_from_slice(&trial);
    true
}
