//! The analyser against the rules on a collection's child (TIME-1,
//! TIME-4, TIME-9, END-8, ERR-14), trace cells written whole or as a
//! delta (TST-5), and the list's delta (TYPE-16). Each test cites its
//! rule.
//!
//! A finding is written `file:line:column category/code rule`.

mod common;

use common::{Checked, body, check};
use hgl_analyser::hir::{Expr, ExprKind, Stmt};

/// No lines.
const NONE: [&str; 0] = [];

/// A core that declares the endpoint operations as TIME-9 has core
/// declare them, with the operators the tests' symbols need.
const CORE: &str = concat!(
    "module core\n",
    "export native const fn valid<T>(input: endpoint<T>) -> bool\n",
    "export native const fn all_valid<T>(input: endpoint<T>) -> bool\n",
    "export native const fn modified<T>(input: endpoint<T>) -> bool\n",
    "export native const fn delta_value<T>(input: endpoint<T>) -> T\n",
    "export native const fn last_modified<T>(input: endpoint<T>) -> datetime?\n",
    "export native const fn is_active<T>(input: endpoint<T>) -> bool\n",
    "operator add_<L, R, O>(lhs: L, rhs: R) -> O\n",
    "native impl const fn add_(lhs: i64, rhs: i64) -> i64\n",
    "operator neg_<T, O>(value: T) -> O\n",
    "native impl const fn neg_(value: i64) -> i64\n",
);

/// Checks the core above with one file, `m.hgl`, of module `m`. The
/// `module` line is added, so the text given starts on line 2.
fn module(text: &str) -> Checked {
    check(&[("core.hgl", CORE), ("m.hgl", &format!("module m\n{text}"))])
}

/// The value the first `return` of `name`'s first hook gives.
fn returned<'a>(checked: &'a Checked, name: &str) -> &'a Expr {
    let Some(Stmt::Hook { body, .. }) = body(checked, name)
        .statements
        .iter()
        .find(|statement| matches!(statement, Stmt::Hook { .. }))
    else {
        panic!("`{name}` has a hook");
    };
    body.iter()
        .find_map(|statement| match statement {
            Stmt::Return {
                value: Some(value), ..
            } => Some(value),
            _ => None,
        })
        .expect("the hook returns a value")
}

/// TIME-1: "In a hook, a read of a child's value, or of its
/// `delta_value`, requires that child's admission: `all_valid(parts)`
/// admits every child, `valid(parts[1])` or `valid(quote.bid)` that
/// one, and `valid(parts)` none (TIME-4). Where the read names its child
/// when the program is compiled, at a constant index or a field, one
/// that is not admitted rejects (`payload.unguarded`)." Core's two
/// rejection cases, and the language's `ask_or_zero` without its `if`.
#[test]
fn a_child_named_when_compiled_is_read_where_it_is_admitted() {
    let checked = module(concat!(
        "struct Quote { bid: i64\n ask: i64 }\n",
        "fn second(pair: list<i64, 2>) -> i64 {\n",
        "    when { return pair[1] }\n",
        "}\n",
        "fn bid(quote: Quote) -> i64 {\n",
        "    when { return quote.bid }\n",
        "}\n",
        "fn guarded(pair: list<i64, 2>, quote: Quote) -> i64 {\n",
        "    when {\n",
        "        if valid(pair[1]) && valid(quote.ask) { return pair[1] + quote.ask }\n",
        "        if valid(pair) { return pair[0] }\n",
        "        return 0\n",
        "    }\n",
        "}\n",
        "fn admitted(pair: list<i64, 2>) -> i64 {\n",
        "    when all_valid(pair) { return pair[0] + pair[1] }\n",
        "}\n",
        "fn changed(pair: list<i64, 2>) -> i64 {\n",
        "    when { return delta_value(pair[1]) }\n",
        "}\n",
        "fn keyed(book: dict<str, i64>) -> i64 {\n",
        "    when { return book[\"bid\"] }\n",
        "}\n",
        "fn ended(pair: list<i64, 2>) -> i64 {\n",
        "    inject out\n",
        "    when { return 1 }\n",
        "    stop { if valid(pair[0]) { let last = pair[0] } }\n",
        "}\n",
        "fn nested(grid: list<list<i64, 2>, 2>) -> i64 {\n",
        "    when all_valid(grid) { return grid[0][1] }\n",
        "}\n",
        "fn deeper(grid: list<list<i64, 2>, 2>) -> i64 {\n",
        "    when all_valid(grid[0]) { return grid[0][1] }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:5:19 phase/payload.unguarded TIME-1",
            "m.hgl:8:19 phase/payload.unguarded TIME-1",
            // `valid(pair)` admits no child.
            "m.hgl:13:33 phase/payload.unguarded TIME-1",
            "m.hgl:21:31 phase/payload.unguarded TIME-1",
            "m.hgl:24:19 phase/payload.unguarded TIME-1",
            // `all_valid(grid)` is one level deep.
            "m.hgl:32:35 phase/payload.unguarded TIME-1",
        ]
    );
}

/// TIME-1: "Elsewhere, `parts[i]`, it is checked when it runs". ERR-14:
/// "Where the place expects `result<T, error>`, T the child's type, it
/// gives `ok` of the child's value, or, for a child that is not valid,
/// `err`". EXPR-5: the place contextualizes "a collection's child read
/// where that is checked when it runs". The language's `at` and
/// `checked_at`.
#[test]
fn a_child_known_when_it_runs_is_checked_then() {
    let checked = module(concat!(
        "fn at(parts: list<i64, 2>, slot: i64) -> i64 {\n",
        "    when { return parts[slot] }\n",
        "}\n",
        "fn checked_at(parts: list<i64, 2>, slot: i64) -> result<i64, error> {\n",
        "    when { return parts[slot] }\n",
        "}\n",
        "fn constant(parts: list<i64, 2>) -> result<i64, error> {\n",
        "    when all_valid(parts) { return parts[0] }\n",
        "}\n",
    ));
    // A child read at a constant index is no read checked when it runs,
    // and its place does not make it a result.
    assert_eq!(checked.summary(), ["m.hgl:9:36 type/type.mismatch EXPR-5"]);
    let at = returned(&checked, "at");
    assert!(
        matches!(at.kind, ExprKind::CheckedChild { result: false, .. }),
        "{at:?}"
    );
    assert_eq!(at.ty.to_string(), "i64");
    let checked_at = returned(&checked, "checked_at");
    assert!(
        matches!(checked_at.kind, ExprKind::CheckedChild { result: true, .. }),
        "{checked_at:?}"
    );
    assert_eq!(checked_at.ty.to_string(), "result<i64, error>");
}

/// TIME-1: "A child of a collection (TIME-9), an element, a field or an
/// entry, is an endpoint whether or not it is valid". END-8: "`valid`,
/// `modified`, `delta_value` and `last_modified` take it, and
/// `all_valid` where it is a collection". TIME-9: `is_active` operates
/// "on direct temporal inputs"; an `atomic<V>`'s field is computed from
/// the whole value and is no child; `start` gives no input to an
/// endpoint operation (TIME-5).
#[test]
fn a_child_is_given_to_an_endpoint_operation() {
    let checked = module(concat!(
        "struct Quote { bid: i64\n ask: i64 }\n",
        "fn seen(parts: list<i64, 2>, quote: Quote, slot: i64) -> bool {\n",
        "    when {\n",
        "        let first = valid(parts[1]) && modified(quote.bid)\n",
        "        let later = last_modified(parts[slot]) != null\n",
        "        return first && later && valid(parts[slot])\n",
        "    }\n",
        "}\n",
        "fn wrong(parts: list<i64, 2>, whole: atomic<Quote>, rows: list<list<i64, 2>, 2>) -> bool {\n",
        "    when {\n",
        "        let one = is_active(parts[0])\n",
        "        let two = valid(whole.bid)\n",
        "        let three = all_valid(parts[0])\n",
        "        return all_valid(rows[0])\n",
        "    }\n",
        "}\n",
        "fn started(parts: list<i64, 2>) -> i64 {\n",
        "    inject out\n",
        "    start { let early = valid(parts[0]) }\n",
        "    when { return 1 }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:13:29 phase/endpoint.argument TIME-9",
            "m.hgl:14:25 phase/endpoint.argument TIME-9",
            "m.hgl:15:31 phase/endpoint.argument TIME-9",
            "m.hgl:21:25 phase/input.lifecycle TIME-5",
        ]
    );
    let Some(Stmt::Hook { body: hook, .. }) = body(&checked, "seen").statements.first() else {
        panic!("`seen` begins with its `when`");
    };
    let Some(Stmt::Declare {
        value: Some(first), ..
    }) = hook.first()
    else {
        panic!("the `when` begins with a `let`");
    };
    let ExprKind::Binary { left, .. } = &first.kind else {
        panic!("`&&` of two tests");
    };
    assert!(
        matches!(left.kind, ExprKind::ChildEndpoint { .. }),
        "{left:?}"
    );
}

/// TIME-4: "a `when` header that has the test alone or as a top-level
/// conjunct guards its body, `when all_valid(quote)`". A test of a
/// child's `valid` in a header guards its body too, and names no input:
/// the default admission stands with it (TIME-3).
#[test]
fn a_header_test_of_a_child_guards_the_body() {
    let checked = module(concat!(
        "struct Quote { bid: i64\n ask: i64 }\n",
        "fn spread(quote: Quote) -> i64 {\n",
        "    when all_valid(quote) { return quote.ask - quote.bid }\n",
        "}\n",
        "fn bid(quote: Quote, other: i64) -> i64 {\n",
        "    when valid(quote.bid) { return quote.bid + other }\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE);
}

/// TST-5: "Each cell is one cycle's publication, written as a `return`
/// is (TIME-7) ... a T, published whole, or a `delta<T>` (TYPE-16) ...
/// A list, a tuple, a struct and a dictionary are each written either
/// way". The language's `whole_and_delta_cells`.
#[test]
fn a_trace_cell_is_written_whole_or_as_a_delta() {
    let checked = module(concat!(
        "struct Quote { bid: f64\n ask: f64 }\n",
        "fn relayed(quote: Quote) -> Quote {\n",
        "    when all_valid(quote) { return quote }\n",
        "}\n",
        "fn book(prices: dict<str, i64>) -> dict<str, i64> {\n",
        "    when all_valid(prices) { return prices }\n",
        "}\n",
        "fn members(values: set<i64>) -> set<i64> {\n",
        "    when { return values }\n",
        "}\n",
        "test whole_and_delta_cells {\n",
        "    assert eval relayed(quote: [\n",
        "        Quote(bid: 1.0, ask: 2.0),\n",
        "        delta<Quote>(ask: 3.0),\n",
        "    ]) == [Quote(bid: 1.0, ask: 2.0), Quote(bid: 1.0, ask: 3.0)]\n",
        "    assert eval book(prices: [[\"a\": 1], delta<dict<str, i64>>(removed: [\"a\"])]) ==\n",
        "        [[\"a\": 1], [\"a\": 1, \"b\": 2]]\n",
        "    assert eval members(values: [[1, 2], delta<set<i64>>(added: [3])]) == [[1, 2], [1, 2, 3]]\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE);
}

/// TYPE-16: "A `list<V, N>` keeps its bounds: a constant entry past
/// them rejects (`delta.index_bounds`)"; the catalogue's row is
/// "Negative/out-of-fixed-range index". A negative `size_modified`
/// "reduces the list by that count", and an entry past a growing list's
/// end extends it: neither rejects.
#[test]
fn a_constant_position_of_a_list_delta_is_within_its_bounds() {
    let checked = module(concat!(
        "const fn past() -> delta<list<i64, 2>> => delta<list<i64, 2>>(updated: [2: 1])\n",
        "const fn below() -> delta<list<i64>> => delta<list<i64>>(updated: [-1: 1])\n",
        "const fn gone() -> delta<list<i64, 2>> => delta<list<i64, 2>>(removed: [5])\n",
        "const fn within() -> delta<list<i64, 2>> => delta<list<i64, 2>>(updated: [1: 1])\n",
        "const fn longer() -> delta<list<i64>> => delta<list<i64>>(updated: [7: 1])\n",
        "const fn shorter() -> delta<list<i64>> => delta<list<i64>>(size_modified: -3)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:2:73 type/delta.index_bounds TYPE-16",
            "m.hgl:3:68 type/delta.index_bounds TYPE-16",
            "m.hgl:4:73 type/delta.index_bounds TYPE-16",
        ]
    );
}

/// TYPE-16: "A delta that names one member, key or position both as
/// added or updated and as removed is malformed. A constant one is
/// rejected (`delta.overlap`)": a constant compared by its value, a
/// negative integer, an enum's member, and an `i64` widened to an `f64`
/// key against the `f64` written.
#[test]
fn an_overlap_is_found_by_the_constants_value() {
    let checked = module(concat!(
        "enum Side { buy, sell }\n",
        "const fn negative() -> delta<set<i64>> => delta<set<i64>>(added: [-1], removed: [-1])\n",
        "const fn member() -> delta<set<Side>> => delta<set<Side>>(added: [.buy], removed: [.buy])\n",
        "const fn widened() -> delta<dict<f64, i64>> => delta<dict<f64, i64>>(updated: [1: 1], removed: [1.0])\n",
        "const fn apart() -> delta<set<Side>> => delta<set<Side>>(added: [.buy], removed: [.sell])\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:82 type/delta.overlap TYPE-16",
            "m.hgl:4:84 type/delta.overlap TYPE-16",
            "m.hgl:5:97 type/delta.overlap TYPE-16",
        ]
    );
}

/// TIME-1: "Where the read names its child when the program is
/// compiled ... one that is not admitted rejects". A constant of a
/// module names its child as the literal it is written as does
/// (UNIT-13: "The constant has that one value wherever it is read").
#[test]
fn a_constant_of_a_module_names_a_child_when_compiled() {
    let checked = module(concat!(
        "const KEY: str = \"bid\"\n",
        "fn unguarded(book: dict<str, i64>) -> i64 {\n",
        "    when { return book[KEY] }\n",
        "}\n",
        "fn guarded(book: dict<str, i64>) -> i64 {\n",
        "    when {\n",
        "        if valid(book[\"bid\"]) { return book[KEY] }\n",
        "        return 0\n",
        "    }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:4:19 phase/payload.unguarded TIME-1"]
    );
}

/// EXPR-1: "Parentheses select grouping": a child named in parentheses
/// is the child named without them, as an endpoint and as a read
/// (TIME-1).
#[test]
fn a_child_in_parentheses_is_the_same_child() {
    let checked = module(concat!(
        "fn grouped(parts: list<i64, 2>, grid: list<list<i64, 2>, 2>) -> i64 {\n",
        "    when {\n",
        "        if valid((parts)[1]) && valid((grid[0])[1]) { return (parts)[1] + (grid[0])[1] }\n",
        "        return 0\n",
        "    }\n",
        "}\n",
        "fn unguarded(parts: list<i64, 2>) -> i64 {\n",
        "    when { return (parts)[1] }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:9:19 phase/payload.unguarded TIME-1"]
    );
}

/// EXPR-1: "Parentheses select grouping", in a `when` header as
/// elsewhere: `valid((quote.bid))` is a guard of a child, as
/// `valid(quote.bid)` is, and names no input, so the default admission
/// stands; `valid((other))` names `other`.
#[test]
fn grouping_never_changes_an_endpoint_argument() {
    let checked = module(concat!(
        "struct Quote { bid: i64\n ask: i64 }\n",
        "fn bid(quote: Quote, other: i64) -> i64 {\n",
        "    when valid((quote.bid)) { return quote.bid + other }\n",
        "}\n",
        "fn named(quote: Quote, other: i64) -> i64 {\n",
        "    when valid((other)) { return other }\n",
        "}\n",
        "fn whole(quote: Quote) -> bool {\n",
        "    when all_valid((quote)) { return valid((quote.ask)) && modified((quote)) }\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE);
}

/// TIME-1: a child named by a constant key is the one its value names:
/// an `i64` widened to an `f64` key is the `f64` of the same number
/// (EXPR-5, NUM-1), so `book[1]` and `book[1.0]` are one child, and an
/// unguarded `book[1]` is named when compiled.
#[test]
fn a_widened_key_names_the_child_its_value_does() {
    let checked = module(concat!(
        "fn guarded(book: dict<f64, i64>) -> i64 {\n",
        "    when {\n",
        "        if valid(book[1]) { return book[1.0] }\n",
        "        return 0\n",
        "    }\n",
        "}\n",
        "fn unguarded(book: dict<f64, i64>) -> i64 {\n",
        "    when { return book[1] }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:9:19 phase/payload.unguarded TIME-1"]
    );
}
