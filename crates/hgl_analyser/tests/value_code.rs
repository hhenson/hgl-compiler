//! The analyser against value code, a test's statements outside an
//! `eval` or a `run` and an application's statements (TST-3, UNIT-14,
//! GEN-5), and against the constructor form of a delta that is no type
//! of its own (TYPE-16). Each test cites its rule.
//!
//! A finding is written `file:line:column category/code rule`.

mod common;

use common::{body, check, module};
use hgl_analyser::hir::{Callee, Expr, ExprKind, Node, Stmt, walk_statement_nodes};

/// The functions and operators the tests call: `pass` a graph, `node_of`
/// a node, `twice` a `const fn`; `scaled` an operator whose one
/// candidate is a node, `halved` one whose one candidate is an `impl
/// const fn`, and `paired` one with both, of one parameter type.
const DECLARED: &str = concat!(
    "fn pass(value: i64) -> i64 => value\n",
    "const fn twice(value: i64) -> i64 => value * 2\n",
    "fn node_of(value: i64) -> i64 {\n",
    "    when { return value }\n",
    "}\n",
    "export operator scaled<T>(value: T) -> T\n",
    "impl fn scaled(value: i64) -> i64 {\n",
    "    when { return value }\n",
    "}\n",
    "export operator halved<T>(value: T) -> T\n",
    "impl const fn halved(value: i64) -> i64 => value\n",
    "operator paired<T>(value: T) -> T\n",
    "impl fn paired(value: i64) -> i64 {\n",
    "    when { return value }\n",
    "}\n",
    "impl const fn paired(value: i64) -> i64 => value\n",
);

/// TST-3: "Outside the argument of an `eval` or a `run`, which is
/// composition (TST-5, TST-8), its statements are value code, as an
/// application's are (UNIT-14) ... They call `const fn`s only: a call of
/// any other function there, a node, a graph or a source, rejects, so no
/// connection is bound in a test ... Nothing there is lifted (FN-6).
/// Proposed: those rejections have the code `call.role`." The call an
/// eval takes is composition's, and selects the graph (FN-4).
#[test]
fn a_test_outside_an_eval_calls_const_fns_only() {
    let checked = module(&format!(
        "{DECLARED}{}",
        concat!(
            "test t {\n",
            "    let a = pass(1)\n",
            "    let b = twice(2)\n",
            "    let c = node_of(2)\n",
            "    let d = twice(b)\n",
            "    assert twice(2) == 4\n",
            "    assert eval pass(value: [1]) == [1]\n",
            "    assert eval node_of(value: [twice(1)]) == [2]\n",
            "}\n",
        )
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:19:13 function-kind/call.role TST-3",
            "m.hgl:21:13 function-kind/call.role TST-3",
        ]
    );
    // `twice(b)` is an ordinary call, and is not lifted.
    let module = checked.program.module("m").expect("module m exists");
    let index = module
        .tests
        .iter()
        .position(|test| test.name == "t")
        .expect("the test exists");
    let statements = &module.test_bodies[index]
        .as_ref()
        .expect("it has a block")
        .statements;
    let mut lifted = Vec::new();
    let mut calls = 0;
    walk_statement_nodes(statements, &mut |node| {
        if let Node::Expression(Expr {
            kind:
                ExprKind::Call {
                    callee: Callee::Function { .. },
                    ..
                },
            ..
        }) = node
        {
            calls += 1;
        }
        if let Node::Expression(value) = node
            && let ExprKind::Call {
                callee: Callee::Function { .. },
                lifted: true,
                ..
            } = &value.kind
        {
            lifted.push(value.at);
        }
    });
    assert!(calls > 0);
    assert!(lifted.is_empty(), "{lifted:?}");
}

/// GEN-5: "A call in a test's or an application's value code (TST-3,
/// UNIT-14) selects among all of them, a value candidate outranking a
/// temporal one of the same parameter types, and a temporal candidate it
/// selects rejects." TST-3's example: "`1 +{invalid: .identity} 2`, whose
/// one candidate that takes the option is temporal (core's OPS-9),
/// rejects the same way", and `1 + 2` is admitted.
#[test]
fn an_operator_in_value_code_selects_a_value_candidate() {
    let checked = module(&format!(
        "{DECLARED}{}",
        concat!(
            "test t {\n",
            "    let a = scaled(1)\n",
            "    let b = halved(1)\n",
            "    let c = paired(1)\n",
            "    assert 1 + 2 == 3\n",
            "    assert 1 +{invalid: .identity} 2 == 3\n",
            "}\n",
        )
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:19:13 function-kind/call.role GEN-5",
            "m.hgl:23:14 function-kind/call.role GEN-5",
        ]
    );
}

/// UNIT-14: "Those statements, and the arguments of the run statement's
/// call and its configuration, are value code; the call itself, of the
/// entry, is composition (RUN-1). Value code calls `const fn`s only ...
/// An operator applied there is resolved by GEN-5, and the candidate it
/// selects must be a value candidate ... Proposed: those rejections have
/// the code `call.role`." So a test and an application are one.
#[test]
fn an_applications_statements_are_value_code() {
    let checked = check(&[
        (
            "m.hgl",
            &format!(
                "module m\n{}{}",
                DECLARED
                    .replace("fn pass", "export fn pass")
                    .replace("const fn twice", "export const fn twice")
                    .replace("export const export fn", "export const fn"),
                concat!(
                    "export fn limited(const limit: i64) -> i64 {\n",
                    "    inject scheduler\n",
                    "    start { schedule(scheduler, 0s) }\n",
                    "    when scheduled() { return limit }\n",
                    "}\n",
                )
            ),
        ),
        (
            "app.hgl",
            concat!(
                "application app.one\n",
                "\n",
                "use m::{pass, twice, limited, scaled, halved}\n",
                "\n",
                "inject global_state\n",
                "put(global_state, \"a\", pass(1))\n",
                "put(global_state, \"b\", scaled(3))\n",
                "put(global_state, \"c\", halved(4))\n",
                "put(global_state, \"d\", twice(2))\n",
                "run limited(limit: pass(2))\n",
            ),
        ),
    ]);
    assert_eq!(
        checked.summary(),
        [
            "app.hgl:6:24 function-kind/call.role UNIT-14",
            "app.hgl:7:24 function-kind/call.role GEN-5",
            "app.hgl:10:20 function-kind/call.role UNIT-14",
        ]
    );
}

/// TYPE-16: "A delta that is another type the language has, a scalar's,
/// a tuple's, a window's, has no constructor: `delta<T>(...)` for such a
/// T rejects (`delta.argument_name`), where it names a property and in
/// the empty form alike, and the value is written as its type writes it,
/// `3` or a tuple, and for a window of a nullable V as that nullable's
/// delta is, `delta<f64?>(value: 3.0)`." Each property named is
/// reported; the empty form at the constructor. A delta that is a type
/// of its own is constructed as before.
#[test]
fn a_delta_that_is_another_type_has_no_constructor() {
    let checked = module(concat!(
        "const fn a() -> i64 => delta<i64>(added: 3)\n",
        "const fn b() -> i64 => delta<rolling_window<i64, 2>>(gone: 3)\n",
        "const fn c() -> i64 => delta<rolling_window<i64, 2>>()\n",
        "const fn d() -> tuple<i64?, i64?> => delta<tuple<i64, i64>>(first: 1, second: 2)\n",
        "const fn e() -> delta<set<i64>> => delta<set<i64>>(added: [1])\n",
        "const fn f() -> delta<f64?> => delta<rolling_window<f64?, 3>>(value: 3.0)\n",
        "const fn g() -> delta<f64?> => delta<f64?>(value: 3.0)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:2:35 name/delta.argument_name TYPE-16",
            "m.hgl:3:54 name/delta.argument_name TYPE-16",
            "m.hgl:4:24 name/delta.argument_name TYPE-16",
            "m.hgl:5:61 name/delta.argument_name TYPE-16",
            "m.hgl:5:71 name/delta.argument_name TYPE-16",
        ]
    );
    // The constructor of one that is a type of its own is read.
    let Some(Stmt::Return {
        value: Some(value), ..
    }) = body(&checked, "e").statements.first()
    else {
        panic!("an arrow body is one return");
    };
    assert!(
        matches!(value.kind, ExprKind::Delta { .. }),
        "{:?}",
        value.kind
    );
}
