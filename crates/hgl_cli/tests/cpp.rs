//! Runs `hgl emit`, `hgl test` and `hgl run` with `--target cpp` the way
//! a user would.
//!
//! The tests that build and run C++ need hgraph's C++ SDK, which the
//! environment variables `HGL_HGRAPH_SDK` and `HGL_HGRAPH_DEPS` name.
//! They are ignored
//! unless asked for: `cargo test -p hgl_cli --test cpp -- --ignored`
//! runs them, as the CI job for C++ does. Run with no C++ SDK named,
//! they say so and have nothing to check.

#[path = "cpp/bool_keys.rs"]
mod bool_keys;
#[cfg(unix)]
#[path = "cpp/build_reuse.rs"]
mod build_reuse;
mod common;
#[path = "cpp/debug_tests.rs"]
mod debug_tests;
#[cfg(unix)]
#[path = "common/executable.rs"]
mod executable;
#[path = "cpp/formatting.rs"]
mod formatting;
#[path = "cpp/result_only.rs"]
mod result_only;
#[path = "cpp/string_append.rs"]
mod string_append;
#[path = "cpp/type_values.rs"]
mod type_values;

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
    "    assert eval pass(value: [[[true]]]) == [[[true]]]\n",
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
    "\n",
    "fn told(value: i64) -> bool {\n",
    "    inject global_state\n",
    "    when { return get<bool>(global_state, \"started\") }\n",
    "}\n",
    "\n",
    "export fn started(const values: list<i64>) -> bool {\n",
    "    return told(ticks(values))\n",
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
    "eval doubled(values: rounds)\n",
);

/// UNIT-14: an application that sets the run's store before its run,
/// calling a specialization of a named operator requirement (GEN-16).
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
    "eval bounded(values: [1, 2, 3, 4])\n",
);

/// FN-9: one whose entry is a graph that sets the store as it is built.
const SEEDED: &str = concat!(
    "application demo.ticks.seeded\n",
    "\n",
    "use demo.ticks::{seeded}\n",
    "\n",
    "eval seeded(limit: 2, values: [1, 2, 3])\n",
);

/// An entry with no result: the run has no output (CRUN-8).
const SILENT: &str = concat!(
    "application demo.ticks.silent\n",
    "\n",
    "use demo.ticks::{quiet}\n",
    "\n",
    "eval quiet(values: [1, 2])\n",
);

/// An application whose run fails.
const BROKEN: &str = concat!(
    "application demo.ticks.broken\n",
    "\n",
    "use demo.ticks::{failing}\n",
    "\n",
    "eval failing()\n",
);

/// CRUN-2: a simulation from a start of its own, for a length: its end
/// is exclusive.
const LATER: &str = concat!(
    "application demo.ticks.later\n",
    "\n",
    "use demo.ticks::{doubled}\n",
    "\n",
    "inject run_config{start: @2026-01-02T00:00:00Z, length: 3s}\n",
    "eval doubled(values: [1, 2, 3, 4])\n",
);

/// CRUN-2: a configuration with both an `end` and a `length`, refused
/// before the run's graph is built.
const BOTH: &str = concat!(
    "application demo.ticks.both\n",
    "\n",
    "use demo.ticks::{doubled}\n",
    "\n",
    "inject run_config{end: @2026-01-02T00:00:00Z, length: 3s}\n",
    "eval doubled(values: [1])\n",
);

/// RUN-6: an application gives its run's configuration defaults, and its
/// store reads the configuration as it is supplied: "a start left out is
/// null there".
const STARTED: &str = concat!(
    "application demo.ticks.started\n",
    "\n",
    "use demo.ticks::{started}\n",
    "\n",
    "inject run_config{length: 3s}\n",
    "inject global_state\n",
    "put(global_state, \"started\", run_config.start != null)\n",
    "\n",
    "eval started(values: [1, 2])\n",
);

/// RUN-3: `run`, in real time, here for a fifth of a second: it ends at
/// its end, with nothing asked of it.
const LIVE: &str = concat!(
    "application demo.ticks.live\n",
    "\n",
    "use demo.ticks::{counted}\n",
    "\n",
    "inject run_config{length: 220ms}\n",
    "run counted(every: 50ms)\n",
);

/// RUN-3: `run`, in real time, with no end: until its host stops it.
const ENDLESS: &str = concat!(
    "application demo.ticks.endless\n",
    "\n",
    "use demo.ticks::{counted}\n",
    "\n",
    "run counted(every: 20ms)\n",
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
             HGL_HGRAPH_SDK is not set and the executable has no bundled `../libexec/hgraph-sdk`: \
             set it to the install prefix of hgraph's C++ SDK, the directory that holds `lib/cmake/hgraph`\n\
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

/// A bundled SDK and its dependencies are found through a symlink to
/// the binary. An explicit prefix overrides the matching bundle path.
#[cfg(unix)]
#[test]
fn release_prefixes_follow_the_real_executable() {
    let root = scratch("release_prefixes");
    let file = write(&root, "flags.hgl", FLAGS);
    let bundle = root.join("bundle");
    let hgl = bundle.join("bin/hgl");
    let source = fs::File::open(env!("CARGO_BIN_EXE_hgl")).unwrap();
    let permissions = source.metadata().unwrap().permissions();
    executable::publish_executable(&hgl, source, permissions);
    let linked = root.join("hgl");
    std::os::unix::fs::symlink(&hgl, &linked).unwrap();
    let sdk = bundle.join("libexec/hgraph-sdk/lib/cmake/hgraph");
    fs::create_dir_all(&sdk).unwrap();
    fs::write(sdk.join("hgraphConfig.cmake"), "").unwrap();
    let deps = bundle.join("libexec/hgraph-deps");
    fs::create_dir_all(deps.join("include")).unwrap();
    fs::create_dir_all(deps.join("lib")).unwrap();

    let from_bundle = |sdk_override: Option<&Path>| {
        let mut hgl = Command::new(&linked);
        hgl.args(["test", &file, "--target", "cpp", "--test", "holds"])
            .env("CXX", "true")
            .env_remove("HGL_HGRAPH_SDK")
            .env_remove("HGL_HGRAPH_DEPS");
        if let Some(sdk_override) = sdk_override {
            hgl.env("HGL_HGRAPH_SDK", sdk_override);
        }
        hgl.output().expect("the linked hgl runs")
    };
    let found_both = text(&from_bundle(None).stdout);
    assert!(
        found_both.contains("neither GCC nor Apple clang"),
        "{found_both}"
    );
    let explicit = text(&from_bundle(Some(&root)).stdout);
    assert!(explicit.contains("HGL_HGRAPH_SDK names"), "{explicit}");
    let empty = text(&from_bundle(Some(Path::new(""))).stdout);
    assert!(empty.contains("HGL_HGRAPH_SDK is empty"), "{empty}");
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
        "modules/type-core.h",
        "modules/mod-core.h",
        "modules/mod-core.cpp",
        "modules/mod-demo-flags.h",
        "modules/mod-demo-flags.cpp",
        "tests/test-demo-flags.cpp",
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
/// target does not write, and runs nothing: here a handler selecting on
/// its own window result (TIME-7).
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
            "    when valid(out, reading) { return reading }\n",
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
    let output = hgl_in(&["run", &argument(&root), "--target", "cpp"], &[], &[]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        text(&output.stdout),
        "not run: the cpp target does not write all that demo.app needs yet\n"
    );
    assert_eq!(
        text(&output.stderr),
        format!(
            "{file}:4:5: the cpp target does not write a handler that selects on a node's own window result yet\n"
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
        "hgl: demo.ticks.backtest was not run: HGL_HGRAPH_SDK is not set and the executable has no bundled \
         `../libexec/hgraph-sdk`: set it to the install prefix of hgraph's C++ SDK, the directory that holds `lib/cmake/hgraph`\n"
    );
}

/// Whether the C++ SDK is named, and with it the tests that build. Where
/// the ignored tests are run with none named, a test that needs it says
/// so and checks nothing.
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

    let executable = kept
        .join("bin")
        .join(format!("test-demo-calc{}", std::env::consts::EXE_SUFFIX));
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
    let executable = kept
        .join("bin")
        .join(format!("test-demo-calc{}", std::env::consts::EXE_SUFFIX));
    assert!(executable.is_file());

    block(&kept.join("obj").join("modules").join("mod-demo-calc.o"));
    let output = hgl_in(&arguments, &[], &[]);
    let stdout = text(&output.stdout);
    assert_eq!(output.status.code(), Some(2), "{stdout}");
    assert!(!kept.join(".hgl-built.json").exists());
    let rows: Vec<&str> = stdout.lines().collect();
    assert_eq!(rows[0], "not run  executable  demo.calc::sums", "{stdout}");
    assert!(
        rows[1].contains("mod-demo-calc.o could not be removed"),
        "{stdout}"
    );
    assert!(
        stdout.ends_with("executable cases: 0 passed, 0 failed, 1 not run\n"),
        "{stdout}"
    );
}

/// Puts a directory in the place of the object at `path`, so the builder
/// cannot clear the unverified object and proceed with a rebuild.
fn block(path: &Path) {
    assert!(path.is_file(), "{} was built", path.display());
    fs::remove_file(path).unwrap();
    fs::create_dir(path).unwrap();
}

/// UNIT-13, EXPR-5: module statement-block initializers use their
/// checked cold scope and retain the constant's one stored value.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn block_constants_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("block_constants");
    let file = write(
        &root,
        "demo.block_constants.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.block_constants.hgl"),
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
        stdout.contains("rejection cases: 0 passed, 0 failed\n"),
        "{stdout}"
    );
    assert!(
        stdout.ends_with("executable cases: 3 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

/// TYPE-5, UNIT-7 and EXPR-5: omitted fields use their defining scope and
/// their applied type, including statement blocks, generic calls and lambdas.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn field_defaults_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("field_defaults");
    let file = write(
        &root,
        "demo.field_defaults.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.field_defaults.hgl"),
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

/// EXPR-5, TYPE-14, UNIT-8: a generic null default is written using
/// the selected nullable value parameter's concrete type. An omitted
/// temporal input supplies a connection with no publication.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn generic_null_defaults_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("generic_null_defaults");
    let file = write(
        &root,
        "demo.generic_null_defaults.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.generic_null_defaults.hgl"),
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
        stdout.contains("rejection cases: 0 passed, 0 failed\n"),
        "{stdout}"
    );
    assert!(
        stdout.ends_with("executable cases: 5 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

/// GEN-7, EXPR-5: literal arguments and their results use the selected
/// instance's type, in functions, node handlers and graphs.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn generic_instance_literals_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("generic_instance_literals");
    let file = write(
        &root,
        "demo.instance_literals.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.instance_literals.hgl"),
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
        stdout.contains("rejection cases: 6 passed, 0 failed\n"),
        "{stdout}"
    );
    assert!(
        stdout.ends_with("executable cases: 8 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

/// GEN-2, GEN-6 and UNIT-8: concrete operator calls read literals at
/// each candidate; generic-body calls retain main's existing behavior.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn direct_operator_literal_options_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("direct_operator_literal_options");
    let file = write(
        &root,
        "demo.operator_literal_options.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.operator_literal_options.hgl"),
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
        stdout.contains("rejection cases: 17 passed, 0 failed\n"),
        "{stdout}"
    );
    assert!(
        stdout.ends_with("executable cases: 6 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

/// TIME-5, END-3 and END-4: hook mutations reject at source checking,
/// and admitted checked deltas handle publication and silence.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn endpoint_hook_checks_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("endpoint_hook_checks");
    let file = write(
        &root,
        "demo.endpoint_hook_checks.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.endpoint_hook_checks.hgl"),
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
        stdout.contains("rejection cases: 5 passed, 0 failed\n"),
        "{stdout}"
    );
    assert!(
        stdout.ends_with("executable cases: 2 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
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

/// TYPE-14 and TYPE-15: graph comparisons with null observe published values.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn graph_null_comparisons_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("graph-null-comparison");
    let file = write(
        &root,
        "demo.graph_null_comparison.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.graph_null_comparison.hgl"),
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

/// TYPE-10: both growing-list spellings share representation and identity
/// in value code, node and graph connections, bundles and instances.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn explicit_unbounded_lists_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("explicit-unbounded");
    let file = write(
        &root,
        "src/demo.explicit_unbounded.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.explicit_unbounded.hgl"),
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

/// Forms made of other families' forms on the SDK (`demo.nested`): a list
/// of tuples, growing and fixed, a tuple of a list, a dictionary of tuples
/// and a struct with a list field, each relayed, published by a delta and
/// replayed, each part through its own form (TYPE-9); a child of a list of
/// tuples and a key's child of a dictionary of tuples as connections of a
/// graph (TIME-9, EXPR-7); a struct with a list field, and a list of them,
/// as a node's configuration (FN-1), traversed by a hook (CTRL-8),
/// and one in the run's store (CAP-6).
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

/// TIME-1: a guarded child of a fixed-list field reads its own endpoint
/// through nested fields or positions, including a fitted struct input.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn fixed_list_fields_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("fixed-list-field");
    let file = write(
        &root,
        "demo.fixed_list_field.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.fixed_list_field.hgl"),
    );
    let output = hgl_in(&["test", &file, "--target", "cpp"], &[], &[]);
    let stdout = text(&output.stdout);
    assert!(output.status.success(), "{stdout}{}", text(&output.stderr));
    assert!(
        stdout.ends_with("executable cases: 6 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

/// A dictionary's tuple entry reads into a result even if it is not all valid (ERR-14, TIME-1).
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn a_partial_dictionary_tuple_entry_reads_as_err_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("dictionary-entry-result");
    let file = write(
        &root,
        "src/demo.dictionary_entry_result.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.dictionary_entry_result.hgl"),
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
        stdout.ends_with("executable cases: 1 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

/// END-8 and ERR-14: hold the selected key, read a partial composite in
/// each form, and report the HGL place of a failing value read.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn endpoint_read_forms_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("endpoint-read-forms");
    let fixture = include_str!("../../hgl_cpp/tests/fixtures/demo.endpoint_read_forms.hgl");
    let file = write(&root, "src/demo.endpoint_read_forms.hgl", fixture);
    let output = hgl_in(&["test", &file, "--target", "cpp"], &[], &[]);
    let stdout = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(1),
        "{stdout}{}",
        text(&output.stderr)
    );
    assert!(
        stdout.ends_with("executable cases: 10 passed, 3 failed, 0 not run\n"),
        "{stdout}"
    );
    let line = fixture
        .lines()
        .position(|line| line.contains("# consuming read"))
        .expect("the read to which the error points")
        + 1;
    assert!(
        stdout.contains("FAILED   executable  demo.endpoint_read_forms::a_plain_child_failure_points_to_the_read\n")
            && stdout.contains("value.unset_read")
            && stdout.contains(&format!("raised at {file}:{line}")),
        "{stdout}"
    );
    let child_line = fixture
        .lines()
        .position(|line| line.contains("# consuming scalar child read"))
        .expect("the scalar read to which the error points")
        + 1;
    assert!(
        stdout.contains("FAILED   executable  demo.endpoint_read_forms::unset_child_diagnostic_names_the_child\n")
            && stdout.contains(&format!(
                "value.unset_read: the child read has no complete value, raised at {file}:{child_line}"
            )),
        "{stdout}"
    );
    assert!(
        stdout
            .split_once(
                "FAILED   executable  demo.endpoint_read_forms::generic_nullable_child_still_raises_if_unset\n"
            )
            .and_then(|(_, tail)| tail.lines().next())
            .is_some_and(|report| report.contains("value.unset_read: the child read has no complete value")),
        "{stdout}"
    );
}

/// EXPR-6 and ERR-12: nested scalar guards in a graph short-circuit
/// in order, and a guard still admits the nullable read it protects.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn nested_scalar_guards_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("nested-scalar-guards");
    let file = write(
        &root,
        "demo.nested_scalar_guards.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.nested_scalar_guards.hgl"),
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

/// ERR-3 and ERR-14: observe validity separately from the tag of a bound
/// result child, and use the matching success or error payload. ERR-12:
/// a present nullable child refines only the alias named in the guard.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn bound_result_children_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("bound-result-children");
    let file = write(
        &root,
        "demo.bound_result_children.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.bound_result_children.hgl"),
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

/// ERR-14 and GEN-7: a generic node's whole read takes the nullable form
/// of the actual dictionary or unsized list.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn generic_endpoint_read_forms_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("generic-endpoint-read-forms");
    let file = write(
        &root,
        "demo.generic_endpoint_read_forms.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.generic_endpoint_read_forms.hgl"),
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
        stdout.ends_with("executable cases: 3 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

/// The temporal code of scalars on the SDK: a graph with lifted calls, a
/// constant connection, nodes with state, handlers in order and
/// capabilities, a source, and the evals of them (`demo.temporal`).
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

/// TST-3, TST-5, CEVAL-1: an eval can replay a stored trace on the SDK.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn recorded_eval_traces_are_replayed_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("recorded-replay");
    let file = write(
        &root,
        "src/demo.recorded_replay.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.recorded_replay.hgl"),
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
        stdout.ends_with("executable cases: 3 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

/// GEN-17 and TST-5: a reflected input type determines its trace cells
/// and the replay that supplies a graph's port.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn reflected_eval_inputs_are_replayed_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("reflected-eval-inputs");
    let file = write(
        &root,
        "demo.reflected_eval_inputs.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.reflected_eval_inputs.hgl"),
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
        stdout.ends_with(
            "rejection cases: 1 passed, 0 failed\nexecutable cases: 2 passed, 0 failed, 0 not run\n"
        ),
        "{stdout}"
    );
}

/// `const` parameters of every kind of ordinary value on the SDK
/// (`demo.configurations`, FN-1): a set, a dictionary, a tuple, a struct
/// that holds a list, one that holds a set, one with an optional field,
/// nullables and lists of them, and a nullable tested
/// against null (ERR-12), given to sources, nodes, lifted calls and a
/// branch. It also checks that a native Float and an opaque tuple each
/// preserve both zero signs in separate nodes (NUM-2).
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
/// (TYPE-11, END-9). A changed branch starts a window afresh (CTRL-2).
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
    let own_reads = write(
        &root,
        "src/demo.own_window_reads.hgl",
        include_str!("fixtures/demo.own_window_reads.hgl"),
    );
    let output = hgl_in(
        &["test", &file, &withdrawal, &own_reads, "--target", "cpp"],
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
        stdout.ends_with("executable cases: 21 passed, 0 failed, 0 not run\n"),
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

/// A node designates a dictionary key across membership changes and can
/// designate an ordinary input on a timer wake (FN-16, END-10, TIME-9).
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn reference_designations_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("reference-designations");
    let file = write(
        &root,
        "src/demo.reference_designations.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.reference_designations.hgl"),
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

/// The timing of nodes and sources on the SDK (`demo.timing`): nodes woken
/// by a scheduler or an alarm, alone or with an input's tick, `scheduled()`
/// and `modified([])` read as values, a wake dropped and one that fails,
/// an alarm's wake kept through an input's tick before it and the earliest
/// asked for kept (CAP-4, CAP-5, TIME-3, TIME-8, TIME-9), and sources that
/// read their clock, keep a `cache` or a `state` and have `start` and
/// `stop` (SOURCE-1 to SOURCE-4), and runs whose configuration's entries
/// are held, made by a function or chosen by an `if` (RUN-6). Every test of
/// it passes.
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

/// TYPE-12, FN-19 and UNIT-8: parameters, configuration and pack
/// connections observe signal ticks. An omitted signal stays silent.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn signal_inputs_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("signals");
    let file = write(
        &root,
        "src/demo.signals.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.signals.hgl"),
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
        stdout.ends_with("executable cases: 5 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

/// TIME-8 and CAP-4: the first dropped-wake warning names the generated node.
/// Later drops are reported even when the scheduler is passed to a value
/// helper and the HGL node has no explicit stop hook.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn dropped_wake_counts_reach_the_run_log_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("wake-logs");
    let node = write(
        &root,
        "wakes.hgl",
        "module wakes\n\
         const fn ask(scheduler: Scheduler, at: datetime) { schedule(scheduler, at) }\n\
         export fn requests() -> bool {\n\
             inject scheduler, clock\n\
             start { schedule(scheduler, 0us) }\n\
             when scheduled() {\n\
                 ask(scheduler, clock.evaluation_time)\n\
                 ask(scheduler, clock.evaluation_time - 1us)\n\
                 return true\n\
             }\n\
         }\n",
    );
    let app = write(
        &root,
        "wakes.app.hgl",
        "application wakes.main\nuse wakes::{requests}\neval requests()\n",
    );
    let output = hgl_in(&["run", &node, &app, "--target", "cpp"], &[], &[]);
    let stdout = text(&output.stdout);
    let stderr = text(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "{stdout}{stderr}");
    assert!(stdout.contains(": 1 publication"), "{stdout}");
    assert_eq!(
        stderr.matches("Dropped wake requested").count(),
        1,
        "{stderr}"
    );
    assert!(
        stderr
            .lines()
            .any(|line| line.contains("Dropped wake requested by wakes::requests")),
        "{stderr}"
    );
    assert_eq!(
        stderr.matches("Additional dropped wakes totaled 1").count(),
        1,
        "{stderr}"
    );
}

/// SOURCE-2, GEN-7. A generic yield time uses its instance's duration
/// or datetime, including when the source is wired from another graph.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn generic_yield_times_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("generic-yield-time");
    let file = write(
        &root,
        "src/demo.generic_yield_time.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.generic_yield_time.hgl"),
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
        stdout.ends_with("executable cases: 1 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

/// TIME-7 and SOURCE-1: whole yields replace a list and delta yields
/// change it. An empty delta on a valid list wakes no reader.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn a_source_yields_whole_values_and_deltas_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("mixed-yields");
    let module = write(
        &root,
        "src/demo.mixed.hgl",
        r#"module demo.mixed

fn mixed(const step: duration) -> list<i64> {
    yield step: [1, 2]
    yield step: delta<list<i64>>(updated: [2: 3])
    yield step: delta<list<i64>>()
    yield step: [8]
    yield step: delta<list<i64>>(updated: [1: 9])
}

fn read(items: list<i64>) -> i64 {
    when {
        var digits = 0
        for item in items { digits = 10 * digits + item }
        return digits
    }
}

export fn observed(const step: duration) -> i64 => read(mixed(step))

test mixed_in_simulation {
    assert eval observed(step: 20ms) == [20ms: 12, 40ms: 123, 80ms: 8, 100ms: 89]
}
"#,
    );
    let simulated = hgl_in(&["test", &module, "--target", "cpp"], &[], &[]);
    let report = text(&simulated.stdout);
    assert_eq!(
        simulated.status.code(),
        Some(0),
        "{report}{}",
        text(&simulated.stderr)
    );
    assert!(
        report.ends_with("executable cases: 1 passed, 0 failed, 0 not run\n"),
        "{report}"
    );

    write(
        &root,
        "src/demo.mixed.live.hgl",
        "application demo.mixed.live\n\nuse demo.mixed::{observed}\n\n\
         inject run_config{length: 2s}\nrun observed(step: 20ms)\n",
    );
    let source = argument(&root.join("src"));
    let live = hgl_in(
        &["run", &source, "--application", "demo.mixed.live"],
        &[],
        &[],
    );
    assert_eq!(live.status.code(), Some(0), "{}", text(&live.stderr));
    let report = text(&live.stdout);
    let lines: Vec<_> = report.lines().collect();
    let (summary, publications) = lines.split_last().expect("the run reports its output");
    assert_eq!(
        *summary, "ran demo.mixed.live on cpp: 4 publications",
        "{report}"
    );
    let values: Vec<_> = publications
        .iter()
        .map(|line| line.split_once("Z  ").expect("a timed publication").1)
        .collect();
    assert_eq!(values, ["12", "123", "8", "89"], "{report}");
}

/// TST-17, RUN-6: an integration test injects `run_config`, whose braces'
/// defaults stand, since `hgl test` supplies nothing; its value code reads
/// it, as supplied, and each eval's fields it does not name are its.
const CONFIGURED: &str = concat!(
    "module demo.configured\n",
    "\n",
    "fn beats(const every: duration) -> i64 {\n",
    "    cache count: i64 = 0\n",
    "    while count < 5 {\n",
    "        count += 1\n",
    "        yield every: count\n",
    "    }\n",
    "}\n",
    "\n",
    "const fn ticks(seen: list<tuple<datetime, i64>>) -> i64 {\n",
    "    var count = 0\n",
    "    for tick in seen { count += 1 }\n",
    "    return count\n",
    "}\n",
    "\n",
    "integration_test configured {\n",
    "    inject run_config{length: 4s}\n",
    "    assert run_config.length == 4s\n",
    "    assert run_config.start == null\n",
    "    assert ticks(eval beats(every: 1s)) == 3\n",
    "    assert ticks(eval{length: 2s} beats(every: 1s)) == 1\n",
    "    let opening = @2026-01-05T09:30:00Z\n",
    "    let seen = eval{start: opening} beats(every: 1s)\n",
    "    assert seen == [1s: 1, 2s: 2, 3s: 3]\n",
    "}\n",
);

/// TST-1 and FN-4: a named test runs its own temporal and value
/// helpers, including a role pair and names reused in another test.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn named_test_helpers_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("named-test-helpers");
    let file = write(
        &root,
        "core_conformance.named_test_helpers.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/core_conformance.named_test_helpers.hgl"),
    );
    let output = hgl_in(
        &["test", &file, "--target", "cpp", "--integration"],
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
}

/// SDOC-9: examples run through the named-test C++ entry, and failures
/// identify the assertion in the original comment.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn runnable_documentation_examples_use_the_named_test_runner() {
    if !sdk_named() {
        return;
    }
    let root = scratch("runnable-documentation-examples");
    let source = include_str!("fixtures/doc_examples.hgl");
    let file = write(&root, "examples.hgl", source);
    let output = hgl_in(&["test", &file, "--target", "cpp"], &[], &[]);
    let listing = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(1),
        "{listing}{}",
        text(&output.stderr)
    );
    assert!(
        listing.contains("FAILED   executable  demo.documentation_examples::Box#example-1"),
        "{listing}"
    );
    let line = source
        .lines()
        .position(|line| line.contains("assert box.value"))
        .unwrap()
        + 1;
    assert!(listing.contains(&format!("{file}:{line}:")), "{listing}");
    assert!(
        listing.ends_with("executable cases: 8 passed, 1 failed, 0 not run\n"),
        "{listing}"
    );
}

/// TST-14 and SDOC-9: giving Core's source makes its tests executable.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn explicitly_supplied_core_examples_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("explicit-core-documentation-examples");
    let file = write(
        &root,
        "core.hgl",
        concat!(
            "/** Core.\n\nExamples:\n    ```hgl\n    assert true\n    ```\n*/\n",
            "module core\n",
            "test supplied { assert true }\n",
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
        stdout.contains("passed   executable  core::[module]#example-1"),
        "{stdout}"
    );
    assert!(
        stdout.contains("passed   executable  core::supplied"),
        "{stdout}"
    );
    assert!(
        stdout.ends_with("executable cases: 2 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

/// SDOC-6: an invalid documentation example cannot affect a production run,
/// including the second parse that applies its argument override.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn running_an_application_ignores_documentation_examples() {
    if !sdk_named() {
        return;
    }
    let root = scratch("application-with-documentation-example");
    let example = "/** Example.\n\nExamples:\n    ```hgl\n    assert 1\n    ```\n*/\n";
    let app = format!(
        "application demo.app\n\n{example}fn echo(const value: i64) -> i64 {{\n    yield 1s: value\n}}\n\neval echo(value: 2)\n"
    );
    let file = write(&root, "app.hgl", &app);
    let run = || {
        hgl_in(
            &["run", &file, "--target", "cpp", "--arg", "value=3"],
            &[],
            &[],
        )
    };
    let with_example = run();
    assert!(
        with_example.status.success(),
        "{}",
        text(&with_example.stderr)
    );
    assert!(text(&with_example.stdout).contains("  3\n"));
    write(
        &root,
        "app.hgl",
        &app.replace(example, &"\n".repeat(example.lines().count())),
    );
    let without_example = run();
    assert!(
        without_example.status.success(),
        "{}",
        text(&without_example.stderr)
    );
    assert_eq!(with_example.stdout, without_example.stdout);
}

/// TST-1 and GEN-16: private declarations belong to their own test, but
/// candidates reach sibling tests and the functions and operators they call.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn named_test_declarations_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("named-test-declarations");
    let file = write(
        &root,
        "local.hgl",
        include_str!("fixtures/test_local_declarations.hgl"),
    );
    let foreign = write(
        &root,
        "foreign.hgl",
        r#"module demo.foreign
use demo.test_local_declarations::{classify, relay, render_relay}
test keeps_production_candidate {
    assert classify(7) == 0
    assert relay(7) == 0
    assert render_relay("[{}]", 7) == "production"
}
"#,
    );
    let output = hgl_in(&["test", &file, &foreign, "--target", "cpp"], &[], &[]);
    let stdout = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{stdout}{}",
        text(&output.stderr)
    );
    assert!(
        stdout.ends_with("executable cases: 11 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );

    let directory = root.join("production");
    let build = hgl_in(
        &["build", &file, &foreign, "--out", &argument(&directory)],
        &[],
        &[],
    );
    assert!(build.status.success(), "{}", text(&build.stderr));
    let source = fs::read_to_string(directory.join("modules/mod-demo-test_local_declarations.cpp"))
        .expect("the production module source is written");
    assert!(!source.contains("test_candidates_"), "{source}");
}

/// TST-1: withholding candidates from the application does not skip
/// validation of either a candidate body or another test's assertion.
#[test]
fn production_commands_report_test_errors_in_modules_with_test_candidates() {
    let root = scratch("test-candidate-production-diagnostics");
    let module = write(
        &root,
        "owner.hgl",
        r#"module demo.diagnostics
export operator kind<T>(value: T) -> i64
impl const fn kind<T>(value: T) -> i64 => 0
export const fn via(value: i64) -> i64 => kind(value)
test declares {
    impl const fn kind(value: i64) -> i64 => value + true
    assert via(1) == 1
}
test rejects {
    assert via(2) == "two"
}
"#,
    );
    let app = write(
        &root,
        "app.hgl",
        r#"application demo.diagnostics.app
use demo.diagnostics::{via}
fn present(const values: list<i64>) -> i64 {
    for value in values { yield 1s: value }
}
eval present(values: [via(7)])
"#,
    );
    for command in ["check", "build", "run"] {
        let output = hgl_in(&[command, &module, &app], &[], &[]);
        let stderr = text(&output.stderr);
        assert_eq!(output.status.code(), Some(1), "{command}\n{stderr}");
        assert_eq!(stderr.matches("error[").count(), 2, "{command}\n{stderr}");
        assert!(stderr.contains("`+` takes no `i64` and `bool`"), "{stderr}");
        assert!(stderr.contains("`==` takes no `i64` and `str`"), "{stderr}");
    }
}

/// ERR-9 and TST-1: a report from a declaration survives a test-view
/// reread once, even when two tests in another module call its body.
#[test]
fn a_test_view_does_not_repeat_an_existing_declaration_error() {
    let root = scratch("test-candidate-diagnostic-reread");
    let provider = write(
        &root,
        "provider.hgl",
        r#"module demo.provider
export operator kind<T>(value: T) -> i64
impl const fn kind<T>(value: T) -> i64 => 0
export const fn broken(value: i64) -> i64 => value + true
"#,
    );
    let tests = write(
        &root,
        "tests.hgl",
        r#"module demo.caller
use demo.provider::{kind, broken}
test first {
    impl const fn kind(value: i64) -> i64 => 1
    assert broken(1) == 1
}
test second {
    assert broken(2) == 2
}
"#,
    );
    let output = hgl_in(&["check", &provider, &tests], &[], &[]);
    let stderr = text(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert_eq!(stderr.matches("error[").count(), 1, "{stderr}");
    assert!(stderr.contains("`+` takes no `i64` and `bool`"), "{stderr}");
}

/// GEN-6 and TST-1: conflicting test candidates report one option kind
/// error during test analysis, and an application still uses production.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn test_candidate_option_conflicts_do_not_affect_an_application() {
    if !sdk_named() {
        return;
    }
    let root = scratch("test-candidate-option-kind");
    let file = write(
        &root,
        "taste.hgl",
        include_str!("fixtures/test_candidate_option_kind.hgl"),
    );
    let checked = hgl_in(&["check", &file], &[], &[]);
    assert_eq!(checked.status.code(), Some(1));
    assert!(text(&checked.stderr).contains("error[operator/impl.option_kind]"));
    let tests = hgl_in(&["test", &file, "--target", "cpp"], &[], &[]);
    let outcomes = text(&tests.stdout);
    assert!(tests.status.success(), "{outcomes}{}", text(&tests.stderr));
    assert!(
        outcomes.contains("rejection cases: 1 passed, 0 failed"),
        "{outcomes}"
    );

    let app = write(
        &root,
        "taste.app.hgl",
        r#"application demo.taste.app
use demo.test_candidate_option_kind::{taste}
fn present(const values: list<i64>) -> i64 {
    for value in values { yield 1s: value }
}
eval present(values: [taste(7)])
"#,
    );
    let run = hgl_in(&["run", &file, &app, "--target", "cpp"], &[], &[]);
    let stdout = text(&run.stdout);
    assert!(run.status.success(), "{stdout}{}", text(&run.stderr));
    assert!(stdout.contains("  0\n"), "{stdout}");
    assert!(
        stdout.contains("ran demo.taste.app on cpp: 1 publication\n"),
        "{stdout}"
    );
}

/// TST-2: an application build omits test units, and a mixed build keeps
/// their code and candidate instances out of the application executable.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn building_tests_preserves_the_applications_production_link() {
    if !sdk_named() {
        return;
    }
    let root = scratch("mixed-test-and-application-link");
    write(
        &root,
        "src/a.hgl",
        r#"module p21.a
export operator kind<T>(value: T) -> i64
impl const fn kind<T>(value: T) -> i64 => 0
export const fn via(value: i64) -> i64 => kind(value)
test a_plain { assert via(7) == 0 }
"#,
    );
    write(
        &root,
        "src/b.hgl",
        r#"module p21.b
use p21.a::{kind, via}
test b_declares {
    impl const fn kind(value: i64) -> i64 => 2
    assert via(7) == 2
}
"#,
    );
    write(
        &root,
        "src/app.hgl",
        r#"application p21.app
use p21.a::{via}
fn present(const values: list<i64>) -> i64 {
    for value in values { yield 1s: value }
}
test app_plain {
    struct AppTestValue { value: i64 }
    const fn app_expected(value: AppTestValue) -> i64 => value.value
    assert app_expected(AppTestValue(value: via(7))) == 0
}
eval present(values: [via(7)])
"#,
    );
    let sources = argument(&root.join("src"));
    let workspace = argument(&root.join("mixed"));
    let build = |directory: &str, options: &[&str]| {
        let mut args = vec![
            "build",
            &sources,
            "--out",
            directory,
            "--format",
            "json",
            "--compile-commands",
        ];
        args.extend_from_slice(options);
        let built = hgl_in(&args, &[], &[]);
        assert!(built.status.success(), "{}", text(&built.stderr));
        serde_json::from_slice::<serde_json::Value>(&built.stdout).unwrap()
    };
    let manifest =
        |directory: &str| fs::read_to_string(Path::new(directory).join("build.ninja")).unwrap();
    let link = |manifest: &str, target: &str| {
        let prefix = format!("build {target}{}: link ", std::env::consts::EXE_SUFFIX);
        manifest
            .lines()
            .find_map(|line| line.strip_prefix(&prefix))
            .unwrap()
            .to_owned()
    };
    let production = argument(&root.join("production"));
    let production_units = serde_json::json!(["core", "p21.a", "p21.app"]);
    let built = build(&production, &[]);
    assert_eq!(built["units"], production_units);
    assert_eq!(built["tests"], serde_json::json!([]));
    let production_manifest = manifest(&production);
    assert!(
        !production_manifest.contains("test-mod-"),
        "{production_manifest}"
    );
    assert!(!root.join("production/modules/test-mod-p21-a.cpp").exists());
    assert!(
        !root
            .join("production/obj/modules/test-mod-p21-a.o")
            .exists()
    );
    assert!(!root.join("production/modules/type-p21-app.h").exists());
    assert!(
        !fs::read_to_string(root.join("production/modules/mod-p21-app.h"))
            .unwrap()
            .contains("AppTestValue")
    );

    let mixed = build(&workspace, &["--tests"]);
    let mixed_manifest = manifest(&workspace);
    let units = mixed["units"].as_array().unwrap();
    for unit in [
        "p21.a",
        "test:p21.a",
        "test:p21.b",
        "p21.app",
        "test:p21.app",
    ] {
        assert_eq!(
            units.iter().filter(|name| *name == unit).count(),
            1,
            "{units:?}"
        );
    }
    assert_eq!(mixed["tests"].as_array().unwrap().len(), 3);
    assert_eq!(
        link(&mixed_manifest, "run/app-p21-app"),
        link(&production_manifest, "run/app-p21-app")
    );
    let other_module_test = link(&mixed_manifest, "bin/test-p21-b");
    assert!(
        other_module_test.contains("obj/modules/test-mod-p21-a.o"),
        "{other_module_test}"
    );
    assert!(
        other_module_test.contains("obj/modules/test-mod-p21-b.o"),
        "{other_module_test}"
    );
    assert!(
        !other_module_test.contains("test-mod-p21-app.o"),
        "{other_module_test}"
    );
    let application_test = link(&mixed_manifest, "bin/test-p21-app");
    assert!(
        application_test.contains("obj/modules/mod-p21-app.o"),
        "{application_test}"
    );
    assert!(
        application_test.contains("obj/modules/test-mod-p21-app.o"),
        "{application_test}"
    );
    let module = fs::read_to_string(root.join("mixed/modules/mod-p21-a.cpp")).unwrap();
    assert!(!module.contains("test_candidates_"), "{module}");

    let runtime = argument(&root.join("runtime"));
    for directory in [&runtime, &workspace] {
        let app = hgl_in(
            &["run", &sources, "--target", "cpp", "--keep", directory],
            &[],
            &[],
        );
        let stdout = text(&app.stdout);
        assert!(app.status.success(), "{stdout}{}", text(&app.stderr));
        assert!(stdout.contains("  0\n"), "{stdout}");
        assert!(
            stdout.contains("ran p21.app on cpp: 1 publication"),
            "{stdout}"
        );
    }
    assert_eq!(manifest(&workspace), mixed_manifest);
    let runtime_manifest = manifest(&runtime);
    assert!(
        !runtime_manifest.contains("test-mod-"),
        "{runtime_manifest}"
    );
    assert!(!root.join("runtime/modules/test-mod-p21-a.cpp").exists());

    let tests = hgl_in(
        &["test", &sources, "--target", "cpp", "--keep", &workspace],
        &[],
        &[],
    );
    let stdout = text(&tests.stdout);
    assert!(tests.status.success(), "{stdout}{}", text(&tests.stderr));
    for test in ["p21.a::a_plain", "p21.b::b_declares", "p21.app::app_plain"] {
        assert!(
            stdout.contains(&format!("passed   executable  {test}\n")),
            "{stdout}"
        );
    }

    let rebuilt = build(&workspace, &[]);
    assert_eq!(rebuilt["units"], production_units);
    assert_eq!(rebuilt["tests"], serde_json::json!([]));
    let rebuilt_manifest = manifest(&workspace);
    assert!(
        !rebuilt_manifest.contains("test-mod-"),
        "{rebuilt_manifest}"
    );
    let compile_commands = fs::read_to_string(root.join("mixed/compile_commands.json")).unwrap();
    assert!(
        !compile_commands.contains("test-mod-"),
        "{compile_commands}"
    );
}

/// The integration test above passes on the SDK.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn an_integration_tests_run_configuration_runs_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("configured");
    let file = write(&root, "src/demo.configured.hgl", CONFIGURED);
    let output = hgl_in(
        &["test", &file, "--target", "cpp", "--integration"],
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
        stdout.ends_with("executable cases: 1 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

/// The empty interval keeps both hooks (CRUN-1), and invalid bounds fail with
/// `run.bounds` before either hook runs (CRUN-3).
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn zero_cycle_hooks_and_run_bounds_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("run-bounds-lifecycle");
    let file = write(
        &root,
        "lifecycle.hgl",
        r#"module probes.run_lifecycle
fn fail_start(const divisor: i64) {
    start { let checked = 1 // divisor }
    when {}
}
fn fail_stop(const divisor: i64) {
    when {}
    stop { let checked = 1 // divisor }
}
test zero_length_runs_start {
    assert raises("numeric.division_by_zero") { eval{length: 0us} fail_start(divisor: 0) }
}
test zero_length_runs_stop {
    assert raises("numeric.division_by_zero") { eval{length: 0us} fail_stop(divisor: 0) }
}
test lower_bound {
    assert raises("run.bounds") { eval{start: never, length: 1us} fail_start(divisor: 0) }
}
test upper_bound {
    assert raises("run.bounds") { eval{end: forever} fail_start(divisor: 0) }
}
"#,
    );
    let output = hgl_in(&["test", &file, "--target", "cpp"], &[], &[]);
    let stdout = text(&output.stdout);
    assert!(output.status.success(), "{stdout}{}", text(&output.stderr));
    assert!(
        stdout.ends_with("executable cases: 4 passed, 0 failed, 0 not run\n"),
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

/// GEN-4, GEN-5: each selected HGL candidate owns its C++ name, for
/// named and symbol calls of value and temporal candidates.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn operator_candidates_have_distinct_cpp_names_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("candidate_names");
    let file = write(
        &root,
        "demo.candidate_names.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.candidate_names.hgl"),
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

/// GEN-5 and GEN-7: value, node and graph instances choose their own
/// operator candidate, including across modules and through a symbol.
/// ERR-15: the choice uses the whole operands before implicit reads.
/// CTRL-13 and CTRL-15: graph conditions use the instance's categories.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn operator_candidates_are_selected_per_instance_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("instance-selection");
    let file = write(
        &root,
        "demo.instance_selection.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.instance_selection.hgl"),
    );
    let library = write(
        &root,
        "demo.instance_selection_library.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.instance_selection_library.hgl"),
    );
    let output = hgl_in(&["test", &file, &library, "--target", "cpp"], &[], &[]);
    let stdout = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{stdout}{}",
        text(&output.stderr)
    );
    assert!(
        stdout.ends_with("executable cases: 19 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

/// GEN-5, GEN-7 and ERR-15: a selected candidate's result takes the
/// form required by its place in value, node and graph instances.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn operator_instance_results_take_their_context_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("instance-result-context");
    let file = write(
        &root,
        "demo.instance_result_context.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.instance_result_context.hgl"),
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

/// GEN-5 and GEN-7: the result type selected by an instance governs
/// its uses in value functions, node handlers and graphs.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn operator_instance_result_shapes_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("instance-result-shapes");
    let file = write(
        &root,
        "demo.instance_result_shapes.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.instance_result_shapes.hgl"),
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

/// FN-8 and GEN-10: C++ calls bind the individual inputs of positional
/// and named packs in value helpers, nodes, and graphs. TST-5: eval
/// replays the trace of each captured argument.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn pack_parameters_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("pack_parameters");
    let file = write(
        &root,
        "demo.packs.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.packs.hgl"),
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
        stdout.ends_with("executable cases: 5 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

fn dependent_pack_program(root: &Path) -> (String, String) {
    let declarations = write(
        root,
        "demo.packdepends.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.packdepends.hgl"),
    );
    let tests = write(
        root,
        "demo.packdepends_tests.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.packdepends_tests.hgl"),
    );
    (declarations, tests)
}

/// FN-8 and GEN-17: the C++ instance accounts for both the static function
/// and the supplied argument shape, even where the body reads neither.
#[test]
fn dependent_pack_instances_distinguish_callable_and_shape() {
    let root = scratch("dependent-pack-instances");
    let (declarations, tests) = dependent_pack_program(&root);
    let out = root.join("out");
    let output = hgl_in(
        &[
            "emit",
            &declarations,
            &tests,
            "--target",
            "cpp",
            "--out",
            &argument(&out),
        ],
        &[],
        &[],
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}{}",
        text(&output.stdout),
        text(&output.stderr)
    );
    let written = fs::read_to_string(out.join("modules/mod-demo-packdepends.cpp")).unwrap();
    let instances: Vec<_> = written
        .lines()
        .filter(|line| {
            line.trim_start().starts_with("// call_shape<") && line.contains("the instance")
        })
        .collect();
    assert_eq!(instances.len(), 6, "{instances:?}");
}

/// FN-8 and GEN-17: bind the callback's arguments, exclusions and keys,
/// and check constraints using its reflected parameter types.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn dependent_call_packs_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("dependent-packs");
    let (declarations, tests) = dependent_pack_program(&root);
    let output = hgl_in(
        &["test", &declarations, &tests, "--target", "cpp"],
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
        stdout.ends_with("executable cases: 12 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

/// EXPR-9: a fixed list literal composes one connection for each entry.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn graph_list_literals_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("graph-list-literals");
    let file = write(
        &root,
        "demo.graph_list_literals.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.graph_list_literals.hgl"),
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

/// EXPR-17: graph comprehensions respect their category and capture
/// each pass's binding in a temporal conditional or switch.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn comprehension_phases_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("comprehension-phases");
    let file = write(
        &root,
        "demo.comprehension_phases.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.comprehension_phases.hgl"),
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
        stdout.contains("rejection cases: 2 passed, 0 failed")
            && stdout.ends_with("executable cases: 8 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

/// EXPR-16 and EXPR-17: range bounds, value construction and graph ports.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn ranges_and_list_comprehensions_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("ranges-and-comprehensions");
    let file = write(
        &root,
        "demo.ranges.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.ranges.hgl"),
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

/// FN-19 and CTRL-16: let-bound entries keep their keys and values when
/// read or spread. UNIT-8 supplies a default for an omitted graph input.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn let_bound_packs_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("let-bound-packs");
    let file = write(
        &root,
        "examples.pack_forms.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/examples.pack_forms.hgl"),
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
        stdout.ends_with(
            "rejection cases: 10 passed, 0 failed\nexecutable cases: 6 passed, 0 failed, 0 not run\n"
        ),
        "{stdout}"
    );
}

/// MAP-1, RED-1, RED-4 and RED-5: C++ reports the remaining native forms.
/// C++ writes a one-dictionary keyed map and growing ordered folds with
/// and without callback captures.
#[test]
fn native_mapping_forms_decline_cpp_emission() {
    let root = scratch("native-mapping-forms");
    let file = write(
        &root,
        "demo.native_mapping.hgl",
        r#"module demo.native_mapping

fn twice_for_map(item: i64) -> i64 => item * 2
fn sum_for_map(left: i64, right: i64) -> i64 => left + right
const fn fold_digit(accumulated: i64, member: i64) -> i64 => accumulated * 10 + member

export fn keyed_map(values: dict<str, i64>, selected: set<str>) -> dict<str, i64> =>
    map{keys: selected}(twice_for_map, values)
export fn joined_map(left: dict<str, i64>, right: dict<str, i64>) -> dict<str, i64> =>
    map(sum_for_map, left, right)
export fn growing_map(values: list<i64>) -> list<i64> => map(twice_for_map, values)
export fn growing_tree(values: list<i64>) -> i64 => reduce(add_, values, 0)
export fn ordered_growing(values: list<i64>) -> i64 => reduce(fold_digit, values, 0)
export fn ordered_captured(values: list<i64>, bias: i64) -> i64 =>
    reduce(fn(accumulated, member) => accumulated * 10 + member + bias, values, 0)
export fn set_tree(values: set<i64>) -> i64 => reduce(add_, values, 0)
export fn dictionary_tree(values: dict<str, i64>) -> i64 => reduce(add_, values, 0)
"#,
    );
    let checked = hgl_in(&["check", &file], &[], &[]);
    assert_eq!(checked.status.code(), Some(0), "{}", text(&checked.stderr));

    let out = root.join("emitted");
    let emitted = hgl_in(
        &["emit", &file, "--target", "cpp", "--out", &argument(&out)],
        &[],
        &[],
    );
    let stderr = text(&emitted.stderr);
    assert_eq!(emitted.status.code(), Some(1), "{stderr}");
    assert!(
        !stderr.contains("reduce(fn(accumulated, member)"),
        "{stderr}"
    );
    assert!(
        !stderr.contains("reduce(fold_digit, values, 0)"),
        "{stderr}"
    );
    assert!(!stderr.contains("map{keys: selected}"), "{stderr}");
    assert!(stderr.contains("map(sum_for_map, left, right)"), "{stderr}");
    assert_eq!(
        stderr.matches("error[build/target.unsupported]").count(),
        5,
        "{stderr}"
    );
}

/// MAP-1 and MAP-2: report the checked keyed forms the target declines.
#[test]
fn keyed_map_declines_name_the_unsupported_form() {
    let root = scratch("keyed-map-declines");
    let file = write(
        &root,
        "demo.keyed_map_declines.hgl",
        r#"module demo.keyed_map_declines

const fn twice(x: i64) -> i64 => x * 2
fn applied(member: i64, f: fn(i64) -> i64) -> i64 => f(member)
fn sum_members(left: i64, right: i64) -> i64 => left + right

export fn mapped_function(items: dict<i64, i64>) -> dict<i64, i64> => map(applied, items, twice)
export fn mapped_local(items: dict<i64, i64>) -> dict<i64, i64> {
    let r = map(applied, items, twice)
    return r
}
export fn mapped_keys(items: dict<i64, i64>) -> set<i64> => keys(map(applied, items, twice))
export fn fixed_local(items: list<i64, 3>) -> list<i64, 3> {
    let r = map(applied, items, twice)
    return r
}
export fn mapped_pair(a: dict<i64, i64>, b: dict<i64, i64>) -> dict<i64, i64> => map(sum_members, a, b)
"#,
    );
    let checked = hgl_in(&["check", &file], &[], &[]);
    assert!(checked.status.success(), "{}", text(&checked.stderr));
    let out = root.join("emitted");
    let emitted = hgl_in(
        &["emit", &file, "--target", "cpp", "--out", &argument(&out)],
        &[],
        &[],
    );
    let stderr = text(&emitted.stderr);
    assert_eq!(emitted.status.code(), Some(1), "{stderr}");
    for reason in [
        "a keyed map passing a function argument to its child",
        "a keyed map with multiple split dictionaries",
    ] {
        assert!(stderr.contains(reason), "{stderr}");
    }
    assert_eq!(
        stderr
            .matches("a keyed map passing a function argument to its child")
            .count(),
        3,
        "{stderr}"
    );
    assert_eq!(
        stderr.matches("error[build/target.unsupported]").count(),
        7,
        "{stderr}"
    );
}

/// RED-5 and TYPE-14: a plain seed widens to the selected nullable accumulator.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn ordered_reduction_widens_the_seed_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("ordered-nullable-accumulator");
    let file = write(
        &root,
        "demo.ordered_nullable_accumulator.hgl",
        r#"module demo.ordered_nullable_accumulator

const fn combine(accumulated: i64?, member: i64?) -> i64? {
    if accumulated == null { return member }
    if member == null { return accumulated }
    return accumulated + member
}

fn fold(values: list<i64?>, seed: i64) -> i64? =>
    reduce<i64?, i64?, list<i64?>>(combine, values, seed)

test nullable_accumulator_takes_a_plain_seed {
    assert eval fold(values: [[2]], seed: [1]) == [delta<i64?>(value: 3)]
}
"#,
    );
    let tested = hgl_in(&["test", &file, "--target", "cpp"], &[], &[]);
    let stdout = text(&tested.stdout);
    assert_eq!(
        tested.status.code(),
        Some(0),
        "{stdout}{}",
        text(&tested.stderr)
    );
    assert!(
        stdout.ends_with("executable cases: 1 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

/// RED-5 and FN-14: ordered folds retain captured values and observe live
/// captures, including when a member's callback stops and starts again.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn ordered_reduction_captures_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("ordered-reduction-captures");
    let file = write(
        &root,
        "demo.ordered_reduce_captures.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.ordered_reduce_captures.hgl"),
    );
    let output = hgl_in(&["test", &file, "--target", "cpp"], &[], &[]);
    let stdout = text(&output.stdout);
    assert!(output.status.success(), "{stdout}{}", text(&output.stderr));
    assert!(
        stdout.ends_with("executable cases: 6 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

/// GEN-7, GEN-13 and TYPE-10: an inferred list size is an integer
/// constant in value code, a node handler and graph composition.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn inferred_list_sizes_are_values_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("list-size-values");
    let file = write(
        &root,
        "demo.list_size_values.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.list_size_values.hgl"),
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

/// TYPE-10: an unsized value parameter takes the entries of a fixed list.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn fixed_lists_supply_unsized_value_parameters_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("fixed-list-arguments");
    let file = write(
        &root,
        "demo.fixed_list_arguments.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.fixed_list_arguments.hgl"),
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
        stdout.ends_with("executable cases: 5 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

/// FN-19: value, operator and callback calls forward a pack; graph
/// calls forward the individual connections. Constructors, defaults and
/// lexical captures retain the concrete shape (FN-8, FN-14, TYPE-5).
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn forwarded_packs_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("forwarded-packs");
    let file = write(
        &root,
        "demo.forwarded_packs.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.forwarded_packs.hgl"),
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
        stdout.ends_with("executable cases: 19 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

/// FN-19 and GEN-17: a constant key reads a fitted entry, including a
/// captured value or a connection. An absent key rejects under the shape
/// supplied at the call. TIME-1, TIME-4 and TIME-9 use that shape for a
/// selected child endpoint and its admission.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn pack_entries_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("pack-entries");
    let file = write(
        &root,
        "demo.pack_reads.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.pack_reads.hgl"),
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
        stdout.ends_with(
            "rejection cases: 8 passed, 0 failed\nexecutable cases: 12 passed, 0 failed, 0 not run\n"
        ),
        "{stdout}"
    );
}

/// TIME-9 and GEN-7: delta and retained time reads observe the fitted
/// pack child in the same way as the child of an ordinary input.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn pack_child_observations_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("pack-child-operations");
    let file = write(
        &root,
        "demo.pack_child_operations.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.pack_child_operations.hgl"),
    );
    let output = hgl_in(&["test", &file, "--target", "cpp"], &[], &[]);
    let stdout = text(&output.stdout);
    assert!(output.status.success(), "{stdout}{}", text(&output.stderr));
    assert!(
        stdout.ends_with("executable cases: 2 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

/// FN-19, TIME-1, TIME-4 and TIME-9: guards and subscriptions use the
/// selected temporal input after pack expansion.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn packed_input_guards_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("pack-endpoint-guards");
    let file = write(
        &root,
        "demo.pack_endpoint_guards.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.pack_endpoint_guards.hgl"),
    );
    let output = hgl_in(&["test", &file, "--target", "cpp"], &[], &[]);
    let stdout = text(&output.stdout);
    assert!(output.status.success(), "{stdout}{}", text(&output.stderr));
    assert!(
        stdout.ends_with("executable cases: 4 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

/// FN-19: a range pass supplies a constant key (CTRL-16), and UNIT-13
/// lets a module constant be computed by a call. Both checks succeed
/// even when C++ cannot yet write the lookup.
#[test]
fn pack_entries_with_unread_constant_keys_decline_cpp_emission() {
    let root = scratch("unread-pack-keys");
    let cases = [
        (
            "range_key",
            concat!(
                "fn summed<...As>(args: ...As) -> i64 {\n",
                "    var s = args[0]\n",
                "    for i in 1..<3 { s = s + args[i] }\n",
                "    return s\n",
                "}\n",
                "test example { assert eval summed([1], [10], [100]) == [111] }\n",
            ),
        ),
        (
            "range_expression_key",
            concat!(
                "fn summed(args: ...i64) -> i64 {\n",
                "    var s = args[0]\n",
                "    for i in 0..<2 { s = s + args[i + 1] }\n",
                "    return s\n",
                "}\n",
                "test example { assert eval summed([1], [10], [100]) == [111] }\n",
            ),
        ),
        (
            "module_key",
            concat!(
                "const fn name() -> str => \"a\"\n",
                "const key: str = name()\n",
                "const fn fetch<...As>(args: ...{As}) -> i64 => args[key + \"b\"]\n",
                "test example { assert fetch(ab: 1) == 1 }\n",
            ),
        ),
        (
            "generic_key",
            concat!(
                "const fn name() -> str => \"a\"\n",
                "const key: str = name()\n",
                "const fn fetch<const given: str, ...As>(args: ...{As}) -> i64 => args[given]\n",
                "test example { assert fetch<key>(a: 1) == 1 }\n",
            ),
        ),
    ];
    for (name, body) in cases {
        let file = write(
            &root,
            &format!("{name}.hgl"),
            &format!("module {name}\n{body}"),
        );
        let checked = hgl_in(&["check", &file], &[], &[]);
        assert_eq!(
            checked.status.code(),
            Some(0),
            "{name}: {}",
            text(&checked.stderr)
        );
        let out = root.join(name);
        let emitted = hgl_in(
            &["emit", &file, "--target", "cpp", "--out", &argument(&out)],
            &[],
            &[],
        );
        let stderr = text(&emitted.stderr);
        assert_eq!(emitted.status.code(), Some(1), "{name}: {stderr}");
        assert!(
            stderr.contains("error[build/target.unsupported]"),
            "{name}: {stderr}"
        );
    }
}

/// FN-17: named, operator and anonymous targets specialize value and
/// temporal calls. Captured values and connections reach their bodies
/// through a node hook, a graph and another static function parameter.
/// MAP-2: map fits its split inputs to an explicit operator's member types.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn function_parameters_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("function_parameters");
    let file = write(
        &root,
        "demo.function_arguments.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.function_arguments.hgl"),
    );
    let library = write(
        &root,
        "demo.callback_library.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.callback_library.hgl"),
    );
    let output = hgl_in(&["test", &file, &library, "--target", "cpp"], &[], &[]);
    let stdout = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{stdout}{}",
        text(&output.stderr)
    );
    assert!(
        stdout.ends_with("executable cases: 5 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

/// FN-17: a finite chain of static wrappers and a recursive call that
/// forwards the same target both specialize and run.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn bounded_callable_specializations_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("bounded_callable_specializations");
    let file = write(
        &root,
        "demo.callable_specialization.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.callable_specialization.hgl"),
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

/// FN-17 and GEN-6: the selected candidate supplies the callback type and
/// kind. GEN-18 still reads the law of the selected static argument.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn candidate_function_arguments_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("candidate-function-arguments");
    let file = write(
        &root,
        "demo.candidate_function_arguments.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.candidate_function_arguments.hgl"),
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

/// FN-17, FN-18 and GEN-17: reflected callbacks carry their generic
/// arguments and defaults to the C++ target.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn reflected_pack_followups_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("reflected-pack-followups");
    let file = write(
        &root,
        "demo.reflected_pack_followups.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.reflected_pack_followups.hgl"),
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
        stdout.ends_with("executable cases: 6 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

/// FN-19, CTRL-16 and GEN-7: computed passes give only the entries that
/// exist, including when a callee supplies its own default.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn computed_pack_passes_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("computed-pack-passes");
    let file = write(
        &root,
        "demo.computed_pack_passes.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.computed_pack_passes.hgl"),
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

/// GEN-8 and GEN-17: reflected types specialize function and operator calls
/// in value and graph bodies and derive evidence from stated requirements (GEN-7).
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn written_pack_member_types_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("written-pack-member-types");
    let file = write(
        &root,
        "demo.written_pack_types.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.written_pack_types.hgl"),
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
        stdout.ends_with(
            "rejection cases: 6 passed, 0 failed\nexecutable cases: 8 passed, 0 failed, 0 not run\n"
        ),
        "{stdout}"
    );
}

/// GEN-10 and GEN-17: an existential or cross-pack relation can leave a
/// member's type untold. Analysis preserves that judgment and C++ reports the call.
#[test]
fn an_unproved_reflected_type_declines_cpp_emission() {
    let root = scratch("unproved-reflected-type");
    let cases = [
        (
            "existential",
            concat!(
                "const fn shape(item: str, other: i64) -> i64 => other\n",
                "const fn choose<...Ps>(f: const fn(...{Ps}) -> i64, item: Ps[key]) -> Ps[key]\n",
                "    requires any arg in arg_keys(Ps) { Ps[arg] == i64 } => exact<Ps[key]>(item)\n",
                "test chosen_member { assert choose(shape, \"text\") == \"text\" }\n",
            ),
        ),
        (
            "cross_pack",
            concat!(
                "const fn shape(item: i64) -> i64 => item\n",
                "const fn choose<...Ps, ...Qs>(f: const fn(...{Ps}) -> i64, g: const fn(...{Qs}) -> i64,\n",
                "    item: Ps[key]) -> Ps[key]\n",
                "    requires each arg in arg_keys(Ps) { Ps[arg] == Qs[arg] } => exact<Ps[key]>(item)\n",
                "test chosen_member { assert choose(shape, shape, 3) == 3 }\n",
            ),
        ),
    ];
    for (name, body) in cases {
        let file = write(
            &root,
            &format!("{name}.hgl"),
            &format!(
                "module {name}\nconst key: str = \"item\"\n\
                 const fn exact<T>(value: T) -> T requires T == i64 => value\n{body}"
            ),
        );
        let checked = hgl_in(&["check", &file], &[], &[]);
        assert_eq!(
            checked.status.code(),
            Some(0),
            "{name}: {}",
            text(&checked.stderr)
        );
        let out = root.join(name);
        let emitted = hgl_in(
            &["emit", &file, "--target", "cpp", "--out", &argument(&out)],
            &[],
            &[],
        );
        let stderr = text(&emitted.stderr);
        assert_eq!(emitted.status.code(), Some(1), "{name}: {stderr}");
        assert!(
            stderr.contains("error[build/target.unsupported]"),
            "{name}: {stderr}"
        );
    }
}

/// GEN-5 and GEN-7: an unread member proof preserves the ordinary
/// call's operator diagnostic at the declaration.
#[test]
fn an_unread_reflected_membership_preserves_the_operator_error() {
    let root = scratch("unread-reflected-membership");
    let file = write(
        &root,
        "m.hgl",
        concat!(
            "module m\n",
            "const item_key: str = \"item\"\n",
            "const fn words(item: str) -> i64 => 0\n",
            "const fn twice<T>(x: T) -> T requires T in {i64, f64} => x + x\n",
            "const fn call<...Ps>(f: const fn(...{Ps}) -> i64, a: Ps[item_key]) -> Ps[item_key]\n",
            "    requires Ps[item_key] in {i64, str} => twice<Ps[item_key]>(a)\n",
            "test t { assert call(words, \"s\") == \"ss\" }\n",
        ),
    );
    let checked = hgl_in(&["check", &file], &[], &[]);
    let stderr = text(&checked.stderr);
    assert_eq!(checked.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains("error[operator/operator.no_candidate]")
            && stderr.contains("no value candidate of operator `add_` takes `Ps[item_key]`"),
        "{stderr}"
    );
}

/// FN-19 and GEN-17: a computed spread emits its selected entries.
/// A function inside an entry can capture its iteration key (FN-14).
#[test]
fn a_computed_pack_spread_emits_selected_key_captures() {
    let root = scratch("computed-pack-spread");
    let file = write(
        &root,
        "m.hgl",
        concat!(
            "module m\n",
            "const fn identity(x: i64) -> i64 => x\n",
            "const fn named(unused: i64) -> i64 => unused\n",
            "const fn apply(f: const fn() -> i64) -> i64 => f()\n",
            "const fn forward<...Ps, ...As>(f: const fn(...{Ps}) -> i64, args: ...{As for f}) -> i64 =>\n",
            "    f(...{for key in arg_keys(Ps):\n",
            "        if key == \"unused\" { apply(fn() => if key == \"unused\" { 0 } else { 1 }) }\n",
            "        else if key in arg_keys(As) { args[key] }})\n",
            "test example {\n",
            "    assert forward(identity, 3) == 3\n",
            "    assert forward(named, unused: 3) == 0\n",
            "}\n",
        ),
    );
    let checked = hgl_in(&["check", &file], &[], &[]);
    assert_eq!(checked.status.code(), Some(0), "{}", text(&checked.stderr));
    let out = root.join("out");
    let emitted = hgl_in(
        &["emit", &file, "--target", "cpp", "--out", &argument(&out)],
        &[],
        &[],
    );
    assert_eq!(emitted.status.code(), Some(0), "{}", text(&emitted.stderr));
}

/// GEN-17 and FN-19: the keys in a computed spread are those of a type pack.
#[test]
fn a_computed_pack_spread_reports_a_value_pack_used_as_a_type_pack() {
    let root = scratch("computed-spread-value-pack");
    let file = write(
        &root,
        "m.hgl",
        concat!(
            "module m\n",
            "const fn pair(a: i64, b: i64) -> i64 => a + b\n",
            "const fn forward(items: ...i64{2}) -> i64 =>\n",
            "    pair(...{for key in arg_keys(items): items[key]})\n",
        ),
    );
    let checked = hgl_in(&["check", &file], &[], &[]);
    let stderr = text(&checked.stderr);
    assert_eq!(checked.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains("error[type/type.mismatch]")
            && stderr.contains("`arg_keys` needs a type pack, and `items` is not one"),
        "{stderr}"
    );
}

/// GEN-17 and FN-19: a written member type in a deferred entry keeps its
/// type reading in calls, nested types and binding or parameter annotations.
#[test]
fn a_computed_pack_spread_preserves_written_pack_member_types() {
    let root = scratch("computed-pack-entry-types");
    let file = write(
        &root,
        "m.hgl",
        concat!(
            "module m\n",
            "const fn exact<T>(value: T) -> T => value\n",
            "const fn apply<T>(f: const fn(T) -> T, value: T) -> T => f(value)\n",
            "operator pick_value<T>(value: T) -> T\n",
            "impl const fn pick_value<T>(value: T) -> T => value\n",
            "const fn forward<...Ps, ...As>(f: const fn(...{Ps}) -> i64, args: ...{As for f}) -> i64 =>\n",
            "    f(...{for key in arg_keys(Ps): {\n",
            "        let value: Ps[key] = args[key]\n",
            "        let values: list<Ps[key]> = [value]\n",
            "        let picked = pick_value<Ps[key]>(value)\n",
            "        apply<Ps[key]>(fn(item: Ps[key]) => item, exact<Ps[key]>(picked))\n",
            "    }})\n",
        ),
    );
    let checked = hgl_in(&["check", &file], &[], &[]);
    assert_eq!(checked.status.code(), Some(0), "{}", text(&checked.stderr));
}

/// CTRL-16 and FN-19: a constant range's pass gives the computed entry
/// a compile-time name in both a graph statement and a list comprehension.
#[test]
fn a_computed_pack_spread_can_read_a_graph_range_binding_in_a_guard() {
    let root = scratch("computed-pack-graph-range-guard");
    let file = write(
        &root,
        "m.hgl",
        concat!(
            "module m\n",
            "fn looped<...Ps, ...As>(f: const fn(...{Ps}) -> f64, args: ...{As for f}) -> f64 {\n",
            "    var out = 0.0\n",
            "    for i in 0..<2 {\n",
            "        out = f(...{for key in arg_keys(Ps): if i == 0 { args[key] } else { 1.0 }})\n",
            "    }\n",
            "    return out\n",
            "}\n",
            "fn nested<...Ps, ...As>(f: const fn(...{Ps}) -> f64, args: ...{As for f}) -> f64 {\n",
            "    var out = 0.0\n",
            "    for i in 0..<2 {\n",
            "        for j in 0..<(i + 1) {\n",
            "            out = f(...{for key in arg_keys(Ps): if j == 0 { args[key] } else { 1.0 }})\n",
            "        }\n",
            "    }\n",
            "    return out\n",
            "}\n",
            "fn listed<...Ps, ...As>(f: const fn(...{Ps}) -> f64, args: ...{As for f}) -> list<f64, 2> =>\n",
            "    [for i in 0..<2: f(...{for key in arg_keys(Ps): if i == 0 { args[key] } else { 1.0 }})]\n",
        ),
    );
    let checked = hgl_in(&["check", &file], &[], &[]);
    assert_eq!(checked.status.code(), Some(0), "{}", text(&checked.stderr));
}

/// FN-17, FN-19 and GEN-17: an entry recognizes the same direct
/// callback operands and written member types as an ordinary expression.
#[test]
fn computed_pack_entries_preserve_call_operands_and_type_places() {
    let cases = [
        ("positional_callback", "apply(1, cb)", true),
        ("named_callback", "apply(x: 1, cb: cb)", true),
        ("braced_callback", "apply{cb: cb}(1)", true),
        ("callback_callee", "(cb)(1)", true),
        ("callback_in_list", "{ let stored = [cb]\n 1 }", false),
        (
            "callback_called_in_lambda",
            "apply(1, fn(v) => cb(v))",
            true,
        ),
        ("callback_read_in_lambda", "apply(1, fn(v) => cb)", false),
        ("callback_forwarded_twice", "relay{cb: cb}(1)", true),
        (
            "member_annotation",
            "{ let value: Ps[key] = args[key]\n 1 }",
            true,
        ),
        (
            "member_argument",
            "{ let value = exact<Ps[key]>(args[key])\n 1 }",
            true,
        ),
        (
            "nested_member_argument",
            "{ let values = exact<list<Ps[key]>>([])\n 1 }",
            true,
        ),
        (
            "lambda_member_annotation",
            "apply_typed<Ps[key]>(args[key], fn(v: Ps[key]) => 1)",
            true,
        ),
    ];
    for (name, expression, valid) in cases {
        for in_entry in [false, true] {
            let root = scratch(&format!("computed-pack-{name}-{in_entry}"));
            let (generics, expression) = if in_entry {
                (
                    "...Ps, ...As",
                    format!("shape(...{{for key in arg_keys(Ps): {expression}}})"),
                )
            } else {
                ("...Ps, ...As, const key: str", expression.to_string())
            };
            let file = write(
                &root,
                "m.hgl",
                &format!(
                    "module m\n\
                     const fn apply(x: i64, cb: const fn(i64) -> i64) -> i64 => cb(x)\n\
                     const fn relay(x: i64, cb: const fn(i64) -> i64) -> i64 => apply{{cb: cb}}(x)\n\
                     const fn exact<T>(value: T) -> T => value\n\
                     const fn apply_typed<T>(value: T, cb: const fn(T) -> i64) -> i64 => cb(value)\n\
                     const fn compare<{generics}>(cb: const fn(i64) -> i64,\n\
                         shape: const fn(...{{Ps}}) -> i64, args: ...{{As for shape}}) -> i64 => {expression}\n"
                ),
            );
            let checked = hgl_in(&["check", &file], &[], &[]);
            let stderr = text(&checked.stderr);
            assert_eq!(
                checked.status.code(),
                Some(i32::from(!valid)),
                "{name} in entry {in_entry}: {stderr}"
            );
            if !valid {
                assert!(
                    stderr.contains("error[type/function.value]"),
                    "{name} in entry {in_entry}: {stderr}"
                );
            }
        }
    }
}

/// FN-19, FN-17 and EXPR-3: invalid places, iterables and declaration-bound
/// reads are diagnosed without unrolling the computed pack.
#[test]
fn a_computed_pack_spread_rejects_invalid_declaration_uses() {
    let cases = [
        (
            "pair(item: {for k in arg_keys(Ps): 1}, other: 2)",
            "type/type.mismatch",
            "a pack loop must be bound",
        ),
        (
            "{ let xs = [({for k in arg_keys(Ps): 1})]\n return 1 }",
            "type/type.mismatch",
            "a pack loop must be bound",
        ),
        (
            "f(...{for k in arg_keys(Ps): {for j in arg_keys(Ps): 1}})",
            "type/type.mismatch",
            "a pack loop must be bound",
        ),
        (
            "f(...{for k in arg_keys(Ps): Ps})",
            "type/type.mismatch",
            "is a type pack and cannot be read as a value",
        ),
        (
            "f(...{for k in arg_keys(Ps): Ps[k]})",
            "type/type.mismatch",
            "is a type pack and cannot be read as a value",
        ),
        (
            "f(...{for k in arg_keys(Ps): f})",
            "type/function.value",
            "a function parameter can be invoked or passed",
        ),
        (
            "f(...{for k in arg_keys(Ps): { k = \"x\"\n 1 }})",
            "name/binding.immutable",
            "is a pack key and cannot be assigned",
        ),
        (
            "f(...{for k in arg_keys(Ps): if b { 1 } else { 2 }})",
            "type/type.mismatch",
            "a pack loop condition must be constant",
        ),
        (
            "{ let flag = x > 1\n return f(...{for k in arg_keys(Ps): if flag { 1 } else { 2 }}) }",
            "type/type.mismatch",
            "a pack loop condition must be constant",
        ),
        (
            "{ var t = 0\n for i in [1, 2] { t += f(...{for k in arg_keys(Ps): if i == 1 { 1 } else { 2 }}) }\n return t }",
            "type/type.mismatch",
            "a pack loop condition must be constant",
        ),
        (
            "f(...{for k in arg_keys(Ps): if false { 1 } else if k in arg_keys(Ps) && b { 2 }})",
            "type/type.mismatch",
            "a pack loop condition must be constant",
        ),
        (
            "f(...{for i in 0..<3: i})",
            "type/type.mismatch",
            "a pack loop needs `arg_keys` of a type pack",
        ),
        (
            "f(...{for i in [1, 2]: i})",
            "type/type.mismatch",
            "a pack loop needs `arg_keys` of a type pack",
        ),
    ];
    for (index, (entry, code, message)) in cases.into_iter().enumerate() {
        let root = scratch(&format!("computed-pack-rejection-{index}"));
        let body = if entry.starts_with('{') { "" } else { "=> " };
        let file = write(
            &root,
            "m.hgl",
            &format!(
                "module m\n\
                 const fn pair(item: i64, other: i64) -> i64 => item + other\n\
                 const fn g<...Ps>(f: const fn(...{{Ps}}) -> i64, b: bool, x: i64) -> i64 {body}{entry}\n"
            ),
        );
        let checked = hgl_in(&["check", &file], &[], &[]);
        let stderr = text(&checked.stderr);
        assert_eq!(checked.status.code(), Some(1), "{entry}\n{stderr}");
        assert!(
            stderr.contains(&format!("error[{code}]")) && stderr.contains(message),
            "{entry}\n{stderr}",
        );
    }
}

/// FN-18 and EXPR-10: brace arguments reach their ordinary, generic,
/// value and temporal slots. Graphs pass const values and connections
/// on to a node, and configuration defaults are used when omitted.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn configuration_parameters_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("configuration_parameters");
    let file = write(
        &root,
        "demo.configuration_parameters.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.configuration_parameters.hgl"),
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

/// ERR-5, ERR-6, ERR-12 and ERR-15 through generated value code and
/// temporal nodes, including result guards (ERR-3), SOURCE-2 yield times
/// and an application failure.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn contextual_result_reads_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("fallible_forms");
    let file = write(
        &root,
        "demo.fallible.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.fallible.hgl"),
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

    let app = write(
        &root,
        "demo.fallible.entry.hgl",
        concat!(
            "application demo.fallible.entry\n",
            "use demo.fallible::{from_source}\n",
            "const fn bad() -> result<i64, error> => err(error(message: \"invalid configuration\", code: \"convert.invalid\"))\n",
            "inject global_state\n",
            "put(global_state, \"start\", {\n",
            "    let value: i64 = bad()\n",
            "    value\n",
            "})\n",
            "eval from_source(input: 1)\n",
        ),
    );
    let output = hgl_in(
        &[
            "run",
            &argument(&root),
            "--target",
            "cpp",
            "--application",
            "demo.fallible.entry",
        ],
        &[],
        &[],
    );
    assert_eq!(output.status.code(), Some(1), "{}", text(&output.stderr));
    let stderr = text(&output.stderr);
    let expected = format!(
        "hgl: demo.fallible.entry failed: convert.invalid: invalid configuration, raised at {app}:6"
    );
    assert_eq!(stderr.lines().last(), Some(expected.as_str()), "{stderr}");
}

/// ERR-12 and FN-11: a null publication fails a required input
/// before handler or lifted-call admission, including captured pack slots.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn unread_null_inputs_fail_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("unread_null_input");
    let file = write(
        &root,
        "demo.unread_null_input.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.unread_null_input.hgl"),
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
/// a `when` and an error's context.
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

/// ERR-7 gives graph and node operator candidates the same capture boundary.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn temporal_operator_captures_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("operator_capture");
    let file = write(
        &root,
        "demo.capture.operators.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.capture.operators.hgl"),
    );
    let output = hgl_in(&["test", &file, "--target", "cpp"], &[], &[]);
    let stdout = text(&output.stdout);
    assert!(output.status.success(), "{stdout}{}", text(&output.stderr));
    assert!(
        stdout.ends_with("executable cases: 2 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

/// RUN-6, UNIT-14: an entry receives the supplied configuration's start;
/// an omitted start is still null when the argument is evaluated.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn an_application_entry_reads_the_run_configuration() {
    if !sdk_named() {
        return;
    }
    let root = scratch("entry_run_config");
    let app = write(
        &root,
        "demo.entry_config.hgl",
        concat!(
            "application demo.entry_config\n",
            "fn chosen(const supplied: datetime?) -> bool {\n",
            "    yield 0s: supplied == null\n",
            "}\n",
            "inject run_config\n",
            "eval chosen(supplied: run_config.start)\n",
        ),
    );
    let omitted = hgl_in(&["run", &app, "--target", "cpp"], &[], &[]);
    assert_eq!(omitted.status.code(), Some(0), "{}", text(&omitted.stderr));
    assert_eq!(
        text(&omitted.stdout),
        "1970-01-01T00:00:00.000001Z  true\nran demo.entry_config on cpp: 1 publication\n"
    );
    let supplied = hgl_in(
        &[
            "run",
            &app,
            "--target",
            "cpp",
            "--config",
            "start=@2026-01-02T00:00:00Z",
        ],
        &[],
        &[],
    );
    assert_eq!(
        supplied.status.code(),
        Some(0),
        "{}",
        text(&supplied.stderr)
    );
    assert_eq!(
        text(&supplied.stdout),
        "2026-01-02T00:00:00Z  false\nran demo.entry_config on cpp: 1 publication\n"
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

/// TYPE-14 and ERR-12: a temporal branch consumes its refined nullable
/// connection as a payload, including in a temporal reduce callback.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn nullable_graph_branches_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("nullable-branches");
    let file = write(
        &root,
        "demo.nullable_branches.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.nullable_branches.hgl"),
    );
    let output = hgl_in(&["test", &file, "--target", "cpp"], &[], &[]);
    let stdout = text(&output.stdout);
    assert!(output.status.success(), "{stdout}{}", text(&output.stderr));
    assert!(
        stdout.ends_with("executable cases: 4 passed, 0 failed, 0 not run\n"),
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
            "    assert eval routed(route: [\"a\", \"c\"], value: [1, 2]) == [2, 3]\n",
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
        stdout.ends_with("executable cases: 24 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

/// A concrete generic struct is replayed and recorded using its applied
/// nominal schema (TYPE-3, TYPE-9, CEVAL-3, CEVAL-4).
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn concrete_generic_struct_traces_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("generic-struct-traces");
    let file = write(
        &root,
        "src/demo.generic-struct-traces.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.generic-struct-traces.hgl"),
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

/// `demo.keyed`, the temporal form of a set and of a dictionary (TYPE-9,
/// FN-3, TIME-7, TIME-9, END-3, END-4, TST-9): every test of it passes on the
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
    let constants = write(
        &root,
        "src/demo.keyed_constants.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.keyed_constants.hgl"),
    );
    let output = hgl_in(&["test", &file, &constants, "--target", "cpp"], &[], &[]);
    let stdout = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{stdout}{}",
        text(&output.stderr)
    );
    assert!(
        stdout.ends_with("executable cases: 11 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

/// Nested dictionary entry observations retain the final child's validity,
/// modification and delta, including where an intermediate key is absent (END-8).
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn nested_dictionary_entry_observations_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("nested-dictionary-observations");
    let file = write(
        &root,
        "src/demo.nested_dictionary_observations.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.nested_dictionary_observations.hgl"),
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
        stdout.ends_with("executable cases: 3 passed, 0 failed, 0 not run\n"),
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

/// FN-1: read fields and constant-index entries as const arguments.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn fixed_projections_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("fixed-reads");
    let file = write(
        &root,
        "demo.fixed_reads.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.fixed_reads.hgl"),
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
        stdout.ends_with(
            "rejection cases: 1 passed, 0 failed\nexecutable cases: 2 passed, 0 failed, 0 not run\n"
        ),
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
            "    assert eval relay(value: [1, null, 2]) == [1, null, 2]\n",
            "    assert eval read(value: [1, null, 2]) == [1, null, 2]\n",
            "    assert eval published(step: [0, 1, 2]) == [null, 7, null]\n",
            "    # A cell written as the delta it publishes, a published null among them.\n",
            "    assert eval relay(value: [delta<i64?>(), delta<i64?>(value: 3), 4]) ==\n",
            "        [null, delta<i64?>(value: 3), delta<i64?>(value: 4)]\n",
            "    assert eval read(value: [delta<i64?>(), 5]) == [delta<i64?>(), 5]\n",
            "}\n",
            "test a_list_of_nullables_publishes_its_delta {\n",
            "    assert eval relay_list(values: [[1, null], [null, 2]]) == [[1, null], [null, 2]]\n",
            "    assert eval read_list(values: [[1, null], delta<list<i64?>>(updated: [0: delta<i64?>()])]) ==\n",
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
/// is evaluated; an offset that is the zone's does not.
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

/// UNIT-14, RUN-1: preparing a debugger launch executes no user code. The
/// described child can run with its debugger's closed stdin and reports each publication.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn builds_an_isolated_debug_application_without_running_it() {
    if !sdk_named() {
        return;
    }
    let root = scratch("debug application");
    write(&root, "ticks.hgl", TICKS);
    write(&root, "backtest.hgl", BACKTEST);
    let builds = root.join("debug builds");
    // The test binary has no release beside it. Only a package stamped
    // for this hgl can supply debug libraries through HGL_HGRAPH_DEBUG.
    let version = std::env::var_os("HGL_HGRAPH_DEBUG")
        .and_then(|directory| fs::read_to_string(Path::new(&directory).join("hgl-debug.txt")).ok());
    let libraries = if version.as_deref().map(str::trim) == Some(env!("CARGO_PKG_VERSION")) {
        "debug"
    } else {
        "optimised"
    };
    let prepare = || {
        let report = hgl_in(
            &["run", &argument(&root), "--debug-build", &argument(&builds)],
            &[],
            &["LD_LIBRARY_PATH"],
        );
        assert!(report.status.success(), "{}", text(&report.stderr));
        assert!(
            text(&report.stderr).starts_with(&format!(
                "hgl: a debug build, with hgraph's {libraries} libraries "
            )),
            "{}",
            text(&report.stderr)
        );
        serde_json::from_slice::<serde_json::Value>(&report.stdout).unwrap()
    };
    let launch = prepare();
    assert_eq!(launch["version"], 1);
    assert_eq!(launch["hgraph"]["libraries"], libraries);
    for prefix in ["sdk", "dependencies"] {
        assert!(Path::new(launch["hgraph"][prefix].as_str().unwrap()).is_dir());
    }
    let executable = launch["executable"].as_str().unwrap();
    #[cfg(target_os = "linux")]
    {
        let directories = launch["environment"]["LD_LIBRARY_PATH"].as_str().unwrap();
        assert!(std::env::split_paths(directories).all(|path| !path.as_os_str().is_empty()));
        check_backtest_line_table(executable);
    }
    let args = launch["arguments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap());
    let results = std::path::Path::new(launch["arguments"][0].as_str().unwrap());
    assert!(!results.exists());
    // A subsequent session cannot rebuild the executable a debugger is using.
    assert_ne!(prepare()["executable"], executable);
    let run = Command::new(executable)
        .args(args)
        .current_dir(launch["workingDirectory"].as_str().unwrap())
        .envs(
            launch["environment"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| (k, v.as_str().unwrap())),
        )
        .output()
        .unwrap();
    assert!(run.status.success(), "{}", text(&run.stderr));
    assert_eq!(std::fs::read(results).unwrap(), run.stdout);
    assert_eq!(
        text(&run.stdout)
            .lines()
            .filter(|line| line.starts_with("published "))
            .count(),
        3
    );
}

/// Inspect the native addresses a debugger can bind to (RUN-1). Line 18 is
/// an assignment in count_of, which this application never wires.
#[cfg(target_os = "linux")]
fn check_backtest_line_table(executable: &str) {
    let dump = Command::new("readelf")
        .args(["--debug-dump=decodedline", executable])
        .env("LC_ALL", "C")
        .output()
        .expect("GNU readelf reads the SDK executable");
    assert!(dump.status.success(), "{}", text(&dump.stderr));
    let table = text(&dump.stdout);
    let count_line = TICKS
        .lines()
        .position(|line| line.trim() == "count += 1")
        .unwrap()
        + 1;
    let mut count_addresses = Vec::new();
    for (file, source) in [("ticks.hgl", TICKS), ("backtest.hgl", BACKTEST)] {
        let end = source.lines().count();
        let mut seen = false;
        for row in table.lines() {
            let mut fields = row.split_whitespace();
            if fields.next() != Some(file) {
                continue;
            }
            // A dash ends a DWARF line sequence and has no source line.
            let number = fields.next().unwrap();
            if number == "-" {
                continue;
            }
            let line: usize = number.parse().unwrap();
            seen = true;
            assert!((1..=end).contains(&line), "{file} has {end} lines: {row}");
            if file == "ticks.hgl" && line == count_line {
                count_addresses.push(fields.next().unwrap());
            }
        }
        assert!(seen, "no line entries for {file}");
    }
    assert!(!count_addresses.is_empty(), "no address for count += 1");
    let symbols = Command::new("addr2line")
        .args(["--functions", "--demangle", "--exe", executable])
        .args(&count_addresses)
        .output()
        .expect("GNU addr2line reads the breakpoint addresses");
    assert!(symbols.status.success(), "{}", text(&symbols.stderr));
    let locations = text(&symbols.stdout);
    let owners: Vec<_> = locations.lines().step_by(2).collect();
    assert_eq!(owners.len(), count_addresses.len());
    assert!(
        owners
            .iter()
            .all(|name| name.contains("::count_of::impl_::eval(")),
        "{locations}"
    );
}

/// RUN-2, RUN-6: inserting a configuration or replacing a multiline one
/// keeps the native bounds and entry call on the eval line the user sees.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn debug_overrides_keep_the_on_disk_eval_line() {
    if !sdk_named() {
        return;
    }
    let root = scratch("debug overrides");
    write(&root, "ticks.hgl", TICKS);
    for inject in ["", "inject run_config{\n    length: 2s\n}\n"] {
        let app = format!(
            "application demo.backtest\nuse demo.ticks::{{doubled}}\n{inject}eval doubled(values: [1, 2])\n"
        );
        let file = write(&root, "entry.hgl", &app);
        let result = hgl_in(
            &[
                "run",
                &argument(&root),
                "--config",
                "length=4s",
                "--arg",
                "values=[3]",
                "--debug-build",
                &argument(&root.join("builds")),
            ],
            &[],
            &[],
        );
        assert!(result.status.success(), "{}", text(&result.stderr));
        let manifest: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
        let executable = std::path::Path::new(manifest["executable"].as_str().unwrap());
        let build = executable.parent().unwrap().parent().unwrap();
        let unit = fs::read_to_string(build.join("modules/mod-demo-backtest.cpp")).unwrap();
        let eval_line = app.lines().count();
        assert!(unit.contains(&format!("demo.backtest, line {eval_line}: `eval`")));
        let directive = format!("#line {eval_line} {file:?}");
        let preceding = |code: &str| {
            unit.lines()
                .zip(unit.lines().skip(1))
                .find_map(|(before, line)| line.contains(code).then_some(before))
                .unwrap()
        };
        assert_eq!(preceding("::Bounds bounds_ ="), directive);
        assert_eq!(preceding("::record::"), directive);
        let config = preceding("::RunConfig run_config =");
        if inject.is_empty() {
            assert!(
                config.ends_with(r#""modules/mod-demo-backtest.cpp""#),
                "{config}"
            );
        } else {
            assert_eq!(config, format!("#line 3 {file:?}"));
        }
        assert_eq!(fs::read_to_string(file).unwrap(), app);
    }
}

/// A rejected debug build has no launch description on stdout.
#[test]
fn a_rejected_debug_application_has_no_launch_description() {
    let root = scratch("rejected debug build");
    write(
        &root,
        "backtest.hgl",
        "application demo.backtest\neval unknown()\n",
    );
    let report = hgl_in(
        &[
            "run",
            &argument(&root),
            "--debug-build",
            &argument(&root.join("build")),
        ],
        &[],
        &[],
    );
    assert!(!report.status.success());
    assert!(report.stdout.is_empty());
    assert!(text(&report.stderr).contains("unknown"));
    assert!(!root.join("build").exists());
}

/// A valid application that cannot be built because no SDK is named
/// exits with status 2 and reports why it was not run.
#[test]
fn an_application_is_not_run_without_an_sdk() {
    let root = scratch("application_no_sdk");
    write(&root, "ticks.hgl", TICKS);
    write(&root, "backtest.hgl", BACKTEST);
    let output = hgl_in(
        &["run", &argument(&root), "--target", "cpp"],
        &[],
        &["HGL_HGRAPH_SDK", "HGL_HGRAPH_DEPS"],
    );
    let stderr = text(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "{stderr}");
    assert_eq!(text(&output.stdout), "");
    assert!(
        stderr.starts_with("hgl: demo.ticks.backtest was not run: HGL_HGRAPH_SDK is not set"),
        "{stderr}"
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
        ("started.hgl", STARTED),
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
    assert!(
        kept.join("applications/app-demo-ticks-backtest.cpp")
            .is_file()
    );
    assert!(
        kept.join(format!(
            "run/app-demo-ticks-backtest{}",
            std::env::consts::EXE_SUFFIX
        ))
        .is_file()
    );

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

    // RUN-6: `run_config` reads the configuration as supplied: the
    // application's defaults, then a start supplied over them, and an end
    // supplied, which replaces the default length.
    let output = run("demo.ticks.started", &[]);
    assert_eq!(
        text(&output.stdout),
        "1970-01-01T00:00:01.000001Z  false\n1970-01-01T00:00:02.000001Z  false\n\
         ran demo.ticks.started on cpp: 2 publications\n",
        "{}",
        text(&output.stderr)
    );
    let opening = ["--config", "start=@2026-01-02T00:00:00Z"];
    let output = run("demo.ticks.started", &opening);
    assert_eq!(
        text(&output.stdout),
        "2026-01-02T00:00:01Z  true\n2026-01-02T00:00:02Z  true\n\
         ran demo.ticks.started on cpp: 2 publications\n",
        "{}",
        text(&output.stderr)
    );
    let output = run(
        "demo.ticks.started",
        &[
            opening[0],
            opening[1],
            "--config",
            "end=@2026-01-02T00:00:02Z",
        ],
    );
    assert_eq!(
        text(&output.stdout),
        "2026-01-02T00:00:01Z  true\nran demo.ticks.started on cpp: 1 publication\n",
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

/// UNIT-1, UNIT-2. An importing module's header does not overwrite
/// the types header of the module it imports.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn module_paths_like_types_headers_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let fixtures =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../hgl_cpp/tests/fixtures/module-stems/types");
    let output = hgl_in(&["test", "--target", "cpp", &argument(&fixtures)], &[], &[]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let printed = text(&output.stdout);
    assert!(
        printed.contains("passed   executable  tc.a.types::both_types"),
        "{printed}"
    );
    assert!(
        printed.contains("executable cases: 1 passed, 0 failed, 0 not run"),
        "{printed}"
    );
}

/// UNIT-1, UNIT-2. A case-insensitive filesystem keeps both builds,
/// including the executable of a path ending in `.app` on macOS.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn case_distinct_module_paths_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let fixtures =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../hgl_cpp/tests/fixtures/module-stems/case");
    let kept = scratch("case_distinct_module_build");
    let output = hgl_in(
        &[
            "test",
            "--target",
            "cpp",
            "--keep",
            &argument(&kept),
            &argument(&fixtures),
        ],
        &[],
        &[],
    );
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let printed = text(&output.stdout);
    assert!(
        printed.contains("passed   executable  tm.Pricing.App::upper_price"),
        "{printed}"
    );
    assert!(
        printed.contains("passed   executable  tm.pricing.app::lower_price"),
        "{printed}"
    );
    assert!(
        printed.contains("executable cases: 2 passed, 0 failed, 0 not run"),
        "{printed}"
    );
    let suffix = std::env::consts::EXE_SUFFIX;
    for name in ["tm-+pricing-+app", "tm-pricing-app"] {
        for artifact in [
            format!("bin/test-{name}{suffix}"),
            format!("obj/modules/mod-{name}.o"),
            format!("obj/tests/test-{name}.o"),
        ] {
            assert!(kept.join(&artifact).is_file(), "missing {artifact}");
        }
    }
    let mut results: Vec<_> = fs::read_dir(kept.join("results"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    results.sort();
    assert_eq!(results.len(), 2);
    assert!(
        results[0].starts_with("test-tm-+pricing-+app."),
        "{results:?}"
    );
    assert!(
        results[1].starts_with("test-tm-pricing-app."),
        "{results:?}"
    );
}

/// UNIT-14: application paths stay visible, while their files have
/// distinct stems. The `.app` case also exercises launch on macOS,
/// which treats a dotted `.app` filename as a bundle.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn applications_with_extension_like_names_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("application_filenames");
    let kept = root.join("build");
    let cases = [
        ("demo.pricing.app", "app-demo-pricing-app", 11),
        ("demo.pricing.exe", "app-demo-pricing-exe", 12),
        ("demo.pricing.so", "app-demo-pricing-so", 13),
        ("a.b_c", "app-a-b_c", 21),
        ("a_b.c", "app-a_b-c", 22),
        ("a.B_c", "app-a-+b_c", 23),
    ];
    for (index, (path, _, value)) in cases.iter().enumerate() {
        let test = if *path == "demo.pricing.app" {
            "test artifact_identity { assert true }\n"
        } else {
            ""
        };
        write(
            &root,
            &format!("case-{index}.hgl"),
            &format!(
                "application {path}\n\
                 struct Quote {{ cents: i64 }}\n\
                 fn price(const item: Quote) -> i64 {{ yield 0s: item.cents }}\n\
                 {test}eval price(item: Quote(cents: {value}))\n"
            ),
        );
    }
    let suffix = std::env::consts::EXE_SUFFIX;
    for (path, stem, value) in cases {
        let output = hgl_in(
            &[
                "run",
                &argument(&root),
                "--target",
                "cpp",
                "--keep",
                &argument(&kept),
                "--application",
                path,
            ],
            &[],
            &[],
        );
        assert_eq!(
            output.status.code(),
            Some(0),
            "{path}: {}",
            text(&output.stderr)
        );
        assert_eq!(
            text(&output.stdout),
            format!("1970-01-01T00:00:00.000001Z  {value}\nran {path} on cpp: 1 publication\n"),
        );
        let name = stem.strip_prefix("app-").unwrap();
        for artifact in [
            format!("applications/{stem}.cpp"),
            format!("modules/mod-{name}.cpp"),
            format!("modules/mod-{name}.h"),
            format!("modules/type-{name}.h"),
            format!("obj/applications/{stem}.o"),
            format!("obj/modules/mod-{name}.o"),
            format!("results/{stem}.run.txt"),
            format!("run/{stem}{suffix}"),
        ] {
            assert!(kept.join(&artifact).is_file(), "{path}: missing {artifact}");
        }
    }

    let tests = hgl_in(
        &[
            "test",
            &argument(&root),
            "--target",
            "cpp",
            "--keep",
            &argument(&kept),
            "--test",
            "artifact_identity",
        ],
        &[],
        &[],
    );
    assert_eq!(tests.status.code(), Some(0), "{}", text(&tests.stderr));
    assert!(
        text(&tests.stdout).contains("passed   executable  demo.pricing.app::artifact_identity")
    );
    assert!(kept.join("tests/test-demo-pricing-app.cpp").is_file());
    assert!(
        kept.join(format!("bin/test-demo-pricing-app{suffix}"))
            .is_file()
    );
    assert!(kept.join("results/test-demo-pricing-app.0.txt").is_file());
}

/// An application of `ticks` whose arguments and configuration are
/// replaced together.
const PACED: &str = concat!(
    "application demo.ticks.paced\n",
    "\n",
    "use demo.ticks::{ticks}\n",
    "\n",
    "inject run_config{length: 10s}\n",
    "eval ticks(values: [1, 2, 3])\n",
);

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

/// The applications of `SIDES`: of the enum's entry; of the generic one,
/// whose `T` its argument makes an `i64`, a `Side` and an `f64`.
const SIDED: [(&str, &str); 4] = [
    (
        "sided.hgl",
        "application demo.sides.sided\n\nuse demo.sides::{sided}\n\neval sided(side: .buy)\n",
    ),
    (
        "echoed.hgl",
        "application demo.sides.echoed\n\nuse demo.sides::{echo}\n\neval echo(value: 3)\n",
    ),
    (
        "member.hgl",
        "application demo.sides.member\n\nuse demo.sides::{echo, Side}\n\neval echo(value: Side.buy)\n",
    ),
    (
        "real.hgl",
        "application demo.sides.real\n\nuse demo.sides::{echo}\n\neval echo(value: 2.5)\n",
    ),
];

/// RUN-2: the replaced arguments and the replaced field are what the
/// application runs with, "and what the application writes is the
/// default". A replacement is read as the argument it replaces was
/// expected: with the `Side` and the `f64` the application's own
/// argument made the entry's `T`, `.sell` is a member (TYPE-7) and `3`
/// an `f64` (EXPR-5). The target is cpp where none is named.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn an_application_runs_with_what_starts_it_replaces() {
    if !sdk_named() {
        return;
    }
    let root = scratch("replaced");
    write(&root, "ticks.hgl", TICKS);
    write(&root, "paced.hgl", PACED);
    write(&root, "sides.hgl", SIDES);
    for (name, text) in SIDED {
        write(&root, name, text);
    }
    let run = |application: &str, more: &[&str]| {
        let mut arguments = vec!["run", root.to_str().unwrap(), "--application", application];
        arguments.extend(more);
        hgl_in(&arguments, &[], &[])
    };
    // As written: `ticks(values: [1, 2, 3])` a second apart, for 10s.
    let output = run(
        "demo.ticks.paced",
        &[
            "--arg",
            "values=[1, 2, 3, 4, 5]",
            "--arg",
            "every=500ms",
            "--config",
            "length=2s",
        ],
    );
    assert_eq!(
        text(&output.stdout),
        concat!(
            "1970-01-01T00:00:00.500001Z  1\n",
            "1970-01-01T00:00:01.000001Z  2\n",
            "1970-01-01T00:00:01.500001Z  3\n",
            "ran demo.ticks.paced on cpp: 3 publications\n",
        ),
        "{}",
        text(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(0));
    let output = run("demo.sides.sided", &["--arg", "side=.sell"]);
    assert_eq!(
        text(&output.stdout),
        concat!(
            "1970-01-01T00:00:01.000001Z  -1\n",
            "1970-01-01T00:00:02.000001Z  -2\n",
            "ran demo.sides.sided on cpp: 2 publications\n",
        ),
        "{}",
        text(&output.stderr)
    );
    let output = run("demo.sides.echoed", &["--arg", "value=4 + 1"]);
    assert_eq!(
        text(&output.stdout),
        "1970-01-01T00:00:01.000001Z  5\nran demo.sides.echoed on cpp: 1 publication\n",
        "{}",
        text(&output.stderr)
    );
    let output = run("demo.sides.member", &["--arg", "value=.sell"]);
    assert_eq!(
        text(&output.stdout),
        "1970-01-01T00:00:01.000001Z  Side.sell\nran demo.sides.member on cpp: 1 publication\n",
        "{}",
        text(&output.stderr)
    );
    let output = run("demo.sides.real", &["--arg", "value=3"]);
    assert_eq!(
        text(&output.stdout),
        "1970-01-01T00:00:01.000001Z  3.0\nran demo.sides.real on cpp: 1 publication\n",
        "{}",
        text(&output.stderr)
    );
    // With nothing replaced it runs as it is written.
    let output = run("demo.ticks.paced", &[]);
    assert_eq!(
        text(&output.stdout),
        concat!(
            "1970-01-01T00:00:01.000001Z  1\n",
            "1970-01-01T00:00:02.000001Z  2\n",
            "1970-01-01T00:00:03.000001Z  3\n",
            "ran demo.ticks.paced on cpp: 3 publications\n",
        )
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
    let kept = root.join("build");
    let results = kept.join("results/app-demo-ticks-endless.run.txt");
    let mut child = Command::new(env!("CARGO_BIN_EXE_hgl"))
        .args([
            "run",
            &argument(&root),
            "--target",
            "cpp",
            "--keep",
            &argument(&kept),
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("the hgl binary runs");
    // Wait for the running application rather than guessing how long its
    // build takes. An interrupt during the build has a different outcome.
    let started = std::time::Instant::now();
    loop {
        let recorded = fs::read_to_string(&results).unwrap_or_default();
        if recorded.starts_with("start\n") {
            break;
        }
        if started.elapsed() >= std::time::Duration::from_secs(120)
            || child.try_wait().expect("hgl can be polled").is_some()
        {
            let _ = child.kill();
            let output = child.wait_with_output().expect("hgl ends");
            panic!("the application never started: {}", text(&output.stderr));
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
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
    // It counted from 1 for as long as it ran. The host can receive the
    // stop before the engine publishes its first value.
    for (position, line) in published.iter().enumerate() {
        assert!(line.ends_with(&format!("Z  {}", position + 1)), "{printed}");
    }
    assert!(
        summary.starts_with("ran demo.ticks.endless on cpp until it was stopped: "),
        "{printed}"
    );
}

/// CAP-6 on the SDK: every ordinary value put in the run's store is got
/// back as itself, including nullables, structs, tuples, dictionaries,
/// sets and deltas. Values are written in a node, where a graph is built,
/// and by an application. Generated `#line` directives give failures
/// in the graph build and application statements their HGL file and line.
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
    for (name, source) in [
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
        (
            "demo.store.nullable.hgl",
            include_str!("../../hgl_cpp/tests/fixtures/demo.store.nullable.hgl"),
        ),
    ] {
        write(&root, name, source);
    }
    // Include the rejected source only in the test harness. The ordinary
    // application runs reject a program with an invalid test context.
    let rejection = write(
        &scratch("store_inferred_nullable"),
        "demo.store.inferred_nullable.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.store.inferred_nullable.hgl"),
    );
    let kept = root.join("build");
    let output = hgl_in(
        &[
            "test",
            &argument(&root),
            &rejection,
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
        stdout.ends_with("executable cases: 4 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
    assert!(
        stdout.contains(
            "passed   rejection   demo.store.inferred_nullable::an_unwritten_store_read_rejects\n"
        ),
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
    // of a key put nowhere on line 80 of the module's, in the graph (CAP-6).
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
                "hgl: demo.store.unbuilt failed: value.missing_key: the global store holds nothing under 'by', \
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
            "eval ticks(values: [1])\n",
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
            "eval ticks(values: [1])\n",
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
            "    assert eval scale(value: [1]) == [2]\n",
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

/// FN-18, GEN-6, EXPR-10, EXPR-13: a named or symbol operator gives
/// the selected candidate its configuration in written evaluation order.
/// An inherited default keeps the operator's generic environment (UNIT-7).
/// A requirement describes the ordinary operands (GEN-16).
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn operator_configuration_runs_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("operator_configuration");
    let file = write(
        &root,
        "demo.operator_configuration.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.operator_configuration.hgl"),
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
        stdout.ends_with("executable cases: 6 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

/// EXPR-14, FN-6 and NUM-11: the selected declaration determines
/// which options are lifted inputs and which bind at graph construction.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn option_declarations_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("option_declarations");
    let cases = [
        (
            "demo.option_declarations.hgl",
            include_str!("../../hgl_cpp/tests/fixtures/demo.option_declarations.hgl"),
            "executable cases: 8 passed, 0 failed, 0 not run\n",
        ),
        (
            "demo.option_declarations_rejections.hgl",
            include_str!("../../hgl_cpp/tests/fixtures/demo.option_declarations_rejections.hgl"),
            "rejection cases: 4 passed, 0 failed\n",
        ),
    ];
    for (name, source, expected) in cases {
        let file = write(&root, name, source);
        let output = hgl_in(&["test", &file, "--target", "cpp"], &[], &[]);
        let stdout = text(&output.stdout);
        assert!(output.status.success(), "{stdout}{}", text(&output.stderr));
        assert!(stdout.contains(expected), "{stdout}");
    }
}

/// EXPR-6, OPS-5, FN-6 and FN-11: Boolean symbols use the selected
/// operator's inputs in a graph and still short-circuit in value code.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn logical_connections_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("logical_connections");
    let file = write(
        &root,
        "demo.logical_connections.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.logical_connections.hgl"),
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
        stdout.ends_with("executable cases: 3 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

/// UNIT-7 and UNIT-8: omitted defaults can run their own blocks in
/// value calls and in graph configuration, including for a lifted call.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn callable_block_defaults_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("callable-block-defaults");
    let file = write(
        &root,
        "demo.callable_block_defaults.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.callable_block_defaults.hgl"),
    );
    let output = hgl_in(&["test", &file, "--target", "cpp"], &[], &[]);
    let stdout = text(&output.stdout);
    assert!(output.status.success(), "{stdout}{}", text(&output.stderr));
    assert!(
        stdout.ends_with("executable cases: 2 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

/// FN-4, FN-6, FN-17, GEN-5 and GEN-6: prefer the direct role,
/// including where a pair takes a callable, and keep ordinary and eval roles.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn call_role_preferences_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("call-role-preference");
    let file = write(
        &root,
        "demo.call_roles.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.call_roles.hgl"),
    );
    let output = hgl_in(&["test", &file, "--target", "cpp"], &[], &[]);
    let stdout = text(&output.stdout);
    assert!(output.status.success(), "{stdout}{}", text(&output.stderr));
    assert!(
        stdout.ends_with(
            "rejection cases: 3 passed, 0 failed\nexecutable cases: 13 passed, 0 failed, 0 not run\n"
        ),
        "{stdout}"
    );
}

/// GEN-16, UNIT-8: a required symbol keeps the public generic arguments
/// that its selected candidate's inherited configuration default reads.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn required_symbols_inherit_configuration_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("required_symbol_configuration");
    write(
        &root,
        "core.hgl",
        concat!(
            "module core\n",
            "export operator add_<T, const N: i64 = 2>{adjustment: i64 = N}(lhs: T, rhs: T) -> T\n",
            "impl const fn add_{adjustment: i64}(lhs: i64, rhs: i64) -> i64 => adjustment\n",
            "export operator neg_<T, const N: i64 = 3>{adjustment: i64 = N}(value: T) -> T\n",
            "impl const fn neg_{adjustment: i64}(value: i64) -> i64 => adjustment\n",
            "export operator eq_(lhs: i64, rhs: i64) -> bool\n",
            "native impl const fn eq_(lhs: i64, rhs: i64, abs: f64 = 0.0, rel: f64 = 0.0) -> bool\n",
        ),
    );
    write(
        &root,
        "demo.required_symbol_configuration.hgl",
        concat!(
            "module demo.required_symbol_configuration\n",
            "const fn sum<T>(lhs: T, rhs: T) -> T requires add_(T, T) -> T => lhs + rhs\n",
            "const fn negate<T>(value: T) -> T requires neg_(T) -> T => -value\n",
            "test inherited_configuration_reads_defaults {\n",
            "    assert sum(11, 12) == 2\n",
            "    assert negate(13) == 3\n",
            "}\n",
        ),
    );
    let output = hgl_in(
        &["test", root.to_str().unwrap(), "--target", "cpp"],
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
        stdout.ends_with("executable cases: 1 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

/// GEN-3, GEN-7, GEN-9, GEN-10 and GEN-18: existential binding and
/// numeric operator laws are settled before the target builds a program.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn any_and_operator_laws_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("any_laws");
    let file = write(
        &root,
        "demo.any_laws.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.any_laws.hgl"),
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

/// GEN-16, UNIT-10: operators named like law predicates retain their
/// ordinary callability requirements in functions and candidates.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn named_law_requirements_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("named_law_requirements");
    let file = write(
        &root,
        "demo.named_law_requirements.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.named_law_requirements.hgl"),
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
        stdout.ends_with("executable cases: 1 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

/// GEN-5: the specification's complete tuple-specificity example
/// exercises both the fixed-length tuple and the admitting pack.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn fixed_length_tuple_example_runs_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("tuple-specificity");
    let file = write(
        &root,
        "examples.tuple_specificity.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/examples.tuple_specificity.hgl"),
    );
    let output = hgl_in(&["test", &file, "--target", "cpp"], &[], &[]);
    let stdout = text(&output.stdout);
    assert!(output.status.success(), "{stdout}{}", text(&output.stderr));
    assert!(
        stdout.ends_with(
            "rejection cases: 0 passed, 0 failed\nexecutable cases: 2 passed, 0 failed, 0 not run\n"
        ),
        "{stdout}"
    );
}

/// GEN-3 and GEN-10: category selection and tuple type packs execute on
/// the SDK, including forwarding and independently typed callbacks.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn category_constraints_and_tuple_packs_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("category-packs");
    let file = write(
        &root,
        "demo.categories.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.categories.hgl"),
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
        stdout.ends_with(
            "rejection cases: 28 passed, 0 failed\nexecutable cases: 19 passed, 0 failed, 0 not run\n"
        ),
        "{stdout}"
    );
}

/// UNIT-10 and UNIT-12: emitted code uses the nearest binding while a
/// module or prelude function still works outside the hiding scope.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn lexical_names_can_hide_module_and_prelude_functions() {
    if !sdk_named() {
        return;
    }
    let root = scratch("unit12_shadowing");
    let file = write(
        &root,
        "src/demo.unit12_shadowing.hgl",
        include_str!("../../hgl_cpp/tests/fixtures/demo.unit12_shadowing.hgl"),
    );
    let checked = hgl_in(&["check", &file], &[], &[]);
    let diagnostics = text(&checked.stderr);
    assert!(checked.status.success(), "{diagnostics}");
    assert_eq!(
        diagnostics.matches("warning[name.hides_function]").count(),
        1,
        "{diagnostics}"
    );

    let output = hgl_in(&["test", &file, "--target", "cpp"], &[], &[]);
    let stdout = text(&output.stdout);
    let stderr = text(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    assert!(
        stdout.contains("passed   executable  demo.unit12_shadowing::uses_nearest_binding"),
        "{stdout}"
    );
}
