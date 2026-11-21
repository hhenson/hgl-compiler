//! A graph was checked under its generic requirements (GEN-7). Check
//! the destinations that depended on a category or type not then known.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use hgl_diagnostics::{Diagnostics, Location};

use super::arguments::Takes;
use super::inference::type_pack;
use super::lifting::{assignment_category, record};
use super::statements::place_assignment_category;
use super::{Tables, Typer};
use crate::declared::substitute_within;
use crate::hir::{self, Body, Callee, Expr, ExprKind, LocalId, Node, NodeMut, Stmt, ValueCategory};
use crate::program::{Program, Role};
use crate::types::{Detail, Signature, Type};

/// Expression types whose source checks deferred for lack of a type.
pub(crate) struct GraphDestinations(HashSet<Location>);

impl GraphDestinations {
    /// Remembers what the generic body could not type before selection.
    pub(crate) fn pending(body: &Body) -> Self {
        let mut pending = HashSet::new();
        hir::walk_statements(&body.statements, &mut |value| {
            if value.ty == Type::Unknown {
                pending.insert(value.at);
            }
        });
        Self(pending)
    }

    /// Record the concrete categories and recover the types that turn on
    /// them, then fit those types as source typing fits its known types.
    pub(crate) fn settle(
        &self,
        program: &Program,
        (module, declaration): (usize, usize),
        signature: &Signature,
        body: &mut Body,
        diagnostics: &mut Diagnostics,
    ) {
        // In source a later assignment may have a known category while
        // the initializer still awaits selection. The first value now
        // decides the category, including for a `var` with no initializer.
        hir::walk_statement_nodes(&body.statements, &mut |node| {
            if let Node::Statement(Stmt::Declare { local, .. }) = node {
                body.locals[*local].category = ValueCategory::Unsettled;
            }
        });
        record(program, body, true);
        if !self.0.is_empty() {
            let owner = &program.modules[module];
            let (bindings, written, calls) = (HashMap::new(), HashMap::new(), HashMap::new());
            let selecting = RefCell::new(Vec::new());
            let mut typer = Typer::bare(
                program,
                owner,
                &owner.declarations[declaration],
                signature,
                Tables {
                    bindings: &bindings,
                    written: &written,
                    calls: &calls,
                    selecting: &selecting,
                },
                diagnostics,
            );
            typer.graph = true;
            typer.locals.clone_from(&body.locals);
            hir::walk_statement_nodes_mut(&mut body.statements, &mut |node| {
                self.fit_destination(&mut typer, node);
            });
            record(program, body, true);
        }
        check_assignments(body, diagnostics);
    }

    fn fit_destination(&self, typer: &mut Typer<'_>, node: NodeMut<'_>) {
        let (value, expected, place, input) = match node {
            NodeMut::Statement(Stmt::Return {
                value: Some(value), ..
            }) => {
                let Some(expected) = &typer.signature.result else {
                    return;
                };
                (value, expected.clone(), "the function's result", true)
            }
            NodeMut::Statement(Stmt::Declare {
                local,
                value: Some(value),
            }) => (
                value,
                typer.locals[*local].ty.clone(),
                "the type written for it",
                false,
            ),
            NodeMut::Statement(Stmt::Assign { local, value, .. }) => (
                value,
                typer.locals[*local].ty.clone(),
                "the type of the `var`",
                false,
            ),
            NodeMut::Statement(Stmt::AssignPlace { place, value, .. }) => {
                (value, place.ty.clone(), "the type of the place", false)
            }
            NodeMut::Statement(Stmt::While {
                condition: Some(condition),
                ..
            }) => {
                self.fit_condition(typer, condition);
                return;
            }
            NodeMut::Expression(Expr {
                kind: ExprKind::If { condition, .. },
                ..
            }) => {
                self.fit_condition(typer, condition);
                return;
            }
            NodeMut::Expression(Expr {
                kind: ExprKind::Call {
                    callee, arguments, ..
                },
                ..
            }) => {
                self.fit_arguments(typer, callee, arguments);
                return;
            }
            _ => return,
        };
        self.fit(typer, value, &expected, place, input);
    }

    fn fit(&self, typer: &mut Typer<'_>, value: &mut Expr, ty: &Type, place: &str, input: bool) {
        if self.known(value) && value.ty != *ty {
            *value = if input {
                typer.fit_input(value.clone(), ty, place)
            } else {
                typer.fit(value.clone(), ty, place)
            };
        }
    }

    fn known(&self, value: &Expr) -> bool {
        value.ty != Type::Unknown && self.0.contains(&value.at)
    }

    fn fit_condition(&self, typer: &mut Typer<'_>, value: &mut Expr) {
        if self.known(value) {
            *value = typer.fit_condition(value.clone());
        }
    }

    fn fit_arguments(
        &self,
        typer: &mut Typer<'_>,
        callee: &Callee,
        arguments: &mut [hir::Argument],
    ) {
        if !arguments.iter().any(|argument| self.known(&argument.value)) {
            return;
        }
        let Some((module, declaration)) = callee.declaration() else {
            let template = match callee {
                Callee::Anonymous(function) => &function.template,
                Callee::Parameter { parameter, .. } => {
                    let Some(slot) = typer.signature.parameters.get(*parameter) else {
                        return;
                    };
                    let Type::Function(template) = &slot.ty else {
                        return;
                    };
                    template
                }
                _ => return,
            };
            for argument in arguments {
                let Some((name, ty)) = argument
                    .parameter
                    .and_then(|slot| template.parameters.get(slot))
                else {
                    continue;
                };
                let place = name.as_ref().map_or_else(
                    || "the function parameter".to_owned(),
                    |name| format!("the parameter `{name}`"),
                );
                self.fit(typer, &mut argument.value, ty, &place, true);
            }
            return;
        };
        let owner = &typer.program.modules[module];
        let Detail::Callable(signature) = &owner.details[declaration] else {
            return;
        };
        let listed = owner.declarations[declaration].kind.role() == Some(Role::Value);
        let map: Vec<_> = signature
            .generics
            .iter()
            .map(|generic| generic.at)
            .zip(callee.generics())
            .collect();
        let mut positions = HashMap::<usize, usize>::new();
        for argument in arguments {
            let Some(slot) = argument.parameter else {
                continue;
            };
            let Some(parameter) = signature.parameters.get(slot) else {
                continue;
            };
            let ty = if let Some(generic) = type_pack(parameter, &signature.generics) {
                let position = positions.entry(slot).or_default();
                let index = *position;
                *position += 1;
                let Some(ty) = callee
                    .type_packs()
                    .iter()
                    .find(|pack| pack.parameter == generic)
                    .and_then(|pack| pack.types.get(index))
                else {
                    continue;
                };
                ty.clone()
            } else {
                substitute_within(&parameter.ty, &map)
            };
            if self.known(&argument.value)
                && !matches!(ty, Type::Function(_))
                && !typer.lifted_window(&ty, listed, &argument.value, Takes::Parameters)
            {
                self.fit(
                    typer,
                    &mut argument.value,
                    &ty,
                    &format!("the parameter `{}`", parameter.name),
                    true,
                );
            }
        }
    }
}

/// CTRL-14 and CTRL-15: assignment to an ordinary binding outside a
/// branch selected at runtime is invalid, and every assignment keeps
/// the binding's category. Use lexical branches, excluding selectors.
fn check_assignments(body: &Body, diagnostics: &mut Diagnostics) {
    let mut runtime = HashSet::new();
    hir::walk_statement_nodes(&body.statements, &mut |node| match node {
        Node::Expression(Expr {
            kind:
                ExprKind::If {
                    condition,
                    then,
                    otherwise,
                },
            ..
        }) if condition.category == ValueCategory::Connection => {
            let mut mark = |node| mark_outer_assignment(body, condition.at, node, &mut runtime);
            then.walk_nodes(&mut mark);
            if let Some(otherwise) = otherwise {
                otherwise.walk_nodes(&mut mark);
            }
        }
        Node::Statement(Stmt::Switch {
            selector,
            cases,
            default,
            ..
        }) if selector.category == ValueCategory::Connection => {
            let mut mark = |node| mark_outer_assignment(body, selector.at, node, &mut runtime);
            for statements in cases.iter().map(|case| &case.body).chain(default) {
                hir::walk_statement_nodes(statements, &mut mark);
            }
        }
        _ => {}
    });
    hir::walk_statement_nodes(&body.statements, &mut |node| {
        let Some((local, value, place, at)) = assignment(node) else {
            return;
        };
        let given = place.map_or((value.category, value.at), |place| {
            place_assignment_category(place, value, |part| part.category)
        });
        if let Some(report) = assignment_category(
            &body.locals[local],
            given,
            place.is_some(),
            runtime.contains(&at),
        ) {
            diagnostics.emit(report);
        }
    });
}

fn mark_outer_assignment(
    body: &Body,
    selector: Location,
    node: Node<'_>,
    runtime: &mut HashSet<Location>,
) {
    if let Some((local, _, _, at)) = assignment(node)
        && let binding = body.locals[local].at
        && binding.file == selector.file
        && binding.span.end <= selector.span.start
    {
        runtime.insert(at);
    }
}

fn assignment(node: Node<'_>) -> Option<(LocalId, &Expr, Option<&Expr>, Location)> {
    match node {
        Node::Statement(Stmt::Assign {
            local, value, at, ..
        }) => Some((*local, value, None, *at)),
        Node::Statement(Stmt::AssignPlace {
            place, value, at, ..
        }) => {
            let ExprKind::Local(local) = place.place_root().kind else {
                return None;
            };
            Some((local, value, Some(place), *at))
        }
        _ => None,
    }
}
