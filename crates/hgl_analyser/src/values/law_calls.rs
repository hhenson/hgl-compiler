//! GEN-7 and GEN-18: read the laws guaranteed of a declaration's own
//! formal function parameters, or the metadata of a concrete callback.

use super::Typer;
use crate::hir::{Callee, Expr, ExprKind};
use crate::laws::Law;
use crate::types::Signature;
use hgl_parser::SyntaxNode;

impl Typer<'_> {
    /// GEN-18: find the formal function a query names and read the
    /// supplied function's law. Outer `None` means an ordinary call;
    /// `Some(None)` is a law whose value is not known under GEN-7.
    pub(super) fn law_call<'e>(
        &self,
        callee: (usize, usize),
        signature: &Signature,
        node: &SyntaxNode,
        argument: &dyn Fn(usize) -> Option<&'e Expr>,
    ) -> Option<Option<bool>> {
        let module = &self.program.modules[callee.0];
        let declared = &module.declarations[callee.1];
        let (law, name) = crate::laws::query(module, declared.at.file, node)?;
        let parameter = crate::laws::parameter(signature, &name)?;
        Some(argument(parameter).and_then(|expression| self.law_value(law, expression)))
    }

    /// A formal function's law is known only under this declaration's
    /// stated requirements; a concrete callback reads its metadata.
    pub(super) fn law_value(&self, law: Law, expression: &Expr) -> Option<bool> {
        if let ExprKind::StaticCallable(callback) = &expression.kind
            && let Callee::Parameter { parameter, .. } = callback.as_ref()
        {
            return crate::laws::stated(
                (self.module, self.file),
                self.signature,
                &self.declaration.syntax,
                *parameter,
                law,
            );
        }
        crate::laws::value(self.program, law, expression)
    }
}
