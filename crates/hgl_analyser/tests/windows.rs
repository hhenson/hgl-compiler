//! A window's sizes computed from constants (TYPE-11, UNIT-13).

mod common;

use common::{Checked, module};
use hgl_analyser::{Detail, Type};

/// The type of the parameter `name` of the callable `function` of
/// module `m`.
fn parameter(checked: &Checked, function: &str, name: &str) -> Type {
    let module = checked.program.module("m").expect("module m exists");
    let index = module
        .declarations
        .iter()
        .position(|declaration| declaration.name == function)
        .expect("the declaration exists");
    let Detail::Callable(signature) = &module.details[index] else {
        panic!("{function} is a callable");
    };
    signature
        .parameters
        .iter()
        .find(|parameter| parameter.name == name)
        .expect("the parameter exists")
        .ty
        .clone()
}

/// TYPE-11: "Its maximum and minimum have one constant kind: tick count
/// or duration ... duration maximum is positive and minimum may be zero
/// ... Minimum cannot exceed maximum. Size/kind are part of identity."
/// A size may be a constant of a module (UNIT-13) and a duration times
/// an `i64`, core's `mul_(lhs: duration, rhs: i64) -> duration`: each
/// is followed to its value, and its kind and bounds are checked.
#[test]
fn a_windows_size_computed_from_constants_is_checked() {
    let checked = module(concat!(
        "const D: duration = 5m\n",
        "const N: i64 = 3\n",
        "fn good(a: rolling_window<f64, D * 2, 30s * 2>, b: rolling_window<f64, 10m, 1m>,\n",
        "    c: rolling_window<f64, N * 2, N>, d: rolling_window<f64, 6, 3>) {}\n",
        "fn bounds(a: rolling_window<f64, 1s * 0>, b: rolling_window<f64, 2s, 1s * 3>) {}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:6:34 type/rolling_window.size_bounds TYPE-11",
            "m.hgl:6:70 type/rolling_window.size_bounds TYPE-11",
        ]
    );
    assert_eq!(
        parameter(&checked, "good", "a"),
        parameter(&checked, "good", "b")
    );
    assert_eq!(
        parameter(&checked, "good", "c"),
        parameter(&checked, "good", "d")
    );
}

/// TYPE-11: a size is a "tick count or duration". A size that names a
/// constant of a module of any other type is the wrong kind
/// (`rolling_window.size_kind`); one of either type is not, whatever
/// its value.
#[test]
fn a_windows_size_that_names_a_constant_of_another_type_is_the_wrong_kind() {
    let checked = module(concat!(
        "enum Side { buy }\n",
        "const F: f64 = 1.5\n",
        "const S: str = \"long\"\n",
        "const K: Side = .buy\n",
        "const M: i64? = 3\n",
        "const fn three() -> i64 => 3\n",
        "const T: i64 = three()\n",
        "fn wrong(a: rolling_window<f64, F>, b: rolling_window<f64, S>, c: rolling_window<f64, 5, K>,\n",
        "    d: rolling_window<f64, M>) {}\n",
        "fn called(a: rolling_window<f64, T>) {}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:9:33 type/rolling_window.size_kind TYPE-11",
            "m.hgl:9:60 type/rolling_window.size_kind TYPE-11",
            "m.hgl:9:90 type/rolling_window.size_kind TYPE-11",
            "m.hgl:10:28 type/rolling_window.size_kind TYPE-11",
        ]
    );
    let messages: Vec<&str> = checked
        .findings
        .iter()
        .map(|finding| finding.message.as_str())
        .collect();
    assert_eq!(
        messages,
        [
            "a window's size is a tick count or a duration, and `F` is a constant of type `f64`",
            "a window's size is a tick count or a duration, and `S` is a constant of type `str`",
            "a window's size is a tick count or a duration, and `K` is a constant of type `Side`",
            "a window's size is a tick count or a duration, and `M` is a constant of type `i64?`",
        ]
    );
    // `T` is an `i64` computed by a call, which this pass does not
    // evaluate: its kind is not known, and nothing is reported of it.
}

/// Core's OPS-1: `*` of a "`duration` and `i64` ..., in either order",
/// `mul_(lhs: duration, rhs: i64)` and `mul_(lhs: i64, rhs: duration)`.
/// A size written either way is followed to its value (TYPE-11): its
/// bounds are checked, and it is the window of that value in identity.
#[test]
fn a_duration_times_an_integer_is_followed_in_either_order() {
    let checked = module(concat!(
        "fn zero(a: rolling_window<f64, 5m * 0>, b: rolling_window<f64, 0 * 5m>) {}\n",
        "fn over(a: rolling_window<f64, 1m, 1m * 2>, b: rolling_window<f64, 1m, 2 * 1m>) {}\n",
        "fn same(a: rolling_window<f64, 5m * 2>, b: rolling_window<f64, 2 * 5m>, c: rolling_window<f64, 10m>) {}\n",
        "fn ten(x: rolling_window<f64, 2 * 5m>) -> f64 { when { return 1.0 } }\n",
        "fn ticks(x: rolling_window<f64, 3>) -> f64 => ten(x)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:2:32 type/rolling_window.size_bounds TYPE-11",
            "m.hgl:2:64 type/rolling_window.size_bounds TYPE-11",
            "m.hgl:3:36 type/rolling_window.size_bounds TYPE-11",
            "m.hgl:3:72 type/rolling_window.size_bounds TYPE-11",
            "m.hgl:6:51 type/type.mismatch EXPR-5",
        ]
    );
    assert_eq!(
        parameter(&checked, "same", "a"),
        parameter(&checked, "same", "c")
    );
    assert_eq!(
        parameter(&checked, "same", "b"),
        parameter(&checked, "same", "c")
    );
}
