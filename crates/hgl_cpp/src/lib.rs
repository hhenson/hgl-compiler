//! The cpp target: an analysed program to C++ against
//! [hgraph](https://github.com/hhenson/hgraph)'s graph, node, type and
//! runtime layers (`docs/cpp-target.md`).
//!
//! This is a transpiler, the last stage of the pipeline. It reads the
//! analysed program and writes C++ source: for each module a
//! translation unit and a header, and for each module with tests an
//! entry unit, the `main` of one executable that runs the module's
//! tests chosen when it runs. The generated code links `libhglrt`, this
//! repository's C++ runtime library ([`runtime`]), and hgraph. Building
//! and running is the build driver's, in `hgl_cli`. Nothing here checks the language: what
//! reaches this stage has been checked.
//!
//! What this target cannot yet write is reported as a `build`
//! diagnostic, and is left out of the files, with every piece of the
//! program that reaches it: C++ is compiled whole, so a function that
//! calls one that is not written is not written either. A test that
//! reaches nothing left out can still be run; one that does is listed
//! with the first form it reaches.
//!
//! How an HGL form is written, in short (`emit`'s modules say it all):
//!
//! | HGL | C++ |
//! | --- | --- |
//! | Module `demo.calc` | `modules/mod-demo-calc.h` and `modules/mod-demo-calc.cpp`, in the namespace `hgl::demo::calc::m_`, and `modules/type-demo-calc.h` of the types it declares |
//! | Its named tests | Functions in `hgl::demo::calc::m_::t_`, and `tests/test-demo-calc.cpp`, an entry unit that registers each with the runtime library's test host and runs those its command line names |
//! | Application `demo.app` (UNIT-14) | Its module's unit, with its run statement as the function `hgl::demo::app::m_::application_`, and `applications/app-demo-app.cpp`, an entry unit whose `main` is the runtime library's host of that run |
//! | `const fn total(a: i64, b: i64) -> i64` | A function `::hgraph::Int total(::hgraph::Int a, ::hgraph::Int b)`; a generic one, a function for each instance |
//! | The scalars, enums, `T?`, tuples, lists, results | hgraph's scalars, `enum class`, `std::optional`, `std::tuple`, `hglrt::list`, `hglrt::result` |
//! | Structs, sets, dictionaries, deltas | Aggregates with HGL's equality, `hglrt::set`, `hglrt::dict`, the deltas of `hglrt` and of each struct |
//! | A native candidate of core, `a + b` of two `i64` | The library's function, named by its operation and its parameters' types: `::hglrt::add__i64_i64(a, b)` |
//! | `assert condition`, `assert raises("code") { ... }` | `::hglrt::testing::check(condition)`, `::hglrt::testing::raises("code", ...)`, after `::hglrt::testing::statement()` |
//!
//! Every other form is reported as not written yet: anything temporal
//! among them. `docs/architecture.md`
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
    /// The stem of the build artifacts for this executable.
    pub stem: String,
    /// Its entry unit's file, one of [`Output::files`]: its `main`.
    pub source: String,
    /// The units it is linked from, as positions in [`Output::units`]:
    /// its module's, and each that one calls into, through the units
    /// those call into. A unit is linked whole, so each function it
    /// holds needs what it calls, whether a test calls it or not.
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

    /// How long the test may run, by the host clock, before it is
    /// stopped and fails. `None` for one with no limit.
    ///
    /// TST-17: an integration test "is time-bound: it runs within a
    /// time limit by the host clock, the duration `@timeout(5s)` gives
    /// on the line before it, or one minute where none is written."
    pub fn limit(&self) -> Option<Duration> {
        self.integration
            .then(|| self.timeout.unwrap_or(hgl_analyser::INTEGRATION_LIMIT))
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

    /// Generated statement lines and their corresponding HGL locations.
    pub fn statements(&self) -> &[(usize, Location)] {
        &self.statements
    }
}

/// An application as the target wrote it (UNIT-14): one executable,
/// which runs its run statement once.
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
    /// Its entry unit's file, one of [`Output::files`], whose `main` is
    /// the runtime library's host of its run (`hglrt/application_host.h`).
    /// `None` for one that is blocked.
    pub source: Option<String>,
    /// The units its executable is linked from, as positions in
    /// [`Output::units`]: its own module's, and each that one calls into,
    /// through those, as an [`Executable`]'s.
    pub units: Vec<usize>,
}

/// The header name relative to the modules directory.
fn header_name(module: &str) -> String {
    format!("{}.h", path_stem("mod-", module))
}

/// The types header name relative to the modules directory.
fn types_header_name(module: &str) -> String {
    format!("{}.h", path_stem("type-", module))
}

/// The header of module `module`, such as `modules/mod-demo-calc.h`.
pub fn header_path(module: &str) -> String {
    format!("modules/{}", header_name(module))
}

/// The types header of module `module`, which holds the enums and the
/// structs it declares, such as `modules/type-demo-calc.h`.
pub fn types_header_path(module: &str) -> String {
    format!("modules/{}", types_header_name(module))
}

/// The translation unit of module `module`, such as `modules/mod-demo-calc.cpp`.
pub fn unit_path(module: &str) -> String {
    format!("modules/{}.cpp", path_stem("mod-", module))
}

/// The file stem shared by a module's test entry unit, executable and results.
fn test_stem(module: &str) -> String {
    path_stem("test-", module)
}

/// The entry unit of the tests of module `module`:
/// `tests/test-demo-calc.cpp`.
pub fn entry_path(module: &str) -> String {
    format!("tests/{}.cpp", test_stem(module))
}

/// The file stem of application `path` (UNIT-14).
pub fn application_stem(path: &str) -> String {
    path_stem("app-", path)
}

/// Encodes a module or application path (UNIT-1, UNIT-2, UNIT-14).
/// Hyphens replace dots and a `+` marks each folded uppercase letter.
/// Neither character occurs in an identifier, so paths stay distinct
/// on case-insensitive filesystems. The role prefix keeps artifact
/// kinds apart and avoids reserved basenames.
fn path_stem(prefix: &str, path: &str) -> String {
    let mut stem = String::from(prefix);
    for character in path.chars() {
        match character {
            '.' => stem.push('-'),
            'A'..='Z' => {
                stem.push('+');
                stem.push(character.to_ascii_lowercase());
            }
            other => stem.push(other),
        }
    }
    stem
}

/// The entry unit of application `path`: `applications/app-demo-app.cpp`. An
/// application's path is no module's (UNIT-14), and the directory is not
/// the tests' own.
pub fn application_path(path: &str) -> String {
    format!("applications/{}.cpp", application_stem(path))
}

/// Writes `program` as C++: each module's header and unit, and an entry
/// unit for each module with a test that can be run and for each
/// application that can be.
///
/// `program` is one the analyser accepted. A form this target does not
/// write yet is reported to `diagnostics`, and the declaration or the
/// test it is in is left out of its file, with all that reaches it. The
/// files are then no whole program; each test says whether it can still
/// be run. Core is written as any module is; what core holds that the
/// target does not write is reported only where the program reaches it.
pub fn transpile(program: &Program, file_name: FileName, diagnostics: &mut Diagnostics) -> Output {
    transpile_for(program, &runtime::provides, Some(file_name), diagnostics)
}

/// The name of the file a place of the program is in, as the driver
/// prints a place: what the generated code's `#line` directives name, so
/// that a failure the C++ raises names the HGL file and line it was
/// written at (`emit`, `Emitter::line_directive`).
pub type FileName<'a> = &'a dyn Fn(Location) -> String;

/// Maps a checked location and its one-based line to the source the debugger
/// should show. An inserted statement can have no original location.
pub type SourcePosition<'a> = &'a dyn Fn(Location, usize) -> Option<(String, usize)>;

/// Writes a program with source positions supplied by the driver, including
/// mappings back to the original HGL file after command-line replacements.
pub fn transpile_with_positions(
    program: &Program,
    position: SourcePosition,
    diagnostics: &mut Diagnostics,
) -> Output {
    transpile_with(
        program,
        &runtime::provides,
        Positions {
            source: Some(position),
            tests: false,
        },
        diagnostics,
    )
}

/// Writes source positions for a native debugger, including the statements
/// of ordinary and integration tests (TST-17).
pub fn transpile_for_debugger(
    program: &Program,
    position: SourcePosition,
    diagnostics: &mut Diagnostics,
) -> Output {
    transpile_with(
        program,
        &runtime::provides,
        Positions {
            source: Some(position),
            tests: true,
        },
        diagnostics,
    )
}

/// [`transpile`], for a runtime library that defines the native
/// functions for which `provides` is true, each by the name it gives
/// it: `add__i64_i64` for `add_(i64, i64)`. A call of a native it does
/// not define is a form not written yet. The library `hgl` carries
/// defines those [`runtime::provides`] says it does; a test of the
/// emitter names its own, so that what it checks does not hang on the
/// library's progress. Where `file_name` is given, each statement the
/// target writes outside a test is preceded by a `#line` directive of its
/// HGL file and line; with none, by nothing.
pub fn transpile_for(
    program: &Program,
    provides: &dyn Fn(&str) -> bool,
    file_name: Option<FileName>,
    diagnostics: &mut Diagnostics,
) -> Output {
    let position = |at, line| file_name.map(|name| (name(at), line));
    transpile_with(
        program,
        provides,
        Positions {
            source: file_name.map(|_| &position as SourcePosition),
            tests: false,
        },
        diagnostics,
    )
}

#[derive(Clone, Copy)]
struct Positions<'a> {
    source: Option<SourcePosition<'a>>,
    tests: bool,
}

fn transpile_with(
    program: &Program,
    provides: &dyn Fn(&str) -> bool,
    positions: Positions<'_>,
    diagnostics: &mut Diagnostics,
) -> Output {
    let core = program.core();
    // GEN-2: C++ values are typed, so every generic function is written
    // once for each set of generic arguments it is called with.
    let instances = hgl_lower::instances::Instances::typed(program);
    let readers = emit::readers::Readers::of(program, &instances);
    let mut unwritten_in_core = Diagnostics::new();
    let written: Vec<emit::Written> = (0..program.modules.len())
        .map(|index| {
            if Some(index) == core {
                emit::module(
                    program,
                    index,
                    provides,
                    positions,
                    &instances,
                    &readers,
                    &mut unwritten_in_core,
                )
            } else {
                emit::module(
                    program,
                    index,
                    provides,
                    positions,
                    &instances,
                    &readers,
                    diagnostics,
                )
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
