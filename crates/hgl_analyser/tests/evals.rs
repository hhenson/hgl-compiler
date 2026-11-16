//! The analyser against the rules on `eval` and on traces (TST-3 to
//! TST-8, TST-16, EXPR-8). Each test cites its rule.
//!
//! A finding is written `file:line:column category/code rule`.

mod common;

use common::{Checked, check};
use hgl_analyser::hir::{ExprKind, Stmt};

/// No lines.
const NONE: [&str; 0] = [];

/// The declarations each test evaluates: lines 2 to 30 of `m.hgl`.
const TARGETS: &str = concat!(
    "fn pass(value: i64) -> i64 {\n",
    "    when { return delta_value(value) }\n",
    "}\n",
    "fn scaled(left: i64, right: i64, const factor: i64) -> i64 {\n",
    "    when { return (left + right) * factor }\n",
    "}\n",
    "fn sink(value: i64) {\n",
    "    when { }\n",
    "}\n",
    "fn relay(enabled: bool, members: set<i64>) -> set<i64> {\n",
    "    when {\n",
    "        if enabled && modified(members) {\n",
    "            return delta_value(members)\n",
    "        }\n",
    "    }\n",
    "}\n",
    "fn same<T>(lhs: T, rhs: T) -> T requires add_(T, T) -> T => lhs + rhs\n",
    "const fn doubled(value: i64) -> i64 => value * 2\n",
    "const fn close(seen: list<i64?>, wanted: list<i64?>) -> bool => true\n",
    "const fn timed(seen: dict<duration, i64>) -> bool => true\n",
    "const fn maybe(value: i64?) -> i64? => value\n",
    "struct Kept {\n",
    "    values: list<i64?>\n",
    "}\n",
    "fn forward(value: i64) -> i64 => pass(value)\n",
    "struct Boxed<T> {\n",
    "    value: T\n",
    "}\n",
    "fn through<T>(value: T) -> T => value\n",
);

/// Checks one file, `m.hgl`, of module `m`: the `module` line, the
/// targets above, and `text`, which starts on line 31.
fn module(text: &str) -> Checked {
    check(&[("m.hgl", &format!("module m\n{TARGETS}{text}"))])
}

/// The locals of the test `name`, as `local: type`.
fn locals(checked: &Checked, name: &str) -> Vec<String> {
    let module = checked.program.module("m").expect("module m exists");
    let index = module
        .tests
        .iter()
        .position(|test| test.name == name)
        .expect("the test exists");
    module.test_bodies[index]
        .as_ref()
        .expect("it has a block")
        .locals
        .iter()
        .map(|local| format!("{}: {}", local.name, local.ty))
        .collect()
}

/// TST-5: "Eval selects a module function, binding arguments by its
/// public positional/named rules ... Const parameters receive
/// configuration; temporal parameters receive contextually typed
/// publication traces ... Each cell is one cycle's publication: it is
/// read as the `delta<T>` (TYPE-16) of its parameter's type T for an
/// input, and of the target's result type for an expectation." TST-3:
/// a trace "is bound with `let`, compared with `==` or `!=` against a
/// trace written out or another eval". TST-8: an outputless target's
/// eval "stands as a statement".
#[test]
fn an_eval_is_the_call_of_its_target_with_traces() {
    let checked = module(concat!(
        "test evaluated {\n",
        // Dense, with `_`; all `_`; none. Bound, and compared either
        // way round and with another eval.
        "    assert eval(pass, value: [7, _, 7, 9]) == [7, _, 7, 9]\n",
        "    assert eval(pass, value: [_, _, _]) == []\n",
        "    assert eval(pass, value: []) == []\n",
        "    let seen = eval(pass, value: [7, _, 7, 9])\n",
        "    assert seen == [7, _, 7, 9]\n",
        "    assert seen != [7]\n",
        "    assert [7] != seen\n",
        "    assert seen == eval(pass, value: [7, _, 7, 9])\n",
        // Timed.
        "    assert eval(pass, value: [0s: 7, 2s: 9]) == [0s: 7, 2s: 9]\n",
        // Configuration for a `const` parameter; by name and by place.
        "    assert eval(scaled, left: [1, 2], right: [3, 4], factor: 2) == [8, 12]\n",
        "    assert eval(scaled, [1, 2], [3, 4], doubled(1)) == [8, 12]\n",
        // No result: a statement.
        "    eval(sink, value: [7, _, 9])\n",
        "    (eval(sink, value: []))\n",
        // A structural input and result: each cell is its delta.
        "    assert eval(relay, enabled: [true, _], members: [_, delta<set<i64>>(added: [1])])\n",
        "        == [_, delta<set<i64>>(added: [1])]\n",
        // A generic target: its cells say what T is.
        "    let sums = eval(same, lhs: [1, 2], rhs: [3, 4])\n",
        "    assert sums == [4, 6]\n",
        // A value function is lifted (FN-4), and a graph is a target.
        "    assert eval(doubled, value: [1, _]) == [2]\n",
        "    assert eval(forward, value: [1]) == [1]\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(
        locals(&checked, "evaluated"),
        ["seen: trace of i64", "sums: trace of i64"]
    );
    // `let seen = eval(...)`: an eval, of the call of `pass` with one
    // argument, a trace of four cells of which the second is `_`.
    let module = checked.program.module("m").expect("module m exists");
    let body = module.test_bodies[0]
        .as_ref()
        .expect("the test has a block");
    let Stmt::Declare {
        value: Some(value), ..
    } = &body.statements[3]
    else {
        panic!("the fourth statement is the `let`");
    };
    let ExprKind::Eval(call) = &value.kind else {
        panic!("the `let` is given an eval");
    };
    let ExprKind::Call { arguments, .. } = &call.kind else {
        panic!("an eval holds the call of its target");
    };
    let ExprKind::Trace(cells) = &arguments[0].value.kind else {
        panic!("the argument is a trace written out");
    };
    assert_eq!(cells.len(), 4);
    assert!(cells[1].value.is_none(), "the second cell is `_`");
    assert!(cells[0].time.is_none(), "a dense cell has no time");
}

/// TST-5: "Its first argument names a function of a module ... An
/// operator is no target: a function wraps it". TST-8: "binding or
/// comparing it rejects". TST-7: "Keys strictly increase; no `_` occurs
/// in timed traces. Within one eval all inputs and expectation use the
/// same dense/timed style." EXPR-8: "outside them a sequence is ordinary
/// data and a placeholder is not a value."
#[test]
fn an_eval_written_wrong_is_reported() {
    let checked = module(concat!(
        "test wrong {\n",
        // No function: a value, an operator, a struct.
        "    assert eval(7, value: [7]) == [7]\n",
        "    assert eval(add_, lhs: [7], rhs: [9]) == [16]\n",
        "    assert eval(Kept, values: [7]) == [7]\n",
        // No result to bind or to compare.
        "    let none = eval(sink, value: [7])\n",
        "    assert eval(sink, value: [7]) == []\n",
        // A `_` in a timed trace, as a value and as a time; a `_`
        // outside a trace.
        "    assert eval(pass, value: [0s: 7, 2s: _]) == [0s: 7]\n",
        "    assert eval(pass, value: [_: 7]) == []\n",
        "    let loose = [1, _, 3]\n",
        // Two styles: between inputs, and in the expectation.
        "    assert eval(scaled, left: [1, 2], right: [0s: 3], factor: 2) == [8]\n",
        "    assert eval(pass, value: [7, 9]) == [0s: 7]\n",
        // Keys that do not increase; keys of two kinds; a key that is
        // no time.
        "    assert eval(pass, value: [2s: 7, 1s: 9, 1s: 5]) == [2s: 7]\n",
        "    assert eval(pass, value: [1: 7]) == []\n",
        // The arguments are a call's: a cell of another type, a name
        // the target has not, a parameter given nothing, a `const`
        // parameter given a trace.
        "    assert eval(pass, value: [\"a\"]) == [7]\n",
        "    assert eval(pass, value: [7]) == [\"a\"]\n",
        "    assert eval(pass, other: [7]) == [7]\n",
        "    assert eval(pass) == [7]\n",
        "    assert eval(scaled, left: [1], right: [2], factor: [3]) == [9]\n",
        // A trace is no `bool`, and no operand of another symbol.
        "    let seen = eval(pass, value: [7])\n",
        "    assert seen\n",
        "    assert seen < [7]\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:32:17 type/eval.target TST-5",
            "m.hgl:33:17 type/eval.target TST-5",
            "m.hgl:34:17 type/eval.target TST-5",
            "m.hgl:35:16 type/eval.no_result TST-8",
            "m.hgl:36:12 type/eval.no_result TST-8",
            "m.hgl:37:42 type/trace.placeholder TST-7",
            "m.hgl:38:31 type/trace.placeholder TST-7",
            "m.hgl:39:21 type/trace.placeholder EXPR-8",
            "m.hgl:40:46 type/trace.style TST-7",
            "m.hgl:41:41 type/trace.style TST-7",
            "m.hgl:42:38 type/trace.key_order TST-7",
            "m.hgl:42:45 type/trace.key_order TST-7",
            "m.hgl:43:31 type/type.mismatch TST-7",
            "m.hgl:44:31 type/type.mismatch EXPR-5",
            "m.hgl:45:39 type/type.mismatch EXPR-5",
            "m.hgl:46:17 type/argument.missing EXPR-2",
            "m.hgl:46:23 name/argument.unknown EXPR-2",
            "m.hgl:47:17 type/argument.missing EXPR-2",
            "m.hgl:48:56 type/type.mismatch EXPR-5",
            "m.hgl:50:12 type/type.mismatch TST-4",
            "m.hgl:51:17 operator/operator.no_candidate OPS-2",
        ]
    );
}

/// TST-3: a trace is "given to a `const fn` as an argument. It is not a
/// temporal argument outside an eval, is stored in no field". TST-16,
/// proposed: "A dense trace is a `list<delta<T>?>`, with null where it
/// has `_` ... Where `delta<T>` is itself nullable ... the dense trace
/// is not given, and rejects there. A timed trace is a
/// `dict<duration, delta<T>>` or a `dict<datetime, delta<T>>`".
#[test]
fn a_trace_goes_to_a_const_fn_and_nowhere_else() {
    let checked = module(concat!(
        "test given {\n",
        "    let seen = eval(pass, value: [7, _, 9])\n",
        "    let spaced = eval(pass, value: [0s: 7, 2s: 9])\n",
        // To a `const fn`: a list, or a dictionary by time.
        "    assert close(seen, [7, null, 9])\n",
        "    assert close(eval(pass, value: [1]), [1])\n",
        "    assert timed(spaced)\n",
        // Of the other style than the parameter takes.
        "    assert timed(seen)\n",
        "    assert close(spaced, [7])\n",
        // A dense trace whose cells may be null.
        "    let nulls = eval(maybe, value: [1, null])\n",
        "    assert close(nulls, [1])\n",
        // A field; a temporal function; and each where the place is
        // generic, and so of whatever it is given.
        "    let kept = Kept(values: seen)\n",
        "    let again = pass(seen)\n",
        "    let boxed = Boxed(value: seen)\n",
        "    let passed = through(seen)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:37:18 type/type.mismatch EXPR-5",
            "m.hgl:38:18 type/type.mismatch EXPR-5",
            "m.hgl:40:18 type/trace.escape TST-16",
            "m.hgl:41:29 type/trace.escape TST-3",
            "m.hgl:42:22 type/trace.escape TST-3",
            "m.hgl:43:30 type/trace.escape TST-3",
            "m.hgl:44:26 type/trace.escape TST-3",
        ]
    );
    assert_eq!(
        locals(&checked, "given")[..3],
        [
            "seen: trace of i64",
            "spaced: trace of i64",
            "nulls: trace of i64?",
        ]
    );
}
