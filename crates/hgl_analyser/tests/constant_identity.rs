//! The analyser against UNIT-13's rule of when two compile-time
//! constants are the same, sentence by sentence, and the rules that
//! point at it: EXPR-5, TYPE-3, GEN-1, GEN-3, GEN-13 and its worked
//! fragment, UNIT-9 and CTRL-6, and TYPE-16's proposed sentence on a
//! delta's keys.
//!
//! A finding is written `line code`: the line of `m.hgl`, counted from
//! the `module` line, and the catalogued code.

mod common;

use common::{Checked, check};
use hgl_analyser::{Argument, Detail, Type};

/// Checks one file, `m.hgl`, of module `m`. The `module` line is added,
/// so the text given starts on line 2.
fn module(text: &str) -> Checked {
    check(&[("m.hgl", &format!("module m\n{text}"))])
}

/// Each finding of `checked` as `line code`.
fn found(checked: &Checked) -> Vec<String> {
    checked
        .summary()
        .iter()
        .map(|finding| {
            let mut parts = finding.split(' ');
            let at = parts.next().unwrap_or_default();
            let code = parts.next().unwrap_or_default();
            let line = at.split(':').nth(1).unwrap_or_default();
            let code = code.rsplit('/').next().unwrap_or_default();
            format!("{line} {code}")
        })
        .collect()
}

/// Asserts what `text`, a module's declarations after `module m`, is
/// rejected for: each finding as `line code`.
fn expect(text: &str, findings: &[&str]) {
    let checked = module(text);
    assert_eq!(found(&checked), findings, "{text}\n{}", checked.report);
}

/// GEN-13's accepted declaration fragments, on lines 2 and 3.
const FRAGMENT: &str = concat!(
    "struct Scaled<const k: f64> { value: f64 }\n",
    "const fn reciprocal<const k: f64>() -> f64 => 1.0 / k\n",
);

/// The key a target names the instance or the class by, of the one
/// generic value argument of each parameter of the function `name`.
fn argument_keys(checked: &Checked, name: &str) -> Vec<Option<String>> {
    let module = checked.program.module("m").expect("the module exists");
    let index = module
        .declarations
        .iter()
        .position(|declaration| declaration.name == name)
        .expect("the function exists");
    let Detail::Callable(signature) = &module.details[index] else {
        panic!("{name} is {:?}", module.details[index]);
    };
    signature
        .parameters
        .iter()
        .map(|parameter| match &parameter.ty {
            Type::Named { arguments, .. } => match arguments.as_slice() {
                [Argument::Constant(constant)] => constant.value.key(),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// GEN-13: "For `struct Scaled<const k: f64>`, `Scaled<1>` and
/// `Scaled<1.0>` are one type, and `Scaled<-0.0>` is not
/// `Scaled<0.0>`." Its fragment: "`Scaled<1>`, `Scaled<1.0>` and
/// `Scaled<1.00>` are one type ... Deliberate rejection: a
/// `Scaled<-0.0>` where a `Scaled<0.0>` is expected (`type.mismatch`)."
/// TYPE-3: "Generic applications are invariant by origin and complete
/// canonical arguments, a value argument by its value (UNIT-13)".
#[test]
fn scaled_of_one_however_written_is_one_type_and_of_minus_zero_another() {
    let accepted = format!(
        "{FRAGMENT}const fn one(a: Scaled<1>, b: Scaled<1.0>, c: Scaled<1.00>) -> list<Scaled<1.0>, 3> => [a, b, c]\n\
         const fn first(a: Scaled<1.00>) -> Scaled<1> => a\n\
         const fn given(b: Scaled<1>) -> list<Scaled<1.0>, 3> => one(b, b, b)\n\
         const fn zeros(a: Scaled<0.0>, b: Scaled<-0.0>) -> f64 => a.value + b.value\n"
    );
    expect(&accepted, &[]);
    // One value is one key, which a target writes one class by.
    let checked = module(&accepted);
    assert_eq!(
        argument_keys(&checked, "one"),
        vec![Some("1.0".to_string()); 3]
    );
    let zeros = argument_keys(&checked, "zeros");
    assert_ne!(zeros[0], zeros[1], "{zeros:?}");
    expect(
        &format!(
            "{FRAGMENT}const fn returned(a: Scaled<-0.0>) -> Scaled<0.0> => a\n\
             const fn wants(a: Scaled<0.0>) -> f64 => a.value\n\
             const fn gives(b: Scaled<-0.0>) -> f64 => wants(b)\n\
             const fn made() -> Scaled<0.0> => Scaled<-0.0>(value: 1.0)\n\
             const fn back(a: Scaled<0.0>) -> Scaled<-0.0> => a\n"
        ),
        &[
            "4 type.mismatch",
            "6 type.mismatch",
            "7 type.mismatch",
            "8 type.mismatch",
        ],
    );
}

/// GEN-13: "`reciprocal<1>()` and `reciprocal<1.0>()` call one
/// instance. `reciprocal<0.0>()` is positive infinity and
/// `reciprocal<-0.0>()` negative infinity, whichever is called first.
/// With `requires k == 0.0` added, `reciprocal` admits both of those
/// calls (GEN-3)." GEN-3: "An equality of two constants there is the
/// `==` a program evaluates, and not their sameness (UNIT-13)".
#[test]
fn requires_k_equal_to_zero_admits_both_zeros() {
    let guarded = concat!(
        "struct Scaled<const k: f64> { value: f64 }\n",
        "const fn reciprocal<const k: f64>() -> f64 requires k == 0.0 => 1.0 / k\n",
    );
    expect(
        &format!(
            "{guarded}const fn both() -> f64 => reciprocal<0.0>() + reciprocal<-0.0>()\n\
             const fn flipped() -> f64 => reciprocal<-0.0>() + reciprocal<0.0>()\n\
             const fn spelled() -> f64 => reciprocal<0>() + reciprocal<-(0.0)>() + reciprocal<0.00>()\n"
        ),
        &[],
    );
    expect(
        &format!(
            "{guarded}const fn one() -> f64 => reciprocal<1>()\n\
             const fn nan() -> f64 => reciprocal<0.0 / 0.0>()\n"
        ),
        &["4 requires.unmet", "5 requires.unmet"],
    );
    // Without the `requires`, each of the fragment's calls is admitted.
    expect(
        &format!(
            "{FRAGMENT}const fn calls() -> list<f64, 4> => \
             [reciprocal<1>(), reciprocal<1.0>(), reciprocal<0.0>(), reciprocal<-0.0>()]\n"
        ),
        &[],
    );
}

/// UNIT-13: "Two compile-time constants are one where those values are
/// the same, however each is written: `1`, `1.0` and `1.00` as an
/// `f64`; `60s` and `1m`; the `datetime`s `@2026-09-03T09:30Z` and
/// `@2026-09-03T10:30+01:00`, one instant." And another value is
/// another constant.
#[test]
fn the_constants_unit_13_names_are_one_however_written() {
    let boxes = concat!(
        "struct FBox<const X: f64> { v: i64 }\n",
        "struct DBox<const X: duration> { v: i64 }\n",
        "struct TBox<const X: datetime> { v: i64 }\n",
    );
    expect(
        &format!(
            "{boxes}const fn f(a: FBox<1>, b: FBox<1.0>) -> FBox<1.00> => b\n\
             const fn g(a: FBox<1.00>) -> FBox<1> => a\n\
             const fn d(a: DBox<60s>) -> DBox<1m> => a\n\
             const fn t(a: TBox<@2026-09-03T09:30Z>) -> TBox<@2026-09-03T10:30+01:00> => a\n"
        ),
        &[],
    );
    expect(
        &format!(
            "{boxes}const fn f(a: FBox<1>) -> FBox<1.01> => a\n\
             const fn d(a: DBox<60s>) -> DBox<1h> => a\n\
             const fn t(a: TBox<@2026-09-03T09:30Z>) -> TBox<@2026-09-03T09:30+01:00> => a\n"
        ),
        &["5 type.mismatch", "6 type.mismatch", "7 type.mismatch"],
    );
}

/// UNIT-13: "The same value is not `==` ...: `-0.0` and `0.0` are two
/// constants, though `-0.0 == 0.0`, and any two NaNs, `0.0 / 0.0` among
/// them, are one constant, though `==` holds for no NaN." "An equality
/// the program writes, in a `requires` as elsewhere, is the language's
/// `==` (GEN-3)": here a constant expression's.
#[test]
fn the_same_constant_is_not_the_equality_the_program_writes() {
    let shared = concat!(
        "struct Scaled<const k: f64> { value: f64 }\n",
        "struct Flag<const X: bool> { v: i64 }\n",
        "const NAN: f64 = 0.0 / 0.0\n",
        "const ZEROS: bool = -0.0 == 0.0\n",
    );
    // Any two NaNs are one constant, a negative one and one an infinity
    // gives among them; and `-0.0 == 0.0` is true however it is read.
    expect(
        &format!(
            "{shared}const fn a(x: Scaled<NAN>) -> Scaled<0.0 / 0.0> => x\n\
             const fn b(x: Scaled<-(0.0 / 0.0)>) -> Scaled<NAN> => x\n\
             const fn c(x: Scaled<(1.0 / 0.0) * 0.0>) -> Scaled<NAN> => x\n\
             const fn d(x: Flag<(-0.0 == 0.0)>, y: Flag<ZEROS>) -> list<Flag<true>, 2> => [x, y]\n\
             const fn e(x: Flag<(NAN == NAN)>, y: Flag<(0.0 / 0.0 == 0.0 / 0.0)>) -> list<Flag<false>, 2> => [x, y]\n\
             const fn f(x: Flag<(-0.0 != 0.0)>, y: Flag<(NAN != NAN)>) -> list<Flag<false>, 2> => [x, Flag<false>(v: y.v)]\n"
        ),
        &[],
    );
    // Equal by `==` and still two constants: two types.
    expect(
        &format!(
            "{shared}const fn a(x: Scaled<-0.0>) -> Scaled<0.0> => x\n\
             const fn b(x: Flag<(-0.0 == 0.0)>) -> Flag<false> => x\n\
             const fn c(x: Flag<(NAN == NAN)>) -> Flag<true> => x\n"
        ),
        &["6 type.mismatch", "7 type.mismatch", "8 type.mismatch"],
    );
}

/// UNIT-13: "How a constant is written is kept with it, for diagnostics
/// ... and is never compared: a constant has one value however it is
/// written." A message names each type as its constants are written.
#[test]
fn a_message_names_a_constant_as_it_is_written() {
    let checked = module(&format!(
        "{FRAGMENT}const fn a(x: Scaled<-(0.0)>) -> Scaled<0.00> => x\n"
    ));
    assert_eq!(found(&checked), ["4 type.mismatch"]);
    assert_eq!(
        checked.findings[0].message,
        "this is `Scaled<-(0.0)>`, and the function's result is `Scaled<0.00>`"
    );
}

/// GEN-1: "Repeated parameters require equivalent types or one constant
/// (UNIT-13)". A generic value parameter written in two parameters is
/// given one constant, in a value function and in a temporal one: `1`
/// and `1.00` of an `f64` are one, and so are two NaNs; `0.0` and
/// `-0.0` are two, and so are `2` and `3`.
#[test]
fn a_repeated_generic_value_parameter_is_one_constant() {
    let shared = concat!(
        "struct Scaled<const k: f64> { value: f64 }\n",
        "const NAN: f64 = 0.0 / 0.0\n",
        "const fn pair<const k: f64>(a: Scaled<k>, b: Scaled<k>) -> f64 => k\n",
        "fn both<const k: f64>(a: Scaled<k>, b: Scaled<k>) -> f64 {\n",
        "    when { return k }\n",
        "}\n",
        "const fn sized<const N: i64>(a: list<i64, N>, b: list<i64, N>) -> i64 => N\n",
    );
    expect(
        &format!(
            "{shared}const fn p(a: Scaled<1>, b: Scaled<1.00>) -> f64 => pair(a, b)\n\
             fn q(a: Scaled<1>, b: Scaled<1.00>) -> f64 => both(a, b)\n\
             const fn r(a: Scaled<NAN>, b: Scaled<0.0 / 0.0>) -> f64 => pair(a, b)\n\
             const fn s(a: list<i64, 2>, b: list<i64, 0x2>) -> i64 => sized(a, b)\n"
        ),
        &[],
    );
    expect(
        &format!(
            "{shared}const fn p(a: Scaled<0.0>, b: Scaled<-0.0>) -> f64 => pair(a, b)\n\
             fn q(a: Scaled<0.0>, b: Scaled<-0.0>) -> f64 => both(a, b)\n\
             const fn s(a: list<i64, 2>, b: list<i64, 3>) -> i64 => sized(a, b)\n"
        ),
        &["9 type.mismatch", "10 type.mismatch", "11 type.mismatch"],
    );
}

/// UNIT-9: "how a literal is written do[es] not [matter] (UNIT-13): `= 1`
/// and `= 1.0` of an `f64` field are one default." A field two parents
/// give with those defaults is accepted, and left out of a construction.
/// The comparison of two inherited defaults that are not one, which
/// "require an explicit child override", is not built (UNIT-9 is
/// planned in `docs/rules/units-and-names.md`).
#[test]
fn one_default_written_two_ways_is_one_default() {
    expect(
        concat!(
            "abstract struct Low { x: f64 = 1 }\n",
            "abstract struct High { x: f64 = 1.0 }\n",
            "struct Both : Low, High { y: i64 }\n",
            "const fn made() -> f64 => Both(y: 1).x\n",
        ),
        &[],
    );
}

/// CTRL-6: "Switch labels are typed source constants (UNIT-13), compared
/// by resolved value. Duplicate values reject, including differently
/// spelled equal enum members, and `case 1:` beside `case 0x1:`."
#[test]
fn case_one_beside_case_hexadecimal_one_is_a_duplicate() {
    let switch = |first: &str, second: &str| {
        format!(
            "const fn s(x: i64) -> i64 {{\n    switch x {{\n        case {first}: return 1\n        \
             case {second}: return 2\n        default: return 0\n    }}\n}}\n"
        )
    };
    expect(&switch("1", "0x1"), &["5 switch.duplicate_label"]);
    expect(&switch("0b1", "1"), &["5 switch.duplicate_label"]);
    expect(&switch("1", "0x2"), &[]);
}

/// TYPE-16, proposed: "two keys, or two members, that a delta names are
/// one where its collection takes them as one, and not by their
/// sameness as constants (UNIT-13)". As `main` has it, they are compared
/// by the language's `==`: `0.0` and `-0.0` are one key, and so one
/// overlap, and two NaNs are none.
#[test]
fn a_deltas_keys_are_compared_by_the_languages_equality_as_proposed() {
    expect(
        concat!(
            "const fn s() -> delta<set<f64>> => delta<set<f64>>(added: [0.0], removed: [-0.0])\n",
            "const fn d() -> delta<dict<f64, i64>> => delta<dict<f64, i64>>(updated: [-0.0: 1], removed: [0.0])\n",
        ),
        &["2 delta.overlap", "3 delta.overlap"],
    );
    expect(
        "const fn n() -> delta<set<f64>> => delta<set<f64>>(added: [0.0 / 0.0], removed: [0.0 / 0.0])\n",
        &[],
    );
}
