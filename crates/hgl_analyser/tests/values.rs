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
        "const fn later(a: i64) -> str {\n",
        "    let prelude = missing(a)\n",
        "    let configured = a /{zero: .ieee} 2\n",
        "    var total: i64\n",
        "    if (a > 1) {\n        total = 1\n    } else {\n        total = 2\n    }\n",
        "    let after: str = total + prelude\n",
        "    return prelude\n",
        "}\n",
        // A temporal function is for the pass that reads phases.
        "fn temporal(a: i64) -> str => a\n",
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(
        locals(&checked, "later"),
        [
            "a: i64",
            "prelude: _",
            "configured: _",
            "total: i64",
            "after: str",
        ]
    );
}

// ---- Calls and constructions ----

/// EXPR-2: "Positional arguments precede named ones; reject duplicates,
/// unknown names, missing required arguments, and wrong types." UNIT-8:
/// "A parameter whose type is written `T?` needs no default to be
/// omitted." FN-4: "runtime/value evaluation chooses the value
/// definition."
#[test]
fn an_argument_is_given_to_its_parameter() {
    let checked = module(concat!(
        "const fn scale(value: f64, factor: f64 = 2.0, bias: f64? = null, note: str?) -> f64 => value\n",
        "fn temporal(value: f64) -> f64 => value\n",
        "const fn calls(a: i64, text: str) -> f64 {\n",
        "    let plain = scale(1.5)\n",
        "    let widened = scale(a, factor: a)\n",
        "    let named = scale(value: 1.5, bias: 2.0)\n",
        "    let wrong = scale(text)\n",
        "    let missing = scale(factor: 2.0)\n",
        "    let unknown = scale(1.5, weight: 2.0)\n",
        "    let twice = scale(1.5, value: 2.5)\n",
        "    let order = scale(value: 1.5, 2.5)\n",
        "    let many = scale(1.5, 2.5, 3.5, \"n\", 5.5)\n",
        "    let role = temporal(1.5)\n",
        // A name left for the prelude is not read, and its arguments
        // are.
        "    let left = prelude(a, !text)\n",
        "    return plain\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:8:23 local:type/type.mismatch EXPR-5",
            "m.hgl:9:19 local:type/argument.missing EXPR-2",
            "m.hgl:10:30 local:name/argument.unknown EXPR-2",
            "m.hgl:11:28 local:name/argument.duplicate EXPR-2",
            "m.hgl:12:35 local:type/argument.order EXPR-2",
            "m.hgl:13:42 local:type/argument.unexpected EXPR-2",
            "m.hgl:14:16 local:function-kind/call.role FN-4",
            "m.hgl:15:28 local:type/type.mismatch EXPR-5",
        ]
    );
    assert_eq!(
        messages(&checked),
        [
            "this is `str`, and the parameter `value` is `f64`",
            "`scale` is given no `value`, a parameter with no default",
            "`scale` has no parameter `weight`",
            "`value` is given twice",
            "an argument with no name follows a named one: positional arguments come first",
            "`scale` has 4 parameters, and this is one more",
            "`temporal` is a temporal function, and a value function calls value functions",
            "this is `str`, and the operand of `!` is `bool`",
        ]
    );
    assert_eq!(
        locals(&checked, "calls"),
        [
            "a: i64",
            "text: str",
            "plain: f64",
            "widened: f64",
            "named: f64",
            "wrong: f64",
            "missing: f64",
            "unknown: f64",
            "twice: f64",
            "order: f64",
            "many: f64",
            "role: _",
            "left: _",
        ]
    );
}

/// TYPE-5: "An ordinary constructor supplies complete required data."
/// TYPE-3: "abstract values are not constructible." GEN-2: "A generic
/// struct application supplies all arguments or infers them from
/// constructor fields and expected type." TYPE-7: `.member` "names a
/// member of the enum type its position expects and rejects where none
/// is."
#[test]
fn a_struct_is_made_from_its_fields_and_read_by_them() {
    let checked = module(concat!(
        "struct Quote {\n    bid: f64\n    ask: f64 = 0.0\n    note: str?\n}\n",
        "abstract struct Base { id: i64 }\n",
        "struct Child : Base { extra: f64 }\n",
        "struct Box<T> { value: T }\n",
        "enum Side { buy, sell }\n",
        "const fn build(a: i64, side: Side) -> Quote {\n",
        "    let quote = Quote(bid: 1.5)\n",
        "    let positional = Quote(a, 2.5)\n",
        "    let short = Quote(ask: 1.0)\n",
        "    let extra = Quote(bid: 1.0, mid: 2.0)\n",
        // A parent's fields come first.
        "    let child = Child(1, extra: 2.5)\n",
        "    let base = Base(id: 1)\n",
        "    let boxed = Box(value: a)\n",
        "    let typed = Box<f64>(value: a)\n",
        "    let expected: Box<str> = Box(value: a)\n",
        "    let bid = quote.bid\n",
        "    let id = child.id\n",
        "    let inner = typed.value\n",
        "    let nothing = quote.mid\n",
        "    let buy = Side.buy\n",
        "    let hold = Side.hold\n",
        "    let implied: Side = .sell\n",
        "    let same = side == .buy\n",
        "    let none = .buy\n",
        "    let number: i64 = .buy\n",
        "    return Quote(bid: 1.5, note: \"n\")\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:14:17 local:type/argument.missing EXPR-2",
            "m.hgl:15:33 local:name/argument.unknown EXPR-2",
            "m.hgl:17:16 local:type/struct.abstract TYPE-3",
            "m.hgl:20:41 local:type/type.mismatch EXPR-5",
            "m.hgl:24:25 local:name/member.unknown EXPR-7",
            "m.hgl:26:21 local:name/member.unknown TYPE-7",
            "m.hgl:29:16 local:type/enum.context_type TYPE-7",
            "m.hgl:30:23 local:type/enum.context_type TYPE-7",
        ]
    );
    assert_eq!(
        messages(&checked),
        [
            "`Quote` is given no `bid`, a field with no default",
            "`Quote` has no field `mid`",
            "`Base` is an abstract struct and is not constructed",
            "this is `i64`, and the field `value` is `str`",
            "`Quote` has no field `mid`",
            "`Side` has no member `hold`",
            "`.buy` is a member of the enum its place expects, and nothing here says which",
            "`.buy` is a member of the enum its place expects, and a `i64` is expected here",
        ]
    );
    assert_eq!(
        locals(&checked, "build"),
        [
            "a: i64",
            "side: Side",
            "quote: Quote",
            "positional: Quote",
            "short: Quote",
            "extra: Quote",
            "child: Child",
            "base: Base",
            "boxed: Box<i64>",
            "typed: Box<f64>",
            "expected: Box<str>",
            "bid: f64",
            "id: i64",
            "inner: f64",
            "nothing: _",
            "buy: Side",
            "hold: Side",
            "implied: Side",
            "same: bool",
            "none: _",
            "number: i64",
        ]
    );
}

/// GEN-2: a generic struct "infers them from constructor fields and
/// expected type". Every field given is then of its type with the
/// arguments in place. TYPE-4: a field is optional by its type, the
/// arguments in place.
#[test]
fn a_generic_struct_is_checked_with_its_arguments_in_place() {
    let checked = module(concat!(
        "struct Pair<T> {\n    first: T\n    second: T\n}\n",
        "struct Box<T> { value: T }\n",
        "const fn pairs(a: i64) {\n",
        "    let same = Pair(1, a)\n",
        "    let mixed = Pair(1, \"x\")\n",
        "    let widened = Pair(1.5, a)\n",
        "    let second = mixed.second\n",
        "    let empty: Box<i64?> = Box()\n",
        "    let typed = Box<i64?>()\n",
        "    let needed: Box<i64> = Box()\n",
        // `null` and `.member` take their type from the field, once the
        // other fields have said what `T` is.
        "    let absent = Pair(1, null)\n",
        "    let sides = Pair(Side.buy, .sell)\n",
        "    let nullable: Pair<i64?> = Pair(1, null)\n",
        // A parameter is found inside a field's type too.
        "    let listed = Bag([1, a])\n",
        "    let element: str = listed.values[0]\n",
        "    let both = Both(1.5, [1])\n",
        "    let maybe = Maybe(a)\n",
        "}\n",
        "enum Side { buy, sell }\n",
        "struct Bag<T> { values: list<T> }\n",
        "struct Both<T> {\n    first: T\n    rest: list<T>\n}\n",
        "struct Maybe<T> { value: T? }\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:9:25 local:type/type.mismatch EXPR-5",
            "m.hgl:14:28 local:type/argument.missing EXPR-2",
            "m.hgl:15:26 local:type/type.mismatch TYPE-14",
            "m.hgl:19:24 local:type/type.mismatch EXPR-5",
        ]
    );
    assert_eq!(
        messages(&checked),
        [
            "this is `str`, and the field `second` is `i64`",
            "`Box` is given no `value`, a field with no default",
            "`null` is no value of `i64`: write `i64?`",
            "this is `i64`, and the type written for it is `str`",
        ]
    );
    assert_eq!(
        locals(&checked, "pairs"),
        [
            "a: i64",
            "same: Pair<i64>",
            "mixed: Pair<i64>",
            "widened: Pair<f64>",
            "second: i64",
            "empty: Box<i64?>",
            "typed: Box<i64?>",
            "needed: Box<i64>",
            "absent: Pair<i64>",
            "sides: Pair<Side>",
            "nullable: Pair<i64?>",
            "listed: Bag<i64>",
            "element: str",
            "both: Both<f64>",
            "maybe: Maybe<i64>",
        ]
    );
}

/// UNIT-10: a struct named through an alias is the struct, with its
/// generic arguments written or not.
#[test]
fn a_struct_named_through_an_alias_is_constructed() {
    let checked = check(&[
        (
            "lib.hgl",
            concat!(
                "module lib\n",
                "export struct Box<T> { value: T }\n",
                "export struct Plain { value: i64 }\n",
            ),
        ),
        (
            "app.hgl",
            concat!(
                "module app\n",
                "use lib as l\n",
                "const fn made() {\n",
                "    let typed = l::Box<i64>(value: \"x\")\n",
                "    let plain = l::Plain(value: \"x\")\n",
                "    let short = l::Box<i64>()\n",
                "}\n",
            ),
        ),
    ]);
    assert_eq!(
        checked.summary(),
        [
            "app.hgl:4:36 local:type/type.mismatch EXPR-5",
            "app.hgl:5:33 local:type/type.mismatch EXPR-5",
            "app.hgl:6:20 local:type/argument.missing EXPR-2",
        ]
    );
}

/// ERR-6: "Ordinary/atomic dictionary brackets ... are strict
/// point-in-time reads." A list is indexed by an `i64`, a dictionary by
/// its key.
#[test]
fn an_element_is_read_by_its_index_or_its_key() {
    let checked = module(concat!(
        "const fn elements(items: list<f64>, by_name: dict<str, i64>, a: i64) -> f64 {\n",
        "    let first = items[0]\n",
        "    let at = items[a]\n",
        "    let bad = items[\"x\"]\n",
        "    let count = by_name[\"x\"]\n",
        "    let wrong = by_name[a]\n",
        "    return first\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:5:21 local:type/type.mismatch EXPR-5",
            "m.hgl:7:25 local:type/type.mismatch EXPR-5",
        ]
    );
    assert_eq!(
        messages(&checked),
        [
            "this is `str`, and the index of a list is `i64`",
            "this is `i64`, and the key of the dictionary is `str`",
        ]
    );
    assert_eq!(
        locals(&checked, "elements"),
        [
            "items: list<f64>",
            "by_name: dict<str, i64>",
            "a: i64",
            "first: f64",
            "at: f64",
            "bad: f64",
            "count: i64",
            "wrong: i64",
        ]
    );
}
