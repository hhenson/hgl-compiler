//! The syntax tree the parser produces.
//!
//! The tree is *lossless*: every token of the file is in it, whitespace and
//! comments included, so the tree's text is the source text exactly. That is
//! what lets the formatter work from the tree alone.
//!
//! The tree types come from the `rowan` crate, which stores a tree in two
//! layers:
//!
//! * a *green* tree: immutable, compact, with no positions or parent links.
//!   This is what the parser builds and what [`Parse`] owns;
//! * a *red* tree ([`SyntaxNode`], [`SyntaxToken`]): a cheap view over the
//!   green tree that knows each element's parent and byte range. Code that
//!   reads the tree uses this layer.

use std::fmt::Write as _;

use hgl_source::Span;
use rowan::{GreenNode, NodeOrToken, TextRange, WalkEvent};

use crate::kind::SyntaxKind;

/// Tells `rowan` that this crate's trees use [`SyntaxKind`].
///
/// An enum with no variants cannot be constructed: the type exists only to
/// carry the [`rowan::Language`] implementation, like a C++ traits class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HglLanguage {}

impl rowan::Language for HglLanguage {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> SyntaxKind {
        // `raw.0` is the number inside rowan's wrapper type.
        SyntaxKind::from_raw(raw.0)
    }

    fn kind_to_raw(kind: SyntaxKind) -> rowan::SyntaxKind {
        rowan::SyntaxKind(kind.to_raw())
    }
}

/// A node of the tree: a [`SyntaxKind`], a byte range, and children.
pub type SyntaxNode = rowan::SyntaxNode<HglLanguage>;

/// A token of the tree: a [`SyntaxKind`], a byte range, and text.
pub type SyntaxToken = rowan::SyntaxToken<HglLanguage>;

/// A node or a token: what a node's children are.
pub type SyntaxElement = rowan::SyntaxElement<HglLanguage>;

/// Gives the bytes a node or token covers as a [`Span`], the form
/// diagnostics take.
///
/// This is an *extension trait*: a trait written here and implemented for
/// types that come from another crate, which adds a method to them. With
/// it in scope (`use hgl_parser::HasSpan`), `node.span()` works on
/// `rowan`'s nodes and tokens.
pub trait HasSpan {
    /// The bytes covered, counted from the start of the file.
    fn span(&self) -> Span;
}

/// `rowan`'s byte range as a [`Span`].
fn span_of(range: TextRange) -> Span {
    Span::new(u32::from(range.start()), u32::from(range.end()))
}

impl HasSpan for SyntaxNode {
    fn span(&self) -> Span {
        span_of(self.text_range())
    }
}

impl HasSpan for SyntaxToken {
    fn span(&self) -> Span {
        span_of(self.text_range())
    }
}

/// The result of parsing: the tree. Problems found on the way are in the
/// diagnostics sink the parser was given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parse {
    green: GreenNode,
}

impl Parse {
    /// Wraps a finished green tree.
    pub(crate) fn new(green: GreenNode) -> Parse {
        Parse { green }
    }

    /// The root node. Its kind is [`SyntaxKind::SourceFile`] or
    /// [`SyntaxKind::Fragment`].
    pub fn syntax(&self) -> SyntaxNode {
        // Cloning a green node copies a reference-counted pointer, not the
        // tree.
        SyntaxNode::new_root(self.green.clone())
    }
}

/// The tree as indented text, one element per line, with byte ranges:
///
/// ```text
/// SourceFile@0..24
///   ModuleDecl@0..23
///     KwModule@0..6 "module"
///     Whitespace@6..7 " "
/// ```
///
/// Every token is listed, so the output shows exactly what the tree holds.
/// `hgl tree` prints this.
pub fn dump(node: &SyntaxNode) -> String {
    let mut out = String::new();
    let mut depth = 0usize;
    // `preorder_with_tokens` visits the tree in source order, saying when
    // it enters and when it leaves each element. Walking this way, and not
    // with a function that calls itself for each child, uses no more of
    // the call stack for a deep tree than for a shallow one.
    for step in node.preorder_with_tokens() {
        match step {
            WalkEvent::Enter(element) => {
                let range = element.text_range();
                // Writing to a `String` cannot fail, so the `Result` is
                // dropped.
                let _ = write!(
                    out,
                    "{:indent$}{:?}@{}..{}",
                    "",
                    element.kind(),
                    u32::from(range.start()),
                    u32::from(range.end()),
                    indent = depth * 2
                );
                match element {
                    NodeOrToken::Node(_) => {
                        out.push('\n');
                        depth += 1;
                    }
                    NodeOrToken::Token(token) => {
                        let _ = writeln!(out, " {:?}", token.text());
                    }
                }
            }
            WalkEvent::Leave(NodeOrToken::Node(_)) => depth -= 1,
            WalkEvent::Leave(NodeOrToken::Token(_)) => {}
        }
    }
    out
}

/// The tree's shape on one line, without whitespace or comments:
/// `BinaryExpr[Literal["1"] "+" Literal["2"]]`. A node is its kind and its
/// children in brackets; a token is its text in quotes.
///
/// Tests compare against this form.
pub fn outline(node: &SyntaxNode) -> String {
    let mut out = String::new();
    // True when the next element written is the first inside its node,
    // and so needs no space before it.
    let mut first = true;
    for step in node.preorder_with_tokens() {
        match step {
            WalkEvent::Enter(element) => {
                if element.kind().is_trivia() {
                    continue;
                }
                if !first {
                    out.push(' ');
                }
                match element {
                    NodeOrToken::Node(node) => {
                        let _ = write!(out, "{:?}[", node.kind());
                        first = true;
                    }
                    NodeOrToken::Token(token) => {
                        let _ = write!(out, "{:?}", token.text());
                        first = false;
                    }
                }
            }
            WalkEvent::Leave(NodeOrToken::Node(_)) => {
                out.push(']');
                first = false;
            }
            WalkEvent::Leave(NodeOrToken::Token(_)) => {}
        }
    }
    out
}
