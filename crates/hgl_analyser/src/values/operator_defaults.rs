//! Checks omitted defaults after function and candidate selection.
//!
//! The operator owns the default (GEN-6). The declaration pass binds and
//! checks the expression in its defining scope (UNIT-7). With generic
//! arguments in place, the selected parameter gives it a type (UNIT-8).

use std::cell::RefCell;
use std::collections::HashMap;

use hgl_diagnostics::{Category, Diagnostics, Location};
use hgl_parser::HasSpan;

use super::constants::parameter_default_type;
use super::fit::{open, whole};
use super::requirements::{IndependentSourceBodies, Made, concrete_default_calls};
use super::{Tables, Typer, lifting};
use crate::declared::{names_parameter, substitute_within};
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
            let span = declaration.syntax.span();
            for default in defaults.iter().flatten() {
                // Check inherited defaults at their original declarations.
                if default.value.at.file == declaration.at.file
                    && span.start <= default.value.at.span.start
                    && default.value.at.span.end <= span.end
                {
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
struct Defaults {
    seen: Vec<DefaultUse>,
    pending: Vec<Expr>,
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
    let mut defaults = Defaults::default();
    defaults.visit(program, value, diagnostics);
    while let Some(default) = defaults.pending.pop() {
        concrete_default_calls(program, &default, made);
        default.walk(&mut |value| defaults.visit(program, value, diagnostics));
    }
}

impl Defaults {
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
                check_declared_defaults(program, self, callee, supplied, diagnostics);
                check_inherited_defaults(program, self, callee, supplied, diagnostics);
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
    diagnostics: &mut Diagnostics,
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
            check_expression(program, value, made, &mut reports);
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
    checking: &mut Defaults,
    callee: &Callee,
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
        let ty = substitute_within(&parameter.ty, &map);
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
            (default, &concrete),
            diagnostics,
        );
    }
}

/// UNIT-8: check only the inherited defaults that this selected call uses.
fn check_inherited_defaults(
    program: &Program,
    checking: &mut Defaults,
    callee: &Callee,
    supplied: impl Fn(usize) -> bool,
    diagnostics: &mut Diagnostics,
) {
    let Callee::Candidate {
        operator,
        operator_generics,
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
        let ty = substitute_within(&parameter.ty, &map);
        let concrete = Parameter {
            ty,
            ..parameter.clone()
        };
        checking.specialize(
            program,
            (*operator, operator_generics, &[]),
            (default, &concrete),
            diagnostics,
        );
    }
}

impl Defaults {
    /// Check a default in its defining scope and follow the calls it uses.
    fn specialize(
        &mut self,
        program: &Program,
        (owner, generics, type_packs): ((usize, usize), &[Argument], &[hir::TypePackArgument]),
        (default, parameter): (&hir::DefaultExpr, &Parameter),
        diagnostics: &mut Diagnostics,
    ) {
        let type_packs = if default.has_type_query() {
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
            if !diagnostics.as_slice().contains(report) {
                diagnostics.emit(report.clone());
            }
        }
        if !reports.has_errors() {
            // GEN-16: the specialized expression can call a candidate
            // that was not known when its source declaration was read.
            self.pending.push(specialized.value);
        }
    }
}

/// An omitted default can depend on generic arguments through its type
/// or through a value read by the expression.
fn dependent_default(parameter: &Parameter, default: &hir::DefaultExpr) -> bool {
    names_parameter(&parameter.ty) || reads_generic_value(default) || default.has_type_query()
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
