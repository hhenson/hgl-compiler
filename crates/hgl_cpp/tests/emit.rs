//! The cpp target against small programs: the C++ it writes, what it
//! reports for a form it does not write yet, and which tests can then
//! still be run.
//!
//! The C++ written for a program is compared with the files under
//! `tests/goldens/`, one directory for each program, holding each file
//! the target writes at the path it writes it to. To write them again
//! after a change to what the target writes, run
//!
//! ```text
//! HGL_UPDATE_GOLDENS=1 cargo test -p hgl_cpp --test emit
//! ```
//!
//! and read the difference before it is committed. Building and running
//! the C++ is the build driver's, and is tested with `hgl_cli`.

use std::fs;
use std::path::{Path, PathBuf};

use hgl_analyser::{ParsedFile, analyse};
use hgl_cpp::{Output, transpile_for};
use hgl_diagnostics::{Diagnostics, render};
use hgl_lexer::{lex, literal};
use hgl_parser::{Entry, Limits, parse};
use hgl_source::SourceMap;

/// What the target makes of a program.
struct Emitted {
    /// The program's files, to say where a place is.
    sources: SourceMap,
    /// The files written, and the program's tests.
    output: Output,
    /// What the target reported, each as `file:line:column message`.
    reported: Vec<String>,
}

impl Emitted {
    /// A place as `file:line:column`.
    fn place(&self, at: hgl_diagnostics::Location) -> String {
        let source = self.sources.file(at.file);
        format!("{}:{}", source.name(), source.line_col(at.span.start))
    }

    /// What blocks the test named `name`, as `file:line:column
    /// message`, or `None` for one that can be run.
    fn blocked(&self, name: &str) -> Option<String> {
        let test = self
            .output
            .tests
            .iter()
            .find(|test| test.name == name)
            .unwrap_or_else(|| panic!("no test `{name}` was written"));
        test.blocked
            .as_ref()
            .map(|diagnostic| format!("{} {}", self.place(diagnostic.primary), diagnostic.message))
    }
}

/// Lexes, parses and analyses `files`, given as `(name, text)`, with
/// the core the compiler is built with, and writes the program as C++
/// for a runtime library that defines the natives [`provides`] names.
/// The program must be one the front end and the analyser accept.
fn emit(files: &[(&str, &str)]) -> Emitted {
    emit_with(files, &provides)
}

/// [`emit`], for a runtime library that defines the natives `provides`
/// names.
fn emit_with(files: &[(&str, &str)], provides: &dyn Fn(&str) -> bool) -> Emitted {
    let mut sources = SourceMap::new();
    let mut diagnostics = Diagnostics::new();
    let mut parsed = Vec::new();
    for (name, text) in files.iter().chain(&hgl_analyser::CORE_FILES) {
        let file = sources.add(*name, *text).unwrap();
        let tokens = lex(&sources, file, &mut diagnostics);
        literal::validate_literals(&sources, file, &tokens, &mut diagnostics);
        let tree = parse(
            &sources,
            file,
            &tokens,
            Entry::SourceFile,
            Limits::default(),
            &mut diagnostics,
        );
        parsed.push(ParsedFile::new(file, tree.syntax()));
    }
    let program = analyse(&parsed, &mut diagnostics);
    assert!(
        !diagnostics.has_errors(),
        "the program is accepted:\n{}",
        render::to_string(&sources, &diagnostics)
    );
    let before = diagnostics.len();
    let output = transpile_for(&program, provides, &mut diagnostics);
    let reported = diagnostics.as_slice()[before..]
        .iter()
        .map(|diagnostic| {
            let source = sources.file(diagnostic.primary.file);
            format!(
                "{}:{} {}",
                source.name(),
                source.line_col(diagnostic.primary.span.start),
                diagnostic.message
            )
        })
        .collect();
    Emitted {
        sources,
        output,
        reported,
    }
}

/// The natives of core these tests take the runtime library to define,
/// by the names it gives them: the checked sum of two `i64` (NUM-2) and
/// the equality of two `i64` within tolerances (NUM-4), which the
/// library's first natives include.
fn provides(name: &str) -> bool {
    ["add__i64_i64", "eq__i64_i64_f64_f64"].contains(&name)
}

/// The directory of the goldens of the program `name`.
fn goldens(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/goldens")
        .join(name)
}

/// The files of `output` that are the program's own: core's module,
/// which every program has, is left out, so that a golden does not
/// change with core.
fn own_files(output: &Output) -> Vec<&hgl_cpp::File> {
    output
        .files
        .iter()
        .filter(|file| !file.path.starts_with("modules/core."))
        .collect()
}

/// Compares every file `output` holds but core's with the goldens of
/// `name`, file by file, and that no golden is left that the target no
/// longer writes; or, with `HGL_UPDATE_GOLDENS` set, writes them.
fn assert_goldens(name: &str, output: &Output) {
    let directory = goldens(name);
    let files = own_files(output);
    if std::env::var_os("HGL_UPDATE_GOLDENS").is_some() {
        fs::remove_dir_all(&directory).ok();
        for file in &files {
            let path = directory.join(&file.path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, &file.text).unwrap();
        }
        return;
    }
    let mut kept = Vec::new();
    for entry in walk(&directory) {
        let relative = entry
            .strip_prefix(&directory)
            .unwrap()
            .components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        kept.push(relative);
    }
    kept.sort();
    let mut written: Vec<String> = files.iter().map(|file| file.path.clone()).collect();
    written.sort();
    assert_eq!(
        written, kept,
        "the files written are the goldens' (HGL_UPDATE_GOLDENS=1 writes them again)"
    );
    for file in &files {
        // Git may check text out with CRLF line ends on Windows.
        let golden = fs::read_to_string(directory.join(&file.path))
            .unwrap()
            .replace("\r\n", "\n");
        assert_eq!(
            file.text, golden,
            "{} differs from its golden (HGL_UPDATE_GOLDENS=1 writes it again)",
            file.path
        );
    }
}

/// Every file under `directory`, its subdirectories included.
fn walk(directory: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = fs::read_dir(directory) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(walk(&path));
        } else {
            found.push(path);
        }
    }
    found
}

/// The smallest program written whole: a `const fn` over `i64` whose
/// `+` is the runtime library's checked native (NUM-2), and tests that
/// assert on what it gives (TST-4). Each statement of a test tells the
/// library its line, which names the statement a failure is in.
#[test]
fn a_value_function_and_its_tests_are_written() {
    let emitted = emit(&[("demo.calc.hgl", include_str!("fixtures/demo.calc.hgl"))]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    assert_goldens("demo.calc", &emitted.output);
    assert_eq!(emitted.blocked("sums"), None);
    assert_eq!(emitted.blocked("overflows"), None);
    // One executable for the module, linked from its unit.
    let output = &emitted.output;
    assert_eq!(output.executables.len(), 1);
    let executable = &output.executables[0];
    assert_eq!(executable.source, "tests/demo.calc.cpp");
    let units: Vec<&str> = executable
        .units
        .iter()
        .map(|&unit| output.units[unit].source.as_str())
        .collect();
    assert_eq!(units, ["modules/demo.calc.cpp"]);
    // A failure at a line of the unit is traced to the statement of the
    // test that line is in.
    let unit = &output
        .files
        .iter()
        .find(|file| file.path == "modules/demo.calc.cpp")
        .unwrap()
        .text;
    let sums = output
        .tests
        .iter()
        .find(|test| test.name == "sums")
        .unwrap();
    let marked: Vec<usize> = unit
        .lines()
        .enumerate()
        .filter(|(_, line)| line.contains("::hglrt::testing::statement()"))
        .map(|(number, _)| number + 1)
        .collect();
    let lines: Vec<String> = marked
        .iter()
        .map(|&line| {
            sums.statement_at(line)
                .map_or("-".to_string(), |at| emitted.place(at))
        })
        .collect();
    assert_eq!(
        lines,
        [
            "demo.calc.hgl:12:5",
            "demo.calc.hgl:13:9",
            "demo.calc.hgl:14:5",
            // The next test's statement is past the last of `sums`.
            "demo.calc.hgl:14:5",
        ]
    );
}

/// A form the target does not write yet is reported where it is
/// written, and a test that reaches it is not run, with that reason; a
/// test that reaches nothing of the kind is run all the same.
#[test]
fn a_form_not_written_yet_blocks_what_reaches_it() {
    let emitted = emit(&[(
        "demo.partial.hgl",
        concat!(
            "module demo.partial\n",
            "\n",
            "const fn total(a: i64, b: i64) -> i64 => a + b\n",
            "const fn halved(a: i64) -> f64 => a / 2\n",
            "const fn kept(a: i64) -> i64 {\n",
            "    let half = halved(a)\n",
            "    return a\n",
            "}\n",
            "\n",
            "test written {\n",
            "    assert total(1, 2) == 3\n",
            "}\n",
            "\n",
            "test reaches_a_division {\n",
            "    assert kept(1) == 1\n",
            "}\n",
        ),
    )]);
    // NUM-1: `/` of two `i64` is core's candidate `div_`, which the
    // library these tests take does not define.
    let division =
        "demo.partial.hgl:4:35 the cpp target does not write the candidate `div_(i64, i64)` yet";
    assert_eq!(emitted.reported, [division]);
    assert_eq!(emitted.blocked("written"), None);
    assert_eq!(
        emitted.blocked("reaches_a_division").as_deref(),
        Some(division)
    );
    // The function that reaches the one not written is left out of the
    // unit with it, and the test that reaches them is left out of the
    // executable.
    let unit = &emitted
        .output
        .files
        .iter()
        .find(|file| file.path == "modules/demo.partial.cpp")
        .unwrap()
        .text;
    assert!(unit.contains("::hgraph::Int total("), "{unit}");
    assert!(!unit.contains("kept"), "{unit}");
    assert!(!unit.contains("halved"), "{unit}");
    assert!(!unit.contains("reaches_a_division"), "{unit}");
}

/// Temporal code over the scalars, as the target writes it: a graph and
/// its lifted calls (FN-6), a constant connection (FN-3), nodes with
/// state, handlers tested in order and capabilities (TIME-3, TIME-6,
/// CAP-2, CAP-3), a source (SOURCE-1), and the evals of them, dense and
/// timed, compared, one of them bounded by a `RunConfig` (TST-4 to TST-8,
/// CRUN-2). Everything is written, and each test
/// can be run, against the runtime library `hgl` carries.
#[test]
fn temporal_code_is_written() {
    let emitted = emit_with(
        &[(
            "demo.temporal.hgl",
            include_str!("fixtures/demo.temporal.hgl"),
        )],
        &hgl_cpp::runtime::provides,
    );
    assert_eq!(emitted.reported, Vec::<String>::new());
    assert_goldens("demo.temporal", &emitted.output);
    for test in &emitted.output.tests {
        assert_eq!(emitted.blocked(&test.name), None, "{}", test.name);
    }
    // CEVAL-1: core's `replay` and `record` are written in core's unit,
    // an instance for each type an eval replays and records; the test's
    // executable links it.
    let core = &emitted
        .output
        .files
        .iter()
        .find(|file| file.path == "modules/core.cpp")
        .unwrap()
        .text;
    for instance in [
        "::hgraph::wire<::hgl::core::m_::n_::i_::replay::of_i64::impl_>",
        "::hgraph::wire<::hgl::core::m_::n_::i_::record::of_i64::impl_>",
        "::hgraph::wire<::hgl::core::m_::n_::i_::record::of_datetime::impl_>",
    ] {
        assert!(core.contains(instance), "{instance} in\n{core}");
    }
    // hgraph issue 1866: a node whose configuration holds an `f64` is a
    // node of its own, `replay<f64>` among them; one with none is
    // interned, above.
    assert!(
        core.contains("add_unique_node(typeid(::hgl::core::m_::n_::i_::replay::of_f64::impl_)")
    );
    assert!(!core.contains("::hgraph::wire<::hgl::core::m_::n_::i_::replay::of_f64::impl_>"));
    // A temporal function's struct is `impl_`, in a namespace of the
    // function's name: one named as a member of it, `name` or `compose`,
    // is no member of the same name as its class.
    let unit = &emitted
        .output
        .files
        .iter()
        .find(|file| file.path == "modules/demo.temporal.cpp")
        .unwrap()
        .text;
    for function in ["name", "compose"] {
        assert!(
            unit.contains(&format!("namespace n_::{function}\n{{\n// {function}(")),
            "{function} in\n{unit}"
        );
        assert!(!unit.contains(&format!("struct {function}\n")), "{unit}");
    }
    let executable = &emitted.output.executables[0];
    let units: Vec<&str> = executable
        .units
        .iter()
        .map(|&unit| emitted.output.units[unit].source.as_str())
        .collect();
    assert!(units.contains(&"modules/core.cpp"), "{units:?}");
}

/// The activity of a node's inputs (TIME-9, END-5) and the temporal form
/// of an enum (TYPE-7, TYPE-16): `activate` and `passivate` in a handler,
/// `is_active` in a handler's condition and in its body, of two bindings
/// of one output, an enum published, read, replayed and wired as a
/// constant. Everything is written, and each test can be run.
#[test]
fn activity_and_enums_are_written() {
    let emitted = emit_with(
        &[(
            "demo.activity.hgl",
            include_str!("fixtures/demo.activity.hgl"),
        )],
        &hgl_cpp::runtime::provides,
    );
    assert_eq!(emitted.reported, Vec::<String>::new());
    assert_goldens("demo.activity", &emitted.output);
    for test in &emitted.output.tests {
        assert_eq!(emitted.blocked(&test.name), None, "{}", test.name);
    }
    let unit = &emitted
        .output
        .files
        .iter()
        .find(|file| file.path == "modules/demo.activity.cpp")
        .unwrap()
        .text;
    // END-5: "Activity belongs to each input binding": the input's own
    // hgraph view is changed and read.
    for written in [
        "::hglrt::fn_activate(data);",
        "::hglrt::fn_passivate(data);",
        "if (::hglrt::fn_modified(data) && ::hglrt::fn_is_active(data))",
        "(::hglrt::fn_is_active(driving) && (!::hglrt::fn_is_active(sampled)))",
        "::hgraph::In<\"mode\", ::hgraph::TS<::hgl::demo::activity::m_::Mode>> mode",
    ] {
        assert!(unit.contains(written), "{written} in\n{unit}");
    }
    // The library names an enum through what the compiler writes beside
    // it, and registers it with hgraph so (`values.h`).
    let header = &emitted
        .output
        .files
        .iter()
        .find(|file| file.path == "modules/demo.activity.h")
        .unwrap()
        .text;
    assert!(
        header.contains(
            "return {\"demo.activity::Mode\", {{Mode::listen, \"listen\"}, {Mode::ignore, \"ignore\"}}};"
        ),
        "{header}"
    );
}

/// A change of an input's activity in `stop` is reported: hgraph makes
/// passive at stop only the inputs a node declares active, so an input
/// it activated would stay subscribed. `is_active` there is written.
#[test]
fn a_change_of_activity_in_stop_is_reported() {
    let emitted = emit_with(
        &[(
            "m.hgl",
            concat!(
                "module m\n",
                "\n",
                "fn stopping(value: i64) -> i64 {\n",
                "    when { return value }\n",
                "    stop { passivate(value) }\n",
                "}\n",
                "\n",
                "fn asking(value: i64) -> i64 {\n",
                "    when { return value }\n",
                "    stop { let active = is_active(value) }\n",
                "}\n",
                "\n",
                "test a_change {\n",
                "    assert eval(stopping(value: [1])) == [1]\n",
                "}\n",
                "\n",
                "test a_read {\n",
                "    assert eval(asking(value: [1])) == [1]\n",
                "}\n",
            ),
        )],
        &hgl_cpp::runtime::provides,
    );
    assert_eq!(
        emitted.blocked("a_change").as_deref(),
        Some("m.hgl:5:12 the cpp target does not write `passivate` here yet")
    );
    assert_eq!(emitted.blocked("a_read"), None);
}

/// The windows' family (TYPE-11; core's END-9): a window of a tick count
/// is hgraph's `TSW` and one of a duration its `TSWDuration`; a window's
/// value, its delta and `removed_value` are read through the runtime
/// library's `windows.h`, each in the form its place takes (ERR-14); a
/// lifted helper is given a window only where it is all valid (FN-12);
/// and a trace of a window is one of its V, which core's `replay` and
/// `record` of the window publish and record (TST-16).
#[test]
fn windows_are_written() {
    let emitted = emit_with(
        &[(
            "demo.windows.hgl",
            include_str!("fixtures/demo.windows.hgl"),
        )],
        &hgl_cpp::runtime::provides,
    );
    assert_eq!(emitted.reported, Vec::<String>::new());
    assert_goldens("demo.windows", &emitted.output);
    for test in &emitted.output.tests {
        assert_eq!(emitted.blocked(&test.name), None, "{}", test.name);
    }
    let unit = &emitted
        .output
        .files
        .iter()
        .find(|file| file.path == "modules/demo.windows.cpp")
        .unwrap()
        .text;
    for written in [
        "::hgraph::Out<::hgraph::TSW<::hgraph::Int, 3, 2>> out_",
        "::hgraph::Out<::hgraph::TSWDuration<::hgraph::Int, 3, 1>> out_",
        "::hglrt::window_push(out_, ::hglrt::checked_value(reading));",
        "::hglrt::window_value_checked(readings)",
        "::hglrt::fn_removed_value_checked(readings)",
        "::hglrt::fn_delta_value_checked(readings)",
        "::hglrt::fn_all_valid(readings)",
        "::hgraph::In<\"readings\", ::hgraph::TSW<::hgraph::Int, 3, 2>, ::hgraph::InputValidity::AllValid> readings",
    ] {
        assert!(unit.contains(written), "{written} in\n{unit}");
    }
    // CEVAL-1, TST-16: core's `replay` of a window publishes each arrival,
    // a V, and its `record` keeps a V for each tick that has a delta.
    let core = &emitted
        .output
        .files
        .iter()
        .find(|file| file.path == "modules/core.cpp")
        .unwrap()
        .text;
    for written in [
        "using yielded_type_ = ::hglrt::Yielded<::hgraph::Int>;\n    using body_type_ = ::hglrt::generator<yielded_type_>;\n\n    // The body (SOURCE-1), given its configuration by value.\n    static body_type_ body_([[maybe_unused]] ::hglrt::list<std::tuple<::hgraph::DateTime, ::hgraph::Int>> items)",
        "[&out_](::hgraph::Int value_) { ::hglrt::window_push(out_, value_); }",
        "::hgraph::In<\"input\", ::hgraph::TSWDuration<::hgraph::Int, 3, 0>> input",
    ] {
        assert!(core.contains(written), "{written} in\n{core}");
    }
}

/// A window the target does not write yet is reported where its type is
/// written, and blocks what reaches it: one of a nullable V, whose delta
/// is the nullable's own (TYPE-11), and the invalidation of a window
/// result (END-4).
#[test]
fn a_window_not_written_yet_is_reported() {
    let emitted = emit_with(
        &[(
            "m.hgl",
            concat!(
                "module m\n",
                "\n",
                "fn kept(reading: f64) -> rolling_window<f64?, 3> {\n",
                "    when { return reading }\n",
                "}\n",
                "\n",
                "fn counted(readings: rolling_window<i64, 2>) -> i64 {\n",
                "    when { return delta_value(readings) }\n",
                "}\n",
                "\n",
                "test a_nullable_window {\n",
                "    assert eval(kept(reading: [1.0, 2.0])) == [1.0, 2.0]\n",
                "}\n",
                "\n",
                "fn cleared(reading: i64) -> rolling_window<i64, 2> {\n",
                "    inject out\n",
                "    when { invalidate(out) }\n",
                "}\n",
                "\n",
                "test a_window {\n",
                "    assert eval(counted(readings: [1, 2])) == [1, 2]\n",
                "}\n",
                "\n",
                "test an_invalidated_window {\n",
                "    assert eval(cleared(reading: [1])) == []\n",
                "}\n",
            ),
        )],
        &hgl_cpp::runtime::provides,
    );
    assert_eq!(
        emitted.blocked("a_nullable_window").as_deref(),
        Some("m.hgl:12:12 the cpp target does not write the type `rolling_window<f64?, 3>` yet")
    );
    assert!(
        emitted.reported.contains(
            &"m.hgl:3:4 the cpp target does not write a temporal value of `rolling_window<f64?, 3>` yet"
                .to_string()
        ),
        "{:?}",
        emitted.reported
    );
    assert_eq!(emitted.blocked("a_window"), None);
    // END-4 of a window result.
    assert_eq!(
        emitted.blocked("an_invalidated_window").as_deref(),
        Some("m.hgl:17:12 the cpp target does not write `invalidate` of a window yet")
    );
}

/// FN-16, TIME-10: a `ref` parameter is hgraph's `REF` input and a `ref`
/// result its `REF` output, which the wiring function gives as a
/// connection of T; returning a whole `ref` parameter publishes what it
/// designates; an input that is no `ref` parameter, designated, is given
/// to the node again as a `REF` input it follows, and its state keeps
/// which one the result follows. Everything is written, and each test can
/// be run.
#[test]
fn references_are_written() {
    let emitted = emit_with(
        &[(
            "demo.references.hgl",
            include_str!("fixtures/demo.references.hgl"),
        )],
        &hgl_cpp::runtime::provides,
    );
    assert_eq!(emitted.reported, Vec::<String>::new());
    assert_goldens("demo.references", &emitted.output);
    for test in &emitted.output.tests {
        assert_eq!(emitted.blocked(&test.name), None, "{}", test.name);
    }
    let unit = &emitted
        .output
        .files
        .iter()
        .find(|file| file.path == "modules/demo.references.cpp")
        .unwrap()
        .text;
    for written in [
        // A `ref` parameter, passive where no handler subscribes to it.
        "::hgraph::In<\"low\", ::hgraph::REF<::hgraph::TS<::hgraph::Int>>, ::hgraph::InputActivity::Passive> low",
        // FN-16: "Returning a whole `ref` parameter samples its current
        // designation when the `return` executes".
        "out_.set(::hglrt::designation(high));",
        // "a `ref` result is a connection of T to whoever reads it".
        "(wiring_, flag, low, high).as<::hgraph::TS<::hgraph::Int>>()",
        // An input designated is given again, and followed.
        "(wiring_, flag, low, high, low, high).as<::hgraph::TS<::hgraph::Int>>()",
        "::hgraph::In<\"designated:low\", ::hgraph::REF<::hgraph::TS<::hgraph::Int>>, ::hgraph::InputValidity::Unchecked> designated_01_",
        "if (designates_ == 0 && designated_01_.modified())",
    ] {
        assert!(unit.contains(written), "{written} in\n{unit}");
    }
}

/// FN-16: a designation of a position of a list is reported, after the
/// list's own temporal form where that is not written yet, so that a case
/// is listed for the first form it reaches.
#[test]
fn a_designation_not_written_yet_is_reported() {
    let emitted = emit_with(
        &[(
            "m.hgl",
            concat!(
                "module m\n",
                "\n",
                "fn route(idx: i64, items: list<i64, 3>) -> ref i64 {\n",
                "    when modified(idx) { return items[idx] }\n",
                "}\n",
            ),
        )],
        &hgl_cpp::runtime::provides,
    );
    assert_eq!(
        emitted.reported,
        ["m.hgl:4:33 the cpp target does not write a temporal value of `list<i64, 3>` yet"]
    );
}

/// The run's store where a graph is built (FN-9) and where an application
/// sets it (UNIT-14), and the applications themselves: each a function in
/// its module's unit and an entry unit of its own, whose executable links
/// what it calls (RUN-1, RUN-3).
#[test]
fn the_store_and_applications_are_written() {
    let emitted = emit_with(
        &[
            ("demo.apps.hgl", include_str!("fixtures/demo.apps.hgl")),
            (
                "demo.apps.backtest.hgl",
                include_str!("fixtures/demo.apps.backtest.hgl"),
            ),
            (
                "demo.apps.live.hgl",
                include_str!("fixtures/demo.apps.live.hgl"),
            ),
        ],
        &hgl_cpp::runtime::provides,
    );
    assert_eq!(emitted.reported, Vec::<String>::new());
    assert_goldens("demo.apps", &emitted.output);
    assert_eq!(
        emitted.blocked("the_store_is_set_where_the_graph_is_built"),
        None
    );
    let applications = &emitted.output.applications;
    assert_eq!(applications.len(), 2);
    for (application, real_time) in applications.iter().zip([false, true]) {
        assert!(application.blocked.is_none(), "{}", application.path);
        assert_eq!(application.real_time, real_time, "{}", application.path);
        assert_eq!(
            application.source,
            Some(format!("applications/{}.cpp", application.path))
        );
        // Linked from its own module's unit, the entry's and core's.
        let units: Vec<&str> = application
            .units
            .iter()
            .map(|&unit| emitted.output.units[unit].source.as_str())
            .collect();
        for unit in [
            "modules/core.cpp",
            "modules/demo.apps.cpp",
            &format!("modules/{}.cpp", application.path),
        ] {
            assert!(units.contains(&unit), "{unit} in {units:?}");
        }
    }
}

/// UNIT-14, EXPR-2: an application's store is set first, in the order
/// written; then its entry's arguments, the defaults of those it leaves
/// out and its configuration, each held in the order written, as its HGL
/// type, where more than one may fail, before the run.
#[test]
fn an_applications_parts_are_evaluated_in_the_order_written() {
    let emitted = emit_with(
        &[
            ("demo.apps.hgl", include_str!("fixtures/demo.apps.hgl")),
            (
                "demo.apps.ordered.hgl",
                concat!(
                    "application demo.apps.ordered\n",
                    "use demo.apps::{ticks}\n",
                    "const step: i64 = 2\n",
                    "inject global_state\n",
                    "put(global_state, \"first\", 6 // step)\n",
                    "eval(ticks(values: [6 // step]), RunConfig(length: step * 2s))\n",
                ),
            ),
        ],
        &hgl_cpp::runtime::provides,
    );
    assert_eq!(emitted.reported, Vec::<String>::new());
    let unit = &emitted
        .output
        .files
        .iter()
        .find(|file| file.path == "modules/demo.apps.ordered.cpp")
        .unwrap()
        .text;
    let at = |text: &str| {
        unit.find(text)
            .unwrap_or_else(|| panic!("{text} in\n{unit}"))
    };
    let values = "const ::hglrt::list<::hgraph::Int> argument_00_ = ";
    // `every`, left out: its default, `1s`.
    let every = "const ::hgraph::TimeDelta argument_01_ = ::hgraph::TimeDelta{1000000};";
    assert!(at("::hglrt::application::Store store_;") < at("put_GlobalState_str_V"));
    assert!(at("put_GlobalState_str_V") < at(values));
    assert!(at(values) < at(every));
    assert!(at(every) < at("const ::hglrt::testing::Bounds bounds_ = "));
    // The entry, wired in the run, is given what was held.
    assert!(at("w_::ticks(wiring_, argument_00_, argument_01_)") > at("bounds_ = "));
}

/// An application that needs a form the target does not write yet is
/// written with no entry unit, and says what it lacks.
#[test]
fn an_application_that_needs_what_is_not_written_is_blocked() {
    let emitted = emit_with(
        &[(
            "m.app.hgl",
            concat!(
                "application m.app\n",
                "\n",
                "fn first(values: list<i64>) -> list<i64> {\n",
                "    when { return values }\n",
                "}\n",
                "\n",
                "fn listed() -> list<i64> => first([1, 2])\n",
                "\n",
                "eval(listed())\n",
            ),
        )],
        &hgl_cpp::runtime::provides,
    );
    let application = &emitted.output.applications[0];
    let blocked = application.blocked.as_ref().expect("it is blocked");
    // The first form it reaches, in the entry or in core's `record` of
    // its result.
    assert_eq!(
        blocked.message,
        "the cpp target does not write a temporal value of `list<i64>` yet"
    );
    assert_eq!(application.source, None);
    assert!(
        emitted
            .output
            .files
            .iter()
            .all(|file| !file.path.starts_with("applications/"))
    );
}

/// A temporal form the target does not write yet is reported through the
/// dispatch point, where it is written, and blocks what reaches it: a
/// temporal value of a type no family writes, and a capability no family
/// makes. A test that reaches neither is run.
/// The timing the target writes (`emit/scheduling.rs`): nodes woken by a
/// scheduler or an alarm, and sources with declarations and hooks, each
/// as the goldens under `tests/goldens/demo.timing` have it. Each test of
/// the program can be run, which `hgl_cli`'s tests do on the SDK.
#[test]
fn timing_is_written() {
    let emitted = emit_with(
        &[("demo.timing.hgl", include_str!("fixtures/demo.timing.hgl"))],
        &hgl_cpp::runtime::provides,
    );
    assert_eq!(emitted.reported, Vec::<String>::new());
    assert_goldens("demo.timing", &emitted.output);
    for test in &emitted.output.tests {
        assert_eq!(emitted.blocked(&test.name), None, "{}", test.name);
    }
    let unit = &emitted
        .output
        .files
        .iter()
        .find(|file| file.path == "modules/demo.timing.cpp")
        .unwrap()
        .text;
    // TIME-9: what woke a node is read once, as it is entered: the
    // scheduler's due wake, or for an alarm that no input ticked.
    assert!(unit.contains("const bool timer_woke_ = scheduler.is_scheduled_now();"));
    assert!(unit.contains("const bool timer_woke_ = !(::hglrt::ticked(value));"));
    // SOURCE-1, TIME-8: a source's body is lent each capability it reads
    // for one resumption.
    assert!(unit.contains("::hglrt::Lending<::hglrt::Clock>"));
}

#[test]
fn a_temporal_form_not_written_yet_is_reported() {
    let emitted = emit_with(
        &[(
            "m.hgl",
            concat!(
                "module m\n",
                "\n",
                "fn first(values: list<i64>) -> i64 {\n",
                "    when { return 1 }\n",
                "}\n",
                "\n",
                "fn later(value: i64) -> i64 {\n",
                "    inject scheduler\n",
                "    start { schedule(scheduler, 0s) }\n",
                "    when { return value }\n",
                "}\n",
                "\n",
                "fn passed(value: i64) -> i64 {\n",
                "    when { return delta_value(value) }\n",
                "}\n",
                "\n",
                "test a_list {\n",
                "    assert eval(first(values: [[1, 2]])) == [1]\n",
                "}\n",
                "\n",
                "test a_scheduler {\n",
                "    assert eval(later(value: [1])) == [1]\n",
                "}\n",
                "\n",
                "test a_scalar {\n",
                "    assert eval(passed(value: [7, _, 7, 9])) == [7, _, 7, 9]\n",
                "}\n",
            ),
        )],
        &hgl_cpp::runtime::provides,
    );
    assert_eq!(
        emitted.blocked("a_list").as_deref(),
        Some("m.hgl:18:31 the cpp target does not write a temporal value of `list<i64>` yet")
    );
    // CAP-4: the scheduler is written (`scheduling`).
    assert_eq!(emitted.blocked("a_scheduler"), None);
    assert_eq!(emitted.blocked("a_scalar"), None);
}

/// EXPR-2: an eval's call, the defaults of the `const` parameters it
/// leaves out among it, is evaluated before the run's configuration:
/// where they are held, each default is held before the configuration.
#[test]
fn an_evals_defaults_are_held_before_its_configuration() {
    let emitted = emit_with(
        &[(
            "m.hgl",
            concat!(
                "module m\n",
                "\n",
                "const fn one() -> i64 => 1\n",
                "\n",
                "const fn span() -> duration => 2us\n",
                "\n",
                "fn scaled(value: i64, const by: i64 = one()) -> i64 {\n",
                "    when { return value * by }\n",
                "}\n",
                "\n",
                "test bounded {\n",
                "    assert eval(scaled(value: [1, 2, 3]), RunConfig(length: span())) == [1, 2]\n",
                "}\n",
            ),
        )],
        &hgl_cpp::runtime::provides,
    );
    assert_eq!(emitted.blocked("bounded"), None);
    let unit = &emitted
        .output
        .files
        .iter()
        .find(|file| file.path == "modules/m.cpp")
        .unwrap()
        .text;
    let test = unit.split("void bounded()").nth(1).unwrap();
    let default = test
        .find("= ::hgl::m::m_::one();")
        .expect("the default held");
    let bounds = test
        .find("= ::hglrt::testing::Bounds{.length = ::hgl::m::m_::span()};")
        .expect("the configuration held");
    assert!(default < bounds, "{test}");
}

/// SOURCE-1: "A `let` there holds a working value": a `let` of a block's
/// value in a source's body is written, its block's own `return` left to
/// the lambda it is written as.
#[test]
fn a_sources_value_block_is_written() {
    let emitted = emit_with(
        &[(
            "m.hgl",
            concat!(
                "module m\n",
                "\n",
                "fn counted(const n: i64) -> i64 {\n",
                "    let next = {\n",
                "        let one = 1\n",
                "        n + one\n",
                "    }\n",
                "    yield 1us: next\n",
                "}\n",
                "\n",
                "test counts {\n",
                "    assert eval(counted(n: 2)) == [_, 3]\n",
                "}\n",
            ),
        )],
        &hgl_cpp::runtime::provides,
    );
    assert_eq!(emitted.reported, Vec::<String>::new());
    assert_eq!(emitted.blocked("counts"), None);
}

/// TIME-6, TIME-8: a node's capability is made in `start` before a
/// `cache` initializer that reads it.
#[test]
fn a_capability_is_made_before_the_initializers_that_read_it() {
    let emitted = emit_with(
        &[(
            "m.hgl",
            concat!(
                "module m\n",
                "\n",
                "fn opened(value: i64) -> datetime {\n",
                "    inject clock\n",
                "    cache opening: datetime = clock.evaluation_time\n",
                "    when { return opening }\n",
                "}\n",
                "\n",
                "test opens {\n",
                "    assert eval(opened(value: [1])) == [earliest_start]\n",
                "}\n",
            ),
        )],
        &hgl_cpp::runtime::provides,
    );
    assert_eq!(emitted.blocked("opens"), None);
    let unit = &emitted
        .output
        .files
        .iter()
        .find(|file| file.path == "modules/m.cpp")
        .unwrap()
        .text;
    let start = unit
        .split("static void start(")
        .nth(1)
        .expect("the node's start");
    let start = start.split("static void eval(").next().unwrap();
    let made = start
        .find("const ::hglrt::Clock clock")
        .expect("the clock made");
    let read = start
        .find("opening = (clock).evaluation_time();")
        .expect("the cache initialized");
    assert!(made < read, "{start}");
}

/// TST-5: a trace given to an eval that is no trace written out, a `let`
/// of an eval's, is reported where it is given, and not wired as a
/// constant of its payload.
#[test]
fn a_trace_held_in_a_let_given_to_an_eval_is_reported() {
    let emitted = emit_with(
        &[(
            "m.hgl",
            concat!(
                "module m\n",
                "\n",
                "fn pass(value: i64) -> i64 {\n",
                "    when { return value }\n",
                "}\n",
                "\n",
                "test again {\n",
                "    let seen = eval(pass(value: [1]))\n",
                "    assert eval(pass(value: seen)) == [1]\n",
                "}\n",
            ),
        )],
        &hgl_cpp::runtime::provides,
    );
    assert_eq!(
        emitted.blocked("again").as_deref(),
        Some(
            "m.hgl:9:29 the cpp target does not write a trace given to an eval that is no trace written out yet"
        )
    );
}

/// A temporal call that leaves out `const` parameters (UNIT-8): where
/// more than one default may fail, each is held in the order of the
/// parameters (EXPR-2); a default that runs statements is reported, as a
/// value call's is; and a lifted call that leaves one out is reported,
/// since its defaults bind as configuration where the graph is built
/// (FN-6).
#[test]
fn a_temporal_calls_defaults_are_held_in_order_or_reported() {
    let emitted = emit_with(
        &[(
            "m.hgl",
            concat!(
                "module m\n",
                "\n",
                "const fn one() -> i64 => 1\n",
                "\n",
                "fn scaled(value: i64, const by: i64 = one(), const plus: i64 = one()) -> i64 {\n",
                "    when { return value * by + plus }\n",
                "}\n",
                "\n",
                "fn twice(value: i64) -> i64 => scaled(value)\n",
                "\n",
                "fn blocky(value: i64, const by: i64 = {\n",
                "    let b = 2\n",
                "    b + 1\n",
                "}) -> i64 {\n",
                "    when { return value * by }\n",
                "}\n",
                "\n",
                "fn blocked(value: i64) -> i64 => blocky(value)\n",
                "\n",
                "const fn add(a: i64, b: i64 = 1) -> i64 => a + b\n",
                "\n",
                "fn lifted_default(value: i64) -> i64 => add(value)\n",
                "\n",
                "test ordered {\n",
                "    assert eval(twice(value: [1])) == [2]\n",
                "}\n",
                "\n",
                "test block {\n",
                "    assert eval(blocked(value: [1])) == [3]\n",
                "}\n",
                "\n",
                "test lifted {\n",
                "    assert eval(lifted_default(value: [1])) == [2]\n",
                "}\n",
            ),
        )],
        &hgl_cpp::runtime::provides,
    );
    assert_eq!(emitted.blocked("ordered"), None);
    let unit = &emitted
        .output
        .files
        .iter()
        .find(|file| file.path == "modules/m.cpp")
        .unwrap()
        .text;
    // Both defaults call `one`, and each may fail: each is held, `by`
    // before `plus`, before the wiring function is called.
    let twice = unit
        .split("namespace n_::twice")
        .nth(1)
        .expect("the graph `twice`");
    assert!(
        twice.contains(
            "[&] { const auto argument_00_ = value; const ::hgraph::Int argument_01_ = ::hgl::m::m_::one(); const ::hgraph::Int argument_02_ = ::hgl::m::m_::one(); return ::hgl::m::m_::w_::scaled(wiring_, argument_00_, argument_01_, argument_02_); }()"
        ),
        "{twice}"
    );
    assert!(
        emitted.blocked("block").is_some_and(
            |reason| reason.ends_with("does not write a default that runs statements yet")
        ),
        "{:?}",
        emitted.blocked("block")
    );
    assert!(
        emitted.blocked("lifted").is_some_and(|reason| reason
            .ends_with("does not write a lifted call that leaves out a parameter yet")),
        "{:?}",
        emitted.blocked("lifted")
    );
}

/// UNIT-8: a default is written where its argument is left out; one
/// that calls its own function with that argument left out again would
/// be written inside itself without end, and is reported.
#[test]
fn a_default_that_leaves_itself_out_is_reported() {
    let emitted = emit(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "const fn again(value: bool = again()) -> bool => value\n",
            "const fn uses() -> bool => again()\n",
            "test calls {\n",
            "    assert uses()\n",
            "}\n",
        ),
    )]);
    let reported = "m.hgl:2:30 the cpp target does not write a default that leaves itself out yet";
    assert_eq!(emitted.reported, [reported]);
    assert_eq!(emitted.blocked("calls").as_deref(), Some(reported));
}

/// A unit holds every function of its module that is written, and is
/// linked whole: an executable is linked with every unit its own calls
/// into, though no test calls the function that does.
#[test]
fn an_executable_links_what_its_units_call() {
    let emitted = emit(&[
        (
            "demo.b.hgl",
            "module demo.b\nexport const fn one() -> i64 => 1\n",
        ),
        (
            "demo.a.hgl",
            concat!(
                "module demo.a\n",
                "use demo.b::{one}\n",
                "const fn unused() -> i64 => one()\n",
                "const fn yes() -> bool => true\n",
                "test holds {\n",
                "    assert yes()\n",
                "}\n",
            ),
        ),
    ]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let output = &emitted.output;
    assert_eq!(output.executables.len(), 1);
    let units: Vec<&str> = output.executables[0]
        .units
        .iter()
        .map(|&unit| output.units[unit].source.as_str())
        .collect();
    assert_eq!(units, ["modules/demo.a.cpp", "modules/demo.b.cpp"]);
}

/// EXPR-2: arguments are evaluated in the order written, and then the
/// defaults of those left out. C++ promises no order for a call's
/// arguments, so where more than one may fail each is a statement of
/// its own, in that order, before the call.
#[test]
fn arguments_that_may_fail_are_evaluated_in_order() {
    let emitted = emit(&[(
        "demo.order.hgl",
        concat!(
            "module demo.order\n",
            "const fn one() -> i64 => 1\n",
            "const fn pair(a: i64, b: i64 = one()) -> i64 => a\n",
            "const fn ordered() -> i64 => pair(one())\n",
            "const fn direct() -> i64 => pair(2)\n",
        ),
    )]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let unit = &emitted
        .output
        .files
        .iter()
        .find(|file| file.path == "modules/demo.order.cpp")
        .unwrap()
        .text;
    let one = "::hgl::demo::order::m_::one()";
    let pair = "::hgl::demo::order::m_::pair";
    assert!(
        unit.contains(&format!(
            "return [&] {{ const ::hgraph::Int argument_00_ = {one}; \
             const ::hgraph::Int argument_01_ = {one}; \
             return {pair}(argument_00_, argument_01_); }}();"
        )),
        "{unit}"
    );
    // One argument that may fail, the default, needs no order.
    assert!(
        unit.contains(&format!("return {pair}(2, {one});")),
        "{unit}"
    );
}

/// The natives `demo.values` reaches: those of [`provides`], `i64`'s
/// difference, product, negation and floor remainder (NUM-1, NUM-2),
/// the order and the equality of one type (OPS-2), and `append`
/// (LIST-1).
fn provides_values(name: &str) -> bool {
    provides(name)
        || [
            "sub__i64_i64",
            "mul__i64_i64",
            "neg__i64",
            "mod__i64_i64",
            "floordiv__i64_i64",
            "lt__T_T",
            "le__T_T",
            "gt__T_T",
            "eq__T_T",
            "fn_append",
        ]
        .contains(&name)
}

/// Value code over the scalars and the plain composites, as the target
/// writes it: constants (UNIT-13), an enum (TYPE-7), `if`, `switch`,
/// `while` and `for` (CTRL-6, CTRL-10, CTRL-12), a place of a list
/// (EXPR-3), defaults and a `T?` left out (UNIT-8), a generic function
/// by instance (GEN-2), a tuple, a result and `?` (ERR-2, ERR-5), a
/// `var` parameter (FN-15), and `assert raises` (TST-10). Everything is
/// written, and each test can be run.
#[test]
fn value_code_is_written() {
    let emitted = emit_with(
        &[("demo.values.hgl", include_str!("fixtures/demo.values.hgl"))],
        &provides_values,
    );
    assert_eq!(emitted.reported, Vec::<String>::new());
    assert_goldens("demo.values", &emitted.output);
    for test in &emitted.output.tests {
        assert_eq!(emitted.blocked(&test.name), None, "{}", test.name);
    }
}

/// The natives `demo.natives` reaches, by the names the runtime library
/// gives them.
fn provides_natives(name: &str) -> bool {
    provides(name)
        || [
            "div__f64_f64",
            "convert_i64",
            "convert_f64_Rounding_i64_",
            "eq__T_T",
            "eq__f64_f64_f64_f64",
        ]
        .contains(&name)
}

/// Core's native candidates as the target calls them: a generic one's
/// arguments are its template's, a zero policy (NUM-3) and the type of
/// an equality (OPS-2); a candidate of `convert`, whose candidates are
/// told apart by what they give (OPS-4), that takes the value alone is
/// given that type first, and the one that takes options is given its
/// `invalid` policy.
#[test]
fn natives_are_called_with_their_templates() {
    let emitted = emit_with(
        &[(
            "demo.natives.hgl",
            include_str!("fixtures/demo.natives.hgl"),
        )],
        &provides_natives,
    );
    assert_eq!(emitted.reported, Vec::<String>::new());
    assert_goldens("demo.natives", &emitted.output);
    assert_eq!(emitted.blocked("natives_are_called"), None);
}

/// FN-15: a place held for a `var` parameter could be one that a later
/// argument, running statements of its own, replaces or grows; such a
/// call is reported, and not written.
#[test]
fn a_place_beside_an_argument_that_runs_statements_is_reported() {
    let emitted = emit(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "const fn bump(var count: i64, by: i64) {\n",
            "    count = by\n",
            "}\n",
            "const fn grown() -> i64 {\n",
            "    var items = [10]\n",
            "    bump(items[0], {\n",
            "        items = [20, 30]\n",
            "        1\n",
            "    })\n",
            "    return 0\n",
            "}\n",
        ),
    )]);
    assert_eq!(
        emitted.reported,
        [
            "m.hgl:7:5 the cpp target does not write a call that gives a `var` parameter a place, beside an argument that runs statements yet"
        ]
    );
}

/// ERR-5, EXPR-2: a `?` is run before its statement, so one after a read
/// in its statement would run before the read, and could change what it
/// reads; it is reported, and not written.
#[test]
fn a_propagation_after_a_read_is_reported() {
    let emitted = emit(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "const fn checked(value: i64) -> result<i64, error> => ok(value)\n",
            "const fn pick(a: i64, b: i64) -> i64 => a\n",
            "const fn first() -> result<i64, error> {\n",
            "    var x = 1\n",
            "    return ok(pick(x, {\n",
            "        x = 2\n",
            "        checked(5)\n",
            "    }?))\n",
            "}\n",
        ),
    )]);
    assert_eq!(
        emitted.reported,
        ["m.hgl:6:23 the cpp target does not write a `?` after a read in its statement yet"]
    );
}

/// UNIT-13, EXPR-2: a constant's value is made where it is first read, so
/// a read of one whose value may fail is something that may fail: a `?`
/// after it in its statement would run first, and is reported.
#[test]
fn a_propagation_after_a_constant_that_may_fail_is_reported() {
    let emitted = emit(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "const broken: i64 = [1][2]\n",
            "const fn checked(value: i64) -> result<i64, error> => ok(value)\n",
            "const fn pick(a: i64, b: i64) -> i64 => a\n",
            "const fn both() -> result<i64, error> => ok(pick(broken, checked(11)?))\n",
        ),
    )]);
    assert_eq!(
        emitted.reported,
        ["m.hgl:5:58 the cpp target does not write a `?` here yet"]
    );
}

/// EXPR-3, EXPR-13: a compound assignment reads its place first, and a
/// read of a list's element may fail (EXPR-7), so a `?` in its value
/// would run before that read; it is reported.
#[test]
fn a_propagation_in_a_compound_assignment_to_an_element_is_reported() {
    let emitted = emit(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "const fn checked(value: i64) -> result<i64, error> => ok(value)\n",
            "const fn grown() -> result<list<i64>, error> {\n",
            "    var items = [1]\n",
            "    items[2] += checked(11)?\n",
            "    return ok(items)\n",
            "}\n",
        ),
    )]);
    assert_eq!(
        emitted.reported,
        ["m.hgl:5:17 the cpp target does not write a `?` here yet"]
    );
}

/// EXPR-2: a `var` argument's place is read first, and an element's
/// lookup is checked; a `?` in a later argument is reported.
#[test]
fn a_propagation_after_a_var_place_is_reported() {
    let emitted = emit(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "const fn checked(value: i64) -> result<i64, error> => ok(value)\n",
            "const fn bump(var count: i64, by: i64) {\n",
            "    count += by\n",
            "}\n",
            "const fn grown() -> result<list<i64>, error> {\n",
            "    var items = [1]\n",
            "    bump(items[2], checked(11)?)\n",
            "    return ok(items)\n",
            "}\n",
        ),
    )]);
    assert_eq!(
        emitted.reported,
        ["m.hgl:8:20 the cpp target does not write a `?` after a read in its statement yet"]
    );
}

/// UNIT-12: a `switch` case binds in the block around the `switch`, and
/// each case is a C++ block of its own: a binding of a case read after
/// the `switch` is reported, and not written.
#[test]
fn a_case_binding_read_after_its_switch_is_reported() {
    let emitted = emit(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "enum Side { buy, sell }\n",
            "const fn chosen_of(side: Side) -> i64 {\n",
            "    switch side {\n",
            "        case .buy:\n",
            "            let chosen = 1\n",
            "        default:\n",
            "            return 0\n",
            "    }\n",
            "    return chosen\n",
            "}\n",
        ),
    )]);
    assert_eq!(
        emitted.reported,
        ["m.hgl:4:5 the cpp target does not write a binding of a `switch` case, read after it yet"]
    );
}

/// UNIT-13, UNIT-8: the analyser keeps a constant's value and a default
/// with no locals of their own, so a block in one that runs statements is
/// reported, and not written (and the target does not fail on it).
#[test]
fn a_constant_or_a_default_that_runs_statements_is_reported() {
    let emitted = emit(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "const made: i64 = {\n",
            "    let a = 20\n",
            "    a + 1\n",
            "}\n",
            "const fn offset(x: i64, by: i64 = {\n",
            "    let b = 2\n",
            "    b + 1\n",
            "}) -> i64 => x + by\n",
            "const fn used() -> i64 => offset(1)\n",
        ),
    )]);
    assert_eq!(
        emitted.reported,
        [
            "m.hgl:2:7 the cpp target does not write a constant whose value runs statements yet",
            "m.hgl:6:35 the cpp target does not write a default that runs statements yet",
        ]
    );
}

/// A plain HGL name is written as it is, a GNU keyword or a header's macro
/// among them; each file `#undef`s the names it writes after its last
/// `#include`, and the standard's own words are escaped.
#[test]
fn names_a_platform_may_take_are_kept_and_undefined() {
    let emitted = emit(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "const fn issubnormal(typeof: i64) -> i64 => typeof\n",
            "const fn errno(x: i64) -> i64 => issubnormal(x)\n",
        ),
    )]);
    assert!(emitted.reported.is_empty(), "{:?}", emitted.reported);
    let header = &emitted
        .output
        .files
        .iter()
        .find(|file| file.path == "modules/m.h")
        .expect("the module's header")
        .text;
    assert!(
        header.contains("#include <hglrt/hglrt.h>\n\n// Names this file writes, which no macro of a header above takes.\n#undef issubnormal\n#undef m\n#undef typeof\n"),
        "{header}"
    );
    assert!(
        header.contains("::hgraph::Int issubnormal(::hgraph::Int typeof);"),
        "{header}"
    );
    assert!(
        header.contains("::hgraph::Int errno_(::hgraph::Int x);"),
        "{header}"
    );
}

/// TYPE-4: a `list<bool>` is a `std::vector<bool>`, which holds bits: an
/// element given to a `var` parameter, a `bool&`, is reported.
#[test]
fn a_bit_given_to_a_var_parameter_is_reported() {
    let emitted = emit(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "const fn set_flag(var flag: bool) {\n",
            "    flag = true\n",
            "}\n",
            "const fn flags_set() -> list<bool> {\n",
            "    var flags = [false, false]\n",
            "    set_flag(flags[1])\n",
            "    return flags\n",
            "}\n",
        ),
    )]);
    assert_eq!(
        emitted.reported,
        [
            "m.hgl:7:14 the cpp target does not write an element of a `list<bool>` given to a `var` parameter yet"
        ]
    );
}
