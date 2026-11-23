//! FMT-1 to FMT-5, FMT-7 and FMT-9 at a selected native value call.

use std::cell::RefCell;

use hgl_diagnostics::{Category, Diagnostics, Location};
use hgl_lexer::literal::string_source_range;
use hgl_parser::HasSpan;
use hgl_parser::ast::AstNode;
use hgl_source::Span;

use super::{Tables, Typer, tables};
use crate::declared::names_parameter;
use crate::formatting::{self, Piece};
use crate::hir::{Callee, Expr, ExprKind, Value};
use crate::{ConstantValue, Declaration, Program, Scalar, Signature, Type, codes, error};

impl Typer<'_> {
    /// Check the argument slots and each type's specifier (FMT-3).
    pub(super) fn format_arguments(&mut self, kind: &ExprKind) {
        let ExprKind::Call {
            callee, arguments, ..
        } = kind
        else {
            return;
        };
        let Some(signature) = formatting::value_signature(self.program, callee) else {
            return;
        };
        let Some(pattern) = formatting::argument(signature, arguments, "fmt_str") else {
            return;
        };
        if pattern.value.ty == Type::Unknown || matches!(pattern.value.kind, ExprKind::Unread) {
            return;
        }
        let written = self.node_at(pattern.value.at);
        let constant = if let ExprKind::Literal(Value::Str(text)) = &pattern.value.kind {
            Some(ConstantValue::Str(text.clone()))
        } else {
            written
                .as_ref()
                .and_then(|node| {
                    self.read_constant(node, Some(&Type::Scalar(Scalar::Str)))
                        .ok()
                })
                .map(|constant| {
                    constant.substituted(&|at| {
                        let (_, argument) = self
                            .instance
                            .iter()
                            .find(|(parameter, _)| *parameter == at)?;
                        match argument {
                            crate::Argument::Constant(constant) => Some(constant.value.clone()),
                            crate::Argument::Type(_) => None,
                        }
                    })
                })
        };
        let text = match constant {
            Some(ConstantValue::Str(text)) => Some(text),
            // Deferred selection can retain a generic value in its
            // typed argument even after the instance has bound it.
            _ => formatting::constant_pattern_in(self.program, &pattern.value, &self.instance),
        };
        let Some(text) = text else {
            if (self.graph || self.value_code) && !self.is_fixed(&pattern.value, &[]) {
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::ARGUMENT_CONSTANT,
                    pattern.value.at,
                    "a format pattern takes a compile-time string".to_string(),
                    "FMT-1",
                ));
            }
            return;
        };
        let Some(parts) = self.checked_places(&pattern.value, &text) else {
            return;
        };
        let mut used = vec![false; arguments.len()];
        let mut next_position = 0;
        let mut missing = false;
        for part in parts {
            let Piece::Place(place) = part else {
                continue;
            };
            if place.name == "engine_time" {
                missing = true;
                self.format_fault(
                    &pattern.value,
                    place.range,
                    codes::FORMAT_ARGUMENT,
                    "an evaluation time is unavailable in value code".to_string(),
                    "FMT-9",
                );
                continue;
            }
            let (ty, operand) = if place.name == "wall_time" {
                (&Type::Scalar(Scalar::Datetime), None)
            } else {
                let position = next_position;
                next_position += usize::from(place.name.is_empty());
                let Some(index) = formatting::argument_index(arguments, place.name, position)
                else {
                    missing = true;
                    let message = if place.name.is_empty() {
                        "this place has no positional argument left".to_string()
                    } else {
                        format!("this place has no argument named `{}`", place.name)
                    };
                    self.format_fault(
                        &pattern.value,
                        place.range,
                        codes::FORMAT_ARGUMENT,
                        message,
                        "FMT-3",
                    );
                    continue;
                };
                used[index] = true;
                (&arguments[index].value.ty, Some(&arguments[index].value))
            };
            let payload = formatting::marked_payload(self.program, ty);
            let effective = payload.unwrap_or(ty);
            if names_parameter(effective) || effective == &Type::Unknown {
                continue;
            }
            if place.marked && payload.is_none() {
                let at = place.range.start + 1;
                self.format_fault(
                    &pattern.value,
                    at..at + 1,
                    codes::FORMAT_SPECIFIER,
                    "a mark in value code takes a Marked argument".to_string(),
                    "FMT-9",
                );
                continue;
            }
            if !admits_specifier(effective, place.specifier) {
                self.format_fault(
                    &pattern.value,
                    place.specifier_start..place.range.end - 1,
                    codes::FORMAT_SPECIFIER,
                    format!(
                        "a value of type `{effective}` does not admit the specifier `{}`",
                        place.specifier
                    ),
                    "FMT-4",
                );
            } else if formatting::needs_text_conversion(effective)
                && let Some(operand) = operand
                && let Some(written) = self.node_at(operand.at)
                && self.text_converter(written, effective).is_none()
            {
                self.diagnostics.emit(error(
                    Category::Operator,
                    codes::OPERATOR_NO_CANDIDATE,
                    operand.at,
                    format!("no value candidate of `convert<str>` takes `{effective}`"),
                    "FMT-5",
                ));
            }
        }
        if !missing {
            for (argument, used) in arguments.iter().zip(used) {
                if !used && argument.pack.is_some() {
                    self.diagnostics.emit(error(
                        Category::Type,
                        codes::FORMAT_UNUSED,
                        argument.value.at,
                        "no format place uses this argument".to_string(),
                        "FMT-3",
                    ));
                }
            }
        }
    }

    fn checked_places<'s>(&mut self, pattern: &Expr, text: &'s str) -> Option<Vec<Piece<'s>>> {
        match formatting::places(text) {
            Ok(parts) => Some(parts),
            Err(brace) => {
                self.format_fault(
                    pattern,
                    brace,
                    codes::FORMAT_SYNTAX,
                    "this brace opens or closes no format place".to_string(),
                    "FMT-3",
                );
                None
            }
        }
    }

    fn format_fault(
        &mut self,
        pattern: &Expr,
        decoded: std::ops::Range<usize>,
        code: &'static str,
        message: String,
        rule: &'static str,
    ) {
        let at = self
            .node_at(pattern.at)
            .filter(|_| matches!(pattern.kind, ExprKind::Literal(Value::Str(_))))
            .and_then(|node| node.first_token())
            .and_then(|token| {
                let source = string_source_range(token.text(), decoded)?;
                let start = token.span().start;
                Some(Location::new(
                    pattern.at.file,
                    Span::new(start + source.start as u32, start + source.end as u32),
                ))
            })
            .unwrap_or(pattern.at);
        self.diagnostics
            .emit(error(Category::Type, code, at, message, rule));
    }
}

/// Select in the source scope of a typed conversion. A test's private
/// candidates remain visible to its own formatting and collection calls.
pub(crate) fn text_converter(program: &Program, ty: &Type, at: Location) -> Option<Callee> {
    let (module, file, source) = program.modules.iter().find_map(|module| {
        let (file, source) = module
            .files
            .iter()
            .enumerate()
            .find(|(_, source)| source.file == at.file)?;
        Some((module, file, source))
    })?;
    let syntax = source.syntax.syntax();
    let mut declaration = Declaration::stand_in("format conversion", at, file, syntax, false);
    declaration.test_owner = module.tests.iter().position(|test| {
        let span = test.syntax.span();
        test.file == file && span.start <= at.span.start && at.span.end <= span.end
    });
    let signature = Signature {
        generics: Vec::new(),
        parameters: Vec::new(),
        result: None,
        reference: false,
    };
    let (bindings, written, calls) = tables(module);
    let selecting = RefCell::new(Vec::new());
    let mut diagnostics = Diagnostics::new();
    let mut typer = Typer::bare(
        program,
        module,
        &declaration,
        &signature,
        Tables {
            bindings: &bindings,
            written: &written,
            calls: &calls,
            selecting: &selecting,
        },
        &mut diagnostics,
    );
    let written = syntax
        .descendants()
        .find(|node| node.span() == at.span)
        .unwrap_or_else(|| syntax.clone());
    typer.text_converter(written, ty)
}

fn admits_specifier(ty: &Type, specifier: &str) -> bool {
    match ty {
        Type::Scalar(scalar @ (Scalar::I64 | Scalar::F64 | Scalar::Str | Scalar::Bool)) => {
            formatting::scalar_specifier(*scalar, specifier)
        }
        Type::Scalar(
            scalar @ (Scalar::Date | Scalar::Time | Scalar::Datetime | Scalar::Duration),
        ) => formatting::chrono_specifier(*scalar, specifier).is_some(),
        _ => specifier.is_empty(),
    }
}
