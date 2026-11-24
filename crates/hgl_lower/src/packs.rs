//! Concrete parameter lists for instances with packs or function
//! parameters (FN-8, GEN-10, FN-17). Lowering gives each captured
//! argument its own runtime slot and removes compile-time function slots.

use hgl_analyser::hir::{
    Argument, Body, Callee, ExprKind, LocalId, NodeMut, PackInput, PackKey, Stmt,
    walk_statement_nodes_mut,
};
use hgl_analyser::{Program, Signature, Type};
use hgl_diagnostics::Location;

use crate::canonical::canonical;

/// A concrete parameter's origin in the unexpanded call.
#[derive(PartialEq, Eq)]
pub(crate) struct Slot {
    /// The position in the original signature.
    pub(crate) parameter: usize,
    pack: Option<PackKey>,
}

impl Slot {
    /// Whether this parameter takes the analysed argument.
    pub(crate) fn takes(&self, argument: &Argument) -> bool {
        Some(self.parameter) == argument.parameter && self.pack == argument.pack
    }
}

/// The pack captures of one call in the order in which it gives them.
#[derive(Default)]
pub(crate) struct Captures {
    entries: Vec<(Slot, Type)>,
}

impl Captures {
    /// Read a call's packed arguments. A trace supplies the type it replays.
    pub(crate) fn of(
        program: &Program,
        callee: &Callee,
        signature: &Signature,
        arguments: &[Argument],
    ) -> Self {
        let entries = program
            .call_pack_inputs(callee, signature, arguments)
            .into_iter()
            .map(|PackInput { parameter, key, ty }| {
                (
                    Slot {
                        parameter,
                        pack: Some(key),
                    },
                    ty,
                )
            })
            .collect();
        Self { entries }
    }

    /// The analyzer's input shape for an instance that forwards a pack (FN-19).
    pub(crate) fn inputs(&self) -> Vec<PackInput> {
        self.entries
            .iter()
            .filter_map(|(slot, ty)| {
                Some(PackInput {
                    parameter: slot.parameter,
                    key: slot.pack.clone()?,
                    ty: ty.clone(),
                })
            })
            .collect()
    }

    /// Types and keys of the captures distinguish instances of both
    /// homogeneous and heterogeneous packs.
    pub(crate) fn key(&self, program: &Program) -> Option<Vec<String>> {
        self.entries
            .iter()
            .map(|(slot, ty)| {
                let captured = match &slot.pack {
                    Some(PackKey::Position(index)) => index.to_string(),
                    Some(PackKey::Name(name)) => name.clone(),
                    None => return None,
                };
                Some(format!(
                    "{}[{captured}]:{}",
                    slot.parameter,
                    canonical(program, ty)?
                ))
            })
            .collect()
    }

    /// The endpoint or value type bound to a captured argument.
    pub(crate) fn ty(&self, argument: &Argument) -> Option<&Type> {
        self.entries
            .iter()
            .find_map(|(slot, ty)| slot.takes(argument).then_some(ty))
    }

    /// Expand the packed inputs, remove the static inputs and remap
    /// local identities. A use of a pack itself still needs a typed
    /// pack expression or reflection.
    /// Ordinary slots keep their places relative to the expanded pack,
    /// including a `join` separator after its pieces (OPS-11).
    pub(crate) fn expand(
        self,
        signature: &mut Signature,
        body: &mut Body,
    ) -> (Option<Vec<Slot>>, Option<Location>) {
        let packed: Vec<_> = signature
            .parameters
            .iter()
            .map(|p| p.pack.is_some())
            .collect();
        let functions: Vec<_> = signature
            .parameters
            .iter()
            .map(|p| matches!(p.ty, Type::Function(_)))
            .collect();
        if !packed.contains(&true) && !functions.contains(&true) {
            return (None, None);
        }
        let original = std::mem::take(&mut body.locals);
        let mut remapped = vec![0; original.len()];
        let mut inputs = vec![Vec::new(); packed.len()];
        let mut slots = Vec::new();
        let parameters = std::mem::take(&mut signature.parameters);
        for (index, parameter) in parameters.into_iter().enumerate() {
            if functions[index] {
                continue;
            }
            if parameter.pack.is_none() {
                remapped[index] = body.locals.len();
                inputs[index].push(body.locals.len());
                body.locals.push(original[index].clone());
                slots.push(Slot {
                    parameter: index,
                    pack: None,
                });
                signature.parameters.push(parameter);
                continue;
            }
            for (offset, (slot, ty)) in self
                .entries
                .iter()
                .filter(|(slot, _)| slot.parameter == index)
                .enumerate()
            {
                let mut name = match &slot.pack {
                    Some(PackKey::Name(key)) => format!("pack_{index}_{offset}_{key}"),
                    _ => format!("pack_{index}_{offset}"),
                };
                while original.iter().any(|local| local.name == name) {
                    name.push('_');
                }
                let mut local = original[index].clone();
                local.ty = ty.clone();
                local.name = name.clone();
                inputs[index].push(body.locals.len());
                body.locals.push(local);
                slots.push(Slot {
                    parameter: index,
                    pack: slot.pack.clone(),
                });
                let mut expanded = parameter.clone();
                expanded.name = name;
                expanded.ty = ty.clone();
                expanded.pack = None;
                signature.parameters.push(expanded);
            }
        }
        // Retain unresolved pack and function bindings with their source
        // locations for the diagnostic if a value still reads one.
        for (index, local) in original.into_iter().enumerate() {
            if packed.get(index) != Some(&false) || functions.get(index) == Some(&true) {
                remapped[index] = body.locals.len();
                body.locals.push(local);
            }
        }

        let mut unsupported = None;
        let change = |local: &mut LocalId| *local = remapped[*local];
        // Each runtime slot and its input local were added together.
        let entry = |pack, key: &PackKey| {
            slots
                .iter()
                .position(|slot| slot.parameter == pack && slot.pack.as_ref() == Some(key))
        };
        walk_statement_nodes_mut(&mut body.statements, &mut |node| match node {
            NodeMut::Statement(statement) => match statement {
                Stmt::Declare { local, .. }
                | Stmt::Assign { local, .. }
                | Stmt::Inject { local, .. } => change(local),
                Stmt::For { names, .. } => names.iter_mut().for_each(change),
                Stmt::Hook {
                    header: Some(header),
                    at,
                    ..
                } => {
                    for (selected, packed) in [
                        (&mut header.subscribed, &mut header.subscribed_entries),
                        (&mut header.required, &mut header.required_entries),
                    ] {
                        *selected = selected
                            .iter()
                            .flat_map(|&local| {
                                inputs
                                    .get(local)
                                    .cloned()
                                    .unwrap_or_else(|| vec![remapped[local]])
                            })
                            .collect();
                        for input in std::mem::take(packed) {
                            if let Some(local) =
                                input.key.as_ref().and_then(|key| entry(input.pack, key))
                            {
                                if !selected.contains(&local) {
                                    selected.push(local);
                                }
                            } else {
                                unsupported.get_or_insert(*at);
                            }
                        }
                    }
                }
                _ => {}
            },
            NodeMut::Expression(expression) => {
                let read = |local: &mut LocalId, unsupported: &mut Option<Location>| {
                    if packed.get(*local) == Some(&true) {
                        unsupported.get_or_insert(expression.at);
                    }
                    change(local);
                };
                let mut remap_captures = |callee: &mut Callee| {
                    if let Callee::Anonymous(function) = callee {
                        for index in 0..function.captures.len() {
                            let capture = function.captures[index];
                            function.captures[index] = match function.capture_pack_key(index) {
                                Some(key) => entry(capture, key).unwrap_or_else(|| {
                                    unsupported.get_or_insert(expression.at);
                                    remapped[capture]
                                }),
                                None => remapped[capture],
                            };
                        }
                        function.pack_captures.clear();
                    }
                };
                match &mut expression.kind {
                    ExprKind::Call { callee, .. } => remap_captures(callee),
                    ExprKind::StaticCallable(callee) => remap_captures(callee),
                    ExprKind::PackEntry { pack, key } => {
                        if let Some(local) = entry(*pack, key) {
                            expression.kind = ExprKind::Local(local);
                        } else {
                            unsupported.get_or_insert(expression.at);
                        }
                    }
                    ExprKind::PackedEndpoint { operation, inputs } => {
                        let locals = inputs
                            .iter()
                            .map(|input| match &input.kind {
                                ExprKind::PackEntry { pack, key } => entry(*pack, key),
                                ExprKind::EndpointOf(local) => Some(remapped[*local]),
                                _ => None,
                            })
                            .collect::<Option<Vec<_>>>();
                        if let Some(locals) = locals {
                            expression.kind = ExprKind::Endpoint {
                                operation: *operation,
                                inputs: Some(locals),
                            };
                        } else {
                            unsupported.get_or_insert(expression.at);
                        }
                    }
                    ExprKind::ListComprehension { binding, .. } => change(binding),
                    ExprKind::Local(local)
                    | ExprKind::EndpointOf(local)
                    | ExprKind::BoundChild(local) => {
                        read(local, &mut unsupported);
                    }
                    ExprKind::Endpoint {
                        inputs: Some(locals),
                        ..
                    } => {
                        locals
                            .iter_mut()
                            .for_each(|local| read(local, &mut unsupported));
                    }
                    ExprKind::Eval {
                        inherited: Some(local),
                        ..
                    } => change(local),
                    _ => {}
                }
            }
        });
        (Some(slots), unsupported)
    }
}
