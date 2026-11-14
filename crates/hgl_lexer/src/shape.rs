//! The shapes of literals: where one ends and how its text is laid out.
//!
//! The lexer needs a literal's shape to cut the token; [`crate::literal`]
//! needs the same shape to compute the value. Both call the functions here,
//! so the two cannot disagree about what a literal is. The functions are
//! pure: bytes in, a description out, no diagnostics.

use std::ops::Range;

use hgl_source::line_break_length;

/// The duration units, largest first (LEX-13). The position of a unit in
/// this list is its rank: a smaller index is a larger unit.
pub(crate) const DURATION_UNITS: [&str; 6] = ["d", "h", "m", "s", "ms", "us"];

// ---- Numbers and durations (LEX-8, LEX-22, LEX-23) -----------------------

/// The form of a numeric or duration literal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NumberForm {
    /// A decimal integer: `42`, `1_000`.
    Decimal,
    /// A hexadecimal integer: `0x2A` (LEX-23).
    Hexadecimal,
    /// A binary integer: `0b10_1010` (LEX-23).
    Binary,
    /// A float: `1.5`, or with `exponent` set `1e5` and `1.5e-3`.
    Float {
        /// True when the literal has an exponent, which rules out a
        /// duration unit after it (LEX-8).
        exponent: bool,
    },
    /// A duration: `1h30m`, `1.5s`.
    Duration,
}

/// A numeric or duration literal found at the start of some bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Number {
    /// Which literal it is.
    pub form: NumberForm,
    /// How many bytes it covers.
    pub length: usize,
}

/// Reads the longest numeric or duration literal at the start of `bytes`,
/// which must begin with a digit.
///
/// ```text
/// 12        Decimal       1_000     Decimal       one `_` between digits
/// 0x1F      Hexadecimal   0b1010    Binary
/// 12.5      Float         a `.` counts only when a digit follows it
/// 1e5       Float         a complete exponent makes a float
/// 1h30m     Duration      numbers each followed by a unit
/// ```
///
/// The caller decides what the bytes after the literal mean. Under LEX-18 a
/// letter, digit or `_` there makes the whole run malformed: for `10x` this
/// function returns the `Decimal` `10`, and the `x` after it is the caller's
/// evidence that `10x` is not a literal.
pub(crate) fn scan_number(bytes: &[u8]) -> Number {
    debug_assert!(bytes.first().is_some_and(u8::is_ascii_digit));

    // LEX-23: `0x` then a hexadecimal digit, `0b` then a binary digit. The
    // prefix letter is lowercase. `if let` with a slice pattern tests the
    // first bytes and names the third in one step.
    if let [b'0', b'x', digit, ..] = bytes
        && digit.is_ascii_hexdigit()
    {
        let digits = digit_run_length(&bytes[2..], |byte| byte.is_ascii_hexdigit());
        return Number {
            form: NumberForm::Hexadecimal,
            length: 2 + digits,
        };
    }
    if let [b'0', b'b', b'0' | b'1', ..] = bytes {
        let digits = digit_run_length(&bytes[2..], |byte| matches!(byte, b'0' | b'1'));
        return Number {
            form: NumberForm::Binary,
            length: 2 + digits,
        };
    }

    let (mut length, has_fraction) = plain_number_length(bytes);

    // LEX-8: a number directly followed by letters is a duration candidate.
    // It is a duration when those letters are a unit; then more parts may
    // follow, each a number and a unit.
    if let Some(unit) = unit_length(&bytes[length..]) {
        length += unit;
        loop {
            let rest = &bytes[length..];
            if !rest.first().is_some_and(u8::is_ascii_digit) {
                break;
            }
            let (number, _) = plain_number_length(rest);
            let Some(unit) = unit_length(&rest[number..]) else {
                break;
            };
            length += number + unit;
        }
        return Number {
            form: NumberForm::Duration,
            length,
        };
    }

    if let Some(exponent) = exponent_length(&bytes[length..]) {
        return Number {
            form: NumberForm::Float { exponent: true },
            length: length + exponent,
        };
    }
    let form = if has_fraction {
        NumberForm::Float { exponent: false }
    } else {
        NumberForm::Decimal
    };
    Number { form, length }
}

/// The length of the digit run at the start of `bytes`: digits accepted by
/// `is_digit`, where one `_` may stand between two of them (LEX-22). Zero
/// when `bytes` does not start with such a digit.
///
/// A `_` that is not between two digits is not part of the run: for `1_`
/// and `1__0` the run is `1`.
pub(crate) fn digit_run_length(bytes: &[u8], is_digit: impl Fn(u8) -> bool) -> usize {
    let digit_at = |index: usize| bytes.get(index).is_some_and(|&byte| is_digit(byte));
    let mut length = 0;
    loop {
        while digit_at(length) {
            length += 1;
        }
        let joins = length > 0 && bytes.get(length) == Some(&b'_') && digit_at(length + 1);
        if !joins {
            return length;
        }
        length += 1;
    }
}

/// The length of `digits` or `digits.digits` at the start of `bytes`, and
/// whether the fraction was there. A `.` not followed by a digit is left
/// alone: `1.max` is `1`, `.`, `max`.
fn plain_number_length(bytes: &[u8]) -> (usize, bool) {
    let whole = digit_run_length(bytes, |byte| byte.is_ascii_digit());
    if bytes.get(whole) == Some(&b'.') {
        let fraction = digit_run_length(&bytes[whole + 1..], |byte| byte.is_ascii_digit());
        if fraction > 0 {
            return (whole + 1 + fraction, true);
        }
    }
    (whole, false)
}

/// The length of the duration unit at the start of `bytes`, or `None`.
///
/// A unit is a maximal run of letters, so `ms` is one unit and `min` is one
/// (unknown) unit, never `m` then `in`.
fn unit_length(bytes: &[u8]) -> Option<usize> {
    let letters = bytes
        .iter()
        .take_while(|byte| byte.is_ascii_alphabetic())
        .count();
    DURATION_UNITS
        .iter()
        .any(|unit| unit.as_bytes() == &bytes[..letters])
        .then_some(letters)
}

/// The length of a complete exponent (`e` or `E`, optional sign, digits) at
/// the start of `bytes`, or `None`. A bare `e` is not an exponent.
fn exponent_length(bytes: &[u8]) -> Option<usize> {
    if !matches!(bytes.first(), Some(b'e' | b'E')) {
        return None;
    }
    let sign = usize::from(matches!(bytes.get(1), Some(b'+' | b'-')));
    let digits = digit_run_length(&bytes[1 + sign..], |byte| byte.is_ascii_digit());
    // `then_some` turns a bool into an Option: Some(...) when true.
    (digits > 0).then_some(1 + sign + digits)
}

// ---- Unicode escapes (LEX-24) --------------------------------------------

/// Reads a `\u{...}` escape at the start of `bytes`, which must begin with
/// `\u`.
///
/// Returns `None` when the text is not `\u{`, hexadecimal digits, `}`.
/// Otherwise returns the escape's length and the character it names, which
/// is `None` when it names no Unicode scalar value: a surrogate, a number
/// above 10FFFF, or more than six digits (LEX-24).
pub(crate) fn unicode_escape(bytes: &[u8]) -> Option<(usize, Option<char>)> {
    debug_assert!(bytes.starts_with(b"\\u"));
    if bytes.get(2) != Some(&b'{') {
        return None;
    }
    let digits = bytes[3..]
        .iter()
        .take_while(|byte| byte.is_ascii_hexdigit())
        .count();
    if digits == 0 || bytes.get(3 + digits) != Some(&b'}') {
        return None;
    }
    let length = 3 + digits + 1;
    if digits > 6 {
        return Some((length, None));
    }
    // Six hexadecimal digits always fit in a u32. `char::from_u32` returns
    // None for exactly the numbers that are not scalar values.
    let number = bytes[3..3 + digits].iter().fold(0u32, |number, &digit| {
        number * 16 + char::from(digit).to_digit(16).expect("a hexadecimal digit")
    });
    Some((length, char::from_u32(number)))
}

// ---- Zone names (LEX-12) -------------------------------------------------

/// True for a byte admitted in a time-zone name component (the
/// `zone_character` production).
pub(crate) fn is_zone_character(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'+' | b'.')
}

// ---- Text blocks (LEX-25) ------------------------------------------------

/// How the lines of a text block are laid out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TextBlockLayout<'a> {
    /// The spaces and tabs before the closing delimiter. They are removed
    /// from the start of every line.
    pub indentation: &'a str,
    /// The byte range, within the block's body, of each line of text,
    /// without its line break.
    pub lines: Vec<Range<usize>>,
}

/// A text block whose layout breaks LEX-25.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TextBlockError {
    /// No line break directly follows the opening delimiter.
    Opening,
    /// Something other than spaces and tabs comes before the closing
    /// delimiter on its line.
    Closing,
    /// The line at this byte range of the body does not start with the
    /// indentation and is not blank.
    Line(Range<usize>),
}

/// True for a line of only spaces and tabs, which stands for an empty line
/// whatever its length.
pub(crate) fn is_blank(line: &str) -> bool {
    line.bytes().all(|byte| byte == b' ' || byte == b'\t')
}

/// Works out the layout of a text block from its `body`: the text between
/// the opening and closing `"""`, both excluded.
///
/// ```text
/// let usage = """<line break>        the body starts at this line break
///     Usage: run<line break>         a line: `Usage: run`
///     """                            `    ` is the indentation
/// ```
pub(crate) fn text_block_layout(body: &str) -> Result<TextBlockLayout<'_>, TextBlockError> {
    let bytes = body.as_bytes();
    let Some(first_break) = line_break_length(bytes) else {
        return Err(TextBlockError::Opening);
    };

    // Cut the rest at line breaks (LEX-15: LF, CRLF and CR alike). What is
    // left after the last one is the text before the closing delimiter.
    let mut lines = Vec::new();
    let mut line_start = first_break;
    let mut offset = first_break;
    while offset < bytes.len() {
        match line_break_length(&bytes[offset..]) {
            Some(length) => {
                lines.push(line_start..offset);
                offset += length;
                line_start = offset;
            }
            None => offset += 1,
        }
    }
    let indentation = &body[line_start..];
    if !is_blank(indentation) {
        return Err(TextBlockError::Closing);
    }

    for line in &lines {
        // `line.clone()` copies the range: slicing consumes the one it is
        // given, and a `Range` is deliberately not `Copy`.
        let text = &body[line.clone()];
        if !is_blank(text) && !text.starts_with(indentation) {
            return Err(TextBlockError::Line(line.clone()));
        }
    }
    Ok(TextBlockLayout { indentation, lines })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The form and text of the literal `scan_number` finds at the start of
    /// `text`.
    fn scanned(text: &str) -> (NumberForm, &str) {
        let number = scan_number(text.as_bytes());
        (number.form, &text[..number.length])
    }

    #[test]
    fn integers_in_three_bases() {
        // LEX-23.
        assert_eq!(scanned("0"), (NumberForm::Decimal, "0"));
        assert_eq!(scanned("42 "), (NumberForm::Decimal, "42"));
        assert_eq!(scanned("0x2A"), (NumberForm::Hexadecimal, "0x2A"));
        assert_eq!(scanned("0xff+1"), (NumberForm::Hexadecimal, "0xff"));
        assert_eq!(scanned("0b1010"), (NumberForm::Binary, "0b1010"));
    }

    #[test]
    fn a_prefix_needs_a_digit_of_its_base() {
        // LEX-23. Without one, `0` is all that matches; the caller sees the
        // letter after it and rejects the run.
        for text in ["0x", "0xG", "0X1F", "0b", "0b2", "0B1"] {
            assert_eq!(scanned(text), (NumberForm::Decimal, "0"), "{text}");
        }
        // The literal stops at the first digit outside the base.
        assert_eq!(scanned("0b102"), (NumberForm::Binary, "0b10"));
        assert_eq!(scanned("0xFFh"), (NumberForm::Hexadecimal, "0xFF"));
    }

    #[test]
    fn one_underscore_joins_two_digits() {
        // LEX-22, in every digit run.
        assert_eq!(scanned("1_000_000"), (NumberForm::Decimal, "1_000_000"));
        assert_eq!(scanned("0xFF_FF"), (NumberForm::Hexadecimal, "0xFF_FF"));
        assert_eq!(scanned("0b1111_0000"), (NumberForm::Binary, "0b1111_0000"));
        assert_eq!(
            scanned("1_0.5_0e1_0"),
            (NumberForm::Float { exponent: true }, "1_0.5_0e1_0")
        );
        assert_eq!(scanned("1_500ms"), (NumberForm::Duration, "1_500ms"));

        // Anywhere else it is not part of the literal.
        assert_eq!(scanned("1_"), (NumberForm::Decimal, "1"));
        assert_eq!(scanned("1__000"), (NumberForm::Decimal, "1"));
        assert_eq!(scanned("0x_FF"), (NumberForm::Decimal, "0"));
        assert_eq!(scanned("0xFF_"), (NumberForm::Hexadecimal, "0xFF"));
        assert_eq!(scanned("1_.5"), (NumberForm::Decimal, "1"));
        assert_eq!(scanned("1._5"), (NumberForm::Decimal, "1"));
    }

    #[test]
    fn floats_need_a_digit_after_the_dot_or_a_whole_exponent() {
        // The FLOAT_LITERAL production.
        assert_eq!(
            scanned("1.5"),
            (NumberForm::Float { exponent: false }, "1.5")
        );
        assert_eq!(scanned("1.max"), (NumberForm::Decimal, "1"));
        assert_eq!(
            scanned("1.5.2"),
            (NumberForm::Float { exponent: false }, "1.5")
        );
        assert_eq!(
            scanned("1e5"),
            (NumberForm::Float { exponent: true }, "1e5")
        );
        assert_eq!(
            scanned("1.5E-3"),
            (NumberForm::Float { exponent: true }, "1.5E-3")
        );
        assert_eq!(scanned("1e"), (NumberForm::Decimal, "1"));
        assert_eq!(scanned("1e+"), (NumberForm::Decimal, "1"));
        assert_eq!(
            scanned("1e5m"),
            (NumberForm::Float { exponent: true }, "1e5")
        );
    }

    #[test]
    fn durations_are_numbers_each_with_a_unit() {
        // The `duration_literal` production. LEX-8: digits then letters are a
        // duration candidate.
        assert_eq!(scanned("1h30m"), (NumberForm::Duration, "1h30m"));
        assert_eq!(scanned("1.5s"), (NumberForm::Duration, "1.5s"));
        assert_eq!(scanned("250ms"), (NumberForm::Duration, "250ms"));
        assert_eq!(scanned("0d"), (NumberForm::Duration, "0d"));
        assert_eq!(scanned("5s.x"), (NumberForm::Duration, "5s"));
        // A unit is the whole run of letters.
        assert_eq!(scanned("5min"), (NumberForm::Decimal, "5"));
        assert_eq!(scanned("1H"), (NumberForm::Decimal, "1"));
        // A number with no unit after it is not a part.
        assert_eq!(scanned("1h30"), (NumberForm::Duration, "1h"));
        assert_eq!(scanned("1h30x"), (NumberForm::Duration, "1h"));
    }

    #[test]
    fn unicode_escapes_name_scalar_values() {
        assert_eq!(unicode_escape(br"\u{41}"), Some((6, Some('A'))));
        assert_eq!(
            unicode_escape(br"\u{20AC} rest"),
            Some((8, Some('\u{20ac}')))
        );
        assert_eq!(
            unicode_escape(br"\u{10ffff}"),
            Some((10, Some('\u{10ffff}')))
        );
        assert_eq!(unicode_escape(br"\u{000041}"), Some((10, Some('A'))));
        // LEX-24: well formed, but naming no scalar value.
        assert_eq!(unicode_escape(br"\u{D800}"), Some((8, None)));
        assert_eq!(unicode_escape(br"\u{DFFF}"), Some((8, None)));
        assert_eq!(unicode_escape(br"\u{110000}"), Some((10, None)));
        assert_eq!(unicode_escape(br"\u{0000041}"), Some((11, None)));
        // Not of the form at all.
        for text in [
            r"\u", r"\u41", r"\u{}", r"\u{41", r"\u{4G}", r"\u{ 41}", r"\u{+41}",
        ] {
            assert_eq!(unicode_escape(text.as_bytes()), None, "{text}");
        }
    }

    /// The lines of a body as text, or the layout error.
    fn lines_of(body: &str) -> Result<(&str, Vec<&str>), TextBlockError> {
        let layout = text_block_layout(body)?;
        let lines = layout
            .lines
            .iter()
            .map(|line| &body[line.clone()])
            .collect();
        Ok((layout.indentation, lines))
    }

    #[test]
    fn a_text_block_body_is_lines_then_the_closing_indentation() {
        assert_eq!(lines_of("\n"), Ok(("", vec![])));
        assert_eq!(lines_of("\n    "), Ok(("    ", vec![])));
        assert_eq!(lines_of("\n  a\n  b\n  "), Ok(("  ", vec!["  a", "  b"])));
        assert_eq!(lines_of("\na\n"), Ok(("", vec!["a"])));
        // Deeper lines keep the extra; blank lines need no indentation.
        assert_eq!(
            lines_of("\n  a\n\n    b\n \n  "),
            Ok(("  ", vec!["  a", "", "    b", " "]))
        );
        // LEX-15: every line break form cuts lines.
        assert_eq!(lines_of("\r\n\ta\r\tb\n\t"), Ok(("\t", vec!["\ta", "\tb"])));
    }

    #[test]
    fn a_text_block_layout_can_break_in_three_ways() {
        // LEX-25.
        assert_eq!(lines_of(""), Err(TextBlockError::Opening));
        assert_eq!(lines_of("text"), Err(TextBlockError::Opening));
        assert_eq!(lines_of(" \n  a\n  "), Err(TextBlockError::Opening));
        assert_eq!(lines_of("\n  a"), Err(TextBlockError::Closing));
        assert_eq!(lines_of("\n  a\n  b "), Err(TextBlockError::Closing));
        assert_eq!(lines_of("\n  a\n b\n  "), Err(TextBlockError::Line(5..7)));
        // Indentation is compared exactly: a tab is not two spaces.
        assert_eq!(lines_of("\n\ta\n  "), Err(TextBlockError::Line(1..3)));
        // Only the first offending line is reported.
        assert_eq!(lines_of("\na\nb\n  "), Err(TextBlockError::Line(1..2)));
    }
}
