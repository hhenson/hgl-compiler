//! `requires`: whether a declaration's condition holds for the generic
//! arguments it is called or applied with, and the operations a generic
//! body may call because its requirements name them (GEN-3, GEN-7,
//! GEN-15, GEN-16). Part of the typing pass ([`super`]).

mod finite;

use finite::FiniteDomains;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use hgl_diagnostics::{Category, Code, Diagnostic, Diagnostics, Location, Severity};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};
use hgl_source::FileId;

use super::candidates::{AtInstance, Given, Wanted};
use super::fit::{open, whole};
use super::forwarded_packs::{visit_anonymous_bodies, walk_nested_calls};
use super::inference::{fixed, mentions, placed, unsaid};
use super::operators::{Requested, from_place, has_generic_configuration_default, unreadable};
use super::{REQUIRING, Typer};
use crate::constant::Scope;
use crate::declared::{
    names_parameter, same_type, substitute_with_members, substitute_within, weight,
};
use crate::hir::{self, Callee, Expr, ExprKind, Form, ValueCategory};
use crate::program::{
    Binding, Declaration, DeclarationKind, LocalKind, OperatorRef, Phase, Program, Role,
};
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
            // Body-level applications whose `type(...)` arguments are still
            // deferred are checked once `written_type_at` resolves them.
            for (struct_, arguments) in &found {
                if arguments.iter().any(|argument| match argument {
                    Argument::Type(ty) => crate::constant::type_has_deferred_query(ty),
                    Argument::Constant(constant) => {
                        crate::constant::has_deferred_type_query(&constant.value)
                    }
                    Argument::NoResult => false,
                }) {
                    continue;
                }
                self.requires_met(written.at, *struct_, arguments);
            }
        }
    }

    /// GEN-3: checks `requires` on struct applications inside `ty` whose
    /// deferred `type(...)` arguments have just resolved in a body (TYPE-18).
    pub(super) fn check_resolved_struct_applications(&mut self, at: Location, ty: &Type) {
        let mut found = Vec::new();
        struct_applications(ty, &mut found);
        for (struct_, arguments) in &found {
            if required(self.program, *struct_).is_some() {
                self.requires_met(at, *struct_, arguments);
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
        // GEN-3, GEN-6: a positive bool term states the same bound as
        // `flag == true`, and its negation as `flag == false`.
        fn bool_terms(node: &SyntaxNode, found: &mut Vec<(SyntaxToken, bool)>) {
            match node.kind() {
                SyntaxKind::ConstraintParen => {
                    if let Some(inside) = node.children().next() {
                        bool_terms(&inside, found);
                    }
                }
                SyntaxKind::ConstraintBinary if has_token(node, SyntaxKind::AmpAmp) => {
                    for side in node.children() {
                        bool_terms(&side, found);
                    }
                }
                SyntaxKind::ConstraintValue => {
                    if let Some(name) = name_token(node) {
                        found.push((name, true));
                    }
                }
                SyntaxKind::ConstraintNot => {
                    let Some(mut inside) = node.children().next() else {
                        return;
                    };
                    // GEN-15: grouping does not change the negated term.
                    while inside.kind() == SyntaxKind::ConstraintParen {
                        let Some(inner) = inside.children().next() else {
                            return;
                        };
                        inside = inner;
                    }
                    if inside.kind() == SyntaxKind::ConstraintValue
                        && let Some(name) = name_token(&inside)
                    {
                        found.push((name, false));
                    }
                }
                _ => {}
            }
        }
        let mut terms = Vec::new();
        if let Some(condition) = clause.children().next() {
            bool_terms(&condition, &mut terms);
        }
        for (name, value) in terms {
            if let Some(generic) = self.signature.generics.iter().find(|generic| {
                generic.name == name.text()
                    && matches!(
                        generic.kind,
                        GenericKind::Value(Type::Scalar(crate::types::Scalar::Bool))
                    )
            }) {
                bound.push((
                    generic.at,
                    Argument::Constant(Constant {
                        text: value.to_string(),
                        value: ConstantValue::Bool(value),
                    }),
                ));
            }
        }
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
                        Argument::Constant(_) | Argument::NoResult => None,
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
                    // GEN-10: the pack evaluator checks a quantified
                    // call with each iteration's bound operands.
                    && !node
                        .ancestors()
                        .any(|parent| matches!(parent.kind(), SyntaxKind::ConstraintEach | SyntaxKind::ConstraintAny))
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
    /// where that declaration states it (GEN-7). `false` keeps an
    /// unproved reflected argument symbolic after reporting its failure.
    pub(super) fn requires_met(
        &mut self,
        at: Location,
        callee: (usize, usize),
        generics: &[Argument],
    ) -> bool {
        let Some((condition, held, unvalued)) = self.requires_told(callee, generics) else {
            return true;
        };
        // A construction that writes its generic arguments,
        // `Sized<str>(value: "x")`, writes a type, of which the same
        // has been reported ([`applications`]).
        if unmet_reported(self.diagnostics, at) {
            return true;
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
            None if function && (passes_on(generics, &own) || reflects(generics, &own)) => {
                return self.check_stated_requirements(at, callee, (generics, &[]), &condition);
            }
            None if unvalued && !passes_on(generics, &own) => self
                .diagnostics
                .emit(untold(program, callee, at, &condition)),
            _ => {}
        }
        true
    }

    /// GEN-3 and GEN-10: a function that receives resolved type packs
    /// must have a requirement known to hold for those types. Report
    /// both a false condition and a form this pass cannot judge. An
    /// unproved reflected member returns `false` as `requires_met` does.
    pub(super) fn pack_requires_met(
        &mut self,
        at: Location,
        callee: (usize, usize),
        generics: &[Argument],
        type_packs: &[hir::TypePackArgument],
    ) -> bool {
        if type_packs.iter().any(|pack| {
            pack.forwarded_from.is_some_and(|source| {
                !pack
                    .types
                    .iter()
                    .any(|ty| matches!(ty, Type::Parameter { at, .. } if *at == source))
            })
        }) {
            // GEN-10 tuple captures keep a marker and must prove the
            // callee's requirements under the body's own constraints.
            // GEN-17: a function parameter's reflected shape is supplied
            // by the caller of the enclosing declaration.
            return true;
        }
        let member_fault = std::cell::RefCell::new(None);
        let Some((condition, held, unvalued)) =
            self.requires_told_with_packs(callee, generics, type_packs, Some(&member_fault))
        else {
            return true;
        };
        if let Some(fault) = member_fault.into_inner() {
            self.pack_member_fault(at, &fault);
            return true;
        }
        if unmet_reported(self.diagnostics, at) {
            return true;
        }
        let program = self.program;
        let own: Vec<_> = self
            .signature
            .generics
            .iter()
            .map(|generic| generic.at)
            .collect();
        let report = match held {
            Some(true) => return true,
            Some(false) => unmet(program, callee, at, &condition),
            None if passes_on(generics, &own)
                || reflects(generics, &own)
                || packs_pass_on(type_packs, &own) =>
            {
                return self.check_stated_requirements(
                    at,
                    callee,
                    (generics, type_packs),
                    &condition,
                );
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
        true
    }

    /// GEN-10, ERR-9: an existential known to be false rejects the call
    /// before its missing binding is reported as an unresolved result.
    pub(super) fn pack_requires_rejects(
        &mut self,
        at: Location,
        callee: (usize, usize),
        generics: &[Option<Argument>],
        type_packs: &[hir::TypePackArgument],
    ) -> bool {
        if type_packs.is_empty()
            || type_packs
                .iter()
                .any(|pack| pack.forwarded_from.is_some() || pack.infer_from_arguments)
        {
            return false;
        }
        let Some((_, condition)) = required(self.program, callee) else {
            return false;
        };
        let mut conjuncts = Vec::new();
        super::quantifiers::positive(&condition, &mut conjuncts);
        if !conjuncts
            .iter()
            .any(|node| node.kind() == SyntaxKind::ConstraintAny)
        {
            return false;
        }
        let generics: Vec<_> = generics
            .iter()
            .map(|argument| argument.clone().unwrap_or(Argument::Type(Type::Unknown)))
            .collect();
        let Some((condition, Some(false), _)) =
            self.requires_told_with_packs(callee, &generics, type_packs, None)
        else {
            return false;
        };
        if !unmet_reported(self.diagnostics, at) {
            self.diagnostics
                .emit(unmet(self.program, callee, at, &condition));
        }
        true
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
                    Argument::NoResult => "none".to_string(),
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
    fn check_stated_requirements(
        &mut self,
        at: Location,
        callee: (usize, usize),
        (generics, type_packs): (&[Argument], &[hir::TypePackArgument]),
        condition: &SyntaxNode,
    ) -> bool {
        let program = self.program;
        let own: Vec<Location> = self
            .signature
            .generics
            .iter()
            .map(|generic| generic.at)
            .collect();
        let evidence_types = generics
            .iter()
            .filter_map(|argument| match argument {
                Argument::Type(ty) => Some(ty),
                _ => None,
            })
            .chain(type_packs.iter().flat_map(|pack| &pack.types))
            .collect::<Vec<_>>();
        let dependencies = self.stated_member_dependencies(&evidence_types);
        let reflected = evidence_types.iter().any(|ty| {
            ty.any_part(
                &|part| matches!(part, Type::PackMember { pack, .. } if own.contains(pack)),
                &|_| false,
            )
        });
        let domains = self.finite_domains();
        let memberships: Vec<(Location, Vec<Argument>)> = domains
            .memberships()
            .into_iter()
            .filter(|(parameter, _)| {
                dependencies.contains(parameter)
                    || passes_on(generics, &[*parameter])
                    || packs_pass_on(type_packs, &[*parameter])
            })
            .collect();
        if memberships.iter().any(|(_, choices)| choices.is_empty()) {
            self.diagnostics.emit(untold_with_reason(
                program,
                callee,
                at,
                condition,
                "no combination satisfies the stated finite requirements",
            ));
            return !reflected;
        }
        let mut combinations = domains.ordinary_trials(&memberships);
        if memberships.is_empty() {
            combinations.push(Vec::new());
        }
        let bound = self.bound.clone();
        let name = &program.modules[callee.0].declarations[callee.1].name;
        let body = &self.declaration.name;
        let mut remaining = crate::INSTANCES;
        if combinations.len() > remaining {
            self.diagnostics.emit(untold_with_reason(
                program,
                callee,
                at,
                condition,
                "the analyser's limit on combinations of stated requirements was reached",
            ));
            return !reflected;
        }
        for combination in &combinations {
            let given: Vec<(Location, &Argument)> = bound
                .iter()
                .chain(combination)
                .map(|(at, argument)| (*at, argument))
                .collect();
            let Some(member_choices) =
                self.stated_member_choices(&evidence_types, &given, &domains)
            else {
                continue;
            };
            // ERR-9: "Failure during argument evaluation precedes later
            // admission checks": a generic argument worked out from the
            // body's parameters that fails for what the body admits.
            let made = Made {
                callee,
                generics: generics.to_vec(),
                type_packs: type_packs.to_vec(),
                constants: ConstArguments::default(),
                answers: Vec::new(),
                test_candidates: self.test_candidates,
                required: None,
                at,
                via: None,
            };
            if let Some(report) = argument_fails(program, &made, &given, &own) {
                self.diagnostics.emit(if combination.is_empty() {
                    report
                } else {
                    report.with_note(format!(
                        "for {}, which the `requires` of `{body}` admits",
                        self.members_named(combination).join(" and ")
                    ))
                });
                return true;
            }
            let mut member_trials = domains.member_trials(&member_choices, &given);
            if member_choices.is_empty() {
                member_trials.push(Vec::new());
            }
            let Some(rest) = remaining.checked_sub(member_trials.len()) else {
                self.diagnostics.emit(untold_with_reason(
                    program,
                    callee,
                    at,
                    condition,
                    "the analyser's limit on combinations of stated requirements was reached",
                ));
                return !reflected;
            };
            remaining = rest;
            let failed = member_trials.iter().find_map(|members| {
                let member_type = |pack, _: &str, key: &Constant| {
                    members.iter().find_map(|((at, stated), ty)| {
                        (*at == pack && stated.value.same(&key.value) == Some(true))
                            .then(|| ty.clone())
                    })
                };
                let bound = members.iter().map(|(_, ty)| weight(ty)).max().unwrap_or(1);
                let project = |ty: &Type| {
                    placed(
                        substitute_with_members(ty, &given, &member_type, bound),
                        &own,
                    )
                };
                let placed_generics = generics
                    .iter()
                    .map(|argument| match argument {
                        Argument::Type(ty) => Argument::Type(project(ty)),
                        _ => given_in(program, argument, &given, &own),
                    })
                    .collect::<Vec<_>>();
                let placed_packs = type_packs
                    .iter()
                    .cloned()
                    .map(|mut pack| {
                        pack.infer_from_arguments = false;
                        pack.place_types(project);
                        pack
                    })
                    .collect::<Vec<_>>();
                let held = self
                    .requires_told_with_packs(callee, &placed_generics, &placed_packs, None)
                    .map(|(_, held, _)| held);
                (held != Some(Some(true))).then_some(held)
            });
            let Some(held) = failed else { continue };
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
            self.diagnostics.emit(
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
            return !reflected;
        }
        if !combinations.is_empty() && remaining == crate::INSTANCES {
            self.diagnostics.emit(untold_with_reason(
                program,
                callee,
                at,
                condition,
                "no combination satisfies the stated reflected member bounds",
            ));
            return !reflected;
        }
        true
    }

    /// Positive equality and membership constrain a GEN-17 member's
    /// type. An `each` over the same pack's keys constrains every member.
    fn stated_pack_members(&self) -> Vec<StatedPackMember> {
        if !self
            .signature
            .generics
            .iter()
            .any(|slot| matches!(slot.kind, GenericKind::Pack { .. }))
        {
            return Vec::new();
        }
        let Some(condition) = child(&self.declaration.syntax, SyntaxKind::RequiresClause)
            .and_then(|clause| clause.children().next())
        else {
            return Vec::new();
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
        let mut members = Vec::new();
        for term in terms {
            if term.kind() == SyntaxKind::ConstraintRelation {
                members.extend(self.stated_member_bound(&term));
                continue;
            }
            // An existential `any` never proves an arbitrary member's type.
            let Some(bound) = tokens(&term)
                .filter(|token| token.kind() == SyntaxKind::Ident)
                .nth(1)
            else {
                continue;
            };
            let mut parts = term.children();
            let (Some((view, name)), Some(body)) = (parts.next().and_then(pack_view), parts.next())
            else {
                continue;
            };
            let Some(pack) = self
                .signature
                .generics
                .iter()
                .find(|generic| generic.name == name.text())
            else {
                continue;
            };
            if view.text() != "arg_keys" {
                continue;
            }
            let bound = Location::new(self.file, bound.span());
            let mut relations = Vec::new();
            conjoined_relations(&body, &mut relations);
            for relation in relations {
                let Some(mut member) = self.stated_member_bound(&relation) else {
                    continue;
                };
                if member.pack == pack.at
                    && matches!(member.key, StatedPackKey::One(ref key) if matches!(key.value, ConstantValue::Parameter { at, .. } if at == bound))
                {
                    member.key = StatedPackKey::Every(bound);
                    members.push(member);
                }
            }
        }
        members
    }

    /// The member and type bound stated in one GEN-3 relation.
    fn stated_member_bound(&self, relation: &SyntaxNode) -> Option<StatedPackMember> {
        if tokens(relation).any(|token| token.text() == "in") {
            return self.stated_member_membership(relation);
        }
        if !has_token(relation, SyntaxKind::EqEq) {
            return None;
        }
        let mut operands = relation.children();
        let left_at = operands.next()?.span().start;
        let right_at = operands.next()?.span().start;
        let left = self.written.get(&(self.file, left_at))?;
        let right = self.written.get(&(self.file, right_at))?;
        // A relation to another member does not read either member's type.
        let readable = |given| {
            whole(given) && !given.any_part(&|ty| matches!(ty, Type::PackMember { .. }), &|_| false)
        };
        let (pack, key, given) = match (*left, *right) {
            (Type::PackMember { pack, key, .. }, given) if readable(given) => (pack, key, given),
            (given, Type::PackMember { pack, key, .. }) if readable(given) => (pack, key, given),
            _ => return None,
        };
        Some(StatedPackMember {
            pack: *pack,
            key: StatedPackKey::One(key.clone()),
            types: StatedMemberTypes::Equal(given.clone()),
        })
    }

    /// Ordinary parameters that supply a reflected member's stated type
    /// or key. Their memberships also constrain a call using that member
    /// (GEN-3, GEN-7, GEN-17).
    pub(super) fn stated_member_dependencies(&self, types: &[&Type]) -> Vec<Location> {
        let bounds = self.stated_pack_members();
        if bounds.is_empty() {
            return Vec::new();
        }
        let ordinary: Vec<_> = self
            .signature
            .generics
            .iter()
            .map(|slot| slot.at)
            .filter(|at| types.iter().any(|ty| mentions(ty, &[*at])))
            .collect();
        let (_, keys) = self.stated_member_keys(types, &ordinary, &bounds, &|_| None);
        let used: Vec<_> = bounds
            .iter()
            .filter(|member| {
                keys.iter().any(|(pack, key)| {
                    *pack == member.pack
                        && match &member.key {
                            StatedPackKey::One(stated) => {
                                stated.value.same(&key.value) != Some(false)
                            }
                            StatedPackKey::Every(_) => true,
                        }
                })
            })
            .collect();
        self.signature
            .generics
            .iter()
            .map(|generic| generic.at)
            .filter(|at| {
                let one = &[*at];
                used.iter().any(|member| {
                    member.types.values().iter().any(|ty| mentions(ty, one))
                        || matches!(&member.key, StatedPackKey::One(key) if key.value.names_one_of(one))
                })
            })
            .collect()
    }

    /// The keys read by this call come first. Also include keys whose stated
    /// bounds can exclude an ordinary argument, so all parameters bounded
    /// through that same key participate in the trial.
    fn stated_member_keys(
        &self,
        types: &[&Type],
        ordinary: &[Location],
        bounds: &[StatedPackMember],
        constant_at: &impl Fn(Location) -> Option<ConstantValue>,
    ) -> (usize, Vec<(Location, Constant)>) {
        let remember = |keys: &mut Vec<(Location, Constant)>, pack, key: &Constant| {
            let value = key.value.substituted(constant_at);
            if !keys
                .iter()
                .any(|(at, known)| *at == pack && known.value.same(&value) == Some(true))
            {
                keys.push((
                    pack,
                    Constant {
                        text: key.text.clone(),
                        value,
                    },
                ));
            }
        };
        let keys = RefCell::new(Vec::new());
        for ty in types {
            ty.any_part(
                &|part| {
                    if let Type::PackMember { pack, key, .. } = part
                        && self.signature.generics.iter().any(|slot| slot.at == *pack)
                    {
                        remember(&mut keys.borrow_mut(), *pack, key);
                    }
                    false
                },
                &|_| false,
            );
        }
        let mut keys = keys.into_inner();
        let used = keys.len();
        // A named key in the declaration can rule out an ordinary binding
        // even if the call uses just that parameter. An each alone promises
        // no key: an empty pack satisfies it as well.
        for bound in bounds {
            let binds_type = bound.types.values().iter().any(|ty| mentions(ty, ordinary));
            match &bound.key {
                StatedPackKey::One(key) if binds_type || key.value.names_one_of(ordinary) => {
                    remember(&mut keys, bound.pack, key);
                }
                StatedPackKey::Every(_) if binds_type => {
                    for named in bounds {
                        if named.pack == bound.pack
                            && let StatedPackKey::One(key) = &named.key
                        {
                            remember(&mut keys, named.pack, key);
                        }
                    }
                }
                _ => {}
            }
        }
        (used, keys)
    }

    /// The finite choices a body states for the reflected members a
    /// call uses. Keep the key as well as the pack (GEN-3, GEN-7, GEN-17).
    /// None means the given ordinary bindings contradict a member's bounds.
    pub(super) fn stated_member_choices(
        &self,
        types: &[&Type],
        given: &[(Location, &Argument)],
        domains: &FiniteDomains,
    ) -> Option<Vec<MemberChoices>> {
        let mut bounds = self.stated_pack_members();
        domains.add_member_bounds(&mut bounds, given);
        if bounds.is_empty() {
            return Some(Vec::new());
        }
        let constant_at = |at| match given.iter().find(|(declared, _)| *declared == at)?.1 {
            Argument::Constant(constant) => Some(constant.value.clone()),
            Argument::Type(_) | Argument::NoResult => None,
        };
        let ordinary: Vec<_> = given.iter().map(|(at, _)| *at).collect();
        let (used, keys) = self.stated_member_keys(types, &ordinary, &bounds, &constant_at);
        let mut choices = Vec::new();
        for (index, (pack, key)) in keys.into_iter().enumerate() {
            let matching = bounds
                .iter()
                .filter(|member| {
                    member.pack == pack
                        && match &member.key {
                            StatedPackKey::One(stated) => {
                                stated.value.substituted(&constant_at).same(&key.value)
                                    == Some(true)
                            }
                            StatedPackKey::Every(_) => true,
                        }
                })
                .map(|member| {
                    member
                        .types
                        .values()
                        .iter()
                        .map(|ty| {
                            let ty = substitute_within(ty, given);
                            match member.key {
                                StatedPackKey::One(_) => ty,
                                StatedPackKey::Every(bound) => substitute_within(
                                    &ty,
                                    &[(bound, &Argument::Constant(key.clone()))],
                                ),
                            }
                        })
                        .collect()
                })
                .collect();
            let types = self.intersect_member_bounds(matching)?;
            // Unused keys establish reachability only. Multiplying by their
            // possible types would repeat the same check of this call.
            if index < used && !types.is_empty() {
                choices.push(((pack, key), types));
            }
        }
        Some(choices)
    }

    /// Intersect all positive bounds stated for one reflected pack member
    /// regardless of conjunction order (GEN-3, GEN-7, GEN-10). Concrete
    /// bounds narrow the candidate set before open ones; open bounds never
    /// discard a type that an unresolved normalizing or open form can later
    /// become. Without a concrete bound, only a known-subset open bound is
    /// kept so differing type variables stay conservative in both orders.
    /// An unproved member supplies no choices. Disjoint concrete bounds
    /// rule out this ordinary binding entirely.
    fn intersect_member_bounds(&self, bounds: Vec<Vec<Type>>) -> Option<Vec<Type>> {
        let is_concrete = |ty: &Type| {
            whole(ty)
                && !ty.any_part(
                    &|part| {
                        matches!(part, Type::Parameter { .. } | Type::PackMember { .. })
                            || matches!(
                                part,
                                Type::Function(template) if template.named_pack.is_some()
                            )
                    },
                    &|constant| constant.value.key().is_none(),
                )
        };
        let (mut closed, open): (Vec<Vec<Type>>, Vec<Vec<Type>>) = bounds
            .into_iter()
            .partition(|set| set.iter().all(is_concrete));
        let (base, closed_rest, open_rest) = if !closed.is_empty() {
            let first = closed.remove(0);
            // Only a definite mismatch excludes a trial. Equality can also
            // return unknown, which supplies no proof either way.
            if first.iter().all(|ty| {
                closed
                    .iter()
                    .any(|set| set.iter().all(|other| same_type(ty, other) == Some(false)))
            }) {
                return None;
            }
            (first, closed, open)
        } else if !open.is_empty() {
            // Without a concrete bound, keep an open bound only when every
            // other open bound is known to contain every element of it.
            let Some(chosen) = open.iter().find(|set| {
                open.iter().all(|other| {
                    set.iter().all(|ty| {
                        other
                            .iter()
                            .any(|candidate| same_type(ty, candidate) == Some(true))
                    })
                })
            }) else {
                return Some(Vec::new());
            };
            (chosen.clone(), Vec::new(), Vec::new())
        } else {
            return Some(Vec::new());
        };
        let mut intersected = Vec::new();
        for ty in base {
            if closed_rest.iter().all(|other| {
                other
                    .iter()
                    .any(|candidate| same_type(&ty, candidate) == Some(true))
            }) && open_rest.iter().all(|other| {
                other
                    .iter()
                    .any(|candidate| self.types_may_meet(&ty, candidate))
            }) && !intersected
                .iter()
                .any(|existing| same_type(existing, &ty) == Some(true))
            {
                intersected.push(ty);
            }
        }
        Some(intersected)
    }

    /// True when some valid instantiation of the enclosing generics may make
    /// `left` and `right` normalize to the same type (TYPE-9, TYPE-10, TYPE-14,
    /// TYPE-16, GEN-7). Used only so an open bound never discards a type that
    /// an unresolved normalizing or open form can later become.
    fn types_may_meet(&self, left: &Type, right: &Type) -> bool {
        fn atomic_may_unwrap(payload: &Type) -> bool {
            match payload {
                Type::Unknown
                | Type::Parameter { .. }
                | Type::PackMember { .. }
                | Type::Scalar(_)
                | Type::Error
                | Type::Result { .. }
                | Type::Atomic(_)
                | Type::Delta(_)
                | Type::Named {
                    kind: NamedKind::Enum | NamedKind::Native,
                    ..
                } => true,
                Type::Nullable(inner) => atomic_may_unwrap(inner),
                _ => false,
            }
        }
        fn delta_may_normalize(of: &Type) -> bool {
            match of {
                Type::Unknown
                | Type::Parameter { .. }
                | Type::PackMember { .. }
                | Type::Scalar(_)
                | Type::Error
                | Type::ErrorContext
                | Type::Result { .. }
                | Type::Named {
                    kind: NamedKind::Enum | NamedKind::Native,
                    ..
                }
                | Type::Delta(_)
                | Type::Atomic(_)
                | Type::RollingWindow { .. }
                | Type::Tuple(_) => true,
                Type::Nullable(inner) => match inner.as_ref() {
                    // TYPE-16: a delta with its own identity is an ordinary
                    // value. Its nullable's delta keeps that identity too.
                    Type::Delta(payload) => delta_may_normalize(payload),
                    Type::Unknown
                    | Type::Parameter { .. }
                    | Type::PackMember { .. }
                    | Type::RollingWindow { .. }
                    | Type::Atomic(_)
                    | Type::Signal
                    | Type::Nullable(_) => true,
                    _ => false,
                },
                _ => false,
            }
        }
        let delta_can_change_shape = |ty: &Type| match ty {
            Type::Unknown | Type::Parameter { .. } | Type::PackMember { .. } => true,
            Type::Delta(inner) => delta_may_normalize(inner),
            _ => false,
        };
        if delta_can_change_shape(left) || delta_can_change_shape(right) {
            return true;
        }
        // TYPE-9: `Type::atomic(payload)` either stays `Atomic(payload)` or
        // unwraps to `payload` when `payload` is non-composite (`whole`), so
        // `atomic<T?>` can meet `i64?` or `atomic<list<i64>?>` but never `i64`
        // or `list<i64>?`.
        match (left, right) {
            (Type::Atomic(left_inner), Type::Atomic(right_inner)) => {
                return self.types_may_meet(left_inner, right_inner)
                    || (atomic_may_unwrap(left_inner) && self.types_may_meet(left_inner, right))
                    || (atomic_may_unwrap(right_inner) && self.types_may_meet(left, right_inner));
            }
            (Type::Atomic(inner), other) | (other, Type::Atomic(inner)) => {
                return atomic_may_unwrap(inner)
                    && atomic_may_unwrap(other)
                    && self.types_may_meet(inner, other);
            }
            _ => {}
        }
        let constants_may_meet = |left: &Constant, right: &Constant| {
            matches!(left.value, ConstantValue::Unknown)
                || matches!(right.value, ConstantValue::Unknown)
                || left.value.same(&right.value) != Some(false)
        };
        let is_pack_type = |ty: &Type| {
            matches!(
                ty,
                Type::Parameter { at, .. }
                    if self
                        .signature
                        .generics
                        .iter()
                        .find(|generic| generic.at == *at)
                        .is_none_or(|generic| matches!(generic.kind, GenericKind::Pack { .. }))
            )
        };
        match (left, right) {
            (
                Type::Named {
                    module,
                    declaration,
                    arguments,
                    ..
                },
                Type::Named {
                    module: other_module,
                    declaration: other_declaration,
                    arguments: other_arguments,
                    ..
                },
            ) => {
                if module != other_module || declaration != other_declaration {
                    return false;
                }
                let has_pack = |args: &[Argument]| {
                    args.iter()
                        .any(|arg| matches!(arg, Argument::Type(ty) if is_pack_type(ty)))
                };
                if has_pack(arguments) || has_pack(other_arguments) {
                    return true;
                }
                arguments.len() == other_arguments.len()
                    && arguments
                        .iter()
                        .zip(other_arguments)
                        .all(|pair| match pair {
                            (Argument::Type(left), Argument::Type(right)) => {
                                self.types_may_meet(left, right)
                            }
                            (Argument::Constant(left), Argument::Constant(right)) => {
                                constants_may_meet(left, right)
                            }
                            (Argument::NoResult, Argument::NoResult) => true,
                            _ => false,
                        })
            }
            (Type::Tuple(left), Type::Tuple(right)) => {
                if left.iter().any(is_pack_type) || right.iter().any(is_pack_type) {
                    return true;
                }
                left.len() == right.len()
                    && left
                        .iter()
                        .zip(right)
                        .all(|(left, right)| self.types_may_meet(left, right))
            }
            (
                Type::List { element, size },
                Type::List {
                    element: other_element,
                    size: other_size,
                },
            ) => {
                let sizes = match (size, other_size) {
                    (Some(left), Some(right)) => {
                        constants_may_meet(left, right)
                            || left.value.same_extent(&right.value)
                            || right.value.same_extent(&left.value)
                    }
                    (Some(size), None) | (None, Some(size)) => size.value.key().is_none(),
                    (None, None) => true,
                };
                sizes && self.types_may_meet(element, other_element)
            }
            (Type::Set(left), Type::Set(right))
            | (Type::Endpoint(left), Type::Endpoint(right))
            | (Type::Delta(left), Type::Delta(right))
            | (Type::Nullable(left), Type::Nullable(right)) => self.types_may_meet(left, right),
            (
                Type::Dict { key, value },
                Type::Dict {
                    key: other_key,
                    value: other_value,
                },
            ) => self.types_may_meet(key, other_key) && self.types_may_meet(value, other_value),
            (
                Type::RollingWindow {
                    value,
                    maximum,
                    minimum,
                },
                Type::RollingWindow {
                    value: other_value,
                    maximum: other_maximum,
                    minimum: other_minimum,
                },
            ) => {
                let minimum = minimum.as_ref().unwrap_or(maximum);
                let other_minimum = other_minimum.as_ref().unwrap_or(other_maximum);
                constants_may_meet(maximum, other_maximum)
                    && constants_may_meet(minimum, other_minimum)
                    && self.types_may_meet(value, other_value)
            }
            (
                Type::Result { value, error },
                Type::Result {
                    value: other_value,
                    error: other_error,
                },
            ) => self.types_may_meet(value, other_value) && self.types_may_meet(error, other_error),
            (Type::Trace { cell: left, .. }, Type::Trace { cell: right, .. }) => {
                names_parameter(left) || names_parameter(right) || self.types_may_meet(left, right)
            }
            (Type::Function(left), Type::Function(right)) => {
                if left.value != right.value || left.reference != right.reference {
                    return false;
                }
                if left.named_pack.is_some() || right.named_pack.is_some() {
                    return true;
                }
                let left_params: Vec<_> = left.ordinary_parameters().collect();
                let right_params: Vec<_> = right.ordinary_parameters().collect();
                if left_params
                    .iter()
                    .chain(&right_params)
                    .any(|param| param.declared().is_some_and(|decl| decl.pack.is_some()))
                {
                    return true;
                }
                let results = match (&left.result, &right.result) {
                    (Some(left), Some(right)) => self.types_may_meet(left, right),
                    (None, None) => true,
                    (Some(_), None) => left.result_admits_none,
                    (None, Some(_)) => right.result_admits_none,
                };
                results
                    && left_params.len() == right_params.len()
                    && left_params
                        .iter()
                        .zip(&right_params)
                        .all(|(left, right)| self.types_may_meet(left.ty(), right.ty()))
            }
            (Type::Scalar(left), Type::Scalar(right)) => left == right,
            (Type::Signal, Type::Signal)
            | (Type::Error, Type::Error)
            | (Type::ErrorContext, Type::ErrorContext) => true,
            _ => false,
        }
    }

    /// A positive membership of a named member in a finite set of types.
    fn stated_member_membership(&self, relation: &SyntaxNode) -> Option<StatedPackMember> {
        let mut operands = relation.children();
        let (member, set) = (operands.next()?, operands.next()?);
        if set.kind() != SyntaxKind::ConstraintSet {
            return None;
        }
        let Type::PackMember { pack, key, .. } =
            self.written.get(&(self.file, member.span().start))?
        else {
            return None;
        };
        let types: Vec<_> = set
            .children()
            .map(|item| {
                let ty = self.written.get(&(self.file, item.span().start))?;
                (whole(ty) && !ty.any_part(&|ty| matches!(ty, Type::PackMember { .. }), &|_| false))
                    .then(|| (*ty).clone())
            })
            .collect::<Option<_>>()?;
        (!types.is_empty()).then_some(StatedPackMember {
            pack: *pack,
            key: StatedPackKey::One(key.clone()),
            types: StatedMemberTypes::Alternatives(types),
        })
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
        if unmet_reported(self.diagnostics, at) {
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

    /// Whether the `requires` of `callee` holds with `generics` for its
    /// generic parameters: the condition, whether it holds (`None` where
    /// that is not told), and whether a constant it compares is one this
    /// compiler does not work out ([`Condition::unvalued`]). `None` for a
    /// callee that states no `requires`.
    /// GEN-18: a requirement may inspect the static function supplied at
    /// this call. Return whether such a requirement was handled here.
    pub(super) fn function_laws_met(
        &mut self,
        at: Location,
        callee: (usize, usize),
        generics: &[Argument],
        packs: &[hir::TypePackArgument],
        arguments: &[hir::Argument],
    ) -> bool {
        let Some((_, condition)) = required(self.program, callee) else {
            return false;
        };
        let Detail::Callable(signature) = &self.program.modules[callee.0].details[callee.1] else {
            return false;
        };
        let laws = crate::laws::verdicts(&condition, &|node| {
            self.law_call(callee, signature, node, &|parameter| {
                arguments
                    .iter()
                    .find(|argument| argument.parameter == Some(parameter))
                    .map(|argument| &argument.value)
            })
        });
        if laws.is_empty() {
            return false;
        }
        let Some((condition, held, _)) =
            self.requires_told_with_laws(callee, generics, packs, None, &laws)
        else {
            return true;
        };
        if unmet_reported(self.diagnostics, at) {
            return true;
        }
        if held == Some(false) {
            self.diagnostics
                .emit(unmet(self.program, callee, at, &condition));
        } else if held.is_none() {
            self.diagnostics.emit(untold_with_reason(
                self.program,
                callee,
                at,
                &condition,
                "the function's declared law is not known for this call",
            ));
        }
        true
    }

    fn requires_told(
        &mut self,
        callee: (usize, usize),
        generics: &[Argument],
    ) -> Option<(SyntaxNode, Option<bool>, bool)> {
        self.requires_told_with_packs(callee, generics, &[], None)
    }

    /// The same check with any resolved type packs available to GEN-10
    /// quantifiers in the condition.
    pub(super) fn requires_told_with_packs(
        &mut self,
        (module, declaration): (usize, usize),
        generics: &[Argument],
        type_packs: &[hir::TypePackArgument],
        member_fault: Option<&std::cell::RefCell<Option<super::dependent_packs::MemberFault>>>,
    ) -> Option<(SyntaxNode, Option<bool>, bool)> {
        self.requires_told_with_laws(
            (module, declaration),
            generics,
            type_packs,
            member_fault,
            &[],
        )
    }

    /// GEN-3 and FN-17: test the known bindings of a callback trial.
    /// Unknown operands remain for the ordinary call check.
    pub(super) fn callback_requirement_holds(
        &mut self,
        (target, signature): ((usize, usize), &Signature),
        (bindings, defaulted): (&mut [Option<Argument>], &[usize]),
        packs: &[hir::TypePackArgument],
        argument: &dyn Fn(&Typer<'_>, usize, crate::laws::Law) -> Option<bool>,
    ) -> Option<bool> {
        let (_, condition) = required(self.program, target)?;
        self.apply_law_defaults(target, signature, defaulted, bindings, argument);
        let laws = crate::laws::verdicts(&condition, &|node| {
            self.law_answer(target, signature, node, argument)
        });
        let arguments: Vec<_> = signature
            .generics
            .iter()
            .zip(bindings.iter())
            .map(|(parameter, bound)| {
                bound.clone().unwrap_or_else(|| match parameter.kind {
                    GenericKind::Value(_) => Argument::Constant(unsaid()),
                    _ => Argument::Type(Type::Unknown),
                })
            })
            .collect();
        let mark = self.mark();
        let verdict = self.requires_told_with_laws(target, &arguments, packs, None, &laws);
        self.take_back(mark, false)
            .then_some(verdict)
            .flatten()
            .and_then(|(_, held, _)| held)
    }

    /// The same requirement with GEN-18 answers supplied by the call's
    /// static function arguments.
    pub(super) fn requires_told_with_laws(
        &mut self,
        (module, declaration): (usize, usize),
        generics: &[Argument],
        type_packs: &[hir::TypePackArgument],
        member_fault: Option<&std::cell::RefCell<Option<super::dependent_packs::MemberFault>>>,
        laws: &[(u32, Option<bool>)],
    ) -> Option<(SyntaxNode, Option<bool>, bool)> {
        let program = self.program;
        let (parameters, condition) = required(program, (module, declaration))?;
        let declared = &program.modules[module].declarations[declaration];
        // GEN-16: "a named operator requirement tests callability with
        // the supplied types."
        let verdicts = self.requirements_hold(declared, parameters, (module, generics), &condition);
        let callable = |node: &SyntaxNode| -> Option<bool> {
            let start = node.span().start;
            if let Some((_, answer)) = laws.iter().find(|(at, _)| *at == start) {
                return *answer;
            }
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
        // Each projection reads constants and then tests the resulting
        // types with the same typer. Those borrows do not overlap.
        let typer = RefCell::new(self);
        let constant = |node: &SyntaxNode, ty: Option<&Type>| -> Option<ConstantValue> {
            typer.borrow().read_in(declared.at.file, node, ty).ok()
        };
        let quantified = |call: &SyntaxNode, written: &RequirementType<'_>| -> Option<bool> {
            let file = declared.at.file;
            let (operator, operands, result) =
                read_requirement(program, &program.modules[module], file, call, written)?;
            typer
                .borrow_mut()
                .callable(operator, (file, operands), result.as_ref())
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
                quantified: &quantified,
                unvalued: &unvalued,
                type_packs,
                member_fault,
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
            .map(|call| read_requirement(program, declared_in, file, call, &written))
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
                            if !unmet_reported(self.diagnostics, at) {
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

    /// FMT-5 asks for the normal value conversion to `str` for a type
    /// that the native formatter does not directly format.
    pub(super) fn text_converter(&mut self, operand: SyntaxNode, ty: &Type) -> Option<Callee> {
        let program = self.program;
        let module = program.core()?;
        let core = &program.modules[module];
        let declaration = core
            .scope
            .get("convert")?
            .iter()
            .copied()
            .find(|&index| core.declarations[index].kind == DeclarationKind::Operator)?;
        let operator = OperatorRef {
            module,
            declaration,
        };
        let result = Type::Scalar(crate::Scalar::Str);
        let Answered::Selected {
            module,
            declaration,
            generics,
        } = self.answered(
            operator,
            (self.file, vec![(operand, ty.clone())]),
            Some(&result),
        )?
        else {
            return None;
        };
        let Detail::Callable(signature) = &program.modules[module].details[declaration] else {
            return None;
        };
        let given: Vec<_> = fixed(&signature.generics).zip(&generics).collect();
        let supplied = signature
            .parameters
            .iter()
            .find(|parameter| !parameter.configuration)?;
        // The formatter holds the whole argument and needs the text
        // itself. Ordinary deferred reads may admit either payload.
        if !self.admits(&substitute_within(&supplied.ty, &given), ty)
            || substitute_within(signature.result.as_ref()?, &given) != result
        {
            return None;
        }
        let requirements = self.answers(module, declaration, &generics);
        Some(Callee::Candidate {
            operator: (operator.module, operator.declaration),
            operator_generics: vec![
                Argument::Type(result.clone()),
                Argument::Type(ty.clone()),
                Argument::Type(result),
            ],
            operator_type_packs: Vec::new(),
            module,
            declaration,
            generics,
            type_packs: Vec::new(),
            requirements,
            test_candidates: self.test_candidates,
        })
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
            callback: None,
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
                operator_type_packs: Vec::new(),
                configuration,
            },
            arguments,
            form,
            text: None,
            braced: Vec::new(),
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

type RequirementType<'a> = dyn Fn(&SyntaxNode) -> Option<Type> + 'a;

/// Read a GEN-16 requirement using the types in scope at this occurrence.
fn read_requirement(
    program: &Program,
    declared_in: &crate::Module,
    file: FileId,
    call: &SyntaxNode,
    written: &RequirementType<'_>,
) -> Option<Read> {
    let name = name_token(call)?;
    let start = name.span().start;
    let used = declared_in
        .uses
        .iter()
        .find(|used| used.at.file == file && used.at.span.start == start)?;
    let Binding::Declarations {
        module,
        declarations,
    } = &used.binding
    else {
        return None;
    };
    let declaration = declarations.iter().copied().find(|&declaration| {
        program.modules[*module].declarations[declaration].kind == DeclarationKind::Operator
    })?;
    let mut operands = Vec::new();
    let mut result = None;
    for part in call.children() {
        match part.kind() {
            SyntaxKind::QualifiedName => {}
            SyntaxKind::ReturnType => result = Some(written(&part.children().next()?)?),
            // An indexed operand is a type only where the read table
            // resolved it to a pack member (GEN-17).
            kind if is_type(kind) || kind == SyntaxKind::IndexExpr => {
                let ty = written(&part)?;
                operands.push((part, ty));
            }
            _ => return None,
        }
    }
    Some((
        OperatorRef {
            module: *module,
            declaration,
        },
        operands,
        result,
    ))
}

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

/// A pack and key paired with the member types admitted for this trial.
type MemberChoices = ((Location, Constant), Vec<Type>);

/// A reflected member type stated by a positive requirement (GEN-17).
struct StatedPackMember {
    pack: Location,
    key: StatedPackKey,
    types: StatedMemberTypes,
}

enum StatedMemberTypes {
    Equal(Type),
    Alternatives(Vec<Type>),
}

impl StatedMemberTypes {
    fn values(&self) -> &[Type] {
        match self {
            Self::Equal(ty) => std::slice::from_ref(ty),
            Self::Alternatives(types) => types,
        }
    }
}

/// One declared key, or the key parameter of a universal requirement.
enum StatedPackKey {
    One(Constant),
    Every(Location),
}

/// The relations of a condition that hold wherever the condition does
/// (GEN-15).
fn conjoined_relations(node: &SyntaxNode, relations: &mut Vec<SyntaxNode>) {
    conjoined_terms(
        node,
        &|kind| kind == SyntaxKind::ConstraintRelation,
        relations,
    );
}

/// Select terms only from the condition itself and the sides of `&&`.
pub(super) fn conjoined_terms(
    node: &SyntaxNode,
    take: &dyn Fn(SyntaxKind) -> bool,
    terms: &mut Vec<SyntaxNode>,
) {
    match node.kind() {
        kind if take(kind) => terms.push(node.clone()),
        SyntaxKind::ConstraintParen => {
            if let Some(inner) = node.children().next() {
                conjoined_terms(&inner, take, terms);
            }
        }
        SyntaxKind::ConstraintBinary if has_token(node, SyntaxKind::AmpAmp) => {
            for side in node.children() {
                conjoined_terms(&side, take, terms);
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
    type_packs: Vec<hir::TypePackArgument>,
    constants: ConstArguments,
    /// What answers each requirement the callee states at this call
    /// (`hir::Answer`), with the call's generic arguments in place.
    answers: Vec<hir::Answer>,
    /// The module whose test candidates are in scope while making this call.
    test_candidates: Option<crate::TestCandidateScope>,
    /// For a call of an operation the enclosing declaration requires
    /// (`Callee::Required`), the requirement's position: what it calls
    /// is the candidate the instance's caller answers it with. `callee`,
    /// `generics` and `answers` say nothing then.
    required: Option<usize>,
    at: Location,
    /// A concrete forwarding path that exposed this call to the walk.
    via: Option<Rc<MakingPath>>,
}

type ConstArguments = crate::const_inputs::ReadInputs;

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
            Type::TypeValue
            | Type::Scalar(_)
            | Type::Parameter { .. }
            | Type::PackMember { .. }
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
    source: &IndependentSourceBodies,
    store_keys: &mut crate::store_keys::KeyUses,
    diagnostics: &mut Diagnostics,
) {
    let declared_reports = diagnostics.len();
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
    let mut independent_roots: HashMap<_, HashSet<_>> = HashMap::new();
    // The declarations whose body gives a callee a generic argument it
    // works out from its own generic parameters.
    let mut computing: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();
    let mut configured = std::collections::HashSet::new();
    let mut endpoint_reads = std::collections::HashSet::new();
    let mut dependent_indices = std::collections::HashSet::new();
    let mut dependent_patterns = std::collections::HashSet::new();
    for (index, module) in program.modules.iter().enumerate() {
        for position in 0..module.declarations.len() {
            let mut made = Vec::new();
            let mut forwarding = false;
            if let Some(Some(body)) = module.bodies.get(position) {
                // FN-4/17: reaching the graph with a concrete callback
                // discovers the pair and consumers of its result. Its
                // provisional source role cannot supply either root.
                let roots = if program.callback_phase_bodies.contains(&(index, position)) {
                    source.body((index, position)).unwrap_or(body)
                } else {
                    body
                };
                made_in(program, roots, &mut made);
                forwarding = super::has_forwarded_calls(body);
            }
            let body_calls = made.len();
            let independent = source.body((index, position)).map(|body| {
                let mut calls = Vec::new();
                made_in(program, body, &mut calls);
                calls.iter().map(source_call_key).collect::<HashSet<_>>()
            });
            for default in module
                .defaults
                .get(position)
                .into_iter()
                .flatten()
                .flatten()
            {
                if module.declarations[position].declares_default(default) {
                    made_by(program, &default.value, &mut made);
                }
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
            if super::node_endpoint_reads(module, position, None) {
                endpoint_reads.insert((index, position));
            }
            if !declared.is_empty()
                && module
                    .bodies
                    .get(position)
                    .and_then(Option::as_ref)
                    .is_some_and(super::has_dependent_index)
            {
                dependent_indices.insert((index, position));
            }
            if !declared.is_empty()
                && module
                    .bodies
                    .get(position)
                    .and_then(Option::as_ref)
                    .is_some_and(|body| {
                        crate::formatting::has_dependent_formatting(
                            program,
                            (index, position),
                            body,
                        )
                    })
            {
                dependent_patterns.insert((index, position));
            }
            for (call_index, call) in made.into_iter().enumerate() {
                // FN-19 and GEN-7: in a body that forwards packs, a call
                // given an entry not yet read has a type no instance has.
                // Each instance reads the entry, and the forwarding walk
                // finds the call there with its type.
                if forwarding && call_index < body_calls && reads_pending_entry(&call) {
                    continue;
                }
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
                let needs_instance = call_index < body_calls
                    && independent
                        .as_ref()
                        .is_some_and(|calls| !calls.contains(&source_call_key(&call)));
                if call.required.is_some()
                    || needs_instance
                    || passes_on(&call.generics, &declared)
                    || call
                        .type_packs
                        .iter()
                        .any(|pack| pack.forwarded_from.is_some())
                {
                    passed.entry((index, position)).or_default().push(call);
                } else {
                    let checks_const_inputs = call.constants.limit.is_some()
                        || call.constants.outgrown
                        || !call.constants.values.is_empty();
                    // A concrete reread checks the calls whose inputs changed.
                    // This call already has its own independent checking root.
                    if call_index < body_calls && (independent.is_some() || checks_const_inputs) {
                        independent_roots
                            .entry((index, position))
                            .or_default()
                            .insert(source_call_key(&call));
                    }
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
            made_in(program, body, &mut given);
        }
        if let Some(run) = &module.run_body {
            made_by(program, &run.call, &mut given);
        }
        for constant in module.constants.iter().flatten() {
            made_by(program, &constant.value, &mut given);
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
        .chain(endpoint_reads.iter().copied())
        .chain(dependent_indices.iter().copied())
        .chain(dependent_patterns.iter().copied())
        .chain(program.const_demands.keys().copied())
        .chain(program.generic_store_bodies.iter().copied())
        .chain(crate::type_values::instance_declarations(program))
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
    let test_body = |(module, declaration): (usize, usize), test_candidates: Option<_>| {
        test_candidates.is_some()
            && program.modules[module]
                .bodies
                .get(declaration)
                .is_some_and(Option::is_some)
    };
    let checks_instance = |callee, generics: &[Argument], test_candidates| {
        reaching.contains(&callee)
            || test_body(callee, test_candidates)
            || generics.contains(&Argument::NoResult)
    };
    // The instances the calls that give every argument make, each once
    // with every call it is made from: two calls of `wrap<1>` are one
    // instance of it.
    let mut roots: Vec<(Made, Vec<InstanceRoot>)> = Vec::new();
    let mut root_of: HashMap<String, usize> = HashMap::new();
    for mut made in given
        .into_iter()
        .filter(|made| checks_instance(made.callee, &made.generics, made.test_candidates))
    {
        // A root whose input could not be read can still check the inputs
        // already known. Only exhaustion while following instances is a limit.
        let key = instance_key(&made);
        let from = InstanceRoot {
            at: made.at,
            via: made.via.take(),
        };
        match root_of.get(&key) {
            Some(&root) => roots[root].1.push(from),
            None => {
                root_of.insert(key, roots.len());
                roots.push((made, vec![from]));
            }
        }
    }
    if roots.is_empty() {
        return;
    }
    let tables: Vec<_> = program.modules.iter().map(super::tables).collect();
    let empty = Signature {
        generics: Vec::new(),
        parameters: Vec::new(),
        result: None,
        reference: false,
    };
    let mut rejected_calls = InstanceCallRejections::default();
    // GEN-7 reports a rejection once at each call written outside every
    // generic body. An instance a walk has told for one of those calls is
    // not told again for it: a call found inside a forwarding instance
    // starts a walk of its own that its root's walk can repeat. So each
    // instance a walk follows carries the root's calls not yet told it.
    let mut told = HashSet::<(String, Location)>::new();
    for (root, all_calls) in roots {
        let root_key = instance_key(&root);
        let untold = |told: &HashSet<(String, Location)>, key: &String, calls: &[usize]| {
            calls
                .iter()
                .copied()
                .filter(|&call| !told.contains(&(key.clone(), all_calls[call].at)))
                .collect::<Vec<_>>()
        };
        let root_untold = untold(&told, &root_key, &(0..all_calls.len()).collect::<Vec<_>>());
        if root_untold.is_empty() {
            continue;
        }
        // Each instance this one makes followed once, by what it is: its
        // callee and its arguments, the root's own among them.
        let mut seen: std::collections::HashSet<String> =
            std::collections::HashSet::from([root_key]);
        let mut waiting = std::collections::VecDeque::from([(root, None, root_untold)]);
        let mut parents = Vec::new();
        // How many instances of each declaration this one has made.
        let mut followed: HashMap<(usize, usize), usize> = HashMap::new();
        let mut exhausted = false;
        while let Some((made, parent, making)) = waiting.pop_front() {
            let root_calls: Vec<InstanceRoot> =
                making.iter().map(|&call| all_calls[call].clone()).collect();
            let from: Vec<_> = root_calls.iter().map(|call| call.at).collect();
            let key = instance_key(&made);
            told.extend(from.iter().map(|&at| (key.clone(), at)));
            let count = followed.entry(made.callee).or_insert(0);
            *count += 1;
            if *count > INSTANCES {
                past_the_limit(&from, made.callee, program, diagnostics);
                break;
            }
            let inner = passed.get(&made.callee).map_or(&[][..], Vec::as_slice);
            let selecting = program.modules.get(made.callee.0).map_or(&[][..], |unit| {
                unit.selecting
                    .get(made.callee.1)
                    .map_or(&[][..], Vec::as_slice)
            });
            let unit = &program.modules[made.callee.0];
            let graph = unit.phases.get(made.callee.1) == Some(&Some(Phase::Graph));
            let recheck_tests = test_body(made.callee, made.test_candidates);
            let recheck_graph =
                graph && (recheck_tests || !selecting.is_empty() || !made.answers.is_empty());
            let recheck_results = selecting.iter().any(|call| call.result.is_some());
            let recheck_indices = dependent_indices.contains(&made.callee);
            let constants = made.constants.values.as_slice();
            let recheck_values = dependent_patterns.contains(&made.callee)
                || !constants.is_empty()
                || program.generic_store_bodies.contains(&made.callee)
                || crate::type_values::has_instance_query(program, made.callee);
            let recheck_resultless = made.generics.contains(&Argument::NoResult);
            let declaration = &unit.declarations[made.callee.1];
            if recheck_resultless
                && diagnostics.as_slice()[..declared_reports]
                    .iter()
                    .any(|report| {
                        let span = declaration.syntax.span();
                        report.severity == Severity::Error
                            && report.primary.file == declaration.at.file
                            && (span.start..span.end).contains(&report.primary.span.start)
                    })
            {
                // ERR-9: the declaration already rejects. Do not
                // report a second reading of its resultless instance.
                continue;
            }
            // GEN-17: rechecking a reflected result's consumers needs the
            // actual members of the pack supplied by the making call.
            let inputs = crate::InstanceInputs {
                type_packs: &made.type_packs,
                constants,
                test_candidates: made.test_candidates,
                ..crate::InstanceInputs::default()
            };
            let declared = own(made.callee);
            let map: Vec<(Location, &Argument)> =
                declared.iter().copied().zip(&made.generics).collect();
            let chain = InstanceChain {
                made: &made,
                from: &root_calls,
                parent,
                parents: &parents,
            };
            let name_reports =
                crate::type_values::instance_parameter_names(program, made.callee, &made.generics);
            if !name_reports.is_empty() {
                let paths = chain.paths(program, &root_calls);
                for mut report in name_reports {
                    // TYPE-18 requires the declared name as primary, even
                    // when GEN-13 determines its kind at a making call.
                    for call in paths.iter().flatten() {
                        let label = format!("this call makes the instance `{}`", call.instance);
                        if !report
                            .related
                            .iter()
                            .any(|related| related.location == call.at && related.message == label)
                        {
                            report = report.with_related(call.at, label);
                        }
                    }
                    if !diagnostics.as_slice().contains(&report) {
                        diagnostics.emit(report);
                    }
                }
                continue;
            }
            let enclosing_packs = match &program.modules[made.callee.0].details[made.callee.1] {
                Detail::Callable(signature) => &signature.generics[..],
                Detail::Struct(read) => &read.generics[..],
                _ => &[],
            };
            let enclosing_packs =
                super::dependent_packs::PackTypes::new(enclosing_packs, &made.type_packs);
            let mut delta_reports = Diagnostics::new();
            crate::type_values::instance_deltas(
                program,
                made.callee,
                |ty| enclosing_packs.project(ty, &map).ok(),
                &mut delta_reports,
            );
            if delta_reports.has_errors() {
                let paths = chain.paths(program, &root_calls);
                for report in delta_reports.as_slice() {
                    emit_instance_report(report.clone(), &paths, diagnostics);
                }
                continue;
            }
            let recheck_endpoints = endpoint_reads.contains(&made.callee)
                && matches!(
                    unit.details.get(made.callee.1),
                    Some(Detail::Callable(signature)) if super::node_endpoint_reads(
                        unit,
                        made.callee.1,
                        Some(&super::candidates::in_instance(signature, &map)),
                    )
                );
            let mut body_reports = Diagnostics::new();
            let mut instance_calls = Vec::new();
            let instance = if configured.contains(&made.callee) {
                super::operator_defaults::check_instance(
                    program,
                    made.callee,
                    (&made.generics, &made.answers),
                    inputs,
                    &mut instance_calls,
                    diagnostics,
                    &mut body_reports,
                )
            } else if recheck_graph
                || recheck_endpoints
                || recheck_results
                || recheck_indices
                || recheck_values
                || recheck_tests
                || recheck_resultless
            {
                program.checked_instance(
                    made.callee,
                    &made.generics,
                    &made.answers,
                    inputs,
                    &RefCell::new(Diagnostics::new()),
                    &mut body_reports,
                )
            } else {
                None
            };
            if let Some(instance) = &instance
                && let Some(result_uses) = &instance.result_uses
            {
                if !result_uses.reports.is_empty() {
                    report_instance_result_uses(
                        program,
                        (made.callee, &made.generics, &made.type_packs),
                        result_uses,
                        &chain.paths(program, &root_calls),
                        diagnostics,
                    );
                }
                if result_uses.reports.has_errors() {
                    continue;
                }
            }
            let mut replaced = HashSet::new();
            let reread_body = instance
                .as_ref()
                .is_some_and(|instance| instance.reread_body);
            if let Some(instance) = &instance
                && let Some(Some(declared_body)) = unit.bodies.get(made.callee.1)
            {
                if reread_body || source.body(made.callee).is_some() {
                    super::walk_body_calls(declared_body, &mut |call| {
                        replaced.insert(call.at);
                    });
                    concrete_body_calls(
                        program,
                        instance,
                        independent_roots.get(&made.callee),
                        &mut instance_calls,
                    );
                } else if recheck_graph || recheck_indices {
                    made_after_inference(program, declared_body, instance, &mut instance_calls);
                }
            }
            if (recheck_indices || recheck_values || recheck_tests || recheck_resultless)
                && !recheck_graph
                && !recheck_endpoints
            {
                let paths = chain.paths(program, &root_calls);
                for report in body_reports.as_slice() {
                    if (recheck_values || recheck_tests) && diagnostics.as_slice().contains(report)
                    {
                        continue;
                    }
                    emit_instance_report(report.clone(), &paths, diagnostics);
                }
                if body_reports.has_errors() {
                    // An ill-typed value body does not make calls its
                    // reread could only partially type (ERR-9).
                    continue;
                }
            }
            if recheck_graph && let Some(instance) = &instance {
                let constants = super::lifting::instance_constants(program, &instance.body);
                for (reports, message) in [
                    (&constants, "this place in the generic body needs a value"),
                    (
                        &body_reports,
                        "this destination in the generic graph is checked for this instance",
                    ),
                ] {
                    let mut elsewhere = Diagnostics::new();
                    for report in reports.as_slice() {
                        if diagnostics.as_slice().contains(report) {
                            continue;
                        }
                        if let Some(call) = selected_call_for_connection(selecting, report) {
                            rejected_calls.record(
                                program,
                                &chain,
                                call,
                                vec![report.clone()],
                                diagnostics,
                            );
                        } else {
                            elsewhere.emit(report.clone());
                        }
                    }
                    super::lifting::report_instance_checks(&elsewhere, &from, diagnostics, message);
                }
            }
            if recheck_endpoints {
                super::lifting::report_instance_checks(
                    &body_reports,
                    &from,
                    diagnostics,
                    "this endpoint read in the generic hook is checked for this instance",
                );
            }
            if let Some(instance) = &instance
                && !body_reports.has_errors()
            {
                store_keys.in_instance(program, &instance.body, made.at);
            }
            if inner.is_empty() && selecting.is_empty() && instance_calls.is_empty() {
                continue;
            }
            // What each call of the instance calls, with what it gives
            // the callee's generic parameters and what answers the
            // callee's requirements there, and where it is written.
            let mut calls: Vec<Resolved> = instance_calls
                .into_iter()
                .map(|call| Resolved {
                    callee: call.callee,
                    generics: call.generics,
                    type_packs: call.type_packs,
                    constants: call.constants,
                    forwarded: false,
                    answers: call.answers,
                    test_candidates: call.test_candidates,
                    at: call.at,
                })
                .collect();
            for call in inner.iter().filter(|call| !replaced.contains(&call.at)) {
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
                            .map(|argument| given_in(program, argument, &map, &declared))
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
                                .map(|answer| {
                                    answered_in(program, answer, &map, &declared, &made.answers)
                                })
                                .collect(),
                        )
                    }
                };
                let mut type_packs = call.type_packs.clone();
                let forwarded = type_packs.iter().any(|pack| pack.forwarded_from.is_some());
                for pack in &mut type_packs {
                    pack.place_types(|ty| placed(substitute_within(ty, &map), &declared));
                    enclosing_packs.place_forwarded(pack);
                }
                calls.push(Resolved {
                    callee,
                    generics,
                    type_packs,
                    constants: call.constants.clone(),
                    forwarded,
                    answers,
                    test_candidates: call.test_candidates.or(made.test_candidates),
                    at: call.at,
                });
            }
            // GEN-5: a call whose candidate a generic parameter of the
            // body selects is selected here, with this instance's types
            // ([`Typer::select_at_instance`]).
            if !reread_body && !selecting.is_empty() {
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
                typer.test_candidates = made.test_candidates.or(typer.test_candidates);
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
                        typer.use_instance_locals(
                            locals
                                .iter()
                                .map(|local| hir::Local {
                                    ty: placed(substitute_within(&local.ty, &map), &declared),
                                    ..local.clone()
                                })
                                .collect(),
                        );
                        current_locals = Some(locals);
                    }
                    let mut rejected =
                        |typer: &mut super::Typer, reports: Vec<hgl_diagnostics::Diagnostic>| {
                            rejected_calls.record(
                                program,
                                &chain,
                                call.name,
                                reports,
                                typer.diagnostics,
                            );
                        };
                    // ERR-9: "Failure during argument evaluation precedes
                    // later admission checks."
                    if let Some(report) = typer.option_fails(call) {
                        rejected(&mut typer, vec![report]);
                        continue;
                    }
                    match typer.select_at_instance(call, &enclosing_packs, None) {
                        AtInstance::Selected(chosen, given) => {
                            let (module, declaration, generics) =
                                (chosen.module, chosen.declaration, chosen.generics.clone());
                            let type_packs = chosen.type_packs.clone();
                            // What the call is held to once its candidate
                            // is selected (`Typer::called`): a `var`
                            // parameter's place among them (FN-15).
                            let mark = typer.diagnostics.len();
                            let (kind, ty) = typer.called(
                                call.operator,
                                *chosen,
                                given,
                                Vec::new(),
                                (call.form, call.name),
                            );
                            let constants = match &kind {
                                ExprKind::Call {
                                    callee, arguments, ..
                                } => program.known_const_inputs(callee, arguments),
                                _ => ConstArguments::default(),
                            };
                            typer.fit_selected_result(
                                call,
                                Expr {
                                    kind,
                                    ty,
                                    at: call.at,
                                    category: ValueCategory::Unsettled,
                                },
                                |ty| placed(substitute_within(ty, &map), &declared),
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
                                type_packs,
                                constants,
                                forwarded: false,
                                answers,
                                test_candidates: typer.test_candidates,
                                at: call.at,
                            });
                        }
                        // GEN-5: "No match rejects; tied most-specific
                        // candidates are ambiguous."
                        AtInstance::Rejected(report) => rejected(&mut typer, vec![report]),
                        AtInstance::Untold => {}
                    }
                }
            }
            let mut link = None;
            for Resolved {
                callee,
                generics,
                type_packs,
                constants,
                forwarded,
                answers,
                test_candidates,
                at,
            } in calls
            {
                let Some(key) = bounded_instance_key(callee, &generics, &type_packs, &answers)
                else {
                    exhausted = true;
                    break;
                };
                // Traversal budgets leave an input unread. A concrete text
                // that outgrows its argument budget exhausts this walk.
                if constants.outgrown {
                    exhausted = true;
                    break;
                }
                let key = with_test_candidates(with_const_inputs(key, &constants), test_candidates);
                // GEN-7: a call of a function or a candidate in a generic
                // body was judged where the body is declared, under what
                // it states. Struct applications and forwarded parameter
                // packs are told here, where their arguments are known.
                let applied = program.modules[callee.0].declarations[callee.1].kind
                    == DeclarationKind::Struct;
                if forwarded || applied && required(program, callee).is_some() {
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
                    let before = typer.diagnostics.len();
                    for &root in &from {
                        if forwarded {
                            let reported = typer.diagnostics.len();
                            typer.check_forwarded_packs(root, callee, &generics, &type_packs);
                            if typer.diagnostics.len() == reported {
                                typer.pack_requires_met(root, callee, &generics, &type_packs);
                            }
                        } else {
                            typer.instance_met((root, at), callee, &generics);
                        }
                    }
                    if forwarded && typer.diagnostics.len() != before {
                        continue;
                    }
                }
                let child_calls = untold(&told, &key, &making);
                if checks_instance(callee, &generics, test_candidates)
                    && !child_calls.is_empty()
                    && seen.insert(key)
                {
                    let caller = *link.get_or_insert_with(|| {
                        let index = parents.len();
                        parents.push((
                            MakingCall::new(
                                program,
                                made.callee,
                                &made.generics,
                                &made.type_packs,
                                made.at,
                            ),
                            parent,
                        ));
                        index
                    });
                    waiting.push_back((
                        Made {
                            callee,
                            generics,
                            type_packs,
                            constants,
                            answers,
                            test_candidates,
                            required: None,
                            at,
                            via: None,
                        },
                        Some(caller),
                        child_calls,
                    ));
                }
            }
            if exhausted {
                past_the_limit(&from, made.callee, program, diagnostics);
                break;
            }
        }
    }
    rejected_calls.emit(diagnostics);
}

/// Whether a call's generic arguments hold a type not yet known: in a
/// forwarding body, that of a pack entry read before the entries are.
fn reads_pending_entry(call: &Made) -> bool {
    let unknown = |ty: &Type| ty.any_part(&|part| matches!(part, Type::Unknown), &|_| false);
    call.generics
        .iter()
        .any(|argument| matches!(argument, Argument::Type(ty) if unknown(ty)))
        || call
            .type_packs
            .iter()
            .flat_map(hir::TypePackArgument::types_with_prefix)
            .any(unknown)
}

/// One call in a chain of concrete instances, shown with the instance it makes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct MakingCall {
    at: Location,
    instance: String,
}

/// A non-generic making site and the concrete forwarding path it exposes.
#[derive(Clone)]
struct InstanceRoot {
    at: Location,
    via: Option<Rc<MakingPath>>,
}

/// The calls from an instance out to the call written outside every
/// generic body, innermost first (GEN-7). The calls one instance makes
/// share its path, so a walk keeps one call for each instance it makes.
#[derive(Debug)]
pub(super) struct MakingPath {
    call: MakingCall,
    outer: Option<Rc<MakingPath>>,
}

impl MakingPath {
    /// `call`, made inside the instance whose path is `outer`.
    pub(super) fn new(call: MakingCall, outer: Option<Rc<MakingPath>>) -> Rc<Self> {
        Rc::new(Self { call, outer })
    }

    /// Each call on the path, from this one out.
    pub(super) fn calls(&self) -> impl Iterator<Item = &MakingCall> {
        std::iter::successors(Some(self), |path| path.outer.as_deref()).map(|path| &path.call)
    }
}

/// A long path is let go one call at a time, and not by a call for each.
impl Drop for MakingPath {
    fn drop(&mut self) {
        let mut outer = self.outer.take();
        while let Some(path) = outer {
            // `Rc::try_unwrap` takes the path where nothing else holds it.
            outer = Rc::try_unwrap(path)
                .ok()
                .and_then(|mut path| path.outer.take());
        }
    }
}

impl MakingCall {
    pub(super) fn new(
        program: &Program,
        callee: (usize, usize),
        generics: &[Argument],
        packs: &[hir::TypePackArgument],
        at: Location,
    ) -> Self {
        Self {
            at,
            instance: instance_name(program, callee, generics, packs),
        }
    }
}

/// Walk the saved parent indices from the innermost call to the root.
pub(super) fn making_path<'a>(
    current: MakingCall,
    mut parent: Option<usize>,
    parent_at: impl Fn(usize) -> &'a (MakingCall, Option<usize>),
) -> Vec<MakingCall> {
    let mut path = vec![current];
    while let Some(index) = parent {
        let (caller, previous) = parent_at(index);
        path.push(caller.clone());
        parent = *previous;
    }
    path
}

/// The route to this instance, recorded as parent links in the bounded walk.
struct InstanceChain<'a> {
    made: &'a Made,
    from: &'a [InstanceRoot],
    parent: Option<usize>,
    parents: &'a [(MakingCall, Option<usize>)],
}

impl InstanceChain<'_> {
    /// One path per supplying call, starting at the innermost instance.
    fn paths<'a>(
        &self,
        program: &Program,
        from: impl IntoIterator<Item = &'a InstanceRoot>,
    ) -> Vec<Vec<MakingCall>> {
        let current = MakingCall::new(
            program,
            self.made.callee,
            &self.made.generics,
            &self.made.type_packs,
            self.made.at,
        );
        let path = making_path(current, self.parent, |index| &self.parents[index]);
        let (root, inner) = path
            .split_last()
            .expect("the path contains its current call");
        from.into_iter()
            .map(|from| {
                let mut path = inner.to_vec();
                match &from.via {
                    Some(via) => path.extend(via.calls().cloned()),
                    None => path.push(MakingCall {
                        at: from.at,
                        instance: root.instance.clone(),
                    }),
                }
                path
            })
            .collect()
    }
}

/// Report the instance's rejection once at each outer making call.
/// Keep the failing body location and the intermediate calls beside it.
pub(super) fn emit_instance_report(
    report: Diagnostic,
    paths: &[Vec<MakingCall>],
    diagnostics: &mut Diagnostics,
) {
    let mut groups = HashMap::<Location, Vec<&[MakingCall]>>::new();
    for path in paths {
        if let Some(outer) = path.last() {
            groups.entry(outer.at).or_default().push(path);
        }
    }
    let mut groups: Vec<_> = groups.into_iter().collect();
    groups.sort_by_key(|(at, _)| (at.file, at.span.start, at.span.end));
    for (at, paths) in groups {
        let mut here = report
            .clone()
            .with_related(report.primary, "the instance's body rejects here");
        here.primary = at;
        for call in paths.into_iter().flatten() {
            let message = format!("this call makes the instance `{}`", call.instance);
            if call.at == at {
                if !here.notes.contains(&message) {
                    here = here.with_note(message);
                }
            } else if !here
                .related
                .iter()
                .any(|label| label.location == call.at && label.message == message)
            {
                here = here.with_related(call.at, message);
            }
        }
        if !diagnostics.as_slice().contains(&here) {
            diagnostics.emit(here);
        }
    }
}

/// Gather the paths that make each instance and give it this rejection.
#[derive(Default)]
struct InstanceCallRejections {
    by_instance: HashMap<String, Vec<usize>>,
    reports: Vec<(Diagnostic, Vec<Vec<MakingCall>>)>,
}

impl InstanceCallRejections {
    fn record(
        &mut self,
        program: &Program,
        chain: &InstanceChain,
        call: Location,
        reports: Vec<Diagnostic>,
        diagnostics: &Diagnostics,
    ) {
        if reports.is_empty() {
            return;
        }
        // A call rejected for a `requires` makes no instance to judge
        // further (ERR-9: "Do not demand cascaded diagnostics").
        let callers: Vec<_> = chain
            .from
            .iter()
            .filter(|root| !unmet_reported(diagnostics, root.at))
            .collect();
        if callers.is_empty() {
            return;
        }
        let made = chain.made;
        let key = instance_key(made);
        let reports_of = self.by_instance.entry(key).or_default();
        let instance = instance_note(program, made.callee, &made.generics, &made.type_packs);
        let paths = chain.paths(program, callers);
        for report in reports {
            let mut report = report.with_note(&instance);
            if report.primary != call {
                let within = report.primary;
                report.primary = call;
                report = report.with_related(within, "here, in that call");
            }
            if let Some(index) = reports_of
                .iter()
                .copied()
                .find(|&index| self.reports[index].0 == report)
            {
                self.reports[index].1.extend_from_slice(&paths);
            } else {
                reports_of.push(self.reports.len());
                self.reports.push((report, paths.clone()));
            }
        }
    }

    fn emit(self, diagnostics: &mut Diagnostics) {
        for (report, paths) in self.reports {
            emit_instance_report(report, &paths, diagnostics);
        }
    }
}

/// An argument or setting that became a connection can point at the
/// selected call that supplies it. Other graph checks keep their own
/// destinations.
fn selected_call_for_connection(
    selecting: &[super::candidates::Selecting],
    report: &Diagnostic,
) -> Option<Location> {
    if report.code != Code::Catalogued(codes::CONNECTION_AS_CONSTANT)
        || !matches!(report.rule, Some("FN-3" | "FN-15" | "EXPR-14"))
    {
        return None;
    }
    selecting
        .iter()
        .filter(|call| contains_location(call.at, report.primary))
        .min_by_key(|call| call.at.span.end - call.at.span.start)
        .map(|call| call.name)
}

/// True where a `requires.unmet` has been reported at `at` already.
fn unmet_reported(diagnostics: &Diagnostics, at: Location) -> bool {
    diagnostics.as_slice().iter().any(|earlier| {
        earlier.code == Code::Catalogued(codes::REQUIRES_UNMET)
            && earlier.primary.file == at.file
            && earlier.primary.span.start == at.span.start
    })
}

/// What an instance is, written out: its declaration, its generic
/// arguments and what answers its requirements. A constant argument is
/// its value however it is written, so `turn<0>` and the `turn<((N + 1)
/// % 2)>` that comes back to it are one instance.
fn instance_key(made: &Made) -> String {
    let key = generic_instance_key(made.callee, &made.generics, &made.type_packs, &made.answers);
    with_test_candidates(
        with_const_inputs(key, &made.constants),
        made.test_candidates,
    )
}

fn with_test_candidates(key: String, test_candidates: Option<crate::TestCandidateScope>) -> String {
    match test_candidates {
        Some(scope) => format!("{key}:test candidates[{}]", scope.module()),
        None => key,
    }
}

/// The generic part of an instance's identity, within the GEN-13 walk's bounds.
pub(crate) fn bounded_instance_key(
    callee: (usize, usize),
    generics: &[Argument],
    type_packs: &[hir::TypePackArgument],
    answers: &[hir::Answer],
) -> Option<String> {
    if generics
        .iter()
        .any(|argument| matches!(argument, Argument::Type(ty) if nesting(ty) > NESTING))
        || type_packs
            .iter()
            .flat_map(hir::TypePackArgument::types_with_prefix)
            .any(|ty| nesting(ty) > NESTING)
    {
        return None;
    }
    let key = generic_instance_key(callee, generics, type_packs, answers);
    (key.len() <= ARGUMENTS).then_some(key)
}

fn generic_instance_key(
    callee: (usize, usize),
    generics: &[Argument],
    type_packs: &[hir::TypePackArgument],
    answers: &[hir::Answer],
) -> String {
    let generics: Vec<String> = generics
        .iter()
        .map(|argument| match argument {
            Argument::Constant(constant) => format!("{:?}", constant.value),
            Argument::Type(ty) => format!("{:?}", by_value(ty)),
            Argument::NoResult => "no result".to_string(),
        })
        .collect();
    let packs = type_packs
        .iter()
        .map(|pack| {
            let reflected = pack.reflected.as_ref().map(|inputs| {
                let prefix = inputs
                    .prefix
                    .iter()
                    .map(|(key, ty)| (key, by_value(ty)))
                    .collect::<Vec<_>>();
                (inputs.fixed, prefix)
            });
            (
                pack.parameter,
                &pack.keys,
                pack.types.iter().map(by_value).collect::<Vec<_>>(),
                reflected,
            )
        })
        .collect::<Vec<_>>();
    format!("{callee:?}{generics:?}{packs:?}{answers:?}")
}

fn with_const_inputs(mut key: String, constants: &ConstArguments) -> String {
    use std::fmt::Write;

    for input in &constants.values {
        let _ = write!(
            key,
            "const[{}]:{}:{}",
            input.slot,
            input.key.len(),
            input.key
        );
    }
    key
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
                    Argument::NoResult => Argument::NoResult,
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
    type_packs: Vec<hir::TypePackArgument>,
    constants: ConstArguments,
    forwarded: bool,
    answers: Vec<hir::Answer>,
    test_candidates: Option<crate::TestCandidateScope>,
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
        Argument::Constant(_) | Argument::NoResult => Vec::new(),
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
                Argument::Type(_) | Argument::NoResult => None,
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
                Argument::NoResult => continue,
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
pub(super) fn past_the_limit(
    from: &[Location],
    (module, declaration): (usize, usize),
    program: &Program,
    diagnostics: &mut Diagnostics,
) {
    let name = &program.modules[module].declarations[declaration].name;
    let arguments = if program.needs_const_inputs((module, declaration)) {
        "compile-time arguments"
    } else {
        "generic arguments"
    };
    instance_limit(
        from,
        format!(
            "the instances of `{name}` this call makes do not end within the \
             {INSTANCES} this compiler follows, with {arguments} no longer than \
             {ARGUMENTS} characters written out and types in them no deeper than \
             {NESTING}: the `requires` and the {arguments} of those past them are \
             not told"
        ),
        diagnostics,
    );
}

/// Report the GEN-13 bound on pack shapes that keep growing along one call path.
pub(super) fn past_pack_growth_limit(
    at: Location,
    (module, declaration): (usize, usize),
    program: &Program,
    growth: usize,
    diagnostics: &mut Diagnostics,
) {
    let name = &program.modules[module].declarations[declaration].name;
    instance_limit(
        &[at],
        format!(
            "the instances of `{name}` this call makes keep adding pack entries beyond the \
             {growth} increases this compiler follows"
        ),
        diagnostics,
    );
}

fn instance_limit(from: &[Location], message: String, diagnostics: &mut Diagnostics) {
    for &at in from {
        let rejected = diagnostics.as_slice().iter().any(|earlier| {
            earlier.code == Code::Catalogued(codes::REQUIRES_UNMET)
                && earlier.primary.file == at.file
                && earlier.primary.span.start == at.span.start
        });
        // GEN-13 catalogues the code, proposed, for instances past
        // the implementation's bound.
        let report = crate::error(
            Category::Type,
            codes::INSTANCE_LIMIT,
            at,
            message.clone(),
            "GEN-13",
        );
        // GEN-13 and ERR-9: the call that makes the first of them rejects
        // once; a second walk that meets a bound there adds nothing.
        let limited = diagnostics.as_slice().iter().any(|earlier| {
            earlier.code == Code::Catalogued(codes::INSTANCE_LIMIT) && earlier.primary == at
        });
        if !rejected && !limited {
            diagnostics.emit(report);
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
                type_packs: Vec::new(),
                constants: ConstArguments::default(),
                answers: Vec::new(),
                test_candidates: None,
                required: None,
                at,
                via: None,
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
            Argument::NoResult => false,
            Argument::Type(ty) => ty.any_part(
                &|part| matches!(part, Type::Parameter { at, .. } if declared.contains(at)),
                &|constant| constant.value.names_one_of(declared),
            ),
        })
}

/// True where a written argument reflects a member of a named pack
/// (GEN-17). Source requirement checking can read its stated type.
fn reflects(generics: &[Argument], packs: &[Location]) -> bool {
    generics.iter().any(|argument| match argument {
        Argument::Constant(_) | Argument::NoResult => false,
        Argument::Type(ty) => ty.any_part(
            &|part| matches!(part, Type::PackMember { pack, .. } if packs.contains(pack)),
            &|_| false,
        ),
    })
}

/// An inferred pack can pass on a type of the enclosing generic body too.
fn packs_pass_on(packs: &[hir::TypePackArgument], declared: &[Location]) -> bool {
    packs
        .iter()
        .flat_map(hir::TypePackArgument::types_with_prefix)
        .any(|ty| mentions(ty, declared))
}

/// `argument`, a generic argument a call in a generic body gives, with
/// what a call of that body gives its parameters, `map`, in place.
fn given_in(
    program: &Program,
    argument: &Argument,
    map: &[(Location, &Argument)],
    declared: &[Location],
) -> Argument {
    match argument {
        Argument::Constant(constant) => Argument::Constant(Constant {
            text: constant.text.clone(),
            value: program.specialize_constant(&constant.value, map),
        }),
        Argument::Type(ty) => Argument::Type(placed(program.specialize_type(ty, map), declared)),
        Argument::NoResult => Argument::NoResult,
    }
}

/// The calls of generic declarations `statements` make, at any depth.
fn made_in(program: &Program, body: &hir::Body, made: &mut Vec<Made>) {
    super::walk_body_calls(body, &mut |expression| made_at(program, expression, made));
}

/// The body as declared, read with provisional operator and pack results
/// unknown. Calls whose arguments still resolve can be checked at their
/// written sites, including in a generic declaration that is never made.
pub(super) struct IndependentSourceBodies(Vec<Vec<Option<hir::Body>>>);

impl IndependentSourceBodies {
    pub(super) fn body(&self, (module, declaration): (usize, usize)) -> Option<&hir::Body> {
        self.0.get(module)?.get(declaration)?.as_ref()
    }
}

pub(super) fn independent_source_bodies(program: &Program) -> IndependentSourceBodies {
    IndependentSourceBodies(
        program
            .modules
            .iter()
            .enumerate()
            .map(|(index, module)| {
                module
                    .bodies
                    .iter()
                    .zip(&module.selecting)
                    .enumerate()
                    .map(|(position, (body, selecting))| {
                        let body = body.as_ref()?;
                        let mut unsettled: HashSet<_> = selecting
                            .iter()
                            .filter(|call| call.result.is_some())
                            .map(|call| call.at)
                            .collect();
                        super::walk_body_calls(body, &mut |value| {
                            if matches!(called_or_referenced(value), Some(Callee::DeferredPackCall))
                            {
                                unsettled.insert(value.at);
                            }
                        });
                        if unsettled.is_empty()
                            && !program.callback_phase_bodies.contains(&(index, position))
                        {
                            return None;
                        }
                        super::independent_source_body(program, module, position, &unsettled)
                    })
                    .collect()
            })
            .collect(),
    )
}

fn source_call_key(call: &Made) -> (Location, String) {
    (call.at, instance_key(call))
}

/// GEN-7: a failure in a selected result's use points at its call and
/// retains the ordinary diagnostic code and the concrete instance.
pub(super) fn report_instance_result_uses(
    program: &Program,
    ((module, declaration), generics, packs): (
        (usize, usize),
        &[Argument],
        &[hir::TypePackArgument],
    ),
    result_uses: &crate::instance::ResultUseChecks,
    paths: &[Vec<MakingCall>],
    diagnostics: &mut Diagnostics,
) {
    if result_uses.reports.is_empty() {
        return;
    }
    let unit = &program.modules[module];
    let instance = instance_note(program, (module, declaration), generics, packs);
    let calls = &result_uses.changed_results;
    for report in result_uses.reports.as_slice() {
        let mut report = report.clone().with_note(&instance);
        let changed = unit.bodies[declaration]
            .as_ref()
            .and_then(|body| changed_result_source(body, report.primary, calls));
        if let Some(changed) = changed
            && report.primary != changed
        {
            let use_at = report.primary;
            report = report.with_related(use_at, "this use checks the selected result");
            report.primary = changed;
        }
        emit_instance_report(report, paths, diagnostics);
    }
}

/// GEN-7: identify the enclosing declaration and its concrete arguments.
fn instance_note(
    program: &Program,
    (module, declaration): (usize, usize),
    generics: &[Argument],
    packs: &[hir::TypePackArgument],
) -> String {
    format!(
        "GEN-7: in `{}`",
        instance_name(program, (module, declaration), generics, packs)
    )
}

fn instance_name(
    program: &Program,
    (module, declaration): (usize, usize),
    generics: &[Argument],
    packs: &[hir::TypePackArgument],
) -> String {
    let name = &program.modules[module].declarations[declaration].name;
    let mut arguments = generics.iter().map(Argument::to_string).collect::<Vec<_>>();
    for pack in packs {
        if let Some(argument) = arguments.get_mut(pack.parameter) {
            let types = pack.types.iter().map(Type::to_string).collect::<Vec<_>>();
            *argument = format!("[{}]", types.join(", "));
        }
    }
    if arguments.is_empty() {
        return name.to_owned();
    }
    format!("{name}<{}>", arguments.join(", "))
}

/// Locate a changed call inside this use or through a local's initializer.
fn changed_result_source(
    body: &hir::Body,
    use_at: Location,
    calls: &[Location],
) -> Option<Location> {
    let within = |outer: Location| {
        unique_location(
            calls
                .iter()
                .copied()
                .filter(|&call| contains_location(outer, call)),
        )
    };
    if let Some(call) = within(use_at) {
        return Some(call);
    }
    let mut reads = HashMap::new();
    let mut initializers = HashMap::new();
    hir::walk_statement_nodes(&body.statements, &mut |node| match node {
        hir::Node::Expression(Expr {
            kind: ExprKind::Local(local),
            at,
            ..
        }) if matches!(body.locals[*local].kind, LocalKind::Let | LocalKind::Var) => {
            reads.insert(*at, *local);
        }
        hir::Node::Statement(hir::Stmt::Declare {
            local: declared,
            value: Some(value),
        }) if matches!(body.locals[*declared].kind, LocalKind::Let | LocalKind::Var) => {
            initializers.insert(*declared, value.at);
        }
        _ => {}
    });
    let mut at = use_at;
    let mut seen = HashSet::new();
    while seen.insert(at) {
        at = *initializers.get(reads.get(&at)?)?;
        if let Some(call) = within(at) {
            return Some(call);
        }
    }
    None
}

fn contains_location(outer: Location, inner: Location) -> bool {
    inner.file == outer.file
        && inner.span.start >= outer.span.start
        && inner.span.end <= outer.span.end
}

/// The only distinct source location, if the locations identify one.
fn unique_location(mut locations: impl Iterator<Item = Location>) -> Option<Location> {
    let first = locations.next()?;
    locations.all(|location| location == first).then_some(first)
}

/// A reread makes its own calls. The same final HIR is used regardless
/// of what caused the reread (GEN-7).
fn concrete_body_calls(
    program: &Program,
    instance: &crate::instance::CheckedInstance,
    independent: Option<&HashSet<(Location, String)>>,
    made: &mut Vec<Made>,
) {
    let mut calls = Vec::new();
    instance.walk_calls(&mut |expression| made_at(program, expression, &mut calls));
    made.extend(calls.into_iter().filter(|call| {
        call.required.is_none()
            && !independent.is_some_and(|calls| calls.contains(&source_call_key(call)))
            && call.type_packs.iter().all(|pack| {
                pack.forwarded_from.is_none() && pack.types_with_prefix().all(super::fit::whole)
            })
            && call
                .generics
                .iter()
                .enumerate()
                .all(|(position, argument)| {
                    resolved_default_argument(argument)
                        || call
                            .type_packs
                            .iter()
                            .any(|pack| pack.parameter == position)
                })
    }));
}

/// Graph settlement can finish an unresolved call without rereading
/// the body. The source's already-resolved calls still stand there.
fn made_after_inference(
    program: &Program,
    source: &hir::Body,
    settled: &crate::instance::CheckedInstance,
    made: &mut Vec<Made>,
) {
    let mut pending = HashSet::new();
    walk_nested_calls(&source.statements, &mut |expression| {
        if super::calls::pending_function_inference(&expression.kind)
            || matches!(
                expression.kind,
                ExprKind::Call {
                    callee: Callee::DeferredOperator(_),
                    ..
                } | ExprKind::DeferredBinary { .. }
                    | ExprKind::DeferredNegation { .. }
            )
        {
            pending.insert(expression.at);
        }
    });
    if pending.is_empty() {
        return;
    }
    settled.walk_calls(&mut |expression| {
        if pending.contains(&expression.at)
            && let Some(callee @ (Callee::Function { .. } | Callee::Candidate { .. })) =
                called_or_referenced(expression)
            && callee.generics().iter().all(resolved_default_argument)
        {
            made_at(program, expression, made);
        }
    });
}

/// The calls of generic declarations `expression` makes, at any depth.
fn made_by(program: &Program, expression: &Expr, made: &mut Vec<Made>) {
    expression.walk(&mut |expression| {
        visit_anonymous_bodies(expression, &mut |part| made_at(program, part, made));
    });
}

/// The calls an omitted, specialized default makes with resolved arguments
/// and evidence. Follow these as ordinary instances, including helper bodies
/// that can themselves omit an inherited default (GEN-16, UNIT-8).
pub(super) fn concrete_default_calls(program: &Program, expression: &Expr, made: &mut Vec<Made>) {
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
    made_by(program, expression, &mut calls);
    made.extend(calls.into_iter().filter(|call| {
        call.required.is_none()
            && call.generics.iter().all(resolved_default_argument)
            && call.answers.iter().all(concrete)
    }));
}

/// The callable reached by a call or a function reference.
pub(super) fn called_or_referenced(expression: &Expr) -> Option<&Callee> {
    match &expression.kind {
        ExprKind::Call { callee, .. } => Some(callee),
        ExprKind::StaticCallable(callee) => Some(callee),
        _ => None,
    }
}

/// The instance a call, function reference, or struct construction makes.
fn made_at(program: &Program, expression: &Expr, made: &mut Vec<Made>) {
    match &expression.kind {
        ExprKind::Call {
            callee, arguments, ..
        } => make_call(
            callee,
            expression.at,
            program.known_const_inputs(callee, arguments),
            made,
        ),
        ExprKind::StaticCallable(callee) => made_callee(callee, expression.at, made),
        ExprKind::Construct {
            module,
            declaration,
            ..
        } if let Type::Named { arguments, .. } = &expression.ty
            && !arguments.is_empty() =>
        {
            made.push(Made {
                callee: (*module, *declaration),
                generics: arguments.clone(),
                type_packs: Vec::new(),
                constants: ConstArguments::default(),
                answers: Vec::new(),
                test_candidates: None,
                required: None,
                at: expression.at,
                via: None,
            });
        }
        _ => {}
    }
}

/// Record the instance reached by a concrete call or static reference (GEN-7).
/// A call exposed by forwarding carries the supplying call's location.
pub(super) fn made_callee(callee: &Callee, at: Location, made: &mut Vec<Made>) {
    make_call(callee, at, ConstArguments::default(), made);
}

fn make_call(callee: &Callee, at: Location, constants: ConstArguments, made: &mut Vec<Made>) {
    match callee {
        Callee::Function {
            module,
            declaration,
            generics,
            type_packs,
            requirements,
            ..
        }
        | Callee::Candidate {
            module,
            declaration,
            generics,
            type_packs,
            requirements,
            ..
        } if !generics.is_empty()
            || !requirements.is_empty()
            || callee.test_candidates().is_some()
            || constants.limit.is_some()
            || constants.outgrown
            || !constants.values.is_empty() =>
        {
            made.push(Made {
                callee: (*module, *declaration),
                generics: generics.clone(),
                type_packs: type_packs.clone(),
                constants,
                answers: requirements.clone(),
                test_candidates: callee.test_candidates(),
                required: None,
                at,
                via: None,
            })
        }
        // GEN-16: an operation the enclosing declaration requires, whose
        // candidate a call of that declaration answers.
        Callee::Required { requirement, .. } => made.push(Made {
            callee: (usize::MAX, usize::MAX),
            generics: Vec::new(),
            type_packs: Vec::new(),
            constants: ConstArguments::default(),
            answers: Vec::new(),
            test_candidates: None,
            required: Some(*requirement),
            at,
            via: None,
        }),
        _ => {}
    }
}

/// A deferred call resolved by forwarding carries the path from its
/// own location to the call that supplied the pack, and the const inputs
/// it gives in the forwarding instance (FN-1, GEN-7).
pub(super) fn made_forwarded_callee(
    callee: &Callee,
    constants: ConstArguments,
    at: Location,
    via: Rc<MakingPath>,
    made: &mut Vec<Made>,
) {
    let index = made.len();
    make_call(callee, at, constants, made);
    if let Some(call) = made.get_mut(index) {
        call.via = Some(via);
    }
}

/// `answer`, recorded at a call in a generic body, for an instance of
/// that body: the body's generic arguments, `map`, in place, and what
/// the instance's caller answers the body's own requirements with,
/// `answers`, in place of each it states (`Answer::Stated`).
fn answered_in(
    program: &Program,
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
                .map(|argument| given_in(program, argument, map, declared))
                .collect(),
            requirements: requirements
                .iter()
                .map(|answer| answered_in(program, answer, map, declared, answers))
                .collect(),
        },
        hir::Answer::Stated(position) => answers
            .get(*position)
            .cloned()
            .unwrap_or(hir::Answer::Unread),
        hir::Answer::Unread => hir::Answer::Unread,
    }
}

/// GEN-3: only a nominal declaration has a category. An unresolved
/// parameter can use facts stated by its own declaration's requirement.
pub(super) fn category_holds(program: &Program, ty: &Type, category: SyntaxKind) -> Option<bool> {
    if let Some(known) = known_category(ty, category) {
        return Some(known);
    }
    let (at, subject) = match ty {
        Type::Parameter { at, .. } => (*at, CategorySubject::Parameter(*at)),
        Type::PackMember { pack, key, .. } => (
            *pack,
            CategorySubject::Member {
                pack: *pack,
                key,
                guaranteed: guaranteed_member(program, *pack, key),
            },
        ),
        _ => return None,
    };
    for (index, module) in program.modules.iter().enumerate() {
        for position in 0..module.declarations.len() {
            let Some((parameters, condition)) = required(program, (index, position)) else {
                continue;
            };
            if parameters.iter().any(|generic| generic.at == at) {
                let file = module.declarations[position].at.file;
                return category_facts(&condition, module, file, subject, true)
                    .into_iter()
                    .find_map(|(stated, positive)| {
                        if stated == category {
                            Some(positive)
                        } else if positive {
                            Some(false)
                        } else {
                            None
                        }
                    });
            }
        }
    }
    None
}

/// The symbolic type whose declaration may state a category (GEN-3, GEN-17).
#[derive(Clone, Copy)]
enum CategorySubject<'a> {
    Parameter(Location),
    Member {
        pack: Location,
        key: &'a Constant,
        guaranteed: bool,
    },
}

/// GEN-7 and GEN-10: only an existing member inherits universal facts.
/// A known nonnegative key must be below the pack's effective minimum.
fn guaranteed_member(program: &Program, pack: Location, key: &Constant) -> bool {
    let ConstantValue::Int(position) = key.value else {
        return false;
    };
    u64::try_from(position).ok().is_some_and(|position| {
        type_pack_bounds(program, pack).is_some_and(|(minimum, _)| position < minimum)
    })
}

fn known_category(ty: &Type, category: SyntaxKind) -> Option<bool> {
    match ty {
        Type::Unknown | Type::Parameter { .. } | Type::PackMember { .. } => None,
        Type::Named { kind, .. } => Some(matches!(
            (kind, category),
            (NamedKind::Struct, SyntaxKind::KwStruct) | (NamedKind::Enum, SyntaxKind::KwEnum)
        )),
        _ => Some(false),
    }
}

/// Facts implied by a true clause. Conjunction collects facts; a
/// disjunction keeps only facts shared by both alternatives. Negation
/// reverses these operations rather than turning a non-struct into an enum.
fn category_facts(
    node: &SyntaxNode,
    module: &crate::Module,
    file: FileId,
    subject: CategorySubject<'_>,
    positive: bool,
) -> Vec<(SyntaxKind, bool)> {
    match node.kind() {
        SyntaxKind::ConstraintParen => node.children().next().map_or_else(Vec::new, |inner| {
            category_facts(&inner, module, file, subject, positive)
        }),
        SyntaxKind::ConstraintNot => node.children().next().map_or_else(Vec::new, |inner| {
            category_facts(&inner, module, file, subject, !positive)
        }),
        SyntaxKind::ConstraintBinary => {
            let mut children = node.children();
            let (Some(left), Some(right)) = (children.next(), children.next()) else {
                return Vec::new();
            };
            let mut left = category_facts(&left, module, file, subject, positive);
            let right = category_facts(&right, module, file, subject, positive);
            if has_token(node, SyntaxKind::AmpAmp) == positive {
                left.extend(right);
            } else {
                left.retain(|fact| right.contains(fact));
            }
            left
        }
        SyntaxKind::ConstraintEach if positive => {
            let at = match subject {
                CategorySubject::Parameter(at)
                | CategorySubject::Member {
                    pack: at,
                    guaranteed: true,
                    ..
                } => at,
                CategorySubject::Member {
                    guaranteed: false, ..
                } => return Vec::new(),
            };
            let mut children = node.children();
            let (Some(view), Some(body)) = (children.next(), children.next()) else {
                return Vec::new();
            };
            let Some((view, name)) = pack_view(view) else {
                return Vec::new();
            };
            if view.text() != "types" || !module.uses.iter().any(|used|
                used.at == Location::new(file, name.span())
                    && matches!(&used.binding, Binding::Local { at: declared, .. } if *declared == at)) {
                return Vec::new();
            }
            let Some(bound) = tokens(node)
                .filter(|token| token.kind() == SyntaxKind::Ident)
                .nth(1)
            else {
                return Vec::new();
            };
            category_facts(
                &body,
                module,
                file,
                CategorySubject::Parameter(Location::new(file, bound.span())),
                true,
            )
        }
        SyntaxKind::ConstraintRelation if tokens(node).any(|token| token.text() == "in") => {
            let mut children = node.children();
            let (Some(left), Some(right)) = (children.next(), children.next()) else {
                return Vec::new();
            };
            if right.kind() != SyntaxKind::ConstraintCategory {
                return Vec::new();
            }
            let matches = match subject {
                CategorySubject::Parameter(at) => name_token(&left).is_some_and(|name| {
                    module.uses.iter().any(|used| {
                        used.at == Location::new(file, name.span())
                            && matches!(&used.binding, Binding::Local { at: declared, .. } if *declared == at)
                    })
                }),
                // GEN-17: a category asserted for one reflected key is
                // evidence only for that member.
                CategorySubject::Member { pack, key, .. } => module.written.iter().any(|written| {
                    written.at.file == file
                        && written.at.span.start == left.span().start
                        && matches!(&written.ty, Type::PackMember { pack: written_pack, key: written_key, .. }
                            if *written_pack == pack && written_key.value.same(&key.value) == Some(true))
                }),
            };
            if !matches {
                return Vec::new();
            }
            vec![(
                if has_token(&right, SyntaxKind::KwStruct) {
                    SyntaxKind::KwStruct
                } else {
                    SyntaxKind::KwEnum
                },
                positive,
            )]
        }
        _ => Vec::new(),
    }
}

/// What a candidate's condition is checked against at a call.
pub(super) struct Condition<'c> {
    /// Declaration identities used to prove category facts of symbolic types.
    pub(super) program: &'c Program,
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
    /// A category fact, including a caller's stated generic requirement.
    pub(super) category: &'c dyn Fn(&SyntaxNode, SyntaxKind) -> Option<bool>,
    /// Set where a comparison is not told because a value it compares
    /// is one this compiler does not work out: a constant held by its
    /// name, or one not worked out. Such a constant is not taken as
    /// known, so a `requires` not told for it is not taken to hold.
    pub(super) unvalued: &'c std::cell::Cell<bool>,
}

/// A concrete application names no type or value parameter and uses
/// only known constant values, including those nested inside its types.
pub(super) fn resolved_default_argument(argument: &Argument) -> bool {
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
        Argument::NoResult => true,
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
    // ERR-9's unknown type agrees with any type for recovery. That
    // agreement cannot prove a requirement; a known mismatch still can.
    let told = match same_type(left, right) {
        Some(true) if !whole(left) || !whole(right) => None,
        answer => answer,
    };
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

type QuantifiedCall<'a> = dyn Fn(&SyntaxNode, &RequirementType<'_>) -> Option<bool> + 'a;

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
    /// Read a requirement using this iteration's bound type or key.
    quantified: &'t QuantifiedCall<'t>,
    /// See [`Condition::unvalued`].
    unvalued: &'t std::cell::Cell<bool>,
    /// The types each pack parameter takes at the call (GEN-10).
    type_packs: &'t [hir::TypePackArgument],
    /// A failed pack member lookup while judging this condition.
    member_fault: Option<&'t std::cell::RefCell<Option<super::dependent_packs::MemberFault>>>,
}

/// GEN-6: what the operator's condition says of the types and values
/// bound by the candidate's declared parameters. Callability requirements
/// and options the declaration leaves open still need call-site facts.
pub(super) fn declared_requirement(
    scope: &Scope,
    operator: (usize, usize),
    arguments: &[Argument],
) -> Option<bool> {
    let (_, condition) = required(scope.program, operator)?;
    requires_holds(
        scope.program,
        operator,
        arguments,
        &condition,
        &Told {
            constant: &|node, ty| crate::constant::read(node, ty, scope).ok(),
            callable: &|_| None,
            quantified: &|_, _| None,
            unvalued: &std::cell::Cell::new(false),
            type_packs: &[],
            member_fault: None,
        },
    )
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
            Argument::Type(_) | Argument::NoResult => None,
        }
    };
    let own: Vec<Location> = parameters.iter().map(|generic| generic.at).collect();
    let map: Vec<(Location, &Argument)> = own.iter().copied().zip(generics).collect();
    let told = |ty: Type| (whole(&ty) && !names_parameter(&ty)).then_some(ty);
    let type_of = |name: &str| -> Option<Type> {
        let index = parameters.iter().position(|generic| generic.name == name)?;
        match generics.get(index)? {
            Argument::Type(ty) => told(ty.clone()),
            Argument::Constant(_) | Argument::NoResult => None,
        }
    };
    let file = declared_in.files[declared.file].file;
    let pack = |view: &SyntaxToken, name: &str| -> Option<&hir::TypePackArgument> {
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
    };
    let pack_types = super::dependent_packs::PackTypes::new(parameters, place.type_packs);
    let keyed_type = |node: &SyntaxNode, keys: &[ConstraintKey<'_>]| -> Option<Type> {
        let start = node.span().start;
        let read = declared_in
            .written
            .iter()
            .find(|written| written.at.file == file && written.at.span.start == start)?;
        let mut map = map.clone();
        map.extend(keys.iter().map(|key| (key.at, &key.argument)));
        // GEN-10: a known outer shape can rule out a quantified entry
        // without resolving every nested generic. same_type keeps a
        // possible match unknown; this check never contributes a binding.
        let projected = |ty| {
            if keys.is_empty() {
                told(placed(ty, &own))
            } else {
                Some(ty)
            }
        };
        // GEN-10: tuple operands expand captures even without an indexed
        // PackMember; a pack's ordinary generic slot is deliberately unknown.
        match pack_types.project(&read.ty, &map) {
            Ok(ty) => projected(ty),
            Err(fault) => {
                if let Some(target) = place.member_fault {
                    target.borrow_mut().get_or_insert(fault);
                }
                None
            }
        }
    };
    let written = |node: &SyntaxNode| keyed_type(node, &[]);
    let callable = |call: &SyntaxNode, keys: &[ConstraintKey<'_>]| {
        let written = |operand: &SyntaxNode| told(placed(keyed_type(operand, keys)?, &own));
        (place.quantified)(call, &written)
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
    let category = |node: &SyntaxNode, category| {
        let raw = (node.kind() == SyntaxKind::NamedType)
            .then(|| name_token(node))
            .flatten()
            .and_then(|name| {
                parameters
                    .iter()
                    .position(|generic| generic.name == name.text())
            })
            .and_then(|index| generics.get(index))
            .and_then(|argument| match argument {
                Argument::Type(ty) => Some(ty.clone()),
                Argument::Constant(_) | Argument::NoResult => None,
            })
            .or_else(|| {
                let read = declared_in
                    .written
                    .iter()
                    .find(|read| read.at.file == file && read.at.span.start == node.span().start)?;
                pack_types.project(&read.ty, &map).ok()
            })?;
        category_holds(program, &raw, category)
    };
    holds_with_packs(
        condition,
        &Condition {
            program,
            value_of: &value_of,
            constant: &constant,
            type_of: &type_of,
            written: &written,
            callable: place.callable,
            category: &category,
            unvalued: place.unvalued,
        },
        Some(&PackScope {
            file,
            pack: &pack,
            written: &keyed_type,
            callable: &callable,
            constant: place.constant,
        }),
        &[],
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
        Type::TypeValue
        | Type::Scalar(_)
        | Type::Parameter { .. }
        | Type::PackMember { .. }
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
    holds_with_packs(node, of, None, &[])
}

/// A compile-time key bound by a GEN-17 quantifier.
#[derive(Clone)]
struct ConstraintKey<'a> {
    name: &'a str,
    at: Location,
    argument: Argument,
}

impl ConstraintKey<'_> {
    fn value(&self) -> Option<&ConstantValue> {
        match &self.argument {
            Argument::Constant(constant) => Some(&constant.value),
            Argument::Type(_) | Argument::NoResult => None,
        }
    }
}

type PackView<'a> = dyn Fn(&SyntaxToken, &str) -> Option<&'a hir::TypePackArgument> + 'a;
type KeyedType<'a> = dyn Fn(&SyntaxNode, &[ConstraintKey<'_>]) -> Option<Type> + 'a;
type KeyedCall<'a> = dyn Fn(&SyntaxNode, &[ConstraintKey<'_>]) -> Option<bool> + 'a;
type ScopedConstant<'a> = dyn Fn(&SyntaxNode, Option<&Type>) -> Option<ConstantValue> + 'a;

/// The known packs and written types of a declaration being specialized.
struct PackScope<'a> {
    file: FileId,
    pack: &'a PackView<'a>,
    written: &'a KeyedType<'a>,
    callable: &'a KeyedCall<'a>,
    constant: &'a ScopedConstant<'a>,
}

/// The verdict for a condition with any resolved type packs in scope.
fn holds_with_packs(
    node: &SyntaxNode,
    of: &Condition,
    scope: Option<&PackScope<'_>>,
    keys: &[ConstraintKey<'_>],
) -> Option<bool> {
    match node.kind() {
        SyntaxKind::ConstraintParen => holds_with_packs(&node.children().next()?, of, scope, keys),
        SyntaxKind::ConstraintNot => {
            holds_with_packs(&node.children().next()?, of, scope, keys).map(|held| !held)
        }
        SyntaxKind::ConstraintEach | SyntaxKind::ConstraintAny => {
            each_holds(node, of, scope?, keys)
        }
        SyntaxKind::ConstraintValue => {
            let name = name_token(node)?;
            match (of.value_of)(name.text()) {
                Some(ConstantValue::Bool(value)) => Some(value),
                _ => {
                    of.unvalued.set(true);
                    None
                }
            }
        }
        // GEN-16: a named operator requirement.
        SyntaxKind::ConstraintCall => (of.callable)(node).or_else(|| {
            if keys.is_empty() {
                return None;
            }
            (scope?.callable)(node, keys)
        }),
        SyntaxKind::ConstraintBinary => {
            let mut sides = node.children();
            let left = holds_with_packs(&sides.next()?, of, scope, keys);
            let both = has_token(node, SyntaxKind::AmpAmp);
            if left == Some(!both) {
                return left;
            }
            let right = holds_with_packs(&sides.next()?, of, scope, keys);
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
            if within && right.kind() == SyntaxKind::ConstraintCategory {
                let category = if has_token(right, SyntaxKind::KwStruct) {
                    SyntaxKind::KwStruct
                } else {
                    SyntaxKind::KwEnum
                };
                if option(left).and_then(|name| (of.value_of)(&name)).is_some()
                    || (left.kind() != SyntaxKind::NamedType
                        && !is_type(left.kind())
                        && scope.is_some_and(|scope| (scope.constant)(left, None).is_some()))
                {
                    return Some(false);
                }
                let ty = option(left)
                    .and_then(|name| {
                        keys.iter()
                            .rev()
                            .find(|key| key.name == name)
                            .and_then(|key| match &key.argument {
                                Argument::Type(ty) => Some(ty.clone()),
                                _ => None,
                            })
                    })
                    .or_else(|| option(left).and_then(|name| (of.type_of)(&name)))
                    .or_else(|| (scope?.written)(left, keys));
                if let Some(ty) = ty
                    && let Some(held) = category_holds(of.program, &ty, category)
                {
                    return Some(held);
                }
                return (of.category)(left, category);
            }
            let key = |operand: &SyntaxNode| {
                let name = option(operand)?;
                keys.iter()
                    .rev()
                    .find(|key| key.name == name)
                    .and_then(ConstraintKey::value)
            };
            if within
                && let Some((view, pack)) = pack_view(right.clone())
                && view.text() == "arg_keys"
            {
                let scope = scope?;
                let left_key = key(left)
                    .cloned()
                    .or_else(|| option(left).and_then(|name| (of.value_of)(&name)))
                    .or_else(|| (scope.constant)(left, None))?;
                if !matches!(left_key, ConstantValue::Str(_) | ConstantValue::Int(_)) {
                    return None;
                }
                return Some(
                    (scope.pack)(&view, pack.text())?
                        .member(&left_key)
                        .is_some(),
                );
            }
            if has_token(node, SyntaxKind::EqEq) || has_token(node, SyntaxKind::BangEq) {
                let equal = has_token(node, SyntaxKind::EqEq);
                if let Some((known, operand)) = key(left)
                    .map(|key| (key, right))
                    .or_else(|| key(right).map(|key| (key, left)))
                {
                    let other = key(operand)
                        .cloned()
                        .or_else(|| (scope?.constant)(operand, None))?;
                    return known.equals(&other).map(|same| same == equal);
                }
            }
            let written = |operand: &SyntaxNode| {
                (of.written)(operand).or_else(|| (scope?.written)(operand, keys))
            };
            // A generic parameter of the candidate: the condition is of
            // the type it stands for. `T == i64`, `T in {i64, f64}`.
            // Each is told as a constant's equality is, in three answers
            // ([`same_type`]): `FBox<X>` is not known to be `FBox<2.0>`
            // while `X` is not given.
            if let Some(ty) = option(left)
                .and_then(|name| (of.type_of)(&name))
                .or_else(|| (scope?.written)(left, keys))
            {
                return if has_token(node, SyntaxKind::EqEq) || has_token(node, SyntaxKind::BangEq) {
                    typed_alike(&written(right)?, &ty, of)
                        .map(|same| same == has_token(node, SyntaxKind::EqEq))
                } else if within && right.kind() == SyntaxKind::ConstraintSet {
                    let mut found = Some(false);
                    for member in right.children() {
                        match typed_alike(&written(&member)?, &ty, of) {
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
            if (has_token(node, SyntaxKind::EqEq) || has_token(node, SyntaxKind::BangEq))
                && let Some(ty) = option(right)
                    .and_then(|name| (of.type_of)(&name))
                    .or_else(|| (scope?.written)(right, keys))
            {
                return typed_alike(&written(left)?, &ty, of)
                    .map(|same| same == has_token(node, SyntaxKind::EqEq));
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
            if has_token(node, SyntaxKind::EqEq) || has_token(node, SyntaxKind::BangEq) {
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
                    .map(|same| same == has_token(node, SyntaxKind::EqEq))
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
fn each_holds(
    node: &SyntaxNode,
    of: &Condition,
    scope: &PackScope<'_>,
    keys: &[ConstraintKey<'_>],
) -> Option<bool> {
    let existential = node.kind() == SyntaxKind::ConstraintAny;
    let bound = tokens(node)
        .filter(|token| token.kind() == SyntaxKind::Ident)
        .nth(usize::from(!existential))?;
    let mut parts = node.children();
    let (view, pack) = pack_view(parts.next()?)?;
    let body = parts.next()?;
    let pack = (scope.pack)(&view, pack.text())?;
    let mut held = Some(!existential);
    if view.text() == "arg_keys" {
        let at = Location::new(scope.file, bound.span());
        let mut nested = keys.to_vec();
        for key in &pack.keys {
            let value = match key {
                hir::PackKey::Name(name) => ConstantValue::Str(name.clone()),
                hir::PackKey::Position(position) => {
                    ConstantValue::Int(i64::try_from(*position).ok()?)
                }
            };
            nested.push(ConstraintKey {
                name: bound.text(),
                at,
                argument: Argument::Constant(Constant {
                    text: bound.text().to_string(),
                    value,
                }),
            });
            let checked = holds_with_packs(&body, of, Some(scope), &nested);
            nested.pop();
            match checked {
                Some(value) if value == existential => return Some(value),
                Some(_) => {}
                None => held = None,
            }
        }
        return held;
    }
    for ty in &pack.types {
        // GEN-10: a zero-length forwarded pack contributes no element
        // to an each/any view, even while its tuple marker is symbolic.
        let source = match ty {
            Type::Parameter { at, .. }
                if pack.tuple_sources.contains(at) || pack.forwarded_from == Some(*at) =>
            {
                Some(*at)
            }
            _ => None,
        };
        let bounds = source.and_then(|at| type_pack_bounds(of.program, at));
        if bounds.is_some_and(|(_, maximum)| maximum == Some(0)) {
            continue;
        }
        let mut nested = keys.to_vec();
        nested.push(ConstraintKey {
            name: bound.text(),
            at: Location::new(scope.file, bound.span()),
            argument: Argument::Type(ty.clone()),
        });
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
                (scope.written)(operand, &nested).or_else(|| (of.written)(operand))
            }
        };
        let here = Condition {
            type_of: &type_of,
            written: &written,
            ..*of
        };
        match holds_with_packs(&body, &here, Some(scope), &nested) {
            Some(value) if value == existential => {
                // A decisive any-true or each-false answer needs an
                // actual element. A symbolic pack may still be empty.
                if source.is_some() && bounds.is_none_or(|(minimum, _)| minimum == 0) {
                    held = None;
                } else {
                    return Some(value);
                }
            }
            Some(_) => {}
            None => held = None,
        }
    }
    held
}

/// GEN-10: explicit bounds and exact tuple<Us> occurrences anywhere in
/// a non-packed parameter type guarantee lengths. Fixed tuple items do not.
pub(super) fn type_pack_bounds(program: &Program, at: Location) -> Option<(u64, Option<u64>)> {
    for detail in program.modules.iter().flat_map(|module| &module.details) {
        let (generics, parameters) = match detail {
            Detail::Callable(signature) => (&signature.generics, &signature.parameters[..]),
            Detail::Struct(structure) => (&structure.generics, &[][..]),
            _ => continue,
        };
        let Some(generic) = generics.iter().find(|generic| generic.at == at) else {
            continue;
        };
        let GenericKind::Pack {
            mut minimum,
            mut maximum,
        } = generic.kind
        else {
            return None;
        };
        for parameter in parameters {
            if parameter.pack.is_some()
                && matches!(parameter.ty, Type::Parameter { at: declared, .. } if declared == at)
            {
                minimum = minimum.max(parameter.pack_bounds.0);
                maximum = maximum.into_iter().chain(parameter.pack_bounds.1).min();
            } else if parameter.pack.is_none()
                && parameter.ty.any_part(
                    &|part| {
                        matches!(part, Type::Tuple(elements) if matches!(elements.as_slice(),
                        [Type::Parameter { at: declared, .. }] if *declared == at))
                    },
                    &|_| false,
                )
            {
                minimum = minimum.max(1);
            }
        }
        return Some((minimum, maximum));
    }
    None
}

/// The view name and pack name in a `types(Ts)` or `arg_keys(Ps)` form.
pub(super) fn pack_view(source: SyntaxNode) -> Option<(SyntaxToken, SyntaxToken)> {
    let source = ungrouped(source);
    if source.kind() != SyntaxKind::CallExpr {
        return None;
    }
    let callee = child(&source, SyntaxKind::NameExpr)?;
    let view = name_token(&callee)?;
    if !matches!(view.text(), "types" | "arg_keys")
        || child(&callee, SyntaxKind::QualifiedName).is_some()
    {
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
