//! Finite domains for declaration-time checking of generic calls (GEN-3,
//! GEN-7, GEN-17). Conjoined equalities link ordinary parameters and projected
//! pack members. Each admitted assignment is tried once per equality class.

use super::{
    MemberChoices, StatedMemberTypes, StatedPackKey, StatedPackMember, conjoined_relations,
    pack_view,
};
use crate::declared::{same_type, substitute_within};
use crate::syntax::{child, has_token, tokens, ungrouped};
use crate::types::{Argument, Constant, GenericKind, Scalar, Type};
use crate::values::Typer;
use crate::values::candidates::combinations;
use crate::values::fit::whole;
use crate::values::inference::mentions;
use hgl_diagnostics::Location;
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode};

impl Typer<'_> {
    /// The finite choices that hold throughout this declaration's requirements.
    pub(in crate::values) fn finite_domains(&self) -> FiniteDomains {
        let mut domains = FiniteDomains::default();
        let Some(condition) = child(&self.declaration.syntax, SyntaxKind::RequiresClause)
            .and_then(|clause| clause.children().next())
        else {
            return domains;
        };
        let own: Vec<_> = self
            .signature
            .generics
            .iter()
            .map(|generic| generic.at)
            .collect();
        let mut relations = Vec::new();
        conjoined_relations(&condition, &mut relations);
        let mut present = Vec::new();
        let mut dependent = Vec::new();
        for relation in relations {
            if let Some((at, choices)) = self.stated_membership(&relation, &own, true) {
                let ordinary = |choice: &Argument| match choice {
                    Argument::Type(Type::Parameter { at, .. }) if own.contains(at) => Some(*at),
                    _ => None,
                };
                if choices.iter().any(|choice| ordinary(choice).is_some()) {
                    let alternatives = choices
                        .into_iter()
                        .map(|choice| match ordinary(&choice) {
                            Some(at) => TypeAlternative::Parameter(at),
                            None => TypeAlternative::Fixed(choice),
                        })
                        .collect();
                    dependent.push((at, alternatives));
                } else {
                    domains.restrict(FiniteSlot::Parameter(at), choices);
                }
                continue;
            }
            let mut operands = relation.children();
            let (Some(left), Some(right)) = (operands.next(), operands.next()) else {
                continue;
            };
            if tokens(&relation).any(|token| token.text() == "in")
                && let Some(member) = self.present_member(&left, &right)
            {
                present.push(member);
                continue;
            }
            let written =
                |node: &SyntaxNode| self.written.get(&(self.file, node.span().start)).copied();
            if has_token(&relation, SyntaxKind::EqEq) {
                let (Some(left), Some(right)) = (written(&left), written(&right)) else {
                    continue;
                };
                let restrict = |domains: &mut FiniteDomains, slot, ty: &Type| {
                    if whole(ty) && !mentions(ty, &own) {
                        domains.restrict(slot, vec![Argument::Type(ty.clone())]);
                    }
                };
                match (FiniteSlot::from_type(left), FiniteSlot::from_type(right)) {
                    (Some(a), Some(b)) => domains.join(a, b),
                    (Some(slot), None) => restrict(&mut domains, slot, right),
                    (None, Some(slot)) => restrict(&mut domains, slot, left),
                    (None, None) => {}
                }
            } else if let Some(StatedPackMember {
                pack,
                key: StatedPackKey::One(key),
                types: StatedMemberTypes::Alternatives(types),
            }) = self.stated_member_membership(&relation)
                && types.iter().all(|ty| !mentions(ty, &own))
            {
                domains.restrict(
                    FiniteSlot::Member(pack, key),
                    types.into_iter().map(Argument::Type).collect(),
                );
            }
        }
        if !present.is_empty() {
            self.apply_present_member_bounds(&mut domains, &present, &own);
        }
        domains.add_dependent_memberships(dependent);
        domains
    }

    /// A positive named key in arg_keys(Ps) guarantees that Ps[key] exists
    /// (GEN-17). The universal on its own also admits an empty pack.
    fn present_member(
        &self,
        left: &SyntaxNode,
        right: &SyntaxNode,
    ) -> Option<(Location, Constant)> {
        let (view, name) = pack_view(right.clone())?;
        let left = ungrouped(left.clone());
        if view.text() != "arg_keys"
            || !matches!(left.kind(), SyntaxKind::NamedType | SyntaxKind::NameExpr)
        {
            return None;
        }
        let pack = self.signature.generics.iter().find(|slot| {
            slot.name == name.text() && matches!(slot.kind, GenericKind::Pack { .. })
        })?;
        let key = self.typed_in(self.file, &left, None).ok()?;
        if !matches!(key.ty(), Some(Type::Scalar(Scalar::I64 | Scalar::Str))) {
            return None;
        }
        Some((
            pack.at,
            Constant {
                text: left.text().to_string(),
                value: key.value,
            },
        ))
    }

    fn apply_present_member_bounds(
        &self,
        domains: &mut FiniteDomains,
        present: &[(Location, Constant)],
        own: &[Location],
    ) {
        for member in self.stated_pack_members() {
            let StatedPackKey::Every(bound) = member.key else {
                continue;
            };
            for (_, key) in present.iter().filter(|(pack, _)| *pack == member.pack) {
                let slot = FiniteSlot::Member(member.pack, key.clone());
                if let StatedMemberTypes::Equal(Type::Parameter { at, .. }) = &member.types {
                    domains.join(slot, FiniteSlot::Parameter(*at));
                } else {
                    let given = Argument::Constant(key.clone());
                    let types: Vec<_> = member
                        .types
                        .values()
                        .iter()
                        .map(|ty| substitute_within(ty, &[(bound, &given)]))
                        .collect();
                    if types.iter().all(|ty| whole(ty) && !mentions(ty, own)) {
                        domains.restrict(slot, types.into_iter().map(Argument::Type).collect());
                    }
                }
            }
        }
    }
}

/// Each positive equality connects two declaration identities. Distinct
/// reflected keys share evidence only when the declaration equates them.
#[derive(Clone)]
enum FiniteSlot {
    Parameter(Location),
    Member(Location, Constant),
}

impl FiniteSlot {
    fn from_type(ty: &Type) -> Option<Self> {
        match ty {
            Type::Parameter { at, .. } => Some(Self::Parameter(*at)),
            Type::PackMember { pack, key, .. } => Some(Self::Member(*pack, key.clone())),
            _ => None,
        }
    }

    fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Parameter(a), Self::Parameter(b)) => a == b,
            (Self::Member(a, k), Self::Member(b, j)) => a == b && k.same_value(j) == Some(true),
            _ => false,
        }
    }
}

#[derive(Clone, Default)]
struct FiniteDomain {
    slots: Vec<FiniteSlot>,
    choices: Option<Vec<Argument>>,
}

impl FiniteDomain {
    fn intersect(bound: &mut Option<Vec<Argument>>, choices: Vec<Argument>) {
        if let Some(bound) = bound {
            bound.retain(|arg| {
                choices
                    .iter()
                    .any(|other| equal_members(arg, other) != Some(false))
            });
            // Equal values may still be distinct constants when the body
            // computes with them (NUM-5, UNIT-13). Keep each stated sign.
            for choice in choices {
                if let Argument::Constant(value) = &choice
                    && bound
                        .iter()
                        .any(|arg| equal_members(arg, &choice) == Some(true))
                    && !bound.iter().any(|arg| {
                        matches!(arg, Argument::Constant(other)
                        if other.same_value(value) == Some(true))
                    })
                {
                    bound.push(choice);
                }
            }
        } else {
            *bound = Some(choices);
        }
    }
}

/// A written membership uses language equality for constant values (GEN-3,
/// UNIT-13). This includes equal signed zeros and excludes NaN.
fn equal_members(left: &Argument, right: &Argument) -> Option<bool> {
    match (left, right) {
        (Argument::Type(left), Argument::Type(right)) => same_type(left, right),
        (Argument::Constant(left), Argument::Constant(right)) => left.value.equals(&right.value),
        _ => Some(left == right),
    }
}

/// A bare type parameter in this declaration depends on its own domain.
enum TypeAlternative {
    Parameter(Location),
    Fixed(Argument),
}

/// A conjunction's finite bounds on ordinary parameters and named members.
/// Absence of a finite bound differs from an empty intersection (GEN-3, GEN-7).
#[derive(Default)]
pub(in crate::values) struct FiniteDomains {
    groups: Vec<FiniteDomain>,
    dependent: Vec<(Location, Vec<TypeAlternative>)>,
}

impl FiniteDomains {
    fn group(&self, slot: &FiniteSlot) -> Option<usize> {
        self.groups
            .iter()
            .position(|domain| domain.slots.iter().any(|member| member.same(slot)))
    }

    fn insert(&mut self, slot: FiniteSlot) -> usize {
        if let Some(group) = self.group(&slot) {
            return group;
        }
        self.groups.push(FiniteDomain {
            slots: vec![slot],
            choices: None,
        });
        self.groups.len() - 1
    }

    fn join(&mut self, left: FiniteSlot, right: FiniteSlot) {
        let first = self.insert(left);
        let second = self.insert(right);
        if first == second {
            return;
        }
        let (first, second) = (first.min(second), first.max(second));
        let other = self.groups.remove(second);
        self.groups[first].slots.extend(other.slots);
        if let Some(choices) = other.choices {
            FiniteDomain::intersect(&mut self.groups[first].choices, choices);
        }
    }

    fn restrict(&mut self, slot: FiniteSlot, choices: Vec<Argument>) {
        let index = self.insert(slot);
        FiniteDomain::intersect(&mut self.groups[index].choices, choices);
    }

    /// Expand T in {U, f64} only when every named alternative has a
    /// nonempty finite domain. Repeated intersections can narrow a source
    /// after it was first read, so propagate until all bounds are stable.
    fn add_dependent_memberships(
        &mut self,
        mut memberships: Vec<(Location, Vec<TypeAlternative>)>,
    ) {
        if memberships.is_empty() {
            return;
        }
        let independent = self.groups.clone();
        loop {
            self.propagate_memberships(&memberships);
            // An impossible source supplies no evidence. If a later
            // intersection ruled it out, withdraw all bounds that relied
            // on it. Independent facts about the subject still hold.
            let before = memberships.len();
            memberships.retain(|(_, alternatives)| {
                !alternatives.iter().any(|alternative| {
                    self.alternative_choices(alternative)
                        .is_some_and(<[_]>::is_empty)
                })
            });
            if before == memberships.len() {
                break;
            }
            self.groups.clone_from(&independent);
        }
        self.dependent = memberships;
    }

    fn propagate_memberships(&mut self, memberships: &[(Location, Vec<TypeAlternative>)]) {
        loop {
            let mut changed = false;
            for (parameter, alternatives) in memberships {
                if let Some(other) = self.only_possible_parameter(*parameter, alternatives) {
                    let left = self.insert(FiniteSlot::Parameter(*parameter));
                    let right = self.insert(FiniteSlot::Parameter(other));
                    if left != right {
                        self.join(
                            FiniteSlot::Parameter(*parameter),
                            FiniteSlot::Parameter(other),
                        );
                        changed = true;
                    }
                }
                let Some(choices) = self.expand_alternatives(alternatives) else {
                    continue;
                };
                let index = self.insert(FiniteSlot::Parameter(*parameter));
                let bound = &mut self.groups[index].choices;
                let before = bound.as_ref().map(Vec::len);
                FiniteDomain::intersect(bound, choices);
                changed |= before != bound.as_ref().map(Vec::len);
            }
            if !changed {
                break;
            }
        }
    }

    /// If T cannot be any alternative other than U, this membership
    /// establishes T == U. In particular T in {U} always links them.
    fn only_possible_parameter(
        &self,
        at: Location,
        alternatives: &[TypeAlternative],
    ) -> Option<Location> {
        let choices = self.parameter_choices(at);
        let mut required = None;
        for alternative in alternatives {
            let values = self.alternative_choices(alternative);
            if let (Some(choices), Some(values)) = (choices, values)
                && !choices.iter().any(|ty| {
                    values
                        .iter()
                        .any(|other| equal_members(ty, other) != Some(false))
                })
            {
                continue;
            }
            let TypeAlternative::Parameter(at) = alternative else {
                return None;
            };
            let at = *at;
            if required.is_some_and(|prior| prior != at) {
                return None;
            }
            required = Some(at);
        }
        required
    }

    fn parameter_choices(&self, at: Location) -> Option<&[Argument]> {
        let index = self.group(&FiniteSlot::Parameter(at))?;
        self.groups[index].choices.as_deref()
    }

    fn alternative_choices<'a>(
        &'a self,
        alternative: &'a TypeAlternative,
    ) -> Option<&'a [Argument]> {
        match alternative {
            TypeAlternative::Parameter(at) => self.parameter_choices(*at),
            TypeAlternative::Fixed(other) => Some(std::slice::from_ref(other)),
        }
    }

    fn expand_alternatives(&self, alternatives: &[TypeAlternative]) -> Option<Vec<Argument>> {
        let mut choices = Vec::new();
        for alternative in alternatives {
            let values = self.alternative_choices(alternative)?;
            if values.is_empty() {
                return None;
            }
            for value in values {
                if !choices
                    .iter()
                    .any(|known| equal_members(known, value) == Some(true))
                {
                    choices.push(value.clone());
                }
            }
        }
        Some(choices)
    }

    /// The finite alternatives of each ordinary parameter relevant to a trial.
    pub(in crate::values) fn memberships(&self) -> Vec<(Location, Vec<Argument>)> {
        let mut result = Vec::new();
        for domain in &self.groups {
            if let Some(choices) = &domain.choices {
                for slot in &domain.slots {
                    if let FiniteSlot::Parameter(at) = slot {
                        result.push((*at, choices.clone()));
                    }
                }
            }
        }
        result
    }

    /// A trial can make two source keys denote the same member. Unite their
    /// classes before checking any of the call's ordinary or reflected slots.
    pub(in crate::values) fn with_trial_keys(
        &self,
        given: &[(Location, &Argument)],
    ) -> Option<Self> {
        if !given
            .iter()
            .any(|(_, arg)| matches!(arg, Argument::Constant(_)))
        {
            return None;
        }
        let constant_at = |at| {
            given.iter().find_map(|(other, arg)| match arg {
                Argument::Constant(value) if *other == at => Some(value.value.clone()),
                _ => None,
            })
        };
        let mut projected = Self::default();
        for domain in &self.groups {
            let mut slots = domain.slots.iter().map(|slot| match slot {
                FiniteSlot::Member(pack, key) => FiniteSlot::Member(
                    *pack,
                    Constant {
                        text: key.text.clone(),
                        value: key.value.substituted(&constant_at),
                    },
                ),
                FiniteSlot::Parameter(at) => FiniteSlot::Parameter(*at),
            });
            let Some(first) = slots.next() else { continue };
            for next in slots {
                projected.join(first.clone(), next);
            }
            if let Some(choices) = &domain.choices {
                projected.restrict(first, choices.clone());
            }
        }
        Some(projected)
    }

    /// Propagate each class's finite bounds and the ordinary types fixed for
    /// this trial to its reflected slots. Keep an empty bound on its keys so
    /// it rules out the trial only when the call uses that evidence (GEN-7).
    pub(super) fn add_member_bounds(
        &self,
        bounds: &mut Vec<StatedPackMember>,
        given: &[(Location, &Argument)],
        relevant: &[Location],
    ) -> Option<()> {
        for domain in &self.groups {
            if domain.slots.len() < 2
                || !domain
                    .slots
                    .iter()
                    .any(|slot| matches!(slot, FiniteSlot::Member(..)))
            {
                continue;
            }
            let mut choices = domain.choices.clone();
            for (at, value) in given {
                if domain
                    .slots
                    .iter()
                    .any(|slot| matches!(slot, FiniteSlot::Parameter(other) if other == at))
                {
                    FiniteDomain::intersect(&mut choices, vec![(*value).clone()]);
                }
            }
            let Some(choices) = choices else { continue };
            if choices.is_empty()
                && domain
                    .slots
                    .iter()
                    .any(|slot| matches!(slot, FiniteSlot::Parameter(at) if relevant.contains(at)))
            {
                return None;
            }
            let types: Vec<_> = choices
                .into_iter()
                .filter_map(|choice| match choice {
                    Argument::Type(ty) => Some(ty),
                    _ => None,
                })
                .collect();
            for slot in &domain.slots {
                if let FiniteSlot::Member(pack, key) = slot {
                    bounds.push(StatedPackMember {
                        pack: *pack,
                        key: StatedPackKey::One(key.clone()),
                        types: StatedMemberTypes::Alternatives(types.clone()),
                    });
                }
            }
        }
        Some(())
    }

    /// Joint bindings of ordinary arguments consistent with the equalities.
    pub(in crate::values) fn ordinary_trials(
        &self,
        choices: &[(Location, Vec<Argument>)],
    ) -> Vec<Vec<(Location, Argument)>> {
        if choices.iter().any(|(_, values)| values.is_empty()) {
            return Vec::new();
        }
        let mut trials = self.trials(
            choices,
            |at| self.group(&FiniteSlot::Parameter(*at)),
            equal_members,
        );
        // The callers diagnose an over-limit product. It is incomplete and
        // cannot supply evidence for eliminating any combination.
        if trials.len() > crate::INSTANCES || self.dependent.is_empty() || trials.is_empty() {
            return trials;
        }
        trials.retain(|trial| self.admits_memberships(trial, &[]));
        let hidden = self.hidden_choices(choices);
        if hidden.is_empty() || trials.is_empty() {
            return trials;
        }
        // A visible assignment is admitted when at least one assignment of
        // the omitted parameters satisfies the same memberships (GEN-3,
        // GEN-7). Count both parts against the existing proof budget. If the
        // finer proof is unavailable, keep every directly admitted trial.
        let mut count = trials.len();
        for (_, values) in &hidden {
            count = count.saturating_mul(values.len());
            if count > crate::INSTANCES {
                return trials;
            }
        }
        let witnesses = combinations(&hidden);
        trials.retain(|trial| {
            witnesses
                .iter()
                .any(|witness| self.admits_memberships(trial, witness))
        });
        trials
    }

    /// Find the finite ordinary classes connected to the call by memberships.
    /// An unbounded alternative permits an unknown type; a contradictory
    /// class supplies no independent evidence for a call using other slots.
    fn hidden_choices(
        &self,
        visible: &[(Location, Vec<Argument>)],
    ) -> Vec<(Location, Vec<Argument>)> {
        let mut reached = vec![false; self.groups.len()];
        for (at, _) in visible {
            if let Some(group) = self.group(&FiniteSlot::Parameter(*at)) {
                reached[group] = true;
            }
        }
        let mut hidden = Vec::new();
        loop {
            let before = hidden.len();
            for (subject, alternatives) in &self.dependent {
                let parameters = std::iter::once(*subject).chain(alternatives.iter().filter_map(
                    |alternative| match alternative {
                        TypeAlternative::Parameter(at) => Some(*at),
                        TypeAlternative::Fixed(_) => None,
                    },
                ));
                let finite_group = |at| {
                    let index = self.group(&FiniteSlot::Parameter(at))?;
                    let choices = self.groups[index].choices.as_deref()?;
                    (!choices.is_empty()).then_some((at, index, choices))
                };
                let Some(groups) = parameters.map(finite_group).collect::<Option<Vec<_>>>() else {
                    continue;
                };
                if groups.iter().any(|(_, group, _)| reached[*group]) {
                    for (at, group, choices) in groups {
                        if !reached[group] {
                            hidden.push((at, choices.to_vec()));
                            reached[group] = true;
                        }
                    }
                }
            }
            if before == hidden.len() {
                return hidden;
            }
        }
    }

    fn admits_memberships(
        &self,
        trial: &[(Location, Argument)],
        witness: &[(Location, Argument)],
    ) -> bool {
        let value = |at| {
            let group = self.group(&FiniteSlot::Parameter(at))?;
            trial.iter().chain(witness).find_map(|(other, value)| {
                let member = FiniteSlot::Parameter(*other);
                self.groups[group]
                    .slots
                    .iter()
                    .any(|slot| slot.same(&member))
                    .then_some(value)
            })
        };
        self.dependent.iter().all(|(subject, alternatives)| {
            let Some(subject) = value(*subject) else {
                return true;
            };
            alternatives.iter().any(|alternative| {
                let other = match alternative {
                    TypeAlternative::Parameter(at) => value(*at),
                    TypeAlternative::Fixed(other) => Some(other),
                };
                other.is_none_or(|other| equal_members(subject, other) != Some(false))
            })
        })
    }

    /// Joint bindings of the reflected members the call reads.
    pub(in crate::values) fn member_trials(
        &self,
        choices: &[MemberChoices],
    ) -> Vec<Vec<((Location, Constant), Type)>> {
        self.trials(
            choices,
            |(pack, key)| self.group(&FiniteSlot::Member(*pack, key.clone())),
            same_type,
        )
    }

    /// Enumerate once per class, then assign that choice to each used slot.
    /// Independent classes still form a product, subject to the usual limit.
    fn trials<K: Clone, V: Clone>(
        &self,
        choices: &[(K, Vec<V>)],
        group_of: impl Fn(&K) -> Option<usize>,
        same: impl Fn(&V, &V) -> Option<bool>,
    ) -> Vec<Vec<(K, V)>> {
        let mut groups: Vec<(Option<usize>, Vec<K>, Vec<V>)> = Vec::new();
        for (key, values) in choices {
            let group = group_of(key);
            if let Some((_, keys, bound)) = groups
                .iter_mut()
                .find(|(id, _, _)| group.is_some() && *id == group)
            {
                bound.retain(|value| values.iter().any(|other| same(value, other) != Some(false)));
                keys.push(key.clone());
            } else {
                groups.push((group, vec![key.clone()], values.clone()));
            }
        }
        let groups: Vec<_> = groups
            .into_iter()
            .map(|(_, keys, values)| (keys, values))
            .collect();
        combinations(&groups)
            .into_iter()
            .map(|trial| {
                trial
                    .into_iter()
                    .flat_map(|(keys, value)| keys.into_iter().map(move |key| (key, value.clone())))
                    .collect()
            })
            .collect()
    }
}
