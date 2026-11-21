//! Contextual reads of a connection at a temporal node's input
//! (ERR-12 and ERR-15). The C++ target writes a wiring variant for a
//! node when a call gives it an input with a different physical
//! schema. Its handlers and place types are those of the HGL node.

use std::collections::BTreeMap;

use hgl_analyser::hir::{Body, Expr, ExprKind, Raise, walk_statements};
use hgl_analyser::{Detail, Phase, Program, Signature, Type};
use hgl_lower::instances::Instances;

use super::lifting::{reader_inner, reader_operand};

/// A call's recorded input read and the parameter it supplies.
#[derive(Clone, PartialEq)]
pub(super) struct Reading {
    /// The parameter position in the node's declared signature.
    pub(super) parameter: usize,
    /// The read, including the connection it reads.
    pub(super) form: Expr,
}

/// Locate the result-to-value read in a consumer's recorded forms.
/// ERR-15 raises on the Err publication even if no handler reads it.
pub(super) fn watches_failure(value: &Expr) -> Option<&Expr> {
    match value.kind {
        ExprKind::Raised {
            raise: Raise::Reader,
            ..
        } => Some(value),
        _ => reader_inner(value).and_then(watches_failure),
    }
}

type InstanceKey = (usize, usize, Option<String>);

/// Physical input variants used by calls in the program, including
/// those in resolved generic bodies (GEN-2).
#[derive(Default)]
pub(crate) struct Readers {
    variants: BTreeMap<InstanceKey, Vec<Vec<Reading>>>,
}

impl Readers {
    /// Read the HIR only once to collect the variants whose wiring
    /// declarations must be available across C++ translation units.
    pub(crate) fn of(program: &Program, instances: &Instances) -> Self {
        let mut readers = Self::default();
        let mut collect = |body: &Body| {
            walk_statements(&body.statements, &mut |value| {
                let Some((key, signature)) = node(program, instances, value) else {
                    return;
                };
                let reads = reads(value, signature);
                if reads.is_empty() {
                    return;
                }
                let variants = readers.variants.entry(key).or_default();
                if !variants.contains(&reads) {
                    variants.push(reads);
                }
            });
        };
        for (module, unit) in program.modules.iter().enumerate() {
            for (declaration, body) in unit.bodies.iter().enumerate() {
                if let Some(body) = body
                    && matches!(unit.details.get(declaration), Some(Detail::Callable(signature)) if signature.generics.is_empty())
                {
                    collect(body);
                }
                for instance in instances.declared(module, declaration) {
                    collect(&instance.body);
                }
            }
            for body in unit.test_bodies.iter().flatten() {
                collect(body);
            }
        }
        readers
    }

    /// The wiring variants this declaration or instance writes.
    pub(super) fn declared(
        &self,
        module: usize,
        declaration: usize,
        instance: Option<&str>,
    ) -> &[Vec<Reading>] {
        self.variants
            .get(&(module, declaration, instance.map(str::to_string)))
            .map_or(&[], Vec::as_slice)
    }

    /// The wiring variant and its physical reads for this HIR call.
    pub(super) fn at_call(
        &self,
        program: &Program,
        instances: &Instances,
        value: &Expr,
    ) -> Option<(usize, &[Reading])> {
        let (key, signature) = node(program, instances, value)?;
        let reads = reads(value, signature);
        let variants = self.variants.get(&key)?;
        let number = variants.iter().position(|variant| *variant == reads)?;
        Some((number, &variants[number]))
    }
}

/// The resolved declaration whose body is a node.
fn node<'a>(
    program: &'a Program,
    instances: &'a Instances,
    value: &Expr,
) -> Option<(InstanceKey, &'a Signature)> {
    let ExprKind::Call { callee, .. } = &value.kind else {
        return None;
    };
    let (module, declaration) = callee.declaration()?;
    let unit = &program.modules[module];
    if unit.phases.get(declaration).copied().flatten() != Some(Phase::Node)
        || unit.is_source(declaration)
    {
        return None;
    }
    let Detail::Callable(signature) = unit.details.get(declaration)? else {
        return None;
    };
    if signature.generics.is_empty() {
        Some(((module, declaration, None), signature))
    } else {
        let instance = instances.find(program, (module, declaration), callee.generics())?;
        Some((
            (module, declaration, Some(instance.suffix.clone())),
            &instance.signature,
        ))
    }
}

/// A direct node reads these operands in its handlers. A `ref`
/// parameter instead designates the endpoint of the projected port.
fn reads(value: &Expr, signature: &Signature) -> Vec<Reading> {
    let ExprKind::Call { arguments, .. } = &value.kind else {
        return Vec::new();
    };
    arguments
        .iter()
        .filter_map(|argument| {
            let parameter = argument.parameter?;
            let declared = signature.parameters.get(parameter)?;
            if argument.pack.is_some()
                || declared.constant
                || declared.reference
                || !direct_read(&argument.value.ty)
                || reader_inner(&argument.value).is_none()
                || !reader_operand(&argument.value).is_connection()
            {
                return None;
            }
            Some(Reading {
                parameter,
                form: argument.value.clone(),
            })
        })
        .collect()
}

/// Whole publications and their deltas can be read directly for
/// scalar endpoints. A composite needs an endpoint of its own to
/// track projected children and their deltas (TYPE-9, TYPE-16).
fn direct_read(ty: &Type) -> bool {
    match ty {
        Type::Nullable(inner) => Type::delta((**inner).clone()) == **inner,
        _ => Type::delta(ty.clone()) == *ty,
    }
}
