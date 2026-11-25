//! Checks FN-17 templates and records the static target of a function argument.

use hgl_diagnostics::{Category, Location};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};

use super::Typer;
use super::arguments::{Slot, Takes};
use super::callback_evidence::CallbackEvidence;
use super::candidates::{
    CallbackOption, CallbackOptionSource, CallbackRequest, Given, Key, Wanted, in_instance,
};
use super::constants::generic_default;
use super::dependent_packs::PackTypes;
use super::fit::{accepts_float, is_integer_literal, open, whole};
use super::forwarded_packs::spread_operands;
use super::inference::{ExplicitGenerics, infer, mentions, pack_bounds, type_pack};
use crate::declared::{agree, names_parameter, substitute_within};
use crate::hir::{
    AnonymousFunction, AnonymousSpecialization, Callee, Expr, ExprKind, Form, LocalId, Node, Stmt,
    TypePackArgument, ValueCategory, walk_statement_nodes,
};
use crate::program::{Declaration, OperatorRef, Role};
use crate::syntax::{child, name_token, ungrouped};
use crate::types::{
    Argument, CallArguments, ConstantValue, Detail, FunctionParameter, FunctionType, GenericKind,
    GenericParameter, Pack, Parameter, Signature, Type,
};
use crate::{codes, error, local_error};

impl<'a> Typer<'a> {
    /// Read an argument the callee expects to be a function known at compile time.
    pub(super) fn function_argument(&mut self, node: &SyntaxNode, template: &FunctionType) -> Expr {
        self.function_argument_with_inputs(node, template, &CallbackInputs::default())
    }

    /// FN-17: an operator call must determine its static argument's type.
    pub(super) fn operator_function_argument(
        &mut self,
        node: &SyntaxNode,
        template: &FunctionType,
        inputs: &CallbackInputs<'_>,
    ) -> Expr {
        let reported = self.diagnostics.len();
        let value = self.function_argument_with_inputs(node, template, inputs);
        if self.diagnostics.len() == reported && !whole(&value.ty) {
            self.diagnostics.emit(error(
                Category::Type,
                codes::GENERIC_UNRESOLVED,
                self.at(node),
                "the call does not determine every type in this function argument".to_string(),
                "FN-17",
            ));
        }
        value
    }

    /// FN-17: an operator named at a reflected template selects by the
    /// resolved argument types, including those in the dependent pack.
    pub(super) fn function_argument_with_inputs(
        &mut self,
        node: &SyntaxNode,
        template: &FunctionType,
        inputs: &CallbackInputs<'_>,
    ) -> Expr {
        let at = self.at(node);
        let argument = ungrouped(node.clone());
        let resolved = self.function_target(&argument, template, inputs);
        let (kind, ty) = resolved.map_or((ExprKind::Unread, Type::Unknown), |(callee, actual)| {
            if !matches_template(template, &actual) {
                self.template_mismatch(at, template, &actual);
                return (ExprKind::Unread, Type::Unknown);
            }
            (
                ExprKind::StaticCallable(Box::new(callee)),
                Type::Function(Box::new(actual)),
            )
        });
        Expr {
            kind,
            ty,
            at,
            category: ValueCategory::Value,
        }
    }

    fn function_target(
        &mut self,
        node: &SyntaxNode,
        template: &FunctionType,
        inputs: &CallbackInputs<'_>,
    ) -> Option<(Callee, FunctionType)> {
        if node.kind() == SyntaxKind::FnExpr {
            return self.anonymous_target(node, template, false);
        }
        if matches!(
            node.kind(),
            SyntaxKind::NameExpr | SyntaxKind::FunctionReference
        ) && let Some(name) = name_token(node)
        {
            if let Some(local) = self.local_of(node)
                && let Type::Function(actual) = &self.locals[local].ty
            {
                let callee = Callee::Parameter {
                    parameter: local,
                    temporal: !actual.value,
                };
                return Some((callee, actual.as_ref().clone()));
            }
            if let Some((module, declarations)) = self.declarations(&name) {
                let program = self.program;
                if let Some(declaration) = declarations.iter().copied().find(|&index| {
                    program.modules[module].declarations[index].kind
                        == crate::program::DeclarationKind::Operator
                }) {
                    return self.operator_target(
                        node,
                        &name,
                        OperatorRef {
                            module,
                            declaration,
                        },
                        (template, inputs),
                    );
                }
                let preferred = if template.value {
                    Role::Value
                } else {
                    Role::Temporal
                };
                let target = |role| {
                    declarations.iter().copied().find(|&index| {
                        program.modules[module].declarations[index].kind.role() == Some(role)
                    })
                };
                if let Some(declaration) = target(preferred).or_else(|| {
                    target(if template.value {
                        Role::Temporal
                    } else {
                        Role::Value
                    })
                }) {
                    return self.declared_target(
                        node,
                        &name,
                        (module, declaration),
                        (template, inputs),
                    );
                }
            }
        }
        self.diagnostics.emit(error(
            Category::Type,
            codes::FUNCTION_ARGUMENT,
            self.at(node),
            "a function parameter needs a function known where the call is compiled".to_string(),
            "FN-17",
        ));
        None
    }

    fn declared_target(
        &mut self,
        node: &SyntaxNode,
        name: &SyntaxToken,
        (module, declaration): (usize, usize),
        (template, inputs): (&FunctionType, &CallbackInputs<'_>),
    ) -> Option<(Callee, FunctionType)> {
        let program = self.program;
        let unit = &program.modules[module];
        let Detail::Callable(signature) = unit.details.get(declaration)? else {
            return None;
        };
        let declared = &unit.declarations[declaration];
        if let Some(parameter) = missing_callback_configuration(signature, None) {
            self.callback_configuration(node, parameter);
            return None;
        }
        if !plain_inputs(signature, template) {
            self.marked_function(node, signature);
            return None;
        }
        let given = inputs
            .supplied
            .map(|_| {
                reflected_arguments(
                    self,
                    signature,
                    template,
                    node,
                    &CallbackInputs {
                        evidence: inputs.evidence.filter(|_| !signature.generics.is_empty()),
                        ..*inputs
                    },
                )
            })
            .transpose()
            .ok()?;
        let ReferenceArguments {
            generics,
            type_packs,
            ..
        } = self.reference_arguments(
            node,
            name,
            (declared, signature),
            (template, given.as_ref().map(|(given, _)| given.as_slice())),
        )?;
        let map = signature
            .generics
            .iter()
            .zip(&generics)
            .filter(|(generic, _)| !matches!(generic.kind, GenericKind::Pack { .. }))
            .map(|(generic, argument)| (generic.at, argument))
            .collect::<Vec<_>>();
        let mut taken = in_instance(signature, &map);
        // GEN-10: ordinary specialization above keeps each scalar
        // generic as one type; pack projection expands tuple sequences.
        let packs = PackTypes::new(&signature.generics, &type_packs);
        for parameter in &mut taken.parameters {
            parameter.ty = packs.project(&parameter.ty, &map).ok()?;
        }
        // GEN-3: an earlier argument's binding cannot discard a different
        // member type another argument supplies to the same generic.
        if let Some((given, true)) = given
            && let Some(argument) = given.iter().find(|argument| {
                callback_parameter(signature, &argument.key)
                    .is_some_and(|slot| !self.admits(&taken.parameters[slot].ty, argument.ty()))
            })
        {
            self.diagnostics.emit(error(
                Category::Type,
                codes::GENERIC_UNRESOLVED,
                argument.value.at,
                "this argument's derived type conflicts with the bare function's inputs"
                    .to_string(),
                "GEN-3",
            ));
            return None;
        }
        taken.result = taken
            .result
            .as_ref()
            .map(|ty| packs.project(ty, &map))
            .transpose()
            .ok()?;
        let actual = function_type(&taken, declared.kind.role() == Some(Role::Value));
        if matches_template(template, &actual) {
            let target = (module, declaration);
            let at = self.at(node);
            if type_packs.is_empty() {
                if template.types().all(whole) {
                    self.requires_met(at, target, &generics);
                }
            } else if actual.types().all(whole)
                && type_packs.iter().flat_map(|pack| &pack.types).all(whole)
            {
                self.pack_requires_met(at, target, &generics, &type_packs);
            }
        }
        let requirements = if type_packs.is_empty() {
            self.answers(module, declaration, &generics)
        } else {
            Vec::new()
        };
        let callee = Callee::Function {
            module,
            declaration,
            generics,
            type_packs,
            requirements,
            defer_pack_members: false,
            contextual: Vec::new(),
            test_candidates: self.test_candidates,
        };
        Some((callee, actual))
    }

    fn operator_target(
        &mut self,
        node: &SyntaxNode,
        name: &SyntaxToken,
        operator: OperatorRef,
        (template, inputs): (&FunctionType, &CallbackInputs<'_>),
    ) -> Option<(Callee, FunctionType)> {
        let unit = &self.program.modules[operator.module];
        let Detail::Callable(public) = &unit.details[operator.declaration] else {
            return None;
        };
        if let Some(parameter) = missing_callback_configuration(public, None) {
            self.callback_configuration(node, parameter);
            return None;
        }
        let at = self.at(node);
        let reflected = template.named_pack.is_some();
        let mut given = if reflected {
            reflected_arguments(self, public, template, node, inputs)
                .ok()?
                .0
        } else {
            Vec::new()
        };
        if let Some(pack) = &template.named_pack
            && !given.iter().any(|argument| whole(argument.ty()))
        {
            self.diagnostics.emit(error(
                Category::Type,
                codes::GENERIC_UNRESOLVED,
                at,
                format!(
                    "the call leaves `{}` unresolved: no parameter type selects a candidate of `{}`",
                    pack.name,
                    name.text()
                ),
                "FN-17",
            ));
            return None;
        }
        // With written generics the operator can also complete a template
        // still open after the enclosing call's other arguments were read.
        let explicit = if node.kind() == SyntaxKind::FunctionReference {
            Some(self.operator_reference(node, name, operator, (template, &given))?)
        } else {
            // FN-17: the inputs can select the callback and thereby
            // determine a result type the receiving template leaves open.
            if !reflected
                && !template
                    .ordinary_parameters()
                    .all(|input| whole(input.ty()))
            {
                return None;
            }
            None
        };
        let request = explicit
            .as_ref()
            .map_or(template, |operator| &operator.signature);
        if !reflected {
            given.extend(
                request
                    .ordinary_parameters()
                    .enumerate()
                    .map(|(index, input)| callback_operand(index, node, at, input.ty().clone())),
            );
        } else if explicit.is_some() {
            // FN-17: use the reference's known fixed input types to
            // select the callback. The dependent call binds its own
            // argument types separately (MAP-2 gives a list to a
            // scalar callback input).
            for argument in &mut given {
                if let Some(index) = argument.key.public_parameter(public) {
                    let input = request.parameters[index].ty();
                    if !input.any_part(&open, &|_| false) {
                        argument.value.ty = input.clone();
                    }
                }
            }
        }
        let graph = std::mem::replace(&mut self.graph, !template.value);
        let value_code = std::mem::replace(&mut self.value_code, false);
        let candidates: Vec<_> = self
            .value_candidates(operator)
            .into_iter()
            .filter(|(_, _, signature)| {
                plain_inputs(signature, template)
                    && missing_callback_configuration(signature, Some(public)).is_none()
            })
            .collect();
        let mut requested_values = Vec::new();
        if let Some(explicit) = &explicit
            && let Detail::Callable(public) =
                &self.program.modules[operator.module].details[operator.declaration]
        {
            let declared =
                &self.program.modules[operator.module].declarations[operator.declaration];
            let written = self.reference_written(node, name);
            let map: Vec<_> = public
                .generics
                .iter()
                .map(|parameter| parameter.at)
                .zip(&explicit.generics)
                .collect();
            for (index, (parameter, argument)) in
                public.generics.iter().zip(&explicit.generics).enumerate()
            {
                let (GenericKind::Value(ty), Argument::Constant(value)) =
                    (&parameter.kind, argument)
                else {
                    continue;
                };
                let source = if let Some(node) = written
                    .and_then(|arguments| arguments.arguments.get(index))
                    .and_then(|(at, _)| self.node_at(*at))
                {
                    CallbackOptionSource::Written(node)
                } else if explicit.defaulted.contains(&index)
                    && let Some(node) = generic_default(&declared.syntax, parameter)
                {
                    CallbackOptionSource::Default {
                        file: declared.at.file,
                        node,
                        generics: &public.generics,
                        arguments: &explicit.generics,
                    }
                } else {
                    CallbackOptionSource::Resolved
                };
                requested_values.push(CallbackOption {
                    name: &parameter.name,
                    value,
                    ty: substitute_within(ty, &map),
                    source,
                });
            }
        }
        let wanted = Wanted {
            place: request.result.as_ref(),
            among: explicit
                .as_ref()
                .and_then(|operator| operator.among.clone()),
            written: explicit.is_some(),
            callback: (explicit.is_some() || reflected).then_some(CallbackRequest {
                reference: request.reference,
                values: &requested_values,
                reflected: reflected.then_some(inputs.excluded),
            }),
        };
        let role = if template.value {
            Role::Value
        } else {
            Role::Temporal
        };
        let (mut matches, unknown) =
            self.admitted_for_callback(operator, &candidates, &given, &wanted, role);
        self.graph = graph;
        self.value_code = value_code;
        if !unknown
            && matches.len() == 1
            && let Some(chosen) = matches.pop()
        {
            let module = chosen.module;
            let declaration = chosen.declaration;
            let taken = chosen.signature();
            let value = self.program.modules[module].declarations[declaration]
                .kind
                .role()
                == Some(Role::Value);
            let mut actual = function_type(taken, value);
            for (parameter, position) in public
                .parameters
                .iter()
                .zip(public.candidate_parameters(taken))
            {
                if let Some(position) = position
                    && let FunctionParameter::Declared(input) = &mut actual.parameters[position]
                {
                    input.name.clone_from(&parameter.name);
                    input.has_default |= parameter.has_default;
                }
            }
            let requirements = self.answers(module, declaration, &chosen.generics);
            let operator_generics = explicit.map_or_else(Vec::new, |reference| {
                let mut found: Vec<_> = reference
                    .generics
                    .into_iter()
                    .map(|argument| {
                        (!matches!(argument, Argument::Type(Type::Unknown))).then_some(argument)
                    })
                    .collect();
                if let (Some(pattern), Some(result)) = (&public.result, &actual.result) {
                    infer(pattern, result, &public.generics, &mut found);
                }
                found
                    .into_iter()
                    .map(|argument| argument.unwrap_or(Argument::Type(Type::Unknown)))
                    .collect()
            });
            let callee = Callee::Candidate {
                operator: (operator.module, operator.declaration),
                operator_generics,
                module,
                declaration,
                generics: chosen.generics,
                type_packs: chosen.type_packs,
                requirements,
                test_candidates: self.test_candidates,
            };
            return Some((callee, actual));
        }
        if reflected
            && !public
                .parameters
                .iter()
                .any(|parameter| parameter.pack == Some(Pack::Named))
        {
            let options = super::operators::options_of(public, &candidates);
            for argument in &given {
                if let Key::Name(unknown) = &argument.key
                    && argument.key.public_parameter(public).is_none()
                    && !options.accepts_name(&argument.key)
                {
                    self.diagnostics.emit(error(
                        Category::Name,
                        codes::ARGUMENT_UNKNOWN,
                        argument.value.at,
                        format!("`{}` has no parameter named `{unknown}`", name.text()),
                        "EXPR-2",
                    ));
                    return None;
                }
            }
        }
        self.diagnostics.emit(error(
            Category::Type,
            codes::TYPE_MISMATCH,
            at,
            format!(
                "the function type does not select one candidate of `{}`",
                name.text()
            ),
            "FN-17",
        ));
        None
    }

    /// Resolve the public signature of an explicit operator reference
    /// from its written arguments and its receiving template (FN-17).
    fn operator_reference(
        &mut self,
        node: &SyntaxNode,
        name: &SyntaxToken,
        operator: OperatorRef,
        (template, given): (&FunctionType, &[Given]),
    ) -> Option<OperatorReference> {
        let module = &self.program.modules[operator.module];
        let Detail::Callable(public) = &module.details[operator.declaration] else {
            return None;
        };
        if !plain_inputs(public, template) {
            self.marked_function(node, public);
            return None;
        }
        let ordinary = public
            .parameters
            .iter()
            .filter(|input| !input.configuration);
        if !takes_input_count(template, ordinary.count())
            || public.result.is_some() != template.result.is_some()
            || public.reference != template.reference
        {
            let signature = function_type(public, template.value);
            self.template_mismatch(self.at(node), template, &signature);
            return None;
        }
        let ReferenceArguments {
            generics,
            defaulted,
            ..
        } = self.reference_arguments(
            node,
            name,
            (&module.declarations[operator.declaration], public),
            (template, Some(given)),
        )?;
        if generics
            .iter()
            .zip(&public.generics)
            .any(|(argument, generic)| {
                matches!(argument, Argument::Type(ty) if !whole(ty))
                    && !reference_result_decides(public, generic, template)
            })
        {
            self.unresolved_reference(node, name);
            return None;
        }
        let own: Vec<_> = public.generics.iter().map(|generic| generic.at).collect();
        let map: Vec<_> = own.iter().copied().zip(&generics).collect();
        let signature = function_type(&in_instance(public, &map), template.value);
        if !matches_template(template, &signature) {
            self.template_mismatch(self.at(node), template, &signature);
            return None;
        }
        let among = self.result_among(operator, public, &map, &own);
        Some(OperatorReference {
            signature,
            generics,
            defaulted,
            among,
        })
    }

    /// Resolve the generic arguments shared by function and operator
    /// references, with any written arguments taking precedence (FN-17).
    fn reference_arguments(
        &mut self,
        node: &SyntaxNode,
        name: &SyntaxToken,
        (declared, signature): (&Declaration, &Signature),
        (template, given): (&FunctionType, Option<&[Given]>),
    ) -> Option<ReferenceArguments> {
        let written = self.reference_written(node, name);
        let ExplicitGenerics {
            fixed: mut found,
            faulty,
            pack,
        } = self.explicit_generics(name, signature, written)?;
        if faulty {
            return None;
        }
        if let (Some(pattern), Some(actual)) = (&signature.result, &template.result) {
            infer(pattern, actual, &signature.generics, &mut found);
        }
        let ordinary = signature
            .parameters
            .iter()
            .filter(|input| !input.configuration);
        for (pattern, actual) in ordinary.zip(template.ordinary_parameters()) {
            infer(&pattern.ty, actual.ty(), &signature.generics, &mut found);
        }
        for given in given.unwrap_or_default() {
            if let Some(index) = callback_parameter(signature, &given.key) {
                infer(
                    &signature.parameters[index].ty,
                    given.ty(),
                    &signature.generics,
                    &mut found,
                );
            }
        }
        // GEN-2 and GEN-8: a generic value parameter nothing says
        // anything of takes the default declared by the callback.
        let defaulted: Vec<_> = found
            .iter()
            .enumerate()
            .filter_map(|(index, value)| value.is_none().then_some(index))
            .collect();
        // GEN-13: fit dependent public options once their type arguments
        // are resolved. An option the read pass already rejected needs
        // no further diagnostic (ERR-9).
        let public_options: Vec<_> = written
            .filter(|_| declared.kind == crate::program::DeclarationKind::Operator)
            .into_iter()
            .flat_map(|written| {
                signature
                    .generics
                    .iter()
                    .zip(&written.arguments)
                    .enumerate()
            })
            .filter_map(|(index, (generic, (at, argument)))| {
                let (GenericKind::Value(ty), Argument::Constant(value)) = (&generic.kind, argument)
                else {
                    return None;
                };
                if !names_parameter(ty) || matches!(value.value, ConstantValue::Unknown) {
                    return None;
                }
                self.node_at(*at).map(|node| (index, node))
            })
            .collect();
        let reported = self.diagnostics.len();
        self.generic_values(
            name,
            declared,
            &signature.generics,
            (&public_options, &defaulted),
            &mut found,
        );
        if self.diagnostics.len() != reported {
            return None;
        }
        let mut type_packs: Vec<_> = pack.into_iter().collect();
        for captured in super::inference::tuple_pack_bindings(
            &signature.generics,
            &mut found,
            &self.signature.generics,
        ) {
            if !self.check_tuple_pack(
                self.at(node),
                &signature.generics[captured.parameter],
                &captured,
            ) {
                return None;
            }
            // GEN-8: an explicit pack decides the referenced function's
            // type. Keep it through declared_target so the final FN-17
            // template check reports both complete function types.
            if !type_packs
                .iter()
                .any(|written| written.parameter == captured.parameter)
            {
                type_packs.push(captured);
            }
        }
        if template.named_pack.is_some() {
            // FN-8, GEN-8: the callback's own supplied entries determine
            // its type packs. A named pack with none supplied is empty.
            for parameter in &signature.parameters {
                let Some(index) = type_pack(parameter, &signature.generics) else {
                    continue;
                };
                if type_packs.iter().any(|pack| pack.parameter == index) {
                    continue;
                }
                let pack = match (parameter.pack, given) {
                    (Some(Pack::Positional), Some(given)) => {
                        let types = given
                            .iter()
                            .filter(|argument| {
                                matches!(argument.key, Key::Position(_))
                                    && argument.key.public_parameter(signature).is_none()
                            })
                            .map(|argument| argument.ty().clone())
                            .collect();
                        TypePackArgument::positional(index, types)
                    }
                    (Some(Pack::Named), given) => {
                        let entries = given
                            .unwrap_or_default()
                            .iter()
                            .filter_map(|argument| {
                                let name = argument.key.name()?;
                                argument.key.public_parameter(signature).is_none().then(|| {
                                    (
                                        crate::hir::PackKey::Name(name.to_string()),
                                        argument.ty().clone(),
                                    )
                                })
                            })
                            .collect();
                        TypePackArgument::keyed(index, entries)
                    }
                    _ => continue,
                };
                type_packs.push(pack);
            }
        }
        type_packs.sort_by_key(|pack| pack.parameter);
        for pack in &type_packs {
            // The sequence is separate from the generic slot (GEN-8).
            found[pack.parameter] = Some(Argument::Type(Type::Unknown));
        }
        if declared.kind == crate::program::DeclarationKind::Operator {
            for (generic, value) in signature.generics.iter().zip(&mut found) {
                if value.is_none() && reference_result_decides(signature, generic, template) {
                    *value = Some(Argument::Type(Type::Unknown));
                }
            }
        }
        let Some(generics) = found.into_iter().collect::<Option<Vec<_>>>() else {
            self.unresolved_reference(node, name);
            return None;
        };
        Some(ReferenceArguments {
            generics,
            type_packs,
            defaulted,
        })
    }

    fn reference_written(
        &mut self,
        node: &SyntaxNode,
        name: &SyntaxToken,
    ) -> Option<&'a CallArguments> {
        // The read pass uses the full name's start, including its module alias.
        let start = child(node, SyntaxKind::QualifiedName)
            .map_or(name.span().start, |qualified| qualified.span().start);
        self.call_arguments_at(start)
    }

    /// Neither the reference nor its receiving template supplies every generic.
    fn unresolved_reference(&mut self, node: &SyntaxNode, name: &SyntaxToken) {
        self.diagnostics.emit(
            error(
                Category::Type,
                codes::GENERIC_UNRESOLVED,
                self.at(node),
                format!(
                    "the function type does not determine every generic argument of `{}`",
                    name.text()
                ),
                "FN-17",
            )
            .with_note("supply the template's missing generic arguments explicitly"),
        );
    }

    fn template_mismatch(&mut self, at: Location, expected: &FunctionType, actual: &FunctionType) {
        self.diagnostics.emit(error(
            Category::Type,
            codes::TYPE_MISMATCH,
            at,
            format!(
                "the function is `{}`, and the parameter takes `{}`",
                Type::Function(Box::new(actual.clone())),
                Type::Function(Box::new(expected.clone())),
            ),
            "FN-17",
        ));
    }

    /// FN-18: a function type supplies no configuration.
    fn callback_configuration(&mut self, node: &SyntaxNode, parameter: &Parameter) {
        self.diagnostics.emit(error(
            Category::Type,
            codes::TYPE_MISMATCH,
            self.at(node),
            format!(
                "the configuration parameter `{}` has no default",
                parameter.name
            ),
            "FN-18",
        ));
    }

    /// FN-17's proposed template rule does not admit marked fixed inputs.
    fn marked_function(&mut self, node: &SyntaxNode, signature: &Signature) {
        self.diagnostics.emit(error(
            Category::Type,
            codes::TYPE_MISMATCH,
            self.at(node),
            format!("a fixed input of the function type takes a plain parameter, but `{signature}` has a marked parameter or pack there"),
            "FN-17",
        ));
    }

    /// FN-4 and FN-17: the temporal template lets a callback compose.
    /// Determine whether the callback uses a connection or a temporal
    /// call, even when its committed read will report another error.
    pub(super) fn anonymous_pair_category(
        &mut self,
        node: &SyntaxNode,
        template: &FunctionType,
    ) -> ValueCategory {
        let mark = self.mark();
        let read = self.anonymous_target(node, template, true);
        self.take_back(mark, false);
        let Some((Callee::Anonymous(function), _)) = read else {
            return ValueCategory::Value;
        };
        let captured = function
            .captures
            .iter()
            .map(|&local| self.locals[local].category)
            .fold(ValueCategory::Value, super::lifting::join_categories);
        let mut temporal = captured == ValueCategory::Connection;
        super::forwarded_packs::walk_nested_calls(&function.body.statements, &mut |value| {
            if let ExprKind::Call { callee, .. } = &value.kind {
                temporal |= matches!(callee, Callee::Parameter { temporal: true, .. })
                    || callee.declaration().is_some_and(|(module, declaration)| {
                        self.program.modules[module].declarations[declaration]
                            .kind
                            .role()
                            == Some(Role::Temporal)
                    });
            }
        });
        if temporal {
            ValueCategory::Connection
        } else {
            captured
        }
    }

    fn anonymous_target(
        &mut self,
        node: &SyntaxNode,
        template: &FunctionType,
        probing_role: bool,
    ) -> Option<(Callee, FunctionType)> {
        let mut parameters = self.anonymous_parameters(node);
        if !takes_input_count(template, parameters.len()) {
            let count = template.parameters.len();
            let at_least = if template.named_pack.is_some() {
                "at least "
            } else {
                ""
            };
            self.diagnostics.emit(error(
                Category::Type,
                codes::TYPE_MISMATCH,
                self.at(node),
                format!(
                    "the function type takes {at_least}{count} inputs, and the anonymous function declares {}",
                    parameters.len()
                ),
                "FN-17",
            ));
            return None;
        }
        for ((_, written), expected) in parameters.iter_mut().zip(&template.parameters) {
            written.get_or_insert_with(|| expected.ty().clone());
        }
        // FN-17: a reflected pack gives no type to an unwritten
        // anonymous input beyond the template's fixed parameters.
        if let Some((name, _)) = parameters
            .iter()
            .skip(template.parameters.len())
            .find(|(_, ty)| ty.is_none())
        {
            self.diagnostics.emit(error(
                Category::Type,
                codes::GENERIC_UNRESOLVED,
                Location::new(self.file, name.span()),
                format!("the function type gives no type to `{}`", name.text()),
                "FN-17",
            ));
            return None;
        }
        let written = crate::syntax::child(node, SyntaxKind::ReturnType);
        let infer_result = written.is_none() && template.result_admits_none;
        // FN-17: reading the body to infer this result can infer
        // another callback result. Charge the reading before entering it.
        if infer_result && !self.may_reread() {
            return None;
        }
        let result = written
            .and_then(|written| self.anonymous_type(&written))
            .or_else(|| {
                if infer_result {
                    Some(Type::Unknown)
                } else {
                    template.result.clone()
                }
            });
        let mut actual = FunctionType {
            value: template.value,
            parameters: parameters
                .iter()
                .map(|(name, ty)| FunctionParameter::Fixed {
                    name: Some(name.text().to_string()),
                    ty: ty.clone().unwrap_or(Type::Unknown),
                })
                .collect(),
            named_pack: None,
            result: result.clone(),
            result_admits_none: false,
            reference: false,
        };
        let graph = std::mem::replace(&mut self.graph, !template.value);
        let value_code = std::mem::replace(&mut self.value_code, false);
        // A value callback written in a hook reads captures under that
        // hook's admission. A temporal callback composes its own graph.
        let outer_hook = self.hook;
        if !template.value {
            self.hook = None;
        }
        // FN-4: during a role probe the template supplies the types
        // and lets the body compose. Its own parameters do not make a
        // call temporal merely because this template is temporal.
        let definition = self.anonymous_definition(
            node,
            parameters,
            (result.as_ref(), infer_result),
            !template.value && !probing_role,
        );
        self.hook = outer_hook;
        self.value_code = value_code;
        self.graph = graph;
        let super::anonymous::AnonymousDefinition {
            mut body,
            captures,
            pack_captures,
        } = definition?;
        super::record(self.program, &mut body, !template.value);
        if result.as_ref().is_some_and(|ty| !whole(ty)) {
            // FN-17 and GEN-2: known inputs can settle the body's
            // result, even if the enclosing call still needs that type.
            let mut inferred: Option<Expr> = None;
            walk_statement_nodes(&body.statements, &mut |node| {
                if let Node::Statement(Stmt::Return {
                    value: Some(value), ..
                }) = node
                {
                    if let Some(earlier) = &mut inferred {
                        let mut value = value.clone();
                        if is_integer_literal(&value) && accepts_float(&earlier.ty) {
                            value = self.fit(value, &earlier.ty, "the function's result");
                        } else if is_integer_literal(earlier) && accepts_float(&value.ty) {
                            *earlier =
                                self.fit(earlier.clone(), &value.ty, "the function's result");
                        }
                        earlier.ty = self.one_type(earlier, &mut value);
                    } else {
                        inferred = Some(value.clone());
                    }
                }
            });
            if let Some(inferred) = inferred {
                actual.result = Some(inferred.ty);
            } else if infer_result {
                actual.result = None;
            }
        }
        let callee = Callee::Anonymous(Box::new(AnonymousFunction {
            at: self.at(node),
            template: actual.clone(),
            body,
            captures,
            pack_captures,
            specialization: AnonymousSpecialization {
                pack_pass_keys: self
                    .pack_passes
                    .iter()
                    .map(|(_, key)| key.clone())
                    .collect(),
                ..Default::default()
            },
        }));
        Some((callee, actual))
    }

    /// FN-17: call the statically known function through its template.
    pub(super) fn call_function_parameter(
        &mut self,
        name: &SyntaxToken,
        list: &SyntaxNode,
        index: usize,
        template: &FunctionType,
    ) -> (ExprKind, Type) {
        let at = Location::new(self.file, name.span());
        if !self.graph && !template.value {
            self.diagnostics.emit(error(
                Category::FunctionKind,
                codes::CALL_ROLE,
                at,
                format!("the function parameter `{}` composes a graph", name.text()),
                "FN-17",
            ));
            return self.unread_call(list);
        }
        if let Some(call) = self.defer_pack_call(list, template.result.as_ref(), None) {
            return call;
        }
        if spread_operands(list).next().is_some()
            && let Some((_, target)) = self.pack_functions.iter().find(|(slot, _)| *slot == index)
        {
            return self.call_forwarded_parameter(name, list, target.clone(), template);
        }
        let slots: Vec<_> = template
            .parameters
            .iter()
            .map(|input| Slot {
                configuration: false,
                name: input.name().unwrap_or(""),
                ty: input.ty().clone(),
                required: !matches!(input.ty(), Type::Nullable(_)),
                pack: None,
                pack_bounds: (0, None),
                type_pack: None,
                listed: template.value,
            })
            .collect();
        let arguments = self.bind(name, list, &slots, Takes::Parameters);
        (
            ExprKind::Call {
                callee: Callee::Parameter {
                    parameter: index,
                    temporal: !template.value,
                },
                arguments,
                form: Form::Named,
                text: None,
                braced: Vec::new(),
                lifted: false,
            },
            template.result.clone().unwrap_or(Type::Unknown),
        )
    }

    /// A spread can give slots reflected from the selected callback (FN-19).
    /// Fixed slots take their names from the template (FN-17).
    fn call_forwarded_parameter(
        &mut self,
        name: &SyntaxToken,
        list: &SyntaxNode,
        target: Callee,
        template: &FunctionType,
    ) -> (ExprKind, Type) {
        let names = |text: &str| {
            template
                .parameters
                .iter()
                .any(|input| input.name() == Some(text))
        };
        let unknown = |text: &str, at| {
            error(
                Category::Name,
                codes::ARGUMENT_UNKNOWN,
                at,
                format!(
                    "`{}` has no named parameter `{text}` in its function type",
                    name.text()
                ),
                "FN-17",
            )
        };
        let before = self.diagnostics.len();
        for (_, given, _) in self.call_entries(list) {
            if let Some(given) = given
                && !names(given.text())
            {
                self.diagnostics.emit(unknown(
                    given.text(),
                    Location::new(self.file, given.span()),
                ));
            }
        }
        if self.diagnostics.len() != before {
            return (ExprKind::Unread, Type::Unknown);
        }
        if template.named_pack.is_none()
            && let Some((key, at)) = self
                .forwarded_arguments(list)
                .into_iter()
                .find_map(|given| match given.key {
                    crate::hir::PackKey::Name(key) if !names(&key) => Some((key, given.value.at)),
                    _ => None,
                })
        {
            self.diagnostics.emit(unknown(&key, at));
            return (ExprKind::Unread, Type::Unknown);
        }
        let program = self.program;
        let (signature, value) = if let Some((module, declaration)) = target.declaration() {
            let owner = &program.modules[module];
            let Detail::Callable(declared) = &owner.details[declaration] else {
                return self.unread_call(list);
            };
            let mut signature = declared.clone();
            if let Callee::Candidate { operator, .. } = &target
                && let Detail::Callable(public) = &program.modules[operator.0].details[operator.1]
            {
                for (source, index) in public
                    .parameters
                    .iter()
                    .zip(public.candidate_parameters(declared))
                {
                    if let Some(index) = index {
                        signature.parameters[index].name.clone_from(&source.name);
                        signature.parameters[index].has_default |= source.has_default;
                    }
                }
            }
            (
                signature,
                owner.declarations[declaration].kind.role() == Some(Role::Value),
            )
        } else if let Callee::Anonymous(function) = &target {
            let signature = Signature {
                generics: Vec::new(),
                parameters: function
                    .template
                    .parameters
                    .iter()
                    .enumerate()
                    .map(|(index, input)| {
                        let local = &function.body.locals[index];
                        Parameter {
                            name: local.name.clone(),
                            at: local.at,
                            ty: input.ty().clone(),
                            configuration: false,
                            constant: false,
                            var: false,
                            reference: false,
                            pack: None,
                            pack_bounds: (0, None),
                            has_default: false,
                        }
                    })
                    .collect(),
                result: function.template.result.clone(),
                reference: function.template.reference,
            };
            (signature, function.template.value)
        } else {
            return self.unread_call(list);
        };
        let map: Vec<_> = signature
            .generics
            .iter()
            .map(|generic| generic.at)
            .zip(target.generics())
            .collect();
        let packs = PackTypes::new(&signature.generics, target.type_packs());
        let project = |ty: &Type| packs.project(ty, &map).unwrap_or(Type::Unknown);
        let mut fixed = template.parameters.iter();
        let slots: Vec<_> = signature
            .parameters
            .iter()
            .map(|parameter| {
                let type_pack = type_pack(parameter, &signature.generics);
                let written = type_pack.and_then(|index| {
                    target
                        .type_packs()
                        .iter()
                        .find(|pack| pack.parameter == index)
                });
                let name = if !parameter.configuration
                    && let Some(input) = fixed.next()
                {
                    input.name().unwrap_or("")
                } else {
                    &parameter.name
                };
                Slot {
                    configuration: parameter.configuration,
                    name,
                    ty: project(&parameter.ty),
                    required: parameter.var
                        || !parameter.has_default && !matches!(parameter.ty, Type::Nullable(_)),
                    pack: parameter.pack,
                    pack_bounds: pack_bounds(parameter, &signature.generics, written),
                    type_pack,
                    listed: value,
                }
            })
            .collect();
        let assigned = self.assign(name, list, &slots, Takes::Parameters, &[]);
        let arguments = assigned
            .into_iter()
            .map(|argument| {
                let slot = argument.slot.map(|index| {
                    let slot = &slots[index];
                    let packed = argument.pack.as_ref().and_then(|(position, key)| {
                        let index = slot.type_pack?;
                        let pack = target
                            .type_packs()
                            .iter()
                            .find(|pack| pack.parameter == index)?;
                        let index = pack
                            .keys
                            .iter()
                            .position(|given| given == key)
                            .unwrap_or(*position);
                        pack.types.get(index)
                    });
                    Slot {
                        ty: packed.cloned().unwrap_or_else(|| slot.ty.clone()),
                        ..*slot
                    }
                });
                let mut value = match argument.forwarded {
                    Some((_, value)) => {
                        let value = if let Some(slot) = &slot {
                            self.fit_argument(value, slot, false, Takes::Parameters)
                        } else {
                            value
                        };
                        crate::hir::Argument {
                            parameter: argument.slot,
                            pack: None,
                            value,
                        }
                    }
                    None => self.argument(
                        argument.slot,
                        &argument.value,
                        slot.as_ref().map(|slot| (slot, false)),
                        Takes::Parameters,
                    ),
                };
                value.pack = argument.pack.map(|(_, key)| key);
                value
            })
            .collect();
        let result = signature.result.as_ref().map_or(Type::Unknown, project);
        let mut kind = ExprKind::Call {
            callee: target,
            arguments,
            form: Form::Named,
            text: None,
            braced: Vec::new(),
            lifted: false,
        };
        self.finished_call(&mut kind, false);
        (kind, result)
    }

    /// A static function can be invoked or passed onward (FN-17).
    /// When an entry's calls are deferred, their operand reader will
    /// finish checking a name with either role.
    pub(super) fn reject_function_value(
        &mut self,
        local: LocalId,
        at: Location,
        deferred: Option<&SyntaxNode>,
    ) -> bool {
        if !matches!(self.locals[local].ty, Type::Function(_))
            || deferred.is_some_and(|node| self.is_call_operand(node))
        {
            return false;
        }
        self.diagnostics.emit(local_error(
            Category::Type,
            codes::local::FUNCTION_VALUE,
            at,
            "a function parameter can be invoked or passed to another function parameter"
                .to_string(),
            "FN-17",
        ));
        true
    }
}

/// The public operator signature and its resolved generic arguments (FN-17).
struct OperatorReference {
    signature: FunctionType,
    generics: Vec<Argument>,
    defaulted: Vec<usize>,
    among: Option<Vec<Type>>,
}

/// Generic arguments from a function reference before candidate selection (FN-17).
struct ReferenceArguments {
    generics: Vec<Argument>,
    type_packs: Vec<TypePackArgument>,
    defaulted: Vec<usize>,
}

/// The arguments the enclosing call gives the callback's own parameters.
#[derive(Default)]
pub(super) struct CallbackInputs<'a> {
    /// A dependent pack's entries in written order, including an empty pack.
    pub(super) supplied: Option<&'a [(Option<&'a str>, &'a Expr)]>,
    /// Parameters the enclosing declaration will give the callback (FN-8).
    pub(super) excluded: &'a [&'a str],
    /// The enclosing declaration's positive memberships of a supplied argument type.
    pub(super) evidence: Option<&'a CallbackEvidence>,
}

fn reflected_arguments(
    typer: &mut Typer<'_>,
    public: &Signature,
    template: &FunctionType,
    node: &SyntaxNode,
    inputs: &CallbackInputs<'_>,
) -> Result<(Vec<Given>, bool), ()> {
    let evidence = inputs
        .evidence
        .filter(|_| node.kind() == SyntaxKind::NameExpr);
    let fixed = public.positional_parameters().count();
    let mut remaining = public
        .positional_parameters()
        .map(|(_, parameter)| parameter)
        .enumerate()
        .filter(|(_, parameter)| !inputs.excluded.contains(&parameter.name.as_str()));
    let mut pack_position = fixed;
    let mut projected = false;
    let mut given: Vec<_> = inputs
        .supplied
        .unwrap_or_default()
        .iter()
        .map(|(name, value)| {
            let key = if let Some(name) = name {
                Key::Name((*name).to_string())
            } else if let Some((position, _)) = remaining.next() {
                Key::Position(position)
            } else {
                let position = pack_position;
                pack_position += 1;
                Key::Position(position)
            };
            let ty = if let Some(evidence) = evidence {
                let slot = key.public_parameter(public);
                let name = key
                    .name()
                    .or_else(|| slot.map(|slot| public.parameters[slot].name.as_str()));
                let position = match key {
                    Key::Position(position) => position.checked_sub(fixed),
                    _ => None,
                };
                let settled = slot.is_some_and(|slot| {
                    let before = public.parameters[..slot]
                        .iter()
                        .filter(|parameter| !parameter.configuration)
                        .count();
                    template
                        .ordinary_parameters()
                        .nth(before)
                        .is_some_and(|parameter| whole(parameter.ty()))
                });
                if settled {
                    value.ty.clone()
                } else {
                    evidence.resolve(name, position, &value.ty).map_err(|()| {
                        typer.diagnostics.emit(error(
                            Category::Type,
                            codes::GENERIC_UNRESOLVED,
                            value.at,
                            "this argument does not determine a unique type for the bare function"
                                .to_string(),
                            "GEN-3",
                        ));
                    })?
                }
            } else {
                value.ty.clone()
            };
            projected |= ty != value.ty;
            let mut argument = callback_operand(0, node, value.at, ty);
            argument.key = key;
            Ok(argument)
        })
        .collect::<Result<_, _>>()?;
    // Fixed inputs specify their own types even where the enclosing
    // declaration supplies them or the callback has a default (FN-17).
    for (index, input) in template.ordinary_parameters().enumerate() {
        if whole(input.ty())
            && let Some(slot) = Key::Position(index).public_parameter(public)
            && given
                .iter()
                .all(|given| given.key.public_parameter(public) != Some(slot))
        {
            given.push(callback_operand(
                index,
                node,
                typer.at(node),
                input.ty().clone(),
            ));
        }
    }
    Ok((given, projected))
}

fn callback_operand(position: usize, node: &SyntaxNode, at: Location, ty: Type) -> Given {
    Given::read(
        position,
        node.clone(),
        Expr {
            kind: ExprKind::Unread,
            ty,
            at,
            category: ValueCategory::Value,
        },
    )
}

/// FN-8 and GEN-2: a homogeneous pack's entries each supply evidence
/// for its element type.
fn callback_parameter(signature: &Signature, key: &Key) -> Option<usize> {
    key.public_parameter(signature).or_else(|| {
        let kind = match key {
            Key::Position(_) => Pack::Positional,
            Key::Name(_) => Pack::Named,
            _ => return None,
        };
        signature.parameters.iter().position(|parameter| {
            parameter.pack == Some(kind) && type_pack(parameter, &signature.generics).is_none()
        })
    })
}

/// FN-17: fixed inputs are plain. GEN-17 reflects the other inputs as
/// they are, including their marks and packs.
fn plain_inputs(signature: &Signature, template: &FunctionType) -> bool {
    let fixed = if template.named_pack.is_some() {
        template.parameters.len()
    } else {
        signature.parameters.len()
    };
    signature
        .parameters
        .iter()
        .filter(|parameter| !parameter.configuration)
        .take(fixed)
        .all(plain_parameter)
}

/// A candidate can take its operator's configuration default (GEN-6).
fn missing_callback_configuration<'a>(
    signature: &'a Signature,
    public: Option<&Signature>,
) -> Option<&'a Parameter> {
    signature
        .parameters
        .iter()
        .enumerate()
        .find(|(index, parameter)| {
            parameter.configuration
                && !parameter.has_default
                && !public.is_some_and(|public| {
                    public
                        .parameters
                        .iter()
                        .zip(public.candidate_parameters(signature))
                        .any(|(input, slot)| slot == Some(*index) && input.has_default)
                })
        })
        .map(|(_, parameter)| parameter)
}

fn plain_parameter(parameter: &Parameter) -> bool {
    !parameter.var && !parameter.constant && !parameter.reference && parameter.pack.is_none()
}

/// A reflected pack takes the remaining slots after the fixed prefix.
fn takes_input_count(template: &FunctionType, count: usize) -> bool {
    let fixed = template.parameters.len();
    count == fixed || (template.named_pack.is_some() && count > fixed)
}

fn function_type(signature: &Signature, value: bool) -> FunctionType {
    FunctionType {
        value,
        parameters: signature
            .parameters
            .iter()
            .cloned()
            .map(FunctionParameter::Declared)
            .collect(),
        named_pack: None,
        result: signature.result.clone(),
        result_admits_none: matches!(signature.result.as_ref(),
            Some(Type::Parameter { at, .. }) if signature.generics.iter().any(|generic| generic.at == *at && generic.result_only)),
        reference: signature.reference,
    }
}

/// An explicit operator reference can learn a result-only generic from
/// the selected candidate when its receiving result is open (FN-17).
fn reference_result_decides(
    signature: &Signature,
    generic: &GenericParameter,
    template: &FunctionType,
) -> bool {
    let occurs = |ty: &Type| mentions(ty, &[generic.at]);
    matches!(generic.kind, GenericKind::Type)
        && template.result.as_ref().is_some_and(|ty| !whole(ty))
        && signature.result.as_ref().is_some_and(occurs)
        && !signature.parameters.iter().any(|input| occurs(&input.ty))
}

/// FN-17's fixed input types, required names and phase.
pub(super) fn matches_template(expected: &FunctionType, actual: &FunctionType) -> bool {
    (!expected.value || actual.value)
        && expected.reference == actual.reference
        && takes_input_count(expected, actual.ordinary_parameters().count())
        && (actual.named_pack.is_none() || expected.named_pack.is_some())
        && expected
            .ordinary_parameters()
            .zip(actual.ordinary_parameters())
            .all(|(expected, actual)| {
                actual.declared().is_none_or(plain_parameter)
                    && expected
                        .name()
                        .is_none_or(|name| Some(name) == actual.name())
                    && agree(expected.ty(), actual.ty())
            })
        && match (&expected.result, &actual.result) {
            (Some(expected), Some(actual)) => agree(expected, actual),
            (None, None) => true,
            (Some(_), None) => expected.result_admits_none,
            _ => false,
        }
}
