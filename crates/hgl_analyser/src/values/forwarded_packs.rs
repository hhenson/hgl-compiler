//! Turns a call's pack spreads into typed argument entries in an instance
//! (FN-19). Argument placement uses each entry's key and reads the input
//! slot that holds it.

use std::collections::{HashMap, HashSet};

use hgl_diagnostics::{Category, Diagnostics, Location};
use hgl_parser::ast::AstNode;
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};

use super::Typer;
use super::requirements::{IndependentSourceBodies, called_or_referenced, pack_view};
use crate::constant::{Fault, Scope};
use crate::declared::{names_parameter, substitute_within};
use crate::hir::{
    BinaryOperator, Callee, Expr, ExprKind, Form, Local, LocalId, PACK_ENTRY_LOCAL_PREFIX,
    PackInput, PackKey, Stmt, TypePackArgument, ValueCategory,
};
use crate::program::{Binding, LocalKind};
use crate::syntax::{
    child, has_token, is_arg_keys_call, is_pack_key_if, name_token, pack_key_branches,
    pack_member_names, tokens, ungrouped,
};
use crate::types::{Argument, Constant, ConstantValue, GenericKind, Pack, Parameter, Scalar, Type};
use crate::{codes, error};

/// Binding and written-type passes still record source information for
/// both arms. Keep their diagnostics with the innermost CTRL-16 arm.
pub(crate) fn defer_pack_branch_reports(
    program: &mut crate::Program,
    diagnostics: &mut Diagnostics,
    start: usize,
) {
    if diagnostics.len() == start {
        return;
    }
    let files: HashSet<_> = diagnostics.as_slice()[start..]
        .iter()
        .map(|report| report.primary.file)
        .collect();
    let mut branches = HashMap::<_, Vec<_>>::new();
    for file in program
        .modules
        .iter()
        .flat_map(|module| &module.files)
        .filter(|file| files.contains(&file.file))
    {
        let found: Vec<_> = pack_key_branches(file.syntax.syntax())
            .map(|branch| branch.span())
            .collect();
        if !found.is_empty() {
            branches.insert(file.file, found);
        }
    }
    if branches.is_empty() {
        return;
    }
    let reports = diagnostics.as_slice()[start..].to_vec();
    diagnostics.truncate(start);
    for report in reports {
        let at = report.primary;
        let branch = branches.get(&at.file).and_then(|branches| {
            branches
                .iter()
                .filter(|span| span.start <= at.span.start && at.span.end <= span.end)
                .min_by_key(|span| span.end - span.start)
        });
        if let Some(branch) = branch {
            program
                .pack_branch_reports
                .entry(Location::new(at.file, *branch))
                .or_default()
                .push(report);
        } else {
            diagnostics.emit(report);
        }
    }
}

/// A concrete entry supplied by one spread in the original call.
pub(super) struct ForwardedArgument {
    /// The source pack key before binding to the destination.
    pub(super) key: PackKey,
    /// Where the spread's pack expression is written.
    pub(super) source: SyntaxNode,
    /// The typed read of the entry in the enclosing instance.
    pub(super) value: Expr,
}

/// The compile-time shape of a pack bound by `let` (FN-19).
pub(super) enum BoundPack {
    /// This binding awaits the instance or a condition's constant value.
    Deferred,
    /// This binding refers to the same entries as another pack.
    Alias(LocalId),
    /// Each entry is initialized once, in pass order, in its own local.
    Entries(Vec<(PackKey, LocalId)>),
}

impl<'a> Typer<'a> {
    /// A concrete membership condition selects this arm's source reports.
    pub(super) fn report_pack_branch(&mut self, branch: &SyntaxNode) {
        if let Some(selected) = self.selected_pack_branches {
            selected
                .borrow_mut()
                .extend(branch.ancestors().filter_map(|arm| {
                    let parent = arm.parent()?;
                    (is_pack_key_if(&parent) && parent.children().skip(1).any(|child| child == arm))
                        .then(|| self.at(&arm))
                }));
        }
        if let Some(reports) = self.program.pack_branch_reports.get(&self.at(branch)) {
            for report in reports {
                self.diagnostics.emit(report.clone());
            }
        }
    }

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
        let computed = self.deferred_computed_spread(list);
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
                .is_some_and(|local| self.names_pack(local));
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
            // GEN-7: keep the written arguments, but reread the spread
            // and the call after the enclosing instance supplies its keys.
            let mut kind = read_written(self);
            if let ExprKind::Call { callee, .. } = &mut kind {
                *callee = Callee::DeferredPackCall;
            }
            return Some((kind, ty));
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

    /// FN-19: a `let` names an existing pack or holds each selected
    /// entry of a comprehension in its own local.
    pub(super) fn bind_let_pack(
        &mut self,
        declaration: &SyntaxNode,
        statements: &mut Vec<Stmt>,
    ) -> bool {
        if declaration.kind() != SyntaxKind::LocalDecl || !has_token(declaration, SyntaxKind::KwLet)
        {
            return false;
        }
        let Some(pack) = declaration
            .children()
            .find(|part| !crate::read::is_type(part.kind()))
            .map(ungrouped)
        else {
            return false;
        };
        let source = if pack.kind() == SyntaxKind::NameExpr {
            self.local_of(&pack).filter(|&local| self.names_pack(local))
        } else {
            None
        };
        if source.is_none() && pack.kind() != SyntaxKind::PackComprehension {
            return false;
        }
        let Some(name) = tokens(declaration).find(|token| token.kind() == SyntaxKind::Ident) else {
            return false;
        };
        let outer = self.begin_construct(declaration);
        if let Some(annotation) = declaration
            .children()
            .find(|part| crate::read::is_type(part.kind()))
        {
            self.diagnostics.emit(error(
                Category::Type,
                codes::TYPE_MISMATCH,
                self.at(&annotation),
                "a pack binding does not take an ordinary value type".to_string(),
                "FN-19",
            ));
        }
        let binding = match source {
            Some(source) => BoundPack::Alias(self.pack_origin(source)),
            None => self.computed_pack_binding(&pack, statements),
        };
        let local = self.declare(
            name.text(),
            Location::new(self.file, name.span()),
            LocalKind::Let,
            Type::Unknown,
            true,
        );
        self.bound_packs.insert(local, binding);
        self.end_construct(outer);
        true
    }

    fn computed_pack_binding(
        &mut self,
        comprehension: &SyntaxNode,
        statements: &mut Vec<Stmt>,
    ) -> BoundPack {
        if self.computed_spread_ready(comprehension) {
            let mut entries = Vec::new();
            self.computed_arguments(comprehension, &mut entries);
            let mut locals = Vec::with_capacity(entries.len());
            for entry in entries {
                let local = self.locals.len();
                let category = if self.graph {
                    self.category(&entry.value)
                } else {
                    ValueCategory::Value
                };
                self.locals.push(Local {
                    name: format!("{PACK_ENTRY_LOCAL_PREFIX}{local}"),
                    at: entry.value.at,
                    kind: LocalKind::Let,
                    ty: entry.value.ty.clone(),
                    category,
                });
                self.assigned.push(true);
                statements.push(Stmt::Declare {
                    local,
                    value: Some(entry.value),
                });
                locals.push((entry.key, local));
            }
            BoundPack::Entries(locals)
        } else {
            self.check_pack_comprehension(comprehension);
            statements.push(Stmt::Expr(Expr {
                kind: ExprKind::DeferredPackRead,
                ty: Type::Unknown,
                at: self.at(comprehension),
                category: ValueCategory::Unsettled,
            }));
            BoundPack::Deferred
        }
    }

    /// Whether a name denotes a parameter pack or a computed pack binding.
    pub(super) fn names_pack(&self, local: LocalId) -> bool {
        self.bound_packs.contains_key(&local) || self.parameter_pack(local).is_some()
    }

    /// The owning parameter or comprehension, through any `let` binding.
    pub(super) fn pack_origin(&self, local: LocalId) -> LocalId {
        match self.bound_packs.get(&local) {
            Some(BoundPack::Alias(origin)) => *origin,
            _ => local,
        }
    }

    /// A pack parameter is the local for that exact lexical declaration.
    pub(super) fn parameter_pack(&self, local: LocalId) -> Option<&'a Parameter> {
        let parameter = self.signature.parameters.get(local)?;
        (self.locals[local].at == parameter.at && parameter.pack.is_some()).then_some(parameter)
    }

    /// The FN-19 `total` example iterates a homogeneous positional parameter
    /// pack. Its concrete entries make the loop sequence in this instance.
    pub(super) fn homogeneous_pack_iterable(&mut self, node: &SyntaxNode) -> Option<Expr> {
        let pack = self.pack_origin(self.local_of(&ungrouped(node.clone()))?);
        let parameter = self.parameter_pack(pack)?;
        if parameter.pack != Some(Pack::Positional)
            || super::inference::type_pack(parameter, &self.signature.generics).is_some()
        {
            return None;
        }
        let at = self.at(node);
        let input = self.is_temporal_input(pack);
        let local_category = self.locals[pack].category;
        let payload = |ty: &Type| {
            if input && self.hook.is_some() {
                super::fit::hook_payload_type(ty)
            } else {
                ty.clone()
            }
        };
        let element = payload(&parameter.ty);
        let Some(inputs) = self.pack_inputs else {
            return Some(Expr {
                kind: ExprKind::DeferredPackRead,
                ty: Type::list(element, None),
                at,
                category: ValueCategory::Unsettled,
            });
        };
        let entries: Vec<_> = inputs
            .values
            .iter()
            .filter(|input| input.parameter == pack)
            .map(|entry| Expr {
                kind: ExprKind::PackEntry {
                    pack,
                    key: entry.key.clone(),
                },
                ty: payload(&entry.ty),
                at,
                category: local_category,
            })
            .collect();
        if input && !entries.is_empty() {
            self.input_read(pack, at, super::children::read_whole_all_valid(&element));
            self.signal_read(pack, at, Some((&element, node)));
        }
        // CTRL-15: a graph composes a pass for each fitted entry. The
        // pack's membership is fixed before any of its connections tick.
        let size = self
            .graph
            .then(|| i64::try_from(entries.len()).ok())
            .flatten()
            .map(|count| Constant {
                text: count.to_string(),
                value: ConstantValue::Int(count),
            });
        Some(Expr {
            kind: ExprKind::Sequence(entries),
            ty: Type::list(element, size),
            at,
            category: ValueCategory::Unsettled,
        })
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

    /// A membership test uses only compile-time keys (GEN-17, CTRL-16).
    pub(super) fn pack_key_membership(&mut self, node: &SyntaxNode) -> (ExprKind, Type) {
        let boolean = Type::Scalar(Scalar::Bool);
        let mut operands = node.children();
        let (Some(key), Some(pack)) = (
            operands.next(),
            operands.next().and_then(|pack| name_token(&pack)),
        ) else {
            return (ExprKind::Unread, boolean);
        };
        let key_valid = match self.typed_in(self.file, &key, None) {
            Ok(read)
                if read.ty().is_none_or(|ty| {
                    matches!(ty, Type::Unknown | Type::Scalar(Scalar::I64 | Scalar::Str))
                }) =>
            {
                true
            }
            Err(Fault::Reported | Fault::Failed) => false,
            _ => {
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::TYPE_MISMATCH,
                    self.at(&key),
                    "a pack membership key must be a module constant or a generic value parameter of type `i64` or `str`".to_string(),
                    "CTRL-16",
                ));
                false
            }
        };
        let pack_valid = self.check_key_pack(&pack);
        let kind = if key_valid && pack_valid {
            self.read_pack_condition(node, None)
                .map_or(ExprKind::DeferredPackCondition, |selected| {
                    ExprKind::Literal(crate::hir::Value::Bool(selected))
                })
        } else {
            ExprKind::Unread
        };
        (kind, boolean)
    }

    fn check_key_pack(&mut self, name: &SyntaxToken) -> bool {
        let Some(binding) = self.bindings.get(&(self.file, name.span().start)) else {
            return false;
        };
        let valid = self.is_type_pack(binding);
        if !valid {
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
        valid
    }

    fn is_type_pack(&self, binding: &Binding) -> bool {
        matches!(binding, Binding::Local { at, kind: LocalKind::GenericParameter }
            if self.signature.generics.iter().any(|generic| generic.at == *at
                && matches!(generic.kind, GenericKind::Pack { .. })))
    }

    /// GEN-7: the pack's keys exist only in the instance that supplies it.
    fn computed_pack(&self, comprehension: &SyntaxNode) -> Option<&'a TypePackArgument> {
        let inputs = self.pack_inputs?;
        let iterable = ungrouped(comprehension.children().next()?);
        let (_, pack) = pack_view(iterable)?;
        let Binding::Local { at, .. } = self.binding_at(self.file, pack.span().start)? else {
            return None;
        };
        inputs
            .types
            .bindings()
            .iter()
            .find(|(declared, pack)| {
                declared == at && !pack.infer_from_arguments && pack.forwarded_from.is_none()
            })
            .map(|(_, pack)| *pack)
    }

    /// Whether a written computed spread must wait for its own keys.
    pub(super) fn deferred_computed_spread(&self, list: &SyntaxNode) -> bool {
        spread_operands(list).map(ungrouped).any(|spread| {
            if spread.kind() == SyntaxKind::PackComprehension {
                !self.computed_spread_ready(&spread)
            } else {
                self.local_of(&spread).is_some_and(|local| {
                    matches!(
                        self.bound_packs.get(&self.pack_origin(local)),
                        Some(BoundPack::Deferred)
                    )
                })
            }
        })
    }

    /// Keep the whole spread unread when the constant reader cannot yet
    /// choose a branch. A known type error is reported by the pass itself.
    fn computed_spread_ready(&self, comprehension: &SyntaxNode) -> bool {
        let (Some(pack), Some(name), Some(entry)) = (
            self.computed_pack(comprehension),
            name_token(comprehension),
            comprehension.children().nth(1),
        ) else {
            return false;
        };
        let binding = Location::new(self.file, name.span());
        // UNIT-12: a nested duplicate has already been reported by the
        // resolver. Its body still names the outer key, so don't expand it.
        if self
            .pack_passes
            .iter()
            .any(|(active, _)| active == name.text())
        {
            return false;
        }
        pack.keys.iter().all(|key| {
            let Some(argument) = pack_key_argument(key) else {
                return false;
            };
            matches!(
                self.selected_pack_entry(ungrouped(entry.clone()), (binding, &argument)),
                Ok(_) | Err((_, Fault::Mismatch(_) | Fault::NoMember(_) | Fault::NoEnum))
            )
        })
    }

    /// The entries the instance gives to this call. Bind names before
    /// positions, preserving spread and capture order within each group.
    pub(super) fn forwarded_arguments(&mut self, list: &SyntaxNode) -> Vec<ForwardedArgument> {
        let Some(inputs) = self.pack_inputs else {
            return Vec::new();
        };
        let mut forwarded = Vec::new();
        for source in spread_operands(list) {
            let operand = ungrouped(source.clone());
            if operand.kind() == SyntaxKind::PackComprehension {
                self.computed_arguments(&operand, &mut forwarded);
                continue;
            }
            let Some(local) = self.local_of(&operand) else {
                continue;
            };
            let local = self.pack_origin(local);
            if let Some(BoundPack::Entries(entries)) = self.bound_packs.get(&local) {
                let at = self.at(&source);
                forwarded.extend(entries.iter().map(|(key, entry)| ForwardedArgument {
                    key: key.clone(),
                    source: source.clone(),
                    value: Expr {
                        kind: ExprKind::Local(*entry),
                        ty: self.locals[*entry].ty.clone(),
                        at,
                        category: self.locals[*entry].category,
                    },
                }));
                continue;
            }
            let category = self.locals[local].category;
            for input in inputs
                .values
                .iter()
                .filter(|input| input.parameter == local)
            {
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

    /// FN-19 and CTRL-16: each pass reads its key as a constant and
    /// contributes only the branch it selects, if any.
    fn computed_arguments(
        &mut self,
        comprehension: &SyntaxNode,
        forwarded: &mut Vec<ForwardedArgument>,
    ) {
        let (Some(pack), Some(name), Some(entry)) = (
            self.computed_pack(comprehension),
            name_token(comprehension),
            comprehension.children().nth(1),
        ) else {
            return;
        };
        let key_binding = Location::new(self.file, name.span());
        let before = self.bound.len();
        for key in pack.keys.iter().cloned() {
            let Some(argument) = pack_key_argument(&key) else {
                continue;
            };
            match self.selected_pack_entry(ungrouped(entry.clone()), (key_binding, &argument)) {
                Ok(Some(source)) => {
                    self.bound.push((key_binding, argument));
                    self.pack_passes
                        .push((name.text().to_string(), key.clone()));
                    self.report_pack_branch(&source);
                    let value = self.expression(&source, None);
                    forwarded.push(ForwardedArgument { key, source, value });
                    self.pack_passes.pop();
                    self.bound.truncate(before);
                }
                Ok(None) => {}
                Err((condition, _)) => self.diagnostics.emit(error(
                    Category::Type,
                    codes::TYPE_MISMATCH,
                    self.at(&condition),
                    "a pack loop condition must be a constant bool".to_string(),
                    "FN-19",
                )),
            }
        }
    }

    /// Find the selected expression of an `if` chain, or omit this key.
    fn selected_pack_entry(
        &self,
        mut entry: SyntaxNode,
        key: (Location, &Argument),
    ) -> Result<Option<SyntaxNode>, (SyntaxNode, Fault)> {
        while entry.kind() == SyntaxKind::IfExpr {
            let mut parts = entry.children();
            let (Some(condition), Some(then)) = (parts.next(), parts.next()) else {
                return Ok(None);
            };
            if self
                .pack_condition(&condition, key)
                .map_err(|fault| (condition, fault))?
            {
                return Ok(Some(then));
            }
            let Some(otherwise) = parts.next() else {
                return Ok(None);
            };
            entry = ungrouped(otherwise);
        }
        Ok(Some(entry))
    }

    fn pack_condition(
        &self,
        condition: &SyntaxNode,
        key: (Location, &Argument),
    ) -> Result<bool, Fault> {
        self.read_pack_condition(condition, Some(key))
    }

    /// CTRL-16 also applies to a constant condition nested in an entry.
    pub(super) fn active_pack_condition(&self, node: &SyntaxNode) -> Option<bool> {
        if self.pack_passes.is_empty() {
            return None;
        }
        self.read_pack_condition(node, None).ok()
    }

    fn read_pack_condition(
        &self,
        condition: &SyntaxNode,
        key: Option<(Location, &Argument)>,
    ) -> Result<bool, Fault> {
        let expected = Type::Scalar(Scalar::Bool);
        match self.pack_pass_constant(condition, &expected, key)? {
            ConstantValue::Bool(value) => Ok(value),
            _ => Err(Fault::NotConstant),
        }
    }

    /// GEN-17: keep a pass key's comparison with a constant of the other
    /// kind, also inside a nested body that captures that key (FN-14).
    pub(super) fn pack_pass_key_comparison(
        &self,
        operator: BinaryOperator,
        left: &SyntaxNode,
        right: &SyntaxNode,
    ) -> Option<bool> {
        if self.bound.is_empty()
            || !matches!(operator, BinaryOperator::Equal | BinaryOperator::NotEqual)
        {
            return None;
        }
        let (key, other) = self
            .pack_iteration_key(left)
            .map(|key| (key, right))
            .or_else(|| self.pack_iteration_key(right).map(|key| (key, left)))?;
        let other_type = Type::Scalar(match key {
            ConstantValue::Int(_) => Scalar::Str,
            ConstantValue::Str(_) => Scalar::I64,
            _ => return None,
        });
        let other = self.pack_pass_constant(other, &other_type, None).ok()?;
        crate::constant::different_pack_key_kinds(key, &other)
            .then_some(operator == BinaryOperator::NotEqual)
    }

    fn pack_iteration_key(&self, node: &SyntaxNode) -> Option<&ConstantValue> {
        let node = ungrouped(node.clone());
        if node.kind() != SyntaxKind::NameExpr {
            return None;
        }
        let name = name_token(&node)?;
        let Binding::Local {
            at,
            kind: LocalKind::PackIterationKey,
        } = self.bindings.get(&(self.file, name.span().start))?
        else {
            return None;
        };
        self.bound_pack_key(*at)
    }

    /// The lexical constant for a key bound while reading a concrete pass.
    pub(super) fn bound_pack_key(&self, declared: Location) -> Option<&ConstantValue> {
        self.bound.iter().find_map(|(at, argument)| match argument {
            Argument::Constant(constant) if *at == declared => Some(&constant.value),
            _ => None,
        })
    }

    fn pack_pass_constant(
        &self,
        node: &SyntaxNode,
        expected: &Type,
        key: Option<(Location, &Argument)>,
    ) -> Result<ConstantValue, Fault> {
        let inputs = self.pack_inputs.ok_or(Fault::NotConstant)?;
        let arguments: Vec<_> = self
            .instance
            .iter()
            .chain(&self.bound)
            .map(|(at, value)| (*at, value))
            .chain(key)
            .collect();
        let binding = |file, start| self.binding_at(file, start);
        let parameter = |at| {
            let generic = self
                .signature
                .generics
                .iter()
                .find(|generic| generic.at == at)?;
            match &generic.kind {
                GenericKind::Value(ty) => Some(ty.clone()),
                _ => None,
            }
        };
        let scope = Scope {
            program: self.program,
            file: self.file,
            binding: &binding,
            parameter: &parameter,
            arguments: &arguments,
            type_packs: inputs.types.bindings(),
            constants: &self.constants,
        };
        let read = crate::constant::typed(node, Some(expected), &scope)?;
        crate::constant::fit(read, Some(expected), self.program)
    }

    /// GEN-17: an annotation in a computed pass sees its key's concrete
    /// type, which was still unknown when the function body was entered.
    pub(super) fn project_pack_pass_type(&mut self, ty: &Type, at: Location) -> Type {
        let Some(inputs) = self.pack_inputs.filter(|_| !self.pack_passes.is_empty()) else {
            return ty.clone();
        };
        if !ty.any_part(
            &|part| matches!(part, Type::PackMember { .. }),
            &|constant| constant.value.key().is_none(),
        ) {
            return ty.clone();
        }
        let bound: Vec<_> = self.bound.iter().map(|(at, value)| (*at, value)).collect();
        match inputs.types.project(ty, &bound) {
            Ok(ty) => ty,
            Err(fault) => {
                self.pack_member_fault(at, &fault);
                Type::Unknown
            }
        }
    }
}

/// An integer position or a name read from the pass in which it is bound.
fn pack_key_argument(key: &PackKey) -> Option<Argument> {
    let value = match key {
        PackKey::Name(name) => ConstantValue::Str(name.clone()),
        PackKey::Position(position) => ConstantValue::Int(i64::try_from(*position).ok()?),
    };
    Some(Argument::Constant(Constant {
        text: value.key().unwrap_or_default(),
        value,
    }))
}

/// The operand of each spread, in the order it is written.
pub(super) fn spread_operands(list: &SyntaxNode) -> impl Iterator<Item = SyntaxNode> + '_ {
    list.children()
        .filter(|argument| argument.kind() == SyntaxKind::PackSpreadArg)
        .filter_map(|argument| argument.children().next())
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
            | ExprKind::DeferredPackCondition
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
    /// The ordinary const inputs a check in the body reads (FN-1, FMT-3),
    /// carried with the entries so the instance is read with both.
    constants: crate::const_inputs::ReadInputs,
    /// The module's test candidates remain visible through a pack call (TST-1).
    test_candidates: Option<crate::TestCandidateScope>,
}

/// GEN-7, FN-17/19: inputs that can change a forwarding body's checks.
/// Runtime copies of an already checked anonymous source are not new
/// generic arguments. Keep their typed source graph instead of their
/// recursively substituted bodies. The complete HIR remains in the instance.
#[derive(PartialEq)]
struct ForwardingCheck {
    declaration: (usize, usize),
    generics: Vec<crate::Argument>,
    requirements: Vec<crate::hir::Answer>,
    functions: Vec<(usize, CallableCheck)>,
    type_packs: Vec<crate::hir::TypePackArgument>,
    pack_inputs: Vec<crate::hir::PackInput>,
    constants: crate::const_inputs::ReadInputs,
    test_candidates: Option<crate::TestCandidateScope>,
    body_calls: Option<Vec<CheckedCall>>,
    body_reports: Vec<hgl_diagnostics::Diagnostic>,
}

impl ForwardingCheck {
    fn of(program: &crate::Program, instance: &ForwardingInstance) -> Self {
        Self {
            declaration: instance.declaration,
            generics: instance.generics.clone(),
            requirements: instance.requirements.clone(),
            functions: instance
                .functions
                .iter()
                .map(|(slot, callee)| (*slot, CallableCheck::of(program, callee)))
                .collect(),
            type_packs: instance.type_packs.clone(),
            pack_inputs: instance.pack_inputs.clone(),
            constants: instance.constants.clone(),
            test_candidates: instance.test_candidates,
            body_calls: None,
            body_reports: Vec::new(),
        }
    }
}

/// One callable's validation graph. Edges retain selected identities and
/// demanded constants; anonymous vertices retain their complete type context.
struct CallableCheck {
    root: CallableSource,
    bodies: Vec<AnonymousCheck>,
}

impl PartialEq for CallableCheck {
    fn eq(&self, other: &Self) -> bool {
        self.root == other.root
            && self.bodies.len() == other.bodies.len()
            && self.bodies.iter().all(|body| other.bodies.contains(body))
    }
}

#[derive(PartialEq)]
enum CallableSource {
    Selected(Callee),
    Anonymous {
        at: Location,
        // Boxing the template keeps every call/source key compact.
        template: Box<crate::types::FunctionType>,
        specialization: crate::hir::AnonymousSpecialization,
        captures: Vec<(Location, Type, ValueCategory, Option<PackKey>)>,
    },
}

impl CallableSource {
    fn of(callee: &Callee) -> Self {
        let Callee::Anonymous(function) = callee else {
            return Self::Selected(callee.clone());
        };
        Self::Anonymous {
            at: function.at,
            template: Box::new(function.template.clone()),
            specialization: function.specialization.clone(),
            captures: (0..function.captures.len())
                .map(|index| {
                    let local = &function.body.locals[function.template.parameters.len() + index];
                    (
                        local.at,
                        local.ty.clone(),
                        local.category,
                        function.capture_pack_key(index).cloned(),
                    )
                })
                .collect(),
        }
    }
}

#[derive(PartialEq)]
struct AnonymousCheck {
    source: CallableSource,
    calls: Vec<CheckedCall>,
}

/// A selected call and the ordinary values its compile-time checks read.
type CheckedCall = (Location, CallableSource, crate::const_inputs::ReadInputs);

fn checked_calls(program: &crate::Program, body: &crate::hir::Body) -> Vec<CheckedCall> {
    let mut calls = Vec::new();
    crate::hir::walk_statements(&body.statements, &mut |value| {
        let (callee, constants) = match &value.kind {
            ExprKind::Call {
                callee, arguments, ..
            } => (callee, program.known_const_inputs(callee, arguments)),
            ExprKind::StaticCallable(callee) => {
                (&**callee, crate::const_inputs::ReadInputs::default())
            }
            _ => return,
        };
        calls.push((value.at, CallableSource::of(callee), constants));
    });
    calls
}

impl CallableCheck {
    fn of(program: &crate::Program, callee: &Callee) -> Self {
        let mut bodies = Vec::new();
        let mut waiting = vec![callee];
        while let Some(callee) = waiting.pop() {
            let Callee::Anonymous(function) = callee else {
                continue;
            };
            let mut calls = Vec::new();
            // Use the explicit-lifetime node walker so pending callees borrow
            // their HIR. Following anonymous bodies needs no recursive clone.
            crate::hir::walk_statement_nodes(&function.body.statements, &mut |node| {
                let crate::hir::Node::Expression(value) = node else {
                    return;
                };
                let (target, constants) = match &value.kind {
                    ExprKind::Call {
                        callee, arguments, ..
                    } => (callee, program.known_const_inputs(callee, arguments)),
                    ExprKind::StaticCallable(callee) => {
                        (&**callee, crate::const_inputs::ReadInputs::default())
                    }
                    _ => return,
                };
                calls.push((value.at, CallableSource::of(target), constants));
                waiting.push(target);
            });
            let body = AnonymousCheck {
                source: CallableSource::of(callee),
                calls,
            };
            if !bodies.contains(&body) {
                bodies.push(body);
            }
        }
        Self {
            root: CallableSource::of(callee),
            bodies,
        }
    }
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
    /// Its const inputs are left out: what the body reads of its constants
    /// is told of the instances [`check_instances`] follows.
    pub(crate) fn of(program: &crate::Program, expression: &Expr) -> Option<Self> {
        let ExprKind::Call {
            callee, arguments, ..
        } = &expression.kind
        else {
            return None;
        };
        Self::from_callee(program, callee, Some(arguments), false)
    }

    /// A static reference can reach a forwarding body even without its own
    /// pack arguments, through the ordinary calls the referenced body makes.
    /// A call gives the instance its const inputs too (FN-1, GEN-7).
    fn for_check(program: &crate::Program, expression: &Expr) -> Option<Self> {
        let (module, declaration) = called_or_referenced(expression)?.declaration()?;
        let instance = match &expression.kind {
            ExprKind::StaticCallable(callee) => Self::from_callee(program, callee, None, false),
            ExprKind::Call {
                callee, arguments, ..
            } => Self::from_callee(program, callee, Some(arguments), true),
            _ => None,
        }?;
        if let crate::Detail::Callable(signature) = &program.modules[module].details[declaration]
            && signature.generics.is_empty()
            && signature
                .parameters
                .iter()
                .all(|parameter| parameter.pack.is_none())
            && instance.constants.values.is_empty()
            && instance.functions.is_empty()
            && instance.test_candidates.is_none()
        {
            // Concrete bodies supply their own forwarding roots. A call
            // of one does not make those same instances a second time,
            // unless it binds a callback, const inputs or a test candidate scope.
            return None;
        }
        Some(instance)
    }

    fn from_callee(
        program: &crate::Program,
        callee: &Callee,
        arguments: Option<&[crate::hir::Argument]>,
        with_constants: bool,
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
                Argument::NoResult => true,
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
            constants: if with_constants {
                program.known_const_inputs(callee, arguments)
            } else {
                crate::const_inputs::ReadInputs::default()
            },
            test_candidates: callee.test_candidates(),
        })
    }

    /// The checked body with this caller's pack keys bound to arguments.
    pub(crate) fn body(
        &self,
        program: &crate::Program,
    ) -> Option<crate::instance::CheckedInstance> {
        program.checked_instance(
            self.declaration,
            &self.generics,
            &self.requirements,
            self.inputs(),
            &std::cell::RefCell::new(Diagnostics::new()),
            &mut Diagnostics::new(),
        )
    }

    fn inputs(&self) -> crate::InstanceInputs<'_> {
        crate::InstanceInputs {
            functions: &self.functions,
            type_packs: &self.type_packs,
            pack_inputs: Some(&self.pack_inputs),
            constants: &self.constants.values,
            test_candidates: self.test_candidates,
        }
    }

    /// GEN-13: a const input that outgrew its text budget makes no instance.
    fn outgrew_its_inputs(&self) -> bool {
        self.constants.outgrown
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
    /// The calls from this instance out to the root, which the calls it
    /// makes share: a path grows by one call at each instance (GEN-7).
    path: std::rc::Rc<super::requirements::MakingPath>,
    parent: Option<usize>,
    shape: PackShape,
}

/// Check pack spreads and reads under the concrete shapes that reach them
/// (FN-19). A pack error belongs to the call that supplies its shape.
/// Record the concrete calls it reaches for the ordinary GEN-7 checks.
pub(super) fn check_instances(
    program: &crate::Program,
    discovered: &mut Vec<super::requirements::Made>,
    source: &IndependentSourceBodies,
    store_keys: &mut crate::store_keys::KeyUses,
    diagnostics: &mut hgl_diagnostics::Diagnostics,
) {
    use super::requirements::{
        MakingCall, MakingPath, bounded_instance_key, emit_instance_report, past_pack_growth_limit,
        past_the_limit,
    };
    use hgl_diagnostics::Code;
    use std::collections::{HashMap, HashSet, VecDeque};

    let mut forwarding = program.callback_phase_bodies.clone();
    let mut callers = HashMap::<_, Vec<_>>::new();
    // GEN-5 and GEN-7: a call whose candidate each instance selects may
    // select one that forwards; each candidate of its operator is called.
    let mut candidates = HashMap::<(usize, usize), Vec<(usize, usize)>>::new();
    for (module, unit) in program.modules.iter().enumerate() {
        for (declaration, declared) in unit.declarations.iter().enumerate() {
            if let Some((_, Some(operator))) = declared.kind.candidate() {
                candidates
                    .entry((operator.module, operator.declaration))
                    .or_default()
                    .push((module, declaration));
            }
        }
    }
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
            for selected in unit.selecting.get(declaration).into_iter().flatten() {
                let operator = (selected.operator.module, selected.operator.declaration);
                for &candidate in candidates.get(&operator).into_iter().flatten() {
                    callers.entry(candidate).or_default().push(owner);
                }
            }
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
            let mut checking = ForwardingCheck::of(program, &instance);
            let anonymous = instance
                .functions
                .iter()
                .any(|(_, callee)| matches!(callee, Callee::Anonymous(_)));
            if !anonymous && seen.contains(&checking) {
                continue;
            }
            let shape = PackShape::of(&instance);
            let ancestors = std::iter::successors(parent, |&index| parents[index].parent)
                .map(|index| parents[index].shape);
            if shape.exceeds_growth(ancestors) {
                past_pack_growth_limit(at, instance.declaration, program, PACK_GROWTH, diagnostics);
                break;
            }
            if bounded_instance_key(
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
            if anonymous {
                // A callback's computed value can feed a compile-time check
                // in this body. Recheck it before reusing the source graph,
                // and retain those selected calls, values and diagnostics.
                checking.body_calls = checked
                    .as_ref()
                    .map(|checked| checked_calls(program, &checked.body));
                checking.body_reports = graph_reports.as_slice().to_vec();
                checking
                    .body_reports
                    .extend_from_slice(reports.borrow().as_slice());
                if let Some(uses) = checked
                    .as_ref()
                    .and_then(|checked| checked.result_uses.as_ref())
                {
                    checking
                        .body_reports
                        .extend_from_slice(uses.reports.as_slice());
                }
                if seen.contains(&checking) {
                    continue;
                }
            }
            let count = counts.entry(instance.declaration).or_insert(0);
            *count += 1;
            if *count > crate::INSTANCES {
                past_the_limit(&[at], instance.declaration, program, diagnostics);
                break;
            }
            let here = MakingPath::new(
                MakingCall::new(
                    program,
                    instance.declaration,
                    &instance.generics,
                    &instance.type_packs,
                    written,
                ),
                parent.map(|index| parents[index].path.clone()),
            );
            let path = || here.calls().cloned().collect::<Vec<_>>();
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
                emit_instance_report(report.clone(), &[path()], diagnostics);
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
                        &[path()],
                        diagnostics,
                    );
                    if result_uses.reports.has_errors() {
                        seen.push(checking);
                        continue;
                    }
                }
                if !graph_reports.has_errors() {
                    store_keys.in_instance(program, &checked.body, written);
                }
                let mut link = None;
                let mut outgrown = false;
                checked.walk_calls(&mut |value| {
                    let Some(child) = ForwardingInstance::for_check(program, value) else {
                        return;
                    };
                    // GEN-13: a const input that grows past its budget
                    // ends the walk, as one past the instance bound does.
                    if child.outgrew_its_inputs() {
                        outgrown = true;
                        return;
                    }
                    // The final body can make new calls wherever the
                    // entries affect a type, including in a callback.
                    if !independent
                        .get(&value.at)
                        .is_some_and(|calls| calls.contains(&child))
                        && let Some(callee) = called_or_referenced(value)
                    {
                        let via = MakingPath::new(
                            MakingCall::new(
                                program,
                                child.declaration,
                                &child.generics,
                                &child.type_packs,
                                value.at,
                            ),
                            Some(here.clone()),
                        );
                        super::requirements::made_forwarded_callee(
                            callee,
                            child.constants.clone(),
                            at,
                            via,
                            discovered,
                        );
                    }
                    if forwarding.contains(&child.declaration) {
                        let caller = *link.get_or_insert_with(|| {
                            let index = parents.len();
                            parents.push(ForwardingParent {
                                path: here.clone(),
                                parent,
                                shape,
                            });
                            index
                        });
                        waiting.push_back((child, value.at, Some(caller)));
                    }
                });
                if outgrown {
                    past_the_limit(&[at], instance.declaration, program, diagnostics);
                    break;
                }
            }
            seen.push(checking);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ForwardingInstance, PACK_GROWTH, PackInput, PackKey, PackShape, Scalar, Type};
    use crate::hir::{ReflectedInputs, TypePackArgument};
    use crate::{Argument, Constant, ConstantValue};

    /// FN-14/17: three finite copies of one typed callback introduce
    /// only a source edge and its selected leaf. A different leaf still
    /// makes a different validation context. Runtime HIR stays distinct.
    #[test]
    fn callback_copies_share_checks_without_losing_selected_sources() {
        use crate::hir::{
            AnonymousFunction, Body, Callee, Expr, ExprKind, Form, Stmt, ValueCategory,
        };
        use crate::types::FunctionType;

        let mut sources = hgl_source::SourceMap::new();
        let file = sources.add("callback.hgl", "fn() => f()").unwrap();
        let at = hgl_diagnostics::Location::new(file, hgl_source::Span::new(0, 11));
        let named = |declaration| Callee::Function {
            module: 0,
            declaration,
            generics: Vec::new(),
            type_packs: Vec::new(),
            requirements: Vec::new(),
            defer_pack_members: false,
            contextual: Vec::new(),
            test_candidates: None,
        };
        let callback = |callee| {
            Callee::Anonymous(Box::new(AnonymousFunction {
                at,
                template: FunctionType {
                    value: true,
                    parameters: Vec::new(),
                    named_pack: None,
                    result: Some(Type::Scalar(Scalar::I64)),
                    result_admits_none: false,
                    reference: false,
                },
                body: Body {
                    locals: Vec::new(),
                    statements: vec![Stmt::Expr(Expr {
                        kind: ExprKind::Call {
                            callee,
                            arguments: Vec::new(),
                            form: Form::Named,
                            text: None,
                            braced: Vec::new(),
                            lifted: false,
                        },
                        ty: Type::Scalar(Scalar::I64),
                        at,
                        category: ValueCategory::Value,
                    })],
                },
                captures: Vec::new(),
                pack_captures: Vec::new(),
                specialization: Default::default(),
            }))
        };
        let one = callback(named(0));
        let two = callback(one.clone());
        let three = callback(two.clone());
        let other = callback(callback(named(1)));
        let program = crate::Program::default();
        assert!(two != three);
        assert!(
            super::CallableCheck::of(&program, &two) == super::CallableCheck::of(&program, &three)
        );
        assert!(
            super::CallableCheck::of(&program, &two) != super::CallableCheck::of(&program, &other)
        );
    }

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
            constants: Default::default(),
            test_candidates: None,
        };
        assert_eq!(PackShape::of(&instance).entries, 3);
        instance.type_packs.clear();
        assert_eq!(PackShape::of(&instance).entries, 1);
        instance.generics.clear();
        assert_eq!(PackShape::of(&instance).entries, 1);
    }
}
