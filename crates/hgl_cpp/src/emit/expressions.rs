//! Writes the expressions of value code as C++. Part of the transpiler,
//! the last stage.
//!
//! | HGL | C++ |
//! | --- | --- |
//! | `7`, `2.5`, `true`, `"text"`, `5m`, `1Y2M`, `2B` | `7`, `2.5`, `true`, `::hgraph::Str{"text"}`, `::hgraph::TimeDelta{300000000}`, `::hgraph::Tenor::months_days(14, 0)`, `::hgraph::Tenor::business_days(2)` |
//! | `@2026-09-03`, `@09:30`, `@2026-09-03T09:30Z`, `@2026-09-03T09:30` | hgraph's `Date`, `Time`, `DateTime` and `CivilDateTime` of them |
//! | `@2026-09-03T09:30+01:00[Europe/London]` | `ZonedDateTime::from_resolved` of the instant, the zone and the offset written |
//! | `null`, `value == null` | `std::nullopt`, `!(value).has_value()` (ERR-12: where a test has proved a binding is not null, it is read as `*binding`) |
//! | `Side.buy` | `Side::buy` |
//! | `(a, b)`, `[a, b]` | `std::tuple<A, B>{a, b}`, `::hglrt::list<T>{a, b}`, each written in order |
//! | `items[i]`, `pair[0]` | `::hglrt::item(items, i)`, which fails for a position outside the list (EXPR-3); `std::get<0>(pair)` |
//! | `a && b`, `!a` | `(a && b)`, `(!a)` (EXPR-6) |
//! | `if c { a } else { b }` | `(c ? T(a) : T(b))`, `T` the `if`'s type (CTRL-10) |
//! | `{ statements; value }` | a lambda called where it is written, where the statements do not return |
//! | `ok(v)`, `err(e)`, `is_ok(r)`, `r.value`, `r.error` | `result<T>{v}`, `result<T>{std::unexpect, e}`, `(r).has_value()`, `(*r)`, `(r).error()` (ERR-2, ERR-3) |
//! | `error(message: m, code: c)`, `e.message`, `e.code` | `::hglrt::make_error(m, c)`, `(e).message`, `(e).code` (ERR-4) |
//! | `value?` | the result held before the statement, the function left with its failure, and `(*held)` (ERR-5) |
//! | A constant `name` | `name()` (UNIT-13) |
//!
//! A form not written yet is reported, and `0` stands in its place: the
//! piece is left out of the files.

use hgl_analyser::hir::{
    BinaryOperator, ErrorPart, Expr, ExprKind, Stmt, Tenor, UnaryOperator, Value,
};
use hgl_analyser::{Scalar, Type};
use hgl_diagnostics::Location;

use super::Emitter;
use super::calls::{changes, may_fail};
use super::names::{self, identifier};
use super::statements::{Scope, refers, returns};
use super::types::enum_name;
use super::values::INDENT;
use crate::runtime;

impl Emitter<'_> {
    /// `value` as a C++ expression. A form that is not written yet is
    /// reported, and `0` stands in its place: the piece is left out.
    pub(super) fn expression(&mut self, scope: &mut Scope, value: &Expr) -> String {
        if let Some((place, written)) = &scope.read_as
            && place == value
        {
            return written.clone();
        }
        match &value.kind {
            ExprKind::Literal(literal) => self.literal(literal, &value.ty, value.at),
            ExprKind::Local(local) => {
                let declared = &scope.body.locals[*local];
                let name = identifier(&declared.name);
                // ERR-12: a binding a test has proved is not null is read
                // as its payload's type there.
                match &declared.ty {
                    Type::Nullable(payload) if **payload == value.ty => format!("(*{name})"),
                    _ => name,
                }
            }
            // UNIT-13: "The constant has that one value wherever it is
            // read".
            ExprKind::Constant {
                module,
                declaration,
            } => {
                self.piece.reach.uses.insert((*module, *declaration, None));
                format!(
                    "{}()",
                    names::function(self.program, *module, *declaration, None)
                )
            }
            ExprKind::EnumMember {
                module,
                declaration,
                member,
            } => {
                let Some(hgl_analyser::Detail::Enum(read)) =
                    self.program.modules[*module].details.get(*declaration)
                else {
                    return self.missing(value.at, "this member");
                };
                self.note_enum(*module, *declaration);
                format!(
                    "{}::{}",
                    enum_name(self.program, *module, *declaration),
                    identifier(&read.members[*member].name)
                )
            }
            // EXPR-5, NUM-1: an `i64` taken as the `f64` of the same
            // number.
            ExprKind::Widen(inner) => {
                let inner = self.expression(scope, inner);
                format!("static_cast<::hgraph::Float>({inner})")
            }
            // EXPR-6: `&&` and `||` short-circuit left to right, as
            // C++'s do.
            ExprKind::Binary {
                operator: operator @ (BinaryOperator::And | BinaryOperator::Or),
                left,
                right,
            } => {
                let symbol = if *operator == BinaryOperator::And {
                    "&&"
                } else {
                    "||"
                };
                let left = self.expression(scope, left);
                scope.conditional += 1;
                let right = self.expression(scope, right);
                scope.conditional -= 1;
                format!("({left} {symbol} {right})")
            }
            // ERR-12: a test against `null` is the language's own.
            ExprKind::Binary {
                operator: operator @ (BinaryOperator::Equal | BinaryOperator::NotEqual),
                left,
                right,
            } if is_null(left) || is_null(right) => {
                let tested = if is_null(left) { right } else { left };
                let tested = self.expression(scope, tested);
                if *operator == BinaryOperator::Equal {
                    format!("(!({tested}).has_value())")
                } else {
                    format!("({tested}).has_value()")
                }
            }
            ExprKind::Unary {
                operator: UnaryOperator::Not,
                operand,
            } => {
                let operand = self.expression(scope, operand);
                format!("(!{operand})")
            }
            ExprKind::Call {
                callee, arguments, ..
            } => self.call(scope, callee, arguments, value),
            ExprKind::Tuple(entries) => {
                let Some(ty) = self.written_type(&value.ty, value.at) else {
                    return "0".to_string();
                };
                self.constructed(scope, &ty, entries)
            }
            // EXPR-9: a sequence is what its type says it makes: a list
            // here.
            ExprKind::Sequence(entries) if matches!(value.ty, Type::List { .. }) => {
                let Some(ty) = self.written_type(&value.ty, value.at) else {
                    return "0".to_string();
                };
                self.constructed(scope, &ty, entries)
            }
            // EXPR-7: an element of a list, checked when it runs; a
            // position of a tuple, which is a constant (FN-3).
            ExprKind::Index { base, index } => match &base.ty {
                Type::Tuple(_) => {
                    let ExprKind::Literal(Value::Int(position)) = index.kind else {
                        return self.missing(index.at, "a tuple's position that is no literal");
                    };
                    let base = self.expression(scope, base);
                    format!("std::get<{position}>({base})")
                }
                // EXPR-2: the list, then the index. Where they are held
                // (`held_read`), the list is held first, and the element
                // read is a value of its own. The list is held by
                // reference only where it is what a local or a constant
                // holds (`refers`) and the index changes nothing: any
                // other may be part of a value that ends with its
                // declaration, `[[7]][0]`.
                Type::List { .. } if held_read(base, index) => {
                    let Some(ty) = self.written_type(&value.ty, value.at) else {
                        return "0".to_string();
                    };
                    let held = if refers(base) && !changes(index) {
                        "const auto&"
                    } else {
                        "const auto"
                    };
                    let base = self.expression(scope, base);
                    let index = self.expression(scope, index);
                    scope.acted = true;
                    let list = names::temporary("list", self.temporary());
                    let at = names::temporary("index", self.temporary());
                    format!(
                        "[&]() -> {ty} {{ {held} {list} = {base}; const auto {at} = {index}; return ::{}::item({list}, {at}); }}()",
                        runtime::NAMESPACE
                    )
                }
                Type::List { .. } => {
                    let base = self.expression(scope, base);
                    let index = self.expression(scope, index);
                    scope.acted = true;
                    format!("::{}::item({base}, {index})", runtime::NAMESPACE)
                }
                _ => self.missing(value.at, "this read of an element"),
            },
            ExprKind::Block {
                statements,
                value: inner,
            } if statements.is_empty() => match inner {
                Some(inner) => self.expression(scope, inner),
                None => self.missing(value.at, "a block with no value"),
            },
            ExprKind::Block { .. } => self.evaluated(scope, value),
            // CTRL-10: an `if` that gives a value. One whose branches are
            // values alone is C++'s conditional, each branch taken as the
            // `if`'s type; "With no `else` it is as if its `else` were
            // null".
            ExprKind::If {
                condition,
                then,
                otherwise,
            } if plain(then) && otherwise.as_deref().is_none_or(plain) => {
                let Some(ty) = self.written_type(&value.ty, value.at) else {
                    return "0".to_string();
                };
                let condition = self.expression(scope, condition);
                scope.conditional += 1;
                let then = self.expression(scope, then);
                let otherwise = match otherwise {
                    Some(otherwise) => self.expression(scope, otherwise),
                    None => String::new(),
                };
                scope.conditional -= 1;
                format!("({condition} ? {ty}({then}) : {ty}({otherwise}))")
            }
            ExprKind::If { .. } => self.evaluated(scope, value),
            // ERR-2: a result that owns its payload, of the result type
            // its place expects.
            ExprKind::Tagged { success, payload } => {
                let Some(ty) = self.written_type(&value.ty, value.at) else {
                    return "0".to_string();
                };
                let payload = self.expression(scope, payload);
                if *success {
                    format!("{ty}{{{payload}}}")
                } else {
                    format!("{ty}{{std::unexpect, {payload}}}")
                }
            }
            // ERR-3: `is_ok(value)` and `is_err(value)`.
            ExprKind::TagTest { success, value } => {
                let tested = self.expression(scope, value);
                if *success {
                    format!("({tested}).has_value()")
                } else {
                    format!("(!({tested}).has_value())")
                }
            }
            // ERR-3: the payload a guard has proved there.
            ExprKind::Payload { success, result } => {
                let result = self.expression(scope, result);
                if *success {
                    format!("(*{result})")
                } else {
                    format!("({result}).error()")
                }
            }
            // ERR-4: `error(message: text, code: code)`, with no context.
            // EXPR-2: the message, then the code; where both may fail, or
            // one runs statements, each is held in turn first.
            ExprKind::Error { message, code } => {
                let ordered = changes(message)
                    || code.as_deref().is_some_and(changes)
                    || (may_fail(message) && code.as_deref().is_some_and(may_fail));
                let message = self.expression(scope, message);
                let code = match code {
                    Some(code) => self.expression(scope, code),
                    None => "std::nullopt".to_string(),
                };
                let made = format!("::{}::make_error", runtime::NAMESPACE);
                if ordered {
                    let held = names::temporary("message", self.temporary());
                    let coded = names::temporary("code", self.temporary());
                    format!(
                        "[&] {{ const auto {held} = {message}; const auto {coded} = {code}; return {made}({held}, {coded}); }}()"
                    )
                } else {
                    format!("{made}({message}, {code})")
                }
            }
            ExprKind::ErrorPart { base, part } => {
                let base = self.expression(scope, base);
                match part {
                    ErrorPart::Message => format!("({base}).message"),
                    ErrorPart::Code => format!("({base}).code"),
                    _ => self.missing(value.at, "this part of an error"),
                }
            }
            ExprKind::Propagate(operand) => self.propagated(scope, operand, value.at),
            ExprKind::Eval { .. } => self.missing(value.at, "an eval"),
            // TST-4: the harness's own comparison of two traces; what
            // either side lacks is reported first.
            ExprKind::TraceCompare { left, right, .. } => {
                self.expression(scope, left);
                self.expression(scope, right);
                self.missing(value.at, "a comparison of traces")
            }
            ExprKind::Trace(_) => self.missing(value.at, "a trace"),
            // A struct, a set and a dictionary are not written yet: each
            // is reported by its type.
            ExprKind::Construct { .. } | ExprKind::Sequence(_) | ExprKind::Pairs(_) => {
                self.missing(value.at, &format!("the type `{}`", value.ty))
            }
            // TYPE-7: `Side(1)`, `Side("buy")`.
            ExprKind::EnumFrom { .. } => {
                self.missing(value.at, "an enum's member made from a value")
            }
            _ => self.missing(value.at, "this expression"),
        }
    }

    /// Reports `what` at `at` as not written yet, and gives what stands
    /// in its place.
    pub(super) fn missing(&mut self, at: Location, what: &str) -> String {
        self.unsupported(at, what);
        "0".to_string()
    }

    /// The entries of a tuple or a list, each as its place expects, in
    /// the order written: C++ evaluates the entries of a braced list in
    /// order (EXPR-13).
    /// `ty{entries...}`, a tuple or a list, its entries evaluated in
    /// the order written (EXPR-2). Where an entry after the first runs
    /// statements, which may change what an earlier one reads, each is
    /// first held as a value of its own, of its own type: `std::tuple`'s
    /// constructor would copy an earlier entry only after the later ones
    /// had run.
    fn constructed(&mut self, scope: &mut Scope, ty: &str, entries: &[Expr]) -> String {
        if !entries.iter().skip(1).any(changes) {
            let written: Vec<String> = entries
                .iter()
                .map(|entry| self.expression(scope, entry))
                .collect();
            return format!("{ty}{{{}}}", written.join(", "));
        }
        let mut steps = String::new();
        let mut held = Vec::new();
        for entry in entries {
            let Some(entry_ty) = self.written_type(&entry.ty, entry.at) else {
                return "0".to_string();
            };
            let written = self.expression(scope, entry);
            let name = names::temporary("entry", self.temporary());
            steps.push_str(&format!("const {entry_ty} {name} = {written}; "));
            held.push(name);
        }
        format!(
            "[&]() -> {ty} {{ {steps}return {ty}{{{}}}; }}()",
            held.join(", ")
        )
    }

    /// A block or an `if` that gives a value and has statements of its
    /// own, as a lambda called where it is written, which runs them and
    /// gives the value. One whose statements may return from the
    /// function, or need what a `?` writes before them, is not written
    /// yet.
    fn evaluated(&mut self, scope: &mut Scope, value: &Expr) -> String {
        let Some(statements) = returning(value) else {
            return self.missing(value.at, "a block or an `if` with statements, as a value");
        };
        if returns(&statements_of(value)) {
            return self.missing(value.at, "a block or an `if` that returns, as a value");
        }
        let Some(ty) = self.written_type(&value.ty, value.at) else {
            return "0".to_string();
        };
        let mut text = format!("[&]() -> {ty} {{\n");
        let before = std::mem::take(&mut scope.before);
        scope.conditional += 1;
        self.statements(scope, &statements, 1, &mut text);
        scope.conditional -= 1;
        scope.before = before;
        text.push_str("}()");
        scope.acted = true;
        text
    }

    /// `operand?` (ERR-5): the result is held under a name of its own
    /// before the statement, and a failure leaves the function with its
    /// error; the expression is the success's payload. Only where it is
    /// run before anything else in its statement that may fail, and in
    /// a function that gives a result.
    fn propagated(&mut self, scope: &mut Scope, operand: &Expr, at: Location) -> String {
        let gives_result = matches!(scope.result, Some(Type::Result { .. }));
        if scope.conditional > 0 || scope.acted || !gives_result || scope.testing {
            return self.missing(at, "a `?` here");
        }
        let written = self.expression(scope, operand);
        let held = names::temporary("propagated", self.temporary());
        scope.before.push(format!("const auto {held} = {written};"));
        scope.before.push(format!(
            "if (!{held}.has_value())\n{{\n{INDENT}return std::unexpected({held}.error());\n}}"
        ));
        format!("(*{held})")
    }

    /// A literal of type `ty` as C++.
    fn literal(&mut self, literal: &Value, ty: &Type, at: Location) -> String {
        match literal {
            Value::Int(number) => integer_literal(*number),
            // `{:?}` writes the shortest text that reads back as the
            // same `f64`, always with a `.` or an exponent, as C++ reads
            // a double.
            Value::Float(number) if number.is_finite() => {
                let text = format!("{number:?}");
                if text.starts_with('-') {
                    format!("({text})")
                } else {
                    text
                }
            }
            Value::Bool(true) => "true".to_string(),
            Value::Bool(false) => "false".to_string(),
            Value::Str(text) => {
                let bytes = if *ty == Type::Scalar(Scalar::Bytes) {
                    "::hgraph::Bytes"
                } else {
                    "::hgraph::Str"
                };
                let literal = names::string(text);
                // A text that holds a NUL is given its length, which the
                // literal alone would end at the NUL.
                let made = if text.contains('\0') {
                    format!("::hgraph::Str({literal}, {})", text.len())
                } else {
                    format!("::hgraph::Str{{{literal}}}")
                };
                if bytes == "::hgraph::Bytes" {
                    format!("::hgraph::Bytes{{{made}}}")
                } else {
                    made
                }
            }
            Value::Null => "std::nullopt".to_string(),
            // TYPE-17: a tenor of months and days, and one of business
            // days, which are two forms: zero of one is not zero of the
            // other.
            Value::Tenor(Tenor::Moves { months, days }) => format!(
                "::hgraph::Tenor::months_days({}, {})",
                integer_literal(*months),
                integer_literal(*days)
            ),
            Value::Tenor(Tenor::BusinessDays(count)) => format!(
                "::hgraph::Tenor::business_days({})",
                integer_literal(*count)
            ),
            Value::Duration(micros) => format!("::hgraph::TimeDelta{{{micros}}}"),
            Value::Temporal(temporal) => match temporal_literal(temporal, ty) {
                Some(written) => written,
                None => self.missing(at, &format!("a literal of `{ty}`")),
            },
            _ => self.missing(at, "this literal"),
        }
    }
}

/// True where an element read of the list `base` at `index` holds the
/// list, then the index, before it reads, so that they are evaluated in
/// that order (EXPR-2): where either runs statements, which may change
/// what the other reads, and where both may fail, which C++ would
/// evaluate in either order. The element read is then a value of its own.
pub(super) fn held_read(base: &Expr, index: &Expr) -> bool {
    changes(base) || changes(index) || (may_fail(base) && may_fail(index))
}

/// An `i64` as a C++ literal. The least has none: its negation is out of
/// range, so it is written as a difference.
pub(super) fn integer_literal(number: i64) -> String {
    match number {
        i64::MIN => "(-9223372036854775807 - 1)".to_string(),
        negative if negative < 0 => format!("({negative})"),
        number => number.to_string(),
    }
}

/// A temporal literal (LEX-11) of type `ty` as C++, where the target
/// writes its type: a `date`, a `time`, a `datetime`, a `civil_datetime`
/// and a `zoned_datetime`. Each part was checked where the literal was
/// read.
fn temporal_literal(temporal: &hgl_analyser::hir::Temporal, ty: &Type) -> Option<String> {
    let date = temporal.date.map(|date| {
        format!(
            "::hgraph::Date{{std::chrono::year{{{}}}, std::chrono::month{{{}}}, std::chrono::day{{{}}}}}",
            date.year, date.month, date.day
        )
    });
    let time = temporal.time;
    match ty {
        Type::Scalar(Scalar::Date) => date,
        Type::Scalar(Scalar::Time) => {
            let time = time?;
            Some(format!(
                "::hgraph::time_of_day({}, {}, {}, {})",
                time.hour, time.minute, time.second, time.microsecond
            ))
        }
        Type::Scalar(Scalar::CivilDatetime) => {
            let time = time?;
            Some(format!(
                "::hgraph::CivilDateTime{{{}, {}, {}, {}, {}}}",
                date?, time.hour, time.minute, time.second, time.microsecond
            ))
        }
        Type::Scalar(Scalar::Datetime) => {
            let micros = instant(temporal)?;
            Some(format!(
                "::hgraph::DateTime{{std::chrono::microseconds{{{micros}}}}}"
            ))
        }
        Type::Scalar(Scalar::ZonedDatetime) => {
            let micros = instant(temporal)?;
            let zone = temporal.zone.as_deref()?;
            let offset = i64::from(temporal.offset?) * 60;
            Some(format!(
                "::hgraph::ZonedDateTime::from_resolved(::hgraph::DateTime{{std::chrono::microseconds{{{micros}}}}}, ::hgraph::ZoneId{{{}}}, {offset})",
                names::string(zone)
            ))
        }
        _ => None,
    }
}

/// The instant a literal with a date, a time and an offset writes, in
/// microseconds from the epoch in UTC.
fn instant(temporal: &hgl_analyser::hir::Temporal) -> Option<i64> {
    let date = temporal.date?;
    let time = temporal.time?;
    let offset = i64::from(temporal.offset?);
    let days = days_from_civil(
        i64::from(date.year),
        i64::from(date.month),
        i64::from(date.day),
    );
    let seconds = days * 86_400
        + i64::from(time.hour) * 3_600
        + i64::from(time.minute) * 60
        + i64::from(time.second)
        - offset * 60;
    Some(seconds * 1_000_000 + i64::from(time.microsecond))
}

/// The days from 1970-01-01 to the Gregorian date `year-month-day`
/// (Howard Hinnant's `days_from_civil`).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let of_era = year - era * 400;
    let of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let of_cycle = of_era * 365 + of_era / 4 - of_era / 100 + of_year;
    era * 146_097 + of_cycle - 719_468
}

/// The statements that give what `value`, a block or an `if`, gives,
/// as a function's do: each value a branch ends with is the function's
/// `return`. `None` where a branch has no value to give.
fn returning(value: &Expr) -> Option<Vec<Stmt>> {
    match &value.kind {
        ExprKind::Block {
            statements,
            value: Some(last),
        } => {
            let mut all = statements.clone();
            all.extend(returning(last)?);
            Some(all)
        }
        ExprKind::If {
            condition,
            then,
            otherwise: Some(otherwise),
        } => {
            let branch = |branch: &Expr| -> Option<Box<Expr>> {
                Some(Box::new(Expr {
                    kind: ExprKind::Block {
                        statements: returning(branch)?,
                        value: None,
                    },
                    ty: branch.ty.clone(),
                    at: branch.at,
                    category: branch.category,
                }))
            };
            Some(vec![Stmt::Expr(Expr {
                kind: ExprKind::If {
                    condition: condition.clone(),
                    then: branch(then)?,
                    otherwise: Some(branch(otherwise)?),
                },
                ty: value.ty.clone(),
                at: value.at,
                category: value.category,
            })])
        }
        ExprKind::Block { value: None, .. }
        | ExprKind::If {
            otherwise: None, ..
        } => None,
        _ => Some(vec![Stmt::Return {
            value: Some(value.clone()),
            at: value.at,
        }]),
    }
}

/// The statements written in `value`, a block or an `if`, at any depth
/// of its branches.
fn statements_of(value: &Expr) -> Vec<Stmt> {
    let mut found = Vec::new();
    match &value.kind {
        ExprKind::Block {
            statements,
            value: last,
        } => {
            found.extend(statements.iter().cloned());
            if let Some(last) = last {
                found.extend(statements_of(last));
            }
        }
        ExprKind::If {
            then, otherwise, ..
        } => {
            found.extend(statements_of(then));
            if let Some(otherwise) = otherwise {
                found.extend(statements_of(otherwise));
            }
        }
        _ => {}
    }
    found
}

/// True for `null`, written as itself.
fn is_null(value: &Expr) -> bool {
    matches!(value.kind, ExprKind::Literal(Value::Null))
}

/// True where `branch`, a branch of an `if`, is a value alone: a block
/// with no statement, or an expression.
fn plain(branch: &Expr) -> bool {
    match &branch.kind {
        ExprKind::Block {
            statements,
            value: Some(value),
        } => statements.is_empty() && plain(value),
        ExprKind::Block { value: None, .. } => false,
        ExprKind::If {
            then, otherwise, ..
        } => plain(then) && otherwise.as_deref().is_some_and(plain),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::days_from_civil;

    #[test]
    fn days_are_counted_from_the_epoch() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
        assert_eq!(days_from_civil(1969, 12, 31), -1);
        assert_eq!(days_from_civil(2026, 9, 3), 20_699);
    }
}
