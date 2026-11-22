//! The documentation stage. It takes the analysed interfaces and the
//! lossless syntax trees they retain (SDOC-4 and SDOC-6) and writes a
//! versioned JSON model. No checking or target stage reads that model.

mod extract;
mod model;

use std::io::{self, Write};

use hgl_analyser::Program;
use hgl_source::{FileId, SourceMap};

/// Writes the API documentation of a checked program as UTF-8 JSON with LF
/// line endings. `path` supplies a portable display path for each file.
/// With `private`, includes declarations outside the module interfaces.
pub fn write_json(
    mut output: impl Write,
    program: &Program,
    sources: &SourceMap,
    path: impl Fn(FileId) -> String,
    private: bool,
) -> io::Result<()> {
    let model = extract::build(program, sources, &path, private);
    serde_json::to_writer_pretty(&mut output, &model)?;
    output.write_all(b"\n")
}
