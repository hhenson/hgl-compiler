//! TST-1 parser tests for the helper prefix of a named test block.

mod common;

use common::run;
use hgl_parser::ast::{AstNode, Item, TestDecl};
use hgl_parser::{Entry, SyntaxKind};

#[test]
fn a_named_test_starts_with_its_own_helpers() {
    // TST-1's with_a_helper example, followed by a generic value
    // helper using the ordinary function grammar (TST-3).
    let outcome = run(
        Entry::Declarations,
        r#"test with_a_helper {
    /** Twice the input. */
    fn twice(value: i64) -> i64 {
        when { return value * 2 }
    }
    const fn identity<T>(value: T) -> T => value
    assert eval twice(value: [1, _, 3]) == [2, _, 6]
    assert identity(3) == 3
}
test other {
    const fn identity(value: i64) -> i64 => value
    assert identity(4) == 4
}
"#,
    );
    assert_eq!(outcome.findings, [], "{}", outcome.report);
    assert!(!outcome.has_error_node());
    let tests: Vec<_> = outcome.tree.children().filter_map(TestDecl::cast).collect();
    assert_eq!(tests.len(), 2);
    let helpers: Vec<_> = tests[0].helpers().collect();
    assert_eq!(helpers.len(), 2);
    assert_eq!(helpers[0].name().unwrap().text(), "twice");
    assert!(!helpers[0].is_const());
    // SDOC-1: the documentation belongs to the function.
    let attached = helpers[0].syntax().children_with_tokens().next().unwrap();
    assert_eq!(attached.kind(), SyntaxKind::DocComment);
    assert_eq!(helpers[1].name().unwrap().text(), "identity");
    assert!(helpers[1].is_const());
    assert!(
        helpers[1]
            .syntax()
            .children()
            .any(|node| node.kind() == SyntaxKind::GenericParams)
    );
    let other: Vec<_> = tests[1].helpers().collect();
    assert_eq!(other.len(), 1);
    assert_eq!(other[0].name().unwrap().text(), "identity");
    let body = tests[0].syntax().children().next().unwrap();
    assert_eq!(body.kind(), SyntaxKind::Block);
    assert_eq!(
        body.children().map(|node| node.kind()).collect::<Vec<_>>(),
        [
            SyntaxKind::FnDecl,
            SyntaxKind::FnDecl,
            SyntaxKind::AssertStmt,
            SyntaxKind::AssertStmt,
        ]
    );
}

#[test]
fn an_integration_test_admits_a_helper_with_its_annotation() {
    // TST-1 applies to integration tests (TST-17). A helper is
    // declared as a function is (TST-3, ERR-10).
    let outcome = run(
        Entry::Declarations,
        r#"@timeout(5s)
integration_test live {
    @suppress_warning("node.no_activation")
    fn beat() -> i64 {
        start { return 1 }
    }
    let seen = run{length: 1s} beat()
}
"#,
    );
    assert_eq!(outcome.findings, [], "{}", outcome.report);
    assert!(!outcome.has_error_node());
    let test = outcome.tree.children().find_map(TestDecl::cast).unwrap();
    assert_eq!(test.name().unwrap().text(), "live");
    assert!(test.is_integration());
    assert_eq!(test.timeout().unwrap().text(), "5s");
    let helpers: Vec<_> = test.helpers().collect();
    assert_eq!(helpers.len(), 1);
    assert_eq!(helpers[0].name().unwrap().text(), "beat");
    assert_eq!(
        helpers[0].syntax().children().next().unwrap().kind(),
        SyntaxKind::WarningAnnotation
    );
}

#[test]
fn a_named_test_rejects_a_helper_after_a_statement() {
    // TST-1: helper declarations precede statements. Recovery keeps
    // the misplaced helper's body and reads the next test statement.
    let text = "test misplaced {\n    assert true\n    fn late() { when {} }\n    assert false\n}";
    let outcome = run(Entry::Declarations, text);
    assert_eq!(outcome.findings.len(), 1, "{}", outcome.report);
    assert_eq!(outcome.findings[0].code, "syntax.expected_token");
    assert!(outcome.findings[0].catalogued);
    assert_eq!(outcome.findings[0].rule, Some("TST-1"));
    assert_eq!(outcome.findings[0].at, "3:5");
    assert_eq!(
        outcome.findings[0].message,
        "a test helper must precede the test's statements"
    );
    let test = outcome.tree.children().find_map(TestDecl::cast).unwrap();
    assert_eq!(test.helpers().count(), 0);
    let body = test.syntax().children().next().unwrap();
    assert_eq!(
        body.children().map(|node| node.kind()).collect::<Vec<_>>(),
        [
            SyntaxKind::AssertStmt,
            SyntaxKind::Error,
            SyntaxKind::AssertStmt
        ]
    );
}

#[test]
fn test_helpers_are_private_and_end_with_a_line_break() {
    // TST-1: the test_block grammar admits private helpers followed
    // by NL, including when the block ends after the helper.
    for item in [
        "export fn helper() {}",
        "export struct Helper {}",
        "export operator helper() -> i64",
        "native fn helper()",
        "native type Helper",
        "use anything::{helper}",
    ] {
        let outcome = run(
            Entry::Declarations,
            &format!("test helpers {{\n    {item}\n}}"),
        );
        assert_eq!(outcome.findings.len(), 1, "{}", outcome.report);
        assert_eq!(outcome.findings[0].code, "syntax.expected_token");
        assert_eq!(outcome.findings[0].rule, Some("TST-1"));
        assert_eq!(outcome.findings[0].at, "2:5");
    }
    let outcome = run(Entry::Declarations, "test helpers { fn helper() {} }");
    assert_eq!(outcome.findings.len(), 1, "{}", outcome.report);
    assert_eq!(outcome.findings[0].rule, Some("TST-1"));
    assert_eq!(
        outcome.findings[0].message,
        "expected a line break after a test helper, found `}`"
    );
    let outcome = run(Entry::Declarations, "test helpers { fn helper() {}\n}");
    assert_eq!(outcome.findings, [], "{}", outcome.report);
    let test = outcome.tree.children().find_map(TestDecl::cast).unwrap();
    assert_eq!(test.helpers().count(), 1);
}

#[test]
fn an_anonymous_function_remains_a_statement_in_a_named_test() {
    // TST-1's helper prefix distinguishes named declarations from
    // anonymous expressions, including a test's first statement.
    let outcome = run(
        Entry::Declarations,
        "test ordinary {\n    fn(value: i64) => value\n    assert true\n}",
    );
    assert_eq!(outcome.findings, [], "{}", outcome.report);
    assert!(!outcome.has_error_node());
    let test = outcome.tree.children().find_map(TestDecl::cast).unwrap();
    assert_eq!(test.helpers().count(), 0);
    let body = test.syntax().children().next().unwrap();
    assert_eq!(
        body.children().map(|node| node.kind()).collect::<Vec<_>>(),
        [SyntaxKind::ExprStmt, SyntaxKind::AssertStmt]
    );
}

/// TST-1: test-local declarations use the same syntax as the module's,
/// and stop before the test's first statement.
#[test]
fn a_test_prefix_accepts_types_constants_operators_and_candidates() {
    let outcome = run(
        Entry::Declarations,
        r#"test with_a_type {
    const cutoff: date = @2026-01-05
    struct Closed {
        day: date
    }
    enum Schedule { regular, holiday }
    operator open_on<C>(calendar: C, day: date) -> bool
    impl const fn open_on(calendar: Closed, day: date) -> bool =>
        day != calendar.day
    assert !open_on(Closed(day: cutoff), cutoff)
}
"#,
    );
    assert_eq!(outcome.findings, [], "{}", outcome.report);
    assert!(!outcome.has_error_node());
    let test = outcome.tree.children().find_map(TestDecl::cast).unwrap();
    let declarations = test.declarations().collect::<Vec<_>>();
    assert!(matches!(
        declarations.as_slice(),
        [
            Item::Constant(_),
            Item::Struct(_),
            Item::Enum(_),
            Item::Operator(_),
            Item::Function(_)
        ]
    ));
    let Item::Function(candidate) = &declarations[4] else {
        unreachable!()
    };
    assert!(candidate.is_impl());
}

#[test]
fn a_type_after_a_test_statement_is_an_error() {
    let outcome = run(
        Entry::Declarations,
        "test misplaced {\n    assert true\n    struct Late { value: i64 }\n    assert false\n}",
    );
    assert_eq!(outcome.findings.len(), 1, "{}", outcome.report);
    assert_eq!(outcome.findings[0].code, "syntax.expected_token");
    assert_eq!(outcome.findings[0].rule, Some("TST-1"));
    assert_eq!(outcome.findings[0].at, "3:5");
    let test = outcome.tree.children().find_map(TestDecl::cast).unwrap();
    assert_eq!(test.declarations().count(), 0);
}
