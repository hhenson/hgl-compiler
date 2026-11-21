//! The analyser against the rules on the bodies of temporal functions:
//! a payload read in a node needs admission (TIME-1), composition reads
//! none (TIME-2), its configuration included (EXPR-14), and a hook's and
//! a graph's expressions are held to what a value function's are. Each
//! test cites its rule.
//!
//! A finding is written `file:line:column category/code rule`.

mod common;

use common::{Checked, module};
use hgl_analyser::hir::ValueCategory;

/// No lines.
const NONE: [&str; 0] = [];

/// The locals of the function `name` of module `m`, its parameters
/// first, as `local: type`, with ` (connection)` after one that is a
/// connection of a graph.
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
        .map(|local| {
            let connection = match local.category {
                ValueCategory::Connection => " (connection)",
                _ => "",
            };
            format!("{}: {}{connection}", local.name, local.ty)
        })
        .collect()
}

// ---- TIME-1: a payload read needs admission ----

/// TIME-1: "A payload read requires admission/readability; metadata may
/// observe an invalid endpoint." Under `valid([])` and in `stop`, where
/// nothing admits `value`, its validity, its modification, when it last
/// published and whether it is active are observed with no guard. Its
/// payload is read where a header or a guard admits it.
#[test]
fn metadata_observes_an_invalid_endpoint() {
    let checked = module(concat!(
        "fn observed(value: i64, probe: bool) -> bool {\n",
        "    when modified(probe) && valid([]) {\n",
        "        let ticked = modified(value)\n",
        "        let at = last_modified(value)\n",
        "        let active = is_active(value)\n",
        "        return valid(value) || ticked || active || at == null\n",
        "    }\n",
        "    stop {\n",
        "        let at = last_modified(value)\n",
        "    }\n",
        "}\n",
        "fn admitted(a: i64, b: i64) -> i64 {\n",
        // The default admits every input; `valid(a)` admits `a`, and a
        // guard `b` where it holds.
        "    when modified(a, b) { return a + b }\n",
        "    when valid(a) {\n",
        "        if valid(b) { return a + b }\n",
        "        return a\n",
        "    }\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE);
}

/// TIME-1: "A payload read requires admission/readability". Where
/// nothing admits a read it rejects: under manual admission with no
/// guard (`payload.unguarded`, TIME-4), in `stop` with none (TIME-5), in
/// `start` and a `state`'s initializer, which run before any input has
/// a value (TIME-5), and in a `cache`'s initializer, which runs on
/// every start (TIME-6). The catalogue names a state's initializer
/// alone, and the cache's code is local.
#[test]
fn a_payload_read_needs_admission() {
    let checked = module(concat!(
        "fn read(value: i64) -> i64 {\n",
        "    state seen: i64 = value\n",
        "    cache kept: i64 = value + 1\n",
        "    cache counted: i64 = 0\n",
        "    start { counted = value }\n",
        "    when valid([]) { return value }\n",
        "    stop { seen = value }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:23 phase/state.input_read TIME-5",
            "m.hgl:4:23 local:phase/cache.input_read TIME-1",
            "m.hgl:6:23 phase/input.lifecycle TIME-5",
            "m.hgl:7:29 phase/payload.unguarded TIME-4",
            "m.hgl:8:19 phase/payload.unguarded TIME-5",
        ]
    );
}

/// TIME-1 with TIME-6: a cache is "initialized on every start using
/// available restored state". Its initializer reads the node's state and
/// its `const` configuration, which have values then; core's
/// `total_from_initial_state` case reads its state so.
#[test]
fn a_cache_is_initialized_from_state_and_configuration() {
    let checked = module(concat!(
        "fn total(value: i64, const seed: i64) -> i64 {\n",
        "    state opening: i64 = seed\n",
        "    cache running: i64 = opening * 2 + seed\n",
        "    when {\n",
        "        running += value\n",
        "        return running\n",
        "    }\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE);
}

/// TIME-1 and TIME-5 with FN-14: "An anonymous function's scope is the
/// scope around it plus its parameters". A temporal input it reads is
/// read where it is written, and held to the admission there: the
/// temporal-model chapter's rejections, each with the read in an
/// anonymous function called on the spot, `return value + 1` under
/// `valid([])`, `start { last = value }`, `stop { last = value }`, and a
/// `state`'s and a `cache`'s initializer. A call of what such a call
/// gives is read too.
#[test]
fn an_anonymous_function_reads_where_it_is_written() {
    let checked = module(concat!(
        "fn manual_forward(value: i64) -> i64 {\n",
        "    when valid([]) { return (fn() => value + 1)() }\n",
        "}\n",
        "fn started(value: i64) -> i64 {\n",
        "    state last: i64 = 0\n",
        "    start { last = (fn() { return value })() }\n",
        "    when { return value }\n",
        "}\n",
        "fn stopped(value: i64) -> i64 {\n",
        "    state last: i64 = 0\n",
        "    when { return value }\n",
        "    stop { last = (fn() => value)() }\n",
        "}\n",
        "fn initialized(value: i64) -> i64 {\n",
        "    state last: i64 = (fn() => value)()\n",
        "    cache kept: i64 = (fn() => value)()\n",
        "    when { return last + kept }\n",
        "}\n",
        "fn curried(value: i64) -> i64 {\n",
        "    when valid([]) { return (fn(a) => fn(b) => a + b + value)(1)(2) }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:38 phase/payload.unguarded TIME-4",
            "m.hgl:7:35 phase/input.lifecycle TIME-5",
            "m.hgl:13:28 phase/payload.unguarded TIME-5",
            "m.hgl:16:32 phase/state.input_read TIME-5",
            "m.hgl:17:32 local:phase/cache.input_read TIME-1",
            "m.hgl:21:56 phase/payload.unguarded TIME-4",
        ]
    );
}

/// TIME-4, TIME-5 with FN-14: "An anonymous function's scope is the
/// scope around it plus its parameters". The callee of a call is an
/// expression like any other, read where it is written: an anonymous
/// function chosen by an `if` or taken from a list reads the node's
/// input under the admission there, as one called on the spot does.
/// The temporal-model chapter's rejections: `return value + 1` under
/// `valid([])` (TIME-4: "A payload read under manual admission must be
/// dominated by a successful guard"), `start { last = value }` (TIME-5:
/// "`start` reads no temporal input"), a `state`'s initializer (TIME-5:
/// "State initializers cannot read current temporal inputs") and
/// `stop { last = value }` (TIME-5: "each payload read is dominated by a
/// guard").
#[test]
fn a_callee_is_read_where_it_is_written() {
    let checked = module(concat!(
        "fn chosen(value: i64, const c: bool) -> i64 {\n",
        "    when valid([]) { return (if c { fn() => value + 1 } else { fn() => 0 })() }\n",
        "}\n",
        "fn indexed(value: i64) -> i64 {\n",
        "    when valid([]) { return [fn() => value + 1][0]() }\n",
        "}\n",
        "fn started(value: i64) -> i64 {\n",
        "    state last: i64 = 0\n",
        "    start { last = (if true { fn() => value } else { fn() => 0 })() }\n",
        "    when { return value }\n",
        "}\n",
        "fn initialized(value: i64) -> i64 {\n",
        "    state last: i64 = [fn() => value][0]()\n",
        "    when { return last }\n",
        "}\n",
        "fn stopped(value: i64) -> i64 {\n",
        "    state last: i64 = 0\n",
        "    when { return value }\n",
        "    stop { last = (if true { fn() => value } else { fn() => 0 })() }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:45 phase/payload.unguarded TIME-4",
            "m.hgl:6:38 phase/payload.unguarded TIME-4",
            "m.hgl:10:39 phase/input.lifecycle TIME-5",
            "m.hgl:14:32 phase/state.input_read TIME-5",
            "m.hgl:20:38 phase/payload.unguarded TIME-5",
        ]
    );
}

/// FN-7: "Anonymous functions obtain their parameter/result context
/// from their accepting form; inference does not create a general
/// callable value type." A value of a type is no function, and a call
/// of one rejects (`type.mismatch`): what a value function returns, an
/// element of a list, the value of an `if`, a sum and a local. The
/// callee is read first, for what it reads (TIME-4: "A payload read
/// under manual admission must be dominated by a successful guard").
/// An anonymous function, a local bound to one, and what one returns
/// are called.
#[test]
fn a_call_of_what_is_no_function_is_rejected() {
    let checked = module(concat!(
        "const fn plus_one(x: i64) -> i64 => x + 1\n",
        "fn called_result(value: i64) -> i64 {\n",
        "    when valid([]) { return plus_one(value)() }\n",
        "}\n",
        "fn called_element(value: i64) -> i64 {\n",
        "    when valid([]) { return [value][0]() }\n",
        "}\n",
        "fn called_if(value: i64, const c: bool) -> i64 {\n",
        "    when valid([]) { return (if c { value } else { 0 })() }\n",
        "}\n",
        "const fn called_literal() -> i64 => (1 + 2)()\n",
        "const fn called_local(n: i64) -> i64 {\n",
        "    let twice = n * 2\n",
        "    return twice()\n",
        "}\n",
        "const fn called_functions(n: i64) -> i64 {\n",
        "    let add = fn(x) => x + n\n",
        "    return add(1) + (fn() => fn() => n)()()\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:4:29 type/type.mismatch FN-7",
            "m.hgl:4:38 phase/payload.unguarded TIME-4",
            "m.hgl:7:29 type/type.mismatch FN-7",
            "m.hgl:7:30 phase/payload.unguarded TIME-4",
            "m.hgl:10:29 type/type.mismatch FN-7",
            "m.hgl:10:37 phase/payload.unguarded TIME-4",
            "m.hgl:12:37 type/type.mismatch FN-7",
            "m.hgl:15:12 type/type.mismatch FN-7",
        ]
    );
}

/// TIME-4 with FN-14: what proves an input valid where an anonymous
/// function is written proves it in the function's body, and so does a
/// guard in the body itself. An input's validity does not change within
/// an evaluation, so a function written under a guard reads under it
/// wherever that evaluation calls it. One written where nothing proves
/// the input valid reads it there, though it is called under a guard.
#[test]
fn an_anonymous_function_reads_under_the_admission_where_it_is_written() {
    let checked = module(concat!(
        "fn admitted(value: i64) -> i64 {\n",
        "    when { return (fn(x: i64) -> i64 => x + value)(1) }\n",
        "}\n",
        "fn inside(value: i64) -> i64 {\n",
        "    when valid([]) { return (fn() => if valid(value) { value } else { 0 })() }\n",
        "}\n",
        "fn outside(value: i64) -> i64 {\n",
        "    when valid([]) {\n",
        "        if valid(value) {\n",
        "            let add = fn(x) => x + value\n",
        "            return add(1)\n",
        "        }\n",
        "        return 0\n",
        "    }\n",
        "}\n",
        "fn unproved(value: i64) -> i64 {\n",
        "    when valid([]) {\n",
        "        let read = fn() => value\n",
        "        if valid(value) { return read() }\n",
        "        return 0\n",
        "    }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:19:28 phase/payload.unguarded TIME-4"]
    );
}

/// FN-7, CTRL-5 and ERR-5: the anonymous function has its own result
/// type for `return` and for a failure its body raises, including in a
/// node's `when`, `start` or `stop` hook. A result written `-> Base`
/// takes the branches of an `if` that are two of its children (TYPE-3).
/// Capturing `out` still rejects at the capture (FN-14).
#[test]
fn an_anonymous_function_does_not_publish_to_the_enclosing_node() {
    let checked = module(concat!(
        "abstract struct Base {\n",
        "    id: i64\n",
        "}\n",
        "struct Left : Base {\n",
        "}\n",
        "struct Right : Base {\n",
        "}\n",
        "fn hooked(value: i64) -> i64 {\n",
        "    inject out\n",
        "    start {\n",
        "        let first = fn(x: result<i64, str>) -> result<i64, str> { let n: i64 = x\n",
        "            return ok(n) }\n",
        "        let written = fn() => out\n",
        "    }\n",
        "    when {\n",
        "        let each = fn(x: result<i64, str>) -> result<i64, str> { let n: i64 = x\n",
        "            return ok(n + value) }\n",
        "        let pick = fn(c: bool, a: Left, b: Right) -> Base => if c { a } else { b }\n",
        "        return value\n",
        "    }\n",
        "    stop {\n",
        "        let last = fn(x: result<i64, str>) -> result<i64, str> { let n: i64 = x\n",
        "            return ok(n) }\n",
        "    }\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), ["m.hgl:14:31 name/name.captured FN-14"]);
}

/// TIME-7: "`out` is read in a `when` and in `stop` ... `start` neither
/// reads nor writes it". A `state`'s and a `cache`'s initializer give
/// them their first values on every start (TIME-6), before the node has
/// published: `out` read there is `out.lifecycle`.
#[test]
fn out_is_not_read_in_an_initializer() {
    let checked = module(concat!(
        "fn kept(value: i64) -> i64 {\n",
        "    inject out\n",
        "    state first: i64 = out\n",
        "    cache again: i64 = out + 1\n",
        "    when { return value + first + again }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:4:24 phase/out.lifecycle TIME-7",
            "m.hgl:5:24 phase/out.lifecycle TIME-7",
        ]
    );
}

// ---- TIME-2: composition reads no live payload ----

/// TIME-2: "Composition describes a typed graph without reading live
/// payloads." In a graph a connection is wired, and a lifted call takes
/// it as an input (FN-6). Where the graph needs a value while it is
/// built, a connection is rejected (`connection.as_constant`): a
/// `const` parameter, a `switch` label and a `while` condition (FN-3,
/// CTRL-15), a call on the store (FN-9), a symbol's configuration
/// (EXPR-14). An endpoint operation is given no input there (TIME-9).
#[test]
fn composition_reads_no_live_payload() {
    let checked = module(concat!(
        "const fn scaled(value: f64, const by: f64) -> f64 => value * by\n",
        // Wired: a lifted call, a constant given to `by`.
        "fn wired(price: f64, factor: f64) -> f64 => scaled(price * factor, 2.0)\n",
        "fn read(price: f64, factor: f64, side: i64, label: i64, open: bool) -> f64 {\n",
        "    inject global_state\n",
        "    put(global_state, \"price\", price)\n",
        "    var running = scaled(price, factor)\n",
        "    switch side {\n",
        "        case label: running = price\n",
        "    }\n",
        "    while open { running = price }\n",
        "    if price =={abs: factor} 0.0 { running = price }\n",
        "    return running\n",
        "}\n",
        "fn observed(price: f64) -> bool => valid(price)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:6:32 phase/connection.as_constant FN-9",
            "m.hgl:7:33 phase/connection.as_constant FN-3",
            "m.hgl:9:14 phase/connection.as_constant FN-3",
            "m.hgl:11:11 phase/connection.as_constant FN-3",
            "m.hgl:12:22 phase/connection.as_constant EXPR-14",
            "m.hgl:15:42 phase/endpoint.argument TIME-9",
        ]
    );
}

// ---- EXPR-14: configuration introduces no temporal input ----

/// EXPR-14: "Scalar operator configuration binds during graph
/// construction in temporal composition. In a value function or runtime
/// hook, evaluate it for the current invocation." The expressions-and-
/// bindings chapter's fragments: `a =={abs: 1e-9, rel: 1e-6} b`, and a
/// tolerance "bound during graph construction", here a `const`
/// parameter. In a value function and a hook the tolerance is a value of
/// the invocation. FN-6: a call by name gives its argument explicitly,
/// "A supplied temporal argument remains an input even when its
/// parameter has a default". A temporal candidate's parameter is
/// "a signature explicitly admitting that input".
#[test]
fn configuration_binds_where_the_graph_is_built() {
    let checked = module(concat!(
        "fn written(a: f64, b: f64) -> bool => a =={abs: 1e-9, rel: 1e-6} b\n",
        "fn configured(a: f64, b: f64, const tolerance: f64) -> bool =>\n",
        "    a =={abs: tolerance} b\n",
        "fn named(a: f64, b: f64, tolerance: f64) -> bool => eq_(a, b, abs: tolerance)\n",
        "const fn valued(a: f64, b: f64, tolerance: f64) -> bool => a =={abs: tolerance} b\n",
        "fn hooked(a: f64, b: f64, tolerance: f64) -> bool {\n",
        "    when { return a =={abs: tolerance} b }\n",
        "}\n",
        "struct Price {\n",
        "    value: f64\n",
        "}\n",
        "impl fn eq_(lhs: Price, rhs: Price, abs: f64 = 0.0) -> bool {\n",
        // TIME-1: a field is read where it is admitted.
        "    when all_valid(lhs) && all_valid(rhs) { return lhs.value - rhs.value < abs }\n",
        "}\n",
        "fn near(a: Price, b: Price, tolerance: f64) -> bool => a =={abs: tolerance} b\n",
    ));
    assert_eq!(checked.summary(), NONE);
}

/// EXPR-14: "Configuration must not silently introduce a temporal
/// input: time-varying temporal configuration requires a signature
/// explicitly admitting that input." The chapter's deliberate
/// rejection: "binding a time-series tolerance to the built-in scalar
/// `abs` option". Core's candidate of `eq_` for two `f64`s is a value
/// candidate, which the graph lifts (FN-6): a connection given to its
/// option by the symbol's configuration is rejected
/// (`connection.as_constant`), whichever entry it is.
#[test]
fn configuration_introduces_no_temporal_input() {
    let checked = module(concat!(
        "fn close(a: f64, b: f64, tolerance: f64) -> bool => a =={abs: tolerance} b\n",
        "fn mixed(a: f64, const b: f64, tolerance: f64) -> bool =>\n",
        "    a =={rel: 1e-6, abs: tolerance * 2.0} b\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:2:63 phase/connection.as_constant EXPR-14",
            "m.hgl:4:26 phase/connection.as_constant EXPR-14",
        ]
    );
}

// ---- A hook's and a graph's expressions ----

/// The value rules hold in a node's hook, which is read as value code
/// (FN-3), and in a graph, where a lifted call gives a connection of
/// what the call gives (FN-6). EXPR-2: "named arguments come in any
/// order, and a parameter that may be omitted may be left out." EXPR-5:
/// "An integer literal is an `f64` where one is expected". EXPR-10: an
/// entry of a symbol's configuration is a named argument of its call.
/// EXPR-11: "`<=>` ... returns core's `CmpResult`". EXPR-12: "core
/// division's `zero` policy selects f64 or `result<f64, error>`".
#[test]
fn a_temporal_bodys_expressions_have_a_value_functions_types() {
    let checked = module(concat!(
        "const fn f(a: i64, b: i64 = 1) -> i64 => a + b\n",
        "fn hooked(x: i64, a: f64, b: f64) -> i64 {\n",
        "    when {\n",
        "        let named = f(b: 2, a: x)\n",
        "        let left = f(x)\n",
        "        let widened: f64 = 1\n",
        "        let near = a =={abs: 1e-9} b\n",
        "        let outcome = a <=> b\n",
        "        let ratio = a /{zero: .error} b\n",
        "        let plain = a /{zero: .nan} b\n",
        "        return named + left\n",
        "    }\n",
        "}\n",
        "fn wired(x: i64, a: f64, b: f64) -> CmpResult {\n",
        "    let named = f(b: 2, a: x)\n",
        "    let near = a =={abs: 1e-9} b\n",
        "    let ratio = a /{zero: .error} b\n",
        "    let plain = a /{zero: .nan} b\n",
        "    return a <=> b\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(
        locals(&checked, "hooked")[3..],
        [
            "named: i64",
            "left: i64",
            "widened: f64",
            "near: bool",
            "outcome: CmpResult",
            "ratio: result<f64, error>",
            "plain: f64",
        ]
    );
    assert_eq!(
        locals(&checked, "wired")[3..],
        [
            "named: i64 (connection)",
            "near: bool (connection)",
            "ratio: result<f64, error> (connection)",
            "plain: f64 (connection)",
        ]
    );
}

/// What a value function rejects, a hook and a graph reject: EXPR-2,
/// "reject duplicates, unknown names, missing required arguments, and
/// wrong types"; EXPR-5, "A value that has a type is not converted by
/// its place"; EXPR-10, "Reject empty configuration, duplicate options
/// ... and an option that no candidate for the operands declares";
/// EXPR-11, an outcome is no bool; EXPR-12, division under `.error` is a
/// result and no `f64`.
#[test]
fn a_temporal_body_rejects_what_a_value_function_does() {
    let checked = module(concat!(
        "const fn f(a: i64, b: i64 = 1) -> i64 => a + b\n",
        "fn hooked(x: i64, a: f64, b: f64) -> i64 {\n",
        "    when {\n",
        "        let twice = f(x, a: 1)\n",
        "        let unknown = f(x, c: 2)\n",
        "        let missing = f(b: 2)\n",
        "        let surplus = f(1, 2, 3)\n",
        "        let text: str = x\n",
        "        let repeated = a =={abs: 1.0, abs: 2.0} b\n",
        "        let option = a =={nope: 1.0} b\n",
        "        let flag: bool = a <=> b\n",
        "        let ratio: f64 = a /{zero: .error} b\n",
        "        return x\n",
        "    }\n",
        "}\n",
        "fn wired(x: i64, a: f64, b: f64) -> i64 {\n",
        "    let twice = f(x, a: 1)\n",
        "    let unknown = f(x, c: 2)\n",
        "    let missing = f(b: 2)\n",
        "    let surplus = f(1, 2, 3)\n",
        "    let text: str = x\n",
        "    let repeated = a =={abs: 1.0, abs: 2.0} b\n",
        "    let option = a =={nope: 1.0} b\n",
        "    let flag: bool = a <=> b\n",
        "    let ratio: f64 = a /{zero: .error} b\n",
        "    return x\n",
        "}\n",
    ));
    // The hook's, then the graph's: the same rejections. EXPR-11 and
    // EXPR-12: the type the place expects is requested of the
    // candidates (GEN-5), and none gives it.
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:5:26 name/argument.duplicate EXPR-2",
            "m.hgl:6:28 name/argument.unknown EXPR-2",
            "m.hgl:7:23 type/argument.missing EXPR-2",
            "m.hgl:8:31 type/argument.unexpected EXPR-2",
            "m.hgl:9:25 type/type.mismatch EXPR-5",
            "m.hgl:10:39 name/argument.duplicate EXPR-10",
            "m.hgl:11:27 name/argument.unknown EXPR-10",
            "m.hgl:12:28 operator/operator.no_candidate OPS-2",
            "m.hgl:18:22 name/argument.duplicate EXPR-2",
            "m.hgl:19:24 name/argument.unknown EXPR-2",
            "m.hgl:20:19 type/argument.missing EXPR-2",
            "m.hgl:21:27 type/argument.unexpected EXPR-2",
            "m.hgl:22:21 type/type.mismatch EXPR-5",
            "m.hgl:23:35 name/argument.duplicate EXPR-10",
            "m.hgl:24:23 name/argument.unknown EXPR-10",
            "m.hgl:25:24 operator/operator.no_candidate OPS-2",
        ]
    );
}
