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
//! The program's modules, what each declares, what each name in it is
//! bound to, and what each type written in it means.
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
//!   implementation cannot have (FN-8, NATIVE-3);
//! * `read` turns every written type into a [`Type`], records each
//!   declaration's signature or fields, rejects a type made of what its
//!   place does not admit (UNIT-4, TYPE-10, TYPE-11, TYPE-12, TYPE-14,
//!   GEN-2), and settles whether each generic parameter is a type or a
//!   value, checking each generic argument against that (GEN-13,
//!   GEN-14);
//! * `declared` reads the declarations against one another: a struct's
//!   parents and fields, the structs that hold themselves, and an
//!   enum's numbering (TYPE-3, TYPE-4, TYPE-6, TYPE-7).
//!
//! What an expression is, phases and constants are not checked yet.
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
mod constant;
mod declare;
mod declared;
mod forms;
mod imports;
mod program;
mod read;
mod resolve;
mod types;

use hgl_diagnostics::{Category, Code, Diagnostic, Diagnostics, Location};
use hgl_parser::SyntaxNode;
use hgl_source::{FileId, Span};

pub use program::{
    Binding, Decision, Declaration, DeclarationKind, Import, ImportKind, LocalKind, Module,
    ModuleFile, NameUse, OperatorRef, PreludeName, Program, Role, Test,
};
pub use types::{
    Argument, Constant, ConstantValue, Detail, EnumMember, EnumType, Field, GenericKind,
    GenericParameter, Pack, Parameter, Scalar, Signature, StructType, Type, WrittenType,
};

/// The diagnostic codes this stage reports.
///
/// Those directly in this module are codes of the specification's
/// diagnostic catalogue, so an `expect-error` annotation can expect them
/// (TST-12). Those in [`codes::local`] are this compiler's own, for
/// rejections the catalogue has no row for yet. The analyser's tests
/// check each diagnostic's category and code against the catalogue, or
/// against [`codes::local::ALL`].
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
    /// A window's size is neither a tick count nor a duration, or its
    /// two sizes are of different kinds (TYPE-11).
    pub const ROLLING_SIZE_KIND: &str = "rolling.size_kind";
    /// A window's size is out of bounds (TYPE-11).
    pub const ROLLING_SIZE_BOUNDS: &str = "rolling.size_bounds";
    /// An integer literal is outside the `i64` domain: 2^63 with no
    /// minus before it (LEX-7).
    pub const NUMBER_INTEGER_RANGE: &str = "number.integer_range";

    /// Codes local to this compiler: the specification rejects what
    /// they report and its catalogue has no row for it. Each is
    /// proposed to the specification, and is replaced by the catalogued
    /// code when one is agreed.
    pub mod local {
        /// A name in a type position is not a type (UNIT-10, GEN-2,
        /// GEN-13).
        pub const TYPE_NOT_A_TYPE: &str = "type.not_a_type";
        /// A `rolling`, `ref`, `atomic` or `signal` where only an
        /// ordinary value type may stand (TYPE-12 and the grammar's
        /// `value_type`).
        pub const TYPE_NOT_ORDINARY: &str = "type.not_ordinary";
        /// `signal` somewhere other than the whole type of an input
        /// (TYPE-12).
        pub const SIGNAL_POSITION: &str = "signal.position";
        /// `?` applied to `ref<T>` or `signal` (TYPE-14, TYPE-15).
        pub const NULLABLE_NOT_ORDINARY: &str = "nullable.not_ordinary";
        /// A generic type given the wrong number of arguments, or a
        /// type that takes none given some (GEN-2, GEN-13).
        pub const GENERIC_ARGUMENT_COUNT: &str = "generic.argument_count";
        /// A list's size is not a nonnegative count (TYPE-10).
        pub const LIST_SIZE: &str = "list.size";
        /// A generic parameter is used as a type and as a value
        /// (GEN-13).
        pub const GENERIC_KIND_CONFLICT: &str = "generic.kind_conflict";
        /// Nothing in a signature says whether a generic parameter is a
        /// type or a value (GEN-13).
        pub const GENERIC_KIND_UNRESOLVED: &str = "generic.kind_unresolved";
        /// A generic argument is a type where the parameter is a value,
        /// or a value where it is a type (GEN-14).
        pub const GENERIC_ARGUMENT_KIND: &str = "generic.argument_kind";
        /// A temporal parameter has a default (UNIT-8).
        pub const DEFAULT_TEMPORAL_PARAMETER: &str = "default.temporal_parameter";
        /// A struct's parent is not an abstract struct (TYPE-3).
        pub const STRUCT_PARENT: &str = "struct.parent";
        /// Structs inherit from one another (TYPE-6).
        pub const STRUCT_INHERITANCE_CYCLE: &str = "struct.inheritance_cycle";
        /// A field's type or optionality differs between two parents, or
        /// between a struct and a parent (TYPE-4).
        pub const FIELD_CONFLICT: &str = "field.conflict";
        /// `name = value` names no field a parent declares (TYPE-4).
        pub const FIELD_NOT_INHERITED: &str = "field.not_inherited";
        /// Two members of a struct have one name (TYPE-4).
        pub const FIELD_DUPLICATE: &str = "field.duplicate";
        /// A field makes its struct hold itself and is not an optional
        /// `atomic<...>` defaulted to null, or a descendant removes that
        /// null (TYPE-6).
        pub const FIELD_RECURSIVE: &str = "field.recursive";
        /// An enum member's number is not an integer (TYPE-7).
        pub const ENUM_VALUE: &str = "enum.value";
        /// Two members of an enum have one number (TYPE-7).
        pub const ENUM_DUPLICATE_VALUE: &str = "enum.duplicate_value";
        /// An enum's numbering passes the largest `i64` (TYPE-7).
        pub const ENUM_OVERFLOW: &str = "enum.overflow";
        /// Two members of an enum have one name (TYPE-7).
        pub const ENUM_DUPLICATE_MEMBER: &str = "enum.duplicate_member";

        /// Every local code of this stage.
        pub const ALL: [&str; 20] = [
            TYPE_NOT_A_TYPE,
            TYPE_NOT_ORDINARY,
            SIGNAL_POSITION,
            NULLABLE_NOT_ORDINARY,
            GENERIC_ARGUMENT_COUNT,
            LIST_SIZE,
            GENERIC_KIND_CONFLICT,
            GENERIC_KIND_UNRESOLVED,
            GENERIC_ARGUMENT_KIND,
            DEFAULT_TEMPORAL_PARAMETER,
            STRUCT_PARENT,
            STRUCT_INHERITANCE_CYCLE,
            FIELD_CONFLICT,
            FIELD_NOT_INHERITED,
            FIELD_DUPLICATE,
            FIELD_RECURSIVE,
            ENUM_VALUE,
            ENUM_DUPLICATE_VALUE,
            ENUM_OVERFLOW,
            ENUM_DUPLICATE_MEMBER,
        ];
    }
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
    read::read(&mut program, diagnostics);
    declared::check(&mut program, diagnostics);
    program
}

/// An error with a code of the specification's catalogue. It cites the
/// rule that requires it.
fn error(
    category: Category,
    code: &'static str,
    at: Location,
    message: String,
    rule: &'static str,
) -> Diagnostic {
    Diagnostic::error(category, Code::Catalogued(code), at, message).with_rule(rule)
}

/// An error with one of this compiler's own codes ([`codes::local`]):
/// the rule cited rejects the source, and the catalogue has no row for
/// it.
fn local_error(
    category: Category,
    code: &'static str,
    at: Location,
    message: String,
    rule: &'static str,
) -> Diagnostic {
    Diagnostic::error(category, Code::Local(code), at, message).with_rule(rule)
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
