//! Typed views of the syntax tree.
//!
//! A [`SyntaxNode`] knows its kind, its text and its children, and nothing
//! of what they mean: to find a function's name one would walk its tokens
//! looking for the identifier after `fn`. The types here do that walking.
//! Each wraps a node of one kind and has a method for each part of it a
//! later stage asks for:
//!
//! ```text
//! SyntaxNode (kind FnDecl)  --FnDecl::cast-->  FnDecl
//!                                              .name()        the token `scale`
//!                                              .is_exported() whether `export` is there
//! ```
//!
//! A view is the node under another type. It copies nothing: making one
//! costs a reference count, and [`AstNode::syntax`] gives the node back.
//!
//! The parser builds a tree for any tokens, so the tree of a broken file
//! may lack any part. Every method that reads a part returns an `Option`
//! (the part, or `None` when it is not there) or an iterator that may be
//! empty; none panics.
//!
//! Only the parts the analyser reads so far have a method. The rest are
//! added as the stages that read them are built.

use crate::kind::SyntaxKind;
use crate::tree::{SyntaxNode, SyntaxToken};

/// A typed view of one kind of node.
///
/// A trait is Rust's interface: every view type implements these two
/// functions, so code can be written once for all of them (see `child`
/// below, which finds the first child of whichever view type is asked
/// for).
pub trait AstNode: Sized {
    /// The view of `node`, or `None` when the node is of another kind.
    fn cast(node: SyntaxNode) -> Option<Self>;

    /// The node this is a view of.
    fn syntax(&self) -> &SyntaxNode;
}

/// Declares a view type for each node kind listed. The type has the
/// kind's name.
///
/// `$( ... ),*` repeats for every comma-separated entry; `$(#[$doc:meta])*`
/// carries each entry's doc comment over to the type.
macro_rules! ast_nodes {
    ( $( $(#[$doc:meta])* $name:ident ),* $(,)? ) => {
        $(
            $(#[$doc])*
            #[derive(Debug, Clone, PartialEq, Eq, Hash)]
            pub struct $name {
                syntax: SyntaxNode,
            }

            impl AstNode for $name {
                fn cast(node: SyntaxNode) -> Option<$name> {
                    // `then_some` turns `true` into `Some(value)` and
                    // `false` into `None`.
                    (node.kind() == SyntaxKind::$name).then_some($name { syntax: node })
                }

                fn syntax(&self) -> &SyntaxNode {
                    &self.syntax
                }
            }
        )*
    };
}

ast_nodes! {
    /// A whole source file.
    SourceFile,
    /// `module pricing.curves part bootstrap`
    ModuleDecl,
    /// `application pricing.live`
    ApplicationDecl,
    /// `run heartbeat()` or `eval heartbeat()` as an item of a file
    RunStmt,
    /// `inject run_config{length: 3s}`, `inject global_state` or
    /// `put(global_state, "limit", 3)` as an item of a file
    StoreStmt,
    /// `pricing.curves`
    ModulePath,
    /// `use pricing.curves::{curve, bump}` or `use pricing.curves as pc`
    UseDecl,
    /// `{curve, bump}` in a `use` declaration
    ImportSet,
    /// `export const fn scale(value: f64) -> f64 => value * 2.0`
    FnDecl,
    /// `export abstract struct Quote<T> : Base { bid: T }`
    StructDecl,
    /// `export enum Side { buy, sell }`
    EnumDecl,
    /// `operator add<T>(left: T, right: T) -> T`
    OperatorDecl,
    /// `instantiate add<i64>, add<f64>`
    InstantiateDecl,
    /// `export native type Handle`
    NativeTypeDecl,
    /// `export const scale: i64 = 2`
    ConstantDecl,
    /// `native const fn describe(value: i64) -> str`, with any contract
    /// body
    NativeFnDecl,
    /// `{ inject out; start; when; }` after a native function
    NativeContractBody,
    /// `test name { ... }`
    TestDecl,
    /// `test { ... }`: an unnamed context holding helpers and tests
    TestContext,
}

// ---- Reading a node's parts ----

/// The first child node of `parent` that is an `N`.
///
/// `N` is chosen by the caller: `child::<ModulePath>(node)`, or simply
/// `child(node)` where the return type says which.
fn child<N: AstNode>(parent: &SyntaxNode) -> Option<N> {
    parent.children().find_map(N::cast)
}

/// The tokens directly inside `parent`, without whitespace and comments,
/// in source order. Tokens of its child nodes are not included.
fn tokens(parent: &SyntaxNode) -> impl Iterator<Item = SyntaxToken> {
    parent
        .children_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
}

/// True when a token of `kind` is directly inside `parent`.
fn has_token(parent: &SyntaxNode, kind: SyntaxKind) -> bool {
    tokens(parent).any(|token| token.kind() == kind)
}

/// The identifier directly after the first token of kind `after` inside
/// `parent`: the name in `fn name`, `struct name`, `as name`. `None` when
/// either is missing or something else stands between them.
fn name_after(parent: &SyntaxNode, after: SyntaxKind) -> Option<SyntaxToken> {
    let mut seen = false;
    for element in parent.children_with_tokens() {
        if element.kind().is_trivia() {
            continue;
        }
        if seen {
            // The element after the keyword: the name, or not a name.
            return element
                .into_token()
                .filter(|token| token.kind() == SyntaxKind::Ident);
        }
        seen = element.kind() == after;
    }
    None
}

/// True when the identifier `word` is directly inside `parent` before its
/// first token of kind `before`: the `const` of `const fn`. `const` and
/// `native` are not reserved (LEX-9, LEX-10), so the tree holds them as
/// identifiers and their place says what they are.
fn has_word_before(parent: &SyntaxNode, word: &str, before: SyntaxKind) -> bool {
    tokens(parent)
        .take_while(|token| token.kind() != before)
        .any(|token| token.kind() == SyntaxKind::Ident && token.text() == word)
}

// ---- Files ----

impl SourceFile {
    /// The `module` line. A file has one, first (UNIT-1); `None` for a
    /// file that does not start with it.
    pub fn module_decl(&self) -> Option<ModuleDecl> {
        child(&self.syntax)
    }

    /// The `application` line, for the file of an application
    /// (UNIT-14); `None` for a module's file.
    pub fn application_decl(&self) -> Option<ApplicationDecl> {
        child(&self.syntax)
    }

    /// The imports and declarations of the file, in source order.
    pub fn items(&self) -> impl Iterator<Item = Item> {
        self.syntax.children().filter_map(Item::cast)
    }
}

impl ApplicationDecl {
    /// The application's path: `pricing.live`.
    pub fn path(&self) -> Option<ModulePath> {
        child(&self.syntax)
    }
}

impl StoreStmt {
    /// The statement inside: an `InjectDecl`, or an `ExprStmt` that
    /// holds a call.
    pub fn statement(&self) -> Option<SyntaxNode> {
        self.syntax.children().next()
    }
}

impl RunStmt {
    /// The `run call` or `eval call` the statement is: a `RunExpr` or
    /// an `EvalExpr` node.
    pub fn form(&self) -> Option<SyntaxNode> {
        self.syntax.children().next()
    }

    /// The word of the statement, `run` or `eval`: its mode (RUN-3).
    pub fn word(&self) -> Option<SyntaxToken> {
        tokens(&self.form()?).next()
    }

    /// True for `run`, a run in real time; false for `eval`, a
    /// simulation (RUN-3).
    pub fn real_time(&self) -> bool {
        self.form()
            .is_some_and(|form| form.kind() == SyntaxKind::RunExpr)
    }
}

impl ModuleDecl {
    /// The module's path: `pricing.curves`.
    pub fn path(&self) -> Option<ModulePath> {
        child(&self.syntax)
    }

    /// The part's name: `bootstrap` in `module pricing.curves part
    /// bootstrap`. `None` for the module's unnamed file.
    pub fn part(&self) -> Option<SyntaxToken> {
        name_after(&self.syntax, SyntaxKind::KwPart)
    }
}

impl ModulePath {
    /// The names of the path, in order: `pricing`, then `curves`.
    pub fn segments(&self) -> impl Iterator<Item = SyntaxToken> {
        tokens(&self.syntax).filter(|token| token.kind() == SyntaxKind::Ident)
    }

    /// The path as it is written when nothing stands between its tokens:
    /// `pricing.curves`. Two paths name the same module when these are
    /// equal.
    pub fn dotted(&self) -> String {
        // `collect` gathers the names into a list; `join` puts a dot
        // between each two.
        let names: Vec<String> = self
            .segments()
            .map(|token| token.text().to_string())
            .collect();
        names.join(".")
    }
}

impl UseDecl {
    /// The module imported from.
    pub fn path(&self) -> Option<ModulePath> {
        child(&self.syntax)
    }

    /// The names of a selective import: `{curve, bump}`.
    pub fn import_set(&self) -> Option<ImportSet> {
        child(&self.syntax)
    }

    /// The alias of `use pricing.curves as pc`: the token `pc`.
    pub fn alias(&self) -> Option<SyntaxToken> {
        name_after(&self.syntax, SyntaxKind::KwAs)
    }
}

impl ImportSet {
    /// The imported names, in order.
    pub fn names(&self) -> impl Iterator<Item = SyntaxToken> {
        tokens(&self.syntax).filter(|token| token.kind() == SyntaxKind::Ident)
    }
}

// ---- Declarations ----

/// An import or a declaration: what a file, and an unnamed test context,
/// holds.
///
/// An `enum` with a value in each variant is Rust's tagged union: an
/// `Item` is exactly one of these, and `match` asks which.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Item {
    /// `use ...`
    Use(UseDecl),
    /// `fn`, `const fn` or `impl fn`
    Function(FnDecl),
    /// `struct`
    Struct(StructDecl),
    /// `enum`
    Enum(EnumDecl),
    /// `operator`
    Operator(OperatorDecl),
    /// `instantiate`
    Instantiate(InstantiateDecl),
    /// `native type`
    NativeType(NativeTypeDecl),
    /// `const name: T = expression`
    Constant(ConstantDecl),
    /// `native fn` or `native const fn`
    NativeFunction(NativeFnDecl),
    /// `test name { ... }`
    Test(TestDecl),
    /// `test { ... }`
    TestContext(TestContext),
    /// `run call` or `eval call`: an application's run statement
    Run(RunStmt),
    /// `inject run_config{length: 3s}`, `inject global_state` or
    /// `put(global_state, "limit", 3)`: what an application gives its
    /// run's configuration as defaults and sets its run's store with
    Store(StoreStmt),
}

impl Item {
    /// The item `node` is, or `None` when it is something else: a second
    /// `module` line, or tokens the parser could place nowhere.
    pub fn cast(node: SyntaxNode) -> Option<Item> {
        let item = match node.kind() {
            SyntaxKind::UseDecl => Item::Use(UseDecl { syntax: node }),
            SyntaxKind::FnDecl => Item::Function(FnDecl { syntax: node }),
            SyntaxKind::StructDecl => Item::Struct(StructDecl { syntax: node }),
            SyntaxKind::EnumDecl => Item::Enum(EnumDecl { syntax: node }),
            SyntaxKind::OperatorDecl => Item::Operator(OperatorDecl { syntax: node }),
            SyntaxKind::InstantiateDecl => Item::Instantiate(InstantiateDecl { syntax: node }),
            SyntaxKind::NativeTypeDecl => Item::NativeType(NativeTypeDecl { syntax: node }),
            SyntaxKind::ConstantDecl => Item::Constant(ConstantDecl { syntax: node }),
            SyntaxKind::NativeFnDecl => Item::NativeFunction(NativeFnDecl { syntax: node }),
            SyntaxKind::TestDecl => Item::Test(TestDecl { syntax: node }),
            SyntaxKind::TestContext => Item::TestContext(TestContext { syntax: node }),
            SyntaxKind::RunStmt => Item::Run(RunStmt { syntax: node }),
            SyntaxKind::StoreStmt => Item::Store(StoreStmt { syntax: node }),
            _ => return None,
        };
        Some(item)
    }

    /// The node of the item.
    pub fn syntax(&self) -> &SyntaxNode {
        match self {
            Item::Use(item) => item.syntax(),
            Item::Function(item) => item.syntax(),
            Item::Struct(item) => item.syntax(),
            Item::Enum(item) => item.syntax(),
            Item::Operator(item) => item.syntax(),
            Item::Instantiate(item) => item.syntax(),
            Item::NativeType(item) => item.syntax(),
            Item::Constant(item) => item.syntax(),
            Item::NativeFunction(item) => item.syntax(),
            Item::Test(item) => item.syntax(),
            Item::TestContext(item) => item.syntax(),
            Item::Run(item) => item.syntax(),
            Item::Store(item) => item.syntax(),
        }
    }
}

impl FnDecl {
    /// The function's name: the identifier after `fn`.
    pub fn name(&self) -> Option<SyntaxToken> {
        name_after(&self.syntax, SyntaxKind::KwFn)
    }

    /// The `export` before the function, when it has one.
    pub fn export(&self) -> Option<SyntaxToken> {
        tokens(&self.syntax).find(|token| token.kind() == SyntaxKind::KwExport)
    }

    /// True for `impl fn` and `impl const fn`: a candidate of an
    /// operator (GEN-4).
    pub fn is_impl(&self) -> bool {
        has_token(&self.syntax, SyntaxKind::KwImpl)
    }

    /// True for `const fn`: a value function (FN-1). In `const fn const()`
    /// the first `const` is the modifier and the second the name (FN-5).
    pub fn is_const(&self) -> bool {
        has_word_before(&self.syntax, "const", SyntaxKind::KwFn)
    }
}

impl StructDecl {
    /// The struct's name.
    pub fn name(&self) -> Option<SyntaxToken> {
        name_after(&self.syntax, SyntaxKind::KwStruct)
    }

    /// The `export` before the struct, when it has one.
    pub fn export(&self) -> Option<SyntaxToken> {
        tokens(&self.syntax).find(|token| token.kind() == SyntaxKind::KwExport)
    }
}

impl EnumDecl {
    /// The enum's name.
    pub fn name(&self) -> Option<SyntaxToken> {
        name_after(&self.syntax, SyntaxKind::KwEnum)
    }

    /// The `export` before the enum, when it has one.
    pub fn export(&self) -> Option<SyntaxToken> {
        tokens(&self.syntax).find(|token| token.kind() == SyntaxKind::KwExport)
    }
}

impl OperatorDecl {
    /// The operator's name.
    pub fn name(&self) -> Option<SyntaxToken> {
        name_after(&self.syntax, SyntaxKind::KwOperator)
    }

    /// The `export` before the operator, when it has one (UNIT-3).
    pub fn export(&self) -> Option<SyntaxToken> {
        tokens(&self.syntax).find(|token| token.kind() == SyntaxKind::KwExport)
    }
}

impl NativeTypeDecl {
    /// The type's name: the identifier after `native type`. Those two
    /// words are identifiers as well (LEX-10), so the name is the third.
    pub fn name(&self) -> Option<SyntaxToken> {
        tokens(&self.syntax)
            .filter(|token| token.kind() == SyntaxKind::Ident)
            .nth(2)
    }

    /// The `export` before the type, when it has one.
    pub fn export(&self) -> Option<SyntaxToken> {
        tokens(&self.syntax).find(|token| token.kind() == SyntaxKind::KwExport)
    }
}

impl ConstantDecl {
    /// The constant's name: the identifier after `const`, which is an
    /// identifier as well (LEX-10), so the name is the second.
    pub fn name(&self) -> Option<SyntaxToken> {
        tokens(&self.syntax)
            .filter(|token| token.kind() == SyntaxKind::Ident)
            .nth(1)
    }

    /// The `export` before the constant, when it has one.
    pub fn export(&self) -> Option<SyntaxToken> {
        tokens(&self.syntax).find(|token| token.kind() == SyntaxKind::KwExport)
    }
}

impl NativeFnDecl {
    /// The function's name: the identifier after `fn`.
    pub fn name(&self) -> Option<SyntaxToken> {
        name_after(&self.syntax, SyntaxKind::KwFn)
    }

    /// The `export` before the function, when it has one (NATIVE-1).
    pub fn export(&self) -> Option<SyntaxToken> {
        tokens(&self.syntax).find(|token| token.kind() == SyntaxKind::KwExport)
    }

    /// True for `native const fn`: a value function (NATIVE-1).
    pub fn is_const(&self) -> bool {
        has_word_before(&self.syntax, "const", SyntaxKind::KwFn)
    }

    /// True for `native impl fn` and `native impl const fn`: a candidate
    /// of an operator whose body a provider supplies (NATIVE-1).
    pub fn is_impl(&self) -> bool {
        has_token(&self.syntax, SyntaxKind::KwImpl)
    }

    /// The contract body, `{ ... }`. With one the declaration is an
    /// implementation; without, a declaration only (NATIVE-3).
    pub fn body(&self) -> Option<NativeContractBody> {
        child(&self.syntax)
    }
}

impl TestDecl {
    /// The named functions this test declares before its statements (TST-1).
    pub fn helpers(&self) -> impl Iterator<Item = FnDecl> {
        self.syntax
            .children()
            .find(|node| node.kind() == SyntaxKind::Block)
            .into_iter()
            .flat_map(|body| body.children())
            .filter_map(FnDecl::cast)
    }

    /// The test's name.
    pub fn name(&self) -> Option<SyntaxToken> {
        if self.is_integration() {
            // `integration_test name`: the word is a name too, and the
            // test's is the one after it.
            return tokens(&self.syntax)
                .filter(|token| token.kind() == SyntaxKind::Ident)
                .nth(1);
        }
        name_after(&self.syntax, SyntaxKind::KwTest)
    }

    /// The duration of `@timeout(5s)` on the line before an integration
    /// test (TST-17), as its token. `None` for a test with none.
    pub fn timeout(&self) -> Option<SyntaxToken> {
        self.syntax
            .children()
            .find(|child| child.kind() == SyntaxKind::TimeoutAnnotation)?
            .children_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| token.kind() == SyntaxKind::Duration)
    }

    /// True for `integration_test name { ... }` (TST-17): a test that
    /// may run in real time, and is run only when it is asked for.
    pub fn is_integration(&self) -> bool {
        // An `export` before it has been reported by the parser.
        tokens(&self.syntax)
            .find(|token| token.kind() != SyntaxKind::KwExport)
            .is_some_and(|word| {
                word.kind() == SyntaxKind::Ident && word.text() == "integration_test"
            })
    }
}

impl TestContext {
    /// The helpers, types and named tests of the context, in source order.
    pub fn items(&self) -> impl Iterator<Item = Item> {
        self.syntax.children().filter_map(Item::cast)
    }
}
