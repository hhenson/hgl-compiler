//! Calls through core's own module, as the pinned `hgl_core` declares
//! it: `spec/hgl_core/hgl/core/core.hgl` and the native candidates in
//! `spec/hgl_core/hgl/native/scalar_operators.hgl`. The prelude binds
//! its public declarations in every module (UNIT-11).
//!
//! These tests hold the compiler to core's declarations as they are
//! written, so they fail when a pin moves them: that is their purpose.

mod common;

use std::fs;
use std::path::Path;

use common::{Checked, check};

/// Checks one file, `m.hgl`, of module `m`, in a program that has
/// core's module. The `module` line is added, so the text given starts
/// on line 2.
fn with_core(text: &str) -> Checked {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/hgl_core/hgl");
    let read = |path: &str| {
        fs::read_to_string(root.join(path))
            .unwrap_or_else(|error| panic!("the pinned core has {path}: {error}"))
    };
    let core = read("core/core.hgl");
    let native = read("native/scalar_operators.hgl");
    check(&[
        ("core.hgl", &core),
        ("m.hgl", &format!("module m\n{text}")),
        ("scalar_operators.hgl", &native),
    ])
}

/// The locals of `name` of module `m` as `local: type`, parameters
/// first.
fn locals(checked: &Checked, name: &str) -> Vec<String> {
    let module = checked.program.module("m").expect("module m exists");
    let index = module
        .declarations
        .iter()
        .position(|declaration| declaration.name == name)
        .expect("the declaration exists");
    module.bodies[index]
        .as_ref()
        .expect("it has a body")
        .locals
        .iter()
        .map(|local| format!("{}: {}", local.name, local.ty))
        .collect()
}

/// NUM-3, NUM-9: "Division with `zero: .ieee` or `zero: .nan` returns
/// f64; division with `zero: .error` returns `result<f64, error>`."
/// Core's candidates of `div_` for numbers declare `zero` and say in
/// `requires` which values each is for (GEN-6), so the option a call
/// gives selects the candidate and the result type (EXPR-12).
#[test]
fn divisions_zero_option_selects_the_result_type() {
    let checked = with_core(concat!(
        "const fn uses(a: f64, b: f64, n: i64, text: str) {\n",
        "    let plain = div_(a, b)\n",
        "    let nan = div_(a, b, zero: .nan)\n",
        "    let checked = div_(a, b, zero: .error)\n",
        "    let mixed = div_(n, b, zero: .error)\n",
        // OPS-1: a `str` is not divided, with an option or without.
        "    let wrong = div_(text, n, zero: .nan)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:7:17 operator/operator.no_candidate GEN-5"]
    );
    assert_eq!(
        locals(&checked, "uses")[4..8],
        [
            "plain: f64",
            "nan: f64",
            "checked: result<f64, error>",
            "mixed: result<f64, error>",
        ]
    );
}

/// OPS-4: "`convert<T>(value)` converts a value to the type T ... It
/// admits the conversions of the second table below and rejects every
/// other." OPS-8: "`invalid` determines the result type ... A
/// conversion that cannot fail returns T." Core's `convert` requires
/// its result to be T or `result<T, error>`, which with the T a call
/// writes (GEN-8) says which candidate the call asks for.
#[test]
fn a_conversion_is_the_type_asked_for_or_a_result_of_it() {
    let checked = with_core(concat!(
        "const fn uses(a: f64, n: i64, text: str, raw: bytes) {\n",
        // The conversions that cannot fail.
        "    let real = convert<f64>(n)\n",
        "    let words = convert<str>(n)\n",
        // Those that can: a `result` by default, the plain type under
        // `invalid: .fail`.
        "    let whole = convert<i64>(a)\n",
        "    let rounded = convert<i64>(a, rounding: .half_even)\n",
        "    let zeroed = convert<i64>(a, nan: 0)\n",
        "    let forced = convert<i64>(a, invalid: .fail)\n",
        "    let parsed = convert<f64>(text)\n",
        "    let decoded = convert<str>(raw, invalid: .fail)\n",
        // No conversion of an `i64` to a `date`; no `rounding` but from
        // an `f64` to an `i64`.
        "    let none = convert<date>(n)\n",
        "    let odd = convert<f64>(n, rounding: .floor)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:11:16 operator/operator.no_candidate GEN-5",
            "m.hgl:12:15 operator/operator.no_candidate GEN-5",
        ]
    );
    assert_eq!(
        locals(&checked, "uses")[4..12],
        [
            "real: f64",
            "words: str",
            "whole: result<i64, error>",
            "rounded: result<i64, error>",
            "zeroed: result<i64, error>",
            "forced: i64",
            "parsed: result<f64, error>",
            "decoded: str",
        ]
    );
}
