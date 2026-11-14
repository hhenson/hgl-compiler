//! Finds what owns a rejection case (TST-12).
//!
//! > The enclosing named test/declaration/import owns the whole rejection
//! > case; reliable source boundaries are required. Unnamed context/module
//! > headers cannot own a case.
//!
//! So what can own one is an item of a file, or of an unnamed test
//! context: an import, a declaration, a named test. The `module` line and
//! the `test {` and `}` of a context cannot. An owner is found by its
//! lines, so an item that shares a line with one of those, or with
//! another item, has no boundary of its own and owns no case either.

use std::ops::RangeInclusive;

use hgl_parser::ast::{AstNode, Item, SourceFile};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode};
use hgl_source::{FileId, Span};

/// An item that can own a rejection case.
#[derive(Debug, Clone)]
pub(crate) struct Owner {
    /// Its file.
    pub file: FileId,
    /// The bytes its node covers: what the analyser is told to leave out.
    pub span: Span,
    /// The lines it covers, counted from 0. Items stand on lines of their
    /// own (LEX-5), so the lines of two owners do not overlap.
    pub lines: RangeInclusive<usize>,
    /// The line it starts on, after any documentation comment, counted
    /// from 0.
    pub start_line: usize,
    /// A few words for it: `fn invalid`, `use pricing.curves`.
    pub label: String,
    /// True when it is the last thing in its file: no import,
    /// declaration, test context or unplaced text follows it. Only such
    /// an item can be what the end of the file cut short.
    pub ends_file: bool,
    /// For a named test: its module's path and its name.
    pub test: Option<(String, String)>,
}

/// Every item of a file that can own a case, in source order.
pub(crate) fn owners(
    source: &hgl_source::SourceFile,
    file: FileId,
    root: &SyntaxNode,
) -> Vec<Owner> {
    let mut found = Vec::new();
    let Some(tree) = SourceFile::cast(root.clone()) else {
        return found;
    };
    let module = tree
        .module_decl()
        .and_then(|header| header.path())
        .map(|path| path.dotted())
        .unwrap_or_default();
    // The file's last node, whatever it is.
    let last_node = root.last_child();
    let mut pending: Vec<Item> = tree.items().collect();
    // Items are taken from the front; a context's items go back on the
    // front, so the result stays in source order.
    pending.reverse();
    // Lines that are not an item's to own (TST-12): those of a `module`
    // line, and those that hold the `test {` or the `}` of an unnamed
    // context.
    let mut reserved: Vec<usize> = Vec::new();
    for header in root
        .children()
        .filter(|node| node.kind() == SyntaxKind::ModuleDecl)
    {
        let span = header.span();
        let last = span.end.saturating_sub(1).max(span.start);
        reserved.extend(source.line_index(span.start)..=source.line_index(last));
    }
    while let Some(item) = pending.pop() {
        if let Item::TestContext(context) = &item {
            // The context cannot own a case; what it holds can.
            let braces = context
                .syntax()
                .children_with_tokens()
                .filter_map(|element| element.into_token())
                .filter(|token| matches!(token.kind(), SyntaxKind::LBrace | SyntaxKind::RBrace));
            for brace in braces {
                reserved.push(source.line_index(brace.span().start));
            }
            let mut inner: Vec<Item> = context.items().collect();
            inner.reverse();
            pending.extend(inner);
            continue;
        }
        let node = item.syntax();
        let span = node.span();
        // The node's last byte is the one before its end.
        let last = span.end.saturating_sub(1).max(span.start);
        let first_line = source.line_index(span.start);
        let lines = first_line..=source.line_index(last);
        // A documentation comment is the node's first token (SDOC-1);
        // the declaration starts at the first token that is not one.
        let start = node
            .descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| !token.kind().is_trivia())
            .map_or(span.start, |token| token.span().start);
        found.push(Owner {
            file,
            span,
            lines,
            start_line: source.line_index(start),
            label: label(&item),
            // A helper or test inside a context is not the file's last
            // node: the context is, and it stands between them and the
            // end of the file.
            ends_file: last_node.as_ref() == Some(node),
            test: match &item {
                Item::Test(test) => test
                    .name()
                    .map(|name| (module.clone(), name.text().to_string())),
                _ => None,
            },
        });
    }

    // TST-12: "reliable source boundaries are required." An owner is
    // found by its lines, so an item owns a case only when its lines are
    // its own. One that shares a line with another item, a `module`
    // line or a context's brace has no such boundary: an error on that
    // line could be either's. It owns nothing, and the line with it.
    //
    // The items are in source order, so one that shares a line shares
    // it with the item before it or the one after.
    let shared: Vec<bool> = (0..found.len())
        .map(|index| {
            let lines = &found[index].lines;
            let before = index
                .checked_sub(1)
                .is_some_and(|before| found[before].lines.end() >= lines.start());
            let after = found
                .get(index + 1)
                .is_some_and(|after| after.lines.start() <= lines.end());
            before || after || reserved.iter().any(|line| lines.contains(line))
        })
        .collect();
    let mut shared = shared.into_iter();
    // `retain` keeps the items for which the closure gives `true`; it
    // visits them in order, as `shared` was made.
    found.retain(|_| !shared.next().unwrap_or(false));
    found
}

/// A few words for an item: its keywords and its name.
fn label(item: &Item) -> String {
    // The name may be missing from a declaration the parser could not
    // read; the keywords stand alone then.
    let named = |keywords: &str, name: Option<hgl_parser::SyntaxToken>| match name {
        Some(name) => format!("{keywords} {}", name.text()),
        None => keywords.to_string(),
    };
    match item {
        Item::Use(import) => match import.path() {
            Some(path) => format!("use {}", path.dotted()),
            None => "use".to_string(),
        },
        Item::Function(function) => {
            let keywords = if function.is_impl() {
                "impl fn"
            } else if function.is_const() {
                "const fn"
            } else {
                "fn"
            };
            named(keywords, function.name())
        }
        Item::NativeFunction(function) => {
            let keywords = if function.is_const() {
                "native const fn"
            } else {
                "native fn"
            };
            named(keywords, function.name())
        }
        Item::Struct(decl) => named("struct", decl.name()),
        Item::Enum(decl) => named("enum", decl.name()),
        Item::Operator(decl) => named("operator", decl.name()),
        Item::NativeType(decl) => named("native type", decl.name()),
        Item::Test(decl) => named("test", decl.name()),
        Item::Instantiate(_) => "instantiate".to_string(),
        Item::TestContext(_) => "test context".to_string(),
    }
}

/// The position in `owners` of the owner of an error of the lexer or
/// parser whose primary range is `span`, in `file`, whose text is
/// `end_of_file` bytes long.
///
/// An annotation is matched to an error by line (TST-13), but a line is
/// too coarse to say whose an error is: text the parser could place
/// nowhere may stand on an item's line, after it, and is no part of the
/// item. So an error is an owner's only when it is *inside* the owner:
///
/// * it starts within the bytes of the owner's node; or
/// * it is an empty range at the node's end, which is how the parser
///   marks something missing after the item's last token; or
/// * it is at the end of the file, and the owner is the item that ends
///   the file. An error there is for whatever the end of the file cut
///   short. When an unnamed test context or unplaced text ends the file,
///   that is not an item, whichever item's line the file ends on; or
/// * it is after the owner, the owner ends the file, and the end of the
///   file cut it short (`cut_short`: the file has an error at its end).
///   Nothing but comments can stand there, and they are inside what the
///   owner left open: a comment that never closes, in a block that never
///   closes.
///
/// Any other error is the surviving source's.
pub(crate) fn owner_of_error(
    owners: &[Owner],
    file: FileId,
    span: Span,
    end_of_file: u32,
    cut_short: bool,
) -> Option<usize> {
    let mut in_file = owners
        .iter()
        .enumerate()
        .filter(|(_, owner)| owner.file == file);
    let found = if span.start >= end_of_file {
        in_file.find(|(_, owner)| owner.ends_file)
    } else {
        in_file.find(|(_, owner)| {
            let inside =
                span.start < owner.span.end || (span.is_empty() && span.start == owner.span.end);
            let in_what_it_left_open = owner.ends_file && cut_short;
            span.start >= owner.span.start && (inside || in_what_it_left_open)
        })
    };
    found.map(|(index, _)| index)
}

/// The position in `owners` of the owner of line `line` of `file`: the
/// item an annotation that applies to that line belongs to (TST-12).
///
/// That is the item whose lines include it. The last line of a file is
/// where an error at the end of the file is reported (TST-13), and an
/// item cut short by the end of the file need not reach that line. So
/// the item that ends the file owns it: the one nothing follows. When
/// what ends the file is an unnamed test context, or text the parser
/// could place nowhere, nothing owns the line.
pub(crate) fn owner_of(
    owners: &[Owner],
    file: FileId,
    line: usize,
    last_line: usize,
) -> Option<usize> {
    let in_file = || {
        owners
            .iter()
            .enumerate()
            .filter(move |(_, owner)| owner.file == file)
    };
    if let Some((index, _)) = in_file().find(|(_, owner)| owner.lines.contains(&line)) {
        return Some(index);
    }
    if line == last_line {
        return in_file()
            .find(|(_, owner)| owner.ends_file)
            .map(|(index, _)| index);
    }
    None
}
