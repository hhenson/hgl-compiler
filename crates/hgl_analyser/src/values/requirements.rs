//! `requires`: whether a declaration's condition holds for the generic
//! arguments it is called or applied with, and the operations a generic
//! body may call because its requirements name them (GEN-3, GEN-7,
//! GEN-15, GEN-16). Part of the typing pass ([`super`]).

use std::collections::{HashMap, HashSet};

use hgl_diagnostics::{Category, Code, Diagnostics, Location};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};
use hgl_source::FileId;

use super::candidates::{AtInstance, Given, Wanted};
use super::fit::{open, whole};
use super::inference::{fixed, mentions, placed, unsaid};
use super::operators::{Requested, from_place, has_generic_configuration_default, unreadable};
use super::{REQUIRING, Typer};
use crate::declared::{names_parameter, same_type, substitute_within};
use crate::hir::{self, Callee, Expr, ExprKind, Form, ValueCategory};
use crate::program::{Binding, Declaration, DeclarationKind, OperatorRef, Program, Role};
use crate::read::is_type;
use crate::syntax::{child, has_token, name_token, tokens, ungrouped};
use crate::types::{
    Argument, Constant, ConstantValue, Detail, GenericKind, GenericParameter, NamedKind, Signature,
    Type,
};
use crate::{codes, error};

impl<'a> Typer<'a> {
    /// GEN-3: each type written in the declaration being read is held
    /// to the `requires` of the structs it applies, at any depth.
    ///
    /// The type is read as it is written. GEN-2: "A struct application
    /// supplies all or none". Where it writes generic arguments each is
    /// read where it stands, as a call's is; where it writes none each
    /// parameter has its default. An argument that is written is never
    /// taken for the default.
    pub(super) fn applications(&mut self) {
        let within = self.declaration.syntax.span();
        let module = self.module;
        let file = self.file;
        let program = self.program;
        let written = module.written.iter().filter(|written| {
            written.at.file == file
                && written.at.span.start >= within.start
                && written.at.span.end <= within.end
        });
        for written in written {
            let mut found = Vec::new();
            struct_applications(&written.ty, &mut found);
            if !found
                .iter()
                .any(|(struct_, _)| required(program, *struct_).is_some())
            {
                continue;
            }
            // Each argument is held as the value it is (`crate::constant`),
            // however the type writes it, or as the default it takes.
            for (struct_, arguments) in &found {
                self.requires_met(written.at, *struct_, arguments);
            }
        }
    }

    /// The generic value parameters of the declaration being read that
    /// its `requires` binds, each where it is declared with the constant
    /// it is bound to ([`Typer::bound`]).
    ///
    /// GEN-3: "A positive conjunctive equality may bind one unresolved
    /// generic from known data; under negation/disjunction it only tests
    /// already-resolved data." An equality of a generic value parameter
    /// with a constant, `M == .slow` or `.slow == M`, alone or joined to
    /// the rest by `&&`, binds the parameter to the constant's value as
    /// one of the parameter's type; one under `!` or `||`, and a
    /// membership, binds nothing.
    pub(super) fn bindings(&self) -> Vec<(Location, Argument)> {
        let Some(clause) = child(&self.declaration.syntax, SyntaxKind::RequiresClause) else {
            return Vec::new();
        };
        let mut relations = Vec::new();
        if let Some(condition) = clause.children().next() {
            conjoined_relations(&condition, &mut relations);
        }
        let own: Vec<Location> = self
            .signature
            .generics
            .iter()
            .map(|generic| generic.at)
            .collect();
        let mut bound: Vec<(Location, Argument)> = Vec::new();
        for relation in relations {
            if !has_token(&relation, SyntaxKind::EqEq) {
                continue;
            }
            let operands: Vec<SyntaxNode> = relation.children().collect();
            let [left, right] = operands.as_slice() else {
                continue;
            };
            // The generic value parameter a lone name is.
            let parameter = |operand: &SyntaxNode| {
                (operand.kind() == SyntaxKind::NamedType)
                    .then(|| operand.text().to_string().trim().to_string())
                    .and_then(|name| {
                        self.signature.generics.iter().find(|generic| {
                            generic.name == name && matches!(generic.kind, GenericKind::Value(_))
                        })
                    })
            };
            let (generic, constant) = match (parameter(left), parameter(right)) {
                (Some(generic), None) => (generic, right),
                (None, Some(generic)) => (generic, left),
                _ => continue,
            };
            let GenericKind::Value(ty) = &generic.kind else {
                continue;
            };
            let Ok(value) = self.read_constant(constant, Some(ty)) else {
                continue;
            };
            if !crate::constant::worked_out(&value)
                || value.names_one_of(&own)
                || bound.iter().any(|(at, _)| *at == generic.at)
            {
                continue;
            }
            bound.push((
                generic.at,
                Argument::Constant(Constant {
                    text: constant.text().to_string().trim().to_string(),
                    value,
                }),
            ));
        }
        bound
    }

    /// The memberships the `requires` of the declaration being read
    /// states of its generic parameters, each parameter where it is
    /// declared with its members: `T in {i64, f64}` of a type parameter,
    /// `T == i64` as a membership of one, and `M in {.slow, .fast}` of a
    /// generic value parameter, each member read as a value of `M`'s
    /// type. Only those that hold wherever the condition does: alone or
    /// joined to the rest by `&&` (GEN-15). A parameter stated twice is
    /// in each: its members are those of the first.
    ///
    /// GEN-3: "A membership of a type parameter in a set of types,
    /// `requires T in {i64, f64}`, authorizes in the body (GEN-7) an
    /// operation that each listed type has a candidate for"; and,
    /// proposed, one of a generic value parameter a call that gives it
    /// as an option.
    pub(super) fn memberships(&self) -> Vec<(Location, Vec<Argument>)> {
        let Some(clause) = child(&self.declaration.syntax, SyntaxKind::RequiresClause) else {
            return Vec::new();
        };
        let mut relations = Vec::new();
        if let Some(condition) = clause.children().next() {
            conjoined_relations(&condition, &mut relations);
        }
        let own: Vec<Location> = self
            .signature
            .generics
            .iter()
            .map(|generic| generic.at)
            .collect();
        let mut seen = HashSet::new();
        relations
            .iter()
            .filter_map(|relation| self.stated_membership(relation, &own))
            .filter(|(at, _)| seen.insert(*at))
            .collect()
    }

    /// SOURCE-2 and GEN-7: the known type bound on a yield time's
    /// parameter, where its declaration states a finite set of types.
    pub(super) fn yield_time_types(&self, parameter: Location) -> Option<Vec<Type>> {
        let clause = child(&self.declaration.syntax, SyntaxKind::RequiresClause)?;
        let condition = clause.children().next()?;
        let own: Vec<Location> = self
            .signature
            .generics
            .iter()
            .map(|generic| generic.at)
            .collect();
        self.type_bound(&condition, parameter, &own)
    }

    /// A positive bound on `parameter` in `condition`. A conjunction
    /// holds only within each known bound. A disjunction has a bound
    /// only if each alternative has one. Unknown comparisons keep a
    /// member in the bound.
    fn type_bound(
        &self,
        condition: &SyntaxNode,
        parameter: Location,
        own: &[Location],
    ) -> Option<Vec<Type>> {
        match condition.kind() {
            SyntaxKind::ConstraintRelation => {
                let (at, members) = self.stated_membership(condition, own)?;
                if at != parameter {
                    return None;
                }
                members
                    .into_iter()
                    .map(|member| match member {
                        Argument::Type(ty) => Some(ty),
                        Argument::Constant(_) => None,
                    })
                    .collect()
            }
            SyntaxKind::ConstraintParen => {
                self.type_bound(&condition.children().next()?, parameter, own)
            }
            SyntaxKind::ConstraintBinary => {
                let mut sides = condition.children();
                let left = self.type_bound(&sides.next()?, parameter, own);
                let right = self.type_bound(&sides.next()?, parameter, own);
                if has_token(condition, SyntaxKind::AmpAmp) {
                    match (left, right) {
                        (Some(mut left), Some(right)) => {
                            left.retain(|ty| {
                                right
                                    .iter()
                                    .any(|other| same_type(ty, other) != Some(false))
                            });
                            Some(left)
                        }
                        (left, right) => left.or(right),
                    }
                } else if has_token(condition, SyntaxKind::PipePipe) {
                    let mut left = left?;
                    left.extend(right?);
                    Some(left)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// A relation that names the known members of one generic parameter.
    fn stated_membership(
        &self,
        relation: &SyntaxNode,
        own: &[Location],
    ) -> Option<(Location, Vec<Argument>)> {
        let mut operands = relation.children();
        let (Some(left), Some(right), None) = (operands.next(), operands.next(), operands.next())
        else {
            return None;
        };
        if left.kind() != SyntaxKind::NamedType {
            return None;
        }
        let name = left.text().to_string();
        let generic = self
            .signature
            .generics
            .iter()
            .find(|generic| generic.name == name.trim())?;
        // What the `read` pass said a type written here means: one that
        // names no generic parameter of the declaration.
        let type_at = |node: &SyntaxNode| -> Option<Type> {
            let ty = self.written.get(&(self.file, node.span().start))?;
            (whole(ty) && !mentions(ty, own)).then(|| (*ty).clone())
        };
        let within = tokens(relation).any(|token| token.text() == "in")
            && right.kind() == SyntaxKind::ConstraintSet;
        let equal = has_token(relation, SyntaxKind::EqEq);
        let members: Vec<Argument> = match &generic.kind {
            GenericKind::Type | GenericKind::Plain if within => right
                .children()
                .map(|member| type_at(&member).map(Argument::Type))
                .collect::<Option<Vec<_>>>()?,
            GenericKind::Type | GenericKind::Plain if equal => {
                vec![Argument::Type(type_at(&right)?)]
            }
            GenericKind::Value(ty) if within => right
                .children()
                .map(|member| {
                    let value = self.read_constant(&member, Some(ty)).ok()?;
                    (crate::constant::worked_out(&value) && !value.names_one_of(own)).then(|| {
                        Argument::Constant(Constant {
                            text: member.text().to_string().trim().to_string(),
                            value,
                        })
                    })
                })
                .collect::<Option<Vec<_>>>()?,
            _ => return None,
        };
        (!members.is_empty()).then_some((generic.at, members))
    }

    /// The types an operator's requirement admits for its result, with
    /// the call's generic arguments `map` in place: for `operator
    /// convert<T, V, O>(value: V) -> O requires O in {T, result<T,
    /// error>}` called as `convert<f64>(count)`, an `f64` and a
    /// `result<f64, error>`. `None` when the operator states none, or
    /// one of them still names a generic parameter.
    ///
    /// The requirement is read where it is `O in {types}` for the bare
    /// parameter `O` the result is, alone or joined to others by `&&`:
    /// where several are so joined, the result is in every set.
    pub(super) fn result_among(
        &self,
        operator: OperatorRef,
        public: &Signature,
        map: &[(Location, &Argument)],
        own: &[Location],
    ) -> Option<Vec<Type>> {
        let Some(Type::Parameter { name, .. }) = &public.result else {
            return None;
        };
        let module = &self.program.modules[operator.module];
        let declaration = &module.declarations[operator.declaration];
        let clause = child(&declaration.syntax, SyntaxKind::RequiresClause)?;
        let mut sets = Vec::new();
        result_sets(&clause.children().next()?, name, &mut sets);
        let file = module.files[declaration.file].file;
        // The types one set admits; `None` for a set with a member that
        // is no type, or that still names a generic parameter. Such a
        // set says nothing here.
        let types_of = |set: &SyntaxNode| -> Option<Vec<Type>> {
            let mut types = Vec::new();
            for member in set.children() {
                if !is_type(member.kind()) {
                    return None;
                }
                // What the `read` pass said the type written there
                // means.
                let start = member.span().start;
                let written = module
                    .written
                    .iter()
                    .find(|written| written.at.file == file && written.at.span.start == start)?;
                let ty = placed(substitute_within(&written.ty, map), own);
                if !whole(&ty) {
                    return None;
                }
                types.push(ty);
            }
            Some(types)
        };
        // Requirements joined by `&&` all hold: the result is in every
        // set.
        let mut among: Option<Vec<Type>> = None;
        for set in &sets {
            let Some(types) = types_of(set) else {
                continue;
            };
            among = Some(match among {
                None => types,
                Some(mut among) => {
                    among.retain(|ty| types.contains(ty));
                    among
                }
            });
        }
        among
    }

    /// Whether each named operator requirement in the condition of
    /// `candidate` holds at a call that takes it with `generics` for
    /// its generic parameters: where the requirement starts, and the
    /// verdict, `None` when it cannot be told.
    ///
    /// GEN-16: "Resolve their meaning from the referenced declaration
    /// or intrinsic ... a named operator requirement tests callability
    /// with the supplied types. A result arrow constrains that
    /// operator's result type". A constraint call that names no
    /// operator is a predicate, which is not read here.
    pub(super) fn requirements_hold(
        &mut self,
        candidate: &Declaration,
        parameters: &[GenericParameter],
        (module, generics): (usize, &[Argument]),
        condition: &SyntaxNode,
    ) -> Vec<(u32, Option<bool>)> {
        let calls: Vec<SyntaxNode> = condition
            .descendants()
            .filter(|node| {
                node.kind() == SyntaxKind::ConstraintCall
                    // GEN-10: calls within `each` need a verdict under
                    // its bound type, which this evaluator does not yet
                    // supply to named operator requirements (GEN-16).
                    && !node
                        .ancestors()
                        .any(|parent| parent.kind() == SyntaxKind::ConstraintEach)
            })
            .collect();
        if calls.is_empty() {
            return Vec::new();
        }
        let file = self.program.modules[module].files[candidate.file].file;
        let read = self.requirements_read(candidate, parameters, (module, generics), &calls);
        let mut verdicts = Vec::new();
        for (call, read) in calls.iter().zip(read) {
            let verdict = match read {
                Some((operator, operands, result)) => {
                    self.callable(operator, (file, operands), result.as_ref())
                }
                None => None,
            };
            verdicts.push((call.span().start, verdict));
        }
        verdicts
    }

    /// Reports a `requires` that does not hold where a function is
    /// called, or a struct applied, with `generics` for its generic
    /// parameters. `at` is the call or the type, and `given` what it
    /// says of the generic value parameters.
    ///
    /// GEN-3: "A call of a function whose `requires` does not hold with
    /// the generic arguments the call resolves is rejected, and so is
    /// an application of a struct whose `requires` does not hold with
    /// its arguments." Nothing is reported where it cannot be told
    /// ([`requires_holds`]). A named operator requirement is told for a
    /// generic parameter of the declaration being read too: it holds
    /// where that declaration states it (GEN-7).
    pub(super) fn requires_met(
        &mut self,
        at: Location,
        callee: (usize, usize),
        generics: &[Argument],
    ) {
        let Some((condition, held, unvalued)) = self.requires_told(callee, generics) else {
            return;
        };
        // A construction that writes its generic arguments,
        // `Sized<str>(value: "x")`, writes a type, of which the same
        // has been reported ([`applications`]).
        if self.unmet_reported(at) {
            return;
        }
        let program = self.program;
        let own: Vec<Location> = self
            .signature
            .generics
            .iter()
            .map(|generic| generic.at)
            .collect();
        // A struct applied with a generic parameter of the declaration
        // being read is told for each instance ([`instances_hold`]).
        let function =
            program.modules[callee.0].declarations[callee.1].kind != DeclarationKind::Struct;
        match held {
            Some(false) => self
                .diagnostics
                .emit(unmet(program, callee, at, &condition)),
            // GEN-7: a generic body "is judged once, where it is declared,
            // by what its `requires` states, and never again for the
            // generic arguments a call of it gives" (hhenson/hgl_spec#144:
            // so is a call in it of another generic function).
            None if function && passes_on(generics, &own) => {
                if let Some(report) =
                    self.unmet_under_stated(at, callee, (generics, &[]), &condition)
                {
                    self.diagnostics.emit(report);
                }
            }
            None if unvalued && !passes_on(generics, &own) => self
                .diagnostics
                .emit(untold(program, callee, at, &condition)),
            _ => {}
        }
    }

    /// GEN-3 and GEN-10: a function that receives resolved type packs
    /// must have a requirement known to hold for those types. Report
    /// both a false condition and a form this pass cannot judge.
    pub(super) fn pack_requires_met(
        &mut self,
        at: Location,
        callee: (usize, usize),
        generics: &[Argument],
        type_packs: &[hir::TypePackArgument],
    ) {
        let Some((condition, held, unvalued)) =
            self.requires_told_with_packs(callee, generics, type_packs)
        else {
            return;
        };
        if self.unmet_reported(at) {
            return;
        }
        let program = self.program;
        let own: Vec<_> = self
            .signature
            .generics
            .iter()
            .map(|generic| generic.at)
            .collect();
        let report = match held {
            Some(true) => return,
            Some(false) => unmet(program, callee, at, &condition),
            None if passes_on(generics, &own) || packs_pass_on(type_packs, &own) => {
                if let Some(report) =
                    self.unmet_under_stated(at, callee, (generics, type_packs), &condition)
                {
                    self.diagnostics.emit(report);
                }
                return;
            }
            None if unvalued => untold(program, callee, at, &condition),
            None => untold_with_reason(
                program,
                callee,
                at,
                &condition,
                "this compiler cannot judge the condition for these type packs",
            ),
        };
        self.diagnostics.emit(report);
    }

    /// Each member of `combination`, by the generic parameter it is of:
    /// `` `T` = `str` ``.
    fn members_named(&self, combination: &[(Location, Argument)]) -> Vec<String> {
        combination
            .iter()
            .map(|(at, member)| {
                let parameter = self
                    .signature
                    .generics
                    .iter()
                    .find(|generic| generic.at == *at)
                    .map_or("_", |generic| generic.name.as_str());
                let member = match member {
                    Argument::Type(ty) => ty.to_string(),
                    Argument::Constant(constant) => constant.text.clone(),
                };
                format!("`{parameter}` = `{member}`")
            })
            .collect()
    }

    /// The rejection of a call at `at`, in the generic body being read, of
    /// `callee` with `generics`, which pass on the body's own generic
    /// parameters, where what the body's `requires` states of those does
    /// not make the callee's `requires`, `condition`, known to hold.
    /// `None` where it does.
    ///
    /// GEN-7: "It is judged once, where it is declared, by what its
    /// `requires` states" (hhenson/hgl_spec#144: a call of another generic
    /// function whose `requires` the body's own stated requirements do not
    /// make known to hold rejects where the body is declared). What the
    /// body states: an equality that binds a parameter (GEN-3), and a
    /// membership, every member of which, and every combination of them
    /// for two parameters, must make the callee's `requires` hold. So under
    /// `requires Y in {i64, f64}` a call of `inner<X>` under `requires X in
    /// {i64, f64}` holds, and under a superset it does not; under
    /// `requires Y == 1.0` one of `one_only` under `requires X == 1.0`
    /// holds. A named operator requirement the callee states is answered
    /// by the body's own (GEN-16).
    fn unmet_under_stated(
        &mut self,
        at: Location,
        callee: (usize, usize),
        (generics, type_packs): (&[Argument], &[hir::TypePackArgument]),
        condition: &SyntaxNode,
    ) -> Option<hgl_diagnostics::Diagnostic> {
        let program = self.program;
        let own: Vec<Location> = self
            .signature
            .generics
            .iter()
            .map(|generic| generic.at)
            .collect();
        let memberships: Vec<(Location, Vec<Argument>)> = self
            .memberships()
            .into_iter()
            .filter(|(parameter, _)| {
                passes_on(generics, &[*parameter]) || packs_pass_on(type_packs, &[*parameter])
            })
            .collect();
        let mut combinations = super::candidates::combinations(&memberships);
        if memberships.is_empty() {
            combinations.push(Vec::new());
        }
        let bound = self.bound.clone();
        let name = &program.modules[callee.0].declarations[callee.1].name;
        let body = &self.declaration.name;
        for combination in combinations.iter().take(crate::INSTANCES + 1) {
            let given: Vec<(Location, &Argument)> = bound
                .iter()
                .chain(combination)
                .map(|(at, argument)| (*at, argument))
                .collect();
            let placed_generics: Vec<Argument> = generics
                .iter()
                .map(|argument| given_in(argument, &given, &own))
                .collect();
            let placed_packs: Vec<_> = type_packs
                .iter()
                .map(|pack| hir::TypePackArgument {
                    parameter: pack.parameter,
                    types: pack
                        .types
                        .iter()
                        .map(|ty| placed(substitute_within(ty, &given), &own))
                        .collect(),
                })
                .collect();
            // ERR-9: "Failure during argument evaluation precedes later
            // admission checks": a generic argument worked out from the
            // body's parameters that fails for what the body admits.
            let made = Made {
                callee,
                generics: generics.to_vec(),
                answers: Vec::new(),
                required: None,
                at,
            };
            if let Some(report) = argument_fails(program, &made, &given, &own) {
                return Some(if combination.is_empty() {
                    report
                } else {
                    report.with_note(format!(
                        "for {}, which the `requires` of `{body}` admits",
                        self.members_named(combination).join(" and ")
                    ))
                });
            }
            let held = self
                .requires_told_with_packs(callee, &placed_generics, &placed_packs)
                .map(|(_, held, _)| held);
            if held == Some(Some(true)) {
                continue;
            }
            let members = self.members_named(combination);
            let message = match (held, members.is_empty()) {
                (Some(Some(false)), false) => format!(
                    "the `requires` of `{name}` does not hold for this call for {}, which the \
                     `requires` of `{body}` admits",
                    members.join(" and ")
                ),
                (_, false) => format!(
                    "the `requires` of `{name}` is not known to hold for this call for {}, which \
                     the `requires` of `{body}` admits",
                    members.join(" and ")
                ),
                (Some(Some(false)), true) => format!(
                    "the `requires` of `{name}` does not hold for this call with what the \
                     `requires` of `{body}` binds"
                ),
                (_, true) => format!(
                    "the `requires` of `{name}` is not known to hold for this call under what the \
                     `requires` of `{body}` states"
                ),
            };
            let declared_in = &program.modules[callee.0];
            let declared = &declared_in.declarations[callee.1];
            return Some(
                error(
                    Category::Constraint,
                    codes::REQUIRES_UNMET,
                    at,
                    message,
                    "GEN-7",
                )
                .with_related(
                    Location::new(declared_in.files[declared.file].file, condition.span()),
                    "the condition that is not known to hold",
                )
                .with_note(format!(
                    "a generic body is judged once, where it is declared, by what its \
                         `requires` states (GEN-7): state of `{body}`'s generic parameters what \
                         makes this hold"
                )),
            );
        }
        None
    }

    /// Reports a `requires` that does not hold, or is not told for a
    /// constant this compiler does not work out, for an instance a call
    /// at `at` makes: `callee` with `generics`, called at `made` in the
    /// body of what `at` calls ([`instances_hold`]).
    fn instance_met(
        &mut self,
        (at, made): (Location, Location),
        callee: (usize, usize),
        generics: &[Argument],
    ) {
        let Some((condition, held, unvalued)) = self.requires_told(callee, generics) else {
            return;
        };
        if self.unmet_reported(at) {
            return;
        }
        let program = self.program;
        let report = match held {
            Some(false) => unmet(program, callee, at, &condition),
            None if unvalued => untold(program, callee, at, &condition),
            _ => return,
        };
        let name = &program.modules[callee.0].declarations[callee.1].name;
        self.diagnostics.emit(report.with_related(
            made,
            format!(
                "`{name}` is called here, with the generic arguments this call gives the \
                 function it calls"
            ),
        ));
    }

    /// True where a `requires.unmet` has been reported at `at` already.
    fn unmet_reported(&self, at: Location) -> bool {
        self.diagnostics.as_slice().iter().any(|earlier| {
            earlier.code == Code::Catalogued(codes::REQUIRES_UNMET)
                && earlier.primary.file == at.file
                && earlier.primary.span.start == at.span.start
        })
    }

    /// Whether the `requires` of `callee` holds with `generics` for its
    /// generic parameters: the condition, whether it holds (`None` where
    /// that is not told), and whether a constant it compares is one this
    /// compiler does not work out ([`Condition::unvalued`]). `None` for a
    /// callee that states no `requires`.
    fn requires_told(
        &mut self,
        callee: (usize, usize),
        generics: &[Argument],
    ) -> Option<(SyntaxNode, Option<bool>, bool)> {
        self.requires_told_with_packs(callee, generics, &[])
    }

    /// The same check with any resolved type packs available to GEN-10
    /// quantifiers in the condition.
    fn requires_told_with_packs(
        &mut self,
        (module, declaration): (usize, usize),
        generics: &[Argument],
        type_packs: &[hir::TypePackArgument],
    ) -> Option<(SyntaxNode, Option<bool>, bool)> {
        let program = self.program;
        let (parameters, condition) = required(program, (module, declaration))?;
        let declared = &program.modules[module].declarations[declaration];
        // GEN-16: "a named operator requirement tests callability with
        // the supplied types."
        let verdicts = self.requirements_hold(declared, parameters, (module, generics), &condition);
        let callable = |node: &SyntaxNode| -> Option<bool> {
            let start = node.span().start;
            verdicts
                .iter()
                .find(|(at, _)| *at == start)
                .and_then(|(_, verdict)| *verdict)
        };
        // What a constant the condition compares with comes to, read
        // where the declaration is written (`crate::constant`). The
        // values the call gives are `generics`, each read as a value of
        // its parameter's type where the call writes it or leaves it to
        // its default.
        let this = &*self;
        let constant = |node: &SyntaxNode, ty: Option<&Type>| -> Option<ConstantValue> {
            this.read_in(declared.at.file, node, ty).ok()
        };
        let unvalued = std::cell::Cell::new(false);
        let held = requires_holds(
            program,
            (module, declaration),
            generics,
            &condition,
            &Told {
                constant: &constant,
                callable: &callable,
                unvalued: &unvalued,
                type_packs,
            },
        );
        Some((condition, held, unvalued.get()))
    }

    /// Each of the constraint calls `calls`, written in the `requires`
    /// of `declared` of `module`, read with `generics` for the
    /// declaration's generic parameters: the operator it names, the
    /// types it is supplied with, each with where it is written, and
    /// the type its arrow states. `None` for one that names no
    /// operator, a predicate, and for one supplied with what is no
    /// type, or with a type that is not known.
    fn requirements_read(
        &self,
        declared: &Declaration,
        parameters: &[GenericParameter],
        (module, generics): (usize, &[Argument]),
        calls: &[SyntaxNode],
    ) -> Vec<Option<Read>> {
        let program = self.program;
        let declared_in = &program.modules[module];
        let file = declared_in.files[declared.file].file;
        let own: Vec<Location> = parameters.iter().map(|generic| generic.at).collect();
        let map: Vec<(Location, &Argument)> = own.iter().copied().zip(generics).collect();
        // What a type written in the condition means, the declaration's
        // generic parameters in place.
        let written = |node: &SyntaxNode| -> Option<Type> {
            let start = node.span().start;
            let read = declared_in
                .written
                .iter()
                .find(|written| written.at.file == file && written.at.span.start == start)?;
            let ty = placed(substitute_within(&read.ty, &map), &own);
            whole(&ty).then_some(ty)
        };
        calls
            .iter()
            .map(|call| {
                let name = name_token(call)?;
                // What the name is bound to where the declaration is
                // written.
                let start = name.span().start;
                let used = declared_in
                    .uses
                    .iter()
                    .find(|used| used.at.file == file && used.at.span.start == start)?;
                let Binding::Declarations {
                    module: operator_module,
                    declarations,
                } = &used.binding
                else {
                    return None;
                };
                let declaration = declarations.iter().copied().find(|&declaration| {
                    program.modules[*operator_module].declarations[declaration].kind
                        == DeclarationKind::Operator
                })?;
                let mut operands = Vec::new();
                let mut result = None;
                for part in call.children() {
                    match part.kind() {
                        SyntaxKind::QualifiedName => {}
                        SyntaxKind::ReturnType => {
                            result = Some(written(&part.children().next()?)?);
                        }
                        kind if is_type(kind) => {
                            let ty = written(&part)?;
                            operands.push((part, ty));
                        }
                        _ => return None,
                    }
                }
                let operator = OperatorRef {
                    module: *operator_module,
                    declaration,
                };
                Some((operator, operands, result))
            })
            .collect()
    }

    /// What answers each requirement that `declaration` of `module`
    /// states, at a call that takes it with `generics` for its generic
    /// parameters: one [`hir::Answer`] for each constraint call that
    /// holds wherever its `requires` does, in the order written.
    ///
    /// GEN-16: "a named operator requirement tests callability with the
    /// supplied types ... it is tested against the operator's value
    /// candidates, and holds where one most-specific value candidate
    /// takes the types supplied (GEN-5)". That candidate is the answer.
    /// GEN-7: where the types name a generic parameter of the
    /// declaration the call is written in, a requirement that
    /// declaration states is the answer, and what answers that is known
    /// where it is called in turn.
    ///
    /// The candidate that answers may state requirements of its own,
    /// which are answered with it. That ends: a candidate is selected
    /// only where each of its requirements was told to hold, which is
    /// tested no deeper than [`REQUIRING`].
    pub(super) fn answers(
        &mut self,
        module: usize,
        declaration: usize,
        generics: &[Argument],
    ) -> Vec<hir::Answer> {
        self.answers_for_default(module, declaration, generics, &[])
    }

    /// Also report a requirement known to fail where an inherited default
    /// actually uses it. A symbolic application can leave an answer unread.
    pub(super) fn answers_for_default(
        &mut self,
        module: usize,
        declaration: usize,
        generics: &[Argument],
        uses: &[(usize, Location)],
    ) -> Vec<hir::Answer> {
        let program = self.program;
        let declared_in = &program.modules[module];
        let declared = &declared_in.declarations[declaration];
        let Some(Detail::Callable(signature)) = declared_in.details.get(declaration) else {
            return Vec::new();
        };
        let Some(clause) = child(&declared.syntax, SyntaxKind::RequiresClause) else {
            return Vec::new();
        };
        let Some(condition) = clause.children().next() else {
            return Vec::new();
        };
        let mut calls = Vec::new();
        required_calls(&condition, &mut calls);
        if calls.is_empty() {
            return Vec::new();
        }
        if self.requiring >= REQUIRING {
            return vec![hir::Answer::Unread; calls.len()];
        }
        let file = declared_in.files[declared.file].file;
        let read =
            self.requirements_read(declared, &signature.generics, (module, generics), &calls);
        let report_unmet = !uses.is_empty()
            && generics.len() == signature.generics.len()
            && generics.iter().all(resolved_default_argument);
        read.into_iter()
            .enumerate()
            .map(|(index, read)| {
                let Some((operator, operands, result)) = read else {
                    return hir::Answer::Unread;
                };
                match self.answered(operator, (file, operands), result.as_ref()) {
                    Some(Answered::Stated(index)) => hir::Answer::Stated(index),
                    Some(Answered::Selected {
                        module,
                        declaration,
                        generics,
                    }) => {
                        self.requiring += 1;
                        let requirements = self.answers(module, declaration, &generics);
                        self.requiring -= 1;
                        hir::Answer::Candidate {
                            operator: (operator.module, operator.declaration),
                            module,
                            declaration,
                            generics,
                            requirements,
                        }
                    }
                    Some(Answered::Unmet) if report_unmet => {
                        for &(_, at) in uses.iter().filter(|(used, _)| *used == index) {
                            if !self.unmet_reported(at) {
                                self.diagnostics.emit(unmet(
                                    program,
                                    (module, declaration),
                                    at,
                                    &condition,
                                ));
                            }
                        }
                        hir::Answer::Unread
                    }
                    Some(Answered::Unmet) | None => hir::Answer::Unread,
                }
            })
            .collect()
    }

    /// Whether `operator` is callable with values of the types
    /// `operands`, to give `result` where one is stated (GEN-16).
    /// `None` when it cannot be told.
    ///
    /// A requirement names a value operation, whatever declares it:
    /// core's OPS-9 has "Addition requires a value `add_(T, T) -> T`",
    /// which a node's hook calls (FN-3), and a graph that states the
    /// same requirement may call that node's candidate. So it is tested
    /// against the operator's value candidates.
    ///
    /// Types that name a generic parameter of the declaration being
    /// read are callable where its own requirements state it (GEN-7).
    /// Otherwise, and for any other types, the operator is callable
    /// when one most-specific value candidate admits them (GEN-5). So a
    /// candidate that takes any one type, core's `eq_<T>`, answers for
    /// a generic parameter with nothing stated.
    fn callable(
        &mut self,
        operator: OperatorRef,
        supplied: (FileId, Vec<(SyntaxNode, Type)>),
        result: Option<&Type>,
    ) -> Option<bool> {
        self.answered(operator, supplied, result)
            .map(|answered| !matches!(answered, Answered::Unmet))
    }

    /// What answers a requirement that `operator` is callable with
    /// values of the types `operands`, to give `result` where one is
    /// stated: see [`Typer::callable`], which says when it holds.
    /// `None` when it cannot be told.
    fn answered(
        &mut self,
        operator: OperatorRef,
        (file, operands): (FileId, Vec<(SyntaxNode, Type)>),
        result: Option<&Type>,
    ) -> Option<Answered> {
        let own: Vec<Location> = self
            .signature
            .generics
            .iter()
            .map(|generic| generic.at)
            .collect();
        let generic = operands.iter().any(|(_, ty)| mentions(ty, &own))
            || result.is_some_and(|result| mentions(result, &own));
        let program = self.program;
        let Some(Detail::Callable(public)) = program.modules[operator.module]
            .details
            .get(operator.declaration)
        else {
            return None;
        };
        // A requirement is tested as the call it names would select
        // (GEN-8). It writes no generic argument: its arrow is what its
        // place expects, and the types it is supplied with are its
        // arguments.
        let mut found: Vec<Option<Argument>> = vec![None; public.generics.len()];
        from_place(public, result, &mut found);
        let supplied = requirement_inputs(public, operands.iter().map(|(_, ty)| ty));
        let Requested {
            place: requested,
            among,
            unfit,
            ..
        } = self.output_requested(operator, public, found, &supplied, false);
        // GEN-13: a default the call would take is not of its
        // parameter's type as these types resolve it. The call would be
        // rejected, and so the operator is not callable with them.
        if unfit.is_some() {
            return Some(Answered::Unmet);
        }
        // The arrow is what the place of the call would expect. Where
        // the operator's defaults make the call's output another type,
        // the call does not give the arrow's: with
        // `operator count<T, const mode: i64 = 0>(value: T) -> i64`,
        // `requires count(bool) -> str` holds for no candidate.
        if let (Some(requested), Some(arrow)) = (&requested, result)
            && !self.admits(arrow, requested)
        {
            return Some(Answered::Unmet);
        }
        // What the call asks of the result: with the operator's
        // defaults, or as its arrow says.
        let asked: Option<Type> = requested.or_else(|| result.cloned());
        if generic {
            // GEN-7: what the declaration being read states is evidence
            // for the same call: this operator, these types, and the
            // same output asked. `requires window(T) -> list<T, 5>` does
            // not say the bare `window(T)`, which asks for the
            // operator's default, is callable.
            let types: Vec<&Type> = operands.iter().map(|(_, ty)| ty).collect();
            let stated = self.requirements().into_iter().find(|requirement| {
                requirement.operator == operator
                    && requirement.operands.iter().eq(types.iter().copied())
                    && self.asked_by(requirement) == Some(asked.clone())
            });
            if let Some(stated) = stated {
                return Some(Answered::Stated(stated.index));
            }
        }
        if self.requiring >= REQUIRING {
            return None;
        }
        // In a graph the candidates of a call are the temporal ones
        // too (GEN-5): a requirement is for the value ones.
        let mut candidates = self.value_candidates(operator);
        candidates.retain(|&(module, declaration, _)| {
            matches!(
                program.modules[module].declarations[declaration]
                    .kind
                    .candidate(),
                Some((Role::Value, _))
            )
            // GEN-16: "A candidate that marks a parameter `var`
            // (FN-15) answers no requirement: a requirement supplies
            // types, and no place."
            && !matches!(
                program.modules[module].details.get(declaration),
                Some(Detail::Callable(signature))
                    if signature.parameters.iter().any(|parameter| parameter.var)
            )
        });
        if unreadable(public, &candidates) {
            return None;
        }
        // Each operand as an argument in its place, of the type
        // supplied: there is no expression to read.
        let given: Vec<Given> = operands
            .into_iter()
            .enumerate()
            .map(|(position, (node, ty))| {
                let at = Location::new(file, node.span());
                let value = Expr {
                    kind: ExprKind::Unread,
                    ty,
                    at,
                    category: ValueCategory::Unsettled,
                };
                Given::read(position, node, value)
            })
            .collect();
        // A requirement writes no generic argument (GEN-16).
        let wanted = Wanted {
            place: asked.as_ref(),
            among,
            written: false,
        };
        self.requiring += 1;
        let (mut selected, untold) = self.admitted(operator, &candidates, &given, &wanted);
        self.requiring -= 1;
        if untold {
            return None;
        }
        // GEN-5: "Select exactly one most-specific admitted candidate."
        Some(match (selected.pop(), selected.is_empty()) {
            (Some(chosen), true) if !chosen.ambiguous_reads => Answered::Selected {
                module: chosen.module,
                declaration: chosen.declaration,
                generics: chosen.generics,
            },
            _ => Answered::Unmet,
        })
    }

    /// What a requirement asks of its operator's result, as the call it
    /// names would (GEN-8): the operator's result with its arrow, the
    /// types it is supplied with and the operator's defaults in place,
    /// or its arrow where those say nothing. The outer `None` is for a
    /// requirement no call meets: a default that call would take does
    /// not fit (GEN-13), or its arrow is not what the operator's result
    /// is with those in place.
    fn asked_by(&self, requirement: &Requirement) -> Option<Option<Type>> {
        let operator = requirement.operator;
        let arrow = requirement.result.as_ref();
        let Some(Detail::Callable(public)) = self.program.modules[operator.module]
            .details
            .get(operator.declaration)
        else {
            return Some(arrow.cloned());
        };
        let mut found: Vec<Option<Argument>> = vec![None; public.generics.len()];
        from_place(public, arrow, &mut found);
        let supplied = requirement_inputs(public, &requirement.operands);
        let Requested { place, unfit, .. } =
            self.output_requested(operator, public, found, &supplied, false);
        if unfit.is_some() {
            return None;
        }
        // An arrow the call's own output cannot be: see `callable`.
        if let (Some(place), Some(arrow)) = (&place, arrow)
            && !self.admits(arrow, place)
        {
            return None;
        }
        Some(place.or_else(|| arrow.cloned()))
    }

    /// The named operator requirements the enclosing declaration
    /// states: those that stand alone in its `requires`, or are joined
    /// to the rest by `&&`. One under a negation or a disjunction says
    /// nothing that holds for sure.
    ///
    /// GEN-16: "a named operator requirement tests callability with the
    /// supplied types. A result arrow constrains that operator's result
    /// type". The name is an operator when it is bound to one; a
    /// requirement supplied with what is no type is not read here.
    fn requirements(&self) -> Vec<Requirement> {
        let Some(clause) = child(&self.declaration.syntax, SyntaxKind::RequiresClause) else {
            return Vec::new();
        };
        let mut calls = Vec::new();
        if let Some(condition) = clause.children().next() {
            required_calls(&condition, &mut calls);
        }
        let program = self.program;
        // What the `read` pass said a type written here means.
        let type_at = |node: &SyntaxNode| -> Option<Type> {
            self.written
                .get(&(self.file, node.span().start))
                .map(|ty| (*ty).clone())
        };
        calls
            .iter()
            .enumerate()
            .filter_map(|(index, call)| {
                let name = name_token(call)?;
                let (module, declarations) = self.declarations(&name)?;
                let declaration = declarations.iter().copied().find(|&declaration| {
                    program.modules[module].declarations[declaration].kind
                        == DeclarationKind::Operator
                })?;
                let mut operands = Vec::new();
                let mut result = None;
                for part in call.children() {
                    match part.kind() {
                        // `alias::name`: the callee.
                        SyntaxKind::QualifiedName => {}
                        SyntaxKind::ReturnType => {
                            result = Some(type_at(&part.children().next()?)?);
                        }
                        kind if is_type(kind) => operands.push(type_at(&part)?),
                        _ => return None,
                    }
                }
                Some(Requirement {
                    index,
                    operator: OperatorRef {
                        module,
                        declaration,
                    },
                    operands,
                    result,
                })
            })
            .collect()
    }

    /// What the enclosing declaration's requirements say of a call of
    /// `operator` with the arguments `given`: `Some` when one of them
    /// is that call.
    ///
    /// GEN-7: "A generic body is checked under its stated
    /// requirements". A requirement is the call when it names the
    /// operator and is supplied with the arguments' types, each in its
    /// place: under `requires add_(T, T) -> T`, `add_(lhs, rhs)` of two
    /// `T` is a `T`, and so is `add_(rhs: b, lhs: a)`. An argument with
    /// no type of its own, a sequence or a tuple written out, is tried
    /// as the type the requirement supplies in its place: `[a] + [b]`
    /// under `requires add_(list<T>, list<T>) -> list<T>`.
    ///
    /// A requirement speaks for types of which one names a generic
    /// parameter of the declaration, its result's among them: any
    /// other call selects its candidate here, as it does in a body
    /// that has no generic parameter. `requested` is the output the
    /// call itself asks for, through the generic arguments it writes
    /// and the operator's defaults (GEN-8): a requirement that gives
    /// another is not this call. `expected` is the type the call's
    /// place expects, which chooses among requirements supplied alike.
    /// `written` is the operator's generic arguments as far as the call
    /// writes them, `make<T>()`: the requirement is one those agree
    /// with.
    pub(super) fn required(
        &mut self,
        operator: OperatorRef,
        given: &[&Given],
        (requested, expected): (Option<&Type>, Option<&Type>),
        written: Option<&[Option<Argument>]>,
    ) -> Option<Required> {
        let own: Vec<Location> = self
            .signature
            .generics
            .iter()
            .map(|generic| generic.at)
            .collect();
        if own.is_empty() {
            return None;
        }
        let Some(Detail::Callable(public)) = self.program.modules[operator.module]
            .details
            .get(operator.declaration)
        else {
            return None;
        };
        // EXPR-2: "Named arguments use the selected public callable's
        // parameter names". Each argument is given to one of the
        // operator's own parameters, by its position or by that
        // parameter's name (GEN-4). A name the operator does not
        // declare is an option of a candidate, which no requirement
        // speaks for.
        let mut places: Vec<RequiredPlace> = Vec::with_capacity(given.len());
        for argument in given {
            // One read as the type of its place whose first reading
            // stands was reported itself: the call is not judged here.
            if argument.by_place && !argument.retried {
                return None;
            }
            let parameter = argument.key.public_parameter(public)?;
            // A requirement names ordinary operands (GEN-16). Their
            // positions skip the configuration slots from FN-18.
            let operand = (!public.parameters[parameter].configuration).then(|| {
                public.parameters[..parameter]
                    .iter()
                    .filter(|parameter| !parameter.configuration)
                    .count()
            });
            places.push(RequiredPlace { operand, parameter });
        }
        // Past the function's bound no argument is tried, and the call
        // is left to be not judged where its candidates are read.
        if self.rereads == 0 && given.iter().any(|argument| argument.retried) {
            return None;
        }
        // What the call's place expects, where that is known.
        let wanted = expected.filter(|expected| !open(expected));
        let mut first: Option<Required> = None;
        // The argument given to each place, in the operator's order.
        let mut ordered: Vec<_> = given
            .iter()
            .zip(&places)
            .filter_map(|(&argument, place)| Some((place.operand?, argument)))
            .collect();
        ordered.sort_unstable_by_key(|&(operand, _)| operand);
        if ordered
            .iter()
            .enumerate()
            .any(|(index, &(operand, _))| index != operand)
        {
            return None;
        }
        for requirement in self.requirements() {
            // One that names a generic parameter of the declaration,
            // among the types it is supplied with or in its result:
            // `make() -> T`.
            let generic = requirement
                .operands
                .iter()
                .chain(&requirement.result)
                .any(|ty| mentions(ty, &own));
            if requirement.operator != operator
                || requirement.operands.len() != ordered.len()
                || !generic
            {
                continue;
            }
            // GEN-8: what the call requests is what it gets. Under
            // `requires window(T) -> list<T, 5>`, a `window(value)` that
            // takes the operator's default of three is another call, and
            // so is a `window<T, 5>(value)` under `requires window(T)`,
            // which asks for that default.
            let Some(asks) = self.asked_by(&requirement) else {
                continue;
            };
            if let Some(requested) = requested
                && !asks
                    .as_ref()
                    .is_some_and(|asks| self.admits(requested, asks))
            {
                continue;
            }
            // GEN-8: "A call may supply its generic arguments as a
            // generic constructor does". With them in place, what the
            // operator's own signature says of a parameter or of the
            // result is what the requirement says of it.
            if let Some(written) = written {
                // What the call does not write is left open, and agrees
                // with any: with `operator make<A, B>() -> tuple<A, B>`,
                // `make<T>()` asks for a `tuple<T, _>`, which a
                // requirement `make() -> tuple<U, T>` is not.
                let explicit: Vec<Argument> = written
                    .iter()
                    .zip(&public.generics)
                    .map(|(argument, generic)| {
                        argument.clone().unwrap_or_else(|| match generic.kind {
                            GenericKind::Value(_) => Argument::Constant(unsaid()),
                            _ => Argument::Type(Type::Unknown),
                        })
                    })
                    .collect();
                let map: Vec<(Location, &Argument)> =
                    fixed(&public.generics).zip(&explicit).collect();
                let agrees = public
                    .parameters
                    .iter()
                    .filter(|parameter| !parameter.configuration)
                    .zip(&requirement.operands)
                    .all(|(parameter, operand)| {
                        self.admits(&substitute_within(&parameter.ty, &map), operand)
                    })
                    && match (&public.result, &requirement.result) {
                        (Some(declared), Some(result)) => {
                            self.admits(&substitute_within(declared, &map), result)
                        }
                        _ => true,
                    };
                if !agrees {
                    continue;
                }
            }
            let mut supplied = true;
            for (operand, &(_, argument)) in requirement.operands.iter().zip(&ordered) {
                supplied &= if argument.retried {
                    // Read again as the type supplied; what the trial
                    // reports is taken back.
                    self.fits_place(&argument.node, operand, false)
                } else {
                    *operand == argument.value.ty
                };
                if !supplied {
                    break;
                }
            }
            if !supplied {
                continue;
            }
            // GEN-5: "Match candidates against the requested output
            // before inputs". Of two requirements supplied alike, the
            // one whose arrow gives what the call's place expects is
            // the call: under `convert(T) -> i64 && convert(T) -> f64`
            // a place that expects an `f64` takes the second.
            let gives = match (wanted, requirement.result.as_ref().or(asks.as_ref())) {
                (Some(wanted), Some(result)) => self.admits(wanted, result),
                _ => true,
            };
            // What the call gives: what its arrow states, or where it
            // states none what the operator's defaults make its result.
            let required = Required {
                index: requirement.index,
                result: requirement.result.or(asks),
                operands: requirement.operands,
                places: places.clone(),
            };
            if gives {
                return Some(required);
            }
            // With none that gives it, the first supplied stands, and
            // its type is reported where it does not fit.
            first.get_or_insert(required);
        }
        first
    }

    /// The call of an operation the enclosing declaration requires
    /// (GEN-7): of the type the requirement's arrow states, and of no
    /// known type where it states none. No candidate is selected: that
    /// is done where the generic parameters are known. An argument
    /// whose first reading was taken back is read where it stands, as
    /// the type the requirement supplies.
    pub(super) fn call_required(
        &mut self,
        operator: OperatorRef,
        required: Required,
        given: Vec<Given>,
        mut operator_generics: Vec<Argument>,
        (form, at): (Form, Location),
    ) -> (ExprKind, Type) {
        let Required {
            index,
            result,
            operands,
            places,
        } = required;
        let public = match &self.program.modules[operator.module].details[operator.declaration] {
            Detail::Callable(public) => public,
            _ => unreachable!("a requirement names an operator signature"),
        };
        let implicit_configuration =
            operator_generics.is_empty() && has_generic_configuration_default(public);
        if implicit_configuration || !operator_generics.is_empty() {
            // The requirement supplies the ordinary operand types. A
            // brace entry is checked against those types (FN-18), so it
            // cannot redefine the generic type of an ordinary operand.
            let mut inferred = vec![None; public.generics.len()];
            from_place(public, result.as_ref(), &mut inferred);
            let supplied = requirement_inputs(public, &operands);
            if implicit_configuration {
                // A symbol has no written generic arguments. Resolve defaults
                // in the public signature with the requirement's operand types.
                let requested = self.output_requested(operator, public, inferred, &supplied, false);
                if let Some(unfit) = requested.unfit {
                    self.diagnostics.emit(unfit.diagnostic(public, at));
                    return (self.unselected(given, form), Type::Unknown);
                }
                operator_generics = requested.generics;
            } else {
                self.infer_operator_inputs(public, &mut inferred, &supplied);
                for (argument, found) in operator_generics.iter_mut().zip(inferred) {
                    if let Some(found) = found {
                        *argument = found;
                    }
                }
            }
        }
        let map: Vec<_> = fixed(&public.generics).zip(&operator_generics).collect();
        let configuration = places.iter().any(|place| place.operand.is_none()).then(|| {
            Box::new(hir::ConfigurationRead {
                raise: self.raise_here(),
                result: self.result_type().cloned(),
                anonymous: self.anonymous.is_some(),
            })
        });
        let arguments = given
            .into_iter()
            .zip(places)
            .map(|(given, place)| {
                let value = match place.operand {
                    Some(operand) if given.retried => self.argument_value(
                        given,
                        &operands[operand],
                        "the operand its requirement supplies",
                    ),
                    Some(_) => given.value,
                    None => {
                        let configuration = &public.parameters[place.parameter];
                        let ty = substitute_within(&configuration.ty, &map);
                        // Check the public place now. The candidate's
                        // place chooses its own form once it is known.
                        let value = self.argument_read(given, &ty);
                        self.fit(
                            value.clone(),
                            &ty,
                            &format!("configuration parameter `{}`", configuration.name),
                        );
                        value
                    }
                };
                hir::Argument {
                    parameter: Some(place.parameter),
                    pack: None,
                    value,
                }
            })
            .collect();
        let kind = ExprKind::Call {
            callee: Callee::Required {
                operator: (operator.module, operator.declaration),
                requirement: index,
                operator_generics,
                configuration,
            },
            arguments,
            form,
            lifted: false,
        };
        (kind, result.unwrap_or(Type::Unknown))
    }
}

/// GEN-16 names ordinary operand types. Output inference takes a slot
/// for every declared parameter, including FN-18 configuration slots.
fn requirement_inputs<'a>(
    signature: &Signature,
    operands: impl IntoIterator<Item = &'a Type>,
) -> Vec<Option<&'a Type>> {
    let mut operands = operands.into_iter();
    signature
        .parameters
        .iter()
        .map(|parameter| {
            if parameter.configuration {
                None
            } else {
                operands.next()
            }
        })
        .collect()
}

/// A requirement as it is read at a call ([`Typer::requirements_read`]):
/// the operator it names, the types it is supplied with, each with
/// where it is written, and the type its arrow states.
type Read = (OperatorRef, Vec<(SyntaxNode, Type)>, Option<Type>);

/// What answers a requirement: see [`Typer::answered`].
enum Answered {
    /// Nothing: no one value candidate takes the types supplied, or
    /// gives what the arrow states.
    Unmet,
    /// A requirement the enclosing declaration states (GEN-7): its
    /// position among that declaration's requirements.
    Stated(usize),
    /// The one most-specific value candidate that takes the types
    /// supplied (GEN-5).
    Selected {
        /// The position of its module in `Program::modules`.
        module: usize,
        /// Its position in that module's `declarations`.
        declaration: usize,
        /// What each of its generic parameters stands for.
        generics: Vec<Argument>,
    },
}

/// A named operator requirement of a declaration (GEN-16): `add_(T, T)
/// -> T` requires that `add_` is callable with two values of `T`, and
/// that it gives a `T`.
struct Requirement {
    /// Its position among the constraint calls that hold wherever the
    /// declaration's `requires` does ([`required_calls`]).
    index: usize,
    /// The operator named.
    operator: OperatorRef,
    /// The types it is supplied with, in order.
    operands: Vec<Type>,
    /// The result its arrow states, when it has one.
    result: Option<Type>,
}

/// The requirement of the enclosing declaration that a call is
/// (GEN-7): see [`Typer::required`].
pub(super) struct Required {
    /// Its position among the declaration's requirements
    /// ([`Requirement::index`]).
    index: usize,
    /// The result its arrow states, when it has one.
    result: Option<Type>,
    /// The types it is supplied with, in the operator's order.
    operands: Vec<Type>,
    /// Each argument's two positions, in written argument order.
    places: Vec<RequiredPlace>,
}

#[derive(Clone)]
struct RequiredPlace {
    /// Position among the requirement's ordinary operands (GEN-16).
    /// A configuration argument has no operand position (FN-18).
    operand: Option<usize>,
    /// Index in the public signature, including configuration (FN-18).
    parameter: usize,
}

/// The constraint calls of a condition that hold wherever the condition
/// does: the condition itself, and each side of an `&&` (GEN-15).
fn required_calls(node: &SyntaxNode, calls: &mut Vec<SyntaxNode>) {
    match node.kind() {
        SyntaxKind::ConstraintCall => calls.push(node.clone()),
        SyntaxKind::ConstraintParen => {
            if let Some(inner) = node.children().next() {
                required_calls(&inner, calls);
            }
        }
        SyntaxKind::ConstraintBinary if has_token(node, SyntaxKind::AmpAmp) => {
            for side in node.children() {
                required_calls(&side, calls);
            }
        }
        _ => {}
    }
}

/// The relations of a condition that hold wherever the condition does:
/// the condition itself, and each side of an `&&` (GEN-15).
fn conjoined_relations(node: &SyntaxNode, relations: &mut Vec<SyntaxNode>) {
    match node.kind() {
        SyntaxKind::ConstraintRelation => relations.push(node.clone()),
        SyntaxKind::ConstraintParen => {
            if let Some(inner) = node.children().next() {
                conjoined_relations(&inner, relations);
            }
        }
        SyntaxKind::ConstraintBinary if has_token(node, SyntaxKind::AmpAmp) => {
            for side in node.children() {
                conjoined_relations(&side, relations);
            }
        }
        _ => {}
    }
}

/// The set of each requirement `name in {...}` in `node`: the relation
/// itself, the sides of an `&&`, and those in parentheses. Each must
/// hold, so the name is in every one of them.
fn result_sets(node: &SyntaxNode, name: &str, sets: &mut Vec<SyntaxNode>) {
    match node.kind() {
        SyntaxKind::ConstraintRelation => {
            let operands: Vec<SyntaxNode> = node.children().collect();
            let [left, right] = operands.as_slice() else {
                return;
            };
            let named = left.kind() == SyntaxKind::NamedType
                && left.text().to_string().trim() == name
                && right.kind() == SyntaxKind::ConstraintSet
                && tokens(node).any(|token| token.text() == "in");
            if named {
                sets.push(right.clone());
            }
        }
        SyntaxKind::ConstraintBinary if has_token(node, SyntaxKind::AmpAmp) => {
            for side in node.children() {
                result_sets(&side, name, sets);
            }
        }
        SyntaxKind::ConstraintParen => {
            if let Some(inner) = node.children().next() {
                result_sets(&inner, name, sets);
            }
        }
        _ => {}
    }
}

/// A call, in a typed body, of a function or a struct with generic
/// parameters: what it calls, what it gives those parameters, and where
/// it is written.
pub(super) struct Made {
    callee: (usize, usize),
    generics: Vec<Argument>,
    /// What answers each requirement the callee states at this call
    /// (`hir::Answer`), with the call's generic arguments in place.
    answers: Vec<hir::Answer>,
    /// For a call of an operation the enclosing declaration requires
    /// (`Callee::Required`), the requirement's position: what it calls
    /// is the candidate the instance's caller answers it with. `callee`,
    /// `generics` and `answers` say nothing then.
    required: Option<usize>,
    at: Location,
}

/// How many instances one call is followed to ([`crate::INSTANCES`]). A
/// recursion that gives new arguments each time, `grow<N * 2>` and
/// `grow<N * 2 + 1>` in `grow<const N: i64>`, makes instances without
/// end; what is past this many is not told, and is reported where it is
/// called from (`generic.instance_limit`).
use crate::INSTANCES;

/// How long, written out, an instance's generic arguments may grow: `S +
/// S` in `doubled<const S: str>` doubles a string at each instance. One
/// past it is not made, and is reported as [`INSTANCES`] is.
const ARGUMENTS: usize = 1 << 16;

/// How deep a type an instance's generic arguments may hold: `list<T>`
/// in `nest<T>` nests a list in a list at each instance. One past it is
/// not made, and is reported as [`INSTANCES`] is: the instances of a
/// type that grows so never end (GEN-13), and a type that deep is not
/// read further.
const NESTING: usize = 256;

/// How deep `ty` nests one type in another: one for a type that holds
/// none. It is counted with a list of what is still to look at, and not
/// by a function calling itself, so a type of any depth is counted.
fn nesting<'t>(ty: &'t Type) -> usize {
    let mut deepest = 0;
    let mut waiting: Vec<(&'t Type, usize)> = vec![(ty, 1)];
    while let Some((ty, depth)) = waiting.pop() {
        deepest = deepest.max(depth);
        let mut inside = |part: &'t Type| waiting.push((part, depth + 1));
        match ty {
            Type::Named { arguments, .. } => {
                for argument in arguments {
                    if let Argument::Type(argument) = argument {
                        inside(argument);
                    }
                }
            }
            Type::Tuple(elements) => elements.iter().for_each(&mut inside),
            Type::List { element: inner, .. }
            | Type::Set(inner)
            | Type::Endpoint(inner)
            | Type::Atomic(inner)
            | Type::Delta(inner)
            | Type::Trace { cell: inner, .. }
            | Type::Nullable(inner)
            | Type::RollingWindow { value: inner, .. } => inside(inner),
            Type::Dict { key, value } => {
                inside(key);
                inside(value);
            }
            Type::Result { value, error } => {
                inside(value);
                inside(error);
            }
            Type::Function(function) => function.types().for_each(&mut inside),
            Type::Scalar(_)
            | Type::Parameter { .. }
            | Type::Signal
            | Type::Error
            | Type::ErrorContext
            | Type::Unknown => {}
        }
    }
    deepest
}

/// Tells the `requires` of each instance a call makes.
///
/// GEN-3: "A call of a function whose `requires` does not hold with the
/// generic arguments the call resolves is rejected." A call in a
/// generic body that passes on a parameter of its own, `one_only<Y>(v)`
/// in `outer<const Y: f64>`, resolves its arguments where a call gives
/// `Y` a value: `outer<2.0>(5.0)` makes `one_only<2.0>`, whose
/// `requires X == 1.0` does not hold. What is not told where the
/// generic body is read is told here, for each call that gives every
/// argument, through calls at any depth, and is reported at that call.
/// Not told is never taken to hold: one still not told for a constant
/// this compiler does not work out is reported too.
pub(super) fn instances_hold(
    program: &Program,
    mut given: Vec<Made>,
    diagnostics: &mut Diagnostics,
) {
    // The generic parameters of `(module, declaration)`, by where each
    // is declared.
    let own = |(module, declaration): (usize, usize)| -> Vec<Location> {
        match program.modules[module].details.get(declaration) {
            Some(Detail::Callable(signature)) => signature.generics.iter().map(|g| g.at).collect(),
            Some(Detail::Struct(read)) => read.generics.iter().map(|g| g.at).collect(),
            _ => Vec::new(),
        }
    };
    // What each body makes that passes on a parameter of its own, by the
    // declaration it is the body of; and what every body makes that
    // gives every argument.
    let mut passed: HashMap<(usize, usize), Vec<Made>> = HashMap::new();
    // The declarations whose body gives a callee a generic argument it
    // works out from its own generic parameters.
    let mut computing: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();
    let mut configured = std::collections::HashSet::new();
    for (index, module) in program.modules.iter().enumerate() {
        for position in 0..module.declarations.len() {
            let mut made = Vec::new();
            if let Some(Some(body)) = module.bodies.get(position) {
                made_in(&body.statements, &mut made);
            }
            for default in module
                .defaults
                .get(position)
                .into_iter()
                .flatten()
                .flatten()
            {
                made_by(&default.value, &mut made);
            }
            // A struct a type written in it applies, a field's or a
            // parameter's (GEN-3: "an application of a struct whose
            // `requires` does not hold with its arguments").
            let span = module.declarations[position].syntax.span();
            let file = module.files[module.declarations[position].file].file;
            for written in &module.written {
                if written.at.file == file
                    && written.at.span.start >= span.start
                    && written.at.span.end <= span.end
                {
                    applied_in(&written.ty, written.at, &mut made);
                }
            }
            let declared = own((index, position));
            if !declared.is_empty()
                && super::operator_defaults::selects_configuration(program, module, position)
            {
                configured.insert((index, position));
            }
            for call in made {
                // ERR-9: a generic argument the body works out from its
                // own parameters, `times<(M * 2)>`, may fail where an
                // instance gives them: each is followed.
                if call
                    .generics
                    .iter()
                    .any(|argument| !computed(argument, &declared).is_empty())
                {
                    computing.insert((index, position));
                }
                if call.required.is_some() || passes_on(&call.generics, &declared) {
                    passed.entry((index, position)).or_default().push(call);
                } else {
                    given.push(call);
                }
            }
        }
        for body in module
            .test_bodies
            .iter()
            .flatten()
            .chain(&module.store_body)
        {
            made_in(&body.statements, &mut given);
        }
        if let Some(run) = &module.run_body {
            made_by(&run.call, &mut given);
        }
        for constant in module.constants.iter().flatten() {
            made_by(constant, &mut given);
        }
    }
    // The declarations whose calls reach a `requires` with what they
    // are given: an instance of any other makes none to tell.
    // A body that works out a generic argument from its own parameters
    // is followed for the instances it makes, whose evaluation may fail;
    // every body that passes its parameters on, whose instances may never
    // end (GEN-13: "Where the instances of one declaration never end when
    // the program is compiled, the program rejects"); and a body with a
    // call whose candidate each instance selects, which reaches the
    // candidate's.
    let mut reaching: std::collections::HashSet<(usize, usize)> = program
        .modules
        .iter()
        .enumerate()
        .flat_map(|(index, module)| {
            (0..module.declarations.len())
                .filter(|&position| module.selects_per_instance(position))
                .map(move |position| (index, position))
        })
        .chain(passed.keys().copied())
        .chain(computing)
        .chain(configured.iter().copied())
        .collect();
    loop {
        let more: Vec<(usize, usize)> = passed
            .iter()
            .filter(|(declared, _)| !reaching.contains(*declared))
            .filter(|(_, calls)| {
                calls.iter().any(|call| {
                    // What a requirement calls is known where the body's
                    // declaration is called: any candidate may be it.
                    call.required.is_some()
                        || required(program, call.callee).is_some()
                        || reaching.contains(&call.callee)
                })
            })
            .map(|(declared, _)| *declared)
            .collect();
        if more.is_empty() {
            break;
        }
        reaching.extend(more);
    }
    if reaching.is_empty() {
        return;
    }
    let tables: Vec<_> = program.modules.iter().map(super::tables).collect();
    let empty = Signature {
        generics: Vec::new(),
        parameters: Vec::new(),
        result: None,
        reference: false,
    };
    // The instances the calls that give every argument make, each once
    // with every call it is made from: two calls of `wrap<1>` are one
    // instance of it.
    let mut roots: Vec<(Made, Vec<Location>)> = Vec::new();
    let mut root_of: HashMap<String, usize> = HashMap::new();
    for made in given
        .into_iter()
        .filter(|made| reaching.contains(&made.callee))
    {
        let key = instance_key(made.callee, &made.generics, &made.answers);
        match root_of.get(&key) {
            Some(&root) => roots[root].1.push(made.at),
            None => {
                root_of.insert(key, roots.len());
                let at = made.at;
                roots.push((made, vec![at]));
            }
        }
    }
    for (root, from) in roots {
        // Each instance this one makes followed once, by what it is: its
        // callee and its arguments, the root's own among them.
        let mut seen: std::collections::HashSet<String> =
            std::collections::HashSet::from([instance_key(
                root.callee,
                &root.generics,
                &root.answers,
            )]);
        let mut waiting: std::collections::VecDeque<Made> =
            std::collections::VecDeque::from([root]);
        // How many instances of each declaration this one has made.
        let mut followed: HashMap<(usize, usize), usize> = HashMap::new();
        let mut exhausted = false;
        while let Some(made) = waiting.pop_front() {
            let count = followed.entry(made.callee).or_insert(0);
            *count += 1;
            if *count > INSTANCES {
                past_the_limit(&from, made.callee, program, diagnostics);
                break;
            }
            let mut defaults = Vec::new();
            if configured.contains(&made.callee) {
                super::operator_defaults::check_instance(
                    program,
                    made.callee,
                    &made.generics,
                    &made.answers,
                    &mut defaults,
                    diagnostics,
                );
            }
            let inner = passed.get(&made.callee).map_or(&[][..], Vec::as_slice);
            let selecting = program.modules.get(made.callee.0).map_or(&[][..], |unit| {
                unit.selecting
                    .get(made.callee.1)
                    .map_or(&[][..], Vec::as_slice)
            });
            if inner.is_empty() && selecting.is_empty() && defaults.is_empty() {
                continue;
            }
            let declared = own(made.callee);
            let map: Vec<(Location, &Argument)> =
                declared.iter().copied().zip(&made.generics).collect();
            // What each call of the instance calls, with what it gives
            // the callee's generic parameters and what answers the
            // callee's requirements there, and where it is written.
            let mut calls: Vec<Resolved> = defaults
                .into_iter()
                .map(|call| Resolved {
                    callee: call.callee,
                    generics: call.generics,
                    answers: call.answers,
                    at: call.at,
                })
                .collect();
            for call in inner {
                // A call of an operation the body requires is a call of
                // the candidate this instance's caller answers it with
                // (GEN-16); any other has the instance's arguments and
                // answers in place.
                let (callee, generics, answers) = match call.required {
                    Some(position) => match made.answers.get(position) {
                        Some(hir::Answer::Candidate {
                            module,
                            declaration,
                            generics,
                            requirements,
                            ..
                        }) => (
                            (*module, *declaration),
                            generics.clone(),
                            requirements.clone(),
                        ),
                        _ => continue,
                    },
                    None => {
                        let generics: Vec<Argument> = call
                            .generics
                            .iter()
                            .map(|argument| given_in(argument, &map, &declared))
                            .collect();
                        // ERR-9: "Failure during argument evaluation
                        // precedes later admission checks."
                        // One fault is reported once where an instance
                        // is made, though both a type the body writes and
                        // a construction give it (ERR-9: "Do not demand
                        // cascaded diagnostics").
                        // One rejected where the body is declared is not
                        // reported again (ERR-9).
                        let rejected_there = diagnostics
                            .as_slice()
                            .iter()
                            .any(|earlier| earlier.primary == call.at);
                        if let Some(report) = argument_fails(program, call, &map, &declared)
                            .filter(|_| !rejected_there)
                        {
                            for &root in &from {
                                let mut report = report.clone();
                                report.primary = root;
                                let reported = diagnostics.as_slice().iter().any(|earlier| {
                                    earlier.code == report.code
                                        && earlier.primary == report.primary
                                        && earlier.message == report.message
                                });
                                if !reported {
                                    diagnostics.emit(report);
                                }
                            }
                            continue;
                        }
                        (
                            call.callee,
                            generics,
                            call.answers
                                .iter()
                                .map(|answer| answered_in(answer, &map, &declared, &made.answers))
                                .collect(),
                        )
                    }
                };
                calls.push(Resolved {
                    callee,
                    generics,
                    answers,
                    at: call.at,
                });
            }
            // GEN-5: a call whose candidate a generic parameter of the
            // body selects is selected here, with this instance's types
            // ([`Typer::select_at_instance`]).
            if !selecting.is_empty() {
                let (module, declaration) = made.callee;
                let unit = &program.modules[module];
                // GEN-13: the body is read as the instance has it, each
                // generic value parameter of its type there.
                let signature = match unit.details.get(declaration) {
                    Some(Detail::Callable(signature)) => {
                        super::candidates::in_instance(signature, &map)
                    }
                    _ => empty.clone(),
                };
                let (bindings, written, arguments) = &tables[module];
                let sink = std::cell::RefCell::new(Vec::new());
                let mut typer = super::Typer::bare(
                    program,
                    unit,
                    &unit.declarations[declaration],
                    &signature,
                    super::Tables {
                        bindings,
                        written,
                        calls: arguments,
                        selecting: &sink,
                    },
                    diagnostics,
                );
                typer.instance = declared
                    .iter()
                    .copied()
                    .zip(made.generics.iter().cloned())
                    .collect();
                let body_locals = unit
                    .bodies
                    .get(declaration)
                    .and_then(Option::as_ref)
                    .map_or(&[][..], |body| body.locals.as_slice());
                let mut current_locals: Option<&[hir::Local]> = None;
                // The last put at each place: the reading of the call
                // that stands.
                let standing = selecting
                    .iter()
                    .enumerate()
                    .filter(|(index, call)| {
                        !selecting[index + 1..]
                            .iter()
                            .any(|later| later.at == call.at)
                    })
                    .map(|(_, call)| call);
                for call in standing {
                    // A deferred anonymous call is read under its own
                    // lexical locals, with the same enclosing generics.
                    let locals = call.scope.as_deref().unwrap_or(body_locals);
                    if !current_locals.is_some_and(|current| std::ptr::eq(current, locals)) {
                        typer.locals = locals
                            .iter()
                            .map(|local| hir::Local {
                                ty: placed(substitute_within(&local.ty, &map), &declared),
                                ..local.clone()
                            })
                            .collect();
                        typer.assigned = vec![true; locals.len()];
                        current_locals = Some(locals);
                    }
                    let name = &program.modules[call.operator.module].declarations
                        [call.operator.declaration]
                        .name;
                    // What is rejected of the call in this instance is
                    // reported at each call that makes the instance, with
                    // the call in the body, and where it points there,
                    // beside it.
                    let rejected =
                        |typer: &mut super::Typer, reports: Vec<hgl_diagnostics::Diagnostic>| {
                            for report in reports {
                                for &root in &from {
                                    let mut report = report.clone();
                                    let within = report.primary;
                                    report.primary = root;
                                    let mut report = report.with_related(
                                        call.name,
                                        format!(
                                            "`{name}` is called here, with the generic arguments \
                                         this call gives the function it calls"
                                        ),
                                    );
                                    if within != call.name {
                                        report = report.with_related(within, "here, in that call");
                                    }
                                    // A call rejected for a `requires` makes
                                    // no instance to judge further (ERR-9:
                                    // "Do not demand cascaded diagnostics").
                                    if !typer.unmet_reported(root)
                                        && !typer.diagnostics.as_slice().contains(&report)
                                    {
                                        typer.diagnostics.emit(report);
                                    }
                                }
                            }
                        };
                    // ERR-9: "Failure during argument evaluation precedes
                    // later admission checks."
                    if let Some(report) = typer.option_fails(call) {
                        rejected(&mut typer, vec![report]);
                        continue;
                    }
                    match typer.select_at_instance(call, None) {
                        AtInstance::Selected(chosen, given) => {
                            let (module, declaration, generics) =
                                (chosen.module, chosen.declaration, chosen.generics.clone());
                            // What the call is held to once its candidate
                            // is selected (`Typer::called`): a `var`
                            // parameter's place among them (FN-15).
                            let mark = typer.diagnostics.len();
                            typer.called(
                                call.operator,
                                *chosen,
                                given,
                                Vec::new(),
                                (call.form, call.name),
                            );
                            let reports = typer.diagnostics.as_slice()[mark..].to_vec();
                            typer.diagnostics.truncate(mark);
                            let fails = !reports.is_empty();
                            rejected(&mut typer, reports);
                            if fails {
                                continue;
                            }
                            let answers = typer.answers(module, declaration, &generics);
                            calls.push(Resolved {
                                callee: (module, declaration),
                                generics,
                                answers,
                                at: call.name,
                            });
                        }
                        // GEN-5: "No match rejects; tied most-specific
                        // candidates are ambiguous", at each call that
                        // makes the instance.
                        AtInstance::Rejected(report) => rejected(&mut typer, vec![report]),
                        AtInstance::Untold => {}
                    }
                }
            }
            for Resolved {
                callee,
                generics,
                answers,
                at,
            } in calls
            {
                let deep = generics.iter().any(
                    |argument| matches!(argument, Argument::Type(ty) if nesting(ty) > NESTING),
                );
                if deep {
                    exhausted = true;
                    break;
                }
                let key = instance_key(callee, &generics, &answers);
                if key.len() > ARGUMENTS {
                    exhausted = true;
                    break;
                }
                // GEN-7: a call of a function or a candidate in a generic
                // body was judged where the body is declared, under what
                // it states. A struct applied there is told here.
                let applied = program.modules[callee.0].declarations[callee.1].kind
                    == DeclarationKind::Struct;
                if applied && required(program, callee).is_some() {
                    let (module, declaration) = callee;
                    let unit = &program.modules[module];
                    let (bindings, written, arguments) = &tables[module];
                    let sink = std::cell::RefCell::new(Vec::new());
                    let mut typer = super::Typer::bare(
                        program,
                        unit,
                        &unit.declarations[declaration],
                        &empty,
                        super::Tables {
                            bindings,
                            written,
                            calls: arguments,
                            selecting: &sink,
                        },
                        diagnostics,
                    );
                    for &root in &from {
                        typer.instance_met((root, at), callee, &generics);
                    }
                }
                if reaching.contains(&callee) && seen.insert(key) {
                    waiting.push_back(Made {
                        callee,
                        generics,
                        answers,
                        required: None,
                        at: made.at,
                    });
                }
            }
            if exhausted {
                past_the_limit(&from, made.callee, program, diagnostics);
                break;
            }
        }
    }
}

/// What an instance is, written out: its declaration, its generic
/// arguments and what answers its requirements. A constant argument is
/// its value however it is written, so `turn<0>` and the `turn<((N + 1)
/// % 2)>` that comes back to it are one instance.
fn instance_key(callee: (usize, usize), generics: &[Argument], answers: &[hir::Answer]) -> String {
    let generics: Vec<String> = generics
        .iter()
        .map(|argument| match argument {
            Argument::Constant(constant) => format!("{:?}", constant.value),
            Argument::Type(ty) => format!("{:?}", by_value(ty)),
        })
        .collect();
    format!("{callee:?}{generics:?}{answers:?}")
}

/// `ty` with each constant in it, a size or a constant argument at any
/// depth, held by its value alone, as it is written put aside:
/// `list<i64, ((N + 1) % 2)>` at `N = 1` is `list<i64, 0>`. What an
/// instance is ([`instance_key`]) is so its arguments' values.
fn by_value(ty: &Type) -> Type {
    let constant = |constant: &Constant| Constant {
        text: String::new(),
        value: constant.value.clone(),
    };
    let part = |inner: &Type| Box::new(by_value(inner));
    match ty {
        Type::Named {
            name,
            module,
            declaration,
            arguments,
            kind,
        } => Type::Named {
            name: name.clone(),
            module: *module,
            declaration: *declaration,
            arguments: arguments
                .iter()
                .map(|argument| match argument {
                    Argument::Type(ty) => Argument::Type(by_value(ty)),
                    Argument::Constant(given) => Argument::Constant(constant(given)),
                })
                .collect(),
            kind: *kind,
        },
        Type::Tuple(elements) => Type::Tuple(elements.iter().map(by_value).collect()),
        Type::List { element, size } => Type::List {
            element: part(element),
            size: size.as_ref().map(constant),
        },
        Type::Set(inner) => Type::Set(part(inner)),
        Type::Endpoint(inner) => Type::Endpoint(part(inner)),
        Type::Atomic(inner) => Type::Atomic(part(inner)),
        Type::Delta(inner) => Type::Delta(part(inner)),
        Type::Nullable(inner) => Type::Nullable(part(inner)),
        Type::Trace { cell, style } => Type::Trace {
            cell: part(cell),
            style: *style,
        },
        Type::Dict { key, value } => Type::Dict {
            key: part(key),
            value: part(value),
        },
        Type::RollingWindow {
            value,
            maximum,
            minimum,
        } => Type::RollingWindow {
            value: part(value),
            maximum: constant(maximum),
            minimum: minimum.as_ref().map(constant),
        },
        Type::Result { value, error } => Type::Result {
            value: part(value),
            error: part(error),
        },
        other => other.clone(),
    }
}

/// A call an instance makes: what it calls, with what it gives the
/// callee's generic parameters and what answers the callee's
/// requirements, and where it is written ([`instances_hold`]).
struct Resolved {
    callee: (usize, usize),
    generics: Vec<Argument>,
    answers: Vec<hir::Answer>,
    at: Location,
}

/// The constants in `argument`, a generic argument a call in a generic
/// body gives, that the body works out from its own generic parameters,
/// declared at `declared`: `M * 2`, `-M`, `[M, 1]`, and one in a type,
/// `list<i64, M + 1>`. A parameter passed on as it is, `M`, is none.
fn computed(argument: &Argument, declared: &[Location]) -> Vec<Constant> {
    let worked = |constant: &Constant| {
        constant.value.names_one_of(declared)
            && !matches!(constant.value, ConstantValue::Parameter { .. })
    };
    match argument {
        Argument::Constant(constant) if worked(constant) => vec![constant.clone()],
        Argument::Constant(_) => Vec::new(),
        Argument::Type(ty) => {
            // `any_part` takes a closure that cannot change what it
            // captures: the constants found are kept in a `RefCell`.
            let found = std::cell::RefCell::new(Vec::new());
            ty.any_part(&|_| false, &|constant| {
                if worked(constant) {
                    found.borrow_mut().push(constant.clone());
                }
                false
            });
            found.into_inner()
        }
    }
}

/// The rejection of `call`, a call in a generic body, where a generic
/// argument it works out from the body's parameters, with what an
/// instance gives them, `map`, has no value: its evaluation fails, or
/// this compiler does not work it out. Its primary place is the call in
/// the body; the caller puts it where the instance is made.
///
/// ERR-9: "Required constant failures reject where specified." GEN-8:
/// "The value a call gives a generic value parameter is a compile-time
/// constant ... Any other rejects": the same argument written in place,
/// `times<(4 * 4611686018427387904)>`, is `generic.argument_constant`.
fn argument_fails(
    program: &Program,
    call: &Made,
    map: &[(Location, &Argument)],
    declared: &[Location],
) -> Option<hgl_diagnostics::Diagnostic> {
    let given = |at: Location| {
        map.iter()
            .find(|(own, _)| *own == at)
            .and_then(|(_, argument)| match argument {
                Argument::Constant(constant) => Some(constant.value.clone()),
                Argument::Type(_) => None,
            })
    };
    let (module, declaration) = call.callee;
    let unit = program.modules.get(module)?;
    let parameters: &[GenericParameter] = match unit.details.get(declaration)? {
        Detail::Callable(signature) => &signature.generics,
        Detail::Struct(read) => &read.generics,
        _ => return None,
    };
    let callee = &unit.declarations[declaration].name;
    for (index, argument) in call.generics.iter().enumerate() {
        for constant in computed(argument, declared) {
            if crate::constant::worked_out(&constant.value.substituted(&given)) {
                continue;
            }
            let parameter = parameters.get(index).map_or_else(
                || "a generic parameter".to_string(),
                |generic| format!("`{}`", generic.name),
            );
            let why = if crate::constant::fails_when_given(&constant.value, &given) {
                "fails when it is evaluated for this call: an operation in it is out of its \
                 type's range, or divides by zero"
            } else {
                "is no compile-time constant for this call: this compiler does not work out its \
                 value"
            };
            let what = match argument {
                Argument::Constant(_) => format!(
                    "{parameter} of `{callee}` is a generic value parameter, and what it is \
                     given, `{}`, {why}",
                    constant.text
                ),
                Argument::Type(ty) => format!(
                    "{parameter} of `{callee}` is given `{ty}`, and `{}` in it {why}",
                    constant.text
                ),
            };
            return Some(
                error(
                    Category::Type,
                    codes::GENERIC_ARGUMENT_CONSTANT,
                    call.at,
                    what,
                    "GEN-8",
                )
                .with_related(
                    call.at,
                    format!(
                        "`{callee}` is called here, with the generic arguments this call gives \
                         the function it calls"
                    ),
                ),
            );
        }
    }
    None
}

/// Reports each call in `from` whose instances go past what is followed
/// ([`INSTANCES`], [`ARGUMENTS`]): what is past it is not told, and is
/// not taken to hold (GEN-3). `callee` is the declaration whose instance
/// was being followed there. One already rejected for a `requires` is
/// not reported again (ERR-9: "Do not demand cascaded diagnostics").
fn past_the_limit(
    from: &[Location],
    (module, declaration): (usize, usize),
    program: &Program,
    diagnostics: &mut Diagnostics,
) {
    let name = &program.modules[module].declarations[declaration].name;
    for &at in from {
        let rejected = diagnostics.as_slice().iter().any(|earlier| {
            earlier.code == Code::Catalogued(codes::REQUIRES_UNMET)
                && earlier.primary.file == at.file
                && earlier.primary.span.start == at.span.start
        });
        if !rejected {
            // GEN-13 catalogues the code, proposed, for instances past
            // the implementation's bound.
            diagnostics.emit(crate::error(
                Category::Type,
                codes::INSTANCE_LIMIT,
                at,
                format!(
                    "the instances of `{name}` this call makes do not end within the \
                     {INSTANCES} this compiler follows, with generic arguments no longer than \
                     {ARGUMENTS} characters written out and types in them no deeper than \
                     {NESTING}: the `requires` and the generic arguments of those past them are \
                     not told"
                ),
                "GEN-13",
            ));
        }
    }
}

/// The struct applications `ty`, written at `at`, makes, at any depth,
/// each that gives generic arguments.
fn applied_in(ty: &Type, at: Location, made: &mut Vec<Made>) {
    let mut found = Vec::new();
    struct_applications(ty, &mut found);
    for (callee, generics) in found {
        if !generics.is_empty() {
            made.push(Made {
                callee,
                generics,
                answers: Vec::new(),
                required: None,
                at,
            });
        }
    }
}

/// True where a generic argument names one of `declared`, the generic
/// parameters of the declaration the call is written in.
fn passes_on(generics: &[Argument], declared: &[Location]) -> bool {
    !declared.is_empty()
        && generics.iter().any(|argument| match argument {
            Argument::Constant(constant) => constant.value.names_one_of(declared),
            Argument::Type(ty) => ty.any_part(
                &|part| matches!(part, Type::Parameter { at, .. } if declared.contains(at)),
                &|constant| constant.value.names_one_of(declared),
            ),
        })
}

/// An inferred pack can pass on a type of the enclosing generic body too.
fn packs_pass_on(packs: &[hir::TypePackArgument], declared: &[Location]) -> bool {
    packs
        .iter()
        .flat_map(|pack| &pack.types)
        .any(|ty| mentions(ty, declared))
}

/// `argument`, a generic argument a call in a generic body gives, with
/// what a call of that body gives its parameters, `map`, in place.
fn given_in(argument: &Argument, map: &[(Location, &Argument)], declared: &[Location]) -> Argument {
    match argument {
        Argument::Constant(constant) => Argument::Constant(Constant {
            text: constant.text.clone(),
            value: constant.value.substituted(&|at| {
                map.iter()
                    .find(|(own, _)| *own == at)
                    .and_then(|(_, given)| match given {
                        Argument::Constant(given) => Some(given.value.clone()),
                        Argument::Type(_) => None,
                    })
            }),
        }),
        Argument::Type(ty) => Argument::Type(placed(substitute_within(ty, map), declared)),
    }
}

/// The calls of generic declarations `statements` make, at any depth.
fn made_in(statements: &[hir::Stmt], made: &mut Vec<Made>) {
    hir::walk_statements(statements, &mut |expression| made_at(expression, made));
}

/// The calls of generic declarations `expression` makes, at any depth.
fn made_by(expression: &Expr, made: &mut Vec<Made>) {
    expression.walk(&mut |expression| made_at(expression, made));
}

/// The calls an omitted, specialized default makes with resolved arguments
/// and evidence. Follow these as ordinary instances, including helper bodies
/// that can themselves omit an inherited default (GEN-16, UNIT-8).
pub(super) fn concrete_default_calls(expression: &Expr, made: &mut Vec<Made>) {
    fn concrete(answer: &hir::Answer) -> bool {
        match answer {
            hir::Answer::Candidate {
                generics,
                requirements,
                ..
            } => {
                generics.iter().all(resolved_default_argument) && requirements.iter().all(concrete)
            }
            hir::Answer::Stated(_) | hir::Answer::Unread => false,
        }
    }
    let mut calls = Vec::new();
    made_by(expression, &mut calls);
    made.extend(calls.into_iter().filter(|call| {
        call.required.is_none()
            && call.generics.iter().all(resolved_default_argument)
            && call.answers.iter().all(concrete)
    }));
}

/// The call `expression` is, where it is one of a function, an
/// operator's candidate or a struct it gives generic arguments.
fn made_at(expression: &Expr, made: &mut Vec<Made>) {
    let (callee, generics, answers) = match &expression.kind {
        // A function, or the candidate a call of an operator selected
        // (GEN-5), whose body may pass its parameters on as any does.
        ExprKind::Call {
            callee:
                Callee::Function {
                    module,
                    declaration,
                    generics,
                    requirements,
                    ..
                }
                | Callee::Candidate {
                    module,
                    declaration,
                    generics,
                    requirements,
                    ..
                },
            ..
        } => (
            (*module, *declaration),
            generics.clone(),
            requirements.clone(),
        ),
        // GEN-16: an operation the enclosing declaration requires, whose
        // candidate a call of that declaration answers.
        ExprKind::Call {
            callee: Callee::Required { requirement, .. },
            ..
        } => {
            made.push(Made {
                callee: (usize::MAX, usize::MAX),
                generics: Vec::new(),
                answers: Vec::new(),
                required: Some(*requirement),
                at: expression.at,
            });
            return;
        }
        ExprKind::Construct {
            module,
            declaration,
            ..
        } => match &expression.ty {
            Type::Named { arguments, .. } => {
                ((*module, *declaration), arguments.clone(), Vec::new())
            }
            _ => return,
        },
        _ => return,
    };
    if !generics.is_empty() || !answers.is_empty() {
        made.push(Made {
            callee,
            generics,
            answers,
            required: None,
            at: expression.at,
        });
    }
}

/// `answer`, recorded at a call in a generic body, for an instance of
/// that body: the body's generic arguments, `map`, in place, and what
/// the instance's caller answers the body's own requirements with,
/// `answers`, in place of each it states (`Answer::Stated`).
fn answered_in(
    answer: &hir::Answer,
    map: &[(Location, &Argument)],
    declared: &[Location],
    answers: &[hir::Answer],
) -> hir::Answer {
    match answer {
        hir::Answer::Candidate {
            operator,
            module,
            declaration,
            generics,
            requirements,
        } => hir::Answer::Candidate {
            operator: *operator,
            module: *module,
            declaration: *declaration,
            generics: generics
                .iter()
                .map(|argument| given_in(argument, map, declared))
                .collect(),
            requirements: requirements
                .iter()
                .map(|answer| answered_in(answer, map, declared, answers))
                .collect(),
        },
        hir::Answer::Stated(position) => answers
            .get(*position)
            .cloned()
            .unwrap_or(hir::Answer::Unread),
        hir::Answer::Unread => hir::Answer::Unread,
    }
}

/// What a candidate's condition is checked against at a call.
pub(super) struct Condition<'c> {
    /// The value an option has at the call: what the call gives it, or
    /// its default. `None` when it is no constant this pass reads.
    pub(super) value_of: &'c dyn Fn(&str) -> Option<ConstantValue>,
    /// What a constant the condition compares the option named with
    /// comes to, as a value of the option's type (`crate::constant`).
    /// `None` when it is no constant this pass reads.
    pub(super) constant: &'c dyn Fn(&SyntaxNode, &str) -> Option<ConstantValue>,
    /// What a generic parameter of the candidate stands for at the
    /// call. `None` for a name that is no generic parameter of it.
    pub(super) type_of: &'c dyn Fn(&str) -> Option<Type>,
    /// What a type written in the condition means, the candidate's
    /// generic parameters in place. `None` for what is no type, or is
    /// not known.
    pub(super) written: &'c dyn Fn(&SyntaxNode) -> Option<Type>,
    /// Whether a named operator requirement written in the condition
    /// holds at the call (GEN-16). `None` when it cannot be told.
    pub(super) callable: &'c dyn Fn(&SyntaxNode) -> Option<bool>,
    /// Set where a comparison is not told because a value it compares
    /// is one this compiler does not work out: a constant held by its
    /// name, or one not worked out. Such a constant is not taken as
    /// known, so a `requires` not told for it is not taken to hold.
    pub(super) unvalued: &'c std::cell::Cell<bool>,
}

/// A concrete application names no type or value parameter and uses
/// only known constant values, including those nested inside its types.
fn resolved_default_argument(argument: &Argument) -> bool {
    fn known(value: &ConstantValue) -> bool {
        match value {
            ConstantValue::Member(member) => member.of.is_some(),
            ConstantValue::List(values) | ConstantValue::Tuple(values) => values.iter().all(known),
            ConstantValue::Parameter { .. }
            | ConstantValue::Named { .. }
            | ConstantValue::Constructed { .. }
            | ConstantValue::Unary(..)
            | ConstantValue::Binary(..)
            | ConstantValue::Unevaluated
            | ConstantValue::Unknown => false,
            _ => true,
        }
    }
    match argument {
        Argument::Type(ty) => !ty.any_part(
            &|part| matches!(part, Type::Parameter { .. } | Type::Unknown),
            &|constant| !known(&constant.value),
        ),
        Argument::Constant(constant) => known(&constant.value),
    }
}

/// True where `value` is, or holds, a constant this compiler does not
/// work out: one held by its name (`ConstantValue::Named`), or one not
/// worked out. A generic value parameter is none: it is given its value
/// where the call that gives it is read.
fn unworked(value: &ConstantValue) -> bool {
    match value {
        ConstantValue::Named { .. } | ConstantValue::Unevaluated => true,
        ConstantValue::Unary(_, operand) | ConstantValue::Constructed { from: operand, .. } => {
            unworked(operand)
        }
        ConstantValue::Binary(_, left, right) => unworked(left) || unworked(right),
        ConstantValue::List(values) | ConstantValue::Tuple(values) => values.iter().any(unworked),
        _ => false,
    }
}

/// [`ConstantValue::equals`] of two values a condition compares, noting
/// in `of` where it is not told for a value not worked out.
fn compared(left: &ConstantValue, right: &ConstantValue, of: &Condition) -> Option<bool> {
    let told = left.equals(right);
    if told.is_none() && (unworked(left) || unworked(right)) {
        of.unvalued.set(true);
    }
    told
}

/// [`same_type`] of two types a condition compares, noting in `of` where
/// it is not told for a constant in either that is not worked out:
/// `FBox<C>` beside `FBox<2.0>`, `C` computed by a call.
fn typed_alike(left: &Type, right: &Type, of: &Condition) -> Option<bool> {
    let told = same_type(left, right);
    let unworked_in = |ty: &Type| ty.any_part(&|_| false, &|constant| unworked(&constant.value));
    if told.is_none() && (unworked_in(left) || unworked_in(right)) {
        of.unvalued.set(true);
    }
    told
}

/// The generic parameters of a function or a struct that states a
/// `requires`, and the condition it states. `None` for one that states
/// none.
fn required(
    program: &Program,
    (module, declaration): (usize, usize),
) -> Option<(&[GenericParameter], SyntaxNode)> {
    let declared_in = &program.modules[module];
    let parameters: &[GenericParameter] = match declared_in.details.get(declaration)? {
        Detail::Callable(signature) => &signature.generics,
        Detail::Struct(read) => &read.generics,
        _ => return None,
    };
    let declared = &declared_in.declarations[declaration];
    let clause = child(&declared.syntax, SyntaxKind::RequiresClause)?;
    Some((parameters, clause.children().next()?))
}

/// What the place a `requires` is tested at says of it, beyond the
/// generic arguments ([`requires_holds`]).
struct Told<'t> {
    /// What a constant written in the condition comes to as a value of
    /// the type given (`crate::constant`), read where the declaration
    /// is written. `None` where it is no constant read so.
    constant: &'t dyn Fn(&SyntaxNode, Option<&Type>) -> Option<ConstantValue>,
    /// Whether a named operator requirement holds (GEN-16). `None`
    /// when it cannot be told.
    callable: &'t dyn Fn(&SyntaxNode) -> Option<bool>,
    /// See [`Condition::unvalued`].
    unvalued: &'t std::cell::Cell<bool>,
    /// The types each pack parameter takes at the call (GEN-10).
    type_packs: &'t [hir::TypePackArgument],
}

/// Whether `condition`, the `requires` of `declaration` of `module`,
/// holds with `generics` for the declaration's generic parameters
/// (GEN-3). `None` when it cannot be told: a generic argument is not
/// known, or names a generic parameter of the declaration it is
/// written in, which is no one type.
fn requires_holds(
    program: &Program,
    (module, declaration): (usize, usize),
    generics: &[Argument],
    condition: &SyntaxNode,
    place: &Told,
) -> Option<bool> {
    let (parameters, _) = required(program, (module, declaration))?;
    let declared_in = &program.modules[module];
    let declared = &declared_in.declarations[declaration];
    // The generic value parameter of the declaration named `name`, by
    // its position.
    let option = |name: &str| {
        parameters.iter().position(|generic| {
            matches!(generic.kind, GenericKind::Value(_)) && generic.name == name
        })
    };
    // GEN-3: the value it has is the constant the generic arguments give
    // it, a default included.
    let value_of = |name: &str| -> Option<ConstantValue> {
        match generics.get(option(name)?)? {
            Argument::Constant(constant) => Some(constant.value.clone()),
            Argument::Type(_) => None,
        }
    };
    let own: Vec<Location> = parameters.iter().map(|generic| generic.at).collect();
    let map: Vec<(Location, &Argument)> = own.iter().copied().zip(generics).collect();
    let told = |ty: Type| (whole(&ty) && !names_parameter(&ty)).then_some(ty);
    let type_of = |name: &str| -> Option<Type> {
        let index = parameters.iter().position(|generic| generic.name == name)?;
        match generics.get(index)? {
            Argument::Type(ty) => told(ty.clone()),
            Argument::Constant(_) => None,
        }
    };
    let file = declared_in.files[declared.file].file;
    let pack_types = |view: &SyntaxToken, name: &str| -> Option<&[Type]> {
        // `types` here is the language's pack view. A resolved name
        // belongs to a different declaration.
        if declared_in
            .uses
            .iter()
            .any(|used| used.at == Location::new(file, view.span()))
        {
            return None;
        }
        let parameter = parameters.iter().position(|generic| generic.name == name)?;
        place
            .type_packs
            .iter()
            .find(|pack| pack.parameter == parameter)
            .map(|pack| pack.types.as_slice())
    };
    let written = |node: &SyntaxNode| -> Option<Type> {
        let start = node.span().start;
        let read = declared_in
            .written
            .iter()
            .find(|written| written.at.file == file && written.at.span.start == start)?;
        told(placed(substitute_within(&read.ty, &map), &own))
    };
    // A constant the condition compares an option with is a value of
    // the option's type (EXPR-5), its generic parameters in place.
    let constant = |node: &SyntaxNode, name: &str| -> Option<ConstantValue> {
        let ty = match &parameters[option(name)?].kind {
            GenericKind::Value(ty) => placed(substitute_within(ty, &map), &own),
            _ => return None,
        };
        (place.constant)(node, Some(&ty))
    };
    holds_with_packs(
        condition,
        &Condition {
            value_of: &value_of,
            constant: &constant,
            type_of: &type_of,
            written: &written,
            callable: place.callable,
            unvalued: place.unvalued,
        },
        &pack_types,
    )
}

/// The rejection of a call of a function, or an application of a
/// struct, at `at`, whose `requires`, `condition`, does not hold
/// (GEN-3).
fn unmet(
    program: &Program,
    (module, declaration): (usize, usize),
    at: Location,
    condition: &SyntaxNode,
) -> hgl_diagnostics::Diagnostic {
    let declared_in = &program.modules[module];
    let declared = &declared_in.declarations[declaration];
    let what = match declared.kind {
        DeclarationKind::Struct => "application",
        _ => "call",
    };
    error(
        Category::Constraint,
        codes::REQUIRES_UNMET,
        at,
        format!(
            "the `requires` of `{}` does not hold for this {what}",
            declared.name
        ),
        "GEN-3",
    )
    .with_related(
        Location::new(declared_in.files[declared.file].file, condition.span()),
        "the condition that does not hold",
    )
}

/// The rejection of a call of a function, or an application of a
/// struct, at `at`, whose `requires`, `condition`, is not told for a
/// constant this compiler does not work out: such a constant is not
/// taken as known (UNIT-13), so the `requires` is not taken to hold
/// (GEN-3).
fn untold(
    program: &Program,
    (module, declaration): (usize, usize),
    at: Location,
    condition: &SyntaxNode,
) -> hgl_diagnostics::Diagnostic {
    untold_with_reason(
        program,
        (module, declaration),
        at,
        condition,
        "this compiler does not work out a constant it compares",
    )
}

/// A condition that cannot be judged has the diagnostic of an unmet
/// requirement and says why this compiler cannot tell whether it holds.
fn untold_with_reason(
    program: &Program,
    (module, declaration): (usize, usize),
    at: Location,
    condition: &SyntaxNode,
    reason: &str,
) -> hgl_diagnostics::Diagnostic {
    let declared_in = &program.modules[module];
    let declared = &declared_in.declarations[declaration];
    let what = match declared.kind {
        DeclarationKind::Struct => "application",
        _ => "call",
    };
    error(
        Category::Constraint,
        codes::REQUIRES_UNMET,
        at,
        format!(
            "the `requires` of `{}` is not known to hold for this {what}: {reason}",
            declared.name
        ),
        "GEN-3",
    )
    .with_related(
        Location::new(declared_in.files[declared.file].file, condition.span()),
        "the condition that is not told",
    )
}

/// Every struct a type applies, at any depth, with the generic
/// arguments it gives it: none for a struct that takes none.
fn struct_applications(ty: &Type, found: &mut Vec<((usize, usize), Vec<Argument>)>) {
    match ty {
        Type::Named {
            module,
            declaration,
            arguments,
            kind,
            ..
        } => {
            if *kind == NamedKind::Struct {
                found.push(((*module, *declaration), arguments.clone()));
            }
            for argument in arguments {
                if let Argument::Type(argument) = argument {
                    struct_applications(argument, found);
                }
            }
        }
        Type::Tuple(elements) => elements
            .iter()
            .for_each(|element| struct_applications(element, found)),
        Type::List { element, .. } => struct_applications(element, found),
        Type::Set(inner)
        | Type::Endpoint(inner)
        | Type::Atomic(inner)
        | Type::Delta(inner)
        | Type::Trace { cell: inner, .. }
        | Type::Nullable(inner) => struct_applications(inner, found),
        Type::Dict { key, value } => {
            struct_applications(key, found);
            struct_applications(value, found);
        }
        Type::RollingWindow { value, .. } => struct_applications(value, found),
        Type::Result { value, error } => {
            struct_applications(value, found);
            struct_applications(error, found);
        }
        Type::Function(function) => function
            .types()
            .for_each(|ty| struct_applications(ty, found)),
        Type::Scalar(_)
        | Type::Parameter { .. }
        | Type::Signal
        | Type::Error
        | Type::ErrorContext
        | Type::Unknown => {}
    }
}

/// Whether a candidate's condition holds at a call. `None` when it
/// cannot be told: the condition is of a form this pass does not read,
/// an option's value is no constant, or a type in it is not known.
///
/// GEN-6: a candidate compares "the option with constants: `requires
/// zero == .error`, `requires zero in {.ieee, .nan}`". GEN-3: a
/// condition "may use type/category membership, equalities": of a
/// generic parameter, `T == i64` or `T in {i64, f64}`. GEN-15:
/// "negation binds before conjunction, which binds before disjunction".
pub(super) fn holds(node: &SyntaxNode, of: &Condition) -> Option<bool> {
    holds_with_packs(node, of, &|_, _| None)
}

/// The verdict for a condition with any resolved type packs in scope.
fn holds_with_packs<'p>(
    node: &SyntaxNode,
    of: &Condition,
    pack_types: &dyn Fn(&SyntaxToken, &str) -> Option<&'p [Type]>,
) -> Option<bool> {
    match node.kind() {
        SyntaxKind::ConstraintParen => holds_with_packs(&node.children().next()?, of, pack_types),
        SyntaxKind::ConstraintNot => {
            holds_with_packs(&node.children().next()?, of, pack_types).map(|held| !held)
        }
        SyntaxKind::ConstraintEach => each_holds(node, of, pack_types),
        // GEN-16: a named operator requirement.
        SyntaxKind::ConstraintCall => (of.callable)(node),
        SyntaxKind::ConstraintBinary => {
            let mut sides = node.children();
            let left = holds_with_packs(&sides.next()?, of, pack_types);
            let right = holds_with_packs(&sides.next()?, of, pack_types);
            let both = has_token(node, SyntaxKind::AmpAmp);
            match (left, right) {
                (Some(left), Some(right)) if both => Some(left && right),
                (Some(left), Some(right)) => Some(left || right),
                // One side settles it whatever the other is.
                (Some(false), _) | (_, Some(false)) if both => Some(false),
                (Some(true), _) | (_, Some(true)) if !both => Some(true),
                _ => None,
            }
        }
        SyntaxKind::ConstraintRelation => {
            let operands: Vec<SyntaxNode> = node.children().collect();
            let [left, right] = operands.as_slice() else {
                return None;
            };
            // The option is the lone name; the read pass has checked
            // that it names one.
            let option = |operand: &SyntaxNode| {
                (operand.kind() == SyntaxKind::NamedType)
                    .then(|| operand.text().to_string().trim().to_string())
            };
            let within = tokens(node).any(|token| token.text() == "in");
            // A generic parameter of the candidate: the condition is of
            // the type it stands for. `T == i64`, `T in {i64, f64}`.
            // Each is told as a constant's equality is, in three answers
            // ([`same_type`]): `FBox<X>` is not known to be `FBox<2.0>`
            // while `X` is not given.
            if let Some(ty) = option(left).and_then(|name| (of.type_of)(&name)) {
                return if has_token(node, SyntaxKind::EqEq) {
                    typed_alike(&(of.written)(right)?, &ty, of)
                } else if within && right.kind() == SyntaxKind::ConstraintSet {
                    let mut found = Some(false);
                    for member in right.children() {
                        match typed_alike(&(of.written)(&member)?, &ty, of) {
                            Some(true) => return Some(true),
                            Some(false) => {}
                            None => found = None,
                        }
                    }
                    found
                } else {
                    None
                };
            }
            // What an operand that is a constant comes to as a value of
            // the option `name`'s type: the value another option has at
            // the call where it names one (GEN-3: "equalities, the
            // membership of a constant in a set of constants", of two
            // options too: `N == M`, `N in {M, 5}`), and otherwise the
            // constant it is.
            let operand = |operand: &SyntaxNode, name: &str| -> Option<ConstantValue> {
                option(operand)
                    .and_then(|other| (of.value_of)(&other))
                    .or_else(|| (of.constant)(operand, name))
            };
            if has_token(node, SyntaxKind::EqEq) {
                // The option is the operand that has a value at the call:
                // the left where both do.
                let valued = |operand: &SyntaxNode| {
                    option(operand).filter(|name| (of.value_of)(name).is_some())
                };
                let (name, constant) = match (valued(left), valued(right)) {
                    (Some(name), _) => (name, right),
                    (None, Some(name)) => (name, left),
                    (None, None) => return None,
                };
                // The language's `==` (`ConstantValue::equals`). GEN-3:
                // "An equality of two constants there is the `==` a
                // program evaluates, and not their sameness (UNIT-13):
                // with `const k: f64`, `requires k == 0.0` holds where `k`
                // is `-0.0`." Where the value is not known, whether it
                // holds is not either.
                compared(&(of.value_of)(&name)?, &operand(constant, &name)?, of)
            } else if within {
                let name = option(left)?;
                if right.kind() != SyntaxKind::ConstraintSet {
                    return None;
                }
                let value = (of.value_of)(&name)?;
                // GEN-3: "the membership of a constant in a set of
                // constants": it holds where it is one of them, and does
                // not where it is known to be none.
                let mut found = Some(false);
                for member in right.children() {
                    match operand(&member, &name).and_then(|member| compared(&value, &member, of)) {
                        Some(true) => return Some(true),
                        Some(false) => {}
                        None => found = None,
                    }
                }
                found
            } else {
                None
            }
        }
        _ => None,
    }
}

/// GEN-10: test every selected type, with the quantifier's name bound
/// to that member. One false test settles the verdict even where
/// another is untold. An empty pack has no false or untold test.
fn each_holds<'p>(
    node: &SyntaxNode,
    of: &Condition,
    pack_types: &dyn Fn(&SyntaxToken, &str) -> Option<&'p [Type]>,
) -> Option<bool> {
    let bound = tokens(node)
        .filter(|token| token.kind() == SyntaxKind::Ident)
        .nth(1)?;
    let mut parts = node.children();
    let (view, pack) = types_view(parts.next()?)?;
    let body = parts.next()?;
    let types = pack_types(&view, pack.text())?;
    let mut held = Some(true);
    for ty in types {
        let type_of = |name: &str| {
            if name == bound.text() {
                (whole(ty) && !names_parameter(ty)).then(|| ty.clone())
            } else {
                (of.type_of)(name)
            }
        };
        let written = |operand: &SyntaxNode| {
            if operand.kind() == SyntaxKind::NamedType && operand.text() == bound.text() {
                type_of(bound.text())
            } else {
                (of.written)(operand)
            }
        };
        let here = Condition {
            type_of: &type_of,
            written: &written,
            ..*of
        };
        match holds_with_packs(&body, &here, pack_types) {
            Some(false) => return Some(false),
            Some(true) => {}
            None => held = None,
        }
    }
    held
}

/// The view name and pack name in the GEN-10 form `types(Ts)`. Forms
/// that do not select a known type sequence are left untold.
fn types_view(source: SyntaxNode) -> Option<(SyntaxToken, SyntaxToken)> {
    let source = ungrouped(source);
    if source.kind() != SyntaxKind::CallExpr {
        return None;
    }
    let callee = child(&source, SyntaxKind::NameExpr)?;
    let view = name_token(&callee)?;
    if view.text() != "types" || child(&callee, SyntaxKind::QualifiedName).is_some() {
        return None;
    }
    let arguments = child(&source, SyntaxKind::ArgList)?;
    let mut arguments = arguments.children();
    let argument = arguments.next()?;
    if argument.kind() != SyntaxKind::Arg || arguments.next().is_some() {
        return None;
    }
    let pack = ungrouped(argument.children().next()?);
    if pack.kind() != SyntaxKind::NameExpr || child(&pack, SyntaxKind::QualifiedName).is_some() {
        return None;
    }
    Some((view, name_token(&pack)?))
}
