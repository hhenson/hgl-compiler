//! Running a written program on hgraph: its tests, and an application.
//!
//! hgraph is run from Python, so the host is a Python interpreter that
//! has hgraph installed. [`run`] writes the program into a directory,
//! starts the interpreter on a small driver the target carries
//! (`support/run_tests.py`), and reads what the driver recorded of each
//! test. [`run_application`] does the same for one application, with
//! a driver of its own (`support/run_application.py`).
//!
//! The driver and this file agree on two files, both in the output
//! directory:
//!
//! - the plan, written here: one test on each line, as its Python
//!   module, a space, and its function;
//! - the results, written by the driver as the tests run. Each line is
//!   `start N`, `passed N`, `failed N LINE TEXT` or `unloaded N TEXT`,
//!   where `N` is the test's position in the plan.
//!
//! The results are a file, and not what the interpreter prints, so that
//! nothing a program or hgraph prints can be taken for an outcome.

use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;

use hgl_diagnostics::Location;

use crate::emit::APPLICATION;
use crate::{File, Output, ROOT, python_module, test_function};

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
/// An interrupt of a run in real time is the driver's to take: it
/// asks the run to stop, and the run's output is then what it
/// recorded up to there. The caller is to outlive the interrupt.
pub fn run_application(
    python: &Path,
    directory: &Path,
    output: &Output,
    application: usize,
) -> RanApplication {
    let driver = directory.join(ROOT).join(APPLICATION_DRIVER_FILE);
    let results = directory.join(APPLICATION_RESULTS_FILE);
    let written = write(&output.files, directory)
        .and_then(|()| fs::write(&driver, APPLICATION_DRIVER))
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
    // `-B`: the interpreter writes no bytecode files, into the output
    // directory or beside hgraph.
    let finished = Command::new(python)
        .arg("-B")
        .arg(&driver)
        .arg(directory)
        .arg(python_module(&output.applications[application].path))
        .arg(APPLICATION)
        .arg(&results)
        .status();
    let status = match finished {
        Ok(status) => status,
        Err(error) => {
            return RanApplication::NotRun(format!(
                "`{}` could not be started: {error}",
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

/// Runs tests of a written program on hgraph, and says what became of
/// each, in the order given.
///
/// `python` is the interpreter to start, one that has hgraph.
/// `directory` is where the program is written; `output` is what
/// [`crate::transpile`] gave, and `tests` are positions in its
/// `tests`. A test that is blocked is not to be given: it, or a
/// function it reaches, was not written.
pub fn run(python: &Path, directory: &Path, output: &Output, tests: &[usize]) -> Vec<Ran> {
    let all = |reason: String| vec![Ran::NotRun(reason); tests.len()];

    let driver = directory.join(ROOT).join(DRIVER_FILE);
    let plan = directory.join(PLAN_FILE);
    let results = directory.join(RESULTS_FILE);
    let planned: String = tests
        .iter()
        .map(|&test| {
            let test = &output.tests[test];
            format!(
                "{} {}\n",
                python_module(&test.module),
                test_function(&test.name)
            )
        })
        .collect();
    let written = write(&output.files, directory)
        .and_then(|()| fs::write(&driver, DRIVER))
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

    // What the driver recorded of each test: the last line that names
    // it. A file that is not there is one with nothing in it.
    let recorded = fs::read_to_string(&results).unwrap_or_default();
    let mut outcomes: Vec<Option<Ran>> = vec![None; tests.len()];
    let mut started = vec![false; tests.len()];
    for line in recorded.lines() {
        let mut parts = line.splitn(3, ' ');
        let (Some(what), Some(number)) = (parts.next(), parts.next()) else {
            continue;
        };
        let Some(number) = number.parse::<usize>().ok().filter(|&n| n < tests.len()) else {
            continue;
        };
        let rest = parts.next().unwrap_or("");
        match what {
            "start" => started[number] = true,
            "passed" => outcomes[number] = Some(Ran::Passed),
            "failed" => {
                let (line, text) = rest.split_once(' ').unwrap_or((rest, ""));
                let at = line
                    .parse::<usize>()
                    .ok()
                    .and_then(|line| output.tests[tests[number]].statement_at(line));
                outcomes[number] = Some(Ran::Failed {
                    at,
                    message: unescaped(text),
                });
            }
            "unloaded" => {
                outcomes[number] = Some(Ran::NotRun(format!(
                    "its module could not be loaded: {}",
                    unescaped(rest)
                )));
            }
            _ => {}
        }
    }
    outcomes
        .into_iter()
        .zip(started)
        .map(|(outcome, started)| {
            outcome.unwrap_or_else(|| {
                let when = if started { "while running" } else { "before" };
                Ran::NotRun(format!("{stopped} {when} this test"))
            })
        })
        .collect()
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
    use super::unescaped;

    #[test]
    fn the_drivers_text_reads_back() {
        assert_eq!(unescaped(r"a\\b\nc\rd"), "a\\b\nc\rd");
        // A backslash the driver would not write is kept.
        assert_eq!(unescaped(r"end\"), "end\\");
    }
}
