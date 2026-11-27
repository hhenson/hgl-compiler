//! Ordinary const parameters read by a compile-time check (FN-1, FMT-3).
//!
//! A const parameter can also be read at run time. Only a check that
//! needs its value during analysis adds its slot to the instance inputs.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::hash::Hash;
use std::rc::Rc;

use hgl_diagnostics::{Diagnostics, Location};
use petgraph::algo::kosaraju_scc;
use petgraph::graph::DiGraph;

use crate::formatting;
use crate::hir::{
    Answer, Argument, BinaryOperator, Callee, Expr, ExprKind, Form, LocalId, PackInput, Stmt,
    Value, ValueCategory,
};
use crate::{
    Argument as GenericArgument, Binding, ConstantValue, Detail, Phase, Program, Role, Scalar,
    Signature, Type,
};

type Declaration = (usize, usize);
type Slot = (Declaration, usize);
/// Const-slot dependency edges in a declaration or type-instance call graph.
type StoreUses<N> = HashMap<(N, usize), Vec<((N, usize), Demand)>>;
pub(crate) type Demands = HashMap<Declaration, BTreeMap<usize, Demand>>;

/// The `const` slots of a cycle whose key can differ between its calls
/// (TIME-8: "Where a function calls itself, directly or through other
/// functions, and the key can differ between the calls on the way round
/// ... the call that uses it fails with that code as it is evaluated"):
/// those its calls change, exchange or give anew. "A key handed round
/// unchanged is as fixed as in a function that does not call itself."
pub(crate) type ChangingSlots = HashSet<Slot>;

/// Store proofs are keyed without key values, so changing strings never
/// create more instances. `None` also guards a proof currently being read.
pub(crate) type StoreRoutes =
    RefCell<HashMap<(String, Option<crate::TestCandidateScope>), Option<StoreRoute>>>;

#[derive(Clone, Debug)]
pub(crate) struct StoreRoute {
    fixed: HashSet<usize>,
    pub(crate) changing: HashSet<usize>,
}

/// Lexical provenance survives when an instance replaces a parameter read
/// with a literal at the original read's source location.
#[derive(Clone, Debug, Default)]
pub(crate) struct StoreInputContext {
    parameters: HashSet<Location>,
    fixed: HashSet<Location>,
}

/// Restores the outer store proof even when instance checking stops early.
pub(crate) struct StoreInputScope<'a> {
    contexts: &'a RefCell<Vec<StoreInputContext>>,
    depth: usize,
}

impl Drop for StoreInputScope<'_> {
    fn drop(&mut self) {
        self.contexts.borrow_mut().truncate(self.depth);
    }
}

/// Selected value calls or the resolved demand for a generic instance and slot.
pub(crate) type SelectedDemands = RefCell<HashMap<InstanceSlot, SelectedDemand>>;

type InstanceSlot = (String, Option<crate::TestCandidateScope>, usize);

/// A slot followed into an instance: its key, the index of the slot whose
/// instance made the call, and the instance's declaration.
type FollowedSlot = (InstanceSlot, Option<usize>, Declaration);

/// A call to follow: its callee, the entries it gives a pack, and the slot.
type FollowedCall = (Callee, Option<Vec<PackInput>>, usize);

#[derive(Clone, Debug)]
pub(crate) enum SelectedDemand {
    Read(bool),
    Calls(Rc<[FollowedCall]>),
}

/// A deferred operator can select a body that only uses this value at run time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Demand {
    /// A key may be compared during source checking when its value is known.
    StoreKey,
    IfSelected,
    Always,
}

/// A known ordinary const input that participates in the instance identity.
#[derive(Clone, Debug)]
pub struct ConstInput {
    /// The parameter's position in its declaration, including configuration.
    pub slot: usize,
    /// The normalized value, independent of the caller's spelling or location.
    pub(crate) value: Expr,
    /// The identity of the value for both analyser and lowering keys.
    pub key: String,
    /// This call's store route proves this input fixed, independently of
    /// other calls of the same declaration.
    pub(crate) store_fixed: bool,
}

/// Two inputs are one where their slot and value are (GEN-13: "each value
/// one constant"); where each was written does not tell them apart.
impl PartialEq for ConstInput {
    fn eq(&self, other: &Self) -> bool {
        self.slot == other.slot && self.key == other.key && self.store_fixed == other.store_fixed
    }
}

/// The values analysis can use, and whether reading another hit its budget.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ReadInputs {
    pub(crate) values: Vec<ConstInput>,
    pub(crate) limit: Option<ConstInputFault>,
    /// A text the instance reads passed the budget, a conditional one
    /// included: a walk that makes the instance ends there (GEN-13).
    /// Lowering, which needs only `limit`, still writes the call.
    pub(crate) outgrown: bool,
}

/// Why a call's compile-time inputs could not be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConstInputFault {
    /// The compiler cannot evaluate a required value here.
    Unread,
    /// Reading a required value exceeded the reader's traversal budget.
    ReaderLimit,
    /// A required value's text exceeded the instance argument budget.
    TextLimit,
}

/// Match the size bound used for a growing generic value argument (GEN-13).
const INPUT_BUDGET: usize = 1 << 16;
const INPUT_DEPTH: usize = 64;

/// Read a string for a source check without requiring the target to know it.
pub(crate) fn known_store_key(program: &Program, expression: &Expr) -> Option<String> {
    if program.runtime_store_keys.contains(&expression.at)
        && !program.proven_store_input(expression)
    {
        return None;
    }
    if let ExprKind::Literal(Value::Str(text)) = &expression.kind {
        return Some(text.clone());
    }
    let mut reader = Reader::new(program);
    reader.read_string_helpers = false;
    let read = reader.read(expression, INPUT_DEPTH, &[]).ok()?;
    match read.kind {
        ExprKind::Literal(Value::Str(text)) => Some(text),
        _ => None,
    }
}

impl Program {
    /// Keep store-only input provenance local to this making call's check.
    pub(crate) fn store_input_scope(
        &self,
        (module, declaration): Declaration,
        inputs: &[ConstInput],
    ) -> StoreInputScope<'_> {
        let mut context = StoreInputContext::default();
        if let Some(Detail::Callable(signature)) = self.modules[module].details.get(declaration) {
            context
                .fixed
                .extend(
                    inputs
                        .iter()
                        .filter(|input| input.store_fixed)
                        .filter_map(|input| {
                            signature
                                .parameters
                                .get(input.slot)
                                .map(|parameter| parameter.at)
                        }),
                );
            if !context.fixed.is_empty() {
                context.parameters.extend(
                    signature
                        .parameters
                        .iter()
                        .filter(|p| p.constant)
                        .map(|p| p.at),
                );
            }
        }
        let mut contexts = self.store_input_contexts.borrow_mut();
        let depth = contexts.len();
        contexts.push(context);
        StoreInputScope {
            contexts: &self.store_input_contexts,
            depth,
        }
    }

    /// Every parameter this reified expression reads is fixed on this route.
    fn proven_store_input(&self, value: &Expr) -> bool {
        let contexts = self.store_input_contexts.borrow();
        let Some(context) = contexts.last().filter(|context| !context.fixed.is_empty()) else {
            return false;
        };
        let mut found = false;
        let mut fixed = true;
        value.walk(&mut |part| {
            if let Some(parameter) = self
                .store_bindings()
                .get(&(part.at.file, part.at.span.start))
                && context.parameters.contains(parameter)
            {
                found = true;
                fixed &= context.fixed.contains(parameter);
            }
        });
        found && fixed
    }

    pub(crate) fn store_bindings(&self) -> &HashMap<(hgl_source::FileId, u32), Location> {
        self.store_input_bindings
            .get_or_init(|| self.modules.iter().flat_map(local_bindings).collect())
    }

    /// Whether a compile-time check in this declaration needs const inputs.
    pub fn needs_const_inputs(&self, declaration: Declaration) -> bool {
        self.const_demands
            .get(&declaration)
            .is_some_and(|slots| slots.values().any(|demand| *demand != Demand::StoreKey))
    }

    /// The const inputs a resolved call must have to check its instance.
    /// The ordinary defaults come from the selected declaration or its operator.
    pub fn call_const_inputs(
        &self,
        callee: &Callee,
        arguments: &[Argument],
    ) -> Result<Vec<ConstInput>, ConstInputFault> {
        let known = self.read_const_inputs(callee, arguments, false);
        if let Some(limit) = known.limit {
            return Err(limit);
        }
        let demands = callee
            .declaration()
            .and_then(|declaration| self.const_demands.get(&declaration));
        if demands.is_some_and(|demands| {
            demands.iter().any(|(&slot, &demand)| {
                demand == Demand::Always && !known.values.iter().any(|input| input.slot == slot)
            })
        }) {
            return Err(ConstInputFault::Unread);
        }
        // A deferred call only needs the conditional value where its
        // selected candidate reads it. That candidate checks its own inputs.
        Ok(known.values)
    }

    /// Analysis can check the known inputs even when another input remains
    /// unread. Lowering requires the complete set through `call_const_inputs`.
    pub(crate) fn known_const_inputs(&self, callee: &Callee, arguments: &[Argument]) -> ReadInputs {
        self.read_const_inputs(callee, arguments, true)
    }

    fn read_const_inputs(
        &self,
        callee: &Callee,
        arguments: &[Argument],
        check_store: bool,
    ) -> ReadInputs {
        use ConstInputFault::{ReaderLimit, TextLimit, Unread};

        let Some((module, declaration)) = callee.declaration() else {
            return ReadInputs::default();
        };
        let Some(demand) = self.const_demands.get(&(module, declaration)) else {
            return ReadInputs::default();
        };
        let owner = &self.modules[module];
        let Some(Detail::Callable(signature)) = owner.details.get(declaration) else {
            return ReadInputs::default();
        };
        let mut reader = Reader::new(self);
        let mut known = ReadInputs::default();
        // FN-19: the entries a call gives a pack decide which calls a
        // forwarding body makes, and so which of its consts they read.
        let pack_inputs = pack_inputs_of(self, callee, signature, arguments);
        let mut store_route = (check_store
            && (demand.values().any(|d| *d == Demand::StoreKey)
                || self
                    .changing_store_slots
                    .iter()
                    .any(|(owner, _)| *owner == (module, declaration))))
        .then(|| self.store_route(callee, arguments))
        .flatten();
        let mut ordered: Vec<_> = demand
            .iter()
            .filter_map(|(&slot, &demand)| {
                if demand == Demand::StoreKey && !check_store {
                    return None;
                }
                // CAP-6/TIME-8: conditional discovery can supersede StoreKey
                // in the declaration's demand map. A candidate that does not
                // read this value for formatting must still retain a separate
                // fixed-store proof, using the store reader's staging rules.
                let demand = if demand == Demand::IfSelected
                    && !self.instance_reads_const(callee, pack_inputs.clone(), slot)
                {
                    if !check_store {
                        return None;
                    }
                    if store_route.is_none() {
                        store_route = self.store_route(callee, arguments);
                    }
                    if !store_route
                        .as_ref()
                        .is_some_and(|route| route.fixed.contains(&slot))
                    {
                        return None;
                    }
                    Demand::StoreKey
                } else {
                    demand
                };
                Some((slot, demand))
            })
            .collect();
        // GEN-13: classify recovered store-only inputs before spending the
        // shared budget, so genuine formatting inputs still precede them.
        ordered.sort_by_key(|&(slot, demand)| (std::cmp::Reverse(demand), slot));
        for (slot, demand) in ordered {
            if demand == Demand::StoreKey
                && store_route.as_ref().map_or_else(
                    || {
                        self.changing_store_slots
                            .contains(&((module, declaration), slot))
                    },
                    |route| !route.fixed.contains(&slot),
                )
            {
                continue;
            }
            // CAP-6: a value function call that computes a key belongs to
            // graph construction, even when another check could read it.
            reader.read_string_helpers = demand != Demand::StoreKey;
            let text_before = reader.text_left;
            let input = (|| {
                let parameter = signature.parameters.get(slot).ok_or(Unread)?;
                let given = arguments
                    .iter()
                    .find(|argument| argument.parameter == Some(slot));
                let mut generics: Vec<_> = signature
                    .generics
                    .iter()
                    .zip(callee.generics())
                    .map(|(parameter, value)| (parameter.at, value))
                    .collect();
                let value = if let Some(given) = given {
                    // TIME-8: a key a cycle changes is the run's, on this route.
                    if demand == Demand::StoreKey
                        && self.runtime_store_keys.contains(&given.value.at)
                        && !self.proven_store_input(&given.value)
                    {
                        return Err(Unread);
                    }
                    reader.read(&given.value, INPUT_DEPTH, &[])?
                } else if let Some(default) = self.inherited_configuration_default(callee, slot) {
                    if let Callee::Candidate {
                        operator,
                        operator_generics,
                        ..
                    } = callee
                        && let Some(Detail::Callable(public)) =
                            self.modules[operator.0].details.get(operator.1)
                    {
                        generics = public
                            .generics
                            .iter()
                            .zip(operator_generics)
                            .map(|(parameter, value)| (parameter.at, value))
                            .collect();
                    }
                    reader.read(&default.value, INPUT_DEPTH, &generics)?
                } else if let Some(default) = owner
                    .defaults
                    .get(declaration)
                    .and_then(|defaults| defaults.get(slot))
                    .and_then(Option::as_ref)
                {
                    reader.read(&default.value, INPUT_DEPTH, &generics)?
                } else if matches!(parameter.ty, Type::Nullable(_)) {
                    Expr {
                        kind: ExprKind::Literal(Value::Null),
                        ty: parameter.ty.clone(),
                        at: parameter.at,
                        category: ValueCategory::Value,
                    }
                } else {
                    return Err(Unread);
                };
                let mut key = String::new();
                key_value(&value, &mut key)?;
                // A string already spent its text budget before key framing.
                // Keep a separate bound on composite and scalar key metadata.
                if key.len() > INPUT_BUDGET
                    && !matches!(value.kind, ExprKind::Literal(Value::Str(_)))
                {
                    return Err(ReaderLimit);
                }
                let store_fixed = store_route
                    .as_ref()
                    .is_some_and(|route| route.fixed.contains(&slot));
                if store_fixed {
                    // Source instance identity includes the proof, not just the
                    // string: another route may read that string only at runtime.
                    key.insert_str(0, "fixed-store:");
                }
                Ok(ConstInput {
                    slot,
                    value,
                    key,
                    store_fixed,
                })
            })();
            match input {
                Ok(input) => known.values.push(input),
                Err(fault) => {
                    if demand == Demand::Always
                        && (fault == TextLimit || (fault == ReaderLimit && known.limit.is_none()))
                    {
                        known.limit = Some(fault);
                    }
                    // GEN-13: a text the instance reads that outgrows the
                    // budget ends a walk, a conditional one included once
                    // its instance is known to read it.
                    known.outgrown |= demand != Demand::StoreKey && fault == TextLimit;
                    reader.text_left = text_before;
                }
            }
        }
        known
    }

    /// Follow a possible demand with the instance's types in place and its
    /// ordinary consts still unread. A forwarded value only enters the key
    /// if a selected candidate reads it, or selection remains unresolved.
    /// A call that gives packs is followed with their entries (FN-19), so
    /// the calls its forwarding body makes are the instance's own.
    fn instance_reads_const(
        &self,
        callee: &Callee,
        pack_inputs: Option<Vec<PackInput>>,
        slot: usize,
    ) -> bool {
        let mut pending = VecDeque::from([(callee.clone(), pack_inputs, slot, None)]);
        let mut followed = HashMap::<Declaration, usize>::new();
        let mut seen = HashSet::new();
        // Parent indices record the path that proves a slot is read.
        let mut paths: Vec<FollowedSlot> = Vec::new();
        while let Some((callee, pack_inputs, slot, parent)) = pending.pop_front() {
            let (Callee::Candidate {
                module,
                declaration,
                requirements,
                ..
            }
            | Callee::Function {
                module,
                declaration,
                requirements,
                ..
            }) = &callee
            else {
                self.remember_read(parent, &paths);
                return true;
            };
            let declaration = (*module, *declaration);
            let Some(key) = crate::values::bounded_instance_key(
                declaration,
                callee.generics(),
                callee.type_packs(),
                requirements,
            ) else {
                // The instance walk will report its own limit.
                self.remember_read(parent, &paths);
                return true;
            };
            let key = match &pack_inputs {
                Some(entries) => format!("{key}entries{entries:?}"),
                None => key,
            };
            let key = (key, callee.test_candidates(), slot);
            let cached = self.selected_const_demands.borrow().get(&key).cloned();
            if let Some(SelectedDemand::Read(read)) = cached {
                if read {
                    self.remember_read(parent, &paths);
                    return true;
                }
                continue;
            }
            if !seen.insert(key.clone()) {
                continue;
            }
            // GEN-13 and FN-19: a forwarding body whose declaration comes
            // back on its own path with other entries may grow its packs
            // without end. Its slot is taken as read there, as a forwarding
            // call left unread is, and the instance walk bounds the growth.
            if pack_inputs.is_some()
                && std::iter::successors(parent, |&index| paths[index].1)
                    .any(|index| paths[index].2 == declaration)
            {
                self.remember_read(parent, &paths);
                return true;
            }
            let count = followed.entry(declaration).or_default();
            *count += 1;
            if *count > crate::INSTANCES {
                // Only the root used this traversal's full budget.
                let (root, _, _) = &paths[0];
                self.selected_const_demands
                    .borrow_mut()
                    .insert(root.clone(), SelectedDemand::Read(true));
                return true;
            }
            let direct = cached.unwrap_or_else(|| {
                let direct = self.selected_const_calls(
                    &callee,
                    declaration,
                    requirements,
                    pack_inputs.as_deref(),
                    slot,
                );
                self.selected_const_demands
                    .borrow_mut()
                    .insert(key.clone(), direct.clone());
                direct
            });
            match direct {
                SelectedDemand::Read(true) => {
                    self.remember_read(parent, &paths);
                    return true;
                }
                SelectedDemand::Read(false) => {}
                SelectedDemand::Calls(calls) => {
                    let index = paths.len();
                    paths.push((key, parent, declaration));
                    pending.extend(
                        calls
                            .iter()
                            .cloned()
                            .map(|(callee, entries, slot)| (callee, entries, slot, Some(index))),
                    );
                }
            }
        }
        // No path reached a read, including paths that only cycle.
        self.selected_const_demands.borrow_mut().extend(
            paths
                .into_iter()
                .map(|(key, _, _)| (key, SelectedDemand::Read(false))),
        );
        false
    }

    fn remember_read(&self, mut parent: Option<usize>, paths: &[FollowedSlot]) {
        let mut cache = self.selected_const_demands.borrow_mut();
        while let Some(index) = parent {
            let (key, earlier, _) = &paths[index];
            cache.insert(key.clone(), SelectedDemand::Read(true));
            parent = *earlier;
        }
    }

    /// Read one concrete body for this slot and keep its direct demand edges.
    fn selected_const_calls(
        &self,
        callee: &Callee,
        (module, declaration): Declaration,
        requirements: &[Answer],
        pack_inputs: Option<&[PackInput]>,
        slot: usize,
    ) -> SelectedDemand {
        let unit = &self.modules[module];
        let Some(Detail::Callable(signature)) = unit.details.get(declaration) else {
            return SelectedDemand::Read(true);
        };
        let Some(parameter) = signature.parameters.get(slot) else {
            return SelectedDemand::Read(true);
        };
        let placement_reports = RefCell::new(Diagnostics::new());
        let mut body_reports = Diagnostics::new();
        let Some(instance) = self.checked_instance(
            (module, declaration),
            callee.generics(),
            requirements,
            crate::InstanceInputs {
                type_packs: callee.type_packs(),
                pack_inputs,
                test_candidates: callee.test_candidates(),
                ..crate::InstanceInputs::default()
            },
            &placement_reports,
            &mut body_reports,
        ) else {
            return SelectedDemand::Read(true);
        };
        if body_reports.has_errors() || placement_reports.borrow().has_errors() {
            return SelectedDemand::Read(true);
        }
        let bindings = local_bindings(unit);
        let slots = HashMap::from([(parameter.at, slot)]);
        let reads_parameter = |argument: &Expr| {
            let mut reads = false;
            slots_read(argument, &bindings, &slots, |_| reads = true);
            reads
        };
        let mut calls = Vec::new();
        let mut required = false;
        instance.walk_calls(&mut |call| {
            let ExprKind::Call {
                callee, arguments, ..
            } = &call.kind
            else {
                return;
            };
            if required {
                return;
            }
            if let Some(signature) = formatting::value_signature(self, callee) {
                required = formatting::argument(signature, arguments, "fmt_str")
                    .is_some_and(|pattern| reads_parameter(&pattern.value));
                return;
            }
            // A forwarding call left unread says nothing of its callee.
            if matches!(callee, Callee::DeferredPackCall) {
                required = true;
                return;
            }
            for argument in arguments
                .iter()
                .filter(|argument| reads_parameter(&argument.value))
            {
                // FN-17: what a function parameter is given goes to a plain
                // parameter of its template, read when the function runs.
                if plain_callback_input(&instance.signature, callee, argument.parameter) {
                    continue;
                }
                let (Some(declaration), Some(slot)) = (callee.declaration(), argument.parameter)
                else {
                    required = true;
                    break;
                };
                match self
                    .const_demands
                    .get(&declaration)
                    .and_then(|slots| slots.get(&slot))
                {
                    Some(Demand::Always) => {
                        required = true;
                        break;
                    }
                    Some(Demand::IfSelected) => {
                        let entries = match &self.modules[declaration.0].details[declaration.1] {
                            Detail::Callable(called) => {
                                pack_inputs_of(self, callee, called, arguments)
                            }
                            _ => None,
                        };
                        calls.push((callee.clone(), entries, slot));
                    }
                    Some(Demand::StoreKey) | None => {}
                }
            }
        });
        if required || calls.is_empty() {
            SelectedDemand::Read(required)
        } else {
            SelectedDemand::Calls(calls.into())
        }
    }
}

struct Reader<'a> {
    program: &'a Program,
    /// Present only when reading a reached immediate expression.
    locals: Option<&'a [(LocalId, Rc<Expr>)]>,
    steps_left: usize,
    text_left: usize,
    reading: Vec<(Declaration, Option<usize>)>,
    read_string_helpers: bool,
}

/// Read a fixed projection for a check that needs its value (FN-1).
pub(crate) fn projection_value(
    program: &Program,
    expression: &Expr,
    generics: &[(Location, GenericArgument)],
) -> Result<Expr, ConstInputFault> {
    let generics: Vec<_> = generics
        .iter()
        .map(|(at, argument)| (*at, argument))
        .collect();
    Reader::new(program).read(expression, INPUT_DEPTH, &generics)
}

/// Normalize a known value in an immediate anonymous call's lexical scope.
/// Preserve the usual builtin operation and reader budget checks.
pub(crate) fn immediate_value(
    program: &Program,
    expression: &Expr,
    generics: &[(Location, GenericArgument)],
    locals: &[(LocalId, Rc<Expr>)],
) -> Result<Expr, ConstInputFault> {
    let generics: Vec<_> = generics
        .iter()
        .map(|(at, argument)| (*at, argument))
        .collect();
    let mut reader = Reader::new(program);
    reader.locals = Some(locals);
    reader.read_string_helpers = false;
    reader.read(expression, INPUT_DEPTH, &generics)
}

impl<'a> Reader<'a> {
    fn new(program: &'a Program) -> Self {
        Self {
            program,
            locals: None,
            steps_left: INPUT_BUDGET,
            text_left: INPUT_BUDGET,
            reading: Vec::new(),
            read_string_helpers: true,
        }
    }

    fn read_declaration(
        &mut self,
        declaration: Declaration,
        field: Option<usize>,
        value: &Expr,
        depth: usize,
        generics: &[(Location, &GenericArgument)],
    ) -> Result<Expr, ConstInputFault> {
        let dependency = (declaration, field);
        if self.reading.contains(&dependency) {
            // UNIT-8 and UNIT-13 report these dependency cycles themselves.
            return Err(ConstInputFault::Unread);
        }
        self.reading.push(dependency);
        // A followed declaration has its own lexical local IDs (UNIT-7,
        // UNIT-13). The immediate call's bindings do not belong to it.
        let locals = self.locals;
        self.locals = locals.map(|_| &[][..]);
        let read = self.read(value, depth, generics);
        self.locals = locals;
        self.reading.pop();
        read
    }

    fn read(
        &mut self,
        expression: &Expr,
        depth: usize,
        generics: &[(Location, &GenericArgument)],
    ) -> Result<Expr, ConstInputFault> {
        use ConstInputFault::{ReaderLimit, TextLimit, Unread};

        let next = depth.checked_sub(1).ok_or(ReaderLimit)?;
        self.steps_left = self.steps_left.checked_sub(1).ok_or(ReaderLimit)?;
        let kind = match &expression.kind {
            ExprKind::Literal(value) => {
                if let Value::Str(text) = value {
                    self.text_left = self.text_left.checked_sub(text.len()).ok_or(TextLimit)?;
                }
                ExprKind::Literal(value.clone())
            }
            ExprKind::EnumMember {
                module,
                declaration,
                member,
            } => ExprKind::EnumMember {
                module: *module,
                declaration: *declaration,
                member: *member,
            },
            ExprKind::Local(local) => {
                let locals = self.locals.ok_or(Unread)?;
                let (_, value) = locals.iter().find(|(id, _)| id == local).ok_or(Unread)?;
                return self.read(value, next, generics);
            }
            ExprKind::GenericValue { at, .. } => {
                let (_, GenericArgument::Constant(given)) = generics
                    .iter()
                    .find(|(parameter, _)| parameter == at)
                    .ok_or(Unread)?
                else {
                    return Err(Unread);
                };
                let value = given.value.literal().ok_or(Unread)?;
                if let Value::Str(text) = &value {
                    self.text_left = self.text_left.checked_sub(text.len()).ok_or(TextLimit)?;
                }
                ExprKind::Literal(value)
            }
            ExprKind::Constant {
                module,
                declaration,
            } => {
                let constant = self.program.modules[*module]
                    .constants
                    .get(*declaration)
                    .and_then(Option::as_ref)
                    .ok_or(Unread)?;
                return self.read_declaration(
                    (*module, *declaration),
                    None,
                    &constant.value,
                    next,
                    &[],
                );
            }
            ExprKind::Call {
                callee,
                arguments,
                form: Form::Named,
                ..
            } if self.read_string_helpers
                && arguments.is_empty()
                && expression.ty == Type::Scalar(Scalar::Str) =>
            {
                // Match the existing format reader's single-return string
                // helpers. A general compile-time call is tracked separately.
                let (module, declaration) = callee.declaration().ok_or(Unread)?;
                let owner = &self.program.modules[module];
                if owner.declarations[declaration].kind
                    != crate::DeclarationKind::Function(Role::Value)
                {
                    return Err(Unread);
                }
                let Some(Detail::Callable(signature)) = owner.details.get(declaration) else {
                    return Err(Unread);
                };
                if !signature.generics.is_empty() || !signature.parameters.is_empty() {
                    return Err(Unread);
                }
                let body = owner
                    .bodies
                    .get(declaration)
                    .and_then(Option::as_ref)
                    .ok_or(Unread)?;
                let [
                    Stmt::Return {
                        value: Some(returned),
                        ..
                    },
                ] = body.statements.as_slice()
                else {
                    return Err(Unread);
                };
                return self.read_declaration((module, declaration), None, returned, next, &[]);
            }
            ExprKind::Tuple(parts) | ExprKind::Sequence(parts)
                if matches!(expression.ty, Type::Tuple(_) | Type::List { .. }) =>
            {
                let parts = parts
                    .iter()
                    .map(|part| self.read(part, next, generics))
                    .collect::<Result<Vec<_>, _>>()?;
                if matches!(expression.kind, ExprKind::Tuple(_)) {
                    ExprKind::Tuple(parts)
                } else {
                    ExprKind::Sequence(parts)
                }
            }
            ExprKind::Pairs(entries)
                if self.locals.is_some() && matches!(expression.ty, Type::Dict { .. }) =>
            {
                let entries = entries
                    .iter()
                    .map(|(key, value)| {
                        Ok((
                            self.read(key, next, generics)?,
                            self.read(value, next, generics)?,
                        ))
                    })
                    .collect::<Result<_, _>>()?;
                ExprKind::Pairs(entries)
            }
            ExprKind::Construct {
                module,
                declaration,
                arguments,
                ..
            } => {
                let Some(Detail::Struct(structure)) =
                    self.program.modules[*module].details.get(*declaration)
                else {
                    return Err(Unread);
                };
                let mut fields = Vec::with_capacity(structure.members.len());
                for (position, member) in structure.members.iter().enumerate() {
                    let value = if let Some(given) =
                        arguments.iter().find(|arg| arg.parameter == Some(position))
                    {
                        self.read(&given.value, next, generics)?
                    } else {
                        let default = member.default.ok_or(Unread)?;
                        let value = &self.program.modules[default.module]
                            .defaults
                            .get(default.declaration)
                            .and_then(|fields| fields.get(default.field))
                            .and_then(Option::as_ref)
                            .ok_or(Unread)?
                            .value;
                        self.read_declaration(
                            (default.module, default.declaration),
                            Some(default.field),
                            value,
                            next,
                            generics,
                        )?
                    };
                    fields.push(Argument {
                        parameter: Some(position),
                        pack: None,
                        value,
                    });
                }
                ExprKind::Construct {
                    module: *module,
                    declaration: *declaration,
                    arguments: fields,
                    infer_types: false,
                }
            }
            ExprKind::Field { base, member } => {
                let ExprKind::Construct { arguments, .. } = self.read(base, next, generics)?.kind
                else {
                    return Err(Unread);
                };
                arguments
                    .into_iter()
                    .find(|argument| argument.parameter == Some(*member))
                    .ok_or(Unread)?
                    .value
                    .kind
            }
            ExprKind::Index { base, index } => {
                // The bounded const-input reader normalizes tuples and lists.
                if self.locals.is_some() && matches!(base.ty, Type::Dict { .. }) {
                    self.dictionary_entry(base, index, next, generics)?.kind
                } else {
                    let ConstantValue::Int(position) = self.scalar(index, next, generics)? else {
                        return Err(Unread);
                    };
                    let position = usize::try_from(position).map_err(|_| Unread)?;
                    let (ExprKind::Tuple(entries) | ExprKind::Sequence(entries)) =
                        self.read(base, next, generics)?.kind
                    else {
                        return Err(Unread);
                    };
                    entries.into_iter().nth(position).ok_or(Unread)?.kind
                }
            }
            ExprKind::Widen(inner) => match self.scalar(inner, next, generics)? {
                ConstantValue::Int(value) => ExprKind::Literal(Value::Float(value as f64)),
                _ => return Err(Unread),
            },
            ExprKind::Block {
                statements,
                value: Some(value),
            } if statements.is_empty()
                || self.locals.is_some_and(|known| {
                    statements.iter().all(|statement| {
                        matches!(statement, Stmt::Declare { local, value: Some(_) }
                        if known.iter().any(|(id, _)| id == local))
                    })
                }) =>
            {
                // A known immutable declaration adds no effect to the
                // block's final expression (CTRL-10).
                return self.read(value, next, generics);
            }
            ExprKind::If {
                condition,
                then,
                otherwise,
                ..
            } => {
                let branch = match self.scalar(condition, next, generics)? {
                    ConstantValue::Bool(true) => then,
                    ConstantValue::Bool(false) => otherwise.as_ref().ok_or(Unread)?,
                    _ => return Err(Unread),
                };
                return self.read(branch, next, generics);
            }
            _ => return self.operation(expression, next, generics),
        };
        Ok(Expr {
            kind,
            at: expression.at,
            ty: expression.ty.clone(),
            category: ValueCategory::Value,
        })
    }

    fn scalar(
        &mut self,
        value: &Expr,
        depth: usize,
        generics: &[(Location, &GenericArgument)],
    ) -> Result<ConstantValue, ConstInputFault> {
        match self.read(value, depth, generics)?.kind {
            ExprKind::Literal(literal) => Ok(literal.into()),
            _ => Err(ConstInputFault::Unread),
        }
    }

    /// A known ordinary dictionary supplies its last entry for a key
    /// (EXPR-7, EXPR-9). Missing or unknown keys supply no fixed value.
    fn dictionary_entry(
        &mut self,
        base: &Expr,
        index: &Expr,
        depth: usize,
        generics: &[(Location, &GenericArgument)],
    ) -> Result<Expr, ConstInputFault> {
        use ConstInputFault::Unread;

        let ExprKind::Pairs(entries) = self.read(base, depth, generics)?.kind else {
            return Err(Unread);
        };
        let index = self.read(index, depth, generics)?;
        if index.ty == Type::Scalar(Scalar::Any) {
            return Err(Unread);
        }
        let mut wanted = String::new();
        key_value(&index, &mut wanted)?;
        let mut read = String::new();
        for (key, value) in entries.into_iter().rev() {
            if key.ty == index.ty {
                read.clear();
                key_value(&key, &mut read)?;
                if read == wanted {
                    return Ok(value);
                }
            }
        }
        Err(Unread)
    }

    fn operation(
        &mut self,
        value: &Expr,
        depth: usize,
        generics: &[(Location, &GenericArgument)],
    ) -> Result<Expr, ConstInputFault> {
        use ConstInputFault::Unread;

        if !builtin_operation(self.program, value) {
            return Err(Unread);
        }
        let calculated = if value.ty == Type::Scalar(Scalar::Str)
            && matches!(value.kind.binary(), Some((BinaryOperator::Add, _, _)))
        {
            ConstantValue::Str(self.concatenation(value, depth, generics)?)
        } else if let Some((operator, left, right)) = value.kind.binary() {
            let left = self.scalar(left, depth, generics)?;
            match (&left, operator) {
                (ConstantValue::Bool(false), BinaryOperator::And)
                | (ConstantValue::Bool(true), BinaryOperator::Or) => left,
                _ => crate::constant::combined(
                    operator,
                    &left,
                    &self.scalar(right, depth, generics)?,
                ),
            }
        } else if let Some((operator, operand)) = value.kind.unary() {
            crate::constant::negated_or_not(operator, &self.scalar(operand, depth, generics)?)
        } else {
            return Err(Unread);
        };
        Ok(Expr {
            kind: ExprKind::Literal(calculated.literal().ok_or(Unread)?),
            at: value.at,
            ty: value.ty.clone(),
            category: ValueCategory::Value,
        })
    }

    /// The components of one joined pattern share the same depth budget.
    fn concatenation(
        &mut self,
        value: &Expr,
        depth: usize,
        generics: &[(Location, &GenericArgument)],
    ) -> Result<String, ConstInputFault> {
        let mut text = String::new();
        let mut pending = vec![value];
        while let Some(piece) = pending.pop() {
            if piece.ty == Type::Scalar(Scalar::Str)
                && builtin_operation(self.program, piece)
                && let Some((BinaryOperator::Add, left, right)) = piece.kind.binary()
            {
                self.steps_left = self
                    .steps_left
                    .checked_sub(1)
                    .ok_or(ConstInputFault::ReaderLimit)?;
                pending.extend([right, left]);
            } else {
                let ConstantValue::Str(piece) = self.scalar(piece, depth, generics)? else {
                    return Err(ConstInputFault::Unread);
                };
                text.push_str(&piece);
            }
        }
        Ok(text)
    }
}

/// A user's named candidate can run value code. Fold only the checked
/// builtin arithmetic, with no opaque configuration operands.
pub(crate) fn builtin_operation(program: &Program, value: &Expr) -> bool {
    match &value.kind {
        ExprKind::Binary { .. } | ExprKind::Unary { .. } => true,
        ExprKind::Call {
            callee,
            form,
            arguments,
            ..
        } => {
            let arity = match form {
                Form::Binary(_) => 2,
                Form::Unary(_) => 1,
                Form::Named => return false,
            };
            arguments.len() == arity
                && callee.declaration().is_some_and(|(module, declaration)| {
                    program.core() == Some(module)
                        && matches!(
                            program.modules[module].declarations[declaration].kind,
                            crate::DeclarationKind::NativeCandidate(Role::Value, _)
                        )
                })
        }
        _ => false,
    }
}

/// Write keys from normalized values. Brackets frame lists, tuples and
/// named structures without repeatedly escaping a nested part's text.
fn key_value(value: &Expr, output: &mut String) -> Result<(), ConstInputFault> {
    use ConstInputFault::Unread;
    use std::fmt::Write;

    let parts = match &value.kind {
        ExprKind::Literal(Value::Str(text)) => {
            let _ = write!(output, "str:{}:", text.len());
            output.push_str(text);
            return Ok(());
        }
        ExprKind::Literal(Value::Float(value)) => {
            let _ = write!(output, "f64:{:016x}", value.to_bits());
            return Ok(());
        }
        ExprKind::Literal(value) => {
            output.push_str(&ConstantValue::from(value.clone()).key().ok_or(Unread)?);
            return Ok(());
        }
        ExprKind::EnumMember {
            module,
            declaration,
            member,
        } => {
            let _ = write!(output, "enum({module},{declaration},{member})");
            return Ok(());
        }
        ExprKind::Tuple(parts) => {
            output.push_str("tuple[");
            parts
        }
        ExprKind::Sequence(parts) => {
            output.push_str("list[");
            parts
        }
        ExprKind::Construct {
            module,
            declaration,
            arguments,
            ..
        } => {
            let _ = write!(output, "struct({module},{declaration})[");
            for argument in arguments {
                key_value(&argument.value, output)?;
                output.push(',');
            }
            output.push(']');
            return Ok(());
        }
        _ => return Err(Unread),
    };
    for part in parts {
        key_value(part, output)?;
        output.push(',');
    }
    output.push(']');
    Ok(())
}

/// Follow demand for a compile-time value back through its call arguments.
pub(crate) fn demands(
    program: &Program,
) -> (
    Demands,
    HashSet<Location>,
    HashSet<Declaration>,
    ChangingSlots,
) {
    let mut required = Demands::new();
    let mut uses: HashMap<Slot, Vec<(Slot, Demand)>> = HashMap::new();
    let mut pending = VecDeque::new();
    let mut candidates: HashMap<Declaration, Vec<Declaration>> = HashMap::new();
    let mut store_reads: HashMap<Location, Vec<Slot>> = HashMap::new();
    // The arguments of calls that hand the store on, and the slots each reads.
    let mut store_arguments: HashMap<Location, Vec<Slot>> = HashMap::new();
    let mut generic_reads = HashSet::new();
    let mut generic_uses: HashMap<Slot, Vec<Declaration>> = HashMap::new();
    let mut store_calls = StoreCalls::default();
    let store = program.capability_type("global_state");
    let borrows_store = |signature: &Signature| {
        store.as_ref().is_some_and(|store| {
            signature
                .parameters
                .iter()
                .any(|parameter| &parameter.ty == store)
        })
    };
    let is_graph = |(module, declaration): Declaration| {
        program.modules[module].phases.get(declaration) == Some(&Some(Phase::Graph))
    };
    for (module, unit) in program.modules.iter().enumerate() {
        for (declaration, declared) in unit.declarations.iter().enumerate() {
            if let Some((Role::Value, Some(operator))) = declared.kind.candidate() {
                candidates
                    .entry((operator.module, operator.declaration))
                    .or_default()
                    .push((module, declaration));
            }
        }
    }
    for (module_index, module) in program.modules.iter().enumerate() {
        let bindings = local_bindings(module);
        for (declaration, body) in module.bodies.iter().enumerate() {
            let (Some(body), Some(Detail::Callable(signature))) =
                (body, module.details.get(declaration))
            else {
                continue;
            };
            let slots: HashMap<Location, usize> = signature
                .parameters
                .iter()
                .enumerate()
                .filter(|(_, parameter)| parameter.constant)
                .map(|(position, parameter)| (parameter.at, position))
                .collect();
            let caller = (module_index, declaration);
            let uses_store = borrows_store(signature) || is_graph(caller);
            if slots.is_empty() && !uses_store && signature.generics.is_empty() {
                continue;
            }
            let mut read_compile_time_value = |value: &Expr, demand: Demand| {
                if crate::values::fixed_expression(value, &|read| {
                    matches!(read.kind, ExprKind::Local(_))
                        && bindings
                            .get(&(read.at.file, read.at.span.start))
                            .is_some_and(|at| slots.contains_key(at))
                }) {
                    if demand == Demand::StoreKey && names_generic_value(value) {
                        generic_reads.insert(caller);
                    }
                    slots_read(value, &bindings, &slots, |slot| {
                        pending.push_back(((caller, slot), demand));
                        if demand == Demand::StoreKey {
                            store_reads
                                .entry(value.at)
                                .or_default()
                                .push((caller, slot));
                        }
                    });
                }
            };
            let mut deferred_at = HashSet::new();
            for selection in &module.selecting[declaration] {
                if let Some(key) = crate::store_keys::deferred_key(program, selection) {
                    read_compile_time_value(key, Demand::StoreKey);
                }
                if selection.graph {
                    continue;
                }
                deferred_at.insert(selection.at);
                if let Some(pattern) = formatting::deferred_pattern(program, selection) {
                    read_compile_time_value(pattern, Demand::Always);
                    continue;
                }
                let operator = selection.operator;
                let Some(alternatives) = candidates.get(&(operator.module, operator.declaration))
                else {
                    continue;
                };
                for &(unit, member) in alternatives {
                    let candidate = &program.modules[unit];
                    if let Some(test) = candidate.declarations[member].test_owner
                        && (unit != module_index
                            || module.declarations[declaration].test_owner != Some(test))
                    {
                        continue;
                    }
                    let Some(Detail::Callable(parameters)) = candidate.details.get(member) else {
                        continue;
                    };
                    // TIME-8: a call an instance resolves reaches each
                    // candidate it can select, so a cycle closed through
                    // the operator is a cycle.
                    if uses_store && (borrows_store(parameters) || is_graph((unit, member))) {
                        let argument_for =
                            |position: usize| selection.argument_for(parameters, position);
                        store_calls.record(
                            program,
                            caller,
                            signature,
                            (unit, member),
                            parameters,
                            argument_for,
                        );
                        for position in 0..parameters.parameters.len() {
                            if let Some(argument) = argument_for(position) {
                                slots_read(argument, &bindings, &slots, |slot| {
                                    store_arguments
                                        .entry(argument.at)
                                        .or_default()
                                        .push((caller, slot));
                                });
                            }
                        }
                    }
                    for (position, _) in parameters
                        .parameters
                        .iter()
                        .enumerate()
                        .filter(|(_, parameter)| parameter.constant)
                    {
                        if let Some(argument) = selection.argument_for(parameters, position) {
                            slots_read(argument, &bindings, &slots, |slot| {
                                uses.entry(((unit, member), position))
                                    .or_default()
                                    .push(((caller, slot), Demand::IfSelected));
                            });
                        }
                    }
                }
            }
            let mut forwarding = Vec::new();
            crate::values::walk_body_calls(body, &mut |call| {
                // A provisional candidate does not make its demands unconditional.
                if deferred_at.contains(&call.at) {
                    return;
                }
                let ExprKind::Call {
                    callee, arguments, ..
                } = &call.kind
                else {
                    return;
                };
                // FN-19: an indexed pack entry can leave the entire call
                // unread, before it has a provisional operator selection.
                let indexed_pack_call = matches!(callee, Callee::Unread | Callee::DeferredValue)
                    && arguments.iter().any(|argument| {
                        let mut deferred = false;
                        argument.value.walk(&mut |value| {
                            deferred |= matches!(value.kind, ExprKind::DeferredPackRead);
                        });
                        deferred
                    });
                if indexed_pack_call
                    || matches!(callee, Callee::DeferredPackCall | Callee::Required { .. })
                {
                    forwarding.push(call.at);
                    return;
                }
                let Some(called) = callee.declaration() else {
                    return;
                };
                if uses_store
                    && let Some(Detail::Callable(target)) =
                        program.modules[called.0].details.get(called.1)
                    && (borrows_store(target) || is_graph(called))
                {
                    let argument_for = |slot: usize| {
                        arguments
                            .iter()
                            .find(|argument| argument.parameter == Some(slot))
                            .map(|argument| &argument.value)
                    };
                    store_calls.record(program, caller, signature, called, target, argument_for);
                    for argument in arguments {
                        slots_read(&argument.value, &bindings, &slots, |slot| {
                            store_arguments
                                .entry(argument.value.at)
                                .or_default()
                                .push((caller, slot));
                        });
                    }
                }
                if let Some(signature) = formatting::value_signature(program, callee)
                    && let Some(pattern) = formatting::argument(signature, arguments, "fmt_str")
                {
                    read_compile_time_value(&pattern.value, Demand::Always);
                }
                if let Some(key) = program.store_key_argument(callee, arguments) {
                    read_compile_time_value(key, Demand::StoreKey);
                }
                for argument in arguments {
                    let Some(position) = argument.parameter else {
                        continue;
                    };
                    if !signature.generics.is_empty() && names_generic_value(&argument.value) {
                        generic_uses
                            .entry((called, position))
                            .or_default()
                            .push(caller);
                    }
                    slots_read(&argument.value, &bindings, &slots, |slot| {
                        uses.entry((called, position))
                            .or_default()
                            .push(((caller, slot), Demand::Always));
                    });
                }
            });
            // FN-19 and GEN-7: pack calls and named requirements are read
            // in each instance, where the selected callee
            // and the slot of each written argument are known. A const read
            // anywhere in it is followed there, as a deferred candidate's
            // is, and is an input only where that instance reads it.
            for call in forwarding {
                for (&(file, start), binding) in &bindings {
                    if file == call.file
                        && (call.span.start..call.span.end).contains(&start)
                        && let Some(&slot) = slots.get(binding)
                    {
                        pending.push_back(((caller, slot), Demand::IfSelected));
                    }
                }
            }
        }
    }
    let observed_keys: HashSet<Slot> = store_reads.values().flatten().copied().collect();
    let conservative = store_calls.changed(observed_keys.clone(), &uses, false);
    // Most programs have no changing store cycle. Only refine a possible
    // cycle, and keep its existing runtime classification if reading its
    // type instances is incomplete or reaches the ordinary instance bound.
    let changing = if conservative.is_empty() {
        conservative
    } else {
        resolved_store_changes(program, &store_calls, &conservative, &observed_keys, &uses)
            .unwrap_or(conservative)
    };
    let changes_in_cycle = |slot: &Slot| changing.contains(slot);
    while let Some(((declaration, slot), demand)) = pending.pop_front() {
        if demand == Demand::StoreKey {
            let position = (declaration, slot);
            if !changes_in_cycle(&position)
                && let Some(callers) = generic_uses.get(&position)
            {
                generic_reads.extend(callers.iter().copied());
            }
        }
        let slots = required.entry(declaration).or_default();
        if slots.get(&slot).is_some_and(|earlier| *earlier >= demand) {
            continue;
        }
        slots.insert(slot, demand);
        if let Some(callers) = uses.get(&(declaration, slot)) {
            pending.extend(
                callers
                    .iter()
                    .map(|&(caller, edge)| (caller, demand.min(edge))),
            );
        }
    }
    // A recursive helper can calculate the actual key before using it.
    // A separate compile-time check can still need the initial input:
    // an instance made for it is given no key a cycle changes, at an
    // operation or in the argument that hands it to a helper (TIME-8).
    let runtime_store_keys = store_reads
        .into_iter()
        .chain(store_arguments)
        .filter_map(|(at, slots)| slots.iter().any(changes_in_cycle).then_some(at))
        .collect();
    (required, runtime_store_keys, generic_reads, changing)
}

fn names_generic_value(value: &Expr) -> bool {
    let mut reads = false;
    value.walk(&mut |value| reads |= matches!(value.kind, ExprKind::GenericValue { .. }));
    reads
}

impl Program {
    /// Prove this call's fixed observations using only its reachable concrete
    /// instances. An unrelated entry must not change a key's checking stage.
    pub(crate) fn store_route(
        &self,
        callee: &Callee,
        arguments: &[Argument],
    ) -> Option<StoreRoute> {
        let declaration = callee.declaration()?;
        let Detail::Callable(signature) = &self.modules[declaration.0].details[declaration.1]
        else {
            return None;
        };
        let requirements = match callee {
            Callee::Function { requirements, .. } | Callee::Candidate { requirements, .. } => {
                requirements
            }
            _ => return None,
        };
        let mut key = crate::values::bounded_instance_key(
            declaration,
            callee.generics(),
            callee.type_packs(),
            requirements,
        )?;
        let entry = store_entry_arguments(self, signature, arguments);
        key.push_str(&format!(
            "aliases{:?}",
            entry
                .iter()
                .filter_map(|argument| {
                    match argument.value.kind {
                        ExprKind::Local(local) => Some((argument.parameter, local)),
                        _ => None,
                    }
                })
                .collect::<Vec<_>>()
        ));
        if let Some(entries) = pack_inputs_of(self, callee, signature, arguments) {
            key.push_str(&format!("entries{entries:?}"));
        }
        if key.len() > INPUT_BUDGET {
            return None;
        }
        let key = (key, callee.test_candidates());
        if let Some(cached) = self.selected_store_routes.borrow().get(&key) {
            return cached.clone();
        }
        self.selected_store_routes
            .borrow_mut()
            .insert(key.clone(), None);
        let proof = (|| {
            let relevant = self
                .modules
                .iter()
                .enumerate()
                .flat_map(|(module, unit)| {
                    unit.details
                        .iter()
                        .enumerate()
                        .filter_map(move |(declaration, detail)| {
                            matches!(detail, Detail::Callable(_)).then_some((module, declaration))
                        })
                })
                .collect();
            let store_slots = HashSet::new();
            let mut read = StoreInstances {
                program: self,
                relevant,
                store_slots: &store_slots,
                nodes: Vec::new(),
                instances: HashMap::new(),
                counts: HashMap::new(),
                pending: VecDeque::new(),
                calls: StoreCalls::default(),
                uses: HashMap::new(),
                observed: HashSet::new(),
            };
            let root = read.instance(callee, arguments)?;
            // Coinductive alias proofs need an external entry. Otherwise a
            // cycle which swaps distinct ports could prove them equal itself.
            let entry_node = read.nodes.len();
            if entry_node >= INPUT_BUDGET {
                return None;
            }
            read.nodes.push((None, signature.clone()));
            let target = &read.nodes[root].1;
            read.calls
                .record(self, entry_node, signature, root, target, |slot| {
                    entry
                        .iter()
                        .find(|argument| argument.parameter == Some(slot))
                        .map(|argument| &argument.value)
                });
            while let Some((node, body)) = read.pending.pop_front() {
                read.body(node, &body)?;
            }
            let changing = read.calls.changed(read.observed.clone(), &read.uses, true);
            let mut fixed = HashSet::new();
            let mut pending: Vec<_> = read.observed.into_iter().collect();
            while let Some(slot) = pending.pop() {
                if changing.contains(&slot) || !fixed.insert(slot) {
                    continue;
                }
                if let Some(callers) = read.uses.get(&slot) {
                    pending.extend(callers.iter().map(|(caller, _)| *caller));
                }
            }
            Some(StoreRoute {
                fixed: fixed
                    .into_iter()
                    .filter_map(|(node, slot)| (node == root).then_some(slot))
                    .collect(),
                changing: changing
                    .into_iter()
                    .filter_map(|(node, slot)| (node == root).then_some(slot))
                    .collect(),
            })
        })();
        self.selected_store_routes
            .borrow_mut()
            .insert(key, proof.clone());
        proof
    }
}

/// Retain only the entry's alias partition, never its string values. Each
/// unproved input is a separate external port. Stable reads and primitive
/// expressions can name one value before the first edge of a cycle (TIME-8).
fn store_entry_arguments(
    program: &Program,
    signature: &Signature,
    arguments: &[Argument],
) -> Vec<Argument> {
    let mut sources: Vec<(usize, &Expr)> = Vec::new();
    let mut remaining = INPUT_BUDGET;
    arguments
        .iter()
        .filter(|argument| {
            argument
                .parameter
                .is_some_and(|slot| signature.parameters.get(slot).is_some_and(|p| p.constant))
        })
        .map(|given| {
            let mut argument = given.clone();
            if let Some(slot) = argument.parameter
                && signature
                    .parameters
                    .get(slot)
                    .is_some_and(|parameter| parameter.constant)
            {
                let source = sources
                    .iter()
                    .find_map(|(slot, earlier)| {
                        let same = same_store_entry(
                            program,
                            earlier,
                            &argument.value,
                            INPUT_DEPTH,
                            &mut remaining,
                        );
                        same.then_some(*slot)
                    })
                    .unwrap_or(slot);
                sources.push((source, &given.value));
                argument.value.kind = ExprKind::Local(source);
            }
            argument
        })
        .collect()
}

/// Compare fixed entry provenance, never evaluate helper code or specialize
/// on key text. Source locations do not distinguish two reads of one value.
fn same_store_entry(
    program: &Program,
    first: &Expr,
    second: &Expr,
    depth: usize,
    remaining: &mut usize,
) -> bool {
    let Some(next) = remaining.checked_sub(1) else {
        return false;
    };
    *remaining = next;
    if depth == 0 || first.ty != second.ty || first.category != second.category {
        return false;
    }
    let Some((first, first_owner)) = store_entry_root(program, first, remaining) else {
        return false;
    };
    let Some((second, second_owner)) = store_entry_root(program, second, remaining) else {
        return false;
    };
    if first_owner.is_some() && first_owner == second_owner {
        return true;
    }
    if let (ExprKind::Literal(Value::Str(a)), ExprKind::Literal(Value::Str(b))) =
        (&first.kind, &second.kind)
    {
        return a.len() <= INPUT_BUDGET && b.len() <= INPUT_BUDGET && a == b;
    }
    // Compare source-known strings independently of their spelling and order.
    // A named constant is no more fixed than its inlined primitive expression.
    if first.ty == Type::Scalar(Scalar::Str)
        && !matches!(first.kind, ExprKind::Local(_))
        && !matches!(second.kind, ExprKind::Local(_))
        && let Some(equal) = same_known_store_entry(program, first, second, depth, remaining)
    {
        return equal;
    }
    let mut same = |a: &Expr, b: &Expr| same_store_entry(program, a, b, depth - 1, remaining);
    match (&first.kind, &second.kind) {
        (ExprKind::Local(a), ExprKind::Local(b)) => a == b,
        (ExprKind::Literal(a), ExprKind::Literal(b))
            if !matches!(a, Value::Type(_) | Value::Str(_)) =>
        {
            a == b
        }
        (ExprKind::Tuple(a), ExprKind::Tuple(b))
        | (ExprKind::Sequence(a), ExprKind::Sequence(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same(a, b))
        }
        (
            ExprKind::Construct {
                module: am,
                declaration: ad,
                arguments: aa,
                ..
            },
            ExprKind::Construct {
                module: bm,
                declaration: bd,
                arguments: ba,
                ..
            },
        ) => {
            (am, ad) == (bm, bd)
                && aa.len() == ba.len()
                && aa
                    .iter()
                    .zip(ba)
                    .all(|(a, b)| a.parameter == b.parameter && same(&a.value, &b.value))
        }
        (
            ExprKind::Field {
                base: a,
                member: am,
            },
            ExprKind::Field {
                base: b,
                member: bm,
            },
        ) => am == bm && same(a, b),
        (ExprKind::Index { base: a, index: ai }, ExprKind::Index { base: b, index: bi }) => {
            same(a, b) && same(ai, bi)
        }
        (ExprKind::Widen(a), ExprKind::Widen(b)) => same(a, b),
        (
            ExprKind::Unary {
                operator: a,
                operand: av,
            },
            ExprKind::Unary {
                operator: b,
                operand: bv,
            },
        ) => a == b && same(av, bv),
        (
            ExprKind::Binary {
                operator: a,
                left: al,
                right: ar,
            },
            ExprKind::Binary {
                operator: b,
                left: bl,
                right: br,
            },
        ) => a == b && same(al, bl) && same(ar, br),
        (
            ExprKind::Call {
                callee: a,
                arguments: aa,
                form: af,
                ..
            },
            ExprKind::Call {
                callee: b,
                arguments: ba,
                form: bf,
                ..
            },
        ) if matches!(af, Form::Unary(_) | Form::Binary(_))
            && af == bf
            && a == b
            && a.declaration().is_some_and(|(module, declaration)| {
                program.core() == Some(module)
                    && matches!(
                        program.modules[module].declarations[declaration].kind,
                        crate::DeclarationKind::NativeCandidate(Role::Value, _)
                    )
            }) =>
        {
            aa.len() == ba.len()
                && aa
                    .iter()
                    .zip(ba)
                    .all(|(a, b)| a.parameter == b.parameter && same(&a.value, &b.value))
        }
        _ => false,
    }
}

/// Compare both source-known entries with the same bounded reader (UNIT-13,
/// TIME-8), without evaluating helpers. Unread values retain structural proof.
fn same_known_store_entry(
    program: &Program,
    first: &Expr,
    second: &Expr,
    depth: usize,
    remaining: &mut usize,
) -> Option<bool> {
    let mut reader = Reader::new(program);
    reader.steps_left = *remaining;
    reader.read_string_helpers = false;
    let left = reader.read(first, depth - 1, &[]);
    let right = reader.read(second, depth - 1, &[]);
    *remaining = reader.steps_left;
    if let (Ok(left), Ok(right)) = (left, right)
        && let (ExprKind::Literal(Value::Str(a)), ExprKind::Literal(Value::Str(b))) =
            (&left.kind, &right.kind)
    {
        Some(a == b)
    } else {
        None
    }
}

/// Keep the last named declaration as well as its initializer: two aliases of
/// one computed constant are equal without executing its initializer again.
fn store_entry_root<'a>(
    program: &'a Program,
    mut value: &'a Expr,
    remaining: &mut usize,
) -> Option<(&'a Expr, Option<Declaration>)> {
    let mut owner = None;
    for _ in 0..INPUT_DEPTH {
        let ExprKind::Constant {
            module,
            declaration,
        } = value.kind
        else {
            return Some((value, owner));
        };
        *remaining = remaining.checked_sub(1)?;
        owner = Some((module, declaration));
        value = &program
            .modules
            .get(module)?
            .constants
            .get(declaration)?
            .as_ref()?
            .value;
    }
    None
}

/// TIME-8 follows the calls an instance makes, not all candidates of the
/// operator it names. Keys stay symbolic here: this graph specializes types
/// and forwarding shapes, never the values a recursive key can reach.
fn resolved_store_changes(
    program: &Program,
    legacy: &StoreCalls,
    conservative: &ChangingSlots,
    observed: &HashSet<Slot>,
    uses: &StoreUses<Declaration>,
) -> Option<ChangingSlots> {
    let store = program.capability_type("global_state")?;
    let borrows_store = |signature: &Signature| {
        signature
            .parameters
            .iter()
            .any(|parameter| parameter.ty == store)
    };
    // Reverse declaration edges find only entries that can reach this cycle.
    // An unread, unrelated graph (for example a Core map example) must not
    // discard a proof about the store key of another component.
    let mut incoming: HashMap<Declaration, Vec<Declaration>> = HashMap::new();
    for &(caller, called) in &legacy.calls {
        incoming.entry(called).or_default().push(caller);
    }
    for (module, unit) in program.modules.iter().enumerate() {
        for (declaration, declared) in unit.declarations.iter().enumerate() {
            if let Some((_, Some(operator))) = declared.kind.candidate() {
                incoming
                    .entry((module, declaration))
                    .or_default()
                    .push((operator.module, operator.declaration));
            }
            if let Some(body) = unit.bodies.get(declaration).and_then(Option::as_ref) {
                crate::values::walk_body_calls(body, &mut |call| {
                    if let ExprKind::Call { callee, .. } = &call.kind
                        && let Some(called) = callee.declaration()
                    {
                        incoming
                            .entry(called)
                            .or_default()
                            .push((module, declaration));
                    }
                });
            }
            for selecting in &unit.selecting[declaration] {
                incoming
                    .entry((selecting.operator.module, selecting.operator.declaration))
                    .or_default()
                    .push((module, declaration));
            }
        }
    }
    let empty = Signature {
        generics: Vec::new(),
        parameters: Vec::new(),
        result: None,
        reference: false,
    };
    let mut changed = conservative.clone();
    for component in legacy.components() {
        if !conservative
            .iter()
            .any(|(declaration, _)| component.contains(declaration))
        {
            continue;
        }
        let mut relevant = component.clone();
        follow_slots(&mut relevant, &incoming);
        // An observer outside the cycle also observes the slot that supplies
        // it. Do not propagate observations around the cycle: an exchanged
        // second key of a reader must remain distinguishable from its first.
        let dependencies: HashMap<Slot, Vec<Slot>> = uses
            .iter()
            .map(|(&slot, callers)| {
                let callers = callers
                    .iter()
                    .map(|&(caller, _)| caller)
                    .filter(|caller| {
                        !(component.contains(&slot.0) && component.contains(&caller.0))
                    })
                    .collect();
                (slot, callers)
            })
            .collect();
        let mut store_slots = observed.clone();
        follow_slots(&mut store_slots, &dependencies);
        let mut read = StoreInstances {
            program,
            relevant,
            store_slots: &store_slots,
            nodes: Vec::new(),
            instances: HashMap::new(),
            counts: HashMap::new(),
            pending: VecDeque::new(),
            calls: StoreCalls::default(),
            uses: HashMap::new(),
            observed: HashSet::new(),
        };
        let refined = (|| {
            // Borrowed-store helpers get their input aliases from actual
            // callers. Nodes, tests and applications provide entry routes.
            for (module, unit) in program.modules.iter().enumerate() {
                for (declaration, body) in unit.bodies.iter().enumerate() {
                    let (Some(body), Some(Detail::Callable(signature))) =
                        (body, unit.details.get(declaration))
                    else {
                        continue;
                    };
                    if signature.generics.is_empty() && !borrows_store(signature) {
                        read.root(Some((module, declaration)), signature, body)?;
                    }
                }
                for body in unit.test_bodies.iter().flatten().chain(&unit.store_body) {
                    read.root(None, &empty, body)?;
                }
                if let Some(run) = &unit.run_body {
                    let body = crate::hir::Body {
                        locals: run.locals.clone(),
                        statements: vec![Stmt::Expr(run.call.clone())],
                    };
                    read.root(None, &empty, &body)?;
                }
            }
            while let Some((node, body)) = read.pending.pop_front() {
                read.body(node, &body)?;
            }
            // Only inspected members have proven input routes. A selected
            // candidate can leave other members of this coarse cycle unread.
            let inspected: HashSet<_> = read
                .nodes
                .iter()
                .filter_map(|(declaration, _)| *declaration)
                .collect();
            let result = read.calls.changed(read.observed, &read.uses, true);
            Some((
                inspected,
                result
                    .into_iter()
                    .filter_map(|(node, slot)| {
                        read.nodes[node].0.map(|declaration| (declaration, slot))
                    })
                    .collect::<ChangingSlots>(),
            ))
        })();
        if let Some((inspected, refined)) = refined {
            changed.retain(|(declaration, _)| {
                !component.contains(declaration) || !inspected.contains(declaration)
            });
            changed.extend(
                refined
                    .into_iter()
                    .filter(|(declaration, _)| component.contains(declaration)),
            );
        }
    }
    Some(changed)
}

/// A bounded type-instance graph, separate from the key's const inputs.
struct StoreInstances<'a> {
    program: &'a Program,
    relevant: HashSet<Declaration>,
    store_slots: &'a HashSet<Slot>,
    nodes: Vec<(Option<Declaration>, Signature)>,
    instances: HashMap<(String, Option<crate::TestCandidateScope>), usize>,
    counts: HashMap<Declaration, usize>,
    pending: VecDeque<(usize, crate::hir::Body)>,
    calls: StoreCalls<usize>,
    uses: StoreUses<usize>,
    observed: HashSet<(usize, usize)>,
}

impl StoreInstances<'_> {
    fn root(
        &mut self,
        declaration: Option<Declaration>,
        signature: &Signature,
        body: &crate::hir::Body,
    ) -> Option<()> {
        let mut needed = false;
        crate::values::walk_body_calls(body, &mut |call| {
            if let ExprKind::Call { callee, .. } = &call.kind {
                needed |= callee
                    .declaration()
                    .is_some_and(|called| self.relevant.contains(&called));
            }
        });
        if needed {
            let node = self.nodes.len();
            self.nodes.push((declaration, signature.clone()));
            self.body(node, body)?;
        }
        Some(())
    }

    fn instance(&mut self, callee: &Callee, arguments: &[Argument]) -> Option<usize> {
        let declaration = callee.declaration()?;
        let unit = &self.program.modules[declaration.0];
        let Detail::Callable(signature) = &unit.details[declaration.1] else {
            return None;
        };
        let requirements = match callee {
            Callee::Function { requirements, .. } | Callee::Candidate { requirements, .. } => {
                requirements
            }
            _ => return None,
        };
        let entries = pack_inputs_of(self.program, callee, signature, arguments);
        let mut key = crate::values::bounded_instance_key(
            declaration,
            callee.generics(),
            callee.type_packs(),
            requirements,
        )?;
        if let Some(entries) = &entries {
            key.push_str(&format!("entries{entries:?}"));
        }
        if key.len() > INPUT_BUDGET {
            return None;
        }
        let key = (key, callee.test_candidates());
        if let Some(&node) = self.instances.get(&key) {
            return Some(node);
        }
        let count = self.counts.entry(declaration).or_default();
        *count += 1;
        if *count > crate::INSTANCES || self.nodes.len() >= INPUT_BUDGET {
            return None;
        }
        let (taken, body) = if unit
            .bodies
            .get(declaration.1)
            .and_then(Option::as_ref)
            .is_some()
        {
            let placement = RefCell::new(Diagnostics::new());
            let mut reports = Diagnostics::new();
            let instance = self.program.checked_instance(
                declaration,
                callee.generics(),
                requirements,
                crate::InstanceInputs {
                    type_packs: callee.type_packs(),
                    pack_inputs: entries.as_deref(),
                    test_candidates: callee.test_candidates(),
                    ..crate::InstanceInputs::default()
                },
                &placement,
                &mut reports,
            )?;
            if reports.has_errors() || placement.borrow().has_errors() {
                return None;
            }
            // Keep the original slot numbering. Existing demand analysis is the
            // conservative fallback for a forwarding shape that expands it.
            if instance
                .signature
                .parameters
                .iter()
                .map(|p| p.at)
                .ne(signature.parameters.iter().map(|p| p.at))
            {
                return None;
            }
            (instance.signature, Some(instance.body))
        } else {
            (signature.clone(), None)
        };
        let node = self.nodes.len();
        self.nodes.push((Some(declaration), taken));
        self.instances.insert(key, node);
        if let Some(body) = body {
            self.pending.push_back((node, body));
        }
        Some(node)
    }

    fn body(&mut self, node: usize, body: &crate::hir::Body) -> Option<()> {
        let (declaration, signature) = self.nodes[node].clone();
        if let Some(declaration) = declaration {
            self.observed.extend(
                signature
                    .parameters
                    .iter()
                    .enumerate()
                    .filter(|(slot, _)| self.store_slots.contains(&(declaration, *slot)))
                    .map(|(slot, _)| (node, slot)),
            );
        }
        let bindings = declaration
            .map(|(module, _)| local_bindings(&self.program.modules[module]))
            .unwrap_or_default();
        let slots = signature
            .parameters
            .iter()
            .enumerate()
            .filter(|(_, p)| p.constant)
            .map(|(slot, p)| (p.at, slot))
            .collect();
        let mut calls = Vec::new();
        let mut unresolved = false;
        crate::values::walk_body_calls(body, &mut |call| {
            if let ExprKind::Call {
                callee, arguments, ..
            } = &call.kind
            {
                if let Some(key) = self.program.store_key_argument(callee, arguments) {
                    slots_read(key, &bindings, &slots, |slot| {
                        self.observed.insert((node, slot));
                    });
                }
                unresolved |= matches!(
                    callee,
                    Callee::DeferredOperator(_) | Callee::DeferredPackCall
                );
                if callee
                    .declaration()
                    .is_some_and(|called| self.relevant.contains(&called))
                {
                    calls.push((callee.clone(), arguments.clone()));
                }
            }
        });
        if unresolved && declaration.is_some_and(|declaration| self.relevant.contains(&declaration))
        {
            return None;
        }
        for (callee, arguments) in calls {
            let target = self.instance(&callee, &arguments)?;
            let parameters = &self.nodes[target].1;
            self.calls
                .record(self.program, node, &signature, target, parameters, |slot| {
                    arguments
                        .iter()
                        .find(|a| a.parameter == Some(slot))
                        .map(|a| &a.value)
                });
            for argument in &arguments {
                if let Some(slot) = argument.parameter {
                    slots_read(&argument.value, &bindings, &slots, |from| {
                        self.uses
                            .entry((target, slot))
                            .or_default()
                            .push(((node, from), Demand::Always));
                    });
                }
            }
        }
        Some(())
    }
}

/// Calls that can pass configuration in a recursive graph or store helper.
/// `N` is a declaration in the conservative graph and a type-instance index
/// in the refinement; both use the same cycle and alias analysis.
#[derive(Default)]
struct StoreCalls<N = Declaration> {
    calls: HashSet<(N, N)>,
    inputs: Vec<StoreInput<N>>,
    routes: HashMap<N, Vec<(N, Vec<StoreSource>)>>,
}

/// What one call hands to a const parameter, without evaluating value code.
#[derive(Clone)]
enum StoreSource {
    Parameter(usize),
    Literal(String),
    /// A stable expression shared by ports of this one incoming call.
    Fixed(usize),
    Other,
}

struct StoreInput<N = Declaration> {
    caller: N,
    callee: (N, usize),
    forwarded: Option<usize>,
}

impl<N: Copy + Eq + Hash> StoreCalls<N> {
    /// Record a call from `caller`, written as `signature`, of `callee`,
    /// written as `target`; `argument_for` gives the argument of each of
    /// the callee's slots, where the call writes one.
    fn record<'a>(
        &mut self,
        program: &Program,
        caller: N,
        signature: &Signature,
        callee: N,
        target: &Signature,
        argument_for: impl Fn(usize) -> Option<&'a Expr>,
    ) {
        self.calls.insert((caller, callee));
        let mut sources: Vec<StoreSource> = Vec::new();
        let mut written: Vec<(usize, &Expr)> = Vec::new();
        let mut remaining = INPUT_BUDGET;
        for (slot, parameter) in target.parameters.iter().enumerate() {
            let source = argument_for(slot).map(|argument| {
                // All spellings join the same incoming partition, including
                // literals beside expressions. Preserve the earlier port's
                // provenance rather than giving expressions a separate class.
                let earlier = written.iter().find_map(|(slot, earlier)| {
                    same_store_entry(program, earlier, argument, INPUT_DEPTH, &mut remaining)
                        .then(|| sources[*slot].clone())
                });
                let source = earlier.unwrap_or_else(|| match &argument.kind {
                    ExprKind::Local(local) => StoreSource::Parameter(*local),
                    ExprKind::Literal(Value::Str(text)) => StoreSource::Literal(text.clone()),
                    _ if same_store_entry(
                        program,
                        argument,
                        argument,
                        INPUT_DEPTH,
                        &mut remaining,
                    ) =>
                    {
                        StoreSource::Fixed(slot)
                    }
                    _ => StoreSource::Other,
                });
                written.push((slot, argument));
                source
            });
            sources.push(source.unwrap_or(StoreSource::Other));
            if !parameter.constant {
                continue;
            }
            // Equal expressions on an entry edge can alias, but an expression
            // evaluated anew inside the cycle is still not plain forwarding.
            let forwarded = argument_for(slot).and_then(|argument| match argument.kind {
                ExprKind::Local(local)
                    if signature
                        .parameters
                        .get(local)
                        .is_some_and(|parameter| parameter.constant) =>
                {
                    Some(local)
                }
                _ => None,
            });
            self.inputs.push(StoreInput {
                caller,
                callee: (callee, slot),
                forwarded,
            });
        }
        self.routes
            .entry(callee)
            .or_default()
            .push((caller, sources));
    }

    /// Two ports are aliases only when every incoming route supplies one key.
    /// Following a pair already visited is coinductive: a cycle that merely
    /// hands the pair on adds no difference. An unequal entry route disproves it.
    fn same_key(&self, function: N, first: usize, second: usize) -> bool {
        let mut waiting = vec![(function, first, second)];
        let mut seen = HashSet::new();
        while let Some((function, first, second)) = waiting.pop() {
            if first == second || !seen.insert((function, first.min(second), first.max(second))) {
                continue;
            }
            let Some(routes) = self.routes.get(&function) else {
                return false;
            };
            for (caller, sources) in routes {
                let (Some(left), Some(right)) = (sources.get(first), sources.get(second)) else {
                    return false;
                };
                match (left, right) {
                    (StoreSource::Parameter(left), StoreSource::Parameter(right)) => {
                        waiting.push((*caller, *left, *right));
                    }
                    (StoreSource::Literal(left), StoreSource::Literal(right)) if left == right => {}
                    (StoreSource::Fixed(left), StoreSource::Fixed(right)) if left == right => {}
                    _ => return false,
                }
            }
        }
        true
    }

    /// Recursive components of this call graph (TIME-8).
    fn components(&self) -> Vec<HashSet<N>> {
        let mut graph = DiGraph::<N, ()>::new();
        let mut nodes = HashMap::new();
        for &(caller, callee) in &self.calls {
            let caller = *nodes
                .entry(caller)
                .or_insert_with(|| graph.add_node(caller));
            let callee = *nodes
                .entry(callee)
                .or_insert_with(|| graph.add_node(callee));
            graph.add_edge(caller, callee, ());
        }
        kosaraju_scc(&graph)
            .into_iter()
            .filter(|component| {
                component.len() > 1 || graph.contains_edge(component[0], component[0])
            })
            .map(|component| component.into_iter().map(|node| graph[node]).collect())
            .collect()
    }

    /// The slots of each cycle whose key can differ between its calls
    /// (TIME-8): one the cycle's calls compute or give anew, one that
    /// receives another parameter on the way round, and what such a slot
    /// is handed on to in the cycle. `observed` holds the slots a store
    /// operation's key reads.
    fn changed(
        &self,
        mut observed: HashSet<(N, usize)>,
        uses: &StoreUses<N>,
        aliases: bool,
    ) -> HashSet<(N, usize)> {
        let cycles: HashMap<N, usize> = self
            .components()
            .into_iter()
            .enumerate()
            .flat_map(|(number, component)| component.into_iter().map(move |node| (node, number)))
            .collect();
        let same_cycle = |caller: N, callee: N| {
            cycles
                .get(&caller)
                .is_some_and(|cycle| cycles.get(&callee) == Some(cycle))
        };
        // From a callee's slot to each slot of its caller that a call
        // leaving the caller's cycle reads for it.
        let mut outside = HashMap::<(N, usize), Vec<(N, usize)>>::new();
        for (&to, senders) in uses {
            for &(from, _) in senders {
                if self.calls.contains(&(from.0, to.0)) && !same_cycle(from.0, to.0) {
                    outside.entry(to).or_default().push(from);
                }
            }
        }
        // A key a helper outside the cycle uses is also observed in the
        // cycle's slot that supplies it: a cycle that exchanges two keys
        // and delegates its operation still exchanges them.
        follow_slots(&mut observed, &outside);

        let mut changed = HashSet::new();
        let mut forwarded = HashMap::<(N, usize), Vec<(N, usize)>>::new();
        let mut received = HashMap::<(N, usize), Vec<(N, usize)>>::new();
        for input in &self.inputs {
            if !same_cycle(input.caller, input.callee.0) {
                continue;
            }
            if let Some(local) = input.forwarded {
                let from = (input.caller, local);
                forwarded.entry(from).or_default().push(input.callee);
                received.entry(input.callee).or_default().push(from);
            } else {
                changed.insert(input.callee);
            }
        }
        let readers: HashSet<_> = observed.iter().map(|slot| slot.0).collect();
        for &read in &observed {
            if !changed.contains(&read)
                && cycle_passes_another_key(read, &received, &observed, &readers, &|node, a, b| {
                    aliases && self.same_key(node, a, b)
                })
            {
                changed.insert(read);
            }
        }
        follow_slots(&mut changed, &forwarded);
        changed
    }
}

/// Add to `slots` every slot their `edges` reach.
fn follow_slots<S: Copy + Eq + Hash>(slots: &mut HashSet<S>, edges: &HashMap<S, Vec<S>>) {
    let mut pending: Vec<_> = slots.iter().copied().collect();
    while let Some(slot) = pending.pop() {
        pending.extend(
            edges
                .get(&slot)
                .into_iter()
                .flatten()
                .copied()
                .filter(|to| slots.insert(*to)),
        );
    }
}

/// Trace the inputs of a key back round a cycle, through the slots that
/// hand it on unchanged (TIME-8: "where it is another `const` parameter,
/// as two exchanged on the way round are"). The key is exchanged where
/// the trace reaches two non-aliased slots of one function, that of the
/// key or any other on the way round, and where it reaches a slot of a function that
/// reads keys which is none of its keys.
fn cycle_passes_another_key<N: Copy + Eq + Hash>(
    read: (N, usize),
    received: &HashMap<(N, usize), Vec<(N, usize)>>,
    observed: &HashSet<(N, usize)>,
    readers: &HashSet<N>,
    same_key: &impl Fn(N, usize, usize) -> bool,
) -> bool {
    let mut seen = HashSet::from([read]);
    let mut reached = HashMap::from([read]);
    let mut waiting = vec![read];
    while let Some(slot) = waiting.pop() {
        for &source in received.get(&slot).into_iter().flatten() {
            if !seen.insert(source) {
                continue;
            }
            let earlier = *reached.entry(source.0).or_insert(source.1);
            let exchanged = earlier != source.1 && !same_key(source.0, earlier, source.1);
            let another = readers.contains(&source.0)
                && !observed.iter().any(|&(node, slot)| {
                    node == source.0 && (slot == source.1 || same_key(node, slot, source.1))
                });
            if exchanged || another {
                return true;
            }
            waiting.push(source);
        }
    }
    false
}

/// Whether a call of a function parameter gives this slot of its template
/// a plain parameter: one that is no `const` and stands for no pack (FN-17).
fn plain_callback_input(
    signature: &crate::Signature,
    callee: &Callee,
    slot: Option<usize>,
) -> bool {
    let Callee::Parameter { parameter, .. } = callee else {
        return false;
    };
    let Some(Type::Function(template)) = signature.parameters.get(*parameter).map(|p| &p.ty) else {
        return false;
    };
    template.named_pack.is_none()
        && slot
            .and_then(|slot| template.parameters.get(slot))
            .is_some_and(|input| input.declared().is_none_or(|declared| !declared.constant))
}

/// The entries a call gives its callee's packs, where it has any (FN-19).
/// An empty shape is an input too: it decides the forwarded calls.
fn pack_inputs_of(
    program: &Program,
    callee: &Callee,
    signature: &crate::Signature,
    arguments: &[Argument],
) -> Option<Vec<PackInput>> {
    signature
        .parameters
        .iter()
        .any(|parameter| parameter.pack.is_some())
        .then(|| program.call_pack_inputs(callee, signature, arguments))
}

fn local_bindings(module: &crate::Module) -> HashMap<(hgl_source::FileId, u32), Location> {
    module
        .uses
        .iter()
        .filter_map(|used| match &used.binding {
            Binding::Local { at, .. } => Some(((used.at.file, used.at.span.start), *at)),
            _ => None,
        })
        .collect()
}

/// Use lexical bindings so a captured parameter keeps its original slot.
fn slots_read(
    value: &Expr,
    bindings: &HashMap<(hgl_source::FileId, u32), Location>,
    slots: &HashMap<Location, usize>,
    mut found: impl FnMut(usize),
) {
    value.walk(&mut |value| {
        if matches!(
            value.kind,
            ExprKind::Local(_) | ExprKind::DeferredPresent(_)
        ) && let Some(location) = bindings.get(&(value.at.file, value.at.span.start))
            && let Some(slot) = slots.get(location)
        {
            found(*slot);
        }
    });
}

#[cfg(test)]
mod tests {
    use hgl_diagnostics::Diagnostics;
    use hgl_parser::{Entry, Limits, parse};
    use hgl_source::SourceMap;

    use super::*;
    use crate::{ParsedFile, analyse};

    /// Equal composite values have the same identity even when their named
    /// fields are given in a different order. Floating keys keep their bits.
    #[test]
    fn const_keys_compare_normalized_parts() {
        let mut sources = SourceMap::new();
        let mut diagnostics = Diagnostics::new();
        let file = sources
            .add(
                "m.hgl",
                r#"module m
struct Settings {
    part: tuple<str, f64>
    count: i64
}
const A: Settings = Settings(part: ("a", 0.0), count: 1)
const B: Settings = Settings(count: 1, part: ("a", 0.0))
const C: Settings = Settings(part: ("a", -0.0), count: 1)
"#,
            )
            .unwrap();
        let tokens = hgl_lexer::lex(&sources, file, &mut diagnostics);
        let tree = parse(
            &sources,
            file,
            &tokens,
            Entry::SourceFile,
            Limits::default(),
            &mut diagnostics,
        );
        let program = analyse(&[ParsedFile::new(file, tree.syntax())], &mut diagnostics);
        assert!(!diagnostics.has_errors(), "{diagnostics:?}");
        let key = |name: &str| {
            let module = &program.modules[0];
            let declaration = module.scope[name][0];
            let constant = module.constants[declaration].as_ref().unwrap();
            let mut reader = Reader::new(&program);
            let value = reader.read(&constant.value, INPUT_DEPTH, &[]).unwrap();
            let mut output = String::new();
            key_value(&value, &mut output).unwrap();
            output
        };
        assert_eq!(key("A"), key("B"));
        assert_ne!(key("A"), key("C"));
    }
}
