//! The hgraph target against small programs: the Python it writes,
//! what it reports for a form it does not write yet, and which tests
//! can then still be run.

use std::fs;
use std::path::Path;

use hgl_analyser::{ParsedFile, analyse};
use hgl_diagnostics::{Diagnostics, Location, render};
use hgl_hgraph::{
    Output, Ran, RanApplication, Stop, Test, run, run_application, run_within, transpile,
};
use hgl_lexer::{lex, literal};
use hgl_parser::{Entry, Limits, parse};
use hgl_source::SourceMap;

/// What the target makes of a program.
struct Emitted {
    /// The program's files, to say where a place is.
    sources: SourceMap,
    /// The files written, and the program's tests.
    output: Output,
    /// What the target reported, each as `file:line:column code`.
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
    let reported = diagnostics.as_slice()[before..]
        .iter()
        .map(|diagnostic| {
            let source = sources.file(diagnostic.primary.file);
            assert_eq!(diagnostic.category.as_str(), "build");
            assert!(hgl_hgraph::codes::ALL.contains(&diagnostic.code.as_str()));
            format!(
                "{}:{} {}",
                source.name(),
                source.line_col(diagnostic.primary.span.start),
                diagnostic.code.as_str()
            )
        })
        .collect();
    Emitted {
        sources,
        output,
        reported,
    }
}

/// A file of the pinned core, by its path under `spec/hgl_core`.
fn core_file(path: &str) -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/hgl_core");
    fs::read_to_string(root.join(path))
        .unwrap_or_else(|error| {
            panic!("cannot read {path}: {error}; run `git submodule update --init`")
        })
        .replace("\r\n", "\n")
}

/// A module with each form the target writes: a value function, a node
/// with a `state` and a `const` parameter, a graph, a node with no
/// result, and a test that binds, compares and runs an eval.
const DEMO: &str = concat!(
    "module demo.scaling\n",
    "\n",
    "const fn doubled(value: i64) -> i64 => value * 2\n",
    "\n",
    "fn scaled(value: i64, const factor: i64) -> i64 {\n",
    "    state seen: i64 = 0\n",
    "    when {\n",
    "        seen += 1\n",
    "        if seen > 1 {\n",
    "            return doubled(value) * factor\n",
    "        } else {\n",
    "            return value\n",
    "        }\n",
    "    }\n",
    "}\n",
    "\n",
    "fn wired(value: i64) -> i64 => scaled(value, 3)\n",
    "\n",
    "fn sink(value: bool) {\n",
    "    when { }\n",
    "}\n",
    "\n",
    "test scales {\n",
    "    let seen = eval(wired(value: [1, _, 2]))\n",
    "    assert seen == [1, _, 12]\n",
    "    assert eval(scaled(value: [5], factor: 2)) != []\n",
    "    eval(sink(value: [true]))\n",
    "}\n",
);

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
    let emitted = emit(&[("demo.hgl", DEMO)]);
    assert_eq!(emitted.reported, Vec::<String>::new());
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
            "_hgl/_harness.py"
        ]
    );
    assert_eq!(
        emitted.text("_hgl/_m_demo/_m_scaling/__init__.py"),
        DEMO_PYTHON
    );
}

/// Core's dense scalar eval cases are written whole: every node and
/// every test of `scalar_eval.hgl`. They use a bare `when` (TIME-3), a
/// `state` (TIME-6), `delta_value` (END-3), a `return` on one path only
/// (FN-7), `i64` addition (NUM-2) and dense traces (TST-6).
#[test]
fn cores_scalar_eval_cases_are_written_whole() {
    let text = core_file("tests/conformance/scalar_eval.hgl");
    let emitted = emit(&[("scalar_eval.hgl", &text)]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let python = emitted.text("_hgl/_m_conformance/__init__.py");
    for expected in [
        "def pass_i64(value: _hg.TS[_py.int]) -> _hg.TS[_py.int]:\n    return value.delta_value\n",
        "    if (a.valid and (a.modified and a.valid)):\n        return a.delta_value\n",
        "    _state.count = _core.add__i64_i64(lhs=_state.count, rhs=1)\n",
        "def _start_count_publications(_state: _hg.STATE = None):\n    _state.count = 0\n",
        "    if not (_harness.same(_harness.evaluate(pass_i64, {\"value\": _hgl_4core._inst_replay__i64}, _hgl_4core._inst_record__i64, value=[None, 7, None, None]), [None, 7])):\n",
        "        raise _py.AssertionError()\n",
        "    if not (_harness.same(_harness.evaluate(pass_str, {\"value\": _hgl_4core._inst_replay__str}, _hgl_4core._inst_record__str, value=[\"\", None, \"\", \"a\"]), [\"\", None, \"\", \"a\"])):\n",
        "        raise _py.AssertionError()\n",
    ] {
        assert!(python.contains(expected), "the module has:\n{expected}");
    }
    assert_eq!(
        python.matches("\ndef test_").count(),
        8,
        "one function a test"
    );
}

/// Core's scalar endpoint cases that publish no null, and a node with
/// two handlers: `when` headers with selectors, and the node's own
/// output.
const ENDPOINTS: &str = concat!(
    "module demo.endpoints\n",
    "\n",
    "fn watched_valid(watched: i64, probe: bool) -> bool {\n",
    "    when valid([]) {\n",
    "        return valid(watched)\n",
    "    }\n",
    "}\n",
    "\n",
    "fn watched_modified(watched: i64, probe: bool) -> bool {\n",
    "    when modified() && valid([]) {\n",
    "        return modified(watched)\n",
    "    }\n",
    "}\n",
    "\n",
    "fn sample_on_probe(watched: i64, probe: bool) -> i64 {\n",
    "    when modified(probe) && valid() {\n",
    "        return watched\n",
    "    }\n",
    "}\n",
    "\n",
    "test silence_preserves_validity {\n",
    "    assert eval(watched_valid(\n",
    "                watched: [_, 7, _, _],\n",
    "                probe: [true, true, true, true])) == [false, true, true, true]\n",
    "}\n",
    "\n",
    "test equal_publication_is_modified_and_idle_is_not {\n",
    "    assert eval(watched_modified(\n",
    "                watched: [_, 7, _, 7],\n",
    "                probe: [true, true, true, _])) == [false, true, false, true]\n",
    "}\n",
    "\n",
    "test retained_value_is_read_without_a_new_tick {\n",
    "    assert eval(sample_on_probe(\n",
    "                watched: [_, 7, _, _, 9, _],\n",
    "                probe: [true, _, true, true, _, true])) == [_, _, 7, 7, _, 9]\n",
    "}\n",
    "\n",
    "fn produce(command: i64) -> i64 {\n",
    "    inject out\n",
    "    when {\n",
    "        if command != 0 { return command }\n",
    "        invalidate(out)\n",
    "    }\n",
    "}\n",
    "\n",
    "fn watch(input: i64) -> bool {\n",
    "    when valid([]) { return valid(input) }\n",
    "}\n",
    "\n",
    "fn observe(command: i64) -> bool => watch(produce(command))\n",
    "\n",
    "test publish_invalidate_recover {\n",
    "    assert eval(observe(command: [7, 0, 0, 7])) == [true, false, _, true]\n",
    "}\n",
    "\n",
    "fn publish_value(value: i64) -> i64 {\n",
    "    when { return value }\n",
    "}\n",
    "\n",
    "fn read_modified(watched: i64, probe: bool) -> bool {\n",
    "    when modified(probe) && valid([]) { return modified(watched) }\n",
    "}\n",
    "\n",
    "fn observe_modified(value: i64, probe: bool) -> bool =>\n",
    "    read_modified(publish_value(value), probe)\n",
    "\n",
    "test equal_publications_are_modified {\n",
    "    assert eval(observe_modified(\n",
    "                value: [_, 7, _, 7],\n",
    "                probe: [true, true, true, true])) == [false, true, false, true]\n",
    "}\n",
    "\n",
    "fn two_handlers(a: i64, b: i64) -> i64 {\n",
    "    when modified(a) && a > 5 {\n",
    "        return a\n",
    "    }\n",
    "    when modified(b) && valid(b) {\n",
    "        return b\n",
    "    }\n",
    "}\n",
    "\n",
    "test handlers_in_order {\n",
    "    assert eval(two_handlers(a: [1, 9, _, 9], b: [_, _, 3, 4])) == [_, _, 3, 9]\n",
    "}\n",
);

/// TIME-3: a selector in a `when` header sets what wakes the handler
/// and what it requires valid. TIME-4: "Aggregate subscriptions cover
/// every handler. Node-wide validity includes only requirements common
/// to executable handlers; other guards stay per-handler." TIME-7 and
/// END-4: `inject out` is the node's output, which `invalidate` clears.
#[test]
fn a_when_header_says_what_wakes_and_admits_a_node() {
    let emitted = emit(&[("endpoints.hgl", ENDPOINTS)]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let python = emitted.text("_hgl/_m_demo/_m_endpoints/__init__.py");
    for expected in [
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
            "    if a.modified and a.valid and _core.gt__T_T(lhs=a.value, rhs=5):\n",
            "        return a.value\n",
            "    if b.modified:\n",
            "        return b.value\n",
        ),
    ] {
        assert!(
            python.contains(expected),
            "the module has:\n{expected}\nin:\n{python}"
        );
    }
}

/// Three modules: two that each export a `pick`, and one that has a
/// `pick` of its own, calls both, leaves a nullable argument out, and
/// gives a `state` its first value from a `const` parameter.
const MODULES: [(&str, &str); 3] = [
    (
        "left.hgl",
        concat!("module demo.left\n", "export const fn pick() -> i64 => 1\n",),
    ),
    (
        "right.hgl",
        concat!(
            "module demo.right\n",
            "export const fn pick() -> i64 => 2\n",
        ),
    ),
    (
        "configured.hgl",
        concat!(
            "module demo.configured\n",
            "use demo.left as l\n",
            "use demo.right as r\n",
            "\n",
            "const fn pick() -> i64 => 100\n",
            "\n",
            "const fn chosen(fallback: i64?) -> i64 {\n",
            "    if fallback == null { return l::pick() }\n",
            "    return r::pick() + pick()\n",
            "}\n",
            "\n",
            "fn kept(value: i64, const first: i64) -> i64 {\n",
            "    state last: i64 = first\n",
            "    when {\n",
            "        let before = last\n",
            "        last = value\n",
            "        return before + chosen()\n",
            "    }\n",
            "}\n",
            "\n",
            "test keeps {\n",
            "    assert eval(kept(value: [5, 6], first: 9)) == [10, 6]\n",
            "    assert chosen(1) == 102\n",
            "}\n",
        ),
    ),
];

/// A function of another module is named through its module, so that
/// two modules' functions of one name, and this module's own, stay
/// apart. A lifecycle hook is a function of its own to hgraph, and is
/// given the node's `const` parameters. UNIT-8: a parameter written
/// `T?` may be left out, and is then null.
#[test]
fn names_configuration_and_omitted_arguments_are_kept() {
    let emitted = emit(&MODULES);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let python = emitted.text("_hgl/_m_demo/_m_configured/__init__.py");
    for expected in [
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
    ] {
        assert!(
            python.contains(expected),
            "the module has:\n{expected}\nin:\n{python}"
        );
    }
}

/// Headers whose selectors add up, repeat an input, or name the node's
/// own output.
const SELECTORS: &str = concat!(
    "module demo.selectors\n",
    "\n",
    "fn both_valid(a: i64, b: i64, c: i64) -> i64 {\n",
    "    when valid(a) && valid(b) { return a + b }\n",
    "}\n",
    "\n",
    "fn repeated(a: i64, b: i64) -> i64 {\n",
    "    when modified(a, a) { return a }\n",
    "    when modified(b) { return b }\n",
    "}\n",
    "\n",
    "fn recovering(command: i64) -> i64 {\n",
    "    inject out\n",
    "    when valid(out) && valid(command) && command > 0 { return command }\n",
    "    when { return 1 }\n",
    "}\n",
    "\n",
    "test selectors {\n",
    "    assert eval(both_valid(a: [1, _], b: [_, 2], c: [])) == [_, 3]\n",
    "    assert eval(repeated(a: [1, _, 3], b: [_, 2, _])) == [_, 2, 3]\n",
    "    assert eval(recovering(command: [0, 5])) == [1, 5]\n",
    "}\n",
);

/// TIME-3: every selector among a header's conjuncts adds what it
/// names, so `valid(a) && valid(b)` requires both. A subscription is a
/// set: an input named twice is one. The node's output named in a
/// header is hgraph's `_output`.
#[test]
fn a_headers_selectors_add_up() {
    let emitted = emit(&[("selectors.hgl", SELECTORS)]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let python = emitted.text("_hgl/_m_demo/_m_selectors/__init__.py");
    for expected in [
        "@_hg.compute_node(valid=(\"a\", \"b\"))\ndef both_valid(",
        concat!(
            "@_hg.compute_node\n",
            "def repeated(a: _hg.TS[_py.int], b: _hg.TS[_py.int]) -> _hg.TS[_py.int]:\n",
            "    if a.modified:\n",
            "        return a.value\n",
            "    if b.modified:\n",
            "        return b.value\n",
        ),
        concat!(
            "    if _output.valid and _core.gt__T_T(lhs=command.value, rhs=0):\n",
            "        return command.value\n",
            "    return 1\n",
        ),
    ] {
        assert!(
            python.contains(expected),
            "the module has:\n{expected}\nin:\n{python}"
        );
    }
}

/// Defaults: of a value function's parameter, where it calls another
/// function, and of a node's `const` parameter.
const DEFAULTS: &str = concat!(
    "module demo.defaults\n",
    "\n",
    "const fn base() -> i64 => 2\n",
    "\n",
    "const fn scale(value: i64, factor: i64 = base()) -> i64 => value * factor\n",
    "\n",
    "fn offset(value: i64, const by: i64 = 10) -> i64 {\n",
    "    when { return value + by }\n",
    "}\n",
    "\n",
    "fn wired(value: i64) -> i64 => offset(value)\n",
    "\n",
    "test defaults {\n",
    "    assert scale(3) == 6\n",
    "    assert scale(3, 5) == 15\n",
    "    assert eval(offset(value: [1])) == [11]\n",
    "    assert eval(offset(value: [1], by: 1)) == [2]\n",
    "    assert eval(wired(value: [5])) == [15]\n",
    "}\n",
);

/// UNIT-8: a parameter with a default may be left out. The call is
/// written with the default in its place, which the analysed program
/// holds as a typed value.
#[test]
fn a_default_is_written_where_a_call_leaves_it_out() {
    let emitted = emit(&[("defaults.hgl", DEFAULTS)]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let python = emitted.text("_hgl/_m_demo/_m_defaults/__init__.py");
    for expected in [
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
    ] {
        assert!(
            python.contains(expected),
            "the module has:\n{expected}\nin:\n{python}"
        );
    }
}

/// A default written in one module and left out by a call in another,
/// whose caller has a parameter named as a function the default calls;
/// and a parameter written `T?` before one with a default.
const DEFAULT_MODULES: [(&str, &str); 2] = [
    (
        "lib.hgl",
        concat!(
            "module demo.lib\n",
            "const fn base() -> i64 => 2\n",
            "export const fn scale(value: i64, factor: i64 = base()) -> i64 => value * factor\n",
            "export const fn pick(first: i64?, second: i64 = 1) -> i64 => second\n",
        ),
    ),
    (
        "caller.hgl",
        concat!(
            "module demo.caller\n",
            "use demo.lib::{scale, pick}\n",
            "const fn caller(base: i64) -> i64 => scale(3) + base\n",
            "test callers {\n",
            "    assert caller(10) == 16\n",
            "    assert pick() == 1\n",
            "    assert pick(7, 5) == 5\n",
            "}\n",
        ),
    ),
];

/// A default means what it means where it is declared (UNIT-7): written
/// in a caller of another module, a function it calls is named through
/// its own module, so a caller's `base` is not taken for it. Within one
/// module no binding takes a name already in scope (UNIT-12). A
/// parameter written `T?` that is left out is given `None` at the call,
/// so Python has no default to put in order.
#[test]
fn a_default_keeps_its_meaning_in_another_module() {
    let emitted = emit(&DEFAULT_MODULES);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let python = emitted.text("_hgl/_m_demo/_m_caller/__init__.py");
    for expected in [
        "def caller(base):\n",
        "_hgl_4demo_3lib.scale(value=3, factor=_hgl_4demo_3lib.base())",
        "_hgl_4demo_3lib.pick(first=None, second=1)",
        "_hgl_4demo_3lib.pick(first=7, second=5)",
    ] {
        assert!(
            python.contains(expected),
            "the module has:\n{expected}\nin:\n{python}"
        );
    }
    let library = emitted.text("_hgl/_m_demo/_m_lib/__init__.py");
    assert!(library.contains("def pick(first, second):\n"));
}

/// Two modules: constants, and their reads in another module.
const CONSTANTS: [(&str, &str); 2] = [
    (
        "constants.hgl",
        concat!(
            "module demo.constants\n",
            "export enum Side { buy, sell }\n",
            "export const scale: i64 = 2\n",
            // A constant written after the one that reads it.
            "const first: i64 = 3 * late + 1\n",
            "const late: i64 = scale\n",
            "export const side: Side = .sell\n",
            "const fn seed() -> i64 => first\n",
            "export const fn choose(base: i64, value: i64 = seed()) -> i64 => base + value\n",
            "test constants_are_read {\n",
            "    assert first == 7\n",
            "    assert choose(1) == 8\n",
            "    assert side == Side.sell\n",
            "}\n",
        ),
    ),
    (
        "reads.hgl",
        concat!(
            "module demo.reads\n",
            "use demo.constants::{scale, choose}\n",
            "const twice: i64 = scale * 2\n",
            "fn scaled(value: i64, const factor: i64) -> i64 {\n",
            "    when { return value * factor }\n",
            "}\n",
            "fn wired(value: i64) -> i64 => scaled(value, scale)\n",
            "test constants_are_imported {\n",
            "    assert twice == 4\n",
            "    assert choose(twice) == 11\n",
            "    assert eval(wired(value: [1, 2])) == [2, 4]\n",
            "}\n",
        ),
    ),
];

/// TYPE-2: "elapsed `duration` ... Instants/durations use signed 64-bit
/// microseconds". On this target a `duration` is a `timedelta`, a count
/// of whole microseconds, as a value and as what a connection ticks.
const DURATIONS: &str = concat!(
    "module demo.durations\n",
    "const pause: duration = 1h30m\n",
    "const fn total(first: duration, second: duration) -> duration => first + second\n",
    "const fn less(first: duration, second: duration) -> duration => first - second\n",
    "const fn times(length: duration, count: i64) -> duration => length * count\n",
    "const fn scaled(length: duration, by: f64) -> duration => length * by\n",
    "fn sum(lhs: duration, rhs: duration) -> duration => lhs + rhs\n",
    "test durations_are_values_and_ticks {\n",
    "    assert total(1s, 250ms) == 1250ms\n",
    "    assert less(1s, 250ms) == 750ms\n",
    "    assert -less(250ms, 1s) == 750ms\n",
    "    assert times(pause, 2) == 3h\n",
    // Core writes a count times a duration as the duration times it.
    "    assert 2 * pause == 3h\n",
    // OPS-10: half a microsecond is a tie, and goes to the even one.
    "    assert scaled(1us, 0.5) == 0us\n",
    "    assert scaled(3us, 0.5) == 2us\n",
    // OPS-10: a duration times an infinity is out of range.
    "    assert raises(\"numeric.out_of_range\") { scaled(1s, 1.0 / 0.0) }\n",
    "    assert eval(sum(lhs: [1s, _, 5m], rhs: [2s, 3s])) == [3s, 4s, 5m3s]\n",
    "}\n",
);

/// A `duration` is written as a `timedelta`: its literal by its count of
/// microseconds (LEX-13), its type in an annotation, and its operators
/// as the provider's functions (OPS-1).
#[test]
fn a_duration_is_a_timedelta() {
    let emitted = emit(&[("durations.hgl", DURATIONS)]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let python = emitted.text("_hgl/_m_demo/_m_durations/__init__.py");
    for expected in [
        "import datetime as _dt\n",
        "@_core.constant\ndef pause():\n    return _dt.timedelta(microseconds=5400000000)\n",
        "return _core.add__duration_duration(lhs=first, rhs=second)",
        "return _core.sub__duration_duration(lhs=first, rhs=second)",
        "return _core.mul__duration_i64(lhs=length, rhs=count)",
        "return _core.mul__duration_f64(lhs=length, rhs=by)",
        "_hg.TS[_dt.timedelta]",
    ] {
        assert!(
            python.contains(expected),
            "the module has:\n{expected}\nin:\n{python}"
        );
    }
    // A module that names no duration does not import the library.
    let plain = emit(&[("demo.hgl", DEMO)]);
    assert!(
        !plain
            .text("_hgl/_m_demo/_m_scaling/__init__.py")
            .contains("import datetime")
    );
}

/// OPS-1: a duration by an `i64` or an `f64` is a duration, by a duration
/// an `f64`, and `//` of two an `i64`. OPS-10: a duration result is "the
/// exact result rounded to the nearest duration the type holds, a tie to
/// the even one". NUM-12: each division takes its zero policy.
const DIVIDED: &str = concat!(
    "module demo.durations.divided\n",
    "const fn shared(length: duration, among: i64) -> duration => length / among\n",
    "const fn scaled(length: duration, by: f64) -> duration => length / by\n",
    "const fn ratio(first: duration, second: duration) -> f64 => first / second\n",
    "const fn whole(first: duration, second: duration) -> i64 => first // second\n",
    "const fn checked(length: duration, among: i64) -> result<duration, error> =>\n",
    "    length /{zero: .error} among\n",
    "const fn steps(first: duration, second: duration) -> result<i64, error> =>\n",
    "    first //{zero: .error} second\n",
    "test a_duration_is_divided {\n",
    // Core's own examples (its OPS-10).
    "    assert shared(5m, 3) == 100s\n",
    "    assert shared(1us, 3) == 0us\n",
    "    assert shared(2us, 3) == 1us\n",
    // A tie goes to the even count.
    "    assert scaled(3us, 2.0) == 2us\n",
    "    assert scaled(1us, 2.0) == 0us\n",
    "    assert ratio(90m, 1h) == 1.5\n",
    // NUM-1: `//` rounds toward negative infinity.
    "    assert whole(7s, 2s) == 3\n",
    "    assert whole(-7s, 2s) == -4\n",
    // NUM-9: under `error` a zero divisor is an `err`, and any other
    // an `ok`.
    "    assert is_ok(checked(1s, 2)) && is_err(checked(1s, 0))\n",
    "    assert is_ok(steps(1s, 1ms)) && is_err(steps(1s, 0s))\n",
    // NUM-3: and under `fail`, which these take by default, an error.
    "    assert raises(\"numeric.division_by_zero\") { shared(1s, 0) }\n",
    "    assert raises(\"numeric.division_by_zero\") { scaled(1s, 0.0) }\n",
    "    assert raises(\"numeric.division_by_zero\") { whole(1s, 0s) }\n",
    // OPS-10: a result outside the range a duration has, and a
    // duration divided by an infinity.
    "    assert raises(\"numeric.out_of_range\") { scaled(106751991d, 0.5) }\n",
    "    assert raises(\"numeric.out_of_range\") { scaled(1s, 1.0 / 0.0) }\n",
    "}\n",
);

/// A division of durations is the provider's function, given its zero
/// policy by name as a division of numbers is.
#[test]
fn a_duration_divided_takes_its_zero_policy() {
    let emitted = emit(&[("divided.hgl", DIVIDED)]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let python = emitted.text("_hgl/_m_demo/_m_durations/_m_divided/__init__.py");
    for expected in [
        "return _core.div__duration_i64(lhs=length, rhs=among, zero=\"fail\")",
        "return _core.div__duration_f64(lhs=length, rhs=by, zero=\"fail\")",
        "return _core.div__duration_duration(lhs=first, rhs=second, zero=\"ieee\")",
        "return _core.floordiv__duration_duration(lhs=first, rhs=second, zero=\"fail\")",
        "return _core.div__duration_i64(lhs=length, rhs=among, zero=\"error\")",
        "return _core.floordiv__duration_duration(lhs=first, rhs=second, zero=\"error\")",
    ] {
        assert!(
            python.contains(expected),
            "the module has:\n{expected}\nin:\n{python}"
        );
    }
}

/// TYPE-2: a `date`, a `time`, a `datetime` and a `civil_datetime` are
/// Python's own, and an instant is a `datetime` in UTC that names no zone,
/// as hgraph has them. OPS-1: an instant and a duration, two instants, a
/// date and a time, and a date and a duration.
const INSTANTS: &str = concat!(
    "module demo.instants\n",
    "const opened: datetime = @2026-09-03T09:30:00Z\n",
    "const fn later(at: datetime, by: duration) -> datetime => at + by\n",
    "const fn earlier(at: datetime, by: duration) -> datetime => at - by\n",
    "const fn between(first: datetime, second: datetime) -> duration => second - first\n",
    "const fn on(day: date, at: time) -> civil_datetime => day + at\n",
    "const fn days(first: date, second: date) -> duration => second - first\n",
    "const fn after(day: date, by: duration) -> civil_datetime => day + by\n",
    "const fn before(day: date, by: duration) -> civil_datetime => day - by\n",
    "fn shifted(at: datetime, by: duration) -> datetime => at + by\n",
    "test instants_are_values_and_ticks {\n",
    "    assert later(opened, 30m) == @2026-09-03T10:00:00Z\n",
    "    assert earlier(opened, 1d) == @2026-09-02T09:30:00Z\n",
    "    assert between(opened, @2026-09-03T10:00:00Z) == 30m\n",
    // LEX-11: an instant written east of UTC is that far before what
    // its date and time say.
    "    assert @2026-09-03T10:30:00+01:00 == opened\n",
    "    assert on(@2026-09-03, @09:30) == @2026-09-03T09:30\n",
    "    assert days(@2026-09-01, @2026-09-03) == 2d\n",
    // OPS-10: a date and a duration give the civil datetime that far
    // from the start of the date, in either order.
    "    assert after(@2026-09-03, 2d) == @2026-09-05T00:00\n",
    "    assert after(@2026-09-03, 36h) == @2026-09-04T12:00\n",
    "    assert 36h + @2026-09-03 == @2026-09-04T12:00\n",
    "    assert before(@2026-09-03, 1h) == @2026-09-02T23:00\n",
    "    assert raises(\"numeric.out_of_range\") { after(@9999-12-31, 1d) }\n",
    // Core's named times (its CRUN-3) are constants of these types.
    "    assert earliest_start == never + smallest_step\n",
    "    assert latest_end == forever - smallest_step\n",
    "    assert eval(shifted(at: [opened], by: [1h])) == [@2026-09-03T10:30:00Z]\n",
    "}\n",
);

/// A temporal literal is written by its parts (LEX-11), and its type as
/// Python's (TYPE-2).
#[test]
fn a_date_a_time_and_an_instant_are_pythons_own() {
    let emitted = emit(&[("instants.hgl", INSTANTS)]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let python = emitted.text("_hgl/_m_demo/_m_instants/__init__.py");
    for expected in [
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
    ] {
        assert!(
            python.contains(expected),
            "the module has:\n{expected}\nin:\n{python}"
        );
    }
}

/// TST-10: "`assert raises(\"code\")` ... Run its block once. A matching
/// escaping error passes ...; then continue after the assertion."
const RAISES: &str = concat!(
    "module demo.raises\n",
    "const fn strict(a: f64, b: f64) -> f64 => a /{zero: .fail} b\n",
    "const fn checked(a: f64, b: f64) -> result<f64, error> => a /{zero: .error} b\n",
    "fn ratios(a: f64, b: f64) -> f64 {\n",
    "    when { return checked(a, b)? }\n",
    "}\n",
    "test a_failure_is_expected {\n",
    "    assert raises(\"numeric.division_by_zero\") { strict(1.0, 0.0) }\n",
    // A node's failure, which comes out of an eval.
    "    assert raises(\"numeric.division_by_zero\") {\n",
    "        eval(ratios(a: [6.0], b: [0.0]))\n",
    "    }\n",
    // The test goes on after the assertion.
    "    assert strict(6.0, 2.0) == 3.0\n",
    "}\n",
);

/// TST-10: "Normal completion, a different code ... fails the test."
/// TST-11: "A failed nested assertion cannot satisfy an enclosing
/// raises."
const RAISES_FAILS: &str = concat!(
    "module demo.raises.fails\n",
    "const fn strict(a: f64, b: f64) -> f64 => a /{zero: .fail} b\n",
    "const fn next(value: i64) -> i64 => value + 1\n",
    "const fn size(sizes: dict<str, i64>, name: str) -> i64 => sizes[name]\n",
    "test nothing_raised {\n",
    "    assert raises(\"numeric.division_by_zero\") { strict(6.0, 2.0) }\n",
    "}\n",
    "test another_code {\n",
    "    assert raises(\"numeric.division_by_zero\") { next(9223372036854775807) }\n",
    "}\n",
    "test nested_assertion {\n",
    "    assert raises(\"numeric.division_by_zero\") {\n",
    "        assert strict(6.0, 2.0) == 4.0\n",
    "    }\n",
    "}\n",
    // The inner assertion fails: it expects another code. What it lets
    // out has the code the outer one expects, and does not satisfy it.
    "test nested_raises {\n",
    "    assert raises(\"numeric.division_by_zero\") {\n",
    "        assert raises(\"value.missing_key\") { strict(1.0, 0.0) }\n",
    "    }\n",
    "}\n",
    // And one whose inner assertion holds goes on, and then fails as the
    // outer one expects.
    "test nested_then_raised {\n",
    "    assert raises(\"numeric.division_by_zero\") {\n",
    "        assert raises(\"value.missing_key\") { size([\"a\": 1], \"z\") }\n",
    "        strict(1.0, 0.0)\n",
    "    }\n",
    "}\n",
);

/// An `assert raises` is a `try` whose handler asks the harness whether
/// what escaped has the code, and whose `else` fails the test.
#[test]
fn an_assert_raises_is_a_try_the_harness_judges() {
    let emitted = emit(&[("raises.hgl", RAISES)]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let python = emitted.text("_hgl/_m_demo/_m_raises/__init__.py");
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
    let emitted = emit(&CONSTANTS);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let python = emitted.text("_hgl/_m_demo/_m_constants/__init__.py");
    for expected in [
        "@_core.constant\ndef scale():\n    return 2\n",
        "@_core.constant\ndef first():\n    return _core.add__i64_i64(lhs=_core.mul__i64_i64(lhs=3, rhs=late()), rhs=1)\n",
        "@_core.constant\ndef late():\n    return scale()\n",
        "@_core.constant\ndef side():\n    return Side[\"sell\"]\n",
        "def seed():\n    return first()\n",
    ] {
        assert!(
            python.contains(expected),
            "the module has:\n{expected}\nin:\n{python}"
        );
    }
    let reads = emitted.text("_hgl/_m_demo/_m_reads/__init__.py");
    for expected in [
        "@_core.constant\ndef twice():\n    return _core.mul__i64_i64(lhs=_hgl_4demo_9constants.scale(), rhs=2)\n",
        // In a graph a constant is a value known where it is built.
        "return scaled(value=value, factor=_hgl_4demo_9constants.scale())",
    ] {
        assert!(
            reads.contains(expected),
            "the module has:\n{expected}\nin:\n{reads}"
        );
    }
    // The provider has the function that runs one once. Its text is a
    // file of this repository, whose line ends are the checkout's.
    assert!(
        emitted
            .text("_hgl/_core_native.py")
            .contains("def constant(function):")
    );
}

/// A default that calls its own function with that argument left out
/// would be written inside itself without end: it is reported.
#[test]
fn a_default_is_not_written_inside_itself() {
    let emitted = emit(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "const fn again(value: i64 = if true { 1 } else { again() }) -> i64 => value\n",
            "const fn uses() -> i64 => again()\n",
        ),
    )]);
    assert_eq!(emitted.reported, ["m.hgl:2:29 target.unsupported"]);
}

/// Names that Python, hgraph or the target would take: each is given a
/// Python name of its own. FN-4: "Composition and eval choose the
/// temporal definition if one exists; runtime/value evaluation chooses
/// the value definition", so a pair of one name is two functions.
const NAMES: &str = concat!(
    "module demo.names\n",
    "\n",
    "const fn lambda() -> i64 => 1\n",
    "const fn lambda_() -> i64 => 2\n",
    "const fn graph() -> i64 => 3\n",
    "const fn _kept() -> i64 => 4\n",
    "const fn test_me() -> i64 => 5\n",
    "const fn float() -> i64 => 6\n",
    "const fn sized(frozenset: i64, int: i64) -> set<i64> => [frozenset, int]\n",
    "\n",
    "const fn doubled(value: i64) -> i64 => value * 2\n",
    "fn doubled(value: i64) -> i64 {\n",
    "    when { return doubled(value) + 1 }\n",
    "}\n",
    "\n",
    "const fn eight() -> i64 => doubled(4)\n",
    "\n",
    "fn pass(target: i64) -> i64 {\n",
    "    when { return target }\n",
    "}\n",
    "\n",
    "test names {\n",
    "    assert lambda() + lambda_() + graph() + _kept() + test_me() + float() == 21\n",
    "    assert eight() == 8\n",
    "    assert sized(1, 2) == sized(2, 1)\n",
    "    assert eval(doubled(value: [4])) == [9]\n",
    "    assert eval(pass(target: [1])) == [1]\n",
    "}\n",
);

#[test]
fn no_two_names_are_one_python_name() {
    let emitted = emit(&[("names.hgl", NAMES)]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let python = emitted.text("_hgl/_m_demo/_m_names/__init__.py");
    for expected in [
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
    ] {
        assert!(
            python.contains(expected),
            "the module has:\n{expected}\nin:\n{python}"
        );
    }
}

/// A module whose path begins another's, with a `var` written `T?`
/// that is given no value and a `start` with nothing in it.
const NESTED: [(&str, &str); 2] = [
    (
        "outer.hgl",
        concat!(
            "module demo\n",
            "use demo.util as u\n",
            "const fn kept() -> i64? {\n",
            "    var held: i64?\n",
            "    return held\n",
            "}\n",
            "fn idle(value: i64) -> i64 {\n",
            "    start { }\n",
            "    when { return u::one() }\n",
            "}\n",
            "test nested {\n",
            "    assert kept() == null\n",
            "    assert eval(idle(value: [7])) == [1]\n",
            "}\n",
        ),
    ),
    (
        "inner.hgl",
        "module demo.util\nexport const fn one() -> i64 => 1\n",
    ),
];

/// A module named as hgraph's own package is.
const SHADOW: &str = concat!(
    "module hgraph\n",
    "fn pass(value: i64) -> i64 {\n",
    "    when { return value }\n",
    "}\n",
    "test shadows {\n",
    "    assert eval(pass(value: [1])) == [1]\n",
    "}\n",
);

/// Every file is under the package `_hgl`, so a module named as hgraph
/// is, or as a module of Python's library is, takes no other's place.
#[test]
fn a_module_takes_no_other_packages_name() {
    let emitted = emit(&[("shadow.hgl", SHADOW)]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let python = emitted.text("_hgl/_m_hgraph/__init__.py");
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
            .text("_hgl/_core_native.py")
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
    (
        "one.hgl",
        "module a_.b\nexport const fn pick() -> i64 => 1\n",
    ),
    (
        "two.hgl",
        "module a._b\nexport const fn pick() -> i64 => 2\n",
    ),
    (
        "child.hgl",
        "module paths.util\nexport const fn pick() -> i64 => 4\n",
    ),
    (
        "parent.hgl",
        concat!(
            "module paths\n",
            "use a_.b as one\n",
            "use a._b as two\n",
            "use paths.util as child\n",
            "const fn util() -> i64 => 8\n",
            "const fn members() -> set<i64> => [1, 2, 1]\n",
            "const fn pair() -> set<i64> => [1, 2]\n",
            "test paths {\n",
            "    assert one::pick() + two::pick() + child::pick() + util() == 15\n",
            "    assert members() == pair()\n",
            "}\n",
        ),
    ),
];

#[test]
fn paths_and_declarations_stay_apart() {
    let emitted = emit(&PATHS);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let python = emitted.text("_hgl/_m_paths/__init__.py");
    for expected in [
        "import _hgl._m_a_._m_b as _hgl_2a__1b\n",
        "import _hgl._m_a._m__b as _hgl_1a_2_b\n",
        "import _hgl._m_paths._m_util as _hgl_5paths_4util\n",
        "def util():\n    return 8\n",
        "def members():\n    return _py.frozenset((1, 2, 1))\n",
    ] {
        assert!(
            python.contains(expected),
            "the module has:\n{expected}\nin:\n{python}"
        );
    }
}

/// Each module is a package, so one module's path may begin another's.
/// CTRL-11: a `var` written `T?` with no value is null. A `start` with
/// nothing in it is still a function.
#[test]
fn modules_nest_and_empty_forms_have_a_body() {
    let emitted = emit(&NESTED);
    assert_eq!(emitted.reported, Vec::<String>::new());
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
            "_hgl/_harness.py"
        ]
    );
    let python = emitted.text("_hgl/_m_demo/__init__.py");
    for expected in [
        "import _hgl._m_demo._m_util as _hgl_4demo_4util\n",
        "def kept():\n    held = None\n    return held\n",
        "@idle.start\ndef _start_idle():\n    pass\n",
        "    if not ((kept() is None)):\n",
        "        raise _py.AssertionError()\n",
    ] {
        assert!(
            python.contains(expected),
            "the module has:\n{expected}\nin:\n{python}"
        );
    }
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
        (
            "m.hgl",
            concat!(
                "module m\n",
                "export fn pass(value: i64) -> i64 {\n",
                "    when { return value }\n",
                "}\n",
                "export fn one() -> i64 => 1\n",
                "test bounded {\n",
                "    assert eval(pass(value: [7]), RunConfig()) == [7]\n",
                "}\n",
                // TST-17: a run in real time.
                "integration_test live {\n",
                "    assert run(pass(value: [7])) == [7]\n",
                "}\n",
            ),
        ),
        (
            "live.hgl",
            concat!(
                "application m.live\n",
                "use m::{one}\n",
                "const fn twice(value: i64) -> i64 => value * 2\n",
                "run(one())\n",
            ),
        ),
    ]);
    // The eval's configuration is written (RUN-1), and so are the run
    // in real time and the application.
    assert_eq!(emitted.reported, Vec::<String>::new());
    let module = emitted.text("_hgl/_m_m/__init__.py");
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
    assert_eq!(
        emitted.text("_hgl/_m_m/_m_live/__init__.py"),
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
const APPLICATIONS: [(&str, &str); 6] = [
    (
        "ticks.hgl",
        concat!(
            "module demo.ticks\n",
            // A source: an entry, with no temporal parameter.
            "export fn counted(const upto: i64, const every: duration = 1s) -> i64 {\n",
            "    cache count: i64 = 0\n",
            "    while count < upto {\n",
            "        count += 1\n",
            "        yield every: count\n",
            "    }\n",
            "}\n",
            // A graph over it, an entry too.
            "export fn doubled(const upto: i64) -> i64 {\n",
            "    return counted(upto) * 2\n",
            "}\n",
            // An entry with no result.
            "fn kept(value: i64) {\n",
            "    inject global_state\n",
            "    when { put(global_state, \"last\", value) }\n",
            "}\n",
            "export fn quiet(const upto: i64) {\n",
            "    kept(counted(upto))\n",
            "}\n",
            // An entry whose run fails.
            "fn inverse(value: i64) -> i64 {\n",
            "    when { return 6 // (value - 2) }\n",
            "}\n",
            "export fn failing() -> i64 {\n",
            "    return inverse(counted(3))\n",
            "}\n",
        ),
    ),
    (
        "backtest.hgl",
        concat!(
            "application demo.ticks.backtest\n",
            "use demo.ticks::{doubled}\n",
            "const rounds: i64 = 3\n",
            // RUN-3: `eval` is a simulation.
            "eval(doubled(upto: rounds), RunConfig(length: 10s))\n",
        ),
    ),
    (
        "live.hgl",
        concat!(
            "application demo.ticks.live\n",
            "use demo.ticks::{counted}\n",
            // RUN-3: `run` is in real time, here for a fifth of a
            // second.
            "run(counted(upto: 100, every: 50ms), RunConfig(length: 220ms))\n",
        ),
    ),
    (
        "endless.hgl",
        concat!(
            "application demo.ticks.endless\n",
            "use demo.ticks::{counted}\n",
            // No end: it runs until its host stops it.
            "run(counted(upto: 1000000, every: 20ms))\n",
        ),
    ),
    (
        "silent.hgl",
        concat!(
            "application demo.ticks.silent\n",
            "use demo.ticks::{quiet}\n",
            "eval(quiet(upto: 2))\n",
        ),
    ),
    (
        "broken.hgl",
        concat!(
            "application demo.ticks.broken\n",
            "use demo.ticks::{failing}\n",
            "eval(failing())\n",
        ),
    ),
];

/// An application's run statement is a function that runs its entry
/// (UNIT-14, RUN-1): the entry, core's `record` for what it publishes
/// (core's CRUN-8), the mode its word is (RUN-3), the entry's `const`
/// arguments by name, and the configuration.
#[test]
fn an_applications_run_statement_is_a_function_that_runs_its_entry() {
    let emitted = emit(&APPLICATIONS);
    assert_eq!(emitted.reported, Vec::<String>::new());
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
            ("demo.ticks.silent", false, false),
        ]
    );
    let backtest = emitted.text("_hgl/_m_demo/_m_ticks/_m_backtest/__init__.py");
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
    let live = emitted.text("_hgl/_m_demo/_m_ticks/_m_live/__init__.py");
    assert!(
        live.contains(concat!(
            "_harness.run(_hgl_4demo_5ticks.counted, _hgl_4core._inst_record__i64, True, ",
            "upto=100, every=_dt.timedelta(microseconds=50000), ",
        )),
        "{live}"
    );
    // An entry with no result is given no `record`, and a statement
    // with no configuration writes none.
    let silent = emitted.text("_hgl/_m_demo/_m_ticks/_m_silent/__init__.py");
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
        (
            "m.hgl",
            concat!(
                "module m\n",
                // `inject scheduler` is not written yet.
                "export fn waits() -> i64 {\n",
                "    inject scheduler\n",
                "    when { return 1 }\n",
                "}\n",
            ),
        ),
        (
            "app.hgl",
            concat!("application m.app\n", "use m::{waits}\n", "eval(waits())\n"),
        ),
    ]);
    let [application] = emitted.output.applications.as_slice() else {
        panic!("one application is written");
    };
    let blocked = application.blocked.as_ref().expect("it is blocked");
    assert_eq!(emitted.place(blocked.primary), "m.hgl:3:12");
}

/// Core's CRUN-2: "`start` is inclusive and `end` exclusive: no cycle
/// runs before `start`, and none at or after `end` ... `length` gives
/// the end counted from the start the run has, given or not".
const BOUNDED: &str = concat!(
    "module demo.bounded\n",
    "fn pass(value: i64) -> i64 {\n",
    "    when { return delta_value(value) }\n",
    "}\n",
    "const fn first(seen: list<tuple<datetime, i64>>) -> datetime => seen[0][0]\n",
    "const later: datetime = earliest_start + 10us\n",
    "test a_run_is_bounded {\n",
    "    assert eval(pass(value: [1, 2, 3, 4, 5]), RunConfig(length: 2us)) == [1, 2]\n",
    "    assert eval(pass(value: [1, 2, 3, 4, 5]), RunConfig(end: earliest_start + 3us)) == [1, 2, 3]\n",
    "    assert eval(pass(value: [1, 2, 3]), RunConfig()) == [1, 2, 3]\n",
    // A run that ends where it starts has no cycle.
    "    assert eval(pass(value: [1, 2, 3]), RunConfig(length: 0us)) == []\n",
    // TST-6: the first cell of a trace is at the run's start.
    "    assert first(eval(pass(value: [7]), RunConfig(start: later))) == later\n",
    "    assert first(eval(pass(value: [7]))) == earliest_start\n",
    "    assert eval(pass(value: [1, 2, 3]), RunConfig(start: later, length: 2us)) == [1, 2]\n",
    // A run may start at any instant: a trace's cells are counted from
    // its run's start, however far that is from the earliest one.
    "    let day: datetime = @2026-09-03T09:30:00Z\n",
    "    assert eval(pass(value: [1, _, 3]), RunConfig(start: day)) == [1, _, 3]\n",
    "    assert first(eval(pass(value: [7]), RunConfig(start: day))) == day\n",
    // A trace that is only given to a `const fn` is not written dense:
    // a publication a second after the start is a million cells on.
    "    assert first(eval(slow())) == earliest_start + 1s\n",
    "}\n",
    "fn slow() -> i64 {\n",
    "    yield 1s: 1\n",
    "}\n",
);

/// Core's CRUN-2: "A `RunConfig` with both an `end` and a `length` is
/// refused before the run's graph is built, an HGL error with code
/// `run.end_and_length`." And a dense trace too long to give: one has a
/// cell for each smallest step, and this source publishes a second
/// after its run starts.
const BOUNDED_FAILS: &str = concat!(
    "module demo.bounded.fails\n",
    "fn pass(value: i64) -> i64 {\n",
    "    when { return delta_value(value) }\n",
    "}\n",
    "fn slow() -> i64 {\n",
    "    yield 1s: 1\n",
    "}\n",
    "test both_ends {\n",
    "    eval(pass(value: [1]), RunConfig(end: earliest_start + 1us, length: 1us))\n",
    "}\n",
    "test too_dense {\n",
    "    assert eval(slow()) == [1]\n",
    "}\n",
);

/// An eval's configuration is given to the harness last, as `_config`:
/// after the call's arguments, as EXPR-13 evaluates them.
#[test]
fn an_evals_configuration_is_given_to_the_harness() {
    let emitted = emit(&[("bounded.hgl", BOUNDED)]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let python = emitted.text("_hgl/_m_demo/_m_bounded/__init__.py");
    for expected in [
        "_harness.evaluate(pass_, {\"value\": _hgl_4core._inst_replay__i64}, _hgl_4core._inst_record__i64, value=[1, 2, 3, 4, 5], _config=",
        "_harness.evaluate(pass_, {\"value\": _hgl_4core._inst_replay__i64}, _hgl_4core._inst_record__i64, value=[7])",
    ] {
        assert!(
            python.contains(expected),
            "the module has:\n{expected}\nin:\n{python}"
        );
    }
}

#[test]
fn a_form_not_written_yet_is_reported() {
    let emitted = emit(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "struct Quote {\n",
            "    bid: f64\n",
            "}\n",
            "fn gated(value: i64, gate: bool) -> i64 {\n",
            "    inject scheduler\n",
            "    when { return value }\n",
            "}\n",
            "fn maybe(value: i64?) -> i64 {\n",
            "    when { return 1 }\n",
            "}\n",
            "fn pass(value: i64) -> i64 {\n",
            "    when { return value }\n",
            "}\n",
            "fn summed(left: i64, right: i64) -> i64 => left + right\n",
            "fn stopped(value: i64) -> i64 {\n",
            "    inject out\n",
            "    when { return value }\n",
            "    stop { let last = out }\n",
            "}\n",
            "fn defaulted(value: i64 = 2) -> i64 {\n    when { return value }\n}\n",
            "const fn packed(rest: ...i64) -> i64 => 1\n",
            "const fn lone(value: i64) -> i64 => value\n",
            "test timed {\n",
            "    assert eval(pass(value: [0s: 7])) == [0s: 7]\n",
            "    let wired = pass(7)\n",
            "    assert eval(lone(value: [1])) == [1]\n",
            "}\n",
            "fn chosen(flag: bool, value: i64) -> i64 {\n",
            "    if flag {\n",
            "        return pass(value)\n",
            "    }\n",
            "    return value\n",
            "}\n",
            // A struct as a temporal input, and one made in a graph.
            "fn quoted(quote: Quote) -> f64 {\n",
            "    when { return quote.bid }\n",
            "}\n",
            "fn fixed(value: f64) -> f64 => quoted(Quote(bid: 1.0))\n",
            // A generic struct made where its arguments are the
            // function's own generic parameters.
            "struct Held<T> { value: T }\n",
            "const fn wrap<T>(value: T) -> Held<T> => Held(value: value)\n",
        ),
    )]);
    assert_eq!(
        emitted.reported,
        [
            "m.hgl:6:12 target.unsupported",
            "m.hgl:17:12 target.unsupported",
            "m.hgl:21:14 target.unsupported",
            "m.hgl:24:17 target.unsupported",
            // A graph's `if`, the three of a struct, then what the test
            // holds.
            "m.hgl:32:5 target.unsupported",
            "m.hgl:37:11 target.unsupported",
            "m.hgl:40:39 target.unsupported",
            "m.hgl:42:42 target.unsupported",
            "m.hgl:27:29 target.unsupported",
            "m.hgl:27:42 target.unsupported",
            "m.hgl:28:17 target.unsupported",
        ]
    );
}

/// Value functions and symbols given connections in graphs, and a
/// value function an eval names.
const LIFTS: &str = concat!(
    "module demo.lift\n",
    "\n",
    "const fn scale(value: f64, factor: f64 = 2.0) -> f64 => value * factor\n",
    "const fn doubled(value: i64) -> i64 => value * 2\n",
    "\n",
    "fn relay(value: i64) -> i64 {\n",
    "    when { return value }\n",
    "}\n",
    "\n",
    "fn summed(left: i64, right: i64) -> i64 => left + right\n",
    "fn scaled(price: f64, factor: f64) -> f64 => scale(price, factor)\n",
    "fn half_configured(price: f64) -> f64 => scale(price, 3.0)\n",
    "fn defaulted(price: f64) -> f64 => scale(price)\n",
    "fn nested(left: i64, right: i64) -> i64 => doubled(relay(left) + right) + 1\n",
    "fn twice(left: i64, right: i64) -> i64 => (left + right) + (left + right)\n",
    "fn positive(price: f64) -> bool => price > 0.0\n",
    "fn fixed(value: i64, const factor: i64) -> i64 => relay(value) + doubled(factor)\n",
    "const fn label(value: i64, text: str = \"{NAME}\") -> str => text\n",
    "fn labelled(value: i64) -> str => label(value)\n",
    "const fn note(value: i64) {}\n",
    "fn watched(value: i64) -> i64 {\n",
    "    note(value)\n",
    "    return value\n",
    "}\n",
    "\n",
    "test lifts {\n",
    "    assert eval(doubled(value: [1, _, 3])) == [2, _, 6]\n",
    "    assert eval(summed(left: [1, _, 3], right: [_, 10])) == [_, 11, 13]\n",
    "    assert eval(scaled(price: [1.0, _, 3.0, 3.0], factor: [_, 4.0])) ==\n",
    "        [2.0, 4.0, 12.0, 12.0]\n",
    "    assert eval(half_configured(price: [1.0, 2.0])) == [3.0, 6.0]\n",
    "    assert eval(defaulted(price: [1.5])) == [3.0]\n",
    "    assert eval(nested(left: [1, 2], right: [10])) == [23, 25]\n",
    "    assert eval(twice(left: [1, 2], right: [10])) == [22, 24]\n",
    "    assert eval(positive(price: [1.0, -1.0])) == [true, false]\n",
    "    assert eval(fixed(value: [1, 2], factor: 5)) == [11, 12]\n",
    "    assert eval(scale(value: [1.0, _, 3.0], factor: [_, 4.0])) == [2.0, 4.0, 12.0]\n",
    "    assert eval(watched(value: [1, 2])) == [1, 2]\n",
    "    assert eval(labelled(value: [1])) == [\"{NAME}\"]\n",
    "}\n",
);

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
    let emitted = emit(&[("lift.hgl", LIFTS)]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let python = emitted.text("_hgl/_m_demo/_m_lift/__init__.py");
    for expected in [
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
    ] {
        assert!(
            python.contains(expected),
            "the module has:\n{expected}\nin:\n{python}"
        );
    }
    assert_eq!(python.matches("def _lift_7_add__i64_i64(").count(), 1);
}

/// An operator of a program with two candidates, and arithmetic that
/// selects candidates core has an HGL body for.
const CANDIDATES: &str = concat!(
    "module demo.mixed\n",
    "\n",
    "operator blend_<L, R, O>(lhs: L, rhs: R) -> O\n",
    "\n",
    "impl const fn blend_(lhs: i64, rhs: i64) -> i64 => lhs * 10 + rhs\n",
    "impl const fn blend_(lhs: f64, rhs: i64) -> f64 => lhs * 10 + rhs\n",
    "\n",
    "fn mixed(count: i64, price: f64) -> f64 => count * price + 1\n",
    "fn blended(left: i64, right: i64) -> i64 => blend_(left, right)\n",
    "\n",
    "test mixes {\n",
    "    assert 1.5 + 1 == 2.5\n",
    "    assert 2.0 == 2\n",
    "    assert 2.5 != 2\n",
    "    assert blend_(1, 2) == 12\n",
    "    assert blend_(1.5, 2) == 17.0\n",
    "    assert eval(mixed(count: [2], price: [1.5])) == [4.0]\n",
    "    assert eval(blended(left: [1, 2], right: [3])) == [13, 23]\n",
    "}\n",
);

/// GEN-4: "An `impl fn` is a temporal candidate of the operator it
/// names and an `impl const fn` a value candidate." GEN-5: "A call in
/// composition selects among all of them ... There, a temporal
/// candidate outranks a value candidate of the same parameter types".
const TEMPORAL_CANDIDATES: [(&str, &str); 2] = [
    (
        "joins.hgl",
        concat!(
            "module demo.joins\n",
            "\n",
            "operator join_<L, R, O>(lhs: L, rhs: R) -> O\n",
            "\n",
            // A node, with state and a start.
            "impl fn join_(lhs: i64, rhs: i64) -> i64 {\n",
            "    state seen: i64 = 0\n",
            "    start { seen = 100 }\n",
            "    when valid(lhs, rhs) {\n",
            "        seen = seen + 1\n",
            "        return lhs + rhs + seen\n",
            "    }\n",
            "}\n",
            // A value candidate of the same parameter types.
            "impl const fn join_(lhs: i64, rhs: i64) -> i64 => lhs - rhs\n",
            // A graph, of the node.
            "impl fn join_(lhs: i64, rhs: bool) -> i64 => join_(lhs, lhs)\n",
            // A value candidate alone: lifted in a graph.
            "impl const fn join_(lhs: str, rhs: str) -> str => lhs + rhs\n",
            "\n",
            "export const fn plain(a: i64, b: i64) -> i64 => join_(a, b)\n",
        ),
    ),
    (
        "joined.hgl",
        concat!(
            "module demo.joined\n",
            "\n",
            "use demo.joins::{join_, plain}\n",
            "\n",
            "fn both(a: i64, b: i64) -> i64 => join_(a, b)\n",
            "fn flagged(a: i64, b: bool) -> i64 => join_(a, b)\n",
            "fn valued(a: i64, b: i64) -> i64 => plain(a, b)\n",
            "fn texts(a: str, b: str) -> str => join_(a, b)\n",
            "\n",
            "test candidates_are_called {\n",
            // The node: its sum, and what its state counts from 100.
            "    assert eval(both(a: [1, 2], b: [10])) == [112, 114]\n",
            "    assert eval(flagged(a: [1, 2], b: [true])) == [103, 106]\n",
            // A value function selects the value candidate.
            "    assert eval(valued(a: [1, 2], b: [10])) == [-9, -8]\n",
            "    assert eval(texts(a: [\"a\"], b: [\"b\"])) == [\"ab\"]\n",
            "}\n",
        ),
    ),
];

/// GEN-4: a temporal candidate with a body is a node or a graph as a
/// temporal function is, named as a value candidate is, by the
/// operator and the types of its parameters. A call in a graph that
/// selects it is a call of that node or graph, and is not lifted
/// (GEN-5, FN-6). The value candidate of the same parameter types is
/// `_value_impl_...`, as the value definition of a name with both is
/// `_value_name` (FN-4).
#[test]
fn a_temporal_candidate_is_a_node_or_a_graph() {
    let emitted = emit(&TEMPORAL_CANDIDATES);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let joins = emitted.text("_hgl/_m_demo/_m_joins/__init__.py");
    for expected in [
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
    ] {
        assert!(
            joins.contains(expected),
            "the module has:\n{expected}\nin:\n{joins}"
        );
    }
    let joined = emitted.text("_hgl/_m_demo/_m_joined/__init__.py");
    for expected in [
        // The candidate of another module, through that module.
        "    return _hgl_4demo_5joins._impl_join__i64_i64(lhs=a, rhs=b)\n",
        "    return _hgl_4demo_5joins._impl_join__i64_bool(lhs=a, rhs=b)\n",
        // A value candidate is lifted, and a temporal one is not.
        "    return _hgl_4demo_5joins._impl_join__str_str(lhs=lhs.value, rhs=rhs.value)\n",
    ] {
        assert!(
            joined.contains(expected),
            "the module has:\n{expected}\nin:\n{joined}"
        );
    }
    assert_eq!(joined.matches("def _lift_").count(), 2, "{joined}");
}

/// GEN-2: "Resolve every needed generic from actual inputs, requested
/// output, or solvable equality." A temporal function with generic
/// parameters, and core's temporal candidates of `+` and `-` under
/// `{invalid: .identity}` (OPS-9), each called with more than one type.
const GENERICS: &str = concat!(
    "module demo.generics\n",
    "\n",
    // Graphs of core's generic temporal candidates.
    "fn identity_sum<T>(lhs: T, rhs: T) -> T\n",
    "    requires add_(T, T) -> T => lhs +{invalid: .identity} rhs\n",
    "fn identity_difference<T>(lhs: T, rhs: T) -> T\n",
    "    requires sub_(T, T) -> T && neg_(T) -> T => lhs -{invalid: .identity} rhs\n",
    // A node, with an operation it requires in its hook.
    "fn doubled<T>(value: T) -> T requires add_(T, T) -> T {\n",
    "    when valid(value) { return add_(value, value) }\n",
    "}\n",
    // A graph of that node, twice.
    "fn quadrupled<T>(value: T) -> T requires add_(T, T) -> T => doubled(doubled(value))\n",
    // An operation a graph requires: the value candidate, lifted.
    "fn summed<T>(a: T, b: T) -> T requires add_(T, T) -> T {\n",
    "    let both = add_(a, b)\n",
    "    return both + both\n",
    "}\n",
    // A generic value, read in a hook.
    "fn scaled<const by: i64>(value: i64) -> i64 {\n",
    "    when valid(value) { return value * by }\n",
    "}\n",
    // State of the generic type, and configuration of it.
    "fn latest<T>(value: T, const fallback: T) -> T {\n",
    "    state held: T = fallback\n",
    "    when valid(value) {\n",
    "        let before = held\n",
    "        held = value\n",
    "        return before\n",
    "    }\n",
    "}\n",
    // A call from a graph with no generic parameter.
    "fn wired(a: i64, b: i64) -> i64 => identity_sum(a, b)\n",
    "fn tripled(value: i64) -> i64 => scaled<3>(value)\n",
    // One no call gives its types to.
    "fn unused<T>(value: T) -> T {\n",
    "    when valid(value) { return value }\n",
    "}\n",
    "\n",
    "test generics_are_instances {\n",
    "    assert eval(identity_sum(lhs: [_, 1.5], rhs: [0.5])) == [0.5, 2.0]\n",
    "    assert eval(identity_difference(lhs: [_, 1.5], rhs: [0.5])) == [-0.5, 1.0]\n",
    "    assert eval(identity_sum(lhs: [_, 5m], rhs: [2m])) == [2m, 7m]\n",
    "    assert eval(identity_sum(lhs: [\"\", _, \"a\"], rhs: [_, \"b\"])) == [\"\", \"b\", \"ab\"]\n",
    "    assert eval(wired(a: [1, 2], b: [10])) == [11, 12]\n",
    "    assert eval(doubled(value: [1, 2])) == [2, 4]\n",
    "    assert eval(doubled(value: [\"a\"])) == [\"aa\"]\n",
    "    assert eval(quadrupled(value: [1.5])) == [6.0]\n",
    "    assert eval(summed(a: [1, 2], b: [10])) == [22, 24]\n",
    "    assert eval(tripled(value: [2, 3])) == [6, 9]\n",
    "    assert eval(scaled<4>(value: [2])) == [8]\n",
    "    assert eval(latest(value: [1, 2], fallback: 0)) == [0, 1]\n",
    "    assert eval(latest(value: [\"a\", \"b\"], fallback: \"\")) == [\"\", \"a\"]\n",
    "}\n",
);

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
    let emitted = emit(&[("generics.hgl", GENERICS)]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    assert_eq!(emitted.blocked("generics_are_instances"), None);
    let python = emitted.text("_hgl/_m_demo/_m_generics/__init__.py");
    for expected in [
        // A graph for each type, of core's candidate for that type.
        concat!(
            "@_hg.graph\n",
            "def _inst_identity_sum__f64(lhs: _hg.TS[_py.float], rhs: _hg.TS[_py.float]) ",
            "-> _hg.TS[_py.float]:\n",
            "    return _hgl_4core._inst__impl_add__T_T__f64__identity(lhs=lhs, rhs=rhs)\n",
        ),
        "def _inst_identity_sum__duration(",
        "def _inst_identity_sum__str(",
        "    return _hgl_4core._inst__impl_sub__T_T__f64__identity(lhs=lhs, rhs=rhs)\n",
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
    ] {
        assert!(
            python.contains(expected),
            "the module has:\n{expected}\nin:\n{python}"
        );
    }
    // Nothing calls `unused`, and nothing is written for it.
    assert!(!python.contains("unused"), "{python}");
    // Core's candidate is written in core's file, once for each type.
    let core = emitted.text("_hgl/_m_core/__init__.py");
    for expected in [
        concat!(
            "@_hg.compute_node(valid=())\n",
            "def _inst__impl_add__T_T__f64__identity(lhs: _hg.TS[_py.float], ",
            "rhs: _hg.TS[_py.float], _output: _hg.TS_OUT[_py.float] = None) ",
            "-> _hg.TS[_py.float]:\n",
            "    if lhs.valid and rhs.valid:\n",
            "        return _core.add__f64_f64(lhs=lhs.value, rhs=rhs.value)\n",
        ),
        "        return _core.add__duration_duration(lhs=lhs.value, rhs=rhs.value)\n",
        "        return _core.neg__f64(value=rhs.value)\n",
    ] {
        assert!(
            core.contains(expected),
            "core has:\n{expected}\nin:\n{core}"
        );
    }
    assert_eq!(
        core.matches("def _inst__impl_add__T_T__").count(),
        4,
        "{core}"
    );
}

/// GEN-16: a value function with generic parameters that calls an
/// operation it requires, or hands a requirement on to what it calls.
const VALUE_GENERICS: &str = concat!(
    "module demo.vals\n",
    "\n",
    "operator join<L, R, O>(lhs: L, rhs: R) -> O\n",
    "impl const fn join(lhs: i64, rhs: i64) -> i64 => lhs * 10 + rhs\n",
    "impl const fn join(lhs: str, rhs: str) -> str => lhs + rhs\n",
    // A candidate that states a requirement of its own.
    "impl const fn join<T>(lhs: list<T>, rhs: list<T>) -> list<T>\n",
    "    requires join(T, T) -> T => [join(lhs[0], rhs[0])]\n",
    "\n",
    // A required operation, by name and by symbol.
    "const fn twice<T>(value: T) -> T requires join(T, T) -> T => join(value, value)\n",
    "const fn sum<T>(a: T, b: T) -> T requires add_(T, T) -> T => a + b\n",
    // One that hands its requirement on.
    "const fn outer<U>(value: U) -> U requires join(U, U) -> U => twice(twice(value))\n",
    // One with no requirement.
    "const fn same<T>(value: T) -> T => value\n",
    // One with no requirement of its own, which is written once, and
    // calls one that requires with whole arguments: nothing else asks
    // for `sum` of two `str`.
    "const fn paired<T>(value: T) -> str => sum(\"a\", \"b\")\n",
    "\n",
    "const hundred: i64 = sum(40, 60)\n",
    "\n",
    // In a node's hook, and lifted in a graph.
    "fn joined(value: i64) -> i64 {\n",
    "    when valid(value) { return twice(value) }\n",
    "}\n",
    "fn graphed(value: f64) -> f64 => sum(value, value)\n",
    "\n",
    "test values_are_instances {\n",
    "    assert twice(1) == 11\n",
    "    assert twice(\"a\") == \"aa\"\n",
    "    assert sum(1.5, 2.0) == 3.5\n",
    "    assert outer(1) == 121\n",
    "    assert same(\"x\") == \"x\"\n",
    "    assert paired(1) == \"ab\"\n",
    "    assert hundred == 100\n",
    "    assert twice([2]) == [22]\n",
    "    assert eval(joined(value: [1, 2])) == [11, 22]\n",
    "    assert eval(graphed(value: [1.5])) == [3.0]\n",
    // TST-8: a value function an eval names is lifted.
    "    assert eval(twice(value: [3])) == [33]\n",
    "}\n",
);

/// GEN-16: which candidate a required operation calls depends on the
/// types, so a value function that calls one, or hands a requirement
/// on, is written once for each set of generic arguments it is called
/// with, as a temporal one is (GEN-2). One with no requirement is
/// written once: Python takes a value of any type. An instance is
/// called wherever the function is: by a value function, a node's
/// hook, a constant, and the node that lifts it in a graph or an eval.
#[test]
fn a_value_function_that_requires_is_written_for_each_call() {
    let emitted = emit(&[("vals.hgl", VALUE_GENERICS)]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    assert_eq!(emitted.blocked("values_are_instances"), None);
    let python = emitted.text("_hgl/_m_demo/_m_vals/__init__.py");
    for expected in [
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
    ] {
        assert!(
            python.contains(expected),
            "the module has:\n{expected}\nin:\n{python}"
        );
    }
    assert!(!python.contains("def twice("), "{python}");
}

/// GEN-7: a requirement that names a generic parameter with nothing
/// stated is answered by a candidate that takes any one type. The
/// caller then states no requirement and hands none on, and still
/// calls a function that is written by instance, with its own generic
/// parameter.
const THROUGH: &str = concat!(
    "module demo.through\n",
    "\n",
    "operator pick<L, R, O>(lhs: L, rhs: R) -> O\n",
    "impl const fn pick<T>(lhs: T, rhs: T) -> T => lhs\n",
    "\n",
    "const fn first<T>(value: T) -> T requires pick(T, T) -> T => pick(value, value)\n",
    "const fn wrapper<U>(value: U) -> U => first(value)\n",
    "const fn outermost<V>(value: V) -> V => wrapper(value)\n",
    // GEN-13: a generic value read as a value, and handed on.
    "const fn width<const N: i64>() -> i64 => N\n",
    "const fn wider<const M: i64>() -> i64 => width<M>() + 1\n",
    "\n",
    "test a_caller_is_an_instance_too {\n",
    "    assert wrapper(1) == 1\n",
    "    assert outermost(\"a\") == \"a\"\n",
    "    assert width<3>() == 3\n",
    "    assert wider<4>() == 5\n",
    "}\n",
);

/// GEN-2: which instance such a caller calls is known only where its
/// own generic parameter is. So it is written by instance too, and so
/// is what calls it in turn with a generic parameter of its own.
/// GEN-13: a value function that reads a generic value parameter is
/// written for each constant it is given.
#[test]
fn a_caller_of_an_instance_with_its_own_parameter_is_an_instance() {
    let emitted = emit(&[("through.hgl", THROUGH)]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    assert_eq!(emitted.blocked("a_caller_is_an_instance_too"), None);
    let python = emitted.text("_hgl/_m_demo/_m_through/__init__.py");
    for expected in [
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
    ] {
        assert!(
            python.contains(expected),
            "the module has:\n{expected}\nin:\n{python}"
        );
    }
    assert!(!python.contains("def wrapper("), "{python}");
}

/// GEN-2: an instance is written in its declaration's module, with the
/// types of the module that calls it: here an enum of a module that
/// imports the declaration's.
const CALLERS_TYPE: [(&str, &str); 2] = [
    (
        "kept.hgl",
        concat!(
            "module demo.kept\n",
            "export fn held<T>(value: T) -> T {\n",
            "    when valid(value) { return value }\n",
            "}\n",
        ),
    ),
    (
        "sides.hgl",
        concat!(
            "module demo.sides\n",
            "use demo.kept::{held}\n",
            "enum Side { buy, sell }\n",
            "test an_instance_of_a_callers_type {\n",
            "    assert eval(held(value: [Side.buy, Side.sell])) == [Side.buy, Side.sell]\n",
            "}\n",
        ),
    ),
];

/// The two modules then import one another in Python, though they do
/// not in the source. A function's annotations are read where it is
/// defined, so a module defines its types before it imports another
/// generated module: whichever of the two is loaded first, the enum is
/// there when the instance that names it is defined.
#[test]
fn a_module_has_its_types_before_it_imports_another() {
    let emitted = emit(&CALLERS_TYPE);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let kept = emitted.text("_hgl/_m_demo/_m_kept/__init__.py");
    for expected in [
        "import _hgl._m_demo._m_sides as _hgl_4demo_5sides\n",
        concat!(
            "def _inst_held__demo_sides__Side(value: _hg.TS[_hgl_4demo_5sides.Side]) ",
            "-> _hg.TS[_hgl_4demo_5sides.Side]:\n",
        ),
    ] {
        assert!(
            kept.contains(expected),
            "the module has:\n{expected}\nin:\n{kept}"
        );
    }
    let sides = emitted.text("_hgl/_m_demo/_m_sides/__init__.py");
    let defined = sides
        .find("Side = _enum.Enum(")
        .expect("the enum is written");
    let imported = sides
        .find("import _hgl._m_demo._m_kept as _hgl_4demo_4kept\n")
        .expect("the module is imported");
    assert!(defined < imported, "{sides}");
}

/// Each instance is a piece of its module. Where one has a form the
/// target does not write, a type here, that is reported where the
/// declaration is written, and a test that reaches that instance is
/// not run. A test that reaches another instance of the declaration
/// is.
#[test]
fn an_instance_that_is_not_written_is_reported() {
    let emitted = emit(&[(
        "held.hgl",
        concat!(
            "module demo.held\n",
            "fn held<T>(value: T) -> T {\n",
            "    when valid(value) { return value }\n",
            "}\n",
            "fn again<T>(value: T) -> T => held(value)\n",
            "test whole {\n",
            "    assert eval(again(value: [1])) == [1]\n",
            "}\n",
            "test listed {\n",
            "    let seen = eval(again(value: [[1]]))\n",
            "}\n",
        ),
    )]);
    // The type of the input, at the parameter, and of the result, at
    // the function's name: once each for the one instance that has it,
    // of the node and of the graph that calls it. Then in core, where
    // the `replay` the eval's run is wired with for that type is
    // declared.
    assert_eq!(
        emitted.reported[..4],
        [
            "held.hgl:2:12 target.unsupported",
            "held.hgl:2:4 target.unsupported",
            "held.hgl:5:13 target.unsupported",
            "held.hgl:5:4 target.unsupported",
        ]
    );
    let [in_core] = &emitted.reported[4..] else {
        panic!("one more is reported: {:?}", emitted.reported);
    };
    assert!(
        in_core.starts_with("<core>/core/core.hgl:") && in_core.ends_with(" target.unsupported"),
        "{in_core}"
    );
    assert_eq!(emitted.blocked("whole"), None);
    let blocked = emitted.blocked("listed").expect("the test is not run");
    assert!(
        blocked.ends_with("the hgraph target does not write the type `list<i64>` yet"),
        "{blocked}"
    );
    let python = emitted.text("_hgl/_m_demo/_m_held/__init__.py");
    assert!(python.contains("def _inst_held__i64("), "{python}");
    assert!(python.contains("def _inst_again__i64("), "{python}");
    assert!(!python.contains("def _inst_held__list"), "{python}");
    assert!(!python.contains("def _inst_again__list"), "{python}");
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
    let emitted = emit(&[("mixed.hgl", CANDIDATES)]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let python = emitted.text("_hgl/_m_demo/_m_mixed/__init__.py");
    for expected in [
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
    ] {
        assert!(
            python.contains(expected),
            "the module has:\n{expected}\nin:\n{python}"
        );
    }
    let core = emitted.text("_hgl/_m_core/__init__.py");
    for expected in [
        "# Generated by the hgl compiler for hgraph, from module `core`. Do not edit.\n",
        concat!(
            "def _impl_add__f64_i64(lhs, rhs):\n",
            "    return _core.add__i64_f64(lhs=rhs, rhs=lhs)\n",
        ),
        concat!(
            "def _impl_eq__f64_i64_f64_f64(lhs, rhs, abs, rel):\n",
            "    return _core.eq__i64_f64_f64_f64(lhs=rhs, rhs=lhs, abs=abs, rel=rel)\n",
        ),
    ] {
        assert!(
            core.contains(expected),
            "core has:\n{expected}\nin:\n{core}"
        );
    }

    // A program that uses nothing core writes in HGL has no file of
    // core's: an eval would, for its `replay` and `record`.
    let emitted = emit(&[(
        "plain.hgl",
        concat!(
            "module demo.plain\n",
            "const fn doubled(value: i64) -> i64 => value * 2\n",
            "test plain {\n",
            "    assert doubled(2) == 4\n",
            "}\n",
        ),
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
/// report only where the program reaches it: here a candidate of core
/// whose body calls a native one the provider has no function for.
/// The report is core's own, at core's source.
#[test]
fn what_core_lacks_is_reported_where_a_program_reaches_it() {
    let emitted = emit(&[(
        "repeats.hgl",
        concat!(
            "module demo.repeats\n",
            // Core writes `i64 * str` in HGL, as `str * i64` turned
            // round, and the provider has no function for that one.
            "const fn twice(text: str) -> str => 2 * text\n",
            "const fn fine(value: i64) -> i64 => value + 1\n",
            "test doubles { assert fine(1) == 2 }\n",
            "test repeats { assert twice(\"ab\") == \"abab\" }\n",
        ),
    )]);
    assert_eq!(emitted.reported.len(), 1, "{:?}", emitted.reported);
    assert!(
        emitted.reported[0].starts_with("<core>/core/core.hgl:"),
        "{:?}",
        emitted.reported
    );
    assert_eq!(emitted.blocked("doubles"), None);
    let blocked = emitted
        .blocked("repeats")
        .expect("the test reaches core's candidate");
    assert!(
        blocked.ends_with("the hgraph target does not write the candidate `mul_(str, i64)` yet"),
        "{blocked}"
    );
}

/// Divisions, each with the zero policy it is given or takes by
/// default.
const DIVISION: &str = concat!(
    "module demo.div\n",
    "\n",
    "const fn ratio(a: f64, b: f64) -> f64 => a / b\n",
    "const fn nans(a: f64, b: f64) -> f64 => a /{zero: .nan} b\n",
    "const fn strict(a: f64, b: f64) -> f64 => a /{zero: .fail} b\n",
    "const fn named(a: f64, b: f64) -> f64 => div_(a, b, zero: ZeroPolicy.nan)\n",
    "const fn grouped(a: f64, b: f64) -> f64 => div_(a, b, zero: (ZeroPolicy.nan))\n",
    "const fn built(a: f64, b: f64) -> f64 => div_(a, b, zero: ZeroPolicy(\"nan\"))\n",
    "const fn floors(a: i64, b: i64) -> i64 => a // b\n",
    "const fn rest(a: i64, b: i64) -> i64 => a % b\n",
    "\n",
    "fn ratios(a: f64, b: f64) -> f64 => a / b\n",
    "fn halves(a: i64) -> f64 => a / 2\n",
    "\n",
    "test divides {\n",
    "    assert ratio(6.0, 4.0) == 1.5\n",
    "    assert 7 / 2 == 3.5\n",
    "    assert 7.0 / 2 == 3.5\n",
    "    assert 7 / 2.0 == 3.5\n",
    // NUM-3: a signed infinity, whose sign combines both operands'.
    "    assert ratio(1.0, 0.0) > 1.0e308\n",
    "    assert ratio(-1.0, 0.0) < -1.0e308\n",
    "    assert ratio(1.0, -0.0) < -1.0e308\n",
    // NaN is equal to nothing, itself included (NUM-5).
    "    assert ratio(0.0, 0.0) != ratio(0.0, 0.0)\n",
    "    assert nans(1.0, 0.0) != nans(1.0, 0.0)\n",
    "    assert nans(6.0, 4.0) == 1.5\n",
    "    assert named(1.0, 0.0) != named(1.0, 0.0)\n",
    "    assert grouped(1.0, 0.0) != grouped(1.0, 0.0)\n",
    "    assert built(1.0, 0.0) != built(1.0, 0.0)\n",
    "    assert strict(6.0, 4.0) == 1.5\n",
    // NUM-1: floor division rounds toward negative infinity, and a
    // modulo has the divisor's sign.
    "    assert floors(7, 2) == 3\n",
    "    assert floors(-7, 2) == -4\n",
    "    assert rest(7, 2) == 1\n",
    "    assert rest(-7, 2) == 1\n",
    "    assert rest(7, -2) == -1\n",
    "    assert 7.5 // 2 == 3.0\n",
    "    assert 7.5 % 2 == 1.5\n",
    "    assert -7.5 % 2 == 0.5\n",
    // NUM-12: what IEEE arithmetic makes of the zero divisor.
    "    assert 1.0 // 0.0 > 1.0e308\n",
    "    assert 1.0 % 0.0 != 1.0 % 0.0\n",
    // NUM-1: "Where an f64 operand is an infinity or NaN and the
    // divisor is not zero, `//` is the floor of the IEEE quotient and
    // `%` what IEEE `fmod` gives". An infinite quotient stays one, with
    // the sign of both operands, and has no remainder.
    "    assert (1.0 / 0.0) // 2.0 > 1.0e308\n",
    "    assert (1.0 / 0.0) // -2.0 < -1.0e308\n",
    "    assert (-1.0 / 0.0) // 2.0 < -1.0e308\n",
    "    assert (1.0 / 0.0) // (1.0 / 0.0) != (1.0 / 0.0) // (1.0 / 0.0)\n",
    "    assert (1.0 / 0.0) % 2.0 != (1.0 / 0.0) % 2.0\n",
    // A finite numerator over an infinity: a zero with the quotient's
    // sign, which a division by it shows, and "a remainder by an
    // infinity is the numerator as it is, whatever its sign".
    "    assert 1.0 // (1.0 / 0.0) == 0.0\n",
    "    assert 1.0 / (1.0 // (1.0 / 0.0)) > 1.0e308\n",
    "    assert 1.0 / (-1.0 // (1.0 / 0.0)) < -1.0e308\n",
    "    assert 1.0 % (1.0 / 0.0) == 1.0\n",
    "    assert -1.0 % (1.0 / 0.0) == -1.0\n",
    "    assert 1.0 % (-1.0 / 0.0) == 1.0\n",
    "    assert eval(ratios(a: [6.0, 1.0], b: [4.0])) == [1.5, 0.25]\n",
    "    assert eval(halves(a: [3])) == [1.5]\n",
    "}\n",
);

/// Divisions by zero under `fail`, given or taken by default.
const DIVISION_FAILS: &str = concat!(
    "module demo.div.fails\n",
    "const fn floors(a: i64, b: i64) -> i64 => a // b\n",
    "const fn strict(a: f64, b: f64) -> f64 => a /{zero: .fail} b\n",
    "test whole_by_zero { assert floors(1, 0) == 0 }\n",
    "test strict_by_zero { assert strict(1.0, 0.0) == 0.0 }\n",
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
    let emitted = emit(&[("div.hgl", DIVISION)]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let python = emitted.text("_hgl/_m_demo/_m_div/__init__.py");
    for expected in [
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
    ] {
        assert!(
            python.contains(expected),
            "the module has:\n{expected}\nin:\n{python}"
        );
    }
}

/// Results and errors in value functions, and a `?` in a `when`.
const RESULTS: &str = concat!(
    "module demo.results\n",
    "\n",
    "const fn checked(a: f64, b: f64) -> result<f64, error> => a /{zero: .error} b\n",
    "\n",
    "const fn halved(a: f64, b: f64) -> result<f64, error> {\n",
    "    let ratio = checked(a, b)?\n",
    "    return ok(ratio / 2)\n",
    "}\n",
    "\n",
    "const fn described(a: f64, b: f64) -> str {\n",
    "    let outcome = checked(a, b)\n",
    "    if is_ok(outcome) { return \"fine\" }\n",
    "    return outcome.error.message\n",
    "}\n",
    "\n",
    "const fn code_of(a: f64, b: f64) -> str? {\n",
    "    let outcome = checked(a, b)\n",
    "    if is_err(outcome) { return outcome.error.code }\n",
    "    return null\n",
    "}\n",
    "\n",
    "const fn value_or(a: f64, b: f64, fallback: f64) -> f64 {\n",
    "    let outcome = checked(a, b)\n",
    "    if is_ok(outcome) { return outcome.value }\n",
    "    return fallback\n",
    "}\n",
    "\n",
    "const fn failing(reason: str) -> result<i64, error> =>\n",
    "    err(error(message: reason, code: \"demo.failed\"))\n",
    "\n",
    "const fn plain(reason: str) -> error => error(message: reason)\n",
    "\n",
    "const fn fail(text: str) -> result<str, str> => err(text)\n",
    "const fn ordered() -> result<error, str> =>\n",
    "    ok(error(code: fail(\"code\")?, message: fail(\"message\")?))\n",
    "const fn met() -> str {\n",
    "    let outcome = ordered()\n",
    "    if is_err(outcome) { return outcome.error }\n",
    "    return \"none\"\n",
    "}\n",
    "\n",
    "fn ratios(a: f64, b: f64) -> f64 {\n",
    "    when { return checked(a, b)? }\n",
    "}\n",
    "\n",
    "test results {\n",
    "    assert value_or(6.0, 4.0, 0.0) == 1.5\n",
    // NUM-9: a zero divisor gives an `err`, and no other does.
    "    assert value_or(6.0, 0.0, -1.0) == -1.0\n",
    "    assert described(6.0, 4.0) == \"fine\"\n",
    "    assert described(6.0, 0.0) == \"a division by zero\"\n",
    "    assert code_of(6.0, 0.0) != null\n",
    "    assert code_of(6.0, 4.0) == null\n",
    "    assert plain(\"no\").code == null\n",
    // ERR-5: a failure leaves the function as the same error.
    "    assert is_ok(halved(6.0, 4.0))\n",
    "    assert is_err(halved(6.0, 0.0))\n",
    "    assert halved(6.0, 0.0) == checked(1.0, 0.0)\n",
    // ERR-2: equality compares the tag, then the payload.
    "    assert halved(6.0, 4.0) == halved(3.0, 2.0)\n",
    "    assert failing(\"no\") != failing(\"yes\")\n",
    "    assert failing(\"no\") == failing(\"no\")\n",
    "    assert plain(\"no\") == plain(\"no\")\n",
    // EXPR-2: the failure met first is the one written first.
    "    assert met() == \"code\"\n",
    "    assert eval(ratios(a: [6.0], b: [4.0])) == [1.5]\n",
    "}\n",
);

/// A node whose `?` meets a failure.
const RESULT_FAILS: &str = concat!(
    "module demo.results.fails\n",
    "const fn checked(a: f64, b: f64) -> result<f64, error> => a /{zero: .error} b\n",
    "fn ratios(a: f64, b: f64) -> f64 {\n",
    "    when { return checked(a, b)? }\n",
    "}\n",
    "test node_fails { eval(ratios(a: [6.0], b: [0.0])) }\n",
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
    let emitted = emit(&[("results.hgl", RESULTS)]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let python = emitted.text("_hgl/_m_demo/_m_results/__init__.py");
    for expected in [
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
    ] {
        assert!(
            python.contains(expected),
            "the module has:\n{expected}\nin:\n{python}"
        );
    }
    // A function with no `?` catches nothing.
    assert_eq!(python.matches("except _core.Propagated").count(), 2);
}

/// Numeric equality within a tolerance: core's observations for
/// NUM-4 and NUM-5.
const CLOSE: &str = concat!(
    "module demo.close\n",
    "\n",
    "const fn close(a: f64, b: f64, tolerance: f64) -> bool => a =={abs: tolerance} b\n",
    "const fn near(a: f64, b: f64, share: f64) -> bool => a =={rel: share} b\n",
    "const fn apart(a: f64, b: f64, tolerance: f64) -> bool => a !={abs: tolerance} b\n",
    "const fn whole(a: i64, b: i64, tolerance: f64) -> bool => a =={abs: tolerance} b\n",
    "const fn mixed(a: i64, b: f64, tolerance: f64) -> bool => a =={abs: tolerance} b\n",
    "\n",
    "fn closes(a: f64, b: f64) -> bool => a =={abs: 0.5} b\n",
    "\n",
    "test closeness {\n",
    "    assert !(1.0 == 1.005)\n",
    "    assert 1.0 =={abs: 0.01} 1.005\n",
    "    assert close(1.0, 1.005, 0.01)\n",
    "    assert !close(1.0, 1.005, 0.001)\n",
    // "Configured `!=` is the complement under the same options."
    "    assert apart(1.0, 1.005, 0.001)\n",
    "    assert !apart(1.0, 1.005, 0.01)\n",
    // NUM-2: an `i64` is not rounded to an `f64` to be compared.
    "    assert !(9007199254740993 == 9007199254740992.0)\n",
    // A relative tolerance under one never reaches zero.
    "    assert !near(1.0, 0.0, 0.5)\n",
    // NUM-5: the difference, and the tolerance scaled, are no `f64`.
    "    assert !near(1.0e308, -1.0e308, 1.9)\n",
    "    assert near(1.0e308, -1.0e308, 2.0)\n",
    // NUM-7: closeness is not transitive.
    "    assert close(0.0, 0.75, 1.0)\n",
    "    assert close(0.75, 1.5, 1.0)\n",
    "    assert !close(0.0, 1.5, 1.0)\n",
    // NUM-5: an infinity is close to itself alone, NaN to nothing,
    // and the two zeros are equal.
    "    assert close(1.0 / 0.0, 1.0 / 0.0, 1.0)\n",
    "    assert !close(1.0 / 0.0, -1.0 / 0.0, 1.0)\n",
    "    assert !close(0.0 / 0.0, 0.0 / 0.0, 1.0)\n",
    "    assert close(0.0, -0.0, 0.0)\n",
    "    assert whole(7, 9, 2.0)\n",
    "    assert !whole(7, 10, 2.0)\n",
    "    assert mixed(9007199254740993, 9007199254740992.0, 1.0)\n",
    "    assert !mixed(9007199254740993, 9007199254740992.0, 0.5)\n",
    "    assert eval(closes(a: [1.0, 2.0], b: [1.4])) == [true, false]\n",
    "}\n",
);

/// A tolerance that is no tolerance.
const CLOSE_FAILS: &str = concat!(
    "module demo.close.fails\n",
    "const fn close(a: f64, b: f64, tolerance: f64) -> bool => a =={abs: tolerance} b\n",
    "test negative_tolerance { assert close(1.0, 1.0, -1.0) }\n",
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
    let emitted = emit(&[("close.hgl", CLOSE)]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let python = emitted.text("_hgl/_m_demo/_m_close/__init__.py");
    for expected in [
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
    ] {
        assert!(
            python.contains(expected),
            "the module has:\n{expected}\nin:\n{python}"
        );
    }
}

/// Enums: members, constructions, and an enum as the type of an
/// input, an output and a configuration. `Side` is declared after the
/// function that first names it.
const ENUMS: &str = concat!(
    "module demo.enums\n",
    "\n",
    "const fn opposite(side: Side) -> Side {\n",
    "    if side == .buy { return .sell }\n",
    "    if side == Side.sell { return .buy }\n",
    "    return side\n",
    "}\n",
    "\n",
    "fn flipped(side: Side) -> Side => opposite(side)\n",
    "\n",
    "enum Side { buy, sell = 5, hold }\n",
    "enum Odd { mro, _x_, class, hgl_mro, name }\n",
    "\n",
    "const fn numbered(number: i64) -> Side => Side(number)\n",
    "const fn named(text: str) -> Side => Side(text)\n",
    "const fn odd(text: str) -> Odd => Odd(text)\n",
    "\n",
    "fn chosen(value: i64, const other: Side) -> Side {\n",
    "    when {\n",
    "        if value == 0 { return Side.buy }\n",
    "        return other\n",
    "    }\n",
    "}\n",
    "\n",
    "fn fixed(side: Side) -> bool => side == .hold\n",
    "\n",
    "test enums {\n",
    "    assert opposite(.buy) == .sell\n",
    "    assert opposite(Side.sell) == Side.buy\n",
    "    assert opposite(.hold) == .hold\n",
    // A member by its number, and by its exact name.
    "    assert Side(5) == .sell\n",
    "    assert Side(\"hold\") == Side(6)\n",
    "    assert numbered(0) == .buy\n",
    "    assert named(\"sell\") == .sell\n",
    // Names Python's `Enum` keeps for itself, and a keyword.
    "    assert odd(\"mro\") == .mro\n",
    "    assert odd(\"_x_\") == Odd._x_\n",
    "    assert odd(\"class\") == .class\n",
    "    assert odd(\"hgl_mro\") == .hgl_mro\n",
    "    assert odd(\"mro\") != odd(\"hgl_mro\")\n",
    "    assert odd(\"name\") == .name\n",
    "    assert eval(flipped(side: [.buy, _, .sell, .hold])) == [.sell, _, .buy, .hold]\n",
    "    assert eval(chosen(value: [0, 7], other: .hold)) == [.buy, .hold]\n",
    "    assert eval(fixed(side: [.buy, .hold])) == [false, true]\n",
    "}\n",
);

/// Constructions of a member from a value that is none of the enum's.
const ENUM_FAILS: &str = concat!(
    "module demo.enums.fails\n",
    "enum Side { buy, sell = 5, hold }\n",
    "const fn numbered(number: i64) -> Side => Side(number)\n",
    "const fn named(text: str) -> Side => Side(text)\n",
    "test no_number { assert numbered(9) == .buy }\n",
    "test no_name { assert named(\"none\") == .buy }\n",
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
    let emitted = emit(&[("enums.hgl", ENUMS)]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let python = emitted.text("_hgl/_m_demo/_m_enums/__init__.py");
    for expected in [
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
    ] {
        assert!(
            python.contains(expected),
            "the module has:\n{expected}\nin:\n{python}"
        );
    }
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
    (
        "structs.hgl",
        concat!(
            "module demo.structs\n",
            "\n",
            "export const fn origin() -> Point => Point(x: 0.0)\n",
            "\n",
            "const fn moved(point: Point, by: f64) -> Point {\n",
            "    var next = point\n",
            "    next.x = next.x + by\n",
            "    next.y += by\n",
            "    return next\n",
            "}\n",
            "\n",
            "const fn stretched(line: Line, by: f64) -> Line {\n",
            "    var next = line\n",
            "    next.end.x += by\n",
            "    next.class = 4\n",
            "    return next\n",
            "}\n",
            "\n",
            "const fn held(base: Base<str>) -> str => base.held\n",
            "const fn order() -> Order => Order(held: \"a\")\n",
            "const fn boxed(value: i64) -> Box<i64> => Box(value: value)\n",
            "\n",
            "impl const fn add_(lhs: Point, rhs: Point) -> Point => Point(x: lhs.x + rhs.x, y: lhs.y + rhs.y)\n",
            "\n",
            "enum Side { buy, sell }\n",
            "\n",
            "abstract struct Base<T> {\n",
            "    id: i64 = 1\n",
            "    held: T\n",
            "    rate: f64 = 2\n",
            "}\n",
            "\n",
            "struct Order : Base<str> {\n",
            "    id = 7\n",
            "    side: Side = .buy\n",
            "    note: str? = null\n",
            "}\n",
            "\n",
            "export struct Point {\n",
            "    x: f64\n",
            "    y: f64 = 0.0\n",
            "}\n",
            "\n",
            "struct Line {\n",
            "    start: Point\n",
            "    end: Point = Point(x: 1.0)\n",
            "    class: i64 = 3\n",
            "}\n",
            "\n",
            "struct Box<T> {\n",
            "    value: T\n",
            "    count: i64 = 0\n",
            "}\n",
            "\n",
            "struct Empty {}\n",
            "\n",
            "abstract struct Shape {}\n",
            "struct Tagged<T> : Shape { tag: T }\n",
            "struct Plain : Shape {}\n",
            "const fn same(lhs: Shape, rhs: Shape) -> bool => lhs == rhs\n",
            "\n",
            "fn travelled(step: f64) -> f64 {\n",
            "    state at: Point = Point(x: 0.0)\n",
            "    state seen: Box<i64> = boxed(0)\n",
            "    when {\n",
            "        at.x += step\n",
            "        at.y = at.x * 2.0\n",
            "        seen.count += 1\n",
            "        return at.x + at.y\n",
            "    }\n",
            "}\n",
            "\n",
            "test points {\n",
            "    let point = Point(y: 2.0, x: 1.0)\n",
            "    assert point.x == 1.0\n",
            "    assert point.y == 2.0\n",
            "    assert origin() == Point(x: 0.0, y: 0.0)\n",
            "    assert moved(point, 1.5) == Point(x: 2.5, y: 3.5)\n",
            "    assert point == Point(x: 1.0, y: 2.0)\n",
            "    assert point != origin()\n",
            "    assert point + Point(x: 3.0) == Point(x: 4.0, y: 2.0)\n",
            "}\n",
            "\n",
            "test lines {\n",
            "    let line = Line(start: origin())\n",
            "    assert line.end == Point(x: 1.0, y: 0.0)\n",
            "    assert line.class == 3\n",
            "    let longer = stretched(line, 2.0)\n",
            "    assert longer.end.x == 3.0\n",
            "    assert longer.class == 4\n",
            "    assert line.end.x == 1.0\n",
            "    assert longer.start == line.start\n",
            "}\n",
            "\n",
            "test orders {\n",
            "    let made = order()\n",
            "    assert made.id == 7\n",
            "    assert made.held == \"a\"\n",
            "    assert made.rate == 2.0\n",
            "    assert made.side == Side.buy\n",
            "    assert made.note == null\n",
            "    assert held(made) == \"a\"\n",
            "    assert Order(held: \"b\", note: \"n\", id: 9).id == 9\n",
            "    assert Order(held: \"b\", note: \"n\").note != null\n",
            "}\n",
            "\n",
            "test generic {\n",
            "    assert boxed(3) == Box(value: 3, count: 0)\n",
            "    assert boxed(3).count == 0\n",
            "    assert Empty() == Empty()\n",
            "}\n",
            "\n",
            "test identity {\n",
            "    assert same(Tagged(tag: 1), Tagged(tag: 1))\n",
            "    assert !same(Tagged(tag: 1), Tagged(tag: 1.0))\n",
            "    assert !same(Tagged(tag: 1), Plain())\n",
            "    let odd = Point(x: 0.0 / 0.0)\n",
            "    assert odd != odd\n",
            "    assert !(odd == odd)\n",
            "    assert Tagged(tag: [odd.x]) != Tagged(tag: [odd.x])\n",
            "}\n",
            "\n",
            "test walks {\n",
            "    assert eval(travelled(step: [1.0, _, 2.0])) == [3.0, _, 9.0]\n",
            "}\n",
        ),
    ),
    (
        "structs_user.hgl",
        concat!(
            "module demo.structs.user\n",
            "\n",
            "use demo.structs::{Point}\n",
            "use demo.structs as shapes\n",
            "\n",
            "const fn base() -> f64 => 5.0\n",
            "\n",
            "struct Marker {\n",
            "    at: Point = shapes::origin()\n",
            "    size: f64 = base()\n",
            "    label: str\n",
            "}\n",
            "\n",
            "const fn marker(label: str) -> Marker => Marker(label: label)\n",
            "\n",
            "test markers {\n",
            "    let made = Marker(label: \"a\")\n",
            "    assert made.size == 5.0\n",
            "    assert made.at == shapes::origin()\n",
            "    assert made == marker(\"a\")\n",
            "    assert marker(\"b\").label == \"b\"\n",
            "}\n",
        ),
    ),
];

/// A default that fails where a construction leaves its field out, and
/// two arguments that each fail.
const STRUCT_FAILS: &str = concat!(
    "module demo.structs.fails\n",
    "const fn broken() -> i64 => 1 // 0\n",
    "const fn next(value: i64) -> i64 => value + 1\n",
    "struct Ratio { value: i64 = broken() }\n",
    "struct Pair {\n",
    "    first: i64\n",
    "    second: i64\n",
    "}\n",
    "test given { assert Ratio(value: 2).value == 2 }\n",
    "test omitted { assert Ratio().value == 1 }\n",
    "test in_order { assert Pair(second: next(9223372036854775807), first: broken()).first == 1 }\n",
    "test unequal { assert Ratio(value: 2) == Ratio(value: 3) }\n",
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
/// functions of its module. Its values are never changed: an
/// assignment to a field gives the place's root a new value.
///
/// TYPE-3: "Generic applications are invariant by origin and complete
/// canonical arguments": a `Tagged<i64>` and a `Tagged<f64>` are of
/// two classes, and are not equal where they meet as values of their
/// parent. OPS-2: "Two structs are equal when they are of one struct
/// with the same generic arguments ... and each field is equal ...
/// NUM-5 holds inside a value: a struct, a list, a tuple, or a
/// dictionary that holds a NaN is not equal to itself."
#[test]
fn a_struct_is_a_class_whose_values_are_never_changed() {
    let emitted = emit(&STRUCTS);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let python = emitted.text("_hgl/_m_demo/_m_structs/__init__.py");
    for expected in [
        "import _hgl._core_native as _core\n",
        // Every field the struct has, a parent's first; a Python word
        // has a `_` after it.
        "Base = _core.struct(\"Base\", (\"id\", \"held\", \"rate\"))\n",
        "Order = _core.struct(\"Order\", (\"id\", \"held\", \"rate\", \"side\", \"note\"))\n",
        "Point = _core.struct(\"Point\", (\"x\", \"y\"))\n",
        "Line = _core.struct(\"Line\", (\"start\", \"end\", \"class_\"))\n",
        "Empty = _core.struct(\"Empty\", ())\n",
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
        // An assignment to a field, and to a field of a field.
        "    next = _core.with_field(next, x=_core.add__f64_f64(lhs=next.x, rhs=by))\n",
        concat!(
            "    next = _core.with_field(next, end=_core.with_field(next.end, ",
            "x=_core.add__f64_f64(lhs=next.end.x, rhs=by)))\n",
        ),
        "    next = _core.with_field(next, class_=4)\n",
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
        concat!(
            "    _state.seen = _core.with_field(_state.seen, ",
            "count=_core.add__i64_i64(lhs=_state.seen.count, rhs=1))\n",
        ),
    ] {
        assert!(
            python.contains(expected),
            "the module has:\n{expected}\nin:\n{python}"
        );
    }
    // The structs come before the function that is declared before
    // them.
    let at = |text: &str| python.find(text).expect("it is written");
    assert!(at("Point = _core.struct(") < at("def origin("));
    assert!(at("Empty = _core.struct(") < at("def origin("));

    // UNIT-7: "Imported defaults retain their defining closure,
    // including private references": a default is written where the
    // construction is, and names what it names through its module.
    let user = emitted.text("_hgl/_m_demo/_m_structs/_m_user/__init__.py");
    for expected in [
        "Marker = _core.struct(\"Marker\", (\"at\", \"size\", \"label\"))\n",
        "    return Marker(label=label, at=_hgl_4demo_7structs.origin(), size=base())\n",
    ] {
        assert!(
            user.contains(expected),
            "the module has:\n{expected}\nin:\n{user}"
        );
    }
}

/// Loops and `switch` in value functions and in a node's `when`, and
/// a dictionary written out.
const LOOPS: &str = concat!(
    "module demo.loops\n",
    "\n",
    "enum Side { buy, sell, hold }\n",
    "\n",
    "const fn total(items: list<i64>) -> i64 {\n",
    "    var sum = 0\n",
    "    for item in items { sum += item }\n",
    "    return sum\n",
    "}\n",
    "\n",
    "const fn largest(items: set<i64>) -> i64? {\n",
    "    var best: i64?\n",
    "    for item in items {\n",
    "        if best == null { best = item }\n",
    "    }\n",
    "    return best\n",
    "}\n",
    "\n",
    "const fn weighted(weights: dict<str, f64>) -> f64 {\n",
    "    var sum = 0.0\n",
    "    for name, weight in weights {\n",
    "        if name != \"skip\" { sum += weight }\n",
    "    }\n",
    "    return sum\n",
    "}\n",
    "\n",
    "const fn halvings(first: i64) -> i64 {\n",
    "    var value = first\n",
    "    var count = 0\n",
    "    while value > 1 {\n",
    "        value = value // 2\n",
    "        count += 1\n",
    "    }\n",
    "    return count\n",
    "}\n",
    "\n",
    "const fn sign(side: Side) -> i64 {\n",
    "    switch side {\n",
    "        case .buy: return 1\n",
    "        case Side.sell: return -1\n",
    "        case .hold: return 0\n",
    "    }\n",
    "}\n",
    "\n",
    "const fn named(number: i64) -> str {\n",
    "    var text: str\n",
    "    switch number {\n",
    "        case 0: text = \"zero\"\n",
    "        case 1:\n",
    "            let one = \"one\"\n",
    "            text = one\n",
    "        default: text = \"many\"\n",
    "    }\n",
    "    return text\n",
    "}\n",
    "\n",
    "const fn strict(text: str) -> i64 {\n",
    "    switch text {\n",
    "        case \"a\": return 1\n",
    "        case \"b\": return 2\n",
    "    }\n",
    "    return 0\n",
    "}\n",
    "\n",
    "const fn nested(flag: bool, number: i64) -> i64 {\n",
    "    switch flag {\n",
    "        case true:\n",
    "            switch number {\n",
    "                case 1: return 10\n",
    "                default: return 11\n",
    "            }\n",
    "        default: return 20\n",
    "    }\n",
    "}\n",
    "\n",
    "const fn only(number: i64) -> i64 {\n",
    "    switch number {\n",
    "        default: return number + 1\n",
    "    }\n",
    "}\n",
    "\n",
    "fn running(value: i64) -> i64 {\n",
    "    state seen: i64 = 0\n",
    "    when {\n",
    "        var steps = 0\n",
    "        while steps < value {\n",
    "            steps += 1\n",
    "            seen += 1\n",
    "        }\n",
    "        switch value {\n",
    "            case 0: return 0\n",
    "            default: return seen\n",
    "        }\n",
    "    }\n",
    "}\n",
    "\n",
    "test loops {\n",
    "    assert total([1, 2, 3]) == 6\n",
    "    assert total([]) == 0\n",
    "    assert weighted([\"a\": 1.5, \"skip\": 9.0, \"b\": 2.0]) == 3.5\n",
    "    assert halvings(16) == 4\n",
    "    assert halvings(1) == 0\n",
    "}\n",
    "\n",
    "test switches {\n",
    "    assert sign(.buy) == 1\n",
    "    assert sign(.sell) == -1\n",
    "    assert sign(.hold) == 0\n",
    "    assert named(0) == \"zero\"\n",
    "    assert named(1) == \"one\"\n",
    "    assert named(7) == \"many\"\n",
    "    assert strict(\"b\") == 2\n",
    "    assert nested(true, 1) == 10\n",
    "    assert nested(true, 5) == 11\n",
    "    assert nested(false, 1) == 20\n",
    "    assert only(4) == 5\n",
    "    assert eval(running(value: [2, 0, 3])) == [2, 0, 5]\n",
    "}\n",
);

/// A `switch` with no `default` that no case matches.
const LOOP_FAILS: &str = concat!(
    "module demo.loops.fails\n",
    "const fn strict(text: str) -> i64 {\n",
    "    switch text {\n",
    "        case \"a\": return 1\n",
    "        case \"b\": return 2\n",
    "    }\n",
    "    return 0\n",
    "}\n",
    "test unmatched { assert strict(\"z\") == 0 }\n",
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
    let emitted = emit(&[("loops.hgl", LOOPS)]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let python = emitted.text("_hgl/_m_demo/_m_loops/__init__.py");
    for expected in [
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
    ] {
        assert!(
            python.contains(expected),
            "the module has:\n{expected}\nin:\n{python}"
        );
    }
}

/// Two modules that each declare an enum `Side`, with other members,
/// and core's `CmpResult`, which has the name of an enum of hgraph's
/// own, each as the type of a node's input and output.
const SAME_NAMES: [(&str, &str); 2] = [
    (
        "left.hgl",
        concat!(
            "module demo.sides.left\n",
            "enum Side { buy, sell }\n",
            "fn kept(side: Side) -> Side {\n",
            "    when { return side }\n",
            "}\n",
            "test left_sides { assert eval(kept(side: [.buy, .sell])) == [.buy, .sell] }\n",
        ),
    ),
    (
        "right.hgl",
        concat!(
            "module demo.sides.right\n",
            "enum Side { up = 5, down }\n",
            "fn kept(side: Side) -> Side {\n",
            "    when { return side }\n",
            "}\n",
            "fn outcome(value: CmpResult) -> CmpResult {\n",
            "    when { return value }\n",
            "}\n",
            "test right_sides {\n",
            "    assert eval(kept(side: [.up, .down])) == [.up, .down]\n",
            "    assert eval(outcome(value: [.less, .greater, .unordered])) == \
             [.less, .greater, .unordered]\n",
            "}\n",
        ),
    ),
];

/// UNIT-4: a type is known by where it is declared, so two modules'
/// enums of one name are two types. hgraph knows a Python enum by its
/// class's name, and so the class is named with its module's path:
/// two `Side`s stay two, and core's `CmpResult` is not hgraph's.
#[test]
fn an_enums_class_is_named_with_its_module() {
    let emitted = emit(&SAME_NAMES);
    assert_eq!(emitted.reported, Vec::<String>::new());
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
const ELEMENTS: &str = concat!(
    "module demo.elements\n",
    "\n",
    "struct Book {\n",
    "    levels: list<f64>\n",
    "    sizes: dict<str, i64> = []\n",
    "}\n",
    "\n",
    "const fn second(items: list<i64>) -> i64 => items[1]\n",
    "const fn last(pair: tuple<i64, str>) -> str => pair[1]\n",
    "const fn size(sizes: dict<str, i64>, name: str) -> i64 => sizes[name]\n",
    "const fn known(sizes: dict<str, i64>, name: str) -> i64? => get(sizes, name)\n",
    "const fn known_or(sizes: dict<str, i64>, name: str, fallback: i64) -> i64 {\n",
    "    let found = get(sizes, name)\n",
    "    if found == null { return fallback }\n",
    "    return found\n",
    "}\n",
    "\n",
    "const fn doubled(items: list<i64>) -> list<i64> {\n",
    "    var next = items\n",
    "    next[0] = next[0] * 2\n",
    "    next[1] += 1\n",
    "    return next\n",
    "}\n",
    "\n",
    "const fn renamed(pair: tuple<i64, str>) -> tuple<i64, str> {\n",
    "    var next = pair\n",
    "    next[1] = \"b\"\n",
    "    return next\n",
    "}\n",
    "\n",
    "const fn counted(names: list<str>) -> dict<str, i64> {\n",
    "    var seen: dict<str, i64> = []\n",
    "    for name in names {\n",
    "        let before = get(seen, name)\n",
    "        if before == null {\n",
    "            seen[name] = 1\n",
    "        } else {\n",
    "            seen[name] = before + 1\n",
    "        }\n",
    "    }\n",
    "    return seen\n",
    "}\n",
    "\n",
    "const fn lifted(book: Book, by: f64) -> Book {\n",
    "    var next = book\n",
    "    next.levels[0] += by\n",
    "    next.sizes[\"top\"] = 1\n",
    "    return next\n",
    "}\n",
    "\n",
    "const fn grid() -> list<list<i64>> {\n",
    "    var rows = [[1, 2], [3, 4]]\n",
    "    rows[1][0] = 9\n",
    "    return rows\n",
    "}\n",
    "\n",
    "fn tallied(name: str) -> i64 {\n",
    "    state seen: dict<str, i64> = []\n",
    "    when {\n",
    "        let before = get(seen, name)\n",
    "        if before == null {\n",
    "            seen[name] = 1\n",
    "        } else {\n",
    "            seen[name] = before + 1\n",
    "        }\n",
    "        return seen[name]\n",
    "    }\n",
    "}\n",
    "\n",
    "test reads {\n",
    "    assert second([4, 5, 6]) == 5\n",
    "    assert last((1, \"a\")) == \"a\"\n",
    "    assert size([\"a\": 1, \"b\": 2], \"b\") == 2\n",
    "    assert known([\"a\": 1], \"a\") != null\n",
    "    assert known([\"a\": 1], \"z\") == null\n",
    "    assert known_or([\"a\": 1], \"a\", 7) == 1\n",
    "    assert known_or([\"a\": 1], \"z\", 7) == 7\n",
    "}\n",
    "\n",
    "test writes {\n",
    "    let items = [1, 2, 3]\n",
    "    assert doubled(items) == [2, 3, 3]\n",
    "    assert items == [1, 2, 3]\n",
    "    assert renamed((1, \"a\")) == (1, \"b\")\n",
    "    assert counted([\"a\", \"b\", \"a\"]) == [\"a\": 2, \"b\": 1]\n",
    "    let book = Book(levels: [1.0, 2.0])\n",
    "    let moved = lifted(book, 0.5)\n",
    "    assert moved.levels == [1.5, 2.0]\n",
    "    assert moved.sizes == [\"top\": 1]\n",
    "    assert book.levels == [1.0, 2.0]\n",
    "    assert grid() == [[1, 2], [9, 4]]\n",
    "    assert eval(tallied(name: [\"a\", \"b\", \"a\"])) == [1, 1, 2]\n",
    "}\n",
);

/// Reads and an assignment outside a list, and a key a dictionary has
/// not.
const ELEMENT_FAILS: &str = concat!(
    "module demo.elements.fails\n",
    "const fn at(items: list<i64>, index: i64) -> i64 => items[index]\n",
    "const fn size(sizes: dict<str, i64>, name: str) -> i64 => sizes[name]\n",
    "const fn placed(items: list<i64>, index: i64) -> list<i64> {\n",
    "    var next = items\n",
    "    next[index] = 0\n",
    "    return next\n",
    "}\n",
    "test beyond { assert at([1, 2], 2) == 0 }\n",
    "test before { assert at([1, 2], -1) == 2 }\n",
    "test absent { assert size([\"a\": 1], \"z\") == 0 }\n",
    "test outside { assert placed([1, 2], 5) == [1, 2] }\n",
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
    let emitted = emit(&[("elements.hgl", ELEMENTS)]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let python = emitted.text("_hgl/_m_demo/_m_elements/__init__.py");
    for expected in [
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
    ] {
        assert!(
            python.contains(expected),
            "the module has:\n{expected}\nin:\n{python}"
        );
    }
}

/// `<=>` on numbers, strings and in a graph, and the Boolean operators
/// called by name.
const COMPARE: &str = concat!(
    "module demo.compare\n",
    "\n",
    "const fn ordered(lhs: f64, rhs: f64) -> CmpResult => lhs <=> rhs\n",
    "const fn mixed(lhs: i64, rhs: f64) -> CmpResult => lhs <=> rhs\n",
    "const fn words(lhs: str, rhs: str) -> CmpResult => lhs <=> rhs\n",
    "const fn both(lhs: bool, rhs: bool) -> bool => and_(lhs, rhs)\n",
    "const fn either(lhs: bool, rhs: bool) -> bool => or_(lhs, rhs)\n",
    "const fn neither(value: bool) -> bool => not_(value)\n",
    "\n",
    "fn outcome(lhs: f64, rhs: f64) -> CmpResult => lhs <=> rhs\n",
    "\n",
    "test outcomes {\n",
    "    assert ordered(1.0, 2.0) == .less\n",
    "    assert ordered(2.0, 1.0) == .greater\n",
    "    assert ordered(0.0, -0.0) == .equivalent\n",
    "    assert ordered(0.0 / 0.0, 1.0) == .unordered\n",
    "    assert ordered(0.0 / 0.0, 0.0 / 0.0) == CmpResult.unordered\n",
    "    assert mixed(9007199254740993, 9007199254740992.0) == .greater\n",
    "    assert mixed(1, 1.0) == .equivalent\n",
    "    assert words(\"a\", \"b\") == .less\n",
    "    assert (2 <=> 1) == .greater\n",
    "    assert both(true, false) == false\n",
    "    assert either(true, false)\n",
    "    assert neither(false)\n",
    "    assert eval(outcome(lhs: [1.0, 2.0, 3.0], rhs: [2.0, 2.0, 2.0])) == [.less, .equivalent, .greater]\n",
    "}\n",
);

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
    let emitted = emit(&[("compare.hgl", COMPARE)]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let python = emitted.text("_hgl/_m_demo/_m_compare/__init__.py");
    for expected in [
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
    ] {
        assert!(
            python.contains(expected),
            "the module has:\n{expected}\nin:\n{python}"
        );
    }
}

/// A node whose `stop` reads an input under its guard, and one whose
/// `stop` would fail where the guard holds, given an input that never
/// is valid.
const STOPS: &str = concat!(
    "module demo.stops\n",
    "\n",
    "fn last_seen(value: i64, const scale: i64) -> i64 {\n",
    "    state last: i64 = 0\n",
    "    state ended: i64 = 0\n",
    "    when {\n",
    "        last = value\n",
    "        return value * scale\n",
    "    }\n",
    "    stop {\n",
    "        if valid(value) { ended = value * scale }\n",
    "    }\n",
    "}\n",
    "\n",
    "fn ends_badly(value: i64) -> i64 {\n",
    "    state ended: i64 = 0\n",
    "    when { return value }\n",
    "    stop {\n",
    "        if valid(value) { ended = value // 0 }\n",
    "    }\n",
    "}\n",
    "\n",
    "test stops {\n",
    "    assert eval(last_seen(value: [1, 2, 3], scale: 2)) == [2, 4, 6]\n",
    "    assert eval(last_seen(value: [_, _], scale: 2)) == []\n",
    // The input is never valid: the guard does not hold, and the
    // division is not made.
    "    assert eval(ends_badly(value: [_, _])) == []\n",
    "}\n",
);

/// TIME-6: a `cache`. TIME-8, core's CAP-1 to CAP-3 and CAP-6: a node
/// injects `logger`, `clock` and `global_state`, reads a capability's
/// properties, gives one to a function, and reads and writes the store.
const CAPABILITIES: &str = concat!(
    "module demo.caps\n",
    "\n",
    "fn counted(value: i64) -> i64 {\n",
    "    cache seen: i64 = 0\n",
    "    inject logger\n",
    "    when {\n",
    "        seen += 1\n",
    // A `%` in a message is text.
    "        log_info(logger, \"seen 100% of it\")\n",
    "        return seen\n",
    "    }\n",
    "    stop { if seen > 1 { log(logger, .debug, \"more than one\") } }\n",
    "}\n",
    "\n",
    "fn stamped(value: i64) -> datetime {\n",
    "    inject clock\n",
    "    when { return clock.evaluation_time }\n",
    "}\n",
    "\n",
    "fn next_step(value: i64) -> duration {\n",
    "    inject clock\n",
    "    when { return clock.next_cycle_evaluation_time - clock.evaluation_time }\n",
    "}\n",
    "\n",
    "fn remembered(value: i64) -> i64 {\n",
    "    inject global_state\n",
    "    when {\n",
    "        let before: i64 =\n",
    "            if has(global_state, \"last\") { get<i64>(global_state, \"last\") } else { 0 }\n",
    "        put(global_state, \"last\", value)\n",
    "        return before\n",
    "    }\n",
    "}\n",
    "\n",
    "fn levelled(value: i64) -> bool {\n",
    "    inject logger\n",
    "    when { return logger.level == .debug || logger.level != .debug }\n",
    "}\n",
    "\n",
    "test capabilities {\n",
    "    assert eval(counted(value: [7, _, 7])) == [1, _, 2]\n",
    "    assert eval(stamped(value: [1, _, 3])) == [earliest_start, _, earliest_start + 2us]\n",
    "    assert eval(next_step(value: [1])) == [1us]\n",
    // CRUN-7: a run's store is empty when its graph is built.
    "    assert eval(remembered(value: [5, 6, _, 7])) == [0, 5, _, 6]\n",
    "    assert eval(remembered(value: [8])) == [0]\n",
    "    assert eval(levelled(value: [1])) == [true]\n",
    "}\n",
);

/// A node that reads a key the store has not.
const CAPABILITIES_FAIL: &str = concat!(
    "module demo.caps.fails\n",
    "fn absent(value: i64) -> i64 {\n",
    "    inject global_state\n",
    "    when { return get<i64>(global_state, \"nothing\") }\n",
    "}\n",
    "test an_absent_key_fails_the_node {\n",
    "    let seen = eval(absent(value: [1]))\n",
    "}\n",
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
    let emitted = emit(&[("caps.hgl", CAPABILITIES)]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let python = emitted.text("_hgl/_m_demo/_m_caps/__init__.py");
    for expected in [
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
    ] {
        assert!(
            python.contains(expected),
            "the module has:\n{expected}\nin:\n{python}"
        );
    }
    // Core's `log_info` is a function of core's module, which calls
    // the provider's `log`.
    let core = emitted.text("_hgl/_m_core/__init__.py");
    assert!(
        core.contains(concat!(
            "def log_info(logger, message):\n",
            "    _core.fn_log(logger=logger, level=LogLevel[\"info\"], message=message)\n",
        )),
        "{core}"
    );
}

/// TIME-8: `scheduler` and `alarm` are not written yet, and are
/// reported where they are injected.
#[test]
fn a_capability_that_is_not_written_is_reported() {
    let emitted = emit(&[(
        "wakes.hgl",
        concat!(
            "module demo.wakes\n",
            "fn woken(value: i64) -> i64 {\n",
            "    inject scheduler\n",
            "    when {\n",
            "        schedule(scheduler, 1s)\n",
            "        return value\n",
            "    }\n",
            "}\n",
            "test wakes {\n",
            "    let seen = eval(woken(value: [1]))\n",
            "}\n",
        ),
    )]);
    assert!(
        emitted
            .reported
            .contains(&"wakes.hgl:3:12 target.unsupported".to_string()),
        "{:?}",
        emitted.reported
    );
    let blocked = emitted.blocked("wakes").expect("the test is not run");
    assert!(
        blocked.ends_with("the hgraph target does not write an `inject` of `scheduler` yet"),
        "{blocked}"
    );
}

/// SOURCE-1 to SOURCE-4: sources. One with no declaration and no hook;
/// one with both hooks and its yields in a block; one that keeps a
/// `cache` from one `yield` to the next; targets that are durations and
/// instants; and a source wired into a graph.
const SOURCES: &str = concat!(
    "module demo.sources\n",
    "\n",
    "fn two_values() -> i64 {\n",
    "    yield 1us: 7\n",
    "    yield 1us: 9\n",
    "}\n",
    "\n",
    "fn greeting(const every: duration = 1us) -> str {\n",
    "    inject logger\n",
    "    start { log_info(logger, \"greeting starts\") }\n",
    "    {\n",
    "        yield every: \"hello\"\n",
    "        yield every: \"world\"\n",
    "    }\n",
    "    stop { log_info(logger, \"greeting ends\") }\n",
    "}\n",
    "\n",
    "fn counter(const every: duration = 1us, const limit: i64 = 3) -> i64 {\n",
    "    inject logger\n",
    "    cache count: i64 = 0\n",
    "    while count < limit {\n",
    "        count += 1\n",
    "        yield every: count\n",
    "    }\n",
    "    stop { if count > 0 { log_info(logger, \"counted\") } }\n",
    "}\n",
    "\n",
    // SOURCE-3: "a current target publishes now".
    "fn at_once(const first: i64) -> i64 {\n",
    "    yield 0us: first\n",
    "    yield 2us: first + 1\n",
    "}\n",
    "\n",
    // SOURCE-2: "A duration is relative to current evaluation time; a
    // datetime is absolute." The clock is read where the source is
    // resumed.
    "fn absolute() -> i64 {\n",
    "    inject clock\n",
    "    {\n",
    "        yield clock.evaluation_time + 3us: 1\n",
    "        yield earliest_start + 5us: 2\n",
    "        yield clock.evaluation_time + 1us: 3\n",
    "    }\n",
    "}\n",
    "\n",
    // SOURCE-3: "A past absolute target is skipped".
    "fn late() -> i64 {\n",
    "    yield earliest_start + 1us: 1\n",
    "    yield earliest_start + 4us: 2\n",
    "}\n",
    "\n",
    // SOURCE-4: "A bare return ... finishes the source", and an
    // unbounded `while`.
    "fn ended(const limit: i64) -> i64 {\n",
    "    cache count: i64 = 0\n",
    "    while {\n",
    "        if count == limit { return }\n",
    "        count += 1\n",
    "        yield 1us: count\n",
    "    }\n",
    "}\n",
    "\n",
    // A block that ends with an expression: a call after its yields,
    // and an `if` that holds them.
    "fn trailed(const first: i64) -> i64 {\n",
    "    inject logger\n",
    "    {\n",
    "        yield 1us: first\n",
    "        log_info(logger, \"done\")\n",
    "    }\n",
    "}\n",
    "\n",
    "fn guarded(const wanted: bool) -> i64 {\n",
    "    {\n",
    "        if wanted { yield 1us: 1 }\n",
    "    }\n",
    "}\n",
    "\n",
    // A `start` that returns early still leaves a source to resume.
    "fn started(const quiet: bool) -> i64 {\n",
    "    inject logger\n",
    "    start {\n",
    "        if quiet { return }\n",
    "        log_info(logger, \"started\")\n",
    "    }\n",
    "    {\n",
    "        yield 1us: 4\n",
    "    }\n",
    "}\n",
    "\n",
    "fn doubled(value: i64) -> i64 {\n",
    "    when { return value * 2 }\n",
    "}\n",
    "\n",
    "fn wired() -> i64 => doubled(two_values())\n",
    "\n",
    "fn backwards() -> i64 {\n",
    "    yield 2us: 1\n",
    "    yield earliest_start + 1us: 2\n",
    "}\n",
    "\n",
    "fn negative() -> i64 {\n",
    "    yield -1us: 1\n",
    "}\n",
    "\n",
    "test sources {\n",
    "    assert eval(two_values()) == [_, 7, 9]\n",
    "    assert eval(greeting()) == [_, \"hello\", \"world\"]\n",
    "    assert eval(greeting(every: 2us)) == [_, _, \"hello\", _, \"world\"]\n",
    "    assert eval(counter()) == [_, 1, 2, 3]\n",
    "    assert eval(counter(limit: 0)) == []\n",
    "    assert eval(at_once(first: 5)) == [5, _, 6]\n",
    "    assert eval(absolute()) == [_, _, _, 1, _, 2, 3]\n",
    "    assert eval(late(), RunConfig(start: earliest_start + 2us)) == [_, _, 2]\n",
    "    assert eval(ended(limit: 2)) == [_, 1, 2]\n",
    "    assert eval(wired()) == [_, 14, 18]\n",
    "    assert eval(trailed(first: 6)) == [_, 6]\n",
    "    assert eval(guarded(wanted: true)) == [_, 1]\n",
    "    assert eval(guarded(wanted: false)) == []\n",
    "    assert eval(started(quiet: true)) == [_, 4]\n",
    "    assert eval(started(quiet: false)) == [_, 4]\n",
    // CRUN-2: a run ends before its `end`, with the source suspended.
    "    assert eval(counter(limit: 9), RunConfig(length: 3us)) == [_, 1, 2]\n",
    "    assert raises(\"yield.non_increasing_time\") { eval(backwards()) }\n",
    "    assert raises(\"yield.negative_duration\") { eval(negative()) }\n",
    "}\n",
);

/// SOURCE-1: a source is a generator that holds its statements, a node
/// that resumes it, with the source's `start` and `stop`, and a graph
/// that wakes the node once as it starts. SOURCE-2: `yield time:
/// value` gives the generator's caller the time, then the value.
#[test]
fn a_source_is_a_generator_a_node_resumes() {
    let emitted = emit(&[("sources.hgl", SOURCES)]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let python = emitted.text("_hgl/_m_demo/_m_sources/__init__.py");
    for expected in [
        concat!(
            "def _yields_two_values(_state):\n",
            "    yield (_dt.timedelta(microseconds=1), 7)\n",
            "    yield (_dt.timedelta(microseconds=1), 9)\n",
        ),
        concat!(
            "@_hg.compute_node\n",
            "def _source_two_values(_begin: _hg.TS[_py.bool], _state: _hg.STATE = None, ",
            "_clock: _hg.EvaluationClock = None, _scheduler: _hg.SCHEDULER = None) ",
            "-> _hg.TS[_py.int]:\n",
            "    _published = _core.resume(_state._source, _clock.evaluation_time, _scheduler)\n",
            "    if _published is not None:\n",
            "        return _published[0]\n",
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
    ] {
        assert!(
            python.contains(expected),
            "the module has:\n{expected}\nin:\n{python}"
        );
    }
}

/// FN-15: a `var` parameter. A call gives it a place and assigns it:
/// a `var`, a field or an element of one, a node's `state` and `cache`.
const PLACES: &str = concat!(
    "module demo.places\n",
    "\n",
    "struct Book {\n",
    "    orders: list<i64>\n",
    "    name: str\n",
    "}\n",
    "\n",
    "const fn bump(var count: i64, by: i64) {\n",
    "    count += by\n",
    "}\n",
    "const fn set_first(var items: list<i64>, value: i64) {\n",
    "    items[0] = value\n",
    "}\n",
    // A `var` parameter is given on to another.
    "const fn through(var count: i64) { bump(count, 1) }\n",
    // A result beside the place.
    "const fn take(var count: i64) -> i64 {\n",
    "    let before = count\n",
    "    count = 0\n",
    "    return before\n",
    "}\n",
    "const fn both(var left: i64, var right: i64) {\n",
    "    let held = left\n",
    "    left = right\n",
    "    right = held\n",
    "}\n",
    // Every way out gives the place what the parameter holds there.
    "const fn early(var count: i64, halt: bool) -> i64 {\n",
    "    if halt { return -1 }\n",
    "    count += 1\n",
    "    return count\n",
    "}\n",
    "\n",
    "const fn total(values: list<i64>) -> i64 {\n",
    "    var sum: i64 = 0\n",
    "    for value in values {\n",
    "        bump(sum, value)\n",
    "    }\n",
    "    return sum\n",
    "}\n",
    // "a value taken from the place before the call is as it was".
    "const fn kept() -> bool {\n",
    "    var entries: list<i64> = [1, 2]\n",
    "    let before = entries\n",
    "    set_first(entries, 9)\n",
    "    return before == [1, 2] && entries == [9, 2]\n",
    "}\n",
    "const fn nested() -> i64 {\n",
    "    var book: Book = Book(orders: [1, 2], name: \"a\")\n",
    "    set_first(book.orders, 5)\n",
    "    bump(book.orders[1], 3)\n",
    "    return book.orders[0] + book.orders[1]\n",
    "}\n",
    "const fn taken() -> i64 {\n",
    "    var held: i64 = 7\n",
    "    let was = take(held)\n",
    "    return was * 10 + held\n",
    "}\n",
    "const fn swapped() -> i64 {\n",
    "    var a: i64 = 1\n",
    "    var b: i64 = 2\n",
    "    both(a, b)\n",
    "    return a * 10 + b\n",
    "}\n",
    "const fn stepped() -> i64 {\n",
    "    var n: i64 = 0\n",
    "    through(n)\n",
    "    let first = early(n, true)\n",
    "    let second = early(n, false)\n",
    "    return first * 100 + second * 10 + n\n",
    "}\n",
    "fn counted(value: i64) -> i64 {\n",
    "    state seen: i64 = 0\n",
    "    cache held: i64 = 0\n",
    "    when {\n",
    "        bump(seen, 1)\n",
    "        bump(held, value)\n",
    "        return seen * 100 + held\n",
    "    }\n",
    "}\n",
    // The place is the one named when the call's arguments are
    // evaluated: `items[i]` is `items[0]`, though the call moves `i`.
    "const fn change(var index: i64, var item: i64) {\n",
    "    index = 1\n",
    "    item = 9\n",
    "}\n",
    // EXPR-2: the first argument fails first, though the second's
    // index is held before the call.
    "const fn apply(value: i64, var item: i64) { item = value }\n",
    "const fn ordered(zero: i64, indices: dict<str, i64>) -> i64 {\n",
    "    var items: list<i64> = [5, 6]\n",
    "    apply(1 //{zero: .fail} zero, items[indices[\"missing\"]])\n",
    "    return items[0]\n",
    "}\n",
    // What an index is of is read before the index is evaluated: a
    // missing `key` fails before the division does.
    "const fn deep(key: str, zero: i64) -> i64 {\n",
    "    var rows: dict<str, list<i64>> = [\"a\": [5, 6]]\n",
    "    bump(rows[key][1 //{zero: .fail} zero], 1)\n",
    "    return rows[\"a\"][1]\n",
    "}\n",
    // Two places inside one root: each assignment reads the root as
    // the one before it left it.
    "const fn crossed(key: str, i: i64, j: i64) -> i64 {\n",
    "    var rows: dict<str, list<i64>> = [\"a\": [5, 6]]\n",
    "    both(rows[key][i], rows[key][j])\n",
    "    return rows[\"a\"][0] * 10 + rows[\"a\"][1]\n",
    "}\n",
    "const fn moved() -> i64 {\n",
    "    var i: i64 = 0\n",
    "    var items: list<i64> = [5, 6]\n",
    "    change(i, items[i])\n",
    "    return items[0] * 100 + items[1] * 10 + i\n",
    "}\n",
    "\n",
    "test places {\n",
    "    assert total([1, 2, 3]) == 6\n",
    "    assert kept()\n",
    "    assert nested() == 10\n",
    "    assert taken() == 70\n",
    "    assert swapped() == 21\n",
    "    assert stepped() == -100 + 20 + 2\n",
    "    var count: i64 = 1\n",
    "    bump(count, 4)\n",
    "    assert count == 5\n",
    "    assert moved() == 961\n",
    "    assert raises(\"numeric.division_by_zero\") { ordered(0, [\"a\": 1]) }\n",
    "    assert raises(\"value.missing_key\") { ordered(1, [\"a\": 1]) }\n",
    "    assert ordered(1, [\"missing\": 1]) == 5\n",
    "    assert raises(\"value.missing_key\") { deep(\"b\", 0) }\n",
    "    assert raises(\"numeric.division_by_zero\") { deep(\"a\", 0) }\n",
    "    assert deep(\"a\", 1) == 7\n",
    "    assert crossed(\"a\", 0, 1) == 65\n",
    "    assert eval(counted(value: [1, _, 2])) == [101, _, 203]\n",
    "}\n",
);

/// FN-15: "what it holds when the function returns is what the place
/// is given". Python has no place to give a value to: a function with
/// `var` parameters returns its result and then what each holds, and
/// the call, the whole of a statement, assigns the places after it, in
/// the order of the parameters.
#[test]
fn a_call_assigns_the_places_its_var_parameters_are_given() {
    let emitted = emit(&[("places.hgl", PLACES)]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let python = emitted.text("_hgl/_m_demo/_m_places/__init__.py");
    for expected in [
        concat!(
            "def bump(count, by):\n",
            "    count = _core.add__i64_i64(lhs=count, rhs=by)\n",
            "    return (None, count)\n",
        ),
        concat!(
            "def take(count):\n",
            "    before = count\n",
            "    count = 0\n",
            "    return (before, count)\n",
        ),
        "    return (None, left, right)\n",
        concat!(
            "def early(count, halt):\n",
            "    if halt:\n",
            "        return (_core.neg__i64(value=1), count)\n",
        ),
        // The call, and then the place.
        concat!(
            "        _given = bump(count=sum, by=value)\n",
            "        sum = _given[1]\n",
        ),
        // A `var` parameter given on is a place of this function.
        concat!(
            "def through(count):\n",
            "    _given = bump(count=count, by=1)\n",
            "    count = _given[1]\n",
            "    return (None, count)\n",
        ),
        // A place inside a value: its root is given a new value.
        concat!(
            "    _given = set_first(items=_core.shared(book.orders), value=5)\n",
            "    book = _core.with_field(book, orders=_given[1])\n",
            "    _given = bump(count=_core.item(book.orders, 1), by=3)\n",
            "    book = _core.set_at(book, ((\"f\", \"orders\"), (\"i\", 1),), lambda: _given[1])\n",
        ),
        // The call's own result is the first of what it gives.
        concat!(
            "    _given = take(count=held)\n",
            "    held = _given[1]\n",
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
    ] {
        assert!(
            python.contains(expected),
            "the module has:\n{expected}\nin:\n{python}"
        );
    }
}

/// FN-15: a call that assigns a place is written with its assignments
/// after it, which a statement has room for. One inside an expression
/// is reported.
#[test]
fn a_place_call_inside_an_expression_is_reported() {
    let emitted = emit(&[(
        "inside.hgl",
        concat!(
            "module demo.inside\n",
            "const fn take(var count: i64) -> i64 {\n",
            "    count = 0\n",
            "    return 1\n",
            "}\n",
            "const fn twice() -> i64 {\n",
            "    var held: i64 = 7\n",
            "    return take(held) + take(held)\n",
            "}\n",
        ),
    )]);
    assert_eq!(
        emitted.reported,
        [
            "inside.hgl:8:12 target.unsupported",
            "inside.hgl:8:25 target.unsupported",
        ]
    );
}

/// TYPE-14: a temporal `T?`. TIME-7: "`return null` for an `i64?`
/// result [publishes] a null". TYPE-16: a nullable's delta "holds what
/// was published, a null among them".
const NULLABLE: &str = concat!(
    "module demo.nullable\n",
    "\n",
    "fn publish_nullable(command: i64) -> i64? {\n",
    "    when {\n",
    "        if command == 0 { return null }\n",
    "        return command\n",
    "    }\n",
    "}\n",
    "\n",
    "fn read_back(value: i64?) -> i64 {\n",
    "    when {\n",
    "        if value == null { return -1 }\n",
    "        return value + 100\n",
    "    }\n",
    "}\n",
    "\n",
    "fn recorded(value: i64?) -> i64? {\n",
    "    when valid(value) && modified(value) { return delta_value(value) }\n",
    "}\n",
    "\n",
    "fn unwrapped(value: i64?) -> i64 {\n",
    "    when {\n",
    "        let change: delta<i64?> = delta_value(value)\n",
    "        if change.value == null { return 0 }\n",
    "        return 1\n",
    "    }\n",
    "}\n",
    "\n",
    "fn made(command: i64) -> i64? {\n",
    "    when {\n",
    "        if command == 0 { return delta<i64?>() }\n",
    "        return delta<i64?>(value: command)\n",
    "    }\n",
    "}\n",
    "\n",
    "fn through_out(command: i64) -> i64? {\n",
    "    inject out\n",
    "    when {\n",
    "        if command == 0 { out = null } else { out = command }\n",
    "    }\n",
    "}\n",
    "\n",
    "fn nothing_then_null() -> i64? {\n",
    "    yield 1us: null\n",
    "    yield 1us: 3\n",
    "}\n",
    "\n",
    "const fn or_zero(value: i64?) -> i64 => if value == null { 0 } else { value }\n",
    "const fn none_of(value: i64) -> i64? => if value == 0 { null } else { value }\n",
    "fn lifted_in(value: i64?) -> i64 => or_zero(value)\n",
    "fn lifted_out(value: i64) -> i64? => none_of(value)\n",
    "fn wired(command: i64) -> i64 => read_back(publish_nullable(command))\n",
    "\n",
    "const fn nulls(seen: list<tuple<datetime, delta<i64?>>>) -> i64 {\n",
    "    var count: i64 = 0\n",
    "    for entry in seen {\n",
    "        if entry[1].value == null { count += 1 }\n",
    "    }\n",
    "    return count\n",
    "}\n",
    "\n",
    "test nullable {\n",
    // A published null is a publication, and `_` is silence.
    "    assert eval(publish_nullable(command: [7, 0, _, 0, 9])) == [7, null, _, null, 9]\n",
    "    assert eval(read_back(value: [null, _, 7, null, _])) == [-1, _, 107, -1]\n",
    "    assert eval(recorded(value: [null, _, 7, null, _])) == [null, _, 7, null]\n",
    "    assert eval(unwrapped(value: [null, 7])) == [0, 1]\n",
    "    assert eval(made(command: [0, 5])) == [null, 5]\n",
    "    assert eval(through_out(command: [0, 5, 0])) == [null, 5, null]\n",
    "    assert eval(nothing_then_null()) == [_, null, 3]\n",
    "    assert eval(lifted_in(value: [null, 3])) == [0, 3]\n",
    "    assert eval(lifted_out(value: [0, 3])) == [null, 3]\n",
    "    assert eval(wired(command: [0, 4])) == [-1, 104]\n",
    "    assert nulls(eval(publish_nullable(command: [7, 0, _, 0]))) == 2\n",
    "}\n",
);

/// TYPE-14: a temporal `T?` is hgraph's value that holds one `T` or is
/// unset, `TS[T | None]`. A null is `None` as a value, and hgraph's
/// `Null` where it is published: to hgraph a node that returns `None`
/// publishes nothing. TYPE-16: a nullable's delta is hgraph's own.
#[test]
fn a_nullable_is_a_value_that_may_be_unset() {
    let emitted = emit(&[("nullable.hgl", NULLABLE)]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let python = emitted.text("_hgl/_m_demo/_m_nullable/__init__.py");
    for expected in [
        concat!(
            "@_hg.compute_node\n",
            "def publish_nullable(command: _hg.TS[_py.int]) -> _hg.TS[_py.int | None]:\n",
            "    if _core.eq__i64_i64_f64_f64(lhs=command.value, rhs=0, abs=0.0, rel=0.0):\n",
            "        return _core.published(None)\n",
            "    return _core.published(command.value)\n",
        ),
        "def read_back(value: _hg.TS[_py.int | None]) -> _hg.TS[_py.int]:\n",
        // TYPE-16: `delta_value` of a nullable is hgraph's own delta.
        "def recorded(value: _hg.TS[_py.int | None]) -> _hg.TS[_py.int | None]:\n    return value.delta_value\n",
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
    ] {
        assert!(
            python.contains(expected),
            "the module has:\n{expected}\nin:\n{python}"
        );
    }
}

/// EXPR-3 on hgraph, for a dictionary and for an element of a list: a
/// `var`, a `state` or a `cache` is changed where it is, and what took
/// its value before is as it was (LIST-3's rule, for every collection).
const ENTRIES: &str = concat!(
    "module demo.entries\n",
    "\n",
    // A function whose result cannot be the dictionary: it is lent it.
    "const fn count_of(counts: dict<str, i64>, name: str) -> i64 {\n",
    "    let found = get(counts, name)\n",
    "    if found != null { return found }\n",
    "    return 0\n",
    "}\n",
    "\n",
    // A dictionary built an entry at a time, each read and then assigned.
    "const fn counted(names: list<str>) -> dict<str, i64> {\n",
    "    var counts: dict<str, i64> = []\n",
    "    for name in names {\n",
    "        counts[name] = count_of(counts, name) + 1\n",
    "    }\n",
    "    return counts\n",
    "}\n",
    "\n",
    "const fn same<T>(value: T) -> T { return value }\n",
    "\n",
    // A `var` parameter's element is assigned by making a new list:
    // where the call fails the caller's list is as it was (FN-15).
    "const fn replaced(var items: list<i64>, value: i64) { items[0] = value }\n",
    "\n",
    "const fn failing(var items: list<i64>, zero: i64) {\n",
    "    items[0] = 9\n",
    "    let quotient = 1 //{zero: .fail} zero\n",
    "}\n",
    "\n",
    "test entries {\n",
    // A `let` is the dictionary as it was, however the place changes.
    "    var book: dict<str, i64> = [\"a\": 1]\n",
    "    let before = book\n",
    "    book[\"b\"] = 2\n",
    "    assert before == [\"a\": 1]\n",
    "    assert book == [\"a\": 1, \"b\": 2]\n",
    "    assert count_of(book, \"b\") == 2\n",
    "    let through = same(book)\n",
    "    book[\"a\"] = 5\n",
    "    assert through == [\"a\": 1, \"b\": 2]\n",
    "    assert book == [\"a\": 5, \"b\": 2]\n",
    "    assert counted([\"x\", \"y\", \"x\"]) == [\"x\": 2, \"y\": 1]\n",
    // An element of a list.
    "    var items: list<i64> = [1, 2, 3]\n",
    "    let was = items\n",
    "    items[1] = 9\n",
    "    items[1] += 1\n",
    "    assert was == [1, 2, 3]\n",
    "    assert items == [1, 10, 3]\n",
    "    replaced(items, 7)\n",
    "    assert items == [7, 10, 3]\n",
    "    assert raises(\"numeric.division_by_zero\") { failing(items, 0) }\n",
    "    assert items == [7, 10, 3]\n",
    // A collection inside another is given a new value, and what
    // took it before is as it was.
    "    var rows: dict<str, list<i64>> = [\"a\": [1]]\n",
    "    let row = rows[\"a\"]\n",
    "    rows[\"a\"][0] = 2\n",
    "    rows[\"b\"] = items\n",
    "    items[0] = 0\n",
    "    assert row == [1]\n",
    "    assert rows == [\"a\": [2], \"b\": [7, 10, 3]]\n",
    "}\n",
);

/// EXPR-3: "An assignment may name a place inside a value". An entry
/// of the dictionary, and an element of the list, that a `var` of the
/// function holds itself is assigned where the collection is,
/// `_core.set_entry` and `_core.set_item`: no entry is copied for it.
/// The collection is shared and copied as a list that grows is: a
/// `let` takes it marked, and a function whose result cannot be the
/// collection is lent it unmarked.
#[test]
fn an_entry_and_an_element_are_assigned_where_the_collection_is() {
    let emitted = emit(&[("entries.hgl", ENTRIES)]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let python = emitted.text("_hgl/_m_demo/_m_entries/__init__.py");
    for expected in [
        // Read by a function that gives an entry, and so is lent the
        // dictionary; then assigned where it is.
        "        counts = _core.set_entry(counts, name, _core.add__i64_i64(lhs=count_of(counts=counts, name=name), rhs=1))\n",
        "    found = _core.get_dict_K__T__K(store=counts, key=name)\n",
        // A `let` takes the dictionary itself, marked.
        concat!(
            "    before = _core.shared(book)\n",
            "    book = _core.set_entry(book, \"b\", 2)\n",
        ),
        // A function that may give back what it is given.
        "    through = same(value=_core.shared(book))\n",
        // An element of a list, and a compound assignment of one.
        "    items = _core.set_item(items, 1, 9)\n",
        "    items = _core.set_item(items, 1, _core.add__i64_i64(lhs=_core.item(items, 1), rhs=1))\n",
        // A `var` parameter's element: a new list.
        "    items = _core.with_item(items, 0, value)\n",
        // An entry that is a list: the dictionary is changed where it
        // is, and the list inside it is given a new value.
        "    rows = _core.set_at(rows, ((\"k\", \"a\"), (\"i\", 0),), lambda: 2)\n",
        // What a dictionary keeps is marked.
        "    rows = _core.set_entry(rows, \"b\", _core.shared(items))\n",
    ] {
        assert!(
            python.contains(expected),
            "the module has:\n{expected}\nin:\n{python}"
        );
    }
}

/// EXPR-3 and LIST-1 on hgraph for a collection inside another value: a
/// struct's dictionary and list, a list in a dictionary, a list in a
/// list. Each is changed where it is, and what took it before is as it
/// was.
const INSIDE: &str = concat!(
    "module demo.inside\n",
    "\n",
    "struct Book {\n",
    "    bids: dict<i64, i64>\n",
    "    orders: list<i64>\n",
    "    name: str\n",
    "}\n",
    "\n",
    "const fn total(values: list<i64>) -> i64 {\n",
    "    var sum: i64 = 0\n",
    "    for value in values { sum += value }\n",
    "    return sum\n",
    "}\n",
    "\n",
    // A function whose result cannot be the struct: it is lent it, and
    // marks the list it gives of it.
    "const fn orders_of(book: Book) -> list<i64> => book.orders\n",
    "\n",
    // A struct's dictionary and its list, each changed a step at a time.
    "const fn built(count: i64) -> Book {\n",
    "    var book: Book = Book(bids: [], orders: [], name: \"a\")\n",
    "    var i: i64 = 0\n",
    "    while i < count {\n",
    "        book.bids[i] = i * 2\n",
    "        append(book.orders, i)\n",
    "        i += 1\n",
    "    }\n",
    "    return book\n",
    "}\n",
    "\n",
    // Lists in a dictionary, under a key that is an expression.
    "const fn grouped(count: i64) -> dict<i64, list<i64>> {\n",
    "    var groups: dict<i64, list<i64>> = [0: [], 1: []]\n",
    "    var i: i64 = 0\n",
    "    while i < count {\n",
    "        append(groups[i % 2], i)\n",
    "        i += 1\n",
    "    }\n",
    "    return groups\n",
    "}\n",
    "\n",
    // A loop's name holds an entry that the list holds too.
    "const fn copied(rows: list<list<i64>>) -> list<list<i64>> {\n",
    "    var out: list<list<i64>> = []\n",
    "    for row in rows { append(out, row) }\n",
    "    return out\n",
    "}\n",
    "\n",
    "const fn rows_kept() -> bool {\n",
    "    var rows: list<list<i64>> = [[1], [2]]\n",
    "    let copy = copied(rows)\n",
    "    append(rows[0], 9)\n",
    "    rows[1][0] = 7\n",
    "    return copy == [[1], [2]] && rows == [[1, 9], [7]]\n",
    "}\n",
    "\n",
    // EXPR-7: the way in is read before the value is evaluated: a key
    // the dictionary has not fails first.
    "const fn missing(zero: i64) -> i64 {\n",
    "    var groups: dict<str, list<i64>> = [\"a\": [1]]\n",
    "    groups[\"nope\"][0] = 1 //{zero: .fail} zero\n",
    "    return 1\n",
    "}\n",
    "\n",
    // What is put in holds a collection on the way in: the value the
    // place had, and not the one it is given. Evaluating the argument
    // marks `rows`, after the way in has been read and before anything
    // is changed.
    "const fn grafted() -> bool {\n",
    "    var rows: list<list<any>> = [[1]]\n",
    "    append(rows[0], 2)\n",
    "    append(rows[0], rows)\n",
    "    return rows == [[1, 2, [[1, 2]]]]\n",
    "}\n",
    "\n",
    "const fn planted() -> bool {\n",
    "    var rows: list<list<any>> = [[1]]\n",
    "    append(rows[0], 2)\n",
    "    rows[0][0] = rows\n",
    "    return rows == [[[[1, 2]], 2]]\n",
    "}\n",
    "\n",
    // A node's `state` that is a struct of collections.
    "fn gathered(value: i64) -> i64 {\n",
    "    state book: Book = Book(bids: [], orders: [], name: \"s\")\n",
    "    when {\n",
    "        append(book.orders, value)\n",
    "        book.bids[value] = value * 2\n",
    "        return total(book.orders) + book.bids[value]\n",
    "    }\n",
    "}\n",
    "\n",
    "test inside {\n",
    // A `let` is the struct as it was, however what is inside the
    // place changes.
    "    var book: Book = Book(bids: [1: 10], orders: [1], name: \"a\")\n",
    "    let before = book\n",
    "    append(book.orders, 2)\n",
    "    book.bids[2] = 20\n",
    "    assert before == Book(bids: [1: 10], orders: [1], name: \"a\")\n",
    "    assert book == Book(bids: [1: 10, 2: 20], orders: [1, 2], name: \"a\")\n",
    // A collection taken out of a value is as it was.
    "    let orders = book.orders\n",
    "    append(book.orders, 3)\n",
    "    assert orders == [1, 2]\n",
    "    assert total(book.orders) == 6\n",
    "    let taken = orders_of(book)\n",
    "    append(book.orders, 4)\n",
    "    assert taken == [1, 2, 3]\n",
    // A field that is no collection: the struct is made anew, and its
    // collections are still its own.
    "    book.name = \"b\"\n",
    "    append(book.orders, 5)\n",
    "    assert book.orders == [1, 2, 3, 4, 5]\n",
    "    assert before.orders == [1]\n",
    "    var groups: dict<str, list<i64>> = [\"a\": [1]]\n",
    "    let group = groups[\"a\"]\n",
    "    append(groups[\"a\"], 2)\n",
    "    groups[\"a\"][0] = 9\n",
    "    groups[\"b\"] = group\n",
    "    append(groups[\"b\"], 5)\n",
    "    assert group == [1]\n",
    "    assert groups == [\"a\": [9, 2], \"b\": [1, 5]]\n",
    "    assert rows_kept()\n",
    "    assert grafted()\n",
    "    assert planted()\n",
    "    assert raises(\"value.missing_key\") { missing(0) }\n",
    "    assert built(3) == Book(bids: [0: 0, 1: 2, 2: 4], orders: [0, 1, 2], name: \"a\")\n",
    "    assert grouped(5) == [0: [0, 2, 4], 1: [1, 3]]\n",
    "    assert eval(gathered(value: [1, _, 2])) == [3, _, 7]\n",
    "}\n",
);

/// EXPR-3: "An assignment may name a place inside a value, at any
/// depth". Where the place is inside the value a `var` or a `state` of
/// the function holds, with a collection on the way, the provider goes
/// in: `_core.set_at` for an assignment, and `_core.given_at` for a
/// native function of core that is given the place (LIST-1, FN-15).
/// Each collection on the way is changed where it is while it is its
/// holder's alone. A collection read out of a value by what may keep
/// it is marked as shared, and one read through, or lent, is not.
#[test]
fn a_collection_inside_a_value_is_changed_where_it_is() {
    let emitted = emit(&[("inside.hgl", INSIDE)]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let python = emitted.text("_hgl/_m_demo/_m_inside/__init__.py");
    for expected in [
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
        // Taken by what may keep it: marked.
        "    before = _core.shared(book)\n",
        "    orders = _core.shared(book.orders)\n",
        "    group = _core.shared(_core.entry(groups, \"a\"))\n",
        "    return _core.shared(book.orders)\n",
        // A loop's entries are marked as they are taken.
        "    for row in _core.each(rows):\n",
        // A field that is no collection: a new struct.
        "    book = _core.with_field(book, name=\"b\")\n",
    ] {
        assert!(
            python.contains(expected),
            "the module has:\n{expected}\nin:\n{python}"
        );
    }
}

/// FN-15, GEN-6: an operator whose value candidates each take a place.
/// The operator's declaration marks none; the candidate a call
/// resolves to says which of its arguments are places.
const PUSHES: &str = concat!(
    "module demo.pushes\n",
    "\n",
    "operator push<C, V>(values: C, item: V)\n",
    "\n",
    "impl const fn push(var values: list<i64>, item: i64) {\n",
    "    append(values, item)\n",
    "}\n",
    "\n",
    "impl const fn push(var values: dict<str, i64>, item: str) {\n",
    "    values[item] = 1\n",
    "}\n",
    "\n",
    "struct Book {\n",
    "    orders: list<i64>\n",
    "}\n",
    "\n",
    "test pushes {\n",
    "    var entries: list<i64> = [1]\n",
    "    let before = entries\n",
    "    push(entries, 2)\n",
    "    assert before == [1]\n",
    "    assert entries == [1, 2]\n",
    "    var seen: dict<str, i64> = []\n",
    "    push(seen, \"a\")\n",
    "    assert seen == [\"a\": 1]\n",
    "    var book: Book = Book(orders: [1])\n",
    "    push(book.orders, 2)\n",
    "    assert book == Book(orders: [1, 2])\n",
    "}\n",
);

/// FN-15: "So may a value candidate of an operator (GEN-6) ... a call of
/// an operator is resolved as any other, and the candidate it resolves
/// to says which of its arguments are places." A candidate with a `var`
/// parameter is written as a function with one is: it returns its
/// result and then what each holds, and the call assigns the places. A
/// place's own list is given as it is, and put back where the call
/// fails.
#[test]
fn a_candidate_that_takes_a_place_is_called_as_a_function_that_does() {
    let emitted = emit(&[("pushes.hgl", PUSHES)]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let python = emitted.text("_hgl/_m_demo/_m_pushes/__init__.py");
    for expected in [
        concat!(
            "def _impl_push_list_i64__i64(values, item):\n",
            "    _given = _core.fn_append(values=values, item=item)\n",
            "    values = _given[1]\n",
            "    return (None, values)\n",
        ),
        // The list's candidate is given the place's own list.
        concat!(
            "    _was_1 = _py.len(entries)\n",
            "    try:\n",
            "        _given = _impl_push_list_i64__i64(values=entries, item=2)\n",
            "    except _py.BaseException:\n",
            "        entries = _core.restored(entries, _was_1)\n",
            "        raise\n",
            "    entries = _given[1]\n",
        ),
        // The dictionary's candidate, selected by the argument's type.
        concat!(
            "    _given = _impl_push_dict_str__i64__str(values=_core.shared(seen), item=\"a\")\n",
            "    seen = _core.shared(_given[1])\n",
        ),
        // A place inside a value.
        concat!(
            "    _given = _impl_push_list_i64__i64(values=_core.shared(book.orders), item=2)\n",
            "    book = _core.with_field(book, orders=_given[1])\n",
        ),
    ] {
        assert!(
            python.contains(expected),
            "the module has:\n{expected}\nin:\n{python}"
        );
    }
}

/// A tenor on hgraph (core's TEN-1 to TEN-9): its literal, its own
/// arithmetic, order and text, and a date moved by one, on a calendar
/// and under a rolling rule.
const MOVES: &str = concat!(
    "module demo.moves\n",
    "\n",
    // A calendar of the program's own: one day closed, beside the weekend.
    "struct Closed {\n",
    "    day: date\n",
    "}\n",
    "\n",
    "impl const fn business_day(calendar: Closed, day: date) -> bool =>\n",
    "    day != calendar.day && business_day(Weekdays(), day)\n",
    "\n",
    // TEN-5, TEN-7: the options of `+`, each an argument of the candidate.
    "const fn settles(trade: date, calendar: Closed) -> date => trade +{calendar: calendar} 2B\n",
    "\n",
    "const fn matures(spot: date) -> date => spot +{rolling_rule: Rolling.modified_following} 3M\n",
    "\n",
    "const fn summed(lhs: tenor, rhs: tenor) -> tenor => lhs + rhs\n",
    "\n",
    "test moves {\n",
    // TEN-4: months first, to the last day of a shorter month.
    "    assert @2026-01-31 + 1M == @2026-02-28\n",
    "    assert @2026-01-31 - 2W == @2026-01-17\n",
    // TEN-6: a count of business days starts from a business day. The
    // 3rd is a Saturday: on to the Monday, or back to the Friday.
    "    assert @2026-01-03 + 2B == @2026-01-07\n",
    "    assert @2026-01-03 - 0B == @2026-01-02\n",
    "    assert settles(@2026-01-02, Closed(day: @2026-01-05)) == @2026-01-07\n",
    // TEN-8: `modified_following` gives the business day before where
    // the one after is in another month.
    "    assert matures(@2025-10-31) == @2026-01-30\n",
    // TEN-2, TEN-3, TEN-9: a tenor's own arithmetic, order and text.
    "    assert 6M + 6M == 1Y\n",
    "    assert 2 * 1Y6M == 3Y\n",
    "    assert -(1M2D) == -1 * 1M2D\n",
    "    assert 1M < 2M\n",
    "    assert !(1M < 45D)\n",
    "    assert (1M <=> 45D) == CmpResult.unordered\n",
    "    assert convert<str>(13D) == \"1W6D\"\n",
    "    assert convert<str>(1M - 1D) == \"1M-1D\"\n",
    "    assert raises(\"tenor.mixed_units\") { summed(2B, 1M) }\n",
    "    assert raises(\"numeric.out_of_range\") { summed(9223372036854775807D, 1D) }\n",
    "}\n",
);

/// TEN-1 to TEN-9 on hgraph. A tenor literal is the provider's `Tenor`
/// by its counts (LEX-28, TYPE-17), and its own operators the
/// provider's functions. `date + tenor` is core's HGL, written for the
/// calendar and the rolling rule it is given, each an argument of the
/// candidate; core's functions ask the program's own `business_day`.
#[test]
fn a_tenor_moves_a_date_on_a_calendar_and_under_a_rule() {
    let emitted = emit(&[("moves.hgl", MOVES)]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let python = emitted.text("_hgl/_m_demo/_m_moves/__init__.py");
    for expected in [
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
        "_core.convert_tenor(value=_core.Tenor(0, 13))",
        // TEN-4: core's candidate, with no option.
        "_hgl_4core._impl_add__date_tenor(lhs=_dt.date(2026, 1, 31), rhs=_core.Tenor(1, 0))",
        // TEN-5: written for the calendar it is given.
        "_hgl_4core._inst__impl_add__date_tenor_C__demo_moves__Closed(lhs=trade, calendar=calendar, rhs=_core.Tenor(business=2))",
        // TEN-7, TEN-8: and for the rolling rule.
        "_hgl_4core._inst__impl_add__date_tenor_R__core__Rolling(lhs=spot, rolling_rule=_hgl_4core.Rolling[\"modified_following\"], rhs=_core.Tenor(3, 0))",
    ] {
        assert!(
            python.contains(expected),
            "the module has:\n{expected}\nin:\n{python}"
        );
    }
    // Core's own functions, written for the program's calendar, ask the
    // program's `business_day`.
    let core = emitted.text("_hgl/_m_core/__init__.py");
    assert!(
        core.contains(
            "_hgl_4demo_5moves._impl_business_day_Closed_date(calendar=calendar, day=at)"
        ),
        "{core}"
    );
}

/// TYPE-17: a tenor is written as a value. As the type of a temporal
/// function's input or result it is reported as not written.
#[test]
fn a_tenor_is_no_temporal_type_yet() {
    let emitted = emit(&[(
        "tenor.hgl",
        concat!(
            "module demo.rules\n",
            "const fn quarter() -> tenor => 3M\n",
            "fn passed(lag: tenor) -> tenor {\n",
            "    when { return lag }\n",
            "}\n",
        ),
    )]);
    assert_eq!(
        emitted.reported,
        [
            "tenor.hgl:3:11 target.unsupported",
            "tenor.hgl:3:4 target.unsupported",
        ]
    );
}

/// TYPE-14: a temporal `T?` is written for a scalar and an enum, which
/// publish whole. A nullable set is not: a set publishes its changes,
/// which hgraph's value that may be unset would not gather. It is
/// reported where it is an input and where it is a result.
#[test]
fn a_nullable_set_is_not_written_as_a_temporal_type() {
    let emitted = emit(&[(
        "sets.hgl",
        concat!(
            "module demo.maybe\n",
            "fn taken(members: set<i64>?) -> bool {\n",
            "    when { return members == null }\n",
            "}\n",
            "fn given(command: i64) -> set<i64>? {\n",
            "    when { return null }\n",
            "}\n",
            // A `const` one is an ordinary value.
            "fn fixed(value: i64, const members: set<i64>?) -> bool {\n",
            "    when { return members == null }\n",
            "}\n",
        ),
    )]);
    assert_eq!(
        emitted.reported,
        [
            "sets.hgl:2:10 target.unsupported",
            "sets.hgl:5:4 target.unsupported",
        ]
    );
}

/// Core's LIST-1 to LIST-3: `append` and `extend` give a list more
/// entries. Each takes its list as a `var` parameter, so the call
/// assigns the place, and a value taken from it before is as it was.
const LISTS: &str = concat!(
    "module demo.lists\n",
    "\n",
    "fn first_gathered(value: i64) -> i64 {\n",
    "    cache seen: list<i64> = []\n",
    "    when {\n",
    "        append(seen, value)\n",
    "        return seen[0]\n",
    "    }\n",
    "}\n",
    "\n",
    "const fn grown(first: list<i64>, more: list<i64>) -> list<i64> {\n",
    "    var entries: list<i64> = first\n",
    "    extend(entries, more)\n",
    "    append(entries, 0)\n",
    "    return entries\n",
    "}\n",
    "\n",
    "test lists {\n",
    "    var entries: list<i64> = [1, 2]\n",
    "    let before = entries\n",
    "    append(entries, 3)\n",
    "    extend(entries, before)\n",
    "    extend(entries, [])\n",
    "    assert before == [1, 2]\n",
    "    assert entries == [1, 2, 3, 1, 2]\n",
    "    assert grown([7], [8, 9]) == [7, 8, 9, 0]\n",
    "    assert eval(first_gathered(value: [7, _, 9])) == [7, _, 7]\n",
    "}\n",
);

/// LIST-1, LIST-2: `append` and `extend` are the provider's functions.
/// LIST-3: each gives back the list its place is to hold, a new one,
/// and the call assigns the place (FN-15).
#[test]
fn a_list_grows_through_cores_append_and_extend() {
    let emitted = emit(&[("lists.hgl", LISTS)]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let python = emitted.text("_hgl/_m_demo/_m_lists/__init__.py");
    for expected in [
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
    ] {
        assert!(
            python.contains(expected),
            "the module has:\n{expected}\nin:\n{python}"
        );
    }
}

/// A generic node whose configuration is of its generic parameter:
/// what `T` is at a call decides how the configuration is given.
const KEPT: &str = concat!(
    "module demo.tagged\n",
    "\n",
    "fn tagged<T>(value: i64, const tag: T) -> i64 {\n",
    "    when { return value }\n",
    "}\n",
    "fn optional<T>(value: i64, const tag: T? = null) -> i64 {\n",
    "    when { return value }\n",
    "}\n",
    "fn wired(value: i64, const tags: list<i64>) -> i64 {\n",
    "    return tagged<list<i64>>(value, tags)\n",
    "}\n",
    "fn defaulted(value: i64) -> i64 {\n",
    "    return optional<list<i64>>(value)\n",
    "}\n",
    "\n",
    "test kept {\n",
    "    assert eval(wired(value: [7, _, 9], tags: [1, 2])) == [7, _, 9]\n",
    "    assert eval(defaulted(value: [7])) == [7]\n",
    "    assert eval(tagged<list<i64>>(value: [7], tag: [1, 2])) == [7]\n",
    "    assert eval(tagged<dict<str, i64>>(value: [7], tag: [\"a\": 1])) == [7]\n",
    "    assert eval(tagged<i64>(value: [7], tag: 1)) == [7]\n",
    "    assert eval(optional<tuple<i64, str>>(value: [7])) == [7]\n",
    "}\n",
);

/// GEN-2: a generic temporal function is written once for each set of
/// generic arguments, and its configuration is of the type the
/// arguments make it. A list, a tuple or a dictionary is given to a
/// node inside the provider's `Held`, so a parameter declared a `T` is
/// given inside one where `T` is a list, and as it is where `T` is an
/// `i64`: by a graph's call, by an eval, and where a default stands in
/// for an argument left out.
#[test]
fn configuration_of_a_generic_type_is_held_by_what_the_type_is() {
    let emitted = emit(&[("tagged.hgl", KEPT)]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let python = emitted.text("_hgl/_m_demo/_m_tagged/__init__.py");
    for expected in [
        "    return _inst_tagged__list_i64_(value=value, tag=_core.Held(tags))\n",
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
    ] {
        assert!(
            python.contains(expected),
            "the module has:\n{expected}\nin:\n{python}"
        );
    }
}

/// LIST-3: "a value taken from the place before the call is as it
/// was". A `let`, a parameter and whatever else is given a place's
/// list is given the list itself; the place copies it before it grows
/// where something else may hold it, and grows it where it is
/// otherwise.
const SHARED: &str = concat!(
    "module demo.shared\n",
    "\n",
    "struct Holder {\n",
    "    items: list<i64>\n",
    "}\n",
    "\n",
    // A function that keeps nothing: its result is a scalar.
    "const fn total(values: list<i64>) -> i64 {\n",
    "    var sum: i64 = 0\n",
    "    for value in values { sum += value }\n",
    "    return sum\n",
    "}\n",
    // One that may keep what it is given: its result may hold it.
    "const fn same<T>(value: T) -> T { return value }\n",
    // One that does not know its place holds a list.
    "const fn keep<T>(var slot: T, value: T) { slot = value }\n",
    "const fn push(var items: list<i64>, item: i64) { append(items, item) }\n",
    // FN-15: a function that fails assigns nothing, though it grew the
    // place's list before it failed.
    "const fn failing(var items: list<i64>, zero: i64) {\n",
    "    append(items, 2)\n",
    "    let quotient = 1 //{zero: .fail} zero\n",
    "}\n",
    // A function handed a run's store may keep there what it is given.
    "const fn save(store: GlobalState, values: list<i64>) {\n",
    "    put(store, \"saved\", [values])\n",
    "}\n",
    "fn saving(value: i64) -> i64 {\n",
    "    cache seen: list<i64> = []\n",
    "    inject global_state\n",
    "    when {\n",
    "        append(seen, value)\n",
    "        save(global_state, seen)\n",
    "        append(seen, value)\n",
    "        let saved: list<list<i64>> = get(global_state, \"saved\")\n",
    "        if saved[0] == [value] { return 1 }\n",
    "        return 0\n",
    "    }\n",
    "}\n",
    // A `var` of the function's own, written from what it was lent.
    "const fn longer(values: list<i64>) -> i64 {\n",
    "    var mine: list<i64> = values\n",
    "    append(mine, 0)\n",
    "    return total(mine)\n",
    "}\n",
    "const fn built(count: i64) -> list<i64> {\n",
    "    var out: list<i64> = []\n",
    "    var i: i64 = 0\n",
    "    while i < count {\n",
    "        append(out, i)\n",
    "        i += 1\n",
    "    }\n",
    "    return out\n",
    "}\n",
    "const fn doubled() -> list<i64> {\n",
    "    var values: list<i64> = [1, 2]\n",
    "    for item in values { append(values, item) }\n",
    "    return values\n",
    "}\n",
    "\n",
    "test shared {\n",
    // A `let` is the list as it was, however the place grows after.
    "    var entries: list<i64> = [1, 2]\n",
    "    let before = entries\n",
    "    append(entries, 3)\n",
    "    assert before == [1, 2]\n",
    "    assert entries == [1, 2, 3]\n",
    "    assert total(entries) == 6\n",
    "    assert longer(entries) == 6\n",
    "    assert entries == [1, 2, 3]\n",
    // What a function gave back is as it was.
    "    let through = same(entries)\n",
    "    append(entries, 4)\n",
    "    assert through == [1, 2, 3]\n",
    // A `var` written from a `let` leaves the `let` as it was.
    "    let source = [7, 8]\n",
    "    var copy: list<i64> = source\n",
    "    push(copy, 9)\n",
    "    assert source == [7, 8]\n",
    "    assert copy == [7, 8, 9]\n",
    "    var slot: list<i64> = [0]\n",
    "    keep(slot, source)\n",
    "    append(slot, 1)\n",
    "    assert source == [7, 8]\n",
    "    assert slot == [7, 8, 1]\n",
    // A list inside another value, and one a list or a struct keeps.
    "    var rows: list<list<i64>> = [[1]]\n",
    "    let row = rows[0]\n",
    "    append(rows[0], 2)\n",
    "    append(rows, entries)\n",
    "    append(entries, 5)\n",
    "    assert row == [1]\n",
    "    assert rows == [[1, 2], [1, 2, 3, 4]]\n",
    "    let held = Holder(items: entries)\n",
    "    append(entries, 6)\n",
    "    assert held.items == [1, 2, 3, 4, 5]\n",
    "    assert doubled() == [1, 2, 1, 2]\n",
    // A place given to `extend` as both of its lists.
    "    var twice: list<i64> = [1, 2]\n",
    "    extend(twice, twice)\n",
    "    assert twice == [1, 2, 1, 2]\n",
    "    var grown: list<i64> = built(2)\n",
    "    append(grown, 9)\n",
    "    assert grown == [0, 1, 9]\n",
    "    assert built(4) == [0, 1, 2, 3]\n",
    "    var kept: list<i64> = [0]\n",
    "    append(kept, 1)\n",
    "    assert raises(\"numeric.division_by_zero\") { failing(kept, 0) }\n",
    "    assert kept == [0, 1]\n",
    "    failing(kept, 1)\n",
    "    assert kept == [0, 1, 2]\n",
    "    assert eval(saving(value: [7])) == [1]\n",
    "}\n",
);

/// LIST-3 on hgraph. A list is read from a `var` as it is, with no
/// copy. What may keep it marks it as shared, `_core.shared`: a
/// `let`, a function whose result may hold it, a value that is made
/// of it. What can keep nothing is lent it unmarked: the read of an
/// element, and a `const fn` with a scalar result and no `var`
/// parameter. `append` grows a list where it is while it is unmarked.
#[test]
fn a_list_is_shared_and_copied_only_where_it_then_grows() {
    let emitted = emit(&[("shared.hgl", SHARED)]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let python = emitted.text("_hgl/_m_demo/_m_shared/__init__.py");
    for expected in [
        // A `let` takes the list itself, marked.
        concat!(
            "    before = _core.shared(entries)\n",
            "    _given = _core.fn_append(values=entries, item=3)\n",
            "    entries = _given[1]\n",
        ),
        // Lent: a scalar result, and no `var` parameter.
        "total(values=entries)",
        "longer(values=entries)",
        "_core.eq__T_T(lhs=entries, rhs=[1, 2, 3])",
        // A `var` written from what the function was lent.
        "    mine = _core.shared(values)\n",
        // A result that may hold what it is given.
        "    through = same(value=_core.shared(entries))\n",
        // A place whose parameter is not declared a list: marked
        // where it is given, and where it is given back.
        concat!(
            "    _given = keep(slot=_core.shared(slot), value=source)\n",
            "    slot = _core.shared(_given[1])\n",
        ),
        // A place inside a value is held by that value too.
        concat!(
            "    _given = _core.given_at(rows, ((\"i\", 0),), _core.fn_append, \"values\", lambda: _py.dict(item=2))\n",
            "    rows = _given[1]\n",
        ),
        // What a list and a struct keep.
        "    _given = _core.fn_append(values=rows, item=_core.shared(entries))\n",
        "    held = Holder(items=_core.shared(entries))\n",
        // A loop runs over the list as it was.
        "    for item in _core.shared(values):\n",
        "    _given = _core.fn_extend(values=twice, items=_core.shared(twice))\n",
        // A result is the function's own list, which the caller may
        // keep.
        "    return _core.shared(out)\n",
        // FN-15: a function of the program that is given the place's
        // own list may grow it and then fail: the list is put back.
        concat!(
            "    _was_1 = _py.len(kept)\n",
            "    try:\n",
            "        _given = failing(items=kept, zero=1)\n",
            "    except _py.BaseException:\n",
            "        kept = _core.restored(kept, _was_1)\n",
            "        raise\n",
            "    kept = _given[1]\n",
        ),
        // A function handed a run's store is not lent the list.
        "save(store=_store, values=_core.shared(_state.seen))",
    ] {
        assert!(
            python.contains(expected),
            "the module has:\n{expected}\nin:\n{python}"
        );
    }
}

/// A `stop` that fails where its guard holds.
const STOP_FAILS: &str = concat!(
    "module demo.stops.fails\n",
    "fn ends_badly(value: i64) -> i64 {\n",
    "    state ended: i64 = 0\n",
    "    when { return value }\n",
    "    stop {\n",
    "        if valid(value) { ended = value // 0 }\n",
    "    }\n",
    "}\n",
    "test stop_fails { eval(ends_badly(value: [7])) }\n",
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
    let emitted = emit(&[("stops.hgl", STOPS)]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let python = emitted.text("_hgl/_m_demo/_m_stops/__init__.py");
    for expected in [
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
    ] {
        assert!(
            python.contains(expected),
            "the module has:\n{expected}\nin:\n{python}"
        );
    }
}

/// Sets: a temporal `set<i64>` and its deltas.
const SETS: &str = concat!(
    "module demo.sets\n",
    "\n",
    "fn members(command: i64) -> set<i64> {\n",
    "    when {\n",
    "        if command == 1 { return delta<set<i64>>(added: [1, 2]) }\n",
    "        if command == 2 { return delta<set<i64>>(added: [3], removed: [1]) }\n",
    "        if command == 3 { return delta<set<i64>>(removed: [2]) }\n",
    "    }\n",
    "}\n",
    "\n",
    "fn twice(command: i64) -> set<i64> {\n",
    "    inject out\n",
    "    when {\n",
    "        if command == 1 { return delta<set<i64>>(added: [1]) }\n",
    "        out = delta<set<i64>>(added: [2, 3])\n",
    "        out = delta<set<i64>>(removed: [1, 2])\n",
    "    }\n",
    "}\n",
    "\n",
    "fn changes(input: set<i64>) -> set<i64> {\n",
    "    when { return delta_value(input) }\n",
    "}\n",
    "\n",
    "fn seen(command: i64) -> set<i64> => changes(twice(command))\n",
    "\n",
    "const fn swapped(left: i64, joined: i64) -> delta<set<i64>> =>\n",
    "    delta<set<i64>>(removed: [left], added: [joined])\n",
    "\n",
    "fn relayed(enabled: bool, input: set<i64>) -> set<i64> {\n",
    "    when {\n",
    "        if enabled && modified(input) { return delta_value(input) }\n",
    "    }\n",
    "}\n",
    "\n",
    "test sets {\n",
    "    assert eval(members(command: [1, _, 2, 3])) == [\n",
    "        delta<set<i64>>(added: [1, 2]),\n",
    "        _,\n",
    "        delta<set<i64>>(added: [3], removed: [1]),\n",
    "        delta<set<i64>>(removed: [2])\n",
    "    ]\n",
    "    assert eval(members(command: [0])) == []\n",
    // What a delta adds is a set: its order is not part of it.
    "    assert eval(members(command: [1])) == [delta<set<i64>>(added: [2, 1])]\n",
    // A delta that removes an element is not one that adds it.
    "    assert eval(members(command: [1, 3])) != [\n",
    "        delta<set<i64>>(added: [1, 2]),\n",
    "        delta<set<i64>>(added: [2])\n",
    "    ]\n",
    // Two writes in one cycle are one delta.
    "    assert eval(seen(command: [1, 2])) == [\n",
    "        delta<set<i64>>(added: [1]),\n",
    "        delta<set<i64>>(added: [3], removed: [1])\n",
    "    ]\n",
    "    assert eval(relayed(enabled: [true, _, true], input: [_, delta<set<i64>>(added: [1])]))\n",
    "        == [_, delta<set<i64>>(added: [1])]\n",
    "}\n",
);

/// Set deltas that are not what the target published.
const SET_FAILS: &str = concat!(
    "module demo.sets.fails\n",
    "fn members(command: i64) -> set<i64> {\n",
    "    when {\n",
    "        if command == 1 { return delta<set<i64>>(added: [1]) }\n",
    "        if command == 2 { return delta<set<i64>>(removed: [1]) }\n",
    "    }\n",
    "}\n",
    "test removed_is_not_added {\n",
    "    assert eval(members(command: [1, 2])) == [delta<set<i64>>(added: [1]), delta<set<i64>>(added: [1])]\n",
    "}\n",
    "test one_more {\n",
    "    assert eval(members(command: [1])) == [delta<set<i64>>(added: [1]), delta<set<i64>>(removed: [1])]\n",
    "}\n",
    // The division is written first, and fails first: the addition
    // after it would overflow.
    "const fn swapped(divisor: i64, value: i64) -> delta<set<i64>> =>\n",
    "    delta<set<i64>>(removed: [1 // divisor], added: [value + 1])\n",
    "test removed_first { let change = swapped(0, 9223372036854775807) }\n",
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
    let emitted = emit(&[("sets.hgl", SETS)]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let python = emitted.text("_hgl/_m_demo/_m_sets/__init__.py");
    for expected in [
        "def members(command: _hg.TS[_py.int]) -> _hg.TSS[_py.int]:\n",
        concat!(
            "        return _hg.set_delta(added=_py.frozenset((3,)), ",
            "removed=_py.frozenset((1,)), tp=_py.int)\n",
        ),
        // A property left out is empty.
        concat!(
            "        return _hg.set_delta(removed=_py.frozenset((2,)), ",
            "added=_py.frozenset(), tp=_py.int)\n",
        ),
        // END-5: the node's own output, as hgraph names a set's.
        concat!(
            "def twice(command: _hg.TS[_py.int], _output: _hg.TSS_OUT[_py.int] = None) ",
            "-> _hg.TSS[_py.int]:\n",
        ),
        concat!(
            "    _output.value = _hg.set_delta(added=_py.frozenset((2, 3)), ",
            "removed=_py.frozenset(), tp=_py.int)\n",
        ),
        "    return input.delta_value\n",
        // EXPR-13: the properties in the order they are written.
        concat!(
            "    return _hg.set_delta(removed=_py.frozenset((left,)), ",
            "added=_py.frozenset((joined,)), tp=_py.int)\n",
        ),
        "    if not (_harness.same(_harness.evaluate(members, {\"command\": _hgl_4core._inst_replay__i64}, _hgl_4core._inst_record__set_i64_, command=[0]), [])):\n",
        "    if not (not _harness.same(_harness.evaluate(members, {\"command\": _hgl_4core._inst_replay__i64}, _hgl_4core._inst_record__set_i64_, command=[1, 3]), [",
    ] {
        assert!(
            python.contains(expected),
            "the module has:\n{expected}\nin:\n{python}"
        );
    }
}

/// A trace given to a `const fn`.
const GIVEN: &str = concat!(
    "module demo.given\n",
    "\n",
    "fn pass(value: i64) -> i64 {\n",
    "    when { return delta_value(value) }\n",
    "}\n",
    "\n",
    "const fn ticks(seen: list<tuple<datetime, i64>>) -> i64 {\n",
    "    var count = 0\n",
    "    for tick in seen { count += 1 }\n",
    "    return count\n",
    "}\n",
    "\n",
    "const fn total(seen: list<tuple<datetime, i64>>) -> i64 {\n",
    "    var sum = 0\n",
    "    for tick in seen { sum += tick[1] }\n",
    "    return sum\n",
    "}\n",
    "\n",
    // The instants are in time order, a smallest step for each
    // position apart.
    "const fn ordered(seen: list<tuple<datetime, i64>>) -> bool => seen[0][0] < seen[1][0]\n",
    "\n",
    "test given {\n",
    "    assert ticks(eval(pass(value: [7, _, 7, 9]))) == 3\n",
    "    let seen = eval(pass(value: [_, 1, _, 2]))\n",
    "    assert total(seen) == 3\n",
    "    assert ordered(seen)\n",
    "    assert ticks(eval(pass(value: [_, _]))) == 0\n",
    "}\n",
);

/// A position with no publication gives a `const fn` no tuple.
const GIVEN_FAILS: &str = concat!(
    "module demo.given.fails\n",
    "fn pass(value: i64) -> i64 {\n",
    "    when { return delta_value(value) }\n",
    "}\n",
    "const fn ticks(seen: list<tuple<datetime, i64>>) -> i64 {\n",
    "    var count = 0\n",
    "    for tick in seen { count += 1 }\n",
    "    return count\n",
    "}\n",
    "test silence_counted { assert ticks(eval(pass(value: [7, _, 7]))) == 3 }\n",
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
    let emitted = emit(&[("given.hgl", GIVEN)]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let python = emitted.text("_hgl/_m_demo/_m_given/__init__.py");
    for expected in [
        // `pass` is a word of Python's, and is written `pass_`.
        "ticks(seen=_harness.publications(_harness.evaluate(pass_, {\"value\": _hgl_4core._inst_replay__i64}, _hgl_4core._inst_record__i64, value=[7, None, 7, 9])))",
        "    seen = _harness.evaluate(pass_, {\"value\": _hgl_4core._inst_replay__i64}, _hgl_4core._inst_record__i64, value=[None, 1, None, 2])\n",
        "total(seen=_harness.publications(seen))",
    ] {
        assert!(
            python.contains(expected),
            "the module has:\n{expected}\nin:\n{python}"
        );
    }
}

/// A program with forms that are not written: a pack parameter of two
/// of its functions, and timed traces in one of its tests.
const REACH: [(&str, &str); 3] = [
    (
        "reach.hgl",
        concat!(
            "module reach\n",
            "use reach.far as far\n",
            "\n",
            "const fn fine(value: i64) -> i64 => value + 1\n",
            "const fn looped(limit: i64, rest: ...i64) -> i64 {\n",
            "    var total = 0\n",
            "    total = total + limit\n",
            "    return total\n",
            "}\n",
            "const fn outer(limit: i64) -> i64 => looped(limit) + 1\n",
            "const fn lone(value: i64) -> i64 => value\n",
            "\n",
            "test direct {\n",
            "    assert fine(1) == 2\n",
            "    assert fine(2) == 3\n",
            "}\n",
            "test reaches { assert outer(1) == 2 }\n",
            "test own { assert eval(lone(value: [0s: 1])) == [0s: 1] }\n",
            "test across { assert far::relay(1) == 2 }\n",
            "test wrong {\n",
            "    assert fine(1) == 2\n",
            "    assert fine(1) == 3\n",
            "}\n",
            "test overflows {\n",
            "    let big = 9223372036854775807\n",
            "    assert fine(big) == 0\n",
            "}\n",
            // OPS-10: a duration result outside its range. The longest
            // duration is a little over 106751991 days.
            "test too_long {\n",
            "    let long = 106751991d\n",
            "    assert long + 1d == 0s\n",
            "}\n",
            // OPS-10: an instant result outside its range, core's own
            // example.
            "test too_late {\n",
            "    assert @9999-12-31T23:59:59Z + 1d == @9999-12-31T23:59:59Z\n",
            "}\n",
        ),
    ),
    (
        "far.hgl",
        concat!(
            "module reach.far\n",
            "use reach.deep as deep\n",
            "export const fn relay(value: i64) -> i64 => deep::counted(value)\n",
        ),
    ),
    (
        "deep.hgl",
        concat!(
            "module reach.deep\n",
            "export const fn counted(limit: i64, rest: ...i64) -> i64 {\n",
            "    var total = 0\n",
            "    total = total + limit\n",
            "    return total\n",
            "}\n",
        ),
    ),
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
            "reach.hgl:5:29 target.unsupported",
            "reach.hgl:18:36 target.unsupported",
            "reach.hgl:18:49 target.unsupported",
            "deep.hgl:2:37 target.unsupported",
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
        Some(format!("deep.hgl:2:37 {pack}"))
    );
    assert_eq!(
        emitted.blocked("own"),
        Some("reach.hgl:18:36 the hgraph target does not write a timed trace yet".to_string())
    );
    let (_, direct) = emitted.test("direct");
    assert_eq!(direct.identity(), "reach::direct");

    let python = emitted.text("_hgl/_m_reach/__init__.py");
    for written in [
        "def fine(",
        "def outer(",
        "def test_direct(",
        "def test_wrong(",
    ] {
        assert!(python.contains(written), "{written} is in:\n{python}");
    }
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

/// The Python interpreter the tests that need hgraph run on.
fn python_with_hgraph() -> String {
    std::env::var("HGL_HGRAPH_PYTHON")
        .expect("HGL_HGRAPH_PYTHON names a Python interpreter that has hgraph")
}

/// Runs the tests the target writes for core's scalar eval cases and
/// for the programs of this file, on hgraph. It needs a Python that
/// has hgraph, named by the environment variable `HGL_HGRAPH_PYTHON`,
/// so it is not among the tests run by default:
/// `cargo test -p hgl_hgraph -- --ignored` runs it.
#[test]
#[ignore = "needs a Python with hgraph, named by HGL_HGRAPH_PYTHON"]
fn the_generated_tests_pass_on_hgraph() {
    let python = python_with_hgraph();
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("hgraph_generated");
    fs::remove_dir_all(&directory).ok();
    let core = core_file("tests/conformance/scalar_eval.hgl");
    let mut files = vec![
        ("scalar_eval.hgl", core.as_str()),
        ("demo.hgl", DEMO),
        ("endpoints.hgl", ENDPOINTS),
        ("selectors.hgl", SELECTORS),
        ("names.hgl", NAMES),
        ("defaults.hgl", DEFAULTS),
        CONSTANTS[0],
        CONSTANTS[1],
        ("lift.hgl", LIFTS),
        ("mixed.hgl", CANDIDATES),
        ("div.hgl", DIVISION),
        ("results.hgl", RESULTS),
        ("close.hgl", CLOSE),
        ("enums.hgl", ENUMS),
        ("durations.hgl", DURATIONS),
        ("raises.hgl", RAISES),
        ("instants.hgl", INSTANTS),
        ("divided.hgl", DIVIDED),
        ("bounded.hgl", BOUNDED),
        TEMPORAL_CANDIDATES[0],
        TEMPORAL_CANDIDATES[1],
        ("generics.hgl", GENERICS),
        ("vals.hgl", VALUE_GENERICS),
        ("through.hgl", THROUGH),
        CALLERS_TYPE[0],
        CALLERS_TYPE[1],
    ];
    files.extend(STRUCTS);
    files.extend(SAME_NAMES);
    files.push(("loops.hgl", LOOPS));
    files.push(("elements.hgl", ELEMENTS));
    files.push(("compare.hgl", COMPARE));
    files.push(("stops.hgl", STOPS));
    files.push(("caps.hgl", CAPABILITIES));
    files.push(("sources.hgl", SOURCES));
    files.push(("places.hgl", PLACES));
    files.push(("nullable.hgl", NULLABLE));
    files.push(("lists.hgl", LISTS));
    files.push(("shared.hgl", SHARED));
    files.push(("moves.hgl", MOVES));
    files.push(("entries.hgl", ENTRIES));
    files.push(("inside.hgl", INSIDE));
    files.push(("pushes.hgl", PUSHES));
    files.push(("tagged.hgl", KEPT));
    files.push(("sets.hgl", SETS));
    files.push(("given.hgl", GIVEN));
    files.push(("notified.hgl", NOTIFIED));
    files.push(("compound.hgl", COMPOUND));
    files.extend(MODULES);
    files.extend(NESTED);
    files.push(("shadow.hgl", SHADOW));
    files.extend(DEFAULT_MODULES);
    files.extend(PATHS);
    let emitted = emit(&files);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let all: Vec<usize> = (0..emitted.output.tests.len()).collect();
    let ran = run(Path::new(&python), &directory, &emitted.output, &all);
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
        71,
        "eight of core's, six on endpoints, seven on structs, two on loops, two on elements, two \
         on enums of one name, two on constants, one on durations, one on raises, one on \
         instants, one on a duration divided, one on a bounded run, one on temporal \
         candidates, one on generic temporal functions, one on value functions that \
         require, one on a caller of one, one on an instance of a caller's type, one on \
         capabilities, one on sources, one on `var` parameters, one on nullables, one on lists, one on lists that are shared, one on tenors, one on entries assigned where they are, one on collections inside values, one on candidates that take a place, one on configuration of a generic type, one on an invalidation's notification, one on a compound assignment, and one of each demo's"
    );
}

/// A program that calls no function of the provider's still has the
/// provider beside its harness, which imports it: its test loads and
/// runs. Needs hgraph, as above.
#[test]
#[ignore = "needs a Python with hgraph, named by HGL_HGRAPH_PYTHON"]
fn a_test_that_calls_no_native_loads_its_harness() {
    let python = python_with_hgraph();
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("hgraph_harness_alone");
    fs::remove_dir_all(&directory).ok();
    let emitted = emit(&[("shadow.hgl", SHADOW)]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let all: Vec<usize> = (0..emitted.output.tests.len()).collect();
    let ran = run(Path::new(&python), &directory, &emitted.output, &all);
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
    let python = python_with_hgraph();
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("hgraph_applications");
    fs::remove_dir_all(&directory).ok();
    let emitted = emit(&APPLICATIONS);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let ran = |path: &str| {
        let application = emitted
            .output
            .applications
            .iter()
            .position(|application| application.path == path)
            .unwrap_or_else(|| panic!("no application `{path}` was written"));
        run_application(
            Path::new(&python),
            &directory,
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

/// END-4: "Invalidation clears validity, modification, payload, and
/// delta. The transition to invalid notifies active subscribers." A
/// handler that subscribes to an input and does not require it valid
/// is woken by the input's invalidation, whatever else the node's
/// other handlers subscribe to; and `modified(input)` is false in that
/// cycle, in a node that injects something as in one that does not.
const NOTIFIED: &str = concat!(
    "module demo.notified\n",
    "fn produced(command: i64) -> i64 {\n",
    "    inject out\n",
    "    when {\n",
    "        if command == 0 { invalidate(out) }\n",
    "        else { return command }\n",
    "    }\n",
    "}\n",
    "fn watch_one(watched: i64, probe: bool) -> bool {\n",
    "    when modified(watched) && valid([]) { return valid(watched) }\n",
    "}\n",
    // A second handler, on an input that never ticks.
    "fn watch_two(watched: i64, probe: bool) -> bool {\n",
    "    when modified(watched) && valid([]) { return valid(watched) }\n",
    "    when modified(probe) && valid([]) { return true }\n",
    "}\n",
    // `modified` as a value, in a node that injects a capability.
    "fn was_modified(watched: i64, probe: bool) -> bool {\n",
    "    inject clock\n",
    "    when modified(watched) && valid([]) { return modified(watched) }\n",
    "}\n",
    "fn alone(command: i64, probe: bool) -> bool => watch_one(produced(command), probe)\n",
    "fn beside(command: i64, probe: bool) -> bool => watch_two(produced(command), probe)\n",
    "fn asked(command: i64, probe: bool) -> bool => was_modified(produced(command), probe)\n",
    "test an_invalidation_wakes_its_subscriber {\n",
    "    assert eval(alone(command: [7, 0, 0, 7], probe: [])) == [true, false, _, true]\n",
    "    assert eval(beside(command: [7, 0, 0, 7], probe: [])) == [true, false, _, true]\n",
    // The other handler is woken by its own input, and by no other's.
    "    assert eval(beside(command: [7, _, 0], probe: [_, true, _])) == [true, true, false]\n",
    "    assert eval(asked(command: [7, 0, 0, 7], probe: [])) == [true, false, _, true]\n",
    "}\n",
);

/// A node whose handlers are woken by different inputs keeps whether
/// each input it must see invalidated was valid when it last looked
/// (END-4), and its `modified` as a value is held to the input being
/// valid.
#[test]
fn a_handler_is_woken_by_its_inputs_invalidation() {
    let emitted = emit(&[("notified.hgl", NOTIFIED)]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let python = emitted.text("_hgl/_m_demo/_m_notified/__init__.py");
    for expected in [
        // One handler: the node is woken by what wakes it.
        concat!(
            "@_hg.compute_node(active=(\"watched\",), valid=())\n",
            "def watch_one(watched: _hg.TS[_py.int], probe: _hg.TS[_py.bool]) -> _hg.TS[_py.bool]:\n",
            "    return watched.valid\n",
        ),
        // Two: each is told what woke it.
        concat!(
            "@_hg.compute_node(valid=())\n",
            "def watch_two(watched: _hg.TS[_py.int], probe: _hg.TS[_py.bool], ",
            "_state: _hg.STATE = None) -> _hg.TS[_py.bool]:\n",
            "    _woke_watched = watched.modified or (_state._valid_watched and not watched.valid)\n",
            "    _state._valid_watched = watched.valid\n",
            "    _woke_probe = probe.modified or (_state._valid_probe and not probe.valid)\n",
            "    _state._valid_probe = probe.valid\n",
            "    if _woke_watched:\n",
            "        return watched.valid\n",
            "    if _woke_probe:\n",
            "        return True\n",
            "\n\n",
            "@watch_two.start\n",
            "def _start_watch_two(_state: _hg.STATE = None):\n",
            "    _state._valid_watched = False\n",
            "    _state._valid_probe = False\n",
        ),
        "    return (watched.modified and watched.valid)\n",
    ] {
        assert!(
            python.contains(expected),
            "the module has:\n{expected}\nin:\n{python}"
        );
    }
}

/// EXPR-3: "A compound assignment reads the place first". EXPR-2: an
/// expression written once is evaluated once: the index of a compound
/// assignment's place is the index of its read and of its write.
const COMPOUND: &str = concat!(
    "module demo.compound\n",
    // Each call gives the next number, from 0.
    "const fn next_slot(store: GlobalState) -> i64 {\n",
    "    if has(store, \"lookups\") {\n",
    "        let previous: i64 = get(store, \"lookups\")\n",
    "        put(store, \"lookups\", previous + 1)\n",
    "        return previous\n",
    "    }\n",
    "    put(store, \"lookups\", 1)\n",
    "    return 0\n",
    "}\n",
    "fn compound(trigger: i64) -> i64 {\n",
    "    inject global_state\n",
    "    when {\n",
    "        var items: list<i64> = [10, 50]\n",
    "        items[next_slot(global_state)] += 1\n",
    "        return items[0]\n",
    "    }\n",
    "}\n",
    // Two indices, each evaluated once and in the order written: the
    // row 0 and then its element 1.
    "fn nested(trigger: i64) -> i64 {\n",
    "    inject global_state\n",
    "    when {\n",
    "        var rows: list<list<i64>> = [[1, 2], [30, 40]]\n",
    "        rows[next_slot(global_state)][next_slot(global_state)] += 5\n",
    "        return rows[0][1]\n",
    "    }\n",
    "}\n",
    "test a_compound_assignment_looks_up_once {\n",
    "    assert eval(compound(trigger: [1])) == [11]\n",
    "    assert eval(nested(trigger: [1])) == [7]\n",
    "}\n",
);

/// A compound assignment's index that is more than a name is evaluated
/// once, before the statement, and read and written through what it
/// came to (EXPR-2, EXPR-3).
#[test]
fn a_compound_assignment_evaluates_its_place_once() {
    let emitted = emit(&[("compound.hgl", COMPOUND)]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let python = emitted.text("_hgl/_m_demo/_m_compound/__init__.py");
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

/// TST-17: "An integration test ... is a named test that may run in
/// real time: beside `eval` it writes `run(call, config)` (RUN-3),
/// whose value is the run's output as an eval's is."
const INTEGRATION: &str = concat!(
    "module demo.live\n",
    "fn pass(value: i64) -> i64 {\n",
    "    when { return value }\n",
    "}\n",
    "fn beats(const every: duration) -> i64 {\n",
    "    cache count: i64 = 0\n",
    "    while {\n",
    "        count += 1\n",
    "        yield every: count\n",
    "    }\n",
    "}\n",
    "const fn ticks(seen: list<tuple<datetime, i64>>) -> i64 {\n",
    "    var count = 0\n",
    "    for item in seen {\n",
    "        count += 1\n",
    "    }\n",
    "    return count\n",
    "}\n",
    // A simulation of four seconds, counted and not written dense.
    "test bounded {\n",
    "    assert ticks(eval(beats(every: 1s), RunConfig(length: 4s))) == 3\n",
    "}\n",
    // A third of a second of the host clock.
    "integration_test beats_live {\n",
    "    let seen = run(beats(every: 10ms), RunConfig(length: 300ms))\n",
    "    assert ticks(seen) > 20\n",
    "}\n",
    // What is replayed is published in the cycle it is given in, in
    // real time as in a simulation.
    "integration_test passes_through {\n",
    "    assert run(pass(value: [7, _, 9]), RunConfig(length: 100ms)) == [7, _, 9]\n",
    "}\n",
    // A run with no end goes on until its test's limit.
    "integration_test endless {\n",
    "    let seen = run(beats(every: 10ms))\n",
    "    assert ticks(seen) > 0\n",
    "}\n",
    "integration_test after {\n",
    "    assert run(pass(value: [1]), RunConfig(length: 50ms)) == [1]\n",
    "}\n",
);

/// TST-17: an integration test runs on hgraph, its `run` in real time.
/// It is held to a time limit (hhenson/hgl_spec#110): one still
/// running when its limit passes is stopped, and has failed, with the
/// statement it was at. The test after it runs as any does. Needs
/// hgraph, as above.
#[test]
#[ignore = "needs a Python with hgraph, named by HGL_HGRAPH_PYTHON"]
fn an_integration_test_runs_in_real_time_within_its_limit() {
    let python = python_with_hgraph();
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("hgraph_integration");
    fs::remove_dir_all(&directory).ok();
    let emitted = emit(&[("live.hgl", INTEGRATION)]);
    assert_eq!(emitted.reported, Vec::<String>::new());
    let all: Vec<usize> = (0..emitted.output.tests.len()).collect();
    // Each integration test is given three seconds here, where it has
    // a minute of its own: long enough for a slow host to run the ones
    // that end.
    let ran = run_within(
        Path::new(&python),
        &directory,
        &emitted.output,
        &all,
        &|test: &Test| {
            test.integration
                .then_some(std::time::Duration::from_secs(3))
        },
    );
    let described: Vec<String> = emitted
        .output
        .tests
        .iter()
        .zip(&ran)
        .map(|(test, ran)| match ran {
            Ran::Passed => format!("{} passed", test.name),
            Ran::Failed { at, message } => format!(
                "{} failed {} {message}",
                test.name,
                at.map_or("?".to_string(), |at| emitted.place(at))
            ),
            Ran::NotRun(reason) => format!("{} not run: {reason}", test.name),
        })
        .collect();
    assert_eq!(
        described,
        [
            "bounded passed",
            "beats_live passed",
            "passes_through passed",
            "endless failed live.hgl:30:9 timed out: it was still running after its limit of 3s",
            "after passed",
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
    let python = python_with_hgraph();
    let emitted = emit(&APPLICATIONS);
    let endless = emitted
        .output
        .applications
        .iter()
        .position(|application| application.path == "demo.ticks.endless")
        .expect("the application is written");
    let ran = |name: &str, stop: &Stop| {
        let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
        fs::remove_dir_all(&directory).ok();
        run_application(
            Path::new(&python),
            &directory,
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
    let python = python_with_hgraph();
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("hgraph_failures");
    fs::remove_dir_all(&directory).ok();
    let mut files = REACH.to_vec();
    files.push(("fails.hgl", DIVISION_FAILS));
    files.push(("node.hgl", RESULT_FAILS));
    files.push(("close.hgl", CLOSE_FAILS));
    files.push(("enums.hgl", ENUM_FAILS));
    files.push(("structs.hgl", STRUCT_FAILS));
    files.push(("raises.hgl", RAISES_FAILS));
    files.push(("bounded.hgl", BOUNDED_FAILS));
    files.push(("loops.hgl", LOOP_FAILS));
    files.push(("elements.hgl", ELEMENT_FAILS));
    files.push(("stops.hgl", STOP_FAILS));
    files.push(("caps.hgl", CAPABILITIES_FAIL));
    files.push(("sets.hgl", SET_FAILS));
    files.push(("given.hgl", GIVEN_FAILS));
    let emitted = emit(&files);
    let tests = [
        emitted.test("direct").0,
        emitted.test("wrong").0,
        emitted.test("overflows").0,
        emitted.test("too_long").0,
        emitted.test("too_late").0,
        emitted.test("whole_by_zero").0,
        emitted.test("strict_by_zero").0,
        emitted.test("negative_tolerance").0,
        emitted.test("no_number").0,
        emitted.test("no_name").0,
        emitted.test("given").0,
        emitted.test("omitted").0,
        emitted.test("in_order").0,
        emitted.test("unequal").0,
        emitted.test("unmatched").0,
        emitted.test("beyond").0,
        emitted.test("before").0,
        emitted.test("absent").0,
        emitted.test("outside").0,
        emitted.test("removed_is_not_added").0,
        emitted.test("one_more").0,
        emitted.test("removed_first").0,
        emitted.test("silence_counted").0,
        emitted.test("nothing_raised").0,
        emitted.test("another_code").0,
        emitted.test("nested_assertion").0,
        emitted.test("nested_raises").0,
        emitted.test("nested_then_raised").0,
        emitted.test("both_ends").0,
        emitted.test("too_dense").0,
    ];
    let ran = run(Path::new(&python), &directory, &emitted.output, &tests);
    let said: Vec<String> = ran
        .iter()
        .map(|ran| match ran {
            Ran::Passed => "passed".to_string(),
            Ran::Failed { at, message } => format!(
                "failed {} {message}",
                at.map_or("nowhere".to_string(), |at| emitted.place(at))
            ),
            Ran::NotRun(reason) => format!("not run {reason}"),
        })
        .collect();
    assert_eq!(
        said,
        [
            "passed",
            "failed reach.hgl:22:5 the assertion does not hold",
            "failed reach.hgl:26:5 HglError: numeric.out_of_range: an i64 result is outside the signed 64-bit range",
            "failed reach.hgl:30:5 HglError: numeric.out_of_range: a duration result is outside the signed 64-bit range of microseconds",
            "failed reach.hgl:33:5 HglError: numeric.out_of_range: an instant result is outside the years 1 to 9999",
            "failed fails.hgl:4:22 HglError: numeric.division_by_zero: a division by zero",
            "failed fails.hgl:5:23 HglError: numeric.division_by_zero: a division by zero",
            "failed close.hgl:3:27 HglError: the tolerance `abs` is not finite and nonnegative",
            "failed enums.hgl:5:18 HglError: `Side` has no member 9",
            "failed enums.hgl:6:16 HglError: `Side` has no member 'none'",
            // TYPE-5: "Defaults are evaluated only when omitted", and
            // "Written arguments evaluate once in source order; failure
            // prevents later arguments."
            "passed",
            "failed structs.hgl:10:16 HglError: numeric.division_by_zero: a division by zero",
            "failed structs.hgl:11:17 HglError: numeric.out_of_range: an i64 result is outside the signed 64-bit range",
            "failed structs.hgl:12:16 the assertion does not hold",
            // CTRL-6: "An unmatched selector uses default or fails."
            "failed loops.hgl:9:18 HglError: no case of the `switch` has the value 'z'",
            // EXPR-3: "An index outside a list fails where it is
            // evaluated", and ERR-6: a strict read of a key that is
            // not there.
            "failed elements.hgl:9:15 HglError: the index 2 is outside a sequence of 2",
            "failed elements.hgl:10:15 HglError: the index -1 is outside a sequence of 2",
            "failed elements.hgl:11:15 HglError: value.missing_key: the dictionary has no key 'z'",
            "failed elements.hgl:12:16 HglError: the index 5 is outside a sequence of 2",
            // TST-4: a delta that removed an element is not one that
            // added it, and a trace of one cell is not one of two.
            "failed sets.hgl:9:5 the assertion does not hold",
            "failed sets.hgl:12:5 the assertion does not hold",
            // EXPR-13: a delta's properties are evaluated as written.
            "failed sets.hgl:16:26 HglError: numeric.division_by_zero: a division by zero",
            // TST-16: a `_` of a dense trace gives a `const fn` no
            // tuple, so two publications are counted, and not three.
            "failed given.hgl:10:24 the assertion does not hold",
            // TST-10: a block that completes fails at the assertion, and
            // one that lets out another code fails where that was
            // raised, with what it says. TST-11: a failed assertion in
            // the block is the test's failure.
            "failed raises.hgl:6:5 AssertionError: the block raised nothing, and `numeric.division_by_zero` was expected",
            "failed raises.hgl:9:49 HglError: numeric.out_of_range: an i64 result is outside the signed 64-bit range",
            "failed raises.hgl:13:9 the assertion does not hold",
            // A refused error fails the inner assertion, and is no
            // match for the outer one though it has its code.
            "failed raises.hgl:18:46 HglError: numeric.division_by_zero: a division by zero",
            "passed",
            // CRUN-2: an `end` with a `length`. TST-6: a publication a
            // second after the start is not one at the start.
            "failed bounded.hgl:9:5 HglError: run.end_and_length: a run's configuration gives both an `end` and a `length`",
            "failed bounded.hgl:12:5 the assertion does not hold",
        ]
    );

    // hgraph words a node's failure itself, and the error's own line
    // is in what it says.
    let node = [emitted.test("node_fails").0];
    let ran = run(Path::new(&python), &directory, &emitted.output, &node);
    let [Ran::Failed { at, message }] = ran.as_slice() else {
        panic!("the test failed: {ran:?}");
    };
    assert_eq!(
        at.map(|at| emitted.place(at)),
        Some("node.hgl:6:19".to_string())
    );
    assert!(
        message.contains("HglError: numeric.division_by_zero: a division by zero"),
        "{message}"
    );

    // TIME-5: a `stop` reads an input under its guard. The input is
    // valid here, so the guarded statement is run, and fails.
    let stop = [emitted.test("stop_fails").0];
    let ran = run(Path::new(&python), &directory, &emitted.output, &stop);
    let [Ran::Failed { at, message }] = ran.as_slice() else {
        panic!("the test failed: {ran:?}");
    };
    assert_eq!(
        at.map(|at| emitted.place(at)),
        Some("stops.hgl:9:19".to_string())
    );
    assert!(
        message.contains("HglError: numeric.division_by_zero: a division by zero"),
        "{message}"
    );

    // CAP-6: "an absent key fails the node."
    let absent = [emitted.test("an_absent_key_fails_the_node").0];
    let ran = run(Path::new(&python), &directory, &emitted.output, &absent);
    let [Ran::Failed { at, message }] = ran.as_slice() else {
        panic!("the test failed: {ran:?}");
    };
    assert_eq!(
        at.map(|at| emitted.place(at)),
        Some("caps.hgl:7:9".to_string())
    );
    assert!(
        message.contains("HglError: the global store holds nothing under 'nothing'"),
        "{message}"
    );
}
