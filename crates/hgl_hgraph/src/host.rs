//! Running a written program on hgraph: its tests, and an application.
//!
//! hgraph is run from Python, so the host is a Python interpreter that
//! has hgraph installed. [`run`] writes the program into a directory,
//! starts the interpreter on a small driver the target carries
//! (`support/run_tests.py`), and reads what the driver recorded of each
//! test. [`run_application`] does the same for one application, with
//! a driver of its own (`support/run_application.py`). What the two
//! drivers share is written beside them (`support/driver.py`).
//!
//! The driver and this file agree on two files, both in the output
//! directory:
//!
//! - the plan, written here: one test on each line, as its Python
//!   module, its function, and its time limit in seconds, `-` for one
//!   with none;
//! - the results, written by the driver as the tests run. Each line is
//!   `start N`, `passed N`, `failed N LINE TEXT` or `unloaded N TEXT`,
//!   where `N` is the test's position in the plan.
//!
//! The results are a file, and not what the interpreter prints, so that
//! nothing a program or hgraph prints can be taken for an outcome.

use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use hgl_diagnostics::Location;

use crate::emit::APPLICATION;
use crate::{File, Output, ROOT, Test, python_module, test_function};

/// The driver that runs named tests, as Python.
const DRIVER: &str = include_str!("../support/run_tests.py");

/// The file the driver is written to. Its name starts with `_`, as no
/// module's Python name does.
const DRIVER_FILE: &str = "_run_tests.py";

/// The file the tests to run are listed in.
const PLAN_FILE: &str = "_plan.txt";

/// The file the driver records what happened in.
const RESULTS_FILE: &str = "_results.txt";

/// The driver that runs an application, as Python.
const APPLICATION_DRIVER: &str = include_str!("../support/run_application.py");

/// The file that driver is written to.
const APPLICATION_DRIVER_FILE: &str = "_run_application.py";

/// The file that driver records an application's run in.
const APPLICATION_RESULTS_FILE: &str = "_run.txt";

/// What the two drivers share, as Python: how a results file escapes
/// its text, the function that writes a line of one, and the path the
/// program is found on.
const SHARED: &str = include_str!("../support/driver.py");

/// The file it is written to, beside the drivers, which import it as
/// `_driver`.
const SHARED_FILE: &str = "_driver.py";

/// What became of an application given to [`run_application`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RanApplication {
    /// The run ended, as core's CRUN-1 has one: built, started,
    /// evaluated and stopped.
    Stopped {
        /// The run's output (RUN-1): each publication of the entry's
        /// result in time order, as its instant and its value, both as
        /// text. `None` for an entry with no result.
        output: Option<Vec<(String, String)>>,
        /// True where the run ended because its host asked it to
        /// (RUN-3), and not at its end.
        by_host: bool,
    },
    /// The run failed, with what the host said of it. Core's CRUN-6: "A
    /// run that failed has no output."
    Failed(String),
    /// It was not run to an end, with the reason: the interpreter did
    /// not start or stopped early, or the module did not load.
    NotRun(String),
}

/// What asks an application's run to stop, as its host (RUN-3: "until
/// the configuration's end or until its host stops it"). It is given
/// to [`run_application`], and may be asked from any thread while the
/// run goes: by what handles an interrupt of the caller, say.
///
/// The request goes to the driver down a pipe, so that it reaches the
/// run however the caller came to ask: a signal sent to the caller
/// alone reaches no other process.
#[derive(Clone, Default)]
pub struct Stop {
    /// How many times it was asked, and the driver's process while it
    /// runs. A lock, since it is asked from one thread and the run is
    /// waited for on another.
    state: Arc<Mutex<StopState>>,
}

/// What a [`Stop`] holds.
#[derive(Default)]
struct StopState {
    /// How many times a stop was asked for.
    asked: u32,
    /// The driver's process, from when it starts until it has ended.
    driver: Option<Child>,
}

impl Stop {
    /// A stop not yet asked for.
    pub fn new() -> Stop {
        Stop::default()
    }

    /// Asks the run to stop. The first request is passed to the
    /// driver, which ends a run in real time as its end does: what it
    /// recorded is its output. A second ends the driver as it is, and
    /// the run then has no output. One made before the driver starts
    /// is kept for it.
    pub fn ask(&self) {
        // A lock that another thread died holding is still the state.
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.asked += 1;
        state.pass_on();
    }
}

impl StopState {
    /// Passes what was asked to the driver, where it has started.
    fn pass_on(&mut self) {
        let Some(driver) = &mut self.driver else {
            return;
        };
        if self.asked > 1 {
            // It may have ended by itself already.
            let _ = driver.kill();
        } else if self.asked == 1
            && let Some(pipe) = &mut driver.stdin
        {
            // A driver that has ended reads nothing, and needs no stop.
            let _ = pipe.write_all(b"stop\n").and_then(|()| pipe.flush());
        }
    }
}

/// Runs application `application` of a written program on hgraph, and
/// says what became of it.
///
/// `python` is the interpreter to start, one that has hgraph.
/// `directory` is where the program is written; `output` is what
/// [`crate::transpile`] gave, and `application` a position in its
/// `applications`, of one that is not blocked.
///
/// What the program logs and hgraph prints goes to this process's
/// standard error, as it is printed: a run in real time may be a long
/// one. Standard output is left to the caller, for the run's output.
/// The run's outcome is read from a file the driver writes, so that
/// nothing printed can be taken for it.
///
/// The run is stopped by its host in one of two ways. An interrupt
/// from a terminal reaches the driver as it reaches the caller, and
/// the driver takes it. Any other request is the caller's to pass on,
/// through `stop`. Either way the run's output is what it recorded up
/// to there, and the caller is to outlive what asked.
pub fn run_application(
    python: &Path,
    directory: &Path,
    output: &Output,
    application: usize,
    stop: &Stop,
) -> RanApplication {
    let driver = directory.join(ROOT).join(APPLICATION_DRIVER_FILE);
    let results = directory.join(APPLICATION_RESULTS_FILE);
    let written = write(&output.files, directory)
        .and_then(|()| write_driver(&driver, APPLICATION_DRIVER))
        // What an earlier run recorded in a directory that was kept is
        // not this run's.
        .and_then(|()| match fs::remove_file(&results) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        });
    if let Err(error) = written {
        return RanApplication::NotRun(format!(
            "the program could not be written to {}: {error}",
            directory.display()
        ));
    }
    let described = &output.applications[application];
    // `-B`: the interpreter writes no bytecode files, into the output
    // directory or beside hgraph. Its standard input is a pipe a stop
    // is passed down.
    let started = Command::new(python)
        .arg("-B")
        .arg(&driver)
        .arg(directory)
        .arg(python_module(&described.path))
        .arg(APPLICATION)
        .arg(&results)
        .arg(if described.real_time {
            "real-time"
        } else {
            "simulation"
        })
        .stdin(Stdio::piped())
        .spawn();
    let child = match started {
        Ok(child) => child,
        Err(error) => {
            return RanApplication::NotRun(format!(
                "`{}` could not be started: {error}",
                python.display()
            ));
        }
    };
    // The driver is where a stop can reach it, and one asked for
    // before now is passed on.
    {
        let mut state = stop
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.driver = Some(child);
        state.pass_on();
    }
    // Waits for the driver with the lock held only to look: a stop is
    // asked from another thread while this one waits.
    let status = loop {
        let mut state = stop
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let ended = match state.driver.as_mut().map(Child::try_wait) {
            Some(Ok(None)) => None,
            Some(Ok(Some(status))) => Some(Ok(status)),
            Some(Err(error)) => Some(Err(error)),
            None => Some(Err(io::Error::other("the driver was lost"))),
        };
        if let Some(ended) = ended {
            state.driver = None;
            break ended;
        }
        drop(state);
        thread::sleep(Duration::from_millis(20));
    };
    let status = match status {
        Ok(status) => status,
        Err(error) => {
            return RanApplication::NotRun(format!(
                "`{}` could not be waited for: {error}",
                python.display()
            ));
        }
    };
    let recorded = fs::read_to_string(&results).unwrap_or_default();
    let mut started = false;
    let mut by_host = false;
    let mut published: Vec<(String, String)> = Vec::new();
    for line in recorded.lines() {
        let (what, rest) = line.split_once(' ').unwrap_or((line, ""));
        match what {
            "start" => started = true,
            "host" => by_host = true,
            "published" => {
                let (at, value) = rest.split_once(' ').unwrap_or((rest, ""));
                published.push((at.to_string(), unescaped(value)));
            }
            "stopped" => {
                return RanApplication::Stopped {
                    output: (rest != "none").then_some(published),
                    by_host,
                };
            }
            "failed" => return RanApplication::Failed(unescaped(rest)),
            "unloaded" => {
                return RanApplication::NotRun(format!(
                    "its module could not be loaded: {}",
                    unescaped(rest)
                ));
            }
            _ => {}
        }
    }
    let when = if started { "while running" } else { "before" };
    RanApplication::NotRun(format!(
        "the interpreter stopped ({status}) {when} the application"
    ))
}

/// What became of a test given to [`run`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ran {
    /// It ran to its end.
    Passed,
    /// It ran, and an assertion did not hold or an error escaped.
    Failed {
        /// The statement of the test that was running, when the host
        /// could say.
        at: Option<Location>,
        /// What the host said of it.
        message: String,
    },
    /// It was not run to an end, with the reason: the interpreter did
    /// not start or stopped early, or the module did not load. Such a
    /// test shows nothing (TST-14: "crashes, and infrastructure
    /// failures cannot prove conformance").
    NotRun(String),
}

/// Writes `files` under `directory`, which is made when it is not
/// there. A file already there with a written file's name is replaced.
pub fn write(files: &[File], directory: &Path) -> io::Result<()> {
    for file in files {
        // A written path has `/` between its parts on every system.
        let path = file
            .path
            .split('/')
            .fold(directory.to_path_buf(), |path, part| path.join(part));
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, &file.text)?;
    }
    Ok(())
}

/// Writes the driver `text` to the file `driver`, and beside it, in the
/// same directory, what the two drivers share ([`SHARED`]).
fn write_driver(driver: &Path, text: &str) -> io::Result<()> {
    fs::write(driver.with_file_name(SHARED_FILE), SHARED)?;
    fs::write(driver, text)
}

/// Runs tests of a written program on hgraph, and says what became of
/// each, in the order given.
///
/// `python` is the interpreter to start, one that has hgraph.
/// `directory` is where the program is written; `output` is what
/// [`crate::transpile`] gave, and `tests` are positions in its
/// `tests`. A test that is blocked is not to be given: it, or a
/// function it reaches, was not written.
///
/// An integration test is held to its time limit ([`Test::limit`]): one
/// still running when it passes is stopped, and has failed.
pub fn run(python: &Path, directory: &Path, output: &Output, tests: &[usize]) -> Vec<Ran> {
    run_within(python, directory, output, tests, &Test::limit)
}

/// [`run`], with each test held to the time limit `limit` gives it,
/// `None` for no limit.
pub fn run_within(
    python: &Path,
    directory: &Path,
    output: &Output,
    tests: &[usize],
    limit: &dyn Fn(&Test) -> Option<Duration>,
) -> Vec<Ran> {
    let all = |reason: String| vec![Ran::NotRun(reason); tests.len()];

    let driver = directory.join(ROOT).join(DRIVER_FILE);
    let plan = directory.join(PLAN_FILE);
    let results = directory.join(RESULTS_FILE);
    let planned: String = tests
        .iter()
        .map(|&test| {
            let test = &output.tests[test];
            let limit =
                limit(test).map_or("-".to_string(), |limit| limit.as_secs_f64().to_string());
            format!(
                "{} {} {limit}\n",
                python_module(&test.module),
                test_function(&test.name)
            )
        })
        .collect();
    let written = write(&output.files, directory)
        .and_then(|()| write_driver(&driver, DRIVER))
        .and_then(|()| fs::write(&plan, planned))
        // Results of an earlier run in a directory that was kept are
        // not this run's.
        .and_then(|()| match fs::remove_file(&results) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        });
    if let Err(error) = written {
        return all(format!(
            "the program could not be written to {}: {error}",
            directory.display()
        ));
    }

    // The program is written whether or not a test is to be run: a
    // directory that is kept then always holds it.
    if tests.is_empty() {
        return Vec::new();
    }

    // `-B`: the interpreter writes no bytecode files, into the output
    // directory or beside hgraph.
    let finished = Command::new(python)
        .arg("-B")
        .arg(&driver)
        .arg(directory)
        .arg(&plan)
        .arg(&results)
        .output();
    let finished = match finished {
        Ok(finished) => finished,
        Err(error) => {
            return all(format!(
                "`{}` could not be started: {error}",
                python.display()
            ));
        }
    };
    // What the interpreter said last as it stopped, for a test it did
    // not finish.
    let printed = String::from_utf8_lossy(&finished.stderr);
    let last = printed
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("it printed no error");
    let stopped = format!("the interpreter stopped ({}): {last}", finished.status);

    // What the driver recorded of each test. A file that is not there
    // is one with nothing in it.
    let recorded = fs::read_to_string(&results).unwrap_or_default();
    read_results(&recorded, tests.len())
        .into_iter()
        .zip(tests)
        .map(|(recorded, &test)| match recorded.said {
            Some(Said::Passed) => Ran::Passed,
            Some(Said::Failed { line, message }) => Ran::Failed {
                at: line.and_then(|line| output.tests[test].statement_at(line)),
                message,
            },
            Some(Said::Unloaded(reason)) => {
                Ran::NotRun(format!("its module could not be loaded: {reason}"))
            }
            None => {
                let when = if recorded.started {
                    "while running"
                } else {
                    "before"
                };
                Ran::NotRun(format!("{stopped} {when} this test"))
            }
        })
        .collect()
}

/// What a results file says of one test of the plan: the protocol the
/// drivers of both targets write, described at the head of this file
/// and of `support/run_tests.py`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Recorded {
    /// True once a `start` line names it.
    pub started: bool,
    /// What the last line that names it with an outcome says, if one
    /// does.
    pub said: Option<Said>,
}

/// An outcome a results file gives a test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Said {
    /// `passed N`: it ran to its end.
    Passed,
    /// `failed N LINE TEXT`: it failed in the statement that starts at
    /// or before `line` of the file its test is written in; `None`
    /// where the line is not a number.
    Failed {
        /// The line, counted from 1; 0 where the driver did not know.
        line: Option<usize>,
        /// What the host said, with the protocol's escapes undone.
        message: String,
    },
    /// `unloaded N TEXT`: it could not be found or loaded, with why.
    Unloaded(String),
}

/// What `recorded`, the text of a results file, says of each of the
/// `count` tests of its plan, in the plan's order. A line that names
/// no test of the plan, or says nothing known, is passed over, and of
/// the lines that name a test with an outcome the last one counts.
pub fn read_results(recorded: &str, count: usize) -> Vec<Recorded> {
    let mut read = vec![Recorded::default(); count];
    for line in recorded.lines() {
        let mut parts = line.splitn(3, ' ');
        let (Some(what), Some(number)) = (parts.next(), parts.next()) else {
            continue;
        };
        let Some(number) = number.parse::<usize>().ok().filter(|&n| n < count) else {
            continue;
        };
        let rest = parts.next().unwrap_or("");
        match what {
            "start" => read[number].started = true,
            "passed" => read[number].said = Some(Said::Passed),
            "failed" => {
                let (line, text) = rest.split_once(' ').unwrap_or((rest, ""));
                read[number].said = Some(Said::Failed {
                    line: line.parse::<usize>().ok(),
                    message: unescaped(text),
                });
            }
            "unloaded" => read[number].said = Some(Said::Unloaded(unescaped(rest))),
            _ => {}
        }
    }
    read
}

/// The text the driver wrote with `\\`, `\n` and `\r` for a backslash,
/// a line feed and a carriage return.
fn unescaped(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut characters = text.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            out.push(character);
            continue;
        }
        match characters.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{Recorded, Said, read_results, unescaped};

    #[test]
    fn the_drivers_text_reads_back() {
        assert_eq!(unescaped(r"a\\b\nc\rd"), "a\\b\nc\rd");
        // A backslash the driver would not write is kept.
        assert_eq!(unescaped(r"end\"), "end\\");
    }

    /// Each test's last outcome counts; a line naming no test of the
    /// plan is passed over.
    #[test]
    fn a_results_file_reads_back() {
        let recorded = "start 0\npassed 0\nstart 1\nfailed 1 12 a\\nb\nstart 2\nunloaded 9 x\n";
        assert_eq!(
            read_results(recorded, 3),
            [
                Recorded {
                    started: true,
                    said: Some(Said::Passed),
                },
                Recorded {
                    started: true,
                    said: Some(Said::Failed {
                        line: Some(12),
                        message: "a\nb".to_string(),
                    }),
                },
                Recorded {
                    started: true,
                    said: None,
                },
            ]
        );
    }
}
