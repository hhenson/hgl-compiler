//! A call written with a name: of a value function, of one of the
//! language's forms, or of an enum, which constructs a member, each read
//! from what the name is bound to (FN-4, EXPR-2, TYPE-7). Part of the
//! typing pass ([`super`]); a call of an operator is read in
//! [`super::operators`].

use std::cell::OnceCell;
use std::collections::HashMap;

use hgl_diagnostics::{Category, Location};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};
use hgl_source::{FileId, Span};

use super::anonymous::Anonymous;
use super::arguments::{Slot, Takes};
use super::children::ChildOrigin;
use super::constants::generic_default;
use super::dependent_packs::{PackTypes, dependent_slot};
use super::evals::{trace_input_type, trace_pack_type};
use super::expressions::{ExpressionFit, has_ordered_block};
use super::fit::{not_null, open, whole};
use super::forwarded_packs::spread_operands;
use super::inference::{
    ExplicitGenerics, Inferred, Places, bare_parameter, infer, infer_from_place, mentions,
    pack_bounds, placed, type_pack,
};
use super::lifting::{fill_type, join_categories, preferred_call_role};
use super::node::RefRead;
use super::sizes;
use super::{CapturedOutput, Fact, Typer};
use crate::declared::{bind_result, names_parameter};
use crate::hir::{
    self, Callee, EndpointOperation, Expr, ExprKind, Form, Hook, Local, LocalId, ValueCategory,
};
use crate::program::{Binding, DeclarationKind, LocalKind, OperatorRef, Program, Role};
use crate::syntax::{child, has_token, name_token, tokens, ungrouped};
use crate::types::{
    Argument, CallArguments, ConstantValue, Detail, FunctionType, GenericKind, NamedKind,
    Parameter, Scalar, Signature, Type,
};
use crate::{codes, error};

/// Maximum cached scope variants per pair call site.
const PAIR_SCOPE_CACHE_LIMIT: usize = 16;

/// Per call site, each lexical scope and expected type has its own result.
type PairResults<T> = HashMap<Location, Vec<(PairScope, Option<Type>, T)>>;

/// One pair call, including its tentative and committed readings.
/// Descendant probes can reuse categories in the same lexical scope.
pub(super) struct PairPhases {
    scope: Location,
    call: SyntaxNode,
    categories: HashMap<Location, Vec<(PairScope, ValueCategory, bool)>>,
    probe_types: PairResults<Type>,
    /// The value signature's inferred result when checking a callback-dependent
    /// pair. A discarded reading has no flow or bindings to replay.
    independent_value_results: PairResults<Option<Type>>,
    local_reads: OnceCell<HashMap<Location, Vec<u32>>>,
}

impl PairPhases {
    fn at(typer: &Typer<'_>, list: &SyntaxNode) -> Option<Self> {
        let call = list.parent()?;
        #[cfg(test)]
        tests::PHASE_ROOTS.with(|count| count.set(count.get() + 1));
        Some(Self {
            scope: typer.at(&call),
            call,
            categories: HashMap::new(),
            probe_types: HashMap::new(),
            independent_value_results: HashMap::new(),
            local_reads: OnceCell::new(),
        })
    }

    fn contains(&self, at: Location) -> bool {
        at.file == self.scope.file
            && self.scope.span.start <= at.span.start
            && at.span.end <= self.scope.span.end
    }

    /// Find written uses of locals declared in this call (parameters,
    /// `let`, and `var` bindings whose category or assignment state may
    /// differ between a role probe and a committed read).
    fn index_local_reads(&self, typer: &Typer<'_>) -> HashMap<Location, Vec<u32>> {
        let mut reads = HashMap::<Location, Vec<u32>>::new();
        for token in self
            .call
            .descendants_with_tokens()
            .filter_map(|part| part.into_token())
        {
            let start = token.span().start;
            if let Some(Binding::Local { at, .. }) = typer.bindings.get(&(typer.file, start))
                && self.contains(*at)
            {
                reads.entry(*at).or_default().push(start);
            }
        }
        reads
    }

    /// True when `local` is declared inside this pair root and has no
    /// resolved reference anywhere inside `call`.
    fn local_unread_in(&self, typer: &Typer<'_>, local: &Local, call: Location) -> bool {
        self.contains(local.at)
            && self
                .local_reads
                .get_or_init(|| self.index_local_reads(typer))
                .get(&local.at)
                .is_none_or(|reads| {
                    let first = reads.partition_point(|&start| start < call.span.start);
                    reads.get(first).is_none_or(|&start| start >= call.span.end)
                })
    }
}

/// The state a phase read starts with. A repeated read can be in a
/// different block, branch, or candidate trial, with different local
/// types, assignments or proof of presence.
struct PairScope {
    locals: Vec<Local>,
    assigned: Vec<bool>,
    facts: Vec<Fact>,
    aliases: HashMap<LocalId, LocalId>,
    children: HashMap<LocalId, ChildOrigin>,
    captures: HashMap<LocalId, CapturedOutput>,
    anonymous: Option<Anonymous>,
    hook: Option<Hook>,
    callable: Option<SyntaxNode>,
    naming: Option<Location>,
    binding_child: Option<Location>,
    designating: Option<Location>,
    dynamic: Option<LocalId>,
    range_bindings: Vec<LocalId>,
    generic_bindings: Vec<(Location, Argument)>,
    instance: Vec<(Location, Argument)>,
    evaluation: PairEvaluation,
    returned: bool,
    projected_locals: bool,
}

/// Reading a callback, a pack pass, or an eval can change the context of
/// nested expressions without changing their written location.
#[derive(PartialEq, Eq)]
struct PairEvaluation {
    graph: bool,
    testing: bool,
    value_code: bool,
    evaluating: bool,
    integration: bool,
    generating: bool,
    valued: u32,
    requiring: u32,
}

impl PairEvaluation {
    fn of(typer: &Typer<'_>) -> Self {
        Self {
            graph: typer.graph,
            testing: typer.testing,
            value_code: typer.value_code,
            evaluating: typer.evaluating,
            integration: typer.integration,
            generating: typer.generating,
            valued: typer.valued,
            requiring: typer.requiring,
        }
    }

    fn matches(&self, other: &Self, ignore_value_mode: bool) -> bool {
        if ignore_value_mode {
            self.testing == other.testing
                && self.evaluating == other.evaluating
                && self.integration == other.integration
                && self.generating == other.generating
                && self.valued == other.valued
                && self.requiring == other.requiring
        } else {
            self == other
        }
    }
}

impl PairScope {
    fn of(typer: &Typer<'_>) -> Self {
        Self {
            locals: typer.locals.clone(),
            assigned: typer.assigned.clone(),
            facts: typer.present.clone(),
            aliases: typer.aliases.clone(),
            children: typer.bound_children.clone(),
            captures: typer.captures.clone(),
            anonymous: typer.anonymous.clone(),
            hook: typer.hook,
            callable: typer.callable.clone(),
            naming: typer.naming,
            binding_child: typer.binding_child,
            designating: typer.designating,
            dynamic: typer.dynamic,
            range_bindings: typer.unrolled_range_bindings.clone(),
            generic_bindings: typer.bound.clone(),
            instance: typer.instance.clone(),
            evaluation: PairEvaluation::of(typer),
            returned: typer.returned,
            projected_locals: Self::can_project_locals(typer),
        }
    }

    // Forwarded entries, hooks, and endpoint aliases can introduce reads
    // that the resolver did not record in the call's written subtree.
    fn can_project_locals(typer: &Typer<'_>) -> bool {
        typer.pack_inputs.is_none()
            && typer.pack_passes.is_empty()
            && typer.pack_functions.is_empty()
            && typer.aliases.is_empty()
            && typer.bound_children.is_empty()
            && typer.captures.is_empty()
            && typer.hook.is_none()
    }

    fn matches(
        &self,
        typer: &Typer<'_>,
        phases: &PairPhases,
        call: Location,
        call_node: &SyntaxNode,
        cached: ValueCategory,
        ignore_value_mode: bool,
    ) -> bool {
        let project_locals = self.projected_locals && Self::can_project_locals(typer);
        let ignored_fact = |fact: &Fact| {
            project_locals
                && fact_local(fact)
                    .and_then(|local| typer.locals.get(local))
                    .is_some_and(|local| phases.local_unread_in(typer, local, call))
        };
        let dynamic_matches = self.dynamic == typer.dynamic
            || (project_locals
                && [self.dynamic, typer.dynamic]
                    .into_iter()
                    .flatten()
                    .all(|outside| {
                        typer.locals.iter().take(outside).all(|local| {
                            local.kind != LocalKind::Var
                                || phases.local_unread_in(typer, local, call)
                        })
                    }));
        let anonymous_matches = match (&self.anonymous, &typer.anonymous) {
            (None, None) => true,
            (Some(probed), Some(current)) => {
                probed == current
                    || (probed.first_local == current.first_local && !has_direct_return(call_node))
            }
            _ => false,
        };
        self.locals.len() == typer.locals.len()
            && self
                .locals
                .iter()
                .zip(&typer.locals)
                .all(|(probed, current)| {
                    probed == current
                        || (project_locals
                            && probed.at == current.at
                            && probed.kind == current.kind
                            && probed.name == current.name
                            && (phases.local_unread_in(typer, current, call)
                                || (cached == ValueCategory::Connection
                                    && phases.contains(current.at)
                                    && probed.ty == current.ty
                                    && probed.category == ValueCategory::Value
                                    && current.category == ValueCategory::Connection)))
                })
            && self.assigned.iter().zip(&typer.assigned).enumerate().all(
                |(index, (probed, current))| {
                    probed == current
                        || (project_locals
                            && phases.local_unread_in(typer, &typer.locals[index], call))
                },
            )
            && self
                .facts
                .iter()
                .filter(|fact| !ignored_fact(fact))
                .eq(typer.present.iter().filter(|fact| !ignored_fact(fact)))
            && self.aliases == typer.aliases
            && self.children == typer.bound_children
            && self.captures == typer.captures
            && anonymous_matches
            && self.hook == typer.hook
            && self.callable == typer.callable
            && self.naming == typer.naming
            && self.binding_child == typer.binding_child
            && self.designating == typer.designating
            && dynamic_matches
            && self.range_bindings == typer.unrolled_range_bindings
            && self.generic_bindings == typer.bound
            && self.instance == typer.instance
            && self
                .evaluation
                .matches(&PairEvaluation::of(typer), ignore_value_mode)
            && self.returned == typer.returned
    }
}

/// The local a presence/admission fact refers to, when it names one.
fn fact_local(fact: &Fact) -> Option<LocalId> {
    match *fact {
        Fact::Present(local) | Fact::Valid(local) | Fact::Success(local) | Fact::Failure(local) => {
            Some(local)
        }
        Fact::AllValid(_)
        | Fact::ChildValid(_)
        | Fact::DeferredChildGuard { .. }
        | Fact::DeferredPackGuard { .. } => None,
    }
}

/// True when `node` contains a `return` statement targeting an enclosing
/// function (not nested inside an inner anonymous function).
fn has_direct_return(node: &SyntaxNode) -> bool {
    let mut stack: Vec<SyntaxNode> = node.children().collect();
    while let Some(child) = stack.pop() {
        match child.kind() {
            SyntaxKind::ReturnStmt => return true,
            SyntaxKind::FnExpr => continue,
            _ => stack.extend(child.children()),
        }
    }
    false
}

impl<'a> Typer<'a> {
    // ---- Calls and constructions ----

    /// What the name that starts at `start` of `file` resolved to
    /// (UNIT-10), in whichever module the file is of: a candidate's
    /// default is written in its own module's file, which need not be
    /// the one being read.
    pub(super) fn binding_at(&self, file: FileId, start: u32) -> Option<&'a Binding> {
        if let Some(found) = self.bindings.get(&(file, start)) {
            return Some(found);
        }
        self.program
            .modules
            .iter()
            .flat_map(|module| module.uses.iter())
            .find(|used| used.at.file == file && used.at.span.start == start)
            .map(|used| &used.binding)
    }

    /// The declarations a name is bound to, when it names declarations
    /// of a module and not a local.
    pub(super) fn declarations(&self, name: &SyntaxToken) -> Option<(usize, &'a [usize])> {
        match self.bindings.get(&(self.file, name.span().start))? {
            Binding::Declarations {
                module,
                declarations,
            } => Some((*module, declarations.as_slice())),
            Binding::Local { .. } => None,
        }
    }

    /// `callee(arguments)`: a call of a value function, or the
    /// construction of a struct.
    ///
    /// FN-4: "runtime/value evaluation chooses the value definition."
    /// A name left for the prelude, an operator, and a callee that is
    /// itself an expression are for later parts: the callee and the
    /// arguments are read, and the call has no type.
    pub(super) fn call(
        &mut self,
        node: &SyntaxNode,
        expected: Option<&Type>,
        used: bool,
        fit: ExpressionFit,
    ) -> (ExprKind, Type) {
        self.call_read(node, expected, used, fit)
    }

    /// A direct call uses its selected signature. A callback parameter
    /// uses the function type in its local binding.
    pub(super) fn call_has_no_result(&self, callee: &Callee) -> bool {
        match callee {
            Callee::Parameter { parameter, .. } => self.locals.get(*parameter).is_some_and(
                |local| matches!(&local.ty, Type::Function(template) if template.result.is_none()),
            ),
            callee => callee.has_result(self.program) == Some(false),
        }
    }

    /// A call, as the form or the function its callee names.
    fn call_read(
        &mut self,
        node: &SyntaxNode,
        expected: Option<&Type>,
        used: bool,
        fit: ExpressionFit,
    ) -> (ExprKind, Type) {
        let (Some(callee), Some(list)) = (call_callee(node), child(node, SyntaxKind::ArgList))
        else {
            return (ExprKind::Unread, Type::Unknown);
        };
        let direct = ungrouped(callee.clone());
        if direct.kind() == SyntaxKind::FnExpr
            && let Some(keyword) = tokens(&direct).find(|token| token.kind() == SyntaxKind::KwFn)
        {
            return self.call_anonymous(&direct, &keyword, &list, expected);
        }
        if direct.kind() == SyntaxKind::NameExpr
            && let Some(index) = self.local_of(&direct)
            && let Type::Function(template) = self.locals[index].ty.clone()
            && let Some(name) = name_token(&direct)
        {
            return self.call_function_parameter(&name, &list, index, &template);
        }
        // GEN-10: a runtime schema view has no ordinary value type.
        // Resolve the language form only where no declaration or local
        // binds that name (UNIT-10).
        if direct.kind() == SyntaxKind::NameExpr
            && child(&direct, SyntaxKind::QualifiedName).is_none()
            && let Some(name) = name_token(&direct)
            && name.text() == "schemas"
            && !self.bindings.contains_key(&(self.file, name.span().start))
        {
            if used {
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::TYPE_MISMATCH,
                    self.at(node),
                    "a borrowed schema view cannot be used as an ordinary value".to_string(),
                    "GEN-10",
                ));
            }
            // Its pack operand names schema slots, so it is not a read
            // of the pack as an ordinary value (FN-19).
            let arguments = self
                .call_entries(&list)
                .into_iter()
                .filter_map(|(_, _, argument)| {
                    let operand = ungrouped(argument.clone());
                    if self
                        .local_of(&operand)
                        .is_some_and(|local| self.parameter_pack(local).is_some())
                    {
                        return None;
                    }
                    Some(hir::Argument {
                        parameter: None,
                        pack: None,
                        value: self.expression(&argument, Some(&Type::Unknown)),
                    })
                })
                .collect();
            return (unread_call_kind(arguments), Type::Unknown);
        }
        // ERR-13: "as the callee of a call each is the form this
        // chapter defines, whatever else is in scope."
        if callee.kind() == SyntaxKind::NameExpr
            && child(&callee, SyntaxKind::QualifiedName).is_none()
            && let Some(name) = name_token(&callee)
            && let Some(form) = self.result_form(&name, &list, expected, fit)
        {
            return form;
        }
        let found = match callee.kind() {
            SyntaxKind::NameExpr => name_token(&callee)
                .and_then(|name| self.declarations(&name).map(|found| (name, found))),
            _ => None,
        };
        let Some((name, (module, declarations))) = found else {
            // TIME-9: "The operations on an endpoint are intrinsics of
            // the language". Nothing declares one, so its name is bound
            // to nothing; it is read in a node's hooks.
            if self.hook.is_some()
                && callee.kind() == SyntaxKind::NameExpr
                && let Some(name) = name_token(&callee)
                && !self.bindings.contains_key(&(self.file, name.span().start))
                && let Some(operation) = EndpointOperation::language_form(name.text())
            {
                return self.endpoint(operation, &name, &list);
            }
            // TIME-9: "`scheduled()` is the handler selector for a wake by
            // the node's scheduler or alarm". Core declares no function
            // of the name: where no hook stands it is judged as the
            // selector it is.
            if self.hook.is_none()
                && callee.kind() == SyntaxKind::NameExpr
                && let Some(name) = name_token(&callee)
                && !self.bindings.contains_key(&(self.file, name.span().start))
                && EndpointOperation::language_form(name.text())
                    == Some(EndpointOperation::Scheduled)
            {
                self.scheduled_outside_hooks(self.at(node));
                return self.unread_call(&list);
            }
            // The callee is an expression like any other, and is read
            // where it is written, for what it reads (FN-14: "An
            // anonymous function's scope is the scope around it plus its
            // parameters"): the call of what one gives,
            // `(fn(a) => fn(b) => a + b)(1)(2)`, and one
            // chosen by an `if` or taken from a list, `[fn() => x][0]()`.
            let called = self.expression(&callee, Some(&Type::Unknown));
            // FN-7: "inference does not create a general callable value
            // type". An anonymous function has no type here, and a value
            // of a type is no function.
            if !open(&called.ty) {
                let mut report = error(
                    Category::Type,
                    codes::TYPE_MISMATCH,
                    self.at(&callee),
                    format!(
                        "this is `{}`, which is no function, and is called",
                        called.ty
                    ),
                    "FN-7",
                )
                .with_note(
                    "what is called is a function: one a module declares, or an anonymous one",
                );
                if direct.kind() == SyntaxKind::NameExpr
                    && let Some(name) = name_token(&direct)
                    && let Some(Binding::Local { at, .. }) =
                        self.binding_at(self.file, name.span().start)
                    && let Some(&hidden) = self
                        .program
                        .modules
                        .iter()
                        .find_map(|module| module.hidden_callables.get(at))
                {
                    report = report
                        .with_related(*at, format!("`{}` is bound here", name.text()))
                        .with_related(hidden, "this function or operator is hidden by the binding");
                }
                self.diagnostics.emit(report);
            }
            return self.unread_call(&list);
        };
        // TIME-9: "The operations on an endpoint are core's native value
        // functions ... `modified` and `valid` are selectors in a `when`
        // header (TIME-3): there they are forms of the language, which
        // set subscription and admission." In a node's hooks each name
        // is read as the form it is, a selector or a guard among them,
        // whether core has declared the function or not: the call is of
        // core's own, and what it gives is the same.
        // So is one named through an alias of core, `c::valid(a)`: it is
        // core's `valid` however it is named.
        if self.hook.is_some()
            && Some(module) == self.program.core()
            && let Some(operation) = EndpointOperation::language_form(name.text())
        {
            return self.endpoint(operation, &name, &list);
        }
        self.call_named(&name, &list, (module, declarations), expected, None)
    }

    /// A call of what `name` is bound to: `declarations` of a module.
    /// `generics` is the generic arguments the call writes after the
    /// name, when it writes them (GEN-8).
    pub(super) fn call_named(
        &mut self,
        name: &SyntaxToken,
        list: &SyntaxNode,
        (module, declarations): (usize, &[usize]),
        expected: Option<&Type>,
        generics: Option<&CallArguments>,
    ) -> (ExprKind, Type) {
        let program = self.program;
        let kind_of = |declaration: usize| program.modules[module].declarations[declaration].kind;
        let (value, temporal) = function_pair(program, module, declarations);
        // FN-4: a composition call takes the direct match before a
        // lifted one. Only the selected definition checks the signature.
        let function = match (self.graph, value, temporal) {
            (true, Some(value), Some(temporal)) => {
                let root = self.pair_phases.is_none();
                if root {
                    self.pair_phases = PairPhases::at(self, list);
                }
                let (operands, all_fn_args_probed) =
                    self.pair_operands(name, list, module, (value, temporal));
                let open_callback_role = operands == ValueCategory::Unsettled
                    && self
                        .signature
                        .parameters
                        .iter()
                        .any(|parameter| matches!(&parameter.ty, Type::Function(f) if f.value));
                self.unsettled_callback_pair |= open_callback_role;
                let selected = match preferred_call_role(true, [operands]) {
                    Role::Value => value,
                    Role::Temporal => temporal,
                };
                let call = if open_callback_role && self.unsettled_results.is_some() {
                    self.independent_pair(name, list, (module, value, temporal), expected, generics)
                } else if self.probing_pair_role > 0
                    && !self.evaluating
                    && (selected == temporal || all_fn_args_probed)
                {
                    self.probe_selected_pair(
                        name,
                        list,
                        (module, selected),
                        selected == temporal,
                        expected,
                        generics,
                    )
                } else {
                    self.call_function(name, list, (module, selected), expected, generics)
                };
                if root {
                    self.pair_phases = None;
                }
                return call;
            }
            (false, Some(value), Some(_))
                if self.probing_pair_role > 0
                    && !self.evaluating
                    && self.value_pair_probe_shape(list, (module, value)) == Some(false) =>
            {
                if let Some(ty) = self.cached_probe_type(list, expected) {
                    return self.pair_probe_call((module, value), Some(ty));
                }
                Some(value)
            }
            (true, None, temporal) => temporal,
            (_, value, _) => value,
        };
        if let Some(declaration) = function {
            return self.call_function(name, list, (module, declaration), expected, generics);
        }
        let structure = declarations
            .iter()
            .copied()
            .find(|&declaration| kind_of(declaration) == DeclarationKind::Struct);
        if let Some(declaration) = structure {
            // GEN-2: a generic struct "infers them from constructor
            // fields and expected type".
            let given = match not_null(expected) {
                Some(Type::Named {
                    module: expected_module,
                    declaration: expected_declaration,
                    arguments,
                    ..
                }) if (*expected_module, *expected_declaration) == (module, declaration) => {
                    Some(arguments.clone())
                }
                _ => None,
            };
            let settled = !expected.is_some_and(open);
            return self.construct(name, list, module, declaration, given, settled);
        }
        // TYPE-7: "A call of the enum's name constructs a member."
        let enumeration = declarations
            .iter()
            .copied()
            .find(|&declaration| kind_of(declaration) == DeclarationKind::Enum);
        if let Some(declaration) = enumeration {
            return self.enum_from(name, list, module, declaration);
        }
        let operator = declarations
            .iter()
            .copied()
            .find(|&declaration| kind_of(declaration) == DeclarationKind::Operator);
        if let Some(declaration) = operator {
            let operator = OperatorRef {
                module,
                declaration,
            };
            return self.call_operator(name, list, operator, expected, generics);
        }
        // NATIVE-6: "a native type is not constructed in HGL: a call of
        // its name rejects."
        let native = declarations
            .iter()
            .any(|&declaration| kind_of(declaration) == DeclarationKind::NativeType);
        if native {
            self.diagnostics.emit(
                error(
                    Category::Type,
                    codes::CALL_NOT_CALLABLE,
                    Location::new(self.file, name.span()),
                    format!(
                        "`{}` is a native type, and no call constructs one",
                        name.text()
                    ),
                    "NATIVE-6",
                )
                .with_note("a value of a native type comes from a native function"),
            );
        }
        if temporal.is_some() {
            // FN-3: "A runtime body calls only value functions, and so
            // does a value function." FN-4: "A bad selected signature
            // is an error, not fallback to the other role." TST-3, and
            // the owner on hhenson/hgl_spec#162: "inside of a test,
            // outside of eval / run only const fns can be called", as in
            // an application's statements (UNIT-14); the code is
            // proposed as `call.role`.
            let (place, rule) = if self.testing {
                ("a test, outside an `eval` or a `run`,", "TST-3")
            } else if self.value_code {
                ("an application's statement", "UNIT-14")
            } else if self.hook.is_some() {
                ("a hook", "FN-3")
            } else if self.generating {
                ("a source", "FN-3")
            } else {
                ("a value function", "FN-3")
            };
            self.diagnostics.emit(error(
                Category::FunctionKind,
                codes::CALL_ROLE,
                Location::new(self.file, name.span()),
                format!(
                    "`{}` is a temporal function, and {place} calls value functions",
                    name.text()
                ),
                rule,
            ));
        }
        self.unread_call(list)
    }

    /// `Side(1)` or `Side("buy")` (TYPE-7): "A call of the enum's name
    /// constructs a member. It takes one argument, by position, that is
    /// an `i64` or a `str`, and any other type rejects: `Side(1)` is the
    /// member whose number is 1, and `Side("buy")` the member of that
    /// exact name. Construction is checked: a constant that is no
    /// member's number or name rejects, and a value that is none fails
    /// where it is evaluated."
    fn enum_from(
        &mut self,
        name: &SyntaxToken,
        list: &SyntaxNode,
        module: usize,
        declaration: usize,
    ) -> (ExprKind, Type) {
        let ty = Type::Named {
            name: name.text().to_string(),
            module,
            declaration,
            arguments: Vec::new(),
            kind: NamedKind::Enum,
        };
        if let Some(spread) = super::forwarded_packs::spread_operands(list).next() {
            self.diagnostics.emit(error(
                Category::Type,
                codes::ARGUMENT_UNEXPECTED,
                self.at(&spread),
                format!(
                    "the constructor of `{}` takes one written value, with no pack spread",
                    name.text()
                ),
                "TYPE-7",
            ));
            return (self.unread_call(list).0, ty);
        }
        let Some(argument) = self.one_argument(name, list, "TYPE-7") else {
            return (ExprKind::Unread, ty);
        };
        let value = self.expression(&argument, None);
        let kind = if self.graph
            && matches!(value.ty, Type::Unknown)
            && !matches!(value.kind, ExprKind::Unread)
        {
            ExprKind::DeferredEnumFrom(Box::new(value))
        } else {
            self.enum_from_value(&ty, value, Some(&argument))
        };
        (kind, ty)
    }

    /// TYPE-7: a deferred constructor already names its enum. Complete
    /// the ordinary constructor check once its operand has a type.
    pub(super) fn settle_enum_from(&mut self, ty: &Type, value: &hir::Expr) -> Option<ExprKind> {
        if !whole(&value.ty) {
            return None;
        }
        let argument = self.syntax_of(value);
        Some(self.enum_from_value(ty, value.clone(), argument.as_ref()))
    }

    fn enum_from_value(
        &mut self,
        ty: &Type,
        value: hir::Expr,
        argument: Option<&SyntaxNode>,
    ) -> ExprKind {
        let Type::Named {
            name,
            module,
            declaration,
            kind: NamedKind::Enum,
            ..
        } = ty
        else {
            return ExprKind::Unread;
        };
        let (module, declaration) = (*module, *declaration);
        if !matches!(value.ty, Type::Scalar(Scalar::I64 | Scalar::Str)) {
            // What is not known has been reported (ERR-9).
            if !open(&value.ty) {
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::TYPE_MISMATCH,
                    value.at,
                    format!(
                        "`{}` is constructed from a member's number, an `i64`, or its name, \
                         a `str`, and this is `{}`",
                        name, value.ty
                    ),
                    "TYPE-7",
                ));
            }
            return ExprKind::Unread;
        }
        match argument.and_then(|argument| self.member_constructed(module, declaration, argument)) {
            // A constant that names a member is that member: `Side(0)`
            // is `Side.buy`, wherever a member is looked for, a `switch`
            // label and a `const` argument among them.
            Some(Ok(member)) => ExprKind::EnumMember {
                module,
                declaration,
                member,
            },
            Some(Err(missing)) => {
                self.diagnostics.emit(
                    error(
                        Category::Name,
                        codes::MEMBER_UNKNOWN,
                        value.at,
                        format!("`{name}` has {missing}"),
                        "TYPE-7",
                    )
                    .with_note("a member's name is matched exactly"),
                );
                ExprKind::Unread
            }
            None => ExprKind::EnumFrom {
                module,
                declaration,
                value: Box::new(value),
            },
        }
    }

    /// The arguments of the call `list` is the list of. For the call a
    /// `capture` names, they are what follows the callable's name:
    /// `capture(required, value)` gives `required` the argument
    /// `value` (ERR-7).
    pub(super) fn call_arguments(
        &self,
        list: &SyntaxNode,
    ) -> Vec<(Option<SyntaxToken>, SyntaxNode)> {
        arguments_of(list)
            .into_iter()
            .filter(|(_, value)| Some(value) != self.callable.as_ref())
            .collect()
    }

    /// Entries of a call, in written order (EXPR-2, EXPR-10). The flag
    /// says whether the name is in the braces.
    pub(super) fn call_entries(
        &self,
        list: &SyntaxNode,
    ) -> Vec<(bool, Option<SyntaxToken>, SyntaxNode)> {
        Self::configuration_entries(list)
            .map(|(name, value)| (true, Some(name), value))
            .chain(
                self.call_arguments(list)
                    .into_iter()
                    .map(|(name, value)| (false, name, value)),
            )
            .collect()
    }

    /// During a tentative role probe, an inner pair call whose role is
    /// already decided avoids redundant nested committed reads (FN-4):
    /// - A temporal selection already proves the enclosing callback calls a
    ///   temporal declaration (`Role::Temporal`).
    /// - A value selection with no spreads, `var` parameters, or block
    ///   arguments returns its concrete result directly, or caches its
    ///   inferred generic result type across probes in the same `PairPhases`.
    fn probe_selected_pair(
        &mut self,
        name: &SyntaxToken,
        list: &SyntaxNode,
        (module, selected): (usize, usize),
        is_temporal: bool,
        expected: Option<&Type>,
        generics: Option<&CallArguments>,
    ) -> (ExprKind, Type) {
        if is_temporal {
            self.probe_saw_temporal = true;
            return self.pair_probe_call((module, selected), None);
        }
        let Some(concrete_result) = self.value_pair_probe_shape(list, (module, selected)) else {
            return self.call_function(name, list, (module, selected), expected, generics);
        };
        if concrete_result {
            return self.pair_probe_call((module, selected), None);
        }
        if let Some(ty) = self.cached_probe_type(list, expected) {
            return self.pair_probe_call((module, selected), Some(ty));
        }
        let at = self.at(list);
        let scope = self
            .pair_phases
            .as_ref()
            .filter(|phases| phases.contains(at))
            .map(|_| PairScope::of(self));
        let reported = self.diagnostics.len();
        let (kind, ty) = self.call_function(name, list, (module, selected), expected, generics);
        if !self.spent
            && self.diagnostics.len() == reported
            && whole(&ty)
            && let Some(scope) = scope
            && let Some(phases) = &mut self.pair_phases
        {
            let entries = phases.probe_types.entry(at).or_default();
            let entry = (scope, expected.cloned(), ty.clone());
            if entries.len() < PAIR_SCOPE_CACHE_LIMIT {
                entries.push(entry);
            } else if let Some(last) = entries.last_mut() {
                *last = entry;
            }
        }
        (kind, ty)
    }

    /// Look up a generic value pair's cached result type from an earlier
    /// graph-mode probe in the same `PairPhases`. Only entries previously
    /// verified as `ValueCategory::Value` with matching referenced locals
    /// match, so connection captures inside value callbacks are preserved.
    fn cached_probe_type(&self, list: &SyntaxNode, expected: Option<&Type>) -> Option<Type> {
        let at = self.at(list);
        let call_node = list.parent();
        let call_at = call_node.as_ref().map_or(at, |call| self.at(call));
        self.pair_phases
            .as_ref()
            .filter(|phases| phases.contains(at))
            .and_then(|phases| {
                phases.probe_types.get(&at)?.iter().find_map(
                    |(scope, cached_expected, cached_ty)| {
                        (cached_expected.as_ref() == expected
                            && scope.matches(
                                self,
                                phases,
                                call_at,
                                call_node.as_ref().unwrap_or(list),
                                ValueCategory::Value,
                                true,
                            ))
                        .then(|| cached_ty.clone())
                    },
                )
            })
    }

    /// Returns `Some(true)` when a value-selected pair can short-circuit
    /// immediately with its concrete declared result, `Some(false)` when its
    /// generic result can be cached by `PairPhases::probe_types`, and `None`
    /// when spreads, `var` parameters, or block arguments prevent skipping.
    fn value_pair_probe_shape(
        &self,
        list: &SyntaxNode,
        (module, declaration): (usize, usize),
    ) -> Option<bool> {
        if spread_operands(list).next().is_some() {
            return None;
        }
        let Some(Detail::Callable(signature)) =
            self.program.modules[module].details.get(declaration)
        else {
            return None;
        };
        if signature.parameters.iter().any(|parameter| parameter.var) {
            return None;
        }
        if self
            .call_entries(list)
            .iter()
            .any(|(_, _, node)| has_ordered_block(node))
        {
            return None;
        }
        Some(
            signature
                .result
                .as_ref()
                .is_none_or(|ty| !names_parameter(ty)),
        )
    }

    /// Lightweight call result returned during a discarded role probe once
    /// an inner pair's role is decided (FN-4).
    fn pair_probe_call(
        &self,
        (module, declaration): (usize, usize),
        override_ty: Option<Type>,
    ) -> (ExprKind, Type) {
        let ty = override_ty.unwrap_or_else(|| {
            match self.program.modules[module].details.get(declaration) {
                Some(Detail::Callable(signature)) => signature
                    .result
                    .as_ref()
                    .filter(|ty| !names_parameter(ty))
                    .cloned()
                    .unwrap_or(Type::Unknown),
                _ => Type::Unknown,
            }
        });
        let callee = Callee::Function {
            module,
            declaration,
            generics: Vec::new(),
            type_packs: Vec::new(),
            requirements: Vec::new(),
            defer_pack_members: false,
            contextual: Vec::new(),
            test_candidates: self.test_candidates,
        };
        (
            ExprKind::Call {
                callee,
                arguments: Vec::new(),
                form: Form::Named,
                text: None,
                braced: Vec::new(),
                lifted: false,
            },
            ty,
        )
    }

    /// Read the category of the supplied arguments before choosing a
    /// role. Omitted arguments are filled only by the selected signature.
    /// Also reports whether every written anonymous callback argument was
    /// probed with a role template (so short-circuiting a value selection
    /// cannot skip captures inside an unprobed `const fn` callback).
    fn pair_operands(
        &mut self,
        name: &SyntaxToken,
        list: &SyntaxNode,
        module: usize,
        (value, temporal): (usize, usize),
    ) -> (ValueCategory, bool) {
        let at = self.at(list);
        let call_node = list.parent();
        let call_at = call_node.as_ref().map_or(at, |call| self.at(call));
        let cache = self
            .pair_phases
            .as_ref()
            .filter(|phases| phases.contains(at));
        if let Some(phases) = cache
            && let Some(entries) = phases.categories.get(&at)
            && let Some((_, category, all_fn_args_probed)) =
                entries.iter().find(|(scope, category, _)| {
                    scope.matches(
                        self,
                        phases,
                        call_at,
                        call_node.as_ref().unwrap_or(list),
                        *category,
                        false,
                    )
                })
        {
            return (*category, *all_fn_args_probed);
        }
        #[cfg(test)]
        tests::PHASE_PROBES.with(|count| count.set(count.get() + 1));
        let scope = cache.map(|_| PairScope::of(self));
        let mark = self.mark();
        let categories: Vec<_> = self.locals.iter().map(|local| local.category).collect();
        let must_return = self.must_return;
        let returning_tails = self.returning_tails.clone();
        let callbacks = self.pair_callback_templates(name, list, module, (value, temporal));
        let mut all_fn_args_probed = true;
        let mut category = if self.pack_inputs.is_none() && spread_operands(list).next().is_some() {
            ValueCategory::Unsettled
        } else {
            self.forwarded_arguments(list)
                .iter()
                .map(|argument| self.category(&argument.value))
                .fold(ValueCategory::Value, join_categories)
        };
        self.probing_pair_role += 1;
        for (_, _, node) in self.call_entries(list) {
            if category == ValueCategory::Connection {
                break;
            }
            if !self.may_reread() {
                category = ValueCategory::Unsettled;
                break;
            }
            let operand = if let Some(&(value, temporal)) = callbacks.get(&self.at(&node)) {
                self.pair_callback_category(&node, value, temporal)
            } else {
                if ungrouped(node.clone()).kind() == SyntaxKind::FnExpr {
                    all_fn_args_probed = false;
                }
                let outer_saw_temporal = std::mem::replace(&mut self.probe_saw_temporal, false);
                let value = self.expression(&node, None);
                self.probe_saw_temporal = outer_saw_temporal;
                self.category(&value)
            };
            category = join_categories(category, operand);
        }
        self.probing_pair_role -= 1;
        self.take_back(mark, false);
        for (local, category) in self.locals.iter_mut().zip(categories) {
            local.category = category;
        }
        self.must_return = must_return;
        self.returning_tails = returning_tails;
        // A probe can be unresolved or report a diagnostic the selected
        // signature must read for itself. Reuse only its category, and
        // never reuse a result cut short by the reading allowance.
        if !self.spent
            && let Some(scope) = scope
            && let Some(phases) = &mut self.pair_phases
        {
            let entries = phases.categories.entry(at).or_default();
            if entries.len() < PAIR_SCOPE_CACHE_LIMIT {
                entries.push((scope, category, all_fn_args_probed));
            } else if let Some(last) = entries.last_mut() {
                *last = (scope, category, all_fn_args_probed);
            }
        }
        (category, all_fn_args_probed)
    }

    /// GEN-6 and FN-17: find the written arguments that take callable
    /// templates of different roles. The selected signature checks the
    /// template's ordinary types (FN-4).
    fn pair_callback_templates(
        &mut self,
        name: &SyntaxToken,
        list: &SyntaxNode,
        module: usize,
        (value, temporal): (usize, usize),
    ) -> HashMap<Location, (&'a FunctionType, &'a FunctionType)> {
        let program = self.program;
        let details = &program.modules[module].details;
        let (Some(Detail::Callable(value)), Some(Detail::Callable(temporal))) =
            (details.get(value), details.get(temporal))
        else {
            return HashMap::new();
        };
        let any_template = |signature: &Signature, value_role| {
            signature.parameters.iter().any(
                |parameter| matches!(&parameter.ty, Type::Function(ty) if ty.value == value_role),
            )
        };
        if !any_template(value, true) || !any_template(temporal, false) {
            return HashMap::new();
        }
        let given = |typer: &mut Self, signature: &'a Signature| {
            let slots: Vec<_> = signature
                .parameters
                .iter()
                .map(|parameter| Slot {
                    name: &parameter.name,
                    configuration: parameter.configuration,
                    ty: Type::Unknown,
                    required: false,
                    pack: parameter.pack,
                    pack_bounds: (0, None),
                    type_pack: None,
                    listed: false,
                })
                .collect();
            let options: Vec<_> = signature
                .generics
                .iter()
                .filter(|generic| matches!(generic.kind, GenericKind::Value(_)))
                .map(|generic| generic.name.as_str())
                .collect();
            let mark = typer.mark();
            let arguments = typer.assign(name, list, &slots, Takes::Parameters, &options);
            typer.take_back(mark, false);
            arguments.into_iter().filter_map(move |argument| {
                let parameter = signature.parameters.get(argument.slot?)?;
                match &parameter.ty {
                    Type::Function(template) if argument.forwarded.is_none() => {
                        Some((argument.value, template.as_ref()))
                    }
                    _ => None,
                }
            })
        };
        let values: HashMap<_, _> = given(self, value)
            .map(|(node, template)| (self.at(&node), template))
            .collect();
        given(self, temporal)
            .filter_map(|(node, temporal)| {
                let at = self.at(&node);
                let value = *values.get(&at)?;
                (value.value && !temporal.value).then_some((at, (value, temporal)))
            })
            .collect()
    }

    /// A value callable that either template admits leaves the role to the
    /// other arguments. A temporal-only callable or a connection capture
    /// selects the temporal definition (FN-4, GEN-6, FN-17).
    fn pair_callback_category(
        &mut self,
        node: &SyntaxNode,
        value: &FunctionType,
        temporal: &FunctionType,
    ) -> ValueCategory {
        let node = ungrouped(node.clone());
        if node.kind() == SyntaxKind::FnExpr {
            return self.anonymous_pair_category(&node, temporal);
        }
        if !matches!(
            node.kind(),
            SyntaxKind::NameExpr | SyntaxKind::FunctionReference
        ) {
            return ValueCategory::Value;
        }
        if let Some(local) = self.local_of(&node) {
            return match &self.locals[local].ty {
                Type::Function(template) if !template.value => ValueCategory::Connection,
                _ => ValueCategory::Value,
            };
        }
        let Some((module, declarations)) =
            name_token(&node).and_then(|token| self.declarations(&token))
        else {
            return ValueCategory::Value;
        };
        match function_pair(self.program, module, declarations) {
            (None, Some(_)) => return ValueCategory::Connection,
            (Some(_), _) => return ValueCategory::Value,
            _ => {}
        }
        if !declarations.iter().any(|&index| {
            self.program.modules[module].declarations[index].kind == DeclarationKind::Operator
        }) {
            return ValueCategory::Value;
        }

        // An operator reference names a set of candidates. The two
        // receiving templates still determine which candidates fit.
        let mark = self.mark();
        let read = self.function_argument(&node, value);
        let fits_value =
            self.take_back(mark, false) && matches!(read.kind, ExprKind::StaticCallable(_));
        if fits_value {
            return ValueCategory::Value;
        }
        if !self.may_reread() {
            return ValueCategory::Unsettled;
        }
        let mark = self.mark();
        let read = self.function_argument(&node, temporal);
        let fits_temporal =
            self.take_back(mark, false) && matches!(read.kind, ExprKind::StaticCallable(_));
        if fits_temporal {
            ValueCategory::Connection
        } else {
            ValueCategory::Value
        }
    }

    /// Whether a name occupies a call's callee or a written argument
    /// position. The argument reader owns both parentheses and braces.
    pub(super) fn is_call_operand(&self, node: &SyntaxNode) -> bool {
        let Some(parent) = crate::syntax::parent_outside_grouping(node) else {
            return false;
        };
        if call_callee(&parent).map(ungrouped).as_ref() == Some(node) {
            return true;
        }
        // The value's parent is its entry. The parentheses or braces
        // containing that entry are a direct child of their call.
        parent
            .parent()
            .and_then(|entries| entries.parent())
            .and_then(|call| child(&call, SyntaxKind::ArgList))
            .is_some_and(|list| {
                self.call_entries(&list)
                    .into_iter()
                    .any(|(_, _, value)| ungrouped(value) == *node)
            })
    }

    /// The names and expressions in a call's configuration (EXPR-10).
    fn configuration_entries(list: &SyntaxNode) -> impl Iterator<Item = (SyntaxToken, SyntaxNode)> {
        list.parent()
            .and_then(|call| child(&call, SyntaxKind::OperatorConfig))
            .into_iter()
            .flat_map(|braces| braces.children())
            .filter(|entry| entry.kind() == SyntaxKind::ConfigArg)
            .filter_map(|entry| Some((tokens(&entry).next()?, entry.children().next()?)))
    }

    /// EXPR-10 retains the rules of intrinsic forms. Where a form has
    /// no named parameter, no brace entry can bind to it.
    pub(super) fn reject_configuration(&mut self, form: &SyntaxToken, list: &SyntaxNode) {
        for (name, value) in Self::configuration_entries(list) {
            self.diagnostics.emit(error(
                Category::Name,
                codes::ARGUMENT_UNKNOWN,
                Location::new(self.file, name.span()),
                format!("`{}` has no parameter `{}`", form.text(), name.text()),
                "EXPR-10",
            ));
            self.expression(&value, Some(&Type::Unknown));
        }
    }

    /// A call this pass does not read: its arguments are read for what
    /// they report themselves, with nothing expected of them.
    pub(super) fn unread_call(&mut self, list: &SyntaxNode) -> (ExprKind, Type) {
        let arguments = self
            .call_entries(list)
            .into_iter()
            .map(|(_, _, value)| hir::Argument {
                parameter: None,
                pack: None,
                value: self.expression(&value, Some(&Type::Unknown)),
            })
            .collect();
        (unread_call_kind(arguments), Type::Unknown)
    }

    /// FN-4/17: an open callback selects neither role yet. Retain the call's
    /// operands for instance discovery, and the result if both signatures
    /// give it. A concrete binding rereads the selected body normally.
    fn independent_pair(
        &mut self,
        name: &SyntaxToken,
        list: &SyntaxNode,
        (module, value, temporal): (usize, usize, usize),
        expected: Option<&Type>,
        generics: Option<&CallArguments>,
    ) -> (ExprKind, Type) {
        if self.probing_pair_role > 0 {
            return (unread_call_kind(Vec::new()), Type::Unknown);
        }
        let other = self.independent_value_result(name, list, (module, value), expected, generics);
        let (mut kind, ty) = self.call_function(name, list, (module, temporal), expected, generics);
        if let ExprKind::Call { callee, .. } = &mut kind {
            *callee = Callee::Unread;
        }
        let common = other
            .filter(|result| result == &ty)
            .unwrap_or(Type::Unknown);
        (kind, common)
    }

    /// Read a generic value result in its own signature and lexical context.
    /// This discarded reading cannot change the state used by the other role.
    fn independent_value_result(
        &mut self,
        name: &SyntaxToken,
        list: &SyntaxNode,
        (module, declaration): (usize, usize),
        expected: Option<&Type>,
        generics: Option<&CallArguments>,
    ) -> Option<Type> {
        let Detail::Callable(signature) = &self.program.modules[module].details[declaration] else {
            return None;
        };
        let result = signature.result.as_ref()?;
        if !names_parameter(result) {
            return Some(result.clone());
        }
        let site = self.at(list);
        let call = list.parent().unwrap_or_else(|| list.clone());
        if let Some(phases) = &self.pair_phases
            && let Some(results) = phases.independent_value_results.get(&site)
            && let Some((_, _, ty)) = results.iter().find(|(scope, place, _)| {
                place.as_ref() == expected
                    && scope.matches(
                        self,
                        phases,
                        self.at(&call),
                        &call,
                        ValueCategory::Unsettled,
                        false,
                    )
            })
        {
            return ty.clone();
        }
        let scope = PairScope::of(self);
        let mark = self.mark();
        let locals = self.locals.clone();
        let must_return = self.must_return;
        let tails = self.returning_tails.clone();
        let (_, result) = self.call_function(name, list, (module, declaration), expected, generics);
        let clean = self.take_back(mark, false);
        self.locals = locals;
        self.must_return = must_return;
        self.returning_tails = tails;
        let result = clean.then_some(result);
        if let Some(phases) = &mut self.pair_phases {
            let entries = phases.independent_value_results.entry(site).or_default();
            if entries.len() < PAIR_SCOPE_CACHE_LIMIT {
                entries.push((scope, expected.cloned(), result.clone()));
            }
        }
        result
    }

    /// A call of the value function `declaration` of `module`.
    /// `given` is the generic arguments the call writes, when it writes
    /// them.
    ///
    /// GEN-2: "Resolve every needed generic from actual inputs,
    /// requested output, or solvable equality." GEN-8: "A call may
    /// supply its generic arguments as a generic constructor does,
    /// `count<i64>(3)`. It may supply the first of them and leave the
    /// rest to be resolved under GEN-2 ... A call rejects when it leaves
    /// a generic parameter unresolved." GEN-1: "Repeated parameters
    /// require equivalent types or equal constants."
    ///
    /// GEN-2: "A call resolves a generic of its callee from what it
    /// writes after the name (GEN-8), then from the type its place
    /// expects of the result, then from its arguments in declared
    /// parameter order (GEN-12), whatever order they are written in.
    /// The first of these that says what a parameter is decides it, and
    /// what follows is checked against that: an integer literal is
    /// widened to an `f64` so decided (EXPR-5), and another argument of
    /// another type rejects." So `let real: f64 = same(1, 2)` is an
    /// `f64`.
    pub(super) fn call_function(
        &mut self,
        name: &SyntaxToken,
        list: &SyntaxNode,
        (module, declaration): (usize, usize),
        expected: Option<&Type>,
        given: Option<&CallArguments>,
    ) -> (ExprKind, Type) {
        let result = match &self.program.modules[module].details[declaration] {
            Detail::Callable(signature) => signature.result.as_ref(),
            _ => None,
        };
        let target = &self.program.modules[module].declarations[declaration];
        let reads_activity = Some(module) == self.program.core()
            && target.kind == DeclarationKind::NativeFunction(Role::Value)
            && target.name == "is_active";
        if reads_activity
            && let [(false, argument_name, argument)] = self.call_entries(list).as_slice()
            && argument_name
                .as_ref()
                .is_none_or(|name| name.text() == "input")
            && let Some(query) = self.packed_activity(name, argument, given)
        {
            return query;
        }
        if let Some(call) = self.defer_pack_call_with(list, result, expected, |typer| {
            let evaluating = std::mem::take(&mut typer.evaluating);
            typer
                .call_function_with(
                    name,
                    list,
                    (module, declaration),
                    expected,
                    given,
                    (evaluating, false),
                )
                .0
        }) {
            return call;
        }
        // The call an `eval` stands for is this one, and not one among
        // its arguments.
        let evaluating = std::mem::take(&mut self.evaluating);
        // ERR-14: core's `removed_value` "takes its form from its place
        // as `delta_value` does": the call gives the window's V, which a
        // place that takes `result<V, error>` makes the `ok` of its
        // result (`Typer::fit`). The window it is given says what V is,
        // and the place does not.
        let expected = if removes(self.program, (module, declaration)) {
            None
        } else {
            expected
        };
        let (kind, ty) = self.call_function_with(
            name,
            list,
            (module, declaration),
            expected,
            given,
            // An endpoint is never a replayable payload. Even an empty
            // trace cannot supply the receiving node's input (TIME-9).
            (evaluating && !reads_activity, reads_activity),
        );
        // FN-16: "`removed_value` (TYPE-11) of a window that is
        // a child of a `ref` parameter or of `out` of a `ref` result, a
        // field of a struct given by `ref`, is set aside as `delta_value`
        // is: a read of it there rejects with `ref.read`." A window is
        // not marked `ref` itself (`ref.position`).
        if removes(self.program, (module, declaration))
            && let ExprKind::Call { arguments, .. } = &kind
            && let [argument] = arguments.as_slice()
            && let ExprKind::ChildEndpointOf(child) = &argument.value.kind
            && let Some(input) = self
                .input_of(child)
                .or_else(|| match child.place_root().kind {
                    ExprKind::Local(local) => Some(local),
                    _ => None,
                })
            && self.designates(input)
        {
            // The child as it is written.
            let written = self
                .call_entries(list)
                .into_iter()
                .next()
                .map(|(_, _, value)| super::endpoint_argument(&value).text().to_string());
            self.ref_read(
                input,
                written.as_deref(),
                RefRead::Removed,
                argument.value.at,
            );
            return (ExprKind::Unread, Type::Unknown);
        }
        // END-5, TIME-9: only the bound core function queries activity.
        // Its ordinary signature has already checked the arguments and T.
        if reads_activity
            && let ExprKind::Call { arguments, .. } = &kind
            && let [argument] = arguments.as_slice()
            && let ExprKind::EndpointOf(input) = argument.value.kind
        {
            if Some(input) == self.out {
                self.diagnostics.emit(error(
                    Category::Phase,
                    codes::ENDPOINT_ARGUMENT,
                    argument.value.at,
                    "`is_active` takes a direct temporal input of the node".to_string(),
                    "TIME-9",
                ));
            } else if self.hook == Some(Hook::Start) {
                let input_name = self.locals[input].name.clone();
                self.input_lifecycle(Location::new(self.file, name.span()), &input_name);
            }
            return (
                ExprKind::Endpoint {
                    operation: EndpointOperation::IsActive,
                    inputs: Some(vec![input]),
                },
                ty,
            );
        }
        // TIME-9: "`is_active(input)`, `activate(input)` and
        // `passivate(input)` operate on direct temporal inputs": a child
        // has no subscription of its own.
        if reads_activity
            && let ExprKind::Call { arguments, .. } = &kind
            && let [argument] = arguments.as_slice()
            && let ExprKind::ChildEndpointOf(child) = &argument.value.kind
            && self.deferred_pack_selection(child).is_none()
        {
            self.diagnostics.emit(error(
                Category::Phase,
                codes::ENDPOINT_ARGUMENT,
                argument.value.at,
                "`is_active` takes a direct temporal input of the node, and this is a child of \
                 one"
                .to_string(),
                "TIME-9",
            ));
            return (ExprKind::Unread, ty);
        }
        (kind, ty)
    }

    /// [`Typer::call_function`], with `evaluating` true where the call
    /// is the one an `eval` stands for (TST-5): "Const parameters
    /// receive configuration; temporal parameters receive contextually
    /// typed publication traces."
    fn call_function_with(
        &mut self,
        name: &SyntaxToken,
        list: &SyntaxNode,
        (module, declaration): (usize, usize),
        expected: Option<&Type>,
        given: Option<&CallArguments>,
        (evaluating, reads_activity): (bool, bool),
    ) -> (ExprKind, Type) {
        let program = self.program;
        let Some(Detail::Callable(signature)) = program.modules[module].details.get(declaration)
        else {
            return self.unread_call(list);
        };
        let computed = self.deferred_computed_spread(list);
        // What a parameter takes: its type, or in an eval a trace for
        // one that is not `const`. TST-5: "Each cell is one cycle's
        // publication: it is read as the `delta<T>` (TYPE-16) of its
        // parameter's type T". A type that names a generic parameter
        // stands for its cells until they say what it is.
        let taken = |parameter: &crate::types::Parameter, ty: Type| {
            if evaluating && !parameter.constant && !matches!(ty, Type::Function(_)) {
                trace_input_type(ty)
            } else {
                ty
            }
        };
        // TST-16: "A trace given to a `const fn` is an ordinary value
        // there". Not in the call an eval stands for: a `const fn` that
        // is an eval's target is lifted (FN-4), and its parameters take
        // traces as any target's do (TST-5).
        let listed = !evaluating
            && matches!(
                program.modules[module].declarations[declaration]
                    .kind
                    .role(),
                Some(Role::Value)
            );
        let generics = &signature.generics;
        let first_pack = generics
            .iter()
            .position(|generic| matches!(generic.kind, GenericKind::Pack { .. }));
        let explicit = self.explicit_generics(name, signature, given);
        let required = |parameter: &crate::types::Parameter| {
            // UNIT-8: "A parameter whose type is written `T?` needs no
            // default to be omitted."
            // "A `var` parameter is the exception: it is always
            // given, whatever its type" (FN-15).
            parameter.var || !parameter.has_default && !matches!(parameter.ty, Type::Nullable(_))
        };
        let read_pack_bounds =
            |parameter: &Parameter, written| pack_bounds(parameter, generics, written);
        let Some(ExplicitGenerics {
            fixed: mut found,
            faulty,
            pack: written_pack,
        }) = explicit.filter(|_| !generics.is_empty())
        else {
            // No generic parameter, or none this pass resolves: a type
            // that names one is not known.
            let known = |ty: &Type| {
                if names_parameter(ty) {
                    Type::Unknown
                } else {
                    ty.clone()
                }
            };
            let slots: Vec<Slot> = signature
                .parameters
                .iter()
                .map(|parameter| Slot {
                    name: &parameter.name,
                    configuration: parameter.configuration,
                    ty: taken(parameter, known(&parameter.ty)),
                    required: required(parameter),
                    pack: parameter.pack,
                    pack_bounds: read_pack_bounds(parameter, None),
                    type_pack: type_pack(parameter, generics),
                    listed,
                })
                .collect();
            let reported = self.diagnostics.len();
            let arguments = self.bind(name, list, &slots, Takes::Parameters);
            let ty = signature.result.as_ref().map_or(Type::Unknown, known);
            if computed {
                return (unread_call_kind(arguments), ty);
            }
            // GEN-3: a function with no generic parameter may state a
            // `requires` too, of types it names itself, and a call of
            // it is held to that. Not where something else was
            // reported of the call (ERR-9), nor where the function has
            // generic parameters this pass does not resolve.
            if generics.is_empty() && self.diagnostics.len() == reported {
                let at = Location::new(self.file, Span::new(name.span().start, list.span().end));
                if !self.function_laws_met(at, (module, declaration), &[], &[], &arguments) {
                    self.requires_met(at, (module, declaration), &[]);
                }
            }
            let requirements = if generics.is_empty() {
                self.answers(module, declaration, &[])
            } else {
                Vec::new()
            };
            let callee = Callee::Function {
                module,
                declaration,
                generics: Vec::new(),
                type_packs: Vec::new(),
                requirements,
                defer_pack_members: false,
                contextual: Vec::new(),
                test_candidates: self.test_candidates,
            };
            let mut kind = ExprKind::Call {
                callee,
                arguments,
                form: Form::Named,
                text: None,
                braced: Vec::new(),
                lifted: false,
            };
            self.finished_call(&mut kind, evaluating);
            return (kind, ty);
        };

        // GEN-8: "A call may give a generic value parameter of its
        // callee by name, among its named arguments". What the call
        // writes decides first (GEN-2).
        let value_names: Vec<&str> = generics
            .iter()
            .filter(|generic| matches!(generic.kind, GenericKind::Value(_)))
            .map(|generic| generic.name.as_str())
            .collect();
        let callee = &program.modules[module].declarations[declaration];
        let reported = self.diagnostics.len();
        let braces_only = dependent_slot(&program.modules[module], signature).is_some();
        let named = self.named_generics(list, signature, &mut found, braces_only);

        // The requested output: `let names: list<str> = repeat(3)` says
        // what the `T` of `repeat<T>(count: i64) -> list<T>` is.
        // What the call writes is found already, and is not changed.
        let result_fallback = signature
            .result
            .as_ref()
            .zip(expected.filter(|ty| !open(ty)))
            .and_then(|(result, expected)| {
                infer_from_place(result, expected, false, generics, &mut found)
            });
        // The inputs.
        let slots: Vec<Slot> = signature
            .parameters
            .iter()
            .map(|parameter| Slot {
                name: &parameter.name,
                configuration: parameter.configuration,
                ty: Type::Unknown,
                required: required(parameter),
                pack: parameter.pack,
                pack_bounds: read_pack_bounds(parameter, written_pack.as_ref()),
                type_pack: type_pack(parameter, generics),
                listed,
            })
            .collect();
        let patterns: Vec<Type> = signature
            .parameters
            .iter()
            .map(|parameter| taken(parameter, parameter.ty.clone()))
            .collect();
        let places = Places {
            slots: &slots,
            patterns: patterns.iter().collect(),
            generics,
            settled: !computed && !expected.is_some_and(open),
            aside: &value_names,
            values: &named,
            written_pack: written_pack
                .as_ref()
                .map(|pack| (pack.parameter, pack.types.as_slice())),
            result_fallback,
            callable: Some((module, declaration, signature)),
            defer_endpoint_child: reads_activity && self.pack_inputs.is_none(),
            defaults: generics
                .iter()
                .map(|generic| self.default_constant(callee, generic))
                .collect(),
        };
        let Inferred {
            arguments,
            mut found,
            unknown,
            defaulted,
            contextual,
            missing_pack_context,
            type_packs: reflected,
        } = self.infer_arguments(name, list, &places, found, Takes::Parameters);
        if computed {
            return (unread_call_kind(arguments), Type::Unknown);
        }
        let mut type_packs = vec![None; generics.len()];
        // FN-8 and GEN-10: collect the individual types in capture order.
        for (slot, parameter) in slots.iter().enumerate() {
            if let Some(index) = parameter.type_pack {
                type_packs[index].get_or_insert_with(|| {
                    let written = written_pack.as_ref().filter(|pack| pack.parameter == index);
                    let entries: Vec<_> = arguments
                        .iter()
                        .filter(|argument| argument.parameter == Some(slot))
                        .enumerate()
                        .map(|(position, argument)| {
                            let ty = if let Some(ty) =
                                written.and_then(|pack| pack.types.get(position))
                            {
                                ty.clone()
                            } else if evaluating {
                                trace_pack_type(&argument.value.ty)
                                    .cloned()
                                    .unwrap_or(Type::Unknown)
                            } else {
                                self.inferred_from(listed, &argument.value, Takes::Parameters)
                            };
                            let key = argument
                                .pack
                                .clone()
                                .unwrap_or(hir::PackKey::Position(position));
                            (key, ty)
                        })
                        .collect();
                    let mut pack = hir::TypePackArgument::keyed(index, entries);
                    pack.infer_from_arguments = self.graph
                        && !self.signature.generics.is_empty()
                        && !evaluating
                        && written.is_none()
                        && !missing_pack_context.contains(&index)
                        && pack.types.iter().any(|ty| !whole(ty));
                    pack
                });
            }
        }
        if let Some(pack) = &written_pack {
            type_packs[pack.parameter].get_or_insert_with(|| pack.clone());
        }
        for pack in reflected {
            let index = pack.parameter;
            type_packs[index] = Some(pack);
        }
        // GEN-3, GEN-10: positive existential equalities can settle
        // the result's otherwise unresolved type from the first match.
        let known_packs: Vec<_> = type_packs.iter().flatten().cloned().collect();
        super::quantifiers::bind(
            &program.modules[module],
            callee,
            generics,
            &known_packs,
            &mut found,
        );
        self.apply_law_defaults(
            (module, declaration),
            signature,
            &defaulted,
            &mut found,
            &|typer, parameter, law| {
                arguments
                    .iter()
                    .find(|argument| argument.parameter == Some(parameter))
                    .and_then(|argument| typer.law_value(law, &argument.value))
            },
        );
        self.generic_values(name, callee, generics, (&named, &defaulted), &mut found);

        // GEN-8: "A call rejects when it leaves a generic parameter
        // unresolved." Not where something else was reported, or an
        // argument is not known: what would have said what the
        // parameter is may be what is missing (ERR-9).
        //
        // Where the call's place is not known, the type it expects may
        // yet say what a parameter of the result is: the call then has
        // no whole type and is read again once the place is known. A
        // parameter the result does not name is never resolved that
        // way, so it is reported here: the `N` of
        // `tag<const N: i64>(x: i64) -> i64` in `same(tag(1), 2)`.
        let place_unknown = expected.is_some_and(open);
        let unresolved: Vec<&str> = generics
            .iter()
            .zip(&found)
            .zip(&type_packs)
            .filter(|((_, found), pack)| {
                found.is_none()
                    && pack.as_ref().is_none_or(|pack| {
                        !pack.infer_from_arguments && pack.types.iter().any(|ty| !whole(ty))
                    })
            })
            .map(|((generic, _), _)| generic)
            .filter(|generic| {
                !place_unknown
                    || !signature
                        .result
                        .as_ref()
                        .is_some_and(|result| mentions(result, &[generic.at]))
            })
            .map(|generic| generic.name.as_str())
            .collect();
        let judged = !faulty && self.diagnostics.len() == reported && !unknown;
        let at = Location::new(self.file, Span::new(name.span().start, list.span().end));
        let requires_rejects = judged
            && !unresolved.is_empty()
            && self.pack_requires_rejects(at, (module, declaration), &found, &known_packs);
        let reports_unresolved = judged && !unresolved.is_empty() && !requires_rejects;
        if reports_unresolved {
            let names: Vec<String> = unresolved.iter().map(|name| format!("`{name}`")).collect();
            self.diagnostics.emit(
                error(
                    Category::Type,
                    codes::GENERIC_UNRESOLVED,
                    Location::new(self.file, name.span()),
                    format!(
                        "this call of `{}` leaves {} unresolved: no argument and no expected \
                         type says what it is",
                        name.text(),
                        names.join(", ")
                    ),
                    "GEN-8",
                )
                .with_note(format!(
                    "a call may write its generic arguments: `{}<...>(...)`",
                    name.text()
                )),
            );
        }

        if listed && judged {
            self.check_value_type_arguments(
                name.text(),
                Location::new(self.file, name.span()),
                signature,
                found.iter().map(Option::as_ref),
            );
        }

        // The result, with what was found in place. One that still
        // names a parameter nothing resolved is not known.
        let own: Vec<Location> = generics.iter().map(|generic| generic.at).collect();
        let resolved: Vec<Argument> = found
            .into_iter()
            .map(|found| found.unwrap_or(Argument::Type(Type::Unknown)))
            .collect();
        let map: Vec<(Location, &Argument)> = own.iter().copied().zip(&resolved).collect();
        // TYPE-10, TYPE-11: the sizes the callee's parameters and result
        // have with what was resolved.
        sizes::call_sizes(
            program,
            (at, name.text()),
            signature,
            (&own, &resolved),
            self.diagnostics,
        );
        let type_packs: Vec<_> = type_packs.into_iter().flatten().collect();
        let pack_types = PackTypes::new(generics, &type_packs);
        let ty = bind_result(signature.result.as_ref(), &map).map_or(Type::Unknown, |result| {
            placed(
                self.program.resolve_instance_type(
                    pack_types.project(result, &map).unwrap_or(Type::Unknown),
                ),
                &own,
            )
        });
        // GEN-3 and GEN-10: the callee's `requires` holds with what was
        // resolved, including the separate sequence in each type pack.
        // Not where something else was reported of the call (ERR-9).
        let mut defer_pack_members = false;
        if judged
            && unresolved.is_empty()
            && type_packs.iter().all(|pack| !pack.infer_from_arguments)
        {
            if let Some(missing) =
                pack_types.written_member_fault(&program.modules[module], declaration, &map)
            {
                self.pack_member_fault(at, &missing);
            } else if self.function_laws_met(
                at,
                (module, declaration),
                &resolved,
                &type_packs,
                &arguments,
            ) {
                // GEN-18 handled the condition with its function identity.
            } else if first_pack.is_none() {
                defer_pack_members = !self.requires_met(at, (module, declaration), &resolved);
            } else {
                defer_pack_members =
                    !self.pack_requires_met(at, (module, declaration), &resolved, &type_packs);
            }
        }
        // Named operator requirements over pack members still need to
        // be resolved with the quantifier's binding (GEN-10, GEN-16).
        let requirements = if first_pack.is_none() {
            self.answers(module, declaration, &resolved)
        } else {
            Vec::new()
        };
        let callee = Callee::Function {
            module,
            declaration,
            generics: resolved,
            type_packs,
            requirements,
            defer_pack_members,
            test_candidates: self.test_candidates,
            contextual: if !faulty && self.diagnostics.len() == reported {
                contextual
            } else {
                Vec::new()
            },
        };
        let mut kind = ExprKind::Call {
            callee,
            arguments,
            form: Form::Named,
            text: None,
            braced: Vec::new(),
            lifted: false,
        };
        self.finished_call(&mut kind, evaluating);
        // An already-rejected bare success supplies no result type for
        // the nullable place to check again (ERR-9).
        if reports_unresolved
            && result_fallback.is_some_and(|(_, place)| matches!(place, Type::Nullable(_)))
            && matches!(&ty, Type::Result { value, .. } if **value == Type::Unknown)
        {
            return (kind, Type::Unknown);
        }
        (kind, ty)
    }

    /// FN-1 and TYPE-12: an ordinary position of a value function
    /// cannot contain a window. A native endpoint's payload is exempt.
    fn check_value_type_arguments<'g>(
        &mut self,
        name: &str,
        at: Location,
        signature: &Signature,
        arguments: impl Iterator<Item = Option<&'g Argument>>,
    ) {
        let ordinary = |at: Location| {
            signature
                .result
                .as_ref()
                .is_some_and(|result| mentions(result, &[at]))
                || signature.parameters.iter().any(|parameter| {
                    !matches!(parameter.ty, Type::Endpoint(_)) && mentions(&parameter.ty, &[at])
                })
        };
        for (generic, found) in signature.generics.iter().zip(arguments) {
            let Some(Argument::Type(ty)) = found else {
                continue;
            };
            if !ordinary(generic.at) {
                continue;
            }
            if !ty.any_part(&|part| matches!(part, Type::RollingWindow { .. }), &|_| {
                false
            }) {
                continue;
            }
            self.diagnostics.emit(
                error(
                    Category::Type,
                    codes::TYPE_NOT_ORDINARY,
                    at,
                    format!(
                        "`{ty}` is not an ordinary value type, and the `{}` of the value \
                         function `{}` stands for it here",
                        generic.name, name
                    ),
                    "FN-1",
                )
                .with_note(
                    "a value function's parameters and result are ordinary values; a window \
                     given to one is the `list<V>` it holds (TYPE-11)",
                ),
            );
        }
    }

    /// GEN-2 and GEN-12: settled argument types can complete a named
    /// call that the source pass could not infer. Keep arguments already
    /// decided by the source and apply the usual checks when all are known.
    pub(super) fn settle_function_call(&mut self, call: &mut Expr) -> bool {
        let ExprKind::Call {
            callee:
                Callee::Function {
                    module,
                    declaration,
                    generics,
                    type_packs,
                    requirements,
                    defer_pack_members,
                    contextual,
                    ..
                },
            arguments,
            ..
        } = &mut call.kind
        else {
            return false;
        };
        let owner = &self.program.modules[*module];
        let Detail::Callable(signature) = &owner.details[*declaration] else {
            return false;
        };
        if signature.generics.len() != generics.len() {
            return false;
        }
        let mut found = Vec::new();
        for (index, (parameter, argument)) in
            signature.generics.iter().zip(generics.iter()).enumerate()
        {
            if matches!(parameter.kind, GenericKind::Pack { .. }) {
                if !type_packs.iter().any(|pack| pack.parameter == index) {
                    return false;
                }
                // A pack's types are separate from its placeholder.
                found.push(Some(argument.clone()));
            } else if matches!(argument, Argument::Type(Type::Unknown)) {
                if parameter.kind != GenericKind::Type {
                    return false;
                }
                found.push(None);
            } else {
                found.push(Some(argument.clone()));
            }
        }
        // FN-17: synthesized captures are not arguments written at
        // this call. They do not take part in inference or source replay.
        let written = |argument: &hir::Argument| {
            argument
                .parameter
                .is_none_or(|slot| slot < signature.parameters.len())
        };
        let Some(mut supplied) = arguments
            .iter()
            .filter(|argument| written(argument))
            .enumerate()
            .filter(|(position, _)| {
                contextual
                    .binary_search_by_key(position, |argument| argument.position)
                    .is_err()
            })
            .map(|(_, argument)| Some((argument.parameter?, &argument.value)))
            .collect::<Option<Vec<_>>>()
        else {
            return false;
        };
        // Use declaration order for inference, leaving the HIR itself
        // in written order for evaluation (GEN-12, EXPR-13).
        supplied.sort_by_key(|(slot, _)| *slot);
        let declared = &owner.declarations[*declaration];
        let listed = declared.kind.role() == Some(Role::Value);
        let inferred_here: Vec<_> = found.iter().map(Option::is_none).collect();
        let mut unfound: Vec<_> = signature
            .generics
            .iter()
            .zip(&found)
            .filter_map(|(generic, found)| found.is_none().then_some(generic.at))
            .collect();
        // GEN-13: a dependent value needs a contextual reading once its
        // type settles. Recover only defaults whose omission is proven.
        let dependent: Vec<_> = signature
            .generics
            .iter()
            .enumerate()
            .filter_map(|(index, generic)| {
                matches!(&generic.kind, GenericKind::Value(ty) if mentions(ty, &unfound))
                    .then_some(index)
            })
            .collect();
        let defaults = if dependent.is_empty() {
            None
        } else {
            let Some((name, defaulted)) =
                self.omitted_function_defaults(call.at, (*module, *declaration), signature)
            else {
                return false;
            };
            if !dependent.iter().all(|index| defaulted.contains(index)) {
                return false;
            }
            Some((name, defaulted))
        };
        for (slot, value) in supplied {
            let Some(parameter) = signature.parameters.get(slot) else {
                return false;
            };
            if !whole(&value.ty) || self.category(value) == ValueCategory::Unsettled {
                return false;
            }
            let widenable = bare_parameter(&parameter.ty, &signature.generics).filter(|&index| {
                inferred_here[index]
                    && matches!(&found[index], Some(Argument::Type(Type::Named { .. })))
            });
            if !mentions(&parameter.ty, &unfound) && widenable.is_none() {
                continue;
            }
            let actual = self.inferred_from(listed, value, Takes::Parameters);
            let actual =
                self.inference_input(&parameter.ty, &actual, &signature.generics, &found, None);
            // GEN-2: a later abstract parent can widen a bare
            // parameter inferred from an earlier argument.
            if let Some(index) = widenable {
                if let Some(Argument::Type(decided)) = &found[index]
                    && self.widens(decided, actual)
                {
                    found[index] = Some(Argument::Type(actual.clone()));
                }
                continue;
            }
            infer(&parameter.ty, actual, &signature.generics, &mut found);
            unfound = signature
                .generics
                .iter()
                .zip(&found)
                .filter_map(|(generic, found)| found.is_none().then_some(generic.at))
                .collect();
        }
        if found.iter().any(Option::is_none) {
            return false;
        }
        let mut packs = type_packs.clone();
        if !self.settle_type_packs(signature, arguments, &mut packs, listed) {
            return false;
        }
        let Some(sources) = contextual
            .iter()
            .map(|argument| {
                let node = self.node_at(argument.at)?;
                // A positional argument and its expression can share a span.
                let value = if node.kind() == SyntaxKind::Arg {
                    node.children().next()?
                } else {
                    node
                };
                Some((argument, value))
            })
            .collect::<Option<Vec<_>>>()
        else {
            return false;
        };
        if let Some((name, defaulted)) = defaults {
            for index in dependent {
                found[index] = None;
            }
            let before = self.diagnostics.len();
            self.generic_values(
                &name,
                declared,
                &signature.generics,
                (&[], &defaulted),
                &mut found,
            );
            if self.diagnostics.len() != before {
                call.kind = ExprKind::Unread;
                return true;
            }
        }
        let Some(resolved) = found.into_iter().collect::<Option<Vec<_>>>() else {
            return false;
        };

        let own: Vec<_> = signature
            .generics
            .iter()
            .map(|generic| generic.at)
            .collect();
        let map: Vec<_> = own.iter().copied().zip(&resolved).collect();
        let before = self.diagnostics.len();
        if listed {
            self.check_value_type_arguments(
                &declared.name,
                call.at,
                signature,
                resolved.iter().map(Some),
            );
        }
        if !sources.is_empty() {
            self.index_instance_locals();
        }
        let pack_types = PackTypes::new(&signature.generics, &packs);
        // Category recording rebuilds captures from the completed callbacks.
        arguments.retain(written);
        let mut sources = sources.into_iter().peekable();
        for (position, argument) in arguments.iter_mut().enumerate() {
            let parameter =
                &signature.parameters[argument.parameter.expect("the call binds each argument")];
            // Graph destinations fit heterogeneous pack elements using
            // their own types in capture order (FN-8, GEN-10).
            if type_pack(parameter, &signature.generics).is_some() {
                continue;
            }
            let ty = placed(
                pack_types
                    .project(&parameter.ty, &map)
                    .unwrap_or(Type::Unknown),
                &own,
            );
            if let Some((source, node)) = sources.next_if(|(source, _)| source.position == position)
            {
                if !self.may_reread() {
                    continue;
                }
                // EXPR-9, ERR-2 and FN-17: use the normal argument reader
                // with its completed place, in written order (EXPR-2).
                let slot = Slot {
                    name: &parameter.name,
                    configuration: parameter.configuration,
                    ty,
                    required: false,
                    pack: parameter.pack,
                    pack_bounds: parameter.pack_bounds,
                    type_pack: None,
                    listed,
                };
                let previous = std::mem::take(&mut self.present);
                self.present
                    .extend(source.facts.iter().filter_map(|fact| match fact {
                        hir::ArgumentFact::Present(at) => {
                            self.declared.get(at).copied().map(Fact::Present)
                        }
                        hir::ArgumentFact::Valid(at) => {
                            self.declared.get(at).copied().map(Fact::Valid)
                        }
                        hir::ArgumentFact::Success(at) => {
                            self.declared.get(at).copied().map(Fact::Success)
                        }
                        hir::ArgumentFact::Failure(at) => {
                            self.declared.get(at).copied().map(Fact::Failure)
                        }
                    }));
                argument.value = self
                    .argument(
                        argument.parameter,
                        &node,
                        Some((&slot, false)),
                        Takes::Parameters,
                    )
                    .value;
                self.present = previous;
                continue;
            }
            if !matches!(ty, Type::Function(_))
                && !self.lifted_window(&ty, listed, &argument.value, Takes::Parameters)
                && argument.value.ty != ty
            {
                let place = format!("the parameter `{}`", parameter.name);
                argument.value = self.fit_input(argument.value.clone(), &ty, &place);
            }
        }
        sizes::call_sizes(
            self.program,
            (call.at, &declared.name),
            signature,
            (&own, &resolved),
            self.diagnostics,
        );
        let callee = (*module, *declaration);
        // GEN-10: the same quantified check applies when a graph
        // completes the pack types that source typing could not see.
        if self.diagnostics.len() == before {
            if let Some(missing) = pack_types.written_member_fault(owner, *declaration, &map) {
                self.pack_member_fault(call.at, &missing);
            } else if packs.is_empty() {
                *defer_pack_members = !self.requires_met(call.at, callee, &resolved);
            } else {
                *defer_pack_members = !self.pack_requires_met(call.at, callee, &resolved, &packs);
            }
        }
        if packs.is_empty() {
            *requirements = self.answers(callee.0, callee.1, &resolved);
        }
        call.ty = bind_result(signature.result.as_ref(), &map).map_or(Type::Unknown, |result| {
            placed(
                self.program.resolve_instance_type(
                    pack_types.project(result, &map).unwrap_or(Type::Unknown),
                ),
                &own,
            )
        });
        *generics = resolved;
        *type_packs = packs;
        contextual.clear();
        self.finished_call(&mut call.kind, false);
        true
    }

    /// GEN-8, GEN-10 and FN-8: an inferred pack keeps its capture order
    /// and any types already known. A written pack keeps all its types.
    fn settle_type_packs(
        &self,
        signature: &Signature,
        arguments: &[hir::Argument],
        packs: &mut [hir::TypePackArgument],
        listed: bool,
    ) -> bool {
        for pack in packs {
            if pack.infer_from_arguments {
                let Some(slot) = signature.parameters.iter().position(|parameter| {
                    type_pack(parameter, &signature.generics) == Some(pack.parameter)
                }) else {
                    return false;
                };
                let captured = arguments
                    .iter()
                    .filter(|argument| argument.parameter == Some(slot));
                if captured.clone().count() != pack.types.len() {
                    return false;
                }
                for (ty, argument) in pack.types.iter_mut().zip(captured) {
                    if !whole(ty) {
                        fill_type(
                            ty,
                            self.inferred_from(listed, &argument.value, Takes::Parameters),
                        );
                    }
                }
            }
            if !pack.types.iter().all(whole) {
                return false;
            }
            pack.infer_from_arguments = false;
        }
        true
    }

    /// Defaults omitted from the original named call (GEN-8). A written
    /// generic option would need its own source to be read in context.
    fn omitted_function_defaults(
        &self,
        at: Location,
        (module, declaration): (usize, usize),
        signature: &Signature,
    ) -> Option<(SyntaxToken, Vec<usize>)> {
        let call = self.node_at(at)?;
        let callee = call.children().next()?;
        if call.kind() != SyntaxKind::CallExpr
            || callee.kind() != SyntaxKind::NameExpr
            || child(&callee, SyntaxKind::GenericArgs).is_some()
        {
            return None;
        }
        let name = name_token(&callee)?;
        if !matches!(self.binding_at(at.file, name.span().start), Some(Binding::Declarations {
            module: bound_module,
            declarations,
        }) if *bound_module == module && declarations.contains(&declaration))
        {
            return None;
        }
        let list = child(&call, SyntaxKind::ArgList)?;
        let entries = self.call_entries(&list);
        let owner = &self.program.modules[module];
        let braces_only = dependent_slot(owner, signature).is_some();
        let declared = &owner.declarations[declaration];
        let mut defaulted = Vec::new();
        for (index, generic) in signature.generics.iter().enumerate() {
            if matches!(generic.kind, GenericKind::Value(_))
                && generic_default(&declared.syntax, generic).is_some()
                && !entries.iter().any(|(brace, name, _)| {
                    (*brace || !braces_only)
                        && name
                            .as_ref()
                            .is_some_and(|name| name.text() == generic.name)
                })
            {
                defaulted.push(index);
            }
        }
        Some((name, defaulted))
    }

    /// The member an enum's construction from `argument` names, where
    /// that is a constant: its place among the members, `Ok`, or what
    /// the enum has none of, `Err`. `None` for a value that is no
    /// constant read here. TYPE-7: "`Side(1)` is the member whose
    /// number is 1, and `Side("buy")` the member of that exact name."
    ///
    /// A member whose number is not known says nothing sure of which
    /// numbers the enum has, and a number is then not looked for.
    fn member_constructed(
        &self,
        module: usize,
        declaration: usize,
        argument: &SyntaxNode,
    ) -> Option<Result<usize, String>> {
        let Some(Detail::Enum(read)) = self.program.modules[module].details.get(declaration) else {
            return None;
        };
        let members = &read.members;
        match self.read_constant(argument, None).ok()? {
            ConstantValue::Int(given) if members.iter().all(|member| member.value.is_some()) => {
                Some(
                    members
                        .iter()
                        .position(|member| member.value == Some(given))
                        .ok_or_else(|| format!("no member numbered {given}")),
                )
            }
            ConstantValue::Str(given) => Some(
                members
                    .iter()
                    .position(|member| member.name == given)
                    .ok_or_else(|| format!("no member named `{given}`")),
            ),
            _ => None,
        }
    }
}

fn unread_call_kind(arguments: Vec<hir::Argument>) -> ExprKind {
    ExprKind::Call {
        callee: Callee::Unread,
        arguments,
        form: Form::Named,
        text: None,
        braced: Vec::new(),
        lifted: false,
    }
}

/// The value and temporal definitions of one name, where declared.
fn function_pair(
    program: &Program,
    module: usize,
    declarations: &[usize],
) -> (Option<usize>, Option<usize>) {
    let mut value = None;
    let mut temporal = None;
    for &index in declarations {
        match program.modules[module].declarations[index].kind {
            DeclarationKind::Function(Role::Value)
            | DeclarationKind::NativeFunction(Role::Value) => {
                value.get_or_insert(index);
            }
            DeclarationKind::Function(Role::Temporal)
            | DeclarationKind::NativeFunction(Role::Temporal) => {
                temporal.get_or_insert(index);
            }
            _ => {}
        }
    }
    (value, temporal)
}

/// The callable expression, as it is written before the arguments.
pub(super) fn call_callee(node: &SyntaxNode) -> Option<SyntaxNode> {
    if node.kind() == SyntaxKind::CallExpr {
        node.children().next()
    } else {
        None
    }
}

/// A named function call with a fixed generic or a pack still to infer.
pub(super) fn pending_function_inference(kind: &ExprKind) -> bool {
    matches!(kind, ExprKind::Call { callee: Callee::Function { generics, type_packs, .. }, .. }
        if type_packs.iter().any(|pack| pack.infer_from_arguments && !pack.types.iter().all(whole))
            || generics.iter().enumerate().any(|(index, argument)|
                matches!(argument, Argument::Type(Type::Unknown))
                    && !type_packs.iter().any(|pack| pack.parameter == index)))
}

/// The arguments of an argument list, in order: each with its name,
/// when it has one, and its value.
pub(super) fn arguments_of(list: &SyntaxNode) -> Vec<(Option<SyntaxToken>, SyntaxNode)> {
    list.children()
        .filter(|argument| argument.kind() == SyntaxKind::Arg)
        .filter_map(|argument| {
            let value = argument.children().next()?;
            // `name: value`: the name is the first token, any word
            // (LEX-9).
            let name = has_token(&argument, SyntaxKind::Colon)
                .then(|| tokens(&argument).next())
                .flatten();
            Some((name, value))
        })
        .collect()
}

/// True where `callee`, a module's position and a declaration's, is
/// core's `removed_value`, the value that left a window in the cycle
/// (TIME-9, TYPE-11).
pub(super) fn removes(program: &Program, (module, declaration): (usize, usize)) -> bool {
    let target = &program.modules[module].declarations[declaration];
    Some(module) == program.core()
        && target.kind == DeclarationKind::NativeFunction(Role::Value)
        && target.name == "removed_value"
}

#[cfg(test)]
pub(super) fn count_anonymous_read() {
    tests::ANONYMOUS_READS.with(|count| count.set(count.get() + 1));
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use hgl_diagnostics::Diagnostics;
    use hgl_parser::{Entry, Limits, parse};
    use hgl_source::SourceMap;

    use crate::{ParsedFile, analyse};

    thread_local! {
        pub(super) static PHASE_PROBES: Cell<usize> = const { Cell::new(0) };
        pub(super) static PHASE_ROOTS: Cell<usize> = const { Cell::new(0) };
        pub(super) static ANONYMOUS_READS: Cell<usize> = const { Cell::new(0) };
    }

    fn check_with_phase_count(body: &str) -> (Diagnostics, usize, usize) {
        let source = format!(
            "module m\n\
             const fn scale(value: i64) -> i64 => value * 2\n\
             fn scale(value: i64) -> i64 => value * 3\n{body}"
        );
        let mut sources = SourceMap::new();
        let mut diagnostics = Diagnostics::new();
        let file = sources.add("m.hgl", &source).unwrap();
        let tokens = hgl_lexer::lex(&sources, file, &mut diagnostics);
        let tree = parse(
            &sources,
            file,
            &tokens,
            Entry::SourceFile,
            Limits::default(),
            &mut diagnostics,
        );
        PHASE_PROBES.set(0);
        PHASE_ROOTS.set(0);
        ANONYMOUS_READS.set(0);
        analyse(&[ParsedFile::new(file, tree.syntax())], &mut diagnostics);
        (diagnostics, PHASE_ROOTS.get(), PHASE_PROBES.get())
    }

    fn check_with_read_counts(body: &str) -> (Diagnostics, usize, usize, usize) {
        let source = format!("module m\n{body}");
        let limits = Limits {
            nesting: 512,
            tree_depth: 1024,
        };
        std::thread::Builder::new()
            .stack_size(crate::stack_size(limits))
            .spawn(move || {
                let mut sources = SourceMap::new();
                let mut diagnostics = Diagnostics::new();
                let mut trees = Vec::new();
                for (name, text) in crate::CORE_FILES {
                    let file = sources.add(name, text).unwrap();
                    let tokens = hgl_lexer::lex(&sources, file, &mut diagnostics);
                    let tree = parse(
                        &sources,
                        file,
                        &tokens,
                        Entry::SourceFile,
                        limits,
                        &mut diagnostics,
                    );
                    trees.push((file, tree));
                }
                let file = sources.add("m.hgl", &source).unwrap();
                let tokens = hgl_lexer::lex(&sources, file, &mut diagnostics);
                let tree = parse(
                    &sources,
                    file,
                    &tokens,
                    Entry::SourceFile,
                    limits,
                    &mut diagnostics,
                );
                trees.push((file, tree));
                let parsed: Vec<_> = trees
                    .iter()
                    .map(|(file, tree)| ParsedFile::new(*file, tree.syntax()))
                    .collect();
                PHASE_PROBES.set(0);
                PHASE_ROOTS.set(0);
                ANONYMOUS_READS.set(0);
                analyse(&parsed, &mut diagnostics);
                (
                    diagnostics,
                    PHASE_ROOTS.get(),
                    PHASE_PROBES.get(),
                    ANONYMOUS_READS.get(),
                )
            })
            .unwrap()
            .join()
            .unwrap()
    }

    /// FN-4: phase probes can cross a computed spread without probing
    /// the same three calls again during the selected call's reading.
    #[test]
    fn phased_pairs_share_reads_around_a_pack_comprehension() {
        let (diagnostics, roots, probes) = check_with_phase_count(
            "fn compose<...Inputs>(cb: fn(...{Inputs}) -> i64, value: i64) -> i64 {
                return value + scale(scale(scale(cb(...{for key in arg_keys(Inputs): 7}))))
            }\n",
        );
        assert!(diagnostics.as_slice().is_empty(), "{diagnostics:?}");
        // The computed spread is read in two analysis passes. Each
        // has its own memo, with one probe per written pair call.
        assert_eq!((roots, probes), (2, 6));
    }

    /// FN-4: the same inner pair does not need another phase read when
    /// it does not read the parameter whose category changed.
    #[test]
    fn phased_pairs_share_reads_across_an_unused_formal_category_change() {
        let (diagnostics, roots, probes) = check_with_phase_count(
            "const fn quote(price: const fn(i64) -> i64, quantity: i64) -> i64 => price(quantity)
             fn quote(price: fn(i64) -> i64, quantity: i64) -> i64 => price(quantity)
             const fn surcharge(cost: const fn(i64) -> i64, units: i64) -> i64 => cost(units) * 2
             fn surcharge(cost: fn(i64) -> i64, units: i64) -> i64 => cost(units) * 3
             fn receipt(rate: i64) -> i64 =>
                 quote(fn(quantity) => quantity + surcharge(fn(fee) => fee + rate, 2), 2)",
        );
        assert!(diagnostics.as_slice().is_empty(), "{diagnostics:?}");
        assert_eq!((roots, probes), (1, 2));
    }

    /// FN-4: an inner pair that reads the enclosing formal must see its
    /// actual category in the committed read.
    #[test]
    fn phased_pairs_read_a_changed_formal_when_it_is_used() {
        let (diagnostics, roots, probes) = check_with_phase_count(
            "const fn quote(price: const fn(i64) -> i64, quantity: i64) -> i64 => price(quantity)
             fn quote(price: fn(i64) -> i64, quantity: i64) -> i64 => price(quantity)
             const fn surcharge(cost: const fn(i64) -> i64, units: i64) -> i64 => cost(units) * 2
             fn surcharge(cost: fn(i64) -> i64, units: i64) -> i64 => cost(units) * 3
             fn receipt(rate: i64) -> i64 =>
                 quote(fn(quantity) => rate + surcharge(fn(fee) => fee + quantity, 2), 2)",
        );
        assert!(diagnostics.as_slice().is_empty(), "{diagnostics:?}");
        assert_eq!((roots, probes), (1, 3));
    }

    /// FN-4 and TST-5: an eval inside a pair's argument retains its
    /// ordinary type diagnostic when the same phase probe is reused.
    #[test]
    fn phased_pairs_share_reads_around_an_eval_operand() {
        let (diagnostics, roots, probes) = check_with_phase_count(
            "fn probe(value: i64) -> i64 => value
            fn configured(const bias: i64, value: i64) -> i64 => bias + value
            test nested {
                eval configured(bias: scale(scale(scale(eval probe(value: [1])))), value: [1])
            }\n",
        );
        assert_eq!(diagnostics.as_slice().len(), 1, "{diagnostics:?}");
        assert_eq!(
            diagnostics.as_slice()[0].code,
            hgl_diagnostics::Code::Catalogued(crate::codes::TYPE_MISMATCH),
        );
        assert_eq!((roots, probes), (1, 3));
    }

    /// FN-4: deeply nested anonymous callbacks given to pairs are probed
    /// and read a bounded number of times per level at depths 16 and 32,
    /// including temporal calls, connection captures, pure callbacks,
    /// mixed forms, and scalar-wrapped temporal callbacks.
    #[test]
    fn nested_pair_callbacks_keep_bounded_reads_at_depth_16_and_32() {
        let pair_header = concat!(
            "const fn ap(f: const fn(i64) -> i64, x: i64) -> i64 => f(x) * 2\n",
            "fn ap(f: fn(i64) -> i64, x: i64) -> i64 => f(x) * 3\n",
            "fn later(n: i64) -> i64 { when { return n + 10 } }\n",
        );
        for depth in [16usize, 32] {
            let later = (0..depth).fold("1".to_string(), |acc, i| {
                format!("ap(fn(n{i}) => later(n{i}) + {acc}, 1)")
            });
            let capture = (0..depth).fold("1".to_string(), |acc, i| {
                format!("ap(fn(n{i}) => n{i} + tick + {acc}, 1)")
            });
            let pure = (0..depth).fold("1".to_string(), |acc, i| {
                format!("ap(fn(n{i}) => n{i} + {acc}, 1)")
            });
            let mixed = (0..depth).fold("1".to_string(), |acc, i| match i % 3 {
                0 => format!("ap(fn(n{i}) => later(n{i}) + {acc}, 1)"),
                1 => format!("ap(fn(n{i}) => n{i} + tick + {acc}, 1)"),
                _ => format!("ap(fn(n{i}) => n{i} + {acc}, 1)"),
            });
            for (name, expr) in [
                ("later", later),
                ("capture", capture),
                ("pure", pure),
                ("mixed", mixed),
            ] {
                let body = format!("{pair_header}fn g(tick: i64) -> i64 => tick + {expr}\n");
                let (diagnostics, roots, probes, reads) = check_with_read_counts(&body);
                assert!(
                    diagnostics.as_slice().is_empty(),
                    "{name} d={depth}: {diagnostics:?}"
                );
                assert_eq!(roots, 1, "{name} d={depth}");
                assert_eq!(probes, depth, "{name} d={depth}: probes={probes}");
                assert_eq!(reads, 2 * depth, "{name} d={depth}: reads={reads}");
            }

            let wrapped = (1..depth).fold("ap(fn(n0) => later(n0), 1)".to_string(), |acc, i| {
                format!("ap(fn(n{i}) => n{i} + 1, {acc})")
            });
            let body = format!("{pair_header}fn g(tick: i64) -> i64 => tick + {wrapped}\n");
            let (diagnostics, roots, probes, reads) = check_with_read_counts(&body);
            assert!(
                diagnostics.as_slice().is_empty(),
                "wrapped d={depth}: {diagnostics:?}"
            );
            assert_eq!(roots, 1, "wrapped d={depth}");
            assert_eq!(probes, depth, "wrapped d={depth}: probes={probes}");
            assert_eq!(reads, 2 * depth, "wrapped d={depth}: reads={reads}");
        }
    }

    /// FN-4 and FN-14: nested pair callbacks across distinct lexical
    /// environments (chained outer formals, all outer formals captured at
    /// the leaf, top-down capture cascades, unread `let`/`var` bindings
    /// from formals, runtime `if`/`switch` branches, and nullable guards)
    /// keep bounded probe and body-read counts at depths 16 and 32.
    #[test]
    fn nested_pair_callbacks_across_lexical_environments_keep_bounded_reads() {
        let pair_header = concat!(
            "const fn ap(f: const fn(i64) -> i64, x: i64) -> i64 => f(x) * 2\n",
            "fn ap(f: fn(i64) -> i64, x: i64) -> i64 => f(x) * 3\n",
            "fn later(n: i64) -> i64 { when { return n + 10 } }\n",
        );
        let null_header = concat!(
            "const fn ap(f: const fn(i64?) -> i64, x: i64?) -> i64 => f(x) * 2\n",
            "fn ap(f: fn(i64?) -> i64, x: i64?) -> i64 => f(x) * 3\n",
            "fn later(n: i64) -> i64 { when { return n + 10 } }\n",
        );
        for depth in [16usize, 32] {
            let formal_chain =
                (1..depth).fold("ap(fn(n0) => later(n0) + n1, 1)".to_string(), |acc, i| {
                    if i == depth - 1 {
                        format!("ap(fn(n{i}) => {acc}, 1)")
                    } else {
                        format!("ap(fn(n{i}) => n{} + {acc}, 1)", i + 1)
                    }
                });
            let sum_all = (0..depth)
                .map(|i| format!("n{i}"))
                .collect::<Vec<_>>()
                .join(" + ");
            let all_at_leaf = (1..depth).fold(
                format!("ap(fn(n0) => {sum_all} + later(n0), 1)"),
                |acc, i| format!("ap(fn(n{i}) => {acc}, 1)"),
            );
            let top_down_inner = (1..depth - 1)
                .fold(format!("ap(fn(n0) => {sum_all}, 1)"), |acc, i| {
                    format!("ap(fn(n{i}) => {acc}, 1)")
                });
            let top_down = format!("ap(fn(n{}) => {top_down_inner} + tick, 1)", depth - 1);
            let let_unused = (0..depth).fold("1".to_string(), |acc, i| {
                format!("ap(fn(n{i}) => {{ let v{i} = n{i} + 1\n later(v{i}) + {acc} }}, 1)")
            });
            let var_unused = (0..depth).fold("1".to_string(), |acc, i| {
                format!("ap(fn(n{i}) => {{ var v{i} = n{i} + 1\n later(v{i}) + {acc} }}, 1)")
            });
            let if_branch = (0..depth).fold("1".to_string(), |acc, i| {
                format!("ap(fn(n{i}) => if n{i} > 0 {{ later(n{i}) + {acc} }} else {{ 0 }}, 1)")
            });
            let switch_branch = (0..depth).fold("1".to_string(), |acc, i| {
                format!(
                    "ap(fn(n{i}) => {{ switch n{i} {{\n case 0: return 0\n default: return later(n{i}) + {acc}\n }} }}, 1)"
                )
            });
            let nullable_guard = (0..depth).fold("1".to_string(), |acc, i| {
                format!("ap(fn(n{i}) => if n{i} != null {{ later(n{i}) + {acc} }} else {{ 0 }}, 1)")
            });

            for (name, header, expr, max_probes, max_reads) in [
                ("formal_chain", pair_header, formal_chain, depth, 2 * depth),
                ("all_at_leaf", pair_header, all_at_leaf, depth, 2 * depth),
                ("top_down", pair_header, top_down, 2 * depth, 3 * depth),
                ("let_unused", pair_header, let_unused, depth, 2 * depth),
                ("var_unused", pair_header, var_unused, depth, 2 * depth),
                ("if_branch", pair_header, if_branch, depth, 2 * depth),
                (
                    "switch_branch",
                    pair_header,
                    switch_branch,
                    depth,
                    2 * depth,
                ),
                (
                    "nullable_guard",
                    null_header,
                    nullable_guard,
                    depth,
                    2 * depth,
                ),
            ] {
                let body = format!("{header}fn g(tick: i64) -> i64 => tick + {expr}\n");
                let (diagnostics, roots, probes, reads) = check_with_read_counts(&body);
                assert!(
                    diagnostics.as_slice().is_empty(),
                    "{name} d={depth}: {diagnostics:?}"
                );
                assert_eq!(roots, 1, "{name} d={depth}");
                assert!(
                    probes <= max_probes,
                    "{name} d={depth}: probes={probes} > {max_probes}"
                );
                assert!(
                    reads <= max_reads,
                    "{name} d={depth}: reads={reads} > {max_reads}"
                );
            }
        }
    }

    /// FN-4 and FN-17: generic pair and non-pair callbacks with inferred
    /// parameter or result types keep bounded reads at depths 16 and 32.
    #[test]
    fn nested_generic_callbacks_keep_bounded_reads() {
        let pair_r = concat!(
            "const fn ap<R>(f: const fn(i64) -> R, x: i64) -> R => f(x)\n",
            "fn ap<R>(f: fn(i64) -> R, x: i64) -> R => f(x)\n",
            "fn later(n: i64) -> i64 { when { return n + 10 } }\n",
        );
        let pair_t = concat!(
            "const fn ap<T>(f: const fn(T) -> T, x: T) -> T => f(x)\n",
            "fn ap<T>(f: fn(T) -> T, x: T) -> T => f(x)\n",
            "fn later(n: i64) -> i64 { when { return n + 10 } }\n",
        );
        let pair_tr = concat!(
            "const fn ap<T,R>(f: const fn(T) -> R, x: T) -> R => f(x)\n",
            "fn ap<T,R>(f: fn(T) -> R, x: T) -> R => f(x)\n",
            "fn later(n: i64) -> i64 { when { return n + 10 } }\n",
        );
        let nonpair_r = concat!(
            "fn ap<R>(f: fn(i64) -> R, x: i64) -> R => f(x)\n",
            "fn later(n: i64) -> i64 { when { return n + 10 } }\n",
        );
        for depth in [16usize, 32] {
            let later = (0..depth).fold("1".to_string(), |acc, i| {
                format!("ap(fn(n{i}) => later(n{i}) + {acc}, 1)")
            });
            let pure = (0..depth).fold("1".to_string(), |acc, i| {
                format!("ap(fn(n{i}) => n{i} + {acc}, 1)")
            });
            let blocked = (0..depth).fold("1".to_string(), |inner, i| {
                format!("ap(fn(n{i}) => later(n{i}) + {inner}, {{ 1 }})")
            });
            let blocked_first = (0..depth).fold("1".to_string(), |inner, i| {
                format!("ap(x: {{ 1 }}, f: fn(n{i}) => n{i} + {inner})")
            });
            for (name, header, expr, max_probes, max_reads) in [
                ("pair_r", pair_r, &later, depth, 2 * depth),
                ("pair_t", pair_t, &later, depth, 2 * depth),
                ("pure_pair_r", pair_r, &pure, depth, 3 * depth),
                ("pure_pair_t", pair_t, &pure, depth, 3 * depth),
                ("nonpair_r", nonpair_r, &later, 0, depth),
                ("later_block", pair_tr, &blocked, depth, 2 * depth),
                ("earlier_block", pair_tr, &blocked_first, depth, 3 * depth),
            ] {
                let body = format!("{header}fn g(tick: i64) -> i64 => tick + {expr}\n");
                let (diagnostics, _, probes, reads) = check_with_read_counts(&body);
                assert!(
                    diagnostics.as_slice().is_empty(),
                    "{name} d={depth}: {diagnostics:?}"
                );
                assert!(
                    probes <= max_probes,
                    "{name} d={depth}: probes={probes} > {max_probes}"
                );
                assert!(
                    reads <= max_reads,
                    "{name} d={depth}: reads={reads} > {max_reads}"
                );
            }
        }
    }

    /// FN-4: a non-callback block operand that discards a temporal pair call
    /// and yields an ordinary tail value still selects the outer value overload.
    #[test]
    fn pair_operand_block_discarding_temporal_pair_stays_value() {
        let (diagnostics, _, _, _) = check_with_read_counts(concat!(
            "fn later(n: i64) -> i64 { when { return n + 10 } }\n",
            "const fn tag(x: i64) -> i64 => x + 10\n",
            "fn tag(x: i64) -> str => \"temporal\"\n",
            "const fn ap(f: const fn(i64) -> i64, x: i64) -> i64 => f(x)\n",
            "fn ap(f: fn(i64) -> i64, x: i64) -> i64 => f(x)\n",
            "fn g(tick: i64) -> i64 {\n",
            "    let value: i64 = tag({ let unused = ap(fn(n) => later(n), 1)\n 5 })\n",
            "    return tick + value\n",
            "}\n",
        ));
        assert!(diagnostics.as_slice().is_empty(), "{diagnostics:?}");
    }

    /// FN-4 and FN-17: probing an outer pair callback that enters a value
    /// callback calling an inner pair with a captured connection preserves the
    /// connection capture so the outer pair selects its temporal overload.
    #[test]
    fn value_callback_pair_preserves_captured_connection() {
        let (diagnostics, _, _, _) = check_with_read_counts(concat!(
            "const fn doubled(x: i64) -> i64 => x * 2\n",
            "fn doubled(x: i64) -> i64 => x * 3\n",
            "const fn gdoubled<T>(x: T) -> T => x\n",
            "fn gdoubled<T>(x: T) -> T => x\n",
            "const fn pair_const_cb(f: const fn(i64) -> i64, x: i64) -> i64 => f(x) * 2\n",
            "fn pair_const_cb(f: const fn(i64) -> i64, x: i64) -> i64 => f(x) * 3\n",
            "const fn apply_value(f: const fn(i64) -> i64, x: i64) -> i64 => f(x)\n",
            "const fn choose(f: const fn(i64) -> i64, x: i64) -> i64 => f(x)\n",
            "fn choose(f: fn(i64) -> i64, x: i64) -> str => \"temporal\"\n",
            "fn g(tick: i64) -> str {\n",
            "    let a: str = choose(fn(n) => apply_value(fn(k) => doubled(tick), n), 1)\n",
            "    let b: str = choose(fn(n) => apply_value(fn(k) => gdoubled(tick), n), 1)\n",
            "    let c: str = choose(fn(n) => pair_const_cb(fn(k) => tick, n), 1)\n",
            "    return a\n",
            "}\n",
        ));
        assert!(diagnostics.as_slice().is_empty(), "{diagnostics:?}");
    }

    /// FN-4: a lexical local whose type changes between an outer callback's
    /// probe and committed read still invalidates the inner pair's memo.
    #[test]
    fn phased_pairs_invalidate_when_lexical_local_type_changes() {
        let (diagnostics, roots, probes) = check_with_phase_count(
            "const fn pick(value: i64) -> i64 => value
             fn pick(value: i64) -> str => \"s\"
             const fn quote(price: const fn(i64) -> i64, quantity: i64) -> i64 => price(quantity)
             fn quote(price: fn(i64) -> i64, quantity: i64) -> i64 => price(quantity)
             const fn accept(x: i64) -> i64 => x
             fn accept(x: str) -> i64 => 1
             fn receipt(rate: i64) -> i64 =>
                 quote(fn(quantity) => { let v = pick(quantity)\n rate + accept(v) }, 2)",
        );
        assert!(diagnostics.as_slice().is_empty(), "{diagnostics:?}");
        // `pick(quantity)` and `accept(v)` each re-probe once when `quantity`
        // flips from `Value` (probe) to `Connection` (commit, changing `v: i64 -> str`).
        assert_eq!((roots, probes), (1, 5));
    }
}
