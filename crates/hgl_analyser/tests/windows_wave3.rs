//! What admits the read of a window's value, and where that read is a
//! `list<V>` (TYPE-11, TIME-4, TIME-5, TIME-1, END-9).

mod common;

use common::module;

/// TYPE-11: "A read of a window's value before it is `all_valid`
/// fails", a failure when it runs (`value.unset_read`). TIME-4: "A
/// payload read under manual admission must be dominated by a successful
/// guard", and TIME-5 holds `stop` to it: a read of a window's value that
/// nothing admits rejects (`payload.unguarded`), as any input's does. A
/// `valid` guard admits it, as of any input, and so does `all_valid`, in
/// the body and in the header; the default header requires the window
/// valid (TIME-3). TIME-1: a window that is a struct's field is a child,
/// read where `valid` of it or `all_valid` of the struct admits it.
#[test]
fn a_windows_value_is_read_where_it_is_admitted() {
    let checked = module(concat!(
        "const fn total(values: list<i64>) -> i64 => 0\n",
        "fn plain(held: rolling_window<i64, 3, 2>) -> i64 {\n",
        "    when { return total(held) }\n",
        "}\n",
        "fn manual(held: rolling_window<i64, 3, 2>, probe: bool) -> i64 {\n",
        "    when valid([]) { return total(held) }\n",
        "}\n",
        "fn by_valid(held: rolling_window<i64, 3, 2>, probe: bool) -> i64 {\n",
        "    when valid([]) {\n",
        "        if valid(held) { return total(held) }\n",
        "        return 0\n",
        "    }\n",
        "}\n",
        "fn by_all_valid(held: rolling_window<i64, 3, 2>, probe: bool) -> i64 {\n",
        "    when valid([]) {\n",
        "        if all_valid(held) { return total(held) }\n",
        "        return 0\n",
        "    }\n",
        "}\n",
        "fn in_header(held: rolling_window<f64, 3s, 0s>) -> list<f64> {\n",
        "    when all_valid(held) { return held }\n",
        "}\n",
        "fn stopped(held: rolling_window<i64, 3, 2>) -> i64 {\n",
        "    state last: i64 = 0\n",
        "    when { return 1 }\n",
        "    stop { last = total(held) }\n",
        "}\n",
        "fn stopped_guarded(held: rolling_window<i64, 3, 2>) -> i64 {\n",
        "    state last: i64 = 0\n",
        "    when { return 1 }\n",
        "    stop {\n",
        "        if valid(held) { last = total(held) }\n",
        "    }\n",
        "}\n",
        "struct Book {\n",
        "    prices: rolling_window<i64, 3, 2>\n",
        "    last: i64\n",
        "}\n",
        "fn field_unadmitted(book: Book) -> list<i64> {\n",
        "    when valid(book.last) { return book.prices }\n",
        "}\n",
        "fn field_by_valid(book: Book) -> list<i64> {\n",
        "    when valid(book.prices) { return book.prices }\n",
        "}\n",
        "fn field_by_all_valid(book: Book) -> list<i64> {\n",
        "    when all_valid(book) { return book.prices }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:7:35 phase/payload.unguarded TIME-4",
            "m.hgl:27:25 phase/payload.unguarded TIME-5",
            "m.hgl:41:36 phase/payload.unguarded TIME-1",
        ]
    );
}

/// TYPE-11: "That value, read in a hook ... is a `list<V>` of the values
/// held, oldest first": compared with a list, iterated, indexed, given to
/// a generic value function's `list<T>`, and of a generic window, whose
/// V is the instance's. It is no window: a window's value is not given
/// where a window is expected.
#[test]
fn a_windows_value_is_a_list_wherever_it_is_read() {
    let checked = module(concat!(
        "const fn count<T>(values: list<T>) -> i64 => 0\n",
        "fn compared(held: rolling_window<i64, 3, 2>) -> bool {\n",
        "    when all_valid(held) { return held == [1, 2] }\n",
        "}\n",
        "fn iterated(held: rolling_window<i64, 3, 2>) -> i64 {\n",
        "    when all_valid(held) {\n",
        "        var sum: i64 = 0\n",
        "        for value in held { sum += value }\n",
        "        return sum\n",
        "    }\n",
        "}\n",
        "fn indexed(held: rolling_window<i64, 3, 2>) -> i64 {\n",
        "    when all_valid(held) { return held[0] }\n",
        "}\n",
        "fn counted(held: rolling_window<str, 5s, 1s>) -> i64 {\n",
        "    when all_valid(held) { return count(held) }\n",
        "}\n",
        "fn generic<T, const N: i64>(held: rolling_window<T, N, 1>) -> list<T> {\n",
        "    when all_valid(held) { return held }\n",
        "}\n",
        "fn not_a_window(held: rolling_window<i64, 3, 2>) -> rolling_window<i64, 3, 2> {\n",
        "    inject out\n",
        "    when all_valid(held) { out = held }\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), ["m.hgl:24:34 type/type.mismatch EXPR-5"]);
}
