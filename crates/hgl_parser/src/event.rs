//! Parse events, and the builder that turns them into a tree.
//!
//! The grammar code does not build the tree directly. It records a flat
//! list of [`Event`]s ("a node starts", "the next token", "the node ends"),
//! and [`build`] replays the list into `rowan`'s tree builder. Two things
//! follow from the split:
//!
//! * the parser can try a reading and take it back by cutting the list
//!   short: nothing has been built yet;
//! * whitespace and comments are placed in one spot, here, and no grammar
//!   function mentions them.
//!
//! Where trivia goes: a node starts at its first significant token and ends
//! at its last, so the whitespace and comments around a node belong to its
//! parent. The one exception is a documentation comment directly before a
//! declaration it documents (SDOC-1), which becomes the first token of that
//! declaration's node.

use std::collections::HashMap;

use hgl_diagnostics::{Category, Code, Diagnostic, Location};
use hgl_lexer::TokenKind;
use hgl_source::FileId;
use rowan::{Checkpoint, GreenNode, GreenNodeBuilder};

use crate::codes;
use crate::input::Input;
use crate::kind::SyntaxKind;

/// One step in building the tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Event {
    /// A node of this kind starts here.
    Start(SyntaxKind),
    /// The place the next element starts, kept so that a node may later be
    /// started there. The parser leaves one before an element when it
    /// cannot yet tell whether the element will be wrapped: `a` becomes the
    /// left side of `a + b` only once the `+` is seen.
    Mark,
    /// A node of this kind starts at the [`Event::Mark`] with this index in
    /// the event list, taking in everything built since.
    StartAt(usize, SyntaxKind),
    /// The next significant token.
    Token,
    /// The innermost open node ends here.
    Finish,
}

/// The declarations a documentation comment can document (SDOC-1): "a
/// module, function/native function, operator, struct, field, enum, enum
/// member, native type, or test".
fn documentable(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::ModuleDecl
            | SyntaxKind::FnDecl
            | SyntaxKind::NativeFnDecl
            | SyntaxKind::OperatorDecl
            | SyntaxKind::StructDecl
            | SyntaxKind::StructMember
            | SyntaxKind::EnumDecl
            | SyntaxKind::EnumMember
            | SyntaxKind::NativeTypeDecl
            | SyntaxKind::TestDecl
    )
}

/// What became of one documentation comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DocState {
    /// It documents the declaration that follows it.
    Attached,
    /// It stands directly before another documentation comment.
    Repeated,
}

/// Replays `events` over `input` into a tree.
///
/// Every token of the input ends up in the tree, in order, whatever the
/// events say: tokens the events never reach are put in the root. The
/// documentation comments that document nothing are reported into
/// `reports` (SDOC-1).
///
/// The tree is at most `max_depth` nodes deep
/// ([`crate::Limits::tree_depth`]). The parser's own nesting limit bounds
/// most of a tree's depth. What it cannot bound is a chain such as
/// `a + b + c + ...` or a ladder of `else if`, where each link adds a
/// level without the parser going any deeper. A node that would be too
/// deep is left out, its tokens kept, and the first such is reported.
pub(crate) fn build(
    input: &Input<'_>,
    events: &[Event],
    file: FileId,
    max_depth: usize,
    reports: &mut Vec<Diagnostic>,
) -> GreenNode {
    let mut tree = TreeBuilder {
        input,
        builder: GreenNodeBuilder::new(),
        raw: 0,
        significant: 0,
        docs: HashMap::new(),
        heights: Vec::new(),
        parents: Vec::new(),
        max_depth,
        too_deep: None,
    };
    // `marks[i]` is the place of the `Mark` at event `i`.
    let mut marks: Vec<Option<Place>> = vec![None; events.len()];
    // Marks whose place is not yet known. A mark stands for "where the next
    // element starts", and that is known only once the trivia before that
    // element has been placed, so a mark waits for the event after it.
    let mut pending: Vec<usize> = Vec::new();
    // One entry for each node the events have started and not finished:
    // true when the node is in the tree, false when it was left out for
    // depth.
    let mut open: Vec<bool> = Vec::new();
    for (index, event) in events.iter().enumerate() {
        match *event {
            Event::Start(kind) => {
                // The root starts before anything, leading trivia included.
                if !open.is_empty() {
                    tree.trivia_before_node(kind);
                }
                tree.place(&mut pending, &mut marks);
                // The root is always built: a tree has one.
                let fits = open.is_empty() || tree.parents.len() < max_depth;
                if fits {
                    tree.parents.push(tree.heights.len());
                    tree.builder.start_node(rowan::SyntaxKind(kind.to_raw()));
                } else {
                    tree.note_too_deep();
                }
                open.push(fits);
            }
            Event::Mark => pending.push(index),
            Event::StartAt(mark, kind) => {
                // A mark with nothing after it wraps nothing, here.
                tree.place(&mut pending, &mut marks);
                let place = marks[mark].expect("StartAt names an earlier Mark");
                // Wrapping pushes everything wrapped one level down. What a
                // mark has once been too deep to wrap, it stays too deep
                // for: nothing is ever taken out again. Remembering that
                // saves measuring a long chain once for each link.
                let fits = !place.too_deep && {
                    let wrapped = tree.heights[place.child..].iter().max().copied();
                    tree.parents.len() + wrapped.unwrap_or(0) < max_depth
                };
                if fits {
                    tree.parents.push(place.child);
                    tree.builder
                        .start_node_at(place.checkpoint, rowan::SyntaxKind(kind.to_raw()));
                } else {
                    marks[mark] = Some(Place {
                        too_deep: true,
                        ..place
                    });
                    tree.note_too_deep();
                }
                open.push(fits);
            }
            Event::Token => {
                tree.trivia_up_to(tree.next_significant_raw());
                tree.place(&mut pending, &mut marks);
                tree.token();
                tree.significant += 1;
            }
            Event::Finish => {
                // A mark left inside this node stays inside it.
                tree.place(&mut pending, &mut marks);
                let in_tree = open.pop().expect("Finish closes a started node");
                if open.is_empty() {
                    // The root takes whatever is left, so the tree's text
                    // is the source text unconditionally.
                    tree.trivia_up_to(input.raw_tokens().len());
                }
                if in_tree {
                    tree.finish_node();
                }
            }
        }
    }
    tree.report_depth(file, reports);
    tree.report_documentation(file, reports);
    tree.builder.finish()
}

/// Where a [`Event::Mark`] stands in the node open at the time.
#[derive(Clone, Copy)]
struct Place {
    /// The tree library's handle for the place.
    checkpoint: Checkpoint,
    /// The same place as an index into [`TreeBuilder::heights`].
    child: usize,
    /// True once a node could not be started here for depth.
    too_deep: bool,
}

/// The state of one [`build`] call.
struct TreeBuilder<'a> {
    input: &'a Input<'a>,
    builder: GreenNodeBuilder<'static>,
    /// Index, in the lexer's list, of the next token not yet in the tree.
    raw: usize,
    /// Index of the next significant token not yet in the tree.
    significant: usize,
    /// The documentation comments accounted for, by index in the lexer's
    /// list. One not listed here is unattached.
    docs: HashMap<usize, DocState>,
    /// The height of each finished child of the nodes still open, in
    /// order: 0 for a token, and for a node one more than its tallest
    /// child. The children of an open node are the entries from its index
    /// in `parents` onwards.
    heights: Vec<usize>,
    /// For each open node, the index in `heights` of its first child. Its
    /// length is the depth of the node being built.
    parents: Vec<usize>,
    /// The most nodes the tree may have on a path from the root down.
    max_depth: usize,
    /// Index of the significant token at which a node was first left out
    /// for depth.
    too_deep: Option<usize>,
}

impl TreeBuilder<'_> {
    /// Index, in the lexer's list, of the next significant token.
    fn next_significant_raw(&self) -> usize {
        self.input.raw_index(self.significant)
    }

    /// Gives every waiting mark the current place in the open node.
    fn place(&mut self, pending: &mut Vec<usize>, marks: &mut [Option<Place>]) {
        // `drain(..)` empties the list while handing out its items.
        for mark in pending.drain(..) {
            marks[mark] = Some(Place {
                checkpoint: self.builder.checkpoint(),
                child: self.heights.len(),
                too_deep: false,
            });
        }
    }

    /// Closes the open node, replacing its children's heights with its own.
    fn finish_node(&mut self) {
        let first = self.parents.pop().expect("a node is open");
        let tallest = self.heights[first..].iter().max().copied().unwrap_or(0);
        self.heights.truncate(first);
        self.heights.push(tallest + 1);
        self.builder.finish_node();
    }

    /// Records that a node was left out for depth. Only the first is
    /// reported: one chain can run to thousands.
    fn note_too_deep(&mut self) {
        if self.too_deep.is_none() {
            self.too_deep = Some(self.significant);
        }
    }

    /// Reports the node left out for depth, when there was one.
    fn report_depth(&self, file: FileId, reports: &mut Vec<Diagnostic>) {
        let Some(token) = self.too_deep else {
            return;
        };
        // Point at the last token placed, which is inside the construct.
        let span = self.input.span(token.saturating_sub(1));
        reports.push(Diagnostic::error(
            Category::Parse,
            Code::Local(codes::NESTING_LIMIT),
            Location::new(file, span),
            format!(
                "a syntax tree deeper than {} levels is not built",
                self.max_depth
            ),
        ));
    }

    /// Adds the next token of the lexer's list to the open node.
    fn token(&mut self) {
        let Some(token) = self.input.raw_tokens().get(self.raw) else {
            return;
        };
        let kind = SyntaxKind::from(token.kind);
        self.builder.token(
            rowan::SyntaxKind(kind.to_raw()),
            self.input.raw_text(self.raw),
        );
        self.heights.push(0);
        self.raw += 1;
    }

    /// Adds tokens to the open node up to, not including, token `end` of
    /// the lexer's list.
    fn trivia_up_to(&mut self, end: usize) {
        while self.raw < end {
            self.token();
        }
    }

    /// Places the trivia before a node of `kind` that is about to start.
    ///
    /// All of it goes to the parent, except that a documentation comment
    /// with only whitespace between it and a documentable declaration is
    /// left for the declaration's node (SDOC-1: "Only whitespace may
    /// intervene").
    fn trivia_before_node(&mut self, kind: SyntaxKind) {
        let end = self.next_significant_raw();
        let tokens = self.input.raw_tokens();
        let mut keep_from = end;
        if documentable(kind) {
            // Walk back over whitespace from the declaration's first token.
            let mut index = end;
            while index > self.raw && is_blank(tokens[index - 1].kind) {
                index -= 1;
            }
            if index > self.raw && tokens[index - 1].kind == TokenKind::DocComment {
                keep_from = index - 1;
                self.docs.insert(keep_from, DocState::Attached);
                // A documentation comment directly before that one
                // documents nothing: the declaration has its comment.
                let mut earlier = keep_from;
                loop {
                    while earlier > self.raw && is_blank(tokens[earlier - 1].kind) {
                        earlier -= 1;
                    }
                    if earlier > self.raw && tokens[earlier - 1].kind == TokenKind::DocComment {
                        earlier -= 1;
                        self.docs.insert(earlier, DocState::Repeated);
                    } else {
                        break;
                    }
                }
            }
        }
        self.trivia_up_to(keep_from);
    }

    /// Reports each documentation comment that documents nothing (SDOC-1).
    fn report_documentation(&self, file: FileId, reports: &mut Vec<Diagnostic>) {
        for (raw, token) in self.input.raw_tokens().iter().enumerate() {
            if token.kind != TokenKind::DocComment {
                continue;
            }
            let location = Location::new(file, token.span);
            // `get` looks the index up; `copied` turns the borrowed state
            // it finds into a state.
            let diagnostic = match self.docs.get(&raw).copied() {
                Some(DocState::Attached) => continue,
                Some(DocState::Repeated) => Diagnostic::error(
                    Category::Parse,
                    Code::Local(codes::DOC_REPEATED),
                    location,
                    "a declaration has one documentation comment",
                ),
                None => Diagnostic::error(
                    Category::Parse,
                    Code::Local(codes::DOC_UNATTACHED),
                    location,
                    "documentation comment documents no declaration",
                )
                .with_note(
                    "`/** ... */` stands directly before a module, function, operator, \
                     struct, field, enum, enum member, native type or test",
                ),
            };
            reports.push(diagnostic.with_rule("SDOC-1"));
        }
    }
}

/// True for whitespace and line breaks.
fn is_blank(kind: TokenKind) -> bool {
    matches!(kind, TokenKind::Whitespace | TokenKind::Newline)
}
