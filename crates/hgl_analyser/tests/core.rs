//! Calls through core's own module, as the pinned `hgl_core` declares
//! it: `spec/hgl_core/hgl/core/core.hgl` and the native candidates in
//! `spec/hgl_core/hgl/native/scalar_operators.hgl`, which are built into
//! the compiler. The prelude binds its public declarations in every
//! module (UNIT-11).
//!
//! These tests hold the compiler to core's declarations as they are
//! written, so they fail when a pin moves them: that is their purpose.

mod common;

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use common::{Checked, check, locals};
use hgl_analyser::hir::{Callee, ExprKind, Stmt, ValueCategory};
use hgl_analyser::{DeclarationKind, Role};

/// Checks one file, `m.hgl`, of module `m`. The `module` line is added,
/// so the text given starts on line 2. The program has core's module,
/// as every program does: `check` supplies the one the compiler is
/// built with.
fn with_core(text: &str) -> Checked {
    check(&[("m.hgl", &format!("module m\n{text}"))])
}

/// FMT-4 implementation gate pending #434: fmt's integer character
/// formatter can emit bytes that are outside UTF-8.
#[test]
fn integer_character_formatting_is_temporarily_unavailable() {
    let checked = with_core(concat!(
        "const fn character() -> str => format(\"{:c}\", 233)\n",
        "const fn padded_character() -> str => format(\"{:>3c}\", 256)\n",
        "const fn number() -> str => format(\"{:+d} {:04x}\", 233, 233)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:2:42 type/format.specifier FMT-4",
            "m.hgl:3:49 type/format.specifier FMT-4",
        ],
        "{}",
        checked.report,
    );
}

/// FMT-1, FMT-3, FMT-4 and FMT-9: check a generic format pattern
/// against the argument types of the same instance.
#[test]
fn formatting_checks_a_generic_pattern_and_argument_together() {
    let checked = with_core(concat!(
        "const fn show<const P: str, T>(value: T) -> str => format(P, value)\n",
        "test mark { assert show<\"{*}\">(1) == \"1\" }\n",
        "test hex { assert show<\"{:x}\">(1.5) == \"1.5\" }\n",
        "test character { assert show<\"{:c}\">(65) == \"A\" }\n",
        "test clock { assert show<\"{engine_time}\">(1) == \"1\" }\n",
        "test unused { assert show<\"none\">(1) == \"none\" }\n",
        "test valid { assert show<\"{:x}\">(42) == \"2a\" }\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:20 type/format.specifier FMT-9",
            "m.hgl:4:19 type/format.specifier FMT-4",
            "m.hgl:5:25 type/format.specifier FMT-4",
            "m.hgl:6:21 type/format.argument FMT-9",
            "m.hgl:7:22 type/format.unused FMT-3",
        ],
        "{}",
        checked.report,
    );
}

/// The module the compiler supplies is the pinned core's, whole: every
/// `.hgl` file under `spec/hgl_core/hgl`, with its text as it stands
/// there. A pin that adds a file fails this until
/// `hgl_analyser::CORE_FILES` lists it.
#[test]
fn the_core_supplied_is_every_file_of_the_pinned_core() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/hgl_core/hgl");
    let mut pinned = BTreeMap::new();
    let mut directories = vec![root.clone()];
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(&directory).expect("the pinned core is read") {
            let path = entry.expect("the pinned core is read").path();
            if path.is_dir() {
                directories.push(path);
            } else if path.extension().is_some_and(|extension| extension == "hgl") {
                // Named as `CORE_FILES` names it, with `/` on every
                // system.
                let relative = path
                    .strip_prefix(&root)
                    .expect("the file is under the root");
                let parts: Vec<String> = relative
                    .iter()
                    .map(|part| part.to_string_lossy().into_owned())
                    .collect();
                let text = fs::read_to_string(&path).expect("the file is text");
                pinned.insert(format!("<core>/{}", parts.join("/")), text);
            }
        }
    }
    let supplied: BTreeMap<String, String> = hgl_analyser::CORE_FILES
        .iter()
        .map(|(name, text)| (name.to_string(), text.to_string()))
        .collect();
    assert_eq!(
        supplied.keys().collect::<Vec<_>>(),
        pinned.keys().collect::<Vec<_>>()
    );
    assert_eq!(supplied, pinned);
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

/// NUM-9: "`zero` is a generic value parameter of the candidates that
/// declare it, and a call gives it a compile-time constant." OPS-8:
/// `invalid` "is a generic value parameter of those conversions, and a
/// call gives it a compile-time constant. `rounding` and `nan` are
/// parameters." GEN-8: a value known only when the program runs is
/// rejected for the first two, and is what a parameter takes.
#[test]
fn an_option_that_selects_the_result_type_is_given_a_constant() {
    let checked = with_core(concat!(
        "const fn uses(a: f64, b: f64, policy: ZeroPolicy, strict: InvalidPolicy, mode: Rounding) {\n",
        "    let runtime = div_(a, b, zero: policy)\n",
        "    let forced = convert<i64>(a, invalid: strict)\n",
        // `rounding` does not select: a parameter.
        "    let rounded = convert<i64>(a, rounding: mode)\n",
        // TYPE-7: an enum member written in full.
        "    let full = div_(a, b, zero: ZeroPolicy.error)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:36 type/generic.argument_constant GEN-8",
            "m.hgl:4:43 type/generic.argument_constant GEN-8",
        ]
    );
    assert_eq!(
        locals(&checked, "uses")[7..],
        ["rounded: result<i64, error>", "full: result<f64, error>"]
    );
}

/// NUM-12: "Every division takes `zero`: `/`, `//`, and `%`, of numbers
/// and of durations (OPS-1). The policies one admits, and its default,
/// follow from what its result can hold". NUM-3: `.fail` makes a zero
/// divisor an HGL error, and so keeps the division's own type. NUM-9: a
/// division returns a `result` of its type under `zero: .error`.
#[test]
fn every_division_takes_the_policies_its_result_can_hold() {
    let checked = with_core(concat!(
        "const fn uses(a: f64, n: i64, span: duration, gap: duration) {\n",
        // A result that holds no infinity or NaN: `.fail`, the default,
        // and `.error`.
        "    let whole = floordiv_(n, n)\n",
        "    let rest = mod_(n, n, zero: .fail)\n",
        "    let checked = floordiv_(n, n, zero: .error)\n",
        "    let piece = div_(span, n)\n",
        "    let pieces = div_(span, a, zero: .error)\n",
        "    let count = floordiv_(span, gap, zero: .error)\n",
        // A result that is an `f64`: every policy.
        "    let raised = div_(a, a, zero: .fail)\n",
        "    let real = floordiv_(a, n, zero: .nan)\n",
        "    let ratio = div_(span, gap)\n",
        "    let safe = div_(span, gap, zero: .error)\n",
        // An `i64` and a `duration` hold no NaN or infinity.
        "    let bad1 = floordiv_(n, n, zero: .nan)\n",
        "    let bad2 = div_(span, n, zero: .ieee)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:13:16 operator/operator.no_candidate GEN-5",
            "m.hgl:14:16 operator/operator.no_candidate GEN-5",
        ]
    );
    assert_eq!(
        locals(&checked, "uses")[4..14],
        [
            "whole: i64",
            "rest: i64",
            "checked: result<i64, error>",
            "piece: duration",
            "pieces: result<duration, error>",
            "count: result<i64, error>",
            "raised: f64",
            "real: f64",
            "ratio: f64",
            "safe: result<f64, error>",
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

/// OPS-2: "`<`, `<=`, `>`, `>=`, and `<=>` compare two values of one
/// type among `str`, `bool`, `date`, `time`, `datetime`,
/// `civil_datetime`, and `duration`. `==` and `!=` compare two values
/// of one type. The zoned types have no order ... Numbers compare as
/// the numeric contract says, an `i64` with an `f64` included." OPS-3:
/// `<=>` returns a `CmpResult`. NUM-4: the tolerances are for numbers.
/// NUM-11: "The built-in `abs` and `rel` tolerance options are scalar
/// f64 configuration."
///
/// Core declares one generic candidate for each comparison, with the
/// types it is for in its `requires`, beside the candidates for
/// numbers.
#[test]
fn a_comparison_is_of_two_values_of_one_type() {
    let checked = with_core(concat!(
        "struct Quote { bid: f64 }\n",
        "const fn uses(a: f64, n: i64, text: str, day: date, q: Quote, zone: zoned_datetime) {\n",
        "    let e1 = eq_(n, n)\n",
        "    let e2 = eq_(text, text)\n",
        "    let e3 = eq_(q, q)\n",
        "    let e4 = eq_(n, a)\n",
        "    let e5 = eq_{abs: 0.01}(a, a)\n",
        "    let l1 = lt_(text, text)\n",
        "    let l2 = lt_(day, day)\n",
        "    let l3 = lt_(n, a)\n",
        "    let c1 = compare_(text, text)\n",
        // No order for a struct or a zoned type; no comparison of a
        // `str` with an `i64`; no tolerance for a `str`.
        "    let bad1 = lt_(q, q)\n",
        "    let bad2 = lt_(zone, zone)\n",
        "    let bad3 = eq_(text, n)\n",
        "    let bad4 = eq_{abs: 0.01}(text, text)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:13:16 operator/operator.no_candidate GEN-5",
            "m.hgl:14:16 operator/operator.no_candidate GEN-5",
            "m.hgl:15:16 operator/operator.no_candidate GEN-5",
            "m.hgl:16:16 operator/operator.no_candidate GEN-5",
        ]
    );
    assert_eq!(
        locals(&checked, "uses")[6..15],
        [
            "e1: bool",
            "e2: bool",
            "e3: bool",
            "e4: bool",
            "e5: bool",
            "l1: bool",
            "l2: bool",
            "l3: bool",
            "c1: CmpResult",
        ]
    );
}

/// OPS-9: "Same-type temporal addition and subtraction may select an
/// `invalid: .identity`, a const generic `InvalidInputPolicy` whose
/// member is `identity`. Addition requires a value `add_(T, T) -> T`;
/// subtraction requires value `sub_(T, T) -> T` and `neg_(T) -> T`. The
/// option admits no mixed operand or result types ... Omission keeps
/// ordinary candidate selection and lifting and validity admission.
/// These temporal candidates require the explicit `.identity` option."
///
/// Core's two candidates are nodes, generic in the operand type, that
/// state those operations in `requires` (GEN-16); a call in composition
/// selects among temporal and value candidates (GEN-5).
#[test]
fn identity_arithmetic_is_selected_by_its_option() {
    let checked = with_core(concat!(
        "fn uses(a: f64, b: f64, n: i64, text: str, flag: bool) -> f64 {\n",
        // Omitted: the value candidate, lifted.
        "    let plain = a + b\n",
        "    let kept = a +{invalid: .identity} b\n",
        "    let named = add_(a, b, invalid: .identity)\n",
        "    let less = a -{invalid: .identity} b\n",
        "    let whole = n -{invalid: .identity} n\n",
        "    let words = text +{invalid: .identity} text\n",
        // No mixed operands; a `bool` is not added; a `str` is not
        // subtracted.
        "    let mixed = a +{invalid: .identity} n\n",
        "    let bad = flag +{invalid: .identity} flag\n",
        "    let gone = text -{invalid: .identity} text\n",
        "    return kept\n",
        "}\n",
        // A generic graph states what its parameter needs (GEN-7).
        "fn stated<T>(lhs: T, rhs: T) -> T\n",
        "    requires add_(T, T) -> T => lhs +{invalid: .identity} rhs\n",
        "fn unstated<T>(lhs: T, rhs: T) -> T => lhs +{invalid: .identity} rhs\n",
        // A value function selects among value candidates, and none
        // declares the option (EXPR-10: a configuration entry names an
        // option of the candidates).
        "const fn value(a: f64, b: f64) -> f64 => a +{invalid: .identity} b\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:9:19 operator/operator.no_candidate OPS-1",
            "m.hgl:10:20 operator/operator.no_candidate OPS-1",
            "m.hgl:11:21 operator/operator.no_candidate OPS-1",
            "m.hgl:16:44 operator/operator.no_candidate OPS-1",
            "m.hgl:17:46 name/argument.unknown EXPR-10",
        ]
    );
    // The role of the candidate each of the first four locals calls.
    let module = checked.program.module("m").expect("module m exists");
    let index = module
        .declarations
        .iter()
        .position(|declaration| declaration.name == "uses")
        .expect("the declaration exists");
    let body = module.bodies[index].as_ref().expect("it has a body");
    let roles: Vec<Role> = body.statements[..4]
        .iter()
        .map(|statement| {
            let Stmt::Declare {
                value: Some(value), ..
            } = statement
            else {
                panic!("a local with a value: {statement:?}");
            };
            let ExprKind::Call {
                callee:
                    Callee::Candidate {
                        module,
                        declaration,
                        ..
                    },
                ..
            } = &value.kind
            else {
                panic!("a candidate's call: {:?}", value.kind);
            };
            match checked.program.modules[*module].declarations[*declaration].kind {
                DeclarationKind::Implementation(role, _)
                | DeclarationKind::NativeCandidate(role, _) => role,
                other => panic!("a candidate: {other:?}"),
            }
        })
        .collect();
    assert_eq!(
        roles,
        [Role::Value, Role::Temporal, Role::Temporal, Role::Temporal]
    );
    let locals: Vec<String> = body.locals[5..11]
        .iter()
        .map(|local| {
            let connection = match local.category {
                ValueCategory::Connection => " (connection)",
                _ => "",
            };
            format!("{}: {}{connection}", local.name, local.ty)
        })
        .collect();
    assert_eq!(
        locals,
        [
            "plain: f64 (connection)",
            "kept: f64 (connection)",
            "named: f64 (connection)",
            "less: f64 (connection)",
            "whole: i64 (connection)",
            "words: str (connection)",
        ]
    );
}
