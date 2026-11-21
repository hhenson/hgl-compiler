//! A graph was checked under its generic requirements (GEN-7). Check
//! the destinations that depended on a category or type not then known.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use hgl_diagnostics::{Diagnostics, Location};
use hgl_source::Span;

use super::arguments::Takes;
use super::control::{bound_by_loop, iterates};
use super::fit::{open, whole};
use super::inference::type_pack;
use super::lifting::{assignment_category, record, record_with};
use super::members::{index_collection, index_operand, indexed_payload};
use super::results::tagged_side;
use super::statements::{child_loop_return, child_loop_write, place_assignment_category};
use super::{BOOL, Selected, Tables, Typer};
use crate::declared::substitute_within;
use crate::hir::{
    self, BinaryOperator, Body, Callee, Expr, ExprKind, Local, LocalId, Node, NodeMut, Stmt,
    UnaryOperator, Value, ValueCategory,
};
use crate::program::{Program, Role};
use crate::types::{Detail, Signature, Type};

/// Expression types whose source checks deferred for lack of a type.
pub(crate) struct GraphDestinations(HashSet<Location>);

impl GraphDestinations {
    /// Remembers what the generic body could not type before selection.
    pub(crate) fn pending(body: &Body) -> Self {
        let mut pending = HashSet::new();
        hir::walk_statements(&body.statements, &mut |value| {
            if !whole(&value.ty) {
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
        hir::walk_statement_nodes(&body.statements, &mut |node| match node {
            Node::Statement(Stmt::Declare { local, .. }) => {
                body.locals[*local].category = ValueCategory::Unsettled;
            }
            Node::Statement(Stmt::For { names, .. }) => {
                for &local in names {
                    body.locals[local].category = ValueCategory::Unsettled;
                }
            }
            _ => {}
        });
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
        record_with(program, body, true, Some(&mut typer));
        typer.locals.clone_from(&body.locals);
        if !self.0.is_empty() {
            hir::walk_statement_nodes_mut(&mut body.statements, &mut |node| {
                self.fit_destination(&mut typer, node);
            });
            record(program, body, true);
        }
        check_control(&mut typer, &body.statements);
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
            NodeMut::Statement(Stmt::For {
                names, sequence, ..
            }) => {
                if self.known(sequence) {
                    *sequence = typer.received(sequence.clone(), iterates);
                    typer.loop_bindings(sequence, names.len());
                }
                return;
            }
            NodeMut::Statement(Stmt::Switch {
                selector, cases, ..
            }) => {
                self.fit_switch(typer, selector, cases);
                return;
            }
            NodeMut::Expression(Expr {
                kind:
                    ExprKind::If {
                        condition,
                        then,
                        otherwise,
                        join_branches,
                    },
                ty,
                ..
            }) => {
                self.fit_condition(typer, condition);
                if !*join_branches && whole(ty) {
                    self.fit_branch(typer, then, ty);
                    if let Some(otherwise) = otherwise {
                        self.fit_branch(typer, otherwise, ty);
                    }
                }
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
            // EXPR-6: keep both operand reads inside the intrinsic
            // expression, so the right operand remains short-circuited.
            NodeMut::Expression(Expr {
                kind:
                    ExprKind::Unary {
                        operator: UnaryOperator::Not,
                        operand,
                    },
                ..
            }) => (operand.as_mut(), BOOL, "the operand of `!`", false),
            NodeMut::Expression(Expr {
                kind:
                    ExprKind::Binary {
                        operator: operator @ (BinaryOperator::And | BinaryOperator::Or),
                        left,
                        right,
                    },
                ..
            }) => {
                let place = format!("an operand of `{}`", operator.symbol());
                self.fit(typer, left, &BOOL, &place, false);
                self.fit(typer, right, &BOOL, &place, false);
                return;
            }
            NodeMut::Expression(Expr {
                kind: ExprKind::Tagged { success, payload },
                ty: Type::Result { value, error },
                ..
            }) => {
                let (side, place) = tagged_side(*success, value, error);
                (payload.as_mut(), side.clone(), place, false)
            }
            NodeMut::Expression(Expr {
                kind:
                    ExprKind::Construct {
                        arguments,
                        infer_types,
                        ..
                    },
                ty,
                at,
                ..
            }) => {
                self.fit_fields(typer, ty, arguments, *infer_types, *at);
                return;
            }
            // EXPR-5 and EXPR-9: use the types already established for
            // these entries, without inferring a new container shape.
            NodeMut::Expression(Expr {
                kind: ExprKind::Tuple(entries),
                ty: Type::Tuple(types),
                ..
            }) => {
                for (entry, ty) in entries.iter_mut().zip(types) {
                    if whole(ty) {
                        self.fit(typer, entry, ty, "this element of the tuple", false);
                    }
                }
                return;
            }
            NodeMut::Expression(Expr {
                kind: ExprKind::Sequence(entries),
                ty: Type::List { element, .. },
                at,
                ..
            }) if whole(element) => {
                for entry in entries {
                    if self.0.contains(at) {
                        // A newly inferred type constrains earlier known
                        // entries as well as the one that just settled.
                        if matches!(entry.kind, ExprKind::Literal(Value::Null))
                            && matches!(**element, Type::Nullable(_))
                        {
                            entry.ty = (**element).clone();
                        } else if whole(&entry.ty) && entry.ty != **element {
                            *entry =
                                typer.fit(entry.clone(), element, "an element, as the first is,");
                        }
                    } else {
                        self.fit(typer, entry, element, "an element", false);
                    }
                }
                return;
            }
            NodeMut::Expression(Expr {
                kind: ExprKind::Sequence(entries),
                ty: Type::Set(element),
                ..
            }) if whole(element) => {
                for entry in entries {
                    self.fit(typer, entry, element, "an element", false);
                }
                return;
            }
            NodeMut::Expression(Expr {
                kind: ExprKind::Pairs(entries),
                ty: Type::Dict { key, value },
                ..
            }) => {
                for (entry_key, entry_value) in entries {
                    if whole(key) {
                        self.fit(typer, entry_key, key, "a dictionary's key", false);
                    }
                    if whole(value) {
                        self.fit(typer, entry_value, value, "a dictionary's value", false);
                    }
                }
                return;
            }
            NodeMut::Expression(Expr {
                kind: ExprKind::Index { base, index },
                ..
            }) => {
                self.fit_index(typer, base, index);
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
        self.0.contains(&value.at) && whole(&value.ty)
    }

    fn fit_condition(&self, typer: &mut Typer<'_>, value: &mut Expr) {
        if self.known(value) {
            *value = typer.fit_condition(value.clone());
        }
    }

    /// A previously unknown base may need its result or nullable value
    /// read, and a newly known index must fit the selected collection.
    fn fit_index(&self, typer: &mut Typer<'_>, base: &mut Expr, index: &mut Expr) {
        let base_settled = self.known(base);
        let received = indexed_payload(&base.ty);
        if base_settled && received != &base.ty && index_collection(received) {
            *base = typer.received(base.clone(), index_collection);
        }
        if let Some((expected, place)) = index_operand(&base.ty)
            && whole(&index.ty)
            && (base_settled || self.known(index))
            && index.ty != *expected
        {
            *index = typer.fit(index.clone(), expected, place);
        }
    }

    /// A deferred selector or label is checked at its concrete CTRL-15 switch.
    fn fit_switch(&self, typer: &mut Typer<'_>, selector: &Expr, cases: &mut [hir::Case]) {
        let selector_settled = self.known(selector);
        if !selector_settled && !cases.iter().any(|case| self.known(&case.label)) {
            return;
        }
        let (ty, _) = typer.switch_type(selector);
        if open(&ty) {
            return;
        }
        for case in cases {
            if whole(&case.label.ty) && (selector_settled || self.known(&case.label)) {
                case.label = typer.fit(case.label.clone(), &ty, "the selector");
                if case.value.is_none()
                    && case.label.ty == ty
                    && let ExprKind::EnumMember { member, .. } = case.label.kind
                {
                    case.value = Some(hir::LabelValue::Member(member));
                }
            }
        }
    }

    /// CTRL-10: a block tail gives the branch its value, which must fit
    /// the type the source or the now-settled branches established.
    fn fit_branch(&self, typer: &mut Typer<'_>, branch: &mut Expr, ty: &Type) {
        if let ExprKind::Block {
            value: Some(tail), ..
        } = &mut branch.kind
            && self.known(tail)
        {
            self.fit(typer, tail, ty, "the value of this block", true);
            branch.ty = tail.ty.clone();
        }
    }

    /// TYPE-5: the selected struct type supplies each constructor field's type.
    fn fit_fields(
        &self,
        typer: &mut Typer<'_>,
        ty: &Type,
        arguments: &mut [hir::Argument],
        infer_types: bool,
        at: Location,
    ) {
        let completed = self.0.contains(&at) && whole(ty);
        if !completed && !arguments.iter().any(|argument| self.known(&argument.value)) {
            return;
        }
        let Type::Named {
            module,
            declaration,
            arguments: generic_arguments,
            ..
        } = ty
        else {
            return;
        };
        let Detail::Struct(structure) = &typer.program.modules[*module].details[*declaration]
        else {
            return;
        };
        let Some(types) = typer.program.field_types(ty) else {
            return;
        };
        let before = typer.diagnostics.len();
        for argument in arguments {
            let Some(slot) = argument.parameter else {
                continue;
            };
            let Some((field, expected)) = structure.members.get(slot).zip(types.get(slot)) else {
                continue;
            };
            if !whole(expected) {
                continue;
            }
            let value = &mut argument.value;
            let place = format!("the field `{}`", field.name);
            if completed
                && !whole(&value.ty)
                && matches!(value.kind, ExprKind::Literal(Value::Null))
                && let Some(node) = typer.syntax_of(value)
            {
                *value = typer.expression(&node, Some(expected));
            } else if completed && whole(&value.ty) && value.ty != *expected {
                // An earlier argument may decide a field's type only
                // now, even when this sibling was already known.
                *value = typer.fit(value.clone(), expected, &place);
            } else {
                self.fit(typer, value, expected, &place, false);
            }
        }
        // GEN-3, ERR-9: an inferred application checks its requirements
        // once its arguments fit and its type is concrete.
        if completed && infer_types && typer.diagnostics.len() == before {
            typer.requires_met(at, (*module, *declaration), generic_arguments);
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

/// The nearest runtime branch and child loop enclosing a typed node.
/// The selectors and loop sequences are outside their own boundaries.
#[derive(Default)]
struct Boundaries {
    runtime: HashMap<Location, Location>,
    children: HashMap<Location, Location>,
}

/// CTRL-14 and CTRL-15: a deferred selector can move writes or returns
/// into a runtime branch or a child graph. Recheck whole and nested
/// assignments, including ordinary values written by FN-15 calls.
fn check_control(typer: &mut Typer<'_>, statements: &[Stmt]) {
    let mut scopes = Boundaries::default();
    hir::walk_statement_nodes(statements, &mut |node| match node {
        Node::Expression(Expr {
            kind:
                ExprKind::If {
                    condition,
                    then,
                    otherwise,
                    ..
                },
            ..
        }) if condition.category == ValueCategory::Connection => {
            let mut mark = |node| mark_boundary(&mut scopes.runtime, condition.at, node);
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
            let mut mark = |node| mark_boundary(&mut scopes.runtime, selector.at, node);
            for statements in cases.iter().map(|case| &case.body).chain(default) {
                hir::walk_statement_nodes(statements, &mut mark);
            }
        }
        Node::Statement(Stmt::For {
            names,
            sequence,
            body,
            at,
        }) if bound_by_loop(sequence.category, &sequence.ty, names.len()).1 => {
            hir::walk_statement_nodes(body, &mut |node| {
                mark_boundary(&mut scopes.children, *at, node)
            });
        }
        _ => {}
    });
    hir::walk_statement_nodes(statements, &mut |node| {
        match node {
            Node::Expression(Expr { kind, at, .. }) => {
                if let Some(selected) = Selected::of(typer.program, kind) {
                    for (local, written, inside) in typer.places_given(&selected) {
                        scopes.check_write(
                            typer,
                            local,
                            *at,
                            (ValueCategory::Value, written),
                            inside,
                            written,
                        );
                    }
                }
            }
            Node::Statement(Stmt::Return { at, .. }) => {
                if scopes.children.contains_key(at) {
                    typer.diagnostics.emit(child_loop_return(*at));
                }
            }
            Node::Statement(Stmt::Assign {
                local, value, at, ..
            }) => {
                // This statement starts at its local name (the same
                // span the declaration check reports for a child write).
                let end = at.span.start + typer.locals[*local].name.len() as u32;
                let written =
                    Location::new(at.file, Span::new(at.span.start, end.min(at.span.end)));
                scopes.check_write(
                    typer,
                    *local,
                    *at,
                    (value.category, value.at),
                    false,
                    written,
                );
            }
            Node::Statement(Stmt::AssignPlace {
                place, value, at, ..
            }) => {
                if let ExprKind::Local(local) = place.place_root().kind {
                    let given = place_assignment_category(place, value, |part| part.category);
                    scopes.check_write(typer, local, *at, given, true, place.at);
                }
            }
            _ => {}
        }
    });
}

impl Boundaries {
    fn check_write(
        &self,
        typer: &mut Typer<'_>,
        local: LocalId,
        at: Location,
        given: (ValueCategory, Location),
        inside: bool,
        written: Location,
    ) {
        let binding = &typer.locals[local];
        let report = if outside(binding, self.children.get(&at)) {
            Some(child_loop_write(binding, written))
        } else {
            assignment_category(
                binding,
                given,
                inside,
                outside(binding, self.runtime.get(&at)),
            )
        };
        if let Some(report) = report {
            typer.diagnostics.emit(report);
        }
    }
}

fn outside(binding: &Local, boundary: Option<&Location>) -> bool {
    boundary.is_some_and(|boundary| {
        binding.at.file == boundary.file && binding.at.span.end <= boundary.span.start
    })
}

fn mark_boundary(scopes: &mut HashMap<Location, Location>, boundary: Location, node: Node<'_>) {
    let at = match node {
        Node::Statement(
            Stmt::Assign { at, .. } | Stmt::AssignPlace { at, .. } | Stmt::Return { at, .. },
        )
        | Node::Expression(Expr {
            kind: ExprKind::Call { .. },
            at,
            ..
        }) => *at,
        _ => return,
    };
    // For nested scopes the inner boundary applies, including to locals
    // declared in the outer scope before entering the inner one.
    scopes
        .entry(at)
        .and_modify(|outer| {
            if boundary.span.start > outer.span.start {
                *outer = boundary;
            }
        })
        .or_insert(boundary);
}
