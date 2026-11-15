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
        // OPS-3: a `CmpResult`, which no HGL source declares yet.
        "    let outcome = a <=> b\n",
        "    let joined = \"a\" + \"b\"\n",
        "    let grouped = (a + 1) * 2\n",
        // LEX-7: "the literal 2^63 only as the direct operand of a
        // unary minus", where the two are the least `i64`.
        "    let least = -9223372036854775808\n",
        "}\n",
        "const fn minimum() -> str => -9223372036854775808\n",
    ));
    assert_eq!(checked.summary(), ["m.hgl:19:30 type/type.mismatch EXPR-5"]);
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
            "joined: str",
            "grouped: i64",
            "least: i64",
        ]
    );
}

/// OPS-1: "The symbol operators admit the operand types of the first
/// table below ... Every other pairing is a source rejection ... An
/// operation that commutes admits its operands in either order; one
/// that does not admits the order shown." OPS-2: which types are
/// ordered, and that equality is between two values of one type.
#[test]
fn an_operator_takes_the_operands_its_contract_admits() {
    let checked = module(concat!(
        "const fn scalars(text: str, span: duration, at: datetime, day: date, clock: time, n: i64, flag: bool) {\n",
        "    let joined = text + text\n",
        "    let longer = span + span\n",
        "    let later = at + span\n",
        "    let also_later = span + at\n",
        "    let civil = day + clock\n",
        "    let apart = at - at\n",
        "    let days = day - day\n",
        "    let twice = 2 * span\n",
        "    let repeated = text * n\n",
        "    let ratio = span / span\n",
        "    let whole = span // span\n",
        "    let half = span / 2\n",
        "    let back = -span\n",
        "    let before = day < day\n",
        "    let same = text == text\n",
        // Pairings the contracts do not admit.
        "    let wrong = span - at\n",
        "    let mixed = text + n\n",
        "    let sum = day + day\n",
        "    let ordered = flag < n\n",
        "    let equal = n == text\n",
        "    let negated = -text\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:18:22 operator/operator.operand OPS-1",
            "m.hgl:19:22 operator/operator.operand OPS-1",
            "m.hgl:20:19 operator/operator.operand OPS-1",
            "m.hgl:21:24 operator/operator.operand OPS-2",
            "m.hgl:22:19 operator/operator.operand OPS-2",
            "m.hgl:23:19 operator/operator.operand OPS-1",
        ]
    );
    assert_eq!(
        messages(&checked),
        [
            "`-` takes no `duration` and `datetime`",
            "`+` takes no `str` and `i64`",
            "`+` takes no `date` and `date`",
            "`<` takes no `bool` and `i64`",
            "`==` takes no `i64` and `str`",
            "`-` takes no `str`",
        ]
    );
    assert_eq!(
        locals(&checked, "scalars")[7..22],
        [
            "joined: str",
            "longer: duration",
            "later: datetime",
            "also_later: datetime",
            "civil: civil_datetime",
            "apart: duration",
            "days: duration",
            "twice: duration",
            "repeated: str",
            "ratio: f64",
            "whole: i64",
            "half: duration",
            "back: duration",
            "before: bool",
            "same: bool",
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
            "m.hgl:3:16 type/type.mismatch EXPR-5",
            "m.hgl:4:26 type/type.mismatch EXPR-5",
            "m.hgl:5:16 type/type.mismatch EXPR-5",
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

/// EXPR-5: "An integer literal is an `f64` where one is expected ... A
/// value that has a type is not converted by its place." TYPE-14: "A T
/// is a value where a `T?` is expected. `null` has the nullable type its
/// place expects, and rejects where nothing gives it one ... It does not
/// make null an admitted value of unqualified T."
#[test]
fn a_value_is_of_the_type_its_place_expects() {
    let checked = module(concat!(
        "const fn places(a: i64, b: f64) {\n",
        "    let widened: f64 = 1\n",
        "    let narrowed: i64 = b\n",
        "    let text: str = 1\n",
        "    let absent: i64? = null\n",
        "    let present: i64? = a\n",
        "    let widened_too: f64? = -1\n",
        "    let never: i64 = null\n",
        "    let whatever: any = a\n",
        // Nothing says which nullable type this is.
        "    let unknown = null\n",
        // A value that has a type is not converted.
        "    let typed: f64 = a\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:4:25 type/type.mismatch EXPR-5",
            "m.hgl:5:21 type/type.mismatch EXPR-5",
            "m.hgl:9:22 type/type.mismatch TYPE-14",
            "m.hgl:11:19 type/null.context_type TYPE-14",
            "m.hgl:12:22 type/type.mismatch EXPR-5",
        ]
    );
    assert_eq!(
        messages(&checked),
        [
            "this is `f64`, and the type written for it is `i64`",
            "this is `i64`, and the type written for it is `str`",
            "`null` is no value of `i64`: write `i64?`",
            "`null` has the type its place expects, and nothing here gives one: write the type",
            "this is `i64`, and the type written for it is `f64`",
        ]
    );
    // The integer literal is written down as widened where an `f64` is
    // wanted.
    let Stmt::Declare {
        value: Some(value), ..
    } = &body(&checked, "places").statements[0]
    else {
        panic!("the first statement declares `widened`");
    };
    assert!(matches!(value.kind, ExprKind::Widen(_)));
    assert_eq!(value.ty.to_string(), "f64");
}

/// FN-7: "A `const fn` that declares a result returns a value of that
/// type: `return` with no value rejects there, as `return value` does in
/// a function that declares no result." An arrow body returns its value.
#[test]
fn a_return_is_of_the_result_type() {
    let checked = module(concat!(
        "const fn whole(a: i64) -> i64 => a + 1\n",
        "const fn real(a: i64) -> f64 => 1\n",
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
        // A function that declares no result returns none.
        "const fn nothing(a: i64) {\n    return a\n}\n",
        "const fn ends(a: i64) {\n    return\n}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:5:33 type/type.mismatch EXPR-5",
            "m.hgl:7:12 type/type.mismatch EXPR-5",
            "m.hgl:10:42 type/type.mismatch EXPR-5",
            "m.hgl:12:37 type/type.mismatch EXPR-5",
            "m.hgl:14:5 type/type.mismatch FN-7",
            "m.hgl:17:12 type/type.mismatch FN-7",
        ]
    );
    assert_eq!(
        checked.findings[0].message,
        "this is `i64`, and the function's result is `str`"
    );
    assert_eq!(
        checked.findings[5].message,
        "this returns a value, and the function declares no result"
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
        // A type pack passed on to a parent is not put in place here:
        // the parent may be the application expected.
        "abstract struct Wide<...Ts> { width: i64 }\n",
        "struct Narrow<...Ts> : Wide<Ts> { }\n",
        "const fn packed(x: Narrow<i64>) -> Wide<i64> => x\n",
        // A parameter of the function is a type like any other.
        "struct Kept<T> : Boxed<T> { }\n",
        "const fn kept<T>(x: Kept<T>) -> Boxed<T> => x\n",
        "const fn lost<T>(x: Kept<T>) -> Boxed<i64> => x\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:10:37 type/type.mismatch EXPR-5",
            "m.hgl:11:36 type/type.mismatch EXPR-5",
            "m.hgl:13:45 type/type.mismatch EXPR-5",
            "m.hgl:19:47 type/type.mismatch EXPR-5",
        ]
    );
}

/// TYPE-6: a struct that reaches its parents by many lines is still a
/// value of each: a ladder of diamonds beside the parent expected does
/// not hide it.
#[test]
fn a_parent_is_found_beside_a_ladder_of_diamonds() {
    const RUNGS: usize = 24;
    let mut text =
        String::from("abstract struct Base { id: i64 }\nabstract struct Left : Base { }\n");
    // Each rung has two structs that both inherit from the two of the
    // rung below: twice as many lines of parents at every rung.
    text.push_str("abstract struct A0 { }\nabstract struct B0 { }\n");
    for rung in 1..=RUNGS {
        let below = rung - 1;
        text.push_str(&format!(
            "abstract struct A{rung} : A{below}, B{below} {{ }}\nabstract struct B{rung} : A{below}, B{below} {{ }}\n"
        ));
    }
    text.push_str(&format!("struct Child : Left, A{RUNGS} {{ }}\n"));
    text.push_str("const fn up(x: Child) -> Base => x\n");
    let checked = module(&text);
    assert_eq!(checked.summary(), NONE);
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
            "m.hgl:9:12 name/binding.unassigned EXPR-3",
            "m.hgl:18:5 name/binding.unassigned EXPR-3",
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
            "m.hgl:6:5 name/binding.immutable EXPR-3",
            "m.hgl:7:5 name/binding.immutable EXPR-3",
            "m.hgl:9:13 type/type.mismatch EXPR-5",
            "m.hgl:10:14 type/type.mismatch EXPR-5",
            "m.hgl:11:12 type/type.mismatch EXPR-5",
        ]
    );
    assert_eq!(
        messages(&checked),
        [
            "`fixed` is a `let` and cannot be assigned: only a `var` can",
            "`value` is a parameter and cannot be assigned: only a `var` can",
            "this is `f64`, and the type of the `var` is `i64`",
            "this is `f64`, and the type of the `var` is `i64`",
            "this is `i64`, and the type of the `var` is `f64`",
        ]
    );
}

// ---- Tuples and sequences ----

/// EXPR-9: "The use site and expected type constrain element types ...
/// With no expected type, value entries make a list of the one type
/// they have: `[1, 2.5]` is a `list<f64>` ... and `[1, null]` a
/// `list<i64?>`. Entries of two types reject, and so do entries that
/// give no type: `[]`, `[null]`."
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
        // A `null` among values makes their type nullable.
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
            "m.hgl:5:21 type/type.mismatch EXPR-5",
            // EXPR-5: `a` has a type, and is not converted.
            "m.hgl:6:33 type/type.mismatch EXPR-5",
            "m.hgl:8:34 type/type.mismatch EXPR-5",
            "m.hgl:9:17 type/sequence.context_type EXPR-9",
            "m.hgl:11:41 type/type.mismatch EXPR-5",
            "m.hgl:12:43 type/type.mismatch EXPR-5",
            "m.hgl:14:38 type/type.mismatch EXPR-5",
            "m.hgl:15:41 type/type.mismatch EXPR-5",
            "m.hgl:16:36 type/type.mismatch EXPR-5",
            "m.hgl:18:31 type/type.mismatch TYPE-10",
            "m.hgl:21:19 type/sequence.context_type EXPR-9",
            "m.hgl:24:36 type/type.mismatch TYPE-10",
        ]
    );
    assert_eq!(
        checked.findings[9].message,
        "this has 1 entry, and a `list<i64, 2>` has 2"
    );
    assert_eq!(
        checked.findings[3].message,
        "nothing here says what this sequence holds: write its type"
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
            "holes: list<i64?>",
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
/// definition." FN-3: "A runtime body calls only value functions, and so
/// does a value function."
#[test]
fn an_argument_is_given_to_its_parameter() {
    let checked = module(concat!(
        "const fn scale(value: f64, factor: f64 = 2.0, bias: f64? = null, note: str?) -> f64 => value\n",
        "fn temporal(value: f64) -> f64 => value\n",
        "const fn calls(a: i64, text: str) -> f64 {\n",
        "    let plain = scale(1.5)\n",
        "    let widened = scale(1, factor: 2)\n",
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
            "m.hgl:8:23 type/type.mismatch EXPR-5",
            "m.hgl:9:19 type/argument.missing EXPR-2",
            "m.hgl:10:30 name/argument.unknown EXPR-2",
            "m.hgl:11:28 name/argument.duplicate EXPR-2",
            "m.hgl:12:35 type/argument.order EXPR-2",
            "m.hgl:13:42 type/argument.unexpected EXPR-2",
            "m.hgl:14:16 function-kind/call.role FN-3",
            "m.hgl:15:28 type/type.mismatch EXPR-5",
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
        // A parent's fields are given by name, as its own are.
        "    let child = Child(extra: 2.5, id: 1)\n",
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
        "    let same = side == .buy && (.sell) != side\n",
        "    let none = .buy\n",
        "    let number: i64 = .buy\n",
        "    return Quote(bid: 1.5, note: \"n\")\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            // TYPE-5: a constructor "takes no positional argument."
            "m.hgl:13:28 type/argument.unexpected TYPE-5",
            "m.hgl:13:31 type/argument.unexpected TYPE-5",
            "m.hgl:14:17 type/argument.missing EXPR-2",
            "m.hgl:15:33 name/argument.unknown EXPR-2",
            "m.hgl:17:16 type/struct.abstract TYPE-3",
            // EXPR-5: `a` has a type, and is not converted.
            "m.hgl:19:33 type/type.mismatch EXPR-5",
            "m.hgl:20:41 type/type.mismatch EXPR-5",
            "m.hgl:24:25 name/member.unknown EXPR-7",
            "m.hgl:26:21 name/member.unknown TYPE-7",
            "m.hgl:29:16 type/enum.context_type TYPE-7",
            "m.hgl:30:23 type/enum.context_type TYPE-7",
        ]
    );
    assert_eq!(
        messages(&checked),
        [
            "a struct's fields are given by name: `Quote(field: value)`",
            "a struct's fields are given by name: `Quote(field: value)`",
            "`Quote` is given no `bid`, a field with no default",
            "`Quote` has no field `mid`",
            "`Base` is an abstract struct and is not constructed",
            "this is `i64`, and the field `value` is `f64`",
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
        "    let same = Pair(first: 1, second: a)\n",
        "    let mixed = Pair(first: 1, second: \"x\")\n",
        // The fields are read in the order written, not declared.
        "    let widened = Pair(second: 1.5, first: 1)\n",
        "    let second = mixed.second\n",
        "    let empty: Box<i64?> = Box()\n",
        "    let typed = Box<i64?>()\n",
        "    let needed: Box<i64> = Box()\n",
        // `null` and `.member` take their type from the field, once the
        // other fields have said what `T` is.
        "    let absent = Pair(second: null, first: 1)\n",
        "    let sides = Pair(first: Side.buy, second: .sell)\n",
        "    let nullable: Pair<i64?> = Pair(first: 1, second: null)\n",
        // A parameter is found inside a field's type too.
        "    let listed = Bag(values: [1, a])\n",
        "    let element: str = listed.values[0]\n",
        "    let both = Both(first: 1.5, rest: [1])\n",
        "    let maybe = Maybe(value: a)\n",
        "}\n",
        // A parameter of the function is a type like any other: a field
        // given it is of it.
        "const fn kept<U>(value: U) -> Box<U> => Box<U>(value: value)\n",
        "const fn lost<U>() -> Box<U> => Box<U>(value: 1)\n",
        "const fn read<U>(box: Box<U>) -> U => box.value\n",
        "enum Side { buy, sell }\n",
        "struct Bag<T> { values: list<T> }\n",
        "struct Both<T> {\n    first: T\n    rest: list<T>\n}\n",
        "struct Maybe<T> { value: T? }\n",
        // A value parameter is found in a size, as a type is.
        "struct Sized<const N: i64> { items: list<i64, N> }\n",
        "const fn sized(xs: list<i64, 2>) {\n",
        "    let two = Sized(items: xs)\n",
        "    let written = Sized<2>(items: xs)\n",
        "    let wrong = Sized<3>(items: xs)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:9:40 type/type.mismatch EXPR-5",
            "m.hgl:14:28 type/argument.missing EXPR-2",
            "m.hgl:15:31 type/type.mismatch TYPE-14",
            "m.hgl:19:24 type/type.mismatch EXPR-5",
            "m.hgl:24:47 type/type.mismatch EXPR-5",
            "m.hgl:37:33 type/type.mismatch EXPR-5",
        ]
    );
    assert_eq!(
        messages(&checked),
        [
            "this is `str`, and the field `second` is `i64`",
            "`Box` is given no `value`, a field with no default",
            "`null` is no value of `i64`: write `i64?`",
            "this is `i64`, and the type written for it is `str`",
            "this is `i64`, and the field `value` is `U`",
            "this is `list<i64, 2>`, and the field `items` is `list<i64, 3>`",
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
    assert_eq!(
        locals(&checked, "sized"),
        [
            "xs: list<i64, 2>",
            "two: Sized<2>",
            "written: Sized<2>",
            "wrong: Sized<3>",
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
            "app.hgl:4:36 type/type.mismatch EXPR-5",
            "app.hgl:5:33 type/type.mismatch EXPR-5",
            "app.hgl:6:20 type/argument.missing EXPR-2",
        ]
    );
}

/// EXPR-7: "Brackets read an element of a list or a tuple by an `i64`
/// index, and the value a dictionary has for a key." ERR-6:
/// "Ordinary/atomic dictionary brackets ... are strict point-in-time
/// reads."
#[test]
fn an_element_is_read_by_its_index_or_its_key() {
    let checked = module(concat!(
        "const fn elements(items: list<f64>, by_name: dict<str, i64>, a: i64, pair: tuple<i64, str>) -> f64 {\n",
        "    let first = items[0]\n",
        "    let at = items[a]\n",
        "    let bad = items[\"x\"]\n",
        "    let count = by_name[\"x\"]\n",
        "    let wrong = by_name[a]\n",
        // EXPR-7: "A tuple's index is a constant within its length: it
        // selects the element's type."
        "    let head = pair[0]\n",
        "    let tail = pair[1]\n",
        "    let beyond = pair[2]\n",
        "    let moving = pair[a]\n",
        "    return first\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:5:21 type/type.mismatch EXPR-5",
            "m.hgl:7:25 type/type.mismatch EXPR-5",
            "m.hgl:10:23 type/tuple.index EXPR-7",
            "m.hgl:11:23 type/tuple.index EXPR-7",
        ]
    );
    assert_eq!(
        messages(&checked),
        [
            "this is `str`, and the index of a list is `i64`",
            "this is `i64`, and the key of the dictionary is `str`",
            "a tuple's index is a constant, here one of 0 to 1: it selects the element's type",
            "a tuple's index is a constant, here one of 0 to 1: it selects the element's type",
        ]
    );
    assert_eq!(
        locals(&checked, "elements"),
        [
            "items: list<f64>",
            "by_name: dict<str, i64>",
            "a: i64",
            "pair: tuple<i64, str>",
            "first: f64",
            "at: f64",
            "bad: f64",
            "count: i64",
            "wrong: i64",
            "head: i64",
            "tail: str",
            "beyond: _",
            "moving: _",
        ]
    );
}

// ---- Operators called by name ----

/// GEN-5: "Match candidates against the requested output before inputs
/// ... Select exactly one most-specific admitted candidate ... No match
/// rejects; tied most-specific candidates are ambiguous ... A call in a
/// value function or a runtime body selects among the value candidates."
/// GEN-4: "An operator is neither temporal nor value: the candidate a
/// call selects is."
#[test]
fn a_call_of_an_operator_selects_a_value_candidate() {
    let checked = module(concat!(
        "operator doubled<T>(value: T) -> T\n",
        "impl const fn doubled(value: i64) -> i64 => value * 2\n",
        "impl const fn doubled(value: f64) -> f64 => value * 2.0\n",
        "impl fn doubled(value: str) -> str => value\n",
        "operator pick<T>(lhs: T, rhs: T) -> T\n",
        "impl const fn pick(lhs: i64, rhs: i64) -> i64 => lhs\n",
        "impl const fn pick(lhs: i64, rhs: i64?) -> i64 => lhs\n",
        "impl const fn pick(lhs: str, rhs: str = \"x\") -> str => lhs\n",
        "operator only<T>(value: T) -> T\n",
        "impl fn only(value: i64) -> i64 => value\n",
        "const fn calls(a: i64, b: f64, s: str) {\n",
        "    let whole = doubled(a)\n",
        "    let real = doubled(b)\n",
        // An integer literal selects as the `i64` it is.
        "    let literal = doubled(1)\n",
        // The requested output comes first, and the literal is widened.
        "    let widened: f64 = doubled(1)\n",
        // EXPR-2: the operator's public names.
        "    let named = doubled(value: a)\n",
        "    let other = doubled(number: a)\n",
        // The candidate for `str` is temporal.
        "    let text = doubled(s)\n",
        "    let wrong: str = doubled(a)\n",
        "    let tie = pick(a, a)\n",
        // GEN-6: "A candidate may give any of its inputs a default."
        "    let short = pick(s)\n",
        // UNIT-8: a parameter written `T?` needs no default to be
        // omitted.
        "    let one = pick(a)\n",
        "    let missing = pick(b)\n",
        "    let temporal = only(a)\n",
        // TYPE-14: `null` is a value of the `i64?` and of no `i64`.
        "    let absent = pick(a, null)\n",
        "    let nothing = pick(null, a)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:18:25 name/argument.unknown EXPR-2",
            "m.hgl:19:16 local:operator/operator.no_candidate GEN-5",
            "m.hgl:20:22 local:operator/operator.no_candidate GEN-5",
            "m.hgl:21:15 local:operator/operator.ambiguous GEN-5",
            "m.hgl:24:19 local:operator/operator.no_candidate GEN-5",
            "m.hgl:25:20 local:operator/operator.no_candidate GEN-5",
            "m.hgl:27:19 local:operator/operator.no_candidate GEN-5",
        ]
    );
    assert_eq!(
        messages(&checked),
        [
            "`doubled` has no parameter `number`",
            "no value candidate of operator `doubled` takes `str`",
            "no value candidate of operator `doubled` takes `i64` and gives `str`",
            "2 value candidates of operator `pick` take `i64`, `i64` equally",
            "no value candidate of operator `pick` takes `f64`",
            "operator `only` has no value candidate, and a value function selects among those",
            "no value candidate of operator `pick` takes `null`, `i64`",
        ]
    );
    assert_eq!(
        locals(&checked, "calls"),
        [
            "a: i64",
            "b: f64",
            "s: str",
            "whole: i64",
            "real: f64",
            "literal: i64",
            "widened: f64",
            "named: i64",
            "other: _",
            "text: _",
            "wrong: str",
            "tie: _",
            "short: str",
            "one: i64",
            "missing: _",
            "temporal: _",
            "absent: i64",
            "nothing: _",
        ]
    );
}

// ---- A parameter that may be omitted comes last ----

/// UNIT-8: "A parameter that may be omitted follows every parameter that
/// may not." TYPE-5: "Fields are declared in any order."
#[test]
fn a_parameter_that_may_be_omitted_follows_those_that_may_not() {
    let checked = module(concat!(
        "const fn fine(a: i64, b: i64 = 1, c: i64?) -> i64 => a\n",
        "const fn late(a: i64 = 1, b: i64) -> i64 => b\n",
        "fn nullable(a: i64?, b: i64) {}\n",
        "fn packed(a: i64 = 1, rest: ...i64) {}\n",
        "struct Fine {\n    id: i64\n    note: str?\n    size: i64 = 1\n}\n",
        "struct Late {\n    size: i64 = 1\n    id: i64\n}\n",
        "abstract struct Base { id: i64 = 0 }\n",
        "struct Child : Base { extra: f64 }\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:27 type/default.order UNIT-8",
            "m.hgl:4:22 type/default.order UNIT-8",
        ]
    );
    assert_eq!(
        messages(&checked),
        [
            "parameter `b` may not be omitted, and follows `a`, which may",
            "parameter `b` may not be omitted, and follows `a`, which may",
        ]
    );
}
