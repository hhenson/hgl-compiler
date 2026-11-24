//! The documentation stage. It takes the analysed interfaces and the
//! lossless syntax trees they retain (SDOC-4 and SDOC-6) and writes a
//! versioned JSON model and a static HTML site from normalized text and
//! structured sections (SDOC-2, SDOC-3).

mod extract;
mod links;
mod markdown;
mod model;
mod signatures;
mod site;

use std::io::{self, Write};

use hgl_analyser::Program;
use hgl_diagnostics::Location;
use hgl_source::{FileId, SourceMap};

pub use site::SiteFile;

/// The analysed module and source position of a comment (SDOC-8).
#[derive(Clone, Copy)]
struct LinkContext<'a> {
    module: &'a str,
    at: Location,
}

/// Builds the static HTML reference for a checked program. The returned
/// paths are relative to the site root and use forward slashes.
pub fn build_site(
    program: &Program,
    sources: &SourceMap,
    path: impl Fn(FileId) -> String,
    private: bool,
) -> Vec<SiteFile> {
    site::build(program, extract::build(program, sources, &path, private))
}

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
