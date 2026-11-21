//! Checks anonymous function bodies with the types they write or get
//! from a direct call (FN-7) and the lexical context of their captures
//! (FN-14). Part of the typing pass ([`super`]).

use hgl_diagnostics::Location;
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};

use super::Typer;
use super::arguments::{Slot, Takes};
use crate::hir::{Callee, ExprKind, Form, LocalId};
use crate::program::LocalKind;
use crate::read::is_type;
use crate::syntax::{child, tokens};
use crate::types::Type;

/// What belongs to the anonymous function currently being checked.
pub(super) struct Anonymous {
    /// The result the body must give, or unknown without result context.
    pub(super) result: Type,
    /// The first of its own locals. Earlier locals would be captures.
    pub(super) first_local: LocalId,
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
        &self,
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
    pub(super) fn anonymous_type(&self, annotation: &SyntaxNode) -> Option<Type> {
        let ty = annotation.children().find(|part| is_type(part.kind()))?;
        Some(
            self.written
                .get(&(self.file, ty.span().start))
                .map_or(Type::Unknown, |ty| (*ty).clone()),
        )
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
        // CTRL-5: a return belongs to this function alone. Keep the
        // enclosing context for captured inputs but isolate flow state.
        self.check_capability_captures(function);
        let mark = self.mark();
        let outer = self.anonymous.replace(Anonymous {
            result: result.clone(),
            first_local: mark.locals,
        });
        let temporal = self.temporal.take();
        let dynamic = self.dynamic.take();
        let must_return = std::mem::take(&mut self.must_return);
        self.returned = false;
        self.unsure = false;
        for (name, ty) in parameters {
            let at = Location::new(self.file, name.span());
            self.declare(
                name.text(),
                at,
                LocalKind::Parameter,
                ty.unwrap_or(Type::Unknown),
                true,
            );
        }
        self.check_body(
            function,
            Some(result),
            self.at(function),
            "this anonymous function",
        );
        self.restore_read(mark, false);
        self.anonymous = outer;
        self.temporal = temporal;
        self.dynamic = dynamic;
        self.must_return = must_return;
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
        let mut parameters = self.anonymous_parameters(function);
        let slots: Vec<_> = parameters
            .iter()
            .map(|(name, ty)| Slot {
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
            lifted: false,
        };
        (kind, result)
    }
}
