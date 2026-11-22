//! Compile-time queries of an operator's declared laws (GEN-9, GEN-18).
//!
//! A property is a contract of the public operator and concrete domain,
//! independent of the candidate selected to implement its call. Nothing here
//! proves a law or authorizes an optimizer to regroup arithmetic.

use hgl_parser::{SyntaxKind, SyntaxNode};

use crate::declared::{names_parameter, same_type};
use crate::hir::Callee;
use crate::program::Program;
use crate::syntax::{child, name_token, tokens, ungrouped};
use crate::types::{FunctionType, Signature, Type};

/// The two law predicates defined by GEN-18.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Law {
    /// The operator permits regrouping under its declared contract.
    Associative,
    /// The operator declares that its operand order may be exchanged.
    Commutative,
}

impl Law {
    fn name(self) -> &'static str {
        match self {
            Self::Associative => "associative",
            Self::Commutative => "commutative",
        }
    }
}

/// A GEN-18 predicate of one bare function parameter, in a requirement
/// or generic default. Other call forms retain ordinary name resolution.
pub(crate) fn predicate(node: &SyntaxNode) -> Option<(Law, String)> {
    let node = ungrouped(node.clone());
    let (name, operands) = match node.kind() {
        SyntaxKind::ConstraintCall => {
            if child(&node, SyntaxKind::QualifiedName).is_some()
                || child(&node, SyntaxKind::ReturnType).is_some()
            {
                return None;
            }
            (name_token(&node)?, node.children().collect::<Vec<_>>())
        }
        SyntaxKind::CallExpr => {
            let callee = child(&node, SyntaxKind::NameExpr)?;
            if callee.children().next().is_some() {
                return None;
            }
            let list = child(&node, SyntaxKind::ArgList)?;
            let operands = list
                .children()
                .map(|arg| {
                    // The predicate has no named argument or spread.
                    if tokens(&arg).any(|token| {
                        matches!(token.kind(), SyntaxKind::Colon | SyntaxKind::Ellipsis)
                    }) {
                        return None;
                    }
                    arg.children().next()
                })
                .collect::<Option<Vec<_>>>()?;
            (name_token(&callee)?, operands)
        }
        _ => return None,
    };
    let law = match name.text() {
        "associative" => Law::Associative,
        "commutative" => Law::Commutative,
        _ => return None,
    };
    let [operand] = operands.as_slice() else {
        return None;
    };
    if !matches!(operand.kind(), SyntaxKind::NamedType | SyntaxKind::NameExpr)
        || operand.children().next().is_some()
    {
        return None;
    }
    Some((law, name_token(operand)?.text().to_string()))
}

/// The function parameter a well-formed predicate asks about.
pub(crate) fn parameter(signature: &Signature, name: &str) -> Option<usize> {
    signature
        .parameters
        .iter()
        .position(|parameter| parameter.name == name && matches!(parameter.ty, Type::Function(_)))
}

/// Whether a generic default is the exception UNIT-8 permits to read
/// one of this declaration's formal function parameters.
pub(crate) fn default_parameter(signature: &Signature, node: &SyntaxNode) -> Option<usize> {
    let (_, name) = predicate(node)?;
    parameter(signature, &name)
}

/// The operator's metadata at this concrete function type. A forwarded
/// function parameter is unsettled until its caller supplies the function.
pub(crate) fn holds(
    program: &Program,
    law: Law,
    callback: &Callee,
    template: &FunctionType,
) -> Option<bool> {
    let operator = match callback {
        Callee::Candidate { operator, .. } | Callee::Required { operator, .. } => *operator,
        Callee::Parameter { .. } | Callee::DeferredOperator(_) | Callee::Unread => return None,
        _ => return Some(false),
    };
    if template.parameters.is_empty()
        || template.parameters.iter().any(|parameter| {
            names_parameter(parameter.ty()) || matches!(parameter.ty(), Type::Unknown)
        })
    {
        return None;
    }
    let module = &program.modules[operator.0];
    let declaration = &module.declarations[operator.1];
    let file = module.files[declaration.file].file;
    for clause in declaration
        .syntax
        .children()
        .filter(|part| part.kind() == SyntaxKind::OperatorProperties)
    {
        if !clause
            .children()
            .filter(|part| part.kind() == SyntaxKind::OperatorProperty)
            .any(|property| name_token(&property).is_some_and(|name| name.text() == law.name()))
        {
            continue;
        }
        for domain in clause
            .children()
            .filter(|part| crate::read::is_type(part.kind()))
        {
            let Some(domain) = module
                .written
                .iter()
                .find(|written| {
                    written.at.file == file && written.at.span == hgl_parser::HasSpan::span(&domain)
                })
                .map(|written| &written.ty)
            else {
                continue;
            };
            if !names_parameter(domain)
                && template
                    .parameters
                    .iter()
                    .all(|parameter| same_type(domain, parameter.ty()) == Some(true))
            {
                return Some(true);
            }
        }
    }
    Some(false)
}

/// Query the concrete operator supplied as one static function argument.
pub(crate) fn value(program: &Program, law: Law, expression: &crate::hir::Expr) -> Option<bool> {
    let Type::Function(template) = &expression.ty else {
        return None;
    };
    let crate::hir::ExprKind::StaticCallable(callback) = &expression.kind else {
        return None;
    };
    holds(program, law, callback, template)
}

/// Resolve each law query in a condition using this call's formal slots.
pub(crate) fn verdicts<'a>(
    evaluate: &dyn Fn(Law, &crate::hir::Expr) -> Option<bool>,
    signature: &Signature,
    condition: &SyntaxNode,
    argument: &dyn Fn(usize) -> Option<&'a crate::hir::Expr>,
) -> Vec<(u32, Option<bool>)> {
    use hgl_parser::HasSpan;
    condition
        .descendants()
        .filter_map(|node| {
            let (law, name) = predicate(&node)?;
            let answer = parameter(signature, &name)
                .and_then(argument)
                .and_then(|expression| evaluate(law, expression));
            Some((node.span().start, answer))
        })
        .collect()
}

/// Whether this operator's public declaration or one of its candidates
/// asks for a law where a static function identity is needed (GEN-18).
pub(crate) fn operator_needs_identity(
    program: &Program,
    operator: crate::program::OperatorRef,
) -> bool {
    program.modules.iter().enumerate().any(|(module, unit)| {
        unit.declarations
            .iter()
            .enumerate()
            .any(|(index, declaration)| {
                let belongs = (module, index) == (operator.module, operator.declaration)
                    || declaration
                        .kind
                        .candidate()
                        .is_some_and(|(_, target)| target == Some(operator));
                belongs
                    && declaration
                        .syntax
                        .descendants()
                        .any(|node| predicate(&node).is_some())
            })
    })
}

/// A law-dependent body must be read with the function its caller supplies.
/// Its declaration-level type template cannot determine that metadata.
pub(crate) fn has_dependent_calls(program: &Program, body: &crate::hir::Body) -> bool {
    use crate::hir::ExprKind;
    let mut dependent = false;
    crate::hir::walk_statements(&body.statements, &mut |expression| {
        let ExprKind::Call {
            callee, arguments, ..
        } = &expression.kind
        else {
            return;
        };
        if !arguments.iter().any(|argument| matches!(&argument.value.kind,
            ExprKind::StaticCallable(callback) if matches!(callback.as_ref(), Callee::Parameter { .. }))) { return; }
        dependent |= match callee {
            Callee::DeferredOperator(deferred) => {
                operator_needs_identity(program, deferred.identity)
            }
            _ => callee.declaration().is_some_and(|(module, declaration)| {
                program.modules[module].declarations[declaration]
                    .syntax
                    .descendants()
                    .any(|node| predicate(&node).is_some())
            }),
        };
    });
    dependent
}

/// A law stated as a positive conjunct (or its negation), GEN-7 and GEN-18.
pub(crate) fn stated(
    signature: &Signature,
    declaration: &SyntaxNode,
    parameter: usize,
    law: Law,
) -> Option<bool> {
    fn term(node: &SyntaxNode, name: &str, law: Law) -> Option<bool> {
        match node.kind() {
            SyntaxKind::ConstraintParen => term(&node.children().next()?, name, law),
            SyntaxKind::ConstraintBinary
                if tokens(node).any(|token| token.kind() == SyntaxKind::AmpAmp) =>
            {
                node.children().find_map(|side| term(&side, name, law))
            }
            SyntaxKind::ConstraintNot => {
                let (other, function) = predicate(&node.children().next()?)?;
                (other == law && function == name).then_some(false)
            }
            _ => {
                let (other, function) = predicate(node)?;
                (other == law && function == name).then_some(true)
            }
        }
    }
    let condition = child(declaration, SyntaxKind::RequiresClause)?
        .children()
        .next()?;
    term(&condition, &signature.parameters.get(parameter)?.name, law)
}

/// GEN-9: property domains are concrete ordinary value types. The
/// written-type pass already enforces the ordinary-value restriction.
pub(crate) fn check_domains(program: &Program, diagnostics: &mut hgl_diagnostics::Diagnostics) {
    use hgl_parser::HasSpan;
    for module in &program.modules {
        for declaration in &module.declarations {
            for clause in declaration
                .syntax
                .children()
                .filter(|node| node.kind() == SyntaxKind::OperatorProperties)
            {
                for domain in clause
                    .children()
                    .filter(|node| crate::read::is_type(node.kind()))
                {
                    if module.written.iter().any(|written| {
                        written.at.file == declaration.at.file
                            && written.at.span.start == domain.span().start
                            && names_parameter(&written.ty)
                    }) {
                        diagnostics.emit(crate::local_error(
                            hgl_diagnostics::Category::Type,
                            crate::codes::local::PROPERTY_DOMAIN,
                            hgl_diagnostics::Location::new(declaration.at.file, domain.span()),
                            "an operator property domain must be a concrete type".to_string(),
                            "GEN-9",
                        ));
                    }
                }
            }
        }
    }
}

/// Whether a trial assignment is excluded by the wrapper's own Boolean
/// law conditions (GEN-7). Unrelated type/value constraints remain unknown.
pub(crate) fn allows(
    signature: &Signature,
    declaration: &SyntaxNode,
    answer: &dyn Fn(usize, Law) -> Option<bool>,
) -> bool {
    fn evaluate(
        node: &SyntaxNode,
        signature: &Signature,
        answer: &dyn Fn(usize, Law) -> Option<bool>,
    ) -> Option<bool> {
        match node.kind() {
            SyntaxKind::ConstraintParen => evaluate(&node.children().next()?, signature, answer),
            SyntaxKind::ConstraintNot => {
                evaluate(&node.children().next()?, signature, answer).map(|value| !value)
            }
            SyntaxKind::ConstraintBinary => {
                let mut parts = node.children();
                let left = evaluate(&parts.next()?, signature, answer);
                let right = evaluate(&parts.next()?, signature, answer);
                let both = tokens(node).any(|token| token.kind() == SyntaxKind::AmpAmp);
                match (left, right) {
                    (Some(a), Some(b)) => Some(if both { a && b } else { a || b }),
                    (Some(false), _) | (_, Some(false)) if both => Some(false),
                    (Some(true), _) | (_, Some(true)) if !both => Some(true),
                    _ => None,
                }
            }
            _ => {
                let (law, name) = predicate(node)?;
                answer(parameter(signature, &name)?, law)
            }
        }
    }
    child(declaration, SyntaxKind::RequiresClause)
        .and_then(|clause| clause.children().next())
        .and_then(|condition| evaluate(&condition, signature, answer))
        != Some(false)
}
