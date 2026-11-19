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

/// RUN-2: "What starts an application may replace an argument of its
/// entry or a field of its configuration with a constant checked
/// against the type it replaces". A replacement that names nothing the
/// entry or the configuration has is refused before anything is run:
/// status 2. One of the wrong type is the program's error, reported
/// where the application would have written it: status 1.
#[test]
fn a_replacement_is_checked_before_the_application_is_run() {
    let root = scratch("replacements_checked");
    write(&root, "ticks.hgl", TICKS);
    let application = write(&root, "backtest.hgl", BACKTEST);
    let run = |given: &[&str]| {
        let mut arguments = vec!["run".to_string(), argument(&root)];
        arguments.extend(given.iter().map(|given| given.to_string()));
        let arguments: Vec<&str> = arguments.iter().map(String::as_str).collect();
        hgl(&arguments)
    };

    let output = run(&["--arg", "limit=4"]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        text(&output.stderr),
        "hgl: the entry `counted` has no parameter `limit`: it has upto, every\n"
    );
    let output = run(&["--config", "until=4"]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        text(&output.stderr),
        "hgl: the configuration `RunConfig` has no field `until`: it has start, end, length\n"
    );
    // Text that would add an argument of its own is no one expression.
    let output = run(&["--arg", "upto=3, every: 2s"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        text(&output.stderr)
            .starts_with("hgl: the value of `upto`, `3, every: 2s`, is not one HGL expression"),
        "{}",
        text(&output.stderr)
    );

    // "a constant checked against the type it replaces".
    let output = run(&["--arg", "upto=\"three\""]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        text(&output.stdout),
        "not run: the program is rejected with the replacements given\n"
    );
    let reported = text(&output.stderr);
    assert!(reported.contains("type.mismatch"), "{reported}");
    // The error is where the application would have written the value,
    // and shows the statement as it was checked.
    assert!(
        reported.contains(&format!("{application}:5:21")),
        "{reported}"
    );
    assert!(
        reported.contains("eval(counted(upto: (\"three\")), RunConfig(length: 10s))"),
        "{reported}"
    );
    // The same for a field of the configuration.
    let output = run(&["--config", "length=3"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        text(&output.stderr).contains("type.mismatch"),
        "{}",
        text(&output.stderr)
    );
    // A replacement is a constant: nothing of the application's file
    // that is no constant is one.
    let output = run(&["--arg", "upto=missing"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        text(&output.stderr).contains("name.unknown"),
        "{}",
        text(&output.stderr)
    );
}

/// A module with an entry that takes an enum, and a generic one.
const SIDES: &str = concat!(
    "module demo.sides\n",
    "\n",
    "export enum Side { buy, sell }\n",
    "\n",
    "export fn sided(const side: Side, const upto: i64 = 2) -> i64 {\n",
    "    cache count: i64 = 0\n",
    "    while count < upto {\n",
    "        count += 1\n",
    "        yield 1s: if side == .buy { count } else { -count }\n",
    "    }\n",
    "}\n",
    "\n",
    "export fn echo<T>(const value: T) -> T {\n",
    "    yield 1s: value\n",
    "}\n",
);

/// An application of the enum's entry, and one of the generic one,
/// whose `T` its argument makes an `i64`.
const SIDED: &str = concat!(
    "application demo.sides.sided\n",
    "\n",
    "use demo.sides::{sided}\n",
    "\n",
    "eval(sided(side: .buy))\n",
);
const ECHOED: &str = concat!(
    "application demo.sides.echoed\n",
    "\n",
    "use demo.sides::{echo}\n",
    "\n",
    "eval(echo(value: 3))\n",
);

/// RUN-2: "a constant checked against the type it replaces". The type
/// of a generic entry's parameter is the one the application's call
/// makes it: `echo(value: 3)` runs `echo<i64>`. The second read has
/// the entry's generic arguments written, so text given for `value`
/// is checked against the `i64` it replaces, and is rejected: status
/// 1, where it would otherwise have made the entry `echo<str>`.
#[test]
fn a_replacement_does_not_change_what_the_entry_is() {
    let root = scratch("replacements_generic");
    write(&root, "sides.hgl", SIDES);
    let application = write(&root, "echoed.hgl", ECHOED);
    let output = hgl(&["run", &argument(&root), "--arg", "value=\"text\""]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        text(&output.stdout),
        "not run: the program is rejected with the replacements given\n"
    );
    let reported = text(&output.stderr);
    assert!(
        reported.contains("this is `str`, and the parameter `value` is `i64`"),
        "{reported}"
    );
    assert!(
        reported.contains(&format!("{application}:5:24")),
        "{reported}"
    );
    assert!(
        reported.contains("eval(echo<i64>(value: (\"text\")))"),
        "{reported}"
    );
}

/// UNIT-4: "Types have module-qualified nominal identity". Two modules
/// each have a `Side`, and a replacement of one with a member of the
/// other is of another type, though both are written `Side`: it is
/// rejected. A member of the same enum is not, and the run then gets
/// as far as its host, which here is no interpreter: status 2.
#[test]
fn a_type_of_the_same_name_is_another_type() {
    let root = scratch("replacements_same_name");
    write(&root, "sides.hgl", SIDES);
    write(
        &root,
        "left.hgl",
        "module demo.left\n\nexport enum Side { buy, sell }\n",
    );
    write(
        &root,
        "right.hgl",
        "module demo.right\n\nexport enum Side { buy, sell }\n",
    );
    write(
        &root,
        "app.hgl",
        concat!(
            "application demo.app\n",
            "\n",
            "use demo.sides::{echo}\n",
            "use demo.left as left\n",
            "use demo.right as right\n",
            "\n",
            "eval(echo(value: left::Side.buy))\n",
        ),
    );
    let output = hgl(&["run", &argument(&root), "--arg", "value=right::Side.sell"]);
    assert_eq!(output.status.code(), Some(1), "{}", text(&output.stderr));
    assert_eq!(
        text(&output.stderr),
        "hgl: a replacement is of the type it replaces: `demo.app` runs \
         `demo.sides::echo<Side>`, and with the replacements given it would run \
         `demo.sides::echo<Side>`, whose generic arguments are other types of the same names\n"
    );
    let missing = root.join("no-such-python");
    let output = hgl(&[
        "run",
        &argument(&root),
        "--arg",
        "value=left::Side.sell",
        "--python",
        &argument(&missing),
    ]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output.stderr));
    assert!(
        text(&output.stderr).contains("was not run"),
        "{}",
        text(&output.stderr)
    );
}

/// RUN-2: the replaced argument and the replaced field are what the
/// application runs with, "and what the application writes is the
/// default". The interpreter is the one `HGL_HGRAPH_PYTHON` names
/// where `--python` is not given. Needs hgraph, as below.
#[test]
#[ignore = "needs a Python with hgraph, named by HGL_HGRAPH_PYTHON"]
fn an_application_runs_with_what_starts_it_replaces() {
    std::env::var("HGL_HGRAPH_PYTHON")
        .expect("HGL_HGRAPH_PYTHON names a Python interpreter that has hgraph");
    let root = scratch("replaced");
    write(&root, "ticks.hgl", TICKS);
    write(&root, "backtest.hgl", BACKTEST);
    // As written: `counted(upto: 3)` under `RunConfig(length: 10s)`.
    let output = hgl(&[
        "run",
        &argument(&root),
        "--arg",
        "upto=5",
        "--arg",
        "every=500ms",
        "--config",
        "length=2s",
    ]);
    assert_eq!(
        text(&output.stdout),
        concat!(
            "1970-01-01T00:00:00.500001Z  1\n",
            "1970-01-01T00:00:01.000001Z  2\n",
            "1970-01-01T00:00:01.500001Z  3\n",
            "ran demo.ticks.backtest on hgraph: 3 publications\n",
        ),
        "{}",
        text(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(0));
    // A member of the enum the parameter is, as the application may
    // write it (TYPE-7), and a generic entry given another value of
    // the type its argument is.
    write(&root, "sides.hgl", SIDES);
    write(&root, "sided.hgl", SIDED);
    write(&root, "echoed.hgl", ECHOED);
    let output = hgl(&[
        "run",
        &argument(&root),
        "--application",
        "demo.sides.sided",
        "--arg",
        "side=.sell",
    ]);
    assert_eq!(
        text(&output.stdout),
        concat!(
            "1970-01-01T00:00:01.000001Z  -1\n",
            "1970-01-01T00:00:02.000001Z  -2\n",
            "ran demo.sides.sided on hgraph: 2 publications\n",
        ),
        "{}",
        text(&output.stderr)
    );
    let output = hgl(&[
        "run",
        &argument(&root),
        "--application",
        "demo.sides.echoed",
        "--arg",
        "value=4 + 1",
    ]);
    assert_eq!(
        text(&output.stdout),
        concat!(
            "1970-01-01T00:00:01.000001Z  5\n",
            "ran demo.sides.echoed on hgraph: 1 publication\n",
        ),
        "{}",
        text(&output.stderr)
    );
    // A replacement is read as the argument it replaces was expected:
    // with the `Side` and the `f64` the application's own argument
    // made the entry's `T`, `.sell` is a member (TYPE-7) and `3` an
    // `f64` (EXPR-5).
    write(
        &root,
        "member.hgl",
        concat!(
            "application demo.sides.member\n",
            "\n",
            "use demo.sides::{echo, Side}\n",
            "\n",
            "eval(echo(value: Side.buy))\n",
        ),
    );
    write(
        &root,
        "real.hgl",
        concat!(
            "application demo.sides.real\n",
            "\n",
            "use demo.sides::{echo}\n",
            "\n",
            "eval(echo(value: 2.5))\n",
        ),
    );
    let output = hgl(&[
        "run",
        &argument(&root),
        "--application",
        "demo.sides.member",
        "--arg",
        "value=.sell",
    ]);
    assert_eq!(
        text(&output.stdout),
        concat!(
            "1970-01-01T00:00:01.000001Z  .sell\n",
            "ran demo.sides.member on hgraph: 1 publication\n",
        ),
        "{}",
        text(&output.stderr)
    );
    let output = hgl(&[
        "run",
        &argument(&root),
        "--application",
        "demo.sides.real",
        "--arg",
        "value=3",
    ]);
    assert_eq!(
        text(&output.stdout),
        concat!(
            "1970-01-01T00:00:01.000001Z  3.0\n",
            "ran demo.sides.real on hgraph: 1 publication\n",
        ),
        "{}",
        text(&output.stderr)
    );
    // With nothing replaced it runs as it is written.
    let output = hgl(&[
        "run",
        &argument(&root),
        "--application",
        "demo.ticks.backtest",
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
            "fn one() -> i64 {\n",
            "    yield 1s: 1\n",
            "}\n",
            // An `if` over a connection whose branches assign two
            // `var`s is not written yet.
            "fn waits() -> i64 {\n",
            "    var first = one()\n",
            "    var second = one()\n",
            "    if one() > 0 {\n",
            "        first = one()\n",
            "        second = one()\n",
            "    }\n",
            "    return first + second\n",
            "}\n",
            "eval(waits())\n",
        ),
    );
    let output = hgl(&["run", &argument(&root)]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        text(&output.stderr),
        format!(
            "{file}:8:5: the hgraph target does not write an `if` or a `switch` over a connection whose branches assign more than one `var` yet\n"
        )
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
    // It counted from 1 for as long as it ran: on a slow host that may
    // be no time at all, where the interrupt comes before the run
    // starts.
    for (position, line) in published.iter().enumerate() {
        assert!(line.ends_with(&format!("  {}", position + 1)), "{printed}");
    }
    assert!(
        summary.starts_with("ran demo.ticks.endless on hgraph until it was stopped: "),
        "{printed}"
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
