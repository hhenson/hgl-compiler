//! Stage 1 of the compiler: lexing.
//!
//! ```text
//! source text --[lex]--> tokens --> (parser)
//! ```
//!
//! [`lex`] cuts a source file into [`Token`]s under the rules of the
//! specification's source-text chapter (`LEX-1` to `LEX-14`). The
//! [`literal`] module turns literal tokens into values.
//!
//! # What the lexer does not decide
//!
//! Lexing is context-free: the same text always gives the same tokens. The
//! specification has four lexical rules that depend on context. Each is left
//! to the stage that has the context, and the lexer keeps the information
//! that stage needs:
//!
//! * **Significant line breaks (LEX-5, LEX-6).** Whether a line break
//!   ends a statement depends on enclosing brackets and neighbouring
//!   tokens. Every line break (LF, CRLF or CR) is one
//!   [`TokenKind::Newline`] token; the parser decides which matter.
//! * **Contextual names (LEX-9, LEX-10).** `const`, `list`, `switch` and
//!   the like are special only in certain positions. They are
//!   [`TokenKind::Ident`] tokens; the parser checks their text.
//! * **The sign of a number (LEX-7).** `-5` is two tokens. Whether a
//!   literal such as 2^63 is in range depends on a preceding minus, so the
//!   analyser makes the final range check.
//! * **Documentation attachment (SDOC-1).** `/** ... */` is a
//!   [`TokenKind::DocComment`] token; the parser attaches it to a
//!   declaration.

pub mod literal;

mod lexer;
mod token;

pub use lexer::{codes, lex};
pub use token::{Token, TokenKind};
