//! EXPR-7 bracket reads with known and generic-dependent base types.

mod common;

use common::module;

/// EXPR-7 admits an element of a list or tuple and a dictionary value.
/// The base of each other read below is known where the body is checked.
#[test]
fn known_noncollections_reject_bracket_reads() {
    let checked = module(concat!(
        "struct Point { x: i64 }\n",
        "const fn on_bool(value: bool) -> i64 => value[0]\n",
        "const fn on_int(value: i64) -> i64 => value[0]\n",
        "const fn on_float(value: f64) -> i64 => value[0]\n",
        "const fn on_text(value: str) -> i64 => value[0]\n",
        "const fn on_struct(value: Point) -> i64 => value[0]\n",
        "const fn on_set(value: set<i64>) -> i64 => value[0]\n",
        "fn on_input(value: bool) -> i64 { when { return value[0] } }\n",
        "const fn on_list(value: list<i64>) -> i64 => value[0]\n",
        "const fn on_tuple(value: tuple<i64, str>) -> i64 => value[0]\n",
        "const fn on_dict(value: dict<str, i64>) -> i64 => value[\"key\"]\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:41 type/type.mismatch EXPR-7",
            "m.hgl:4:39 type/type.mismatch EXPR-7",
            "m.hgl:5:41 type/type.mismatch EXPR-7",
            "m.hgl:6:40 type/type.mismatch EXPR-7",
            "m.hgl:7:44 type/type.mismatch EXPR-7",
            "m.hgl:8:44 type/type.mismatch EXPR-7",
            "m.hgl:9:49 type/type.mismatch EXPR-7",
        ]
    );
}

/// GEN-7 checks a generic-dependent bracket read with the instance's
/// base type, including when a generic wrapper supplies that type.
#[test]
fn generic_bracket_reads_reject_only_the_instances_with_invalid_bases() {
    let checked = module(concat!(
        "const fn at_zero<T>(value: T) -> i64 => value[0]\n",
        "const fn forward<T>(value: T) -> i64 => at_zero(value)\n",
        "const fn good() -> i64 => at_zero([1, 2])\n",
        "const fn bad_direct() -> i64 => at_zero(true)\n",
        "const fn bad_forwarded() -> i64 => forward(1.5)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:5:33 type/type.mismatch EXPR-7",
            "m.hgl:6:36 type/type.mismatch EXPR-7",
        ]
    );
    assert!(checked.report.contains("GEN-7: this bracket read"));
}
