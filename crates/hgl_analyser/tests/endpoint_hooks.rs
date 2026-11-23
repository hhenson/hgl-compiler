//! Endpoint mutation and delta admission in hooks (TIME-1, TIME-5, END-3, END-4).

mod common;

use common::module;

/// TIME-5: all inputs are passive in stop; activity queries are still
/// metadata, while changing that activity rejects.
#[test]
fn stop_rejects_both_activity_mutations() {
    let checked = module(concat!(
        "fn observe(input: i64) {\n",
        " when { activate(input)\n passivate(input) }\n",
        " stop {\n",
        "  let active = is_active(input)\n",
        "  activate(input)\n",
        "  passivate(input)\n",
        " }\n",
        "}\n",
    ));
    assert_eq!(checked.findings.len(), 2, "{}", checked.report);
    assert!(
        checked
            .findings
            .iter()
            .all(|f| f.code == "phase/input.lifecycle" && f.rule == "TIME-5"),
        "{}",
        checked.report
    );
}

/// END-4: the operation clears the node's own out; an input is not
/// writable, whether it holds a value or a reference designation.
#[test]
fn invalidation_rejects_value_and_ref_inputs() {
    let checked = module(concat!(
        "fn plain(input: i64) { when { invalidate(input) } }\n",
        "fn reference(ref input: i64) { when { invalidate(input) } }\n",
        "fn output(input: i64) -> i64 {\n",
        " inject out\n",
        " when { invalidate(out) }\n",
        "}\n",
    ));
    assert_eq!(checked.findings.len(), 2, "{}", checked.report);
    assert!(
        checked
            .findings
            .iter()
            .all(|f| f.code == "phase/endpoint.argument" && f.rule == "END-4"),
        "{}",
        checked.report
    );
}

/// END-3, TIME-4: neither a plain nor a checked result-form delta
/// read bypasses manual endpoint admission.
#[test]
fn delta_reads_require_direct_input_admission_in_all_places() {
    let checked = module(concat!(
        "fn plain(pulse: bool, input: i64) -> i64 {\n",
        " when modified(pulse) && valid([]) { return delta_value(input) }\n",
        "}\n",
        "fn checked(pulse: bool, input: i64) -> bool {\n",
        " when modified(pulse) && valid([]) {\n",
        "  let read: result<i64, error> = delta_value(input)\n",
        "  return is_ok(read)\n",
        " }\n",
        "}\n",
    ));
    assert_eq!(checked.findings.len(), 2, "{}", checked.report);
    assert!(
        checked
            .findings
            .iter()
            .all(|f| f.code == "phase/payload.unguarded" && f.rule == "TIME-4"),
        "{}",
        checked.report
    );
}

/// END-3: validity admits the read; a result or nullable place handles
/// a cycle with no delta, so no modified guard is required.
#[test]
fn admitted_delta_forms_need_no_static_tick_proof() {
    let checked = module(concat!(
        "fn checked(pulse: bool, input: i64) -> bool {\n",
        " when modified(pulse) && valid(input) {\n",
        "  let read: result<i64, error> = delta_value(input)\n",
        "  return is_ok(read)\n",
        " }\n",
        "}\n",
        "fn optional(pulse: bool, input: i64) -> i64? {\n",
        " when modified(pulse) && valid([]) {\n",
        "  if valid(input) { let read: i64? = delta_value(input)\n return read }\n",
        " }\n",
        "}\n",
    ));
    assert!(checked.findings.is_empty(), "{}", checked.report);
}

/// END-3, TIME-1: a delta of a partly valid collection is not a whole
/// payload read. The refactored whole-read check still needs all_valid.
#[test]
fn a_collection_delta_does_not_require_all_children_to_be_valid() {
    let checked = module(concat!(
        "struct Pair {\n left: i64\n right: i64\n}\n",
        "fn relay(input: Pair) -> Pair { when valid(input) { return delta_value(input) } }\n",
        "fn whole(input: Pair) -> Pair { when valid(input) { return input } }\n",
    ));
    assert_eq!(checked.findings.len(), 1, "{}", checked.report);
    assert_eq!(
        checked.findings[0].code, "phase/payload.unguarded",
        "{}",
        checked.report
    );
    assert_eq!(checked.findings[0].rule, "TIME-1", "{}", checked.report);
    assert!(
        checked.findings[0].at.starts_with("m.hgl:7:"),
        "{}",
        checked.report
    );
}

/// TIME-5: stop has no admitting header, while a successful valid
/// guard admits the read there. Start still reports just its lifecycle fault.
#[test]
fn lifecycle_delta_reads_keep_their_existing_phase_rules() {
    let checked = module(concat!(
        "fn stopped(input: i64) {\n",
        " when {}\n",
        " stop { let read: result<i64, error> = delta_value(input) }\n",
        "}\n",
        "fn guarded(input: i64) {\n",
        " when {}\n",
        " stop { if valid(input) { let read: result<i64, error> = delta_value(input) } }\n",
        "}\n",
        "fn started(input: i64) {\n",
        " start { let read = delta_value(input) }\n",
        " when {}\n",
        "}\n",
    ));
    assert_eq!(checked.findings.len(), 2, "{}", checked.report);
    assert_eq!(
        checked.findings[0].code, "phase/payload.unguarded",
        "{}",
        checked.report
    );
    assert_eq!(
        checked.findings[1].code, "phase/input.lifecycle",
        "{}",
        checked.report
    );
}
