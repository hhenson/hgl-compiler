//! `hgl run --target cpp`: an application's executable built and run
//! once (UNIT-14, RUN-1), a stop of its host passed on to it (RUN-3),
//! and what it recorded read back.
//!
//! The executable is `run/<application-stem>` of the program's workspace,
//! `EXECUTABLE RESULTS`, and its `main` is the runtime library's host of
//! the run (`hglrt/application_host.h`). It writes RESULTS as the run
//! goes, one line for each thing that happens:
//!
//! ```text
//! start                the run is about to start
//! published TIME TEXT  one publication of the run's output
//! stopped              the run ended, every publication of its output above
//! stopped none         the run ended, and its entry has no result
//! host                 the host asked the run to stop, before `stopped`
//! failed TEXT          the run failed, and has no output
//! ```
//!
//! What the program logs and hgraph prints, to either stream, goes to
//! this process's standard error as it is printed: standard output is
//! left to what reports the run's output. The executable's standard
//! input is a pipe a stop is passed down.

use std::fs;
use std::io::{self, Write as _};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use hgl_cpp::application_stem;

use super::build::Workspace;

/// What became of an application given to [`launch`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum RanApplication {
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
    /// It was not run to an end, with the reason: its executable was not
    /// built, did not start or stopped early.
    NotRun(String),
}

/// What asks an application's run to stop, as its host (RUN-3: "until
/// the configuration's end or until its host stops it"). It may be asked
/// from any thread while the run goes: by what handles an interrupt of
/// `hgl`, say.
///
/// The first request goes to the executable down its standard input, so
/// that it reaches the run however `hgl` came to ask: a signal sent to
/// `hgl` alone reaches no other process. A run in real time then ends as
/// it does at its end, and what it recorded is its output. A second
/// request ends the executable as it is, and the run has no output.
#[derive(Clone, Default)]
pub(super) struct Stop {
    /// How many times it was asked, and the executable's process while it
    /// runs. A lock, since it is asked from one thread and the run is
    /// waited for on another.
    state: Arc<Mutex<StopState>>,
}

/// What a [`Stop`] holds.
#[derive(Default)]
struct StopState {
    /// How many times a stop was asked for.
    asked: u32,
    /// The executable's process, from when it starts until it has ended.
    running: Option<Child>,
}

impl Stop {
    /// A stop not yet asked for.
    pub(super) fn new() -> Stop {
        Stop::default()
    }

    /// Asks the run to stop. One asked for before the executable starts
    /// is kept for it.
    pub(super) fn ask(&self) {
        let mut state = self.lock();
        state.asked += 1;
        state.pass_on();
    }

    /// The state, even where a thread died holding its lock.
    fn lock(&self) -> std::sync::MutexGuard<'_, StopState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl StopState {
    /// Passes what was asked to the executable, where it has started.
    fn pass_on(&mut self) {
        let Some(running) = &mut self.running else {
            return;
        };
        if self.asked > 1 {
            // It may have ended by itself already.
            let _ = running.kill();
        } else if self.asked == 1
            && let Some(pipe) = &mut running.stdin
        {
            // One that has ended reads nothing, and needs no stop.
            let _ = pipe.write_all(b"stop\n").and_then(|()| pipe.flush());
        }
    }
}

/// The variable that names where an executable finds its libraries.
pub(super) type Libraries = Option<(&'static str, std::ffi::OsString)>;

/// Executable and arguments to pass to the debugger.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct DebugLaunch {
    executable: String,
    arguments: [String; 2],
    working_directory: String,
    environment: serde_json::Map<String, serde_json::Value>,
}

/// The launch description consumed by a native debugger.
pub(super) fn debug_launch(
    workspace: &Workspace,
    path: &str,
    libraries: Libraries,
) -> Result<DebugLaunch, String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let mut environment = serde_json::Map::new();
    if let Some((name, value)) = libraries {
        let value = value
            .into_string()
            .map_err(|_| format!("{name} is not UTF-8"))?;
        environment.insert(name.into(), value.into());
    }
    let path_text = |path: &Path| {
        path.to_str()
            .map(str::to_owned)
            .ok_or_else(|| format!("debug launch path is not UTF-8: {}", path.display()))
    };
    let results = path_text(&workspace.directory.join("results.txt"))?;
    Ok(DebugLaunch {
        executable: path_text(&workspace.application(path))?,
        arguments: [results, "--debug-console".to_string()],
        working_directory: path_text(&cwd)?,
        environment,
    })
}

/// Runs the executable of `application` built in `workspace`, with
/// `libraries` where it finds its libraries, passing it what `stop` is
/// asked, and reads what became of the run from the results it writes.
pub(super) fn launch(
    workspace: &Workspace,
    path: &str,
    libraries: &Libraries,
    stop: &Stop,
) -> RanApplication {
    let results = workspace
        .directory
        .join("results")
        .join(format!("{}.run.txt", application_stem(path)));
    // What an earlier run recorded is not this run's.
    let cleared = match fs::remove_file(&results) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => fs::create_dir_all(workspace.directory.join("results")),
    };
    if let Err(error) = cleared {
        return RanApplication::NotRun(format!(
            "{} could not be made ready: {error}",
            results.display()
        ));
    }
    let executable = workspace.application(path);
    let mut command = Command::new(&executable);
    if let Some((variable, value)) = libraries {
        command.env(variable, value);
    }
    let started = command
        .arg(&results)
        .stdin(Stdio::piped())
        .stdout(Stdio::from(io::stderr()))
        .spawn();
    let child = match started {
        Ok(child) => child,
        Err(error) => {
            return RanApplication::NotRun(format!(
                "{} could not be started: {error}",
                executable.display()
            ));
        }
    };
    // The executable is where a stop can reach it, and one asked for
    // before now is passed on.
    {
        let mut state = stop.lock();
        state.running = Some(child);
        state.pass_on();
    }
    // Waits for it with the lock held only to look: a stop is asked from
    // another thread while this one waits.
    let status = loop {
        let mut state = stop.lock();
        let ended = match state.running.as_mut().map(Child::try_wait) {
            Some(Ok(None)) => None,
            Some(Ok(Some(status))) => Some(Ok(status)),
            Some(Err(error)) => Some(Err(error)),
            None => Some(Err(io::Error::other("the executable was lost"))),
        };
        if let Some(ended) = ended {
            state.running = None;
            break ended;
        }
        drop(state);
        thread::sleep(Duration::from_millis(20));
    };
    let status = match status {
        Ok(status) => status,
        Err(error) => {
            return RanApplication::NotRun(format!(
                "{} could not be waited for: {error}",
                executable.display()
            ));
        }
    };
    let recorded = fs::read_to_string(&results).unwrap_or_default();
    read_run(&recorded).unwrap_or_else(|started| {
        let when = if started { "while running" } else { "before" };
        RanApplication::NotRun(format!(
            "the executable stopped ({status}) {when} the application"
        ))
    })
}

/// What became of a run, from the results `recorded` its executable
/// wrote; or, where they say no end, whether it had started.
fn read_run(recorded: &str) -> Result<RanApplication, bool> {
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
                return Ok(RanApplication::Stopped {
                    output: (rest != "none").then_some(published),
                    by_host,
                });
            }
            "failed" => return Ok(RanApplication::Failed(unescaped(rest))),
            _ => {}
        }
    }
    Err(started)
}

/// `text` as the executable meant it: `\\`, `\n` and `\r` are a
/// backslash, a line feed and a carriage return, and a backslash before
/// anything else is kept as it is.
fn unescaped(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut characters = text.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            out.push(character);
            continue;
        }
        match characters.next() {
            Some('\\') => out.push('\\'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{RanApplication, read_run, unescaped};

    #[test]
    fn the_hosts_text_reads_back() {
        assert_eq!(unescaped(r"a\\b\nc\rd"), "a\\b\nc\rd");
        assert_eq!(unescaped(r"\q"), r"\q");
        assert_eq!(unescaped("end\\"), "end\\");
    }

    #[test]
    fn what_became_of_a_run_is_read_from_its_results() {
        assert_eq!(
            read_run("start\npublished 1970-01-01T00:00:01Z 1\nstopped\n"),
            Ok(RanApplication::Stopped {
                output: Some(vec![("1970-01-01T00:00:01Z".to_string(), "1".to_string())]),
                by_host: false,
            })
        );
        assert_eq!(
            read_run("start\nhost\nstopped none\n"),
            Ok(RanApplication::Stopped {
                output: None,
                by_host: true,
            })
        );
        assert_eq!(
            read_run("start\nfailed numeric.out_of_range: a\\nb\n"),
            Ok(RanApplication::Failed(
                "numeric.out_of_range: a\nb".to_string()
            ))
        );
        assert_eq!(
            read_run("start\npublished 1970-01-01T00:00:01Z 1\n"),
            Err(true)
        );
        assert_eq!(read_run(""), Err(false));
    }
}
