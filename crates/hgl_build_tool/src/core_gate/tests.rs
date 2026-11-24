//! Keep the core gate's case accounting and the suite root selection stable.

use super::*;

const DISCOVERY: &str =
    "not run  executable  conformance::ready\nnot run  executable  conformance::pending\n";
const PASSING: &str = "passed   executable  conformance::ready\n";
const PENDING: &str = "not run  executable  conformance::pending\n";
const REASON: &str = "    fixture.hgl:3:4: the cpp target does not write a graph switch yet\n";
const REJECTION: &str = "spec/hgl_core/tests/conformance/rejections/children.hgl:46 fn whole";
const NOT_REJECTED: &str = "    expected phase/payload.unguarded on line 49 of spec/hgl_core/tests/conformance/rejections/children.hgl: not reported\n";

fn checked(
    inventory: &str,
    rows: &str,
    detail: &str,
    (rejected, failed_rejections): (usize, usize),
    (passed, failed, waiting): (usize, usize, usize),
    statuses: (i32, i32),
    unrejected: &BTreeSet<&str>,
) -> Vec<String> {
    let discovered = Run {
        status: statuses.0,
        stdout: inventory.to_owned(),
    };
    let execution = Run {
        status: statuses.1,
        stdout: format!(
            "{rows}{detail}rejection cases: {rejected} passed, {failed_rejections} failed\n\
             executable cases: {passed} passed, {failed} failed, {waiting} not run\n"
        ),
    };
    let mut expected = BTreeMap::from([("conformance::pending", "a graph switch")]);
    expected.extend(
        unrejected
            .iter()
            .map(|&name| (name, "a whole read needs `all_valid` (TIME-1)")),
    );
    verify(&discovered, &execution, &expected, unrejected).unwrap()
}

#[test]
fn each_invocation_has_only_its_own_suite_root() {
    // Explicitly naming core would also select its examples (TST-14, SDOC-9).
    for suite in SUITES {
        let root = Path::new("pinned-core");
        let expected = vec![
            OsString::from("test"),
            root.join("tests").join(suite).into_os_string(),
            OsString::from("--integration"),
        ];
        assert_eq!(suite_args(root, suite, false), expected);
        assert_eq!(
            suite_args(root, suite, true),
            expected
                .into_iter()
                .chain(["--target".into(), "cpp".into()])
                .collect::<Vec<_>>(),
        );
    }
}

#[test]
fn accepts_only_the_reviewed_case_and_source_located_reason() {
    let rows = format!("{PASSING}{PENDING}");
    let no_rejections = BTreeSet::new();
    let problems = |detail, counts, code| {
        checked(
            DISCOVERY,
            &rows,
            detail,
            (0, 0),
            counts,
            (2, code),
            &no_rejections,
        )
    };
    assert!(problems(REASON, (1, 0, 1), 2).is_empty());
    for wrong in [
        "    the interpreter could not be started\n",
        "    the cpp target does not write a graph switch yet\n",
        "    fixture.hgl:3:4: the cpp target does not write another construct yet\n",
    ] {
        assert!(!problems(wrong, (1, 0, 1), 2).is_empty(), "{wrong}");
    }
    assert!(!problems(REASON, (1, 0, 1), 1).is_empty());
    assert!(!problems(REASON, (2, 0, 1), 2).is_empty());
}

#[test]
fn detects_failed_new_missing_and_newly_passing_cases() {
    let no_rejections = BTreeSet::new();
    let failed = format!("{}{PENDING}", PASSING.replace("passed", "FAILED"));
    assert!(
        !checked(
            DISCOVERY,
            &failed,
            REASON,
            (0, 0),
            (0, 1, 1),
            (2, 1),
            &no_rejections
        )
        .is_empty()
    );
    let different = format!("{PASSING}{}", PENDING.replace("::pending", "::another"));
    assert!(
        !checked(
            DISCOVERY,
            &different,
            REASON,
            (0, 0),
            (1, 0, 1),
            (2, 2),
            &no_rejections
        )
        .is_empty()
    );

    let reduced = "not run  executable  conformance::ready\n";
    assert!(
        !checked(
            reduced,
            PASSING,
            "",
            (0, 0),
            (1, 0, 0),
            (2, 0),
            &no_rejections
        )
        .is_empty()
    );
    let passing = format!("{PASSING}{}", PASSING.replace("::ready", "::pending"));
    assert!(
        !checked(
            DISCOVERY,
            &passing,
            "",
            (0, 0),
            (2, 0, 0),
            (2, 0),
            &no_rejections
        )
        .is_empty()
    );
}

#[test]
fn a_suite_that_did_not_list_cases_is_rejected() {
    let stopped = Run {
        status: 1,
        stdout: "no case was run\n".into(),
    };
    assert!(
        !verify(&stopped, &stopped, &BTreeMap::new(), &BTreeSet::new())
            .unwrap()
            .is_empty()
    );
}

fn rejection_problems(
    row: &str,
    diagnosis: &str,
    rejection_counts: (usize, usize),
    listed: BTreeSet<&str>,
) -> Vec<String> {
    let rows = format!("{row}{diagnosis}{PASSING}{PENDING}");
    let discovery = format!("{row}{DISCOVERY}");
    let status = if row.starts_with("FAILED") { 1 } else { 2 };
    checked(
        &discovery,
        &rows,
        REASON,
        rejection_counts,
        (1, 0, 1),
        (status, status),
        &listed,
    )
}

#[test]
fn a_listed_rejection_must_fail_by_not_being_rejected() {
    let name = BTreeSet::from(["conformance::rejections/children.hgl::whole"]);
    let failed = format!("FAILED   rejection   {REJECTION}\n");
    assert!(rejection_problems(&failed, NOT_REJECTED, (0, 1), name.clone()).is_empty());

    let passed = format!("passed   rejection   {REJECTION}\n");
    assert!(!rejection_problems(&passed, "", (1, 0), name.clone()).is_empty());
    let wrong = "    expected phase/payload.unguarded on line 49: found type/type.mismatch\n";
    assert!(!rejection_problems(&failed, wrong, (0, 1), name.clone()).is_empty());
    assert!(!rejection_problems(&failed, NOT_REJECTED, (0, 1), BTreeSet::new()).is_empty());
    assert!(!rejection_problems("", "", (0, 0), name).is_empty());
}

#[test]
fn a_listed_rejection_finds_a_const_function_by_its_name() {
    let row = "FAILED   rejection   spec/hgl_core/tests/conformance/rejections/instances.hgl:21 const fn climbed\n";
    let detail = "    expected type/generic.instance_limit on line 23 of spec/hgl_core/tests/conformance/rejections/instances.hgl: not reported\n";
    let listed = BTreeSet::from(["conformance::rejections/instances.hgl::climbed"]);
    assert!(rejection_problems(row, detail, (0, 1), listed).is_empty());
}

#[test]
fn accepts_the_cpp_targets_words_even_when_every_case_is_listed() {
    let rows = "not run  executable  conformance::pending\n";
    let reason = "    fixture.hgl:3:4: the cpp target does not write an eval yet\n";
    let counts =
        "rejection cases: 0 passed, 0 failed\nexecutable cases: 0 passed, 0 failed, 1 not run\n";
    let inventory = Run {
        status: 2,
        stdout: rows.into(),
    };
    let result = Run {
        status: 2,
        stdout: format!("{rows}{reason}{counts}"),
    };
    let expected = BTreeMap::from([("conformance::pending", "an eval")]);
    assert!(
        verify(&inventory, &result, &expected, &BTreeSet::new())
            .unwrap()
            .is_empty()
    );
    assert_eq!(gaps_of(&result).unwrap(), expected);

    let other = Run {
        status: 2,
        stdout: result.stdout.replace("cpp target", "other target"),
    };
    assert!(
        !verify(&inventory, &other, &expected, &BTreeSet::new())
            .unwrap()
            .is_empty()
    );
    assert!(gaps_of(&other).unwrap().is_empty());
}

#[test]
fn writes_only_cpp_gaps_and_preserves_rejection_comments() -> Result<()> {
    let scratch = tempfile::tempdir()?;
    let file = scratch.path().join("unsupported_cases.toml");
    let old = "# Header\n\n\"stale reason\" = [\"conformance::stale\"]\n\n# Rejection header\n[rejections]\n\"missing\" = [\"conformance::x::name\"]\n";
    let known = toml::from_str(old)?;
    let gaps = BTreeMap::from([("conformance::pending", "an \"eval\"")]);
    write_list(&file, old, &gaps, &known)?;
    assert_eq!(
        fs::read_to_string(&file)?,
        "# Header\n\n\"an \\\"eval\\\"\" = [\n    \"conformance::pending\",\n]\n\n# Rejection header\n[rejections]\n\"missing\" = [\"conformance::x::name\"]\n",
    );
    Ok(())
}
