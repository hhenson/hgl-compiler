//! Checks omitted inherited configuration defaults after candidate selection.
//!
//! The operator owns the default (GEN-6). The declaration pass binds and
//! checks the expression in its defining scope (UNIT-7). With generic
//! arguments in place, the selected parameter gives it a type (UNIT-8).

use std::cell::RefCell;
use std::collections::HashMap;

use hgl_diagnostics::{Category, Diagnostics, Location};
use hgl_parser::HasSpan;

use super::fit::{open, whole};
use super::{Tables, Typer, lifting};
use crate::declared::{names_parameter, substitute_within};
use crate::hir::{self, Callee, Expr, ExprKind, Value};
use crate::program::Program;
use crate::types::{Argument, Detail, Parameter, Type};
use crate::{codes, error};

/// Visit the selected calls once the declaration pass has read every default.
pub(super) fn check(program: &Program, diagnostics: &mut Diagnostics) {
    for module in &program.modules {
        let mut visit = |value: &Expr| check_expression(program, value, diagnostics);
        for body in module
            .bodies
            .iter()
            .flatten()
            .chain(module.test_bodies.iter().flatten())
            .chain(&module.store_body)
        {
            hir::walk_statements(&body.statements, &mut visit);
        }
        if let Some(run) = &module.run_body {
            run.call.walk(&mut visit);
        }
        for constant in module.constants.iter().flatten() {
            constant.walk(&mut visit);
        }
        for (defaults, declaration) in module.defaults.iter().zip(&module.declarations) {
            let span = declaration.syntax.span();
            for default in defaults.iter().flatten() {
                // Check inherited defaults at their original declarations.
                if default.at.file == declaration.at.file
                    && span.start <= default.at.span.start
                    && default.at.span.end <= span.end
                {
                    default.walk(&mut visit);
                }
            }
        }
    }
}

fn check_expression(program: &Program, value: &Expr, diagnostics: &mut Diagnostics) {
    match &value.kind {
        ExprKind::Call {
            callee, arguments, ..
        } => {
            check_call(
                program,
                callee,
                |slot| {
                    arguments
                        .iter()
                        .any(|argument| argument.parameter == Some(slot))
                },
                diagnostics,
            );
        }
        ExprKind::StaticCallable(callee) if let Callee::Anonymous(function) = callee.as_ref() => {
            hir::walk_statements(&function.body.statements, &mut |value| {
                check_expression(program, value, diagnostics);
            });
        }
        _ => {}
    }
}

/// UNIT-8: check only the inherited defaults that this selected call uses.
pub(super) fn check_call(
    program: &Program,
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
        program.specialize_operator_default(
            default,
            *operator,
            operator_generics,
            &concrete,
            diagnostics,
        );
    }
}

/// A configuration parameter with a default of its own.
fn configuration_default(parameter: &Parameter) -> bool {
    parameter.configuration && parameter.has_default
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
    let expected = &parameter.ty;
    if value.ty == *expected
        || names_parameter(expected)
        || !whole(expected)
        || names_parameter(&value.ty)
    {
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
                    "`null` is no default of `{}`, which takes `{expected}`",
                    parameter.name
                ),
                "TYPE-14",
            ));
        }
        return value;
    }
    let owner = &program.modules[operator.0];
    let Detail::Callable(signature) = &owner.details[operator.1] else {
        return value;
    };
    let (bindings, written, calls) = (HashMap::new(), HashMap::new(), HashMap::new());
    let selecting = RefCell::new(Vec::new());
    let mut typer = Typer::bare(
        program,
        owner,
        &owner.declarations[operator.1],
        signature,
        Tables {
            bindings: &bindings,
            written: &written,
            calls: &calls,
            selecting: &selecting,
        },
        diagnostics,
    );
    let mut value = typer.fit(
        value,
        expected,
        &format!("the configuration parameter `{}`", parameter.name),
    );
    lifting::record_value(program, &mut value);
    value
}
