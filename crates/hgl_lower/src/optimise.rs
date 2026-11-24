//! Target-independent changes that preserve the meaning of a typed body.
//!
//! Each pass gives emitters a plan for the forms it can improve. Keeping the
//! analysed body intact preserves its places and source spans for other passes.

use std::collections::BTreeSet;

use hgl_analyser::hir::{
    BinaryOperator, Body, Expr, ExprKind, Form, LocalId, Node, Stmt, Value, walk_statement_nodes,
};
use hgl_analyser::{DeclarationKind, LocalKind, Program, Role, Scalar, Type};

/// The optimisation passes available when a checked body is written.
pub struct Optimisations {
    text_variables: BTreeSet<LocalId>,
}

impl Optimisations {
    /// Find the body's declared string variables. `var` parameters have
    /// the same local kind, but no declaration statement in the body.
    pub fn of(body: &Body) -> Self {
        let mut text_variables = BTreeSet::new();
        walk_statement_nodes(&body.statements, &mut |node| {
            if let Node::Statement(Stmt::Declare { local, .. }) = node
                && body.locals[*local].kind == LocalKind::Var
                && body.locals[*local].ty == Type::Scalar(Scalar::Str)
            {
                text_variables.insert(*local);
            }
        });
        Self { text_variables }
    }

    /// The pieces to append in place for a `str` variable's `+=` (TYPE-1,
    /// EXPR-3). More than one piece means the target can skip concatenation.
    /// With no plan, the original compound assignment preserves its reads.
    pub fn text_append<'s>(&self, program: &Program, statement: &'s Stmt) -> Option<Vec<&'s Expr>> {
        let Stmt::Assign {
            local,
            operator: Some(BinaryOperator::Add),
            value,
            ..
        } = statement
        else {
            return None;
        };
        if !self.text_variables.contains(local) {
            return None;
        }
        let core = program.core()?;
        let (left, rhs) = string_add(program, core, value)?;
        if !matches!(left.kind, ExprKind::Local(target) if target == *local) {
            return None;
        }

        let mut parts = Vec::new();
        let mut pending = vec![rhs];
        while let Some(part) = pending.pop() {
            if let Some((left, right)) = string_add(program, core, part) {
                pending.extend([right, left]);
            } else if matches!(part.kind, ExprKind::Literal(Value::Str(_)))
                || matches!(part.kind, ExprKind::Local(read) if read != *local)
            {
                // Locals hold independent values. The copy pass keeps an
                // earlier let independent even when it originally read this var.
                parts.push(part);
            } else {
                // Evaluate an alias or nontrivial expression as a single
                // RHS before appending. A block can write the target itself,
                // so keep the original read-before-RHS assignment there.
                let mut has_statements = false;
                rhs.walk_nodes(&mut |node| has_statements |= matches!(node, Node::Statement(_)));
                return (!has_statements).then(|| vec![rhs]);
            }
        }
        Some(parts)
    }
}

/// The checked builtin string `+`, with its inputs in evaluation order.
fn string_add<'a>(program: &Program, core: usize, value: &'a Expr) -> Option<(&'a Expr, &'a Expr)> {
    let ExprKind::Call {
        callee,
        arguments,
        form: Form::Binary(BinaryOperator::Add),
        lifted: false,
        ..
    } = &value.kind
    else {
        return None;
    };
    let [left, right] = arguments.as_slice() else {
        return None;
    };
    let (module, declaration) = callee.declaration()?;
    let text = Type::Scalar(Scalar::Str);
    (module == core
        && matches!(
            program.modules[module].declarations[declaration].kind,
            DeclarationKind::NativeCandidate(Role::Value, _)
        )
        && value.ty == text
        && left.value.ty == text
        && right.value.ty == text)
        .then_some((&left.value, &right.value))
}
