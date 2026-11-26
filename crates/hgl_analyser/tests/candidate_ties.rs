//! GEN-6 declaration warnings reuse the candidate choice made by real calls.

mod common;

use common::{Checked, check, module};

const CROSS: &str = "operator amb<A, B>(a: A, b: B) -> i64\n\
impl const fn amb<B>(a: i64, b: B) -> i64 => 1\n\
impl const fn amb<A>(a: A, b: i64) -> i64 => 2\n";

fn warnings(checked: &Checked) -> Vec<&str> {
    checked
        .findings
        .iter()
        .filter(|finding| finding.code == "warning/operator.candidates_tie")
        .map(|finding| finding.at.rsplit_once(':').unwrap().0)
        .collect()
}

/// GEN-6: incomparable signatures warn even without a call. A concrete
/// third candidate can take the witness and remove the warnings.
#[test]
fn concrete_overlap_warns_until_a_more_specific_candidate_takes_it() {
    let checked = module(CROSS);
    assert_eq!(warnings(&checked).len(), 2, "{}", checked.report);
    assert!(!checked.rejected, "{}", checked.report);
    let checked = module(&format!("{CROSS}const fn use_pair() -> i64 => amb(1, 1)"));
    assert_eq!(warnings(&checked).len(), 2, "{}", checked.report);
    assert!(
        checked
            .findings
            .iter()
            .any(|finding| finding.code == "operator/operator.ambiguous"),
        "{}",
        checked.report
    );

    let checked = module(&format!(
        "{CROSS}impl const fn amb(a: i64, b: i64) -> i64 => 3\n\
         const fn use_pair() -> i64 => amb(1, 1)\n"
    ));
    assert!(checked.findings.is_empty(), "{}", checked.report);
}

/// GEN-6: the all-default call is a witness even when the full
/// signatures cannot take the same pair of ordinary arguments.
#[test]
fn omitted_arguments_can_tie_candidates_with_different_types() {
    let definitions = "operator select_pair<A, B>(a: A, b: B) -> i64\n\
        impl const fn select_pair(a: i64 = 1, b: i64 = 1) -> i64 => 1\n\
        impl const fn select_pair(a: f64 = 1.0, b: f64 = 2.0) -> i64 => 2\n";
    let checked = module(definitions);
    assert_eq!(warnings(&checked).len(), 2, "{}", checked.report);
    assert!(!checked.rejected, "{}", checked.report);
    let checked = module(&format!(
        "{definitions}const fn use_pair() -> i64 => select_pair()\n"
    ));
    assert!(
        checked
            .findings
            .iter()
            .any(|finding| finding.code == "operator/operator.ambiguous"),
        "{}",
        checked.report
    );

    let required = definitions.replace("a: f64 = 1.0", "a: f64");
    let checked = module(&required);
    assert!(checked.findings.is_empty(), "{}", checked.report);

    let generic = concat!(
        "operator choice<A, B>(a: A, b: B) -> i64\n",
        "impl const fn choice<A>(a: A = 1, b: i64 = 1) -> i64 => 1\n",
        "impl const fn choice<B>(a: i64 = 1, b: B = 1) -> i64 => 2\n",
    );
    let checked = module(generic);
    assert_eq!(warnings(&checked).len(), 2, "{}", checked.report);
    let checked = module(&format!(
        "{generic}impl const fn choice(a: i64 = 1, b: i64 = 1) -> i64 => 3\n\
         const fn use_choice() -> i64 => choice()\n"
    ));
    assert!(checked.findings.is_empty(), "{}", checked.report);
}

/// GEN-6 and ERR-10: candidates in different modules can tie. An
/// acknowledgement on one declaration suppresses only its own warning.
#[test]
fn candidates_in_two_modules_acknowledge_warnings_independently() {
    let public = "module public_api\nexport operator amb<A, B>(a: A, b: B) -> i64\n";
    let left = "module left_candidate\nuse public_api::{amb}\n\
                impl const fn amb<B>(a: i64, b: B) -> i64 => 1\n";
    let right = "module right_candidate\nuse public_api::{amb}\n\
                 impl const fn amb<A>(a: A, b: i64) -> i64 => 2\n";
    let checked = check(&[
        ("public.hgl", public),
        ("left.hgl", left),
        ("right.hgl", right),
    ]);
    assert!(!checked.rejected, "{}", checked.report);
    assert_eq!(
        warnings(&checked),
        ["left.hgl:3", "right.hgl:3"],
        "{}",
        checked.report
    );

    let left = left.replace(
        "impl const fn",
        "@suppress_warning(\"operator.candidates_tie\")\nimpl const fn",
    );
    let checked = check(&[
        ("public.hgl", public),
        ("left.hgl", &left),
        ("right.hgl", right),
    ]);
    assert_eq!(warnings(&checked), ["right.hgl:3"], "{}", checked.report);
}

/// GEN-5 and FN-4: equal parameter types in different callable roles
/// are resolved by the call's context.
#[test]
fn value_and_temporal_candidates_do_not_warn_for_equal_inputs() {
    let checked = module(concat!(
        "operator select_role(value: i64) -> i64\n",
        "impl const fn select_role(value: i64) -> i64 => 1\n",
        "impl fn select_role(value: i64) -> i64 => value\n",
        "const fn scalar() -> i64 => select_role(2)\n",
        "fn temporal(value: i64) -> i64 => select_role(value)\n",
    ));
    assert!(checked.findings.is_empty(), "{}", checked.report);
}

/// TST-1 and GEN-6: a declaration witness sees the module's test
/// candidates both for the operator it checks and through requirements.
#[test]
fn declaration_witnesses_resolve_requirements_in_the_test_view() {
    let declarations = concat!(
        "operator choose<A, B>(a: A, b: B) -> i64\n",
        "operator helper<X>(x: X) -> i64\n",
    );
    let crossed = concat!(
        "impl const fn choose<T>(a: i64, b: T) -> i64 => 1\n",
        "impl const fn choose<T>(a: T, b: i64) -> i64 => 2\n",
    );
    let concrete = "impl const fn choose(a: i64, b: i64) -> i64 requires helper(i64) -> i64 => 3\n";
    let checked = module(&format!(
        "{declarations}test local {{\n\
         impl const fn helper(x: i64) -> i64 => 99\n\
         {crossed}{concrete}assert choose(1, 1) == 3\n}}\n"
    ));
    assert!(checked.findings.is_empty(), "{}", checked.report);

    let checked = module(&format!(
        "{declarations}test local {{\n\
         impl const fn helper(x: str) -> i64 => 99\n\
         {crossed}{concrete}assert choose(1, 1) == 1\n}}\n"
    ));
    assert_ambiguous(&checked);

    let guarded = crossed.replace("-> i64 =>", "-> i64 requires helper(T) -> i64 =>");
    let checked = module(&format!(
        "{declarations}test local {{\n\
         impl const fn helper(x: i64) -> i64 => 99\n\
         {guarded}assert choose(1, 1) == 1\n}}\n"
    ));
    assert_ambiguous(&checked);
}

fn assert_ambiguous(checked: &Checked) {
    assert_eq!(warnings(checked).len(), 2, "{}", checked.report);
    assert!(
        checked
            .findings
            .iter()
            .any(|finding| finding.code == "operator/operator.ambiguous"),
        "{}",
        checked.report
    );
}

/// TST-1: a test view can supply requirement evidence even when all
/// candidates of the operator being checked are production declarations.
#[test]
fn test_only_requirement_candidates_supply_a_view_for_production_candidates() {
    let definitions = concat!(
        "operator helper<T>(value: T) -> i64\n",
        "operator choose<A, B>(a: A, b: B) -> i64\n",
        "impl const fn choose<T>(a: i64, b: T) -> i64 requires helper(T) -> i64 => 1\n",
        "impl const fn choose<T>(a: T, b: i64) -> i64 requires helper(T) -> i64 => 2\n",
    );
    let checked = module(definitions);
    assert!(checked.findings.is_empty(), "{}", checked.report);
    let checked = module(&format!(
        "{definitions}test local {{\n\
         impl const fn helper(value: i64) -> i64 => value\n\
         assert choose(1, 1) == 1\n}}\n"
    ));
    assert_ambiguous(&checked);
}

/// GEN-6 and UNIT-8: ordinary omitted-argument admission uses the
/// operator's default even when candidates declare none of their own.
#[test]
fn an_inherited_public_default_is_a_tie_witness() {
    let checked = module(concat!(
        "operator choose<T>(x: T = 1) -> i64\n",
        "impl const fn choose(x: i64) -> i64 => 1\n",
        "impl const fn choose(x: f64) -> i64 => 2\n",
        "const fn use_pair() -> i64 => choose()\n",
    ));
    assert_ambiguous(&checked);
}
