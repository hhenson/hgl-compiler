//! The lexer's tables against the pinned specification's own text.
//!
//! Three lists in this crate are copies of lists the specification owns:
//! the hard reserved words, the punctuation symbols, and the diagnostic
//! codes. These tests read the lists out of the pinned documents and compare
//! them, so raising the pin shows at once where the specification moved.
//!
//! They check that the lists agree. They do not check what the lexer does
//! with them: the unit tests and `spec_illustrations.rs` do that.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use hgl_lexer::{TokenKind, codes, literal};

/// Every diagnostic code the lexer and the literal checks can report.
const LEXICAL_CODES: [&str; 14] = [
    codes::SYNTAX_INVALID_ENCODING,
    codes::SYNTAX_UNEXPECTED_CHARACTER,
    codes::STRING_UNCLOSED,
    codes::STRING_UNKNOWN_ESCAPE,
    codes::STRING_INVALID_ESCAPE,
    codes::STRING_MALFORMED_TEXT_BLOCK,
    codes::COMMENT_UNCLOSED,
    codes::NUMBER_MALFORMED,
    codes::DATE_TIME_MALFORMED,
    literal::codes::NUMBER_INTEGER_RANGE,
    literal::codes::NUMBER_FLOAT_RANGE,
    literal::codes::DURATION_INVALID,
    literal::codes::DATE_TIME_INVALID_FIELD,
    literal::codes::DATE_TIME_INVALID_ZONE,
];

/// Reads one chapter of the pinned language specification.
fn chapter(name: &str) -> String {
    // CARGO_MANIFEST_DIR is this crate's directory, set by Cargo when it
    // compiles the test. The workspace root is two levels up.
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/hgl_spec/spec/language")
        .join(name);
    fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "cannot read {}: {error}; is the `spec/hgl_spec` submodule checked out?",
            path.display()
        )
    })
}

/// The lines of the first fenced block that follows the line `heading`.
fn fence_after<'a>(document: &'a str, heading: &str) -> Vec<&'a str> {
    let mut lines = document
        .lines()
        .skip_while(|line| line.trim() != heading)
        .skip(1)
        .skip_while(|line| !line.starts_with("```"));
    assert!(
        lines.next().is_some(),
        "no fenced block follows `{heading}`"
    );
    lines.take_while(|line| !line.starts_with("```")).collect()
}

/// The quoted terminals of the EBNF production named `name`, in order.
///
/// A production runs from `name =` to the first `;` outside quotes. No
/// terminal of the productions read here contains a quote.
fn terminals(document: &str, name: &str) -> Vec<String> {
    let opening = format!("{name} =");
    let start = document
        .find(&opening)
        .unwrap_or_else(|| panic!("no production `{name}`"));

    let mut found = Vec::new();
    // `Some(text)` while inside a quoted terminal.
    let mut terminal: Option<String> = None;
    for character in document[start + opening.len()..].chars() {
        match (&mut terminal, character) {
            (Some(_), '"') => found.extend(terminal.take()),
            (Some(text), other) => text.push(other),
            (None, '"') => terminal = Some(String::new()),
            (None, ';') => return found,
            (None, _) => {}
        }
    }
    panic!("production `{name}` has no closing `;`");
}

#[test]
fn reserved_words_are_the_chapters_list() {
    // LEX-1. Same words, same order.
    let source_text = chapter("source-text.md");
    let listed: Vec<&str> = fence_after(&source_text, "Hard reserved words:")
        .into_iter()
        .flat_map(str::split_whitespace)
        .collect();
    let known: Vec<&str> = TokenKind::KEYWORDS.iter().map(|&(text, _)| text).collect();
    assert!(!listed.is_empty());
    assert_eq!(known, listed);
}

#[test]
fn symbols_are_the_punctuation_production() {
    // LEX-17: "each PUNCTUATION alternative is one token". The order is
    // not compared: the lexer picks the longest match however the
    // alternatives are listed.
    let source_text = chapter("source-text.md");
    let listed: BTreeSet<String> = terminals(&source_text, "PUNCTUATION").into_iter().collect();
    let known: BTreeSet<String> = TokenKind::PUNCTUATION
        .iter()
        .map(|&(text, _)| text.to_string())
        .collect();
    assert!(!listed.is_empty());
    assert_eq!(known, listed);
    // No symbol is listed twice on either side.
    assert_eq!(known.len(), TokenKind::PUNCTUATION.len());
    assert_eq!(listed.len(), terminals(&source_text, "PUNCTUATION").len());
}

/// The rows of the diagnostic catalog in category `parse`: each code with
/// its condition column.
fn parse_rows(catalog: &str) -> Vec<(String, String)> {
    catalog
        .lines()
        .filter_map(|line| {
            // A row reads `| parse | `code` | condition |`.
            let mut cells = line.strip_prefix('|')?.split('|').map(str::trim);
            if cells.next()? != "parse" {
                return None;
            }
            let code = cells.next()?.trim_matches('`').to_string();
            // No condition in the catalog holds a `|` of its own.
            let condition = cells.next()?.to_string();
            Some((code, condition))
        })
        .collect()
}

#[test]
fn every_lexical_code_is_catalogued() {
    // "A diagnostic uses the catalogued category and code when the
    // specification has one."
    let rows = parse_rows(&chapter("errors-and-diagnostics.md"));
    assert!(!rows.is_empty());
    for code in LEXICAL_CODES {
        assert!(
            rows.iter().any(|(listed, _)| listed == code),
            "`{code}` is not a `parse` code of the diagnostic catalog"
        );
    }
    let distinct: BTreeSet<&str> = LEXICAL_CODES.into_iter().collect();
    assert_eq!(distinct.len(), LEXICAL_CODES.len());
}

#[test]
fn every_catalogued_lexical_code_is_known() {
    // The other direction: a `parse` row whose condition cites a lexical
    // rule is one this crate must be able to report.
    let rows = parse_rows(&chapter("errors-and-diagnostics.md"));
    let lexical: Vec<&str> = rows
        .iter()
        .filter(|(_, condition)| condition.contains("LEX-"))
        .map(|(code, _)| code.as_str())
        .collect();
    assert!(!lexical.is_empty());
    for code in lexical {
        assert!(
            LEXICAL_CODES.contains(&code),
            "the catalog's `{code}` is not a code of this crate"
        );
    }
}
