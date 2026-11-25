//! Fixes a node's store keys as the SDK attaches it to a graph (CAP-6, FN-1).

use std::cell::Cell;

use hgl_analyser::hir::{
    Argument, Body, Callee, Expr, ExprKind, Form, Node, Value, ValueCategory, walk_statement_nodes,
    walk_statements,
};
use hgl_analyser::{DeclarationKind, Detail, Program, Role, Signature, Type};
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
    unread: Option<Location>,
    pub(super) recorder: bool,
}

#[derive(PartialEq)]
enum BuildKey<'a> {
    Held(&'a Expr),
    Forwarded(Box<Expr>),
}

impl<'a> Keys<'a> {
    pub(super) fn of(emitter: &Emitter<'_>, declaration: usize, body: &'a Body) -> Self {
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
                if !matches!(key.kind, ExprKind::Literal(Value::Str(_))) {
                    keys.add(BuildKey::Held(key));
                }
            } else {
                let mut read = Vec::new();
                match trace.call(callee, arguments, None, *at, &mut read) {
                    Ok(()) => {
                        for key in read {
                            keys.add(BuildKey::Forwarded(Box::new(key)));
                        }
                    }
                    Err(at) => {
                        keys.unread.get_or_insert(at);
                    }
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
            .filter_map(|key| match key {
                BuildKey::Held(expression) => Some((*expression).clone()),
                BuildKey::Forwarded(_) => None,
            })
            .collect()
    }

    pub(super) fn slots(&self) -> std::ops::Range<usize> {
        0..self
            .values
            .iter()
            .filter(|key| matches!(key, BuildKey::Held(_)))
            .count()
    }

    pub(super) fn holds_values(&self) -> bool {
        !self.slots().is_empty()
    }

    pub(super) fn fields(&self) -> String {
        self.slots()
            .map(|slot| format!("{INDENT}{INDENT}::hgraph::Str {}{{}};\n", local(slot)))
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
        if let Some(at) = self.unread {
            emitter.unsupported(at, "a store key a hook passes through a value function without a fixed build-time expression");
            return None;
        }
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
            let (assignment, key) = match key {
                BuildKey::Held(key) => (
                    format!(
                        "prepared_.{} = ",
                        local(slots.next().expect("held key slot"))
                    ),
                    *key,
                ),
                BuildKey::Forwarded(key) => (String::new(), key.as_ref()),
            };
            let value = emitter.expression(&mut scope, key);
            let checked = checked(&value);
            let step = if assignment.is_empty() {
                format!("{indent}static_cast<void>({checked});\n")
            } else {
                format!("{indent}{assignment}{checked};\n")
            };
            checks.push_str(&emitter.debug_stamp(key.at, step));
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
    path: Vec<ForwardedCall>,
    remaining: Cell<usize>,
}

#[derive(PartialEq)]
struct ForwardedCall {
    declaration: (usize, usize),
    fixed: Vec<Option<Expr>>,
}

impl ForwardedKeys<'_> {
    fn call(
        &mut self,
        callee: &Callee,
        arguments: &[Argument],
        caller: Option<&[Option<Expr>]>,
        at: Location,
        keys: &mut Vec<Expr>,
    ) -> Result<(), Location> {
        let Some((module, declaration)) = callee.declaration() else {
            return Ok(());
        };
        let program = self.program;
        let unit = &program.modules[module];
        let Some(Detail::Callable(signature)) = unit.details.get(declaration) else {
            return Ok(());
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
            return Ok(());
        }
        spend(&self.remaining).ok_or(at)?;
        if self.path.len() == 64 {
            return Err(at);
        }
        let instance = self.instances.find_call(program, callee, arguments);
        let (signature, body) = if let Some(instance) = instance {
            (&instance.signature, &instance.body)
        } else {
            let body = unit
                .bodies
                .get(declaration)
                .and_then(Option::as_ref)
                .ok_or(at)?;
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
                    return from_caller(program, &written.value, caller, &self.remaining);
                }
                let slot = instance.map_or(slot, |instance| instance.original_parameter(slot));
                match left_out(program, callee, slot, parameter)? {
                    LeftOut::Default(default) => {
                        from_caller(program, &default, Some(&[]), &self.remaining)
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
        let call = ForwardedCall {
            declaration: (module, declaration),
            fixed,
        };
        if self.path.contains(&call) {
            return Ok(());
        }
        if self
            .path
            .iter()
            .any(|earlier| earlier.declaration == call.declaration)
        {
            return Err(at);
        }
        let fixed = call.fixed.clone();
        self.path.push(call);
        let mut result = Ok(());
        walk_statements(&body.statements, &mut |expression| {
            let ExprKind::Call {
                callee, arguments, ..
            } = &expression.kind
            else {
                return;
            };
            if result.is_err() {
                return;
            }
            result = if let Some(key) = program.store_key_argument(callee, arguments) {
                match from_caller(program, key, Some(&fixed), &self.remaining) {
                    Some(key) => {
                        keys.push(key);
                        Ok(())
                    }
                    None => Err(key.at),
                }
            } else {
                self.call(callee, arguments, Some(&fixed), expression.at, keys)
            };
        });
        self.path.pop();
        result
    }
}

/// Express a helper's fixed value using its caller's bindings. Only fixed reads
/// and Core's primitive operators may be repeated without running helper code.
fn from_caller(
    program: &Program,
    value: &Expr,
    fixed: Option<&[Option<Expr>]>,
    remaining: &Cell<usize>,
) -> Option<Expr> {
    spend(remaining)?;
    let read = |value: &Expr| from_caller(program, value, fixed, remaining);
    let kind = match &value.kind {
        ExprKind::Local(slot) => {
            return match fixed {
                Some(fixed) => {
                    let value = fixed.get(*slot)?.as_ref()?;
                    value.walk(&mut |_| {
                        let _ = spend(remaining);
                    });
                    (remaining.get() != 0).then(|| value.clone())
                }
                None => Some(value.clone()),
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
