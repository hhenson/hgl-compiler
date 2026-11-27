//! Compare the list and set readings an operator's admitted candidates
//! supply to the same written sequence (EXPR-9, GEN-5, WRITE-7).

use super::{
    Candidate, Given, Place, ReflectedCall, Specific, Typer, Wanted, not_null, parameter_type,
    ungrouped, value_of_branch,
};
use crate::hir::TypePackArgument;
use crate::program::{OperatorRef, Role};
use crate::syntax::child;
use crate::types::{Signature, Type};
use crate::values::{calls::arguments_of, expressions::result_constructor, results::tagged_side};
use crate::{codes, warning};
use hgl_diagnostics::{Category, Location};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode};

/// ERR-2: the single positional payload uses the side selected by the tag.
fn tagged_payload(call: &SyntaxNode) -> Option<(bool, SyntaxNode)> {
    let success = result_constructor(call)?;
    let list = child(call, SyntaxKind::ArgList)?;
    let mut arguments = arguments_of(&list).into_iter();
    let (name, payload) = arguments.next()?;
    (name.is_none() && arguments.next().is_none()).then_some((success, payload))
}

/// Only the argument's result and its elements take the expected reading.
/// Calls, statements and conditions establish their own operand contexts.
pub(in crate::values) fn contains_sequence(node: &SyntaxNode) -> bool {
    let node = ungrouped(node.clone());
    match node.kind() {
        SyntaxKind::SequenceExpr => {
            if node
                .children()
                .any(|entry| entry.kind() == SyntaxKind::SequencePair)
            {
                node.children()
                    .flat_map(|pair| pair.children())
                    .any(|part| contains_sequence(&part))
            } else {
                !node
                    .children()
                    .any(|entry| entry.kind() == SyntaxKind::Hole)
            }
        }
        SyntaxKind::CallExpr => {
            tagged_payload(&node).is_some_and(|(_, payload)| contains_sequence(&payload))
        }
        // EXPR-17: only the entry takes the expected element type.
        SyntaxKind::ListComprehension => node
            .children()
            .nth(1)
            .is_some_and(|entry| contains_sequence(&entry)),
        SyntaxKind::SetExpr | SyntaxKind::TupleExpr => {
            node.children().any(|entry| contains_sequence(&entry))
        }
        SyntaxKind::KeyedExpr => node
            .children()
            .flat_map(|pair| pair.children())
            .any(|part| contains_sequence(&part)),
        SyntaxKind::Block => value_of_branch(&node).is_some_and(|value| contains_sequence(&value)),
        SyntaxKind::IfExpr => node
            .children()
            .skip(1)
            .any(|branch| contains_sequence(&branch)),
        _ => false,
    }
}

/// Include a set supplied by a sibling or requested result when a candidate
/// has an unconstrained input parameter.
pub(super) fn may_compete(candidates: &[Candidate], given: &[Given], wanted: &Wanted) -> bool {
    let contains_set = |ty: &Type| ty.any_part(&|part| matches!(part, Type::Set(_)), &|_| false);
    candidates.len() > 1
        && given
            .iter()
            .any(|arg| arg.by_place && contains_sequence(&arg.node))
        && (candidates.iter().any(|(_, _, signature)| {
            signature
                .parameters
                .iter()
                .any(|parameter| contains_set(&parameter.ty))
        }) || given.iter().any(|arg| contains_set(arg.ty()))
            || wanted.place.is_some_and(contains_set))
}

/// The type a candidate supplies to an actual sequence at its source
/// location. Pair entries and trace holes do not have the list/set choice.
struct Reading {
    at: Location,
    ty: Type,
}

impl Reading {
    fn set(&self) -> bool {
        matches!(self.ty, Type::Set(_))
    }
}

/// The declarations to try, optionally within one selected combination of
/// sequence readings. Numeric precision is settled by all type fits within it.
#[derive(Clone, Copy)]
pub(super) struct CandidateGroup<'g, 's> {
    pub(super) candidates: &'g [Candidate<'s>],
    required: &'g [Reading],
}

impl<'g, 's> CandidateGroup<'g, 's> {
    /// An ordinary call or an independent trial for one declaration.
    pub(super) fn all(candidates: &'g [Candidate<'s>]) -> Self {
        Self {
            candidates,
            required: &[],
        }
    }

    /// A reflected callback trial retains the enclosing call's reading.
    pub(super) fn for_candidate<'t>(
        &'t self,
        candidate: &'t Candidate<'s>,
    ) -> CandidateGroup<'t, 's> {
        CandidateGroup {
            candidates: std::slice::from_ref(candidate),
            required: self.required,
        }
    }

    /// Test the instantiated slots, including a generic supplied by a sibling,
    /// before this arrangement can count as a type fit (GEN-5).
    pub(super) fn fits_reading(
        &self,
        given: &[Given],
        bound: &[Place],
        signature: &Signature,
        packs: &[TypePackArgument],
    ) -> bool {
        self.required.is_empty()
            || compare(self.required, &readings(given, bound, signature, packs)) == Specific::Same
    }
}

fn readings(
    given: &[Given],
    bound: &[Place],
    signature: &Signature,
    packs: &[TypePackArgument],
) -> Vec<Reading> {
    let mut result = Vec::new();
    for (arg, &place) in given.iter().zip(bound) {
        if arg.by_place
            && let Some(ty) = parameter_type(signature, packs, place)
        {
            collect(arg.value.at.file, &arg.node, ty, &mut result);
        }
    }
    result
}

fn collect(file: hgl_source::FileId, node: &SyntaxNode, ty: &Type, out: &mut Vec<Reading>) {
    let node = ungrouped(node.clone());
    let ty = not_null(Some(ty)).unwrap_or(ty);
    match (node.kind(), ty) {
        (SyntaxKind::SequenceExpr, Type::List { element, .. } | Type::Set(element)) => {
            if node
                .children()
                .any(|entry| entry.kind() == SyntaxKind::Hole)
            {
                return;
            }
            // A colon entry uses a tuple in its list context. Its children
            // can still contain a disputed bracket, but the outer list cannot.
            if !node
                .children()
                .any(|entry| entry.kind() == SyntaxKind::SequencePair)
            {
                out.push(Reading {
                    at: Location::new(file, node.span()),
                    ty: ty.clone(),
                });
            }
            for entry in node.children() {
                collect(file, &entry, element, out);
            }
        }
        (SyntaxKind::SetExpr, Type::Set(element)) => {
            for entry in node.children() {
                collect(file, &entry, element, out);
            }
        }
        (SyntaxKind::SequenceExpr | SyntaxKind::KeyedExpr, Type::Dict { key, value }) => {
            for pair in node
                .children()
                .filter(|entry| entry.kind() == SyntaxKind::SequencePair)
            {
                for (part, ty) in pair.children().zip([key.as_ref(), value.as_ref()]) {
                    collect(file, &part, ty, out);
                }
            }
        }
        (SyntaxKind::ListComprehension, Type::List { element, .. }) => {
            if let Some(entry) = node.children().nth(1) {
                collect(file, &entry, element, out);
            }
        }
        (SyntaxKind::CallExpr, Type::Result { value, error }) => {
            if let Some((success, payload)) = tagged_payload(&node) {
                collect(file, &payload, tagged_side(success, value, error).0, out);
            }
        }
        (SyntaxKind::TupleExpr | SyntaxKind::SequencePair, Type::Tuple(elements)) => {
            for (entry, ty) in node.children().zip(elements) {
                collect(file, &entry, ty, out);
            }
        }
        (SyntaxKind::Block, _) => {
            if let Some(value) = value_of_branch(&node) {
                collect(file, &value, ty, out);
            }
        }
        (SyntaxKind::IfExpr, _) => {
            for branch in node.children().skip(1) {
                collect(file, &branch, ty, out);
            }
        }
        _ => {}
    }
}

/// GEN-8: a public collection pattern determines the list/set reading at
/// its literal sites. The candidate can still take a more precise extent.
pub(super) fn compatible(given: &Given, public: &Type, candidate: &Type) -> bool {
    let mut required = Vec::new();
    let mut supplied = Vec::new();
    collect(given.value.at.file, &given.node, public, &mut required);
    collect(given.value.at.file, &given.node, candidate, &mut supplied);
    compare(&required, &supplied) == Specific::Same
}

/// A list reading is preferred to a set at the same site. Opposing
/// preferences at different sites give neither candidate precedence.
fn compare(left: &[Reading], right: &[Reading]) -> Specific {
    left.iter().fold(Specific::Same, |order, a| {
        if !right.iter().any(|b| b.at == a.at && b.set() != a.set()) {
            return order;
        }
        order.and(if a.set() {
            Specific::Less
        } else {
            Specific::More
        })
    })
}

impl<'a> Typer<'a> {
    /// Obtain the independently admitted interpretations before the ordinary
    /// precision and specificity rules choose among candidates of one reading.
    pub(super) fn sequence_candidates(
        &mut self,
        operator: OperatorRef,
        candidates: &[Candidate<'a>],
        given: &[Given],
        wanted: &Wanted,
        role: Option<Role>,
        default: Option<&ReflectedCall>,
    ) -> Option<super::Selection<'a>> {
        let mut viable = Vec::new();
        let mut unknown = false;
        for candidate in candidates {
            let ((choices, pending), _) = self.admit_reading(
                operator,
                CandidateGroup::all(std::slice::from_ref(candidate)),
                given,
                wanted,
                role,
                default,
            );
            unknown |= pending;
            for chosen in choices {
                let interpretation =
                    readings(given, &chosen.bound, &chosen.taken, &chosen.type_packs);
                viable.push((chosen, interpretation));
            }
        }
        let differs = viable.iter().enumerate().any(|(i, (_, reading))| {
            viable[..i]
                .iter()
                .any(|(_, other)| compare(reading, other) != Specific::Same)
        });
        if !differs && (unknown || viable.len() == candidates.len()) {
            return None;
        }
        // ERR-15: candidates taking fewer result payloads rank before any
        // GEN-5 preference. Count the actual reads of each admitted trial.
        let fewest = viable.iter().map(|(chosen, _)| chosen.result_reads).min()?;
        let mut excluded = Vec::new();
        viable.retain(|(chosen, _)| {
            if !differs || chosen.result_reads == fewest {
                return true;
            }
            excluded.push((chosen.module, chosen.declaration));
            false
        });
        let dominated = viable
            .iter()
            .filter(|(_, lower)| {
                viable
                    .iter()
                    .any(|(_, higher)| compare(higher, lower) == Specific::More)
            })
            .map(|(chosen, _)| (chosen.module, chosen.declaration));
        excluded.extend(dominated);
        let admitted_sets: Vec<_> = viable
            .iter()
            .flat_map(|(_, literal)| literal.iter())
            .filter(|r| r.set())
            .map(|r| (r.at, r.ty.clone()))
            .collect();
        viable.retain(|(chosen, _)| !excluded.contains(&(chosen.module, chosen.declaration)));
        // Distinct surviving combinations are ambiguous even if ordinary
        // numeric precision or signature specificity would break the tie.
        let crossed = viable.iter().enumerate().any(|(i, (_, reading))| {
            viable[..i]
                .iter()
                .any(|(_, other)| compare(reading, other) == Specific::Neither)
        });
        if crossed {
            let choices = viable.into_iter().map(|(chosen, _)| chosen).collect();
            return Some(((choices, unknown), Vec::new()));
        }
        // A declaration whose condition failed still settles numeric precision
        // when its parameter and result types fit the preferred reading.
        let required: Vec<_> = viable
            .into_iter()
            .flat_map(|(_, readings)| readings)
            .collect();
        if required.is_empty() {
            return None;
        }
        let candidates: Vec<_> = candidates
            .iter()
            .copied()
            .filter(|(m, d, _)| !excluded.contains(&(*m, *d)))
            .collect();
        let group = CandidateGroup {
            candidates: &candidates,
            required: &required,
        };
        let ((mut selected, pending), diagnostics) =
            self.admit_reading(operator, group, given, wanted, role, default);
        if let [chosen] = selected.as_mut_slice()
            && !pending
            && !crate::syntax::suppresses_warning(
                &self.declaration.syntax,
                codes::OPERATOR_SEQUENCE_READING,
            )
        {
            for reading in readings(given, &chosen.bound, &chosen.taken, &chosen.type_packs)
                .into_iter()
                .filter(|r| !r.set())
            {
                if let Some((_, set)) = admitted_sets.iter().find(|(at, _)| *at == reading.at) {
                    let literal = self
                        .node_at(reading.at)
                        .map(|node| node.text().to_string())
                        .unwrap_or_default();
                    chosen.sequence_warnings.push(warning(
                        Category::Operator, codes::OPERATOR_SEQUENCE_READING, reading.at,
                        format!("this sequence is read as a list (`{}`) and a set reading also fits", reading.ty),
                        "GEN-5",
                    ).with_note(format!("to select the set, bind `let members: {set} = {literal}` and pass `members`")));
                }
            }
        }
        Some(((selected, pending), diagnostics))
    }
}
