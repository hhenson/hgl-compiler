//! SDOC-9: documentation examples use ordinary test syntax at the source
//! bytes of their comments, including in nested Markdown.

use hgl_diagnostics::Diagnostics;
use hgl_lexer::lex;
use hgl_parser::documentation::{Comment, attached, parse_examples};
use hgl_parser::{Entry, HasSpan, Limits, SyntaxKind, parse};
use hgl_source::SourceMap;

#[test]
fn runnable_and_shown_fences_keep_their_order_and_code_positions() {
    let input = r#"module demo.docs

/**
A réglage.

Examples:
    > ~~~hgl
    > assert true
    > ~~~

    ~~~hgl no-run
    This is shown only.
    ~~~

    ```hgl
    assert false
    ```

Notes:
    ~~~hgl
    Another shown example.
    ~~~
*/
export const marker: i64 = 1
"#
    .replace('\n', "\r\n");
    let mut sources = SourceMap::new();
    let file = sources.add("docs.hgl", input).unwrap();
    let mut diagnostics = Diagnostics::new();
    let limits = Limits::default();
    let tokens = lex(&sources, file, &mut diagnostics);
    let parsed = parse(
        &sources,
        file,
        &tokens,
        Entry::SourceFile,
        limits,
        &mut diagnostics,
    );
    let root = parsed.syntax();
    let owner = root
        .children()
        .find(|node| node.kind() == SyntaxKind::ConstantDecl)
        .unwrap();
    let token = attached(&owner).unwrap();
    let comment = Comment::read(&token);
    let fences = comment.examples();
    let metadata: Vec<_> = fences
        .iter()
        .map(|fence| (fence.position, fence.runnable))
        .collect();
    assert_eq!(
        metadata,
        [
            (Some(1), true),
            (Some(2), false),
            (Some(3), true),
            (None, false)
        ]
    );

    let examples = parse_examples(&sources, file, &root, limits, &mut diagnostics);
    assert!(diagnostics.is_empty(), "{:?}", diagnostics.as_slice());
    let assertions: Vec<_> = examples
        .iter()
        .map(|example| {
            let assertion = example
                .test
                .descendants_with_tokens()
                .filter_map(|element| element.into_token())
                .find(|token| token.kind() == SyntaxKind::KwAssert)
                .unwrap();
            assert_eq!(sources.file(file).slice(assertion.span()), "assert");
            (
                example.position,
                sources.file(file).line_col(assertion.span().start).line,
            )
        })
        .collect();
    assert_eq!(assertions, [(1, 8), (3, 16)]);
}

#[test]
fn malformed_example_is_reported_on_its_line_and_the_next_one_is_read() {
    let input = r#"module demo.docs

/**
Examples:
    ~~~hgl
    assert )
    ~~~
    ~~~hgl
    assert true
    ~~~
*/
const marker: i64 = 1
"#;
    let mut sources = SourceMap::new();
    let file = sources.add("broken.hgl", input).unwrap();
    let mut diagnostics = Diagnostics::new();
    let limits = Limits::default();
    let tokens = lex(&sources, file, &mut diagnostics);
    let parsed = parse(
        &sources,
        file,
        &tokens,
        Entry::SourceFile,
        limits,
        &mut diagnostics,
    );
    assert!(diagnostics.is_empty(), "{:?}", diagnostics.as_slice());
    let examples = parse_examples(&sources, file, &parsed.syntax(), limits, &mut diagnostics);
    assert_eq!(examples.len(), 2);
    assert!(diagnostics.has_errors());
    assert!(
        diagnostics.as_slice().iter().all(|diagnostic| {
            diagnostic.primary.file == file
                && sources
                    .file(file)
                    .line_col(diagnostic.primary.span.start)
                    .line
                    == 6
        }),
        "{:?}",
        diagnostics.as_slice()
    );
}
