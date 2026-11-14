//! Checks the forms of declarations that the grammar admits and a rule
//! rejects.
//!
//! The parser reads one permissive form where the chapters describe
//! several: any parameter may have `const`, a pack and a default, and a
//! native contract body may hold any hooks. Which combinations mean
//! something is decided here, from the declaration alone: no name is
//! looked up and no type is known.

use hgl_diagnostics::{Category, Diagnostics, Location};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};
use hgl_source::FileId;

use crate::program::{DeclarationKind, Program, Role};
use crate::{codes, error};

/// Checks every declaration of the program.
pub(crate) fn check(program: &Program, diagnostics: &mut Diagnostics) {
    for module in &program.modules {
        for declaration in &module.declarations {
            let file = module.files[declaration.file].file;
            let node = &declaration.syntax;
            if matches!(
                node.kind(),
                SyntaxKind::FnDecl | SyntaxKind::OperatorDecl | SyntaxKind::NativeFnDecl
            ) {
                packs(node, file, diagnostics);
            }
            if let DeclarationKind::NativeImplementation(role) = declaration.kind {
                hooks(node, file, role, diagnostics);
            }
        }
    }
}

/// The tokens directly inside `node`, without whitespace and comments.
fn tokens(node: &SyntaxNode) -> impl Iterator<Item = SyntaxToken> {
    node.children_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
}

/// The first child node of `node` of kind `kind`.
fn child(node: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxNode> {
    node.children().find(|child| child.kind() == kind)
}

/// FN-8: "Packs have no defaults, cannot be const, and follow all fixed
/// parameters", and "`...{Fields}` captures the named ones ... where
/// `Fields` is a declared type pack."
fn packs(declaration: &SyntaxNode, file: FileId, diagnostics: &mut Diagnostics) {
    let at = |token: &SyntaxToken| Location::new(file, token.span());
    // The declaration's type packs: its generic parameters written
    // `...Name`.
    let type_packs: Vec<String> = child(declaration, SyntaxKind::GenericParams)
        .into_iter()
        .flat_map(|generics| generics.children())
        .filter(|parameter| tokens(parameter).any(|token| token.kind() == SyntaxKind::Ellipsis))
        .filter_map(|parameter| tokens(&parameter).find(|token| token.kind() == SyntaxKind::Ident))
        .map(|name| name.text().to_string())
        .collect();

    let parameters = child(declaration, SyntaxKind::ParamList)
        .into_iter()
        .flat_map(|list| list.children())
        .filter(|child| child.kind() == SyntaxKind::Param);
    // The `...` of the first pack met, once one has been.
    let mut pack_seen: Option<SyntaxToken> = None;
    for parameter in parameters {
        let ellipsis = tokens(&parameter).find(|token| token.kind() == SyntaxKind::Ellipsis);
        // The identifiers before the `:`: the name, after `const` when
        // the parameter has it (FN-5).
        let before_colon: Vec<SyntaxToken> = tokens(&parameter)
            .take_while(|token| token.kind() != SyntaxKind::Colon)
            .filter(|token| token.kind() == SyntaxKind::Ident)
            .collect();
        let Some(name) = before_colon.last() else {
            continue;
        };
        let Some(ellipsis) = ellipsis else {
            // A fixed parameter.
            if let Some(pack) = &pack_seen {
                diagnostics.emit(
                    error(
                        Category::Type,
                        codes::PACK_ORDER,
                        at(name),
                        format!("parameter `{}` follows a pack", name.text()),
                        "FN-8",
                    )
                    .with_related(at(pack), "the pack is here")
                    .with_note("packs follow all fixed parameters"),
                );
            }
            continue;
        };
        if before_colon.len() > 1 {
            diagnostics.emit(error(
                Category::Type,
                codes::PACK_CONST,
                at(&before_colon[0]),
                format!("pack `{}` cannot be const", name.text()),
                "FN-8",
            ));
        }
        if let Some(assign) = tokens(&parameter).find(|token| token.kind() == SyntaxKind::Eq) {
            diagnostics.emit(error(
                Category::Type,
                codes::PACK_DEFAULT,
                at(&assign),
                format!("pack `{}` cannot have a default", name.text()),
                "FN-8",
            ));
        }
        // `...{Fields}`: what is in the braces is a declared type pack,
        // by its bare name.
        if let Some(named) = child(&parameter, SyntaxKind::PackType) {
            let inner = named.children().next();
            let pack_name = inner
                .as_ref()
                .filter(|inner| {
                    inner.kind() == SyntaxKind::NamedType && inner.children().next().is_none()
                })
                .and_then(|inner| tokens(inner).find(|token| token.kind() == SyntaxKind::Ident))
                .filter(|token| type_packs.iter().any(|pack| pack == token.text()));
            if pack_name.is_none() {
                let place = match &inner {
                    Some(inner) => Location::new(file, inner.span()),
                    None => Location::new(file, named.span()),
                };
                diagnostics.emit(
                    error(
                        Category::Type,
                        codes::PACK_UNDECLARED,
                        place,
                        format!(
                            "named pack `{}` does not name a type pack of its declaration",
                            name.text()
                        ),
                        "FN-8",
                    )
                    .with_note("declare one among the generic parameters: `<...Fields>`"),
                );
            }
        }
        pack_seen.get_or_insert(ellipsis);
    }
}

/// NATIVE-3: "Start/stop hooks require when and are unique. A native
/// const implementation stays value-level and admits no temporal
/// lifecycle hooks."
fn hooks(implementation: &SyntaxNode, file: FileId, role: Role, diagnostics: &mut Diagnostics) {
    let Some(body) = child(implementation, SyntaxKind::NativeContractBody) else {
        return;
    };
    // Each hook's keyword: `start`, `when` or `stop`.
    let hooks: Vec<SyntaxToken> = body
        .children()
        .filter(|child| child.kind() == SyntaxKind::NativeHook)
        .filter_map(|hook| hook.first_token())
        .collect();
    let at = |token: &SyntaxToken| Location::new(file, token.span());

    if role == Role::Value {
        for hook in &hooks {
            diagnostics.emit(error(
                Category::Phase,
                codes::NATIVE_CONST_HOOK,
                at(hook),
                format!(
                    "a native const implementation has no `{}` hook",
                    hook.text()
                ),
                "NATIVE-3",
            ));
        }
        return;
    }

    let has_when = hooks.iter().any(|hook| hook.kind() == SyntaxKind::KwWhen);
    for kind in [SyntaxKind::KwStart, SyntaxKind::KwStop] {
        let mut of_kind = hooks.iter().filter(|hook| hook.kind() == kind);
        let Some(first) = of_kind.next() else {
            continue;
        };
        if !has_when {
            diagnostics.emit(
                error(
                    Category::Phase,
                    codes::NATIVE_HOOK_WITHOUT_WHEN,
                    at(first),
                    format!("`{}` needs a `when` hook", first.text()),
                    "NATIVE-3",
                )
                .with_note("an implementation with no `when;` denotes a graph, which has no hooks"),
            );
        }
        for later in of_kind {
            diagnostics.emit(
                error(
                    Category::Phase,
                    codes::NATIVE_DUPLICATE_HOOK,
                    at(later),
                    format!("a second `{}` hook", later.text()),
                    "NATIVE-3",
                )
                .with_related(at(first), "the first is here"),
            );
        }
    }
}
