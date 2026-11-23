//! FMT-1 to FMT-5, FMT-7 and FMT-9 at a selected native value call.

use std::cell::RefCell;

use hgl_diagnostics::{Category, Diagnostics, Location};
use hgl_lexer::literal::string_source_range;
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode};
use hgl_source::Span;

use super::{Tables, Typer, calls::call_callee, tables};
use crate::declared::names_parameter;
use crate::formatting::{self, Piece};
use crate::hir::{Argument, Expr, ExprKind, Form, Value, ValueCategory};
use crate::syntax::{child, name_token, ungrouped};
use crate::{ConstantValue, DeclarationKind, Detail, Role, Scalar, Signature, Type, codes, error};

impl Typer<'_> {
    /// Check the argument slots and each type's specifier (FMT-3).
    pub(super) fn format_arguments(&mut self, kind: &mut ExprKind) {
        let ExprKind::Call {
            callee,
            arguments,
            text_conversions,
            ..
        } = kind
        else {
            return;
        };
        let Some(signature) = formatting::value_signature(self.program, callee) else {
            if let [argument] = arguments.as_slice()
                && let Some(members) =
                    formatting::collection_members(self.program, callee, &argument.value.ty)
            {
                *text_conversions = self
                    .member_conversions(&members, argument.value.at)
                    .unwrap_or_default();
            }
            return;
        };
        let Some(pattern) = formatting::argument(signature, arguments, "fmt_str") else {
            return;
        };
        let pattern_index = arguments
            .iter()
            .position(|given| std::ptr::eq(given, pattern))
            .expect("the pattern is a given argument");
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
        let folded = if text.is_none() {
            written
                .as_ref()
                .and_then(|source| self.fold_source_pattern(source, 64))
        } else {
            None
        };
        let folded_call = folded.is_some();
        let Some(text) = text.or(folded) else {
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
        let mut conversions = vec![None; arguments.len()];
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
                (&arguments[index].value.ty, Some(index))
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
                && let Some(index) = operand
                && conversions[index].is_none()
            {
                let at = arguments[index].value.at;
                conversions[index] = self.text_conversion(effective, at);
                if conversions[index].is_none() {
                    self.diagnostics.emit(error(
                        Category::Operator,
                        codes::OPERATOR_NO_CANDIDATE,
                        at,
                        format!("no value candidate of `convert<str>` takes `{effective}`"),
                        "FMT-5",
                    ));
                }
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
        *text_conversions = conversions.into_boxed_slice();
        if folded_call {
            arguments[pattern_index].value.kind = ExprKind::Literal(Value::Str(text));
        }
    }

    /// A string pattern may name a module constant whose initializer
    /// calls a value function (FMT-1, UNIT-13). Read a pure return
    /// expression in that declaration's scope with a bounded call chain.
    fn fold_source_pattern(&self, source: &SyntaxNode, remaining: usize) -> Option<String> {
        let next = remaining.checked_sub(1)?;
        let source = if source.kind() == SyntaxKind::Arg {
            source.children().last()?
        } else {
            source.clone()
        };
        let source = ungrouped(source);
        let text = Type::Scalar(Scalar::Str);
        let (module, declaration, is_call) = match self.read_constant(&source, Some(&text)) {
            Ok(ConstantValue::Str(text)) => return Some(text),
            Ok(ConstantValue::Named {
                module,
                declaration,
            }) => (module, declaration, false),
            _ => {
                let callee = call_callee(&source)?;
                if callee.kind() != SyntaxKind::NameExpr
                    || child(&source, SyntaxKind::ArgList)?
                        .children()
                        .next()
                        .is_some()
                {
                    return None;
                }
                let (module, declarations) = self.declarations(&name_token(&callee)?)?;
                let [declaration] = declarations else {
                    return None;
                };
                (module, *declaration, true)
            }
        };
        let unit = &self.program.modules[module];
        let owner = &unit.declarations[declaration];
        let empty = Signature {
            generics: Vec::new(),
            parameters: Vec::new(),
            result: None,
            reference: false,
        };
        let (expression, signature) = if is_call {
            let Detail::Callable(signature) = &unit.details[declaration] else {
                return None;
            };
            if owner.kind != DeclarationKind::Function(Role::Value)
                || !signature.generics.is_empty()
                || !signature.parameters.is_empty()
                || signature.result.as_ref() != Some(&text)
            {
                return None;
            }
            let expression = if let Some(arrow) = child(&owner.syntax, SyntaxKind::ArrowBody) {
                arrow.children().next()?
            } else {
                let block = child(&owner.syntax, SyntaxKind::Block)?;
                let mut statements = block.children();
                let only = statements.next()?;
                if statements.next().is_some()
                    || !matches!(only.kind(), SyntaxKind::ReturnStmt | SyntaxKind::ExprStmt)
                {
                    return None;
                }
                only.children().next()?
            };
            (expression, signature)
        } else {
            let written = crate::read::type_child(&owner.syntax);
            let expression = owner
                .syntax
                .children()
                .find(|node| Some(node) != written.as_ref())?;
            (expression, &empty)
        };
        let (bindings, written, calls) = tables(unit);
        let selecting = RefCell::new(Vec::new());
        let mut diagnostics = Diagnostics::new();
        let typer = Typer::bare(
            self.program,
            unit,
            owner,
            signature,
            Tables {
                bindings: &bindings,
                written: &written,
                calls: &calls,
                selecting: &selecting,
            },
            &mut diagnostics,
        );
        typer.fold_source_pattern(&expression, next)
    }

    /// Record the selected ordinary call, with an input supplied by
    /// the formatter or collection renderer when it uses the call.
    fn text_conversion(&mut self, ty: &Type, at: Location) -> Option<Expr> {
        let written = self.node_at(at)?;
        let callee = self.text_converter(written, ty)?;
        let (module, declaration) = callee.declaration()?;
        let Detail::Callable(signature) = &self.program.modules[module].details[declaration] else {
            return None;
        };
        let parameter = signature
            .parameters
            .iter()
            .position(|parameter| !parameter.configuration)?;
        let text_conversions = match formatting::collection_members(self.program, &callee, ty) {
            Some(members) => self.member_conversions(&members, at)?,
            None => Box::default(),
        };
        Some(Expr {
            kind: ExprKind::Call {
                callee,
                arguments: vec![Argument {
                    parameter: Some(parameter),
                    pack: None,
                    value: Expr {
                        kind: ExprKind::Unread,
                        ty: ty.clone(),
                        at,
                        category: ValueCategory::Value,
                    },
                }],
                form: Form::Named,
                text_conversions,
                braced: Vec::new(),
                lifted: false,
            },
            ty: Type::Scalar(Scalar::Str),
            at,
            category: ValueCategory::Value,
        })
    }

    fn member_conversions(
        &mut self,
        members: &[&Type],
        at: Location,
    ) -> Option<Box<[Option<Expr>]>> {
        members
            .iter()
            .map(|ty| {
                let ty = match ty {
                    Type::Nullable(inner) => inner.as_ref(),
                    ty => *ty,
                };
                if formatting::needs_text_conversion(ty)
                    && !names_parameter(ty)
                    && *ty != Type::Unknown
                {
                    Some(Some(self.text_conversion(ty, at)?))
                } else {
                    Some(None)
                }
            })
            .collect()
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
