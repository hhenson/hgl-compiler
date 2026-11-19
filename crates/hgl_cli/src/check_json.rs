//! The report `hgl check --json` writes for an editor: every diagnostic
//! of the program, as one JSON object on standard output.
//! `docs/architecture.md` describes the format.
//!
//! It comes after the analyser, and changes nothing the stages found.
//! The stages keep a place as a byte range of a file; an editor counts
//! lines from zero and columns in UTF-16 units, as the Language Server
//! Protocol does. This module converts both ends of each range, here at
//! the edge of the compiler.

use std::io::Write;

use hgl_diagnostics::{Diagnostic, Diagnostics, Location, Related, Severity};
use hgl_source::{SourceFile, SourceMap};
use serde_json::{Value, json};

/// Writes the report on `diagnostics` to `writer`:
/// `{"version": 1, "diagnostics": [...]}`, an entry for each diagnostic
/// in the order it is given (the caller sorts them by place). A program
/// with nothing to report has an empty array. A failure to write, which
/// a buffered `writer` may hold back until it is flushed, is returned.
pub(super) fn write(
    mut writer: impl Write,
    sources: &SourceMap,
    diagnostics: &Diagnostics,
) -> std::io::Result<()> {
    let entries: Vec<Value> = diagnostics
        .as_slice()
        .iter()
        .map(|diagnostic| entry(sources, diagnostic))
        .collect();
    // `json!` builds a JSON value from text written as JSON, each value
    // in it a Rust expression.
    serde_json::to_writer(
        &mut writer,
        &json!({ "version": 1, "diagnostics": entries }),
    )?;
    writer.flush()
}

/// One diagnostic as the report writes it: its severity, category and
/// code; whether the code is the specification's (`catalogued`) or the
/// compiler's own; its place and message; the rule that requires it,
/// `null` for none; its notes; and the other places it names.
fn entry(sources: &SourceMap, diagnostic: &Diagnostic) -> Value {
    let severity = match diagnostic.severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
    };
    let related: Vec<Value> = diagnostic
        .related
        .iter()
        .map(|related| related_entry(sources, related))
        .collect();
    json!({
        "severity": severity,
        "category": diagnostic.category.as_str(),
        "code": diagnostic.code.as_str(),
        "catalogued": diagnostic.code.is_catalogued(),
        "primary": location(sources, diagnostic.primary),
        "message": diagnostic.message,
        "rule": diagnostic.rule,
        "notes": diagnostic.notes,
        "related": related,
    })
}

/// A place a diagnostic names besides its own, `first declared here`
/// say: where it is, and what it is.
fn related_entry(sources: &SourceMap, related: &Related) -> Value {
    json!({
        "location": location(sources, related.location),
        "message": related.message,
    })
}

/// A place as the report writes it: the file's name, and the positions
/// of the range's two ends, the end one past the last character.
///
/// The name is the one the file was read under: its path, as the source
/// roots gave it. A file of the core the compiler supplies was read
/// from no path, and carries the display name the compiler gives it,
/// such as `<core>/core/core.hgl`, which is not a path.
fn location(sources: &SourceMap, at: Location) -> Value {
    let file = sources.file(at.file);
    json!({
        "file": file.name(),
        "start": position(file, at.span.start),
        "end": position(file, at.span.end),
    })
}

/// The place of the byte at `offset` as an editor counts it: the line,
/// from zero, and the column, the number of UTF-16 units before it on
/// that line, from zero. A character outside the Basic Multilingual
/// Plane, an emoji say, is two units.
///
/// A line ends at any of the line breaks of LEX-15. An offset inside a
/// line break is the end of that line's text: a column counts
/// characters, and a line break is none. One past the end of the file
/// is its end. A leading byte order mark is not counted: LEX-16 ignores
/// it, and [`SourceFile`] leaves it out of the text the offsets count,
/// so a column of the first line starts after it.
fn position(file: &SourceFile, offset: u32) -> Value {
    let line = file.line_index(offset);
    let range = file
        .line_range(line)
        .expect("line index belongs to the file");
    // The line's text, without the line break that ends it. The pattern
    // `['\r', '\n']` matches either character: CRLF, LF and CR all go.
    let text = file.text()[range.clone()].trim_end_matches(['\r', '\n']);
    // The offset within the line, held to its text. No stage gives one
    // inside a character; `floor_char_boundary` would take its start.
    let byte = text.floor_char_boundary((offset as usize - range.start).min(text.len()));
    // `encode_utf16` gives the text's UTF-16 units, one at a time.
    json!({ "line": line, "character": text[..byte].encode_utf16().count() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufWriter, Error, Result};

    /// A failure to write that a buffer held until the flush is not
    /// lost: `write` returns it.
    #[test]
    fn a_buffered_output_failure_reaches_the_caller() {
        struct Full;
        impl Write for Full {
            fn write(&mut self, _: &[u8]) -> Result<usize> {
                Err(Error::other("full"))
            }
            fn flush(&mut self) -> Result<()> {
                Ok(())
            }
        }
        let result = write(BufWriter::new(Full), &SourceMap::new(), &Diagnostics::new());
        assert_eq!(result.unwrap_err().to_string(), "full");
    }
}
