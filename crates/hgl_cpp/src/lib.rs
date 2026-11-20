//! The cpp target: an analysed program to C++ against
//! [hgraph](https://github.com/hhenson/hgraph)'s graph, node, type and
//! runtime layers (`docs/cpp-target.md`).
//!
//! This is a transpiler, the last stage of the pipeline, beside the
//! hgraph target that writes Python. It reads the analysed program and
//! writes C++ source: for each module a translation unit and a header,
//! and for each module with tests an entry unit, the `main` of one
//! executable that runs the module's tests chosen when it runs. The
//! generated code links `libhglrt`, this repository's C++ runtime
//! library ([`runtime`]), and hgraph. Building and running is the build
//! driver's, in `hgl_cli`. Nothing here checks the language: what
//! reaches this stage has been checked.
//!
//! What this target cannot yet write is reported as a `build`
//! diagnostic, and is left out of the files, with every piece of the
//! program that reaches it: C++ is compiled whole, so a function that
//! calls one that is not written is not written either. A test that
//! reaches nothing left out can still be run; one that does is listed
//! with the first form it reaches, as the hgraph target lists it.
//!
//! How an HGL form is written:
//!
//! | HGL | C++ |
//! | --- | --- |
//! | Module `demo.calc` | `modules/demo.calc.h` and `modules/demo.calc.cpp`, in the namespace `hgl::demo::calc::m_` |
//! | Its named tests | Functions in `hgl::demo::calc::m_::t_`, and `tests/demo.calc.cpp`, an entry unit that runs those its command line names |
//! | `const fn total(a: i64, b: i64) -> i64` | A function `::hgraph::Int total(::hgraph::Int a, ::hgraph::Int b)` |
//! | `i64`, `f64`, `bool` | hgraph's `Int`, `Float`, `Bool` |
//! | A native candidate of core, `a + b` of two `i64` | The library's function named as the hgraph target names it: `::hglrt::add__i64_i64(a, b)` |
//! | `assert condition` | `::hglrt::check(condition)`, after `::hglrt::statement(__LINE__)` |
//!
//! Every other form is reported as not written yet. `docs/architecture.md`
//! says what is covered and how the target's progress is counted.

mod assemble;
mod emit;
pub mod runtime;

use std::time::Duration;

use hgl_analyser::Program;
use hgl_diagnostics::{Diagnostic, Diagnostics, Location};

/// The name `hgl emit --target` takes for this target.
pub const TARGET: &str = "cpp";

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
    /// The files: each module's header and unit, and each entry unit.
    pub files: Vec<File>,
    /// The translation units, one for each module with something
    /// written in it, in the order of the program's modules.
    pub units: Vec<Unit>,
    /// The executables, one for each module with a test that can be
    /// run, in the order of the program's modules.
    pub executables: Vec<Executable>,
    /// The named tests of the program, in the order of its modules and
    /// then as declared, those that cannot be run among them.
    pub tests: Vec<Test>,
    /// The applications of the program (UNIT-14), in the order its
    /// units are in.
    pub applications: Vec<Application>,
}

/// The translation unit of one module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit {
    /// The module's path.
    pub module: String,
    /// The unit's file, one of [`Output::files`].
    pub source: String,
    /// Its header's file, which the units that call into the module
    /// include.
    pub header: String,
}

/// One executable: the tests of one module, run as its command line
/// asks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Executable {
    /// The module whose tests it runs.
    pub module: String,
    /// Its entry unit's file, one of [`Output::files`]: its `main`.
    pub source: String,
    /// The units it is linked from, as positions in [`Output::units`]:
    /// those of the modules its tests reach, the module's own among
    /// them.
    pub units: Vec<usize>,
}

/// A named test as the target wrote it.
#[derive(Debug, Clone)]
pub struct Test {
    /// The path of its module.
    pub module: String,
    /// Its name.
    pub name: String,
    /// True for an integration test (TST-17), which may run in real
    /// time and is held to a time limit.
    pub integration: bool,
    /// The limit `@timeout(...)` gives the test, where it writes one.
    pub timeout: Option<Duration>,
    /// The first form the test needs that the target does not write
    /// yet, in the test or in a function it reaches. `None` for a test
    /// that can be run.
    pub blocked: Option<Diagnostic>,
    /// The executable that runs it, as a position in
    /// [`Output::executables`]; `None` for one that is blocked.
    pub executable: Option<usize>,
    /// Where each statement of the test starts in its module's unit:
    /// the line, counted from 1, and the place in the source. The
    /// generated code tells the runtime library the line of the
    /// statement it is running, and a failure is reported with it.
    statements: Vec<(usize, Location)>,
}

impl Test {
    /// What names the test (TST-15): `module.path::name`.
    pub fn identity(&self) -> String {
        format!("{}::{}", self.module, self.name)
    }

    /// The statement of the test that line `line` of its module's unit
    /// is part of: the last one to start at or before it.
    pub fn statement_at(&self, line: usize) -> Option<Location> {
        self.statements
            .iter()
            .rev()
            .find(|(start, _)| *start <= line)
            .map(|(_, at)| *at)
    }
}

/// An application as the target wrote it.
#[derive(Debug, Clone)]
pub struct Application {
    /// Its path.
    pub path: String,
    /// True where its run statement is `run`, a run in real time, and
    /// false for `eval`, a simulation (RUN-3).
    pub real_time: bool,
    /// The first form the application needs that the target does not
    /// write yet. `None` for one that can be run.
    pub blocked: Option<Diagnostic>,
}

/// The header of module `module`: `demo.calc` is `modules/demo.calc.h`.
/// A module's path is unique in a program, and is made of identifiers
/// and dots, so it is a file name as it is.
pub fn header_path(module: &str) -> String {
    format!("modules/{module}.h")
}

/// The translation unit of module `module`: `modules/demo.calc.cpp`.
pub fn unit_path(module: &str) -> String {
    format!("modules/{module}.cpp")
}

/// The entry unit of the tests of module `module`:
/// `tests/demo.calc.cpp`.
pub fn entry_path(module: &str) -> String {
    format!("tests/{module}.cpp")
}

/// Writes `program` as C++: each module's header and unit, and an entry
/// unit for each module with a test that can be run.
///
/// `program` is one the analyser accepted. A form this target does not
/// write yet is reported to `diagnostics`, and the declaration or the
/// test it is in is left out of its file, with all that reaches it. The
/// files are then no whole program; each test says whether it can still
/// be run. Core is written as any module is; what core holds that the
/// target does not write is reported only where the program reaches it.
pub fn transpile(program: &Program, diagnostics: &mut Diagnostics) -> Output {
    transpile_for(program, &runtime::provides, diagnostics)
}

/// [`transpile`], for a runtime library that defines the native
/// functions for which `provides` is true, each by the name it gives
/// it: `add__i64_i64` for `add_(i64, i64)`. A call of a native it does
/// not define is a form not written yet. The library `hgl` carries
/// defines those [`runtime::provides`] says it does; a test of the
/// emitter names its own, so that what it checks does not hang on the
/// library's progress.
pub fn transpile_for(
    program: &Program,
    provides: &dyn Fn(&str) -> bool,
    diagnostics: &mut Diagnostics,
) -> Output {
    let core = program.core();
    let mut unwritten_in_core = Diagnostics::new();
    let written: Vec<emit::Written> = (0..program.modules.len())
        .map(|index| {
            if Some(index) == core {
                emit::module(program, index, provides, &mut unwritten_in_core)
            } else {
                emit::module(program, index, provides, diagnostics)
            }
        })
        .collect();
    let (output, blocking) = assemble::output(program, &written);
    // A piece of the program that reaches a piece of core's that is not
    // written is reported with what that piece lacks, once.
    for diagnostic in blocking {
        if unwritten_in_core.as_slice().contains(&diagnostic)
            && !diagnostics.as_slice().contains(&diagnostic)
        {
            diagnostics.emit(diagnostic);
        }
    }
    output
}
