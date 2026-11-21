//! What a `when` header admits of its own reads, however many handlers
//! stand before it (TIME-3, TIME-4), and where `scheduled()` and the
//! other endpoint operations may be written, in an anonymous function
//! among the places (TIME-5, TIME-9, FN-9, FN-14, SOURCE-1).

mod common;

use common::{Checked, check};

/// Checks one file, `m.hgl`, of module `m`. The `module` line is added,
/// so the text given starts on line 2.
fn module(text: &str) -> Checked {
    check(&[("m.hgl", &format!("module m\n{text}"))])
}

/// A node of `count` handlers, each reading `b` in its header under its
/// own `valid(b)`, and after them one whose header reads `a`, which it
/// does not admit, and one that admits what it reads.
fn many_handlers(count: usize) -> String {
    let mut text = String::from("fn many(a: i64, b: i64) -> i64 {\n");
    for value in 0..count {
        text.push_str(&format!(
            "    when valid(b) && b == {value} {{ return b }}\n"
        ));
    }
    text.push_str("    when valid(b) && a > 0 { return b }\n");
    text.push_str("    when valid(a) && a > 0 { return a }\n");
    text.push_str("}\n");
    text
}

/// TIME-4: "A payload read under manual admission must be dominated by
/// a successful guard", and TIME-3: "`valid(a, b)` requires both named
/// inputs ... A selector sets subscription or admission when it stands
/// alone in the header or appears as a top-level conjunct". So
/// `when valid(b) && a > 0` admits `b` alone, and its read of `a` is
/// unguarded, however many handlers the node has before it.
fn header_reads_are_judged_after(count: usize) {
    let checked = module(&many_handlers(count));
    assert_eq!(
        checked.summary(),
        [format!(
            "m.hgl:{}:22 phase/payload.unguarded TIME-4",
            count + 3
        )]
    );
}

/// See [`header_reads_are_judged_after`]: 1,000 handlers.
#[test]
fn a_header_is_judged_after_a_thousand_handlers() {
    header_reads_are_judged_after(1_000);
}

/// See [`header_reads_are_judged_after`]: 5,000 handlers.
#[test]
fn a_header_is_judged_after_five_thousand_handlers() {
    header_reads_are_judged_after(5_000);
}

/// TIME-9: "`scheduled()` is the handler selector for a wake by the
/// node's scheduler or alarm ... A node that injects neither rejects it
/// (`inject.no_scheduler`), and so do `start` and `stop`
/// (`scheduled.lifecycle`)." FN-14: "An anonymous function's scope is
/// the scope around it plus its parameters", so the selector in one is
/// judged where the function is written: in a hook or an initializer of
/// a node that injects neither, nested or twice; in `start` and `stop`;
/// in a source, which injects "not `out`, `scheduler`, or `alarm`"
/// (SOURCE-1); and in a value function, a graph and a test, which are no
/// node: FN-9, "A value function injects none", and TIME-9, it "has no
/// output or schedule of its own" (`scheduled.outside_node`).
/// A `return` in the function is the function's,
/// and is no publication of the node's (TIME-7): `returned` publishes an
/// `i64`, and its function's `bool` is no mismatch.
#[test]
fn scheduled_in_an_anonymous_function_is_judged_where_it_is_written() {
    let checked = module(concat!(
        "fn neither(value: i64) -> bool {\n",
        "    when { return (fn() => scheduled())() }\n",
        "}\n",
        "fn nested(value: i64) -> bool {\n",
        "    when { return (fn() => (fn() => scheduled())())() }\n",
        "}\n",
        "fn twice(value: i64) -> bool {\n",
        "    when { return (fn() => scheduled() || scheduled())() }\n",
        "}\n",
        "fn initialised(value: i64) -> bool {\n",
        "    state woken: bool = (fn() => scheduled())()\n",
        "    when { return woken }\n",
        "}\n",
        "fn lifecycle(value: i64) -> i64 {\n",
        "    inject scheduler\n",
        "    start {\n",
        "        let due = (fn() => scheduled())()\n",
        "        schedule(scheduler, 1s)\n",
        "    }\n",
        "    when { return value }\n",
        "    stop { let due = (fn() => scheduled())() }\n",
        "}\n",
        "const fn valued() -> bool => (fn() => scheduled())()\n",
        "fn source() -> bool {\n",
        "    yield 1s: (fn() => scheduled())()\n",
        "}\n",
        "fn composed(value: i64) -> bool => (fn() => scheduled())()\n",
        "test probe {\n",
        "    assert (fn() => scheduled())()\n",
        "}\n",
        "fn returned(value: i64) -> i64 {\n",
        "    inject scheduler\n",
        "    start { schedule(scheduler, 1s) }\n",
        "    when {\n",
        "        let due = (fn() { return scheduled() })()\n",
        "        return value\n",
        "    }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:28 injectable/inject.no_scheduler TIME-9",
            "m.hgl:6:37 injectable/inject.no_scheduler TIME-9",
            "m.hgl:9:28 injectable/inject.no_scheduler TIME-9",
            "m.hgl:9:43 injectable/inject.no_scheduler TIME-9",
            "m.hgl:12:34 injectable/inject.no_scheduler TIME-9",
            "m.hgl:18:28 phase/scheduled.lifecycle TIME-9",
            "m.hgl:22:31 phase/scheduled.lifecycle TIME-9",
            "m.hgl:24:39 phase/scheduled.outside_node FN-9",
            "m.hgl:26:24 injectable/inject.no_scheduler TIME-9",
            "m.hgl:28:45 phase/scheduled.outside_node FN-9",
            "m.hgl:30:21 phase/scheduled.outside_node FN-9",
        ]
    );
}

/// TIME-9: `scheduled()` is the selector, and core declares no function
/// of the name: it has no generic parameter, and GEN-8's "A call may
/// supply its generic arguments" gives it none to take
/// (`generic.argument_count`). Written with one it is the selector all
/// the same, and is held to where it stands: in a value function
/// (`scheduled.outside_node`, FN-9), a node that injects neither
/// (`inject.no_scheduler`), a header, and an anonymous function.
#[test]
fn scheduled_takes_no_generic_argument() {
    let checked = module(concat!(
        "const fn typed() -> bool => scheduled<i64>()\n",
        "fn timed(value: i64) -> bool {\n",
        "    inject scheduler\n",
        "    start { schedule(scheduler, 1s) }\n",
        "    when scheduled<i64>() { return true }\n",
        "}\n",
        "fn untimed(value: i64) -> bool {\n",
        "    when { return scheduled<i64>() }\n",
        "}\n",
        "fn anonymous(value: i64) -> bool {\n",
        "    inject alarm\n",
        "    start { schedule(alarm, 1s) }\n",
        "    when { return (fn() => scheduled<bool>())() }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:2:29 phase/scheduled.outside_node FN-9",
            "m.hgl:2:29 type/generic.argument_count GEN-8",
            "m.hgl:6:10 type/generic.argument_count GEN-8",
            "m.hgl:9:19 injectable/inject.no_scheduler TIME-9",
            "m.hgl:9:19 type/generic.argument_count GEN-8",
            "m.hgl:14:28 type/generic.argument_count GEN-8",
        ]
    );
}

/// FN-14: "An anonymous function's scope is the scope around it plus its
/// parameters". TIME-9: "A call gives an `endpoint<T>` parameter a
/// temporal input of the enclosing node by its name, a child of one ...
/// or `out`, and nothing else", and "`all_valid` takes only a
/// collection or a window; another endpoint is rejected
/// (`endpoint.argument`)". So an operation in an anonymous function is
/// given what it would be given where the function is written: not the
/// function's own parameter, nor a `let` of its body, and not an
/// endpoint at all in a value function; core's `is_active` alike, and a
/// child of the function's parameter is no child of an input. Each is
/// held to the number of arguments it takes too, `scheduled()` none and
/// `delta_value` one, and to the type its generic argument names
/// (EXPR-5); and a `signal` has no delta, and a `ref` parameter's is not
/// read (TYPE-16, FN-16).
#[test]
fn an_endpoint_operation_in_an_anonymous_function_takes_what_it_would_where_written() {
    let checked = module(concat!(
        "fn parameter(value: i64) -> bool {\n",
        "    when { return (fn(given: i64) => valid(given))(value) }\n",
        "}\n",
        "fn bound(value: i64) -> bool {\n",
        "    when {\n",
        "        return (fn() {\n",
        "            let held = value\n",
        "            return modified(held)\n",
        "        })()\n",
        "    }\n",
        "}\n",
        "fn scalar(value: i64) -> bool {\n",
        "    when { return (fn() => all_valid(value))() }\n",
        "}\n",
        "const fn plain(flag: bool) -> bool => (fn() => valid(flag))()\n",
        "fn counted(value: i64) -> bool {\n",
        "    inject scheduler\n",
        "    start { schedule(scheduler, 1s) }\n",
        "    when { return (fn() => scheduled(value) || delta_value() == 1)() }\n",
        "}\n",
        "fn active(value: i64) -> bool {\n",
        "    when { return (fn(given: i64) => is_active(given))(value) }\n",
        "}\n",
        "fn typed(value: i64) -> bool {\n",
        "    when { return (fn() => valid<f64>(value))() }\n",
        "}\n",
        "fn child(parts: list<i64>) -> bool {\n",
        "    when { return (fn(given: list<i64>) => valid(given[0]))(parts) }\n",
        "}\n",
        "fn ticked(probe: signal, ref pointer: f64) -> bool {\n",
        "    when {\n",
        "        let first = (fn() => delta_value(probe))()\n",
        "        let second = (fn() => delta_value(pointer))()\n",
        "        return true\n",
        "    }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:44 phase/endpoint.argument TIME-9",
            "m.hgl:9:29 phase/endpoint.argument TIME-9",
            "m.hgl:14:38 phase/endpoint.argument TIME-9",
            "m.hgl:16:54 phase/endpoint.argument TIME-9",
            "m.hgl:20:38 type/argument.unexpected TIME-9",
            "m.hgl:20:48 type/argument.missing TIME-9",
            "m.hgl:23:48 phase/endpoint.argument TIME-9",
            "m.hgl:26:34 type/type.mismatch EXPR-5",
            "m.hgl:29:50 phase/endpoint.argument TIME-9",
            "m.hgl:33:30 shape/delta.unsupported_shape TYPE-16",
            "m.hgl:34:31 phase/ref.read FN-16",
        ]
    );
}

/// TIME-4 and TIME-5 with FN-14: what is right where the function is
/// written stays right in it. `valid` and `modified` of an input are
/// metadata, and a `valid(value)` in the function guards its own read;
/// one in a header is no selector, as it is no top-level conjunct, and
/// the default admission stands (TIME-3); `modified([])` there selects
/// nothing, and is not warned of. A test of a call's result proves
/// nothing (TIME-4: "A saved bool proves nothing"), and `start` gives no
/// input to an endpoint operation (TIME-5). A `return` in the function
/// is the function's, and its `bool` no mismatch with what the node
/// publishes (TIME-7).
#[test]
fn an_endpoint_operation_in_an_anonymous_function_keeps_its_guards_and_phases() {
    let checked = module(concat!(
        "fn guarded(value: i64) -> bool {\n",
        "    when valid([]) { return (fn() => valid(value) && value > 0)() }\n",
        "}\n",
        "fn metadata(value: i64) -> bool {\n",
        "    when valid([]) { return (fn() => modified(value) || valid(value))() }\n",
        "}\n",
        "fn header(value: i64) -> i64 {\n",
        "    when (fn() => valid(value))() { return value }\n",
        "}\n",
        "fn empty(value: i64) -> i64 {\n",
        "    when (fn() => modified([]))() { return value }\n",
        "}\n",
        "fn saved(value: i64) -> i64 {\n",
        "    when valid([]) {\n",
        "        if (fn() => valid(value))() { return value }\n",
        "        return 0\n",
        "    }\n",
        "}\n",
        "fn started(value: i64) -> i64 {\n",
        "    start { let seen = (fn() => modified(value))() }\n",
        "    when { return value }\n",
        "}\n",
        "fn published(value: i64) -> str {\n",
        "    when {\n",
        "        let seen = (fn() { return valid(value) })()\n",
        "        return \"seen\"\n",
        "    }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:16:46 phase/payload.unguarded TIME-4",
            "m.hgl:21:33 phase/input.lifecycle TIME-5",
        ]
    );
}
