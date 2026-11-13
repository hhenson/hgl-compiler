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
//! # For readers coming from C++
//!
//! * `&str` is a borrowed UTF-8 string view (`std::string_view`); `String`
//!   owns its bytes (`std::string`).
//! * `#[derive(...)]` asks the compiler to generate the listed operations
//!   (copying, `==`, ordering, hashing, debug printing) member-wise.
//! * `Option<T>` is `std::optional<T>`; `Result<T, E>` is `std::expected<T, E>`.

use std::fmt;
use std::ops::Range;

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

/// One source file: its name, its text, and an index of where lines start.
#[derive(Debug, Clone)]
pub struct SourceFile {
    name: String,
    text: String,
    /// Byte offset of the first character of each line. Always starts with 0,
    /// so the vector's length is the number of lines.
    line_starts: Vec<u32>,
}

impl SourceFile {
    /// Wraps `text` under the display name `name` (normally its path).
    ///
    /// A line ends at a line feed (`\n`). A carriage return before it belongs
    /// to the same line ending, so both LF and CRLF files index correctly.
    pub fn new(
        name: impl Into<String>,
        text: impl Into<String>,
    ) -> Result<SourceFile, SourceError> {
        let name = name.into();
        let text = text.into();
        if u32::try_from(text.len()).is_err() {
            return Err(SourceError::TooLarge { name });
        }
        let mut line_starts = vec![0];
        for (offset, byte) in text.bytes().enumerate() {
            if byte == b'\n' {
                // The cast cannot truncate: the length check above passed.
                line_starts.push(offset as u32 + 1);
            }
        }
        Ok(SourceFile {
            name,
            text,
            line_starts,
        })
    }

    /// The display name given at construction.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The complete source text.
    pub fn text(&self) -> &str {
        &self.text
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

    /// Converts a byte offset to a 1-based line and column.
    ///
    /// An offset past the end is treated as the end, and an offset inside a
    /// multi-byte character as the start of that character.
    pub fn line_col(&self, offset: u32) -> LineCol {
        let mut offset = (offset as usize).min(self.text.len());
        while !self.text.is_char_boundary(offset) {
            offset -= 1;
        }
        // `partition_point` is a binary search: it returns how many line
        // starts are at or before `offset`, which is the 1-based line number.
        let line = self
            .line_starts
            .partition_point(|&start| start as usize <= offset);
        let line_start = self.line_starts[line - 1] as usize;
        let column = self.text[line_start..offset].chars().count() + 1;
        LineCol {
            line: line as u32,
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

    /// Adds a file and returns its id.
    pub fn add(
        &mut self,
        name: impl Into<String>,
        text: impl Into<String>,
    ) -> Result<FileId, SourceError> {
        let file = SourceFile::new(name, text)?;
        let id = FileId(self.files.len() as u32);
        self.files.push(file);
        Ok(id)
    }

    /// The file an id names. Ids come only from [`SourceMap::add`] on this
    /// map, so the lookup cannot fail for a correctly used id.
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
    fn crlf_line_endings_index_like_lf() {
        let file = SourceFile::new("t.hgl", "ab\r\ncd").unwrap();
        assert_eq!(file.line_col(4), LineCol { line: 2, column: 1 });
        assert_eq!(file.line_col(5), LineCol { line: 2, column: 2 });
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
    fn source_map_hands_out_ids_in_order() {
        let mut map = SourceMap::new();
        let first = map.add("a.hgl", "one").unwrap();
        let second = map.add("b.hgl", "two").unwrap();
        assert_eq!(first.index(), 0);
        assert_eq!(second.index(), 1);
        assert_eq!(map.file(second).name(), "b.hgl");
        assert_eq!(map.file(first).slice(Span::new(0, 2)), "on");
        assert_eq!(map.files().len(), 2);
    }
}
