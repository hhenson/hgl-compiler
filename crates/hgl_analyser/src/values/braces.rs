//! How a braced operand obtains a known struct schema under TYPE-5,
//! independently of the list/set choice of a nested bracket (EXPR-9).

use std::collections::HashMap;

use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode};

use super::Typer;
use super::candidates::Key;
use super::expressions::takes_its_type;
use super::fit::{not_null, open, whole};
use crate::declared::{mentions, substitute_within};
use crate::hir::{Expr, ExprKind, Node};
use crate::syntax::{child, ungrouped};
use crate::types::{Argument, Signature, Type};

impl Typer<'_> {
    /// TYPE-5: a braced operand can receive a public schema from a written
    /// generic, its result or its siblings. Its own provisional reading is
    /// never evidence for that schema, including inside an explicit collection.
    pub(super) fn operator_brace_schemas(
        &mut self,
        public: &Signature,
        known: &[Option<Argument>],
        written: &[(Key, SyntaxNode, Option<Expr>)],
    ) -> Vec<Option<Type>> {
        let targets: Vec<_> = written
            .iter()
            .filter_map(|(key, node, ready)| {
                let slot = key.public_parameter(public)?;
                (ready.is_none()
                    && public.parameters[slot].pack.is_none()
                    && contains_braced_literal(node))
                .then_some(slot)
            })
            .collect();
        if targets.is_empty() {
            return Vec::new();
        }
        let mut sources = vec![None; public.parameters.len()];
        if known.iter().any(Option::is_none) {
            let mark = self.mark();
            for (key, node, ready) in written {
                let Some(slot) = key.public_parameter(public) else {
                    continue;
                };
                if public.parameters[slot].pack.is_some() || targets.as_slice() == [slot] {
                    continue;
                }
                let before = self.diagnostics.len();
                let actual = ready
                    .as_ref()
                    .map(|value| value.ty.clone())
                    .unwrap_or_else(|| self.expression(node, Some(&Type::Unknown)).ty);
                if self.diagnostics.len() == before && whole(&actual) && !open(&actual) {
                    sources[slot] = if ready.is_some() {
                        Some(actual)
                    } else {
                        bracket_evidence(node, &public.parameters[slot].ty, &actual)
                    };
                }
            }
            self.take_back(mark, false);
        }
        let own: Vec<_> = public.generics.iter().map(|generic| generic.at).collect();
        let mut schemas = vec![None; public.parameters.len()];
        let mut supplied: Vec<_> = sources.iter().map(Option::as_ref).collect();
        for slot in targets {
            let mut found = known.to_vec();
            if found.iter().any(Option::is_none) {
                let own_read = supplied[slot].take();
                self.infer_operator_inputs(public, &mut found, &supplied);
                supplied[slot] = own_read;
            }
            let bindings: Vec<_> = own
                .iter()
                .copied()
                .zip(&found)
                .filter_map(|(at, arg)| Some((at, arg.as_ref()?)))
                .collect();
            let ty = substitute_within(&public.parameters[slot].ty, &bindings);
            if whole(&ty) && !open(&ty) && !mentions(&ty, &own) {
                schemas[slot] = Some(ty);
            }
        }
        schemas
    }
}

/// TYPE-5 and EXPR-9: a brace literal can take its type from the operand
/// position, including inside a sequence, tuple, or branch value.
pub(super) fn contains_braced_literal(node: &SyntaxNode) -> bool {
    contains_collection(node, |kind| {
        matches!(kind, SyntaxKind::KeyedExpr | SyntaxKind::SetExpr)
    })
}

/// TYPE-5: only a known schema can turn the names in `{field: value}`
/// into struct fields. A candidate may refine collection elements, but
/// cannot change whether an enclosed keyed literal was a construction.
pub(super) fn keeps_struct_fields(node: &SyntaxNode, public: &Expr, candidate: &Expr) -> bool {
    let mut schemas: HashMap<_, _> = node
        .descendants()
        .filter(|part| part.kind() == SyntaxKind::KeyedExpr)
        .map(|part| (part.span(), None))
        .collect();
    if schemas.is_empty() {
        return true;
    }
    public.walk_nodes(&mut |node| {
        if let Node::Expression(read) = node
            && matches!(read.kind, ExprKind::Construct { .. })
            && let Some(slot) = schemas.get_mut(&read.at.span)
        {
            *slot = Some(&read.ty);
        }
    });
    let mut matches = true;
    candidate.walk(&mut |read| {
        if matches!(read.kind, ExprKind::Construct { .. })
            && let Some(schema) = schemas.remove(&read.at.span)
        {
            matches &= schema.is_some_and(|known| crate::declared::agree(known, &read.ty));
        }
    });
    matches && schemas.values().all(Option::is_none)
}

/// A bracket literal in a contextual position does not determine its own
/// collection type independently of candidate matching (EXPR-9).
fn contains_bracket_literal(node: &SyntaxNode) -> bool {
    contains_collection(node, |kind| kind == SyntaxKind::SequenceExpr)
}

/// Sibling evidence for TYPE-5. A shared bracket acquires its outer kind
/// from a declared parameter, or can still be a list or a set under GEN-5.
fn bracket_evidence(node: &SyntaxNode, pattern: &Type, actual: &Type) -> Option<Type> {
    if !contains_bracket_literal(node) {
        return Some(actual.clone());
    }
    let node = ungrouped(node.clone());
    match (
        node.kind(),
        not_null(Some(pattern))?,
        not_null(Some(actual))?,
    ) {
        (
            SyntaxKind::SequenceExpr | SyntaxKind::SetExpr,
            pattern @ (Type::List { element, .. } | Type::Set(element)),
            Type::List {
                element: actual, ..
            }
            | Type::Set(actual),
        ) => {
            if node.kind() == SyntaxKind::SetExpr && !matches!(pattern, Type::Set(_)) {
                return None;
            }
            let read = common_evidence(node.children(), element, actual)?;
            match pattern {
                Type::Set(_) => Some(Type::Set(Box::new(read))),
                Type::List { size, .. } => Some(Type::List {
                    element: Box::new(read),
                    size: size.clone(),
                }),
                _ => unreachable!(),
            }
        }
        (
            SyntaxKind::KeyedExpr | SyntaxKind::SequenceExpr,
            Type::Dict { key, value },
            Type::Dict {
                key: actual_key,
                value: actual_value,
            },
        ) => {
            let column = |which| {
                node.children()
                    .filter_map(move |pair| pair.children().nth(which))
            };
            let k = common_evidence(column(0), key, actual_key)?;
            let v = common_evidence(column(1), value, actual_value)?;
            Some(Type::Dict {
                key: Box::new(k),
                value: Box::new(v),
            })
        }
        (
            SyntaxKind::TupleExpr | SyntaxKind::SequencePair,
            Type::Tuple(written),
            Type::Tuple(elements),
        ) if written.len() == elements.len() => Some(Type::Tuple(
            node.children()
                .zip(written)
                .zip(elements)
                .map(|((entry, pattern), actual)| bracket_evidence(&entry, pattern, actual))
                .collect::<Option<_>>()?,
        )),
        _ => None,
    }
}

/// All occurrences of a shared element or column must give the same reading.
fn common_evidence(
    entries: impl Iterator<Item = SyntaxNode>,
    pattern: &Type,
    actual: &Type,
) -> Option<Type> {
    let mut entries = entries.map(|entry| bracket_evidence(&entry, pattern, actual));
    let first = entries.next().unwrap_or_else(|| Some(actual.clone()))?;
    entries
        .all(|entry| Type::same_optional(entry.as_ref(), Some(&first)))
        .then_some(first)
}

fn contains_collection(node: &SyntaxNode, sought: fn(SyntaxKind) -> bool) -> bool {
    let node = ungrouped(node.clone());
    match node.kind() {
        kind if sought(kind) => true,
        SyntaxKind::KeyedExpr
        | SyntaxKind::SetExpr
        | SyntaxKind::TupleExpr
        | SyntaxKind::SequenceExpr
        | SyntaxKind::SequencePair => node
            .children()
            .any(|part| contains_collection(&part, sought)),
        // The comprehension entry inherits the list element's context.
        SyntaxKind::ListComprehension => node
            .children()
            .nth(1)
            .is_some_and(|entry| contains_collection(&entry, sought)),
        // ERR-2: only the payload inherits the selected result side.
        SyntaxKind::CallExpr if takes_its_type(&node) => child(&node, SyntaxKind::ArgList)
            .is_some_and(|list| match super::calls::arguments_of(&list).as_slice() {
                [(None, payload)] => contains_collection(payload, sought),
                _ => false,
            }),
        SyntaxKind::IfExpr => node
            .children()
            .skip(1)
            .any(|branch| contains_collection(&branch, sought)),
        SyntaxKind::Block => super::control::value_of_branch(&node)
            .is_some_and(|value| contains_collection(&value, sought)),
        _ => false,
    }
}
