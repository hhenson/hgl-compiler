//! The keys of the global store across a whole program: one type for
//! each (TIME-8, core's CAP-6). A pass of the analyser, run once every
//! body of the program is typed: it reads the typed bodies and nothing
//! else.
//!
//! TIME-8: "All uses of a key bind the same exact ordinary type: a use
//! of another type in one program rejects (`store.key_type`)." The
//! catalogue says what a use is, and where the report goes: "a `put` or
//! a `get` of another type than an earlier use of the key has (TIME-8,
//! core's CAP-6); the later use".

use std::collections::HashMap;

use hgl_diagnostics::{Category, Diagnostics, Location};

use crate::hir::{Argument as CallArgument, Body, Callee, Expr, ExprKind, walk_statements};
use crate::program::Program;
use crate::types::{Argument, Type};
use crate::{codes, error};

/// One use of a key: the key, the type the use binds it to, and where
/// the use is written.
struct KeyUse {
    key: String,
    ty: Type,
    at: Location,
    made_at: Option<Location>,
}

/// Store uses made concrete while reading a callable instance.
#[derive(Default)]
pub(crate) struct KeyUses(Vec<KeyUse>);

impl KeyUses {
    pub(crate) fn in_instance(&mut self, program: &Program, body: &Body, made_at: Location) {
        crate::values::walk_body_calls(body, &mut |expression| {
            if let Some(found) = key_use(program, expression, Some(made_at)) {
                self.0.push(found);
            }
        });
    }
}

/// Compare the known keys and exact types of native get and put calls (TIME-8).
/// Uses in the original bodies and concrete instances are compared together.
pub(crate) fn key_types(
    program: &Program,
    KeyUses(instances): KeyUses,
    diagnostics: &mut Diagnostics,
) {
    let mut uses = Vec::new();
    let mut file_order = HashMap::new();
    for (module_index, module) in program.modules.iter().enumerate() {
        for (file_index, file) in module.files.iter().enumerate() {
            file_order.insert(file.file, (module_index, file_index));
        }
        let bodies = module
            .bodies
            .iter()
            .flatten()
            .chain(module.test_bodies.iter().flatten())
            .chain(&module.store_body);
        for body in bodies {
            walk_statements(&body.statements, &mut |expression| {
                if let Some(found) = key_use(program, expression, None) {
                    uses.push(found);
                }
            });
        }
    }
    let position = |at: Location| (file_order.get(&at.file).copied(), at.span.start);
    uses.extend(instances);
    uses.sort_by_key(|used| (position(used.at), used.made_at.map(position)));
    // The type each key has so far, and the use that gave it.
    let mut first: HashMap<&str, &KeyUse> = HashMap::new();
    let mut reported = std::collections::HashSet::new();
    for found in &uses {
        let Some(earlier) = first.get(found.key.as_str()).copied() else {
            first.insert(found.key.as_str(), found);
            continue;
        };
        if earlier.ty == found.ty {
            continue;
        }
        let at = if found.at == earlier.at {
            found.made_at.unwrap_or(found.at)
        } else {
            found.at
        };
        if !reported.insert((at, found.key.as_str())) {
            continue;
        }
        diagnostics.emit(
            error(
                Category::Injectable,
                codes::STORE_KEY_TYPE,
                at,
                format!(
                    "the key `\"{}\"` of the global store is used here as `{}`, and before as \
                     `{}`: a key has one type in a program",
                    found.key, found.ty, earlier.ty
                ),
                "TIME-8",
            )
            .with_related(earlier.at, "used here before"),
        );
    }
}

impl Program {
    /// Parameter slots a key reads, including reads reified as literals in an
    /// instance. Their lexical use locations retain the original provenance.
    pub fn store_input_slots(&self, value: &Expr, parameters: &[crate::Parameter]) -> Vec<usize> {
        let declared: HashMap<_, _> = parameters
            .iter()
            .enumerate()
            .map(|(slot, p)| (p.at, slot))
            .collect();
        let mut slots = std::collections::BTreeSet::new();
        value.walk(&mut |part| {
            if let Some(binding) = self
                .store_bindings()
                .get(&(part.at.file, part.at.span.start))
            {
                if let Some(slot) = declared.get(binding) {
                    slots.insert(*slot);
                }
            } else if let Some(slot) = declared.get(&part.at) {
                slots.insert(*slot);
            } else if let ExprKind::Local(slot) = part.kind
                && slot < parameters.len()
            {
                slots.insert(slot);
            }
        });
        slots.into_iter().collect()
    }

    /// Whether `const` parameter `slot` of `declaration` is one of a cycle
    /// whose store key can differ between its calls (TIME-8).
    pub fn store_key_changes(&self, declaration: (usize, usize), slot: usize) -> bool {
        self.changing_store_slots.contains(&(declaration, slot))
    }

    /// Whether this concrete call changes its key on a recursive route.
    /// Unread or bounded-out routes retain the declaration's classification.
    pub fn store_key_changes_in_call(
        &self,
        callee: &Callee,
        arguments: &[CallArgument],
        slot: usize,
    ) -> bool {
        self.store_route(callee, arguments).map_or_else(
            || {
                callee
                    .declaration()
                    .is_some_and(|declaration| self.store_key_changes(declaration, slot))
            },
            |route| route.changing.contains(&slot),
        )
    }

    /// Whether TIME-8 leaves this store key to a recursive operation: the
    /// key of an operation, or an argument that hands a helper with the
    /// store a key its cycle changes, in the declaration or an instance.
    pub fn store_key_at_execution(&self, key: &Expr) -> bool {
        self.runtime_store_keys.contains(&key.at)
    }

    /// A key the source checker can fix without evaluating value code.
    pub fn source_store_key(&self, key: &Expr) -> Option<String> {
        crate::const_inputs::known_store_key(self, key)
    }

    /// The key of a native CAP-6 operation, in its written argument order.
    pub fn store_key_argument<'a>(
        &self,
        callee: &Callee,
        arguments: &'a [CallArgument],
    ) -> Option<&'a Expr> {
        self.store_arguments(callee, arguments).map(|(_, key)| key)
    }

    /// The borrowed store and its key at a native CAP-6 operation.
    pub fn store_arguments<'a>(
        &self,
        callee: &Callee,
        arguments: &'a [CallArgument],
    ) -> Option<(&'a Expr, &'a Expr)> {
        let (module, declaration) = callee.declaration()?;
        if Some(module) != self.core() {
            return None;
        }
        let unit = &self.modules[module];
        if !store_operation(&unit.declarations[declaration].name) {
            return None;
        }
        let crate::types::Detail::Callable(signature) = unit.details.get(declaration)? else {
            return None;
        };
        let store = self.capability_type("global_state")?;
        let argument_for = |matches: &dyn Fn(&crate::Parameter) -> bool| {
            arguments.iter().find_map(|argument| {
                let parameter = signature.parameters.get(argument.parameter?)?;
                matches(parameter).then_some(&argument.value)
            })
        };
        Some((
            argument_for(&|parameter| parameter.ty == store)?,
            argument_for(&|parameter| parameter.constant && parameter.name == "key")?,
        ))
    }
}

/// The key of a Core store call whose candidate awaits concrete types.
pub(crate) fn deferred_key<'a>(
    program: &Program,
    call: &'a crate::values::Selecting,
) -> Option<&'a Expr> {
    let operator = call.operator;
    if Some(operator.module) != program.core() {
        return None;
    }
    let unit = &program.modules[operator.module];
    if !store_operation(&unit.declarations[operator.declaration].name) {
        return None;
    }
    let crate::types::Detail::Callable(signature) = unit.details.get(operator.declaration)? else {
        return None;
    };
    let store = signature
        .parameters
        .iter()
        .position(|p| p.name == "store")?;
    if call.argument_for(signature, store)?.ty != program.capability_type("global_state")? {
        return None;
    }
    let key = signature.parameters.iter().position(|p| p.name == "key")?;
    call.argument_for(signature, key)
}

fn store_operation(name: &str) -> bool {
    matches!(name, "put" | "get" | "has")
}

/// The native get or put's known key, exact payload type and source place.
fn key_use(program: &Program, expression: &Expr, made_at: Option<Location>) -> Option<KeyUse> {
    let ExprKind::Call {
        callee, arguments, ..
    } = &expression.kind
    else {
        return None;
    };
    let key = program.store_key_argument(callee, arguments)?;
    // `V`: the candidate's one generic parameter, as the call gives it.
    let ty = match callee {
        Callee::Function { generics, .. } | Callee::Candidate { generics, .. } => {
            match generics.first()? {
                Argument::Type(ty) => ty.clone(),
                Argument::Constant(_) | Argument::NoResult => return None,
            }
        }
        _ => return None,
    };
    let unknown = ty.any_part(
        &|part| matches!(part, Type::Unknown | Type::Parameter { .. }),
        &|_| false,
    );
    if unknown {
        return None;
    }
    Some(KeyUse {
        key: crate::const_inputs::known_store_key(program, key)?,
        ty,
        at: expression.at,
        made_at,
    })
}
