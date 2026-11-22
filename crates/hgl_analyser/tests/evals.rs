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
        "    assert eval pass(value: [7, _, 7, 9]) == [7, _, 7, 9]\n",
        "    assert eval pass(value: [_, _, _]) == []\n",
        "    assert eval pass(value: []) == []\n",
        "    let seen = eval pass(value: [7, _, 7, 9])\n",
        "    assert seen == [7, _, 7, 9]\n",
        "    assert seen != [7]\n",
        "    assert [7] != seen\n",
        "    assert seen == eval pass(value: [7, _, 7, 9])\n",
        // Timed.
        "    assert eval pass(value: [0s: 7, 2s: 9]) == [0s: 7, 2s: 9]\n",
        // Configuration for a `const` parameter; by name and by place.
        "    assert eval scaled(left: [1, 2], right: [3, 4], factor: 2) == [8, 12]\n",
        "    assert eval scaled([1, 2], [3, 4], doubled(1)) == [8, 12]\n",
        // No result: a statement.
        "    eval sink(value: [7, _, 9])\n",
        "    (eval sink(value: []))\n",
        // A structural input and result: each cell is its delta.
        "    assert eval relay(enabled: [true, _], members: [_, delta<set<i64>>(added: [1])])\n",
        "        == [_, delta<set<i64>>(added: [1])]\n",
        // A generic target: its cells say what T is.
        "    let sums = eval same(lhs: [1, 2], rhs: [3, 4])\n",
        "    assert sums == [4, 6]\n",
        // A value function is lifted (FN-4), and a graph is a target.
        "    assert eval doubled(value: [1, _]) == [2]\n",
        "    assert eval forward(value: [1]) == [1]\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(
        locals(&checked, "evaluated"),
        ["seen: trace of i64", "sums: trace of i64"]
    );
    // `let seen = eval pass(...)`: an eval, of the call of `pass` with one
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

/// TST-5 and FN-8: each captured argument receives its own trace. An
/// inferred type pack keeps the types those traces replay, in capture order.
#[test]
fn an_eval_reads_pack_arguments_as_separate_traces() {
    let checked = check(&[(
        "m.hgl",
        r#"module m
fn fixed(const seed: i64, rest: ...i64{2}) -> i64 { when { return seed } }
fn same_cells<T>(rest: ...T{2}) -> i64 { when { return 1 } }
fn positional<...Ts>(rest: ...Ts{2}) -> i64 { when { return 1 } }
fn named<...Ts>(rest: ...{Ts}{2}) -> i64 { when { return 1 } }
test captured {
    let fixed_trace = eval fixed(1, [1, 2], [_, 3])
    let same_trace = eval same_cells([1, 2], [_, _])
    let explicit = eval positional<i64, bool>([_, 2], [_, _])
    let inferred = eval positional([_, 2], [_, true])
    let named_trace = eval named(flag: [0s: true], items: [0s: [1, 2]])
}
"#,
    )]);
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
    let module = checked.program.module("m").expect("module m exists");
    let body = module.test_bodies[0]
        .as_ref()
        .expect("the test has a block");
    let calls: Vec<_> = body
        .statements
        .iter()
        .map(|statement| {
            let Stmt::Declare {
                value: Some(value), ..
            } = statement
            else {
                panic!("the test binds an eval");
            };
            let ExprKind::Eval { call, .. } = &value.kind else {
                panic!("the binding holds an eval");
            };
            let ExprKind::Call {
                callee, arguments, ..
            } = &call.kind
            else {
                panic!("the eval holds the call of its target");
            };
            (callee, arguments)
        })
        .collect();
    assert!(matches!(calls[0].1[0].value.kind, ExprKind::Literal(_)));
    for &(_, arguments) in &calls {
        for argument in arguments.iter().filter(|argument| argument.pack.is_some()) {
            assert!(matches!(argument.value.kind, ExprKind::Trace(_)));
        }
    }
    let types = |index: usize| {
        calls[index].0.type_packs()[0]
            .types
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    };
    assert_eq!(types(2), ["i64", "bool"]);
    assert_eq!(types(3), ["i64", "bool"]);
    assert_eq!(types(4), ["bool", "list<i64>"]);
}

/// TST-5 and FN-8: a trace's cells still have one type when the trace
/// supplies the type of a heterogeneous pack element.
#[test]
fn an_eval_rejects_a_wrong_cell_type_in_a_pack_argument() {
    let checked = check(&[(
        "m.hgl",
        r#"module m
fn fixed(rest: ...i64) -> i64 { when { return 1 } }
fn inferred<...Ts>(rest: ...Ts) -> i64 { when { return 1 } }
test rejected {
    assert eval fixed(["text"]) == [1]
    assert eval inferred([1, "text"]) == [1]
}
"#,
    )]);
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:5:24 type/type.mismatch EXPR-5",
            "m.hgl:6:30 type/type.mismatch EXPR-5",
        ]
    );
}

/// GEN-8: a packed trace with no cells that publish cannot decide a
/// generic type. The target still gives the argument its trace context.
#[test]
fn a_pack_cannot_infer_a_type_from_gaps_in_an_eval() {
    let checked = check(&[(
        "m.hgl",
        r#"module m
fn same<T>(rest: ...T) -> i64 { when { return 1 } }
fn disparate<...Ts>(rest: ...Ts) -> i64 { when { return 1 } }
test rejected {
    assert eval same([_]) == []
    assert eval disparate([_]) == []
}
"#,
    )]);
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:5:17 type/generic.unresolved GEN-8",
            "m.hgl:6:17 type/generic.unresolved GEN-8",
        ]
    );
}

/// GEN-8 and TST-5: a packed trace containing only contextual cells
/// needs a resolved pack type even when it has publications.
#[test]
fn a_pack_cannot_infer_from_contextual_cells_in_an_eval() {
    let checked = check(&[(
        "m.hgl",
        r#"module m
fn positional<...Ts>(rest: ...Ts) -> i64 { when { return 1 } }
fn named<...Ts>(rest: ...{Ts}) -> i64 { when { return 1 } }
test traces {
    assert eval positional([null]) == []
    assert eval positional([ok(1)]) == []
    assert eval named(x: [0s: null]) == []
    assert eval named(x: [0s: ok(1)]) == []
    assert eval positional([(null, 1)]) == []
    assert eval positional<i64?, result<i64, error>>([null], [ok(1)]) == [1]
    assert eval positional([(null, not_here)]) == []
}
"#,
    )]);
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:5:17 type/generic.unresolved GEN-8",
            "m.hgl:6:17 type/generic.unresolved GEN-8",
            "m.hgl:7:17 type/generic.unresolved GEN-8",
            "m.hgl:8:17 type/generic.unresolved GEN-8",
            "m.hgl:9:17 type/generic.unresolved GEN-8",
            "m.hgl:11:36 name/name.unknown UNIT-10",
        ],
        "{}",
        checked.report
    );
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
        "    assert [first] == eval pass(value: [{\n",
        "        first = 7\n",
        "        7\n",
        "    }])\n",
        // Assigned on the left, and read on the right.
        "    var second: i64\n",
        "    assert [{\n",
        "        second = 7\n",
        "        7\n",
        "    }] == eval pass(value: [second])\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:33:13 name/binding.unassigned EXPR-3"]
    );
}

/// TST-5: "Eval takes a call of a module function ... An operator is
/// no target: a function wraps it". RUN-6: "An entry that names no field
/// is an argument that names no parameter (`argument.unknown`)". TST-8:
/// "binding or comparing it rejects". TST-7: "Keys strictly increase; no `_` occurs
/// in timed traces. Within one eval all inputs and expectation use the
/// same dense/timed style." EXPR-8: "outside them a sequence is ordinary
/// data and a placeholder is not a value."
#[test]
fn an_eval_written_wrong_is_reported() {
    let checked = module(concat!(
        "test wrong {\n",
        // A field the configuration has not; no function: an
        // operator, a struct.
        "    assert eval{until: 3us} pass(value: [7]) == [7]\n",
        "    assert eval add_(lhs: [7], rhs: [9]) == [16]\n",
        "    assert eval Kept(values: [7]) == [7]\n",
        // No result to bind or to compare.
        "    let none = eval sink(value: [7])\n",
        "    assert eval sink(value: [7]) == []\n",
        // A `_` in a timed trace, as a value and as a time; a `_`
        // outside a trace.
        "    assert eval pass(value: [0s: 7, 2s: _]) == [0s: 7]\n",
        "    assert eval pass(value: [_: 7]) == []\n",
        "    let loose = [1, _, 3]\n",
        // Two styles: between inputs, and in the expectation.
        "    assert eval scaled(left: [1, 2], right: [0s: 3], factor: 2) == [8]\n",
        "    assert eval pass(value: [7, 9]) == [0s: 7]\n",
        // Keys that do not increase; keys of two kinds; a key that is
        // no time.
        "    assert eval pass(value: [2s: 7, 1s: 9, 1s: 5]) == [2s: 7]\n",
        "    assert eval pass(value: [1: 7]) == []\n",
        // The arguments are a call's: a cell of another type, a name
        // the target has not, a parameter given nothing, a `const`
        // parameter given a trace.
        "    assert eval pass(value: [\"a\"]) == [7]\n",
        "    assert eval pass(value: [7]) == [\"a\"]\n",
        "    assert eval pass(other: [7]) == [7]\n",
        "    assert eval pass() == [7]\n",
        "    assert eval scaled(left: [1], right: [2], factor: [3]) == [9]\n",
        // A trace is no `bool`, and no operand of another symbol.
        "    let seen = eval pass(value: [7])\n",
        "    assert seen\n",
        "    assert seen < [7]\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:32:17 name/argument.unknown RUN-6",
            "m.hgl:33:17 type/eval.target TST-5",
            "m.hgl:34:17 type/eval.target TST-5",
            "m.hgl:35:16 type/eval.no_result TST-8",
            "m.hgl:36:12 type/eval.no_result TST-8",
            "m.hgl:37:41 type/trace.placeholder TST-7",
            "m.hgl:38:30 type/trace.placeholder TST-7",
            "m.hgl:39:21 type/trace.placeholder EXPR-8",
            "m.hgl:40:45 type/trace.style TST-7",
            "m.hgl:41:40 type/trace.style TST-7",
            "m.hgl:42:37 type/trace.key_order TST-7",
            "m.hgl:42:44 type/trace.key_order TST-7",
            "m.hgl:43:30 type/type.mismatch TST-7",
            "m.hgl:44:30 type/type.mismatch EXPR-5",
            "m.hgl:45:38 type/type.mismatch EXPR-5",
            "m.hgl:46:17 type/argument.missing EXPR-2",
            "m.hgl:46:22 name/argument.unknown EXPR-2",
            "m.hgl:47:17 type/argument.missing EXPR-2",
            "m.hgl:48:55 type/type.mismatch EXPR-5",
            "m.hgl:50:12 type/type.mismatch TST-4",
            "m.hgl:51:17 operator/operator.no_candidate OPS-2",
        ]
    );
}

/// TST-5: "The eval's configuration, `eval{length: 4s} target(...)`, is
/// the run's (RUN-6)." RUN-6: "Each entry names a field of the
/// configuration, and its value is value code of that field's type; a
/// field no entry names is left out." The call "binds ... its generic
/// arguments as any call does".
#[test]
fn an_eval_takes_a_call_and_a_run_configuration() {
    let checked = module(concat!(
        "test configured {\n",
        "    assert eval pass(value: [7]) == [7]\n",
        "    assert eval{start: @2026-01-01T00:00:00Z} pass(value: [7]) == [7]\n",
        "    assert eval {end: @2026-01-01T00:00:00Z, length: 2us,} pass(value: [7]) == [7]\n",
        "    assert eval through<i64>(value: [7]) == [7]\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
    let module = checked.program.module("m").unwrap();
    let body = module.test_bodies[0].as_ref().unwrap();
    // Each assertion compares an eval with a trace: the fields the
    // eval's configuration names, and their types, in order.
    let configured: Vec<Vec<String>> = body
        .statements
        .iter()
        .filter_map(|statement| match statement {
            Stmt::Assert { condition, .. } => match &condition.kind {
                ExprKind::TraceCompare { left, .. } => match &left.kind {
                    ExprKind::Eval {
                        config, inherited, ..
                    } => {
                        assert!(inherited.is_none(), "a test that injects nothing");
                        Some(
                            config
                                .iter()
                                .map(|entry| format!("{}: {}", entry.field, entry.value.ty))
                                .collect(),
                        )
                    }
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(
        configured,
        [
            vec![],
            vec!["start: datetime".to_string()],
            vec!["end: datetime".to_string(), "length: duration".to_string()],
            vec![],
        ]
    );

    let wrong = module_text(concat!(
        "test wrong {\n",
        // A field given a value of another type, and one given twice.
        "    assert eval{length: 3} pass(value: [7]) == [7]\n",
        "    assert eval{length: 3us, length: 4us} pass(value: [7]) == [7]\n",
        // The call's generic arguments are checked as a call's are.
        "    assert eval through<i64, i64>(value: [7]) == [7]\n",
        // "A trace is an argument of that call and of no call inside
        // it": the call inside takes a value, and gives one.
        "    assert eval pass(value: forward(value: [7])) == [7]\n",
        // FN-3: a `const` parameter of the call is given no connection.
        "    assert eval scaled(left: [1], right: [2], factor: forward(3)) == [9]\n",
        "}\n",
    ));
    assert_eq!(
        wrong.summary(),
        [
            "m.hgl:32:25 type/type.mismatch EXPR-5",
            "m.hgl:33:30 name/argument.duplicate EXPR-10",
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
/// `run call` (RUN-3), whose value is the run's output as an eval's
/// is ... `run` in a test that is no integration test rejects
/// (`run.position`) ...
/// In every other respect it is a test: its name is one of its module's
/// test names (TST-2), and its block is a test's (TST-3, TST-4)."
#[test]
fn an_integration_test_writes_run_and_a_test_does_not() {
    let checked = module(concat!(
        "integration_test live {\n",
        "    let seen = run pass(value: [7])\n",
        "    assert seen == [7]\n",
        "    assert run pass(value: [7]) == eval pass(value: [7])\n",
        "}\n",
        // TST-17: "the duration `@timeout(5s)` gives on the line before
        // it, or one minute where none is written".
        "@timeout(1m30s)\n",
        "integration_test bounded {\n",
        "    assert true\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
    assert_eq!(locals(&checked, "live"), ["seen: trace of i64"]);
    let module = checked.program.module("m").unwrap();
    assert!(module.tests[0].integration);
    // The limit a test writes, in microseconds; none where it writes
    // none.
    assert_eq!(module.tests[0].timeout, None);
    assert!(module.tests[1].integration);
    assert_eq!(module.tests[1].timeout, Some(90_000_000));
    // The run is an eval's, in real time.
    let body = module.test_bodies[0].as_ref().unwrap();
    let Stmt::Declare {
        value: Some(value), ..
    } = &body.statements[0]
    else {
        panic!("a `let`");
    };
    assert!(matches!(
        &value.kind,
        ExprKind::Eval {
            real_time: true,
            config,
            inherited: None,
            ..
        } if config.is_empty()
    ));

    let wrong = module_text(concat!(
        // `run` in a test that is no integration test, read all the
        // same: its trace is compared as an eval's is.
        "test plain {\n",
        "    assert run pass(value: [7]) == [7]\n",
        "}\n",
        // An integration test is a test: what a test may not hold, its
        // assertion's type, and its name among the module's.
        "integration_test live {\n",
        "    state count: i64 = 0\n",
        "    assert run pass(value: [7])\n",
        "    assert run add_(lhs: [7], rhs: [9]) == [16]\n",
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
        "    assert eval through<Side>([.buy, _, .sell]) == [.buy, _, .sell]\n",
        "    let seen = eval through<Side>([.buy])\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
    assert_eq!(locals(&checked, "generic"), ["seen: trace of Side"]);
}

/// TST-17: "An integration test may inject `run_config`, before its
/// other statements: what runs the test supplies the configuration of
/// its runs, and the braces give defaults ... Its value code may read
/// `run_config` then". RUN-6: "In an integration test that injects
/// `run_config`, an entry of a run statement's configuration is that
/// statement's own value for its field, and a field the statement does
/// not name is the injected configuration's."
#[test]
fn an_integration_test_injects_its_run_configuration() {
    let checked = module(concat!(
        "integration_test configured {\n",
        "    inject run_config{length: 1s}\n",
        "    let bound = run_config.length\n",
        "    let seen = run{start: @2026-01-01T00:00:00Z} pass(value: [7])\n",
        "    assert seen == eval pass(value: [7])\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
    assert_eq!(
        locals(&checked, "configured"),
        [
            "run_config: RunConfig",
            "bound: duration?",
            "seen: trace of i64"
        ]
    );
    let module = checked.program.module("m").unwrap();
    let body = module.test_bodies[0].as_ref().unwrap();
    let Stmt::Inject { local, config, .. } = &body.statements[0] else {
        panic!("the inject is the test's first statement");
    };
    assert_eq!(config.len(), 1);
    assert_eq!(config[0].field, "length");
    // The run's own entry, and the injected configuration for the rest.
    let Stmt::Declare {
        value: Some(value), ..
    } = &body.statements[2]
    else {
        panic!("a `let`");
    };
    let ExprKind::Eval {
        config, inherited, ..
    } = &value.kind
    else {
        panic!("a run");
    };
    assert_eq!(*inherited, Some(*local));
    assert_eq!(config.len(), 1);
    assert_eq!(config[0].field, "start");
}

/// RUN-6: "`inject run_config` where it has no place rejects
/// (`inject.position`), and so do braces on an `inject` of any other
/// capability (`inject.configuration`)". "A test that is no integration
/// test does not, and neither does a function". TST-17: an integration
/// test injects it "before its other statements". "`RunConfig` is the
/// type of the capability `run_config` (TIME-8): no expression
/// constructs one" (`call.not_callable`). Braces that are rejected are
/// not read (ERR-9): a name in them that nothing binds is not reported.
#[test]
fn run_config_where_it_has_no_place_is_reported() {
    let wrong = module_text(concat!(
        "test unit {\n",
        "    inject run_config\n",
        "}\n",
        "integration_test late {\n",
        "    assert true\n",
        "    inject run_config{length: missing}\n",
        "}\n",
        "fn node(value: i64) -> i64 {\n",
        "    inject run_config{start: missing}\n",
        "    when { return value }\n",
        "}\n",
        "fn clocked(value: i64) -> datetime {\n",
        "    inject clock{step: missing}\n",
        "    when { return clock.evaluation_time }\n",
        "}\n",
        "const fn bounded() -> i64 {\n",
        "    inject run_config\n",
        "    return 1\n",
        "}\n",
        "test constructed {\n",
        "    let bounds = RunConfig(length: 3s)\n",
        "}\n",
        "integration_test clocked_test {\n",
        "    inject run_config, clock\n",
        "}\n",
    ));
    assert_eq!(
        wrong.summary(),
        [
            "m.hgl:32:12 injectable/inject.position RUN-6",
            "m.hgl:36:12 injectable/inject.position RUN-6",
            "m.hgl:39:12 injectable/inject.position RUN-6",
            "m.hgl:43:17 injectable/inject.configuration RUN-6",
            "m.hgl:47:12 injectable/inject.position RUN-6",
            "m.hgl:51:18 type/call.not_callable NATIVE-6",
            "m.hgl:54:5 phase/test.statement_phase TST-3",
        ],
        "{}",
        wrong.report
    );
}

/// EXPR-13: operands are evaluated in the order written. An eval's
/// configuration is written before its call (RUN-6) and read first, so
/// what the configuration assigns the call may read, and not the other
/// way about.
#[test]
fn an_evals_configuration_is_read_before_its_call() {
    let checked = module(concat!(
        "test order {\n",
        "    var x: i64\n",
        "    assert eval{length: {\n",
        "        x = 7\n",
        "        7us\n",
        "    }} pass(value: [x]) == [7]\n",
        "}\n",
        "test reversed {\n",
        "    var y: i64\n",
        "    assert eval{length: if y == 0 { 1us } else { 2us }} pass(value: [{\n",
        "        y = 7\n",
        "        7\n",
        "    }]) == [7]\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:40:28 name/binding.unassigned EXPR-3"],
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
        "    assert ticks(eval pass(value: [7, _, 7, 9])) == 3\n",
        "    let seen = eval pass(value: [0s: 7, 2s: 9])\n",
        "    let twice = ticks(seen)\n",
        // What the trace is of says what a generic parameter is.
        "    assert tally(seen) == 0\n",
        "    let one: i64 = first(seen, 1)\n",
        // A structural result is given as its deltas; a nullable
        // scalar's as the optional that holds what was published.
        "    let members = eval relay(enabled: [true], members: [delta<set<i64>>(added: [1])])\n",
        "    assert changes(members) == 0\n",
        "    assert nulls(eval maybe(value: [1, null])) == 0\n",
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

/// TST-16 and GEN-10: a captured trace becomes a publication list
/// before the const function's type pack and its `each` are resolved.
#[test]
fn an_inferred_const_fn_pack_captures_the_publications_of_a_trace() {
    let checked = module(concat!(
        "const fn collect<...Ts>(items: ...Ts) -> i64\n",
        "    requires each T in types(Ts) { T == list<tuple<datetime, i64>> } => 1\n",
        "const fn named<...Ts>(items: ...{Ts}) -> i64\n",
        "    requires each T in types(Ts) { T == list<tuple<datetime, i64>> } => 2\n",
        "test collected {\n",
        "    let seen = eval pass(value: [7, _, 9])\n",
        "    let positional = collect(seen)\n",
        "    let keyed = named(trace: seen)\n",
        "}\n",
    ));
    assert!(checked.findings.is_empty(), "{}", checked.report);
    let body = checked.program.module("m").unwrap().test_bodies[0]
        .as_ref()
        .unwrap();
    for statement in &body.statements[1..] {
        let Stmt::Declare {
            value: Some(call), ..
        } = statement
        else {
            panic!("expected a declaration with a call");
        };
        let ExprKind::Call {
            callee, arguments, ..
        } = &call.kind
        else {
            panic!("expected a call with a captured argument");
        };
        let [pack] = callee.type_packs() else {
            panic!("expected a single inferred type pack");
        };
        assert_eq!(pack.types[0].to_string(), "list<tuple<datetime, i64>>");
        assert!(matches!(arguments[0].value.kind, ExprKind::Publications(_)));
    }
}

/// TST-5: "Each cell is one cycle's publication, written as a `return`
/// is (TIME-7) ... a T, published whole, or a `delta<T>` (TYPE-16),
/// published as that change. A list, a tuple, a struct and a dictionary
/// are each written either way". TIME-7: "a value that is a T is
/// published whole ... A value that is no T and is a `delta<T>` is
/// published as that change." So a tuple written in a cell is the whole
/// tuple where it is a T: `(2, null)` of a `tuple<i64, i64?>` holds a
/// published null, and `(1, [1, 2, 3])` a whole list. Where it is no T,
/// `(null, 3)` of a `tuple<i64, i64>`, it is the tuple's delta. An
/// expectation's cells are read the same. TST-16: a trace given to a
/// `const fn` is "for a result type T a `list<tuple<datetime,
/// delta<T>>>`".
#[test]
fn a_tuple_cell_is_the_whole_tuple_where_it_is_one() {
    let checked = module(concat!(
        "fn second_null(pair: tuple<i64, i64?>) -> bool {\n",
        "    when valid(pair[1]) { return pair[1] == null }\n",
        "}\n",
        "const fn digits(values: list<i64>) -> i64 => 0\n",
        "fn second_digits(pair: tuple<i64, list<i64>>) -> i64 => digits(pair[1])\n",
        "fn first_of(pair: tuple<i64, i64>) -> i64 => pair[0]\n",
        "fn pair_out(command: i64) -> tuple<i64, list<i64>> {\n",
        "    when { return (command, [1, 2, 3]) }\n",
        "}\n",
        "fn pair_of(command: i64) -> tuple<i64, i64?> {\n",
        "    when { return (command, null) }\n",
        "}\n",
        "const fn pairs_seen(published: list<tuple<datetime, delta<tuple<i64, i64?>>>>) -> i64 => 0\n",
        "test cells {\n",
        "    assert eval second_null(pair: [(1, 5), (2, null)]) == [false, true]\n",
        "    assert eval second_digits(pair: [(1, [1, 2, 3]), (2, [1])]) == [0, 0]\n",
        "    assert eval second_digits(pair: [\n",
        "        (1, [1]),\n",
        "        (null, delta<list<i64>>(size_modified: -1)),\n",
        "    ]) == [0, 0]\n",
        "    assert eval first_of(pair: [(1, 2), (null, 3)]) == [1, _]\n",
        "    assert eval pair_out(command: [7]) == [(7, [1, 2, 3])]\n",
        "    assert eval pair_of(command: [7, 8]) == [(7, null), (null, 9)]\n",
        "    let count = pairs_seen(eval pair_of(command: [7]))\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
    // The type of each cell that is a tuple, in the order written.
    let module = checked.program.module("m").expect("module m exists");
    let body = module.test_bodies[0]
        .as_ref()
        .expect("the test has a block");
    let mut cells = Vec::new();
    hgl_analyser::hir::walk_statements(&body.statements, &mut |expression| {
        if let ExprKind::Trace(written) = &expression.kind {
            cells.extend(
                written
                    .iter()
                    .filter_map(|cell| cell.value.as_ref())
                    .map(|value| value.ty.to_string())
                    .filter(|ty| ty.starts_with("tuple")),
            );
        }
    });
    assert_eq!(
        cells,
        [
            // Whole, a null among them.
            "tuple<i64, i64?>",
            "tuple<i64, i64?>",
            "tuple<i64, list<i64>>",
            "tuple<i64, list<i64>>",
            // Whole, then the delta.
            "tuple<i64, list<i64>>",
            "tuple<i64?, delta<list<i64>>?>",
            "tuple<i64, i64>",
            "tuple<i64?, i64?>",
            // An expectation's: whole, and whole then the delta.
            "tuple<i64, list<i64>>",
            "tuple<i64, i64?>",
            "tuple<i64?, delta<i64?>?>",
        ]
    );
    assert_eq!(locals(&checked, "cells"), ["count: i64"]);
}

/// TST-5: a cell of a trace given to a parameter is "written as a
/// `return` is", the parameter's type T saying which a cell is. A
/// generic target's parameters of types written out are the same with
/// its generic arguments in place (GEN-2): a tuple's trace there is a
/// trace of that tuple, so a trace an earlier eval gave is taken, and a
/// cell written as a T is read whole. A tuple that names a generic
/// parameter, `tuple<T, T>`, is found from a trace an eval gave as from
/// one written out (GEN-2: "Resolve every needed generic from actual
/// inputs"), and its cells are then read as any tuple's are. An
/// `atomic<tuple<A, B>>` publishes whole (TYPE-9), its cell the tuple
/// itself, generic target or not.
#[test]
fn a_tuple_trace_is_given_to_a_generic_target() {
    let checked = module(concat!(
        "fn two(command: i64) -> tuple<i64, i64> {\n",
        "    when { return (command, command) }\n",
        "}\n",
        "fn consume<T>(pair: tuple<i64, i64?>, const tag: T) -> i64 {\n",
        "    when all_valid(pair) { return pair[0] }\n",
        "}\n",
        "fn snap(command: i64) -> atomic<tuple<i64, i64>> {\n",
        "    when { return (command, command) }\n",
        "}\n",
        "fn kept_whole<T>(pair: atomic<tuple<i64, i64>>, const tag: T) -> i64 {\n",
        "    when { return pair[0] }\n",
        "}\n",
        "fn kept_part<T>(pair: atomic<tuple<T, i64>>, const tag: T) -> i64 {\n",
        "    when { return pair[1] }\n",
        "}\n",
        "fn same_pair<T>(pair: tuple<T, T>) -> i64 {\n",
        "    when all_valid(pair) { return 1 }\n",
        "}\n",
        "fn forwarded<T>(pair: tuple<i64, i64>, const tag: T) -> tuple<i64, i64> {\n",
        "    when all_valid(pair) { return pair }\n",
        "}\n",
        "test given {\n",
        "    let pairs = eval two(command: [1, 2])\n",
        "    assert eval forwarded(pair: pairs, tag: 1) == [(1, 1), (2, 2)]\n",
        "    assert eval consume(pair: [(1, null)], tag: 1) == [1]\n",
        "    assert eval same_pair(pair: pairs) == [1, 1]\n",
        "    assert eval same_pair(pair: [(1, 2), (null, 3)]) == [1, 1]\n",
        "    let snaps = eval snap(command: [1])\n",
        "    assert eval kept_whole(pair: snaps, tag: 1) == [1]\n",
        "    assert eval kept_whole(pair: [(1, 2)], tag: 1) == [1]\n",
        "    assert eval kept_part(pair: snaps, tag: 1) == [1]\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
    let module = checked.program.module("m").expect("module m exists");
    let body = module.test_bodies[0]
        .as_ref()
        .expect("the test has a block");
    let mut cells = Vec::new();
    hgl_analyser::hir::walk_statements(&body.statements, &mut |expression| {
        if let ExprKind::Trace(written) = &expression.kind {
            cells.extend(
                written
                    .iter()
                    .filter_map(|cell| cell.value.as_ref())
                    .map(|value| value.ty.to_string()),
            );
        }
    });
    assert_eq!(
        cells,
        [
            "i64",
            "i64",
            "tuple<i64, i64>",
            "tuple<i64, i64>",
            "tuple<i64, i64?>",
            "i64",
            "i64",
            "i64",
            "tuple<i64, i64>",
            "tuple<i64?, i64?>",
            "i64",
            "i64",
            "i64",
            "i64",
            "tuple<i64, i64>",
            "i64",
            "i64",
        ]
    );
}

/// TYPE-16: "A delta that names one member, key or position both as
/// added or updated and as removed is malformed. A constant one is
/// rejected (`delta.overlap`)." TIME-7: "A value that is no T and is a
/// `delta<T>` is published as that change", and TST-5 has a cell
/// written as a `return` is. A malformed delta returned, or written in a
/// cell, is a delta all the same: it is reported for its overlap, and
/// not again as no complete value of its place.
#[test]
fn a_malformed_delta_is_reported_once() {
    let checked = module(concat!(
        "fn kept_set(values: set<i64>) -> set<i64> {\n",
        "    when { return delta<set<i64>>(added: [1], removed: [1]) }\n",
        "}\n",
        "test overlapping {\n",
        "    assert eval kept_set(values: [delta<set<i64>>(added: [2], removed: [2])]) == [[2]]\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:32:57 type/delta.overlap TYPE-16",
            "m.hgl:35:73 type/delta.overlap TYPE-16",
        ]
    );
}

/// TST-3: a trace is "given to a `const fn` as an argument. It is not a
/// temporal argument outside an eval, is stored in no field". TST-16:
/// there it is "a `list<tuple<datetime, delta<T>>>`, one tuple for each
/// publication", whichever style it was written in, and "the parameter
/// is written with that type". A value that holds a trace is held to
/// the same as the trace is. A temporal function, `pass` or the graph
/// `through`, is not called in a test outside an `eval` at all (TST-3:
/// its statements are value code there), with the code proposed as
/// `call.role`.
#[test]
fn a_trace_goes_to_a_const_fn_and_nowhere_else() {
    let checked = module(concat!(
        "test given {\n",
        "    let seen = eval pass(value: [7, _, 9])\n",
        "    let spaced = eval pass(value: [0s: 7, 2s: 9])\n",
        // To a `const fn`, of either style, bound or not: the list of
        // its publications. One whose parameter is another type.
        "    assert close(seen, [7, null, 9])\n",
        "    assert close(eval pass(value: [1]), [1])\n",
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
            "m.hgl:40:17 function-kind/call.role TST-3",
            "m.hgl:41:30 type/trace.escape TST-3",
            "m.hgl:42:18 function-kind/call.role TST-3",
            "m.hgl:43:30 type/trace.escape TST-3",
            "m.hgl:44:19 function-kind/call.role TST-3",
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

/// TST-7: distinct timed traces may use relative or absolute keys.
/// Both are timed, including the expectation of an eval with a relative input.
#[test]
fn timed_traces_can_name_instants_in_different_ways() {
    let checked = module(concat!(
        "test times {\n",
        "    let at = @2026-01-05T09:30:00Z\n",
        "    assert eval scaled(left: [0s: 2], right: [at: 3], factor: 1) == [at: 5]\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE);
}
