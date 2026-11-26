//! A generic declaration for the types of one call of it (GEN-2,
//! GEN-16).
//!
//! A generic declaration is typed once, with each generic parameter
//! standing for itself: the `T` of
//!
//! ```text
//! fn sum<T>(a: T, b: T) -> T requires add_(T, T) -> T => add_(a, b)
//! ```
//!
//! is the type of `a`, and the body's `add_(a, b)` is recorded as an
//! operation the declaration requires, with no candidate
//! ([`Callee::Required`]). A call says what `T` is there, and what
//! answers the requirement ([`Answer`]).
//!
//! A target with no generics of its own writes such a declaration once
//! for each set of types it is called with. [`Program::instance`] gives
//! it what to write: the signature and the body with each generic
//! parameter replaced by what it stands for at one call, and each
//! required operation replaced by the call of the candidate that
//! answers it there. `sum(1.5, 2.5)` gives
//!
//! ```text
//! fn sum(a: f64, b: f64) -> f64 => <core's add_(f64, f64)>(a, b)
//! ```
//!
//! The body was checked under its stated requirements (GEN-7), and
//! each call recorded the candidates that meet them: this puts the two
//! together. One call is read again: a call of an operator that a
//! membership the declaration states authorises, `requires T in {i64,
//! f64}`, is the call of the candidate it selects with the instance's
//! types (GEN-7: "Each instance then selects its own candidate";
//! [`crate::values::Selecting`]).

use std::cell::RefCell;

use hgl_diagnostics::{Diagnostics, Location};

use crate::declared::substitute_within;
use crate::hir::{
    AnonymousFunction, Answer, Body, Callee, ConfigurationRead, DefaultExpr, Expr, ExprKind,
    PackInput, TypePackArgument, Value, ValueCategory, walk_statements, walk_statements_mut,
};
use crate::program::{LocalKind, Phase, Program, Role};
use crate::types::{Argument, Constant, ConstantValue, Detail, Parameter, Scalar, Signature, Type};

/// Where each generic parameter of a declaration is declared, with what
/// it stands for at one call.
type Map<'a> = [(Location, &'a Argument)];

use crate::values::PackTypes;

/// The compile-time call inputs that specialize a declaration (FN-17, FN-19).
#[derive(Clone, Copy, Default)]
pub struct InstanceInputs<'a> {
    /// Each function supplied at an original parameter slot.
    pub functions: &'a [(usize, Callee)],
    /// Generic type and reflected packs of the declaration.
    pub type_packs: &'a [TypePackArgument],
    /// Concrete entries held by the caller's packs, including an empty shape.
    pub pack_inputs: Option<&'a [PackInput]>,
    /// Ordinary const inputs read by a compile-time check, by original slot.
    pub constants: &'a [crate::ConstInput],
    /// The active test's module when reading this callable for a test run (TST-1).
    pub test_candidates: Option<crate::TestCandidateScope>,
}

/// The enclosing declaration and the arguments common to its body and
/// any anonymous bodies written inside it.
struct InstanceArguments<'a> {
    program: &'a Program,
    declaration: (usize, usize),
    generics: &'a [Argument],
    types: &'a Map<'a>,
    packs: &'a PackTypes<'a>,
    type_packs: &'a [TypePackArgument],
    pack_inputs: &'a [PackInput],
    constants: &'a [crate::ConstInput],
    test_candidates: Option<crate::TestCandidateScope>,
    requirements: &'a [Answer],
    diagnostics: &'a RefCell<Diagnostics>,
    changed_results: RefCell<Vec<Location>>,
}

impl InstanceArguments<'_> {
    fn project(&self, ty: &Type) -> Type {
        if ty.any_part(
            &|part| matches!(part, Type::Parameter { .. } | Type::PackMember { .. }),
            &|constant| constant.value.key().is_none(),
        ) {
            self.program
                .resolve_instance_type(self.packs.project(ty, self.types).unwrap_or(Type::Unknown))
        } else {
            // A reread can already supply a concrete type. Substitution's
            // growth bound applies only when there is something to place.
            ty.clone()
        }
    }
}

/// A specialized callable, with any reports from rechecking the uses
/// of an operator result whose type changed (GEN-7).
pub(crate) struct CheckedInstance {
    /// The concrete signature used to read the body.
    pub(crate) signature: Signature,
    /// The body with its concrete types and callees.
    pub(crate) body: Body,
    /// The instance read the body from its syntax with concrete types.
    pub(crate) reread_body: bool,
    /// Pack-key branches selected by this body for folded dependencies (CTRL-16).
    pub(crate) selected_pack_branches: Vec<Location>,
    /// Checks made when a selected result differs from the admitting result.
    pub(crate) result_uses: Option<ResultUseChecks>,
}

/// The calls whose results changed and the checks their new types caused.
pub(crate) struct ResultUseChecks {
    /// Written locations of the calls that selected a different result.
    pub(crate) changed_results: Vec<Location>,
    /// Type checks from reading the body with the selected results.
    pub(crate) reports: Diagnostics,
}

impl CheckedInstance {
    /// Visit calls and references in the final body, including nested functions.
    pub(crate) fn walk_calls(&self, visit: &mut dyn FnMut(&crate::hir::Expr)) {
        crate::values::walk_body_calls(&self.body, visit);
    }
}

impl Program {
    /// The operator's configuration default inherited by a candidate's parameter
    /// at `slot` (GEN-6). Candidate-only configuration has its own default.
    pub fn inherited_configuration_default(
        &self,
        callee: &Callee,
        slot: usize,
    ) -> Option<&DefaultExpr> {
        let Callee::Candidate {
            operator,
            module,
            declaration,
            ..
        } = callee
        else {
            return None;
        };
        let owner = self.modules.get(operator.0)?;
        let Detail::Callable(public) = owner.details.get(operator.1)? else {
            return None;
        };
        let Detail::Callable(candidate) = self.modules.get(*module)?.details.get(*declaration)?
        else {
            return None;
        };
        let index = public
            .candidate_parameters(candidate)
            .position(|index| index == Some(slot))?;
        if !public.parameters[index].configuration {
            return None;
        }
        owner.defaults.get(operator.1)?.get(index)?.as_ref()
    }

    /// Places the operator's generic arguments in a checked configuration default.
    /// Its declaration references still name the operator's defining scope (UNIT-7).
    /// The selected configuration parameter gives the default its type (UNIT-8).
    pub fn specialize_operator_default(
        &self,
        default: &DefaultExpr,
        operator: (usize, usize),
        generics: &[Argument],
        type_packs: &[TypePackArgument],
        parameter: &Parameter,
        diagnostics: &mut Diagnostics,
    ) -> DefaultExpr {
        let packs = self.operator_default_packs(operator, generics, type_packs);
        self.specialize_parameter_default(
            default,
            operator,
            generics,
            &packs,
            parameter,
            diagnostics,
        )
    }

    /// GEN-6/10/17: inherited defaults use the public operator's packs,
    /// preserving reflected keys and falling back to positional tuple arguments.
    /// This applies even when the selected candidate has no generics.
    pub(crate) fn operator_default_packs(
        &self,
        operator: (usize, usize),
        generics: &[Argument],
        reflected: &[TypePackArgument],
    ) -> Vec<TypePackArgument> {
        let Detail::Callable(signature) = &self.modules[operator.0].details[operator.1] else {
            return Vec::new();
        };
        signature
            .generics
            .iter()
            .zip(generics)
            .enumerate()
            .filter_map(|(index, (generic, argument))| {
                if !matches!(generic.kind, crate::GenericKind::Pack { .. }) {
                    return None;
                }
                if let Some(pack) = reflected.iter().find(|pack| pack.parameter == index) {
                    return Some(pack.clone());
                }
                let Argument::Type(Type::Tuple(types)) = argument else {
                    return None;
                };
                Some(TypePackArgument::positional(index, types.clone()))
            })
            .collect()
    }

    /// Places a default in its defining callable's scope and fits it
    /// against the selected parameter type (UNIT-7, UNIT-8). A cold type
    /// query or pack-key guard receives the call's bound type packs
    /// (GEN-10, TYPE-18, CTRL-16).
    pub fn specialize_parameter_default(
        &self,
        default: &DefaultExpr,
        operator: (usize, usize),
        generics: &[Argument],
        type_packs: &[TypePackArgument],
        parameter: &Parameter,
        diagnostics: &mut Diagnostics,
    ) -> DefaultExpr {
        let Detail::Callable(signature) = &self.modules[operator.0].details[operator.1] else {
            return default.clone();
        };
        // UNIT-8, EXPR-5: a literal's declaration-time type is provisional
        // when the omitted parameter was generic. Read its syntax again
        // once the selected parameter supplies a concrete context.
        let reread = default.needs_contextual_specialization(&parameter.ty);
        let type_packs = if reread { type_packs } else { &[] };
        let types: Vec<_> = signature
            .generics
            .iter()
            .map(|generic| generic.at)
            .zip(generics)
            .collect();
        let mut uses = Vec::new();
        default.value.walk(&mut |value| match &value.kind {
            ExprKind::Call { callee, .. } => required_uses(callee, value.at, &mut uses),
            ExprKind::StaticCallable(callee) => required_uses(callee, value.at, &mut uses),
            _ => {}
        });
        let reports = RefCell::new(Diagnostics::new());
        let requirements = if uses.is_empty() {
            Vec::new()
        } else {
            crate::values::operator_default_answers(
                self,
                operator,
                generics,
                &uses,
                &mut reports.borrow_mut(),
            )
        };
        let packs = PackTypes::new(&signature.generics, type_packs);
        let instance = InstanceArguments {
            program: self,
            declaration: operator,
            generics,
            types: &types,
            packs: &packs,
            type_packs,
            pack_inputs: &[],
            constants: &[],
            test_candidates: None,
            requirements: &requirements,
            diagnostics: &reports,
            changed_results: RefCell::new(Vec::new()),
        };
        // UNIT-7: a default has no formal parameters in scope. Run the
        // same membership-based selection here as in a specialized body.
        let mut body = if reread && !generics.is_empty() {
            crate::values::reread_parameter_default(
                self,
                operator,
                default,
                crate::values::ParameterDefaultRead::Specialization {
                    arguments: generics,
                    type_packs,
                    project: &|ty| instance.project(ty),
                },
                &mut reports.borrow_mut(),
            )
            .unwrap_or_else(|| default.clone())
        } else {
            default.clone()
        }
        .into_body();
        self.place_body(&mut body, &instance, &[]);
        crate::values::record(self, &mut body, false);
        let mut default = DefaultExpr::from_body(body);
        let reports = reports.into_inner();
        for report in reports.as_slice() {
            if !diagnostics.as_slice().contains(report) {
                diagnostics.emit(report.clone());
            }
        }
        if !reports.has_errors() {
            default.value = crate::values::fit_operator_default(
                self,
                operator,
                default.value,
                parameter,
                diagnostics,
            );
        }
        default
    }

    /// The signature and the body of `declaration` of `module` as one
    /// call takes it: `None` for a declaration that is no function, or
    /// has no body.
    ///
    /// `generics` is what each generic parameter stands for, in the
    /// order declared, and `requirements` what answers each requirement
    /// the declaration states: both as a call records them
    /// ([`Callee::Function`], [`Callee::Candidate`]).
    ///
    /// A call written in a generic body records what names that body's
    /// own generic parameters, and requirements that body states
    /// ([`Answer::Stated`]). Those are given as the body's own instance
    /// has them: a call in the body this returns names no generic
    /// parameter of the declaration, and no requirement of it. What each
    /// expression is and which calls are lifted are recorded again for
    /// the instance ([`Expr::category`]): the generic body could not say
    /// what an operation it requires gives (GEN-7).
    pub fn instance(
        &self,
        module: usize,
        declaration: usize,
        generics: &[Argument],
        requirements: &[Answer],
    ) -> Option<(Signature, Body)> {
        self.instance_with_functions(module, declaration, generics, requirements, &[])
    }

    /// Instantiate a declaration with the compile-time functions supplied
    /// at their original parameter slots (FN-17).
    pub fn instance_with_functions(
        &self,
        module: usize,
        declaration: usize,
        generics: &[Argument],
        requirements: &[Answer],
        functions: &[(usize, Callee)],
    ) -> Option<(Signature, Body)> {
        self.instance_with_packs(
            (module, declaration),
            generics,
            requirements,
            functions,
            &[],
        )
    }

    /// Instantiate a declaration with its static function and type pack
    /// arguments, including member types of reflected packs (GEN-17).
    pub fn instance_with_packs(
        &self,
        declaration: (usize, usize),
        generics: &[Argument],
        requirements: &[Answer],
        functions: &[(usize, Callee)],
        packs: &[TypePackArgument],
    ) -> Option<(Signature, Body)> {
        self.instance_with_inputs(
            declaration,
            generics,
            requirements,
            InstanceInputs {
                functions,
                type_packs: packs,
                ..InstanceInputs::default()
            },
        )
    }

    /// Instantiate a declaration with the inputs its caller supplies.
    pub fn instance_with_inputs(
        &self,
        declaration: (usize, usize),
        generics: &[Argument],
        requirements: &[Answer],
        inputs: InstanceInputs<'_>,
    ) -> Option<(Signature, Body)> {
        self.checked_instance(
            declaration,
            generics,
            requirements,
            inputs,
            &RefCell::new(Diagnostics::new()),
            &mut Diagnostics::new(),
        )
        .map(|instance| (instance.signature, instance.body))
    }

    /// Instantiate a body and report any configuration that fails to fit
    /// the selected candidate (FN-18, GEN-16). Collect new body reports
    /// separately so the instance walk can point them at its caller (GEN-7).
    pub(crate) fn checked_instance(
        &self,
        (module, declaration): (usize, usize),
        generics: &[Argument],
        requirements: &[Answer],
        inputs: InstanceInputs<'_>,
        diagnostics: &RefCell<Diagnostics>,
        body_diagnostics: &mut Diagnostics,
    ) -> Option<CheckedInstance> {
        let declared = self.modules.get(module)?;
        let Some(Detail::Callable(signature)) = declared.details.get(declaration) else {
            return None;
        };
        let body = declared.bodies.get(declaration)?.as_ref()?;
        let map: Vec<(Location, &Argument)> = signature
            .generics
            .iter()
            .map(|generic| generic.at)
            .zip(generics)
            .collect();
        let packs = PackTypes::new(&signature.generics, inputs.type_packs);
        let project = |ty: &Type| {
            self.resolve_instance_type(packs.project(ty, &map).unwrap_or(Type::Unknown))
        };
        let mut taken = Signature {
            generics: Vec::new(),
            parameters: signature
                .parameters
                .iter()
                .map(|parameter| Parameter {
                    ty: project(&parameter.ty),
                    ..parameter.clone()
                })
                .collect(),
            result: signature.result_at_call(generics).map(project),
            reference: signature.reference,
        };
        let mut body = body.clone();
        let temporal = declared.declarations[declaration].kind.role() == Some(Role::Temporal);
        let functions = hidden_captures(inputs.functions, &mut taken, &mut body, temporal);
        let pending_forwards = inputs
            .pack_inputs
            .filter(|_| crate::values::has_forwarded_calls(&body));
        let arguments = InstanceArguments {
            program: self,
            declaration: (module, declaration),
            generics,
            types: &map,
            packs: &packs,
            type_packs: inputs.type_packs,
            pack_inputs: pending_forwards.unwrap_or_default(),
            constants: inputs.constants,
            test_candidates: inputs.test_candidates,
            requirements,
            diagnostics,
            changed_results: RefCell::new(Vec::new()),
        };
        let graph = declared.phases.get(declaration) == Some(&Some(Phase::Graph));
        let recheck_endpoints =
            crate::values::node_endpoint_reads(declared, declaration, Some(&taken));
        let mut reread_body = false;
        let mut selected_pack_branches = Vec::new();
        let reread_inputs = InstanceInputs {
            functions: &functions,
            pack_inputs: pending_forwards,
            ..inputs
        };
        // GEN-7: a callback bound to no result can change how the
        // instance reads its tail expression or an explicit return.
        if (inputs.test_candidates.is_some()
            || recheck_endpoints
            || (!functions.is_empty() && self.callback_phase_bodies.contains(&(module, declaration)))
            || generics.contains(&Argument::NoResult)
            || pending_forwards.is_some()
            || !inputs.constants.is_empty()
            || self.generic_store_bodies.contains(&(module, declaration))
            // TYPE-18: a cold query's operand is absent from runtime HIR.
            // Re-read it with concrete inputs so selected result types are
            // retained without retaining or executing the operand's calls.
            || crate::type_values::has_instance_query(self, (module, declaration))
            || (!generics.is_empty()
                && (crate::values::has_dependent_index(&body)
                    || crate::formatting::has_dependent_formatting(
                        self,
                        (module, declaration),
                        &body,
                    ))))
            && let Some((reread, branches)) = crate::values::reread_instance_body(
                self,
                (module, declaration),
                (&taken, &map),
                reread_inputs,
                &project,
                body_diagnostics,
            )
        {
            body = reread;
            selected_pack_branches = branches;
            reread_body = true;
        }
        let recheck = graph
            && (inputs.test_candidates.is_some()
                || !declared.selecting[declaration].is_empty()
                || !requirements.is_empty()
                || pending_forwards.is_some());
        let mut destinations = recheck.then(|| crate::values::GraphDestinations::pending(&body));
        let mut result_uses = None;
        let before = diagnostics.borrow().len();
        self.place_body(&mut body, &arguments, &functions);
        let changed_results = std::mem::take(&mut *arguments.changed_results.borrow_mut());
        if !changed_results.is_empty() {
            // GEN-7: the original body used the admitting candidate's
            // result. Read its uses with the selected type in place,
            // including inferred locals and the calls that consume them.
            let mut reports = Diagnostics::new();
            if let Some((reread, branches)) = crate::values::reread_instance_body(
                self,
                (module, declaration),
                (&taken, &map),
                reread_inputs,
                &project,
                &mut reports,
            ) {
                diagnostics.borrow_mut().truncate(before);
                body = reread;
                selected_pack_branches = branches;
                reread_body = true;
                destinations = recheck.then(|| crate::values::GraphDestinations::pending(&body));
                self.place_body(&mut body, &arguments, &functions);
            }
            result_uses = Some(ResultUseChecks {
                changed_results,
                reports,
            });
        }
        // FN-3, FN-6: what each expression is, and which calls are
        // lifted, with the candidates in place, which the generic body
        // could not say of an operation it requires or a pack it forwards.
        if let Some(destinations) = destinations {
            destinations.settle(
                self,
                (module, declaration),
                (&taken, &map, inputs.test_candidates),
                &mut body,
                body_diagnostics,
                &|expression| self.place(expression, &arguments, &functions),
            );
        } else {
            crate::values::record(self, &mut body, graph);
        }
        Some(CheckedInstance {
            signature: taken,
            body,
            reread_body,
            selected_pack_branches,
            result_uses,
        })
    }

    /// Specialize one lexical body using its own local table. A call
    /// a generic candidate or membership authorizes is selected with
    /// the instance's types in place (GEN-5 and GEN-7), including in an
    /// anonymous body.
    fn place_body(
        &self,
        body: &mut Body,
        instance: &InstanceArguments<'_>,
        functions: &[(usize, Callee)],
    ) {
        for local in &mut body.locals {
            local.ty = instance.project(&local.ty);
        }
        walk_statements_mut(&mut body.statements, &mut |expression| {
            self.place(expression, instance, functions);
        });
        let changed = crate::values::select_in_instance(
            self,
            instance.declaration,
            instance.generics,
            instance.packs,
            instance.test_candidates,
            body,
            &|call| call.walk_mut(&mut |inner| self.place(inner, instance, functions)),
        );
        instance.changed_results.borrow_mut().extend(changed);
    }

    /// Puts the instance's arguments in place in an expression's type,
    /// in what it calls, and where it reads a generic value parameter.
    fn place(
        &self,
        expression: &mut Expr,
        instance: &InstanceArguments<'_>,
        functions: &[(usize, Callee)],
    ) {
        expression.ty = instance.project(&expression.ty);
        // TYPE-18: a generic query holds the instance's operand type,
        // even though the literal's own type is always `Type`.
        if let ExprKind::Literal(Value::Type(ty)) = &mut expression.kind {
            **ty = instance.project(ty);
        }
        // ERR-14 and ERR-15: the generic child can look nonnullable
        // before placement. A nullable child compares its own payload
        // with null and still fails if the endpoint is unset.
        if let ExprKind::OrNull {
            result,
            reader: false,
        } = &expression.kind
            && let ExprKind::CheckedChild { read, result: true } = &result.kind
            && let Type::Result { value, .. } = instance.project(&result.ty)
            && matches!(*value, Type::Nullable(_))
        {
            expression.ty = *value;
            expression.kind = ExprKind::CheckedChild {
                read: read.clone(),
                result: false,
            };
        }
        match &mut expression.kind {
            // GEN-13: a generic value parameter read as a value is the
            // constant the call gives it.
            ExprKind::GenericValue { at, .. } => {
                let given = instance
                    .types
                    .iter()
                    .find_map(|(declared, argument)| match argument {
                        Argument::Constant(given) if declared == at => Some(given),
                        _ => None,
                    });
                let read = given
                    .and_then(|given| self.constant(&given.value, &expression.ty, expression.at));
                if let Some(kind) = read {
                    expression.kind = kind;
                }
            }
            ExprKind::Call {
                callee, arguments, ..
            } => {
                let required = if let Callee::Required { configuration, .. } = callee {
                    Some(configuration.take())
                } else {
                    None
                };
                let callback = matches!(callee, Callee::Parameter { .. });
                self.place_target(callee, instance, functions);
                if callback
                    && let Some((module, declaration)) = callee.declaration()
                    && let Detail::Callable(signature) = &self.modules[module].details[declaration]
                    && signature
                        .parameters
                        .iter()
                        .any(|parameter| parameter.configuration)
                {
                    // FN-18: template slots omit the callback's defaulted
                    // configuration, which still occupies declaration slots.
                    let ordinary = signature
                        .parameters
                        .iter()
                        .enumerate()
                        .filter(|(_, parameter)| !parameter.configuration);
                    for argument in arguments.iter_mut() {
                        argument.parameter = argument
                            .parameter
                            .and_then(|index| ordinary.clone().nth(index).map(|(slot, _)| slot));
                    }
                }
                if let Some(configuration) = required {
                    if let Callee::Required {
                        configuration: unresolved,
                        ..
                    } = callee
                    {
                        *unresolved = configuration;
                    } else {
                        self.place_required_arguments(callee, arguments, configuration, instance);
                    }
                }
            }
            ExprKind::DeferredBinary {
                expected: Some(expected),
                ..
            }
            | ExprKind::DeferredNegation {
                expected: Some(expected),
                ..
            } => {
                **expected = substitute_within(expected, instance.types);
            }
            ExprKind::StaticCallable(callee) => self.place_target(callee, instance, functions),
            _ => {}
        }
    }

    /// A GEN-16 call records public parameter indices. The chosen
    /// candidate may arrange or add configuration slots (FN-18).
    fn place_required_arguments(
        &self,
        callee: &Callee,
        arguments: &mut [crate::hir::Argument],
        context: Option<Box<ConfigurationRead>>,
        instance: &InstanceArguments<'_>,
    ) {
        let Callee::Candidate {
            operator,
            module,
            declaration,
            generics,
            ..
        } = callee
        else {
            return;
        };
        let Some(Detail::Callable(public)) = self.modules[operator.0].details.get(operator.1)
        else {
            return;
        };
        let Some(Detail::Callable(candidate)) = self.modules[*module].details.get(*declaration)
        else {
            return;
        };
        let slots: Vec<_> = public.candidate_parameters(candidate).collect();
        let map: Vec<_> = candidate
            .generics
            .iter()
            .map(|generic| generic.at)
            .zip(generics)
            .collect();
        let context = context.map(|mut context| {
            context.result = context.result.map(|ty| instance.project(&ty));
            context
        });
        for argument in arguments {
            argument.parameter = argument
                .parameter
                .and_then(|index| slots.get(index).copied().flatten());
            let Some(parameter) = argument.parameter.map(|slot| &candidate.parameters[slot]) else {
                continue;
            };
            let Some(context) = context.as_deref().filter(|_| parameter.configuration) else {
                continue;
            };
            // This walk visits the call before each argument. Its type
            // is specialized here, while the expression still records
            // whether it was written as a literal or a generic constant.
            let actual = instance.project(&argument.value.ty);
            let expected = substitute_within(&parameter.ty, &map);
            if actual != expected {
                let mut value = argument.value.clone();
                value.ty = actual;
                argument.value = crate::values::fit_required_configuration(
                    self,
                    instance.declaration,
                    value,
                    &expected,
                    &parameter.name,
                    context,
                    &mut instance.diagnostics.borrow_mut(),
                );
            }
        }
    }

    fn place_target(
        &self,
        callee: &mut Callee,
        instance: &InstanceArguments<'_>,
        functions: &[(usize, Callee)],
    ) {
        if let Callee::Anonymous(function) = callee {
            let functions = inherited_functions(function, functions);
            let template = Type::Function(Box::new(function.template.clone()));
            if let Type::Function(template) = instance.project(&template) {
                function.template = *template;
            }
            let specialization = &mut function.specialization;
            // A type pack's slot holds a placeholder. Keep its concrete
            // type sequence and keys separately.
            let type_pack = |index| {
                instance
                    .type_packs
                    .iter()
                    .any(|pack| pack.parameter == index)
            };
            specialization.generics.extend(
                instance
                    .types
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| !type_pack(*index))
                    .map(|(_, (_, value))| (*value).clone()),
            );
            specialization
                .type_packs
                .extend_from_slice(instance.type_packs);
            specialization
                .pack_inputs
                .extend_from_slice(instance.pack_inputs);
            specialization.const_inputs.extend(
                instance
                    .constants
                    .iter()
                    .map(|input| (input.slot, input.key.clone())),
            );
            self.place_body(&mut function.body, instance, &functions);
            crate::values::record(self, &mut function.body, !function.template.value);
        } else {
            place_callee(callee, instance, functions);
            if let Callee::Function { type_packs, .. } | Callee::Candidate { type_packs, .. } =
                callee
            {
                for pack in type_packs {
                    instance.packs.place_forwarded(pack);
                }
            }
        }
    }

    /// A constant a generic value parameter is given, `value` of type
    /// `ty`, as the expression that reads it, written at `at`: `None`
    /// for one whose value is not held. A list, string set and tuple
    /// have each element as its own expression.
    pub(crate) fn constant(
        &self,
        value: &ConstantValue,
        ty: &Type,
        at: Location,
    ) -> Option<ExprKind> {
        // TYPE-14: "`T?` denotes an ordinary value that is either a T or
        // null": a value of it other than null is a T.
        let of = match ty {
            Type::Nullable(inner) if !matches!(value, ConstantValue::Null) => inner.as_ref(),
            other => other,
        };
        // A constant under `any`, an element of a `list<any>`, says its
        // own type: a member its enum, and a list's or a tuple's
        // elements are each an `any` in turn.
        let any = matches!(of, Type::Scalar(Scalar::Any));
        let element = |value: &ConstantValue, ty: &Type| -> Option<Expr> {
            Some(Expr {
                kind: self.constant(value, ty, at)?,
                ty: ty.clone(),
                at,
                category: ValueCategory::Unsettled,
            })
        };
        match value {
            // TYPE-7: a member is known by its name, and the enum is
            // the parameter's type.
            ConstantValue::Member(named) => {
                let (module, declaration) = match of {
                    Type::Named {
                        module,
                        declaration,
                        ..
                    } => (module, declaration),
                    _ if any => {
                        let (module, declaration) = &named.of.as_ref()?.declared;
                        (module, declaration)
                    }
                    _ => return None,
                };
                let Some(Detail::Enum(read)) = self.modules.get(*module)?.details.get(*declaration)
                else {
                    return None;
                };
                let member = read
                    .members
                    .iter()
                    .position(|member| member.name == named.name)?;
                Some(ExprKind::EnumMember {
                    module: *module,
                    declaration: *declaration,
                    member,
                })
            }
            ConstantValue::List(values) => {
                let of = match of {
                    Type::List { element, .. } => element.as_ref(),
                    _ if any => of,
                    _ => return None,
                };
                values
                    .iter()
                    .map(|value| element(value, of))
                    .collect::<Option<Vec<Expr>>>()
                    .map(ExprKind::Sequence)
            }
            ConstantValue::StringSet(names) => {
                let Type::Set(element) = of else {
                    return None;
                };
                Some(ExprKind::Sequence(
                    names
                        .iter()
                        .map(|name| Expr {
                            kind: ExprKind::Literal(Value::Str(name.clone())),
                            ty: (**element).clone(),
                            at,
                            category: ValueCategory::Unsettled,
                        })
                        .collect(),
                ))
            }
            ConstantValue::Tuple(values) => {
                let anys;
                let elements = match of {
                    Type::Tuple(elements) => elements,
                    _ if any => {
                        anys = vec![of.clone(); values.len()];
                        &anys
                    }
                    _ => return None,
                };
                if elements.len() != values.len() {
                    return None;
                }
                values
                    .iter()
                    .zip(elements)
                    .map(|(value, of)| element(value, of))
                    .collect::<Option<Vec<Expr>>>()
                    .map(ExprKind::Tuple)
            }
            // A literal's value: an integer, a float, a string.
            value => value.literal().map(ExprKind::Literal),
        }
    }
}

/// FN-17: a nested function that uses a static parameter also needs the
/// ordinary values its target captures. Relay only the captures used by
/// this scope or its descendants.
fn inherited_functions(
    function: &mut AnonymousFunction,
    enclosing: &[(usize, Callee)],
) -> Vec<(usize, Callee)> {
    if enclosing.is_empty() {
        return Vec::new();
    }
    let mut used = Vec::new();
    used_function_parameters(&function.body, &mut used);
    let mut functions: Vec<_> = enclosing
        .iter()
        .filter(|(slot, _)| used.contains(slot))
        .cloned()
        .collect();
    let inputs = function.template.parameters.len();
    let first_local = inputs + function.captures.len();
    let mut added = Vec::new();
    for (_, target) in &mut functions {
        let Callee::Anonymous(target) = target else {
            continue;
        };
        let target_inputs = target.template.parameters.len();
        for index in 0..target.captures.len() {
            let captured = target.captures[index];
            let key = target.capture_pack_key(index);
            let position = function
                .captures
                .iter()
                .enumerate()
                .position(|(index, outer)| {
                    *outer == captured && function.capture_pack_key(index) == key
                });
            let offset = position.unwrap_or_else(|| {
                let offset = function.captures.len();
                function.captures.push(captured);
                if let Some(key) = key {
                    function.pack_captures.push((offset, key.clone()));
                }
                added.push(target.body.locals[target_inputs + index].clone());
                offset
            });
            target.captures[index] = inputs + offset;
        }
        target.pack_captures.clear();
    }
    if !added.is_empty() {
        let remapped: Vec<_> = (0..function.body.locals.len())
            .map(|local| {
                if local < first_local {
                    local
                } else {
                    local + added.len()
                }
            })
            .collect();
        crate::values::remap_locals(&mut function.body, &remapped);
        function.body.locals.splice(first_local..first_local, added);
    }
    functions
}

/// Record the declaration slots used by this anonymous body, including
/// the slots referenced in a nested anonymous argument.
pub(crate) fn used_function_parameters(body: &Body, used: &mut Vec<usize>) {
    walk_statements(&body.statements, &mut |expression| {
        let target = match &expression.kind {
            ExprKind::Call { callee, .. } => callee,
            ExprKind::StaticCallable(callee) => callee,
            _ => return,
        };
        match target {
            Callee::Parameter { parameter, .. } => used.push(*parameter),
            Callee::Anonymous(nested) => used_function_parameters(&nested.body, used),
            _ => {}
        }
    });
}

/// Carry an anonymous function's immutable environment as ordinary
/// parameters of the specialized function. Its compile-time identity is
/// kept separately from the captures supplied by each invocation.
fn hidden_captures(
    functions: &[(usize, Callee)],
    signature: &mut Signature,
    body: &mut Body,
    temporal: bool,
) -> Vec<(usize, Callee)> {
    let mut functions = functions.to_vec();
    functions.sort_by_key(|(parameter, _)| *parameter);
    let original = signature.parameters.len();
    let mut locals = Vec::new();
    for (slot, callee) in &mut functions {
        let Callee::Anonymous(function) = callee else {
            continue;
        };
        let start = function.template.parameters.len();
        for (index, capture) in function.captures.iter_mut().enumerate() {
            let mut local = function.body.locals[start + index].clone();
            let mut name = format!("fn_capture_{slot}_{index}");
            while body.locals.iter().any(|other| other.name == name) {
                name.push('_');
            }
            local.name = name.clone();
            local.kind = LocalKind::Parameter;
            *capture = signature.parameters.len();
            signature.parameters.push(Parameter {
                configuration: false,
                name,
                at: local.at,
                constant: temporal && local.category == ValueCategory::Value,
                var: false,
                reference: false,
                ty: local.ty.clone(),
                pack: None,
                pack_bounds: (0, None),
                has_default: false,
            });
            locals.push(local);
        }
        function.pack_captures.clear();
    }
    if !locals.is_empty() {
        let added = locals.len();
        let remapped = (0..body.locals.len())
            .map(|index| {
                if index < original {
                    index
                } else {
                    index + added
                }
            })
            .collect::<Vec<_>>();
        crate::values::remap_locals(body, &remapped);
        body.locals.splice(original..original, locals);
    }
    functions
}

/// What an expression calls by requiring it directly or passing evidence on.
fn required_uses(callee: &Callee, at: Location, uses: &mut Vec<(usize, Location)>) {
    fn forwarded(answers: &[Answer], at: Location, uses: &mut Vec<(usize, Location)>) {
        for answer in answers {
            match answer {
                Answer::Stated(index) => uses.push((*index, at)),
                Answer::Candidate { requirements, .. } => forwarded(requirements, at, uses),
                Answer::Unread => {}
            }
        }
    }
    match callee {
        Callee::Required { requirement, .. } => uses.push((*requirement, at)),
        Callee::Function { requirements, .. } | Callee::Candidate { requirements, .. } => {
            forwarded(requirements, at, uses);
        }
        Callee::Anonymous(function) => {
            walk_statements(&function.body.statements, &mut |value| match &value.kind {
                ExprKind::Call { callee, .. } => required_uses(callee, value.at, uses),
                ExprKind::StaticCallable(callee) => required_uses(callee, value.at, uses),
                _ => {}
            })
        }
        _ => {}
    }
}

/// Puts what `map` gives in place in what a call calls. A call of an
/// operation the declaration requires becomes the call of the candidate
/// that answers the requirement (GEN-16); one nothing answers is left
/// as it is.
fn place_callee(
    callee: &mut Callee,
    instance: &InstanceArguments<'_>,
    functions: &[(usize, Callee)],
) {
    if let Callee::Parameter { parameter, .. } = callee
        && let Some((_, target)) = functions.iter().find(|(index, _)| index == parameter)
    {
        *callee = target.clone();
        // The argument belongs to the caller's context, which was already placed.
        return;
    }
    // GEN-17: written type arguments can mention a reflected pack
    // member, which needs the enclosing instance's keys and types.
    // An untold member requirement keeps those arguments symbolic.
    let defer_pack_members = matches!(
        callee,
        Callee::Function {
            defer_pack_members: true,
            ..
        }
    );
    let place_type = |ty: &Type| {
        if defer_pack_members {
            substitute_within(ty, instance.types)
        } else {
            instance.project(ty)
        }
    };
    let place = |argument: &Argument| match argument {
        Argument::Type(ty) => Argument::Type(place_type(ty)),
        Argument::Constant(c) if matches!(c.value, ConstantValue::Parameter { .. }) => {
            placed(argument, instance.types)
        }
        Argument::Constant(c) => Argument::Constant(Constant {
            text: c.text.clone(),
            value: instance
                .program
                .specialize_constant(&c.value, instance.types),
        }),
        Argument::NoResult => Argument::NoResult,
    };
    if let Callee::Function { type_packs, .. } | Callee::Candidate { type_packs, .. } = callee {
        for pack in type_packs {
            pack.place_types(place_type);
        }
    }
    if let Callee::Candidate {
        operator_type_packs,
        ..
    }
    | Callee::Required {
        operator_type_packs,
        ..
    } = callee
    {
        for pack in operator_type_packs {
            pack.place_types(place_type);
        }
    }
    if let Callee::Candidate {
        operator_generics, ..
    }
    | Callee::Required {
        operator_generics, ..
    } = callee
    {
        for argument in operator_generics {
            *argument = place(argument);
        }
    }
    match callee {
        Callee::Function {
            generics,
            requirements,
            ..
        }
        | Callee::Candidate {
            generics,
            requirements,
            ..
        } => {
            for argument in generics {
                *argument = place(argument);
            }
            for answer in requirements {
                place_answer(answer, instance);
            }
        }
        Callee::Required {
            requirement,
            operator_generics,
            operator_type_packs,
            ..
        } => {
            if let Some(Answer::Candidate {
                operator,
                module,
                declaration,
                generics,
                type_packs,
                requirements,
            }) = instance.requirements.get(*requirement)
            {
                *callee = Callee::Candidate {
                    operator: *operator,
                    operator_generics: std::mem::take(operator_generics),
                    operator_type_packs: std::mem::take(operator_type_packs),
                    module: *module,
                    declaration: *declaration,
                    generics: generics.clone(),
                    type_packs: type_packs.clone(),
                    requirements: requirements.clone(),
                    test_candidates: instance.test_candidates,
                };
            }
        }
        Callee::Parameter { .. }
        | Callee::Anonymous(_)
        | Callee::DeferredOperator(_)
        | Callee::DeferredPackCall
        | Callee::DeferredValue
        | Callee::Unread => {}
    }
}

/// Places the instance's arguments in an answer, using its caller's
/// evidence for any requirement the enclosing declaration states (GEN-7).
fn place_answer(answer: &mut Answer, instance: &InstanceArguments<'_>) {
    match answer {
        Answer::Stated(index) => {
            *answer = instance
                .requirements
                .get(*index)
                .cloned()
                .unwrap_or(Answer::Unread);
        }
        Answer::Candidate {
            generics,
            type_packs,
            requirements,
            ..
        } => {
            for argument in generics {
                *argument = placed(argument, instance.types);
            }
            for pack in type_packs {
                pack.place_types(|ty| instance.project(ty));
                instance.packs.place_forwarded(pack);
            }
            for own in requirements {
                place_answer(own, instance);
            }
        }
        Answer::Unread => {}
    }
}

/// A generic argument with what `map` gives in place: a type with its
/// parameters replaced, a constant that is a generic value parameter
/// replaced by the constant given for it, and one that holds such a
/// parameter, `N + 1` or `[N]`, worked out with it.
pub(crate) fn placed(argument: &Argument, map: &Map) -> Argument {
    let given = |at: Location| {
        map.iter().find_map(|(declared, given)| match given {
            Argument::Constant(given) if *declared == at => Some(given),
            _ => None,
        })
    };
    match argument {
        Argument::Type(ty) => Argument::Type(substitute_within(ty, map)),
        Argument::NoResult => Argument::NoResult,
        Argument::Constant(constant) => match &constant.value {
            ConstantValue::Type(ty) => Argument::Constant(Constant {
                text: constant.text.clone(),
                value: ConstantValue::Type(Box::new(substitute_within(ty, map))),
            }),
            ConstantValue::Parameter { at, .. } => given(*at)
                .map(|given| Argument::Constant(given.clone()))
                .unwrap_or_else(|| argument.clone()),
            value => Argument::Constant(Constant {
                text: constant.text.clone(),
                value: value.substituted(&|at| given(at).map(|given| given.value.clone())),
            }),
        },
    }
}
