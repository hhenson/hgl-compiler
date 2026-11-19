//! A call's arguments, each given to a parameter of a function or to a
//! field of a struct, and the construction of a struct (EXPR-2, TYPE-3,
//! TYPE-5). Part of the typing pass ([`super`]).

use hgl_diagnostics::{Category, Location};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};
use hgl_source::Span;

use super::evals::{holds_trace, publications};
use super::{Inferred, Places, Typer, fixed, placed};
use crate::declared::{is_optional, substitute_within};
use crate::hir::{self, EndpointOperation, ExprKind};
use crate::program::DeclarationKind;
use crate::syntax::{child, has_token, name_token};
use crate::types::{Argument, Detail, GenericKind, GenericParameter, NamedKind, Pack, Type};
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
        let written = self.written.get(&(self.file, start)).copied();
        match written {
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
                let generics = self.calls.get(&(self.file, start)).copied();
                // TIME-9: an endpoint operation written with its generic
                // argument, `delta_value<i64>(out)`, is the form it is
                // without one, and is held to the same rules.
                if self.hook.is_some()
                    && child(node, SyntaxKind::QualifiedName).is_none()
                    && let Some(operation) = EndpointOperation::language_form(name.text())
                    && self.is_the_languages(&name)
                {
                    let read = self.endpoint(operation, &name, &list);
                    self.endpoint_generics(&name, generics, &read.0);
                    return read;
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
                let ty = substitute_within(&member.ty, &map);
                Slot {
                    name: &member.name,
                    // TYPE-4: "A field is optional when its type is
                    // nullable ... or an `atomic<...>` of a nullable
                    // type", the generic arguments in place: the
                    // `value: T` of a `Box<i64?>` is.
                    required: !member.has_default && is_optional(&ty) != Some(true),
                    ty: placed(ty, &own),
                    pack: None,
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
            Type::Named {
                name: declared.name.clone(),
                module,
                declaration,
                arguments: arguments_of_type,
                kind: NamedKind::Struct,
            }
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
        };
        (kind, ty)
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
    /// The arguments come back in the order written, each with the slot
    /// it is given to: `None` for one given to a pack, or to nothing.
    /// Their values are not read here.
    pub(super) fn assign(
        &mut self,
        callee: &SyntaxToken,
        list: &SyntaxNode,
        slots: &[Slot],
        takes: Takes,
        aside: &[&str],
    ) -> Vec<(Option<usize>, SyntaxNode)> {
        let what = takes.noun();
        let callee_at = Location::new(self.file, callee.span());
        let has_pack = |pack: Pack| slots.iter().any(|slot| slot.pack == Some(pack));
        // Which slots have been given a value.
        let mut filled = vec![false; slots.len()];
        let mut named_seen = false;
        // Whether a constructor was given an argument with no name. Which
        // field it was meant for is not known, so no field is then
        // reported as missing (ERR-9).
        let mut unnamed_field = false;
        let mut arguments = Vec::new();
        for (name, value) in self.call_arguments(list) {
            let at = self.at(&value);
            let slot = match &name {
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
                    // The first slot not yet filled that is no pack; a
                    // positional pack takes what the others leave
                    // (FN-8).
                    let next = (0..slots.len())
                        .find(|&index| !filled[index] && slots[index].pack.is_none());
                    if next.is_none() && !has_pack(Pack::Positional) {
                        let fixed = slots.iter().filter(|slot| slot.pack.is_none()).count();
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
                    next
                }
                Some(name) => {
                    named_seen = true;
                    // GEN-8: "A name binds the callee's generic value
                    // parameter of that name, or else its parameter of
                    // that name." One that does is no place's.
                    if aside.contains(&name.text()) {
                        continue;
                    }
                    let name_at = Location::new(self.file, name.span());
                    let found = slots
                        .iter()
                        .position(|slot| slot.pack.is_none() && slot.name == name.text());
                    match found {
                        Some(index) if filled[index] => {
                            self.diagnostics.emit(error(
                                Category::Name,
                                codes::ARGUMENT_DUPLICATE,
                                name_at,
                                format!("`{}` is given twice", name.text()),
                                "EXPR-2",
                            ));
                            None
                        }
                        Some(index) => Some(index),
                        // A named pack takes the names the others
                        // leave (FN-8).
                        None if has_pack(Pack::Named) => None,
                        None => {
                            self.diagnostics.emit(error(
                                Category::Name,
                                codes::ARGUMENT_UNKNOWN,
                                name_at,
                                format!("`{}` has no {what} `{}`", callee.text(), name.text()),
                                "EXPR-2",
                            ));
                            None
                        }
                    }
                }
            };
            if let Some(index) = slot {
                filled[index] = true;
            }
            arguments.push((slot, value));
        }
        for (index, slot) in slots.iter().enumerate() {
            if slot.required && slot.pack.is_none() && !filled[index] && !unnamed_field {
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::ARGUMENT_MISSING,
                    callee_at,
                    format!(
                        "`{}` is given no `{}`, a {what} with no default",
                        callee.text(),
                        slot.name
                    ),
                    "EXPR-2",
                ));
            }
        }
        arguments
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
            .map(|(slot, value)| {
                // No generic parameter here: a slot that is a trace is
                // one an eval gives (TST-5).
                let expected = slot.map(|index| {
                    let slot = &slots[index];
                    (slot, matches!(slot.ty, Type::Trace { .. }))
                });
                self.argument(slot, &value, expected, takes)
            })
            .collect()
    }

    /// The argument written at `node`, given to `slot`. One given to a
    /// pack, or to nothing, is read for what it reports itself. With
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
            Some((slot, traced)) => {
                let value = self.expression(node, Some(&slot.ty));
                let value = if slot.listed {
                    publications(value)
                } else {
                    value
                };
                if !traced && holds_trace(&value.ty) {
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
            None => self.expression(node, Some(&Type::Unknown)),
        };
        hir::Argument { parameter, value }
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

/// What an argument may be given to: a parameter of a function, or a
/// field of a struct.
pub(super) struct Slot<'a> {
    /// Its name.
    pub(super) name: &'a str,
    /// Its type; not known when it names a generic parameter that
    /// nothing has given an argument.
    pub(super) ty: Type,
    /// True when a call that gives it nothing is rejected.
    pub(super) required: bool,
    /// The pack it is, when it is one (FN-8).
    pub(super) pack: Option<Pack>,
    /// True for a parameter of a `const fn`: a trace given to it is the
    /// list of its publications there (TST-16).
    pub(super) listed: bool,
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
