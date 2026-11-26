//! FN-4/17: a graph must not reject a call on the strength of a callback
//! capture category that only its caller can supply. Check the original source
//! with concrete capture categories; actual calls still check their own graph.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use hgl_diagnostics::{Diagnostic, Diagnostics, Location, Severity};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode};

use super::{Tables, Typer};
use crate::{
    Binding, LocalKind, Type,
    hir::{Callee, ValueCategory},
};

struct OpenCapture {
    at: Location,
    slot: usize,
    classes: Vec<usize>,
}

impl Typer<'_> {
    /// A clean reading is evidence that the declaration's original errors
    /// depended on a provisional choice. No provisional HIR or trial calls
    /// are substituted for the graph checked at an actual binding.
    pub(super) fn check_open_pair_reports(&mut self, found: Tables<'_>, before: usize) {
        let original = &self.diagnostics.as_slice()[before..];
        if original.iter().all(|r| r.severity != Severity::Error) || original.iter().any(limit) {
            return;
        }
        let callbacks = self.open_callback_captures();
        let Some(last_class) = callbacks.iter().flat_map(|cb| &cb.classes).max() else {
            return;
        };
        let mut possible = vec![true; original.len()];
        let mut pending = vec![(vec![None; last_class + 1], None)];
        let mut deps = HashMap::new();
        let mut budgets = HashMap::new();
        let mut unresolved = None;
        // Each rejected reading identifies a statement whose dependencies
        // must change. Unreferenced callback slots never branch the search.
        while let Some((assigned, previous)) = pending.pop() {
            let failures = if let Some(previous) = previous {
                previous
            } else {
                let reports = self.read_capture_categories(found, &assigned, &callbacks);
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
                // A fixed bad operand and a provisional consumer can be in
                // one expression. Match diagnostics at their own source
                // site; use the enclosing statement only for dependencies.
                for (keep, original) in possible.iter_mut().zip(original) {
                    *keep &= reports
                        .as_slice()
                        .iter()
                        .any(|r| r.primary == original.primary && r.code == original.code);
                }
                reports
                    .as_slice()
                    .iter()
                    .filter(|r| r.severity == Severity::Error)
                    .map(|r| self.report_scope(r.primary))
                    .collect::<Vec<_>>()
            };
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
        if original
            .iter()
            .zip(&possible)
            .any(|(report, keep)| *keep && report.severity == Severity::Error)
        {
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

    /// Each outer formal has one capture category, shared by every receiving
    /// slot that forwards it directly or closes over it in an anonymous body.
    fn open_callback_captures(&self) -> Vec<OpenCapture> {
        let mut classes = HashMap::new();
        let mut open = Vec::new();
        for (slot, parameter) in self.signature.parameters.iter().enumerate() {
            if !matches!(&parameter.ty, Type::Function(f) if f.value) {
                continue;
            }
            let target = self.pack_functions.iter().find(|(id, _)| *id == slot);
            let mut used = Vec::new();
            match target {
                None => used.push(slot),
                Some((
                    _,
                    Callee::Parameter {
                        parameter,
                        temporal: false,
                        ..
                    },
                )) => used.push(*parameter),
                Some((_, Callee::Anonymous(function))) if function.template.value => {
                    crate::instance::used_function_parameters(&function.body, &mut used);
                }
                _ => {}
            }
            if used.is_empty() {
                continue;
            }
            used.sort_unstable();
            used.dedup();
            let capture_classes = used
                .into_iter()
                .map(|id| {
                    // An unsupplied formal belongs to this declaration, while a
                    // forwarded reference denotes a formal in the caller.
                    let next = classes.len();
                    *classes.entry((target.is_some(), id)).or_insert(next)
                })
                .collect();
            open.push(OpenCapture {
                at: parameter.at,
                slot,
                classes: capture_classes,
            });
        }
        open
    }

    pub(super) fn mark_open_instance_captures(&mut self) {
        self.callback_captures = vec![ValueCategory::Value; self.signature.parameters.len()];
        for callback in self.open_callback_captures() {
            self.callback_captures[callback.slot] = ValueCategory::Unsettled;
        }
    }

    fn read_capture_categories(
        &self,
        found: Tables<'_>,
        assigned: &[Option<ValueCategory>],
        callbacks: &[OpenCapture],
    ) -> Diagnostics {
        let selecting = RefCell::new(Vec::new());
        let mut reports = Diagnostics::new();
        let mut trial = Typer::with_test_candidates(
            self.program,
            self.module,
            self.declaration,
            self.signature,
            Tables {
                selecting: &selecting,
                ..found
            },
            &mut reports,
            self.test_candidates,
        );
        // Keep the actual inputs of a partially bound instance, changing
        // only captures whose caller still supplied another open formal.
        trial.absent_type_uses = self.absent_type_uses;
        trial.guarded_pack_faults = self.guarded_pack_faults;
        trial.instance.clone_from(&self.instance);
        trial.const_inputs.clone_from(&self.const_inputs);
        trial.pack_inputs = self.pack_inputs;
        trial.pack_functions.clone_from(&self.pack_functions);
        trial.callback_captures = vec![ValueCategory::Value; self.signature.parameters.len()];
        for callback in callbacks {
            trial.callback_captures[callback.slot] = callback
                .classes
                .iter()
                .filter_map(|&id| assigned[id])
                .fold(ValueCategory::Value, super::lifting::join_categories);
        }
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
    /// through local definitions. Each use of a mutable binding can pass
    /// it to a writing callee or assign it in its enclosing statement.
    fn capture_slots(&self, scope: &SyntaxNode, callbacks: &[OpenCapture]) -> Vec<usize> {
        let mut slots = Vec::new();
        let mut seen = HashSet::new();
        let mut variables = HashSet::new();
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
                if let Some(cb) = callbacks.iter().find(|cb| cb.at == *at) {
                    for class in &cb.classes {
                        if !slots.contains(class) {
                            slots.push(*class);
                        }
                    }
                } else if matches!(kind, LocalKind::Let | LocalKind::Var | LocalKind::Iteration) {
                    pending.push(self.report_scope(*at));
                    if *kind == LocalKind::Var && variables.insert(*at) {
                        for usage in &self.module.uses {
                            if matches!(usage.binding, Binding::Local { at: target, .. } if target == *at)
                            {
                                pending.push(self.report_scope(usage.at));
                            }
                        }
                    }
                }
            }
        }
        slots
    }
}

fn limit(report: &Diagnostic) -> bool {
    report.code == hgl_diagnostics::Code::Local(crate::codes::local::READING_LIMIT)
}
