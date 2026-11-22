//! `--target cpp`: the cpp target in `hgl emit`, `hgl test` and
//! `hgl run`, and its build driver.
//!
//! `hgl_cpp` writes a program as C++; this compiles it and runs it. For
//! `hgl test` the driver
//!
//! 1. writes the program and lists each test the target could not
//!    write all of as not run, with the first form it reaches that is
//!    not written;
//! 2. finds the toolchain (`sdk`): hgraph's C++ SDK and its
//!    dependencies from the release bundle or `HGL_HGRAPH_SDK` and `HGL_HGRAPH_DEPS`, the
//!    compiler, `ninja`; where one is missing every test that could run
//!    is not run, with what is missing;
//! 3. prepares the toolchain once and keeps it (`sdk`): the runtime
//!    library, the description of a compile and a link, and the
//!    precompiled header;
//! 4. writes the program and its `build.ninja` into the program's
//!    workspace and runs `ninja` (`build`);
//! 5. runs each executable built, one for each module, on a plan of the
//!    module's tests to run, and reads what became of each from the
//!    results file it writes (`results`).
//!
//! For `hgl run` it builds the one executable of the application chosen
//! in the same way, runs it once, and reads what became of the run
//! (`application`).

mod application;
mod build;
mod results;
mod sdk;

use std::ffi::OsString;
use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use hgl_analyser::Program;
use hgl_cpp::Output;
use hgl_diagnostics::{Diagnostics, Location};
use hgl_runner::{Executor, Outcome};
use hgl_source::SourceMap;

use super::{EXIT_FAILED, EXIT_REJECTED, count};
use application::RanApplication;
use build::{Workspace, write_if_changed};
use results::{Said, read_results};

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
        let file_name = |at: Location| self.sources.file(at.file).name().to_string();
        let output = hgl_cpp::transpile(program, &file_name, &mut unwritten);
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
            // test is run; with no test to say it of, a failure to write
            // it is not reported.
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
        // A test held to a time limit (TST-17) is run in a process of its
        // own, which is ended at the limit; the others of an executable
        // share one.
        let mut launches: Vec<Launch> = Vec::new();
        for (&executable, _) in wanted.iter().zip(&built.fresh).filter(|(_, fresh)| **fresh) {
            let tests = runnable
                .iter()
                .enumerate()
                .filter(|(_, (_, _, of))| *of == executable);
            let mut shared = Vec::new();
            for (slot, (_, test, _)) in tests {
                match test.limit() {
                    Some(limit) => launches.push(Launch {
                        executable,
                        tests: vec![(slot, *test)],
                        limit: Some(limit),
                    }),
                    None => shared.push((slot, *test)),
                }
            }
            if !shared.is_empty() {
                launches.push(Launch {
                    executable,
                    tests: shared,
                    limit: None,
                });
            }
        }
        let mut outcomes = vec![None; runnable.len()];
        let libraries = sdk::library_path(&description);
        for (slot, outcome) in launch_all(
            &workspace,
            output,
            &launches,
            self.sources,
            libraries.as_ref(),
            jobs.as_deref(),
        ) {
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

/// One run of an executable, and the tests it is to run: each as a
/// position among the tests asked to run, and the test.
struct Launch<'o> {
    /// Its position in `Output::executables`.
    executable: usize,
    /// The tests.
    tests: Vec<(usize, &'o hgl_cpp::Test)>,
    /// How long the run may take, for the one test held to a limit
    /// (TST-17); `None` for tests with none.
    limit: Option<Duration>,
}

/// Runs each of `launches` in `workspace`, as many at once as `jobs`
/// says or the machine has cores, each with `libraries`, a variable and
/// its value, in its environment where there is one
/// (`sdk::library_path`), and gives what became of each test, by its
/// position among those asked to run.
fn launch_all(
    workspace: &Workspace,
    output: &Output,
    launches: &[Launch],
    sources: &SourceMap,
    libraries: Option<&(&'static str, OsString)>,
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
                        ran.extend(launch_one(
                            workspace, output, launch, taken, sources, libraries,
                        ));
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
    output: &Output,
    launch: &Launch,
    position: usize,
    sources: &SourceMap,
    libraries: Option<&(&'static str, OsString)>,
) -> Vec<(usize, Outcome)> {
    let target = &output.executables[launch.executable];
    let stem = &target.stem;
    let all = |reason: String| -> Vec<(usize, Outcome)> {
        launch
            .tests
            .iter()
            .map(|(slot, _)| (*slot, Outcome::NotRun(reason.clone())))
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
    let executable = workspace.executable(target);
    let mut command = Command::new(&executable);
    if let Some((variable, value)) = libraries {
        command.env(variable, value);
    }
    command
        .arg(&results)
        .args(launch.tests.iter().map(|(_, test)| test.identity()));
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
            .map(|(slot, _)| (*slot, Outcome::FailedRun(said.clone())))
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
    let place = |at: Location| {
        let source = sources.file(at.file);
        format!("{}:{}", source.name(), source.line_col(at.span.start))
    };
    read_results(&recorded, launch.tests.len())
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

/// `hgl run --target cpp` of application `chosen` of `program`
/// (UNIT-14, RUN-1): its executable built in the program's workspace,
/// the directory `keep` names or the one of its source `roots`, and run
/// once, and its output printed (`ran`). An interrupt of `hgl`, or a
/// request for it to end, is the host's stop of the run (RUN-3), which
/// is passed on to the executable (`application::Stop`).
pub(super) fn run(
    sources: &SourceMap,
    program: &Program,
    chosen: &str,
    keep: Option<&Path>,
    roots: &[PathBuf],
    debug_build: Option<&Path>,
    original_source: Option<&(hgl_source::SourceFile, super::replace::SourceEdits)>,
) -> ExitCode {
    let mut unwritten = Diagnostics::new();
    let position = |at: Location, line: usize| {
        let name = sources.file(at.file).name();
        let line = match original_source {
            Some((original, edits)) if original.name() == name => {
                original.line_index(edits.original_offset(at.span.start)?) + 1
            }
            _ => line,
        };
        let path = if debug_build.is_some() && !name.starts_with('<') {
            std::path::absolute(name).map_or_else(|_| name.to_string(), |p| p.display().to_string())
        } else {
            name.to_string()
        };
        Some((path, line))
    };
    let output = hgl_cpp::transpile_with_positions(program, &position, &mut unwritten);
    let name = hgl_cpp::TARGET;
    let Some(application) = output
        .applications
        .iter()
        .find(|application| application.path == chosen)
    else {
        eprintln!("hgl: the target wrote no application `{chosen}`");
        return ExitCode::from(EXIT_FAILED);
    };
    if let Some(diagnostic) = &application.blocked {
        let source = sources.file(diagnostic.primary.file);
        eprintln!(
            "{}:{}: {}",
            source.name(),
            source.line_col(diagnostic.primary.span.start),
            diagnostic.message
        );
        if debug_build.is_none() {
            println!(
                "not run: the {name} target does not write all that {} needs yet",
                application.path
            );
        }
        return ExitCode::from(EXIT_FAILED);
    }
    if let Some(directory) = debug_build {
        return match application::debug_launch(&output, application, directory, roots) {
            Ok(launch) => {
                println!("{launch}");
                ExitCode::SUCCESS
            }
            Err(reason) => {
                eprintln!("hgl: {reason}");
                ExitCode::from(EXIT_FAILED)
            }
        };
    }
    // Where the handler cannot be set the run goes on all the same, and
    // an interrupt then ends `hgl` as it always did.
    let stop = application::Stop::new();
    let asked = stop.clone();
    let _ = ctrlc::set_handler(move || asked.ask());
    let ran = application::run(&output, application, keep, roots, &stop);
    self::ran(&application.path, name, ran)
}

/// What `hgl run` prints of what became of application `path` on the
/// target `name`, and the status it ends with: the run's output once the
/// run has stopped, a line for each publication (core's CRUN-8), or why
/// it failed (CRUN-6: "A run that failed has no output") or was not run.
fn ran(path: &str, name: &str, ran: RanApplication) -> ExitCode {
    match ran {
        RanApplication::Stopped { output, by_host } => {
            let how = if by_host { " until it was stopped" } else { "" };
            match output {
                Some(published) => {
                    for (at, value) in &published {
                        println!("{at}  {value}");
                    }
                    println!(
                        "ran {path} on {name}{how}: {}",
                        count(published.len(), "publication")
                    );
                }
                None => println!("ran {path} on {name}{how}: its entry has no result"),
            }
            ExitCode::SUCCESS
        }
        RanApplication::Failed(message) => {
            eprintln!("hgl: {path} failed: {message}");
            ExitCode::from(EXIT_REJECTED)
        }
        RanApplication::NotRun(reason) => {
            eprintln!("hgl: {path} was not run: {reason}");
            ExitCode::from(EXIT_FAILED)
        }
    }
}
