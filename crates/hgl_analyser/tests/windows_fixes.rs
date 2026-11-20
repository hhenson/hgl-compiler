//! A window given to a value function's generic parameter, and a window
//! that is a child of `out`, read as the `list<V>` it holds (TYPE-11,
//! FN-1, FN-12, GEN-1, GEN-11, TIME-7).

mod common;

use common::{locals, module};

/// The window, an input of a graph, and value functions generic in what
/// they take.
const WINDOWS: &str = concat!(
    "fn readings(reading: i64) -> rolling_window<i64, 3, 2> {\n",
    "    when { return reading }\n",
    "}\n",
    "const fn ident<T>(value: T) -> T => value\n",
    "const fn same<T>(a: T, b: T) -> bool => a == b\n",
    "const fn one<T>(value: T) -> i64 => 1\n",
    "const fn wrap<T>(value: T) -> list<T> => [value]\n",
);

/// FN-1: a value function's "parameter and result types are ordinary
/// value types", and GEN-1: "ordinary positions require value types",
/// which a window is not (TYPE-12). TYPE-11: a window's value "given to
/// a lifted helper (FN-12), is a `list<V>` of the values held". So a
/// value function's `T` given a window binds to that list: `ident` of a
/// window is a `list<i64>`, and `same` compares two lists.
#[test]
fn a_value_functions_generic_given_a_window_is_its_list() {
    let checked = module(&format!(
        "{WINDOWS}{}",
        concat!(
            "fn listed(reading: i64) -> list<i64> => ident(readings(reading))\n",
            "fn compared(a: i64, b: i64) -> bool => same(readings(a), readings(b))\n",
            "fn counted(a: i64) -> i64 => one(readings(a))\n",
            "fn wrapped(a: i64) -> list<list<i64>> => wrap(readings(a))\n",
            "fn bound(reading: i64) -> list<i64> {\n",
            "    let held = ident(readings(reading))\n",
            "    return held\n",
            "}\n",
        )
    ));
    assert_eq!(checked.summary(), Vec::<String>::new());
    assert_eq!(
        locals(&checked, "bound"),
        ["reading: i64", "held: list<i64>"]
    );
}

/// Where the place, or what the call writes, binds a value function's
/// `T` to a window, or to a type that holds one, the call rejects: no
/// value function takes or gives a window (FN-1, `type.not_ordinary`).
#[test]
fn a_value_functions_generic_bound_to_a_window_rejects() {
    let checked = module(&format!(
        "{WINDOWS}{}",
        concat!(
            "fn as_window(reading: i64) -> rolling_window<i64, 3, 2> => ident(readings(reading))\n",
            "fn wrapped(a: i64) -> list<rolling_window<i64, 3, 2>> => wrap(readings(a))\n",
            "fn written(a: i64) -> i64 => one<rolling_window<i64, 3, 2>>(readings(a))\n",
        )
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:9:60 type/type.not_ordinary FN-1",
            "m.hgl:10:58 type/type.not_ordinary FN-1",
            "m.hgl:11:30 type/type.not_ordinary FN-1",
        ]
    );
}

/// TIME-7: "`inject out` exposes that same result for reads", and
/// TYPE-11: a window's value, "read in a hook ... is a `list<V>`". A
/// window that is a child of `out`, a field, an element, a field of an
/// element or an entry, is read as its list, as an input's child is.
/// The list read is indexed as any list is: a window is no collection
/// (TIME-9), so `book.prices[0]` is no child of it, and `valid` of the
/// window admits it (TIME-1).
#[test]
fn a_window_child_of_out_is_read_as_its_list() {
    let checked = module(concat!(
        "const fn total(values: list<i64>) -> i64 => 0\n",
        "struct Book {\n",
        "    prices: rolling_window<i64, 3, 2>\n",
        "    last: i64\n",
        "}\n",
        "fn field(price: i64) -> Book {\n",
        "    inject out\n",
        "    when {\n",
        "        if all_valid(out) && total(out.prices) > 5 { return }\n",
        "        if all_valid(out) {\n",
        "            let held: list<i64> = out.prices\n",
        "            var sum: i64 = 0\n",
        "            for value in out.prices { sum += value }\n",
        "            if sum > held[0] + out.prices[1] { return }\n",
        "        }\n",
        "        return delta<Book>(prices: delta<rolling_window<i64, 3, 2>>(added: price), last: price)\n",
        "    }\n",
        "}\n",
        "fn element(slot: i64, price: i64) -> list<rolling_window<i64, 2>, 2> {\n",
        "    inject out\n",
        "    when {\n",
        "        if all_valid(out) && total(out[0]) > 5 { return }\n",
        "        if total(out[slot]) > 5 { return }\n",
        "        return delta<list<rolling_window<i64, 2>, 2>>(\n",
        "            updated: [slot: delta<rolling_window<i64, 2>>(added: price)],\n",
        "        )\n",
        "    }\n",
        "}\n",
        "fn element_field(slot: i64, price: i64) -> list<Book, 2> {\n",
        "    inject out\n",
        "    when {\n",
        "        if all_valid(out) && total(out[0].prices) > 5 { return }\n",
        "        return delta<list<Book, 2>>(\n",
        "            updated: [slot: delta<Book>(prices: delta<rolling_window<i64, 3, 2>>(added: price))],\n",
        "        )\n",
        "    }\n",
        "}\n",
        "fn entry(key: str, price: i64) -> dict<str, rolling_window<i64, 2>> {\n",
        "    inject out\n",
        "    when {\n",
        "        if total(out[key]) > 5 { return }\n",
        "        return delta<dict<str, rolling_window<i64, 2>>>(\n",
        "            updated: [key: delta<rolling_window<i64, 2>>(added: price)],\n",
        "        )\n",
        "    }\n",
        "}\n",
        "fn input_index(book: Book) -> i64 {\n",
        "    when valid(book.prices) { return book.prices[0] + total(book.prices) }\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), Vec::<String>::new());
}

/// FN-1 has two exceptions to its ordinary parameters, among them "in a
/// native value function, one of `endpoint<T>` (TIME-9)", which takes
/// the endpoint and not its payload. A generic that stands only in such
/// a parameter may stand for a window: core's `is_active` of a window
/// input, and a program's native function of a window's endpoint.
#[test]
fn an_endpoints_generic_may_stand_for_a_window() {
    let checked = module(concat!(
        "native const fn peek<T>(input: endpoint<T>) -> bool\n",
        "fn active(prices: rolling_window<i64, 3>) -> bool {\n",
        "    when { return is_active(prices) }\n",
        "}\n",
        "fn peeked(prices: rolling_window<i64, 3>) -> bool {\n",
        "    when { return peek(prices) }\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), Vec::<String>::new());
}
