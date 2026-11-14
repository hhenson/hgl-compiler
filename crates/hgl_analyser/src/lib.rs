//! Stage 3 of the compiler: analysis.
//!
//! ```text
//! (parser) --> syntax trees --[analyse]--> analysed program --> (optimiser, transpilers)
//! ```
//!
//! The parser says what the source *is*: this is a function, that is an
//! import. The analyser says what it *means*, and rejects source that
//! means nothing: a name bound twice, an import of a declaration that is
//! not there. It works on a whole program, every file of every module at
//! once, because the answers cross files.
//!
//! # What is built so far
//!
//! The first pass: the program's modules and what each declares.
//!
//! * `assemble` groups the files into modules by the `module` line of
//!   each (UNIT-1);
//! * `declare` lists what each module declares, builds its declaration
//!   scope and its test scope, and rejects a name declared twice (UNIT-5,
//!   UNIT-12, TST-1, TST-2, NATIVE-2);
//! * `imports` finds what each `use` line names, and rejects a missing
//!   or private declaration and a cycle of imports (UNIT-2, UNIT-3), then
//!   finds the operator of each `impl fn` (GEN-4).
//!
//! Function bodies, types and constants are not read yet. Resolving the
//! names inside them is the next pass.
//!
//! # The prelude
//!
//! Every module sees the public declarations of core's mandatory module
//! without importing them (UNIT-11). Core has not declared that module
//! yet. Until it does, a name that nothing in the program binds is not
//! reported: it is listed in [`Module::prelude_names`], as a name the
//! prelude is expected to bind.
//!
//! # Like every stage
//!
//! [`analyse`] reads the parser's output and the diagnostics sink and
//! nothing else, reports every problem it finds, and gives a program for
//! any input: a declaration the parser could not read whole is left out
//! and the rest is analysed.

mod assemble;
mod declare;
mod imports;
mod program;

use hgl_diagnostics::{Category, Code, Diagnostic, Diagnostics, Location};
use hgl_parser::SyntaxNode;
use hgl_source::{FileId, Span};

pub use program::{
    Declaration, DeclarationKind, Import, ImportKind, Module, ModuleFile, OperatorRef, PreludeName,
    Program, Role, Test,
};

/// The diagnostic codes this stage reports.
///
/// The specification's catalogue has no code yet for a module or name
/// rejection, so every one of these is local to this compiler. A local
/// code cannot satisfy an `expect-error` annotation (TST-12).
pub mod codes {
    /// A module has a second unnamed file (UNIT-1).
    pub const MODULE_DUPLICATE_UNNAMED: &str = "module.duplicate_unnamed";
    /// Two files of a module have the same part name (UNIT-1).
    pub const MODULE_DUPLICATE_PART: &str = "module.duplicate_part";
    /// A `use` line stands after a declaration of its file (UNIT-2).
    pub const IMPORT_MISPLACED: &str = "import.misplaced";
    /// A `use` line names a module that no file declares (UNIT-2).
    pub const IMPORT_UNKNOWN_MODULE: &str = "import.unknown_module";
    /// A selective import names nothing the module declares (UNIT-2).
    pub const IMPORT_UNKNOWN_NAME: &str = "import.unknown_name";
    /// A selective import names a declaration that is not public
    /// (UNIT-2, UNIT-3).
    pub const IMPORT_PRIVATE: &str = "import.private";
    /// Modules import one another (UNIT-3).
    pub const IMPORT_CYCLE: &str = "import.cycle";
    /// A binding takes a name that is already bound where it stands
    /// (UNIT-5, UNIT-12, TST-2, NATIVE-2).
    pub const NAME_DUPLICATE: &str = "name.duplicate";
    /// Two tests of a module have the same name (TST-2).
    pub const TEST_DUPLICATE_NAME: &str = "test.duplicate_name";
    /// A declaration in an unnamed test context is marked `export`
    /// (TST-1).
    pub const TEST_EXPORT: &str = "test.export";
    /// An `impl fn` stands in an unnamed test context (TST-1).
    pub const TEST_IMPLEMENTATION: &str = "test.implementation";
    /// An `impl fn` names no operator (GEN-4).
    pub const IMPL_UNKNOWN_OPERATOR: &str = "impl.unknown_operator";
    /// A native implementation completes no declaration (NATIVE-2).
    pub const NATIVE_MISSING_DECLARATION: &str = "native.missing_declaration";
}

/// One parsed file: what the analyser is given for each source file.
#[derive(Debug, Clone)]
pub struct ParsedFile {
    /// The file.
    pub file: FileId,
    /// The root of its syntax tree, as `hgl_parser::parse` gives it for a
    /// whole file.
    pub root: SyntaxNode,
    /// Declarations and imports to leave out, each named by the bytes
    /// its node covers. The program is analysed as if they were not
    /// written.
    ///
    /// This is how a test runner checks a rejection case: the
    /// declaration that is expected to be rejected is left out of the
    /// program that must be accepted (TST-13).
    pub omit: Vec<Span>,
}

impl ParsedFile {
    /// A file with nothing left out.
    pub fn new(file: FileId, root: SyntaxNode) -> ParsedFile {
        ParsedFile {
            file,
            root,
            omit: Vec::new(),
        }
    }
}

/// Analyses a program: every file of every module it is made of.
///
/// The files may be given in any order; the result does not depend on it
/// (UNIT-1). A file whose `module` line the parser could not read belongs
/// to no module and is left out; the parser has reported it.
pub fn analyse(files: &[ParsedFile], diagnostics: &mut Diagnostics) -> Program {
    // `omits[m][f]` is the `omit` list of file `f` of module `m`.
    let (mut program, omits) = assemble::assemble(files, diagnostics);
    for (module, omits) in program.modules.iter_mut().zip(&omits) {
        declare::declare(module, omits, diagnostics);
    }
    imports::resolve(&mut program, diagnostics);
    program
}

/// An error this stage reports: every one has a local code and cites the
/// rule that requires it.
fn error(
    category: Category,
    code: &'static str,
    at: Location,
    message: String,
    rule: &'static str,
) -> Diagnostic {
    Diagnostic::error(category, Code::Local(code), at, message).with_rule(rule)
}
