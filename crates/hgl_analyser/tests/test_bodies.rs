//! The analyser against the rules on a named test's block: what it is
//! read as, and what an `assert` takes. Each test cites its rule. What a
//! test's block may hold is in `phases.rs`.
//!
//! A finding is written `file:line:column category/code rule`.

mod common;

use common::{Checked, check};
use hgl_analyser::hir::Stmt;

/// Checks one file, `m.hgl`, of module `m`. The `module` line is added,
/// so the text given starts on line 2.
fn module(text: &str) -> Checked {
    check(&[("m.hgl", &format!("module m\n{text}"))])
}

/// What each statement of the test `name`'s block is, in a word.
fn outline(checked: &Checked, name: &str) -> Vec<&'static str> {
    let module = checked.program.module("m").expect("module m exists");
    let index = module
        .tests
        .iter()
        .position(|test| test.name == name)
        .expect("the test exists");
    module.test_bodies[index]
        .as_ref()
        .expect("it has a block")
        .statements
        .iter()
        .map(|statement| match statement {
            Stmt::Declare { .. } => "let",
            Stmt::Assert { .. } => "assert",
            Stmt::AssertRaises { .. } => "raises",
            _ => "other",
        })
        .collect()
}

/// TST-3: "A test is a composition-phase block with assertions." TST-4:
/// "Assert requires a wiring-time bool." TST-11: the block of a
/// `raises` "has ordinary lexical test scope". An `eval` is not read
/// yet, so nothing is said of a comparison with one (ERR-9).
#[test]
fn an_assert_takes_a_wiring_time_bool() {
    let checked = module(concat!(
        "const fn limit() -> i64 => 3\n",
        "fn forward(value: i64) -> i64 {\n",
        "    when { return value }\n",
        "}\n",
        "test asserted {\n",
        "    let bound = limit()\n",
        "    assert true\n",
        "    assert bound == 3\n",
        "    assert limit() > 2 && bound < 10\n",
        // A trace is compared by the harness, and `eval` is not read.
        "    assert eval(forward, value: [7, _, 9]) == [7, _, 9]\n",
        "    assert eval(forward, value: [_, _]) == []\n",
        "    let outcome = eval(forward, value: [1])\n",
        "    assert outcome == []\n",
        // The block of a `raises` sees the test's locals.
        "    assert raises(\"value.missing_key\") {\n",
        "        let inner = bound + 1\n",
        "        assert inner == 4\n",
        "    }\n",
        "}\n",
        "test rejected {\n",
        "    let bound = limit()\n",
        // No bool: a number, a string, and a list whose entries say
        // nothing of what it holds.
        "    assert bound\n",
        "    assert \"text\"\n",
        "    assert []\n",
        // A connection: what `forward` publishes is known only as the
        // graph runs.
        "    assert forward(bound) == 3\n",
        // What any composition block rejects.
        "    assert missing == 3\n",
        "    let wrong: str = bound\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:22:12 type/type.mismatch TST-4",
            "m.hgl:23:12 type/type.mismatch TST-4",
            "m.hgl:24:12 type/type.mismatch TST-4",
            "m.hgl:25:12 phase/connection.as_constant TST-4",
            "m.hgl:27:22 type/type.mismatch EXPR-5",
        ]
    );
    // `missing` is a name nothing binds, left for the prelude.
    assert_eq!(checked.prelude_names("m"), ["missing"]);
    assert_eq!(
        outline(&checked, "asserted"),
        [
            "let", "assert", "assert", "assert", "assert", "assert", "let", "assert", "raises"
        ]
    );
}
