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
//! * **Lossless.** The tokens cover the source exactly, with no gaps and no
//!   overlaps. Joining their text reproduces the file. The formatter depends
//!   on this to preserve comments.
//! * **Total.** Lexing never fails or stops early. Problems are reported to
//!   the diagnostics sink and scanning continues.

use hgl_diagnostics::{Category, Code, Diagnostic, Diagnostics, Location};
use hgl_source::{FileId, SourceMap, Span, line_break_length};

use crate::token::{Token, TokenKind};

/// The lexer's compiler-local diagnostic codes.
///
/// The specification rejects each of these conditions (the rule is cited on
/// the diagnostic) but its catalog assigns them no code, so they are reported
/// as [`Code::Local`].
pub mod codes {
    /// A character that cannot start a token (LEX-1).
    pub const UNEXPECTED_CHARACTER: &str = "lex.unexpected_character";
    /// A string with no closing quote before the line break (LEX-2).
    pub const UNCLOSED_STRING: &str = "lex.unclosed_string";
    /// A backslash followed by anything but `"`, `\`, `n`, `r`, `t` (LEX-2).
    pub const UNKNOWN_ESCAPE: &str = "lex.unknown_escape";
    /// A block comment with no closing `*/` (LEX-2).
    pub const UNCLOSED_COMMENT: &str = "lex.unclosed_comment";
    /// A number run together with letters that are not a duration (LEX-8).
    pub const MALFORMED_NUMBER: &str = "lex.malformed_number";
    /// An `@` literal that matches none of the temporal forms (LEX-11,
    /// LEX-12).
    pub const MALFORMED_TEMPORAL: &str = "lex.malformed_temporal";
}

/// The duration units, largest first (LEX-13). The position of a unit in
/// this list is its rank: a smaller index is a larger unit.
pub(crate) const DURATION_UNITS: [&str; 6] = ["d", "h", "m", "s", "ms", "us"];

/// Splits one source file into tokens.
///
/// Returns every token, including whitespace, line breaks and comments, in
/// source order. Lexical errors are appended to `diagnostics`; the returned
/// tokens are complete either way.
///
/// The lexer checks the *shape* of literals only. Whether `@2026-02-30` is a
/// real date or `30m1h` orders its units correctly is decided by
/// [`crate::literal`], which also produces the literal's value.
pub fn lex(sources: &SourceMap, file: FileId, diagnostics: &mut Diagnostics) -> Vec<Token> {
    let text = sources.file(file).text();
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
        let kind = lexer.next_kind();
        debug_assert!(lexer.pos > start, "every token consumes at least one byte");
        tokens.push(Token {
            kind,
            span: span(start, lexer.pos),
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

/// True for a byte that can continue an identifier (LEX-1).
fn is_ident_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// True for a byte admitted in a time-zone name component (the
/// `zone_character` production).
fn is_zone_character(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'+' | b'.')
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

    /// Consumes a run of ASCII digits (possibly empty).
    fn eat_digits(&mut self) {
        self.eat_while(|byte| byte.is_ascii_digit());
    }

    /// True when the `count` bytes starting `ahead` positions on are all
    /// ASCII digits.
    fn digits_at(&self, ahead: usize, count: usize) -> bool {
        (0..count).all(|i| {
            self.peek_at(ahead + i)
                .is_some_and(|byte| byte.is_ascii_digit())
        })
    }

    /// Starts an error diagnostic for the bytes `start..end`. Every lexical
    /// error is in the `parse` category and carries a local code.
    fn error(
        &self,
        start: usize,
        end: usize,
        code: &'static str,
        message: impl Into<String>,
    ) -> Diagnostic {
        Diagnostic::error(
            Category::Parse,
            Code::Local(code),
            Location::new(self.file, span(start, end)),
            message,
        )
    }

    // ---- Dispatch -------------------------------------------------------

    /// Consumes one token starting at `self.pos` and returns its kind.
    fn next_kind(&mut self) -> TokenKind {
        match self.bytes[self.pos] {
            b' ' | b'\t' => {
                self.eat_while(|byte| byte == b' ' || byte == b'\t');
                TokenKind::Whitespace
            }
            // A line break is LF, CRLF or a lone CR. Each is one token and
            // none is preferred; `line_break_length` says how long this one
            // is.
            b'\n' | b'\r' => {
                self.pos += line_break_length(&self.bytes[self.pos..])
                    .expect("a line feed or carriage return starts a line break");
                TokenKind::Newline
            }
            b'#' => self.line_comment(),
            // `if` after a pattern is a guard: the arm applies only when the
            // condition also holds.
            b'/' if self.peek_at(1) == Some(b'*') => self.block_comment(),
            b'"' => self.string(),
            b'0'..=b'9' => self.number(),
            b'@' => self.temporal_or_at(),
            byte if is_ident_start(byte) => self.word(),
            _ => self.punctuation(),
        }
    }

    // ---- Comments (LEX-3) -----------------------------------------------

    /// `# ...` to the end of the line. The line break (LF, CRLF or CR) is
    /// left for the next token: it stays meaningful (LEX-3).
    fn line_comment(&mut self) -> TokenKind {
        self.eat_while(|byte| byte != b'\n' && byte != b'\r');
        TokenKind::LineComment
    }

    /// `/* ... */`. Comments do not nest, so the first `*/` closes it.
    /// A comment that opens with `/**` is a documentation comment (SDOC-1);
    /// `/**/` is an ordinary empty comment.
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
                        start + 2,
                        codes::UNCLOSED_COMMENT,
                        "block comment has no closing `*/`",
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

    // ---- Strings (LEX-2) ------------------------------------------------

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
                            codes::UNCLOSED_STRING,
                            "string has no closing quote",
                        )
                        .with_rule("LEX-2")
                        .with_note("a string must close on the line it opens on");
                    self.diagnostics.emit(diagnostic);
                    break;
                }
                Some(b'"') => {
                    self.pos += 1;
                    break;
                }
                Some(b'\\') => self.escape(),
                // Any other byte, including the bytes of a multi-byte
                // character, is string content.
                Some(_) => self.pos += 1,
            }
        }
        TokenKind::Str
    }

    /// A backslash escape inside a string, starting at the backslash.
    fn escape(&mut self) {
        let start = self.pos;
        match self.peek_at(1) {
            Some(b'"' | b'\\' | b'n' | b'r' | b't') => self.pos += 2,
            // A backslash at the end of the line: consume it and let
            // `string` report the unclosed string.
            None | Some(b'\n' | b'\r') => self.pos += 1,
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
                        codes::UNKNOWN_ESCAPE,
                        format!("unknown escape `\\{escaped}`"),
                    )
                    .with_rule("LEX-2")
                    .with_note(r#"the escapes are \" \\ \n \r \t"#);
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

    // ---- Numbers and durations (LEX-7, LEX-8, LEX-13) -------------------

    /// An integer, float or duration literal, starting at a digit.
    ///
    /// ```text
    /// 12        Int
    /// 12.5      Float      a `.` counts only when a digit follows it
    /// 1e5       Float      a complete exponent makes a float
    /// 1h30m     Duration   a digit run followed by letters is a duration
    /// 10x       Duration   ...even when the unit is unknown: reported
    /// 1e5m      Float      an exponent cannot take a unit: reported
    /// ```
    ///
    /// There is no sign here: `-5` is the minus operator and then `5`
    /// (LEX-7).
    fn number(&mut self) -> TokenKind {
        let start = self.pos;
        self.eat_digits();
        let has_fraction = self.eat_fraction();

        if let Some(length) = self.exponent_length() {
            self.pos += length;
            if self.peek().is_some_and(is_ident_start) {
                self.eat_while(is_ident_continue);
                let literal = &self.text[start..self.pos];
                let diagnostic = self
                    .error(
                        start,
                        self.pos,
                        codes::MALFORMED_NUMBER,
                        format!("`{literal}` is not a number: letters follow the exponent"),
                    )
                    .with_rule("LEX-8")
                    .with_note("a float with an exponent cannot take a duration unit");
                self.diagnostics.emit(diagnostic);
            }
            return TokenKind::Float;
        }

        if self.peek().is_some_and(is_ident_start) {
            return self.duration_parts(start);
        }
        if has_fraction {
            TokenKind::Float
        } else {
            TokenKind::Int
        }
    }

    /// Consumes `.digits` if present and reports whether it did. A `.` not
    /// followed by a digit is left alone: `1.max` is `1`, `.`, `max`.
    fn eat_fraction(&mut self) -> bool {
        let present = self.peek() == Some(b'.') && self.digits_at(1, 1);
        if present {
            self.pos += 1;
            self.eat_digits();
        }
        present
    }

    /// The length of a complete exponent (`e`, optional sign, digits) at the
    /// cursor, or `None`. A bare `e` is not an exponent, so `2e` falls
    /// through to the duration path and is reported as an unknown unit.
    fn exponent_length(&self) -> Option<usize> {
        if !matches!(self.peek(), Some(b'e' | b'E')) {
            return None;
        }
        let mut length = 1;
        if matches!(self.peek_at(length), Some(b'+' | b'-')) {
            length += 1;
        }
        let digits_start = length;
        while self.digits_at(length, 1) {
            length += 1;
        }
        // `then_some` turns a bool into an Option: Some(length) when true.
        (length > digits_start).then_some(length)
    }

    /// The rest of a duration literal. Called with the first part's number
    /// consumed and the cursor on its unit letters; `start` is where the
    /// literal began.
    ///
    /// The shape is `number unit` repeated: `1h30m`, `1.5s`. Unit order and
    /// the placing of fractions are checked later by
    /// [`crate::literal::decode_duration`].
    fn duration_parts(&mut self, start: usize) -> TokenKind {
        loop {
            // A unit is a maximal run of letters, so `ms` is one unit and
            // `min` is one (unknown) unit, never `m` then `in`.
            let unit_start = self.pos;
            self.eat_while(is_ident_start);
            let unit = &self.text[unit_start..self.pos];
            if !DURATION_UNITS.contains(&unit) {
                // LEX-8: reject the whole run instead of splitting it into a
                // number and an identifier.
                self.eat_while(is_ident_continue);
                let literal = &self.text[start..self.pos];
                let diagnostic = self
                    .error(
                        start,
                        self.pos,
                        codes::MALFORMED_NUMBER,
                        format!("`{literal}` is not a number or a duration: `{unit}` is not a duration unit"),
                    )
                    .with_rule("LEX-8")
                    .with_note("digits followed by letters are read as a duration; the units are d, h, m, s, ms, us");
                self.diagnostics.emit(diagnostic);
                return TokenKind::Duration;
            }

            // Another part follows only if a number does.
            if !self.digits_at(0, 1) {
                return TokenKind::Duration;
            }
            self.eat_digits();
            self.eat_fraction();
            if !self.peek().is_some_and(is_ident_start) {
                let literal = &self.text[start..self.pos];
                let diagnostic = self
                    .error(
                        start,
                        self.pos,
                        codes::MALFORMED_NUMBER,
                        format!("`{literal}` is not a duration: its last number has no unit"),
                    )
                    .with_rule("LEX-8")
                    .with_note(
                        "each part of a duration is a number followed by one of d, h, m, s, ms, us",
                    );
                self.diagnostics.emit(diagnostic);
                return TokenKind::Duration;
            }
        }
    }

    // ---- Temporal literals (LEX-11, LEX-12) -----------------------------

    /// A token starting with `@`: a temporal literal when a digit or `[`
    /// follows, otherwise the `@` symbol that introduces an annotation such
    /// as `@suppress_warning(...)`.
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
            return;
        }
        // Malformed. Take the text up to a `]` on this line, if there is
        // one, so the whole attempt is one token; otherwise just the `[`.
        let rest = &self.bytes[self.pos..];
        let line_length = rest
            .iter()
            .position(|&byte| byte == b'\n' || byte == b'\r')
            .unwrap_or(rest.len());
        let length = rest[..line_length]
            .iter()
            .position(|&byte| byte == b']')
            .map_or(1, |index| index + 1);
        self.pos += length;
        let diagnostic = self
            .error(start, self.pos, codes::MALFORMED_TEMPORAL, "malformed time-zone literal")
            .with_rule("LEX-12")
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

        // A literal that runs straight into a letter or digit is malformed:
        // `@2026-09-03t09:30` (lowercase t), `@09:30:15.1234567` (a seventh
        // fraction digit), `@12` (neither a date nor a time).
        if !well_formed || self.peek().is_some_and(is_ident_continue) {
            self.eat_while(|byte| is_ident_continue(byte) || byte == b':');
            let literal = &self.text[start..self.pos];
            let diagnostic = self
                .error(start, self.pos, codes::MALFORMED_TEMPORAL, format!("malformed temporal literal `{literal}`"))
                .with_rule("LEX-11")
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
    /// digits is left as an operator.
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

    /// The length of a `[Component/Component]` zone annotation at the
    /// cursor, or `None` when the text there is not a complete annotation.
    /// Components are non-empty, so leading, trailing and doubled slashes
    /// do not match (LEX-12).
    fn zone_annotation_length(&self) -> Option<usize> {
        if self.peek() != Some(b'[') {
            return None;
        }
        let mut length = 1;
        loop {
            let component_start = length;
            while self.peek_at(length).is_some_and(is_zone_character) {
                length += 1;
            }
            if length == component_start {
                return None;
            }
            match self.peek_at(length) {
                Some(b'/') => length += 1,
                Some(b']') => return Some(length + 1),
                _ => return None,
            }
        }
    }

    // ---- Punctuation (LEX-14) -------------------------------------------

    /// A symbol, taking the longest that matches: `<=>` before `<=` before
    /// `<` (LEX-14). Anything else is an unexpected character.
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
            (b':', Some(b':'), _) => (ColonColon, 2),
            (b'/', Some(b'/'), _) => (SlashSlash, 2),
            (b'/', Some(b'='), _) => (SlashEq, 2),
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

    /// A character that starts no token. The whole character is consumed,
    /// which may be several bytes, so the next token starts on a character
    /// boundary.
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
            .error(start, self.pos, codes::UNEXPECTED_CHARACTER, message)
            .with_rule("LEX-1");
        self.diagnostics.emit(diagnostic);
        TokenKind::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use TokenKind::*;

    /// Lexes `text` and returns the tokens with the diagnostics' codes.
    fn lex_text(text: &str) -> (Vec<Token>, Vec<&'static str>) {
        let mut sources = SourceMap::new();
        let file = sources.add("test.hgl", text).unwrap();
        let mut diagnostics = Diagnostics::new();
        let tokens = lex(&sources, file, &mut diagnostics);
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

    #[test]
    fn tokens_cover_the_source_exactly() {
        // Valid and invalid input alike: no gaps, no overlaps.
        for text in [
            "",
            "fn f(a: i64) -> i64 { when { return a } }\n",
            "let s = \"caf\u{e9}\" # tr\u{e8}s bien\r\n",
            "\"unclosed\n/* unclosed",
            "\u{a3} \u{1f600} 10x @12 \"\\q\"",
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
            "const", "atomic", "tuple", "list", "set", "dict", "rolling", "result", "error",
        ] {
            assert_eq!(kinds_of(name), [Ident], "{name}");
        }
        for name in [
            "ref", "signal", "delta", "native", "type", "switch", "case", "default", "in", "each",
        ] {
            assert_eq!(kinds_of(name), [Ident], "{name}");
        }
    }

    #[test]
    fn non_ascii_letters_do_not_form_identifiers() {
        // LEX-1 admits ASCII letters only.
        let (codes, tokens) = rejected("caf\u{e9}");
        assert_eq!(codes, [codes::UNEXPECTED_CHARACTER]);
        assert_eq!(tokens, [(Ident, "caf"), (Unknown, "\u{e9}")]);
    }

    #[test]
    fn every_line_break_form_is_one_newline_token() {
        // LF (Linux), CRLF (Windows) and CR (classic Mac), in one file.
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
    fn line_comments_stop_before_the_line_ending() {
        // LEX-3.
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
    }

    #[test]
    fn documentation_comments_are_recognised() {
        // SDOC-1.
        assert_eq!(kinds_of("/** Doc. */"), [DocComment]);
        assert_eq!(kinds_of("/**/"), [BlockComment]);
        assert_eq!(kinds_of("/* plain */"), [BlockComment]);
    }

    #[test]
    fn unclosed_block_comment_is_rejected() {
        // LEX-2.
        let (codes, tokens) = rejected("a /* never closed\nb");
        assert_eq!(codes, [codes::UNCLOSED_COMMENT]);
        assert_eq!(tokens, [(Ident, "a"), (BlockComment, "/* never closed\nb")]);
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
        assert_eq!(codes, [codes::UNKNOWN_ESCAPE]);
        assert_eq!(tokens, [(Str, r#""a\qb""#), (Ident, "c")]);
        // A multi-byte character after the backslash is taken whole.
        let (codes, tokens) = rejected("\"\\\u{e9}\"");
        assert_eq!(codes, [codes::UNKNOWN_ESCAPE]);
        assert_eq!(tokens, [(Str, "\"\\\u{e9}\"")]);
    }

    #[test]
    fn unclosed_string_ends_at_the_line_ending() {
        // LEX-2.
        let (codes, tokens) = rejected("\"abc\nnext");
        assert_eq!(codes, [codes::UNCLOSED_STRING]);
        assert_eq!(tokens, [(Str, "\"abc"), (Newline, "\n"), (Ident, "next")]);
        for line_break in ["\r\n", "\r"] {
            let text = format!("\"abc{line_break}next");
            let (codes, tokens) = rejected(&text);
            assert_eq!(codes, [codes::UNCLOSED_STRING]);
            assert_eq!(
                tokens,
                [(Str, "\"abc"), (Newline, line_break), (Ident, "next")]
            );
        }
        let (codes, _) = rejected("\"abc\\");
        assert_eq!(codes, [codes::UNCLOSED_STRING]);
        // An escaped quote does not close the string.
        let (codes, _) = rejected(r#""abc\""#);
        assert_eq!(codes, [codes::UNCLOSED_STRING]);
    }

    #[test]
    fn integers_and_floats() {
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
    fn a_dot_needs_a_digit_to_make_a_float() {
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
    fn digits_followed_by_unknown_letters_are_rejected_whole() {
        // LEX-8: "an unknown unit is rejected rather than split into a
        // number and identifier".
        for text in [
            "10x", "5min", "1H", "0x1F", "1_000", "2e", "3s_total", "1.5q",
        ] {
            let (codes, tokens) = rejected(text);
            assert_eq!(codes, [codes::MALFORMED_NUMBER], "{text}");
            assert_eq!(tokens, [(Duration, text)], "{text}");
        }
    }

    #[test]
    fn an_exponent_cannot_take_a_unit() {
        // LEX-8: "`1e5` is valid, `1e5m` is not".
        assert_eq!(tokens_of("1e5"), [(Float, "1e5")]);
        let (codes, tokens) = rejected("1e5m");
        assert_eq!(codes, [codes::MALFORMED_NUMBER]);
        assert_eq!(tokens, [(Float, "1e5m")]);
    }

    #[test]
    fn a_duration_cannot_end_in_a_bare_number() {
        let (codes, tokens) = rejected("1h30");
        assert_eq!(codes, [codes::MALFORMED_NUMBER]);
        assert_eq!(tokens, [(Duration, "1h30")]);
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
        for text in [
            "@12",
            "@9:30",
            "@2026-9-3",
            "@2026-09-03t09:30",
            "@2026-09-03T09:30z",
            "@09:30:15.1234567",
            "@20260903",
        ] {
            let (codes, _) = rejected(text);
            assert_eq!(codes[0], codes::MALFORMED_TEMPORAL, "{text}");
        }
        for text in [
            "@[]",
            "@[Europe//London]",
            "@[/Europe]",
            "@[Europe/]",
            "@[Europe London]",
            "@[",
        ] {
            let (codes, tokens) = rejected(text);
            assert_eq!(codes, [codes::MALFORMED_TEMPORAL], "{text}");
            assert_eq!(tokens[0].0, Temporal, "{text}");
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
        // Guards the hand-written matcher against the token table.
        for &(text, kind) in TokenKind::PUNCTUATION {
            assert_eq!(tokens_of(text), [(kind, text)], "{text}");
        }
    }

    #[test]
    fn the_longest_symbol_wins() {
        // LEX-14.
        assert_eq!(kinds_of("a <=> b"), [Ident, Spaceship, Ident]);
        assert_eq!(kinds_of("a<=>b"), [Ident, Spaceship, Ident]);
        assert_eq!(kinds_of("a <= b"), [Ident, LtEq, Ident]);
        assert_eq!(kinds_of("a => b"), [Ident, FatArrow, Ident]);
        assert_eq!(kinds_of("a::b"), [Ident, ColonColon, Ident]);
        assert_eq!(kinds_of("a /= b"), [Ident, SlashEq, Ident]);
    }

    #[test]
    fn nested_generic_closers_are_separate_tokens() {
        // LEX-8.
        assert_eq!(
            kinds_of("list<list<i64>>"),
            [Ident, Lt, Ident, Lt, KwI64, Gt, Gt]
        );
    }

    #[test]
    fn stray_characters_are_reported_one_by_one() {
        let (codes, tokens) = rejected("a & b | c $ \u{1f600}");
        assert_eq!(codes, [codes::UNEXPECTED_CHARACTER; 4]);
        let unknown: Vec<&str> = tokens
            .iter()
            .filter(|(kind, _)| *kind == Unknown)
            .map(|(_, text)| *text)
            .collect();
        assert_eq!(unknown, ["&", "|", "$", "\u{1f600}"]);
    }

    #[test]
    fn diagnostics_point_at_the_offending_text() {
        let text = "let s = \"a\\qb\"";
        let mut sources = SourceMap::new();
        let file = sources.add("test.hgl", text).unwrap();
        let mut diagnostics = Diagnostics::new();
        lex(&sources, file, &mut diagnostics);
        let diagnostic = &diagnostics.as_slice()[0];
        assert_eq!(diagnostic.category, Category::Parse);
        assert_eq!(diagnostic.rule, Some("LEX-2"));
        assert!(!diagnostic.code.is_catalogued());
        assert_eq!(&text[diagnostic.primary.span.range()], "\\q");
    }
}
