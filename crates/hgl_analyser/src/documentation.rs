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
use hgl_parser::documentation::{Comment, attached};
use hgl_parser::{SyntaxKind, SyntaxNode};
use hgl_source::FileId;

use crate::program::Program;
use crate::types::Detail;
use crate::{codes, error};

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
    /// parameters: a module, an enum and its members, a struct field,
    /// a native type and its properties, a constant, a test.
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
            // A struct field, enum member or native property (SDOC-1).
            for member in declaration.syntax.descendants().filter(|node| {
                matches!(
                    node.kind(),
                    SyntaxKind::StructMember | SyntaxKind::EnumMember | SyntaxKind::NativeProperty
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
    let Some(comment) = attached(node) else {
        return;
    };
    for section in Comment::read(&comment).sections() {
        let (heading, names, what) = match section.heading() {
            Some(heading @ "Args:") => (heading, &keys.parameters, "parameter"),
            Some(heading @ "Type Args:") => (heading, &keys.generics, "generic parameter"),
            // The spelling of requirement and property keys is unspecified.
            _ => continue,
        };
        let Some(names) = names else {
            continue;
        };
        for entry in section.entries() {
            let key = entry.name;
            if names.contains(&key) {
                continue;
            }
            diagnostics.emit(
                error(
                    Category::Name,
                    codes::DOC_UNKNOWN_KEY,
                    Location::new(file, entry.span),
                    format!(
                        "`{heading}` names `{key}`, and what it documents has no {what} `{key}`"
                    ),
                    "SDOC-3",
                )
                .with_note(
                    "a structured key names a real parameter, generic parameter, requirement or \
                     property domain of the declaration",
                ),
            );
        }
    }
}
