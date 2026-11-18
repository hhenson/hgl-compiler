//! The analyser against the rules on applications and run statements
//! (UNIT-14, RUN-1, RUN-3). Each test cites its rule.
//!
//! A finding is written `file:line:column category/code rule`.

mod common;

use common::check;
use hgl_analyser::hir::{Callee, ExprKind, Stmt};

/// No lines.
const NONE: [&str; 0] = [];

/// A module with an entry: `beats` has no temporal parameter.
const MODULE: &str = concat!(
    "module app.feed\n",
    "\n",
    "export fn beats(const every: duration = 1s) -> i64 {\n",
    "    state count: i64 = 0\n",
    "    while {\n",
    "        count += 1\n",
    "        yield every: count\n",
    "    }\n",
    "}\n",
);

/// A module whose entry reads a key of the store.
const LIMITED: &str = concat!(
    "module app.limited\n",
    "\n",
    "export fn counted(const every: duration = 1s) -> i64 {\n",
    "    inject global_state\n",
    "    cache count: i64 = 0\n",
    "    while count < get<i64>(global_state, \"limit\") {\n",
    "        count += 1\n",
    "        yield every: count\n",
    "    }\n",
    "}\n",
);

/// UNIT-14: "Between its declarations and its run statement it may set
/// the run's store: `inject global_state`, and then calls of `put` on
/// it. Each is evaluated once, in the order written, before the run's
/// graph is built, and its value is read as a constant's is (UNIT-13),
/// with no parameter in scope." FN-9: "An application injects
/// `global_state` and no other".
#[test]
fn an_application_sets_its_runs_store_before_its_run() {
    let checked = check(&[
        ("limited.hgl", LIMITED),
        (
            "three.hgl",
            concat!(
                "application app.limited.three\n",
                "\n",
                "use app.limited::{counted}\n",
                "\n",
                "const base: i64 = 2\n",
                "\n",
                "inject global_state\n",
                "put(global_state, \"limit\", base + 1)\n",
                "put(global_state, \"venue\", \"XLON\")\n",
                "\n",
                "eval(counted())\n",
            ),
        ),
    ]);
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
    let application = checked.program.module("app.limited.three").unwrap();
    assert_eq!(application.store.len(), 3);
    // The statements are typed together: the store is the one local,
    // and each `put` a call of core's.
    let body = application.store_body.as_ref().expect("the store is set");
    let locals: Vec<String> = body
        .locals
        .iter()
        .map(|local| format!("{}: {}", local.name, local.ty))
        .collect();
    assert_eq!(locals, ["global_state: GlobalState"]);
    let puts = body
        .statements
        .iter()
        .filter(|statement| {
            matches!(
                statement,
                Stmt::Expr(value) if matches!(value.kind, ExprKind::Call { .. })
            )
        })
        .count();
    assert_eq!(puts, 2);
    // A module has none, and neither has an application that writes
    // none.
    let module = checked.program.module("app.limited").unwrap();
    assert!(module.store.is_empty() && module.store_body.is_none());
}

/// UNIT-14: "Any other statement there rejects
/// (`application.statement`). A module holds none of these." TIME-8: a
/// capability is used "for the `global_state` an application injects,
/// in the statements before its run statement".
#[test]
fn a_statement_that_sets_no_store_is_rejected() {
    let checked = check(&[
        ("limited.hgl", LIMITED),
        (
            "module.hgl",
            // In a module.
            concat!(
                "module app.other\n",
                "inject global_state\n",
                "put(global_state, \"limit\", 3)\n",
            ),
        ),
        (
            "wrong.hgl",
            concat!(
                "application app.limited.wrong\n",
                "use app.limited::{counted}\n",
                // Before a declaration.
                "inject global_state\n",
                "const base: i64 = 2\n",
                "eval(counted())\n",
                // After the run statement.
                "put(global_state, \"limit\", 3)\n",
            ),
        ),
        (
            "calls.hgl",
            concat!(
                "application app.limited.calls\n",
                "use app.limited::{counted}\n",
                // Another capability, and one beside the store.
                "inject clock\n",
                "inject global_state, logger\n",
                // A call that is no `put`, and a `put` on no store.
                "has(global_state, \"limit\")\n",
                "log_info(logger, \"set\")\n",
                "put(global_state, \"limit\", 3)\n",
                "eval(counted())\n",
            ),
        ),
    ]);
    assert_eq!(
        checked.summary(),
        [
            "calls.hgl:3:8 phase/application.statement UNIT-14",
            "calls.hgl:4:22 phase/application.statement UNIT-14",
            "calls.hgl:5:1 phase/application.statement UNIT-14",
            "calls.hgl:6:1 phase/application.statement UNIT-14",
            "module.hgl:2:1 phase/application.statement UNIT-14",
            "module.hgl:3:1 phase/application.statement UNIT-14",
            "wrong.hgl:3:1 phase/application.statement UNIT-14",
            "wrong.hgl:6:1 phase/application.statement UNIT-14",
        ],
        "{}",
        checked.report
    );
}

/// UNIT-14: "An application is one file, headed `application name`,
/// where a module's files are headed `module name`. It holds what a
/// module holds, and one run statement (RUN-1)".
#[test]
fn an_application_is_a_unit_that_holds_what_a_module_holds() {
    let checked = check(&[
        ("feed.hgl", MODULE),
        (
            "live.hgl",
            concat!(
                "application app.feed.live\n",
                "\n",
                "use app.feed::{beats}\n",
                "\n",
                "const pace: duration = 2s\n",
                "\n",
                "fn twice() -> i64 => beats(every: pace) * 2\n",
                "\n",
                "run(twice())\n",
            ),
        ),
    ]);
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
    let application = checked.program.module("app.feed.live").unwrap();
    assert!(application.application);
    assert!(!checked.program.module("app.feed").unwrap().application);
    // Its declarations are a unit's own: a constant and a function.
    let names: Vec<&str> = application
        .declarations
        .iter()
        .map(|declaration| declaration.name.as_str())
        .collect();
    assert_eq!(names, ["pace", "twice"]);
}

/// UNIT-14: "Its declarations are private: an application exports
/// nothing, and no `use` names one."
#[test]
fn an_application_exports_nothing_and_no_use_names_one() {
    let checked = check(&[
        ("feed.hgl", MODULE),
        (
            "live.hgl",
            concat!(
                "application app.feed.live\n",
                "\n",
                "use app.feed::{beats}\n",
                "\n",
                "export const pace: duration = 2s\n",
                "export fn twice() -> i64 => beats(every: pace) * 2\n",
                "\n",
                "run(twice())\n",
            ),
        ),
        (
            "other.hgl",
            concat!(
                "module app.other\n",
                "\n",
                "use app.feed.live::{twice}\n",
                "use app.feed.live as live\n",
            ),
        ),
    ]);
    assert_eq!(
        checked.summary(),
        [
            "live.hgl:5:1 module/application.export UNIT-14",
            "live.hgl:6:1 module/application.export UNIT-14",
            "other.hgl:3:5 module/application.import UNIT-14",
            "other.hgl:4:5 module/application.import UNIT-14",
        ],
        "{}",
        checked.report
    );
    // What is marked `export` is private all the same.
    let application = checked.program.module("app.feed.live").unwrap();
    assert!(
        application
            .declarations
            .iter()
            .all(|declaration| !declaration.exported)
    );
}

/// UNIT-14: an application holds "one run statement".
/// `application.run_count`: "Application with no run statement, or with
/// a second; the application's path, or the later statement".
#[test]
fn an_application_has_one_run_statement() {
    let checked = check(&[
        ("feed.hgl", MODULE),
        (
            "none.hgl",
            concat!(
                "application app.feed.none\n",
                "\n",
                "use app.feed::{beats}\n",
            ),
        ),
        (
            "two.hgl",
            concat!(
                "application app.feed.two\n",
                "\n",
                "use app.feed::{beats}\n",
                "\n",
                "run(beats())\n",
                "eval(beats(every: 2s))\n",
                "run(beats(every: 3s))\n",
            ),
        ),
    ]);
    assert_eq!(
        checked.summary(),
        [
            "none.hgl:1:13 module/application.run_count UNIT-14",
            "two.hgl:6:1 module/application.run_count UNIT-14",
            "two.hgl:7:1 module/application.run_count UNIT-14",
        ],
        "{}",
        checked.report
    );
}

/// UNIT-14: an application holds "one run statement (RUN-1), which
/// follows its declarations. A module holds none." `run.position`: "Run
/// statement in a module or before a declaration of its application
/// ...; the word".
#[test]
fn a_run_statement_stands_in_no_module_and_after_its_declarations() {
    let checked = check(&[
        (
            "feed.hgl",
            &format!("{MODULE}\nrun(beats())\neval(beats())\n"),
        ),
        (
            "early.hgl",
            concat!(
                "application app.feed.early\n",
                "\n",
                "use app.feed::{beats}\n",
                "\n",
                "run(beats())\n",
                "\n",
                "const pace: duration = 2s\n",
                "test late { assert true }\n",
            ),
        ),
    ]);
    assert_eq!(
        checked.summary(),
        [
            "early.hgl:5:1 phase/run.position UNIT-14",
            "feed.hgl:11:1 phase/run.position UNIT-14",
            "feed.hgl:12:1 phase/run.position UNIT-14",
        ],
        "{}",
        checked.report
    );
}

/// The findings of an application over [`MODULE`] and a module of
/// things that are no entry, with `run` as its run statement on line 5
/// of `live.hgl`.
fn running(run: &str) -> common::Checked {
    check(&[
        ("feed.hgl", MODULE),
        (
            "other.hgl",
            concat!(
                "module app.other\n",
                "\n",
                "export fn scaled(price: f64, const factor: f64 = 2.0) -> f64 => price * factor\n",
                "export const fn fixed() -> i64 => 1\n",
                "export struct Held { value: i64 }\n",
                "export fn paced(const every: i64) -> i64 => every\n",
            ),
        ),
        (
            "live.hgl",
            &format!(
                "application app.feed.live\n\nuse app.feed::{{beats}}\n\
                 use app.other::{{scaled, fixed, Held, paced}}\n{run}\n"
            ),
        ),
    ])
}

/// RUN-1: "An entry is a temporal function with no temporal parameters.
/// An application (UNIT-14) runs one: its run statement is `run(call,
/// config)` or `eval(call, config)`, where the call is of an entry,
/// written as any call is and giving the entry's const arguments, and
/// the configuration is a value of core's `RunConfig`, or `RunConfig()`
/// where none is written." RUN-3: "The word of a run statement is its
/// mode."
#[test]
fn a_run_statement_calls_an_entry_under_a_configuration() {
    let live = running("run(beats(every: 2s))");
    assert_eq!(live.summary(), NONE, "{}", live.report);
    let run = live
        .program
        .module("app.feed.live")
        .unwrap()
        .run_body
        .as_ref()
        .expect("the run statement is read");
    assert!(run.real_time);
    assert!(run.config.is_none());
    // The call is the entry's, with what it is given.
    let ExprKind::Call {
        callee: Callee::Function { .. },
        arguments,
        ..
    } = &run.call.kind
    else {
        panic!("a call of a function: {:?}", run.call.kind);
    };
    assert_eq!(arguments.len(), 1);
    assert_eq!(run.call.ty.to_string(), "i64");

    // `eval` is a simulation, and a configuration is a `RunConfig`.
    let backtest = running("eval((beats()), RunConfig(end: @2026-01-01T00:00:00Z))");
    assert_eq!(backtest.summary(), NONE, "{}", backtest.report);
    let run = backtest
        .program
        .module("app.feed.live")
        .unwrap()
        .run_body
        .as_ref()
        .unwrap();
    assert!(!run.real_time);
    assert_eq!(
        run.config.as_ref().map(|config| config.ty.to_string()),
        Some("RunConfig".to_string())
    );
    // A module has no run statement to read.
    assert!(
        backtest
            .program
            .module("app.feed")
            .unwrap()
            .run_body
            .is_none()
    );
}

/// RUN-1, `run.entry`: "A run statement's first argument is no call of
/// an entry; the argument". The entry's arguments are a call's, and
/// the configuration a `RunConfig`.
#[test]
fn a_run_statement_that_calls_no_entry_is_reported() {
    let cases = [
        // A function with a temporal input; a value function; a struct;
        // what is no call.
        (
            "run(scaled(price: [1.0]))",
            "live.hgl:5:5 type/run.entry RUN-1",
        ),
        ("run(fixed())", "live.hgl:5:5 type/run.entry RUN-1"),
        ("run(Held(value: 1))", "live.hgl:5:5 type/run.entry RUN-1"),
        ("eval(7)", "live.hgl:5:6 type/run.entry RUN-1"),
        ("run((beats))", "live.hgl:5:5 type/run.entry RUN-1"),
        // The entry's arguments.
        (
            "run(beats(every: 1))",
            "live.hgl:5:18 type/type.mismatch EXPR-5",
        ),
        (
            "run(beats(pace: 1s))",
            "live.hgl:5:11 name/argument.unknown EXPR-2",
        ),
        // LEX-7: an integer literal out of range, as anywhere.
        (
            "run(paced(every: 9223372036854775808))",
            "live.hgl:5:18 parse/number.integer_range LEX-7",
        ),
        // FN-3: an entry's `const` parameter is given no connection.
        (
            "run(paced(every: beats()))",
            "live.hgl:5:18 phase/connection.as_constant FN-3",
        ),
        // The configuration, and what follows it.
        ("run(beats(), 3)", "live.hgl:5:14 type/type.mismatch EXPR-5"),
        (
            "run(beats(), RunConfig(), 4)",
            "live.hgl:5:27 type/argument.unexpected RUN-1",
        ),
    ];
    for (run, expected) in cases {
        let checked = running(run);
        assert_eq!(checked.summary(), [expected], "{run}\n{}", checked.report);
    }
}

/// UNIT-14: "An application's path is no module's and no other
/// application's." `application.duplicate_path`: "the path in every
/// such file but one".
#[test]
fn an_applications_path_is_no_other_files() {
    let checked = check(&[
        ("feed.hgl", MODULE),
        (
            "a.hgl",
            "application app.feed\n\nuse app.feed.parts::{one}\n\nrun(one())\n",
        ),
        ("b.hgl", "application app.twice\n\nrun(one())\n"),
        ("c.hgl", "application app.twice\n\nrun(one())\n"),
    ]);
    // The first file of a path is its unit, and each later one is
    // reported and left out: nothing more is said of what it holds.
    assert_eq!(
        checked.summary(),
        [
            "a.hgl:1:13 module/application.duplicate_path UNIT-14",
            "b.hgl:3:5 name/name.unknown UNIT-10",
            "c.hgl:1:13 module/application.duplicate_path UNIT-14",
        ],
        "{}",
        checked.report
    );
    assert!(!checked.program.module("app.feed").unwrap().application);
    assert_eq!(checked.program.module("app.twice").unwrap().files.len(), 1);
}
