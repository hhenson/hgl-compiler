//! Which compile-time function parameters a value body invokes (FN-17).
//!
//! The summary holds only a slot and the slots given at its invocation.
//! Passing a callable to a body that does not invoke it adds no use. A
//! recursive wrapper can repeat a use, but cannot enlarge the source
//! body's set by producing another runtime instance.

use std::collections::{BTreeSet, HashMap};

use crate::hir::{Argument, Body, Callee, Expr, ExprKind, walk_statements};
use crate::program::{Program, Role};
use crate::types::{Detail, Type};

use super::Declared;

/// One use of a callable parameter and the argument slots it receives.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Invocation {
    slot: usize,
    supplied: Vec<usize>,
}

/// Calls of the function parameters of each value declaration.
#[derive(Default)]
pub(super) struct CallbackUses {
    bodies: HashMap<Declared, BTreeSet<Invocation>>,
}

impl CallbackUses {
    /// Propagate each invocation through the written calls that can reach it.
    pub(super) fn of(program: &Program) -> Self {
        let declarations: Vec<_> = program
            .modules
            .iter()
            .enumerate()
            .flat_map(|(module, unit)| {
                unit.bodies
                    .iter()
                    .enumerate()
                    .filter_map(move |(index, body)| {
                        let body = body.as_ref()?;
                        let Detail::Callable(signature) = unit.details.get(index)? else {
                            return None;
                        };
                        (unit.declarations[index].kind.role() == Some(Role::Value)
                            && signature
                                .parameters
                                .iter()
                                .any(|parameter| matches!(parameter.ty, Type::Function(_))))
                        .then_some(((module, index), body))
                    })
            })
            .collect();
        let mut calls = Self::default();
        loop {
            let mut changed = false;
            for &(declaration, body) in &declarations {
                let mut found = BTreeSet::new();
                calls.gather(body, &mut found);
                let known = calls.bodies.entry(declaration).or_default();
                let before = known.len();
                known.extend(found);
                changed |= known.len() != before;
            }
            if !changed {
                return calls;
            }
        }
    }

    /// The concrete callable arguments that this call's body invokes.
    pub(super) fn of_call<'a>(
        &'a self,
        called: Declared,
        arguments: &'a [Argument],
    ) -> impl Iterator<Item = (&'a Callee, &'a [usize])> + 'a {
        self.bodies
            .get(&called)
            .into_iter()
            .flatten()
            .filter_map(move |used| {
                Some((
                    supplied_target(arguments, used.slot)?,
                    used.supplied.as_slice(),
                ))
            })
    }

    fn gather(&self, body: &Body, found: &mut BTreeSet<Invocation>) {
        walk_statements(&body.statements, &mut |expression| {
            self.gather_call(expression, found)
        });
    }

    fn gather_call(&self, expression: &Expr, found: &mut BTreeSet<Invocation>) {
        let ExprKind::Call {
            callee, arguments, ..
        } = &expression.kind
        else {
            return;
        };
        match callee {
            Callee::Parameter {
                parameter,
                temporal: false,
            } => {
                let mut supplied: Vec<_> = arguments
                    .iter()
                    .filter_map(|argument| argument.parameter)
                    .collect();
                supplied.sort_unstable();
                supplied.dedup();
                found.insert(Invocation {
                    slot: *parameter,
                    supplied,
                });
            }
            Callee::Anonymous(callback) => self.gather(&callback.body, found),
            _ => {
                let Some(called) = callee.declaration() else {
                    return;
                };
                for (target, supplied) in self.of_call(called, arguments) {
                    match target {
                        Callee::Parameter {
                            parameter,
                            temporal: false,
                        } => {
                            found.insert(Invocation {
                                slot: *parameter,
                                supplied: supplied.to_vec(),
                            });
                        }
                        Callee::Anonymous(callback) => self.gather(&callback.body, found),
                        _ => {}
                    }
                }
            }
        }
    }
}

fn supplied_target(arguments: &[Argument], slot: usize) -> Option<&Callee> {
    arguments
        .iter()
        .find_map(|argument| match &argument.value.kind {
            ExprKind::StaticCallable(target) if argument.parameter == Some(slot) => {
                Some(target.as_ref())
            }
            _ => None,
        })
}
