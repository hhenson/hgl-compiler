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
    "const fn close(seen: list<tuple<datetime, i64>>, wanted: list<i64?>) -> bool => true\n",
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

/// As [`module`]: a second program in one test.
fn module_text(text: &str) -> Checked {
    module(text)
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
        "    assert eval(pass(value: [7, _, 7, 9])) == [7, _, 7, 9]\n",
        "    assert eval(pass(value: [_, _, _])) == []\n",
        "    assert eval(pass(value: [])) == []\n",
        "    let seen = eval(pass(value: [7, _, 7, 9]))\n",
        "    assert seen == [7, _, 7, 9]\n",
        "    assert seen != [7]\n",
        "    assert [7] != seen\n",
        "    assert seen == eval(pass(value: [7, _, 7, 9]))\n",
        // Timed.
        "    assert eval(pass(value: [0s: 7, 2s: 9])) == [0s: 7, 2s: 9]\n",
        // Configuration for a `const` parameter; by name and by place.
        "    assert eval(scaled(left: [1, 2], right: [3, 4], factor: 2)) == [8, 12]\n",
        "    assert eval(scaled([1, 2], [3, 4], doubled(1))) == [8, 12]\n",
        // No result: a statement.
        "    eval(sink(value: [7, _, 9]))\n",
        "    (eval(sink(value: [])))\n",
        // A structural input and result: each cell is its delta.
        "    assert eval(relay(enabled: [true, _], members: [_, delta<set<i64>>(added: [1])]))\n",
        "        == [_, delta<set<i64>>(added: [1])]\n",
        // A generic target: its cells say what T is.
        "    let sums = eval(same(lhs: [1, 2], rhs: [3, 4]))\n",
        "    assert sums == [4, 6]\n",
        // A value function is lifted (FN-4), and a graph is a target.
        "    assert eval(doubled(value: [1, _])) == [2]\n",
        "    assert eval(forward(value: [1])) == [1]\n",
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
    let ExprKind::Eval { call, .. } = &value.kind else {
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

/// EXPR-13: operands are evaluated in the order written, and CTRL-10
/// has a block among them assign what a later one reads. A trace
/// written out on the left of an eval takes its type from the eval,
/// and is still read before it.
#[test]
fn a_trace_comparison_reads_its_operands_in_order() {
    let checked = module(concat!(
        "test ordered {\n",
        // Read on the left before the right assigns it.
        "    var first: i64\n",
        "    assert [first] == eval(pass(value: [{\n",
        "        first = 7\n",
        "        7\n",
        "    }]))\n",
        // Assigned on the left, and read on the right.
        "    var second: i64\n",
        "    assert [{\n",
        "        second = 7\n",
        "        7\n",
        "    }] == eval(pass(value: [second]))\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:33:13 name/binding.unassigned EXPR-3"]
    );
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
        "    assert eval(7) == [7]\n",
        "    assert eval(add_(lhs: [7], rhs: [9])) == [16]\n",
        "    assert eval(Kept(values: [7])) == [7]\n",
        // No result to bind or to compare.
        "    let none = eval(sink(value: [7]))\n",
        "    assert eval(sink(value: [7])) == []\n",
        // A `_` in a timed trace, as a value and as a time; a `_`
        // outside a trace.
        "    assert eval(pass(value: [0s: 7, 2s: _])) == [0s: 7]\n",
        "    assert eval(pass(value: [_: 7])) == []\n",
        "    let loose = [1, _, 3]\n",
        // Two styles: between inputs, and in the expectation.
        "    assert eval(scaled(left: [1, 2], right: [0s: 3], factor: 2)) == [8]\n",
        "    assert eval(pass(value: [7, 9])) == [0s: 7]\n",
        // Keys that do not increase; keys of two kinds; a key that is
        // no time.
        "    assert eval(pass(value: [2s: 7, 1s: 9, 1s: 5])) == [2s: 7]\n",
        "    assert eval(pass(value: [1: 7])) == []\n",
        // The arguments are a call's: a cell of another type, a name
        // the target has not, a parameter given nothing, a `const`
        // parameter given a trace.
        "    assert eval(pass(value: [\"a\"])) == [7]\n",
        "    assert eval(pass(value: [7])) == [\"a\"]\n",
        "    assert eval(pass(other: [7])) == [7]\n",
        "    assert eval(pass()) == [7]\n",
        "    assert eval(scaled(left: [1], right: [2], factor: [3])) == [9]\n",
        // A trace is no `bool`, and no operand of another symbol.
        "    let seen = eval(pass(value: [7]))\n",
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
            "m.hgl:37:41 type/trace.placeholder TST-7",
            "m.hgl:38:30 type/trace.placeholder TST-7",
            "m.hgl:39:21 type/trace.placeholder EXPR-8",
            "m.hgl:40:45 type/trace.style TST-7",
            "m.hgl:41:41 type/trace.style TST-7",
            "m.hgl:42:37 type/trace.key_order TST-7",
            "m.hgl:42:44 type/trace.key_order TST-7",
            "m.hgl:43:30 type/type.mismatch TST-7",
            "m.hgl:44:30 type/type.mismatch EXPR-5",
            "m.hgl:45:39 type/type.mismatch EXPR-5",
            "m.hgl:46:17 type/argument.missing EXPR-2",
            "m.hgl:46:22 name/argument.unknown EXPR-2",
            "m.hgl:47:17 type/argument.missing EXPR-2",
            "m.hgl:48:55 type/type.mismatch EXPR-5",
            "m.hgl:50:12 type/type.mismatch TST-4",
            "m.hgl:51:17 operator/operator.no_candidate OPS-2",
        ]
    );
}

/// TST-5: "A second argument is the run's configuration, a `RunConfig`
/// (RUN-1), and where none is written the configuration is
/// `RunConfig()`." The call "binds ... its generic arguments as any call
/// does".
#[test]
fn an_eval_takes_a_call_and_a_run_configuration() {
    let checked = module(concat!(
        "test configured {\n",
        "    assert eval(pass(value: [7])) == [7]\n",
        "    assert eval(pass(value: [7]), RunConfig()) == [7]\n",
        "    assert eval(pass(value: [7]), RunConfig(end: @2026-01-01T00:00:00Z),) == [7]\n",
        "    assert eval(through<i64>(value: [7])) == [7]\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
    let module = checked.program.module("m").unwrap();
    let body = module.test_bodies[0].as_ref().unwrap();
    // Each assertion compares an eval with a trace: whether the eval
    // has a configuration, in order.
    let configured: Vec<bool> = body
        .statements
        .iter()
        .filter_map(|statement| match statement {
            Stmt::Assert { condition, .. } => match &condition.kind {
                ExprKind::TraceCompare { left, .. } => match &left.kind {
                    ExprKind::Eval { config, .. } => Some(config.is_some()),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(configured, [false, true, true, false]);

    let wrong = module_text(concat!(
        "test wrong {\n",
        // No `RunConfig`; an argument past it; a label.
        "    assert eval(pass(value: [7]), 3) == [7]\n",
        "    assert eval(pass(value: [7]), RunConfig(), 4) == [7]\n",
        // The call's generic arguments are checked as a call's are.
        "    assert eval(through<i64, i64>(value: [7])) == [7]\n",
        // "A trace is an argument of that call and of no call inside
        // it": the call inside takes a value, and gives one.
        "    assert eval(pass(value: forward(value: [7]))) == [7]\n",
        // FN-3: a `const` parameter of the call is given no connection.
        "    assert eval(scaled(left: [1], right: [2], factor: forward(3))) == [9]\n",
        "}\n",
    ));
    assert_eq!(
        wrong.summary(),
        [
            "m.hgl:32:35 type/type.mismatch EXPR-5",
            "m.hgl:33:48 type/argument.unexpected TST-5",
            "m.hgl:34:17 type/generic.argument_count GEN-8",
            "m.hgl:35:29 type/type.mismatch EXPR-5",
            "m.hgl:35:44 type/type.mismatch EXPR-5",
            "m.hgl:36:55 phase/connection.as_constant FN-3",
        ],
        "{}",
        wrong.report
    );
}

/// TST-17: "An integration test, `integration_test name { ... }`, is a
/// named test that may run in real time: beside `eval` it writes
/// `run(call, config)` (RUN-3), whose value is the run's output as an
/// eval's is ... in a test that is no integration test it rejects ...
/// In every other respect it is a test: its name is one of its module's
/// test names (TST-2), and its block is a test's (TST-3, TST-4)."
#[test]
fn an_integration_test_writes_run_and_a_test_does_not() {
    let checked = module(concat!(
        "integration_test live {\n",
        "    let seen = run(pass(value: [7]))\n",
        "    assert seen == [7]\n",
        "    assert run(pass(value: [7]), RunConfig()) == eval(pass(value: [7]))\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
    assert_eq!(locals(&checked, "live"), ["seen: trace of i64"]);
    let module = checked.program.module("m").unwrap();
    assert!(module.tests[0].integration);
    // The run is an eval's, in real time.
    let body = module.test_bodies[0].as_ref().unwrap();
    let Stmt::Declare {
        value: Some(value), ..
    } = &body.statements[0]
    else {
        panic!("a `let`");
    };
    assert!(matches!(
        value.kind,
        ExprKind::Eval {
            real_time: true,
            config: None,
            ..
        }
    ));

    let wrong = module_text(concat!(
        // `run` in a test that is no integration test, read all the
        // same: its trace is compared as an eval's is.
        "test plain {\n",
        "    assert run(pass(value: [7])) == [7]\n",
        "}\n",
        // An integration test is a test: what a test may not hold, its
        // assertion's type, and its name among the module's.
        "integration_test live {\n",
        "    state count: i64 = 0\n",
        "    assert run(pass(value: [7]))\n",
        "    assert run(7) == [7]\n",
        "}\n",
        "integration_test plain { assert true }\n",
    ));
    assert_eq!(
        wrong.summary(),
        [
            "m.hgl:32:12 phase/run.position RUN-3",
            "m.hgl:35:5 phase/test.statement_phase TST-3",
            "m.hgl:36:12 type/type.mismatch TST-4",
            "m.hgl:37:16 type/eval.target TST-5",
            "m.hgl:39:18 name/test.duplicate_name TST-2",
        ],
        "{}",
        wrong.report
    );
}

/// TST-5: the call "binds ... its generic arguments as any call does".
/// GEN-14: `name<Type>(value)` is one the parser leaves undecided, and
/// the resolver decides it is a call.
#[test]
fn an_evals_call_may_be_one_the_parser_left_undecided() {
    let checked = module(concat!(
        "enum Side { buy, sell }\n",
        "test generic {\n",
        "    assert eval(through<Side>([.buy, _, .sell])) == [.buy, _, .sell]\n",
        "    let seen = eval(through<Side>([.buy]), RunConfig())\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
    assert_eq!(locals(&checked, "generic"), ["seen: trace of Side"]);
}

/// EXPR-1: "parentheses group". A call in parentheses is the eval's
/// call, and what is no call is reported where the argument is written.
#[test]
fn an_evals_call_may_stand_in_parentheses() {
    let checked = module(concat!(
        "enum Side { buy, sell }\n",
        "test grouped {\n",
        "    assert eval((pass(value: [7]))) == [7]\n",
        "    assert eval(((through<Side>([.buy]))), (RunConfig())) == [.buy]\n",
        "    assert eval((7)) == [7]\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:35:17 type/eval.target TST-5"],
        "{}",
        checked.report
    );
}

/// EXPR-13: arguments are evaluated in the order written. An eval's
/// call is read before its configuration, so what the call assigns the
/// configuration may read, and not the other way about.
#[test]
fn an_evals_call_is_read_before_its_configuration() {
    let checked = module(concat!(
        "const fn configured(count: i64) -> RunConfig => RunConfig()\n",
        "test order {\n",
        "    var x: i64\n",
        "    assert eval(pass(value: [{\n",
        "        x = 7\n",
        "        7\n",
        "    }]), configured(x)) == [7]\n",
        "}\n",
        "test reversed {\n",
        "    var y: i64\n",
        "    assert eval(pass(value: [y]), {\n",
        "        y = 7\n",
        "        RunConfig()\n",
        "    }) == [7]\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:41:30 name/binding.unassigned EXPR-3"],
        "{}",
        checked.report
    );
}

/// TST-16: "A trace given to a `const fn` is an ordinary value there:
/// for a result type T a `list<tuple<datetime, delta<T>>>`, one tuple
/// for each publication, in time order ... a `delta<T>` (TYPE-16), which
/// is T itself for a scalar ... The parameter is written with that type,
/// and what the function returns is ordinary data."
#[test]
fn a_const_fn_is_given_a_trace_as_the_list_of_its_publications() {
    let checked = module(concat!(
        "const fn ticks(seen: list<tuple<datetime, i64>>) -> i64 {\n",
        "    var count = 0\n",
        "    for tick in seen { count += 1 }\n",
        "    return count\n",
        "}\n",
        "const fn tally<T>(seen: list<tuple<datetime, T>>) -> i64 => 0\n",
        "const fn first<T>(seen: list<tuple<datetime, T>>, otherwise: T) -> T => otherwise\n",
        "const fn changes(seen: list<tuple<datetime, delta<set<i64>>>>) -> i64 => 0\n",
        "const fn wholes(seen: list<tuple<datetime, set<i64>>>) -> i64 => 0\n",
        "const fn nulls(seen: list<tuple<datetime, delta<i64?>>>) -> i64 => 0\n",
        "test counted {\n",
        // The specification's own case; and a trace bound first.
        "    assert ticks(eval(pass(value: [7, _, 7, 9]))) == 3\n",
        "    let seen = eval(pass(value: [0s: 7, 2s: 9]))\n",
        "    let twice = ticks(seen)\n",
        // What the trace is of says what a generic parameter is.
        "    assert tally(seen) == 0\n",
        "    let one: i64 = first(seen, 1)\n",
        // A structural result is given as its deltas; a nullable
        // scalar's as the optional that holds what was published.
        "    let members = eval(relay(enabled: [true], members: [delta<set<i64>>(added: [1])]))\n",
        "    assert changes(members) == 0\n",
        "    assert nulls(eval(maybe(value: [1, null]))) == 0\n",
        // Rejected: a parameter of another type; and a trace of `i64`
        // where the place has said that `T` is `str`.
        "    assert wholes(members) == 0\n",
        "    assert ticks(members) == 0\n",
        "    let word: str = first(seen, \"x\")\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:50:19 type/type.mismatch EXPR-5",
            "m.hgl:51:18 type/type.mismatch EXPR-5",
            "m.hgl:52:27 type/type.mismatch EXPR-5",
        ]
    );
    // `ticks(seen)`: the argument is the trace's publications, a
    // `list<tuple<datetime, i64>>`, around the trace itself.
    let module = checked.program.module("m").expect("module m exists");
    let body = module.test_bodies[0]
        .as_ref()
        .expect("the test has a block");
    let Stmt::Declare {
        value: Some(value), ..
    } = &body.statements[2]
    else {
        panic!("the third statement is the `let`");
    };
    let ExprKind::Call { arguments, .. } = &value.kind else {
        panic!("its value is the call of `ticks`");
    };
    let given = &arguments[0].value;
    let ExprKind::Publications(trace) = &given.kind else {
        panic!("the argument is the trace's publications");
    };
    assert_eq!(given.ty.to_string(), "list<tuple<datetime, i64>>");
    assert_eq!(trace.ty.to_string(), "trace of i64");
}

/// TST-3: a trace is "given to a `const fn` as an argument. It is not a
/// temporal argument outside an eval, is stored in no field". TST-16:
/// there it is "a `list<tuple<datetime, delta<T>>>`, one tuple for each
/// publication", whichever style it was written in, and "the parameter
/// is written with that type". A value that holds a trace is held to
/// the same as the trace is.
#[test]
fn a_trace_goes_to_a_const_fn_and_nowhere_else() {
    let checked = module(concat!(
        "test given {\n",
        "    let seen = eval(pass(value: [7, _, 9]))\n",
        "    let spaced = eval(pass(value: [0s: 7, 2s: 9]))\n",
        // To a `const fn`, of either style, bound or not: the list of
        // its publications. One whose parameter is another type.
        "    assert close(seen, [7, null, 9])\n",
        "    assert close(eval(pass(value: [1])), [1])\n",
        "    assert close(spaced, [])\n",
        "    assert timed(seen)\n",
        "    let listed = [seen]\n",
        // A field; a temporal function; and each where the place is
        // generic, and so of whatever it is given.
        "    let kept = Kept(values: seen)\n",
        "    let again = pass(seen)\n",
        "    let boxed = Boxed(value: seen)\n",
        "    let passed = through(seen)\n",
        // Wrapped in a sequence, it goes no further.
        "    let boxes = Boxed(value: [seen])\n",
        "    let wrapped = through([seen])\n",
        "    let pair = Boxed(value: (seen, 1))\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            // `timed(seen)`: a `list<tuple<datetime, i64>>` where a
            // `dict<duration, i64>` is expected.
            "m.hgl:37:18 type/type.mismatch EXPR-5",
            "m.hgl:39:29 type/trace.escape TST-3",
            "m.hgl:40:22 type/trace.escape TST-3",
            "m.hgl:41:30 type/trace.escape TST-3",
            "m.hgl:42:26 type/trace.escape TST-3",
            "m.hgl:43:30 type/trace.escape TST-3",
            "m.hgl:44:27 type/trace.escape TST-3",
            "m.hgl:45:29 type/trace.escape TST-3",
        ]
    );
    assert_eq!(
        locals(&checked, "given")[..3],
        [
            "seen: trace of i64",
            "spaced: trace of i64",
            "listed: list<trace of i64>",
        ]
    );
}
