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
//! * `phases` says of each temporal function whether its body describes
//!   composition, a graph, or runtime, a node (FN-2), and checks that a
//!   node's body is its declarations and then its hooks (FN-2, TIME-3,
//!   TIME-5, TIME-7);
//! * `read` turns every written type into a [`Type`], records each
//!   declaration's signature or fields, rejects a type made of what its
//!   place does not admit (UNIT-4, TYPE-10, TYPE-11, TYPE-12, TYPE-14,
//!   GEN-2), and settles whether each generic parameter is a type or a
//!   value, checking each generic argument against that (GEN-13,
//!   GEN-14);
//! * `declared` reads the declarations against one another: a struct's
//!   parents and fields, the structs that hold themselves, and an
//!   enum's numbering (TYPE-3, TYPE-4, TYPE-6, TYPE-7);
//! * `signatures` checks each callable's signature with its types read:
//!   a generic value parameter's default is of its type, and an option
//!   is one kind across an operator's candidates (GEN-13, GEN-6).
//!
//! What an expression is, phases and constants are not checked yet.
//!
//! # The prelude
//!
//! Every module sees the public declarations of core's mandatory module,
//! `core`, without importing them (UNIT-11). When the program has that
//! module, a name nothing nearer binds is bound to its public
//! declaration of the name. Core declares only part of its module so
//! far, so a name it does not have, and any name in a program given
//! without it, is not reported: it is listed in
//! [`Module::prelude_names`], as a name the prelude is expected to bind.
//!
//! # Like every stage
//!
//! [`analyse`] reads the parser's output and the diagnostics sink and
//! nothing else, reports every problem it finds, and gives a program for
//! any input: a declaration the parser could not read whole is left out
//! and the rest is analysed.

mod assemble;
mod constant;
mod cycles;
mod declare;
mod declared;
mod forms;
pub mod hir;
mod imports;
mod instance;
mod phases;
mod program;
mod read;
mod resolve;
mod signatures;
mod types;
mod values;

use hgl_diagnostics::{Category, Code, Diagnostic, Diagnostics, Location};
use hgl_parser::SyntaxNode;
use hgl_source::{FileId, Span};

pub use program::{
    Binding, Decision, Declaration, DeclarationKind, Import, ImportKind, LocalKind, Module,
    ModuleFile, NameUse, OperatorRef, Phase, PreludeName, Program, Role, Test,
};
pub use types::{
    Argument, CallArguments, Constant, ConstantValue, Detail, EnumMember, EnumType, Field,
    FieldRef, GenericKind, GenericParameter, Member, NamedKind, Pack, Parameter, Scalar, Signature,
    StructType, TraceStyle, Type, WrittenType,
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
    /// A candidate, `impl fn` or `impl const fn`, native or not, names
    /// no operator (GEN-4).
    pub const IMPL_UNKNOWN_OPERATOR: &str = "impl.unknown_operator";
    /// A candidate declares as a parameter an option that another
    /// candidate of its operator declares as a generic value parameter
    /// (GEN-6).
    pub const IMPL_OPTION_KIND: &str = "impl.option_kind";
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
    /// `inject out` in a function that declares no result (TIME-7).
    pub const INJECT_OUT_WITHOUT_RESULT: &str = "inject.out_without_result";
    /// A call of a native type's name, which no call constructs
    /// (NATIVE-6).
    pub const CALL_NOT_CALLABLE: &str = "call.not_callable";
    /// A capability retained, or a capability type written where one
    /// would be (TIME-8).
    pub const CAPABILITY_AS_VALUE: &str = "capability.as_value";
    /// `alarm` and `scheduler` injected in one node (TIME-8).
    pub const INJECT_ALARM_AND_SCHEDULER: &str = "inject.alarm_and_scheduler";
    /// `endpoint<T>` other than as the whole type of a native value
    /// function's parameter (TYPE-12).
    pub const ENDPOINT_POSITION: &str = "endpoint.position";
    /// A node's construct inside a block of a temporal function
    /// (TIME-3, TIME-5).
    pub const NODE_NESTED: &str = "node.nested";
    /// A node's construct in a `const fn` (FN-1, FN-2).
    pub const NODE_IN_VALUE_FUNCTION: &str = "node.in_value_function";
    /// A `state`, `cache` or `inject` after a hook (TIME-5).
    pub const NODE_DECLARATION_ORDER: &str = "node.declaration_order";
    /// A second `start` or a second `stop` (TIME-5).
    pub const NODE_DUPLICATE_HOOK: &str = "node.duplicate_hook";
    /// A statement of a node's body that is no declaration and no hook
    /// (FN-2).
    pub const NODE_STATEMENT: &str = "node.statement";
    /// A node's construct or a `return` in a test (TST-3).
    pub const TEST_STATEMENT_PHASE: &str = "test.statement_phase";
    /// `assert raises(...)` given what is no literal catalogued
    /// execution code (TST-10).
    pub const TEST_RAISES_CODE: &str = "test.raises_code";
    /// Eval's first argument names no function of a module (TST-5).
    pub const EVAL_TARGET: &str = "eval.target";
    /// The value of an eval of an outputless target is bound or
    /// compared (TST-8).
    pub const EVAL_NO_RESULT: &str = "eval.no_result";
    /// `_` in a timed trace, or outside a trace of a test (EXPR-8,
    /// TST-7).
    pub const TRACE_PLACEHOLDER: &str = "trace.placeholder";
    /// Dense and timed traces in one eval, its expectation included
    /// (TST-7).
    pub const TRACE_STYLE: &str = "trace.style";
    /// Constant keys of a timed trace that do not strictly increase
    /// (TST-7).
    pub const TRACE_KEY_ORDER: &str = "trace.key_order";
    /// A trace stored in a field, or given as a temporal argument
    /// outside an eval (TST-3).
    pub const TRACE_ESCAPE: &str = "trace.escape";
    /// A statement a source's shape has no place for (SOURCE-1).
    pub const SOURCE_STATEMENT: &str = "source.statement";
    /// `when` in a source (SOURCE-1).
    pub const SOURCE_WHEN: &str = "source.when";
    /// A temporal parameter of a source (SOURCE-1).
    pub const SOURCE_INPUT: &str = "source.input";
    /// `out` used in `start` or written in `stop`, or a `return` with a
    /// value in either (TIME-7).
    pub const OUT_LIFECYCLE: &str = "out.lifecycle";
    /// A temporal input read, or given to an endpoint operation, in
    /// `start` (TIME-5).
    pub const INPUT_LIFECYCLE: &str = "input.lifecycle";
    /// A key of the global store that is the run's own (TIME-8).
    pub const STORE_RESERVED_KEY: &str = "store.reserved_key";
    /// A `use` line names an application (UNIT-14).
    pub const APPLICATION_IMPORT: &str = "application.import";
    /// A declaration of an application is marked `export` (UNIT-14).
    pub const APPLICATION_EXPORT: &str = "application.export";
    /// An application has no run statement, or a second (UNIT-14).
    pub const APPLICATION_RUN_COUNT: &str = "application.run_count";
    /// A run statement stands in a module, or before a declaration of
    /// its application (UNIT-14); `run` is written in a test that is no
    /// integration test (RUN-3).
    pub const RUN_POSITION: &str = "run.position";
    /// An application's path is a module's or another application's
    /// (UNIT-14).
    pub const APPLICATION_DUPLICATE_PATH: &str = "application.duplicate_path";
    /// A run statement's first argument is no call of an entry (RUN-1).
    pub const RUN_ENTRY: &str = "run.entry";
    /// A constant's value depends on itself (UNIT-13).
    pub const CONSTANT_CYCLE: &str = "constant.cycle";
    /// An endpoint operation given what is no temporal input of its
    /// function and not `out` (TIME-9).
    pub const ENDPOINT_ARGUMENT: &str = "endpoint.argument";
    /// A payload read under manual admission that no guard dominates
    /// (TIME-4).
    pub const PAYLOAD_UNGUARDED: &str = "payload.unguarded";
    /// The dormant-node warning: `modified([])` in a node that injects
    /// neither `scheduler` nor `alarm`, or whose `start` does not use
    /// the one it injects (TIME-3). It does not reject the source.
    pub const NODE_NO_ACTIVATION: &str = "node.no_activation";
    /// A temporal connection where composition requires a constant
    /// (FN-3).
    pub const CONNECTION_AS_CONSTANT: &str = "connection.as_constant";
    /// A connection assigned to a `var` that holds an ordinary value,
    /// or the reverse (EXPR-4, CTRL-14).
    pub const BINDING_CATEGORY: &str = "binding.category";
    /// An assignment to a `var` declared outside a composition loop
    /// that makes one child for each member, or a `return` in one
    /// (CTRL-8, CTRL-15).
    pub const LOOP_CARRIED: &str = "loop.carried";
    /// A state initializer reads a temporal input (TIME-5).
    pub const STATE_INPUT_READ: &str = "state.input_read";
    /// A window's size is neither a tick count nor a duration, or its
    /// two sizes are of different kinds (TYPE-11).
    pub const ROLLING_SIZE_KIND: &str = "rolling.size_kind";
    /// A window's size is out of bounds (TYPE-11).
    pub const ROLLING_SIZE_BOUNDS: &str = "rolling.size_bounds";
    /// An integer literal is outside the `i64` domain: 2^63 with no
    /// minus before it (LEX-7).
    pub const NUMBER_INTEGER_RANGE: &str = "number.integer_range";

    /// A name in a type position is not a type (UNIT-10, GEN-2,
    /// GEN-13).
    pub const TYPE_NOT_A_TYPE: &str = "type.not_a_type";
    /// A `rolling`, `ref`, `atomic` or `signal` where only an
    /// ordinary value type may stand, under a `?` included (TYPE-12,
    /// TYPE-14 and the grammar's `value_type`).
    pub const TYPE_NOT_ORDINARY: &str = "type.not_ordinary";
    /// `delta<T>` of a T that has no delta (TYPE-16).
    pub const DELTA_UNSUPPORTED_SHAPE: &str = "delta.unsupported_shape";
    /// A delta's constructor given a name that is no property of the
    /// delta (TYPE-16).
    pub const DELTA_ARGUMENT_NAME: &str = "delta.argument_name";
    /// A delta's constructor given one property twice (TYPE-16).
    pub const DELTA_DUPLICATE_ARGUMENT: &str = "delta.duplicate_argument";
    /// `signal` somewhere other than the whole type of an input
    /// (TYPE-12).
    pub const SIGNAL_POSITION: &str = "signal.position";
    /// A generic type given the wrong number of arguments, or a
    /// type that takes none given some (GEN-2, GEN-13).
    pub const GENERIC_ARGUMENT_COUNT: &str = "generic.argument_count";
    /// A list's size is not a nonnegative count (TYPE-10).
    pub const LIST_SIZE: &str = "list.size";
    /// A set's element or a dictionary's key is nullable (TYPE-14).
    pub const NULLABLE_KEY: &str = "nullable.key";
    /// A cardinality's upper bound is below its lower (GEN-10).
    pub const PACK_CARDINALITY: &str = "pack.cardinality";
    /// A struct's parent is not an abstract struct (TYPE-3).
    pub const STRUCT_PARENT: &str = "struct.parent";
    /// A struct lists one parent twice (TYPE-3).
    pub const STRUCT_DUPLICATE_PARENT: &str = "struct.duplicate_parent";
    /// Structs inherit from one another (TYPE-6).
    pub const STRUCT_INHERITANCE_CYCLE: &str = "struct.inheritance_cycle";
    /// A field's type, and so its optionality, differs between two
    /// parents, or between a struct and a parent (TYPE-4).
    pub const FIELD_CONFLICT: &str = "field.conflict";
    /// `name = value` names no field a parent declares (TYPE-4).
    pub const FIELD_NOT_INHERITED: &str = "field.not_inherited";
    /// Two members of a struct have one name (TYPE-4).
    pub const FIELD_DUPLICATE: &str = "field.duplicate";
    /// A field makes its struct hold itself and is not optional, a
    /// descendant removes its null default, or the field gives a struct
    /// of its cycle a type built from a generic parameter (TYPE-6).
    pub const FIELD_RECURSIVE: &str = "field.recursive";
    /// A field that is not optional has a default of null (TYPE-4).
    pub const FIELD_NULL_DEFAULT: &str = "field.null_default";
    /// An enum member's number is not an integer (TYPE-7).
    pub const ENUM_VALUE: &str = "enum.value";
    /// Two members of an enum have one number (TYPE-7).
    pub const ENUM_DUPLICATE_VALUE: &str = "enum.duplicate_value";
    /// An enum's numbering passes the largest `i64` (TYPE-7).
    pub const ENUM_OVERFLOW: &str = "enum.overflow";
    /// Two members of an enum have one name (TYPE-7).
    pub const ENUM_DUPLICATE_MEMBER: &str = "enum.duplicate_member";

    /// A generic parameter is used as a type and as a value (GEN-13).
    pub const GENERIC_KIND_CONFLICT: &str = "generic.kind_conflict";
    /// Nothing in a struct's fields or a constraint says whether a
    /// generic parameter is a type or a value (GEN-13).
    pub const GENERIC_KIND_UNRESOLVED: &str = "generic.kind_unresolved";
    /// A generic argument is a type where the parameter is a value, or
    /// a value where it is a type (GEN-14).
    pub const GENERIC_ARGUMENT_KIND: &str = "generic.argument_kind";
    /// A call leaves a generic parameter of its callee unresolved
    /// (GEN-2, GEN-8).
    pub const GENERIC_UNRESOLVED: &str = "generic.unresolved";
    /// A call gives a generic value parameter of its callee no
    /// compile-time constant (GEN-8).
    pub const GENERIC_ARGUMENT_CONSTANT: &str = "generic.argument_constant";
    /// A function is called, or a struct applied, with generic
    /// arguments its `requires` does not hold for (GEN-3, GEN-16).
    pub const REQUIRES_UNMET: &str = "requires.unmet";

    /// A value's type is not the type its place requires (EXPR-5),
    /// or `null` stands where the type is not nullable (TYPE-14).
    pub const TYPE_MISMATCH: &str = "type.mismatch";
    /// `ok` or `err` where no complete `result<T, E>` is expected
    /// (ERR-2).
    pub const RESULT_CONTEXT_TYPE: &str = "result.context_type";
    /// `.value` or `.error` of a result no guard has proved a success,
    /// or a failure (ERR-3).
    pub const RESULT_REQUIRED_VARIANT: &str = "result.required_variant";
    /// `?` on what is no result, or whose failure the enclosing
    /// function does not return (ERR-5).
    pub const RESULT_PROPAGATION_TYPE: &str = "result.propagation_type";
    /// `?` where nothing propagates: composition and lifecycle (ERR-5).
    pub const RESULT_PROPAGATION_PHASE: &str = "result.propagation_phase";
    /// `capture` outside composition, or of what is no temporal call
    /// (ERR-7).
    pub const CAPTURE_PHASE: &str = "capture.phase";
    /// `.value` of a capture whose call has no output (ERR-7).
    pub const CAPTURE_OUTPUT_FIELD: &str = "capture.output_field";
    /// A value that may be null where its payload is required (ERR-12).
    pub const NULLABLE_REQUIRED_PRESENCE: &str = "nullable.required_presence";
    /// A `var` is read on a path that has given it no value
    /// (EXPR-3).
    pub const BINDING_UNASSIGNED: &str = "binding.unassigned";
    /// A binding that is no `var` is assigned (EXPR-3).
    pub const BINDING_IMMUTABLE: &str = "binding.immutable";
    /// An argument with no name follows a named one (EXPR-2).
    pub const ARGUMENT_ORDER: &str = "argument.order";
    /// A call is given more positional arguments than it has
    /// parameters (EXPR-2), or a constructor is given one (TYPE-5).
    pub const ARGUMENT_UNEXPECTED: &str = "argument.unexpected";
    /// A named argument names no parameter or field (EXPR-2).
    pub const ARGUMENT_UNKNOWN: &str = "argument.unknown";
    /// A parameter or field is given twice (EXPR-2).
    pub const ARGUMENT_DUPLICATE: &str = "argument.duplicate";
    /// A parameter or field with no default is given nothing
    /// (EXPR-2, TYPE-5).
    pub const ARGUMENT_MISSING: &str = "argument.missing";
    /// In a hook or a value function, a `const` parameter given what is
    /// no constant expression (FN-1).
    pub const ARGUMENT_CONSTANT: &str = "argument.constant";
    /// A `var` parameter is given what is no place (FN-15).
    pub const ARGUMENT_PLACE: &str = "argument.place";
    /// A `var` parameter of a temporal function, an operator or a
    /// candidate, or one of a capability's type or `endpoint<T>`
    /// (FN-15).
    pub const PARAMETER_VAR: &str = "parameter.var";
    /// A value function calls a temporal function (FN-4).
    pub const CALL_ROLE: &str = "call.role";
    /// An abstract struct is constructed (TYPE-3).
    pub const STRUCT_ABSTRACT: &str = "struct.abstract";
    /// A struct has no field, or an enum no member, of the name
    /// (EXPR-7, TYPE-7).
    pub const MEMBER_UNKNOWN: &str = "member.unknown";
    /// `.member` stands where no enum type is expected (TYPE-7).
    pub const ENUM_CONTEXT: &str = "enum.context_type";
    /// `null` stands where nothing gives it a type (TYPE-14).
    pub const NULL_CONTEXT: &str = "null.context_type";
    /// A sequence's entries give no type, and nothing is expected
    /// of it (EXPR-9).
    pub const SEQUENCE_CONTEXT: &str = "sequence.context_type";
    /// A call of an operator, by symbol or by name, that no candidate
    /// admits (GEN-5: "No match rejects"; OPS-1, OPS-2 for a symbol's
    /// operands).
    pub const OPERATOR_NO_CANDIDATE: &str = "operator.no_candidate";
    /// A call of an operator that two candidates admit equally (GEN-5:
    /// "tied most-specific candidates are ambiguous").
    pub const OPERATOR_AMBIGUOUS: &str = "operator.ambiguous";

    /// A parameter that may not be omitted follows one that may
    /// (UNIT-8).
    pub const DEFAULT_ORDER: &str = "default.order";
    /// A tuple's index is no constant, or is outside the tuple
    /// (EXPR-7).
    pub const TUPLE_INDEX: &str = "tuple.index";
    /// A value function that declares a result can reach its end with
    /// no value (FN-7).
    pub const RETURN_MISSING: &str = "return.missing";
    /// A `switch` label has the value of an earlier label (CTRL-6).
    pub const SWITCH_DUPLICATE_LABEL: &str = "switch.duplicate_label";
    /// A `while` with no condition in a value function (CTRL-12).
    pub const WHILE_CONDITION: &str = "while.condition";

    /// Codes local to this compiler: the specification rejects what
    /// they report and its catalogue has no row for it. Each is
    /// proposed to the specification, and is replaced by the catalogued
    /// code when one is agreed. There is none at present.
    pub mod local {
        /// Every local code of this stage.
        pub const ALL: [&str; 0] = [];
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

/// A stack size, in bytes, on which analysing source parsed within
/// `limits` does not overflow.
///
/// The analyser reads the tree by calling itself for what is nested: a
/// block inside an `if` inside a block, a member of a member, the `if`
/// of an `else if`. A chain of binary operators is read in a loop and
/// costs none. Measured in an unoptimised build, a level of nesting
/// costs about 12 KiB and a level of tree depth about 6 KiB. This
/// allows near three times that, on top of 1 MiB.
pub fn stack_size(limits: hgl_parser::Limits) -> usize {
    const KIB: usize = 1024;
    // `saturating_*` stops at the largest number instead of wrapping
    // round to a small one when the limits are absurdly large.
    (1024 * KIB)
        .saturating_add(limits.nesting.saturating_mul(32 * KIB))
        .saturating_add(limits.tree_depth.saturating_mul(16 * KIB))
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
    phases::classify(&mut program);
    read::read(&mut program, diagnostics);
    phases::check(&program, diagnostics);
    declared::check(&mut program, diagnostics);
    signatures::check(&program, diagnostics);
    values::check(&mut program, diagnostics);
    // UNIT-13: asked of the typed program, where each call has the
    // definition it selected.
    cycles::check(&program, diagnostics);
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

/// A warning of the specification's catalogue, which does not reject
/// the source (ERR-1). It cites the rule that says what is warned of.
fn warning(
    category: Category,
    code: &'static str,
    at: Location,
    message: String,
    rule: &'static str,
) -> Diagnostic {
    Diagnostic::warning(category, Code::Catalogued(code), at, message).with_rule(rule)
}

/// Core's module as the pinned `hgl_core` declares it, built into the
/// compiler: each file's name for diagnostics, and its text.
///
/// UNIT-11: every module binds core's public declarations with no
/// `use`. Core is the part of the language every program has, so a
/// program is always analysed with core's module: its own where it
/// holds a `module core`, and this one where it does not. Whoever calls
/// [`analyse`] parses these files with the program's and passes them
/// along; the analyser reads only what it is given.
///
/// `include_str!` puts a file's text in the compiler when the compiler
/// is built, so the module is the one at the `spec/hgl_core` pin. A
/// test fails when the pin adds a file this list does not have.
pub const CORE_FILES: [(&str, &str); 4] = [
    (
        "<core>/core/core.hgl",
        include_str!("../../../spec/hgl_core/hgl/core/core.hgl"),
    ),
    (
        "<core>/native/capabilities.hgl",
        include_str!("../../../spec/hgl_core/hgl/native/capabilities.hgl"),
    ),
    (
        "<core>/native/endpoints.hgl",
        include_str!("../../../spec/hgl_core/hgl/native/endpoints.hgl"),
    ),
    (
        "<core>/native/scalar_operators.hgl",
        include_str!("../../../spec/hgl_core/hgl/native/scalar_operators.hgl"),
    ),
];

/// The contextual names of the result forms (ERR-13): "`ok`, `err`,
/// `is_ok`, `is_err`, and `capture` are contextual names (LEX-10): as
/// the callee of a call each is the form this chapter defines, whatever
/// else is in scope. No declaration, import alias, parameter, or local
/// takes one of them".
pub const RESULT_FORMS: [&str; 5] = ["ok", "err", "is_ok", "is_err", "capture"];

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
