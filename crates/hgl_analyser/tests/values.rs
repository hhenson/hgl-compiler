//! The bodies of value functions: the type of each expression, what a
//! place expects of it, and the bindings of a body. Each test cites its
//! rule.
//!
//! A finding is written `file:line:column category/code rule`; a code
//! local to this compiler has `local:` before it.

mod common;

use common::{Checked, body, check, locals, module};
use hgl_analyser::hir::{BinaryOperator, Callee, Expr, ExprKind, Form, Stmt, UnaryOperator};
use hgl_analyser::{Argument, ConstantValue, Detail};

/// No findings.
const NONE: [&str; 0] = [];

/// The message of each finding, in order.
fn messages(checked: &Checked) -> Vec<&str> {
    checked
        .findings
        .iter()
        .map(|finding| finding.message.as_str())
        .collect()
}

/// The call an expression is, as `operator(candidate's signature)`:
/// `add_(lhs: i64, rhs: f64) -> f64`. With it, how the call is written.
fn call_of(checked: &Checked, value: &Expr) -> (String, Form) {
    let ExprKind::Call {
        callee:
            Callee::Candidate {
                operator,
                module,
                declaration,
                ..
            },
        form,
        ..
    } = &value.kind
    else {
        panic!("a call of a candidate: {:?}", value.kind);
    };
    let modules = &checked.program.modules;
    let Detail::Callable(signature) = &modules[*module].details[*declaration] else {
        panic!("a candidate has a signature");
    };
    let name = &modules[operator.0].declarations[operator.1].name;
    (format!("{name}{signature}"), *form)
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
        // OPS-3: `<=>` returns core's `CmpResult`.
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
            "outcome: CmpResult",
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
            "m.hgl:18:22 operator/operator.no_candidate OPS-1",
            "m.hgl:19:22 operator/operator.no_candidate OPS-1",
            "m.hgl:20:19 operator/operator.no_candidate OPS-1",
            "m.hgl:21:24 operator/operator.no_candidate OPS-2",
            "m.hgl:22:19 operator/operator.no_candidate OPS-2",
            "m.hgl:23:19 operator/operator.no_candidate OPS-1",
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
        "    let empty: i64 = null\n",
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
            "m.hgl:10:11 operator/operator.no_candidate OPS-1",
            "m.hgl:11:12 type/type.mismatch EXPR-5",
        ]
    );
    assert_eq!(
        messages(&checked),
        [
            "`fixed` is a `let` and cannot be assigned: only a `var` can",
            "`value` is a parameter and cannot be assigned: only a `var` can",
            "this is `f64`, and the type of the `var` is `i64`",
            // `count += 1.5` assigns what `count + 1.5` is, of which the
            // `var`'s type is requested.
            "`+` takes no `i64` and `f64` to give `i64`",
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
        // A block whose value is `null` is a null entry too, and the
        // entries are read in the order written (CTRL-10).
        "    let in_block = [{ null }, 1]\n",
        "    var late: i64\n",
        "    let ordered = [{\n        late = 1\n        null\n    }, late]\n",
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
            "m.hgl:27:19 type/sequence.context_type EXPR-9",
            "m.hgl:30:36 type/type.mismatch TYPE-10",
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
            "in_block: list<i64?>",
            "late: i64",
            "ordered: list<i64?>",
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
        // What the call expects is not known, and so is what the
        // entries of its arguments are expected to be: the `null` and
        // the `[]` inside are not reported.
        "    let nested = missing((null, 1), [[]], [(null, [])])\n",
        "    var total: i64\n",
        "    if (a > 1) {\n        total = 1\n    } else {\n        total = 2\n    }\n",
        "    let after: str = total + prelude\n",
        "    return prelude\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:19 name/name.unknown UNIT-10",
            "m.hgl:4:18 name/name.unknown UNIT-10",
        ]
    );
    assert_eq!(
        locals(&checked, "later"),
        [
            "a: i64",
            "prelude: _",
            "nested: _",
            "total: i64",
            "after: str",
        ]
    );
}

/// ERR-9: where what a place expects is not known, a sequence's entries
/// of two types are not judged: the place may give them one type, as a
/// `list<Shape>` does a `Round` and a `Square`. EXPR-9: with nothing
/// expected, "Entries of two types reject".
#[test]
fn entries_of_two_types_are_judged_only_where_nothing_is_expected() {
    let checked = module(concat!(
        "abstract struct Shape { id: i64 }\n",
        "struct Round : Shape { radius: f64 }\n",
        "struct Square : Shape { side: f64 }\n",
        "const fn shapes(a: Round, b: Square) {\n",
        "    let passed = missing([a, b])\n",
        "    let typed: list<Shape> = [a, b]\n",
        "    let alone = [a, b]\n",
        // A place that is known and holds no sequence reports it, once.
        "    let number: i64 = [1, \"x\"]\n",
        "    let empty: i64 = []\n",
        "    let nested: list<i64> = [[1, \"x\"]]\n",
        // A dictionary's entries are for a later pass.
        "    let named: dict<str, i64> = []\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:6:18 name/name.unknown UNIT-10",
            "m.hgl:8:21 type/type.mismatch EXPR-5",
            "m.hgl:9:23 type/type.mismatch EXPR-5",
            "m.hgl:10:22 type/type.mismatch EXPR-5",
            "m.hgl:11:30 type/type.mismatch EXPR-5",
        ]
    );
    assert_eq!(
        messages(&checked),
        [
            "nothing binds `missing` here",
            "this is `Square`, and an element, as the first is, is `Round`",
            "this is `list<_>`, and the type written for it is `i64`",
            "this is `list<_>`, and the type written for it is `i64`",
            "this is `list<_>`, and an element is `i64`",
        ]
    );
    assert_eq!(
        locals(&checked, "shapes"),
        [
            "a: Round",
            "b: Square",
            "passed: _",
            "typed: list<Shape>",
            "alone: list<Round>",
            "number: i64",
            "empty: i64",
            "nested: list<i64>",
            "named: dict<str, i64>",
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
        // A name nothing binds is reported and not read, and its
        // arguments are.
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
            "m.hgl:15:16 name/name.unknown UNIT-10",
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
            "nothing binds `prelude` here",
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
        // A sequence that gives no type while `T` is not found is
        // judged once it is: its entries are a `T`'s.
        "struct Batch<T> {\n    values: list<T>\n    sample: T\n}\n",
        "const fn batches(a: i64) {\n",
        "    let mixed = Batch(values: [1, \"x\"], sample: 1)\n",
        "    let nested = Batch(values: [[1, \"x\"]], sample: [1])\n",
        "    let fine = Batch(values: [], sample: a)\n",
        "}\n",
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
            "m.hgl:38:35 type/type.mismatch EXPR-5",
            "m.hgl:39:37 type/type.mismatch EXPR-5",
            "m.hgl:46:33 type/type.mismatch EXPR-5",
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
            "this is `str`, and an element is `i64`",
            "this is `str`, and an element is `i64`",
            "this is `list<i64, 2>`, and the field `items` is `list<i64, 3>`",
        ]
    );
    assert_eq!(
        locals(&checked, "batches"),
        [
            "a: i64",
            "mixed: Batch<i64>",
            "nested: Batch<list<i64>>",
            "fine: Batch<i64>",
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
        // A constant need not be a literal: `1 - 1` is 0, `-1 + 2` is 1,
        // and `2 // 2` is 1 (NUM-1).
        "    let folded = pair[1 - 1]\n",
        "    let signed = pair[(-1 + 2) * 1]\n",
        "    let outside = pair[1 + 1]\n",
        "    let divided = pair[2 // 2]\n",
        "    let mixed = pair[a + 1]\n",
        // EXPR-7: the index is an `i64`, which is so requested of the
        // symbol: a `/` gives none, and neither does a `==`.
        "    let real = pair[0 / 1]\n",
        "    let truth = pair[0 == 0]\n",
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
            "m.hgl:14:24 type/tuple.index EXPR-7",
            "m.hgl:16:22 type/tuple.index EXPR-7",
            "m.hgl:17:23 operator/operator.no_candidate OPS-1",
            "m.hgl:18:24 operator/operator.no_candidate OPS-2",
        ]
    );
    assert_eq!(
        messages(&checked),
        [
            "this is `str`, and the index of a list is `i64`",
            "this is `i64`, and the key of the dictionary is `str`",
            "a tuple's index is a constant, here one of 0 to 1: it selects the element's type",
            "a tuple's index is a constant, here one of 0 to 1: it selects the element's type",
            "a tuple's index is a constant, here one of 0 to 1: it selects the element's type",
            "a tuple's index is a constant, here one of 0 to 1: it selects the element's type",
            "`/` takes no `i64` and `i64` to give `i64`",
            "`==` takes no `i64` and `i64` to give `i64`",
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
            "folded: i64",
            "signed: str",
            "outside: _",
            "divided: str",
            "mixed: _",
            "real: _",
            "truth: _",
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
        "const fn calls(a: i64, b: f64, s: str, round: Round, square: Square) {\n",
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
        // EXPR-5, EXPR-9: a sequence or a tuple written out is read as
        // the parameter's type, as in a function's call.
        "    let summed = total([1])\n",
        "    let size = count([1])\n",
        "    let couple = count((1, \"x\"))\n",
        "    let unfit = count([1.5])\n",
        // TYPE-7: `.member` is of the enum the parameter expects.
        "    let bought = sign(.buy)\n",
        "    let held = sign(.hold)\n",
        // A block, and an `if`, whose value is of the enum its place
        // expects is tried as the parameter's too.
        // LEX-5: `.member` is an expression where it is the first
        // statement of its block.
        "    let in_block = sign({ .buy })\n",
        "    let either = sign(if a > 0 { .buy } else { .sell })\n",
        "    let no_member = sign({ .hold })\n",
        // An entry that takes its type from its place is read as the
        // candidate's too: a `null` in a tuple, a `[]` in a list.
        "    let partly = first((null, 1))\n",
        "    let inner = first([[]])\n",
        "    let neither = first((null, null))\n",
        // An integer literal among the entries selects as the `i64` it
        // is, and is widened only where no candidate takes one.
        "    let whole_list = each([1])\n",
        // The same inside a block.
        "    let in_a_block = each({ [1] })\n",
        // An integer literal that is a block's value is widened where
        // the candidate that is wanted takes an `f64`.
        "    let widened_block: f64 = doubled({ 1 })\n",
        "    let whole_block = doubled({ 1 })\n",
        "    let real_list = each([1.5])\n",
        "    let whole_pair = each((1, [1]))\n",
        "    let real_pair = each((1.5, [1]))\n",
        // The candidate's parameter gives entries of two types their
        // one type.
        "    let shapes = area([round, square])\n",
        // The output a candidate's parameter requests selects among the
        // candidates of a call inside the argument.
        "    let made = each([make()])\n",
        "    let tied = make()\n",
        "}\n",
        "operator total<T>(values: T) -> f64\n",
        "impl const fn total(values: list<f64>) -> f64 => 0.0\n",
        "operator count<T>(values: T) -> i64\n",
        "impl const fn count(values: set<i64>) -> i64 => 0\n",
        "impl const fn count(values: tuple<i64, str>) -> i64 => 0\n",
        "enum Side { buy, sell }\n",
        "operator sign<T>(side: T) -> i64\n",
        "impl const fn sign(side: Side) -> i64 => 1\n",
        "operator first<T>(values: T) -> i64\n",
        "impl const fn first(values: tuple<i64?, i64>) -> i64 => 0\n",
        "impl const fn first(values: list<list<i64>>) -> i64 => 0\n",
        "operator each<T>(values: T) -> T\n",
        "impl const fn each(values: list<i64>) -> list<i64> => values\n",
        "impl const fn each(values: list<f64>) -> list<f64> => values\n",
        "impl const fn each(values: tuple<i64, list<i64>>) -> tuple<i64, list<i64>> => values\n",
        "impl const fn each(values: tuple<f64, list<f64>>) -> tuple<f64, list<f64>> => values\n",
        "abstract struct Shape { id: i64 }\n",
        "struct Round : Shape { radius: f64 }\n",
        "struct Square : Shape { side: f64 }\n",
        "operator area<T>(shapes: T) -> f64\n",
        "impl const fn area(shapes: list<Shape>) -> f64 => 0.0\n",
        "operator make<T>() -> T\n",
        "impl const fn make() -> i64 => 1\n",
        "impl const fn make() -> str => \"x\"\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:18:25 name/argument.unknown EXPR-2",
            "m.hgl:19:16 operator/operator.no_candidate GEN-5",
            "m.hgl:20:22 operator/operator.no_candidate GEN-5",
            "m.hgl:21:15 operator/operator.ambiguous GEN-5",
            "m.hgl:24:19 operator/operator.no_candidate GEN-5",
            "m.hgl:25:20 operator/operator.no_candidate GEN-5",
            "m.hgl:27:19 operator/operator.no_candidate GEN-5",
            "m.hgl:31:17 operator/operator.no_candidate GEN-5",
            "m.hgl:33:16 operator/operator.no_candidate GEN-5",
            "m.hgl:36:21 operator/operator.no_candidate GEN-5",
            "m.hgl:39:19 operator/operator.no_candidate GEN-5",
            "m.hgl:49:16 operator/operator.ambiguous GEN-5",
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
            "no value candidate of operator `count` takes `[1.5]`",
            "no value candidate of operator `sign` takes `.hold`",
            "no value candidate of operator `sign` takes `{ .hold }`",
            "no value candidate of operator `first` takes `(null, null)`",
            "2 value candidates of operator `make` take no argument equally",
        ]
    );
    assert_eq!(
        locals(&checked, "calls"),
        [
            "a: i64",
            "b: f64",
            "s: str",
            "round: Round",
            "square: Square",
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
            "summed: f64",
            "size: i64",
            "couple: i64",
            "unfit: _",
            "bought: i64",
            "held: _",
            "in_block: i64",
            "either: i64",
            "no_member: _",
            "partly: i64",
            "inner: i64",
            "neither: _",
            "whole_list: list<i64>",
            "in_a_block: list<i64>",
            "widened_block: f64",
            "whole_block: i64",
            "real_list: list<f64>",
            "whole_pair: tuple<i64, list<i64>>",
            "real_pair: tuple<f64, list<f64>>",
            "shapes: f64",
            "made: list<i64>",
            "tied: _",
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

// ---- Control flow ----

/// CTRL-10: "A block whose last statement is an expression has that
/// expression's value and type; any other block has none ... the
/// condition of an `if` or a `while` is a `bool` ... An `if` whose value
/// is used has the one type its two branches' values have ... With no
/// `else` it is as if its `else` were null ... A branch that returns from
/// the function gives the `if` no value and takes no part in its type."
/// FN-7: "A body that ends in an expression returns that expression ...
/// one that can reach its end with no value rejects."
#[test]
fn a_block_and_an_if_have_the_value_of_their_last_expression() {
    let checked = module(concat!(
        "const fn larger(a: i64, b: i64) -> i64 {\n    if a > b { a } else { b }\n}\n",
        "const fn positive(a: i64) -> i64? {\n    if a > 0 { a }\n}\n",
        "const fn typed(a: i64, c: bool) {\n",
        "    let whole = if c { 1 } else { 2 }\n",
        // EXPR-5: an integer literal is an `f64` beside one.
        "    let real = if c { 1 } else { 2.5 }\n",
        // A null branch makes it `T?`, whichever branch it is.
        "    let maybe = if c { a } else { null }\n",
        "    let first = if c { null } else { a }\n",
        // With no `else` it is a `T?`, which a `T` rejects.
        "    let short = if c { a }\n",
        "    let need: i64 = if c { a }\n",
        "    let mixed = if c { 1 } else { \"x\" }\n",
        "    let number = if a { 1 } else { 2 }\n",
        // A block whose last statement is no expression has no value.
        "    let empty = if c { let x = 1 } else { 2 }\n",
        "    let block = {\n        let y = a\n        y + 1\n    }\n",
        // Where the place says, each branch is checked against it.
        "    let widened: f64 = if c { 1 } else { 2 }\n",
        // A branch that returns takes no part.
        "    let rest = if c { return } else { 2 }\n",
        // With no `else`, a branch that returns leaves the `if` null,
        // of the type its place gives.
        "    let gone: i64? = if c { return }\n",
        "    let untyped = if c { return }\n",
        "    let chain = if c { 1 } else if a > 1 { 2 } else { 3 }\n",
        // The first branch gives its type to the `if` of an `else if`
        // too: to its members, and to a null in it.
        "    let side = if c { Side.buy } else if a > 1 { .sell } else { .buy }\n",
        "    let none = if c { a } else if a > 1 { null } else { 2 }\n",
        "}\n",
        "enum Side { buy, sell }\n",
        "const fn missing(a: i64) -> i64 {\n    if a > 0 { return a }\n}\n",
        // The same body returns null where the result is written `T?`.
        "const fn or_null(a: i64) -> i64? {\n    if a > 0 { return a }\n}\n",
        "const fn early(c: bool) -> i64 {\n    if c { return 1 } else { 2 }\n}\n",
        "const fn both(c: bool) -> i64 {\n    if c { return 1 } else { return 2 }\n}\n",
        // In a function that declares no result the expression is a
        // statement.
        "const fn dropped() => 1\n",
        "const fn unit() { 1 + 1 }\n",
        // CTRL-10 is of an `if` whose value is used: one whose value is
        // dropped has branches of any types.
        "const fn either(c: bool) { if c { 1 } else { \"x\" } }\n",
        "const fn stated(c: bool) {\n    if c { 1 } else { \"x\" }\n    let after = 1\n}\n",
        "const fn wrong(c: bool) -> i64 {\n    if c { 1 } else { \"x\" }\n}\n",
        // CTRL-5: what follows a `return` in a branch is not its value.
        "const fn past_return(c: bool) -> i64 {\n    if c {\n        return 1\n        \"unused\"\n    } else {\n        2\n    }\n}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:14:21 type/nullable.required_presence ERR-12",
            "m.hgl:15:33 type/type.mismatch CTRL-10",
            "m.hgl:16:21 type/type.mismatch CTRL-10",
            "m.hgl:17:17 type/type.mismatch CTRL-10",
            "m.hgl:25:19 type/null.context_type TYPE-14",
            "m.hgl:31:10 type/return.missing FN-7",
            "m.hgl:51:23 type/type.mismatch EXPR-5",
        ]
    );
    assert_eq!(
        messages(&checked),
        [
            "this is `i64?`, and the type written for it is `i64`: it may be null here",
            "this branch is `str`, and the other is `i64`",
            "this is `i64`, and a condition is a `bool`",
            "this has no value: a block's value is its last statement, when that is an expression",
            "this is null when its condition does not hold, and nothing here gives that a type: write the type",
            "`missing` declares a result of `i64`, and can reach its end with no value",
            "this is `str`, and the value of this block is `i64`",
        ]
    );
    assert_eq!(
        locals(&checked, "typed"),
        [
            "a: i64",
            "c: bool",
            "whole: i64",
            "real: f64",
            "maybe: i64?",
            "first: i64?",
            "short: i64?",
            "need: i64",
            "mixed: i64",
            "number: i64",
            "x: i64",
            "empty: _",
            "y: i64",
            "block: i64",
            "widened: f64",
            "rest: i64",
            "gone: i64?",
            "untyped: _",
            "chain: i64",
            "side: Side",
            "none: i64?",
        ]
    );
}

/// CTRL-11: "a `var` declared with no value is assigned after an `if`
/// when every branch that does not return assigns it, so not by an `if`
/// with no `else`; after a `switch` when every case that does not return
/// assigns it and the cases leave no value out, with a `default` or by
/// covering an enum (CTRL-6); and never by a loop's body, which may run
/// no times. A `var` whose type is written `T?` is null where it is
/// declared with no value".
#[test]
fn a_var_is_assigned_by_every_path_that_reaches_its_read() {
    let checked = module(concat!(
        "enum Side { buy, sell }\n",
        "const fn paths(c: bool, side: Side, items: list<i64>) -> i64 {\n",
        "    var a: i64\n",
        "    if c { a = 1 } else { a = 2 }\n",
        "    let got = a\n",
        "    var b: i64\n",
        "    if c { b = 1 }\n",
        "    let unset = b\n",
        // CTRL-5: a branch that returns assigns nothing it needs to.
        "    var d: i64\n",
        "    if c { d = 1 } else { return 0 }\n",
        "    let fine = d\n",
        "    var e: i64\n",
        "    for item in items { e = item }\n",
        "    let maybe = e\n",
        "    var f: i64\n",
        "    switch side {\n        case .buy: f = 1\n        case .sell: f = 2\n    }\n",
        "    let covered = f\n",
        "    var g: i64\n",
        "    switch side {\n        case .buy: g = 1\n    }\n",
        "    let partial = g\n",
        "    var h: i64\n",
        "    switch side {\n        case .buy: h = 1\n        default: h = 2\n    }\n",
        "    let defaulted = h\n",
        "    var k: i64?\n",
        "    while c { k = 1 }\n",
        "    let or_null = k\n",
        // EXPR-6: the right operand of `&&` is not evaluated when the
        // left is false, so what it assigns is not assigned after.
        "    var m: i64\n",
        "    let both = c && {\n        m = 1\n        true\n    }\n",
        "    let skipped = m\n",
        // UNIT-12: a case's local is in scope after the `switch`, and is
        // not assigned on a path that did not declare it.
        "    switch side {\n        case .buy: var n: i64 = 1\n    }\n",
        "    let outside = n\n",
        "    return a\n",
        "}\n",
        // Arguments are read in the order written: a block in one may
        // assign what a later one reads, and not what an earlier one
        // does (CTRL-10).
        "struct Two<T> {\n    first: i64\n    second: T\n}\n",
        "const fn ordered() {\n",
        "    var x: i64\n",
        "    let early = Two(first: x, second: {\n        x = 1\n        2\n    })\n",
        "    var y: i64\n",
        "    let late = Two(second: {\n        y = 1\n        2\n    }, first: y)\n",
        "}\n",
        // A `return` in an operand that may be skipped ends no path.
        "const fn skips(c: bool) -> i64 {\n",
        "    let done = c || {\n        return 1\n    }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:9:17 name/binding.unassigned EXPR-3",
            "m.hgl:15:17 name/binding.unassigned EXPR-3",
            "m.hgl:26:19 name/binding.unassigned EXPR-3",
            "m.hgl:41:19 name/binding.unassigned EXPR-3",
            "m.hgl:45:19 name/binding.unassigned EXPR-3",
            "m.hgl:54:28 name/binding.unassigned EXPR-3",
            "m.hgl:64:10 type/return.missing FN-7",
        ]
    );
    assert_eq!(
        messages(&checked),
        [
            "`b` is read before it is given a value",
            "`e` is read before it is given a value",
            "`g` is read before it is given a value",
            "`m` is read before it is given a value",
            "`n` is read before it is given a value",
            "`x` is read before it is given a value",
            "`skips` declares a result of `i64`, and can reach its end with no value",
        ]
    );
}

/// CTRL-12: "`for item in items` runs its body for each element of a
/// list, in index order, or of a set, and `for key, value in entries`
/// for each key and value of a dictionary. One name over a dictionary,
/// two over a list or a set, and any other type after `in` reject.
/// `while condition { ... }` repeats its body while the condition holds;
/// the form with no condition is a source's (SOURCE-4) and rejects
/// there. A `switch` there selects on a `bool`, `i64`, `str`, or enum
/// value. Each label is a constant of the selector's type (CTRL-6)".
/// CTRL-6: "Duplicate values reject, including differently spelled equal
/// enum members."
#[test]
fn a_value_function_loops_and_switches() {
    let checked = module(concat!(
        "enum Side { buy, sell }\n",
        "const fn loops(items: list<i64>, names: set<str>, entries: dict<str, f64>, pair: tuple<i64, i64>, n: i64, side: Side, text: str, real: f64) -> i64 {\n",
        "    var total = 0\n",
        "    for item in items { total += item }\n",
        "    for name in names { total += 1 }\n",
        "    for key, value in entries { total += 1 }\n",
        "    for key2 in entries { }\n",
        "    for a, b in items { }\n",
        "    for x in pair { }\n",
        "    for y in n { }\n",
        "    while total > 10 { total = total // 2 }\n",
        "    while n { }\n",
        "    while { }\n",
        "    switch n {\n        case 1: total += 1\n        case 2: total += 2\n        case 1: total += 3\n        case \"x\": total += 4\n        default: total = 0\n    }\n",
        "    switch side {\n        case .buy: total += 1\n        case Side.buy: total += 2\n    }\n",
        "    switch text {\n        case \"a\": total += 1\n    }\n",
        "    switch real {\n        case 1.5: total += 1\n    }\n",
        "    return total\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:8:17 type/type.mismatch CTRL-12",
            "m.hgl:9:17 type/type.mismatch CTRL-12",
            "m.hgl:10:14 type/type.mismatch CTRL-12",
            "m.hgl:11:14 type/type.mismatch CTRL-12",
            "m.hgl:13:11 type/type.mismatch CTRL-10",
            "m.hgl:14:5 phase/while.condition CTRL-12",
            "m.hgl:18:14 type/switch.duplicate_label CTRL-6",
            "m.hgl:19:14 type/type.mismatch EXPR-5",
            "m.hgl:24:14 type/switch.duplicate_label CTRL-6",
            "m.hgl:29:12 type/type.mismatch CTRL-12",
        ]
    );
    assert_eq!(
        messages(&checked),
        [
            "this is `dict<str, f64>`, and `for` with one name loops over a list or a set",
            "this is `list<i64>`, and `for` with two names loops over a dictionary",
            "this is `tuple<i64, i64>`, and `for` with one name loops over a list or a set",
            "this is `i64`, and `for` with one name loops over a list or a set",
            "this is `i64`, and a condition is a `bool`",
            "a `while` with no condition is for a source that yields: in a value function it has one",
            "this label has the value of an earlier one",
            "this is `str`, and the selector is `i64`",
            "this label has the value of an earlier one",
            "this is `f64`, and a `switch` selects on a `bool`, `i64`, `str` or enum value",
        ]
    );
    assert_eq!(
        locals(&checked, "loops"),
        [
            "items: list<i64>",
            "names: set<str>",
            "entries: dict<str, f64>",
            "pair: tuple<i64, i64>",
            "n: i64",
            "side: Side",
            "text: str",
            "real: f64",
            "total: i64",
            "item: i64",
            "name: str",
            "key: str",
            "value: f64",
            "key2: _",
            "a: _",
            "b: _",
            "x: _",
            "y: _",
        ]
    );
}

// ---- Calls of generic functions ----

/// GEN-2: "Resolve every needed generic from actual inputs, requested
/// output, or solvable equality." GEN-1: "Repeated parameters require
/// equivalent types". What a generic parameter stands for is found at
/// each call, and the call has the result's type with that in place.
#[test]
fn a_generic_function_is_resolved_at_each_call() {
    let checked = module(concat!(
        "struct Box<T> { value: T }\n",
        "const fn same<T>(left: T, right: T) -> T => left\n",
        "const fn repeat<T>(n: i64) -> list<T> => []\n",
        "const fn first<T>(items: list<T>) -> T? => null\n",
        "const fn pair<A, B>(a: A, b: B) -> tuple<A, B> => (a, b)\n",
        "const fn uses(text: str) {\n",
        // From the arguments.
        "    let whole = same(1, 2)\n",
        "    let texts = same(text, \"b\")\n",
        "    let head = first([1.5, 2.5])\n",
        "    let both = pair(text, 1)\n",
        // From the type the place expects of the result, which the
        // arguments agree with: the `2` is then widened (EXPR-5).
        "    let names: list<str> = repeat(3)\n",
        "    let real: f64 = same(1.5, 2)\n",
        "    let maybe: i64? = same(1, 2)\n",
        // An argument that is itself a generic call, and a field.
        "    let nested = same(same(1, 2), 3)\n",
        "    let boxed = Box(value: first([text]))\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(
        locals(&checked, "uses"),
        [
            "text: str",
            "whole: i64",
            "texts: str",
            "head: f64?",
            "both: tuple<str, i64>",
            "names: list<str>",
            "real: f64",
            "maybe: i64?",
            "nested: i64",
            "boxed: Box<str?>",
        ]
    );
}

/// GEN-1: "Repeated parameters require equivalent types". GEN-12: "A
/// prior binding constrains every later occurrence in declared
/// parameter order." The first parameter whose argument says what a
/// generic is decides it, whatever order the arguments are written in,
/// and the others are checked against that (EXPR-5).
#[test]
fn a_repeated_generic_parameter_is_one_type() {
    let checked = module(concat!(
        "const fn same<T>(left: T, right: T) -> T => left\n",
        "const fn uses(count: i64) {\n",
        "    let a = same(\"a\", 1)\n",
        // Only a literal is widened (EXPR-5): `count` is an `i64`.
        "    let b: f64 = same(1.5, count)\n",
        // `left` is declared first: it decides, and `right` is held to
        // it.
        "    let c = same(right: 1, left: \"a\")\n",
        // The `1` is widened to the `f64` that `left` says `T` is.
        "    let d = same(right: 1, left: 2.0)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:4:23 type/type.mismatch EXPR-5",
            "m.hgl:5:28 type/type.mismatch EXPR-5",
            "m.hgl:6:25 type/type.mismatch EXPR-5",
        ]
    );
    assert_eq!(
        messages(&checked),
        [
            "this is `i64`, and the parameter `right` is `str`",
            "this is `i64`, and the parameter `right` is `f64`",
            "this is `i64`, and the parameter `right` is `str`",
        ]
    );
    assert_eq!(locals(&checked, "uses")[4], "d: f64");
}

/// GEN-2: a generic is resolved "from actual inputs". An argument whose
/// type is not known all the way down, the `list<_>` of `[]`, decides
/// nothing: a later argument that is known does, and every argument is
/// then held to it (GEN-1).
#[test]
fn a_partly_known_argument_does_not_decide_a_generic_parameter() {
    let checked = module(concat!(
        "const fn same<T>(a: T, b: T, c: T) -> T => a\n",
        "const fn uses() {\n",
        "    let fine = same(([], 0), ([1], 0), ([2], 0))\n",
        "    let bad = same(([], 0), ([1], 0), ([\"x\"], 0))\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), ["m.hgl:5:41 type/type.mismatch EXPR-5"]);
    assert_eq!(locals(&checked, "uses")[0], "fine: tuple<list<i64>, i64>");
}

/// EXPR-5, EXPR-9, TYPE-10: a sequence or a tuple written out is read as
/// the type of its place, which says what its entries are and, for a
/// sized list, how many. An argument that says what a generic parameter
/// is is first read with no place; it is read again as its parameter's
/// type once that is known, and not held to what it was without one.
#[test]
fn an_argument_typed_by_its_place_is_read_as_the_resolved_parameter() {
    let checked = module(concat!(
        "struct Pair<T> { both: list<T, 2> }\n",
        "const fn keep<T>(xs: list<T, 2>) -> list<T, 2> => xs\n",
        "const fn uses() {\n",
        "    let a = keep([1, 2])\n",
        "    let b = keep([1.5, 2])\n",
        "    let c = Pair(both: [1, 2])\n",
        "    let d = keep([1, 2, 3])\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), ["m.hgl:8:18 type/type.mismatch TYPE-10"]);
    assert_eq!(
        locals(&checked, "uses")[..3],
        ["a: list<i64, 2>", "b: list<f64, 2>", "c: Pair<i64>"]
    );
}

/// GEN-2: "A call resolves a generic of its callee from what it writes
/// after the name (GEN-8), then from the type its place expects of the
/// result, then from its arguments in declared parameter order ... The
/// first of these that says what a parameter is decides it, and what
/// follows is checked against that: an integer literal is widened to an
/// `f64` so decided (EXPR-5), and another argument of another type
/// rejects."
#[test]
fn the_expected_type_decides_a_generic_before_the_arguments() {
    let checked = module(concat!(
        "const fn same<T>(left: T, right: T) -> T => left\n",
        "const fn uses(count: i64) {\n",
        // The place says `f64`: each literal is widened.
        "    let real: f64 = same(1, 2)\n",
        // `count` is no literal, and is held to the `f64`.
        "    let other: f64 = same(count, 2)\n",
        // What the call writes decides before the place does.
        "    let written: f64 = same<f64>(1, 2)\n",
        "    let wrong: str = same<f64>(1, 2)\n",
        // With nothing expected, the arguments decide.
        "    let whole = same(1, 2)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:5:27 type/type.mismatch EXPR-5",
            "m.hgl:7:22 type/type.mismatch EXPR-5",
        ]
    );
    assert_eq!(
        messages(&checked),
        [
            "this is `i64`, and the parameter `left` is `f64`",
            "this is `f64`, and the type written for it is `str`",
        ]
    );
    let types: Vec<String> = body(&checked, "uses")
        .statements
        .iter()
        .map(|statement| match statement {
            Stmt::Declare {
                value: Some(value), ..
            } => value.ty.to_string(),
            other => panic!("a declaration with a value: {other:?}"),
        })
        .collect();
    assert_eq!(types[0], "f64");
    assert_eq!(types[2], "f64");
    assert_eq!(types[4], "i64");
}

/// GEN-8: "A call may supply its generic arguments as a generic
/// constructor does, `count<i64>(3)`. It may supply the first of them
/// and leave the rest to be resolved under GEN-2". GEN-14: `<` after a
/// name resolved to a generic function opens generic arguments.
#[test]
fn a_call_may_write_its_generic_arguments() {
    let checked = module(concat!(
        "const fn same<T>(left: T, right: T) -> T => left\n",
        "const fn repeat<T>(n: i64) -> list<T> => []\n",
        "const fn first<T>(items: list<T>) -> T? => null\n",
        "const fn pair<A, B>(a: A, b: B) -> tuple<A, B> => (a, b)\n",
        "const fn limit<const N: i64>() -> i64 => N\n",
        "const fn uses() {\n",
        "    let texts = repeat<str>(3)\n",
        // What is written decides: the literals are widened.
        "    let real = same<f64>(1, 2)\n",
        // The first of them; the second is found from the argument.
        "    let both = pair<str>(\"a\", 1)\n",
        "    let top = limit<3>()\n",
        "}\n",
        // A lone name among the arguments: the parser could not tell
        // these from comparisons (GEN-14).
        "const fn outer<U>(items: list<U>) -> U? => first<U>(items)\n",
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(
        locals(&checked, "uses"),
        [
            "texts: list<str>",
            "real: f64",
            "both: tuple<str, i64>",
            "top: i64",
        ]
    );
    // The typed body records what each parameter was resolved to.
    let Stmt::Declare {
        value: Some(value), ..
    } = &body(&checked, "uses").statements[2]
    else {
        panic!("a declaration with a value");
    };
    let ExprKind::Call {
        callee: Callee::Function { generics, .. },
        ..
    } = &value.kind
    else {
        panic!("a call of a function: {:?}", value.kind);
    };
    let resolved: Vec<String> = generics.iter().map(|generic| generic.to_string()).collect();
    assert_eq!(resolved, ["str", "i64"]);
}

/// GEN-8: "A call rejects when it leaves a generic parameter
/// unresolved." Nothing is said where the place is not known, or
/// something else was reported (ERR-9).
#[test]
fn a_call_that_leaves_a_generic_parameter_unresolved_is_rejected() {
    let checked = module(concat!(
        "const fn repeat<T>(n: i64) -> list<T> => []\n",
        "const fn pair<A, B>(a: A) -> tuple<A, B>? => null\n",
        "const fn uses() {\n",
        "    let a = repeat(3)\n",
        "    let b = pair<str>(\"a\")\n",
        "    repeat(3)\n",
        // Reported for itself, and not again as unresolved.
        "    let c = repeat(\"x\")\n",
        // Resolved by what is written, or by the place.
        "    let d = repeat<i64>(3)\n",
        "    let e: list<i64> = repeat(3)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:5:13 type/generic.unresolved GEN-8",
            "m.hgl:6:13 type/generic.unresolved GEN-8",
            "m.hgl:7:5 type/generic.unresolved GEN-8",
            "m.hgl:8:20 type/type.mismatch EXPR-5",
        ]
    );
    assert_eq!(
        messages(&checked)[..2],
        [
            "this call of `repeat` leaves `T` unresolved: no argument and no expected type says \
             what it is",
            "this call of `pair` leaves `B` unresolved: no argument and no expected type says \
             what it is",
        ]
    );
}

/// GEN-8: "A call rejects when it leaves a generic parameter
/// unresolved." A call that is itself an argument is read before its
/// place is known. A parameter its result names may yet be resolved by
/// that place; one its result does not name cannot be, and is reported
/// where the call is first read.
#[test]
fn an_unresolved_parameter_of_a_call_that_is_an_argument_is_rejected() {
    let checked = module(concat!(
        "const fn same<T>(a: T, b: T) -> T => a\n",
        "const fn repeat<T>(n: i64) -> list<T> => []\n",
        "const fn tag<const N: i64>(x: i64) -> i64 => x\n",
        "const fn uses() {\n",
        // `N` is in no argument and not in the result.
        "    let a = same(tag(1), 2)\n",
        "    let b = same(2, tag(1))\n",
        // The place says what the `T` of `repeat` is.
        "    let c = same(repeat(1), [\"x\"])\n",
        "    let d = same(tag<3>(1), 2)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:6:18 type/generic.unresolved GEN-8",
            "m.hgl:7:21 type/generic.unresolved GEN-8",
        ]
    );
    assert_eq!(
        locals(&checked, "uses"),
        ["a: i64", "b: i64", "c: list<str>", "d: i64"]
    );
}

/// GEN-8: "A call rejects when it leaves a generic parameter
/// unresolved." A call read as an argument puts its report off while
/// the place it is given to may yet say what its parameter is. Where
/// the enclosing call is settled and that place stays unknown, nothing
/// will, and each is reported: none is left unresolved in silence.
#[test]
fn an_unresolved_parameter_is_reported_once_nothing_can_resolve_it() {
    let checked = module(concat!(
        "struct Box<T> { value: T }\n",
        "enum Colour { red, blue }\n",
        "const fn same<T>(a: T, b: T) -> T => a\n",
        "const fn repeat<T>(n: i64) -> list<T> => []\n",
        "const fn uses() {\n",
        // Neither argument says what `T` is, and nothing is expected.
        "    let a = same(repeat(1), repeat(2))\n",
        "    let b = Box(value: repeat(1))\n",
        // `null` and `.member` have no type of their own.
        "    let c = same(null, null)\n",
        "    let d = same(.red, .blue)\n",
        // Resolved by the place, at whatever depth.
        "    let e: list<str> = same(repeat(1), repeat(2))\n",
        "    let f = same(same(repeat(1), repeat(2)), [\"x\"])\n",
        "    let g: Colour = same(.red, .blue)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:7:18 type/generic.unresolved GEN-8",
            "m.hgl:7:29 type/generic.unresolved GEN-8",
            "m.hgl:8:24 type/generic.unresolved GEN-8",
            "m.hgl:9:13 type/generic.unresolved GEN-8",
            "m.hgl:10:13 type/generic.unresolved GEN-8",
        ]
    );
    assert_eq!(
        locals(&checked, "uses")[4..],
        ["e: list<str>", "f: list<str>", "g: Colour"]
    );
}

/// GEN-8: a call supplies "the first of them", so no more than the
/// callee has. GEN-14: "Check each argument against the corresponding
/// parameter kind." What is written then says nothing of the
/// parameters, and nothing more is reported of the call (ERR-9).
#[test]
fn a_calls_generic_arguments_are_checked_against_its_callee() {
    let checked = module(concat!(
        "const fn same<T>(left: T, right: T) -> T => left\n",
        "const fn helper() -> i64 => 1\n",
        "const fn sized<T, const N: i64>(items: list<T, N>) -> i64 => N\n",
        "const fn uses() {\n",
        "    let a = same<i64, str>(1, 2)\n",
        "    let b = helper<i64>()\n",
        "    let c = same<3>(1, 2)\n",
        "    let d = sized<i64, str>([1, 2])\n",
        // What is written decides, and the arguments are held to it.
        "    let e = same<str>(1, 2)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:6:13 type/generic.argument_count GEN-8",
            "m.hgl:7:13 type/generic.argument_count GEN-8",
            "m.hgl:8:18 type/generic.argument_kind GEN-14",
            "m.hgl:9:24 type/generic.argument_kind GEN-14",
            "m.hgl:10:23 type/type.mismatch EXPR-5",
            "m.hgl:10:26 type/type.mismatch EXPR-5",
        ]
    );
    assert_eq!(
        messages(&checked)[..4],
        [
            "`same` takes 1 generic argument and is given 2",
            "`helper` takes no generic arguments and is given 1",
            "`same` takes a type for `T`, and this is a value",
            "`sized` takes a value for `N`, and `str` is a type",
        ]
    );
}

/// GEN-8: "with `operator convert<T, V>(value: V) -> T`,
/// `convert<f64>(count)` supplies T". GEN-5: "Match candidates against
/// the requested output before inputs". The generic arguments a call of
/// an operator writes are the operator's, and where they settle its
/// result that is the output requested of the candidates.
#[test]
fn an_operators_generic_arguments_request_its_output() {
    let checked = module(concat!(
        "operator change<T, V>(value: V) -> T\n",
        "impl const fn change(value: i64) -> f64 => 1.0\n",
        "impl const fn change(value: i64) -> str => \"x\"\n",
        "impl const fn change(value: bool) -> f64 => 1.0\n",
        "const fn uses(count: i64, flag: bool) {\n",
        "    let real = change<f64>(count)\n",
        "    let text = change<str>(count)\n",
        "    let truth = change<f64>(flag)\n",
        // What is written wins over what the place expects.
        "    let wrong: str = change<f64>(count)\n",
        "    let none = change<bool>(count)\n",
        "    let many = change<f64, i64, str>(count)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:10:22 type/type.mismatch EXPR-5",
            "m.hgl:11:16 operator/operator.no_candidate GEN-5",
            "m.hgl:12:16 type/generic.argument_count GEN-8",
        ]
    );
    assert_eq!(
        locals(&checked, "uses")[2..5],
        ["real: f64", "text: str", "truth: f64"]
    );
}

// ---- Options a candidate declares ----

/// GEN-6: "An operator declares the parameters every candidate takes. A
/// candidate's shape honors them ... and it may declare further const
/// options of its own, each with a default or without. A call is
/// matched against each candidate's own signature. One that names an
/// option admits only the candidates that declare it. An option the
/// call omits takes its candidate's default, and a candidate whose
/// option has none is admitted only when the call gives it ... A call
/// admits the candidate only where that holds for the option's value at
/// the call, a default included."
#[test]
fn a_call_selects_by_the_options_its_candidates_declare() {
    let checked = module(concat!(
        "enum Zero { ieee, nan, error }\n",
        "operator ratio<L, R, O>(lhs: L, rhs: R) -> O\n",
        "impl const fn ratio(lhs: f64, rhs: f64, zero: Zero = .ieee) -> f64\n",
        "    requires zero in {.ieee, .nan} => lhs\n",
        "impl const fn ratio(lhs: f64, rhs: f64, zero: Zero) -> result<f64, error>\n",
        "    requires zero == .error => ok(lhs)\n",
        "impl const fn ratio(lhs: str, rhs: i64) -> str => lhs\n",
        "const fn uses(a: f64, b: f64, n: i64, text: str) {\n",
        // The default fills `zero`, and the first candidate's
        // condition holds for it.
        "    let plain = ratio(a, b)\n",
        "    let nan = ratio(a, b, zero: .nan)\n",
        // Only the second is for `.error`; it has no default, and is
        // admitted because the call gives the option.
        "    let checked = ratio(a, b, zero: .error)\n",
        // An argument with no name is given to the candidate's
        // parameter in that position.
        "    let placed = ratio(a, b, .error)\n",
        // A candidate that declares no option.
        "    let texts = ratio(text, n)\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(
        locals(&checked, "uses")[4..],
        [
            "plain: f64",
            "nan: f64",
            "checked: result<f64, error>",
            "placed: result<f64, error>",
            "texts: str",
        ]
    );
}

/// GEN-6: a call "that names an option admits only the candidates that
/// declare it". EXPR-2: a name that neither the operator nor any
/// candidate declares is unknown.
#[test]
fn an_option_no_candidate_for_the_operands_declares_is_rejected() {
    let checked = module(concat!(
        "enum Zero { ieee, nan, error }\n",
        "operator ratio<L, R, O>(lhs: L, rhs: R) -> O\n",
        "impl const fn ratio(lhs: f64, rhs: f64, zero: Zero = .ieee) -> f64\n",
        "    requires zero in {.ieee, .nan} => lhs\n",
        "impl const fn ratio(lhs: str, rhs: i64) -> str => lhs\n",
        "const fn uses(a: f64, b: f64, n: i64, text: str) {\n",
        // The candidate for a `str` and an `i64` declares no `zero`.
        "    let a1 = ratio(text, n, zero: .nan)\n",
        // No candidate is for `.error` here.
        "    let a2 = ratio(a, b, zero: .error)\n",
        "    let a3 = ratio(a, b, zerro: .nan)\n",
        "    let a4 = ratio(a, b, zero: .nan, zero: .ieee)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:8:14 operator/operator.no_candidate GEN-5",
            "m.hgl:9:14 operator/operator.no_candidate GEN-5",
            "m.hgl:10:26 name/argument.unknown EXPR-2",
            "m.hgl:11:38 name/argument.duplicate EXPR-2",
        ]
    );
    assert_eq!(
        messages(&checked)[..2],
        [
            "no value candidate of operator `ratio` takes `str`, `i64`, `zero: .nan`",
            "no value candidate of operator `ratio` takes `f64`, `f64`, `zero: .error`",
        ]
    );
}

/// GEN-4: an operator "owns ... generic relationships, and
/// requirements". Where its requirement says what its result may be, a
/// candidate's result is one of those: with the call's generic
/// arguments in place (GEN-8), that tells apart candidates the
/// arguments alone do not.
#[test]
fn an_operators_requirement_says_what_its_result_may_be() {
    let checked = module(concat!(
        "enum Invalid { error, fail }\n",
        "operator change<T, V, O>(value: V) -> O\n",
        "    requires O in {T, result<T, error>}\n",
        "impl const fn change(value: i64) -> f64 => 1.0\n",
        "impl const fn change(value: i64) -> str => \"x\"\n",
        "impl const fn change(value: str, invalid: Invalid = .error) -> result<i64, error>\n",
        "    requires invalid == .error => ok(1)\n",
        "impl const fn change(value: str, invalid: Invalid) -> i64\n",
        "    requires invalid == .fail => 1\n",
        "const fn uses(n: i64, text: str) {\n",
        "    let real = change<f64>(n)\n",
        "    let words = change<str>(n)\n",
        "    let parsed = change<i64>(text)\n",
        "    let forced = change<i64>(text, invalid: .fail)\n",
        "    let none = change<bool>(n)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:16:16 operator/operator.no_candidate GEN-5"]
    );
    assert_eq!(
        messages(&checked),
        [
            "no value candidate of operator `change` takes `i64` and gives `bool` or \
          `result<bool, error>`"
        ]
    );
    assert_eq!(
        locals(&checked, "uses")[2..6],
        [
            "real: f64",
            "words: str",
            "parsed: result<i64, error>",
            "forced: i64",
        ]
    );
}

/// GEN-15: constraints joined by `&&` all hold. Two requirements of an
/// operator's result so joined admit only a result that is in both
/// sets.
#[test]
fn an_operators_joined_requirements_of_its_result_all_hold() {
    let checked = module(concat!(
        "operator both<V, O>(value: V) -> O\n",
        "    requires O in {i64, str} && (O in {str, bool})\n",
        "impl const fn both(value: i64) -> i64 => 1\n",
        "impl const fn both(value: i64) -> str => \"x\"\n",
        "operator neither<V, O>(value: V) -> O\n",
        "    requires O in {i64} && O in {str}\n",
        "impl const fn neither(value: i64) -> i64 => 1\n",
        "impl const fn neither(value: i64) -> str => \"x\"\n",
        "const fn uses(n: i64) {\n",
        "    let one = both(n)\n",
        "    let none = neither(n)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:12:16 operator/operator.no_candidate GEN-5"]
    );
    assert_eq!(locals(&checked, "uses")[1], "one: str");
}

/// EXPR-12: "Resolve configuration that selects an expression's source
/// result type during source checking. A runtime-varying selector
/// cannot change that type." An option a candidate's condition reads is
/// given as no constant: which candidate is for it cannot be told. The
/// catalogue has no row for it, so the call is not judged here.
#[test]
fn an_option_that_is_no_constant_is_not_judged() {
    let checked = module(concat!(
        "enum Zero { ieee, nan, error }\n",
        "operator ratio<L, R, O>(lhs: L, rhs: R) -> O\n",
        "impl const fn ratio(lhs: f64, rhs: f64, zero: Zero = .ieee) -> f64\n",
        "    requires zero in {.ieee, .nan} => lhs\n",
        "impl const fn ratio(lhs: f64, rhs: f64, zero: Zero) -> result<f64, error>\n",
        "    requires zero == .error => ok(lhs)\n",
        "const fn uses(a: f64, b: f64, policy: Zero) {\n",
        "    let untold = ratio(a, b, zero: policy)\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(locals(&checked, "uses")[3], "untold: _");
}

/// GEN-6: "The operator owns the defaults of the parameters it
/// declares." A candidate that takes such a parameter with no default
/// of its own is admitted for a call that leaves it out, and its
/// condition is checked against the operator's default.
#[test]
fn an_operators_default_is_the_options_value_for_a_candidates_condition() {
    let checked = module(concat!(
        "enum Zero { ieee, nan, error }\n",
        "operator ratio<L, R, O>(lhs: L, rhs: R, const zero: Zero = .ieee) -> O\n",
        "impl const fn ratio(lhs: f64, rhs: f64, zero: Zero) -> f64\n",
        "    requires zero in {.ieee, .nan} => lhs\n",
        "impl const fn ratio(lhs: f64, rhs: f64, zero: Zero) -> result<f64, error>\n",
        "    requires zero == .error => ok(lhs)\n",
        "const fn uses(a: f64, b: f64) {\n",
        "    let plain = ratio(a, b)\n",
        "    let checked = ratio(a, b, zero: .error)\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(
        locals(&checked, "uses")[2..],
        ["plain: f64", "checked: result<f64, error>"]
    );
}

/// GEN-6: "A call is matched against each candidate's own signature."
/// Candidates may declare their options in different orders, so which
/// option an argument with no name is given to is each candidate's to
/// say: one given in position for one candidate and named for another
/// is given twice only to a candidate that has both in one place.
#[test]
fn an_option_given_in_position_is_each_candidates_own() {
    let checked = module(concat!(
        "operator pick<V>(value: V) -> i64\n",
        "impl const fn pick(value: str, a: i64 = 0) -> i64 => a\n",
        "impl const fn pick(value: i64, b: i64 = 0, a: i64 = 0) -> i64 => a + b\n",
        "const fn uses() {\n",
        // For the second candidate `2` is `b`, and `a` is given once.
        "    let fine = pick(1, 2, a: 3)\n",
        // For the first, `2` is `a`, which is then given twice: it is
        // not admitted, and nothing else takes a `str`.
        "    let twice = pick(\"x\", 2, a: 3)\n",
        // A parameter of the operator is the same place in every
        // candidate.
        "    let again = pick(1, value: 2)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:7:17 operator/operator.no_candidate GEN-5",
            "m.hgl:8:25 name/argument.duplicate EXPR-2",
        ]
    );
    assert_eq!(locals(&checked, "uses")[0], "fine: i64");
}

/// GEN-6: a candidate compares "the option with constants". They are
/// compared as the values they denote, not as they are written: a
/// string by its contents, a number by its value, an integer given to
/// an `f64` option as that number.
#[test]
fn an_options_value_is_compared_as_a_value() {
    let checked = module(concat!(
        "operator mark<V>(value: V) -> str\n",
        "impl const fn mark(value: i64, mode: str = \"ab\") -> str\n",
        "    requires mode == \"ab\" => \"exact\"\n",
        "impl const fn mark(value: i64, mode: str) -> str\n",
        "    requires mode == \"a b\" => \"spaced\"\n",
        "operator sized<V>(value: V) -> i64\n",
        "impl const fn sized(value: i64, limit: i64 = 1000) -> i64\n",
        "    requires limit == 1_000 => 1\n",
        "impl const fn sized(value: i64, limit: i64) -> i64\n",
        "    requires limit in {-1, 2} => 2\n",
        "operator near<V>(value: V) -> i64\n",
        "impl const fn near(value: f64, within: f64 = 0.0) -> i64\n",
        "    requires within == 1.0 => 1\n",
        // A number under a minus: the least `i64`, whose magnitude is
        // no `i64` (LEX-7), and a duration.
        "operator floor<V>(value: V) -> i64\n",
        "impl const fn floor(value: i64, least: i64 = 0) -> i64\n",
        "    requires least == -9223372036854775808 => 1\n",
        "impl const fn floor(value: f64, offset: duration = 0s) -> i64\n",
        "    requires offset == -1s => 2\n",
        "const fn uses(x: f64) {\n",
        "    let least = floor(1, least: -9223372036854775808)\n",
        "    let offset = floor(x, offset: -1s)\n",
        // Each is one candidate: were the two strings taken for one,
        // the second call would be ambiguous.
        "    let exact = mark(1)\n",
        "    let spaced = mark(1, mode: \"a b\")\n",
        "    let default = sized(1)\n",
        "    let written = sized(1, limit: 1000)\n",
        "    let negative = sized(1, limit: -1)\n",
        "    let widened = near(x, within: 1)\n",
        // No candidate is for these values.
        "    let other = sized(1, limit: 3)\n",
        "    let unset = near(x)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:29:17 operator/operator.no_candidate GEN-5",
            "m.hgl:30:17 operator/operator.no_candidate GEN-5",
        ]
    );
    assert_eq!(
        locals(&checked, "uses")[1..9],
        [
            "least: i64",
            "offset: i64",
            "exact: str",
            "spaced: str",
            "default: i64",
            "written: i64",
            "negative: i64",
            "widened: i64",
        ]
    );
}

// ---- Generic candidates ----

/// GEN-5: "Select exactly one most-specific admitted candidate. Concrete
/// types outrank variables; structurally constrained variables outrank
/// unconstrained ones." GEN-12: "Compare structural specificity
/// component by component". A generic candidate is admitted with its
/// parameters resolved from the call (GEN-2), and the call has its
/// result's type with them in place.
#[test]
fn the_most_specific_admitted_candidate_is_selected() {
    let checked = module(concat!(
        "operator shape<V, O>(value: V) -> O\n",
        "impl const fn shape<T>(value: T) -> str => \"any\"\n",
        "impl const fn shape<T>(value: list<T>) -> T? => null\n",
        "impl const fn shape(value: list<bool>) -> bool => true\n",
        "impl const fn shape(value: i64) -> i64 => value\n",
        "const fn uses(text: str, flags: list<bool>, names: list<str>) {\n",
        // Only the bare variable takes a `str`.
        "    let plain = shape(text)\n",
        // A variable inside a structure outranks a bare one.
        "    let inner = shape(names)\n",
        // A concrete type outranks both.
        "    let exact = shape(flags)\n",
        "    let whole = shape(1)\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(
        locals(&checked, "uses")[3..],
        ["plain: str", "inner: str?", "exact: bool", "whole: i64"]
    );
}

/// GEN-12: "Compare structural specificity component by component".
/// Two candidates whose parameters are both structures with variables
/// in them are compared part for part: a concrete part outranks a
/// variable in the same place, and one more specific in one part and
/// less in another outranks nothing.
#[test]
fn specificity_is_compared_inside_a_structure() {
    let checked = module(concat!(
        "operator pair<V>(value: V) -> i64\n",
        "impl const fn pair<T>(value: tuple<i64, T>) -> i64 => 1\n",
        "impl const fn pair<U, V>(value: tuple<U, V>) -> i64 => 2\n",
        "operator cross<V>(value: V) -> i64\n",
        "impl const fn cross<T>(value: tuple<i64, T>) -> i64 => 1\n",
        "impl const fn cross<T>(value: tuple<T, str>) -> i64 => 2\n",
        "operator deep<V>(value: V) -> i64\n",
        "impl const fn deep<T>(value: list<tuple<T, bool>>) -> i64 => 1\n",
        "impl const fn deep<T>(value: list<T>) -> i64 => 2\n",
        // TYPE-14: a `T` is given where a `T?` is expected, so both of
        // these take an `i64`. The concrete one outranks.
        "operator maybe<V>(value: V) -> i64\n",
        "impl const fn maybe(value: i64) -> i64 => 1\n",
        "impl const fn maybe<T>(value: T?) -> i64 => 2\n",
        "const fn uses(a: tuple<i64, str>, b: tuple<bool, str>, c: list<tuple<i64, bool>>) {\n",
        // The first candidate's first part is concrete.
        "    let first = pair(a)\n",
        "    let second = pair(b)\n",
        "    let nested = deep(c)\n",
        "    let whole = maybe(1)\n",
        "    let text = maybe(\"x\")\n",
        // Each is more specific in one part: a tie.
        "    let tie = cross(a)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:20:15 operator/operator.ambiguous GEN-5"]
    );
    assert_eq!(
        locals(&checked, "uses")[3..8],
        [
            "first: i64",
            "second: i64",
            "nested: i64",
            "whole: i64",
            "text: i64",
        ]
    );
    // Which candidate each call selected: the first declared of `pair`
    // for `a`, the second for `b`, and the first of `deep`.
    let selected: Vec<usize> = body(&checked, "uses")
        .statements
        .iter()
        .take(5)
        .map(|statement| match statement {
            Stmt::Declare {
                value: Some(value), ..
            } => match &value.kind {
                ExprKind::Call {
                    callee: Callee::Candidate { declaration, .. },
                    ..
                } => *declaration,
                other => panic!("a call of a candidate: {other:?}"),
            },
            other => panic!("a declaration with a value: {other:?}"),
        })
        .collect();
    // Declarations of module `m`, in order: `pair` is 0, its
    // candidates 1 and 2; `cross` 3, 4, 5; `deep` 6, 7, 8; `maybe` 9,
    // 10, 11.
    assert_eq!(selected, [1, 2, 7, 10, 11]);
}

/// GEN-3: "`requires` may use type/category membership, equalities".
/// A generic candidate is admitted only where its condition holds for
/// what its parameter stands for at the call. GEN-1: a repeated
/// parameter is one type.
#[test]
fn a_generic_candidates_condition_is_checked_at_the_call() {
    let checked = module(concat!(
        "struct Quote { bid: f64 }\n",
        "operator below<L, R>(lhs: L, rhs: R) -> bool\n",
        "impl const fn below<T>(lhs: T, rhs: T) -> bool\n",
        "    requires T in {i64, f64, str} => true\n",
        "impl const fn below(lhs: i64, rhs: f64) -> bool => true\n",
        "operator alike<L, R>(lhs: L, rhs: R) -> bool\n",
        "impl const fn alike<T>(lhs: T, rhs: T) -> bool requires T == str => true\n",
        "const fn uses(n: i64, x: f64, text: str, q: Quote) {\n",
        "    let a = below(n, n)\n",
        "    let b = below(text, text)\n",
        "    let c = below(n, x)\n",
        "    let d = alike(text, text)\n",
        // A `Quote` is not among the types the condition names.
        "    let e = below(q, q)\n",
        // Two types for the one `T`, and no other candidate takes them.
        "    let f = below(text, n)\n",
        "    let g = alike(n, n)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:14:13 operator/operator.no_candidate GEN-5",
            "m.hgl:15:13 operator/operator.no_candidate GEN-5",
            "m.hgl:16:13 operator/operator.no_candidate GEN-5",
        ]
    );
    assert_eq!(
        locals(&checked, "uses")[4..8],
        ["a: bool", "b: bool", "c: bool", "d: bool"]
    );
}

/// GEN-5: "tied most-specific candidates are ambiguous". Two candidates
/// that are as specific as one another in every parameter, or each more
/// specific in a different one, are a tie.
#[test]
fn equally_specific_generic_candidates_are_ambiguous() {
    let checked = module(concat!(
        "operator join<L, R>(lhs: L, rhs: R) -> i64\n",
        "impl const fn join<T>(lhs: T, rhs: i64) -> i64 => 1\n",
        "impl const fn join<T>(lhs: i64, rhs: T) -> i64 => 2\n",
        "const fn uses(n: i64, text: str) {\n",
        "    let one = join(text, n)\n",
        "    let two = join(n, text)\n",
        "    let tie = join(n, n)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:8:15 operator/operator.ambiguous GEN-5"]
    );
    assert_eq!(locals(&checked, "uses")[2..4], ["one: i64", "two: i64"]);
}

// ---- An option that selects ----

/// GEN-6: "An option that selects among candidates is a generic value
/// parameter, `<const zero: Zero = .ieee>` ... A call admits the
/// candidate only where that holds for the parameter's value at the
/// call, a default included." GEN-8: "A call may give a generic value
/// parameter of its callee by name ... A name binds the callee's generic
/// value parameter of that name, or else its parameter of that name."
#[test]
fn an_option_that_selects_is_a_generic_value_parameter() {
    let checked = module(concat!(
        "enum Zero { ieee, nan, error }\n",
        "operator ratio<L, R, O>(lhs: L, rhs: R) -> O\n",
        "impl const fn ratio<const zero: Zero = .ieee>(lhs: f64, rhs: f64) -> f64\n",
        "    requires zero in {.ieee, .nan} => lhs\n",
        "impl const fn ratio<const zero: Zero>(lhs: f64, rhs: f64) -> result<f64, error>\n",
        "    requires zero == .error => ok(lhs)\n",
        "impl const fn ratio(lhs: str, rhs: i64) -> str => lhs\n",
        "const fn uses(a: f64, b: f64, text: str) {\n",
        // The default: the first candidate's condition holds for it.
        "    let plain = ratio(a, b)\n",
        "    let nan = ratio(a, b, zero: .nan)\n",
        // The second has no default: it is admitted because the call
        // gives the option.
        "    let checked = ratio(a, b, zero: .error)\n",
        // TYPE-7: an enum member written in full is the same constant,
        // in parentheses or not (EXPR-1).
        "    let full = ratio(a, b, zero: Zero.error)\n",
        "    let grouped = ratio(a, b, zero: (Zero.error))\n",
        "    let texts = ratio(text, 2)\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(
        locals(&checked, "uses")[3..],
        [
            "plain: f64",
            "nan: f64",
            "checked: result<f64, error>",
            "full: result<f64, error>",
            "grouped: result<f64, error>",
            "texts: str",
        ]
    );
    // What the option is given is a compile-time constant: it is among
    // the call's generic arguments, and not among its arguments.
    let Stmt::Declare {
        value: Some(value), ..
    } = &body(&checked, "uses").statements[2]
    else {
        panic!("a declaration with a value");
    };
    let ExprKind::Call {
        callee: Callee::Candidate { generics, .. },
        arguments,
        ..
    } = &value.kind
    else {
        panic!("a call of a candidate: {:?}", value.kind);
    };
    assert_eq!(arguments.len(), 2);
    let resolved: Vec<String> = generics.iter().map(|generic| generic.to_string()).collect();
    assert_eq!(resolved, [".error"]);

    // TYPE-7: an enum member is one however it is written. What each
    // call gives `zero`, or the default it takes, comes to the member:
    // the default `.ieee`, `.nan`, `.error`, `Zero.error` and
    // `(Zero.error)`.
    let members: Vec<Option<String>> = body(&checked, "uses").statements[..5]
        .iter()
        .map(|statement| {
            let Stmt::Declare {
                value: Some(value), ..
            } = statement
            else {
                panic!("a declaration with a value");
            };
            let ExprKind::Call {
                callee: Callee::Candidate { generics, .. },
                ..
            } = &value.kind
            else {
                panic!("a call of a candidate: {:?}", value.kind);
            };
            match generics.as_slice() {
                [Argument::Constant(constant)] => match &constant.value {
                    ConstantValue::Member(member) => Some(member.name.clone()),
                    _ => None,
                },
                _ => None,
            }
        })
        .collect();
    let member = |name: &str| Some(name.to_string());
    assert_eq!(
        members,
        [
            member("ieee"),
            member("nan"),
            member("error"),
            member("error"),
            member("error"),
        ]
    );
}

/// GEN-8: "The value a call gives a generic value parameter is a
/// compile-time constant: a literal, an enum member however it is
/// written (TYPE-7), `Zero.nan` or `.nan`, a generic value parameter in
/// scope, or a constant expression of those. Any other rejects." GEN-6:
/// a call "that names an option admits only the candidates that declare
/// it".
#[test]
fn a_selecting_option_is_given_a_compile_time_constant() {
    let checked = module(concat!(
        "enum Zero { ieee, nan, error }\n",
        "operator ratio<L, R, O>(lhs: L, rhs: R) -> O\n",
        "impl const fn ratio<const zero: Zero = .ieee>(lhs: f64, rhs: f64) -> f64\n",
        "    requires zero in {.ieee, .nan} => lhs\n",
        "impl const fn ratio(lhs: str, rhs: i64) -> str => lhs\n",
        "const fn uses<const P: Zero>(a: f64, b: f64, text: str, policy: Zero) {\n",
        // A parameter is known only when the program runs.
        "    let runtime = ratio(a, b, zero: policy)\n",
        // The candidate for a `str` and an `i64` declares no `zero`.
        "    let absent = ratio(text, 2, zero: .nan)\n",
        // An `i64` is no `Zero`.
        "    let mistyped = ratio(a, b, zero: 3)\n",
        // A generic value parameter in scope is a compile-time constant.
        // GEN-7: "A generic body is checked under its stated
        // requirements": `uses` states nothing of `P`, so no candidate is
        // known to admit the call, and it is rejected here.
        "    let passed = ratio(a, b, zero: P)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:8:37 type/generic.argument_constant GEN-8",
            "m.hgl:9:18 operator/operator.no_candidate GEN-5",
            "m.hgl:10:20 operator/operator.no_candidate GEN-5",
            "m.hgl:11:18 operator/operator.no_candidate GEN-5",
        ]
    );
    assert_eq!(locals(&checked, "uses")[7], "passed: _");
}

/// GEN-13: "An explicit `const N: V` declares a value parameter of
/// ordinary type V", and V may name another generic parameter. What a
/// call gives it is held to that type with the candidate's generics
/// resolved.
#[test]
fn a_generic_value_parameters_type_may_be_generic() {
    let checked = module(concat!(
        "operator pick<V, O>(value: V) -> O\n",
        "impl const fn pick<T, const fallback: T>(value: T) -> T => value\n",
        // `T` is in the type of no parameter here: what `fallback` is
        // given says what it is.
        "operator make<V, O>(value: V) -> O\n",
        "impl const fn make<T, const fallback: T>(value: i64) -> T => fallback\n",
        // GEN-12: the first declared that says what `T` is decides it,
        // whatever order the call writes them in.
        "operator both<V>(value: V) -> i64\n",
        "impl const fn both<T, const first: T, const second: T>(value: i64) -> i64 => value\n",
        "const fn orders() {\n",
        "    let declared = both(0, first: 1.5, second: 2)\n",
        "    let written = both(0, second: 2, first: 1.5)\n",
        "}\n",
        "const fn uses() {\n",
        "    let whole = pick(1, fallback: 2)\n",
        "    let text = pick(\"a\", fallback: \"b\")\n",
        // The parameter decides `T` first: an integer literal is then
        // an `f64` where one is expected (EXPR-5).
        "    let real = pick(1.5, fallback: 2)\n",
        "    let made = make(1, fallback: 2)\n",
        "    let word = make(1, fallback: \"b\")\n",
        // A `str` is no `i64`.
        "    let mixed = pick(1, fallback: \"b\")\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:18:17 operator/operator.no_candidate GEN-5"]
    );
    assert_eq!(
        locals(&checked, "orders"),
        ["declared: i64", "written: i64"]
    );
    assert_eq!(
        locals(&checked, "uses")[..5],
        [
            "whole: i64",
            "text: str",
            "real: f64",
            "made: i64",
            "word: str"
        ]
    );
}

/// GEN-13: "An explicit `const N: V` declares a value parameter of
/// ordinary type V", and the default a callable gives it (GEN-2) is a
/// V. A default that is not is reported where it is declared, and a
/// call that takes it is not judged (ERR-9). One whose type names
/// another generic parameter is checked where a call resolves that.
#[test]
fn a_generic_value_parameters_default_is_of_its_type() {
    let checked = module(concat!(
        "enum Zero { ieee, nan }\n",
        "operator pick<V>(value: V) -> i64\n",
        "impl const fn pick<const N: i64 = \"bad\">(value: i64) -> i64 => N\n",
        "impl const fn pick<const mode: Zero = .none>(value: str) -> i64 => 1\n",
        "impl const fn pick<const exact: bool = 1.5>(value: f64) -> i64 => 1\n",
        "operator hold<V, O>(value: V) -> O\n",
        "impl const fn hold<T, const fallback: T = 2>(value: T) -> T => fallback\n",
        "const fn uses() {\n",
        "    let bad = pick(1)\n",
        // Given by name, the default is not what the call takes.
        "    let given = pick(1, N: 3)\n",
        "    let whole = hold(1)\n",
        // EXPR-5: an integer literal is an `f64` where one is expected.
        "    let real = hold(1.5)\n",
        // The default, `2`, is no `str`.
        "    let text = hold(\"a\")\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:4:35 type/type.mismatch GEN-13",
            "m.hgl:5:39 name/member.unknown TYPE-7",
            "m.hgl:6:40 type/type.mismatch GEN-13",
            "m.hgl:14:16 operator/operator.no_candidate GEN-5",
        ]
    );
    assert_eq!(
        checked.findings[0].message,
        "this is a `str`, and the generic value parameter `N` is `i64`"
    );
    assert_eq!(
        locals(&checked, "uses"),
        ["bad: _", "given: i64", "whole: i64", "real: f64", "text: _"]
    );
}

/// GEN-2: "A call resolves a generic of its callee from what it writes
/// ... then from its arguments". A generic value parameter the call
/// gives by name is what it writes: an argument whose type says
/// otherwise is then no argument for the candidate, and is not what
/// decides.
#[test]
fn a_generic_value_given_by_name_is_not_overridden_by_an_argument() {
    let checked = module(concat!(
        "operator count<V>(value: V) -> i64\n",
        "impl const fn count<const N: i64>(value: list<i64, N>) -> i64 => N\n",
        "const fn uses(pair: list<i64, 2>) {\n",
        // From the argument's type, and by name where they agree.
        "    let inferred = count(pair)\n",
        "    let agreed = count(pair, N: 2)\n",
        // A list of two is no list of three.
        "    let differs = count(pair, N: 3)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:7:19 operator/operator.no_candidate GEN-5"]
    );
    assert_eq!(
        locals(&checked, "uses")[1..3],
        ["inferred: i64", "agreed: i64"]
    );
}

/// GEN-6: "A call admits the candidate only where that holds for the
/// parameter's value at the call, a default included." The value is the
/// one resolved: from an argument's type where that says it, and the
/// default only where nothing else does. GEN-8: a constant expression is
/// a compile-time constant, and is compared as what it comes to.
#[test]
fn a_candidates_condition_is_checked_against_the_value_resolved() {
    let checked = module(concat!(
        "operator count<V>(value: V) -> i64\n",
        "impl const fn count<const N: i64 = 3>(value: list<i64, N>) -> i64\n",
        "    requires N == 3 => N\n",
        "operator pick<V>(value: V) -> i64\n",
        "impl const fn pick<const N: i64>(value: i64) -> i64 requires N == 2 => value\n",
        "operator twice<V>(value: V) -> i64\n",
        "impl const fn twice<const N: i64 = 1 + 1>(value: i64) -> i64\n",
        "    requires N in {2, 4} => value\n",
        "const fn uses(pair: list<i64, 2>, triple: list<i64, 3>) {\n",
        "    let three = count(triple)\n",
        // `N` is 2 here, from the list, and not its default 3.
        "    let two = count(pair)\n",
        // `1 + 1` is 2.
        "    let sum = pick(1, N: 1 + 1)\n",
        "    let other = pick(1, N: 1 + 2)\n",
        "    let default = twice(1)\n",
        "    let given = twice(1, N: 2 * 2)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:12:15 operator/operator.no_candidate GEN-5",
            "m.hgl:14:17 operator/operator.no_candidate GEN-5",
        ]
    );
    let locals = locals(&checked, "uses");
    assert_eq!(locals[2], "three: i64");
    assert_eq!(locals[4], "sum: i64");
    assert_eq!(locals[6..], ["default: i64", "given: i64"]);
}

/// GEN-8: "a generic value parameter in scope" is a compile-time
/// constant. One passed on to a callee's generic value parameter is
/// still that parameter there: the result of `keep(value, N: M)` is a
/// list of `M`, and nothing says `M` is 2.
#[test]
fn a_generic_value_parameter_passed_on_is_still_that_parameter() {
    let checked = module(concat!(
        "operator keep<V, O>(value: V) -> O\n",
        "impl const fn keep<const N: i64>(value: list<i64, N>) -> list<i64, N> => value\n",
        "const fn uses<const M: i64>(value: list<i64, M>) {\n",
        "    let kept = keep(value, N: M)\n",
        "    let two: list<i64, 2> = keep(value, N: M)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:6:29 operator/operator.no_candidate GEN-5"]
    );
    assert_eq!(locals(&checked, "uses")[1], "kept: list<i64, M>");
}

/// GEN-2: a generic value parameter's default is written in its
/// candidate's declaration, which may be in another file than the call.
/// TYPE-7: an enum member written in full there, `Zero.ieee`, is the
/// constant `.ieee` is, and the candidate's condition is checked
/// against it.
#[test]
fn a_default_is_read_where_its_candidate_is_declared() {
    let checked = check(&[
        (
            "lib.hgl",
            concat!(
                "module lib\n",
                "export enum Zero { ieee, nan, error }\n",
                "operator ratio<L, R, O>(lhs: L, rhs: R) -> O\n",
                "impl const fn ratio<const zero: Zero = (Zero.ieee)>(lhs: f64, rhs: f64) -> f64\n",
                "    requires zero == .ieee => lhs\n",
                // An option that is a parameter, under a name of its
                // own (GEN-6).
                "impl const fn ratio(lhs: i64, rhs: i64, mode: Zero = Zero.nan) -> i64\n",
                "    requires mode == .nan => lhs\n",
                // TYPE-14: an optional enum's default may be a member.
                "impl const fn ratio<const zero: Zero? = Zero.error>(lhs: str, rhs: str) -> str\n",
                "    requires zero == .error => lhs\n",
            ),
        ),
        (
            "m.hgl",
            concat!(
                "module m\n",
                "use lib::{ratio}\n",
                // Padding, so that the call is not at the default's offset.
                "const fn uses(a: f64, n: i64, text: str) {\n",
                "    let real = ratio(a, a)\n",
                "    let whole = ratio(n, n)\n",
                "    let words = ratio(text, text)\n",
                "}\n",
            ),
        ),
    ]);
    assert_eq!(checked.summary(), NONE);
    assert_eq!(
        locals(&checked, "uses")[3..],
        ["real: f64", "whole: i64", "words: str"]
    );
}

// ---- Null tests ----

/// ERR-12: "An immutable nullable parameter or local may be tested
/// directly against null. Refine that same binding to T on paths
/// proving a non-null T? value, including after a terminating null
/// branch." ERR-3: "Track negation, short-circuit paths, terminating
/// branches, and merges."
#[test]
fn a_null_test_refines_an_immutable_nullable_binding() {
    let checked = module(concat!(
        // The types chapter's accepted example.
        "const fn value_or_zero(value: f64?) -> f64 {\n",
        "    if value == null { return 0.0 }\n",
        "    return value\n",
        "}\n",
        "const fn guarded(value: f64?, other: f64?) -> f64 {\n",
        // In the branch the test holds for.
        "    if value != null { return value + 1.0 }\n",
        // The right operand of `&&` is reached only where the left
        // holds, and the branch only where both do.
        "    if null != other && other > 0.0 { return other }\n",
        // After a branch that returns, neither is null.
        "    if value == null || other == null { return 0.0 }\n",
        "    return value + other\n",
        "}\n",
        "const fn locals(xs: list<f64?>) -> f64 {\n",
        "    var total = 0.0\n",
        // A name a `for` binds is immutable, and so is a `let`.
        "    for item in xs {\n",
        "        if item != null { total += item }\n",
        "    }\n",
        "    let first = xs[0]\n",
        "    if !(first == null) { return first }\n",
        "    return total\n",
        "}\n",
        // A `while`'s body is reached only where its condition holds.
        "const fn looped(value: i64?) -> i64 {\n",
        "    while value != null { return value + 1 }\n",
        "    return 0\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE);
    // A refined read is of the payload's type: `value + other` is the
    // call for two `f64`.
    let Some(Stmt::Return {
        value: Some(sum), ..
    }) = body(&checked, "guarded").statements.last()
    else {
        panic!("a return with a value");
    };
    assert_eq!(call_of(&checked, sum).0, "add_(lhs: f64, rhs: f64) -> f64");
}

/// ERR-12: "Using it where T is required without that proof rejects
/// with `nullable.required_presence`." ERR-3: "A saved bool, alias,
/// fresh expression, or mutable var supplies no retained fact."
#[test]
fn a_nullable_is_no_payload_without_a_null_test() {
    let checked = module(concat!(
        // The errors chapter's deliberate rejection.
        "const fn unchecked(value: f64?) -> f64 => value\n",
        "const fn uses(value: f64?, other: f64?) -> f64 {\n",
        // An operand that may be null.
        "    let sum = value + 1.0\n",
        "    let minus = -value\n",
        // A `var` may be given a null after its test.
        "    var kept: f64? = value\n",
        "    if kept != null { return kept }\n",
        // A saved bool proves nothing where it is read.
        "    let present = value != null\n",
        "    if present { return value }\n",
        // One or the other is not null: `value` may be.
        "    if value != null || other != null { return value }\n",
        // A branch that does not return proves nothing after it.
        "    if value == null { }\n",
        "    return value\n",
        "}\n",
        // A loop's body may run no times, and one case of a `switch` is
        // not the others: what is proved inside is proved of nothing
        // after.
        "const fn looped(xs: list<i64>, value: i64?) -> i64 {\n",
        "    for x in xs {\n",
        "        if value == null { return 0 }\n",
        "    }\n",
        "    return value\n",
        "}\n",
        "const fn switched(kind: i64, value: i64?) -> i64 {\n",
        "    switch kind {\n",
        "        case 1:\n",
        "            if value == null { return 0 }\n",
        "        case 2:\n",
        "            return value\n",
        "    }\n",
        "    return value\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:2:43 type/nullable.required_presence ERR-12",
            "m.hgl:4:15 type/nullable.required_presence ERR-12",
            "m.hgl:5:18 type/nullable.required_presence ERR-12",
            "m.hgl:7:30 type/nullable.required_presence ERR-12",
            "m.hgl:9:25 type/nullable.required_presence ERR-12",
            "m.hgl:10:48 type/nullable.required_presence ERR-12",
            "m.hgl:12:12 type/nullable.required_presence ERR-12",
            "m.hgl:18:12 type/nullable.required_presence ERR-12",
            "m.hgl:25:20 type/nullable.required_presence ERR-12",
            "m.hgl:27:12 type/nullable.required_presence ERR-12",
        ]
    );
    assert_eq!(
        messages(&checked)[0],
        "this is `f64?`, and the function's result is `f64`: it may be null here"
    );
    assert_eq!(
        messages(&checked)[1],
        "this is `f64?`, and `+` takes no `f64?` and `f64`: it may be null here"
    );
}

// ---- A function's generic value parameters ----

/// The generic arguments a call of a function resolved, as written:
/// `["5"]`.
fn generics_of(value: &Expr) -> Vec<String> {
    let ExprKind::Call {
        callee: Callee::Function { generics, .. },
        ..
    } = &value.kind
    else {
        panic!("a call of a function: {:?}", value.kind);
    };
    generics.iter().map(|generic| generic.to_string()).collect()
}

/// GEN-8: "A call may give a generic value parameter of its callee by
/// name, among its named arguments ... A name binds the callee's
/// generic value parameter of that name, or else its parameter of that
/// name ... A generic value parameter the call does not give takes its
/// default, when it has one." GEN-2: "A generic value parameter of a
/// function or a candidate may have a default".
#[test]
fn a_functions_generic_value_parameter_is_given_by_name_or_defaulted() {
    let checked = module(concat!(
        "const fn scale<const factor: i64 = 3>(value: i64) -> i64 => value * factor\n",
        "const fn pick<T, const fallback: T>(value: i64) -> T => fallback\n",
        "const fn uses<const M: i64>(n: i64) {\n",
        "    let plain = scale(n)\n",
        "    let wide = scale(n, factor: 5)\n",
        "    let written = scale<7>(n)\n",
        // A constant expression, and a generic value parameter in scope.
        "    let sum = scale(n, factor: 2 + 2)\n",
        "    let passed = scale(n, factor: M)\n",
        // What `fallback` is given says what `T` is.
        "    let text = pick(1, fallback: \"none\")\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE);
    let statements = &body(&checked, "uses").statements;
    let resolved: Vec<Vec<String>> = statements
        .iter()
        .map(|statement| match statement {
            Stmt::Declare {
                value: Some(value), ..
            } => generics_of(value),
            other => panic!("a declaration with a value: {other:?}"),
        })
        .collect();
    assert_eq!(
        resolved,
        [
            vec!["3"],
            vec!["5"],
            vec!["7"],
            vec!["2 + 2"],
            vec!["M"],
            vec!["str", "\"none\""],
        ]
    );
    assert_eq!(locals(&checked, "uses")[6], "text: str");
    // What gives a generic value parameter is no argument of the call.
    let Stmt::Declare {
        value: Some(wide), ..
    } = &statements[1]
    else {
        panic!("a declaration with a value");
    };
    let ExprKind::Call { arguments, .. } = &wide.kind else {
        panic!("a call");
    };
    assert_eq!(arguments.len(), 1);
}

/// GEN-8: "The value a call gives a generic value parameter is a
/// compile-time constant ... Any other rejects." GEN-13: it is of the
/// parameter's ordinary type, and so is a default the call takes.
/// EXPR-2: a parameter is given once.
#[test]
fn a_functions_generic_value_parameter_is_given_a_constant_of_its_type() {
    let checked = module(concat!(
        "const fn scale<const factor: i64 = 3>(value: i64) -> i64 => value * factor\n",
        "const fn hold<T, const fallback: T = 2>(value: T) -> T => fallback\n",
        "const fn sized<const N: i64 = 2>(items: list<i64, N>) -> i64 => N\n",
        "const fn uses(n: i64, count: i64) {\n",
        // A parameter is known only when the program runs.
        "    let runtime = scale(n, factor: count)\n",
        "    let mistyped = scale(n, factor: \"wide\")\n",
        "    let twice = scale<4>(n, factor: 5)\n",
        "    let again = scale(n, factor: 4, factor: 5)\n",
        // No generic value parameter and no parameter has the name.
        "    let unknown = scale(n, length: 5)\n",
        // The default, `2`, is an `f64` beside one and no `str`.
        "    let real = hold(1.5)\n",
        "    let text = hold(\"a\")\n",
        // What `fallback` is given says `T` is an `i64`, before the
        // argument is held to it: a `null` is no `i64`.
        "    let absent = hold(null, fallback: 1)\n",
        // The default says what `N` is before the entries are counted.
        "    let pair = sized([1, 2])\n",
        "    let triple = sized([1, 2, 3])\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:6:36 type/generic.argument_constant GEN-8",
            "m.hgl:7:37 type/type.mismatch EXPR-5",
            "m.hgl:8:29 name/argument.duplicate EXPR-2",
            "m.hgl:9:37 name/argument.duplicate EXPR-2",
            "m.hgl:10:28 name/argument.unknown EXPR-2",
            "m.hgl:12:16 type/type.mismatch GEN-13",
            "m.hgl:13:23 type/type.mismatch TYPE-14",
            "m.hgl:15:24 type/type.mismatch TYPE-10",
        ]
    );
    assert_eq!(
        messages(&checked)[5],
        "this leaves `fallback` to its default, an `i64`, and `fallback` is `str` here"
    );
    assert_eq!(locals(&checked, "uses")[7], "real: f64");
}

// ---- A default on any generic value parameter ----

/// GEN-2: "A generic value parameter may have a default, a function's,
/// a candidate's, a struct's, or an operator's, which it takes where a
/// call or an application neither gives it nor resolves it (GEN-8). A
/// type parameter and a pack have none." "Where an application supplies
/// none, a generic value parameter that nothing says anything of takes
/// its default: for `struct Window<const size: i64 = 3>`,
/// `value: Window` is a `Window<3>`." GEN-13: a default is of its
/// parameter's type.
#[test]
fn a_struct_named_bare_takes_the_defaults_of_its_generic_values() {
    let checked = module(concat!(
        "struct Window<const size: i64 = 3> { values: list<f64, size> }\n",
        "struct Grid<const rows: i64 = 2, const cols: i64 = 2> {\n",
        "    cells: list<list<f64, cols>, rows>\n",
        "}\n",
        "struct Box<T> { value: T }\n",
        // A type parameter has no default: the struct is not named bare.
        "struct Run<T, const size: i64 = 3> { values: list<T, size> }\n",
        "struct Bad<const size: i64 = \"three\"> { values: list<f64, size> }\n",
        // A default whose type names a type parameter is of it for one
        // construction and not another.
        "struct Hold<T, const fallback: T = 2> { value: T }\n",
        "const fn uses(w: Window, five: Window<5>, g: Grid, xs: list<f64, 3>) {\n",
        "    let values = w.values\n",
        "    let wide = five.values\n",
        "    let cells = g.cells\n",
        // A construction that nothing else says the size of.
        "    let made = Window(values: xs)\n",
        "    let held = Hold(value: 1.5)\n",
        "    let text = Hold(value: \"x\")\n",
        "}\n",
        "const fn rejected(b: Box, r: Run) {}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:8:30 type/type.mismatch GEN-13",
            "m.hgl:16:16 type/type.mismatch GEN-13",
            "m.hgl:18:22 type/generic.argument_count GEN-2",
            "m.hgl:18:30 type/generic.argument_count GEN-2",
        ]
    );
    assert_eq!(
        locals(&checked, "uses"),
        [
            "w: Window<3>",
            "five: Window<5>",
            "g: Grid<2, 2>",
            "xs: list<f64, 3>",
            "values: list<f64, 3>",
            "wide: list<f64, 5>",
            "cells: list<list<f64, 2>, 2>",
            "made: Window<3>",
            "held: Hold<f64, 2>",
            "text: Hold<str, 2>",
        ]
    );
}

/// GEN-8: "An operator's takes part in the output requested, in GEN-2's
/// order: what the call writes, then what its place expects, then its
/// arguments, and the default where none of them says."
#[test]
fn an_operators_default_is_part_of_the_output_requested() {
    let checked = module(concat!(
        "operator window<T, const size: i64 = 3>(value: T) -> list<T, size>\n",
        "impl const fn window(value: f64) -> list<f64, 3> => [value, value, value]\n",
        "impl const fn window(value: f64) -> list<f64, 5> =>\n",
        "    [value, value, value, value, value]\n",
        "operator copy<const size: i64 = 3>(value: list<f64, size>) -> list<f64, size>\n",
        "impl const fn copy<const N: i64>(value: list<f64, N>) -> list<f64, N> => value\n",
        "const fn uses(x: f64, five: list<f64, 5>) {\n",
        // The default: a list of three.
        "    let three = window<f64>(x)\n",
        "    let also = window(x)\n",
        // What the call writes, and what its place expects, come first.
        "    let written = window<f64, 5>(x)\n",
        "    let placed: list<f64, 5> = window<f64>(x)\n",
        // And so does an argument.
        "    let copied = copy(five)\n",
        // What the call writes decides, and the place expects another.
        "    let wrong: list<f64, 5> = window<f64, 3>(x)\n",
        "}\n",
        // GEN-13: a default the call takes is of its parameter's type as
        // the call resolves it. GEN-2: an abstract struct widens what an
        // earlier argument decided, for an operator's generics too.
        "operator choose<T, const fallback: T = 2>(value: T) -> T\n",
        "impl const fn choose(value: i64) -> i64 => value\n",
        "impl const fn choose(value: str) -> str => value\n",
        "abstract struct Shape { id: i64 }\n",
        "struct Round : Shape { radius: f64 }\n",
        "operator same<T, const mode: i64 = 0>(left: T, right: T) -> T\n",
        "impl const fn same(left: Shape, right: Shape) -> Shape => left\n",
        // A value the call does not say leaves the rest of the output
        // requested as it is: three rows, of any width.
        "operator grid<const cols: i64, const rows: i64 = 3>() -> list<list<f64, cols>, rows>\n",
        "impl const fn grid() -> list<list<f64, 2>, 3> => [[1.0, 2.0], [1.0, 2.0], [1.0, 2.0]]\n",
        "impl const fn grid() -> list<list<f64, 2>, 5> =>\n",
        "    [[1.0, 2.0], [1.0, 2.0], [1.0, 2.0], [1.0, 2.0], [1.0, 2.0]]\n",
        // A value the output requested does not say is still the
        // candidate's to find from its argument.
        "operator wrap<V, const width: i64, const rows: i64 = 3>(value: V) -> list<list<f64, width>, rows>\n",
        "impl const fn wrap<const N: i64>(value: list<f64, N>) -> list<list<f64, N>, 3>\n",
        "    requires N == 2 => [value, value, value]\n",
        // TYPE-14: a result is a value of the optional type requested.
        "operator make<T, const n: i64 = 3>() -> list<T, n>?\n",
        "impl const fn make() -> list<f64, 3> => [1.0, 2.0, 3.0]\n",
        "const fn resolved(shape: Shape, round: Round, pair: list<f64, 2>) {\n",
        "    let wrapped = wrap(pair)\n",
        "    let made = make()\n",
        "    let rows = grid()\n",
        "    let whole = choose(1)\n",
        "    let text = choose(\"x\")\n",
        "    let child_first = same(round, shape)\n",
        "    let parent_first = same(shape, round)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:14:31 type/type.mismatch EXPR-5",
            "m.hgl:37:16 type/type.mismatch GEN-13",
        ]
    );
    assert_eq!(
        locals(&checked, "resolved")[3..],
        [
            "wrapped: list<list<f64, 2>, 3>",
            "made: list<f64, 3>",
            "rows: list<list<f64, 2>, 3>",
            "whole: i64",
            "text: _",
            "child_first: Shape",
            "parent_first: Shape",
        ]
    );
    assert_eq!(
        locals(&checked, "uses")[2..7],
        [
            "three: list<f64, 3>",
            "also: list<f64, 3>",
            "written: list<f64, 5>",
            "placed: list<f64, 5>",
            "copied: list<f64, 5>",
        ]
    );
}

// ---- Symbols ----

/// The generics chapter: "Syntax binds these core identities
/// independently of local same-named functions": `+` is `add_` and a
/// unary `-` is `neg_`. A symbol is recorded as the call of the
/// candidate selected for its operands (GEN-5), marked as written with
/// the symbol. `name += value` assigns what `name + value` is.
#[test]
fn a_symbol_is_the_call_of_the_operator_it_stands_for() {
    let checked = module(concat!(
        // A function of the module named as core's operator is, were
        // one admitted, is not what the symbol stands for.
        "const fn uses(a: i64, b: f64) {\n",
        "    let sum = a + b\n",
        "    let minus = -a\n",
        "    let same = a == 1\n",
        "    var total = a\n",
        "    total += 2\n",
        "    let both = a < 1 && b > 2.0\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE);
    let statements = &body(&checked, "uses").statements;
    let value = |index: usize| match &statements[index] {
        Stmt::Declare {
            value: Some(value), ..
        }
        | Stmt::Assign { value, .. } => value,
        other => panic!("a statement with a value: {other:?}"),
    };
    assert_eq!(
        call_of(&checked, value(0)),
        (
            "add_(lhs: i64, rhs: f64) -> f64".to_string(),
            Form::Binary(BinaryOperator::Add)
        )
    );
    assert_eq!(
        call_of(&checked, value(1)),
        (
            "neg_(value: i64) -> i64".to_string(),
            Form::Unary(UnaryOperator::Negate)
        )
    );
    assert_eq!(
        call_of(&checked, value(2)),
        (
            "eq_(lhs: i64, rhs: i64, abs: f64 = ..., rel: f64 = ...) -> bool".to_string(),
            Form::Binary(BinaryOperator::Equal)
        )
    );
    // `total += 2`: the local's value is the left operand.
    assert_eq!(
        call_of(&checked, value(4)),
        (
            "add_(lhs: i64, rhs: i64) -> i64".to_string(),
            Form::Binary(BinaryOperator::Add)
        )
    );
    let ExprKind::Call { arguments, .. } = &value(4).kind else {
        panic!("a call");
    };
    let places: Vec<Option<usize>> = arguments.iter().map(|given| given.parameter).collect();
    assert_eq!(places, [Some(0), Some(1)]);
    // EXPR-6: `&&` is read as it is written; its operands are calls.
    let (operator, left, right) = value(5).kind.binary().expect("a symbol with two operands");
    assert_eq!(operator, BinaryOperator::And);
    assert!(matches!(value(5).kind, ExprKind::Binary { .. }));
    assert_eq!(
        call_of(&checked, left).1,
        Form::Binary(BinaryOperator::Less)
    );
    assert_eq!(
        call_of(&checked, right).1,
        Form::Binary(BinaryOperator::Greater)
    );
}

/// GEN-4: "an `impl const fn` a value candidate" of the operator it
/// names. A symbol selects among every candidate of its operator, so a
/// module that declares one for its own type gives the symbol that
/// type's meaning. GEN-5: "No match rejects".
#[test]
fn a_symbol_selects_a_candidate_a_module_declares() {
    let checked = module(concat!(
        "struct Money { amount: i64 }\n",
        "impl const fn add_(lhs: Money, rhs: Money) -> Money =>\n",
        "    Money(amount: lhs.amount + rhs.amount)\n",
        "impl const fn neg_(value: Money) -> Money => Money(amount: -value.amount)\n",
        "const fn uses(a: Money, b: Money) {\n",
        "    let sum = a + b\n",
        "    let owed = -a\n",
        "    var total = a\n",
        "    total += b\n",
        // No candidate of `sub_` takes two `Money`.
        "    let less = a - b\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:11:18 operator/operator.no_candidate OPS-1"]
    );
    assert_eq!(messages(&checked), ["`-` takes no `Money` and `Money`"]);
    assert_eq!(
        locals(&checked, "uses")[2..5],
        ["sum: Money", "owed: Money", "total: Money"]
    );
}

/// EXPR-7: "A tuple's index is a constant within its length". EXPR-5: an
/// integer literal, under a minus or not, is an `f64` where one is
/// expected. Both read `1 - 1` and `-1` as core's own arithmetic. A
/// symbol that is the call of a module's own candidate, or that has
/// configuration, computes what this pass does not know: it is no
/// constant index and no integer literal.
#[test]
fn only_cores_own_arithmetic_is_read_as_a_constant() {
    let checked = module(concat!(
        "impl const fn add_(lhs: i64, rhs: i64, offset: i64) -> i64 => lhs + rhs + offset\n",
        "impl const fn neg_(value: i64, twice: bool) -> i64 => -value\n",
        "const fn uses(pair: tuple<i64, str>, n: i64) {\n",
        "    let head = pair[1 - 1]\n",
        "    let shifted = pair[0 +{offset: n} 0]\n",
        "    let real: f64 = -1\n",
        "    let flipped: f64 = -{twice: true} 1\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:6:24 type/tuple.index EXPR-7",
            "m.hgl:8:24 operator/operator.no_candidate OPS-1",
        ]
    );
    assert_eq!(locals(&checked, "uses")[2], "head: i64");
}

/// EXPR-11: "`<=>` is full comparison ... It returns core's `CmpResult`,
/// an ordinary enum, rather than a bool; `==`, `!=`, `<`, `<=`, `>`, and
/// `>=` return a bool." NUM-6: "An outcome has no implicit bool or
/// numeric conversion."
#[test]
fn a_full_comparison_is_an_outcome_and_no_bool() {
    let checked = module(concat!(
        "const fn uses(a: i64, b: f64, text: str) {\n",
        "    let outcome = a <=> b\n",
        "    let words = text <=> \"b\"\n",
        "    let less: bool = a < b\n",
        // An outcome is an enum's member, and is compared as one.
        "    let is_less = outcome == .less\n",
        "    let flag: bool = a <=> b\n",
        "    let number: i64 = a <=> b\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:7:24 operator/operator.no_candidate OPS-2",
            "m.hgl:8:25 operator/operator.no_candidate OPS-2",
        ]
    );
    // GEN-5: the type the place expects is requested of the candidates,
    // and none of `compare_` gives a bool.
    assert_eq!(
        messages(&checked)[0],
        "`<=>` takes no `i64` and `f64` to give `bool`"
    );
    assert_eq!(
        locals(&checked, "uses")[3..7],
        [
            "outcome: CmpResult",
            "words: CmpResult",
            "less: bool",
            "is_less: bool"
        ]
    );
}

/// OPS-2: "`==` and `!=` compare two values of one type." Core's
/// candidate is `eq_<T>(lhs: T, rhs: T)`. GEN-2: the first argument
/// that says what `T` is decides it, and "One later argument changes
/// what an earlier argument decided: an abstract struct that the type
/// decided inherits from widens the parameter to that struct, so the
/// order of two such arguments does not matter. What the call writes,
/// or its place expects, is not widened".
#[test]
fn an_abstract_parent_widens_what_an_earlier_argument_decided() {
    let checked = module(concat!(
        "abstract struct Shape { id: i64 }\n",
        "struct Round : Shape { radius: f64 }\n",
        "struct Square : Shape { side: f64 }\n",
        "const fn same<T>(left: T, right: T) -> T => left\n",
        "const fn uses(shape: Shape, round: Round, square: Square) {\n",
        "    let parent_first = shape == round\n",
        "    let child_first = round != shape\n",
        "    let either = same(round, shape)\n",
        // Neither inherits from the other.
        "    let siblings = round == square\n",
        // What the call writes, and what its place expects, stand.
        "    let written = same<Round>(round, shape)\n",
        "    let placed: Round = same(round, shape)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:10:26 operator/operator.no_candidate OPS-2",
            "m.hgl:11:38 type/type.mismatch EXPR-5",
            "m.hgl:12:37 type/type.mismatch EXPR-5",
        ]
    );
    let statements = &body(&checked, "uses").statements;
    let value = |index: usize| match &statements[index] {
        Stmt::Declare {
            value: Some(value), ..
        } => value,
        other => panic!("a declaration with a value: {other:?}"),
    };
    // Each is the generic candidate, with `T` the `Shape`.
    for (index, name) in [(0, "eq_"), (1, "ne_")] {
        assert_eq!(
            call_of(&checked, value(index)).0,
            format!("{name}<T>(lhs: T, rhs: T) -> bool")
        );
        let ExprKind::Call {
            callee: Callee::Candidate { generics, .. },
            ..
        } = &value(index).kind
        else {
            panic!("a call of a candidate");
        };
        let resolved: Vec<String> = generics.iter().map(|generic| generic.to_string()).collect();
        assert_eq!(resolved, ["Shape"]);
    }
    assert_eq!(
        locals(&checked, "uses")[3..7],
        [
            "parent_first: bool",
            "child_first: bool",
            "either: Shape",
            "siblings: bool"
        ]
    );
}

/// The generics chapter: "A symbol is the call of its identity: its
/// operands are read as that call's arguments are, the type its place
/// expects is requested of the candidates (GEN-5) ... `n + 1` is
/// `add_(n, 1)` wherever it stands." So a symbol and the call by name
/// give one result in one place.
#[test]
fn a_symbol_is_its_call_in_its_place_and_in_its_operands() {
    let checked = module(concat!(
        "const fn uses(n: i64, xs: list<f64>) {\n",
        // The place requests an `f64`: the candidate for an `i64` and
        // an `f64` takes the `1` widened (EXPR-5).
        "    let symbol: f64 = n + 1\n",
        "    let named: f64 = add_(n, 1)\n",
        // An operand with no type of its own is read as its parameter:
        // a `list<f64>`.
        "    let same = xs == [1, 2]\n",
        "    let also = eq_(xs, [1, 2])\n",
        // A link inside a chain is an operand, and is asked for
        // nothing: `n + 1` is an `i64` here.
        "    let chain: f64 = n + 1 + 2.5\n",
        "    var total = 1.5\n",
        "    total += 1\n",
        "}\n",
        // An operand that leaves a generic parameter for its place to
        // say has no place that says: it is reported, as it is where it
        // is an argument (GEN-8).
        "const fn repeat<T>(n: i64) -> list<T> => []\n",
        "const fn unresolved() {\n",
        "    let bad = repeat(1) == repeat(2)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:13:15 type/generic.unresolved GEN-8",
            "m.hgl:13:28 type/generic.unresolved GEN-8",
        ]
    );
    let statements = &body(&checked, "uses").statements;
    let value = |index: usize| match &statements[index] {
        Stmt::Declare {
            value: Some(value), ..
        }
        | Stmt::Assign { value, .. } => value,
        other => panic!("a statement with a value: {other:?}"),
    };
    for index in [0, 1] {
        assert_eq!(
            call_of(&checked, value(index)).0,
            "add_(lhs: i64, rhs: f64) -> f64"
        );
    }
    for index in [2, 3] {
        assert_eq!(
            call_of(&checked, value(index)).0,
            "eq_<T>(lhs: T, rhs: T) -> bool"
        );
    }
    assert_eq!(
        call_of(&checked, value(4)).0,
        "add_(lhs: i64, rhs: f64) -> f64"
    );
    let (_, inner, _) = value(4).kind.binary().expect("a symbol with two operands");
    assert_eq!(
        call_of(&checked, inner).0,
        "add_(lhs: i64, rhs: i64) -> i64"
    );
    assert_eq!(
        call_of(&checked, value(6)).0,
        "add_(lhs: f64, rhs: i64) -> f64"
    );
}

/// GEN-5: "A candidate that takes each integer literal as the `i64` it
/// is outranks one for which a literal is widened to an `f64`". A
/// literal under a symbol that is core's own arithmetic is still a
/// literal among a sequence's entries: `[-1]` and `[1 + 2]` select the
/// candidate for a `list<i64>`, as `[1]` does.
#[test]
fn a_literal_under_a_symbol_is_an_integer_first_among_entries() {
    let checked = module(concat!(
        "operator each<V>(values: V) -> V\n",
        "impl const fn each(values: list<i64>) -> list<i64> => values\n",
        "impl const fn each(values: list<f64>) -> list<f64> => values\n",
        "const fn uses() {\n",
        "    let plain = each([1])\n",
        "    let minus = each([-1])\n",
        "    let sum = each([1 + 2])\n",
        "    let real = each([-1.5])\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(
        locals(&checked, "uses"),
        [
            "plain: list<i64>",
            "minus: list<i64>",
            "sum: list<i64>",
            "real: list<f64>",
        ]
    );
}

/// GEN-5: "Whether a literal is widened is settled by the candidates'
/// result and parameter types, before their conditions: where a
/// candidate that gives the requested output takes every integer
/// literal as an `i64`, none is widened, and the conditions select
/// among the candidates that do. An option so does not change what a
/// literal is. Among candidates that tie and differ only in which
/// integer literals they take as an `f64`, the one that takes every one
/// of them as an `f64` is selected."
#[test]
fn what_an_integer_literal_is_at_a_call_is_settled_by_types() {
    let checked = module(concat!(
        "const fn uses(n: i64) {\n",
        // Three candidates give an `f64` with a literal widened: the
        // one that widens both is selected.
        "    let symbol: f64 = 1 + 2\n",
        "    let named: f64 = add_(1, 2)\n",
        // A division of two `i64` takes the `2` as the `i64` it is, and
        // none admits `.nan` (NUM-12): the `2` is not then an `f64`.
        "    let literal = n //{zero: .nan} 2\n",
        "    let whole = n //{zero: .nan} n\n",
        "    let call = floordiv_(n, 2, zero: .nan)\n",
        // No candidate that gives an `f64` takes two `i64`.
        "    let mixed: f64 = add_(n, 1)\n",
        "    let plain = n // 2\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:5:21 operator/operator.no_candidate OPS-1",
            "m.hgl:6:19 operator/operator.no_candidate OPS-1",
            "m.hgl:7:16 operator/operator.no_candidate GEN-5",
        ]
    );
    let statements = &body(&checked, "uses").statements;
    for index in [0, 1] {
        let Stmt::Declare {
            value: Some(value), ..
        } = &statements[index]
        else {
            panic!("a declaration with a value");
        };
        assert_eq!(
            call_of(&checked, value).0,
            "add_(lhs: f64, rhs: f64) -> f64"
        );
    }
    assert_eq!(locals(&checked, "uses")[6..], ["mixed: f64", "plain: i64"]);
}

/// EXPR-10: "A symbol operator may carry named configuration
/// immediately after its token: `a /{zero: .ieee} b` ... Bind these
/// arguments to the same core identity as the named arguments of a call
/// of it ... An entry finds what it names as a named argument does
/// (GEN-8)". EXPR-12: an entry that selects the result type does so
/// when the source is checked. NUM-9: a division under `zero: .error`
/// is a `result`. NUM-8: "A configuration selects the same nominal
/// operator ... an omitted option takes the default they declare".
#[test]
fn a_symbols_configuration_is_its_calls_named_arguments() {
    let checked = module(concat!(
        "const fn uses(a: f64, b: f64, n: i64) {\n",
        "    let plain = a / b\n",
        "    let nan = a /{zero: .nan} b\n",
        "    let checked = a /{zero: .error} b\n",
        "    let whole = n //{zero: .error} 2\n",
        // NUM-4: the tolerances of a comparison of numbers.
        "    let near = a =={abs: 0.01} b\n",
        "    let far = a !={abs: 0.01, rel: 0.1} b\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(
        locals(&checked, "uses")[3..],
        [
            "plain: f64",
            "nan: f64",
            "checked: result<f64, error>",
            "whole: result<i64, error>",
            "near: bool",
            "far: bool",
        ]
    );
    let statements = &body(&checked, "uses").statements;
    let Stmt::Declare {
        value: Some(near), ..
    } = &statements[4]
    else {
        panic!("a declaration with a value");
    };
    // EXPR-13: "left operand, configuration entries in their written
    // order, then right operand".
    let ExprKind::Call { arguments, .. } = &near.kind else {
        panic!("a call: {:?}", near.kind);
    };
    let places: Vec<Option<usize>> = arguments.iter().map(|given| given.parameter).collect();
    assert_eq!(places, [Some(0), Some(2), Some(1)]);
    // What a generic value parameter is given is a constant of the
    // call, and no argument of it.
    let Stmt::Declare {
        value: Some(checked_division),
        ..
    } = &statements[2]
    else {
        panic!("a declaration with a value");
    };
    let ExprKind::Call {
        callee: Callee::Candidate { generics, .. },
        arguments,
        ..
    } = &checked_division.kind
    else {
        panic!("a call of a candidate");
    };
    assert_eq!(arguments.len(), 2);
    let resolved: Vec<String> = generics.iter().map(|generic| generic.to_string()).collect();
    assert_eq!(resolved, [".error"]);
}

/// EXPR-10: "reject unsupported configuration". An entry is checked as
/// a named argument is (EXPR-2, GEN-6, GEN-8): its name is an option a
/// candidate declares, given once; a candidate that does not declare it
/// is not for the symbol; and what selects is a compile-time constant.
#[test]
fn a_symbols_configuration_is_checked_as_named_arguments_are() {
    let checked = module(concat!(
        "const fn uses(a: f64, b: f64, n: i64, policy: ZeroPolicy) {\n",
        "    let unknown = a /{zeo: .nan} b\n",
        "    let twice = a /{zero: .nan, zero: .ieee} b\n",
        "    let operand = a /{rhs: 2.0} b\n",
        // An `i64` cannot hold a NaN (NUM-12).
        "    let none = n //{zero: .nan} n\n",
        // No candidate of `add_` declares `zero`.
        "    let absent = a +{zero: .nan} b\n",
        "    let runtime = a /{zero: policy} b\n",
        "    let minus = -{zero: .nan} a\n",
        // A tolerance is an `f64` (NUM-11).
        "    let mistyped = a =={abs: \"wide\"} b\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:23 name/argument.unknown EXPR-10",
            "m.hgl:4:33 name/argument.duplicate EXPR-10",
            "m.hgl:5:23 name/argument.duplicate EXPR-10",
            "m.hgl:6:18 operator/operator.no_candidate OPS-1",
            "m.hgl:7:22 name/argument.unknown EXPR-10",
            "m.hgl:8:29 type/generic.argument_constant GEN-8",
            "m.hgl:9:19 name/argument.unknown EXPR-10",
            "m.hgl:10:22 operator/operator.no_candidate OPS-2",
        ]
    );
    assert_eq!(
        messages(&checked)[3],
        "`//` takes no `i64` and `i64` with `zero: .nan`"
    );
    assert_eq!(
        messages(&checked)[7],
        "`==` takes no `f64` and `f64` with `abs: str`"
    );
    assert_eq!(messages(&checked)[0], "`/` has no option `zeo`");
}
/// FN-1: "a `const` parameter takes what is fixed when the graph is
/// built. In composition that is any ordinary value. In a hook and in a
/// value function it is a constant expression: of literals, enum
/// members, generic value parameters, and the enclosing function's own
/// `const` parameters."
#[test]
fn a_const_parameter_takes_what_is_fixed() {
    let checked = module(concat!(
        "enum Mode { fast, slow }\n",
        "native type Store\n",
        "native const fn held(store: Store, const key: str) -> bool\n",
        "native const fn tuned(store: Store, const way: Mode, const limit: i64 = 1) -> bool\n",
        // A literal, the function's own `const` parameter, a generic
        // value parameter in a constant expression, and enum members.
        "const fn counted(store: Store) -> bool => held(store, \"count\")\n",
        "const fn named(store: Store, const key: str) -> bool => held(store, key)\n",
        "const fn sized<const N: i64>(store: Store) -> bool => tuned(store, .fast, N + 1)\n",
        "const fn slowed(store: Store) -> bool => tuned(store, Mode.slow, limit: -2)\n",
        // A value of one call: a parameter, a local, and each of two.
        "const fn found(store: Store, key: str) -> bool => held(store, key)\n",
        "const fn bound(store: Store) -> bool {\n",
        "    let key = \"count\"\n",
        "    return held(store, key)\n",
        "}\n",
        "const fn both(store: Store, way: Mode, size: i64) -> bool => tuned(store, way, size)\n",
        // In a hook a node's `const` parameter is fixed, and its input
        // is not.
        "native const fn scaled(value: i64, const factor: i64) -> i64\n",
        "fn node(value: i64, const factor: i64) -> i64 {\n",
        "    when {\n",
        "        let fine = scaled(value, factor)\n",
        "        return scaled(value, value)\n",
        "    }\n",
        "}\n",
        // In composition any ordinary value is fixed.
        "fn graph(value: i64, const factor: i64) -> i64 {\n",
        "    let doubled = factor * 2\n",
        "    return scaled(value, doubled)\n",
        "}\n",
        // A symbol calls the candidate it selects: each link of a
        // chain, and a compound assignment.
        "native impl const fn add_(lhs: Store, const rhs: str) -> Store\n",
        "const fn joined(store: Store) -> Store => store + \"a\" + \"b\"\n",
        "const fn chained(store: Store, key: str) -> Store => store + key + key\n",
        "const fn grown(store: Store, key: str) -> Store {\n",
        "    var total = store\n",
        "    total += key\n",
        "    return total\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:10:63 type/argument.constant FN-1",
            "m.hgl:13:24 type/argument.constant FN-1",
            "m.hgl:15:75 type/argument.constant FN-1",
            "m.hgl:15:80 type/argument.constant FN-1",
            "m.hgl:20:30 type/argument.constant FN-1",
            "m.hgl:29:62 type/argument.constant FN-1",
            "m.hgl:29:68 type/argument.constant FN-1",
            "m.hgl:32:14 type/argument.constant FN-1",
        ]
    );
}

/// TYPE-7: "A call of the enum's name constructs a member. It takes one
/// argument, by position, that is an `i64` or a `str`, and any other
/// type rejects: `Side(1)` is the member whose number is 1, and
/// `Side(\"buy\")` the member of that exact name. Construction is
/// checked: a constant that is no member's number or name rejects, and
/// a value that is none fails where it is evaluated."
#[test]
fn an_enum_is_constructed_from_a_number_or_a_name() {
    let checked = module(concat!(
        "enum Side { buy, sell }\n",
        "enum Level { low = 10, high = 20 }\n",
        "const fn made(code: i64, name: str) -> Side {\n",
        "    let one = Side(1)\n",
        "    let named = Side(\"buy\")\n",
        "    let level = Level(20)\n",
        // A value of the run is checked where it is evaluated.
        "    let late = Side(code)\n",
        "    let spoken = Side(name)\n",
        // A constant that is no member's: the number, and the name,
        // which is matched exactly.
        "    let two = Side(2)\n",
        "    let upper = Side(\"BUY\")\n",
        "    let between = Level(15)\n",
        // Neither an `i64` nor a `str`; no value; two.
        "    let real = Side(1.5)\n",
        "    let none = Side()\n",
        "    let pair = Side(1, 2)\n",
        "    return one\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:10:20 name/member.unknown TYPE-7",
            "m.hgl:11:22 name/member.unknown TYPE-7",
            "m.hgl:12:25 name/member.unknown TYPE-7",
            "m.hgl:13:21 type/type.mismatch TYPE-7",
            "m.hgl:14:16 type/argument.missing TYPE-7",
            "m.hgl:15:24 type/argument.unexpected TYPE-7",
        ]
    );
    assert_eq!(
        locals(&checked, "made")[2..7],
        [
            "one: Side",
            "named: Side",
            "level: Level",
            "late: Side",
            "spoken: Side",
        ]
    );

    // A constant that names a member is that member, wherever one is
    // looked for: a `switch` label (CTRL-6), a `switch` that covers the
    // enum (CTRL-5), and a `const` argument (FN-1).
    let members = module(concat!(
        "enum Side { buy, sell }\n",
        "native const fn priced(const side: Side) -> i64\n",
        "const fn covered(side: Side) -> i64 {\n",
        "    switch side {\n",
        "        case Side(0): return 1\n",
        "        case Side(\"sell\"): return 2\n",
        "    }\n",
        "}\n",
        "const fn doubled(side: Side) -> i64 {\n",
        "    switch side {\n",
        "        case Side(0): return 1\n",
        "        case .buy: return 2\n",
        "        default: return 3\n",
        "    }\n",
        "}\n",
        "const fn fixed<const N: i64>(code: i64) -> i64 {\n",
        "    let one = priced(Side(1))\n",
        "    let two = priced(Side(N))\n",
        "    return priced(Side(code))\n",
        "}\n",
    ));
    assert_eq!(
        members.summary(),
        [
            "m.hgl:13:14 type/switch.duplicate_label CTRL-6",
            "m.hgl:20:19 type/argument.constant FN-1",
        ]
    );

    // GEN-8: "The value a call gives a generic value parameter is a
    // compile-time constant: a literal, an enum member however it is
    // written (TYPE-7)". A member constructed from a constant is one,
    // and selects the candidate its member does; one constructed from
    // a value of the run is none.
    let generic = module(concat!(
        "enum Side { buy, sell }\n",
        "const fn priced<const side: Side>() -> i64 => 1\n",
        "operator quoted<L, O>(lhs: L) -> O\n",
        "impl const fn quoted(lhs: f64, side: Side = Side(0)) -> str\n",
        "    requires side == .buy => \"bid\"\n",
        "impl const fn quoted(lhs: f64, side: Side) -> f64\n",
        "    requires side == .sell => lhs\n",
        "const fn given<const N: i64>(code: i64, price: f64) -> i64 {\n",
        "    let member = priced(side: Side.sell)\n",
        "    let numbered = priced(side: Side(1))\n",
        "    let named = priced(side: Side(\"buy\"))\n",
        "    let fixed = priced(side: Side(N))\n",
        "    let bid = quoted(price, side: Side(0))\n",
        "    let offer = quoted(price, side: Side(\"sell\"))\n",
        // A default that is a construction: the member it names.
        "    let plain = quoted(price)\n",
        "    return priced(side: Side(code))\n",
        "}\n",
    ));
    assert_eq!(
        generic.summary(),
        ["m.hgl:17:25 type/generic.argument_constant GEN-8"]
    );
    assert_eq!(
        locals(&generic, "given")[6..],
        ["bid: str", "offer: f64", "plain: str"],
        "a constructed member selects the candidate its member does"
    );

    // A default is read in the file it is written in, which may be of
    // another module. Only a call of the option's own enum constructs
    // a member: `choose(0)` is a call of a function, and says nothing
    // of which member it gives.
    let elsewhere = check(&[
        (
            "lib.hgl",
            concat!(
                "module lib\n",
                "export enum Side { buy, sell }\n",
                "const fn choose(code: i64) -> Side => Side.sell\n",
                "operator quoted<L, O>(lhs: L) -> O\n",
                "impl const fn quoted(lhs: f64, side: Side = Side(0)) -> str\n",
                "    requires side == .buy => \"bid\"\n",
                "impl const fn quoted(lhs: i64, side: Side = choose(0)) -> str\n",
                "    requires side == .buy => \"bid\"\n",
            ),
        ),
        (
            "m.hgl",
            concat!(
                "module m\n",
                "use lib::{quoted}\n",
                "const fn uses(price: f64, count: i64) {\n",
                "    let constructed = quoted(price)\n",
                "    let called = quoted(count)\n",
                "}\n",
            ),
        ),
    ]);
    assert_eq!(elsewhere.summary(), NONE);
    assert_eq!(
        locals(&elsewhere, "uses")[2..],
        ["constructed: str", "called: _"],
        "`choose(0)` is not read as the member numbered 0"
    );
}

/// EXPR-9: "`[a: b]` has one pair entry, not a standalone pair value.
/// Empty brackets have no entries. The use site and expected type
/// constrain element types, keys, ordering, uniqueness, and placeholder
/// admission; syntax alone does not select a dictionary, timed trace,
/// or sparse delta."
#[test]
fn pairs_are_a_dictionary_where_one_is_expected() {
    let checked = module(concat!(
        "const fn made() -> dict<str, f64> {\n",
        // An integer literal is an `f64` where one is expected; no
        // entries make an empty dictionary; a value may be a sequence.
        "    let book: dict<str, f64> = [\"a\": 1.0, \"b\": 2]\n",
        "    let empty: dict<str, i64> = []\n",
        "    let nested: dict<str, list<i64>> = [\"a\": [1, 2], \"b\": []]\n",
        // Nothing says what the pairs make.
        "    let loose = [\"a\": 1.0]\n",
        // A key, and a value, of another type; a place that takes no
        // dictionary.
        "    let keyed: dict<str, f64> = [1: 1.0]\n",
        "    let valued: dict<str, f64> = [\"a\": \"text\"]\n",
        "    let listed: list<f64> = [\"a\": 1.0]\n",
        "    return [\"x\": 1.5]\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:6:17 type/sequence.context_type EXPR-9",
            "m.hgl:7:34 type/type.mismatch EXPR-5",
            "m.hgl:8:40 type/type.mismatch EXPR-5",
            "m.hgl:9:29 type/type.mismatch EXPR-5",
        ]
    );
    assert_eq!(
        locals(&checked, "made")[..3],
        [
            "book: dict<str, f64>",
            "empty: dict<str, i64>",
            "nested: dict<str, list<i64>>",
        ]
    );
    // `["x": 1.5]`: the pairs of the function's result.
    let Some(Stmt::Return {
        value: Some(returned),
        ..
    }) = body(&checked, "made").statements.last()
    else {
        panic!("`made` ends in a `return`");
    };
    assert_eq!(returned.ty.to_string(), "dict<str, f64>");
    let ExprKind::Pairs(entries) = &returned.kind else {
        panic!("the result is a sequence of pairs");
    };
    assert_eq!(entries.len(), 1);

    // GEN-5: a candidate that fits with no integer literal widened is
    // taken before one that needs it, a literal in a pair among them.
    let selected = module(concat!(
        "operator each<D>(book: D) -> str\n",
        "impl const fn each(book: dict<str, i64>) -> str => \"whole\"\n",
        "impl const fn each(book: dict<str, f64>) -> str => \"real\"\n",
        "impl const fn each(book: dict<str, list<f64>>) -> str => \"lists\"\n",
        "const fn chosen() -> str => each([\"a\": 1])\n",
        "const fn widened() -> str => each([\"a\": 1.5])\n",
        "const fn nested() -> str => each([\"a\": [1, 2]])\n",
    ));
    assert_eq!(selected.summary(), NONE);
    let candidate = |name: &str| {
        let Some(Stmt::Return {
            value: Some(call), ..
        }) = body(&selected, name).statements.last()
        else {
            panic!("`{name}` returns a call");
        };
        let ExprKind::Call {
            callee: Callee::Candidate { declaration, .. },
            ..
        } = &call.kind
        else {
            panic!("`{name}` calls a candidate of `each`");
        };
        *declaration
    };
    // The candidates are declared in order: 1, 2, 3 after the operator.
    assert_eq!(candidate("chosen"), 1, "the `dict<str, i64>` candidate");
    assert_eq!(candidate("widened"), 2, "the `dict<str, f64>` candidate");
    assert_eq!(candidate("nested"), 3, "the only one that takes lists");
}

/// EXPR-3: "An assignment may name a place inside a value, at any
/// depth: a field, `point.x = 1`, or an element, `row[0] = 1`,
/// `book["a"] = 1`. The place's root is a `var` ... A `let`, a
/// parameter, and an iteration binding are read-only all the way down.
/// The value is of the field's type; of a list's element type, under an
/// `i64` index; of a dictionary's value type, under a key of its key
/// type; or of a tuple's position, under a constant index ... A set has
/// no place to assign. A compound assignment reads the place first".
#[test]
fn a_place_inside_a_value_is_assigned() {
    let checked = module(concat!(
        "struct Point {\n",
        "    x: f64\n",
        "    y: f64\n",
        "}\n",
        "struct Line {\n",
        "    first: Point\n",
        "    last: Point\n",
        "}\n",
        "const fn moved(origin: Point, by: f64, rows: list<i64>) -> Point {\n",
        // A field, with an integer literal an `f64` there; a field of
        // a field; a dictionary's value, a list's element, a tuple's
        // position.
        "    var point = origin\n",
        "    point.x += by\n",
        "    point.y = 2\n",
        "    var line = Line(first: origin, last: point)\n",
        "    line.last.x = by\n",
        "    var counts: dict<str, i64> = []\n",
        "    counts[\"a\"] = 1\n",
        "    counts[\"a\"] += 1\n",
        "    var row = rows\n",
        "    row[0] = 7\n",
        "    var pair = (1, \"a\")\n",
        "    pair[1] = \"b\"\n",
        // A field of a parameter and of a `let`: read-only all the way
        // down.
        "    origin.x = by\n",
        "    let fixed = line\n",
        "    fixed.last.x = by\n",
        // A value of another type than the place's; a key of another
        // type than the dictionary's; a position the tuple has not.
        "    point.x = \"a\"\n",
        "    counts[1] = 1\n",
        "    pair[2] = 1\n",
        "    point.x += \"a\"\n",
        // A set has no place.
        "    var members: set<i64> = [1]\n",
        "    members[0] = 2\n",
        "    return point\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:23:5 name/binding.immutable EXPR-3",
            "m.hgl:25:5 name/binding.immutable EXPR-3",
            "m.hgl:26:15 type/type.mismatch EXPR-5",
            "m.hgl:27:12 type/type.mismatch EXPR-5",
            "m.hgl:28:10 type/tuple.index EXPR-7",
            "m.hgl:29:13 operator/operator.no_candidate OPS-1",
            "m.hgl:31:5 type/type.mismatch EXPR-7",
        ]
    );
    // `point.x += by`: the place, an `f64`, and what it becomes, the
    // sum of the place and `by`.
    let Stmt::AssignPlace {
        place,
        operator,
        value,
        ..
    } = &body(&checked, "moved").statements[1]
    else {
        panic!("the second statement assigns into a place");
    };
    assert_eq!(place.ty.to_string(), "f64");
    assert!(matches!(place.kind, ExprKind::Field { .. }));
    assert_eq!(*operator, Some(BinaryOperator::Add));
    assert_eq!(value.ty.to_string(), "f64");

    // NATIVE-6: "None is assigned." A property of a native type is
    // read-only however the native value is reached: as an element of
    // a `var`'s list, and under a field of it.
    let native = module(concat!(
        "native type Handle {\n",
        "    size: i64\n",
        "}\n",
        "struct Held {\n",
        "    handle: Handle\n",
        "    count: i64\n",
        "}\n",
        "const fn resized(handles: list<Handle>, held: Held) -> i64 {\n",
        "    var items = handles\n",
        "    items[0].size = 2\n",
        "    var keeper = held\n",
        "    keeper.handle.size += 1\n",
        "    keeper.count = 3\n",
        "    return keeper.count\n",
        "}\n",
    ));
    assert_eq!(
        native.summary(),
        [
            "m.hgl:11:5 name/binding.immutable NATIVE-6",
            "m.hgl:13:5 name/binding.immutable NATIVE-6",
        ]
    );
}

/// UNIT-8: "Defaults remain required-constant/cold expressions checked
/// against the expected type. Any parameter but a pack may have one: a
/// value function's, const configuration, or a temporal input." Each
/// default is kept, typed, for the stages after.
#[test]
fn a_default_is_a_value_of_its_parameters_type() {
    let checked = module(concat!(
        "enum Side { buy, sell }\n",
        "const fn base() -> i64 => 2\n",
        // An integer literal is an `f64` there; `.member` and `null`
        // take the parameter's type; a call is read as any.
        "const fn scaled(value: f64, factor: f64 = 2, side: Side = .buy, extra: i64? = null,\n",
        "    seed: i64 = base()) -> f64 => value\n",
        "fn node(value: i64, const by: i64 = 10, other: f64 = 1.5) -> i64 {\n",
        "    when { return value }\n",
        "}\n",
        "native const fn held(first: str = \"a\") -> str\n",
        // Of another type than the parameter's.
        "const fn wrong(value: i64 = \"text\", flag: bool = 1) -> i64 => 1\n",
        "fn temporal(value: i64 = 1.5) -> i64 {\n",
        "    when { return value }\n",
        "}\n",
        // A generic parameter's type is a call's to say.
        "const fn open<T>(value: T, fallback: T = 1) -> T => value\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:10:29 type/type.mismatch EXPR-5",
            "m.hgl:10:50 type/type.mismatch EXPR-5",
            "m.hgl:11:26 type/type.mismatch EXPR-5",
        ]
    );
    // `scaled`: no default for `value`, and one for each of the rest.
    let module = checked.program.module("m").expect("module m exists");
    let index = module
        .declarations
        .iter()
        .position(|declaration| declaration.name == "scaled")
        .expect("`scaled` is declared");
    let defaults = &module.defaults[index];
    let types: Vec<String> = defaults
        .iter()
        .map(|default| {
            default
                .as_ref()
                .map_or("none".to_string(), |value| value.ty.to_string())
        })
        .collect();
    assert_eq!(types, ["none", "f64", "Side", "i64?", "i64"]);
    assert!(
        matches!(
            defaults[1].as_ref().map(|value| &value.kind),
            Some(ExprKind::Widen(_))
        ),
        "the `2` of `factor: f64 = 2` is widened"
    );
}
