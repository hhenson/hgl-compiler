//! Contextual reads of a connection at a temporal node's input
//! (ERR-12 and ERR-15). The C++ target writes a wiring variant for a
//! node when a call gives it an input with a different physical
//! schema. Its handlers and place types are those of the HGL node.

use hgl_lower::calls::runtime_arguments;
use std::collections::BTreeMap;

use hgl_analyser::hir::{Body, Expr, ExprKind, Raise, walk_statements};
use hgl_analyser::{Detail, Phase, Program, Type};
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
                let Some((key, reads)) = node_reads(program, instances, value) else {
                    return;
                };
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
                    && !instances.instanced(program, module, declaration)
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
        let (key, reads) = node_reads(program, instances, value)?;
        let variants = self.variants.get(&key)?;
        let number = variants.iter().position(|variant| *variant == reads)?;
        Some((number, &variants[number]))
    }
}

/// The resolved node and the reads of its concrete inputs (FN-8,
/// ERR-12, ERR-15). A `ref` parameter designates the projected endpoint.
fn node_reads(
    program: &Program,
    instances: &Instances,
    value: &Expr,
) -> Option<(InstanceKey, Vec<Reading>)> {
    let ExprKind::Call {
        callee, arguments, ..
    } = &value.kind
    else {
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
    // FN-16: a designation must point at the projected endpoint itself.
    if signature.reference {
        return None;
    }
    let instance = if instances.instanced(program, module, declaration) {
        Some(instances.find_call(program, callee, arguments)?)
    } else {
        None
    };
    let signature = instance.map_or(signature, |instance| &instance.signature);
    let reads = runtime_arguments(arguments)
        .filter_map(|(_, argument)| {
            let parameter = instance.map_or_else(
                || argument.parameter.filter(|_| argument.pack.is_none()),
                |instance| instance.parameter(argument),
            )?;
            let declared = signature.parameters.get(parameter)?;
            if declared.constant
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
        .collect();
    let suffix = instance.map(|instance| instance.suffix.clone());
    Some(((module, declaration, suffix), reads))
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
