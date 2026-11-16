//! The analyser against the rule on `delta<T>` (TYPE-16) and on what a
//! node publishes (TIME-7, END-3). Each test cites its rule.
//!
//! A finding is written `file:line:column category/code rule`.

mod common;

use common::{Checked, check};

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
            "i: f64?",
            "j: dict<i64, f64?>",
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
