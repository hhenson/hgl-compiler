//! Reads of the parser's syntax tree that the analyser's passes share.
//!
//! The parser's tree (`hgl_parser`) is lossless: whitespace and comments
//! are tokens in it too. These answer the small questions every pass
//! asks of a node: the tokens directly inside it, its first child of a
//! kind, the name it uses, and what it is inside its parentheses.

use hgl_lexer::literal::decode_string;
use hgl_parser::{SyntaxKind, SyntaxNode, SyntaxToken};

/// The first child node of `node` of kind `kind`.
pub(crate) fn child(node: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxNode> {
    node.children().find(|child| child.kind() == kind)
}

/// Branches checked only when a pack-key condition selects them (CTRL-16).
pub(crate) fn pack_key_branches(root: &SyntaxNode) -> impl Iterator<Item = SyntaxNode> {
    root.descendants()
        .filter(is_pack_key_if)
        .flat_map(|node| node.children().skip(1))
}

/// A conditional whose guard directly tests membership in a pack (CTRL-16).
pub(crate) fn is_pack_key_if(node: &SyntaxNode) -> bool {
    node.kind() == SyntaxKind::IfExpr
        && node
            .children()
            .next()
            .is_some_and(|guard| ungrouped(guard).kind() == SyntaxKind::PackKeyMembership)
}

/// EXPR-9, TYPE-5: only one unquoted word can also be a field label.
pub(crate) fn keyed_field_name(key: &SyntaxNode) -> Option<SyntaxToken> {
    if key.children().next().is_some() {
        return None;
    }
    let token = tokens(key).next()?;
    (key.kind() == SyntaxKind::NameExpr
        || matches!(
            token.kind(),
            SyntaxKind::KwTrue | SyntaxKind::KwFalse | SyntaxKind::KwNull
        ))
    .then_some(token)
}

pub(crate) fn is_keyed_field_name(key: &SyntaxNode) -> bool {
    let Some(pair) = key.parent() else {
        return false;
    };
    pair.kind() == SyntaxKind::SequencePair
        && pair.children().next().as_ref() == Some(key)
        && pair
            .parent()
            .is_some_and(|literal| literal.kind() == SyntaxKind::KeyedExpr)
        && keyed_field_name(key).is_some()
}

/// The tokens directly inside `node`, without whitespace and comments.
pub(crate) fn tokens(node: &SyntaxNode) -> impl Iterator<Item = SyntaxToken> {
    node.children_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
}

/// Whether a declaration acknowledges a warning by its code (ERR-10).
pub(crate) fn suppresses_warning(node: &SyntaxNode, code: &str) -> bool {
    node.children()
        .filter(|annotation| annotation.kind() == SyntaxKind::WarningAnnotation)
        .filter_map(|annotation| tokens(&annotation).find(|token| token.kind() == SyntaxKind::Str))
        .any(|string| decode_string(string.text()).is_ok_and(|value| value == code))
}

/// True when a token of `kind` is directly inside `node`.
pub(crate) fn has_token(node: &SyntaxNode, kind: SyntaxKind) -> bool {
    tokens(node).any(|token| token.kind() == kind)
}

/// The bare left name of a less-than expression (GEN-14).
pub(crate) fn name_before_less_than(node: &SyntaxNode) -> Option<SyntaxNode> {
    if node.kind() != SyntaxKind::BinaryExpr || !has_token(node, SyntaxKind::Lt) {
        return None;
    }
    node.children()
        .next()
        .filter(|left| left.kind() == SyntaxKind::NameExpr)
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

/// The two bare names of a pack member in either parser representation (GEN-17).
pub(crate) fn pack_member_names(node: &SyntaxNode) -> Option<(SyntaxToken, SyntaxToken)> {
    if node.kind() == SyntaxKind::PackMemberType {
        let mut identifiers = tokens(node).filter(|token| token.kind() == SyntaxKind::Ident);
        return Some((identifiers.next()?, identifiers.next()?));
    }
    let mut operands = node.children();
    let bare_name = |operand: SyntaxNode| {
        if operand.kind() == SyntaxKind::NameExpr && operand.children().next().is_none() {
            name_token(&operand)
        } else {
            None
        }
    };
    Some((bare_name(operands.next()?)?, bare_name(operands.next()?)?))
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

/// The first enclosing node that does more than group this expression (EXPR-1).
pub(crate) fn parent_outside_grouping(node: &SyntaxNode) -> Option<SyntaxNode> {
    node.ancestors()
        .skip(1)
        .find(|parent| parent.kind() != SyntaxKind::ParenExpr)
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

/// TST-1's test-local declarations use the module's declaration syntax.
pub(crate) fn is_test_declaration(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::FnDecl
            | SyntaxKind::ConstantDecl
            | SyntaxKind::StructDecl
            | SyntaxKind::EnumDecl
            | SyntaxKind::OperatorDecl
    )
}

/// The statements of a test's block. Its declarations are checked separately.
pub(crate) fn test_statements(test: &SyntaxNode) -> impl Iterator<Item = SyntaxNode> {
    child(test, SyntaxKind::Block)
        .into_iter()
        .flat_map(|block| block.children())
        .filter(|node| !is_test_declaration(node.kind()))
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
        if statement.kind() != SyntaxKind::InjectDecl && !is_test_declaration(statement.kind()) {
            return false;
        }
        before = statement.prev_sibling();
    }
    in_block
}
