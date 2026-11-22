//! Compile-time queries of an operator's declared laws (GEN-9, GEN-18).
//!
//! A property is a contract of the public operator and concrete domain,
//! independent of the candidate selected to implement its call. Nothing here
//! proves a law or authorizes an optimizer to regroup arithmetic.

use hgl_diagnostics::Location;
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode};
use hgl_source::FileId;

use crate::declared::{names_parameter, same_type};
use crate::hir::Callee;
use crate::program::{Module, Program};
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

fn query_node(node: &SyntaxNode) -> Option<SyntaxNode> {
    let mut node = ungrouped(node.clone());
    // GEN-15: constraint grouping preserves a law predicate's meaning,
    // including when the grouped predicate follows a negation.
    while node.kind() == SyntaxKind::ConstraintParen {
        node = ungrouped(node.children().next()?);
    }
    Some(node)
}

/// A GEN-18 predicate of one bare function parameter, in a requirement
/// or generic default. Other call forms retain ordinary name resolution.
pub(crate) fn predicate(node: &SyntaxNode) -> Option<(Law, String)> {
    let node = query_node(node)?;
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

/// A GEN-18 requirement only where ordinary name resolution binds no
/// operator to its callee (UNIT-10, GEN-16).
pub(crate) fn requirement(
    module: &Module,
    file: FileId,
    node: &SyntaxNode,
) -> Option<(Law, String)> {
    let node = query_node(node)?;
    if node.kind() != SyntaxKind::ConstraintCall {
        return None;
    }
    let name = name_token(&node)?;
    if module
        .uses
        .iter()
        .any(|used| used.at == Location::new(file, name.span()))
    {
        return None;
    }
    predicate(&node)
}

/// Recognize a law query at a call: requirements honor the binding of
/// their callee; defaults retain GEN-18's special formal-argument query.
pub(crate) fn query(module: &Module, file: FileId, node: &SyntaxNode) -> Option<(Law, String)> {
    let node = query_node(node)?;
    if node.kind() == SyntaxKind::ConstraintCall {
        requirement(module, file, &node)
    } else {
        predicate(&node)
    }
}

/// GEN-18: a law requirement names a formal parameter that takes a
/// function. Reject an invalid operand where the requirement is declared.
pub(crate) fn check_requirements(
    program: &Program,
    diagnostics: &mut hgl_diagnostics::Diagnostics,
) {
    for module in &program.modules {
        for (declaration, detail) in module.declarations.iter().zip(&module.details) {
            let crate::types::Detail::Callable(signature) = detail else {
                continue;
            };
            let Some(condition) = child(&declaration.syntax, SyntaxKind::RequiresClause) else {
                continue;
            };
            for node in condition
                .descendants()
                .filter(|node| node.kind() == SyntaxKind::ConstraintCall)
            {
                let Some((_, name)) = requirement(module, declaration.at.file, &node) else {
                    continue;
                };
                if parameter(signature, &name).is_none() {
                    diagnostics.emit(crate::error(
                        hgl_diagnostics::Category::Type,
                        crate::codes::TYPE_MISMATCH,
                        Location::new(declaration.at.file, node.span()),
                        "a law requirement must query a formal parameter that takes a function"
                            .to_string(),
                        "GEN-18",
                    ));
                }
            }
        }
    }
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

/// The operator's metadata at this concrete function type. A formal
/// function has no metadata here; its declaration must state a guarantee.
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

/// Collect this call's answers to the built-in law requirements. The
/// query returns no entry for an ordinary named-operator requirement.
pub(crate) fn verdicts(
    condition: &SyntaxNode,
    query: &dyn Fn(&SyntaxNode) -> Option<Option<bool>>,
) -> Vec<(u32, Option<bool>)> {
    condition
        .descendants()
        .filter(|node| node.kind() == SyntaxKind::ConstraintCall)
        .filter_map(|node| Some((node.span().start, query(&node)?)))
        .collect()
}

/// A law stated as a positive conjunct (or its negation), GEN-7 and GEN-18.
pub(crate) fn stated(
    context: (&Module, FileId),
    signature: &Signature,
    declaration: &SyntaxNode,
    parameter: usize,
    law: Law,
) -> Option<bool> {
    fn term(context: (&Module, FileId), node: &SyntaxNode, name: &str, law: Law) -> Option<bool> {
        match node.kind() {
            SyntaxKind::ConstraintParen => term(context, &node.children().next()?, name, law),
            SyntaxKind::ConstraintBinary
                if tokens(node).any(|token| token.kind() == SyntaxKind::AmpAmp) =>
            {
                node.children()
                    .find_map(|side| term(context, &side, name, law))
            }
            SyntaxKind::ConstraintNot => {
                let (other, function) =
                    requirement(context.0, context.1, &node.children().next()?)?;
                (other == law && function == name).then_some(false)
            }
            _ => {
                let (other, function) = requirement(context.0, context.1, node)?;
                (other == law && function == name).then_some(true)
            }
        }
    }
    let condition = child(declaration, SyntaxKind::RequiresClause)?
        .children()
        .next()?;
    term(
        context,
        &condition,
        &signature.parameters.get(parameter)?.name,
        law,
    )
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
