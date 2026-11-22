//! Selected value candidates own ordinary omission rules (GEN-6, UNIT-8).
mod common;

use common::check;

const CHOICES: &str = r#"module m
operator selected<T, const ordered: bool = false>(value: T, zero: T? = null) -> T
impl const fn selected<const ordered: bool = false>(value: i64, zero: i64? = null) -> i64 requires !ordered => value
impl const fn selected<const ordered: bool = false>(value: i64, zero: i64) -> i64 requires ordered => zero + value
"#;

#[test]
fn the_selected_required_zero_rejects_at_the_call() {
    // UNIT-8, core RED-5: a public null default does not supply the
    // selected ordered candidate's mandatory ordinary argument.
    for call in [
        "selected{ordered: true}(1)",
        "selected{ordered: true}(value: 1)",
    ] {
        let checked = check(&[(
            "m.hgl",
            &format!("{CHOICES}test observed {{ assert {call} == 3 }}\n"),
        )]);
        assert_eq!(checked.findings.len(), 1, "{}", checked.report);
        assert_eq!(checked.findings[0].code, "type/argument.missing");
        assert_eq!(checked.findings[0].rule, "UNIT-8");
        assert!(
            checked.findings[0].message.contains("zero"),
            "{}",
            checked.report
        );
        assert!(
            checked.report.contains("test observed"),
            "{}",
            checked.report
        );
    }
}

#[test]
fn missing_arguments_do_not_select_a_less_specific_candidate() {
    // GEN-5, GEN-6: default availability does not substitute a fallback
    // after the concrete candidate has won signature selection.
    let checked = check(&[(
        "m.hgl",
        r#"module m
operator choose<T>(value: T, zero: T? = null) -> T
impl const fn choose<T>(value: T, zero: T? = null) -> T => value
impl const fn choose(value: i64, zero: i64) -> i64 => value + zero
test observed { assert choose(1) == 1 }
"#,
    )]);
    assert_eq!(checked.findings.len(), 1, "{}", checked.report);
    assert_eq!(checked.findings[0].code, "type/argument.missing");
}

#[test]
fn a_supplied_zero_and_the_optional_candidate_remain_admitted() {
    // GEN-6, UNIT-8: given arguments and nullable/defaulted slots remain valid.
    let checked = check(&[(
        "m.hgl",
        &format!(
            "{CHOICES}test observed {{ assert selected(1) == 1\nassert selected{{ordered: true}}(1, 2) == 3\nassert selected{{ordered: true}}(zero: 2, value: 1) == 3 }}\n"
        ),
    )]);
    assert!(!checked.rejected, "{}", checked.report);
}

#[test]
fn candidate_defaults_and_implicit_nullable_defaults_are_kept() {
    // UNIT-8: each selected candidate can supply its own default or null.
    let checked = check(&[(
        "m.hgl",
        r#"module m
operator defaulted<T>(value: T, zero: T? = null) -> T
impl const fn defaulted(value: i64, zero: i64 = 7) -> i64 => value + zero
operator optional<T>(value: T, zero: T? = null) -> T
impl const fn optional(value: i64, zero: i64?) -> i64 => value
test observed { assert defaulted(1) == 8
assert optional(1) == 1 }
"#,
    )]);
    assert!(!checked.rejected, "{}", checked.report);
}

#[test]
fn inherited_configuration_defaults_are_distinct_from_ordinary_defaults() {
    // GEN-6, FN-18: configuration still inherits the public default.
    let checked = check(&[(
        "m.hgl",
        r#"module m
operator configured<T>{amount: i64 = 7}(value: T) -> T
impl const fn configured{amount: i64}(value: i64) -> i64 => value + amount
test observed { assert configured(1) == 8 }
"#,
    )]);
    assert!(!checked.rejected, "{}", checked.report);
}

#[test]
fn public_constant_selector_defaults_are_kept() {
    // GEN-6: a constant selector retains the public default used in
    // candidate admission; it is not the ordinary runtime zero of RED-5.
    let checked = check(&[(
        "m.hgl",
        r#"module m
enum Mode { slow, fast }
operator step(value: i64, const mode: Mode = .slow) -> i64
impl const fn step(value: i64, mode: Mode) -> i64 requires mode == .slow => value + 1
impl const fn step(value: i64, mode: Mode) -> i64 requires mode == .fast => value + 2
test observed { assert step(1) == 2
assert step(1, mode: .fast) == 3 }
"#,
    )]);
    assert!(!checked.rejected, "{}", checked.report);
}
