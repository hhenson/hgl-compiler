//! The hgraph target against small programs: the Python it writes,
//! what it reports for a form it does not write yet, and which tests
//! can then still be run.

use std::fs;
use std::path::Path;

use hgl_analyser::{ParsedFile, analyse};
use hgl_diagnostics::{Diagnostics, Location, render};
use hgl_hgraph::{Output, Ran, Test, run, transpile};
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
    "    seen = _harness.evaluate(wired, value=[1, None, 2])\n",
    "    if not (_harness.same(seen, [1, None, 12])):\n",
    "        raise _py.AssertionError()\n",
    "    if not (not _harness.same(_harness.evaluate(scaled, value=[5], factor=2), [])):\n",
    "        raise _py.AssertionError()\n",
    "    _harness.evaluate(sink, value=[True])\n",
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
        "    if (a.valid and a.modified):\n        return a.delta_value\n",
        "    _state.count = _core.add__i64_i64(lhs=_state.count, rhs=1)\n",
        "def _start_count_publications(_state: _hg.STATE = None):\n    _state.count = 0\n",
        "    if not (_harness.same(_harness.evaluate(pass_i64, value=[None, 7, None, None]), [None, 7])):\n",
        "        raise _py.AssertionError()\n",
        "    if not (_harness.same(_harness.evaluate(pass_str, value=[\"\", None, \"\", \"a\"]), [\"\", None, \"\", \"a\"])):\n",
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
        "    if not (_harness.same(_harness.evaluate(offset, value=[1], by=10), [11])):\n",
        "        raise _py.AssertionError()\n",
        "    if not (_harness.same(_harness.evaluate(offset, value=[1], by=1), [2])):\n",
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
        "    if not (_harness.same(_harness.evaluate(doubled, value=[4]), [9])):\n",
        "        raise _py.AssertionError()\n",
        // An input named as the harness's own parameter is.
        "    if not (_harness.same(_harness.evaluate(pass_, target=[1]), [1])):\n",
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
/// RUN-1, UNIT-14, TST-17: an eval's run configuration, a run in real
/// time, and an application's run statement, are not written yet. Each
/// is reported where it is written, and what the application declares
/// is written all the same.
#[test]
fn a_run_configuration_and_an_application_are_reported() {
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
    assert_eq!(
        emitted.reported,
        [
            "m.hgl:7:35 target.unsupported",
            "m.hgl:10:12 target.unsupported",
            "live.hgl:1:13 target.unsupported",
        ]
    );
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
            "    inject clock\n",
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
            "m.hgl:9:10 target.unsupported",
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
            "    if not (_harness.same(_harness.evaluate(_lift_12_doubled, value=[1, None, 3]), [2, None, 6])):\n",
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
    // core's.
    let emitted = emit(&[("demo.hgl", DEMO)]);
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
        "spans.hgl",
        concat!(
            "module demo.spans\n",
            "const fn twice(length: duration) -> duration => 2 * length\n",
            // A duration with no literal, which the target would
            // report in the program itself. It is written, not run.
            "const fn pause() -> duration => twice(pause())\n",
            "const fn fine(value: i64) -> i64 => value + 1\n",
            "test doubles { assert fine(1) == 2 }\n",
            "test spans { assert twice(pause()) == pause() }\n",
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
        .blocked("spans")
        .expect("the test reaches core's candidate");
    assert!(
        blocked
            .ends_with("the hgraph target does not write the candidate `mul_(duration, i64)` yet"),
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
            "_harness.same(_harness.evaluate(flipped, side=[Side[\"buy\"], None, Side[\"sell\"], ",
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
        // A compound assignment reads the place first.
        concat!(
            "    next = _core.with_item(next, 1, ",
            "_core.add__i64_i64(lhs=_core.item(next, 1), rhs=1))\n",
        ),
        "    next = _core.with_item(next, 1, \"b\")\n",
        "            seen = _core.with_entry(seen, name, 1)\n",
        // An element of a field, and an element of an element.
        "    next = _core.with_field(next, sizes=_core.with_entry(next.sizes, \"top\", 1))\n",
        "    rows = _core.with_item(rows, 1, _core.with_item(_core.item(rows, 1), 0, 9))\n",
        // A `state` that is a dictionary.
        "        _state.seen = _core.with_entry(_state.seen, name.value, 1)\n",
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
        "    if not (_harness.same(_harness.evaluate(members, command=[0]), [])):\n",
        "    if not (not _harness.same(_harness.evaluate(members, command=[1, 3]), [",
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
        "ticks(seen=_harness.publications(_harness.evaluate(pass_, value=[7, None, 7, 9])))",
        "    seen = _harness.evaluate(pass_, value=[None, 1, None, 2])\n",
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
        ("lift.hgl", LIFTS),
        ("mixed.hgl", CANDIDATES),
        ("div.hgl", DIVISION),
        ("results.hgl", RESULTS),
        ("close.hgl", CLOSE),
        ("enums.hgl", ENUMS),
    ];
    files.extend(STRUCTS);
    files.extend(SAME_NAMES);
    files.push(("loops.hgl", LOOPS));
    files.push(("elements.hgl", ELEMENTS));
    files.push(("compare.hgl", COMPARE));
    files.push(("stops.hgl", STOPS));
    files.push(("sets.hgl", SETS));
    files.push(("given.hgl", GIVEN));
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
        46,
        "eight of core's, six on endpoints, seven on structs, two on loops, two on elements, two \
         on enums of one name, and one of each demo's"
    );
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
    files.push(("loops.hgl", LOOP_FAILS));
    files.push(("elements.hgl", ELEMENT_FAILS));
    files.push(("stops.hgl", STOP_FAILS));
    files.push(("sets.hgl", SET_FAILS));
    files.push(("given.hgl", GIVEN_FAILS));
    let emitted = emit(&files);
    let tests = [
        emitted.test("direct").0,
        emitted.test("wrong").0,
        emitted.test("overflows").0,
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
            "failed reach.hgl:26:5 HglError: an i64 result is outside the signed 64-bit range",
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
            "failed structs.hgl:11:17 HglError: an i64 result is outside the signed 64-bit range",
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
}
