//! Rules of names, types, expressions and conversions: a chained
//! comparison (EXPR-1), the defaults a struct has from its parents
//! (UNIT-9), `const` as a name (FN-5), what a source returns and where
//! it yields (SOURCE-4), a tolerance and full comparison (core's NUM-7),
//! and what a conversion is (core's OPS-6, OPS-7). Each test cites its
//! rule.
//!
//! A finding is written `file:line:column category/code rule`; a code
//! local to this compiler has `local:` before it.

mod common;

use common::{Checked, check};
use hgl_analyser::hir::{Body, Callee, Expr, ExprKind, Form, Stmt};
use hgl_analyser::{DeclarationKind, Detail, Role};

/// No findings.
const NONE: [&str; 0] = [];

/// Checks one file, `m.hgl`, of module `m`. The `module` line is added,
/// so the text given starts on line 2.
fn module(text: &str) -> Checked {
    check(&[("m.hgl", &format!("module m\n{text}"))])
}

/// The typed body of the function `name` of module `m`.
fn body<'a>(checked: &'a Checked, name: &str) -> &'a Body {
    let module = checked.program.module("m").expect("module m exists");
    let index = module
        .declarations
        .iter()
        .position(|declaration| declaration.name == name)
        .expect("the declaration exists");
    module.bodies[index].as_ref().expect("it has a body")
}

/// The value the function `name` of module `m` returns: its arrow
/// body's.
fn returned<'a>(checked: &'a Checked, name: &str) -> &'a Expr {
    match body(checked, name).statements.as_slice() {
        [
            Stmt::Return {
                value: Some(value), ..
            },
        ] => value,
        other => panic!("`{name}` returns one value: {other:?}"),
    }
}

/// The locals of `name` as `local: type`, parameters first.
fn locals(checked: &Checked, name: &str) -> Vec<String> {
    body(checked, name)
        .locals
        .iter()
        .map(|local| format!("{}: {}", local.name, local.ty))
        .collect()
}

// ---- EXPR-1 ----

/// EXPR-1: "Repeated binary forms associate left ... A chained
/// comparison is not implicit mathematical conjunction." `a < b < c` is
/// `(a < b) < c`: a `bool` compared with `c`. Of three `i64`s that is a
/// `bool` beside an `i64`, which no candidate of `<` takes (OPS-2); of
/// three `bool`s it is admitted, a `bool` being ordered.
#[test]
fn a_chained_comparison_is_typed_as_written() {
    let checked = module(concat!(
        "const fn chained(a: i64, b: i64, c: i64) -> bool => a < b < c\n",
        "const fn flags(a: bool, b: bool, c: bool) -> bool => a < b < c\n",
        "const fn equal(a: i64, b: i64, c: bool) -> bool => a < b == c\n",
        "const fn spelled(a: i64, b: i64, c: i64) -> bool => a < b && b < c\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:2:59 operator/operator.no_candidate OPS-2"]
    );
    assert_eq!(
        checked.findings[0].message,
        "`<` takes no `bool` and `i64` to give `bool`"
    );
    // The left operand of the second `<` is the first.
    let ExprKind::Call {
        arguments, form, ..
    } = &returned(&checked, "flags").kind
    else {
        panic!("a call of `<`");
    };
    assert!(matches!(form, Form::Binary(_)));
    let inner = &arguments[0].value;
    assert!(matches!(
        inner.kind,
        ExprKind::Call {
            form: Form::Binary(_),
            ..
        }
    ));
    assert_eq!(inner.ty.to_string(), "bool");
}

// ---- UNIT-9 ----

/// UNIT-9: "Compare inherited defaults by bound expression and
/// canonical type after substitution, before folding. Declaration
/// identity matters; source location and alias spelling do not.
/// Conflicting inherited defaults require an explicit child override."
///
/// Each struct below the first two has a field from two parents. The
/// same constant read through a selective import and through an alias
/// is one default; so is a generic value parameter given the number
/// the other parent writes, and an enum member written in full or as
/// `.member`, and `1` where an `f64` is wanted is `1.0` (EXPR-5).
/// `[]` is not `[1]`. A struct that writes the field itself chooses.
///
/// Only two constant values of the field's type are judged: `1 + 1`
/// against `2`, and a call against a call, are not, and nothing is
/// reported of them.
#[test]
fn defaults_from_two_parents_are_one_or_the_struct_chooses() {
    let checked = check(&[
        (
            "shared.hgl",
            concat!(
                "module shared\n",
                "export const scale: i64 = 2\n",
                "export enum Side { buy, sell }\n",
            ),
        ),
        (
            "m.hgl",
            concat!(
                "module m\n",
                "use shared::{scale, Side}\n",
                "use shared as sh\n",
                "abstract struct Base { size: i64 }\n",
                "abstract struct ByName : Base { size = scale }\n",
                "abstract struct ByAlias : Base { size = sh::scale }\n",
                "struct Same : ByName, ByAlias {}\n",
                "abstract struct Sum : Base { size = 1 + 1 }\n",
                "abstract struct Two : Base { size = 2 }\n",
                "struct Folded : Sum, Two {}\n",
                "abstract struct Sized<const N: i64> : Base { size = N }\n",
                "abstract struct Three : Base { size = 3 }\n",
                "struct Agree : Sized<3>, Three {}\n",
                "struct Differ : Sized<4>, Three {}\n",
                "abstract struct A { size: i64 = 1 }\n",
                "abstract struct B { size: i64 = 1 }\n",
                "abstract struct C { size: i64 = 2 }\n",
                "struct AB : A, B {}\n",
                "struct AC : A, C {}\n",
                "struct Chosen : A, C { size = 5 }\n",
                "abstract struct Sided { side: Side }\n",
                "abstract struct Short : Sided { side = .buy }\n",
                "abstract struct Full : Sided { side = sh::Side.buy }\n",
                "struct Sides : Short, Full {}\n",
                "abstract struct Real { x: f64 }\n",
                "abstract struct Whole : Real { x = 1 }\n",
                "abstract struct Float : Real { x = 1.0 }\n",
                "struct Mixed : Whole, Float {}\n",
                "abstract struct Holder<T> { items: list<T> = [] }\n",
                "abstract struct Passed<T> : Holder<T> {}\n",
                "abstract struct Empty : Holder<i64> { items = [] }\n",
                "abstract struct One : Holder<i64> { items = [1] }\n",
                "struct Kept : Passed<i64>, Empty {}\n",
                "struct Changed : Passed<i64>, One {}\n",
                "abstract struct Mid : ByName {}\n",
                "struct Diamond : Mid, ByName {}\n",
                // A call is no constant value: neither pair is judged.
                "const fn limit<const N: i64>() -> i64 => N\n",
                "abstract struct Limited<const N: i64> : Base { size = limit<N>() }\n",
                "abstract struct Capped : Base { size = limit<3>() }\n",
                "struct Called : Limited<3>, Capped {}\n",
                "struct Miscalled : Limited<4>, Capped {}\n",
            ),
        ),
    ]);
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:14:27 local:type/default.conflict UNIT-9",
            "m.hgl:19:16 local:type/default.conflict UNIT-9",
            "m.hgl:34:31 local:type/default.conflict UNIT-9",
        ]
    );
    assert_eq!(
        checked.findings[0].message,
        "`Differ` has `size` from two parents with different defaults, and gives it none of \
         its own: write `size = ...` in `Differ`"
    );
}

/// UNIT-9: "Compare inherited defaults by bound expression and
/// canonical type after substitution". A generic value parameter that
/// is a `bool`, a `str` or an `f64` is the literal its path gives it. A
/// minus before a number is the negative number: `Counts<-3>` gives
/// `size = N` what `size = -3` is. A call, `mark<1.0>()` against
/// `mark<2.0>()` among them, is no constant value and is not judged.
#[test]
fn literal_constants_in_defaults_are_compared_by_value() {
    let checked = module(concat!(
        "abstract struct Flagged { enabled: bool }\n",
        "abstract struct ByParam<const B: bool> : Flagged { enabled = B }\n",
        "abstract struct Off : Flagged { enabled = false }\n",
        "struct Clash : ByParam<true>, Off {}\n",
        "struct Agree : ByParam<false>, Off {}\n",
        "abstract struct Named { label: str }\n",
        "abstract struct ByLabel<const S: str> : Named { label = S }\n",
        "abstract struct Fixed : Named { label = \"a\" }\n",
        "struct LabelSame : ByLabel<\"a\">, Fixed {}\n",
        "struct LabelClash : ByLabel<\"b\">, Fixed {}\n",
        "const fn mark<const X: f64>() -> f64 => X\n",
        "abstract struct Scaled { scale: f64 }\n",
        "abstract struct One : Scaled { scale = mark<1.0>() }\n",
        "abstract struct OneAgain : Scaled { scale = mark<1.00>() }\n",
        "abstract struct Two : Scaled { scale = mark<2.0>() }\n",
        "struct Marked : One, OneAgain {}\n",
        "struct Remarked : One, Two {}\n",
        "abstract struct ByReal<const R: f64> : Scaled { scale = R }\n",
        "abstract struct Half : Scaled { scale = 0.5 }\n",
        "struct RealSame : ByReal<0.50>, Half {}\n",
        "struct RealClash : ByReal<0.25>, Half {}\n",
        // A minus before a number is the negative number, in a
        // generic argument and in a default alike.
        "abstract struct Counted { size: i64 }\n",
        "abstract struct Counts<const N: i64> : Counted { size = N }\n",
        "abstract struct Minus : Counted { size = -3 }\n",
        "struct IntSame : Counts<-3>, Minus {}\n",
        "abstract struct Negative : Scaled { scale = -0.5 }\n",
        "struct NegativeSame : ByReal<-0.5>, Negative {}\n",
        "struct NegativeClash : ByReal<-0.25>, Negative {}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:5:31 local:type/default.conflict UNIT-9",
            "m.hgl:11:35 local:type/default.conflict UNIT-9",
            "m.hgl:22:34 local:type/default.conflict UNIT-9",
            "m.hgl:29:39 local:type/default.conflict UNIT-9",
        ]
    );
}

/// UNIT-9, GEN-8: a constant given to a generic value parameter is a
/// compile-time constant, and is compared by its value, not its text:
/// in parentheses or not, after a minus or as a negative argument, with
/// an underscore in its digits, by the name of an `i64` constant, or as
/// an integer where an `f64` is wanted (EXPR-5). A minus before a
/// number is that negative number, by symbol or by `neg_`. Each is
/// checked both ways: one value agrees, and another is reported.
#[test]
fn constants_in_defaults_are_compared_by_value_however_written() {
    let checked = module(concat!(
        "abstract struct Flagged { enabled: bool }\n",
        "abstract struct ByParam<const B: bool> : Flagged { enabled = B }\n",
        "abstract struct Off : Flagged { enabled = false }\n",
        "struct GroupedClash : ByParam<(true)>, Off {}\n",
        "struct GroupedSame : ByParam<(false)>, Off {}\n",
        "const fn mark<const X: f64>() -> f64 => X\n",
        "abstract struct Scaled { scale: f64 }\n",
        "abstract struct One : Scaled { scale = mark<1.0>() }\n",
        "abstract struct Grouped : Scaled { scale = mark<(1.0)>() }\n",
        "abstract struct Whole : Scaled { scale = mark<1>() }\n",
        "abstract struct GroupedTwo : Scaled { scale = mark<(2.0)>() }\n",
        "struct MarkSame : One, Grouped {}\n",
        "struct MarkWhole : One, Whole {}\n",
        "struct MarkClash : One, GroupedTwo {}\n",
        "abstract struct ByReal<const R: f64> : Scaled { scale = R }\n",
        "abstract struct Unit : Scaled { scale = 1.0 }\n",
        "struct RealWhole : ByReal<1>, Unit {}\n",
        "struct RealWholeClash : ByReal<2>, Unit {}\n",
        "abstract struct Negative : Scaled { scale = -0.5 }\n",
        "struct SignSame : ByReal<-(0.5)>, Negative {}\n",
        "struct SignClash : ByReal<(0.5)>, Negative {}\n",
        "const limit: i64 = 1000\n",
        "abstract struct Counted { size: i64 }\n",
        "abstract struct Counts<const N: i64> : Counted { size = N }\n",
        "abstract struct Thousand : Counted { size = 1000 }\n",
        "struct DigitsSame : Counts<1_000>, Thousand {}\n",
        "struct NamedSame : Counts<limit>, Thousand {}\n",
        "struct NamedClash : Counts<limit + 1>, Thousand {}\n",
        // A negation by symbol and by core's operator's name is one
        // (EXPR-10).
        "abstract struct ByName : Scaled { scale = neg_(0.5) }\n",
        "abstract struct ByNameOther : Scaled { scale = neg_(0.25) }\n",
        "struct NegationSame : Negative, ByName {}\n",
        "struct NegationClash : Negative, ByNameOther {}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:5:40 local:type/default.conflict UNIT-9",
            "m.hgl:19:36 local:type/default.conflict UNIT-9",
            "m.hgl:22:35 local:type/default.conflict UNIT-9",
            "m.hgl:29:40 local:type/default.conflict UNIT-9",
            "m.hgl:33:34 local:type/default.conflict UNIT-9",
        ]
    );
}

/// UNIT-9 rejects only what it can tell: two defaults that are each a
/// constant value of the field's type as the struct has it, its
/// parents' generic arguments in place, and that are two values. A
/// default of a field whose type names a generic parameter is read with
/// no type expected; once the arguments are in place it is typed
/// against the field's type, so `x: T = 1` under `A<f64>` is `1.0`
/// (EXPR-5). Two defaults that are not both constant values, `1 + 2`
/// among them, a generic value parameter of the struct below left as
/// one, a date or time literal, and a field whose type is not concrete
/// are not judged.
#[test]
fn only_two_constant_values_of_the_fields_type_are_judged() {
    let checked = module(concat!(
        "abstract struct A<T> { x: T = 1 }\n",
        "abstract struct One { x: f64 = 1.0 }\n",
        "abstract struct Two { x: f64 = 2.0 }\n",
        "struct Widened : A<f64>, One {}\n",
        "struct WidenedClash : A<f64>, Two {}\n",
        "abstract struct Counted { size: i64 }\n",
        "abstract struct Counts<const N: i64> : Counted { size = N }\n",
        "abstract struct Summed : Counted { size = 1 + 2 }\n",
        "abstract struct Four : Counted { size = 4 }\n",
        "struct Unjudged : Summed, Four {}\n",
        "abstract struct Below<const K: i64> : Counts<K>, Four {}\n",
        "abstract struct Day { on: date = @2026-01-01 }\n",
        "abstract struct Other { on: date = @2026-01-02 }\n",
        "struct Dated : Day, Other {}\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:6:31 local:type/default.conflict UNIT-9"]
    );
}

/// UNIT-9: every two defaults a struct has for one field are compared,
/// so a default that is not judged, `1 + 0`, hides no two that are,
/// whatever order the parents are written in.
#[test]
fn a_default_that_is_not_judged_hides_no_conflict() {
    let checked = module(concat!(
        "abstract struct Sum { x: i64 = 1 + 0 }\n",
        "abstract struct One { x: i64 = 1 }\n",
        "abstract struct Two { x: i64 = 2 }\n",
        "struct First : Sum, One, Two {}\n",
        "struct Middle : One, Sum, Two {}\n",
        "struct Agreeing : Sum, One {}\n",
        "abstract struct Base { size: i64 }\n",
        "abstract struct Summed : Base { size = 1 + 0 }\n",
        "abstract struct Single : Base { size = 1 }\n",
        "abstract struct Double : Base { size = 2 }\n",
        "struct Paths : Summed, Single, Double {}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:5:26 local:type/default.conflict UNIT-9",
            "m.hgl:6:27 local:type/default.conflict UNIT-9",
            "m.hgl:12:32 local:type/default.conflict UNIT-9",
        ]
    );
}

/// UNIT-9 compares the defaults of one field. Two parents' fields of
/// one name and two types are a `field.conflict` (TYPE-4), and are not
/// reported again for their defaults.
#[test]
fn defaults_of_fields_that_conflict_in_type_are_not_compared() {
    let checked = module(concat!(
        "abstract struct Whole { size: i64 = 1 }\n",
        "abstract struct Real { size: f64 = 2.0 }\n",
        "struct Both : Whole, Real {}\n",
    ));
    assert_eq!(checked.summary(), ["m.hgl:4:22 type/field.conflict TYPE-4"]);
}

/// UNIT-9: of two parents' fields that are one field, where only one
/// path gives it a default, that is the default it has, whatever order
/// the parents are written in. A construction may leave it out (TYPE-5:
/// "A field with a default, its own or one a struct below gives it, may
/// be left out").
#[test]
fn a_default_one_parent_gives_is_the_fields_in_either_order() {
    let checked = module(concat!(
        "abstract struct Base { size: i64 }\n",
        "abstract struct Given : Base { size = 1 }\n",
        "abstract struct Plain : Base {}\n",
        "struct First : Given, Plain {}\n",
        "struct Second : Plain, Given {}\n",
        "abstract struct Own { size: i64 = 1 }\n",
        "abstract struct Bare { size: i64 }\n",
        "struct Third : Bare, Own {}\n",
        "const fn made() -> i64 => First().size + Second().size + Third().size\n",
    ));
    assert_eq!(checked.summary(), NONE);
    let module = checked.program.module("m").expect("module m exists");
    for name in ["First", "Second", "Third"] {
        let index = module
            .declarations
            .iter()
            .position(|declaration| declaration.name == name)
            .expect("the struct exists");
        let Detail::Struct(read) = &module.details[index] else {
            panic!("{name} is a struct");
        };
        let size = read
            .members
            .iter()
            .find(|member| member.name == "size")
            .expect("it has `size`");
        assert!(size.has_default && size.default.is_some(), "{name}");
    }
}

// ---- FN-5 ----

/// FN-5: "`const` is a modifier only in a position admitting that
/// modifier. Otherwise it is an ordinary name, including for an
/// operator or function. `const(argument)` is an ordinary call
/// resolved by the usual naming and phase rules; it has no intrinsic
/// function-role selection behavior." The accepted fragments, and a
/// graph that calls a temporal and value pair named `const`, which
/// selects the temporal one (FN-4).
#[test]
fn a_call_of_const_is_an_ordinary_call() {
    let checked = module(concat!(
        "const fn const(value: i64) -> i64 => value\n",
        "const fn example() -> i64 => const(7)\n",
        "fn const(value: i64) -> i64 {\n",
        "    when { return value }\n",
        "}\n",
        "fn wired(x: i64) -> i64 => const(x)\n",
    ));
    assert_eq!(checked.summary(), NONE);
    let role = |name: &str| {
        let ExprKind::Call {
            callee:
                Callee::Function {
                    module,
                    declaration,
                    ..
                },
            ..
        } = &returned(&checked, name).kind
        else {
            panic!("`{name}` calls a function");
        };
        let declared = &checked.program.modules[*module].declarations[*declaration];
        assert_eq!(declared.name, "const");
        declared.kind
    };
    assert_eq!(role("example"), DeclarationKind::Function(Role::Value));
    assert_eq!(role("wired"), DeclarationKind::Function(Role::Temporal));
}

/// FN-5: "`const(argument)` is an ordinary call resolved by the usual
/// naming and phase rules". With nothing named `const` it names
/// nothing (UNIT-10); a value function does not call a temporal
/// function of that name (FN-3).
#[test]
fn a_call_of_const_is_resolved_by_the_usual_rules() {
    let unbound = module("const fn example() -> i64 => const(7)\n");
    assert_eq!(unbound.summary(), ["m.hgl:2:30 name/name.unknown UNIT-10"]);
    let temporal = module(concat!(
        "fn const(value: i64) -> i64 {\n",
        "    when { return value }\n",
        "}\n",
        "const fn example() -> i64 => const(7)\n",
    ));
    assert_eq!(
        temporal.summary(),
        ["m.hgl:5:30 function-kind/call.role FN-3"]
    );
}

// ---- SOURCE-4 ----

/// SOURCE-4: "A bare return or body completion finishes the source ...
/// A valued return and yield inside a value expression reject." A
/// `yield` stands in the source's statements, and in the blocks of the
/// `if`, `while` and `for` statements and of a block among them; a
/// block or an `if` whose value is used is a value expression.
#[test]
fn a_source_returns_no_value_and_yields_in_statements() {
    let checked = module(concat!(
        "fn kept(const done: bool, const values: list<i64>) -> i64 {\n",
        "    yield 1s: 7\n",
        "    if done { return }\n",
        "    {\n",
        "        yield 1s: 8\n",
        "    }\n",
        "    if done {\n",
        "        yield 1s: 9\n",
        "    } else {\n",
        "        yield 2s: 9\n",
        "    }\n",
        "    for value in values {\n",
        "        yield 1s: value\n",
        "    }\n",
        "}\n",
        "fn valued(const done: bool) -> i64 {\n",
        "    yield 1s: 7\n",
        "    if done { return 9 }\n",
        "}\n",
        "fn in_block() -> i64 {\n",
        "    let x = {\n",
        "        yield 1s: 7\n",
        "        3\n",
        "    }\n",
        "    yield 1s: x\n",
        "}\n",
        "fn in_if(const done: bool) -> i64 {\n",
        "    let x = if done {\n",
        "        yield 1s: 7\n",
        "        3\n",
        "    } else {\n",
        "        4\n",
        "    }\n",
        "    yield 1s: x\n",
        "}\n",
        "fn in_loop(const values: list<i64>) -> i64 {\n",
        "    let x = {\n",
        "        for value in values {\n",
        "            yield 1s: value\n",
        "        }\n",
        "        3\n",
        "    }\n",
        "    yield 1s: x\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:19:22 local:phase/source.return_value SOURCE-4",
            "m.hgl:23:9 local:phase/yield.in_value SOURCE-4",
            "m.hgl:30:9 local:phase/yield.in_value SOURCE-4",
            "m.hgl:40:13 local:phase/yield.in_value SOURCE-4",
        ]
    );
}

// ---- NUM-7 ----

/// Core's NUM-7: "A tolerance option grants no ordering law ...
/// Approximate equality does not configure `<=>`." No candidate of
/// `compare_` or of an ordering declares `abs` or `rel`, so a tolerance
/// given to one, by symbol or by name, is an option nothing declares
/// (EXPR-10, EXPR-2). Equality takes it (NUM-4). Core declares no
/// operation that orders a domain, so none asks for a total-order
/// policy.
#[test]
fn a_tolerance_configures_no_comparison_but_equality() {
    let checked = module(concat!(
        "const fn close(a: f64, b: f64) -> bool => a =={abs: 0.1} b\n",
        "const fn full(a: f64, b: f64) -> CmpResult => a <=> b\n",
        "const fn loose(a: f64, b: f64) -> CmpResult => a <=>{abs: 0.1} b\n",
        "const fn named(a: f64, b: f64) -> CmpResult => compare_(a, b, rel: 0.1)\n",
        "const fn less(a: f64, b: f64) -> bool => a <{abs: 0.1} b\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:4:54 name/argument.unknown EXPR-10",
            "m.hgl:5:63 name/argument.unknown EXPR-2",
            "m.hgl:6:46 name/argument.unknown EXPR-10",
        ]
    );
}

// ---- OPS-6, OPS-7 ----

/// Core's OPS-6: "A conversion yields what the third table below says."
/// A conversion that cannot fail is the type asked for; one that can is
/// a `result` of it (OPS-7, OPS-8).
#[test]
fn each_conversion_is_of_the_type_its_table_gives() {
    let checked = module(concat!(
        "const fn uses(n: i64, real: f64, flag: bool, text: str, raw: bytes, day: date, moment: datetime, step: tenor) {\n",
        "    let a = convert<f64>(n)\n",
        "    let b = convert<i64>(real)\n",
        "    let c = convert<bool>(n)\n",
        "    let d = convert<bool>(real)\n",
        "    let e = convert<i64>(flag)\n",
        "    let f = convert<f64>(flag)\n",
        "    let g = convert<str>(n)\n",
        "    let h = convert<str>(real)\n",
        "    let i = convert<str>(flag)\n",
        "    let j = convert<i64>(text)\n",
        "    let k = convert<f64>(text)\n",
        "    let l = convert<bool>(text)\n",
        "    let m = convert<bytes>(text)\n",
        "    let o = convert<str>(raw)\n",
        "    let p = convert<datetime>(day)\n",
        "    let q = convert<date>(moment)\n",
        "    let r = convert<str>(step)\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(
        locals(&checked, "uses")[8..],
        [
            "a: f64",
            "b: result<i64, error>",
            "c: bool",
            "d: bool",
            "e: i64",
            "f: f64",
            "g: str",
            "h: str",
            "i: str",
            "j: result<i64, error>",
            "k: result<f64, error>",
            "l: result<bool, error>",
            "m: bytes",
            "o: result<str, error>",
            "p: datetime",
            "q: date",
            "r: str",
        ]
    );
}

/// Core's OPS-6: "`convert<list<B>>(values)` is admitted too, where
/// `values` is a `list<A>` and the struct `A` inherits from the abstract
/// struct `B`; it yields the same elements." Core declares no candidate
/// for it, so the analyser admits it: through any number of parents,
/// each with the arguments the struct below gives it (TYPE-3). It
/// cannot fail and takes no option, so it is the list asked for (OPS-8).
///
/// A list of a struct to a list of that struct, of a struct that
/// inherits from no `B`, of a size, or of nullable elements is no such
/// conversion, and neither is a `result` of the list.
#[test]
fn a_list_of_structs_converts_to_a_list_of_their_parent() {
    let checked = module(concat!(
        "abstract struct Shape {}\n",
        "abstract struct Polygon : Shape {}\n",
        "struct Round : Shape { radius: f64 }\n",
        "struct Square : Polygon { side: f64 }\n",
        "struct Loose { x: f64 }\n",
        "abstract struct Tagged<T> : Shape { label: T }\n",
        "struct Tag<T> : Tagged<T> {}\n",
        "const fn widened(values: list<Round>) -> list<Shape> => convert<list<Shape>>(values)\n",
        "const fn deep(values: list<Square>) -> list<Shape> => convert<list<Shape>>(values)\n",
        "const fn parents(values: list<Polygon>) -> list<Shape> => convert<list<Shape>>(values)\n",
        "const fn generic(values: list<Tag<i64>>) -> list<Tagged<i64>> => convert<list<Tagged<i64>>>(values)\n",
        "const fn same(values: list<Round>) -> list<Round> => convert<list<Round>>(values)\n",
        "const fn unrelated(values: list<Loose>) -> list<Shape> => convert<list<Shape>>(values)\n",
        "const fn sized(values: list<Round, 2>) -> list<Shape, 2> => convert<list<Shape, 2>>(values)\n",
        "const fn optional(values: list<Round?>) -> list<Shape?> => convert<list<Shape?>>(values)\n",
        "const fn other(values: list<Tag<i64>>) -> list<Tagged<f64>> => convert<list<Tagged<f64>>>(values)\n",
        "const fn sibling(values: list<Round>) -> list<Polygon> => convert<list<Polygon>>(values)\n",
        "const fn checked(values: list<Round>) -> result<list<Shape>, error> => convert<list<Shape>>(values)\n",
        "const fn forced(values: list<Round>) -> list<Shape> => convert<list<Shape>>(values, invalid: .fail)\n",
        // By the operator's name for its parameter (EXPR-2), and with
        // its other generic arguments written: the value's type, and
        // the result, which is the list asked for.
        "const fn named(values: list<Round>) -> list<Shape> => convert<list<Shape>>(value: values)\n",
        "const fn written(values: list<Round>) -> list<Shape> => convert<list<Shape>, list<Round>, list<Shape>>(values)\n",
        "const fn output(values: list<Round>) -> str => convert<list<Shape>, list<Round>, str>(values)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:13:54 operator/operator.no_candidate GEN-5",
            "m.hgl:14:59 operator/operator.no_candidate GEN-5",
            "m.hgl:15:61 operator/operator.no_candidate GEN-5",
            "m.hgl:16:60 operator/operator.no_candidate GEN-5",
            "m.hgl:17:64 operator/operator.no_candidate GEN-5",
            "m.hgl:18:59 operator/operator.no_candidate GEN-5",
            "m.hgl:19:72 operator/operator.no_candidate GEN-5",
            "m.hgl:20:56 operator/operator.no_candidate GEN-5",
            "m.hgl:23:48 operator/operator.no_candidate GEN-5",
        ]
    );
    for name in ["widened", "deep", "parents", "generic", "named", "written"] {
        let value = returned(&checked, name);
        assert!(
            matches!(value.kind, ExprKind::ParentList(_)),
            "{name}: {value:?}"
        );
    }
    assert_eq!(returned(&checked, "widened").ty.to_string(), "list<Shape>");
    assert_eq!(
        returned(&checked, "generic").ty.to_string(),
        "list<Tagged<i64>>"
    );
}

/// Core's OPS-7: "A conversion is configured as division is (NUM-3,
/// NUM-9). Options select how an `f64` is rounded to an `i64` and what
/// a NaN becomes ... Its error policy is checked wherever it can be:
/// the conversion then returns `result<T, error>` ... A policy under
/// which a failure escapes instead is an option." The accepted
/// fragments, with `price` an `f64`, are of the types the chapter gives.
/// Its deliberate rejection, `invalid: policy` with `policy` no
/// constant, leaves the result type unresolved (GEN-8). A conversion
/// that cannot fail takes no `invalid`, and only an `f64` to an `i64`
/// takes `rounding` and `nan`.
#[test]
fn a_conversions_options_and_the_result_its_policy_returns() {
    let checked = module(concat!(
        "const fn uses(price: f64, policy: InvalidPolicy, n: i64, text: str) {\n",
        "    let a = convert<i64>(price)\n",
        "    let b = convert<i64>(price, rounding: .half_even)\n",
        "    let c = convert<i64>(price, nan: 0)\n",
        "    let d = convert<i64>(price, invalid: .fail)\n",
        "    let e = convert<f64>(\"1.5\")\n",
        "    let f = convert<i64>(price, invalid: .error)\n",
        "    let bad1 = convert<i64>(price, invalid: policy)\n",
        "    let bad2 = convert<f64>(n, invalid: .fail)\n",
        "    let bad3 = convert<i64>(text, rounding: .floor)\n",
        "    let bad4 = convert<i64>(price, nan: 1.5)\n",
        "}\n",
        "const fn plain(price: f64) -> i64 => convert<i64>(price)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:9:45 type/generic.argument_constant GEN-8",
            "m.hgl:10:16 operator/operator.no_candidate GEN-5",
            "m.hgl:11:16 operator/operator.no_candidate GEN-5",
            "m.hgl:12:16 operator/operator.no_candidate GEN-5",
            "m.hgl:14:38 operator/operator.no_candidate GEN-5",
        ]
    );
    assert_eq!(
        locals(&checked, "uses")[4..10],
        [
            "a: result<i64, error>",
            "b: result<i64, error>",
            "c: result<i64, error>",
            "d: i64",
            "e: result<f64, error>",
            "f: result<i64, error>",
        ]
    );
}
