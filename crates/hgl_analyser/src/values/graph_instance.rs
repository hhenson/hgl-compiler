//! A graph was checked under its generic requirements (GEN-7). Check
//! the destinations that depended on a category or type not then known.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use hgl_diagnostics::{Diagnostics, Location};
use hgl_parser::SyntaxKind;
use hgl_source::Span;

use super::arguments::Takes;
use super::control::{bound_by_loop, duplicate_switch_label, iterates};
use super::fit::{open, whole};
use super::inference::type_pack;
use super::lifting::{
    assignment_category, block_tail, contextual_tail_syntax, record, record_with,
    resolve_contextual_tail,
};
use super::members::{index_collection, index_operand, indexed_payload};
use super::results::{error_fields, tagged_side};
use super::statements::{
    child_loop_return, child_loop_write, missing_return, place_assignment_category,
};
use super::{BOOL, Selected, Tables, Typer};
use crate::hir::{
    self, BinaryOperator, Body, Callee, Expr, ExprKind, IfInference, Local, LocalId, Node, NodeMut,
    Stmt, UnaryOperator, Value, ValueCategory,
};
use crate::program::{OperatorRef, Program, Role};
use crate::syntax::ungrouped;
use crate::types::{Argument, Detail, Signature, Type};

/// Source checks awaiting concrete types and categories in a graph instance.
pub(crate) struct GraphDestinations {
    expressions: HashSet<Location>,
    captures: HashSet<Location>,
    unsettled_arguments: HashSet<Location>,
    index_bases: HashSet<Location>,
    list_assignments: HashSet<Location>,
    replay_source: bool,
}

impl GraphDestinations {
    /// Remembers what the generic body could not check before selection.
    pub(crate) fn pending(body: &Body) -> Self {
        let mut expressions = HashSet::new();
        let mut replay_source = false;
        let mut captures = HashSet::new();
        let mut unsettled_arguments = HashSet::new();
        let mut index_bases = HashSet::new();
        let mut list_assignments = HashSet::new();
        hir::walk_statement_nodes(&body.statements, &mut |node| {
            let value = match node {
                Node::Expression(value) => value,
                Node::Statement(Stmt::Assign {
                    local,
                    operator: None,
                    value,
                    ..
                }) => {
                    if matches!(&body.locals[*local].ty, Type::List { element, .. } if !whole(element))
                        && matches!(&value.kind, ExprKind::Sequence(_))
                    {
                        list_assignments.insert(value.at);
                    }
                    return;
                }
                _ => return,
            };
            if !whole(&value.ty) {
                expressions.insert(value.at);
            }
            if let ExprKind::Index { base, .. } = &value.kind
                && !whole(&base.ty)
                && !matches!(indexed_payload(&base.ty), Type::Tuple(_) | Type::Set(_))
            {
                // Source already checked a known tuple's position or
                // rejected a set, even with unknown element types.
                index_bases.insert(value.at);
            }
            if let ExprKind::Capture(call) = &value.kind
                && call.category == ValueCategory::Unsettled
            {
                captures.insert(call.at);
            }
            if let ExprKind::Call { arguments, .. } = &value.kind {
                unsettled_arguments.extend(
                    arguments
                        .iter()
                        .map(|argument| &argument.value)
                        .filter(|value| value.category == ValueCategory::Unsettled)
                        .map(|value| value.at),
                );
            }
            replay_source |= match &value.kind {
                ExprKind::DeferredBinary { configuration, .. }
                | ExprKind::DeferredNegation { configuration, .. } => !configuration.is_empty(),
                ExprKind::Call {
                    callee: Callee::DeferredOperator(_),
                    ..
                } => true,
                ExprKind::Call {
                    callee: Callee::Function { contextual, .. },
                    ..
                } => !contextual.is_empty(),
                _ => false,
            };
        });
        Self {
            expressions,
            captures,
            unsettled_arguments,
            index_bases,
            list_assignments,
            replay_source,
        }
    }

    /// Record the concrete categories and recover the types that turn on
    /// them, then fit those types as source typing fits its known types.
    pub(crate) fn settle(
        &self,
        program: &Program,
        (module, declaration): (usize, usize),
        (signature, map): (&Signature, &[(Location, &Argument)]),
        body: &mut Body,
        diagnostics: &mut Diagnostics,
        place_required: &dyn Fn(&mut Expr),
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
        let (bindings, written, calls) = if self.replay_source {
            super::tables(owner)
        } else {
            (HashMap::new(), HashMap::new(), HashMap::new())
        };
        let reading = if self.replay_source
            && let Detail::Callable(declared) = &owner.details[declaration]
        {
            // EXPR-10 and GEN-8: a contextual option is still read in
            // the scope and generic instance that wrote it.
            Some(Signature {
                generics: super::candidates::in_instance(declared, map).generics,
                ..signature.clone()
            })
        } else {
            None
        };
        let selecting = RefCell::new(Vec::new());
        let mut typer = Typer::bare(
            program,
            owner,
            &owner.declarations[declaration],
            reading.as_ref().unwrap_or(signature),
            Tables {
                bindings: &bindings,
                written: &written,
                calls: &calls,
                selecting: &selecting,
            },
            diagnostics,
        );
        if self.replay_source {
            typer.instance = map
                .iter()
                .map(|(at, argument)| (*at, (*argument).clone()))
                .collect();
        }
        typer.graph = true;
        record_with(program, body, true, Some(&mut typer), Some(place_required));
        typer.locals.clone_from(&body.locals);
        if !self.expressions.is_empty() || !self.list_assignments.is_empty() {
            hir::walk_statement_nodes_mut(&mut body.statements, &mut |node| {
                self.fit_destination(&mut typer, node);
            });
            record(program, body, true);
        }
        if !self.captures.is_empty() || !self.unsettled_arguments.is_empty() {
            hir::walk_statements(&body.statements, &mut |value| match &value.kind {
                ExprKind::Capture(call) if self.captures.contains(&call.at) => {
                    typer.check_capture_phase(call, call.category, None);
                }
                kind @ ExprKind::Call { .. } if !self.unsettled_arguments.is_empty() => {
                    // FN-9: use the graph's injected locals and typed
                    // aliases to recognize the same store as source typing.
                    typer.store_call(kind, |_, value| {
                        self.unsettled_arguments.contains(&value.at)
                            && value.category == ValueCategory::Connection
                    });
                    self.check_configuration(&mut typer, kind);
                }
                _ => {}
            });
        }
        check_control(
            &mut typer,
            &body.statements,
            owner.graph_return_checks[declaration].as_ref(),
        );
    }

    /// EXPR-14: only an actual brace entry whose category was unknown
    /// at source can require another check against the selected parameter.
    fn check_configuration(&self, typer: &mut Typer<'_>, kind: &ExprKind) {
        let ExprKind::Call {
            callee:
                Callee::Candidate {
                    operator,
                    module,
                    declaration,
                    ..
                },
            arguments,
            form,
            braced,
            ..
        } = kind
        else {
            return;
        };
        if braced.is_empty() {
            return;
        }
        let Some(Detail::Callable(signature)) =
            typer.program.modules[*module].details.get(*declaration)
        else {
            return;
        };
        let operator = OperatorRef {
            module: operator.0,
            declaration: operator.1,
        };
        for argument in arguments {
            let value = &argument.value;
            if self.unsettled_arguments.contains(&value.at)
                && braced.contains(&value.at)
                && let Some(parameter) = argument
                    .parameter
                    .and_then(|slot| signature.parameters.get(slot))
            {
                typer.configuration_input(
                    (operator, *form),
                    (*module, *declaration),
                    parameter,
                    value,
                    value.category,
                );
            }
        }
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
            NodeMut::Statement(Stmt::Assign { local, value, .. }) => {
                let expected = typer.locals[*local].ty.clone();
                if self.list_assignments.contains(&value.at)
                    && let Type::List { element, size } = &expected
                    && matches!(&value.ty, Type::List { size: given, .. } if given == size)
                    && whole(element)
                    && let ExprKind::Sequence(entries) = &mut value.kind
                    && entries.iter().all(|entry| whole(&entry.ty))
                {
                    // EXPR-3, EXPR-5: the initializer's type is the
                    // expected type of the assigned literal and its entries.
                    for entry in entries {
                        *entry = typer.fit(entry.clone(), element, "an element");
                    }
                    value.ty = expected;
                    return;
                }
                (value, expected, "the type of the `var`", false)
            }
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
                        inference,
                    },
                ty,
                ..
            }) => {
                self.fit_condition(typer, condition);
                if whole(ty) {
                    if matches!(inference, IfInference::None | IfInference::Then) {
                        self.fit_branch(typer, then, ty);
                    }
                    if matches!(inference, IfInference::None | IfInference::Otherwise)
                        && let Some(otherwise) = otherwise
                    {
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
                kind:
                    ExprKind::Binary {
                        operator: BinaryOperator::Equal | BinaryOperator::NotEqual,
                        left,
                        right,
                    },
                ..
            }) => {
                // ERR-15: keep the intrinsic comparison in source order
                // and take the settled result's form beside `null`.
                let (operand, null) = match (&left.kind, &right.kind) {
                    (_, ExprKind::Literal(Value::Null)) => (left, right),
                    (ExprKind::Literal(Value::Null), _) => (right, left),
                    _ => return,
                };
                if self.known(operand) && matches!(operand.ty, Type::Result { .. }) {
                    **operand = typer.operand_form((**operand).clone(), &null.ty, true);
                }
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
                kind: ExprKind::Error { message, code },
                ..
            }) => {
                let [(text, message_place), (optional, code_place)] = error_fields();
                self.fit(typer, message, &text, message_place, false);
                if let Some(code) = code {
                    self.fit(typer, code, &optional, code_place, false);
                }
                return;
            }
            NodeMut::Expression(Expr {
                kind: ExprKind::TagTest { success, value },
                ..
            }) => {
                if self.known(value) {
                    typer.check_tag_test(*success, value);
                }
                return;
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
            // TYPE-16: the constructor already bound each named
            // property to a slot. Reuse its property types and fit.
            NodeMut::Expression(Expr {
                kind: ExprKind::Delta { arguments },
                ty: Type::Delta(of),
                ..
            }) => {
                let Some(properties) = typer.program.delta_properties(of) else {
                    return;
                };
                for argument in arguments {
                    if let Some((name, expected)) =
                        argument.parameter.and_then(|slot| properties.get(slot))
                        && whole(expected)
                    {
                        let place = format!("the property `{name}`");
                        self.fit(typer, &mut argument.value, expected, &place, false);
                    }
                }
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
            }) if whole(element) && !self.list_assignments.contains(at) => {
                for entry in entries {
                    if self.expressions.contains(at) {
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
                at,
                ..
            }) => {
                self.fit_index(typer, base, index, self.index_bases.contains(at));
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
        self.expressions.contains(&value.at) && whole(&value.ty)
    }

    fn fit_condition(&self, typer: &mut Typer<'_>, value: &mut Expr) {
        if self.known(value) {
            *value = typer.fit_condition(value.clone());
        }
    }

    /// A previously unknown base may need its result or nullable value
    /// read, and a newly known index must fit the selected collection.
    fn fit_index(
        &self,
        typer: &mut Typer<'_>,
        base: &mut Expr,
        index: &mut Expr,
        base_kind_pending: bool,
    ) {
        let base_settled = self.known(base);
        let received = indexed_payload(&base.ty);
        if base_kind_pending {
            typer.check_index_base(received, base.at);
        }
        if base_settled && received != &base.ty && index_collection(received) {
            *base = typer.received(base.clone(), index_collection);
        }
        if base_kind_pending && whole(&index.ty) && matches!(base.ty, Type::Tuple(_)) {
            typer.tuple_index(&base.ty, index, None, true);
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
        let mut members = HashMap::<usize, (Location, bool)>::new();
        for case in cases {
            let mut replayed = false;
            if selector_settled
                && case.value.is_none()
                && case.label.ty == Type::Unknown
                && matches!(case.label.kind, ExprKind::Unread)
                && let Some(node) = typer.syntax_of(&case.label).map(ungrouped)
                && node.kind() == SyntaxKind::ImpliedMember
            {
                // TYPE-7: the source label needed the selector's enum.
                case.label = typer.expression(&node, Some(&ty));
                replayed = true;
            }
            if whole(&case.label.ty) && (selector_settled || self.known(&case.label)) {
                case.label = typer.fit(case.label.clone(), &ty, "the selector");
                if case.value.is_none()
                    && case.label.ty == ty
                    && let ExprKind::EnumMember { member, .. } = case.label.kind
                {
                    case.value = Some(hir::LabelValue::Member(member));
                }
            }
            if case.label.ty == ty
                && let Some(hir::LabelValue::Member(member)) = case.value
            {
                // TYPE-7 gives each member of this enum a unique value.
                // Source already checked a repeat between its own labels.
                if let Some((first, seen_at_source)) = members.get_mut(&member) {
                    if replayed || !*seen_at_source {
                        typer
                            .diagnostics
                            .emit(duplicate_switch_label(case.label.at, *first));
                    }
                    *seen_at_source |= !replayed;
                } else {
                    members.insert(member, (case.label.at, !replayed));
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
        let completed = self.expressions.contains(&at) && whole(ty);
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
            let tail = block_tail(value);
            let place = format!("the field `{}`", field.name);
            if completed
                && !whole(&tail.ty)
                && let Some(node) = contextual_tail_syntax(typer, tail).map(ungrouped)
                && match node.kind() {
                    // TYPE-7 and EXPR-9: these leaves take their type
                    // from the completed field. A source-resolved
                    // leaf already has a whole type and stays intact.
                    SyntaxKind::ImpliedMember => matches!(tail.kind, ExprKind::Unread),
                    SyntaxKind::SequenceExpr => node.children().next().is_none(),
                    _ => matches!(tail.kind, ExprKind::Literal(Value::Null)),
                }
            {
                let mut read = typer.expression(&node, Some(expected));
                read.at = tail.at;
                let fitted = typer.fit(read, expected, &place);
                resolve_contextual_tail(value, (fitted.kind, fitted.ty));
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
                let Some(input) = argument
                    .parameter
                    .and_then(|slot| template.parameters.get(slot))
                else {
                    continue;
                };
                let place = input.name().map_or_else(
                    || "the function parameter".to_owned(),
                    |name| format!("the parameter `{name}`"),
                );
                self.fit(typer, &mut argument.value, input.ty(), &place, true);
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
        let packs =
            super::dependent_packs::PackTypes::new(&signature.generics, callee.type_packs());
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
                packs.project(&parameter.ty, &map).unwrap_or(Type::Unknown)
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

/// A graph that may need a continuation check after selection, from
/// the declaration's FN-7 and CTRL-14 return flow.
#[derive(Debug, Clone)]
pub(crate) enum GraphReturnCheck {
    /// A path reaches the end of the body.
    Fallthrough,
    /// The tail returns if all these switches cover their enums.
    TailSwitches(Vec<Location>),
}

impl GraphReturnCheck {
    /// CTRL-6: each switch identified by source flow must cover every
    /// enum member in the instance for the continuation to be complete.
    fn needs_continuation(&self, typer: &Typer<'_>, statements: &[Stmt]) -> bool {
        let Self::TailSwitches(switches) = self else {
            return true;
        };
        let mut pending: HashSet<_> = switches.iter().copied().collect();
        hir::walk_statement_nodes(statements, &mut |node| {
            if let Node::Statement(Stmt::Switch {
                selector,
                cases,
                at,
                ..
            }) = node
                && pending.contains(at)
                && covers_enum(typer, selector, cases)
            {
                pending.remove(at);
            }
        });
        !pending.is_empty()
    }
}

/// CTRL-6: the cases cover every member when all the distinct
/// positions of the selector's own enum are among the settled labels.
fn covers_enum(typer: &Typer<'_>, selector: &Expr, cases: &[hir::Case]) -> bool {
    let Some(members) = typer.enum_members(&selector.ty) else {
        return false;
    };
    let covered: HashSet<_> = cases
        .iter()
        .filter_map(|case| match case.value {
            Some(hir::LabelValue::Member(member))
                if case.label.ty == selector.ty && member < members =>
            {
                Some(member)
            }
            _ => None,
        })
        .collect();
    covered.len() == members
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
fn check_control(
    typer: &mut Typer<'_>,
    statements: &[Stmt],
    check_return: Option<&GraphReturnCheck>,
) {
    let check_return =
        check_return.is_some_and(|check| check.needs_continuation(typer, statements));
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
    let mut needs_continuation = false;
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
            Node::Statement(Stmt::Return { value, at }) => {
                if scopes.children.contains_key(at) {
                    typer.diagnostics.emit(child_loop_return(*at));
                } else if check_return && value.is_some() && scopes.runtime.contains_key(at) {
                    needs_continuation = true;
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
    if needs_continuation
        && let Some(result) = &typer.signature.result
        && !open(result)
    {
        let at = typer.declaration.at;
        let name = format!("`{}`", typer.declaration.name);
        typer.diagnostics.emit(missing_return(at, &name, result));
    }
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
