//! Fixes a node's store keys as the SDK attaches it to a graph (CAP-6, FN-1).

use std::cell::Cell;
use std::collections::HashSet;

use hgl_analyser::hir::{
    Argument, Body, Callee, Expr, ExprKind, Form, Node, Raise, Value, ValueCategory,
    walk_statement_nodes, walk_statements,
};
use hgl_analyser::{
    DeclarationKind, Detail, Parameter, Program, Role, Signature, StoreRoute, Type,
};
use hgl_diagnostics::Location;
use hgl_lower::calls::{LeftOut, left_out};
use hgl_lower::instances::{Instance, Instances};

use super::Emitter;
use super::names::{self, identifier};
use super::statements::Scope;
use super::values::INDENT;
use crate::runtime;

/// Store keys checked when the SDK attaches a node.
pub(super) struct Keys {
    values: Vec<Expr>,
    contains_fixed: bool,
    complete: bool,
    pub(super) recorder: bool,
}

impl Keys {
    pub(super) fn of(
        emitter: &Emitter<'_>,
        declaration: usize,
        signature: &Signature,
        body: &Body,
    ) -> Self {
        let program = emitter.program;
        let module = emitter.module;
        let recorder = program.core() == Some(module)
            && program.modules[module].declarations[declaration].name == "record";
        let mut keys = Self {
            values: Vec::new(),
            contains_fixed: false,
            complete: true,
            recorder,
        };
        if recorder {
            return keys;
        }
        let Some(store) = program.capability_type("global_state") else {
            return keys;
        };
        let mut trace = ForwardedKeys {
            program,
            instances: emitter.instances,
            store,
            path: Vec::new(),
            remaining: Cell::new(1 << 16),
        };
        let bindings = Bindings::Configuration(&signature.parameters);
        walk_statement_nodes(&body.statements, &mut |node| {
            let Node::Expression(Expr {
                kind: ExprKind::Call {
                    callee, arguments, ..
                },
                at,
                ..
            }) = node
            else {
                return;
            };
            if let Some(key) = program.store_key_argument(callee, arguments) {
                keys.read(program, key, bindings, &trace.remaining);
            } else {
                trace.call(callee, arguments, bindings, *at, &mut keys);
            }
        });
        keys
    }

    /// Record a key for attachment as its node's configuration fixes it.
    /// A key that differs between the calls of a cycle on this route is
    /// left to its operation (TIME-8), as is one the source check proved.
    fn read(
        &mut self,
        program: &Program,
        key: &Expr,
        bindings: Bindings<'_>,
        remaining: &Cell<usize>,
    ) {
        if program.store_key_at_execution(key) || bindings.changes(key) || source_safe(program, key)
        {
            return;
        }
        self.contains_fixed = true;
        let Some(projected) = project(program, key, bindings, remaining) else {
            self.complete = false;
            return;
        };
        if !source_safe(program, &projected) {
            self.values.push(projected);
        }
    }

    /// Marks the borrowed capability so fixed keys already checked at
    /// attachment do not need checking in each hook invocation.
    pub(super) fn prechecked(&self) -> bool {
        self.contains_fixed && self.complete
    }

    /// The SDK invokes prepare once on first attachment, including for a
    /// temporal child, before any node of that graph starts.
    pub(super) fn prepare(
        &self,
        emitter: &mut Emitter<'_>,
        signature: &Signature,
        body: &Body,
    ) -> Option<String> {
        if self.values.is_empty() {
            return Some(String::new());
        }
        let indent = INDENT.repeat(2);
        let mut checks = String::new();
        // Read only the configuration the keys are made of.
        let mut read_slots = HashSet::new();
        for key in &self.values {
            key.walk(&mut |part| {
                if let ExprKind::Local(slot) = part.kind {
                    read_slots.insert(slot);
                }
            });
        }
        for (slot, parameter) in signature.parameters.iter().enumerate() {
            if !parameter.constant || !read_slots.contains(&slot) {
                continue;
            }
            let name = identifier(&body.locals[slot].name);
            let scalar = names::temporary("store_config", slot);
            let label = names::string(&parameter.name);
            let ty = emitter.written_type(&parameter.ty, parameter.at)?;
            let held = super::configuration::held(emitter.program, &parameter.ty, &ty);
            let read = super::configuration::read(emitter.program, &parameter.ty, &scalar);
            checks.push_str(&format!(
                "{indent}const ::hgraph::Scalar<{label}, {held}> {scalar}{{node_.scalars().as_bundle().field({label})}};\n\
                 {indent}[[maybe_unused]] const auto& {name} = {read};\n"
            ));
        }
        let mut scope = Scope::cold(emitter.program, body);
        let mut seen = HashSet::new();
        for key in &self.values {
            let value = emitter.expression(&mut scope, key);
            if !seen.insert(value.clone()) {
                continue;
            }
            let library = runtime::NAMESPACE;
            let at = emitter.source_location(key.at);
            let step = format!(
                "{indent}::{library}::prepare_store_key([&]() -> ::hgraph::Str {{ return {value}; }}, {at});\n"
            );
            checks.push_str(&emitter.debug_stamp(key.at, step));
        }
        Some(format!(
            "\n{INDENT}static void prepare([[maybe_unused]] const ::hgraph::NodeView& node_)\n{INDENT}{{\n{checks}{INDENT}}}\n"
        ))
    }
}

/// Follow fixed arguments through value helpers that borrow the run's store.
struct ForwardedKeys<'a> {
    program: &'a Program,
    instances: &'a Instances,
    store: Type,
    path: Vec<(usize, usize)>,
    remaining: Cell<usize>,
}

/// What the locals of the code being followed stand for.
#[derive(Clone, Copy)]
enum Bindings<'a> {
    /// The node's own hooks: its `const` parameters are its configuration.
    Configuration(&'a [Parameter]),
    /// A helper's parameters, as the route followed gives them.
    Given(&'a [Binding]),
}

/// One parameter of a helper on the route followed.
#[derive(Default)]
struct Binding {
    /// Its value in the node's configuration, where it can be written so.
    value: Option<Expr>,
    /// Whether it can differ between the calls of a cycle on this route
    /// (TIME-8): its key is then checked at the operation, not here.
    changing: bool,
}

impl Bindings<'_> {
    /// Whether `value` reads a parameter that varies on this route.
    fn changes(self, value: &Expr) -> bool {
        let Self::Given(bindings) = self else {
            return false;
        };
        let mut changing = false;
        value.walk(&mut |part| {
            if let ExprKind::Local(local) = part.kind {
                changing |= bindings.get(local).is_some_and(|binding| binding.changing);
            }
        });
        changing
    }
}

impl ForwardedKeys<'_> {
    fn call(
        &mut self,
        callee: &Callee,
        arguments: &[Argument],
        caller: Bindings<'_>,
        at: Location,
        keys: &mut Keys,
    ) {
        let Some((module, declaration)) = callee.declaration() else {
            return;
        };
        let program = self.program;
        let unit = &program.modules[module];
        let Some(Detail::Callable(signature)) = unit.details.get(declaration) else {
            return;
        };
        let declared = &unit.declarations[declaration];
        if !matches!(
            declared.kind,
            DeclarationKind::Function(Role::Value)
                | DeclarationKind::Implementation(Role::Value, _)
        ) || !signature
            .parameters
            .iter()
            .any(|parameter| parameter.ty == self.store)
        {
            return;
        }
        let identity = (module, declaration);
        // The source classification already says which keys can change
        // on the way round. Follow the fixed ones once through the cycle;
        // a helper reached again by another route is followed again, with
        // that route's bindings.
        if self.path.contains(&identity) {
            return;
        }
        if spend(&self.remaining).is_none() || self.path.len() == 64 {
            keys.complete = false;
            return;
        }
        let instance = self.instances.find_call(program, callee, arguments);
        let (signature, body) = if let Some(instance) = instance {
            (&instance.signature, &instance.body)
        } else {
            let Some(body) = unit.bodies.get(declaration).and_then(Option::as_ref) else {
                keys.complete = false;
                return;
            };
            (signature, body)
        };
        let fixed: Vec<_> = signature
            .parameters
            .iter()
            .enumerate()
            .map(|(slot, parameter)| {
                if !parameter.constant {
                    return Binding::default();
                }
                let declared_slot =
                    instance.map_or(slot, |instance| instance.original_parameter(slot));
                let written = arguments.iter().find(|argument| {
                    instance.map_or(argument.parameter, |instance| instance.parameter(argument))
                        == Some(slot)
                });
                // TIME-8: a parameter its cycle changes varies on every
                // route, and one given a varying key varies on this one.
                if program.store_route(identity, declared_slot) == StoreRoute::Changing
                    || written.is_some_and(|argument| caller.changes(&argument.value))
                {
                    return Binding {
                        value: None,
                        changing: true,
                    };
                }
                let value = match written {
                    Some(written) => project(program, &written.value, caller, &self.remaining),
                    None => left_out(program, callee, declared_slot, parameter).and_then(|left| {
                        match left {
                            LeftOut::Default(default) => {
                                project(program, &default, Bindings::Given(&[]), &self.remaining)
                            }
                            LeftOut::Null => Some(Expr {
                                kind: ExprKind::Literal(Value::Null),
                                ty: parameter.ty.clone(),
                                at,
                                category: ValueCategory::Value,
                            }),
                        }
                    }),
                };
                Binding {
                    value,
                    changing: false,
                }
            })
            .collect();
        self.path.push(identity);
        walk_statements(&body.statements, &mut |expression| {
            let ExprKind::Call {
                callee, arguments, ..
            } = &expression.kind
            else {
                return;
            };
            let bindings = Bindings::Given(&fixed);
            if let Some(key) = program.store_key_argument(callee, arguments) {
                keys.read(program, key, bindings, &self.remaining);
            } else {
                self.call(callee, arguments, bindings, expression.at, keys);
            }
        });
        self.path.pop();
    }
}

/// Express a helper's fixed value using its caller's bindings. Only fixed reads
/// and Core's primitive operators may be repeated without running helper code.
fn project(
    program: &Program,
    value: &Expr,
    bindings: Bindings<'_>,
    remaining: &Cell<usize>,
) -> Option<Expr> {
    spend(remaining)?;
    let read = |value: &Expr| project(program, value, bindings, remaining);
    let kind = match &value.kind {
        ExprKind::Local(slot) => {
            return match bindings {
                Bindings::Given(fixed) => {
                    let value = fixed.get(*slot)?.value.as_ref()?;
                    value.walk(&mut |_| {
                        let _ = spend(remaining);
                    });
                    (remaining.get() != 0).then(|| value.clone())
                }
                Bindings::Configuration(parameters) => parameters
                    .get(*slot)
                    .filter(|parameter| parameter.constant)
                    .map(|_| value.clone()),
            };
        }
        ExprKind::Literal(_)
        | ExprKind::Constant { .. }
        | ExprKind::EnumMember { .. }
        | ExprKind::GenericValue { .. } => return Some(value.clone()),
        ExprKind::Field { base, member } => ExprKind::Field {
            base: Box::new(read(base)?),
            member: *member,
        },
        ExprKind::Index { base, index } => ExprKind::Index {
            base: Box::new(read(base)?),
            index: Box::new(read(index)?),
        },
        ExprKind::Widen(inner) => ExprKind::Widen(Box::new(read(inner)?)),
        ExprKind::Raised { result, .. } => ExprKind::Raised {
            result: Box::new(read(result)?),
            raise: Raise::Build,
        },
        ExprKind::OrNull { result, reader } => ExprKind::OrNull {
            result: Box::new(read(result)?),
            reader: *reader,
        },
        ExprKind::NullRead { value, code, .. } => ExprKind::NullRead {
            value: Box::new(read(value)?),
            raise: Raise::Build,
            code,
        },
        ExprKind::EnumFrom {
            module,
            declaration,
            value,
        } => ExprKind::EnumFrom {
            module: *module,
            declaration: *declaration,
            value: Box::new(read(value)?),
        },
        ExprKind::Unary { operator, operand } => ExprKind::Unary {
            operator: *operator,
            operand: Box::new(read(operand)?),
        },
        ExprKind::Binary {
            operator,
            left,
            right,
        } => ExprKind::Binary {
            operator: *operator,
            left: Box::new(read(left)?),
            right: Box::new(read(right)?),
        },
        ExprKind::Call {
            callee,
            form: Form::Binary(_) | Form::Unary(_),
            ..
        } if callee.declaration().is_some_and(|(module, declaration)| {
            program.core() == Some(module)
                && matches!(
                    program.modules[module].declarations[declaration].kind,
                    DeclarationKind::NativeCandidate(Role::Value, _)
                )
        }) =>
        {
            let ExprKind::Call {
                mut arguments,
                callee,
                form,
                text,
                braced,
                lifted,
            } = value.kind.clone()
            else {
                unreachable!()
            };
            for argument in &mut arguments {
                argument.value = read(&argument.value)?;
            }
            ExprKind::Call {
                callee,
                arguments,
                form,
                text,
                braced,
                lifted,
            }
        }
        _ => return None,
    };
    Some(Expr {
        kind,
        ..value.clone()
    })
}

fn spend(remaining: &Cell<usize>) -> Option<()> {
    remaining.set(remaining.get().checked_sub(1)?);
    Some(())
}

pub(super) fn source_safe(program: &Program, key: &Expr) -> bool {
    program
        .source_store_key(key)
        .is_some_and(|key| key != "run.output")
}

pub(super) fn proof_argument(name: &str, fixed_keys_checked: bool) -> &'static str {
    if name == "global_state" && fixed_keys_checked {
        ", true"
    } else {
        ""
    }
}

/// How the key of a store operation is checked where it is used (TIME-8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum KeyCheck {
    /// Always: the key can differ between the calls of a cycle on every
    /// route into the operation.
    Always,
    /// Where attachment did not prove it, or where one of these route bits
    /// of the enclosing function's parameters is set (`Routes`).
    Unless(u64),
}

/// The C++ that hands `value` on as a key, checked as `check` says.
/// `store` is the borrowed store, which carries attachment's proof and
/// the route; `KeyCheck::Always` reads neither.
pub(super) fn checked(value: &str, store: Option<&str>, check: KeyCheck) -> String {
    let library = runtime::NAMESPACE;
    match (check, store) {
        (KeyCheck::Unless(0), Some(store)) => {
            format!("::{library}::use_fixed_store_key({store}, {value})")
        }
        (KeyCheck::Unless(bits), Some(store)) => {
            format!("::{library}::use_fixed_store_key({store}, {value}, {bits:#x}ull)")
        }
        (KeyCheck::Always, _) | (KeyCheck::Unless(_), None) => {
            format!("::{library}::checked_store_key({value})")
        }
    }
}

/// Whether a `const` parameter of the value function being written can
/// hold a key that differs between the calls of a cycle (TIME-8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Varying {
    /// On no route.
    Never,
    /// On every route: the parameter is one the cycle changes.
    Always,
    /// On the routes that set this bit of the borrowed store
    /// (`GlobalState::routed`): a function outside the cycle, or one the
    /// cycle hands it on to unchanged, given a changed key on some calls.
    Route(u64),
}

/// The routes of the value function being written: how each of its
/// parameters, by local, can vary (TIME-8). Fixedness is per route: one
/// helper can be given a changed key from a cycle and a fixed key
/// elsewhere, and only the first is checked at its operation. A value
/// function's store comes from its caller, so each value function has
/// these, all `Never` where nothing varies; other code makes its own
/// store, on which nothing varies.
#[derive(Default)]
pub(super) struct Routes {
    varying: Vec<Varying>,
}

/// One bit for each parameter position up to this one; a later
/// parameter that varies on a route is checked as one that always does.
const ROUTE_BITS: usize = 64;

impl Routes {
    /// The routes of declaration `identity`, written as `signature`, or
    /// as its instance `instance`.
    pub(super) fn of(
        program: &Program,
        identity: (usize, usize),
        signature: &Signature,
        instance: Option<&Instance>,
    ) -> Self {
        if !program.has_store_routes() {
            return Self::default();
        }
        let varying = (0..signature.parameters.len())
            .map(|local| {
                let slot = instance.map_or(local, |instance| instance.original_parameter(local));
                match program.store_route(identity, slot) {
                    StoreRoute::Fixed => Varying::Never,
                    StoreRoute::Changing => Varying::Always,
                    StoreRoute::Inherited if slot < ROUTE_BITS => Varying::Route(1 << slot),
                    StoreRoute::Inherited => Varying::Always,
                }
            })
            .collect();
        Self { varying }
    }

    fn of_local(&self, local: usize) -> Varying {
        self.varying.get(local).copied().unwrap_or(Varying::Never)
    }

    /// How a key read from these parameters is checked at its operation.
    pub(super) fn key(&self, key: &Expr) -> KeyCheck {
        let mut bits = 0;
        let mut always = false;
        key.walk(&mut |part| {
            if let ExprKind::Local(local) = part.kind {
                match self.of_local(local) {
                    Varying::Never => {}
                    Varying::Always => always = true,
                    Varying::Route(bit) => bits |= bit,
                }
            }
        });
        if always {
            KeyCheck::Always
        } else {
            KeyCheck::Unless(bits)
        }
    }
}

/// The route a value call gives the store it hands on (TIME-8): for each
/// parameter of the callee that can vary on some route, whether it does
/// on this one. Written only into a callee that has such a parameter.
pub(super) struct CallRoute {
    /// The positions of the callee's store parameters.
    stores: Vec<usize>,
    /// The bits set on every call written here.
    always: u64,
    /// Bits set where a bit of the caller's own route is.
    inherited: Vec<(u64, u64)>,
}

impl CallRoute {
    /// The route of a call of `callee` as `signature`, or as its instance
    /// `instance`, from code whose routes are `caller`: `None` in code
    /// that makes its own store, where nothing varies, and a callee that
    /// varies on no route.
    pub(super) fn of(
        program: &Program,
        callee: &Callee,
        instance: Option<&Instance>,
        signature: &Signature,
        arguments: &[Argument],
        caller: Option<&Routes>,
    ) -> Option<Self> {
        // Code that makes its own store hands it on with no bit set.
        let caller = caller?;
        if !program.has_store_routes() {
            return None;
        }
        let identity = callee.declaration()?;
        if !matches!(
            program.modules[identity.0].declarations[identity.1].kind,
            DeclarationKind::Function(Role::Value)
                | DeclarationKind::Implementation(Role::Value, _)
        ) {
            return None;
        }
        let store = program.capability_type("global_state")?;
        let stores: Vec<_> = signature
            .parameters
            .iter()
            .enumerate()
            .filter_map(|(index, parameter)| (parameter.ty == store).then_some(index))
            .collect();
        let routed: Vec<_> = (0..signature.parameters.len())
            .map(|local| instance.map_or(local, |instance| instance.original_parameter(local)))
            .filter(|&slot| {
                slot < ROUTE_BITS && program.store_route(identity, slot) == StoreRoute::Inherited
            })
            .collect();
        if stores.is_empty() || routed.is_empty() {
            return None;
        }
        let mut route = Self {
            stores,
            always: 0,
            inherited: Vec::new(),
        };
        for slot in routed {
            let bit = 1u64 << slot;
            // A slot left to its default is given the same value on each
            // call: it varies on no route.
            let Some(argument) = arguments
                .iter()
                .find(|argument| argument.parameter == Some(slot) && argument.pack.is_none())
            else {
                continue;
            };
            let mut from = 0;
            argument.value.walk(&mut |part| {
                if let ExprKind::Local(local) = part.kind {
                    match caller.of_local(local) {
                        Varying::Never => {}
                        Varying::Always => route.always |= bit,
                        Varying::Route(caller_bit) => from |= caller_bit,
                    }
                }
            });
            if route.always & bit == 0 && from != 0 {
                route.inherited.push((from, bit));
            }
        }
        Some(route)
    }

    /// Hand each store argument on with this call's route. A held
    /// argument is the caller's store, whose route the bits read.
    pub(super) fn forward(&self, given: &mut [String]) {
        for &index in &self.stores {
            let store = &given[index];
            let mut bits: Vec<_> = self
                .inherited
                .iter()
                .map(|(from, bit)| format!("({store}.varies({from:#x}ull) ? {bit:#x}ull : 0ull)"))
                .collect();
            if self.always != 0 || bits.is_empty() {
                bits.insert(0, format!("{:#x}ull", self.always));
            }
            given[index] = format!("{store}.routed({})", bits.join(" | "));
        }
    }
}
