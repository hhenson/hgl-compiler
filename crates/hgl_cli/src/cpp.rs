//! `--target cpp`: the cpp target in `hgl emit`, `hgl test` and
//! `hgl run`, and its build driver.
//!
//! `hgl_cpp` writes a program as C++; this compiles it and runs it. For
//! `hgl test` the driver
//!
//! 1. writes the program and lists each test the target could not
//!    write all of as not run, with the first form it reaches that is
//!    not written, as the hgraph target's tests are listed;
//! 2. finds the toolchain (`sdk`): hgraph's C++ SDK and its
//!    dependencies through `HGL_HGRAPH_SDK` and `HGL_HGRAPH_DEPS`, the
//!    compiler, `ninja`; where one is missing every test that could run
//!    is not run, with what is missing;
//! 3. prepares the toolchain once and keeps it (`sdk`): the runtime
//!    library, the description of a compile and a link, and the
//!    precompiled header;
//! 4. writes the program and its `build.ninja` into the program's
//!    workspace and runs `ninja` (`build`);
//! 5. runs each executable built, one for each module, on a plan of the
//!    module's tests to run, and reads what became of each from the
//!    results file it writes: the protocol of the hgraph target's
//!    driver, read by the same code (`hgl_hgraph::read_results`).

mod build;
mod sdk;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use hgl_analyser::Program;
use hgl_cpp::Output;
use hgl_diagnostics::{Diagnostics, Location};
use hgl_hgraph::Said;
use hgl_runner::{Executor, Outcome};
use hgl_source::SourceMap;

use super::EXIT_FAILED;
use build::{Workspace, write_if_changed};
use sdk::Toolchain;

/// Writes `files` under `directory`, each where it differs from what is
/// there: `hgl emit --target cpp`.
pub(super) fn write(files: &[hgl_cpp::File], directory: &Path) -> Result<(), String> {
    for file in files {
        let path = file
            .path
            .split('/')
            .fold(directory.to_path_buf(), |path, part| path.join(part));
        write_if_changed(&path, &file.text)?;
    }
    Ok(())
}

/// `hgl test --target cpp`: what builds and runs the executable cases
/// of a program as C++.
pub(super) struct OnCpp<'a> {
    /// The program's files, to say where a failure is.
    pub(super) sources: &'a SourceMap,
    /// The directory to build in, where `--keep` names one.
    pub(super) keep: Option<&'a Path>,
    /// The program's source roots, which name its workspace.
    pub(super) roots: &'a [PathBuf],
}

impl Executor for OnCpp<'_> {
    fn execute(&mut self, program: &Program, tests: &[String]) -> Vec<Outcome> {
        // What the target does not write yet is no error of the
        // program: it is why a test that needs it is not run.
        let mut unwritten = Diagnostics::new();
        let output = hgl_cpp::transpile(program, &mut unwritten);
        let place = |at: Location| {
            let source = self.sources.file(at.file);
            format!("{}:{}", source.name(), source.line_col(at.span.start))
        };
        let found: Vec<Option<&hgl_cpp::Test>> = tests
            .iter()
            .map(|identity| {
                output
                    .tests
                    .iter()
                    .find(|test| test.identity() == *identity)
            })
            .collect();
        let mut outcomes: Vec<Option<Outcome>> = found
            .iter()
            .map(|test| match test {
                None => Some(Outcome::NotRun("the target wrote no such test".to_string())),
                Some(test) => test.blocked.as_ref().map(|diagnostic| {
                    Outcome::NotRun(format!(
                        "{}: {}",
                        place(diagnostic.primary),
                        diagnostic.message
                    ))
                }),
            })
            .collect();
        // The tests to run, each as its position in `tests`, the test
        // and its executable.
        let runnable: Vec<Runnable> = found
            .iter()
            .enumerate()
            .filter_map(|(position, test)| {
                let test = (*test)?;
                Some((position, test, test.executable?))
            })
            .collect();
        if !runnable.is_empty() {
            let ran = self.run(&output, &runnable);
            for ((position, _, _), outcome) in runnable.iter().zip(ran) {
                outcomes[*position] = Some(outcome);
            }
        } else if let Some(directory) = self.keep {
            // A directory that is kept holds the program whether or not a
            // test is run, as the hgraph target's does; with no test to
            // say it of, a failure to write it is not reported.
            let _ = write(&output.files, directory);
        }
        outcomes
            .into_iter()
            .map(|outcome| {
                outcome
                    .unwrap_or_else(|| Outcome::NotRun("the target gave it no outcome".to_string()))
            })
            .collect()
    }
}

/// A test to run: its position among the tests asked for, the test, and
/// its executable's position in `Output::executables`.
type Runnable<'o> = (usize, &'o hgl_cpp::Test, usize);

impl OnCpp<'_> {
    /// Builds the executables of the tests `runnable` and runs them:
    /// what became of each test, in that order.
    fn run(&self, output: &Output, runnable: &[Runnable]) -> Vec<Outcome> {
        let not_run = |reason: String| vec![Outcome::NotRun(reason); runnable.len()];
        let toolchain = match Toolchain::find() {
            Ok(toolchain) => toolchain,
            Err(reason) => return not_run(reason),
        };
        let cache = match build::cache() {
            Ok(cache) => cache,
            Err(reason) => return not_run(reason),
        };
        let jobs = std::env::var(build::JOBS)
            .ok()
            .filter(|jobs| !jobs.is_empty());
        let description = match toolchain.prepare(&cache, jobs.as_deref()) {
            Ok(description) => description,
            Err(reason) => return not_run(reason),
        };
        // The workspace is held from here until the last executable has
        // ended.
        let workspace = match Workspace::open(self.keep, self.roots, &cache) {
            Ok(workspace) => workspace,
            Err(reason) => return not_run(reason),
        };
        if let Err(reason) = workspace.write(output, &description) {
            return not_run(reason);
        }
        let mut wanted: Vec<usize> = runnable
            .iter()
            .map(|(_, _, executable)| *executable)
            .collect();
        wanted.sort_unstable();
        wanted.dedup();
        let built = match workspace.build(&toolchain.ninja, output, &wanted, jobs.as_deref()) {
            Ok(built) => built,
            Err(reason) => return not_run(reason),
        };
        // Each executable built runs its tests, and those of one that is
        // not are not run: an older one is never run in its place.
        let launches: Vec<Launch> = wanted
            .iter()
            .zip(&built.fresh)
            .filter(|(_, fresh)| **fresh)
            .map(|(&executable, _)| Launch {
                executable,
                tests: runnable
                    .iter()
                    .enumerate()
                    .filter(|(_, (_, _, of))| *of == executable)
                    .map(|(slot, (_, test, _))| (slot, *test))
                    .collect(),
            })
            .collect();
        let mut outcomes = vec![None; runnable.len()];
        for (slot, outcome) in
            launch_all(&workspace, output, &launches, self.sources, jobs.as_deref())
        {
            outcomes[slot] = Some(outcome);
        }
        let failure = built.failure.unwrap_or_default();
        outcomes
            .into_iter()
            .map(|outcome| {
                outcome.unwrap_or_else(|| {
                    Outcome::NotRun(format!("the build of its executable failed:\n{failure}"))
                })
            })
            .collect()
    }
}

/// One executable to run, and the tests it is to run: each as a
/// position among the tests asked to run, and the test.
struct Launch<'o> {
    /// Its position in `Output::executables`.
    executable: usize,
    /// The tests.
    tests: Vec<(usize, &'o hgl_cpp::Test)>,
}

/// Runs each of `launches` in `workspace`, as many at once as `jobs`
/// says or the machine has cores, and gives what became of each test,
/// by its position among those asked to run.
fn launch_all(
    workspace: &Workspace,
    output: &Output,
    launches: &[Launch],
    sources: &SourceMap,
    jobs: Option<&str>,
) -> Vec<(usize, Outcome)> {
    let threads = jobs
        .and_then(|jobs| jobs.parse::<usize>().ok())
        .or_else(|| std::thread::available_parallelism().ok().map(usize::from))
        .unwrap_or(1)
        .clamp(1, launches.len().max(1));
    let next = AtomicUsize::new(0);
    // Each thread takes the next launch until none is left; a scope's
    // threads are joined before it ends.
    let ran: Vec<Vec<(usize, Outcome)>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut ran = Vec::new();
                    loop {
                        let taken = next.fetch_add(1, Ordering::Relaxed);
                        let Some(launch) = launches.get(taken) else {
                            break;
                        };
                        ran.extend(launch_one(workspace, output, launch, sources));
                    }
                    ran
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| {
                handle
                    .join()
                    .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
            })
            .collect()
    });
    ran.into_iter().flatten().collect()
}

/// Runs one executable on a plan of its tests, and reads what became of
/// each from the results it writes.
fn launch_one(
    workspace: &Workspace,
    output: &Output,
    launch: &Launch,
    sources: &SourceMap,
) -> Vec<(usize, Outcome)> {
    let module = &output.executables[launch.executable].module;
    let all = |reason: String| -> Vec<(usize, Outcome)> {
        launch
            .tests
            .iter()
            .map(|(slot, _)| (*slot, Outcome::NotRun(reason.clone())))
            .collect()
    };
    let plan = workspace
        .directory
        .join("plans")
        .join(format!("{module}.txt"));
    let results = workspace
        .directory
        .join("results")
        .join(format!("{module}.txt"));
    // One test on each line: its identity and its time limit in seconds,
    // `-` for none (TST-17).
    let planned: String = launch
        .tests
        .iter()
        .map(|(_, test)| {
            let limit =
                limit(test).map_or("-".to_string(), |limit| limit.as_secs_f64().to_string());
            format!("{} {limit}\n", test.identity())
        })
        .collect();
    let written = write_if_changed(&plan, &planned).and_then(|()| {
        // What an earlier run recorded is not this run's.
        match fs::remove_file(&results) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(format!(
                "{} could not be removed: {error}",
                results.display()
            )),
            _ => fs::create_dir_all(results.parent().expect("under the workspace"))
                .map_err(|error| format!("{} could not be made: {error}", results.display())),
        }
    });
    if let Err(reason) = written {
        return all(reason);
    }
    let executable = workspace.executable(module);
    let finished = match Command::new(&executable).arg(&plan).arg(&results).output() {
        Ok(finished) => finished,
        Err(error) => {
            return all(format!(
                "{} could not be started: {error}",
                executable.display()
            ));
        }
    };
    let printed = String::from_utf8_lossy(&finished.stderr);
    let last = printed
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("it printed no error");
    let stopped = format!("the executable stopped ({}): {last}", finished.status);
    let recorded = fs::read_to_string(&results).unwrap_or_default();
    let place = |at: Location| {
        let source = sources.file(at.file);
        format!("{}:{}", source.name(), source.line_col(at.span.start))
    };
    hgl_hgraph::read_results(&recorded, launch.tests.len())
        .into_iter()
        .zip(&launch.tests)
        .map(|(recorded, (slot, test))| {
            let outcome = match recorded.said {
                Some(Said::Passed) => Outcome::Passed,
                Some(Said::Failed { line, message }) => {
                    match line.and_then(|line| test.statement_at(line)) {
                        Some(at) => Outcome::FailedRun(format!("{}: {message}", place(at))),
                        None => Outcome::FailedRun(message),
                    }
                }
                Some(Said::Unloaded(reason)) => {
                    Outcome::NotRun(format!("its executable could not run it: {reason}"))
                }
                None => {
                    let when = if recorded.started {
                        "while running"
                    } else {
                        "before"
                    };
                    Outcome::NotRun(format!("{stopped} {when} this test"))
                }
            };
            (*slot, outcome)
        })
        .collect()
}

/// How long `test` may run, by the host clock, before it is stopped
/// and fails: TST-17's limit for an integration test, the one
/// `@timeout` writes or one minute, and none for any other. The minute
/// is the hgraph target's constant until the shared crate holds it.
fn limit(test: &hgl_cpp::Test) -> Option<Duration> {
    test.integration
        .then(|| test.timeout.unwrap_or(hgl_hgraph::INTEGRATION_LIMIT))
}

/// `hgl run --target cpp` of application `chosen` of `program`: no
/// application is written for this target yet, so none is run, and
/// what stops it is said.
pub(super) fn run(sources: &SourceMap, program: &Program, chosen: &str) -> ExitCode {
    let mut unwritten = Diagnostics::new();
    let output = hgl_cpp::transpile(program, &mut unwritten);
    let name = hgl_cpp::TARGET;
    let Some(application) = output
        .applications
        .iter()
        .find(|application| application.path == chosen)
    else {
        eprintln!("hgl: the target wrote no application `{chosen}`");
        return ExitCode::from(EXIT_FAILED);
    };
    match &application.blocked {
        Some(diagnostic) => {
            let source = sources.file(diagnostic.primary.file);
            eprintln!(
                "{}:{}: {}",
                source.name(),
                source.line_col(diagnostic.primary.span.start),
                diagnostic.message
            );
            println!(
                "not run: the {name} target does not write all that {} needs yet",
                application.path
            );
        }
        None => println!("not run: the {name} target writes no entry for an application yet"),
    }
    ExitCode::from(EXIT_FAILED)
}
