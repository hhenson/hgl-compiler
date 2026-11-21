//! Runs `hgl run` the way a user would.
//!
//! The programs are written by the tests when they run, into a scratch
//! directory of their own each.

mod common;

use common::{argument, hgl, hgl_with, scratch, text, write};

/// A module with an entry, a source that counts, and one whose run
/// fails.
const TICKS: &str = concat!(
    "module demo.ticks\n",
    "\n",
    "export fn counted(const upto: i64, const every: duration = 1s) -> i64 {\n",
    "    cache count: i64 = 0\n",
    "    while count < upto {\n",
    "        count += 1\n",
    "        yield every: count\n",
    "    }\n",
    "}\n",
    "\n",
    "fn inverse(value: i64) -> i64 {\n",
    "    when { return 6 // (value - 2) }\n",
    "}\n",
    "\n",
    "export fn failing() -> i64 {\n",
    "    return inverse(counted(3))\n",
    "}\n",
);

/// RUN-3: `eval`, a simulation.
const BACKTEST: &str = concat!(
    "application demo.ticks.backtest\n",
    "\n",
    "use demo.ticks::{counted}\n",
    "\n",
    "inject run_config{length: 10s}\n",
    "eval counted(upto: 3)\n",
);

/// An application whose run fails.
const BROKEN: &str = concat!(
    "application demo.ticks.broken\n",
    "\n",
    "use demo.ticks::{failing}\n",
    "\n",
    "eval failing()\n",
);

/// `hgl run` runs the one application a program has, and one of
/// several only when it is named: status 2 where it cannot tell which,
/// where the name is of none, and where the program has none.
#[test]
fn the_application_to_run_is_the_only_one_or_the_one_named() {
    let root = scratch("which");
    write(&root, "ticks.hgl", TICKS);
    let output = hgl(&["run", &argument(&root)]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        text(&output.stderr),
        "hgl: the program has no application to run\n"
    );

    write(&root, "backtest.hgl", BACKTEST);
    write(&root, "broken.hgl", BROKEN);
    let output = hgl(&["run", &argument(&root)]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        text(&output.stderr),
        "hgl: the program has 2 applications; name one with `--application`: \
         demo.ticks.backtest, demo.ticks.broken\n"
    );

    let output = hgl(&["run", &argument(&root), "--application", "demo.ticks"]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        text(&output.stderr),
        "hgl: the program has no application `demo.ticks`; it has: \
         demo.ticks.backtest, demo.ticks.broken\n"
    );
}

/// A program the language rejects is not run: status 1, with its
/// errors. RUN-1: an entry has no temporal parameter (`run.entry`).
#[test]
fn a_rejected_program_is_not_run() {
    let root = scratch("rejected");
    write(
        &root,
        "app.hgl",
        concat!(
            "application demo.app\n",
            "fn pass(value: i64) -> i64 {\n",
            "    when { return value }\n",
            "}\n",
            "eval pass(value: [1])\n",
        ),
    );
    let output = hgl(&["run", &argument(&root)]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(text(&output.stdout), "not run: the program is rejected\n");
    assert!(
        text(&output.stderr).contains("run.entry"),
        "{}",
        text(&output.stderr)
    );
}

/// RUN-2: "What starts an application may replace an argument of its
/// entry or a field of its configuration with a constant checked
/// against the type it replaces". A replacement that names nothing the
/// entry or the configuration has is refused before anything is run:
/// status 2. One of the wrong type is the program's error, reported
/// where the application would have written it: status 1.
#[test]
fn a_replacement_is_checked_before_the_application_is_run() {
    let root = scratch("replacements_checked");
    write(&root, "ticks.hgl", TICKS);
    let application = write(&root, "backtest.hgl", BACKTEST);
    let run = |given: &[&str]| {
        let mut arguments = vec!["run".to_string(), argument(&root)];
        arguments.extend(given.iter().map(|given| given.to_string()));
        let arguments: Vec<&str> = arguments.iter().map(String::as_str).collect();
        hgl(&arguments)
    };

    let output = run(&["--arg", "limit=4"]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        text(&output.stderr),
        "hgl: the entry `counted` has no parameter `limit`: it has upto, every\n"
    );
    let output = run(&["--config", "until=4"]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        text(&output.stderr),
        "hgl: the configuration `RunConfig` has no field `until`: it has start, end, length\n"
    );
    // Text that would add an argument of its own is no one expression.
    let output = run(&["--arg", "upto=3, every: 2s"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        text(&output.stderr)
            .starts_with("hgl: the value of `upto`, `3, every: 2s`, is not one HGL expression"),
        "{}",
        text(&output.stderr)
    );

    // "a constant checked against the type it replaces".
    let output = run(&["--arg", "upto=\"three\""]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        text(&output.stdout),
        "not run: the program is rejected with the replacements given\n"
    );
    let reported = text(&output.stderr);
    assert!(reported.contains("type.mismatch"), "{reported}");
    // The error is where the application would have written the value,
    // and shows the statement as it was checked.
    assert!(
        reported.contains(&format!("{application}:6:21")),
        "{reported}"
    );
    assert!(
        reported.contains("eval counted(upto: (\"three\"))"),
        "{reported}"
    );
    // The same for a field of the configuration.
    let output = run(&["--config", "length=3"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        text(&output.stderr).contains("type.mismatch"),
        "{}",
        text(&output.stderr)
    );
    // A replacement is a constant: nothing of the application's file
    // that is no constant is one.
    let output = run(&["--arg", "upto=missing"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        text(&output.stderr).contains("name.unknown"),
        "{}",
        text(&output.stderr)
    );
}

/// A module with an entry that takes an enum, and a generic one.
const SIDES: &str = concat!(
    "module demo.sides\n",
    "\n",
    "export enum Side { buy, sell }\n",
    "\n",
    "export fn sided(const side: Side, const upto: i64 = 2) -> i64 {\n",
    "    cache count: i64 = 0\n",
    "    while count < upto {\n",
    "        count += 1\n",
    "        yield 1s: if side == .buy { count } else { -count }\n",
    "    }\n",
    "}\n",
    "\n",
    "export fn echo<T>(const value: T) -> T {\n",
    "    yield 1s: value\n",
    "}\n",
);

/// An application of the generic entry, whose `T` its argument makes
/// an `i64`.
const ECHOED: &str = concat!(
    "application demo.sides.echoed\n",
    "\n",
    "use demo.sides::{echo}\n",
    "\n",
    "eval echo(value: 3)\n",
);

/// RUN-2: "a constant checked against the type it replaces". The type
/// of a generic entry's parameter is the one the application's call
/// makes it: `echo(value: 3)` runs `echo<i64>`. The second read has
/// the entry's generic arguments written, so text given for `value`
/// is checked against the `i64` it replaces, and is rejected: status
/// 1, where it would otherwise have made the entry `echo<str>`.
#[test]
fn a_replacement_does_not_change_what_the_entry_is() {
    let root = scratch("replacements_generic");
    write(&root, "sides.hgl", SIDES);
    let application = write(&root, "echoed.hgl", ECHOED);
    let output = hgl(&["run", &argument(&root), "--arg", "value=\"text\""]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        text(&output.stdout),
        "not run: the program is rejected with the replacements given\n"
    );
    let reported = text(&output.stderr);
    assert!(
        reported.contains("this is `str`, and the parameter `value` is `i64`"),
        "{reported}"
    );
    assert!(
        reported.contains(&format!("{application}:5:24")),
        "{reported}"
    );
    assert!(
        reported.contains("eval echo<i64>(value: (\"text\"))"),
        "{reported}"
    );
}

/// UNIT-4: "Types have module-qualified nominal identity". Two modules
/// each have a `Side`, and a replacement of one with a member of the
/// other is of another type, though both are written `Side`: it is
/// rejected. A member of the same enum is not, and the run then gets
/// as far as its build, which here has no SDK: status 2.
#[test]
fn a_type_of_the_same_name_is_another_type() {
    let root = scratch("replacements_same_name");
    write(&root, "sides.hgl", SIDES);
    write(
        &root,
        "left.hgl",
        "module demo.left\n\nexport enum Side { buy, sell }\n",
    );
    write(
        &root,
        "right.hgl",
        "module demo.right\n\nexport enum Side { buy, sell }\n",
    );
    write(
        &root,
        "app.hgl",
        concat!(
            "application demo.app\n",
            "\n",
            "use demo.sides::{echo}\n",
            "use demo.left as left\n",
            "use demo.right as right\n",
            "\n",
            "eval echo(value: left::Side.buy)\n",
        ),
    );
    let output = hgl(&["run", &argument(&root), "--arg", "value=right::Side.sell"]);
    assert_eq!(output.status.code(), Some(1), "{}", text(&output.stderr));
    assert_eq!(
        text(&output.stderr),
        "hgl: a replacement is of the type it replaces: `demo.app` runs \
         `demo.sides::echo<Side>`, and with the replacements given it would run \
         `demo.sides::echo<Side>`, whose generic arguments are other types of the same names\n"
    );
    let missing = argument(&root.join("no-such-sdk"));
    let output = hgl_with(
        &["run", &argument(&root), "--arg", "value=left::Side.sell"],
        &[("HGL_HGRAPH_SDK", &missing)],
    );
    assert_eq!(output.status.code(), Some(2), "{}", text(&output.stderr));
    assert!(
        text(&output.stderr).contains("was not run"),
        "{}",
        text(&output.stderr)
    );
}

/// An application that needs a form the target does not write yet is
/// not run, and says what it needs and where: status 2. The target is
/// cpp, the one there is, where none is named.
#[test]
fn an_application_the_target_cannot_write_is_not_run() {
    let root = scratch("blocked");
    let file = write(
        &root,
        "app.hgl",
        concat!(
            "application demo.app\n",
            // A read of a node's own window result in its `when` is not
            // written yet.
            "fn cleared(reading: i64) -> rolling_window<i64, 2> {\n",
            "    inject out\n",
            "    when { if valid(out) { return reading } }\n",
            "}\n",
            "fn counted(readings: rolling_window<i64, 2>) -> i64 {\n",
            "    when { return delta_value(readings) }\n",
            "}\n",
            "fn later() -> i64 {\n",
            "    return counted(readings: cleared(reading: 1))\n",
            "}\n",
            "eval later()\n",
        ),
    );
    let output = hgl(&["run", &argument(&root)]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        text(&output.stderr),
        format!(
            "{file}:4:15: the cpp target does not write a read of a node's own window result in its `when` yet\n"
        )
    );
    assert_eq!(
        text(&output.stdout),
        "not run: the cpp target does not write all that demo.app needs yet\n"
    );
}
