//! Reads `# expect-error` annotations (TST-12).
//!
//! ```text
//! # expect-error(type, "rolling.size_kind")
//! fn invalid(value: rolling<f64, 5m, 3>) { when {} }
//! ```
//!
//! An annotation is a line comment, alone on its line, of the form
//!
//! ```text
//! expect_error = "#", gap, "expect-error", "(", gap, error_category, gap, ",",
//!                gap, STRING_LITERAL, gap, ")", gap ;
//! gap = { ? U+0020 or U+0009 ? } ;
//! ```
//!
//! It says that the next line of source that is not itself an annotation
//! is rejected, with that category and code. The production gives the
//! characters of the comment, not tokens: the lexer has made the whole
//! comment one token, and this module reads inside it.

use hgl_diagnostics::{Category, Location, catalogue};
use hgl_lexer::literal::decode_string;
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode};
use hgl_source::{FileId, SourceFile};

use crate::Problem;

/// One annotation: what it expects and which line it expects it on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expectation {
    /// The category expected.
    pub category: Category,
    /// The code expected. It is in the catalogue under that category.
    pub code: String,
    /// Where the annotation is written.
    pub at: Location,
    /// The line the annotation applies to, counted from 0: the next line
    /// of its file that is not an annotation.
    pub line: usize,
}

/// What a line comment is.
#[derive(Debug, PartialEq, Eq)]
enum Comment {
    /// An ordinary comment.
    Plain,
    /// An annotation, with its category and code.
    Annotation(Category, String),
    /// A comment that begins `expect-error` and is not an annotation:
    /// invalid metadata (TST-12), with what is wrong.
    Invalid(String),
}

/// Reads the annotations of one file, in source order. What is wrong
/// with one that cannot be read is added to `problems`.
pub(crate) fn read(
    source: &SourceFile,
    file: FileId,
    root: &SyntaxNode,
    problems: &mut Vec<Problem>,
) -> Vec<Expectation> {
    // Each annotation as (its own line, category, code, location), the
    // line it applies to still unknown.
    let mut found: Vec<(usize, Category, String, Location)> = Vec::new();
    let comments = root
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| token.kind() == SyntaxKind::LineComment);
    for comment in comments {
        let at = Location::new(file, comment.span());
        let (category, code) = match classify(comment.text()) {
            Comment::Plain => continue,
            Comment::Invalid(reason) => {
                problems.push(Problem::at(at, reason));
                continue;
            }
            Comment::Annotation(category, code) => (category, code),
        };
        let line = source.line_index(at.span.start);
        if !alone_on_line(source, line, at.span.start) {
            problems.push(Problem::at(
                at,
                "an `expect-error` annotation stands alone on its line (TST-12)",
            ));
            continue;
        }
        found.push((line, category, code, at));
    }

    // TST-12: an annotation "applies to the next physical source line
    // that is not itself an annotation, so several may precede one line".
    let annotated: Vec<usize> = found.iter().map(|(line, ..)| *line).collect();
    let lines = source.line_count() as usize;
    let mut expectations = Vec::new();
    for (line, category, code, at) in found {
        let mut target = line + 1;
        while annotated.contains(&target) {
            target += 1;
        }
        if target >= lines {
            problems.push(Problem::at(
                at,
                "an `expect-error` annotation applies to the next line, and none follows (TST-12)",
            ));
            continue;
        }
        expectations.push(Expectation {
            category,
            code,
            at,
            line: target,
        });
    }
    expectations
}

/// True when only spaces and tabs stand before byte `offset` on line
/// `line`.
fn alone_on_line(source: &SourceFile, line: usize, offset: u32) -> bool {
    let Some(range) = source.line_range(line) else {
        return false;
    };
    source.text()[range.start..offset as usize]
        .chars()
        .all(|character| character == ' ' || character == '\t')
}

/// `text` without the spaces and tabs it starts with: the grammar's
/// `gap`.
fn after_gap(text: &str) -> &str {
    text.trim_start_matches([' ', '\t'])
}

/// Reads the text of one line comment against the `expect_error`
/// production.
fn classify(comment: &str) -> Comment {
    // A line comment starts with `#`. `strip_prefix` gives the rest, or
    // `None` when the text starts with something else.
    let Some(body) = comment.strip_prefix('#') else {
        return Comment::Plain;
    };
    let Some(rest) = after_gap(body).strip_prefix("expect-error") else {
        return Comment::Plain;
    };
    // TST-12: from here the comment "begins `expect-error` after `#` and
    // any spaces or tabs". It is an annotation or it is invalid.
    let invalid = |reason: &str| {
        Comment::Invalid(format!(
            "invalid `expect-error` annotation: {reason} (TST-12)"
        ))
    };
    let Some(rest) = rest.strip_prefix('(') else {
        return invalid("`(` directly follows `expect-error`");
    };
    let rest = after_gap(rest);

    // The category runs up to the gap or the comma after it.
    let end = rest.find([' ', '\t', ',', ')', '"']).unwrap_or(rest.len());
    let (category_text, rest) = rest.split_at(end);
    let Some(rest) = after_gap(rest).strip_prefix(',') else {
        return invalid("a comma follows the category");
    };
    let rest = after_gap(rest);

    // The code is a string literal: from its `"` to the next `"` that no
    // backslash precedes.
    if !rest.starts_with('"') {
        return invalid("the code is a string literal");
    }
    let mut end = None;
    let mut escaped = false;
    // `char_indices` gives each character with its byte position;
    // `skip(1)` steps over the opening quote.
    for (index, character) in rest.char_indices().skip(1) {
        match character {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '"' => {
                end = Some(index);
                break;
            }
            _ => {}
        }
    }
    let Some(end) = end else {
        return invalid("the code's string has no closing quote");
    };
    // `..=end` includes the closing quote, which is one byte.
    let (literal, rest) = rest.split_at(end + 1);
    let Ok(code) = decode_string(literal) else {
        return invalid("the code is not a valid string literal");
    };

    let Some(rest) = after_gap(rest).strip_prefix(')') else {
        return invalid("`)` follows the code");
    };
    if !after_gap(rest).is_empty() {
        return invalid("nothing follows `)` on the line");
    }

    // TST-12: "Category/code must be catalogued."
    let Some(category) = Category::from_name(category_text) else {
        return invalid(&format!("`{category_text}` is not a source category"));
    };
    if !catalogue::contains(category, &code) {
        return invalid(&format!(
            "the diagnostic catalogue has no `{code}` in category `{category_text}`"
        ));
    }
    Comment::Annotation(category, code)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The reason a comment is invalid, without the words around it.
    fn reason(comment: &str) -> String {
        match classify(comment) {
            Comment::Invalid(reason) => reason
                .trim_start_matches("invalid `expect-error` annotation: ")
                .trim_end_matches(" (TST-12)")
                .to_string(),
            other => panic!("{comment:?} is {other:?}"),
        }
    }

    /// TST-12 and the `expect_error` production: a gap of spaces and
    /// tabs is allowed in six places and nowhere else.
    #[test]
    fn the_form_is_the_production() {
        let expected = Comment::Annotation(Category::Type, "rolling.size_kind".to_string());
        for comment in [
            "# expect-error(type, \"rolling.size_kind\")",
            "#expect-error(type,\"rolling.size_kind\")",
            "#\t expect-error( \ttype \t, \t\"rolling.size_kind\" \t) \t",
        ] {
            assert_eq!(classify(comment), expected, "{comment:?}");
        }
        assert_eq!(
            classify("# expect-error(function-kind, \"x\")"),
            Comment::Invalid(
                "invalid `expect-error` annotation: the diagnostic catalogue has no `x` in \
                 category `function-kind` (TST-12)"
                    .to_string()
            )
        );
        // Any row of the catalogue may be expected, not only a `parse`
        // one.
        assert_eq!(
            classify("# expect-error(name, \"name.duplicate\")"),
            Comment::Annotation(Category::Name, "name.duplicate".to_string())
        );
        // The code is a string literal, read as one (LEX-24).
        assert_eq!(
            classify("# expect-error(parse, \"string\\u{2E}unclosed\")"),
            Comment::Annotation(Category::Parse, "string.unclosed".to_string())
        );
    }

    #[test]
    fn other_comments_are_plain() {
        for comment in [
            "#",
            "# a comment",
            "# expects-error(type, \"rolling.size_kind\")",
            "# see expect-error(type, \"rolling.size_kind\")",
            "# Expect-Error(type, \"rolling.size_kind\")",
            "## expect-error(type, \"rolling.size_kind\")",
        ] {
            assert_eq!(classify(comment), Comment::Plain, "{comment:?}");
        }
    }

    /// TST-12: "A line comment that begins `expect-error` after `#` and
    /// any spaces or tabs, without that form, is invalid metadata".
    #[test]
    fn a_comment_that_begins_expect_error_without_the_form_is_invalid() {
        let cases = [
            ("# expect-error", "`(` directly follows `expect-error`"),
            (
                "# expect-error (type, \"rolling.size_kind\")",
                "`(` directly follows `expect-error`",
            ),
            (
                "# expect-errors(type, \"rolling.size_kind\")",
                "`(` directly follows `expect-error`",
            ),
            (
                "# expect-error(type \"rolling.size_kind\")",
                "a comma follows the category",
            ),
            (
                "# expect-error(type, rolling.size_kind)",
                "the code is a string literal",
            ),
            (
                "# expect-error(type, \"rolling.size_kind)",
                "the code's string has no closing quote",
            ),
            (
                "# expect-error(type, \"rolling\\q\")",
                "the code is not a valid string literal",
            ),
            (
                "# expect-error(type, \"rolling.size_kind\"",
                "`)` follows the code",
            ),
            (
                "# expect-error(type, \"rolling.size_kind\", \"again\")",
                "`)` follows the code",
            ),
            (
                "# expect-error(type, \"rolling.size_kind\") and more",
                "nothing follows `)` on the line",
            ),
            (
                "# expect-error(types, \"rolling.size_kind\")",
                "`types` is not a source category",
            ),
            (
                "# expect-error(build, \"rolling.size_kind\")",
                "`build` is not a source category",
            ),
            (
                "# expect-error(, \"rolling.size_kind\")",
                "`` is not a source category",
            ),
            // The category is part of what is catalogued.
            (
                "# expect-error(parse, \"rolling.size_kind\")",
                "the diagnostic catalogue has no `rolling.size_kind` in category `parse`",
            ),
            // An execution code is not a source code.
            (
                "# expect-error(type, \"value.missing_key\")",
                "the diagnostic catalogue has no `value.missing_key` in category `type`",
            ),
            // Nor is a code local to this compiler.
            (
                "# expect-error(parse, \"doc.unattached\")",
                "the diagnostic catalogue has no `doc.unattached` in category `parse`",
            ),
        ];
        for (comment, expected) in cases {
            assert_eq!(reason(comment), expected, "{comment:?}");
        }
    }
}
