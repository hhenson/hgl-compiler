//! The analyser against the rules on what a temporal function's body
//! describes. Each test cites its rule.

mod common;

use common::{check, check_broken};

/// FN-2: "`state`, `cache`, `start`, `when`, `stop`, `yield`, or an
/// `inject` of any capability but `global_state` classifies that
/// complete body as runtime ... A graph may inject `global_state`
/// (FN-9)." FN-9: "A graph injects `global_state` and no other, and
/// uses it where the graph is built: each call on it is evaluated once
/// then ... and is given no connection (`connection.as_constant`)."
#[test]
fn a_graph_injects_the_store_and_sets_it_where_it_is_built() {
    let checked = check(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "fn pass(value: i64) -> i64 {\n",
            "    when { return value }\n",
            "}\n",
            "fn scaled(value: i64, const factor: i64) -> i64 {\n",
            "    when { return value * factor }\n",
            "}\n",
            // A graph: it puts a value and reads one back, each where
            // it is built.
            "fn seeded(value: i64, const first: i64) -> i64 {\n",
            "    inject global_state\n",
            "    put(global_state, \"factor\", first)\n",
            "    let factor = get<i64>(global_state, \"factor\")\n",
            "    return scaled(value, factor)\n",
            "}\n",
            // A connection has no value where the graph is built.
            "fn connected(value: i64) -> i64 {\n",
            "    inject global_state\n",
            "    put(global_state, \"limit\", value)\n",
            "    return pass(value)\n",
            "}\n",
            // The store under another name is the store.
            "fn aliased(value: i64) -> i64 {\n",
            "    inject global_state\n",
            "    let store = global_state\n",
            "    put(store, \"limit\", value)\n",
            "    return pass(value)\n",
            "}\n",
            // Any other capability makes the body a node's.
            "fn clocked(value: i64) -> i64 {\n",
            "    inject clock\n",
            "    return pass(value)\n",
            "}\n",
            // So does the store beside one.
            "fn both(value: i64) -> i64 {\n",
            "    inject global_state, logger\n",
            "    return pass(value)\n",
            "}\n",
        ),
    )]);
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:16:32 phase/connection.as_constant FN-9",
            "m.hgl:22:25 phase/connection.as_constant FN-9",
            "m.hgl:27:5 phase/node.statement FN-2",
            "m.hgl:31:5 phase/node.statement FN-2",
        ],
        "{}",
        checked.report
    );
    let module = checked.program.module("m").unwrap();
    let phases: Vec<String> = module
        .declarations
        .iter()
        .zip(&module.phases)
        .map(|(declaration, phase)| format!("{}: {phase:?}", declaration.name))
        .collect();
    assert_eq!(
        phases,
        [
            "pass: Some(Node)",
            "scaled: Some(Node)",
            "seeded: Some(Graph)",
            "connected: Some(Graph)",
            "aliased: Some(Graph)",
            "clocked: Some(Node)",
            "both: Some(Node)",
        ]
    );
}

/// FN-9 and GEN-7: a graph's store still takes ordinary values after
/// each instance selects the operations that supply its arguments.
#[test]
fn graph_instances_recheck_store_arguments() {
    let checked = check(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "operator helper<T>(input: T) -> i64\n",
            "impl const fn helper<T>(input: T) -> i64 => 5\n",
            "impl fn helper(input: i64) -> i64 { when { return input } }\n",
            "fn stash<T>(const input: T) -> i64 {\n",
            "    inject global_state\n",
            "    put(global_state, \"direct\", helper(input))\n",
            "    return 7\n",
            "}\n",
            "fn write_aliased<T>(const input: T) -> i64 {\n",
            "    inject global_state\n",
            "    let store = global_state\n",
            "    let given = helper(input)\n",
            "    put(store, \"alias\", given)\n",
            "    return 7\n",
            "}\n",
            "fn already_rejected<T>(const input: T, given: i64) -> i64 {\n",
            "    inject global_state\n",
            "    put(global_state, \"known\", given)\n",
            "    return helper(input)\n",
            "}\n",
            "fn temporal() -> i64 => stash(1)\n",
            "fn aliased_temporal() -> i64 => write_aliased(1)\n",
            "fn ordinary() -> i64 => stash(\"s\")\n",
            "fn aliased_ordinary() -> i64 => write_aliased(\"s\")\n",
            "fn check_once() -> i64 => already_rejected(\"s\", 1)\n",
        ),
    )]);
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:19:32 phase/connection.as_constant FN-9",
            "m.hgl:22:25 phase/connection.as_constant FN-9",
            "m.hgl:23:33 phase/connection.as_constant FN-9",
        ],
        "{}",
        checked.report
    );
}

/// FN-2: "An ordinary temporal body with no node-only construct
/// describes composition: the function is a graph. `state`, `cache`,
/// `start`, `when`, `stop`, `yield`, or an `inject` of any capability
/// but `global_state` classifies that complete body as runtime: the
/// function is a node ... Classification
/// precedes phase checking, even if the trigger occurs in an invalid
/// nested position." FN-3: "Iteration and conditionals follow the
/// containing phase. They do not alone classify a body as runtime."
/// GEN-4: "An `impl fn` is a temporal candidate", and is one or the
/// other as a function is.
#[test]
fn a_temporal_body_is_a_graph_or_a_node() {
    let checked = check(&[(
        "m.hgl",
        concat!(
            "module m\n",
            // A value function is neither.
            "const fn scale(value: f64) -> f64 => value * 2.0\n",
            "fn doubled(value: f64) -> f64 => scale(value)\n",
            "fn chosen(flag: bool, left: i64, right: i64) -> i64 {\n",
            "    if flag { left } else { right }\n",
            "}\n",
            "fn forward(value: i64) -> i64 {\n",
            "    when { return value }\n",
            "}\n",
            "fn total(value: f64) -> f64 {\n",
            "    state sum: f64 = 0.0\n",
            "    when {\n",
            "        sum += value\n",
            "        return sum\n",
            "    }\n",
            "}\n",
            "fn injected(value: i64) -> i64 {\n",
            "    inject out\n",
            "}\n",
            "fn started(value: i64) {\n",
            "    start { }\n",
            "}\n",
            // A `when` in a block is rejected (TIME-3), and classifies
            // the body all the same.
            "fn nested(flag: bool, value: i64) -> i64 {\n",
            "    if flag {\n",
            "        when { return value }\n",
            "    }\n",
            "}\n",
            "operator pass<V>(value: V) -> V\n",
            "impl fn pass(value: i64) -> i64 {\n",
            "    when { return value }\n",
            "}\n",
            "impl fn pass(value: f64) -> f64 => value\n",
        ),
    )]);
    // TIME-3: the nested `when` is rejected, and so is the `if` it
    // stands in, a statement of a node's body that is no hook (FN-2).
    // It classifies its body all the same.
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:24:5 phase/node.statement FN-2",
            "m.hgl:25:9 phase/node.nested TIME-3",
        ]
    );
    let module = checked.program.module("m").expect("module m exists");
    let phases: Vec<String> = module
        .declarations
        .iter()
        .zip(&module.phases)
        .map(|(declaration, phase)| {
            format!(
                "{}: {}",
                declaration.name,
                phase.map_or("-", |phase| phase.as_str())
            )
        })
        .collect();
    assert_eq!(
        phases,
        [
            "scale: -",
            "doubled: graph",
            "chosen: graph",
            "forward: node",
            "total: node",
            "injected: node",
            "started: node",
            "nested: node",
            "pass: -",
            "pass: node",
            "pass: graph",
        ]
    );
}

/// FN-2: "A node's body is its `state`, `cache`, and `inject`
/// declarations and then its `start`, `when`, and `stop` hooks, and
/// nothing else: a local that every hook reads is a `cache`, and work
/// done once belongs in `start`." TIME-5: "State/cache/inject
/// declarations precede executable hooks. At most one start and one stop
/// are admitted." TIME-3: "A `when` nested in another block is
/// rejected." TIME-7: `inject out` "is rejected for outputless
/// functions."
#[test]
fn a_nodes_body_is_its_declarations_and_then_its_hooks() {
    let checked = check(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "fn counted(value: i64) -> i64 {\n",
            "    state seen: i64 = 0\n",
            "    cache step: i64 = 1\n",
            "    inject out, scheduler\n",
            "    start { }\n",
            "    when modified() {\n",
            "        seen += step\n",
            "        return seen\n",
            "    }\n",
            "    when scheduled() { return seen }\n",
            "    stop { }\n",
            "}\n",
            "fn disordered(value: i64) -> i64 {\n",
            "    when { return value }\n",
            // A declaration after a hook.
            "    state late: i64 = 0\n",
            "    start { }\n",
            // A second `start`.
            "    start { }\n",
            "    stop { }\n",
            "    stop { }\n",
            "}\n",
            "fn mixed(value: i64) -> i64 {\n",
            // Neither a declaration nor a hook.
            "    let doubled = 2\n",
            "    when { return value }\n",
            "    announce(value)\n",
            "}\n",
            "fn nested(flag: bool, value: i64) -> i64 {\n",
            "    when {\n",
            "        state inner: i64 = 0\n",
            "        return value\n",
            "    }\n",
            "    start {\n",
            "        when { }\n",
            "    }\n",
            "}\n",
            // No result for `out` to be.
            "fn sink(value: i64) {\n",
            "    inject out, logger\n",
            "    when { }\n",
            "}\n",
            // An anonymous function is a function of its own (FN-14):
            // what it holds is no declaration of the node's inside a
            // block. In a node it is value code (FN-3), which has no
            // capability (FN-9), no state and no hooks.
            "fn helped(value: i64) -> i64 {\n",
            "    start {\n",
            "        let helper = fn() { inject logger }\n",
            "        let keeper = fn() { state seen: i64 = 0 }\n",
            "    }\n",
            "    when { return value }\n",
            "}\n",
        ),
    )]);
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:16:5 phase/node.declaration_order TIME-5",
            "m.hgl:18:5 phase/node.duplicate_hook TIME-5",
            "m.hgl:20:5 phase/node.duplicate_hook TIME-5",
            "m.hgl:23:5 phase/node.statement FN-2",
            "m.hgl:25:5 name/name.unknown UNIT-10",
            "m.hgl:25:5 phase/node.statement FN-2",
            "m.hgl:29:9 phase/node.nested TIME-5",
            "m.hgl:33:9 phase/node.nested TIME-3",
            "m.hgl:37:12 injectable/inject.out_without_result TIME-7",
            "m.hgl:42:29 phase/node.in_value_function FN-9",
            "m.hgl:43:29 phase/node.in_value_function FN-3",
        ]
    );
}

/// FN-1: "A `const fn` is a value function": it has no state and no
/// hooks. FN-9: "Capabilities belong to nodes: a node declares each
/// with `inject` and uses it in its hooks ... A value function has
/// none. A value it needs from one, the evaluation time say, is its
/// argument". NATIVE-3: "A native const implementation stays
/// value-level and admits neither `inject` nor temporal lifecycle
/// hooks." NATIVE-5: "A native value function injects nothing".
#[test]
fn a_value_function_has_no_state_no_capability_and_no_hooks() {
    let checked = check(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "const fn total(value: i64) -> i64 {\n",
            "    state seen: i64 = 0\n",
            "    when { return value }\n",
            "    return value\n",
            "}\n",
            "operator pass<V>(value: V) -> V\n",
            "impl const fn pass(value: i64) -> i64 {\n",
            "    inject logger\n",
            "    start { }\n",
            "    return value\n",
            "}\n",
            // What it needs of a capability is its argument.
            "const fn stamped(at: datetime, value: f64) -> tuple<datetime, f64> => (at, value)\n",
            "fn stamp(value: f64) -> tuple<datetime, f64> {\n",
            "    inject clock\n",
            "    when { return stamped(clock.evaluation_time, value) }\n",
            "}\n",
            // A native value function's contract.
            "native const fn now() -> datetime\n",
            "native const fn now() -> datetime { inject clock }\n",
        ),
    )]);
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:5 phase/node.in_value_function FN-1",
            "m.hgl:4:5 phase/node.in_value_function FN-1",
            "m.hgl:9:5 phase/node.in_value_function FN-9",
            "m.hgl:10:5 phase/node.in_value_function FN-1",
            "m.hgl:19:37 phase/node.in_value_function FN-9",
        ]
    );
}

/// SOURCE-1, FN-1 and FN-2. An anonymous body cannot yield even where
/// its enclosing function is a source. A named value function cannot
/// yield either. A source's own yields are valid.
#[test]
fn yield_requires_a_source_body() {
    let checked = check(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "fn source() -> i64 {\n",
            "    (fn() { yield 1s: 2 })()\n",
            "    yield 1s: 7\n",
            "}\n",
            "const fn value() -> i64 {\n",
            "    (fn() { yield 1s: 2 })()\n",
            "    return 0\n",
            "}\n",
            "fn hooked(input: i64) -> i64 {\n",
            "    when {\n",
            "        (fn() { yield 1s: 2 })()\n",
            "        return input\n",
            "    }\n",
            "}\n",
            "fn outputless() {\n",
            "    (fn() { yield 1s: 2 })()\n",
            "}\n",
            "const fn direct() -> i64 {\n",
            "    yield 1s: 2\n",
            "    return 0\n",
            "}\n",
            "fn valid_source() -> i64 {\n",
            "    yield 1s: 23\n",
            "}\n",
        ),
    )]);
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:13 phase/yield.outside_source SOURCE-1",
            "m.hgl:7:13 phase/yield.outside_source SOURCE-1",
            "m.hgl:12:17 phase/yield.outside_source SOURCE-1",
            "m.hgl:17:13 phase/yield.outside_source SOURCE-1",
            "m.hgl:20:5 phase/yield.outside_source FN-1",
        ]
    );
}

/// SOURCE-1: "A source declares a result, which is what it exists to
/// publish: one with no result arrow rejects."
#[test]
fn a_source_requires_a_result() {
    let checked = check(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "fn ticking() {\n",
            "    yield 1s: 7\n",
            "    yield 1s: 9\n",
            "}\n",
            "fn repeating() { while { yield 1s: 7 } }\n",
            "fn source() -> i64 { yield 1s: 7 }\n",
            "fn sink(value: i64) { when { } }\n",
            "fn graph(value: i64) { sink(value) }\n",
        ),
    )]);
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:2:4 phase/source.result SOURCE-1",
            "m.hgl:6:4 phase/source.result SOURCE-1",
        ],
        "{}",
        checked.report
    );
}

/// SOURCE-1 checks the missing arrow. The parser reports a missing type
/// after a written arrow (ERR-9).
#[test]
fn a_source_with_a_malformed_result_has_only_its_parse_error() {
    let checked = check_broken(&[("m.hgl", "module m\nfn ticking() -> { yield 1s: 7 }\n")]);
    assert_eq!(checked.front_end_errors, 1, "{}", checked.report);
    assert!(checked.summary().is_empty(), "{}", checked.report);
}

/// SOURCE-1: "A temporal `fn` whose body has a `yield` is a source: a
/// node that produces its output and has no temporal parameters. It
/// injects `clock`, `logger`, and `global_state`, and not `out`,
/// `scheduler`, or `alarm` ... A source with no declaration and no hook
/// has a body of statements, `yield` among them. One with an `inject`,
/// a `cache`, a `state`, a `start`, or a `stop` has its declarations,
/// then `start`, then one block that holds every `yield`, then `stop`,
/// and nothing else; the block is `{ ... }`, a `while`, or a `for` ...
/// a source declares no `var`. A source has no `when`."
#[test]
fn a_source_has_its_own_shape() {
    // The chapter's `examples.source`, each of its four forms.
    let accepted = check(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "fn two_values() -> i64 {\n",
            "    yield 1s: 7\n",
            "    yield 1s: 9\n",
            "}\n",
            "fn greeting(const every: duration = 1s) -> str {\n",
            "    inject logger\n",
            "    start { log_info(logger, \"greeting starts\") }\n",
            "    {\n",
            "        yield every: \"hello\"\n",
            "        yield every: \"world\"\n",
            "    }\n",
            "    stop { log_info(logger, \"greeting ends\") }\n",
            "}\n",
            "fn counter(const every: duration = 1s, const limit: i64 = 3) -> i64 {\n",
            "    inject logger\n",
            "    cache count: i64 = 0\n",
            "    while count < limit {\n",
            "        count += 1\n",
            "        yield every: count\n",
            "    }\n",
            "    stop { log_info(logger, str(count)) }\n",
            "}\n",
            "fn replay(const values: list<f64>, const every: duration = 1s) -> f64 {\n",
            "    inject logger\n",
            "    start { log_info(logger, \"replay starts\") }\n",
            "    for value in values {\n",
            "        let doubled = value * 2.0\n",
            "        yield every: doubled\n",
            "    }\n",
            "    stop { log_info(logger, \"replay ends\") }\n",
            "}\n",
        ),
    )]);
    assert_eq!(accepted.summary(), [""; 0]);

    let checked = check(&[(
        "m.hgl",
        concat!(
            "module m\n",
            // With no declaration and no hook: a temporal parameter, a
            // `var` and a `when`.
            "fn plain(value: i64, const every: duration = 1s) -> i64 {\n",
            "    var seen = 0\n",
            "    yield every: 1\n",
            "    when { }\n",
            "}\n",
            "fn shaped(const every: duration = 1s) -> i64 {\n",
            "    inject logger, out, scheduler\n",
            // Outside the block, in a hook, and a statement of no
            // place.
            "    yield every: 0\n",
            "    start { yield every: 1 }\n",
            "    let loose = 1\n",
            "    {\n",
            "        var inner = 1\n",
            "        yield every: 2\n",
            "    }\n",
            // A second block, a late declaration, a second `start` and
            // a second `stop`.
            "    while true {\n",
            "        yield every: 3\n",
            "    }\n",
            "    cache late: i64 = 0\n",
            "    stop { }\n",
            "    start { }\n",
            "    stop { }\n",
            "}\n",
            // `stop` before the block and `start` after it; a
            // declaration inside the block.
            "fn disordered(const every: duration = 1s) -> i64 {\n",
            "    stop { }\n",
            "    for value in [1, 2] {\n",
            "        cache nested: i64 = 0\n",
            "        yield every: value\n",
            "    }\n",
            "    start { }\n",
            "}\n",
        ),
    )]);
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:2:10 phase/source.input SOURCE-1",
            "m.hgl:3:5 phase/source.statement SOURCE-1",
            "m.hgl:5:5 phase/source.when SOURCE-1",
            "m.hgl:8:5 phase/source.statement SOURCE-1",
            "m.hgl:9:5 phase/source.statement SOURCE-1",
            "m.hgl:10:13 phase/source.statement SOURCE-1",
            "m.hgl:11:5 phase/source.statement SOURCE-1",
            "m.hgl:13:9 phase/source.statement SOURCE-1",
            "m.hgl:16:5 phase/source.statement SOURCE-1",
            "m.hgl:19:5 phase/node.declaration_order TIME-5",
            "m.hgl:21:5 phase/node.duplicate_hook TIME-5",
            "m.hgl:22:5 phase/node.duplicate_hook TIME-5",
            "m.hgl:26:5 phase/source.statement SOURCE-1",
            "m.hgl:27:9 phase/node.nested TIME-5",
            "m.hgl:30:5 phase/source.statement SOURCE-1",
        ]
    );
    // Each is a node all the same (FN-2).
    let module = checked.program.module("m").expect("module m exists");
    assert!(
        module
            .phases
            .iter()
            .all(|phase| phase.map(|phase| phase.as_str()) == Some("node"))
    );
}

/// TST-3: "A test is a composition-phase block with assertions. It
/// cannot itself declare state/cache, inject, lifecycle hooks, when,
/// yield, or return." TST-10: "`assert raises(\"code\")` requires one
/// literal catalogued execution code."
#[test]
fn a_test_is_a_composition_block_with_assertions() {
    let checked = check(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "fn forward(value: i64) -> i64 {\n",
            "    when { return value }\n",
            "}\n",
            // Assertions, bindings, and a block that is expected to fail.
            "test accepted {\n",
            "    let outcome = eval forward(value: [7, _, 9])\n",
            "    assert outcome == [7, _, 9]\n",
            "    assert raises(\"value.missing_key\") {\n",
            "        let inner = eval forward(value: [1])\n",
            "    }\n",
            "}\n",
            "test shaped {\n",
            "    state seen: i64 = 0\n",
            "    cache step: i64 = 1\n",
            "    inject logger\n",
            "    start { }\n",
            "    when { }\n",
            "    stop { }\n",
            "    yield 1s: 1\n",
            "    return\n",
            // In a block of the test, and in the block of a `raises`.
            "    if true { return }\n",
            "    assert raises(\"value.missing_key\") {\n",
            "        state inner: i64 = 0\n",
            "    }\n",
            "}\n",
            // The code: a source code, a name the catalogue has not, a
            // value that is no literal, and none.
            "test coded {\n",
            "    let code = \"value.missing_key\"\n",
            "    assert raises(\"type.mismatch\") { }\n",
            "    assert raises(\"value.banana\") { }\n",
            "    assert raises(code) { }\n",
            "    assert raises(1) { }\n",
            "    assert raises(\"value\\u{2e}missing_key\") { }\n",
            "}\n",
            // Each of these is an execution code of the catalogue, a
            // result out of range and an index outside a list (EXPR-3,
            // EXPR-7) among them, and is accepted.
            "test catalogued {\n",
            "    assert raises(\"numeric.division_by_zero\") { }\n",
            "    assert raises(\"numeric.out_of_range\") { }\n",
            "    assert raises(\"value.index_out_of_range\") { }\n",
            "}\n",
        ),
    )]);
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:13:5 phase/test.statement_phase TST-3",
            "m.hgl:14:5 phase/test.statement_phase TST-3",
            "m.hgl:15:5 phase/test.statement_phase TST-3",
            "m.hgl:16:5 phase/test.statement_phase TST-3",
            "m.hgl:17:5 phase/test.statement_phase TST-3",
            "m.hgl:18:5 phase/test.statement_phase TST-3",
            "m.hgl:19:5 phase/test.statement_phase TST-3",
            "m.hgl:20:5 phase/test.statement_phase TST-3",
            "m.hgl:21:15 phase/test.statement_phase TST-3",
            "m.hgl:23:9 phase/test.statement_phase TST-3",
            "m.hgl:28:19 type/test.raises_code TST-10",
            "m.hgl:29:19 type/test.raises_code TST-10",
            "m.hgl:30:19 type/test.raises_code TST-10",
            "m.hgl:31:19 type/test.raises_code TST-10",
        ]
    );
}

/// TST-10: the parser reports an absent raises argument once. Analyser
/// validation applies to a supplied argument.
#[test]
fn a_missing_raises_argument_is_reported_once() {
    for (tail, expected) in [
        (" {\n    boom(3)\n}\n}", "expected `(`, found `{`"),
        (" }", "expected `(`, found `}`"),
        ("", "expected `(`, found end of file"),
        (" == 1 }", "expected `(`, found `==`"),
        ("() {} }", "expected an expression, found `)`"),
    ] {
        let source = format!("module m\nconst fn boom(n: i64) {{}}\ntest t {{ assert raises{tail}");
        let checked = check_broken(&[("m.hgl", &source)]);
        assert_eq!(checked.front_end_errors, 1, "{}", checked.report);
        assert!(checked.findings.is_empty(), "{}", checked.report);
        assert!(
            checked
                .report
                .contains(&format!("error[parse/syntax.expected_token]: {expected}")),
            "{}",
            checked.report
        );
    }
}
