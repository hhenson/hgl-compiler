//! The hgraph target against small programs: the Python it writes,
//! what it reports for a form it does not write yet, and which tests
//! can then still be run.
//!
//! The programs are files under `tests/fixtures`, each named after its
//! module: `demo.div` is `demo.div.hgl`. Those directly in `fixtures` are
//! written whole and their tests pass, and
//! `the_generated_tests_pass_on_hgraph` runs every one of them. A
//! directory in `fixtures` holds the programs of the one test it is named
//! after, or of the tests that name them by the constant it is named
//! after.

use std::fs;
use std::path::{Path, PathBuf};

use hgl_analyser::{ParsedFile, analyse};
use hgl_diagnostics::{Diagnostics, Location, render};
use hgl_hgraph::{
    Output, Ran, RanApplication, Stop, Test, run, run_application, run_within, transpile,
};
use hgl_lexer::{lex, literal};
use hgl_parser::{Entry, Limits, parse};
use hgl_source::SourceMap;

/// A program of `tests/fixtures`, as `emit` takes it: the name of its
/// file and its text. `fixture!("demo.div.hgl")` is a file directly in
/// `fixtures`, and `fixture!("reach", "reach.far.hgl")` one in a
/// directory of it.
// A macro, so that `include_str!` is given the file's path as a literal,
// and the program is part of the test program's build.
macro_rules! fixture {
    ($file:literal) => {
        ($file, include_str!(concat!("fixtures/", $file)))
    };
    ($directory:literal, $file:literal) => {
        (
            $file,
            include_str!(concat!("fixtures/", $directory, "/", $file)),
        )
    };
}

/// What the target makes of a program.
struct Emitted {
    /// The program's files, to say where a place is.
    sources: SourceMap,
    /// The files written, and the program's tests.
    output: Output,
    /// What the target reported, each as `file:line:column code:
    /// message`.
    reported: Vec<String>,
}

impl Emitted {
    /// The text of the file at `path`.
    fn text(&self, path: &str) -> &str {
        &self
            .output
            .files
            .iter()
            .find(|file| file.path == path)
            .unwrap_or_else(|| panic!("no file `{path}` was written"))
            .text
    }

    /// The text of the file the module `name` is written to: that of
    /// `demo.scaling` is `_hgl/_m_demo/_m_scaling/__init__.py`.
    fn module(&self, name: &str) -> &str {
        let parts: Vec<String> = name.split('.').map(|part| format!("_m_{part}")).collect();
        self.text(&format!("_hgl/{}/__init__.py", parts.join("/")))
    }

    /// The test named `name`, and its position among the tests.
    fn test(&self, name: &str) -> (usize, &Test) {
        self.output
            .tests
            .iter()
            .enumerate()
            .find(|(_, test)| test.name == name)
            .unwrap_or_else(|| panic!("no test `{name}` was written"))
    }

    /// A place as `file:line:column`.
    fn place(&self, at: Location) -> String {
        let source = self.sources.file(at.file);
        format!("{}:{}", source.name(), source.line_col(at.span.start))
    }

    /// What blocks the test named `name`, as `file:line:column
    /// message`.
    fn blocked(&self, name: &str) -> Option<String> {
        let (_, test) = self.test(name);
        test.blocked
            .as_ref()
            .map(|diagnostic| format!("{} {}", self.place(diagnostic.primary), diagnostic.message))
    }
}

/// Lexes, parses and analyses `files`, given as `(name, text)`, with the
/// core the compiler is built with, and writes the program for hgraph.
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
        parsed.push(ParsedFile {
            file,
            root: tree.syntax(),
            omit: Vec::new(),
        });
    }
    let program = analyse(&parsed, &mut diagnostics);
    assert!(
        !diagnostics.has_errors(),
        "the program is accepted:\n{}",
        render::to_string(&sources, &diagnostics)
    );
    let before = diagnostics.len();
    let output = transpile(&program, &mut diagnostics);
    let declared_tests: usize = program
        .modules
        .iter()
        .map(|module| module.tests.len())
        .sum();
    assert_eq!(
        output.tests.len(),
        declared_tests,
        "every analysed test has a target entry, including blocked tests"
    );
    let reported = diagnostics.as_slice()[before..]
        .iter()
        .map(|diagnostic| {
            let source = sources.file(diagnostic.primary.file);
            assert_eq!(diagnostic.category.as_str(), "build");
            assert!(hgl_hgraph::codes::ALL.contains(&diagnostic.code.as_str()));
            format!(
                "{}:{} {}: {}",
                source.name(),
                source.line_col(diagnostic.primary.span.start),
                diagnostic.code.as_str(),
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

/// [`emit`], for a program the target writes all of: nothing is
/// reported.
fn emit_clean(files: &[(&str, &str)]) -> Emitted {
    let emitted = emit(files);
    assert_eq!(emitted.reported, Vec::<String>::new());
    emitted
}

/// A check of the text of a file the target wrote.
trait Writes {
    /// Asserts that the text has each of `expected` in it.
    // `S: AsRef<str>` takes a list of `&str` and a list of `String` alike.
    fn assert_writes<S: AsRef<str>>(&self, expected: &[S]);
}

// A trait, so that the check is written `python.assert_writes(&[...])`:
// the text first, and the list as long as it needs.
impl Writes for str {
    fn assert_writes<S: AsRef<str>>(&self, expected: &[S]) {
        for expected in expected {
            let expected = expected.as_ref();
            assert!(
                self.contains(expected),
                "the module has:\n{expected}\nin:\n{self}"
            );
        }
    }
}

/// A module with each form the target writes: a value function, a node
/// with a `state` and a `const` parameter, a graph, a node with no
/// result, and a test that binds, compares and runs an eval.
const DEMO: (&str, &str) = fixture!("demo.scaling.hgl");

/// What the target writes for [`DEMO`].
const DEMO_PYTHON: &str = concat!(
    "# Generated by the hgl compiler for hgraph, from module `demo.scaling`. Do not edit.\n",
    "import builtins as _py\n",
    "import hgraph as _hg\n",
    "import _hgl._core_native as _core\n",
    "import _hgl._harness as _harness\n",
    "import _hgl._m_core as _hgl_4core\n",
    "\n",
    "\n",
    "def doubled(value):\n",
    "    return _core.mul__i64_i64(lhs=value, rhs=2)\n",
    "\n",
    "\n",
    "@_hg.compute_node\n",
    "def scaled(value: _hg.TS[_py.int], factor: _py.int, _state: _hg.STATE = None) -> _hg.TS[_py.int]:\n",
    "    _state.seen = _core.add__i64_i64(lhs=_state.seen, rhs=1)\n",
    "    if _core.gt__T_T(lhs=_state.seen, rhs=1):\n",
    "        return _core.mul__i64_i64(lhs=doubled(value=value.value), rhs=factor)\n",
    "    else:\n",
    "        return value.value\n",
    "\n",
    "\n",
    "@scaled.start\n",
    "def _start_scaled(factor: _py.int, _state: _hg.STATE = None):\n",
    "    _state.seen = 0\n",
    "\n",
    "\n",
    "@_hg.graph\n",
    "def wired(value: _hg.TS[_py.int]) -> _hg.TS[_py.int]:\n",
    "    return scaled(value=value, factor=3)\n",
    "\n",
    "\n",
    "@_hg.sink_node\n",
    "def sink(value: _hg.TS[_py.bool]):\n",
    "    pass\n",
    "\n",
    "\n",
    "def test_scales():\n",
    "    seen = _harness.evaluate(wired, {\"value\": _hgl_4core._inst_replay__i64}, _hgl_4core._inst_record__i64, value=[1, None, 2])\n",
    "    if not (_harness.same(seen, [1, None, 12])):\n",
    "        raise _py.AssertionError()\n",
    "    if not (not _harness.same(_harness.evaluate(scaled, {\"value\": _hgl_4core._inst_replay__i64}, _hgl_4core._inst_record__i64, value=[5], factor=2), [])):\n",
    "        raise _py.AssertionError()\n",
    "    _harness.evaluate(sink, {\"value\": _hgl_4core._inst_replay__bool}, None, value=[True])\n",
);

#[test]
fn a_module_is_written_as_python() {
    let emitted = emit_clean(&[DEMO]);
    let paths: Vec<&str> = emitted
        .output
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    assert_eq!(
        paths,
        [
            "_hgl/__init__.py",
            "_hgl/_m_demo/_m_scaling/__init__.py",
            // Core, whose `replay` and `record` an eval's run is wired
            // with.
            "_hgl/_m_core/__init__.py",
            "_hgl/_core_native.py",
            "_hgl/_core_support.py",
            "_hgl/_harness.py"
        ]
    );
    assert_eq!(emitted.module("demo.scaling"), DEMO_PYTHON);
}

/// The target fixture for temporal scalar inputs, state and dense traces.
const SCALAR_TRACES: (&str, &str) = fixture!("demo.scalar_traces.hgl");

/// A bare `when` (TIME-3), state initialization and updates (TIME-6),
/// a guarded scalar delta (END-3, FN-7) and dense traces (TST-6).
#[test]
fn dense_scalar_traces_are_written_as_hgraph_tests() {
    let emitted = emit_clean(&[SCALAR_TRACES]);
    let python = emitted.module("demo.scalar_traces");
    python.assert_writes(&[
        "def pass_i64(value: _hg.TS[_py.int]) -> _hg.TS[_py.int]:\n    return value.delta_value\n",
        "    if (a.valid and (a.modified and a.valid)):\n        return a.delta_value\n",
        "    _state.count = _core.add__i64_i64(lhs=_state.count, rhs=1)\n",
        "def _start_count_publications(_state: _hg.STATE = None):\n    _state.count = 0\n",
        "    if not (_harness.same(_harness.evaluate(pass_i64, {\"value\": _hgl_4core._inst_replay__i64}, _hgl_4core._inst_record__i64, value=[None, 7, None, None]), [None, 7])):\n",
        "        raise _py.AssertionError()\n",
        "    if not (_harness.same(_harness.evaluate(pass_str, {\"value\": _hgl_4core._inst_replay__str}, _hgl_4core._inst_record__str, value=[\"\", None, \"\", \"a\"]), [\"\", None, \"\", \"a\"])):\n",
        "        raise _py.AssertionError()\n",
    ]);
    assert_eq!(
        python.matches("\ndef test_").count(),
        emitted.output.tests.len(),
        "one function a test"
    );
}

/// Core's scalar endpoint cases that publish no null, and a node with
/// two handlers: `when` headers with selectors, and the node's own
/// output.
const ENDPOINTS: (&str, &str) = fixture!("demo.endpoints.hgl");

/// TIME-3: a modification selector subscribes its node and guards its handler.
/// A validity selector says which inputs the handler requires valid. TIME-4: "Aggregate subscriptions cover
/// every handler. Node-wide validity includes only requirements common
/// to executable handlers; other guards stay per-handler." TIME-7 and
/// END-4: `inject out` is the node's output, which `invalidate` clears.
#[test]
fn a_when_header_says_what_wakes_and_admits_a_node() {
    let emitted = emit_clean(&[ENDPOINTS]);
    let python = emitted.module("demo.endpoints");
    python.assert_writes(&[
        // `valid([])`: no input need be valid.
        "@_hg.compute_node(valid=())\ndef watched_valid(watched: _hg.TS[_py.int], probe: _hg.TS[_py.bool]) -> _hg.TS[_py.bool]:\n    return watched.valid\n",
        // `modified(probe) && valid()`: only `probe` wakes it.
        "@_hg.compute_node(active=(\"probe\",))\ndef sample_on_probe(",
        "@_hg.compute_node(active=(\"probe\",), valid=())\ndef read_modified(",
        // The node's own output.
        concat!(
            "def produce(command: _hg.TS[_py.int], _output: _hg.TS_OUT[_py.int] = None) -> _hg.TS[_py.int]:\n",
            "    if _core.ne__i64_i64_f64_f64(lhs=command.value, rhs=0, abs=0.0, rel=0.0):\n",
            "        return command.value\n",
            "    _output.invalidate()\n",
        ),
        // Two handlers: the node is woken by either input and requires
        // what both require; each is guarded by the rest of what it
        // asks, in the order written.
        concat!(
            "@_hg.compute_node(valid=(\"b\",))\n",
            "def two_handlers(a: _hg.TS[_py.int], b: _hg.TS[_py.int]) -> _hg.TS[_py.int]:\n",
            "    if a.valid and a.modified and _core.gt__T_T(lhs=a.value, rhs=5):\n",
            "        return a.value\n",
            "    if b.modified:\n",
            "        return b.value\n",
        ),
    ]);
}

/// END-6 and TIME-7: both an input and the node's own output keep a
/// timestamp only while valid. The stop hook receives the output it reads.
#[test]
fn last_modified_reads_valid_endpoint_timestamps_in_when_and_stop() {
    let emitted = emit_clean(&[fixture!(
        "last_modified_reads_valid_endpoint_timestamps_in_when_and_stop",
        "timestamps.hgl"
    )]);
    let python = emitted.module("timestamps");
    python.assert_writes(&[
        "    previous = (_output.last_modified_time if _output.valid else None)\n",
        "    return _core.published((value.last_modified_time if value.valid else None))\n",
        concat!(
            "@sample.stop\n",
            "def _stop_sample(value: _hg.TS[_py.int | None], _output: _hg.TS_OUT[_dt.datetime | None] = None):\n",
            "    final_input = (value.last_modified_time if value.valid else None)\n",
            "    final_output = (_output.last_modified_time if _output.valid else None)\n",
        ),
    ]);
}

/// Three modules: two that each export a `pick`, and one that has a
/// `pick` of its own, calls both, leaves a nullable argument out, and
/// gives a `state` its first value from a `const` parameter.
const MODULES: [(&str, &str); 3] = [
    fixture!("demo.left.hgl"),
    fixture!("demo.right.hgl"),
    fixture!("demo.configured.hgl"),
];

/// A function of another module is named through its module, so that
/// two modules' functions of one name, and this module's own, stay
/// apart. A lifecycle hook is a function of its own to hgraph, and is
/// given the node's `const` parameters. UNIT-8: a parameter written
/// `T?` may be left out, and is then null.
#[test]
fn names_configuration_and_omitted_arguments_are_kept() {
    let emitted = emit_clean(&MODULES);
    let python = emitted.module("demo.configured");
    python.assert_writes(&[
        "import _hgl._m_demo._m_left as _hgl_4demo_4left\nimport _hgl._m_demo._m_right as _hgl_4demo_5right\n",
        concat!(
            "def chosen(fallback):\n",
            "    if (fallback is None):\n",
            "        return _hgl_4demo_4left.pick()\n",
            "    return _core.add__i64_i64(lhs=_hgl_4demo_5right.pick(), rhs=pick())\n",
        ),
        concat!(
            "@kept.start\n",
            "def _start_kept(first: _py.int, _state: _hg.STATE = None):\n",
            "    _state.last = first\n",
        ),
        "    return _core.add__i64_i64(lhs=before, rhs=chosen(fallback=None))\n",
    ]);
}

/// Headers whose selectors add up, repeat an input, or name the node's
/// own output.
const SELECTORS: (&str, &str) = fixture!("demo.selectors.hgl");

/// TIME-3: every selector among a header's conjuncts adds what it
/// names, so `valid(a) && valid(b)` requires both. A subscription is a
/// set: an input named twice is one. The node's output named in a
/// header is hgraph's `_output`.
#[test]
fn a_headers_selectors_add_up() {
    let emitted = emit_clean(&[SELECTORS]);
    let python = emitted.module("demo.selectors");
    python.assert_writes(&[
        "@_hg.compute_node(valid=(\"a\", \"b\"))\ndef both_valid(",
        concat!(
            "@_hg.compute_node\n",
            "def repeated(a: _hg.TS[_py.int], b: _hg.TS[_py.int]) -> _hg.TS[_py.int]:\n",
        ),
        concat!(
            "    if _output.valid and _core.gt__T_T(lhs=command.value, rhs=0):\n",
            "        return command.value\n",
            "    return 1\n",
        ),
    ]);
}

/// Defaults: of a value function's parameter, where it calls another
/// function, and of a node's `const` parameter.
const DEFAULTS: (&str, &str) = fixture!("demo.defaults.hgl");

/// UNIT-8: a parameter with a default may be left out. The call is
/// written with the default in its place, which the analysed program
/// holds as a typed value.
#[test]
fn a_default_is_written_where_a_call_leaves_it_out() {
    let emitted = emit_clean(&[DEFAULTS]);
    let python = emitted.module("demo.defaults");
    python.assert_writes(&[
        // The function is always given the parameter.
        "def scale(value, factor):\n",
        "lhs=scale(value=3, factor=base()), rhs=6",
        "lhs=scale(value=3, factor=5), rhs=15",
        // A node's configuration, in a graph and in an eval.
        "def offset(value: _hg.TS[_py.int], by: _py.int) -> _hg.TS[_py.int]:\n",
        "    return offset(value=value, by=10)\n",
        "    if not (_harness.same(_harness.evaluate(offset, {\"value\": _hgl_4core._inst_replay__i64}, _hgl_4core._inst_record__i64, value=[1], by=10), [11])):\n",
        "        raise _py.AssertionError()\n",
        "    if not (_harness.same(_harness.evaluate(offset, {\"value\": _hgl_4core._inst_replay__i64}, _hgl_4core._inst_record__i64, value=[1], by=1), [2])):\n",
        "        raise _py.AssertionError()\n",
    ]);
}

/// A default written in one module and left out by a call in another,
/// whose caller has a parameter named as a function the default calls;
/// and a parameter written `T?` before one with a default.
const DEFAULT_MODULES: [(&str, &str); 2] = [fixture!("demo.lib.hgl"), fixture!("demo.caller.hgl")];

/// A default means what it means where it is declared (UNIT-7): written
/// in a caller of another module, a function it calls is named through
/// its own module, so a caller's `base` is not taken for it. Within one
/// module no binding takes a name already in scope (UNIT-12). A
/// parameter written `T?` that is left out is given `None` at the call,
/// so Python has no default to put in order.
#[test]
fn a_default_keeps_its_meaning_in_another_module() {
    let emitted = emit_clean(&DEFAULT_MODULES);
    let python = emitted.module("demo.caller");
    python.assert_writes(&[
        "def caller(base):\n",
        "_hgl_4demo_3lib.scale(value=3, factor=_hgl_4demo_3lib.base())",
        "_hgl_4demo_3lib.pick(first=None, second=1)",
        "_hgl_4demo_3lib.pick(first=7, second=5)",
    ]);
    let library = emitted.module("demo.lib");
    assert!(library.contains("def pick(first, second):\n"));
}

/// Two modules: constants, and their reads in another module.
const CONSTANTS: [(&str, &str); 2] = [fixture!("demo.constants.hgl"), fixture!("demo.reads.hgl")];

/// TYPE-2: "elapsed `duration` ... Instants/durations use signed 64-bit
/// microseconds". On this target a `duration` is a `timedelta`, a count
/// of whole microseconds, as a value and as what a connection ticks.
const DURATIONS: (&str, &str) = fixture!("demo.durations.hgl");

/// A `duration` is written as a `timedelta`: its literal by its count of
/// microseconds (LEX-13), its type in an annotation, and its operators
/// as the provider's functions (OPS-1).
#[test]
fn a_duration_is_a_timedelta() {
    let emitted = emit_clean(&[DURATIONS]);
    let python = emitted.module("demo.durations");
    python.assert_writes(&[
        "import datetime as _dt\n",
        "@_core.constant\ndef pause():\n    return _dt.timedelta(microseconds=5400000000)\n",
        "return _core.add__duration_duration(lhs=first, rhs=second)",
        "return _core.sub__duration_duration(lhs=first, rhs=second)",
        "return _core.mul__duration_i64(lhs=length, rhs=count)",
        "return _core.mul__duration_f64(lhs=length, rhs=by)",
        "_hg.TS[_dt.timedelta]",
    ]);
    // A module that names no duration does not import the library.
    let plain = emit(&[DEMO]);
    assert!(!plain.module("demo.scaling").contains("import datetime"));
}

/// OPS-1: a duration by an `i64` or an `f64` is a duration, by a duration
/// an `f64`, and `//` of two an `i64`. OPS-10: a duration result is "the
/// exact result rounded to the nearest duration the type holds, a tie to
/// the even one". NUM-12: each division takes its zero policy. NUM-1:
/// `//` rounds toward negative infinity.
const DIVIDED: (&str, &str) = fixture!("demo.durations.divided.hgl");

/// A division of durations is the provider's function, given its zero
/// policy by name as a division of numbers is.
#[test]
fn a_duration_divided_takes_its_zero_policy() {
    let emitted = emit_clean(&[DIVIDED]);
    let python = emitted.module("demo.durations.divided");
    python.assert_writes(&[
        "return _core.div__duration_i64(lhs=length, rhs=among, zero=\"fail\")",
        "return _core.div__duration_f64(lhs=length, rhs=by, zero=\"fail\")",
        "return _core.div__duration_duration(lhs=first, rhs=second, zero=\"ieee\")",
        "return _core.floordiv__duration_duration(lhs=first, rhs=second, zero=\"fail\")",
        "return _core.div__duration_i64(lhs=length, rhs=among, zero=\"error\")",
        "return _core.floordiv__duration_duration(lhs=first, rhs=second, zero=\"error\")",
    ]);
}

/// TYPE-2: a `date`, a `time`, a `datetime` and a `civil_datetime` are
/// Python's own, and an instant is a `datetime` in UTC that names no zone,
/// as hgraph has them. OPS-1: an instant and a duration, two instants, a
/// date and a time, and a date and a duration. Core's named times (its
/// CRUN-3) are constants of these types.
const INSTANTS: (&str, &str) = fixture!("demo.instants.hgl");

/// A temporal literal is written by its parts (LEX-11), and its type as
/// Python's (TYPE-2).
#[test]
fn a_date_a_time_and_an_instant_are_pythons_own() {
    let emitted = emit_clean(&[INSTANTS]);
    let python = emitted.module("demo.instants");
    python.assert_writes(&[
        "import datetime as _dt\n",
        "@_core.constant\ndef opened():\n    return _dt.datetime(2026, 9, 3, 9, 30, 0, 0)\n",
        "return _core.add__datetime_duration(lhs=at, rhs=by)",
        "return _core.sub__datetime_duration(lhs=at, rhs=by)",
        "return _core.sub__datetime_datetime(lhs=second, rhs=first)",
        "return _core.add__date_time(lhs=day, rhs=at)",
        "return _core.sub__date_date(lhs=second, rhs=first)",
        "return _core.add__date_duration(lhs=day, rhs=by)",
        "return _core.sub__date_duration(lhs=day, rhs=by)",
        // An offset other than UTC is the provider's to apply.
        "_core.instant(_dt.datetime(2026, 9, 3, 10, 30, 0, 0), 60)",
        "_dt.date(2026, 9, 3)",
        "_dt.time(9, 30, 0, 0)",
        "_hg.TS[_dt.datetime]",
    ]);
}

/// TST-10: "`assert raises(\"code\")` ... Run its block once. A matching
/// escaping error passes ...; then continue after the assertion."
const RAISES: (&str, &str) = fixture!("demo.raises.hgl");

/// TST-10: "Normal completion, a different code ... fails the test."
/// TST-11: "A failed nested assertion cannot satisfy an enclosing
/// raises."
const RAISES_FAILS: (&str, &str) = fixture!(
    "a_failure_names_the_statement_that_failed",
    "demo.raises.fails.hgl"
);

/// An `assert raises` is a `try` whose handler asks the harness whether
/// what escaped has the code, and whose `else` fails the test.
#[test]
fn an_assert_raises_is_a_try_the_harness_judges() {
    let emitted = emit_clean(&[RAISES]);
    let python = emitted.module("demo.raises");
    let expected = concat!(
        "    try:\n",
        "        strict(a=1.0, b=0.0)\n",
        "    except _py.Exception as _raised:\n",
        "        _harness.raised(_raised, \"numeric.division_by_zero\")\n",
        "    else:\n",
        "        _harness.not_raised(\"numeric.division_by_zero\")\n",
    );
    assert!(
        python.contains(expected),
        "the module has:\n{expected}\nin:\n{python}"
    );
    assert!(python.contains("import _hgl._harness as _harness\n"));
}

/// UNIT-13: "The constant has that one value wherever it is read ... It
/// is referenced forward as any module declaration is (UNIT-6)". A
/// constant is a function of no parameter that the provider runs once,
/// and a read is a call of it: in value code, in a graph, and from
/// another module.
#[test]
fn a_constant_is_a_function_run_once_and_a_read_calls_it() {
    let emitted = emit_clean(&CONSTANTS);
    let python = emitted.module("demo.constants");
    python.assert_writes(&[
        "@_core.constant\ndef scale():\n    return 2\n",
        "@_core.constant\ndef first():\n    return _core.add__i64_i64(lhs=_core.mul__i64_i64(lhs=3, rhs=late()), rhs=1)\n",
        "@_core.constant\ndef late():\n    return scale()\n",
        "@_core.constant\ndef side():\n    return Side[\"sell\"]\n",
        "def seed():\n    return first()\n",
    ]);
    let reads = emitted.module("demo.reads");
    reads.assert_writes(&[
        "@_core.constant\ndef twice():\n    return _core.mul__i64_i64(lhs=_hgl_4demo_9constants.scale(), rhs=2)\n",
        // In a graph a constant is a value known where it is built.
        "return scaled(value=value, factor=_hgl_4demo_9constants.scale())",
    ]);
    // The provider has the function that runs one once. Its text is a
    // file of this repository, whose line ends are the checkout's.
    assert!(
        emitted
            .text("_hgl/_core_support.py")
            .contains("def constant(function):")
    );
}

/// A default that calls its own function with that argument left out
/// would be written inside itself without end: it is reported.
#[test]
fn a_default_is_not_written_inside_itself() {
    let emitted = emit(&[fixture!("a_default_is_not_written_inside_itself", "m.hgl")]);
    assert_eq!(
        emitted.reported,
        [
            "m.hgl:2:29 target.unsupported: the hgraph target does not write a default that leaves itself out yet"
        ]
    );
}

/// Names that Python, hgraph or the target would take: each is given a
/// Python name of its own. FN-4: "Composition and eval choose the
/// temporal definition if one exists; runtime/value evaluation chooses
/// the value definition", so a pair of one name is two functions.
const NAMES: (&str, &str) = fixture!("demo.names.hgl");

#[test]
fn no_two_names_are_one_python_name() {
    let emitted = emit_clean(&[NAMES]);
    let python = emitted.module("demo.names");
    python.assert_writes(&[
        // Python's words, and one that is a word with a `_` after it.
        "def lambda_():\n    return 1\n",
        "def lambda__():\n    return 2\n",
        // A built-in name is no keyword: the target reaches Python's
        // own through `_py`, so a program's `float` and `frozenset` are
        // free.
        "def float():\n    return 6\n",
        "def sized(frozenset, int):\n    return _py.frozenset((frozenset, int))\n",
        // hgraph's `graph` is `_hg.graph` here, so a program's is free.
        "def graph():\n    return 3\n",
        // What starts as the target's own names do.
        "def hgl__kept():\n    return 4\n",
        "def hgl_test_me():\n    return 5\n",
        // A value and a temporal definition of one name.
        "def _value_doubled(value):\n",
        concat!(
            "@_hg.compute_node\n",
            "def doubled(value: _hg.TS[_py.int]) -> _hg.TS[_py.int]:\n",
            "    return _core.add__i64_i64(lhs=_value_doubled(value=value.value), rhs=1)\n",
        ),
        "def eight():\n    return _value_doubled(value=4)\n",
        "    if not (_harness.same(_harness.evaluate(doubled, {\"value\": _hgl_4core._inst_replay__i64}, _hgl_4core._inst_record__i64, value=[4]), [9])):\n",
        "        raise _py.AssertionError()\n",
        // An input named as the harness's own parameter is.
        "    if not (_harness.same(_harness.evaluate(pass_, {\"target\": _hgl_4core._inst_replay__i64}, _hgl_4core._inst_record__i64, target=[1]), [1])):\n",
        "        raise _py.AssertionError()\n",
    ]);
}

/// A module whose path begins another's, with a `var` written `T?`
/// that is given no value and a `start` with nothing in it.
const NESTED: [(&str, &str); 2] = [fixture!("demo.hgl"), fixture!("demo.util.hgl")];

/// A module named as hgraph's own package is.
const SHADOW: (&str, &str) = fixture!("hgraph.hgl");

/// Every file is under the package `_hgl`, so a module named as hgraph
/// is, or as a module of Python's library is, takes no other's place.
#[test]
fn a_module_takes_no_other_packages_name() {
    let emitted = emit_clean(&[SHADOW]);
    let python = emitted.module("hgraph");
    assert!(python.contains("import hgraph as _hg\n"));
    // The module calls no function of the provider's, and its test
    // calls the harness, which imports the provider: both are written.
    assert!(!python.contains("_core_native"));
    assert!(python.contains("import _hgl._harness as _harness\n"));
    assert!(
        emitted
            .text("_hgl/_harness.py")
            .contains("import _hgl._core_native as _core")
    );
    assert!(
        emitted
            .text("_hgl/_core_support.py")
            .contains("class HglError")
    );
    assert_eq!(hgl_hgraph::python_module("hgraph"), "_hgl._m_hgraph");
    assert_eq!(
        hgl_hgraph::python_module("demo.pass"),
        "_hgl._m_demo._m_pass"
    );
}

/// Paths whose parts differ only in where a `_` falls, and a module
/// that declares a name a module under it has: each stays its own. A
/// sequence that is a set is one in Python too (EXPR-9).
const PATHS: [(&str, &str); 4] = [
    fixture!("a_.b.hgl"),
    fixture!("a._b.hgl"),
    fixture!("paths.util.hgl"),
    fixture!("paths.hgl"),
];

#[test]
fn paths_and_declarations_stay_apart() {
    let emitted = emit_clean(&PATHS);
    let python = emitted.module("paths");
    python.assert_writes(&[
        "import _hgl._m_a_._m_b as _hgl_2a__1b\n",
        "import _hgl._m_a._m__b as _hgl_1a_2_b\n",
        "import _hgl._m_paths._m_util as _hgl_5paths_4util\n",
        "def util():\n    return 8\n",
        "def members():\n    return _py.frozenset((1, 2, 1))\n",
    ]);
}

/// Each module is a package, so one module's path may begin another's.
/// CTRL-11: a `var` written `T?` with no value is null. A `start` with
/// nothing in it is still a function.
#[test]
fn modules_nest_and_empty_forms_have_a_body() {
    let emitted = emit_clean(&NESTED);
    let paths: Vec<&str> = emitted
        .output
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    assert_eq!(
        paths,
        [
            "_hgl/__init__.py",
            "_hgl/_m_demo/__init__.py",
            "_hgl/_m_demo/_m_util/__init__.py",
            // Core, whose `replay` and `record` an eval's run is wired
            // with.
            "_hgl/_m_core/__init__.py",
            // The harness, and the provider it imports.
            "_hgl/_core_native.py",
            "_hgl/_core_support.py",
            "_hgl/_harness.py"
        ]
    );
    let python = emitted.module("demo");
    python.assert_writes(&[
        "import _hgl._m_demo._m_util as _hgl_4demo_4util\n",
        "def kept():\n    held = None\n    return held\n",
        "@idle.start\ndef _start_idle():\n    pass\n",
        "    if not ((kept() is None)):\n",
        "        raise _py.AssertionError()\n",
    ]);
}

/// A form the target does not write yet is reported where it is
/// written, as a `build` diagnostic: no source category, since the
/// language accepts the source.
/// TST-17: `run(call, config)` in an integration test is the eval's
/// run in real time. RUN-1: an eval's run configuration is written.
/// UNIT-14: an application is written, what it declares as a module's
/// is and its run statement after it.
#[test]
fn a_run_in_real_time_and_an_application_are_written() {
    let emitted = emit(&[
        fixture!("a_run_in_real_time_and_an_application_are_written", "m.hgl"),
        fixture!(
            "a_run_in_real_time_and_an_application_are_written",
            "m.live.hgl"
        ),
    ]);
    // The eval's configuration is written (RUN-1), and so are the run
    // in real time and the application.
    assert_eq!(emitted.reported, Vec::<String>::new());
    let module = emitted.module("m");
    assert!(
        module.contains(concat!(
            "def test_live():\n",
            "    if not (_harness.same(_harness.evaluate(pass_, ",
            "{\"value\": _hgl_4core._inst_replay__i64}, _hgl_4core._inst_record__i64, ",
            "value=[7], _real_time=True), [7])):\n",
        )),
        "{module}"
    );
    // TST-17: an integration test is one, and has a time limit; a test
    // that is none has no limit.
    let limits: Vec<(&str, bool, Option<u64>)> = emitted
        .output
        .tests
        .iter()
        .map(|test| {
            (
                test.name.as_str(),
                test.integration,
                test.limit().map(|limit| limit.as_secs()),
            )
        })
        .collect();
    assert_eq!(limits, [("bounded", false, None), ("live", true, Some(60))]);
    // One that writes `@timeout(...)` has that limit.
    let written = emit(&[INTEGRATION]);
    let (_, brief) = written.test("brief");
    assert_eq!(brief.limit(), Some(std::time::Duration::from_millis(500)));
    assert_eq!(
        emitted.module("m.live"),
        concat!(
            "# Generated by the hgl compiler for hgraph, from application `m.live`. Do not edit.\n",
            "import builtins as _py\n",
            "import _hgl._core_native as _core\n",
            "import _hgl._harness as _harness\n",
            "import _hgl._m_core as _hgl_4core\n",
            "import _hgl._m_m as _hgl_1m\n",
            "\n\n",
            "def twice(value):\n",
            "    return _core.mul__i64_i64(lhs=value, rhs=2)\n",
            "\n\n",
            // RUN-3: `run` is a run in real time. CRUN-8: core's
            // `record`, for the `i64` the entry publishes.
            "def _application():\n",
            "    return _harness.run(_hgl_1m.one, _hgl_4core._inst_record__i64, True)\n",
        )
    );
}

/// UNIT-14: "An application is a unit of its own ... It holds what a
/// module holds, and one run statement". RUN-1: "its run statement is
/// `run(call, config)` or `eval(call, config)`, where the call is of an
/// entry ... giving the entry's const arguments, and the configuration
/// is a value of core's `RunConfig`".
const APPLICATIONS: [(&str, &str); 7] = [
    fixture!("applications", "demo.ticks.hgl"),
    fixture!("applications", "demo.ticks.backtest.hgl"),
    fixture!("applications", "demo.ticks.live.hgl"),
    fixture!("applications", "demo.ticks.seeded.hgl"),
    fixture!("applications", "demo.ticks.endless.hgl"),
    fixture!("applications", "demo.ticks.silent.hgl"),
    fixture!("applications", "demo.ticks.broken.hgl"),
];

/// An application's run statement is a function that runs its entry
/// (UNIT-14, RUN-1): the entry, core's `record` for what it publishes
/// (core's CRUN-8), the mode its word is (RUN-3), the entry's `const`
/// arguments by name, and the configuration.
#[test]
fn an_applications_run_statement_is_a_function_that_runs_its_entry() {
    let emitted = emit_clean(&APPLICATIONS);
    let listed: Vec<(&str, bool, bool)> = emitted
        .output
        .applications
        .iter()
        .map(|application| {
            (
                application.path.as_str(),
                application.real_time,
                application.blocked.is_some(),
            )
        })
        .collect();
    assert_eq!(
        listed,
        [
            ("demo.ticks.backtest", false, false),
            ("demo.ticks.broken", false, false),
            ("demo.ticks.endless", true, false),
            ("demo.ticks.live", true, false),
            ("demo.ticks.seeded", false, false),
            ("demo.ticks.silent", false, false),
        ]
    );
    let backtest = emitted.module("demo.ticks.backtest");
    assert!(
        backtest.ends_with(concat!(
            "def _application():\n",
            "    return _harness.run(_hgl_4demo_5ticks.doubled, ",
            "_hgl_4core._inst_record__i64, False, upto=rounds(), ",
            "_config=_hgl_4core.RunConfig(length=_dt.timedelta(microseconds=10000000), ",
            "start=None, end=None))\n",
        )),
        "{backtest}"
    );
    // An argument left out takes its default, and `run` is real time.
    let live = emitted.module("demo.ticks.live");
    assert!(
        live.contains(concat!(
            "_harness.run(_hgl_4demo_5ticks.counted, _hgl_4core._inst_record__i64, True, ",
            "upto=100, every=_dt.timedelta(microseconds=50000), ",
        )),
        "{live}"
    );
    // UNIT-14: what sets the run's store is a function of the store,
    // given to the run, which calls it before it builds the graph.
    // The instance of `sum` its value calls is written, as one a body
    // calls is (GEN-2).
    let seeded = emitted.module("demo.ticks.seeded");
    assert!(seeded.contains("def _inst_sum__i64(a, b):\n"), "{seeded}");
    assert!(
        seeded.ends_with(concat!(
            "def _application_store(_store):\n",
            "    _core.put_GlobalState_str_V(key=\"limit\", store=_store, ",
            "value=_inst_sum__i64(a=base(), b=1))\n",
            "\n\n",
            "def _application():\n",
            "    return _harness.run(_hgl_4demo_5ticks.limited, _hgl_4core._inst_record__i64, ",
            "False, every=_dt.timedelta(microseconds=1000000), _store=_application_store)\n",
        )),
        "{seeded}"
    );
    // An entry with no result is given no `record`, and a statement
    // with no configuration writes none.
    let silent = emitted.module("demo.ticks.silent");
    assert!(
        silent.ends_with("    return _harness.run(_hgl_4demo_5ticks.quiet, None, False, upto=2)\n"),
        "{silent}"
    );
}

/// An application that needs a form the target does not write yet is
/// blocked by it, as a test is: it is reported, and is not to be run.
#[test]
fn an_application_that_needs_what_is_not_written_is_blocked() {
    let emitted = emit(&[
        fixture!(
            "an_application_that_needs_what_is_not_written_is_blocked",
            "m.hgl"
        ),
        fixture!(
            "an_application_that_needs_what_is_not_written_is_blocked",
            "m.app.hgl"
        ),
    ]);
    let [application] = emitted.output.applications.as_slice() else {
        panic!("one application is written");
    };
    let blocked = application.blocked.as_ref().expect("it is blocked");
    assert_eq!(emitted.place(blocked.primary), "m.hgl:10:5");
}

/// Core's CRUN-2: "`start` is inclusive and `end` exclusive: no cycle
/// runs before `start`, and none at or after `end` ... `length` gives
/// the end counted from the start the run has, given or not".
const BOUNDED: (&str, &str) = fixture!("demo.bounded.hgl");

/// Core's CRUN-2: "A `RunConfig` with both an `end` and a `length` is
/// refused before the run's graph is built, an HGL error with code
/// `run.end_and_length`." And a dense trace too long to give: one has a
/// cell for each smallest step, and this source publishes a second
/// after its run starts.
const BOUNDED_FAILS: (&str, &str) = fixture!(
    "a_failure_names_the_statement_that_failed",
    "demo.bounded.fails.hgl"
);

/// An eval's configuration is given to the harness last, as `_config`:
/// after the call's arguments, as EXPR-13 evaluates them.
#[test]
fn an_evals_configuration_is_given_to_the_harness() {
    let emitted = emit_clean(&[BOUNDED]);
    let python = emitted.module("demo.bounded");
    python.assert_writes(&[
        "_harness.evaluate(pass_, {\"value\": _hgl_4core._inst_replay__i64}, _hgl_4core._inst_record__i64, value=[1, 2, 3, 4, 5], _config=",
        "_harness.evaluate(pass_, {\"value\": _hgl_4core._inst_replay__i64}, _hgl_4core._inst_record__i64, value=[7])",
    ]);
}

#[test]
fn a_form_not_written_yet_is_reported() {
    let emitted = emit(&[fixture!("a_form_not_written_yet_is_reported", "m.hgl")]);
    assert_eq!(
        emitted.reported,
        [
            // `gated`, which injects a scheduler, and `stopped`, whose
            // `stop` reads `out`, are written (TIME-9, TIME-7).
            "m.hgl:21:14 target.unsupported: the hgraph target does not write a default of a temporal input yet",
            "m.hgl:24:17 target.unsupported: the hgraph target does not write a pack parameter yet",
            // `chosen`, a graph with an `if` over a connection, is
            // written (CTRL-13). The two of a struct, an atomic
            // tuple's, then what the test holds.
            "m.hgl:42:39 target.unsupported: the hgraph target does not write this expression in a graph yet",
            "m.hgl:46:42 target.unsupported: the hgraph target does not write a generic struct made where its arguments are not known yet",
            "m.hgl:48:11 target.unsupported: the hgraph target does not write the type `atomic<tuple<f64, f64>>` yet",
        ]
    );
}

/// Value functions and symbols given connections in graphs, and a
/// value function an eval names.
const LIFTS: (&str, &str) = fixture!("demo.lift.hgl");

/// FN-6: "A selected value helper, a `const fn` or a value candidate of
/// an operator, with temporal arguments in composition is lifted:
/// temporal arguments become inputs; scalar arguments and omitted
/// defaults bind as configuration." The lifted call is a node of its
/// own, written before the graph that uses it.
///
/// FN-11: "A lifted argument with a declared default does not require a
/// valid input: bind the default when that input is invalid."
///
/// TST-8: "A selected value helper uses ordinary lifting."
#[test]
fn a_value_function_given_a_connection_is_lifted() {
    let emitted = emit_clean(&[LIFTS]);
    let python = emitted.module("demo.lift");
    python.assert_writes(&[
        // A symbol: the candidate selected for it, lifted.
        concat!(
            "@_hg.compute_node\n",
            "def _lift_0_add__i64_i64(lhs: _hg.TS[_py.int], rhs: _hg.TS[_py.int]) -> _hg.TS[_py.int]:\n",
            "    return _core.add__i64_i64(lhs=lhs.value, rhs=rhs.value)\n",
            "\n",
            "\n",
            "@_hg.graph\n",
            "def summed(left: _hg.TS[_py.int], right: _hg.TS[_py.int]) -> _hg.TS[_py.int]:\n",
            "    return _lift_0_add__i64_i64(lhs=left, rhs=right)\n",
        ),
        // An input whose parameter has a default need not be valid.
        concat!(
            "@_hg.compute_node(valid=(\"value\",))\n",
            "def _lift_1_scale(value: _hg.TS[_py.float], factor: _hg.TS[_py.float]) -> _hg.TS[_py.float]:\n",
            "    return scale(value=value.value, factor=(factor.value if factor.valid else 2.0))\n",
        ),
        // A scalar argument is configuration.
        concat!(
            "@_hg.compute_node\n",
            "def _lift_2_scale(value: _hg.TS[_py.float], factor: _py.float) -> _hg.TS[_py.float]:\n",
            "    return scale(value=value.value, factor=factor)\n",
            "\n",
            "\n",
            "@_hg.graph\n",
            "def half_configured(price: _hg.TS[_py.float]) -> _hg.TS[_py.float]:\n",
            "    return _lift_2_scale(value=price, factor=3.0)\n",
        ),
        // A default left out is bound where the node calls.
        concat!(
            "def _lift_3_scale(value: _hg.TS[_py.float]) -> _hg.TS[_py.float]:\n",
            "    return scale(value=value.value, factor=2.0)\n",
        ),
        // What a temporal or a lifted call gives is a connection.
        concat!(
            "    return _lift_6_add__i64_i64(lhs=_lift_5_doubled(value=",
            "_lift_4_add__i64_i64(lhs=relay(value=left), rhs=right)), rhs=1)\n",
        ),
        // The same lifting twice in a graph is one node.
        concat!(
            "def twice(left: _hg.TS[_py.int], right: _hg.TS[_py.int]) -> _hg.TS[_py.int]:\n",
            "    return _lift_7_add__i64_i64(lhs=_lift_7_add__i64_i64(lhs=left, rhs=right), ",
            "rhs=_lift_7_add__i64_i64(lhs=left, rhs=right))\n",
        ),
        // "All-scalar calls execute directly and invent no source
        // timing."
        "rhs=doubled(value=factor))\n",
        // A string a program wrote is its own, whatever is in it.
        concat!(
            "def _lift_10_label(value: _hg.TS[_py.int]) -> _hg.TS[_py.str]:\n",
            "    return label(value=value.value, text=\"{NAME}\")\n",
        ),
        // "An outputless helper lifts to a sink."
        concat!(
            "@_hg.sink_node\n",
            "def _lift_11_note(value: _hg.TS[_py.int]):\n",
            "    note(value=value.value)\n",
        ),
        // An eval of a value function runs it lifted.
        concat!(
            "    if not (_harness.same(_harness.evaluate(_lift_12_doubled, {\"value\": _hgl_4core._inst_replay__i64}, _hgl_4core._inst_record__i64, value=[1, None, 3]), [2, None, 6])):\n",
            "        raise _py.AssertionError()\n",
        ),
    ]);
    assert_eq!(python.matches("def _lift_7_add__i64_i64(").count(), 1);
}

/// An operator of a program with two candidates, and arithmetic that
/// selects candidates core has an HGL body for.
const CANDIDATES: (&str, &str) = fixture!("demo.mixed.hgl");

/// GEN-4: "An `impl fn` is a temporal candidate of the operator it
/// names and an `impl const fn` a value candidate." GEN-5: "A call in
/// composition selects among all of them ... There, a temporal
/// candidate outranks a value candidate of the same parameter types".
const TEMPORAL_CANDIDATES: [(&str, &str); 2] =
    [fixture!("demo.joins.hgl"), fixture!("demo.joined.hgl")];

/// GEN-4: a temporal candidate with a body is a node or a graph as a
/// temporal function is, named as a value candidate is, by the
/// operator and the types of its parameters. A call in a graph that
/// selects it is a call of that node or graph, and is not lifted
/// (GEN-5, FN-6). The value candidate of the same parameter types is
/// `_value_impl_...`, as the value definition of a name with both is
/// `_value_name` (FN-4).
#[test]
fn a_temporal_candidate_is_a_node_or_a_graph() {
    let emitted = emit_clean(&TEMPORAL_CANDIDATES);
    let joins = emitted.module("demo.joins");
    joins.assert_writes(&[
        concat!(
            "@_hg.compute_node\n",
            "def _impl_join__i64_i64(lhs: _hg.TS[_py.int], rhs: _hg.TS[_py.int], ",
            "_state: _hg.STATE = None) -> _hg.TS[_py.int]:\n",
        ),
        concat!(
            "@_impl_join__i64_i64.start\n",
            "def _start__impl_join__i64_i64(_state: _hg.STATE = None):\n",
        ),
        concat!(
            "def _value_impl_join__i64_i64(lhs, rhs):\n",
            "    return _core.sub__i64_i64(lhs=lhs, rhs=rhs)\n",
        ),
        concat!(
            "@_hg.graph\n",
            "def _impl_join__i64_bool(lhs: _hg.TS[_py.int], rhs: _hg.TS[_py.bool]) ",
            "-> _hg.TS[_py.int]:\n",
            "    return _impl_join__i64_i64(lhs=lhs, rhs=lhs)\n",
        ),
        // A value function calls the value candidate.
        concat!(
            "def plain(a, b):\n",
            "    return _value_impl_join__i64_i64(lhs=a, rhs=b)\n",
        ),
    ]);
    let joined = emitted.module("demo.joined");
    joined.assert_writes(&[
        // The candidate of another module, through that module.
        "    return _hgl_4demo_5joins._impl_join__i64_i64(lhs=a, rhs=b)\n",
        "    return _hgl_4demo_5joins._impl_join__i64_bool(lhs=a, rhs=b)\n",
        // A value candidate is lifted, and a temporal one is not.
        "    return _hgl_4demo_5joins._impl_join__str_str(lhs=lhs.value, rhs=rhs.value)\n",
    ]);
    assert_eq!(joined.matches("def _lift_").count(), 2, "{joined}");
}

/// GEN-2: "Resolve every needed generic from actual inputs, requested
/// output, or solvable equality." A temporal function with generic
/// parameters, and core's temporal candidates of `+` and `-` under
/// `{invalid: .identity}` (OPS-9), each called with more than one type.
const GENERICS: (&str, &str) = fixture!("demo.generics.hgl");

/// GEN-2: hgraph's nodes and graphs are written with their inputs'
/// types, so a temporal function with generic parameters is written
/// once for each set of generic arguments the program calls it with,
/// in the module that declares it, and not at all where it is called
/// with none. GEN-16: an operation the declaration requires is, in each
/// instance, the call of the candidate that answers the requirement
/// there. GEN-13: a generic value read as a value is the constant the
/// call gives.
#[test]
fn a_generic_temporal_function_is_written_for_each_call() {
    let emitted = emit_clean(&[GENERICS]);
    assert_eq!(emitted.blocked("generics_are_instances"), None);
    let python = emitted.module("demo.generics");
    python.assert_writes(&[
        // A graph for each type, of core's candidate for that type.
        concat!(
            "@_hg.graph\n",
            "def _inst_identity_sum__f64(lhs: _hg.TS[_py.float], rhs: _hg.TS[_py.float]) ",
            "-> _hg.TS[_py.float]:\n",
            "    return _hgl_4core._inst__impl_add__T_T__f64_core__InvalidInputPolicy_identity(lhs=lhs, rhs=rhs)\n",
        ),
        "def _inst_identity_sum__duration(",
        "def _inst_identity_sum__str(",
        "    return _hgl_4core._inst__impl_sub__T_T__f64_core__InvalidInputPolicy_identity(lhs=lhs, rhs=rhs)\n",
        // A node for each type, its required `add_` the candidate.
        concat!(
            "@_hg.compute_node\n",
            "def _inst_doubled__str(value: _hg.TS[_py.str]) -> _hg.TS[_py.str]:\n",
            "    return _core.add__str_str(lhs=value.value, rhs=value.value)\n",
        ),
        "    return _core.add__i64_i64(lhs=value.value, rhs=value.value)\n",
        // An instance calls the instances it asks for.
        "    return _inst_doubled__f64(value=_inst_doubled__f64(value=value))\n",
        // In a graph the value candidate is lifted.
        "def _inst_summed__i64(a: _hg.TS[_py.int], b: _hg.TS[_py.int]) -> _hg.TS[_py.int]:\n",
        "    return _core.add__i64_i64(lhs=lhs.value, rhs=rhs.value)\n",
        // The constant a generic value is given.
        concat!(
            "def _inst_scaled__3(value: _hg.TS[_py.int]) -> _hg.TS[_py.int]:\n",
            "    return _core.mul__i64_i64(lhs=value.value, rhs=3)\n",
        ),
        "    return _core.mul__i64_i64(lhs=value.value, rhs=4)\n",
        concat!(
            "def _inst_latest__str(value: _hg.TS[_py.str], fallback: _py.str, ",
            "_state: _hg.STATE = None) -> _hg.TS[_py.str]:\n",
        ),
        // A call names the instance for its types.
        "    return _inst_identity_sum__i64(lhs=a, rhs=b)\n",
        "    return _inst_scaled__3(value=value)\n",
        "_harness.evaluate(_inst_scaled__4, {\"value\": _hgl_4core._inst_replay__i64}, _hgl_4core._inst_record__i64, value=[2])",
    ]);
    // Nothing calls `unused`, and nothing is written for it.
    assert!(!python.contains("unused"), "{python}");
    // Core's candidate is written in core's file, once for each type.
    let core = emitted.module("core");
    core.assert_writes(&[
        concat!(
            "@_hg.compute_node(valid=())\n",
            "def _inst__impl_add__T_T__f64_core__InvalidInputPolicy_identity(lhs: _hg.TS[_py.float], ",
            "rhs: _hg.TS[_py.float], _output: _hg.TS_OUT[_py.float] = None) ",
            "-> _hg.TS[_py.float]:\n",
            "    if lhs.valid and rhs.valid:\n",
            "        return _core.add__f64_f64(lhs=lhs.value, rhs=rhs.value)\n",
        ),
        "        return _core.add__duration_duration(lhs=lhs.value, rhs=rhs.value)\n",
        "        return _core.neg__f64(value=rhs.value)\n",
    ]);
    assert_eq!(
        core.matches("def _inst__impl_add__T_T__").count(),
        4,
        "{core}"
    );
}

/// GEN-16: a value function with generic parameters that calls an
/// operation it requires, or hands a requirement on to what it calls.
const VALUE_GENERICS: (&str, &str) = fixture!("demo.vals.hgl");

/// GEN-16: which candidate a required operation calls depends on the
/// types, so a value function that calls one, or hands a requirement
/// on, is written once for each set of generic arguments it is called
/// with, as a temporal one is (GEN-2). One with no requirement is
/// written once: Python takes a value of any type. An instance is
/// called wherever the function is: by a value function, a node's
/// hook, a constant, and the node that lifts it in a graph or an eval.
#[test]
fn a_value_function_that_requires_is_written_for_each_call() {
    let emitted = emit_clean(&[VALUE_GENERICS]);
    assert_eq!(emitted.blocked("values_are_instances"), None);
    let python = emitted.module("demo.vals");
    python.assert_writes(&[
        concat!(
            "def _inst_twice__i64(value):\n",
            "    return _impl_join_i64_i64(lhs=value, rhs=value)\n",
        ),
        concat!(
            "def _inst_twice__str(value):\n",
            "    return _impl_join_str_str(lhs=value, rhs=value)\n",
        ),
        // The candidate for lists is an instance too, for its own
        // requirement.
        concat!(
            "def _inst_twice__list_i64_(value):\n",
            "    return _inst__impl_join_list_T__list_T___i64(lhs=value, rhs=value)\n",
        ),
        concat!(
            "def _inst__impl_join_list_T__list_T___i64(lhs, rhs):\n",
            "    return [_impl_join_i64_i64(lhs=_core.item(lhs, 0), rhs=_core.item(rhs, 0))]\n",
        ),
        // A symbol the function requires.
        concat!(
            "def _inst_sum__f64(a, b):\n",
            "    return _core.add__f64_f64(lhs=a, rhs=b)\n",
        ),
        // A requirement handed on.
        concat!(
            "def _inst_outer__i64(value):\n",
            "    return _inst_twice__i64(value=_inst_twice__i64(value=value))\n",
        ),
        // No requirement: one function. A call in it with whole
        // arguments names its instance, which nothing else asks for.
        "def same(value):\n",
        concat!(
            "def paired(value):\n",
            "    return _inst_sum__str(a=\"a\", b=\"b\")\n",
        ),
        concat!(
            "def _inst_sum__str(a, b):\n",
            "    return _core.add__str_str(lhs=a, rhs=b)\n",
        ),
        // Where an instance is called.
        "    return _inst_sum__i64(a=40, b=60)\n",
        "    return _inst_twice__i64(value=value.value)\n",
        "    return _inst_sum__f64(a=a.value, b=b.value)\n",
        "lhs=_inst_twice__i64(value=1), rhs=11,",
    ]);
    assert!(!python.contains("def twice("), "{python}");
}

/// GEN-7: a requirement that names a generic parameter with nothing
/// stated is answered by a candidate that takes any one type. The
/// caller then states no requirement and hands none on, and still
/// calls a function that is written by instance, with its own generic
/// parameter.
const THROUGH: (&str, &str) = fixture!("demo.through.hgl");

/// GEN-2: which instance such a caller calls is known only where its
/// own generic parameter is. So it is written by instance too, and so
/// is what calls it in turn with a generic parameter of its own.
/// GEN-13: a value function that reads a generic value parameter is
/// written for each constant it is given.
#[test]
fn a_caller_of_an_instance_with_its_own_parameter_is_an_instance() {
    let emitted = emit_clean(&[THROUGH]);
    assert_eq!(emitted.blocked("a_caller_is_an_instance_too"), None);
    let python = emitted.module("demo.through");
    python.assert_writes(&[
        // The candidate for any one type is written once.
        "def _impl_pick_T_T(lhs, rhs):\n",
        concat!(
            "def _inst_first__i64(value):\n",
            "    return _impl_pick_T_T(lhs=value, rhs=value)\n",
        ),
        concat!(
            "def _inst_wrapper__i64(value):\n",
            "    return _inst_first__i64(value=value)\n",
        ),
        concat!(
            "def _inst_outermost__str(value):\n",
            "    return _inst_wrapper__str(value=value)\n",
        ),
        concat!(
            "def _inst_wrapper__str(value):\n",
            "    return _inst_first__str(value=value)\n",
        ),
        // A generic value is the constant the call gives.
        concat!("def _inst_width__3():\n", "    return 3\n"),
        concat!(
            "def _inst_wider__4():\n",
            "    return _core.add__i64_i64(lhs=_inst_width__4(), rhs=1)\n",
        ),
    ]);
    assert!(!python.contains("def wrapper("), "{python}");
}

/// GEN-2: an instance is written in its declaration's module, with the
/// types of the module that calls it: here an enum of a module that
/// imports the declaration's.
const CALLERS_TYPE: [(&str, &str); 2] = [fixture!("demo.kept.hgl"), fixture!("demo.sides.hgl")];

/// The two modules then import one another in Python, though they do
/// not in the source. A function's annotations are read where it is
/// defined, so a module defines its types before it imports another
/// generated module: whichever of the two is loaded first, the enum is
/// there when the instance that names it is defined.
#[test]
fn a_module_has_its_types_before_it_imports_another() {
    let emitted = emit_clean(&CALLERS_TYPE);
    let kept = emitted.module("demo.kept");
    kept.assert_writes(&[
        "import _hgl._m_demo._m_sides as _hgl_4demo_5sides\n",
        concat!(
            "def _inst_held__demo_sides__Side(value: _hg.TS[_hgl_4demo_5sides.Side]) ",
            "-> _hg.TS[_hgl_4demo_5sides.Side]:\n",
        ),
    ]);
    let sides = emitted.module("demo.sides");
    let defined = sides
        .find("Side = _enum.Enum(")
        .expect("the enum is written");
    let imported = sides
        .find("import _hgl._m_demo._m_kept as _hgl_4demo_4kept\n")
        .expect("the module is imported");
    assert!(defined < imported, "{sides}");
}

/// Each instance is a piece of its module. Where one has a form the
/// target does not write, a whole list of lists published here, that is
/// reported where the declaration is written, and a test that reaches
/// that instance is not run. A test that reaches another instance of
/// the declaration is.
#[test]
fn an_instance_that_is_not_written_is_reported() {
    let emitted = emit(&[fixture!(
        "an_instance_that_is_not_written_is_reported",
        "demo.held.hgl"
    )]);
    // Once, for the one instance that has it, of the node.
    assert_eq!(
        emitted.reported,
        [
            "demo.held.hgl:3:32 target.unsupported: the hgraph target does not write a whole list published where an element is temporal composite or nullable yet",
        ]
    );
    assert_eq!(emitted.blocked("whole"), None);
    let blocked = emitted.blocked("listed").expect("the test is not run");
    assert!(
        blocked.ends_with(
            "the hgraph target does not write a whole list published where an element is temporal composite or nullable yet"
        ),
        "{blocked}"
    );
    let python = emitted.module("demo.held");
    assert!(python.contains("def _inst_held__i64("), "{python}");
    assert!(python.contains("def _inst_again__i64("), "{python}");
    assert!(!python.contains("def _inst_held__list"), "{python}");
}

/// GEN-4: an `impl const fn` is a value candidate of its operator. One
/// with a body is written as a function, named by the operator and
/// the types of its parameters, since an operator may have many and a
/// candidate binds no name (UNIT-12). An operator has nothing of its
/// own to write.
///
/// Core is a module as any other: the candidates it has an HGL body
/// for are written from that body, in a file of core's, and its
/// native ones are the provider's.
#[test]
fn a_candidate_with_a_body_is_a_function() {
    let emitted = emit_clean(&[CANDIDATES]);
    let python = emitted.module("demo.mixed");
    python.assert_writes(&[
        "import _hgl._m_core as _hgl_4core\n",
        concat!(
            "def _impl_blend__i64_i64(lhs, rhs):\n",
            "    return _core.add__i64_i64(lhs=_core.mul__i64_i64(lhs=lhs, rhs=10), rhs=rhs)\n",
        ),
        // `f64 * i64` and `f64 + i64` are candidates core writes in HGL.
        concat!(
            "def _impl_blend__f64_i64(lhs, rhs):\n",
            "    return _hgl_4core._impl_add__f64_i64(",
            "lhs=_hgl_4core._impl_mul__f64_i64(lhs=lhs, rhs=10), rhs=rhs)\n",
        ),
        // A candidate with a body is lifted as a function is.
        concat!(
            "def _lift_1_add_(lhs: _hg.TS[_py.float], rhs: _py.int) -> _hg.TS[_py.float]:\n",
            "    return _hgl_4core._impl_add__f64_i64(lhs=lhs.value, rhs=rhs)\n",
        ),
        "    return _lift_2_blend_(lhs=left, rhs=right)\n",
        "    if not (_hgl_4core._impl_eq__f64_i64_f64_f64(lhs=2.0, rhs=2, abs=0.0, rel=0.0)):\n",
    ]);
    let core = emitted.module("core");
    core.assert_writes(&[
        "# Generated by the hgl compiler for hgraph, from module `core`. Do not edit.\n",
        concat!(
            "def _impl_add__f64_i64(lhs, rhs):\n",
            "    return _core.add__i64_f64(lhs=rhs, rhs=lhs)\n",
        ),
        concat!(
            "def _impl_eq__f64_i64_f64_f64(lhs, rhs, abs, rel):\n",
            "    return _core.eq__i64_f64_f64_f64(lhs=rhs, rhs=lhs, abs=abs, rel=rel)\n",
        ),
    ]);

    // A program that uses nothing core writes in HGL has no file of
    // core's: an eval would, for its `replay` and `record`.
    let emitted = emit(&[fixture!(
        "a_candidate_with_a_body_is_a_function",
        "demo.plain.hgl"
    )]);
    assert!(
        emitted
            .output
            .files
            .iter()
            .all(|file| file.path != "_hgl/_m_core/__init__.py")
    );
}

/// What core holds that the target does not write is the program's to
/// report only where the program reaches it: here the instance of core's
/// identity addition (OPS-9) for a type whose publication the target
/// does not write, which the program's graph reaches. The report is
/// core's own, at core's source. A test that does not reach it is run,
/// and a program that reaches none of it has no report.
#[test]
fn what_core_lacks_is_reported_where_a_program_reaches_it() {
    let emitted = emit(&[fixture!(
        "what_core_lacks_is_reported_where_a_program_reaches_it",
        "demo.kept.hgl"
    )]);
    assert_eq!(emitted.reported.len(), 1, "{:?}", emitted.reported);
    assert!(
        emitted.reported[0].starts_with("<core>/core/core.hgl:")
            && emitted.reported[0].ends_with(
                " target.unsupported: the hgraph target does not write a whole list published where an element is temporal composite or nullable yet"
            ),
        "{:?}",
        emitted.reported
    );
    assert_eq!(emitted.blocked("doubles"), None);
    let emitted = emit(&[fixture!(
        "a_candidate_with_a_body_is_a_function",
        "demo.plain.hgl"
    )]);
    assert_eq!(emitted.reported, Vec::<String>::new());
}

/// Divisions, each with the zero policy it is given or takes by
/// default. NUM-1: floor division rounds toward negative infinity, and
/// with an infinity or a NaN is the floor of the IEEE quotient.
const DIVISION: (&str, &str) = fixture!("demo.div.hgl");

/// Divisions by zero under `fail`, given or taken by default.
const DIVISION_FAILS: (&str, &str) = fixture!(
    "a_failure_names_the_statement_that_failed",
    "demo.div.fails.hgl"
);

/// NUM-12: "Every division takes `zero`: `/`, `//`, and `%`". NUM-9:
/// "`zero` is a generic value parameter of the candidates that declare
/// it, and a call gives it a compile-time constant." The provider's
/// function is given it by name, as the name of the policy: the one
/// written, or the default the candidate declares (NUM-3: `/`
/// "defaults to the `ieee` zero policy"; a division of two `i64`
/// takes `.fail`).
#[test]
fn a_division_takes_its_zero_policy() {
    let emitted = emit_clean(&[DIVISION]);
    let python = emitted.module("demo.div");
    python.assert_writes(&[
        "def ratio(a, b):\n    return _core.div__f64_f64(lhs=a, rhs=b, zero=\"ieee\")\n",
        "def nans(a, b):\n    return _core.div__f64_f64(lhs=a, rhs=b, zero=\"nan\")\n",
        "def strict(a, b):\n    return _core.div__f64_f64(lhs=a, rhs=b, zero=\"fail\")\n",
        // TYPE-7: a member is the same policy however it is written.
        "def named(a, b):\n    return _core.div__f64_f64(lhs=a, rhs=b, zero=\"nan\")\n",
        "def grouped(a, b):\n    return _core.div__f64_f64(lhs=a, rhs=b, zero=\"nan\")\n",
        "def built(a, b):\n    return _core.div__f64_f64(lhs=a, rhs=b, zero=\"nan\")\n",
        "def floors(a, b):\n    return _core.floordiv__i64_i64(lhs=a, rhs=b, zero=\"fail\")\n",
        "def rest(a, b):\n    return _core.mod__i64_i64(lhs=a, rhs=b, zero=\"fail\")\n",
        // A lifted division is configured as the call is.
        concat!(
            "def _lift_1_div__i64_i64(lhs: _hg.TS[_py.int], rhs: _py.int) -> _hg.TS[_py.float]:\n",
            "    return _core.div__i64_i64(lhs=lhs.value, rhs=rhs, zero=\"ieee\")\n",
        ),
    ]);
}

/// Results and errors in value functions, and a `?` in a `when`.
const RESULTS: (&str, &str) = fixture!("demo.results.hgl");

/// A node whose `?` meets a failure.
const RESULT_FAILS: (&str, &str) = fixture!(
    "a_failure_names_the_statement_that_failed",
    "demo.results.fails.hgl"
);

/// ERR-2: "`ok(value)` constructs a success and `err(value)` a
/// failure." ERR-3: `is_ok` and `is_err` give a `bool`, and
/// `binding.value` and `binding.error` the payload. ERR-4: an `error`
/// has a message and a code that may be null. They are values of the
/// provider's.
///
/// ERR-5: "Postfix `?` evaluates once. Success yields T. In a value
/// function returning `result<U,E>`, failure returns the same E as
/// Err. In when, only `result<T,error>` may propagate to node failure."
///
/// NUM-9: "division with `zero: .error` returns `result<f64, error>`".
/// NUM-10: "A checked-division Err is ordinary result data. It neither
/// implicitly fails a node nor requires an exception."
#[test]
fn a_result_is_a_value_and_a_failure_leaves_through_a_question_mark() {
    let emitted = emit_clean(&[RESULTS]);
    let python = emitted.module("demo.results");
    python.assert_writes(&[
        "def checked(a, b):\n    return _core.div__f64_f64(lhs=a, rhs=b, zero=\"error\")\n",
        // A `?` is inside an expression, where Python has no `return`:
        // the function returns what the provider raises.
        concat!(
            "def halved(a, b):\n",
            "    try:\n",
            "        ratio = _core.propagated(checked(a=a, b=b))\n",
            "        return _core.Ok(_core.div__f64_i64(lhs=ratio, rhs=2, zero=\"ieee\"))\n",
            "    except _core.Propagated as _left:\n",
            "        return _left.failure\n",
        ),
        concat!(
            "    if _py.isinstance(outcome, _core.Ok):\n",
            "        return \"fine\"\n",
            "    return outcome.error.message\n",
        ),
        concat!(
            "    if _py.isinstance(outcome, _core.Err):\n",
            "        return outcome.error.code\n",
        ),
        "        return outcome.value\n",
        "    return _core.Err(_core.Error(message=reason, code=\"demo.failed\"))\n",
        "    return _core.Error(message=reason, code=None)\n",
        // The parts of an error are evaluated in the order written.
        concat!(
            "        return _core.Ok(_core.Error(code=_core.propagated(fail(text=\"code\")), ",
            "message=_core.propagated(fail(text=\"message\"))))\n",
        ),
        // In a `when` a failure fails the node.
        "    return _core.succeeded(checked(a=a.value, b=b.value))\n",
    ]);
    // A function with no `?` catches nothing.
    assert_eq!(python.matches("except _core.Propagated").count(), 2);
}

/// Numeric equality within a tolerance: core's observations for
/// NUM-4 and NUM-5. NUM-7: closeness is not transitive.
const CLOSE: (&str, &str) = fixture!("demo.close.hgl");

/// A tolerance that is no tolerance.
const CLOSE_FAILS: (&str, &str) = fixture!(
    "a_failure_names_the_statement_that_failed",
    "demo.close.fails.hgl"
);

/// NUM-4: "Configured `==` accepts absolute (`abs`) and relative
/// (`rel`) tolerance, each defaulting to zero." The tolerances are
/// arguments of the provider's function, which makes the test of
/// NUM-4 and NUM-5 in exact arithmetic.
///
/// EXPR-13: "Evaluate a configured binary expression in written order:
/// left operand, configuration entries in their written order, then
/// right operand, each once." The call is written in that order, and
/// Python evaluates its arguments in the order of the call.
///
/// NUM-11: "In temporal composition they bind during graph
/// construction": a lifted comparison takes its tolerance as
/// configuration, and no input.
#[test]
fn a_tolerance_is_an_argument_written_where_the_source_has_it() {
    let emitted = emit_clean(&[CLOSE]);
    let python = emitted.module("demo.close");
    python.assert_writes(&[
        concat!(
            "def close(a, b, tolerance):\n",
            "    return _core.eq__f64_f64_f64_f64(lhs=a, abs=tolerance, rhs=b, rel=0.0)\n",
        ),
        concat!(
            "def near(a, b, share):\n",
            "    return _core.eq__f64_f64_f64_f64(lhs=a, rel=share, rhs=b, abs=0.0)\n",
        ),
        concat!(
            "def apart(a, b, tolerance):\n",
            "    return _core.ne__f64_f64_f64_f64(lhs=a, abs=tolerance, rhs=b, rel=0.0)\n",
        ),
        concat!(
            "def _lift_0_eq__f64_f64_f64_f64(lhs: _hg.TS[_py.float], rhs: _hg.TS[_py.float], ",
            "abs: _py.float) -> _hg.TS[_py.bool]:\n",
            "    return _core.eq__f64_f64_f64_f64(lhs=lhs.value, rhs=rhs.value, abs=abs, rel=0.0)\n",
        ),
        "    return _lift_0_eq__f64_f64_f64_f64(lhs=a, abs=0.5, rhs=b)\n",
    ]);
}

/// Enums: members, constructions, and an enum as the type of an
/// input, an output and a configuration. `Side` is declared after the
/// function that first names it.
const ENUMS: (&str, &str) = fixture!("demo.enums.hgl");

/// Constructions of a member from a value that is none of the enum's.
const ENUM_FAILS: (&str, &str) = fixture!(
    "a_failure_names_the_statement_that_failed",
    "demo.enums.fails.hgl"
);

/// TYPE-7: "Enum members are distinct typed constants with distinct
/// names. ... Unnumbered numbering starts at zero and increments the
/// previous member; explicit numbers reset the next value. ... A call
/// of the enum's name constructs a member ... `Side(1)` is the member
/// whose number is 1, and `Side("buy")` the member of that exact name.
/// ... a value that is none fails where it is evaluated."
///
/// An enum is a Python `Enum`, which hgraph takes as a scalar. It is
/// written before the functions of its module, whose annotations name
/// it where they are defined.
#[test]
fn an_enum_is_a_python_enum_and_a_scalar() {
    let emitted = emit_clean(&[ENUMS]);
    let python = emitted.module("demo.enums");
    python.assert_writes(&[
        "import enum as _enum\n",
        "Side = _enum.Enum(\"demo.enums::Side\", [(\"buy\", 0), (\"sell\", 5), (\"hold\", 6)])\n",
        // A name that `Enum` keeps has `hgl_` before it, and so has one
        // that would then be taken for it.
        concat!(
            "Odd = _enum.Enum(\"demo.enums::Odd\", [(\"hgl_mro\", 0), (\"hgl__x_\", 1), (\"class\", 2), ",
            "(\"hgl_hgl_mro\", 3), (\"name\", 4)])\n",
        ),
        concat!(
            "def opposite(side):\n",
            "    if _core.eq__T_T(lhs=side, rhs=Side[\"buy\"]):\n",
            "        return Side[\"sell\"]\n",
        ),
        "def numbered(number):\n    return _core.member(Side, number)\n",
        // An input, an output and a configuration of the enum.
        concat!(
            "@_hg.compute_node\n",
            "def chosen(value: _hg.TS[_py.int], other: Side) -> _hg.TS[Side]:\n",
        ),
        concat!(
            "def _lift_1_eq__T_T(lhs: _hg.TS[Side], rhs: Side) -> _hg.TS[_py.bool]:\n",
            "    return _core.eq__T_T(lhs=lhs.value, rhs=rhs)\n",
        ),
        "    return _lift_1_eq__T_T(lhs=side, rhs=Side[\"hold\"])\n",
        // A trace of members.
        concat!(
            "_harness.same(_harness.evaluate(flipped, {\"side\": _hgl_4core._inst_replay__demo_enums__Side}, _hgl_4core._inst_record__demo_enums__Side, side=[Side[\"buy\"], None, Side[\"sell\"], ",
            "Side[\"hold\"]]), [Side[\"sell\"], None, Side[\"buy\"], Side[\"hold\"]])",
        ),
    ]);
    // The enums come before the function that is declared before them.
    let at = |text: &str| python.find(text).expect("it is written");
    assert!(at("Side = _enum.Enum(") < at("def opposite("));
    assert!(at("Odd = _enum.Enum(") < at("def opposite("));
}

/// Structs: declarations, constructions that leave fields out, fields
/// read and assigned at depth, a struct that inherits, a generic one, an
/// operator's candidate on one, two compared as values of their
/// parent, one that holds a NaN, and a node that keeps two in its
/// state. The structs are declared after the functions that name them,
/// and a second module takes a default from the first.
const STRUCTS: [(&str, &str); 2] = [
    fixture!("demo.structs.hgl"),
    fixture!("demo.structs.user.hgl"),
];

/// A default that fails where a construction leaves its field out, and
/// two arguments that each fail.
const STRUCT_FAILS: (&str, &str) = fixture!(
    "a_failure_names_the_statement_that_failed",
    "demo.structs.fails.hgl"
);

/// TYPE-3: "Structs are nominal, with public ordered fields and no
/// methods." TYPE-5: "Its arguments are named, each for a field the
/// struct has, a parent's included, in any order ... A field with a
/// default, its own or one a struct below gives it, may be left out,
/// and then has that default. An optional field with no default may be
/// left out too, and is then null ... Retained results own their data;
/// later mutation of a source cannot change an earlier retained
/// value." EXPR-3: "An assignment may name a place inside a value, at
/// any depth: a field, `point.x = 1`".
///
/// A struct is a class the provider makes, written before the
/// functions of its module. By the owner's rule (the compiler's issue
/// 151), a struct of at most two scalar fields is a value: an
/// assignment to a field gives the place a new one. Any other that a
/// place holds is changed where it is, and what took it took a copy.
///
/// TYPE-3: "Generic applications are invariant by origin and complete
/// canonical arguments": a `Tagged<i64>` and a `Tagged<f64>` are of
/// two classes, and are not equal where they meet as values of their
/// parent. OPS-2: "Two structs are equal when they are of one struct
/// with the same generic arguments ... and each field is equal ...
/// NUM-5 holds inside a value: a struct, a list, a tuple, or a
/// dictionary that holds a NaN is not equal to itself."
#[test]
fn a_struct_is_a_class_and_a_value_of_two_scalars_is_never_changed() {
    let emitted = emit_clean(&STRUCTS);
    let python = emitted.module("demo.structs");
    python.assert_writes(&[
        "import _hgl._core_native as _core\n",
        // Every field the struct has, a parent's first; a Python word
        // has a `_` after it.
        "Base = _core.struct(\"Base\", (\"id\", \"held\", \"rate\"))\n",
        "Order = _core.struct(\"Order\", (\"id\", \"held\", \"rate\", \"side\", \"note\"))\n",
        // A struct of at most two scalar fields is a value.
        "Point = _core.struct(\"Point\", (\"x\", \"y\"), by_value=True)\n",
        "Line = _core.struct(\"Line\", (\"start\", \"end\", \"class_\"))\n",
        "Empty = _core.struct(\"Empty\", (), by_value=True)\n",
        // A struct another inherits from, and a generic one, are not.
        "Shape = _core.struct(\"Shape\", ())\n",
        "Box = _core.struct(\"Box\", (\"value\", \"count\"))\n",
        // A field left out has its default: the struct's own `id = 7`
        // for the parent's field, the parent's for `rate`, and null
        // for an optional field with none.
        concat!(
            "def order():\n",
            "    return Order(held=\"a\", id=7, rate=_py.float(2), side=Side[\"buy\"], note=None)\n",
        ),
        // The arguments in the order written, then the defaults.
        "    point = Point(y=2.0, x=1.0)\n",
        "    line = Line(start=origin(), end=Point(x=1.0, y=0.0), class_=3)\n",
        "def held(base):\n    return base.held\n",
        // An assignment to a field of a value: a new one, and the
        // value given as it is.
        "    next = point\n",
        "    next = _core.with_field(next, x=_core.add__f64_f64(lhs=next.x, rhs=by))\n",
        // A struct of three fields is copied where a place takes it,
        // and changed where it is: a field of it, and a field of a
        // field, a `Point` that is given a new value.
        "    next = _core.copied(line)\n",
        concat!(
            "    next = _core.set_at(next, ((\"f\", \"end\"), (\"f\", \"x\"),), ",
            "lambda: _core.add__f64_f64(lhs=next.end.x, rhs=by))\n",
        ),
        "    next = _core.set_field(next, \"class_\", 4)\n",
        "    return next\n",
        // A candidate of an operator on a struct.
        "def _impl_add__Point_Point(lhs, rhs):\n",
        // A generic struct is made of the class for its arguments.
        "    return _core.applied(Box, \"i64\")(value=value, count=0)\n",
        concat!(
            "same(lhs=_core.applied(Tagged, \"i64\")(tag=1), ",
            "rhs=_core.applied(Tagged, \"f64\")(tag=1.0))",
        ),
        "_core.applied(Tagged, \"list<f64>\")(tag=[odd.x])",
        // A `state` that is a struct.
        "    _state.at = Point(x=0.0, y=0.0)\n",
        "    _state.at = _core.with_field(_state.at, x=_core.add__f64_f64(lhs=_state.at.x, rhs=step.value))\n",
        concat!(
            "    _state.seen = _core.set_field(_state.seen, \"count\", ",
            "_core.add__i64_i64(lhs=_state.seen.count, rhs=1))\n",
        ),
    ]);
    // The structs come before the function that is declared before
    // them.
    let at = |text: &str| python.find(text).expect("it is written");
    assert!(at("Point = _core.struct(") < at("def origin("));
    assert!(at("Empty = _core.struct(") < at("def origin("));

    // UNIT-7: "Imported defaults retain their defining closure,
    // including private references": a default is written where the
    // construction is, and names what it names through its module.
    let user = emitted.module("demo.structs.user");
    user.assert_writes(&[
        "Marker = _core.struct(\"Marker\", (\"at\", \"size\", \"label\"))\n",
        "    return Marker(label=label, at=_hgl_4demo_7structs.origin(), size=base())\n",
    ]);
}

/// Loops and `switch` in value functions and in a node's `when`, and
/// a dictionary written out.
const LOOPS: (&str, &str) = fixture!("demo.loops.hgl");

/// A `switch` with no `default` that no case matches.
const LOOP_FAILS: (&str, &str) = fixture!(
    "a_failure_names_the_statement_that_failed",
    "demo.loops.fails.hgl"
);

/// CTRL-12: "In a value function, `for item in items` runs its body
/// for each element of a list, in index order, or of a set, and `for
/// key, value in entries` for each key and value of a dictionary ...
/// `while condition { ... }` repeats its body while the condition
/// holds ... A `switch` there selects on a `bool`, `i64`, `str`, or
/// enum value." CTRL-6: "There is no fallthrough. An unmatched selector
/// uses default or fails." EXPR-9: a sequence of pairs is a dictionary.
#[test]
fn a_loop_and_a_switch_run_as_they_are_written() {
    let emitted = emit_clean(&[LOOPS]);
    let python = emitted.module("demo.loops");
    python.assert_writes(&[
        concat!(
            "    for item in items:\n",
            "        sum = _core.add__i64_i64(lhs=sum, rhs=item)\n",
        ),
        "    for name, weight in weights.items():\n",
        "    while _core.gt__T_T(lhs=value, rhs=1):\n",
        // The selector is held once, and each case is an `if` on it.
        concat!(
            "def sign(side):\n",
            "    _selected_1 = side\n",
            "    if _selected_1 == Side[\"buy\"]:\n",
            "        return 1\n",
            "    elif _selected_1 == Side[\"sell\"]:\n",
            "        return _core.neg__i64(value=1)\n",
            "    elif _selected_1 == Side[\"hold\"]:\n",
            "        return 0\n",
            "    else:\n",
            "        _core.unmatched(_selected_1)\n",
        ),
        // A `default` is the `else`.
        "    else:\n        text = \"many\"\n",
        // A `switch` in a case has a name of its own for its selector.
        concat!(
            "    if _selected_1 == True:\n",
            "        _selected_2 = number\n",
            "        if _selected_2 == 1:\n",
        ),
        // With no case there is only the `default`.
        concat!(
            "def only(number):\n",
            "    _selected_1 = number\n",
            "    return _core.add__i64_i64(lhs=number, rhs=1)\n",
        ),
        // A dictionary written out.
        "weighted(weights={\"a\": 1.5, \"skip\": 9.0, \"b\": 2.0})",
    ]);
}

/// Two modules that each declare an enum `Side`, with other members,
/// and core's `CmpResult`, which has the name of an enum of hgraph's
/// own, each as the type of a node's input and output.
const SAME_NAMES: [(&str, &str); 2] = [
    fixture!("demo.sides.left.hgl"),
    fixture!("demo.sides.right.hgl"),
];

/// UNIT-4: a type is known by where it is declared, so two modules'
/// enums of one name are two types. hgraph knows a Python enum by its
/// class's name, and so the class is named with its module's path:
/// two `Side`s stay two, and core's `CmpResult` is not hgraph's.
#[test]
fn an_enums_class_is_named_with_its_module() {
    let emitted = emit_clean(&SAME_NAMES);
    for (path, expected) in [
        (
            "_hgl/_m_demo/_m_sides/_m_left/__init__.py",
            "Side = _enum.Enum(\"demo.sides.left::Side\", [(\"buy\", 0), (\"sell\", 1)])\n",
        ),
        (
            "_hgl/_m_demo/_m_sides/_m_right/__init__.py",
            "Side = _enum.Enum(\"demo.sides.right::Side\", [(\"up\", 5), (\"down\", 6)])\n",
        ),
        (
            "_hgl/_m_core/__init__.py",
            "CmpResult = _enum.Enum(\"core::CmpResult\", [(\"less\", 0), ",
        ),
    ] {
        let python = emitted.text(path);
        assert!(
            python.contains(expected),
            "the module has:\n{expected}\nin:\n{python}"
        );
    }
}

/// Elements: a list's, a tuple's and a dictionary's read in brackets
/// and assigned, at depth and inside a struct; core's `get`; and a
/// node that keeps a dictionary in its state.
const ELEMENTS: (&str, &str) = fixture!("demo.elements.hgl");

/// Reads and an assignment outside a list, and a key a dictionary has
/// not.
const ELEMENT_FAILS: (&str, &str) = fixture!(
    "a_failure_names_the_statement_that_failed",
    "demo.elements.fails.hgl"
);

/// EXPR-7: "Brackets read an element of a list or a tuple by an `i64`
/// index, and the value a dictionary has for a key." ERR-6:
/// "Ordinary/atomic dictionary brackets and `at` are strict
/// point-in-time reads; absence fails. `get` with no fallback or null
/// fallback yields contextual absence." EXPR-3: an assignment's value
/// is "of a list's element type, under an `i64` index; of a
/// dictionary's value type, under a key of its key type; or of a
/// tuple's position, under a constant index. A key the dictionary has
/// not is added. An index outside a list fails where it is evaluated."
///
/// A list, a tuple and a dictionary are never changed: an assignment
/// to an element gives the place's root a new value, as one to a field
/// does.
#[test]
fn an_element_is_read_and_assigned_through_the_provider() {
    let emitted = emit_clean(&[ELEMENTS]);
    let python = emitted.module("demo.elements");
    python.assert_writes(&[
        "def second(items):\n    return _core.item(items, 1)\n",
        "def last(pair):\n    return _core.item(pair, 1)\n",
        "def size(sizes, name):\n    return _core.entry(sizes, name)\n",
        "    return _core.get_dict_K__T__K(store=sizes, key=name)\n",
        // A compound assignment reads the place first. An element of
        // the list a `var` holds itself is assigned where the list is.
        concat!(
            "    next = _core.set_item(next, 1, ",
            "_core.add__i64_i64(lhs=_core.item(next, 1), rhs=1))\n",
        ),
        // A tuple is small: a new one is made.
        "    next = _core.with_item(next, 1, \"b\")\n",
        // An entry of the dictionary a `var` holds itself, read by a
        // function that is lent it and then assigned where it is.
        "        before = _core.get_dict_K__T__K(store=seen, key=name)\n",
        "            seen = _core.set_entry(seen, name, 1)\n",
        // An element of a field, and an element of an element.
        "    next = _core.set_at(next, ((\"f\", \"sizes\"), (\"k\", \"top\"),), lambda: 1)\n",
        "    rows = _core.set_at(rows, ((\"i\", 1), (\"i\", 0),), lambda: 9)\n",
        // A `state` that is a dictionary.
        "        _state.seen = _core.set_entry(_state.seen, name.value, 1)\n",
        // `[]` where a dictionary is expected.
        "    seen = {}\n",
    ]);
}

/// `<=>` on numbers, strings and in a graph, and the Boolean operators
/// called by name.
const COMPARE: (&str, &str) = fixture!("demo.compare.hgl");

/// NUM-6: "The numeric full-comparison operator `compare_`, selected
/// by `<=>`, returns an ordinary comparison outcome, a `CmpResult`:
/// `less`, `equivalent`, `greater`, or `unordered`. A NaN operand
/// yields `unordered`; equivalent numeric values, including opposite
/// signed zeros, yield `equivalent`." OPS-3: "Only a numeric
/// comparison yields `unordered`."
///
/// A native candidate that gives an enum gives its member's name, and
/// the member is read from the enum the program declares: core's
/// `CmpResult`, through core's module.
#[test]
fn a_full_comparison_gives_a_member_of_cores_enum() {
    let emitted = emit_clean(&[COMPARE]);
    let python = emitted.module("demo.compare");
    python.assert_writes(&[
        "import _hgl._m_core as _hgl_4core\n",
        concat!(
            "def ordered(lhs, rhs):\n",
            "    return _core.member(_hgl_4core.CmpResult, ",
            "_core.compare__T_T(lhs=lhs, rhs=rhs))\n",
        ),
        "_core.compare__i64_f64(lhs=lhs, rhs=rhs))\n",
        // Lifted over connections, the node gives the member too.
        concat!(
            "def _lift_0_compare__T_T(lhs: _hg.TS[_py.float], rhs: _hg.TS[_py.float]) ",
            "-> _hg.TS[_hgl_4core.CmpResult]:\n",
            "    return _core.member(_hgl_4core.CmpResult, ",
            "_core.compare__T_T(lhs=lhs.value, rhs=rhs.value))\n",
        ),
        // OPS-5: the Boolean operators, called by name.
        "    return _core.and__bool_bool(lhs=lhs, rhs=rhs)\n",
        "    return _core.or__bool_bool(lhs=lhs, rhs=rhs)\n",
        "    return _core.not__bool(value=value)\n",
    ]);
}

/// A node whose `stop` reads an input under its guard, and one whose
/// `stop` would fail where the guard holds, given an input that never
/// is valid.
const STOPS: (&str, &str) = fixture!("demo.stops.hgl");

/// TIME-7: "`out` is read in a `when` and in `stop`". A node whose
/// `stop` reads what it last published, under `valid(out)`: the run
/// ends well only where that is `expected`, since the division is by
/// zero where it is not.
const OUT_IN_STOP: (&str, &str) = fixture!("out_in_stop", "demo.ended.hgl");

/// TIME-7: hgraph gives a node's `stop` the node's output where it
/// names it, as it gives the node: `out` is `_output` there too.
#[test]
fn a_stop_is_given_the_nodes_output() {
    let emitted = emit_clean(&[OUT_IN_STOP]);
    let python = emitted.module("demo.ended");
    let expected = concat!(
        "@totalled.stop\n",
        "def _stop_totalled(expected: _py.int, value: _hg.TS[_py.int], ",
        "_state: _hg.STATE = None, _output: _hg.TS_OUT[_py.int] = None):\n",
        "    if _output.valid:\n",
    );
    assert!(
        python.contains(expected),
        "the module has:\n{expected}\nin:\n{python}"
    );
    // TIME-7: `out` read holds what was published, and fails with
    // `value.unset_read` before it holds a value.
    let read = out_read("_output.value", "_output.valid");
    assert!(
        python.contains(&format!(
            "if _core.eq__i64_i64_f64_f64(lhs={read}, rhs=expected"
        )),
        "{python}"
    );
}

/// TIME-6: a `cache`. TIME-8, core's CAP-1 to CAP-3 and CAP-6: a node
/// injects `logger`, `clock` and `global_state`, reads a capability's
/// properties, gives one to a function, and reads and writes the store.
const CAPABILITIES: (&str, &str) = fixture!("demo.caps.hgl");

/// A node that reads a key the store has not.
const CAPABILITIES_FAIL: (&str, &str) = fixture!(
    "a_failure_names_the_statement_that_failed",
    "demo.caps.fails.hgl"
);

/// TIME-6: "Cache is reconstructible node-local value data, initialized
/// on every start": a field of the node's state, as a `state` is.
/// TIME-8: a capability is the parameter hgraph gives it as, to the
/// node, its `start` and its `stop`. CAP-1: a property is read through
/// the provider; CAP-2: `log` is the provider's; CAP-3: the clock's
/// properties; CAP-6: `get`, `has` and `put` are core's candidates for
/// the store.
#[test]
fn a_cache_and_a_capability_are_the_nodes_own() {
    let emitted = emit_clean(&[CAPABILITIES]);
    let python = emitted.module("demo.caps");
    python.assert_writes(&[
        concat!(
            "@_hg.compute_node\n",
            "def counted(value: _hg.TS[_py.int], _state: _hg.STATE = None, ",
            "_logger: _hg.LOGGER = None) -> _hg.TS[_py.int]:\n",
            "    _state.seen = _core.add__i64_i64(lhs=_state.seen, rhs=1)\n",
            "    _hgl_4core.log_info(logger=_logger, message=\"seen 100% of it\")\n",
            "    return _state.seen\n",
        ),
        concat!(
            "@counted.start\n",
            "def _start_counted(_state: _hg.STATE = None, _logger: _hg.LOGGER = None):\n",
            "    _state.seen = 0\n",
        ),
        concat!(
            "@counted.stop\n",
            "def _stop_counted(value: _hg.TS[_py.int], _state: _hg.STATE = None, ",
            "_logger: _hg.LOGGER = None):\n",
            "    if _core.gt__T_T(lhs=_state.seen, rhs=1):\n",
            "        _core.fn_log(logger=_logger, level=_hgl_4core.LogLevel[\"debug\"], ",
            "message=\"more than one\")\n",
        ),
        concat!(
            "def stamped(value: _hg.TS[_py.int], _clock: _hg.EvaluationClock = None) ",
            "-> _hg.TS[_dt.datetime]:\n",
            "    return _core.Clock_evaluation_time(_clock)\n",
        ),
        "def remembered(value: _hg.TS[_py.int], _store: _hg.GlobalState = None)",
        "_core.get_GlobalState_str(store=_store, key=\"last\")",
        "_core.has_GlobalState_str(store=_store, key=\"last\")",
        "    _core.put_GlobalState_str_V(store=_store, key=\"last\", value=value.value)\n",
        "_core.member(_hgl_4core.LogLevel, _core.Logger_level(_logger))",
    ]);
    // Core's `log_info` is a function of core's module, which calls
    // the provider's `log`.
    let core = emitted.module("core");
    assert!(
        core.contains(concat!(
            "def log_info(logger, message):\n",
            "    _core.fn_log(logger=logger, level=LogLevel[\"info\"], message=message)\n",
        )),
        "{core}"
    );
}

/// SOURCE-1 to SOURCE-4: sources. One with no declaration and no hook;
/// one with both hooks and its yields in a block; one that keeps a
/// `cache` from one `yield` to the next; targets that are durations and
/// instants; and a source wired into a graph.
const SOURCES: (&str, &str) = fixture!("demo.sources.hgl");

/// SOURCE-1: a source is a generator that holds its statements, a node
/// that resumes it, with the source's `start` and `stop`, and a graph
/// that wakes the node once as it starts. SOURCE-2: `yield time:
/// value` gives the generator's caller the time, then the value.
#[test]
fn a_source_is_a_generator_a_node_resumes() {
    let emitted = emit_clean(&[SOURCES]);
    let python = emitted.module("demo.sources");
    python.assert_writes(&[
        concat!(
            "def _yields_two_values(_state):\n",
            "    yield (_dt.timedelta(microseconds=1), 7)\n",
            "    yield (_dt.timedelta(microseconds=1), 9)\n",
        ),
        concat!(
            "@_hg.compute_node\n",
            "def _source_two_values(_begin: _hg.TS[_py.bool], _state: _hg.STATE = None, ",
            "_clock: _hg.EvaluationClock = None, _scheduler: _hg.SCHEDULER = None, ",
            "_output: _hg.TS_OUT[_py.int] = None) -> _hg.TS[_py.int]:\n",
            // SOURCE-3: "On wake it publishes and resumes after yield":
            // the provider publishes through `_output`, then resumes.
            "    _core.resume(_state._source, _clock.evaluation_time, _scheduler, _output)\n",
        ),
        concat!(
            "@_source_two_values.start\n",
            "def _start__source_two_values(_state: _hg.STATE = None):\n",
            "    _state._source = _core.Source(_yields_two_values(_state))\n",
        ),
        concat!(
            "@_hg.graph\n",
            "def two_values() -> _hg.TS[_py.int]:\n",
            "    return _source_two_values(_hg.const(True))\n",
        ),
        // A capability is the node's for one evaluation: the node puts
        // it in its state, where the generator reads it.
        concat!(
            "def _yields_absolute(_state):\n",
            "    yield (_core.add__datetime_duration(lhs=_core.Clock_evaluation_time(",
            "_state._clock), rhs=_dt.timedelta(microseconds=3)), 1)\n",
        ),
        "    _state._clock = _clock\n",
        // A `cache` is a field of the state the generator is given,
        // which `stop` reads.
        concat!(
            "def _yields_counter(every, limit, _state):\n",
            "    while _core.lt__T_T(lhs=_state.count, rhs=limit):\n",
            "        _state.count = _core.add__i64_i64(lhs=_state.count, rhs=1)\n",
            "        yield (every, _state.count)\n",
        ),
        concat!(
            "@_source_counter.start\n",
            "def _start__source_counter(every: _dt.timedelta, limit: _py.int, ",
            "_state: _hg.STATE = None, _logger: _hg.LOGGER = None):\n",
            "    _state.count = 0\n",
            "    _state._source = _core.Source(_yields_counter(every, limit, _state))\n",
        ),
        concat!(
            "@_source_greeting.stop\n",
            "def _stop__source_greeting(every: _dt.timedelta, _state: _hg.STATE = None, ",
            "_logger: _hg.LOGGER = None):\n",
            "    _hgl_4core.log_info(logger=_logger, message=\"greeting ends\")\n",
        ),
        // A block's last expression is a statement of the generator.
        concat!(
            "def _yields_trailed(first, _state):\n",
            "    yield (_dt.timedelta(microseconds=1), first)\n",
            "    _hgl_4core.log_info(logger=_state._logger, message=\"done\")\n",
        ),
        concat!(
            "def _yields_guarded(wanted, _state):\n",
            "    if wanted:\n",
            "        yield (_dt.timedelta(microseconds=1), 1)\n",
        ),
        // The generator is made before the source's `start` runs.
        concat!(
            "def _start__source_started(quiet: _py.bool, _state: _hg.STATE = None, ",
            "_logger: _hg.LOGGER = None):\n",
            "    _state._source = _core.Source(_yields_started(quiet, _state))\n",
            "    if quiet:\n",
            "        return\n",
        ),
        // SOURCE-4: an unbounded `while`, and a bare `return`.
        concat!(
            "    while True:\n",
            "        if _core.eq__i64_i64_f64_f64(lhs=_state.count, rhs=limit, abs=0.0, rel=0.0):\n",
            "            return\n",
        ),
        // A graph wires a source as any function.
        "def wired() -> _hg.TS[_py.int]:\n    return doubled(value=two_values())\n",
    ]);
}

/// FN-15: a `var` parameter. A call gives it a place and assigns it:
/// a `var`, a field or an element of one, a node's `state` and `cache`.
const PLACES: (&str, &str) = fixture!("demo.places.hgl");

/// FN-15: "what it holds when the function returns is what the place
/// is given". Python has no place to give a value to: a function with
/// `var` parameters returns its result and then what each holds, and
/// the call, the whole of a statement, assigns the places after it, in
/// the order of the parameters.
#[test]
fn a_call_assigns_the_places_its_var_parameters_are_given() {
    let emitted = emit_clean(&[PLACES]);
    let python = emitted.module("demo.places");
    python.assert_writes(&[
        // FN-15: "A function that fails leaves the place holding what
        // its parameter held when it failed": what fails is given
        // back in the result's place, with what each parameter holds.
        concat!(
            "def bump(count, by):\n",
            "    try:\n",
            "        count = _core.add__i64_i64(lhs=count, rhs=by)\n",
            "        return (None, count)\n",
            "    except _py.Exception as _failed:\n",
            "        return (_core.Failed(_failed), count)\n",
        ),
        concat!(
            "def take(count):\n",
            "    try:\n",
            "        before = count\n",
            "        count = 0\n",
            "        return (before, count)\n",
        ),
        "    return (None, left, right)\n",
        "        return (_core.Failed(_failed), left, right)\n",
        concat!(
            "def early(count, halt):\n",
            "    try:\n",
            "        if halt:\n",
            "            return (_core.neg__i64(value=1), count)\n",
        ),
        // The call, then the place, and then what failed raised again.
        concat!(
            "        _given = bump(count=sum, by=value)\n",
            "        sum = _given[1]\n",
            "        _core.raise_if_failed(_given[0])\n",
        ),
        // A `var` parameter given on is a place of this function.
        concat!(
            "def through(count):\n",
            "    try:\n",
            "        _given = bump(count=count, by=1)\n",
            "        count = _given[1]\n",
            "        _core.raise_if_failed(_given[0])\n",
            "        return (None, count)\n",
        ),
        // A place inside a value: a field of a struct that is no value
        // is assigned where the struct is.
        concat!(
            "    _given = set_first(items=book.orders, value=5)\n",
            "    book = _core.set_field(book, \"orders\", _given[1])\n",
            "    _core.raise_if_failed(_given[0])\n",
            "    _given = bump(count=_core.item(book.orders, 1), by=3)\n",
            "    book = _core.set_at(book, ((\"f\", \"orders\"), (\"i\", 1),), lambda: _given[1])\n",
        ),
        // The call's own result is the first of what it gives.
        concat!(
            "    _given = take(count=held)\n",
            "    held = _given[1]\n",
            "    _core.raise_if_failed(_given[0])\n",
            "    was = _given[0]\n",
        ),
        concat!(
            "    _given = both(left=a, right=b)\n",
            "    a = _given[1]\n",
            "    b = _given[2]\n",
        ),
        // An index of a place is evaluated once, before the call, and
        // every argument with it, each in the order written (EXPR-2).
        concat!(
            "    _arg_1 = i\n",
            "    _at_1 = i\n",
            "    _arg_2 = _core.item(items, _at_1)\n",
            "    _given = change(index=_arg_1, item=_arg_2)\n",
            "    i = _given[1]\n",
            "    items = _core.set_item(items, _at_1, _given[2])\n",
        ),
        concat!(
            "    _arg_1 = _core.floordiv__i64_i64(lhs=1, rhs=zero, zero=\"fail\")\n",
            "    _at_1 = _core.entry(indices, \"missing\")\n",
            "    _arg_2 = _core.item(items, _at_1)\n",
            "    _given = apply(value=_arg_1, item=_arg_2)\n",
        ),
        // What an index is of is read, and held, before the index is
        // evaluated. Where the place is assigned it is read again.
        concat!(
            "    _at_1 = key\n",
            "    _in_1 = _core.entry(rows, _at_1)\n",
            "    _at_2 = _core.floordiv__i64_i64(lhs=1, rhs=zero, zero=\"fail\")\n",
            "    _arg_1 = _core.item(_in_1, _at_2)\n",
            "    _arg_2 = 1\n",
            "    _given = bump(count=_arg_1, by=_arg_2)\n",
            "    rows = _core.set_at(rows, ((\"k\", _at_1), (\"i\", _at_2),), lambda: _given[1])\n",
        ),
        // A node's `state` and `cache` are places.
        concat!(
            "    _given = bump(count=_state.seen, by=1)\n",
            "    _state.seen = _given[1]\n",
        ),
    ]);
}

/// FN-15: a call that assigns a place is written with its assignments
/// after it, which a statement has room for. One inside an expression
/// is reported.
#[test]
fn a_place_call_inside_an_expression_is_reported() {
    let emitted = emit(&[fixture!(
        "a_place_call_inside_an_expression_is_reported",
        "demo.inside.hgl"
    )]);
    assert_eq!(
        emitted.reported,
        [
            "demo.inside.hgl:8:12 target.unsupported: the hgraph target does not write a call that assigns a place, inside an expression yet",
            "demo.inside.hgl:8:25 target.unsupported: the hgraph target does not write a call that assigns a place, inside an expression yet",
        ]
    );
}

/// TYPE-14: a temporal `T?`. TIME-7: "`return null` for an `i64?`
/// result [publishes] a null". TYPE-16: a nullable's delta "holds what
/// was published, a null among them".
const NULLABLE: (&str, &str) = fixture!("demo.nullable.hgl");

/// TYPE-14: a temporal `T?` is hgraph's value that holds one `T` or is
/// unset, `TS[T | None]`. A null is `None` as a value, and hgraph's
/// `Null` where it is published: to hgraph a node that returns `None`
/// publishes nothing. TYPE-16: a nullable's delta is hgraph's own.
#[test]
fn a_nullable_is_a_value_that_may_be_unset() {
    let emitted = emit_clean(&[NULLABLE]);
    let python = emitted.module("demo.nullable");
    python.assert_writes(&[
        concat!(
            "@_hg.compute_node\n",
            "def publish_nullable(command: _hg.TS[_py.int]) -> _hg.TS[_py.int | None]:\n",
            "    if _core.eq__i64_i64_f64_f64(lhs=command.value, rhs=0, abs=0.0, rel=0.0):\n",
            "        return _core.published(None)\n",
            "    return _core.published(command.value)\n",
        ),
        "def read_back(value: _hg.TS[_py.int | None]) -> _hg.TS[_py.int]:\n",
        // TYPE-16: `delta_value` of a nullable is hgraph's own delta.
        "def recorded(value: _hg.TS[_py.int | None]) -> _hg.TS[_py.int | None]:\n    if value.modified:\n        return value.delta_value\n",
        "    change = value.delta_value\n",
        "_core.unpublished(change)",
        // `delta<i64?>()` is a published null.
        "        return _hg.Null\n",
        "    return _core.published(command.value)\n",
        // TIME-7: an assignment to `out` publishes.
        "        _output.value = _core.published(None)\n",
        // A source's `yield`.
        "    yield (_dt.timedelta(microseconds=1), _core.published(None))\n",
        // A lifted value function that gives a nullable.
        "-> _hg.TS[_py.int | None]:\n    return _core.published(none_of(value=value.value))\n",
        // TST-6: `null` in a trace is a published null, and `_` silence.
        concat!(
            "_harness.evaluate(read_back, {\"value\": _hgl_4core._inst_replay__i64_}, _hgl_4core._inst_record__i64, ",
            "value=[_hg.Null, None, 7, _hg.Null, None])",
        ),
        "[7, _hg.Null, None, _hg.Null, 9]",
    ]);
}

/// EXPR-3 on hgraph, for a dictionary and for an element of a list: a
/// `var`, a `state` or a `cache` is changed where it is, and what took
/// its value before is as it was (LIST-3's rule, for every collection).
const ENTRIES: (&str, &str) = fixture!("demo.entries.hgl");

/// EXPR-3: "An assignment may name a place inside a value". An entry
/// of the dictionary, and an element of the list, that a `var` of the
/// function holds itself is assigned where the collection is,
/// `_core.set_entry` and `_core.set_item`: no entry is copied for it.
/// The collection is copied as a list that grows is: by a `let` of
/// what then changes, and where it is kept. A function is given it as
/// it is.
#[test]
fn an_entry_and_an_element_are_assigned_where_the_collection_is() {
    let emitted = emit_clean(&[ENTRIES]);
    let python = emitted.module("demo.entries");
    python.assert_writes(&[
        // Read by a function that gives an entry, and so is lent the
        // dictionary; then assigned where it is.
        "        counts = _core.set_entry(counts, name, _core.add__i64_i64(lhs=count_of(counts=counts, name=name), rhs=1))\n",
        "    found = _core.get_dict_K__T__K(store=counts, key=name)\n",
        // A `let` of a dictionary that then changes takes a copy.
        concat!(
            "    before = _core.copied(book)\n",
            "    book = _core.set_entry(book, \"b\", 2)\n",
        ),
        // A function is given the dictionary as it is, and gives back
        // a copy of what it was given.
        "    through = same(value=book)\n",
        "def same(value):\n    return _core.copied(value)\n",
        // An element of a list, and a compound assignment of one.
        "    items = _core.set_item(items, 1, 9)\n",
        "    items = _core.set_item(items, 1, _core.add__i64_i64(lhs=_core.item(items, 1), rhs=1))\n",
        // A `var` parameter's element: the caller's list, changed
        // where it is.
        "    items = _core.set_item(items, 0, value)\n",
        // An entry that is a list: the dictionary is changed where it
        // is, and the list inside it is given a new value.
        "    rows = _core.set_at(rows, ((\"k\", \"a\"), (\"i\", 0),), lambda: 2)\n",
        // What a dictionary keeps of another place's list is a copy.
        "    rows = _core.set_entry(rows, \"b\", _core.copied(items))\n",
    ]);
}

/// EXPR-3 and LIST-1 on hgraph for a collection inside another value: a
/// struct's dictionary and list, a list in a dictionary, a list in a
/// list. Each is changed where it is, and what took it before is as it
/// was.
const INSIDE: (&str, &str) = fixture!("demo.inside.hgl");

/// EXPR-3: "An assignment may name a place inside a value, at any
/// depth". Where the place is inside the value a `var` or a `state` of
/// the function holds, with a collection on the way, the provider goes
/// in: `_core.set_at` for an assignment, and `_core.given_at` for a
/// native function of core that is given the place (LIST-1, FN-15).
/// Each collection on the way is changed where it is. A collection read
/// out of a value is read as it is, and is copied where it is kept: by
/// a `let` of what then changes, and by a result.
#[test]
fn a_collection_inside_a_value_is_changed_where_it_is() {
    let emitted = emit_clean(&[INSIDE]);
    let python = emitted.module("demo.inside");
    python.assert_writes(&[
        // A struct's dictionary and list: the way in, and the value as
        // it is asked for.
        "        book = _core.set_at(book, ((\"f\", \"bids\"), (\"k\", i),), lambda: _core.mul__i64_i64(lhs=i, rhs=2))\n",
        concat!(
            "        _given = _core.given_at(book, ((\"f\", \"orders\"),), _core.fn_append, \"values\", lambda: _py.dict(item=i))\n",
            "        book = _given[1]\n",
        ),
        // A key that is an expression is evaluated once, before.
        concat!(
            "        _key_1 = _core.mod__i64_i64(lhs=i, rhs=2, zero=\"fail\")\n",
            "        _given = _core.given_at(groups, ((\"k\", _key_1),), _core.fn_append, \"values\", lambda: _py.dict(item=i))\n",
        ),
        // Two steps in.
        "    rows = _core.set_at(rows, ((\"i\", 1), (\"i\", 0),), lambda: 7)\n",
        // A node's `state`.
        "    _state.book = _core.set_at(_state.book, ((\"f\", \"bids\"), (\"k\", value.value),), lambda: _core.mul__i64_i64(lhs=value.value, rhs=2))\n",
        // Read through, and lent: unmarked.
        "total(values=_state.book.orders)",
        "_core.entry(_state.book.bids, value.value)",
        "    taken = orders_of(book=book)\n",
        // A `let` of what its place then changes takes a copy.
        "    before = _core.copied(book)\n",
        "    orders = _core.copied(book.orders)\n",
        "    group = _core.copied(_core.entry(groups, \"a\"))\n",
        // A result is the caller's to keep: a copy of a part of what
        // the function was given.
        "    return _core.copied(book.orders)\n",
        // A loop whose body changes nothing of what it runs over reads
        // each entry where it is.
        "    for row in rows:\n",
        // A field that is no collection, of a struct that is no value:
        // the struct is changed where it is.
        "    book = _core.set_field(book, \"name\", \"b\")\n",
    ]);
}

/// FN-15, GEN-6: an operator whose value candidates each take a place.
/// The operator's declaration marks none; the candidate a call
/// resolves to says which of its arguments are places.
const PUSHES: (&str, &str) = fixture!("demo.pushes.hgl");

/// FN-15: "So may a value candidate of an operator (GEN-6) ... a call of
/// an operator is resolved as any other, and the candidate it resolves
/// to says which of its arguments are places." A candidate with a `var`
/// parameter is written as a function with one is: it returns its
/// result and then what each holds, and the call assigns the places. A
/// place's own list is given as it is, and what the call did to it
/// stays where the call fails.
#[test]
fn a_candidate_that_takes_a_place_is_called_as_a_function_that_does() {
    let emitted = emit_clean(&[PUSHES]);
    let python = emitted.module("demo.pushes");
    python.assert_writes(&[
        concat!(
            "def _impl_push_list_i64__i64(values, item):\n",
            "    try:\n",
            "        _given = _core.fn_append(values=values, item=item)\n",
            "        values = _given[1]\n",
            "        return (None, values)\n",
        ),
        // The list's candidate is given the place's own list.
        concat!(
            "    _given = _impl_push_list_i64__i64(values=entries, item=2)\n",
            "    entries = _given[1]\n",
        ),
        // The dictionary's candidate, selected by the argument's type.
        concat!(
            "    _given = _impl_push_dict_str__i64__str(values=seen, item=\"a\")\n",
            "    seen = _given[1]\n",
        ),
        // A place inside a value: a struct of a list is no value, and
        // is changed where it is.
        concat!(
            "    _given = _impl_push_list_i64__i64(values=book.orders, item=2)\n",
            "    book = _core.set_field(book, \"orders\", _given[1])\n",
            "    _core.raise_if_failed(_given[0])\n",
        ),
    ]);
}

/// A tenor on hgraph (core's TEN-1 to TEN-9): its literal, its own
/// arithmetic, order and text, and a date moved by one, on a calendar
/// and under a rolling rule. TEN-6: a count of business days starts from
/// a business day.
const MOVES: (&str, &str) = fixture!("demo.moves.hgl");

/// TEN-1 to TEN-9 on hgraph. A tenor literal is the provider's `Tenor`
/// by its counts (LEX-28, TYPE-17), and its own operators the
/// provider's functions. `date + tenor` is core's HGL, written for the
/// calendar and the rolling rule it is given, each an argument of the
/// candidate; core's functions ask the program's own `business_day`.
#[test]
fn a_tenor_moves_a_date_on_a_calendar_and_under_a_rule() {
    let emitted = emit_clean(&[MOVES]);
    let python = emitted.module("demo.moves");
    python.assert_writes(&[
        // LEX-28: a year is twelve months, a week seven days, and
        // business days a count of their own.
        "_core.Tenor(1, 0)",
        "_core.Tenor(0, 14)",
        "_core.Tenor(18, 0)",
        "_core.Tenor(business=2)",
        // TEN-2, TEN-3, TEN-9: the provider's.
        "_core.add__tenor_tenor(lhs=_core.Tenor(6, 0), rhs=_core.Tenor(6, 0))",
        "_core.neg__tenor(value=_core.Tenor(1, 2))",
        "_core.lt__tenor_tenor(lhs=_core.Tenor(1, 0), rhs=_core.Tenor(2, 0))",
        "_core.compare__tenor_tenor(lhs=_core.Tenor(1, 0), rhs=_core.Tenor(0, 45))",
        "_core.convert_tenor_to_str(value=_core.Tenor(0, 13))",
        // TEN-4: core's candidate, with no option.
        "_hgl_4core._impl_add__date_tenor(lhs=_dt.date(2026, 1, 31), rhs=_core.Tenor(1, 0))",
        // TEN-5: written for the calendar it is given.
        "_hgl_4core._inst__impl_add__date_tenor_C__demo_moves__Closed(lhs=trade, calendar=calendar, rhs=_core.Tenor(business=2))",
        // TEN-7, TEN-8: and for the rolling rule.
        "_hgl_4core._inst__impl_add__date_tenor_R__core__Rolling(lhs=spot, rolling_rule=_hgl_4core.Rolling[\"modified_following\"], rhs=_core.Tenor(3, 0))",
    ]);
    // Core's own functions, written for the program's calendar, ask the
    // program's `business_day`.
    let core = emitted.module("core");
    assert!(
        core.contains(
            "_hgl_4demo_5moves._impl_business_day_Closed_date(calendar=calendar, day=at)"
        ),
        "{core}"
    );
}

/// TYPE-14: a temporal `T?` is written for a scalar and an enum, which
/// publish whole. A nullable set is not: a set publishes its changes,
/// which hgraph's value that may be unset would not gather. It is
/// reported where it is an input and where it is a result.
#[test]
fn a_nullable_set_is_not_written_as_a_temporal_type() {
    let emitted = emit(&[fixture!(
        "a_nullable_set_is_not_written_as_a_temporal_type",
        "demo.maybe.hgl"
    )]);
    assert_eq!(
        emitted.reported,
        [
            "demo.maybe.hgl:2:10 target.unsupported: the hgraph target does not write the type `set<i64>?` yet",
            "demo.maybe.hgl:5:4 target.unsupported: the hgraph target does not write the type `set<i64>?` yet",
        ]
    );
}

/// Core's LIST-1 to LIST-3: `append` and `extend` give a list more
/// entries. Each takes its list as a `var` parameter, so the call
/// assigns the place, and a value taken from it before is as it was.
const LISTS: (&str, &str) = fixture!("demo.lists.hgl");

/// LIST-1, LIST-2: `append` and `extend` are the provider's functions.
/// LIST-3: each gives back the list its place is to hold, a new one,
/// and the call assigns the place (FN-15).
#[test]
fn a_list_grows_through_cores_append_and_extend() {
    let emitted = emit_clean(&[LISTS]);
    let python = emitted.module("demo.lists");
    python.assert_writes(&[
        // A node's `cache` is the place.
        concat!(
            "    _given = _core.fn_append(values=_state.seen, item=value.value)\n",
            "    _state.seen = _given[1]\n",
        ),
        concat!(
            "    _given = _core.fn_extend(values=entries, items=more)\n",
            "    entries = _given[1]\n",
            "    _given = _core.fn_append(values=entries, item=0)\n",
            "    entries = _given[1]\n",
        ),
    ]);
}

/// A generic node whose configuration is of its generic parameter:
/// what `T` is at a call decides how the configuration is given.
const KEPT: (&str, &str) = fixture!("demo.tagged.hgl");

/// GEN-2: a generic temporal function is written once for each set of
/// generic arguments, and its configuration is of the type the
/// arguments make it. A list, a tuple or a dictionary is given to a
/// node inside the provider's `Held`, so a parameter declared a `T` is
/// given inside one where `T` is a list, and as it is where `T` is an
/// `i64`: by a graph's call, by an eval, and where a default stands in
/// for an argument left out.
#[test]
fn configuration_of_a_generic_type_is_held_by_what_the_type_is() {
    let emitted = emit_clean(&[KEPT]);
    let python = emitted.module("demo.tagged");
    python.assert_writes(&[
        // A node keeps what a graph gives it: a copy of the graph's
        // parameter, which its caller may go on changing.
        "    return _inst_tagged__list_i64_(value=value, tag=_core.Held(_core.copied(tags)))\n",
        "    return _inst_optional__list_i64_(value=value, tag=_core.Held(None))\n",
        // The node written for a list takes it out; the one for an
        // `i64` has nothing to take out.
        concat!(
            "def _inst_tagged__list_i64_(value: _hg.TS[_py.int], tag: _py.object) -> _hg.TS[_py.int]:\n",
            "    tag = tag.value\n",
        ),
        concat!(
            "def _inst_tagged__i64(value: _hg.TS[_py.int], tag: _py.int) -> _hg.TS[_py.int]:\n",
            "    return value.value\n",
        ),
        "value=[7], tag=_core.Held([1, 2]))",
        "value=[7], tag=_core.Held({\"a\": 1}))",
        "_harness.evaluate(_inst_tagged__i64, {\"value\": _hgl_4core._inst_replay__i64}, _hgl_4core._inst_record__i64, value=[7], tag=1)",
        "_harness.evaluate(_inst_optional__tuple_i64__str_, {\"value\": _hgl_4core._inst_replay__i64}, _hgl_4core._inst_record__i64, value=[7], tag=_core.Held(None))",
    ]);
}

/// LIST-3: "a value taken from the place before the call is as it
/// was". A `let`, a parameter and whatever else is given a place's
/// list is given the list itself; the place copies it before it grows
/// where something else may hold it, and grows it where it is
/// otherwise.
const SHARED: (&str, &str) = fixture!("demo.shared.hgl");

/// LIST-3 on hgraph, by the owner's rule (the compiler's issue 151):
/// "all var mutations are mutations, not copies." A `var` changes its
/// list where it is, with no check and no copy. A copy is made where a
/// reference is made to what a place holds and the place then changes,
/// a `let` and a loop; where a call gives a place to a `var` parameter
/// and what it holds to another; and where a reference is kept: by a
/// place, by a native function that puts it in one, and by a result.
/// What is written out, and a function's own result, is copied
/// nowhere.
#[test]
fn a_list_is_changed_where_it_is_and_copied_where_it_is_kept() {
    let emitted = emit_clean(&[SHARED]);
    let python = emitted.module("demo.shared");
    python.assert_writes(&[
        // A `let` of a list its `var` then grows takes a copy.
        concat!(
            "    before = _core.copied(entries)\n",
            "    _given = _core.fn_append(values=entries, item=3)\n",
            "    entries = _given[1]\n",
        ),
        // An argument is a reference: nothing is copied for a call.
        "total(values=entries)",
        "longer(values=entries)",
        "_core.eq__T_T(lhs=entries, rhs=[1, 2, 3])",
        "    through = same(value=entries)\n",
        "save(store=_store, values=_state.seen)",
        // A `var` keeps what it is given: a copy of its parameter.
        "    mine = _core.copied(values)\n",
        // A result is the caller's to keep: a copy of what the
        // function was given, and its own `var` as it is.
        "def same(value):\n    return _core.copied(value)\n",
        "    return out\n",
        // A `var` parameter is the place itself, and what the function
        // keeps in it is a copy.
        "def keep(slot, value):\n    try:\n        slot = _core.copied(value)\n",
        concat!(
            "    _given = keep(slot=slot, value=source)\n",
            "    slot = _given[1]\n",
        ),
        // A `let` of an element of a list that then changes.
        "    row = _core.copied(_core.item(rows, 0))\n",
        // A place inside a value is changed where it is.
        concat!(
            "    _given = _core.given_at(rows, ((\"i\", 0),), _core.fn_append, \"values\", lambda: _py.dict(item=2))\n",
            "    rows = _given[1]\n",
        ),
        // What a list and a struct keep of another place's list.
        "    _given = _core.fn_append(values=rows, item=_core.copied(entries))\n",
        "    held = _core.copied(Holder(items=entries))\n",
        // A loop over a list its own body grows runs over a copy.
        "    for item in _core.copied(values):\n",
        // The owner's first case: the call has a `var` reference to
        // what it is also given.
        "    _given = _core.fn_extend(values=twice, items=_core.copied(twice))\n",
        // FN-15: a function that fails leaves what it had done, and
        // nothing is put back.
        concat!(
            "        _given = failing(items=kept, zero=0)\n",
            "        kept = _given[1]\n",
        ),
    ]);
    // No mark is kept at run time, and nothing is put back.
    for gone in ["_core.shared", "_core.each", "_core.restored", "_was_"] {
        assert!(!python.contains(gone), "the module has `{gone}`:\n{python}");
    }
}

/// A `stop` that fails where its guard holds.
const STOP_FAILS: (&str, &str) = fixture!(
    "a_failure_names_the_statement_that_failed",
    "demo.stops.fails.hgl"
);

/// TIME-5: "`stop` reads a temporal input under manual admission, as a
/// `when` under `valid([])` does: no header stands over it, so each
/// payload read is dominated by a guard".
///
/// hgraph gives a `stop` each input it names: the function takes the
/// node's configuration, its inputs and its state, and reads an input
/// as a `when` does. It gives a `start` none.
#[test]
fn a_stop_is_given_the_nodes_inputs() {
    let emitted = emit_clean(&[STOPS]);
    let python = emitted.module("demo.stops");
    python.assert_writes(&[
        concat!(
            "@last_seen.start\n",
            "def _start_last_seen(scale: _py.int, _state: _hg.STATE = None):\n",
        ),
        concat!(
            "@last_seen.stop\n",
            "def _stop_last_seen(scale: _py.int, value: _hg.TS[_py.int], ",
            "_state: _hg.STATE = None):\n",
            "    if value.valid:\n",
            "        _state.ended = _core.mul__i64_i64(lhs=value.value, rhs=scale)\n",
        ),
    ]);
}

/// Sets: a temporal `set<i64>` and its deltas.
const SETS: (&str, &str) = fixture!("demo.sets.hgl");

/// Set deltas that are not what the target published.
const SET_FAILS: (&str, &str) = fixture!(
    "a_failure_names_the_statement_that_failed",
    "demo.sets.fails.hgl"
);

/// TYPE-9, TYPE-16: a temporal `set<T>` is hgraph's `TSS[T]`, whose
/// tick is the set's delta: what it added and what it removed. END-5:
/// a node's writes to its set in one cycle are one delta.
///
/// TST-4: "Harness comparison is intrinsic: compare presence, exact
/// present data, and shape/length". The harness compares, where
/// Python's `==` would have hgraph's set delta take a removed element
/// for an added one.
#[test]
fn a_set_is_a_tss_and_its_delta_is_what_it_added_and_removed() {
    let emitted = emit_clean(&[SETS]);
    let python = emitted.module("demo.sets");
    python.assert_writes(&[
        // TYPE-16: a delta of a set is published on the node's own
        // output, where it ticks though it changes nothing.
        concat!(
            "def members(command: _hg.TS[_py.int], _output: _hg.TSS_OUT[_py.int] = None) ",
            "-> _hg.TSS[_py.int]:\n",
        ),
        concat!(
            "        return _core.entries_published(_output, _hg.set_delta(added=_py.frozenset((3,)), ",
            "removed=_py.frozenset((1,)), tp=_py.int), \"entries\")\n",
        ),
        // A property left out is empty.
        concat!(
            "        return _core.entries_published(_output, _hg.set_delta(removed=_py.frozenset((2,)), ",
            "added=_py.frozenset(), tp=_py.int), \"entries\")\n",
        ),
        // END-5: the node's own output, as hgraph names a set's.
        concat!(
            "def twice(command: _hg.TS[_py.int], _output: _hg.TSS_OUT[_py.int] = None) ",
            "-> _hg.TSS[_py.int]:\n",
        ),
        concat!(
            "    _core.entries_published(_output, _hg.set_delta(added=_py.frozenset((2, 3)), ",
            "removed=_py.frozenset(), tp=_py.int), \"entries\")\n",
        ),
        "    return _core.entries_published(_output, input.delta_value, \"entries\")\n",
        // EXPR-13: the properties in the order they are written.
        concat!(
            "    return _hg.set_delta(removed=_py.frozenset((left,)), ",
            "added=_py.frozenset((joined,)), tp=_py.int)\n",
        ),
        "    if not (_harness.same(_harness.evaluate(members, {\"command\": _hgl_4core._inst_replay__i64}, _hgl_4core._inst_record__set_i64_, command=[0], _hold=lambda _port: (_py.frozenset(_port.value) if _port.valid else _core.unset_read())), [])):\n",
        "    if not (not _harness.same(_harness.evaluate(members, {\"command\": _hgl_4core._inst_replay__i64}, _hgl_4core._inst_record__set_i64_, command=[1, 3], _hold=lambda _port: (_py.frozenset(_port.value) if _port.valid else _core.unset_read())), [",
    ]);
}

/// A trace given to a `const fn`.
const GIVEN: (&str, &str) = fixture!("demo.given.hgl");

/// A position with no publication gives a `const fn` no tuple.
const GIVEN_FAILS: (&str, &str) = fixture!(
    "a_failure_names_the_statement_that_failed",
    "demo.given.fails.hgl"
);

/// TST-16: "A trace given to a `const fn` is an ordinary value there:
/// for a result type T a `list<tuple<datetime, delta<T>>>`, one tuple
/// for each publication, in time order ... Nothing published is no
/// entry: a dense trace's `_` gives none ... The instant of a dense
/// trace's position i is the run's start plus i smallest steps".
///
/// The harness makes the list of the dense trace an eval gives.
#[test]
fn a_trace_given_to_a_const_fn_is_its_publications() {
    let emitted = emit_clean(&[GIVEN]);
    let python = emitted.module("demo.given");
    python.assert_writes(&[
        // `pass` is a word of Python's, and is written `pass_`.
        "ticks(seen=_harness.publications(_harness.evaluate(pass_, {\"value\": _hgl_4core._inst_replay__i64}, _hgl_4core._inst_record__i64, value=[7, None, 7, 9])))",
        "    seen = _harness.evaluate(pass_, {\"value\": _hgl_4core._inst_replay__i64}, _hgl_4core._inst_record__i64, value=[None, 1, None, 2])\n",
        "total(seen=_harness.publications(seen))",
    ]);
}

/// TST-7 and CEVAL-2: accept offsets and instants, compare each actual
/// publication, and reject unordered or out-of-range keys before replay.
#[test]
#[ignore = "needs a Python with hgraph, named by HGL_HGRAPH_PYTHON"]
fn timed_traces_replay_and_compare_at_the_runs_start() {
    let hgraph = on_hgraph("hgraph_timed_traces");
    let emitted = emit(&[fixture!(
        "timed_traces_replay_and_compare_at_the_runs_start",
        "demo.timed_keys.hgl"
    )]);
    let cases = [
        ("written_publications_preserve_absolute_instants", None),
        (
            "written_publications_require_ordered_keys",
            Some("must strictly increase"),
        ),
        ("an_earlier_expectation_differs_from_the_recording", None),
        (
            "a_replay_cannot_publish_before_its_configured_start",
            Some("before the run's start"),
        ),
        ("the_first_evaluation_instant_is_a_valid_start", None),
        (
            "the_never_sentinel_cannot_start_an_empty_run",
            Some("before the earliest evaluation time"),
        ),
        ("computed_nullable_and_mixed_keys", None),
        ("equal_computed_keys", Some("must strictly increase")),
        ("backward_computed_keys", Some("must strictly increase")),
        ("before_start", Some("before the run's start")),
        ("negative_offset", Some("before the run's start")),
        ("offset_outside_datetime", Some("numeric.out_of_range")),
        ("invalid_expectation", Some("must strictly increase")),
    ];
    let tests: Vec<_> = cases.iter().map(|(name, _)| emitted.test(name).0).collect();
    let ran = hgraph.run(&emitted, &tests);
    for ((name, error), outcome) in cases.iter().zip(&ran) {
        match error {
            None => assert_eq!(outcome, &Ran::Passed, "{name}"),
            Some(error) => assert!(
                matches!(outcome, Ran::Failed { message, .. } if message.contains(error)),
                "{name}: {outcome:?}"
            ),
        }
    }
    assert_eq!(ran.len(), cases.len());
}

/// A program with forms that are not written: a pack parameter of two
/// of its functions, and an instant outside the years 1 to 9999 in UTC
/// in one of its tests.
const REACH: [(&str, &str); 3] = [
    fixture!("reach", "reach.hgl"),
    fixture!("reach", "reach.far.hgl"),
    fixture!("reach", "reach.deep.hgl"),
];

/// A declaration or a test with a form that is not written is left out
/// of its file. A test can be run when it reaches nothing left out,
/// through the functions it names and those they name, in its module
/// or another; one that cannot says the first form it needs.
#[test]
fn a_test_runs_when_all_it_reaches_is_written() {
    let emitted = emit(&REACH);
    assert_eq!(
        emitted.reported,
        [
            "reach.hgl:5:29 target.unsupported: the hgraph target does not write a pack parameter yet",
            "reach.hgl:18:21 target.unsupported: the hgraph target does not write an instant outside the years 1 to 9999 yet",
            "reach.deep.hgl:2:37 target.unsupported: the hgraph target does not write a pack parameter yet",
        ]
    );
    assert_eq!(emitted.blocked("direct"), None);
    assert_eq!(emitted.blocked("wrong"), None);
    assert_eq!(emitted.blocked("overflows"), None);
    let pack = "the hgraph target does not write a pack parameter yet";
    assert_eq!(
        emitted.blocked("reaches"),
        Some(format!("reach.hgl:5:29 {pack}"))
    );
    assert_eq!(
        emitted.blocked("across"),
        Some(format!("reach.deep.hgl:2:37 {pack}"))
    );
    assert_eq!(
        emitted.blocked("own"),
        Some(
            "reach.hgl:18:21 the hgraph target does not write an instant outside the years 1 to 9999 yet"
                .to_string()
        )
    );
    let (_, direct) = emitted.test("direct");
    assert_eq!(direct.identity(), "reach::direct");

    let python = emitted.module("reach");
    python.assert_writes(&[
        "def fine(",
        "def outer(",
        "def test_direct(",
        "def test_wrong(",
    ]);
    // What has such a form itself is left out. A test that only
    // reaches one is written, and is not run.
    for left_out in ["def looped(", "def test_own("] {
        assert!(
            !python.contains(left_out),
            "{left_out} is not in:\n{python}"
        );
    }

    // A line of the file is traced back to the statement of the test
    // that starts at or before it.
    let (_, wrong) = emitted.test("wrong");
    let line = |text: &str| {
        1 + python
            .lines()
            .position(|line| line == text)
            .unwrap_or_else(|| panic!("`{text}` is a line of:\n{python}"))
    };
    let defined = line("def test_wrong():");
    assert_eq!(wrong.statement_at(defined), None);
    let place = |line| wrong.statement_at(line).map(|at| emitted.place(at));
    // An `assert` is two lines of Python: the `if`, and the `raise`
    // a failure is reported on.
    assert_eq!(place(defined + 1), Some("reach.hgl:21:5".to_string()));
    assert_eq!(place(defined + 2), Some("reach.hgl:21:5".to_string()));
    assert_eq!(place(defined + 3), Some("reach.hgl:22:5".to_string()));
    assert_eq!(place(defined + 4), Some("reach.hgl:22:5".to_string()));
}

/// An interpreter that cannot be started runs no test: each is not
/// run, with the reason, and none has failed (TST-14: "infrastructure
/// failures cannot prove conformance").
#[test]
fn an_interpreter_that_does_not_start_runs_nothing() {
    let emitted = emit(&REACH);
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("hgraph_not_started");
    fs::remove_dir_all(&directory).ok();
    let tests = [emitted.test("direct").0, emitted.test("wrong").0];
    let ran = run(
        Path::new("hgl-no-such-python"),
        &directory,
        &emitted.output,
        &tests,
    );
    assert_eq!(ran.len(), 2);
    for outcome in ran {
        let Ran::NotRun(reason) = outcome else {
            panic!("the test was not run: {outcome:?}");
        };
        assert!(
            reason.starts_with("`hgl-no-such-python` could not be started: "),
            "{reason}"
        );
    }
    // The program was written before the interpreter was asked for.
    assert!(directory.join("_hgl/_m_reach/__init__.py").is_file());
}

/// Where a test that needs hgraph runs what the target wrote.
struct Hgraph {
    /// The Python interpreter that has hgraph.
    python: PathBuf,
    /// An empty directory of the test's own, to write the program into.
    directory: PathBuf,
}

/// The interpreter the environment variable `HGL_HGRAPH_PYTHON` names,
/// and the directory `name` under Cargo's scratch directory, emptied.
fn on_hgraph(name: &str) -> Hgraph {
    let python = std::env::var("HGL_HGRAPH_PYTHON")
        .expect("HGL_HGRAPH_PYTHON names a Python interpreter that has hgraph");
    // Cargo gives integration tests a scratch directory under `target/`.
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    // Left over from an earlier run, perhaps; `ok()` ignores "not there".
    fs::remove_dir_all(&directory).ok();
    Hgraph {
        python: PathBuf::from(python),
        directory,
    }
}

impl Hgraph {
    /// Runs the tests of `emitted` at the positions `tests`, and gives
    /// what each did.
    fn run(&self, emitted: &Emitted, tests: &[usize]) -> Vec<Ran> {
        run(&self.python, &self.directory, &emitted.output, tests)
    }

    /// Runs every test of `emitted`, in order.
    fn run_all(&self, emitted: &Emitted) -> Vec<Ran> {
        let all: Vec<usize> = (0..emitted.output.tests.len()).collect();
        self.run(emitted, &all)
    }
}

/// What each test at the positions `tests` did, as `ran` has it, beside
/// its name: `name passed`, `name failed at message`, `at` the place of
/// the statement it failed at or `?`, or `name not run: reason`.
fn outcomes(emitted: &Emitted, tests: &[usize], ran: &[Ran]) -> Vec<String> {
    assert_eq!(ran.len(), tests.len(), "one outcome for each test run");
    tests
        .iter()
        .zip(ran)
        .map(|(&test, ran)| {
            let name = &emitted.output.tests[test].name;
            match ran {
                Ran::Passed => format!("{name} passed"),
                Ran::Failed { at, message } => format!(
                    "{name} failed {} {message}",
                    at.map_or("?".to_string(), |at| emitted.place(at))
                ),
                Ran::NotRun(reason) => format!("{name} not run: {reason}"),
            }
        })
        .collect()
}

/// The place and the message of the one test `ran` holds, which failed.
fn failed(emitted: &Emitted, ran: &[Ran]) -> (Option<String>, String) {
    let [Ran::Failed { at, message }] = ran else {
        panic!("the test failed: {ran:?}");
    };
    (at.map(|at| emitted.place(at)), message.clone())
}

/// Runs the tests the target writes for its fixtures on hgraph: every
/// program directly in `tests/fixtures`, emitted together. It needs
/// a Python that has hgraph, named by the environment variable `HGL_HGRAPH_PYTHON`,
/// so it is not among the tests run by default:
/// `cargo test -p hgl_hgraph -- --ignored` runs it.
#[test]
#[ignore = "needs a Python with hgraph, named by HGL_HGRAPH_PYTHON"]
fn the_generated_tests_pass_on_hgraph() {
    let hgraph = on_hgraph("hgraph_generated");
    // Every program directly in `tests/fixtures`, in the order of the
    // files' names.
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut programs: Vec<(String, String)> = fs::read_dir(&directory)
        .expect("the fixtures are there")
        .map(|entry| entry.expect("the directory is read").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "hgl"))
        .map(|path| {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            (
                name,
                fs::read_to_string(&path).expect("the fixture is read"),
            )
        })
        .collect();
    programs.sort();
    // `emit` takes each as two `&str`, borrowed from what `programs` owns.
    let files: Vec<(&str, &str)> = programs
        .iter()
        .map(|(name, text)| (name.as_str(), text.as_str()))
        .collect();
    let emitted = emit_clean(&files);
    let all: Vec<usize> = (0..emitted.output.tests.len()).collect();
    assert!(!all.is_empty(), "the fixtures contain named HGL tests");
    let ran = hgraph.run(&emitted, &all);
    let not_passed: Vec<String> = emitted
        .output
        .tests
        .iter()
        .zip(&ran)
        .filter(|(_, ran)| **ran != Ran::Passed)
        .map(|(test, ran)| format!("{}: {ran:?}", test.identity()))
        .collect();
    assert_eq!(not_passed, Vec::<String>::new());
    assert_eq!(
        ran.len(),
        all.len(),
        "one outcome per selected test in the hgraph emitter fixtures"
    );
}

/// A program that calls no function of the provider's still has the
/// provider beside its harness, which imports it: its test loads and
/// runs. Needs hgraph, as above.
#[test]
#[ignore = "needs a Python with hgraph, named by HGL_HGRAPH_PYTHON"]
fn a_test_that_calls_no_native_loads_its_harness() {
    let hgraph = on_hgraph("hgraph_harness_alone");
    let emitted = emit_clean(&[SHADOW]);
    let ran = hgraph.run_all(&emitted);
    assert_eq!(ran, [Ran::Passed]);
}

/// An application runs on hgraph (UNIT-14, RUN-1), and its output is
/// each publication of its entry's result, in time order (core's
/// CRUN-8). Core's CRUN-1: each run is built, started, evaluated and
/// stopped once, and RUN-4: "Each run has independent runtime-owned
/// state, captures, capabilities, store, and lifecycle": the four here
/// are four runs, one after another. RUN-3: `eval` is a simulation,
/// from the earliest start (CRUN-4: "a cycle runs at the next scheduled
/// time without waiting"), and `run` is in real time, from the present
/// (CRUN-5: "a cycle runs when the host clock reaches its scheduled
/// time"). CRUN-2: a `length` ends the run that long after its start.
/// CRUN-6: "A run that failed has no output." An entry with no result
/// has none either. Needs hgraph, as above.
#[test]
#[ignore = "needs a Python with hgraph, named by HGL_HGRAPH_PYTHON"]
fn an_application_runs_on_hgraph() {
    let hgraph = on_hgraph("hgraph_applications");
    let emitted = emit_clean(&APPLICATIONS);
    let ran = |path: &str| {
        let application = emitted
            .output
            .applications
            .iter()
            .position(|application| application.path == path)
            .unwrap_or_else(|| panic!("no application `{path}` was written"));
        run_application(
            &hgraph.python,
            &hgraph.directory,
            &emitted.output,
            application,
            &Stop::new(),
        )
    };

    // A simulation: `doubled` publishes each second from the earliest
    // start, a microsecond after 1970.
    let published = |pairs: &[(&str, &str)]| RanApplication::Stopped {
        output: Some(
            pairs
                .iter()
                .map(|(at, value)| (at.to_string(), value.to_string()))
                .collect(),
        ),
        by_host: false,
    };
    assert_eq!(
        ran("demo.ticks.backtest"),
        published(&[
            ("1970-01-01T00:00:01.000001Z", "2"),
            ("1970-01-01T00:00:02.000001Z", "4"),
            ("1970-01-01T00:00:03.000001Z", "6"),
        ])
    );

    // UNIT-14, core's CRUN-7: the store holds what the application put
    // there when the graph is built, so the entry reads its limit, 3,
    // from its first evaluation.
    assert_eq!(
        ran("demo.ticks.seeded"),
        published(&[
            ("1970-01-01T00:00:01.000001Z", "1"),
            ("1970-01-01T00:00:02.000001Z", "2"),
            ("1970-01-01T00:00:03.000001Z", "3"),
        ])
    );

    // No result, so no output.
    assert_eq!(
        ran("demo.ticks.silent"),
        RanApplication::Stopped {
            output: None,
            by_host: false
        }
    );

    // A failure names the HGL error's code.
    match ran("demo.ticks.broken") {
        RanApplication::Failed(message) => assert!(
            message.starts_with("numeric.division_by_zero: "),
            "{message}"
        ),
        other => panic!("the run fails: {other:?}"),
    }

    // Real time: 220ms of a source that publishes every 50ms is four
    // publications, counted from 1, at instants of the host's clock.
    let RanApplication::Stopped {
        output: Some(live),
        by_host: false,
    } = ran("demo.ticks.live")
    else {
        panic!("the run in real time stops at its end");
    };
    let values: Vec<&str> = live.iter().map(|(_, value)| value.as_str()).collect();
    assert_eq!(values, ["1", "2", "3", "4"]);
    assert!(live.iter().all(|(at, _)| at.as_str() > "2000-"), "{live:?}");
}

/// CRUN-8: a run's output is each publication, "its instant and then
/// what was published", and what was published is written as the
/// language writes it. OPS-6: an `f64` "has an exponent where the number
/// is 1e16 or more, or below 1e-4 and not zero", `1e21` and `1e-5`,
/// which Python's own text writes `1e+21` and `1e-05`. Needs hgraph, as
/// above.
#[test]
#[ignore = "needs a Python with hgraph, named by HGL_HGRAPH_PYTHON"]
fn a_published_number_is_written_as_the_language_writes_it() {
    let hgraph = on_hgraph("hgraph_shown");
    let emitted = emit_clean(&[
        fixture!(
            "a_published_number_is_written_as_the_language_writes_it",
            "demo.shown.hgl"
        ),
        fixture!(
            "a_published_number_is_written_as_the_language_writes_it",
            "demo.shown.printed.hgl"
        ),
    ]);
    let application = emitted
        .output
        .applications
        .iter()
        .position(|application| application.path == "demo.shown.printed")
        .expect("the application is written");
    let ran = run_application(
        &hgraph.python,
        &hgraph.directory,
        &emitted.output,
        application,
        &Stop::new(),
    );
    let RanApplication::Stopped {
        output: Some(published),
        by_host: false,
    } = ran
    else {
        panic!("the run ends: {ran:?}");
    };
    let values: Vec<&str> = published.iter().map(|(_, value)| value.as_str()).collect();
    assert_eq!(values, ["1e21", "1e-5", "2.5"]);
}

/// TST-4 and TST-5: a trace's cell written as a `delta<T>` is compared
/// with the delta its cycle published, by the language's equality: a
/// set's delta in a struct's cell or a tuple's is equal to another where
/// what each added and what each removed are (TYPE-16), and so is one a
/// program compares with `==` (OPS-2). hgraph's own equality of two takes
/// `Removed(1)` for `1`. Needs hgraph, as above.
#[test]
#[ignore = "needs a Python with hgraph, named by HGL_HGRAPH_PYTHON"]
fn a_set_delta_within_a_cell_is_compared_by_its_changes() {
    let hgraph = on_hgraph("hgraph_cell_deltas");
    let emitted = emit_clean(&[fixture!(
        "a_set_delta_within_a_cell_is_compared_by_its_changes",
        "demo.cell_deltas.hgl"
    )]);
    let all: Vec<usize> = (0..emitted.output.tests.len()).collect();
    let ran = hgraph.run(&emitted, &all);
    assert_eq!(
        outcomes(&emitted, &all, &ran),
        [
            "a_set_delta_in_a_struct_cell_is_compared_by_its_changes passed",
            "a_set_delta_in_a_tuple_cell_is_compared_by_its_changes passed",
            "two_set_deltas_are_equal_by_their_changes passed",
        ]
    );
}

/// END-4 and TIME-3: a transition to invalid wakes an active node, but no
/// publication occurred. Explicit modification guards are checked at evaluation.
const NOTIFIED: (&str, &str) = fixture!("demo.notified.hgl");

/// The owner's rule for a `var`, where it copies and where it does
/// not.
const COPIES: (&str, &str) = fixture!("demo.copies.hgl");

/// "all var mutations are mutations, not copies. The copies only occur
/// when: 1. the value is passed to a function and the function also
/// recieves a var reference to the item or items it contains ... 2. let
/// assignment". And: "It is only when the list could be modified in
/// the scope of the let / parameter that we need to make a copy", "If
/// a parameter or let is retained, then it needs to make the copy."
#[test]
fn a_copy_is_made_where_a_reference_is_kept_or_outlives_a_change() {
    let emitted = emit_clean(&[COPIES]);
    let python = emitted.module("demo.copies");
    python.assert_writes(&[
        // A result: the function's own `var` and its own `let` as they
        // are, and a copy of what it was given, read through a name or
        // not.
        "    return out\n",
        "    return fresh\n",
        "    return _core.copied(_core.item(rows, 0))\n",
        "    return _core.copied(named)\n",
        // What a native function keeps in the place it is given.
        "        _given = _core.fn_append(values=items, item=_core.copied(row))\n",
        // A `let` in a loop whose body changes the place it reads.
        "        last = _core.copied(_core.item(rows, i))\n",
        // A `let` of one of two places, each of which then changes.
        "    picked = _core.copied((left if flag else right))\n",
        // A `let` whose place changes no more in its scope, and a loop
        // whose body changes nothing of what it runs over: references.
        "    last = _core.item(rows, 2)\n",
        "    for row in rows:\n",
        // What a place keeps: a value written out that holds a name's
        // list, and an entry.
        "    book = _core.copied(Book(orders=source, name=\"a\"))\n",
        "    books = _core.set_entry(books, \"a\", _core.copied(source))\n",
        // A function's own result is kept as it is.
        "    mine = built(count=2)\n",
        // The owner's first case: a place given to a `var` parameter,
        // and given again.
        "    _given = push_all(items=rows, more=_core.copied(rows))\n",
        "    _given = both(left=twice, right=_core.copied(twice))\n",
    ]);
}

/// The owner's rule for a struct, FN-15's failure, and FN-13's stable
/// arguments and retained results: what no copy is made for, and what
/// one is. The fixture's tests, run on hgraph, show that nothing that
/// took a value sees a later change of it.
const COPY_POINTS: (&str, &str) = fixture!("demo.copy_points.hgl");

/// "The copy by value exception is for trivial structures (think
/// sizeof(i64))": a struct of at most two scalar fields is a value,
/// copied and never changed. Any other is changed where it is (EXPR-3:
/// "An assignment may name a place inside a value, at any depth"), and
/// copied at the points a list is: "the value is passed to a function
/// and the function also recieves a var reference to the item or items
/// it contains", "let assignment", and where it is kept.
///
/// FN-15: "A function that fails leaves the place holding what its
/// parameter held when it failed: no change it had made is undone."
/// FN-13: "Each invocation observes a stable ordinary argument value
/// ... Retained results must remain independent of later temporal
/// changes."
#[test]
fn a_struct_is_changed_where_it_is_and_copied_where_it_is_kept() {
    let emitted = emit_clean(&[COPY_POINTS]);
    let python = emitted.module("demo.copy_points");
    let saved = format!(
        "            _state.saved = Ticket(id={}, size={}, price={})\n",
        part("current[\"id\"]"),
        part("current[\"size\"]"),
        part("current[\"price\"]")
    );
    python.assert_writes(&[
        // A struct of two scalars is a value; one of three, one with a
        // `str`, one another inherits from, and one of structs are not.
        "Point = _core.struct(\"Point\", (\"x\", \"y\"), by_value=True)\n",
        "Small = _core.struct(\"Small\", (\"size\",), by_value=True)\n",
        "Ticket = _core.struct(\"Ticket\", (\"id\", \"size\", \"price\"))\n",
        "Key = _core.struct(\"Key\", (\"name\", \"side\"))\n",
        "Sized = _core.struct(\"Sized\", (\"size\",))\n",
        "Book = _core.struct(\"Book\", (\"best\", \"name\", \"at\"))\n",
        // No copy: a change of a field, at any depth, is made where the
        // struct is, and a value on the way is given a new one.
        "    mine = _core.set_field(mine, \"size\", 2)\n",
        "    book = _core.set_at(book, ((\"f\", \"best\"), (\"f\", \"size\"),), lambda: 5)\n",
        "    book = _core.set_at(book, ((\"f\", \"at\"), (\"f\", \"x\"),), lambda: 1)\n",
        "    tickets = _core.set_at(tickets, ((\"i\", 1), (\"f\", \"size\"),), lambda: 8)\n",
        "    pair = _core.set_at(pair, ((\"i\", 0), (\"f\", \"size\"),), lambda: 12)\n",
        "        into = _core.set_field(into, \"size\", ",
        "        _state.mine = _core.set_field(_state.mine, \"size\", ",
        // No copy: a `let` whose place changes no more in its scope, a
        // loop over what its body does not change, a value written out,
        // a function's own result and its own `var`, and an argument of
        // a call with no `var` reference to it.
        "    read = mine\n",
        "    for each in tickets:\n",
        "    fourth = Ticket(id=4, size=4, price=4.0)\n",
        "    third = grown(given=first, by=2)\n",
        "    _given = _core.fn_append(values=tickets, item=ticket(size=3))\n",
        "    return mine\n",
        "    _given = absorb(into=mine, other=other)\n",
        // No copy, and no change where it is: a value.
        "    moved = at\n",
        "    before = at\n",
        "    moved = _core.with_field(moved, x=5)\n",
        "    held = small\n",
        "    _given = _core.fn_append(values=items, item=small)\n",
        // A copy: a `let` and a loop whose place changes in its scope.
        "    before = _core.copied(mine)\n",
        "    best = _core.copied(book.best)\n",
        "    for each in _core.copied(mine):\n",
        // A copy: what a place keeps, assigned to it or into it, or put
        // in it by a native function, a dictionary's key among them.
        "    second = _core.copied(first)\n",
        "    mine = _core.copied(given)\n",
        "    tickets = _core.set_item(tickets, 0, _core.copied(first))\n",
        "    _given = _core.fn_append(values=tickets, item=_core.copied(first))\n",
        "    named = _core.set_entry(named, \"a\", _core.copied(first))\n",
        "    book = _core.set_field(book, \"best\", _core.copied(first))\n",
        "    pair = _core.copied((first, 1,))\n",
        "    counts = _core.set_entry(counts, _core.copied(key), 1)\n",
        // A copy: a result of what the function was given, and what is
        // published of a `state`; a node's `var` of its configuration.
        "def same(given):\n    return _core.copied(given)\n",
        "            return _core.copied(_state.mine)\n",
        "    mine = _core.copied(initial)\n",
        // A copy: the owner's first case (FN-13's stable argument).
        "    _given = absorb(into=mine, other=_core.copied(mine))\n",
        // A key held before a call that may change its source: the
        // entry assigned is the one it named (FN-15).
        concat!(
            "    _at_1 = _core.copied(key)\n",
            "    _arg_2 = _core.entry(counts, _at_1)\n",
            "    _given = rekeyed(key=_arg_1, count=_arg_2)\n",
            "    key = _given[1]\n",
            "    counts = _core.set_entry(counts, _at_1, _given[2])\n",
        ),
        // FN-15: what fails is given back beside what the parameter
        // holds, the place is assigned, and it is raised again.
        concat!(
            "    except _py.Exception as _failed:\n",
            "        return (_core.Failed(_failed), count)\n",
        ),
        concat!(
            "        _given = bump_then_fail(count=count, zero=0)\n",
            "        count = _given[1]\n",
            "        _core.raise_if_failed(_given[0])\n",
        ),
        concat!(
            "        book = _core.set_at(book, ((\"f\", \"best\"), (\"f\", \"size\"),), lambda: _given[1])\n",
            "        _core.raise_if_failed(_given[0])\n",
        ),
        // No copy: a struct read whole from its bundle is made anew.
        saved.as_str(),
        // A copy: a result that may be the function's own `var` or its
        // parameter, a constant or what a loop over one gives.
        "def pick(p, flag):\n    mine = [0]\n    x = (mine if flag else p)\n    return _core.copied(x)\n",
        "        return _core.copied(x)\n",
        // In a graph, an ordinary value: a `var` and a `let` of one, a
        // call given it twice, a loop over what its body grows, and
        // what a node or a source keeps. A graph that gives on what it
        // was given gives it as it is: the node copies it.
        "    b = _core.copied(a)\n",
        "    snap = _core.copied(a)\n",
        "    _given = add_twice(into=a, more=_core.copied(a))\n",
        "    for value in _core.copied(a):\n",
        "    n = summed(values=_core.Held(_core.copied(cfg)), tick=tick)\n",
        "    return summed(values=_core.Held(_core.copied(values)), tick=tick)\n",
        "    n = relayed(values=cfg, tick=tick)\n",
        "_core.Held(_core.copied(values)))\n",
    ]);
}

/// Every copy the target writes for `copy_points.hgl` is needed: with
/// any one of them taken out, a test of the fixture fails on hgraph. So
/// the fixture shows each copy point it has, and the claim that each
/// copy is needed is checked, not asserted. Needs hgraph, as above.
#[test]
#[ignore = "needs a Python with hgraph, named by HGL_HGRAPH_PYTHON"]
fn every_copy_the_copy_points_fixture_makes_is_needed() {
    let hgraph = on_hgraph("hgraph_copy_points");
    let emitted = emit_clean(&[COPY_POINTS]);
    let all: Vec<usize> = (0..emitted.output.tests.len()).collect();
    // A test that a copy taken out sends round a loop without end is
    // stopped: it has not passed.
    let limit = |_: &Test| Some(std::time::Duration::from_secs(30));
    let ran = run_within(
        &hgraph.python,
        &hgraph.directory.join("whole"),
        &emitted.output,
        &all,
        &limit,
    );
    assert!(ran.iter().all(|ran| *ran == Ran::Passed), "{ran:?}");
    let module = "_hgl/_m_demo/_m_copy_points/__init__.py";
    let text = emitted.text(module);
    let copy = "_core.copied";
    let copies: Vec<usize> = text.match_indices(copy).map(|(at, _)| at).collect();
    assert!(
        copies.len() > 20,
        "the fixture makes {} copies",
        copies.len()
    );
    let mut needless = Vec::new();
    for (number, &at) in copies.iter().enumerate() {
        // `_core.copied(x)` taken out is `(x)`, the value itself.
        let mut output = emitted.output.clone();
        let file = output
            .files
            .iter_mut()
            .find(|file| file.path == module)
            .expect("the module is written");
        file.text.replace_range(at..at + copy.len(), "");
        let ran = run_within(
            &hgraph.python,
            &hgraph.directory.join(format!("without_{number}")),
            &output,
            &all,
            &limit,
        );
        // A copy is shown needed by a test that ran and failed: one
        // that did not run shows nothing.
        if !ran.iter().any(|ran| matches!(ran, Ran::Failed { .. })) {
            let line = text[..at].matches('\n').count() + 1;
            let written = text.lines().nth(line - 1).unwrap_or_default().trim();
            needless.push(format!("line {line}: {written}: {ran:?}"));
        }
    }
    assert_eq!(needless, Vec::<String>::new());
}

/// TIME-9, core's CAP-4 and CAP-5: nodes that a scheduler or an alarm
/// wakes, with inputs and with none.
const TIMERS: (&str, &str) = fixture!("demo.timers.hgl");

/// TIME-9: `scheduled()` is the scheduler's `is_scheduled_now`, one
/// `if` over its handler, since "a wake asked of the scheduler is due
/// in this evaluation, whether or not a temporal input has ticked".
/// With an alarm it "holds where the node is evaluated and no temporal
/// input has ticked". TIME-3: a header that is `scheduled()` alone
/// requires no input to be valid, so the node requires none, and a
/// handler with no `modified` selector is run by an input's wake.
#[test]
fn a_timer_wakes_its_own_handlers() {
    let emitted = emit_clean(&[TIMERS]);
    let python = emitted.module("demo.timers");
    python.assert_writes(&[
        // A scheduler: the node requires no input valid, and each
        // handler is under its own guard.
        concat!(
            "@_hg.compute_node(valid=())\n",
            "def sampled(price: _hg.TS[_py.float], every: _dt.timedelta, ",
            "_state: _hg.STATE = None, _scheduler: _hg.SCHEDULER = None, _clock: _hg.EvaluationClock = None) -> _hg.TS[_py.float]:\n",
            // CAP-4: the wake being delivered is taken as the node is
            // entered, and is no longer pending.
            "    _scheduler = _core.Wake(_scheduler, _clock.evaluation_time)\n",
            // What woke the node is read where it is entered: a
            // handler before the one that asks may change it.
            "    _timer_woke = _scheduler.is_scheduled_now\n",
            "    if price.valid and price.modified:\n",
            "        _state.last = price.value\n",
            "    if _timer_woke:\n",
        ),
        // An alarm: no input has ticked, where it published or was
        // invalidated.
        "    _timer_woke = _scheduler.is_scheduled_now and not (price.modified or price.invalidated)\n",
        // A handler with no `modified` selector, in a node a scheduler
        // wakes too: an input woke the node.
        concat!(
            "    _timer_woke = _scheduler.is_scheduled_now\n",
            "    _input_woke = (value.modified or value.invalidated)\n",
            "    if _input_woke and value.valid:\n",
            "        _state.seen = _core.add__i64_i64(lhs=_state.seen, rhs=1)\n",
        ),
        // An input that woke the node is one that was active when the
        // node was entered.
        "    _input_woke = ((value.modified or value.invalidated) and value.active)\n",
        // The scheduler's operations and properties are the
        // provider's. TIME-8: a duration that is a `const` parameter is
        // checked as the program runs, in `start` and in a `when`.
        "_core.schedule_Scheduler_duration_str__bool(capability=_scheduler, at=every, tag=None, on_wall_clock=False, _starting=True)",
        "_core.schedule_Scheduler_duration_str__bool(capability=_scheduler, at=every, tag=None, on_wall_clock=False, _starting=False)",
        "_core.schedule_Alarm_duration(capability=_scheduler, at=every, _starting=True)",
        "_core.fn_un_schedule(scheduler=_scheduler, tag=None)",
        "_core.Scheduler_is_scheduled(_scheduler)",
        // hgraph takes no node without an input: one with none is
        // given one that never wakes it, and a graph of the function's
        // name gives it a constant.
        concat!(
            "@_hg.compute_node(active=(), valid=())\n",
            "def _node_beats(_begin: _hg.TS[_py.bool], every: _dt.timedelta, times: _py.int, ",
            "_state: _hg.STATE = None, _scheduler: _hg.SCHEDULER = None, _clock: _hg.EvaluationClock = None) -> _hg.TS[_py.int]:\n",
            "    _scheduler = _core.Wake(_scheduler, _clock.evaluation_time)\n",
            "    if _scheduler.is_scheduled_now:\n",
        ),
        concat!(
            "@_hg.graph\n",
            "def beats(every: _dt.timedelta, times: _py.int) -> _hg.TS[_py.int]:\n",
            "    return _node_beats(_begin=_hg.const(True), every=every, times=times)\n",
        ),
    ]);
}

/// TIME-3 and TIME-9: `scheduled()` under a negation and in a
/// disjunction, a dormant handler, and a dormant node.
const TIMER_SELECTORS: (&str, &str) = fixture!("demo.timer_selectors.hgl");

/// TIME-3: "Selectors under negation or disjunction leave the default in
/// place", so such a handler subscribes to every input and is run by an
/// input's wake. TIME-9: `scheduled()` there is what woke the node, held
/// where it is entered. With neither a scheduler nor an alarm "nothing
/// wakes it: the node is dormant".
#[test]
fn scheduled_is_a_bool_where_it_selects_nothing() {
    let emitted = emit_clean(&[TIMER_SELECTORS]);
    let python = emitted.module("demo.timer_selectors");
    python.assert_writes(&[
        // An input's wake, and no wake of the scheduler due.
        concat!(
            "    _timer_woke = _scheduler.is_scheduled_now\n",
            "    if (value.modified or value.invalidated) and (not _timer_woke):\n",
        ),
        // With an alarm, no input ticked.
        concat!(
            "    _timer_woke = _scheduler.is_scheduled_now and not (value.modified or value.invalidated)\n",
            "    if (value.modified or value.invalidated) and (not _timer_woke):\n",
        ),
        // A disjunction: every input subscribed, and each required valid.
        concat!(
            "@_hg.compute_node\n",
            "def either(a: _hg.TS[_py.int], b: _hg.TS[_py.int], _scheduler: _hg.SCHEDULER = None, _clock: _hg.EvaluationClock = None) -> _hg.TS[_py.int]:\n",
            "    _scheduler = _core.Wake(_scheduler, _clock.evaluation_time)\n",
            "    _timer_woke = _scheduler.is_scheduled_now\n",
            "    if (a.modified or a.invalidated or b.modified or b.invalidated) and ((a.modified and a.valid) or _timer_woke):\n",
        ),
        // In a body, after asking again under the tag that is due.
        "        return _timer_woke\n",
        // No input wakes a handler that subscribes to none, and it is not
        // written; a node that nothing wakes is never active.
        "@_hg.sink_node(active=())\ndef null_sink(a: _hg.TS[_py.float]):\n    pass\n",
        "def half_dormant(a: _hg.TS[_py.int]) -> _hg.TS[_py.int]:\n    if a.modified:\n",
        "@_hg.compute_node(active=(), valid=())\ndef _node_idle(_begin: _hg.TS[_py.bool]) -> _hg.TS[_py.int]:\n    pass\n",
    ]);
}

/// TIME-3, TIME-8 and core's CAP-4: `modified([])` read as a bool, a
/// wake's duration checked as the program runs, and a wake that stays
/// due for the rest of its evaluation.
const TIMERS_WAVE3: (&str, &str) = fixture!("demo.timers_wave3.hgl");

/// TIME-3: "In that handler's body `modified([])`, as a bool, is true
/// ... Elsewhere, in another body or under a negation or a disjunction,
/// it is true in a cycle where no temporal input was modified": `True`
/// in the body of the handler whose header selects it, and elsewhere
/// that no input is modified as `modified(input)` asks it, so that an
/// invalidation is none, and a `ref` parameter is modified where it is
/// given a designation (END-10); `True` in a node with no input. TIME-8: a
/// duration whose value the target does not work out is checked by the
/// provider's function, `_starting=`, once every argument is evaluated
/// (EXPR-2): the call fails where it is not more than zero, but for a
/// zero in `start`. One worked out and admitted, a literal or a
/// constant expression, is given as it is.
#[test]
fn the_empty_selection_as_a_bool_and_a_wakes_duration_are_written() {
    let emitted = emit_clean(&[TIMERS_WAVE3]);
    let python = emitted.module("demo.timers_wave3");
    python.assert_writes(&[
        concat!(
            "    if _scheduler.is_scheduled_now:\n",
            "        return _py.tuple([True, (value.modified and value.valid)])\n",
        ),
        "    if _scheduler.is_scheduled_now and not (value.modified or value.invalidated):\n        if True:\n",
        "    if _scheduler.is_scheduled_now:\n        return (not ((value.modified and value.valid)))\n",
        "    if _timer_woke and (not (not ((value.modified and value.valid)))):\n",
        "    return _py.tuple([(not ((input.modified and input.valid))), (not (not ((input.modified and input.valid))))])\n",
        "    if ((not ((input.modified and input.valid))) or (input.modified and input.valid)):\n",
        "        return (not ((a.modified and a.valid) or (b.modified and b.valid)))\n",
        "_begin: _hg.TS[_py.bool], _scheduler: _hg.SCHEDULER = None, _clock: _hg.EvaluationClock = None) -> _hg.TS[_py.bool]:\n    _scheduler = _core.Wake(_scheduler, _clock.evaluation_time)\n    if _scheduler.is_scheduled_now:\n        return True\n",
        "        return (not (target.modified))\n",
        // A literal, which the analyser has judged.
        "at=_dt.timedelta(microseconds=1000000), tag=\"with input\"",
        "at=delay.value, tag=\"next\", on_wall_clock=False, _starting=False)",
        "_core.schedule_Alarm_duration(capability=_scheduler, at=delay.value, _starting=False)",
        "_core.schedule_Scheduler_duration_str__bool(capability=_scheduler, at=every, tag=None, on_wall_clock=False, _starting=True)",
        "_core.schedule_Alarm_duration(capability=_scheduler, at=every, _starting=True)",
        concat!(
            "def _stop_asked_in_stop(every: _dt.timedelta, value: _hg.TS[_py.int], _scheduler: _hg.SCHEDULER = None):\n",
            "    _core.schedule_Scheduler_duration_str__bool(capability=_scheduler, at=every, tag=None, on_wall_clock=False, _starting=False)",
        ),
        // A constant expression worked out to be more than zero.
        "_core.schedule_Alarm_duration(capability=_scheduler, at=_core.add__duration_duration(lhs=_core.sub__duration_duration(lhs=_core.mul__duration_i64(lhs=STEP(), rhs=2), ",
        // An instance's literal, which the analyser did not see: one
        // more than zero is given as it is, and zero is checked.
        "at=_dt.timedelta(microseconds=1000000), tag=None",
        "def _inst_generic_after__0us(value: _hg.TS[_py.int], _scheduler: _hg.SCHEDULER = None, _clock: _hg.EvaluationClock = None) -> _hg.TS[_py.bool]:\n    _scheduler = _core.Wake(_scheduler, _clock.evaluation_time)\n    if value.modified:\n        _core.schedule_Scheduler_duration_str__bool(capability=_scheduler, at=_dt.timedelta(microseconds=0), tag=None, on_wall_clock=False, _starting=False)",
    ]);
    let support = emitted.text("_hgl/_core_support.py");
    assert!(support.contains("code=\"schedule.non_positive_duration\""));
}

/// TIME-8: a wake's duration that is a constant made of constants is
/// worked out once for each constant, however many times it is named:
/// `D40`, each `D{n}` the sum of two of `D{n-1}`, is more than zero, and
/// is given as it is, with no check as the program runs.
#[test]
fn a_chain_of_constant_durations_is_worked_out_once_each() {
    let mut text = String::from("module m\nconst D0: duration = 1us\n");
    for n in 1..=40 {
        text.push_str(&format!("const D{n}: duration = D{} + D{}\n", n - 1, n - 1));
    }
    text.push_str(concat!(
        "fn deep() -> bool {\n",
        "    inject scheduler\n",
        "    start { schedule(scheduler, D40) }\n",
        "    when scheduled() { return true }\n",
        "}\n",
    ));
    let emitted = emit_clean(&[("m.hgl", &text)]);
    emitted.module("m").assert_writes(&[
        "_core.schedule_Scheduler_duration_str__bool(capability=_scheduler, at=D40(), tag=None, on_wall_clock=False)\n",
    ]);
}

/// TIME-3: `modified([])` is "true in a cycle where no temporal input
/// was modified", and `start` and `stop` are no cycle of the node: what
/// it is there the rule does not say, and it is reported. TIME-8: "A
/// wake asked for with a `duration` takes one more than zero, but in
/// `start`": whether a value function given the scheduler or the alarm
/// asks in `start` the rule does not say, and a duration there that may
/// be zero, a literal, a constant, a constant expression or a parameter,
/// is reported; one worked out to be more than zero, a constant
/// expression of core's arithmetic among them, and an instant, are
/// written.
#[test]
fn what_a_timer_form_not_written_is_reported() {
    let emitted = emit(&[fixture!(
        "what_a_timer_form_not_written_is_reported",
        "m.hgl"
    )]);
    assert_eq!(
        emitted.reported,
        [
            "m.hgl:6:16 target.unsupported: the hgraph target does not write `modified([])` in `start` or `stop` yet",
            "m.hgl:16:20 target.unsupported: the hgraph target does not write `modified([])` in `start` or `stop` yet",
            "m.hgl:20:59 target.unsupported: the hgraph target does not write a wake asked for with a duration that may be zero in a value function (TIME-8) yet",
            "m.hgl:21:77 target.unsupported: the hgraph target does not write a wake asked for with a duration that may be zero in a value function (TIME-8) yet",
            "m.hgl:22:56 target.unsupported: the hgraph target does not write a wake asked for with a duration that may be zero in a value function (TIME-8) yet",
            "m.hgl:32:52 target.unsupported: the hgraph target does not write a wake asked for with a duration that may be zero in a value function (TIME-8) yet",
        ]
    );
}

/// Core's CAP-4 and CAP-5: a wake not ahead of the current evaluation,
/// the empty tag, the wake being delivered; and a node with no input,
/// no handler and no timer.
const TIMER_FIXES: (&str, &str) = fixture!("demo.timer_fixes.hgl");

/// hgraph takes no node without an input: every node with no temporal
/// input is given one that never wakes it, one with only a `start`
/// among them. CAP-4: the wake being delivered is taken as the node is
/// entered, and the empty tag is given a mark as every tag is.
#[test]
fn every_inputless_node_has_a_hidden_input() {
    let emitted = emit_clean(&[TIMER_FIXES]);
    let python = emitted.module("demo.timer_fixes");
    python.assert_writes(&[
        concat!(
            "@_hg.sink_node(active=(), valid=())\n",
            "def _node_started_only(_begin: _hg.TS[_py.bool], _store: _hg.GlobalState = None):\n",
            "    pass\n",
        ),
        "    _node_started_only(_begin=_hg.const(True))\n",
        "    _scheduler = _core.Wake(_scheduler, _clock.evaluation_time)\n",
    ]);
    let support = emitted.text("_hgl/_core_support.py");
    assert!(support.contains("return None if tag is None else \"#\" + tag"));
}

/// TIME-3: what `modified([])` asks beside a `modified` selector that
/// names inputs is not settled, and is reported; beside `valid([])` it
/// is the spec's own illustration, and is written.
#[test]
fn an_empty_selection_beside_another_is_reported() {
    let emitted = emit(&[fixture!(
        "an_empty_selection_beside_another_is_reported",
        "m.hgl"
    )]);
    assert_eq!(
        emitted.reported,
        [
            "m.hgl:5:10 target.unsupported: the hgraph target does not write `modified([])` beside another `modified` selector yet"
        ]
    );
}

/// TYPE-11: windows of ticks and of a duration, as inputs and results;
/// TYPE-16: a window's delta is "V: the value that arrived".
const WINDOWS: (&str, &str) = fixture!("demo.windows.hgl");

/// TYPE-11: "`rolling_window<V, Max, Min>` is a rolling window of V ...
/// Omitted minimum equals maximum". It is hgraph's `TSW`, of a count of
/// ticks or of a `timedelta`, which takes one size as both. TYPE-16: a
/// window's delta is "V: the value that arrived", so a node publishes
/// one value to it, `delta_value` is the value that arrived, and an
/// eval's replay and record of one are core's for the window (core's
/// CEVAL-1), whose trace is each value that arrived (TST-5).
#[test]
fn a_window_is_hgraphs_tsw() {
    let emitted = emit_clean(&[WINDOWS]);
    let python = emitted.module("demo.windows");
    let ticks = "_hg.TSW[_py.float, _hg.WindowSize[3], _hg.WindowSize[1]]";
    python.assert_writes(&[
        format!("def collect(price: _hg.TS[_py.float]) -> {ticks}:\n    return price.value\n"),
        format!("_output: _hg.TSW_OUT[_py.float, _hg.WindowSize[3], _hg.WindowSize[1]] = None) -> {ticks}:"),
        "    if ((not _output.valid) and ".to_string(),
        "    _output.value = price.value\n".to_string(),
        // A minimum of `0s` is written as it is (hgraph 0.9.8).
        concat!(
        "-> _hg.TSW[_py.float, _hg.WindowSize[_dt.timedelta(microseconds=5000000)], ",
        "_hg.WindowSize[_dt.timedelta(microseconds=0)]]:"
        )
        .to_string(),
        "-> _hg.TSW[_py.float, _hg.WindowSize[1]]:".to_string(),
        format!("def arrived(prices: {ticks}) -> _hg.TS[_py.float]:\n    return prices.delta_value\n"),
        "    return prices.valid\n".to_string(),
        "    return (prices.modified and prices.valid)\n".to_string(),
        "(prices.last_modified_time if prices.valid else None)".to_string(),
        "-> _hg.TSW[Side, _hg.WindowSize[2], _hg.WindowSize[1]]:".to_string(),
        "def _inst_newest__i64_4(values: _hg.TSW[_py.int, _hg.WindowSize[4], _hg.WindowSize[1]])"
        .to_string(),
        "    return arrived(prices=collect(price=price))\n".to_string(),
        "_harness.evaluate(collect, {\"price\": _hgl_4core._inst_replay__f64}, \
         _hgl_4core._inst_record__rolling_window_f64__3__1_, "
        .to_string(),
        "_harness.evaluate(arrived, {\"prices\": _hgl_4core._inst_replay__rolling_window_f64__3__1_}, \
         _hgl_4core._inst_record__f64, "
        .to_string(),
    ]);
    // TYPE-9: a window within a struct, a fixed list, a dictionary and
    // a tuple is that one's child, read by its port, and wired from it
    // in a graph; a part that is no window is read in a hook.
    let small = "_hg.TSW[_py.float, _hg.WindowSize[2], _hg.WindowSize[1]]";
    python.assert_writes(&[
        format!("_core.schema(Book, {{\"prices\": {ticks}, \"last\": _hg.TS[_py.float]}})"),
        format!("_hg.TSL[{small}, _hg.Size[2]]"),
        format!("_hg.TSD[_py.str, {small}]"),
        format!("_hg.TSB[\"_0\": {small}, \"_1\": _hg.TS[_py.float]]"),
        format!("    return {}\n", part("book[\"last\"]")),
        format!("    return {}\n", part("pair[\"_1\"]")),
        "_core.changed(book.delta_value, \"prices\")".to_string(),
        "    return arrived(prices=booked(price=price)[\"prices\"])\n".to_string(),
        "    return arrived_two(prices=slots(slot=slot, price=price)[1])\n".to_string(),
    ]);
    // Core's `replay` and `record` written for the window. `record` reads
    // the delta where a `result` is taken (core's CEVAL-4), which a
    // window of ticks always has (ERR-14): `ok` of hgraph's own.
    let core = emitted.module("core");
    core.assert_writes(&[
        format!("def _inst_replay__rolling_window_f64__3__1_(items: _py.object) -> {ticks}:"),
        format!("def _inst_record__rolling_window_f64__3__1_(input: {ticks}, "),
        "        change = (_core.Ok(input.delta_value) if (input.modified and input.valid) else \
         _core.Err(_core.Error(\"the endpoint has not ticked in the cycle\", \
         code=\"value.unset_read\")))\n"
            .to_string(),
    ]);
}

/// What this target does not write of a window is reported: the
/// window of a value that is no scalar or enum, and a window of
/// a duration's `delta_value` read where a `result` is taken (ERR-14),
/// which an eval's record of one reads (core's CEVAL-4). Core's END-9
/// gives every window's readiness, of ticks and of a duration, which is
/// written: an input or `out` read at any minimum, and a window given to
/// a lifted helper as a `list<V>` (FN-12).
#[test]
fn a_window_form_not_written_yet_is_reported() {
    let emitted = emit(&[fixture!(
        "a_window_form_not_written_yet_is_reported",
        "m.hgl"
    )]);
    let reported: Vec<&String> = emitted
        .reported
        .iter()
        .filter(|place| place.starts_with("m.hgl:"))
        .collect();
    assert_eq!(
        reported,
        [
            "m.hgl:7:19 target.unsupported: the hgraph target does not write `delta_value` of a child or of a window of a duration, or `removed_value`, read where a `result` is taken (ERR-14) yet",
            "m.hgl:23:4 target.unsupported: the hgraph target does not write the type `rolling_window<Quote, 3, 1>` yet",
            "m.hgl:26:4 target.unsupported: the hgraph target does not write the type `rolling_window<f64?, 3, 1>` yet",
        ]
    );
    // The record of `recent`'s window is core's, which reads the window's
    // delta where a `result` is taken (core's CEVAL-4): a window of a
    // duration's is reported.
    assert_eq!(
        emitted.blocked("recorded").as_deref(),
        Some(
            "<core>/core/core.hgl:96:47 the hgraph target does not write `delta_value` of a \
             child or of a window of a duration, or `removed_value`, read where a `result` is \
             taken (ERR-14) yet"
        )
    );
}

/// TST-10: "`assert raises(\"code\")` ... A matching escaping error
/// passes". An assertion of a failure this target gives with no code, or
/// does not write, is reported, and its test is not run: EXPR-3 and
/// EXPR-7's index outside a list, `value.index_out_of_range`, which the
/// provider's index failure has no code for, and core's OPS-1 text
/// repeated a count below zero, `numeric.out_of_range`, which the
/// provider does not write, core's `mul_` of the text and the count.
/// Another `numeric.out_of_range` is asserted. Core's TEN-4: "Only the
/// result is held to the range", which the provider's month step is held
/// to as well: a move of a date in the range's first or last year, core's
/// `add_` or `sub_` of it and a tenor, is reported, and one within it is
/// written.
#[test]
fn a_failure_this_target_gives_no_code_is_not_asserted() {
    let emitted = emit(&[fixture!(
        "a_failure_this_target_gives_no_code_is_not_asserted",
        "m.hgl"
    )]);
    let repeated = "target.unsupported: the hgraph target does not write text repeated a count \
                    below zero, `numeric.out_of_range` (OPS-1) yet";
    assert_eq!(
        emitted.reported,
        [
            "m.hgl:5:5 target.unsupported: the hgraph target does not write a failure coded \
             `value.index_out_of_range` (EXPR-3, EXPR-7) yet"
                .to_string(),
            format!("m.hgl:9:5 {repeated}"),
            "m.hgl:15:12 target.unsupported: the hgraph target does not write a move of a date \
             in the first or last year of the range, which only its result is held to (TEN-4) yet"
                .to_string(),
        ]
    );
    assert_eq!(emitted.blocked("other_codes"), None);
    assert_eq!(emitted.blocked("moved_within"), None);
    // A function of a date and a tenor, or of a `str` and an `i64`, that
    // is no call of core's `add_`, `sub_` or `mul_` is written.
    assert_eq!(emitted.blocked("not_a_move"), None);
}

/// TYPE-9: a window inside a struct, a tuple, a list or a dictionary is
/// a child of its own, read as its `list<V>` where it is admitted
/// (TYPE-11, TIME-1). A value that holds one is not read whole, and `out`
/// that holds one is not invalidated.
#[test]
fn a_window_within_a_value_is_reported_where_it_is_read() {
    let emitted = emit(&[fixture!(
        "a_window_within_a_value_is_reported_where_it_is_read",
        "m.hgl"
    )]);
    assert_eq!(
        emitted.reported,
        [
            // A struct read whole, bound whole, and a dictionary of
            // windows bound whole.
            "m.hgl:27:35 target.unsupported: the hgraph target does not write a window's value inside another value yet",
            "m.hgl:31:20 target.unsupported: the hgraph target does not write a window's value inside another value yet",
            "m.hgl:43:19 target.unsupported: the hgraph target does not write a window's value inside another value yet",
            // `out` that holds a window, invalidated.
            "m.hgl:55:12 target.unsupported: the hgraph target does not write the invalidation of a window yet",
        ]
    );
}

/// hgraph's `switch_` and `map_` fail as the graph runs where a window,
/// alone or within a bundle or a dictionary, is given to the graph they
/// make (`windows_given`): a window read in the branch of a graph's `if`
/// (CTRL-13), a struct that holds one, and a loop over a dictionary of
/// windows (CTRL-15) are reported. TIME-7, END-5: a second write in one
/// evaluation is the publication, and hgraph's `TSW` takes one value in
/// a cycle, so a window `out` may be written where it is the one place
/// that publishes, out of any loop (`window_published_once`): two writes,
/// one in a loop, and one with a `return` after it are reported, and
/// returns alone are not.
#[test]
fn a_window_a_target_cannot_carry_is_reported() {
    let emitted = emit(&[fixture!(
        "a_window_a_target_cannot_carry_is_reported",
        "m.hgl"
    )]);
    assert_eq!(
        emitted.reported,
        [
            "m.hgl:20:5 target.unsupported: the hgraph target does not write a window given to the graph of a branch or of a loop yet",
            "m.hgl:24:5 target.unsupported: the hgraph target does not write a window given to the graph of a branch or of a loop yet",
            "m.hgl:33:5 target.unsupported: the hgraph target does not write a window given to the graph of a branch or of a loop yet",
            "m.hgl:38:9 target.unsupported: the hgraph target does not write a window that one evaluation may publish to more than once yet",
            "m.hgl:45:34 target.unsupported: the hgraph target does not write a window that one evaluation may publish to more than once yet",
            "m.hgl:50:12 target.unsupported: the hgraph target does not write a window that one evaluation may publish to more than once yet",
        ]
    );
}

/// TYPE-11 and core's END-9: a window is valid "once it holds a value,
/// fewer than its minimum included", and all valid "where it holds at
/// least its minimum": hgraph's `TSW`'s own `valid` and `all_valid`, of
/// ticks and of a duration's span, each size written as the program
/// writes it, a minimum of `0s` among them (hgraph 0.9.8).
/// Its value, "read in a hook or given to a lifted helper (FN-12), is a
/// `list<V>` of the values held, oldest first, and the read before
/// `all_valid` fails with `value.unset_read`" (`_core.window_values`): an
/// input's, `out`'s, a child's read where it is admitted and one checked
/// when it runs (TIME-1), and in `stop`. FN-12: "A window is passed only
/// where it is `all_valid`": a lifted helper is invoked only there. The
/// fixture's tests run on hgraph with the others.
#[test]
fn a_windows_readiness_and_its_list_are_written() {
    let emitted = emit_clean(&[fixture!("demo.windows_wave3.hgl")]);
    let python = emitted.module("demo.windows_wave3");
    let ticks = "_hg.TSW[_py.int, _hg.WindowSize[3], _hg.WindowSize[2]]";
    let seconds = |count: i64| {
        format!(
            "_hg.WindowSize[_dt.timedelta(microseconds={})]",
            count * 1_000_000
        )
    };
    python.assert_writes(&[
        format!("def readings(reading: _hg.TS[_py.int]) -> {ticks}:"),
        "    return _py.tuple([held.valid, held.all_valid])\n".to_string(),
        // A read whole, under ordinary validity and under `all_valid`.
        "    return _core.list_replaced(_output, _core.window_values(held))\n".to_string(),
        "    if held.all_valid:\n        return _core.list_replaced(_output, _core.window_values(held))\n"
            .to_string(),
        "        for value in _core.window_values(held):\n".to_string(),
        "_core.eq__T_T(lhs=_core.window_values(held), rhs=[1, 2])".to_string(),
        "_core.item(_core.window_values(held), 0)".to_string(),
        // FN-12: lifted where it is all valid, one window and two.
        format!(
            "(values: {ticks}) -> _hg.TS[_py.int]:\n    if not (values.all_valid):\n        return\n    return total(values=_core.window_values(values))\n"
        ),
        "    if not (first.all_valid and second.all_valid):\n        return\n    return longer(first=_core.window_values(first), second=_core.window_values(second))\n"
            .to_string(),
        // `out`, `stop`, and a child: a field, a position, and one read
        // when it runs.
        "total(values=_core.window_values(_output))".to_string(),
        "    if held.valid:\n        _state.last = total(values=_core.window_values(held))\n".to_string(),
        "_core.window_values(book[\"prices\"])".to_string(),
        "_core.window_values(_core.position_of(both, 0))".to_string(),
        "_core.checked_child(_core.position_of(both, at.value), lambda _port: _core.window_values(_port))"
            .to_string(),
        "_core.window_values(_core.entry_port(prices, \"a\"))".to_string(),
        "_core.checked_child(_core.entry_port(prices, key.value), lambda _port: _core.window_values(_port))"
            .to_string(),
        "_core.checked_child(_core.position_of(rows, at.value)[\"prices\"], lambda _port: _core.window_values(_port))"
            .to_string(),
        // A duration, and a minimum of `0s` written as it is.
        format!("-> _hg.TSW[_py.float, {}, {}]:", seconds(3), seconds(2)),
        format!(
            "def collect_from_zero(price: _hg.TS[_py.float]) -> _hg.TSW[_py.float, {}, {}]:",
            seconds(3),
            seconds(0)
        ),
        format!(
            "def collect_whole(price: _hg.TS[_py.float]) -> _hg.TSW[_py.float, {}]:",
            seconds(3)
        ),
    ]);
}

/// FN-11: a window "is supplied only where it is `all_valid` as well,
/// whatever its default or null contract (FN-12): one that is valid and
/// not all valid binds no fallback, its default included, and the helper
/// is not invoked": the default binds where the window is not valid, and
/// the node returns where it is valid and not all valid. ERR-14: "a read
/// of a window's value before it is `all_valid`" takes its form from its
/// place: a window child read when it runs where a `result` is taken is
/// `checked_child_result` of the window's list, whose failure is the
/// `err`. `demo.decisions_1120` runs both on hgraph.
#[test]
fn a_window_below_its_minimum_given_a_default_or_a_result_is_written() {
    let emitted = emit_clean(&[fixture!(
        "a_window_below_its_minimum_given_a_default_or_a_result_is_written",
        "m.hgl"
    )]);
    emitted.module("m").assert_writes(&[
        concat!(
            "@_hg.compute_node(valid=())\n",
            "def _lift_0_total(values: _hg.TSW[_py.int, _hg.WindowSize[3], _hg.WindowSize[2]]) -> _hg.TS[_py.int]:\n",
            "    if not ((not values.valid or values.all_valid)):\n",
            "        return\n",
            "    return total(values=(_core.window_values(values) if values.valid else []))\n",
        ),
        "    return _core.checked_child_result(_core.position_of(both, at.value), lambda _port: _core.window_values(_port))\n",
    ]);
}

/// FN-1: a value function's "parameter and result types are ordinary
/// value types", and TYPE-11: a window's value "given to a lifted helper
/// (FN-12), is a `list<V>`": a value function's `T` given a window is
/// that list, passed as it is to a `list<V>` (`_core.window_values`).
/// TIME-7: "`inject out` exposes that same result for reads": a window
/// child of `out`, a field, an element, a field of an element and an
/// entry, is read as its list. A window is no collection (TIME-9): its
/// list indexed is no child. The fixture's tests run on hgraph.
#[test]
fn a_generic_given_a_window_and_outs_window_child_are_lists() {
    let emitted = emit_clean(&[fixture!("demo.windows_fixes.hgl")]);
    let python = emitted.module("demo.windows_fixes");
    python.assert_writes(&[
        "ident(value=_core.window_values(value))",
        "same(a=_core.window_values(a), b=_core.window_values(b))",
        "total(values=_core.window_values(_output[\"prices\"]))",
        "        held = _core.window_values(_output[\"prices\"])\n",
        "        for value in _core.window_values(_output[\"prices\"]):\n",
        "_core.item(_core.window_values(_output[\"prices\"]), 0)",
        "_core.window_values(_core.position_of(_output, slot.value))",
        "_core.window_values(_core.position_of(_output, 0)[\"prices\"])",
        "_core.window_values(_core.entry_port(_output, key.value))",
        "_core.item(_core.window_values(book[\"prices\"]), 0)",
    ]);
}

/// A generic declaration is checked once, its `T` read as a `T` (GEN-7).
/// Where its type parameter stands for a window, a node's hook that
/// reads the parameter's value, and a value function the declaration
/// gives it to (FN-1), are reported: the rules do not say what the read
/// of a `T` that is a window gives, nor that the binding rejects. A
/// node that asks only `valid` of it is written.
#[test]
fn a_window_a_generic_stands_for_is_reported() {
    let emitted = emit(&[fixture!(
        "a_window_a_generic_stands_for_is_reported",
        "m.hgl"
    )]);
    assert_eq!(
        emitted.reported,
        [
            "m.hgl:6:19 target.unsupported: the hgraph target does not write the value of a window that a generic type parameter stands for (GEN-7, TYPE-11) yet",
            "m.hgl:10:28 target.unsupported: the hgraph target does not write a window that a value function's generic type parameter stands for (FN-1) yet",
        ]
    );
}

/// CTRL-13 to CTRL-15: a graph's `if` and `switch`, over a connection
/// and over a `const` parameter.
const CONDITIONALS: (&str, &str) = fixture!("demo.conditionals.hgl");

/// ERR-7 and ERR-8: captures of a node and of a graph, nested.
const CAPTURES: (&str, &str) = fixture!("demo.captures.hgl");

/// ERR-7: "`capture(callable, args...)` creates a temporal error
/// boundary", hgraph's `try_except`, whose `out` is `.value` and whose
/// failures, each made an `error` by the provider, are `.error`. A
/// capture of an outputless call is read the same way. ERR-8: the
/// provider's `captured` lets the failures of a `start` or a `stop` out.
#[test]
fn a_capture_is_a_try_except() {
    let emitted = emit_clean(&[
        CAPTURES,
        fixture!("a_capture_is_a_try_except", "demo.sink.hgl"),
    ]);
    let python = emitted.module("demo.captures");
    python.assert_writes(&[
        "    boundary = _core.captured(_hg.try_except(doubled, value=value))\n    return boundary.out\n",
        "    return is_negative(failure=_core.captured_error(boundary.exception))\n",
        "    outer = _core.captured(_hg.try_except(handled, value=value, repeat=repeat))\n",
        "def handled(value: _hg.TS[_py.int], repeat: _hg.TS[_py.bool]) -> _hg.TS[_core.Error]:\n",
    ]);
    let sink = emitted.module("demo.sink");
    let expected = concat!(
        "    @_hg.graph\n",
        "    def _capture_0(value: _hg.TS[_py.int]) -> None:\n",
        "        return drop(value=value)\n",
        "    boundary = _core.captured(_hg.combine(exception=_hg.try_except(_capture_0, value=value)))\n",
        "    return _core.captured_error(boundary.exception)\n",
    );
    assert!(sink.contains(expected), "{expected}\nnot in\n{sink}");
}

/// ERR-4, ERR-7, ERR-8, ERR-11 and TYPE-13: an error's context,
/// captures of a graph given a wiring-time value and of a lifted value
/// function, lifecycle failures that escape, a temporal `result`, and
/// non-finite numbers as data.
const ERRORS: (&str, &str) = fixture!("demo.errors.hgl");

/// hgraph's `try_except` does not take a graph with a scalar input,
/// and makes one node of two calls of a node with the same inputs.
/// ERR-7: a capture of a node, of a lifted value function (FN-6) or of
/// a graph given a wiring-time value, bound with `let`, is of a graph
/// of the target's own that calls it, binding each wiring-time value:
/// the arguments are evaluated in the order written, before the
/// statement (EXPR-13), a wiring-time value and a connection that is a
/// call each held, and the defaults left out in the call of the child.
#[test]
fn a_capture_binds_a_wiring_time_value_in_a_graph_of_its_own() {
    let emitted = emit_clean(&[ERRORS]);
    let python = emitted.module("demo.errors");
    python.assert_writes(&[
        // A node is called in a graph of the capture's own, which no
        // call outside it shares.
        concat!(
            "    @_hg.graph\n",
            "    def _capture_0(value: _hg.TS[_py.int]) -> _hg.TS[_py.int]:\n",
            "        return checked_node(value=value)\n",
            "    boundary = _core.captured(_hg.try_except(_capture_0, value=value))\n",
        ),
        concat!(
            "    _capture_3_by = 3\n",
            "    @_hg.graph\n",
            "    def _capture_3(value: _hg.TS[_py.int]) -> _hg.TS[_py.int]:\n",
            "        return scaled(value=value, by=_capture_3_by)\n",
            "    boundary = _core.captured(_hg.try_except(_capture_3, value=value))\n",
        ),
        concat!(
            "    _capture_4_by = by\n",
            "    _capture_4_value = forward(value=value)\n",
            "    @_hg.graph\n",
            "    def _capture_4(value: _hg.TS[_py.int]) -> _hg.TS[_py.int]:\n",
            "        return offset(by=_capture_4_by, value=value, plus=10)\n",
            "    boundary = _core.captured(_hg.try_except(_capture_4, value=_capture_4_value))\n",
        ),
        concat!(
            "    def _capture_8(value: _hg.TS[_py.int]) -> None:\n",
            "        return sink_of(value=value, by=_capture_8_by)\n",
            "    boundary = _core.captured(_hg.combine(exception=_hg.try_except(_capture_8, value=value)))\n",
        ),
        concat!(
            "    def _capture_9(value: _hg.TS[_py.int]) -> _hg.TS[_py.int]:\n",
            "        return _lift_5_hundredth(value=value)\n",
            "    boundary = _core.captured(_hg.try_except(_capture_9, value=value))\n",
        ),
        "    return _core.lifted_is_err(value)\n",
    ]);
}

/// ERR-1: "Distinguish source rejection, execution failure, ordinary
/// result data, and test/infrastructure failure." ERR-7: a capture takes
/// a failure of the program, an HGL error, and no other: what the
/// provider's `captured` wires beside a `try_except` raises any other
/// failure again, through a capture around it too. The script runs a
/// node that raises an HGL error and Python's `NotImplementedError`
/// behind the provider's own capture. Needs hgraph, as above.
#[test]
#[ignore = "needs a Python with hgraph, named by HGL_HGRAPH_PYTHON"]
fn a_failure_that_is_no_hgl_error_escapes_every_capture() {
    let hgraph = on_hgraph("hgraph_no_hgl_error");
    // The provider, as the target writes it: its natives and what the
    // generated code names by hand, in the package `_hgl`.
    let package = hgraph.directory.join("_hgl");
    fs::create_dir_all(&package).unwrap();
    fs::write(package.join("__init__.py"), "").unwrap();
    fs::write(
        package.join("_core_native.py"),
        include_str!("../support/core_native.py"),
    )
    .unwrap();
    fs::write(
        package.join("_core_support.py"),
        include_str!("../support/core_support.py"),
    )
    .unwrap();
    let script = hgraph.directory.join("escapes.py");
    fs::write(&script, include_str!("fixtures/escapes.py")).unwrap();
    let ran = std::process::Command::new(&hgraph.python)
        .arg("-I")
        .arg("-B")
        .arg(&script)
        .arg(&hgraph.directory)
        .output()
        .expect("the interpreter starts");
    assert!(
        ran.status.success(),
        "{}",
        String::from_utf8_lossy(&ran.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "[None, 'negative demo.negative', None]\nescaped\nescaped\n"
    );
}

/// ERR-7: a capture of a node, of a lifted call or of a graph given a
/// wiring-time value anywhere but as the value of a `let` is reported,
/// and not written; so is a capture of a lifted operator. A test outside
/// an eval is value code (TST-3), which the analyser holds to that.
#[test]
fn a_capture_outside_a_let_is_reported() {
    let emitted = emit(&[fixture!("a_capture_outside_a_let_is_reported", "m.hgl")]);
    assert_eq!(
        emitted.reported,
        [
            "m.hgl:4:12 target.unsupported: the hgraph target does not write a capture of a node, of a lifted call or of a graph given a wiring-time value, other than as the value of a `let` yet",
            "m.hgl:7:20 target.unsupported: the hgraph target does not write a capture of a lifted operator or native function yet",
            "m.hgl:12:12 target.unsupported: the hgraph target does not write a capture of a node, of a lifted call or of a graph given a wiring-time value, other than as the value of a `let` yet",
        ]
    );
}

/// CTRL-15: over a connection "each case is a child graph selected at
/// runtime", hgraph's `switch_` with a graph for each branch. CTRL-13:
/// "an ordinary value in a branch is then wired as a constant", and with
/// no `else` the `if` "never ticks while the condition is false".
/// CTRL-14: a returning branch has the rest of the block as the other,
/// and "the `var` is one connection: the branch's binding while that
/// branch is selected, and the binding it had before for a branch that
/// does not assign it". Over an ordinary value the one branch is
/// composed: Python's own `if`.
#[test]
fn a_graph_if_and_switch_are_child_graphs() {
    let emitted = emit_clean(&[CONDITIONALS]);
    let python = emitted.module("demo.conditionals");
    python.assert_writes(&[
        // No `else`: the false branch is a graph that never ticks.
        concat!(
            "    @_hg.graph\n",
            "    def _case_0_1(price: _hg.TS[_py.int]) -> _hg.TS[_py.int]:\n",
            "        return _hg.nothing(_hg.TS[_py.int])\n",
            "    return _hg.switch_(open, {True: _case_0_0, False: _case_0_1}, price=price)\n",
        ),
        // An `else if` is a `switch_` in the `else`'s graph, and an
        // ordinary value a constant.
        concat!(
            "    def _case_2_1(score: _hg.TS[_py.int]) -> _hg.TS[_py.str]:\n",
            "        @_hg.graph\n",
            "        def _case_3_0() -> _hg.TS[_py.str]:\n",
            "            return _hg.const(\"mid\")\n",
        ),
        "        return _hg.switch_(_lift_0_gt__T_T(lhs=score, rhs=5), {True: _case_3_0, False: _case_3_1})\n",
        "        return _hg.const(0)\n",
        // A returning branch: the rest of the block is the other graph.
        concat!(
            "    def _case_5_1(value: _hg.TS[_py.int]) -> _hg.TS[_py.int]:\n",
            "        doubled = _lift_1_mul__i64_i64(lhs=value, rhs=2)\n",
            "        return doubled\n",
            "    return _hg.switch_(skip, {True: _case_5_0, False: _case_5_1}, value=value)\n",
        ),
        // A branch that does not assign the `var` gives what it had.
        concat!(
            "    def _case_6_1(price: _hg.TS[_py.int], chosen: _hg.TS[_py.int]) -> _hg.TS[_py.int]:\n",
            "        return chosen\n",
            "    chosen = _hg.switch_(rising, {True: _case_6_0, False: _case_6_1}, price=price, chosen=chosen)\n",
        ),
        // A `switch` with a `default` (CTRL-6).
        "    selected = _hg.switch_(route, {\"left\": _case_8_0, \"right\": _case_8_1, _hg.DEFAULT: _case_8_2}, left=left, right=right, fallback=fallback, selected=selected)\n",
        "    return _hg.switch_(route, {1: _case_9_0, 2: _case_9_1, _hg.DEFAULT: _case_9_2}, left=left, right=right)\n",
        // A `const` condition is decided where the graph is built.
        concat!(
            "def sized(price: _hg.TS[_py.int], wide: _py.bool) -> _hg.TS[_py.int]:\n",
            "    if wide:\n",
            "        return _lift_6_mul__i64_i64(lhs=price, rhs=2)\n",
            "    return price\n",
        ),
        concat!(
            "    if wide:\n",
            "        value = _lift_7_mul__i64_i64(lhs=price, rhs=3)\n",
            "    else:\n",
            "        value = price\n",
        ),
        // A nested `if` is a `switch_` inside the branch's graph.
        "        return _hg.switch_(high, {True: _case_11_0, False: _case_11_1}, price=price)\n",
        // A statement `else if` is the `else`'s one statement: its
        // graph tests the second condition.
        "        chosen = _hg.switch_(second, {True: _case_13_0, False: _case_13_1}, price=price, chosen=chosen)\n",
        // A branch that assigns the `var` on one path only is given the
        // binding it had before, for the other.
        concat!(
            "    def _case_14_0(inner: _hg.TS[_py.bool], price: _hg.TS[_py.int], chosen: _hg.TS[_py.int]) -> _hg.TS[_py.int]:\n",
            "        @_hg.graph\n",
        ),
        // An `if` over a connection in a wiring-time branch is a
        // `switch_` there, not Python's `if` on the port.
        concat!(
            "    if wide:\n",
            "        @_hg.graph\n",
            "        def _case_16_0(price: _hg.TS[_py.int], chosen: _hg.TS[_py.int]) -> _hg.TS[_py.int]:\n",
        ),
        // A read after the branch assigns the `var` is no input.
        "    return _hg.switch_(flag, {True: _case_17_0, False: _case_17_1}, left=left, right=right)\n",
    ]);
}

#[test]
fn notification_and_publication_have_distinct_guards() {
    let emitted = emit_clean(&[NOTIFIED]);
    let python = emitted.module("demo.notified");
    python.assert_writes(&[
        "@_hg.compute_node(valid=())\ndef on_publication(",
        "@_hg.compute_node(valid=())\ndef on_notification(",
        "@_hg.compute_node(valid=())\ndef watch_both(",
        "    if (watched.modified and watched.valid):\n        return True",
        "return (watched.modified and watched.valid)",
        concat!(
            "    if ((left.modified and left.valid) or (right.modified and right.valid)) ",
            "and (probe.modified and probe.valid):\n        return True"
        ),
    ]);
}

/// EXPR-3: "A compound assignment reads the place first". EXPR-2: an
/// expression written once is evaluated once: the index of a compound
/// assignment's place is the index of its read and of its write.
const COMPOUND: (&str, &str) = fixture!("demo.compound.hgl");

/// A compound assignment's index that is more than a name is evaluated
/// once, before the statement, and read and written through what it
/// came to (EXPR-2, EXPR-3).
#[test]
fn a_compound_assignment_evaluates_its_place_once() {
    let emitted = emit_clean(&[COMPOUND]);
    let python = emitted.module("demo.compound");
    assert!(
        python.contains(concat!(
            "    _at_1 = next_slot(store=_store)\n",
            "    items = _core.set_item(items, _at_1, ",
            "_core.add__i64_i64(lhs=_core.item(items, _at_1), rhs=1))\n",
        )),
        "{python}"
    );
    // One call of `next_slot` for each index written.
    let nested = python
        .split("def nested(")
        .nth(1)
        .and_then(|rest| rest.split("\n\n\n").next())
        .expect("the node is written");
    assert_eq!(nested.matches("next_slot(").count(), 2, "{nested}");
}

/// FN-9: "A graph injects `global_state` and no other, and uses it where
/// the graph is built: each call on it is evaluated once then, in the
/// order written, before any node of that graph starts". So a graph
/// that wraps another sets the store for it, and a test evaluates the
/// wrapper. RUN-4: each eval is a run with a store of its own.
const WRAPPED: (&str, &str) = fixture!("demo.wrapped.hgl");

/// A graph's `inject global_state` is the run's store where the graph's
/// function runs, and a call on it there is run then (FN-9).
#[test]
fn a_graph_sets_the_store_where_it_is_built() {
    let emitted = emit_clean(&[WRAPPED]);
    let python = emitted.module("demo.wrapped");
    python.assert_writes(&[
        concat!(
            "@_hg.graph\n",
            "def seeded(limit: _py.int) -> _hg.TS[_py.int]:\n",
            "    _store = _hg.GlobalState.instance()\n",
            "    _core.put_GlobalState_str_V(store=_store, key=\"limit\", value=limit)\n",
            "    return counted(every=_dt.timedelta(microseconds=1000000))\n",
        ),
        concat!(
            "    _store = _hg.GlobalState.instance()\n",
            "    _core.put_GlobalState_str_V(store=_store, key=\"factor\", value=first)\n",
            "    factor = _core.get_GlobalState_str(store=_store, key=\"factor\")\n",
            "    return scaled(value=value, factor=factor)\n",
        ),
    ]);
}

/// OPS-4: "`convert<T>(value)` converts a value to the type T." OPS-6
/// says what each conversion yields, OPS-7 which can fail, and OPS-8
/// their options. Its tests assert what core's own
/// (`tests/implementation/conversions.hgl`, run by `check_core.py`)
/// does not.
const CONVERSIONS: (&str, &str) = fixture!("demo.converted.hgl");

/// OPS-4: core's candidates of `convert` take one type and give
/// different ones, so each is the provider's function named with what
/// it gives. OPS-8: `invalid` is given as the name of its member, as
/// division's `zero` is (NUM-9), and `rounding` and `nan` as arguments.
#[test]
fn a_conversion_is_the_providers_function_for_what_it_gives() {
    let emitted = emit_clean(&[CONVERSIONS]);
    let python = emitted.module("demo.converted");
    python.assert_writes(&[
        "def as_float(value):\n    return _core.convert_i64_to_f64(value=value)\n",
        "def as_flag(value):\n    return _core.convert_f64_to_bool(value=value)\n",
        concat!(
            "def as_whole(value):\n",
            "    return _core.convert_f64_Rounding_i64__to_i64(value=value, ",
            "rounding=_hgl_4core.Rounding[\"truncate\"], nan=None, invalid=\"fail\")\n",
        ),
        concat!(
            "def checked(value):\n",
            "    return _core.convert_f64_Rounding_i64__to_i64(value=value, ",
            "rounding=_hgl_4core.Rounding[\"truncate\"], nan=None, invalid=\"error\")\n",
        ),
        "def flag(text):\n    return _core.convert_str_to_bool(value=text, invalid=\"error\")\n",
        concat!(
            "    return _core.convert_bytes_to_str(",
            "value=_core.convert_str_to_bytes(value=text), invalid=\"fail\")\n",
        ),
        // FN-6: the lifted conversion reads the connection's value.
        "    return _core.convert_i64_to_f64(value=value.value)\n",
        "    return _core.convert_f64_to_str(value=value.value)\n",
        concat!(
            "def again(text):\n",
            "    return _core.convert_f64_to_str(",
            "value=_core.convert_str_to_f64(value=text, invalid=\"fail\"))\n",
        ),
    ]);
}

/// TST-5: "A trace written out is dense, `[7, _, 9]`, or timed, `[0s: 7,
/// 2s: 9]`." TST-7: "Timed entries use duration offsets from run start
/// or absolute datetime keys ... Sparse expectations enumerate exactly
/// the actual output ticks and times."
const TIMED: (&str, &str) = fixture!("demo.timed.hgl");

/// A timed trace is given to the harness as its entries, each a key and
/// a publication, in the order written (TST-7).
#[test]
fn a_timed_trace_is_its_entries() {
    let emitted = emit_clean(&[TIMED]);
    let python = emitted.module("demo.timed");
    python.assert_writes(&[
        concat!(
            "_harness.evaluate(pass_, {\"value\": _hgl_4core._inst_replay__i64}, ",
            "_hgl_4core._inst_record__i64, ",
            "value=_harness.timed([(_dt.timedelta(microseconds=0), 1), ",
            "(_dt.timedelta(microseconds=2000000), 2)])), ",
            "_harness.timed([(_dt.timedelta(microseconds=0), 1), ",
            "(_dt.timedelta(microseconds=2000000), 2)]))",
        ),
        "_harness.timed([(later(), 7)])",
    ]);
}

/// TST-17: "An integration test ... is a named test that may run in
/// real time: beside `eval` it writes `run(call, config)` (RUN-3),
/// whose value is the run's output as an eval's is."
const INTEGRATION: (&str, &str) = fixture!("integration", "demo.live.hgl");

/// TST-17: an integration test runs on hgraph, its `run` in real time.
/// It is held to a time limit, the one `@timeout(...)` gives it or a
/// minute: one still running when its limit passes is stopped, and has
/// failed, with the statement it was at. The test after it runs as any
/// does. Needs hgraph, as above.
#[test]
#[ignore = "needs a Python with hgraph, named by HGL_HGRAPH_PYTHON"]
fn an_integration_test_runs_in_real_time_within_its_limit() {
    let hgraph = on_hgraph("hgraph_integration");
    let emitted = emit_clean(&[INTEGRATION]);
    let all: Vec<usize> = (0..emitted.output.tests.len()).collect();
    // An integration test that writes no limit is given three seconds
    // here, where it has a minute: long enough for a slow host to run
    // the ones that end. One that writes its own keeps it.
    let ran = run_within(
        &hgraph.python,
        &hgraph.directory,
        &emitted.output,
        &all,
        &|test: &Test| {
            test.integration
                .then(|| test.timeout.unwrap_or(std::time::Duration::from_secs(3)))
        },
    );
    let described = outcomes(&emitted, &all, &ran);
    assert_eq!(
        described,
        [
            "bounded passed",
            "beats_live passed",
            "passes_through passed",
            "endless failed demo.live.hgl:35:9 timed out: it was still running after its limit of 3s",
            "after passed",
            "brief failed demo.live.hgl:44:9 timed out: it was still running after its limit of 0.5s",
        ]
    );
}

/// TIME-7: a `stop` reads what the node last published. The run whose
/// `stop` expects that value ends well, and the one whose `stop`
/// expects another fails there: `out` is read, and is what was
/// published. Needs hgraph, as above.
#[test]
#[ignore = "needs a Python with hgraph, named by HGL_HGRAPH_PYTHON"]
fn a_stop_reads_what_the_node_published() {
    let hgraph = on_hgraph("hgraph_out_in_stop");
    let emitted = emit_clean(&[OUT_IN_STOP]);
    let ran = hgraph.run_all(&emitted);
    let described: Vec<String> = emitted
        .output
        .tests
        .iter()
        .zip(&ran)
        .map(|(test, ran)| match ran {
            Ran::Passed => format!("{} passed", test.name),
            // The message is hgraph's account of the node that failed,
            // which holds the error's own text.
            Ran::Failed { message, .. } => format!(
                "{} failed: {}",
                test.name,
                if message.contains("numeric.division_by_zero: a division by zero") {
                    "a division by zero"
                } else {
                    message.as_str()
                }
            ),
            Ran::NotRun(reason) => format!("{} not run: {reason}", test.name),
        })
        .collect();
    assert_eq!(
        described,
        [
            "reads_what_was_published passed",
            "reads_no_other failed: a division by zero",
        ]
    );
}

/// OPS-8: under `invalid: .fail` a conversion "returns T, and a failure
/// is an escaping HGL error". The error that escapes fails the test
/// that was running, at the statement it was at. Needs hgraph, as above.
#[test]
#[ignore = "needs a Python with hgraph, named by HGL_HGRAPH_PYTHON"]
fn a_conversion_that_fails_under_fail_escapes() {
    let hgraph = on_hgraph("hgraph_escapes");
    let emitted = emit_clean(&[fixture!(
        "a_conversion_that_fails_under_fail_escapes",
        "demo.escapes.hgl"
    )]);
    let all: Vec<usize> = (0..emitted.output.tests.len()).collect();
    let ran = hgraph.run(&emitted, &all);
    let described = outcomes(&emitted, &all, &ran);
    assert_eq!(
        described,
        [
            "fits passed",
            "too_large failed demo.escapes.hgl:4:18 HglError: convert.invalid: the number is outside `i64` once rounded",
            "no_number failed demo.escapes.hgl:5:18 HglError: convert.invalid: a NaN is no `i64`",
            "no_flag failed demo.escapes.hgl:6:16 HglError: convert.invalid: the text is no `bool` literal",
        ]
    );
}

/// RUN-3: a run in real time goes on "until the configuration's end or
/// until its host stops it". A stop the host asks for ends the run as
/// its end does: what it recorded is its output. One asked for before
/// the run starts is kept for it, and a second ends the run as it is,
/// with no output (core's CRUN-6). Needs hgraph, as above.
#[test]
#[ignore = "needs a Python with hgraph, named by HGL_HGRAPH_PYTHON"]
fn a_run_in_real_time_is_stopped_by_its_host() {
    let emitted = emit(&APPLICATIONS);
    let endless = emitted
        .output
        .applications
        .iter()
        .position(|application| application.path == "demo.ticks.endless")
        .expect("the application is written");
    let ran = |name: &str, stop: &Stop| {
        let hgraph = on_hgraph(name);
        run_application(
            &hgraph.python,
            &hgraph.directory,
            &emitted.output,
            endless,
            stop,
        )
    };

    // Asked while the run goes, from another thread.
    let stop = Stop::new();
    let asked = stop.clone();
    let asking = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(1500));
        asked.ask();
    });
    let stopped = ran("hgraph_stopped", &stop);
    asking.join().expect("the stop is asked");
    let RanApplication::Stopped {
        output: Some(published),
        by_host: true,
    } = stopped
    else {
        panic!("the host stops the run: {stopped:?}");
    };
    // It counted from 1 for as long as it ran: on a slow host that may
    // be no time at all, where the stop is asked before the run starts.
    for (position, (_, value)) in published.iter().enumerate() {
        assert_eq!(*value, (position + 1).to_string());
    }

    // Asked before the run starts: it is stopped as soon as it can be.
    let stop = Stop::new();
    stop.ask();
    assert!(matches!(
        ran("hgraph_stopped_early", &stop),
        RanApplication::Stopped {
            output: Some(_),
            by_host: true
        }
    ));

    // Asked twice: the run is ended as it is, and has no output.
    let stop = Stop::new();
    stop.ask();
    stop.ask();
    assert!(matches!(
        ran("hgraph_stopped_twice", &stop),
        RanApplication::NotRun(_) | RanApplication::Failed(_)
    ));
}

/// A test that fails on hgraph says which of its statements failed,
/// and why: an assertion that does not hold, or an error that escaped
/// (NUM-2: an `i64` result out of range; NUM-3: "The `fail` policy
/// makes a zero divisor an HGL error with code
/// `numeric.division_by_zero`"). ERR-5: a failure a `?` meets in a
/// `when` fails the node, and the test with it. NUM-4: "An invalid
/// tolerance is an HGL error". TYPE-7: an enum constructed from "a
/// value that is none fails where it is evaluated". TYPE-5: a
/// construction's arguments and defaults. CTRL-6: a `switch` no case
/// of which matches. EXPR-3, ERR-6: an index outside a list, and a key
/// a dictionary has not. Needs hgraph, as above.
#[test]
#[ignore = "needs a Python with hgraph, named by HGL_HGRAPH_PYTHON"]
fn a_failure_names_the_statement_that_failed() {
    let hgraph = on_hgraph("hgraph_failures");
    let mut files = REACH.to_vec();
    files.push(DIVISION_FAILS);
    files.push(RESULT_FAILS);
    files.push(CLOSE_FAILS);
    files.push(ENUM_FAILS);
    files.push(STRUCT_FAILS);
    files.push(RAISES_FAILS);
    files.push(BOUNDED_FAILS);
    files.push(LOOP_FAILS);
    files.push(ELEMENT_FAILS);
    files.push(STOP_FAILS);
    files.push(CAPABILITIES_FAIL);
    files.push(SET_FAILS);
    files.push(GIVEN_FAILS);
    let emitted = emit(&files);
    let tests: Vec<usize> = [
        "direct",
        "wrong",
        "overflows",
        "too_long",
        "too_late",
        "whole_by_zero",
        "strict_by_zero",
        "negative_tolerance",
        "no_number",
        "no_name",
        "given",
        "omitted",
        "in_order",
        "unequal",
        "unmatched",
        "beyond",
        "before",
        "absent",
        "outside",
        "removed_is_not_added",
        "one_more",
        "removed_first",
        "silence_counted",
        "nothing_raised",
        "another_code",
        "nested_assertion",
        "nested_raises",
        "nested_then_raised",
        "both_ends",
        "too_dense",
    ]
    .iter()
    .map(|name| emitted.test(name).0)
    .collect();
    let ran = hgraph.run(&emitted, &tests);
    assert_eq!(
        outcomes(&emitted, &tests, &ran),
        [
            "direct passed",
            "wrong failed reach.hgl:22:5 the assertion does not hold",
            "overflows failed reach.hgl:26:5 HglError: numeric.out_of_range: an i64 result is outside the signed 64-bit range",
            "too_long failed reach.hgl:32:5 HglError: numeric.out_of_range: a duration result is outside the signed 64-bit range of microseconds",
            "too_late failed reach.hgl:37:5 HglError: numeric.out_of_range: an instant result is outside the years 1 to 9999",
            "whole_by_zero failed demo.div.fails.hgl:4:22 HglError: numeric.division_by_zero: a division by zero",
            "strict_by_zero failed demo.div.fails.hgl:5:23 HglError: numeric.division_by_zero: a division by zero",
            "negative_tolerance failed demo.close.fails.hgl:3:27 HglError: the tolerance `abs` is not finite and nonnegative",
            "no_number failed demo.enums.fails.hgl:5:18 HglError: `Side` has no member 9",
            "no_name failed demo.enums.fails.hgl:6:16 HglError: `Side` has no member 'none'",
            // TYPE-5: "Defaults are evaluated only when omitted", and
            // "Written arguments evaluate once in source order; failure
            // prevents later arguments."
            "given passed",
            "omitted failed demo.structs.fails.hgl:10:16 HglError: numeric.division_by_zero: a division by zero",
            "in_order failed demo.structs.fails.hgl:11:17 HglError: numeric.out_of_range: an i64 result is outside the signed 64-bit range",
            "unequal failed demo.structs.fails.hgl:12:16 the assertion does not hold",
            // CTRL-6: "An unmatched selector uses default or fails."
            "unmatched failed demo.loops.fails.hgl:9:18 HglError: no case of the `switch` has the value 'z'",
            // EXPR-3: "An index outside a list fails where it is
            // evaluated", and ERR-6: a strict read of a key that is
            // not there.
            "beyond failed demo.elements.fails.hgl:9:15 HglError: the index 2 is outside a sequence of 2",
            "before failed demo.elements.fails.hgl:10:15 HglError: the index -1 is outside a sequence of 2",
            "absent failed demo.elements.fails.hgl:11:15 HglError: value.missing_key: the dictionary has no key 'z'",
            "outside failed demo.elements.fails.hgl:12:16 HglError: the index 5 is outside a sequence of 2",
            // TST-4: a delta that removed an element is not one that
            // added it, and a trace of one cell is not one of two.
            "removed_is_not_added failed demo.sets.fails.hgl:9:5 the assertion does not hold",
            "one_more failed demo.sets.fails.hgl:12:5 the assertion does not hold",
            // EXPR-13: a delta's properties are evaluated as written.
            "removed_first failed demo.sets.fails.hgl:18:26 HglError: numeric.division_by_zero: a division by zero",
            // TST-16: a `_` of a dense trace gives a `const fn` no
            // tuple, so two publications are counted, and not three.
            "silence_counted failed demo.given.fails.hgl:10:24 the assertion does not hold",
            // TST-10: a block that completes fails at the assertion, and
            // one that lets out another code fails where that was
            // raised, with what it says. TST-11: a failed assertion in
            // the block is the test's failure.
            "nothing_raised failed demo.raises.fails.hgl:6:5 AssertionError: the block raised nothing, and `numeric.division_by_zero` was expected",
            "another_code failed demo.raises.fails.hgl:9:49 HglError: numeric.out_of_range: an i64 result is outside the signed 64-bit range",
            "nested_assertion failed demo.raises.fails.hgl:13:9 the assertion does not hold",
            // A refused error fails the inner assertion, and is no
            // match for the outer one though it has its code.
            "nested_raises failed demo.raises.fails.hgl:20:46 HglError: numeric.division_by_zero: a division by zero",
            "nested_then_raised passed",
            // CRUN-2: an `end` with a `length`. TST-6: a publication a
            // second after the start is not one at the start.
            "both_ends failed demo.bounded.fails.hgl:9:5 HglError: run.end_and_length: a run's configuration gives both an `end` and a `length`",
            "too_dense failed demo.bounded.fails.hgl:12:5 the assertion does not hold",
        ]
    );

    // hgraph words a node's failure itself, and the error's own line
    // is in what it says.
    let node = [emitted.test("node_fails").0];
    let (at, message) = failed(&emitted, &hgraph.run(&emitted, &node));
    assert_eq!(at, Some("demo.results.fails.hgl:6:19".to_string()));
    assert!(
        message.contains("HglError: numeric.division_by_zero: a division by zero"),
        "{message}"
    );

    // TIME-5: a `stop` reads an input under its guard. The input is
    // valid here, so the guarded statement is run, and fails.
    let stop = [emitted.test("stop_fails").0];
    let (at, message) = failed(&emitted, &hgraph.run(&emitted, &stop));
    assert_eq!(at, Some("demo.stops.fails.hgl:9:19".to_string()));
    assert!(
        message.contains("HglError: numeric.division_by_zero: a division by zero"),
        "{message}"
    );

    // CAP-6: "an absent key fails the node."
    let absent = [emitted.test("an_absent_key_fails_the_node").0];
    let (at, message) = failed(&emitted, &hgraph.run(&emitted, &absent));
    assert_eq!(at, Some("demo.caps.fails.hgl:7:9".to_string()));
    assert!(
        message.contains("HglError: the global store holds nothing under 'nothing'"),
        "{message}"
    );
}

/// TIME-2, TIME-3, TIME-9: the target changes the subscription on the
/// input itself. Once a node is running, a publication can satisfy the
/// data handler even if another input woke the node.
#[test]
fn changing_input_activity_preserves_a_modified_guard() {
    let emitted = emit_clean(&[fixture!(
        "changing_input_activity_preserves_a_modified_guard",
        "activity.hgl"
    )]);
    let python = emitted.module("activity");
    python.assert_writes(&[
        "data.make_active()",
        "data.make_passive()",
        "if data.valid and data.modified:",
    ]);
}

/// END-5, TIME-3: querying activity reads the input at that point in the
/// handler or in stop. A query in a header adds no subscription or validity
/// requirement and stays in the executable condition.
#[test]
fn an_activity_query_in_a_guard_reads_the_direct_input() {
    let emitted = emit_clean(&[fixture!(
        "an_activity_query_in_a_guard_reads_the_direct_input",
        "observed.hgl"
    )]);
    let python = emitted.module("observed");
    python.assert_writes(&[
        "@_hg.compute_node(active=(\"probe\",), valid=())\ndef sample(",
        "data.active:\n        data.make_passive()\n        return data.active\n",
        "    final_activity = data.active\n",
    ]);
}

/// TST-5, FN-6, TYPE-16: the replay publishes a nullable or a set to
/// the lifted function. Its wire carries the payload, not the trace delta.
#[test]
fn a_value_helper_in_eval_is_lifted_with_the_replayed_payload_type() {
    let emitted = emit_clean(&[fixture!(
        "a_value_helper_in_eval_is_lifted_with_the_replayed_payload_type",
        "payload.hgl"
    )]);
    let python = emitted.module("payload");
    assert!(python.contains("def _lift_0_keep(value: _hg.TS[_py.int | None])"));
    assert!(python.contains("def _lift_1_from_members(values: _hg.TSS[_py.int])"));
}

/// TYPE-9: "Temporalization recursively maps tuples/structs to child
/// fields ... `atomic<V>` stops expansion at a complete ordinary V".
/// TYPE-16: a struct's delta has "a property for each field".
const TEMPORAL_STRUCTS: (&str, &str) = fixture!("demo.bundles.hgl");

/// A temporal struct is hgraph's bundle over a schema the provider
/// makes beside the struct's class, each field temporalized in turn;
/// `atomic<Quote>` is a `TS` of the class. A field is the field's port
/// in a graph and in a hook, and a struct's delta is a `dict` of the
/// fields that changed.
#[test]
fn a_temporal_struct_is_a_bundle_of_its_fields() {
    let emitted = emit_clean(&[TEMPORAL_STRUCTS]);
    let python = emitted.module("demo.bundles");
    let quote = concat!(
        "_hg.TSB[_core.schema(Quote, ",
        "{\"bid\": _hg.TS[_py.int], \"ask\": _hg.TS[_py.int]})]"
    );
    python.assert_writes(&[
        format!("def quoted(bid: _hg.TS[_py.int], ask: _hg.TS[_py.int]) -> {quote}:"),
        "    return _core.bundled(Quote(bid=bid.value, ask=ask.value))".to_string(),
        "        return _core.entries_published(_output, _core.struct_delta(bid=price.value), (\"fields\", {}))".to_string(),
        format!(
            "_output: _hg.TSB_OUT[{}] = None",
            &quote["_hg.TSB[".len()..quote.len() - 1]
        ),
        format!(
            "    return _core.sub__i64_i64(lhs={}, rhs={})",
            part("quote[\"ask\"]"),
            part("quote[\"bid\"]")
        ),
        format!(
            "    held = Quote(bid={}, ask={})",
            part("quote[\"bid\"]"),
            part("quote[\"ask\"]")
        ),
        "    bid = _core.changed(quote.delta_value, \"bid\")".to_string(),
        format!("\"quote\": {quote}, \"size\": _hg.TS[_py.int]"),
        format!("    return {}", part("order[\"quote\"][\"bid\"]")),
        "-> _hg.TS[Quote]:".to_string(),
        "    return quote.value.ask".to_string(),
        "    return moved(is_bid=is_bid, price=price)[\"ask\"]".to_string(),
        "    return ordered(bid=bid, size=size)[\"quote\"][\"bid\"]".to_string(),
        "    return _hg.getattr_[_hg.SCALAR: _py.int](snapshot(bid=bid, ask=ask), \"ask\")"
            .to_string(),
        // A source's yield, and a lifted helper's argument and result.
        "    yield (_dt.timedelta(microseconds=1), _core.bundled(Quote(bid=1, ask=2)))".to_string(),
        format!(
            "    return width(quote=Quote(bid={}, ask={}))",
            part("quote[\"bid\"]"),
            part("quote[\"ask\"]")
        ),
        format!(
            "    return _core.bundled(widened(quote=Quote(bid={}, ask={})))",
            part("quote[\"bid\"]"),
            part("quote[\"ask\"]")
        ),
    ]);
}

/// TYPE-9: a dictionary field of a temporal struct is a `TSD`, which
/// takes an update as keys changed. A whole struct published with such a
/// field would not remove the keys it has not, so it is reported, and
/// its delta is not.
#[test]
fn a_whole_struct_with_a_dictionary_field_is_not_published_yet() {
    let emitted = emit(&[fixture!(
        "a_whole_struct_with_a_dictionary_field_is_not_published_yet",
        "m.hgl"
    )]);
    assert_eq!(
        emitted.reported,
        [
            "m.hgl:6:19 target.unsupported: the hgraph target does not write a whole dictionary published to a temporal dictionary yet"
        ]
    );
}

/// CTRL-13 and TYPE-9: an ordinary fixed list or dictionary of structs
/// in a branch of a graph's `if` would be a constant published whole,
/// which is not written for composite elements: reported.
#[test]
fn an_ordinary_list_of_structs_branch_of_a_graph_is_not_written_yet() {
    let emitted = emit(&[fixture!(
        "an_ordinary_list_of_structs_branch_of_a_graph_is_not_written_yet/1",
        "m.hgl"
    )]);
    assert_eq!(
        emitted.reported,
        [
            "m.hgl:11:12 target.unsupported: the hgraph target does not write a whole list published where an element is temporal composite or nullable yet"
        ]
    );
    // A dictionary of structs as a branch's constant would be given each
    // struct whole, where its `TSD`'s children take their fields.
    let emitted = emit(&[fixture!(
        "an_ordinary_list_of_structs_branch_of_a_graph_is_not_written_yet/2",
        "m.hgl"
    )]);
    assert_eq!(
        emitted.reported,
        [
            "m.hgl:11:12 target.unsupported: the hgraph target does not write an ordinary dictionary of composites as a branch's connection yet"
        ]
    );
}

/// TYPE-9: temporalization maps "lists to child positions, dictionaries
/// to keyed children". TYPE-10: "`list<T>` denotes a growing shape".
/// TYPE-16: a list's delta is a `dict<i64, delta<V>>`, a dictionary's
/// has `updated` and `removed`.
const TEMPORAL_COLLECTIONS: (&str, &str) = fixture!("demo.collections.hgl");

/// A fixed list is hgraph's `TSL`, a dictionary its `TSD`, and an
/// unsized list hgraph's unbounded `TSL`, `Size[-1]`. An element is the
/// child's port in a graph and in a hook; a whole value is read in a
/// hook and given to a lifted helper as a Python list or dict, a list's
/// once each of its positions is valid (`list_value`).
#[test]
fn a_temporal_list_or_dictionary_is_written_by_its_children() {
    let emitted = emit_clean(&[TEMPORAL_COLLECTIONS]);
    let python = emitted.module("demo.collections");
    let pair = "_hg.TSL[_hg.TS[_py.int], _hg.Size[2]]";
    let prices = "_hg.TSD[_py.str, _hg.TS[_py.int]]";
    python.assert_writes(&[
        "-> _hg.TSL[_hg.TS[_py.int], _hg.Size[-1]]:".to_string(),
        "        return _core.list_replaced(_output, [])".to_string(),
        "        return (_core.list_position(values, 1).value if _core.list_position(values, 1).valid else _core.unset_read())".to_string(),
        "    return _core.list_replaced(_output, doubled(values=_core.list_value(values)))".to_string(),
        format!("-> {pair}:"),
        "    return _core.list_within(_core.list_delta(updated={slot.value: reading.value}), 2)".to_string(),
        "    return _py.tuple([low.value, _core.add__i64_i64(lhs=low.value, rhs=1)])".to_string(),
        // TIME-9: a fixed list is valid once any position is.
        format!("@_hg.compute_node\ndef pair_span(pair: {pair})"),
        "store=_core.updated(pair.delta_value), key=1".to_string(),
        format!("@_hg.compute_node\ndef _lift_3_pair_total(pair: {pair})"),
        "    return one_position(slot=slot, reading=reading)[0]".to_string(),
        format!("-> {prices}:"),
        "        return _core.entries_published(_output, _core.dict_delta(removed=_py.frozenset((key.value,))), \"entries\")".to_string(),
        "    return _core.entries_published(_output, _core.dict_delta(updated={key.value: price.value}), \"entries\")".to_string(),
        "store=_py.dict(prices.value), key=\"B\"".to_string(),
        "    for key in _core.removed(change):".to_string(),
        "store=_core.updated(change), key=\"A\"".to_string(),
        "    return _core.entry_of(priced(key=key, price=price), _hg.const(\"B\", tp=_hg.TS[_py.str]))".to_string(),
        "    return _core.entry_of(priced(key=key, price=price), chosen)".to_string(),
    ]);
}

/// What this target does not write of a temporal list or dictionary
/// is reported: a whole dictionary of composites published, a nullable
/// element of a fixed list published whole, and a list's element at a
/// position that ticks. A removed position written to a fixed list, an
/// unsized list's delta read, an unsized list of composite elements and
/// one as the child of a dictionary are written (hgraph 0.9.8's `TSL`).
#[test]
fn a_temporal_collection_form_not_written_yet_is_reported() {
    let emitted = emit(&[fixture!(
        "a_temporal_collection_form_not_written_yet_is_reported",
        "m.hgl"
    )]);
    assert_eq!(
        emitted.reported,
        [
            "m.hgl:15:19 target.unsupported: the hgraph target does not write a whole dictionary of composites yet",
            "m.hgl:18:19 target.unsupported: the hgraph target does not write a whole list published where an element is temporal composite or nullable yet",
            "m.hgl:23:44 target.unsupported: the hgraph target does not write a list's element at a position that ticks yet",
        ]
    );
}

/// The scalars as temporal values and as configuration (TYPE-1, TYPE-2,
/// TYPE-14, TYPE-17), what an endpoint is observed to hold (TIME-1,
/// END-1, END-2), and value and lifted calls of one computation (NUM-8).
const SCALARS: (&str, &str) = fixture!("demo.scalars.hgl");

/// TYPE-1: "`bool`, signed 64-bit `i64`, binary64 `f64`, Unicode `str`,
/// and octet-sequence `bytes` retain distinct identities": `bytes` is
/// Python's. TYPE-17: a tenor is the provider's `Tenor`. TYPE-2: "named
/// `timezone`, resolved `zoned_datetime`, and date-independent
/// `zoned_time`" are the provider's classes, each a temporal value, and
/// TYPE-14 a nullable one. LEX-12: "The selected run provider checks
/// exact catalog membership and offset consistency": a zone literal is
/// made by the provider's function, which checks it.
#[test]
fn every_scalar_is_a_temporal_type() {
    let emitted = emit_clean(&[SCALARS]);
    let python = emitted.module("demo.scalars");
    python.assert_writes(&[
        "def pass_bytes(value: _hg.TS[_py.bytes]) -> _hg.TS[_py.bytes]:",
        "def pass_tenor(value: _hg.TS[_core.Tenor]) -> _hg.TS[_core.Tenor]:",
        "def pass_zone(value: _hg.TS[_core.Timezone]) -> _hg.TS[_core.Timezone]:",
        "def pass_zoned(value: _hg.TS[_core.ZonedDatetime]) -> _hg.TS[_core.ZonedDatetime]:",
        "def pass_zoned_time(value: _hg.TS[_core.ZonedTime]) -> _hg.TS[_core.ZonedTime]:",
        "def maybe_bytes(value: _hg.TS[_py.bytes | None]) -> _hg.TS[_py.bytes | None]:",
        "def maybe_tenor(value: _hg.TS[_core.Tenor | None]) -> _hg.TS[_core.Tenor | None]:",
        "def maybe_zoned(value: _hg.TS[_core.ZonedDatetime | None]) -> _hg.TS[_core.ZonedDatetime | None]:",
        "def zoned_on(tick: _hg.TS[_py.bool], zone: _core.Timezone, lag: _core.Tenor) -> _hg.TS[_core.Tenor]:",
        "_core.timezone(\"Europe/London\")",
        "_core.zoned_time(_dt.time(9, 30, 0, 0), \"Europe/London\")",
        "_core.zoned_datetime(_dt.datetime(2026, 9, 3, 9, 30, 0, 0), 60, \"Europe/London\")",
        "_core.add__zoned_datetime_duration(lhs=",
    ]);
}

/// TYPE-2: "Instants/durations use signed 64-bit microseconds", and an
/// instant on hgraph is a Python `datetime`, of the years 1 to 9999. A
/// literal written with an offset near either end whose instant in UTC
/// is outside them is reported where it is written; one inside is
/// written.
#[test]
fn an_instant_outside_the_calendar_in_utc_is_reported() {
    let emitted = emit(&[fixture!(
        "an_instant_outside_the_calendar_in_utc_is_reported",
        "edges.hgl"
    )]);
    assert_eq!(
        emitted.reported,
        [
            "edges.hgl:2:22 target.unsupported: the hgraph target does not write an instant outside the years 1 to 9999 yet",
            "edges.hgl:3:23 target.unsupported: the hgraph target does not write an instant outside the years 1 to 9999 yet",
            "edges.hgl:4:23 target.unsupported: the hgraph target does not write an instant outside the years 1 to 9999 yet",
        ]
    );
    assert_eq!(emitted.blocked("inside"), None);
}

/// LEX-12: "The selected run provider checks exact catalog membership
/// and offset consistency". A name the run's zone catalogue has not
/// fails where it is evaluated, and so does an offset that is not the
/// zone's at the instant it gives: London is an hour east of UTC in
/// September, and skips 01:30 on 29 March 2026. No execution code is
/// catalogued for either. Needs hgraph, as above.
#[test]
#[ignore = "needs a Python with hgraph, named by HGL_HGRAPH_PYTHON"]
fn the_run_provider_checks_a_zone() {
    let hgraph = on_hgraph("hgraph_zones");
    let emitted = emit_clean(&[fixture!("the_run_provider_checks_a_zone", "zones.hgl")]);
    let all: Vec<usize> = (0..emitted.output.tests.len()).collect();
    let ran = hgraph.run(&emitted, &all);
    let described = outcomes(&emitted, &all, &ran);
    let absent = "HglError: the zone `Mars/Olympus` is not in the run provider's zone catalogue";
    let offset = "is not the offset of the zone `Europe/London` at that time";
    assert_eq!(
        described,
        [
            format!("unknown failed zones.hgl:2:20 {absent}"),
            format!("unknown_time failed zones.hgl:3:25 {absent}"),
            format!(
                "inconsistent failed zones.hgl:4:25 HglError: the offset of `2026-09-03T09:30:00` {offset}"
            ),
            format!(
                "skipped failed zones.hgl:5:20 HglError: the offset of `2026-03-29T01:30:00` {offset}"
            ),
            "consistent passed".to_string(),
        ]
    );
}

/// TIME-9: "`valid` of a collection is true where any element, field or
/// entry is valid ... `all_valid` is true where every one is", hgraph's
/// `valid` and `all_valid`, the language's `valid` of a list where
/// hgraph's `TSL` stays valid once every position has lost its value
/// (`valid_of`). TYPE-16: "A delta written with a smaller
/// `size_modified` and no `removed` shrinks the list": an unsized list,
/// hgraph's unbounded `TSL`, counts its size from what its `_output`
/// holds, and a source's node applies what it yields as it is due. A
/// fixed list's `removed` is read: hgraph 0.9.8's bounded `TSL` reports
/// it.
#[test]
fn collection_validity_and_a_lists_delta_are_written() {
    let emitted = emit_clean(&[fixture!(
        "collection_validity_and_a_lists_delta_are_written",
        "m.hgl"
    )]);
    let python = emitted.module("m");
    python.assert_writes(&[
        "(_core.valid_of(pair, (\"list\", None)) and pair.all_valid)",
        "_output: _hg.TS_OUT[_hg.TSL[_hg.TS[_py.int], _hg.Size[-1]]] = None",
        "return _core.list_applied(_output, _core.list_delta(size_modified=size.value))",
        "    _core.resume(_state._source, _clock.evaluation_time, _scheduler, _output, \"list\")",
        // TIME-7: a whole set is published as hgraph's `TSS` takes it,
        // with no port of the node's output: "a set given the members it
        // holds ... is no tick".
        "def lost(pair: _hg.TSL[_hg.TS[_py.int], _hg.Size[2]]) -> _hg.TSS[_py.int]:\n\
         \x20   if _core.valid_of(pair, (\"list\", None)):\n\
         \x20       return _core.removed(pair.delta_value)",
    ]);
}

/// TYPE-16: "A delta written with a smaller `size_modified` and no
/// `removed` shrinks the list. A negative `size_modified` written
/// reduces the list by that count: `size_modified: -1` drops its last
/// position, and a count more than the list holds leaves it empty, which
/// is no failure."
#[test]
#[ignore = "needs a Python with hgraph, named by HGL_HGRAPH_PYTHON"]
fn a_negative_list_size_reduces_the_list_as_the_graph_runs() {
    let hgraph = on_hgraph("hgraph_negative_size");
    let emitted = emit_clean(&[fixture!(
        "a_negative_list_size_reduces_the_list_as_the_graph_runs",
        "sizes.hgl"
    )]);
    let tests = [emitted.test("shrinks").0, emitted.test("negative").0];
    let ran = hgraph.run(&emitted, &tests);
    assert!(matches!(ran[0], Ran::Passed), "{:?}", ran[0]);
    assert!(matches!(ran[1], Ran::Passed), "{:?}", ran[1]);
}

/// TYPE-10: "`list<T>` denotes a growing shape", TYPE-16 its delta and
/// END-3 the delta read, on hgraph 0.9.8's unbounded `TSL`.
const LIST_DELTA: (&str, &str) = fixture!("demo.list_delta.hgl");

/// A growing list is hgraph's unbounded `TSL`, `Size[-1]`, as an input,
/// an output, `out`, a struct's field and a connection. A node counts a
/// delta's size from what its `_output` holds (`list_applied`), makes an
/// empty list tick (`list_replaced`), and a source applies what it
/// yields as it is due (`list_published`). TIME-9: `valid` of a list is
/// the language's (`valid_of`), as a node's admission and a lifted
/// helper's are (FN-12). CTRL-15: a graph's loop over one is hgraph's
/// `map_` over the `TSL`. A fixed list's `removed` is written and read.
#[test]
fn a_growing_list_is_an_unbounded_tsl() {
    let emitted = emit_clean(&[LIST_DELTA]);
    let python = emitted.module("demo.list_delta");
    let list = "_hg.TSL[_hg.TS[_py.int], _hg.Size[-1]]";
    let basket = "_hg.TSB[_core.schema(Basket, {\"owner\": _hg.TS[_py.str], \"items\": _hg.TSL[_hg.TS[_py.int], _hg.Size[-1]]})]";
    python.assert_writes(&[
        format!("def edited(command: _hg.TS[_py.int], _output: _hg.TS_OUT[{list}] = None) -> {list}:"),
        "        return _core.list_replaced(_output, [1, 2, 3])\n".to_string(),
        "        return _core.list_applied(_output, _core.list_delta(size_modified=_core.neg__i64(value=1)))\n".to_string(),
        "        return _core.list_applied(_output, _core.list_delta(removed=_py.frozenset((1,))))\n".to_string(),
        // FN-12: a lifted helper is admitted by `valid` of the list.
        concat!(
            "    if not (_core.valid_of(values, (\"list\", None))):\n",
            "        return\n",
            "    return digits(values=_core.list_value(values))\n",
        )
        .to_string(),
        // END-3: the delta read is hgraph's `TSLDelta`.
        "        return _core.list_applied(_output, values.delta_value)\n".to_string(),
        "_core.valid_of(values, (\"list\", None)), values.all_valid, _core.list_position(values, 1).valid".to_string(),
        // TIME-7: `out` by position and by delta.
        "        _core.list_child(_output, 1).value = 9\n".to_string(),
        "        _output.value = _core.list_applied(_output, _core.list_delta(size_modified=_core.neg__i64(value=1)))\n".to_string(),
        // SOURCE-1: what a source yields is applied as it is due.
        "    _core.resume(_state._source, _clock.evaluation_time, _scheduler, _output, \"list\")\n".to_string(),
        // FN-3: a branch's constant is the provider's node.
        format!("        return _core.constant_of({list})(_hg.const(True), given=_core.Held((_core.copied(no_values()), \"list\")))\n"),
        // A lifted helper's empty result ticks through its `_output`.
        format!("def _lift_3_cleared(values: {list}, _output: _hg.TS_OUT[{list}] = None) -> {list}:"),
        // TYPE-9: a struct's field.
        format!("def basket(owner: _hg.TS[_py.str], command: _hg.TS[_py.int], _output: _hg.TSB_OUT[_core.schema(Basket, {{\"owner\": _hg.TS[_py.str], \"items\": {list}}})] = None) -> {basket}:"),
        "        return _core.bundled(Basket(owner=owner.value, items=[]), {\"owner\": None, \"items\": \"list\"}, _output)\n".to_string(),
        "        _output[\"items\"].value = _core.list_replaced(_output[\"items\"], [])\n".to_string(),
        "        _core.list_child(_output[\"items\"], 0).value = 8\n".to_string(),
        "    return _lift_6_digits(values=basket(owner=owner, command=command)[\"items\"])\n".to_string(),
        "_core.all_valid_of(held, (\"fields\", {\"owner\": None, \"items\": (\"list\", None)}))".to_string(),
        "        return _core.published(_core.size_modified(_core.child_delta(held[\"items\"], (\"list\", None))))\n".to_string(),
        // END-4, END-6: a list child's own `valid`, by its shape.
        "_core.child_modified(held[\"items\"], (\"list\", None))".to_string(),
        "_core.child_last_modified(held[\"items\"], (\"list\", None))".to_string(),
        // CTRL-15: a loop over the `TSL`.
        "    _core.discarded(_hg.map_(_each_0, item=edited(command=command), __key_arg__=\"\"))\n".to_string(),
        // A fixed list's `removed`, written and read.
        "        return _core.list_within(_core.list_delta(removed=_py.frozenset((1,))), 2)\n".to_string(),
        "        for position in _core.removed(pair.delta_value):\n".to_string(),
    ]);
}

/// TYPE-16, END-3 and TIME-9 as hgraph runs them: a growing list grows
/// by an entry after its last and past a gap, shrinks by its size alone
/// and by a negative one, to empty at most; a position of a growing and
/// of a fixed list made not valid; the delta read tick by tick; an empty
/// list that ticks first and again; `out` by position and two deltas in
/// one evaluation (END-5); a struct's field; a graph's loop with a child
/// for each position (CTRL-7); and a trace replayed and recorded by
/// core's `replay` and `record`, through `eval`. `the_generated_tests_pass_on_hgraph`
/// runs them too, beside the other fixtures.
#[test]
#[ignore = "needs a Python with hgraph, named by HGL_HGRAPH_PYTHON"]
fn a_growing_lists_delta_runs_on_hgraph() {
    let hgraph = on_hgraph("hgraph_list_delta");
    let emitted = emit_clean(&[LIST_DELTA]);
    let ran = hgraph.run_all(&emitted);
    let all: Vec<usize> = (0..emitted.output.tests.len()).collect();
    let described = outcomes(&emitted, &all, &ran);
    assert!(
        described.iter().all(|outcome| outcome.ends_with(" passed")),
        "{described:#?}"
    );
}

/// What fails of a growing list as the graph runs: an entry at or past a
/// size written with it, which contradicts the delta (the owner's
/// decision on hhenson/hgl_spec#124: "a failure"), with no code, since
/// none catalogued is one for it; an index of `out` past the list
/// (EXPR-3: "An index outside a list fails where it is evaluated"), which
/// hgraph would grow the list to; and, named as not written, an empty
/// list assigned to a position of `out`, which hgraph 0.9.8's `TSL` does
/// not tick for and which has no port of its own where it is written
/// (TIME-9).
#[test]
#[ignore = "needs a Python with hgraph, named by HGL_HGRAPH_PYTHON"]
fn a_list_form_not_written_fails_as_the_graph_runs() {
    let hgraph = on_hgraph("hgraph_list_unwritten");
    let emitted = emit_clean(&[fixture!(
        "a_list_form_not_written_fails_as_the_graph_runs",
        "unwritten.hgl"
    )]);
    let ran = hgraph.run_all(&emitted);
    let all: Vec<usize> = (0..emitted.output.tests.len()).collect();
    let described = outcomes(&emitted, &all, &ran);
    let expected = [
        (
            "an_entry_past_the_size_written_with_it failed unwritten.hgl:24:9",
            "HglError: the delta updates the position 4 and gives the list a size of 2",
        ),
        (
            "a_position_of_out_past_the_list failed unwritten.hgl:28:9",
            "HglError: the index 5 is outside a sequence of 3",
        ),
        (
            "an_empty_list_assigned_to_a_position_of_out failed unwritten.hgl:50:9",
            "NotImplementedError: the hgraph target does not write an empty unsized list assigned to an entry or a list's position of `out` yet",
        ),
    ];
    assert_eq!(described.len(), expected.len(), "{described:#?}");
    for (outcome, (start, what)) in described.iter().zip(expected) {
        assert!(
            outcome.starts_with(start) && outcome.contains(what),
            "{outcome}"
        );
    }
}

/// What hgraph 0.9.8 takes of a growing list in no way the language
/// has is reported: an unsized list's delta inside a list's or a
/// dictionary's delta the program makes, which hgraph applies with no
/// negative size and whose position or key the delta may add, with no
/// port of the node's output yet to count the size from (TYPE-16); and
/// an element of an unsized list of composites selected in a graph,
/// which the provider's `position_in` does not take. A struct's and a
/// tuple's delta that holds one is counted (`published_into`).
#[test]
fn a_growing_list_form_hgraph_does_not_take_is_reported() {
    let emitted = emit(&[fixture!(
        "a_growing_list_form_hgraph_does_not_take_is_reported",
        "m.hgl"
    )]);
    let nested = "the hgraph target does not write an unsized list's delta inside the delta of a list or a dictionary yet";
    assert_eq!(
        emitted.reported,
        [
            format!("m.hgl:4:51 target.unsupported: {nested}"),
            format!("m.hgl:7:56 target.unsupported: {nested}"),
            "m.hgl:12:34 target.unsupported: the hgraph target does not write an element of an unsized list of composites yet".to_string(),
        ]
    );
}

/// TIME-1, ERR-14: a position a growing list does not hold, before it
/// publishes, while it is short and after a shrink, is a child that is
/// not valid, as a dictionary's absent key is (`list_position`), and so
/// is one selected in a graph (`position_in`, EXPR-7). TYPE-16: a
/// removed position the list never held is ignored, as a dictionary's
/// absent key is (hhenson/hgl_spec#124, by analogy); a tuple's and a
/// struct's delta that holds a growing list's is counted from what the
/// list holds (`published_into`), by a node and by a source when it is
/// due, which also makes a struct's empty list tick. END-4: an
/// invalidated list is emptied first (`invalidated`).
const LIST_POSITIONS: (&str, &str) = fixture!("demo.list_positions.hgl");

/// What the target writes for them.
#[test]
fn a_growing_lists_position_is_an_endpoint_of_its_own() {
    let emitted = emit_clean(&[LIST_POSITIONS]);
    let python = emitted.module("demo.list_positions");
    python.assert_writes(&[
        "_core.list_position(values, 1).valid".to_string(),
        "_core.child_modified(_core.list_position(values, 1))".to_string(),
        "_core.checked_child_result(_core.list_position(values, index.value), lambda _port: ".to_string(),
        "        return _core.published_into(_output, (None, _core.list_delta(size_modified=_core.neg__i64(value=1)),), (None, \"list\",), entries=(\"positions\", (None, None,)))\n".to_string(),
        "_core.published_into(_output, _core.struct_delta(items=_core.list_delta(size_modified=_core.neg__i64(value=command.value))), {\"items\": \"list\"}, entries=(\"fields\", {}))".to_string(),
        "    _core.resume(_state._source, _clock.evaluation_time, _scheduler, _output, {\"items\": \"list\"}, entries=(\"fields\", {}))\n".to_string(),
        "_core.bundled(Basket(owner=\"b\", items=[]), {\"owner\": None, \"items\": \"list\"}, _core.DEFERRED)".to_string(),
        "        _core.invalidated(_output, \"list\")\n".to_string(),
        "        _core.invalidated(_output, {\"items\": \"list\"})\n".to_string(),
        "_core.position_in(edited(command=command), _hg.const(1, tp=_hg.TS[_py.int]))".to_string(),
        "_core.position_in(edited(command=command), index)".to_string(),
        // hhenson/hgl_spec#124: an `f64` list's hole reads as NaN.
        "holes(values=_core.list_value(values, _py.float(\"nan\")))".to_string(),
    ]);
}

/// They run on hgraph; `the_generated_tests_pass_on_hgraph` runs them too.
#[test]
#[ignore = "needs a Python with hgraph, named by HGL_HGRAPH_PYTHON"]
fn a_growing_lists_position_runs_on_hgraph() {
    let hgraph = on_hgraph("hgraph_list_positions");
    let emitted = emit_clean(&[LIST_POSITIONS]);
    let ran = hgraph.run_all(&emitted);
    let all: Vec<usize> = (0..emitted.output.tests.len()).collect();
    let described = outcomes(&emitted, &all, &ran);
    assert!(
        described.iter().all(|outcome| outcome.ends_with(" passed")),
        "{described:#?}"
    );
}

/// CTRL-2, CTRL-3 and CTRL-7 to CTRL-15: a graph's `for`, `while` and
/// `switch` over an ordinary value, an outputless `if`, and a `return`
/// in an `if` that gives a `let` its value.
const GRAPH_FOR: (&str, &str) = fixture!("demo.graph_for.hgl");

/// CTRL-15: "`for item in items` over an ordinary list, set, or
/// dictionary composes its body once for each element ... Over a
/// connection of `list<T, N>` it composes its body `N` times": Python's
/// own loop. "Over a connection of `list<T>`, `set<T>`, or `dict<K, V>`
/// it makes one child for each member (CTRL-7)": hgraph's `map_`, with a
/// graph for the body that takes the key's port and, CTRL-7, "Captured
/// temporal connections are bound whole and add no membership". CTRL-3:
/// "An outputless if has no synthetic result; an omitted false branch
/// performs no conditional work": `switch_` of graphs with no output.
#[test]
fn a_graph_for_is_a_python_loop_or_a_child_for_each_member() {
    let emitted = emit_clean(&[GRAPH_FOR]);
    let python = emitted.module("demo.graph_for");
    python.assert_writes(&[
        // Unrolled where the graph is built, carrying the `var`.
        concat!(
            "    running = price\n",
            "    for factor in factors:\n",
            "        running = _lift_0_mul__i64_i64(lhs=running, rhs=factor)\n",
        ),
        "    for name, weight in weights.items():\n",
        // A fixed list's `TSL` gives a port for each position.
        "    for item in both_positions(low=low):\n",
        // A dictionary: a child for each key, given the key's port.
        concat!(
            "    @_hg.graph\n",
            "    def _each_0(name: _hg.TS[_py.str], value: _hg.TS[_py.int]):\n",
            "        counted(name=name, price=value)\n",
            "    _hg.map_(_each_0, value=priced(key=key, price=price), __key_arg__=\"name\")\n",
        ),
        // A list: its unbounded `TSL`, and no key. Each child gives a
        // constant, which `discarded` keeps in the graph.
        "    def _each_1(item: _hg.TS[_py.int]) -> _hg.TS[_py.bool]:\n",
        "    _core.discarded(_hg.map_(_each_1, item=readings(reading=reading), __key_arg__=\"\"))\n",
        // A set: its members are the keys.
        "    _hg.map_(_each_2, __keys__=members(member=member), __key_arg__=\"each\")\n",
        // A connection from outside is given whole; one named as a
        // parameter of `switch_` is taken under a name of its own.
        concat!(
            "        def _case_0_0(_taken_name: _hg.TS[_py.str], value: _hg.TS[_py.int]):\n",
            "            name = _taken_name\n",
            "            counted(name=name, price=value)\n",
        ),
        "        _hg.switch_(open, {True: _case_0_0, False: _case_0_1}, _taken_name=name, value=value)\n",
        "    _hg.map_(_each_3, value=priced(key=key, price=price), __key_arg__=\"name\", open=_hg.pass_through(open))\n",
        // A `while` and a `switch` over an ordinary value.
        "    while _core.gt__T_T(lhs=left, rhs=0):\n",
        concat!(
            "    _selected_1 = route\n",
            "    if _selected_1 == 1:\n",
            "        return price\n",
            "    elif _selected_1 == 2:\n",
        ),
        // An outputless `if`: no `else` is a graph with nothing in it.
        concat!(
            "    def _case_3_1(price: _hg.TS[_py.int]):\n",
            "        pass\n",
            "    _hg.switch_(open, {True: _case_3_0, False: _case_3_1}, price=price)\n",
        ),
        // A `return` in a `let`'s `if`: the other branch's graph gives
        // the `let` its value and has what follows.
        concat!(
            "    def _case_4_1(left: _hg.TS[_py.int], right: _hg.TS[_py.int]) -> _hg.TS[_py.int]:\n",
            "        taken = _lift_9_mul__i64_i64(lhs=right, rhs=2)\n",
            "        return _lift_10_add__i64_i64(lhs=taken, rhs=1)\n",
            "    return _hg.switch_(flag, {True: _case_4_0, False: _case_4_1}, left=left, right=right)\n",
        ),
    ]);
}

/// What this target does not write of a graph's loops is reported: a
/// key read where hgraph has no value for it, as an operand and as a
/// `const` argument (`map_` gives a child its key as a port); a
/// `return` under a branch selected at runtime in an unrolled loop,
/// whose other path is the passes after it; and a `return` in an `if`
/// over a connection that gives a `let` its value where a branch has
/// none. A loop over a growing list of nullables is written: each
/// child is given its position's port of hgraph's unbounded `TSL`.
#[test]
fn a_graph_loop_form_not_written_yet_is_reported() {
    let emitted = emit(&[fixture!(
        "a_graph_loop_form_not_written_yet_is_reported",
        "m.hgl"
    )]);
    assert_eq!(
        emitted.reported,
        [
            "m.hgl:6:12 target.unsupported: the hgraph target does not write a loop's key or member read other than as the argument of a temporal parameter yet",
            "m.hgl:11:25 target.unsupported: the hgraph target does not write a loop's key or member read other than as the argument of a temporal parameter yet",
            "m.hgl:16:9 target.unsupported: the hgraph target does not write a `return` under a branch selected at runtime in a loop unrolled where the graph is built yet",
            "m.hgl:25:23 target.unsupported: the hgraph target does not write a `return` in an `if` over a connection that gives a `let` or a `var` its value, with a branch that has no value yet",
        ]
    );
}

/// TYPE-9: "Temporalization recursively maps tuples/structs to child
/// fields". TYPE-10: "A tuple retains positional identity". TYPE-16: a
/// tuple's delta is "`tuple<delta<A>?, delta<B>?>`". TYPE-3: a generic
/// struct's application, and an abstract struct with the structs below
/// it. TIME-7: `out` assigned by part.
const TUPLES: (&str, &str) = fixture!("demo.tuples.hgl");

/// A tuple is hgraph's bundle with no name, a field for each position;
/// a generic struct's bundle has the field types its arguments give; an
/// abstract struct's has its own fields, and a connection of a struct
/// below it is narrowed to them; a whole struct sets a struct field by
/// its fields and a set field whole; a part of `out` is its own port.
#[test]
fn a_temporal_tuple_and_struct_are_written_by_their_parts() {
    let emitted = emit_clean(&[TUPLES]);
    let python = emitted.module("demo.tuples");
    let pair = "_hg.TSB[\"_0\": _hg.TS[_py.int], \"_1\": _hg.TS[_py.str]]";
    let shape = "_hg.TSB[_core.schema(Shape, {\"id\": _hg.TS[_py.int]})]";
    python.assert_writes(&[
        // TYPE-9, TYPE-10, TYPE-16: a tuple.
        format!("-> {pair}:\n    return _core.bundled((a.value, b.value,), (None, None,))"),
        "    only_first = (value.value, None,)".to_string(),
        format!("    return {}", part("pair[\"_0\"]")),
        format!(
            "    held = ({}, {},)",
            part("pair[\"_0\"]"),
            part("pair[\"_1\"]")
        ),
        "    first = _core.item(_core.positions(pair.delta_value, (None, None,)), 0)"
            .to_string(),
        "_core.positions(parts.delta_value, (None, (None, None,),))".to_string(),
        "    return _core.bundled((None, \"none\",), (None, None,))".to_string(),
        "    return paired(a=a, b=b)[\"_0\"]".to_string(),
        "    return nested(bid=bid, size=size)[\"_0\"][\"bid\"]".to_string(),
        "    return paired(a=a, b=b)[_core.position(second_slot())]".to_string(),
        "        _output[_core.position(_core.sub__i64_i64(lhs=1, rhs=1))].value = value.value"
            .to_string(),
        format!("    return {pair}.from_ts(_0=a, _1=b)"),
        format!("_lift_2_total(pair={pair}.from_ts(_0=a, _1=\"y\"))"),
        // TYPE-3, TYPE-4: a generic struct.
        concat!(
            "-> _hg.TSB[_core.schema(_core.applied(Box, \"i64\"), ",
            "{\"value\": _hg.TS[_py.int]})]:"
        )
        .to_string(),
        format!(
            "    whole = _core.applied(Box, \"i64\")(value={})",
            part("held[\"value\"]")
        ),
        "{\"value\": {\"bid\": None, \"ask\": None}})".to_string(),
        "-> _hg.TS[_core.applied(Box, \"i64\")]:".to_string(),
        // TYPE-3: an abstract struct and the structs below it.
        format!("def id_of(shape: {shape}) -> _hg.TS[_py.int]:"),
        format!("    return _core.narrowed(circle(id=id, radius=radius), {shape}, (\"id\",))"),
        format!("    shape = _core.narrowed(circle(id=id, radius=radius), {shape}, (\"id\",))"),
        format!(
            "from_ts(_0=_core.narrowed(circle(id=id, radius=radius), {shape}, (\"id\",)), _1=radius)"
        ),
        "    return _core.bundled(Square(id=id.value, side=1), {\"id\": None})".to_string(),
        // TIME-7: a whole struct with a struct field and a set field.
        "{\"quote\": {\"bid\": None, \"ask\": None}, \"size\": None})".to_string(),
        // Its set is given as its members (TIME-7).
        "    return _core.bundled(Tagged(tags=_py.frozenset((tag.value,)), count=1), {\"tags\": \"set\", \"count\": None})\n"
            .to_string(),
        // TIME-7: `out` assigned by part, and read back.
        format!(
            "    _output[\"ask\"].value = _core.add__i64_i64(lhs={}, rhs=1)",
            part("_output[\"bid\"]")
        ),
        "        _output[\"quote\"].value = _core.bundled(Quote(bid=bid.value, ask=bid.value))"
            .to_string(),
        "        _output[\"quote\"][\"ask\"].value = ".to_string(),
        format!(
            "    _output[\"_0\"].value = _core.mul__i64_i64(lhs={}, rhs=2)",
            part("_output[\"_1\"]")
        ),
        "        _output[\"price\"].value = _core.published(None)".to_string(),
        "_output: _hg.TS_OUT[_hg.TSL[_hg.TS[_py.int], _hg.Size[2]]] = None".to_string(),
        "    _output[1].value = ".to_string(),
        "    _output.value = {key.value: value.value}".to_string(),
        format!(
            "    _output.value = _core.with_field({}, ask=",
            out_read("_output.value", "_output.valid")
        ),
        format!(
            "    whole = {}",
            out_read(
                &format!(
                    "Quote(bid={}, ask={})",
                    part("_output[\"bid\"]"),
                    part("_output[\"ask\"]")
                ),
                "_output.valid"
            )
        ),
    ]);
}

/// TST-5: "Each cell is one cycle's publication, written as a `return`
/// is (TIME-7) ... a T, published whole, or a `delta<T>` (TYPE-16) ...
/// A list, a tuple, a struct and a dictionary are each written either
/// way". A tuple's cell written whole and its delta.
const TUPLE_CELLS: (&str, &str) = fixture!("demo.tuple_cells.hgl");

/// A tuple's cell written whole is marked so, with how each position is
/// published (`bundled`): an input publishes it whole, a null position
/// as a published null (TST-9: "a T whole, which replaces what the
/// input held"), and an expectation's is compared with the value the
/// result holds, read from its port (`_hold`). A cell that is no T is
/// the tuple's delta, as it was.
#[test]
fn a_tuple_cell_written_whole_is_published_whole() {
    let emitted = emit_clean(&[TUPLE_CELLS]);
    let python = emitted.module("demo.tuple_cells");
    let whole = |cell: &str| format!("_harness.whole({cell}, shape=(None, None,))");
    python.assert_writes(&[
        format!("pair=[{}, {}]", whole("(1, 5,)"), whole("(2, None,)")),
        format!("pair=[{}, (None, 3,)]", whole("(1, 2,)")),
        "_hold=lambda _port: ".to_string(),
        format!("[{}, {}]", whole("(1, 2,)"), whole("(7, 2,)")),
        format!("[{}, (7, None,)]", whole("(1, 2,)")),
    ]);
}

/// TIME-9: core's `valid` is the selector however it is named, by its
/// name or through an alias of core, and the guard in a body too.
const ALIASED_SELECTOR: (&str, &str) = fixture!("demo.aliased_selector.hgl");

/// `c::valid(a)` is written as `valid(a)` is: the node requires `a`
/// valid, and `c::valid(b)` in a body is `b.valid`, as the analyser's
/// record of the header (`hir::Header`) has it.
#[test]
fn a_selector_named_through_an_alias_is_written_as_the_selector() {
    let emitted = emit_clean(&[ALIASED_SELECTOR]);
    let python = emitted.module("demo.aliased_selector");
    python.assert_writes(&[
        "@_hg.compute_node(valid=(\"a\",))\ndef qualified(".to_string(),
        "    if b.valid:\n".to_string(),
        "@_hg.compute_node(valid=(\"a\",))\ndef written_with_its_type(".to_string(),
    ]);
}

/// What this target does not write of a temporal tuple or struct is
/// reported: an abstract struct read whole, in a hook and by a lifted
/// helper, or as a node's configuration; the delta of a list of tuples;
/// a place inside an entry of a dictionary `out`; and a composite value
/// in a tuple of connections.
#[test]
fn a_temporal_tuple_or_struct_form_not_written_yet_is_reported() {
    let emitted = emit(&[fixture!(
        "a_temporal_tuple_or_struct_form_not_written_yet_is_reported",
        "m.hgl"
    )]);
    assert_eq!(
        emitted.reported,
        [
            "m.hgl:7:20 target.unsupported: the hgraph target does not write a whole value of the temporal `Shape` yet",
            "m.hgl:15:33 target.unsupported: the hgraph target does not write a whole value of the temporal `Shape` yet",
            "m.hgl:16:21 target.unsupported: the hgraph target does not write the type `Shape` yet",
            "m.hgl:21:22 target.unsupported: the hgraph target does not write the delta of a list or a dictionary that holds a tuple yet",
            "m.hgl:27:12 target.unsupported: the hgraph target does not write an assignment inside an entry of `out` yet",
            "m.hgl:32:37 target.unsupported: the hgraph target does not write a composite value in a tuple of connections yet",
        ]
    );
}

/// FN-16: "A node ... may mark a temporal parameter `ref`, `ref low:
/// f64`, and its result, `-> ref f64`. The mark is a mode ... A
/// connection of T is given to a `ref` parameter, and a `ref` result is a
/// connection of T to whoever reads it." TIME-10: a reader of a `ref`
/// result "observes the endpoint designated as if wired to it", and "a
/// `ref` parameter is passive to what it designates". TYPE-12: "`signal`
/// is a complete temporal input constraint with no readable payload".
/// TYPE-9: "`atomic<V>` stops expansion at a complete ordinary V".
const REFERENCES: (&str, &str) = fixture!("demo.references.hgl");

/// A `ref` parameter and a `ref` result are hgraph's `REF` of their type,
/// and a `signal` its `SIGNAL`. A node returns a `ref` parameter's value,
/// the reference it holds, sampled (FN-16), to re-point what follows it:
/// the provider's `Designation` keeps what it designates. An input that is no `ref`
/// parameter, is designated through a hidden `REF` input that a graph of
/// the function's name gives it the input twice, and a fixed list's
/// position through one for each position, which the graph gives each
/// position: each is active, and the node returns before its handlers
/// where no input of its own woke it. A graph passes a `ref` result on as
/// it is, and hgraph
/// binds an ordinary input, a lifted node's among them, to what it
/// designates. A generic input is written for the connection's type. A
/// `signal?` left out is bound to an output that never ticks (UNIT-8), by
/// a call and a captured one alike. A hook reads an atomic input as its
/// whole value.
#[test]
fn a_ref_is_a_ref_and_a_signal_a_signal() {
    let emitted = emit_clean(&[REFERENCES]);
    let python = emitted.module("demo.references");
    let reference = "_hg.REF[_hg.TS[_py.int]]";
    python.assert_writes(&[
        format!(
            "def pick(use_left: _hg.TS[_py.bool], left: {reference}, right: {reference}, \
             _output: _hg.TS_OUT[{reference}] = None, _state: _hg.STATE = None) -> {reference}:"
        ),
        "    _designation = _core.Designation(_output, _state, {})\n".to_string(),
        "        return _designation.sampled(left)\n".to_string(),
        "    if (target.valid and target.modified):\n        return _designation.sampled(target)\n"
            .to_string(),
        format!("def checked(target: {reference}) -> _hg.TS[_py.bool]:\n    if target.modified:\n        return target.valid\n"),
        format!(
            "@_hg.compute_node(active=(\"idx\", \"_ref0_items\", \"_ref1_items\", \"_ref2_items\"), valid=())\n\
             def _node_route(idx: _hg.TS[_py.int], items: _hg.TSL[_hg.TS[_py.int], _hg.Size[3]], \
             _ref0_items: {reference}, _ref1_items: {reference}, _ref2_items: {reference}, "
        ),
        "    _designation = _core.Designation(_output, _state, \
         {\"_ref_items\": (_ref0_items, _ref1_items, _ref2_items,)})\n\
         \x20   if not (idx.modified or idx.invalidated):\n\
         \x20       return _designation.published()\n"
            .to_string(),
        "        return _designation.returned(\"_ref_items\", idx.value)\n".to_string(),
        format!(
            "def route(idx: _hg.TS[_py.int], items: _hg.TSL[_hg.TS[_py.int], _hg.Size[3]]) \
             -> {reference}:\n    return _node_route(idx=idx, items=items, _ref0_items=items[0], \
             _ref1_items=items[1], _ref2_items=items[2])\n"
        ),
        "        _designation.designate(\"_ref_items\", idx.value)\n    return _designation.published()\n"
            .to_string(),
        "        return _designation.returned(\"_ref_value\")\n".to_string(),
        "    return _node_hold(flag=flag, value=value, _ref_value=value)\n".to_string(),
        "def counted(tick: _hg.SIGNAL, _state: _hg.STATE = None) -> _hg.TS[_py.int]:"
            .to_string(),
        "def _lift_0_doubled(value: _hg.TS[_py.int]) -> _hg.TS[_py.int]:".to_string(),
        "    return _lift_0_doubled(value=pick(use_left=use_left, left=left, right=right))"
            .to_string(),
        "    return _inst_ticks__i64(value=pick(use_left=use_left, left=left, right=right))"
            .to_string(),
        format!("def _inst_ref_ticks__i64(value: {reference}, "),
        "    return _inst_ref_ticks__i64(value=_inst_same__i64(value=left))".to_string(),
        "    return maybe_sampled(value=value, tick=_hg.nothing(tp=_hg.TS[_py.bool]))"
            .to_string(),
        // A captured call binds what it leaves out as a call does.
        "        return maybe_sampled(value=value, tick=_hg.nothing(tp=_hg.TS[_py.bool]))"
            .to_string(),
        "    return width(quote=quote.value)".to_string(),
        "    held = quote.value\n".to_string(),
        "    return quote.delta_value.ask".to_string(),
        format!(
            "    whole = {}\n",
            out_read("_output.value", "_output.valid")
        ),
        "def bid_or(value: _hg.TS[_py.int], quote: _hg.TS[Quote]) -> _hg.TS[_py.int]:"
            .to_string(),
        // An atomic input's value is hgraph's: a `var` takes a copy.
        "    saved = _core.copied(triple.value)\n".to_string(),
        "        whole = quote.value\n".to_string(),
        "rhs=quote.value.ask), rhs=quote.value.ask)".to_string(),
    ]);
}

/// TIME-10: "Ordinary followers observe the selected target". A lifted
/// helper given a `ref` result takes an ordinary input of its type
/// (FN-16), which hgraph binds to what the result designates.
#[test]
fn a_lifted_input_follows_the_ref_result_it_is_given() {
    let emitted = emit_clean(&[fixture!(
        "a_lifted_input_follows_the_ref_result_it_is_given",
        "m.hgl"
    )]);
    let python = emitted.module("m");
    assert!(!python.contains("value: _hg.REF"), "{python}");
    python.assert_writes(&[
        "def _lift_0_doubled(value: _hg.TS[_py.int]) -> _hg.TS[_py.int]:",
        "def pick(flag: _hg.TS[_py.bool], low: _hg.REF[_hg.TS[_py.int]], high: _hg.REF[_hg.TS[_py.int]], ",
        "    return _lift_0_doubled(value=pick(flag=flag, low=low, high=high))",
    ]);
}

/// FN-16: what this target does not write of a `ref` parameter or result
/// is reported: a child of one as an endpoint, `all_valid` of one, and a
/// designation of a child that is no position of a fixed list.
#[test]
fn what_a_ref_cannot_be_given_yet_is_reported() {
    let emitted = emit(&[fixture!(
        "what_a_ref_cannot_be_given_yet_is_reported",
        "m.hgl"
    )]);
    assert_eq!(
        emitted.reported,
        [
            "m.hgl:4:12 target.unsupported: the hgraph target does not write an endpoint of a child of a `ref` parameter or result yet",
            "m.hgl:5:16 target.unsupported: the hgraph target does not write `all_valid` of a `ref` parameter or result yet",
            "m.hgl:9:33 target.unsupported: the hgraph target does not write a designation of a child that is no position of a fixed list yet",
            "m.hgl:12:19 target.unsupported: the hgraph target does not write a child of a `ref` parameter designated yet",
        ]
    );
}

/// FN-16: "Returning a whole `ref` parameter samples its current
/// designation when the `return` executes ... Returning an ordinary input
/// or a child path designates the logical endpoint named by the path.
/// Readers follow a later rebinding of that endpoint without waking the
/// node for it."
const REFS_WAVE3: (&str, &str) = fixture!("demo.refs_wave3.hgl");

/// A `ref` parameter returned whole is sampled from its own `REF`
/// (`Designation.sampled`), and the node is given no other input for it:
/// it is written as any node is. An input that is no `ref` parameter is
/// given to the node a second time, as a hidden `REF` input that is
/// active: the provider's `Designation` follows a re-binding of the one
/// the result designates, and the node returns before its handlers where
/// no active input of its own woke it. No validity is that node's, so a
/// re-binding to an endpoint that is not valid is followed too: each
/// handler asks its own. What an evaluation writes is published as it
/// ends, an invalidation among it. A constant given to a `ref` parameter
/// is a constant connection, and one left out designates nothing.
#[test]
fn a_ref_parameter_is_sampled_and_an_ordinary_input_followed() {
    let emitted = emit_clean(&[REFS_WAVE3]);
    let python = emitted.module("demo.refs_wave3");
    let reference = "_hg.REF[_hg.TS[_py.int]]";
    python.assert_writes(&[
        format!(
            "@_hg.compute_node(active=(\"once\",))\n\
             def pass_on(once: _hg.TS[_py.bool], target: {reference}, "
        ),
        "    _designation = _core.Designation(_output, _state, {})\n\
         \x20   if once.modified:\n\
         \x20       return _designation.sampled(target)\n\
         \x20   return _designation.published()\n"
            .to_string(),
        "    _designation = _core.Designation(_output, _state, {\"_ref_value\": _ref_value})\n\
         \x20   if not (once.modified or once.invalidated):\n\
         \x20       return _designation.published()\n\
         \x20   if once.valid and value.valid and once.modified:\n\
         \x20       return _designation.returned(\"_ref_value\")\n\
         \x20   return _designation.published()\n"
            .to_string(),
        "    return _node_hold(once=once, value=value, _ref_value=value)\n".to_string(),
        // An input whose activity the body changes is asked whether it is
        // active.
        "    if not (((once.modified or once.invalidated) and once.active) or \
         ((data.modified or data.invalidated) and data.active) or "
            .to_string(),
        // The writes of an evaluation are kept, and the node returns the
        // last as it ends.
        "        _designation.sample(a)\n\
         \x20       _designation.sample(b)\n\
         \x20   return _designation.published()\n"
            .to_string(),
        // END-4: `invalidate(out)` of a `ref` result leaves it designating
        // nothing, which the node publishes as it ends.
        "        _designation.invalidate()\n    return _designation.published()\n".to_string(),
        // FN-3: an ordinary value given to a `ref` parameter is wired as a
        // constant connection of its type, which it designates.
        "    return designations(target=_hg.const(5))\n".to_string(),
        "    return nullable_designations(target=_hg.const(_core.published(None), _hg.TS[_py.int | None]))\n"
            .to_string(),
        // UNIT-8: "an omitted temporal input is never valid": a `ref`
        // parameter left out is bound to a reference that never ticks.
        "    return optional_valid(probe=probe, x=_hg.nothing(tp=_hg.REF[_hg.TS[_py.int | None]]))\n"
            .to_string(),
    ]);
}

/// Two zoned datetimes compared (TYPE-2, OPS-2), a duration moving a
/// zoned instant (OPS-10), the order of the temporal scalars (OPS-2) and
/// a string times a count (OPS-1). Its tests run on hgraph with every
/// fixture's (`the_generated_tests_pass_on_hgraph`).
const SCALARS_WAVE3: (&str, &str) = fixture!("demo.scalars_wave3.hgl");

/// TYPE-2: "Two `zoned_datetime` values are equal when both their
/// instants and their zone names are ... Compared as UTC, they are equal
/// when their instants are. That comparison is written `a =={utc: true}
/// b`". OPS-2: "The option is `utc`, a `bool` that is false unless given
/// ... declared by the candidates of `eq_` and `ne_` for two zoned
/// datetimes." Each is the provider's function for the candidate, given
/// `utc` by name: the default where it is left out. EXPR-13: "left
/// operand, configuration entries in their written order, then right
/// operand", which Python evaluates the arguments in. In a graph the
/// option is configuration of the lifted node (EXPR-14), and an
/// argument of a call by name given a connection an input that takes
/// the default while it is invalid (FN-11).
#[test]
fn two_zoned_datetimes_are_compared_by_cores_candidates() {
    let emitted = emit_clean(&[SCALARS_WAVE3]);
    let python = emitted.module("demo.scalars_wave3");
    python.assert_writes(&[
        "def same(a, b):\n    return _core.eq__zoned_datetime_zoned_datetime_bool(lhs=a, rhs=b, utc=False)",
        "def apart(a, b):\n    return _core.ne__zoned_datetime_zoned_datetime_bool(lhs=a, rhs=b, utc=False)",
        "def same_instant(a, b):\n    return _core.eq__zoned_datetime_zoned_datetime_bool(lhs=a, utc=True, rhs=b)",
        "def same_as(a, b, as_utc):\n    return _core.eq__zoned_datetime_zoned_datetime_bool(lhs=a, utc=as_utc, rhs=b)",
        "def _inst_same_by__true(a, b):\n    return _core.eq__zoned_datetime_zoned_datetime_bool(lhs=a, utc=True, rhs=b)",
        "def named(a, b, as_utc):\n    return _core.eq__zoned_datetime_zoned_datetime_bool(lhs=a, rhs=b, utc=as_utc)",
        "return _core.ne__zoned_datetime_zoned_datetime_bool(lhs=a.value, utc=flag.value, rhs=b.value)",
        "rhs: _hg.TS[_core.ZonedDatetime], utc: _py.bool) -> _hg.TS[_py.bool]:\n    return _core.eq__zoned_datetime_zoned_datetime_bool(lhs=lhs.value, rhs=rhs.value, utc=utc)",
        "def wired_as_utc(a: _hg.TS[_core.ZonedDatetime], b: _hg.TS[_core.ZonedDatetime]) -> _hg.TS[_py.bool]:",
        "utc=(utc.value if utc.valid else False)",
        "return _core.mul__str_i64(lhs=text, rhs=count)",
    ]);
}

/// OPS-1: "`*` | `str` and `i64`, in either order | `str`", core's
/// string repetition. The pinned contract says nothing of a count below
/// zero, so the provider does not write one: it fails, with no HGL
/// error, where it is met, in a value function and in a lifted node.
/// Needs hgraph, as above.
#[test]
#[ignore = "needs a Python with hgraph, named by HGL_HGRAPH_PYTHON"]
fn a_string_times_a_count_below_zero_is_not_written() {
    let hgraph = on_hgraph("hgraph_negative_repetition");
    let emitted = emit_clean(&[fixture!(
        "a_string_times_a_count_below_zero_is_not_written",
        "m.hgl"
    )]);
    let ran = hgraph.run_all(&emitted);
    let all: Vec<usize> = (0..emitted.output.tests.len()).collect();
    let described = outcomes(&emitted, &all, &ran);
    let expected = [
        "below_zero_in_a_value_function failed m.hgl:6:5",
        "below_zero_in_a_graph failed m.hgl:10:5",
    ];
    assert_eq!(described.len(), expected.len(), "{described:#?}");
    for (outcome, start) in described.iter().zip(expected) {
        assert!(
            outcome.starts_with(start)
                && outcome.contains(
                    "NotImplementedError: the hgraph target does not write a string times a count below zero yet"
                ),
            "{outcome}"
        );
    }
}

/// A field that may be left out because its type is no ordinary value,
/// an atomic's (TYPE-14), is reported.
#[test]
fn an_atomic_field_that_may_be_left_out_is_not_written_yet() {
    let emitted = emit(&[fixture!(
        "an_atomic_field_that_may_be_left_out_is_not_written_yet",
        "m.hgl"
    )]);
    assert_eq!(
        emitted.reported,
        ["m.hgl:6:10 target.unsupported: the hgraph target does not write the type `Maybe` yet"]
    );
}

/// How a hook reads a required part of a temporal struct or tuple, the
/// port `port`: its value where it is valid, and `value.unset_read`
/// where it is not.
fn part(port: &str) -> String {
    format!("({port}.value if {port}.valid else _core.unset_read())")
}

/// How a hook reads the whole of its `out`, `read` where `out` is valid
/// by `valid`, and `value.unset_read` where it is not (TIME-7).
fn out_read(read: &str, valid: &str) -> String {
    format!("({read} if {valid} else _core.unset_read(\"`out` is read before it holds a value\"))")
}

/// TYPE-9 and TIME-9: a part of a struct or a tuple that has not
/// published, read alone or whole; an ordinary composite as a graph
/// branch's constant (FN-3); and a struct with no field.
const STRUCT_REVIEW: (&str, &str) = fixture!("demo.struct_review.hgl");

/// A part read in a hook is its port's value where it is valid: a
/// required one fails with `value.unset_read` where it is not (an
/// interim reading of hhenson/hgl_spec#126), and an optional one is
/// null. A composite branch's constant is told its time-series type. A
/// struct with no field is a `TS` of its class, whose publication is
/// its one value.
#[test]
fn a_part_not_published_and_a_composite_constant_are_written() {
    let emitted = emit_clean(&[STRUCT_REVIEW]);
    let python = emitted.module("demo.struct_review");
    python.assert_writes(&[
        "    return _core.add__i64_i64(lhs=(quote[\"ask\"].value if quote[\"ask\"].valid else _core.unset_read()), rhs=1)",
        "    price = (level[\"price\"].value if level[\"price\"].valid else None)",
        "    held = ((pair[\"_0\"].value if pair[\"_0\"].valid else _core.unset_read()), (pair[\"_1\"].value if pair[\"_1\"].valid else _core.unset_read()),)",
        "_hg.const(_core.bundled(fixed_pair(), (None, None,)), tp=_hg.TSB[\"_0\": _hg.TS[_py.int], \"_1\": _hg.TS[_py.str]])",
        "_hg.const(_py.tuple(fixed_list()), tp=_hg.TSL[_hg.TS[_py.int], _hg.Size[2]])",
        "_hg.const(fixed_tags(), tp=_hg.TSS[_py.int])",
        "def emptied(a: _hg.TS[_py.int]) -> _hg.TS[Empty]:\n    return _core.only_value(Empty(), Empty)",
        "    _output.value = _core.only_value(_core.struct_delta(), Empty)",
        "    return _py.tuple([e.valid, e.all_valid, (e.modified and e.valid)])",
    ]);
}

// ---- Analyser value rules ----

/// Core's OPS-6: "`convert<list<B>>(values)` is admitted too, where
/// `values` is a `list<A>` and the struct `A` inherits from the abstract
/// struct `B`". The analyser admits it; the target does not write it
/// yet, and reports it where it is written.
#[test]
fn a_list_of_structs_converted_to_their_parent_is_reported() {
    let emitted = emit(&[fixture!(
        "a_list_of_structs_converted_to_their_parent_is_reported",
        "m.hgl"
    )]);
    assert_eq!(
        emitted.reported,
        [
            "m.hgl:4:64 target.unsupported: the hgraph target does not write a list of structs converted to a list of their parent yet"
        ]
    );
}

/// CTRL-6, CTRL-13, CTRL-14 and TYPE-14: a label that is a constant, a
/// `T?` result of a graph's `if`, an ordinary null a graph returns, and
/// a `var` read in a returned branch.
const CONTROL_REVIEW: (&str, &str) = fixture!("demo.control_review.hgl");

/// TYPE-14: "a T is a value where a T? is expected": a branch's graph
/// gives a connection of `T` as the provider's `nullable` of it, and an
/// ordinary value, a null among them, as a constant of `T?`. CTRL-6:
/// with no `default`, `switch_` is given a graph that fails as a node's
/// `switch` does.
#[test]
fn a_nullable_branch_and_an_unmatched_selector_are_written() {
    let emitted = emit_clean(&[
        CONTROL_REVIEW,
        fixture!(
            "a_nullable_branch_and_an_unmatched_selector_are_written",
            "m.hgl"
        ),
    ]);
    let python = emitted.module("demo.control_review");
    python.assert_writes(&[
        "        return _core.nullable(price, _py.int)\n",
        "        return _hg.const(_core.published(None), _hg.TS[_py.int | None])\n",
        "    return _hg.const(_core.published(t), _hg.TS[_py.int | None])\n",
    ]);
    let routed = emitted.module("m");
    routed.assert_writes(&[
        "    _core.unmatched(key.value)\n",
        "def _case_0_1(key: _hg.TS[_py.int], a: _hg.TS[_py.int]) -> _hg.TS[_py.int]:\n",
        "    return _hg.switch_(route, {1: _case_0_0, _hg.DEFAULT: _case_0_1}, a=a)\n",
    ]);
}

/// CTRL-14: "After the `if` the `var` is one connection: the branch's
/// binding while that branch is selected". An `if` over a connection
/// that gives a `let` or a `var` its value gives that one connection,
/// and a `var` a branch assigns is reported, there and one level down.
/// CTRL-6: "Duplicate values reject": an instance whose generic label
/// has another label's value is reported.
#[test]
fn a_control_form_that_would_be_wrong_is_reported() {
    let emitted = emit(&[fixture!(
        "a_control_form_that_would_be_wrong_is_reported",
        "m.hgl"
    )]);
    assert_eq!(
        emitted.reported,
        [
            "m.hgl:4:17 target.unsupported: the hgraph target does not write an `if` over a connection that gives a `let` or a `var` its value and assigns a `var` declared before it yet",
            "m.hgl:14:17 target.unsupported: the hgraph target does not write an `if` over a connection that gives a `let` or a `var` its value and assigns a `var` declared before it yet",
            "m.hgl:21:21 target.unsupported: the hgraph target does not write an `if` over a connection that gives a `let` or a `var` its value and assigns a `var` declared before it yet",
            "m.hgl:29:14 target.unsupported: the hgraph target does not write a `switch` two of whose labels have one value in an instance yet",
        ]
    );
}

/// CTRL-6: "Switch labels are typed source constants, compared by
/// resolved value ... Duplicate values reject." A `switch` over a
/// connection is hgraph's `switch_`, whose branches are keyed by each
/// label's value: of two labels of one value it keeps the last, where a
/// `switch` over an ordinary value takes the first. So a label whose
/// value the analyser has not worked out, a constant a `const fn`
/// computes or a negation by a candidate of the program's own, is
/// reported beside another label: it may be another's value. A label
/// alone, and labels whose values are worked out, are written.
const UNVALUED_LABELS: (&str, &str) =
    fixture!("a_switch_label_not_worked_out_is_reported", "m.hgl");

#[test]
fn a_switch_label_not_worked_out_is_reported() {
    let emitted = emit(&[UNVALUED_LABELS]);
    let unvalued = "the hgraph target does not write a `switch` over a connection with a label whose value the compiler does not work out yet";
    assert_eq!(
        emitted.reported,
        [
            format!("m.hgl:10:14 target.unsupported: {unvalued}"),
            format!("m.hgl:17:14 target.unsupported: {unvalued}"),
            format!("m.hgl:39:14 target.unsupported: {unvalued}"),
        ]
    );
    assert_eq!(
        emitted.blocked("graph_path_int"),
        Some(format!("m.hgl:10:14 {unvalued}"))
    );
    assert_eq!(
        emitted.blocked("graph_path_enum"),
        Some(format!("m.hgl:17:14 {unvalued}"))
    );
    assert_eq!(emitted.blocked("graph_path_valued"), None);
}

/// The `switch`es of [`UNVALUED_LABELS`] that are written select the
/// case of the selector's value. Needs hgraph, as above.
#[test]
#[ignore = "needs a Python with hgraph, named by HGL_HGRAPH_PYTHON"]
fn a_switch_whose_labels_are_worked_out_selects_by_value() {
    let hgraph = on_hgraph("hgraph_unvalued_labels");
    let emitted = emit(&[UNVALUED_LABELS]);
    let tests = [emitted.test("graph_path_valued").0];
    let ran = hgraph.run(&emitted, &tests);
    assert_eq!(
        outcomes(&emitted, &tests, &ran),
        ["graph_path_valued passed"]
    );
}

/// EXPR-15: "In a graph, an assignment into a place of a `var` is copy
/// on write ... `point.x = price` rebinds `point` to a new connection".
/// A struct's, a tuple's and a fixed list's connection is made anew of
/// its parts (`demo.graph_places`). One whose parts hgraph has as one
/// value or knows only as the graph runs is reported, each with its own
/// reason: a field of an atomic struct, an entry of a dictionary and a
/// position of an unsized list.
#[test]
fn a_place_of_a_connection_not_written_yet_is_reported() {
    let emitted = emit(&[fixture!(
        "a_place_of_a_connection_not_written_yet_is_reported",
        "m.hgl"
    )]);
    let unsupported = "target.unsupported: the hgraph target does not write";
    assert_eq!(
        emitted.reported,
        [
            format!(
                "m.hgl:11:5 {unsupported} a field of an atomic struct's connection assigned in a graph yet"
            ),
            format!(
                "m.hgl:19:5 {unsupported} an entry of a dictionary's connection assigned in a graph yet"
            ),
            format!(
                "m.hgl:27:5 {unsupported} a position of an unsized list's connection assigned in a graph yet"
            ),
        ]
    );
}

/// CTRL-6: "An unmatched selector uses default or fails": a `switch` of
/// a graph over a connection fails as one in a node's hook does, with
/// the provider's error. Needs hgraph, as above.
#[test]
#[ignore = "needs a Python with hgraph, named by HGL_HGRAPH_PYTHON"]
fn an_unmatched_selector_fails_alike_on_each_path() {
    let hgraph = on_hgraph("hgraph_unmatched");
    let emitted = emit_clean(&[fixture!(
        "an_unmatched_selector_fails_alike_on_each_path",
        "u.hgl"
    )]);
    let ran = hgraph.run_all(&emitted);
    let messages: Vec<String> = ran
        .iter()
        .map(|ran| match ran {
            Ran::Passed => "passed".to_string(),
            Ran::Failed { message, .. } => message.clone(),
            Ran::NotRun(reason) => format!("not run {reason}"),
        })
        .collect();
    let expected = "HglError: no case of the `switch` has the value 3";
    for message in &messages[..4] {
        assert!(message.contains(expected), "{messages:?}");
    }
    assert_eq!(messages[4], "passed");
}

/// A collection's child (TIME-1, TIME-4, TIME-9, END-8, ERR-14), trace
/// cells written whole or as a delta (TST-5, TST-9), the list's delta
/// (TYPE-16), and what a node publishes to its own `out` (TIME-7).
const VALIDITY: (&str, &str) = fixture!("demo.validity.hgl");

/// TIME-1: a child is hgraph's own child of the input, an endpoint
/// whether or not it is valid; a read at a run-time index is checked
/// when it runs (ERR-14). TST-5: a whole cell is marked so, and the
/// value the result holds is read for it. TYPE-16: a fixed list's delta
/// keeps its bounds. TIME-7: `out[key] = value` writes the one entry,
/// and a fixed list's `out` is hgraph's `TS_OUT[TSL[...]]`.
#[test]
fn a_collections_child_is_an_endpoint_of_its_own() {
    let emitted = emit_clean(&[VALIDITY]);
    let python = emitted.module("demo.validity");
    let pair = "_hg.TSL[_hg.TS[_py.int], _hg.Size[2]]";
    python.assert_writes(&[
        "    if _core.position_of(parts, 1).valid:\n".to_string(),
        "    if parts.all_valid:\n".to_string(),
        "    if quote[\"ask\"].valid:\n".to_string(),
        "    return _core.checked_child(_core.position_of(parts, slot.value), lambda _port: (_port.value if _port.valid else _core.unset_read()))\n".to_string(),
        "    return _core.checked_child_result(_core.position_of(parts, slot.value), lambda _port: (_port.value if _port.valid else _core.unset_read()))\n".to_string(),
        "    if _core.child_modified(_core.position_of(parts, 0)):\n".to_string(),
        "        return _core.child_delta(_core.position_of(parts, 1))\n".to_string(),
        "    return _core.entry_port(prices, \"B\").valid\n".to_string(),
        "    return _core.list_within(_core.list_delta(updated={slot.value: reading.value}), 2)\n".to_string(),
        "_hold=lambda _port: _core.list_value(_port)), [_harness.whole([1, 2], fixed=True), _harness.whole([5, 6], fixed=True)]".to_string(),
        "    return _core.dict_replaced(_output, _py.dict(book.value))\n".to_string(),
        format!("def pair_once(value: _hg.TS[_py.int], _output: _hg.TS_OUT[{pair}] = None) -> {pair}:"),
        "    _output.value = {key.value: value.value}\n".to_string(),
        "    _output[0].value = value.value\n".to_string(),
    ]);
}

/// What this target does not write of a collection is reported: an
/// unsized list of a nullable published whole (TYPE-14, TIME-7); a
/// fixed list of no position, which hgraph 0.9.8's `TSL` cannot publish
/// (TYPE-10, TIME-9); an assignment inside a child of a node's `out`
/// (TIME-7); and a trace cell written whole whose parts are composites,
/// a struct's or a tuple's, or a list's that are nullable, or of a
/// struct below the cell's (TST-5, TST-9).
#[test]
fn a_collection_form_not_written_yet_is_reported_by_this_lane() {
    let emitted = emit(&[fixture!(
        "a_collection_form_not_written_yet_is_reported_by_this_lane",
        "m.hgl"
    )]);
    assert_eq!(
        emitted.reported,
        [
            "m.hgl:3:19 target.unsupported: the hgraph target does not write a whole list published where an element is temporal composite or nullable yet",
            "m.hgl:5:4 target.unsupported: the hgraph target does not write the type `list<i64, 0>` yet",
            "m.hgl:13:12 target.unsupported: the hgraph target does not write an assignment inside an entry of `out` yet",
            "m.hgl:21:31 target.unsupported: the hgraph target does not write a whole trace cell of composites yet",
            "m.hgl:21:70 target.unsupported: the hgraph target does not write a whole trace cell of composites yet",
            "m.hgl:21:96 target.unsupported: the hgraph target does not write a whole trace cell of a list of nullables yet",
            "m.hgl:29:36 target.unsupported: the hgraph target does not write a whole trace cell of a struct below its type yet",
            "m.hgl:35:31 target.unsupported: the hgraph target does not write a whole trace cell of composites yet",
        ]
    );
}

// ---- Constants by value (UNIT-13) ----

/// UNIT-13: "A compile-time constant ... is one value of the type its
/// place gives it (EXPR-5): `1` where an `f64` is expected is `1.0`. Two
/// compile-time constants are one where those values are the same,
/// however each is written", and "That sameness gives type identity
/// (TYPE-3) and the instance a call makes (GEN-13)". So `FBox<1>` and
/// `FBox<1.00>` are one class, and `scaled<1>` and `scaled<1.0>` one
/// instance, which writes the value as `1.0`.
#[test]
fn a_constant_argument_is_written_by_its_value() {
    let emitted = emit(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "struct FBox<const X: f64> { v: i64 }\n",
            "const fn a() -> FBox<1> => FBox<1>(v: 1)\n",
            "const fn b() -> FBox<1.00> => FBox<1.00>(v: 2)\n",
            "fn scaled<const k: f64>(x: f64) -> f64 => x * k\n",
            "fn uses(x: f64) -> f64 => scaled<1>(x) + scaled<1.0>(x) + scaled<2.5>(x)\n",
        ),
    )]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let python = emitted.text("_hgl/_m_m/__init__.py");
    assert_eq!(
        python.matches("_core.applied(FBox, \"1.0\")").count(),
        2,
        "{python}"
    );
    for expected in [
        "_lift_2_add__f64_f64(lhs=_inst_scaled__1_0(x=x), rhs=_inst_scaled__1_0(x=x))",
        "def _inst_scaled__1_0(x: _hg.TS[_py.float]) -> _hg.TS[_py.float]:\n",
        "    return _lift_0_mul__f64_f64(lhs=x, rhs=1.0)\n",
        "def _inst_scaled__2_5(x: _hg.TS[_py.float]) -> _hg.TS[_py.float]:\n",
    ] {
        assert!(python.contains(expected), "{expected}\nnot in\n{python}");
    }
    assert_eq!(python.matches("def _inst_scaled__").count(), 2, "{python}");
}

/// UNIT-13: "`-0.0` and `0.0` are two constants, though `-0.0 ==
/// 0.0`", and GEN-13: "arguments that differ make another" instance.
/// Each instance is written with its own constant, whichever call is met
/// first: core's `1.0 / -0.0` is negative infinity and `1.0 / 0.0`
/// positive.
#[test]
fn a_negative_zero_is_an_instance_of_its_own() {
    let emitted = emit(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "fn negative<const k: f64>(x: f64) -> bool => x / k < 0.0\n",
            "fn first(x: f64) -> bool => negative<-0.0>(x)\n",
            "fn second(x: f64) -> bool => negative<0.0>(x)\n",
        ),
    )]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let python = emitted.text("_hgl/_m_m/__init__.py");
    assert_eq!(
        python.matches("def _inst_negative__").count(),
        2,
        "{python}"
    );
    assert!(python.contains("div__f64_f64(lhs=x, rhs=-0.0)"), "{python}");
    assert!(python.contains("div__f64_f64(lhs=x, rhs=0.0)"), "{python}");
}

/// GEN-13's fragment, `const fn reciprocal<const k: f64>() -> f64 =>
/// 1.0 / k`, with its calls in the order `first` gives them and in the
/// other: "`reciprocal<1>()` and `reciprocal<1.0>()` call one instance.
/// `reciprocal<0.0>()` is positive infinity and `reciprocal<-0.0>()`
/// negative infinity, whichever is called first." Each program's test
/// asserts the two infinities by their text (OPS-6: "the infinities are
/// `inf` and `-inf`").
fn reciprocal_calls(first: &str, second: &str) -> String {
    format!(
        "module m\n\
         const fn reciprocal<const k: f64>() -> f64 => 1.0 / k\n\
         const fn shown() -> list<str, 4> => [\n    \
         convert<str>({first}), convert<str>({second}),\n    \
         convert<str>(reciprocal<1>()), convert<str>(reciprocal<1.0>()),\n]\n\
         test signs {{\n    \
         assert shown() == [\"{}\", \"{}\", \"1.0\", \"1.0\"]\n}}\n",
        if first.contains('-') { "-inf" } else { "inf" },
        if second.contains('-') { "-inf" } else { "inf" },
    )
}

/// The two orders of `reciprocal_calls`: each zero is an instance of its
/// own, written with its own constant, and `1` and `1.0` are one.
#[test]
fn the_reciprocal_of_each_zero_is_an_instance_of_its_own_in_either_order() {
    for (first, second) in [
        ("reciprocal<0.0>()", "reciprocal<-0.0>()"),
        ("reciprocal<-0.0>()", "reciprocal<0.0>()"),
    ] {
        let text = reciprocal_calls(first, second);
        let emitted = emit(&[("m.hgl", text.as_str())]);
        assert_eq!(emitted.reported, Vec::<String>::new());
        let python = emitted.text("_hgl/_m_m/__init__.py");
        for expected in [
            "def _inst_reciprocal__0_0():\n    return _core.div__f64_f64(lhs=1.0, rhs=0.0, zero=\"ieee\")\n",
            "def _inst_reciprocal___0_0():\n    return _core.div__f64_f64(lhs=1.0, rhs=-0.0, zero=\"ieee\")\n",
            "def _inst_reciprocal__1_0():\n",
        ] {
            assert!(python.contains(expected), "{expected}\nnot in\n{python}");
        }
        assert_eq!(
            python.matches("def _inst_reciprocal__").count(),
            3,
            "{python}"
        );
    }
}

/// The same two programs run on hgraph: positive infinity and negative
/// infinity whichever is called first. Needs hgraph, as above.
#[test]
#[ignore = "needs a Python with hgraph, named by HGL_HGRAPH_PYTHON"]
fn the_reciprocal_of_each_zero_runs_in_either_order() {
    for (name, first, second) in [
        (
            "hgraph_reciprocal_zero_first",
            "reciprocal<0.0>()",
            "reciprocal<-0.0>()",
        ),
        (
            "hgraph_reciprocal_minus_zero_first",
            "reciprocal<-0.0>()",
            "reciprocal<0.0>()",
        ),
    ] {
        let hgraph = on_hgraph(name);
        let text = reciprocal_calls(first, second);
        let emitted = emit_clean(&[("m.hgl", text.as_str())]);
        assert_eq!(hgraph.run_all(&emitted), [Ran::Passed], "{first} first");
    }
}

/// TIME-7: a node's delta of a value that holds an unsized list and a
/// dictionary is published with both shapes, so that a delta that names
/// no change of the dictionary, "Where it is valid ... is no tick"
/// (`demo.pins2_wave3` runs it). A value that holds no dictionary or set
/// is told none.
#[test]
fn a_delta_beside_an_unsized_list_is_told_where_its_dictionaries_are() {
    let emitted = emit(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "fn books(step: i64) -> tuple<list<i64>, dict<str, dict<str, i64>>> {\n",
            "    when { return (null, delta<dict<str, dict<str, i64>>>(updated: [\"a\": delta<dict<str, i64>>()])) }\n",
            "}\n",
            "fn counts(step: i64) -> tuple<i64, list<i64>> {\n",
            "    when { return (null, delta<list<i64>>()) }\n",
            "}\n",
        ),
    )]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let python = emitted.text("_hgl/_m_m/__init__.py");
    for expected in [
        "(\"list\", None,), entries=(\"positions\", (None, (\"entries\", \"entries\"),)))\n",
        "return _core.published_into(_output, (None, _core.list_delta(),), (None, \"list\",), entries=(\"positions\", (None, None,)))\n",
    ] {
        assert!(python.contains(expected), "{expected}\nnot in\n{python}");
    }
}

/// GEN-13: a constant given a `list<any>` is written whole, however
/// deep: a nested list, a tuple and a member each say their own type.
#[test]
fn a_constant_under_any_is_written_whole() {
    let emitted = emit(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "enum Mode { fast, slow }\n",
            "const fn values<const X: list<any>>() -> list<any> => X\n",
            "const fn a() -> list<any> => values<[[1]]>()\n",
            "const fn c() -> list<any> => values<[Mode.slow]>()\n",
            "const fn d() -> list<any> => values<[(1, \"a\")]>()\n",
        ),
    )]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let python = emitted.text("_hgl/_m_m/__init__.py");
    for expected in [
        "    return _core.copied([[1]])\n",
        "    return _core.copied([Mode[\"slow\"]])\n",
        "    return _core.copied([(1, \"a\",)])\n",
    ] {
        assert!(python.contains(expected), "{expected}\nnot in\n{python}");
    }
}

/// TYPE-11: in a window of a duration "a value is held while its age is
/// at most Max ... and leaves one smallest step later (RUN-3), whether or
/// not a value arrives then. The window ticks for that exit", as hgraph
/// 0.9.8's `TSW` does (hhenson/hgraph#1818): its sizes are written as the
/// program writes them, "A minimum of `0s` is held by the first value".
/// Its delta is read where a `result` is taken (ERR-14), which is
/// reported (`a_window_form_not_written_yet_is_reported`). The fixture's
/// tests run on hgraph with the others.
#[test]
fn a_duration_windows_exit_is_written() {
    let emitted = emit_clean(&[fixture!("demo.hgraph097_wave3.hgl")]);
    let python = emitted.module("demo.hgraph097_wave3");
    let micros = |count: i64| format!("_hg.WindowSize[_dt.timedelta(microseconds={count})]");
    python.assert_writes(&[format!(
        "def from_zero(reading: _hg.TS[_py.int]) -> _hg.TSW[_py.int, {}, {}]:",
        micros(3),
        micros(0)
    )]);
}

/// A window of a duration has no delta where a value leaves and none
/// arrives (TYPE-11); what the delta of a struct or a list that holds one
/// is then, the rules do not say, and hgraph gives the field none, as for
/// one that did not change: it is reported. One of a window of ticks is
/// written. The child's own delta is read where a `result` is taken
/// (ERR-14), which is reported.
#[test]
fn the_delta_of_a_value_that_holds_a_duration_window_is_reported() {
    let emitted = emit(&[fixture!(
        "the_delta_of_a_value_that_holds_a_duration_window_is_reported",
        "m.hgl"
    )]);
    assert_eq!(
        emitted.reported,
        [
            "m.hgl:11:19 target.unsupported: the hgraph target does not write the delta of a value that holds a window of a duration yet",
            "m.hgl:14:19 target.unsupported: the hgraph target does not write the delta of a value that holds a window of a duration yet",
            "m.hgl:20:40 target.unsupported: the hgraph target does not write `delta_value` of a child or of a window of a duration, or `removed_value`, read where a `result` is taken (ERR-14) yet",
        ]
    );
}

/// How a candidate is selected for each instance of a generic function,
/// where a generic parameter of that function selects it (GEN-3, GEN-5,
/// GEN-6). Its tests run on hgraph with every fixture's
/// (`the_generated_tests_pass_on_hgraph`).
const GENERICS_FIXES_WAVE3: (&str, &str) = fixture!("demo.generics_fixes_wave3.hgl");

/// GEN-7: "A generic body is checked under its stated requirements".
/// GEN-16: the body of `wrap<U>` calls `kind_of`, which it requires, and
/// each instance is written with the candidate that answers the
/// requirement at its call: for an `i64` the concrete one, for any other
/// the generic one (GEN-5: "Concrete types outrank variables"); in an
/// instance reached through another; and in a graph, lifted (FN-6).
#[test]
fn a_requirement_is_written_with_its_answer_for_each_instance() {
    let emitted = emit_clean(&[GENERICS_FIXES_WAVE3]);
    let python = emitted.module("demo.generics_fixes_wave3");
    python.assert_writes(&[
        "def _inst_wrap__i64(value):\n    return _impl_kind_of_i64(value=value)",
        "def _inst_wrap__bool(value):\n    return _impl_kind_of_T(value=value)",
        "def _inst_twice__f64(value):\n    return _core.add__i64_i64(lhs=_impl_kind_of_T(value=value), rhs=_impl_kind_of_T(value=value))",
        "def _inst_relay__i64(value):\n    return _inst_wrap__i64(value=value)",
        "(value: _hg.TS[_py.bool]) -> _hg.TS[_py.int]:\n    return _impl_kind_of_T(value=value.value)",
    ]);
}

/// GEN-6: "A call admits the candidate only where that holds for the
/// parameter's value at the call". GEN-3: "A positive conjunctive
/// equality may bind one unresolved generic from known data". Core's
/// division given its `zero` option by a generic value parameter the
/// body's `requires` binds is written with the bound value.
#[test]
fn an_option_a_bound_generic_parameter_gives_is_written_with_its_value() {
    let emitted = emit_clean(&[GENERICS_FIXES_WAVE3]);
    let python = emitted.module("demo.generics_fixes_wave3");
    python.assert_writes(&[
        "return _core.div__f64_f64(lhs=a, rhs=b, zero=\"nan\")",
        "return _core.div__f64_f64(lhs=a, rhs=b, zero=\"error\")",
    ]);
}

/// GEN-3: a membership "authorizes in the body (GEN-7) an operation that
/// each listed type has a candidate for ... and each instance selects its
/// own candidate (GEN-5)". Under a membership each instance
/// is written with the candidate it selects: by name, by symbol (core's
/// `<`), and for the `zero` option of core's division that a generic
/// value parameter gives.
#[test]
fn a_membership_is_written_with_each_instances_candidate() {
    let emitted = emit_clean(&[GENERICS_FIXES_WAVE3]);
    let python = emitted.module("demo.generics_fixes_wave3");
    python.assert_writes(&[
        "def _inst_sized_of__i64(value):\n    return _impl_sized_i64(value=value)",
        "def _inst_sized_of__str(value):\n    return _impl_sized_str(value=value)",
        "def _inst_less_num__f64(a, b):\n    return _core.lt__T_T(lhs=a, rhs=b)",
        "return _core.div__f64_f64(lhs=a, rhs=b, zero=\"ieee\")",
    ]);
}

/// UNIT-13: "A constant is read by its name wherever an ordinary value
/// is." A `requires` that compares a generic value parameter with a
/// module's constant holds for the constant's value, and the instance
/// is written.
#[test]
fn a_constant_in_a_requires_is_read_by_its_name() {
    let emitted = emit_clean(&[GENERICS_FIXES_WAVE3]);
    let python = emitted.module("demo.generics_fixes_wave3");
    python.assert_writes(&["def _inst_named__3(v):", "def _inst_among__5(v):"]);
}

/// Lexes, parses and analyses `files` as [`emit`] does, and writes the
/// program for hgraph whatever the analyser reported: each code the
/// analyser reported, and what the target reported, each as `emit`
/// gives it.
fn written_despite(files: &[(&str, &str)]) -> (Vec<String>, Vec<String>) {
    let mut sources = SourceMap::new();
    let mut diagnostics = Diagnostics::new();
    let mut parsed = Vec::new();
    for (name, text) in files.iter().chain(&hgl_analyser::CORE_FILES) {
        let file = sources.add(*name, *text).unwrap();
        let tokens = lex(&sources, file, &mut diagnostics);
        let tree = parse(
            &sources,
            file,
            &tokens,
            Entry::SourceFile,
            Limits::default(),
            &mut diagnostics,
        );
        parsed.push(ParsedFile {
            file,
            root: tree.syntax(),
            omit: Vec::new(),
        });
    }
    let program = analyse(&parsed, &mut diagnostics);
    let analysed: Vec<String> = diagnostics
        .as_slice()
        .iter()
        .map(|diagnostic| diagnostic.code.as_str().to_string())
        .collect();
    let before = diagnostics.len();
    transpile(&program, &mut diagnostics);
    let reported = diagnostics.as_slice()[before..]
        .iter()
        .map(|diagnostic| {
            let source = sources.file(diagnostic.primary.file);
            format!(
                "{}:{} {}: {}",
                source.name(),
                source.line_col(diagnostic.primary.span.start),
                diagnostic.code.as_str(),
                diagnostic.message
            )
        })
        .collect();
    (analysed, reported)
}

/// The instances of one declaration that one call makes are written up
/// to the analyser's bound (`hgl_analyser::INSTANCES`), so writing a
/// program ends however many it would make. A call that makes exactly
/// that many is written whole. One that makes one more, which the
/// analyser rejects (`generic.instance_limit`, GEN-13), is not written past
/// them, and the call of the next is reported; and so is one whose
/// instances never end, `climb<N + 1>` in `climb<N>`, which is given to
/// the target here as the analyser leaves it.
#[test]
fn the_instances_one_call_makes_are_written_up_to_one_bound() {
    let turning = |size: usize| {
        format!(
            "module m\n\
             const fn need<const N: i64>(v: i64) -> i64 => v + N\n\
             const fn turn<const N: i64>(v: i64) -> i64 =>\n    \
                 if v <= 0 {{ need<N>(v) }} else {{ turn<((N + 1) % {size})>(v - 1) }}\n\
             test a {{ assert turn<0>(3) == 3 }}\n"
        )
    };
    let within = emit_clean(&[("m.hgl", &turning(hgl_analyser::INSTANCES))]);
    let python = within.text("_hgl/_m_m/__init__.py");
    assert_eq!(
        python.matches("def _inst_turn__").count(),
        hgl_analyser::INSTANCES
    );
    let (analysed, reported) = written_despite(&[("m.hgl", &turning(hgl_analyser::INSTANCES + 1))]);
    assert_eq!(analysed, ["generic.instance_limit"]);
    assert_eq!(
        reported,
        [
            "m.hgl:4:37 target.unsupported: the hgraph target does not write an instance of `turn` past the 4096 instances of it one call makes yet"
        ]
    );
    let (analysed, reported) = written_despite(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "const fn small<const K: i64>(v: i64) -> i64 => v + K\n",
            "const fn climb<const N: i64>(v: i64) -> i64 =>\n",
            "    if v <= 0 { small<N>(v) } else { climb<N + 1>(v - 1) }\n",
            "test a { assert climb<0>(3) == 3 }\n",
        ),
    )]);
    assert_eq!(analysed, ["generic.instance_limit"]);
    assert_eq!(
        reported,
        [
            "m.hgl:4:38 target.unsupported: the hgraph target does not write an instance of `climb` past the 4096 instances of it one call makes yet"
        ]
    );
}

/// END-4, END-5: a window result discards history and a pending arrival
/// when explicitly withdrawn. Its next value starts a fresh window.
#[test]
fn a_window_can_be_withdrawn_and_republished() {
    let emitted = emit_clean(&[fixture!("demo.window_withdrawal.hgl")]);
    emitted.module("demo.window_withdrawal").assert_writes(&[
        "with _core.WindowWrites(_output) as _window:",
        "_window.invalidate()",
        "_window.hold(80)",
        "_core.invalidate_window(_output)",
    ]);
}
