//! Reads of the parser's syntax tree that the analyser's passes share.
//!
//! The parser's tree (`hgl_parser`) is lossless: whitespace and comments
//! are tokens in it too. These answer the small questions every pass
//! asks of a node: the tokens directly inside it, its first child of a
//! kind, the name it uses, and what it is inside its parentheses.

use hgl_parser::{SyntaxKind, SyntaxNode, SyntaxToken};

/// The first child node of `node` of kind `kind`.
pub(crate) fn child(node: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxNode> {
    node.children().find(|child| child.kind() == kind)
}

/// The tokens directly inside `node`, without whitespace and comments.
pub(crate) fn tokens(node: &SyntaxNode) -> impl Iterator<Item = SyntaxToken> {
    node.children_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
}

/// True when a token of `kind` is directly inside `node`.
pub(crate) fn has_token(node: &SyntaxNode, kind: SyntaxKind) -> bool {
    tokens(node).any(|token| token.kind() == kind)
}

/// The identifier a name uses: of `alias::name` the one after the `::`,
/// otherwise its first identifier. That is the token the resolver
/// recorded the use at.
pub(crate) fn name_token(node: &SyntaxNode) -> Option<SyntaxToken> {
    let identifiers = |node: &SyntaxNode| {
        tokens(node)
            .filter(|token| token.kind() == SyntaxKind::Ident)
            .collect::<Vec<_>>()
    };
    match child(node, SyntaxKind::QualifiedName) {
        Some(qualified) => identifiers(&qualified).into_iter().nth(1),
        None => identifiers(node).into_iter().next(),
    }
}

/// What `node` is with the parentheses around it taken off. EXPR-1:
/// "parentheses group", so `((a))` is `a`.
pub(crate) fn ungrouped(mut node: SyntaxNode) -> SyntaxNode {
    while node.kind() == SyntaxKind::ParenExpr {
        match node.children().next() {
            Some(inner) => node = inner,
            None => break,
        }
    }
    node
}
