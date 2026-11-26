//! Finite forwarded callback contexts retain independent type obligations
//! (GEN-7, FN-17/19). These are ordinary calls, without recursive growth.

mod common;

use common::check;

/// Named targets, anonymous sources and concrete type packs remain separate.
#[test]
fn finite_forwarded_callbacks_keep_their_contexts() {
    let checked = check(&[(
        "callbacks.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.forwarded_callback_contexts.hgl"),
    )]);
    assert!(checked.findings.is_empty(), "{}", checked.report);
}

/// FN-17/19: a good forwarding context does not hide a second pack's
/// wrong element type, nor an unrelated ordinary return mismatch (EXPR-5).
#[test]
fn independent_callback_and_value_errors_still_report() {
    let source = concat!(
        "module independent_callbacks\n",
        "const fn apply<T, ...As>(f: const fn(T) -> T, values: ...As) -> T => f(...values)\n",
        "const fn wrap<T, ...As>(f: const fn(T) -> T, values: ...As) -> T => apply<T>(fn(value) => f(value), ...values)\n",
        "const fn identity(value: i64) -> i64 => value\n",
        "const fn good() -> i64 => wrap<i64>(identity, 3)\n",
        "const fn wrong_pack() -> i64 => wrap<i64>(identity, \"bad\")\n",
        "const fn wrong_value() -> i64 => \"bad\"\n",
    );
    let checked = check(&[("independent.hgl", source)]);
    assert!(checked.rejected, "{}", checked.report);
    assert!(checked.findings.len() >= 2, "{}", checked.report);
    assert!(
        checked.findings.iter().any(|finding| {
            finding.at.starts_with("independent.hgl:7:") && finding.code == "type/type.mismatch"
        }),
        "{}",
        checked.report
    );
    assert!(
        checked.report.contains("wrap<i64>(identity, \"bad\")"),
        "{}",
        checked.report
    );
}

/// FN-1, FMT-3/4: independent captured constants keep independent
/// formatting checks. The catalogue's existing format.syntax spelling
/// is proposed; this test does not change that status.
#[test]
fn captured_const_inputs_keep_independent_diagnostics() {
    let checked = check(&[(
        "patterns.hgl",
        concat!(
            "module captured_patterns\n",
            "const fn apply<...As>(f: const fn(i64) -> str, values: ...As) -> str => f(...values)\n",
            "const fn captured<...As>(const pattern: str, values: ...As) -> str => apply(fn(value) => format(pattern, value), ...values)\n",
            "const fn valid() -> str => captured(\"{}\", 3)\n",
            "const fn bad_type() -> str => captured(\"{:c}\", 3)\n",
            "const fn bad_syntax() -> str => captured(\"{\", 3)\n",
        ),
    )]);
    assert!(
        checked
            .findings
            .iter()
            .any(|finding| finding.code == "type/format.specifier"),
        "{}",
        checked.report
    );
    assert!(
        checked
            .findings
            .iter()
            .any(|finding| finding.code == "type/format.syntax"),
        "{}",
        checked.report
    );
}
