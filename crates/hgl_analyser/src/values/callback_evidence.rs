//! Type evidence positive argument requirements give a bare callback (GEN-3, FN-17).
//! Part of the analyser's typing pass for static function arguments.

use hgl_diagnostics::Location;
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode};

use super::Candidate;
use super::Typer;
use super::candidates::{Specific, alternative_specificity};
use super::constants::generic_default;
use super::dependent_packs::{bind, dependent_slot, pack_exclusions, reflected_argument};
use super::fit::{open, whole};
use super::inference::{infer, mentions, type_pack, unsaid};
use super::law_calls::CallbackLawInput;
use super::requirements::{conjoined_terms, pack_view};
use crate::declared::{
    names_parameter, same_type, substitute_with_members, substitute_with_packs, substitute_within,
    weight,
};
use crate::hir::{Expr, TypePackArgument};
use crate::syntax::{child, has_token, tokens};
use crate::types::{Argument, ConstantValue, Detail, GenericKind, GenericParameter, Type};
use crate::{Program, ReflectedPack};

/// Selection evidence only. The enclosing call still receives the original argument.
#[derive(Default)]
pub(super) struct CallbackEvidence {
    arguments: Vec<ArgumentPatterns>,
    results: Vec<Type>,
    enclosing_bindings: Vec<EnclosingBindings>,
}

impl CallbackEvidence {
    /// Does any positive argument requirement give a type to this callback?
    pub(super) fn is_empty(&self) -> bool {
        self.arguments.is_empty()
    }

    /// A known result that fits the open template and every declaration.
    pub(super) fn expected_result(&self, current: &Type) -> Option<&Type> {
        let result = self.results.iter().find(|result| whole(result))?;
        (!self.is_empty()
            && !whole(current)
            && same_type(current, result) != Some(false)
            && self
                .results
                .iter()
                .all(|other| same_type(result, other) != Some(false)))
        .then_some(result)
    }

    /// Result types and partial shapes stated by the receiver or candidate.
    pub(super) fn result_shapes(&self) -> &[Type] {
        &self.results
    }

    /// Result restrictions from the receiver and the bindings learned in a trial (GEN-3).
    pub(super) fn trial_result<'a>(
        &self,
        program: &Program,
        readings: impl IntoIterator<Item = &'a MemberReading<'a>>,
        include_requirements: bool,
    ) -> Result<ReflectedResultEvidence, ()> {
        let mut result = ReflectedResultEvidence::default();
        if self.enclosing_bindings.is_empty() {
            return Ok(result);
        }
        let bindings: Vec<_> = readings
            .into_iter()
            .flat_map(|reading| reading.shared_bindings())
            .collect();
        for scope in &self.enclosing_bindings {
            scope.add_result_evidence(program, &bindings, &mut result, include_requirements)?;
        }
        Ok(result)
    }

    /// Read the receiver's evidence and, in an operator trial, its candidate's.
    pub(super) fn from_callables<'a, 'b>(
        program: &Program,
        callables: impl IntoIterator<
            Item = (Candidate<'a>, &'b [(Location, &'b Argument)], &'b [usize]),
        >,
    ) -> Self {
        let mut evidence = Self::default();
        for ((module, declaration, signature), bindings, defaulted) in callables {
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
            let Some(template) = signature.parameters.iter().find_map(|parameter| {
                if parameter.at == callback
                    && let Type::Function(template) = &parameter.ty
                {
                    Some(template)
                } else {
                    None
                }
            }) else {
                continue;
            };
            let declared = &unit.declarations[declaration];
            let mut found: Vec<_> = signature
                .generics
                .iter()
                .map(|generic| {
                    fixed
                        .iter()
                        .find_map(|(at, value)| (*at == generic.at).then(|| (*value).clone()))
                })
                .collect();
            super::quantifiers::bind(unit, declared, &signature.generics, &[], &mut found);
            let solved: Vec<_> = signature
                .generics
                .iter()
                .zip(&found)
                .filter_map(|(generic, value)| Some((generic.at, value.as_ref()?)))
                .collect();
            let unknown_type = Argument::Type(Type::Unknown);
            let unknown_value = Argument::Constant(unsaid());
            let partial: Vec<_> = signature
                .generics
                .iter()
                .zip(&found)
                .map(|(generic, value)| {
                    let unknown = if matches!(generic.kind, GenericKind::Value(_)) {
                        &unknown_value
                    } else {
                        &unknown_type
                    };
                    (generic.at, value.as_ref().unwrap_or(unknown))
                })
                .collect();
            let written_at = |node: &SyntaxNode| {
                let at = Location::new(declared.at.file, node.span());
                unit.written.iter().find(|ty| ty.at == at).map(|ty| &ty.ty)
            };
            let own: Vec<_> = signature
                .generics
                .iter()
                .map(|generic| generic.at)
                .collect();
            let mut record = |result: Type| {
                if !open(&result) && !evidence.results.contains(&result) {
                    evidence.results.push(result);
                }
            };
            if let Some(written) = &template.result {
                let before = substitute_within(written, &fixed);
                let result = substitute_within(written, &solved);
                if result != before || !mentions(written, &own) {
                    record(substitute_within(written, &partial));
                }
            }
            let Some(target) = &template.named_pack else {
                continue;
            };
            let Some(condition) = child(&declared.syntax, SyntaxKind::RequiresClause)
                .and_then(|clause| clause.children().next())
            else {
                continue;
            };
            let type_at =
                |node: &SyntaxNode| written_at(node).map(|ty| substitute_within(ty, &solved));
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
            let mut equalities = Vec::new();
            for term in terms {
                if term.kind() == SyntaxKind::ConstraintRelation {
                    if has_token(&term, SyntaxKind::EqEq) {
                        let mut sides = term.children();
                        if let (Some(left), Some(right)) = (
                            sides.next().and_then(|side| written_at(&side)),
                            sides.next().and_then(|side| written_at(&side)),
                        ) {
                            if template.result.as_ref() == Some(left) {
                                record(substitute_within(right, &partial));
                            }
                            if template.result.as_ref() == Some(right) {
                                record(substitute_within(left, &partial));
                            }
                            let left = substitute_within(left, &solved);
                            let right = substitute_within(right, &solved);
                            if (mentions(&left, &own) || mentions(&right, &own))
                                && [&left, &right].iter().all(|ty| {
                                    !ty.any_part(
                                        &|part| matches!(part, Type::PackMember { .. }),
                                        &|_| false,
                                    )
                                })
                            {
                                equalities.push((left, right));
                            }
                        }
                    }
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
                .arguments
                .extend(relations.iter().filter_map(|(relation, bound)| {
                    ArgumentPatterns::read(relation, *bound, source, target, &generics, &type_at)
                }));
            let laws = condition.descendants().any(|node| {
                node.kind() == SyntaxKind::ConstraintCall
                    && crate::laws::query(unit, declared.at.file, &node).is_some()
            }) || defaulted.iter().any(|&index| {
                generic_default(&declared.syntax, &signature.generics[index])
                    .is_some_and(|node| crate::laws::default_parameter(signature, &node).is_some())
            });
            if laws
                || !equalities.is_empty()
                || template
                    .result
                    .as_ref()
                    .is_some_and(|result| mentions(result, &own))
            {
                evidence.enclosing_bindings.push(EnclosingBindings {
                    module,
                    declaration,
                    generics: signature.generics.clone(),
                    found,
                    equalities,
                    result: template.result.clone(),
                    defaulted: defaulted.to_vec(),
                    laws,
                });
            }
        }
        evidence
    }

    /// Alternatives that satisfy the positive requirements for one argument.
    pub(super) fn readings(
        &self,
        name: Option<&str>,
        position: Option<usize>,
        actual: &Type,
    ) -> Vec<MemberReading<'_>> {
        let mut groups = self
            .arguments
            .iter()
            .filter(|rule| rule.applies(name, position))
            .peekable();
        if groups.peek().is_none() {
            return vec![MemberReading::unchanged(actual)];
        }
        let mut readings = vec![MemberReading {
            ty: None,
            patterns: Vec::new(),
        }];
        for rule in groups {
            let alternatives: Vec<_> = rule
                .patterns
                .iter()
                .filter_map(|pattern| {
                    Some(FittedAlternative {
                        pattern,
                        rule,
                        bindings: rule.fit(pattern, actual)?,
                    })
                })
                .collect();
            let mut joined: Vec<MemberReading<'_>> = Vec::new();
            for reading in readings {
                for alternative in &alternatives {
                    let ty = selected_type(&alternative.bindings, rule.target);
                    if reading
                        .ty
                        .as_ref()
                        .zip(ty)
                        .is_some_and(|(one, two)| same_type(one, two) == Some(false))
                    {
                        continue;
                    }
                    let mut joined_reading = reading.clone();
                    joined_reading.ty = joined_reading.ty.or_else(|| ty.cloned());
                    joined_reading.patterns.push(alternative.clone());
                    if joined.iter().any(|previous| {
                        previous.same_binding(&joined_reading)
                            && matches!(
                                previous.specificity(&joined_reading),
                                Specific::More | Specific::Same
                            )
                    }) {
                        continue;
                    }
                    joined.retain(|previous| {
                        !previous.same_binding(&joined_reading)
                            || joined_reading.specificity(previous) != Specific::More
                    });
                    joined.push(joined_reading);
                }
            }
            readings = joined;
        }
        // This only sets the search order. The final comparison also
        // considers combinations that rank equally or are incomparable.
        let precedence: Vec<_> = readings
            .iter()
            .map(|reading| {
                readings
                    .iter()
                    .filter(|other| other.specificity(reading) == Specific::More)
                    .count()
            })
            .collect();
        let mut ordered: Vec<_> = precedence.into_iter().zip(readings).collect();
        ordered.sort_by_key(|(preceding, _)| *preceding);
        ordered.into_iter().map(|(_, reading)| reading).collect()
    }
}

struct EnclosingBindings {
    module: usize,
    declaration: usize,
    generics: Vec<GenericParameter>,
    found: Vec<Option<Argument>>,
    equalities: Vec<(Type, Type)>,
    result: Option<Type>,
    defaulted: Vec<usize>,
    laws: bool,
}

/// Result shapes and finite restrictions that hold for a reflected reading.
#[derive(Default)]
pub(super) struct ReflectedResultEvidence {
    /// Known result shapes inferred from the argument bindings in this reading.
    pub(super) shapes: Vec<Type>,
    /// Receiver requirements that can be checked against a candidate result (GEN-3).
    pub(super) requirements: Vec<ResultRequirement>,
}

/// The result type and known bindings of one receiving declaration.
pub(super) struct ResultRequirement {
    declaration: (usize, usize),
    known: Vec<Option<Argument>>,
    defaulted: Vec<usize>,
}

impl ResultRequirement {
    /// Whether the receiver's requirement holds for this callback trial (GEN-3).
    pub(super) fn admits(
        &self,
        typer: &mut Typer<'_>,
        callback: &Expr,
        supplied: &[(Option<&str>, &Expr)],
        law_inputs: &[CallbackLawInput],
    ) -> Option<bool> {
        let program = typer.program;
        let (module, declaration) = self.declaration;
        let unit = &program.modules[module];
        let Detail::Callable(signature) = &unit.details[declaration] else {
            return Some(true);
        };
        let Some(input) = dependent_slot(unit, signature).map(|slot| &signature.parameters[slot])
        else {
            return Some(true);
        };
        let dependent = &unit.dependent_packs[&input.at];
        let Some((callback_slot, template)) =
            signature
                .parameters
                .iter()
                .enumerate()
                .find_map(|(slot, parameter)| {
                    if parameter.at == dependent.function
                        && let Type::Function(template) = &parameter.ty
                    {
                        Some((slot, template))
                    } else {
                        None
                    }
                })
        else {
            return Some(true);
        };
        let Type::Function(actual) = &callback.ty else {
            return Some(true);
        };
        let generics = &signature.generics;
        let mut found = self.known.clone();
        if let (Some(expected), Some(actual)) = (&template.result, &actual.result) {
            infer(expected, actual, generics, &mut found);
        }
        let map: Vec<_> = generics
            .iter()
            .zip(&found)
            .filter_map(|(generic, value)| Some((generic.at, value.as_ref()?)))
            .collect();
        let mut packs = Vec::new();
        if let Ok(bound) = bind(
            program,
            template,
            callback,
            supplied,
            &pack_exclusions(dependent, &map),
        ) {
            if let Some(index) = type_pack(input, generics) {
                packs.push(TypePackArgument::keyed(index, bound.captured));
            }
            if let Some(pack) = &template.named_pack
                && let Some(index) = generics.iter().position(|generic| generic.at == pack.at)
            {
                packs.push(reflected_argument(
                    index,
                    template,
                    callback,
                    bound.reflected,
                ));
            }
        }
        let Ok(siblings) = typer.read_law_siblings(
            (self.declaration, signature),
            &mut found,
            &packs,
            law_inputs,
        ) else {
            return Some(false);
        };
        let argument = |typer: &Typer<'_>, slot, law| {
            if slot == callback_slot {
                typer.law_value(law, callback)
            } else if let Some((_, value)) = siblings.iter().find(|(index, _)| *index == slot) {
                typer.law_value(law, value)
            } else {
                law_inputs
                    .iter()
                    .find_map(|known| known.answer(typer, self.declaration, slot, law))
                    .flatten()
            }
        };
        typer.callback_requirement_holds(
            (self.declaration, signature),
            (&mut found, &self.defaulted),
            &packs,
            &argument,
        )
    }
}

impl EnclosingBindings {
    fn add_result_evidence(
        &self,
        program: &Program,
        bindings: &[(Location, &Argument)],
        evidence: &mut ReflectedResultEvidence,
        include_requirements: bool,
    ) -> Result<(), ()> {
        let mut found = self.found.clone();
        let mut learned = false;
        for (generic, slot) in self.generics.iter().zip(&mut found) {
            for (_, value) in bindings.iter().filter(|(at, _)| *at == generic.at) {
                if let Some(before) = slot {
                    if same_argument(before, value) == Some(false) {
                        return Err(());
                    }
                } else {
                    *slot = Some((*value).clone());
                    learned = true;
                }
            }
        }
        let unit = &program.modules[self.module];
        let declaration = &unit.declarations[self.declaration];
        if learned {
            super::quantifiers::bind(unit, declaration, &self.generics, &[], &mut found);
        }
        if include_requirements && (self.result.is_some() || self.laws) {
            evidence.requirements.push(ResultRequirement {
                declaration: (self.module, self.declaration),
                known: found.clone(),
                defaulted: self.defaulted.clone(),
            });
        }
        if !learned {
            return Ok(());
        }
        let map: Vec<_> = self
            .generics
            .iter()
            .zip(&found)
            .filter_map(|(generic, value)| Some((generic.at, value.as_ref()?)))
            .collect();
        if self.equalities.iter().any(|(left, right)| {
            let left = substitute_within(left, &map);
            let right = substitute_within(right, &map);
            !names_parameter(&left)
                && !names_parameter(&right)
                && same_type(&left, &right) == Some(false)
        }) {
            return Err(());
        }
        let unknown_type = Argument::Type(Type::Unknown);
        let unknown_value = Argument::Constant(unsaid());
        let partial: Vec<_> = self
            .generics
            .iter()
            .zip(&found)
            .map(|(generic, value)| {
                let unknown = if matches!(generic.kind, GenericKind::Value(_)) {
                    &unknown_value
                } else {
                    &unknown_type
                };
                (generic.at, value.as_ref().unwrap_or(unknown))
            })
            .collect();
        if let Some(shape) = self
            .result
            .as_ref()
            .map(|ty| substitute_within(ty, &partial))
            .filter(|ty| !open(ty))
        {
            evidence.shapes.push(shape);
        }
        Ok(())
    }
}

#[derive(Clone)]
struct FittedAlternative<'a> {
    pattern: &'a Type,
    rule: &'a ArgumentPatterns,
    bindings: Vec<Option<Argument>>,
}

/// One argument reading supplies a type or leaves the callback's type open.
#[derive(Clone)]
pub(super) struct MemberReading<'a> {
    /// The type from this reading, if it supplies one.
    pub(super) ty: Option<Type>,
    patterns: Vec<FittedAlternative<'a>>,
}

impl MemberReading<'_> {
    /// Keep the ordinary supplied type where no rule describes it.
    pub(super) fn unchanged(actual: &Type) -> Self {
        Self {
            ty: Some(actual.clone()),
            patterns: Vec::new(),
        }
    }

    /// Only a matching rule can constrain other choices in the trial.
    pub(super) fn evidence_type(&self) -> Option<&Type> {
        if self.patterns.is_empty() {
            return None;
        }
        self.ty.as_ref()
    }

    fn same_binding(&self, other: &Self) -> bool {
        let same_input = match (&self.ty, &other.ty) {
            (None, None) => true,
            (Some(one), Some(two)) => same_type(one, two) == Some(true),
            _ => false,
        };
        let supplies = |left: &Self, right: &Self| {
            left.shared_bindings().all(|(at, value)| {
                right
                    .shared_bindings()
                    .any(|(other, bound)| at == other && same_argument(value, bound) == Some(true))
            })
        };
        same_input && supplies(self, other) && supplies(other, self)
    }

    fn shared_bindings(&self) -> impl Iterator<Item = (Location, &Argument)> {
        self.patterns.iter().flat_map(|alternative| {
            alternative
                .rule
                .generics
                .iter()
                .zip(&alternative.bindings)
                // The synthetic last variable belongs to this argument's
                // callback parameter. Only the enclosing generics are shared.
                .take(alternative.rule.target)
                .filter_map(|(generic, value)| Some((generic.at, value.as_ref()?)))
        })
    }

    fn specificity(&self, other: &Self) -> Specific {
        self.patterns
            .iter()
            .zip(&other.patterns)
            .fold(Specific::Same, |order, (one, two)| {
                order.and(alternative_specificity(
                    (one.pattern, &one.rule.generics, &one.bindings),
                    (two.pattern, &two.rule.generics, &two.bindings),
                ))
            })
    }
}

/// GEN-3: e.g. every split argument of one keyed map gives the same K.
fn shared_bindings_agree(readings: &[Option<&MemberReading<'_>>]) -> bool {
    let mut bindings = Vec::new();
    for (at, value) in readings
        .iter()
        .flatten()
        .flat_map(|reading| reading.shared_bindings())
    {
        if bindings
            .iter()
            .any(|&(other, bound)| at == other && same_argument(value, bound) == Some(false))
        {
            return false;
        }
        bindings.push((at, value));
    }
    true
}

fn same_argument(one: &Argument, two: &Argument) -> Option<bool> {
    match (one, two) {
        (Argument::Type(one), Argument::Type(two)) => same_type(one, two),
        (Argument::Constant(one), Argument::Constant(two)) => one.value.same(&two.value),
        _ => Some(one == two),
    }
}

/// Choose a unique best compatible reading of the complete call (GEN-3).
/// A trial never commits generic bindings to a later trial.
pub(super) fn select_readings<'a>(
    choices: &'a [Vec<MemberReading<'a>>],
    mut compatible: impl FnMut(&[Option<&MemberReading<'a>>], bool) -> bool,
) -> Result<Vec<&'a MemberReading<'a>>, ReadingFailure> {
    let variable: Vec<_> = choices
        .iter()
        .enumerate()
        .filter(|(_, options)| options.len() > 1)
        .map(|(position, options)| (position, options.as_slice()))
        .collect();
    let mut readings: Vec<_> = choices
        .iter()
        .map(|options| match options.as_slice() {
            [only] => Some(only),
            _ => None,
        })
        .collect();
    if !shared_bindings_agree(&readings) || !compatible(&readings, false) {
        return Err(ReadingFailure::NoMatch);
    }
    let mut best: Vec<Vec<usize>> = Vec::new();
    let mut trial = Vec::new();
    if search_readings(
        &variable,
        &mut compatible,
        &mut readings,
        &mut trial,
        &mut best,
    ) {
        return Err(ReadingFailure::Ambiguous);
    }
    let first = best.first().ok_or(ReadingFailure::NoMatch)?;
    if best
        .iter()
        .skip(1)
        .any(|other| distinct_bindings(&variable, first, other))
    {
        return Err(ReadingFailure::Ambiguous);
    }
    let mut selected = choices
        .iter()
        .map(|options| options.first())
        .collect::<Option<Vec<_>>>()
        .ok_or(ReadingFailure::NoMatch)?;
    for ((position, options), &index) in variable.into_iter().zip(first) {
        selected[position] = &options[index];
    }
    Ok(selected)
}

/// Why the known types cannot choose a reading (GEN-3).
pub(super) enum ReadingFailure {
    /// No complete reading agreed with the known types.
    NoMatch,
    /// The best compatible readings give different bindings.
    Ambiguous,
}

fn distinct_bindings(
    choices: &[(usize, &[MemberReading<'_>])],
    one: &[usize],
    two: &[usize],
) -> bool {
    choices
        .iter()
        .zip(one.iter().zip(two))
        .any(|((_, options), (&one, &two))| !options[one].same_binding(&options[two]))
}

fn search_readings<'a>(
    choices: &[(usize, &'a [MemberReading<'a>])],
    compatible: &mut impl FnMut(&[Option<&MemberReading<'a>>], bool) -> bool,
    readings: &mut [Option<&'a MemberReading<'a>>],
    trial: &mut Vec<usize>,
    best: &mut Vec<Vec<usize>>,
) -> bool {
    let compare = |one: &[usize], two: &[usize]| {
        choices.iter().zip(one.iter().zip(two)).fold(
            Specific::Same,
            |order, ((_, options), (&one, &two))| {
                order.and(options[one].specificity(&options[two]))
            },
        )
    };
    if trial.len() == choices.len() {
        if best
            .iter()
            .any(|other| compare(other, trial) == Specific::More)
            || !compatible(readings, true)
        {
            return false;
        }
        best.retain(|other| compare(trial, other) != Specific::More);
        best.push(trial.clone());
        // Two different readings that are already maximal for every
        // argument remain ambiguous however the search finishes.
        let maximal = best.iter().filter(|reading| {
            choices.iter().zip(*reading).all(|((_, options), &chosen)| {
                options
                    .iter()
                    .all(|option| option.specificity(&options[chosen]) != Specific::More)
            })
        });
        let mut maximal = maximal;
        return maximal
            .next()
            .is_some_and(|first| maximal.any(|other| distinct_bindings(choices, first, other)));
    }
    let (position, options) = choices[trial.len()];
    for (index, option) in options.iter().enumerate() {
        trial.push(index);
        let dominated = best.iter().any(|other| {
            compare(other, trial) == Specific::More
                && choices
                    .iter()
                    .zip(other)
                    .skip(trial.len())
                    .all(|((_, options), &chosen)| {
                        options.iter().all(|option| {
                            matches!(
                                options[chosen].specificity(option),
                                Specific::Same | Specific::More
                            )
                        })
                    })
        });
        readings[position] = Some(option);
        if !dominated
            && shared_bindings_agree(readings)
            && compatible(readings, false)
            && search_readings(choices, compatible, readings, trial, best)
        {
            return true;
        }
        trial.pop();
    }
    readings[position] = None;
    false
}

fn selected_type(bindings: &[Option<Argument>], target: usize) -> Option<&Type> {
    match &bindings[target] {
        Some(Argument::Type(ty)) => Some(ty),
        _ => None,
    }
}

struct ArgumentPatterns {
    key: Option<ConstantValue>,
    target: usize,
    generics: Vec<GenericParameter>,
    patterns: Vec<Type>,
}

impl ArgumentPatterns {
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
        let argument_key = |operand: &SyntaxNode| {
            let Type::PackMember { pack, key, .. } = type_at(operand)? else {
                return None;
            };
            (pack == source.at
                && bound.is_none_or(
                    |bound| matches!(key.value, ConstantValue::Parameter { at, .. } if at == bound),
                ))
            .then_some(key)
        };
        let (key, alternatives) = if has_token(relation, SyntaxKind::EqEq) {
            // A positive equality supplies a single interpretation, in either order.
            if let Some(key) = argument_key(&left) {
                (key, vec![right])
            } else {
                (argument_key(&right)?, vec![left])
            }
        } else if right.kind() == SyntaxKind::ConstraintSet
            && tokens(relation)
                .any(|token| token.kind() == SyntaxKind::Ident && token.text() == "in")
        {
            (argument_key(&left)?, right.children().collect())
        } else {
            return None;
        };
        let mut patterns = Vec::new();
        for alternative in alternatives {
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
