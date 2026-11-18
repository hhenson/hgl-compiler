//! Runs `hgl test` the way a user would.
//!
//! The programs are written by the tests when they run, into a scratch
//! directory of their own each.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Runs `hgl` with `arguments` and waits for it to finish.
fn hgl(arguments: &[&str]) -> Output {
    hgl_with(arguments, &[])
}

/// Runs `hgl` with `arguments` and with each of `environment`, a name
/// and a value, set in its environment.
fn hgl_with(arguments: &[&str], environment: &[(&str, &str)]) -> Output {
    // Cargo builds the binary before the test and puts its path in this
    // environment variable at compile time.
    Command::new(env!("CARGO_BIN_EXE_hgl"))
        .args(arguments)
        .envs(environment.iter().copied())
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
        .join("cases")
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

/// A program of rejection cases that pass, with one test to run.
const PASSING: &str = concat!(
    "module examples.rejection\n",
    "\n",
    "# expect-error(parse, \"string.unknown_escape\")\n",
    "const fn bad_escape() -> str => \"a\\q\"\n",
    "\n",
    "test rejected {\n",
    "    # expect-error(parse, \"string.unknown_escape\")\n",
    "    assert \"a\\q\" == \"a\"\n",
    "}\n",
    "\n",
    "test survives { assert true }\n",
);

/// TST-15: "A rejection-only run is valid", and succeeds when every case
/// passes. TST-14: "Report executed/rejected counts separately".
#[test]
fn a_rejection_only_run_that_passes_exits_zero() {
    let root = scratch("passing");
    let file = write(&root, "cases.hgl", PASSING);
    let output = hgl(&["test", &argument(&root), "--rejections-only"]);
    assert_eq!(text(&output.stderr), "");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        text(&output.stdout),
        format!(
            concat!(
                "passed   rejection   {file}:4 const fn bad_escape\n",
                "passed   rejection   examples.rejection::rejected\n",
                "rejection cases: 2 passed, 0 failed\n",
                "executable cases: none selected\n",
            ),
            file = file
        )
    );
}

/// TST-15: "Success requires every selected executable and required
/// rejection case to pass." A test is run on a target, so a run that
/// selects one and names no target does not succeed: status 2, the
/// command could not run.
#[test]
fn a_test_that_cannot_be_run_is_not_a_success() {
    let root = scratch("not_run");
    let file = write(&root, "cases.hgl", PASSING);
    let output = hgl(&["test", &argument(&root)]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        text(&output.stdout),
        format!(
            concat!(
                "passed   rejection   {file}:4 const fn bad_escape\n",
                "passed   rejection   examples.rejection::rejected\n",
                "not run  executable  examples.rejection::survives\n",
                "rejection cases: 2 passed, 0 failed\n",
                "executable cases: 1 not run: no target was given to run a test on\n",
            ),
            file = file
        )
    );

    // TST-14: a selector names tests; the declaration-owned case runs
    // all the same.
    let selected = hgl(&["test", &argument(&root), "--test", "rejected"]);
    assert_eq!(selected.status.code(), Some(0));
    assert_eq!(
        text(&selected.stdout),
        format!(
            concat!(
                "passed   rejection   {file}:4 const fn bad_escape\n",
                "passed   rejection   examples.rejection::rejected\n",
                "rejection cases: 2 passed, 0 failed\n",
                "executable cases: none selected\n",
            ),
            file = file
        )
    );
}

/// TST-13: a case fails when its errors and annotations do not match one
/// to one, and the output says which. TST-14: "A rejection mismatch fails
/// that case while other valid cases continue."
#[test]
fn a_failed_case_says_what_did_not_match_and_exits_one() {
    let root = scratch("failing");
    let file = write(
        &root,
        "cases.hgl",
        concat!(
            "module m\n",
            "fn scale() {}\n",
            "# expect-error(parse, \"string.unclosed\")\n",
            "const fn wrong_code() -> str => \"a\\q\"\n",
            "# expect-error(name, \"name.duplicate\")\n",
            "fn scale() {}\n",
            "# expect-error(parse, \"string.unknown_escape\")\n",
            "const fn right() -> str => \"a\\q\"\n",
            // TST-12: only a row of the catalogue can be expected. The
            // code for this compiler's own nesting limit is no row, so
            // no annotation meets it.
            "# expect-error(parse, \"syntax.expected_token\")\n",
            "const fn deep() -> i64 => ((((((1))))))\n",
        ),
    );
    let output = hgl(&["test", &argument(&root), "--nesting-limit", "4"]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        text(&output.stdout),
        format!(
            concat!(
                "FAILED   rejection   {file}:4 const fn wrong_code\n",
                "    expected parse/string.unclosed on line 4 of {file}: not reported\n",
                "    reported parse/string.unknown_escape at {file}:4:35, not expected: ",
                "unknown escape `\\q`\n",
                "passed   rejection   {file}:6 fn scale\n",
                "passed   rejection   {file}:8 const fn right\n",
                "FAILED   rejection   {file}:10 const fn deep\n",
                "    expected parse/syntax.expected_token on line 10 of {file}: not reported\n",
                "    reported parse/syntax.nesting_limit at {file}:10:31, not expected ",
                "(a compiler-local code, which no annotation can expect): ",
                "nesting deeper than 4 levels is not read\n",
                "rejection cases: 2 passed, 2 failed\n",
                "executable cases: none selected\n",
            ),
            file = file
        )
    );
}

/// TST-14: "Invalid metadata, ambiguous boundaries, or surviving-source
/// errors prevent execution."
#[test]
fn invalid_metadata_and_surviving_errors_stop_the_run() {
    let root = scratch("stopped");
    let file = write(
        &root,
        "cases.hgl",
        concat!(
            "module m\n",
            "# expect-error(parse, \"no.such_code\")\n",
            "fn a() {}\n",
            "# expect-error(parse, \"string.unknown_escape\")\n",
            "\n",
            "const fn stray() -> str => \"a\\q\"\n",
        ),
    );
    let output = hgl(&["test", &argument(&root)]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(text(&output.stdout), "no case was run\n");
    let stderr = text(&output.stderr);
    assert!(
        stderr.contains(&format!(
            "{file}:2:1: invalid `expect-error` annotation: the diagnostic catalogue has no \
             `no.such_code` in category `parse` (TST-12)\n"
        )),
        "{stderr}"
    );
    assert!(
        stderr.contains(&format!(
            "{file}:4:1: this `expect-error` annotation applies to a line of no import, \
             declaration or named test (TST-12)\n"
        )),
        "{stderr}"
    );
    // The error no annotation owns is shown as `hgl check` shows it.
    assert!(
        stderr.contains("error[parse/string.unknown_escape]"),
        "{stderr}"
    );
    assert!(stderr.contains(&format!("{file}:6:30")), "{stderr}");
}

/// TST-14: "Unknown selectors reject."
#[test]
fn an_unknown_selector_stops_the_run() {
    let root = scratch("selector");
    write(&root, "cases.hgl", PASSING);
    let output = hgl(&["test", &argument(&root), "--test", "missing"]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(text(&output.stdout), "no case was run\n");
    assert_eq!(
        text(&output.stderr),
        "hgl: no test is named `missing` (TST-14)\n"
    );
}

/// TST-15: "A run with no cases supplies no conformance evidence."
#[test]
fn a_run_with_no_cases_says_so() {
    let root = scratch("none");
    write(&root, "m.hgl", "module m\nfn f() {}\n");
    let output = hgl(&["test", &argument(&root)]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        text(&output.stdout),
        "no case was selected: this run shows nothing\n"
    );
}

/// TST-14: "Other commands treat annotations as comments." `hgl check`
/// reports the annotated error as any other and reads no annotation.
#[test]
fn other_commands_treat_an_annotation_as_a_comment() {
    let root = scratch("comments");
    write(&root, "cases.hgl", PASSING);
    let checked = hgl(&["check", &argument(&root)]);
    assert_eq!(checked.status.code(), Some(1));
    assert!(
        text(&checked.stdout).starts_with("checked 1 file in 1 module: 2 errors\n"),
        "{}",
        text(&checked.stdout)
    );

    let invalid = scratch("comments_invalid");
    write(
        &invalid,
        "m.hgl",
        "module m\n# expect-error(nothing at all\nfn f() {}\n",
    );
    let checked = hgl(&["check", &argument(&invalid)]);
    assert_eq!(text(&checked.stderr), "");
    assert_eq!(checked.status.code(), Some(0));
}

/// The specification's own rejection illustration (tests-and-eval.md):
/// TYPE-11's mixed window sizes, expected as `rolling_window.size_kind`. The
/// declaration is rejected with that code on that line, so the case
/// passes; the test beside it cannot be run yet.
#[test]
fn the_specifications_rejection_illustration_passes() {
    let root = scratch("illustration");
    let file = write(
        &root,
        "rejection.hgl",
        concat!(
            "module examples.rejection\n",
            "\n",
            "# expect-error(type, \"rolling_window.size_kind\")\n",
            "fn invalid(value: rolling_window<f64, 5m, 3>) { when {} }\n",
            "\n",
            "test survives { assert true }\n",
        ),
    );
    let output = hgl(&["test", &argument(&root)]);
    assert_eq!(text(&output.stderr), "");
    // Status 2: a selected test could not be run.
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        text(&output.stdout),
        format!(
            concat!(
                "passed   rejection   {file}:4 fn invalid\n",
                "not run  executable  examples.rejection::survives\n",
                "rejection cases: 1 passed, 0 failed\n",
                "executable cases: 1 not run: no target was given to run a test on\n",
            ),
            file = file
        )
    );

    // TST-15: "A rejection-only run is valid."
    let output = hgl(&["test", &argument(&root), "--rejections-only"]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        text(&output.stdout),
        format!(
            concat!(
                "passed   rejection   {file}:4 fn invalid\n",
                "rejection cases: 1 passed, 0 failed\n",
                "executable cases: none selected\n",
            ),
            file = file
        )
    );

    // A rejection the catalogue has a row for can be a case (TST-12):
    // `list.size` is one since the type-formation rows were agreed.
    let listed = scratch("illustration_listed");
    let file = write(
        &listed,
        "listed.hgl",
        concat!(
            "module examples.listed\n",
            "# expect-error(type, \"list.size\")\n",
            "fn invalid(value: list<f64, -1>) {}\n",
        ),
    );
    let output = hgl(&["test", &argument(&listed)]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        text(&output.stdout),
        format!(
            concat!(
                "passed   rejection   {file}:3 fn invalid\n",
                "rejection cases: 1 passed, 0 failed\n",
                "executable cases: none selected\n",
            ),
            file = file
        )
    );

    // And so can the chapter's own under GEN-13, "using the same
    // generic as both an element type and a list size", since the
    // generic-kind rows were agreed.
    let kinds = scratch("illustration_kinds");
    let file = write(
        &kinds,
        "kinds.hgl",
        concat!(
            "module examples.kinds\n",
            "# expect-error(type, \"generic.kind_conflict\")\n",
            "fn invalid<T>(value: T, items: list<f64, T>) {}\n",
        ),
    );
    let output = hgl(&["test", &argument(&kinds)]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        text(&output.stdout),
        format!(
            concat!(
                "passed   rejection   {file}:3 fn invalid\n",
                "rejection cases: 1 passed, 0 failed\n",
                "executable cases: none selected\n",
            ),
            file = file
        )
    );
}

/// A program with a test that passes, one that fails at its second
/// statement, and one the hgraph target cannot write yet.
const RUNNING: &str = concat!(
    "module examples.running\n",
    "\n",
    "const fn doubled(value: i64) -> i64 => value * 2\n",
    "const fn lone(value: i64) -> i64 => value\n",
    "\n",
    "test holds { assert doubled(2) == 4 }\n",
    "test wrong {\n",
    "    assert doubled(2) == 4\n",
    "    assert doubled(2) == 5\n",
    "}\n",
    "test later { assert eval(lone(value: [0s: 1])) == [0s: 1] }\n",
);

/// TST-14: "crashes, and infrastructure failures cannot prove
/// conformance". With a target and an interpreter that cannot be
/// started, each test is not run and says why; one the target cannot
/// write says what it needs. Nothing failed, and the run is no
/// success: status 2.
#[test]
fn a_host_that_does_not_start_runs_no_test() {
    let root = scratch("no_host");
    let file = write(&root, "cases.hgl", RUNNING);
    let output = hgl(&[
        "test",
        &argument(&root),
        "--target",
        "hgraph",
        "--python",
        "hgl-no-such-python",
    ]);
    assert_eq!(output.status.code(), Some(2));
    let printed = text(&output.stdout);
    let lines: Vec<&str> = printed.lines().collect();
    assert_eq!(lines.len(), 8, "{printed}");
    assert_eq!(lines[0], "not run  executable  examples.running::holds");
    // What the system says of a program that is not there differs from
    // one system to another.
    let reason = "    `hgl-no-such-python` could not be started: ";
    assert!(lines[1].starts_with(reason), "{printed}");
    assert_eq!(lines[2], "not run  executable  examples.running::wrong");
    assert!(lines[3].starts_with(reason), "{printed}");
    assert_eq!(lines[4], "not run  executable  examples.running::later");
    assert_eq!(
        lines[5],
        format!("    {file}:11:38: the hgraph target does not write a timed trace yet")
    );
    assert_eq!(lines[6], "rejection cases: 0 passed, 0 failed");
    assert_eq!(lines[7], "executable cases: 0 passed, 0 failed, 3 not run");
}

/// The interpreter and the directory to keep are the target's, and
/// mean nothing without one.
#[test]
fn a_host_option_needs_a_target() {
    let root = scratch("no_target");
    write(&root, "cases.hgl", RUNNING);
    for option in ["--python", "--keep"] {
        let output = hgl(&["test", &argument(&root), option, "somewhere"]);
        assert_eq!(output.status.code(), Some(2), "{option}");
        assert!(text(&output.stderr).contains("--target"), "{option}");
    }
}

/// TST-15: "Report each selected case's identity, kind, and outcome",
/// and TST-14, "Report executed/rejected counts separately", for tests
/// run on hgraph. A test that fails says which statement failed, and
/// the run is rejected: status 1. What was written for the run is
/// kept where `--keep` says.
///
/// It needs a Python that has hgraph, named by the environment
/// variable `HGL_HGRAPH_PYTHON`, so it is not among the tests run by
/// default: `cargo test -p hgl_cli -- --ignored` runs it.
#[test]
#[ignore = "needs a Python with hgraph, named by HGL_HGRAPH_PYTHON"]
fn tests_run_on_hgraph() {
    let python = std::env::var("HGL_HGRAPH_PYTHON")
        .expect("HGL_HGRAPH_PYTHON names a Python interpreter that has hgraph");
    let root = scratch("on_hgraph");
    let file = write(&root, "cases.hgl", RUNNING);
    let kept = scratch("on_hgraph_kept");
    // `PYTHONOPTIMIZE` has Python remove its own `assert` statements.
    // An HGL assertion is checked all the same: the test that fails
    // still fails.
    let output = hgl_with(
        &[
            "test",
            &argument(&root),
            "--target",
            "hgraph",
            "--python",
            &python,
            "--keep",
            &argument(&kept),
        ],
        &[("PYTHONOPTIMIZE", "2")],
    );
    assert_eq!(
        text(&output.stdout),
        format!(
            concat!(
                "passed   executable  examples.running::holds\n",
                "FAILED   executable  examples.running::wrong\n",
                "    {file}:9:5: the assertion does not hold\n",
                "not run  executable  examples.running::later\n",
                "    {file}:11:38: the hgraph target does not write a timed trace yet\n",
                "rejection cases: 0 passed, 0 failed\n",
                "executable cases: 1 passed, 1 failed, 1 not run\n",
            ),
            file = file
        )
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(
        kept.join("_hgl/_m_examples/_m_running/__init__.py")
            .is_file()
    );

    // Only the test that passes: the run succeeds.
    let output = hgl(&[
        "test",
        &argument(&root),
        "--test",
        "holds",
        "--target",
        "hgraph",
        "--python",
        &python,
    ]);
    assert_eq!(
        text(&output.stdout),
        concat!(
            "passed   executable  examples.running::holds\n",
            "rejection cases: 0 passed, 0 failed\n",
            "executable cases: 1 passed, 0 failed, 0 not run\n",
        )
    );
    assert_eq!(output.status.code(), Some(0));
}
