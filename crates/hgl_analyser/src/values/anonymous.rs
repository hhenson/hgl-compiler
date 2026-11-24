//! Checks anonymous function bodies with the types they write or get
//! from a direct call (FN-7) and the lexical context of their captures
//! (FN-14). Part of the typing pass ([`super`]).

use std::collections::HashSet;

use hgl_diagnostics::Location;
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};

use super::Typer;
use super::arguments::{Slot, Takes};
use crate::declared::substitute_within;
use crate::hir::{
    Body, Callee, ExprKind, Form, LocalId, Node, NodeMut, PackKey, Stmt, ValueCategory,
    walk_statement_nodes, walk_statement_nodes_mut, walk_statements_mut,
};
use crate::program::LocalKind;
use crate::read::is_type;
use crate::syntax::{child, tokens};
use crate::types::Type;

/// What belongs to the anonymous function currently being checked.
#[derive(Clone, PartialEq, Eq)]
pub(super) struct Anonymous {
    /// The result the body must give, if any.
    pub(super) result: Option<Type>,
    /// FN-17: infer whether a complete, unwritten result exists.
    pub(super) infer_result: bool,
    /// The first of its own locals. Earlier locals would be captures.
    pub(super) first_local: LocalId,
}

pub(super) struct AnonymousDefinition {
    pub(super) body: Body,
    pub(super) captures: Vec<LocalId>,
    pub(super) pack_captures: Vec<(usize, PackKey)>,
}

impl Typer<'_> {
    /// A name from an enclosing scope is captured (FN-14).
    pub(super) fn is_capture(&self, local: LocalId) -> bool {
        self.anonymous
            .as_ref()
            .is_some_and(|scope| local < scope.first_local)
    }

    /// The parameters of the anonymous function, each with its written
    /// type if it has one (FN-7).
    pub(super) fn anonymous_parameters(
        &mut self,
        function: &SyntaxNode,
    ) -> Vec<(SyntaxToken, Option<Type>)> {
        child(function, SyntaxKind::ParamList)
            .into_iter()
            .flat_map(|list| list.children())
            .filter(|part| part.kind() == SyntaxKind::LambdaParam)
            .filter_map(|parameter| {
                let name = tokens(&parameter).find(|token| token.kind() == SyntaxKind::Ident)?;
                let ty = self.anonymous_type(&parameter);
                Some((name, ty))
            })
            .collect()
    }

    /// The type written on an anonymous parameter or result (FN-7).
    pub(super) fn anonymous_type(&mut self, annotation: &SyntaxNode) -> Option<Type> {
        let written = annotation.children().find(|part| is_type(part.kind()))?;
        let ty = self
            .written
            .get(&(self.file, written.span().start))
            .copied()
            .unwrap_or(&Type::Unknown);
        let ty = self.project_pack_pass_type(ty, self.at(&written));
        if self.instance.is_empty() {
            return Some(ty);
        }
        // GEN-7: a callback tested in a concrete instance reads its
        // written annotations with the enclosing instance's types.
        let map: Vec<_> = self
            .instance
            .iter()
            .map(|(at, argument)| (*at, argument))
            .collect();
        Some(substitute_within(&ty, &map))
    }

    /// The body of the anonymous function, read where it is written,
    /// with its own parameters and result (FN-7).
    ///
    /// FN-14 gives the body its own scope with access to the enclosing
    /// parameters and immutable bindings. A captured input is read
    /// under the admission where the function is written (TIME-1,
    /// TIME-4, TIME-5). Endpoint operations also retain that context,
    /// but a parameter declared inside this function is no node input
    /// (TIME-9).
    ///
    /// A written type or one supplied by the accepting form guides this
    /// check. Missing types stay unknown.
    pub(super) fn anonymous_body(
        &mut self,
        function: &SyntaxNode,
        parameters: Vec<(SyntaxToken, Option<Type>)>,
        result: &Type,
    ) {
        self.checked_anonymous(function, parameters, (Some(result), false), None);
    }

    /// Read a static anonymous argument in the phase supplied by FN-17.
    pub(super) fn anonymous_definition(
        &mut self,
        function: &SyntaxNode,
        parameters: Vec<(SyntaxToken, Option<Type>)>,
        result: (Option<&Type>, bool),
        temporal: bool,
    ) -> Option<AnonymousDefinition> {
        self.checked_anonymous(function, parameters, result, Some(temporal))
    }

    fn checked_anonymous(
        &mut self,
        function: &SyntaxNode,
        parameters: Vec<(SyntaxToken, Option<Type>)>,
        (result, infer_result): (Option<&Type>, bool),
        specialization: Option<bool>,
    ) -> Option<AnonymousDefinition> {
        // CTRL-5: a return belongs to this function alone.
        self.check_capability_captures(function);
        let mut mark = self.mark();
        let first_local = mark.locals;
        let outer = self.anonymous.replace(Anonymous {
            result: result.cloned(),
            infer_result,
            first_local,
        });
        let temporal = self.temporal.take();
        let dynamic = self.dynamic.take();
        let must_return = std::mem::take(&mut self.must_return);
        self.returned = false;
        self.unsure = false;
        let count = parameters.len();
        for (name, ty) in parameters {
            let at = Location::new(self.file, name.span());
            let local = self.declare(
                name.text(),
                at,
                LocalKind::Parameter,
                ty.unwrap_or(Type::Unknown),
                true,
            );
            if specialization == Some(true) {
                self.locals[local].category = ValueCategory::Connection;
            }
        }
        let statements = self.check_body(
            function,
            result,
            self.at(function),
            "this anonymous function",
        );
        let output = specialization.and(statements).map(|statements| {
            // GEN-7: a deferred selection survives closing over its
            // lexical scope. Diagnostics still use the local IDs from
            // the first reading, before close renumbers them.
            let mut selections = self.selecting.borrow_mut();
            let unresolved = selections[mark.selecting..]
                .iter_mut()
                .filter(|call| call.scope.is_none());
            let mut scope = None;
            for call in unresolved {
                call.scope = Some(std::sync::Arc::clone(
                    scope.get_or_insert_with(|| self.locals.clone().into()),
                ));
            }
            mark.selecting = selections.len();
            let body = Body {
                locals: self.locals.clone(),
                statements,
            };
            close(body, first_local, count, specialization == Some(false))
        });
        self.restore_read(mark, false);
        self.anonymous = outer;
        self.temporal = temporal;
        self.dynamic = dynamic;
        self.must_return = must_return;
        output
    }

    /// The direct call `(fn(x) => x + 1)(7)` supplies the parameter
    /// and result context for types the function leaves unwritten
    /// (FN-7). Its result is still an ordinary value, with no type
    /// representing the function itself.
    pub(super) fn call_anonymous(
        &mut self,
        function: &SyntaxNode,
        keyword: &SyntaxToken,
        list: &SyntaxNode,
        expected: Option<&Type>,
    ) -> (ExprKind, Type) {
        if let Some(unread) = self.unread_pack_call(list) {
            return unread;
        }
        let mut parameters = self.anonymous_parameters(function);
        let slots: Vec<_> = parameters
            .iter()
            .map(|(name, ty)| Slot {
                configuration: false,
                name: name.text(),
                ty: ty.clone().unwrap_or(Type::Unknown),
                // UNIT-8: an omitted argument of a written nullable type is null.
                required: !matches!(ty, Some(Type::Nullable(_))),
                pack: None,
                pack_bounds: (0, None),
                type_pack: None,
                listed: false,
            })
            .collect();
        let arguments = self.bind(keyword, list, &slots, Takes::Parameters);
        for argument in &arguments {
            if let Some(index) = argument.parameter {
                parameters[index]
                    .1
                    .get_or_insert_with(|| argument.value.ty.clone());
            }
        }
        let result = child(function, SyntaxKind::ReturnType)
            .and_then(|annotation| self.anonymous_type(&annotation))
            .or_else(|| expected.cloned())
            .unwrap_or(Type::Unknown);
        self.anonymous_body(function, parameters, &result);
        // The C++ target has no anonymous functions yet.
        let kind = ExprKind::Call {
            callee: Callee::Unread,
            arguments,
            form: Form::Named,
            text: None,
            braced: Vec::new(),
            lifted: false,
        };
        (kind, result)
    }
}

/// Put the anonymous function's inputs and captures first in its own
/// local table. Every outer reference then has an explicit input.
fn close(mut body: Body, first: LocalId, parameters: usize, value: bool) -> AnonymousDefinition {
    let mut captures = Vec::new();
    let mut packed = Vec::new();
    let mut capture_entry = |pack: LocalId, key: &PackKey, ty: &Type| {
        if pack < first
            && !packed
                .iter()
                .any(|(outer, entry, _)| *outer == pack && entry == key)
        {
            packed.push((pack, key.clone(), ty.clone()));
        }
    };
    let mut endpoints = HashSet::new();
    let mut captured = |local: LocalId| {
        if local < first && !captures.contains(&local) {
            captures.push(local);
        }
    };
    walk_statement_nodes(&body.statements, &mut |node| {
        let Node::Expression(expression) = node else {
            return;
        };
        match &expression.kind {
            ExprKind::Local(local) | ExprKind::EndpointOf(local) | ExprKind::BoundChild(local) => {
                captured(*local)
            }
            ExprKind::PackEntry { pack, key } => capture_entry(*pack, key, &expression.ty),
            ExprKind::Endpoint {
                inputs: Some(locals),
                ..
            } => {
                locals.iter().copied().for_each(&mut captured);
            }
            // ERR-14: a child bound in the helper needs the endpoint
            // of the captured parent. An ordinary snapshot loses validity.
            ExprKind::ChildEndpointOf(child) if value => {
                if let ExprKind::Local(local) = child.place_root().kind
                    && local < first
                {
                    endpoints.insert(local);
                }
            }
            _ => {}
        }
        let target = match &expression.kind {
            ExprKind::Call { callee, .. } => Some(callee),
            ExprKind::StaticCallable(callee) => Some(&**callee),
            _ => None,
        };
        if let Some(Callee::Anonymous(function)) = target {
            for (index, &local) in function.captures.iter().enumerate() {
                if let Some(key) = function.capture_pack_key(index) {
                    capture_entry(local, key, function.capture_type(index));
                    continue;
                }
                captured(local);
                if value
                    && local < first
                    && matches!(function.capture_type(index), Type::Endpoint(_))
                {
                    endpoints.insert(local);
                }
            }
        }
    });
    if !endpoints.is_empty() {
        walk_statements_mut(&mut body.statements, &mut |expression| {
            if let ExprKind::Local(local) = expression.kind
                && endpoints.contains(&local)
                && !matches!(expression.ty, Type::Endpoint(_))
            {
                expression.kind = ExprKind::BoundChild(local);
            }
        });
    }
    let ordinary_locals = body.locals.len();
    let ordinary_captures = captures.len();
    let pack_captures = materialize_pack_captures(&mut body, &mut captures, &packed);
    let mut remapped = vec![0; body.locals.len()];
    let order = (first..first + parameters)
        .chain(captures[..ordinary_captures].iter().copied())
        .chain(ordinary_locals..body.locals.len())
        .chain(first + parameters..ordinary_locals);
    body.locals = order
        .enumerate()
        .map(|(new, old)| {
            remapped[old] = new;
            let mut local = body.locals[old].clone();
            if endpoints.contains(&old) && !matches!(local.ty, Type::Endpoint(_)) {
                local.ty = Type::Endpoint(Box::new(local.ty));
            }
            local
        })
        .collect();
    remap_locals(&mut body, &remapped);
    AnonymousDefinition {
        body,
        captures,
        pack_captures,
    }
}

/// Give each captured entry its own local inside the closed function, including
/// where the entry is only captured by a more deeply nested anonymous body.
fn materialize_pack_captures(
    body: &mut Body,
    captures: &mut Vec<LocalId>,
    packed: &[(LocalId, PackKey, Type)],
) -> Vec<(usize, PackKey)> {
    if packed.is_empty() {
        return Vec::new();
    }
    let start = body.locals.len();
    let mut pack_captures = Vec::new();
    for (pack, key, ty) in packed {
        let mut name = format!("pack_capture_{}", pack_captures.len());
        while body.locals.iter().any(|local| local.name == name) {
            name.push('_');
        }
        let binding = crate::hir::Local {
            ty: ty.clone(),
            name,
            ..body.locals[*pack].clone()
        };
        body.locals.push(binding);
        pack_captures.push((captures.len(), key.clone()));
        captures.push(*pack);
    }
    let entry = |pack, key: &PackKey| {
        packed
            .iter()
            .position(|(outer, captured, _)| *outer == pack && captured == key)
            .map(|offset| start + offset)
    };
    walk_statements_mut(&mut body.statements, &mut |expression| {
        let callee = match &mut expression.kind {
            ExprKind::PackEntry { pack, key } => {
                if let Some(local) = entry(*pack, key) {
                    expression.kind = ExprKind::Local(local);
                }
                return;
            }
            ExprKind::Call { callee, .. } => callee,
            ExprKind::StaticCallable(callee) => callee,
            _ => return,
        };
        if let Callee::Anonymous(function) = callee {
            function.pack_captures.retain(|(index, key)| {
                let local = &mut function.captures[*index];
                if let Some(slot) = entry(*local, key) {
                    *local = slot;
                    false
                } else {
                    true
                }
            });
        }
    });
    pack_captures
}

/// Remap the local references of a body. A nested anonymous target has
/// its own table, so only its capture list refers to the surrounding body.
pub(crate) fn remap_locals(body: &mut Body, slots: &[LocalId]) {
    let change = |local: &mut LocalId| *local = slots[*local];
    let change_callee = |callee: &mut Callee| {
        if let Callee::Anonymous(function) = callee {
            function.captures.iter_mut().for_each(change);
        }
    };
    walk_statement_nodes_mut(&mut body.statements, &mut |node| match node {
        NodeMut::Statement(statement) => match statement {
            Stmt::Declare { local, .. }
            | Stmt::Assign { local, .. }
            | Stmt::Inject { local, .. } => change(local),
            Stmt::For { names, .. } => names.iter_mut().for_each(change),
            Stmt::Hook {
                header: Some(header),
                ..
            } => {
                header.subscribed.iter_mut().for_each(change);
                header.required.iter_mut().for_each(change);
                header
                    .subscribed_entries
                    .iter_mut()
                    .chain(&mut header.required_entries)
                    .for_each(|entry| change(&mut entry.pack));
            }
            _ => {}
        },
        NodeMut::Expression(expression) => match &mut expression.kind {
            ExprKind::Local(local) | ExprKind::EndpointOf(local) | ExprKind::BoundChild(local) => {
                change(local)
            }
            ExprKind::PackEntry { pack, .. } => change(pack),
            ExprKind::ListComprehension { binding, .. } => change(binding),
            ExprKind::Endpoint {
                inputs: Some(locals),
                ..
            } => locals.iter_mut().for_each(change),
            ExprKind::Eval {
                inherited: Some(local),
                ..
            } => change(local),
            ExprKind::Call { callee, .. } => change_callee(callee),
            ExprKind::StaticCallable(callee) => change_callee(callee),
            _ => {}
        },
    });
}
