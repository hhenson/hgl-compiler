//! What a graph's `if`, `switch` and `for` over connections need of
//! their statements, as the language says it: what each child graph
//! takes of the graph it is in, the value of a `switch` label, the
//! `var`s a branch assigns, and which paths return. Part of lowering: a
//! transpiler writes each branch or loop body as a child graph with
//! these (CTRL-2, CTRL-7, CTRL-11, CTRL-14, CTRL-15).

use hgl_analyser::hir::{
    Callee, Case, Expr, ExprKind, LabelValue, Local, Stmt, Value, ValueCategory,
};
use hgl_analyser::{Detail, Program, Role};
use hgl_diagnostics::Location;

/// The value of `case`'s label: the one the analyser resolved it to
/// (`Case::value`), or, for a label it could not resolve, the
/// literal or the member an instance has in its place (GEN-13).
/// `None` for any other, a constant a `const fn` computes among them.
pub fn label_value(case: &Case) -> Option<LabelValue> {
    case.value.clone().or_else(|| match &case.label.kind {
        ExprKind::Literal(Value::Int(value)) => Some(LabelValue::Int(*value)),
        ExprKind::Literal(Value::Bool(value)) => Some(LabelValue::Bool(*value)),
        ExprKind::Literal(Value::Str(value)) => Some(LabelValue::Str(value.clone())),
        ExprKind::EnumMember { member, .. } => Some(LabelValue::Member(*member)),
        _ => None,
    })
}

/// What a child graph takes of the graph it is written in, from what
/// its statements name, `read`, and declare, `declared` ([`named_in`]):
/// every connection named, read or assigned, at any depth, that has a
/// value where the child is written, `bound`, less the names the child
/// declares itself (CTRL-7: "Captured temporal connections are bound
/// whole and add no membership"). `locals` are the graph's locals. One the
/// child does not need is harmless; one with no value yet is never
/// given. A local the target gives the child as a connection, though
/// the language has it as an ordinary value, is one too: `ported`, the
/// key of a loop that makes one child for each member.
pub fn taken(
    locals: &[Local],
    bound: &[usize],
    ported: &[usize],
    mut read: Vec<usize>,
    declared: &[usize],
) -> Vec<usize> {
    read.retain(|local| {
        !declared.contains(local)
            && bound.contains(local)
            && (locals[*local].category == ValueCategory::Connection || ported.contains(local))
    });
    read.sort_unstable();
    read.dedup();
    read
}

/// Where `body`, the body of a loop over a connection that makes one
/// child for each member, reads `key`, the loop's key or member, other
/// than as the whole argument of a temporal parameter of a temporal
/// function: `None` where it reads it only so. Given so, the key is a
/// connection of the child, or an ordinary value wired as a constant of
/// one where the child starts (FN-3); read anywhere else it is a value
/// the child must know as it is built (CTRL-7).
pub fn key_read_as_value(program: &Program, key: usize, body: &[Stmt]) -> Option<Location> {
    use hgl_analyser::hir::{Node, walk_statement_nodes};
    let mut reads = Vec::new();
    let mut given = Vec::new();
    walk_statement_nodes(body, &mut |node| match node {
        Node::Expression(Expr {
            kind: ExprKind::Local(local),
            at,
            ..
        }) if *local == key => reads.push(*at),
        Node::Expression(Expr {
            kind:
                ExprKind::Call {
                    callee:
                        Callee::Function {
                            module,
                            declaration,
                            ..
                        },
                    arguments,
                    ..
                },
            ..
        }) => {
            let declared = &program.modules[*module];
            let temporal = declared.declarations[*declaration].kind.role() == Some(Role::Temporal);
            let Some(Detail::Callable(signature)) = declared.details.get(*declaration) else {
                return;
            };
            for argument in arguments {
                let taken = argument
                    .parameter
                    .and_then(|index| signature.parameters.get(index))
                    .is_some_and(|parameter| {
                        !parameter.constant && !parameter.var && parameter.pack.is_none()
                    });
                if temporal
                    && taken
                    && matches!(argument.value.kind, ExprKind::Local(local) if local == key)
                {
                    given.push(argument.value.at);
                }
            }
        }
        _ => {}
    });
    reads.into_iter().find(|at| !given.contains(at))
}

/// The statements of a branch of an `if` that stands as a statement:
/// none with no `else`, a block's, with the block's last expression (a
/// nested `if`, say) as a statement of its own, and for an `else if`
/// the nested `if` as its one statement.
pub fn alternative_statements(otherwise: Option<&Expr>) -> Vec<Stmt> {
    match otherwise {
        None => Vec::new(),
        Some(Expr {
            kind: ExprKind::Block { statements, value },
            ..
        }) => {
            let mut all = statements.clone();
            all.extend(value.iter().map(|value| Stmt::Expr((**value).clone())));
            all
        }
        Some(alternative) => vec![Stmt::Expr(alternative.clone())],
    }
}

/// True where some path through `statements` has a `return`, at any
/// depth.
pub fn may_return(statements: &[Stmt]) -> bool {
    let mut found = false;
    hgl_analyser::hir::walk_statement_nodes(statements, &mut |node| {
        found |= matches!(
            node,
            hgl_analyser::hir::Node::Statement(Stmt::Return { .. })
        );
    });
    found
}

/// True where an `if` or a `switch` among `statements` has a `return`
/// in it: it needs what follows it to lower its other paths.
pub fn nested_return(statements: &[Stmt]) -> bool {
    statements.iter().any(|statement| {
        matches!(
            statement,
            Stmt::Switch { .. }
                | Stmt::Expr(Expr {
                    kind: ExprKind::If { .. },
                    ..
                })
        ) && may_return(std::slice::from_ref(statement))
    })
}

/// `statements`, and after them `rest` where they can reach their end.
pub fn continued(statements: &[Stmt], rest: &[Stmt]) -> Vec<Stmt> {
    let mut all = statements.to_vec();
    if !always_returns(statements) {
        all.extend_from_slice(rest);
    }
    all
}

/// The locals `statements` assign, by name or by a place inside them,
/// that they do not declare themselves: the `var`s of what encloses
/// them that they change.
pub fn assigned_outside(statements: &[Stmt]) -> Vec<usize> {
    use hgl_analyser::hir::{Node, walk_statement_nodes};
    let mut assigned = Vec::new();
    let mut declared = Vec::new();
    walk_statement_nodes(statements, &mut |node| match node {
        Node::Statement(Stmt::Declare { local, .. }) => declared.push(*local),
        Node::Statement(Stmt::For { names, .. }) => declared.extend(names),
        Node::Statement(Stmt::Assign { local, .. }) => assigned.push(*local),
        Node::Statement(Stmt::AssignPlace { place, .. }) => {
            if let ExprKind::Local(local) = place.place_root().kind {
                assigned.push(local);
            }
        }
        _ => {}
    });
    assigned.retain(|local| !declared.contains(local));
    assigned.sort_unstable();
    assigned.dedup();
    assigned
}

/// Adds what `node`, a part of a child graph's statements, names to
/// `read`, and what it declares to `declared`: a local read or
/// assigned, and a `let`, a `var` or a loop's names.
pub fn named_in(node: hgl_analyser::hir::Node, read: &mut Vec<usize>, declared: &mut Vec<usize>) {
    use hgl_analyser::hir::Node;
    match node {
        Node::Statement(Stmt::Declare { local, .. }) => declared.push(*local),
        Node::Statement(Stmt::For { names, .. }) => declared.extend(names),
        Node::Expression(Expr {
            kind: ExprKind::ListComprehension { binding, .. },
            ..
        }) => {
            declared.push(*binding);
        }
        Node::Statement(Stmt::Assign { local, .. }) => read.push(*local),
        Node::Expression(Expr {
            kind: ExprKind::Local(local),
            ..
        }) => read.push(*local),
        Node::Expression(Expr {
            kind:
                ExprKind::Call {
                    callee: Callee::Anonymous(function),
                    ..
                },
            ..
        }) => read.extend(&function.captures),
        _ => {}
    }
}

/// True where every path through `statements` ends in a `return`: the
/// last is one, or an `if` with an `else` or a `switch` with a
/// `default` each branch of which always returns.
pub fn always_returns(statements: &[Stmt]) -> bool {
    match statements.last() {
        Some(Stmt::Return { .. }) => true,
        Some(Stmt::Expr(Expr {
            kind:
                ExprKind::If {
                    then,
                    otherwise: Some(otherwise),
                    ..
                },
            ..
        })) => {
            always_returns(&alternative_statements(Some(then)))
                && always_returns(&alternative_statements(Some(otherwise)))
        }
        Some(Stmt::Switch {
            cases,
            default: Some(default),
            ..
        }) => cases.iter().all(|case| always_returns(&case.body)) && always_returns(default),
        _ => false,
    }
}
