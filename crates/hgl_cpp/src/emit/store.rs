//! Fixes a node's store keys as the SDK attaches it to a graph (CAP-6, FN-1).

use std::cell::Cell;

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

/// Store keys computed at graph construction and kept for the node's hooks.
#[derive(Default)]
pub(super) struct Keys<'a> {
    values: Vec<BuildKey<'a>>,
    pub(super) recorder: bool,
}

#[derive(PartialEq)]
struct BuildKey<'a> {
    /// The hook's own read when the checked value is held in its frame.
    held_for_hook: Option<&'a Expr>,
    projected: Box<Expr>,
}

impl<'a> Keys<'a> {
    pub(super) fn of(
        emitter: &Emitter<'_>,
        declaration: usize,
        signature: &Signature,
        body: &'a Body,
    ) -> Self {
        let program = emitter.program;
        let module = emitter.module;
        let recorder = program.core() == Some(module)
            && program.modules[module].declarations[declaration].name == "record";
        let mut keys = Self {
            recorder,
            ..Self::default()
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
                if !matches!(key.kind, ExprKind::Literal(Value::Str(_)))
                    && let Some(projected) = project(program, key, bindings, &trace.remaining)
                {
                    keys.add(BuildKey {
                        held_for_hook: Some(key),
                        projected: Box::new(projected),
                    });
                }
            } else {
                let mut read = Vec::new();
                trace.call(callee, arguments, bindings, *at, &mut read);
                for key in read {
                    keys.add(BuildKey {
                        held_for_hook: None,
                        projected: Box::new(key),
                    });
                }
            }
        });
        keys
    }

    fn add(&mut self, key: BuildKey<'a>) {
        if !self.values.contains(&key) {
            self.values.push(key);
        }
    }

    pub(super) fn expressions(&self) -> Vec<Expr> {
        self.values
            .iter()
            .filter_map(|key| key.held_for_hook.cloned())
            .collect()
    }

    pub(super) fn slots(&self) -> std::ops::Range<usize> {
        0..self
            .values
            .iter()
            .filter(|key| key.held_for_hook.is_some())
            .count()
    }

    pub(super) fn holds_values(&self) -> bool {
        !self.slots().is_empty()
    }

    pub(super) fn fields(&self) -> String {
        self.slots()
            .map(|slot| {
                let name = local(slot);
                format!("{INDENT}{INDENT}::std::optional<::hgraph::Str> {name}{{}};\n")
            })
            .collect()
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
        for (slot, parameter) in signature.parameters.iter().enumerate() {
            if !parameter.constant {
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
        let mut slots = self.slots();
        for key in &self.values {
            let value = emitter.expression(&mut scope, &key.projected);
            let library = runtime::NAMESPACE;
            let at = emitter.source_location(key.projected.at);
            let checked = format!(
                "::{library}::prepare_store_key([&]() -> ::hgraph::Str {{ return {value}; }}, {at})"
            );
            let step = if key.held_for_hook.is_some() {
                let name = local(slots.next().expect("held key slot"));
                format!("{indent}prepared_.{name} = {checked};\n")
            } else {
                format!("{indent}static_cast<void>({checked});\n")
            };
            checks.push_str(&emitter.debug_stamp(key.projected.at, step));
        }
        let (state, keep) = if self.holds_values() {
            (
                format!("{indent}state_type_ prepared_{{}};\n"),
                format!(
                    "{indent}node_.replace_state(::hgraph::Value{{::std::move(prepared_)}});\n"
                ),
            )
        } else {
            (String::new(), String::new())
        };
        Some(format!(
            "\n{INDENT}static void prepare([[maybe_unused]] const ::hgraph::NodeView& node_)\n{INDENT}{{\n{state}{checks}{keep}{INDENT}}}\n"
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

#[derive(Clone, Copy)]
enum Bindings<'a> {
    Configuration(&'a [Parameter]),
    Given(&'a [Option<Expr>]),
}

impl ForwardedKeys<'_> {
    fn call(
        &mut self,
        callee: &Callee,
        arguments: &[Argument],
        caller: Bindings<'_>,
        at: Location,
        keys: &mut Vec<Expr>,
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
        if spend(&self.remaining).is_none()
            || self.path.len() == 64
            || self.path.contains(&identity)
        {
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
                    return None;
                }
                let written = arguments.iter().find(|argument| {
                    instance.map_or(argument.parameter, |instance| instance.parameter(argument))
                        == Some(slot)
                });
                if let Some(written) = written {
                    return project(program, &written.value, caller, &self.remaining);
                }
                let slot = instance.map_or(slot, |instance| instance.original_parameter(slot));
                match left_out(program, callee, slot, parameter)? {
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
                if let Some(key) = project(program, key, bindings, &self.remaining) {
                    keys.push(key);
                }
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
                    let value = fixed.get(*slot)?.as_ref()?;
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

pub(super) fn local(slot: usize) -> String {
    names::temporary("store_key", slot)
}

pub(super) fn checked(value: &str) -> String {
    format!("::{}::checked_store_key({value})", runtime::NAMESPACE)
}
