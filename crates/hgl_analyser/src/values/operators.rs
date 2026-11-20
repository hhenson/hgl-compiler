//! A call of an operator, by its name or where a symbol stands for it:
//! the output it requests, its arguments as they are written, and the
//! operator's value candidates (GEN-5, GEN-6, EXPR-2). Part of the typing
//! pass ([`super`]); the candidate is selected in [`super::candidates`].

use hgl_diagnostics::{Category, Diagnostic, Location};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};

use super::candidates::{Chosen, Given, Key, Settled, Wanted, typed_by_place};
use super::constants::generic_default;
use super::fit::{not_null, open};
use super::inference::{bare_parameter, fixed, infer, placed, unsaid};
use super::{Candidate, Typer};
use crate::declared::{agree, substitute_within};
use crate::hir::{self, Callee, Expr, ExprKind, Form};
use crate::program::{OperatorRef, Role};
use crate::syntax::has_token;
use crate::types::{Argument, CallArguments, Detail, GenericKind, NamedKind, Signature, Type};
use crate::{codes, error};

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
            infer(&parameter.ty, actual, &public.generics, &mut found);
            for (generic, was) in before.into_iter().enumerate() {
                by_argument[generic] |= !was && found[generic].is_some();
            }
        }
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
            unfit: None,
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
    /// What is not read here: a call of an operator that has a pack, or
    /// a candidate with one. Such a call has no type.
    pub(super) fn call_operator(
        &mut self,
        name: &SyntaxToken,
        list: &SyntaxNode,
        operator: OperatorRef,
        expected: Option<&Type>,
        generics: Option<&CallArguments>,
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
        let written: Vec<Option<Argument>> = match generics {
            None => vec![None; public.generics.len()],
            Some(_) => {
                let Some((explicit, _)) = self.generic_arguments(name, &public.generics, generics)
                else {
                    return self.unread_call(list);
                };
                explicit
            }
        };
        // GEN-8: "A generic value parameter the call does not give takes
        // its default, when it has one. An operator's takes part in the
        // output requested, in GEN-2's order: what the call writes, then
        // what its place expects, then its arguments, and the default
        // where none of them says."
        // What the call writes, before its place says anything.
        let explicit = written.clone();
        let mut found = written;
        from_place(public, expected, &mut found);
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
        let (selecting, options) = options_of(public, &candidates);
        let reported = self.diagnostics.len();
        let written = self.operator_arguments(name, list, public, &options);
        // Each argument, read with nothing expected of it: what it
        // reports itself is reported here, once.
        let mut given: Vec<Given> = written
            .into_iter()
            .map(|(key, node)| self.give(key, node))
            .collect();
        self.constants_given(&mut given, &selecting);
        // An argument given to no parameter, or rejected itself, has
        // been reported, and one whose type is not known says nothing
        // of the candidates: the call is then not judged (ERR-9).
        let faulty = self.diagnostics.len() != reported;
        let unknown = given
            .iter()
            .any(|given| !given.by_place && open(&given.value.ty));
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
        let written_generics: Option<Vec<Option<Argument>>> = generics.map(|_| found.clone());

        // Then its arguments, in the order the operator declares its
        // parameters (GEN-12), and its defaults: see
        // `Typer::output_requested`.
        let supplied: Vec<Option<&Type>> = public
            .parameters
            .iter()
            .enumerate()
            .map(|(index, parameter)| {
                given
                    .iter()
                    .find(|given| match &given.key {
                        Key::Position(position) => *position == index,
                        Key::Name(name) => *name == parameter.name,
                    })
                    .map(|given| &given.value.ty)
            })
            .collect();
        let Requested {
            place: requested,
            among,
            unfit,
        } = self.output_requested(operator, public, found, &supplied, generics.is_some());
        // GEN-13: a default the call takes is of its parameter's type
        // as the call resolves it: the `2` of
        // `operator choose<T, const fallback: T = 2>` is no `str`. One
        // that is not of the type as it is declared is reported there.
        if let Some(Unfit {
            generic,
            described,
            resolved,
        }) = unfit
        {
            let generic = &public.generics[generic];
            self.diagnostics.emit(error(
                Category::Type,
                codes::TYPE_MISMATCH,
                Location::new(self.file, name.span()),
                format!(
                    "this leaves `{}` to its default, {described}, and `{}` is `{resolved}` here",
                    generic.name, generic.name
                ),
                "GEN-13",
            ));
            return (self.unselected(given, Form::Named), Type::Unknown);
        }
        // GEN-7: "A generic body is checked under its stated
        // requirements": what the enclosing declaration requires of its
        // generic parameters is callable in its body. The requirement
        // is of the call as it asks: one that gives another output than
        // the call requests, its defaults included, is not this call.
        if let Some(result) = self.required(
            operator,
            &given.iter().collect::<Vec<_>>(),
            (requested.as_ref(), expected),
            written_generics.as_deref(),
        ) {
            return self.call_required(operator, result, given, Form::Named);
        }
        // GEN-5: "the requested output before inputs". EXPR-5: an
        // integer literal is an `f64` where one is expected; it selects
        // as the `i64` it is where a candidate takes one.
        let wanted = Wanted {
            place: requested
                .as_ref()
                .or(expected.filter(|expected| !open(expected))),
            among,
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
        match self.select_per_instance(
            operator,
            &given,
            (&wanted, requested.as_ref()),
            (Form::Named, written, named),
            untold,
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
        self.called(operator, chosen, given, (Form::Named, at))
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
                    given.value.ty.to_string()
                };
                match &given.key {
                    // An option is known by its name.
                    Key::Name(name) if !public.parameters.iter().any(|p| p.name == *name) => {
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
        let named = match &value.key {
            Key::Position(position) => *position == 0,
            Key::Name(name) => *name == parameter.name,
        };
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

    /// The value candidates of `operator`, wherever they are declared.
    /// GEN-5: "Library membership and registration order grant no
    /// priority."
    pub(super) fn value_candidates(&mut self, operator: OperatorRef) -> Vec<Candidate<'a>> {
        let key = (operator.module, operator.declaration);
        if let Some(found) = self.candidates.get(&key) {
            return found.clone();
        }
        let program = self.program;
        let mut candidates = Vec::new();
        for (module, declared) in program.modules.iter().enumerate() {
            for (declaration, candidate) in declared.declarations.iter().enumerate() {
                // GEN-5: "A call in a value function or a runtime body
                // selects among the value candidates. A call in
                // composition selects among all of them".
                let admitted = match candidate.kind.candidate() {
                    Some((Role::Value, Some(of))) => of == operator,
                    Some((Role::Temporal, Some(of))) => self.graph && of == operator,
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
        options: &std::collections::HashSet<&str>,
    ) -> Vec<(Key, SyntaxNode)> {
        let mut named_seen = false;
        let mut position = 0;
        let mut names: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut arguments = Vec::new();
        for (name, value) in self.call_arguments(list) {
            let key = match name {
                None => {
                    if named_seen {
                        self.diagnostics.emit(error(
                            Category::Type,
                            codes::ARGUMENT_ORDER,
                            self.at(&value),
                            "an argument with no name follows a named one: positional \
                             arguments come first"
                                .to_string(),
                            "EXPR-2",
                        ));
                    }
                    position += 1;
                    Key::Position(position - 1)
                }
                Some(name) => {
                    named_seen = true;
                    let text = name.text();
                    let at = Location::new(self.file, name.span());
                    // The operator's parameters are the same places in
                    // every candidate, the first of each.
                    let own = public
                        .parameters
                        .iter()
                        .position(|parameter| parameter.name == text);
                    if own.is_none() && !options.contains(text) {
                        self.diagnostics.emit(error(
                            Category::Name,
                            codes::ARGUMENT_UNKNOWN,
                            at,
                            format!("`{}` has no parameter `{text}`", callee.text()),
                            "EXPR-2",
                        ));
                    } else if !names.insert(text.to_string())
                        || own.is_some_and(|index| index < position)
                    {
                        self.diagnostics.emit(error(
                            Category::Name,
                            codes::ARGUMENT_DUPLICATE,
                            at,
                            format!("`{text}` is given twice"),
                            "EXPR-2",
                        ));
                    }
                    Key::Name(text.to_string())
                }
            };
            arguments.push((key, value));
        }
        arguments
    }

    /// The argument `given` as the value of a parameter of type `ty`,
    /// fitted to it (EXPR-5). One whose first reading was taken back
    /// (`retried`) is read again where it stands, as that type: what
    /// the first reading declared is gone, and the value read again
    /// declares its own. `place` names the parameter for messages.
    pub(super) fn argument_value(&mut self, given: Given, ty: &Type, place: &str) -> Expr {
        if given.retried {
            let value = self.expression(&given.node, Some(ty));
            self.fit(value, ty, place)
        } else {
            self.fit(given.value, ty, place)
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
            lifted: false,
        }
    }
}

/// The note on a call in a generic body that no candidate is known to
/// admit: whether one does depends on a generic parameter of the body,
/// which its `requires` does not settle (GEN-7).
pub(super) const UNSETTLED: &str = "which candidate admits this depends on a generic parameter of \
     the declaration it is written in, which its `requires` does not settle: a generic body \
     is checked under its stated requirements (GEN-7), so state the operation it calls, \
     `requires name(T) -> R`, the parameter's value, `requires M == .value`, or the types or \
     values it may be, `requires T in {i64, f64}`";

/// The names a call of an operator may give beside the operator's own
/// parameters: those its candidates declare as generic value
/// parameters, which select among them (GEN-6), and those with every
/// other option they declare.
pub(super) fn options_of<'a>(
    public: &Signature,
    candidates: &[Candidate<'a>],
) -> (
    std::collections::HashSet<&'a str>,
    std::collections::HashSet<&'a str>,
) {
    let own = public.parameters.len();
    let selecting: std::collections::HashSet<&str> = candidates
        .iter()
        .flat_map(|(_, _, signature)| &signature.generics)
        .filter(|generic| matches!(generic.kind, GenericKind::Value(_)))
        .map(|generic| generic.name.as_str())
        .collect();
    let options = candidates
        .iter()
        .flat_map(|(_, _, signature)| signature.parameters.iter().skip(own))
        .map(|parameter| parameter.name.as_str())
        .chain(selecting.iter().copied())
        .collect();
    (selecting, options)
}

/// What a call of an operator requests of its candidates: see
/// [`Typer::output_requested`].
pub(super) struct Requested {
    /// The operator's result with what is known of its generic
    /// parameters in place, when the call says something of it.
    pub(super) place: Option<Type>,
    /// The types the operator's own requirement admits for its result.
    pub(super) among: Option<Vec<Type>>,
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

/// What the place a call stands in says of the operator's generic
/// parameters: its result is the type the place expects (GEN-8). A `T`
/// is a value of a `T?` (TYPE-14), so a place that expects an optional
/// type says what a result that is not optional is.
pub(super) fn from_place(
    public: &Signature,
    expected: Option<&Type>,
    found: &mut [Option<Argument>],
) {
    if let (Some(result), Some(expected)) = (&public.result, expected)
        && !open(expected)
    {
        let place = match result {
            Type::Nullable(_) => expected,
            _ => not_null(Some(expected)).unwrap_or(expected),
        };
        infer(result, place, &public.generics, found);
    }
}

/// True when a call of an operator is not read: the operator or a
/// candidate has a pack, or a candidate has a generic parameter that is
/// a type pack. Such a call has no type.
pub(super) fn unreadable(public: &Signature, candidates: &[(usize, usize, &Signature)]) -> bool {
    let packed = |signature: &Signature| {
        signature
            .parameters
            .iter()
            .any(|parameter| parameter.pack.is_some())
    };
    packed(public)
        || candidates.iter().any(|(_, _, signature)| {
            packed(signature)
                || signature
                    .generics
                    .iter()
                    .any(|generic| matches!(generic.kind, GenericKind::Pack { .. }))
        })
}
