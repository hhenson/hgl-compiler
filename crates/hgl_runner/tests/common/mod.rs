//! Helpers shared by the test files of this directory.
//!
//! Cargo compiles each file directly under `tests/` as a program of its
//! own. A file in a subdirectory is not one; the test files pull this one
//! in with `mod common;`.

// Each test file uses some of these helpers and not others; without this,
// every unused one would be a warning in that file's build.
#![allow(dead_code)]

use hgl_analyser::ParsedFile;
use hgl_diagnostics::Diagnostics;
use hgl_lexer::{lex, literal};
use hgl_parser::{Entry, Limits, parse};
use hgl_runner::{CaseKind, Outcome, Report, Selection, run};
use hgl_source::SourceMap;

/// A run, reduced to lines that tests compare.
pub struct Ran {
    /// The report itself.
    pub report: Report,
    /// One line for each case, `outcome kind identity`, and under a
    /// failed one a line for each annotation no error met (`missing`)
    /// and each error no annotation expects (`unexpected`).
    pub cases: Vec<String>,
    /// One line for each thing that stopped the run: a problem as
    /// `file:line:column message`, an error of the surviving source as
    /// `file:line:column category/code`.
    pub stopped: Vec<String>,
}

/// Lexes and parses a program given as `(file name, text)` pairs and
/// runs every case of it.
pub fn run_all(files: &[(&str, &str)]) -> Ran {
    run_selected(files, &Selection::default())
}

/// Lexes and parses a program and runs the cases `selection` selects.
pub fn run_selected(files: &[(&str, &str)], selection: &Selection) -> Ran {
    let mut sources = SourceMap::new();
    let mut front_end = Diagnostics::new();
    let mut parsed = Vec::new();
    for (name, text) in files {
        let file = sources.add(*name, *text).unwrap();
        let tokens = lex(&sources, file, &mut front_end);
        literal::validate_literals(&sources, file, &tokens, &mut front_end);
        let tree = parse(
            &sources,
            file,
            &tokens,
            Entry::SourceFile,
            Limits::default(),
            &mut front_end,
        );
        parsed.push(ParsedFile::new(file, tree.syntax()));
    }
    let report = run(&sources, &parsed, &front_end, selection);

    // `file:line:column` of a byte of a file.
    let place = |file: hgl_source::FileId, offset: u32| {
        let source = sources.file(file);
        format!("{}:{}", source.name(), source.line_col(offset))
    };
    let mut cases = Vec::new();
    for case in &report.cases {
        let kind = match case.kind {
            CaseKind::Executable => "executable",
            CaseKind::Rejection => "rejection",
        };
        let outcome = match &case.outcome {
            Outcome::Passed => "passed",
            Outcome::Failed { .. } => "failed",
            Outcome::NotRun(_) => "not-run",
        };
        cases.push(format!("{outcome} {kind} {}", case.identity));
        if let Outcome::Failed {
            missing,
            unexpected,
        } = &case.outcome
        {
            for expectation in missing {
                cases.push(format!(
                    "  missing {}/{} {}:{}",
                    expectation.category.as_str(),
                    expectation.code,
                    sources.file(expectation.at.file).name(),
                    expectation.line + 1
                ));
            }
            for diagnostic in unexpected {
                cases.push(format!(
                    "  unexpected {}/{} {}",
                    diagnostic.category.as_str(),
                    diagnostic.code.as_str(),
                    place(diagnostic.primary.file, diagnostic.primary.span.start)
                ));
            }
        }
    }
    let mut stopped = Vec::new();
    for problem in &report.problems {
        match problem.at {
            Some(at) => stopped.push(format!(
                "{} {}",
                place(at.file, at.span.start),
                problem.message
            )),
            None => stopped.push(problem.message.clone()),
        }
    }
    for diagnostic in report.surviving.as_slice() {
        stopped.push(format!(
            "{} {}/{}",
            place(diagnostic.primary.file, diagnostic.primary.span.start),
            diagnostic.category.as_str(),
            diagnostic.code.as_str()
        ));
    }
    Ran {
        report,
        cases,
        stopped,
    }
}
