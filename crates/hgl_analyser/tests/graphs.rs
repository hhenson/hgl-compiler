//! The analyser against the rules on a graph's body: composition. Each
//! test cites its rule.
//!
//! A finding is written `file:line:column category/code rule`.

mod common;

use common::{Checked, check};
use hgl_analyser::hir::{Body, Callee, ExprKind, Stmt, ValueCategory};
use hgl_analyser::{DeclarationKind, Role};

/// No lines.
const NONE: [&str; 0] = [];

/// Checks one file, `m.hgl`, of module `m`. The `module` line is added,
/// so the text given starts on line 2.
fn module(text: &str) -> Checked {
    check(&[("m.hgl", &format!("module m\n{text}"))])
}

/// The typed body of the function `name` of module `m`.
fn body<'a>(checked: &'a Checked, name: &str) -> &'a Body {
    let module = checked.program.module("m").expect("module m exists");
    let index = module
        .declarations
        .iter()
        .position(|declaration| declaration.name == name)
        .expect("the declaration exists");
    module.bodies[index].as_ref().expect("it has a body")
}

/// The locals of `name` as `local: type`, with ` (connection)` after one
/// that is a connection and ` (unsettled)` after one that may be.
fn locals(checked: &Checked, name: &str) -> Vec<String> {
    body(checked, name)
        .locals
        .iter()
        .map(|local| {
            let connection = match local.category {
                ValueCategory::Connection => " (connection)",
                ValueCategory::Unsettled => " (unsettled)",
                ValueCategory::Value => "",
            };
            format!("{}: {}{connection}", local.name, local.ty)
        })
        .collect()
}

/// FN-3: "There a temporal parameter, and the result of a temporal or
/// lifted call, is a connection: the argument of a call, the operand of
/// a symbol, a condition, a bound local, or what is returned ... An
/// ordinary value of type T is accepted where composition expects a
/// connection of T, and is wired as a constant". FN-6: a value helper
/// "with temporal arguments in composition is lifted ... All-scalar
/// calls execute directly". EXPR-3: "Initialization fixes a binding's
/// value/connection category". EXPR-4: "Ordinary values and temporal
/// connections remain distinct. A graph binding denotes a connection
/// without reading a payload."
#[test]
fn a_graphs_values_are_connections_or_ordinary_values() {
    let checked = module(concat!(
        "const fn scale(value: f64, factor: f64 = 2.0) -> f64 => value * factor\n",
        "fn scaled(value: f64, factor: f64 = 2.0) -> f64 => value * factor\n",
        "fn priced(price: f64, const lots: i64) -> f64 {\n",
        // A value helper given a connection is lifted.
        "    let lifted = scale(price)\n",
        // Given ordinary values alone it is called while the graph is
        // built.
        "    let wired = scale(2.0, 3.0)\n",
        // A temporal function's result is a connection, and `3.0` is
        // wired to its input as a constant.
        "    let nested = scaled(lifted, 3.0)\n",
        // A symbol is its call: `add_` for two `f64`, lifted.
        "    let sum = nested + wired\n",
        "    let fixed = lots * 2\n",
        "    return sum\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(
        locals(&checked, "priced"),
        [
            "price: f64 (connection)",
            "lots: i64",
            "lifted: f64 (connection)",
            "wired: f64",
            "nested: f64 (connection)",
            "sum: f64 (connection)",
            "fixed: i64",
        ]
    );
}

/// FN-3: "A composition body calls any function". FN-4: "Composition
/// and eval choose the temporal definition if one exists". GEN-5: "A
/// call in composition selects among all of them ... There, a temporal
/// candidate outranks a value candidate of the same parameter types".
#[test]
fn composition_takes_the_temporal_definition_and_candidate() {
    let checked = module(concat!(
        "fn pair(value: f64) -> f64 => value\n",
        "const fn pair(value: f64) -> f64 => value\n",
        "operator smooth<V>(value: V) -> V\n",
        "impl fn smooth(value: f64) -> f64 => value\n",
        "impl const fn smooth(value: f64) -> f64 => value\n",
        "impl const fn smooth(value: i64) -> i64 => value\n",
        "fn graph(price: f64, count: i64) -> f64 {\n",
        "    let paired = pair(price)\n",
        "    let smoothed = smooth(price)\n",
        // The only candidate for an `i64` is a value one: it is lifted.
        "    let counted = smooth(count)\n",
        "    return smoothed\n",
        "}\n",
        // A value function takes the value definition and candidate.
        "const fn plain(price: f64) -> f64 {\n",
        "    let paired = pair(price)\n",
        "    return smooth(price)\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE);
    // The role of what each call of `name`'s statement `index` selects.
    let role = |name: &str, index: usize| {
        let value = match &body(&checked, name).statements[index] {
            Stmt::Declare {
                value: Some(value), ..
            } => value,
            Stmt::Return {
                value: Some(value), ..
            } => value,
            other => panic!("a statement with a value: {other:?}"),
        };
        let ExprKind::Call { callee, .. } = &value.kind else {
            panic!("a call: {:?}", value.kind);
        };
        let (module, declaration) = match callee {
            Callee::Function {
                module,
                declaration,
                ..
            }
            | Callee::Candidate {
                module,
                declaration,
                ..
            } => (*module, *declaration),
            Callee::Required { .. } | Callee::Unread => panic!("a callee"),
        };
        match checked.program.modules[module].declarations[declaration].kind {
            DeclarationKind::Function(role) | DeclarationKind::Implementation(role, _) => role,
            other => panic!("a function or a candidate: {other:?}"),
        }
    };
    assert_eq!(role("graph", 0), Role::Temporal);
    assert_eq!(role("graph", 1), Role::Temporal);
    assert_eq!(role("graph", 2), Role::Value);
    assert_eq!(role("plain", 0), Role::Value);
    assert_eq!(role("plain", 1), Role::Value);
    assert_eq!(
        locals(&checked, "graph")[2..],
        [
            "paired: f64 (connection)",
            "smoothed: f64 (connection)",
            "counted: i64 (connection)",
        ]
    );
}

/// FN-3: "Where a constant is required, a `switch` label, a `const`
/// parameter's argument, a generic argument, a size, or a tuple's
/// index, a connection rejects." GEN-8: what a call gives a generic
/// value parameter is a compile-time constant.
#[test]
fn a_connection_is_no_constant() {
    let checked = module(concat!(
        "fn sized(value: f64, const size: i64) -> f64 => value\n",
        "const fn window<const width: i64 = 3>(value: f64) -> f64 => value\n",
        "fn graph(price: f64, count: i64, const lots: i64, pair: tuple<f64, f64>) -> f64 {\n",
        "    let fixed = sized(price, lots)\n",
        "    let literal = sized(price, 4)\n",
        // A connection has no value while the graph is built.
        "    let moving = sized(price, count)\n",
        "    let lifted = sized(price, count + 1)\n",
        "    let wide = window(price, width: count)\n",
        "    let first = pair[count]\n",
        "    return fixed\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:7:31 phase/connection.as_constant FN-3",
            "m.hgl:8:31 phase/connection.as_constant FN-3",
            "m.hgl:9:37 type/generic.argument_constant GEN-8",
            "m.hgl:10:22 type/tuple.index EXPR-7",
        ]
    );
}

/// CTRL-1: "A wiring-time condition chooses which branch to compose. A
/// temporal condition in composition selects a child graph at runtime."
/// CTRL-13: "In composition the condition of an `if` is a `bool`, an
/// ordinary value or a connection: a `bool?` or any other type rejects.
/// An `if` whose value is used has the one type its branches' values
/// have, and two types reject. It is a connection when its condition is
/// one or either branch's value is ... With no `else`, under a temporal
/// condition it is a connection of `T` that never ticks while the
/// condition is false (CTRL-3): `T`, not `T?`. Under a wiring-time
/// condition a branch that is an ordinary value makes it a `T?` ... and
/// a branch that is a connection makes it a connection of `T`".
#[test]
fn an_if_in_a_graph_is_composed_or_selected() {
    let checked = module(concat!(
        "const fn scale(value: f64, factor: f64 = 2.0) -> f64 => value * factor\n",
        "fn uses(price: f64, open: bool, const wide: bool, count: i64, maybe: bool?) -> f64 {\n",
        // A wiring-time condition: one branch is composed.
        "    let sized = if wide { scale(price) } else { price }\n",
        "    let fixed = if wide { 1.0 } else { 2.0 }\n",
        // A temporal condition selects at runtime; an ordinary value in
        // a branch is wired as a constant.
        "    let picked = if open { scale(price) } else { price }\n",
        "    let wired = if open { 1.0 } else { 2.0 }\n",
        // With no `else`.
        "    let gated = if open { price }\n",
        "    let some = if wide { 1.0 }\n",
        "    let lone = if wide { price }\n",
        // A condition that is no `bool`, and branches of two types.
        "    let bad1 = if count { price } else { price }\n",
        "    let bad2 = if maybe { price } else { price }\n",
        "    let bad3 = if open { price } else { \"text\" }\n",
        "    return picked\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:11:19 type/type.mismatch CTRL-13",
            "m.hgl:12:19 type/type.mismatch CTRL-13",
            "m.hgl:13:39 type/type.mismatch CTRL-13",
        ]
    );
    assert_eq!(
        locals(&checked, "uses")[5..12],
        [
            "sized: f64 (connection)",
            "fixed: f64",
            "picked: f64 (connection)",
            "wired: f64 (connection)",
            "gated: f64 (connection)",
            "some: f64?",
            "lone: f64 (connection)",
        ]
    );
}

/// CTRL-14: "A `var` keeps the category of its first value (EXPR-3): a
/// connection assigned to one that holds an ordinary value, or the
/// reverse, rejects (EXPR-4). Under a temporal condition a `var`
/// declared before the `if` is assigned in a branch only where it is a
/// connection ... After the `if` the `var` is one connection: the
/// branch's binding while that branch is selected, and the binding it
/// had before for a branch that does not assign it (CTRL-4). With none
/// before, every branch that does not return assigns it."
#[test]
fn a_var_of_a_graph_keeps_its_category() {
    let checked = module(concat!(
        "const fn scale(value: f64, factor: f64 = 2.0) -> f64 => value * factor\n",
        "fn adjusted(price: f64, rising: bool) -> f64 {\n",
        "    var chosen = price\n",
        "    if rising { chosen = scale(price) }\n",
        "    return chosen\n",
        "}\n",
        // With no binding before, every branch assigns it.
        "fn late(price: f64, rising: bool) -> f64 {\n",
        "    var chosen: f64\n",
        "    if rising { chosen = scale(price) } else { chosen = price }\n",
        "    return chosen\n",
        "}\n",
        "fn clashes(price: f64, rising: bool, const wide: bool) -> f64 {\n",
        "    var level = 1.0\n",
        // An ordinary value to an ordinary value, and under a
        // wiring-time condition.
        "    level = 2.0\n",
        "    if wide { level = 3.0 }\n",
        // A connection to one that holds an ordinary value, and the
        // reverse.
        "    level = price\n",
        "    var held = price\n",
        "    held = 2.0\n",
        // An ordinary value chosen at runtime.
        "    if rising { level = 4.0 }\n",
        // A `var` of the branch itself is the branch's.
        "    if rising {\n",
        "        var inner = 1.0\n",
        "        inner = 2.0\n",
        "    }\n",
        // Assigned in one branch of two, and none before.
        "    var unset: f64\n",
        "    if rising { unset = price }\n",
        "    return unset\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:17:13 type/binding.category CTRL-14",
            "m.hgl:19:12 type/binding.category CTRL-14",
            "m.hgl:20:25 type/binding.category CTRL-14",
            "m.hgl:27:12 name/binding.unassigned EXPR-3",
        ]
    );
    assert_eq!(locals(&checked, "late")[2..], ["chosen: f64 (connection)"]);
}

/// CTRL-14: "A `return value` in a branch of a temporal `if` makes the
/// graph's result that value while the branch is selected and the
/// continuation's otherwise (CTRL-5): the continuation returns a value
/// too, and one that can reach its end with none rejects."
#[test]
fn a_return_selected_at_runtime_needs_its_continuation() {
    let checked = module(concat!(
        "fn early(price: f64, open: bool) -> f64 {\n",
        "    if open { return price }\n",
        "    return price * 2.0\n",
        "}\n",
        "fn both(price: f64, open: bool) -> f64 {\n",
        "    if open { return price } else { return price * 2.0 }\n",
        "}\n",
        "fn falls(price: f64, open: bool) -> f64 {\n",
        "    if open { return price }\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), ["m.hgl:9:4 type/return.missing FN-7"]);
}
