//! Literal values: from a literal token's text to the value it denotes.
//!
//! The lexer settles only a literal's *shape*. This module settles its
//! *value*, and in doing so applies the rules that need arithmetic: unit
//! order and exact scaling for durations (LEX-13), calendar validity for
//! dates and times (LEX-11), zone-name limits (LEX-12), and the numeric
//! domains (LEX-7).
//!
//! Each `decode_*` function takes the token's text and is pure: no source
//! map, no diagnostics sink. [`validate_literals`] runs them over a token
//! list and reports what they reject.
//!
//! Nothing here consults a clock or a time-zone database. Whether a zone
//! name exists is a provider check at run time (LEX-12), not a property of
//! the source text.

use hgl_diagnostics::{Category, Code, Diagnostic, Diagnostics, Location};
use hgl_source::{FileId, SourceMap};

use crate::lexer::DURATION_UNITS;
use crate::token::{Token, TokenKind};

/// The compiler-local diagnostic codes for literal values. As with the
/// lexer's codes, the specification rejects these conditions without
/// cataloguing a code for them.
pub mod codes {
    /// An integer literal above the i64 domain (LEX-7).
    pub const INTEGER_RANGE: &str = "lex.integer_range";
    /// A float literal too large for binary64 (LEX-7).
    pub const FLOAT_RANGE: &str = "lex.float_range";
    /// A duration that breaks LEX-13.
    pub const INVALID_DURATION: &str = "lex.invalid_duration";
    /// A date or time whose fields do not exist (LEX-11).
    pub const INVALID_TEMPORAL: &str = "lex.invalid_temporal";
    /// A zone name that breaks LEX-12.
    pub const INVALID_ZONE: &str = "lex.invalid_zone";
}

/// Why a literal's text has no value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiteralError {
    /// The text does not have the literal's shape. The lexer produces such
    /// a token only together with its own diagnostic, so this needs no
    /// second report.
    Malformed,
    /// The text has the right shape but breaks a value rule.
    Invalid {
        /// The local code, one of [`codes`].
        code: &'static str,
        /// The specification rule broken, for example `LEX-13`.
        rule: &'static str,
        /// The explanation.
        message: String,
    },
}

/// Shorthand for building [`LiteralError::Invalid`] as an `Err`.
fn invalid<T>(
    code: &'static str,
    rule: &'static str,
    message: impl Into<String>,
) -> Result<T, LiteralError> {
    Err(LiteralError::Invalid {
        code,
        rule,
        message: message.into(),
    })
}

// ---- Strings --------------------------------------------------------------

/// The contents of a string literal, with the quotes removed and escapes
/// replaced (LEX-2). `text` is the whole token, quotes included.
pub fn decode_string(text: &str) -> Result<String, LiteralError> {
    // `strip_prefix` returns the remainder when the prefix is present. The
    // `?` after `ok_or` returns the error from this function straight away,
    // like an early `return` on failure.
    let body = text.strip_prefix('"').ok_or(LiteralError::Malformed)?;
    let mut value = String::with_capacity(body.len());
    let mut characters = body.chars();
    while let Some(character) = characters.next() {
        match character {
            // The closing quote must be the last character of the token.
            '"' => {
                return if characters.next().is_none() {
                    Ok(value)
                } else {
                    Err(LiteralError::Malformed)
                };
            }
            '\\' => match characters.next() {
                Some('"') => value.push('"'),
                Some('\\') => value.push('\\'),
                Some('n') => value.push('\n'),
                Some('r') => value.push('\r'),
                Some('t') => value.push('\t'),
                _ => return Err(LiteralError::Malformed),
            },
            '\n' | '\r' => return Err(LiteralError::Malformed),
            other => value.push(other),
        }
    }
    // Ran out of text without meeting the closing quote.
    Err(LiteralError::Malformed)
}

// ---- Numbers --------------------------------------------------------------

/// The largest magnitude an integer literal may have: 2^63.
///
/// 2^63 itself is not an `i64`; it is admitted here because the source
/// `-9223372036854775808` is a minus operator applied to this literal
/// (LEX-7). Whether the context permits that is for the analyser, which
/// sees the minus.
pub const MAX_INTEGER_MAGNITUDE: u64 = 1 << 63;

/// The magnitude of an integer literal (LEX-7). Literals carry no sign, so
/// the result is unsigned and at most [`MAX_INTEGER_MAGNITUDE`].
pub fn decode_integer(text: &str) -> Result<u64, LiteralError> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(LiteralError::Malformed);
    }
    // `parse` fails here only on overflow: the text is known to be digits.
    match text.parse::<u64>() {
        Ok(magnitude) if magnitude <= MAX_INTEGER_MAGNITUDE => Ok(magnitude),
        _ => invalid(
            codes::INTEGER_RANGE,
            "LEX-7",
            format!("integer literal `{text}` is outside the i64 domain"),
        ),
    }
}

/// The value of a float literal (LEX-7). The nearest binary64 value is
/// taken; a literal too large to be finite is rejected.
pub fn decode_float(text: &str) -> Result<f64, LiteralError> {
    let value: f64 = text.parse().map_err(|_| LiteralError::Malformed)?;
    if value.is_finite() {
        Ok(value)
    } else {
        invalid(
            codes::FLOAT_RANGE,
            "LEX-7",
            format!("float literal `{text}` is outside the f64 domain"),
        )
    }
}

// ---- Durations ------------------------------------------------------------

/// Microseconds in each duration unit, in the order of
/// [`DURATION_UNITS`]: d, h, m, s, ms, us. One day is exactly 86,400
/// seconds (LEX-13).
const UNIT_MICROSECONDS: [i128; 6] = [
    86_400_000_000,
    3_600_000_000,
    60_000_000,
    1_000_000,
    1_000,
    1,
];

/// The value of a duration literal in microseconds (LEX-13, TYPE-2).
///
/// Checks that units appear once each in descending order, that only the
/// final part has a fraction, that the fraction scales to a whole number of
/// microseconds, and that the total fits the signed 64-bit duration domain.
/// The result is never negative: a sign is a separate operator (LEX-7).
pub fn decode_duration(text: &str) -> Result<i64, LiteralError> {
    let parts = duration_parts(text)?;
    let reject = |message: String| invalid(codes::INVALID_DURATION, "LEX-13", message);

    // Arithmetic is done in i128 so that intermediate products cannot
    // overflow before the final range check.
    let mut total: i128 = 0;
    let mut previous_unit: Option<usize> = None;
    for (index, part) in parts.iter().enumerate() {
        if let Some(previous) = previous_unit {
            let (earlier, later) = (DURATION_UNITS[previous], DURATION_UNITS[part.unit]);
            if part.unit == previous {
                return reject(format!(
                    "duration `{text}` uses the unit `{later}` more than once"
                ));
            }
            if part.unit < previous {
                return reject(format!(
                    "duration `{text}` is out of order: `{later}` cannot follow `{earlier}`; \
                     units go from largest to smallest"
                ));
            }
        }
        previous_unit = Some(part.unit);

        let is_final = index + 1 == parts.len();
        if !part.fraction.is_empty() && !is_final {
            return reject(format!(
                "duration `{text}` has a fraction before its final part"
            ));
        }

        let unit = UNIT_MICROSECONDS[part.unit];
        let whole: i128 = match part.whole.parse::<u64>() {
            Ok(whole) => whole.into(),
            Err(_) => return reject(format!("duration `{text}` is outside the duration domain")),
        };
        total += whole * unit + fraction_microseconds(text, part.fraction, unit)?;
    }
    match i64::try_from(total) {
        Ok(microseconds) => Ok(microseconds),
        Err(_) => reject(format!("duration `{text}` is outside the duration domain")),
    }
}

/// One `number unit` part of a duration, still as text.
struct DurationPart<'a> {
    /// The digits before any `.`.
    whole: &'a str,
    /// The digits after the `.`, or empty.
    fraction: &'a str,
    /// Index of the unit in [`DURATION_UNITS`]; a smaller index is a
    /// larger unit.
    unit: usize,
}

/// Splits a duration literal into its parts, or reports that the text does
/// not have the duration shape.
fn duration_parts(text: &str) -> Result<Vec<DurationPart<'_>>, LiteralError> {
    /// Splits `text` after its leading run of bytes satisfying `accept`.
    fn split_run(text: &str, accept: impl Fn(u8) -> bool) -> (&str, &str) {
        let length = text.bytes().take_while(|&byte| accept(byte)).count();
        text.split_at(length)
    }

    let mut parts = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let (whole, after_whole) = split_run(rest, |byte| byte.is_ascii_digit());
        let (fraction, after_number) = match after_whole.strip_prefix('.') {
            Some(after_dot) => split_run(after_dot, |byte| byte.is_ascii_digit()),
            None => ("", after_whole),
        };
        let (unit_text, after_unit) = split_run(after_number, |byte| byte.is_ascii_alphabetic());
        let unit = DURATION_UNITS.iter().position(|&unit| unit == unit_text);
        let dot_without_digits = after_whole.starts_with('.') && fraction.is_empty();
        // `let ... else` binds on success and runs the `else` block (which
        // must leave the function) otherwise.
        let Some(unit) = unit else {
            return Err(LiteralError::Malformed);
        };
        if whole.is_empty() || dot_without_digits {
            return Err(LiteralError::Malformed);
        }
        parts.push(DurationPart {
            whole,
            fraction,
            unit,
        });
        rest = after_unit;
    }
    if parts.is_empty() {
        Err(LiteralError::Malformed)
    } else {
        Ok(parts)
    }
}

/// The microseconds contributed by the fraction digits of a part whose unit
/// is `unit` microseconds long. LEX-13 requires the scaling to be exact:
/// `0.5ms` is 500 microseconds, but `0.5us` has no whole-microsecond value.
fn fraction_microseconds(text: &str, fraction: &str, unit: i128) -> Result<i128, LiteralError> {
    // Trailing zeros do not change the value: `1.50s` is `1.5s`.
    let digits = fraction.trim_end_matches('0');
    if digits.is_empty() {
        return Ok(0);
    }
    let inexact = || {
        invalid(
            codes::INVALID_DURATION,
            "LEX-13",
            format!("duration `{text}` is not a whole number of microseconds"),
        )
    };
    // The largest unit is under 10^11 microseconds, so more than 11
    // significant fraction digits can never scale exactly. Bounding the
    // digit count also keeps the arithmetic below within i128.
    if digits.len() > 11 {
        return inexact();
    }
    let numerator: i128 = digits.parse::<i128>().expect("at most 11 digits") * unit;
    let denominator = 10_i128.pow(digits.len() as u32);
    if numerator % denominator == 0 {
        Ok(numerator / denominator)
    } else {
        inexact()
    }
}

// ---- Temporal literals ----------------------------------------------------

/// A calendar date in the proleptic Gregorian calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Date {
    /// Year, 1 to 9999 (TYPE-2).
    pub year: u16,
    /// Month, 1 to 12.
    pub month: u8,
    /// Day of the month, 1 to the month's length.
    pub day: u8,
}

/// A time of day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Time {
    /// Hour, 0 to 23.
    pub hour: u8,
    /// Minute, 0 to 59.
    pub minute: u8,
    /// Second, 0 to 59. Zero when the literal omits seconds (LEX-11).
    pub second: u8,
    /// Microsecond within the second, 0 to 999,999.
    pub microsecond: u32,
}

/// An offset from UTC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UtcOffset {
    /// Minutes east of UTC; negative for west. `Z` is zero.
    pub minutes: i16,
}

/// The fields written in a temporal literal.
///
/// Which fields are present records the literal's form; mapping a form to
/// an HGL type (`date`, `civil_datetime`, `datetime`, ...) is the
/// analyser's job, under the value contracts in types-and-values.md.
///
/// | Source                              | date | time | offset | zone |
/// | ----------------------------------- | ---- | ---- | ------ | ---- |
/// | `@2026-09-03`                       | yes  |      |        |      |
/// | `@2026-09-03T09:30`                 | yes  | yes  |        |      |
/// | `@2026-09-03T09:30Z`                | yes  | yes  | yes    |      |
/// | `@2026-09-03T09:30+01:00[Europe/London]` | yes | yes | yes | yes  |
/// | `@09:30`                            |      | yes  |        |      |
/// | `@09:30[Europe/London]`             |      | yes  |        | yes  |
/// | `@[Europe/London]`                  |      |      |        | yes  |
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TemporalLiteral<'a> {
    /// The calendar date, if written.
    pub date: Option<Date>,
    /// The time of day, if written.
    pub time: Option<Time>,
    /// The UTC offset, if written.
    pub offset: Option<UtcOffset>,
    /// The zone name without its brackets, if written. It borrows from the
    /// literal's text (that is what the `'a` records).
    pub zone: Option<&'a str>,
}

/// The fields of a temporal literal (LEX-11, LEX-12). `text` is the whole
/// token, `@` included.
///
/// Field values are checked: the date must exist, the time must be a real
/// time of day (no `24:00:00`, no leap second), and a zone name must meet
/// LEX-12's limits.
pub fn decode_temporal(text: &str) -> Result<TemporalLiteral<'_>, LiteralError> {
    let mut reader = FieldReader { text, pos: 0 };
    if !reader.eat(b'@') {
        return Err(LiteralError::Malformed);
    }
    let mut literal = TemporalLiteral {
        date: None,
        time: None,
        offset: None,
        zone: None,
    };

    if reader.peek() == Some(b'[') {
        literal.zone = Some(reader.zone()?);
    } else if reader.peek_at(4) == Some(b'-') {
        literal.date = Some(reader.date()?);
        if reader.eat(b'T') {
            literal.time = Some(reader.time()?);
            if matches!(reader.peek(), Some(b'Z' | b'+' | b'-')) {
                literal.offset = Some(reader.offset()?);
                if reader.peek() == Some(b'[') {
                    literal.zone = Some(reader.zone()?);
                }
            }
        }
    } else {
        literal.time = Some(reader.time()?);
        if reader.peek() == Some(b'[') {
            literal.zone = Some(reader.zone()?);
        }
    }

    // Anything left over means the text was not one complete literal.
    if reader.pos == text.len() {
        Ok(literal)
    } else {
        Err(LiteralError::Malformed)
    }
}

/// Days in `month` of `year` in the Gregorian calendar. A year is a leap
/// year when divisible by 4, except century years not divisible by 400.
fn days_in_month(year: u16, month: u8) -> u8 {
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Reads the fields of a temporal literal from left to right.
struct FieldReader<'a> {
    text: &'a str,
    pos: usize,
}

impl<'a> FieldReader<'a> {
    fn peek(&self) -> Option<u8> {
        self.peek_at(0)
    }

    fn peek_at(&self, ahead: usize) -> Option<u8> {
        self.text.as_bytes().get(self.pos + ahead).copied()
    }

    /// Consumes `byte` if it is next, and says whether it did.
    fn eat(&mut self, byte: u8) -> bool {
        let present = self.peek() == Some(byte);
        if present {
            self.pos += 1;
        }
        present
    }

    /// Consumes `byte`, or fails: the text does not have the literal shape.
    fn expect(&mut self, byte: u8) -> Result<(), LiteralError> {
        if self.eat(byte) {
            Ok(())
        } else {
            Err(LiteralError::Malformed)
        }
    }

    /// Reads exactly `count` digits as a number.
    fn number(&mut self, count: usize) -> Result<u32, LiteralError> {
        let mut value = 0;
        for _ in 0..count {
            match self.peek() {
                Some(digit) if digit.is_ascii_digit() => {
                    value = value * 10 + u32::from(digit - b'0');
                    self.pos += 1;
                }
                _ => return Err(LiteralError::Malformed),
            }
        }
        Ok(value)
    }

    /// `YYYY-MM-DD`, which must name a real day (LEX-11).
    fn date(&mut self) -> Result<Date, LiteralError> {
        let start = self.pos;
        let year = self.number(4)?;
        self.expect(b'-')?;
        let month = self.number(2)?;
        self.expect(b'-')?;
        let day = self.number(2)?;
        let written = &self.text[start..self.pos];

        // The casts below are safe: four digits fit u16, two fit u8.
        let (year, month, day) = (year as u16, month as u8, day as u8);
        let reject = |message: String| invalid(codes::INVALID_TEMPORAL, "LEX-11", message);
        if year == 0 {
            return reject(format!("date `{written}` is outside the years 1 to 9999"));
        }
        if !(1..=12).contains(&month) {
            return reject(format!("date `{written}` has no month {month:02}"));
        }
        if day == 0 || day > days_in_month(year, month) {
            return reject(format!(
                "date `{written}` does not exist: {year:04}-{month:02} has no day {day:02}"
            ));
        }
        Ok(Date { year, month, day })
    }

    /// `HH:MM[:SS[.ffffff]]`. Seconds default to zero; `24:00:00` and leap
    /// seconds are excluded (LEX-11).
    fn time(&mut self) -> Result<Time, LiteralError> {
        let start = self.pos;
        let hour = self.number(2)?;
        self.expect(b':')?;
        let minute = self.number(2)?;
        let mut second = 0;
        let mut microsecond = 0;
        if self.eat(b':') {
            second = self.number(2)?;
            if self.eat(b'.') {
                // One to six digits; `.5` means 500,000 microseconds.
                let mut digits = 0;
                while digits < 6 && self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
                    microsecond = microsecond * 10 + self.number(1)?;
                    digits += 1;
                }
                if digits == 0 {
                    return Err(LiteralError::Malformed);
                }
                microsecond *= 10_u32.pow(6 - digits);
            }
        }
        let written = &self.text[start..self.pos];
        if hour > 23 || minute > 59 || second > 59 {
            return invalid(
                codes::INVALID_TEMPORAL,
                "LEX-11",
                format!(
                    "time `{written}` does not exist: hours are 00 to 23, minutes and seconds 00 to 59"
                ),
            );
        }
        Ok(Time {
            hour: hour as u8,
            minute: minute as u8,
            second: second as u8,
            microsecond,
        })
    }

    /// `Z`, `+HH`, `-HH`, `+HH:MM` or `-HH:MM`. Minutes default to zero
    /// (LEX-11).
    fn offset(&mut self) -> Result<UtcOffset, LiteralError> {
        let start = self.pos;
        if self.eat(b'Z') {
            return Ok(UtcOffset { minutes: 0 });
        }
        let negative = match self.peek() {
            Some(b'+') => false,
            Some(b'-') => true,
            _ => return Err(LiteralError::Malformed),
        };
        self.pos += 1;
        let hours = self.number(2)?;
        let minutes = if self.eat(b':') { self.number(2)? } else { 0 };
        let written = &self.text[start..self.pos];
        if hours > 23 || minutes > 59 {
            return invalid(
                codes::INVALID_TEMPORAL,
                "LEX-11",
                format!("offset `{written}` does not exist: hours are 00 to 23, minutes 00 to 59"),
            );
        }
        // At most 23 * 60 + 59, which fits i16.
        let total = (hours * 60 + minutes) as i16;
        Ok(UtcOffset {
            minutes: if negative { -total } else { total },
        })
    }

    /// `[Component/Component]`, returning the name between the brackets
    /// (LEX-12).
    fn zone(&mut self) -> Result<&'a str, LiteralError> {
        self.expect(b'[')?;
        let rest = &self.text[self.pos..];
        let length = rest.find(']').ok_or(LiteralError::Malformed)?;
        let name = &rest[..length];
        self.pos += length + 1;

        let zone_character =
            |byte: u8| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'+' | b'.');
        for component in name.split('/') {
            // An empty component is a leading, trailing or doubled slash.
            if component.is_empty() || !component.bytes().all(zone_character) {
                return Err(LiteralError::Malformed);
            }
            if component == "." || component == ".." {
                return invalid(
                    codes::INVALID_ZONE,
                    "LEX-12",
                    format!("zone name `{name}` has the component `{component}`"),
                );
            }
        }
        if name.len() > 255 {
            return invalid(
                codes::INVALID_ZONE,
                "LEX-12",
                "zone name is longer than 255 bytes",
            );
        }
        Ok(name)
    }
}

// ---- Validation over a token list -----------------------------------------

/// Decodes every literal token and reports each value-rule violation.
///
/// This is the check half of decoding: it discards the values. A stage that
/// needs a value calls the matching `decode_*` function on the token's
/// text. Tokens the lexer already reported as malformed are skipped, so a
/// problem is reported once.
pub fn validate_literals(
    sources: &SourceMap,
    file: FileId,
    tokens: &[Token],
    diagnostics: &mut Diagnostics,
) {
    let source = sources.file(file);
    for token in tokens {
        let text = source.slice(token.span);
        // `.err()` keeps only the failure, turning Result<T, E> into
        // Option<E>, so the four value types can share one variable.
        let failure = match token.kind {
            TokenKind::Int => decode_integer(text).err(),
            TokenKind::Float => decode_float(text).err(),
            TokenKind::Str => decode_string(text).err(),
            TokenKind::Duration => decode_duration(text).err(),
            TokenKind::Temporal => decode_temporal(text).err(),
            _ => None,
        };
        if let Some(LiteralError::Invalid {
            code,
            rule,
            message,
        }) = failure
        {
            let location = Location::new(file, token.span);
            diagnostics.emit(
                Diagnostic::error(Category::Parse, Code::Local(code), location, message)
                    .with_rule(rule),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The code of an `Invalid` error; panics on success or `Malformed`.
    fn invalid_code<T: std::fmt::Debug>(result: Result<T, LiteralError>) -> &'static str {
        match result {
            Err(LiteralError::Invalid { code, .. }) => code,
            other => panic!("expected an Invalid error, got {other:?}"),
        }
    }

    #[test]
    fn strings_decode_escapes() {
        // LEX-2.
        assert_eq!(decode_string(r#""""#).unwrap(), "");
        assert_eq!(decode_string(r#""a b""#).unwrap(), "a b");
        assert_eq!(decode_string(r#""q\"q""#).unwrap(), "q\"q");
        assert_eq!(decode_string(r#""\\ \n \r \t""#).unwrap(), "\\ \n \r \t");
        assert_eq!(decode_string("\"caf\u{e9}\"").unwrap(), "caf\u{e9}");
    }

    #[test]
    fn malformed_strings_have_no_value() {
        for text in [
            r#""abc"#,
            r#""a\qb""#,
            r#""abc\""#,
            r#"abc""#,
            r#""a"b""#,
            "",
        ] {
            assert_eq!(decode_string(text), Err(LiteralError::Malformed), "{text}");
        }
    }

    #[test]
    fn integers_use_the_i64_domain() {
        // LEX-7.
        assert_eq!(decode_integer("0"), Ok(0));
        assert_eq!(decode_integer("007"), Ok(7));
        assert_eq!(decode_integer("9223372036854775807"), Ok(i64::MAX as u64));
        // 2^63: valid only under a minus, which the analyser checks.
        assert_eq!(
            decode_integer("9223372036854775808"),
            Ok(MAX_INTEGER_MAGNITUDE)
        );
        assert_eq!(
            invalid_code(decode_integer("9223372036854775809")),
            codes::INTEGER_RANGE
        );
        assert_eq!(
            invalid_code(decode_integer("99999999999999999999999999")),
            codes::INTEGER_RANGE
        );
        assert_eq!(decode_integer("12a"), Err(LiteralError::Malformed));
    }

    #[test]
    fn floats_use_the_f64_domain() {
        // LEX-7.
        assert_eq!(decode_float("1.5"), Ok(1.5));
        assert_eq!(decode_float("1e5"), Ok(100_000.0));
        assert_eq!(decode_float("2.5E-3"), Ok(0.0025));
        assert_eq!(decode_float("1.7976931348623157e308"), Ok(f64::MAX));
        assert_eq!(invalid_code(decode_float("1e309")), codes::FLOAT_RANGE);
        assert_eq!(decode_float("1.5q"), Err(LiteralError::Malformed));
    }

    #[test]
    fn durations_scale_to_microseconds() {
        // LEX-13, TYPE-2.
        assert_eq!(decode_duration("1us"), Ok(1));
        assert_eq!(decode_duration("1ms"), Ok(1_000));
        assert_eq!(decode_duration("1s"), Ok(1_000_000));
        assert_eq!(decode_duration("1m"), Ok(60_000_000));
        assert_eq!(decode_duration("1h"), Ok(3_600_000_000));
        // "One day is exactly 86,400 seconds."
        assert_eq!(decode_duration("1d"), Ok(86_400 * 1_000_000));
        assert_eq!(decode_duration("1h30m"), Ok(90 * 60_000_000));
        assert_eq!(decode_duration("1d2h3m4s5ms6us"), Ok(93_784_005_006));
        assert_eq!(decode_duration("0s"), Ok(0));
        // Units need not carry: 90 minutes is fine.
        assert_eq!(decode_duration("90m"), Ok(90 * 60_000_000));
    }

    #[test]
    fn a_final_fraction_must_scale_exactly() {
        // LEX-13.
        assert_eq!(decode_duration("1.5s"), Ok(1_500_000));
        assert_eq!(decode_duration("0.5ms"), Ok(500));
        assert_eq!(decode_duration("1h0.5m"), Ok(3_600_000_000 + 30_000_000));
        assert_eq!(decode_duration("0.000001s"), Ok(1));
        assert_eq!(decode_duration("1.50s"), Ok(1_500_000));
        assert_eq!(decode_duration("1.0us"), Ok(1));
        // 0.25 of a day is six hours exactly.
        assert_eq!(decode_duration("0.25d"), Ok(6 * 3_600_000_000));
        assert_eq!(
            invalid_code(decode_duration("0.5us")),
            codes::INVALID_DURATION
        );
        assert_eq!(
            invalid_code(decode_duration("0.0000001s")),
            codes::INVALID_DURATION
        );
        assert_eq!(
            invalid_code(decode_duration("0.3333333333333333333333d")),
            codes::INVALID_DURATION
        );
    }

    #[test]
    fn duration_units_descend_and_do_not_repeat() {
        // LEX-13.
        assert_eq!(
            invalid_code(decode_duration("30m1h")),
            codes::INVALID_DURATION
        );
        assert_eq!(
            invalid_code(decode_duration("1us1ms")),
            codes::INVALID_DURATION
        );
        assert_eq!(
            invalid_code(decode_duration("1h2h")),
            codes::INVALID_DURATION
        );
        assert_eq!(
            invalid_code(decode_duration("1.5h30m")),
            codes::INVALID_DURATION
        );
    }

    #[test]
    fn durations_stay_in_the_signed_64_bit_domain() {
        // TYPE-2: signed 64-bit microseconds.
        assert_eq!(decode_duration("9223372036854775807us"), Ok(i64::MAX));
        assert_eq!(
            invalid_code(decode_duration("9223372036854775808us")),
            codes::INVALID_DURATION
        );
        assert_eq!(
            invalid_code(decode_duration("106751992d")),
            codes::INVALID_DURATION
        );
        assert_eq!(
            invalid_code(decode_duration("99999999999999999999999999s")),
            codes::INVALID_DURATION
        );
    }

    #[test]
    fn malformed_durations_have_no_value() {
        for text in ["", "5", "5x", "h", "1.s", "1h30", "1e5m", "1 h"] {
            assert_eq!(
                decode_duration(text),
                Err(LiteralError::Malformed),
                "{text}"
            );
        }
    }

    #[test]
    fn dates_must_exist() {
        // LEX-11.
        let date = decode_temporal("@2026-09-03").unwrap();
        assert_eq!(
            date.date,
            Some(Date {
                year: 2026,
                month: 9,
                day: 3
            })
        );
        assert_eq!((date.time, date.offset, date.zone), (None, None, None));

        assert!(
            decode_temporal("@2024-02-29").is_ok(),
            "2024 is a leap year"
        );
        assert!(
            decode_temporal("@2000-02-29").is_ok(),
            "2000 is a leap year"
        );
        assert_eq!(
            invalid_code(decode_temporal("@2026-02-29")),
            codes::INVALID_TEMPORAL
        );
        assert_eq!(
            invalid_code(decode_temporal("@1900-02-29")),
            codes::INVALID_TEMPORAL
        );
        assert_eq!(
            invalid_code(decode_temporal("@2026-04-31")),
            codes::INVALID_TEMPORAL
        );
        assert_eq!(
            invalid_code(decode_temporal("@2026-13-01")),
            codes::INVALID_TEMPORAL
        );
        assert_eq!(
            invalid_code(decode_temporal("@2026-00-10")),
            codes::INVALID_TEMPORAL
        );
        assert_eq!(
            invalid_code(decode_temporal("@2026-01-00")),
            codes::INVALID_TEMPORAL
        );
        // TYPE-2: years 1 to 9999.
        assert_eq!(
            invalid_code(decode_temporal("@0000-01-01")),
            codes::INVALID_TEMPORAL
        );
        assert!(decode_temporal("@0001-01-01").is_ok());
        assert!(decode_temporal("@9999-12-31").is_ok());
    }

    #[test]
    fn times_default_seconds_and_scale_fractions() {
        // LEX-11.
        let time = decode_temporal("@09:30").unwrap();
        assert_eq!(
            time.time,
            Some(Time {
                hour: 9,
                minute: 30,
                second: 0,
                microsecond: 0
            })
        );
        assert_eq!(time.date, None);

        let time = decode_temporal("@23:59:59.5").unwrap();
        assert_eq!(
            time.time,
            Some(Time {
                hour: 23,
                minute: 59,
                second: 59,
                microsecond: 500_000
            })
        );
        let time = decode_temporal("@00:00:00.000001").unwrap();
        assert_eq!(time.time.unwrap().microsecond, 1);
        let time = decode_temporal("@00:00:00.123456").unwrap();
        assert_eq!(time.time.unwrap().microsecond, 123_456);
    }

    #[test]
    fn times_exclude_twenty_four_hundred_and_leap_seconds() {
        // LEX-11.
        assert_eq!(
            invalid_code(decode_temporal("@24:00:00")),
            codes::INVALID_TEMPORAL
        );
        assert_eq!(
            invalid_code(decode_temporal("@24:00")),
            codes::INVALID_TEMPORAL
        );
        assert_eq!(
            invalid_code(decode_temporal("@23:59:60")),
            codes::INVALID_TEMPORAL
        );
        assert_eq!(
            invalid_code(decode_temporal("@12:60")),
            codes::INVALID_TEMPORAL
        );
    }

    #[test]
    fn date_times_take_offsets_and_zones() {
        // LEX-11.
        let civil = decode_temporal("@2026-09-03T09:30").unwrap();
        assert!(civil.date.is_some() && civil.time.is_some());
        assert_eq!(
            civil.offset, None,
            "offset-free date/time is civil, not UTC"
        );

        let utc = decode_temporal("@2026-09-03T09:30Z").unwrap();
        assert_eq!(utc.offset, Some(UtcOffset { minutes: 0 }));

        let east = decode_temporal("@2026-09-03T09:30:15+01").unwrap();
        assert_eq!(
            east.offset,
            Some(UtcOffset { minutes: 60 }),
            "offset minutes default to zero"
        );
        let west = decode_temporal("@2026-09-03T09:30-05:30").unwrap();
        assert_eq!(west.offset, Some(UtcOffset { minutes: -330 }));

        let zoned = decode_temporal("@2026-09-03T09:30+01:00[Europe/London]").unwrap();
        assert_eq!(zoned.zone, Some("Europe/London"));
        assert_eq!(zoned.offset, Some(UtcOffset { minutes: 60 }));

        assert_eq!(
            invalid_code(decode_temporal("@2026-09-03T09:30+24:00")),
            codes::INVALID_TEMPORAL
        );
        assert_eq!(
            invalid_code(decode_temporal("@2026-09-03T09:30+01:60")),
            codes::INVALID_TEMPORAL
        );
        // A zone without an offset is not a literal form.
        assert_eq!(
            decode_temporal("@2026-09-03T09:30[Europe/London]"),
            Err(LiteralError::Malformed)
        );
    }

    #[test]
    fn zone_names_meet_their_limits() {
        // LEX-12.
        let zone = decode_temporal("@[Europe/London]").unwrap();
        assert_eq!(zone.zone, Some("Europe/London"));
        assert_eq!((zone.date, zone.time, zone.offset), (None, None, None));
        assert_eq!(
            decode_temporal("@09:30[Etc/GMT+1]").unwrap().zone,
            Some("Etc/GMT+1")
        );

        assert_eq!(
            invalid_code(decode_temporal("@[Europe/..]")),
            codes::INVALID_ZONE
        );
        assert_eq!(
            invalid_code(decode_temporal("@[./London]")),
            codes::INVALID_ZONE
        );
        assert!(
            decode_temporal("@[a.b/c..d]").is_ok(),
            "dots inside a longer component are fine"
        );

        let longest = format!("@[{}]", "a".repeat(255));
        assert!(decode_temporal(&longest).is_ok());
        let too_long = format!("@[{}]", "a".repeat(256));
        assert_eq!(
            invalid_code(decode_temporal(&too_long)),
            codes::INVALID_ZONE
        );

        for text in [
            "@[]",
            "@[/Europe]",
            "@[Europe/]",
            "@[Europe//London]",
            "@[Europe London]",
            "@[Europe",
        ] {
            assert_eq!(
                decode_temporal(text),
                Err(LiteralError::Malformed),
                "{text}"
            );
        }
    }

    #[test]
    fn malformed_temporal_literals_have_no_value() {
        for text in [
            "",
            "2026-09-03",
            "@",
            "@12",
            "@2026-9-3",
            "@2026-09-03T",
            "@2026-09-03t09:30",
            "@09:30:15.",
            "@09:30x",
        ] {
            assert_eq!(
                decode_temporal(text),
                Err(LiteralError::Malformed),
                "{text}"
            );
        }
    }
}
