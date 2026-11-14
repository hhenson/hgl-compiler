//! Finds what owns a rejection case (TST-12).
//!
//! > The enclosing named test/declaration/import owns the whole rejection
//! > case; reliable source boundaries are required. Unnamed context/module
//! > headers cannot own a case.
//!
//! So what can own one is an item of a file, or of an unnamed test
//! context: an import, a declaration, a named test. The `module` line and
//! the `test {` and `}` of a context cannot.

use std::ops::RangeInclusive;

use hgl_parser::ast::{AstNode, Item, SourceFile};
use hgl_parser::{HasSpan, SyntaxNode};
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
    while let Some(item) = pending.pop() {
        if let Item::TestContext(context) = &item {
            // The context cannot own a case; what it holds can.
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
            lines: first_line..=source.line_index(last),
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

/// The position in `owners` of the owner of line `line` of `file`.
///
/// That is the item whose lines include it. The last line of a file is
/// where an error at the end of the file is reported (TST-13), and an
/// item cut short by the end of the file need not reach that line. So
/// the item that ends the file owns it: the one nothing follows. When
/// what ends the file is an unnamed test context, or text the parser
/// could place nowhere, nothing owns the line, and an error there is
/// one of the surviving source's.
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
