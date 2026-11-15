//! Folds the constants written inside types.
//!
//! A list's size and a window's sizes are constant expressions: `3`,
//! `5m`, `N + 1`. To check them (TYPE-10, TYPE-11) their values are
//! needed before any expression has a type. This module works out the
//! ones that need no types: integer and duration literals, a minus before
//! one, `+`, `-` and `*` between two, parentheses, and the name of a
//! generic parameter.
//!
//! Everything else is [`ConstantValue::Unevaluated`]: a call, a name
//! that is no generic parameter, an operation that overflows. Nothing is
//! reported for those here. The pass that evaluates constants in full,
//! with types, reads them again and reports what is wrong with them.

use hgl_lexer::literal::{MAX_INTEGER_MAGNITUDE, decode_duration, decode_integer};
use hgl_parser::{SyntaxKind, SyntaxNode, SyntaxToken};

use crate::types::ConstantValue;

/// How deep an expression is folded. A size is a few tokens; one nested
/// deeper than this is left unevaluated, so that folding keeps little on
/// the call stack whatever it is given.
const DEPTH: u32 = 32;

/// The tokens directly inside `node`, without whitespace and comments.
fn tokens(node: &SyntaxNode) -> impl Iterator<Item = SyntaxToken> {
    node.children_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
}

/// What the constant expression `node` comes to.
///
/// `name` is asked what a name stands for: a generic parameter gives
/// [`ConstantValue::Parameter`], anything else
/// [`ConstantValue::Unevaluated`]. `&dyn Fn` is a reference to any
/// function or closure with that signature.
pub(crate) fn fold(
    node: &SyntaxNode,
    name: &dyn Fn(&SyntaxNode) -> ConstantValue,
) -> ConstantValue {
    fold_at(node, name, 0)
}

fn fold_at(
    node: &SyntaxNode,
    name: &dyn Fn(&SyntaxNode) -> ConstantValue,
    depth: u32,
) -> ConstantValue {
    if depth > DEPTH {
        return ConstantValue::Unevaluated;
    }
    match node.kind() {
        SyntaxKind::Literal => literal(node),
        SyntaxKind::NameExpr => name(node),
        SyntaxKind::ParenExpr => match node.children().next() {
            Some(inner) => fold_at(&inner, name, depth + 1),
            None => ConstantValue::Unevaluated,
        },
        SyntaxKind::UnaryExpr => unary(node, name, depth),
        SyntaxKind::BinaryExpr => binary(node, name, depth),
        _ => ConstantValue::Unevaluated,
    }
}

/// The magnitude of an integer literal, when `node` is one.
fn integer_magnitude(node: &SyntaxNode) -> Option<u64> {
    if node.kind() != SyntaxKind::Literal {
        return None;
    }
    let token = tokens(node).next()?;
    if token.kind() != SyntaxKind::Int {
        return None;
    }
    // A literal the lexer rejected has been reported there.
    decode_integer(token.text()).ok()
}

fn literal(node: &SyntaxNode) -> ConstantValue {
    let Some(token) = tokens(node).next() else {
        return ConstantValue::Unevaluated;
    };
    match token.kind() {
        // LEX-7: a literal has no sign, and its magnitude may be 2^63,
        // which is an `i64` only under a minus. `try_from` fails for
        // that one value, and `unary` reads it.
        SyntaxKind::Int => match decode_integer(token.text()).map(i64::try_from) {
            Ok(Ok(value)) => ConstantValue::Int(value),
            _ => ConstantValue::Unevaluated,
        },
        SyntaxKind::Duration => match decode_duration(token.text()) {
            Ok(microseconds) => ConstantValue::Duration(microseconds),
            Err(_) => ConstantValue::Unevaluated,
        },
        SyntaxKind::Float => ConstantValue::Other("a float"),
        SyntaxKind::Str => ConstantValue::Other("a string"),
        SyntaxKind::KwTrue | SyntaxKind::KwFalse => ConstantValue::Other("a bool"),
        SyntaxKind::KwNull => ConstantValue::Other("null"),
        SyntaxKind::Temporal => ConstantValue::Other("a date or time"),
        _ => ConstantValue::Unevaluated,
    }
}

fn unary(
    node: &SyntaxNode,
    name: &dyn Fn(&SyntaxNode) -> ConstantValue,
    depth: u32,
) -> ConstantValue {
    let minus = tokens(node).next().map(|token| token.kind()) == Some(SyntaxKind::Minus);
    // The operand is the last child node: configuration, when the
    // operator has some, stands before it.
    let (Some(operand), true) = (node.children().last(), minus) else {
        return ConstantValue::Unevaluated;
    };
    if node
        .children()
        .any(|child| child.kind() == SyntaxKind::OperatorConfig)
    {
        return ConstantValue::Unevaluated;
    }
    // LEX-7: "Minus is an operator, not part of a numeric literal", and
    // the full signed minimum is written as a minus before 2^63.
    if integer_magnitude(&operand) == Some(MAX_INTEGER_MAGNITUDE) {
        return ConstantValue::Int(i64::MIN);
    }
    match fold_at(&operand, name, depth + 1) {
        // `checked_neg` gives `None` when the result does not fit, which
        // for `i64` is the negation of its minimum.
        ConstantValue::Int(value) => value
            .checked_neg()
            .map_or(ConstantValue::Unevaluated, ConstantValue::Int),
        ConstantValue::Duration(value) => value
            .checked_neg()
            .map_or(ConstantValue::Unevaluated, ConstantValue::Duration),
        other @ ConstantValue::Other(_) => other,
        _ => ConstantValue::Unevaluated,
    }
}

fn binary(
    node: &SyntaxNode,
    name: &dyn Fn(&SyntaxNode) -> ConstantValue,
    depth: u32,
) -> ConstantValue {
    let mut operands = node.children();
    let (Some(left), Some(right), None) = (operands.next(), operands.next(), operands.next())
    else {
        // A third child node is the operator's configuration.
        return ConstantValue::Unevaluated;
    };
    let Some(operator) = tokens(node).next() else {
        return ConstantValue::Unevaluated;
    };
    let left = fold_at(&left, name, depth + 1);
    let right = fold_at(&right, name, depth + 1);
    // The `checked_` operations give `None` on overflow. What a constant
    // that overflows means is for the pass that evaluates constants.
    let value = match (operator.kind(), left, right) {
        (SyntaxKind::Plus, ConstantValue::Int(left), ConstantValue::Int(right)) => {
            left.checked_add(right).map(ConstantValue::Int)
        }
        (SyntaxKind::Minus, ConstantValue::Int(left), ConstantValue::Int(right)) => {
            left.checked_sub(right).map(ConstantValue::Int)
        }
        (SyntaxKind::Star, ConstantValue::Int(left), ConstantValue::Int(right)) => {
            left.checked_mul(right).map(ConstantValue::Int)
        }
        (SyntaxKind::Plus, ConstantValue::Duration(left), ConstantValue::Duration(right)) => {
            left.checked_add(right).map(ConstantValue::Duration)
        }
        (SyntaxKind::Minus, ConstantValue::Duration(left), ConstantValue::Duration(right)) => {
            left.checked_sub(right).map(ConstantValue::Duration)
        }
        _ => None,
    };
    value.unwrap_or(ConstantValue::Unevaluated)
}
