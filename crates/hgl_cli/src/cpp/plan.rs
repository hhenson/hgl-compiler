//! The metadata a built program needs to run. It contains source locations
//! and checked rejection outcomes so reuse needs no parsed program.

use std::fmt::Write as _;
use std::time::Duration;

use hgl_cpp::Output;
use hgl_diagnostics::Location;
use hgl_runner::{Case, CaseKind, CaseSelection, Outcome, Report, Selection};
use hgl_source::SourceMap;
use serde::{Deserialize, Serialize};

/// The units and executable entries of a successful build.
#[derive(Deserialize, Serialize)]
pub(super) struct Program {
    pub(super) units: Vec<String>,
    pub(super) applications: Vec<Application>,
    pub(super) tests: Option<Vec<TestCase>>,
}

/// A program's named application and what it needs from its host (RUN-3).
#[derive(Deserialize, Serialize)]
pub(super) struct Application {
    pub(super) path: String,
    pub(super) real_time: bool,
    pub(super) blocked: Option<String>,
}

/// A checked rejection, a blocked test, or an executable case (TST-15).
#[derive(Deserialize, Serialize)]
pub(super) struct TestCase {
    pub(super) identity: String,
    /// The written source name, separate from the identity printed to users.
    /// Older stamps can still supply their artifact inventory.
    #[serde(default)]
    pub(super) file: String,
    pub(super) kind: CaseKind,
    pub(super) selection: CaseSelection,
    pub(super) action: Action,
}

#[derive(Deserialize, Serialize)]
pub(super) enum Action {
    Checked(CaseOutcome),
    Run(NativeTest),
}

/// The host entry, its per-test timeout and its generated source map.
#[derive(Deserialize, Serialize)]
pub(super) struct NativeTest {
    /// The containing module, also the executable's module. A stamp
    /// without this field can still provide old artifact digests.
    #[serde(default)]
    pub(super) module: String,
    pub(super) stem: String,
    pub(super) timeout: Option<Duration>,
    pub(super) statements: Vec<(usize, String)>,
}

/// The case outcome and the details the CLI prints under its row.
#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) enum CaseOutcome {
    Passed,
    Failed(String),
    NotRun(String),
}

impl CaseOutcome {
    pub(crate) fn from_runner(outcome: &Outcome, sources: &SourceMap) -> Self {
        match outcome {
            Outcome::Passed => Self::Passed,
            Outcome::FailedRun(reason) => Self::Failed(reason.clone()),
            Outcome::NotRun(reason) => Self::NotRun(reason.clone()),
            Outcome::Failed {
                missing,
                unexpected,
            } => {
                let mut details = String::new();
                for expectation in missing {
                    let _ = writeln!(
                        details,
                        "expected {}/{} on line {} of {}: not reported",
                        expectation.category.as_str(),
                        expectation.code,
                        expectation.line + 1,
                        sources.file(expectation.at.file).name()
                    );
                }
                for diagnostic in unexpected {
                    let local = if diagnostic.code.is_catalogued() {
                        ""
                    } else {
                        " (a compiler-local code, which no annotation can expect)"
                    };
                    let _ = writeln!(
                        details,
                        "reported {}/{} at {}, not expected{local}: {}",
                        diagnostic.category.as_str(),
                        diagnostic.code.as_str(),
                        place(sources, diagnostic.primary),
                        diagnostic.message
                    );
                }
                Self::Failed(details)
            }
        }
    }
}

impl Program {
    pub(super) fn new(output: &Output, report: Option<&Report>, sources: &SourceMap) -> Self {
        Self {
            units: output
                .units
                .iter()
                .map(|unit| unit.module.clone())
                .collect(),
            applications: output
                .applications
                .iter()
                .map(|app| Application {
                    path: app.path.clone(),
                    real_time: app.real_time,
                    blocked: app.blocked.as_ref().map(|diagnostic| {
                        format!(
                            "{}: {}",
                            place(sources, diagnostic.primary),
                            diagnostic.message
                        )
                    }),
                })
                .collect(),
            tests: report.map(|report| {
                report
                    .cases
                    .iter()
                    .map(|case| TestCase::new(case, output, sources))
                    .collect()
            }),
        }
    }
}

impl TestCase {
    fn new(case: &Case, output: &Output, sources: &SourceMap) -> Self {
        let action = match case.kind {
            CaseKind::Rejection => {
                Action::Checked(CaseOutcome::from_runner(&case.outcome, sources))
            }
            CaseKind::Executable => match output
                .tests
                .iter()
                .find(|test| test.identity() == case.identity)
            {
                Some(test) => match test.executable {
                    Some(index) => Action::Run(NativeTest {
                        module: output.executables[index].module.clone(),
                        stem: output.executables[index].stem.clone(),
                        timeout: test.limit(),
                        statements: test
                            .statements()
                            .iter()
                            .map(|(line, at)| (*line, place(sources, *at)))
                            .collect(),
                    }),
                    None => {
                        Action::Checked(CaseOutcome::NotRun(test.blocked.as_ref().map_or_else(
                            || "the target gave it no outcome".to_string(),
                            |diagnostic| {
                                format!(
                                    "{}: {}",
                                    place(sources, diagnostic.primary),
                                    diagnostic.message
                                )
                            },
                        )))
                    }
                },
                None => Action::Checked(CaseOutcome::NotRun(
                    "the target wrote no such test".to_string(),
                )),
            },
        };
        Self {
            identity: case.identity.clone(),
            file: sources.file(case.file).name().to_string(),
            kind: case.kind,
            selection: case.selection,
            action,
        }
    }

    pub(super) fn selected(&self, selection: &Selection) -> bool {
        selection.selects_case(&self.identity, self.kind, self.selection, &self.file)
    }
}

fn place(sources: &SourceMap, at: Location) -> String {
    let source = sources.file(at.file);
    format!("{}:{}", source.name(), source.line_col(at.span.start))
}
