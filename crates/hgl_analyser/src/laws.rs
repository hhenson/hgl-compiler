//! Compile-time queries of an operator's declared laws (GEN-9, GEN-18).
//!
//! A property is a contract of the public operator and concrete domain,
//! independent of the candidate selected to implement its call. Nothing here
//! proves a law or authorizes an optimizer to regroup arithmetic.

use std::collections::HashSet;

use hgl_diagnostics::{Category, Diagnostics, Location};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode};
use hgl_source::FileId;

use crate::declared::{names_parameter, same_type};
use crate::hir::Callee;
use crate::program::{Module, Program};
use crate::syntax::{child, name_token, tokens, ungrouped};
use crate::types::{ConstantValue, FunctionType, Signature, Type};
use crate::{codes, error, local_error};

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
        Callee::Parameter { .. }
        | Callee::DeferredOperator(_)
        | Callee::UnreadWithConnection
        | Callee::Unread => return None,
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
    for clause in clauses(&declaration.syntax) {
        if !clause
            .children()
            .filter(|part| part.kind() == SyntaxKind::OperatorProperty)
            .any(|property| name_token(&property).is_some_and(|name| name.text() == law.name()))
        {
            continue;
        }
        for domain in domains(&clause) {
            let Some(domain) = module
                .written
                .iter()
                .find(|written| written.at == Location::new(file, domain.span()))
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

/// The GEN-9 property clauses written on an operator.
pub(crate) fn clauses(declaration: &SyntaxNode) -> impl Iterator<Item = SyntaxNode> {
    declaration
        .children()
        .filter(|node| node.kind() == SyntaxKind::OperatorProperties)
}

/// Each domain a GEN-9 property clause names, in source order.
pub(crate) fn domains(clause: &SyntaxNode) -> impl Iterator<Item = SyntaxNode> {
    clause
        .children()
        .filter(|node| crate::read::is_type(node.kind()))
}

/// GEN-9: domains are concrete ordinary value types named once per
/// operator, and each clause states a property at most once. The
/// written-type pass already enforces the ordinary-value restriction.
pub(crate) fn check_properties(program: &Program, diagnostics: &mut Diagnostics) {
    for module in &program.modules {
        for declaration in &module.declarations {
            let mut seen: Vec<&Type> = Vec::new();
            for clause in clauses(&declaration.syntax) {
                let earlier = seen.len();
                for domain in domains(&clause) {
                    let at = Location::new(declaration.at.file, domain.span());
                    let Some(ty) = module
                        .written
                        .iter()
                        .find(|written| written.at == at)
                        .map(|written| &written.ty)
                    else {
                        continue;
                    };
                    if names_parameter(ty) {
                        diagnostics.emit(local_error(
                            Category::Type,
                            codes::local::PROPERTY_DOMAIN,
                            at,
                            "an operator property domain must be a concrete type".to_string(),
                            "GEN-9",
                        ));
                        continue;
                    }
                    if ty.any_part(&|part| matches!(part, Type::Unknown), &|constant| {
                        matches!(constant.value, ConstantValue::Unknown)
                    }) {
                        continue;
                    }
                    if let Some(first) = seen
                        .iter()
                        .position(|previous| same_type(previous, ty) == Some(true))
                    {
                        let clause = if first < earlier {
                            "an earlier property clause"
                        } else {
                            "this property clause"
                        };
                        diagnostics.emit(error(
                            Category::Operator,
                            codes::PROPERTY_DUPLICATE,
                            at,
                            format!("{clause} already names `{ty}`"),
                            "GEN-9",
                        ));
                    }
                    seen.push(ty);
                }
                let mut properties = HashSet::new();
                for name in clause
                    .children()
                    .filter(|part| part.kind() == SyntaxKind::OperatorProperty)
                    .filter_map(|property| name_token(&property))
                {
                    if !properties.insert(name.text().to_owned()) {
                        diagnostics.emit(error(
                            Category::Operator,
                            codes::PROPERTY_DUPLICATE,
                            Location::new(declaration.at.file, name.span()),
                            format!("this property clause already gives `{}`", name.text()),
                            "GEN-9",
                        ));
                    }
                }
            }
        }
    }
}
