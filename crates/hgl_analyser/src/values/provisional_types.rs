//! GEN-7 and TYPE-18: the part of an inferred value's type that a selected
//! operator supplies. Known tuple positions, fields and independent generic
//! arguments remain known even when a sibling takes its type from the instance.

use std::collections::HashMap;

use hgl_diagnostics::Location;

use super::{Typer, candidates::Selecting, inference::infer, members::index_result};
use crate::hir::{self, Expr, ExprKind};
use crate::{Argument, Constant, ConstantValue, Detail, GenericKind, GenericParameter, Type};

impl Typer<'_> {
    /// Replace provisional call results with unknowns. None means the
    /// declaration already knows the result type of this expression.
    pub(super) fn provisional_type(&self, value: &Expr, since: usize) -> Option<Type> {
        let calls = self.selecting.borrow();
        let calls = &calls[since..];
        if self.provisional_locals.is_empty() && calls.is_empty() {
            return None;
        }
        let positions = calls.iter().map(|call| (call.at, call)).collect();
        self.project_result(value, &positions)
    }

    fn project_result(&self, value: &Expr, calls: &HashMap<Location, &Selecting>) -> Option<Type> {
        if let Some(call) = calls.get(&value.at) {
            // A different candidate can only change a part of the public
            // result left open by the operator signature and its inputs.
            let operator = call.operator;
            let Detail::Callable(public) =
                &self.program.modules[operator.module].details[operator.declaration]
            else {
                return Some(Type::Unknown);
            };
            let result = public.result.as_ref()?;
            if result.identity().is_some() {
                return (result != &value.ty).then(|| result.clone());
            }
            let mut found = vec![None; public.generics.len()];
            if let Some(written) = self.calls.get(&(value.at.file, value.at.span.start)) {
                for (slot, (_, arg)) in found.iter_mut().zip(&written.arguments) {
                    *slot = Some(arg.clone());
                }
            }
            for (slot, param) in public.parameters.iter().enumerate() {
                if let Some(arg) = call.argument_for(public, slot) {
                    let projected = if arg.at == value.at {
                        Some(Type::Unknown)
                    } else {
                        self.project_result(arg, calls)
                    };
                    infer(
                        &param.ty,
                        projected.as_ref().unwrap_or(&arg.ty),
                        &public.generics,
                        &mut found,
                    );
                }
            }
            let bindings: Vec<_> = public
                .generics
                .iter()
                .zip(found)
                .map(|(generic, bound)| bound.unwrap_or_else(|| unknown_arg(generic)))
                .collect();
            let map: Vec<_> = public
                .generics
                .iter()
                .map(|g| g.at)
                .zip(&bindings)
                .collect();
            let projected = self.program.specialize_type(result, &map);
            return (projected != value.ty || projected == Type::Unknown).then_some(projected);
        }
        let project = |part: &Expr| self.project_result(part, calls);
        if let Some(branches) = value.kind.branch_values() {
            return common_projection(branches.into_iter().filter_map(project));
        }
        let partial = match &value.kind {
            ExprKind::Local(id) | ExprKind::DeferredPresent(id) => {
                self.provisional_locals.get(id)?.clone()
            }
            ExprKind::Tuple(entries) => {
                let mut types = Vec::with_capacity(entries.len());
                let mut changed = false;
                for entry in entries {
                    let part = project(entry);
                    changed |= part.is_some();
                    types.push(part.unwrap_or_else(|| entry.ty.clone()));
                }
                if !changed {
                    return None;
                }
                Type::Tuple(types)
            }
            ExprKind::Sequence(entries) => {
                let element = common_projection(entries.iter().filter_map(project))?;
                match &value.ty {
                    Type::List { size, .. } => Type::List {
                        element: Box::new(element),
                        size: size.clone(),
                    },
                    Type::Set(_) => Type::Set(Box::new(element)),
                    _ => Type::Unknown,
                }
            }
            ExprKind::Pairs(entries) => {
                let key = common_projection(entries.iter().filter_map(|(key, _)| project(key)));
                let item = common_projection(entries.iter().filter_map(|(_, item)| project(item)));
                if key.is_none() && item.is_none() {
                    return None;
                }
                match &value.ty {
                    Type::Dict {
                        key: original_key,
                        value: original_value,
                    } => Type::Dict {
                        key: key.map_or_else(|| original_key.clone(), Box::new),
                        value: item.map_or_else(|| original_value.clone(), Box::new),
                    },
                    _ => Type::Unknown,
                }
            }
            ExprKind::Index { base, index } => {
                let base = project(base)?;
                let position = match &index.kind {
                    ExprKind::Literal(hir::Value::Int(index)) => Some(*index),
                    _ => None,
                };
                index_result(&base, position)
                    .cloned()
                    .unwrap_or(Type::Unknown)
            }
            ExprKind::Field { base, member } => {
                let base = project(base)?;
                if let Type::Named {
                    module,
                    declaration,
                    arguments,
                    ..
                } = &base
                    && let Detail::Struct(read) =
                        &self.program.modules[*module].details[*declaration]
                    && let Some(field) = read.members.get(*member)
                {
                    let map: Vec<_> = read.generics.iter().map(|g| g.at).zip(arguments).collect();
                    self.program.specialize_type(&field.ty, &map)
                } else {
                    Type::Unknown
                }
            }
            ExprKind::Call {
                callee, arguments, ..
            } if callee.declaration().is_some() => {
                let (module, declaration) = callee.declaration()?;
                let Detail::Callable(sig) = &self.program.modules[module].details[declaration]
                else {
                    return None;
                };
                let result = sig.result.as_ref()?;
                if result.identity().is_some() {
                    return None;
                }
                let mut projected = self.provisional_arguments(
                    arguments,
                    &sig.generics,
                    callee.generics(),
                    |slot| sig.parameters.get(slot).map(|p| &p.ty),
                    calls,
                )?;
                // Explicit function type arguments take precedence over inference.
                if matches!(callee, hir::Callee::Function { .. })
                    && let Some(written) = self.calls.get(&(value.at.file, value.at.span.start))
                {
                    for (slot, binding) in projected
                        .iter_mut()
                        .zip(callee.generics())
                        .take(written.arguments.len())
                    {
                        *slot = binding.clone();
                    }
                }
                let map: Vec<_> = sig.generics.iter().map(|g| g.at).zip(&projected).collect();
                self.program.specialize_type(result, &map)
            }
            ExprKind::Construct {
                module,
                declaration,
                arguments,
                infer_types,
            } => {
                if crate::constant::type_has_instance_query(&value.ty) {
                    return Some(value.ty.clone());
                }
                if !infer_types {
                    return None;
                }
                let Detail::Struct(read) = &self.program.modules[*module].details[*declaration]
                else {
                    return None;
                };
                let mut ty = value.ty.clone();
                let Type::Named {
                    arguments: given, ..
                } = &mut ty
                else {
                    return None;
                };
                *given = self.provisional_arguments(
                    arguments,
                    &read.generics,
                    given,
                    |slot| read.members.get(slot).map(|m| &m.ty),
                    calls,
                )?;
                ty
            }
            _ => {
                let mut found = false;
                value.walk(&mut |part| {
                    found |= calls.contains_key(&part.at) || matches!(part.kind,
                        ExprKind::Local(id) | ExprKind::DeferredPresent(id) if self.provisional_locals.contains_key(&id));
                });
                if !found {
                    return None;
                }
                Type::Unknown
            }
        };
        (partial != value.ty || partial == Type::Unknown).then_some(partial)
    }

    /// Reuse ordinary GEN-2 inference with the partially known operands.
    /// Preserve any binding which does not depend on such an operand.
    fn provisional_arguments<'t>(
        &self,
        args: &[hir::Argument],
        generics: &[GenericParameter],
        given: &[Argument],
        formal: impl Fn(usize) -> Option<&'t Type>,
        calls: &HashMap<Location, &Selecting>,
    ) -> Option<Vec<Argument>> {
        let mut found = vec![None; generics.len()];
        let mut dependent = vec![false; generics.len()];
        for arg in args {
            let Some(pattern) = arg.parameter.and_then(&formal) else {
                continue;
            };
            let projected = self.project_result(&arg.value, calls);
            if projected.is_some() {
                for (flag, generic) in dependent.iter_mut().zip(generics) {
                    *flag |= crate::declared::mentions(pattern, &[generic.at]);
                }
            }
            infer(
                pattern,
                projected.as_ref().unwrap_or(&arg.value.ty),
                generics,
                &mut found,
            );
        }
        if !dependent.iter().any(|flag| *flag) {
            return None;
        }
        Some(
            given
                .iter()
                .zip(generics)
                .zip(found.into_iter().zip(dependent))
                .map(|((original, generic), (inferred, affected))| {
                    if !affected {
                        return original.clone();
                    }
                    inferred.unwrap_or_else(|| unknown_arg(generic))
                })
                .collect(),
        )
    }
}

/// A branch or homogeneous collection may receive any of its projected
/// result types. Agreeing projections preserve the common partially known type.
fn common_projection(types: impl Iterator<Item = Type>) -> Option<Type> {
    types.reduce(common_type)
}

/// Keep the shape and slots on which two possible result types agree.
fn common_type(mut left: Type, right: Type) -> Type {
    if left == right {
        return left;
    }
    let merge = |a: &mut Type, b| *a = common_type(std::mem::replace(a, Type::Unknown), b);
    match (&mut left, right) {
        (Type::Tuple(left), Type::Tuple(right)) if left.len() == right.len() => {
            for (a, b) in left.iter_mut().zip(right) {
                merge(a, b);
            }
        }
        (
            Type::List {
                element: a,
                size: x,
            },
            Type::List {
                element: b,
                size: y,
            },
        ) if *x == y => merge(a, *b),
        (Type::Set(a), Type::Set(b))
        | (Type::Nullable(a), Type::Nullable(b))
        | (Type::Atomic(a), Type::Atomic(b))
        | (Type::Endpoint(a), Type::Endpoint(b))
        | (Type::Delta(a), Type::Delta(b)) => merge(a, *b),
        (Type::Dict { key: a, value: x }, Type::Dict { key: b, value: y })
        | (Type::Result { value: a, error: x }, Type::Result { value: b, error: y }) => {
            merge(a, *b);
            merge(x, *y);
        }
        (
            Type::Named {
                module: lm,
                declaration: ld,
                arguments: x,
                ..
            },
            Type::Named {
                module: rm,
                declaration: rd,
                arguments: y,
                ..
            },
        ) if (*lm, *ld) == (rm, rd) && x.len() == y.len() => {
            for (a, b) in x.iter_mut().zip(y) {
                match (a, b) {
                    (Argument::Type(a), Argument::Type(b)) => merge(a, b),
                    (Argument::Constant(a), Argument::Constant(b)) if *a != b => {
                        *a = Constant {
                            text: "_".into(),
                            value: ConstantValue::Unknown,
                        };
                    }
                    (a, b) if *a == b => {}
                    _ => return Type::Unknown,
                }
            }
        }
        _ => return Type::Unknown,
    }
    left
}

fn unknown_arg(generic: &GenericParameter) -> Argument {
    match generic.kind {
        GenericKind::Value(_) => Argument::Constant(Constant {
            text: "_".into(),
            value: ConstantValue::Unknown,
        }),
        _ => Argument::Type(Type::Unknown),
    }
}
