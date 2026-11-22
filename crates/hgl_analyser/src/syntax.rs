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

/// The function name and the exclusion names written in `for f without key` (FN-8).
pub(crate) fn pack_for_names(
    node: &SyntaxNode,
) -> Option<(SyntaxToken, impl Iterator<Item = SyntaxToken>)> {
    let mut names = tokens(node).filter(|token| token.kind() == SyntaxKind::Ident);
    let function = names.next()?;
    // The next identifier is the contextual `without`, when present.
    Some((function, names.skip(1)))
}

/// The call of the pack key form `arg_keys(Ps)`, before checking its argument (GEN-17).
pub(crate) fn is_arg_keys_call(node: &SyntaxNode) -> bool {
    node.kind() == SyntaxKind::CallExpr
        && node.children().next().is_some_and(|callee| {
            callee.kind() == SyntaxKind::NameExpr
                && tokens(&callee)
                    .next()
                    .is_some_and(|name| name.text() == "arg_keys")
        })
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

/// The statements of a test's block. Leading helper declarations have their own bodies.
pub(crate) fn test_statements(test: &SyntaxNode) -> impl Iterator<Item = SyntaxNode> {
    child(test, SyntaxKind::Block)
        .into_iter()
        .flat_map(|block| block.children())
        .filter(|node| node.kind() != SyntaxKind::FnDecl)
}

/// True when the `inject` `node` stands among the first statements of
/// its test's block, before any that is no `inject` (TST-17).
pub(crate) fn leads_its_test(node: &SyntaxNode) -> bool {
    let in_block = node.parent().is_some_and(|block| {
        block.kind() == SyntaxKind::Block
            && block
                .parent()
                .is_some_and(|test| test.kind() == SyntaxKind::TestDecl)
    });
    let mut before = node.prev_sibling();
    while let Some(statement) = before {
        if !matches!(
            statement.kind(),
            SyntaxKind::InjectDecl | SyntaxKind::FnDecl
        ) {
            return false;
        }
        before = statement.prev_sibling();
    }
    in_block
}
