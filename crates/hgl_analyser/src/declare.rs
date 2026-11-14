//! Lists what a module declares and builds its scopes.
//!
//! A module has one declaration scope, shared by all its files (UNIT-1),
//! and one test scope, made of the unnamed `test { ... }` contexts of all
//! its files (TST-1). A name is bound once in scope (UNIT-12), with one
//! exception: a temporal function and a value function may share a name
//! (FN-4).

use std::collections::HashMap;

use hgl_diagnostics::{Category, Diagnostics, Location};
use hgl_parser::ast::{AstNode, Item, UseDecl};
use hgl_parser::{HasSpan, SyntaxNode, SyntaxToken};
use hgl_source::{FileId, Span};

use crate::program::{Declaration, DeclarationKind, Import, ImportKind, Module, Role, Test};
use crate::{codes, error};

/// Fills in a module's imports, declarations, tests and scope from its
/// files. `omits[f]` lists the nodes of file `f` to leave out.
pub(crate) fn declare(module: &mut Module, omits: &[&[Span]], diagnostics: &mut Diagnostics) {
    let mut walk = Walk {
        declarations: Vec::new(),
        tests: Vec::new(),
        diagnostics,
    };
    for (index, file) in module.files.iter_mut().enumerate() {
        let omit = omits[index];
        // UNIT-2: imports precede declarations. Once a declaration has
        // been seen, an import is out of place.
        let mut declaration_seen = false;
        for item in file.syntax.items() {
            if omit.contains(&item.syntax().span()) {
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
    module.declarations = walk.declarations;
    module.tests = walk.tests;
    bind_declarations(module, diagnostics);
    bind_native_implementations(module, diagnostics);
    check_tests(module, diagnostics);
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
    tests: Vec<Test>,
    diagnostics: &'a mut Diagnostics,
}

impl Walk<'_> {
    /// Records one item of file number `index`. `in_test` is true for an
    /// item of an unnamed test context.
    fn item(&mut self, index: usize, file: FileId, item: Item, in_test: bool, omit: &[Span]) {
        let syntax = item.syntax().clone();
        // Each arm gives the item's name, what it is, and its `export`
        // token when it has one. An item without a name is one the
        // parser could not read and has reported; it declares nothing.
        let (name, kind, export) = match item {
            // TST-1: a context holds no import. The parser has said so.
            Item::Use(_) => return,
            // GEN-8: `instantiate` requests candidates and declares
            // nothing.
            Item::Instantiate(_) => return,
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
                    self.tests.push(Test {
                        name: name.text().to_string(),
                        at: Location::new(file, name.span()),
                        file: index,
                        syntax,
                    });
                }
                return;
            }
            Item::Function(function) => {
                let role = if function.is_const() {
                    Role::Value
                } else {
                    Role::Temporal
                };
                let kind = if function.is_impl() {
                    DeclarationKind::Implementation(None)
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
                // declaration.
                let kind = if function.body().is_some() {
                    DeclarationKind::NativeImplementation(role)
                } else {
                    DeclarationKind::NativeFunction(role)
                };
                (function.name(), kind, None)
            }
            Item::Operator(operator) => (operator.name(), DeclarationKind::Operator, None),
            Item::Struct(decl) => (decl.name(), DeclarationKind::Struct, decl.export()),
            Item::Enum(decl) => (decl.name(), DeclarationKind::Enum, decl.export()),
            Item::NativeType(decl) => (decl.name(), DeclarationKind::NativeType, decl.export()),
        };
        let Some(name) = name else {
            return;
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
            if matches!(kind, DeclarationKind::Implementation(_)) {
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
        self.declarations.push(Declaration {
            name: name.text().to_string(),
            at,
            kind,
            exported: export.is_some() && !in_test,
            test_only: in_test,
            file: index,
            syntax,
        });
    }
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
            declarations[index].kind.binds_name() && declarations[index].test_only == test_only
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
