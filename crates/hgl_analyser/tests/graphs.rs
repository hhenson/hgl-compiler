//! The analyser against the rules on a graph's body: composition. Each
//! test cites its rule.
//!
//! A finding is written `file:line:column category/code rule`.

mod common;

use common::{Checked, body, module};
use hgl_analyser::hir::{Callee, ExprKind, Stmt, ValueCategory};
use hgl_analyser::{DeclarationKind, Role};

/// No lines.
const NONE: [&str; 0] = [];

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
            Callee::Required { .. }
            | Callee::Parameter { .. }
            | Callee::Anonymous(_)
            | Callee::DeferredValue
            | Callee::Unread => {
                panic!("a callee")
            }
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
        // CTRL-13 and ERR-12: a nullable condition is read as bool.
        // A condition of another type and mismatched branches reject.
        "    let bad1 = if count { price } else { price }\n",
        "    let from_nullable = if maybe { price } else { price }\n",
        "    let bad3 = if open { price } else { \"text\" }\n",
        "    return picked\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:11:19 type/type.mismatch CTRL-13",
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

/// CTRL-13: with no `else` a branch that is a connection makes the `if`
/// a connection of `T`, under a temporal condition and under a
/// wiring-time one alike. So where a condition is not settled, an
/// operation the graph requires given no connection (GEN-7), the type
/// still is.
#[test]
fn an_unsettled_condition_keeps_a_connections_type() {
    let checked = module(concat!(
        "fn gates<T>(price: f64, const sample: T) -> str requires eq_(T, T) -> bool {\n",
        "    let gated = if eq_(sample, sample) { price }\n",
        "    let fixed = if eq_(sample, sample) { 1.0 }\n",
        "    return gated\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), ["m.hgl:5:12 type/type.mismatch EXPR-5"]);
    assert_eq!(
        locals(&checked, "gates")[2..],
        ["gated: f64 (connection)", "fixed: _ (unsettled)"]
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
        // One `if` selected at runtime inside another: a `var` of the
        // outer branch is declared before the inner `if`.
        "fn nested(price: f64, rising: bool, open: bool) -> f64 {\n",
        "    if rising {\n",
        "        var level = 1.0\n",
        "        level = 2.0\n",
        "        if open { level = 3.0 }\n",
        "        if open {\n",
        "            var deeper = 1.0\n",
        "            deeper = 2.0\n",
        "        }\n",
        "    }\n",
        "    return price\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:17:13 type/binding.category CTRL-14",
            "m.hgl:19:12 type/binding.category CTRL-14",
            "m.hgl:20:25 type/binding.category CTRL-14",
            "m.hgl:27:12 name/binding.unassigned EXPR-3",
            "m.hgl:33:27 type/binding.category CTRL-14",
        ]
    );
    assert_eq!(locals(&checked, "late")[2..], ["chosen: f64 (connection)"]);
}

/// EXPR-15: "In a graph, an assignment into a place of a `var` is copy
/// on write. A graph's `var` denotes a connection (EXPR-4), and a field
/// of a connection is no storage: `point.x = price` rebinds `point` to
/// a new connection, which is `point` with its `x` from `price` ... The
/// place, the type of its value, and a compound assignment are as
/// EXPR-3 has them."
///
/// It is an assignment to the `var`, and is held as one: to the `var`'s
/// category (CTRL-14), and to what a loop that makes one child for each
/// member admits (CTRL-15).
#[test]
fn a_place_assigned_in_a_graph_rebinds_its_var() {
    let checked = module(concat!(
        "struct Point {\n    x: f64\n    y: f64\n}\n",
        "fn log_tick(value: f64) {\n    when { }\n}\n",
        // A connection stays one, whatever its place is given: another
        // connection, a compound assignment, an ordinary value.
        "fn shifted(first: Point, price: f64, rising: bool) -> Point {\n",
        "    var point = first\n",
        "    point.x = price\n",
        "    point.y += price\n",
        "    point.y = 2.0\n",
        // In a branch selected at runtime, as the whole `var` is.
        "    if rising { point.x = price }\n",
        "    return point\n",
        "}\n",
        // An ordinary value: its place takes an ordinary value, and a
        // connection would make it one; so would a branch selected at
        // runtime.
        "fn fixed(price: f64, rising: bool) -> f64 {\n",
        "    var point = Point(x: 1.0, y: 2.0)\n",
        "    point.y = 3.0\n",
        "    point.x = price\n",
        "    if rising { point.y = 4.0 }\n",
        "    return price\n",
        "}\n",
        // A `let` and a parameter are read-only all the way down, and
        // the value is of the place's type.
        "fn held(first: Point, price: f64, flag: bool) -> Point {\n",
        "    let kept = first\n",
        "    kept.x = price\n",
        "    first.y = price\n",
        "    var point = first\n",
        "    point.x = flag\n",
        "    return point\n",
        "}\n",
        // An index is part of what the `var` becomes: one that is a
        // connection selects the element as the graph runs.
        "fn indexed(index: i64, const slot: i64, price: f64, levels: list<f64, 2>) -> f64 {\n",
        "    var row = [1.0, 2.0]\n",
        "    row[slot] = 3.0\n",
        "    row[index] = 3.0\n",
        "    row[index] += 1.0\n",
        "    var live = levels\n",
        "    live[index] = price\n",
        "    return price\n",
        "}\n",
        // One child for each member: nothing is carried out of it.
        "fn each(first: Point, growing: list<f64>) -> Point {\n",
        "    var point = first\n",
        "    for item in growing {\n",
        "        point.x = item\n",
        "        var inner = first\n",
        "        inner.x = item\n",
        "        log_tick(inner.x)\n",
        "    }\n",
        "    return point\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:20:15 type/binding.category CTRL-14",
            "m.hgl:21:27 type/binding.category CTRL-14",
            "m.hgl:26:5 name/binding.immutable EXPR-3",
            "m.hgl:27:5 name/binding.immutable EXPR-3",
            "m.hgl:29:15 type/type.mismatch EXPR-5",
            // The index that is a connection, and the compound
            // assignment that reads the place it selects.
            "m.hgl:35:9 type/binding.category CTRL-14",
            "m.hgl:36:5 type/binding.category CTRL-14",
            "m.hgl:44:9 local:phase/loop.carried CTRL-15",
        ]
    );
    // The `var` is a connection before and after, and the assignment is
    // kept as one into its place.
    assert_eq!(
        locals(&checked, "shifted")[3..],
        ["point: Point (connection)"]
    );
    let places = body(&checked, "shifted")
        .statements
        .iter()
        .filter(|statement| matches!(statement, Stmt::AssignPlace { .. }))
        .count();
    assert_eq!(places, 3);
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
        // CTRL-10: a branch that returns gives the `if` no value, and so
        // takes no part in what it is: `level` is the ordinary 1.0.
        "fn sized(value: f64, const size: f64) -> f64 => value\n",
        "fn picks(price: f64, open: bool, const wide: bool) -> f64 {\n",
        "    let level = if wide { if open { return price } else { return price } } else { 1.0 }\n",
        "    return sized(price, level)\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), ["m.hgl:9:4 type/return.missing FN-7"]);
    assert_eq!(locals(&checked, "picks")[3..], ["level: f64"]);
}

/// GEN-7 and CTRL-14: a newly temporal return needs a value on its
/// continuation, using the same completeness check as a direct graph.
#[test]
fn an_instance_checks_its_temporal_return_continuation() {
    let checked = module(concat!(
        "operator predicate<T>(input: T) -> bool\n",
        "impl const fn predicate<T>(input: T) -> bool => true\n",
        "impl fn predicate(input: i64) -> bool { when { return input > 0 } }\n",
        "fn partial<T>(const input: T) -> i64 {\n",
        "    if predicate(input) { return 1 }\n",
        "}\n",
        "fn continued<T>(const input: T) -> i64 {\n",
        "    if predicate(input) { return 1 }\n",
        "    return 2\n",
        "}\n",
        "fn both<T>(const input: T) -> i64 {\n",
        "    if predicate(input) { return 1 } else { return 2 }\n",
        "}\n",
        "fn checked_in_source<T>(const input: T, ready: bool) -> i64 {\n",
        "    let chosen = predicate(input)\n",
        "    if ready { return 1 }\n",
        "}\n",
        "fn node_control(ready: bool) -> i64 {\n",
        "    when { if ready { return 1 } }\n",
        "}\n",
        "fn live_partial() -> i64 => partial(1)\n",
        "fn wiring_partial() -> i64 => partial(\"s\")\n",
        "fn live_continued() -> i64 => continued(1)\n",
        "fn live_both() -> i64 => both(1)\n",
        "fn direct_once() -> i64 => checked_in_source(1, true)\n",
        "fn unused<T>(const input: T) -> i64 {\n",
        "    if predicate(input) { return 1 }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:15:4 type/return.missing FN-7",
            "m.hgl:22:29 type/return.missing FN-7",
        ],
        "{}",
        checked.report
    );
}

/// CTRL-6 and CTRL-14: a returning final switch can complete a graph
/// when its selector settles to an enum whose members it covers.
#[test]
fn an_instance_can_complete_its_return_in_a_final_switch() {
    let checked = module(concat!(
        "operator predicate<T>(input: T) -> bool\n",
        "impl const fn predicate<T>(input: T) -> bool => true\n",
        "impl fn predicate(input: i64) -> bool { when { return input > 0 } }\n",
        "enum Side { left, right }\n",
        "fn covered<T>(const input: T) -> i64 {\n",
        "    if predicate(input) { return 1 }\n",
        "    let side = if predicate(input) { Side.left }\n",
        "    switch side {\n",
        "        case Side.left: return 2\n",
        "        case Side.right: return 3\n",
        "    }\n",
        "}\n",
        "fn uncovered<T>(const input: T) -> i64 {\n",
        "    if predicate(input) { return 1 }\n",
        "    let side = if predicate(input) { Side.left }\n",
        "    switch side { case Side.left: return 2 }\n",
        "}\n",
        "fn falls_through<T>(const input: T) -> i64 {\n",
        "    if predicate(input) { return 1 }\n",
        "    let side = if predicate(input) { Side.left }\n",
        "    switch side {\n",
        "        case Side.left: return 2\n",
        "        case Side.right: let unused = 3\n",
        "    }\n",
        "}\n",
        "fn live_covered() -> i64 => covered(1)\n",
        "fn live_uncovered() -> i64 => uncovered(1)\n",
        "fn live_falls_through() -> i64 => falls_through(1)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:28:31 type/return.missing FN-7",
            "m.hgl:29:35 type/return.missing FN-7",
        ],
        "{}",
        checked.report
    );
}

/// CTRL-6 and CTRL-14: every arm of the final `if` must complete the
/// continuation, including arms which end in a deferred switch.
#[test]
fn an_instance_can_complete_its_return_in_both_tail_branches() {
    let checked = module(concat!(
        "operator predicate<T>(input: T) -> bool\n",
        "impl const fn predicate<T>(input: T) -> bool => true\n",
        "impl fn predicate(input: i64) -> bool { when { return input > 0 } }\n",
        "enum Side { left, right }\n",
        "fn covered<T>(const input: T, const use_switch: bool) -> i64 {\n",
        "    if predicate(input) { return 1 }\n",
        "    let side = if predicate(input) { Side.left }\n",
        "    if use_switch {\n",
        "        switch side {\n",
        "            case Side.left: return 2\n",
        "            case Side.right: return 3\n",
        "        }\n",
        "    } else { return 4 }\n",
        "}\n",
        "fn falls_through<T>(const input: T, const use_switch: bool) -> i64 {\n",
        "    if predicate(input) { return 1 }\n",
        "    let side = if predicate(input) { Side.left }\n",
        "    if use_switch {\n",
        "        switch side {\n",
        "            case Side.left: return 2\n",
        "            case Side.right: return 3\n",
        "        }\n",
        "    } else { let unused = 4 }\n",
        "}\n",
        "fn live_covered() -> i64 => covered(1, true)\n",
        "fn live_falls_through() -> i64 => falls_through(1, false)\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:27:35 type/return.missing FN-7"],
        "{}",
        checked.report
    );
}

/// CTRL-15: "`for item in items` over an ordinary list, set, or
/// dictionary composes its body once for each element, with ordinary
/// values bound, and may carry a `var` from one pass to the next. Over a
/// connection of `list<T, N>` it composes its body `N` times, `item` a
/// connection of `T`; over a connection of a tuple it rejects. Over a
/// connection of `list<T>`, `set<T>`, or `dict<K, V>` it makes one child
/// for each member (CTRL-7): `item` is a connection of `T` for a list
/// and an ordinary `T`, the member itself, for a set; `key` is an
/// ordinary `K` and `value` a connection of `V` for a dictionary. In
/// such a loop an assignment to a `var` declared outside it rejects
/// (CTRL-8), and so does a `return`."
#[test]
fn a_loop_in_a_graph_binds_values_or_connections() {
    let checked = module(concat!(
        "fn log_tick(value: f64) {\n",
        "    when { }\n",
        "}\n",
        "fn loops(\n",
        "    price: f64,\n",
        "    const factors: list<f64>,\n",
        "    fixed: list<f64, 3>,\n",
        "    growing: list<f64>,\n",
        "    names: set<str>,\n",
        "    book: dict<str, f64>,\n",
        "    pair: tuple<f64, f64>,\n",
        ") -> f64 {\n",
        "    var running = price\n",
        // Ordinary data: unrolled while the graph is built.
        "    for factor in factors {\n",
        "        running = running * factor\n",
        "    }\n",
        // A fixed list of connections: composed three times.
        "    for slot in fixed {\n",
        "        running = running + slot\n",
        "    }\n",
        // One child for each member: nothing is carried, and nothing
        // returns.
        "    for item in growing {\n",
        "        var scaled = item\n",
        "        scaled = scaled * 2.0\n",
        "        log_tick(scaled)\n",
        "        running = running + item\n",
        "        return item\n",
        "    }\n",
        "    for name in names {\n",
        "        log_tick(price)\n",
        "    }\n",
        "    for key, value in book {\n",
        "        log_tick(value)\n",
        "    }\n",
        "    for piece in pair {\n",
        "        log_tick(price)\n",
        "    }\n",
        "    return running\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:25:9 local:phase/loop.carried CTRL-15",
            "m.hgl:26:9 local:phase/loop.carried CTRL-15",
            "m.hgl:34:18 type/type.mismatch CTRL-15",
        ]
    );
    // A loop nested in another has a boundary of its own: a `var` of
    // the outer loop's body is outside the inner one. A loop over
    // ordinary data inside keeps the boundary of the loop it stands in.
    let nested = module(concat!(
        "fn nested(rows: list<f64>, columns: list<f64>, const weights: list<f64>) {\n",
        "    for row in rows {\n",
        "        var total = row\n",
        "        for weight in weights {\n",
        "            total = total * weight\n",
        "        }\n",
        "        for column in columns {\n",
        "            var inner = column\n",
        "            inner = inner + row\n",
        "            total = total + column\n",
        "        }\n",
        "        total = total + row\n",
        "    }\n",
        "}\n",
    ));
    assert_eq!(
        nested.summary(),
        ["m.hgl:11:13 local:phase/loop.carried CTRL-15"]
    );
    let bound: Vec<String> = locals(&checked, "loops")
        .into_iter()
        .filter(|local| {
            ["factor", "slot", "item", "name", "key", "value"]
                .iter()
                .any(|name| local.starts_with(&format!("{name}:")))
        })
        .collect();
    assert_eq!(
        bound,
        [
            "factor: f64",
            "slot: f64 (connection)",
            "item: f64 (connection)",
            "name: str",
            "key: str",
            "value: f64 (connection)",
        ]
    );
}

/// CTRL-15: "In composition a `switch` over an ordinary value composes
/// the one case selected, and over a connection each case is a child
/// graph selected at runtime. The selector is a `bool`, `i64`, `str`, or
/// enum value and each label a constant (CTRL-12); a case's assignments
/// and returns follow CTRL-14." FN-3: "Where a constant is required, a
/// `switch` label ... a connection rejects."
#[test]
fn a_switch_in_a_graph_composes_or_selects() {
    let checked = module(concat!(
        "enum Side { buy, sell }\n",
        "fn picks(price: f64, side: Side, const mode: i64, level: i64) -> f64 {\n",
        "    var chosen = price\n",
        "    var size = 1.0\n",
        // An ordinary selector: one case is composed.
        "    switch mode {\n",
        "        case 1: size = 2.0\n",
        "        default: size = 3.0\n",
        "    }\n",
        // A connection: each case is selected at runtime.
        "    switch side {\n",
        "        case .buy: chosen = price * 2.0\n",
        "        case .sell: size = 4.0\n",
        "    }\n",
        // A label that is a connection, and a selector that is no
        // `bool`, `i64`, `str` or enum.
        "    switch mode {\n",
        "        case level: chosen = price\n",
        "        default: chosen = price\n",
        "    }\n",
        "    switch price {\n",
        "        default: chosen = price\n",
        "    }\n",
        "    return chosen\n",
        "}\n",
        // CTRL-14: a `return` in a case selected at runtime needs the
        // continuation to return too.
        "fn early(price: f64, side: Side) -> f64 {\n",
        "    switch side {\n",
        "        case .buy: return price\n",
        "    }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:12:28 type/binding.category CTRL-14",
            "m.hgl:15:14 phase/connection.as_constant FN-3",
            "m.hgl:18:12 type/type.mismatch CTRL-15",
            "m.hgl:23:4 type/return.missing FN-7",
        ]
    );
}

/// CTRL-15: "The condition of a `while` is an ordinary `bool`, tested
/// while the graph is built: a connection there rejects (FN-3), and the
/// form with no condition is a source's (SOURCE-4)."
#[test]
fn a_while_in_a_graph_takes_an_ordinary_condition() {
    let checked = module(concat!(
        "fn repeats(price: f64, open: bool, const count: i64) -> f64 {\n",
        "    var running = price\n",
        "    var left = count\n",
        "    while left > 0 {\n",
        "        running = running * 2.0\n",
        "        left -= 1\n",
        "    }\n",
        "    while open {\n",
        "        running = running * 2.0\n",
        "    }\n",
        "    while {\n",
        "        running = running * 2.0\n",
        "    }\n",
        "    return running\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:9:11 phase/connection.as_constant FN-3",
            "m.hgl:12:5 phase/while.condition CTRL-15",
        ]
    );
}
