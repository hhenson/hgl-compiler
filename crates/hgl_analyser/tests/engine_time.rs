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

#[test]
fn eval_input_values_have_no_cycle_time() {
    // FMT-9, TST-3: replay values and const arguments are made before
    // a run, even while its target call is read as composition.
    for (source, count) in [
        (
            r#"fn id(s: str) -> str => s
test t { assert eval id(s: [format("{engine_time:%S}")]) == ["x"] }"#,
            1,
        ),
        (
            r#"fn id(const s: str, v: i64) -> str => format("{} {}", s, v)
test t { assert eval id(s: format("{engine_time:%S}"), v: [1]) == ["x"] }"#,
            1,
        ),
        (
            r#"fn id(s: str) -> str => s
test t {
    const opening: datetime = @2026-01-02T03:04:05Z
    assert eval{start: opening, length: 4s} id(s: [0s: format("{engine_time:%S} {}", 1), 2s: format("{engine_time:%S} {}", 2)]) == [0s: "05 1", 2s: "07 2"]
}"#,
            2,
        ),
    ] {
        let checked = module(source);
        assert_eq!(checked.findings.len(), count, "{}", checked.report);
        assert!(
            checked
                .findings
                .iter()
                .all(|finding| finding.code == "type/format.argument"),
            "{}",
            checked.report
        );
    }
    // FMT-9: the process clock remains available to these value contexts.
    let checked = module(
        r#"fn id(const s: str, v: str) -> str => format("{} {}", s, v)
test t { assert eval id(s: format("{wall_time:%S}"), v: [format("{wall_time:%S}")]) == ["x"] }"#,
    );
    assert!(checked.findings.is_empty(), "{}", checked.report);
}

#[test]
fn only_cores_private_declaration_receives_the_renderer_clock() {
    // UNIT-3, FMT-9: an application's same-named native is an ordinary call.
    // The literal would be rejected as an unmatched brace by the format checker.
    let checked = common::check(&[(
        "m.hgl",
        r#"module m part formatting
native const fn render_graph_values(const fmt_str: str, engine_time: datetime) -> str
const fn local() -> str => render_graph_values("{", @2026-01-01T00:00:00Z)"#,
    )]);
    assert!(checked.findings.is_empty(), "{}", checked.report);

    let checked =
        module(r#"const fn external() -> str => render_graph_values("{}", @2026-01-01T00:00:00Z)"#);
    assert!(checked.rejected, "{}", checked.report);
    assert!(
        checked.findings.iter().any(|f| f.code == "name/name.unknown"),
        "{}",
        checked.report
    );
}
