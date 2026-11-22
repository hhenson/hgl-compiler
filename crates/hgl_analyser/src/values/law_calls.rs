//! GEN-7 and GEN-18: prove law-dependent admission at a declaration,
//! then preserve the call for selection with a concrete static function.

use super::Typer;
use crate::hir::{Callee, Expr, ExprKind};
use crate::laws::Law;

impl Typer<'_> {
    /// A concrete callback's law, or what the enclosing declaration
    /// guarantees of a formal function. Trial assumptions only test
    /// whether the same body is admitted under every remaining value.
    pub(super) fn law_value(&self, law: Law, expression: &Expr) -> Option<bool> {
        if let ExprKind::StaticCallable(callback) = &expression.kind
            && let Callee::Parameter { parameter, .. } = callback.as_ref()
        {
            return crate::laws::stated(self.signature, &self.declaration.syntax, *parameter, law)
                .or_else(|| {
                    self.law_assumptions
                        .iter()
                        .rev()
                        .find(|(slot, other, _)| slot == parameter && *other == law)
                        .map(|(_, _, value)| *value)
                });
        }
        crate::laws::value(self.program, law, expression)
    }
}
