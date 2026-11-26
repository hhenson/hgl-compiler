//! FN-4/17: a graph must not reject a call on the strength of a callback
//! capture category that only its caller can supply. Check the original source
//! with concrete capture categories; actual calls still check their own graph.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use hgl_diagnostics::{Diagnostic, Diagnostics, Location, Severity};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode};

use super::{Tables, Typer};
use crate::{Binding, LocalKind, Type, hir::ValueCategory};

impl Typer<'_> {
    /// A clean reading is evidence that the declaration's original errors
    /// depended on a provisional choice. No provisional HIR or trial calls
    /// are substituted for the graph checked at an actual binding.
    pub(super) fn check_open_pair_reports(&mut self, found: Tables<'_>, before: usize) {
        let original = &self.diagnostics.as_slice()[before..];
        if original.iter().all(|r| r.severity != Severity::Error) || original.iter().any(limit) {
            return;
        }
        let callbacks: Vec<_> = self
            .signature
            .parameters
            .iter()
            .enumerate()
            .filter_map(|(slot, parameter)| {
                matches!(&parameter.ty, Type::Function(f) if f.value)
                    .then_some((parameter.at, slot))
            })
            .collect();
        let scopes: Vec<_> = original
            .iter()
            .map(|report| self.report_scope(report.primary))
            .collect();
        let mut possible = vec![true; original.len()];
        let mut pending = vec![(vec![None; self.signature.parameters.len()], None)];
        let mut deps = HashMap::new();
        let mut budgets = HashMap::new();
        let mut unresolved = None;
        // Each rejected reading identifies a statement whose dependencies
        // must change. Unreferenced callback slots never branch the search.
        while let Some((assigned, previous)) = pending.pop() {
            let failures = if let Some(previous) = previous {
                previous
            } else {
                let reports = self.read_capture_categories(found, &assigned);
                if !reports.has_errors() {
                    let kept: Vec<_> = original
                        .iter()
                        .filter(|r| r.severity != Severity::Error)
                        .cloned()
                        .collect();
                    self.diagnostics.truncate(before);
                    for report in kept {
                        self.diagnostics.emit(report);
                    }
                    return;
                }
                if let Some(report) = reports.as_slice().iter().find(|r| limit(r)) {
                    unresolved = Some(report.clone());
                    continue;
                }
                reports
                    .as_slice()
                    .iter()
                    .filter(|r| r.severity == Severity::Error)
                    .map(|r| self.report_scope(r.primary))
                    .collect::<Vec<_>>()
            };
            for (keep, scope) in possible.iter_mut().zip(&scopes) {
                *keep &= failures.contains(scope);
            }
            let mut smallest = None;
            for scope in &failures {
                let used = deps
                    .entry(scope.clone())
                    .or_insert_with(|| self.capture_slots(scope, &callbacks));
                let free: Vec<_> = used
                    .iter()
                    .copied()
                    .filter(|slot| assigned[*slot].is_none())
                    .collect();
                if free.is_empty() {
                    smallest = None;
                    break;
                }
                if smallest
                    .as_ref()
                    .is_none_or(|(size, _, _)| free.len() < *size)
                {
                    smallest = Some((free.len(), free[0], scope));
                }
            }
            if let Some((_, slot, scope)) = smallest {
                // Charge a distinct alternative to the statement that needs
                // it. Independent statements keep their own reading allowance.
                let (left, cost) = budgets.entry(scope.clone()).or_insert_with(|| {
                    (
                        super::allowance(scope) as usize,
                        scope.descendants().count().max(1),
                    )
                });
                let Some(rest) = left.checked_sub(*cost) else {
                    self.diagnostics.truncate(before);
                    self.construct = self.at(scope);
                    self.rereads = 0;
                    self.may_reread();
                    return;
                };
                *left = rest;
                let mut live = assigned.clone();
                live[slot] = Some(ValueCategory::Connection);
                let mut value = assigned;
                value[slot] = Some(ValueCategory::Value);
                // Unassigned slots were read as values, so pinning one to
                // Value does not require another identical source read.
                pending.push((value, Some(failures)));
                pending.push((live, None));
            }
        }
        // Keep unrelated source errors on their declarations. When different
        // choices fail at different uses and no whole reading works, retain
        // the original explanation of the incompatible body.
        if possible.contains(&true) {
            let retained: Vec<_> = original
                .iter()
                .zip(possible)
                .filter(|(report, keep)| *keep || report.severity != Severity::Error)
                .map(|(report, _)| report.clone())
                .collect();
            self.diagnostics.truncate(before);
            for report in retained {
                self.diagnostics.emit(report);
            }
        }
        if let Some(report) = unresolved {
            self.diagnostics.emit(report);
        }
    }

    fn read_capture_categories(
        &self,
        found: Tables<'_>,
        assigned: &[Option<ValueCategory>],
    ) -> Diagnostics {
        let selecting = RefCell::new(Vec::new());
        let mut reports = Diagnostics::new();
        let mut trial = Typer::new(
            self.program,
            self.module,
            self.declaration,
            self.signature,
            Tables {
                selecting: &selecting,
                ..found
            },
            &mut reports,
        );
        trial.callback_captures = assigned
            .iter()
            .map(|slot| slot.unwrap_or(ValueCategory::Value))
            .collect();
        trial.graph();
        reports
    }

    /// Select the owning statement, including the surrounding control flow
    /// when a branch or iteration can depend on the same callback.
    fn report_scope(&self, at: Location) -> SyntaxNode {
        let root = &self.declaration.syntax;
        if self.file == at.file {
            for node in root.descendants() {
                if matches!(
                    node.kind(),
                    SyntaxKind::ArrowBody
                        | SyntaxKind::LocalDecl
                        | SyntaxKind::ReturnStmt
                        | SyntaxKind::AssignStmt
                        | SyntaxKind::ExprStmt
                        | SyntaxKind::ForStmt
                        | SyntaxKind::WhileStmt
                        | SyntaxKind::SwitchStmt
                ) && node.span().start <= at.span.start
                    && at.span.end <= node.span().end
                {
                    return node;
                }
            }
        }
        root.clone()
    }

    /// Which formal captures can affect a statement, including initializers
    /// through local aliases. A mutable binding can be assigned elsewhere.
    fn capture_slots(&self, scope: &SyntaxNode, callbacks: &[(Location, usize)]) -> Vec<usize> {
        let mut slots = Vec::new();
        let mut seen = HashSet::new();
        let mut pending = vec![scope.clone()];
        while let Some(node) = pending.pop() {
            if !seen.insert(node.span()) {
                continue;
            }
            for token in node
                .descendants_with_tokens()
                .filter_map(|part| part.into_token())
            {
                let Some(Binding::Local { at, kind }) =
                    self.bindings.get(&(self.file, token.span().start))
                else {
                    continue;
                };
                if let Some((_, slot)) = callbacks.iter().find(|(id, _)| id == at) {
                    if !slots.contains(slot) {
                        slots.push(*slot);
                    }
                } else if *kind == LocalKind::Let {
                    pending.push(self.report_scope(*at));
                } else if matches!(kind, LocalKind::Var | LocalKind::Iteration) {
                    return callbacks.iter().map(|(_, slot)| *slot).collect();
                }
            }
        }
        slots
    }
}

fn limit(report: &Diagnostic) -> bool {
    report.code == hgl_diagnostics::Code::Local(crate::codes::local::READING_LIMIT)
}
