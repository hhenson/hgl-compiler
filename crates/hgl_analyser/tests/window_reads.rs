//! The analyser against what a window publishes and loses in a cycle:
//! its delta, the value that arrived (TYPE-16, TYPE-11), `delta_value` of
//! any endpoint taking its form from its place (ERR-14), core's
//! `removed_value` (TIME-9, END-9), and what a window's result takes
//! (TIME-7). Each test cites its rule.
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

/// TYPE-16's table: "`rolling_window<V, Max, Min>`: V: the value that
/// arrived; for a nullable V, `delta<V>`". TYPE-11: "A window's
/// delta (TYPE-16) is the value that arrived in the cycle, and is read
/// before the window is `all_valid` as after". TIME-7: "A result of
/// `rolling_window<V, Max, Min>` takes a V, which arrives, and nothing
/// else": a V widened as an `f64` is, and not a list or another type.
#[test]
fn a_windows_delta_is_the_value_that_arrived() {
    let checked = module(concat!(
        "const fn kinds(a: delta<rolling_window<i64, 3>>, b: delta<rolling_window<f64?, 5s>>) -> i64 {\n",
        "    return 1\n",
        "}\n",
        "fn collect(reading: i64) -> rolling_window<i64, 3> {\n",
        "    when { return reading }\n",
        "}\n",
        "fn relayed(readings: rolling_window<i64, 3>) -> rolling_window<i64, 3> {\n",
        "    inject out\n",
        "    when {\n",
        "        if valid(out) { out = delta_value(readings) }\n",
        "        return delta_value(readings)\n",
        "    }\n",
        "}\n",
        "fn arrived(readings: rolling_window<i64, 3, 2>) -> i64 {\n",
        "    when modified(readings) { return delta_value(readings) }\n",
        "}\n",
        "fn widened(reading: i64) -> rolling_window<f64, 3> {\n",
        "    when { return 1 }\n",
        "}\n",
        // A list, and another type.
        "fn listed(reading: i64) -> rolling_window<i64, 3> {\n",
        "    when { return [reading] }\n",
        "}\n",
        "fn wrong(reading: str) -> rolling_window<i64, 3> {\n",
        "    when { return reading }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:22:19 type/type.mismatch EXPR-5",
            "m.hgl:25:19 type/type.mismatch EXPR-5",
        ]
    );
    assert_eq!(locals(&checked, "kinds"), ["a: i64", "b: delta<f64?>"]);
    assert_eq!(returned(&checked, "arrived").ty.to_string(), "i64");
}

/// ERR-14: "So does `delta_value` of any endpoint (TIME-9), T being its
/// `delta<T>`: where the place expects `result<delta<T>, error>` it gives
/// `ok` of the delta, or `err` coded `value.unset_read` where the
/// endpoint has no delta in the cycle; elsewhere it gives the delta".
/// Returned, bound and given, of a scalar, a list, a child and a window,
/// it is of the result, the read held as checked (`CheckedDelta`); "for
/// every other endpoint the `result` form is `ok`, so generic code may
/// read the delta of a T it does not know in that form", as core's
/// `record` does.
#[test]
fn delta_value_takes_its_form_from_its_place() {
    let checked = module(concat!(
        "struct Book {\n    prices: rolling_window<i64, 3s, 1s>\n    last: i64\n}\n",
        "const fn code_of(outcome: result<f64, error>) -> str => \"\"\n",
        "fn scalar(price: f64) -> result<f64, error> {\n",
        "    when { return delta_value(price) }\n",
        "}\n",
        "fn listed(rows: list<i64>) -> result<delta<list<i64>>, error> {\n",
        "    when {\n",
        "        let change: result<delta<list<i64>>, error> = delta_value(rows)\n",
        "        return change\n",
        "    }\n",
        "}\n",
        "fn given(price: f64) -> str {\n",
        "    when { return code_of(delta_value(price)) }\n",
        "}\n",
        "fn timed(readings: rolling_window<i64, 3s, 1s>) -> result<i64, error> {\n",
        "    when modified(readings) { return delta_value(readings) }\n",
        "}\n",
        "fn child(book: Book) -> result<i64, error> {\n",
        "    when valid(book.prices) { return delta_value(book.prices) }\n",
        "}\n",
        "fn recorded<T>(input: T) -> list<delta<T>> {\n",
        "    cache entries: list<delta<T>> = []\n",
        "    when valid(input) && modified(input) {\n",
        "        let change: result<delta<T>, error> = delta_value(input)\n",
        "        if is_ok(change) { append(entries, change.value) }\n",
        "        return entries\n",
        "    }\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), Vec::<String>::new());
    for name in ["scalar", "timed", "child"] {
        let value = returned(&checked, name);
        assert!(
            matches!(value.kind, ExprKind::CheckedDelta(_)),
            "{name}: {:?}",
            value.kind
        );
    }
    assert_eq!(
        returned(&checked, "timed").ty.to_string(),
        "result<i64, error>"
    );
    assert_eq!(
        locals(&checked, "listed")[1],
        "change: result<delta<list<i64>>, error>"
    );
}

/// TYPE-11: "A read of the delta of a window of a duration whose place is
/// not a `result` rejects (`payload.unguarded`): an arrival may be
/// missing at its tick. One of a window of a tick count is not rejected:
/// it ticks only at an arrival." Of an input, of a child and of `out`,
/// returned and bound; a window that may be left out is the window
/// under its `?` (TYPE-14). A window's VALUE read is a checked read,
/// where a `result` is taken, as before (`CheckedWindow`).
#[test]
fn a_plain_read_of_the_delta_of_a_window_of_a_duration_rejects() {
    let checked = module(concat!(
        "struct Book {\n    prices: rolling_window<i64, 3s, 1s>\n    last: i64\n}\n",
        "fn ticks(readings: rolling_window<i64, 3>) -> i64 {\n",
        "    when modified(readings) { return delta_value(readings) }\n",
        "}\n",
        "fn timed(readings: rolling_window<i64, 3s, 1s>) -> i64 {\n",
        "    when modified(readings) { return delta_value(readings) }\n",
        "}\n",
        "fn bound(readings: rolling_window<i64, 3s, 1s>? = null) -> i64 {\n",
        "    when modified(readings) {\n",
        "        let plain = delta_value(readings)\n",
        "        return 0\n",
        "    }\n",
        "}\n",
        "fn child(book: Book) -> i64 {\n",
        "    when valid(book.prices) { return delta_value(book.prices) }\n",
        "}\n",
        "fn own(price: i64) -> rolling_window<i64, 3s> {\n",
        "    inject out\n",
        "    when {\n",
        "        if valid(out) && modified(out) { let seen = delta_value(out) }\n",
        "        return price\n",
        "    }\n",
        "}\n",
        "fn values(readings: rolling_window<i64, 3s, 1s>) -> result<list<i64>, error> {\n",
        "    when { return readings }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:10:38 phase/payload.unguarded TYPE-11",
            "m.hgl:14:21 phase/payload.unguarded TYPE-11",
            "m.hgl:19:38 phase/payload.unguarded TYPE-11",
            "m.hgl:24:53 phase/payload.unguarded TYPE-11",
        ]
    );
    assert!(matches!(
        returned(&checked, "values").kind,
        ExprKind::CheckedWindow(_)
    ));
}

/// TIME-9: "The operations on an endpoint are core's native value
/// functions with such a parameter: ... `removed_value` ...
/// `removed_value` takes a window (TYPE-11), and nothing else." TYPE-11:
/// "It takes its form from its place as `delta_value` does, and gives an
/// `err` where no value left (ERR-14). A read of it whose place is not a
/// `result` rejects for every window (`payload.unguarded`)". Where the
/// place takes `result<V, error>` the place says what V is, as for any
/// call (GEN-2), and the read is of the result. Given no window, core's
/// declaration, `endpoint<rolling_window<V, Max, Min>>`, binds nothing of
/// the window's sizes (`generic.unresolved`, GEN-8).
#[test]
fn removed_value_is_read_where_a_result_is_taken() {
    let checked = module(concat!(
        "fn departure(readings: rolling_window<i64, 3s, 1s>) -> result<i64, error> {\n",
        "    when modified(readings) { return removed_value(readings) }\n",
        "}\n",
        "fn pushed_out(readings: rolling_window<f64, 2>) -> result<f64, error> {\n",
        "    when modified(readings) {\n",
        "        let left: result<f64, error> = removed_value(readings)\n",
        "        return left\n",
        "    }\n",
        "}\n",
        // A plain read, of ticks and of a duration.
        "fn ticks(readings: rolling_window<i64, 2>) -> i64 {\n",
        "    when modified(readings) { return removed_value(readings) }\n",
        "}\n",
        "fn timed(readings: rolling_window<i64, 3s>) -> i64 {\n",
        "    when modified(readings) {\n",
        "        let left = removed_value(readings)\n",
        "        return 0\n",
        "    }\n",
        "}\n",
        "fn scalar(price: i64) -> result<i64, error> {\n",
        "    when { return removed_value(price) }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:12:38 phase/payload.unguarded TYPE-11",
            "m.hgl:16:20 phase/payload.unguarded TYPE-11",
            "m.hgl:21:19 type/generic.unresolved GEN-8",
        ]
    );
    let value = returned(&checked, "departure");
    assert!(
        matches!(value.kind, ExprKind::CheckedDelta(_)),
        "{:?}",
        value.kind
    );
    assert_eq!(value.ty.to_string(), "result<i64, error>");
    assert_eq!(
        locals(&checked, "pushed_out")[1],
        "left: result<f64, error>"
    );
}

/// TST-5: "Each cell is one cycle's publication": of a window, the `V`
/// that arrived, its delta (TYPE-16). TST-16: a trace given to a `const
/// fn` is "a `list<tuple<datetime, delta<T>>>`", for a window one of V. A
/// generic target's window has cells of V once its arguments are known.
#[test]
fn a_windows_cell_in_a_trace_is_a_v() {
    let checked = module(concat!(
        "fn collect(reading: i64) -> rolling_window<i64, 3> {\n",
        "    when { return reading }\n",
        "}\n",
        "fn arrived(readings: rolling_window<i64, 3>) -> i64 {\n",
        "    when modified(readings) { return delta_value(readings) }\n",
        "}\n",
        "fn counted<T, const N: i64>(values: rolling_window<T, N>) -> i64 {\n",
        "    when { return 1 }\n",
        "}\n",
        "const fn first(trace: list<tuple<datetime, i64>>) -> i64 => trace[0][1]\n",
        "test traces {\n",
        "    assert eval collect(reading: [1, 2]) == [1, 2]\n",
        "    assert eval arrived(readings: [1, _, 3]) == [1, _, 3]\n",
        "    assert eval counted<i64, 4>(values: [1, 2]) == [1, 1]\n",
        "    assert first(eval collect(reading: [1])) == 1\n",
        "    assert eval arrived(readings: [\"x\"]) == []\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), ["m.hgl:17:36 type/type.mismatch EXPR-5"]);
}

/// FN-16: "Nor does `ref` stand on a window: a `ref` parameter or a
/// `ref` result whose type is a `rolling_window` rejects ... A mark on a
/// window rejects with `ref.position` too", one that may be left out
/// among them. "`removed_value` (TYPE-11) of a window
/// that is a child of a `ref` parameter or of `out` of a `ref` result, a
/// field of a struct given by `ref`, is set aside as `delta_value` is: a
/// read of it there rejects with `ref.read`", where a `result` is taken
/// as elsewhere.
#[test]
fn a_window_is_not_marked_ref() {
    let checked = module(concat!(
        "struct Book {\n    prices: rolling_window<i64, 3>\n    last: i64\n}\n",
        "fn child(ref book: Book) -> result<i64, error> {\n",
        "    when modified(book) { return removed_value(book.prices) }\n",
        "}\n",
        "fn held(ref readings: rolling_window<i64, 3>) -> result<i64, error> {\n",
        "    when modified(readings) { return removed_value(readings) }\n",
        "}\n",
        "fn maybe(ref readings: rolling_window<i64, 3s>? = null) -> bool {\n",
        "    when { return valid(readings) }\n",
        "}\n",
        "fn chosen(readings: rolling_window<i64, 3>) -> ref rolling_window<i64, 3> {\n",
        "    when { return readings }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:7:48 phase/ref.read FN-16",
            "m.hgl:9:9 function-kind/ref.position FN-16",
            "m.hgl:12:10 function-kind/ref.position FN-16",
            "m.hgl:15:48 function-kind/ref.position FN-16",
        ]
    );
}

/// TYPE-11: "That of a window of a nullable V, `rolling_window<f64?,
/// 3>`, is the nullable's own delta, `delta<f64?>`, which holds what
/// arrived, a null among them ... `removed_value` of it gives the V
/// itself, null included". TIME-7: "One of a nullable V
/// takes its delta as well, `delta<f64?>(value: 1.0)` or `delta<f64?>()`,
/// as the arrival it holds". TST-5: its trace's cells are that delta, a
/// cell written `null` a published null.
#[test]
fn a_window_of_a_nullable_has_the_nullables_delta() {
    let checked = module(concat!(
        "fn collect(reading: f64?) -> rolling_window<f64?, 3> {\n",
        "    when {\n",
        "        if reading == null { return delta<f64?>() }\n",
        "        return reading\n",
        "    }\n",
        "}\n",
        "fn arrived(readings: rolling_window<f64?, 3>) -> delta<f64?> {\n",
        "    when modified(readings) { return delta_value(readings) }\n",
        "}\n",
        "fn left(readings: rolling_window<f64?, 3>) -> result<f64?, error> {\n",
        "    when modified(readings) { return removed_value(readings) }\n",
        "}\n",
        "fn plain(readings: rolling_window<f64, 3>) -> rolling_window<f64, 3> {\n",
        "    when { return delta<f64?>() }\n",
        "}\n",
        "test traces {\n",
        "    assert eval arrived(readings: [1.0, null, _]) == [1.0, null]\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), ["m.hgl:15:19 type/type.mismatch EXPR-5"]);
    assert_eq!(returned(&checked, "arrived").ty.to_string(), "delta<f64?>");
    assert_eq!(
        returned(&checked, "left").ty.to_string(),
        "result<f64?, error>"
    );
}
