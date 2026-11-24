//! Lists what a module declares and builds its scopes.
//!
//! A module has one declaration scope, shared by all its files (UNIT-1),
//! and a test scope made of all its unnamed `test { ... }` contexts. Each
//! named test has its own scope (TST-1). A name is bound once in scope
//! (UNIT-12), with one
//! exception: a temporal function and a value function may share a name
//! (FN-4).

use std::collections::HashMap;

use hgl_diagnostics::{Category, Diagnostics, Location};
use hgl_parser::ast::{AstNode, Item, TestDecl, UseDecl};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};
use hgl_source::{FileId, Span};

use crate::hir::EndpointOperation;
use crate::program::{
    Declaration, DeclarationKind, Import, ImportKind, Module, Program, Role, RunStatement,
    StoreStatement, Test,
};
use crate::{ParsedFile, codes, error, syntax};

/// Fills in a module's imports, declarations, tests and scope from its
/// files. The parsed inputs give each file's omissions and documentation tests.
pub(crate) fn declare(module: &mut Module, inputs: &[&ParsedFile], diagnostics: &mut Diagnostics) {
    let mut walk = Walk {
        declarations: Vec::new(),
        test_declarations: Vec::new(),
        tests: Vec::new(),
        instantiations: Vec::new(),
        application: module.application,
        runs: Vec::new(),
        stores: Vec::new(),
        diagnostics,
    };
    let mut run_omitted = false;
    for (index, file) in module.files.iter_mut().enumerate() {
        let omit = &inputs[index].omit;
        // UNIT-2: imports precede declarations. Once a declaration has
        // been seen, an import is out of place.
        let mut declaration_seen = false;
        for item in file.syntax.items() {
            if omit.contains(&item.syntax().span()) {
                // TST-13: what is left out is no part of the program. A
                // run statement left out is still the one its
                // application has: the application is not then one
                // with none.
                run_omitted |= matches!(item, Item::Run(_));
                continue;
            }
            match item {
                Item::Use(import) => {
                    if declaration_seen {
                        walk.diagnostics.emit(
                            error(
                                Category::Module,
                                codes::IMPORT_MISPLACED,
                                Location::new(file.file, first_token_span(import.syntax())),
                                "an import stands after a declaration".to_string(),
                                "UNIT-2",
                            )
                            .with_note("a file's imports come before its declarations"),
                        );
                    }
                    // It is bound all the same, so that what uses it is
                    // not reported as well.
                    read_import(&import, file.file, &mut file.imports);
                }
                item => {
                    declaration_seen = true;
                    walk.item(index, file.file, item, false, omit);
                }
            }
        }
    }
    walk.examples(module, inputs);
    let places: Vec<(Location, bool)> = walk
        .runs
        .iter()
        .map(|run| (run.statement.at, run.followed))
        .collect();
    check_runs(module, &places, run_omitted, walk.diagnostics);
    // RUN-1: an application runs its run statement: its first, where
    // it writes more.
    if module.application {
        module.run = walk.runs.into_iter().next().map(|run| run.statement);
    }
    // UNIT-14: "Between its declarations and its run statement it may
    // set the run's store ... A module holds none of these." One that
    // stands elsewhere is reported, and sets nothing.
    for store in walk.stores {
        let wrong = if !module.application {
            Some("stands in a module: an application sets its run's store")
        } else if store.late {
            Some("follows the run statement: the store is set before the run")
        } else if store.followed {
            Some("stands before a declaration: the store is set after the declarations")
        } else {
            None
        };
        match wrong {
            Some(why) => walk.diagnostics.emit(error(
                Category::Phase,
                codes::APPLICATION_STATEMENT,
                store.statement.at,
                format!("this statement {why}"),
                "UNIT-14",
            )),
            None => module.store.push(store.statement),
        }
    }
    module.declarations = walk.declarations;
    module.declarations.extend(walk.test_declarations);
    module.tests = walk.tests;
    module.instantiations = walk.instantiations;
    check_reserved_function_names(module, diagnostics);
    bind_declarations(module, diagnostics);
    bind_native_implementations(module, diagnostics);
    bind_native_candidates(module, diagnostics);
    check_tests(module, diagnostics);
}

/// UNIT-10 reserves an activation form's name on a function declaration.
fn check_reserved_function_names(module: &Module, diagnostics: &mut Diagnostics) {
    let name = EndpointOperation::Scheduled.name();
    for declaration in &module.declarations {
        if declaration.name == name && declaration.kind.binding_role().is_some() {
            diagnostics.emit(error(
                Category::Name,
                codes::NAME_RESERVED,
                declaration.at,
                format!(
                    "the function name `{name}` is reserved for the activation form `{name}()`"
                ),
                "UNIT-10",
            ));
        }
    }
}

/// The declaration label in a documentation test selector.
fn documented_name(owner: &SyntaxNode, part: Option<&str>) -> String {
    let member = match owner.kind() {
        SyntaxKind::ModuleDecl | SyntaxKind::ApplicationDecl => {
            return match part {
                None => "[module]".to_string(),
                Some(part) => format!("[part {part}]"),
            };
        }
        SyntaxKind::StructMember | SyntaxKind::EnumMember => syntax::name_token(owner),
        // A native property may be named by a reserved word (LEX-9).
        SyntaxKind::NativeProperty => syntax::tokens(owner).next(),
        _ => None,
    };
    if let (Some(parent), Some(member)) = (owner.parent(), member) {
        return format!("{}.{}", documented_name(&parent, part), member.text());
    }
    let name = match Item::cast(owner.clone()) {
        Some(Item::Function(item)) => item.name(),
        Some(Item::Struct(item)) => item.name(),
        Some(Item::Enum(item)) => item.name(),
        Some(Item::Operator(item)) => item.name(),
        Some(Item::NativeType(item)) => item.name(),
        Some(Item::Constant(item)) => item.name(),
        Some(Item::NativeFunction(item)) => item.name(),
        Some(Item::Test(item)) => item.name(),
        _ => syntax::name_token(owner),
    };
    name.map_or_else(
        || "[declaration]".to_string(),
        |name| name.text().to_string(),
    )
}

/// UNIT-14: an application holds "one run statement (RUN-1), which
/// follows its declarations. A module holds none." `runs` is where the
/// word of each run statement of the unit is written, in file order and
/// then source order, and whether a declaration follows it.
/// `run_omitted` is true when a run statement of the unit is left out
/// of the program (TST-13).
fn check_runs(
    module: &Module,
    runs: &[(Location, bool)],
    run_omitted: bool,
    diagnostics: &mut Diagnostics,
) {
    if !module.application {
        for &(at, _) in runs {
            diagnostics.emit(
                error(
                    Category::Phase,
                    codes::RUN_POSITION,
                    at,
                    "a run statement stands in a module".to_string(),
                    "UNIT-14",
                )
                .with_note("an application runs an entry: a file headed `application name`"),
            );
        }
        return;
    }
    for &(at, followed) in runs {
        if followed {
            diagnostics.emit(error(
                Category::Phase,
                codes::RUN_POSITION,
                at,
                "a declaration follows this run statement".to_string(),
                "UNIT-14",
            ));
        }
    }
    let runs: Vec<Location> = runs.iter().map(|&(at, _)| at).collect();
    match runs.as_slice() {
        // The one it has is left out (TST-13).
        [] if run_omitted => {}
        [] => {
            // "the application's path": where its file writes it.
            if let Some(at) = module.application_at() {
                diagnostics.emit(error(
                    Category::Module,
                    codes::APPLICATION_RUN_COUNT,
                    at,
                    format!("application `{}` has no run statement", module.path),
                    "UNIT-14",
                ));
            }
        }
        [first, later @ ..] => {
            for &at in later {
                diagnostics.emit(
                    error(
                        Category::Module,
                        codes::APPLICATION_RUN_COUNT,
                        at,
                        format!("application `{}` has a second run statement", module.path),
                        "UNIT-14",
                    )
                    .with_related(*first, "the first is here"),
                );
            }
        }
    }
}

/// The bytes of a node's first token: the keyword a declaration starts
/// with. An empty span at the node's start when it has no token.
fn first_token_span(node: &SyntaxNode) -> Span {
    match node.first_token() {
        Some(token) => token.span(),
        None => Span::empty(node.span().start),
    }
}

/// Records the names one `use` line binds.
fn read_import(import: &UseDecl, file: FileId, imports: &mut Vec<Import>) {
    // A `use` line the parser could not read whole binds what it can.
    let Some(path) = import.path() else {
        return;
    };
    let from = path.dotted();
    let from_at = Location::new(file, path.syntax().span());
    // `|token, kind|` is a closure, a function written in place, which
    // can use `from` and `from_at` from around it.
    let mut bind = |token: SyntaxToken, kind: ImportKind| {
        imports.push(Import {
            name: token.text().to_string(),
            at: Location::new(file, token.span()),
            kind,
            from: from.clone(),
            from_at,
            module: None,
        });
    };
    if let Some(set) = import.import_set() {
        for name in set.names() {
            bind(name, ImportKind::Selective);
        }
    }
    if let Some(alias) = import.alias() {
        bind(alias, ImportKind::Alias);
    }
}

/// Walks a module's items and lists its declarations and tests.
struct Walk<'a> {
    declarations: Vec<Declaration>,
    /// Kept after ordinary declarations so a test cannot change their identities (TST-2).
    test_declarations: Vec<Declaration>,
    tests: Vec<Test>,
    instantiations: Vec<(usize, SyntaxNode)>,
    /// True when the unit walked is an application (UNIT-14).
    application: bool,
    /// Each run statement, in file order and then source order.
    runs: Vec<WalkedRun>,
    /// Each statement that sets the run's store, in the same order.
    stores: Vec<WalkedStore>,
    diagnostics: &'a mut Diagnostics,
}

/// A statement that sets the run's store, which the walk has met.
struct WalkedStore {
    /// The statement, as written.
    statement: StoreStatement,
    /// True when a declaration of its file follows it (UNIT-14).
    followed: bool,
    /// True when a run statement of its file stands before it.
    late: bool,
}

/// A run statement the walk has met.
struct WalkedRun {
    /// The statement, as written.
    statement: RunStatement,
    /// True when a declaration of its file follows it (UNIT-14).
    followed: bool,
}

impl Walk<'_> {
    /// Records a named test and the declarations private to its block (TST-1).
    fn test(&mut self, test: Test, syntax: &TestDecl, omit: &[Span]) {
        let owner = self.tests.len();
        for item in syntax.declarations() {
            let syntax = item.syntax().clone();
            if omit.contains(&syntax.span()) {
                continue;
            }
            let Some((name, kind, _)) = declaration(item) else {
                continue;
            };
            self.test_declarations.push(Declaration {
                name: name.text().to_string(),
                at: Location::new(test.at.file, name.span()),
                kind,
                exported: false,
                test_only: true,
                test_owner: Some(owner),
                file: test.file,
                syntax,
            });
        }
        self.tests.push(test);
    }

    /// Adds SDOC-9 tests after authored tests, preserving their indices.
    fn examples(&mut self, module: &Module, inputs: &[&ParsedFile]) {
        let mut owners = HashMap::new();
        for (index, input) in inputs.iter().enumerate() {
            let part = module.files[index].part.as_deref();
            let mut previous = None;
            let mut label = String::new();
            for example in &input.examples {
                let span = example.owner.span();
                if input
                    .omit
                    .iter()
                    .any(|omit| omit.start <= span.start && span.end <= omit.end)
                {
                    continue;
                }
                let Some(test) = TestDecl::cast(example.test.clone()) else {
                    continue;
                };
                if previous != Some(span) {
                    label = documented_name(&example.owner, part);
                    let occurrence = owners.entry(label.clone()).or_insert(0);
                    *occurrence += 1;
                    if *occurrence > 1 {
                        // `@` separates a timeout from a test name in the native host.
                        label.push_str(&format!("~{occurrence}"));
                    }
                    previous = Some(span);
                }
                self.test(
                    Test {
                        name: format!("{label}#example-{}", example.position),
                        documentation_example: true,
                        integration: false,
                        timeout: None,
                        at: Location::new(input.file, example.at),
                        file: index,
                        syntax: example.test.clone(),
                        scope: HashMap::new(),
                    },
                    &test,
                    &input.omit,
                );
            }
        }
    }

    /// Records one item of file number `index`. `in_test` is true for an
    /// item of an unnamed test context.
    fn item(&mut self, index: usize, file: FileId, item: Item, in_test: bool, omit: &[Span]) {
        let syntax = item.syntax().clone();
        // UNIT-14: the run statement "follows its declarations". A
        // declaration of the file after one is marked against it.
        if !in_test && !matches!(item, Item::Run(_) | Item::Store(_)) {
            for run in &mut self.runs {
                run.followed = true;
            }
            for store in &mut self.stores {
                store.followed = true;
            }
        }
        // Each arm gives the item's name, what it is, and its `export`
        // token when it has one. An item without a name is one the
        // parser could not read and has reported; it declares nothing.
        let (name, kind, export) = match item {
            // TST-1: a context holds no import. The parser has said so.
            Item::Use(_) => return,
            // RUN-1: a run statement declares nothing. Its word is kept
            // for the count of them (UNIT-14). One in a test context
            // has been rejected by the parser (TST-1).
            Item::Run(run) => {
                if !in_test {
                    let span = match run.word() {
                        Some(word) => word.span(),
                        None => first_token_span(&syntax),
                    };
                    self.runs.push(WalkedRun {
                        statement: RunStatement {
                            at: Location::new(file, span),
                            file: index,
                            syntax,
                        },
                        followed: false,
                    });
                }
                return;
            }
            // UNIT-14: a statement that sets the run's store declares
            // nothing. One in a test context has been rejected by the
            // parser (TST-1).
            Item::Store(_) => {
                if !in_test {
                    self.stores.push(WalkedStore {
                        statement: StoreStatement {
                            at: Location::new(file, first_token_span(&syntax)),
                            file: index,
                            syntax,
                        },
                        followed: false,
                        late: !self.runs.is_empty(),
                    });
                }
                return;
            }
            // GEN-8: `instantiate` requests candidates and declares
            // nothing. It is kept for the names it uses. One in a test
            // context has been rejected by the parser (TST-1).
            Item::Instantiate(_) => {
                if !in_test {
                    self.instantiations.push((index, syntax));
                }
                return;
            }
            Item::TestContext(context) => {
                // TST-1: contexts make no namespace of their own, so one
                // inside another, which the parser has reported, adds to
                // the same test scope.
                for inner in context.items() {
                    if !omit.contains(&inner.syntax().span()) {
                        self.item(index, file, inner, true, omit);
                    }
                }
                return;
            }
            Item::Test(test) => {
                if let Some(name) = test.name() {
                    self.test(
                        Test {
                            name: name.text().to_string(),
                            documentation_example: false,
                            integration: test.is_integration(),
                            timeout: test.timeout().and_then(|written| {
                                hgl_lexer::literal::decode_duration(written.text()).ok()
                            }),
                            at: Location::new(file, name.span()),
                            file: index,
                            syntax,
                            scope: HashMap::new(),
                        },
                        &test,
                        omit,
                    );
                }
                return;
            }
            item => {
                let Some(declaration) = declaration(item) else {
                    return;
                };
                declaration
            }
        };
        let at = Location::new(file, name.span());

        if in_test {
            // TST-1: a context contributes private helpers and
            // unexported types, and no operator implementation.
            if let Some(export) = &export {
                self.diagnostics.emit(error(
                    Category::Module,
                    codes::TEST_EXPORT,
                    Location::new(file, export.span()),
                    format!(
                        "`{}` is declared for tests and cannot be exported",
                        name.text()
                    ),
                    "TST-1",
                ));
            }
            if matches!(kind, DeclarationKind::Implementation(..)) {
                self.diagnostics.emit(error(
                    Category::Operator,
                    codes::TEST_IMPLEMENTATION,
                    at,
                    format!(
                        "an implementation of operator `{}` stands in a test context",
                        name.text()
                    ),
                    "TST-1",
                ));
            }
        }
        // UNIT-14: "Its declarations are private: an application
        // exports nothing".
        if self.application
            && !in_test
            && let Some(export) = &export
        {
            self.diagnostics.emit(error(
                Category::Module,
                codes::APPLICATION_EXPORT,
                Location::new(file, export.span()),
                format!(
                    "`{}` is declared in an application, which exports nothing",
                    name.text()
                ),
                "UNIT-14",
            ));
        }
        self.declarations.push(Declaration {
            name: name.text().to_string(),
            at,
            kind,
            exported: export.is_some() && !in_test && !self.application,
            test_only: in_test,
            test_owner: None,
            file: index,
            syntax,
        });
    }
}

/// Reads a declaration's name, kind and export mark. The caller places it
/// in the module or in a test's own scope.
fn declaration(item: Item) -> Option<(SyntaxToken, DeclarationKind, Option<SyntaxToken>)> {
    let (name, kind, export) = match item {
        Item::Function(function) => {
            let role = if function.is_const() {
                Role::Value
            } else {
                Role::Temporal
            };
            // GEN-4: "An `impl fn` is a temporal candidate of the
            // operator it names and an `impl const fn` a value
            // candidate."
            let kind = if function.is_impl() {
                DeclarationKind::Implementation(role, None)
            } else {
                DeclarationKind::Function(role)
            };
            (function.name(), kind, function.export())
        }
        Item::NativeFunction(function) => {
            let role = if function.is_const() {
                Role::Value
            } else {
                Role::Temporal
            };
            // NATIVE-3: a body makes an implementation; none, a
            // declaration. NATIVE-1: with `impl`, the declaration
            // is a candidate of an operator. NATIVE-7: "Checking an
            // interface needs no executable provider loading." The
            // role is the one written here, and every later pass
            // checks a call against what the declaration says:
            // nothing a provider supplies is read, and a declaration
            // with no implementation part keeps its role.
            let kind = match (function.is_impl(), function.body().is_some()) {
                (false, false) => DeclarationKind::NativeFunction(role),
                (false, true) => DeclarationKind::NativeImplementation(role),
                (true, false) => DeclarationKind::NativeCandidate(role, None),
                (true, true) => DeclarationKind::NativeCandidateImplementation(role),
            };
            // NATIVE-1: a native function that is no candidate "is
            // private unless exported, as a function is".
            (function.name(), kind, function.export())
        }
        // UNIT-3, GEN-4: record an operator's export mark.
        Item::Operator(operator) => (
            operator.name(),
            DeclarationKind::Operator,
            operator.export(),
        ),
        Item::Struct(decl) => (decl.name(), DeclarationKind::Struct, decl.export()),
        Item::Enum(decl) => (decl.name(), DeclarationKind::Enum, decl.export()),
        Item::NativeType(decl) => (decl.name(), DeclarationKind::NativeType, decl.export()),
        // UNIT-13: a constant is a declaration of its module, private
        // unless exported (UNIT-3).
        Item::Constant(decl) => (decl.name(), DeclarationKind::Constant, decl.export()),
        _ => return None,
    };
    Some((name?, kind, export))
}

/// Builds the module's scope from its declarations and rejects a name
/// declared twice.
///
/// The module's own declarations are bound first and its test helpers
/// after them, so that of a helper and a declaration with one name it is
/// the helper that is reported (TST-2: "A test-only helper or type takes
/// no name of a production declaration"). Within each group the order is
/// the order of the module's files and then of the source, and the first
/// declaration keeps the name.
fn bind_declarations(module: &mut Module, diagnostics: &mut Diagnostics) {
    let declarations = &module.declarations;
    let binders = |test_only: bool| {
        // `move` lets the closure keep its own copy of `test_only`.
        (0..declarations.len()).filter(move |&index| {
            let declaration = &declarations[index];
            declaration.test_owner.is_none()
                && declaration.kind.binds_name()
                && declaration.test_only == test_only
        })
    };
    let mut scope: HashMap<String, Vec<usize>> = HashMap::new();
    for index in binders(false).chain(binders(true)) {
        let declaration = &declarations[index];
        let bound = scope.entry(declaration.name.clone()).or_default();
        let clash = bound
            .iter()
            .map(|&other| &declarations[other])
            .find(|other| {
                // TST-2: a helper takes no name of the module at all, not
                // even to pair a value function with a temporal one. Two
                // helpers may pair as two declarations may.
                clashes(other.kind, declaration.kind) || (declaration.test_only && !other.test_only)
            });
        match clash {
            Some(first) => {
                diagnostics.emit(duplicate(&module.path, first, declaration));
            }
            None => bound.push(index),
        }
    }
    module.scope = scope;
}

/// Binds a module's named-test declarations after its imports are known.
pub(crate) fn bind_test_declarations(
    program: &mut Program,
    index: usize,
    diagnostics: &mut Diagnostics,
) {
    let core = program.core();
    let module = &program.modules[index];
    let prelude = core
        .filter(|&core| core != index)
        .map(|core| &program.modules[core]);
    let mut scopes = vec![HashMap::<String, Vec<usize>>::new(); module.tests.len()];
    for (position, declaration) in module.declarations.iter().enumerate() {
        let Some(owner) = declaration.test_owner else {
            continue;
        };
        // A candidate is reached through its operator, with no binding of its own.
        if !declaration.kind.binds_name() {
            continue;
        }
        let name = &declaration.name;
        let test = &module.tests[owner];
        let visible = module.lookup(name).next().or_else(|| {
            prelude?
                .lookup(name)
                .find(|declaration| declaration.is_public())
        });
        let imported = module.files[declaration.file]
            .imports
            .iter()
            .find(|import| &import.name == name);
        if let Some(first) = imported
            .map(|import| import.at)
            .or_else(|| visible.map(|decl| decl.at))
        {
            diagnostics.emit(
                error(
                    Category::Name,
                    codes::NAME_DUPLICATE,
                    declaration.at,
                    format!("`{name}` takes a name test `{}` already sees", test.name),
                    "TST-2",
                )
                .with_related(first, "this name is already visible to the test"),
            );
            continue;
        }
        // FN-4 permits a value and a temporal function to share a name in this test.
        let bound = scopes[owner].entry(name.clone()).or_default();
        if let Some(first) = bound
            .iter()
            .map(|&other| &module.declarations[other])
            .find(|other| clashes(other.kind, declaration.kind))
        {
            diagnostics.emit(
                error(
                    Category::Name,
                    codes::NAME_DUPLICATE,
                    declaration.at,
                    format!(
                        "test `{}` has two {}s named `{name}`",
                        test.name,
                        declaration.kind.describe()
                    ),
                    "UNIT-5",
                )
                .with_related(first.at, "the first is declared here"),
            );
        } else {
            bound.push(position);
        }
    }
    for (test, scope) in program.modules[index].tests.iter_mut().zip(scopes) {
        test.scope = scope;
    }
}

/// True when two declarations of one name cannot both stand: always,
/// unless they are a temporal function and a value function (FN-4).
fn clashes(first: DeclarationKind, later: DeclarationKind) -> bool {
    match (first.binding_role(), later.binding_role()) {
        (Some(first), Some(later)) => first == later,
        _ => true,
    }
}

/// The diagnostic for `later`, which takes the name `first` has.
fn duplicate(
    module: &str,
    first: &Declaration,
    later: &Declaration,
) -> hgl_diagnostics::Diagnostic {
    let name = &later.name;
    let same_role = first.kind.binding_role().is_some()
        && first.kind.binding_role() == later.kind.binding_role();
    let both_native_declarations = matches!(first.kind, DeclarationKind::NativeFunction(_))
        && matches!(later.kind, DeclarationKind::NativeFunction(_));
    let (message, rule) = if later.test_only {
        // TST-2: a helper takes no name of the module, and helpers do
        // not repeat one another.
        (
            format!("test declaration `{name}` takes a name module `{module}` already declares"),
            "TST-2",
        )
    } else if both_native_declarations {
        (
            format!("native function `{name}` is declared twice in module `{module}`"),
            "NATIVE-2",
        )
    } else if same_role {
        (
            format!(
                "module `{module}` has two {}s named `{name}`",
                later.kind.describe()
            ),
            "UNIT-5",
        )
    } else {
        (
            format!("`{name}` is declared twice in module `{module}`"),
            "UNIT-12",
        )
    };
    let mut diagnostic = error(
        Category::Name,
        codes::NAME_DUPLICATE,
        later.at,
        message,
        rule,
    )
    .with_related(
        first.at,
        format!("first declared here, as a {}", first.kind.describe()),
    );
    if later.test_only && !first.test_only {
        diagnostic = diagnostic.with_note("a test helper or type needs a name of its own");
    } else if !same_role {
        diagnostic = diagnostic
            .with_note("only a temporal function and a value function may share a name (FN-4)");
    }
    diagnostic
}

/// NATIVE-2: a native implementation completes exactly one declaration,
/// that of its name and role, and a part implements it once.
fn bind_native_implementations(module: &mut Module, diagnostics: &mut Diagnostics) {
    // For each declaration completed, the files that implement it.
    let mut implemented: HashMap<usize, Vec<(usize, Location)>> = HashMap::new();
    for index in 0..module.declarations.len() {
        let implementation = &module.declarations[index];
        let DeclarationKind::NativeImplementation(role) = implementation.kind else {
            continue;
        };
        let name = &implementation.name;
        let bound = module.scope.get(name).map(Vec::as_slice).unwrap_or(&[]);
        // The declaration it completes: a native one of its role.
        let completes = bound.iter().copied().find(|&other| {
            module.declarations[other].kind == DeclarationKind::NativeFunction(role)
        });
        if let Some(declaration) = completes {
            let files = implemented.entry(declaration).or_default();
            // Two implementations in one part would both be selected
            // with it.
            let earlier = files
                .iter()
                .find(|(file, _)| *file == implementation.file)
                .map(|&(_, at)| at);
            match earlier {
                Some(earlier) => diagnostics.emit(
                    error(
                        Category::Name,
                        codes::NAME_DUPLICATE,
                        implementation.at,
                        format!("native function `{name}` is implemented twice in one part"),
                        "NATIVE-2",
                    )
                    .with_related(earlier, "first implemented here"),
                ),
                None => files.push((implementation.file, implementation.at)),
            }
            continue;
        }
        // Something else has the name in this role, or nothing has.
        let other = bound
            .iter()
            .map(|&other| &module.declarations[other])
            .find(|other| clashes(other.kind, DeclarationKind::NativeFunction(role)));
        let diagnostic = match other {
            Some(first) => error(
                Category::Name,
                codes::NAME_DUPLICATE,
                implementation.at,
                format!(
                    "native implementation `{name}` takes the name of a {}",
                    first.kind.describe()
                ),
                "NATIVE-2",
            )
            .with_related(first.at, "declared here"),
            None => error(
                Category::Name,
                codes::NATIVE_MISSING_DECLARATION,
                implementation.at,
                format!(
                    "native implementation `{name}` completes no declaration of module `{}`",
                    module.path
                ),
                "NATIVE-2",
            )
            .with_note(format!(
                "declare it without a body: `native {}fn {name}(...)`",
                match role {
                    Role::Temporal => "",
                    Role::Value => "const ",
                }
            )),
        };
        diagnostics.emit(diagnostic);
    }
}

/// NATIVE-2: "Native candidates of one operator that differ in role or
/// parameter types are distinct declarations, and an implementation
/// completes the candidate whose signature it matches."
///
/// A candidate binds no name, so its implementation is looked for among
/// the module's declarations and not in its scope. Signatures are read
/// by a later pass: here an implementation needs a native candidate of
/// its name and role, and which of them it completes is not decided.
fn bind_native_candidates(module: &Module, diagnostics: &mut Diagnostics) {
    for implementation in &module.declarations {
        let DeclarationKind::NativeCandidateImplementation(role) = implementation.kind else {
            continue;
        };
        let name = &implementation.name;
        let declared = module.declarations.iter().any(|other| {
            other.name == *name
                && matches!(other.kind, DeclarationKind::NativeCandidate(of, _) if of == role)
        });
        if declared {
            continue;
        }
        diagnostics.emit(
            error(
                Category::Name,
                codes::NATIVE_MISSING_DECLARATION,
                implementation.at,
                format!(
                    "native implementation `{name}` completes no candidate of module `{}`",
                    module.path
                ),
                "NATIVE-2",
            )
            .with_note(format!(
                "declare it without a body: `native impl {}fn {name}(...)`",
                match role {
                    Role::Temporal => "",
                    Role::Value => "const ",
                }
            )),
        );
    }
}

/// TST-2: test names are unique in a module, across all its files and
/// contexts. A test is not a value (UNIT-6), so its name is in no scope
/// and may match a declaration's.
fn check_tests(module: &Module, diagnostics: &mut Diagnostics) {
    let mut first: HashMap<&str, Location> = HashMap::new();
    for test in &module.tests {
        match first.get(test.name.as_str()) {
            Some(&earlier) => diagnostics.emit(
                error(
                    Category::Name,
                    codes::TEST_DUPLICATE_NAME,
                    test.at,
                    format!(
                        "module `{}` has two tests named `{}`",
                        module.path, test.name
                    ),
                    "TST-2",
                )
                .with_related(earlier, "the other is here"),
            ),
            None => {
                first.insert(&test.name, test.at);
            }
        }
    }
}
