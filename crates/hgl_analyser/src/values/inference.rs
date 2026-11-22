//! What a call's generic parameters stand for: the generic arguments the
//! call writes, the generic values it names, and what its arguments say
//! of the rest (GEN-2, GEN-8, GEN-12, GEN-13). Part of the typing pass
//! ([`super`]).

use hgl_diagnostics::{Category, Location, Severity};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};

use super::arguments::{Assigned, Slot, Takes};
use super::candidates::typed_by_place;
use super::constants::{constant_text, generic_default};
use super::control::value_of_branch;
use super::dependent_packs::{PackTypes, dependent_slot};
use super::evals::{holds_trace, publications, trace_input_type, trace_pack_type};
use super::expressions::{holds_block, takes_its_type, written_null};
use super::fit::{not_null, open, whole};
use super::forms::Taken;
use super::{Fact, Typer};
use crate::constant::Fault;
use crate::declared::{names_one_of, names_parameter, substitute_within};
use crate::hir::{self, Expr, ValueCategory};
use crate::program::Declaration;
use crate::syntax::ungrouped;
use crate::types::{
    Argument, CallArguments, Constant, ConstantValue, GenericKind, GenericParameter, Pack,
    Parameter, Signature, Type,
};
use crate::{codes, error};

impl<'a> Typer<'a> {
    /// What a partly resolved parameter learns from an argument. Keep
    /// already resolved parts of the parameter's type (GEN-12), but
    /// leave its other parts open. For `list<T>` given a
    /// `result<list<i64>, error>`, ERR-15 gives the list, whose elements
    /// decide `T`. A bare unresolved `T` takes the whole result unless
    /// the requested output requires that parameter's payload (GEN-2).
    pub(super) fn inference_input<'t>(
        &self,
        pattern: &Type,
        actual: &'t Type,
        generics: &[GenericParameter],
        found: &[Option<Argument>],
        result_fallback: Option<(&Type, &'t Type)>,
    ) -> &'t Type {
        let payload = match actual {
            Type::Result { value, .. } | Type::Nullable(value) => value,
            _ => return actual,
        };
        if let Some((result, requested)) = result_fallback
            && pattern == result
            && !matches!(requested, Type::Nullable(_))
            && matches!(actual, Type::Result { .. })
            && let Some(index) = bare_parameter(pattern, generics)
            && found[index].is_none()
            && self.taken(requested, actual).is_some()
        {
            // `wrap<T>(value: T) -> result<T, error>` at an `i64`
            // place takes the value of a `result<i64, error>`. Binding
            // T to the whole input would produce a nested result.
            return requested;
        }
        let unknown_type = Argument::Type(Type::Unknown);
        let unknown_constant = Argument::Constant(Constant {
            text: String::new(),
            value: ConstantValue::Unknown,
        });
        let map: Vec<(Location, &Argument)> = generics
            .iter()
            .zip(found)
            .map(|(generic, argument)| {
                let unknown = match generic.kind {
                    GenericKind::Value(_) => &unknown_constant,
                    _ => &unknown_type,
                };
                (generic.at, argument.as_ref().unwrap_or(unknown))
            })
            .collect();
        let expected = substitute_within(pattern, &map);
        match self.taken(&expected, actual) {
            Some(Taken::ValueRead) => match payload.as_ref() {
                Type::Nullable(inner) => inner,
                _ => unreachable!("ValueRead requires a nullable success"),
            },
            Some(_) => payload,
            None => actual,
        }
    }

    /// The named arguments of a call that give a generic value
    /// parameter of the callee, each recorded in `found` as the
    /// constant it is. Returns each with the position of its parameter
    /// among `generics`, in the order written.
    ///
    /// GEN-8: "A call may give a generic value parameter of its callee
    /// by name, among its named arguments ... The value a call gives a
    /// generic value parameter is a compile-time constant ... Any other
    /// rejects."
    pub(super) fn named_generics(
        &mut self,
        list: &SyntaxNode,
        signature: &crate::types::Signature,
        found: &mut [Option<Argument>],
    ) -> Vec<(usize, SyntaxNode)> {
        let generics = &signature.generics;
        let mut named = Vec::new();
        for (_, name, value) in self.call_entries(list) {
            let Some(name) = name else {
                continue;
            };
            if signature
                .parameters
                .iter()
                .any(|parameter| parameter.configuration && parameter.name == name.text())
            {
                continue;
            }
            let index = generics.iter().position(|generic| {
                matches!(generic.kind, GenericKind::Value(_)) && generic.name == name.text()
            });
            let Some(index) = index else {
                continue;
            };
            // Given among the call's generic arguments too, `f<2>(N:
            // 3)`, or twice by name.
            if found[index].is_some() {
                self.diagnostics.emit(error(
                    Category::Name,
                    codes::ARGUMENT_DUPLICATE,
                    Location::new(self.file, name.span()),
                    format!("`{}` is given twice", name.text()),
                    "EXPR-2",
                ));
                // Read for what it reports itself.
                self.expression(&value, Some(&Type::Unknown));
                continue;
            }
            // Read as a value of the parameter's type where that names
            // no other parameter: one that does is held to it once that
            // is known (`generic_values`), and its type reported there.
            let expected = match &generics[index].kind {
                GenericKind::Value(ty) if !names_parameter(ty) => Some(ty),
                _ => None,
            };
            let read = self.read_constant(&value, expected);
            if let Some(why) = Self::not_constant(&read) {
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::GENERIC_ARGUMENT_CONSTANT,
                    self.at(&value),
                    format!(
                        "`{}` is a generic value parameter, and this is no compile-time \
                         constant: {why}",
                        name.text()
                    ),
                    "GEN-8",
                ));
            }
            found[index] = Some(Argument::Constant(Constant {
                text: constant_text(&value),
                value: read.unwrap_or(ConstantValue::Unknown),
            }));
            named.push((index, value));
        }
        named
    }

    /// What the generic value parameters of a function are at a call,
    /// once its arguments are read: each one the call gives by name,
    /// `named`, is held to its parameter's type, and each one nothing
    /// says anything of takes its default.
    ///
    /// GEN-13: a generic value parameter has an ordinary type, which
    /// may name a type parameter, the `T` of `<T, const fallback: T>`:
    /// what the call gives `fallback` says what `T` is where the
    /// parameters have not. GEN-8: "A generic value parameter the call
    /// does not give takes its default, when it has one."
    pub(super) fn generic_values(
        &mut self,
        callee: &SyntaxToken,
        declaration: &Declaration,
        generics: &[GenericParameter],
        (named, defaulted): (&[(usize, SyntaxNode)], &[usize]),
        found: &mut [Option<Argument>],
    ) {
        let own: Vec<Location> = generics.iter().map(|generic| generic.at).collect();
        // The type of the generic value parameter at `index`, with what
        // has been found in place; `Type::Unknown` where it still names
        // a parameter nothing has resolved.
        let type_of = |index: usize, found: &[Option<Argument>]| {
            let GenericKind::Value(pattern) = &generics[index].kind else {
                return Type::Unknown;
            };
            let resolved: Vec<Argument> = found
                .iter()
                .map(|found| found.clone().unwrap_or(Argument::Type(Type::Unknown)))
                .collect();
            let map: Vec<(Location, &Argument)> = own.iter().copied().zip(&resolved).collect();
            placed(substitute_within(pattern, &map), &own)
        };
        // GEN-12: in the order they are declared, whatever order the
        // call writes them in.
        let mut ordered: Vec<&(usize, SyntaxNode)> = named.iter().collect();
        ordered.sort_by_key(|(index, _)| *index);
        for (index, node) in ordered {
            let GenericKind::Value(pattern) = &generics[*index].kind else {
                continue;
            };
            let place = format!("the generic value parameter `{}`", generics[*index].name);
            let expected = type_of(*index, found);
            let expected = if whole(&expected) {
                let value = self.expression(node, Some(&expected));
                self.fit(value, &expected, &place);
                expected
            } else {
                // Its type names a parameter nothing has said anything
                // of: what it is given says.
                let value = self.expression(node, Some(&Type::Unknown));
                infer(pattern, &value.ty, generics, found);
                let expected = type_of(*index, found);
                self.fit(value, &expected, &place);
                expected
            };
            // The constant as a value of that type (EXPR-5): `fallback:
            // 1` is the `f64` one where `T` is an `f64`.
            if whole(&expected) {
                let value = self
                    .read_constant(node, Some(&expected))
                    .unwrap_or(ConstantValue::Unknown);
                found[*index] = Some(Argument::Constant(Constant {
                    text: constant_text(node),
                    value,
                }));
            }
        }
        for &index in defaulted {
            let generic = &generics[index];
            let GenericKind::Value(declared) = &generic.kind else {
                continue;
            };
            let Some(default) = generic_default(&declaration.syntax, generic) else {
                continue;
            };
            // A default that is not of the type as it is declared is
            // reported there (`signatures`). One whose type names a
            // type parameter is of it for one call and not another: the
            // `2` of `<T, const fallback: T = 2>` is no `str`.
            let expected = type_of(index, found);
            match self.unfit_default(declaration, &default, &expected, declared) {
                Some(Ok(value)) => {
                    let Some(value) = defaulted_value(value, index, &own, found) else {
                        found[index] = None;
                        continue;
                    };
                    // The default as a value of the type the call
                    // resolves (EXPR-5): the `f64` 2 where `T` is one.
                    found[index] = Some(Argument::Constant(Constant {
                        text: constant_text(&default),
                        value,
                    }));
                }
                Some(Err(described)) => {
                    self.diagnostics.emit(error(
                        Category::Type,
                        codes::TYPE_MISMATCH,
                        Location::new(self.file, callee.span()),
                        format!(
                            "this leaves `{}` to its default, {described}, and `{}` is \
                             `{expected}` here",
                            generic.name, generic.name
                        ),
                        "GEN-13",
                    ));
                }
                None => {}
            }
        }
    }

    /// The default `default` of a generic value parameter of
    /// `declaration`, declared of type `declared`, as a value of
    /// `resolved`, the type a call resolves the parameter to (GEN-13):
    /// `Ok` with it where it is one, and `Err` with what it is where it
    /// is of another type and of the one declared. `None` where that is
    /// not known, and where it is not of the type declared, which is
    /// reported where it is declared.
    pub(super) fn unfit_default(
        &self,
        declaration: &Declaration,
        default: &SyntaxNode,
        resolved: &Type,
        declared: &Type,
    ) -> Option<Result<ConstantValue, String>> {
        if !whole(resolved) {
            return None;
        }
        let file = declaration.at.file;
        // An implied member is of no type but an enum, and of no enum
        // but one that has it (TYPE-7: "With no enum type expected,
        // `.member` rejects"): `.slow` where the call resolves `T` to an
        // `i64`, and `.medium` where it resolves it to a `Mode` with no
        // such member. Where the declared type names a parameter, it is
        // the call's to tell; otherwise it is reported where declared.
        let no_member = |fault: &Fault| matches!(fault, Fault::NoEnum | Fault::NoMember(_));
        let as_written = || -> Option<Result<ConstantValue, String>> {
            let read = self.typed_in(file, default, None).ok()?;
            Some(Err(crate::constant::described(&read)))
        };
        let read = match self.typed_in(file, default, Some(resolved)) {
            Ok(read) => read,
            Err(fault) if no_member(&fault) && names_parameter(declared) => return as_written(),
            Err(_) => return None,
        };
        let fits_declared = names_parameter(declared)
            || crate::constant::fit(read.clone(), Some(declared), self.program).is_ok();
        match crate::constant::fit(read.clone(), Some(resolved), self.program) {
            Ok(value) => Some(Ok(value)),
            Err(Fault::Mismatch(_)) if fits_declared => {
                Some(Err(crate::constant::described(&read)))
            }
            Err(fault) if no_member(&fault) && names_parameter(declared) => as_written(),
            Err(_) => None,
        }
    }

    /// Separate explicitly written generic arguments from the types of
    /// a positional pack (GEN-8, GEN-14, and the owner's ruling on #336).
    /// Named packs get their types from the named arguments they capture.
    pub(super) fn explicit_generics(
        &mut self,
        name: &SyntaxToken,
        signature: &Signature,
        given: Option<&CallArguments>,
    ) -> Option<ExplicitGenerics> {
        let generics = &signature.generics;
        let first_pack = generics
            .iter()
            .position(|generic| matches!(generic.kind, GenericKind::Pack { .. }));
        let fixed_count = first_pack.unwrap_or(generics.len());
        let positional = signature
            .parameters
            .iter()
            .find(|parameter| parameter.pack == Some(Pack::Positional))
            .and_then(|parameter| type_pack(parameter, generics));
        let written = given.map(|given| given.arguments.as_slice());
        let prefix = written.map(|written| {
            if positional.is_some() {
                &written[..fixed_count.min(written.len())]
            } else {
                written
            }
        });
        let (mut fixed, mut faulty) =
            self.generic_arguments(name, &generics[..fixed_count], prefix)?;
        fixed.resize(generics.len(), None);
        let mut pack = None;
        if let Some(index) = positional
            && let Some(types) = written.and_then(|written| written.get(fixed_count..))
            && !types.is_empty()
        {
            let types = types
                .iter()
                .map(|(at, argument)| match argument {
                    Argument::Type(ty) => ty.clone(),
                    Argument::Constant(_) => {
                        faulty = true;
                        self.diagnostics.emit(error(
                            Category::Type,
                            codes::GENERIC_ARGUMENT_KIND,
                            *at,
                            format!(
                                "the pack `{}` takes types, and this is a value",
                                generics[index].name
                            ),
                            "GEN-14",
                        ));
                        Type::Unknown
                    }
                })
                .collect();
            pack = Some(hir::TypePackArgument::positional(index, types));
        }
        Some(ExplicitGenerics {
            fixed,
            faulty,
            pack,
        })
    }

    /// What the generic arguments a call writes say of the callee's
    /// generic parameters `generics`: an entry for each parameter,
    /// `None` for one the call does not write, and whether an argument
    /// was reported where it is written. `None` when the arguments are
    /// rejected here.
    ///
    /// GEN-8: a call "may supply the first of them and leave the rest
    /// to be resolved". GEN-14: "Check each argument against the
    /// corresponding parameter kind."
    pub(super) fn generic_arguments(
        &mut self,
        name: &SyntaxToken,
        generics: &[GenericParameter],
        given: Option<&[(Location, Argument)]>,
    ) -> Option<(Vec<Option<Argument>>, bool)> {
        let mut found: Vec<Option<Argument>> = vec![None; generics.len()];
        let Some(given) = given else {
            return Some((found, false));
        };
        let callee = name.text();
        if given.len() > generics.len() {
            let takes = match generics.len() {
                0 => "no generic arguments".to_string(),
                1 => "1 generic argument".to_string(),
                count => format!("{count} generic arguments"),
            };
            self.diagnostics.emit(error(
                Category::Type,
                codes::GENERIC_ARGUMENT_COUNT,
                Location::new(self.file, name.span()),
                format!("`{callee}` takes {takes} and is given {}", given.len()),
                "GEN-8",
            ));
            return None;
        }
        let mut faulty = false;
        let mut rejected = false;
        for (position, (at, argument)) in given.iter().enumerate() {
            let generic = &generics[position];
            let mismatch = match (&generic.kind, argument) {
                // Reported where it is written.
                (_, Argument::Type(Type::Unknown)) => {
                    faulty = true;
                    None
                }
                (GenericKind::Value(_), Argument::Type(ty)) => Some(format!(
                    "`{callee}` takes a value for `{}`, and `{ty}` is a type",
                    generic.name
                )),
                (GenericKind::Type, Argument::Constant(_)) => Some(format!(
                    "`{callee}` takes a type for `{}`, and this is a value",
                    generic.name
                )),
                // A parameter whose kind its signature does not settle
                // is what the call gives it (GEN-13).
                _ => {
                    // GEN-8: "The value a call gives a generic value
                    // parameter is a compile-time constant ... Any other
                    // rejects." What it comes to has been read with the
                    // types (`read`); one not valued is read again here
                    // for what it is.
                    if let Argument::Constant(Constant {
                        value: ConstantValue::Unevaluated,
                        ..
                    }) = argument
                        && let Some(node) = self.node_at(*at)
                        && let Some(why) = Self::not_constant(&self.read_constant(&node, None))
                    {
                        self.diagnostics.emit(error(
                            Category::Type,
                            codes::GENERIC_ARGUMENT_CONSTANT,
                            *at,
                            format!(
                                "`{}` is a generic value parameter, and this is no \
                                 compile-time constant: {why}",
                                generic.name
                            ),
                            "GEN-8",
                        ));
                    }
                    found[position] = Some(argument.clone());
                    None
                }
            };
            if let Some(message) = mismatch {
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::GENERIC_ARGUMENT_KIND,
                    *at,
                    message,
                    "GEN-14",
                ));
                rejected = true;
            }
        }
        (!rejected).then_some((found, faulty))
    }

    /// An unwritten type pack takes each trace's type from its own
    /// cells. Fit the cells once that type is found, as for the traces
    /// of an eval with fixed parameters (FN-8, TST-5).
    fn infer_trace_pack_argument(
        &mut self,
        index: usize,
        node: &SyntaxNode,
        slot: &Slot,
    ) -> hir::Argument {
        let mark = self.mark();
        let before = self.diagnostics.len();
        let argument = self.argument(Some(index), node, Some((slot, true)), Takes::Parameters);
        if self.diagnostics.len() == before
            && matches!(argument.value.kind, hir::ExprKind::Trace(_))
            && let Some(ty) =
                trace_pack_type(&argument.value.ty).filter(|ty| whole(ty) && !open(ty))
            && self.may_reread()
        {
            let typed = Slot {
                ty: trace_input_type(ty.clone()),
                ..*slot
            };
            self.take_back(mark, false);
            return self.argument(Some(index), node, Some((&typed, true)), Takes::Parameters);
        }
        argument
    }

    /// Reads the arguments of a call into places whose types name
    /// generic parameters, finding what those stand for from the values
    /// given (GEN-2). `found` is what is known of each parameter
    /// before; what comes back is every argument, each fitted to its
    /// place with what was found put in, and what is known after.
    ///
    /// The value of a place whose type names a parameter not yet found
    /// says what the parameter is: `1` for `first: T`, `[1]` for
    /// `values: list<T>`. Such values are read first, a call's in the
    /// order its parameters are declared in (GEN-12) and a
    /// construction's in the order written, with nothing expected of
    /// them, but for `null` and `.member`, which take their type from
    /// the place and say nothing of it.
    pub(super) fn infer_arguments(
        &mut self,
        name: &SyntaxToken,
        list: &SyntaxNode,
        places: &Places,
        mut found: Vec<Option<Argument>>,
        takes: Takes,
    ) -> Inferred {
        let Places {
            slots,
            patterns,
            generics,
            settled,
            aside,
            values: named,
            written_pack,
            result_fallback,
            defaults,
            callable,
        } = places;
        // The parameters being resolved. A place's type that still
        // names one once what was found is in place is not known.
        let own: Vec<Location> = generics.iter().map(|generic| generic.at).collect();
        let dependent = callable.and_then(|(module, signature)| {
            dependent_slot(&self.program.modules[module], signature)
        });
        let mut assigned = self.assign_dependent(name, list, slots, takes, aside, dependent);
        let mut values: Vec<Option<Expr>> = vec![None; assigned.len()];
        // True for a value kept from its first reading that is not
        // then fitted to its place.
        let mut unfitted: Vec<bool> = vec![false; assigned.len()];
        // The values that say what a parameter is are read first, out
        // of the order written. A block among the arguments may assign
        // what another reads (CTRL-10), so with one such a reading is
        // tried and taken back, and every argument is then read where
        // it is written.
        let ordered = assigned.iter().any(|argument| holds_block(&argument.value));
        // GEN-12: a call's parameters say what a generic is "in declared
        // parameter order". A struct's fields have no order of their
        // own (TYPE-5: named, "in any order"), so the first written
        // decides (GEN-2).
        let order = match takes {
            Takes::Parameters => declared_order(&assigned),
            Takes::Fields => (0..assigned.len()).collect(),
        };
        // The generic parameters an argument decided: what the call
        // writes, or its place expects, is not widened (GEN-2).
        let mut by_argument = vec![false; generics.len()];
        // Written value arguments and static function templates are
        // checked during the source pass. Preserve the precedence of
        // argument inference over the result payload fallback, too.
        let mut source_contexts: Vec<_> = generics
            .iter()
            .zip(&found)
            .filter_map(|(generic, value)| match (&generic.kind, value) {
                (GenericKind::Value(ty), Some(_)) => Some(ty),
                _ => None,
            })
            .chain(
                patterns
                    .iter()
                    .copied()
                    .filter(|ty| matches!(ty, Type::Function(_))),
            )
            .chain(result_fallback.iter().map(|(result, _)| *result))
            .collect();
        if takes == Takes::Fields {
            // Constructor replay completes type arguments and fits
            // typed fields. Context-only fields and dependent generic
            // defaults still need their original source reading.
            source_contexts.extend(assigned.iter().filter_map(|field| {
                let slot = field.slot?;
                field_needs_argument_place(&field.value).then_some(patterns[slot])
            }));
            source_contexts.extend(generics.iter().zip(defaults).filter_map(
                |(generic, default)| match (&generic.kind, default) {
                    (GenericKind::Value(ty), Some(_)) => Some(ty),
                    _ => None,
                },
            ));
        }
        let mut late = std::collections::HashSet::new();
        for position in order {
            let Assigned {
                slot, value: node, ..
            } = &assigned[position];
            let Some(index) = *slot else {
                continue;
            };
            if slots[index].type_pack.is_some() {
                continue;
            }
            let pattern = patterns[index];
            if let Type::Function(_) = pattern {
                let arguments: Vec<_> = found
                    .iter()
                    .map(|found| found.clone().unwrap_or(Argument::Type(Type::Unknown)))
                    .collect();
                let map: Vec<_> = own.iter().copied().zip(&arguments).collect();
                let Type::Function(template) = substitute_within(pattern, &map) else {
                    continue;
                };
                let mark = self.mark();
                let actual = self.function_argument(node, &template);
                self.take_back(mark, false);
                infer(pattern, &actual.ty, generics, &mut found);
                continue;
            }
            let unfound: Vec<Location> = generics
                .iter()
                .zip(&found)
                .filter(|(_, found)| found.is_none())
                .map(|(generic, _)| generic.at)
                .collect();
            // GEN-2: "One later argument changes what an earlier
            // argument decided: an abstract struct that the type
            // decided inherits from widens the parameter to that
            // struct". So a parameter that is `T` and nothing more is
            // read here though an earlier argument has said what `T`
            // is, where that is a struct.
            let widenable = match bare_parameter(pattern, generics) {
                Some(generic) if takes == Takes::Parameters && by_argument[generic] => {
                    matches!(&found[generic], Some(Argument::Type(Type::Named { .. })))
                        .then_some(generic)
                }
                _ => None,
            };
            if takes_its_type(node) || !(mentions(pattern, &unfound) || widenable.is_some()) {
                continue;
            }
            // What the argument's type says: a parameter not yet found,
            // or a wider one for a parameter an argument decided.
            let listed = slots[index].listed;
            let mut learn = |this: &Self, value: &Expr, found: &mut Vec<Option<Argument>>| {
                let unsettled = this.graph
                    && this.hook.is_none()
                    && !whole(&value.ty)
                    && this.category(value) == ValueCategory::Unsettled;
                let actual = this.inferred_from(listed, value, takes);
                let actual =
                    this.inference_input(pattern, &actual, generics, found, *result_fallback);
                if let Some(generic) = widenable {
                    if let Some(Argument::Type(decided)) = &found[generic]
                        && this.widens(decided, actual)
                    {
                        found[generic] = Some(Argument::Type(actual.clone()));
                    }
                    return;
                }
                let before: Vec<bool> = found.iter().map(Option::is_some).collect();
                infer(pattern, actual, generics, found);
                for (index, was) in before.into_iter().enumerate() {
                    if was {
                        continue;
                    }
                    let generic = &generics[index];
                    if late.contains(&generic.at)
                        && generic.kind == GenericKind::Type
                        && !source_contexts.iter().any(|ty| mentions(ty, &[generic.at]))
                    {
                        // GEN-2 and GEN-12: the earlier unresolved
                        // input gets the first opportunity to bind
                        // this parameter, in written field order or
                        // declared parameter order.
                        found[index] = None;
                    }
                    if unsettled && found[index].is_none() && mentions(pattern, &[generic.at]) {
                        late.insert(generic.at);
                    }
                    by_argument[index] |= found[index].is_some();
                }
            };
            // Nothing is expected of the value while its place is not
            // known, but that the place is an endpoint: the argument of
            // `valid<T>(input: endpoint<T>)` is an endpoint of whatever
            // the input holds, and says what `T` is (TIME-9).
            // In an eval the place is a trace (TST-5): a sequence there
            // is one written out, and its cells say what it is of.
            let unknown = Type::Unknown;
            let trace = Type::Trace {
                cell: Box::new(Type::Unknown),
                style: None,
            };
            let read_as = match pattern {
                Type::Endpoint(_) => pattern,
                Type::Trace { .. } => &trace,
                _ => &unknown,
            };
            // TST-16: what a trace is where a `const fn` is given it
            // says what the parameter is, as any value's type does.
            let as_given = |value: hir::Expr| {
                if listed { publications(value) } else { value }
            };
            if ordered {
                if self.may_reread() {
                    let mark = self.mark();
                    let value = as_given(self.expression(node, Some(read_as)));
                    learn(self, &value, &mut found);
                    self.take_back(mark, false);
                }
                continue;
            }
            let reported = self.diagnostics.len();
            let value = as_given(self.expression(node, Some(read_as)));
            learn(self, &value, &mut found);
            // A value with no whole type while nothing is known of its
            // place, `[1, "x"]`, has not been judged. Neither has one
            // that is read as the type of its place, a sequence or a
            // tuple written out: its entries and a list's size are the
            // place's to say (EXPR-5, EXPR-9, TYPE-10), so `[1, 2]` is
            // a `list<i64>` here and a `list<i64, 2>` where one is
            // expected. Each is read again below as the place's type,
            // once the parameters are found. One that reported
            // something is read once.
            let by_place = typed_by_place(node);
            let again = (by_place || !whole(&value.ty))
                && self.diagnostics.len() == reported
                && self.may_reread();
            if !again {
                values[position] = Some(value);
                // Kept as it was read, with no place: it is not held to
                // the place's type, which that reading did not know.
                unfitted[position] = by_place;
            }
        }
        // GEN-13: a generic value parameter's type may name a type
        // parameter, the `T` of `<T, const fallback: T>`. What the call
        // gives it says what that is where the places have not, in the
        // order declared (GEN-12), and before any argument is held to
        // its place: the `null` of `pick(null, fallback: 1)` is then
        // given to an `i64`. The value is read again, and held to its
        // own type, once every parameter is found.
        let mut ordered: Vec<&(usize, SyntaxNode)> = named.iter().collect();
        ordered.sort_by_key(|(index, _)| *index);
        for (index, node) in ordered {
            let GenericKind::Value(pattern) = &generics[*index].kind else {
                continue;
            };
            let unfound: Vec<Location> = generics
                .iter()
                .zip(&found)
                .filter(|(_, found)| found.is_none())
                .map(|(generic, _)| generic.at)
                .collect();
            if takes_its_type(node) || !mentions(pattern, &unfound) || !self.may_reread() {
                continue;
            }
            let mark = self.mark();
            let value = self.expression(node, Some(&Type::Unknown));
            infer(pattern, &value.ty, generics, &mut found);
            self.take_back(mark, false);
        }
        if let Some((result, requested)) = result_fallback {
            infer(
                result,
                not_null(Some(requested)).unwrap_or(requested),
                generics,
                &mut found,
            );
        }
        // GEN-8: "A generic value parameter the call does not give takes
        // its default, when it has one." Nothing has said what it is,
        // and it is its default before any argument is held to its
        // place: `sized([1, 2, 3])` for `sized<const N: i64 = 2>(items:
        // list<i64, N>)` gives three entries to a list of two.
        let mut defaulted = Vec::new();
        for (index, default) in defaults.iter().enumerate() {
            if found[index].is_none()
                && let Some(default) = default
                && let Some(value) = defaulted_value(default.value.clone(), index, &own, &found)
            {
                found[index] = Some(Argument::Constant(Constant {
                    text: default.text.clone(),
                    value,
                }));
                defaulted.push(index);
            }
        }
        // A resolved callback determines its reflected types before
        // another place can use a keyed member of those types (GEN-17).
        let type_packs = callable.map_or_else(Vec::new, |(module, signature)| {
            let unresolved = Argument::Type(Type::Unknown);
            let map: Vec<_> = generics
                .iter()
                .zip(&found)
                .filter(|(generic, _)| !matches!(generic.kind, GenericKind::Pack { .. }))
                .map(|(generic, argument)| (generic.at, argument.as_ref().unwrap_or(&unresolved)))
                .collect();
            self.read_dependent_call(
                (name, list),
                (module, signature, dependent),
                (slots, &map),
                (&mut assigned, &mut values),
            )
        });
        for (entry, value) in assigned.iter().zip(&values) {
            if let (Some(index), Some(value)) = (entry.slot, value)
                && matches!(value.kind, hir::ExprKind::StaticCallable(_))
            {
                infer(patterns[index], &value.ty, generics, &mut found);
            }
        }
        let pack_types = PackTypes::new(generics, &type_packs);
        // Each argument now takes its place with the resolved parameters.
        let unresolved = Argument::Type(Type::Unknown);
        let map: Vec<_> = generics
            .iter()
            .zip(&found)
            .filter(|(generic, _)| !matches!(generic.kind, GenericKind::Pack { .. }))
            .map(|(generic, argument)| (generic.at, argument.as_ref().unwrap_or(&unresolved)))
            .collect();
        // The parameters nothing resolved.
        let unfound: Vec<Location> = generics
            .iter()
            .zip(&found)
            .filter(|(_, found)| found.is_none())
            .map(|(generic, _)| generic.at)
            .collect();
        let mut contextual = Vec::new();
        let mut missing_pack_context = Vec::new();
        let arguments: Vec<hir::Argument> = assigned
            .iter()
            .enumerate()
            .map(|(position, assigned)| {
                let Assigned {
                    slot,
                    pack,
                    value: node,
                    ..
                } = assigned;
                let Some(index) = *slot else {
                    return self.argument(None, node, None, takes);
                };
                let pattern = patterns[index];
                let traced = matches!(pattern, Type::Trace { .. });
                let mut resolved_type = |ty| {
                    let ty = pack_types.project(ty, &map).unwrap_or_else(|missing| {
                        self.pack_member_fault(self.at(node), &missing);
                        Type::Unknown
                    });
                    placed(ty, &own)
                };
                let written_type = written_pack
                    .filter(|(generic, _)| slots[index].type_pack == Some(*generic))
                    .and_then(|(_, types)| types.get(pack.as_ref()?.0));
                let ty = match (written_type, pattern) {
                    (Some(ty), Type::Trace { .. }) => trace_input_type(ty.clone()),
                    (Some(ty), _) => ty.clone(),
                    // TST-5: form the publication type after a homogeneous
                    // or heterogeneous pack's input type is substituted.
                    (None, Type::Trace { cell, .. }) if pack.is_some() && names_parameter(cell) => {
                        trace_input_type(resolved_type(cell))
                    }
                    (None, _) => resolved_type(pattern),
                };
                let slot = Slot { ty, ..slots[index] };
                // EXPR-9, ERR-2 and FN-17: a contextual expression or
                // anonymous callback may need the slot another argument
                // can settle in the graph instance (GEN-2).
                let depends_on_instance = !late.is_empty() && !traced && !whole(&slot.ty) && {
                    let mut missing = unfound
                        .iter()
                        .filter(|at| mentions(pattern, &[**at]))
                        .peekable();
                    let needs_place = match takes {
                        Takes::Parameters => {
                            needs_argument_place(node)
                                || matches!(slot.ty, Type::Function(_))
                                    && ungrouped(node.clone()).kind() == SyntaxKind::FnExpr
                        }
                        Takes::Fields => field_needs_argument_place(node),
                    };
                    needs_place && missing.peek().is_some() && missing.all(|at| late.contains(at))
                };
                // A place whose type names a parameter nothing resolved
                // is not known, and where the call is settled it never
                // will be. Its argument is then read as one of which
                // nothing is expected, so that what it leaves unresolved
                // itself is reported: the `repeat(1)` of
                // `same(repeat(1), repeat(2))`. `null` and `.member`
                // have no reading of their own. An unwritten type pack
                // learns each argument's type separately (GEN-8). A
                // captured eval trace stays a trace even when its cells
                // give no type (TST-5).
                let unfound_type = match slot.type_pack {
                    Some(parameter) => written_pack.is_none_or(|(given, _)| given != parameter),
                    None => mentions(patterns[index], &unfound),
                };
                let placeless = *settled
                    && !matches!(slot.ty, Type::Function(_))
                    && !(traced && pack.is_some())
                    && !takes_its_type(node)
                    && !depends_on_instance
                    && unfound_type;
                let reported = self.diagnostics.len();
                let facts = depends_on_instance.then(|| {
                    self.present
                        .iter()
                        .filter_map(|fact| match fact {
                            Fact::Present(local) => {
                                Some(hir::ArgumentFact::Present(self.locals[*local].at))
                            }
                            Fact::Valid(local) => {
                                Some(hir::ArgumentFact::Valid(self.locals[*local].at))
                            }
                            Fact::Success(local) => {
                                Some(hir::ArgumentFact::Success(self.locals[*local].at))
                            }
                            Fact::Failure(local) => {
                                Some(hir::ArgumentFact::Failure(self.locals[*local].at))
                            }
                            // Child reads consume these only in hooks
                            // (TIME-1); replay is graph composition.
                            Fact::AllValid(_) | Fact::ChildValid(_) => None,
                        })
                        .collect()
                });
                let mut argument = match values[position].take() {
                    None if placeless => {
                        let value = self.expression(node, None);
                        // TST-16: a const function takes the publications
                        // of a trace even when the slot supplies no type.
                        let value = if slot.listed {
                            publications(value)
                        } else {
                            value
                        };
                        hir::Argument {
                            parameter: Some(index),
                            pack: None,
                            value,
                        }
                    }
                    Some(value)
                        if unfitted[position]
                            || matches!(slot.ty, Type::Function(_))
                                && matches!(
                                    value.kind,
                                    hir::ExprKind::StaticCallable(_) | hir::ExprKind::Unread
                                ) =>
                    {
                        hir::Argument {
                            parameter: Some(index),
                            pack: None,
                            value,
                        }
                    }
                    // A value that holds a trace is the place's to
                    // report where the call is finished (TST-3), and is
                    // not held to its type.
                    Some(value) if !traced && holds_trace(&value.ty) => hir::Argument {
                        parameter: Some(index),
                        pack: None,
                        value,
                    },
                    Some(value) if self.lifted_window(&slot.ty, slot.listed, &value, takes) => {
                        hir::Argument {
                            parameter: Some(index),
                            pack: None,
                            value,
                        }
                    }
                    Some(value) => {
                        let place = format!("the {} `{}`", takes.noun(), slot.name);
                        let value = if takes == Takes::Parameters {
                            self.fit_input(value, &slot.ty, &place)
                        } else {
                            self.fit(value, &slot.ty, &place)
                        };
                        hir::Argument {
                            parameter: Some(index),
                            pack: None,
                            value,
                        }
                    }
                    None if traced && written_type.is_none() && slot.type_pack.is_some() => {
                        self.infer_trace_pack_argument(index, node, &slot)
                    }
                    None => self.argument(Some(index), node, Some((&slot, traced)), takes),
                };
                argument.pack = pack.as_ref().map(|(_, key)| key.clone());
                if let Some(parameter) = slot.type_pack
                    && !whole(&argument.value.ty)
                    && takes_its_type(node)
                {
                    missing_pack_context.push(parameter);
                }
                if let Some(facts) = facts
                    && !whole(&argument.value.ty)
                    && self.diagnostics.len() == reported
                {
                    contextual.push(hir::ContextualArgument {
                        position,
                        at: self.at(node),
                        facts,
                    });
                }
                argument
            })
            .collect();
        let unknown = assigned.iter().zip(&arguments).any(|(assigned, argument)| {
            if whole(&argument.value.ty) || takes_its_type(&assigned.value) {
                return false;
            }
            if argument.pack.is_some()
                && assigned
                    .slot
                    .is_some_and(|index| slots[index].type_pack.is_some())
            {
                // GEN-8: each captured argument must resolve its own
                // type. Report an unresolved pack unless an earlier
                // error in the argument already explains it (ERR-9).
                let span = assigned.value.span();
                return self.diagnostics.as_slice().iter().any(|report| {
                    report.severity == Severity::Error
                        && report.primary.file == self.file
                        && report.primary.span.start >= span.start
                        && report.primary.span.end <= span.end
                });
            }
            // Gaps in a homogeneous packed trace give no evidence for
            // inference. GEN-8 reports an unresolved generic at the call.
            let empty_trace = argument.pack.is_some()
                && assigned
                    .slot
                    .is_some_and(|index| matches!(patterns[index], Type::Trace { .. }))
                && matches!(&argument.value.kind, hir::ExprKind::Trace(cells)
                    if cells.iter().all(|cell| cell.value.is_none()));
            !empty_trace
        });
        Inferred {
            arguments,
            found,
            unknown,
            defaulted,
            contextual,
            missing_pack_context,
            type_packs,
        }
    }
}

/// Whether an argument has no complete type without its expected place.
fn needs_argument_place(node: &SyntaxNode) -> bool {
    let node = ungrouped(node.clone());
    takes_its_type(&node)
        || node.kind() == SyntaxKind::SequenceExpr && node.children().next().is_none()
}

/// CTRL-10: a constructor supplies a block's tail with its field type.
/// Only the contextual leaves that field settlement can replay wait.
fn field_needs_argument_place(node: &SyntaxNode) -> bool {
    let mut tail = ungrouped(node.clone());
    if tail.kind() != SyntaxKind::Block {
        return needs_argument_place(&tail);
    }
    while let Some(value) = value_of_branch(&tail) {
        tail = ungrouped(value);
    }
    written_null(&tail)
        || tail.kind() == SyntaxKind::ImpliedMember
        || (tail.kind() == SyntaxKind::SequenceExpr && tail.children().next().is_none())
}

/// Explicit arguments to ordinary generics and to the positional type
/// pack, which has no single type (GEN-8, GEN-10).
pub(super) struct ExplicitGenerics {
    /// Types or constants aligned with the declared generic parameters.
    pub(super) fixed: Vec<Option<Argument>>,
    /// True when one of the arguments was already reported.
    pub(super) faulty: bool,
    /// The complete type sequence the call writes for the positional pack.
    pub(super) pack: Option<hir::TypePackArgument>,
}

/// The generic parameter of a heterogeneous value pack (FN-8).
pub(super) fn type_pack(parameter: &Parameter, generics: &[GenericParameter]) -> Option<usize> {
    parameter.pack?;
    bare_parameter(&parameter.ty, generics)
        .filter(|&index| matches!(generics[index].kind, GenericKind::Pack { .. }))
}

/// A value pack observes its own bounds, those of its type pack, and
/// the length of any type sequence the call writes (GEN-10, #336).
pub(super) fn pack_bounds(
    parameter: &Parameter,
    generics: &[GenericParameter],
    written: Option<&hir::TypePackArgument>,
) -> (u64, Option<u64>) {
    let (mut minimum, mut maximum) = parameter.pack_bounds;
    if let Some(index) = type_pack(parameter, generics) {
        if let GenericKind::Pack {
            minimum: lower,
            maximum: upper,
        } = generics[index].kind
        {
            minimum = minimum.max(lower);
            maximum = maximum.into_iter().chain(upper).min();
        }
        if let Some(written) = written.filter(|pack| pack.parameter == index) {
            let count = written.types.len() as u64;
            minimum = minimum.max(count);
            maximum = Some(maximum.map_or(count, |maximum| maximum.min(count)));
        }
    }
    (minimum, maximum)
}

/// The places a call's arguments are given to, where their types name
/// generic parameters to be found from the arguments (GEN-2).
pub(super) struct Places<'s> {
    /// Each place: its name, whether it must be given, the pack it is.
    pub(super) slots: &'s [Slot<'s>],
    /// The type each place is declared with, generic parameters and
    /// all. One for each of `slots`.
    pub(super) patterns: Vec<&'s Type>,
    /// The generic parameters being found.
    pub(super) generics: &'s [GenericParameter],
    /// True when nothing later says more of these places: the call's
    /// own place is known, or it has none. False while the call is read
    /// before its place is, which may yet say what a parameter is.
    pub(super) settled: bool,
    /// The names that are no place's: a named argument under one gives
    /// a generic value parameter of the callee (GEN-8), and is read
    /// with the call's generic arguments.
    pub(super) aside: &'s [&'s str],
    /// What the call gives those: each with the position of its
    /// parameter among `generics`.
    pub(super) values: &'s [(usize, SyntaxNode)],
    /// The written types of a call's positional type pack, when given.
    pub(super) written_pack: Option<(usize, &'s [Type])>,
    /// A requested result kept until the inputs determine whether they can
    /// supply its payload or leave it open.
    pub(super) result_fallback: Option<(&'s Type, &'s Type)>,
    /// The default of each generic value parameter that has one (GEN-2):
    /// an entry for each of `generics`, or none at all.
    pub(super) defaults: Vec<Option<Constant>>,
    /// A callable declaration whose callbacks may reflect parameter packs.
    pub(super) callable: Option<(usize, &'s Signature)>,
}

/// What [`Typer::infer_arguments`] read.
pub(super) struct Inferred {
    /// Every argument, each fitted to its place.
    pub(super) arguments: Vec<hir::Argument>,
    /// What is known of each generic parameter.
    pub(super) found: Vec<Option<Argument>>,
    /// True when an argument other than `null` and `.member`, which
    /// have no type of their own, has no whole type.
    pub(super) unknown: bool,
    /// The generic value parameters that took their defaults, by their
    /// positions among the generic parameters.
    pub(super) defaulted: Vec<usize>,
    /// The context-dependent arguments still waiting on graph types.
    pub(super) contextual: Vec<hir::ContextualArgument>,
    /// Generic packs with an element that has no type of its own
    /// (GEN-8, ERR-2), even when an instance selects other argument types.
    pub(super) missing_pack_context: Vec<usize>,
    /// Packs determined by static callback reflection (GEN-17).
    pub(super) type_packs: Vec<hir::TypePackArgument>,
}

/// The positions of a call's arguments in the order their places are
/// declared in, those given to no place last.
///
/// GEN-12: "A prior binding constrains every later occurrence in
/// declared parameter order." So in `same(right: 1, left: 2.0)` it is
/// `left` that says what `T` is.
fn declared_order(assigned: &[Assigned]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..assigned.len()).collect();
    // A stable sort: two arguments of one place keep their written
    // order.
    order.sort_by_key(|&position| assigned[position].slot.unwrap_or(usize::MAX));
    order
}

/// Where each generic parameter before a type pack is declared: the
/// ones an argument is given to place for place (GEN-2).
pub(super) fn fixed(generics: &[GenericParameter]) -> impl Iterator<Item = Location> + '_ {
    generics
        .iter()
        .take_while(|generic| !matches!(generic.kind, GenericKind::Pack { .. }))
        .map(|generic| generic.at)
}

/// The position among `generics` of the type parameter a type is, when
/// it is one and nothing more: the `T` of `value: T`, and not of
/// `values: list<T>`.
pub(super) fn bare_parameter(ty: &Type, generics: &[GenericParameter]) -> Option<usize> {
    match ty {
        Type::Parameter { at, .. } => generics.iter().position(|generic| generic.at == *at),
        _ => None,
    }
}

/// Finds what generic parameters stand for from a field's type,
/// `pattern`, and the type of the value the field is given, `actual`:
/// `T` is `i64` when `list<T>` is given a `list<i64>`, and `N` is 2
/// when `list<i64, N>` is given a `list<i64, 2>`. `found` has an entry
/// for each of `generics`; one already found is left as it is, and a
/// type that is not known finds nothing.
pub(super) fn infer(
    pattern: &Type,
    actual: &Type,
    generics: &[GenericParameter],
    found: &mut [Option<Argument>],
) {
    match (pattern, actual) {
        (_, Type::Unknown) => {}
        (Type::Parameter { at, .. }, actual) => {
            find(*at, Argument::Type(actual.clone()), generics, found);
        }
        (Type::Function(pattern), Type::Function(actual)) => {
            for (pattern, actual) in pattern.parameters.iter().zip(&actual.parameters) {
                infer(pattern.ty(), actual.ty(), generics, found);
            }
            if let (Some(pattern), Some(actual)) = (&pattern.result, &actual.result) {
                infer(pattern, actual, generics, found);
            }
        }
        (Type::Nullable(pattern), Type::Nullable(actual)) => {
            infer(pattern, actual, generics, found)
        }
        // A `T` given where a `T?` is expected (TYPE-14).
        (Type::Nullable(pattern), actual) => infer(pattern, actual, generics, found),
        (
            Type::Named {
                module,
                declaration,
                arguments,
                ..
            },
            Type::Named {
                module: actual_module,
                declaration: actual_declaration,
                arguments: actual_arguments,
                ..
            },
        ) if (module, declaration) == (actual_module, actual_declaration) => {
            for pair in arguments.iter().zip(actual_arguments) {
                match pair {
                    (Argument::Type(pattern), Argument::Type(actual)) => {
                        infer(pattern, actual, generics, found);
                    }
                    (Argument::Constant(pattern), Argument::Constant(actual)) => {
                        infer_constant(pattern, actual, generics, found);
                    }
                    _ => {}
                }
            }
        }
        (Type::Tuple(patterns), Type::Tuple(actuals)) if patterns.len() == actuals.len() => {
            for (pattern, actual) in patterns.iter().zip(actuals) {
                infer(pattern, actual, generics, found);
            }
        }
        (
            Type::List { element, size },
            Type::List {
                element: actual,
                size: actual_size,
            },
        ) => {
            infer(element, actual, generics, found);
            if let (Some(size), Some(actual_size)) = (size, actual_size) {
                infer_constant(size, actual_size, generics, found);
            }
        }
        (Type::Set(pattern), Type::Set(actual))
        | (Type::Endpoint(pattern), Type::Endpoint(actual))
        | (Type::Atomic(pattern), Type::Atomic(actual))
        | (Type::Delta(pattern), Type::Delta(actual)) => infer(pattern, actual, generics, found),
        // A generic target's trace has its parameter's own type as its
        // cell, and a tuple's trace the tuple it is of (TST-5,
        // `Type::trace_cell`): `tuple<T, T>` is found from it.
        (Type::Trace { cell: pattern, .. }, Type::Trace { cell: actual, .. }) => match &**actual {
            Type::Delta(of)
                if matches!(**of, Type::Tuple(_)) && !matches!(**pattern, Type::Delta(_)) =>
            {
                infer(pattern, of, generics, found)
            }
            _ => infer(pattern, actual, generics, found),
        },
        // TYPE-9: "For non-composite S, `atomic<S>` normalizes to S
        // before matching". An `atomic<T>` given a value of a type that
        // is its own atomic says that `T` is that type.
        (Type::Atomic(pattern), actual)
            if !matches!(Type::atomic(actual.clone()), Type::Atomic(_)) =>
        {
            infer(pattern, actual, generics, found)
        }
        (
            Type::Dict { key, value },
            Type::Dict {
                key: actual_key,
                value: actual_value,
            },
        ) => {
            infer(key, actual_key, generics, found);
            infer(value, actual_value, generics, found);
        }
        (
            Type::RollingWindow {
                value,
                maximum,
                minimum,
            },
            Type::RollingWindow {
                value: actual,
                maximum: actual_maximum,
                minimum: actual_minimum,
            },
        ) => {
            infer(value, actual, generics, found);
            infer_constant(maximum, actual_maximum, generics, found);
            // TYPE-11: "Omitted minimum equals maximum."
            if let Some(minimum) = minimum {
                let actual_minimum = actual_minimum.as_ref().unwrap_or(actual_maximum);
                infer_constant(minimum, actual_minimum, generics, found);
            }
        }
        (
            Type::Result { value, error },
            Type::Result {
                value: actual_value,
                error: actual_error,
            },
        ) => {
            infer(value, actual_value, generics, found);
            infer(error, actual_error, generics, found);
        }
        _ => {}
    }
}

/// What a requested output decides about the result's type (GEN-2).
/// A nullable place alone leaves a bare type parameter open. An
/// explicit type argument decides it whole. A requested payload of a
/// fallible result can also be read from a nullable success (ERR-12,
/// ERR-15). In that case a bare success parameter can be decided by
/// the inputs first, such as a dictionary's value type in `get`.
/// The returned pair supplies the output constraint during inference.
pub(super) fn infer_from_place<'r, 'p>(
    result: &'r Type,
    requested: &'p Type,
    written: bool,
    generics: &[GenericParameter],
    found: &mut [Option<Argument>],
) -> Option<(&'r Type, &'p Type)> {
    let (result, fallible) = if let Type::Result { value, error } = result
        && !matches!(not_null(Some(requested)), Some(Type::Result { .. }))
    {
        (value.as_ref(), **error == Type::Error)
    } else {
        (result, false)
    };
    if !written && matches!(result, Type::Parameter { .. }) {
        if fallible {
            // A store read's inputs say nothing of V (core CAP-6).
            return Some((result, requested));
        }
        if matches!(requested, Type::Nullable(_)) {
            return None;
        }
    }
    let place = if written || matches!(result, Type::Nullable(_)) {
        requested
    } else {
        not_null(Some(requested)).unwrap_or(requested)
    };
    infer(result, place, generics, found);
    None
}

/// [`infer`], for a size or a constant argument: a generic value
/// parameter written there is the constant given in its place.
fn infer_constant(
    pattern: &Constant,
    actual: &Constant,
    generics: &[GenericParameter],
    found: &mut [Option<Argument>],
) {
    // A value nothing has said anything of says nothing here either:
    // an argument, or a default, may yet say what the parameter is.
    if is_unsaid(actual) {
        return;
    }
    infer_value(&pattern.value, actual, generics, found);
}

/// [`infer_constant`], for the value `pattern` is, at any depth inside a
/// list or a tuple: `[N, 1]` given `[2, 1]` says `N` is 2.
fn infer_value(
    pattern: &ConstantValue,
    actual: &Constant,
    generics: &[GenericParameter],
    found: &mut [Option<Argument>],
) {
    match (pattern, &actual.value) {
        (ConstantValue::Parameter { at, .. }, _) => {
            find(*at, Argument::Constant(actual.clone()), generics, found);
        }
        (ConstantValue::List(patterns), ConstantValue::List(values))
        | (ConstantValue::Tuple(patterns), ConstantValue::Tuple(values))
            if patterns.len() == values.len() =>
        {
            for (pattern, value) in patterns.iter().zip(values) {
                // An element has no text of its own: it is written as
                // the value it is.
                let element = Constant {
                    text: value.key().unwrap_or_else(|| actual.text.clone()),
                    value: value.clone(),
                };
                infer_value(pattern, &element, generics, found);
            }
        }
        _ => {}
    }
}

/// The constant that stands for a generic value nothing has said
/// anything of, in an output requested in part: it agrees with any, and
/// decides nothing.
pub(super) fn unsaid() -> Constant {
    Constant {
        text: "_".to_string(),
        value: ConstantValue::Unknown,
    }
}

/// True for [`unsaid`]. No constant a source writes is `_`.
fn is_unsaid(constant: &Constant) -> bool {
    constant.text == "_" && matches!(constant.value, ConstantValue::Unknown)
}

/// Records that the generic parameter declared at `at` stands for
/// `argument`, unless something has been found for it already or the
/// argument is a type with a part that is not known.
fn find(
    at: Location,
    argument: Argument,
    generics: &[GenericParameter],
    found: &mut [Option<Argument>],
) {
    // A type that is not known all the way down decides nothing: the
    // `tuple<list<_>, i64>` of `([], 0)` would let any list through in
    // its unknown part. A later argument may say the whole of it, and
    // this one is then read again as that.
    if let Argument::Type(ty) = &argument
        && !whole(ty)
    {
        return;
    }
    let position = generics.iter().position(|generic| generic.at == at);
    if let Some(position) = position
        && found[position].is_none()
    {
        found[position] = Some(argument);
    }
}

/// A type once generic arguments are put in place of the parameters
/// declared at `own`: not known when it still names one of them, which
/// has then been given no argument.
pub(super) fn placed(ty: Type, own: &[Location]) -> Type {
    if mentions(&ty, own) {
        Type::Unknown
    } else {
        ty
    }
}

/// True when `ty` names one of the generic parameters declared at
/// `own`: as a type, or as a value in a size or a constant argument.
pub(super) fn mentions(ty: &Type, own: &[Location]) -> bool {
    names_one_of(ty, own) || names_value_of(ty, own)
}

/// True when a size or a constant argument inside `ty` names a generic
/// value parameter declared at one of `own`: the `N` of `list<i64, N>`,
/// of `list<i64, N + 1>`, of `Box<[N]>`.
fn names_value_of(ty: &Type, own: &[Location]) -> bool {
    ty.any_part(&|_| false, &|constant| constant.value.names_one_of(own))
}

/// The default `value` of the generic value parameter at `index`, of a
/// declaration whose generic parameters are declared at `own`, with what
/// a call has `found` for the others in place. A default that names a
/// parameter before its own, `<const N: i64 = 1, const M: i64 = N>`, is
/// of what the call gives that one. `None` for one that names its own,
/// or one the call leaves unresolved, which gives it no value: GEN-8, "A
/// call rejects when it leaves a generic parameter unresolved".
fn defaulted_value(
    value: ConstantValue,
    index: usize,
    own: &[Location],
    found: &[Option<Argument>],
) -> Option<ConstantValue> {
    // What the call gives the parameter declared at `at`: none for the
    // one whose default this is.
    let given = |at: Location| -> Option<ConstantValue> {
        let other = own.iter().position(|own| *own == at)?;
        match found.get(other)? {
            Some(Argument::Constant(given)) if other != index => Some(given.value.clone()),
            _ => None,
        }
    };
    // The default is asked of what it names before what the call gives
    // is put in its place: a call in the function's own body may give
    // `N` its own `N`, `rec<N>(v)`, which is then no default left
    // unresolved.
    let unresolved: Vec<Location> = own
        .iter()
        .copied()
        .filter(|at| given(*at).is_none())
        .collect();
    (!value.names_one_of(&unresolved)).then(|| value.substituted(&given))
}
