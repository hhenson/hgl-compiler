//! Runs `hgl emit`, `hgl test` and `hgl run` with `--target cpp` the way
//! a user would.
//!
//! The tests that build and run C++ need hgraph's C++ SDK, which the
//! environment variables `HGL_HGRAPH_SDK` and `HGL_HGRAPH_DEPS` name.
//! They are ignored
//! unless asked for: `cargo test -p hgl_cli --test cpp -- --ignored`
//! runs them, as the CI job for C++ does. The CI job for hgraph runs
//! every ignored test of this crate with no C++ SDK; there they say so
//! and have nothing to check.

mod common;

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use common::{argument, scratch, text, write};

/// A module with a value function and two tests: one that the target
/// writes whole, needing no native of the runtime library's, and one it
/// does not write yet: a set of lists, whose element is no scalar.
const FLAGS: &str = concat!(
    "module demo.flags\n",
    "\n",
    "const fn yes() -> bool => true\n",
    "\n",
    "fn pass(value: set<list<bool>>) -> set<list<bool>> {\n",
    "    when { return value }\n",
    "}\n",
    "\n",
    "test holds {\n",
    "    assert yes()\n",
    "}\n",
    "\n",
    "test evaluates {\n",
    "    assert eval(pass(value: [[[true]]])) == [[[true]]]\n",
    "}\n",
);

/// The smallest program the target writes whole with the runtime
/// library's natives: a `const fn` over `i64` whose `+` is checked
/// (NUM-2), a test that passes, and one whose sum is past the largest
/// `i64`.
const CALC: &str = concat!(
    "module demo.calc\n",
    "\n",
    "const fn total(a: i64, b: i64) -> i64 => a + b\n",
    "\n",
    "test sums {\n",
    "    assert total(2, 3) == 5\n",
    "}\n",
    "\n",
    "test overflows {\n",
    "    assert total(9223372036854775807, 1) == 0\n",
    "}\n",
);

/// A module of entries over a source: one that publishes a list of
/// values a step apart, one that counts a beat for as long as its run
/// goes, one that reads the run's store and one with no result over a
/// node that writes it, a graph that sets the store where it is built
/// (FN-9), and one whose run fails.
const TICKS: &str = concat!(
    "module demo.ticks\n",
    "\n",
    "export fn ticks(const values: list<i64>, const every: duration = 1s) -> i64 {\n",
    "    for value in values {\n",
    "        yield every: value\n",
    "    }\n",
    "}\n",
    "\n",
    "fn beats(const every: duration) -> i64 {\n",
    "    while true {\n",
    "        yield every: 1\n",
    "    }\n",
    "}\n",
    "\n",
    "fn count_of(value: i64) -> i64 {\n",
    "    state count: i64 = 0\n",
    "    when {\n",
    "        count += 1\n",
    "        return count\n",
    "    }\n",
    "}\n",
    "\n",
    "export fn counted(const every: duration = 1s) -> i64 {\n",
    "    return count_of(beats(every))\n",
    "}\n",
    "\n",
    "export fn doubled(const values: list<i64>) -> i64 {\n",
    "    return ticks(values) * 2\n",
    "}\n",
    "\n",
    "fn limited(value: i64) -> i64 {\n",
    "    inject global_state\n",
    "    when {\n",
    "        if value <= get<i64>(global_state, \"limit\") { return value }\n",
    "    }\n",
    "}\n",
    "\n",
    "export fn bounded(const values: list<i64>) -> i64 {\n",
    "    return limited(ticks(values))\n",
    "}\n",
    "\n",
    "export fn seeded(const limit: i64, const values: list<i64>) -> i64 {\n",
    "    inject global_state\n",
    "    put(global_state, \"limit\", limit)\n",
    "    return limited(ticks(values, every: get<i64>(global_state, \"limit\") * 1s))\n",
    "}\n",
    "\n",
    "fn kept(value: i64) {\n",
    "    inject global_state\n",
    "    when { put(global_state, \"last\", value) }\n",
    "}\n",
    "\n",
    "export fn quiet(const values: list<i64>) {\n",
    "    kept(ticks(values))\n",
    "}\n",
    "\n",
    "fn inverse(value: i64) -> i64 {\n",
    "    when { return 6 // (value - 2) }\n",
    "}\n",
    "\n",
    "export fn failing() -> i64 {\n",
    "    return inverse(ticks([1, 2, 3]))\n",
    "}\n",
);

/// RUN-3: `eval`, a simulation.
const BACKTEST: &str = concat!(
    "application demo.ticks.backtest\n",
    "\n",
    "use demo.ticks::{doubled}\n",
    "\n",
    "const rounds: list<i64> = [1, 2, 3]\n",
    "\n",
    "eval(doubled(values: rounds))\n",
);

/// UNIT-14: an application that sets the run's store before its run.
const STORED: &str = concat!(
    "application demo.ticks.stored\n",
    "\n",
    "use demo.ticks::{bounded}\n",
    "\n",
    "const fn sum<T>(a: T, b: T) -> T requires add_(T, T) -> T => a + b\n",
    "\n",
    "inject global_state\n",
    "put(key: \"limit\", store: global_state, value: sum(1, 1))\n",
    "\n",
    "eval(bounded(values: [1, 2, 3, 4]))\n",
);

/// FN-9: one whose entry is a graph that sets the store as it is built.
const SEEDED: &str = concat!(
    "application demo.ticks.seeded\n",
    "\n",
    "use demo.ticks::{seeded}\n",
    "\n",
    "eval(seeded(limit: 2, values: [1, 2, 3]))\n",
);

/// An entry with no result: the run has no output (CRUN-8).
const SILENT: &str = concat!(
    "application demo.ticks.silent\n",
    "\n",
    "use demo.ticks::{quiet}\n",
    "\n",
    "eval(quiet(values: [1, 2]))\n",
);

/// An application whose run fails.
const BROKEN: &str = concat!(
    "application demo.ticks.broken\n",
    "\n",
    "use demo.ticks::{failing}\n",
    "\n",
    "eval(failing())\n",
);

/// CRUN-2: a simulation from a start of its own, for a length: its end
/// is exclusive.
const LATER: &str = concat!(
    "application demo.ticks.later\n",
    "\n",
    "use demo.ticks::{doubled}\n",
    "\n",
    "eval(doubled(values: [1, 2, 3, 4]), RunConfig(start: @2026-01-02T00:00:00Z, length: 3s))\n",
);

/// CRUN-2: a configuration with both an `end` and a `length`, refused
/// before the run's graph is built.
const BOTH: &str = concat!(
    "application demo.ticks.both\n",
    "\n",
    "use demo.ticks::{doubled}\n",
    "\n",
    "eval(doubled(values: [1]), RunConfig(end: @2026-01-02T00:00:00Z, length: 3s))\n",
);

/// RUN-3: `run`, in real time, here for a fifth of a second: it ends at
/// its end, with nothing asked of it.
const LIVE: &str = concat!(
    "application demo.ticks.live\n",
    "\n",
    "use demo.ticks::{counted}\n",
    "\n",
    "run(counted(every: 50ms), RunConfig(length: 220ms))\n",
);

/// RUN-3: `run`, in real time, with no end: until its host stops it.
const ENDLESS: &str = concat!(
    "application demo.ticks.endless\n",
    "\n",
    "use demo.ticks::{counted}\n",
    "\n",
    "run(counted(every: 20ms))\n",
);

/// Runs `hgl` with `arguments`, with `set` in its environment and each
/// of `unset` taken out of it.
fn hgl_in(arguments: &[&str], set: &[(&str, &str)], unset: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_hgl"));
    command.args(arguments).envs(set.iter().copied());
    for name in unset {
        command.env_remove(name);
    }
    command.output().expect("the hgl binary runs")
}

/// With no SDK named, `hgl test --target cpp` says so plainly for each
/// test it could run, and still lists one it does not write with what
/// it lacks: nothing is built, and nothing passes.
#[test]
fn with_no_sdk_named_the_tests_are_not_run_and_it_is_said() {
    let root = scratch("no_sdk");
    let file = write(&root, "flags.hgl", FLAGS);
    let output = hgl_in(
        &["test", &argument(&root), "--target", "cpp"],
        &[],
        &["HGL_HGRAPH_SDK", "HGL_HGRAPH_DEPS"],
    );
    assert_eq!(output.status.code(), Some(2), "{}", text(&output.stderr));
    assert_eq!(
        text(&output.stdout),
        format!(
            "not run  executable  demo.flags::holds\n    \
             HGL_HGRAPH_SDK is not set: set it to the install prefix of hgraph's C++ SDK, \
             the directory that holds `lib/cmake/hgraph`\n\
             not run  executable  demo.flags::evaluates\n    \
             {file}:14:29: the cpp target does not write a temporal value of `set<list<bool>>` yet\n\
             rejection cases: 0 passed, 0 failed\n\
             executable cases: 0 passed, 0 failed, 2 not run\n"
        )
    );

    // A variable that names a directory without what it should hold is
    // named with what it names.
    let wrong = argument(&root);
    let output = hgl_in(
        &["test", &file, "--target", "cpp", "--test", "holds"],
        &[("HGL_HGRAPH_SDK", &wrong)],
        &["HGL_HGRAPH_DEPS"],
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(
        text(&output.stdout).contains(&format!(
            "HGL_HGRAPH_SDK names {wrong}, which holds no `lib/cmake/hgraph/hgraphConfig.cmake`"
        )),
        "{}",
        text(&output.stdout)
    );
}

/// A compiler is known by the macros it defines, not by its name, and
/// must be of the platform's family: one that is not says what was
/// found and what is needed, GCC on Linux and Apple clang on macOS. The
/// SDK named is only what the driver looks for before the compiler.
#[cfg(unix)]
#[test]
fn a_compiler_not_of_the_platforms_family_is_named_and_refused() {
    let root = scratch("compiler");
    let file = write(&root, "flags.hgl", FLAGS);
    let sdk = root.join("sdk");
    fs::create_dir_all(sdk.join("lib/cmake/hgraph")).expect("the scratch directory is writable");
    fs::write(sdk.join("lib/cmake/hgraph/hgraphConfig.cmake"), "")
        .expect("the scratch directory is writable");
    for part in ["include", "lib"] {
        fs::create_dir_all(root.join("deps").join(part))
            .expect("the scratch directory is writable");
    }
    // `true` runs, and defines nothing.
    let output = hgl_in(
        &["test", &file, "--target", "cpp", "--test", "holds"],
        &[
            ("HGL_HGRAPH_SDK", &argument(&sdk)),
            ("HGL_HGRAPH_DEPS", &argument(&root.join("deps"))),
            ("CXX", "true"),
        ],
        &[],
    );
    let said = text(&output.stdout);
    let needed = if cfg!(target_os = "macos") {
        "the cpp target needs Apple clang 21 or later here, named by CXX or on the PATH as one of clang++"
    } else {
        "the cpp target needs GCC 15 or later here, named by CXX or on the PATH as one of g++-17, g++-16, \
         g++-15, g++"
    };
    assert_eq!(output.status.code(), Some(2), "{said}");
    assert!(said.contains(needed), "{said}");
    assert!(said.contains("(neither GCC nor Apple clang)"), "{said}");
}

/// `hgl emit --target cpp` writes each module's header and unit, core's
/// among them with core's types header, and an entry unit for its tests;
/// a program with a form
/// the target does not write yet is not written.
#[test]
fn emit_writes_a_unit_a_header_and_an_entry() {
    let root = scratch("emit");
    let source = write(
        &root,
        "src/flags.hgl",
        concat!(
            "module demo.flags\n",
            "const fn yes() -> bool => true\n",
            "test holds {\n",
            "    assert yes()\n",
            "}\n",
        ),
    );
    let out = root.join("out");
    let output = hgl_in(
        &["emit", &source, "--target", "cpp", "--out", &argument(&out)],
        &[],
        &[],
    );
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert_eq!(
        text(&output.stdout),
        format!("wrote 6 files for cpp to {}\n", argument(&out))
    );
    for file in [
        "modules/core.types.h",
        "modules/core.h",
        "modules/core.cpp",
        "modules/demo.flags.h",
        "modules/demo.flags.cpp",
        "tests/demo.flags.cpp",
    ] {
        assert!(out.join(file).is_file(), "{file}");
    }

    let source = write(&root, "src/flags.hgl", FLAGS);
    let output = hgl_in(
        &["emit", &source, "--target", "cpp", "--out", &argument(&out)],
        &[],
        &[],
    );
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        text(&output.stdout),
        "nothing written: the cpp target does not write all of the program yet\n"
    );
}

/// `hgl run --target cpp` says what an application needs that the
/// target does not write, and runs nothing: here a read of the pending
/// window result while its evaluation holds an arrival (TYPE-11).
#[test]
fn run_says_what_an_application_lacks() {
    let root = scratch("run");
    let file = write(
        &root,
        "app.hgl",
        concat!(
            "application demo.app\n",
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
            "eval(later())\n",
        ),
    );
    let output = hgl_in(&["run", &argument(&root), "--target", "cpp"], &[], &[]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        text(&output.stdout),
        "not run: the cpp target does not write all that demo.app needs yet\n"
    );
    assert_eq!(
        text(&output.stderr),
        format!(
            "{file}:4:15: the cpp target does not write a read of a node's own window result in its `when` yet\n"
        )
    );
}

/// With no SDK named, `hgl run --target cpp` of an application the
/// target writes says so, and runs nothing: status 2.
#[test]
fn run_with_no_sdk_named_says_so() {
    let root = scratch("run_no_sdk");
    write(&root, "ticks.hgl", TICKS);
    write(&root, "backtest.hgl", BACKTEST);
    let output = hgl_in(
        &["run", &argument(&root), "--target", "cpp"],
        &[],
        &["HGL_HGRAPH_SDK", "HGL_HGRAPH_DEPS"],
    );
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(text(&output.stdout), "");
    assert_eq!(
        text(&output.stderr),
        "hgl: demo.ticks.backtest was not run: HGL_HGRAPH_SDK is not set: set it to the install \
         prefix of hgraph's C++ SDK, the directory that holds `lib/cmake/hgraph`\n"
    );
}

/// Whether the C++ SDK is named, and with it the tests that build. The
/// CI job for hgraph runs every ignored test with none named: there a
/// test that needs it says so and checks nothing.
fn sdk_named() -> bool {
    let named = std::env::var_os("HGL_HGRAPH_SDK").is_some_and(|named| !named.is_empty());
    if !named {
        eprintln!("HGL_HGRAPH_SDK is not set: there is no C++ SDK to build with");
    }
    named
}

/// The whole route on the SDK: the program is written, the runtime
/// library and the precompiled header are made for the SDK, the units
/// compiled, the executable linked and run, and what it records read
/// back: a test passes, and one fails at its statement with the code
/// of the checked sum's failure. A second run with nothing changed
/// builds nothing.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn the_whole_route_runs_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("route");
    let file = write(&root, "src/calc.hgl", CALC);
    let kept = root.join("build");
    let arguments = ["test", &file, "--target", "cpp", "--keep", &argument(&kept)];
    let output = hgl_in(&arguments, &[], &[]);
    let stdout = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(1),
        "{stdout}{}",
        text(&output.stderr)
    );
    let rows: Vec<&str> = stdout.lines().collect();
    assert_eq!(rows[0], "passed   executable  demo.calc::sums", "{stdout}");
    assert_eq!(
        rows[1], "FAILED   executable  demo.calc::overflows",
        "{stdout}"
    );
    assert!(
        rows[2].starts_with(&format!("    {file}:10:5: "))
            && rows[2].contains("numeric.out_of_range"),
        "{stdout}"
    );
    assert_eq!(
        &rows[3..],
        [
            "rejection cases: 0 passed, 0 failed",
            "executable cases: 1 passed, 1 failed, 0 not run"
        ]
    );

    let executable = kept.join("bin").join("demo.calc");
    let built = fs::metadata(&executable).unwrap().modified().unwrap();
    let again = hgl_in(&arguments, &[], &[]);
    assert_eq!(text(&again.stdout), stdout);
    assert_eq!(
        fs::metadata(&executable).unwrap().modified().unwrap(),
        built,
        "nothing changed, and nothing is linked again"
    );
}

/// TYPE-17, TEN-2 and TEN-3 through generated C++ against the released SDK.
/// The two zero forms are unordered, arithmetic keeps its form, and mixing
/// forms is an HGL execution failure.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn tenors_use_the_shared_sdk_scalar() {
    if !sdk_named() {
        return;
    }
    let root = scratch("shared_tenor");
    let file = write(
        &root,
        "src/tenors.hgl",
        r#"module demo.tenors
const fn plus(a: tenor, b: tenor) -> tenor => a + b
const fn before(a: tenor, b: tenor) -> bool => a < b
const fn at_most(a: tenor, b: tenor) -> bool => a <= b

test counts {
    assert before(plus(1Y2M, 1D), 1Y2M2D)
    assert before(plus(2B, 1B), 4B)
    assert !at_most(0D, 0B)
    assert !at_most(0B, 0D)
}

test mixed {
    assert at_most(plus(0B, 0D), 0D)
}
"#,
    );
    let output = hgl_in(&["test", &file, "--target", "cpp"], &[], &[]);
    let report = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(1),
        "{report}{}",
        text(&output.stderr)
    );
    assert!(
        report.contains("passed   executable  demo.tenors::counts"),
        "{report}"
    );
    assert!(
        report.contains("FAILED   executable  demo.tenors::mixed"),
        "{report}"
    );
    assert!(report.contains("tenor.mixed_units"), "{report}");
}

/// A build that fails runs no executable it did not finish, even where
/// an older one is there: after a run that passed, the place of the
/// module's object is taken by a directory, so that `ninja` compiles it
/// again and the compile cannot write it; the test is then not run,
/// with the failure, where the old executable would have passed it.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn a_failed_build_runs_no_older_executable() {
    if !sdk_named() {
        return;
    }
    let root = scratch("failed_build");
    let file = write(&root, "src/calc.hgl", CALC);
    let kept = root.join("build");
    let arguments = [
        "test",
        &file,
        "--target",
        "cpp",
        "--keep",
        &argument(&kept),
        "--test",
        "sums",
    ];
    let output = hgl_in(&arguments, &[], &[]);
    assert_eq!(
        text(&output.stdout).lines().next(),
        Some("passed   executable  demo.calc::sums"),
        "{}{}",
        text(&output.stdout),
        text(&output.stderr)
    );
    let executable = kept.join("bin").join("demo.calc");
    assert!(executable.is_file());

    block(&kept.join("obj").join("modules").join("demo.calc.o"));
    let output = hgl_in(&arguments, &[], &[]);
    let stdout = text(&output.stdout);
    assert_eq!(output.status.code(), Some(2), "{stdout}");
    let rows: Vec<&str> = stdout.lines().collect();
    assert_eq!(rows[0], "not run  executable  demo.calc::sums", "{stdout}");
    assert_eq!(
        rows[1], "    the build of its executable failed:",
        "{stdout}"
    );
    assert!(stdout.contains("FAILED: "), "{stdout}");
    assert!(
        stdout.ends_with("executable cases: 0 passed, 0 failed, 1 not run\n"),
        "{stdout}"
    );
}

/// Puts a directory in the place of the object at `path`: `ninja`
/// takes the object for changed, and a compile can write no object
/// there.
fn block(path: &Path) {
    assert!(path.is_file(), "{} was built", path.display());
    fs::remove_file(path).unwrap();
    fs::create_dir(path).unwrap();
}

/// Value code over the scalars and the plain composites, the emitter's
/// own test program: constants, an enum, `if`, `switch`, `while` and
/// `for`, places, defaults, a generic function by instance, tuples,
/// results and `?`, a `var` parameter and `assert raises`. Every test
/// of it passes on the SDK. With `HGL_CPP_FLAGS` naming the address and
/// undefined-behaviour sanitizers, as the CI job for C++ runs it again,
/// the same is true of the sanitized build.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn value_code_runs_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("values");
    let file = write(
        &root,
        "src/demo.values.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.values.hgl"),
    );
    let output = hgl_in(&["test", &file, "--target", "cpp"], &[], &[]);
    let stdout = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{stdout}{}",
        text(&output.stderr)
    );
    assert!(
        stdout.ends_with("executable cases: 16 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

/// The temporal forms of a list and of a tuple on the SDK (`demo.lists`):
/// each test passes, a failing child read among them.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn list_code_runs_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("lists");
    let file = write(
        &root,
        "src/demo.lists.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.lists.hgl"),
    );
    let output = hgl_in(&["test", &file, "--target", "cpp"], &[], &[]);
    let stdout = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{stdout}{}",
        text(&output.stderr)
    );
    assert!(
        stdout.ends_with("executable cases: 7 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

/// Forms made of other families' forms on the SDK (`demo.nested`): a list
/// of tuples, growing and fixed, a tuple of a list, a dictionary of tuples
/// and a struct with a list field, each relayed, published by a delta and
/// replayed, each part through its own form (TYPE-9); a child of a list of
/// tuples and a key's child of a dictionary of tuples as connections of a
/// graph (TIME-9, EXPR-7); a struct with a list field, and a list of them,
/// as a node's configuration (FN-1), and one in the run's store (CAP-6).
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn nested_forms_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("nested");
    let file = write(
        &root,
        "src/demo.nested.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.nested.hgl"),
    );
    let output = hgl_in(&["test", &file, "--target", "cpp"], &[], &[]);
    let stdout = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{stdout}{}",
        text(&output.stderr)
    );
    assert!(
        stdout.ends_with("executable cases: 18 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

/// The temporal code of scalars on the SDK: a graph with lifted calls, a
/// constant connection, nodes with state, handlers in order and
/// capabilities, a source, and the evals of them (`demo.temporal`), each
/// as the hgraph target gives it.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn temporal_code_runs_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("temporal");
    let file = write(
        &root,
        "src/demo.temporal.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.temporal.hgl"),
    );
    let output = hgl_in(&["test", &file, "--target", "cpp"], &[], &[]);
    let stdout = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{stdout}{}",
        text(&output.stderr)
    );
    assert!(
        stdout.ends_with("executable cases: 8 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

/// `const` parameters of every kind of ordinary value on the SDK
/// (`demo.configurations`, FN-1): a set, a dictionary, a tuple, a struct
/// that holds a list, one that holds a set, one with an optional field,
/// nullables and lists of them, and a nullable tested
/// against null (ERR-12), given to sources, nodes, lifted calls and a
/// branch; and two nodes of one input whose configurations differ, by a
/// value or by the sign of a zero (hgraph issue 1866), which stay two.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn configurations_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("configurations");
    let file = write(
        &root,
        "src/demo.configurations.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.configurations.hgl"),
    );
    let output = hgl_in(&["test", &file, "--target", "cpp"], &[], &[]);
    let stdout = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{stdout}{}",
        text(&output.stderr)
    );
    assert!(
        stdout.ends_with("executable cases: 4 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

/// Windows on the SDK (`demo.windows`): of a tick count and of a
/// duration, values leaving one by age, their reads checked when they
/// run, a lifted helper given one, a window recorded and replayed, and two
/// writes of a window result in one evaluation, the last of which arrives
/// (TYPE-11; core's END-9).
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn windows_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("windows");
    let file = write(
        &root,
        "src/demo.windows.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.windows.hgl"),
    );
    let withdrawal = write(
        &root,
        "src/demo.window_withdrawal.hgl",
        include_str!("fixtures/demo.window_withdrawal.hgl"),
    );
    let output = hgl_in(&["test", &file, &withdrawal, "--target", "cpp"], &[], &[]);
    let stdout = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{stdout}{}",
        text(&output.stderr)
    );
    assert!(
        stdout.ends_with("executable cases: 15 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

/// The `ref` parameters and results on the SDK (`demo.references`): what
/// an ordinary reader of a `ref` result observes, what wakes a node of a
/// `ref` parameter, a sampled designation, and a designated input whose
/// re-binding its reader follows (FN-16, TIME-10; core's END-10).
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn references_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("references");
    let file = write(
        &root,
        "src/demo.references.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.references.hgl"),
    );
    let output = hgl_in(&["test", &file, "--target", "cpp"], &[], &[]);
    let stdout = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{stdout}{}",
        text(&output.stderr)
    );
    assert!(
        stdout.ends_with("executable cases: 15 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

/// The timing of nodes and sources on the SDK (`demo.timing`): nodes woken
/// by a scheduler or an alarm, alone or with an input's tick, `scheduled()`
/// and `modified([])` read as values, a wake dropped and one that fails,
/// an alarm's wake kept through an input's tick before it and the earliest
/// asked for kept (CAP-4, CAP-5, TIME-3, TIME-8, TIME-9), and sources that
/// read their clock, keep a `cache` or a `state` and have `start` and
/// `stop` (SOURCE-1 to SOURCE-4), and runs whose configuration is a `RunConfig`
/// value held or made elsewhere (RUN-1). Every test of it passes.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn timing_runs_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("timing");
    let file = write(
        &root,
        "src/demo.timing.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.timing.hgl"),
    );
    let output = hgl_in(&["test", &file, "--target", "cpp"], &[], &[]);
    let stdout = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{stdout}{}",
        text(&output.stderr)
    );
    assert!(
        stdout.ends_with("executable cases: 17 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

/// The activity of a node's inputs and the temporal form of an enum on
/// the SDK (`demo.activity`): a passive input wakes no node and is read
/// where another wakes it, activity belongs to each binding of one
/// output (END-5), and an enum is published, replayed and wired as a
/// constant.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn activity_runs_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("activity");
    let file = write(
        &root,
        "src/demo.activity.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.activity.hgl"),
    );
    let output = hgl_in(&["test", &file, "--target", "cpp"], &[], &[]);
    let stdout = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{stdout}{}",
        text(&output.stderr)
    );
    assert!(
        stdout.ends_with("executable cases: 4 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

/// Lifting and capture on the SDK (`demo.lifting`): lifted calls binding
/// defaults, a sink, an eval of a value function, captures of a node, a
/// graph and a lifted call, a write that stands before a failure, a `?` in
/// a `when` and an error's context, each as the hgraph target gives it.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn lifting_code_runs_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("lifting");
    let file = write(
        &root,
        "src/demo.lifting.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.lifting.hgl"),
    );
    let output = hgl_in(&["test", &file, "--target", "cpp"], &[], &[]);
    let stdout = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{stdout}{}",
        text(&output.stderr)
    );
    assert!(
        stdout.ends_with("executable cases: 9 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

/// Control flow over connections on the SDK (`demo.control`): each
/// branch of an `if` or a `switch` over a connection a child graph that
/// hgraph's `compiled::switch_` runs while it is selected, started fresh
/// each time (CTRL-2, CTRL-13 to CTRL-15). Every test of it passes, each
/// expectation the language's.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn control_flow_runs_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("control");
    let file = write(
        &root,
        "src/demo.control.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.control.hgl"),
    );
    let output = hgl_in(&["test", &file, "--target", "cpp"], &[], &[]);
    let stdout = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{stdout}{}",
        text(&output.stderr)
    );
    assert!(
        stdout.ends_with("executable cases: 18 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

/// CTRL-6: "An unmatched selector uses default or fails." A `switch`
/// over a connection with no `default` fails in the cycle its selector
/// has a value no case has, with no code, and says the value.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn an_unmatched_connection_selector_fails_the_run() {
    if !sdk_named() {
        return;
    }
    let root = scratch("unmatched_connection");
    let file = write(
        &root,
        "src/demo.routed.hgl",
        concat!(
            "module demo.routed\n",
            "\n",
            "fn routed(route: str, value: i64) -> i64 {\n",
            "    var out = value\n",
            "    switch route {\n",
            "        case \"a\": out = value + 1\n",
            "    }\n",
            "    return out\n",
            "}\n",
            "\n",
            "test another_value {\n",
            "    assert eval(routed(route: [\"a\", \"c\"], value: [1, 2])) == [2, 3]\n",
            "}\n",
        ),
    );
    let output = hgl_in(&["test", &file, "--target", "cpp"], &[], &[]);
    let stdout = text(&output.stdout);
    let rows: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        rows[0], "FAILED   executable  demo.routed::another_value",
        "{stdout}"
    );
    assert!(
        rows[1].starts_with(&format!("    {file}:12:5: "))
            && rows[1].contains("no case of the `switch` has the value \"c\""),
        "{stdout}"
    );
}

/// `demo.structs`, the temporal forms of a struct, a tuple, an atomic
/// struct and a nullable scalar (TYPE-9, TYPE-15, TYPE-16, TIME-7, TIME-9,
/// EXPR-7): every test of it passes on the SDK, and under the sanitizers
/// where `HGL_CPP_FLAGS` names them.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn temporal_structs_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("structs");
    let file = write(
        &root,
        "src/demo.structs.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.structs.hgl"),
    );
    let output = hgl_in(&["test", &file, "--target", "cpp"], &[], &[]);
    let stdout = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{stdout}{}",
        text(&output.stderr)
    );
    assert!(
        stdout.ends_with("executable cases: 23 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

/// `demo.keyed`, the temporal form of a set and of a dictionary (TYPE-9,
/// TIME-7, TIME-9, END-3, END-4, TST-9): every test of it passes on the
/// SDK, and under the sanitizers where `HGL_CPP_FLAGS` names them.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn keyed_temporal_code_runs_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("keyed");
    let file = write(
        &root,
        "src/demo.keyed.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.keyed.hgl"),
    );
    let output = hgl_in(&["test", &file, "--target", "cpp"], &[], &[]);
    let stdout = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{stdout}{}",
        text(&output.stderr)
    );
    assert!(
        stdout.ends_with("executable cases: 10 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

/// `demo.composites`, structs, sets, dictionaries and deltas as values
/// (TYPE-3 to TYPE-6, TYPE-14, TYPE-16, EXPR-3, ERR-6, CTRL-12): every
/// test of it passes on the SDK, and under the sanitizers where
/// `HGL_CPP_FLAGS` names them, as the CI job for C++ runs it again.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn composite_values_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("composites");
    let file = write(
        &root,
        "src/demo.composites.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.composites.hgl"),
    );
    let output = hgl_in(&["test", &file, "--target", "cpp"], &[], &[]);
    let stdout = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{stdout}{}",
        text(&output.stderr)
    );
    assert!(
        stdout.ends_with("executable cases: 16 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

/// Two modules, one of whose structs holds the other's, with an instance
/// of the other's generic function over it: each is built, and its tests
/// pass (GEN-2, UNIT-6).
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn a_struct_that_holds_another_modules_is_built() {
    if !sdk_named() {
        return;
    }
    let root = scratch("struct-modules");
    write(
        &root,
        "src/demo.inner.hgl",
        concat!(
            "module demo.inner\n",
            "export struct Inner { value: i64 }\n",
            "export const fn same<T>(value: T) -> T => value\n",
        ),
    );
    write(
        &root,
        "src/demo.outer.hgl",
        concat!(
            "module demo.outer\n",
            "use demo.inner::{Inner, same}\n",
            "struct Outer { inner: Inner }\n",
            "test each_names_the_other {\n",
            "    let outer = Outer(inner: Inner(value: 4))\n",
            "    assert same(outer) == outer && same(outer).inner.value == 4\n",
            "}\n",
        ),
    );
    let source = argument(&root.join("src"));
    let output = hgl_in(&["test", &source, "--target", "cpp"], &[], &[]);
    let stdout = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{stdout}{}",
        text(&output.stderr)
    );
    assert!(
        stdout.contains("passed   executable  demo.outer::each_names_the_other"),
        "{stdout}"
    );
}

/// UNIT-12: "A member name is not a binding and may match any name", and
/// GEN-1's parameters are named as any declaration is. A module whose
/// path has a part named `std`, a struct with a field of its own name,
/// and generic parameters named as what a struct's C++ declares (a
/// hash's argument, the list of its fields, a comparison's operands)
/// and as a field: each is built, and its tests pass.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn names_the_generated_cpp_declares_are_built() {
    if !sdk_named() {
        return;
    }
    let root = scratch("declared-names");
    write(
        &root,
        "src/demo.named.hgl",
        concat!(
            "module demo.named\n",
            "struct value {\n",
            "    value: i64\n",
            "}\n",
            "test a_field_named_as_its_struct {\n",
            "    assert value(value: 1).value == 1\n",
            "    assert delta<value>(value: 2).value == 2\n",
            "}\n",
        ),
    );
    write(
        &root,
        "src/demo.std.hgl",
        concat!(
            "module demo.std\n",
            "struct Point {\n",
            "    x: f64\n",
            "    note: str? = null\n",
            "}\n",
            "struct Limited<const value: i64> {\n",
            "    n: i64\n",
            "}\n",
            "struct Group<const members: i64> {\n",
            "    n: i64\n",
            "}\n",
            "abstract struct Compared<const lhs: i64> {\n",
            "    n: i64\n",
            "}\n",
            "struct One : Compared<1> {}\n",
            "abstract struct Capped<const value: i64> {\n",
            "    n: i64\n",
            "}\n",
            "struct Ten : Capped<10> {}\n",
            "struct Counted<const count: i64> {\n",
            "    count: i64\n",
            "}\n",
            "struct Carton<item> {\n",
            "    item: item\n",
            "}\n",
            "const fn read(capped: Capped<10>) -> i64 => capped.n\n",
            "const fn same(a: Compared<1>, b: Compared<1>) -> bool => a == b\n",
            "test each_is_built {\n",
            "    assert Point(x: 1.0).note == null\n",
            "    assert Limited<10>(n: 1).n == 1\n",
            "    assert Group<3>(n: 1).n == 1\n",
            "    assert same(One(n: 1), One(n: 1))\n",
            "    assert read(Ten(n: 1)) == 1\n",
            "    assert Counted<3>(count: 2).count == 2\n",
            "    assert Carton<i64>(item: 2).item == 2\n",
            "}\n",
        ),
    );
    let source = argument(&root.join("src"));
    let output = hgl_in(&["test", &source, "--target", "cpp"], &[], &[]);
    let stdout = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{stdout}{}",
        text(&output.stderr)
    );
    for test in [
        "demo.named::a_field_named_as_its_struct",
        "demo.std::each_is_built",
    ] {
        assert!(
            stdout.contains(&format!("passed   executable  {test}")),
            "{stdout}"
        );
    }
}

/// TYPE-16: a nullable's delta is a type of its own, which holds what was
/// published, a null among them. A temporal `i64?` read by its delta,
/// published by one, and relayed, its trace's cells written as values and
/// as deltas (TST-5), and a temporal `list<i64?>` relayed and read by its
/// delta, a published null at a position among it: each runs, and its
/// tests pass.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn a_nullables_delta_is_read_and_published() {
    if !sdk_named() {
        return;
    }
    let root = scratch("nullable-deltas");
    let file = write(
        &root,
        "src/demo.deltas.hgl",
        concat!(
            "module demo.deltas\n",
            "fn relay(value: i64?) -> i64? {\n",
            "    when { return value }\n",
            "}\n",
            "fn read(value: i64?) -> i64? {\n",
            "    when { return delta_value(value) }\n",
            "}\n",
            "fn published(step: i64) -> i64? {\n",
            "    when {\n",
            "        if step == 0 { return delta<i64?>() }\n",
            "        if step == 1 { return delta<i64?>(value: 7) }\n",
            "        return null\n",
            "    }\n",
            "}\n",
            "fn relay_list(values: list<i64?>) -> list<i64?> {\n",
            "    when { return values }\n",
            "}\n",
            "fn read_list(values: list<i64?>) -> list<i64?> {\n",
            "    when { return delta_value(values) }\n",
            "}\n",
            "test a_nullable_publishes_its_delta {\n",
            "    assert eval(relay(value: [1, null, 2])) == [1, null, 2]\n",
            "    assert eval(read(value: [1, null, 2])) == [1, null, 2]\n",
            "    assert eval(published(step: [0, 1, 2])) == [null, 7, null]\n",
            "    # A cell written as the delta it publishes, a published null among them.\n",
            "    assert eval(relay(value: [delta<i64?>(), delta<i64?>(value: 3), 4])) ==\n",
            "        [null, delta<i64?>(value: 3), delta<i64?>(value: 4)]\n",
            "    assert eval(read(value: [delta<i64?>(), 5])) == [delta<i64?>(), 5]\n",
            "}\n",
            "test a_list_of_nullables_publishes_its_delta {\n",
            "    assert eval(relay_list(values: [[1, null], [null, 2]])) == [[1, null], [null, 2]]\n",
            "    assert eval(read_list(values: [[1, null], delta<list<i64?>>(updated: [0: delta<i64?>()])])) ==\n",
            "        [[1, null], delta<list<i64?>>(updated: [0: delta<i64?>()])]\n",
            "}\n",
        ),
    );
    let output = hgl_in(&["test", &file, "--target", "cpp"], &[], &[]);
    let stdout = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{stdout}{}",
        text(&output.stderr)
    );
    assert!(
        stdout.ends_with("executable cases: 2 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

/// LEX-12: "The selected run provider checks exact catalog membership and
/// offset consistency." A zone the provider's catalogue does not have,
/// of a `timezone`, a `zoned_time` and a `zoned_datetime`, and an offset
/// that is not the zone's at the instant written, fail where the literal
/// is evaluated, as the hgraph target's do; an offset that is the zone's
/// does not.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn a_zone_literal_is_checked_by_the_run_provider() {
    if !sdk_named() {
        return;
    }
    let root = scratch("zones");
    let file = write(
        &root,
        "src/demo.zones.hgl",
        concat!(
            "module demo.zones\n",
            "test a_zone_not_in_the_catalogue {\n",
            "    let zone = @[Nowhere/Atlantis]\n",
            "    assert zone == zone\n",
            "}\n",
            "test a_time_in_a_zone_not_in_the_catalogue {\n",
            "    let opening = @09:30[Nowhere/Atlantis]\n",
            "    assert opening == opening\n",
            "}\n",
            "test an_instant_in_a_zone_not_in_the_catalogue {\n",
            "    let at = @2026-09-03T09:30+01:00[Nowhere/Atlantis]\n",
            "    assert at == at\n",
            "}\n",
            "test an_offset_the_zone_does_not_have {\n",
            "    let at = @2026-09-03T08:30+00:00[Europe/London]\n",
            "    assert at == at\n",
            "}\n",
            "test an_offset_the_zone_has {\n",
            "    let at = @2026-09-03T09:30+01:00[Europe/London]\n",
            "    assert at == @2026-09-03T09:30+01:00[Europe/London]\n",
            "}\n",
        ),
    );
    let output = hgl_in(&["test", &file, "--target", "cpp"], &[], &[]);
    let stdout = text(&output.stdout);
    let absent = "the zone `Nowhere/Atlantis` is not in the run provider's zone catalogue";
    for (test, line, message) in [
        ("a_zone_not_in_the_catalogue", 3, absent),
        ("a_time_in_a_zone_not_in_the_catalogue", 7, absent),
        ("an_instant_in_a_zone_not_in_the_catalogue", 11, absent),
        (
            "an_offset_the_zone_does_not_have",
            15,
            "the offset of `2026-09-03T08:30:00` is not the offset of the zone `Europe/London` at that time",
        ),
    ] {
        let row = format!("FAILED   executable  demo.zones::{test}\n    {file}:{line}:");
        let at = stdout
            .find(&row)
            .unwrap_or_else(|| panic!("{row}\n{stdout}"));
        let reason = stdout[at + row.len()..].lines().next().unwrap_or_default();
        assert!(reason.contains(message), "{stdout}");
    }
    assert!(
        stdout.contains("passed   executable  demo.zones::an_offset_the_zone_has"),
        "{stdout}"
    );
}

/// EXPR-3: "An index outside a list fails where it is evaluated". The
/// test is reported at the statement of it that was running, inside a
/// function it called, with the runtime library's text of the failure.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn a_failure_is_reported_at_its_statement() {
    if !sdk_named() {
        return;
    }
    let root = scratch("index");
    let file = write(
        &root,
        "src/demo.index.hgl",
        concat!(
            "module demo.index\n",
            "\n",
            "const fn first(values: list<i64>) -> i64 => values[0]\n",
            "\n",
            "test outside {\n",
            "    let none: list<i64> = []\n",
            "    assert first(none) == 0\n",
            "}\n",
        ),
    );
    let output = hgl_in(&["test", &file, "--target", "cpp"], &[], &[]);
    let stdout = text(&output.stdout);
    let rows: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        rows[0], "FAILED   executable  demo.index::outside",
        "{stdout}"
    );
    assert!(
        rows[1].starts_with(&format!("    {file}:7:5: "))
            && rows[1].contains("the index 0 is outside a"),
        "{stdout}"
    );
}

/// A module whose declarations name another's enum: its header declares
/// the enum opaque, and its unit includes the other's header, which
/// defines it (TYPE-7), as it does where a test alone names it.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn a_module_that_names_anothers_enum_is_built() {
    if !sdk_named() {
        return;
    }
    let root = scratch("enums");
    write(
        &root,
        "src/demo.left.hgl",
        concat!("module demo.left\n", "export enum Side { up, down }\n",),
    );
    write(
        &root,
        "src/demo.right.hgl",
        concat!(
            "module demo.right\n",
            "use demo.left::{Side}\n",
            "const fn flip(side: Side) -> Side => if side == .up { .down } else { .up }\n",
            "test it_finds_the_enum {\n",
            "    assert flip(.up) == .down\n",
            "}\n",
            "test a_test_alone_names_it {\n",
            "    let side = Side.up\n",
            "    assert side == Side.up\n",
            "}\n",
        ),
    );
    let source = argument(&root.join("src"));
    let output = hgl_in(&["test", &source, "--target", "cpp"], &[], &[]);
    let stdout = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{stdout}{}",
        text(&output.stderr)
    );
    assert!(
        stdout.contains("passed   executable  demo.right::it_finds_the_enum")
            && stdout.contains("passed   executable  demo.right::a_test_alone_names_it"),
        "{stdout}"
    );
}

/// An instance of a generic function is written with its declaration,
/// so it may name an enum of a module that imports the declaring one:
/// each of the two headers then names the other's enum.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn two_modules_whose_headers_name_each_others_enums_are_built() {
    if !sdk_named() {
        return;
    }
    let root = scratch("enum-cycle");
    write(
        &root,
        "src/demo.left.hgl",
        concat!(
            "module demo.left\n",
            "export enum Side { up, down }\n",
            "export const fn same<T>(value: T) -> T => value\n",
        ),
    );
    write(
        &root,
        "src/demo.right.hgl",
        concat!(
            "module demo.right\n",
            "use demo.left::{Side, same}\n",
            "export enum Mood { calm, keen }\n",
            "const fn flip(side: Side) -> Side => if side == .up { .down } else { .up }\n",
            "test each_names_the_other {\n",
            "    assert flip(.up) == .down\n",
            "    assert same(Mood.keen) == Mood.keen\n",
            "}\n",
        ),
    );
    let source = argument(&root.join("src"));
    let output = hgl_in(&["test", &source, "--target", "cpp"], &[], &[]);
    let stdout = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{stdout}{}",
        text(&output.stderr)
    );
    assert!(
        stdout.contains("passed   executable  demo.right::each_names_the_other"),
        "{stdout}"
    );
}

/// CTRL-6: "An unmatched selector uses default or fails": a `switch`
/// with no `default` whose selector no case has fails its test.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn an_unmatched_switch_fails() {
    if !sdk_named() {
        return;
    }
    let root = scratch("unmatched");
    let file = write(
        &root,
        "src/demo.unmatched.hgl",
        concat!(
            "module demo.unmatched\n",
            "const fn matched(x: i64) -> i64 {\n",
            "    var r = 0\n",
            "    switch x {\n",
            "        case 1:\n",
            "            r = 10\n",
            "    }\n",
            "    return r\n",
            "}\n",
            "test matched_one { assert matched(1) == 10 }\n",
            "test unmatched_two { assert matched(2) == 0 }\n",
        ),
    );
    let output = hgl_in(&["test", &file, "--target", "cpp"], &[], &[]);
    let stdout = text(&output.stdout);
    assert!(
        stdout.contains("passed   executable  demo.unmatched::matched_one")
            && stdout.contains("FAILED   executable  demo.unmatched::unmatched_two")
            && stdout.contains("no case of the `switch` has the selector's value"),
        "{stdout}"
    );
}

/// Plain names that a platform's compiler or headers take, a GNU keyword
/// and the macros of glibc's headers among them, are built as they are
/// written: units are strict C++23, and each file `#undef`s its names.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn names_a_platform_takes_are_built() {
    if !sdk_named() {
        return;
    }
    let root = scratch("platform-names");
    let file = write(
        &root,
        "src/demo.names.hgl",
        concat!(
            "module demo.names\n",
            "const fn issubnormal(typeof: i64) -> i64 => typeof\n",
            "const fn htobe16(alloca: i64, major: i64) -> i64 => alloca + major\n",
            "const fn linux(unix: i64) -> i64 => unix\n",
            "test each_is_its_own {\n",
            "    let timerclear = issubnormal(1)\n",
            "    assert htobe16(timerclear, 2) == 3 && linux(4) == 4\n",
            "}\n",
        ),
    );
    let output = hgl_in(&["test", &file, "--target", "cpp"], &[], &[]);
    let stdout = text(&output.stdout);
    assert!(
        stdout.contains("passed   executable  demo.names::each_is_its_own"),
        "{stdout}{}",
        text(&output.stderr)
    );
}

/// UNIT-14, RUN-1: an application runs on the SDK, and its output is
/// printed: a line for each publication of its entry's result, in time
/// order, its instant and then what was published (core's CRUN-8). The
/// run's store holds what the application put there before its run
/// (CRUN-7), and what a graph put there as it was built (FN-9). An entry
/// with no result has no output. A run that fails has none either
/// (CRUN-6), and says why: status 1. What was written for the run is kept
/// where `--keep` says, and what starts the application may replace an
/// argument of its entry (RUN-2).
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn an_application_runs_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("applications");
    write(&root, "ticks.hgl", TICKS);
    for (name, text) in [
        ("backtest.hgl", BACKTEST),
        ("stored.hgl", STORED),
        ("seeded.hgl", SEEDED),
        ("silent.hgl", SILENT),
        ("broken.hgl", BROKEN),
        ("later.hgl", LATER),
        ("both.hgl", BOTH),
        ("live.hgl", LIVE),
    ] {
        write(&root, name, text);
    }
    let kept = root.join("build");
    let run = |application: &str, more: &[&str]| {
        let mut arguments = vec![
            "run",
            root.to_str().unwrap(),
            "--target",
            "cpp",
            "--application",
            application,
            "--keep",
            kept.to_str().unwrap(),
        ];
        arguments.extend(more);
        hgl_in(&arguments, &[], &[])
    };
    let output = run("demo.ticks.backtest", &[]);
    assert_eq!(
        text(&output.stdout),
        concat!(
            "1970-01-01T00:00:01.000001Z  2\n",
            "1970-01-01T00:00:02.000001Z  4\n",
            "1970-01-01T00:00:03.000001Z  6\n",
            "ran demo.ticks.backtest on cpp: 3 publications\n",
        ),
        "{}",
        text(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(0));
    assert!(kept.join("applications/demo.ticks.backtest.cpp").is_file());
    assert!(kept.join("run/demo.ticks.backtest").is_file());

    // RUN-2: an argument replaced.
    let output = run("demo.ticks.backtest", &["--arg", "values=[5]"]);
    assert_eq!(
        text(&output.stdout),
        "1970-01-01T00:00:01.000001Z  10\nran demo.ticks.backtest on cpp: 1 publication\n",
        "{}",
        text(&output.stderr)
    );

    for application in ["demo.ticks.stored", "demo.ticks.seeded"] {
        let output = run(application, &[]);
        let step = if application == "demo.ticks.stored" {
            1
        } else {
            2
        };
        assert_eq!(
            text(&output.stdout),
            format!(
                "1970-01-01T00:00:0{}.000001Z  1\n1970-01-01T00:00:0{}.000001Z  2\n\
                 ran {application} on cpp: 2 publications\n",
                step,
                2 * step
            ),
            "{}",
            text(&output.stderr)
        );
        assert_eq!(output.status.code(), Some(0));
    }

    let output = run("demo.ticks.silent", &[]);
    assert_eq!(
        text(&output.stdout),
        "ran demo.ticks.silent on cpp: its entry has no result\n"
    );
    assert_eq!(output.status.code(), Some(0));

    for (application, code) in [
        ("demo.ticks.broken", "numeric.division_by_zero"),
        ("demo.ticks.both", "run.end_and_length"),
    ] {
        let output = run(application, &[]);
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(text(&output.stdout), "");
        let said = text(&output.stderr);
        let last = said.lines().last().unwrap_or("");
        assert!(
            last.starts_with(&format!("hgl: {application} failed: {code}: ")),
            "{said}"
        );
    }

    // CRUN-2: the run's own start, and an end that is exclusive; a field
    // replaced (RUN-2).
    let output = run("demo.ticks.later", &[]);
    assert_eq!(
        text(&output.stdout),
        concat!(
            "2026-01-02T00:00:01Z  2\n",
            "2026-01-02T00:00:02Z  4\n",
            "ran demo.ticks.later on cpp: 2 publications\n",
        ),
        "{}",
        text(&output.stderr)
    );
    let output = run("demo.ticks.later", &["--config", "length=2s"]);
    assert_eq!(
        text(&output.stdout),
        "2026-01-02T00:00:01Z  2\nran demo.ticks.later on cpp: 1 publication\n",
        "{}",
        text(&output.stderr)
    );

    // RUN-3, CRUN-5: in real time it ends at its end, counting from 1 at
    // the host clock's pace.
    let output = run("demo.ticks.live", &[]);
    let printed = text(&output.stdout);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let lines: Vec<&str> = printed.lines().collect();
    let (summary, published) = lines.split_last().expect("something is printed");
    assert!(!published.is_empty() && published.len() <= 5, "{printed}");
    for (position, line) in published.iter().enumerate() {
        assert!(line.ends_with(&format!("Z  {}", position + 1)), "{printed}");
    }
    assert!(
        summary.starts_with("ran demo.ticks.live on cpp: "),
        "{printed}"
    );
}

/// RUN-3: an interrupt of `hgl` alone, a signal its executable does not
/// get, stops a run in real time all the same: `hgl` passes it on, and
/// prints what the run recorded up to then. Needs the SDK, as above, and
/// a system with `kill`.
#[cfg(unix)]
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn an_interrupt_of_hgl_alone_stops_a_run_in_real_time() {
    if !sdk_named() {
        return;
    }
    let root = scratch("interrupted");
    write(&root, "ticks.hgl", TICKS);
    write(&root, "endless.hgl", ENDLESS);
    let child = Command::new(env!("CARGO_BIN_EXE_hgl"))
        .args(["run", &argument(&root), "--target", "cpp"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("the hgl binary runs");
    // Long enough for the program to be built and its run started.
    std::thread::sleep(std::time::Duration::from_millis(8000));
    // To `hgl` alone: its process, and not its group.
    let sent = Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .expect("`kill` runs");
    assert!(sent.success());
    let output = child.wait_with_output().expect("hgl ends");
    let printed = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{printed}{}",
        text(&output.stderr)
    );
    let lines: Vec<&str> = printed.lines().collect();
    let (summary, published) = lines.split_last().expect("something is printed");
    // It counted from 1 for as long as it ran: on a slow host that may be
    // no time at all, where the interrupt comes before the run starts.
    for (position, line) in published.iter().enumerate() {
        assert!(line.ends_with(&format!("Z  {}", position + 1)), "{printed}");
    }
    assert!(
        summary.starts_with("ran demo.ticks.endless on cpp until it was stopped: "),
        "{printed}"
    );
}

/// CAP-6 on the SDK: every ordinary value put in the run's store is got
/// back as itself, a struct, a tuple, a dictionary, a set and a delta among
/// them, in a node, where a graph is built and by an application. A failure
/// where a graph is built or in an application's store statements names the
/// HGL file and line it was raised at, which the generated code sets with
/// `#line`, and no place of the generated C++.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn store_values_and_failure_places_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("store_values");
    let module = write(
        &root,
        "demo.store.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.store.hgl"),
    );
    for (name, application) in [
        (
            "demo.store.quotes.hgl",
            include_str!("../../hgl_cpp/tests/fixtures/demo.store.quotes.hgl"),
        ),
        (
            "demo.store.failing.hgl",
            include_str!("../../hgl_cpp/tests/fixtures/demo.store.failing.hgl"),
        ),
        (
            "demo.store.unbuilt.hgl",
            include_str!("../../hgl_cpp/tests/fixtures/demo.store.unbuilt.hgl"),
        ),
    ] {
        write(&root, name, application);
    }
    let kept = root.join("build");
    let output = hgl_in(
        &[
            "test",
            &argument(&root),
            "--target",
            "cpp",
            "--keep",
            &argument(&kept),
        ],
        &[],
        &[],
    );
    let stdout = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{stdout}{}",
        text(&output.stderr)
    );
    assert!(
        stdout.ends_with("executable cases: 3 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
    let run = |application: &str| {
        hgl_in(
            &[
                "run",
                &argument(&root),
                "--target",
                "cpp",
                "--application",
                application,
                "--keep",
                &argument(&kept),
            ],
            &[],
            &[],
        )
    };
    let output = run("demo.store.quotes");
    assert_eq!(
        text(&output.stdout),
        concat!(
            "1970-01-01T00:00:01.000001Z  delta<demo.store::Quote>(bid: 11, ask: 12)\n",
            "1970-01-01T00:00:02.000001Z  delta<demo.store::Quote>(bid: 12, ask: 13)\n",
            "ran demo.store.quotes on cpp: 2 publications\n",
        ),
        "{}",
        text(&output.stderr)
    );
    // The failing `l[3]` is on line 7 of the application's file; the `get`
    // of a key put nowhere on line 80 of the module's, in the graph.
    let failing = root.join("demo.store.failing.hgl").display().to_string();
    for (application, said) in [
        (
            "demo.store.failing",
            format!(
                "hgl: demo.store.failing failed: value.index_out_of_range: the index 3 is \
                 outside a list of 1, raised at {failing}:7"
            ),
        ),
        (
            "demo.store.unbuilt",
            format!(
                "hgl: demo.store.unbuilt failed: the global store holds nothing under 'by', \
                 raised at {module}:80"
            ),
        ),
    ] {
        let output = run(application);
        assert_eq!(output.status.code(), Some(1));
        let stderr = text(&output.stderr);
        assert_eq!(stderr.lines().last(), Some(said.as_str()), "{stderr}");
    }
}

/// A failure names the HGL line of the statement it is raised in, though
/// the target writes more than one line of C++ for it: `xs[i] = ys[j]`
/// holds its index and checks its place first (EXPR-2), and its value
/// fails; and a block's last statement, after such a statement, fails in
/// turn. Each is raised in a value function an application's store
/// statement calls (UNIT-14).
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn failure_places_of_statements_written_on_several_lines_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("failure_places");
    write(
        &root,
        "demo.places.hgl",
        concat!(
            "module demo.places\n",
            "\n",
            "export fn ticks(const values: list<i64>) -> i64 {\n",
            "    for value in values {\n",
            "        yield 1s: value\n",
            "    }\n",
            "}\n",
        ),
    );
    let preludes = write(
        &root,
        "demo.places.preludes.hgl",
        concat!(
            "application demo.places.preludes\n",
            "\n",
            "use demo.places::{ticks}\n",
            "\n",
            "const fn moved(i: i64, j: i64) -> i64 {\n",
            "    var xs = [1, 2]\n",
            "    let ys = [3, 4]\n",
            "    xs[i] = ys[j]\n",
            "    return xs[0]\n",
            "}\n",
            "\n",
            "inject global_state\n",
            "put(global_state, \"moved\", moved(0, 5))\n",
            "\n",
            "eval(ticks(values: [1]))\n",
        ),
    );
    let block = write(
        &root,
        "demo.places.block.hgl",
        concat!(
            "application demo.places.block\n",
            "\n",
            "use demo.places::{ticks}\n",
            "\n",
            "const fn read_after(i: i64, j: i64) -> i64 {\n",
            "    var xs = [1, 2]\n",
            "    let ys = [3, 4]\n",
            "    if i >= 0 {\n",
            "        xs[i] = ys[j - 5]\n",
            "        return xs[j]\n",
            "    }\n",
            "    return 0\n",
            "}\n",
            "\n",
            "inject global_state\n",
            "put(global_state, \"read\", read_after(0, 5))\n",
            "\n",
            "eval(ticks(values: [1]))\n",
        ),
    );
    let kept = root.join("build");
    // The assignment is on line 8 of its file; the block's last statement
    // on line 10 of its.
    for (application, file, line) in [
        ("demo.places.preludes", &preludes, 8),
        ("demo.places.block", &block, 10),
    ] {
        let output = hgl_in(
            &[
                "run",
                &argument(&root),
                "--target",
                "cpp",
                "--application",
                application,
                "--keep",
                &argument(&kept),
            ],
            &[],
            &[],
        );
        assert_eq!(output.status.code(), Some(1), "{}", text(&output.stderr));
        let stderr = text(&output.stderr);
        let said = format!(
            "hgl: {application} failed: value.index_out_of_range: the index 5 is outside a list \
             of 2, raised at {file}:{line}"
        );
        assert_eq!(stderr.lines().last(), Some(said.as_str()), "{stderr}");
    }
}

/// A test's failing statement is still named in HGL after a node the test
/// writes ahead of itself, whose statements set their own HGL lines: an
/// eval's lifted call of a value function (TST-8).
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn a_tests_failure_is_named_after_a_node_written_ahead_of_it() {
    if !sdk_named() {
        return;
    }
    let root = scratch("ahead");
    let file = write(
        &root,
        "demo.ahead.hgl",
        concat!(
            "module demo.ahead\n",
            "const fn scale(value: i64) -> i64 => value * 2\n",
            "test a_failure_after_an_eval_of_a_value_function {\n",
            "    assert eval(scale(value: [1])) == [2]\n",
            "    assert scale(1) == 3\n",
            "}\n",
        ),
    );
    let output = hgl_in(&["test", &file, "--target", "cpp"], &[], &[]);
    let stdout = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(1),
        "{stdout}{}",
        text(&output.stderr)
    );
    assert!(
        stdout.contains(&format!("    {file}:5:5: the assertion does not hold\n")),
        "{stdout}"
    );
}
