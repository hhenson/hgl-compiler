//! The hgraph target: an analysed program to Python wiring for
//! [hgraph](https://github.com/hhenson/hgraph).
//!
//! This is a transpiler, the last stage of the pipeline. It reads the
//! analysed program and writes Python source: one file for each module,
//! and beside them the support files a generated module imports. Nothing
//! here checks the language: what reaches this stage has been checked.
//! What this target cannot yet write is reported as a `build`
//! diagnostic, and is left out of the files: they are then no whole
//! program. A named test that reaches nothing left out can still be
//! run ([`run`]), which is how `hgl test` runs a program on hgraph.
//!
//! How an HGL form is written for hgraph:
//!
//! | HGL | Python |
//! | --- | --- |
//! | `const fn` | A plain function |
//! | A node, `fn` with a `when` | `@_hg.compute_node`, or `@_hg.sink_node` with no result |
//! | A temporal parameter | `name: _hg.TS[type]`; its payload is `name.value` |
//! | A `const` parameter of a node | A scalar parameter |
//! | `state name: T = value` | A field of the node's `STATE`, set in its `start` |
//! | `valid(x)`, `modified(x)`, `delta_value(x)` | `x.valid`, `x.modified`, `x.delta_value` |
//! | A temporal `set<i64>` | `_hg.TSS[_py.int]`, whose tick is the set's delta |
//! | `delta<set<i64>>(added: [1], removed: [2])` | `_hg.set_delta(added=..., removed=..., tp=_py.int)` |
//! | A native candidate of core | A function of `_hgl._core_native` |
//! | A `state` and a `cache` | Fields of the node's `STATE`, given their values in its `start` |
//! | A source, a `fn` with a `yield` | A Python generator, a node that resumes it, and a graph that wakes the node as it starts |
//! | `inject clock`, `logger`, `global_state` | hgraph's `EvaluationClock`, `LOGGER` and `GlobalState`, given to the node, its `start` and its `stop` |
//! | `impl const fn` with a body | A function, `_impl_add__f64_i64` |
//! | `impl fn` with a body | A node or a graph, `_impl_join__i64_i64` |
//! | A temporal function or candidate with generic parameters | A node or a graph for each set of generic arguments it is called with, `_inst_identity_sum__f64` |
//! | A value function with generic parameters that calls what it requires | A function for each set of generic arguments, `_inst_twice__i64` |
//! | `enum Side { buy, sell }` | `Side = _enum.Enum("demo.orders::Side", [...])`, a scalar to hgraph |
//! | `struct Point { x: f64, y: f64 }` | `Point = _core.struct("Point", ("x", "y"))`, a class whose values are never changed |
//! | `Point(x: 1.0)`, `point.x = 2.0` | `Point(x=1.0, y=...)`, `point = _core.with_field(point, x=2.0)` |
//! | `for`, `while` and `switch` in a value function or a hook | `for`, `while`, and an `if` for each case |
//! | `items[1]`, `sizes[name]`, `items[1] = 2` | `_core.item(items, 1)`, `_core.entry(sizes, name)`, `items = _core.with_item(items, 1, 2)` |
//! | `ok(v)`, `err(e)`, `r?` | `_core.Ok(v)`, `_core.Err(e)`, `_core.propagated(r)` |
//! | A value function given a connection in a graph | A node of its own that calls it (FN-6) |
//! | A named test | A function `test_name` |
//! | `assert condition` | `if not (condition): raise _py.AssertionError()` |
//! | `eval(target(input: [7, _, 9]))` | `_harness.evaluate(target, input=[7, None, 9])` |
//! | `eval(target(input: [7]), RunConfig(length: 2us))` | `_harness.evaluate(target, input=[7], _config=...)` |
//! | `assert raises("code") { ... }` | A `try` whose handler is `_harness.raised(error, "code")` |
//! | `eval(...) == [...]` | `_harness.same(left, right)` |
//! | A trace given to a `const fn` | `_harness.publications(trace)` |
//!
//! `docs/architecture.md` says what is covered and what is not.

mod emit;
mod host;
mod instances;

use std::collections::BTreeSet;

use hgl_analyser::Program;
use hgl_diagnostics::{Diagnostic, Diagnostics, Location};

pub use host::{Ran, run, write};

/// The name `hgl emit --target` takes for this target.
pub const TARGET: &str = "hgraph";

/// The codes of this target's own diagnostics. They are local to the
/// compiler: the specification catalogues what source is rejected, and
/// what a target can write is not the language's to say.
pub mod codes {
    /// A form the target does not write yet.
    pub const UNSUPPORTED: &str = "target.unsupported";
    /// Every code of this target.
    pub const ALL: [&str; 1] = [UNSUPPORTED];
}

/// One file the target writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct File {
    /// Where it goes, relative to the output directory, with `/`
    /// between its parts on every system.
    pub path: String,
    /// Its text.
    pub text: String,
}

/// What the target wrote for a program.
#[derive(Debug, Clone)]
pub struct Output {
    /// The files: the root package's, one for each module, core's
    /// when a module imports it,
    /// and the support files those import.
    pub files: Vec<File>,
    /// The named tests of the program, in the order of its modules and
    /// then as declared.
    pub tests: Vec<Test>,
}

/// A named test as the target wrote it.
#[derive(Debug, Clone)]
pub struct Test {
    /// The path of its module.
    pub module: String,
    /// Its name.
    pub name: String,
    /// The first form the test needs that the target does not write
    /// yet, in the test or in a function it reaches. `None` for a test
    /// that can be run.
    pub blocked: Option<Diagnostic>,
    /// Where each statement of the test starts in its module's file:
    /// the line, counted from 1, and the place in the source.
    statements: Vec<(usize, Location)>,
}

impl Test {
    /// What names the test (TST-15): `module.path::name`.
    pub fn identity(&self) -> String {
        format!("{}::{}", self.module, self.name)
    }

    /// The statement of the test that line `line` of its module's file
    /// is part of: the last one to start at or before it.
    pub fn statement_at(&self, line: usize) -> Option<Location> {
        self.statements
            .iter()
            .rev()
            .find(|(start, _)| *start <= line)
            .map(|(_, at)| *at)
    }
}

/// The Python function a named test is written as. No function of a
/// program is: a name that starts `test_` is written with `hgl_`
/// before it.
pub fn test_function(name: &str) -> String {
    format!("test_{name}")
}

/// Core's native value functions for this target, as Python.
const CORE_NATIVE: &str = include_str!("../support/core_native.py");

/// The eval harness a generated test calls.
const HARNESS: &str = include_str!("../support/harness.py");

/// The Python package every file of a program is written under. A
/// program's modules are then in a namespace of their own: a module
/// named `hgraph`, or as a module of Python's own library is, takes
/// the place of nothing Python or hgraph imports.
pub const ROOT: &str = "_hgl";

/// The Python name of a module: its path under [`ROOT`], each part
/// with `_m_` before it. `examples.tests` is
/// `_hgl._m_examples._m_tests`.
///
/// A module's package holds its declarations and the packages of the
/// modules under it, side by side as attributes. The `_m_` keeps the
/// two apart: no declaration's Python name starts with `_`, and no
/// name the target makes up starts with `_m_`. So `module demo` may
/// declare a `util` beside `module demo.util`.
pub fn python_module(module: &str) -> String {
    let parts: Vec<String> = module.split('.').map(|part| format!("_m_{part}")).collect();
    format!("{ROOT}.{}", parts.join("."))
}

/// The file a module is written to: `examples.tests` to
/// `_hgl/_m_examples/_m_tests/__init__.py`. Each module is a Python
/// package, so that one module's path may begin another's: `demo` and
/// `demo.util` are `_hgl/_m_demo/__init__.py` and
/// `_hgl/_m_demo/_m_util/__init__.py`.
pub fn file_path(module: &str) -> String {
    format!("{}/__init__.py", python_module(module).replace('.', "/"))
}

/// The file that makes [`ROOT`] a package.
pub const ROOT_FILE: &str = "_hgl/__init__.py";

/// The support file that provides core's native functions. Its name
/// starts with `_`, as no module's Python name does. It is written when
/// a module calls one of its functions, and with the harness, which
/// imports it.
pub const CORE_NATIVE_FILE: &str = "_hgl/_core_native.py";

/// The support file that holds the eval harness. It is written when a
/// test calls it.
pub const HARNESS_FILE: &str = "_hgl/_harness.py";

/// Writes `program` for hgraph: a file for each of its modules, and
/// the support files those modules import. Core is a module as any
/// other, for what it has an HGL body for; its native functions the
/// target provides itself, and its file is written when a module of
/// the program imports it.
///
/// `program` is one the analyser accepted. A form this target does not
/// write yet is reported to `diagnostics`, and the declaration or the
/// test it is in is left out of its file. The files are then no whole
/// program; each test says whether it can still be run.
pub fn transpile(program: &Program, diagnostics: &mut Diagnostics) -> Output {
    let mut files = vec![File {
        path: ROOT_FILE.to_string(),
        text: "\"\"\"A program written by the hgl compiler for hgraph.\"\"\"\n".to_string(),
    }];
    let mut natives = false;
    let mut harness = false;
    // What was written for each module, at the module's position in
    // the program.
    let mut modules: Vec<Option<emit::Written>> = Vec::new();
    // Core is written as any module is, for the functions and the
    // candidates it has an HGL body for; its native ones are the
    // provider's. What core holds that the target does not write is
    // the program's to report only where the program reaches it.
    let mut core_file = None;
    let mut unwritten_in_core = Diagnostics::new();
    // GEN-2: the instances of generic temporal declarations the program
    // asks for, found before any module is written: a call in one
    // module asks for an instance written in another.
    let instances = instances::Instances::of(program);
    for (index, module) in program.modules.iter().enumerate() {
        if module.path == Program::CORE {
            let mut written = emit::module(program, index, &instances, &mut unwritten_in_core);
            core_file = Some(File {
                path: file_path(&module.path),
                text: std::mem::take(&mut written.text),
            });
            modules.push(Some(written));
            continue;
        }
        let mut written = emit::module(program, index, &instances, diagnostics);
        natives |= written.natives;
        harness |= written.harness;
        files.push(File {
            path: file_path(&module.path),
            text: std::mem::take(&mut written.text),
        });
        modules.push(Some(written));
    }
    // Core's file is written when a module of the program imports it.
    let core = program
        .modules
        .iter()
        .position(|module| module.path == Program::CORE);
    let imported = modules.iter().enumerate().any(|(index, written)| {
        Some(index) != core
            && written
                .as_ref()
                .is_some_and(|written| written.imports.contains(Program::CORE))
    });
    if let (true, Some(file), Some(core)) = (imported, core_file, core) {
        files.push(file);
        if let Some(written) = &modules[core] {
            natives |= written.natives;
            harness |= written.harness;
        }
    }
    // A piece of the program that was written, and reaches one of
    // core's that was not, is reported with what core's piece lacks.
    let mut reached: Vec<Diagnostic> = Vec::new();
    for (index, written) in modules.iter().enumerate() {
        let Some(written) = written.as_ref().filter(|_| Some(index) != core) else {
            continue;
        };
        let instances = written.instances.iter().map(|(_, _, piece)| piece);
        for piece in written
            .declarations
            .iter()
            .chain(instances)
            .chain(&written.tests)
        {
            if piece.broken.is_some() {
                continue;
            }
            if let Some(diagnostic) = blocked(&modules, piece)
                && unwritten_in_core.as_slice().contains(&diagnostic)
                && !reached.contains(&diagnostic)
            {
                diagnostics.emit(diagnostic.clone());
                reached.push(diagnostic);
            }
        }
    }
    let mut tests = Vec::new();
    for (index, module) in program.modules.iter().enumerate() {
        let Some(written) = modules[index].as_ref().filter(|_| Some(index) != core) else {
            continue;
        };
        for (test, piece) in module.tests.iter().zip(&written.tests) {
            tests.push(Test {
                module: module.path.clone(),
                name: test.name.clone(),
                blocked: blocked(&modules, piece),
                statements: piece.statements.clone(),
            });
        }
    }
    // The harness imports the provider: it knows an HGL error by the
    // provider's class (TST-10). So the provider is written with it,
    // whether or not a module calls a function of the provider's.
    if natives || harness {
        files.push(File {
            path: CORE_NATIVE_FILE.to_string(),
            text: CORE_NATIVE.to_string(),
        });
    }
    if harness {
        files.push(File {
            path: HARNESS_FILE.to_string(),
            text: HARNESS.to_string(),
        });
    }
    Output { files, tests }
}

/// The first form `test` needs that the target did not write: one of
/// its own, or of a function it reaches through the functions it
/// names. `modules` is what was written for each module of the
/// program.
fn blocked(modules: &[Option<emit::Written>], test: &emit::Piece) -> Option<Diagnostic> {
    if let Some(diagnostic) = &test.broken {
        return Some(diagnostic.clone());
    }
    // What is still to look at, the next one last, and what was looked
    // at: a function may be reached twice, or reach itself.
    let mut waiting: Vec<emit::Used> = test.uses.iter().rev().cloned().collect();
    let mut seen = BTreeSet::new();
    while let Some(used) = waiting.pop() {
        if !seen.insert(used.clone()) {
            continue;
        }
        let (module, declaration, instance) = used;
        let Some(written) = modules.get(module).and_then(Option::as_ref) else {
            continue;
        };
        // The declaration's own piece. One written by instance has
        // nothing in it, and is still left out where the declaration
        // itself is not written: one of a test context, say.
        if let Some(piece) = written.declarations.get(declaration) {
            if let Some(diagnostic) = &piece.broken {
                return Some(diagnostic.clone());
            }
            waiting.extend(piece.uses.iter().rev().cloned());
        }
        // GEN-2: the instance named, which is a piece of its own.
        let piece = instance.and_then(|suffix| {
            written
                .instances
                .iter()
                .find(|(of, named, _)| *of == declaration && *named == suffix)
                .map(|(_, _, piece)| piece)
        });
        if let Some(piece) = piece {
            if let Some(diagnostic) = &piece.broken {
                return Some(diagnostic.clone());
            }
            waiting.extend(piece.uses.iter().rev().cloned());
        }
    }
    None
}

/// The functions the provider of core's natives defines, read from its text: the
/// name after each `def` at the start of a line, but for the private
/// ones.
pub(crate) fn provided() -> impl Iterator<Item = &'static str> {
    CORE_NATIVE
        .lines()
        .filter_map(|line| line.strip_prefix("def "))
        .filter_map(|rest| rest.split('(').next())
        .filter(|name| !name.starts_with('_'))
}
