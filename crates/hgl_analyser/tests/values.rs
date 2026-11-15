//! The bodies of value functions: the type of each expression, what a
//! place expects of it, and the bindings of a body. Each test cites its
//! rule.
//!
//! A finding is written `file:line:column category/code rule`; a code
//! local to this compiler has `local:` before it.

mod common;

use common::{Checked, check};
use hgl_analyser::hir::{Body, ExprKind, Stmt};

/// No findings.
const NONE: [&str; 0] = [];

/// Checks one file, `m.hgl`, of module `m`. The `module` line is added,
/// so the text given starts on line 2.
fn module(text: &str) -> Checked {
    check(&[("m.hgl", &format!("module m\n{text}"))])
}

/// The message of each finding, in order.
fn messages(checked: &Checked) -> Vec<&str> {
    checked
        .findings
        .iter()
        .map(|finding| finding.message.as_str())
        .collect()
}

/// The typed body of the value function `name` of module `m`.
fn body<'a>(checked: &'a Checked, name: &str) -> &'a Body {
    let module = checked.program.module("m").expect("module m exists");
    let index = module
        .declarations
        .iter()
        .position(|declaration| declaration.name == name)
        .expect("the declaration exists");
    module.bodies[index].as_ref().expect("it has a body")
}

/// The locals of `name` as `local: type`, parameters first; `_` for a
/// type that is not known.
fn locals(checked: &Checked, name: &str) -> Vec<String> {
    body(checked, name)
        .locals
        .iter()
        .map(|local| format!("{}: {}", local.name, local.ty))
        .collect()
}

// ---- Literals ----

/// TYPE-1, TYPE-2: each literal has the type its form gives it. LEX-11:
/// "Offset-free date/time is civil, not an implicit UTC instant. A zoned
/// datetime requires an explicit offset."
#[test]
fn a_literal_has_the_type_of_its_form() {
    let checked = module(concat!(
        "const fn literals() {\n",
        "    let whole = 1\n",
        "    let real = 1.5\n",
        "    let text = \"x\"\n",
        "    let flag = true\n",
        "    let span = 5m\n",
        "    let day = @2026-09-03\n",
        "    let clock = @09:30\n",
        "    let civil = @2026-09-03T09:30\n",
        "    let instant = @2026-09-03T09:30Z\n",
        "    let zoned = @2026-09-03T09:30+01:00[Europe/Paris]\n",
        "    let local = @09:30[Europe/Paris]\n",
        "    let zone = @[Europe/Paris]\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(
        locals(&checked, "literals"),
        [
            "whole: i64",
            "real: f64",
            "text: str",
            "flag: bool",
            "span: duration",
            "day: date",
            "clock: time",
            "civil: civil_datetime",
            "instant: datetime",
            "zoned: zoned_datetime",
            "local: zoned_time",
            "zone: timezone",
        ]
    );
}

// ---- Operators ----

/// NUM-1: "For i64 operands, `+`, `-`, `*`, `//`, and `%` return i64.
/// Unconfigured mixed i64/f64 and f64 arithmetic returns f64; `/`
/// defaults to f64 ... Unary negation preserves the numeric type."
/// NUM-6's table: a comparison is a bool.
#[test]
fn an_operator_has_the_type_cores_contract_gives() {
    let checked = module(concat!(
        "const fn operators(a: i64, b: f64) {\n",
        "    let sum = a + 2\n",
        "    let mixed = a * b\n",
        "    let real = b - 1.5\n",
        "    let ratio = 7 / 2\n",
        "    let floor = 7 // 2\n",
        "    let rest = a % 2\n",
        "    let minus = -a\n",
        "    let minus_real = -b\n",
        "    let less = a < b\n",
        "    let same = a == 1\n",
        "    let both = a < 1 && !(b >= 2.0) || true\n",
        // What these give is for core to say.
        "    let outcome = a <=> b\n",
        "    let joined = \"a\" + \"b\"\n",
        "    let grouped = (a + 1) * 2\n",
        // LEX-7: "the literal 2^63 only as the direct operand of a
        // unary minus", where the two are the least `i64`.
        "    let least = -9223372036854775808\n",
        "}\n",
        "const fn minimum() -> str => -9223372036854775808\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:19:30 local:type/type.mismatch EXPR-5"]
    );
    assert_eq!(
        locals(&checked, "operators"),
        [
            "a: i64",
            "b: f64",
            "sum: i64",
            "mixed: f64",
            "real: f64",
            "ratio: f64",
            "floor: i64",
            "rest: i64",
            "minus: i64",
            "minus_real: f64",
            "less: bool",
            "same: bool",
            "both: bool",
            "outcome: _",
            "joined: _",
            "grouped: i64",
            "least: i64",
        ]
    );
}

/// EXPR-6: the Boolean connectives and `!` are of bools.
#[test]
fn a_boolean_connective_takes_bools() {
    let checked = module(concat!(
        "const fn connectives(a: i64, flag: bool) {\n",
        "    let both = a && flag\n",
        "    let either = flag || 1.5\n",
        "    let not = !a\n",
        "    let fine = !flag && flag\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:16 local:type/type.mismatch EXPR-5",
            "m.hgl:4:26 local:type/type.mismatch EXPR-5",
            "m.hgl:5:16 local:type/type.mismatch EXPR-5",
        ]
    );
    assert_eq!(
        messages(&checked),
        [
            "this is `i64`, and an operand of `&&` is `bool`",
            "this is `f64`, and an operand of `||` is `bool`",
            "this is `i64`, and the operand of `!` is `bool`",
        ]
    );
}

// ---- What a place expects ----

/// EXPR-5: "An expected type contextualizes literals and constructors
/// ... Numeric widening follows the admitted destination relationship,
/// not arbitrary native coercion." TYPE-14: "`T?` denotes an ordinary
/// value that is either a T or null ... It does not make null an
/// admitted value of unqualified T."
#[test]
fn a_value_is_of_the_type_its_place_expects() {
    let checked = module(concat!(
        "const fn places(a: i64, b: f64) {\n",
        "    let widened: f64 = a\n",
        "    let narrowed: i64 = b\n",
        "    let text: str = 1\n",
        "    let absent: i64? = null\n",
        "    let present: i64? = a\n",
        "    let widened_too: f64? = a\n",
        "    let never: i64 = null\n",
        "    let whatever: any = a\n",
        // Nothing says which nullable type this is.
        "    let unknown = null\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:4:25 local:type/type.mismatch EXPR-5",
            "m.hgl:5:21 local:type/type.mismatch EXPR-5",
            "m.hgl:9:22 local:type/type.mismatch TYPE-14",
        ]
    );
    assert_eq!(
        messages(&checked),
        [
            "this is `f64`, and the type written for it is `i64`",
            "this is `i64`, and the type written for it is `str`",
            "`null` is no value of `i64`: write `i64?`",
        ]
    );
    // The `i64` is written down as widened where an `f64` is wanted.
    let Stmt::Declare {
        value: Some(value), ..
    } = &body(&checked, "places").statements[0]
    else {
        panic!("the first statement declares `widened`");
    };
    assert!(matches!(value.kind, ExprKind::Widen(_)));
    assert_eq!(value.ty.to_string(), "f64");
}

/// FN-7: "A value return yields an ordinary value", of the function's
/// result type. An arrow body returns its value.
#[test]
fn a_return_is_of_the_result_type() {
    let checked = module(concat!(
        "const fn whole(a: i64) -> i64 => a + 1\n",
        "const fn real(a: i64) -> f64 => a\n",
        "const fn maybe(a: i64) -> i64? => a\n",
        "const fn text(a: i64) -> str => a\n",
        "const fn block(a: i64) -> bool {\n    return a\n}\n",
        "const fn width<const N: i64>() -> i64 => N\n",
        "const fn label<const N: i64>() -> str => N\n",
        // A generic type parameter is its own type and no other.
        "const fn same<T>(value: T) -> T => value\n",
        "const fn other<T>(value: T) -> T => 1\n",
        // A `return` with nothing written yields no value.
        "const fn bare() -> i64 {\n    return\n}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:5:33 local:type/type.mismatch EXPR-5",
            "m.hgl:7:12 local:type/type.mismatch EXPR-5",
            "m.hgl:10:42 local:type/type.mismatch EXPR-5",
            "m.hgl:12:37 local:type/type.mismatch EXPR-5",
            "m.hgl:14:5 local:type/type.mismatch FN-7",
        ]
    );
    assert_eq!(
        checked.findings[0].message,
        "this is `i64`, and the function's result is `str`"
    );
    assert_eq!(
        checked.findings[4].message,
        "this returns no value, and the function's result is `i64`"
    );
}

/// TYPE-6: "A field whose type is an abstract struct holds every struct
/// that inherits from it": a value of a struct is a value of the
/// abstract structs it inherits from. TYPE-3: "Generic applications are
/// invariant."
#[test]
fn a_struct_is_a_value_of_the_abstract_structs_it_inherits_from() {
    let checked = module(concat!(
        "abstract struct Base { id: i64 }\n",
        "abstract struct Middle : Base { }\n",
        "struct Child : Middle { }\n",
        "struct Other { id: i64 }\n",
        "abstract struct Boxed<T> { value: T }\n",
        "struct Whole : Boxed<i64> { }\n",
        "const fn up(x: Child) -> Base => x\n",
        "const fn near(x: Child) -> Middle? => x\n",
        "const fn other(x: Other) -> Base => x\n",
        "const fn down(x: Base) -> Child => x\n",
        "const fn boxed(x: Whole) -> Boxed<i64> => x\n",
        "const fn unboxed(x: Whole) -> Boxed<str> => x\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:10:37 local:type/type.mismatch EXPR-5",
            "m.hgl:11:36 local:type/type.mismatch EXPR-5",
            "m.hgl:13:45 local:type/type.mismatch EXPR-5",
        ]
    );
}

// ---- Bindings ----

/// EXPR-3: "`let` is immutable; `var` may be assigned within the same
/// category/type. An uninitialized typed var supplies no value ...
/// every path reaching a read must assign it ... A compound assignment
/// reads its existing target." The chapter's illustration, and its
/// deliberate rejection: "reading `var total: i64` before assignment".
#[test]
fn a_var_is_assigned_before_it_is_read() {
    let checked = module(concat!(
        "const fn doubled(value: i64) -> i64 {\n",
        "    var total: i64 = value\n",
        "    total += value\n",
        "    return total\n",
        "}\n",
        "const fn unset() -> i64 {\n",
        "    var total: i64\n",
        "    return total\n",
        "}\n",
        "const fn set_later() -> i64 {\n",
        "    var total: i64\n",
        "    total = 1\n",
        "    return total\n",
        "}\n",
        "const fn compound() -> i64 {\n",
        "    var total: i64\n",
        "    total += 1\n",
        "    return total\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:9:12 local:name/binding.unassigned EXPR-3",
            "m.hgl:18:5 local:name/binding.unassigned EXPR-3",
        ]
    );
    assert_eq!(
        checked.findings[0].message,
        "`total` is read before it is given a value"
    );
}

/// EXPR-3: "`let` is immutable; `var` may be assigned within the same
/// category/type."
#[test]
fn only_a_var_is_assigned_and_with_its_own_type() {
    let checked = module(concat!(
        "const fn assign(value: i64) -> i64 {\n",
        "    let fixed = 1\n",
        "    var count = 1\n",
        "    var real = 1.5\n",
        "    fixed = 2\n",
        "    value = 2\n",
        "    count = 2\n",
        "    count = 2.5\n",
        "    count += 1.5\n",
        "    real = count\n",
        "    real += count\n",
        "    return count\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:6:5 local:name/binding.immutable EXPR-3",
            "m.hgl:7:5 local:name/binding.immutable EXPR-3",
            "m.hgl:9:13 local:type/type.mismatch EXPR-5",
            "m.hgl:10:14 local:type/type.mismatch EXPR-5",
        ]
    );
    assert_eq!(
        messages(&checked),
        [
            "`fixed` is a `let` and cannot be assigned: only a `var` can",
            "`value` is a parameter and cannot be assigned: only a `var` can",
            "this is `f64`, and the type of the `var` is `i64`",
            "this is `f64`, and the type of the `var` is `i64`",
        ]
    );
}

// ---- Tuples and sequences ----

/// EXPR-9: "The use site and expected type constrain element types."
/// With no expected type the values of a sequence are a list of the
/// one type they have.
#[test]
fn a_sequence_takes_its_type_from_its_place_or_its_values() {
    let checked = module(concat!(
        "const fn sequences(a: i64) {\n",
        "    let whole = [1, a]\n",
        "    let real = [1, 2.5]\n",
        "    let mixed = [1, \"a\"]\n",
        "    let wanted: list<f64> = [1, a]\n",
        "    let members: set<i64> = [1, 2]\n",
        "    let wrong: list<str> = [\"a\", 2]\n",
        "    let empty = []\n",
        "    let pair = (1, \"a\")\n",
        "    let wanted_pair: tuple<f64, str> = (a, \"a\")\n",
        "    let wrong_pair: tuple<i64, i64> = (1, \"a\")\n",
        // TYPE-14: a `T?` is "either a T or null", so what is expected
        // of a sequence or a tuple that is no `null` is the `T`.
        "    let maybe: set<i64>? = [1, 2]\n",
        "    let maybe_real: list<f64>? = [1, a]\n",
        "    let maybe_pair: tuple<f64, str>? = (a, \"a\")\n",
        "    let maybe_wrong: list<str>? = [1]\n",
        // TYPE-10: "Size participates in identity."
        "    let sized: list<i64, 2> = [1, 2]\n",
        "    let short: list<i64, 2> = [1]\n",
        // `null` takes its type from its place: among values, theirs.
        "    let holes = [1, null]\n",
        "    let maybe_holes: list<i64?> = [1, null]\n",
        "    let nothing = [null]\n",
        "}\n",
        "const fn member_set() -> set<i64>? => [1, 2]\n",
        "const fn trio() -> list<i64, 2> => [1, 2, 3]\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:5:21 local:type/type.mismatch EXPR-5",
            "m.hgl:8:34 local:type/type.mismatch EXPR-5",
            "m.hgl:12:43 local:type/type.mismatch EXPR-5",
            "m.hgl:16:36 local:type/type.mismatch EXPR-5",
            "m.hgl:18:31 local:type/type.mismatch TYPE-10",
            "m.hgl:19:21 local:type/type.mismatch TYPE-14",
            "m.hgl:24:36 local:type/type.mismatch TYPE-10",
        ]
    );
    assert_eq!(
        checked.findings[4].message,
        "this has 1 entry, and a `list<i64, 2>` has 2"
    );
    assert_eq!(
        locals(&checked, "sequences"),
        [
            "a: i64",
            "whole: list<i64>",
            "real: list<f64>",
            "mixed: list<i64>",
            "wanted: list<f64>",
            "members: set<i64>",
            "wrong: list<str>",
            "empty: _",
            "pair: tuple<i64, str>",
            "wanted_pair: tuple<f64, str>",
            "wrong_pair: tuple<i64, i64>",
            "maybe: set<i64>?",
            "maybe_real: list<f64>?",
            "maybe_pair: tuple<f64, str>?",
            "maybe_wrong: list<str>?",
            "sized: list<i64, 2>",
            "short: list<i64, 2>",
            "holes: list<i64>",
            "maybe_holes: list<i64?>",
            "nothing: _",
        ]
    );
}

// ---- What is not read yet ----

/// ERR-9: what a later pass reads has no type here, and nothing is
/// reported of it or of what uses it.
#[test]
fn a_form_not_read_yet_is_not_reported() {
    let checked = module(concat!(
        "struct Quote { bid: f64 }\n",
        "const fn helper(a: i64) -> i64 => a\n",
        "const fn later(a: i64, quote: Quote) -> str {\n",
        "    let called = helper(a)\n",
        "    let member = quote.bid\n",
        "    let prelude = missing(a)\n",
        "    var total: i64\n",
        "    if (a > 1) {\n        total = 1\n    } else {\n        total = 2\n    }\n",
        "    let after: str = total + called\n",
        "    return called\n",
        "}\n",
        // A temporal function is for the pass that reads phases.
        "fn temporal(a: i64) -> str => a\n",
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(
        locals(&checked, "later"),
        [
            "a: i64",
            "quote: Quote",
            "called: _",
            "member: _",
            "prelude: _",
            "total: i64",
            "after: str",
        ]
    );
}
