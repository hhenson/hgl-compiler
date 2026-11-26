//! A call's arguments, each given to a parameter of a function or to a
//! field of a struct, and the construction of a struct (EXPR-2, TYPE-3,
//! TYPE-5). Part of the typing pass ([`super`]).

use std::collections::HashSet;

use hgl_diagnostics::{Category, Location};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};
use hgl_source::Span;

use super::Typer;
use super::evals::{holds_trace, publications};
use super::fit::whole;
use super::inference::{Inferred, Places, fixed, infer, mentions, placed};
use crate::declared::{agree, is_optional};
use crate::hir::{self, EndpointOperation, Expr, ExprKind, Value};
use crate::program::DeclarationKind;
use crate::syntax::{child, has_token, name_token, tokens};
use crate::types::{
    Argument, ConstantValue, Detail, GenericKind, GenericParameter, NamedKind, Pack, Type,
};
use crate::{RESULT_FORMS, codes, error};

impl<'a> Typer<'a> {
    /// `Name<arguments>(fields)`: the construction of a generic struct
    /// with its arguments written.
    pub(super) fn typed_constructor(
        &mut self,
        node: &SyntaxNode,
        expected: Option<&Type>,
    ) -> (ExprKind, Type) {
        // `Name<...>(...)` or `alias.Name<...>(...)`: the name is the
        // last identifier, and the type is recorded where the whole of
        // it starts.
        let start = match child(node, SyntaxKind::QualifiedName) {
            Some(qualified) => Some(qualified.span().start),
            None => name_token(node).map(|name| name.span().start),
        };
        let (Some(name), Some(start), Some(list)) =
            (name_token(node), start, child(node, SyntaxKind::ArgList))
        else {
            return (ExprKind::Unread, Type::Unknown);
        };
        // ERR-2: `ok` and `err` each take "no generic argument", and
        // no other result form has one to take. The form is read as it
        // would be without them.
        if child(node, SyntaxKind::QualifiedName).is_none() && RESULT_FORMS.contains(&name.text()) {
            self.diagnostics.emit(
                error(
                    Category::Type,
                    codes::GENERIC_ARGUMENT_COUNT,
                    Location::new(self.file, name.span()),
                    format!("`{}` takes no generic argument", name.text()),
                    "ERR-2",
                )
                .with_note("a result's types come from the `result<T, E>` its place expects"),
            );
            if let Some(form) = self.result_form(&name, &list, expected) {
                return form;
            }
        }
        // The type the `read` pass gave `Name<arguments>`.
        let written = self.written_type_at(start);
        match written.as_deref() {
            Some(Type::Named {
                module,
                declaration,
                arguments,
                ..
            }) if self.program.modules[*module].declarations[*declaration].kind
                == DeclarationKind::Struct =>
            {
                self.construct(
                    &name,
                    &list,
                    *module,
                    *declaration,
                    Some(arguments.clone()),
                    true,
                )
            }
            // GEN-8: "A call may supply its generic arguments as a
            // generic constructor does, `count<i64>(3)`." The `read`
            // pass kept them, the name being a function or an operator.
            _ => {
                let generics = self.call_arguments_at(start);
                // TIME-9: an endpoint operation written with its generic
                // argument, `delta_value<i64>(out)`, is the form it is
                // without one, and is held to the same rules; so is one
                // named through an alias of core, `c::valid<i64>(a)`.
                if self.hook.is_some()
                    && let Some(operation) = EndpointOperation::language_form(name.text())
                    && self.is_the_languages(&name)
                {
                    let read = self.endpoint(operation, &name, &list);
                    if operation == EndpointOperation::Scheduled {
                        self.scheduled_generics(node, &name);
                    } else {
                        self.endpoint_generics(&name, generics, &read.0);
                    }
                    return read;
                }
                // So is `scheduled<T>()` where no hook stands: it is
                // judged as `scheduled()` is there (`call_read`), and
                // takes no generic argument.
                if self.hook.is_none()
                    && EndpointOperation::language_form(name.text())
                        == Some(EndpointOperation::Scheduled)
                    && self.is_the_languages(&name)
                {
                    self.scheduled_outside_hooks(self.at(node));
                    self.scheduled_generics(node, &name);
                    return self.unread_call(&list);
                }
                match (generics, self.declarations(&name)) {
                    (Some(generics), Some(found)) => {
                        self.call_named(&name, &list, found, expected, Some(generics))
                    }
                    _ => self.unread_call(&list),
                }
            }
        }
    }

    /// The construction of the struct `declaration` of `module`. `given`
    /// is its generic arguments, when they are written or expected.
    ///
    /// TYPE-5: "An ordinary constructor supplies complete required
    /// data." TYPE-3: "abstract values are not constructible."
    pub(super) fn construct(
        &mut self,
        name: &SyntaxToken,
        list: &SyntaxNode,
        module: usize,
        declaration: usize,
        given: Option<Vec<Argument>>,
        settled: bool,
    ) -> (ExprKind, Type) {
        let program = self.program;
        let declared = &program.modules[module].declarations[declaration];
        let Some(Detail::Struct(read)) = program.modules[module].details.get(declaration) else {
            return self.unread_call(list);
        };
        let before = self.diagnostics.len();
        if has_token(&declared.syntax, SyntaxKind::KwAbstract) {
            self.diagnostics.emit(
                error(
                    Category::Type,
                    codes::STRUCT_ABSTRACT,
                    Location::new(self.file, name.span()),
                    format!(
                        "`{}` is an abstract struct and is not constructed",
                        declared.name
                    ),
                    "TYPE-3",
                )
                .with_related(declared.at, "declared here"),
            );
        }
        // Arguments of the wrong number or kind have been reported
        // where they are written (GEN-2, GEN-14): the struct made is
        // then not known, and nothing more is reported of it (ERR-9).
        let reported = given
            .as_ref()
            .is_some_and(|given| !applies(&read.generics, given));
        let given = given.filter(|_| !reported);
        // With none given, a generic struct's arguments are found from
        // the fields it is given.
        let inferring = given.is_none() && !read.generics.is_empty();
        let structure = |arguments| Type::Named {
            name: declared.name.clone(),
            module,
            declaration,
            arguments,
            kind: NamedKind::Struct,
        };
        // FN-19: a constructor receives the named entries through its
        // ordinary field binding once the enclosing pack is known.
        if super::forwarded_packs::spread_operands(list)
            .next()
            .is_some()
        {
            let result =
                (!inferring && !reported).then(|| structure(given.clone().unwrap_or_default()));
            if let Some(call) = self.defer_pack_call(list, result.as_ref(), None) {
                return call;
            }
        }
        // The struct's own parameters. A field's type that still names
        // one once the arguments are in place has been given none: with
        // no arguments given, or where a pack's place is not filled in.
        // It is then not known. A parameter of the function being read
        // is a type like any other.
        let own: Vec<Location> = read.generics.iter().map(|generic| generic.at).collect();
        // Each field's type, with the generic arguments in place when
        // they are given.
        let map: Vec<(Location, &Argument)> = match &given {
            Some(given) => fixed(&read.generics).zip(given).collect(),
            None => Vec::new(),
        };
        let slots: Vec<Slot> = read
            .members
            .iter()
            .map(|member| {
                let ty = self.program.specialize_type(&member.ty, &map);
                Slot {
                    name: &member.name,
                    configuration: false,
                    // TYPE-4: "A field is optional when its type is
                    // nullable ... or an `atomic<...>` of a nullable
                    // type", the generic arguments in place: the
                    // `value: T` of a `Box<i64?>` is.
                    required: !member.has_default && is_optional(&ty) != Some(true),
                    ty: placed(ty, &own),
                    pack: None,
                    pack_bounds: (0, None),
                    type_pack: None,
                    listed: false,
                }
            })
            .collect();
        // With the arguments given, each field's type is known and the
        // values are read against it.
        let (arguments, arguments_of_type) = match given {
            Some(given) => (self.bind(name, list, &slots, Takes::Fields), given),
            None if !inferring => (self.bind(name, list, &slots, Takes::Fields), Vec::new()),
            // GEN-2: "infers them from constructor fields".
            None => {
                let places = Places {
                    slots: &slots,
                    patterns: read.members.iter().map(|member| &member.ty).collect(),
                    generics: &read.generics,
                    settled,
                    aside: &[],
                    values: &[],
                    written_pack: None,
                    result_fallback: None,
                    callable: None,
                    defer_endpoint_child: false,
                    // GEN-2: "a generic value parameter that nothing
                    // says anything of takes its default".
                    defaults: read
                        .generics
                        .iter()
                        .map(|generic| self.default_constant(declared, generic))
                        .collect(),
                };
                let found = vec![None; read.generics.len()];
                let Inferred {
                    arguments,
                    mut found,
                    defaulted,
                    ..
                } = self.infer_arguments(name, list, &places, found, Takes::Fields);
                // GEN-13: a default the construction takes is of its
                // parameter's type as the construction resolves it: the
                // `2` of `struct Hold<T, const fallback: T = 2>` is no
                // `str`.
                self.generic_values(
                    name,
                    declared,
                    &read.generics,
                    (&[], &defaulted),
                    &mut found,
                );
                let found: Vec<Argument> = found
                    .into_iter()
                    .map(|found| found.unwrap_or(Argument::Type(Type::Unknown)))
                    .collect();
                (arguments, found)
            }
        };
        let ty = if reported {
            Type::Unknown
        } else {
            structure(arguments_of_type)
        };
        // GEN-3: the struct's `requires` holds with its arguments. Not
        // where something else was reported of the construction
        // (ERR-9).
        if let Type::Named {
            arguments: applied, ..
        } = &ty
            && self.diagnostics.len() == before
        {
            let at = Location::new(self.file, Span::new(name.span().start, list.span().end));
            self.requires_met(at, (module, declaration), applied);
        }
        // TST-3: a trace "is stored in no field", bare or inside a
        // value given to one.
        let stored: Vec<Location> = arguments
            .iter()
            .filter(|argument| holds_trace(&argument.value.ty))
            .map(|argument| argument.value.at)
            .collect();
        for at in stored {
            self.trace_escape(at, Takes::Fields);
        }
        let kind = ExprKind::Construct {
            module,
            declaration,
            arguments,
            infer_types: inferring,
        };
        (kind, ty)
    }

    /// GEN-2: finish inference for a constructor whose supplied fields
    /// included an unsettled value. As with source inference, the first
    /// eligible field in written order decides each argument.
    pub(super) fn infer_settled_fields(
        &self,
        mut ty: Type,
        fields: &[hir::Argument],
    ) -> Option<Type> {
        let Type::Named {
            module,
            declaration,
            arguments,
            kind: NamedKind::Struct,
            ..
        } = &mut ty
        else {
            return None;
        };
        let Detail::Struct(structure) = &self.program.modules[*module].details[*declaration] else {
            return None;
        };
        let generics = &structure.generics;
        let mut found: Vec<_> = generics
            .iter()
            .zip(arguments.iter())
            .map(|(generic, argument)| match argument {
                Argument::Type(ty) if generic.kind == GenericKind::Type && !whole(ty) => None,
                _ => Some(argument.clone()),
            })
            .collect();
        if found.len() != generics.len() {
            return None;
        }
        for field in fields {
            let Some(pattern) = field
                .parameter
                .and_then(|slot| structure.members.get(slot))
                .map(|field| &field.ty)
            else {
                continue;
            };
            // A written `null` never decides a parameter (EXPR-9).
            if matches!(field.value.kind, ExprKind::Literal(Value::Null)) {
                continue;
            }
            let unfound: Vec<_> = generics
                .iter()
                .zip(&found)
                .filter(|(_, found)| found.is_none())
                .map(|(generic, _)| generic.at)
                .collect();
            if !mentions(pattern, &unfound) {
                continue;
            }
            // A later field must wait if this one can still decide
            // the slot. Other fields' already known types stay fixed.
            if !whole(&field.value.ty) {
                return None;
            }
            let actual = self.inference_input(pattern, &field.value.ty, generics, &found, None);
            infer(pattern, actual, generics, &mut found);
        }
        for (argument, inferred) in arguments.iter_mut().zip(found) {
            if let Some(inferred) = inferred {
                *argument = inferred;
            }
        }
        Some(ty)
    }

    /// Gives each argument of `list` to one of `slots`: the parameters
    /// of a function, or the fields of a struct, as `takes` says.
    ///
    /// EXPR-2: "Positional arguments precede named ones; reject
    /// duplicates, unknown names, missing required arguments, and wrong
    /// types. Named arguments use the selected public callable's
    /// parameter names." TYPE-5: a constructor's "arguments are named,
    /// each for a field the struct has, a parent's included, in any
    /// order; it takes no positional argument."
    ///
    /// The arguments come back in written order. Captured arguments
    /// keep both their pack's slot and their key within it (FN-8).
    /// Their values are not read here.
    pub(super) fn assign(
        &mut self,
        callee: &SyntaxToken,
        list: &SyntaxNode,
        slots: &[Slot],
        takes: Takes,
        aside: &[&str],
    ) -> Vec<Assigned> {
        self.assign_dependent(callee, list, slots, takes, aside, None)
    }

    /// A dependent named pack takes the remaining positional arguments
    /// as well as names. Their callback slots are settled separately (FN-8).
    pub(super) fn assign_dependent(
        &mut self,
        callee: &SyntaxToken,
        list: &SyntaxNode,
        slots: &[Slot],
        takes: Takes,
        aside: &[&str],
        dependent: Option<usize>,
    ) -> Vec<Assigned> {
        let what = takes.noun();
        let callee_at = Location::new(self.file, callee.span());
        let pack = |kind| {
            let mut matches = slots
                .iter()
                .enumerate()
                .filter(|(_, slot)| slot.pack == Some(kind));
            let first = matches.next().map(|(index, _)| index);
            // #336: once the pinned catalogue has pack.duplicate, a
            // second pack of the same kind will be rejected at its declaration.
            (first.is_some(), first.filter(|_| matches.next().is_none()))
        };
        let (has_positional_pack, positional_pack) = pack(Pack::Positional);
        let (has_named_pack, named_pack) = pack(Pack::Named);
        let mut pack_locations = vec![Vec::new(); slots.len()];
        let mut pack_names = HashSet::new();
        // Which slots have been given a value.
        let mut filled = vec![false; slots.len()];
        let mut named_seen = false;
        // Whether a constructor was given an argument with no name. Which
        // field it was meant for is not known, so no field is then
        // reported as missing (ERR-9).
        let mut unnamed_field = false;
        let mut arguments = Vec::new();
        let entries = self.call_entries(list);
        // EXPR-2 and EXPR-10: braces can name an ordinary slot, but the
        // parentheses' positional arguments still start at the first
        // ordinary parameter. Keep the value reads in written order.
        let reserved_through = if entries.first().is_some_and(|(brace, ..)| *brace) {
            let positional = entries.iter().filter(|(_, name, _)| name.is_none()).count();
            slots
                .iter()
                .enumerate()
                .filter(|(_, slot)| slot.takes_position())
                .take(positional)
                .last()
                .map(|(index, _)| index)
        } else {
            None
        };
        // FN-19: written arguments bind first. Forwarded names then
        // bind ahead of forwarded positions, which take the slots left.
        let computed = self.deferred_computed_spread(list);
        let spreads = if computed {
            Vec::new()
        } else {
            self.forwarded_arguments(list)
        };
        let written = entries
            .into_iter()
            .map(|(brace, name, value)| (brace, name, value, None));
        let expanded = spreads.into_iter().map(|entry| {
            (
                false,
                None,
                entry.source,
                Some((entry.key, entry.value, entry.input)),
            )
        });
        for (brace, name, value, forwarded) in written.chain(expanded) {
            let at = self.at(&value);
            let name_text = name.as_ref().map(SyntaxToken::text).or(match &forwarded {
                Some((hir::PackKey::Name(name), ..)) => Some(name.as_str()),
                _ => None,
            });
            let slot = match name_text {
                // TYPE-5: "it takes no positional argument."
                None if takes == Takes::Fields => {
                    unnamed_field = true;
                    self.diagnostics.emit(error(
                        Category::Type,
                        codes::ARGUMENT_UNEXPECTED,
                        at,
                        format!(
                            "a struct's fields are given by name: `{}(field: value)`",
                            callee.text()
                        ),
                        "TYPE-5",
                    ));
                    None
                }
                None => {
                    if named_seen && forwarded.is_none() {
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
                    // The first positional slot not yet filled, then
                    // the pack captures what follows (FN-8).
                    let next = slots.iter().enumerate().find_map(|(index, slot)| {
                        (!filled[index] && slot.takes_position()).then_some(index)
                    });
                    if next.is_none() && !has_positional_pack && dependent.is_none() {
                        let fixed = slots.iter().filter(|slot| slot.takes_position()).count();
                        self.diagnostics.emit(error(
                            Category::Type,
                            codes::ARGUMENT_UNEXPECTED,
                            at,
                            format!(
                                "`{}` has {fixed} {what}{}, and this is one more",
                                callee.text(),
                                if fixed == 1 { "" } else { "s" }
                            ),
                            "EXPR-2",
                        ));
                    }
                    next.or(positional_pack).or(dependent)
                }
                Some(text) => {
                    named_seen |= !brace && forwarded.is_none();
                    let name_at = name
                        .as_ref()
                        .map_or(at, |name| Location::new(self.file, name.span()));
                    let found = slots
                        .iter()
                        .position(|slot| slot.pack.is_none() && slot.name == text);
                    let configuration = found.is_some_and(|index| slots[index].configuration);
                    // EXPR-10: configuration names bind first, then
                    // generic value parameters, then ordinary ones.
                    // FN-18: the parentheses of a declaration with
                    // a dependent pack supply that callback's arguments.
                    if !configuration && (brace || dependent.is_none()) && aside.contains(&text) {
                        if forwarded.is_some() {
                            self.report_forwarded_generic_value(callee, text, name_at);
                        }
                        continue;
                    }
                    let found = found.filter(|_| {
                        (!configuration || brace) && !(brace && takes == Takes::Fields)
                    });
                    match found {
                        Some(index)
                            if filled[index]
                                || brace
                                    && slots[index].takes_position()
                                    && reserved_through.is_some_and(|last| index <= last) =>
                        {
                            self.diagnostics.emit(error(
                                Category::Name,
                                codes::ARGUMENT_DUPLICATE,
                                name_at,
                                format!("`{text}` is given twice"),
                                "EXPR-2",
                            ));
                            None
                        }
                        Some(index) => Some(index),
                        // A named pack takes the names the others
                        // leave (FN-8).
                        None if has_named_pack
                            && (!configuration || !brace && dependent.is_some())
                            && (!brace || dependent.is_none()) =>
                        {
                            if named_pack.is_some() && !pack_names.insert(text.to_string()) {
                                self.diagnostics.emit(error(
                                    Category::Name,
                                    codes::ARGUMENT_DUPLICATE,
                                    name_at,
                                    format!("`{text}` is given twice"),
                                    "EXPR-2",
                                ));
                                None
                            } else {
                                named_pack
                            }
                        }
                        None => {
                            self.diagnostics.emit(error(
                                Category::Name,
                                codes::ARGUMENT_UNKNOWN,
                                name_at,
                                format!("`{}` has no {what} `{text}`", callee.text()),
                                if configuration { "FN-18" } else { "EXPR-2" },
                            ));
                            None
                        }
                    }
                }
            };
            let pack = slot.and_then(|index| {
                filled[index] = true;
                slots[index].pack.map(|kind| {
                    let site = name.as_ref().map_or(at, |name| {
                        Location::new(self.file, Span::new(name.span().start, value.span().end))
                    });
                    capture_key(&mut pack_locations[index], kind, name_text, site)
                })
            });
            let forwarded = forwarded.map(|(key, entry, input)| {
                let borrowed =
                    slot.is_some_and(|index| matches!(slots[index].ty, Type::Endpoint(_)));
                let entry = if borrowed {
                    self.borrow_forwarded_entry(entry, input)
                } else {
                    if let Some(input) = input {
                        // FN-19: an ordinary slot reads its matched payload.
                        self.pack_input_read(
                            input,
                            Some(key.clone()),
                            &value,
                            super::children::read_whole_all_valid(&entry.ty),
                        );
                        self.signal_read(input, entry.at, Some((&entry.ty, &value)));
                    }
                    entry
                };
                (key, entry)
            });
            arguments.push(Assigned {
                slot,
                pack,
                name,
                value,
                forwarded,
            });
        }
        for (index, slot) in slots.iter().enumerate() {
            if slot.required
                && (!computed || slot.configuration)
                && slot.pack.is_none()
                && !filled[index]
                && !unnamed_field
            {
                let missing = if slot.name.is_empty() {
                    format!(
                        "`{}` needs an argument at position {}",
                        callee.text(),
                        index + 1
                    )
                } else {
                    format!(
                        "`{}` is given no `{}`, a {what} with no default",
                        callee.text(),
                        slot.name
                    )
                };
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::ARGUMENT_MISSING,
                    callee_at,
                    missing,
                    if slot.configuration {
                        "FN-18"
                    } else {
                        "EXPR-2"
                    },
                ));
            }
        }
        for index in [positional_pack, named_pack].into_iter().flatten() {
            let (minimum, maximum) = slots[index].pack_bounds;
            self.check_pack_count(
                list,
                slots[index].name,
                (if computed { 0 } else { minimum }, maximum),
                &pack_locations[index],
            );
        }
        arguments
    }

    /// GEN-8 and FN-19: an ordinary pack entry is a runtime value,
    /// including when its original argument was a literal.
    pub(super) fn report_forwarded_generic_value(
        &mut self,
        callee: &SyntaxToken,
        name: &str,
        at: Location,
    ) {
        self.diagnostics.emit(error(
            Category::Type,
            codes::GENERIC_ARGUMENT_CONSTANT,
            at,
            format!(
                "a forwarded pack entry is a runtime value, but the generic value parameter `{name}` of `{}` needs a compile-time constant",
                callee.text()
            ),
            "GEN-8",
        ));
    }

    /// GEN-10 and the owner's decision on #336: blame the first excess
    /// argument, or the closing parenthesis if the pack has too few.
    pub(super) fn check_pack_count(
        &mut self,
        list: &SyntaxNode,
        name: &str,
        (minimum, maximum): (u64, Option<u64>),
        given: &[Location],
    ) {
        let count = given.len() as u64;
        let at = if let Some(maximum) = maximum
            && count > maximum
        {
            given[maximum as usize]
        } else if count < minimum {
            tokens(list)
                .find(|token| token.kind() == SyntaxKind::RParen)
                .map_or_else(
                    || self.at(list),
                    |token| Location::new(self.file, token.span()),
                )
        } else {
            return;
        };
        self.check_pack_count_at(at, name, (minimum, maximum), count);
    }

    /// Check a pack whose argument count became known in an instance (GEN-10).
    pub(super) fn check_pack_count_at(
        &mut self,
        at: Location,
        name: &str,
        (minimum, maximum): (u64, Option<u64>),
        count: u64,
    ) {
        let (bound, direction) = if let Some(maximum) = maximum
            && count > maximum
        {
            (maximum, "at most")
        } else if count < minimum {
            (minimum, "at least")
        } else {
            return;
        };
        let direction = if maximum == Some(minimum) {
            "exactly"
        } else {
            direction
        };
        let arguments = if bound == 1 { "argument" } else { "arguments" };
        self.diagnostics.emit(error(
            Category::Type,
            codes::PACK_CARDINALITY,
            at,
            format!("the pack `{name}` takes {direction} {bound} {arguments} and is given {count}"),
            "GEN-10",
        ));
    }

    /// Gives each argument of `list` to one of `slots` and reads its
    /// value, which is of the slot's type.
    pub(super) fn bind(
        &mut self,
        callee: &SyntaxToken,
        list: &SyntaxNode,
        slots: &[Slot],
        takes: Takes,
    ) -> Vec<hir::Argument> {
        self.assign(callee, list, slots, takes, &[])
            .into_iter()
            .map(
                |Assigned {
                     slot,
                     pack,
                     value,
                     forwarded,
                     ..
                 }| {
                    // No generic parameter here: a slot that is a trace is
                    // one an eval gives (TST-5).
                    let expected = slot.map(|index| {
                        let slot = &slots[index];
                        (slot, matches!(slot.ty, Type::Trace { .. }))
                    });
                    if let Some((_, read)) = forwarded {
                        let value = match expected {
                            Some((slot, traced)) => self.fit_argument(read, slot, traced, takes),
                            None => read,
                        };
                        hir::Argument {
                            parameter: slot,
                            pack: pack.map(|(_, key)| key),
                            value,
                        }
                    } else {
                        let mut argument = self.argument(slot, &value, expected, takes);
                        argument.pack = pack.map(|(_, key)| key);
                        argument
                    }
                },
            )
            .collect()
    }

    /// The argument written at `node`, given to `slot`. One given to
    /// nothing is read for what it reports itself. With
    /// the slot is whether it is a trace an eval gives (TST-5).
    ///
    /// A value that holds a trace, given to a place that takes none,
    /// is not held to the place's type: where it may go is for the
    /// call or the construction to report once it is built (TST-3).
    pub(super) fn argument(
        &mut self,
        parameter: Option<usize>,
        node: &SyntaxNode,
        slot: Option<(&Slot, bool)>,
        takes: Takes,
    ) -> hir::Argument {
        let value = match slot {
            Some((
                Slot {
                    ty: Type::Function(template),
                    ..
                },
                _,
            )) => self.function_argument(node, template),
            Some((slot, traced)) => {
                let value = self.expression(node, Some(&slot.ty));
                self.fit_argument(value, slot, traced, takes)
            }
            None => self.expression(node, Some(&Type::Unknown)),
        };
        hir::Argument {
            parameter,
            pack: None,
            value,
        }
    }

    /// Fits an already-read argument, including a forwarded pack entry.
    pub(super) fn fit_argument(
        &mut self,
        value: Expr,
        slot: &Slot<'_>,
        traced: bool,
        takes: Takes,
    ) -> Expr {
        let value = if slot.listed {
            publications(value)
        } else {
            value
        };
        self.fit_prepared_argument(value, slot, traced, takes)
    }

    /// Fits an argument whose publication read has already been chosen.
    pub(super) fn fit_prepared_argument(
        &mut self,
        value: Expr,
        slot: &Slot<'_>,
        traced: bool,
        takes: Takes,
    ) -> Expr {
        // FN-12: a window's connection given to a value
        // function's `list<V>` in composition (`lifted_window`).
        let lifted = self.lifted_window(&slot.ty, slot.listed, &value, takes);
        // TYPE-10: an ordinary list argument keeps its actual size;
        // an unsized value parameter takes its entries in order.
        let fixed_list_value = takes == Takes::Parameters
            && slot.listed
            && !self.composes(&value)
            && matches!(
                (&slot.ty, value.ty.omission_removed()),
                (Type::List { element, size: None }, Type::List { element: actual, size: Some(size) })
                    if !matches!(size.value, ConstantValue::Unbounded) && agree(element, actual)
            );
        if lifted || fixed_list_value || (!traced && holds_trace(&value.ty)) {
            value
        } else {
            let place = format!("the {} `{}`", takes.noun(), slot.name);
            if takes == Takes::Parameters {
                self.fit_input(value, &slot.ty, &place)
            } else {
                self.fit(value, &slot.ty, &place)
            }
        }
    }

    /// Reports a trace given to a field, or to a parameter of a
    /// function that is no `const fn` (TST-3).
    pub(super) fn trace_escape(&mut self, at: Location, takes: Takes) {
        let how = match takes {
            Takes::Fields => "it is stored in no field",
            Takes::Parameters => "it is a temporal argument only of an eval",
        };
        self.diagnostics.emit(
            error(
                Category::Type,
                codes::TRACE_ESCAPE,
                at,
                format!("this is a trace, a value of the test: {how}"),
                "TST-3",
            )
            .with_note("a trace is bound with `let`, compared, and given to a `const fn`"),
        );
    }
}

/// Retain a capture's destination key and its place in that pack.
fn capture_key(
    locations: &mut Vec<Location>,
    kind: Pack,
    name: Option<&str>,
    at: Location,
) -> (usize, hir::PackKey) {
    let position = locations.len();
    locations.push(at);
    let key = match (kind, name) {
        (Pack::Named, Some(name)) => hir::PackKey::Name(name.to_string()),
        _ => hir::PackKey::Position(position),
    };
    (position, key)
}

/// An argument tied to a place before its value is read.
pub(super) struct Assigned {
    /// The index of its parameter or field.
    pub(super) slot: Option<usize>,
    /// The position and key within the capture when the slot is a pack.
    pub(super) pack: Option<(usize, hir::PackKey)>,
    /// Its source name, where it has one.
    pub(super) name: Option<SyntaxToken>,
    /// The value written by the call.
    pub(super) value: SyntaxNode,
    /// The source key and typed read for an entry supplied by a spread.
    pub(super) forwarded: Option<(hir::PackKey, Expr)>,
}

impl Assigned {
    /// The supplied name, whether written in this call or forwarded.
    pub(super) fn name_text(&self) -> Option<&str> {
        self.name
            .as_ref()
            .map(SyntaxToken::text)
            .or_else(|| match &self.forwarded {
                Some((hir::PackKey::Name(name), _)) => Some(name),
                _ => None,
            })
    }
}

/// What an argument may be given to: a parameter of a function, or a
/// field of a struct.
pub(super) struct Slot<'a> {
    /// Its name.
    pub(super) name: &'a str,
    /// True for a parameter that only a call's braces can name (FN-18).
    pub(super) configuration: bool,
    /// Its type; not known when it names a generic parameter that
    /// nothing has given an argument.
    pub(super) ty: Type,
    /// True when a call that gives it nothing is rejected.
    pub(super) required: bool,
    /// The pack it is, when it is one (FN-8).
    pub(super) pack: Option<Pack>,
    /// How many arguments the pack takes (GEN-10).
    pub(super) pack_bounds: (u64, Option<u64>),
    /// The generic parameter of a heterogeneous pack, when it has one.
    pub(super) type_pack: Option<usize>,
    /// True for a parameter of a `const fn`: a trace given to it is the
    /// list of its publications there (TST-16).
    pub(super) listed: bool,
}

impl Slot<'_> {
    fn takes_position(&self) -> bool {
        !self.configuration && self.pack.is_none()
    }
}

/// What takes the arguments of a call.
///
/// `#[derive(Clone, Copy, PartialEq)]` asks the compiler to write the
/// code that copies one of these and compares two of them.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Takes {
    /// The parameters of a function: positional arguments, then named
    /// ones (EXPR-2).
    Parameters,
    /// The fields of a struct: named arguments only (TYPE-5).
    Fields,
}

impl Takes {
    /// The word a message uses for one of them.
    pub(super) fn noun(self) -> &'static str {
        match self {
            Takes::Parameters => "parameter",
            Takes::Fields => "field",
        }
    }
}

/// True when `given` are arguments the generic parameters `generics`
/// can be given: as many, where there is no type pack, and each a type
/// or a value as its parameter is (GEN-2, GEN-14).
fn applies(generics: &[GenericParameter], given: &[Argument]) -> bool {
    let packed = generics
        .iter()
        .any(|generic| matches!(generic.kind, GenericKind::Pack { .. }));
    (packed || generics.len() == given.len())
        && generics
            .iter()
            .zip(given)
            .all(|(generic, given)| match (&generic.kind, given) {
                (GenericKind::Value(_), Argument::Type(ty)) => matches!(ty, Type::Unknown),
                (GenericKind::Type | GenericKind::Plain, Argument::Constant(_)) => false,
                _ => true,
            })
}
