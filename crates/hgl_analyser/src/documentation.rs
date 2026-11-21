//! Checks what a documentation comment's structured sections name
//! (SDOC-3), once `read` has read each declaration's signature.
//!
//! The parser attaches each `/** ... */` to the declaration after it
//! (SDOC-1): the comment is that declaration's first token. This pass
//! reads its text as SDOC-2 lays it out, finds the sections SDOC-3
//! names, and checks that each key of an `Args:` or a `Type Args:`
//! section names a parameter or a generic parameter of the declaration.
//!
//! SDOC-6: "Documentation-only changes do not affect execution,
//! overload resolution, or provider semantics." Nothing here is recorded
//! in the program: what this pass reads of a comment can reject the
//! source, and changes nothing that a later pass or stage reads.

use hgl_diagnostics::{Category, Diagnostics, Location};
use hgl_parser::ast::AstNode;
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};
use hgl_source::{FileId, Span};

use crate::program::Program;
use crate::types::Detail;
use crate::{codes, error};

/// The section headings SDOC-3's table lists. "Unknown headings remain
/// prose."
const HEADINGS: [&str; 10] = [
    "Args:",
    "Type Args:",
    "Returns:",
    "Raises:",
    "Ticks:",
    "Validity:",
    "Properties:",
    "Requires:",
    "Notes:",
    "Examples:",
];

/// The names a declaration's documentation may key its sections by.
/// `None` where this pass does not check that section.
struct Keys<'a> {
    /// Its parameters, for `Args:`.
    parameters: Option<Vec<&'a str>>,
    /// Its generic parameters, for `Type Args:`.
    generics: Option<Vec<&'a str>>,
}

impl Keys<'_> {
    /// A declaration that has neither parameters nor generic
    /// parameters: a module, an enum and its members, a struct's field,
    /// a native type, a constant, a test.
    fn none() -> Keys<'static> {
        Keys {
            parameters: Some(Vec::new()),
            generics: Some(Vec::new()),
        }
    }
}

/// Checks the documentation of every declaration of the program.
pub(crate) fn check(program: &Program, diagnostics: &mut Diagnostics) {
    for module in &program.modules {
        // The `module` line of each file.
        for file in &module.files {
            let root = file.syntax.syntax();
            if let Some(line) = root
                .children()
                .find(|node| node.kind() == SyntaxKind::ModuleDecl)
            {
                documented(&line, file.file, &Keys::none(), diagnostics);
            }
        }
        for (index, declaration) in module.declarations.iter().enumerate() {
            let file = module.files[declaration.file].file;
            let keys = match module.details.get(index) {
                Some(Detail::Callable(signature)) => Keys {
                    parameters: Some(
                        signature
                            .parameters
                            .iter()
                            .map(|parameter| parameter.name.as_str())
                            .collect(),
                    ),
                    generics: Some(
                        signature
                            .generics
                            .iter()
                            .map(|generic| generic.name.as_str())
                            .collect(),
                    ),
                },
                // A struct has fields and no parameters, and SDOC-3
                // does not say whether `Args:` may key its fields,
                // which its construction is given by name (TYPE-5):
                // that section is not checked.
                Some(Detail::Struct(fields)) => Keys {
                    parameters: None,
                    generics: Some(
                        fields
                            .generics
                            .iter()
                            .map(|generic| generic.name.as_str())
                            .collect(),
                    ),
                },
                Some(Detail::Enum(_) | Detail::Native(_) | Detail::Constant(_)) => Keys::none(),
                // A signature that could not be read says nothing of
                // what its documentation names (ERR-9).
                _ => Keys {
                    parameters: None,
                    generics: None,
                },
            };
            documented(&declaration.syntax, file, &keys, diagnostics);
            // A struct's fields and an enum's members (SDOC-1).
            for member in declaration.syntax.descendants().filter(|node| {
                matches!(
                    node.kind(),
                    SyntaxKind::StructMember | SyntaxKind::EnumMember
                )
            }) {
                documented(&member, file, &Keys::none(), diagnostics);
            }
        }
        for test in &module.tests {
            let file = module.files[test.file].file;
            documented(&test.syntax, file, &Keys::none(), diagnostics);
        }
    }
}

/// Checks the documentation comment of `node`, when it has one, against
/// what `keys` says the declaration has.
fn documented(node: &SyntaxNode, file: FileId, keys: &Keys, diagnostics: &mut Diagnostics) {
    // SDOC-1: the comment is the declaration's first token.
    let Some(comment) = node
        .children_with_tokens()
        .next()
        .and_then(|element| element.into_token())
        .filter(|token| token.kind() == SyntaxKind::DocComment)
    else {
        return;
    };
    for (heading, key, at) in structured_keys(&comment) {
        let (names, what) = match heading {
            "Args:" => (&keys.parameters, "parameter"),
            "Type Args:" => (&keys.generics, "generic parameter"),
            // SDOC-3 has `Requires:` and `Properties:` keyed "by
            // declared contracts", and does not say how a key names a
            // requirement or a property domain: they are not checked.
            _ => continue,
        };
        let Some(names) = names else {
            continue;
        };
        if names.contains(&key.as_str()) {
            continue;
        }
        diagnostics.emit(
            error(
                Category::Name,
                codes::DOC_UNKNOWN_KEY,
                Location::new(file, at),
                format!("`{heading}` names `{key}`, and what it documents has no {what} `{key}`"),
                "SDOC-3",
            )
            .with_note(
                "a structured key names a real parameter, generic parameter, requirement or \
                 property domain of the declaration",
            ),
        );
    }
}

/// The keys of the sections of a documentation comment, each with the
/// heading of its section and the bytes it covers in the file.
///
/// SDOC-2: "Remove delimiters, outer blank lines, and common
/// indentation ... A same-line summary does not set subsequent
/// indentation." SDOC-3: "Section headings start at column zero after
/// common-indent removal; contents are indented four spaces." A key is
/// a name and a `:` at the start of a line of contents; what follows is
/// its description, which may be missing ("Missing descriptions are
/// permitted"). A line deeper than four spaces continues a description.
fn structured_keys(comment: &SyntaxToken) -> Vec<(&'static str, String, Span)> {
    let text = comment.text();
    let start = comment.span().start;
    // The delimiters: `/**` and, where the comment is closed, `*/`.
    let inner_end = if text.len() >= 5 && text.ends_with("*/") {
        text.len() - 2
    } else {
        text.len()
    };
    let inner = text.get(3..inner_end).unwrap_or("");
    // Each line with its offset in the file. LEX-15: `\r\n`, `\r` and
    // `\n` each end a line.
    let mut lines: Vec<(u32, &str)> = Vec::new();
    let mut from = 0;
    let bytes = inner.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'\n' => {
                lines.push((from as u32, &inner[from..at]));
                at += 1;
                from = at;
            }
            b'\r' => {
                lines.push((from as u32, &inner[from..at]));
                at += if bytes.get(at + 1) == Some(&b'\n') {
                    2
                } else {
                    1
                };
                from = at;
            }
            _ => at += 1,
        }
    }
    lines.push((from as u32, &inner[from..]));
    // The first line is the one `/**` stands on: a summary written
    // there sets no indentation and is no heading.
    let body = &lines[1..];
    let indent = |line: &str| line.len() - line.trim_start_matches([' ', '\t']).len();
    let common = body
        .iter()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(_, line)| indent(line))
        .min()
        .unwrap_or(0);
    let mut keys = Vec::new();
    let mut section: Option<&'static str> = None;
    for &(offset, line) in body {
        if line.trim().is_empty() {
            continue;
        }
        let column = indent(line) - common;
        let content = &line[indent(line)..];
        if column == 0 {
            // A line at column zero is a heading when it is one SDOC-3
            // lists, and otherwise prose, which ends the section.
            section = HEADINGS
                .iter()
                .copied()
                .find(|heading| content.trim_end() == *heading);
            continue;
        }
        let Some(heading) = section else {
            continue;
        };
        if column != 4 {
            continue;
        }
        // The name: a letter or `_`, then letters, digits and `_`
        // (LEX-9), up to its `:`.
        let length = content
            .find(|letter: char| !(letter.is_alphanumeric() || letter == '_'))
            .unwrap_or(content.len());
        let named = content
            .chars()
            .next()
            .is_some_and(|first| first.is_alphabetic() || first == '_');
        if !named || !content[length..].starts_with(':') {
            continue;
        }
        let key_start = start + 3 + offset + indent(line) as u32;
        keys.push((
            heading,
            content[..length].to_string(),
            Span::new(key_start, key_start + length as u32),
        ));
    }
    keys
}
