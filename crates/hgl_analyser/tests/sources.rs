//! The analyser against the rules on a source's body: what its
//! statements and each `yield` are. Each test cites its rule. The shape
//! a source's body has is in `phases.rs`.
//!
//! A finding is written `file:line:column category/code rule`.

mod common;

use common::{Checked, check};
use hgl_analyser::hir::{Body, Hook, Stmt};

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

/// The locals of `name`, as `local: type`.
fn locals(checked: &Checked, name: &str) -> Vec<String> {
    body(checked, name)
        .locals
        .iter()
        .map(|local| format!("{}: {}", local.name, local.ty))
        .collect()
}

/// What each statement of `name`'s body is, in a word.
fn outline(checked: &Checked, name: &str) -> Vec<&'static str> {
    body(checked, name)
        .statements
        .iter()
        .map(|statement| match statement {
            Stmt::Declare { .. } => "declare",
            Stmt::Inject { .. } => "inject",
            Stmt::Hook {
                hook: Hook::Start, ..
            } => "start",
            Stmt::Hook {
                hook: Hook::Stop, ..
            } => "stop",
            Stmt::Hook { .. } => "when",
            Stmt::Yield { .. } => "yield",
            Stmt::While { .. } => "while",
            Stmt::For { .. } => "for",
            Stmt::Expr(_) => "block",
            _ => "other",
        })
        .collect()
}

/// SOURCE-1: "A temporal `fn` whose body has a `yield` is a source ...
/// `yield time: value` writes its output and sets its next evaluation.
/// It declares `cache` and `state` (TIME-6), which are what it keeps
/// from one `yield` to the next ... A source with no declaration and no
/// hook has a body of statements, `yield` among them. One with an
/// `inject`, a `cache`, a `state`, a `start`, or a `stop` has its
/// declarations, then `start`, then one block that holds every `yield`,
/// then `stop` ... A `let` there holds a working value".
#[test]
fn a_sources_body_is_read() {
    // The chapter's `examples.source`.
    let checked = module(concat!(
        "fn two_values() -> i64 {\n",
        "    yield 1s: 7\n",
        "    yield 1s: 9\n",
        "}\n",
        "fn greeting(const every: duration = 1s) -> str {\n",
        "    inject logger\n",
        "    start { log_info(logger, \"greeting starts\") }\n",
        "    {\n",
        "        yield every: \"hello\"\n",
        "        yield every: \"world\"\n",
        "    }\n",
        "    stop { log_info(logger, \"greeting ends\") }\n",
        "}\n",
        "fn counter(const every: duration = 1s, const limit: i64 = 3) -> i64 {\n",
        "    inject logger\n",
        "    cache count: i64 = 0\n",
        "    while count < limit {\n",
        "        count += 1\n",
        "        yield every: count\n",
        "    }\n",
        "    stop { log_info(logger, str(count)) }\n",
        "}\n",
        "fn replay(const values: list<f64>, const every: duration = 1s) -> f64 {\n",
        "    inject logger\n",
        "    start { log_info(logger, \"replay starts\") }\n",
        "    for value in values {\n",
        "        let doubled = value * 2.0\n",
        "        yield every: doubled\n",
        "    }\n",
        "    stop { log_info(logger, \"replay ends\") }\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(outline(&checked, "two_values"), ["yield", "yield"]);
    assert_eq!(
        outline(&checked, "greeting"),
        ["inject", "start", "block", "stop"]
    );
    assert_eq!(
        outline(&checked, "counter"),
        ["inject", "declare", "while", "stop"]
    );
    assert_eq!(
        outline(&checked, "replay"),
        ["inject", "start", "for", "stop"]
    );
    assert_eq!(
        locals(&checked, "replay")[..2],
        ["values: list<f64>", "every: duration"]
    );
    assert!(
        locals(&checked, "replay").contains(&"doubled: f64".to_string()),
        "a `let` in the block holds a working value"
    );
    // `yield 1s: 7`: the time, and the value of the result's type.
    let Some(Stmt::Yield { time, value, .. }) = body(&checked, "two_values").statements.first()
    else {
        panic!("`two_values` begins with a `yield`");
    };
    assert_eq!(time.ty.to_string(), "duration");
    assert_eq!(value.ty.to_string(), "i64");
}

/// SOURCE-1: "`yield time: value` writes its output": the value is of
/// the source's result type (EXPR-5). Its statements are runtime code:
/// FN-3, "A runtime body calls only value functions"; ERR-5,
/// "Composition, lifecycle, and generator uses reject
/// (`result.propagation_phase`)"; ERR-7, in a value function or a hook
/// a capture rejects; TIME-8, a capability is not "what a node
/// publishes". CTRL-15: a `while` "with no condition is a source's".
#[test]
fn a_yield_writes_a_value_of_the_sources_type() {
    let checked = module(concat!(
        "const fn checked(value: i64) -> result<i64, error> => ok(value)\n",
        "fn feeder(value: i64) -> i64 {\n",
        "    when { return value }\n",
        "}\n",
        "fn typed(const every: duration = 1s, const limit: i64 = 3) -> f64 {\n",
        "    inject clock\n",
        "    cache count: i64 = 0\n",
        // No condition, and a `return` with no value, which ends it.
        "    while {\n",
        "        count += 1\n",
        "        if count > limit { return }\n",
        // An integer literal is widened; a value function is called.
        "        yield every: 1\n",
        "        yield clock.now: 2.5\n",
        // Not of the result's type.
        "        yield every: \"text\"\n",
        "        yield every: count\n",
        // A capability published, a temporal function called, a
        // failure propagated and a capture.
        "        yield every: clock\n",
        "        yield every: feeder(count)\n",
        "        let whole = checked(count)?\n",
        "        let seen = capture(feeder, count)\n",
        "    }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:14:22 type/type.mismatch EXPR-5",
            "m.hgl:15:22 type/type.mismatch EXPR-5",
            "m.hgl:16:22 injectable/capability.as_value TIME-8",
            "m.hgl:17:22 function-kind/call.role FN-3",
            "m.hgl:18:35 phase/result.propagation_phase ERR-5",
            "m.hgl:19:20 phase/capture.phase ERR-7",
        ]
    );
}
