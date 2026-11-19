//! `requires`: whether a declaration's condition holds for the generic
//! arguments it is called or applied with, and the operations a generic
//! body may call because its requirements name them (GEN-3, GEN-7,
//! GEN-15, GEN-16). Part of the typing pass ([`super`]).

use std::collections::HashMap;

use hgl_diagnostics::{Category, Code, Diagnostics, Location};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode};
use hgl_source::FileId;

use super::candidates::{Given, Key, Wanted};
use super::fit::{open, whole};
use super::inference::{fixed, mentions, placed, unsaid};
use super::operators::{Requested, from_place, unreadable};
use super::{REQUIRING, Typer};
use crate::declared::{names_parameter, same_type, substitute_within};
use crate::hir::{self, Callee, Expr, ExprKind, Form};
use crate::program::{Binding, Declaration, DeclarationKind, OperatorRef, Program, Role};
use crate::read::is_type;
use crate::syntax::{child, has_token, name_token, tokens};
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
            .filter(|node| node.kind() == SyntaxKind::ConstraintCall)
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
        match held {
            Some(false) => self
                .diagnostics
                .emit(unmet(program, callee, at, &condition)),
            None if unvalued => self
                .diagnostics
                .emit(untold(program, callee, at, &condition)),
            _ => {}
        }
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
        (module, declaration): (usize, usize),
        generics: &[Argument],
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
        let program = self.program;
        let declared_in = &program.modules[module];
        let declared = &declared_in.declarations[declaration];
        let Some(Detail::Callable(signature)) = declared_in.details.get(declaration) else {
            return Vec::new();
        };
        let Some(clause) = child(&declared.syntax, SyntaxKind::RequiresClause) else {
            return Vec::new();
        };
        let mut calls = Vec::new();
        if let Some(condition) = clause.children().next() {
            required_calls(&condition, &mut calls);
        }
        if calls.is_empty() {
            return Vec::new();
        }
        if self.requiring >= REQUIRING {
            return vec![hir::Answer::Unread; calls.len()];
        }
        let file = declared_in.files[declared.file].file;
        let read =
            self.requirements_read(declared, &signature.generics, (module, generics), &calls);
        read.into_iter()
            .map(|read| {
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
        let supplied: Vec<Option<&Type>> = (0..public.parameters.len())
            .map(|index| operands.get(index).map(|(_, ty)| ty))
            .collect();
        let Requested {
            place: requested,
            among,
            unfit,
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
                };
                Given::read(position, node, value)
            })
            .collect();
        let wanted = Wanted {
            place: asked.as_ref(),
            among,
        };
        self.requiring += 1;
        let (mut selected, untold) = self.admitted(operator, &candidates, &given, &wanted);
        self.requiring -= 1;
        if untold {
            return None;
        }
        // GEN-5: "Select exactly one most-specific admitted candidate."
        Some(match (selected.pop(), selected.is_empty()) {
            (Some(chosen), true) => Answered::Selected {
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
        let supplied: Vec<Option<&Type>> = (0..public.parameters.len())
            .map(|index| requirement.operands.get(index))
            .collect();
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
        let mut places = Vec::with_capacity(given.len());
        for argument in given {
            // One read as the type of its place whose first reading
            // stands was reported itself: the call is not judged here.
            if argument.by_place && !argument.retried {
                return None;
            }
            let place = match &argument.key {
                Key::Position(position) => *position,
                Key::Name(name) => public
                    .parameters
                    .iter()
                    .position(|parameter| parameter.name == *name)?,
            };
            // The operands a requirement is supplied with stand in the
            // operator's first places, each given once.
            if place >= given.len() || places.contains(&place) {
                return None;
            }
            places.push(place);
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
        let mut ordered: Vec<&Given> = given.to_vec();
        for (argument, &place) in given.iter().zip(&places) {
            ordered[place] = argument;
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
                let agrees = public.parameters.iter().zip(&requirement.operands).all(
                    |(parameter, operand)| {
                        self.admits(&substitute_within(&parameter.ty, &map), operand)
                    },
                ) && match (&public.result, &requirement.result) {
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
            for (operand, argument) in requirement.operands.iter().zip(&ordered) {
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
        form: Form,
    ) -> (ExprKind, Type) {
        let Required {
            index,
            result,
            operands,
            places,
        } = required;
        // The operator's own parameters, in the order it declares them
        // (GEN-4).
        let parameters = match self.program.modules[operator.module]
            .details
            .get(operator.declaration)
        {
            Some(Detail::Callable(public)) => public.parameters.len(),
            _ => 0,
        };
        let arguments = given
            .into_iter()
            .zip(places)
            .map(|(given, place)| {
                let value = if given.retried {
                    let operand = &operands[place];
                    let value = self.expression(&given.node, Some(operand));
                    self.fit(value, operand, "the operand its requirement supplies")
                } else {
                    given.value
                };
                hir::Argument {
                    parameter: (place < parameters).then_some(place),
                    value,
                }
            })
            .collect();
        let kind = ExprKind::Call {
            callee: Callee::Required {
                operator: (operator.module, operator.declaration),
                requirement: index,
            },
            arguments,
            form,
        };
        (kind, result.unwrap_or(Type::Unknown))
    }
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
    /// The place among them each argument of the call is given to, in
    /// the order the call writes them.
    places: Vec<usize>,
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
struct Made {
    callee: (usize, usize),
    generics: Vec<Argument>,
    at: Location,
}

/// How many calls deep an instance is followed from a call that gives
/// every generic argument. An instance past it makes no more: a
/// recursion that gives a new argument each time,
/// `count<N - 1>` in `count<const N: i64>`, makes instances without end.
const INSTANCES_DEEP: usize = 64;

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
pub(super) fn instances_hold(program: &Program, diagnostics: &mut Diagnostics) {
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
    let mut given: Vec<Made> = Vec::new();
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
                made_by(default, &mut made);
            }
            let declared = own((index, position));
            for call in made {
                if passes_on(&call.generics, &declared) {
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
            if let Some(config) = &run.config {
                made_by(config, &mut given);
            }
        }
        for constant in module.constants.iter().flatten() {
            made_by(constant, &mut given);
        }
    }
    if passed.is_empty() {
        return;
    }
    let tables: Vec<_> = program.modules.iter().map(super::tables).collect();
    let empty = Signature {
        generics: Vec::new(),
        parameters: Vec::new(),
        result: None,
    };
    // Each instance followed once, by what it is: its callee and its
    // arguments.
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut waiting: std::collections::VecDeque<(Made, usize)> =
        given.into_iter().map(|made| (made, 0)).collect();
    while let Some((made, depth)) = waiting.pop_front() {
        let Some(inner) = passed.get(&made.callee) else {
            continue;
        };
        let declared = own(made.callee);
        let map: Vec<(Location, &Argument)> =
            declared.iter().copied().zip(&made.generics).collect();
        for call in inner {
            let generics: Vec<Argument> = call
                .generics
                .iter()
                .map(|argument| given_in(argument, &map, &declared))
                .collect();
            let (module, declaration) = call.callee;
            let unit = &program.modules[module];
            let (bindings, written, calls) = &tables[module];
            let mut typer = super::Typer::bare(
                program,
                unit,
                &unit.declarations[declaration],
                &empty,
                super::Tables {
                    bindings,
                    written,
                    calls,
                },
                diagnostics,
            );
            typer.instance_met((made.at, call.at), call.callee, &generics);
            let key = format!("{:?}{generics:?}", call.callee);
            if depth < INSTANCES_DEEP && seen.insert(key) {
                waiting.push_back((
                    Made {
                        callee: call.callee,
                        generics,
                        at: made.at,
                    },
                    depth + 1,
                ));
            }
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

/// The call `expression` is, where it is one of a function or a struct
/// it gives generic arguments.
fn made_at(expression: &Expr, made: &mut Vec<Made>) {
    let (callee, generics) = match &expression.kind {
        ExprKind::Call {
            callee:
                Callee::Function {
                    module,
                    declaration,
                    generics,
                    ..
                },
            ..
        } => ((*module, *declaration), generics.clone()),
        ExprKind::Construct {
            module,
            declaration,
            ..
        } => match &expression.ty {
            Type::Named { arguments, .. } => ((*module, *declaration), arguments.clone()),
            _ => return,
        },
        _ => return,
    };
    if !generics.is_empty() {
        made.push(Made {
            callee,
            generics,
            at: expression.at,
        });
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
    holds(
        condition,
        &Condition {
            value_of: &value_of,
            constant: &constant,
            type_of: &type_of,
            written: &written,
            callable: place.callable,
            unvalued: place.unvalued,
        },
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
            "the `requires` of `{}` is not known to hold for this {what}: this compiler does \
             not work out a constant it compares",
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
        | Type::Ref(inner)
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
    match node.kind() {
        SyntaxKind::ConstraintParen => holds(&node.children().next()?, of),
        SyntaxKind::ConstraintNot => holds(&node.children().next()?, of).map(|held| !held),
        // GEN-16: a named operator requirement.
        SyntaxKind::ConstraintCall => (of.callable)(node),
        SyntaxKind::ConstraintBinary => {
            let mut sides = node.children();
            let left = holds(&sides.next()?, of);
            let right = holds(&sides.next()?, of);
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
                    same_type(&(of.written)(right)?, &ty)
                } else if within && right.kind() == SyntaxKind::ConstraintSet {
                    let mut found = Some(false);
                    for member in right.children() {
                        match same_type(&(of.written)(&member)?, &ty) {
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
                // The language's `==` (`ConstantValue::equals`): "`requires
                // X == 0.0` is the language's `==`: it holds for `-0.0`"
                // (hhenson/hgl_spec#139, UNIT-13). Where the value is not
                // known, whether it holds is not either.
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
