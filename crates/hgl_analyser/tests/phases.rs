//! The analyser against the rules on what a temporal function's body
//! describes. Each test cites its rule.

mod common;

use common::check;

/// FN-2: "An ordinary temporal body with no node-only construct
/// describes composition: the function is a graph. `state`, `cache`,
/// `inject`, `start`, `when`, `stop`, or `yield` classifies that
/// complete body as runtime: the function is a node. Classification
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
            // An anonymous function is a function of its own (FN-14): a
            // value helper may inject a capability (FN-9), and that is
            // no declaration of the node's inside a block.
            "fn helped(value: i64) -> i64 {\n",
            "    start {\n",
            "        let helper = fn() { inject logger }\n",
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
            "m.hgl:25:5 phase/node.statement FN-2",
            "m.hgl:29:9 phase/node.nested TIME-5",
            "m.hgl:33:9 phase/node.nested TIME-3",
            "m.hgl:37:12 injectable/inject.out_without_result TIME-7",
        ]
    );
}

/// FN-1: "A `const fn` is a value function." It has no state and no
/// hooks. FN-9: "Injection does not turn a value function into a node":
/// it may inject a capability.
#[test]
fn a_value_function_has_no_state_and_no_hooks() {
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
        ),
    )]);
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:5 phase/node.in_value_function FN-1",
            "m.hgl:4:5 phase/node.in_value_function FN-1",
            "m.hgl:10:5 phase/node.in_value_function FN-1",
        ]
    );
}
