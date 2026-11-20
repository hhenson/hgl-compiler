//! The analyser against the whole read of a struct, a tuple and a fixed
//! list (TIME-1, TIME-4, TIME-5, TIME-9): admitted where its `all_valid`
//! holds, and rejected (`payload.unguarded`) where the read is seen when
//! the program is compiled and nothing admits it. Each test cites its
//! rule.
//!
//! A finding is written `file:line:column category/code rule`.

mod common;

use common::{Checked, check};

/// No lines.
const NONE: [&str; 0] = [];

/// A core that declares the endpoint operations as TIME-9 has core
/// declare them, with the operators the tests' symbols need.
const CORE: &str = concat!(
    "module core\n",
    "export native const fn valid<T>(input: endpoint<T>) -> bool\n",
    "export native const fn all_valid<T>(input: endpoint<T>) -> bool\n",
    "export native const fn modified<T>(input: endpoint<T>) -> bool\n",
    "operator add_<L, R, O>(lhs: L, rhs: R) -> O\n",
    "native impl const fn add_(lhs: i64, rhs: i64) -> i64\n",
    "operator gt_<L, R, O>(lhs: L, rhs: R) -> O\n",
    "native impl const fn gt_(lhs: i64, rhs: i64) -> bool\n",
);

/// Checks the core above with one file, `m.hgl`, of module `m`. The
/// `module` line is added, so the text given starts on line 2.
fn module(text: &str) -> Checked {
    check(&[("core.hgl", CORE), ("m.hgl", &format!("module m\n{text}"))])
}

/// TIME-1: "A read of a struct, a tuple or a fixed list whole, `return
/// quote`, requires `all_valid` of it ... Where such a read is seen when
/// the program is compiled and `all_valid` does not admit it, it rejects
/// (`payload.unguarded`)." TIME-9: `valid` of a collection "admits no
/// child's read". Core's rejection case `quote_without_admission`, and
/// the language's `when { return quote }`.
#[test]
fn a_struct_a_tuple_and_a_fixed_list_are_read_whole_where_all_valid() {
    let checked = module(concat!(
        "struct Quote { bid: i64\n ask: i64 }\n",
        "fn relayed(quote: Quote) -> Quote {\n",
        "    when { return quote }\n",
        "}\n",
        "fn selected(quote: Quote) -> Quote {\n",
        "    when valid(quote) { return quote }\n",
        "}\n",
        "fn admitted(quote: Quote) -> Quote {\n",
        "    when all_valid(quote) { return quote }\n",
        "}\n",
        "fn pair_of(pair: tuple<i64, i64>) -> tuple<i64, i64> {\n",
        "    when { return pair }\n",
        "}\n",
        "fn slots(values: list<i64, 2>) -> list<i64, 2> {\n",
        "    when { return values }\n",
        "}\n",
        "fn pair_admitted(pair: tuple<i64, i64>, values: list<i64, 2>) -> list<i64, 2> {\n",
        "    when all_valid(pair) && all_valid(values) {\n",
        "        let held = pair\n",
        "        return values\n",
        "    }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:5:19 phase/payload.unguarded TIME-1",
            // `valid(quote)` is true once any field is.
            "m.hgl:8:32 phase/payload.unguarded TIME-1",
            "m.hgl:14:19 phase/payload.unguarded TIME-1",
            "m.hgl:17:19 phase/payload.unguarded TIME-1",
        ]
    );
}

/// TIME-1: "A dictionary and an unsized list hold what they hold: a read
/// of one whole requires its `valid` and no more." TIME-9: "`atomic<V>`
/// stops expansion", so an atomic struct is no collection.
#[test]
fn a_dictionary_an_unsized_list_and_an_atomic_are_read_whole_where_valid() {
    let checked = module(concat!(
        "struct Quote { bid: i64\n ask: i64 }\n",
        "fn book(prices: dict<str, i64>) -> dict<str, i64> {\n",
        "    when { return prices }\n",
        "}\n",
        "fn readings(values: list<i64>) -> list<i64> {\n",
        "    when { return values }\n",
        "}\n",
        "fn snapshot(quote: atomic<Quote>) -> atomic<Quote> {\n",
        "    when { return quote }\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE);
}

/// TIME-4: "The guard of a whole read of a struct, a tuple or a fixed
/// list ... is a test of `all_valid` of what it reads ... A `when`
/// header that has the test alone or as a top-level conjunct guards its
/// body". A guard "holds where the read stands: in the branch it holds
/// for, in the right operand of the `&&` it guards, and after a branch
/// that returns".
#[test]
fn a_test_of_all_valid_guards_a_whole_read() {
    let checked = module(concat!(
        "struct Quote { bid: i64\n ask: i64 }\n",
        "const fn spread(quote: Quote) -> i64 => quote.ask + quote.bid\n",
        "fn branch(quote: Quote) -> i64 {\n",
        "    when valid([]) {\n",
        "        if all_valid(quote) { return spread(quote) }\n",
        "        return 0\n",
        "    }\n",
        "}\n",
        "fn after(quote: Quote) -> i64 {\n",
        "    when {\n",
        "        if !all_valid(quote) { return 0 }\n",
        "        return spread(quote)\n",
        "    }\n",
        "}\n",
        "fn operand(quote: Quote) -> bool {\n",
        "    when { return all_valid(quote) && spread(quote) > 0 }\n",
        "}\n",
        "fn conjunct(probe: bool, quote: Quote) -> i64 {\n",
        "    when modified(probe) && all_valid(quote) { return spread(quote) }\n",
        "}\n",
        "fn header(quote: Quote) -> bool {\n",
        "    when all_valid(quote) && spread(quote) > 0 { return true }\n",
        "}\n",
        "fn unguarded(quote: Quote) -> i64 {\n",
        "    when valid([]) {\n",
        "        if valid(quote) { return spread(quote) }\n",
        "        return 0\n",
        "    }\n",
        "}\n",
        "fn saved(quote: Quote) -> i64 {\n",
        "    when {\n",
        "        let ready = all_valid(quote)\n",
        "        if ready { return spread(quote) }\n",
        "        return 0\n",
        "    }\n",
        "}\n",
        "fn other(quote: Quote, second: Quote) -> i64 {\n",
        "    when all_valid(quote) { return spread(quote) + spread(second) }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:28:41 phase/payload.unguarded TIME-1",
            // TIME-4: "A saved bool proves nothing."
            "m.hgl:35:34 phase/payload.unguarded TIME-1",
            "m.hgl:40:59 phase/payload.unguarded TIME-1",
        ]
    );
}

/// TIME-1: "So does one of a child that is a struct, a tuple or a fixed
/// list: `return rows[0]`, for `rows: list<Quote, 2>`, requires
/// `all_valid(rows[0])`, which `all_valid(rows)`, one level deep, does
/// not give ... Elsewhere, `rows[i]` read whole, it is checked when it
/// runs". TIME-4: "`all_valid(rows[0])` for `rows[0]`".
#[test]
fn a_child_read_whole_needs_its_own_all_valid() {
    let checked = module(concat!(
        "struct Quote { bid: i64\n ask: i64 }\n",
        "struct Outer { inner: Quote\n tag: i64 }\n",
        "fn first(rows: list<Quote, 2>) -> Quote {\n",
        "    when all_valid(rows) { return rows[0] }\n",
        "}\n",
        "fn first_valid(rows: list<Quote, 2>) -> Quote {\n",
        "    when valid(rows[0]) { return rows[0] }\n",
        "}\n",
        "fn first_admitted(rows: list<Quote, 2>) -> Quote {\n",
        "    when all_valid(rows[0]) { return rows[0] }\n",
        "}\n",
        "fn at(rows: list<Quote, 2>, slot: i64) -> Quote {\n",
        "    when { return rows[slot] }\n",
        "}\n",
        "fn inner(outer: Outer) -> Quote {\n",
        "    when all_valid(outer) { return outer.inner }\n",
        "}\n",
        "fn inner_admitted(outer: Outer) -> Quote {\n",
        "    when all_valid(outer.inner) { return outer.inner }\n",
        "}\n",
        "fn inner_field(outer: Outer) -> i64 {\n",
        "    when all_valid(outer.inner) { return outer.inner.bid + outer.tag }\n",
        "}\n",
        "fn grid(cells: list<list<i64, 2>, 2>) -> list<i64, 2> {\n",
        "    when all_valid(cells) { return cells[1] }\n",
        "}\n",
        "fn grid_admitted(cells: list<list<i64, 2>, 2>) -> list<i64, 2> {\n",
        "    when all_valid(cells[1]) { return cells[1] }\n",
        "}\n",
        "fn pair(parts: tuple<i64, tuple<i64, i64>>) -> tuple<i64, i64> {\n",
        "    when all_valid(parts) { return parts[1] }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:7:35 phase/payload.unguarded TIME-1",
            "m.hgl:10:34 phase/payload.unguarded TIME-1",
            "m.hgl:19:36 phase/payload.unguarded TIME-1",
            // `outer.tag` is not admitted by `all_valid(outer.inner)`.
            "m.hgl:25:60 phase/payload.unguarded TIME-1",
            "m.hgl:28:36 phase/payload.unguarded TIME-1",
            "m.hgl:34:36 phase/payload.unguarded TIME-1",
        ]
    );
}

/// TIME-5: "`stop` reads a temporal input under manual admission, as a
/// `when` under `valid([])` does: no header stands over it, so each
/// payload read is dominated by a guard (TIME-4)", the guard of a whole
/// read being `all_valid` of what it reads.
#[test]
fn stop_reads_a_struct_whole_under_all_valid() {
    let checked = module(concat!(
        "struct Quote { bid: i64\n ask: i64 }\n",
        "fn last_seen(quote: Quote) -> i64 {\n",
        "    state last: Quote = Quote(bid: 0, ask: 0)\n",
        "    when all_valid(quote) { return quote.bid }\n",
        "    stop {\n",
        "        if valid(quote) { last = quote }\n",
        "        if all_valid(quote) { last = quote }\n",
        "    }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:8:34 phase/payload.unguarded TIME-1"]
    );
}

/// TIME-9: an optional child "is not asked", and TIME-1: "In the value
/// it reads, an optional child (TIME-9) that is not valid is null". The
/// language's `labelled` and `noted`: `all_valid(order)` admits the
/// order whole; `when { return label(order) }` rejects.
#[test]
fn a_struct_with_an_optional_field_is_read_whole_under_all_valid() {
    let checked = module(concat!(
        "struct Order { price: i64\n note: str? }\n",
        "const fn label(order: Order) -> str {\n",
        "    let note = order.note\n",
        "    if note == null { return \"plain\" }\n",
        "    return note\n",
        "}\n",
        "fn labelled(order: Order) -> str {\n",
        "    when all_valid(order) { return label(order) }\n",
        "}\n",
        "fn unlabelled(order: Order) -> str {\n",
        "    when { return label(order) }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:13:25 phase/payload.unguarded TIME-1"]
    );
}
