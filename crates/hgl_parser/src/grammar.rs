//! The grammar: one function for each production of the specification.
//!
//! Every function here is a method on [`crate::parser::Parser`] and is
//! named after the production it parses, which its doc comment quotes in
//! the specification's notation. The modules follow the specification's
//! chapters:
//!
//! * `items`: files and declarations;
//! * `types`: types;
//! * `generics`: generic parameters and arguments, `requires` clauses;
//! * `expressions`: expressions;
//! * `statements`: blocks and statements.
//!
//! Three habits are common to all of them:
//!
//! * A function that parses an optional thing leaves the cursor where it
//!   was when the thing is absent, and reports nothing. Its caller decides
//!   whether absence is an error.
//! * A token whose kind has just been checked is taken with `bump`. A
//!   token the grammar needs next is taken with `eat` or `expect`, which
//!   reach across one line break and not across a blank line (LEX-5).
//!   Where what has been parsed could be all there is, the function asks
//!   `nl` before going on.
//! * After an error, parsing goes on with what is there. Later stages can
//!   rely on a node's kind, never on a node being complete.

mod expressions;
mod generics;
mod items;
mod statements;
mod types;

use crate::kind::SyntaxKind;

/// True for a token that can begin an expression.
pub(crate) fn can_start_expression(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::Int
            | SyntaxKind::Float
            | SyntaxKind::Str
            | SyntaxKind::Duration
            | SyntaxKind::Temporal
            | SyntaxKind::KwTrue
            | SyntaxKind::KwFalse
            | SyntaxKind::KwNull
            | SyntaxKind::Ident
            | SyntaxKind::KwStr
            | SyntaxKind::KwBytes
            | SyntaxKind::KwAny
            | SyntaxKind::LParen
            | SyntaxKind::LBracket
            | SyntaxKind::LBrace
            | SyntaxKind::KwFn
            | SyntaxKind::KwIf
            | SyntaxKind::KwEval
            | SyntaxKind::Minus
            | SyntaxKind::Bang
    )
}

/// True for a token that can begin a declaration or a statement.
///
/// LEX-5: after one line break, such a token begins a new declaration or
/// statement; any other token continues the line before. A name counts,
/// whatever it spells, since a statement can begin with a name. So do
/// `!`, which begins an expression, and `(`, `[` and `{`.
///
/// `-` does not, though it too begins an expression: "`-` at the start of
/// a line is the binary operator: a statement that began with it would
/// have no effect" (owner decision, 2026-11-14).
pub(crate) fn can_begin(kind: SyntaxKind) -> bool {
    (can_start_expression(kind) && kind != SyntaxKind::Minus)
        || matches!(
            kind,
            // Statements.
            SyntaxKind::KwLet
                | SyntaxKind::KwVar
                | SyntaxKind::KwReturn
                | SyntaxKind::KwState
                | SyntaxKind::KwCache
                | SyntaxKind::KwInject
                | SyntaxKind::KwStart
                | SyntaxKind::KwStop
                | SyntaxKind::KwWhen
                | SyntaxKind::KwWhile
                | SyntaxKind::KwFor
                | SyntaxKind::KwYield
                | SyntaxKind::KwAssert
                // Declarations.
                | SyntaxKind::KwModule
                | SyntaxKind::KwUse
                | SyntaxKind::KwExport
                | SyntaxKind::KwImpl
                | SyntaxKind::KwStruct
                | SyntaxKind::KwAbstract
                | SyntaxKind::KwEnum
                | SyntaxKind::KwOperator
                | SyntaxKind::KwInstantiate
                | SyntaxKind::KwTest
                | SyntaxKind::At
        )
}

/// True for a reserved word that names a scalar type.
pub(crate) fn is_scalar_type(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::KwBool
            | SyntaxKind::KwI64
            | SyntaxKind::KwF64
            | SyntaxKind::KwStr
            | SyntaxKind::KwBytes
            | SyntaxKind::KwAny
            | SyntaxKind::KwDate
            | SyntaxKind::KwTime
            | SyntaxKind::KwDatetime
            | SyntaxKind::KwDuration
            | SyntaxKind::KwCivilDatetime
            | SyntaxKind::KwTimezone
            | SyntaxKind::KwZonedDatetime
            | SyntaxKind::KwZonedTime
    )
}

/// True for a token that can begin a type.
pub(crate) fn can_start_type(kind: SyntaxKind) -> bool {
    kind == SyntaxKind::Ident || is_scalar_type(kind)
}
