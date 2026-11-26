//! Checks omitted defaults after function and candidate selection.
//!
//! The operator owns the default (GEN-6). The declaration pass binds and
//! checks the expression in its defining scope (UNIT-7). With generic
//! arguments in place, the selected parameter gives it a type (UNIT-8).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use hgl_diagnostics::{Category, Code, Diagnostics, Location};

use super::constants::parameter_default_type;
use super::fit::{open, whole};
use super::requirements::{
    IndependentSourceBodies, Made, MakingCall, MakingPath, concrete_default_calls,
    dependent_default_call_sites, emit_labeled_instance_report,
};
use super::{Tables, Typer, lifting};
use crate::declared::names_parameter;
use crate::hir::{self, Body, Callee, ConfigurationRead, Expr, ExprKind, Value};
use crate::program::{Module, Program};
use crate::types::{Argument, Detail, Parameter, Type};
use crate::{codes, error};

/// Visit the selected calls once the declaration pass has read every default.
pub(super) fn check(
    program: &Program,
    source: &IndependentSourceBodies,
    diagnostics: &mut Diagnostics,
) -> Vec<Made> {
    let mut made = Vec::new();
    for (unit, module) in program.modules.iter().enumerate() {
        let mut visit = |value: &Expr| check_expression(program, value, &mut made, diagnostics);
        for (declaration, body) in module.bodies.iter().enumerate() {
            if let Some(body) = source.body((unit, declaration)).or(body.as_ref()) {
                hir::walk_statements(&body.statements, &mut visit);
            }
        }
        for body in module
            .test_bodies
            .iter()
            .flatten()
            .chain(&module.store_body)
        {
            hir::walk_statements(&body.statements, &mut visit);
        }
        if let Some(run) = &module.run_body {
            run.call.walk(&mut visit);
        }
        for constant in module.constants.iter().flatten() {
            constant.value.walk(&mut visit);
        }
        for (defaults, declaration) in module.defaults.iter().zip(&module.declarations) {
            for default in defaults.iter().flatten() {
                // Check inherited defaults at their original declarations.
                if declaration.declares_default(default) {
                    default.value.walk(&mut visit);
                }
            }
        }
    }
    made
}

/// Each used default is specialized once in its effective environment.
/// An explicit worklist keeps cycles and long chains out of the Rust
/// stack. Like body instances, the work is bounded by INSTANCES.
#[derive(Default)]
struct Defaults<'a> {
    seen: Vec<DefaultUse>,
    pending: Vec<PendingDefault>,
    /// Path to the default whose expression the worklist is visiting.
    containing_default: Option<Rc<MakingPath>>,
    pack_conditions_only: bool,
    /// Enclosing body-instance paths, needed only if a default rejects.
    making_paths: Option<&'a dyn Fn() -> Vec<Vec<MakingCall>>>,
}

struct PendingDefault {
    value: Expr,
    path: Rc<MakingPath>,
    dependent_calls: Vec<Location>,
}

#[derive(PartialEq)]
struct DefaultUse {
    at: Location,
    owner: (usize, usize),
    generics: Vec<Argument>,
    type_packs: Vec<hir::TypePackArgument>,
    expected: Type,
}

fn check_expression(
    program: &Program,
    value: &Expr,
    made: &mut Vec<Made>,
    diagnostics: &mut Diagnostics,
) {
    follow_defaults(program, value, made, diagnostics, Defaults::default());
}

/// A call first resolved in a forwarded pack body can omit a pack-guarded
/// default. Ordinary default checking retains its existing traversal.
pub(super) fn check_forwarded_default(
    program: &Program,
    value: &Expr,
    made: &mut Vec<Made>,
    diagnostics: &mut Diagnostics,
) {
    follow_defaults(
        program,
        value,
        made,
        diagnostics,
        Defaults {
            pack_conditions_only: true,
            ..Defaults::default()
        },
    );
}

fn follow_defaults(
    program: &Program,
    value: &Expr,
    made: &mut Vec<Made>,
    diagnostics: &mut Diagnostics,
    mut defaults: Defaults<'_>,
) {
    defaults.visit(program, value, diagnostics);
    defaults.pack_conditions_only = false;
    while let Some(default) = defaults.pending.pop() {
        concrete_default_calls(
            program,
            &default.value,
            (&default.path, &default.dependent_calls),
            made,
        );
        defaults.containing_default = Some(default.path);
        default
            .value
            .walk(&mut |value| defaults.visit(program, value, diagnostics));
    }
}

impl Defaults<'_> {
    fn visit(&mut self, program: &Program, value: &Expr, diagnostics: &mut Diagnostics) {
        match &value.kind {
            ExprKind::Call {
                callee, arguments, ..
            } => {
                let supplied = |slot| {
                    arguments
                        .iter()
                        .any(|argument| argument.parameter == Some(slot))
                };
                check_declared_defaults(program, self, (callee, value.at), supplied, diagnostics);
                check_inherited_defaults(program, self, (callee, value.at), supplied, diagnostics);
            }
            ExprKind::StaticCallable(callee)
                if let Callee::Anonymous(function) = callee.as_ref() =>
            {
                hir::walk_statements(&function.body.statements, &mut |value| {
                    self.visit(program, value, diagnostics);
                });
            }
            _ => {}
        }
    }
}

/// True where an instance can resolve a default's expected type or
/// the selected configuration of a required call.
pub(super) fn selects_configuration(program: &Program, unit: &Module, declaration: usize) -> bool {
    let has_default = |operator: (usize, usize)| {
        let unit = &program.modules[operator.0];
        let Some(Detail::Callable(signature)) = unit.details.get(operator.1) else {
            return false;
        };
        signature.parameters.iter().any(configuration_default)
            || unit.defaults.get(operator.1).is_some_and(|defaults| {
                signature
                    .parameters
                    .iter()
                    .zip(defaults)
                    .any(|(parameter, default)| {
                        default
                            .as_ref()
                            .is_some_and(|default| dependent_default(parameter, default))
                    })
            })
    };
    if unit.selecting.get(declaration).is_some_and(|calls| {
        calls
            .iter()
            .any(|call| has_default((call.operator.module, call.operator.declaration)))
    }) {
        return true;
    }
    let Some(Some(body)) = unit.bodies.get(declaration) else {
        return false;
    };
    fn in_body(body: &Body, has_default: &dyn Fn((usize, usize)) -> bool) -> bool {
        let mut found = false;
        hir::walk_statements(&body.statements, &mut |value| {
            if found {
                return;
            }
            match &value.kind {
                ExprKind::Call {
                    callee:
                        Callee::Required {
                            configuration: Some(_),
                            ..
                        },
                    ..
                } => found = true,
                ExprKind::Call {
                    callee:
                        Callee::Candidate {
                            operator,
                            module,
                            declaration,
                            ..
                        },
                    ..
                } => found = has_default(*operator) || has_default((*module, *declaration)),
                ExprKind::Call {
                    callee: Callee::Required { operator, .. },
                    ..
                } => found = has_default(*operator),
                ExprKind::Call { callee, .. } => {
                    found = callee.declaration().is_some_and(has_default);
                }
                ExprKind::StaticCallable(callee)
                    if let Callee::Anonymous(function) = callee.as_ref() =>
                {
                    found = in_body(&function.body, has_default);
                }
                _ => {}
            }
        });
        found
    }
    in_body(body, &has_default)
}

/// UNIT-8: uses the specialized body's actual candidate and argument slots.
pub(super) fn check_instance(
    program: &Program,
    callee: (usize, usize),
    (generics, requirements): (&[Argument], &[hir::Answer]),
    inputs: crate::InstanceInputs<'_>,
    made: &mut Vec<Made>,
    (diagnostics, making_paths): (&mut Diagnostics, &dyn Fn() -> Vec<Vec<MakingCall>>),
    graph_diagnostics: &mut Diagnostics,
) -> Option<crate::instance::CheckedInstance> {
    let reports = RefCell::new(Diagnostics::new());
    let instance = program.checked_instance(
        callee,
        generics,
        requirements,
        inputs,
        &reports,
        graph_diagnostics,
    )?;
    let mut reports = reports.into_inner();
    instance.walk_calls(&mut |value| {
        if matches!(value.kind, ExprKind::Call { .. }) {
            follow_defaults(
                program,
                value,
                made,
                &mut reports,
                Defaults {
                    making_paths: Some(making_paths),
                    ..Defaults::default()
                },
            );
        }
    });
    for report in reports.as_slice() {
        if !diagnostics.as_slice().contains(report) {
            diagnostics.emit(report.clone());
        }
    }
    Some(instance)
}

/// UNIT-8: check a selected call's own omitted defaults once their
/// parameter types are concrete.
fn check_declared_defaults(
    program: &Program,
    checking: &mut Defaults<'_>,
    (callee, at): (&Callee, Location),
    supplied: impl Fn(usize) -> bool,
    diagnostics: &mut Diagnostics,
) {
    let Some(owner) = callee.declaration() else {
        return;
    };
    let unit = &program.modules[owner.0];
    let Some(Detail::Callable(signature)) = unit.details.get(owner.1) else {
        return;
    };
    if !signature
        .parameters
        .iter()
        .any(|parameter| parameter.has_default)
    {
        return;
    }
    let Some(defaults) = unit.defaults.get(owner.1) else {
        return;
    };
    let generics = callee.generics();
    let map: Vec<_> = signature
        .generics
        .iter()
        .map(|generic| generic.at)
        .zip(generics)
        .collect();
    for (slot, (parameter, default)) in signature.parameters.iter().zip(defaults).enumerate() {
        let Some(default) = default else {
            continue;
        };
        if !dependent_default(parameter, default)
            || supplied(slot)
            || parameter.configuration
                && program
                    .inherited_configuration_default(callee, slot)
                    .is_some()
        {
            continue;
        }
        let ty = program.specialize_type(&parameter.ty, &map);
        if names_parameter(&ty) || !whole(&ty) {
            continue;
        }
        let concrete = Parameter {
            ty,
            ..parameter.clone()
        };
        checking.specialize(
            program,
            (owner, generics, callee.type_packs()),
            (
                default,
                &concrete,
                at,
                follows_omitted_default(parameter, default),
            ),
            diagnostics,
        );
    }
}

/// UNIT-8: check only the inherited defaults that this selected call uses.
fn check_inherited_defaults(
    program: &Program,
    checking: &mut Defaults<'_>,
    (callee, at): (&Callee, Location),
    supplied: impl Fn(usize) -> bool,
    diagnostics: &mut Diagnostics,
) {
    let Callee::Candidate {
        operator,
        operator_generics,
        operator_type_packs,
        module,
        declaration,
        generics,
        ..
    } = callee
    else {
        return;
    };
    let owner = &program.modules[operator.0];
    let Some(Detail::Callable(public)) = owner.details.get(operator.1) else {
        return;
    };
    if !public.parameters.iter().any(configuration_default) {
        return;
    }
    let Some(Detail::Callable(candidate)) = program.modules[*module].details.get(*declaration)
    else {
        return;
    };
    let Some(defaults) = owner.defaults.get(operator.1) else {
        return;
    };
    let map: Vec<(Location, &Argument)> = candidate
        .generics
        .iter()
        .map(|generic| generic.at)
        .zip(generics)
        .collect();
    let packs = program.operator_default_packs(*operator, operator_generics, operator_type_packs);
    for ((parameter, slot), default) in public
        .parameters
        .iter()
        .zip(public.candidate_parameters(candidate))
        .zip(defaults)
    {
        let (Some(slot), Some(default)) = (slot, default) else {
            continue;
        };
        if !configuration_default(parameter) || supplied(slot) {
            continue;
        }
        let parameter = &candidate.parameters[slot];
        let ty = program.specialize_type(&parameter.ty, &map);
        let concrete = Parameter {
            ty,
            ..parameter.clone()
        };
        checking.specialize(
            program,
            (*operator, operator_generics, &packs),
            (default, &concrete, at, false),
            diagnostics,
        );
    }
}

impl Defaults<'_> {
    /// Check a default in its defining scope and follow the calls it uses.
    fn specialize(
        &mut self,
        program: &Program,
        (owner, generics, type_packs): ((usize, usize), &[Argument], &[hir::TypePackArgument]),
        (default, parameter, at, carry_path): (&hir::DefaultExpr, &Parameter, Location, bool),
        diagnostics: &mut Diagnostics,
    ) {
        let pack_condition = default.needs_pack_specialization();
        if self.pack_conditions_only && !pack_condition {
            return;
        }
        let path = || {
            MakingPath::new(
                MakingCall::new(program, owner, generics, type_packs, at),
                self.containing_default.clone(),
            )
        };
        let type_packs = if pack_condition || default.needs_type_specialization() {
            // A generic caller can forward its own parameters here. Its
            // concrete call will visit this default with the chosen types.
            if !generics.iter().enumerate().all(|(index, argument)| {
                if pack_condition
                    && let Some(pack) = type_packs.iter().find(|pack| pack.parameter == index)
                {
                    // The type argument slot is a placeholder. The pack
                    // carries the resolved keys, including an empty pack.
                    return !pack.infer_from_arguments
                        && pack.forwarded_from.is_none()
                        && pack
                            .types_with_prefix()
                            .all(|ty| whole(ty) && !names_parameter(ty));
                }
                super::requirements::resolved_default_argument(argument)
            }) {
                return;
            }
            type_packs
        } else {
            &[]
        };
        let use_key = DefaultUse {
            at: default.value.at,
            owner,
            generics: generics.to_vec(),
            type_packs: type_packs.to_vec(),
            expected: parameter.ty.clone(),
        };
        if self.seen.len() >= crate::INSTANCES || self.seen.contains(&use_key) {
            return;
        }
        self.seen.push(use_key);
        let mut reports = Diagnostics::new();
        let specialized = program.specialize_parameter_default(
            default,
            owner,
            generics,
            type_packs,
            parameter,
            &mut reports,
        );
        if !reports.has_errors() && reads_generic_value(default) {
            with_configuration_typer(program, owner, &mut reports, |typer| {
                typer.check_default_evaluation(default.value.at, parameter, generics);
            });
        }
        for report in reports.as_slice() {
            let mut report = report.clone();
            if pack_condition {
                let selected = report.primary;
                report.primary = at;
                report = report.with_related(selected, "in the selected arm of the default");
            } else if !parameter.configuration
                && !generics.is_empty()
                && report.code == Code::Catalogued(codes::TYPE_MISMATCH)
            {
                // GEN-7: the call, including any enclosing instance
                // calls, decides the omitted default's expected type.
                let here = path();
                let mut paths = self.making_paths.map_or_else(Vec::new, |paths| paths());
                if paths.is_empty() {
                    paths.push(here.calls().cloned().collect());
                } else {
                    for outer in &mut paths {
                        *outer = here.calls().cloned().chain(std::mem::take(outer)).collect();
                    }
                }
                emit_labeled_instance_report(
                    report,
                    &paths,
                    diagnostics,
                    "the omitted default is checked here",
                );
                continue;
            }
            if !diagnostics.as_slice().contains(&report) {
                diagnostics.emit(report);
            }
        }
        if !reports.has_errors() {
            // GEN-16: the specialized expression can call a candidate
            // that was not known when its source declaration was read.
            let dependent_calls = if carry_path {
                dependent_default_call_sites(program, owner, &default.value)
            } else {
                Vec::new()
            };
            self.pending.push(PendingDefault {
                value: specialized.value,
                path: path(),
                dependent_calls,
            });
        }
    }
}

/// An ordinary generic parameter's dependent calls are made by the
/// argument that omits it. Pack guards retain their dedicated traversal.
pub(super) fn follows_omitted_default(parameter: &Parameter, default: &hir::DefaultExpr) -> bool {
    !parameter.configuration
        && !default.needs_pack_specialization()
        && (names_parameter(&parameter.ty) || reads_generic_call(default))
}

fn reads_generic_call(default: &hir::DefaultExpr) -> bool {
    let mut found = false;
    default.value.walk(&mut |value| {
        if !found && let Some(callee) = super::requirements::called_or_referenced(value) {
            found = super::requirements::forwards_default_types(
                callee.generics(),
                callee.type_packs(),
                |_| true,
            );
        }
    });
    found
}

/// An omitted default can depend on generic arguments through its type
/// or through a value read by the expression.
fn dependent_default(parameter: &Parameter, default: &hir::DefaultExpr) -> bool {
    names_parameter(&parameter.ty)
        || crate::constant::type_has_instance_query(&parameter.ty)
        || reads_generic_value(default)
        || default.needs_type_specialization()
        || default.needs_pack_specialization()
        || !parameter.configuration && reads_generic_call(default)
}

fn reads_generic_value(default: &hir::DefaultExpr) -> bool {
    let mut found = false;
    default
        .value
        .walk(&mut |value| found |= matches!(value.kind, ExprKind::GenericValue { .. }));
    found
}

/// A configuration parameter with a default of its own.
fn configuration_default(parameter: &Parameter) -> bool {
    parameter.configuration && parameter.has_default
}

/// GEN-16: a call in a default uses the operator's own requirements,
/// with the operator's types in place and its lexical candidates visible.
pub(crate) fn operator_default_answers(
    program: &Program,
    operator: (usize, usize),
    generics: &[Argument],
    uses: &[(usize, Location)],
    diagnostics: &mut Diagnostics,
) -> Vec<hir::Answer> {
    with_configuration_typer(program, operator, diagnostics, |typer| {
        typer.answers_for_default(operator.0, operator.1, generics, uses)
    })
}

/// Apply the ordinary place rules to the substituted expression in the
/// operator's context. No syntax or nested defaults are read again.
pub(crate) fn fit_operator_default(
    program: &Program,
    operator: (usize, usize),
    mut value: Expr,
    parameter: &Parameter,
    diagnostics: &mut Diagnostics,
) -> Expr {
    let null = matches!(value.kind, ExprKind::Literal(Value::Null));
    let expected = parameter_default_type(&parameter.ty, null);
    if value.ty == *expected
        || names_parameter(expected)
        || !whole(expected)
        || names_parameter(&value.ty)
    {
        return value;
    }
    if null {
        if matches!(expected, Type::Nullable(_)) {
            value.ty = expected.clone();
        } else if !open(expected) && *expected != Type::Signal {
            diagnostics.emit(error(
                Category::Type,
                codes::TYPE_MISMATCH,
                value.at,
                format!(
                    "`null` is no default of `{}`, which takes `{expected}`",
                    parameter.name
                ),
                "TYPE-14",
            ));
        }
        return value;
    }
    let place = format!("the default of `{}`", parameter.name);
    let mut value = with_configuration_typer(program, operator, diagnostics, |typer| {
        typer.fit_parameter_default(value, parameter, expected, &place)
    });
    lifting::record_value(program, &mut value);
    value
}

/// Fits supplied configuration as a value of the selected candidate's
/// parameter (FN-18). The original call still checks its public type.
pub(crate) fn fit_required_configuration(
    program: &Program,
    caller: (usize, usize),
    mut value: Expr,
    expected: &Type,
    name: &str,
    context: &ConfigurationRead,
    diagnostics: &mut Diagnostics,
) -> Expr {
    if names_parameter(expected) || !whole(expected) || names_parameter(&value.ty) {
        return value;
    }
    if matches!(value.kind, ExprKind::Literal(Value::Null)) {
        if matches!(expected, Type::Nullable(_)) {
            value.ty = expected.clone();
        } else if !open(expected) && *expected != Type::Signal {
            diagnostics.emit(error(
                Category::Type,
                codes::TYPE_MISMATCH,
                value.at,
                format!(
                    "`null` cannot be given to the configuration parameter `{name}`, which takes `{expected}`"
                ),
                "TYPE-14",
            ));
        }
        return value;
    }
    fit_configuration(
        program,
        caller,
        value,
        expected,
        name,
        Some(context),
        diagnostics,
    )
}

fn fit_configuration(
    program: &Program,
    declaration: (usize, usize),
    value: Expr,
    expected: &Type,
    name: &str,
    context: Option<&ConfigurationRead>,
    diagnostics: &mut Diagnostics,
) -> Expr {
    with_configuration_typer(program, declaration, diagnostics, |typer| {
        typer.fit_with(
            value,
            expected,
            &format!("the configuration parameter `{name}`"),
            context,
        )
    })
}

fn with_configuration_typer<T>(
    program: &Program,
    declaration: (usize, usize),
    diagnostics: &mut Diagnostics,
    use_typer: impl FnOnce(&mut Typer<'_>) -> T,
) -> T {
    let owner = &program.modules[declaration.0];
    let Detail::Callable(signature) = &owner.details[declaration.1] else {
        unreachable!("configuration is read in a callable declaration");
    };
    let (bindings, written, calls) = (HashMap::new(), HashMap::new(), HashMap::new());
    let selecting = RefCell::new(Vec::new());
    let mut typer = Typer::bare(
        program,
        owner,
        &owner.declarations[declaration.1],
        signature,
        Tables {
            bindings: &bindings,
            written: &written,
            calls: &calls,
            selecting: &selecting,
        },
        diagnostics,
    );
    use_typer(&mut typer)
}
