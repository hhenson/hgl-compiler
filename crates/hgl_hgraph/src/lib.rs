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
//! | A native candidate of core | A function of `hgl_core_native` |
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
const CORE_NATIVE: &str = include_str!("../support/hgl_core_native.py");

/// The eval harness a generated test calls.
const HARNESS: &str = include_str!("../support/hgl_harness.py");

/// The file a module is written to: `examples.tests` to
/// `examples/tests.py`. Python finds it by the same dotted name.
pub fn file_path(module: &str) -> String {
    format!("{}.py", module.replace('.', "/"))
}

/// Writes `program` for hgraph: a file for each of its modules but
/// core, whose native functions the target provides itself, and the
/// support files those modules import.
///
/// `program` is one the analyser accepted. A form this target does not
/// write yet is reported to `diagnostics`; the files are then
/// incomplete and are not to be used.
pub fn transpile(program: &Program, diagnostics: &mut Diagnostics) -> Vec<File> {
    let mut files = Vec::new();
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
            path: "hgl_core_native.py".to_string(),
            text: CORE_NATIVE.to_string(),
        });
    }
    if harness {
        files.push(File {
            path: "hgl_harness.py".to_string(),
            text: HARNESS.to_string(),
        });
    }
    files
}

/// The functions `hgl_core_native.py` provides, read from its text: the
/// name after each `def` at the start of a line, but for the private
/// ones.
pub(crate) fn provided() -> impl Iterator<Item = &'static str> {
    CORE_NATIVE
        .lines()
        .filter_map(|line| line.strip_prefix("def "))
        .filter_map(|rest| rest.split('(').next())
        .filter(|name| !name.starts_with('_'))
}
