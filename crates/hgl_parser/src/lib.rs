//! Stage 2 of the compiler: parsing.
//!
//! ```text
//! (lexer) --> tokens --[parse]--> syntax tree --> (analyser, formatter)
//! ```
//!
//! [`parse`] turns the lexer's tokens into a syntax tree under the grammar
//! of the specification's language chapters. The tree is *lossless*: every
//! token is in it, whitespace and comments included, so its text is the
//! source text. The formatter works from the tree alone, and the analyser
//! reads spans from it.
//!
//! The parser is *total*: any token list gives a tree. Source the grammar
//! does not allow is reported to the diagnostics sink and parsing goes on;
//! the tokens that fit nowhere are kept in [`SyntaxKind::Error`] nodes.
//!
//! # How it is built
//!
//! A hand-written recursive-descent parser: one function for each
//! production of the grammar, each calling the functions for the
//! productions it mentions. The modules, in the order a token passes
//! through them:
//!
//! * `input` indexes the tokens the grammar can see, and notes where line
//!   breaks stand between them;
//! * `parser` is the machinery: the cursor, error reports, and trying a
//!   reading and taking it back;
//! * `grammar` is the grammar itself;
//! * `event` turns what the grammar recorded into the tree, placing
//!   whitespace and comments;
//! * `kind` and `tree` define the tree;
//! * [`ast`] gives typed views of it: a function's name, a file's imports.
//!
//! # What the parser does not decide
//!
//! The grammar is not context-free in two places, and the parser leaves
//! both to the analyser, which knows what each name means:
//!
//! * **`name<...>(...)` (GEN-14)** constructs a generic type when `name` is
//!   one, and is two comparisons otherwise. The parser reads the tokens
//!   both ways. When only one reading parses, that is the result. When
//!   both do, it guesses the constructor and marks the node
//!   [`SyntaxKind::UndecidedConstructor`] for the analyser to confirm.
//! * **A bare name as a generic argument**, as in `list<T, N>`, is read as
//!   a type. Whether it names a type or a constant is the analyser's to
//!   say.
//!
//! Rules about meaning are not checked here either: this stage accepts
//! `fn f(a: ...T = 1)` because it has the form of a parameter, and the
//! analyser rejects the default (FN-8).

pub mod ast;
mod event;
mod grammar;
mod input;
mod kind;
mod parser;
mod tree;

use hgl_diagnostics::Diagnostics;
use hgl_lexer::Token;
use hgl_source::{FileId, SourceMap};

pub use kind::SyntaxKind;
pub use tree::{
    HasSpan, HglLanguage, Parse, SyntaxElement, SyntaxNode, SyntaxToken, dump, outline,
};

/// The diagnostic codes this stage reports.
///
/// The specification's catalogue has one code for the grammar,
/// `syntax.expected_token`, and two for documentation comments. The
/// last names a limit of this compiler and is reported as a local code.
pub mod codes {
    /// A required token is missing, or a token or the end of the file
    /// stands where the grammar allows neither. Catalogued.
    pub const SYNTAX_EXPECTED_TOKEN: &str = "syntax.expected_token";
    /// A documentation comment that documents no declaration (SDOC-1).
    /// Catalogued.
    pub const DOC_UNATTACHED: &str = "doc.unattached";
    /// A documentation comment directly before another one (SDOC-1).
    /// Catalogued.
    pub const DOC_REPEATED: &str = "doc.repeated";
    /// Source nested more deeply than this compiler was asked to read
    /// ([`crate::Limits`]). Local: the language sets no limit.
    pub const NESTING_LIMIT: &str = "syntax.nesting_limit";
}

/// The compiler's limits on what the parser reads.
///
/// The language sets no limit on nesting. A parser must: each level of
/// nesting in the source is a level of nested function calls here, and in
/// any later stage that walks the tree, and the call stack is finite.
/// Source past a limit is reported (`syntax.nesting_limit`) and parsing
/// goes on; nothing is lost from the tree's text.
///
/// The defaults suit source written by hand with a wide margin. Raise them
/// for generated source that needs it, and give the thread that parses a
/// stack to match: [`Limits::stack_size`] says how much.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// How deep expressions, types, blocks and constraints may nest: the
    /// parser's own call depth. `((a))` nests two deep.
    pub nesting: usize,
    /// How deep the tree may be: the most nodes on any path from the root
    /// down. Nesting adds to it, and so does each link of a chain such as
    /// `a + b + c` or of an `else if` ladder, which nest in the tree
    /// though not in the source.
    pub tree_depth: usize,
}

impl Limits {
    /// The limits used when none are given: 100 levels of nesting, and a
    /// tree 500 nodes deep.
    pub const DEFAULT: Limits = Limits {
        nesting: 100,
        tree_depth: 500,
    };

    /// A stack size, in bytes, on which parsing within these limits, and
    /// freeing the tree, does not overflow.
    ///
    /// Measured in an unoptimised build, the parser uses about 1.5 KiB of
    /// stack for each level of nesting and freeing a tree about 0.4 KiB
    /// for each level of its depth. This allows ten times that, on top of
    /// 1 MiB.
    pub fn stack_size(self) -> usize {
        const KIB: usize = 1024;
        // `saturating_*` stops at the largest number instead of wrapping
        // round to a small one when the limits are absurdly large.
        (1024 * KIB)
            .saturating_add(self.nesting.saturating_mul(16 * KIB))
            .saturating_add(self.tree_depth.saturating_mul(4 * KIB))
    }
}

// `Default` is Rust's standard trait for "the usual value of this type":
// it makes `Limits::default()` work.
impl Default for Limits {
    fn default() -> Limits {
        Limits::DEFAULT
    }
}

/// What the tokens are parsed as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entry {
    /// A whole source file: a `module` line, then declarations. The root
    /// is a [`SyntaxKind::SourceFile`].
    SourceFile,
    /// Declarations with no `module` line, as in a fragment of a document.
    /// The root is a [`SyntaxKind::Fragment`].
    Declarations,
    /// Statements with no enclosing function, as in a fragment of a
    /// document. The root is a [`SyntaxKind::Fragment`].
    Statements,
}

/// Parses the tokens of one file.
///
/// `tokens` are the tokens [`hgl_lexer::lex`] gave for `file`, all of
/// them. Every problem found is reported to `diagnostics`; a tree comes
/// back whatever the tokens are.
///
/// `limits` bounds the nesting read and the depth of the tree;
/// [`Limits::default`] is the usual choice. With the default limits any
/// thread's stack is enough. With raised ones the caller provides a stack
/// of [`Limits::stack_size`] bytes.
pub fn parse(
    sources: &SourceMap,
    file: FileId,
    tokens: &[Token],
    entry: Entry,
    limits: Limits,
    diagnostics: &mut Diagnostics,
) -> Parse {
    let input = input::Input::new(sources.file(file), tokens);
    let mut parser = parser::Parser::new(&input, file, limits.nesting);
    match entry {
        Entry::SourceFile => parser.source_file(),
        Entry::Declarations => parser.declarations(),
        Entry::Statements => parser.statements(),
    }
    let (events, mut reports) = parser.finish_parse();
    let green = event::build(&input, &events, file, limits.tree_depth, &mut reports);
    for report in reports {
        diagnostics.emit(report);
    }
    Parse::new(green)
}
