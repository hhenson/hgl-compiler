//! Turns a call's pack spreads into typed argument entries in an instance
//! (FN-19). Argument placement uses each entry's key and reads the input
//! slot that holds it.

use hgl_diagnostics::{Category, Location};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};

use super::Typer;
use super::requirements::pack_view;
use crate::declared::{names_parameter, substitute_within};
use crate::hir::{Callee, Expr, ExprKind, Form, PackInput, PackKey};
use crate::program::{Binding, LocalKind};
use crate::syntax::{child, has_token, is_arg_keys_call, name_token, ungrouped};
use crate::types::{GenericKind, Scalar, Type};
use crate::{codes, error};

/// A concrete entry supplied by one spread in the original call.
pub(super) struct ForwardedArgument {
    /// The source pack key before binding to the destination.
    pub(super) key: PackKey,
    /// Where the spread's pack expression is written.
    pub(super) source: SyntaxNode,
    /// The typed read of the entry in the enclosing instance.
    pub(super) value: Expr,
}

impl Typer<'_> {
    /// A form that does not implement pack forwarding must not discard
    /// the spread and emit a call using only its written arguments.
    pub(super) fn unread_pack_call(&mut self, list: &SyntaxNode) -> Option<(ExprKind, Type)> {
        spread_operands(list).next()?;
        Some(self.unread_call(list))
    }

    /// Checks that a spread names a pack and preserves its call until the
    /// pack's concrete argument types and keys are available (FN-19).
    pub(super) fn defer_pack_call(
        &mut self,
        list: &SyntaxNode,
        result: Option<&Type>,
        expected: Option<&Type>,
    ) -> Option<(ExprKind, Type)> {
        let mut spreads = spread_operands(list).peekable();
        spreads.peek()?;
        let computed = spread_operands(list)
            .any(|spread| ungrouped(spread).kind() == SyntaxKind::PackComprehension);
        if self.pack_inputs.is_some() && !computed {
            return None;
        }
        let before = self.diagnostics.len();
        for spread in spreads {
            let operand = ungrouped(spread.clone());
            if operand.kind() == SyntaxKind::PackComprehension {
                self.check_pack_comprehension(&operand);
                continue;
            }
            let pack = self
                .local_of(&operand)
                .and_then(|local| self.signature.parameters.get(local))
                .is_some_and(|parameter| parameter.pack.is_some());
            if !pack {
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::TYPE_MISMATCH,
                    self.at(&spread),
                    "a spread argument needs a pack".to_string(),
                    "FN-19",
                ));
            }
        }
        if self.diagnostics.len() != before {
            return Some((ExprKind::Unread, Type::Unknown));
        }
        let ty = result
            .filter(|ty| !names_parameter(ty))
            .or(expected)
            .cloned()
            .unwrap_or(Type::Unknown);
        if computed {
            // FN-19: this spread is an inline pack. Keep the call for
            // the target to decline without instantiating its passes.
            return Some((self.unread_call(list).0, ty));
        }
        Some((
            ExprKind::Call {
                callee: Callee::DeferredPackCall,
                arguments: Vec::new(),
                form: Form::Named,
                braced: Vec::new(),
                lifted: false,
            },
            ty,
        ))
    }

    /// Check the declaration-time shape of a pack comprehension (FN-19).
    /// Its generic passes and their branch values stay unread.
    pub(super) fn check_pack_comprehension(&mut self, node: &SyntaxNode) {
        let Some(iterable) = node.children().next().map(ungrouped) else {
            return;
        };
        if !is_arg_keys_call(&iterable) {
            return;
        }
        let operand = pack_view(iterable.clone()).filter(|(view, _)| {
            view.text() == "arg_keys"
                && iterable.children().count() == 2
                && child(&iterable, SyntaxKind::ArgList)
                    .and_then(|list| list.children().next())
                    .is_some_and(|argument| !has_token(&argument, SyntaxKind::Colon))
        });
        if let Some((_, pack)) = operand {
            self.check_key_pack(&pack);
        } else {
            self.diagnostics.emit(error(
                Category::Type,
                codes::TYPE_MISMATCH,
                self.at(&iterable),
                "`arg_keys` takes a single type pack name".to_string(),
                "GEN-17",
            ));
        }
        // Membership syntax says which names are compile-time keys and
        // type packs. Neither the truth value nor a branch is selected here.
        if let Some(entry) = node.children().nth(1) {
            for membership in entry
                .descendants()
                .filter(|part| part.kind() == SyntaxKind::PackKeyMembership)
            {
                self.pack_key_membership(&membership);
            }
        }
    }

    /// `key in arg_keys(Ps)` is a compile-time bool with its pack still
    /// symbolic in this pass (GEN-17).
    pub(super) fn pack_key_membership(&mut self, node: &SyntaxNode) -> (ExprKind, Type) {
        let mut operands = node.children();
        if let Some(key) = operands.next().and_then(|node| name_token(&node))
            && let Some(binding) = self.bindings.get(&(self.file, key.span().start))
            && !matches!(
                binding,
                Binding::Local {
                    kind: LocalKind::PackKey,
                    ..
                }
            )
        {
            self.diagnostics.emit(error(
                Category::Type,
                codes::TYPE_MISMATCH,
                Location::new(self.file, key.span()),
                format!("`{}` is not a pack key", key.text()),
                "GEN-17",
            ));
        }
        if let Some(pack) = operands.next().and_then(|node| name_token(&node)) {
            self.check_key_pack(&pack);
        }
        (ExprKind::Unread, Type::Scalar(Scalar::Bool))
    }

    fn check_key_pack(&mut self, name: &SyntaxToken) {
        let Some(binding) = self.bindings.get(&(self.file, name.span().start)) else {
            return;
        };
        let type_pack = matches!(binding, Binding::Local { at, kind: LocalKind::GenericParameter }
            if self.signature.generics.iter().any(|generic| generic.at == *at
                && matches!(generic.kind, GenericKind::Pack { .. })));
        if !type_pack {
            self.diagnostics.emit(error(
                Category::Type,
                codes::TYPE_MISMATCH,
                Location::new(self.file, name.span()),
                format!(
                    "`arg_keys` needs a type pack, and `{}` is not one",
                    name.text()
                ),
                "GEN-17",
            ));
        }
    }

    /// The entries the instance gives to this call. Bind names before
    /// positions, preserving spread and capture order within each group.
    pub(super) fn forwarded_arguments(&self, list: &SyntaxNode) -> Vec<ForwardedArgument> {
        let Some(inputs) = &self.pack_inputs else {
            return Vec::new();
        };
        let mut forwarded = Vec::new();
        for source in spread_operands(list) {
            let Some(local) = self.local_of(&ungrouped(source.clone())) else {
                continue;
            };
            let category = self.locals[local].category;
            for input in inputs.iter().filter(|input| input.parameter == local) {
                let key = input.key.clone();
                let value = Expr {
                    kind: ExprKind::PackEntry {
                        pack: local,
                        key: key.clone(),
                    },
                    ty: input.ty.clone(),
                    at: self.at(&source),
                    category,
                };
                forwarded.push(ForwardedArgument {
                    key,
                    source: source.clone(),
                    value,
                });
            }
        }
        forwarded.sort_by_key(|entry| matches!(entry.key, PackKey::Position(_)));
        forwarded
    }
}

/// The operand of each spread, in the order it is written.
pub(super) fn spread_operands(list: &SyntaxNode) -> impl Iterator<Item = SyntaxNode> + '_ {
    list.children()
        .filter(|argument| argument.kind() == SyntaxKind::PackSpreadArg)
        .filter_map(|argument| argument.children().next())
}

/// Whether the shape of a parameter pack still needs to be applied.
pub(crate) fn has_forwarded_calls(body: &crate::hir::Body) -> bool {
    let mut pending = false;
    walk_nested_calls(&body.statements, &mut |value| {
        pending |= is_forwarded_call(value);
    });
    pending
}

fn is_forwarded_call(value: &Expr) -> bool {
    matches!(
        &value.kind,
        ExprKind::Call {
            callee: Callee::DeferredPackCall,
            ..
        }
    )
}

/// One concrete specialization reached by a call with a forwarding path.
#[derive(Debug, Clone, PartialEq)]
struct ForwardingInstance {
    declaration: (usize, usize),
    generics: Vec<crate::Argument>,
    requirements: Vec<crate::hir::Answer>,
    functions: Vec<(usize, Callee)>,
    type_packs: Vec<crate::hir::TypePackArgument>,
    pack_inputs: Vec<crate::hir::PackInput>,
}

impl crate::Program {
    /// The type and source key of each pack entry bound to a call (FN-8,
    /// FN-19). A concrete parameter type takes precedence over the type
    /// of the argument read at the call. For a trace that supplies an
    /// independently typed entry, use its replay payload (TST-5).
    pub fn call_pack_inputs(
        &self,
        callee: &Callee,
        signature: &crate::Signature,
        arguments: &[crate::hir::Argument],
    ) -> Vec<PackInput> {
        let mut counts = vec![0; signature.parameters.len()];
        let bindings: Vec<_> = signature
            .generics
            .iter()
            .map(|parameter| parameter.at)
            .zip(callee.generics())
            .collect();
        arguments
            .iter()
            .filter_map(|argument| {
                let parameter = argument.parameter?;
                let key = argument.pack.as_ref()?;
                let declared = &signature.parameters[parameter];
                let index = &mut counts[parameter];
                let expected = super::inference::type_pack(declared, &signature.generics)
                    .and_then(|generic| {
                        callee
                            .type_packs()
                            .iter()
                            .find(|pack| pack.parameter == generic)
                    })
                    .and_then(|pack| pack.types.get(*index))
                    .cloned()
                    .unwrap_or_else(|| substitute_within(&declared.ty, &bindings));
                *index += 1;
                let unresolved = expected.any_part(
                    &|part| {
                        matches!(
                            part,
                            Type::Unknown
                                | Type::Parameter { .. }
                                | Type::PackMember { .. }
                                | Type::Trace { .. }
                                | Type::Function(_)
                        )
                    },
                    &|constant| constant.value.key().is_none(),
                );
                let ty = if unresolved {
                    argument
                        .value
                        .ty
                        .trace_payload()
                        .unwrap_or_else(|| argument.value.ty.clone())
                } else {
                    expected
                };
                Some(PackInput {
                    parameter,
                    key: key.clone(),
                    ty,
                })
            })
            .collect()
    }
}

impl ForwardingInstance {
    fn of(program: &crate::Program, expression: &Expr) -> Option<Self> {
        use crate::types::Argument;
        let ExprKind::Call {
            callee, arguments, ..
        } = &expression.kind
        else {
            return None;
        };
        let declaration = callee.declaration()?;
        let crate::Detail::Callable(signature) =
            &program.modules[declaration.0].details[declaration.1]
        else {
            return None;
        };
        let type_packs = callee.type_packs();
        let known = |ty: &Type| super::fit::whole(ty) && !names_parameter(ty);
        if !callee
            .generics()
            .iter()
            .enumerate()
            .all(|(index, given)| match given {
                Argument::Type(ty) => {
                    known(ty) || type_packs.iter().any(|pack| pack.parameter == index)
                }
                Argument::Constant(constant) => !matches!(
                    constant.value,
                    crate::ConstantValue::Unknown
                        | crate::ConstantValue::Unevaluated
                        | crate::ConstantValue::Parameter { .. }
                        | crate::ConstantValue::Unary(..)
                        | crate::ConstantValue::Binary(..)
                ),
            })
        {
            return None;
        }
        let pack_inputs = program.call_pack_inputs(callee, signature, arguments);
        if pack_inputs.iter().any(|entry| !known(&entry.ty)) {
            return None;
        }
        let functions = arguments
            .iter()
            .filter_map(|argument| match &argument.value.kind {
                ExprKind::StaticCallable(callee) => Some((argument.parameter?, (**callee).clone())),
                _ => None,
            })
            .collect();
        let requirements = match callee {
            Callee::Function { requirements, .. } | Callee::Candidate { requirements, .. } => {
                requirements.clone()
            }
            _ => return None,
        };
        Some(Self {
            declaration,
            generics: callee.generics().to_vec(),
            requirements,
            functions,
            type_packs: type_packs.to_vec(),
            pack_inputs,
        })
    }

    fn inputs(&self) -> crate::InstanceInputs<'_> {
        crate::InstanceInputs {
            functions: &self.functions,
            type_packs: &self.type_packs,
            pack_inputs: Some(&self.pack_inputs),
        }
    }
}

/// Include anonymous bodies, which have their own local tables and
/// are separate from an ordinary HIR expression walk (FN-14).
fn visit_anonymous_bodies(value: &Expr, visit: &mut dyn FnMut(&Expr)) {
    visit(value);
    let callee = match &value.kind {
        ExprKind::Call { callee, .. } => callee,
        ExprKind::StaticCallable(callee) => callee,
        _ => return,
    };
    if let Callee::Anonymous(function) = callee {
        walk_nested_calls(&function.body.statements, visit);
    }
}

fn walk_nested_calls(statements: &[crate::hir::Stmt], visit: &mut dyn FnMut(&Expr)) {
    crate::hir::walk_statements(statements, &mut |value| {
        visit_anonymous_bodies(value, visit)
    });
}

/// Check forwarded calls under the concrete shapes that reach them (FN-19).
/// A forwarding error belongs to the call that supplies that pack shape.
pub(super) fn check_instances(
    program: &crate::Program,
    diagnostics: &mut hgl_diagnostics::Diagnostics,
) {
    use std::collections::{HashMap, HashSet, VecDeque};

    let mut forwarding = HashSet::new();
    let mut callers = HashMap::<_, Vec<_>>::new();
    for (module, unit) in program.modules.iter().enumerate() {
        for (declaration, body) in unit.bodies.iter().enumerate() {
            let Some(body) = body else { continue };
            let owner = (module, declaration);
            walk_nested_calls(&body.statements, &mut |value| {
                if let ExprKind::Call { callee, .. } = &value.kind {
                    if matches!(callee, Callee::DeferredPackCall) {
                        forwarding.insert(owner);
                    } else if let Some(called) = callee.declaration() {
                        callers.entry(called).or_default().push(owner);
                    }
                }
            });
        }
    }
    if forwarding.is_empty() {
        return;
    }
    let mut waiting: VecDeque<_> = forwarding.iter().copied().collect();
    while let Some(called) = waiting.pop_front() {
        for &owner in callers.get(&called).into_iter().flatten() {
            if forwarding.insert(owner) {
                waiting.push_back(owner);
            }
        }
    }
    let mut roots = Vec::new();
    for unit in &program.modules {
        let mut record = |value: &Expr| {
            if let Some(instance) = ForwardingInstance::of(program, value)
                && forwarding.contains(&instance.declaration)
            {
                roots.push((instance, value.at));
            }
        };
        for body in unit
            .bodies
            .iter()
            .flatten()
            .chain(unit.test_bodies.iter().flatten())
            .chain(&unit.store_body)
        {
            walk_nested_calls(&body.statements, &mut record);
        }
        if let Some(run) = &unit.run_body {
            run.call
                .walk(&mut |value| visit_anonymous_bodies(value, &mut record));
        }
        for constant in unit.constants.iter().flatten() {
            constant
                .value
                .walk(&mut |value| visit_anonymous_bodies(value, &mut record));
        }
        for default in unit.defaults.iter().flatten().flatten() {
            default
                .value
                .walk(&mut |value| visit_anonymous_bodies(value, &mut record));
        }
    }
    for (initial, at) in roots {
        let mut waiting = VecDeque::from([initial]);
        let mut seen = Vec::new();
        let mut counts = HashMap::new();
        while let Some(instance) = waiting.pop_front() {
            if seen.contains(&instance) {
                continue;
            }
            let count = counts.entry(instance.declaration).or_insert(0);
            *count += 1;
            if *count > crate::INSTANCES {
                super::requirements::past_the_limit(
                    &[at],
                    instance.declaration,
                    program,
                    diagnostics,
                );
                break;
            }
            let reports = std::cell::RefCell::new(hgl_diagnostics::Diagnostics::new());
            let mut graph_reports = hgl_diagnostics::Diagnostics::new();
            let checked = program.checked_instance(
                instance.declaration,
                &instance.generics,
                &instance.requirements,
                instance.inputs(),
                &reports,
                &mut graph_reports,
            );
            seen.push(instance);
            for report in graph_reports.as_slice() {
                if diagnostics.as_slice().contains(report) {
                    continue;
                }
                let mut report = report
                    .clone()
                    .with_related(report.primary, "the forwarded call is written here");
                report.primary = at;
                if !diagnostics.as_slice().contains(&report) {
                    diagnostics.emit(report);
                }
            }
            if let Some((_, body)) = checked {
                walk_nested_calls(&body.statements, &mut |value| {
                    if let Some(child) = ForwardingInstance::of(program, value)
                        && forwarding.contains(&child.declaration)
                    {
                        waiting.push_back(child);
                    }
                });
            }
        }
    }
}
