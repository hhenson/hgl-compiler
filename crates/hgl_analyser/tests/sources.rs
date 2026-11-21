//! The analyser against the rules on a source's body: what its
//! statements and each `yield` are. Each test cites its rule. The shape
//! a source's body has is in `phases.rs`.
//!
//! A finding is written `file:line:column category/code rule`.

mod common;

use common::{Checked, body, locals, module};
use hgl_analyser::hir::{ExprKind, Hook, Raise, Stmt};
use hgl_analyser::{Scalar, Type};

/// No lines.
const NONE: [&str; 0] = [];

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

/// SOURCE-2: "A duration is relative to current evaluation time; a
/// datetime is absolute." Other types do not give a time (EXPR-5).
#[test]
fn a_yield_time_is_a_duration_or_datetime() {
    let checked = module(concat!(
        "fn typed(const at: datetime, const wrong: result<i64, error>, const maybe: str?) -> i64 {\n",
        "    yield 1s: 1\n",
        "    yield at: 2\n",
        "    yield \"text\": 3\n",
        "    yield 42: 4\n",
        "    yield wrong: 5\n",
        "    yield maybe: 6\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:5:11 type/yield.time_type SOURCE-2",
            "m.hgl:6:11 type/yield.time_type SOURCE-2",
            "m.hgl:7:11 type/yield.time_type SOURCE-2",
            "m.hgl:8:11 type/yield.time_type SOURCE-2",
        ]
    );
    for (statement, scalar) in body(&checked, "typed")
        .statements
        .iter()
        .zip([Scalar::Duration, Scalar::Datetime])
    {
        let Stmt::Yield { time, .. } = statement else {
            panic!("expected a yield");
        };
        assert_eq!(time.ty, Type::Scalar(scalar));
    }
}

/// SOURCE-2 and GEN-7. The body's requirement must prove that the type
/// of this yield's time is a duration or a datetime. The body keeps
/// the parameter so that each instance uses its actual time type.
#[test]
fn a_generic_yield_time_needs_a_type_bound() {
    let checked = module(
        "fn equal<T>(const t: T) -> i64 requires T == duration { yield t: 1 }\n\
         fn either<T>(const t: T) -> i64 requires T in {duration, datetime} { yield t: 1 }\n\
         fn unbounded<T>(const t: T) -> i64 { yield t: 1 }\n\
         fn mixed<T>(const t: T) -> i64 requires T in {duration, i64} { yield t: 1 }\n\
         fn other<T, U>(const t: T, const u: U) -> i64 requires U == duration { yield t: 1 }\n\
         fn alternate<T>(const t: T) -> i64 requires T == duration || T == i64 { yield t: 1 }\n\
         fn both<T>(const t: T) -> i64 requires T in {duration, i64} && T in {duration, datetime} { yield t: 1 }\n\
         fn narrowed<T>(const t: T) -> i64 requires T in {duration, i64} && T == duration { yield t: 1 }\n\
         fn disjoined<T>(const t: T) -> i64 requires T == duration || T == datetime { yield t: 1 }\n\
         fn guarded<T, U>(const t: T, const u: U) -> i64 requires T == duration || U == datetime { yield t: 1 }\n",
    );
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:4:44 type/yield.time_type SOURCE-2",
            "m.hgl:5:70 type/yield.time_type SOURCE-2",
            "m.hgl:6:78 type/yield.time_type SOURCE-2",
            "m.hgl:7:79 type/yield.time_type SOURCE-2",
            "m.hgl:11:97 type/yield.time_type SOURCE-2",
        ],
        "{}",
        checked.report
    );
    for name in ["equal", "either", "both", "narrowed", "disjoined"] {
        let [Stmt::Yield { time, .. }] = body(&checked, name).statements.as_slice() else {
            panic!("expected a yield");
        };
        assert!(matches!(&time.ty, Type::Parameter { name, .. } if name == "T"));
    }
}

/// SOURCE-2, ERR-12, ERR-15: the time takes a result's successful
/// payload and reads a nullable value. Neither form changes where a
/// failure is raised (ERR-5).
#[test]
fn a_yield_reads_the_time_from_nullable_and_fallible_values() {
    let checked = module(
        r#"
const fn delay() -> result<duration, error> => ok(1s)
fn timed(const outcome: result<datetime?, error>, const maybe: duration?) -> i64 {
    yield delay(): 1
    yield maybe: 2
    yield outcome: 3
}
"#,
    );
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
    let [
        Stmt::Yield { time: delay, .. },
        Stmt::Yield { time: maybe, .. },
        Stmt::Yield { time: outcome, .. },
    ] = body(&checked, "timed").statements.as_slice()
    else {
        panic!("expected three yields");
    };
    assert_eq!(delay.ty, Type::Scalar(Scalar::Duration));
    assert!(matches!(
        delay.kind,
        ExprKind::Raised {
            raise: Raise::Source,
            ..
        }
    ));
    assert_eq!(maybe.ty, Type::Scalar(Scalar::Duration));
    assert!(matches!(
        maybe.kind,
        ExprKind::NullRead {
            raise: Raise::Source,
            ..
        }
    ));
    assert_eq!(outcome.ty, Type::Scalar(Scalar::Datetime));
    let ExprKind::NullRead {
        value,
        raise: Raise::Source,
        ..
    } = &outcome.kind
    else {
        panic!("expected a read of the nullable success");
    };
    assert!(matches!(
        value.kind,
        ExprKind::Raised {
            raise: Raise::Source,
            ..
        }
    ));
}

/// SOURCE-1: "`yield time: value` writes its output": the value is of
/// the source's result type (EXPR-5). Its statements are runtime code:
/// FN-3, "A runtime body calls only value functions"; ERR-5 allows a
/// failure to be raised in a source. ERR-7, in a value function or a hook
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
        // A capability published, which is no `f64` either; a temporal
        // function called, a failure propagated and a capture.
        "        yield every: clock\n",
        "        yield every: feeder(count)\n",
        "        let whole: i64 = checked(count)\n",
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
            "m.hgl:16:22 type/type.mismatch EXPR-5",
            "m.hgl:17:22 function-kind/call.role FN-3",
            "m.hgl:19:20 phase/capture.phase ERR-7",
        ]
    );
}

/// SOURCE-1: "A temporal `fn` whose body has a `yield` is a source". The
/// analyser says which declarations are (`Module::is_source`), so that no
/// target works it out again: a node and a graph are not.
#[test]
fn which_declarations_are_sources_is_said() {
    let checked = module(concat!(
        "fn two_values() -> i64 {\n",
        "    yield 1s: 7\n",
        "    yield 1s: 9\n",
        "}\n",
        "fn counted(value: i64) -> i64 {\n",
        "    when { return value }\n",
        "}\n",
        "fn wired(value: i64) -> i64 => counted(value)\n",
    ));
    assert_eq!(checked.summary(), NONE);
    let module = checked.program.module("m").unwrap();
    let sources: Vec<(&str, bool)> = module
        .declarations
        .iter()
        .enumerate()
        .map(|(index, declared)| (declared.name.as_str(), module.is_source(index)))
        .collect();
    assert_eq!(
        sources,
        [("two_values", true), ("counted", false), ("wired", false)]
    );
}
