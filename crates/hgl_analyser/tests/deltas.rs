//! The analyser against the rule on `delta<T>` (TYPE-16) and on what a
//! node publishes (TIME-7, END-3). Each test cites its rule.
//!
//! A finding is written `file:line:column category/code rule`.

mod common;

use common::{Checked, check};
use hgl_analyser::hir::{ExprKind, Stmt};

/// Checks one file, `m.hgl`, of module `m`, with the core the compiler
/// is built with. The `module` line is added, so the text given starts
/// on line 2.
fn module(text: &str) -> Checked {
    check(&[("m.hgl", &format!("module m\n{text}"))])
}

/// The locals of `name`, as `local: type`.
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

/// TYPE-16: "`delta<T>` is the ordinary value type of what a temporal
/// value of T publishes in one cycle. The table gives it for each T.
/// For most T it is another type the language has ... A set's, a
/// dictionary's, and a struct's delta is a type of its own".
#[test]
fn a_delta_is_what_the_table_gives_for_its_type() {
    let checked = module(concat!(
        "enum Side { buy, sell }\n",
        "struct Quote {\n",
        "    bid: f64\n",
        "    ask: f64\n",
        "}\n",
        "const fn kinds(\n",
        // T itself; V of an atomic and of a window.
        "    a: delta<f64>, b: delta<Side>, c: delta<result<f64, error>>,\n",
        "    d: delta<atomic<Quote>>, e: delta<rolling<f64, 5>>,\n",
        // A list, of either kind; a tuple; a nullable.
        "    f: delta<list<f64>>, g: delta<list<Quote, 3>>, h: delta<tuple<f64, str>>,\n",
        "    i: delta<f64?>, j: delta<list<f64?>>,\n",
        // A type of its own, and the delta of one.
        "    l: delta<set<str>>, m: delta<dict<str, f64>>, n: delta<Quote>,\n",
        "    o: delta<delta<set<str>>>,\n",
        ") -> i64 => 1\n",
        // `ref` and `signal` have none.
        "const fn none(a: delta<signal>, b: delta<ref<f64>>, c: delta<signal?>) -> i64 => 1\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:15:18 shape/delta.unsupported_shape TYPE-16",
            "m.hgl:15:36 shape/delta.unsupported_shape TYPE-16",
            "m.hgl:15:56 shape/delta.unsupported_shape TYPE-16",
        ]
    );
    assert_eq!(
        locals(&checked, "kinds"),
        [
            "a: f64",
            "b: Side",
            "c: result<f64, error>",
            "d: Quote",
            "e: f64",
            "f: dict<i64, f64>",
            "g: dict<i64, delta<Quote>>",
            "h: tuple<f64?, str?>",
            "i: delta<f64?>",
            "j: dict<i64, delta<f64?>>",
            "l: delta<set<str>>",
            "m: delta<dict<str, f64>>",
            "n: delta<Quote>",
            "o: delta<set<str>>",
        ]
    );
}

/// TYPE-16: "In a generic declaration it stays as written until T is
/// known ... Such a delta is no T: a held complete value is not a
/// delta, and neither converts to the other. It is atomic: it has
/// properties, none is assigned". A set's delta has "the properties
/// `added` and `removed`, each a `set<K>`".
#[test]
fn a_delta_is_a_value_with_properties_that_are_read() {
    let checked = module(concat!(
        "struct TimedValue<T> {\n",
        "    at: datetime\n",
        "    value: delta<T>\n",
        "}\n",
        "const fn arrived(change: delta<set<str>>) -> set<str> => change.added\n",
        "const fn uses(change: delta<set<str>>, members: set<str>, stamp: datetime) -> i64 {\n",
        // In a generic struct the field is the delta of what T is.
        "    let scalar = TimedValue<f64>(at: stamp, value: 1.5)\n",
        "    let whole: f64 = scalar.value\n",
        "    let sets = TimedValue<set<str>>(at: stamp, value: change)\n",
        "    let kept: delta<set<str>> = sets.value\n",
        // Bound, stored, given, and read.
        "    let gone = change.removed\n",
        "    let came = arrived(change)\n",
        // No T, either way; no property of that name; none assigned.
        "    let complete: set<str> = change\n",
        "    let partial: delta<set<str>> = members\n",
        "    let wrong = change.missing\n",
        "    var held = change\n",
        "    held.added = []\n",
        "    return 1\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:14:30 type/type.mismatch EXPR-5",
            "m.hgl:15:36 type/type.mismatch EXPR-5",
            "m.hgl:16:24 name/member.unknown TYPE-16",
            "m.hgl:18:5 name/binding.immutable TYPE-16",
        ]
    );
    assert_eq!(
        locals(&checked, "uses")[3..9],
        [
            "scalar: TimedValue<f64>",
            "whole: f64",
            "sets: TimedValue<set<str>>",
            "kept: delta<set<str>>",
            "gone: set<str>",
            "came: set<str>",
        ]
    );
}

/// TYPE-16: a dictionary's delta is "a type of its own, with the
/// properties `updated`, a `dict<K, delta<V>>` of each key added or
/// changed with its delta, and `removed`, a `set<K>`". "A set's and a
/// dictionary's delta is constructed by its properties, by name:
/// `delta<set<i64>>(added: [1, 2], removed: [3])`,
/// `delta<dict<i64, i64>>(updated: [7: 10], removed: [9])`. A property
/// left out is empty." The constructor of a struct's delta is proposed,
/// and is not read.
#[test]
fn a_sets_and_a_dictionarys_delta_is_made_from_its_properties() {
    let checked = module(concat!(
        "struct Quote {\n    bid: f64\n    ask: f64\n}\n",
        "const fn joined(name: str) -> delta<set<str>> => delta<set<str>>(added: [name])\n",
        // In any order.
        "const fn swapped(old: str, new: str) -> delta<set<str>> =>\n",
        "    delta<set<str>>(removed: [old], added: [new])\n",
        "const fn priced(name: str, price: f64) -> delta<dict<str, f64>> =>\n",
        "    delta<dict<str, f64>>(updated: [name: price], removed: [])\n",
        "const fn uses(change: delta<dict<str, f64>>, sets: delta<dict<str, set<i64>>>) -> i64 {\n",
        "    let gone = change.removed\n",
        "    let moved = change.updated\n",
        // The value's own delta, where the value has one.
        "    let inner = sets.updated\n",
        "    let none = change.added\n",
        "    return 1\n",
        "}\n",
        // A name that is no property, one given twice, and values of
        // another type.
        "const fn wrong_name() -> delta<set<str>> => delta<set<str>>(updated: [\"a\"])\n",
        "const fn twice() -> delta<set<str>> => delta<set<str>>(added: [\"a\"], added: [\"b\"])\n",
        "const fn wrong_type() -> delta<set<str>> => delta<set<str>>(added: [1])\n",
        "const fn wrong_key() -> delta<dict<str, f64>> => delta<dict<str, f64>>(removed: [1])\n",
        // Proposed, and not read.
        "const fn of_struct() -> delta<Quote> => delta<Quote>(bid: 1.0)\n",
        // TIME-7: a hook publishes a delta it makes.
        "fn made(name: str) -> set<str> {\n",
        "    when { return delta<set<str>>(added: [name]) }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:15:23 name/member.unknown TYPE-16",
            "m.hgl:18:61 name/delta.argument_name TYPE-16",
            "m.hgl:19:70 name/delta.duplicate_argument TYPE-16",
            "m.hgl:20:69 type/type.mismatch EXPR-5",
            "m.hgl:21:82 type/type.mismatch EXPR-5",
        ]
    );
    assert_eq!(
        locals(&checked, "uses")[2..5],
        [
            "gone: set<str>",
            "moved: dict<str, f64>",
            "inner: dict<str, delta<set<i64>>>",
        ]
    );
    // The construction is kept with each property at its position:
    // `removed`, the second, is written first in `swapped`.
    let module = checked.program.module("m").expect("module m exists");
    let index = module
        .declarations
        .iter()
        .position(|declaration| declaration.name == "swapped")
        .expect("`swapped` is declared");
    let body = module.bodies[index].as_ref().expect("it has a body");
    let Some(Stmt::Return {
        value: Some(value), ..
    }) = body.statements.first()
    else {
        panic!("an arrow body is one return");
    };
    let ExprKind::Delta { arguments } = &value.kind else {
        panic!("the value is a delta's construction: {:?}", value.kind);
    };
    let positions: Vec<Option<usize>> = arguments
        .iter()
        .map(|argument| argument.parameter)
        .collect();
    assert_eq!(positions, [Some(1), Some(0)]);
    assert_eq!(value.ty.to_string(), "delta<set<str>>");
}

/// TYPE-16: a nullable's delta is "a type of its own, an optional, with
/// the property `value`, a `delta<V>?`: null for a published null".
/// "`delta<i64?>()` is a published null. In a tuple's position or a
/// struct's field, null leaves the position or the field as it was ...
/// A `delta<V>` is a value where a `delta<V?>` is expected, as a T is
/// where a `T?` is". A struct's delta has "a property for each field
/// `name: F`, of the same name and of type `delta<F>?`".
#[test]
fn a_nullables_delta_is_an_optional_and_a_structs_has_its_fields() {
    let checked = module(concat!(
        "struct Quote {\n    bid: f64\n    note: str? = null\n}\n",
        "enum Side { buy, sell }\n",
        // Null is no tick; the optional holds what was published.
        "const fn seen(change: delta<i64?>?) -> str {\n",
        "    if change == null { return \"no tick\" }\n",
        "    if change.value == null { return \"a null\" }\n",
        "    return \"a value\"\n",
        "}\n",
        "const fn made() -> i64 {\n",
        "    let empty = delta<i64?>()\n",
        "    let seven = delta<i64?>(value: 7)\n",
        // A `delta<V>` where a `delta<V?>` is expected.
        "    let direct: delta<i64?> = 7\n",
        "    let wide: delta<f64?> = 1\n",
        "    let position: tuple<delta<i64?>?, delta<i64?>?> = (7, null)\n",
        // Null is no delta; a value of another type; no such property.
        "    let absent: delta<i64?> = null\n",
        "    let wrong = delta<i64?>(value: \"a\")\n",
        "    let other = delta<i64?>(added: 1)\n",
        // A form that takes its type from its place is the `delta<V>`:
        // a list's delta is a dictionary, and its entries are held to
        // it; an enum's member; a set's delta is no literal.
        "    let moved: delta<list<i64>?> = [0: 7]\n",
        "    let side: delta<Side?> = .buy\n",
        "    let good: delta<result<i64, str>?> = ok(1)\n",
        "    let bad: delta<result<i64, str>?> = err(\"no\")\n",
        "    let keyed: delta<list<i64>?> = [\"wrong\": \"wrong\"]\n",
        "    return 1\n",
        "}\n",
        // A branch gives the whole optional, or what it holds.
        "const fn chosen(flag: bool) -> delta<i64?> => if flag { delta<i64?>() } else { 7 }\n",
        // A struct's delta: a property for each field.
        "const fn bids(change: delta<Quote>) -> f64? => change.bid\n",
        "const fn notes(change: delta<Quote>) -> delta<str?>? => change.note\n",
        "const fn cleared_note() -> delta<Quote> => delta<Quote>(bid: 1.0, note: delta<str?>())\n",
        "const fn noted() -> delta<Quote> => delta<Quote>(note: \"n\")\n",
        "const fn no_field() -> delta<Quote> => delta<Quote>(ask: 1.0)\n",
        // A field that is nullable: its property is a `delta<F>?`.
        // Null leaves it as it was, and a literal is the list's delta.
        "struct Held {\n    items: list<i64>? = null\n}\n",
        "const fn held() -> delta<Held> => delta<Held>(items: [0: 7])\n",
        "const fn kept() -> delta<Held> => delta<Held>(items: null)\n",
        "const fn held_wrong() -> delta<Held> => delta<Held>(items: [\"wrong\": \"wrong\"])\n",
        // TIME-7: `return null` publishes a null, as the delta does.
        "fn cleared(command: i64) -> i64? {\n",
        "    when {\n",
        "        if command == 0 { return null }\n",
        "        if command == 1 { return delta<i64?>() }\n",
        "        return command\n",
        "    }\n",
        "}\n",
        "fn relayed(value: i64?) -> i64? {\n",
        "    when { return delta_value(value) }\n",
        "}\n",
        // TST-5: a cell written `null` is a published null. One
        // written as the delta is that delta, in a block as outside it.
        "test nulls {\n",
        "    assert eval(cleared(command: [0, 2])) == [null, 2]\n",
        "    assert eval(relayed(value: [null, _, 3])) == [null, _, 3]\n",
        "    assert eval(cleared(command: [1, 2])) == [delta<i64?>(), 2]\n",
        "    assert eval(cleared(command: [1, 0])) == [{ delta<i64?>() }, (null)]\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:18:31 type/type.mismatch TYPE-14",
            "m.hgl:19:36 type/type.mismatch EXPR-5",
            "m.hgl:20:29 name/delta.argument_name TYPE-16",
            // The key and the value of the entry, each against the
            // dictionary a list's delta is.
            "m.hgl:25:37 type/type.mismatch EXPR-5",
            "m.hgl:25:46 type/type.mismatch EXPR-5",
            "m.hgl:33:53 name/delta.argument_name TYPE-16",
            // Through the `?` of a struct delta's property.
            "m.hgl:39:61 type/type.mismatch EXPR-5",
            "m.hgl:39:70 type/type.mismatch EXPR-5",
        ]
    );
    assert_eq!(
        locals(&checked, "made"),
        [
            "empty: delta<i64?>",
            "seven: delta<i64?>",
            "direct: delta<i64?>",
            "wide: delta<f64?>",
            "position: tuple<delta<i64?>?, delta<i64?>?>",
            "absent: delta<i64?>",
            "wrong: delta<i64?>",
            "other: delta<i64?>",
            "moved: delta<list<i64>?>",
            "side: delta<Side?>",
            "good: delta<result<i64, str>?>",
            "bad: delta<result<i64, str>?>",
            "keyed: delta<list<i64>?>",
        ]
    );
}

/// END-3: "`delta_value(x)` returns what `x` published this cycle, a
/// `delta<T>` for an endpoint of T (the language's TYPE-16). For a
/// scalar it is the value published". TIME-7: "A runtime return
/// publishes to its result": a complete value, or what the result
/// publishes in one cycle, which core's cases relay with `return
/// delta_value(input)`. TYPE-9: "`atomic<V>` stops expansion at a
/// complete ordinary V".
#[test]
fn a_node_publishes_a_whole_value_or_a_delta() {
    let checked = module(concat!(
        "struct Quote {\n",
        "    bid: f64\n",
        "    ask: f64\n",
        "}\n",
        "const fn arrived(change: delta<set<str>>) -> set<str> => change.added\n",
        // A delta relayed; a whole value; `out` assigned either.
        "fn relay(members: set<str>) -> set<str> {\n",
        "    inject out\n",
        "    when {\n",
        "        let change = delta_value(members)\n",
        "        out = change\n",
        "        out = members\n",
        "        return delta_value(members)\n",
        "    }\n",
        "}\n",
        // An atomic result takes its V, and an atomic input as it is.
        "fn arrivals(members: set<str>) -> atomic<set<str>> {\n",
        "    when { return arrived(delta_value(members)) }\n",
        "}\n",
        "fn pass(quote: atomic<Quote>) -> atomic<Quote> {\n",
        "    when { return quote }\n",
        "}\n",
        "fn made(bid: f64) -> atomic<Quote> {\n",
        "    when { return Quote(bid: bid, ask: bid) }\n",
        "}\n",
        // A scalar's delta is the scalar; a list's a dictionary.
        // Under a `?` that marks an input that may be left out, the
        // delta is the shape's.
        "fn scalars(price: f64, rows: list<f64>, window: rolling<f64, 5>? = null) -> f64 {\n",
        "    when {\n",
        "        let moved: dict<i64, f64> = delta_value(rows)\n",
        "        let latest: f64 = delta_value(window)\n",
        "        return delta_value(price)\n",
        "    }\n",
        "}\n",
        // Neither the result nor its delta.
        "fn wrong(members: set<str>, others: set<i64>) -> set<str> {\n",
        "    when { return delta_value(others) }\n",
        "}\n",
        "fn source() -> set<str> {\n",
        "    yield 1s: 7\n",
        "}\n",
        // A delta written out takes its type from the result's delta: a
        // position that did not change is null. What is neither a
        // complete value nor the delta is reported once, as the value.
        "fn pairs(first: f64, second: f64) -> tuple<f64, f64> {\n",
        "    inject out\n",
        "    when {\n",
        "        out = (first, null)\n",
        "        out = (first, second)\n",
        "        return (1, null)\n",
        "    }\n",
        "}\n",
        "fn wrong_pairs(first: f64) -> tuple<f64, f64> {\n",
        "    when { return (first, \"a\") }\n",
        "}\n",
        // A `signal` and a `ref` publish nothing of their own.
        "fn ticked(probe: signal, pointer: ref<f64>) -> bool {\n",
        "    when {\n",
        "        let first = delta_value(probe)\n",
        "        let second = delta_value(pointer)\n",
        "        return true\n",
        "    }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:33:19 type/type.mismatch EXPR-5",
            "m.hgl:36:15 type/type.mismatch EXPR-5",
            "m.hgl:47:27 type/type.mismatch EXPR-5",
            "m.hgl:51:21 shape/delta.unsupported_shape TYPE-16",
            "m.hgl:52:22 shape/delta.unsupported_shape TYPE-16",
        ]
    );
    assert_eq!(locals(&checked, "relay")[2..], ["change: delta<set<str>>"]);
}

/// TYPE-16: "In a generic declaration it stays as written until T is
/// known". TIME-7 and END-3, in the bodies core's eval replay and
/// recorder have: a source of T yields the `delta<T>` an entry holds,
/// and a recorder stores `delta_value(input)` as one. A scalar, a
/// nullable and a set are told apart once T is known, and a value that
/// is no delta of the output's shape is rejected.
#[test]
fn a_generic_delta_is_published_and_recorded() {
    let checked = module(concat!(
        "struct EvalEntry<T> {\n",
        "    time: datetime\n",
        "    value: delta<T>\n",
        "}\n",
        "native const fn appended<T>(items: list<T>, item: T) -> list<T>\n",
        "fn eval_replay<T>(const items: list<EvalEntry<T>>) -> T {\n",
        "    for item in items {\n",
        "        yield item.time: item.value\n",
        "    }\n",
        "}\n",
        "fn eval_record<T>(input: T) {\n",
        "    cache entries: list<EvalEntry<T>> = []\n",
        "    inject clock, global_state\n",
        "    when valid(input) && modified(input) {\n",
        "        entries = appended(entries, EvalEntry<T>(\n",
        "            time: clock.evaluation_time,\n",
        "            value: delta_value(input)\n",
        "        ))\n",
        "    }\n",
        "    stop { put(global_state, \"result\", entries) }\n",
        "}\n",
        "fn positive(value: i64) -> i64 {\n",
        "    when {\n",
        "        if value > 0 { return value }\n",
        "    }\n",
        "}\n",
        "fn wired_positive(const items: list<EvalEntry<i64>>) {\n",
        "    let replayed = eval_replay<i64>(items)\n",
        "    let filtered = positive(replayed)\n",
        "    eval_record<i64>(filtered)\n",
        "}\n",
        // Once T is known the entry's value is that T's delta: the
        // scalar itself, the optional of a nullable, a set's delta.
        "const fn made(at: datetime, change: delta<set<str>>, members: set<str>) -> i64 {\n",
        "    let scalar = EvalEntry<i64>(time: at, value: 7)\n",
        "    let nullable = EvalEntry<i64?>(time: at, value: delta<i64?>())\n",
        "    let structural = EvalEntry<set<str>>(time: at, value: change)\n",
        "    let whole = EvalEntry<set<str>>(time: at, value: members)\n",
        "    let absent = EvalEntry<i64>(time: at, value: null)\n",
        "    return 1\n",
        "}\n",
        // An entry of another shape than the source's output; a whole
        // value where the entry holds a delta.
        "fn replay_other<T, U>(const items: list<EvalEntry<U>>) -> T {\n",
        "    for item in items {\n",
        "        yield item.time: item.value\n",
        "    }\n",
        "}\n",
        "fn record_whole<T>(input: T) {\n",
        "    cache entries: list<EvalEntry<T>> = []\n",
        "    inject clock\n",
        "    when {\n",
        "        entries = appended(entries, EvalEntry<T>(time: clock.evaluation_time, value: input))\n",
        "    }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:37:54 type/type.mismatch EXPR-5",
            "m.hgl:38:50 type/type.mismatch TYPE-14",
            "m.hgl:43:26 type/type.mismatch EXPR-5",
            "m.hgl:50:86 type/type.mismatch EXPR-5",
        ]
    );
    assert_eq!(
        locals(&checked, "made")[3..],
        [
            "scalar: EvalEntry<i64>",
            "nullable: EvalEntry<i64?>",
            "structural: EvalEntry<set<str>>",
            "whole: EvalEntry<set<str>>",
            "absent: EvalEntry<i64>",
        ]
    );
}
