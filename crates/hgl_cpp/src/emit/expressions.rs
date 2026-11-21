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
//! | `@[Europe/London]`, `@09:30[Europe/London]` | hgraph's `ZoneId` and `ZonedTime` of them |
//! | A struct, a field, a set, a dictionary, a delta, `Side(1)`, `maybe == 1` | `composites`' |
//!
//! A form not written yet is reported, and `0` stands in its place: the
//! piece is left out of the files.

use hgl_analyser::hir::{
    BinaryOperator, ErrorPart, Expr, ExprKind, Raise, Stmt, Tenor, UnaryOperator, Value,
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
            // EXPR-3: "A compound assignment reads its existing target",
            // its left operand, first (EXPR-13); a read of a list's
            // element is checked, and may fail (EXPR-7).
            scope.read = true;
            if reads_an_element(place) {
                scope.acted = true;
            }
            return written.clone();
        }
        // The temporal forms, and the reads of a hook, are the dispatch
        // point's (`forms`), which a construct lane adds to.
        if let Some(written) = self.form_expression(scope, value) {
            return written;
        }
        if matches!(value.kind, ExprKind::Local(_) | ExprKind::Constant { .. }) {
            scope.read = true;
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
            // read". Its value is made where it is first read, so a read
            // of one whose value may fail is something that may fail: a
            // `?` after it is not run before it (EXPR-2).
            ExprKind::Constant {
                module,
                declaration,
            } => {
                if let Some(Some(value)) = self.program.modules[*module].constants.get(*declaration)
                    && may_fail(value)
                {
                    scope.acted = true;
                }
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
                let equal = *operator == BinaryOperator::Equal;
                let (is, is_not) = if equal {
                    ("true", "false")
                } else {
                    ("false", "true")
                };
                // `null == null`.
                if is_null(tested) {
                    return is.to_string();
                }
                // ERR-12: a binding a test has proved is not null is read
                // as its payload's type; it is tested as the optional it
                // is declared.
                if let ExprKind::Local(local) = tested.kind
                    && matches!(scope.body.locals[local].ty, Type::Nullable(_))
                {
                    scope.read = true;
                    let declared = &scope.body.locals[local];
                    // A node's `const` parameter is its `hgraph::Scalar`,
                    // whose value is the optional (`configuration`).
                    let name = if self.role(local) == super::nodes::Role::Config {
                        super::configuration::read(
                            self.program,
                            &declared.ty,
                            &identifier(&declared.name),
                        )
                    } else {
                        identifier(&declared.name)
                    };
                    return if equal {
                        format!("(!{name}.has_value())")
                    } else {
                        format!("{name}.has_value()")
                    };
                }
                // A value of a type that is no `T?` is never null; it is
                // still evaluated, for what it does.
                if !matches!(tested.ty, Type::Nullable(_)) {
                    let tested = self.expression(scope, tested);
                    return format!("(static_cast<void>({tested}), {is_not})");
                }
                let tested = self.expression(scope, tested);
                if equal {
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
            // TYPE-14, OPS-2: a `T?` compared with a `T`, which the
            // analyser leaves to its symbol (`composites`).
            ExprKind::Binary {
                operator: operator @ (BinaryOperator::Equal | BinaryOperator::NotEqual),
                left,
                right,
            } => {
                match self.nullable_equality(scope, *operator == BinaryOperator::Equal, left, right)
                {
                    Some(written) => written,
                    None => self.missing(value.at, "this expression"),
                }
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
                    format!("::std::get<{position}>({base})")
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
                // ERR-6: a dictionary's value for a key (`composites`).
                Type::Dict { .. } => self.dict_read(scope, value, base, index),
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
                    format!("{ty}{{::std::unexpect, {payload}}}")
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
            // EXPR-2: the two in the order written, named arguments in
            // any order; where both may fail, or one runs statements,
            // each is held in turn first.
            ExprKind::Error { message, code } => {
                let ordered = changes(message)
                    || code.as_deref().is_some_and(changes)
                    || (may_fail(message) && code.as_deref().is_some_and(may_fail));
                let code_first = code
                    .as_deref()
                    .is_some_and(|code| code.at.span.start < message.at.span.start);
                let (message, code) = if code_first {
                    let code = code.as_deref().map(|code| self.expression(scope, code));
                    (self.expression(scope, message), code)
                } else {
                    let message = self.expression(scope, message);
                    (
                        message,
                        code.as_deref().map(|code| self.expression(scope, code)),
                    )
                };
                let code = code.unwrap_or_else(|| "::std::nullopt".to_string());
                let made = format!("::{}::make_error", runtime::NAMESPACE);
                if ordered {
                    let held = names::temporary("message", self.temporary());
                    let coded = names::temporary("code", self.temporary());
                    let message = format!("const auto {held} = {message};");
                    let code = format!("const auto {coded} = {code};");
                    let (first, second) = if code_first {
                        (code, message)
                    } else {
                        (message, code)
                    };
                    format!("[&] {{ {first} {second} return {made}({held}, {coded}); }}()")
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
            // ERR-5, ERR-15: a result given where its T is expected, in a
            // value function that gives a result: its failure leaves the
            // function. Every other place a failure is raised, a result's
            // `T?` and a `T?` read where a T is expected are not written
            // yet.
            ExprKind::Raised {
                result,
                raise: Raise::Return,
            } => self.propagated(scope, result, value.at),
            ExprKind::Raised { raise, .. } => self.missing(value.at, raised_where(*raise)),
            ExprKind::OrNull { .. } => {
                self.missing(value.at, "a result given where its `T?` is expected")
            }
            ExprKind::NullRead { .. } => {
                self.missing(value.at, "a `T?` read where its T is expected")
            }
            ExprKind::Eval { .. } => self.missing(value.at, "an eval"),
            // TST-4: the harness's own comparison of two traces; what
            // either side lacks is reported first.
            ExprKind::TraceCompare { left, right, .. } => {
                self.expression(scope, left);
                self.expression(scope, right);
                self.missing(value.at, "a comparison of traces")
            }
            ExprKind::Trace(_) => self.missing(value.at, "a trace"),
            // A struct, a set, a dictionary and a delta (`composites`).
            ExprKind::Construct { arguments, .. } => self.construct(scope, value, arguments),
            ExprKind::Field { base, member } => self.field(scope, value, base, *member),
            ExprKind::Delta { arguments } => self.delta(scope, value, arguments),
            ExprKind::Sequence(entries) => self.set_literal(scope, value, entries),
            ExprKind::Pairs(entries) => self.dict_literal(scope, value, entries),
            ExprKind::ParentList(inner) => self.parent_list(scope, value, inner),
            // TYPE-7: `Side(1)`, `Side("buy")`.
            ExprKind::EnumFrom {
                module,
                declaration,
                value: from,
            } => self.enum_from(scope, value, (*module, *declaration), from),
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
    pub(super) fn constructed(&mut self, scope: &mut Scope, ty: &str, entries: &[Expr]) -> String {
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

    /// A result given where its T is expected in a value function that
    /// gives a result (ERR-5: "the function gives that E as its
    /// failure"): the result is held under a name of its own before the
    /// statement, and a failure leaves the function with its error; the
    /// expression is the success's payload. Only where it is the first
    /// thing its statement evaluates: nothing before it in the statement
    /// has been run that may fail, or read a local, a constant or a
    /// place, which it would otherwise run before (EXPR-2, EXPR-13).
    fn propagated(&mut self, scope: &mut Scope, operand: &Expr, at: Location) -> String {
        let gives_result = matches!(scope.result, Some(Type::Result { .. }));
        if scope.conditional > 0 || scope.acted || !gives_result || scope.testing {
            return self.missing(at, "a result raised here");
        }
        if scope.read {
            return self.missing(at, "a result raised after a read in its statement");
        }
        let written = self.expression(scope, operand);
        let held = names::temporary("propagated", self.temporary());
        scope.before.push(format!("const auto {held} = {written};"));
        scope.before.push(format!(
            "if (!{held}.has_value())\n{{\n{INDENT}return ::std::unexpected({held}.error());\n}}"
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
            Value::Null => "::std::nullopt".to_string(),
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

/// True where the place `place` is, at any depth, an element of a list
/// or an entry of a dictionary, whose read is checked (EXPR-7, ERR-6).
pub(super) fn reads_an_element(place: &Expr) -> bool {
    match &place.kind {
        // A dictionary's entry is read checked as a list's element is
        // (ERR-6), through a struct's field (`composites`).
        ExprKind::Index { base, .. } => {
            matches!(base.ty, Type::List { .. } | Type::Dict { .. }) || reads_an_element(base)
        }
        ExprKind::Field { base, .. } => reads_an_element(base),
        _ => false,
    }
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
            "::hgraph::Date{{::std::chrono::year{{{}}}, ::std::chrono::month{{{}}}, ::std::chrono::day{{{}}}}}",
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
                "::hgraph::DateTime{{::std::chrono::microseconds{{{micros}}}}}"
            ))
        }
        // TYPE-2: "named `timezone`" and "date-independent `zoned_time`".
        // LEX-12: "The selected run provider checks exact catalog
        // membership and offset consistency": the library checks each
        // zone, and a `zoned_datetime`'s offset, where it is evaluated.
        Type::Scalar(Scalar::Timezone) => Some(format!(
            "::{}::zone_literal({})",
            runtime::NAMESPACE,
            names::string(temporal.zone.as_deref()?)
        )),
        Type::Scalar(Scalar::ZonedTime) => {
            let time = time?;
            Some(format!(
                "::{}::zoned_time_literal(::hgraph::time_of_day({}, {}, {}, {}), {})",
                runtime::NAMESPACE,
                time.hour,
                time.minute,
                time.second,
                time.microsecond,
                names::string(temporal.zone.as_deref()?)
            ))
        }
        Type::Scalar(Scalar::ZonedDatetime) => {
            let micros = instant(temporal)?;
            let zone = temporal.zone.as_deref()?;
            let offset = i64::from(temporal.offset?) * 60;
            Some(format!(
                "::{}::zoned_datetime_literal(::hgraph::DateTime{{::std::chrono::microseconds{{{micros}}}}}, {offset}, {})",
                runtime::NAMESPACE,
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

/// What a result raised as `raise` says, and not written here, is reported
/// as (ERR-5): each place a failure is raised that the target does not
/// write yet.
pub(super) fn raised_where(raise: Raise) -> &'static str {
    match raise {
        Raise::Return => "a result raised here",
        Raise::Execution => "a result raised as an execution failure",
        Raise::Node => "a result raised in a `when` here",
        Raise::Source => "a result raised in a source's body",
        Raise::Lifecycle(_) => "a result raised in a lifecycle hook",
        Raise::Build => "a result raised where a graph is built",
        Raise::Test => "a result raised among a test's statements",
        Raise::Application => "a result raised among an application's statements",
        Raise::CallNode => "a call's result connection taken as its T",
        Raise::Reader => "a held result connection taken as its T",
    }
}
