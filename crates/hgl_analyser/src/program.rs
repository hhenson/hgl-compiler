//! The analysed program: what the analyser gives the stages after it.
//!
//! These are plain records. The analyser fills them in; nothing here
//! checks a rule.

use std::collections::HashMap;

use hgl_diagnostics::Location;
use hgl_parser::ast::{AstNode, SourceFile};
use hgl_parser::{HasSpan, SyntaxNode};
use hgl_source::FileId;

use crate::declared::{names_one_of, substitute_within};
use crate::hir::{Body, Expr, Run};
use crate::types::{Argument, CallArguments, Detail, GenericKind, NamedKind, Type, WrittenType};

/// Every module found in the files analysed, in order of module path.
#[derive(Debug, Clone, Default)]
pub struct Program {
    /// The modules. A module's position here is how other records refer
    /// to it ([`Import::module`], [`OperatorRef::module`]).
    pub modules: Vec<Module>,
}

impl Program {
    /// The path of core's mandatory module (UNIT-11).
    pub const CORE: &'static str = "core";

    /// The position of core's mandatory module in [`Program::modules`],
    /// when the program has it. Its public declarations are the prelude
    /// (UNIT-11).
    pub fn core(&self) -> Option<usize> {
        self.modules
            .iter()
            .position(|module| module.path == Program::CORE)
    }

    /// The type of the capability `inject` names, when core declares
    /// it. TIME-8: "`inject` binds each of the others to a local of its
    /// capability's type, which core declares and names as the
    /// capability with a capital". Core's CAP-1: "Each capability but
    /// `out` has a native type of core's module".
    pub fn capability_type(&self, capability: &str) -> Option<Type> {
        let (_, name) = CAPABILITY_TYPES
            .iter()
            .find(|(injected, _)| *injected == capability)?;
        let core = self.core()?;
        let module = &self.modules[core];
        let declaration = module.scope.get(*name)?.iter().copied().find(|&index| {
            let declaration = &module.declarations[index];
            declaration.kind == DeclarationKind::NativeType && declaration.is_public()
        })?;
        Some(Type::Named {
            name: (*name).to_string(),
            module: core,
            declaration,
            arguments: Vec::new(),
            kind: NamedKind::Native,
        })
    }

    /// The type of a run's configuration: core's struct `RunConfig`
    /// (RUN-1, core's CRUN-2). `None` where core is not supplied or
    /// declares none.
    pub fn run_config_type(&self) -> Option<Type> {
        const NAME: &str = "RunConfig";
        let core = self.core()?;
        let module = &self.modules[core];
        let declaration = module.scope.get(NAME)?.iter().copied().find(|&index| {
            let declaration = &module.declarations[index];
            declaration.kind == DeclarationKind::Struct && declaration.is_public()
        })?;
        Some(Type::Named {
            name: NAME.to_string(),
            module: core,
            declaration,
            arguments: Vec::new(),
            kind: NamedKind::Struct,
        })
    }

    /// True for the type of a capability: one of core's native types
    /// that TIME-8 names.
    pub fn is_capability_type(&self, ty: &Type) -> bool {
        let Type::Named {
            module,
            declaration,
            ..
        } = ty
        else {
            return false;
        };
        if Some(*module) != self.core() {
            return false;
        }
        let declaration = &self.modules[*module].declarations[*declaration];
        declaration.kind == DeclarationKind::NativeType
            && CAPABILITY_TYPES
                .iter()
                .any(|(_, name)| *name == declaration.name)
    }

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

    /// The type of each field the struct `ty` has, in the struct's
    /// order, with the generic arguments `ty` gives in place of the
    /// struct's parameters. TYPE-4: "So has a struct's own field, with
    /// the arguments the struct is given: the `value: T` of a
    /// `Box<i64?>` is optional." `None` where `ty` is no struct. A field
    /// whose type still names a parameter of the struct, one that is
    /// given no argument, is not known, [`Type::Unknown`].
    pub fn field_types(&self, ty: &Type) -> Option<Vec<Type>> {
        let Type::Named {
            module,
            declaration,
            arguments,
            kind: NamedKind::Struct,
            ..
        } = ty
        else {
            return None;
        };
        let Some(Detail::Struct(read)) = self.modules.get(*module)?.details.get(*declaration)
        else {
            return None;
        };
        // An argument is given to each parameter before a type pack,
        // place for place (GEN-2); a pack takes a sequence of them, which
        // is not put in place here.
        let map: Vec<(Location, &Argument)> = read
            .generics
            .iter()
            .take_while(|generic| !matches!(generic.kind, GenericKind::Pack { .. }))
            .map(|generic| generic.at)
            .zip(arguments)
            .collect();
        let own: Vec<Location> = read.generics.iter().map(|generic| generic.at).collect();
        Some(
            read.members
                .iter()
                .map(|member| {
                    let ty = substitute_within(&member.ty, &map);
                    if names_one_of(&ty, &own) {
                        Type::Unknown
                    } else {
                        ty
                    }
                })
                .collect(),
        )
    }
}

/// One module: the files that name it, assembled (UNIT-1).
#[derive(Debug, Clone)]
pub struct Module {
    /// The module's path, written with dots.
    pub path: String,
    /// True for an application (UNIT-14): "one file, headed
    /// `application name`, where a module's files are headed `module
    /// name`. It holds what a module holds, and one run statement".
    pub application: bool,
    /// Its files: the unnamed one first, then the parts in order of part
    /// name. That is the order the diagnostic catalogue gives for "the
    /// later of two", so what is reported does not depend on the order
    /// the files were given in; it changes no meaning (UNIT-1).
    pub files: Vec<ModuleFile>,
    /// Every declaration of the module, test helpers included, in file
    /// order and then source order.
    pub declarations: Vec<Declaration>,
    /// The named tests, in file order and then source order.
    pub tests: Vec<Test>,
    /// Names the module uses that nothing binds: no local, parameter,
    /// declaration, import, or declaration of the prelude (UNIT-10,
    /// UNIT-11), each once, where it is first used. In a program that
    /// has core's module each is reported, `name.unknown`, but for the
    /// two forms of the language no pass reads yet, `types` and
    /// `schemas` (GEN-9, GEN-10). In one that has not, no prelude is
    /// known, and they are only listed here.
    pub prelude_names: Vec<PreludeName>,
    /// Every name used in the module's signatures, bodies, types and
    /// constraints that resolved, with what it resolved to (UNIT-10), in
    /// file order and then source order.
    pub uses: Vec<NameUse>,
    /// What each `name<...>(...)` the parser left undecided was decided
    /// to be (GEN-14).
    pub decisions: Vec<Decision>,
    /// What the types of each declaration are: its signature, or its
    /// fields. One for each of `declarations`, in the same order.
    pub details: Vec<Detail>,
    /// Every type written in the module, with what it means, in file
    /// order and then source order. A type inside another is part of
    /// that one and not listed again.
    pub written: Vec<WrittenType>,
    /// The generic arguments each call writes after its callee's name,
    /// `count<i64>(3)` (GEN-8), in the order the calls are read.
    pub call_arguments: Vec<CallArguments>,
    /// The typed body of each value function: one entry for each of
    /// `declarations`, in the same order, `None` for a declaration that
    /// is no value function or has no body.
    pub bodies: Vec<Option<Body>>,
    /// The typed block of each named test: one entry for each of
    /// `tests`, in the same order, `None` for a test with no block.
    pub test_bodies: Vec<Option<Body>>,
    /// The value each constant is written with, read as a value of the
    /// constant's type (UNIT-13): one entry for each of `declarations`,
    /// in the same order, `None` for a declaration that is no constant.
    pub constants: Vec<Option<Expr>>,
    /// An application's run statement, as it is written (RUN-1): its
    /// first, where it writes more than one. `None` for a module, and
    /// for an application that writes none.
    pub run: Option<RunStatement>,
    /// That statement, typed. `None` where it could not be read.
    pub run_body: Option<Run>,
    /// What an application sets its run's store with, as it is
    /// written, in the order written (UNIT-14): "`inject global_state`,
    /// and then calls of `put` on it", between its declarations and
    /// its run statement. Empty for a module, and for an application
    /// that writes none. One that stands where none may is reported
    /// and is not here.
    pub store: Vec<StoreStatement>,
    /// Those statements, typed: `inject global_state` as the one local
    /// it binds, and each call as a statement. `None` where there are
    /// none.
    pub store_body: Option<Body>,
    /// The defaults each declaration writes: one entry for each of
    /// `declarations`, in the same order. A callable's has one for each
    /// of its parameters (UNIT-8), and a struct's one for each of its
    /// own `fields` (TYPE-4), `None` for one with no default. It is
    /// empty for any other declaration. A default names no parameter
    /// and no field (UNIT-7), so it is read with no local in scope.
    pub defaults: Vec<Vec<Option<Expr>>>,
    /// What the body of each temporal function describes (FN-2): one
    /// entry for each of `declarations`, in the same order, `None` for
    /// a declaration that is no temporal function or has no body
    /// written in HGL.
    pub phases: Vec<Option<Phase>>,
    /// The module's declaration scope: for each name, the positions in
    /// `declarations` of what it is bound to. One, or the two of a
    /// temporal and value pair (FN-4).
    pub(crate) scope: HashMap<String, Vec<usize>>,
    /// The module's `instantiate` items, each with the position of its
    /// file in `files`. They declare nothing, and name what they request.
    pub(crate) instantiations: Vec<(usize, SyntaxNode)>,
    /// The calls in each declaration's body that a membership its
    /// `requires` states authorises, whose candidate each instance of
    /// it selects for itself (GEN-3, GEN-7, GEN-5): one entry
    /// for each of `declarations`, in the same order. The body has each
    /// as a call with no callee ([`crate::hir::Callee::Unread`]);
    /// [`Program::instance`] gives the instance's.
    pub(crate) selecting: Vec<Vec<crate::values::Selecting>>,
}

impl Module {
    /// True where the body of `declaration` has a call whose candidate
    /// is selected for each instance of the declaration: its candidate
    /// is known only in an instance ([`Program::instance`]), so the
    /// body is written for each.
    pub fn selects_per_instance(&self, declaration: usize) -> bool {
        self.selecting
            .get(declaration)
            .is_some_and(|calls| !calls.is_empty())
    }

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

    /// Where the path of an application is written, in the line that
    /// heads its file (UNIT-14). `None` for a module.
    pub fn application_at(&self) -> Option<Location> {
        if !self.application {
            return None;
        }
        let file = self.files.first()?;
        let path = file.syntax.application_decl()?.path()?;
        Some(Location::new(file.file, path.syntax().span()))
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

/// What the body of a temporal function describes (FN-2). It follows
/// from the body alone: "Generic selection, target choice, and optimizer
/// decisions cannot change the role."
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Phase {
    /// Composition: the function is a graph. Its body describes typed
    /// connections and reads no payload.
    Graph,
    /// Runtime: the function is a node. Its body holds state and hooks
    /// that run when the node is evaluated.
    Node,
}

impl Phase {
    /// `graph` or `node`.
    pub fn as_str(self) -> &'static str {
        match self {
            Phase::Graph => "graph",
            Phase::Node => "node",
        }
    }
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
    /// `impl fn` or `impl const fn`: a temporal or a value candidate of
    /// the operator it names (GEN-4). It binds no name of its own
    /// (UNIT-12). The operator is filled in once it is found.
    Implementation(Role, Option<OperatorRef>),
    /// `native impl fn` or `native impl const fn` with no body: a
    /// candidate whose body a provider supplies (NATIVE-1).
    NativeCandidate(Role, Option<OperatorRef>),
    /// The same with a contract body: an implementation part. It
    /// completes a native candidate of its name and role (NATIVE-2).
    NativeCandidateImplementation(Role),
    /// `struct`.
    Struct,
    /// `enum`.
    Enum,
    /// `native type`.
    NativeType,
    /// `const name: T = expression`: a constant (UNIT-13).
    Constant,
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
    /// An operator's candidate and a native implementation attach to
    /// another declaration instead.
    pub fn binds_name(self) -> bool {
        !matches!(
            self,
            DeclarationKind::Implementation(..)
                | DeclarationKind::NativeImplementation(_)
                | DeclarationKind::NativeCandidate(..)
                | DeclarationKind::NativeCandidateImplementation(_)
        )
    }

    /// The role of a function, whatever its form: `None` for an
    /// operator, which has none (FN-1), and for what is no function.
    pub fn role(self) -> Option<Role> {
        match self {
            DeclarationKind::Function(role)
            | DeclarationKind::NativeFunction(role)
            | DeclarationKind::NativeImplementation(role)
            | DeclarationKind::Implementation(role, _)
            | DeclarationKind::NativeCandidate(role, _)
            | DeclarationKind::NativeCandidateImplementation(role) => Some(role),
            _ => None,
        }
    }

    /// For a candidate of an operator, HGL or native: its role and the
    /// operator it was found to name (GEN-4).
    pub fn candidate(self) -> Option<(Role, Option<OperatorRef>)> {
        match self {
            DeclarationKind::Implementation(role, operator)
            | DeclarationKind::NativeCandidate(role, operator) => Some((role, operator)),
            _ => None,
        }
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
            DeclarationKind::Implementation(role, _) => {
                format!("{} operator implementation", role.as_str())
            }
            DeclarationKind::NativeCandidate(role, _) => {
                format!("native {} operator implementation", role.as_str())
            }
            DeclarationKind::NativeCandidateImplementation(role) => {
                format!("native {} operator implementation part", role.as_str())
            }
            DeclarationKind::Struct => "struct".to_string(),
            DeclarationKind::Enum => "enum".to_string(),
            DeclarationKind::NativeType => "native type".to_string(),
            DeclarationKind::Constant => "constant".to_string(),
        }
    }
}

/// Each capability that has a type, with the type's name (TIME-8):
/// "`Logger`, `Clock`, `Scheduler`, `Alarm`, `GlobalState`". `out` is
/// the node's result, and has none.
pub const CAPABILITY_TYPES: [(&str, &str); 5] = [
    ("logger", "Logger"),
    ("clock", "Clock"),
    ("scheduler", "Scheduler"),
    ("alarm", "Alarm"),
    ("global_state", "GlobalState"),
];

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

    /// A temporal function named `name` that stands in for what is no
    /// declaration and is read as a graph's body is: a test (TST-3), a
    /// run statement (RUN-1), or an application's store statements
    /// (UNIT-14). `file` is the position of its file in
    /// [`Module::files`]; `test_only` is true for a test.
    pub(crate) fn stand_in(
        name: &str,
        at: Location,
        file: usize,
        syntax: &SyntaxNode,
        test_only: bool,
    ) -> Declaration {
        Declaration {
            name: name.to_string(),
            at,
            kind: DeclarationKind::Function(Role::Temporal),
            exported: false,
            test_only,
            file,
            syntax: syntax.clone(),
        }
    }
}

/// One named test.
#[derive(Debug, Clone)]
pub struct Test {
    /// Its name.
    pub name: String,
    /// True for an integration test (TST-17): one that may run in real
    /// time, and "is run only when it is asked for".
    pub integration: bool,
    /// The time limit `@timeout(5s)` gives an integration test, in
    /// microseconds (TST-17: "the duration `@timeout(5s)` gives on the
    /// line before it, or one minute where none is written"). `None`
    /// where none is written, and where what is written is no valid
    /// duration, which the lexer has reported.
    pub timeout: Option<i64>,
    /// Where the name is written.
    pub at: Location,
    /// The position of its file in [`Module::files`].
    pub file: usize,
    /// The test's syntax tree.
    pub syntax: SyntaxNode,
}

/// A statement an application sets its run's store with, as it is
/// written (UNIT-14).
#[derive(Debug, Clone)]
pub struct StoreStatement {
    /// Where the statement starts.
    pub at: Location,
    /// The position of its file in [`Module::files`].
    pub file: usize,
    /// The statement's syntax tree: a `StoreStmt`.
    pub syntax: SyntaxNode,
}

/// An application's run statement, as it is written.
#[derive(Debug, Clone)]
pub struct RunStatement {
    /// Where its word, `run` or `eval`, is written.
    pub at: Location,
    /// The position of its file in [`Module::files`].
    pub file: usize,
    /// The statement's syntax tree: a `RunStmt`.
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
/// guess when `name` is a generic type, function or operator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    /// Where the node starts.
    pub at: Location,
    /// `Some(true)`: generic arguments and a call's arguments, of a
    /// generic type's constructor or of a generic function or operator
    /// (GEN-8). `Some(false)`: comparisons. `None`: `name` is left for
    /// the prelude, so it is not known which.
    pub constructor: Option<bool>,
}

/// A name nothing binds. See [`Module::prelude_names`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreludeName {
    /// The name.
    pub name: String,
    /// Where it is used.
    pub at: Location,
}
