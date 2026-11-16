//! Says what the body of each temporal function describes: composition
//! or runtime (FN-2).
//!
//! > An ordinary temporal body with no node-only construct describes
//! > composition: the function is a graph. `state`, `cache`, `inject`,
//! > `start`, `when`, `stop`, or `yield` classifies that complete body
//! > as runtime: the function is a node. Classification precedes phase
//! > checking, even if the trigger occurs in an invalid nested position.
//!
//! So [`classify`] looks only at which constructs a body holds,
//! wherever it holds them, and reports nothing.
//!
//! [`check`] then holds each body to the shape its phase has (FN-2):
//!
//! > A node's body is its `state`, `cache`, and `inject` declarations
//! > and then its `start`, `when`, and `stop` hooks, and nothing else.
//!
//! What a hook or a graph admits inside is for the passes that read the
//! bodies.

use hgl_diagnostics::{Category, Diagnostics, Location};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};
use hgl_source::FileId;

use crate::program::{Declaration, DeclarationKind, Phase, Program, Role};
use crate::read::{child, tokens};
use crate::types::Detail;
use crate::{codes, error};

/// Fills in [`crate::Module::phases`] for every module of the program.
pub(crate) fn classify(program: &mut Program) {
    for module in &mut program.modules {
        module.phases = module.declarations.iter().map(phase_of).collect();
    }
}

/// What the body of `declaration` describes, when it is a temporal
/// function or a temporal candidate with a body written in HGL.
fn phase_of(declaration: &Declaration) -> Option<Phase> {
    match declaration.kind {
        DeclarationKind::Function(Role::Temporal)
        | DeclarationKind::Implementation(Role::Temporal, _) => {}
        _ => return None,
    }
    // FN-3: "Iteration and conditionals follow the containing phase.
    // They do not alone classify a body as runtime." Only these do, in
    // any position: `descendants` walks every node under the
    // declaration.
    let runtime = own_nodes(&declaration.syntax).iter().any(|node| {
        matches!(
            node.kind(),
            SyntaxKind::StateDecl
                | SyntaxKind::CacheDecl
                | SyntaxKind::InjectDecl
                | SyntaxKind::LifecycleBlock
                | SyntaxKind::WhenStmt
                | SyntaxKind::YieldStmt
        )
    });
    Some(if runtime { Phase::Node } else { Phase::Graph })
}

/// Every node of a function's own body: what stands under `root`,
/// but for what an anonymous function inside it holds. That is a
/// function of its own (FN-14), and what it holds is not `root`'s.
fn own_nodes(root: &SyntaxNode) -> Vec<SyntaxNode> {
    fn collect(node: &SyntaxNode, found: &mut Vec<SyntaxNode>) {
        for child in node.children() {
            if child.kind() == SyntaxKind::FnExpr {
                continue;
            }
            found.push(child.clone());
            collect(&child, found);
        }
    }
    let mut found = Vec::new();
    collect(root, &mut found);
    found
}

/// True for a source: a temporal function whose own body has a `yield`
/// (SOURCE-1). A `yield` in an anonymous function inside it is that
/// function's.
pub(crate) fn is_source(declaration: &Declaration) -> bool {
    own_nodes(&declaration.syntax)
        .iter()
        .any(|node| node.kind() == SyntaxKind::YieldStmt)
}

/// True for a construct only a node's body holds, one of its
/// declarations or one of its hooks. `yield` is a source's, whose
/// contract is proposed and is not checked here.
fn is_node_construct(kind: SyntaxKind) -> bool {
    is_declaration(kind) || is_hook(kind)
}

/// `state`, `cache`, `inject`.
fn is_declaration(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::StateDecl | SyntaxKind::CacheDecl | SyntaxKind::InjectDecl
    )
}

/// `start`, `when`, `stop`.
fn is_hook(kind: SyntaxKind) -> bool {
    matches!(kind, SyntaxKind::LifecycleBlock | SyntaxKind::WhenStmt)
}

/// The keyword a construct begins with, and where it is written.
fn keyword(node: &SyntaxNode, file: FileId) -> Option<(SyntaxToken, Location)> {
    let token = tokens(node).next()?;
    let at = Location::new(file, token.span());
    Some((token, at))
}

/// Holds the body of every function of the program to the shape its
/// phase has.
pub(crate) fn check(program: &Program, diagnostics: &mut Diagnostics) {
    for module in &program.modules {
        for (index, declaration) in module.declarations.iter().enumerate() {
            let file = module.files[declaration.file].file;
            match declaration.kind {
                // A native one states its contract in its body
                // (NATIVE-3): "A native const implementation stays
                // value-level and admits neither `inject` nor temporal
                // lifecycle hooks." NATIVE-5: "A native value function
                // injects nothing".
                DeclarationKind::Function(Role::Value)
                | DeclarationKind::Implementation(Role::Value, _)
                | DeclarationKind::NativeImplementation(Role::Value)
                | DeclarationKind::NativeCandidateImplementation(Role::Value) => {
                    in_value_function(declaration, file, diagnostics);
                }
                _ => {}
            }
            if module.phases.get(index) == Some(&Some(Phase::Node)) {
                let outputless = matches!(
                    module.details.get(index),
                    Some(Detail::Callable(signature)) if signature.result.is_none()
                );
                node_body(declaration, file, outputless, diagnostics);
                in_anonymous_function(declaration, file, diagnostics);
            }
        }
    }
}

/// FN-1: "A `const fn` is a value function." It has no state and no
/// hooks: each of those in one is rejected, at its keyword. FN-9:
/// "Capabilities belong to nodes ... A value function has none. A
/// value it needs from one, the evaluation time say, is its argument",
/// so an `inject` in one is rejected as they are.
fn in_value_function(declaration: &Declaration, file: FileId, diagnostics: &mut Diagnostics) {
    for node in declaration.syntax.descendants() {
        if !is_node_construct(node.kind()) {
            continue;
        }
        let Some((word, at)) = keyword(&node, file) else {
            continue;
        };
        let diagnostic = if node.kind() == SyntaxKind::InjectDecl {
            error(
                Category::Phase,
                codes::NODE_IN_VALUE_FUNCTION,
                at,
                format!(
                    "`inject` is a node's, and `{}` is a `const fn`: a value function has no \
                     capability",
                    declaration.name
                ),
                "FN-9",
            )
            .with_note(
                "pass what it needs as an argument: the node reads `clock.evaluation_time` \
                 and gives it to the function",
            )
        } else {
            error(
                Category::Phase,
                codes::NODE_IN_VALUE_FUNCTION,
                at,
                format!(
                    "`{}` is a node's, and `{}` is a `const fn`: a value function has no state \
                     and no hooks",
                    word.text(),
                    declaration.name
                ),
                "FN-1",
            )
        };
        diagnostics.emit(diagnostic);
    }
}

/// An anonymous function written in a node is value code: a hook
/// "calls only value functions" (FN-3), so it can be called from
/// nowhere else. It has no state and no hooks, and no capability
/// (FN-9: "A value function has none"), as a `const fn` has none.
///
/// One written in a graph is not judged here: what it may be there is
/// not stated.
fn in_anonymous_function(declaration: &Declaration, file: FileId, diagnostics: &mut Diagnostics) {
    for node in declaration.syntax.descendants() {
        if !is_node_construct(node.kind())
            || !node
                .ancestors()
                .any(|ancestor| ancestor.kind() == SyntaxKind::FnExpr)
        {
            continue;
        }
        let Some((word, at)) = keyword(&node, file) else {
            continue;
        };
        let (what, rule) = if node.kind() == SyntaxKind::InjectDecl {
            ("no capability", "FN-9")
        } else {
            ("no state and no hooks", "FN-3")
        };
        diagnostics.emit(error(
            Category::Phase,
            codes::NODE_IN_VALUE_FUNCTION,
            at,
            format!(
                "`{}` is a node's, and this anonymous function is called from the hooks of \
                 `{}`: a value function has {what}",
                word.text(),
                declaration.name
            ),
            rule,
        ));
    }
}

/// The body of a node: its declarations and then its hooks, and nothing
/// else (FN-2).
fn node_body(
    declaration: &Declaration,
    file: FileId,
    outputless: bool,
    diagnostics: &mut Diagnostics,
) {
    // A source is a node of another shape, with statements and `yield`
    // (SOURCE-1), whose contract is proposed: it is not checked here.
    if is_source(declaration) {
        return;
    }
    let own = own_nodes(&declaration.syntax);
    let body = child(&declaration.syntax, SyntaxKind::Block);
    // TIME-3: "A `when` nested in another block is rejected." TIME-5
    // has the declarations and the lifecycle hooks at the same level:
    // they "precede executable hooks", and "At most one start and one
    // stop are admitted."
    for node in &own {
        if !is_node_construct(node.kind()) || node.parent() == body {
            continue;
        }
        let Some((word, at)) = keyword(node, file) else {
            continue;
        };
        diagnostics.emit(error(
            Category::Phase,
            codes::NODE_NESTED,
            at,
            format!(
                "`{}` stands in a node's body itself, and not inside another block",
                word.text()
            ),
            if node.kind() == SyntaxKind::WhenStmt {
                "TIME-3"
            } else {
                "TIME-5"
            },
        ));
    }
    let Some(body) = body else {
        return;
    };
    // The first hook, once one is met: a declaration after it is out of
    // order.
    let mut hook: Option<Location> = None;
    // Where `start` and `stop` were first written.
    let mut lifecycle: Vec<(String, Location)> = Vec::new();
    for statement in body.children() {
        let kind = statement.kind();
        if !is_node_construct(kind) {
            // FN-2: "and nothing else: a local that every hook reads is
            // a `cache`, and work done once belongs in `start`."
            diagnostics.emit(
                error(
                    Category::Phase,
                    codes::NODE_STATEMENT,
                    Location::new(file, statement.span()),
                    format!(
                        "`{}` is a node, and its body holds only its declarations and its hooks",
                        declaration.name
                    ),
                    "FN-2",
                )
                .with_note(
                    "a local that every hook reads is a `cache`, and work done once belongs \
                     in `start`",
                ),
            );
            continue;
        }
        let Some((word, at)) = keyword(&statement, file) else {
            continue;
        };
        if is_declaration(kind) {
            // TIME-5: "State/cache/inject declarations precede
            // executable hooks."
            if let Some(hook) = hook {
                diagnostics.emit(
                    error(
                        Category::Phase,
                        codes::NODE_DECLARATION_ORDER,
                        at,
                        format!(
                            "`{}` follows a hook: a node's declarations come before its hooks",
                            word.text()
                        ),
                        "TIME-5",
                    )
                    .with_related(hook, "the first hook"),
                );
            }
            // TIME-7: `inject out` "is rejected for outputless
            // functions."
            if kind == SyntaxKind::InjectDecl && outputless {
                for name in tokens(&statement)
                    .filter(|token| token.kind() == SyntaxKind::Ident && token.text() == "out")
                {
                    diagnostics.emit(error(
                        Category::Injectable,
                        codes::INJECT_OUT_WITHOUT_RESULT,
                        Location::new(file, name.span()),
                        format!(
                            "`out` is a node's result, and `{}` declares none",
                            declaration.name
                        ),
                        "TIME-7",
                    ));
                }
            }
        } else if is_hook(kind) {
            hook.get_or_insert(at);
            if kind == SyntaxKind::LifecycleBlock {
                // TIME-5: "At most one start and one stop are admitted."
                match lifecycle.iter().find(|(earlier, _)| earlier == word.text()) {
                    Some((_, first)) => diagnostics.emit(
                        error(
                            Category::Phase,
                            codes::NODE_DUPLICATE_HOOK,
                            at,
                            format!("a node has one `{}`, and this is a second", word.text()),
                            "TIME-5",
                        )
                        .with_related(*first, "the first"),
                    ),
                    None => lifecycle.push((word.text().to_string(), at)),
                }
            }
        }
    }
}
