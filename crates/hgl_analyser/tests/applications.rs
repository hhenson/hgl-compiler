//! The analyser against the rules on applications and run statements
//! (UNIT-14, RUN-1, RUN-3). Each test cites its rule.
//!
//! A finding is written `file:line:column category/code rule`.

mod common;

use common::check;

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
            "c.hgl:1:13 module/application.duplicate_path UNIT-14",
        ],
        "{}",
        checked.report
    );
    assert!(!checked.program.module("app.feed").unwrap().application);
    assert_eq!(checked.program.module("app.twice").unwrap().files.len(), 1);
}
