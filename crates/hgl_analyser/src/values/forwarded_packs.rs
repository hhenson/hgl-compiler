//! Turns a call's pack spreads into typed argument entries in an instance
//! (FN-19). Argument placement uses each entry's key and reads the input
//! slot that holds it.

use hgl_diagnostics::{Category, Location};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};

use super::Typer;
use super::requirements::{IndependentSourceBodies, called_or_referenced, pack_view};
use crate::declared::{names_parameter, substitute_within};
use crate::hir::{Callee, Expr, ExprKind, Form, PackInput, PackKey};
use crate::program::{Binding, LocalKind};
use crate::syntax::{child, has_token, is_arg_keys_call, name_token, pack_member_names, ungrouped};
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
        let mut spreads = spread_operands(list).peekable();
        spreads.peek()?;
        let before = self.diagnostics.len();
        for spread in spreads.map(ungrouped) {
            if spread.kind() == SyntaxKind::PackComprehension {
                self.check_pack_comprehension(&spread);
            }
        }
        if self.diagnostics.len() != before {
            return Some((ExprKind::Unread, Type::Unknown));
        }
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
        self.defer_pack_call_with(list, result, expected, |typer| typer.unread_call(list).0)
    }

    /// A computed pack leaves its entries unread. A known callee can
    /// still supply the types for the arguments the call writes (FN-19).
    pub(super) fn defer_pack_call_with(
        &mut self,
        list: &SyntaxNode,
        result: Option<&Type>,
        expected: Option<&Type>,
        read_written: impl FnOnce(&mut Self) -> ExprKind,
    ) -> Option<(ExprKind, Type)> {
        let mut spreads = spread_operands(list).peekable();
        spreads.peek()?;
        let computed = has_computed_spread(list);
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
            return Some((read_written(self), ty));
        }
        Some((
            ExprKind::Call {
                callee: Callee::DeferredPackCall,
                arguments: Vec::new(),
                form: Form::Named,
                text: None,
                braced: Vec::new(),
                lifted: false,
            },
            ty,
        ))
    }

    /// Check the declaration-time shape and bindings of a pack comprehension.
    /// Its generic passes and their branch values stay unread (FN-19).
    pub(super) fn check_pack_comprehension(&mut self, node: &SyntaxNode) {
        self.check_pack_iterable(node);
        let Some(entry) = node.children().nth(1) else {
            return;
        };
        self.check_pack_conditions(ungrouped(entry.clone()));
        self.check_pack_entry(&entry);
    }

    fn check_pack_entry(&mut self, node: &SyntaxNode) {
        match node.kind() {
            SyntaxKind::PackComprehension => return self.check_pack_comprehension(node),
            SyntaxKind::PackKeyMembership => {
                // GEN-17: these operands name a key and a type pack.
                self.pack_key_membership(node);
                return;
            }
            SyntaxKind::IndexExpr if self.is_pack_member_type_argument(node) => return,
            SyntaxKind::NameExpr => self.check_pack_entry_name(node),
            SyntaxKind::AssignStmt => self.check_pack_key_assignment(node),
            _ => {}
        }
        for part in node.children() {
            self.check_pack_entry(&part);
        }
    }

    /// GEN-17: the parser keeps a bare projection in generic arguments
    /// as an index until the base's binding tells whether it names a type.
    fn is_pack_member_type_argument(&self, node: &SyntaxNode) -> bool {
        node.parent()
            .is_some_and(|parent| parent.kind() == SyntaxKind::GenericArgs)
            && pack_member_names(node)
                .and_then(|(pack, _)| self.bindings.get(&(self.file, pack.span().start)))
                .is_some_and(|binding| self.is_type_pack(binding))
    }

    fn check_pack_iterable(&mut self, node: &SyntaxNode) {
        let Some(iterable) = node.children().next().map(ungrouped) else {
            return;
        };
        // The resolver checks the required iterable syntax (FN-19).
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
    }

    /// The pack's condition chain reads fixed values only (FN-19). A
    /// condition that depends on a pack pass stays symbolic here.
    fn check_pack_conditions(&mut self, mut entry: SyntaxNode) {
        while entry.kind() == SyntaxKind::IfExpr {
            if let Some(condition) = entry.children().next()
                && self.reads_runtime_binding(&condition)
            {
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::TYPE_MISMATCH,
                    self.at(&condition),
                    "a pack loop condition must be constant".to_string(),
                    "FN-19",
                ));
            }
            let Some(next) = child(&entry, SyntaxKind::IfExpr) else {
                break;
            };
            entry = next;
        }
    }

    /// Recognize a known runtime name through grouping or operators.
    /// Calls and per-key reads still need the selected pass to type them.
    fn reads_runtime_binding(&self, expression: &SyntaxNode) -> bool {
        match expression.kind() {
            SyntaxKind::ParenExpr | SyntaxKind::UnaryExpr | SyntaxKind::BinaryExpr => expression
                .children()
                .any(|operand| self.reads_runtime_binding(&operand)),
            SyntaxKind::NameExpr => self.is_runtime_guard_name(expression),
            _ => false,
        }
    }

    fn is_runtime_guard_name(&self, expression: &SyntaxNode) -> bool {
        let Some(name) = name_token(expression) else {
            return false;
        };
        let Some(Binding::Local { kind, .. }) = self.bindings.get(&(self.file, name.span().start))
        else {
            return false;
        };
        let local = self.local_of(expression);
        match kind {
            LocalKind::Parameter => !local
                .and_then(|local| self.signature.parameters.get(local))
                .is_some_and(|parameter| parameter.constant),
            // A loop without a known iterable is judged where it is
            // written. Its iteration binding does not settle a guard.
            LocalKind::Iteration => local.is_some_and(|local| {
                self.locals[local].ty != Type::Unknown
                    && !self.unrolled_range_bindings.contains(&local)
            }),
            LocalKind::Let
            | LocalKind::Var
            | LocalKind::State
            | LocalKind::Cache
            | LocalKind::Inject => true,
            _ => false,
        }
    }

    /// Names in a deferred entry still have the declaration's binding.
    fn check_pack_entry_name(&mut self, node: &SyntaxNode) {
        let Some(name) = name_token(node) else {
            return;
        };
        if let Some(binding) = self.bindings.get(&(self.file, name.span().start))
            && self.is_type_pack(binding)
        {
            self.diagnostics.emit(error(
                Category::Type,
                codes::TYPE_MISMATCH,
                self.at(node),
                format!(
                    "`{}` is a type pack and cannot be read as a value",
                    name.text()
                ),
                "FN-19",
            ));
        } else if let Some(local) = self.local_of(node) {
            self.reject_function_value(local, self.at(node), Some(node));
        }
    }

    /// EXPR-3: an iteration binding is read-only before it has a key.
    fn check_pack_key_assignment(&mut self, assignment: &SyntaxNode) {
        let Some(mut place) = assignment.children().next().map(ungrouped) else {
            return;
        };
        let at = self.at(&place);
        while matches!(place.kind(), SyntaxKind::MemberExpr | SyntaxKind::IndexExpr) {
            let Some(base) = place.children().next().map(ungrouped) else {
                return;
            };
            place = base;
        }
        if place.kind() == SyntaxKind::NameExpr
            && let Some(name) = name_token(&place)
            && let Some(Binding::Local {
                at: key,
                kind: LocalKind::PackIterationKey,
            }) = self.bindings.get(&(self.file, name.span().start))
        {
            self.diagnostics.emit(
                error(
                    Category::Name,
                    codes::BINDING_IMMUTABLE,
                    at,
                    format!("`{}` is a pack key and cannot be assigned", name.text()),
                    "EXPR-3",
                )
                .with_related(*key, "declared here"),
            );
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
                    kind: LocalKind::PackIterationKey,
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
        if !self.is_type_pack(binding) {
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

    fn is_type_pack(&self, binding: &Binding) -> bool {
        matches!(binding, Binding::Local { at, kind: LocalKind::GenericParameter }
            if self.signature.generics.iter().any(|generic| generic.at == *at
                && matches!(generic.kind, GenericKind::Pack { .. })))
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

/// Whether this call's written slots can be read while its spread stays deferred.
pub(super) fn has_computed_spread(list: &SyntaxNode) -> bool {
    spread_operands(list).any(|spread| ungrouped(spread).kind() == SyntaxKind::PackComprehension)
}

/// Whether a body needs its concrete pack inputs for a spread or entry read.
pub(crate) fn has_forwarded_calls(body: &crate::hir::Body) -> bool {
    let mut pending = false;
    walk_nested_calls(&body.statements, &mut |value| {
        pending |= needs_pack_inputs(value);
    });
    pending
}

fn needs_pack_inputs(value: &Expr) -> bool {
    matches!(
        &value.kind,
        ExprKind::Call {
            callee: Callee::DeferredPackCall,
            ..
        } | ExprKind::DeferredPackRead
    )
}

/// One concrete specialization reached by a call with a forwarding path.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ForwardingInstance {
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
    /// The declaration and fully known argument shape supplied by a call.
    pub(crate) fn of(program: &crate::Program, expression: &Expr) -> Option<Self> {
        let ExprKind::Call {
            callee, arguments, ..
        } = &expression.kind
        else {
            return None;
        };
        Self::from_callee(program, callee, Some(arguments))
    }

    /// A static reference can reach a forwarding body even without its own
    /// pack arguments, through the ordinary calls the referenced body makes.
    fn for_check(program: &crate::Program, expression: &Expr) -> Option<Self> {
        let (module, declaration) = called_or_referenced(expression)?.declaration()?;
        if let crate::Detail::Callable(signature) = &program.modules[module].details[declaration]
            && signature.generics.is_empty()
            && signature
                .parameters
                .iter()
                .all(|parameter| parameter.pack.is_none())
        {
            // Concrete bodies supply their own forwarding roots. A call
            // of one does not make those same instances a second time.
            return None;
        }
        match &expression.kind {
            ExprKind::StaticCallable(callee) => Self::from_callee(program, callee, None),
            _ => Self::of(program, expression),
        }
    }

    fn from_callee(
        program: &crate::Program,
        callee: &Callee,
        arguments: Option<&[crate::hir::Argument]>,
    ) -> Option<Self> {
        use crate::types::Argument;
        let declaration = callee.declaration()?;
        let crate::Detail::Callable(signature) =
            &program.modules[declaration.0].details[declaration.1]
        else {
            return None;
        };
        // The call of a pack-taking reference must supply its entries.
        if arguments.is_none()
            && signature
                .parameters
                .iter()
                .any(|parameter| parameter.pack.is_some())
        {
            return None;
        }
        let arguments = arguments.unwrap_or_default();
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

    /// The checked body with this caller's pack keys bound to arguments.
    pub(crate) fn body(&self, program: &crate::Program) -> Option<crate::hir::Body> {
        let (_, body) = program.instance_with_inputs(
            self.declaration,
            &self.generics,
            &self.requirements,
            self.inputs(),
        )?;
        Some(body)
    }

    fn inputs(&self) -> crate::InstanceInputs<'_> {
        crate::InstanceInputs {
            functions: &self.functions,
            type_packs: &self.type_packs,
            pack_inputs: Some(&self.pack_inputs),
            ..crate::InstanceInputs::default()
        }
    }
}

/// Include anonymous bodies, which have their own local tables and
/// are separate from an ordinary HIR expression walk (FN-14).
pub(super) fn visit_anonymous_bodies(value: &Expr, visit: &mut dyn FnMut(&Expr)) {
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

/// Walk expressions in the body and its nested anonymous functions.
pub(super) fn walk_nested_calls(statements: &[crate::hir::Stmt], visit: &mut dyn FnMut(&Expr)) {
    crate::hir::walk_statements(statements, &mut |value| {
        visit_anonymous_bodies(value, visit)
    });
}

/// GEN-13: limit growth along one path without limiting a single pack's
/// width, independent calls, or recursion over an unchanged pack shape.
const PACK_GROWTH: usize = 64;

#[derive(Clone, Copy)]
struct PackShape {
    declaration: (usize, usize),
    entries: usize,
}

impl PackShape {
    fn of(instance: &ForwardingInstance) -> Self {
        let typed_entries = instance
            .type_packs
            .iter()
            .map(|pack| pack.types_with_prefix().count())
            .sum::<usize>();
        Self {
            declaration: instance.declaration,
            entries: instance.pack_inputs.len() + typed_entries,
        }
    }

    fn exceeds_growth(self, ancestors: impl Iterator<Item = Self>) -> bool {
        if self.entries <= PACK_GROWTH {
            return false;
        }
        let mut width = self.entries;
        ancestors
            .filter(|ancestor| {
                if ancestor.declaration == self.declaration && ancestor.entries < width {
                    width = ancestor.entries;
                    true
                } else {
                    false
                }
            })
            .nth(PACK_GROWTH)
            .is_some()
    }
}

struct ForwardingParent {
    call: (super::requirements::MakingCall, Option<usize>),
    shape: PackShape,
}

/// Check pack spreads and reads under the concrete shapes that reach them
/// (FN-19). A pack error belongs to the call that supplies its shape.
/// Record the concrete calls it reaches for the ordinary GEN-7 checks.
pub(super) fn check_instances(
    program: &crate::Program,
    discovered: &mut Vec<super::requirements::Made>,
    source: &IndependentSourceBodies,
    diagnostics: &mut hgl_diagnostics::Diagnostics,
) {
    use super::requirements::{
        MakingCall, bounded_instance_key, emit_instance_report, making_path,
        past_pack_growth_limit, past_the_limit,
    };
    use hgl_diagnostics::Code;
    use std::collections::{HashMap, HashSet, VecDeque};

    let mut forwarding = HashSet::new();
    let mut callers = HashMap::<_, Vec<_>>::new();
    for (module, unit) in program.modules.iter().enumerate() {
        for (declaration, body) in unit.bodies.iter().enumerate() {
            let Some(body) = body else { continue };
            let owner = (module, declaration);
            walk_nested_calls(&body.statements, &mut |value| {
                if needs_pack_inputs(value) {
                    forwarding.insert(owner);
                }
                if let Some(called) = called_or_referenced(value).and_then(Callee::declaration) {
                    callers.entry(called).or_default().push(owner);
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
    let mut independent = HashMap::<Location, Vec<ForwardingInstance>>::new();
    for (module, unit) in program.modules.iter().enumerate() {
        let mut record = |value: &Expr, in_forwarding_body: bool| {
            if let Some(instance) = ForwardingInstance::for_check(program, value) {
                if in_forwarding_body {
                    independent
                        .entry(value.at)
                        .or_default()
                        .push(instance.clone());
                }
                if forwarding.contains(&instance.declaration) {
                    roots.push((instance, value.at));
                }
            }
        };
        for (declaration, body) in unit.bodies.iter().enumerate() {
            let owner = (module, declaration);
            if let Some(body) = source.body(owner).or(body.as_ref()) {
                let in_forwarding_body = forwarding.contains(&owner);
                super::walk_body_calls(body, &mut |value| record(value, in_forwarding_body));
            }
        }
        for body in unit.test_bodies.iter().flatten().chain(&unit.store_body) {
            super::walk_body_calls(body, &mut |value| record(value, false));
        }
        if let Some(run) = &unit.run_body {
            run.call
                .walk(&mut |value| visit_anonymous_bodies(value, &mut |part| record(part, false)));
        }
        for constant in unit.constants.iter().flatten() {
            constant
                .value
                .walk(&mut |value| visit_anonymous_bodies(value, &mut |part| record(part, false)));
        }
        for default in unit.defaults.iter().flatten().flatten() {
            default
                .value
                .walk(&mut |value| visit_anonymous_bodies(value, &mut |part| record(part, false)));
        }
    }
    let rejected_pack_calls: HashSet<_> = diagnostics
        .as_slice()
        .iter()
        .filter(|report| report.code == Code::Catalogued(codes::PACK_NAME))
        .map(|report| report.primary)
        .collect();
    for (initial, at) in roots {
        let mut waiting = VecDeque::from([(initial, at, None::<usize>)]);
        let mut parents = Vec::<ForwardingParent>::new();
        let mut seen = Vec::new();
        let mut counts = HashMap::new();
        while let Some((instance, written, parent)) = waiting.pop_front() {
            if seen.contains(&instance) {
                continue;
            }
            let count = counts.entry(instance.declaration).or_insert(0);
            *count += 1;
            let shape = PackShape::of(&instance);
            let ancestors = std::iter::successors(parent, |&index| parents[index].call.1)
                .map(|index| parents[index].shape);
            if shape.exceeds_growth(ancestors) {
                past_pack_growth_limit(at, instance.declaration, program, PACK_GROWTH, diagnostics);
                break;
            }
            if *count > crate::INSTANCES
                || bounded_instance_key(
                    instance.declaration,
                    &instance.generics,
                    &instance.type_packs,
                    &instance.requirements,
                )
                .is_none()
            {
                past_the_limit(&[at], instance.declaration, program, diagnostics);
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
            let making = || {
                MakingCall::new(
                    program,
                    instance.declaration,
                    &instance.generics,
                    &instance.type_packs,
                    written,
                )
            };
            for report in graph_reports.as_slice() {
                // A missing member in the written signature rejects the
                // making call already. Its body supplies no second pack
                // diagnostic at the same call (ERR-9).
                if diagnostics.as_slice().contains(report)
                    || (report.code == Code::Catalogued(codes::PACK_NAME)
                        && rejected_pack_calls.contains(&at))
                {
                    continue;
                }
                emit_instance_report(
                    report.clone(),
                    &[making_path(making(), parent, |index| &parents[index].call)],
                    diagnostics,
                );
            }
            if let Some(checked) = checked {
                if let Some(result_uses) = &checked.result_uses
                    && !result_uses.reports.is_empty()
                {
                    super::requirements::report_instance_result_uses(
                        program,
                        (
                            instance.declaration,
                            &instance.generics,
                            &instance.type_packs,
                        ),
                        result_uses,
                        &[making_path(making(), parent, |index| &parents[index].call)],
                        diagnostics,
                    );
                    if result_uses.reports.has_errors() {
                        seen.push(instance);
                        continue;
                    }
                }
                let mut link = None;
                checked.walk_calls(&mut |value| {
                    let Some(child) = ForwardingInstance::for_check(program, value) else {
                        return;
                    };
                    // The final body can make new calls wherever the
                    // entries affect a type, including in a callback.
                    if !independent
                        .get(&value.at)
                        .is_some_and(|calls| calls.contains(&child))
                        && let Some(callee) = called_or_referenced(value)
                    {
                        let mut path = vec![MakingCall::new(
                            program,
                            child.declaration,
                            &child.generics,
                            &child.type_packs,
                            value.at,
                        )];
                        path.extend(making_path(making(), parent, |index| &parents[index].call));
                        super::requirements::made_forwarded_callee(callee, at, path, discovered);
                    }
                    if forwarding.contains(&child.declaration) {
                        let caller = *link.get_or_insert_with(|| {
                            let index = parents.len();
                            parents.push(ForwardingParent {
                                call: (making(), parent),
                                shape,
                            });
                            index
                        });
                        waiting.push_back((child, value.at, Some(caller)));
                    }
                });
            }
            seen.push(instance);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ForwardingInstance, PACK_GROWTH, PackInput, PackKey, PackShape, Scalar, Type};
    use crate::hir::{ReflectedInputs, TypePackArgument};
    use crate::{Argument, Constant, ConstantValue};

    /// GEN-13: the growth bound follows increasing widths of the same
    /// declaration across a call path. Unchanged and unrelated shapes do
    /// not spend that allowance.
    #[test]
    fn pack_growth_is_bounded_along_one_path() {
        let shape = PackShape {
            declaration: (0, 0),
            entries: PACK_GROWTH + 1,
        };
        let smaller = (0..shape.entries)
            .rev()
            .map(|entries| PackShape { entries, ..shape });
        assert!(!shape.exceeds_growth(smaller.clone().take(PACK_GROWTH)));
        assert!(shape.exceeds_growth(smaller));
        assert!(!shape.exceeds_growth(std::iter::repeat_n(shape, PACK_GROWTH * 2)));
        let other = PackShape {
            declaration: (0, 1),
            entries: 0,
        };
        assert!(!shape.exceeds_growth(std::iter::repeat_n(other, PACK_GROWTH * 2)));
    }

    /// GEN-13 and GEN-17: the shape includes both forwarded arguments and
    /// reflected types, independently of the const arguments beside them.
    #[test]
    fn pack_width_counts_bound_and_reflected_entries() {
        let ty = Type::Scalar(Scalar::I64);
        let mut reflected = TypePackArgument::positional(1, vec![ty.clone()]);
        reflected.reflected = Some(ReflectedInputs {
            fixed: 1,
            prefix: vec![(None, ty.clone())],
        });
        let mut instance = ForwardingInstance {
            declaration: (0, 0),
            generics: vec![Argument::Constant(Constant {
                text: "1".into(),
                value: ConstantValue::Int(1),
            })],
            requirements: Vec::new(),
            functions: Vec::new(),
            type_packs: vec![reflected],
            pack_inputs: vec![PackInput {
                parameter: 0,
                key: PackKey::Position(0),
                ty,
            }],
        };
        assert_eq!(PackShape::of(&instance).entries, 3);
        instance.type_packs.clear();
        assert_eq!(PackShape::of(&instance).entries, 1);
        instance.generics.clear();
        assert_eq!(PackShape::of(&instance).entries, 1);
    }
}
