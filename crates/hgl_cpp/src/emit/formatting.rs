//! Emit the checked places of Core's native value-format call (FMT-2
//! to FMT-5, FMT-7 and FMT-9). C++ sees individual compiled fmt patterns
//! and typed values. Argument expressions run once in source order.

use std::fmt::Write;

use hgl_analyser::formatting::{self, ChronoPart, Piece};
use hgl_analyser::hir::{Argument, Callee, Expr, ExprKind};
use hgl_analyser::{Scalar, Signature, Type};
use hgl_diagnostics::Location;
use hgl_lower::calls::{LeftOut, left_out};

use super::Emitter;
use super::names::{self, temporary};
use super::statements::Scope;

impl Emitter<'_> {
    /// Write the selected native format call with no runtime format parsing.
    pub(super) fn format_call(
        &mut self,
        scope: &mut Scope,
        callee: &Callee,
        signature: &Signature,
        arguments: &[Argument],
        whole: &Expr,
    ) -> String {
        let Some(pattern) = formatting::argument(signature, arguments, "fmt_str") else {
            return self.missing(whole.at, "a format call without a pattern");
        };
        let Some(text) = formatting::constant_pattern(self.program, &pattern.value) else {
            return self.missing(
                pattern.value.at,
                "a format pattern whose text is not known here",
            );
        };
        let Ok(parts) = formatting::places(&text) else {
            return self.missing(pattern.value.at, "an unchecked format pattern");
        };
        self.piece.formatting = true;
        scope.acted = true;
        let mut statements = String::new();
        let mut held = Vec::with_capacity(arguments.len());
        for argument in arguments {
            if std::ptr::eq(argument, pattern) {
                held.push(String::new());
                continue;
            }
            let value = self.expression(scope, &argument.value);
            let name = temporary("format_argument", self.temporary());
            let _ = write!(statements, "[[maybe_unused]] const auto {name} = {value}; ");
            held.push(name);
        }
        let Some(not_valid) = self.format_option(
            &mut statements,
            callee,
            signature,
            arguments,
            &held,
            "not_valid",
        ) else {
            return self.missing(whole.at, "a format call without its not_valid option");
        };
        let Some(ticked) = self.format_option(
            &mut statements,
            callee,
            signature,
            arguments,
            &held,
            "ticked",
        ) else {
            return self.missing(whole.at, "a format call without its ticked option");
        };
        let output = temporary("format_text", self.temporary());
        let _ = write!(statements, "::hglrt::str {output}; ");
        let mut wall_time = None;
        let mut next_position = 0;
        for part in parts {
            match part {
                Piece::Text(text) => append_text(&mut statements, &output, text),
                Piece::Place(place) => {
                    let (held, ty, at) = if place.name == "wall_time" {
                        let held = wall_time.get_or_insert_with(|| {
                            let name = temporary("wall_time", self.temporary());
                            let _ = write!(
                                statements,
                                "const auto {name} = ::hglrt::format_wall_time(); "
                            );
                            name
                        });
                        (
                            held.as_str(),
                            &Type::Scalar(Scalar::Datetime),
                            pattern.value.at,
                        )
                    } else {
                        let position = next_position;
                        next_position += usize::from(place.name.is_empty());
                        let Some(index) =
                            formatting::argument_index(arguments, place.name, position)
                        else {
                            return self.missing(pattern.value.at, "an unchecked format place");
                        };
                        (
                            held[index].as_str(),
                            &arguments[index].value.ty,
                            arguments[index].value.at,
                        )
                    };
                    if let Some(payload) = formatting::marked_payload(self.program, ty) {
                        if place.marked {
                            let _ = write!(
                                statements,
                                "if ({held}.ticked) {{ {output} += {ticked}; }} "
                            );
                        }
                        let _ = write!(statements, "if ({held}.value) {{ ");
                        self.format_field(
                            scope,
                            &mut statements,
                            &output,
                            &format!("(*{held}.value)"),
                            (payload, at),
                            place.specifier,
                        );
                        let _ = write!(statements, "}} else {{ {output} += {not_valid}; }} ");
                    } else {
                        self.format_field(
                            scope,
                            &mut statements,
                            &output,
                            held,
                            (ty, at),
                            place.specifier,
                        );
                    }
                }
            }
        }
        format!("[&] {{ {statements}return {output}; }}()")
    }

    fn format_option(
        &mut self,
        statements: &mut String,
        callee: &Callee,
        signature: &Signature,
        arguments: &[Argument],
        held: &[String],
        name: &str,
    ) -> Option<String> {
        if let Some(given) = formatting::argument(signature, arguments, name) {
            return arguments
                .iter()
                .zip(held)
                .find_map(|(argument, held)| std::ptr::eq(argument, given).then(|| held.clone()));
        }
        let (index, parameter) = signature
            .parameters
            .iter()
            .enumerate()
            .find(|(_, parameter)| parameter.name == name)?;
        let Some(LeftOut::Default(default)) = left_out(self.program, callee, index, parameter)
        else {
            return None;
        };
        let value = self.write_default(&default)?;
        let held = temporary("format_option", self.temporary());
        let _ = write!(statements, "[[maybe_unused]] const auto {held} = {value}; ");
        Some(held)
    }

    fn format_field(
        &mut self,
        scope: &mut Scope,
        text: &mut String,
        output: &str,
        value: &str,
        (ty, at): (&Type, Location),
        specifier: &str,
    ) {
        match ty {
            Type::Scalar(
                scalar @ (Scalar::Date | Scalar::Time | Scalar::Datetime | Scalar::Duration),
            ) => {
                let Some(plan) = formatting::chrono_specifier(*scalar, specifier) else {
                    self.unsupported(at, "an unchecked chrono specifier");
                    return;
                };
                let temporary_text = temporary("chrono_text", self.temporary());
                if plan.parts.is_empty() {
                    let _ = write!(
                        text,
                        "const auto {temporary_text} = ::hglrt::format_default({value}); "
                    );
                } else {
                    let clock = temporary("chrono_parts", self.temporary());
                    let _ = write!(
                        text,
                        "::hglrt::str {temporary_text}; auto {clock} = ::hglrt::format_clock({value}); "
                    );
                    for part in plan.parts {
                        match part {
                            ChronoPart::Text(literal) => {
                                append_text(text, &temporary_text, literal)
                            }
                            ChronoPart::Directive {
                                letter,
                                total,
                                precision,
                            } => {
                                let _ = write!(
                                    text,
                                    "::hglrt::format_directive({temporary_text}, {clock}, '{letter}', {total}, {precision}); "
                                );
                            }
                        }
                    }
                }
                if plan.padding.is_empty() {
                    let _ = write!(text, "{output} += {temporary_text}; ");
                } else {
                    format_scalar(text, output, &temporary_text, plan.padding);
                }
            }
            Type::Scalar(Scalar::Str) if specifier.is_empty() => {
                let _ = write!(text, "{output} += {value}; ");
            }
            Type::Scalar(Scalar::I64 | Scalar::F64 | Scalar::Bool) if specifier.is_empty() => {
                let _ = write!(
                    text,
                    "{output} += ::hglrt::convert_{ty}<::hglrt::str>({value}); "
                );
            }
            Type::Scalar(Scalar::I64 | Scalar::F64 | Scalar::Str | Scalar::Bool) => {
                format_scalar(text, output, value, specifier);
            }
            _ => {
                let converted = self.convert_to_text(scope, value, ty, at);
                let _ = write!(text, "{output} += {converted}; ");
            }
        }
    }

    /// The selected native collection conversion uses the ordinary
    /// selected converter of each member type (FMT-5, OPS-4).
    pub(super) fn collection_text(
        &mut self,
        scope: &mut Scope,
        value: &Expr,
        members: &[&Type],
    ) -> String {
        self.piece.formatting = true;
        scope.acted = true;
        let rendered = self.expression(scope, value);
        let renderers: Vec<_> = members
            .iter()
            .map(|ty| {
                let part = temporary("collection_member", self.temporary());
                let content = self.collection_member(scope, &part, ty, value.at);
                format!("[&](const auto& {part}) -> ::hglrt::str {{ return {content}; }}")
            })
            .collect();
        let function = if renderers.len() == 1 {
            "collection"
        } else {
            "dictionary"
        };
        format!(
            "::hglrt::format_{function}({rendered}, {})",
            renderers.join(", ")
        )
    }

    fn collection_member(
        &mut self,
        scope: &mut Scope,
        value: &str,
        ty: &Type,
        at: Location,
    ) -> String {
        match ty {
            Type::Nullable(inner) => {
                let text = self.collection_member(scope, &format!("(*{value})"), inner, at);
                format!("({value} ? {text} : ::hglrt::str{{\"null\"}})")
            }
            Type::Scalar(Scalar::Str) => format!("::hglrt::format_quoted({value})"),
            Type::Scalar(Scalar::I64 | Scalar::F64 | Scalar::Bool) => {
                format!("::hglrt::convert_{ty}<::hglrt::str>({value})")
            }
            Type::Scalar(Scalar::Date | Scalar::Time | Scalar::Datetime | Scalar::Duration) => {
                format!("::hglrt::format_default({value})")
            }
            _ => self.convert_to_text(scope, value, ty, at),
        }
    }

    fn convert_to_text(
        &mut self,
        scope: &mut Scope,
        value: &str,
        ty: &Type,
        at: Location,
    ) -> String {
        let Some(call) = formatting::text_call(self.program, ty, at) else {
            return self.missing(at, &format!("the text conversion of `{ty}`"));
        };
        let ExprKind::Call { arguments, .. } = &call.kind else {
            unreachable!("text conversion is a call");
        };
        // A formatter formats the value it already evaluated. Bind the
        // synthetic conversion's input to the held C++ value, then write
        // the same selected call as an ordinary HGL `convert<str>` call.
        let previous = scope
            .read_as
            .replace((arguments[0].value.clone(), value.to_string()));
        let written = self.expression(scope, &call);
        scope.read_as = previous;
        written
    }
}

fn append_text(statements: &mut String, output: &str, text: &str) {
    let _ = write!(
        statements,
        "{output}.append({}, {}); ",
        names::string(text),
        text.len()
    );
}

fn format_scalar(statements: &mut String, output: &str, value: &str, specifier: &str) {
    let compile = names::string(&format!("{{:{specifier}}}"));
    let _ = write!(
        statements,
        "::fmt::format_to(::std::back_inserter({output}), ::fmt::literals::operator\"\"_cf<{compile}>(), {value}); "
    );
}
