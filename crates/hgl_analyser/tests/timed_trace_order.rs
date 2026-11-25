//! Source ordering of eval input and expectation keys (TST-7).

mod common;

use common::check;

/// Reads one trace on either side, with a valid trace on the other.
fn findings(trace: &str, expectation: bool) -> Vec<String> {
    let (input, expected) = if expectation {
        ("[0us: 1]", trace)
    } else {
        (trace, "[0us: 1]")
    };
    check(&[(
        "m.hgl",
        &format!(
            "module m\n\
             const EARLIER: datetime = @2026-01-01T00:00:01Z\n\
             const LATER: datetime = @2026-01-01T00:00:02Z\n\
             fn pass(value: i64) -> i64 => value\n\
             test order {{\n\
                 let dynamic = @2026-01-01T00:00:01Z\n\
                 assert eval pass(value: {input}) == {expected}\n\
             }}\n"
        ),
    )])
    .summary()
}

/// TST-7: keys strictly increase in inputs and expectations, using
/// instant ordering for datetimes rather than their written local time.
#[test]
fn constant_timed_trace_keys_strictly_increase() {
    let mut missing = Vec::new();
    for trace in [
        "[2us: 2, 1us: 1]",
        "[1us: 1, 1us: 2]",
        "[@2026-01-01T00:00:02Z: 2, @2026-01-01T00:00:01Z: 1]",
        "[@2026-01-01T00:00:01Z: 1, @2026-01-01T00:00:01Z: 2]",
        "[@2026-01-01T01:00:00+01:00: 1, @2026-01-01T00:00:00Z: 2]",
        "[@2026-01-01T00:00:00Z: 1, @2026-01-01T00:30:00+01:00: 2]",
        "[LATER: 2, EARLIER: 1]",
    ] {
        for expectation in [false, true] {
            let summary = findings(trace, expectation);
            if summary.len() != 1 || !summary[0].ends_with(" type/trace.key_order TST-7") {
                missing.push(format!("{trace}, expectation={expectation}: {summary:?}"));
            }
        }
    }
    assert!(missing.is_empty(), "{}", missing.join("\n"));
}

/// TST-7: increasing keys pass. Keys not known in source keep their
/// execution-time ordering check, including equal dynamic keys.
#[test]
fn ordered_and_dynamic_timed_trace_keys_keep_their_rules() {
    for trace in [
        "[1us: 1, 2us: 2]",
        "[@2025-12-31T23:59:59.999999Z: 1, @2026-01-01T00:00:00Z: 2]",
        "[@2026-01-01T01:00:00+01:00: 1, @2026-01-01T00:30:00Z: 2]",
        "[EARLIER: 1, LATER: 2]",
        "[dynamic: 1, dynamic: 2]",
    ] {
        for expectation in [false, true] {
            assert!(findings(trace, expectation).is_empty(), "{trace}");
        }
    }
}

/// TST-7: incompatible key kinds retain their type diagnostic rather
/// than acquiring an ordering comparison between unrelated time scales.
#[test]
fn a_mixed_trace_reports_its_key_type() {
    for trace in [
        "[1us: 1, @2026-01-01T00:00:00Z: 2]",
        "[@2026-01-01T00:00:00Z: 1, 1us: 2]",
    ] {
        for expectation in [false, true] {
            let summary = findings(trace, expectation);
            assert_eq!(summary.len(), 1, "{trace}: {summary:?}");
            assert!(summary[0].ends_with(" type/type.mismatch TST-7"));
        }
    }
}
