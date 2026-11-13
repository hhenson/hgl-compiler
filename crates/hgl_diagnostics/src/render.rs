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
//! error[parse/lex.unclosed_string]: string has no closing quote
//!   ┌─ example.hgl:3:13
//!   │
//! 3 │     let s = "abc
//!   │             ^^^^
//!   │
//!   = rule LEX-2
//!   = lex.unclosed_string is a compiler-local code; the specification has not catalogued this rejection
//! ```

use std::io::{self, IsTerminal};

use codespan_reporting::diagnostic as cs;
use codespan_reporting::files::SimpleFiles;
use codespan_reporting::term::termcolor::{ColorChoice, StandardStream};
use codespan_reporting::term::{self, Config};
use hgl_source::SourceMap;

use crate::{Diagnostic, Diagnostics, Severity};

/// Renders every diagnostic as plain text, without colour. Used by tests and
/// when output is captured.
pub fn to_string(sources: &SourceMap, diagnostics: &Diagnostics) -> String {
    let files = renderer_files(sources);
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
    let files = renderer_files(sources);
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

/// Builds the renderer's view of our files.
///
/// `SimpleFiles` numbers files 0, 1, 2... in the order they are added, which
/// is exactly how [`hgl_source::FileId::index`] numbers them, so a `FileId`
/// index can be used as the renderer's file id. The renderer borrows the
/// names and text (`&str`); nothing is copied.
fn renderer_files(sources: &SourceMap) -> SimpleFiles<&str, &str> {
    let mut files = SimpleFiles::new();
    for file in sources.files() {
        files.add(file.name(), file.text());
    }
    files
}

/// Translates one diagnostic into the renderer's form.
fn translate(diagnostic: &Diagnostic) -> cs::Diagnostic<usize> {
    let severity = match diagnostic.severity {
        Severity::Error => cs::Severity::Error,
        Severity::Warning => cs::Severity::Warning,
    };
    let label = cs::Label::primary(
        diagnostic.primary.file.index(),
        diagnostic.primary.span.range(),
    );
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
        .with_label(label)
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
                Code::Local("lex.unclosed_string"),
                Location::new(file, Span::new(8, 12)),
                "string has no closing quote",
            )
            .with_rule("LEX-2"),
        );
        let text = to_string(&sources, &diagnostics);
        assert!(
            text.contains("error[parse/lex.unclosed_string]: string has no closing quote"),
            "{text}"
        );
        assert!(text.contains("example.hgl:1:9"), "{text}");
        assert!(text.contains("rule LEX-2"), "{text}");
        assert!(text.contains("compiler-local code"), "{text}");
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
