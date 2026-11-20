//! The analyser against the owner's decisions of 2026-11-20, written
//! into the pinned specification and core: the code of a wake's duration
//! the compiler sees (TIME-8), a test of `modified` as a guard (TIME-4,
//! TIME-5), a part of `out` of a `ref` result and a `signal` marked `ref`
//! (FN-16, TYPE-12), and a window read where a `result` is taken (ERR-14,
//! TYPE-11). Each test cites its rule.
//!
//! A finding is written `file:line:column category/code rule`.

mod common;

use common::module;

/// TIME-8: "A duration that is not admitted, a negative one in `start`
/// included, rejects where the compiler sees its value, a literal or a
/// constant (UNIT-13), with `schedule.duration_bounds`, and elsewhere
/// fails the call as the program runs, with
/// `schedule.non_positive_duration`." The catalogue's row: "A wake asked
/// for with a `duration` that is not more than zero, other than a zero in
/// `start`, whose value the compiler sees (TIME-8)". A literal and a
/// constant, of the scheduler and of the alarm, in a `when`, in `start`
/// and in `stop`, and a negative one in a value function given the
/// scheduler; a duration the compiler does not see is accepted.
#[test]
fn a_duration_the_compiler_sees_rejects_with_duration_bounds() {
    let checked = module(concat!(
        "const ZERO: duration = 0s\n",
        "fn in_when(value: i64, delay: duration) -> i64 {\n",
        "    inject scheduler\n",
        "    when modified(value) {\n",
        "        schedule(scheduler, 0s)\n",
        "        schedule(scheduler, ZERO)\n",
        "        schedule(scheduler, delay)\n",
        "        return value\n",
        "    }\n",
        "}\n",
        "fn in_start(value: i64) -> i64 {\n",
        "    inject alarm\n",
        "    start {\n",
        "        schedule(alarm, 0s)\n",
        "        schedule(alarm, -1s)\n",
        "    }\n",
        "    when { return value }\n",
        "    stop { schedule(alarm, ZERO) }\n",
        "}\n",
        "const fn helper(scheduler: Scheduler) {\n",
        "    schedule(scheduler, -1us)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:6:29 type/schedule.duration_bounds TIME-8",
            "m.hgl:7:29 type/schedule.duration_bounds TIME-8",
            "m.hgl:16:25 type/schedule.duration_bounds TIME-8",
            "m.hgl:19:28 type/schedule.duration_bounds TIME-8",
            "m.hgl:22:25 type/schedule.duration_bounds TIME-8",
        ]
    );
}

/// TIME-4: "A guard is a test of `valid(input)` or of `modified(input)`,
/// on the input by its name, that holds where the read stands: in the
/// branch it holds for, in the right operand of the `&&` it guards, and
/// after a branch that returns ... An input is not modified without
/// being valid, so `modified` of it guards what `valid` of it does;
/// `modified(a, b)`, which holds where either is, guards neither. A saved
/// bool proves nothing." And "A `when` header that has the test alone or
/// as a top-level conjunct guards its body, `when all_valid(quote)` and
/// `when modified(a) && valid([])`." TIME-5: `stop` reads an input under
/// a guard as a `when` does. `modified()`, of every input, guards none
/// either, nor does a disjunction.
#[test]
fn a_test_of_modified_guards_the_read_of_its_input() {
    let checked = module(concat!(
        "fn latest_a(a: f64, b: f64) -> f64 {\n",
        "    when modified(a) && valid([]) { return a }\n",
        "}\n",
        "fn forward(value: i64, probe: bool) -> i64 {\n",
        "    when valid([]) {\n",
        "        if modified(value) { return value }\n",
        "        if modified(value) && value > 0 { return value }\n",
        "        if !modified(value) { return -1 }\n",
        "        return value\n",
        "    }\n",
        "}\n",
        "fn maybe(probe: bool, value: i64?) -> i64? {\n",
        "    when valid([]) {\n",
        "        if modified(value) { return value }\n",
        "        return null\n",
        "    }\n",
        "}\n",
        "fn either(a: f64, b: f64) -> f64 {\n",
        "    when modified(a, b) && valid([]) { return a }\n",
        "}\n",
        "fn unproved(a: f64, b: f64) -> f64 {\n",
        "    when valid([]) {\n",
        "        if modified(a, b) { return b }\n",
        "        if modified() { return a }\n",
        "        let seen = modified(a)\n",
        "        if seen { return a }\n",
        "        if modified(a) || modified(b) { return a }\n",
        "        return 0.0\n",
        "    }\n",
        "}\n",
        "fn stopped(a: f64) -> f64 {\n",
        "    state last: f64 = 0.0\n",
        "    when { return a }\n",
        "    stop {\n",
        "        if modified(a) { last = a }\n",
        "        last = a\n",
        "    }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:20:47 phase/payload.unguarded TIME-4",
            "m.hgl:24:36 phase/payload.unguarded TIME-4",
            "m.hgl:25:32 phase/payload.unguarded TIME-4",
            "m.hgl:27:26 phase/payload.unguarded TIME-4",
            "m.hgl:28:48 phase/payload.unguarded TIME-4",
            "m.hgl:37:16 phase/payload.unguarded TIME-5",
        ]
    );
}

/// TIME-4: "A child's guard is a test of `all_valid` of its input, or of
/// `valid` or `modified` of the child as the read names it,
/// `valid(parts[1])` for `parts[1]` (TIME-1). The guard of a whole read
/// of a struct, a tuple or a fixed list, a child among them, is a test of
/// `all_valid` of what it reads ...; of a dictionary or an unsized list,
/// `valid` or `modified` of it, as of any input." `modified(parts[1])`
/// admits that child and no other; `modified` of a struct admits no
/// whole read of it. Whether `modified` of a collection admits a child,
/// as `valid` of it does not, the rule does not say: such a read is left
/// rejected, as under `valid` of it.
#[test]
fn a_test_of_modified_guards_a_child_as_the_read_names_it() {
    let checked = module(concat!(
        "struct Quote {\n",
        "    bid: f64\n",
        "    ask: f64\n",
        "}\n",
        "fn second(parts: list<i64, 2>, probe: bool) -> i64 {\n",
        "    when valid([]) {\n",
        "        if modified(parts[1]) { return parts[1] }\n",
        "        if modified(parts[1]) { return parts[0] }\n",
        "        if modified(parts) { return parts[1] }\n",
        "        return -1\n",
        "    }\n",
        "}\n",
        "fn bid(quote: Quote, probe: bool) -> f64 {\n",
        "    when valid([]) {\n",
        "        if modified((quote.bid)) { return quote.bid }\n",
        "        if modified(quote) { return quote.ask }\n",
        "        return 0.0\n",
        "    }\n",
        "}\n",
        "fn whole(quote: Quote, probe: bool) -> Quote {\n",
        "    when valid([]) {\n",
        "        if modified(quote) { return quote }\n",
        "    }\n",
        "}\n",
        "fn book(entries: dict<str, i64>, probe: bool) -> dict<str, i64> {\n",
        "    when valid([]) {\n",
        "        if modified(entries) { return entries }\n",
        "    }\n",
        "}\n",
        "fn items(xs: list<i64>, probe: bool) -> list<i64> {\n",
        "    when valid([]) {\n",
        "        if modified(xs) { return xs }\n",
        "    }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:9:40 phase/payload.unguarded TIME-1",
            "m.hgl:10:37 phase/payload.unguarded TIME-1",
            "m.hgl:17:37 phase/payload.unguarded TIME-1",
            "m.hgl:23:37 phase/payload.unguarded TIME-1",
        ]
    );
}

/// FN-16: "A `ref` result designates one endpoint whole: an assignment to
/// a part of its `out`, `out.bid = x` or `out[0] = x`, rejects.
/// Proposed: that assignment rejects with the code `binding.immutable`."
/// A field, a position, one known only when the program runs, one at
/// depth and a compound assignment are each such a part; `out =` of a
/// `ref` result designates, and a part of `out` of a result that is not
/// `ref` is assigned.
#[test]
fn a_part_of_out_of_a_ref_result_is_not_assigned() {
    let checked = module(concat!(
        "struct Quote {\n",
        "    bid: i64\n",
        "    ask: i64\n",
        "}\n",
        "struct Book {\n",
        "    top: Quote\n",
        "}\n",
        "fn designated_bid(quote: Quote) -> ref Quote {\n",
        "    inject out\n",
        "    when all_valid(quote) {\n",
        "        out.bid = quote.bid\n",
        "    }\n",
        "}\n",
        "fn firsts(items: list<i64, 3>, at: i64) -> ref list<i64, 3> {\n",
        "    inject out\n",
        "    when all_valid(items) {\n",
        "        out[0] = items[0]\n",
        "        out[1] += 1\n",
        "        out[at] = 2\n",
        "    }\n",
        "}\n",
        "fn deep(book: Book) -> ref Book {\n",
        "    inject out\n",
        "    when all_valid(book.top) { out.top.bid = book.top.bid }\n",
        "}\n",
        "fn whole(items: list<i64, 3>) -> ref i64 {\n",
        "    inject out\n",
        "    when { out = items[0] }\n",
        "}\n",
        "fn plain(quote: Quote) -> Quote {\n",
        "    inject out\n",
        "    when all_valid(quote) { out.bid = quote.bid }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:12:9 name/binding.immutable FN-16",
            "m.hgl:18:9 name/binding.immutable FN-16",
            "m.hgl:19:9 name/binding.immutable FN-16",
            "m.hgl:20:9 name/binding.immutable FN-16",
            "m.hgl:25:32 name/binding.immutable FN-16",
        ]
    );
}

/// FN-16: "A `signal` parameter is not marked either, and a mark on one
/// rejects: a signal has no payload to designate (TYPE-12) ...
/// Proposed: ... a mark on what is no node, or on a `signal` parameter,
/// with `ref.position`." TYPE-12: "A `signal` input takes no default but
/// `null`, and is not marked `ref` (FN-16)". Of a node, of a native
/// node's contract, and one that may be left out; a `signal` beside a
/// `ref` parameter of another type is accepted, and a value function's
/// mark is reported once.
#[test]
fn a_signal_parameter_is_not_marked_ref() {
    let checked = module(concat!(
        "fn counted_ticks(ref tick: signal) -> i64 {\n",
        "    state count: i64 = 0\n",
        "    when {\n",
        "        count += 1\n",
        "        return count\n",
        "    }\n",
        "}\n",
        "fn maybe_ticks(ref price: i64, ref tick: signal? = null) -> ref i64 {\n",
        "    when { return price }\n",
        "}\n",
        "native fn native_ticks(ref tick: signal) -> i64\n",
        "fn routed(ref price: i64, tick: signal) -> ref i64 {\n",
        "    when { return price }\n",
        "}\n",
        "const fn helper(ref tick: signal) -> i64 => 1\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:2:18 function-kind/ref.position FN-16",
            "m.hgl:9:32 function-kind/ref.position FN-16",
            "m.hgl:12:24 function-kind/ref.position FN-16",
            "m.hgl:16:17 function-kind/ref.position FN-16",
            "m.hgl:16:27 type/type.not_ordinary TYPE-12",
        ]
    );
}
