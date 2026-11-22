//! Builds selected test modules for a native debugger and describes each
//! launch. Selection is the same as an ordinary run (TST-14, TST-17).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use hgl_runner::{CaseKind, Selection};
use hgl_source::SourceMap;
use serde::Serialize;

use super::application::{DebugLaunch, debug_path};
use super::commands::{BuildRequest, build_error};
use super::plan::{Action, CaseOutcome, TestCase};
use super::{BuildSession, Description, Interrupt, sdk, select_tests};
use crate::{EXIT_FAILED, ProgramSources, TestArgs, write_problems};

/// The JSON contract read by IDEs. A launch never starts in this command.
#[derive(Serialize)]
struct Plan {
    version: u8,
    launches: Vec<Launch>,
    skipped: Vec<Skipped>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Launch {
    module: String,
    tests: Vec<String>,
    #[serde(flatten)]
    command: DebugLaunch,
    results_file: String,
}

#[derive(Serialize)]
struct Skipped {
    name: String,
    reason: String,
}

/// A module's selected tests in the order given to its native host.
struct Module<'a> {
    stem: &'a str,
    tests: Vec<(&'a str, Option<Duration>)>,
}

struct Selected<'a> {
    modules: BTreeMap<&'a str, Module<'a>>,
    skipped: Vec<Skipped>,
}

impl<'a> Selected<'a> {
    fn from_cases(cases: &'a [TestCase], selection: &Selection) -> Result<Self, ExitCode> {
        let chosen = select_tests(cases, selection).map_err(|problems| {
            write_problems(&SourceMap::new(), &problems);
            ExitCode::from(EXIT_FAILED)
        })?;
        let mut selected = Self {
            modules: BTreeMap::new(),
            skipped: Vec::new(),
        };
        for case in chosen {
            match &case.action {
                Action::Run(test) => selected
                    .modules
                    .entry(&test.module)
                    .or_insert_with(|| Module {
                        stem: &test.stem,
                        tests: Vec::new(),
                    })
                    .tests
                    .push((&case.identity, test.timeout)),
                Action::Checked(outcome) => {
                    let reason = match (case.kind, outcome) {
                        (CaseKind::Rejection, _) => "rejection case checked by the compiler",
                        (_, CaseOutcome::NotRun(reason) | CaseOutcome::Failed(reason)) => reason,
                        (_, CaseOutcome::Passed) => "the case has no native test executable",
                    };
                    selected.skipped.push(Skipped {
                        name: case.identity.clone(),
                        reason: reason.to_string(),
                    });
                }
            }
        }
        Ok(selected)
    }

    fn stems(&self) -> BTreeSet<&str> {
        self.modules.values().map(|module| module.stem).collect()
    }

    fn plan(self, launches: Vec<Launch>) -> Plan {
        Plan {
            version: 1,
            launches,
            skipped: self.skipped,
        }
    }

    fn describe(self, session: &BuildSession, description: &Description) -> Result<Plan, String> {
        let directory = session.workspace.directory.join("debug-results");
        fs::create_dir_all(&directory)
            .map_err(|error| format!("{} could not be made: {error}", directory.display()))?;
        let libraries = sdk::library_path(description);
        let launches = self
            .modules
            .iter()
            .map(|(&name, module)| {
                let results = debug_path(&directory.join(format!("{}.txt", module.stem)))?;
                let mut arguments = vec![results.clone()];
                // TST-17: the native host accepts a per-test clock limit,
                // so it can keep it even when tests share one launch.
                arguments.extend(module.tests.iter().map(|&(test, limit)| match limit {
                    Some(limit) => format!("{test}@{}", limit.as_micros()),
                    None => test.to_string(),
                }));
                Ok(Launch {
                    module: name.to_string(),
                    tests: module.tests.iter().map(|&(test, _)| test.into()).collect(),
                    command: DebugLaunch::new(
                        &session.workspace.test_executable(module.stem),
                        arguments,
                        &libraries,
                        session.toolchain.hgraph(),
                    )?,
                    results_file: results,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(self.plan(launches))
    }
}

/// Prints a v1 launch plan on success. Native build messages go to stderr.
pub(super) fn run(
    arguments: &TestArgs,
    directory: &Path,
    selection: &Selection,
    paths: Option<&[PathBuf]>,
) -> ExitCode {
    let plan = match prepare(arguments, directory, selection, paths) {
        Ok(plan) => plan,
        Err(status) => return status,
    };
    match serde_json::to_string(&plan) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(error) => build_error(error.to_string()),
    }
}

fn prepare(
    arguments: &TestArgs,
    directory: &Path,
    selection: &Selection,
    paths: Option<&[PathBuf]>,
) -> Result<Plan, ExitCode> {
    let interrupt = Interrupt::watch(None).map_err(build_error)?;
    let request = BuildRequest::debug_tests(arguments, directory);
    let inputs = match paths {
        Some(paths) => ProgramSources::read_paths(paths)?,
        None => ProgramSources::read(&arguments.roots)?,
    };
    // A warm call can validate selectors from the complete stamped inventory.
    // A new directory is opened only after validation finds work to build.
    let existing = directory.is_dir().then(|| request.keyed_session(&inputs));
    if let Some(Ok((session, key))) = &existing
        && let Some(previous) = &session.previous
        && let Some(cases) = previous.test_inventory(key)
    {
        let selected = Selected::from_cases(cases, selection)?;
        if selected.modules.is_empty() {
            interrupt.finish().map_err(build_error)?;
            return Ok(selected.plan(Vec::new()));
        }
        if previous.tests_reusable(&session.workspace, key, &selected.stems())
            && let Some(description) = session
                .toolchain
                .prepared(&session.cache, &session.toolchain_key)
        {
            interrupt.finish().map_err(build_error)?;
            return selected
                .describe(session, &description)
                .map_err(build_error);
        }
    }

    let (output, program, _) = request.prepare(&inputs)?;
    let selected = Selected::from_cases(program.tests.as_deref().unwrap_or_default(), selection)?;
    if selected.modules.is_empty() {
        interrupt.finish().map_err(build_error)?;
        return Ok(selected.plan(Vec::new()));
    }
    let (session, key) = existing
        .unwrap_or_else(|| request.keyed_session(&inputs))
        .map_err(build_error)?;
    session.begin().map_err(build_error)?;
    let description = session
        .finish_native(&key, &output, &program, &interrupt, Some(&selected.stems()))
        .map_err(build_error)?;
    selected
        .describe(&session, &description)
        .map_err(build_error)
}
