//! SDOC-9: reads runnable code fences through the ordinary named-test
//! grammar. Their syntax keeps the byte positions in the source comment.

use hgl_diagnostics::{Diagnostics, Location};
use hgl_lexer::{lex, literal::validate_literals};
use hgl_source::{FileId, SourceFile, SourceMap, Span};
use rowan::{GreenNode, GreenToken, Language, NodeOrToken};

use crate::documentation::{Comment, Example, attached};
use crate::tree::HglLanguage;
use crate::{Entry, Limits, SyntaxKind, SyntaxNode, parse};

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

/// Places a test header and closing brace in the fence's space. The
/// partial source retains each code byte and line break at its offset.
/// Bytes skipped by Markdown, such as block-quote markers, become spaces.
fn read_test(
    source: &SourceFile,
    file: FileId,
    example: &Example,
    limits: Limits,
    diagnostics: &mut Diagnostics,
) -> Option<(usize, GreenNode)> {
    const HEADER: &[u8] = b"test x{";
    let first_line = source.line_index(example.fence.start);
    let start = source.line_range(first_line)?.start;
    let body_end = example
        .source
        .last()
        .map_or(start + HEADER.len(), |last| last.end as usize);
    let expected_close = example.fence.end.saturating_sub(1) as usize;
    // CommonMark also admits an unclosed fence. Use the comment's
    // remaining margin for our closing brace in that case.
    let unclosed = body_end > expected_close;
    let close = if unclosed {
        body_end + 1
    } else {
        expected_close
    };
    let bytes = source.text().as_bytes();
    let mut text: Vec<_> = bytes
        .get(start..=close)?
        .iter()
        .map(|&byte| {
            if matches!(byte, b'\r' | b'\n') {
                byte
            } else {
                b' '
            }
        })
        .collect();
    text.get_mut(..HEADER.len())?.copy_from_slice(HEADER);
    for span in &example.source {
        let range = span.range();
        let local = range.start - start..range.end - start;
        text.get_mut(local)?.copy_from_slice(&bytes[range]);
    }
    if unclosed {
        text[close - start - 1] = b'\n';
    }
    text[close - start] = b'}';

    let mut fragment = SourceMap::new();
    let fragment_file = fragment.add_bytes("<documentation example>", text).ok()?;
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
        let shift = |location: &mut Location| {
            location.file = file;
            location.span.start += start as u32;
            location.span.end += start as u32;
        };
        shift(&mut report.primary);
        for related in &mut report.related {
            shift(&mut related.location);
        }
        diagnostics.emit(report);
    }
    let test = parsed
        .syntax()
        .children()
        .find(|node| node.kind() == SyntaxKind::TestDecl)?;
    Some((start, test.green().to_owned()))
}
