//! FMT-1 to FMT-5, FMT-7 and FMT-9 at a selected native value call.

use std::cell::RefCell;

use hgl_diagnostics::{Category, Diagnostics, Location, Severity};
use hgl_lexer::literal::string_source_range;
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode};
use hgl_source::Span;

use super::{Tables, Typer, calls::call_callee, fit::not_null, tables};
use crate::declared::names_parameter;
use crate::formatting::{
    self, PATTERN_DECLARATIONS, PatternFault, PatternMemo, PatternText, Piece,
};
use crate::hir::{Argument, Expr, ExprKind, Form, TextCall, Value, ValueCategory};
use crate::syntax::{child, name_token, ungrouped};
use crate::{ConstantValue, DeclarationKind, Detail, Role, Scalar, Signature, Type, codes, error};

impl Typer<'_> {
    /// Check the argument slots and each type's specifier (FMT-3).
    pub(super) fn format_arguments(&mut self, kind: &mut ExprKind) {
        let ExprKind::Call {
            callee,
            arguments,
            text: planned,
            ..
        } = kind
        else {
            return;
        };
        *planned = None;
        let Some(signature) = formatting::value_signature(self.program, callee) else {
            return;
        };
        let Some(pattern) = formatting::argument(signature, arguments, "fmt_str") else {
            return;
        };
        if pattern.value.ty == Type::Unknown || matches!(pattern.value.kind, ExprKind::Unread) {
            return;
        }
        let text =
            match formatting::constant_pattern_in(self.program, &pattern.value, &self.instance) {
                Err(PatternFault::Unread) => self
                    .node_at(pattern.value.at)
                    .ok_or(PatternFault::Unread)
                    .and_then(|source| {
                        self.fold_source_pattern(
                            &source,
                            PATTERN_DECLARATIONS,
                            &mut PatternMemo::default(),
                        )
                    }),
                known => known,
            };
        let text = match text {
            Ok(text) => text,
            Err(PatternFault::Limit) => return,
            Err(PatternFault::Unread) => {
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
            }
        };
        let before = self.diagnostics.len();
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
            // TST-3 temporarily reads an eval's target as composition,
            // but its trace cells and const operands remain value code.
            if place.name == "engine_time" && (!self.graph || self.value_code) {
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
            // FMT-9: a native call made from a graph may ask for its
            // cycle's evaluation time. Neither clock place consumes an
            // argument, and both use datetime's ordinary specifiers.
            let (ty, operand) = if matches!(place.name, "wall_time" | "engine_time") {
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
            let text_type = not_null(Some(effective)).unwrap_or(effective);
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
            } else if formatting::needs_text_conversion(text_type)
                && let Some(index) = operand
                && conversions[index].is_none()
            {
                let at = arguments[index].value.at;
                conversions[index] = self.text_conversion(text_type, at);
                if conversions[index].is_none() {
                    self.diagnostics.emit(error(
                        Category::Operator,
                        codes::OPERATOR_NO_CANDIDATE,
                        at,
                        format!("no value candidate of `convert<str>` takes `{text_type}`"),
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
        if self.diagnostics.as_slice()[before..]
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error)
        {
            return;
        }
        *planned = Some(Box::new(TextCall {
            pattern: text.to_string(),
            conversions: conversions.into_boxed_slice(),
        }));
    }

    /// A string pattern may name a module constant whose initializer
    /// calls a value function (FMT-1, UNIT-13). Read a pure return
    /// expression in that declaration's scope with a bounded call chain.
    fn fold_source_pattern(
        &self,
        source: &SyntaxNode,
        remaining: usize,
        memo: &mut PatternMemo,
    ) -> PatternText {
        use PatternFault::Unread;

        let mut text = String::new();
        let mut pending = vec![source.clone()];
        while let Some(source) = pending.pop() {
            let source = if source.kind() == SyntaxKind::Arg {
                source.children().last().ok_or(Unread)?
            } else {
                source
            };
            let source = ungrouped(source);
            match source.kind() {
                SyntaxKind::Literal => {
                    let Ok(ConstantValue::Str(value)) =
                        self.read_constant(&source, Some(&Type::Scalar(Scalar::Str)))
                    else {
                        return Err(Unread);
                    };
                    formatting::append_pattern(&mut text, &value)?;
                }
                SyntaxKind::BinaryExpr
                    if source.children_with_tokens().any(|child| {
                        child
                            .into_token()
                            .is_some_and(|token| token.kind() == SyntaxKind::Plus)
                    }) && child(&source, SyntaxKind::OperatorConfig).is_none() =>
                {
                    let mut operands = source.children();
                    let left = operands.next().ok_or(Unread)?;
                    let right = operands.next().ok_or(Unread)?;
                    pending.extend([right, left]);
                }
                _ => {
                    let value = self.fold_pattern_reference(&source, remaining, memo)?;
                    formatting::append_pattern(&mut text, &value)?;
                }
            }
        }
        Ok(text.into())
    }

    fn fold_pattern_reference(
        &self,
        source: &SyntaxNode,
        remaining: usize,
        memo: &mut PatternMemo,
    ) -> PatternText {
        use PatternFault::Unread;

        let (name, kind) = if source.kind() == SyntaxKind::NameExpr {
            (name_token(source).ok_or(Unread)?, DeclarationKind::Constant)
        } else {
            let callee = call_callee(source).ok_or(Unread)?;
            if callee.kind() != SyntaxKind::NameExpr
                || child(source, SyntaxKind::ArgList)
                    .ok_or(Unread)?
                    .children()
                    .next()
                    .is_some()
            {
                return Err(Unread);
            }
            (
                name_token(&callee).ok_or(Unread)?,
                DeclarationKind::Function(Role::Value),
            )
        };
        let (module, declarations) = self.declarations(&name).ok_or(Unread)?;
        let [declaration] = declarations else {
            return Err(Unread);
        };
        if self.program.modules[module].declarations[*declaration].kind != kind {
            return Err(Unread);
        }
        memo.declaration((module, *declaration), remaining, |memo, next| {
            self.fold_pattern_declaration(module, *declaration, next, memo)
        })
    }

    fn fold_pattern_declaration(
        &self,
        module: usize,
        declaration: usize,
        remaining: usize,
        memo: &mut PatternMemo,
    ) -> PatternText {
        use PatternFault::Unread;

        let unit = &self.program.modules[module];
        let owner = &unit.declarations[declaration];
        let text = Type::Scalar(Scalar::Str);
        let empty = Signature {
            generics: Vec::new(),
            parameters: Vec::new(),
            result: None,
            reference: false,
        };
        let (expression, signature) = match &unit.details[declaration] {
            Detail::Callable(signature)
                if signature.generics.is_empty()
                    && signature.parameters.is_empty()
                    && signature.result.as_ref() == Some(&text) =>
            {
                let expression = if let Some(arrow) = child(&owner.syntax, SyntaxKind::ArrowBody) {
                    arrow.children().next().ok_or(Unread)?
                } else {
                    let block = child(&owner.syntax, SyntaxKind::Block).ok_or(Unread)?;
                    let mut statements = block.children();
                    let only = statements.next().ok_or(Unread)?;
                    if statements.next().is_some()
                        || !matches!(only.kind(), SyntaxKind::ReturnStmt | SyntaxKind::ExprStmt)
                    {
                        return Err(Unread);
                    }
                    only.children().next().ok_or(Unread)?
                };
                (expression, signature)
            }
            Detail::Constant(ty) if ty == &text => {
                let written = crate::read::type_child(&owner.syntax);
                let expression = owner
                    .syntax
                    .children()
                    .find(|node| Some(node) != written.as_ref())
                    .ok_or(Unread)?;
                (expression, &empty)
            }
            _ => return Err(Unread),
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
        typer.fold_source_pattern(&expression, remaining, memo)
    }

    /// Record the selected ordinary call, with an input supplied by
    /// the formatter when it uses the call.
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
                text: None,
                braced: Vec::new(),
                lifted: false,
            },
            ty: Type::Scalar(Scalar::Str),
            at,
            category: ValueCategory::Value,
        })
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
