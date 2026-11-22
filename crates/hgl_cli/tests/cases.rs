//! Runs `hgl test` the way a user would.
//!
//! The programs are written by the tests when they run, into a scratch
//! directory of their own each.

mod common;

use common::{argument, hgl, hgl_with, scratch, text, write};

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

/// TST-13, TST-14: selection keeps the whole program available to a test
/// and reports only the cases written in the selected files.
#[test]
fn test_paths_select_files_and_recursive_directories() {
    let root = scratch("test_paths");
    write(
        &root,
        "helper.hgl",
        "module selection.helper\nexport const fn answer() -> i64 => 42\n",
    );
    let directory = argument(&root.join("with a space"));
    let first = write(
        &root,
        "with a space/first.hgl",
        concat!(
            "module selection.first\n",
            "use selection.helper::{answer}\n",
            "test first { assert answer() == 42 }\n",
            "test another { assert true }\n",
            "# expect-error(type, \"type.mismatch\")\n",
            "const fn standalone() -> i64 => true\n",
        ),
    );
    let nested = write(
        &root,
        "with a space/nested/inner.hgl",
        "module selection.inner\ntest inner { assert true }\n",
    );
    let outside = write(
        &root,
        "other.hgl",
        concat!(
            "module selection.other\n",
            "test other { assert false }\n",
            "# expect-error(type, \"type.mismatch\")\n",
            "const fn outside_mismatch() -> i64 => 1\n",
        ),
    );
    let root = argument(&root);
    let run = |selectors: &[&str]| {
        let mut arguments = vec!["test", &root];
        arguments.extend_from_slice(selectors);
        hgl(&arguments)
    };

    let file = run(&["--test-path", &first]);
    let printed = text(&file.stdout);
    assert_eq!(file.status.code(), Some(2), "{}", text(&file.stderr));
    assert!(printed.contains("selection.first::first\n"), "{printed}");
    assert!(printed.contains("const fn standalone\n"), "{printed}");
    assert!(!printed.contains("selection.inner::inner"), "{printed}");
    assert!(!printed.contains("outside_mismatch"), "{printed}");

    let folder = run(&["--test-path", &directory]);
    let printed = text(&folder.stdout);
    assert_eq!(folder.status.code(), Some(2));
    assert!(printed.contains("selection.inner::inner\n"), "{printed}");
    assert!(!printed.contains("selection.other::other"), "{printed}");

    let both = run(&["--test-path", &nested, "--test-path", &outside]);
    let printed = text(&both.stdout);
    assert_eq!(both.status.code(), Some(1));
    assert!(printed.contains("selection.inner::inner\n"), "{printed}");
    assert!(printed.contains("selection.other::other\n"), "{printed}");
    assert!(printed.contains("outside_mismatch"), "{printed}");
    assert!(!printed.contains("selection.first::first"), "{printed}");

    let named = run(&["--test-path", &directory, "--test", "first"]);
    let printed = text(&named.stdout);
    assert_eq!(named.status.code(), Some(2));
    assert!(printed.contains("selection.first::first\n"), "{printed}");
    assert!(printed.contains("const fn standalone\n"), "{printed}");
    assert!(!printed.contains("selection.first::another"), "{printed}");
    assert!(!printed.contains("selection.inner::inner"), "{printed}");
}

/// A linked subtree can select a source even when an earlier root
/// supplied the same file through a different path.
#[cfg(unix)]
#[test]
fn a_test_path_matches_an_alternate_discovered_path() {
    let root = scratch("linked_test_path");
    let file = write(
        &root,
        "outside/case.hgl",
        "module m\ntest linked { assert true }\n",
    );
    let directory = root.join("src");
    std::fs::create_dir(&directory).unwrap();
    std::os::unix::fs::symlink(root.join("outside"), directory.join("linked")).unwrap();
    let directory = argument(&directory);
    let output = hgl(&["test", &file, &directory, "--test-path", &directory]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output.stderr));
    assert_eq!(text(&output.stderr), "");
    assert_eq!(
        text(&output.stdout),
        concat!(
            "not run  executable  m::linked\n",
            "rejection cases: 0 passed, 0 failed\n",
            "executable cases: 1 not run: no target was given to run a test on\n",
        )
    );
}

/// TST-13, TST-17: rejected integration tests keep their kind even though
/// their declarations are omitted from the surviving program.
#[test]
fn integration_only_filters_owners_and_combines_with_test_path() {
    let root = scratch("integration_only");
    let selected = write(
        &root,
        "chosen.hgl",
        concat!(
            "module modes.chosen\n",
            "# expect-error(type, \"type.mismatch\")\n",
            "const fn standalone() -> i64 => true\n",
            "test ordinary { assert false }\n",
            "test ordinary_rejection {\n",
            "    # expect-error(type, \"type.mismatch\")\n",
            "    assert 1\n",
            "}\n",
            "integration_test integrated { assert true }\n",
            "integration_test integrated_rejection {\n",
            "    # expect-error(type, \"type.mismatch\")\n",
            "    assert 1\n",
            "}\n",
        ),
    );
    write(
        &root,
        "other.hgl",
        "module modes.other\nintegration_test other { assert true }\n",
    );
    let root = argument(&root);
    let defaults = hgl(&["test", &root]);
    let printed = text(&defaults.stdout);
    assert_eq!(
        defaults.status.code(),
        Some(2),
        "{}",
        text(&defaults.stderr)
    );
    assert!(printed.contains("ordinary_rejection\n"), "{printed}");
    assert!(
        printed.contains("passed   rejection   modes.chosen::integrated_rejection\n"),
        "{printed}"
    );

    let all = hgl(&["test", &root, "--integration-only"]);
    let printed = text(&all.stdout);
    assert_eq!(all.status.code(), Some(2));
    assert!(printed.contains("modes.other::other\n"), "{printed}");
    assert!(printed.contains("modes.chosen::integrated\n"), "{printed}");
    assert!(
        printed.contains("passed   rejection   modes.chosen::integrated_rejection\n"),
        "{printed}"
    );
    assert!(!printed.contains("ordinary"), "{printed}");
    assert!(!printed.contains("standalone"), "{printed}");

    let path = hgl(&[
        "test",
        &root,
        "--integration-only",
        "--test-path",
        &selected,
    ]);
    let printed = text(&path.stdout);
    assert_eq!(path.status.code(), Some(2));
    assert!(printed.contains("modes.chosen::integrated\n"), "{printed}");
    assert!(!printed.contains("modes.other::other"), "{printed}");

    let rejections = hgl(&[
        "test",
        &root,
        "--integration-only",
        "--rejections-only",
        "--test-path",
        &selected,
    ]);
    assert_eq!(rejections.status.code(), Some(0));
    assert_eq!(
        text(&rejections.stdout),
        concat!(
            "passed   rejection   modes.chosen::integrated_rejection\n",
            "rejection cases: 1 passed, 0 failed\n",
            "executable cases: none selected\n",
        )
    );

    let conflict = hgl(&["test", &root, "--integration", "--integration-only"]);
    assert_eq!(conflict.status.code(), Some(2));
    assert!(text(&conflict.stderr).contains("--integration-only"));
}

/// TST-13, TST-17: the integration test's annotated rejection is checked
/// without running the test. A wrong expectation still fails a normal run.
#[test]
fn ordinary_cpp_runs_check_integration_rejections() {
    let root = scratch("ordinary_integration_rejection");
    let source = write(
        &root,
        "src/m.hgl",
        concat!(
            "module m\n",
            "integration_test wrong {\n",
            "    # expect-error(type, \"type.mismatch\")\n",
            "    assert true\n",
            "}\n",
        ),
    );
    let cache = argument(&root.join("cache"));
    let source_root = argument(&root.join("src"));
    for options in [
        &[][..],
        &["--rejections-only"][..],
        &["--test-path", source.as_str()][..],
    ] {
        let mut arguments = vec!["test", &source_root, "--target", "cpp"];
        arguments.extend_from_slice(options);
        let output = hgl_with(&arguments, &[("HGL_NATIVE_CACHE", &cache)]);
        let printed = text(&output.stdout);
        assert_eq!(output.status.code(), Some(1), "{printed}");
        assert!(
            printed.starts_with("FAILED   rejection   m::wrong\n"),
            "{printed}"
        );
        assert!(
            printed.contains("executable cases: none selected\n"),
            "{printed}"
        );
    }
}

/// TST-15: a valid path with no selected cases succeeds. Invalid paths
/// are usage errors, even when the native target was requested.
#[test]
fn empty_and_invalid_test_paths_are_reported_before_building() {
    let root = scratch("test_paths_empty");
    let source = write(
        &root,
        "src/m.hgl",
        "module m\ntest ordinary { assert true }\n",
    );
    let source_root = root.join("src");
    let empty = source_root.join("empty");
    std::fs::create_dir(&empty).unwrap();
    let outside = write(&root, "outside.hgl", "module outside\n");
    let source_root = argument(&source_root);
    let empty = argument(&empty);
    let missing = argument(&root.join("src/missing.hgl"));
    let workspace = root.join("build");
    let kept = argument(&workspace);
    for options in [
        vec!["--test-path", &empty],
        vec!["--integration-only", "--test-path", &source],
    ] {
        let mut arguments = vec!["test", &source_root];
        arguments.extend(options);
        let output = hgl(&arguments);
        assert_eq!(output.status.code(), Some(0));
        assert_eq!(
            text(&output.stdout),
            concat!(
                "no case was selected: this run shows nothing\n",
                "rejection cases: 0 passed, 0 failed\n",
                "executable cases: 0 passed, 0 failed, 0 not run\n",
            )
        );
    }

    for arguments in [
        vec!["test", &empty],
        vec![
            "test",
            &empty,
            "--test-path",
            &empty,
            "--target",
            "cpp",
            "--keep",
            &kept,
        ],
    ] {
        let output = hgl(&arguments);
        assert_eq!(output.status.code(), Some(2));
        assert!(text(&output.stderr).contains(&format!("no .hgl file under {empty}")));
        assert!(output.stdout.is_empty());
        assert!(!workspace.exists());
    }

    for invalid in [&missing, &outside] {
        let output = hgl(&[
            "test",
            &source_root,
            "--test-path",
            invalid,
            "--target",
            "cpp",
            "--keep",
            &kept,
        ]);
        assert_eq!(output.status.code(), Some(2));
        assert!(text(&output.stderr).contains(invalid));
        assert!(output.stdout.is_empty());
        assert!(!workspace.exists());
    }
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

/// The directory to keep is the target's, and means nothing without
/// one.
#[test]
fn a_host_option_needs_a_target() {
    let root = scratch("no_target");
    write(&root, "cases.hgl", PASSING);
    let output = hgl(&["test", &argument(&root), "--keep", "somewhere"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(text(&output.stderr).contains("--target"));
}

/// FN-8, GEN-10 and GEN-17: dependent call packs still enforce the callback's
/// argument matching, the pack's cardinality and the instance's requirements.
#[test]
fn dependent_call_packs_reject_invalid_arguments() {
    let root = scratch("dependent-pack-rejections");
    let declarations = write(
        &root,
        "demo.packdepends.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.packdepends.hgl"),
    );
    let cases = write(
        &root,
        "demo.packdepends_rejections.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.packdepends_rejections.hgl"),
    );
    let output = hgl(&["test", &declarations, &cases, "--rejections-only"]);
    let stdout = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{stdout}{}",
        text(&output.stderr)
    );
    assert!(
        stdout.contains("rejection cases: 11 passed, 0 failed"),
        "{stdout}"
    );
}

/// FN-19: concrete pack keys use the ordinary call diagnostics and
/// an operand must name a pack.
#[test]
fn forwarded_packs_reject_invalid_arguments() {
    let root = scratch("forwarded-pack-rejections");
    let file = write(
        &root,
        "demo.forwarded_packs_rejections.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.forwarded_packs_rejections.hgl"),
    );
    let output = hgl(&["test", &file, "--rejections-only"]);
    let stdout = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{stdout}{}",
        text(&output.stderr)
    );
    assert!(
        stdout.contains("rejection cases: 9 passed, 0 failed"),
        "{stdout}"
    );
}

/// FN-17, FN-18 and GEN-17: reflected callbacks reject unknown
/// anonymous inputs, invalid options, and a false forwarded requirement.
#[test]
fn reflected_pack_followups_reject_invalid_calls() {
    let root = scratch("reflected-pack-followup-rejections");
    let file = write(
        &root,
        "demo.reflected_pack_followups.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.reflected_pack_followups.hgl"),
    );
    let output = hgl(&["test", &file, "--rejections-only"]);
    let stdout = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{stdout}{}",
        text(&output.stderr)
    );
    assert!(
        stdout.contains("rejection cases: 9 passed, 0 failed"),
        "{stdout}"
    );
}

/// GEN-17: a member lookup of a name missing from the bound pack reports
/// the catalogue code.
#[test]
fn a_missing_reflected_pack_member_is_reported() {
    let root = scratch("dependent-pack-member");
    let file = write(
        &root,
        "demo.packdepends_missing_member.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.packdepends_missing_member.hgl"),
    );
    let output = hgl(&["check", &file, "--json"]);
    let stdout = text(&output.stdout);
    assert_eq!(output.status.code(), Some(1), "{stdout}");
    let report: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let diagnostics = report["diagnostics"].as_array().unwrap();
    assert_eq!(diagnostics.len(), 1, "{stdout}");
    assert_eq!(diagnostics[0]["code"], "pack.name", "{stdout}");
    assert_eq!(diagnostics[0]["category"], "type", "{stdout}");
}

/// GEN-7, GEN-10 and GEN-18: missing existential matches and law
/// guarantees reject at the specified declaration or call.
#[test]
fn any_and_operator_law_rejections_are_checked() {
    let root = scratch("any_laws_rejections");
    let file = write(
        &root,
        "demo.any_laws.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.any_laws.hgl"),
    );
    let output = hgl(&["test", &file, "--rejections-only"]);
    let stdout = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{stdout}{}",
        text(&output.stderr)
    );
    assert!(
        stdout.contains("rejection cases: 17 passed, 0 failed\n"),
        "{stdout}"
    );
}
