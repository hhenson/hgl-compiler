//! Tokens: the words and symbols source text is cut into.

use hgl_source::Span;

/// Declares [`TokenKind`] and its lookup tables from one list.
///
/// This is Rust's version of the C++ "X-macro" technique. The reserved words
/// and punctuation are written once, at the bottom of this file, as
/// `Variant => "spelling"` pairs. The macro expands that list three times:
/// into enum variants, into [`TokenKind::keyword`] (spelling to kind) and
/// into [`TokenKind::fixed_text`] (kind to spelling), so the three can never
/// disagree.
///
/// Reading the pattern: `$( ... ),*` means "repeat for every comma-separated
/// entry"; `$name:ident` captures an identifier and `$text:literal` a string.
macro_rules! define_token_kinds {
    (
        keywords { $( $keyword:ident => $keyword_text:literal ),* $(,)? }
        punctuation { $( $punct:ident => $punct_text:literal ),* $(,)? }
    ) => {
        /// What a token is.
        ///
        /// The lexer classifies by *intent*: a string that is missing its
        /// closing quote is still [`TokenKind::Str`], with the problem
        /// reported as a diagnostic. That gives the parser the best chance of
        /// carrying on. [`TokenKind::Unknown`] is reserved for characters
        /// that cannot start any token.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum TokenKind {
            /// A run of spaces and tabs.
            Whitespace,
            /// One line break: LF (`\n`), CRLF (`\r\n`) or CR (`\r`), the
            /// Linux, Windows and classic Mac forms. All three mean the same
            /// (see `hgl_source::line_break_length`). Whether it ends a
            /// statement depends on context (LEX-5), which the parser
            /// decides.
            Newline,
            /// `# ...` up to, not including, the line break (LEX-3).
            LineComment,
            /// `/* ... */`, non-nesting (LEX-3).
            BlockComment,
            /// `/** ... */`, a documentation comment (SDOC-1).
            DocComment,
            /// A name: `[A-Za-z_][A-Za-z0-9_]*` that is neither reserved nor
            /// the lone placeholder (LEX-1). Contextual names such as `const`
            /// and `list` (LEX-10) are identifiers; the parser gives them
            /// meaning by position.
            Ident,
            /// The lone `_` placeholder.
            Placeholder,
            /// An integer literal: digits only. A leading minus is a separate
            /// operator token (LEX-7).
            Int,
            /// A float literal: `1.5`, `1e5`, `1.5e-3`.
            Float,
            /// A string literal in double quotes (LEX-2).
            Str,
            /// A duration literal such as `1h30m` (LEX-8, LEX-13).
            Duration,
            /// A temporal literal introduced by `@`: a date, time, date-time
            /// or time zone (LEX-11, LEX-12).
            Temporal,
            $(
                #[doc = concat!("The reserved word `", $keyword_text, "`.")]
                $keyword,
            )*
            $(
                #[doc = concat!("The symbol `", $punct_text, "`.")]
                $punct,
            )*
            /// A character that cannot start a token. Always accompanied by a
            /// diagnostic.
            Unknown,
        }

        impl TokenKind {
            /// Every reserved word with its kind, in specification order.
            pub const KEYWORDS: &'static [(&'static str, TokenKind)] =
                &[ $( ($keyword_text, TokenKind::$keyword), )* ];

            /// Every punctuation symbol with its kind.
            pub const PUNCTUATION: &'static [(&'static str, TokenKind)] =
                &[ $( ($punct_text, TokenKind::$punct), )* ];

            /// The kind of a hard reserved word (the list in source-text.md), or `None`
            /// when `text` is not reserved.
            pub fn keyword(text: &str) -> Option<TokenKind> {
                match text {
                    $( $keyword_text => Some(TokenKind::$keyword), )*
                    _ => None,
                }
            }

            /// The spelling of a kind that is always written the same way (a
            /// reserved word or symbol), or `None` for kinds whose text
            /// varies, such as identifiers and literals.
            pub fn fixed_text(self) -> Option<&'static str> {
                match self {
                    $( TokenKind::$keyword => Some($keyword_text), )*
                    $( TokenKind::$punct => Some($punct_text), )*
                    _ => None,
                }
            }
        }
    };
}

define_token_kinds! {
    // The hard reserved words, in the order source-text.md lists them.
    keywords {
        KwModule => "module",
        KwPart => "part",
        KwUse => "use",
        KwAs => "as",
        KwExport => "export",
        KwAbstract => "abstract",
        KwImpl => "impl",
        KwInstantiate => "instantiate",
        KwOperator => "operator",
        KwFn => "fn",
        KwCpp => "cpp",
        KwStruct => "struct",
        KwEnum => "enum",
        KwRequires => "requires",
        KwIs => "is",
        KwLet => "let",
        KwVar => "var",
        KwState => "state",
        KwCache => "cache",
        KwInject => "inject",
        KwReturn => "return",
        KwIf => "if",
        KwElse => "else",
        KwStart => "start",
        KwWhen => "when",
        KwStop => "stop",
        KwFor => "for",
        KwWhile => "while",
        KwYield => "yield",
        KwTest => "test",
        KwAssert => "assert",
        KwEval => "eval",
        KwTrue => "true",
        KwFalse => "false",
        KwNull => "null",
        KwBool => "bool",
        KwI64 => "i64",
        KwF64 => "f64",
        KwStr => "str",
        KwBytes => "bytes",
        KwAny => "any",
        KwDate => "date",
        KwTime => "time",
        KwDatetime => "datetime",
        KwDuration => "duration",
        KwCivilDatetime => "civil_datetime",
        KwZonedDatetime => "zoned_datetime",
        KwZonedTime => "zoned_time",
        KwTimezone => "timezone",
    }
    // Every symbol the chapter grammars use as a terminal. The lexer takes
    // the longest symbol that matches (LEX-14 states this for `<=>`).
    punctuation {
        LParen => "(",
        RParen => ")",
        LBracket => "[",
        RBracket => "]",
        LBrace => "{",
        RBrace => "}",
        Comma => ",",
        Semicolon => ";",
        Colon => ":",
        ColonColon => "::",
        Dot => ".",
        Ellipsis => "...",
        At => "@",
        Question => "?",
        Bang => "!",
        Plus => "+",
        Minus => "-",
        Star => "*",
        Slash => "/",
        SlashSlash => "//",
        Percent => "%",
        AmpAmp => "&&",
        PipePipe => "||",
        Eq => "=",
        PlusEq => "+=",
        MinusEq => "-=",
        StarEq => "*=",
        SlashEq => "/=",
        EqEq => "==",
        BangEq => "!=",
        Lt => "<",
        LtEq => "<=",
        Gt => ">",
        GtEq => ">=",
        Spaceship => "<=>",
        Arrow => "->",
        FatArrow => "=>",
    }
}

impl TokenKind {
    /// True for whitespace and comments: text the grammar never mentions.
    ///
    /// [`TokenKind::Newline`] is not included, because a line break can be
    /// significant (LEX-5). [`TokenKind::DocComment`] is included here but
    /// the parser attaches it to the declaration that follows (SDOC-1).
    pub fn is_trivia(self) -> bool {
        matches!(
            self,
            TokenKind::Whitespace
                | TokenKind::LineComment
                | TokenKind::BlockComment
                | TokenKind::DocComment
        )
    }

    /// True for a hard reserved word.
    pub fn is_keyword(self) -> bool {
        TokenKind::KEYWORDS.iter().any(|&(_, kind)| kind == self)
    }

    /// True for a literal token (`true`, `false` and `null` are reserved
    /// words and are not included).
    pub fn is_literal(self) -> bool {
        matches!(
            self,
            TokenKind::Int
                | TokenKind::Float
                | TokenKind::Str
                | TokenKind::Duration
                | TokenKind::Temporal
        )
    }
}

/// One token: its kind and the source bytes it covers.
///
/// A token does not hold its text. Slice the source with the span to get it:
/// `&text[token.span.range()]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    /// What the token is.
    pub kind: TokenKind,
    /// Where it is.
    pub span: Span,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_words_match_the_specification_list() {
        // The hard reserved words, copied verbatim from source-text.md.
        let specified = "module part use as export abstract impl instantiate operator fn cpp struct enum \
                         requires is let var state cache inject return if else start when stop \
                         for while yield test assert eval true false null bool i64 f64 str bytes any \
                         date time datetime duration civil_datetime zoned_datetime zoned_time timezone";
        let listed: Vec<&str> = TokenKind::KEYWORDS.iter().map(|&(text, _)| text).collect();
        let expected: Vec<&str> = specified.split_whitespace().collect();
        assert_eq!(listed, expected);
    }

    #[test]
    fn keyword_lookup_and_spelling_round_trip() {
        for &(text, kind) in TokenKind::KEYWORDS {
            assert_eq!(TokenKind::keyword(text), Some(kind));
            assert_eq!(kind.fixed_text(), Some(text));
            assert!(kind.is_keyword());
        }
        assert_eq!(
            TokenKind::keyword("const"),
            None,
            "`const` is contextual (LEX-9)"
        );
        assert_eq!(
            TokenKind::keyword("list"),
            None,
            "`list` is contextual (LEX-10)"
        );
        assert_eq!(TokenKind::Ident.fixed_text(), None);
    }

    #[test]
    fn trivia_excludes_newlines() {
        assert!(TokenKind::Whitespace.is_trivia());
        assert!(TokenKind::LineComment.is_trivia());
        assert!(!TokenKind::Newline.is_trivia());
        assert!(!TokenKind::Ident.is_trivia());
    }
}
