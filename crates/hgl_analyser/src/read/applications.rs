//! Resolves named types and reads generic arguments and constants.

use hgl_diagnostics::{Category, Location};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};
use hgl_source::FileId;

use crate::constant::{Fault, Scope, Typed};
use crate::program::{Binding, Declaration, DeclarationKind, LocalKind};
use crate::syntax::{child, name_token};
use crate::types::{Argument, Constant, ConstantValue, GenericKind, NamedKind, Type};
use crate::{codes, error};

use super::declarations::{Form, Kind, generic_forms};
use super::{Id, Position, Reader, default_of, is_type, text_of, type_child};

/// `1 argument`, `2 arguments`.
fn arguments(count: u64) -> String {
    match count {
        1 => "1 argument".to_string(),
        count => format!("{count} arguments"),
    }
}

impl Reader<'_> {
    // ---- Named types ----

    /// `Quote`, `pc::Curve`, `Box<i64>`, or a generic parameter.
    pub(super) fn named(&mut self, node: &SyntaxNode, position: Position) -> Type {
        // TYPE-18: `Type` is the language's type of a type, not a
        // declaration supplied by the program or by core.
        if child(node, SyntaxKind::QualifiedName).is_none()
            && name_token(node).is_some_and(|name| name.text() == "Type")
        {
            return Type::TypeValue;
        }
        let list = child(node, SyntaxKind::GenericArgs);
        let name = name_token(node);
        let binding = name
            .as_ref()
            .and_then(|name| self.bindings.get(&(self.file, name.span().start)).copied());
        // The type the name is bound to, when it is one. A type is one
        // declaration; only a pair of functions is two (FN-4).
        let target: Option<Id> = match binding {
            Some(Binding::Declarations {
                module,
                declarations,
            }) => {
                let declared = &self.program.modules[*module].declarations;
                declarations
                    .iter()
                    .copied()
                    .find(|&index| {
                        matches!(
                            declared[index].kind,
                            DeclarationKind::Struct
                                | DeclarationKind::Enum
                                | DeclarationKind::NativeType
                        )
                    })
                    .map(|index| (*module, index))
            }
            _ => None,
        };
        // The arguments are read whatever the name is, so that what is
        // wrong inside them is reported.
        // Where an ordinary value type is required, the arguments of a
        // generic type are held to it too: a `Box<atomic<Quote>>` is no
        // more an ordinary value than a `list<atomic<Quote>>` is.
        let arguments = list.as_ref().map_or(Vec::new(), |list| {
            self.arguments(
                list,
                Position {
                    value: position.value,
                    ..Position::ARGUMENT
                },
                target,
                None,
            )
        });
        // No binding: the name is left for the prelude, or the resolver
        // has reported it.
        let (Some(name), Some(binding)) = (name, binding) else {
            return Type::Unknown;
        };
        match binding {
            Binding::Local {
                at,
                kind: LocalKind::GenericParameter,
            } => {
                // `...{Fields<i64>}` names no type pack, and has been
                // reported as that (FN-8).
                let in_named_pack = node
                    .parent()
                    .is_some_and(|parent| parent.kind() == SyntaxKind::PackType);
                self.generic(&name, *at, list.is_some() && !in_named_pack, position)
            }
            Binding::Local { at, kind } => {
                // A parameter among a constraint's operands that is no
                // const option (see `names_an_option`): the report says
                // what a constraint may name.
                let in_constraint = *kind == LocalKind::Parameter
                    && node.parent().is_some_and(|parent| {
                        matches!(
                            parent.kind(),
                            SyntaxKind::ConstraintRelation | SyntaxKind::ConstraintSet
                        )
                    });
                self.not_a_type(&name, kind.describe(), *at, in_constraint);
                Type::Unknown
            }
            Binding::Declarations {
                module,
                declarations,
            } => {
                let declared = &self.program.modules[*module].declarations;
                let Some((_, index)) = target else {
                    let first = &declared[declarations[0]];
                    let what = match first.kind {
                        DeclarationKind::Operator => "an operator".to_string(),
                        kind => format!("a {}", kind.describe()),
                    };
                    self.not_a_type(&name, &what, first.at, false);
                    return Type::Unknown;
                };
                let declaration = &declared[index];
                // GEN-2: "Where an application supplies none, a generic
                // value parameter that nothing says anything of takes
                // its default". A type position infers nothing, so a
                // type named bare is the type with every default, where
                // every parameter has one.
                let arguments = match (&list, self.defaults_of((*module, index))) {
                    (None, Some(defaults)) if !position.bare => defaults,
                    _ => {
                        self.argument_count(&name, declaration, list.as_ref(), position);
                        arguments
                    }
                };
                Type::Named {
                    name: declaration.name.clone(),
                    module: *module,
                    declaration: index,
                    arguments,
                    kind: match declaration.kind {
                        DeclarationKind::Enum => NamedKind::Enum,
                        DeclarationKind::NativeType => NamedKind::Native,
                        _ => NamedKind::Struct,
                    },
                }
            }
        }
    }

    /// Reports a name that stands where a type does and is none.
    /// `in_constraint` is true for a parameter among a constraint's
    /// operands, where a const option may stand and this one is none.
    fn not_a_type(
        &mut self,
        name: &SyntaxToken,
        what: &str,
        declared: Location,
        in_constraint: bool,
    ) {
        let mut report = error(
            Category::Type,
            codes::TYPE_NOT_A_TYPE,
            self.at(name.span()),
            format!("`{}` is {what}, not a type", name.text()),
            "UNIT-10",
        )
        .with_related(declared, "declared here");
        if in_constraint {
            // GEN-3: "It cannot inspect temporal values". GEN-6: a
            // candidate compares "a const option" with constants.
            report = report.with_note(
                "a constraint compares a `const` option with constants with `==` or `in`, \
                 and reads no other parameter (GEN-3, GEN-6)",
            );
        }
        self.diagnostics.emit(report);
    }

    /// A generic parameter named as a type (GEN-13): "A generic
    /// parameter denotes a type, type pack, or compile-time ordinary
    /// value."
    fn generic(
        &mut self,
        name: &SyntaxToken,
        declared: Location,
        applied: bool,
        position: Position,
    ) -> Type {
        let parameter = Type::Parameter {
            name: name.text().to_string(),
            at: declared,
        };
        if applied {
            self.diagnostics.emit(
                error(
                    Category::Type,
                    codes::GENERIC_ARGUMENT_COUNT,
                    self.at(name.span()),
                    format!(
                        "`{}` is a generic parameter and takes no generic arguments",
                        name.text()
                    ),
                    "GEN-13",
                )
                .with_related(declared, "declared here"),
            );
            // It stands where a type does, which is what it is taken
            // for: one report for one fault.
            self.note_type(declared, self.at(name.span()));
            return parameter;
        }
        // One bound by `each T in ...` is not among the declaration's
        // own; it is a type.
        let kind = self
            .generics
            .iter()
            .find(|generic| generic.at == declared)
            .map(|generic| Kind::of(&generic.kind));
        match kind {
            // GEN-2: "A type pack is a sequence, not a singular source
            // type."
            Some(Kind::Pack) if !position.pack => {
                self.diagnostics.emit(
                    error(
                        Category::Type,
                        codes::TYPE_NOT_A_TYPE,
                        self.at(name.span()),
                        format!(
                            "`{}` is a type pack: a sequence of types, not one type",
                            name.text()
                        ),
                        "GEN-2",
                    )
                    .with_related(declared, "declared here"),
                );
                Type::Unknown
            }
            Some(Kind::Value) if !position.constant => {
                self.diagnostics.emit(
                    error(
                        Category::Type,
                        codes::TYPE_NOT_A_TYPE,
                        self.at(name.span()),
                        format!("`{}` is a generic value parameter, not a type", name.text()),
                        "GEN-13",
                    )
                    .with_related(declared, "declared here"),
                );
                Type::Unknown
            }
            // GEN-13: a plain parameter named where a type stands is
            // used as a type. Where a value might stand as well, the
            // use says nothing yet.
            Some(Kind::Unknown) => {
                if !position.constant {
                    self.note_type(declared, self.at(name.span()));
                } else if self.unsettled_target {
                    self.note_dependent(declared);
                } else {
                    self.note_open(declared);
                }
                parameter
            }
            _ => parameter,
        }
    }

    /// GEN-2: "A generic struct application supplies all arguments ...
    /// Partial applications and `_` struct arguments are not admitted."
    fn argument_count(
        &mut self,
        name: &SyntaxToken,
        declaration: &Declaration,
        list: Option<&SyntaxNode>,
        position: Position,
    ) {
        // How many the type declares: its fixed parameters, and what a
        // pack among them may add (GEN-10).
        let mut fixed: u64 = 0;
        let mut pack = None;
        let forms = generic_forms(&declaration.syntax);
        for (_, _, form) in &forms {
            match form {
                Form::Pack(minimum, maximum) => pack = Some((*minimum, *maximum)),
                Form::Plain | Form::Value => fixed += 1,
            }
        }
        // A cardinality may be any integer literal, so the sums are
        // taken with care. `saturating_add` stops at the largest `u64`
        // where a sum would pass it; `checked_add` gives `None`, which
        // here is no limit.
        let fewest = fixed.saturating_add(pack.map_or(0, |(minimum, _)| minimum));
        // `None`: no limit.
        let most = match pack {
            None => Some(fixed),
            Some((_, maximum)) => maximum.and_then(|maximum| fixed.checked_add(maximum)),
        };
        let takes = match most {
            Some(most) if most == fewest => arguments(fewest),
            Some(most) => format!("from {fewest} to {most} arguments"),
            None => format!("at least {}", arguments(fewest)),
        };

        let problem = match list {
            // A type that needs none may be named bare.
            None if fewest == 0 || position.bare => return,
            None => format!(
                "`{}` is a generic type and is given no arguments: it takes {takes}",
                declaration.name
            ),
            Some(_) if forms.is_empty() => format!(
                "`{}` is not a generic type and takes no generic arguments",
                declaration.name
            ),
            Some(list) => {
                // How many are given. An argument that names a type pack
                // passes on as many types as the pack holds (GEN-10),
                // which is a range until the pack is resolved.
                let mut given_fewest: u64 = 0;
                let mut given_most: Option<u64> = Some(0);
                for argument in list
                    .children()
                    .filter(|argument| argument.kind() != SyntaxKind::Error)
                {
                    let (minimum, maximum) = self.pack_bounds(&argument).unwrap_or((1, Some(1)));
                    given_fewest = given_fewest.saturating_add(minimum);
                    // `zip` gives `None`, no limit, when either has
                    // none; a sum past the largest `u64` is no limit
                    // either.
                    given_most = given_most
                        .zip(maximum)
                        .and_then(|(most, more)| most.checked_add(more));
                }
                // Rejected only when no length the packs may have fits.
                // Whether the length a pack is resolved to fits is for
                // the pass that resolves it.
                let too_many = most.is_some_and(|most| given_fewest > most);
                let too_few = given_most.is_some_and(|given_most| given_most < fewest);
                if !too_many && !too_few {
                    return;
                }
                let given = match given_most {
                    Some(1) if given_fewest == 1 => "1 is".to_string(),
                    Some(most) if most == given_fewest => format!("{most} are"),
                    Some(most) => format!("from {given_fewest} to {most} are"),
                    // The sum stopped at the largest `u64`.
                    None if given_fewest == u64::MAX => "more than can be counted are".to_string(),
                    None => format!("at least {given_fewest} are"),
                };
                format!("`{}` takes {takes}, and {given} given", declaration.name)
            }
        };
        self.diagnostics.emit(
            error(
                Category::Type,
                codes::GENERIC_ARGUMENT_COUNT,
                self.at(name.span()),
                problem,
                "GEN-2",
            )
            .with_related(declaration.at, "declared here"),
        );
    }

    /// How many types a generic argument passes on when it names a type
    /// pack of the declaration being read: the fewest and the most
    /// (GEN-10). `None` for any other argument, which is one.
    fn pack_bounds(&self, argument: &SyntaxNode) -> Option<(u64, Option<u64>)> {
        if argument.kind() != SyntaxKind::NamedType
            || child(argument, SyntaxKind::GenericArgs).is_some()
        {
            return None;
        }
        let name = name_token(argument)?;
        let &binding = self.bindings.get(&(self.file, name.span().start))?;
        let Binding::Local {
            at,
            kind: LocalKind::GenericParameter,
        } = binding
        else {
            return None;
        };
        self.generics
            .iter()
            .find(|generic| generic.at == *at)
            .and_then(|generic| match generic.kind {
                GenericKind::Pack { minimum, maximum } => Some((minimum, maximum)),
                _ => None,
            })
    }

    /// The arguments of `<...>`: each a type or a constant.
    ///
    /// `target` is the generic type they are given to, when the name
    /// before them is one. Each argument is then read for the parameter
    /// it is given to (GEN-14): "Check each argument against the
    /// corresponding parameter kind."
    ///
    /// `values_of` is the declaration whose generic value parameters the
    /// constants among them are read for, where that is not `target`: a
    /// function or an operator the arguments are written after, which is
    /// no type (GEN-8).
    pub(super) fn arguments(
        &mut self,
        list: &SyntaxNode,
        position: Position,
        target: Option<Id>,
        values_of: Option<Id>,
    ) -> Vec<Argument> {
        // The name and kind of each generic parameter of the target.
        let (target_name, parameters): (String, Vec<(String, Kind)>) = match target {
            Some((module, index)) => {
                let declaration = &self.program.modules[module].declarations[index];
                let kinds = self.kinds.get(&(module, index));
                let parameters = generic_forms(&declaration.syntax)
                    .iter()
                    .enumerate()
                    .map(|(position, (_, name, _))| {
                        let kind = kinds
                            .and_then(|kinds| kinds.get(position))
                            .copied()
                            .unwrap_or(Kind::Unknown);
                        (name.text().to_string(), kind)
                    })
                    .collect();
                (declaration.name.clone(), parameters)
            }
            None => (String::new(), Vec::new()),
        };

        // The type of each value parameter of what the arguments are
        // given to, and where each of its parameters is declared, with
        // its name: a value parameter's type may be a type parameter's,
        // given by an argument before it.
        let (types, places, values_name) = match target.or(values_of) {
            Some(of) => (
                self.parameter_types(of),
                self.parameter_places(of),
                self.program.modules[of.0].declarations[of.1].name.clone(),
            ),
            None => (Vec::new(), Vec::new(), String::new()),
        };
        let mut arguments = Vec::new();
        // The parameter the next argument is given to. A pack takes
        // every argument from its place on. An argument that passes a
        // pack on stands for as many as the pack holds: when that is
        // one number the place of those after it is still known, and
        // otherwise it is not.
        let mut next: usize = 0;
        let mut placed = true;
        // The place of the target's pack parameter, when it has one.
        let pack_at = parameters.iter().position(|(_, kind)| *kind == Kind::Pack);
        for argument in list.children() {
            let kind = argument.kind();
            if kind == SyntaxKind::Error {
                continue;
            }
            // How many arguments this one stands for: one, or for the
            // name of a type pack as many as the pack holds, which is
            // at least a number and may be exactly one.
            let (at_least, exactly) = match self.pack_bounds(&argument) {
                None => (1, Some(1)),
                Some((minimum, maximum)) => {
                    (minimum, maximum.filter(|maximum| *maximum == minimum))
                }
            };
            // A pack that may hold nothing is given to no parameter for
            // certain.
            let parameter = if placed && at_least > 0 {
                parameters.get(next)
            } else {
                None
            };
            let want = parameter.map_or(Kind::Unknown, |(_, kind)| *kind);
            // What a value given to this parameter is a value of
            // (EXPR-5), with what an earlier argument gave a type
            // parameter in place: the `T` of `S<T, const v: T>`.
            let at = (placed && at_least > 0).then_some(next);
            let expected =
                at.and_then(|at| types.get(at).cloned().flatten())
                    .and_then(|ty| match ty {
                        Type::Parameter { at, .. } => places
                            .iter()
                            .position(|place| *place == at)
                            .and_then(|index| match arguments.get(index) {
                                Some(Argument::Type(given)) => Some(given.clone()),
                                _ => None,
                            }),
                        ty => Some(ty),
                    });
            let place = at
                .and_then(|at| places.get(at))
                .and_then(|declared| {
                    let of = target.or(values_of)?;
                    let declaration = &self.program.modules[of.0].declarations[of.1];
                    generic_forms(&declaration.syntax)
                        .into_iter()
                        .find(|(_, name, _)| name.span() == declared.span)
                        .map(|(_, name, _)| name.text().to_string())
                })
                .map(|name| format!("`{name}` of `{values_name}`"));
            // A pack of several types fills as many places as it holds
            // at least. Each after the first is a type given to that
            // parameter too, and a value parameter among them takes
            // none (GEN-14).
            let covered = usize::try_from(at_least).unwrap_or(usize::MAX);
            let value_slot = parameters
                .iter()
                .skip(next.saturating_add(1))
                .take(covered.saturating_sub(1))
                .take_while(|(_, kind)| *kind != Kind::Pack)
                .find(|(_, kind)| *kind == Kind::Value)
                .filter(|_| placed);
            if let Some((name, _)) = value_slot {
                let message = format!(
                    "`{target_name}` takes a value for `{name}`, and `{}` passes on types",
                    text_of(&argument)
                );
                self.argument_kind(&argument, Some(message));
            }
            match exactly {
                // A pack parameter takes them all and stays the place.
                Some(_) if want == Kind::Pack => {}
                // A count past what a `usize` holds is past every
                // parameter.
                Some(count) => {
                    next = next.saturating_add(usize::try_from(count).unwrap_or(usize::MAX));
                    // What runs past the fixed parameters is given to
                    // the pack after them, when there is one.
                    if let Some(pack) = pack_at {
                        next = next.min(pack);
                    }
                }
                // How many it holds is a range: the place of the
                // arguments after it is not known.
                None => placed = false,
            }
            // What to say when the argument is of the other kind.
            let mismatch = |wanted: &str, given: &str| {
                parameter.map(|(name, _)| {
                    format!(
                        "`{target_name}` takes {wanted} for `{name}`, and `{}` is {given}",
                        text_of(&argument)
                    )
                })
            };

            if kind == SyntaxKind::Hole {
                // GEN-8: `_` in an `instantiate` keeps a parameter open.
                arguments.push(Argument::Type(Type::Unknown));
            } else if !is_type(kind) && !self.indexed_pack(&argument) {
                // An expression is a value.
                if matches!(want, Kind::Type | Kind::Pack) {
                    self.argument_kind(&argument, mismatch("a type", "a value"));
                }
                arguments.push(Argument::Constant(self.constant(
                    &argument,
                    expected.as_ref(),
                    place.as_deref(),
                )));
            } else if let Some(constant) =
                self.value_name(&argument, want, expected.as_ref(), place.as_deref())
            {
                // The parser reads a lone name as a type. One that names
                // a value is a constant.
                arguments.push(Argument::Constant(constant));
            } else {
                let position = match want {
                    // Where a type goes, a type pack may be passed on.
                    // What the place around the type asks of an
                    // ordinary value is asked of its arguments too.
                    Kind::Type | Kind::Pack => Position {
                        value: position.value,
                        ..Position::PACK
                    },
                    Kind::Value | Kind::Unknown => position,
                };
                // A lone name given to a parameter of the target whose
                // kind nothing has settled says nothing of what the
                // name is (GEN-13).
                let unsettled = parameter.is_some() && want == Kind::Unknown;
                let before = std::mem::replace(&mut self.unsettled_target, unsettled);
                let ty = self.type_(&argument, position);
                self.unsettled_target = before;
                // A name that is not known may yet be a constant; one
                // known to be a type is not: a declared type, or a
                // generic parameter settled as a type.
                let is_type = kind != SyntaxKind::NamedType
                    || matches!(ty, Type::Named { .. })
                    || self.is_type_parameter(&ty);
                if want == Kind::Value && is_type {
                    self.argument_kind(&argument, mismatch("a value", "a type"));
                }
                arguments.push(Argument::Type(ty));
            }
        }
        arguments
    }

    /// True when `ty` is a generic parameter known to stand for types:
    /// one of the declaration being read that is settled as a type, a
    /// type pack, or a name bound by `each T in ...`.
    fn is_type_parameter(&self, ty: &Type) -> bool {
        let Type::Parameter { at, .. } = ty else {
            return false;
        };
        match self.generics.iter().find(|generic| generic.at == *at) {
            Some(generic) => {
                matches!(generic.kind, GenericKind::Type | GenericKind::Pack { .. })
            }
            // One bound by `each T in ...` is not among the
            // declaration's own; it is a type (GEN-10).
            None => true,
        }
    }

    fn argument_kind(&mut self, argument: &SyntaxNode, message: Option<String>) {
        if let Some(message) = message {
            self.diagnostics.emit(error(
                Category::Type,
                codes::GENERIC_ARGUMENT_KIND,
                self.at(argument.span()),
                message,
                "GEN-14",
            ));
        }
    }

    /// The constant a lone name is, when it names a value: a generic
    /// parameter declared `const N: V`; a plain one given where a value
    /// goes, which that use makes a value (GEN-13); or a local, a
    /// parameter or another binding of a body. Whether such a binding
    /// may stand in a constant is for the pass that evaluates
    /// constants.
    ///
    /// `want` is the kind of the parameter the name is given to. Where
    /// a type goes, a name is read as a type.
    fn value_name(
        &mut self,
        node: &SyntaxNode,
        want: Kind,
        expected: Option<&Type>,
        place: Option<&str>,
    ) -> Option<Constant> {
        if matches!(want, Kind::Type | Kind::Pack)
            || node.kind() != SyntaxKind::NamedType
            || child(node, SyntaxKind::GenericArgs).is_some()
        {
            return None;
        }
        let name = name_token(node)?;
        let &binding = self.bindings.get(&(self.file, name.span().start))?;
        match binding {
            Binding::Local {
                at,
                kind: LocalKind::GenericParameter,
            } => {
                // `?` on an `Option` leaves the function with `None`
                // when there is nothing: here, for a parameter bound by
                // `each ... in`, which is a type.
                let kind = self
                    .generics
                    .iter()
                    .find(|generic| generic.at == *at)
                    .map(|generic| Kind::of(&generic.kind))?;
                match kind {
                    Kind::Value => {}
                    Kind::Unknown if want == Kind::Value => {
                        self.note_value(*at, self.at(name.span()));
                    }
                    _ => return None,
                }
            }
            Binding::Local { .. } => {}
            // UNIT-13: a constant of a module is "a compile-time
            // constant where a generic value argument (GEN-8) ...
            // requires one".
            Binding::Declarations {
                module,
                declarations,
            } => match declarations.as_slice() {
                &[declaration]
                    if self.program.modules[*module].declarations[declaration].kind
                        == DeclarationKind::Constant => {}
                _ => return None,
            },
        }
        let read = self.typed_constant(node, expected);
        let value = self.held(node, read, expected, place).value;
        Some(Constant {
            text: name.text().to_string(),
            value,
        })
    }

    /// The type of the generic value parameter declared at `declared`;
    /// `None` when what is declared there is not one.
    pub(super) fn value_type(&self, declared: Location) -> Option<&Type> {
        self.generics
            .iter()
            .find(|generic| generic.at == declared)
            .and_then(|generic| match &generic.kind {
                GenericKind::Value(ty) => Some(ty),
                _ => None,
            })
    }

    // ---- Constants ----

    /// The constant expression `node`, read as a value of `expected`,
    /// the type its place expects (EXPR-5); `None` where that is not
    /// known. `place` names the place for a report: `` `X` of `Box` ``.
    fn constant(
        &mut self,
        node: &SyntaxNode,
        expected: Option<&Type>,
        place: Option<&str>,
    ) -> Constant {
        let read = self.typed_constant(node, expected);
        let constant = self.held(node, read, expected, place);
        self.note_values_in(node);
        constant
    }

    /// What the constant expression `node` reads as, nothing yet held
    /// to its place's type (`crate::constant`).
    pub(super) fn typed_constant(
        &self,
        node: &SyntaxNode,
        expected: Option<&Type>,
    ) -> Result<Typed, Fault> {
        self.constant_scope(|scope| crate::constant::typed(node, expected, scope))
    }

    /// Reads a constant in the declaration's compile-time scope.
    pub(super) fn constant_scope<T>(&self, read: impl FnOnce(&Scope<'_>) -> T) -> T {
        // A closure: a function written in place, which may use what is
        // in scope around it; here, `self`.
        let binding = |file: FileId, offset: u32| self.everywhere.get(&(file, offset)).copied();
        let parameter = |at: Location| self.value_type(at).cloned();
        let scope = Scope {
            program: self.program,
            file: self.file,
            binding: &binding,
            parameter: &parameter,
            arguments: &[],
            type_packs: &[],
            constants: &self.constants,
        };
        read(&scope)
    }

    /// The constant `node` is, read as `read`, held as a value of
    /// `expected`. Where `place` names its place, a value of another
    /// type is reported (`type.mismatch`, EXPR-5), and so is a member
    /// its enum does not have (`member.unknown`, TYPE-7) and `.member`
    /// where no enum is expected (`enum.context_type`, TYPE-7).
    pub(super) fn held(
        &mut self,
        node: &SyntaxNode,
        read: Result<Typed, Fault>,
        expected: Option<&Type>,
        place: Option<&str>,
    ) -> Constant {
        let value = read.and_then(|read| crate::constant::fit(read, expected, self.program));
        let value = match value {
            // GEN-8: a generic value argument is "a compile-time
            // constant ... Any other rejects." One whose value this pass
            // does not work out cannot be told from another, and is
            // rejected as that.
            Ok(value) if place.is_some() && !crate::constant::worked_out(&value) => {
                if let Some(place) = place {
                    self.diagnostics.emit(error(
                        Category::Type,
                        codes::GENERIC_ARGUMENT_CONSTANT,
                        self.at(node.span()),
                        format!(
                            "{place} is a generic value parameter, and this is no compile-time \
                             constant: this compiler does not work out its value"
                        ),
                        "GEN-8",
                    ));
                }
                ConstantValue::Unknown
            }
            Ok(value) => value,
            Err(Fault::NotConstant) if place.is_some() => {
                if let Some(place) = place {
                    self.diagnostics.emit(error(
                        Category::Type,
                        codes::GENERIC_ARGUMENT_CONSTANT,
                        self.at(node.span()),
                        format!(
                            "{place} is a generic value parameter, and this is no compile-time \
                             constant: it is known only when the program runs"
                        ),
                        "GEN-8",
                    ));
                }
                ConstantValue::Unknown
            }
            Err(fault) => {
                if let (Some(place), Some(expected)) = (place, expected) {
                    let at = self.at(node.span());
                    if let Some(report) = fault.report(at, place, expected) {
                        self.diagnostics.emit(report);
                    }
                }
                match fault {
                    // What is no constant is the same as nothing.
                    Fault::NotConstant => ConstantValue::Unevaluated,
                    // What has been reported says nothing more (ERR-9).
                    _ => ConstantValue::Unknown,
                }
            }
        };
        Constant {
            text: text_of(node),
            value,
        }
    }

    /// The default of every generic parameter of `declaration` of
    /// `module`, as the arguments of an application that supplies none:
    /// `Some` only where it has generic parameters and each is a value
    /// with a default (GEN-2). Each is read as a value of its
    /// parameter's type, in the file the declaration is written in.
    fn defaults_of(&self, (module, index): Id) -> Option<Vec<Argument>> {
        let declaration = &self.program.modules[module].declarations[index];
        let forms = generic_forms(&declaration.syntax);
        if forms.is_empty() {
            return None;
        }
        let file = self.program.modules[module].files[declaration.file].file;
        let types = self.parameter_types((module, index));
        // Where each parameter is declared: what a default that names one
        // is bound to.
        let own: Vec<Location> = forms
            .iter()
            .map(|(_, name, _)| Location::new(file, name.span()))
            .collect();
        let mut arguments = Vec::new();
        for (position, (parameter, _, form)) in forms.iter().enumerate() {
            let default = match form {
                Form::Value => default_of(parameter)?,
                Form::Plain | Form::Pack(..) => return None,
            };
            let expected = types
                .get(position)
                .cloned()
                .flatten()
                .filter(|ty| !matches!(ty, Type::Parameter { .. }));
            let read = {
                let binding =
                    |file: FileId, offset: u32| self.everywhere.get(&(file, offset)).copied();
                let scope = Scope {
                    program: self.program,
                    file,
                    binding: &binding,
                    parameter: &|_| None,
                    arguments: &[],
                    type_packs: &[],
                    constants: &self.constants,
                };
                crate::constant::read(&default, expected.as_ref(), &scope)
            };
            // What is wrong with a default is reported where it is
            // written (`signatures`), and says nothing more (ERR-9).
            let value = match read {
                Ok(value) => value,
                Err(Fault::NotConstant) => ConstantValue::Unevaluated,
                Err(_) => ConstantValue::Unknown,
            };
            // A default that names a parameter before its own is of the
            // value that one takes: `W2` of `struct W2<const A: i64 = 1,
            // const B: i64 = A>` is `W2<1, 1>`. One that names its own,
            // or one after it, gives it no value (GEN-8: "A call rejects
            // when it leaves a generic parameter unresolved"), and the
            // struct is written with too few arguments.
            let value = value.substituted(&|at| {
                let before = own.iter().position(|own| *own == at)?;
                match arguments.get(before)? {
                    Argument::Constant(given) => Some(given.value.clone()),
                    Argument::Type(_) | Argument::NoResult => None,
                }
            });
            if value.names_one_of(&own) {
                return None;
            }
            arguments.push(Argument::Constant(Constant {
                text: text_of(&default),
                value,
            }));
        }
        Some(arguments)
    }

    /// The type each generic parameter of `target` is declared with, as
    /// far as it is read before the program's types are
    /// ([`crate::constant::written_type`]): `None` for a type parameter
    /// and a pack, and for a value parameter of a type not read so.
    fn parameter_types(&self, (module, index): Id) -> Vec<Option<Type>> {
        let declaration = &self.program.modules[module].declarations[index];
        let file = self.program.modules[module].files[declaration.file].file;
        let binding = |file: FileId, offset: u32| self.everywhere.get(&(file, offset)).copied();
        generic_forms(&declaration.syntax)
            .iter()
            .map(|(parameter, _, form)| match form {
                Form::Value => type_child(parameter).and_then(|written| {
                    crate::constant::written_type(
                        &written,
                        self.program,
                        file,
                        &binding,
                        &self.constants,
                    )
                }),
                Form::Plain | Form::Pack(..) => None,
            })
            .collect()
    }

    /// The generic parameters of `target`, by where each is declared.
    fn parameter_places(&self, (module, index): Id) -> Vec<Location> {
        let declaration = &self.program.modules[module].declarations[index];
        let file = self.program.modules[module].files[declaration.file].file;
        generic_forms(&declaration.syntax)
            .iter()
            .map(|(_, name, _)| Location::new(file, name.span()))
            .collect()
    }
}
