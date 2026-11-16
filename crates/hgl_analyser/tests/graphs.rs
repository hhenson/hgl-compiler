//! The analyser against the rules on a graph's body: composition. Each
//! test cites its rule.
//!
//! A finding is written `file:line:column category/code rule`.

mod common;

use common::{Checked, check};
use hgl_analyser::hir::{Body, Callee, ExprKind, Stmt};
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
/// that is a connection.
fn locals(checked: &Checked, name: &str) -> Vec<String> {
    body(checked, name)
        .locals
        .iter()
        .map(|local| {
            let connection = if local.connection {
                " (connection)"
            } else {
                ""
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
