//! GEN-7 and GEN-18: read the laws guaranteed of a declaration's own
//! formal function parameters, or the metadata of a concrete callback.

use super::Typer;
use crate::hir::{Callee, Expr, ExprKind};
use crate::laws::Law;

impl Typer<'_> {
    /// A formal function's law is known only under this declaration's
    /// stated requirements; a concrete callback reads its metadata.
    pub(super) fn law_value(&self, law: Law, expression: &Expr) -> Option<bool> {
        if let ExprKind::StaticCallable(callback) = &expression.kind
            && let Callee::Parameter { parameter, .. } = callback.as_ref()
        {
            return crate::laws::stated(self.signature, &self.declaration.syntax, *parameter, law);
        }
        crate::laws::value(self.program, law, expression)
    }
}
