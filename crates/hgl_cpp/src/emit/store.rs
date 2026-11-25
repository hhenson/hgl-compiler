//! The checks of the run's own store key (CAP-6, TIME-8). Every store
//! operation checks its key as it is evaluated. A node also checks, as the
//! SDK attaches it to a graph, the keys its configuration fixes (FN-1):
//! the earlier place for a key the graph fixes. Source checking is the
//! earlier place for a key the text fixes. Those places decide only when
//! a reserved key fails, never whether.

use std::cell::Cell;
use std::collections::HashSet;

use hgl_analyser::hir::{
    Argument, Body, Callee, Expr, ExprKind, Form, Node, Raise, Value, ValueCategory,
    walk_statement_nodes, walk_statements,
};
use hgl_analyser::{DeclarationKind, Detail, Parameter, Program, Role, Signature, Type};
use hgl_diagnostics::Location;
use hgl_lower::calls::{LeftOut, left_out};
use hgl_lower::instances::Instances;

use super::Emitter;
use super::names::{self, identifier};
use super::statements::Scope;
use super::values::INDENT;
use crate::runtime;

/// Store keys checked when the SDK attaches a node.
pub(super) struct Keys {
    /// Each key, written with the node's configuration.
    values: Vec<Expr>,
    /// Whether the node is core's recorder, which writes the run's key.
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
    /// left to its operation (TIME-8), as is one the source check proved,
    /// and one that cannot be written with the configuration.
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
        if let Some(projected) = project(program, key, bindings, remaining)
            && !source_safe(program, &projected)
        {
            self.values.push(projected);
        }
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
        // A route not followed leaves its keys to their operations.
        if spend(&self.remaining).is_none() || self.path.len() == 64 {
            return;
        }
        let instance = self.instances.find_call(program, callee, arguments);
        let (signature, body) = if let Some(instance) = instance {
            (&instance.signature, &instance.body)
        } else {
            let Some(body) = unit.bodies.get(declaration).and_then(Option::as_ref) else {
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
                if program.store_key_changes(identity, declared_slot)
                    || written.is_some_and(|argument| {
                        caller.changes(&argument.value)
                            || program.store_key_at_execution(&argument.value)
                    })
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

/// The C++ that hands `value` on to a store operation as its key, checked
/// there, as the operation is evaluated (TIME-8, CAP-6).
pub(super) fn checked(value: &str) -> String {
    let library = runtime::NAMESPACE;
    format!("::{library}::checked_store_key({value})")
}
