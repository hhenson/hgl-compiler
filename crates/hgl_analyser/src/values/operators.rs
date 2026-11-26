//! A call of an operator, by its name or where a symbol stands for it:
//! the output it requests, its arguments as they are written, and the
//! operator's value candidates (GEN-5, GEN-6, EXPR-2). Part of the typing
//! pass ([`super`]); the candidate is selected in [`super::candidates`].

use std::collections::HashSet;

use hgl_diagnostics::{Category, Diagnostic, Location, Severity};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};

use super::callback_evidence::CallbackEvidence;
use super::calls::arguments_of;
use super::candidates::{
    Chosen, DependentCallback, Given, Key, Settled, Wanted, bound, typed_by_place,
};
use super::constants::{constant_text, generic_default};
use super::dependent_packs::{
    PackTypes, dependent_slot, operator_packs, pack_exclusions, reflects, specialize_named_pack,
};
use super::expressions::written_null;
use super::fit::{fitted_input_pattern, is_integer_literal, negated, open, whole};
use super::forms::received_type;
use super::function_arguments::CallbackInputs;
use super::inference::{
    ExplicitGenerics, bare_parameter, defaulted_argument, infer, infer_from_place, mentions,
    pack_bounds, placed, unsaid,
};
use super::lifting::contextual_type_is_pending;
use super::{Candidate, Typer};
use crate::declared::{agree, names_parameter, substitute_within};
use crate::hir::{self, Callee, Expr, ExprKind, Form, ValueCategory};
use crate::program::{Declaration, DeclarationKind, DependentPack, OperatorRef, Program, Role};
use crate::syntax::{child, has_token, name_token, ungrouped};
use crate::types::{
    Argument, CallArguments, Constant, ConstantValue, Detail, GenericKind, NamedKind, Pack, Scalar,
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

/// Whether a named operator can select its candidate with the current inputs.
enum OperatorRead<'a> {
    Source,
    /// FN-19: read the written arguments while a computed spread awaits its keys.
    BeforeSpread,
    /// GEN-7: a graph operand has acquired its instance type.
    Replay(ReplayedCall<'a>),
}

/// The source settings needed to check the public signature after a candidate
/// has independently determined a bare callback's type (GEN-4, GEN-17).
#[derive(Debug, Clone)]
pub(super) struct ReflectedCall {
    name: SyntaxToken,
    list: SyntaxNode,
    /// Generics known from the call before reading any candidate's callback.
    pub(super) found: Vec<Option<Argument>>,
    said: bool,
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
    /// Named operator requirements (GEN-16) retain complete inference
    /// here. Value calls use `argument_output_requested`.
    pub(super) fn output_requested(
        &self,
        operator: OperatorRef,
        public: &Signature,
        found: Vec<Option<Argument>>,
        supplied: &[Option<&Type>],
        said: bool,
    ) -> Requested {
        self.request_output(operator, public, found, supplied, said, None)
    }

    /// GEN-5, GEN-6, GEN-8: literal-aware calls keep generic value options
    /// without turning a literal's preliminary type into a result request.
    /// GEN-7 extends this to generic-dependent literals and instance rereads.
    /// Every candidate still checks every input.
    pub(super) fn argument_output_requested(
        &self,
        operator: OperatorRef,
        public: &Signature,
        found: Vec<Option<Argument>>,
        given: &[Given],
        said: bool,
    ) -> Requested {
        let supplied = public_inputs(public, given);
        // GEN-2: a typed argument still decides its generic. Only literal,
        // contextual and callback readings wait for the candidate; an
        // unrelated literal flag cannot erase another argument's type.
        // GEN-7: a generic-dependent literal is fitted by each instance's
        // candidate. Its preliminary type cannot request that result. The
        // instance reread uses the same rule as the written-out call.
        let own: Vec<_> = self
            .signature
            .generics
            .iter()
            .map(|generic| generic.at)
            .collect();
        let instance_literal = given.iter().any(|argument| literal_input(&argument.value))
            && given.iter().any(|argument| mentions(argument.ty(), &own));
        let requested =
            (own.is_empty() || !self.instance.is_empty() || instance_literal).then(|| {
                let mut requested = found.clone();
                let mut fixed = supplied.clone();
                for argument in given {
                    if argument.explicit.is_none()
                        && (argument.by_place
                            || argument.by_function
                            || literal_input(&argument.value))
                        && let Some(index) = argument.key.public_parameter(public)
                    {
                        fixed[index] = None;
                    }
                }
                self.infer_operator_inputs(public, &mut requested, &fixed);
                requested
            });
        let mut request = self.request_output(operator, public, found, &supplied, said, requested);
        if !own.is_empty() {
            // UNIT-8: retain generic configuration-default metadata. GEN-7
            // changes the requested result here, not those defaults.
            request.call_generics = request.generics.clone();
        }
        request
    }

    fn request_output(
        &self,
        operator: OperatorRef,
        public: &Signature,
        mut found: Vec<Option<Argument>>,
        supplied: &[Option<&Type>],
        said: bool,
        requested_types: Option<Vec<Option<Argument>>>,
    ) -> Requested {
        let program = self.program;
        // GEN-5, GEN-6, GEN-8: a generic value default is a request for
        // that value, not for every input-derived type. Public checks below
        // still use every argument. Written types and the expected place
        // constrain a value call's result independently of input spelling.
        let value_call = requested_types.is_some();
        let requested_types = requested_types.unwrap_or_else(|| found.clone());
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
        let own_generics: Vec<Location> =
            public.generics.iter().map(|generic| generic.at).collect();
        let mut defaulted: Vec<(usize, SyntaxNode)> = Vec::new();
        for (index, generic) in public.generics.iter().enumerate() {
            if found[index].is_none()
                && let Some(default) = generic_default(operator_syntax, generic)
            {
                if let Some(constant) = self.default_constant(operator_declaration, generic) {
                    let Some(argument) =
                        defaulted_argument(program, constant, index, &own_generics, &found)
                    else {
                        return Requested::rejected(Unfit::Unresolved { generic: index });
                    };
                    found[index] = Some(argument);
                }
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
        let map: Vec<(Location, &Argument)> = public
            .generics
            .iter()
            .zip(&explicit)
            .filter(|(generic, _)| !matches!(generic.kind, GenericKind::Pack { .. }))
            .map(|(generic, argument)| (generic.at, argument))
            .collect();
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
                return Requested::rejected(Unfit::Mismatch {
                    generic: index,
                    described,
                    resolved: Box::new(resolved),
                });
            }
        }
        // The output requested, as far as it is known: a `list<T, 3>`
        // with `T` not yet said is a list of three of anything.
        let output_arguments: Vec<_> = public
            .generics
            .iter()
            .zip(&explicit)
            .zip(requested_types)
            .map(|((generic, inferred), requested)| match generic.kind {
                GenericKind::Plain | GenericKind::Type if value_call => {
                    requested.unwrap_or(Argument::Type(Type::Unknown))
                }
                _ => inferred.clone(),
            })
            .collect();
        let output_map: Vec<_> = public
            .generics
            .iter()
            .zip(&output_arguments)
            .filter(|(generic, _)| !matches!(generic.kind, GenericKind::Pack { .. }))
            .map(|(generic, argument)| (generic.at, argument))
            .collect();
        let place: Option<Type> = asked
            .then(|| {
                public
                    .result
                    .as_ref()
                    .map(|result| placed(substitute_within(result, &output_map), &own_generics))
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
                || reflects(&program.modules[operator.module], public)
            {
                explicit
            } else {
                Vec::new()
            },
            call_generics: if public
                .parameters
                .iter()
                .any(|parameter| parameter.configuration)
                || reflects(&program.modules[operator.module], public)
            {
                output_arguments
            } else {
                Vec::new()
            },
            type_packs: Vec::new(),
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
        let result = match &self.program.modules[operator.module].details[operator.declaration] {
            Detail::Callable(signature) => signature.result.as_ref(),
            _ => None,
        };
        if let Some(call) = self.defer_pack_call_with(list, result, expected, |typer| {
            typer
                .call_operator_with(
                    name,
                    list,
                    operator,
                    expected,
                    generics,
                    OperatorRead::BeforeSpread,
                )
                .0
        }) {
            return call;
        }
        self.call_operator_with(
            name,
            list,
            operator,
            expected,
            generics,
            OperatorRead::Source,
        )
    }

    /// Read the named call, reusing settled argument readings in an instance.
    fn call_operator_with(
        &mut self,
        name: &SyntaxToken,
        list: &SyntaxNode,
        operator: OperatorRef,
        expected: Option<&Type>,
        generics: Option<&CallArguments>,
        reading: OperatorRead<'_>,
    ) -> (ExprKind, Type) {
        let before_spread = matches!(reading, OperatorRead::BeforeSpread);
        let replay = match reading {
            OperatorRead::Replay(call) => Some(call),
            _ => None,
        };
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
        // They can also fix types of the operator's public parameters.
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
        let unit = &program.modules[operator.module];
        let dependent = dependent_slot(unit, public);
        let dependent_pack =
            dependent.map(|index| &unit.dependent_packs[&public.parameters[index].at]);
        let candidate_evidence = dependent.is_some()
            && std::iter::once((operator.module, operator.declaration, public))
                .chain(candidates.iter().copied())
                .any(|candidate| {
                    !CallbackEvidence::from_callables(program, [(candidate, &[][..], &[][..])])
                        .is_empty()
                });
        let signature = (operator, public, dependent_pack, candidate_evidence);
        let written = self.operator_arguments(
            name,
            list,
            (public, dependent),
            &options,
            pack.as_ref(),
            before_spread,
        );
        let named_generics = self.diagnostics.len() == reported
            && self.operator_generic_arguments(
                public,
                written.iter().map(|(key, node, _)| (key, node)),
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
                && written.iter().any(|(key, _, _)| {
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
                    signature,
                    written.iter().cloned(),
                    Some(&inferred),
                    pack.as_ref(),
                    replay,
                    &[],
                );
                self.take_back(mark, false);
                self.infer_operator_inputs(public, &mut inferred, &public_inputs(public, &probe));
            }
            context = Some(inferred);
        }
        let mut given = self.read_operator_arguments(
            signature,
            written,
            context.as_deref(),
            pack.as_ref(),
            replay,
            &candidates,
        );
        // FN-19: the operator supplies context for written arguments. Its
        // candidate is selected once the spread supplies the remaining slots.
        if before_spread {
            return (self.unselected(given, Form::Named), Type::Unknown);
        }
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
            .any(|given| !given.by_place && !given.by_function && open(given.ty()));
        if self.graph
            && replay.is_none()
            && !faulty
            && given.iter().any(|given| {
                !given.by_function
                    && if given.by_place {
                        self.contextual_operator_argument_pending(&given.value, false)
                    } else {
                        !whole(given.ty())
                    }
            })
        {
            // GEN-7: the arguments can settle only after an earlier
            // operator in this graph has selected its instance candidate.
            let deferred = hir::DeferredOperator {
                identity: operator,
                list: self.at(list),
                expected: expected.cloned(),
                generics: generics.cloned(),
                contextual: given
                    .iter()
                    .map(|given| given.by_place || given.by_function)
                    .collect(),
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
        if !self.written_operator_inputs_fit(public, &explicit, &given) {
            return (self.unselected(given, Form::Named), Type::Unknown);
        }
        let dictionary_fallback =
            match self.dictionary_get_fallback(operator, public, &candidates, &given) {
                Ok(DictionaryFallback::WrittenNull(index)) => {
                    given.remove(index);
                    false
                }
                Ok(DictionaryFallback::Value) => true,
                Ok(DictionaryFallback::Omitted) => false,
                Err(report) => {
                    self.diagnostics.emit(*report);
                    return (self.unselected(given, Form::Named), Type::Unknown);
                }
            };
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
        // A candidate can supply the type of a contextual callback. Keep
        // the original request if a reflected pack or a public default
        // may need the completed callback signature (FN-17, GEN-8).
        let dependent_callback = given
            .iter()
            .any(|argument| argument.dependent_callback.is_some());
        let can_supply_default = public
            .generics
            .iter()
            .any(|generic| matches!(generic.kind, GenericKind::Value(_)))
            && given.iter().any(|argument| argument.by_function);
        let trial_call = (dependent_callback || can_supply_default).then(|| ReflectedCall {
            name: name.clone(),
            list: list.clone(),
            found: found.clone(),
            said,
        });
        if let Some(callback) = given
            .iter_mut()
            .find_map(|argument| argument.dependent_callback.as_mut())
        {
            callback.call = trial_call.clone();
        }

        // Then its arguments, in the order the operator declares its
        // parameters (GEN-12), and its defaults: see
        // `Typer::output_requested`.
        let Requested {
            place: mut requested,
            among,
            generics: operator_generics,
            call_generics,
            type_packs: mut operator_type_packs,
            unfit,
        } = self.argument_output_requested(operator, public, found, &given, said);
        // ERR-6: the fallback form gives the payload type a caller writes.
        // A plain read still requests the whole result of that payload.
        if dictionary_fallback && let Some(Type::Result { value, .. }) = requested {
            requested = Some(*value);
        }
        // GEN-13: a default the call takes is of its parameter's type
        // as the call resolves it: the `2` of
        // `operator choose<T, const fallback: T = 2>` is no `str`. One
        // that is not of the type as it is declared is reported there.
        let pending_default = match unfit {
            Some(Unfit::Unresolved { .. }) if trial_call.is_some() => trial_call.as_ref(),
            Some(unfit) => {
                self.diagnostics
                    .emit(unfit.diagnostic(public, Location::new(self.file, name.span())));
                return (self.unselected(given, Form::Named), Type::Unknown);
            }
            None => None,
        };
        if !dependent_callback && pending_default.is_none() && reflects(unit, public) {
            match self.reflected_operator_call(
                (operator, public, dependent),
                (name, list),
                &operator_generics,
                &mut given,
            ) {
                Ok(reflected) => {
                    if let Some(projected) = reflected.projected {
                        requested = Some(projected);
                    }
                    operator_type_packs = reflected.packs;
                }
                Err(()) => return (self.unselected(given, Form::Named), Type::Unknown),
            }
        }
        // Public constraints and pack checks above use the original
        // inference. Only configuration defaults use the fitted argument
        // types completed by `called` (UNIT-8, GEN-6).
        let operator_generics = call_generics;
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
            let called = self.call_required(
                operator,
                result,
                given,
                operator_generics,
                (Form::Named, at),
            );
            return with_operator_packs(called, operator_type_packs);
        }
        // True where the output requested is the type the call writes
        // for the operator's result that is a type parameter and nothing
        // more: the `i64?` of `get<i64?>(global_state, key)` for core's
        // `operator get<V, S, K>(store: S, key: K) -> V` (GEN-8).
        let result_parameter = bare_result_parameter(public);
        let result_written = requested.is_some()
            && result_parameter
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
            callback: None,
        };
        let ((mut selected, untold), function_errors) = self.admitted_with_function_errors(
            operator,
            &candidates,
            &given,
            &wanted,
            pending_default,
        );
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
                // GEN-18: a formal function's unknown law is not a
                // guarantee, even in a body with no type parameters.
                let formal_law = given.iter().any(|argument| matches!(&argument.value.kind,
                    ExprKind::StaticCallable(callback) if matches!(callback.as_ref(), Callee::Parameter { .. })))
                    && candidates.iter().any(|(module, declaration, _)| program.modules[*module].declarations[*declaration]
                        .syntax.descendants().any(|node| crate::laws::predicate(&node).is_some()));
                if formal_law {
                    self.diagnostics.emit(self.no_one_candidate(
                        (name.text(), Location::new(self.file, name.span())),
                        public,
                        (&given, &wanted),
                        (!candidates.is_empty(), &selected),
                    ));
                }
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
        let nullable_place = match wanted.place {
            Some(Type::Nullable(payload)) if !wanted.written && self.designating != Some(at) => {
                Some(payload.as_ref())
            }
            _ => None,
        };
        if selected.is_empty() && function_errors.is_empty() && nullable_place.is_some() {
            // A nonfallible bare candidate may have no place-based
            // binding even for admission. Try the payload only to
            // diagnose an otherwise valid call.
            selected.extend(self.nullable_result_candidate(operator, &candidates, &given, &wanted));
        }
        if selected.len() != 1 {
            // GEN-5: "the requested output before inputs". Where what
            // the place expects is not known, it may be what tells tied
            // candidates apart: the `make()` of `[make()]` given to a
            // `list<i64>`. The tie is then not reported, and the call
            // has no type.
            if selected.len() > 1 && expected.is_some_and(open) {
                return (self.unselected(given, Form::Named), Type::Unknown);
            }
            if selected.is_empty() && !function_errors.is_empty() {
                for diagnostic in function_errors {
                    self.diagnostics.emit(diagnostic);
                }
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
        let unresolved = nullable_place
            .filter(|_| chosen.nullable_result_fallback)
            .map(|payload| {
                self.unresolved_operator_result(
                    (name.text(), at),
                    public,
                    &given,
                    explicit,
                    result_parameter,
                    payload,
                )
            });
        let called = self.called_with_result_diagnostic(
            operator,
            chosen,
            given,
            operator_generics,
            (Form::Named, at),
            unresolved,
        );
        with_operator_packs(called, operator_type_packs)
    }

    /// Read the public request and reflected keys using this trial's callback.
    pub(super) fn trial_reflected_call(
        &mut self,
        operator: OperatorRef,
        public: &Signature,
        call: &ReflectedCall,
        given: &mut [Given],
    ) -> Result<Requested, ()> {
        let mut request =
            self.argument_output_requested(operator, public, call.found.clone(), given, call.said);
        if let Some(unfit) = request.unfit.take() {
            self.diagnostics
                .emit(unfit.diagnostic(public, Location::new(self.file, call.name.span())));
            return Err(());
        }
        let unit = &self.program.modules[operator.module];
        if !reflects(unit, public) {
            return Ok(request);
        }
        let dependent = dependent_slot(unit, public);
        let reflected = self.reflected_operator_call(
            (operator, public, dependent),
            (&call.name, &call.list),
            &request.generics,
            given,
        )?;
        if let Some(projected) = reflected.projected {
            request.place = Some(projected);
        }
        request.type_packs = reflected.packs;
        Ok(request)
    }

    /// Check the operator’s public reflected keys and conditions (GEN-4, GEN-17).
    fn reflected_operator_call(
        &mut self,
        (operator, public, dependent): (OperatorRef, &Signature, Option<usize>),
        (name, list): (&SyntaxToken, &SyntaxNode),
        operator_generics: &[Argument],
        given: &mut [Given],
    ) -> Result<ReflectedOperatorCall, ()> {
        let program = self.program;
        let unit = &program.modules[operator.module];
        // GEN-6: candidate-only options do not supply a public slot.
        // The candidate still receives them when the call is selected.
        let public_entries = given
            .iter()
            .enumerate()
            .filter(|(_, argument)| match &argument.key {
                Key::Configuration(_) | Key::Brace(_) => {
                    argument.key.public_parameter(public).is_some()
                }
                Key::Generic(_) => argument.key.generic_parameter(public).is_some(),
                _ => true,
            });
        let Some(placements) = bound(
            public,
            public,
            public_entries.clone().map(|(_, argument)| argument),
            dependent,
        ) else {
            return Err(());
        };
        let mut slots = vec![None; given.len()];
        for ((index, _), place) in public_entries.zip(placements) {
            slots[index] = place.parameter();
        }
        let reflected = operator_packs(
            program,
            (operator.module, public),
            operator_generics,
            given,
            &slots,
        );
        let resolved = match reflected {
            Ok(resolved) => resolved,
            Err(fault) => {
                self.operator_pack_fault((name, list), given, &fault);
                return Err(());
            }
        };
        for (index, pack) in resolved.callbacks {
            specialize_named_pack(&mut given[index].value, Some(pack));
        }
        let at = Location::new(self.file, name.span());
        let packs = PackTypes::new(&public.generics, &resolved.packs);
        let map: Vec<_> = public
            .generics
            .iter()
            .map(|generic| generic.at)
            .zip(operator_generics)
            .collect();
        if let Some(missing) = packs.written_member_fault(unit, operator.declaration, &map) {
            self.pack_member_fault(at, &missing);
            return Err(());
        }
        let before = self.diagnostics.len();
        self.pack_requires_met(
            at,
            (operator.module, operator.declaration),
            operator_generics,
            &resolved.packs,
        );
        if self.diagnostics.len() != before {
            return Err(());
        }
        if let Some(result) = &public.result
            && result.any_part(&|part| matches!(part, Type::PackMember { .. }), &|_| false)
        {
            match packs.project(result, &map) {
                Ok(ty) if whole(&ty) => {
                    return Ok(ReflectedOperatorCall {
                        projected: Some(ty),
                        packs: resolved.packs,
                    });
                }
                Ok(_) => {}
                Err(missing) => {
                    self.pack_member_fault(at, &missing);
                    return Err(());
                }
            }
        }
        Ok(ReflectedOperatorCall {
            projected: None,
            packs: resolved.packs,
        })
    }

    /// A diagnostic trial runs the selected call's own checks first.
    /// Its provisional type creates neither a store key use nor an
    /// instance when the result remains unresolved.
    pub(super) fn called_with_result_diagnostic(
        &mut self,
        operator: OperatorRef,
        chosen: Chosen,
        given: Vec<Given>,
        operator_generics: Vec<Argument>,
        form: (Form, Location),
        unresolved: Option<Diagnostic>,
    ) -> (ExprKind, Type) {
        let reported = self.diagnostics.len();
        let (mut kind, ty) = self.called(operator, chosen, given, operator_generics, form);
        if let Some(diagnostic) = unresolved
            && let ExprKind::Call { callee, .. } = &mut kind
        {
            if self.diagnostics.len() == reported && !open(&ty) {
                self.diagnostics.emit(diagnostic);
            }
            *callee = Callee::Unread;
            return (kind, Type::Unknown);
        }
        (kind, ty)
    }

    /// The result parameter and its written position belong to the
    /// public operator, even when the candidate calls it another name.
    pub(super) fn unresolved_operator_result(
        &self,
        (name, at): (&str, Location),
        public: &Signature,
        given: &[Given],
        mut found: Vec<Option<Argument>>,
        parameter: Option<usize>,
        payload: &Type,
    ) -> Diagnostic {
        let Some(index) = parameter else {
            return error(
                Category::Type,
                codes::GENERIC_UNRESOLVED,
                at,
                format!(
                    "this call of `{name}` leaves its result type unresolved in a nullable place"
                ),
                "GEN-8",
            );
        };
        let parameter = &public.generics[index].name;
        let diagnostic = error(
            Category::Type,
            codes::GENERIC_UNRESOLVED,
            at,
            format!("this call of `{name}` leaves `{parameter}` unresolved in a nullable place"),
            "GEN-8",
        );
        self.infer_operator_inputs(public, &mut found, &public_inputs(public, given));
        let prefix: Option<Vec<_>> = found[..index]
            .iter()
            .map(|argument| match argument.as_ref()? {
                Argument::Type(ty) if !whole(ty) => None,
                Argument::Constant(value) if matches!(value.value, ConstantValue::Unknown) => None,
                argument => Some(argument.to_string()),
            })
            .collect();
        let Some(mut arguments) = prefix else {
            return diagnostic.with_note(format!(
                "write `{parameter}` as generic argument {} of `{name}`",
                index + 1
            ));
        };
        arguments.push(payload.to_string());
        let plain = arguments.join(", ");
        arguments[index] = Type::Nullable(Box::new(payload.clone())).to_string();
        let optional = arguments.join(", ");
        diagnostic.with_note(format!(
            "write the type argument, `{name}<{optional}>(...)` or `{name}<{plain}>(...)`"
        ))
    }

    /// A contextual literal may contain a computation or a read whose type
    /// is not settled. That dependency must settle before the call can
    /// choose a place for the literal. An empty list or null itself can
    /// still need the candidate's type.
    fn contextual_operator_argument_pending(&self, argument: &Expr, replaying: bool) -> bool {
        let mut pending = false;
        argument.walk(&mut |value| {
            if pending {
                return;
            }
            let dependency = match &value.kind {
                // A call with no result yet can itself need the
                // candidate's parameter type to select (GEN-5).
                ExprKind::Call {
                    callee: Callee::Unread,
                    ..
                } if open(&value.ty) => false,
                ExprKind::Local(_) | ExprKind::Call { .. } => {
                    !whole(&value.ty) || self.category(value) == ValueCategory::Unsettled
                }
                _ => false,
            };
            pending = dependency || (replaying && contextual_type_is_pending(self, value));
        });
        pending
    }

    /// GEN-5 and GEN-7: reapply named selection to the original written
    /// call with the types and categories of its settled arguments.
    pub(super) fn settle_operator(&mut self, call: &Expr) -> Option<(ExprKind, Type)> {
        let ExprKind::Call {
            callee: Callee::DeferredOperator(deferred),
            arguments,
            form,
            ..
        } = &call.kind
        else {
            return None;
        };
        let ready = arguments
            .iter()
            .zip(&deferred.contextual)
            .all(|(argument, contextual)| {
                if *contextual {
                    !self.contextual_operator_argument_pending(&argument.value, true)
                } else {
                    whole(&argument.value.ty)
                        && self.category(&argument.value) != ValueCategory::Unsettled
                }
            });
        if !ready {
            return None;
        }
        let list = self.node_at(deferred.list)?;
        if *form != Form::Named {
            return self.settle_logic(*form, deferred.identity, &list, arguments);
        }
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
            OperatorRead::Replay(ReplayedCall {
                arguments,
                contextual: &deferred.contextual,
                source_expected: deferred.expected.as_ref(),
            }),
        );
        self.callable = previous;
        Some(selected)
    }

    /// Read the arguments in source order. The public reading of a
    /// static argument can inform the output request (GEN-2, FN-17).
    /// GEN-6 leaves its final function template to each candidate.
    fn read_operator_arguments(
        &mut self,
        (operator, public, dependent, candidate_evidence): (
            OperatorRef,
            &Signature,
            Option<&DependentPack>,
            bool,
        ),
        written: impl IntoIterator<Item = (Key, SyntaxNode, Option<Expr>)>,
        context: Option<&[Option<Argument>]>,
        pack: Option<&hir::TypePackArgument>,
        replay: Option<ReplayedCall<'_>>,
        candidates: &[Candidate<'_>],
    ) -> Vec<Given> {
        let declared = &self.program.modules[operator.module].declarations[operator.declaration];
        let mut defaulted = Vec::new();
        let resolved: Vec<_> = context
            .unwrap_or_default()
            .iter()
            .zip(&public.generics)
            .enumerate()
            .map(|(index, (value, generic))| {
                value
                    .clone()
                    .or_else(|| {
                        dependent?;
                        let default = self.default_constant(declared, generic)?;
                        defaulted.push(index);
                        Some(Argument::Constant(default))
                    })
                    .unwrap_or(Argument::Type(Type::Unknown))
            })
            .collect();
        let map: Vec<_> = public
            .generics
            .iter()
            .map(|generic| generic.at)
            .zip(&resolved)
            .collect();
        let evidence = dependent.map(|_| {
            CallbackEvidence::from_callables(
                self.program,
                [(
                    (operator.module, operator.declaration, public),
                    map.as_slice(),
                    defaulted.as_slice(),
                )],
            )
        });
        let fixed = public.positional_parameters().count();
        let written: Vec<_> = written.into_iter().collect();
        // For FN-17 selection the dependent arguments supply type
        // evidence even when written after the callback itself.
        let positional_pack = public
            .parameters
            .iter()
            .any(|p| p.pack == Some(Pack::Positional));
        let dependent_positions: Vec<_> = written
            .iter()
            .enumerate()
            .filter_map(|(index, (key, _, _))| {
                let captured = match key {
                    Key::Position(position) => *position >= fixed && !positional_pack,
                    Key::ForwardedPosition(None) => !positional_pack,
                    Key::Name(_) => key.public_parameter(public).is_none(),
                    _ => false,
                };
                (dependent.is_some() && captured).then_some(index)
            })
            .collect();
        let mut dependent_values = Vec::new();
        if context.is_some() && replay.is_none() && dependent.is_some() {
            let mark = self.mark();
            dependent_values.extend(dependent_positions.iter().map(|&index| {
                let (key, node, ready) = &written[index];
                match ready {
                    Some(value) => Given {
                        key: key.clone(),
                        node: node.clone(),
                        value: value.clone(),
                        by_place: false,
                        by_function: false,
                        type_evidence: true,
                        dependent_callback: None,
                        explicit: None,
                        retried: false,
                    },
                    None => self.give(key.clone(), node.clone()),
                }
            }));
            self.take_back(mark, false);
        }
        let supplied: Vec<_> = dependent_values
            .iter()
            .map(|given| (given.key.name(), &given.value))
            .collect();
        let excluded = dependent.map_or_else(Vec::new, |pack| pack_exclusions(pack, &map));
        let result_shapes = dependent
            .filter(|_| evidence.as_ref().is_some_and(|value| !value.is_empty()))
            .and_then(|pack| {
                let parameter = public
                    .parameters
                    .iter()
                    .find(|parameter| parameter.at == pack.function)?;
                let Type::Function(template) = &parameter.ty else {
                    return None;
                };
                Some(self.sibling_result_shapes(
                    public,
                    template.result.as_ref(),
                    &map,
                    written.iter().filter_map(|(key, node, _)| {
                        Some((&public.parameters[key.public_parameter(public)?].ty, node))
                    }),
                ))
            })
            .unwrap_or_default();
        let law_inputs = dependent
            .and_then(|pack| {
                public
                    .parameters
                    .iter()
                    .position(|parameter| parameter.at == pack.function)
            })
            .filter(|&slot| {
                written.iter().any(|(key, node, ready)| {
                    key.public_parameter(public) == Some(slot)
                        && ready.is_none()
                        && ungrouped(node.clone()).kind() == SyntaxKind::FunctionReference
                        && !(candidate_evidence && self.uses_callback_evidence(node))
                })
            })
            .map_or_else(Vec::new, |slot| {
                self.callback_law_inputs(
                    (operator.module, operator.declaration, public),
                    &defaulted,
                    slot,
                    written.iter().filter_map(|(key, node, ready)| {
                        Some((key.public_parameter(public)?, node, ready.as_ref()))
                    }),
                )
            });
        written
            .into_iter()
            .enumerate()
            .map(|(position, (key, node, ready))| {
                let public_type = public_function(public, &key);
                let public_slot = key.public_parameter(public);
                let dependent_function = dependent.is_some_and(|pack| {
                    public_slot.is_some_and(|index| public.parameters[index].at == pack.function)
                });
                let fixed_template = |ty: &Type| {
                    matches!(ty, Type::Function(template) if template.named_pack.is_none())
                };
                let candidate_callback = || {
                    let node = ungrouped(node.clone());
                    if !self.is_static_callback(&node) {
                        return false;
                    }
                    candidates.iter().filter_map(|&(_, _, candidate)| {
                        let index = public
                            .candidate_parameters(candidate)
                            .nth(public_slot?)
                            .flatten()?;
                        Some(&candidate.parameters[index].ty)
                    }).any(|ty| {
                        // FN-17: a reflected anonymous input takes its
                        // written type where the template gives none.
                        matches!(ty, Type::Function(template)
                            if node.kind() == SyntaxKind::FnExpr || template.named_pack.is_none())
                    })
                };
                // A reflected pack needs its public reading to establish
                // the arguments the operator admits (GEN-17).
                let dependent_callback = (candidate_evidence
                    && dependent_function
                    && ready.is_none()
                    && self.uses_callback_evidence(&node))
                .then(|| match substitute_within(public_type?, &map) {
                    Type::Function(template) => Some(Box::new(DependentCallback {
                        template: *template,
                        supplied: dependent_positions.clone(),
                        excluded: excluded.iter().map(|name| (*name).to_string()).collect(),
                        call: None,
                        bindings: map.iter().map(|(at, value)| (*at, (*value).clone())).collect(),
                        defaulted: defaulted.clone(),
                    })),
                    _ => None,
                })
                .flatten();
                let by_function = dependent_callback.is_some()
                    || !candidates.is_empty()
                    && !dependent_function
                    && ready.is_none()
                    && (public_type.is_some_and(fixed_template)
                        || public_type.is_none() && candidate_callback());
                let explicit = match &key {
                    Key::Position(position) => position
                        .checked_sub(fixed)
                        .and_then(|index| pack?.types.get(index)),
                    _ => None,
                };
                if let Some(replayed) = replay {
                    let by_place = replayed.contextual[position] && !by_function;
                    Given {
                        key,
                        node,
                        value: replayed.arguments[position].value.clone(),
                        by_place,
                        by_function,
                        type_evidence: true,
                        dependent_callback,
                        explicit: explicit.cloned(),
                        retried: by_place || by_function,
                    }
                } else if let Some(value) = ready {
                    Given {
                        key,
                        node,
                        value,
                        by_place: false,
                        by_function: false,
                        type_evidence: true,
                        dependent_callback: None,
                        explicit: None,
                        retried: false,
                    }
                } else if dependent_callback.is_none()
                    && context.is_some()
                    && let Some(pattern) = public_type
                    && let Type::Function(template) = substitute_within(pattern, &map)
                {
                    let inputs = if dependent_function {
                        CallbackInputs {
                            supplied: Some(&supplied),
                            excluded: &excluded,
                            evidence: evidence.as_ref(),
                            result_shapes: &result_shapes,
                            law_inputs: &law_inputs,
                        }
                    } else {
                        CallbackInputs::default()
                    };
                    let mark = by_function.then(|| self.mark());
                    let mut value = self.operator_function_argument(&node, &template, &inputs);
                    if mark.is_some_and(|mark| !self.take_back(mark, false)) {
                        value.kind = ExprKind::Unread;
                        value.ty = Type::Unknown;
                    }
                    Given {
                        key,
                        node,
                        value,
                        by_place: false,
                        by_function,
                        type_evidence: true,
                        dependent_callback,
                        explicit: None,
                        retried: by_function,
                    }
                } else if by_function {
                    // Read this callback under the candidate's
                    // template (FN-17).
                    let at = self.at(&node);
                    Given {
                        key,
                        node,
                        value: Expr {
                            kind: ExprKind::Unread,
                            ty: Type::Unknown,
                            at,
                            category: ValueCategory::Unsettled,
                        },
                        by_place: false,
                        by_function: true,
                        type_evidence: true,
                        dependent_callback,
                        explicit: None,
                        retried: true,
                    }
                } else if let Some(ty) = explicit {
                    let value = self.expression(&node, Some(ty));
                    let value = self.fit_input(value, ty, "the explicitly typed pack argument");
                    Given {
                        key,
                        node,
                        value,
                        by_place: false,
                        by_function: false,
                        type_evidence: true,
                        dependent_callback: None,
                        explicit: Some(ty.clone()),
                        retried: false,
                    }
                } else {
                    self.give(key, node)
                }
            })
            .collect()
    }

    /// GEN-3 and FN-17: the enclosing declaration's memberships can
    /// complete an operator or a bare generic function.
    fn uses_callback_evidence(&self, node: &SyntaxNode) -> bool {
        let node = ungrouped(node.clone());
        if !matches!(
            node.kind(),
            SyntaxKind::NameExpr | SyntaxKind::FunctionReference
        ) || self.local_of(&node).is_some()
        {
            return false;
        }
        let Some(name) = name_token(&node) else {
            return false;
        };
        let bare = node.kind() == SyntaxKind::NameExpr;
        let written = if bare {
            0
        } else {
            let start = child(&node, SyntaxKind::QualifiedName)
                .map_or(name.span().start, |qualified| qualified.span().start);
            self.calls
                .get(&(self.file, start))
                .map_or(0, |call| call.arguments.len())
        };
        self.declarations(&name)
            .is_some_and(|(module, declarations)| {
                let unit = &self.program.modules[module];
                declarations.iter().any(|&index| {
                    let operator = unit.declarations[index].kind == DeclarationKind::Operator;
                    (bare && operator)
                        || matches!(&unit.details[index], Detail::Callable(signature)
                        if (operator || bare) && signature.generics.len() > written)
                })
            })
    }

    /// FN-17: a candidate can give an ungrouped static callback its template
    /// when the operator's public slot supplies no function type.
    fn is_static_callback(&self, node: &SyntaxNode) -> bool {
        if node.kind() == SyntaxKind::FnExpr {
            return true;
        }
        if !matches!(
            node.kind(),
            SyntaxKind::NameExpr | SyntaxKind::FunctionReference
        ) {
            return false;
        }
        if let Some(local) = self.local_of(node) {
            return matches!(self.locals[local].ty, Type::Function(_));
        }
        name_token(node)
            .and_then(|name| self.declarations(&name))
            .is_some_and(|(module, declarations)| {
                declarations.iter().any(|&index| {
                    let kind = self.program.modules[module].declarations[index].kind;
                    kind == DeclarationKind::Operator || kind.binding_role().is_some()
                })
            })
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
                let value = if given.by_place || given.by_function {
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
                // TST-1: a test-declared candidate is visible to every test
                // of its module and to the bodies those tests call.
                if candidate.test_owner.is_some()
                    && self.test_candidates.map(crate::TestCandidateScope::module) != Some(module)
                {
                    continue;
                }
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
            by_function: false,
            type_evidence: true,
            dependent_callback: None,
            value,
            explicit: None,
            retried,
            node,
        }
    }

    /// GEN-8 and EXPR-5: a written generic argument fixes the public
    /// parameter types it determines. Fit those arguments before trying
    /// candidates, without changing the readings used to choose one.
    fn written_operator_inputs_fit(
        &mut self,
        public: &Signature,
        explicit: &[Option<Argument>],
        given: &[Given],
    ) -> bool {
        let written: Vec<_> = public
            .generics
            .iter()
            .zip(explicit)
            .filter_map(|(parameter, argument)| {
                matches!(argument, Some(Argument::Type(_) | Argument::Constant(_)))
                    .then_some(parameter.at)
            })
            .collect();
        if written.is_empty() {
            return true;
        }
        let inputs: Vec<_> = given
            .iter()
            .filter_map(|argument| {
                let index = argument.key.public_parameter(public)?;
                let parameter = &public.parameters[index];
                (parameter.pack.is_none()
                    && argument.dependent_callback.is_none()
                    && mentions(&parameter.ty, &written))
                .then_some((argument, parameter))
            })
            .collect();
        if inputs.is_empty() {
            return true;
        }

        let mut found = explicit.to_vec();
        self.infer_operator_inputs(public, &mut found, &public_inputs(public, given));
        let own: Vec<_> = public
            .generics
            .iter()
            .map(|parameter| parameter.at)
            .collect();
        let bindings: Vec<_> = own
            .iter()
            .copied()
            .zip(&found)
            .filter_map(|(at, value)| value.as_ref().map(|value| (at, value)))
            .collect();
        let mut rejected = Vec::new();
        for (argument, parameter) in inputs {
            let ty = placed(substitute_within(&parameter.ty, &bindings), &own);
            if !whole(&ty) || open(&ty) {
                continue;
            }
            if whole(argument.ty()) && agree(&ty, argument.ty()) {
                continue;
            }
            if argument.retried && !self.may_reread() {
                continue;
            }
            let mark = self.mark();
            if argument.by_function {
                if let Type::Function(template) = &ty {
                    self.operator_function_argument(
                        &argument.node,
                        template,
                        &CallbackInputs::default(),
                    );
                }
            } else {
                let value = if argument.retried {
                    self.expression_for_input_pattern(&argument.node, &ty)
                } else {
                    argument.value.clone()
                };
                // A full result can itself match the pattern. Try it
                // before looking at what an ordinary read takes.
                let ty = fitted_input_pattern(&value.ty, &ty)
                    .or_else(|| fitted_input_pattern(received_type(&value.ty, |_| true), &ty))
                    .unwrap_or(ty);
                let place = format!("the parameter `{}`", parameter.name);
                self.fit_input(value, &ty, &place);
            }
            rejected.extend(
                self.diagnostics.as_slice()[mark.diagnostics..]
                    .iter()
                    .filter(|diagnostic| {
                        diagnostic.severity == Severity::Error
                            && diagnostic.code.as_str() != codes::local::READING_LIMIT
                    })
                    .cloned(),
            );
            self.take_back(mark, false);
        }
        let fits = rejected.is_empty();
        for diagnostic in rejected {
            self.diagnostics.emit(diagnostic);
        }
        fits
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
        (public, dependent): (&Signature, Option<usize>),
        options: &OperatorOptions<'_>,
        written_pack: Option<&hir::TypePackArgument>,
        before_spread: bool,
    ) -> Vec<(Key, SyntaxNode, Option<Expr>)> {
        let mut named_seen = false;
        let mut position = 0;
        let mut names = HashSet::new();
        let mut filled = HashSet::new();
        let mut arguments = Vec::new();
        let fixed = public.positional_parameters().count();
        let has_named_pack = public
            .parameters
            .iter()
            .any(|parameter| parameter.pack == Some(Pack::Named));
        let mut positional = Vec::new();
        let mut named = Vec::new();
        let spreads = if before_spread {
            Vec::new()
        } else {
            self.forwarded_arguments(list)
        };
        let explicit = self
            .call_entries(list)
            .into_iter()
            .map(|(brace, name, node)| (brace, name, node, None));
        let expanded = spreads
            .into_iter()
            .map(|entry| (false, None, entry.source, Some((entry.key, entry.value))));
        for (brace, name, value, forwarded) in explicit.chain(expanded) {
            let at = name.as_ref().map_or_else(
                || self.at(&value),
                |name| Location::new(self.file, name.span()),
            );
            let key = match (&forwarded, name) {
                (Some((hir::PackKey::Name(name), _)), _) => {
                    if dependent.is_some() {
                        Key::Name(name.clone())
                    } else {
                        options.name_key(name, false)
                    }
                }
                (Some((hir::PackKey::Position(_), _)), _) => {
                    let slot = public
                        .positional_parameters()
                        .find(|(index, _)| !filled.contains(index))
                        .map(|(index, _)| index);
                    if slot.is_none() {
                        if dependent.is_none()
                            && options
                                .positional_limit
                                .is_some_and(|maximum| fixed + positional.len() >= maximum)
                        {
                            self.diagnostics.emit(error(
                                Category::Type,
                                codes::ARGUMENT_UNEXPECTED,
                                at,
                                format!("`{}` takes no positional argument here", callee.text()),
                                "FN-19",
                            ));
                        }
                        positional.push(at);
                    }
                    Key::ForwardedPosition(slot)
                }
                (_, None) => {
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
                    if dependent.is_none()
                        && !options.configuration.is_empty()
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
                (_, Some(name)) => {
                    // A named brace entry does not put the parentheses
                    // into their named-argument portion (EXPR-2, EXPR-10).
                    named_seen |= !brace;
                    if !brace && dependent.is_some() {
                        Key::Name(name.text().to_string())
                    } else {
                        options.name_key(name.text(), brace)
                    }
                }
            };
            let own = key.public_parameter(public);
            let mut duplicate = own.is_some_and(|index| !filled.insert(index));
            if let Some(text) = key.name() {
                let known = own.is_some() || options.accepts_name(&key);
                let captured = !known
                    && has_named_pack
                    && if dependent.is_some() {
                        !brace
                    } else {
                        !(options.configuration.contains(text) && !brace)
                    };
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
                    let callback_name = captured && dependent.is_some();
                    duplicate |= !names.insert((callback_name, text.to_string()));
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
            if forwarded.is_some()
                && let Key::Generic(name) = &key
            {
                if !duplicate {
                    self.report_forwarded_generic_value(callee, name, at);
                }
                continue;
            }
            arguments.push((key, value, forwarded.map(|(_, value)| value)));
        }
        // The complete pack determines its cardinality (FN-8, FN-19).
        if before_spread {
            return arguments;
        }
        for (index, parameter) in public.parameters.iter().enumerate() {
            let combined: Vec<_>;
            let captured = match parameter.pack {
                Some(Pack::Positional) => &positional,
                Some(Pack::Named) if dependent == Some(index) => {
                    let has_positional = public
                        .parameters
                        .iter()
                        .any(|parameter| parameter.pack == Some(Pack::Positional));
                    combined = positional
                        .iter()
                        .filter(|_| !has_positional)
                        .chain(&named)
                        .copied()
                        .collect();
                    &combined
                }
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
            self.remember_type_queries_in(node);
            let read = self.read_constant(node, expected);
            if let (Some(ty @ Type::Set(element)), Err(fault)) = (expected, &read)
                && **element == Type::Scalar(Scalar::Str)
                && let Some(report) = fault.report(
                    self.at(node),
                    &format!("the generic value parameter `{}`", generic.name),
                    ty,
                )
            {
                self.diagnostics.emit(report);
            } else if let Some(why) = Self::not_constant(&read) {
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
                value: if given.retried && !given.by_function {
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
            text: None,
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

/// The Core value candidate whose fallback must be supplied (ERR-6).
pub(super) fn core_dictionary_fallback(
    program: &Program,
    module: usize,
    candidate: &Declaration,
    signature: &Signature,
) -> bool {
    let [store, _, fallback] = signature.parameters.as_slice() else {
        return false;
    };
    candidate.name == "get"
        && program.core() == Some(module)
        && matches!(
            candidate.kind,
            DeclarationKind::Implementation(Role::Value, _)
        )
        && matches!(store.ty, Type::Dict { .. })
        && fallback.name == "fallback"
}

/// How the call supplies the optional dictionary fallback (ERR-6).
pub(super) enum DictionaryFallback {
    /// The call is no dictionary fallback read, or it omits the fallback.
    Omitted,
    /// The written literal selects the plain read. This is its argument index.
    WrittenNull(usize),
    /// The call evaluates the fallback and gives the dictionary's value type.
    Value,
}

impl Typer<'_> {
    /// The form of a dictionary read, or its known wrong fallback type.
    pub(super) fn dictionary_get_fallback(
        &self,
        operator: OperatorRef,
        public: &Signature,
        candidates: &[Candidate<'_>],
        given: &[Given],
    ) -> Result<DictionaryFallback, Box<Diagnostic>> {
        let program = self.program;
        if program.modules[operator.module].declarations[operator.declaration].name != "get"
            || program.core() != Some(operator.module)
        {
            return Ok(DictionaryFallback::Omitted);
        }
        let Some(&(_, _, signature)) =
            candidates
                .iter()
                .find(|&&(module, declaration, signature)| {
                    core_dictionary_fallback(
                        program,
                        module,
                        &program.modules[module].declarations[declaration],
                        signature,
                    )
                })
        else {
            return Ok(DictionaryFallback::Omitted);
        };
        let Some(places) = bound(signature, public, given, None) else {
            return Ok(DictionaryFallback::Omitted);
        };
        let Some(store) = places.iter().position(|place| place.parameter() == Some(0)) else {
            return Ok(DictionaryFallback::Omitted);
        };
        let store = &given[store];
        let value_type = match store.ty() {
            Type::Dict { value, .. } => Some(value.as_ref()),
            // EXPR-9: these pairs take their types from the dictionary candidate.
            _ if matches!(store.value.kind, ExprKind::Pairs(_)) => None,
            // GEN-7: the operations of every stated member are callable here.
            Type::Parameter { at, .. }
                if self.memberships().iter().any(|(position, members)| {
                    position == at
                        && !members.is_empty()
                        && members
                            .iter()
                            .all(|member| matches!(member, Argument::Type(Type::Dict { .. })))
                }) =>
            {
                None
            }
            _ => return Ok(DictionaryFallback::Omitted),
        };
        let Some(fallback) = places.iter().position(|place| place.parameter() == Some(2)) else {
            return Ok(DictionaryFallback::Omitted);
        };
        let argument = &given[fallback];
        if written_null(&argument.node) {
            return Ok(DictionaryFallback::WrittenNull(fallback));
        }
        if let Some(value) = value_type
            && !dictionary_fallback_type(argument, value)
        {
            return Err(Box::new(error(
                Category::Type,
                codes::LOOKUP_DEFAULT_TYPE,
                argument.value.at,
                format!(
                    "the fallback has type `{}` and the dictionary's value type is `{value}`",
                    argument.ty()
                ),
                "ERR-6",
            )));
        }
        Ok(DictionaryFallback::Value)
    }
}

/// The result projection and keyed packs from one public reflection pass.
struct ReflectedOperatorCall {
    projected: Option<Type>,
    packs: Vec<hir::TypePackArgument>,
}

/// GEN-6/17: public defaults keep the operator's resolved pack metadata,
/// independently of the chosen candidate's own generic packs.
fn with_operator_packs(
    (mut kind, ty): (ExprKind, Type),
    packs: Vec<hir::TypePackArgument>,
) -> (ExprKind, Type) {
    if !packs.is_empty()
        && let ExprKind::Call {
            callee:
                Callee::Candidate {
                    operator_type_packs,
                    ..
                }
                | Callee::Required {
                    operator_type_packs,
                    ..
                },
            ..
        } = &mut kind
    {
        *operator_type_packs = packs;
    }
    (kind, ty)
}

/// ERR-6 requires the fallback's exact type. A literal can take its
/// expected type under EXPR-5; an open generic is checked in its instance.
pub(super) fn dictionary_fallback_type(argument: &Given, expected: &Type) -> bool {
    let actual = argument.ty();
    argument.by_place
        || actual == expected
        || !whole(expected)
        || !whole(actual)
        || names_parameter(expected)
        || names_parameter(actual)
        || (matches!(expected, Type::Scalar(Scalar::F64)) && is_integer_literal(&argument.value))
}

/// A literal's preliminary type is not a requested output (GEN-5).
/// Parentheses leave no HIR node; a minus retains its literal operand.
fn literal_input(value: &Expr) -> bool {
    matches!(value.kind, ExprKind::Literal(_)) || negated(value).is_some_and(literal_input)
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
            .then(|| maximum.max(signature.positional_parameters().count()))
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
    /// Complete public inference, used by reflected pack constraints.
    pub(super) generics: Vec<Argument>,
    /// Arguments for inherited configuration defaults: an argument-derived
    /// type stays open until the candidate fits its source expression.
    pub(super) call_generics: Vec<Argument>,
    /// Public keyed packs resolved by the same callback trial (GEN-6, GEN-17).
    pub(super) type_packs: Vec<hir::TypePackArgument>,
    /// Failure to specialize or fit a default the call takes (GEN-8, GEN-13).
    pub(super) unfit: Option<Unfit>,
}

impl Requested {
    fn rejected(unfit: Unfit) -> Self {
        Self {
            place: None,
            among: None,
            generics: Vec::new(),
            call_generics: Vec::new(),
            type_packs: Vec::new(),
            unfit: Some(unfit),
        }
    }
}

/// A generic value default the operator call cannot take.
pub(super) enum Unfit {
    /// GEN-8: an argument referenced by the default has no binding.
    Unresolved { generic: usize },
    /// GEN-13: the default does not fit the parameter's concrete type.
    Mismatch {
        generic: usize,
        described: String,
        resolved: Box<Type>,
    },
}

impl Unfit {
    /// Report the failed default at the operator call.
    pub(super) fn diagnostic(&self, public: &Signature, at: Location) -> Diagnostic {
        match self {
            Self::Unresolved { generic } => {
                let parameter = &public.generics[*generic].name;
                error(
                    Category::Type,
                    codes::GENERIC_UNRESOLVED,
                    at,
                    format!(
                        "this call leaves `{parameter}` unresolved: its default needs an unresolved generic argument"
                    ),
                    "GEN-8",
                )
            }
            Self::Mismatch {
                generic,
                described,
                resolved,
            } => {
                let parameter = &public.generics[*generic].name;
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

/// The public generic named by a bare result or bare fallible success.
pub(super) fn bare_result_parameter(public: &Signature) -> Option<usize> {
    let result = match public.result.as_ref()? {
        Type::Result { value, error } if **error == Type::Error => value.as_ref(),
        other => other,
    };
    bare_parameter(result, &public.generics)
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
