//! Checks FN-17 templates and records the static target of a function argument.

use hgl_diagnostics::{Category, Location};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};

use super::Typer;
use super::arguments::{Slot, Takes};
use super::candidates::{
    CallbackOption, CallbackOptionSource, CallbackRequest, Given, Key, Wanted, in_instance,
};
use super::constants::generic_default;
use super::fit::whole;
use super::inference::{ExplicitGenerics, infer, type_pack};
use crate::declared::{agree, names_parameter, substitute_within};
use crate::hir::{
    AnonymousFunction, Callee, Expr, ExprKind, Form, TypePackArgument, ValueCategory,
};
use crate::program::{Declaration, OperatorRef, Role};
use crate::syntax::{child, name_token, ungrouped};
use crate::types::{
    Argument, CallArguments, ConstantValue, Detail, FunctionParameter, FunctionType, GenericKind,
    Pack, Parameter, Signature, Type,
};
use crate::{codes, error, local_error};

impl<'a> Typer<'a> {
    /// Read an argument the callee expects to be a function known at compile time.
    pub(super) fn function_argument(&mut self, node: &SyntaxNode, template: &FunctionType) -> Expr {
        self.function_argument_with_inputs(node, template, &CallbackInputs::default())
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
            return self.anonymous_target(node, template);
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
                    return self.declared_target(node, &name, (module, declaration), template);
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
        template: &FunctionType,
    ) -> Option<(Callee, FunctionType)> {
        let program = self.program;
        let unit = &program.modules[module];
        let Detail::Callable(signature) = unit.details.get(declaration)? else {
            return None;
        };
        let declared = &unit.declarations[declaration];
        if !plain_inputs(signature, template) {
            self.marked_function(node, signature);
            return None;
        }
        let ReferenceArguments {
            generics,
            pack,
            empty_named_pack,
            ..
        } = self.reference_arguments(node, name, (declared, signature), (template, &[]))?;
        let map = signature
            .generics
            .iter()
            .zip(&generics)
            .filter(|(generic, _)| !matches!(generic.kind, GenericKind::Pack { .. }))
            .map(|(generic, argument)| (generic.at, argument))
            .collect::<Vec<_>>();
        let taken = in_instance(signature, &map);
        let actual = function_type(&taken, declared.kind.role() == Some(Role::Value));
        if pack.is_none() && matches_template(template, &actual) && template.types().all(whole) {
            self.requires_met(self.at(node), (module, declaration), &generics);
        }
        let requirements = if pack.is_none() {
            self.answers(module, declaration, &generics)
        } else {
            Vec::new()
        };
        let mut type_packs: Vec<_> = pack.into_iter().chain(empty_named_pack).collect();
        type_packs.sort_by_key(|pack| pack.parameter);
        let callee = Callee::Function {
            module,
            declaration,
            generics,
            type_packs,
            requirements,
            contextual: Vec::new(),
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
        let at = self.at(node);
        let reflected = template.named_pack.is_some();
        let mut given = if reflected {
            reflected_operator_arguments(public, template, node, at, inputs)
        } else {
            Vec::new()
        };
        if reflected && !given.iter().any(|argument| whole(argument.ty())) {
            self.diagnostics.emit(error(
                Category::Type,
                codes::GENERIC_UNRESOLVED,
                at,
                format!("the call gives no resolved parameter type to select a candidate of `{}`", name.text()),
                "FN-17",
            ));
            return None;
        }
        // With written generics the operator can also complete a template
        // still open after the enclosing call's other arguments were read.
        let explicit = if node.kind() == SyntaxKind::FunctionReference {
            Some(self.operator_reference(node, name, operator, (template, &given))?)
        } else {
            if !reflected && !template.types().all(whole) {
                return None;
            }
            None
        };
        let request = explicit
            .as_ref()
            .map_or(template, |operator| &operator.signature);
        if !reflected {
            given.extend(request.parameters.iter().enumerate().map(|(index, input)| {
                callback_operand(index, node, at, input.ty().clone())
            }));
        }
        let graph = std::mem::replace(&mut self.graph, !template.value);
        let value_code = std::mem::replace(&mut self.value_code, false);
        let candidates: Vec<_> = self
            .value_candidates(operator)
            .into_iter()
            .filter(|(_, _, signature)| plain_inputs(signature, template))
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
        let (mut matches, unknown) = self.admitted(operator, &candidates, &given, &wanted);
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
            for (parameter, position) in public.parameters.iter().zip(public.candidate_parameters(taken)) {
                if let Some(position) = position
                    && let FunctionParameter::Declared(input) = &mut actual.parameters[position]
                {
                    input.name.clone_from(&parameter.name);
                    input.has_default |= parameter.has_default;
                }
            }
            let requirements = self.answers(module, declaration, &chosen.generics);
            let callee = Callee::Candidate {
                operator: (operator.module, operator.declaration),
                operator_generics: explicit.map_or_else(Vec::new, |operator| operator.generics),
                module,
                declaration,
                generics: chosen.generics,
                type_packs: chosen.type_packs,
                requirements,
            };
            return Some((callee, actual));
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
        if !takes_input_count(template, public.parameters.len())
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
            (template, given),
        )?;
        if generics
            .iter()
            .any(|argument| matches!(argument, Argument::Type(ty) if !whole(ty)))
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
        (template, given): (&FunctionType, &[Given]),
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
        for (pattern, actual) in signature.parameters.iter().zip(&template.parameters) {
            infer(&pattern.ty, actual.ty(), &signature.generics, &mut found);
        }
        for given in given {
            if let Some(index) = given.key.public_parameter(signature) {
                infer(&signature.parameters[index].ty, given.ty(), &signature.generics, &mut found);
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
        // GEN-8: explicitly written positional pack types live on the
        // target as a sequence, with no single argument at its generic slot.
        if let Some(pack) = &pack {
            found[pack.parameter] = Some(Argument::Type(Type::Unknown));
        }
        // FN-8: an unapplied callback's own named pack contributes no
        // entries to a reflected template. Its shape is still declared.
        let empty_named_pack = template.named_pack.as_ref().and_then(|_| {
            let named = signature
                .parameters
                .iter()
                .find(|parameter| parameter.pack == Some(Pack::Named))?;
            let index = type_pack(named, &signature.generics)?;
            if pack.as_ref().is_some_and(|given| given.parameter == index) {
                return None;
            }
            found[index] = Some(Argument::Type(Type::Unknown));
            Some(TypePackArgument::positional(index, Vec::new()))
        });
        let Some(generics) = found.into_iter().collect::<Option<Vec<_>>>() else {
            self.unresolved_reference(node, name);
            return None;
        };
        Some(ReferenceArguments {
            generics,
            pack,
            empty_named_pack,
            defaulted,
        })
    }

    fn reference_written(
        &self,
        node: &SyntaxNode,
        name: &SyntaxToken,
    ) -> Option<&'a CallArguments> {
        // The read pass uses the full name's start, including its module alias.
        let start = child(node, SyntaxKind::QualifiedName)
            .map_or(name.span().start, |qualified| qualified.span().start);
        self.calls.get(&(self.file, start)).copied()
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

    fn anonymous_target(
        &mut self,
        node: &SyntaxNode,
        template: &FunctionType,
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
        let result = crate::syntax::child(node, SyntaxKind::ReturnType)
            .and_then(|written| self.anonymous_type(&written))
            .or_else(|| template.result.clone());
        let actual = FunctionType {
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
        let definition =
            self.anonymous_definition(node, parameters, result.as_ref(), !template.value);
        self.hook = outer_hook;
        self.value_code = value_code;
        self.graph = graph;
        let (mut body, captures) = definition?;
        super::record(self.program, &mut body, !template.value);
        let callee = Callee::Anonymous(Box::new(AnonymousFunction {
            at: self.at(node),
            template: actual.clone(),
            body,
            captures,
            specialization: Vec::new(),
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
                braced: Vec::new(),
                lifted: false,
            },
            template.result.clone().unwrap_or(Type::Unknown),
        )
    }

    /// A static function's body is reached from a call or by passing the
    /// parameter onward. There is no expression that reads its value.
    pub(super) fn function_value(&mut self, at: Location) {
        self.diagnostics.emit(local_error(
            Category::Type,
            codes::local::FUNCTION_VALUE,
            at,
            "a function parameter can be invoked or passed to another function parameter"
                .to_string(),
            "FN-17",
        ));
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
    pack: Option<TypePackArgument>,
    empty_named_pack: Option<TypePackArgument>,
    defaulted: Vec<usize>,
}

/// The arguments the enclosing call gives the callback's own parameters.
#[derive(Default)]
pub(super) struct CallbackInputs<'a> {
    pub(super) supplied: &'a [(Option<&'a str>, &'a Expr)],
    pub(super) excluded: &'a [&'a str],
}

fn reflected_operator_arguments(
    public: &Signature,
    template: &FunctionType,
    node: &SyntaxNode,
    at: Location,
    inputs: &CallbackInputs<'_>,
) -> Vec<Given> {
    let mut positions = public
        .parameters
        .iter()
        .filter(|parameter| !parameter.configuration && parameter.pack.is_none())
        .enumerate();
    let fixed = positions.clone().count();
    let mut remaining = positions
        .by_ref()
        .filter(|(_, parameter)| !inputs.excluded.contains(&parameter.name.as_str()));
    let mut pack_position = fixed;
    let mut given: Vec<_> = inputs
        .supplied
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
            let mut argument = callback_operand(0, node, value.at, value.ty.clone());
            argument.key = key;
            argument
        })
        .collect();
    // Fixed inputs specify their own types even where the enclosing
    // declaration supplies them or the callback has a default (FN-17).
    for (index, input) in template.parameters.iter().enumerate() {
        if whole(input.ty())
            && given.iter().all(|given| given.key.public_parameter(public) != Some(index))
        {
            given.push(callback_operand(index, node, at, input.ty().clone()));
        }
    }
    given
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

/// FN-17: fixed inputs are plain. GEN-17 reflects the other inputs as
/// they are, including their marks and packs.
fn plain_inputs(signature: &Signature, template: &FunctionType) -> bool {
    let fixed = if template.named_pack.is_some() {
        template.parameters.len()
    } else {
        signature.parameters.len()
    };
    signature.parameters.iter().take(fixed).all(plain_parameter)
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
        reference: signature.reference,
    }
}

/// FN-17's fixed input types, required names and phase.
pub(super) fn matches_template(expected: &FunctionType, actual: &FunctionType) -> bool {
    (!expected.value || actual.value)
        && expected.reference == actual.reference
        && takes_input_count(expected, actual.parameters.len())
        && (actual.named_pack.is_none() || expected.named_pack.is_some())
        && expected
            .parameters
            .iter()
            .zip(&actual.parameters)
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
            _ => false,
        }
}
