//! The hgraph target: an analysed program to Python wiring for
//! [hgraph](https://github.com/hhenson/hgraph).
//!
//! This is a transpiler, the last stage of the pipeline. It reads the
//! analysed program and writes Python source: one file for each module,
//! and beside them the support files a generated module imports. Nothing
//! here checks the language: what reaches this stage has been checked.
//! What this target cannot yet write is reported as a `build`
//! diagnostic, and no file is then to be used.
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
//! | A native candidate of core | A function of `_hgl._core_native` |
//! | A named test | A function `test_name` |
//! | `eval(target, input: [7, _, 9])` | `_harness.evaluate(target, input=[7, None, 9])` |
//!
//! `docs/architecture.md` says what is covered and what is not.

mod emit;

use hgl_analyser::Program;
use hgl_diagnostics::Diagnostics;

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
/// starts with `_`, as no module's Python name does.
pub const CORE_NATIVE_FILE: &str = "_hgl/_core_native.py";

/// The support file that holds the eval harness.
pub const HARNESS_FILE: &str = "_hgl/_harness.py";

/// Writes `program` for hgraph: a file for each of its modules but
/// core, whose native functions the target provides itself, and the
/// support files those modules import.
///
/// `program` is one the analyser accepted. A form this target does not
/// write yet is reported to `diagnostics`; the files are then
/// incomplete and are not to be used.
pub fn transpile(program: &Program, diagnostics: &mut Diagnostics) -> Vec<File> {
    let mut files = vec![File {
        path: ROOT_FILE.to_string(),
        text: "\"\"\"A program written by the hgl compiler for hgraph.\"\"\"\n".to_string(),
    }];
    let mut natives = false;
    let mut harness = false;
    for (index, module) in program.modules.iter().enumerate() {
        if module.path == Program::CORE {
            continue;
        }
        let written = emit::module(program, index, diagnostics);
        natives |= written.natives;
        harness |= written.harness;
        files.push(File {
            path: file_path(&module.path),
            text: written.text,
        });
    }
    if natives {
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
    files
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
