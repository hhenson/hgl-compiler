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
/// the core the compiler is built with, and writes the program as C++.
/// The program must be one the front end and the analyser accept.
fn emit(files: &[(&str, &str)]) -> Emitted {
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
    let output = transpile_for(&program, &provides, &mut diagnostics);
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

/// Compares every file `output` holds with the goldens of `name`, file
/// by file, and that no golden is left that the target no longer
/// writes; or, with `HGL_UPDATE_GOLDENS` set, writes them.
fn assert_goldens(name: &str, output: &Output) {
    let directory = goldens(name);
    if std::env::var_os("HGL_UPDATE_GOLDENS").is_some() {
        fs::remove_dir_all(&directory).ok();
        for file in &output.files {
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
    let mut written: Vec<String> = output.files.iter().map(|file| file.path.clone()).collect();
    written.sort();
    assert_eq!(
        written, kept,
        "the files written are the goldens' (HGL_UPDATE_GOLDENS=1 writes them again)"
    );
    for file in &output.files {
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
        .filter(|(_, line)| line.contains("::hglrt::statement(__LINE__)"))
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
    // NUM-1: `/` of two `i64` is a candidate with a generic `zero`
    // policy, which is not written yet.
    let division =
        "demo.partial.hgl:4:35 the cpp target does not write a call with generic arguments yet";
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
