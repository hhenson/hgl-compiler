//! The analysed program: what the analyser gives the stages after it.
//!
//! These are plain records. The analyser fills them in; nothing here
//! checks a rule.

use std::collections::HashMap;

use hgl_diagnostics::Location;
use hgl_parser::SyntaxNode;
use hgl_parser::ast::SourceFile;
use hgl_source::FileId;

/// Every module found in the files analysed, in order of module path.
#[derive(Debug, Clone, Default)]
pub struct Program {
    /// The modules. A module's position here is how other records refer
    /// to it ([`Import::module`], [`OperatorRef::module`]).
    pub modules: Vec<Module>,
}

impl Program {
    /// The module with this path, written with dots: `pricing.curves`.
    pub fn module(&self, path: &str) -> Option<&Module> {
        self.module_index(path).map(|index| &self.modules[index])
    }

    /// The position of the module with this path in [`Program::modules`].
    pub fn module_index(&self, path: &str) -> Option<usize> {
        // The modules are in path order, so a binary search finds one.
        // `.ok()` turns "found at index" into `Some(index)` and "not
        // found" into `None`.
        self.modules
            .binary_search_by(|module| module.path.as_str().cmp(path))
            .ok()
    }
}

/// One module: the files that name it, assembled (UNIT-1).
#[derive(Debug, Clone)]
pub struct Module {
    /// The module's path, written with dots.
    pub path: String,
    /// Its files: the unnamed one first, then the parts in order of part
    /// name. The order is the analyser's own, so that what it reports
    /// does not depend on the order the files were given in; it changes
    /// no meaning (UNIT-1).
    pub files: Vec<ModuleFile>,
    /// Every declaration of the module, test helpers included, in file
    /// order and then source order.
    pub declarations: Vec<Declaration>,
    /// The named tests, in file order and then source order.
    pub tests: Vec<Test>,
    /// Names the module uses that nothing it declares or imports binds.
    /// The prelude binds such a name, or nothing does (UNIT-11). Core's
    /// mandatory module has no declarations yet, so the analyser cannot
    /// say which, and reports neither.
    pub prelude_names: Vec<PreludeName>,
    /// Every name used in the module's signatures, bodies, types and
    /// constraints that resolved, with what it resolved to (UNIT-10), in
    /// file order and then source order.
    pub uses: Vec<NameUse>,
    /// What each `name<...>(...)` the parser left undecided was decided
    /// to be (GEN-14).
    pub decisions: Vec<Decision>,
    /// The module's declaration scope: for each name, the positions in
    /// `declarations` of what it is bound to. One, or the two of a
    /// temporal and value pair (FN-4).
    pub(crate) scope: HashMap<String, Vec<usize>>,
    /// The module's `instantiate` items, each with the position of its
    /// file in `files`. They declare nothing, and name what they request.
    pub(crate) instantiations: Vec<(usize, SyntaxNode)>,
}

impl Module {
    /// The declarations `name` is bound to in the module's scope: none,
    /// one, or the two functions of a temporal and value pair (FN-4).
    pub fn lookup(&self, name: &str) -> impl Iterator<Item = &Declaration> {
        // `into_iter().flatten()` walks the list when the name is bound
        // and nothing when it is not.
        self.scope
            .get(name)
            .into_iter()
            .flatten()
            .map(|&index| &self.declarations[index])
    }
}

/// One file of a module.
#[derive(Debug, Clone)]
pub struct ModuleFile {
    /// The file.
    pub file: FileId,
    /// Its part name; `None` for the module's unnamed file.
    pub part: Option<String>,
    /// The names its `use` lines bind. They bind in this file only
    /// (UNIT-2).
    pub imports: Vec<Import>,
    /// The file's syntax tree.
    pub syntax: SourceFile,
}

/// One name bound by a `use` line.
#[derive(Debug, Clone)]
pub struct Import {
    /// The name bound: an imported declaration's, or an alias.
    pub name: String,
    /// Where the name is written.
    pub at: Location,
    /// What the name is.
    pub kind: ImportKind,
    /// The path of the module imported from, written with dots.
    pub from: String,
    /// Where that path is written.
    pub from_at: Location,
    /// The position of that module in [`Program::modules`]; `None` when
    /// no file names it.
    pub module: Option<usize>,
}

/// The two forms of `use` (UNIT-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportKind {
    /// `use m::{name}`: the name of a public declaration of `m`.
    Selective,
    /// `use m as name`: a qualifier for `name::declaration`.
    Alias,
}

/// The two roles of a function (FN-1). A temporal and a value function
/// may share a name (FN-4); two of one role may not (UNIT-5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    /// `fn`: temporal inputs and a temporal result.
    Temporal,
    /// `const fn`: ordinary values.
    Value,
}

impl Role {
    /// `temporal` or `value`.
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Temporal => "temporal",
            Role::Value => "value",
        }
    }
}

/// What a declaration is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclarationKind {
    /// `fn` or `const fn`.
    Function(Role),
    /// `native fn` or `native const fn` with no body: the shared
    /// declaration (NATIVE-3).
    NativeFunction(Role),
    /// `native fn` or `native const fn` with a contract body: an
    /// implementation part. It completes the declaration of its name and
    /// role (NATIVE-2) and binds no name of its own.
    NativeImplementation(Role),
    /// `operator`.
    Operator,
    /// `impl fn`: a candidate of the operator it names (GEN-4). It binds
    /// no name of its own (UNIT-12).
    Implementation(Option<OperatorRef>),
    /// `struct`.
    Struct,
    /// `enum`.
    Enum,
    /// `native type`.
    NativeType,
}

impl DeclarationKind {
    /// The declaration's role, when it is a function that binds its
    /// name.
    pub(crate) fn binding_role(self) -> Option<Role> {
        match self {
            DeclarationKind::Function(role) | DeclarationKind::NativeFunction(role) => Some(role),
            _ => None,
        }
    }

    /// True when the declaration binds its name in the module's scope.
    /// An operator implementation and a native implementation attach to
    /// another declaration instead.
    pub fn binds_name(self) -> bool {
        !matches!(
            self,
            DeclarationKind::Implementation(_) | DeclarationKind::NativeImplementation(_)
        )
    }

    /// A few words for messages and listings: `temporal function`,
    /// `struct`.
    pub fn describe(self) -> String {
        match self {
            DeclarationKind::Function(role) => format!("{} function", role.as_str()),
            DeclarationKind::NativeFunction(role) => {
                format!("native {} function", role.as_str())
            }
            DeclarationKind::NativeImplementation(role) => {
                format!("native {} implementation", role.as_str())
            }
            DeclarationKind::Operator => "operator".to_string(),
            DeclarationKind::Implementation(_) => "operator implementation".to_string(),
            DeclarationKind::Struct => "struct".to_string(),
            DeclarationKind::Enum => "enum".to_string(),
            DeclarationKind::NativeType => "native type".to_string(),
        }
    }
}

/// Where an operator is declared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OperatorRef {
    /// The position of its module in [`Program::modules`].
    pub module: usize,
    /// The position of the operator in that module's
    /// [`Module::declarations`].
    pub declaration: usize,
}

/// One declaration of a module.
#[derive(Debug, Clone)]
pub struct Declaration {
    /// Its name. For an `impl fn`, the name of the operator it
    /// implements.
    pub name: String,
    /// Where the name is written.
    pub at: Location,
    /// What it is.
    pub kind: DeclarationKind,
    /// True when it is marked `export`. An operator is public without
    /// the mark (UNIT-3); see [`Declaration::is_public`].
    pub exported: bool,
    /// True when it is declared in an unnamed test context: a helper or
    /// type of the module's test scope (TST-1).
    pub test_only: bool,
    /// The position of its file in [`Module::files`].
    pub file: usize,
    /// The declaration's syntax tree.
    pub syntax: SyntaxNode,
}

impl Declaration {
    /// True when another module may import the declaration: it is
    /// exported, or it is an operator (UNIT-3).
    pub fn is_public(&self) -> bool {
        self.exported || self.kind == DeclarationKind::Operator
    }
}

/// One named test.
#[derive(Debug, Clone)]
pub struct Test {
    /// Its name.
    pub name: String,
    /// Where the name is written.
    pub at: Location,
    /// The position of its file in [`Module::files`].
    pub file: usize,
    /// The test's syntax tree.
    pub syntax: SyntaxNode,
}

/// One use of a name, and what it is bound to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameUse {
    /// The name.
    pub name: String,
    /// Where it is used.
    pub at: Location,
    /// What it is bound to.
    pub binding: Binding,
}

/// What a name is bound to (UNIT-10).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Binding {
    /// A binding of the declaration the use stands in: a local, a
    /// parameter, a generic parameter.
    Local {
        /// Where it is declared.
        at: Location,
        /// What declares it.
        kind: LocalKind,
    },
    /// Declarations of a module: the use's own, or one it imports from.
    /// One declaration, or the two of a temporal and value pair (FN-4).
    Declarations {
        /// The position of the module in [`Program::modules`].
        module: usize,
        /// The positions of the declarations in its
        /// [`Module::declarations`].
        declarations: Vec<usize>,
    },
}

/// What declares a binding inside a declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalKind {
    /// `let`.
    Let,
    /// `var`.
    Var,
    /// `state`.
    State,
    /// `cache`.
    Cache,
    /// `inject`.
    Inject,
    /// The names of a `for` pattern.
    Iteration,
    /// A parameter of a function, or of an anonymous function.
    Parameter,
    /// A generic parameter, or the name an `each ... in` constraint
    /// binds.
    GenericParameter,
}

impl LocalKind {
    /// A few words for messages: `a parameter`, `a \`var\``.
    pub fn describe(self) -> &'static str {
        match self {
            LocalKind::Let => "a `let`",
            LocalKind::Var => "a `var`",
            LocalKind::State => "a `state`",
            LocalKind::Cache => "a `cache`",
            LocalKind::Inject => "an injected capability",
            LocalKind::Iteration => "an iteration binding",
            LocalKind::Parameter => "a parameter",
            LocalKind::GenericParameter => "a generic parameter",
        }
    }
}

/// What a `name<...>(...)` that reads two ways was decided to be
/// (GEN-14). The parser guesses a constructor; the analyser confirms the
/// guess when `name` is a generic type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    /// Where the node starts.
    pub at: Location,
    /// `Some(true)`: a constructor of a generic type. `Some(false)`:
    /// comparisons. `None`: `name` is left for the prelude, so it is not
    /// known which.
    pub constructor: Option<bool>,
}

/// A name left for the prelude to bind. See [`Module::prelude_names`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreludeName {
    /// The name.
    pub name: String,
    /// Where it is used.
    pub at: Location,
}
