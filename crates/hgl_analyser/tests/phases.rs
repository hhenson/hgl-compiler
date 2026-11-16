//! The analyser against the rules on what a temporal function's body
//! describes. Each test cites its rule.

mod common;

use common::check;

/// No lines.
const NONE: [&str; 0] = [];

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
    assert_eq!(checked.summary(), NONE);
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
