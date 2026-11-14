//! The typed views of `hgl_parser::ast`: each reads the part of the tree
//! it is named for, and gives `None` where a broken file lacks the part.

mod common;

use common::run;
use hgl_parser::ast::{AstNode, Item, SourceFile};
use hgl_parser::{Entry, HasSpan};
use hgl_source::Span;

/// The file view of `text`, parsed as a whole file.
fn file(text: &str) -> SourceFile {
    SourceFile::cast(run(Entry::SourceFile, text).tree).expect("the root is a file")
}

/// The text of a token that may be missing.
fn text(token: Option<hgl_parser::SyntaxToken>) -> Option<String> {
    token.map(|token| token.text().to_string())
}

/// A one-word description of each item, in order.
fn kinds(items: impl Iterator<Item = Item>) -> Vec<&'static str> {
    items
        .map(|item| match item {
            Item::Use(_) => "use",
            Item::Function(_) => "fn",
            Item::Struct(_) => "struct",
            Item::Enum(_) => "enum",
            Item::Operator(_) => "operator",
            Item::Instantiate(_) => "instantiate",
            Item::NativeType(_) => "native type",
            Item::NativeFunction(_) => "native fn",
            Item::Test(_) => "test",
            Item::TestContext(_) => "test context",
        })
        .collect()
}

/// UNIT-1: the module line names a module and optionally a part.
#[test]
fn a_module_line_gives_its_path_and_part() {
    let unnamed = file("module pricing.curves\n").module_decl().unwrap();
    assert_eq!(unnamed.path().unwrap().dotted(), "pricing.curves");
    assert_eq!(text(unnamed.part()), None);

    // Whitespace and comments between the names are not part of the path.
    let part = file("module pricing . /* x */ curves part bootstrap\n")
        .module_decl()
        .unwrap();
    assert_eq!(part.path().unwrap().dotted(), "pricing.curves");
    assert_eq!(text(part.part()), Some("bootstrap".to_string()));
    let segments: Vec<String> = part
        .path()
        .unwrap()
        .segments()
        .map(|token| token.text().to_string())
        .collect();
    assert_eq!(segments, ["pricing", "curves"]);
}

/// UNIT-2: an import is selective or an alias.
#[test]
fn an_import_gives_its_module_and_names() {
    let source = file("module m\nuse a.b::{x, y,}\nuse a.b as ab\n");
    let imports: Vec<_> = source
        .items()
        .filter_map(|item| match item {
            Item::Use(import) => Some(import),
            _ => None,
        })
        .collect();
    assert_eq!(imports.len(), 2);

    let selective = &imports[0];
    assert_eq!(selective.path().unwrap().dotted(), "a.b");
    let names: Vec<String> = selective
        .import_set()
        .unwrap()
        .names()
        .map(|token| token.text().to_string())
        .collect();
    assert_eq!(names, ["x", "y"]);
    assert_eq!(text(selective.alias()), None);

    let alias = &imports[1];
    assert_eq!(alias.path().unwrap().dotted(), "a.b");
    assert!(alias.import_set().is_none());
    assert_eq!(text(alias.alias()), Some("ab".to_string()));
}

#[test]
fn a_file_lists_its_items_in_order() {
    let source = file(concat!(
        "module m\n",
        "use a::{x}\n",
        "fn f() {}\n",
        "struct S { a: i64 }\n",
        "enum E { a }\n",
        "operator add(left: i64, right: i64) -> i64\n",
        "instantiate add<i64>\n",
        "native type Handle\n",
        "native fn tick() -> i64\n",
        "test t { assert true }\n",
        "test {\n    fn helper() {}\n    test inner { assert true }\n}\n",
    ));
    assert_eq!(
        kinds(source.items()),
        [
            "use",
            "fn",
            "struct",
            "enum",
            "operator",
            "instantiate",
            "native type",
            "native fn",
            "test",
            "test context"
        ]
    );
    let Some(Item::TestContext(context)) = source.items().last() else {
        panic!("the last item is the test context");
    };
    assert_eq!(kinds(context.items()), ["fn", "test"]);
}

/// FN-1, FN-5, GEN-4: a function's modifiers and its name.
#[test]
fn a_function_gives_its_name_and_modifiers() {
    let source = file(concat!(
        "module m\n",
        "fn plain() {}\n",
        "export fn shared() {}\n",
        "const fn value() -> i64 => 1\n",
        "export const fn both() -> i64 => 1\n",
        "impl fn add(left: i64, right: i64) -> i64 => left\n",
        // FN-5: the first `const` is the modifier, the second the name.
        "const fn const(value: i64) -> i64 => value\n",
        // And a temporal function may be named `const`.
        "fn const() {}\n",
    ));
    let seen: Vec<(String, bool, bool, bool)> = source
        .items()
        .filter_map(|item| match item {
            Item::Function(function) => Some((
                text(function.name()).unwrap(),
                function.export().is_some(),
                function.is_const(),
                function.is_impl(),
            )),
            _ => None,
        })
        .collect();
    let expected = [
        ("plain", false, false, false),
        ("shared", true, false, false),
        ("value", false, true, false),
        ("both", true, true, false),
        ("add", false, false, true),
        ("const", false, true, false),
        ("const", false, false, false),
    ]
    .map(|(name, export, constant, implementation)| {
        (name.to_string(), export, constant, implementation)
    });
    assert_eq!(seen, expected);
}

/// LEX-10, NATIVE-1, NATIVE-3: `native`, `type` and `const` are names by
/// position, and a body makes an implementation.
#[test]
fn types_operators_natives_and_tests_give_their_names() {
    let source = file(concat!(
        "module m\n",
        "export abstract struct Quote<T> { bid: T }\n",
        "export enum Side { buy, sell }\n",
        "operator scale(value: f64) -> f64\n",
        "export native type Handle\n",
        "native type type\n",
        "native fn tick() -> i64\n",
        "native const fn describe(value: i64) -> str {}\n",
        "native fn native() -> i64 { when; }\n",
        "test survives { assert true }\n",
    ));
    let mut seen = Vec::new();
    for item in source.items() {
        let line = match item {
            Item::Struct(decl) => format!(
                "struct {} export={}",
                text(decl.name()).unwrap(),
                decl.export().is_some()
            ),
            Item::Enum(decl) => format!(
                "enum {} export={}",
                text(decl.name()).unwrap(),
                decl.export().is_some()
            ),
            Item::Operator(decl) => format!("operator {}", text(decl.name()).unwrap()),
            Item::NativeType(decl) => format!(
                "native type {} export={}",
                text(decl.name()).unwrap(),
                decl.export().is_some()
            ),
            Item::NativeFunction(decl) => format!(
                "native fn {} const={} body={}",
                text(decl.name()).unwrap(),
                decl.is_const(),
                decl.body().is_some()
            ),
            Item::Test(decl) => format!("test {}", text(decl.name()).unwrap()),
            other => panic!("unexpected item {other:?}"),
        };
        seen.push(line);
    }
    assert_eq!(
        seen,
        [
            "struct Quote export=true",
            "enum Side export=true",
            "operator scale",
            "native type Handle export=true",
            "native type type export=false",
            "native fn tick const=false body=false",
            "native fn describe const=true body=true",
            "native fn native const=false body=true",
            "test survives",
        ]
    );
}

/// The parser builds a tree for any tokens; a view of a broken
/// declaration gives `None` for the part that is missing.
#[test]
fn a_missing_part_is_none() {
    let outcome = run(
        Entry::SourceFile,
        "module\nfn () {}\nstruct { a: i64 }\nuse a\ntest { fn }\n",
    );
    assert!(!outcome.findings.is_empty(), "the file is broken");
    let source = SourceFile::cast(outcome.tree).unwrap();
    let module = source.module_decl().unwrap();
    assert_eq!(module.path().map(|path| path.dotted()), Some(String::new()));
    for item in source.items() {
        match item {
            Item::Function(decl) => assert_eq!(text(decl.name()), None),
            Item::Struct(decl) => assert_eq!(text(decl.name()), None),
            Item::Use(decl) => {
                assert!(decl.import_set().is_none());
                assert_eq!(text(decl.alias()), None);
            }
            Item::TestContext(context) => {
                for helper in context.items() {
                    if let Item::Function(decl) = helper {
                        assert_eq!(text(decl.name()), None);
                    }
                }
            }
            _ => {}
        }
    }

    // A file with no module line has none.
    let headless = file("fn f() {}\n");
    assert!(headless.module_decl().is_none());
    assert_eq!(kinds(headless.items()), ["fn"]);
}

#[test]
fn a_span_is_the_bytes_covered() {
    let source = file("module m\nfn scale() {}\n");
    let Some(Item::Function(function)) = source.items().next() else {
        panic!("the first item is the function");
    };
    assert_eq!(function.syntax().span(), Span::new(9, 22));
    assert_eq!(function.name().unwrap().span(), Span::new(12, 17));
}
