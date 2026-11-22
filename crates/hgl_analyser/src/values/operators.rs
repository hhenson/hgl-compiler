//! A call of an operator, by its name or where a symbol stands for it:
//! the output it requests, its arguments as they are written, and the
//! operator's value candidates (GEN-5, GEN-6, EXPR-2). Part of the typing
//! pass ([`super`]); the candidate is selected in [`super::candidates`].

use std::collections::HashSet;

use hgl_diagnostics::{Category, Diagnostic, Location};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};

use super::calls::arguments_of;
use super::candidates::{Chosen, Given, Key, Settled, Wanted, typed_by_place};
use super::constants::{constant_text, generic_default};
use super::fit::{open, whole};
use super::inference::{
    ExplicitGenerics, bare_parameter, fixed, infer, infer_from_place, mentions, pack_bounds,
    placed, unsaid,
};
use super::{Candidate, Typer};
use crate::declared::{agree, names_parameter, substitute_within};
use crate::hir::{self, Callee, Expr, ExprKind, Form, ValueCategory};
use crate::program::{OperatorRef, Program, Role};
use crate::syntax::{has_token, name_token};
use crate::types::{
    Argument, CallArguments, Constant, ConstantValue, Detail, GenericKind, NamedKind, Pack,
    Signature, Type,
};
use crate::{codes, error};

/// A deferred named call's arguments and its place as written in the
/// generic body. Requirement matching still uses that source place.
#[derive(Clone, Copy)]
struct ReplayedCall<'a> {
    arguments: &'a [hir::Argument],
    contextual: &'a [bool],
    source_expected: Option<&'a Type>,
}

impl<'a> Typer<'a> {
    /// The output a call of `operator` requests of its candidates
    /// (GEN-5), from what is known of the operator's generic parameters
    /// so far, `found`, and the type given to each of its parameters,
    /// `supplied`. `said` is true when the call writes generic
    /// arguments, which is a request in itself.
    ///
    /// GEN-8: "An operator's takes part in the output requested, in
    /// GEN-2's order: what the call writes, then what its place
    /// expects, then its arguments, and the default where none of them
    /// says." GEN-12: the arguments in the order the operator declares
    /// its parameters. GEN-2: "an abstract struct that the type decided
    /// inherits from widens the parameter to that struct". GEN-4: the
    /// operator's own requirement of its result.
    ///
    /// A call by name and the test of a named operator requirement
    /// (GEN-16) both ask through here, so a requirement holds as the
    /// call it names would select.
    pub(super) fn output_requested(
        &self,
        operator: OperatorRef,
        public: &Signature,
        mut found: Vec<Option<Argument>>,
        supplied: &[Option<&Type>],
        said: bool,
    ) -> Requested {
        let program = self.program;
        self.infer_operator_inputs(public, &mut found, supplied);
        let operator_declaration =
            &program.modules[operator.module].declarations[operator.declaration];
        let operator_syntax = &operator_declaration.syntax;
        // True when the call says something of the output itself: it
        // writes generic arguments, or one of the operator's takes its
        // default.
        let mut asked = said;
        // The operator's generic value parameters that take their
        // defaults, each with the default.
        let mut defaulted: Vec<(usize, SyntaxNode)> = Vec::new();
        for (index, (generic, found)) in public.generics.iter().zip(found.iter_mut()).enumerate() {
            if found.is_none()
                && let Some(default) = generic_default(operator_syntax, generic)
            {
                *found = self
                    .default_constant(operator_declaration, generic)
                    .map(Argument::Constant);
                asked = true;
                defaulted.push((index, default));
            }
        }
        // A generic nothing has said anything of is left open: a type
        // as one not known, and a value as a constant not evaluated,
        // which agrees with any. So a `list<list<f64, cols>, rows>` with
        // `rows` 3 and `cols` not said is still asked to have three
        // rows.
        let explicit: Vec<Argument> = found
            .into_iter()
            .zip(&public.generics)
            .map(|(found, generic)| {
                found.unwrap_or_else(|| match generic.kind {
                    GenericKind::Value(_) => Argument::Constant(unsaid()),
                    _ => Argument::Type(Type::Unknown),
                })
            })
            .collect();
        let own_generics: Vec<Location> =
            public.generics.iter().map(|generic| generic.at).collect();
        let map: Vec<(Location, &Argument)> = fixed(&public.generics).zip(&explicit).collect();
        // GEN-13: a default the call takes is of its parameter's type
        // as the call resolves it. One that is not of the type as it is
        // declared is reported where it is declared, and is none of the
        // call's.
        for (index, default) in defaulted {
            let GenericKind::Value(declared) = &public.generics[index].kind else {
                continue;
            };
            let resolved = placed(substitute_within(declared, &map), &own_generics);
            if let Some(Err(described)) =
                self.unfit_default(operator_declaration, &default, &resolved, declared)
            {
                return Requested {
                    place: None,
                    among: None,
                    generics: Vec::new(),
                    unfit: Some(Unfit {
                        generic: index,
                        described,
                        resolved,
                    }),
                };
            }
        }
        // The output requested, as far as it is known: a `list<T, 3>`
        // with `T` not yet said is a list of three of anything.
        let place: Option<Type> = asked
            .then(|| {
                public
                    .result
                    .as_ref()
                    .map(|result| placed(substitute_within(result, &map), &own_generics))
                    .filter(|requested| !matches!(requested, Type::Unknown))
            })
            .flatten();
        // GEN-4: an operator "owns ... generic relationships, and
        // requirements". Where its requirement says what its result may
        // be, a candidate's result is one of those.
        let among = self.result_among(operator, public, &map, &own_generics);
        Requested {
            place,
            among,
            generics: if public
                .parameters
                .iter()
                .any(|parameter| parameter.configuration)
            {
                explicit
            } else {
                Vec::new()
            },
            unfit: None,
        }
    }

    /// GEN-2 and GEN-12: the operator's inputs resolve its generics in
    /// declaration order, whether for the requested output or to complete
    /// an FN-17 function template before binding static arguments.
    pub(super) fn infer_operator_inputs(
        &self,
        public: &Signature,
        found: &mut [Option<Argument>],
        supplied: &[Option<&Type>],
    ) {
        // The generic parameters an argument decided: what the call
        // writes, or its place expects, is not widened (GEN-2).
        let mut by_argument = vec![false; public.generics.len()];
        for (parameter, actual) in public.parameters.iter().zip(supplied) {
            let Some(actual) = *actual else {
                continue;
            };
            if let Some(generic) = bare_parameter(&parameter.ty, &public.generics)
                && by_argument[generic]
                && let Some(Argument::Type(decided)) = &found[generic]
                && self.widens(decided, actual)
            {
                found[generic] = Some(Argument::Type(actual.clone()));
                continue;
            }
            let before: Vec<bool> = found.iter().map(Option::is_some).collect();
            infer(&parameter.ty, actual, &public.generics, found);
            for (generic, was) in before.into_iter().enumerate() {
                by_argument[generic] |= !was && found[generic].is_some();
            }
        }
    }

    /// A call of the operator `operator` by its name.
    ///
    /// GEN-5: "Match candidates against the requested output before
    /// inputs, then check their conditions. Select exactly one
    /// most-specific admitted candidate ... No match rejects; tied
    /// most-specific candidates are ambiguous ... A call in a value
    /// function or a runtime body selects among the value candidates."
    /// FN-3: "Any body calls an operator; in a runtime body or a value
    /// function the call selects a value candidate."
    ///
    /// The arguments are given to the operator's public parameters
    /// (GEN-4: the operator "owns public names/order"), and a candidate
    /// is admitted when each fits the parameter the candidate has in
    /// that place. A candidate that fits with no integer literal
    /// widened is taken before one that needs it.
    ///
    pub(super) fn call_operator(
        &mut self,
        name: &SyntaxToken,
        list: &SyntaxNode,
        operator: OperatorRef,
        expected: Option<&Type>,
        generics: Option<&CallArguments>,
    ) -> (ExprKind, Type) {
        self.call_operator_with(name, list, operator, expected, generics, None)
    }

    /// Read the named call, reusing settled argument readings in an instance.
    fn call_operator_with(
        &mut self,
        name: &SyntaxToken,
        list: &SyntaxNode,
        operator: OperatorRef,
        expected: Option<&Type>,
        generics: Option<&CallArguments>,
        replay: Option<ReplayedCall<'_>>,
    ) -> (ExprKind, Type) {
        let program = self.program;
        let Some(Detail::Callable(public)) = program.modules[operator.module]
            .details
            .get(operator.declaration)
        else {
            return self.unread_call(list);
        };
        // GEN-8: the generic arguments the call writes are the
        // operator's: with `operator convert<T, V>(value: V) -> T`,
        // "`convert<f64>(count)` supplies T". With them in place the
        // operator's result is the output requested, when nothing of it
        // is left to resolve: `convert<f64>(count)` asks for an `f64`.
        // What they say of the operands is for the pass that resolves
        // generic candidates.
        let Some(ExplicitGenerics {
            fixed: written,
            faulty,
            pack,
        }) = self.explicit_generics(name, public, generics)
        else {
            return self.unread_call(list);
        };
        if faulty {
            return self.unread_call(list);
        }
        // GEN-8: "A generic value parameter the call does not give takes
        // its default, when it has one. An operator's takes part in the
        // output requested, in GEN-2's order: what the call writes, then
        // what its place expects, then its arguments, and the default
        // where none of them says."
        // What the call writes, before its place says anything.
        let mut explicit = written;
        let candidates = self.value_candidates(operator);
        if unreadable(public, &candidates) {
            return self.unread_call(list);
        }

        // EXPR-2: "Named arguments use the selected public callable's
        // parameter names; a call of an operator may also name an
        // option its candidates declare (GEN-6)." Which parameter of a
        // candidate an argument is given to, and what a call may leave
        // out, is each candidate's to say (GEN-6: "A call is matched
        // against each candidate's own signature").
        let options = options_of(public, &candidates);
        let reported = self.diagnostics.len();
        let written = self.operator_arguments(name, list, public, &options, pack.as_ref());
        let named_generics = self.diagnostics.len() == reported
            && self.operator_generic_arguments(
                public,
                written.iter().map(|(key, node)| (key, node)),
                &mut explicit,
            );
        if replay.is_some() && named_generics {
            let map: Vec<_> = self
                .instance
                .iter()
                .map(|(at, argument)| (*at, argument))
                .collect();
            for argument in explicit.iter_mut().flatten() {
                *argument = crate::instance::placed(argument, &map);
            }
        }
        let mut found = explicit.clone();
        from_place(public, expected, &mut found);
        let mut context = None;
        if public
            .parameters
            .iter()
            .any(|parameter| matches!(parameter.ty, Type::Function(_)))
        {
            let mut inferred = found.clone();
            let unknown: Vec<_> = public
                .generics
                .iter()
                .zip(&inferred)
                .filter_map(|(generic, given)| given.is_none().then_some(generic.at))
                .collect();
            if !unknown.is_empty()
                && written.iter().any(|(key, _)| {
                    public_function(public, key).is_some_and(|ty| mentions(ty, &unknown))
                })
            {
                // GEN-2, FN-17: a function's signature may say what a
                // generic stands for, while an argument later in the call
                // can complete the template needed by a bare callback or
                // lambda. The speculative read is taken back before
                // arguments are read in source order.
                let mark = self.mark();
                let probe = self.read_operator_arguments(
                    public,
                    written.iter().cloned(),
                    Some(&inferred),
                    pack.as_ref(),
                    replay,
                );
                self.take_back(mark, false);
                self.infer_operator_inputs(public, &mut inferred, &public_inputs(public, &probe));
            }
            context = Some(inferred);
        }
        let mut given = self.read_operator_arguments(
            public,
            written,
            context.as_deref(),
            pack.as_ref(),
            replay,
        );
        self.constants_given(&given, public);
        if self.diagnostics.len() == reported {
            self.missing_configuration(
                public,
                &given,
                name.text(),
                Location::new(self.file, name.span()),
            );
        }
        // An argument given to no parameter, or rejected itself, has
        // been reported, and one whose type is not known says nothing
        // of the candidates: the call is then not judged (ERR-9).
        let faulty = self.diagnostics.len() != reported;
        let unknown = given
            .iter()
            .any(|given| !given.by_place && open(given.ty()));
        if self.graph
            && replay.is_none()
            && !faulty
            && unknown
            && given
                .iter()
                .any(|given| !given.by_place && matches!(given.ty(), Type::Unknown))
        {
            // GEN-7: the arguments can settle only after an earlier
            // operator in this graph has selected its instance candidate.
            let deferred = hir::DeferredOperator {
                identity: operator,
                list: self.at(list),
                expected: expected.cloned(),
                generics: generics.cloned(),
                contextual: given.iter().map(|given| given.by_place).collect(),
                captured: self.callable.is_some(),
            };
            let mut kind = self.unselected(given, Form::Named);
            if let ExprKind::Call { callee, .. } = &mut kind {
                *callee = Callee::DeferredOperator(Box::new(deferred));
            }
            return (kind, Type::Unknown);
        }
        if faulty || unknown {
            return (self.unselected(given, Form::Named), Type::Unknown);
        }
        // Core's OPS-6: "`convert<list<B>>(values)` is admitted too,
        // where `values` is a `list<A>` and the struct `A` inherits from
        // the abstract struct `B`; it yields the same elements." Core
        // declares no candidate for it.
        if let Some(ty) = self.parent_list(operator, public, &explicit, &given, expected) {
            // The value as `called` has an argument: read again where
            // its first reading was taken back, so that the locals it
            // declares are this reading's.
            let value = given.swap_remove(0);
            let converted = value.value.ty.clone();
            let value = self.argument_value(value, &converted, "the value converted");
            return (ExprKind::ParentList(Box::new(value)), ty);
        }
        // The generic arguments the call writes, for a requirement to
        // agree with (GEN-8).
        let said = generics.is_some() || named_generics;
        let written_generics = said.then(|| found.clone());

        // Then its arguments, in the order the operator declares its
        // parameters (GEN-12), and its defaults: see
        // `Typer::output_requested`.
        let supplied = public_inputs(public, &given);
        let Requested {
            place: requested,
            among,
            generics: operator_generics,
            unfit,
        } = self.output_requested(operator, public, found, &supplied, said);
        // GEN-13: a default the call takes is of its parameter's type
        // as the call resolves it: the `2` of
        // `operator choose<T, const fallback: T = 2>` is no `str`. One
        // that is not of the type as it is declared is reported there.
        if let Some(unfit) = unfit {
            let at = Location::new(self.file, name.span());
            self.diagnostics.emit(unfit.diagnostic(public, at));
            return (self.unselected(given, Form::Named), Type::Unknown);
        }
        // GEN-7: "A generic body is checked under its stated
        // requirements": what the enclosing declaration requires of its
        // generic parameters is callable in its body. The requirement
        // is of the call as it asks: one that gives another output than
        // the call requests, its defaults included, is not this call.
        // Named public generics are already in `written_generics` (GEN-8).
        let value_argument = |argument: &Given| argument.key.generic_parameter(public).is_none();
        if let Some(result) = self.required(
            operator,
            &given
                .iter()
                .filter(|argument| value_argument(argument))
                .collect::<Vec<_>>(),
            (
                requested.as_ref(),
                replay.map_or(expected, |call| call.source_expected),
            ),
            written_generics.as_deref(),
        ) {
            given.retain(value_argument);
            let at = Location::new(self.file, name.span());
            return self.call_required(
                operator,
                result,
                given,
                operator_generics,
                (Form::Named, at),
            );
        }
        // True where the output requested is the type the call writes
        // for the operator's result that is a type parameter and nothing
        // more: the `i64?` of `get<i64?>(global_state, key)` for core's
        // `operator get<V, S, K>(store: S, key: K) -> V` (GEN-8).
        let result_written = requested.is_some()
            && bare_parameter(
                public.result.as_ref().unwrap_or(&Type::Unknown),
                &public.generics,
            )
            .is_some_and(|index| matches!(explicit[index], Some(Argument::Type(_))));
        // GEN-5: "the requested output before inputs". EXPR-5: an
        // integer literal is an `f64` where one is expected; it selects
        // as the `i64` it is where a candidate takes one.
        let wanted = Wanted {
            place: requested
                .as_ref()
                .or(expected.filter(|expected| !open(expected))),
            among,
            written: result_written,
        };
        let (mut selected, untold) = self.admitted(operator, &candidates, &given, &wanted);
        // EXPR-12: "Resolve configuration that selects an expression's
        // source result type during source checking." An option a
        // candidate's condition reads is given here as no constant this
        // pass reads: which candidates admit the call cannot be told,
        // and the call is not judged. GEN-7: "A generic body is checked
        // under its stated requirements": where that is for a generic
        // parameter of the declaration being read, what its `requires`
        // states settles it ([`Typer::select_per_instance`]).
        let named = Location::new(self.file, name.span());
        let written = list.parent().map_or(named, |call| self.at(&call));
        let provisional = (selected.len() == 1).then(|| &mut selected[0]);
        match self.select_per_instance(
            operator,
            &given,
            (&wanted, requested.as_ref(), &operator_generics),
            (Form::Named, written, named),
            (untold, provisional),
        ) {
            Settled::Deferred(ty) => return (self.unselected(given, Form::Named), ty),
            Settled::Rejected(report) => {
                self.diagnostics.emit(report);
                return (self.unselected(given, Form::Named), Type::Unknown);
            }
            Settled::Elsewhere if untold => {
                return (self.unselected(given, Form::Named), Type::Unknown);
            }
            Settled::Elsewhere => {}
        }
        // An argument could not be tried once the function's readings
        // were used up (`REREADS`): the call is then not judged.
        if self.rereads == 0 && given.iter().any(|given| given.by_place) {
            return (self.unselected(given, Form::Named), Type::Unknown);
        }
        let at = Location::new(self.file, name.span());
        if selected.len() != 1 {
            // GEN-5: "the requested output before inputs". Where what
            // the place expects is not known, it may be what tells tied
            // candidates apart: the `make()` of `[make()]` given to a
            // `list<i64>`. The tie is then not reported, and the call
            // has no type.
            if selected.len() > 1 && expected.is_some_and(open) {
                return (self.unselected(given, Form::Named), Type::Unknown);
            }
            let diagnostic = self.no_one_candidate(
                (name.text(), at),
                public,
                (&given, &wanted),
                (!candidates.is_empty(), &selected),
            );
            self.diagnostics.emit(diagnostic);
            return (self.unselected(given, Form::Named), Type::Unknown);
        }
        let Some(chosen) = selected.pop() else {
            return (self.unselected(given, Form::Named), Type::Unknown);
        };
        if let Some(diagnostic) = self.ambiguous_result_reads(&chosen, at) {
            self.diagnostics.emit(diagnostic);
            return (self.unselected(given, Form::Named), Type::Unknown);
        }
        self.called(
            operator,
            chosen,
            given,
            operator_generics,
            (Form::Named, at),
        )
    }

    /// GEN-5 and GEN-7: reapply named selection to the original written
    /// call with the types and categories of its settled arguments.
    pub(super) fn settle_operator(&mut self, call: &Expr) -> Option<(ExprKind, Type)> {
        let ExprKind::Call {
            callee: Callee::DeferredOperator(deferred),
            arguments,
            ..
        } = &call.kind
        else {
            return None;
        };
        let ready = arguments
            .iter()
            .zip(&deferred.contextual)
            .all(|(argument, contextual)| {
                *contextual
                    || whole(&argument.value.ty)
                        && self.category(&argument.value) != ValueCategory::Unsettled
            });
        if !ready {
            return None;
        }
        let list = self.node_at(deferred.list)?;
        let parent = list.parent()?;
        let callable = if deferred.captured {
            Some(arguments_of(&list).into_iter().next()?.1)
        } else {
            None
        };
        let named = match &callable {
            Some(callable) => callable.clone(),
            None if parent.kind() == SyntaxKind::TypedConstructor => parent,
            None => parent.children().next()?,
        };
        let name = name_token(&named)?;
        let map: Vec<_> = self
            .instance
            .iter()
            .map(|(at, argument)| (*at, argument))
            .collect();
        let expected = deferred
            .expected
            .as_ref()
            .map(|ty| substitute_within(ty, &map));
        let written = deferred.generics.as_ref().map(|given| CallArguments {
            at: given.at,
            arguments: given
                .arguments
                .iter()
                .map(|(at, argument)| (*at, crate::instance::placed(argument, &map)))
                .collect(),
        });
        self.index_instance_locals();
        let previous = std::mem::replace(&mut self.callable, callable);
        let selected = self.call_operator_with(
            &name,
            &list,
            deferred.identity,
            expected.as_ref(),
            written.as_ref(),
            Some(ReplayedCall {
                arguments,
                contextual: &deferred.contextual,
                source_expected: deferred.expected.as_ref(),
            }),
        );
        self.callable = previous;
        Some(selected)
    }

    /// Read the arguments in source order. FN-17's static arguments
    /// take their context from the operator's public function template.
    fn read_operator_arguments(
        &mut self,
        public: &Signature,
        written: impl IntoIterator<Item = (Key, SyntaxNode)>,
        context: Option<&[Option<Argument>]>,
        pack: Option<&hir::TypePackArgument>,
        replay: Option<ReplayedCall<'_>>,
    ) -> Vec<Given> {
        let resolved: Vec<_> = context
            .unwrap_or_default()
            .iter()
            .map(|value| value.clone().unwrap_or(Argument::Type(Type::Unknown)))
            .collect();
        let map: Vec<_> = public
            .generics
            .iter()
            .map(|generic| generic.at)
            .zip(&resolved)
            .collect();
        let fixed = public
            .parameters
            .iter()
            .filter(|parameter| !parameter.configuration && parameter.pack.is_none())
            .count();
        written
            .into_iter()
            .enumerate()
            .map(|(position, (key, node))| {
                let explicit = match &key {
                    Key::Position(position) => position
                        .checked_sub(fixed)
                        .and_then(|index| pack?.types.get(index)),
                    _ => None,
                };
                if let Some(replayed) = replay {
                    let by_place = replayed.contextual[position];
                    Given {
                        key,
                        node,
                        value: replayed.arguments[position].value.clone(),
                        by_place,
                        explicit: explicit.cloned(),
                        retried: by_place,
                    }
                } else if context.is_some()
                    && let Some(pattern) = public_function(public, &key)
                    && let Type::Function(template) = substitute_within(pattern, &map)
                {
                    let reported = self.diagnostics.len();
                    let value = self.function_argument(&node, &template);
                    if self.diagnostics.len() == reported && !whole(&value.ty) {
                        self.diagnostics.emit(error(
                            Category::Type,
                            codes::GENERIC_UNRESOLVED,
                            self.at(&node),
                            "the call does not determine every type in this function argument"
                                .to_string(),
                            "FN-17",
                        ));
                    }
                    Given {
                        key,
                        node,
                        value,
                        by_place: false,
                        explicit: None,
                        retried: false,
                    }
                } else if let Some(ty) = explicit {
                    let value = self.expression(&node, Some(ty));
                    let value = self.fit_input(value, ty, "the explicitly typed pack argument");
                    Given {
                        key,
                        node,
                        value,
                        by_place: false,
                        explicit: Some(ty.clone()),
                        retried: false,
                    }
                } else {
                    self.give(key, node)
                }
            })
            .collect()
    }

    /// ERR-15: equally few result reads in different operands leave no
    /// way to choose which values the candidate takes.
    pub(super) fn ambiguous_result_reads(
        &self,
        chosen: &Chosen,
        at: Location,
    ) -> Option<Diagnostic> {
        chosen.ambiguous_reads.then(|| {
            let candidate = &self.program.modules[chosen.module].declarations[chosen.declaration];
            error(
                Category::Operator,
                codes::OPERATOR_AMBIGUOUS,
                at,
                "one operator candidate takes the call in more than one arrangement of result reads".to_string(),
                "ERR-15",
            )
            .with_related(candidate.at, "this candidate")
        })
    }

    /// The report of a call of an operator, named `name` and written at
    /// `at`, that no candidate admits, or that more than one
    /// most-specific candidate admits equally: GEN-5, "No match rejects;
    /// tied most-specific candidates are ambiguous." `any` is whether the
    /// operator has a candidate to select among, and `selected` the most
    /// specific of those that admit the call.
    pub(super) fn no_one_candidate(
        &self,
        (name, at): (&str, Location),
        public: &Signature,
        (given, wanted): (&[Given], &Wanted),
        (any, selected): (bool, &[Chosen]),
    ) -> Diagnostic {
        let program = self.program;
        let takes: Vec<String> = given
            .iter()
            .map(|given| {
                let value = if given.by_place {
                    given.node.text().to_string().trim().to_string()
                } else {
                    given.ty().to_string()
                };
                match &given.key {
                    // An option is known by its name.
                    key if key.public_parameter(public).is_none() && key.name().is_some() => {
                        let name = key.name().expect("named operator option");
                        format!("`{name}: {value}`")
                    }
                    _ => format!("`{value}`"),
                }
            })
            .collect();
        let takes = if takes.is_empty() {
            "no argument".to_string()
        } else {
            takes.join(", ")
        };
        if selected.is_empty() {
            let message = if !any {
                format!(
                    "operator `{}` has no value candidate, and a value function selects \
                     among those",
                    name
                )
            } else {
                format!(
                    "no value candidate of operator `{}` takes {takes}{}",
                    name,
                    match (wanted.place, &wanted.among) {
                        (Some(wanted), _) => format!(" and gives `{wanted}`"),
                        // What the operator's requirement admits.
                        (None, Some(among)) => {
                            let among: Vec<String> =
                                among.iter().map(|ty| format!("`{ty}`")).collect();
                            format!(" and gives {}", among.join(" or "))
                        }
                        (None, None) => String::new(),
                    }
                )
            };
            error(
                Category::Operator,
                codes::OPERATOR_NO_CANDIDATE,
                at,
                message,
                "GEN-5",
            )
        } else {
            let mut diagnostic = error(
                Category::Operator,
                codes::OPERATOR_AMBIGUOUS,
                at,
                format!(
                    "{} value candidates of operator `{}` take {takes} equally",
                    selected.len(),
                    name
                ),
                "GEN-5",
            );
            for chosen in selected {
                let candidate = &program.modules[chosen.module].declarations[chosen.declaration];
                diagnostic = diagnostic.with_related(candidate.at, "this candidate");
            }
            diagnostic
        }
    }

    /// The list a call of `operator` converts its one argument to, where
    /// the call is core's `convert<list<B>>(values)` of a `list<A>` and
    /// the struct `A` inherits from the abstract struct `B` (core's
    /// OPS-6). `public` is the operator's signature, `explicit` the
    /// generic arguments the call writes, in its order, and `expected`
    /// what its place expects.
    ///
    /// Core declares no candidate for it: "A list of structs to a list
    /// of their parent is not declared here." It takes no option, and
    /// "yields the same elements", so it cannot fail and is the list
    /// asked for (OPS-8: "A conversion that cannot fail returns T"); a
    /// place that expects another type, a `result` among them, is
    /// matched by no candidate (GEN-5). So is another generic argument
    /// the call writes: the value's type, `V`, is what it is given, and
    /// the output, `O`, is T. A list of a struct converted to a list of
    /// that struct, a list of a size, and a list of nullable elements
    /// are none of it.
    fn parent_list(
        &self,
        operator: OperatorRef,
        public: &Signature,
        explicit: &[Option<Argument>],
        given: &[Given],
        expected: Option<&Type>,
    ) -> Option<Type> {
        let program = self.program;
        let declared = &program.modules[operator.module].declarations[operator.declaration];
        if Some(operator.module) != program.core() || declared.name != "convert" {
            return None;
        }
        let Some(Argument::Type(
            asked @ Type::List {
                element: parent,
                size: None,
            },
        )) = explicit.first()?
        else {
            return None;
        };
        if expected.is_some_and(|expected| !self.admits(expected, asked)) {
            return None;
        }
        // The one argument, by position or by the operator's name for
        // its parameter (EXPR-2).
        let ([value], [parameter]) = (given, public.parameters.as_slice()) else {
            return None;
        };
        let named = value.key.public_parameter(public) == Some(0);
        if !named {
            return None;
        }
        let Type::List {
            element: child,
            size: None,
        } = &value.value.ty
        else {
            return None;
        };
        // Each other generic argument the call writes: the parameter's
        // type, which is the value's, or the result, which is T.
        for (generic, written) in public.generics.iter().zip(explicit).skip(1) {
            let Some(written) = written else {
                continue;
            };
            let names = |ty: &Type| matches!(ty, Type::Parameter { at, .. } if *at == generic.at);
            let stands_for = if names(&parameter.ty) {
                &value.value.ty
            } else if public.result.as_ref().is_some_and(names) {
                asked
            } else {
                return None;
            };
            if !matches!(written, Argument::Type(ty) if agree(ty, stands_for)) {
                return None;
            }
        }
        let (
            Type::Named {
                kind: NamedKind::Struct,
                ..
            },
            Type::Named {
                module,
                declaration,
                kind: NamedKind::Struct,
                ..
            },
        ) = (&**child, &**parent)
        else {
            return None;
        };
        let abstract_parent = has_token(
            &program.modules[*module].declarations[*declaration].syntax,
            SyntaxKind::KwAbstract,
        );
        (abstract_parent && !agree(child, parent) && self.inherits(child, parent))
            .then(|| asked.clone())
    }

    /// The value candidates of `operator`, wherever they are declared,
    /// and in composition and value code its temporal ones too. GEN-5:
    /// "Library membership and registration order grant no priority."
    pub(super) fn value_candidates(&mut self, operator: OperatorRef) -> Vec<Candidate<'a>> {
        let all = self.graph || self.value_code;
        let key = (operator.module, operator.declaration, all);
        if let Some(found) = self.candidates.get(&key) {
            return found.clone();
        }
        let program = self.program;
        let mut candidates = Vec::new();
        for (module, declared) in program.modules.iter().enumerate() {
            for (declaration, candidate) in declared.declarations.iter().enumerate() {
                // GEN-5: "A call in a value function or a runtime body
                // selects among the value candidates. A call in a test's
                // or an application's value code (TST-3, UNIT-14)
                // selects among all of them ... A call in composition
                // selects among all of them".
                let admitted = match candidate.kind.candidate() {
                    Some((Role::Value, Some(of))) => of == operator,
                    Some((Role::Temporal, Some(of))) => all && of == operator,
                    _ => false,
                };
                if !admitted {
                    continue;
                }
                if let Some(Detail::Callable(signature)) = declared.details.get(declaration) {
                    candidates.push((module, declaration, signature));
                }
            }
        }
        self.candidates.insert(key, candidates.clone());
        candidates
    }

    /// One argument of an operator's call, read with nothing expected
    /// of it: what it reports itself is reported here, once.
    pub(super) fn give(&mut self, key: Key, node: SyntaxNode) -> Given {
        let by_place = typed_by_place(&node);
        let mark = self.mark();
        let value = self.expression(&node, Some(&Type::Unknown));
        // An argument read as the type of its place is read again once
        // the candidate is known. Its first reading, when it reports
        // nothing, is taken back but for what it assigned, which a
        // later argument may read.
        let retried = by_place && self.diagnostics.len() == mark.diagnostics && self.may_reread();
        if retried {
            self.take_back(mark, true);
        }
        Given {
            key,
            by_place,
            value,
            explicit: None,
            retried,
            node,
        }
    }

    /// The arguments of an operator's call as they are written, each
    /// with its position or its name. What is wrong with the call
    /// whichever candidate takes it is reported here (EXPR-2): an
    /// argument with no name after a named one; a name that neither the
    /// operator nor any candidate declares; a name given twice, or a
    /// parameter of the operator given in position and by name.
    ///
    /// Which parameter an option's argument is given to differs from
    /// candidate to candidate, so that is not judged here: a candidate
    /// the arguments do not fit is not admitted (GEN-6).
    fn operator_arguments(
        &mut self,
        callee: &SyntaxToken,
        list: &SyntaxNode,
        public: &Signature,
        options: &OperatorOptions<'_>,
        written_pack: Option<&hir::TypePackArgument>,
    ) -> Vec<(Key, SyntaxNode)> {
        let mut named_seen = false;
        let mut position = 0;
        let mut names = HashSet::new();
        let mut filled = HashSet::new();
        let mut arguments = Vec::new();
        let fixed = public
            .parameters
            .iter()
            .filter(|parameter| !parameter.configuration && parameter.pack.is_none())
            .count();
        let has_named_pack = public
            .parameters
            .iter()
            .any(|parameter| parameter.pack == Some(Pack::Named));
        let mut positional = Vec::new();
        let mut named = Vec::new();
        for (brace, name, value) in self.call_entries(list) {
            let at = name.as_ref().map_or_else(
                || self.at(&value),
                |name| Location::new(self.file, name.span()),
            );
            let key = match name {
                None => {
                    if named_seen {
                        self.diagnostics.emit(error(
                            Category::Type,
                            codes::ARGUMENT_ORDER,
                            at,
                            "an argument with no name follows a named one: positional \
                             arguments come first"
                                .to_string(),
                            "EXPR-2",
                        ));
                    }
                    // FN-18: a surplus positional argument cannot fill a
                    // configuration slot, even where it has no default.
                    if !options.configuration.is_empty()
                        && options
                            .positional_limit
                            .is_some_and(|maximum| position >= maximum)
                    {
                        self.diagnostics.emit(error(
                            Category::Type,
                            codes::ARGUMENT_UNEXPECTED,
                            at,
                            format!("`{}` takes no positional argument here", callee.text()),
                            "FN-18",
                        ));
                    }
                    if position >= fixed {
                        positional.push(at);
                    }
                    position += 1;
                    Key::Position(position - 1)
                }
                Some(name) => {
                    // A named brace entry does not put the parentheses
                    // into their named-argument portion (EXPR-2, EXPR-10).
                    named_seen |= !brace;
                    options.name_key(name.text(), brace)
                }
            };
            let own = key.public_parameter(public);
            let mut duplicate = own.is_some_and(|index| !filled.insert(index));
            if let Some(text) = key.name() {
                let known = own.is_some() || options.accepts_name(&key);
                let captured =
                    !known && has_named_pack && !(options.configuration.contains(text) && !brace);
                if !known && !captured {
                    self.diagnostics.emit(error(
                        Category::Name,
                        codes::ARGUMENT_UNKNOWN,
                        at,
                        format!("`{}` has no parameter `{text}`", callee.text()),
                        if brace || options.configuration.contains(text) {
                            "FN-18"
                        } else {
                            "EXPR-2"
                        },
                    ));
                } else {
                    duplicate |= !names.insert(text.to_string());
                    if captured && !duplicate {
                        named.push(at);
                    }
                }
            }
            if duplicate {
                let name = own.map_or_else(
                    || key.name().unwrap_or_default(),
                    |index| public.parameters[index].name.as_str(),
                );
                self.diagnostics.emit(error(
                    Category::Name,
                    codes::ARGUMENT_DUPLICATE,
                    at,
                    format!("`{name}` is given twice"),
                    "EXPR-2",
                ));
            }
            arguments.push((key, value));
        }
        for parameter in &public.parameters {
            let captured = match parameter.pack {
                Some(Pack::Positional) => &positional,
                Some(Pack::Named) => &named,
                None => continue,
            };
            let bounds = pack_bounds(parameter, &public.generics, written_pack);
            self.check_pack_count(list, &parameter.name, bounds, captured);
        }
        arguments
    }

    /// GEN-8: public named generic values take part in the requested
    /// result even where a candidate specializes it without a generic.
    pub(super) fn operator_generic_arguments<'s>(
        &mut self,
        signature: &Signature,
        written: impl Iterator<Item = (&'s Key, &'s SyntaxNode)>,
        found: &mut [Option<Argument>],
    ) -> bool {
        let mut said = false;
        for (key, node) in written {
            let Some(index) = key.generic_parameter(signature) else {
                continue;
            };
            let generic = &signature.generics[index];
            if found[index].is_some() {
                self.diagnostics.emit(error(
                    Category::Name,
                    codes::ARGUMENT_DUPLICATE,
                    self.at(node),
                    format!("`{}` is given twice", generic.name),
                    "EXPR-2",
                ));
                continue;
            }
            let expected = match &generic.kind {
                GenericKind::Value(ty) if !names_parameter(ty) => Some(ty),
                _ => None,
            };
            let read = self.read_constant(node, expected);
            if let Some(why) = Self::not_constant(&read) {
                let name = &generic.name;
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::GENERIC_ARGUMENT_CONSTANT,
                    self.at(node),
                    format!("`{name}` is a generic value parameter, and this is no compile-time constant: {why}"),
                    "GEN-8",
                ));
            }
            found[index] = Some(Argument::Constant(Constant {
                text: constant_text(node),
                value: read.unwrap_or(ConstantValue::Unknown),
            }));
            said = true;
        }
        said
    }

    /// FN-18: the operator owns the required entries in its braces.
    /// Candidate-only options are still judged during selection (GEN-6).
    pub(super) fn missing_configuration(
        &mut self,
        public: &Signature,
        given: &[Given],
        name: &str,
        at: Location,
    ) -> bool {
        let mut missing = false;
        for parameter in public.parameters.iter().filter(|parameter| {
            parameter.configuration
                && !parameter.has_default
                && !matches!(parameter.ty, Type::Nullable(_))
        }) {
            if given.iter().any(
                |given| matches!(&given.key, Key::Configuration(name) if *name == parameter.name),
            ) {
                continue;
            }
            missing = true;
            self.diagnostics.emit(error(
                Category::Type,
                codes::ARGUMENT_MISSING,
                at,
                format!(
                    "`{name}` is given no `{}`, a configuration parameter with no default",
                    parameter.name
                ),
                "FN-18",
            ));
        }
        missing
    }

    /// The argument `given` as the value of a parameter of type `ty`,
    /// fitted to it (EXPR-5). One whose first reading was taken back
    /// (`retried`) is read again where it stands, as that type: what
    /// the first reading declared is gone, and the value read again
    /// declares its own. `place` names the parameter for messages.
    pub(super) fn argument_value(&mut self, given: Given, ty: &Type, place: &str) -> Expr {
        let value = self.argument_read(given, ty);
        self.fit(value, ty, place)
    }

    /// Reads an argument before its place takes its form.
    pub(super) fn argument_read(&mut self, given: Given, ty: &Type) -> Expr {
        if given.retried {
            self.expression(&given.node, Some(ty))
        } else {
            given.value
        }
    }

    /// The call of an operator for which no candidate is selected: its
    /// arguments as they were read, and no callee. One whose first
    /// reading was taken back is read where it stands.
    pub(super) fn unselected(&mut self, given: Vec<Given>, form: Form) -> ExprKind {
        let arguments = given
            .into_iter()
            .map(|given| hir::Argument {
                parameter: None,
                pack: None,
                value: if given.retried {
                    self.expression(&given.node, Some(&Type::Unknown))
                } else {
                    given.value
                },
            })
            .collect();
        ExprKind::Call {
            callee: Callee::Unread,
            arguments,
            form,
            braced: Vec::new(),
            lifted: false,
        }
    }
}

/// FN-17: the public function template of the parameter the call gives.
fn public_function<'s>(public: &'s Signature, key: &Key) -> Option<&'s Type> {
    let parameter = &public.parameters[key.public_parameter(public)?];
    matches!(parameter.ty, Type::Function(_)).then_some(&parameter.ty)
}

pub(super) fn public_inputs<'a>(public: &Signature, given: &'a [Given]) -> Vec<Option<&'a Type>> {
    let mut inputs = vec![None; public.parameters.len()];
    for argument in given {
        if let Some(index) = argument.key.public_parameter(public) {
            inputs[index] = Some(argument.ty());
        }
    }
    inputs
}

/// The note on a call in a generic body that no candidate is known to
/// admit: whether one does depends on a generic parameter of the body,
/// which its `requires` does not settle (GEN-7).
pub(super) const UNSETTLED: &str = "which candidate admits this depends on a generic parameter of \
     the declaration it is written in, which its `requires` does not settle: a generic body \
     is checked under its stated requirements (GEN-7), so state the operation it calls, \
     `requires name(T) -> R`, the parameter's value, `requires M == .value`, or the types or \
     values it may be, `requires T in {i64, f64}`";

/// The names available before selection, across the operator and its candidates.
pub(super) struct OperatorOptions<'a> {
    /// Generic value parameters used as candidate selectors (GEN-6).
    selecting: HashSet<&'a str>,
    /// Parameters that only the braces can supply (FN-18).
    configuration: HashSet<&'a str>,
    /// Ordinary candidate-only options (GEN-6).
    ordinary: HashSet<&'a str>,
    /// The largest number of positional arguments any candidate can take,
    /// or no limit here because a signature has a positional pack.
    positional_limit: Option<usize>,
}

impl OperatorOptions<'_> {
    /// An EXPR-10 brace entry first names a configuration parameter.
    /// Otherwise the existing generic and ordinary option binding applies.
    pub(super) fn name_key(&self, name: &str, brace: bool) -> Key {
        if brace && self.configuration.contains(name) {
            Key::Configuration(name.to_string())
        } else if self.selecting.contains(name) {
            Key::Generic(name.to_string())
        } else if brace {
            Key::Brace(name.to_string())
        } else {
            Key::Name(name.to_string())
        }
    }

    /// Whether the name belongs to an option in the argument's section.
    pub(super) fn accepts_name(&self, key: &Key) -> bool {
        key.name().is_some_and(|name| {
            matches!(key, Key::Configuration(_) | Key::Generic(_)) || self.ordinary.contains(name)
        })
    }
}

pub(super) fn options_of<'a>(
    public: &'a Signature,
    candidates: &[Candidate<'a>],
) -> OperatorOptions<'a> {
    let mut options = OperatorOptions {
        selecting: HashSet::new(),
        configuration: HashSet::new(),
        ordinary: HashSet::new(),
        positional_limit: Some(0),
    };
    for signature in
        std::iter::once(public).chain(candidates.iter().map(|(_, _, signature)| *signature))
    {
        options.selecting.extend(
            signature
                .generics
                .iter()
                .filter(|generic| matches!(generic.kind, GenericKind::Value(_)))
                .map(|generic| generic.name.as_str()),
        );
        options.configuration.extend(
            signature
                .parameters
                .iter()
                .filter(|parameter| parameter.configuration)
                .map(|parameter| parameter.name.as_str()),
        );
        options.positional_limit = options.positional_limit.and_then(|maximum| {
            (!signature
                .parameters
                .iter()
                .any(|parameter| parameter.pack == Some(Pack::Positional)))
            .then(|| {
                maximum.max(
                    signature
                        .parameters
                        .iter()
                        .filter(|parameter| !parameter.configuration && parameter.pack.is_none())
                        .count(),
                )
            })
        });
    }
    for &(_, _, signature) in candidates {
        let shared: HashSet<_> = public.candidate_parameters(signature).flatten().collect();
        options.ordinary.extend(
            signature
                .parameters
                .iter()
                .enumerate()
                .filter(|(index, parameter)| !parameter.configuration && !shared.contains(index))
                .map(|(_, option)| option.name.as_str()),
        );
    }
    options
}

/// What a call of an operator requests of its candidates: see
/// [`Typer::output_requested`].
pub(super) struct Requested {
    /// The operator's result with what is known of its generic
    /// parameters in place, when the call says something of it.
    pub(super) place: Option<Type>,
    /// The types the operator's own requirement admits for its result.
    pub(super) among: Option<Vec<Type>>,
    /// The operator's resolved generic arguments, for any configuration defaults.
    pub(super) generics: Vec<Argument>,
    /// A default the call takes that is not of its parameter's type as
    /// the call resolves it (GEN-13). Nothing is then requested.
    pub(super) unfit: Option<Unfit>,
}

/// A generic value parameter's default that does not fit where a call
/// takes it.
pub(super) struct Unfit {
    /// The parameter's position among the operator's generics.
    generic: usize,
    /// What the default is, for the message: `an \`i64\``.
    described: String,
    /// The parameter's type as the call resolves it.
    resolved: Type,
}

impl Unfit {
    /// GEN-13: a default does not fit the type the operator requested.
    pub(super) fn diagnostic(&self, public: &Signature, at: Location) -> Diagnostic {
        let parameter = &public.generics[self.generic].name;
        let Self {
            described,
            resolved,
            ..
        } = self;
        error(
            Category::Type,
            codes::TYPE_MISMATCH,
            at,
            format!(
                "this leaves `{parameter}` to its default, {described}, and `{parameter}` is `{resolved}` here"
            ),
            "GEN-13",
        )
    }
}

/// An inherited configuration default can read the operator's generic
/// arguments even when the call writes none (GEN-6, UNIT-7).
pub(super) fn has_generic_configuration_default(public: &Signature) -> bool {
    !public.generics.is_empty()
        && public
            .parameters
            .iter()
            .any(|parameter| parameter.configuration && parameter.has_default)
}

/// What the place a call stands in says of the operator's generic
/// parameters (GEN-2). The candidate can resolve a result parameter
/// from its own inputs where the place leaves its type open.
pub(super) fn from_place(
    public: &Signature,
    expected: Option<&Type>,
    found: &mut [Option<Argument>],
) {
    if let (Some(result), Some(expected)) = (&public.result, expected)
        && !open(expected)
    {
        let _ = infer_from_place(result, expected, false, &public.generics, found);
    }
}

/// True when the call cannot bind the public or candidate pack slots.
/// Duplicate packs and fixed candidate parameters in pack slots are
/// reported at their declarations (FN-8, GEN-6).
pub(super) fn unreadable(public: &Signature, candidates: &[(usize, usize, &Signature)]) -> bool {
    let duplicate = |signature: &Signature| {
        [Pack::Positional, Pack::Named].into_iter().any(|kind| {
            signature
                .parameters
                .iter()
                .filter(|parameter| parameter.pack == Some(kind))
                .nth(1)
                .is_some()
        })
    };
    duplicate(public)
        || candidates.iter().any(|(_, _, signature)| {
            duplicate(signature)
                || public
                    .parameters
                    .iter()
                    .zip(public.candidate_parameters(signature))
                    .any(|(parameter, index)| {
                        index
                            .is_some_and(|index| signature.parameters[index].pack != parameter.pack)
                            || index.is_none()
                                && (parameter.configuration || parameter.pack.is_some())
                    })
        })
}

/// GEN-6: the operator owns the defaults of its configuration parameters.
/// Copy the checked expression to each candidate's corresponding slot,
/// keeping the operator's original bindings and source location (UNIT-7).
pub(super) fn inherit_configuration_defaults(program: &mut Program) {
    let mut inherited = Vec::new();
    for (module, unit) in program.modules.iter().enumerate() {
        for (index, declaration) in unit.declarations.iter().enumerate() {
            let (Some((_, Some(operator))), Some(Detail::Callable(candidate))) =
                (declaration.kind.candidate(), unit.details.get(index))
            else {
                continue;
            };
            let owner = &program.modules[operator.module];
            let Some(Detail::Callable(public)) = owner.details.get(operator.declaration) else {
                continue;
            };
            for (public_index, place) in public.candidate_parameters(candidate).enumerate() {
                if !public.parameters[public_index].configuration {
                    continue;
                }
                if let Some(place) = place
                    && let Some(Some(default)) =
                        owner.defaults[operator.declaration].get(public_index)
                {
                    inherited.push((module, index, place, default.clone()));
                }
            }
        }
    }
    for (module, index, place, default) in inherited {
        program.modules[module].defaults[index][place] = Some(default);
    }
}
