//! Reads the types written in a program.
//!
//! Every type the source writes is turned into a [`Type`]: the types of
//! parameters and results, of fields, of locals, and the type a
//! constructor names. What a type is made of is checked as it is read:
//!
//! * a name in a type position names a struct, an enum, a native type or
//!   a generic parameter that is a type;
//! * a generic type is given as many arguments as it declares (GEN-2);
//! * a set's element, a dictionary's key, a window's value, an atomic or
//!   result payload, and the type of constant configuration are ordinary
//!   value types (TYPE-12 and the grammar's `value_type`);
//! * `signal` stands only as the whole type of an input (TYPE-12), and
//!   `?` is not applied to `signal` (TYPE-14);
//! * a parameter and a result marked `ref` are recorded as marked: the
//!   mark is a mode, and no part of the type (TYPE-12, FN-16);
//! * a list's size is a count (TYPE-10), and a window's sizes are of one
//!   kind and in bounds (TYPE-11).
//!
//! Whether a value *has* the type its position asks for is not checked
//! here: no expression has a type yet.
//!
//! A type whose name nothing in the program binds is left for the prelude
//! (UNIT-11) and read as [`Type::Unknown`]; so is one already reported.
//! Nothing further is reported about an unknown type (ERR-9).

use std::collections::HashMap;

use hgl_diagnostics::{Diagnostics, Location};
use hgl_parser::{SyntaxKind, SyntaxNode, SyntaxToken};
use hgl_source::{FileId, Span};

use crate::constant::Constants;
use crate::program::{Binding, DependentPack, Module, OperatorRef, Program};
use crate::syntax::{test_statements, tokens};
use crate::types::{CallArguments, DeltaInput, Detail, GenericParameter, Type, WrittenType};

use self::declarations::{Evidence, Kinds, declaration_kinds};

mod applications;
mod declarations;
mod forms;
mod packs;
mod scan;

/// Reads the types of every module, filling in [`Module::details`] and
/// [`Module::written`].
pub(crate) fn read(program: &mut Program, diagnostics: &mut Diagnostics) {
    // TYPE-7: an enum's members, each with its number, are read first:
    // a constant written in a type may name one, `Tagged<Mode(1)>`.
    crate::declared::number_enums(program, diagnostics);
    // A generic argument is read for the parameter it is given to, so
    // declared struct and callable kinds are settled first (GEN-13/14).
    let kinds = declaration_kinds(program);
    for index in 0..program.modules.len() {
        let module = &program.modules[index];
        // The reader reads the whole program and so cannot change it.
        // What it finds is taken out when it is done, and written back.
        // A module is made from at least one file.
        let Some(mut reader) = Reader::new(program, module, &kinds, diagnostics) else {
            continue;
        };
        let mut details = Vec::with_capacity(module.declarations.len());
        for (position, declaration) in module.declarations.iter().enumerate() {
            let detail = reader.declaration(declaration);
            // An enum's members have been read already.
            details.push(match module.details.get(position) {
                Some(numbered @ Detail::Enum(_)) => numbered.clone(),
                _ => detail,
            });
        }
        for test in &module.tests {
            reader.enter(module.files[test.file].file);
            for statement in test_statements(&test.syntax) {
                reader.scan(&statement);
            }
        }
        if let Some(run) = &module.run {
            reader.enter(module.files[run.file].file);
            reader.scan(&run.syntax);
        }
        for statement in &module.store {
            reader.enter(module.files[statement.file].file);
            reader.scan(&statement.syntax);
        }
        for (file, node) in &module.instantiations {
            reader.enter(module.files[*file].file);
            reader.scan(node);
        }
        let calls = std::mem::take(&mut reader.calls);
        let dependent_packs = std::mem::take(&mut reader.dependent_packs);
        let delta_inputs = std::mem::take(&mut reader.delta_inputs);
        let mut written = reader.written;

        // File order, then source order.
        let position: HashMap<FileId, usize> = module
            .files
            .iter()
            .enumerate()
            .map(|(position, file)| (file.file, position))
            .collect();
        written.sort_by_key(|written| (position[&written.at.file], written.at.span.start));
        let module = &mut program.modules[index];
        module.details = details;
        module.dependent_packs = dependent_packs;
        module.written = written;
        module.delta_inputs = delta_inputs;
        module.call_arguments = calls;
    }
}

/// A declaration, by the position of its module in `Program::modules`
/// and its own in that module's `declarations`.
type Id = (usize, usize);

/// What the place a type is written in admits.
///
/// `#[derive(Clone, Copy)]` makes a `Position` a plain value, passed
/// about by copying, like an integer.
#[derive(Debug, Clone, Copy)]
struct Position {
    /// Set where only an ordinary value type may stand: what the place
    /// is, for the message, and the rule that says so.
    value: Option<(&'static str, &'static str)>,
    /// True where `signal` may stand.
    signal: bool,
    /// True where `?` may follow a type that is no ordinary value type:
    /// the whole type of an input, whose argument may then be omitted,
    /// or of a struct field, which is then optional (TYPE-14).
    omissible: bool,
    /// True where the name of a type pack may stand.
    pack: bool,
    /// True where the name of a generic value parameter may stand.
    constant: bool,
    /// True where a generic type may be named without its arguments.
    bare: bool,
}

impl Position {
    /// A result, a field, a parent, or a part of another type.
    const PLAIN: Position = Position {
        value: None,
        signal: false,
        omissible: false,
        pack: false,
        constant: false,
        bare: false,
    };

    /// The whole type of an input: `signal` may stand (TYPE-12), and so
    /// may `?` after a type that is no ordinary value type (TYPE-14).
    const INPUT: Position = Position {
        signal: true,
        omissible: true,
        ..Position::PLAIN
    };

    /// The whole type of a struct field: `?` may follow a type that is
    /// no ordinary value type, and the field is then optional (TYPE-14).
    const FIELD: Position = Position {
        omissible: true,
        ..Position::PLAIN
    };

    /// The type after `...` or inside `...{ }`: a type pack may be named
    /// (FN-8).
    const PACK: Position = Position {
        pack: true,
        ..Position::PLAIN
    };

    /// A generic argument: a type pack or a value parameter may be
    /// passed on by name.
    const ARGUMENT: Position = Position {
        pack: true,
        constant: true,
        ..Position::PLAIN
    };

    /// A place whose own rules are checked by a later pass: an operand
    /// of a constraint, an argument of an `instantiate`. Only what the
    /// type is made of is checked here.
    const OPEN: Position = Position {
        value: None,
        signal: true,
        omissible: true,
        pack: true,
        constant: true,
        bare: true,
    };

    /// A place for an ordinary value type only.
    fn value(what: &'static str, rule: &'static str) -> Position {
        Position {
            value: Some((what, rule)),
            ..Position::PLAIN
        }
    }

    /// The position of a part of a type written here: an element of a
    /// tuple or list, a dictionary's value. An ordinary value type is
    /// made of ordinary value types.
    fn part(self) -> Position {
        Position {
            value: self.value,
            ..Position::PLAIN
        }
    }
}

struct Reader<'a> {
    program: &'a Program,
    /// The module being read.
    module: &'a Module,
    /// The file being read.
    file: FileId,
    /// What each name of the module resolved to, by the file and offset
    /// of its use.
    bindings: HashMap<(FileId, u32), &'a Binding>,
    /// Where the `name<...>(...)` nodes the resolver decided are
    /// constructors start (GEN-14).
    constructors: Vec<(FileId, u32)>,
    /// The kind of each generic parameter of every struct, as far as
    /// it is settled.
    kinds: &'a Kinds,
    /// The generic parameters of the declaration being read.
    generics: Vec<GenericParameter>,
    /// What the signature has said so far of each of `generics`.
    evidence: Vec<Evidence>,
    /// True while the signature of the declaration is read: its generic
    /// parameters, parameters, result, parents and fields. Only there
    /// does a use say what a generic parameter is (GEN-13).
    in_signature: bool,
    /// True while a struct is read.
    reading_struct: bool,
    /// True while a value function is read: a `const fn` or a native
    /// one (FN-1).
    value_function: bool,
    /// The operator a value candidate being read names (GEN-4). The
    /// candidate writes that operator's const options with no marker
    /// (GEN-6).
    option_owner: Option<OperatorRef>,
    /// True while a generic argument is read that is given to a
    /// parameter of a known struct whose kind is not settled.
    unsettled_target: bool,
    /// The types of the declaration being read, by the offset each
    /// starts at, so that none is read twice.
    read: HashMap<u32, Type>,
    /// Every type read in the module so far.
    written: Vec<WrittenType>,
    /// Original operands of explicit delta applications (TYPE-16/18).
    delta_inputs: Vec<DeltaInput>,
    /// Which function and exclusions each dependent pack names (FN-8).
    dependent_packs: HashMap<Location, DependentPack>,
    /// The generic arguments of every call read in the module so far
    /// (GEN-8).
    calls: Vec<CallArguments>,
    /// What each name of the program resolved to, by the file and
    /// offset of its use: a constant a type names may be declared in
    /// another module, and is read there (UNIT-13).
    everywhere: HashMap<(FileId, u32), &'a Binding>,
    /// The values of the constants the module's types have named
    /// (UNIT-13).
    constants: Constants,
    diagnostics: &'a mut Diagnostics,
}

impl<'a> Reader<'a> {
    /// A reader for `module`; `None` for a module with no file, which
    /// has nothing to read.
    fn new(
        program: &'a Program,
        module: &'a Module,
        kinds: &'a Kinds,
        diagnostics: &'a mut Diagnostics,
    ) -> Option<Reader<'a>> {
        let first = module.files.first()?;
        Some(Reader {
            program,
            module,
            file: first.file,
            // What each name resolved to, by where it is used (UNIT-10).
            bindings: module
                .uses
                .iter()
                .map(|used| ((used.at.file, used.at.span.start), &used.binding))
                .collect(),
            constructors: module
                .decisions
                .iter()
                .filter(|decision| decision.constructor == Some(true))
                .map(|decision| (decision.at.file, decision.at.span.start))
                .collect(),
            kinds,
            generics: Vec::new(),
            evidence: Vec::new(),
            in_signature: false,
            reading_struct: false,
            value_function: false,
            option_owner: None,
            unsettled_target: false,
            read: HashMap::new(),
            written: Vec::new(),
            delta_inputs: Vec::new(),
            dependent_packs: HashMap::new(),
            calls: Vec::new(),
            everywhere: program
                .modules
                .iter()
                .flat_map(|module| &module.uses)
                .map(|used| ((used.at.file, used.at.span.start), &used.binding))
                .collect(),
            constants: Constants::default(),
            diagnostics,
        })
    }
}

// ---- Reading the tree ----

/// True for a node that is a type.
pub(crate) fn is_type(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::ScalarType
            | SyntaxKind::NamedType
            | SyntaxKind::PackMemberType
            | SyntaxKind::TupleType
            | SyntaxKind::ListType
            | SyntaxKind::SetType
            | SyntaxKind::DictType
            | SyntaxKind::RollingWindowType
            | SyntaxKind::EndpointType
            | SyntaxKind::SignalType
            | SyntaxKind::AtomicType
            | SyntaxKind::ResultType
            | SyntaxKind::ErrorType
            | SyntaxKind::DeltaType
            | SyntaxKind::NullableType
    )
}

/// The first child node of `node` that is a type.
pub(crate) fn type_child(node: &SyntaxNode) -> Option<SyntaxNode> {
    node.children().find(|child| is_type(child.kind()))
}

/// The identifiers directly inside `node` before its `:`.
fn names_before_colon(node: &SyntaxNode) -> Vec<SyntaxToken> {
    tokens(node)
        .take_while(|token| token.kind() != SyntaxKind::Colon)
        .filter(|token| token.kind() == SyntaxKind::Ident)
        .collect()
}

/// The default of a parameter or struct member: its first child node
/// after the `=`.
pub(crate) fn default_of(node: &SyntaxNode) -> Option<SyntaxNode> {
    node.children_with_tokens()
        .skip_while(|element| element.kind() != SyntaxKind::Eq)
        .find_map(|element| element.into_node())
}

/// True when an expression is the empty collection `[]`, in parentheses
/// or not (EXPR-9: "Empty brackets have no entries").
fn is_empty_sequence(expression: &SyntaxNode) -> bool {
    let mut node = expression.clone();
    // The parser bounds how deep parentheses nest; so does this.
    for _ in 0..128 {
        if node.kind() != SyntaxKind::ParenExpr {
            break;
        }
        match node.children().next() {
            Some(inner) => node = inner,
            None => return false,
        }
    }
    node.kind() == SyntaxKind::SequenceExpr && node.children().next().is_none()
}

/// True when an expression is the literal `null`, in parentheses or
/// not: "Parentheses select grouping" (EXPR-1), so `(null)` is `null`.
fn is_null(expression: &SyntaxNode) -> bool {
    let mut node = expression.clone();
    // The parser bounds how deep parentheses nest; so does this.
    for _ in 0..128 {
        if node.kind() != SyntaxKind::ParenExpr {
            break;
        }
        match node.children().next() {
            Some(inner) => node = inner,
            None => return false,
        }
    }
    node.kind() == SyntaxKind::Literal
        && tokens(&node)
            .next()
            .is_some_and(|token| token.kind() == SyntaxKind::KwNull)
}

/// The text of `node` with one space wherever whitespace or a comment
/// stands between two of its tokens. Each token is kept as written, so
/// the spaces inside a string literal are the literal's own.
fn text_of(node: &SyntaxNode) -> String {
    let mut text = String::new();
    // True when whitespace or a comment has been passed since the last
    // token written.
    let mut gap = false;
    let tokens = node
        .descendants_with_tokens()
        .filter_map(|element| element.into_token());
    for token in tokens {
        if token.kind().is_trivia() {
            gap = true;
            continue;
        }
        if gap && !text.is_empty() {
            text.push(' ');
        }
        gap = false;
        text.push_str(token.text());
    }
    text
}

/// True when a parameter is marked `const`. In `const: i64` the word is
/// the parameter's name (FN-5), so the mark is a `const` with a name
/// after it.
pub(crate) fn is_const(parameter: &SyntaxNode) -> bool {
    let names = names_before_colon(parameter);
    names.len() > 1 && names[0].text() == "const"
}

/// The parameters a function, an operator or a native function
/// declares, as written.
pub(crate) fn parameters(declaration: &SyntaxNode) -> Vec<SyntaxNode> {
    declaration
        .children()
        .filter(|list| {
            matches!(
                list.kind(),
                SyntaxKind::ConfigParamList | SyntaxKind::ParamList
            )
        })
        .flat_map(|list| list.children())
        .filter(|parameter| parameter.kind() == SyntaxKind::Param)
        .collect()
}

impl Reader<'_> {
    // ---- Reporting ----

    fn at(&self, span: Span) -> Location {
        Location::new(self.file, span)
    }
}
