//! The C++ build driver for `hgl build`, `hgl test` and `hgl run`.
//! It prepares the SDK toolchain once, builds the emitted sources in a
//! locked workspace, and records the successful artifacts by their input
//! content. A matching build can launch its application or its selected
//! tests without the frontend or Ninja.

mod application;
mod build;
mod commands;
mod debug_tests;
mod plan;
mod results;
mod sdk;
mod stamp;

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use hgl_cpp::Output;
use hgl_diagnostics::{Diagnostics, Location};
use hgl_runner::{CaseSelection, Selection};
use hgl_source::SourceMap;

use super::{LimitArgs, ProgramSources, TestArgs};
use build::{Interrupt, Workspace, write_if_changed};
use plan::{Action, NativeTest, TestCase};
use results::{Said, read_results};

use sdk::{Description, Toolchain};
use stamp::{PreviousBuild, Stamp};

#[cfg(target_os = "linux")]
pub(super) use build::native_build_child;
pub(super) use commands::{build, run};
pub(super) use plan::CaseOutcome;
pub(super) use stamp::Replacements;

/// Workspace directory for embedded sources named by debugger line tables.
const DEBUG_SOURCES: &str = "debug-sources";

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

/// The locked workspace and its selected native toolchain.
pub(super) struct BuildSession {
    workspace: Workspace,
    previous: Option<PreviousBuild>,
    toolchain: Toolchain,
    toolchain_key: String,
    cache: PathBuf,
    debug: bool,
    jobs: Option<String>,
}

/// The run information belonging to a successful program build.
pub(super) struct BuiltProgram {
    program: plan::Program,
    description: Description,
    reused: bool,
}

impl BuildSession {
    fn open(
        roots: &[PathBuf],
        keep: Option<&Path>,
        debug: bool,
        interrupt: &Interrupt,
    ) -> Result<Self, String> {
        let cache = build::cache()?;
        let workspace = Workspace::open(keep, roots, &cache, debug)?;
        let previous = PreviousBuild::read(&workspace);
        let found = if debug {
            Toolchain::find_debug(interrupt)
        } else {
            Toolchain::find(interrupt)
        };
        interrupt.check()?;
        let toolchain = match found {
            Ok(toolchain) => toolchain,
            Err(reason) => {
                PreviousBuild::invalidate(&workspace, previous.as_ref())?;
                return Err(reason);
            }
        };
        if debug {
            // Said before the build, which may fail. Standard output is
            // left to what a debugger's integration reads.
            eprintln!("hgl: {}", toolchain.hgraph().said());
        }
        let toolchain_key = toolchain.key();
        let jobs = std::env::var(build::JOBS)
            .ok()
            .filter(|jobs| !jobs.is_empty());
        Ok(Self {
            workspace,
            previous,
            toolchain,
            toolchain_key,
            cache,
            debug,
            jobs,
        })
    }

    pub(super) fn key(
        &self,
        sources: &ProgramSources,
        limits: LimitArgs,
        replacements: &Replacements<'_>,
    ) -> Result<String, String> {
        let key = stamp::key(
            sources,
            limits,
            &self.toolchain_key,
            &self.cache,
            self.debug,
            replacements,
        );
        if key.is_err() {
            self.begin()?;
        }
        key
    }

    fn reused(&mut self, key: &str, tests: bool) -> Option<BuiltProgram> {
        if !self
            .previous
            .as_ref()?
            .reusable(&self.workspace, key, tests)
        {
            return None;
        }
        let description = self.toolchain.prepared(&self.cache, &self.toolchain_key)?;
        Some(BuiltProgram {
            program: self.previous.take()?.stamp.program,
            description,
            reused: true,
        })
    }

    /// Withdraws the runnable stamp before checking or writing a new build.
    pub(super) fn begin(&self) -> Result<(), String> {
        PreviousBuild::invalidate(&self.workspace, self.previous.as_ref())
    }

    /// Exports compile commands from the manifest of this locked workspace.
    fn compile_commands(&self) -> Result<PathBuf, String> {
        self.workspace.compile_commands(self.toolchain.ninja()?)
    }

    /// Writes the program and publishes its stamp after Ninja succeeds.
    fn finish_build(
        &self,
        key: &str,
        output: &Output,
        program: &plan::Program,
        interrupt: &Interrupt,
    ) -> Result<Description, String> {
        self.finish_native(key, output, program, interrupt, None)
    }

    fn finish_native(
        &self,
        key: &str,
        output: &Output,
        program: &plan::Program,
        interrupt: &Interrupt,
        debug_tests: Option<&BTreeSet<&str>>,
    ) -> Result<Description, String> {
        interrupt.check()?;
        let ninja = self.toolchain.ninja()?;
        let description = self.toolchain.prepare(
            &self.cache,
            &self.toolchain_key,
            self.jobs.as_deref(),
            interrupt,
        );
        interrupt.check()?;
        let description = description?;
        self.workspace
            .write(output, &description, program.tests.is_some())?;
        let previous = self.previous.as_ref().map(|previous| &previous.stamp);
        let jobs = self.jobs.as_deref();
        let artifacts = match debug_tests {
            Some(stems) => self
                .workspace
                .build_debug_tests(ninja, output, stems, jobs, previous, interrupt),
            None => self.workspace.build_program(
                ninja,
                output,
                program.tests.is_some(),
                jobs,
                previous,
                interrupt,
            ),
        };
        interrupt.check()?;
        let mut artifacts = artifacts?;
        // A reusable debug launch needs the sources from its line tables
        // as well as the executable.
        artifacts.extend(
            output
                .files
                .iter()
                .filter(|file| Path::new(&file.path).starts_with(DEBUG_SOURCES))
                .map(|file| file.path.clone()),
        );
        if self.toolchain.key() != self.toolchain_key {
            return Err("the C++ toolchain changed during the build".to_string());
        }
        let retained = debug_tests.and(previous);
        let stamp = Stamp::describe(
            &self.workspace,
            key,
            artifacts,
            program,
            debug_tests.is_some(),
            retained,
        )?;
        interrupt.finish()?;
        Stamp::save(&self.workspace, &stamp)?;
        Ok(description)
    }
}

/// Builds the executable cases on a miss and runs the selected cases.
pub(super) fn test(
    arguments: &TestArgs,
    selection: &Selection,
    paths: Option<&[PathBuf]>,
) -> ExitCode {
    if let Some(directory) = arguments.debug_build.as_deref() {
        return debug_tests::run(arguments, directory, selection, paths);
    }
    let interrupt = match Interrupt::watch(None) {
        Ok(interrupt) => interrupt,
        Err(reason) => return commands::build_error(reason),
    };
    let (inputs, session) = match commands::BuildRequest::tests(arguments).start(paths, &interrupt)
    {
        Ok(commands::BuildStart::Hit(session, built)) => {
            let cases = built.program.tests.as_deref().unwrap_or_default();
            let ready = interrupt.finish();
            let run = ready
                .as_ref()
                .map(|()| (&session, &built.description))
                .map_err(String::as_str);
            return show_tests(cases, selection, run);
        }
        Ok(commands::BuildStart::Miss(inputs, session)) => (inputs, session),
        Err(status) => return status,
    };
    let super::Parsed {
        sources,
        files,
        diagnostics,
        ..
    } = match super::parse_sources(&inputs, arguments.limits.limits(), None, true) {
        Ok(parsed) => parsed,
        Err(status) => return status,
    };
    let prepared = hgl_runner::prepare_all(&sources, &files, &diagnostics, selection);
    let Some(program) = prepared.program else {
        return super::print_report(&sources, &prepared.report);
    };
    let mut unwritten = Diagnostics::new();
    let file_name = |at: Location| sources.file(at.file).name().to_string();
    let output = hgl_cpp::transpile(&program, &file_name, &mut unwritten);
    let program = plan::Program::new(&output, Some(&prepared.report), &sources);
    let cases = program.tests.as_deref().unwrap_or_default();
    let chosen = match select_tests(cases, selection) {
        Ok(chosen) => chosen,
        Err(problems) => return super::print_problems(&sources, &problems),
    };
    if chosen.is_empty() {
        return super::print_empty_test_selection(selection.source_files.is_some());
    }
    if !chosen
        .iter()
        .any(|case| matches!(case.action, Action::Run(_)))
    {
        if arguments.keep.is_some()
            && !selection.rejections_only
            && let Ok((session, _)) = &session
            && interrupt.check().is_ok()
        {
            let _ = write(&output.files, &session.workspace.directory);
        }
        return print_cases(&chosen, Err("the target gave it no outcome"));
    }
    match session {
        Ok((session, key)) => match session.finish_build(&key, &output, &program, &interrupt) {
            Ok(description) => print_cases(&chosen, Ok((&session, &description))),
            Err(reason) => print_cases(&chosen, Err(&reason)),
        },
        Err(reason) => print_cases(&chosen, Err(&reason)),
    }
}

fn select_tests<'a>(
    cases: &'a [TestCase],
    selection: &Selection,
) -> Result<Vec<&'a TestCase>, Vec<hgl_runner::Problem>> {
    let names: Vec<_> = cases
        .iter()
        .filter(|case| matches!(case.selection, CaseSelection::Named { .. }))
        .map(|case| case.identity.as_str())
        .collect();
    let problems = selection.unknown(&names);
    if problems.is_empty() {
        Ok(cases
            .iter()
            .filter(|case| case.selected(selection))
            .collect())
    } else {
        Err(problems)
    }
}

fn show_tests(
    cases: &[TestCase],
    selection: &Selection,
    run: Result<(&BuildSession, &Description), &str>,
) -> ExitCode {
    match select_tests(cases, selection) {
        Ok(chosen) if chosen.is_empty() => {
            super::print_empty_test_selection(selection.source_files.is_some())
        }
        Ok(chosen) => print_cases(&chosen, run),
        Err(problems) => super::print_problems(&SourceMap::new(), &problems),
    }
}

/// Reports checked or unsupported cases and runs the built ones by host.
fn print_cases(cases: &[&TestCase], run: Result<(&BuildSession, &Description), &str>) -> ExitCode {
    let mut outcomes: Vec<_> = cases
        .iter()
        .map(|case| match &case.action {
            Action::Checked(outcome) => outcome.clone(),
            Action::Run(_) => CaseOutcome::NotRun(
                run.as_ref()
                    .err()
                    .copied()
                    .unwrap_or("the target gave it no outcome")
                    .to_string(),
            ),
        })
        .collect();
    if let Ok((session, description)) = run {
        let mut shared = BTreeMap::new();
        let mut launches = Vec::new();
        for (slot, case) in cases.iter().enumerate() {
            if let Action::Run(test) = &case.action {
                let wanted = (slot, case.identity.as_str(), test);
                match test.timeout {
                    Some(limit) => launches.push(Launch {
                        stem: &test.stem,
                        tests: vec![wanted],
                        limit: Some(limit),
                    }),
                    None => shared
                        .entry(test.stem.as_str())
                        .or_insert_with(Vec::new)
                        .push(wanted),
                }
            }
        }
        launches.extend(shared.into_iter().map(|(stem, tests)| Launch {
            stem,
            tests,
            limit: None,
        }));
        let libraries = sdk::library_path(description);
        for (slot, outcome) in launch_all(
            &session.workspace,
            &launches,
            libraries.as_ref(),
            session.jobs.as_deref(),
        ) {
            outcomes[slot] = outcome;
        }
    }
    let outcomes: Vec<_> = cases
        .iter()
        .zip(outcomes)
        .map(|(case, outcome)| (case.identity.as_str(), case.kind, outcome))
        .collect();
    super::print_outcomes(&outcomes)
}

/// One host run and the cases supplied to it.
struct Launch<'o> {
    stem: &'o str,
    tests: Vec<(usize, &'o str, &'o NativeTest)>,
    /// A time-limited case runs by itself (TST-17).
    limit: Option<Duration>,
}

/// Runs each of `launches` in `workspace`, as many at once as `jobs`
/// says or the machine has cores, each with `libraries`, a variable and
/// its value, in its environment where there is one
/// (`sdk::library_path`), and gives what became of each test, by its
/// position among those asked to run.
fn launch_all(
    workspace: &Workspace,
    launches: &[Launch],
    libraries: Option<&(&'static str, OsString)>,
    jobs: Option<&str>,
) -> Vec<(usize, CaseOutcome)> {
    let threads = jobs
        .and_then(|jobs| jobs.parse::<usize>().ok())
        .or_else(|| std::thread::available_parallelism().ok().map(usize::from))
        .unwrap_or(1)
        .clamp(1, launches.len().max(1));
    let next = AtomicUsize::new(0);
    // Each thread takes the next launch until none is left; a scope's
    // threads are joined before it ends.
    let ran: Vec<Vec<(usize, CaseOutcome)>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut ran = Vec::new();
                    loop {
                        let taken = next.fetch_add(1, Ordering::Relaxed);
                        let Some(launch) = launches.get(taken) else {
                            break;
                        };
                        ran.extend(launch_one(workspace, launch, taken, libraries));
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

/// Runs one executable, the `position`th of the runs, on its tests, with
/// `libraries` in its environment where there is one, and reads what
/// became of each from the results it writes.
fn launch_one(
    workspace: &Workspace,
    launch: &Launch,
    position: usize,
    libraries: Option<&(&'static str, OsString)>,
) -> Vec<(usize, CaseOutcome)> {
    let stem = launch.stem;
    let all = |reason: String| -> Vec<(usize, CaseOutcome)> {
        launch
            .tests
            .iter()
            .map(|(slot, _, _)| (*slot, CaseOutcome::NotRun(reason.clone())))
            .collect()
    };
    // A results file for each run, by its position among them.
    let results = workspace
        .directory
        .join("results")
        .join(format!("{stem}.{position}.txt"));
    // What an earlier run recorded is not this run's.
    let cleared = match fs::remove_file(&results) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(format!(
            "{} could not be removed: {error}",
            results.display()
        )),
        _ => fs::create_dir_all(workspace.directory.join("results"))
            .map_err(|error| format!("{} could not be made: {error}", results.display())),
    };
    if let Err(reason) = cleared {
        return all(reason);
    }
    // `EXECUTABLE RESULTS NAME...`: the runtime library's test host
    // (`hglrt/testing.h`).
    let executable = workspace.test_executable(stem);
    let mut command = Command::new(&executable);
    if let Some((variable, value)) = libraries {
        command.env(variable, value);
    }
    command
        .arg(&results)
        .args(launch.tests.iter().map(|(_, identity, _)| identity));
    let (finished, timed_out) = match finish(&mut command, launch.limit) {
        Ok(finished) => finished,
        Err(error) => {
            return all(format!(
                "{} could not be run: {error}",
                executable.display()
            ));
        }
    };
    // TST-17: a test still running at its limit has failed.
    if let (true, Some(limit)) = (timed_out, launch.limit) {
        let said = format!(
            "timed out: it was still running after its limit of {}",
            seconds(limit)
        );
        return launch
            .tests
            .iter()
            .map(|(slot, _, _)| (*slot, CaseOutcome::Failed(said.clone())))
            .collect();
    }
    let printed = String::from_utf8_lossy(&finished.stderr);
    let last = printed
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("it printed no error");
    let stopped = format!("the executable stopped ({}): {last}", finished.status);
    let recorded = fs::read_to_string(&results).unwrap_or_default();
    read_results(&recorded, launch.tests.len())
        .into_iter()
        .zip(&launch.tests)
        .map(|(recorded, (slot, _, test))| {
            let outcome = match recorded.said {
                Some(Said::Passed) => CaseOutcome::Passed,
                Some(Said::Failed { line, message }) => {
                    match line.and_then(|line| {
                        test.statements
                            .iter()
                            .rev()
                            .find(|(start, _)| *start <= line)
                    }) {
                        Some((_, at)) => CaseOutcome::Failed(format!("{at}: {message}")),
                        None => CaseOutcome::Failed(message),
                    }
                }
                Some(Said::Unloaded(reason)) => {
                    CaseOutcome::NotRun(format!("its executable could not run it: {reason}"))
                }
                None => {
                    let when = if recorded.started {
                        "while running"
                    } else {
                        "before"
                    };
                    CaseOutcome::NotRun(format!("{stopped} {when} this test"))
                }
            };
            (*slot, outcome)
        })
        .collect()
}

/// Runs `command` to its end, and gives what it printed and how it
/// ended; or, where `limit` is given and the run is still going when it
/// passes, ends it and says so. Only this run's own process is ended.
fn finish(
    command: &mut Command,
    limit: Option<Duration>,
) -> std::io::Result<(std::process::Output, bool)> {
    let Some(limit) = limit else {
        return command.output().map(|output| (output, false));
    };
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;
    // What it prints is read as it runs, so that it never waits on a
    // full pipe.
    let mut stderr = child.stderr.take().expect("piped above");
    let reader = std::thread::spawn(move || {
        let mut printed = Vec::new();
        let _ = stderr.read_to_end(&mut printed);
        printed
    });
    let deadline = Instant::now() + limit;
    let (status, timed_out) = loop {
        if let Some(status) = child.try_wait()? {
            break (status, false);
        }
        if Instant::now() >= deadline {
            // It may have ended by itself since.
            let _ = child.kill();
            break (child.wait()?, true);
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let stderr = reader.join().unwrap_or_default();
    let output = std::process::Output {
        status,
        stdout: Vec::new(),
        stderr,
    };
    Ok((output, timed_out))
}

/// A time limit as a person reads it: `1m`, `90s`, `0.5s`.
fn seconds(limit: Duration) -> String {
    let seconds = limit.as_secs_f64();
    if seconds >= 60.0 && seconds % 60.0 == 0.0 {
        format!("{}m", (seconds / 60.0) as u64)
    } else {
        format!("{seconds}s")
    }
}
