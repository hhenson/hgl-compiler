//! Runs `hgl run` the way a user would.
//!
//! The programs are written by the tests when they run, into a scratch
//! directory of their own each.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Runs `hgl` with `arguments` and waits for it to finish.
fn hgl(arguments: &[&str]) -> Output {
    // Cargo builds the binary before the test and puts its path in this
    // environment variable at compile time.
    Command::new(env!("CARGO_BIN_EXE_hgl"))
        .args(arguments)
        .output()
        .expect("the hgl binary runs")
}

/// A stream's bytes as text.
fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("hgl writes UTF-8")
}

/// An empty directory for the test named `test`.
fn scratch(test: &str) -> PathBuf {
    // Cargo gives integration tests a scratch directory under `target/`.
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("run")
        .join(test);
    // Left over from an earlier run, perhaps; `ok()` ignores "not there".
    fs::remove_dir_all(&directory).ok();
    fs::create_dir_all(&directory).unwrap();
    directory
}

/// Writes `text` to `directory/name` and gives the path as `hgl` will
/// print it.
fn write(directory: &Path, name: &str, text: &str) -> String {
    let path = directory.join(name);
    fs::write(&path, text).unwrap();
    path.display().to_string()
}

/// A path as an argument.
fn argument(path: &Path) -> String {
    path.display().to_string()
}

/// A module with an entry, a source that counts, and one whose run
/// fails.
const TICKS: &str = concat!(
    "module demo.ticks\n",
    "\n",
    "export fn counted(const upto: i64, const every: duration = 1s) -> i64 {\n",
    "    cache count: i64 = 0\n",
    "    while count < upto {\n",
    "        count += 1\n",
    "        yield every: count\n",
    "    }\n",
    "}\n",
    "\n",
    "fn inverse(value: i64) -> i64 {\n",
    "    when { return 6 // (value - 2) }\n",
    "}\n",
    "\n",
    "export fn failing() -> i64 {\n",
    "    return inverse(counted(3))\n",
    "}\n",
);

/// RUN-3: `eval`, a simulation.
const BACKTEST: &str = concat!(
    "application demo.ticks.backtest\n",
    "\n",
    "use demo.ticks::{counted}\n",
    "\n",
    "eval(counted(upto: 3), RunConfig(length: 10s))\n",
);

/// An application whose run fails.
const BROKEN: &str = concat!(
    "application demo.ticks.broken\n",
    "\n",
    "use demo.ticks::{failing}\n",
    "\n",
    "eval(failing())\n",
);

/// `hgl run` runs the one application a program has, and one of
/// several only when it is named: status 2 where it cannot tell which,
/// where the name is of none, and where the program has none.
#[test]
fn the_application_to_run_is_the_only_one_or_the_one_named() {
    let root = scratch("which");
    write(&root, "ticks.hgl", TICKS);
    let output = hgl(&["run", &argument(&root)]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        text(&output.stderr),
        "hgl: the program has no application to run\n"
    );

    write(&root, "backtest.hgl", BACKTEST);
    write(&root, "broken.hgl", BROKEN);
    let output = hgl(&["run", &argument(&root)]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        text(&output.stderr),
        "hgl: the program has 2 applications; name one with `--application`: \
         demo.ticks.backtest, demo.ticks.broken\n"
    );

    let output = hgl(&["run", &argument(&root), "--application", "demo.ticks"]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        text(&output.stderr),
        "hgl: the program has no application `demo.ticks`; it has: \
         demo.ticks.backtest, demo.ticks.broken\n"
    );
}

/// A program the language rejects is not run: status 1, with its
/// errors. RUN-1: an entry has no temporal parameter (`run.entry`).
#[test]
fn a_rejected_program_is_not_run() {
    let root = scratch("rejected");
    write(
        &root,
        "app.hgl",
        concat!(
            "application demo.app\n",
            "fn pass(value: i64) -> i64 {\n",
            "    when { return value }\n",
            "}\n",
            "eval(pass(value: [1]))\n",
        ),
    );
    let output = hgl(&["run", &argument(&root)]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(text(&output.stdout), "not run: the program is rejected\n");
    assert!(
        text(&output.stderr).contains("run.entry"),
        "{}",
        text(&output.stderr)
    );
}

/// An application that needs a form the target does not write yet is
/// not run, and says what it needs and where: status 2.
#[test]
fn an_application_the_target_cannot_write_is_not_run() {
    let root = scratch("blocked");
    let file = write(
        &root,
        "app.hgl",
        concat!(
            "application demo.app\n",
            "fn waits() -> i64 {\n",
            "    inject scheduler\n",
            "    when { return 1 }\n",
            "}\n",
            "eval(waits())\n",
        ),
    );
    let output = hgl(&["run", &argument(&root)]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        text(&output.stderr),
        format!("{file}:3:12: the hgraph target does not write an `inject` of `scheduler` yet\n")
    );
    assert_eq!(
        text(&output.stdout),
        "not run: the hgraph target does not write all that demo.app needs yet\n"
    );
}

/// A host that does not start runs nothing: status 2.
#[test]
fn a_host_that_does_not_start_runs_no_application() {
    let root = scratch("no_host");
    write(&root, "ticks.hgl", TICKS);
    write(&root, "backtest.hgl", BACKTEST);
    let output = hgl(&["run", &argument(&root), "--python", "hgl-no-such-python"]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(text(&output.stdout), "");
    // What the system says of a program that is not there differs from
    // one system to another.
    assert!(
        text(&output.stderr).starts_with(
            "hgl: demo.ticks.backtest was not run: `hgl-no-such-python` could not be started: "
        ),
        "{}",
        text(&output.stderr)
    );
}

/// An application with no end: it runs until its host stops it.
const ENDLESS: &str = concat!(
    "application demo.ticks.endless\n",
    "\n",
    "use demo.ticks::{counted}\n",
    "\n",
    "run(counted(upto: 1000000, every: 20ms))\n",
);

/// RUN-3: a run in real time goes on "until its host stops it". An
/// interrupt sent to `hgl` alone, as what supervises it sends one,
/// stops the run all the same: `hgl` passes it on, and prints what the
/// run recorded. Needs hgraph, as below, and a system with `kill`.
#[cfg(unix)]
#[test]
#[ignore = "needs a Python with hgraph, named by HGL_HGRAPH_PYTHON"]
fn an_interrupt_of_hgl_alone_stops_the_run() {
    let python = std::env::var("HGL_HGRAPH_PYTHON")
        .expect("HGL_HGRAPH_PYTHON names a Python interpreter that has hgraph");
    let root = scratch("interrupted");
    write(&root, "ticks.hgl", TICKS);
    write(&root, "endless.hgl", ENDLESS);
    let child = Command::new(env!("CARGO_BIN_EXE_hgl"))
        .args(["run", &argument(&root), "--python", &python])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("the hgl binary runs");
    std::thread::sleep(std::time::Duration::from_millis(2000));
    // To `hgl` alone: its process, and not its group.
    let sent = Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .expect("`kill` runs");
    assert!(sent.success());
    let output = child.wait_with_output().expect("hgl ends");
    let printed = text(&output.stdout);
    assert_eq!(output.status.code(), Some(0), "{printed}");
    let lines: Vec<&str> = printed.lines().collect();
    let (summary, published) = lines.split_last().expect("something is printed");
    assert!(!published.is_empty(), "{printed}");
    assert!(published[0].ends_with("  1"), "{printed}");
    assert_eq!(
        *summary,
        format!(
            "ran demo.ticks.endless on hgraph until it was stopped: {} publications",
            published.len()
        )
    );
}

/// UNIT-14, RUN-1: an application runs, and its output is printed: a
/// line for each publication of its entry's result, in time order,
/// its instant and then what was published (core's CRUN-8). A run
/// that fails has no output (CRUN-6), and says why: status 1. What
/// was written for the run is kept where `--keep` says.
///
/// It needs a Python that has hgraph, named by the environment
/// variable `HGL_HGRAPH_PYTHON`, so it is not among the tests run by
/// default: `cargo test -p hgl_cli -- --ignored` runs it.
#[test]
#[ignore = "needs a Python with hgraph, named by HGL_HGRAPH_PYTHON"]
fn an_application_runs_on_hgraph() {
    let python = std::env::var("HGL_HGRAPH_PYTHON")
        .expect("HGL_HGRAPH_PYTHON names a Python interpreter that has hgraph");
    let root = scratch("on_hgraph");
    write(&root, "ticks.hgl", TICKS);
    write(&root, "backtest.hgl", BACKTEST);
    write(&root, "broken.hgl", BROKEN);
    let kept = scratch("on_hgraph_kept");
    let output = hgl(&[
        "run",
        &argument(&root),
        "--application",
        "demo.ticks.backtest",
        "--python",
        &python,
        "--keep",
        &argument(&kept),
    ]);
    assert_eq!(
        text(&output.stdout),
        concat!(
            "1970-01-01T00:00:01.000001Z  1\n",
            "1970-01-01T00:00:02.000001Z  2\n",
            "1970-01-01T00:00:03.000001Z  3\n",
            "ran demo.ticks.backtest on hgraph: 3 publications\n",
        )
    );
    assert_eq!(output.status.code(), Some(0));
    assert!(
        kept.join("_hgl/_m_demo/_m_ticks/_m_backtest/__init__.py")
            .is_file()
    );

    let output = hgl(&[
        "run",
        &argument(&root),
        "--application",
        "demo.ticks.broken",
        "--python",
        &python,
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(text(&output.stdout), "");
    let said = text(&output.stderr);
    let last = said.lines().last().unwrap_or("");
    assert!(
        last.starts_with("hgl: demo.ticks.broken failed: numeric.division_by_zero: "),
        "{said}"
    );
}
