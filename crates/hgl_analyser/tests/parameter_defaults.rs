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
        ["m.hgl:2:29 type/constant.failure UNIT-8"],
        "{}",
        checked.report
    );
}

/// EXPR-6: a generic condition decides whether the right operand is
/// evaluated when a concrete call uses the default.
#[test]
fn a_short_circuiting_default_is_checked_at_its_use() {
    let defaults = concat!(
        "const fn either<const Skip: bool>(value: bool = Skip || (1 // 0 == 0)) -> bool => value\n",
        "const fn both<const Run: bool>(value: bool = Run && (1 // 0 == 0)) -> bool => value\n",
        "const fn forward<const Skip: bool>() -> bool => either<Skip>()\n",
    );
    let good = module(&format!(
        "{defaults}\
         const fn skips() -> bool => either<true>() || both<false>()\n\
         const fn supplied() -> bool => either<false>(true)\n\
         const fn skips_forwarded() -> bool => forward<true>()\n",
    ));
    assert!(good.summary().is_empty(), "{}", good.report);
    let bad = module(&format!(
        "{defaults}\
         const fn fails_and() -> bool => both<true>()\n\
         const fn fails_forwarded() -> bool => forward<false>()\n",
    ));
    assert_eq!(
        bad.summary(),
        [
            "m.hgl:2:49 type/constant.failure UNIT-8",
            "m.hgl:3:46 type/constant.failure UNIT-8",
        ],
        "{}",
        bad.report
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

/// CTRL-16 and UNIT-8: only an omitted default reads the selected pack arm.
/// Report a rejected arm at the call that supplies its keys (GEN-7).
#[test]
fn an_omitted_default_checks_the_pack_arm_at_the_making_call() {
    let defaults = concat!(
        "const First: i64 = 0\n",
        "const fn price<...Ps>(value: i64 = if First in arg_keys(Ps) { 11 } else { \"wrong\" }, items: ...Ps) -> i64 => value\n",
        "const fn config<...Ps>{value: i64 = if First in arg_keys(Ps) { 11 } else { \"wrong\" }}(items: ...Ps) -> i64 => value\n",
        "const fn forward<...Ps>(items: ...Ps) -> i64 => config(...items)\n",
    );
    let accepted = module(&format!(
        "{defaults}\
         const fn explicit() -> i64 => price(23)\n\
         const fn selected() -> i64 => config(true) + forward(true)\n"
    ));
    assert!(accepted.summary().is_empty(), "{}", accepted.report);
    let rejected = module(&format!(
        "{defaults}\
         const fn direct() -> i64 => price()\n\
         const fn nested() -> i64 => forward()\n"
    ));
    assert_eq!(
        rejected.summary(),
        [
            "m.hgl:6:29 type/type.mismatch EXPR-5",
            "m.hgl:7:29 type/type.mismatch EXPR-5",
        ],
        "{}",
        rejected.report
    );
    assert!(
        rejected
            .report
            .contains("in the selected arm of the default")
    );
}

/// UNIT-8: the omitted default's dependency cycle reads even an arm
/// absent in every making instance. A call giving the argument is safe.
#[test]
fn a_pack_guarded_default_keeps_its_written_dependencies() {
    let checked = module(concat!(
        "const First: i64 = 0\n",
        "const fn price<...Ps>(value: i64 = if First in arg_keys(Ps) { relay() } else { 7 }, items: ...Ps) -> i64 => value\n",
        "const fn relay() -> i64 => price()\n",
        "const fn safe<...Ps>(value: i64 = if First in arg_keys(Ps) { safe(3) } else { 7 }, items: ...Ps) -> i64 => value\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:3:36 name/default.cycle UNIT-8"],
        "{}",
        checked.report
    );
    assert!(checked.report.contains("through `relay`"));
}

/// UNIT-7: a selected pack arm still has no formal argument in scope.
#[test]
fn a_selected_default_arm_cannot_read_a_formal_argument() {
    let checked = module(concat!(
        "const First: i64 = 0\n",
        "const fn price<...Ps>{value: i64 = if First in arg_keys(Ps) { formal } else { 7 }}(formal: i64, items: ...Ps) -> i64 => value\n",
        "const fn invalid() -> i64 => price(23, true)\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:4:30 name/default.formal_argument UNIT-7"],
        "{}",
        checked.report
    );
}

/// UNIT-8 and EXPR-5: an aggregate literal takes the omitted parameter's
/// instantiated element types, rather than keeping its declaration-time i64s.
#[test]
fn generic_literal_defaults_use_the_concrete_context() {
    let checked = module(concat!(
        "const fn listed<T>(fallback: T = [1]) -> T => fallback\n",
        "const fn paired<T>(fallback: T = (1, 2)) -> T => fallback\n",
        "const fn optional<T>(fallback: T? = [1]) -> T? => fallback\n",
        "struct Holder<T> { value: T = [1] }\n",
        "const fn floats() -> list<f64> => listed<list<f64>>()\n",
        "const fn tupled() -> tuple<f64, f64> => paired<tuple<f64, f64>>()\n",
        "const fn nullable() -> list<f64>? => optional<list<f64>>()\n",
        "const fn field() -> Holder<list<f64>> => Holder<list<f64>>()\n",
        "const fn supplied() -> str => listed<str>(\"given\")\n",
    ));
    assert!(checked.summary().is_empty(), "{}", checked.report);
}

/// EXPR-5: rereading a named constant does not turn it into an aggregate
/// literal, so its already established integer element type cannot widen.
#[test]
fn a_named_default_keeps_its_established_element_type() {
    let checked = module(concat!(
        "const ints: list<i64> = [1]\n",
        "const fn listed<T>(fallback: T = ints) -> T => fallback\n",
        "const fn wrong() -> list<f64> => listed<list<f64>>()\n",
    ));
    assert_eq!(checked.findings.len(), 1, "{}", checked.report);
    assert_eq!(
        checked.findings[0].code, "type/type.mismatch",
        "{}",
        checked.report
    );
}

/// UNIT-8, ERR-9: the catalogued constant.failure code also
/// covers field defaults; an operand skipped by EXPR-6 does not fail.
#[test]
fn field_defaults_use_the_catalogued_constant_failure_code() {
    let checked = module(concat!(
        "struct Bad { value: i64 = 1 // 0 }\n",
        "struct Skipped { value: bool = false && (1 // 0 == 0) }\n",
        "const fn supplied() -> Bad => Bad(value: 1)\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:2:27 type/constant.failure UNIT-8"],
        "{}",
        checked.report
    );
}

/// ERR-9: an earlier type error prevents another diagnostic about the
/// same field default's required evaluation.
#[test]
fn an_invalid_field_default_type_does_not_cascade_to_constant_failure() {
    let checked = module("struct Bad { value: bool = 1 // 0 }\n");
    assert_eq!(
        checked.summary(),
        ["m.hgl:2:30 operator/operator.no_candidate OPS-1"],
        "{}",
        checked.report
    );
}
