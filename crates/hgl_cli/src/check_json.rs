//! Editor-facing output for `hgl check`. Compiler stages keep byte spans;
//! the CLI converts both ends to editor coordinates at this boundary.

use std::io::Write;

use hgl_diagnostics::{Diagnostics, Location, Severity};
use hgl_source::{SourceFile, SourceMap};
use serde_json::{Value, json};

/// Writes the versioned, source-ordered diagnostic report, including an empty report.
pub(super) fn write(
    mut writer: impl Write,
    sources: &SourceMap,
    diagnostics: &Diagnostics,
) -> std::io::Result<()> {
    let entries: Vec<_> = diagnostics.as_slice().iter().map(|d| {
        let related: Vec<_> = d.related.iter().map(|r| {
            json!({ "location": location(sources, r.location), "message": r.message })
        }).collect();
        json!({
            "severity": match d.severity { Severity::Error => "error", Severity::Warning => "warning" },
            "category": d.category.as_str(), "code": d.code.as_str(), "catalogued": d.code.is_catalogued(),
            "primary": location(sources, d.primary), "message": d.message,
            "rule": d.rule, "notes": d.notes, "related": related,
        })
    }).collect();
    serde_json::to_writer(
        &mut writer,
        &json!({ "version": 1, "diagnostics": entries }),
    )?;
    writer.flush()
}

fn location(sources: &SourceMap, at: Location) -> Value {
    let file = sources.file(at.file);
    json!({ "file": file.name(), "start": position(file, at.span.start), "end": position(file, at.span.end) })
}

fn position(file: &SourceFile, offset: u32) -> Value {
    let line = file.line_index(offset);
    let range = file
        .line_range(line)
        .expect("line index belongs to the file");
    let text = file.text()[range.clone()].trim_end_matches(['\r', '\n']);
    let byte = text.floor_char_boundary((offset as usize - range.start).min(text.len()));
    json!({ "line": line, "character": text[..byte].encode_utf16().count() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufWriter, Error, Result};

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
