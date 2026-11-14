//! Turns diagnostics into text for a terminal.
//!
//! The layout (source excerpt, underline, notes) is delegated to the
//! `codespan-reporting` crate. This module only translates our
//! [`Diagnostic`] into that crate's form, so the rest of the compiler does
//! not depend on it.
//!
//! A rendered diagnostic looks like:
//!
//! ```text
//! error[parse/string.unclosed]: string has no closing quote
//!   ┌─ example.hgl:3:13
//!   │
//! 3 │     let s = "abc
//!   │             ^^^^
//!   │
//!   = rule LEX-2
//!   = a string must close on the line it opens on
//! ```
//!
//! The heading is the diagnostic's category and code. The notes begin with
//! the specification rule it cites. A code the specification has not
//! catalogued gets a further note that says it is local to this compiler.

use std::io::{self, IsTerminal};
use std::ops::Range;

use codespan_reporting::diagnostic as cs;
use codespan_reporting::files::{self, Files};
use codespan_reporting::term::termcolor::{ColorChoice, StandardStream};
use codespan_reporting::term::{self, Config};
use hgl_source::{SourceFile, SourceMap};

use crate::{Diagnostic, Diagnostics, Severity};

/// Renders every diagnostic as plain text, without colour. Used by tests and
/// when output is captured.
pub fn to_string(sources: &SourceMap, diagnostics: &Diagnostics) -> String {
    let files = RendererFiles(sources);
    let config = Config::default();
    let mut out = String::new();
    for diagnostic in diagnostics.as_slice() {
        term::emit_to_string(&mut out, &config, &files, &translate(diagnostic))
            .expect("diagnostic locations refer to files in the source map");
    }
    out
}

/// Renders every diagnostic to standard error, in colour when standard error
/// is a terminal.
pub fn to_stderr(sources: &SourceMap, diagnostics: &Diagnostics) -> io::Result<()> {
    let files = RendererFiles(sources);
    let config = Config::default();
    let choice = if io::stderr().is_terminal() {
        ColorChoice::Auto
    } else {
        ColorChoice::Never
    };
    let stream = StandardStream::stderr(choice);
    // Locking once avoids interleaving with other output while we write.
    let mut writer = stream.lock();
    for diagnostic in diagnostics.as_slice() {
        term::emit_to_write_style(&mut writer, &config, &files, &translate(diagnostic))
            .map_err(io::Error::other)?;
    }
    Ok(())
}

/// The renderer's view of our files.
///
/// The renderer asks for names, text and line positions through its
/// [`Files`] trait. Its ready-made implementation finds lines by looking for
/// `\n` alone, so a file written with carriage returns would be one long
/// line to it. Answering from our [`SourceMap`] instead means the excerpt
/// and the `file:line:column` heading use the one line index the rest of
/// the compiler uses (see [`hgl_source::line_break_length`]).
///
/// This is a "newtype": a struct with a single unnamed field. Rust only
/// lets a crate implement a trait when it owns the trait or the type, and
/// both `Files` and `SourceMap` come from other crates, so we wrap the
/// borrowed map in a type of our own. The wrapper costs nothing at run time.
///
/// The renderer's file id is [`hgl_source::FileId::index`]: the position of
/// the file in the map.
struct RendererFiles<'a>(&'a SourceMap);

impl<'a> RendererFiles<'a> {
    /// The file with index `id`, or the renderer's "no such file" error.
    fn file(&self, id: usize) -> Result<&'a SourceFile, files::Error> {
        self.0.files().get(id).ok_or(files::Error::FileMissing)
    }
}

// `Files<'a>` carries a lifetime so that the names and text it hands out can
// be borrowed (`&'a str`) from the source map rather than copied.
impl<'a> Files<'a> for RendererFiles<'a> {
    type FileId = usize;
    type Name = &'a str;
    type Source = &'a str;

    fn name(&'a self, id: usize) -> Result<&'a str, files::Error> {
        Ok(self.file(id)?.name())
    }

    fn source(&'a self, id: usize) -> Result<&'a str, files::Error> {
        Ok(self.file(id)?.text())
    }

    fn line_index(&'a self, id: usize, byte_index: usize) -> Result<usize, files::Error> {
        // A source file is under 4 GiB, so an offset that does not fit in a
        // `u32` is past the end, which `SourceFile::line_index` treats as
        // the end.
        let offset = u32::try_from(byte_index).unwrap_or(u32::MAX);
        Ok(self.file(id)?.line_index(offset))
    }

    fn line_range(&'a self, id: usize, line_index: usize) -> Result<Range<usize>, files::Error> {
        let file = self.file(id)?;
        file.line_range(line_index)
            .ok_or(files::Error::LineTooLarge {
                given: line_index,
                // A file always has at least one line.
                max: file.line_count() as usize - 1,
            })
    }
}

/// Translates one diagnostic into the renderer's form.
fn translate(diagnostic: &Diagnostic) -> cs::Diagnostic<usize> {
    let severity = match diagnostic.severity {
        Severity::Error => cs::Severity::Error,
        Severity::Warning => cs::Severity::Warning,
    };
    let mut labels = vec![cs::Label::primary(
        diagnostic.primary.file.index(),
        diagnostic.primary.span.range(),
    )];
    // A related place is a second underline with its own words, in the
    // same file or another.
    for related in &diagnostic.related {
        labels.push(
            cs::Label::secondary(related.location.file.index(), related.location.span.range())
                .with_message(&related.message),
        );
    }
    let mut notes = Vec::new();
    if let Some(rule) = diagnostic.rule {
        notes.push(format!("rule {rule}"));
    }
    if !diagnostic.code.is_catalogued() {
        notes.push(format!(
            "{} is a compiler-local code; the specification has not catalogued this rejection",
            diagnostic.code.as_str()
        ));
    }
    notes.extend(diagnostic.notes.iter().cloned());
    cs::Diagnostic::new(severity)
        .with_code(format!(
            "{}/{}",
            diagnostic.category.as_str(),
            diagnostic.code.as_str()
        ))
        .with_message(&diagnostic.message)
        .with_labels(labels)
        .with_notes(notes)
}

#[cfg(test)]
mod tests {
    use hgl_source::Span;

    use super::*;
    use crate::{Category, Code, Location};

    #[test]
    fn rendering_shows_code_position_rule_and_local_marker() {
        let mut sources = SourceMap::new();
        let file = sources.add("example.hgl", "let s = \"abc\n").unwrap();
        let mut diagnostics = Diagnostics::new();
        diagnostics.emit(
            Diagnostic::error(
                Category::Parse,
                Code::Local("example.local_code"),
                Location::new(file, Span::new(8, 12)),
                "string has no closing quote",
            )
            .with_rule("LEX-2"),
        );
        let text = to_string(&sources, &diagnostics);
        assert!(
            text.contains("error[parse/example.local_code]: string has no closing quote"),
            "{text}"
        );
        assert!(text.contains("example.hgl:1:9"), "{text}");
        assert!(text.contains("rule LEX-2"), "{text}");
        assert!(text.contains("compiler-local code"), "{text}");
    }

    /// Renders one error on the `x` of `let x = 1` in a three-line file
    /// whose lines are separated by `line_break`.
    fn render_third_line(line_break: &str) -> String {
        let text = ["# one", "# two", "let x = 1", ""].join(line_break);
        let start = text.find('x').unwrap() as u32;
        let mut sources = SourceMap::new();
        let file = sources.add("example.hgl", text).unwrap();
        let mut diagnostics = Diagnostics::new();
        diagnostics.emit(Diagnostic::error(
            Category::Parse,
            Code::Local("example.local_code"),
            Location::new(file, Span::new(start, start + 1)),
            "example",
        ));
        to_string(&sources, &diagnostics)
    }

    #[test]
    fn every_line_break_form_renders_the_same_excerpt() {
        // The renderer's own line index would see a file written
        // with carriage returns as a single line.
        let expected = render_third_line("\n");
        assert!(expected.contains("example.hgl:3:5"), "{expected}");
        assert!(expected.contains("3 │ let x = 1"), "{expected}");
        assert!(!expected.contains("# two"), "{expected}");
        for line_break in ["\r\n", "\r"] {
            assert_eq!(render_third_line(line_break), expected, "{line_break:?}");
        }
    }

    #[test]
    fn a_location_in_a_missing_file_is_an_error_not_a_panic() {
        let sources = SourceMap::new();
        let files = RendererFiles(&sources);
        assert!(matches!(files.name(0), Err(files::Error::FileMissing)));
        assert!(matches!(
            files.line_range(0, 0),
            Err(files::Error::FileMissing)
        ));
    }

    #[test]
    fn a_line_past_the_end_is_reported_with_the_last_line_index() {
        let mut sources = SourceMap::new();
        sources.add("example.hgl", "a\rb").unwrap();
        let files = RendererFiles(&sources);
        assert_eq!(files.line_range(0, 1).unwrap(), 2..3);
        assert!(matches!(
            files.line_range(0, 2),
            Err(files::Error::LineTooLarge { given: 2, max: 1 })
        ));
        // An offset beyond the text counts as its end.
        assert_eq!(files.line_index(0, usize::MAX).unwrap(), 1);
    }

    #[test]
    fn a_related_place_is_shown_with_its_words() {
        let mut sources = SourceMap::new();
        let first = sources.add("first.hgl", "fn scale() {}\n").unwrap();
        let second = sources.add("second.hgl", "fn scale() {}\n").unwrap();
        let mut diagnostics = Diagnostics::new();
        diagnostics.emit(
            Diagnostic::error(
                Category::Name,
                Code::Local("example.local_code"),
                Location::new(second, Span::new(3, 8)),
                "`scale` is declared twice",
            )
            .with_related(Location::new(first, Span::new(3, 8)), "first declared here"),
        );
        let text = to_string(&sources, &diagnostics);
        assert!(text.contains("second.hgl:1:4"), "{text}");
        assert!(text.contains("first.hgl:1:4"), "{text}");
        assert!(text.contains("first declared here"), "{text}");
    }

    #[test]
    fn catalogued_codes_carry_no_local_marker() {
        let mut sources = SourceMap::new();
        let file = sources.add("example.hgl", "fn\n").unwrap();
        let mut diagnostics = Diagnostics::new();
        diagnostics.emit(Diagnostic::error(
            Category::Parse,
            Code::Catalogued("syntax.expected_token"),
            Location::new(file, Span::new(2, 2)),
            "expected a function name",
        ));
        let text = to_string(&sources, &diagnostics);
        assert!(
            text.contains("error[parse/syntax.expected_token]"),
            "{text}"
        );
        assert!(!text.contains("compiler-local"), "{text}");
    }
}
