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
/// does not write yet.
const FLAGS: &str = concat!(
    "module demo.flags\n",
    "\n",
    "const fn yes() -> bool => true\n",
    "\n",
    "fn pass(value: bool) -> bool {\n",
    "    when { return value }\n",
    "}\n",
    "\n",
    "test holds {\n",
    "    assert yes()\n",
    "}\n",
    "\n",
    "test evaluates {\n",
    "    assert eval(pass(value: [true])) == [true]\n",
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
             {file}:14:12: the cpp target does not write an eval yet\n\
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
/// target does not write, and runs nothing.
#[test]
fn run_says_what_an_application_lacks() {
    let root = scratch("run");
    let file = write(
        &root,
        "app.hgl",
        concat!(
            "application demo.app\n",
            "fn counted() -> i64 {\n",
            "    yield 1s: 1\n",
            "}\n",
            "eval(counted())\n",
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
        format!("{file}:5:1: the cpp target does not write an application's run statement yet\n")
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
        stdout.ends_with("executable cases: 13 passed, 0 failed, 0 not run\n"),
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
        stdout.ends_with("executable cases: 12 passed, 0 failed, 0 not run\n"),
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
