//! The source-text chapter's own examples.
//!
//! The chapter closes with a table of accepted illustrations and a list of
//! deliberate rejections. Each one is checked here as the chapter states it.
//! This is not a conformance claim: the chapter says conformance cases for
//! these are still planned.

use hgl_diagnostics::Diagnostics;
use hgl_lexer::literal::{self, Date, Time, UtcOffset};
use hgl_lexer::{Token, TokenKind, lex};
use hgl_source::SourceMap;

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

#[test]
fn one_duration_token_ninety_minutes() {
    accepted("1h30m", &[(TokenKind::Duration, "1h30m")]);
    let ninety_minutes = 90 * 60 * 1_000_000;
    assert_eq!(literal::decode_duration("1h30m"), Ok(ninety_minutes));
}

#[test]
fn minus_followed_by_a_duration_token() {
    accepted(
        "-250ms",
        &[(TokenKind::Minus, "-"), (TokenKind::Duration, "250ms")],
    );
    assert_eq!(literal::decode_duration("250ms"), Ok(250_000));
}

#[test]
fn time_with_seconds_zero() {
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

#[test]
fn duration_units_out_of_order_are_rejected() {
    rejected("30m1h", literal::codes::INVALID_DURATION, "LEX-13");
}

#[test]
fn a_fraction_of_a_microsecond_is_rejected() {
    rejected("0.5us", literal::codes::INVALID_DURATION, "LEX-13");
}

#[test]
fn a_date_that_does_not_exist_is_rejected() {
    rejected("@2026-02-29", literal::codes::INVALID_TEMPORAL, "LEX-11");
}

#[test]
fn an_unclosed_string_is_rejected() {
    rejected(
        "let name = \"unclosed\n",
        hgl_lexer::codes::UNCLOSED_STRING,
        "LEX-2",
    );
}

#[test]
fn rejections_use_local_codes() {
    // The diagnostic catalogue has no code for a lexical or literal
    // rejection, so none of these can yet satisfy `# expect-error` (TST-12).
    // This test fails when a code is catalogued and adopted, as a prompt to
    // revisit the cases above.
    let mut sources = SourceMap::new();
    let mut diagnostics = Diagnostics::new();
    for (index, text) in ["30m1h", "0.5us", "@2026-02-29", "\"unclosed"]
        .iter()
        .enumerate()
    {
        let file = sources
            .add(format!("rejection_{index}.hgl"), *text)
            .unwrap();
        let tokens = lex(&sources, file, &mut diagnostics);
        literal::validate_literals(&sources, file, &tokens, &mut diagnostics);
    }
    assert_eq!(diagnostics.len(), 4);
    for diagnostic in diagnostics.as_slice() {
        assert!(
            !diagnostic.code.is_catalogued(),
            "{} is local",
            diagnostic.code.as_str()
        );
    }
}
