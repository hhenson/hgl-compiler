//! Groups files into modules (UNIT-1).
//!
//! > A file names one module and optionally one part. An assembled module
//! > has one declaration scope, at most one unnamed file, and unique part
//! > names.

use std::collections::BTreeMap;

use hgl_diagnostics::{Category, Diagnostics, Location};
use hgl_parser::HasSpan;
use hgl_parser::ast::{AstNode, SourceFile};
use hgl_source::Span;

use crate::program::{Module, ModuleFile, Program};
use crate::{ParsedFile, codes, error};

/// A file on its way into a module.
struct Entry<'a> {
    file: ModuleFile,
    /// Where the file's module path is written.
    path_at: Location,
    /// Where its part name is written, when it has one.
    part_at: Option<Location>,
    /// The file's `omit` list.
    omit: &'a [Span],
}

/// Builds the program's modules, each with its files and nothing else
/// yet, and the `omit` lists of those files in the same order.
///
/// `'a` is a lifetime: it says the lists returned are borrowed from
/// `files` and live as long as it does.
pub(crate) fn assemble<'a>(
    files: &'a [ParsedFile],
    diagnostics: &mut Diagnostics,
) -> (Program, Vec<Vec<&'a [Span]>>) {
    // A `BTreeMap` keeps its keys in order, so the modules come out in
    // order of path whatever order the files came in.
    let mut by_path: BTreeMap<String, Vec<Entry<'a>>> = BTreeMap::new();
    for parsed in files {
        let Some(source) = SourceFile::cast(parsed.root.clone()) else {
            continue;
        };
        // A file with no module line, or none the parser could read, is
        // in no module. The parser has reported it (UNIT-1).
        let Some(header) = source.module_decl() else {
            continue;
        };
        let Some(path) = header.path() else {
            continue;
        };
        let dotted = path.dotted();
        if dotted.is_empty() {
            continue;
        }
        let part = header.part();
        by_path.entry(dotted).or_default().push(Entry {
            path_at: Location::new(parsed.file, path.syntax().span()),
            part_at: part
                .as_ref()
                .map(|token| Location::new(parsed.file, token.span())),
            file: ModuleFile {
                file: parsed.file,
                part: part.map(|token| token.text().to_string()),
                imports: Vec::new(),
                syntax: source,
            },
            omit: &parsed.omit,
        });
    }

    let mut program = Program::default();
    let mut omits = Vec::new();
    for (path, mut entries) in by_path {
        // The unnamed file first, then the parts by name: `None` sorts
        // before `Some`. The sort is stable, so files that tie keep the
        // order they were given in; they are the ones reported below.
        entries.sort_by(|left, right| left.file.part.cmp(&right.file.part));
        check_parts(&path, &entries, diagnostics);
        omits.push(entries.iter().map(|entry| entry.omit).collect());
        program.modules.push(Module {
            path,
            files: entries.into_iter().map(|entry| entry.file).collect(),
            declarations: Vec::new(),
            tests: Vec::new(),
            prelude_names: Vec::new(),
            uses: Vec::new(),
            decisions: Vec::new(),
            scope: Default::default(),
            instantiations: Vec::new(),
        });
    }
    (program, omits)
}

/// UNIT-1: at most one unnamed file, and unique part names. `entries` is
/// sorted, so files that clash are next to one another.
fn check_parts(path: &str, entries: &[Entry<'_>], diagnostics: &mut Diagnostics) {
    // `windows(2)` gives each pair of neighbours.
    for pair in entries.windows(2) {
        let (first, later) = (&pair[0], &pair[1]);
        if first.file.part != later.file.part {
            continue;
        }
        let diagnostic = match (&later.file.part, later.part_at) {
            (Some(part), Some(part_at)) => error(
                Category::Module,
                codes::MODULE_DUPLICATE_PART,
                part_at,
                format!("module `{path}` has two parts named `{part}`"),
                "UNIT-1",
            )
            .with_related(first.part_at.unwrap_or(first.path_at), "the other is here"),
            _ => error(
                Category::Module,
                codes::MODULE_DUPLICATE_UNNAMED,
                later.path_at,
                format!("module `{path}` has two unnamed files"),
                "UNIT-1",
            )
            .with_related(first.path_at, "the other is here")
            .with_note(format!(
                "name all but one of a module's files: `module {path} part <name>`"
            )),
        };
        diagnostics.emit(diagnostic);
    }
}
