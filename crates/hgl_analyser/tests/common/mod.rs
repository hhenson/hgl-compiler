//! Helpers shared by the test files of this directory.
//!
//! Cargo compiles each file directly under `tests/` as a program of its
//! own. A file in a subdirectory is not one; the test files pull this one
//! in with `mod common;`.

// Each test file uses some of these helpers and not others; without this,
// every unused one would be a warning in that file's build.
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use hgl_analyser::{ParsedFile, Program, analyse};
use hgl_diagnostics::{Diagnostics, render};
use hgl_lexer::{lex, literal};
use hgl_parser::{Entry, Limits, parse};
use hgl_source::{SourceMap, Span};

/// One diagnostic, reduced to what tests compare.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Where it points, as `file:line:column`.
    pub at: String,
    /// The category and code, for example `name/name.duplicate`.
    pub code: String,
    /// The rule cited, for example `UNIT-12`.
    pub rule: &'static str,
    /// The message.
    pub message: String,
}

/// What the compiler makes of a program.
pub struct Checked {
    /// The analysed program.
    pub program: Program,
    /// The analyser's diagnostics, in order of file name and position.
    pub findings: Vec<Finding>,
    /// How many diagnostics the lexer and parser reported.
    pub front_end_errors: usize,
    /// Every diagnostic as `hgl` prints it; empty when there are none.
    pub report: String,
    /// The sources, for looking up positions.
    pub sources: SourceMap,
}

impl Checked {
    /// Each finding as `file:line:column code rule`, for comparing in one
    /// assertion.
    pub fn summary(&self) -> Vec<String> {
        self.findings
            .iter()
            .map(|finding| format!("{} {} {}", finding.at, finding.code, finding.rule))
            .collect()
    }

    /// The names module `path` is waiting on the prelude for.
    pub fn prelude_names(&self, path: &str) -> Vec<&str> {
        self.program
            .module(path)
            .expect("the module exists")
            .prelude_names
            .iter()
            .map(|name| name.name.as_str())
            .collect()
    }
}

/// Lexes, parses and analyses a program given as `(file name, text)`
/// pairs. The lexer and parser must accept every file.
pub fn check(files: &[(&str, &str)]) -> Checked {
    let checked = check_with(files, |_, _| Vec::new());
    assert_eq!(
        checked.front_end_errors, 0,
        "the lexer and parser accept the files:\n{}",
        checked.report
    );
    checked
}

/// As [`check`], for files the parser may reject.
pub fn check_broken(files: &[(&str, &str)]) -> Checked {
    check_with(files, |_, _| Vec::new())
}

/// Lexes, parses and analyses a program. `omit` is asked, for each file
/// name and text, which declarations to leave out, as byte spans.
pub fn check_with(files: &[(&str, &str)], omit: impl Fn(&str, &str) -> Vec<Span>) -> Checked {
    let mut sources = SourceMap::new();
    let mut diagnostics = Diagnostics::new();
    let mut parsed = Vec::new();
    for (name, text) in files {
        let file = sources.add(*name, *text).unwrap();
        let tokens = lex(&sources, file, &mut diagnostics);
        literal::validate_literals(&sources, file, &tokens, &mut diagnostics);
        let tree = parse(
            &sources,
            file,
            &tokens,
            Entry::SourceFile,
            Limits::default(),
            &mut diagnostics,
        );
        parsed.push(ParsedFile {
            file,
            root: tree.syntax(),
            omit: omit(name, text),
        });
    }
    let front_end_errors = diagnostics.len();

    let program = analyse(&parsed, &mut diagnostics);
    // The analyser's diagnostics are the ones after the front end's, here
    // put in order of file and position.
    let mut reported: Vec<_> = diagnostics.as_slice()[front_end_errors..].to_vec();
    reported.sort_by_key(|diagnostic| {
        (
            sources.file(diagnostic.primary.file).name().to_string(),
            diagnostic.primary.span.start,
        )
    });
    let findings: Vec<Finding> = reported
        .iter()
        .map(|diagnostic| {
            let source = sources.file(diagnostic.primary.file);
            let span = diagnostic.primary.span;
            assert!(
                span.start <= span.end && span.end as usize <= source.text().len(),
                "a diagnostic points inside its file"
            );
            assert!(
                !diagnostic.code.is_catalogued(),
                "the catalogue has no module or name code yet"
            );
            Finding {
                at: format!("{}:{}", source.name(), source.line_col(span.start)),
                code: format!(
                    "{}/{}",
                    diagnostic.category.as_str(),
                    diagnostic.code.as_str()
                ),
                rule: diagnostic
                    .rule
                    .expect("every analyser diagnostic cites a rule"),
                message: diagnostic.message.clone(),
            }
        })
        .collect();
    diagnostics.sort_by_location();
    let report = if diagnostics.is_empty() {
        String::new()
    } else {
        render::to_string(&sources, &diagnostics)
    };
    Checked {
        program,
        findings,
        front_end_errors,
        report,
        sources,
    }
}

/// The span of the first occurrence of `needle` in `text`, up to the end
/// of its line or of its closing brace, whichever `needle` asks for: the
/// span runs from the start of `needle` to the end of `through`.
pub fn span_of(text: &str, needle: &str, through: &str) -> Span {
    let start = text.find(needle).expect("the text holds the needle");
    let end = text[start..]
        .find(through)
        .expect("the text holds the end marker")
        + start
        + through.len();
    Span::new(start as u32, end as u32)
}

// ---- The specification's HGL ----

/// One piece of HGL source and where it came from.
pub struct Sample {
    /// For example `hgl_spec/spec/language/source-text.md:12`.
    pub origin: String,
    /// The source, with LF line breaks.
    pub text: String,
}

/// The `spec/` directory at the workspace root.
pub fn spec_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is this crate's directory, set by Cargo when it
    // compiles the test. The workspace root is two levels up.
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec")
}

/// Collects every file under `directory`, depth first, in name order so
/// the tests visit files in the same order on every machine.
fn files_under(directory: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            files_under(&path, found);
        } else {
            found.push(path);
        }
    }
}

/// The fenced `hgl` blocks of a Markdown document, each with the 1-based
/// line number of its first line of code.
fn hgl_blocks(markdown: &str) -> Vec<(usize, String)> {
    let mut blocks = Vec::new();
    // `Some((first_line, text_so_far))` while inside a block.
    let mut open: Option<(usize, String)> = None;
    for (index, line) in markdown.lines().enumerate() {
        let fence = line.trim_start().starts_with("```");
        match (&mut open, fence) {
            (None, true) if line.trim() == "```hgl" => open = Some((index + 2, String::new())),
            (Some(_), true) => blocks.extend(open.take()),
            (Some((_, text)), false) => {
                text.push_str(line);
                text.push('\n');
            }
            _ => {}
        }
    }
    blocks
}

/// Reads a file as text with LF line breaks, whatever the checkout wrote.
fn read(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
        .replace("\r\n", "\n")
        .replace('\r', "\n")
}

/// The path of `path` under `spec/`, with `/` on every system.
fn origin(path: &Path) -> String {
    path.strip_prefix(spec_root())
        .unwrap_or(path)
        .display()
        .to_string()
        .replace('\\', "/")
}

/// Every complete module among the pinned specification's examples: the
/// fenced `hgl` blocks of its Markdown that start with a `module` line.
pub fn example_modules() -> Vec<Sample> {
    let mut files = Vec::new();
    files_under(&spec_root(), &mut files);
    let mut samples = Vec::new();
    for path in files {
        if path.extension().is_some_and(|extension| extension == "md") {
            for (line, text) in hgl_blocks(&read(&path)) {
                if text.trim_start().starts_with("module ") {
                    samples.push(Sample {
                        origin: format!("{}:{line}", origin(&path)),
                        text,
                    });
                }
            }
        }
    }
    // An empty corpus means the submodules are not checked out. Passing
    // then would report success for a check that never ran.
    assert!(
        !samples.is_empty(),
        "no HGL found under spec/; run `git submodule update --init`"
    );
    samples
}

/// Every `.hgl` file under `directory` of `spec/`.
pub fn hgl_files(directory: &str) -> Vec<Sample> {
    let mut files = Vec::new();
    files_under(&spec_root().join(directory), &mut files);
    let samples: Vec<Sample> = files
        .iter()
        .filter(|path| path.extension().is_some_and(|extension| extension == "hgl"))
        .map(|path| Sample {
            origin: origin(path),
            text: read(path),
        })
        .collect();
    assert!(
        !samples.is_empty(),
        "no HGL found under spec/{directory}; run `git submodule update --init`"
    );
    samples
}
