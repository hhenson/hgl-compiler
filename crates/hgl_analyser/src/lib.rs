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
//! The program's modules, what each declares, and what each name in it
//! is bound to.
//!
//! * `assemble` groups the files into modules by the `module` line of
//!   each (UNIT-1);
//! * `declare` lists what each module declares, builds its declaration
//!   scope and its test scope, and rejects a name declared twice (UNIT-5,
//!   UNIT-12, TST-1, TST-2, NATIVE-2);
//! * `imports` finds what each `use` line names, and rejects a missing
//!   or private declaration and a cycle of imports (UNIT-2, UNIT-3), then
//!   finds the operator of each `impl fn` (GEN-4);
//! * `resolve` walks every signature, body, type and constraint, finds
//!   the one binding each name has in scope (UNIT-10), and rejects a
//!   binding that takes a name already there (UNIT-12);
//! * `forms` rejects the forms of a declaration that the grammar admits
//!   and a rule does not: a pack with a default, a hook a native
//!   implementation cannot have (FN-8, NATIVE-3).
//!
//! Types, phases and constants are not checked yet.
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
mod forms;
mod imports;
mod program;
mod resolve;

use hgl_diagnostics::{Category, Code, Diagnostic, Diagnostics, Location};
use hgl_parser::SyntaxNode;
use hgl_source::{FileId, Span};

pub use program::{
    Binding, Decision, Declaration, DeclarationKind, Import, ImportKind, LocalKind, Module,
    ModuleFile, NameUse, OperatorRef, PreludeName, Program, Role, Test,
};

/// The diagnostic codes this stage reports.
///
/// Every one is a code of the specification's diagnostic catalogue, so an
/// `expect-error` annotation can expect it (TST-12). The analyser's tests
/// check each diagnostic's category and code against the catalogue.
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
    /// A local is used before the statement that declares it (UNIT-6).
    pub const NAME_USED_BEFORE_DECLARATION: &str = "name.used_before_declaration";
    /// A declaration outside the tests uses one declared for them
    /// (TST-2).
    pub const NAME_TEST_ONLY: &str = "name.test_only";
    /// An anonymous function uses a `var`, `state`, `cache` or injected
    /// capability of the function around it (FN-14).
    pub const NAME_CAPTURED: &str = "name.captured";
    /// A default reads a parameter of its declaration or a field of its
    /// struct (UNIT-7).
    pub const DEFAULT_FORMAL_ARGUMENT: &str = "default.formal_argument";
    /// A module alias is used without `::name` after it (UNIT-2).
    pub const ALIAS_UNQUALIFIED: &str = "alias.unqualified";
    /// The name before `::` is not a module alias of its file (UNIT-2).
    pub const ALIAS_UNKNOWN: &str = "alias.unknown";
    /// A generic type's name stands before a `<` that does not open its
    /// arguments (GEN-14).
    pub const GENERIC_INVALID_APPLICATION: &str = "generic.invalid_application";
    /// A pack parameter has a default (FN-8).
    pub const PACK_DEFAULT: &str = "pack.default";
    /// A pack parameter is marked `const` (FN-8).
    pub const PACK_CONST: &str = "pack.const";
    /// A fixed parameter follows a pack (FN-8).
    pub const PACK_ORDER: &str = "pack.order";
    /// `...{Name}` names no type pack of its declaration (FN-8).
    pub const PACK_UNDECLARED: &str = "pack.undeclared";
    /// A native const implementation has a lifecycle hook (NATIVE-3).
    pub const NATIVE_CONST_HOOK: &str = "native.const_hook";
    /// A native implementation has `start;` or `stop;` and no `when;`
    /// (NATIVE-3).
    pub const NATIVE_HOOK_WITHOUT_WHEN: &str = "native.hook_without_when";
    /// A native implementation has a second `start;` or `stop;`
    /// (NATIVE-3).
    pub const NATIVE_DUPLICATE_HOOK: &str = "native.duplicate_hook";
    /// An `inject` names something that is not a capability (TIME-8).
    pub const INJECT_UNKNOWN_CAPABILITY: &str = "inject.unknown_capability";
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
    resolve::resolve(&mut program, diagnostics);
    forms::check(&program, diagnostics);
    program
}

/// An error this stage reports: every one has a catalogued code and
/// cites the rule that requires it.
fn error(
    category: Category,
    code: &'static str,
    at: Location,
    message: String,
    rule: &'static str,
) -> Diagnostic {
    Diagnostic::error(category, Code::Catalogued(code), at, message).with_rule(rule)
}

/// The names `inject` may name (TIME-8): "The injectables are `out`,
/// `logger`, `clock`, and `scheduler`, and the proposed `alarm` and
/// `global_state` below; `inject` names no other."
pub const INJECTABLES: [&str; 6] = [
    "out",
    "logger",
    "clock",
    "scheduler",
    "alarm",
    "global_state",
];
