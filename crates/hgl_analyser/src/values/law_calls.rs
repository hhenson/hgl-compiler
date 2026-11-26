//! GEN-7 and GEN-18: read the laws guaranteed of a declaration's own
//! formal function parameters, or the metadata of a concrete callback.

use super::constants::generic_default;
use super::dependent_packs::PackTypes;
use super::fit::whole;
use super::inference::mentions;
use super::{Candidate, Typer};
use crate::declared::names_parameter;
use crate::hir::{Callee, Expr, ExprKind, TypePackArgument};
use crate::laws::Law;
use crate::syntax::{child, ungrouped};
use crate::types::{Argument, Constant, ConstantValue, Signature, Type};
use hgl_parser::{SyntaxKind, SyntaxNode};

/// A known static sibling of a reflected callback, by receiving declaration and slot.
pub(super) struct CallbackLawInput {
    receiver: (usize, usize),
    slot: usize,
    value: CallbackLawValue,
}

enum CallbackLawValue {
    Static(Box<Expr>),
    Anonymous,
    Named(SyntaxNode),
}

impl CallbackLawInput {
    /// Answer a law of this receiver's static argument, if the slot matches.
    pub(super) fn answer(
        &self,
        typer: &Typer<'_>,
        receiver: (usize, usize),
        slot: usize,
        law: Law,
    ) -> Option<Option<bool>> {
        (self.receiver == receiver && self.slot == slot).then(|| match &self.value {
            CallbackLawValue::Static(value) => typer.law_value(law, value),
            CallbackLawValue::Anonymous => Some(false),
            CallbackLawValue::Named(_) => None,
        })
    }
}

impl Typer<'_> {
    /// GEN-18: preserve static siblings named by laws or by law-derived defaults.
    pub(super) fn callback_law_inputs<'e>(
        &self,
        (module, declaration, signature): Candidate<'_>,
        defaulted: &[usize],
        callback_slot: usize,
        arguments: impl IntoIterator<Item = (usize, &'e SyntaxNode, Option<&'e Expr>)>,
    ) -> Vec<CallbackLawInput> {
        let unit = &self.program.modules[module];
        let declared = &unit.declarations[declaration];
        let mut needed: Vec<_> = child(&declared.syntax, SyntaxKind::RequiresClause)
            .into_iter()
            .flat_map(|clause| clause.descendants())
            .filter(|node| node.kind() == SyntaxKind::ConstraintCall)
            .filter_map(|node| {
                let (_, name) = crate::laws::query(unit, declared.at.file, &node)?;
                crate::laws::parameter(signature, &name)
            })
            .collect();
        needed.extend(defaulted.iter().filter_map(|&index| {
            let node = generic_default(&declared.syntax, &signature.generics[index])?;
            crate::laws::default_parameter(signature, &node)
        }));
        if needed.is_empty() {
            return Vec::new();
        }
        let mut known = Vec::new();
        for (slot, node, ready) in arguments {
            if slot == callback_slot || !needed.contains(&slot) {
                continue;
            }
            let node = ungrouped(node.clone());
            let value = if let Some(value) =
                ready.filter(|value| matches!(value.kind, ExprKind::StaticCallable(_)))
            {
                CallbackLawValue::Static(Box::new(value.clone()))
            } else if node.kind() == SyntaxKind::FnExpr {
                // GEN-18: anonymous callbacks declare no laws even before
                // the argument reader has checked the body against its slot.
                CallbackLawValue::Anonymous
            } else if matches!(
                node.kind(),
                SyntaxKind::NameExpr | SyntaxKind::FunctionReference
            ) {
                CallbackLawValue::Named(node)
            } else {
                continue;
            };
            known.push(CallbackLawInput {
                receiver: (module, declaration),
                slot,
                value,
            });
        }
        known
    }

    /// Resolve named siblings after the reflected trial has bound the receiver's types.
    pub(super) fn read_law_siblings(
        &mut self,
        (receiver, signature): ((usize, usize), &Signature),
        found: &mut [Option<Argument>],
        packs: &[TypePackArgument],
        inputs: &[CallbackLawInput],
    ) -> Result<Vec<(usize, Expr)>, ()> {
        let mut named = inputs
            .iter()
            .filter(|input| input.receiver == receiver)
            .filter_map(|input| match &input.value {
                CallbackLawValue::Named(node) => Some((input.slot, node)),
                _ => None,
            })
            .peekable();
        if named.peek().is_none() {
            return Ok(Vec::new());
        }
        let unit = &self.program.modules[receiver.0];
        let declaration = &unit.declarations[receiver.1];
        let generics = &signature.generics;
        super::quantifiers::bind(unit, declaration, generics, packs, found);
        let unbound: Vec<_> = generics
            .iter()
            .zip(found.iter())
            .filter_map(|(generic, value)| value.is_none().then_some(generic.at))
            .collect();
        let bindings: Vec<_> = generics
            .iter()
            .zip(found.iter())
            .filter_map(|(generic, value)| Some((generic.at, value.as_ref()?)))
            .collect();
        let packs = PackTypes::new(generics, packs);
        let mut resolved = Vec::new();
        for (slot, node) in named {
            let Ok(Type::Function(template)) =
                packs.project(&signature.parameters[slot].ty, &bindings)
            else {
                continue;
            };
            if template.named_pack.is_some()
                || !template
                    .ordinary_parameters()
                    .all(|input| whole(input.ty()) && !mentions(input.ty(), &unbound))
                || !self.may_reread()
            {
                continue;
            }
            // An outer generic can still name an ordinary function or
            // a formal argument with a stated law. A failed symbolic
            // reading leaves the final instance check to the caller.
            let complete = template
                .ordinary_parameters()
                .map(|input| input.ty())
                .chain(template.result.iter())
                .all(|ty| whole(ty) && !names_parameter(ty));
            let mark = self.mark();
            let value = self.function_argument(node, &template);
            if !self.take_back(mark, false) {
                if complete {
                    return Err(());
                }
                continue;
            }
            if matches!(value.kind, ExprKind::StaticCallable(_)) {
                resolved.push((slot, value));
            }
        }
        Ok(resolved)
    }

    /// GEN-18: evaluate defaults the call actually takes from its static arguments.
    pub(super) fn apply_law_defaults(
        &self,
        callee: (usize, usize),
        signature: &Signature,
        defaulted: &[usize],
        found: &mut [Option<Argument>],
        argument: &dyn Fn(&Typer<'_>, usize, Law) -> Option<bool>,
    ) {
        let declaration = &self.program.modules[callee.0].declarations[callee.1];
        for &index in defaulted {
            let Some(default) = generic_default(&declaration.syntax, &signature.generics[index])
            else {
                continue;
            };
            let Some(value) = self.law_answer(callee, signature, &default, argument) else {
                continue;
            };
            found[index] = Some(Argument::Constant(Constant {
                text: default.text().to_string(),
                value: value.map_or(ConstantValue::Unknown, ConstantValue::Bool),
            }));
        }
    }

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
        self.law_answer(callee, signature, node, &|typer, slot, law| {
            argument(slot).and_then(|expression| typer.law_value(law, expression))
        })
    }

    /// Resolve the law and its parameter before asking the current call for the answer.
    pub(super) fn law_answer(
        &self,
        callee: (usize, usize),
        signature: &Signature,
        node: &SyntaxNode,
        argument: &dyn Fn(&Typer<'_>, usize, Law) -> Option<bool>,
    ) -> Option<Option<bool>> {
        let module = &self.program.modules[callee.0];
        let declared = &module.declarations[callee.1];
        let (law, name) = crate::laws::query(module, declared.at.file, node)?;
        let parameter = crate::laws::parameter(signature, &name)?;
        Some(argument(self, parameter, law))
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
