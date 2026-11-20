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

/// Core's conformance case `replay_preserves_equal_ticks` evaluates
/// nodes, which are not written yet: it is listed with the first form
/// it needs, and nothing of it is written.
#[test]
fn an_eval_is_not_written_yet() {
    let emitted = emit(&[(
        "scalar_eval.hgl",
        concat!(
            "module conformance part scalar_eval\n",
            "\n",
            "fn pass_i64(value: i64) -> i64 {\n",
            "    when { return delta_value(value) }\n",
            "}\n",
            "\n",
            "test replay_preserves_equal_ticks {\n",
            "    assert eval(pass_i64(value: [7, _, 7, 9])) == [7, _, 7, 9]\n",
            "}\n",
        ),
    )]);
    assert_eq!(
        emitted.blocked("replay_preserves_equal_ticks").as_deref(),
        Some("scalar_eval.hgl:8:12 the cpp target does not write an eval yet")
    );
    assert!(emitted.output.executables.is_empty());
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

/// FN-15, EXPR-2: a place held for a `var` parameter could be one that a
/// later argument, running statements of its own, replaces or grows: the
/// place's index is held in turn, and the place is taken where the call
/// is made.
#[test]
fn a_place_beside_an_argument_that_runs_statements_is_taken_at_the_call() {
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
    assert_eq!(emitted.reported, Vec::<String>::new());
    let unit = &emitted
        .output
        .files
        .iter()
        .find(|file| file.path == "modules/m.cpp")
        .unwrap()
        .text;
    assert!(
        unit.contains(
            "[&] { const ::hgraph::Int index_00_ = 0; const ::hgraph::Int argument_01_ = "
        ) && unit.contains(
            "return ::hgl::m::m_::bump(::hglrt::item(items, index_00_), argument_01_); }();"
        ),
        "{unit}"
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

/// Structs, sets, dictionaries and deltas as values (TYPE-3 to TYPE-6,
/// TYPE-14, TYPE-16, EXPR-3, ERR-6, CTRL-12), an enum's member made from a
/// value (TYPE-7), a zone and a time in it (TYPE-2), a `T?` compared with
/// a `T`, and a `var` place beside an argument that runs statements
/// (FN-15), as the target writes them, with the runtime library's
/// natives. Everything is written, and each test can be run. A struct is
/// in its module's types header, after the structs it holds.
#[test]
fn composite_values_are_written() {
    let emitted = emit_with(
        &[(
            "demo.composites.hgl",
            include_str!("fixtures/demo.composites.hgl"),
        )],
        &hgl_cpp::runtime::provides,
    );
    assert_eq!(emitted.reported, Vec::<String>::new());
    assert_goldens("demo.composites", &emitted.output);
    for test in &emitted.output.tests {
        assert_eq!(emitted.blocked(&test.name), None, "{}", test.name);
    }
    let types = &emitted
        .output
        .files
        .iter()
        .find(|file| file.path == "modules/demo.composites.types.h")
        .unwrap()
        .text;
    // UNIT-6: `Line` is declared before `Point`, which it holds.
    let point = types.find("\nstruct Point\n").unwrap();
    let line = types.find("\nstruct Line\n").unwrap();
    assert!(point < line, "{types}");
}

/// What the target does not write of the composites yet is reported
/// where it is written, and nothing is miswritten: a struct that holds
/// itself (TYPE-6), a field's default that runs statements of its own,
/// a field of an abstract struct assigned, a dictionary written with keys
/// that may be one, which the pinned text does not say what it holds, and
/// a dictionary's entry given to a `var` parameter.
#[test]
fn composite_forms_not_written_yet_are_reported() {
    let emitted = emit(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "struct Node {\n",
            "    value: i64\n",
            "    next: atomic<Node?> = null\n",
            "}\n",
            "struct Made {\n",
            "    value: i64 = {\n",
            "        let a = 20\n",
            "        a + 1\n",
            "    }\n",
            "}\n",
            "abstract struct Shape { id: i64 }\n",
            "struct Round : Shape { radius: f64 }\n",
            "const fn renumbered(shape: Shape) -> Shape {\n",
            "    var changed = shape\n",
            "    changed.id = 2\n",
            "    return changed\n",
            "}\n",
            "const fn bump(var count: i64, by: i64) {\n",
            "    count = by\n",
            "}\n",
            "const fn twice(name: str) -> dict<str, i64> => [name: 1, \"b\": 2]\n",
            "const fn given(book: dict<str, i64>) -> dict<str, i64> {\n",
            "    var changed = book\n",
            "    bump(changed[\"a\"], 1)\n",
            "    return changed\n",
            "}\n",
            "const fn made() -> i64 => Made().value\n",
            "test noded {\n",
            "    assert Node(value: 1).value == 1\n",
            "}\n",
        ),
    )]);
    let holds_itself = "m.hgl:2:8 the cpp target does not write a struct that holds itself yet";
    assert_eq!(
        emitted.reported,
        [
            holds_itself,
            "m.hgl:16:5 the cpp target does not write an assignment to this place yet",
            "m.hgl:22:48 the cpp target does not write a dictionary written with keys that may be one yet",
            "m.hgl:25:10 the cpp target does not write this place given to a `var` parameter yet",
            // Where the default is written, which a construction that
            // leaves the field out reaches.
            "m.hgl:7:18 the cpp target does not write a default that runs statements yet",
        ]
    );
    assert_eq!(emitted.blocked("noded").as_deref(), Some(holds_itself));
}

/// A module's struct that holds another module's, and an instance of a
/// generic function of the other module over it: each header includes
/// the types header of the module whose types it names, and no types
/// header includes a module's header (GEN-2, UNIT-6).
#[test]
fn a_struct_of_another_module_is_named_through_its_types_header() {
    let emitted = emit(&[
        (
            "demo.b.hgl",
            concat!(
                "module demo.b\n",
                "export struct Inner { value: i64 }\n",
                "export const fn same<T>(value: T) -> T => value\n",
            ),
        ),
        (
            "demo.a.hgl",
            concat!(
                "module demo.a\n",
                "use demo.b::{Inner, same}\n",
                "struct Outer { inner: Inner }\n",
                "const fn kept(outer: Outer) -> Outer => same(outer)\n",
            ),
        ),
    ]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let file = |path: &str| {
        &emitted
            .output
            .files
            .iter()
            .find(|file| file.path == path)
            .unwrap_or_else(|| panic!("no {path}"))
            .text
    };
    assert!(file("modules/demo.a.types.h").contains("#include \"demo.b.types.h\""));
    assert!(file("modules/demo.b.h").contains("#include \"demo.a.types.h\""));
    assert!(file("modules/demo.a.h").contains("#include \"demo.a.types.h\""));
    assert!(!file("modules/demo.b.types.h").contains("demo.a"));
}
