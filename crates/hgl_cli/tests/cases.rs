//! Runs `hgl test` the way a user would.
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
/// rejection case to pass." No test can be run yet, so a run that
/// selects one does not succeed: status 2, the command could not run.
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
                "executable cases: 1 not run: this compiler has no target to run a test on yet\n",
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

/// The specification's own rejection illustration (tests-and-eval.md).
/// The runner reads its annotation and finds its owner. The error it
/// expects is a type error, and the analyser has no types yet, so the
/// case fails for now and says why.
#[test]
fn the_specifications_illustration_is_read_and_its_type_error_is_not_reported_yet() {
    let root = scratch("illustration");
    let file = write(
        &root,
        "rejection.hgl",
        concat!(
            "module examples.rejection\n",
            "\n",
            "# expect-error(type, \"rolling.size_kind\")\n",
            "fn invalid(value: rolling<f64, 5m, 3>) { when {} }\n",
            "\n",
            "test survives { assert true }\n",
        ),
    );
    let output = hgl(&["test", &argument(&root)]);
    assert_eq!(text(&output.stderr), "");
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        text(&output.stdout),
        format!(
            concat!(
                "FAILED   rejection   {file}:4 fn invalid\n",
                "    expected type/rolling.size_kind on line 4 of {file}: not reported\n",
                "not run  executable  examples.rejection::survives\n",
                "rejection cases: 0 passed, 1 failed\n",
                "executable cases: 1 not run: this compiler has no target to run a test on yet\n",
            ),
            file = file
        )
    );
}
