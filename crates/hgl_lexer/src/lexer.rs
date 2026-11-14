//! The lexer proper: one pass over the source bytes, producing tokens.
//!
//! The lexer is a hand-written scanner: it looks at the next byte, decides
//! which kind of token starts there, and consumes bytes until that token
//! ends. All of HGL's structural characters are ASCII, so scanning works on
//! bytes; non-ASCII text can appear only inside strings and comments, where
//! it is skipped over, or as a stray character, which is reported.
//!
//! Two properties hold for every input, valid or not:
//!
//! * **Lossless.** The tokens cover the source text exactly, with no gaps
//!   and no overlaps. Joining their text reproduces it. The formatter
//!   depends on this to preserve comments.
//! * **Total.** Lexing never fails or stops early. Problems are reported to
//!   the diagnostics sink and scanning continues.
//!
//! A token is cut by its form alone: at every position the longest token
//! that matches is taken, whatever came before (LEX-17).

use hgl_diagnostics::{Category, Code, Diagnostic, Diagnostics, Location};
use hgl_source::{FileId, SourceMap, Span, line_break_length};

use crate::shape::{self, NumberForm, TextBlockError, is_zone_character};
use crate::token::{Token, TokenKind};

/// The lexer's diagnostic codes.
///
/// Each is a `parse` code of the specification's diagnostic catalog
/// (errors-and-diagnostics.md), so they are reported as
/// [`Code::Catalogued`]. The rule a rejection rests on is cited on the
/// diagnostic. A constant's name is its code in upper case.
pub mod codes {
    /// Source that is not well-formed UTF-8 (LEX-16).
    pub const SYNTAX_INVALID_ENCODING: &str = "syntax.invalid_encoding";
    /// A character that begins no token (LEX-16).
    pub const SYNTAX_UNEXPECTED_CHARACTER: &str = "syntax.unexpected_character";
    /// A string that reaches a line break or the end of the file, or a text
    /// block that reaches the end of the file (LEX-2).
    pub const STRING_UNCLOSED: &str = "string.unclosed";
    /// An escape outside the admitted set (LEX-2).
    pub const STRING_UNKNOWN_ESCAPE: &str = "string.unknown_escape";
    /// A `\u{...}` that names no Unicode scalar value (LEX-24).
    pub const STRING_INVALID_ESCAPE: &str = "string.invalid_escape";
    /// A text block whose layout breaks LEX-25.
    pub const STRING_MALFORMED_TEXT_BLOCK: &str = "string.malformed_text_block";
    /// A block or documentation comment that reaches the end of the file
    /// (LEX-2).
    pub const COMMENT_UNCLOSED: &str = "comment.unclosed";
    /// A numeric or duration run that is not one literal (LEX-8, LEX-18).
    pub const NUMBER_MALFORMED: &str = "number.malformed";
    /// An `@` run that matches no temporal form (LEX-18).
    pub const DATE_TIME_MALFORMED: &str = "date_time.malformed";
}

/// The note that lists the escapes, shown with every unknown one.
const ESCAPES_NOTE: &str = r#"the escapes are \" \\ \n \r \t \u{...}"#;

/// Splits one source file into tokens.
///
/// Returns every token, including whitespace, line breaks and comments, in
/// source order. Lexical errors are appended to `diagnostics`; the returned
/// tokens are complete either way.
///
/// The lexer checks the *shape* of literals only. Whether `@2026-02-30` is a
/// real date or `30m1h` orders its units correctly is decided by
/// [`crate::literal`], which also produces the literal's value.
///
/// A file whose bytes are not UTF-8 (LEX-16) gets that one diagnostic and
/// no other. Its tokens cover the text before the first offending byte.
pub fn lex(sources: &SourceMap, file: FileId, diagnostics: &mut Diagnostics) -> Vec<Token> {
    let source = sources.file(file);
    // `let ... else` binds the pattern or runs the `else` block, which must
    // leave the function. Here: the usual case, a well-formed file.
    let Some(error) = source.encoding_error() else {
        return scan(file, source.text(), diagnostics);
    };

    let diagnostic = Diagnostic::error(
        Category::Parse,
        Code::Catalogued(codes::SYNTAX_INVALID_ENCODING),
        Location::new(file, Span::empty(error.offset)),
        "source is not well-formed UTF-8",
    )
    .with_rule("LEX-16")
    .with_note(format!(
        "the byte 0x{:02X} here begins no UTF-8 character; the rest of the file was not read",
        error.byte
    ));
    diagnostics.emit(diagnostic);
    // The text stops where the encoding broke, perhaps in the middle of a
    // string or comment, so what the scanner would say about it is noise.
    // It goes to a sink that is thrown away.
    scan(file, source.text(), &mut Diagnostics::new())
}

/// Cuts `text`, the text of `file`, into tokens.
fn scan(file: FileId, text: &str, diagnostics: &mut Diagnostics) -> Vec<Token> {
    let mut lexer = Lexer {
        file,
        text,
        bytes: text.as_bytes(),
        pos: 0,
        diagnostics,
    };
    let mut tokens = Vec::new();
    while lexer.pos < lexer.bytes.len() {
        let start = lexer.pos;
        let reports_before = lexer.diagnostics.len();
        let kind = lexer.next_kind();
        debug_assert!(lexer.pos > start, "every token consumes at least one byte");
        tokens.push(Token {
            kind,
            span: span(start, lexer.pos),
            malformed: lexer.diagnostics.len() > reports_before,
        });
    }
    tokens
}

/// Builds a [`Span`] from `usize` offsets. The casts cannot truncate:
/// [`hgl_source::SourceFile`] refuses text longer than `u32::MAX` bytes.
fn span(start: usize, end: usize) -> Span {
    Span::new(start as u32, end as u32)
}

/// True for a byte that can start an identifier (LEX-1).
fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

/// True for a byte that can continue an identifier (LEX-1). These are also
/// the bytes a numeric or temporal literal cannot run into (LEX-18).
fn is_ident_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// The ASCII letters `text` starts with.
fn leading_letters(text: &str) -> &str {
    let length = text
        .bytes()
        .take_while(|byte| byte.is_ascii_alphabetic())
        .count();
    &text[..length]
}

/// The scanner's state while lexing one file.
///
/// The `'a` is a lifetime parameter: it says the lexer borrows the source
/// text and the sink, and cannot outlive either. C++ has the same constraint
/// for a struct holding a `std::string_view`, but leaves it unchecked.
struct Lexer<'a> {
    /// The file being lexed, recorded on each diagnostic.
    file: FileId,
    /// The source as text, for slicing out words.
    text: &'a str,
    /// The same source as bytes, for scanning.
    bytes: &'a [u8],
    /// Offset of the next unread byte.
    pos: usize,
    /// Where problems are reported.
    diagnostics: &'a mut Diagnostics,
}

impl<'a> Lexer<'a> {
    // ---- Cursor helpers -------------------------------------------------

    /// The next unread byte, or `None` at the end of the file.
    fn peek(&self) -> Option<u8> {
        self.peek_at(0)
    }

    /// The byte `ahead` positions past the next unread byte, or `None` past
    /// the end of the file.
    fn peek_at(&self, ahead: usize) -> Option<u8> {
        self.bytes.get(self.pos + ahead).copied()
    }

    /// Consumes bytes for as long as `accept` holds.
    fn eat_while(&mut self, accept: impl Fn(u8) -> bool) {
        while self.peek().is_some_and(&accept) {
            self.pos += 1;
        }
    }

    /// True when the `count` bytes starting `ahead` positions on are all
    /// ASCII digits.
    fn digits_at(&self, ahead: usize, count: usize) -> bool {
        (0..count).all(|i| {
            self.peek_at(ahead + i)
                .is_some_and(|byte| byte.is_ascii_digit())
        })
    }

    /// True when the next three bytes are `"""`, the text block delimiter.
    fn at_triple_quote(&self) -> bool {
        self.bytes[self.pos..].starts_with(b"\"\"\"")
    }

    /// Starts an error diagnostic for the bytes `start..end`. Every lexical
    /// error is in the `parse` category and carries one of [`codes`].
    fn error(
        &self,
        start: usize,
        end: usize,
        code: &'static str,
        message: impl Into<String>,
    ) -> Diagnostic {
        Diagnostic::error(
            Category::Parse,
            Code::Catalogued(code),
            Location::new(self.file, span(start, end)),
            message,
        )
    }

    // ---- Dispatch -------------------------------------------------------

    /// Consumes one token starting at `self.pos` and returns its kind.
    fn next_kind(&mut self) -> TokenKind {
        match self.bytes[self.pos] {
            // LEX-16: spaces and tabs are the only whitespace.
            b' ' | b'\t' => {
                self.eat_while(|byte| byte == b' ' || byte == b'\t');
                TokenKind::Whitespace
            }
            // LEX-15: a line break is LF, CRLF or a lone CR. Each is one
            // token and none is preferred; `line_break_length` says how
            // long this one is.
            b'\n' | b'\r' => {
                self.pos += line_break_length(&self.bytes[self.pos..])
                    .expect("a line feed or carriage return starts a line break");
                TokenKind::Newline
            }
            b'\\' => self.backslash(),
            b'#' => self.line_comment(),
            // `if` after a pattern is a guard: the arm applies only when the
            // condition also holds.
            b'/' if self.peek_at(1) == Some(b'*') => self.block_comment(),
            // Three quotes always open a text block; they are never an
            // empty string followed by a quote (LEX-25).
            b'"' if self.at_triple_quote() => self.text_block(),
            b'"' => self.string(),
            b'0'..=b'9' => self.number(),
            b'@' => self.temporal_or_at(),
            byte if is_ident_start(byte) => self.word(),
            _ => self.punctuation(),
        }
    }

    // ---- Comments (LEX-3, LEX-21) ---------------------------------------

    /// `# ...` to the end of the line. The line break (LF, CRLF or CR) is
    /// left for the next token: it stays meaningful (LEX-3, LEX-21).
    fn line_comment(&mut self) -> TokenKind {
        self.eat_while(|byte| byte != b'\n' && byte != b'\r');
        TokenKind::LineComment
    }

    /// `/* ... */`. Comments do not nest, so the first `*/` closes it.
    /// A comment that opens with `/**` is a documentation comment (SDOC-1);
    /// `/**/` is an ordinary empty comment (LEX-21).
    fn block_comment(&mut self) -> TokenKind {
        let start = self.pos;
        self.pos += 2;
        match self.text[self.pos..].find("*/") {
            Some(offset) => self.pos += offset + 2,
            None => {
                self.pos = self.bytes.len();
                let diagnostic = self
                    .error(
                        start,
                        self.pos,
                        codes::COMMENT_UNCLOSED,
                        "comment has no closing `*/`",
                    )
                    .with_rule("LEX-2");
                self.diagnostics.emit(diagnostic);
            }
        }
        let comment = &self.text[start..self.pos];
        if comment.starts_with("/**") && comment != "/**/" {
            TokenKind::DocComment
        } else {
            TokenKind::BlockComment
        }
    }

    // ---- Strings (LEX-2, LEX-24, LEX-25) --------------------------------

    /// A double-quoted string. It must close on the line it opens on: the
    /// grammar excludes line breaks (LF, CRLF and CR alike) from
    /// `string_character`.
    fn string(&mut self) -> TokenKind {
        let start = self.pos;
        self.pos += 1;
        loop {
            match self.peek() {
                None | Some(b'\n' | b'\r') => {
                    let diagnostic = self
                        .error(
                            start,
                            self.pos,
                            codes::STRING_UNCLOSED,
                            "string has no closing quote",
                        )
                        .with_rule("LEX-2")
                        .with_note("a string must close on the line it opens on")
                        .with_note(r#"text over several lines goes in a text block: """ ... """"#);
                    self.diagnostics.emit(diagnostic);
                    break;
                }
                Some(b'"') => {
                    self.pos += 1;
                    break;
                }
                Some(b'\\') => self.escape(false),
                // Any other byte, including the bytes of a multi-byte
                // character, is string content.
                Some(_) => self.pos += 1,
            }
        }
        TokenKind::Str
    }

    /// A text block: a string over several lines (LEX-25).
    ///
    /// ```text
    /// let usage = """        a line break directly follows the opening
    ///     Usage: run         the text, one or more lines
    ///     """                the closing delimiter, after spaces and tabs
    /// ```
    ///
    /// The block runs to the next `"""`, wherever that is, and is one token.
    /// Its layout is then checked; at most one layout fault is reported for
    /// a block. The value (indentation removed, lines joined) is worked out
    /// by [`crate::literal::decode_string`].
    fn text_block(&mut self) -> TokenKind {
        let start = self.pos;
        self.pos += 3;
        let body = self.pos;
        loop {
            match self.peek() {
                None => {
                    let diagnostic = self
                        .error(
                            start,
                            self.pos,
                            codes::STRING_UNCLOSED,
                            r#"text block has no closing `"""`"#,
                        )
                        .with_rule("LEX-2");
                    self.diagnostics.emit(diagnostic);
                    return TokenKind::Str;
                }
                Some(b'"') if self.at_triple_quote() => break,
                // An escape is skipped whole, so `\"""` does not close the
                // block.
                Some(b'\\') => self.escape(true),
                // Anything else is text, line breaks included.
                Some(_) => self.pos += 1,
            }
        }
        let close = self.pos;
        self.pos += 3;

        let fault = match shape::text_block_layout(&self.text[body..close]) {
            Ok(_) => return TokenKind::Str,
            Err(fault) => fault,
        };
        // The catalog places the diagnostic at the "offending delimiter or
        // line".
        let diagnostic = match fault {
            TextBlockError::Opening => self
                .error(
                    start,
                    body,
                    codes::STRING_MALFORMED_TEXT_BLOCK,
                    r#"a line break must directly follow the opening `"""` of a text block"#,
                )
                .with_note(
                    r#"the text starts on the next line; a string on one line is written "text""#,
                ),
            TextBlockError::Closing => self
                .error(
                    close,
                    close + 3,
                    codes::STRING_MALFORMED_TEXT_BLOCK,
                    r#"only spaces and tabs may come before the closing `"""` of a text block"#,
                )
                .with_note(r#"to put `"""` in the text, escape its first quote: `\"""`"#),
            TextBlockError::Line(line) => self
                .error(
                    body + line.start,
                    body + line.end,
                    codes::STRING_MALFORMED_TEXT_BLOCK,
                    r#"this line does not start with the indentation of the closing `"""`"#,
                )
                .with_note(
                    r#"the spaces and tabs before the closing `"""` are removed from every line, so each line with text must start with exactly them"#,
                ),
        };
        self.diagnostics.emit(diagnostic.with_rule("LEX-25"));
        TokenKind::Str
    }

    /// A backslash escape inside a string, starting at the backslash.
    /// `in_text_block` says which kind of string holds it: only a text block
    /// carries on past a line break.
    fn escape(&mut self, in_text_block: bool) {
        let start = self.pos;
        match self.peek_at(1) {
            Some(b'"' | b'\\' | b'n' | b'r' | b't') => self.pos += 2,
            Some(b'u') => self.unicode_escape(),
            // A backslash at the end of the file: consume it and let the
            // caller report the unclosed string.
            None => self.pos += 1,
            Some(b'\n' | b'\r') => {
                self.pos += 1;
                // On one line the caller reports the unclosed string. A
                // text block continues, so the backslash is the fault.
                if in_text_block {
                    let diagnostic = self
                        .error(
                            start,
                            self.pos,
                            codes::STRING_UNKNOWN_ESCAPE,
                            "a backslash at the end of a line is not an escape",
                        )
                        .with_rule("LEX-2")
                        .with_note(ESCAPES_NOTE);
                    self.diagnostics.emit(diagnostic);
                }
            }
            Some(_) => {
                // Take the whole character after the backslash, which may be
                // several bytes, so the span ends on a character boundary.
                let escaped = self.text[start + 1..]
                    .chars()
                    .next()
                    .expect("a byte follows the backslash");
                self.pos += 1 + escaped.len_utf8();
                let diagnostic = self
                    .error(
                        start,
                        self.pos,
                        codes::STRING_UNKNOWN_ESCAPE,
                        format!("unknown escape `\\{escaped}`"),
                    )
                    .with_rule("LEX-2")
                    .with_note(ESCAPES_NOTE);
                self.diagnostics.emit(diagnostic);
            }
        }
    }

    /// A `\u` escape, starting at the backslash.
    ///
    /// Two different faults are possible. Text that is not `\u{`, digits,
    /// `}` is not an escape at all (LEX-2); only the `\u` is consumed. A
    /// well-formed escape that names no character is consumed whole and
    /// reported under LEX-24.
    fn unicode_escape(&mut self) {
        let start = self.pos;
        match shape::unicode_escape(&self.bytes[start..]) {
            Some((length, Some(_))) => self.pos += length,
            Some((length, None)) => {
                self.pos += length;
                let escape = &self.text[start..self.pos];
                let diagnostic = self
                    .error(
                        start,
                        self.pos,
                        codes::STRING_INVALID_ESCAPE,
                        format!("`{escape}` names no Unicode scalar value"),
                    )
                    .with_rule("LEX-24")
                    .with_note(
                        "a scalar value is 0 to 10FFFF except the surrogates D800 to DFFF, written with one to six hexadecimal digits",
                    );
                self.diagnostics.emit(diagnostic);
            }
            None => {
                self.pos += 2;
                let diagnostic = self
                    .error(
                        start,
                        self.pos,
                        codes::STRING_UNKNOWN_ESCAPE,
                        r"`\u` must be followed by `{`, one to six hexadecimal digits and `}`",
                    )
                    .with_rule("LEX-2")
                    .with_note(ESCAPES_NOTE);
                self.diagnostics.emit(diagnostic);
            }
        }
    }

    // ---- Names (LEX-1) --------------------------------------------------

    /// An identifier, a reserved word, or the lone `_` placeholder.
    fn word(&mut self) -> TokenKind {
        let start = self.pos;
        self.eat_while(is_ident_continue);
        let word = &self.text[start..self.pos];
        if word == "_" {
            TokenKind::Placeholder
        } else {
            // `unwrap_or` supplies the value to use when the lookup found
            // nothing (`std::optional::value_or`).
            TokenKind::keyword(word).unwrap_or(TokenKind::Ident)
        }
    }

    // ---- Numbers and durations (LEX-7, LEX-8, LEX-18, LEX-22, LEX-23) ---

    /// An integer, float or duration literal, starting at a digit.
    ///
    /// [`shape::scan_number`] finds the longest literal there. LEX-18 then
    /// decides whether it stands: a literal can end only where the next
    /// character is not a letter, digit or `_`. When one of those follows,
    /// the whole run up to the next other character is one malformed token:
    /// `12abc` is never `12` then `abc`.
    ///
    /// There is no sign here: `-5` is the minus operator and then `5`
    /// (LEX-7).
    fn number(&mut self) -> TokenKind {
        let start = self.pos;
        let number = shape::scan_number(&self.bytes[start..]);
        self.pos += number.length;
        if !self.peek().is_some_and(is_ident_continue) {
            return match number.form {
                NumberForm::Decimal | NumberForm::Hexadecimal | NumberForm::Binary => {
                    TokenKind::Int
                }
                NumberForm::Float { .. } => TokenKind::Float,
                NumberForm::Duration => TokenKind::Duration,
            };
        }
        let literal_end = self.pos;
        self.eat_while(is_ident_continue);
        self.malformed_number(start, literal_end, number.form)
    }

    /// Reports the run `start..self.pos` as not one literal and returns the
    /// kind closest to what was meant. `form` is the literal that matched up
    /// to `literal_end`, where a letter, digit or `_` follows it.
    ///
    /// ```text
    /// 10x       Duration   digits then letters are a duration candidate
    /// 1e        Float      an exponent with no digits
    /// 1e5m      Float      letters after an exponent (LEX-8)
    /// 1__000    Int        two `_` between digits (LEX-22)
    /// 0b102     Int        a digit outside the base
    /// 1h30      Duration   a part with no unit
    /// ```
    fn malformed_number(
        &mut self,
        start: usize,
        literal_end: usize,
        form: NumberForm,
    ) -> TokenKind {
        let run = &self.text[start..self.pos];
        let rest = &self.text[literal_end..self.pos];
        let is_digit = |byte: u8| match form {
            NumberForm::Hexadecimal => byte.is_ascii_hexdigit(),
            _ => byte.is_ascii_digit(),
        };

        // LEX-22 names the fault when underscores stand between two digits
        // but there is more than one of them. Everything else is LEX-18,
        // except the one case LEX-8 spells out (below).
        let mut rule = "LEX-18";
        if rest.starts_with("__")
            && is_digit(self.bytes[literal_end - 1])
            && rest
                .trim_start_matches('_')
                .bytes()
                .next()
                .is_some_and(is_digit)
        {
            rule = "LEX-22";
        }

        let (kind, message, note) = if run.starts_with("0x") || run.starts_with("0X") {
            (
                TokenKind::Int,
                format!("`{run}` is not a hexadecimal integer"),
                "write `0x` in lowercase, then hexadecimal digits; one `_` may stand between two digits",
            )
        } else if run.starts_with("0b") || run.starts_with("0B") {
            (
                TokenKind::Int,
                format!("`{run}` is not a binary integer"),
                "write `0b` in lowercase, then the digits 0 and 1; one `_` may stand between two digits",
            )
        } else if rest.starts_with('_') {
            let (kind, what) = match form {
                NumberForm::Duration => (TokenKind::Duration, "a duration"),
                NumberForm::Float { .. } => (TokenKind::Float, "a number"),
                _ => (TokenKind::Int, "a number"),
            };
            (
                kind,
                format!("`{run}` is not {what}: `_` may stand only between two digits"),
                "one `_` may separate two digits, as in `1_000`",
            )
        } else if matches!(form, NumberForm::Float { exponent: true }) {
            rule = "LEX-8";
            (
                TokenKind::Float,
                format!("`{run}` is not a number: letters follow the exponent"),
                "a float with an exponent cannot take a duration unit",
            )
        } else if form == NumberForm::Duration {
            // After the last unit comes another number. Either nothing
            // usable follows it or its unit is unknown.
            let number = shape::digit_run_length(rest.as_bytes(), |byte| byte.is_ascii_digit());
            let unit = leading_letters(&rest[number..]);
            let fault = if unit.is_empty() {
                "its last number has no unit".to_string()
            } else {
                format!("`{unit}` is not a duration unit")
            };
            (
                TokenKind::Duration,
                format!("`{run}` is not a duration: {fault}"),
                "each part of a duration is a number followed by one of d, h, m, s, ms, us",
            )
        } else {
            // A decimal integer or plain float with letters after it.
            let letters = leading_letters(rest);
            if letters == "e" || letters == "E" {
                (
                    TokenKind::Float,
                    format!("`{run}` is not a number: its exponent has no digits"),
                    "an exponent is `e` or `E`, an optional sign, then digits: `1e5`, `2.5e-3`",
                )
            } else {
                (
                    TokenKind::Duration,
                    format!(
                        "`{run}` is not a number or a duration: `{letters}` is not a duration unit"
                    ),
                    "digits followed by letters are read as a duration; the units are d, h, m, s, ms, us",
                )
            }
        };

        let diagnostic = self
            .error(start, self.pos, codes::NUMBER_MALFORMED, message)
            .with_rule(rule)
            .with_note(note);
        self.diagnostics.emit(diagnostic);
        kind
    }

    // ---- Temporal literals (LEX-11, LEX-12, LEX-18) ---------------------

    /// A token starting with `@`: a temporal literal when a digit or `[`
    /// follows (LEX-18), otherwise the `@` symbol that introduces an
    /// annotation such as `@suppress_warning(...)`.
    fn temporal_or_at(&mut self) -> TokenKind {
        let start = self.pos;
        self.pos += 1;
        match self.peek() {
            Some(b'[') => {
                self.zone_literal(start);
                TokenKind::Temporal
            }
            Some(byte) if byte.is_ascii_digit() => {
                self.date_or_time_literal(start);
                TokenKind::Temporal
            }
            _ => TokenKind::At,
        }
    }

    /// `@[Zone/Name]`, with the cursor on the `[`.
    fn zone_literal(&mut self, start: usize) {
        if let Some(length) = self.zone_annotation_length() {
            self.pos += length;
            if !self.peek().is_some_and(is_ident_continue) {
                return;
            }
            // LEX-18: the literal runs straight into a name or number.
            self.eat_while(is_ident_continue);
        } else {
            // Take the zone characters there are. If a `]` comes later on
            // this line, take the text up to it as well, so that the whole
            // attempt is one token.
            self.pos += 1;
            self.eat_while(|byte| is_zone_character(byte) || byte == b'/');
            let rest = &self.bytes[self.pos..];
            let line_length = rest
                .iter()
                .position(|&byte| byte == b'\n' || byte == b'\r')
                .unwrap_or(rest.len());
            if let Some(index) = rest[..line_length].iter().position(|&byte| byte == b']') {
                self.pos += index + 1;
            }
        }
        let literal = &self.text[start..self.pos];
        let diagnostic = self
            .error(
                start,
                self.pos,
                codes::DATE_TIME_MALFORMED,
                format!("malformed time-zone literal `{literal}`"),
            )
            .with_rule("LEX-18")
            .with_note("write `@[Area/Location]`: components of letters, digits and `_ - + .` separated by `/`");
        self.diagnostics.emit(diagnostic);
    }

    /// The body of a date, date-time or time literal, with the cursor on the
    /// first digit after `@`.
    ///
    /// ```text
    /// @2026-09-03                              date
    /// @2026-09-03T09:30                        date and time
    /// @2026-09-03T09:30:15.250Z                ...with a UTC offset
    /// @2026-09-03T09:30+01:00[Europe/London]   ...and a zone
    /// @09:30                                   time
    /// @09:30[Europe/London]                    time in a zone
    /// ```
    ///
    /// Each optional piece is consumed only when it is completely present,
    /// so `@09:30 - @09:00` and `@09:30-@09:00` both lex as two literals
    /// and a minus.
    fn date_or_time_literal(&mut self, start: usize) {
        let mut well_formed = true;
        if self.date_length().is_some() {
            self.pos += 10;
            // The time part needs an uppercase `T` (LEX-11). `&& let` joins
            // a plain test and a pattern match in one condition: the block
            // runs only when the `T` is there and a time follows it.
            if self.peek() == Some(b'T')
                && let Some(time) = self.clock_time_length(1)
            {
                self.pos += 1 + time;
                // A zone may follow only an explicit offset (LEX-11).
                if let Some(offset) = self.utc_offset_length() {
                    self.pos += offset;
                    self.pos += self.zone_annotation_length().unwrap_or(0);
                }
            }
        } else if let Some(time) = self.clock_time_length(0) {
            self.pos += time;
            self.pos += self.zone_annotation_length().unwrap_or(0);
        } else {
            well_formed = false;
        }

        // LEX-18: a literal that matches no form, or that runs straight
        // into a letter, digit or `_`, is rejected whole:
        // `@2026-09-03t09:30` (lowercase t), `@09:30:15.1234567` (a seventh
        // fraction digit), `@12` (neither a date nor a time).
        if !well_formed || self.peek().is_some_and(is_ident_continue) {
            self.eat_while(|byte| is_ident_continue(byte) || byte == b':');
            let literal = &self.text[start..self.pos];
            let diagnostic = self
                .error(
                    start,
                    self.pos,
                    codes::DATE_TIME_MALFORMED,
                    format!("malformed temporal literal `{literal}`"),
                )
                .with_rule("LEX-18")
                .with_note("forms: @YYYY-MM-DD, @YYYY-MM-DDTHH:MM[:SS[.ffffff]] with optional Z or +HH[:MM], @HH:MM[:SS[.ffffff]]")
                .with_note("`T` and `Z` are uppercase; fields have exactly the digits shown");
            self.diagnostics.emit(diagnostic);
        }
    }

    /// The length (always 10) of a `YYYY-MM-DD` date at the cursor, or
    /// `None`.
    fn date_length(&self) -> Option<usize> {
        let present = self.digits_at(0, 4)
            && self.peek_at(4) == Some(b'-')
            && self.digits_at(5, 2)
            && self.peek_at(7) == Some(b'-')
            && self.digits_at(8, 2);
        present.then_some(10)
    }

    /// The length of an `HH:MM[:SS[.ffffff]]` clock time starting `ahead`
    /// bytes past the cursor, or `None`. At most six fraction digits are
    /// taken (LEX-11).
    fn clock_time_length(&self, ahead: usize) -> Option<usize> {
        let hours_and_minutes = self.digits_at(ahead, 2)
            && self.peek_at(ahead + 2) == Some(b':')
            && self.digits_at(ahead + 3, 2);
        if !hours_and_minutes {
            return None;
        }
        let mut length = 5;
        let seconds =
            self.peek_at(ahead + length) == Some(b':') && self.digits_at(ahead + length + 1, 2);
        if seconds {
            length += 3;
            let fraction =
                self.peek_at(ahead + length) == Some(b'.') && self.digits_at(ahead + length + 1, 1);
            if fraction {
                length += 1;
                let mut digits = 0;
                while digits < 6 && self.digits_at(ahead + length, 1) {
                    length += 1;
                    digits += 1;
                }
            }
        }
        Some(length)
    }

    /// The length of a UTC offset at the cursor: `Z`, `+HH`, `-HH`, `+HH:MM`
    /// or `-HH:MM`; or `None`. A sign followed by anything but exactly two
    /// digits is left as an operator. Whether the hours and minutes exist
    /// (LEX-19) is checked with the literal's value.
    fn utc_offset_length(&self) -> Option<usize> {
        match self.peek() {
            Some(b'Z') => Some(1),
            Some(b'+' | b'-') if self.digits_at(1, 2) => {
                let mut length = 3;
                if self.peek_at(length) == Some(b':') && self.digits_at(length + 1, 2) {
                    length += 3;
                }
                // `-123` is subtraction of 123, not an offset `-12` then `3`.
                if self.digits_at(length, 1) {
                    None
                } else {
                    Some(length)
                }
            }
            _ => None,
        }
    }

    /// The length of a `[...]` zone annotation at the cursor, or `None`
    /// when the text there is not one.
    ///
    /// The lexer asks only that zone characters and `/` stand between the
    /// brackets. That the components between the slashes are non-empty and
    /// well named is LEX-12's rule about the name, checked with the
    /// literal's value: `@[Europe//London]` is one token here and an
    /// invalid zone there.
    fn zone_annotation_length(&self) -> Option<usize> {
        if self.peek() != Some(b'[') {
            return None;
        }
        let mut length = 1;
        while self
            .peek_at(length)
            .is_some_and(|byte| is_zone_character(byte) || byte == b'/')
        {
            length += 1;
        }
        (self.peek_at(length) == Some(b']')).then_some(length + 1)
    }

    // ---- Punctuation (LEX-14, LEX-17) -----------------------------------

    /// A symbol, taking the longest that matches: `<=>` before `<=` before
    /// `<` (LEX-17). Anything else is an unexpected character.
    ///
    /// No symbol joins `>` to a following `>`, so the two closers of
    /// `list<list<i64>>` are separate tokens (LEX-8). `>` directly followed
    /// by `=` is `>=` wherever it stands.
    ///
    /// The match looks at up to three bytes. `_` in a pattern means "any
    /// value", so `(b'<', Some(b'='), _)` reads "`<`, then `=`, then
    /// anything". Arms are tried top to bottom, which is why longer symbols
    /// come first.
    fn punctuation(&mut self) -> TokenKind {
        use TokenKind::*;
        let (kind, length) = match (self.bytes[self.pos], self.peek_at(1), self.peek_at(2)) {
            (b'.', Some(b'.'), Some(b'.')) => (Ellipsis, 3),
            (b'<', Some(b'='), Some(b'>')) => (Spaceship, 3),
            (b'/', Some(b'/'), Some(b'=')) => (SlashSlashEq, 3),
            (b':', Some(b':'), _) => (ColonColon, 2),
            (b'/', Some(b'/'), _) => (SlashSlash, 2),
            (b'/', Some(b'='), _) => (SlashEq, 2),
            (b'%', Some(b'='), _) => (PercentEq, 2),
            (b'&', Some(b'&'), _) => (AmpAmp, 2),
            (b'|', Some(b'|'), _) => (PipePipe, 2),
            (b'+', Some(b'='), _) => (PlusEq, 2),
            (b'-', Some(b'='), _) => (MinusEq, 2),
            (b'-', Some(b'>'), _) => (Arrow, 2),
            (b'*', Some(b'='), _) => (StarEq, 2),
            (b'=', Some(b'='), _) => (EqEq, 2),
            (b'=', Some(b'>'), _) => (FatArrow, 2),
            (b'!', Some(b'='), _) => (BangEq, 2),
            (b'<', Some(b'='), _) => (LtEq, 2),
            (b'>', Some(b'='), _) => (GtEq, 2),
            (b'(', ..) => (LParen, 1),
            (b')', ..) => (RParen, 1),
            (b'[', ..) => (LBracket, 1),
            (b']', ..) => (RBracket, 1),
            (b'{', ..) => (LBrace, 1),
            (b'}', ..) => (RBrace, 1),
            (b',', ..) => (Comma, 1),
            (b';', ..) => (Semicolon, 1),
            (b':', ..) => (Colon, 1),
            (b'.', ..) => (Dot, 1),
            (b'?', ..) => (Question, 1),
            (b'!', ..) => (Bang, 1),
            (b'+', ..) => (Plus, 1),
            (b'-', ..) => (Minus, 1),
            (b'*', ..) => (Star, 1),
            (b'/', ..) => (Slash, 1),
            (b'%', ..) => (Percent, 1),
            (b'=', ..) => (Eq, 1),
            (b'<', ..) => (Lt, 1),
            (b'>', ..) => (Gt, 1),
            _ => return self.unexpected_character(),
        };
        self.pos += length;
        kind
    }

    /// A `\` between tokens: a line continuation when a line break
    /// follows it with only spaces or tabs between, and otherwise a
    /// character that begins no token (LEX-26).
    fn backslash(&mut self) -> TokenKind {
        let start = self.pos;
        let mut end = start + 1;
        while matches!(self.bytes.get(end), Some(b' ' | b'\t')) {
            end += 1;
        }
        if let Some(line_break) = line_break_length(&self.bytes[end..]) {
            // The token takes in the line break, so no `Newline` token
            // follows and the parser sees no line break here.
            self.pos = end + line_break;
            return TokenKind::LineContinuation;
        }
        self.pos = start + 1;
        let diagnostic = self
            .error(
                start,
                self.pos,
                codes::SYNTAX_UNEXPECTED_CHARACTER,
                "unexpected character '\\\\'",
            )
            .with_rule("LEX-26")
            .with_note(
                "`\\` continues a line when only spaces or tabs stand between it and the line break",
            );
        self.diagnostics.emit(diagnostic);
        TokenKind::Unknown
    }

    /// A character that begins no token (LEX-16). The whole character is
    /// consumed, which may be several bytes, so the next token starts on a
    /// character boundary.
    fn unexpected_character(&mut self) -> TokenKind {
        let start = self.pos;
        let character = self.text[start..]
            .chars()
            .next()
            .expect("the cursor is before the end of the text");
        self.pos += character.len_utf8();
        // `{:?}` prints the character quoted, with control characters
        // escaped, so an invisible character is still readable.
        let message = format!("unexpected character {character:?}");
        let diagnostic = self
            .error(start, self.pos, codes::SYNTAX_UNEXPECTED_CHARACTER, message)
            .with_rule("LEX-16");
        self.diagnostics.emit(diagnostic);
        TokenKind::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use TokenKind::*;

    /// Lexes `text` and returns the tokens with the diagnostics.
    fn lex_all(text: &str) -> (Vec<Token>, Diagnostics) {
        let mut sources = SourceMap::new();
        let file = sources.add("test.hgl", text).unwrap();
        let mut diagnostics = Diagnostics::new();
        let tokens = lex(&sources, file, &mut diagnostics);
        (tokens, diagnostics)
    }

    /// Lexes `text` and returns the tokens with the diagnostics' codes.
    fn lex_text(text: &str) -> (Vec<Token>, Vec<&'static str>) {
        let (tokens, diagnostics) = lex_all(text);
        let codes = diagnostics
            .as_slice()
            .iter()
            .map(|item| item.code.as_str())
            .collect();
        (tokens, codes)
    }

    /// The kind and text of every token except whitespace, for source that
    /// must lex without error.
    fn tokens_of(text: &str) -> Vec<(TokenKind, &str)> {
        let (tokens, codes) = lex_text(text);
        assert!(
            codes.is_empty(),
            "unexpected diagnostics {codes:?} for {text:?}"
        );
        significant(text, &tokens)
    }

    /// The kind and text of every token except whitespace.
    fn significant<'t>(text: &'t str, tokens: &[Token]) -> Vec<(TokenKind, &'t str)> {
        tokens
            .iter()
            .filter(|token| token.kind != Whitespace)
            .map(|token| (token.kind, &text[token.span.range()]))
            .collect()
    }

    /// The kinds only, for source that must lex without error.
    fn kinds_of(text: &str) -> Vec<TokenKind> {
        tokens_of(text).into_iter().map(|(kind, _)| kind).collect()
    }

    /// The diagnostic codes and significant tokens for source that must be
    /// rejected.
    fn rejected(text: &str) -> (Vec<&'static str>, Vec<(TokenKind, &str)>) {
        let (tokens, codes) = lex_text(text);
        assert!(!codes.is_empty(), "expected a diagnostic for {text:?}");
        (codes, significant(text, &tokens))
    }

    /// Each diagnostic for `text` as its code, the rule it cites and the
    /// source text it points at.
    fn reports(text: &str) -> Vec<(&'static str, &'static str, &str)> {
        let (_, diagnostics) = lex_all(text);
        diagnostics
            .as_slice()
            .iter()
            .map(|item| {
                assert_eq!(item.category, Category::Parse);
                assert!(item.code.is_catalogued(), "{}", item.code.as_str());
                (
                    item.code.as_str(),
                    item.rule.expect("every lexical diagnostic cites a rule"),
                    &text[item.primary.span.range()],
                )
            })
            .collect()
    }

    #[test]
    fn tokens_cover_the_source_exactly() {
        // Valid and invalid input alike: no gaps, no overlaps.
        for text in [
            "",
            "fn f(a: i64) -> i64 { when { return a } }\n",
            "let s = \"caf\u{e9}\" # tr\u{e8}s bien\r\n",
            "\"unclosed\n/* unclosed",
            "\u{a3} \u{1f600} 10x @12 \"\\q\"",
            "0x 1__0 0b102 1e @[a b] @[ \"\\u{D800}\\u\"",
            "\"\"\"\n  text\n  \"\"\" \"\"\"one line\"\"\"",
            "\"\"\"\n never closed \\",
        ] {
            let (tokens, _) = lex_text(text);
            let mut expected_start = 0;
            for token in &tokens {
                assert_eq!(
                    token.span.start, expected_start,
                    "gap or overlap in {text:?}"
                );
                assert!(!token.span.is_empty());
                expected_start = token.span.end;
            }
            assert_eq!(
                expected_start as usize,
                text.len(),
                "tokens stop short in {text:?}"
            );
        }
    }

    #[test]
    fn names_reserved_words_and_the_placeholder_are_distinct() {
        // LEX-1.
        assert_eq!(
            tokens_of("value _ __ _x fn state2 i64"),
            [
                (Ident, "value"),
                (Placeholder, "_"),
                (Ident, "__"),
                (Ident, "_x"),
                (KwFn, "fn"),
                (Ident, "state2"),
                (KwI64, "i64"),
            ]
        );
    }

    #[test]
    fn contextual_names_are_identifiers() {
        // LEX-9, LEX-10: meaning comes from position, which the parser knows.
        for name in [
            "const",
            "native",
            "type",
            "switch",
            "case",
            "default",
            "in",
            "each",
            "unbounded",
            "properties",
            "raises",
        ] {
            assert_eq!(kinds_of(name), [Ident], "{name}");
        }
    }

    #[test]
    fn built_in_type_names_are_reserved() {
        // The hard reserved words: the names of the types the language
        // defines are reserved, as the scalar type names are.
        assert_eq!(
            kinds_of("atomic tuple list set dict rolling result ref signal delta error"),
            [
                KwAtomic, KwTuple, KwList, KwSet, KwDict, KwRolling, KwResult, KwRef, KwSignal,
                KwDelta, KwError
            ]
        );
        // A longer name that starts with one is a name (LEX-1).
        assert_eq!(kinds_of("listing results set_point"), [Ident, Ident, Ident]);
    }

    #[test]
    fn non_ascii_letters_do_not_form_identifiers() {
        // LEX-1 admits ASCII letters only.
        let (codes, tokens) = rejected("caf\u{e9}");
        assert_eq!(codes, [codes::SYNTAX_UNEXPECTED_CHARACTER]);
        assert_eq!(tokens, [(Ident, "caf"), (Unknown, "\u{e9}")]);
    }

    #[test]
    fn every_line_break_form_is_one_newline_token() {
        // LEX-15: LF (Linux), CRLF (Windows) and CR (classic Mac), in one
        // file.
        assert_eq!(
            tokens_of("a\nb\r\nc\rd"),
            [
                (Ident, "a"),
                (Newline, "\n"),
                (Ident, "b"),
                (Newline, "\r\n"),
                (Ident, "c"),
                (Newline, "\r"),
                (Ident, "d")
            ]
        );
        // CRLF is one line break; CR CR and LF CR are two.
        assert_eq!(kinds_of("\r\n"), [Newline]);
        assert_eq!(kinds_of("\r\r"), [Newline, Newline]);
        assert_eq!(kinds_of("\n\r"), [Newline, Newline]);
        assert_eq!(kinds_of("\r\r\n"), [Newline, Newline]);
    }

    #[test]
    fn line_break_forms_give_the_same_tokens() {
        // LEX-15.
        // The same source under each convention differs only in the text of
        // its line-break tokens.
        fn without_line_breaks(text: &str) -> Vec<(TokenKind, &str)> {
            tokens_of(text)
                .into_iter()
                .filter(|(kind, _)| *kind != Newline)
                .collect()
        }
        let lf = "fn f(a: i64) -> i64 {\n    # note\n    a + 1\n}\n";
        for line_break in ["\r\n", "\r"] {
            let text = lf.replace('\n', line_break);
            assert_eq!(kinds_of(&text), kinds_of(lf), "{text:?}");
            assert_eq!(
                without_line_breaks(&text),
                without_line_breaks(lf),
                "{text:?}"
            );
        }
    }

    #[test]
    fn a_backslash_and_its_line_break_are_one_token() {
        // LEX-26. The token takes in the spaces and tabs between, and the
        // whole line break, so no `Newline` follows it.
        for (text, continuation) in [
            ("a \\\nb", "\\\n"),
            ("a \\\r\nb", "\\\r\n"),
            ("a \\\rb", "\\\r"),
            ("a \\ \t \nb", "\\ \t \n"),
        ] {
            assert_eq!(
                tokens_of(text),
                [(Ident, "a"), (LineContinuation, continuation), (Ident, "b")],
                "{text:?}"
            );
        }
        assert!(LineContinuation.is_trivia());
        // Two in a row join three lines.
        assert_eq!(kinds_of("a \\\n \\\n b").len(), 4);
    }

    #[test]
    fn a_backslash_not_before_a_line_break_begins_no_token() {
        // LEX-26. Only the `\` is rejected; lexing goes on after it.
        for text in ["a \\ b", "a \\ # note\nb", "a \\"] {
            assert_eq!(
                reports(text),
                [(codes::SYNTAX_UNEXPECTED_CHARACTER, "LEX-26", "\\")],
                "{text:?}"
            );
        }
        let (_, tokens) = rejected("a \\ b");
        assert_eq!(tokens, [(Ident, "a"), (Unknown, "\\"), (Ident, "b")]);
        // In a comment a backslash is text, and in a string an escape.
        assert_eq!(kinds_of("a # \\\nb"), [Ident, LineComment, Newline, Ident]);
    }

    #[test]
    fn only_spaces_and_tabs_are_whitespace() {
        // LEX-16.
        assert_eq!(kinds_of("a \t b"), [Ident, Ident]);
        // A form feed, a vertical tab, a no-break space and a byte order
        // mark anywhere but the start of the file begin no token.
        for character in ['\u{c}', '\u{b}', '\u{a0}', '\u{feff}'] {
            let text = format!("a{character}b");
            assert_eq!(
                reports(&text),
                [(
                    codes::SYNTAX_UNEXPECTED_CHARACTER,
                    "LEX-16",
                    &text[1..1 + character.len_utf8()]
                )],
                "{character:?}"
            );
        }
    }

    #[test]
    fn one_leading_byte_order_mark_is_ignored() {
        // LEX-16. The source map drops it, so offsets start at the first
        // character after it.
        let (tokens, codes) = lex_text("\u{feff}a");
        assert_eq!(codes, [] as [&str; 0]);
        assert_eq!(tokens.len(), 1);
        assert_eq!((tokens[0].kind, tokens[0].span), (Ident, span(0, 1)));
        // A second one is an ordinary stray character.
        let (tokens, codes) = lex_text("\u{feff}\u{feff}a");
        assert_eq!(codes, [codes::SYNTAX_UNEXPECTED_CHARACTER]);
        assert_eq!((tokens[0].kind, tokens[0].span), (Unknown, span(0, 3)));
    }

    #[test]
    fn source_that_is_not_utf8_gets_one_diagnostic() {
        // LEX-16. 0xFF begins no UTF-8 character. The string it sits in is
        // cut short; that is not reported as an unclosed string.
        let mut sources = SourceMap::new();
        let file = sources
            .add_bytes("test.hgl", b"let a = \"x\xff\"\nlet b = 1\n".to_vec())
            .unwrap();
        let mut diagnostics = Diagnostics::new();
        let tokens = lex(&sources, file, &mut diagnostics);

        assert_eq!(diagnostics.len(), 1);
        let diagnostic = &diagnostics.as_slice()[0];
        assert_eq!(diagnostic.category, Category::Parse);
        assert_eq!(
            diagnostic.code,
            Code::Catalogued(codes::SYNTAX_INVALID_ENCODING)
        );
        assert_eq!(diagnostic.rule, Some("LEX-16"));
        // The primary location is the first offending byte.
        assert_eq!(diagnostic.primary.span, Span::empty(10));

        // The tokens cover the text before that byte.
        let text = sources.file(file).text();
        assert_eq!(text, "let a = \"x");
        assert_eq!(
            significant(text, &tokens),
            [(KwLet, "let"), (Ident, "a"), (Eq, "="), (Str, "\"x")]
        );
    }

    #[test]
    fn line_comments_stop_before_the_line_ending() {
        // LEX-3, LEX-21.
        assert_eq!(
            tokens_of("a # note \"x\" /* y\nb"),
            [
                (Ident, "a"),
                (LineComment, "# note \"x\" /* y"),
                (Newline, "\n"),
                (Ident, "b")
            ]
        );
        assert_eq!(
            tokens_of("# c\r\n"),
            [(LineComment, "# c"), (Newline, "\r\n")]
        );
        assert_eq!(
            tokens_of("# c\rb"),
            [(LineComment, "# c"), (Newline, "\r"), (Ident, "b")]
        );
    }

    #[test]
    fn block_comments_do_not_nest() {
        // LEX-3: the first `*/` closes the comment.
        assert_eq!(
            tokens_of("/* a /* b */ c */"),
            [
                (BlockComment, "/* a /* b */"),
                (Ident, "c"),
                (Star, "*"),
                (Slash, "/")
            ]
        );
        assert_eq!(
            tokens_of("/* line 1\nline 2 */"),
            [(BlockComment, "/* line 1\nline 2 */")]
        );
        // LEX-21: a comment separates tokens as whitespace does.
        assert_eq!(kinds_of("a/* */b"), [Ident, BlockComment, Ident]);
    }

    #[test]
    fn documentation_comments_are_recognised() {
        // SDOC-1, LEX-21.
        assert_eq!(kinds_of("/** Doc. */"), [DocComment]);
        assert_eq!(kinds_of("/**/"), [BlockComment]);
        assert_eq!(kinds_of("/* plain */"), [BlockComment]);
    }

    #[test]
    fn unclosed_block_comment_is_rejected() {
        // LEX-2. The diagnostic points at the comment, which runs to the
        // end of the file.
        let text = "a /* never closed\nb";
        let (codes, tokens) = rejected(text);
        assert_eq!(codes, [codes::COMMENT_UNCLOSED]);
        assert_eq!(tokens, [(Ident, "a"), (BlockComment, "/* never closed\nb")]);
        assert_eq!(
            reports(text),
            [(codes::COMMENT_UNCLOSED, "LEX-2", "/* never closed\nb")]
        );
        let (codes, tokens) = rejected("/** doc");
        assert_eq!(codes, [codes::COMMENT_UNCLOSED]);
        assert_eq!(tokens, [(DocComment, "/** doc")]);
    }

    #[test]
    fn double_slash_is_floor_division_not_a_comment() {
        // LEX-3.
        assert_eq!(kinds_of("a // b"), [Ident, SlashSlash, Ident]);
    }

    #[test]
    fn strings_take_escapes_and_unicode() {
        // LEX-2.
        assert_eq!(
            tokens_of(r#""" "a b" "q\"q" "\\" "\n\r\t""#),
            [
                (Str, r#""""#),
                (Str, r#""a b""#),
                (Str, r#""q\"q""#),
                (Str, r#""\\""#),
                (Str, r#""\n\r\t""#)
            ]
        );
        assert_eq!(tokens_of("\"h\u{e9}llo\""), [(Str, "\"h\u{e9}llo\"")]);
        // A `#` or `/*` inside a string is content.
        assert_eq!(kinds_of("\"# not /* a comment\""), [Str]);
    }

    #[test]
    fn unknown_escape_is_rejected_and_the_string_continues() {
        // LEX-2.
        let (codes, tokens) = rejected(r#""a\qb" c"#);
        assert_eq!(codes, [codes::STRING_UNKNOWN_ESCAPE]);
        assert_eq!(tokens, [(Str, r#""a\qb""#), (Ident, "c")]);
        assert_eq!(
            reports(r#""a\qb" c"#),
            [(codes::STRING_UNKNOWN_ESCAPE, "LEX-2", r"\q")]
        );
        // A multi-byte character after the backslash is taken whole.
        let (codes, tokens) = rejected("\"\\\u{e9}\"");
        assert_eq!(codes, [codes::STRING_UNKNOWN_ESCAPE]);
        assert_eq!(tokens, [(Str, "\"\\\u{e9}\"")]);
    }

    #[test]
    fn unicode_escapes_name_scalar_values() {
        // LEX-24.
        assert_eq!(
            tokens_of(r#""\u{20AC}" "\u{41}\u{10ffff}" "\u{000041}""#),
            [
                (Str, r#""\u{20AC}""#),
                (Str, r#""\u{41}\u{10ffff}""#),
                (Str, r#""\u{000041}""#)
            ]
        );
        // A surrogate, a number above 10FFFF and a seventh digit name no
        // scalar value. The escape is taken whole and the string goes on.
        for escape in [r"\u{D800}", r"\u{DFFF}", r"\u{110000}", r"\u{0000041}"] {
            let text = format!("\"a{escape}b\" c");
            assert_eq!(
                reports(&text),
                [(codes::STRING_INVALID_ESCAPE, "LEX-24", escape)],
                "{escape}"
            );
            let (_, tokens) = rejected(&text);
            assert_eq!(tokens, [(Str, &text[..text.len() - 2]), (Ident, "c")]);
        }
    }

    #[test]
    fn a_unicode_escape_needs_braces_and_digits() {
        // LEX-2: without them `\u` is not one of the escapes.
        for body in [r"\u", r"\u41", r"\u{}", r"\u{41", r"\u{4G}", r"\u{ 41}"] {
            let text = format!("\"{body}\"");
            assert_eq!(
                reports(&text),
                [(codes::STRING_UNKNOWN_ESCAPE, "LEX-2", r"\u")],
                "{body}"
            );
            let (_, tokens) = rejected(&text);
            assert_eq!(tokens, [(Str, text.as_str())], "{body}");
        }
    }

    #[test]
    fn unclosed_string_ends_at_the_line_ending() {
        // LEX-2.
        let (codes, tokens) = rejected("\"abc\nnext");
        assert_eq!(codes, [codes::STRING_UNCLOSED]);
        assert_eq!(tokens, [(Str, "\"abc"), (Newline, "\n"), (Ident, "next")]);
        assert_eq!(
            reports("\"abc\nnext"),
            [(codes::STRING_UNCLOSED, "LEX-2", "\"abc")]
        );
        for line_break in ["\r\n", "\r"] {
            let text = format!("\"abc{line_break}next");
            let (codes, tokens) = rejected(&text);
            assert_eq!(codes, [codes::STRING_UNCLOSED]);
            assert_eq!(
                tokens,
                [(Str, "\"abc"), (Newline, line_break), (Ident, "next")]
            );
        }
        let (codes, _) = rejected("\"abc\\");
        assert_eq!(codes, [codes::STRING_UNCLOSED]);
        // A backslash before the line break adds nothing to the report.
        let (codes, _) = rejected("\"abc\\\nnext");
        assert_eq!(codes, [codes::STRING_UNCLOSED]);
        // An escaped quote does not close the string.
        let (codes, _) = rejected(r#""abc\""#);
        assert_eq!(codes, [codes::STRING_UNCLOSED]);
    }

    #[test]
    fn a_text_block_is_one_string_token() {
        // LEX-25, with the chapter's illustration.
        let block = "\"\"\"\n    Usage: run [options]\n      -v  verbose\n    \"\"\"";
        let lf = format!("let usage = {block}\nnext");
        // LEX-15: the line breaks inside may be of any form.
        for line_break in ["\n", "\r\n", "\r"] {
            let text = lf.replace('\n', line_break);
            let block = block.replace('\n', line_break);
            assert_eq!(
                tokens_of(&text),
                [
                    (KwLet, "let"),
                    (Ident, "usage"),
                    (Eq, "="),
                    (Str, block.as_str()),
                    (Newline, line_break),
                    (Ident, "next")
                ],
                "{text:?}"
            );
        }
    }

    #[test]
    fn text_blocks_hold_quotes_blank_lines_and_escapes() {
        // LEX-25.
        for text in [
            // No lines at all.
            "\"\"\"\n\"\"\"",
            "\"\"\"\n    \"\"\"",
            // One or two quotes are text.
            "\"\"\"\n  say \"hi\" and \"\"\n  \"\"\"",
            // Blank lines need no indentation; deeper lines keep theirs.
            "\"\"\"\n  a\n\n      b\n \n  \"\"\"",
            // An escaped first quote keeps `\"\"\"` from closing the block.
            "\"\"\"\n  \\\"\"\"\n  \"\"\"",
            "\"\"\"\n  \\u{20AC} \\n \\\\\n  \"\"\"",
            // Tabs indent as well as spaces.
            "\"\"\"\n\ta\n\t\"\"\"",
        ] {
            assert_eq!(tokens_of(text), [(Str, text)], "{text:?}");
        }
    }

    #[test]
    fn three_quotes_always_open_a_text_block() {
        // LEX-25: never an empty string and then a quote.
        let text = "\"\"\"text\"\"\" next";
        let (codes, tokens) = rejected(text);
        assert_eq!(codes, [codes::STRING_MALFORMED_TEXT_BLOCK]);
        assert_eq!(tokens, [(Str, "\"\"\"text\"\"\""), (Ident, "next")]);
        // The diagnostic points at the opening delimiter.
        let (_, diagnostics) = lex_all(text);
        assert_eq!(diagnostics.as_slice()[0].primary.span, span(0, 3));
        assert_eq!(diagnostics.as_slice()[0].rule, Some("LEX-25"));
        // Two quotes are an empty string.
        assert_eq!(tokens_of("\"\" x"), [(Str, "\"\""), (Ident, "x")]);
    }

    #[test]
    fn a_text_block_closes_alone_on_its_line() {
        // LEX-25. The diagnostic points at the closing delimiter.
        let text = "\"\"\"\n  a\n  b \"\"\" next";
        let (codes, tokens) = rejected(text);
        assert_eq!(codes, [codes::STRING_MALFORMED_TEXT_BLOCK]);
        assert_eq!(tokens, [(Str, &text[..text.len() - 5]), (Ident, "next")]);
        let (_, diagnostics) = lex_all(text);
        assert_eq!(diagnostics.as_slice()[0].primary.span, span(12, 15));
        assert_eq!(diagnostics.as_slice()[0].rule, Some("LEX-25"));
    }

    #[test]
    fn text_block_lines_start_with_the_closing_indentation() {
        // LEX-25. The diagnostic points at the first line that does not;
        // one is enough for a block.
        let text = "\"\"\"\n    a\n  b\n c\n    \"\"\"";
        assert_eq!(
            reports(text),
            [(codes::STRING_MALFORMED_TEXT_BLOCK, "LEX-25", "  b")]
        );
        let (_, tokens) = rejected(text);
        assert_eq!(tokens, [(Str, text)]);
        // Indentation is compared character by character: a tab is not
        // spaces.
        let text = "\"\"\"\n\ta\n    \"\"\"";
        assert_eq!(
            reports(text),
            [(codes::STRING_MALFORMED_TEXT_BLOCK, "LEX-25", "\ta")]
        );
    }

    #[test]
    fn an_unclosed_text_block_runs_to_the_end_of_the_file() {
        // LEX-2.
        let text = "let a = \"\"\"\n  text\n  \"\" still text\nlet b = 1\n";
        let (codes, tokens) = rejected(text);
        assert_eq!(codes, [codes::STRING_UNCLOSED]);
        assert_eq!(
            tokens,
            [(KwLet, "let"), (Ident, "a"), (Eq, "="), (Str, &text[8..])]
        );
        assert_eq!(
            reports(text),
            [(codes::STRING_UNCLOSED, "LEX-2", &text[8..])]
        );
    }

    #[test]
    fn a_backslash_cannot_end_a_text_block_line() {
        // LEX-2: backslash then a line break is not an escape. The grammar
        // gives a text line only characters and escapes.
        for line_break in ["\n", "\r\n", "\r"] {
            let text = format!("\"\"\"{line_break}  a\\{line_break}  b{line_break}  \"\"\"");
            assert_eq!(
                reports(&text),
                [(codes::STRING_UNKNOWN_ESCAPE, "LEX-2", "\\")],
                "{text:?}"
            );
            let (_, tokens) = rejected(&text);
            assert_eq!(tokens, [(Str, text.as_str())], "{text:?}");
        }
    }

    #[test]
    fn integers_and_floats() {
        // The INTEGER_LITERAL and FLOAT_LITERAL productions. LEX-8: an exponent
        // with no unit after it is a float.
        assert_eq!(
            tokens_of("0 42 007 1.5 0.25 1e5 1E5 2e+3 2e-3 1.5e10"),
            [
                (Int, "0"),
                (Int, "42"),
                (Int, "007"),
                (Float, "1.5"),
                (Float, "0.25"),
                (Float, "1e5"),
                (Float, "1E5"),
                (Float, "2e+3"),
                (Float, "2e-3"),
                (Float, "1.5e10"),
            ]
        );
    }

    #[test]
    fn hexadecimal_and_binary_integers() {
        // LEX-23.
        assert_eq!(
            tokens_of("0x1F 0xff 0xDead_Beef 0x0 0b0 0b1111_1111"),
            [
                (Int, "0x1F"),
                (Int, "0xff"),
                (Int, "0xDead_Beef"),
                (Int, "0x0"),
                (Int, "0b0"),
                (Int, "0b1111_1111"),
            ]
        );
        // Neither is a duration candidate, though `d` is a unit: `0x1d` is
        // 29, and `0d` is no days.
        assert_eq!(tokens_of("0x1d 0d"), [(Int, "0x1d"), (Duration, "0d")]);
        // What follows is still a separate token.
        assert_eq!(
            tokens_of("0xFF.x"),
            [(Int, "0xFF"), (Dot, "."), (Ident, "x")]
        );
    }

    #[test]
    fn one_underscore_may_separate_two_digits() {
        // LEX-22: in a whole part, a fraction, an exponent and the digits
        // of a duration.
        assert_eq!(
            tokens_of("1_000 1_000_000 1.5_5 1_0.2_5e1_0 1e1_0 1_500ms 1h3_0m 1.2_5s"),
            [
                (Int, "1_000"),
                (Int, "1_000_000"),
                (Float, "1.5_5"),
                (Float, "1_0.2_5e1_0"),
                (Float, "1e1_0"),
                (Duration, "1_500ms"),
                (Duration, "1h3_0m"),
                (Duration, "1.2_5s"),
            ]
        );
    }

    #[test]
    fn a_dot_needs_a_digit_to_make_a_float() {
        // FLOAT_LITERAL has digits on both sides of its dot; LEX-17 gives `...`.
        assert_eq!(tokens_of("1.max"), [(Int, "1"), (Dot, "."), (Ident, "max")]);
        assert_eq!(
            tokens_of("1...3"),
            [(Int, "1"), (Ellipsis, "..."), (Int, "3")]
        );
        assert_eq!(tokens_of("1.5.2"), [(Float, "1.5"), (Dot, "."), (Int, "2")]);
    }

    #[test]
    fn minus_is_never_part_of_a_number() {
        // LEX-7.
        assert_eq!(tokens_of("-5"), [(Minus, "-"), (Int, "5")]);
        assert_eq!(tokens_of("-1.5"), [(Minus, "-"), (Float, "1.5")]);
        assert_eq!(tokens_of("-250ms"), [(Minus, "-"), (Duration, "250ms")]);
        assert_eq!(tokens_of("a-1"), [(Ident, "a"), (Minus, "-"), (Int, "1")]);
    }

    #[test]
    fn durations_are_single_tokens() {
        // LEX-8, LEX-13.
        assert_eq!(
            tokens_of("1h30m 5s 250ms 10us 1d2h3m4s5ms6us 1.5s 1h0.5m"),
            [
                (Duration, "1h30m"),
                (Duration, "5s"),
                (Duration, "250ms"),
                (Duration, "10us"),
                (Duration, "1d2h3m4s5ms6us"),
                (Duration, "1.5s"),
                (Duration, "1h0.5m"),
            ]
        );
        // The shape is all the lexer checks: unit order is a later check.
        assert_eq!(tokens_of("30m1h"), [(Duration, "30m1h")]);
        // A member access after a duration is not swallowed.
        assert_eq!(
            tokens_of("5s.x"),
            [(Duration, "5s"), (Dot, "."), (Ident, "x")]
        );
    }

    #[test]
    fn a_literal_is_never_split_into_a_literal_and_a_name() {
        // LEX-18: a run of digits, letters and `_` that is not one literal
        // is one malformed token. The kind says what the run most resembles;
        // the rule is the one the run breaks.
        for (text, kind, rule) in [
            // LEX-8: digits then letters are a duration candidate.
            ("12abc", Duration, "LEX-18"),
            ("10x", Duration, "LEX-18"),
            ("5min", Duration, "LEX-18"),
            ("1H", Duration, "LEX-18"),
            ("1.5x", Duration, "LEX-18"),
            ("1.5q", Duration, "LEX-18"),
            // An exponent with no digits.
            ("1e", Float, "LEX-18"),
            ("2E", Float, "LEX-18"),
            ("1.5e", Float, "LEX-18"),
            ("1e_1", Float, "LEX-18"),
            // LEX-8: "`1e5` is valid, `1e5m` is not".
            ("1e5m", Float, "LEX-8"),
            ("1.5e-3s", Float, "LEX-8"),
            // An underscore that is not between two digits.
            ("1_", Int, "LEX-18"),
            ("1_x", Int, "LEX-18"),
            ("1.5_", Float, "LEX-18"),
            ("1e5_", Float, "LEX-18"),
            ("1h_", Duration, "LEX-18"),
            ("3s_total", Duration, "LEX-18"),
            // LEX-22: one underscore, not two.
            ("1__000", Int, "LEX-22"),
            ("1.5__5", Float, "LEX-22"),
            ("0xFF__FF", Int, "LEX-22"),
            // A duration part with no unit, or an unknown one.
            ("1h30", Duration, "LEX-18"),
            ("1h30x", Duration, "LEX-18"),
            // LEX-23: a prefix with no digit, a digit outside the base, an
            // uppercase prefix.
            ("0x", Int, "LEX-18"),
            ("0b", Int, "LEX-18"),
            ("0b102", Int, "LEX-18"),
            ("0b2", Int, "LEX-18"),
            ("0X1F", Int, "LEX-18"),
            ("0B11", Int, "LEX-18"),
            ("0xFFh", Int, "LEX-18"),
            ("0x_FF", Int, "LEX-18"),
            ("0xFF_", Int, "LEX-18"),
        ] {
            let (_, tokens) = rejected(text);
            assert_eq!(tokens, [(kind, text)], "{text}");
            assert_eq!(
                reports(text),
                [(codes::NUMBER_MALFORMED, rule, text)],
                "{text}"
            );
        }
    }

    #[test]
    fn a_malformed_number_ends_where_its_run_ends() {
        // LEX-18: the run stops at the first character that is not a
        // letter, digit or `_`.
        let (codes, tokens) = rejected("1e+");
        assert_eq!(codes, [codes::NUMBER_MALFORMED]);
        assert_eq!(tokens, [(Float, "1e"), (Plus, "+")]);
        let (codes, tokens) = rejected("12abc.x");
        assert_eq!(codes, [codes::NUMBER_MALFORMED]);
        assert_eq!(tokens, [(Duration, "12abc"), (Dot, "."), (Ident, "x")]);
        let (codes, tokens) = rejected("f(10x, 2)");
        assert_eq!(codes, [codes::NUMBER_MALFORMED]);
        assert_eq!(
            tokens,
            [
                (Ident, "f"),
                (LParen, "("),
                (Duration, "10x"),
                (Comma, ","),
                (Int, "2"),
                (RParen, ")")
            ]
        );
    }

    #[test]
    fn an_exponent_cannot_take_a_unit() {
        // LEX-8: "`1e5` is valid, `1e5m` is not".
        assert_eq!(tokens_of("1e5"), [(Float, "1e5")]);
        let (codes, tokens) = rejected("1e5m");
        assert_eq!(codes, [codes::NUMBER_MALFORMED]);
        assert_eq!(tokens, [(Float, "1e5m")]);
    }

    #[test]
    fn temporal_literal_forms() {
        // LEX-11, LEX-12.
        for text in [
            "@2026-09-03",
            "@2026-09-03T09:30",
            "@2026-09-03T09:30:15",
            "@2026-09-03T09:30:15.5",
            "@2026-09-03T09:30:15.123456",
            "@2026-09-03T09:30Z",
            "@2026-09-03T09:30+01",
            "@2026-09-03T09:30-05:30",
            "@2026-09-03T09:30+01:00[Europe/London]",
            "@2026-09-03T09:30Z[Etc/GMT+1]",
            "@09:30",
            "@09:30:15",
            "@09:30:15.250",
            "@09:30[Europe/London]",
            "@[Europe/London]",
            "@[America/Argentina/Buenos_Aires]",
            "@[UTC]",
        ] {
            assert_eq!(tokens_of(text), [(Temporal, text)], "{text}");
        }
    }

    #[test]
    fn temporal_literals_end_where_their_form_ends() {
        // Subtraction of two times, with and without spaces.
        assert_eq!(kinds_of("@09:30 - @09:00"), [Temporal, Minus, Temporal]);
        assert_eq!(kinds_of("@09:30-@09:00"), [Temporal, Minus, Temporal]);
        // An offset is a sign and exactly two digits; otherwise the sign is
        // an operator.
        assert_eq!(
            tokens_of("@2026-09-03T09:30-5"),
            [(Temporal, "@2026-09-03T09:30"), (Minus, "-"), (Int, "5")]
        );
        assert_eq!(
            tokens_of("@2026-09-03T09:30-123"),
            [(Temporal, "@2026-09-03T09:30"), (Minus, "-"), (Int, "123")]
        );
        // A time alone takes no offset.
        assert_eq!(
            tokens_of("@09:30+01"),
            [(Temporal, "@09:30"), (Plus, "+"), (Int, "01")]
        );
        // A zone follows only an explicit offset (LEX-11); here `[` indexes.
        assert_eq!(
            kinds_of("@2026-09-03T09:30[UTC]"),
            [Temporal, LBracket, Ident, RBracket]
        );
        // Closing punctuation is not part of the literal.
        assert_eq!(
            kinds_of("f(@09:30, @2026-09-03)"),
            [Ident, LParen, Temporal, Comma, Temporal, RParen]
        );
    }

    #[test]
    fn malformed_temporal_literals_are_rejected() {
        // LEX-18: `@` then a digit or `[` begins a temporal literal, and
        // the run is rejected when it matches no form.
        for text in [
            "@12",
            "@9:30",
            "@2026-9-3",
            "@2026-09-03t09:30",
            "@2026-09-03T09:30z",
            "@09:30:15.1234567",
            "@20260903",
            "@09:30x",
            "@2026-09-03_",
        ] {
            let (codes, tokens) = rejected(text);
            assert_eq!(codes[0], codes::DATE_TIME_MALFORMED, "{text}");
            assert_eq!(tokens[0].0, Temporal, "{text}");
            assert_eq!(reports(text)[0].1, "LEX-18", "{text}");
        }
    }

    #[test]
    fn malformed_zone_literals_are_one_token() {
        // LEX-18. The token runs to the `]` when there is one on the line.
        for (text, token) in [
            ("@[Europe London] x", "@[Europe London]"),
            ("@[Europe*] x", "@[Europe*]"),
            ("@[", "@["),
            ("@[UTC", "@[UTC"),
            ("@[UTC\n]", "@[UTC"),
            ("@[UTC]x", "@[UTC]x"),
            ("@[UTC]_1 + 2", "@[UTC]_1"),
        ] {
            let (_, tokens) = rejected(text);
            assert_eq!(tokens[0], (Temporal, token), "{text}");
            assert_eq!(
                reports(text),
                [(codes::DATE_TIME_MALFORMED, "LEX-18", token)],
                "{text}"
            );
        }
    }

    #[test]
    fn zone_names_are_checked_with_the_value() {
        // LEX-12's limits on a zone name (non-empty components, no `.` or
        // `..`, at most 255 bytes) are about the name, not the token. These
        // lex; `literal` rejects them.
        for text in [
            "@[]",
            "@[Europe//London]",
            "@[/Europe]",
            "@[Europe/]",
            "@[..]",
            "@09:30[/]",
        ] {
            assert_eq!(tokens_of(text), [(Temporal, text)], "{text}");
        }
    }

    #[test]
    fn at_before_a_name_is_an_annotation_marker() {
        // ERR-10: `@suppress_warning("...")`.
        assert_eq!(
            kinds_of("@suppress_warning(\"node.no_activation\")"),
            [At, Ident, LParen, Str, RParen]
        );
    }

    #[test]
    fn every_symbol_lexes_to_its_own_kind() {
        // LEX-17: each PUNCTUATION alternative is one token.
        // Guards the hand-written matcher against the token table.
        for &(text, kind) in TokenKind::PUNCTUATION {
            assert_eq!(tokens_of(text), [(kind, text)], "{text}");
        }
    }

    #[test]
    fn the_longest_symbol_wins() {
        // LEX-17; `<=>` is LEX-14.
        assert_eq!(kinds_of("a <=> b"), [Ident, Spaceship, Ident]);
        assert_eq!(kinds_of("a<=>b"), [Ident, Spaceship, Ident]);
        assert_eq!(kinds_of("a <= b"), [Ident, LtEq, Ident]);
        assert_eq!(kinds_of("a => b"), [Ident, FatArrow, Ident]);
        assert_eq!(kinds_of("a::b"), [Ident, ColonColon, Ident]);
        assert_eq!(kinds_of("a /= b"), [Ident, SlashEq, Ident]);
        assert_eq!(kinds_of("a //= b"), [Ident, SlashSlashEq, Ident]);
        assert_eq!(kinds_of("a//=b"), [Ident, SlashSlashEq, Ident]);
        assert_eq!(kinds_of("a %= b"), [Ident, PercentEq, Ident]);
        assert_eq!(kinds_of("a ... b"), [Ident, Ellipsis, Ident]);
        // Symbols apart are separate tokens.
        assert_eq!(kinds_of("a // = b"), [Ident, SlashSlash, Eq, Ident]);
        assert_eq!(kinds_of("a .. b"), [Ident, Dot, Dot, Ident]);
    }

    #[test]
    fn nested_generic_closers_are_separate_tokens() {
        // LEX-8, LEX-17: no token joins `>` to a following `>`.
        assert_eq!(
            kinds_of("list<list<i64>>"),
            [KwList, Lt, KwList, Lt, KwI64, Gt, Gt]
        );
        // LEX-17: `>` directly followed by `=` is `>=` even after generic
        // arguments, so the space matters.
        assert_eq!(
            kinds_of("xs: list<i64>= value"),
            [Ident, Colon, KwList, Lt, KwI64, GtEq, Ident]
        );
        assert_eq!(
            kinds_of("xs: list<i64> = value"),
            [Ident, Colon, KwList, Lt, KwI64, Gt, Eq, Ident]
        );
        assert_eq!(
            kinds_of("list<list<i64>>= value"),
            [KwList, Lt, KwList, Lt, KwI64, Gt, GtEq, Ident]
        );
    }

    #[test]
    fn stray_characters_are_reported_one_by_one() {
        // LEX-16.
        let (codes, tokens) = rejected("a & b | c $ \u{1f600}");
        assert_eq!(codes, [codes::SYNTAX_UNEXPECTED_CHARACTER; 4]);
        let unknown: Vec<&str> = tokens
            .iter()
            .filter(|(kind, _)| *kind == Unknown)
            .map(|(_, text)| *text)
            .collect();
        assert_eq!(unknown, ["&", "|", "$", "\u{1f600}"]);
    }

    #[test]
    fn diagnostics_carry_catalogued_codes_and_point_at_the_text() {
        let text = "let s = \"a\\qb\"";
        let (_, diagnostics) = lex_all(text);
        let diagnostic = &diagnostics.as_slice()[0];
        assert_eq!(diagnostic.category, Category::Parse);
        assert_eq!(diagnostic.code, Code::Catalogued("string.unknown_escape"));
        assert_eq!(diagnostic.rule, Some("LEX-2"));
        assert_eq!(&text[diagnostic.primary.span.range()], "\\q");
    }
}
