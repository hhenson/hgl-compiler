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
//!   `?` is not applied to `ref<T>` or `signal` (TYPE-14);
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

use hgl_diagnostics::{Category, Diagnostics, Location};
use hgl_lexer::literal::decode_integer;
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};
use hgl_source::{FileId, Span};

use crate::constant::fold;
use crate::program::{Binding, Declaration, DeclarationKind, LocalKind, Module, Program, Role};
use crate::types::{
    Argument, Constant, ConstantValue, Detail, Field, GenericKind, GenericParameter, Pack,
    Parameter, Scalar, Signature, StructType, Type, WrittenType,
};
use crate::{codes, error, local_error};

/// Reads the types of every module, filling in [`Module::details`] and
/// [`Module::written`].
pub(crate) fn read(program: &mut Program, diagnostics: &mut Diagnostics) {
    // A generic argument is read for the parameter it is given to, so
    // the kind of every struct's parameters is settled first (GEN-13).
    let kinds = struct_kinds(program);
    for index in 0..program.modules.len() {
        let module = &program.modules[index];
        // The reader reads the whole program and so cannot change it.
        // What it finds is taken out when it is done, and written back.
        // A module is made from at least one file.
        let Some(mut reader) = Reader::new(program, module, &kinds, diagnostics) else {
            continue;
        };
        let mut details = Vec::with_capacity(module.declarations.len());
        for declaration in &module.declarations {
            details.push(reader.declaration(declaration));
        }
        for test in &module.tests {
            reader.enter(module.files[test.file].file);
            reader.scan(&test.syntax);
        }
        for (file, node) in &module.instantiations {
            reader.enter(module.files[*file].file);
            reader.scan(node);
        }
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
        module.written = written;
    }
}

/// What a generic parameter is, as far as is known (GEN-13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// A type.
    Type,
    /// A compile-time value.
    Value,
    /// A type pack.
    Pack,
    /// Not settled.
    Unknown,
}

impl Kind {
    fn of(kind: &GenericKind) -> Kind {
        match kind {
            GenericKind::Plain => Kind::Unknown,
            GenericKind::Type => Kind::Type,
            GenericKind::Pack { .. } => Kind::Pack,
            GenericKind::Value(_) => Kind::Value,
        }
    }
}

/// A declaration, by the position of its module in `Program::modules`
/// and its own in that module's `declarations`.
type Id = (usize, usize);

/// The kind of each generic parameter of every struct of the program.
type Kinds = HashMap<Id, Vec<Kind>>;

/// Settles the kind of each generic parameter of every struct.
///
/// A parameter written plain, `T`, is a type or a value by how the
/// struct's parents and fields use it (GEN-13). One use is as an
/// argument of another struct, `Grid<N>`, which says what `N` is only
/// when the kind of `Grid`'s parameter is known. So the structs are read
/// again and again, each time with what the time before settled, until a
/// reading settles nothing more. What these readings report is thrown
/// away: `read` reports it once, reading with every kind known.
fn struct_kinds(program: &Program) -> Kinds {
    let mut kinds = Kinds::new();
    for (module, declared) in program.modules.iter().enumerate() {
        for (index, declaration) in declared.declarations.iter().enumerate() {
            if declaration.kind == DeclarationKind::Struct {
                let written = generic_forms(&declaration.syntax)
                    .iter()
                    .map(|(_, _, form)| match form {
                        Form::Plain => Kind::Unknown,
                        Form::Pack(..) => Kind::Pack,
                        Form::Value => Kind::Value,
                    })
                    .collect();
                kinds.insert((module, index), written);
            }
        }
    }
    loop {
        // What this reading settles: a parameter, by its struct and its
        // position among the struct's parameters, and its kind.
        let mut settled: Vec<(Id, usize, Kind)> = Vec::new();
        for (module, declared) in program.modules.iter().enumerate() {
            let mut unheard = Diagnostics::new();
            let Some(mut reader) = Reader::new(program, declared, &kinds, &mut unheard) else {
                continue;
            };
            for (index, declaration) in declared.declarations.iter().enumerate() {
                let id = (module, index);
                let open = kinds
                    .get(&id)
                    .is_some_and(|known| known.contains(&Kind::Unknown));
                if !open {
                    continue;
                }
                reader.declaration(declaration);
                for (position, generic) in reader.generics.iter().enumerate() {
                    let now = Kind::of(&generic.kind);
                    // A kind once settled stays, so the readings end.
                    if kinds[&id].get(position) == Some(&Kind::Unknown) && now != Kind::Unknown {
                        settled.push((id, position, now));
                    }
                }
            }
        }
        if settled.is_empty() {
            return kinds;
        }
        for (id, position, kind) in settled {
            if let Some(known) = kinds.get_mut(&id) {
                known[position] = kind;
            }
        }
    }
}

/// What the signature of a declaration says of one of its plain generic
/// parameters (GEN-13).
#[derive(Debug, Clone, Default)]
struct Evidence {
    /// Where it is first used as a type.
    as_type: Option<Location>,
    /// Where it is first used as a value.
    as_value: Option<Location>,
    /// True when something this pass does not read may yet say what
    /// it is: a constraint names it, or it is given to a type whose
    /// declaration is not known.
    open: bool,
    /// True when it is given to a generic parameter of a struct whose
    /// own kind nothing settles. That says nothing of what it is.
    dependent: bool,
}

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
        pack: false,
        constant: false,
        bare: false,
    };

    /// The whole type of an input: `signal` may stand (TYPE-12).
    const INPUT: Position = Position {
        signal: true,
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
    /// True while a generic argument is read that is given to a
    /// parameter of a known struct whose kind is not settled.
    unsettled_target: bool,
    /// The types of the declaration being read, by the offset each
    /// starts at, so that none is read twice.
    read: HashMap<u32, Type>,
    /// Every type read in the module so far.
    written: Vec<WrittenType>,
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
            unsettled_target: false,
            read: HashMap::new(),
            written: Vec::new(),
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
            | SyntaxKind::TupleType
            | SyntaxKind::ListType
            | SyntaxKind::SetType
            | SyntaxKind::DictType
            | SyntaxKind::RollingType
            | SyntaxKind::RefType
            | SyntaxKind::SignalType
            | SyntaxKind::AtomicType
            | SyntaxKind::ResultType
            | SyntaxKind::ErrorType
            | SyntaxKind::DeltaType
            | SyntaxKind::NullableType
    )
}

/// The first child node of `node` of kind `kind`.
pub(crate) fn child(node: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxNode> {
    node.children().find(|child| child.kind() == kind)
}

/// The first child node of `node` that is a type.
fn type_child(node: &SyntaxNode) -> Option<SyntaxNode> {
    node.children().find(|child| is_type(child.kind()))
}

/// The tokens directly inside `node`, without whitespace and comments.
pub(crate) fn tokens(node: &SyntaxNode) -> impl Iterator<Item = SyntaxToken> {
    node.children_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
}

/// True when a token of `kind` is directly inside `node`.
pub(crate) fn has_token(node: &SyntaxNode, kind: SyntaxKind) -> bool {
    tokens(node).any(|token| token.kind() == kind)
}

/// The identifiers directly inside `node` before its `:`.
fn names_before_colon(node: &SyntaxNode) -> Vec<SyntaxToken> {
    tokens(node)
        .take_while(|token| token.kind() != SyntaxKind::Colon)
        .filter(|token| token.kind() == SyntaxKind::Ident)
        .collect()
}

/// The name a node uses: of `alias::name` the one after the `::`,
/// otherwise its first identifier. That is the token the resolver
/// recorded the use at.
fn used_name(node: &SyntaxNode) -> Option<SyntaxToken> {
    let identifiers = |node: &SyntaxNode| {
        tokens(node)
            .filter(|token| token.kind() == SyntaxKind::Ident)
            .collect::<Vec<_>>()
    };
    match child(node, SyntaxKind::QualifiedName) {
        Some(qualified) => identifiers(&qualified).into_iter().nth(1),
        None => identifiers(node).into_iter().next(),
    }
}

/// The default of a parameter or struct member: its first child node
/// after the `=`.
fn default_of(node: &SyntaxNode) -> Option<SyntaxNode> {
    node.children_with_tokens()
        .skip_while(|element| element.kind() != SyntaxKind::Eq)
        .find_map(|element| element.into_node())
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
fn is_const(parameter: &SyntaxNode) -> bool {
    let names = names_before_colon(parameter);
    names.len() > 1 && names[0].text() == "const"
}

/// What a generic parameter is written as.
enum Form {
    /// `T`
    Plain,
    /// `...Ts`, with the fewest and the most it holds.
    Pack(u64, Option<u64>),
    /// `const N: V`
    Value,
}

/// The name and form of a generic parameter.
fn generic_form(parameter: &SyntaxNode) -> Option<(SyntaxToken, Form)> {
    let name = tokens(parameter)
        .take_while(|token| token.kind() != SyntaxKind::Colon)
        .filter(|token| token.kind() == SyntaxKind::Ident)
        .last()?;
    let form = if has_token(parameter, SyntaxKind::Ellipsis) {
        let (minimum, maximum) = cardinality(child(parameter, SyntaxKind::Cardinality));
        Form::Pack(minimum, maximum)
    } else if has_token(parameter, SyntaxKind::Colon) {
        Form::Value
    } else {
        Form::Plain
    };
    Some((name, form))
}

/// The fewest and the most a pack holds (GEN-10): `{n}` is exactly n,
/// `{n:*}` n or more, `{n:m}` from n to m, and no cardinality any
/// number.
fn cardinality(node: Option<SyntaxNode>) -> (u64, Option<u64>) {
    let Some(node) = node else {
        return (0, None);
    };
    let bounds: Vec<u64> = tokens(&node)
        .filter(|token| token.kind() == SyntaxKind::Int)
        .filter_map(|token| decode_integer(token.text()).ok())
        .collect();
    // `as_slice` lets the `match` look at how many there are and what
    // they are in one step.
    match (bounds.as_slice(), has_token(&node, SyntaxKind::Colon)) {
        ([exact], false) => (*exact, Some(*exact)),
        ([minimum], true) => (*minimum, None),
        ([minimum, maximum], _) => (*minimum, Some(*maximum)),
        _ => (0, None),
    }
}

/// The generic parameters of a declaration, as written.
fn generic_forms(declaration: &SyntaxNode) -> Vec<(SyntaxNode, SyntaxToken, Form)> {
    child(declaration, SyntaxKind::GenericParams)
        .into_iter()
        .flat_map(|generics| generics.children())
        .filter(|parameter| parameter.kind() == SyntaxKind::GenericParam)
        .filter_map(|parameter| {
            let (name, form) = generic_form(&parameter)?;
            Some((parameter, name, form))
        })
        .collect()
}

/// `1 argument`, `2 arguments`.
fn arguments(count: u64) -> String {
    match count {
        1 => "1 argument".to_string(),
        count => format!("{count} arguments"),
    }
}

impl Reader<'_> {
    // ---- Reporting ----

    fn at(&self, span: Span) -> Location {
        Location::new(self.file, span)
    }

    // ---- Declarations ----

    /// Points the reader at a file, outside any declaration with generic
    /// parameters.
    fn enter(&mut self, file: FileId) {
        self.file = file;
        self.generics.clear();
        self.evidence.clear();
        self.in_signature = false;
        self.read.clear();
    }

    fn declaration(&mut self, declaration: &Declaration) -> Detail {
        self.enter(self.module.files[declaration.file].file);
        let node = &declaration.syntax;
        self.in_signature = true;
        self.reading_struct = declaration.kind == DeclarationKind::Struct;
        self.generic_parameters(node);
        let mut detail = match node.kind() {
            SyntaxKind::FnDecl | SyntaxKind::OperatorDecl | SyntaxKind::NativeFnDecl => {
                Detail::Callable(self.signature(node, declaration.kind))
            }
            SyntaxKind::StructDecl => Detail::Struct(self.structure(node)),
            _ => Detail::Plain,
        };
        self.in_signature = false;
        // GEN-13: what the signature and the constraints say each plain
        // generic parameter is.
        self.constrained(node);
        self.settle_kinds(declaration);
        match &mut detail {
            Detail::Callable(signature) => signature.generics = self.generics.clone(),
            Detail::Struct(read) => read.generics = self.generics.clone(),
            _ => {}
        }
        // Everything else the declaration writes: its constraints and
        // its body.
        self.scan(node);
        detail
    }

    /// Reads the generic parameters of a declaration into
    /// `self.generics`.
    fn generic_parameters(&mut self, declaration: &SyntaxNode) {
        let forms = generic_forms(declaration);
        // GEN-1: "Generic scope covers the whole declaration", so every
        // parameter is known before the type of any is read: in
        // `<const N: T, T>` the type of `N` is the `T` after it.
        self.generics = forms
            .iter()
            .map(|(_, name, form)| GenericParameter {
                name: name.text().to_string(),
                at: self.at(name.span()),
                kind: match form {
                    Form::Plain => GenericKind::Plain,
                    Form::Pack(minimum, maximum) => GenericKind::Pack {
                        minimum: *minimum,
                        maximum: *maximum,
                    },
                    Form::Value => GenericKind::Value(Type::Unknown),
                },
            })
            .collect();
        self.evidence = vec![Evidence::default(); self.generics.len()];

        // The type of each value parameter. A size inside one may name
        // another, declared before it or after:
        // `<const N: list<i64, D>, const D: duration>`. So each is read
        // twice: first to learn what it is, with what is reported set
        // aside and nothing recorded; then, with the type of every one
        // known, for the record.
        //
        // `mem::replace` puts an empty sink where the real one was and
        // gives the real one back, to be restored after.
        let reported = std::mem::replace(self.diagnostics, Diagnostics::new());
        let recorded = self.written.len();
        self.value_parameter_types(&forms);
        *self.diagnostics = reported;
        self.written.truncate(recorded);
        self.read.clear();
        self.value_parameter_types(&forms);
    }

    /// Reads the type of each `const N: V` among `forms` into
    /// `self.generics`.
    fn value_parameter_types(&mut self, forms: &[(SyntaxNode, SyntaxToken, Form)]) {
        for (index, (parameter, _, form)) in forms.iter().enumerate() {
            if let (Form::Value, Some(written)) = (form, type_child(parameter)) {
                self.generics[index].kind = GenericKind::Value(self.outermost(&written));
            }
        }
    }

    /// The parameters and result of a function, an operator or a native
    /// function.
    fn signature(&mut self, node: &SyntaxNode, kind: DeclarationKind) -> Signature {
        let mut parameters = Vec::new();
        let written = child(node, SyntaxKind::ParamList)
            .into_iter()
            .flat_map(|list| list.children())
            .filter(|parameter| parameter.kind() == SyntaxKind::Param);
        for parameter in written {
            let Some(name) = names_before_colon(&parameter).pop() else {
                continue;
            };
            let named_pack = child(&parameter, SyntaxKind::PackType);
            let pack = if named_pack.is_some() {
                Some(Pack::Named)
            } else if has_token(&parameter, SyntaxKind::Ellipsis) {
                Some(Pack::Positional)
            } else {
                None
            };
            // `...{T}` holds its type inside the braces.
            let ty = type_child(named_pack.as_ref().unwrap_or(&parameter))
                .map_or(Type::Unknown, |written| self.outermost(&written));
            let constant = is_const(&parameter);
            let has_default = has_token(&parameter, SyntaxKind::Eq);
            if let Some(default) = default_of(&parameter) {
                self.note_values_in(&default);
            }

            // UNIT-8: "Value-function parameters and const configuration
            // may have defaults; ordinary temporal parameters may not."
            // A native implementation states its signature again
            // (NATIVE-2), and is held to the rule as its declaration is.
            let temporal = matches!(
                kind,
                DeclarationKind::Function(Role::Temporal)
                    | DeclarationKind::NativeFunction(Role::Temporal)
                    | DeclarationKind::NativeImplementation(Role::Temporal)
            );
            if temporal && has_default && !constant && pack.is_none() {
                self.diagnostics.emit(
                    local_error(
                        Category::FunctionKind,
                        codes::local::DEFAULT_TEMPORAL_PARAMETER,
                        self.at(name.span()),
                        format!(
                            "`{}` is a temporal parameter and cannot have a default",
                            name.text()
                        ),
                        "UNIT-8",
                    )
                    .with_note(
                        "a `const` parameter, and a parameter of a `const fn`, may have one",
                    ),
                );
            }
            parameters.push(Parameter {
                name: name.text().to_string(),
                at: self.at(name.span()),
                constant,
                ty,
                pack,
                has_default,
            });
        }
        let result = child(node, SyntaxKind::ReturnType)
            .and_then(|result| type_child(&result))
            .map(|written| self.outermost(&written));
        Signature {
            generics: self.generics.clone(),
            parameters,
            result,
        }
    }

    /// The parents and members of a struct.
    fn structure(&mut self, node: &SyntaxNode) -> StructType {
        let parents: Vec<Type> = child(node, SyntaxKind::StructBases)
            .into_iter()
            .flat_map(|bases| bases.children())
            .filter(|parent| is_type(parent.kind()))
            .map(|parent| self.outermost(&parent))
            .collect();
        let mut fields = Vec::new();
        for member in node
            .children()
            .filter(|member| member.kind() == SyntaxKind::StructMember)
        {
            // A member's name is its first token. It may be a reserved
            // word (LEX-9).
            let Some(name) = tokens(&member).next() else {
                continue;
            };
            // `name = value` has no `:` and no type: it overrides the
            // default of a field a parent declares (TYPE-4).
            let ty = if has_token(&member, SyntaxKind::Colon) {
                Some(type_child(&member).map_or(Type::Unknown, |written| self.outermost(&written)))
            } else {
                None
            };
            if let Some(default) = default_of(&member) {
                self.note_values_in(&default);
            }
            // The default is the member's child that is no type.
            let null_default = member
                .children()
                .find(|part| !is_type(part.kind()))
                .is_some_and(|default| {
                    default.kind() == SyntaxKind::Literal
                        && tokens(&default)
                            .next()
                            .is_some_and(|token| token.kind() == SyntaxKind::KwNull)
                });
            fields.push(Field {
                name: name.text().to_string(),
                at: self.at(name.span()),
                ty,
                has_default: has_token(&member, SyntaxKind::Eq),
                null_default,
            });
        }
        StructType {
            generics: self.generics.clone(),
            parents,
            fields,
        }
    }

    // ---- Finding the types ----

    /// Reads every type written in `root` that has not been read.
    ///
    /// `descendants` walks the whole tree under `root` without calling
    /// itself, so a deep tree costs no call stack here.
    fn scan(&mut self, root: &SyntaxNode) {
        for node in root.descendants() {
            match node.kind() {
                kind if is_type(kind) => {
                    // A type inside another is read with it, and the
                    // arguments of a constructor with the constructor.
                    let inside = node.parent().is_some_and(|parent| {
                        is_type(parent.kind()) || parent.kind() == SyntaxKind::GenericArgs
                    });
                    if !inside {
                        self.outermost(&node);
                    }
                }
                SyntaxKind::TypedConstructor => self.constructor(&node),
                SyntaxKind::UndecidedConstructor => {
                    // GEN-14: the resolver decided what the node is.
                    if self.constructors.contains(&(self.file, node.span().start)) {
                        self.constructor(&node);
                    }
                }
                SyntaxKind::DeltaConstructor => self.delta_constructor(&node),
                SyntaxKind::Instantiation => {
                    // GEN-8: `add<i64, _>` requests candidates. What the
                    // arguments are checked against is for the pass that
                    // selects them.
                    if let Some(list) = child(&node, SyntaxKind::GenericArgs) {
                        self.arguments(&list, Position::OPEN, None);
                    }
                }
                _ => {}
            }
        }
    }

    /// Reads a type that stands in no other type, once.
    fn outermost(&mut self, node: &SyntaxNode) -> Type {
        let span = node.span();
        if let Some(ty) = self.read.get(&span.start) {
            return ty.clone();
        }
        let position = self.position(node);
        let ty = self.type_(node, position);
        self.record(span, ty)
    }

    fn record(&mut self, span: Span, ty: Type) -> Type {
        self.read.insert(span.start, ty.clone());
        self.written.push(WrittenType {
            at: self.at(span),
            ty: ty.clone(),
        });
        ty
    }

    /// What the place `node` is written in admits, from the node around
    /// it.
    fn position(&self, node: &SyntaxNode) -> Position {
        let Some(parent) = node.parent() else {
            return Position::OPEN;
        };
        match parent.kind() {
            SyntaxKind::Param => {
                if has_token(&parent, SyntaxKind::Ellipsis) {
                    // FN-8: "`...T` captures the positional ones: all of
                    // type T, or each of its own type when T is a
                    // declared type pack." Each one captured is an
                    // input, so `...signal` is a pack of signals.
                    Position {
                        signal: true,
                        ..Position::PACK
                    }
                } else if is_const(&parent) {
                    // FN-1: "Parameter-level `const` instead fixes
                    // configuration."
                    Position::value("a `const` parameter", "FN-1")
                } else if has_token(&parent, SyntaxKind::Eq) {
                    // UNIT-8: a default is a constant, so what takes one
                    // is an ordinary value.
                    Position::value("a parameter with a default", "UNIT-8")
                } else {
                    Position::INPUT
                }
            }
            SyntaxKind::PackType => Position::PACK,
            SyntaxKind::ReturnType => Position::PLAIN,
            SyntaxKind::StructMember | SyntaxKind::StructBases => Position::PLAIN,
            // GEN-13: "An explicit `const N: V` declares a value
            // parameter of ordinary type V."
            SyntaxKind::GenericParam => Position::value("a generic value parameter", "GEN-13"),
            // GEN-9: "Property clauses apply to explicit concrete
            // domains", written `properties<value_type, ...>`.
            SyntaxKind::OperatorProperties => {
                Position::value("the domain of an operator's properties", "GEN-9")
            }
            // A local's annotation and an anonymous function's
            // parameter: which types those may have is for the pass
            // that knows the phase.
            SyntaxKind::LocalDecl
            | SyntaxKind::StateDecl
            | SyntaxKind::CacheDecl
            | SyntaxKind::LambdaParam => Position::INPUT,
            _ => Position::OPEN,
        }
    }

    // ---- Reading a type ----

    /// Reads the type `node` is, checking what it is made of against
    /// what `position` admits. It calls itself for the types inside:
    /// as deep as the type is nested, which the parser bounds.
    fn type_(&mut self, node: &SyntaxNode, position: Position) -> Type {
        match node.kind() {
            SyntaxKind::ScalarType => tokens(node)
                .next()
                .and_then(|word| Scalar::from_keyword(word.kind()))
                .map_or(Type::Unknown, Type::Scalar),
            SyntaxKind::ErrorType => Type::Error,
            SyntaxKind::SignalType => {
                self.signal(node, position);
                Type::Signal
            }
            SyntaxKind::NullableType => self.nullable(node, position),
            SyntaxKind::NamedType => self.named(node, position),
            SyntaxKind::TupleType => Type::Tuple(
                node.children()
                    .filter(|element| is_type(element.kind()))
                    .map(|element| self.type_(&element, position.part()))
                    .collect(),
            ),
            SyntaxKind::ListType => self.list(node, position),
            // The grammar: `set_type = "set", "<", value_type, ">"`.
            SyntaxKind::SetType => Type::Set(Box::new(
                self.inner(node, Position::value("a set's element", "TYPE-9")),
            )),
            SyntaxKind::DictType => self.dict(node, position),
            SyntaxKind::RollingType => self.rolling(node, position),
            SyntaxKind::RefType => {
                self.ordinary(node, position, true);
                Type::Ref(Box::new(self.inner(node, Position::PLAIN)))
            }
            SyntaxKind::AtomicType => {
                self.ordinary(node, position, false);
                // TYPE-9: "`atomic<V>` stops expansion at a complete
                // ordinary V."
                Type::Atomic(Box::new(
                    self.inner(node, Position::value("an atomic payload", "TYPE-9")),
                ))
            }
            SyntaxKind::ResultType => self.result(node),
            SyntaxKind::DeltaType => Type::Delta(Box::new(self.inner(node, Position::PLAIN))),
            _ => Type::Unknown,
        }
    }

    /// The one type inside `node`, read for `position`.
    fn inner(&mut self, node: &SyntaxNode, position: Position) -> Type {
        type_child(node).map_or(Type::Unknown, |inner| self.type_(&inner, position))
    }

    /// Reports `node`, which is no ordinary value type, where only one
    /// may stand. `endpoint` is true for `ref` and `signal`.
    fn ordinary(&mut self, node: &SyntaxNode, position: Position, endpoint: bool) {
        let Some((what, place_rule)) = position.value else {
            return;
        };
        // TYPE-12 says where `ref` and `signal` may not stand. For the
        // others, and under a `?`, the place's own rule asks for an
        // ordinary value.
        let rule = if endpoint && place_rule != "TYPE-14" {
            "TYPE-12"
        } else {
            place_rule
        };
        self.diagnostics.emit(
            local_error(
                Category::Type,
                codes::local::TYPE_NOT_ORDINARY,
                self.at(node.span()),
                format!(
                    "`{}` is not an ordinary value type, and {what} is one",
                    text_of(node)
                ),
                rule,
            )
            .with_note("`rolling`, `ref`, `atomic` and `signal` are not ordinary value types"),
        );
    }

    /// TYPE-12: "`signal` is a complete temporal input constraint with
    /// no readable payload; it is not a result, nested shape, field, or
    /// const value."
    fn signal(&mut self, node: &SyntaxNode, position: Position) {
        if position.value.is_some() {
            self.ordinary(node, position, true);
        } else if !position.signal {
            self.diagnostics.emit(
                local_error(
                    Category::Type,
                    codes::local::SIGNAL_POSITION,
                    self.at(node.span()),
                    "`signal` is the whole type of an input, and nothing else".to_string(),
                    "TYPE-12",
                )
                .with_note("it is not a result, a field, or a part of another type"),
            );
        }
    }

    /// `T?` (TYPE-14, TYPE-15).
    ///
    /// TYPE-14: "`T?` denotes an ordinary value that is either a T or
    /// null, where T is an admitted ordinary value type." TYPE-15:
    /// "The suffix admits only ordinary value types; applying it to
    /// `ref<T>` or `signal` rejects under TYPE-12/14 rather than
    /// creating nullable endpoint handles."
    fn nullable(&mut self, node: &SyntaxNode, position: Position) -> Type {
        let Some(base) = type_child(node) else {
            return Type::Unknown;
        };
        // The whole of what `?` is applied to is an ordinary value
        // type: `list<ref<f64>>?` is no more one than `ref<f64>?`.
        // Where the place around it asks for one already, that place is
        // the one named.
        let base_position = match position.value {
            Some(_) => position,
            None => Position::value("what `?` is applied to", "TYPE-14"),
        };
        Type::Nullable(Box::new(self.type_(&base, base_position)))
    }

    /// `list<T>`, `list<T, size>` (TYPE-10).
    fn list(&mut self, node: &SyntaxNode, position: Position) -> Type {
        let element = self.inner(node, position.part());
        // The size is the child after the element: `unbounded`, or an
        // expression.
        let size = node
            .children()
            .find(|part| !is_type(part.kind()) && part.kind() != SyntaxKind::Error)
            .map(|size| self.list_size(&size));
        Type::List {
            element: Box::new(element),
            size,
        }
    }

    /// TYPE-10: "fixed sizes are nonnegative constants. `unbounded` is
    /// the size sentinel."
    fn list_size(&mut self, node: &SyntaxNode) -> Constant {
        if node.kind() == SyntaxKind::UnboundedSize {
            return Constant {
                text: "unbounded".to_string(),
                value: ConstantValue::Unbounded,
            };
        }
        let size = self.constant(node);
        let problem = match &size.value {
            ConstantValue::Int(count) if *count < 0 => {
                Some("a list's size is not negative".to_string())
            }
            ConstantValue::Duration(_) => {
                Some("a list's size is a count, and this is a duration".to_string())
            }
            ConstantValue::Other(what) => {
                Some(format!("a list's size is a count, and this is {what}"))
            }
            ConstantValue::Parameter { name, at } => match self.value_type(*at) {
                Some(Type::Scalar(Scalar::I64) | Type::Parameter { .. } | Type::Unknown) | None => {
                    None
                }
                Some(other) => Some(format!(
                    "a list's size is a count, and `{name}` is a constant of type `{other}`"
                )),
            },
            _ => None,
        };
        if let Some(problem) = problem {
            self.diagnostics.emit(local_error(
                Category::Type,
                codes::local::LIST_SIZE,
                self.at(node.span()),
                problem,
                "TYPE-10",
            ));
        }
        size
    }

    /// `dict<K, V>`: `dict_type = "dict", "<", value_type, ",", type, ">"`.
    fn dict(&mut self, node: &SyntaxNode, position: Position) -> Type {
        let mut parts = node.children().filter(|part| is_type(part.kind()));
        let key = parts.next().map_or(Type::Unknown, |key| {
            self.type_(&key, Position::value("a dictionary's key", "TYPE-9"))
        });
        let value = parts
            .next()
            .map_or(Type::Unknown, |value| self.type_(&value, position.part()));
        Type::Dict {
            key: Box::new(key),
            value: Box::new(value),
        }
    }

    /// `result<T, E>`: both payloads are value types, and "result and
    /// error envelopes are ordinary atomic values" (TYPE-13).
    fn result(&mut self, node: &SyntaxNode) -> Type {
        let position = Position::value("a result's payload", "TYPE-13");
        let mut parts = node.children().filter(|part| is_type(part.kind()));
        let mut next = |reader: &mut Self| {
            parts
                .next()
                .map_or(Type::Unknown, |part| reader.type_(&part, position))
        };
        let value = next(self);
        let error = next(self);
        Type::Result {
            value: Box::new(value),
            error: Box::new(error),
        }
    }

    // ---- Windows (TYPE-11) ----

    /// `rolling<V, maximum>` or `rolling<V, maximum, minimum>`.
    fn rolling(&mut self, node: &SyntaxNode, position: Position) -> Type {
        self.ordinary(node, position, false);
        let value = self.inner(node, Position::value("a window's value", "TYPE-11"));
        let mut sizes = node
            .children()
            .filter(|part| !is_type(part.kind()) && part.kind() != SyntaxKind::Error);
        let Some(maximum_node) = sizes.next() else {
            // The parser has reported the missing size.
            return Type::Unknown;
        };
        let minimum_node = sizes.next();
        let maximum = self.constant(&maximum_node);
        let minimum = minimum_node.as_ref().map(|node| self.constant(node));
        // `zip` pairs the two when both are there.
        self.window_sizes(
            &maximum_node,
            &maximum,
            minimum_node.as_ref().zip(minimum.as_ref()),
        );
        // "Omitted minimum equals maximum": a minimum written as the
        // maximum is the same window as one with none.
        let minimum = minimum.filter(|minimum| *minimum != maximum);
        Type::Rolling {
            value: Box::new(value),
            maximum,
            minimum,
        }
    }

    /// TYPE-11: "A rolling maximum/minimum has one constant kind: tick
    /// count or duration. Omitted minimum equals maximum. Tick sizes are
    /// positive; duration maximum is positive and minimum may be zero.
    /// Minimum cannot exceed maximum."
    fn window_sizes(
        &mut self,
        maximum_node: &SyntaxNode,
        maximum: &Constant,
        minimum: Option<(&SyntaxNode, &Constant)>,
    ) {
        let maximum_kind = self.size_kind(maximum_node, maximum);
        let minimum_kind = match minimum {
            Some((node, size)) => self.size_kind(node, size),
            None => Some(SizeKind::Unknown),
        };
        // A size of the wrong kind has been reported; its bounds mean
        // nothing.
        let (Some(maximum_kind), Some(minimum_kind)) = (maximum_kind, minimum_kind) else {
            return;
        };
        if let Some((node, _)) = minimum {
            let mixed = matches!(
                (maximum_kind, minimum_kind),
                (SizeKind::Ticks, SizeKind::Time) | (SizeKind::Time, SizeKind::Ticks)
            );
            if mixed {
                self.diagnostics.emit(
                    error(
                        Category::Type,
                        codes::ROLLING_SIZE_KIND,
                        self.at(node.span()),
                        format!(
                            "a window's sizes are of one kind: the maximum is {} and the \
                             minimum is {}",
                            maximum_kind.describe(),
                            minimum_kind.describe()
                        ),
                        "TYPE-11",
                    )
                    .with_related(self.at(maximum_node.span()), "the maximum"),
                );
                return;
            }
        }

        // The bounds of what is known.
        match maximum.value {
            ConstantValue::Int(ticks) if ticks <= 0 => {
                self.bounds(maximum_node, "a window holds at least one tick");
            }
            ConstantValue::Duration(length) if length <= 0 => {
                self.bounds(maximum_node, "a window's duration is more than zero");
            }
            _ => {}
        }
        let Some((minimum_node, minimum)) = minimum else {
            return;
        };
        match (&maximum.value, &minimum.value) {
            (_, ConstantValue::Int(ticks)) if *ticks <= 0 => {
                self.bounds(minimum_node, "a window's minimum is at least one tick");
            }
            (_, ConstantValue::Duration(length)) if *length < 0 => {
                self.bounds(minimum_node, "a window's minimum duration is not negative");
            }
            (ConstantValue::Int(maximum), ConstantValue::Int(minimum))
            | (ConstantValue::Duration(maximum), ConstantValue::Duration(minimum))
                if minimum > maximum =>
            {
                self.bounds(minimum_node, "a window's minimum cannot exceed its maximum");
            }
            _ => {}
        }
    }

    fn bounds(&mut self, node: &SyntaxNode, message: &str) {
        self.diagnostics.emit(error(
            Category::Type,
            codes::ROLLING_SIZE_BOUNDS,
            self.at(node.span()),
            message.to_string(),
            "TYPE-11",
        ));
    }

    /// The kind of one size of a window; `None`, with a report, when it
    /// is neither a tick count nor a duration.
    fn size_kind(&mut self, node: &SyntaxNode, size: &Constant) -> Option<SizeKind> {
        let wrong = match &size.value {
            ConstantValue::Int(_) => return Some(SizeKind::Ticks),
            ConstantValue::Duration(_) => return Some(SizeKind::Time),
            ConstantValue::Unevaluated => return Some(SizeKind::Unknown),
            ConstantValue::Parameter { name, at } => match self.value_type(*at) {
                Some(Type::Scalar(Scalar::I64)) => return Some(SizeKind::Ticks),
                Some(Type::Scalar(Scalar::Duration)) => return Some(SizeKind::Time),
                Some(Type::Parameter { .. } | Type::Unknown) | None => {
                    return Some(SizeKind::Unknown);
                }
                Some(other) => format!("`{name}` is a constant of type `{other}`"),
            },
            ConstantValue::Unbounded => "this is `unbounded`".to_string(),
            ConstantValue::Other(what) => format!("this is {what}"),
        };
        self.diagnostics.emit(error(
            Category::Type,
            codes::ROLLING_SIZE_KIND,
            self.at(node.span()),
            format!("a window's size is a tick count or a duration, and {wrong}"),
            "TYPE-11",
        ));
        None
    }

    // ---- Named types ----

    /// `Quote`, `pc::Curve`, `Box<i64>`, or a generic parameter.
    fn named(&mut self, node: &SyntaxNode, position: Position) -> Type {
        let list = child(node, SyntaxKind::GenericArgs);
        let name = used_name(node);
        let binding = name
            .as_ref()
            .and_then(|name| self.bindings.get(&(self.file, name.span().start)).copied());
        // The type the name is bound to, when it is one. A type is one
        // declaration; only a pair of functions is two (FN-4).
        let target: Option<Id> = match binding {
            Some(Binding::Declarations {
                module,
                declarations,
            }) => {
                let declared = &self.program.modules[*module].declarations;
                declarations
                    .iter()
                    .copied()
                    .find(|&index| {
                        matches!(
                            declared[index].kind,
                            DeclarationKind::Struct
                                | DeclarationKind::Enum
                                | DeclarationKind::NativeType
                        )
                    })
                    .map(|index| (*module, index))
            }
            _ => None,
        };
        // The arguments are read whatever the name is, so that what is
        // wrong inside them is reported.
        let arguments = list.as_ref().map_or(Vec::new(), |list| {
            self.arguments(list, Position::ARGUMENT, target)
        });
        // No binding: the name is left for the prelude, or the resolver
        // has reported it.
        let (Some(name), Some(binding)) = (name, binding) else {
            return Type::Unknown;
        };
        match binding {
            Binding::Local {
                at,
                kind: LocalKind::GenericParameter,
            } => {
                // `...{Fields<i64>}` names no type pack, and has been
                // reported as that (FN-8).
                let in_named_pack = node
                    .parent()
                    .is_some_and(|parent| parent.kind() == SyntaxKind::PackType);
                self.generic(&name, *at, list.is_some() && !in_named_pack, position)
            }
            Binding::Local { at, kind } => {
                self.not_a_type(&name, kind.describe(), *at);
                Type::Unknown
            }
            Binding::Declarations {
                module,
                declarations,
            } => {
                let declared = &self.program.modules[*module].declarations;
                let Some((_, index)) = target else {
                    let first = &declared[declarations[0]];
                    let what = match first.kind {
                        DeclarationKind::Operator | DeclarationKind::Implementation(_) => {
                            "an operator".to_string()
                        }
                        kind => format!("a {}", kind.describe()),
                    };
                    self.not_a_type(&name, &what, first.at);
                    return Type::Unknown;
                };
                let declaration = &declared[index];
                self.argument_count(&name, declaration, list.as_ref(), position);
                Type::Named {
                    name: declaration.name.clone(),
                    module: *module,
                    declaration: index,
                    arguments,
                }
            }
        }
    }

    fn not_a_type(&mut self, name: &SyntaxToken, what: &str, declared: Location) {
        self.diagnostics.emit(
            local_error(
                Category::Type,
                codes::local::TYPE_NOT_A_TYPE,
                self.at(name.span()),
                format!("`{}` is {what}, not a type", name.text()),
                "UNIT-10",
            )
            .with_related(declared, "declared here"),
        );
    }

    /// A generic parameter named as a type (GEN-13): "A generic
    /// parameter denotes a type, type pack, or compile-time ordinary
    /// value."
    fn generic(
        &mut self,
        name: &SyntaxToken,
        declared: Location,
        applied: bool,
        position: Position,
    ) -> Type {
        let parameter = Type::Parameter {
            name: name.text().to_string(),
            at: declared,
        };
        if applied {
            self.diagnostics.emit(
                local_error(
                    Category::Type,
                    codes::local::GENERIC_ARGUMENT_COUNT,
                    self.at(name.span()),
                    format!(
                        "`{}` is a generic parameter and takes no generic arguments",
                        name.text()
                    ),
                    "GEN-13",
                )
                .with_related(declared, "declared here"),
            );
            // It stands where a type does, which is what it is taken
            // for: one report for one fault.
            self.note_type(declared, self.at(name.span()));
            return parameter;
        }
        // One bound by `each T in ...` is not among the declaration's
        // own; it is a type.
        let kind = self
            .generics
            .iter()
            .find(|generic| generic.at == declared)
            .map(|generic| Kind::of(&generic.kind));
        match kind {
            // GEN-2: "A type pack is a sequence, not a singular source
            // type."
            Some(Kind::Pack) if !position.pack => {
                self.diagnostics.emit(
                    local_error(
                        Category::Type,
                        codes::local::TYPE_NOT_A_TYPE,
                        self.at(name.span()),
                        format!(
                            "`{}` is a type pack: a sequence of types, not one type",
                            name.text()
                        ),
                        "GEN-2",
                    )
                    .with_related(declared, "declared here"),
                );
                Type::Unknown
            }
            Some(Kind::Value) if !position.constant => {
                self.diagnostics.emit(
                    local_error(
                        Category::Type,
                        codes::local::TYPE_NOT_A_TYPE,
                        self.at(name.span()),
                        format!("`{}` is a generic value parameter, not a type", name.text()),
                        "GEN-13",
                    )
                    .with_related(declared, "declared here"),
                );
                Type::Unknown
            }
            // GEN-13: a plain parameter named where a type stands is
            // used as a type. Where a value might stand as well, the
            // use says nothing yet.
            Some(Kind::Unknown) => {
                if !position.constant {
                    self.note_type(declared, self.at(name.span()));
                } else if self.unsettled_target {
                    self.note_dependent(declared);
                } else {
                    self.note_open(declared);
                }
                parameter
            }
            _ => parameter,
        }
    }

    /// GEN-2: "A generic struct application supplies all arguments ...
    /// Partial applications, generic defaults, and `_` struct arguments
    /// are not admitted."
    fn argument_count(
        &mut self,
        name: &SyntaxToken,
        declaration: &Declaration,
        list: Option<&SyntaxNode>,
        position: Position,
    ) {
        // How many the type declares: its fixed parameters, and what a
        // pack among them may add (GEN-10).
        let mut fixed: u64 = 0;
        let mut pack = None;
        let forms = generic_forms(&declaration.syntax);
        for (_, _, form) in &forms {
            match form {
                Form::Pack(minimum, maximum) => pack = Some((*minimum, *maximum)),
                Form::Plain | Form::Value => fixed += 1,
            }
        }
        // A cardinality may be any integer literal, so the sums are
        // taken with care. `saturating_add` stops at the largest `u64`
        // where a sum would pass it; `checked_add` gives `None`, which
        // here is no limit.
        let fewest = fixed.saturating_add(pack.map_or(0, |(minimum, _)| minimum));
        // `None`: no limit.
        let most = match pack {
            None => Some(fixed),
            Some((_, maximum)) => maximum.and_then(|maximum| fixed.checked_add(maximum)),
        };
        let takes = match most {
            Some(most) if most == fewest => arguments(fewest),
            Some(most) => format!("from {fewest} to {most} arguments"),
            None => format!("at least {}", arguments(fewest)),
        };

        let problem = match list {
            // A type that needs none may be named bare.
            None if fewest == 0 || position.bare => return,
            None => format!(
                "`{}` is a generic type and is given no arguments: it takes {takes}",
                declaration.name
            ),
            Some(_) if forms.is_empty() => format!(
                "`{}` is not a generic type and takes no generic arguments",
                declaration.name
            ),
            Some(list) => {
                // How many are given. An argument that names a type pack
                // passes on as many types as the pack holds (GEN-10),
                // which is a range until the pack is resolved.
                let mut given_fewest: u64 = 0;
                let mut given_most: Option<u64> = Some(0);
                for argument in list
                    .children()
                    .filter(|argument| argument.kind() != SyntaxKind::Error)
                {
                    let (minimum, maximum) = self.pack_bounds(&argument).unwrap_or((1, Some(1)));
                    given_fewest = given_fewest.saturating_add(minimum);
                    // `zip` gives `None`, no limit, when either has
                    // none; a sum past the largest `u64` is no limit
                    // either.
                    given_most = given_most
                        .zip(maximum)
                        .and_then(|(most, more)| most.checked_add(more));
                }
                // Rejected only when no length the packs may have fits.
                // Whether the length a pack is resolved to fits is for
                // the pass that resolves it.
                let too_many = most.is_some_and(|most| given_fewest > most);
                let too_few = given_most.is_some_and(|given_most| given_most < fewest);
                if !too_many && !too_few {
                    return;
                }
                let given = match given_most {
                    Some(1) if given_fewest == 1 => "1 is".to_string(),
                    Some(most) if most == given_fewest => format!("{most} are"),
                    Some(most) => format!("from {given_fewest} to {most} are"),
                    // The sum stopped at the largest `u64`.
                    None if given_fewest == u64::MAX => "more than can be counted are".to_string(),
                    None => format!("at least {given_fewest} are"),
                };
                format!("`{}` takes {takes}, and {given} given", declaration.name)
            }
        };
        self.diagnostics.emit(
            local_error(
                Category::Type,
                codes::local::GENERIC_ARGUMENT_COUNT,
                self.at(name.span()),
                problem,
                "GEN-2",
            )
            .with_related(declaration.at, "declared here"),
        );
    }

    /// How many types a generic argument passes on when it names a type
    /// pack of the declaration being read: the fewest and the most
    /// (GEN-10). `None` for any other argument, which is one.
    fn pack_bounds(&self, argument: &SyntaxNode) -> Option<(u64, Option<u64>)> {
        if argument.kind() != SyntaxKind::NamedType
            || child(argument, SyntaxKind::GenericArgs).is_some()
        {
            return None;
        }
        let name = used_name(argument)?;
        let &binding = self.bindings.get(&(self.file, name.span().start))?;
        let Binding::Local {
            at,
            kind: LocalKind::GenericParameter,
        } = binding
        else {
            return None;
        };
        self.generics
            .iter()
            .find(|generic| generic.at == *at)
            .and_then(|generic| match generic.kind {
                GenericKind::Pack { minimum, maximum } => Some((minimum, maximum)),
                _ => None,
            })
    }

    /// The arguments of `<...>`: each a type or a constant.
    ///
    /// `target` is the generic type they are given to, when the name
    /// before them is one. Each argument is then read for the parameter
    /// it is given to (GEN-14): "Check each argument against the
    /// corresponding parameter kind."
    fn arguments(
        &mut self,
        list: &SyntaxNode,
        position: Position,
        target: Option<Id>,
    ) -> Vec<Argument> {
        // The name and kind of each generic parameter of the target.
        let (target_name, parameters): (String, Vec<(String, Kind)>) = match target {
            Some((module, index)) => {
                let declaration = &self.program.modules[module].declarations[index];
                let kinds = self.kinds.get(&(module, index));
                let parameters = generic_forms(&declaration.syntax)
                    .iter()
                    .enumerate()
                    .map(|(position, (_, name, _))| {
                        let kind = kinds
                            .and_then(|kinds| kinds.get(position))
                            .copied()
                            .unwrap_or(Kind::Unknown);
                        (name.text().to_string(), kind)
                    })
                    .collect();
                (declaration.name.clone(), parameters)
            }
            None => (String::new(), Vec::new()),
        };

        let mut arguments = Vec::new();
        // The parameter the next argument is given to. A pack takes
        // every argument from its place on. An argument that passes a
        // pack on stands for as many as the pack holds: when that is
        // one number the place of those after it is still known, and
        // otherwise it is not.
        let mut next: usize = 0;
        let mut placed = true;
        // The place of the target's pack parameter, when it has one.
        let pack_at = parameters.iter().position(|(_, kind)| *kind == Kind::Pack);
        for argument in list.children() {
            let kind = argument.kind();
            if kind == SyntaxKind::Error {
                continue;
            }
            // How many arguments this one stands for: one, or for the
            // name of a type pack as many as the pack holds, which is
            // at least a number and may be exactly one.
            let (at_least, exactly) = match self.pack_bounds(&argument) {
                None => (1, Some(1)),
                Some((minimum, maximum)) => {
                    (minimum, maximum.filter(|maximum| *maximum == minimum))
                }
            };
            // A pack that may hold nothing is given to no parameter for
            // certain.
            let parameter = if placed && at_least > 0 {
                parameters.get(next)
            } else {
                None
            };
            let want = parameter.map_or(Kind::Unknown, |(_, kind)| *kind);
            // A pack of several types fills as many places as it holds
            // at least. Each after the first is a type given to that
            // parameter too, and a value parameter among them takes
            // none (GEN-14).
            let covered = usize::try_from(at_least).unwrap_or(usize::MAX);
            let value_slot = parameters
                .iter()
                .skip(next.saturating_add(1))
                .take(covered.saturating_sub(1))
                .take_while(|(_, kind)| *kind != Kind::Pack)
                .find(|(_, kind)| *kind == Kind::Value)
                .filter(|_| placed);
            if let Some((name, _)) = value_slot {
                let message = format!(
                    "`{target_name}` takes a value for `{name}`, and `{}` passes on types",
                    text_of(&argument)
                );
                self.argument_kind(&argument, Some(message));
            }
            match exactly {
                // A pack parameter takes them all and stays the place.
                Some(_) if want == Kind::Pack => {}
                // A count past what a `usize` holds is past every
                // parameter.
                Some(count) => {
                    next = next.saturating_add(usize::try_from(count).unwrap_or(usize::MAX));
                    // What runs past the fixed parameters is given to
                    // the pack after them, when there is one.
                    if let Some(pack) = pack_at {
                        next = next.min(pack);
                    }
                }
                // How many it holds is a range: the place of the
                // arguments after it is not known.
                None => placed = false,
            }
            // What to say when the argument is of the other kind.
            let mismatch = |wanted: &str, given: &str| {
                parameter.map(|(name, _)| {
                    format!(
                        "`{target_name}` takes {wanted} for `{name}`, and `{}` is {given}",
                        text_of(&argument)
                    )
                })
            };

            if kind == SyntaxKind::Hole {
                // GEN-8: `_` in an `instantiate` keeps a parameter open.
                arguments.push(Argument::Type(Type::Unknown));
            } else if !is_type(kind) {
                // An expression is a value.
                if matches!(want, Kind::Type | Kind::Pack) {
                    self.argument_kind(&argument, mismatch("a type", "a value"));
                }
                arguments.push(Argument::Constant(self.constant(&argument)));
            } else if let Some(constant) = self.value_name(&argument, want) {
                // The parser reads a lone name as a type. One that names
                // a value is a constant.
                arguments.push(Argument::Constant(constant));
            } else {
                let position = match want {
                    // Where a type goes, a type pack may be passed on.
                    Kind::Type | Kind::Pack => Position::PACK,
                    Kind::Value | Kind::Unknown => position,
                };
                // A lone name given to a parameter of the target whose
                // kind nothing has settled says nothing of what the
                // name is (GEN-13).
                let unsettled = parameter.is_some() && want == Kind::Unknown;
                let before = std::mem::replace(&mut self.unsettled_target, unsettled);
                let ty = self.type_(&argument, position);
                self.unsettled_target = before;
                // A name that is not known may yet be a constant; one
                // known to be a type is not: a declared type, or a
                // generic parameter settled as a type.
                let is_type = kind != SyntaxKind::NamedType
                    || matches!(ty, Type::Named { .. })
                    || self.is_type_parameter(&ty);
                if want == Kind::Value && is_type {
                    self.argument_kind(&argument, mismatch("a value", "a type"));
                }
                arguments.push(Argument::Type(ty));
            }
        }
        arguments
    }

    /// True when `ty` is a generic parameter of the declaration being
    /// read that is known to stand for types: one settled as a type, or
    /// a type pack.
    fn is_type_parameter(&self, ty: &Type) -> bool {
        matches!(ty, Type::Parameter { at, .. } if self.generics.iter().any(|generic| {
            generic.at == *at
                && matches!(generic.kind, GenericKind::Type | GenericKind::Pack { .. })
        }))
    }

    fn argument_kind(&mut self, argument: &SyntaxNode, message: Option<String>) {
        if let Some(message) = message {
            self.diagnostics.emit(local_error(
                Category::Type,
                codes::local::GENERIC_ARGUMENT_KIND,
                self.at(argument.span()),
                message,
                "GEN-14",
            ));
        }
    }

    /// The constant a lone name is, when it names a value: a generic
    /// parameter declared `const N: V`; a plain one given where a value
    /// goes, which that use makes a value (GEN-13); or a local, a
    /// parameter or another binding of a body. Whether such a binding
    /// may stand in a constant is for the pass that evaluates
    /// constants.
    ///
    /// `want` is the kind of the parameter the name is given to. Where
    /// a type goes, a name is read as a type.
    fn value_name(&mut self, node: &SyntaxNode, want: Kind) -> Option<Constant> {
        if matches!(want, Kind::Type | Kind::Pack)
            || node.kind() != SyntaxKind::NamedType
            || child(node, SyntaxKind::GenericArgs).is_some()
        {
            return None;
        }
        let name = used_name(node)?;
        let &binding = self.bindings.get(&(self.file, name.span().start))?;
        let value = match binding {
            Binding::Local {
                at,
                kind: LocalKind::GenericParameter,
            } => {
                // `?` on an `Option` leaves the function with `None`
                // when there is nothing: here, for a parameter bound by
                // `each ... in`, which is a type.
                let kind = self
                    .generics
                    .iter()
                    .find(|generic| generic.at == *at)
                    .map(|generic| Kind::of(&generic.kind))?;
                match kind {
                    Kind::Value => {}
                    Kind::Unknown if want == Kind::Value => {
                        self.note_value(*at, self.at(name.span()));
                    }
                    _ => return None,
                }
                ConstantValue::Parameter {
                    name: name.text().to_string(),
                    at: *at,
                }
            }
            Binding::Local { .. } => ConstantValue::Unevaluated,
            Binding::Declarations { .. } => return None,
        };
        Some(Constant {
            text: name.text().to_string(),
            value,
        })
    }

    /// The type of the generic value parameter declared at `declared`;
    /// `None` when what is declared there is not one.
    fn value_type(&self, declared: Location) -> Option<&Type> {
        self.generics
            .iter()
            .find(|generic| generic.at == declared)
            .and_then(|generic| match &generic.kind {
                GenericKind::Value(ty) => Some(ty),
                _ => None,
            })
    }

    // ---- Constants ----

    /// The constant expression `node`, folded as far as it needs no
    /// types.
    fn constant(&mut self, node: &SyntaxNode) -> Constant {
        // A closure: a function written in place, which may use what is
        // in scope around it; here, `self`.
        let name = |used: &SyntaxNode| {
            let Some(name) = used_name(used) else {
                return ConstantValue::Unevaluated;
            };
            match self.bindings.get(&(self.file, name.span().start)) {
                Some(Binding::Local {
                    at,
                    kind: LocalKind::GenericParameter,
                }) => ConstantValue::Parameter {
                    name: name.text().to_string(),
                    at: *at,
                },
                _ => ConstantValue::Unevaluated,
            }
        };
        let constant = Constant {
            text: text_of(node),
            value: fold(node, &name),
        };
        self.note_values_in(node);
        constant
    }

    /// GEN-13: a generic parameter named in an expression of the
    /// signature, a size or a default, is used as a value.
    fn note_values_in(&mut self, expression: &SyntaxNode) {
        let named: Vec<(Location, Location)> = expression
            .descendants()
            .filter(|used| used.kind() == SyntaxKind::NameExpr)
            .filter_map(|used| {
                let name = used_name(&used)?;
                match self.bindings.get(&(self.file, name.span().start)) {
                    Some(Binding::Local {
                        at,
                        kind: LocalKind::GenericParameter,
                    }) => Some((*at, self.at(name.span()))),
                    _ => None,
                }
            })
            .collect();
        for (declared, used) in named {
            self.note_value(declared, used);
        }
    }

    // ---- The kind of a generic parameter (GEN-13) ----

    /// What the signature has said of the plain generic parameter
    /// declared at `declared`; `None` for one that is not plain, or not
    /// of the declaration being read.
    fn evidence_of(&mut self, declared: Location) -> Option<&mut Evidence> {
        let index = self
            .generics
            .iter()
            .position(|generic| generic.at == declared && generic.kind == GenericKind::Plain)?;
        self.evidence.get_mut(index)
    }

    /// Notes a use of a plain generic parameter as a type, at `used`.
    /// Only a use in the signature says what a parameter is: "Infer its
    /// kind from signature positions and constraints."
    fn note_type(&mut self, declared: Location, used: Location) {
        if self.in_signature
            && let Some(evidence) = self.evidence_of(declared)
        {
            evidence.as_type.get_or_insert(used);
        }
    }

    /// Notes a use of a plain generic parameter as a value, at `used`.
    fn note_value(&mut self, declared: Location, used: Location) {
        if self.in_signature
            && let Some(evidence) = self.evidence_of(declared)
        {
            evidence.as_value.get_or_insert(used);
        }
    }

    /// Notes a use of a plain generic parameter that may yet say what
    /// it is: as an argument of a type whose declaration is not known.
    fn note_open(&mut self, declared: Location) {
        if self.in_signature
            && let Some(evidence) = self.evidence_of(declared)
        {
            evidence.open = true;
        }
    }

    /// Notes a use of a plain generic parameter as an argument of a
    /// struct whose own parameter nothing settles.
    fn note_dependent(&mut self, declared: Location) {
        if self.in_signature
            && let Some(evidence) = self.evidence_of(declared)
        {
            evidence.dependent = true;
        }
    }

    /// Notes each generic parameter a constraint of `declaration`
    /// names. Which kind a constraint makes it is for the pass that
    /// reads constraints.
    fn constrained(&mut self, declaration: &SyntaxNode) {
        let Some(clause) = child(declaration, SyntaxKind::RequiresClause) else {
            return;
        };
        let named: Vec<Location> = clause
            .descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .filter(|token| token.kind() == SyntaxKind::Ident)
            .filter_map(
                |token| match self.bindings.get(&(self.file, token.span().start)) {
                    Some(Binding::Local {
                        at,
                        kind: LocalKind::GenericParameter,
                    }) => Some(*at),
                    _ => None,
                },
            )
            .collect();
        for declared in named {
            if let Some(evidence) = self.evidence_of(declared) {
                evidence.open = true;
            }
        }
    }

    /// GEN-13: "Infer its kind from signature positions and
    /// constraints ... Reject conflicting kinds or an unresolved kind
    /// lacking a determining declaration/constraint."
    fn settle_kinds(&mut self, declaration: &Declaration) {
        for index in 0..self.generics.len() {
            if self.generics[index].kind != GenericKind::Plain {
                continue;
            }
            let evidence = self.evidence[index].clone();
            let name = self.generics[index].name.clone();
            // A second parameter of one name has been reported as a
            // duplicate (UNIT-12), and no use can name it.
            if self.generics[..index]
                .iter()
                .any(|earlier| earlier.name == name)
            {
                continue;
            }
            match (evidence.as_type, evidence.as_value) {
                (Some(as_type), Some(as_value)) => {
                    // Reported at the later use; the other is shown.
                    let (at, other, other_is) = if as_type.span.start > as_value.span.start {
                        (as_type, as_value, "used as a value here")
                    } else {
                        (as_value, as_type, "used as a type here")
                    };
                    self.diagnostics.emit(
                        local_error(
                            Category::Type,
                            codes::local::GENERIC_KIND_CONFLICT,
                            at,
                            format!("`{name}` is used as a type and as a value"),
                            "GEN-13",
                        )
                        .with_related(other, other_is),
                    );
                }
                (Some(_), None) => self.generics[index].kind = GenericKind::Type,
                (None, Some(_)) => self.generics[index].kind = GenericKind::Value(Type::Unknown),
                // Something may yet say what it is.
                (None, None) if evidence.open => {}
                // A function given a struct whose parameter nothing
                // settles: the struct is where that is reported.
                (None, None) if evidence.dependent && !self.reading_struct => {}
                (None, None) => {
                    self.diagnostics.emit(
                        local_error(
                            Category::Type,
                            codes::local::GENERIC_KIND_UNRESOLVED,
                            self.generics[index].at,
                            format!(
                                "nothing in the signature of `{}` says whether `{name}` is a \
                                 type or a value",
                                declaration.name
                            ),
                            "GEN-13",
                        )
                        .with_note(
                            "use it as a type or as a size in the signature, or declare a \
                             value `const N: i64`",
                        ),
                    );
                }
            }
        }
    }

    // ---- Constructors ----

    /// `Box<i64>(value: 7)`: the type a constructor names.
    fn constructor(&mut self, node: &SyntaxNode) {
        let span = match (used_start(node), child(node, SyntaxKind::GenericArgs)) {
            (Some(start), Some(list)) => Span::new(start, list.span().end),
            _ => return,
        };
        if self.read.contains_key(&span.start) {
            return;
        }
        let ty = self.named(node, Position::PLAIN);
        self.record(span, ty);
    }

    /// `delta<list<f64>>(added: values)`: the shape a delta is of.
    fn delta_constructor(&mut self, node: &SyntaxNode) {
        let Some(list) = child(node, SyntaxKind::GenericArgs) else {
            return;
        };
        let span = Span::new(node.span().start, list.span().end);
        if self.read.contains_key(&span.start) {
            return;
        }
        let shape = self.inner(&list, Position::PLAIN);
        self.record(span, Type::Delta(Box::new(shape)));
    }
}

/// Where the name of a constructor starts: its alias, when it has one.
fn used_start(node: &SyntaxNode) -> Option<u32> {
    match child(node, SyntaxKind::QualifiedName) {
        Some(qualified) => Some(qualified.span().start),
        None => used_name(node).map(|name| name.span().start),
    }
}

/// The two kinds a window's size may be (TYPE-11), and not known yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SizeKind {
    /// A count of ticks.
    Ticks,
    /// A duration.
    Time,
    /// A generic parameter or an expression this pass does not
    /// evaluate.
    Unknown,
}

impl SizeKind {
    fn describe(self) -> &'static str {
        match self {
            SizeKind::Ticks => "a tick count",
            SizeKind::Time => "a duration",
            SizeKind::Unknown => "not known",
        }
    }
}
