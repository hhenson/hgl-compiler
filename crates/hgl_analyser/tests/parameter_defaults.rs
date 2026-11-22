//! Required evaluation and the expected type of parameter defaults (UNIT-8).

mod common;

use common::module;

/// UNIT-8 and ERR-9: reject a required failure, with the ordinary
/// EXPR-6 evaluation order for an operand the left side skips.
#[test]
fn a_known_failure_in_a_default_is_rejected() {
    let checked = module(concat!(
        "const fn fails(value: i64 = 1 // 0) -> i64 => value\n",
        "const fn skips(value: bool = false && 1 // 0 > 0) -> bool => value\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:2:29 local:type/default.constant_failure UNIT-8"],
        "{}",
        checked.report
    );
}

/// UNIT-8: a constant of a window publishes an arrival (TYPE-11),
/// and one of an atomic input publishes its whole payload (TYPE-9).
#[test]
fn a_temporal_default_is_checked_against_what_it_publishes() {
    let checked = module(concat!(
        "fn window_default(value: rolling_window<i64, 5> = \"wrong\") {}\n",
        "fn atomic_default(value: atomic<list<i64>> = \"wrong\") {}\n",
        "fn window_arrival(value: rolling_window<i64, 5> = 1) {}\n",
        "fn atomic_payload(value: atomic<list<i64>> = []) {}\n",
        "fn silent_window(value: rolling_window<i64, 5>? = null) {}\n",
        "fn null_arrival(value: rolling_window<i64?, 5> = delta<i64?>()) {}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:2:51 type/type.mismatch EXPR-5",
            "m.hgl:3:46 type/type.mismatch EXPR-5",
        ],
        "{}",
        checked.report
    );
}

/// UNIT-7 and UNIT-8: the omitted value is read in its own scope
/// and fits the parameter whose type the call has inferred.
#[test]
fn an_omitted_default_fits_the_concrete_parameter_type() {
    let default = "const fn choose<T>(value: T, fallback: T = 1) -> T => fallback\n";
    let direct = module(&format!(
        "{default}\
         const fn wrong() -> str => choose(\"text\")\n\
         const fn good() -> i64 => choose(2)\n\
         const fn supplied() -> str => choose(\"text\", \"given\")\n",
    ));
    assert_eq!(
        direct.summary(),
        ["m.hgl:2:44 type/type.mismatch EXPR-5"],
        "{}",
        direct.report
    );
    let nested = module(&format!(
        "{default}\
         const fn forwards<T>(value: T) -> T => choose(value)\n\
         const fn wrong() -> str => forwards(\"text\")\n\
         const fn good() -> i64 => forwards(2)\n",
    ));
    assert_eq!(
        nested.summary(),
        ["m.hgl:2:44 type/type.mismatch EXPR-5"],
        "{}",
        nested.report
    );
}
