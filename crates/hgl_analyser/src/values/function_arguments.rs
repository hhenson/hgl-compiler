//! Checks FN-17 templates and records the static target of a function argument.

use hgl_diagnostics::{Category, Location};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};

use super::Typer;
use super::arguments::{Slot, Takes};
use super::candidates::{
    CallbackOption, CallbackOptionSource, CallbackRequest, Given, Wanted, in_instance,
};
use super::constants::generic_default;
use super::fit::whole;
use super::inference::{ExplicitGenerics, infer};
use crate::declared::{agree, names_parameter, substitute_within};
use crate::hir::{
    AnonymousFunction, Callee, Expr, ExprKind, Form, TypePackArgument, ValueCategory,
};
use crate::program::{Declaration, OperatorRef, Role};
use crate::syntax::{child, name_token, ungrouped};
use crate::types::{
    Argument, CallArguments, ConstantValue, Detail, FunctionType, GenericKind, Signature, Type,
};
use crate::{codes, error, local_error};

impl<'a> Typer<'a> {
    /// Read an argument the callee expects to be a function known at compile time.
    pub(super) fn function_argument(&mut self, node: &SyntaxNode, template: &FunctionType) -> Expr {
        let at = self.at(node);
        let argument = ungrouped(node.clone());
        let resolved = self.function_target(&argument, template);
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
                        template,
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
        if !plain_parameters(signature) {
            self.marked_function(node, signature);
            return None;
        }
        let ReferenceArguments { generics, pack, .. } =
            self.reference_arguments(node, name, (declared, signature), template)?;
        let map = signature
            .generics
            .iter()
            .map(|generic| generic.at)
            .zip(&generics)
            .collect::<Vec<_>>();
        let taken = in_instance(signature, &map);
        let actual = function_type(&taken, declared.kind.role() == Some(Role::Value));
        if matches_template(template, &actual) && template.types().all(whole) {
            self.requires_met(self.at(node), (module, declaration), &generics);
        }
        let requirements = self.answers(module, declaration, &generics);
        let callee = Callee::Function {
            module,
            declaration,
            generics,
            type_packs: pack.into_iter().collect(),
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
        template: &FunctionType,
    ) -> Option<(Callee, FunctionType)> {
        // Bare references use the existing template-driven selector.
        // With written arguments, the operator can also complete a
        // template still open after the other arguments were read.
        let explicit = if node.kind() == SyntaxKind::FunctionReference {
            Some(self.operator_reference(node, name, operator, template)?)
        } else {
            if !template.types().all(whole) {
                return None;
            }
            None
        };
        let request = explicit
            .as_ref()
            .map_or(template, |operator| &operator.signature);
        let graph = std::mem::replace(&mut self.graph, !template.value);
        let value_code = std::mem::replace(&mut self.value_code, false);
        let candidates = self.value_candidates(operator);
        let candidates: Vec<_> = candidates
            .into_iter()
            .filter(|(_, _, signature)| plain_parameters(signature))
            .collect();
        let at = self.at(node);
        let given = request
            .parameters
            .iter()
            .enumerate()
            .map(|(index, (_, ty))| {
                Given::read(
                    index,
                    node.clone(),
                    Expr {
                        kind: ExprKind::Unread,
                        ty: ty.clone(),
                        at,
                        category: ValueCategory::Value,
                    },
                )
            })
            .collect::<Vec<_>>();
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
            callback: explicit.as_ref().map(|_| CallbackRequest {
                reference: request.reference,
                values: &requested_values,
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
            if let Some(Detail::Callable(public)) = self.program.modules[operator.module]
                .details
                .get(operator.declaration)
            {
                for ((name, _), public) in actual.parameters.iter_mut().zip(&public.parameters) {
                    *name = Some(public.name.clone());
                }
            }
            let requirements = self.answers(module, declaration, &chosen.generics);
            let callee = Callee::Candidate {
                operator: (operator.module, operator.declaration),
                operator_generics: explicit.map_or_else(Vec::new, |operator| operator.generics),
                module,
                declaration,
                generics: chosen.generics,
                type_packs: Vec::new(),
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
        template: &FunctionType,
    ) -> Option<OperatorReference> {
        let module = &self.program.modules[operator.module];
        let Detail::Callable(public) = &module.details[operator.declaration] else {
            return None;
        };
        if !plain_parameters(public) {
            self.marked_function(node, public);
            return None;
        }
        if public.parameters.len() != template.parameters.len()
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
            template,
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
        template: &FunctionType,
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
        for (pattern, (_, actual)) in signature.parameters.iter().zip(&template.parameters) {
            infer(&pattern.ty, actual, &signature.generics, &mut found);
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
        let Some(generics) = found.into_iter().collect::<Option<Vec<_>>>() else {
            self.unresolved_reference(node, name);
            return None;
        };
        Some(ReferenceArguments {
            generics,
            pack,
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
            format!("a function type takes plain parameters, but `{signature}` has a marked parameter or pack"),
            "FN-17",
        ));
    }

    fn anonymous_target(
        &mut self,
        node: &SyntaxNode,
        template: &FunctionType,
    ) -> Option<(Callee, FunctionType)> {
        let mut parameters = self.anonymous_parameters(node);
        if parameters.len() != template.parameters.len() {
            self.diagnostics.emit(error(
                Category::Type,
                codes::TYPE_MISMATCH,
                self.at(node),
                format!(
                    "the function type takes {} inputs, and the anonymous function declares {}",
                    template.parameters.len(),
                    parameters.len()
                ),
                "FN-17",
            ));
            return None;
        }
        for ((_, written), (_, expected)) in parameters.iter_mut().zip(&template.parameters) {
            written.get_or_insert_with(|| expected.clone());
        }
        let result = crate::syntax::child(node, SyntaxKind::ReturnType)
            .and_then(|written| self.anonymous_type(&written))
            .or_else(|| template.result.clone());
        let actual = FunctionType {
            value: template.value,
            parameters: parameters
                .iter()
                .map(|(name, ty)| {
                    (
                        Some(name.text().to_string()),
                        ty.clone().unwrap_or(Type::Unknown),
                    )
                })
                .collect(),
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
            .map(|(name, ty)| Slot {
                configuration: false,
                name: name.as_deref().unwrap_or(""),
                ty: ty.clone(),
                required: !matches!(ty, Type::Nullable(_)),
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
    defaulted: Vec<usize>,
}

fn plain_parameters(signature: &Signature) -> bool {
    signature.parameters.iter().all(|parameter| {
        !parameter.var && !parameter.constant && !parameter.reference && parameter.pack.is_none()
    })
}

fn function_type(signature: &Signature, value: bool) -> FunctionType {
    FunctionType {
        value,
        parameters: signature
            .parameters
            .iter()
            .map(|parameter| (Some(parameter.name.clone()), parameter.ty.clone()))
            .collect(),
        result: signature.result.clone(),
        reference: signature.reference,
    }
}

/// FN-17's fixed input types, required names and phase.
pub(super) fn matches_template(expected: &FunctionType, actual: &FunctionType) -> bool {
    (!expected.value || actual.value)
        && expected.reference == actual.reference
        && expected.parameters.len() == actual.parameters.len()
        && expected.parameters.iter().zip(&actual.parameters).all(
            |((name, ty), (actual_name, actual_ty))| {
                (name.is_none() || name == actual_name) && agree(ty, actual_ty)
            },
        )
        && match (&expected.result, &actual.result) {
            (Some(expected), Some(actual)) => agree(expected, actual),
            (None, None) => true,
            _ => false,
        }
}
