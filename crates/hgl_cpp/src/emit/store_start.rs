//! The startup-only store operations of a selected child (FN-9, CRUN-7).
//! The SDK composes the child's wiring before selection. An ordinary
//! prefix that supplies no wiring value can instead run in a start hook
//! before the child's user node starts.

use hgl_analyser::hir::{Callee, Expr, ExprKind, Hook, Node, Stmt, walk_statement_nodes};
use hgl_analyser::{Phase, Program, Type};
use hgl_lower::graphs::named_in;

use super::Emitter;
use super::configuration;
use super::graphs::WIRING;
use super::names::{identifier, string};
use super::nodes::{Context, Kind, Role};
use super::statements::Scope;
use super::values::INDENT;

/// An ordinary prefix and its order relative to a directly wired user node.
#[derive(Clone, Copy)]
pub(super) struct StoreStart {
    /// The number of branch statements executed by the start hook.
    pub(super) count: usize,
    /// Whether the branch's only user node returns the result port.
    pub(super) rank_result: bool,
    store: usize,
}

impl StoreStart {
    /// Accept a straight-line prefix whose locals never leave it. The
    /// rest may return a captured connection, wire one scalar node
    /// directly, or do nothing. With multiple nodes, the terminal's
    /// rank alone would not order all user startup hooks.
    pub(super) fn of(
        program: &Program,
        store: usize,
        statements: &[Stmt],
        result: Option<&Expr>,
    ) -> Option<Self> {
        if statements.is_empty() {
            return None;
        }
        for statement in statements {
            match statement {
                Stmt::Declare { value: Some(_), .. } => {}
                Stmt::Expr(Expr {
                    kind:
                        ExprKind::Call {
                            callee, arguments, ..
                        },
                    ..
                }) if program.store_arguments(callee, arguments).is_some_and(
                    |(arg, _)| matches!(arg.kind, ExprKind::Local(local) if local == store),
                ) => {}
                _ => return None,
            }
        }
        let mut declared = Vec::new();
        let mut read = Vec::new();
        let mut supported = true;
        walk_statement_nodes(statements, &mut |node| {
            named_in(node, &mut read, &mut declared);
            supported &= match node {
                Node::Expression(value) => !value.is_connection(),
                Node::Statement(Stmt::Declare { value: Some(_), .. } | Stmt::Expr(_)) => true,
                Node::Statement(_) => false,
            };
        });
        if !supported || !super::nodes::uses(statements, store) {
            return None;
        }
        declared.push(store);
        let rank_result = match result {
            Some(result) => Self::rank_result(program, result, &declared)?,
            None => false,
        };
        Some(Self {
            count: statements.len(),
            rank_result,
            store,
        })
    }

    /// Whether the only user node is the producer of the result port.
    fn rank_result(program: &Program, result: &Expr, startup: &[usize]) -> Option<bool> {
        let mut read = Vec::new();
        let mut declared = Vec::new();
        result.walk_nodes(&mut |node| named_in(node, &mut read, &mut declared));
        if !result.is_connection() || read.iter().any(|local| startup.contains(local)) {
            return None;
        }
        match &result.kind {
            ExprKind::Local(_) => Some(false),
            ExprKind::Call {
                callee:
                    Callee::Function {
                        module,
                        declaration,
                        ..
                    },
                arguments,
                ..
            } if program.modules[*module].phases[*declaration] == Some(Phase::Node)
                && matches!(result.ty, Type::Scalar(_))
                && arguments.iter().all(|arg| {
                    matches!(
                        arg.value.kind,
                        ExprKind::Local(_) | ExprKind::Literal(_) | ExprKind::Constant { .. }
                    )
                }) =>
            {
                Some(true)
            }
            _ => None,
        }
    }

    /// Write the starter declaration and the wiring expression for it.
    /// A starter with a layout dependency has an inert output. Its
    /// outputless form is a sink so the SDK retains it independently.
    pub(super) fn emit(
        self,
        emitter: &mut Emitter<'_>,
        scope: &Scope<'_>,
        statements: &[Stmt],
        configured: &[usize],
    ) -> Option<(String, String)> {
        let program = emitter.program;
        let locals = &scope.body.locals;
        let mut context = Context::new(Kind::Hook(Hook::Start), locals.len());
        let mut parameters = Vec::new();
        let mut given = vec![WIRING.to_string()];
        let mut read = Vec::new();
        let mut declared = Vec::new();
        walk_statement_nodes(statements, &mut |node| {
            named_in(node, &mut read, &mut declared);
        });
        for &local in configured {
            if !read.contains(&local) {
                continue;
            }
            let item = &locals[local];
            let name = identifier(&item.name);
            let ty = emitter.written_type(&item.ty, item.at)?;
            let held = configuration::held(program, &item.ty, &ty);
            parameters.push(format!(
                "::hgraph::Scalar<{}, {held}> {name}",
                string(&item.name)
            ));
            // A branch's compose already holds this configuration in the
            // same representation as the starter.
            given.push(format!("{name}.value()"));
            context.roles[local] = Role::Config;
        }
        // hgraph takes the node's scalar schema from its eval signature.
        let mut evaluation: Vec<String> = parameters
            .iter()
            .map(|parameter| format!("[[maybe_unused]] {parameter}"))
            .collect();
        if self.rank_result {
            evaluation
                .push("[[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Bool>> out".into());
        }
        let mut body = emitter.capabilities(
            scope.body,
            &[self.store],
            statements,
            Hook::Start,
            false,
            &mut parameters,
        )?;
        let mut startup = Scope::cold(program, scope.body);
        emitter.in_context(&mut context, |emitter| {
            emitter.statements(&mut startup, statements, 2, &mut body);
        });
        let body = body
            .lines()
            .map(|line| format!("{INDENT}{INDENT}{line}\n"))
            .collect::<String>();
        let definition = format!(
            "{INDENT}{INDENT}struct store_start_\n\
             {INDENT}{INDENT}{{\n\
             {INDENT}{INDENT}{INDENT}static constexpr auto name = \"hgl.child_store_start\";\n\
             {INDENT}{INDENT}{INDENT}static void start({})\n\
             {INDENT}{INDENT}{INDENT}{{\n{body}{INDENT}{INDENT}{INDENT}}}\n\
             {INDENT}{INDENT}{INDENT}static void eval({}) {{}}\n\
             {INDENT}{INDENT}}};\n",
            parameters.join(", "),
            evaluation.join(", "),
        );
        Some((
            definition,
            format!("::hgraph::wire<store_start_>({})", given.join(", ")),
        ))
    }
}
