//! Emit the checked places of Core's native value-format call (FMT-2,
//! FMT-4, FMT-5, FMT-7 and FMT-9). C++ sees individual compiled fmt patterns
//! and typed values. Argument expressions run once in source order.
//! Core's value `debug_print` calls this formatter and flushes the line
//! through `write` (DBG-1, DBG-3, DBG-9).
//! Graph calls currently lift this value formatter (FMT-6, FMT-8, FN-11),
//! so whole inputs use value rendering until #434 supplies the graph body.

use std::fmt::Write;

use hgl_analyser::formatting::{self, ChronoPart, Piece};
use hgl_analyser::hir::{Argument, Callee, Expr, ExprKind, TextCall};
use hgl_analyser::{Scalar, Signature, Type};
use hgl_diagnostics::Location;
use hgl_lower::calls::{LeftOut, left_out};

use super::Emitter;
use super::names::{self, temporary};
use super::nodes::{Kind, OUT};
use super::statements::Scope;

impl Emitter<'_> {
    /// Write the selected native format call with no runtime format parsing.
    pub(super) fn format_call(
        &mut self,
        scope: &mut Scope,
        callee: &Callee,
        signature: &Signature,
        arguments: &[Argument],
        text: Option<&TextCall>,
        whole: &Expr,
    ) -> String {
        let Some(pattern) = formatting::argument(signature, arguments, "fmt_str") else {
            return self.missing(whole.at, "a format call without a pattern");
        };
        let Some(TextCall {
            pattern: text,
            conversions,
        }) = text
        else {
            return self.missing(
                pattern.value.at,
                "a format pattern whose text is not known here",
            );
        };
        let Ok(parts) = formatting::places(text) else {
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
        let mut engine_time = None;
        let mut next_position = 0;
        for part in parts {
            match part {
                Piece::Text(text) => append_text(&mut statements, &output, text),
                Piece::Place(place) => {
                    let (held, ty, at, conversion) = if matches!(
                        place.name,
                        "wall_time" | "engine_time"
                    ) {
                        // FMT-9: the lifted node already owns its cycle's
                        // time through its output endpoint, not a native
                        // argument. Read each reserved clock only once.
                        let (clock, source) = if place.name == "engine_time" {
                            if self.kind() != Some(Kind::Lifted) {
                                return self.missing(
                                        pattern.value.at,
                                        "an {engine_time} format place without a lifted node's cycle context",
                                    );
                            }
                            (&mut engine_time, format!("{OUT}.evaluation_time()"))
                        } else {
                            (&mut wall_time, "::hglrt::format_wall_time()".to_string())
                        };
                        let held = clock.get_or_insert_with(|| {
                            let name = temporary(place.name, self.temporary());
                            let _ = write!(statements, "const auto {name} = {source}; ");
                            name
                        });
                        (
                            held.as_str(),
                            &Type::Scalar(Scalar::Datetime),
                            pattern.value.at,
                            None,
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
                            conversions.get(index).and_then(Option::as_ref),
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
                            (payload, at, conversion),
                            place.specifier,
                        );
                        let _ = write!(statements, "}} else {{ {output} += {not_valid}; }} ");
                    } else {
                        self.format_field(
                            scope,
                            &mut statements,
                            &output,
                            held,
                            (ty, at, conversion),
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
        (ty, at, conversion): (&Type, Location, Option<&Expr>),
        specifier: &str,
    ) {
        match ty {
            Type::Nullable(inner) => {
                let _ = write!(text, "if ({value}) {{ ");
                self.format_field(
                    scope,
                    text,
                    output,
                    &format!("(*{value})"),
                    (inner, at, conversion),
                    specifier,
                );
                let _ = write!(text, "}} else {{ {output} += \"null\"; }} ");
            }
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
            Type::Scalar(Scalar::Str) if specifier == "?" => {
                let _ = write!(text, "{output} += ::hglrt::format_quoted({value}); ");
            }
            Type::Scalar(Scalar::I64 | Scalar::F64 | Scalar::Bool) if specifier.is_empty() => {
                let _ = write!(
                    text,
                    "{output} += ::hglrt::convert_{ty}<::hglrt::str>({value}); "
                );
            }
            Type::Scalar(scalar @ (Scalar::I64 | Scalar::F64 | Scalar::Str | Scalar::Bool)) => {
                if let Some(layout) = formatting::text_layout(*scalar, specifier) {
                    let fill = names::string(layout.fill);
                    let padding = format!(
                        "::hglrt::TextPadding{{::std::string_view{{{fill}, {}}}, {}, '{}'}}",
                        layout.fill.len(),
                        layout.width,
                        layout.align
                    );
                    if *scalar == Scalar::F64 {
                        let sign = layout.sign.map_or_else(
                            || "::std::nullopt".to_string(),
                            |flag| format!("'{flag}'"),
                        );
                        let _ = write!(
                            text,
                            "::hglrt::format_float_text({output}, {value}, {padding}, {sign}, {}); ",
                            layout.zero
                        );
                    } else {
                        let precision = layout.precision.map_or_else(
                            || "::std::nullopt".to_string(),
                            |length| length.to_string(),
                        );
                        let _ = write!(
                            text,
                            "::hglrt::format_text({output}, {value}, {padding}, {precision}); "
                        );
                    }
                } else {
                    format_scalar(text, output, value, specifier);
                }
            }
            _ => {
                let converted = self.convert_to_text(scope, value, ty, at, conversion);
                let _ = write!(text, "{output} += {converted}; ");
            }
        }
    }

    fn convert_to_text(
        &mut self,
        scope: &mut Scope,
        value: &str,
        ty: &Type,
        at: Location,
        conversion: Option<&Expr>,
    ) -> String {
        let Some(call) = conversion else {
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
        let written = self.expression(scope, call);
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
