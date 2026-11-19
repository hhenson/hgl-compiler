//! A call written with a name: of a value function, of one of the
//! language's forms, or of an enum, which constructs a member, each read
//! from what the name is bound to (FN-4, EXPR-2, TYPE-7). Part of the
//! typing pass ([`super`]); a call of an operator is read in
//! [`super::operators`].

use hgl_diagnostics::{Category, Location};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};
use hgl_source::{FileId, Span};

use super::arguments::{Slot, Takes};
use super::{Inferred, Places, Typer, infer, mentions, not_null, open, placed};
use crate::declared::{names_parameter, substitute_within};
use crate::hir::{self, Callee, EndpointOperation, ExprKind, Form, Hook};
use crate::program::{Binding, DeclarationKind, OperatorRef, Role};
use crate::syntax::{child, has_token, name_token, tokens, ungrouped};
use crate::types::{
    Argument, CallArguments, ConstantValue, Detail, GenericKind, NamedKind, Scalar, Type,
};
use crate::{codes, error};

impl<'a> Typer<'a> {
    // ---- Calls and constructions ----

    /// What the name that starts at `start` of `file` resolved to
    /// (UNIT-10), in whichever module the file is of: a candidate's
    /// default is written in its own module's file, which need not be
    /// the one being read.
    pub(super) fn binding_at(&self, file: FileId, start: u32) -> Option<&'a Binding> {
        if let Some(found) = self.bindings.get(&(file, start)) {
            return Some(found);
        }
        self.program
            .modules
            .iter()
            .flat_map(|module| module.uses.iter())
            .find(|used| used.at.file == file && used.at.span.start == start)
            .map(|used| &used.binding)
    }

    /// The declarations a name is bound to, when it names declarations
    /// of a module and not a local.
    pub(super) fn declarations(&self, name: &SyntaxToken) -> Option<(usize, &'a [usize])> {
        match self.bindings.get(&(self.file, name.span().start))? {
            Binding::Declarations {
                module,
                declarations,
            } => Some((*module, declarations.as_slice())),
            Binding::Local { .. } => None,
        }
    }

    /// `callee(arguments)`: a call of a value function, or the
    /// construction of a struct.
    ///
    /// FN-4: "runtime/value evaluation chooses the value definition."
    /// A name left for the prelude, an operator, and a callee that is
    /// itself an expression are for later parts: their arguments are
    /// read, and the call has no type.
    pub(super) fn call(&mut self, node: &SyntaxNode, expected: Option<&Type>) -> (ExprKind, Type) {
        self.call_read(node, expected)
    }

    /// A call, as the form or the function its callee names.
    fn call_read(&mut self, node: &SyntaxNode, expected: Option<&Type>) -> (ExprKind, Type) {
        let (Some(callee), Some(list)) = (node.children().next(), child(node, SyntaxKind::ArgList))
        else {
            return (ExprKind::Unread, Type::Unknown);
        };
        // ERR-13: "as the callee of a call each is the form this
        // chapter defines, whatever else is in scope."
        if callee.kind() == SyntaxKind::NameExpr
            && child(&callee, SyntaxKind::QualifiedName).is_none()
            && let Some(name) = name_token(&callee)
            && let Some(form) = self.result_form(&name, &list, expected)
        {
            return form;
        }
        let found = match callee.kind() {
            SyntaxKind::NameExpr => name_token(&callee)
                .and_then(|name| self.declarations(&name).map(|found| (name, found))),
            _ => None,
        };
        let Some((name, (module, declarations))) = found else {
            // TIME-9: "The operations on an endpoint are intrinsics of
            // the language". Nothing declares one, so its name is bound
            // to nothing; it is read in a node's hooks.
            if self.hook.is_some()
                && callee.kind() == SyntaxKind::NameExpr
                && let Some(name) = name_token(&callee)
                && !self.bindings.contains_key(&(self.file, name.span().start))
                && let Some(operation) = EndpointOperation::language_form(name.text())
            {
                return self.endpoint(operation, &name, &list);
            }
            // `(fn(x) => x + 1)(7)` (FN-10), and the call of what one
            // gives, `(fn(a) => fn(b) => a + b)(1)(2)`: the function is
            // read where it is written, for what it reads (FN-14).
            if calls_anonymous(&callee) {
                self.expression(&callee, Some(&Type::Unknown));
            }
            return self.unread_call(&list);
        };
        // TIME-9: "The operations on an endpoint are core's native value
        // functions ... `modified` and `valid` are selectors in a `when`
        // header (TIME-3): there they are forms of the language, which
        // set subscription and admission." In a node's hooks each name
        // is read as the form it is, a selector or a guard among them,
        // whether core has declared the function or not: the call is of
        // core's own, and what it gives is the same.
        if self.hook.is_some()
            && Some(module) == self.program.core()
            && child(&callee, SyntaxKind::QualifiedName).is_none()
            && let Some(operation) = EndpointOperation::language_form(name.text())
        {
            return self.endpoint(operation, &name, &list);
        }
        self.call_named(&name, &list, (module, declarations), expected, None)
    }

    /// A call of what `name` is bound to: `declarations` of a module.
    /// `generics` is the generic arguments the call writes after the
    /// name, when it writes them (GEN-8).
    pub(super) fn call_named(
        &mut self,
        name: &SyntaxToken,
        list: &SyntaxNode,
        (module, declarations): (usize, &[usize]),
        expected: Option<&Type>,
        generics: Option<&CallArguments>,
    ) -> (ExprKind, Type) {
        let program = self.program;
        let kind_of = |declaration: usize| program.modules[module].declarations[declaration].kind;
        // FN-4: "Composition and eval choose the temporal definition if
        // one exists; runtime/value evaluation chooses the value
        // definition." FN-3: "A composition body calls any function".
        let temporal_function = declarations.iter().copied().find(|&declaration| {
            matches!(
                kind_of(declaration),
                DeclarationKind::Function(Role::Temporal)
                    | DeclarationKind::NativeFunction(Role::Temporal)
            )
        });
        if let (true, Some(declaration)) = (self.graph, temporal_function) {
            return self.call_function(name, list, (module, declaration), expected, generics);
        }
        let value_function = declarations.iter().copied().find(|&declaration| {
            matches!(
                kind_of(declaration),
                DeclarationKind::Function(Role::Value)
                    | DeclarationKind::NativeFunction(Role::Value)
            )
        });
        if let Some(declaration) = value_function {
            return self.call_function(name, list, (module, declaration), expected, generics);
        }
        let structure = declarations
            .iter()
            .copied()
            .find(|&declaration| kind_of(declaration) == DeclarationKind::Struct);
        if let Some(declaration) = structure {
            // GEN-2: a generic struct "infers them from constructor
            // fields and expected type".
            let given = match not_null(expected) {
                Some(Type::Named {
                    module: expected_module,
                    declaration: expected_declaration,
                    arguments,
                    ..
                }) if (*expected_module, *expected_declaration) == (module, declaration) => {
                    Some(arguments.clone())
                }
                _ => None,
            };
            let settled = !expected.is_some_and(open);
            return self.construct(name, list, module, declaration, given, settled);
        }
        // TYPE-7: "A call of the enum's name constructs a member."
        let enumeration = declarations
            .iter()
            .copied()
            .find(|&declaration| kind_of(declaration) == DeclarationKind::Enum);
        if let Some(declaration) = enumeration {
            return self.enum_from(name, list, module, declaration);
        }
        let operator = declarations
            .iter()
            .copied()
            .find(|&declaration| kind_of(declaration) == DeclarationKind::Operator);
        if let Some(declaration) = operator {
            let operator = OperatorRef {
                module,
                declaration,
            };
            return self.call_operator(name, list, operator, expected, generics);
        }
        let temporal = declarations.iter().any(|&declaration| {
            matches!(
                kind_of(declaration),
                DeclarationKind::Function(Role::Temporal)
                    | DeclarationKind::NativeFunction(Role::Temporal)
            )
        });
        // NATIVE-6: "a native type is not constructed in HGL: a call of
        // its name rejects."
        let native = declarations
            .iter()
            .any(|&declaration| kind_of(declaration) == DeclarationKind::NativeType);
        if native {
            self.diagnostics.emit(
                error(
                    Category::Type,
                    codes::CALL_NOT_CALLABLE,
                    Location::new(self.file, name.span()),
                    format!(
                        "`{}` is a native type, and no call constructs one",
                        name.text()
                    ),
                    "NATIVE-6",
                )
                .with_note("a value of a native type comes from a native function"),
            );
        }
        if temporal {
            // FN-3: "A runtime body calls only value functions, and so
            // does a value function." FN-4: "A bad selected signature
            // is an error, not fallback to the other role."
            self.diagnostics.emit(error(
                Category::FunctionKind,
                codes::CALL_ROLE,
                Location::new(self.file, name.span()),
                format!(
                    "`{}` is a temporal function, and a {} calls value functions",
                    name.text(),
                    if self.hook.is_some() {
                        "hook"
                    } else if self.generating {
                        "source"
                    } else {
                        "value function"
                    }
                ),
                "FN-3",
            ));
        }
        self.unread_call(list)
    }

    /// `Side(1)` or `Side("buy")` (TYPE-7): "A call of the enum's name
    /// constructs a member. It takes one argument, by position, that is
    /// an `i64` or a `str`, and any other type rejects: `Side(1)` is the
    /// member whose number is 1, and `Side("buy")` the member of that
    /// exact name. Construction is checked: a constant that is no
    /// member's number or name rejects, and a value that is none fails
    /// where it is evaluated."
    fn enum_from(
        &mut self,
        name: &SyntaxToken,
        list: &SyntaxNode,
        module: usize,
        declaration: usize,
    ) -> (ExprKind, Type) {
        let ty = Type::Named {
            name: name.text().to_string(),
            module,
            declaration,
            arguments: Vec::new(),
            kind: NamedKind::Enum,
        };
        let Some(argument) = self.one_argument(name, list, "TYPE-7") else {
            return (ExprKind::Unread, ty);
        };
        let value = self.expression(&argument, None);
        let number = value.ty == Type::Scalar(Scalar::I64);
        let text = value.ty == Type::Scalar(Scalar::Str);
        if !number && !text {
            // What is not known has been reported (ERR-9).
            if !open(&value.ty) {
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::TYPE_MISMATCH,
                    value.at,
                    format!(
                        "`{}` is constructed from a member's number, an `i64`, or its name, \
                         a `str`, and this is `{}`",
                        name.text(),
                        value.ty
                    ),
                    "TYPE-7",
                ));
            }
            return (ExprKind::Unread, ty);
        }
        let kind = match self.member_constructed(module, declaration, &argument) {
            // A constant that names a member is that member: `Side(0)`
            // is `Side.buy`, wherever a member is looked for, a `switch`
            // label and a `const` argument among them.
            Some(Ok(member)) => ExprKind::EnumMember {
                module,
                declaration,
                member,
            },
            Some(Err(missing)) => {
                self.diagnostics.emit(
                    error(
                        Category::Name,
                        codes::MEMBER_UNKNOWN,
                        value.at,
                        format!("`{}` has {missing}", name.text()),
                        "TYPE-7",
                    )
                    .with_note("a member's name is matched exactly"),
                );
                ExprKind::Unread
            }
            None => ExprKind::EnumFrom {
                module,
                declaration,
                value: Box::new(value),
            },
        };
        (kind, ty)
    }

    /// The arguments of the call `list` is the list of. For the call a
    /// `capture` names, they are what follows the callable's name:
    /// `capture(required, value)` gives `required` the argument
    /// `value` (ERR-7).
    pub(super) fn call_arguments(
        &self,
        list: &SyntaxNode,
    ) -> Vec<(Option<SyntaxToken>, SyntaxNode)> {
        arguments_of(list)
            .into_iter()
            .filter(|(_, value)| Some(value) != self.callable.as_ref())
            .collect()
    }

    /// A call this pass does not read: its arguments are read for what
    /// they report themselves, with nothing expected of them.
    pub(super) fn unread_call(&mut self, list: &SyntaxNode) -> (ExprKind, Type) {
        let arguments = self
            .call_arguments(list)
            .into_iter()
            .map(|(_, value)| hir::Argument {
                parameter: None,
                value: self.expression(&value, Some(&Type::Unknown)),
            })
            .collect();
        let kind = ExprKind::Call {
            callee: Callee::Unread,
            arguments,
            form: Form::Named,
        };
        (kind, Type::Unknown)
    }

    /// A call of the value function `declaration` of `module`.
    /// `given` is the generic arguments the call writes, when it writes
    /// them.
    ///
    /// GEN-2: "Resolve every needed generic from actual inputs,
    /// requested output, or solvable equality." GEN-8: "A call may
    /// supply its generic arguments as a generic constructor does,
    /// `count<i64>(3)`. It may supply the first of them and leave the
    /// rest to be resolved under GEN-2 ... A call rejects when it leaves
    /// a generic parameter unresolved." GEN-1: "Repeated parameters
    /// require equivalent types or equal constants."
    ///
    /// GEN-2: "A call resolves a generic of its callee from what it
    /// writes after the name (GEN-8), then from the type its place
    /// expects of the result, then from its arguments in declared
    /// parameter order (GEN-12), whatever order they are written in.
    /// The first of these that says what a parameter is decides it, and
    /// what follows is checked against that: an integer literal is
    /// widened to an `f64` so decided (EXPR-5), and another argument of
    /// another type rejects." So `let real: f64 = same(1, 2)` is an
    /// `f64`.
    ///
    /// Not read here: a function with a type pack or a pack parameter.
    /// A type of it that names a generic parameter is then not known.
    pub(super) fn call_function(
        &mut self,
        name: &SyntaxToken,
        list: &SyntaxNode,
        (module, declaration): (usize, usize),
        expected: Option<&Type>,
        given: Option<&CallArguments>,
    ) -> (ExprKind, Type) {
        // The call an `eval` stands for is this one, and not one among
        // its arguments.
        let evaluating = std::mem::take(&mut self.evaluating);
        let target = &self.program.modules[module].declarations[declaration];
        let reads_activity = Some(module) == self.program.core()
            && target.kind == DeclarationKind::NativeFunction(Role::Value)
            && target.name == "is_active";
        let (kind, ty) = self.call_function_with(
            name,
            list,
            (module, declaration),
            expected,
            given,
            // An endpoint is never a replayable payload. Even an empty
            // trace cannot supply the receiving node's input (TIME-9).
            evaluating && !reads_activity,
        );
        // END-5, TIME-9: only the bound core function queries activity.
        // Its ordinary signature has already checked the arguments and T.
        if reads_activity
            && let ExprKind::Call { arguments, .. } = &kind
            && let [argument] = arguments.as_slice()
            && let ExprKind::EndpointOf(input) = argument.value.kind
        {
            if Some(input) == self.out {
                self.diagnostics.emit(error(
                    Category::Phase,
                    codes::ENDPOINT_ARGUMENT,
                    argument.value.at,
                    "`is_active` takes a direct temporal input of the node".to_string(),
                    "TIME-9",
                ));
            } else if self.hook == Some(Hook::Start) {
                let input_name = self.locals[input].name.clone();
                self.input_lifecycle(Location::new(self.file, name.span()), &input_name);
            }
            return (
                ExprKind::Endpoint {
                    operation: EndpointOperation::IsActive,
                    inputs: Some(vec![input]),
                },
                ty,
            );
        }
        // TIME-9: "`is_active(input)`, `activate(input)` and
        // `passivate(input)` operate on direct temporal inputs": a child
        // has no subscription of its own.
        if reads_activity
            && let ExprKind::Call { arguments, .. } = &kind
            && let [argument] = arguments.as_slice()
            && let ExprKind::ChildEndpointOf(_) = argument.value.kind
        {
            self.diagnostics.emit(error(
                Category::Phase,
                codes::ENDPOINT_ARGUMENT,
                argument.value.at,
                "`is_active` takes a direct temporal input of the node, and this is a child of \
                 one"
                .to_string(),
                "TIME-9",
            ));
            return (ExprKind::Unread, ty);
        }
        (kind, ty)
    }

    /// [`Typer::call_function`], with `evaluating` true where the call
    /// is the one an `eval` stands for (TST-5): "Const parameters
    /// receive configuration; temporal parameters receive contextually
    /// typed publication traces."
    fn call_function_with(
        &mut self,
        name: &SyntaxToken,
        list: &SyntaxNode,
        (module, declaration): (usize, usize),
        expected: Option<&Type>,
        given: Option<&CallArguments>,
        evaluating: bool,
    ) -> (ExprKind, Type) {
        let program = self.program;
        let Some(Detail::Callable(signature)) = program.modules[module].details.get(declaration)
        else {
            return self.unread_call(list);
        };
        // What a parameter takes: its type, or in an eval a trace for
        // one that is not `const`. TST-5: "Each cell is one cycle's
        // publication: it is read as the `delta<T>` (TYPE-16) of its
        // parameter's type T". A type that names a generic parameter
        // stands for its cells until they say what it is.
        let taken = |parameter: &crate::types::Parameter, ty: Type| {
            if !evaluating || parameter.constant || parameter.pack.is_some() {
                return ty;
            }
            let cell = if names_parameter(&ty) || matches!(ty, Type::Unknown) {
                ty
            } else {
                Type::delta(ty)
            };
            Type::Trace {
                cell: Box::new(cell),
                style: None,
            }
        };
        // TST-16: "A trace given to a `const fn` is an ordinary value
        // there". Not in the call an eval stands for: a `const fn` that
        // is an eval's target is lifted (FN-4), and its parameters take
        // traces as any target's do (TST-5).
        let listed = !evaluating
            && matches!(
                program.modules[module].declarations[declaration]
                    .kind
                    .role(),
                Some(Role::Value)
            );
        let generics = &signature.generics;
        let packed = generics
            .iter()
            .any(|generic| matches!(generic.kind, GenericKind::Pack { .. }))
            || signature
                .parameters
                .iter()
                .any(|parameter| parameter.pack.is_some());
        // What the call writes of the generic parameters. Arguments
        // that were rejected say nothing: the call is then read as one
        // whose generic parameters are not known (ERR-9).
        let explicit = if packed {
            None
        } else {
            self.generic_arguments(name, generics, given)
        };
        let required = |parameter: &crate::types::Parameter| {
            // UNIT-8: "A parameter whose type is written `T?` needs no
            // default to be omitted."
            // "A `var` parameter is the exception: it is always
            // given, whatever its type" (FN-15).
            parameter.var || !parameter.has_default && !matches!(parameter.ty, Type::Nullable(_))
        };
        let Some((mut found, faulty)) = explicit.filter(|_| !generics.is_empty()) else {
            // No generic parameter, or none this pass resolves: a type
            // that names one is not known.
            let known = |ty: &Type| {
                if names_parameter(ty) {
                    Type::Unknown
                } else {
                    ty.clone()
                }
            };
            let slots: Vec<Slot> = signature
                .parameters
                .iter()
                .map(|parameter| Slot {
                    name: &parameter.name,
                    ty: taken(parameter, known(&parameter.ty)),
                    required: required(parameter),
                    pack: parameter.pack,
                    listed,
                })
                .collect();
            let reported = self.diagnostics.len();
            let arguments = self.bind(name, list, &slots, Takes::Parameters);
            let ty = signature.result.as_ref().map_or(Type::Unknown, known);
            // GEN-3: a function with no generic parameter may state a
            // `requires` too, of types it names itself, and a call of
            // it is held to that. Not where something else was
            // reported of the call (ERR-9), nor where the function has
            // generic parameters this pass does not resolve.
            if generics.is_empty() && self.diagnostics.len() == reported {
                let at = Location::new(self.file, Span::new(name.span().start, list.span().end));
                self.requires_met(at, (module, declaration), &[]);
            }
            let requirements = if generics.is_empty() {
                self.answers(module, declaration, &[])
            } else {
                Vec::new()
            };
            let callee = Callee::Function {
                module,
                declaration,
                generics: Vec::new(),
                requirements,
            };
            let kind = ExprKind::Call {
                callee,
                arguments,
                form: Form::Named,
            };
            self.finished_call(&kind, evaluating);
            return (kind, ty);
        };

        // GEN-8: "A call may give a generic value parameter of its
        // callee by name, among its named arguments". What the call
        // writes decides first (GEN-2).
        let value_names: Vec<&str> = generics
            .iter()
            .filter(|generic| matches!(generic.kind, GenericKind::Value(_)))
            .map(|generic| generic.name.as_str())
            .collect();
        let callee = &program.modules[module].declarations[declaration];
        let reported = self.diagnostics.len();
        let named = self.named_generics(list, generics, &mut found);

        // The requested output: `let names: list<str> = repeat(3)` says
        // what the `T` of `repeat<T>(count: i64) -> list<T>` is. A `T`
        // result given to a `T?` place is the `T`.
        if let (Some(result), Some(expected)) = (&signature.result, expected)
            && !open(expected)
        {
            let wanted = match result {
                Type::Nullable(_) => expected,
                _ => not_null(Some(expected)).unwrap_or(expected),
            };
            infer(result, wanted, generics, &mut found);
        }
        // The inputs.
        let slots: Vec<Slot> = signature
            .parameters
            .iter()
            .map(|parameter| Slot {
                name: &parameter.name,
                ty: Type::Unknown,
                required: required(parameter),
                pack: parameter.pack,
                listed,
            })
            .collect();
        let patterns: Vec<Type> = signature
            .parameters
            .iter()
            .map(|parameter| taken(parameter, parameter.ty.clone()))
            .collect();
        let places = Places {
            slots: &slots,
            patterns: patterns.iter().collect(),
            generics,
            settled: !expected.is_some_and(open),
            aside: &value_names,
            values: &named,
            defaults: generics
                .iter()
                .map(|generic| self.default_constant(callee, generic))
                .collect(),
        };
        let Inferred {
            arguments,
            mut found,
            unknown,
            defaulted,
        } = self.infer_arguments(name, list, &places, found, Takes::Parameters);
        self.generic_values(name, callee, generics, (&named, &defaulted), &mut found);

        // GEN-8: "A call rejects when it leaves a generic parameter
        // unresolved." Not where something else was reported, or an
        // argument is not known: what would have said what the
        // parameter is may be what is missing (ERR-9).
        //
        // Where the call's place is not known, the type it expects may
        // yet say what a parameter of the result is: the call then has
        // no whole type and is read again once the place is known. A
        // parameter the result does not name is never resolved that
        // way, so it is reported here: the `N` of
        // `tag<const N: i64>(x: i64) -> i64` in `same(tag(1), 2)`.
        let place_unknown = expected.is_some_and(open);
        let unresolved: Vec<&str> = generics
            .iter()
            .zip(&found)
            .filter(|(_, found)| found.is_none())
            .filter(|(generic, _)| {
                !place_unknown
                    || !signature
                        .result
                        .as_ref()
                        .is_some_and(|result| mentions(result, &[generic.at]))
            })
            .map(|(generic, _)| generic.name.as_str())
            .collect();
        let judged = !faulty && self.diagnostics.len() == reported && !unknown;
        if judged && !unresolved.is_empty() {
            let names: Vec<String> = unresolved.iter().map(|name| format!("`{name}`")).collect();
            self.diagnostics.emit(
                error(
                    Category::Type,
                    codes::GENERIC_UNRESOLVED,
                    Location::new(self.file, name.span()),
                    format!(
                        "this call of `{}` leaves {} unresolved: no argument and no expected \
                         type says what it is",
                        name.text(),
                        names.join(", ")
                    ),
                    "GEN-8",
                )
                .with_note(format!(
                    "a call may write its generic arguments: `{}<...>(...)`",
                    name.text()
                )),
            );
        }

        // The result, with what was found in place. One that still
        // names a parameter nothing resolved is not known.
        let own: Vec<Location> = generics.iter().map(|generic| generic.at).collect();
        let resolved: Vec<Argument> = found
            .into_iter()
            .map(|found| found.unwrap_or(Argument::Type(Type::Unknown)))
            .collect();
        let map: Vec<(Location, &Argument)> = own.iter().copied().zip(&resolved).collect();
        let ty = signature.result.as_ref().map_or(Type::Unknown, |result| {
            placed(substitute_within(result, &map), &own)
        });
        // GEN-3: the callee's `requires` holds with what was resolved.
        // Not where something else was reported of the call (ERR-9).
        if judged && unresolved.is_empty() {
            let at = Location::new(self.file, Span::new(name.span().start, list.span().end));
            self.requires_met(at, (module, declaration), &resolved);
        }
        let requirements = self.answers(module, declaration, &resolved);
        let callee = Callee::Function {
            module,
            declaration,
            generics: resolved,
            requirements,
        };
        let kind = ExprKind::Call {
            callee,
            arguments,
            form: Form::Named,
        };
        self.finished_call(&kind, evaluating);
        (kind, ty)
    }

    /// The member an enum's construction from `argument` names, where
    /// that is a constant: its place among the members, `Ok`, or what
    /// the enum has none of, `Err`. `None` for a value that is no
    /// constant read here. TYPE-7: "`Side(1)` is the member whose
    /// number is 1, and `Side("buy")` the member of that exact name."
    ///
    /// A member whose number is not known says nothing sure of which
    /// numbers the enum has, and a number is then not looked for.
    fn member_constructed(
        &self,
        module: usize,
        declaration: usize,
        argument: &SyntaxNode,
    ) -> Option<Result<usize, String>> {
        let Some(Detail::Enum(read)) = self.program.modules[module].details.get(declaration) else {
            return None;
        };
        let members = &read.members;
        match self.read_constant(argument, None).ok()? {
            ConstantValue::Int(given) if members.iter().all(|member| member.value.is_some()) => {
                Some(
                    members
                        .iter()
                        .position(|member| member.value == Some(given))
                        .ok_or_else(|| format!("no member numbered {given}")),
                )
            }
            ConstantValue::Str(given) => Some(
                members
                    .iter()
                    .position(|member| member.name == given)
                    .ok_or_else(|| format!("no member named `{given}`")),
            ),
            _ => None,
        }
    }
}

/// True when the callee `node` is an anonymous function, in
/// parentheses or not, or a call of one, or of what such a call gives.
fn calls_anonymous(node: &SyntaxNode) -> bool {
    let node = &ungrouped(node.clone());
    match node.kind() {
        SyntaxKind::FnExpr => true,
        SyntaxKind::CallExpr => node
            .children()
            .next()
            .is_some_and(|inner| calls_anonymous(&inner)),
        _ => false,
    }
}

/// The arguments of an argument list, in order: each with its name,
/// when it has one, and its value.
pub(super) fn arguments_of(list: &SyntaxNode) -> Vec<(Option<SyntaxToken>, SyntaxNode)> {
    list.children()
        .filter(|argument| argument.kind() == SyntaxKind::Arg)
        .filter_map(|argument| {
            let value = argument.children().next()?;
            // `name: value`: the name is the first token, any word
            // (LEX-9).
            let name = has_token(&argument, SyntaxKind::Colon)
                .then(|| tokens(&argument).next())
                .flatten();
            Some((name, value))
        })
        .collect()
}
