//! Says what the body of each temporal function describes: composition
//! or runtime (FN-2).
//!
//! > An ordinary temporal body with no node-only construct describes
//! > composition: the function is a graph. `state`, `cache`, `inject`,
//! > `start`, `when`, `stop`, or `yield` classifies that complete body
//! > as runtime: the function is a node. Classification precedes phase
//! > checking, even if the trigger occurs in an invalid nested position.
//!
//! So this pass looks only at which constructs a body holds, wherever
//! it holds them, and reports nothing. What each phase then admits is
//! for the passes that read the bodies.

use hgl_parser::SyntaxKind;

use crate::program::{Declaration, DeclarationKind, Phase, Program, Role};

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
    let runtime = declaration.syntax.descendants().any(|node| {
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
