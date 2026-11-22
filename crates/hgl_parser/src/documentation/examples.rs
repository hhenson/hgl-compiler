//! SDOC-9: reads runnable code fences through the ordinary named-test
//! grammar. Their syntax keeps the byte positions in the source comment.

use hgl_diagnostics::{Category, Code, Diagnostic, Diagnostics, Location};
use hgl_lexer::{lex, literal::validate_literals};
use hgl_source::{FileId, SourceFile, SourceMap, Span};
use rowan::{GreenNode, GreenNodeBuilder, GreenToken, Language, NodeOrToken, WalkEvent};

use crate::documentation::{Comment, Example, attached};
use crate::tree::HglLanguage;
use crate::{Entry, HasSpan, Limits, SyntaxKind, SyntaxNode, codes, parse};

/// A runnable fence and the declaration whose comment owns it.
#[derive(Debug, Clone)]
pub struct RunnableExample {
    /// The original declaration, which identifies the example.
    pub owner: SyntaxNode,
    /// The position among the Examples fences of that comment, from one.
    pub position: usize,
    /// The place of the opening fence in the original source.
    pub at: Span,
    /// A test declaration whose block contains the example's statements.
    pub test: SyntaxNode,
}

struct ReadExample {
    owner: SyntaxNode,
    position: usize,
    at: Span,
    start: usize,
    test: GreenNode,
}

/// Reads a source file's runnable examples as named tests (SDOC-9).
/// Parse diagnostics already point into the original comments.
pub fn parse_examples(
    sources: &SourceMap,
    file: FileId,
    root: &SyntaxNode,
    limits: Limits,
    diagnostics: &mut Diagnostics,
) -> Vec<RunnableExample> {
    let mut read = Vec::new();
    for owner in root.descendants() {
        let Some(token) = attached(&owner) else {
            continue;
        };
        if !token.text().contains("Examples:") {
            continue;
        }
        for example in Comment::read(&token).examples() {
            if !example.runnable {
                continue;
            }
            let Some((start, test)) =
                read_test(sources.file(file), file, &example, limits, diagnostics)
            else {
                continue;
            };
            read.push(ReadExample {
                owner: owner.clone(),
                position: example.position.expect("runnable examples are numbered"),
                at: Span::empty(example.fence.start),
                start,
                test,
            });
        }
    }
    read.sort_by_key(|example| example.start);

    // Rowan takes a node's position from the byte lengths of its older
    // siblings. One synthetic root preserves all original byte positions
    // without giving each example its own copy of the file's prefix.
    let raw = HglLanguage::kind_to_raw;
    let mut children = Vec::new();
    let mut at = 0;
    for example in &read {
        if let Some(gap) = example.start.checked_sub(at).filter(|&gap| gap != 0) {
            let gap = GreenToken::new(raw(SyntaxKind::Whitespace), &" ".repeat(gap));
            children.push(NodeOrToken::Token(gap));
        }
        at = example.start + u32::from(example.test.text_len()) as usize;
        children.push(NodeOrToken::Node(example.test.clone()));
    }
    let tree = SyntaxNode::new_root(GreenNode::new(raw(SyntaxKind::Fragment), children));
    tree.children()
        .zip(read)
        .map(|(test, example)| RunnableExample {
            owner: example.owner,
            position: example.position,
            at: example.at,
            test,
        })
        .collect()
}

/// Location of the wrapper and each code run in the source comment.
struct Positions<'a> {
    example: &'a Example,
    start: usize,
    close: usize,
}

const HEADER: &str = "test x{\n";

impl Positions<'_> {
    fn at(&self, offset: usize) -> usize {
        let Some(code) = offset.checked_sub(HEADER.len()) else {
            return self.start + offset;
        };
        if code >= self.example.code.len() {
            return self.close;
        }
        let index = self
            .example
            .source
            .partition_point(|run| run.offset <= code)
            - 1;
        let run = &self.example.source[index];
        if run.synthetic {
            run.start
        } else {
            run.start + (code - run.offset)
        }
    }

    fn span(&self, written: Span) -> Span {
        let start = self.at(written.start as usize);
        let end = if written.is_empty() {
            start
        } else {
            (self.at(written.end as usize - 1) + 1).max(start)
        };
        Span::new(start as u32, end as u32)
    }
}

/// Parses the CommonMark code itself. Markdown margins cannot enter
/// tokens whose contents are significant, such as LEX-25 text blocks.
fn read_test(
    source: &SourceFile,
    file: FileId,
    example: &Example,
    limits: Limits,
    diagnostics: &mut Diagnostics,
) -> Option<(usize, GreenNode)> {
    let start = source
        .line_range(source.line_index(example.fence.start))?
        .start;
    let positions = Positions {
        example,
        start,
        close: example.fence.end.saturating_sub(1) as usize,
    };
    let mut text = String::from(HEADER);
    text.push_str(&example.code);
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text.push('}');

    let mut fragment = SourceMap::new();
    let fragment_file = fragment.add("<documentation example>", text).ok()?;
    let mut reports = Diagnostics::new();
    let tokens = lex(&fragment, fragment_file, &mut reports);
    validate_literals(&fragment, fragment_file, &tokens, &mut reports);
    let parsed = parse(
        &fragment,
        fragment_file,
        &tokens,
        Entry::Declarations,
        limits,
        &mut reports,
    );
    for mut report in reports.as_slice().iter().cloned() {
        let move_to_comment = |location: &mut Location| {
            location.file = file;
            location.span = positions.span(location.span);
        };
        move_to_comment(&mut report.primary);
        for related in &mut report.related {
            move_to_comment(&mut related.location);
        }
        diagnostics.emit(report);
    }
    let test = parsed
        .syntax()
        .children()
        .find(|node| node.kind() == SyntaxKind::TestDecl)?;
    let mut closing = test.last_token()?;
    while closing.kind().is_trivia() {
        closing = closing.prev_token()?;
    }
    // The test must close with the wrapper's brace. An earlier one can
    // leave a second, syntactically valid declaration in the fragment.
    let wrapper_brace = fragment.file(fragment_file).text().len() - 1;
    if closing.kind() != SyntaxKind::RBrace || closing.span().start as usize != wrapper_brace {
        if !reports.has_errors() {
            diagnostics.emit(
                Diagnostic::error(
                    Category::Parse,
                    Code::Catalogued(codes::SYNTAX_EXPECTED_TOKEN),
                    Location::new(file, positions.span(closing.span())),
                    "unexpected closing brace before the end of the documentation example",
                )
                .with_rule("SDOC-9"),
            );
        }
        return None;
    }
    Some((start, at_source_positions(&test, &positions)))
}

/// Repositions the syntax by placing Markdown's removed bytes between
/// tokens as trivia, leaving each token's text as the HGL lexer read it.
/// Positions within a multiline token are mapped before this step, when
/// reporting lexer and parser diagnostics.
fn at_source_positions(test: &SyntaxNode, positions: &Positions<'_>) -> GreenNode {
    let raw = HglLanguage::kind_to_raw;
    let whitespace = raw(SyntaxKind::Whitespace);
    let mut builder = GreenNodeBuilder::new();
    let mut at = positions.start;
    for event in test.preorder_with_tokens() {
        match event {
            WalkEvent::Enter(element) => {
                let start = positions.at(u32::from(element.text_range().start()) as usize);
                if at < start {
                    builder.token(whitespace, &" ".repeat(start - at));
                    at = start;
                }
                match element {
                    NodeOrToken::Node(node) => builder.start_node(raw(node.kind())),
                    NodeOrToken::Token(token) if token.kind() == SyntaxKind::Whitespace => {
                        // CommonMark may expand part of a tab to spaces. Its
                        // original width is what the next token must follow.
                        let end = positions.span(token.span()).end as usize;
                        if at < end {
                            builder.token(whitespace, &" ".repeat(end - at));
                            at = end;
                        }
                    }
                    NodeOrToken::Token(token) => {
                        builder.token(raw(token.kind()), token.text());
                        at += token.text().len();
                    }
                }
            }
            WalkEvent::Leave(NodeOrToken::Node(_)) => builder.finish_node(),
            WalkEvent::Leave(NodeOrToken::Token(_)) => {}
        }
    }
    builder.finish()
}
