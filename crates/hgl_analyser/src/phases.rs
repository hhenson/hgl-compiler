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
use hgl_lexer::literal::decode_string;
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};
use hgl_source::FileId;

use crate::program::{Declaration, DeclarationKind, Phase, Program, RUN_CONFIG, Role};
use crate::syntax::{child, test_statements, tokens};
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
    //
    // FN-2: "`state`, `cache`, `start`, `when`, `stop`, `yield`, or an
    // `inject` of any capability but `global_state` classifies that
    // complete body as runtime ... A graph may inject `global_state`
    // (FN-9)."
    let runtime = own_nodes(&declaration.syntax)
        .iter()
        .any(|node| match node.kind() {
            SyntaxKind::StateDecl
            | SyntaxKind::CacheDecl
            | SyntaxKind::LifecycleBlock
            | SyntaxKind::WhenStmt
            | SyntaxKind::YieldStmt => true,
            SyntaxKind::InjectDecl => tokens(node)
                .filter(|token| token.kind() == SyntaxKind::Ident)
                .any(|name| name.text() != "global_state"),
            _ => false,
        });
    Some(if runtime { Phase::Node } else { Phase::Graph })
}

/// Nodes of a function or test's own body. An anonymous function (FN-14)
/// or a test's named helper (TST-1) has a body of its own.
fn own_nodes(root: &SyntaxNode) -> Vec<SyntaxNode> {
    fn collect(node: &SyntaxNode, found: &mut Vec<SyntaxNode>) {
        for child in node.children() {
            if matches!(child.kind(), SyntaxKind::FnExpr | SyntaxKind::FnDecl) {
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
/// declarations or one of its hooks. `yield` is a source's, which has
/// a shape of its own (SOURCE-1).
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
            // Native value implementations state their contract in the
            // body (NATIVE-3, NATIVE-5).
            let value_function = matches!(
                declaration.kind,
                DeclarationKind::Function(Role::Value)
                    | DeclarationKind::Implementation(Role::Value, _)
                    | DeclarationKind::NativeImplementation(Role::Value)
                    | DeclarationKind::NativeCandidateImplementation(Role::Value)
            );
            if value_function {
                in_value_function(declaration, file, diagnostics);
            }
            yield_placement(&declaration.syntax, file, value_function, diagnostics);
            if module.phases.get(index) == Some(&Some(Phase::Node)) {
                let outputless = matches!(
                    module.details.get(index),
                    Some(Detail::Callable(signature)) if signature.result.is_none()
                );
                node_body(declaration, file, outputless, diagnostics);
                in_anonymous_function(declaration, file, diagnostics);
            }
            in_function(&declaration.syntax, file, diagnostics);
        }
        for test in &module.tests {
            let file = module.files[test.file].file;
            test_body(&test.syntax, file, diagnostics);
            for statement in test_statements(&test.syntax) {
                yield_placement(&statement, file, false, diagnostics);
            }
            in_test(&test.syntax, test.integration, file, diagnostics);
        }
        for (syntax, file) in module
            .run
            .iter()
            .map(|run| (&run.syntax, run.file))
            .chain(module.store.iter().map(|store| (&store.syntax, store.file)))
        {
            yield_placement(syntax, module.files[file].file, false, diagnostics);
        }
    }
}

/// SOURCE-1 reserves `yield` for a temporal source. An anonymous
/// function has its own body (FN-14) and never inherits that role, even
/// inside a source. A value function also cannot yield (FN-1, FN-2).
/// Direct yields in a source or a test keep their own phase checks.
fn yield_placement(
    root: &SyntaxNode,
    file: FileId,
    value_function: bool,
    diagnostics: &mut Diagnostics,
) {
    for node in root
        .descendants()
        .filter(|node| node.kind() == SyntaxKind::YieldStmt)
    {
        let (context, rule) = if node
            .ancestors()
            .any(|ancestor| ancestor.kind() == SyntaxKind::FnExpr)
        {
            ("an anonymous function", "SOURCE-1")
        } else if value_function {
            ("a value function", "FN-1")
        } else {
            continue;
        };
        if let Some((_, at)) = keyword(&node, file) {
            diagnostics.emit(error(
                Category::Phase,
                codes::YIELD_OUTSIDE_SOURCE,
                at,
                format!("`yield` belongs to a source, and this one is inside {context}"),
                rule,
            ));
        }
    }
}

/// True for an `inject` of `run_config` alone, which RUN-6 places
/// (`inject.position`), and no other rule judges.
fn injects_run_config_alone(node: &SyntaxNode) -> bool {
    let mut names = tokens(node)
        .filter(|token| token.kind() == SyntaxKind::Ident)
        .peekable();
    names.peek().is_some() && names.all(|name| name.text() == RUN_CONFIG)
}

/// Each `run_config` an `inject` of a body names, where it is written:
/// the statement's own nodes, and not an anonymous function's, which
/// injects nothing (FN-9).
fn run_config_injections(root: &SyntaxNode, file: FileId) -> Vec<(SyntaxNode, Location)> {
    own_nodes(root)
        .into_iter()
        .filter(|node| node.kind() == SyntaxKind::InjectDecl)
        .flat_map(|node| {
            tokens(&node)
                .filter(|token| token.kind() == SyntaxKind::Ident && token.text() == RUN_CONFIG)
                .map(|name| (node.clone(), Location::new(file, name.span())))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// RUN-6: "An application injects `run_config`, and so does an
/// integration test (TST-17) ... A test that is no integration test
/// does not, and neither does a function: one that needs a bound of the
/// run takes it as a `const` argument." So an `inject run_config` in a
/// function's body rejects, at the name (`inject.position`).
fn in_function(root: &SyntaxNode, file: FileId, diagnostics: &mut Diagnostics) {
    for (_, at) in run_config_injections(root, file) {
        diagnostics.emit(
            error(
                Category::Injectable,
                codes::INJECT_POSITION,
                at,
                "a function does not inject `run_config`: it is an application's and an \
                 integration test's"
                    .to_string(),
                "RUN-6",
            )
            .with_note("a function that needs a bound of the run takes it as a `const` argument"),
        );
    }
}

/// RUN-6 and TST-17 for a test's `inject run_config`: "An integration
/// test may inject `run_config`, before its other statements", and "A
/// test that is no integration test does not". Each that has no place
/// there rejects, at the name (`inject.position`): in a test that is no
/// integration test, and in one that is, after another statement or
/// inside one.
fn in_test(test: &SyntaxNode, integration: bool, file: FileId, diagnostics: &mut Diagnostics) {
    // The test's leading `inject` statements, which come before its
    // other statements.
    let leading: Vec<SyntaxNode> = test_statements(test)
        .take_while(|statement| statement.kind() == SyntaxKind::InjectDecl)
        .collect();
    for (statement, at) in run_config_injections(test, file) {
        if integration && leading.contains(&statement) {
            continue;
        }
        let message = if integration {
            "an integration test injects `run_config` before its other statements"
        } else {
            "a test that is no integration test does not inject `run_config`: its evals give \
             their configuration in braces"
        };
        diagnostics.emit(
            error(
                Category::Injectable,
                codes::INJECT_POSITION,
                at,
                message.to_string(),
                "RUN-6",
            )
            .with_note("`eval{length: 4s} target(...)` gives an eval its configuration"),
        );
    }
}

/// FN-1: "A `const fn` is a value function." It has no state and no
/// hooks: each of those in one is rejected, at its keyword. FN-9:
/// "Capabilities belong to nodes ... A value function has none. A
/// value it needs from one, the evaluation time say, is its argument",
/// so an `inject` in one is rejected as they are.
fn in_value_function(declaration: &Declaration, file: FileId, diagnostics: &mut Diagnostics) {
    for node in declaration.syntax.descendants() {
        // RUN-6 places `inject run_config` (`in_function`).
        if !is_node_construct(node.kind())
            || (node.kind() == SyntaxKind::InjectDecl && injects_run_config_alone(&node))
        {
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

/// The block of a named test. TST-3: "A test is a composition-phase
/// block with assertions. It cannot itself declare state/cache, inject,
/// lifecycle hooks, when, yield, or return." TST-10: "`assert
/// raises(\"code\")` requires one literal catalogued execution code."
///
/// What an anonymous function in a test holds is that function's, and
/// not the test's own.
fn test_body(test: &SyntaxNode, file: FileId, diagnostics: &mut Diagnostics) {
    for node in own_nodes(test) {
        match node.kind() {
            // TST-3: "an integration test injects `run_config`, and no
            // other capability (TST-17)". Where it has a place is RUN-6's
            // to say (`in_test`).
            SyntaxKind::InjectDecl if injects_run_config_alone(&node) => {}
            SyntaxKind::StateDecl
            | SyntaxKind::CacheDecl
            | SyntaxKind::InjectDecl
            | SyntaxKind::LifecycleBlock
            | SyntaxKind::WhenStmt
            | SyntaxKind::YieldStmt
            | SyntaxKind::ReturnStmt => {
                let Some((word, at)) = keyword(&node, file) else {
                    continue;
                };
                diagnostics.emit(
                    error(
                        Category::Phase,
                        codes::TEST_STATEMENT_PHASE,
                        at,
                        format!(
                            "`{}` has no place in a test: a test is a composition block with \
                             assertions",
                            word.text()
                        ),
                        "TST-3",
                    )
                    .with_note("what a test runs is a function it names in `eval`"),
                );
            }
            SyntaxKind::RaisesAssertion => raises_code(&node, file, diagnostics),
            _ => {}
        }
    }
}

/// TST-10: "`assert raises(\"code\")` requires one literal catalogued
/// execution code." The argument is a string literal, written out, whose
/// value the catalogue lists among its execution codes.
fn raises_code(assertion: &SyntaxNode, file: FileId, diagnostics: &mut Diagnostics) {
    // What stands between the parentheses: every child but the block.
    let arguments: Vec<SyntaxNode> = assertion
        .children()
        .filter(|part| part.kind() != SyntaxKind::Block)
        .collect();
    let Some(first) = arguments.first() else {
        // The parser already reported a missing argument.
        return;
    };
    let code = match arguments.as_slice() {
        [literal] if literal.kind() == SyntaxKind::Literal => tokens(literal)
            .next()
            .filter(|token| token.kind() == SyntaxKind::Str)
            .and_then(|token| decode_string(token.text()).ok()),
        _ => None,
    };
    if code
        .as_deref()
        .is_some_and(hgl_diagnostics::catalogue::is_execution)
    {
        return;
    }
    let message = match &code {
        Some(code) => format!("`{code}` is no execution code of the catalogue"),
        None => "`raises` names one execution code, as a string literal".to_string(),
    };
    diagnostics.emit(
        error(
            Category::Type,
            codes::TEST_RAISES_CODE,
            Location::new(file, first.span()),
            message,
            "TST-10",
        )
        .with_note("an execution code names a failure of a running program: `value.missing_key`"),
    );
}

/// The body of a source (SOURCE-1): "A temporal `fn` whose body has a
/// `yield` is a source: a node that produces its output and has no
/// temporal parameters. It injects `clock`, `logger`, and
/// `global_state`, and not `out`, `scheduler`, or `alarm` ... A source
/// with no declaration and no hook has a body of statements, `yield`
/// among them. One with an `inject`, a `cache`, a `state`, a `start`,
/// or a `stop` has its declarations, then `start`, then one block that
/// holds every `yield`, then `stop`, and nothing else; the block is
/// `{ ... }`, a `while`, or a `for` ... a source declares no `var`. A
/// source has no `when`."
fn source_body(declaration: &Declaration, file: FileId, diagnostics: &mut Diagnostics) {
    let rejected = |diagnostics: &mut Diagnostics, at: Location, message: String| {
        diagnostics.emit(error(
            Category::Phase,
            codes::SOURCE_STATEMENT,
            at,
            message,
            "SOURCE-1",
        ));
    };
    let name = &declaration.name;
    // SOURCE-1 forbids temporal inputs. A function parameter is fixed
    // at compile time (FN-17), so it supplies no input.
    for parameter in crate::read::parameters(&declaration.syntax) {
        if !crate::read::is_const(&parameter)
            && child(&parameter, SyntaxKind::FunctionType).is_none()
        {
            diagnostics.emit(
                error(
                    Category::Phase,
                    codes::SOURCE_INPUT,
                    Location::new(file, parameter.span()),
                    format!("`{name}` is a source, and has no temporal parameter"),
                    "SOURCE-1",
                )
                .with_note("a source takes its configuration as `const` parameters"),
            );
        }
    }
    let own = own_nodes(&declaration.syntax);
    let body = child(&declaration.syntax, SyntaxKind::Block);
    for node in &own {
        match node.kind() {
            // "A source has no `when`", wherever it is written.
            SyntaxKind::WhenStmt => {
                if let Some((_, at)) = keyword(node, file) {
                    diagnostics.emit(error(
                        Category::Phase,
                        codes::SOURCE_WHEN,
                        at,
                        format!("`{name}` is a source, and a source has no `when`"),
                        "SOURCE-1",
                    ));
                }
            }
            // "A source declares no `var`."
            SyntaxKind::LocalDecl
                if tokens(node)
                    .next()
                    .is_some_and(|word| word.kind() == SyntaxKind::KwVar) =>
            {
                rejected(
                    diagnostics,
                    Location::new(file, node.span()),
                    format!(
                        "`{name}` is a source, and a source declares no `var`: what it keeps \
                         from one `yield` to the next is a `cache` or a `state`"
                    ),
                );
            }
            // It injects `clock`, `logger` and `global_state`, "and not
            // `out`, `scheduler`, or `alarm`".
            SyntaxKind::InjectDecl => {
                let taken: Vec<String> = tokens(node)
                    .filter(|token| {
                        token.kind() == SyntaxKind::Ident
                            && matches!(token.text(), "out" | "scheduler" | "alarm")
                    })
                    .map(|token| format!("`{}`", token.text()))
                    .collect();
                if !taken.is_empty() {
                    rejected(
                        diagnostics,
                        Location::new(file, node.span()),
                        format!(
                            "`{name}` is a source, and does not inject {}: `yield` writes its \
                             output and sets its next evaluation",
                            taken.join(" or ")
                        ),
                    );
                }
            }
            _ => {}
        }
        // TIME-5 has the declarations and the lifecycle hooks at the
        // level of the body, in a source as in any node.
        if (is_declaration(node.kind()) || node.kind() == SyntaxKind::LifecycleBlock)
            && node.parent() != body
            && let Some((word, at)) = keyword(node, file)
        {
            diagnostics.emit(error(
                Category::Phase,
                codes::NODE_NESTED,
                at,
                format!(
                    "`{}` stands in a node's body itself, and not inside another block",
                    word.text()
                ),
                "TIME-5",
            ));
        }
    }
    let Some(body) = body else {
        return;
    };
    let statements: Vec<SyntaxNode> = body.children().collect();
    // "A source with no declaration and no hook has a body of
    // statements, `yield` among them."
    let structured = statements.iter().any(|statement| {
        is_declaration(statement.kind()) || statement.kind() == SyntaxKind::LifecycleBlock
    });
    if !structured {
        return;
    }
    // "Its declarations, then `start`, then one block that holds every
    // `yield`, then `stop`, and nothing else". Each statement has a
    // place in that order, and none comes before one it follows.
    #[derive(Clone, Copy, PartialEq, PartialOrd)]
    enum Place {
        Declarations,
        Start,
        Block,
        Stop,
    }
    let mut reached = Place::Declarations;
    // Where the block, `start` and `stop` were first written.
    let mut block: Option<Location> = None;
    let mut lifecycle: Vec<(String, Location)> = Vec::new();
    for statement in &statements {
        let at = Location::new(file, statement.span());
        let kind = statement.kind();
        // `{ ... }` as a statement is a block in an expression
        // statement.
        let is_block = match kind {
            SyntaxKind::WhileStmt | SyntaxKind::ForStmt => true,
            SyntaxKind::ExprStmt => statement
                .children()
                .next()
                .is_some_and(|inner| inner.kind() == SyntaxKind::Block),
            _ => false,
        };
        let place = if is_declaration(kind) {
            Place::Declarations
        } else if kind == SyntaxKind::LifecycleBlock {
            let word = keyword(statement, file);
            let Some((word, word_at)) = word else {
                continue;
            };
            // TIME-5: "At most one start and one stop are admitted."
            match lifecycle.iter().find(|(earlier, _)| earlier == word.text()) {
                Some((_, first)) => {
                    diagnostics.emit(
                        error(
                            Category::Phase,
                            codes::NODE_DUPLICATE_HOOK,
                            word_at,
                            format!("a node has one `{}`, and this is a second", word.text()),
                            "TIME-5",
                        )
                        .with_related(*first, "the first"),
                    );
                    continue;
                }
                None => lifecycle.push((word.text().to_string(), word_at)),
            }
            // "A `yield` in a hook" is no part of the one block.
            for inner in own_nodes(statement)
                .iter()
                .filter(|inner| inner.kind() == SyntaxKind::YieldStmt)
            {
                rejected(
                    diagnostics,
                    Location::new(file, inner.span()),
                    format!(
                        "a source yields in its one block, and this `yield` is in `{}`",
                        word.text()
                    ),
                );
            }
            if word.kind() == SyntaxKind::KwStart {
                Place::Start
            } else {
                Place::Stop
            }
        } else if is_block {
            if let Some(first) = block {
                diagnostics.emit(
                    error(
                        Category::Phase,
                        codes::SOURCE_STATEMENT,
                        at,
                        format!(
                            "`{name}` has one block that holds every `yield`, and this is a second"
                        ),
                        "SOURCE-1",
                    )
                    .with_related(first, "the first"),
                );
                continue;
            }
            block = Some(at);
            Place::Block
        } else if kind == SyntaxKind::WhenStmt {
            // Reported above, as `source.when`.
            continue;
        } else {
            rejected(
                diagnostics,
                at,
                format!(
                    "`{name}` is a source with a declaration or a hook: its body is its \
                     declarations, `start`, one block that holds every `yield`, and `stop`"
                ),
            );
            continue;
        };
        if place < reached {
            if place == Place::Declarations {
                // TIME-5: "State/cache/inject declarations precede
                // executable hooks", and so the block they run around.
                if let Some((word, word_at)) = keyword(statement, file) {
                    diagnostics.emit(error(
                        Category::Phase,
                        codes::NODE_DECLARATION_ORDER,
                        word_at,
                        format!(
                            "`{}` follows a hook or the block: a source's declarations come \
                             first",
                            word.text()
                        ),
                        "TIME-5",
                    ));
                }
            } else {
                rejected(
                    diagnostics,
                    at,
                    format!(
                        "`{name}` is out of order here: a source has its declarations, then \
                         `start`, then its one block, then `stop`"
                    ),
                );
            }
        } else {
            reached = place;
        }
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
    // (SOURCE-1).
    if is_source(declaration) {
        // SOURCE-1: "A source declares a result, which is what it exists
        // to publish: one with no result arrow rejects."
        if child(&declaration.syntax, SyntaxKind::ReturnType).is_none() {
            diagnostics.emit(error(
                Category::Phase,
                codes::SOURCE_RESULT,
                declaration.at,
                format!(
                    "`{}` is a source and must declare a result",
                    declaration.name
                ),
                "SOURCE-1",
            ));
        }
        source_body(declaration, file, diagnostics);
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
