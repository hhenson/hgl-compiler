//! Source text and positions, shared by every compiler stage.
//!
//! A [`SourceMap`] owns the loaded files. A [`FileId`] names one of them and a
//! [`Span`] is a byte range inside it. Tokens, syntax nodes and diagnostics
//! all store spans rather than copies of text.
//!
//! Positions are byte offsets, not line/column pairs. An offset is four bytes
//! and can slice the text directly; [`SourceFile::line_col`] converts one to a
//! line and column only when a message is shown to a person.
//!
//! This crate also applies the two rules about a file as a whole (LEX-16):
//! one byte order mark at the start is ignored, and bytes that are not
//! UTF-8 are recorded for the lexer to report ([`SourceFile::from_bytes`]).
//!
//! # For readers coming from C++
//!
//! * `&str` is a borrowed UTF-8 string view (`std::string_view`); `String`
//!   owns its bytes (`std::string`).
//! * `#[derive(...)]` asks the compiler to generate the listed operations
//!   (copying, `==`, ordering, hashing, debug printing) member-wise.
//! * `Option<T>` is `std::optional<T>`; `Result<T, E>` is `std::expected<T, E>`.

use std::fmt;
use std::ops::Range;

/// The length in bytes of the line break at the start of `bytes`, or `None`
/// when `bytes` does not start with one.
///
/// A line break is a line feed (LF, `\n`), a carriage return followed by a
/// line feed (CRLF, `\r\n`), or a carriage return on its own (CR, `\r`):
/// the Linux, Windows and classic Mac conventions. The three mean the same
/// everywhere, a file may mix them, and CRLF is one line break, not two.
///
/// This is the only place a line break is defined. The line index below and
/// the lexer both call it, so they cannot disagree about where lines end.
pub fn line_break_length(bytes: &[u8]) -> Option<usize> {
    // A slice pattern: `[a, b, ..]` matches a slice that starts with `a`
    // then `b`, whatever follows. Arms are tried in order, so CRLF is
    // recognised before a carriage return on its own.
    match bytes {
        [b'\r', b'\n', ..] => Some(2),
        [b'\n' | b'\r', ..] => Some(1),
        _ => None,
    }
}

/// A half-open byte range `start..end` within one source file.
///
/// Offsets are `u32`, so a span is eight bytes and a file is limited to 4 GiB
/// ([`SourceFile::new`] enforces the limit).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Span {
    /// Offset of the first byte.
    pub start: u32,
    /// Offset one past the last byte.
    pub end: u32,
}

impl Span {
    /// Creates the span `start..end`. `start` must not exceed `end`.
    pub fn new(start: u32, end: u32) -> Span {
        debug_assert!(start <= end, "span start {start} is after end {end}");
        Span { start, end }
    }

    /// Creates the empty span at `offset`, used to point between two bytes
    /// (for example at the end of the file).
    pub fn empty(offset: u32) -> Span {
        Span {
            start: offset,
            end: offset,
        }
    }

    /// Number of bytes covered.
    pub fn len(self) -> u32 {
        self.end - self.start
    }

    /// True when the span covers no bytes.
    pub fn is_empty(self) -> bool {
        self.start == self.end
    }

    /// The span as a `usize` range, the form string slicing takes:
    /// `&text[span.range()]`.
    pub fn range(self) -> Range<usize> {
        self.start as usize..self.end as usize
    }
}

/// A 1-based line and column, for showing a position to a person.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineCol {
    /// Line number; the first line is 1.
    pub line: u32,
    /// Column number; the first character of a line is 1. Counted in Unicode
    /// scalar values, so a multi-byte character advances the column by one.
    pub column: u32,
}

impl fmt::Display for LineCol {
    /// Formats as `line:column`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.column)
    }
}

/// The reason a source file was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceError {
    /// The text is longer than `u32::MAX` bytes, which a [`Span`] cannot
    /// address.
    TooLarge {
        /// Name of the refused file.
        name: String,
    },
}

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SourceError::TooLarge { name } => {
                write!(f, "{name}: source files are limited to 4 GiB")
            }
        }
    }
}

impl std::error::Error for SourceError {}

/// The byte order mark, U+FEFF. Some editors write one at the start of a
/// UTF-8 file; LEX-16 ignores one there.
const BYTE_ORDER_MARK: char = '\u{feff}';

/// Where a file's bytes stop being well-formed UTF-8 (LEX-16).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncodingError {
    /// Offset of the first offending byte. It is the end of
    /// [`SourceFile::text`], which holds only the bytes before it.
    pub offset: u32,
    /// The offending byte.
    pub byte: u8,
}

/// One source file: its name, its text, and an index of where lines start.
#[derive(Debug, Clone)]
pub struct SourceFile {
    name: String,
    text: String,
    /// Byte offset of the first character of each line. Always starts with 0,
    /// so the vector's length is the number of lines.
    line_starts: Vec<u32>,
    has_byte_order_mark: bool,
    encoding_error: Option<EncodingError>,
}

impl SourceFile {
    /// Wraps `text` under the display name `name` (normally its path).
    ///
    /// A line ends at a line break as [`line_break_length`] defines it, so
    /// LF, CRLF and CR files, and files that mix them, index alike.
    ///
    /// One byte order mark at the start is removed (LEX-16), so offsets and
    /// columns count from the first character after it.
    /// [`SourceFile::has_byte_order_mark`] records that it was there.
    pub fn new(
        name: impl Into<String>,
        text: impl Into<String>,
    ) -> Result<SourceFile, SourceError> {
        let name = name.into();
        let mut text = text.into();
        let has_byte_order_mark = text.starts_with(BYTE_ORDER_MARK);
        if has_byte_order_mark {
            // `drain` removes a byte range from the string in place.
            text.drain(..BYTE_ORDER_MARK.len_utf8());
        }
        if u32::try_from(text.len()).is_err() {
            return Err(SourceError::TooLarge { name });
        }
        let mut line_starts = vec![0];
        let bytes = text.as_bytes();
        let mut offset = 0;
        while offset < bytes.len() {
            match line_break_length(&bytes[offset..]) {
                Some(length) => {
                    // The next line starts after the whole line break. The
                    // cast cannot truncate: the length check above passed.
                    offset += length;
                    line_starts.push(offset as u32);
                }
                None => offset += 1,
            }
        }
        Ok(SourceFile {
            name,
            text,
            line_starts,
            has_byte_order_mark,
            encoding_error: None,
        })
    }

    /// Wraps the bytes of a file as read from disk.
    ///
    /// Source must be well-formed UTF-8 (LEX-16). When `bytes` is not, the
    /// file is still created so that the rejection can be reported like any
    /// other, with a position: its text is the well-formed bytes before the
    /// first offending one, and [`SourceFile::encoding_error`] says where
    /// that was.
    pub fn from_bytes(name: impl Into<String>, bytes: Vec<u8>) -> Result<SourceFile, SourceError> {
        // `String::from_utf8` takes ownership of the bytes and hands them
        // back inside the error when they are not UTF-8, so nothing is
        // copied on either path.
        let error = match String::from_utf8(bytes) {
            Ok(text) => return SourceFile::new(name, text),
            Err(error) => error,
        };
        let valid = error.utf8_error().valid_up_to();
        let mut bytes = error.into_bytes();
        let byte = bytes[valid];
        bytes.truncate(valid);
        let text = String::from_utf8(bytes).expect("the bytes before the error are UTF-8");
        let mut file = SourceFile::new(name, text)?;
        file.encoding_error = Some(EncodingError {
            // Not `valid`: a byte order mark may have been removed.
            offset: file.text.len() as u32,
            byte,
        });
        Ok(file)
    }

    /// The display name given at construction.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The source text: everything after an ignored byte order mark and
    /// before an encoding error.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// True when the file began with a byte order mark, which
    /// [`SourceFile::text`] leaves out.
    pub fn has_byte_order_mark(&self) -> bool {
        self.has_byte_order_mark
    }

    /// Where the file's bytes stopped being UTF-8, or `None` for a
    /// well-formed file. Such a file is rejected as a whole; the lexer
    /// reports it.
    pub fn encoding_error(&self) -> Option<EncodingError> {
        self.encoding_error
    }

    /// The text a span covers. The span must lie on character boundaries of
    /// this file, which holds for every span a compiler stage produces.
    pub fn slice(&self, span: Span) -> &str {
        &self.text[span.range()]
    }

    /// Number of lines. An empty file has one (empty) line.
    pub fn line_count(&self) -> u32 {
        self.line_starts.len() as u32
    }

    /// The zero-based index of the line that contains the byte at `offset`.
    /// A line break belongs to the line it ends. An offset past the end is
    /// treated as the end.
    pub fn line_index(&self, offset: u32) -> usize {
        let offset = (offset as usize).min(self.text.len());
        // `partition_point` is a binary search: it returns how many line
        // starts are at or before `offset`. That count is the 1-based line
        // number, so one less is the index.
        self.line_starts
            .partition_point(|&start| start as usize <= offset)
            - 1
    }

    /// The byte range of the line with zero-based index `line`, including
    /// the line break that ends it, or `None` when there is no such line.
    pub fn line_range(&self, line: usize) -> Option<Range<usize>> {
        let start = *self.line_starts.get(line)? as usize;
        // The line runs to the start of the next one, or to the end of the
        // text for the last line.
        let end = self
            .line_starts
            .get(line + 1)
            .map_or(self.text.len(), |&next| next as usize);
        Some(start..end)
    }

    /// Converts a byte offset to a 1-based line and column.
    ///
    /// An offset past the end is treated as the end, and an offset inside a
    /// multi-byte character as the start of that character.
    pub fn line_col(&self, offset: u32) -> LineCol {
        let mut offset = (offset as usize).min(self.text.len());
        while !self.text.is_char_boundary(offset) {
            offset -= 1;
        }
        let line = self.line_index(offset as u32);
        let line_start = self.line_starts[line] as usize;
        let column = self.text[line_start..offset].chars().count() + 1;
        LineCol {
            line: line as u32 + 1,
            column: column as u32,
        }
    }
}

/// Identifies one file in a [`SourceMap`].
///
/// A small copyable handle (an index), so that a diagnostic or syntax node
/// can name its file without borrowing the file itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FileId(u32);

impl FileId {
    /// The position of the file in its [`SourceMap`], counting from zero in
    /// the order files were added.
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// The set of source files in one compilation.
///
/// A module may be assembled from several part files; all of them live here
/// and are referred to by [`FileId`].
#[derive(Debug, Clone, Default)]
pub struct SourceMap {
    files: Vec<SourceFile>,
}

impl SourceMap {
    /// Creates an empty map.
    pub fn new() -> SourceMap {
        SourceMap::default()
    }

    /// Adds a file from its text and returns its id.
    pub fn add(
        &mut self,
        name: impl Into<String>,
        text: impl Into<String>,
    ) -> Result<FileId, SourceError> {
        Ok(self.push(SourceFile::new(name, text)?))
    }

    /// Adds a file from its bytes as read from disk and returns its id. See
    /// [`SourceFile::from_bytes`] for bytes that are not UTF-8.
    pub fn add_bytes(
        &mut self,
        name: impl Into<String>,
        bytes: Vec<u8>,
    ) -> Result<FileId, SourceError> {
        Ok(self.push(SourceFile::from_bytes(name, bytes)?))
    }

    fn push(&mut self, file: SourceFile) -> FileId {
        let id = FileId(self.files.len() as u32);
        self.files.push(file);
        id
    }

    /// The file an id names. Ids come only from this map's `add` functions,
    /// so the lookup cannot fail for a correctly used id.
    pub fn file(&self, id: FileId) -> &SourceFile {
        &self.files[id.index()]
    }

    /// All files, in the order they were added (so in [`FileId`] order).
    pub fn files(&self) -> &[SourceFile] {
        &self.files
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_reports_its_length_and_range() {
        let span = Span::new(2, 5);
        assert_eq!(span.len(), 3);
        assert!(!span.is_empty());
        assert_eq!(span.range(), 2..5);
        assert!(Span::empty(7).is_empty());
    }

    #[test]
    fn line_and_column_are_one_based() {
        let file = SourceFile::new("t.hgl", "ab\ncd\n").unwrap();
        assert_eq!(file.line_count(), 3);
        assert_eq!(file.line_col(0), LineCol { line: 1, column: 1 });
        assert_eq!(file.line_col(1), LineCol { line: 1, column: 2 });
        // The line feed itself is the last column of its line.
        assert_eq!(file.line_col(2), LineCol { line: 1, column: 3 });
        assert_eq!(file.line_col(3), LineCol { line: 2, column: 1 });
        // End of file, after the final line feed.
        assert_eq!(file.line_col(6), LineCol { line: 3, column: 1 });
    }

    #[test]
    fn a_line_break_is_lf_crlf_or_cr() {
        assert_eq!(line_break_length(b"\nx"), Some(1));
        assert_eq!(line_break_length(b"\r\nx"), Some(2));
        assert_eq!(line_break_length(b"\rx"), Some(1));
        assert_eq!(line_break_length(b"\r"), Some(1));
        // Two line breaks, not one: LF then CR is not a pair.
        assert_eq!(line_break_length(b"\n\r"), Some(1));
        assert_eq!(line_break_length(b"x\n"), None);
        assert_eq!(line_break_length(b""), None);
    }

    #[test]
    fn every_line_break_form_indexes_alike() {
        // The same three lines under each convention. `cd` is line 2 and
        // `ef` line 3 whichever line break separates them.
        for line_break in ["\n", "\r\n", "\r"] {
            let text = ["ab", "cd", "ef"].join(line_break);
            let file = SourceFile::new("t.hgl", text.as_str()).unwrap();
            assert_eq!(file.line_count(), 3, "{text:?}");
            let c = text.find('c').unwrap() as u32;
            let f = text.find('f').unwrap() as u32;
            assert_eq!(file.line_col(c), LineCol { line: 2, column: 1 });
            assert_eq!(file.line_col(f), LineCol { line: 3, column: 2 });
            // The line break itself sits at the end of the line it ends.
            assert_eq!(file.line_col(2), LineCol { line: 1, column: 3 });
        }
    }

    #[test]
    fn mixed_line_breaks_each_count_once() {
        // LF, CRLF, CR, then CR followed by CRLF (an empty line between).
        let file = SourceFile::new("t.hgl", "a\nb\r\nc\rd\r\r\ne").unwrap();
        assert_eq!(file.line_count(), 6);
        assert_eq!(file.line_col(11), LineCol { line: 6, column: 1 });
        // Inside CRLF, the line feed is still on the line the pair ends.
        assert_eq!(file.line_col(4), LineCol { line: 2, column: 3 });
    }

    #[test]
    fn line_ranges_include_the_line_break() {
        let file = SourceFile::new("t.hgl", "ab\r\ncd\re").unwrap();
        assert_eq!(file.line_range(0), Some(0..4));
        assert_eq!(file.line_range(1), Some(4..7));
        assert_eq!(file.line_range(2), Some(7..8));
        assert_eq!(file.line_range(3), None);
        assert_eq!(file.line_index(5), 1);
        assert_eq!(file.line_index(99), 2);
    }

    #[test]
    fn column_counts_characters_not_bytes() {
        // 'é' is two bytes, so `x` is at byte offset 2 but in column 2.
        let file = SourceFile::new("t.hgl", "éx").unwrap();
        assert_eq!(file.line_col(2), LineCol { line: 1, column: 2 });
        // An offset inside the two-byte character reports the character.
        assert_eq!(file.line_col(1), file.line_col(0));
    }

    #[test]
    fn offsets_past_the_end_clamp_to_the_end() {
        let file = SourceFile::new("t.hgl", "ab").unwrap();
        assert_eq!(file.line_col(99), LineCol { line: 1, column: 3 });
    }

    #[test]
    fn one_leading_byte_order_mark_is_ignored() {
        // LEX-16. Positions count from the first character after the mark.
        let file = SourceFile::new("t.hgl", "\u{feff}ab\ncd").unwrap();
        assert!(file.has_byte_order_mark());
        assert_eq!(file.text(), "ab\ncd");
        assert_eq!(file.line_col(0), LineCol { line: 1, column: 1 });
        assert_eq!(file.line_col(3), LineCol { line: 2, column: 1 });

        // Only one, and only at the start.
        let file = SourceFile::new("t.hgl", "\u{feff}\u{feff}a\u{feff}").unwrap();
        assert!(file.has_byte_order_mark());
        assert_eq!(file.text(), "\u{feff}a\u{feff}");

        let file = SourceFile::new("t.hgl", "ab").unwrap();
        assert!(!file.has_byte_order_mark());
    }

    #[test]
    fn well_formed_bytes_are_the_text() {
        let file = SourceFile::from_bytes("t.hgl", "let é = 1\n".as_bytes().to_vec()).unwrap();
        assert_eq!(file.text(), "let é = 1\n");
        assert_eq!(file.encoding_error(), None);
    }

    #[test]
    fn bytes_that_are_not_utf8_keep_the_text_before_them() {
        // LEX-16. 0xFF never occurs in UTF-8.
        let file = SourceFile::from_bytes("t.hgl", b"ab\ncd\xff ef".to_vec()).unwrap();
        assert_eq!(file.text(), "ab\ncd");
        let error = file.encoding_error().unwrap();
        assert_eq!(
            error,
            EncodingError {
                offset: 5,
                byte: 0xff
            }
        );
        assert_eq!(file.line_col(error.offset), LineCol { line: 2, column: 3 });

        // A character cut short at the end of the file: 0xC3 opens a
        // two-byte sequence that never completes.
        let file = SourceFile::from_bytes("t.hgl", b"a\xc3".to_vec()).unwrap();
        assert_eq!(
            file.encoding_error(),
            Some(EncodingError {
                offset: 1,
                byte: 0xc3
            })
        );

        // The offset is in the text, which excludes a byte order mark.
        let file = SourceFile::from_bytes("t.hgl", b"\xef\xbb\xbfa\x80".to_vec()).unwrap();
        assert!(file.has_byte_order_mark());
        assert_eq!(file.text(), "a");
        assert_eq!(file.encoding_error().unwrap().offset, 1);
    }

    #[test]
    fn source_map_hands_out_ids_in_order() {
        let mut map = SourceMap::new();
        let first = map.add("a.hgl", "one").unwrap();
        let second = map.add("b.hgl", "two").unwrap();
        assert_eq!(first.index(), 0);
        assert_eq!(second.index(), 1);
        assert_eq!(map.file(second).name(), "b.hgl");
        assert_eq!(map.file(first).slice(Span::new(0, 2)), "on");
        assert_eq!(map.files().len(), 2);
        let third = map.add_bytes("c.hgl", b"three".to_vec()).unwrap();
        assert_eq!(third.index(), 2);
        assert_eq!(map.file(third).text(), "three");
    }
}
