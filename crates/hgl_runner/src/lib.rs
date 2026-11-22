//! The test runner: decides which cases a program holds and whether each
//! passes.
//!
//! ```text
//! (parser) --> syntax trees ---+
//!                              +--[run]--> report: each case and its outcome
//! lexer and parser diagnostics-+     |
//!                                    +-- calls the analyser, once for the
//!                                        program and once for each case
//! ```
//!
//! The runner is not a stage of the pipeline: it drives the stages. A
//! program holds two kinds of case (TST-15):
//!
//! * an **executable** case is a named test. Running one needs a target
//!   and a host to run the written program on, which the runner knows
//!   nothing of: it hands the tests to an [`Executor`], and lists them
//!   as not run when it is given none;
//! * a **rejection** case is a declaration, import or test that is
//!   expected to be rejected, marked with `# expect-error` annotations
//!   (TST-12). The runner checks these now.
//!
//! # How a rejection case is checked (TST-13)
//!
//! 1. `annotation` reads every `# expect-error` comment.
//! 2. `owner` finds the item each applies to: its *owner*.
//! 3. The program is analysed with every owner left out. What is left,
//!    the *surviving* source, must be accepted. An error of the lexer or
//!    parser is an owner's when it is inside that owner, and the
//!    surviving source's otherwise.
//! 4. For each owner, the program is analysed again with that owner put
//!    back. Every error then reported must be one an annotation expects,
//!    by category, code, file and line, and every annotation must be met
//!    by one error.
//!
//! The owner is put back and nothing else is changed: "no stubs or
//! placeholder overloads are inserted".

mod annotation;
mod owner;

use hgl_analyser::{ParsedFile, Program, analyse};
use hgl_diagnostics::{Diagnostic, Diagnostics, Location, Severity};
use hgl_source::{FileId, SourceMap, Span};
use serde::{Deserialize, Serialize};

pub use annotation::Expectation;
use owner::{Owner, owner_of, owner_of_error, owners};

/// Why no executable case is run when the run is given no
/// [`Executor`].
pub const NO_TARGET: &str = "no target was given to run a test on";

/// What runs executable cases: a target, and the host it runs a
/// program on. The runner knows no target; whoever starts a run gives
/// it one.
pub trait Executor {
    /// Runs the named tests `tests` of `program`, each given as
    /// `module.path::name`, and gives the outcome of each in the order
    /// asked. `program` is the surviving source, analysed and accepted.
    ///
    /// An outcome is [`Outcome::Passed`], [`Outcome::FailedRun`] or
    /// [`Outcome::NotRun`]. A test given no outcome is counted as not
    /// run.
    fn execute(&mut self, program: &Program, tests: &[String]) -> Vec<Outcome>;
}

/// Which cases to run (TST-14).
#[derive(Debug, Clone, Default)]
pub struct Selection {
    /// Names of tests, as `name` or `module.path::name`. When empty,
    /// every test is selected. A rejection case owned by a declaration
    /// or an import is always selected.
    pub tests: Vec<String>,
    /// Select no executable case: a rejection-only run (TST-15).
    pub rejections_only: bool,
    /// Select the integration tests too (TST-17): "An integration test
    /// is run only when it is asked for; a run of a module's tests
    /// leaves it out." One that `tests` names is asked for as well.
    pub integration: bool,
}

impl Selection {
    /// Whether a named test is selected (TST-14, TST-17).
    pub fn selects(&self, test: &str, integration: bool) -> bool {
        let named = self.tests.iter().any(|selector| matches(selector, test));
        named || (self.tests.is_empty() && (!integration || self.integration))
    }

    /// Any selectors that name neither an executable nor a rejection test.
    pub fn unknown(&self, names: &[&str]) -> Vec<Problem> {
        self.tests
            .iter()
            .filter(|selector| !names.iter().any(|name| matches(selector, name)))
            .map(|selector| Problem {
                at: None,
                message: format!("no test is named `{selector}` (TST-14)"),
            })
            .collect()
    }
}

fn matches(selector: &str, test: &str) -> bool {
    selector == test || test.rsplit("::").next() == Some(selector)
}

/// Something that stops a run before any case is checked (TST-14):
/// invalid metadata, an annotation nothing can own, an unknown selector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// Where it is, when it is in a source file.
    pub at: Option<Location>,
    /// What is wrong.
    pub message: String,
}

impl Problem {
    /// A problem at a place in a source file.
    pub(crate) fn at(at: Location, message: impl Into<String>) -> Problem {
        Problem {
            at: Some(at),
            message: message.into(),
        }
    }
}

/// The two kinds of case (TST-15).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CaseKind {
    /// A named test, to be run.
    Executable,
    /// A declaration, import or test expected to be rejected.
    Rejection,
}

/// What became of a case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// It passed.
    Passed,
    /// A rejection case whose errors and annotations do not match one to
    /// one (TST-13).
    Failed {
        /// Annotations no error met.
        missing: Vec<Expectation>,
        /// Errors no annotation expects.
        unexpected: Vec<Diagnostic>,
    },
    /// An executable case that ran and did not pass: an assertion did
    /// not hold or an error escaped, with what the target said of it.
    FailedRun(String),
    /// It was selected and could not be run, with the reason.
    NotRun(String),
}

/// One selected case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Case {
    /// What names the case (TST-15): `module.path::name` for a test; the
    /// file, the line the owner starts on and a few words for it
    /// otherwise.
    pub identity: String,
    /// Its kind.
    pub kind: CaseKind,
    /// Whether this case can be selected by name and is an integration test.
    /// Declaration- and import-owned cases are always selected.
    pub selection: CaseSelection,
    /// What became of it.
    pub outcome: Outcome,
}

/// How a case is selected after discovery (TST-14, TST-17).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CaseSelection {
    /// A declaration- or import-owned rejection that always runs.
    Always,
    /// A named executable or rejection test.
    Named {
        /// Whether it must be asked for as an integration test.
        integration: bool,
    },
}

/// The result of a run.
#[derive(Debug, Clone, Default)]
pub struct Report {
    /// What stopped the run. When this or `surviving` is not empty, no
    /// case was checked (TST-14).
    pub problems: Vec<Problem>,
    /// The errors of the surviving source: everything but the owners of
    /// rejection cases. It must be accepted before any case is checked.
    pub surviving: Diagnostics,
    /// The selected cases, rejection cases first, each in source order.
    pub cases: Vec<Case>,
}

impl Report {
    /// True when the run was stopped before any case was checked.
    pub fn prevented(&self) -> bool {
        !self.problems.is_empty() || !self.surviving.is_empty()
    }

    /// How many cases of `kind` have an outcome that `test` accepts.
    pub fn count(&self, kind: CaseKind, test: impl Fn(&Outcome) -> bool) -> usize {
        self.cases
            .iter()
            .filter(|case| case.kind == kind && test(&case.outcome))
            .count()
    }

    /// TST-15: "Success requires every selected executable and required
    /// rejection case to pass." A run that was stopped did not succeed.
    /// A run with no cases succeeds and is evidence of nothing.
    pub fn succeeded(&self) -> bool {
        !self.prevented()
            && self
                .cases
                .iter()
                .all(|case| case.outcome == Outcome::Passed)
    }
}

/// Runs the selected cases of a program.
///
/// `files` are the program's parsed files, with nothing left out, and
/// `front_end` holds what the lexer and parser reported for them.
/// `executor` runs the executable cases; with none, they are listed
/// and not run.
pub fn run(
    sources: &SourceMap,
    files: &[ParsedFile],
    front_end: &Diagnostics,
    selection: &Selection,
    executor: Option<&mut dyn Executor>,
) -> Report {
    let PreparedRun {
        mut report,
        program,
    } = prepare(sources, files, front_end, selection);
    if let (Some(executor), Some(program)) = (executor, program) {
        let chosen: Vec<_> = report
            .cases
            .iter_mut()
            .filter(|case| case.kind == CaseKind::Executable)
            .collect();
        if !chosen.is_empty() {
            let names: Vec<_> = chosen.iter().map(|case| case.identity.clone()).collect();
            let mut outcomes = executor.execute(&program, &names).into_iter();
            for case in chosen {
                case.outcome = outcomes.next().unwrap_or_else(|| {
                    Outcome::NotRun("the target gave it no outcome".to_string())
                });
            }
        }
    }
    report
}

/// A checked test run before its executable cases are handed to a target.
pub struct PreparedRun {
    /// Rejection outcomes and the selected executable cases.
    pub report: Report,
    /// The surviving program, if it was accepted and the selection is valid.
    pub program: Option<Program>,
}

/// Checks the surviving program and its selected rejections, and lists
/// executable cases before running a target.
pub fn prepare(
    sources: &SourceMap,
    files: &[ParsedFile],
    front_end: &Diagnostics,
    selection: &Selection,
) -> PreparedRun {
    prepare_cases(sources, files, front_end, selection, false)
}

/// Validates the requested names and prepares the complete test inventory
/// so a native build can serve subsequent selections.
pub fn prepare_all(
    sources: &SourceMap,
    files: &[ParsedFile],
    front_end: &Diagnostics,
    selection: &Selection,
) -> PreparedRun {
    prepare_cases(sources, files, front_end, selection, true)
}

fn prepare_cases(
    sources: &SourceMap,
    files: &[ParsedFile],
    front_end: &Diagnostics,
    selection: &Selection,
    complete: bool,
) -> PreparedRun {
    let mut report = Report::default();

    // 1. The annotations, and 2. what can own them.
    let mut expectations: Vec<Expectation> = Vec::new();
    let mut candidates: Vec<Owner> = Vec::new();
    for parsed in files {
        let source = sources.file(parsed.file);
        expectations.extend(annotation::read(
            source,
            parsed.file,
            &parsed.root,
            &mut report.problems,
        ));
        candidates.extend(owners(source, parsed.file, &parsed.root));
    }
    let last_line = |file: FileId| sources.file(file).line_count() as usize - 1;

    // Each annotation with the position in `candidates` of its owner.
    let mut owned: Vec<(usize, Expectation)> = Vec::new();
    for expectation in expectations {
        let file = expectation.at.file;
        match owner_of(&candidates, file, expectation.line, last_line(file)) {
            Some(owner) => owned.push((owner, expectation)),
            None => report.problems.push(Problem::at(
                expectation.at,
                "this `expect-error` annotation applies to a line of no import, \
                 declaration or named test (TST-12)",
            )),
        }
    }
    // The candidates that own a case, in source order.
    let mut owning: Vec<usize> = owned.iter().map(|(owner, _)| *owner).collect();
    owning.sort_unstable();
    owning.dedup();

    // The lexer's and parser's errors, each given to the owner it is
    // inside, or to the surviving source.
    let mut front_end_of: Vec<Vec<Diagnostic>> = vec![Vec::new(); candidates.len()];
    let end_of = |file: FileId| sources.file(file).text().len() as u32;
    // The files the end of the file cut something short in: those with
    // an error at their end.
    let cut_short: Vec<FileId> = errors(front_end)
        .map(|diagnostic| diagnostic.primary)
        .filter(|primary| primary.span.start >= end_of(primary.file))
        .map(|primary| primary.file)
        .collect();
    for diagnostic in errors(front_end) {
        let file = diagnostic.primary.file;
        let owner = owner_of_error(
            &candidates,
            file,
            diagnostic.primary.span,
            end_of(file),
            cut_short.contains(&file),
        );
        match owner {
            Some(owner) if owning.contains(&owner) => {
                front_end_of[owner].push(diagnostic.clone());
            }
            _ => report.surviving.emit(diagnostic.clone()),
        }
    }

    // 3. The surviving source: every owner left out (TST-13).
    let without = |kept: Option<usize>| -> Vec<ParsedFile> {
        files
            .iter()
            .map(|parsed| ParsedFile {
                file: parsed.file,
                root: parsed.root.clone(),
                omit: owning
                    .iter()
                    .filter(|&&owner| Some(owner) != kept)
                    .map(|&owner| &candidates[owner])
                    .filter(|owner| owner.file == parsed.file)
                    .map(|owner| owner.span)
                    .collect::<Vec<Span>>(),
            })
            .collect()
    };
    let mut surviving = Diagnostics::new();
    let program = analyse(&without(None), &mut surviving);
    for diagnostic in errors(&surviving) {
        report.surviving.emit(diagnostic.clone());
    }

    // The tests there are to select: those of the surviving source, to
    // run, and those that own a rejection case.
    let tests = || {
        program.modules.iter().flat_map(|module| {
            module
                .tests
                .iter()
                .map(|test| (format!("{}::{}", module.path, test.name), test.integration))
        })
    };
    let executable: Vec<String> = tests().map(|(test, _)| test).collect();
    // TST-17: the integration tests, which a run leaves out unless it
    // is asked for them.
    let integration: Vec<String> = tests()
        .filter(|(_, integration)| *integration)
        .map(|(test, _)| test)
        .collect();
    let identity = |owner: &Owner| match &owner.test {
        Some((module, name)) => format!("{module}::{name}"),
        None => format!(
            "{}:{} {}",
            sources.file(owner.file).name(),
            owner.start_line + 1,
            owner.label
        ),
    };
    let rejected_tests: Vec<String> = owning
        .iter()
        .map(|&owner| &candidates[owner])
        .filter(|owner| owner.test.is_some())
        .map(identity)
        .collect();

    // TST-14: "Unknown selectors reject."
    let names: Vec<_> = executable
        .iter()
        .chain(&rejected_tests)
        .map(String::as_str)
        .collect();
    report.problems.extend(selection.unknown(&names));
    let is_integration = |test: &str| integration.iter().any(|found| found == test);
    let selected = |test: &str| complete || selection.selects(test, is_integration(test));

    // TST-14: "Invalid metadata, ambiguous boundaries, or surviving-source
    // errors prevent execution."
    if report.prevented() {
        report.surviving.sort_by_location();
        return PreparedRun {
            report,
            program: None,
        };
    }

    // 4. Each rejection case: its owner put back, alone (TST-13).
    for &owner in &owning {
        let candidate = &candidates[owner];
        let identity = identity(candidate);
        // TST-14: "Named selectors include rejection tests;
        // declaration-owned cases always run."
        if candidate.test.is_some() && !selected(&identity) {
            continue;
        }
        let mut reported = Diagnostics::new();
        analyse(&without(Some(owner)), &mut reported);
        let mut unexpected: Vec<Diagnostic> = front_end_of[owner].clone();
        unexpected.extend(errors(&reported).cloned());
        let mut missing: Vec<Expectation> = owned
            .iter()
            .filter(|(of, _)| *of == owner)
            .map(|(_, expectation)| expectation.clone())
            .collect();

        // TST-13: "Every primary diagnostic must match one annotation
        // exactly by category, code, file, and next-line location; every
        // annotation must match once."
        unexpected.retain(|diagnostic| {
            let met = missing
                .iter()
                .position(|expectation| meets(sources, diagnostic, expectation));
            match met {
                // The annotation is used up, and the error is expected:
                // `false` drops it from `unexpected`.
                Some(position) => {
                    missing.remove(position);
                    false
                }
                None => true,
            }
        });
        let selection = match candidate.test {
            Some(_) => CaseSelection::Named {
                integration: is_integration(&identity),
            },
            None => CaseSelection::Always,
        };
        report.cases.push(Case {
            identity,
            kind: CaseKind::Rejection,
            selection,
            outcome: if missing.is_empty() && unexpected.is_empty() {
                Outcome::Passed
            } else {
                Outcome::Failed {
                    missing,
                    unexpected,
                }
            },
        });
    }

    // The executable cases will be run by the target where one is named.
    if complete || !selection.rejections_only {
        for test in executable.into_iter().filter(|test| selected(test)) {
            let selection = CaseSelection::Named {
                integration: is_integration(&test),
            };
            report.cases.push(Case {
                identity: test,
                kind: CaseKind::Executable,
                selection,
                outcome: Outcome::NotRun(NO_TARGET.to_string()),
            });
        }
    }
    PreparedRun {
        report,
        program: Some(program),
    }
}

/// The errors among `diagnostics`. A warning does not reject source
/// (ERR-1), so it is no part of a rejection.
fn errors(diagnostics: &Diagnostics) -> impl Iterator<Item = &Diagnostic> {
    diagnostics
        .as_slice()
        .iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
}

/// TST-13: an error meets an annotation when category, code, file and
/// line are the same. The error's line is that of the first character
/// of its primary range; for an error at the end of the file that is
/// the file's last line. A code local to this compiler meets none.
fn meets(sources: &SourceMap, diagnostic: &Diagnostic, expectation: &Expectation) -> bool {
    let file = diagnostic.primary.file;
    diagnostic.code.is_catalogued()
        && diagnostic.category == expectation.category
        && diagnostic.code.as_str() == expectation.code
        && file == expectation.at.file
        && sources.file(file).line_index(diagnostic.primary.span.start) == expectation.line
}
