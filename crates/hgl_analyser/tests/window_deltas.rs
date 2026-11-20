//! The analyser against a window's delta, a type of its own (TYPE-16,
//! TYPE-11): its properties, its constructor, `delta_value` of a window
//! (END-3, END-9), what a window's result takes (TIME-7), and a window's
//! cell in a trace (TST-5, TST-16). Each test cites its rule.
//!
//! A finding is written `file:line:column category/code rule`.

mod common;

use common::{body, locals, module};
use hgl_analyser::hir::{Expr, ExprKind, Node, Stmt, walk_statement_nodes};

/// The value of the first `return` among the statements of the function
/// `name`, at any depth of its hooks and branches.
fn returned<'a>(checked: &'a common::Checked, name: &str) -> &'a Expr {
    let mut found = None;
    walk_statement_nodes(&body(checked, name).statements, &mut |node| {
        if let Node::Statement(Stmt::Return {
            value: Some(value), ..
        }) = node
        {
            found = found.or(Some(value));
        }
    });
    found.expect("the function returns a value")
}

/// TYPE-16's table: "`rolling_window<V, Max, Min>`: A type of its own,
/// with the properties `added`, the value that arrived in the cycle, and
/// `removed`, the value that left in it, each a `V?`, null where absent
/// (TYPE-11)". TYPE-11: "In a window of a nullable V,
/// `rolling_window<f64?, 5>` ... each is instead the optional a
/// nullable's delta is, a `delta<f64?>?`". "Such a delta is no T ... and
/// neither converts to the other. It is atomic: it has properties, none
/// is assigned", and it "keeps T whole in its identity": the deltas of
/// two windows of other sizes are two types, and a minimum written equal
/// to the maximum is the window without one (TYPE-11). A V that is a
/// generic parameter may be nullable or not, and its properties are not
/// known until it is.
#[test]
fn a_windows_delta_is_a_type_of_its_own_with_added_and_removed() {
    let checked = module(concat!(
        "struct Quote {\n    bid: f64\n}\n",
        "const fn kinds(\n",
        "    a: delta<rolling_window<i64, 3>>, b: delta<rolling_window<f64?, 5>>,\n",
        "    c: delta<rolling_window<Quote, 2>>, d: delta<rolling_window<i64, 3s, 1s>>,\n",
        ") -> i64 {\n",
        "    let arrived = a.added\n",
        "    let left = a.removed\n",
        "    let null_arrived = b.added\n",
        "    let quote_left = c.removed\n",
        "    let timed = d.added\n",
        // No such property; no V; another window's; the same window.
        "    let none = a.value\n",
        "    let whole: i64 = a\n",
        "    let other: delta<rolling_window<i64, 4>> = a\n",
        "    let same: delta<rolling_window<i64, 3, 3>> = a\n",
        // None is assigned.
        "    var held = a\n",
        "    held.added = 3\n",
        "    return 1\n",
        "}\n",
        "const fn generic<T>(change: delta<rolling_window<T, 3>>) -> i64 {\n",
        "    let seen = change.added\n",
        "    return 1\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:14:18 name/member.unknown TYPE-16",
            "m.hgl:15:22 type/type.mismatch EXPR-5",
            "m.hgl:16:48 type/type.mismatch EXPR-5",
            "m.hgl:19:5 name/binding.immutable TYPE-16",
        ]
    );
    assert_eq!(
        locals(&checked, "kinds")[..10],
        [
            "a: delta<rolling_window<i64, 3>>",
            "b: delta<rolling_window<f64?, 5>>",
            "c: delta<rolling_window<Quote, 2>>",
            "d: delta<rolling_window<i64, 3s, 1s>>",
            "arrived: i64?",
            "left: i64?",
            "null_arrived: delta<f64?>?",
            "quote_left: Quote?",
            "timed: i64?",
            "none: _",
        ]
    );
    assert_eq!(locals(&checked, "generic")[1], "seen: _");
}

/// TYPE-16: "One that is a type of its own is constructed by its
/// properties, by name: ... `delta<rolling_window<i64, 3>>(added: 5)`
/// ... a window's `added` or `removed` left out is absent (TYPE-11)".
/// Either, both or neither is given, null among them: "each a `V?`, null
/// where absent". "A window's delta holds values, not members, and is
/// not malformed where they are equal: `delta<rolling_window<i64,
/// 2>>(added: 3, removed: 3)` is a 3 that arrives as an earlier 3
/// leaves", so no `delta.overlap`. In a window of a nullable V each is a
/// `delta<f64?>?`: "`delta<f64?>()` for a null that arrived or left", and
/// "A `delta<V>` is a value where a `delta<V?>` is expected". A name that
/// is no property, one given twice, and a value of another type reject.
#[test]
fn a_windows_delta_is_made_from_either_property_both_or_neither() {
    let checked = module(concat!(
        "struct Quote {\n    bid: f64\n}\n",
        "const fn made() -> i64 {\n",
        "    let arrival = delta<rolling_window<i64, 3>>(added: 5)\n",
        "    let both = delta<rolling_window<i64, 3>>(removed: 2, added: 5)\n",
        "    let neither = delta<rolling_window<i64, 3>>()\n",
        "    let exit = delta<rolling_window<i64, 3s, 1s>>(removed: 2)\n",
        "    let absent = delta<rolling_window<i64, 3>>(added: null)\n",
        "    let equal = delta<rolling_window<i64, 2>>(added: 3, removed: 3)\n",
        "    let quote = delta<rolling_window<Quote, 2>>(added: Quote(bid: 1.0))\n",
        "    let null_arrived = delta<rolling_window<f64?, 5>>(added: delta<f64?>())\n",
        "    let value_arrived = delta<rolling_window<f64?, 5>>(added: 1.5, removed: 1)\n",
        "    let nothing_arrived = delta<rolling_window<f64?, 5>>(added: null)\n",
        "    let wrong_name = delta<rolling_window<i64, 3>>(gone: 3)\n",
        "    let wrong_type = delta<rolling_window<i64, 3>>(added: \"x\")\n",
        "    let twice = delta<rolling_window<i64, 3>>(added: 1, added: 2)\n",
        "    let inner = delta<rolling_window<f64?, 5>>(added: delta<f64?>(value: \"x\"))\n",
        "    return 1\n",
        "}\n",
        "const fn given() -> delta<rolling_window<i64, 3>> => delta<rolling_window<i64, 3>>(added: 5)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:16:52 name/delta.argument_name TYPE-16",
            "m.hgl:17:59 type/type.mismatch EXPR-5",
            "m.hgl:18:57 name/delta.duplicate_argument TYPE-16",
            "m.hgl:19:74 type/type.mismatch EXPR-5",
        ]
    );
    assert_eq!(
        locals(&checked, "made")[..10],
        [
            "arrival: delta<rolling_window<i64, 3>>",
            "both: delta<rolling_window<i64, 3>>",
            "neither: delta<rolling_window<i64, 3>>",
            "exit: delta<rolling_window<i64, 3s, 1s>>",
            "absent: delta<rolling_window<i64, 3>>",
            "equal: delta<rolling_window<i64, 2>>",
            "quote: delta<rolling_window<Quote, 2>>",
            "null_arrived: delta<rolling_window<f64?, 5>>",
            "value_arrived: delta<rolling_window<f64?, 5>>",
            "nothing_arrived: delta<rolling_window<f64?, 5>>",
        ]
    );
    // Each property is kept at its position: `removed`, the second, is
    // written first in `both`.
    let Stmt::Declare {
        value: Some(value), ..
    } = &body(&checked, "made").statements[1]
    else {
        panic!("the second statement binds `both`");
    };
    let ExprKind::Delta { arguments } = &value.kind else {
        panic!("`both` is a delta's construction: {:?}", value.kind);
    };
    let positions: Vec<Option<usize>> = arguments
        .iter()
        .map(|argument| argument.parameter)
        .collect();
    assert_eq!(positions, [Some(1), Some(0)]);
}

/// END-3: "`delta_value(x)` returns what `x` published this cycle, a
/// `delta<T>` for an endpoint of T", and END-9: "`delta_value(x)` of a
/// window has `added` ... and `removed`". TYPE-11: it "is read at each
/// tick of it, an exit alone included, before the window is `all_valid`
/// as after", and "A read of [the window's value] before the window is
/// `all_valid` fails with `value.unset_read`, or gives an `err` ...
/// where its place takes a `result<list<V>, error>`". So the delta is
/// read where the window is valid, and not as a checked read; the value
/// is the checked read. The delta of a child window, and the struct's
/// delta property for its window field, are a window's (TYPE-16: "of
/// type `delta<F>?`"). A window's delta is no V, and has no `err` form.
#[test]
fn delta_value_of_a_window_is_its_delta_read_before_it_is_all_valid() {
    let checked = module(concat!(
        "struct Book {\n    prices: rolling_window<f64, 3, 1>\n    last: f64\n}\n",
        "fn change(readings: rolling_window<i64, 3, 2>) -> delta<rolling_window<i64, 3, 2>> {\n",
        "    when modified(readings) { return delta_value(readings) }\n",
        "}\n",
        "fn arrived(readings: rolling_window<i64, 3, 2>) -> i64? {\n",
        "    when modified(readings) {\n",
        "        let d = delta_value(readings)\n",
        "        return d.added\n",
        "    }\n",
        "}\n",
        "fn left(readings: rolling_window<i64, 3, 2>) -> i64? {\n",
        "    when { return delta_value(readings).removed }\n",
        "}\n",
        "fn values(readings: rolling_window<i64, 3, 2>) -> result<list<i64>, error> {\n",
        "    when { return readings }\n",
        "}\n",
        "fn children(book: Book) -> delta<rolling_window<f64, 3, 1>>? {\n",
        "    when valid(book.prices) {\n",
        "        let child = delta_value(book.prices)\n",
        "        return delta_value(book).prices\n",
        "    }\n",
        "}\n",
        // No V; no `err` of the delta's read.
        "fn plain(readings: rolling_window<i64, 3, 2>) -> i64 {\n",
        "    when modified(readings) { return delta_value(readings) }\n",
        "}\n",
        "fn wrapped(readings: rolling_window<i64, 3, 2>) -> result<delta<rolling_window<i64, 3, 2>>, error> {\n",
        "    when { return delta_value(readings) }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:28:38 type/type.mismatch EXPR-5",
            "m.hgl:31:19 type/type.mismatch EXPR-5",
        ]
    );
    assert!(matches!(
        returned(&checked, "change").kind,
        ExprKind::Endpoint { .. }
    ));
    assert!(matches!(
        returned(&checked, "values").kind,
        ExprKind::CheckedWindow(_)
    ));
    assert_eq!(
        locals(&checked, "arrived")[1],
        "d: delta<rolling_window<i64, 3, 2>>"
    );
    assert_eq!(
        locals(&checked, "children")[1],
        "child: delta<rolling_window<f64, 3, 1>>"
    );
    assert_eq!(
        returned(&checked, "children").ty.to_string(),
        "delta<rolling_window<f64, 3, 1>>?"
    );
}

/// TIME-7: "A result of `rolling_window<V, Max, Min>` takes a V, which
/// arrives, or a delta of the window (TYPE-11)", by a `return` and by
/// `out =`; TYPE-11: "A window's delta that is published ... appends its
/// `added`". The delta of another window, a list, and a value of another
/// type are none of these. A window inside a struct is given its delta,
/// as the struct's delta has it: "a property for each field `name: F`
/// ... of type `delta<F>?`", which a V is not.
#[test]
fn a_windows_result_takes_a_value_or_its_delta() {
    let checked = module(concat!(
        "struct Book {\n    prices: rolling_window<i64, 3>\n    last: i64\n}\n",
        "fn collect(reading: i64) -> rolling_window<i64, 3> {\n",
        "    when { return reading }\n",
        "}\n",
        "fn relayed(readings: rolling_window<i64, 3>) -> rolling_window<i64, 3> {\n",
        "    inject out\n",
        "    when {\n",
        "        if valid(out) { out = delta_value(readings) }\n",
        "        return delta<rolling_window<i64, 3>>(added: 1)\n",
        "    }\n",
        "}\n",
        "fn widened(reading: i64) -> rolling_window<f64, 3> {\n",
        "    when { return 1 }\n",
        "}\n",
        "fn nullable(reading: f64?) -> rolling_window<f64?, 3> {\n",
        "    when { return reading }\n",
        "}\n",
        // Another window's delta; a list; another type.
        "fn other(readings: rolling_window<i64, 2>) -> rolling_window<i64, 3> {\n",
        "    when { return delta_value(readings) }\n",
        "}\n",
        "fn listed(reading: i64) -> rolling_window<i64, 3> {\n",
        "    when { return [reading] }\n",
        "}\n",
        "fn wrong(reading: str) -> rolling_window<i64, 3> {\n",
        "    when { return reading }\n",
        "}\n",
        // A window within a struct.
        "fn booked(price: i64) -> Book {\n",
        "    when {\n",
        "        if price < 0 { return delta<Book>(prices: price) }\n",
        "        return delta<Book>(prices: delta<rolling_window<i64, 3>>(added: price), last: price)\n",
        "    }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:23:19 type/type.mismatch EXPR-5",
            "m.hgl:26:19 type/type.mismatch EXPR-5",
            "m.hgl:29:19 type/type.mismatch EXPR-5",
            "m.hgl:33:51 type/type.mismatch EXPR-5",
        ]
    );
    assert_eq!(returned(&checked, "collect").ty.to_string(), "i64");
    assert_eq!(
        returned(&checked, "relayed").ty.to_string(),
        "delta<rolling_window<i64, 3>>"
    );
}

/// TST-5: "Each cell is one cycle's publication, written as a `return`
/// is (TIME-7), with T its parameter's type for an input and the
/// target's result type for an expectation": for a window a V, which
/// arrives, or the window's delta. TST-16: "for a result type T a
/// `list<tuple<datetime, delta<T>>>`, one tuple for each publication,
/// and for each exit of a window". A generic target's window is a
/// window's cell once its arguments are known. A cell of another type
/// rejects.
#[test]
fn a_windows_cell_in_a_trace_is_a_value_or_its_delta() {
    let checked = module(concat!(
        "fn collect(reading: i64) -> rolling_window<i64, 3> {\n",
        "    when { return reading }\n",
        "}\n",
        "fn change(readings: rolling_window<i64, 3>) -> delta<rolling_window<i64, 3>> {\n",
        "    when modified(readings) { return delta_value(readings) }\n",
        "}\n",
        "fn newest<T, const N: i64>(values: rolling_window<T, N>) -> delta<rolling_window<T, N>> {\n",
        "    when { return delta_value(values) }\n",
        "}\n",
        "const fn first(trace: list<tuple<datetime, delta<rolling_window<i64, 3>>>>) -> i64? =>\n",
        "    trace[0][1].added\n",
        "test traces {\n",
        "    assert eval(collect(reading: [1, 2])) ==\n",
        "        [delta<rolling_window<i64, 3>>(added: 1), delta<rolling_window<i64, 3>>(added: 2)]\n",
        "    assert eval(collect(reading: [1, 2])) == [1, 2]\n",
        "    assert eval(change(readings: [1, _, delta<rolling_window<i64, 3>>(added: 3)])) ==\n",
        "        [delta<rolling_window<i64, 3>>(added: 1), _, delta<rolling_window<i64, 3>>(added: 3)]\n",
        "    assert eval(newest<i64, 4>(values: [1, delta<rolling_window<i64, 4>>(added: 2)])) ==\n",
        "        [delta<rolling_window<i64, 4>>(added: 1), delta<rolling_window<i64, 4>>(added: 2)]\n",
        "    assert first(eval(collect(reading: [1]))) == 1\n",
        "    assert eval(change(readings: [\"x\"])) == []\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), ["m.hgl:22:35 type/type.mismatch EXPR-5"]);
}

/// TYPE-12: "An ordinary value type is written with no `rolling_window`,
/// `atomic`, or `signal` at any depth ... A dictionary's value and the
/// argument of `delta` are any type": a window's delta is an ordinary
/// value. It is a value function's parameter and result, a list's
/// element, and what a value function's generic stands for (FN-1); a
/// window itself is none of these.
#[test]
fn a_windows_delta_is_an_ordinary_value() {
    let checked = module(concat!(
        "const fn identity<T>(value: T) -> T => value\n",
        "const fn kept(changes: list<delta<rolling_window<i64, 3>>>) -> delta<rolling_window<i64, 3>> =>\n",
        "    changes[0]\n",
        "fn relayed(readings: rolling_window<i64, 3>) -> delta<rolling_window<i64, 3>> {\n",
        "    when modified(readings) { return identity(delta_value(readings)) }\n",
        "}\n",
        "const fn windowed(window: list<rolling_window<i64, 3>>) -> i64 => 1\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:8:32 type/type.not_ordinary FN-1"]
    );
}

/// OPS-2 (core): "`==` and `!=` compare two values of one type", core's
/// `eq_<T>` and `ne_<T>`: two deltas of one window are given to them, as
/// any two values of one type are, and two windows' deltas of other sizes
/// are not of one type. What makes two window deltas equal, core's OPS-2
/// does not say: the compiler decides nothing of it.
#[test]
fn two_deltas_of_one_window_are_given_to_equality() {
    let checked = module(concat!(
        "const fn compared(\n",
        "    a: delta<rolling_window<i64, 3>>, b: delta<rolling_window<i64, 3>>,\n",
        "    c: delta<rolling_window<i64, 4>>,\n",
        ") -> bool {\n",
        "    let same = a == b\n",
        "    return a != c\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:7:14 operator/operator.no_candidate OPS-2"]
    );
    assert_eq!(locals(&checked, "compared")[3], "same: bool");
}
