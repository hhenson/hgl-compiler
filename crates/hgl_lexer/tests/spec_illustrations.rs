//! The source-text chapter's own examples.
//!
//! The chapter gives tables of accepted illustrations and lists of
//! deliberate rejections. Each one is checked here as the chapter states it.
//! This is not a conformance claim: the chapter says conformance cases for
//! these are still planned.

use hgl_diagnostics::{Category, Diagnostics};
use hgl_lexer::literal::{self, Date, Time, UtcOffset};
use hgl_lexer::{Token, TokenKind, lex};
use hgl_source::{LineCol, SourceMap};

/// What lexing and literal validation make of one fragment.
struct Outcome {
    /// Each non-trivia token: its kind and its text.
    tokens: Vec<(TokenKind, String)>,
    /// Each diagnostic: its code and the rule it cites.
    errors: Vec<(String, Option<&'static str>)>,
}

/// Runs the lexer and the literal checks over `text`.
fn check(text: &str) -> Outcome {
    let mut sources = SourceMap::new();
    let file = sources.add("illustration.hgl", text).unwrap();
    let mut diagnostics = Diagnostics::new();
    let tokens: Vec<Token> = lex(&sources, file, &mut diagnostics);
    literal::validate_literals(&sources, file, &tokens, &mut diagnostics);

    for diagnostic in diagnostics.as_slice() {
        // Every lexical rejection is a `parse` code of the diagnostic
        // catalog, so each can be the subject of `# expect-error` (TST-12).
        assert_eq!(diagnostic.category, Category::Parse);
        assert!(
            diagnostic.code.is_catalogued(),
            "{} is not a catalogued code",
            diagnostic.code.as_str()
        );
    }

    let source = sources.file(file);
    Outcome {
        tokens: tokens
            .iter()
            .filter(|token| !token.kind.is_trivia())
            .map(|token| (token.kind, source.slice(token.span).to_string()))
            .collect(),
        errors: diagnostics
            .as_slice()
            .iter()
            .map(|diagnostic| (diagnostic.code.as_str().to_string(), diagnostic.rule))
            .collect(),
    }
}

/// Asserts that `text` is accepted as exactly the given tokens.
fn accepted(text: &str, expected: &[(TokenKind, &str)]) {
    let outcome = check(text);
    assert_eq!(outcome.errors, [], "`{text}` should be accepted");
    let tokens: Vec<(TokenKind, &str)> = outcome
        .tokens
        .iter()
        .map(|(kind, text)| (*kind, text.as_str()))
        .collect();
    assert_eq!(tokens, expected, "tokens of `{text}`");
}

/// Asserts that `text` is rejected once, with `code` under `rule`.
fn rejected(text: &str, code: &str, rule: &'static str) {
    let outcome = check(text);
    assert_eq!(
        outcome.errors,
        [(code.to_string(), Some(rule))],
        "`{text}` should be rejected once"
    );
}

// ---- First table ----------------------------------------------------------

#[test]
fn one_duration_token_ninety_minutes() {
    // LEX-8: digits then letters are a duration. LEX-13: its value.
    accepted("1h30m", &[(TokenKind::Duration, "1h30m")]);
    let ninety_minutes = 90 * 60 * 1_000_000;
    assert_eq!(literal::decode_duration("1h30m"), Ok(ninety_minutes));
}

#[test]
fn minus_followed_by_a_duration_token() {
    // LEX-7: the minus is an operator, not part of the literal.
    accepted(
        "-250ms",
        &[(TokenKind::Minus, "-"), (TokenKind::Duration, "250ms")],
    );
    assert_eq!(literal::decode_duration("250ms"), Ok(250_000));
}

#[test]
fn time_with_seconds_zero() {
    // LEX-11: seconds default to zero.
    accepted("@09:30", &[(TokenKind::Temporal, "@09:30")]);
    let value = literal::decode_temporal("@09:30").unwrap();
    let half_past_nine = Time {
        hour: 9,
        minute: 30,
        second: 0,
        microsecond: 0,
    };
    assert_eq!(value.time, Some(half_past_nine));
    assert_eq!((value.date, value.offset, value.zone), (None, None, None));
}

#[test]
fn utc_instant() {
    // LEX-11.
    accepted(
        "@2026-09-03T09:30Z",
        &[(TokenKind::Temporal, "@2026-09-03T09:30Z")],
    );
    let value = literal::decode_temporal("@2026-09-03T09:30Z").unwrap();
    let third_of_september = Date {
        year: 2026,
        month: 9,
        day: 3,
    };
    assert_eq!(value.date, Some(third_of_september));
    assert_eq!(value.offset, Some(UtcOffset { minutes: 0 }));
    assert_eq!(value.zone, None);
}

#[test]
fn named_zone() {
    // "Subject to provider validation": the compiler checks the name's form
    // only (LEX-12).
    accepted(
        "@[Europe/London]",
        &[(TokenKind::Temporal, "@[Europe/London]")],
    );
    let value = literal::decode_temporal("@[Europe/London]").unwrap();
    assert_eq!(value.zone, Some("Europe/London"));
}

// ---- First rejections -----------------------------------------------------

#[test]
fn duration_units_out_of_order_are_rejected() {
    rejected("30m1h", literal::codes::DURATION_INVALID, "LEX-13");
}

#[test]
fn a_fraction_of_a_microsecond_is_rejected() {
    rejected("0.5us", literal::codes::DURATION_INVALID, "LEX-13");
}

#[test]
fn a_date_that_does_not_exist_is_rejected() {
    rejected(
        "@2026-02-29",
        literal::codes::DATE_TIME_INVALID_FIELD,
        "LEX-11",
    );
}

#[test]
fn an_unclosed_string_is_rejected() {
    rejected(
        "let name = \"unclosed\n",
        hgl_lexer::codes::STRING_UNCLOSED,
        "LEX-2",
    );
}

// ---- Line breaks (LEX-15) -------------------------------------------------

/// The 1-based line and column at which each non-trivia, non-newline token
/// of `text` begins, with the token's text.
fn positions(text: &str) -> Vec<(String, LineCol)> {
    let mut sources = SourceMap::new();
    let file = sources.add("lines.hgl", text).unwrap();
    let mut diagnostics = Diagnostics::new();
    let tokens = lex(&sources, file, &mut diagnostics);
    assert!(diagnostics.is_empty(), "{text:?} should be accepted");
    let source = sources.file(file);
    tokens
        .iter()
        .filter(|token| !token.kind.is_trivia() && token.kind != TokenKind::Newline)
        .map(|token| {
            (
                source.slice(token.span).to_string(),
                source.line_col(token.span.start),
            )
        })
        .collect()
}

/// How many line-break tokens `text` lexes to.
fn line_breaks(text: &str) -> usize {
    let mut sources = SourceMap::new();
    let file = sources.add("lines.hgl", text).unwrap();
    let tokens = lex(&sources, file, &mut Diagnostics::new());
    tokens
        .iter()
        .filter(|token| token.kind == TokenKind::Newline)
        .count()
}

#[test]
fn four_physical_lines_under_three_line_break_forms() {
    // LEX-15.
    // "The source `a` LF `b` CR LF `c` CR `d` has four physical lines and
    // `d` begins line 4."
    let text = "a\nb\r\nc\rd";
    let mut sources = SourceMap::new();
    let file = sources.add("lines.hgl", text).unwrap();
    assert_eq!(sources.file(file).line_count(), 4);

    let at = |line, column| LineCol { line, column };
    assert_eq!(
        positions(text),
        [
            ("a".to_string(), at(1, 1)),
            ("b".to_string(), at(2, 1)),
            ("c".to_string(), at(3, 1)),
            ("d".to_string(), at(4, 1)),
        ]
    );
    assert_eq!(line_breaks(text), 3);
}

#[test]
fn a_carriage_return_pairs_only_with_the_line_feed_after_it() {
    // LEX-15.
    // "CR CR LF is two line breaks, as is LF CR."
    assert_eq!(line_breaks("a\r\r\nb"), 2);
    assert_eq!(line_breaks("a\n\rb"), 2);
    assert_eq!(line_breaks("a\r\nb"), 1);
}

#[test]
fn rewriting_line_breaks_keeps_tokens_lines_and_columns() {
    // LEX-15.
    // "A source rewritten from one form to another keeps its tokens, their
    // lines and columns, and its meaning."
    let unix = "let a = 1\n  let b = \"two\"\n\nlet c = @09:30\n";
    let windows = unix.replace('\n', "\r\n");
    let classic_mac = unix.replace('\n', "\r");
    assert_eq!(positions(unix), positions(&windows));
    assert_eq!(positions(unix), positions(&classic_mac));
}

// ---- Second table ---------------------------------------------------------

#[test]
fn two_closers_end_two_argument_lists() {
    // LEX-8, LEX-17: no token joins `>` to a following `>`.
    use TokenKind::*;
    accepted(
        "list<list<i64>>",
        &[
            (Ident, "list"),
            (Lt, "<"),
            (Ident, "list"),
            (Lt, "<"),
            (KwI64, "i64"),
            (Gt, ">"),
            (Gt, ">"),
        ],
    );
}

#[test]
fn a_float_may_round_to_zero() {
    // LEX-20.
    accepted("1e-400", &[(TokenKind::Float, "1e-400")]);
    assert_eq!(literal::decode_float("1e-400"), Ok(0.0));
}

#[test]
fn empty_block_comment() {
    // LEX-21.
    // A block comment, not the start of a documentation comment.
    let mut sources = SourceMap::new();
    let file = sources.add("comment.hgl", "/**/").unwrap();
    let mut diagnostics = Diagnostics::new();
    let tokens = lex(&sources, file, &mut diagnostics);
    assert!(diagnostics.is_empty());
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].kind, TokenKind::BlockComment);
}

#[test]
fn integer_one_million() {
    // LEX-22.
    accepted("1_000_000", &[(TokenKind::Int, "1_000_000")]);
    assert_eq!(literal::decode_integer("1_000_000"), Ok(1_000_000));
}

#[test]
fn integer_255_in_two_bases() {
    // LEX-23, with LEX-22's separator in the binary form.
    accepted("0xFF", &[(TokenKind::Int, "0xFF")]);
    accepted("0b1111_1111", &[(TokenKind::Int, "0b1111_1111")]);
    assert_eq!(literal::decode_integer("0xFF"), Ok(255));
    assert_eq!(literal::decode_integer("0b1111_1111"), Ok(255));
}

#[test]
fn duration_of_one_and_a_half_seconds() {
    // LEX-22 in a duration. LEX-13: its value.
    accepted("1_500ms", &[(TokenKind::Duration, "1_500ms")]);
    assert_eq!(literal::decode_duration("1_500ms"), Ok(1_500_000));
}

#[test]
fn string_of_one_character() {
    // LEX-24.
    let text = r#""\u{20AC}""#;
    accepted(text, &[(TokenKind::Str, text)]);
    assert_eq!(literal::decode_string(text).unwrap(), "\u{20ac}");
}

// ---- Text block -----------------------------------------------------------

#[test]
fn text_block_value_drops_four_columns_and_ends_without_a_line_feed() {
    // LEX-25.
    let block = "\"\"\"\n    Usage: run [options]\n      -v  verbose\n    \"\"\"";
    let text = format!("let usage = {block}\n");
    accepted(
        &text,
        &[
            (TokenKind::KwLet, "let"),
            (TokenKind::Ident, "usage"),
            (TokenKind::Eq, "="),
            (TokenKind::Str, block),
            (TokenKind::Newline, "\n"),
        ],
    );
    // "`usage` is `Usage: run [options]`, a line feed, then `  -v  verbose`."
    assert_eq!(
        literal::decode_string(block).unwrap(),
        "Usage: run [options]\n  -v  verbose"
    );
}

// ---- Second rejections ----------------------------------------------------

#[test]
fn a_literal_is_not_split_into_a_literal_and_a_name() {
    // LEX-18.
    for text in ["12abc", "1.5x", "1e", "1_", "1h_", "0x", "0b102"] {
        rejected(text, hgl_lexer::codes::NUMBER_MALFORMED, "LEX-18");
    }
}

#[test]
fn two_separators_in_a_row_are_rejected() {
    rejected("1__000", hgl_lexer::codes::NUMBER_MALFORMED, "LEX-22");
}

#[test]
fn a_hexadecimal_integer_outside_the_domain_is_rejected() {
    rejected(
        "0xFFFF_FFFF_FFFF_FFFF",
        literal::codes::NUMBER_INTEGER_RANGE,
        "LEX-23",
    );
}

#[test]
fn a_surrogate_escape_is_rejected() {
    rejected(
        r#""\u{D800}""#,
        hgl_lexer::codes::STRING_INVALID_ESCAPE,
        "LEX-24",
    );
}

#[test]
fn text_block_layout_faults_are_rejected() {
    // LEX-25, in the chapter's order: `"""text"""` on one line; a line
    // holding `"""` after other text; a line indented less than the closing
    // delimiter.
    for text in [
        "\"\"\"text\"\"\"",
        "\"\"\"\n    one\n    two \"\"\"",
        "\"\"\"\n    one\n  two\n    \"\"\"",
    ] {
        rejected(
            text,
            hgl_lexer::codes::STRING_MALFORMED_TEXT_BLOCK,
            "LEX-25",
        );
    }
}

#[test]
fn an_offset_of_a_whole_day_is_rejected() {
    rejected(
        "@2026-09-03T09:30+24:00",
        literal::codes::DATE_TIME_INVALID_FIELD,
        "LEX-19",
    );
}

#[test]
fn a_float_that_rounds_to_infinity_is_rejected() {
    rejected("1e400", literal::codes::NUMBER_FLOAT_RANGE, "LEX-20");
}

#[test]
fn a_closer_directly_before_equals_is_one_token() {
    // LEX-17. The lexer's part is the token `>=`; the rejection itself is
    // the parser's, which finds `>=` where the type's `>` should be.
    use TokenKind::*;
    accepted(
        "xs: list<i64>= value",
        &[
            (Ident, "xs"),
            (Colon, ":"),
            (Ident, "list"),
            (Lt, "<"),
            (KwI64, "i64"),
            (GtEq, ">="),
            (Ident, "value"),
        ],
    );
}

#[test]
fn a_form_feed_between_tokens_is_rejected() {
    rejected(
        "a \u{c} b",
        hgl_lexer::codes::SYNTAX_UNEXPECTED_CHARACTER,
        "LEX-16",
    );
}

// ---- Line continuation -----------------------------------------------------

#[test]
fn a_backslash_joins_a_line_to_the_next() {
    // LEX-26, from the chapter's illustration of lines that continue:
    // `return \` then `result`. The `\` and its line break are one token
    // of whitespace, so no `Newline` stands between `return` and
    // `result`, whatever the form of the line break (LEX-15).
    use TokenKind::{Ident, KwReturn, Newline};
    let joined = [(KwReturn, "return"), (Ident, "result")];
    for line_break in ["\n", "\r\n", "\r"] {
        accepted(&format!("return \\{line_break}        result"), &joined);
    }
    // Spaces and tabs may stand between the `\` and the line break.
    accepted("return \\ \t\n        result", &joined);
    // Without it the line break is there.
    accepted(
        "return\nresult",
        &[(KwReturn, "return"), (Newline, "\n"), (Ident, "result")],
    );
}

#[test]
fn a_backslash_followed_by_anything_else_is_rejected() {
    // LEX-26's rejections: "`\` followed on its line by a comment or by
    // any token".
    rejected(
        "a \\ # note\nb",
        hgl_lexer::codes::SYNTAX_UNEXPECTED_CHARACTER,
        "LEX-26",
    );
    rejected(
        "a \\ b",
        hgl_lexer::codes::SYNTAX_UNEXPECTED_CHARACTER,
        "LEX-26",
    );
    // At the very end of the source there is no line break to continue.
    rejected(
        "a \\",
        hgl_lexer::codes::SYNTAX_UNEXPECTED_CHARACTER,
        "LEX-26",
    );
}
