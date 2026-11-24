//! FMT-9: evaluation-time places depend on the call's graph context.

mod common;

use common::module;

#[test]
fn graph_format_calls_admit_evaluation_time_as_a_datetime() {
    // FMT-9: reserved clock places consume none of the call's arguments.
    let checked = module(
        r#"fn describe(value: i64) -> str => format("{engine_time:%S} {} {wall_time:%S}", value)
fn generic<const P: str>(value: i64) -> str => format(P, value)
fn forwarded(value: i64) -> str => generic<"{engine_time:%H:%M:%S} {}">(value)"#,
    );
    assert!(checked.findings.is_empty(), "{}", checked.report);
}

#[test]
fn value_functions_and_handlers_still_reject_evaluation_time() {
    // FMT-9: value code has no graph evaluation-time operand.
    for source in [
        r#"const fn describe(value: i64) -> str => format("{engine_time:%S} {}", value)"#,
        r#"fn describe(value: i64) -> str {
    when modified(value) { return format("{engine_time:%S} {}", value) }
}"#,
    ] {
        let checked = module(source);
        assert_eq!(checked.findings.len(), 1, "{}", checked.report);
        assert_eq!(checked.findings[0].code, "type/format.argument");
        assert!(checked.report.contains("FMT-9"), "{}", checked.report);
    }
}

#[test]
fn engine_time_keeps_datetime_specifiers_and_unused_argument_checks() {
    // FMT-4, FMT-9: engine_time has datetime's specifiers, not i64's.
    let checked = module(r#"fn bad(value: i64) -> str => format("{engine_time:x} {}", value)"#);
    assert_eq!(checked.findings.len(), 1, "{}", checked.report);
    assert_eq!(checked.findings[0].code, "type/format.specifier");
    // FMT-3: engine_time does not use a supplied argument.
    let checked = module(r#"fn bad(value: i64) -> str => format("{engine_time}", value)"#);
    assert_eq!(checked.findings.len(), 1, "{}", checked.report);
    assert_eq!(checked.findings[0].code, "type/format.unused");
}
