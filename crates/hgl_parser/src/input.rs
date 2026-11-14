//! The parser's view of the token list.
//!
//! The grammar is written over the tokens that carry meaning. Whitespace,
//! comments and line breaks are not in any production, yet two of them
//! matter: a line break can end a statement (LEX-5), and the tree must keep
//! them all. [`Input`] therefore keeps both lists: the lexer's tokens as
//! they are, and an index of the *significant* ones, each with a count of
//! the line breaks that stand before it.

use hgl_lexer::{Token, TokenKind};
use hgl_source::{SourceFile, Span};

use crate::kind::SyntaxKind;

/// One token the grammar can see.
#[derive(Debug, Clone, Copy)]
struct Significant {
    kind: SyntaxKind,
    /// Its index in the lexer's token list.
    raw: usize,
    /// How many line breaks stand between the previous significant token
    /// and this one. One may be ignored; two or more, as around a blank
    /// line, always separate (LEX-5). The count stops at two.
    line_breaks_before: u8,
}

/// The tokens of one file, as the parser reads them.
pub(crate) struct Input<'a> {
    source: &'a SourceFile,
    tokens: &'a [Token],
    significant: Vec<Significant>,
    /// How many line breaks stand after the last significant token.
    line_breaks_before_end: u8,
}

impl<'a> Input<'a> {
    /// Indexes `tokens`, which the lexer produced from `source`.
    pub(crate) fn new(source: &'a SourceFile, tokens: &'a [Token]) -> Input<'a> {
        let mut significant = Vec::new();
        let mut line_breaks = 0u8;
        for (raw, token) in tokens.iter().enumerate() {
            match token.kind {
                TokenKind::Newline => line_breaks = (line_breaks + 1).min(2),
                // LEX-21: a block or documentation comment that contains a
                // line break counts as one line break where it stands. A
                // line comment ends before its line break, which is its own
                // `Newline` token.
                TokenKind::BlockComment | TokenKind::DocComment => {
                    let text = source.slice(token.span);
                    if text.contains(['\n', '\r']) {
                        line_breaks = (line_breaks + 1).min(2);
                    }
                }
                // LEX-26: the line break inside a line continuation is not
                // a line break. The lexer has reported an `Unknown` token;
                // parsing reads past it, as past a comment.
                TokenKind::Whitespace
                | TokenKind::LineContinuation
                | TokenKind::LineComment
                | TokenKind::Unknown => {}
                kind => {
                    significant.push(Significant {
                        kind: SyntaxKind::from(kind),
                        raw,
                        line_breaks_before: line_breaks,
                    });
                    line_breaks = 0;
                }
            }
        }
        Input {
            source,
            tokens,
            significant,
            line_breaks_before_end: line_breaks,
        }
    }

    /// Number of significant tokens.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.significant.len()
    }

    /// Kind of significant token `index`; [`SyntaxKind::Eof`] past the end.
    pub(crate) fn kind(&self, index: usize) -> SyntaxKind {
        // `map_or(default, f)`: `default` when there is no such token.
        self.significant
            .get(index)
            .map_or(SyntaxKind::Eof, |token| token.kind)
    }

    /// Text of significant token `index`; empty past the end.
    ///
    /// The text is read through the token's span, so the parser never sees
    /// source text outside a token.
    pub(crate) fn text(&self, index: usize) -> &'a str {
        self.significant
            .get(index)
            .map_or("", |token| self.source.slice(self.tokens[token.raw].span))
    }

    /// Span of significant token `index`; an empty span at the end of the
    /// file past the end.
    pub(crate) fn span(&self, index: usize) -> Span {
        match self.significant.get(index) {
            Some(token) => self.tokens[token.raw].span,
            None => Span::empty(self.tokens.last().map_or(0, |token| token.span.end)),
        }
    }

    /// How many line breaks stand between significant token `index` and
    /// the one before it: 0, 1, or 2 for two or more. Past the end,
    /// between the last token and the end of the file.
    pub(crate) fn line_breaks_before(&self, index: usize) -> u8 {
        self.significant
            .get(index)
            .map_or(self.line_breaks_before_end, |token| {
                token.line_breaks_before
            })
    }

    /// True when a line break stands between significant token `index` and
    /// the one before it.
    pub(crate) fn line_break_before(&self, index: usize) -> bool {
        self.line_breaks_before(index) > 0
    }

    /// True when significant token `index` directly follows the one before
    /// it: no whitespace, comment, line break or line continuation stands
    /// between them (LEX-27).
    pub(crate) fn adjacent(&self, index: usize) -> bool {
        // `checked_sub` gives `None` for the first token, which follows
        // nothing; `?` passes that `None` on.
        let neighbours = || {
            let previous = self.significant.get(index.checked_sub(1)?)?;
            let token = self.significant.get(index)?;
            Some(token.raw == previous.raw + 1)
        };
        neighbours().unwrap_or(false)
    }

    /// Index, in the lexer's list, of significant token `index`; the length
    /// of that list past the end.
    pub(crate) fn raw_index(&self, index: usize) -> usize {
        self.significant
            .get(index)
            .map_or(self.tokens.len(), |token| token.raw)
    }

    /// The lexer's tokens, trivia included.
    pub(crate) fn raw_tokens(&self) -> &'a [Token] {
        self.tokens
    }

    /// Text of token `raw` of the lexer's list.
    pub(crate) fn raw_text(&self, raw: usize) -> &'a str {
        self.source.slice(self.tokens[raw].span)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hgl_diagnostics::Diagnostics;
    use hgl_source::SourceMap;

    /// Each significant token of `text` with its line-break flag, then the
    /// flag for the end of the file.
    fn flags(text: &str) -> (Vec<(String, bool)>, bool) {
        let mut sources = SourceMap::new();
        let file = sources.add("test.hgl", text).unwrap();
        let mut diagnostics = Diagnostics::new();
        let tokens = hgl_lexer::lex(&sources, file, &mut diagnostics);
        let input = Input::new(sources.file(file), &tokens);
        let seen = (0..input.len())
            .map(|index| {
                (
                    input.text(index).to_string(),
                    input.line_break_before(index),
                )
            })
            .collect();
        (seen, input.line_break_before(input.len()))
    }

    #[test]
    fn a_line_break_is_noted_on_the_next_token() {
        // LEX-5: the parser asks for the line break; LEX-15: every form
        // counts.
        for line_break in ["\n", "\r\n", "\r"] {
            let (seen, at_end) = flags(&format!("a b{line_break}c"));
            assert_eq!(
                seen,
                [
                    ("a".to_string(), false),
                    ("b".to_string(), false),
                    ("c".to_string(), true)
                ]
            );
            assert!(!at_end);
        }
        assert!(flags("a\n").1);
    }

    #[test]
    fn a_comment_over_several_lines_counts_as_a_line_break() {
        // LEX-21.
        let (seen, _) = flags("a /* one\ntwo */ b /* same line */ c /** doc\n */ d");
        let breaks: Vec<bool> = seen.iter().map(|(_, line_break)| *line_break).collect();
        assert_eq!(breaks, [false, true, false, true]);
    }

    #[test]
    fn line_breaks_are_counted_up_to_two() {
        // LEX-5: one line break may be ignored; two or more always
        // separate. A line that holds only a comment is a line.
        let counts = |text: &str| {
            let mut sources = SourceMap::new();
            let file = sources.add("test.hgl", text).unwrap();
            let tokens = hgl_lexer::lex(&sources, file, &mut Diagnostics::new());
            let input = Input::new(sources.file(file), &tokens);
            (0..=input.len())
                .map(|index| input.line_breaks_before(index))
                .collect::<Vec<u8>>()
        };
        assert_eq!(counts("a b\nc\n\nd\n\n\n\ne"), [0, 0, 1, 2, 2, 0]);
        assert_eq!(counts("a\n# note\nb"), [0, 2, 0]);
        assert_eq!(counts("a # note\nb\n"), [0, 1, 1]);
    }

    #[test]
    fn a_continued_line_break_is_no_line_break() {
        // LEX-26.
        let (seen, _) = flags("a \\\n b \\  \r\n c\n d");
        let breaks: Vec<bool> = seen.iter().map(|(_, line_break)| *line_break).collect();
        assert_eq!(breaks, [false, false, false, true]);
    }

    #[test]
    fn adjacent_tokens_have_nothing_between_them() {
        // LEX-27.
        let adjacent = |text: &str| {
            let mut sources = SourceMap::new();
            let file = sources.add("test.hgl", text).unwrap();
            let tokens = hgl_lexer::lex(&sources, file, &mut Diagnostics::new());
            let input = Input::new(sources.file(file), &tokens);
            (0..=input.len())
                .map(|index| input.adjacent(index))
                .collect::<Vec<bool>>()
        };
        // `f(x)`: `(`, `x` and `)` each touch the token before.
        assert_eq!(adjacent("f(x)"), [false, true, true, true, false]);
        assert_eq!(adjacent("f (x)"), [false, false, true, true, false]);
        assert_eq!(adjacent("f/* */(x)"), [false, false, true, true, false]);
        assert_eq!(adjacent("f\n(x)"), [false, false, true, true, false]);
        assert_eq!(adjacent("f\\\n(x)"), [false, false, true, true, false]);
    }

    #[test]
    fn a_line_comment_leaves_its_line_break() {
        // LEX-21.
        let (seen, _) = flags("a # note\nb");
        assert_eq!(seen[1], ("b".to_string(), true));
    }

    #[test]
    fn past_the_end_is_the_end_of_the_file() {
        let mut sources = SourceMap::new();
        let file = sources.add("test.hgl", "ab ").unwrap();
        let mut diagnostics = Diagnostics::new();
        let tokens = hgl_lexer::lex(&sources, file, &mut diagnostics);
        let input = Input::new(sources.file(file), &tokens);
        assert_eq!(input.kind(1), SyntaxKind::Eof);
        assert_eq!(input.text(1), "");
        assert_eq!(input.span(1), Span::empty(3));
        assert_eq!(input.raw_index(1), 2);
    }
}
