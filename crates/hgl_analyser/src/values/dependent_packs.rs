//! Compile-time matching of a dependent argument pack (FN-8, GEN-17).

use std::cell::{Cell, RefCell};

use hgl_diagnostics::{Category, Location};
use hgl_parser::{HasSpan, SyntaxNode, SyntaxToken};

use super::Typer;
use super::arguments::{Assigned, Slot, Takes};
use super::candidates::Given;
use super::function_arguments::CallbackInputs;
use super::inference::{pack_bounds, type_pack};
use crate::declared::{substitute_with_packs, weight};
use crate::hir::{Callee, Expr, ExprKind, PackKey, ReflectedInputs, TypePackArgument};
use crate::syntax::child;
use crate::types::{
    Argument as GenericArgument, Constant, ConstantValue, Detail, FunctionType, GenericParameter,
    Pack, Signature, Type,
};
use crate::{Module, Program};
use crate::{codes, error};

/// Generic type packs read in the scope of one specialization.
pub(crate) struct PackTypes<'p> {
    bindings: Vec<(Location, &'p TypePackArgument)>,
    heaviest: usize,
    sequence_weight: usize,
}

impl<'p> PackTypes<'p> {
    /// The lexical declaration and concrete pack of each generic slot.
    pub(super) fn bindings(&self) -> &[(Location, &'p TypePackArgument)] {
        &self.bindings
    }

    /// Match resolved packs to their declarations, by generic slot.
    pub(crate) fn new(generics: &[GenericParameter], packs: &'p [TypePackArgument]) -> Self {
        Self {
            bindings: packs
                .iter()
                .filter_map(|pack| Some((generics.get(pack.parameter)?.at, pack)))
                .collect(),
            heaviest: packs
                .iter()
                .flat_map(|pack| &pack.types)
                .map(weight)
                .max()
                .unwrap_or(1),
            sequence_weight: packs
                .iter()
                .map(|pack| {
                    pack.types
                        .iter()
                        .fold(1usize, |sum, ty| sum.saturating_add(weight(ty)))
                })
                .max()
                .unwrap_or(1),
        }
    }

    /// Put the enclosing compile-time inputs into a passed-on pack (GEN-17).
    pub(crate) fn place_forwarded(&self, pack: &mut TypePackArgument) {
        if !pack.tuple_sources.is_empty() {
            let sources = std::mem::take(&mut pack.tuple_sources);
            let mut unresolved = Vec::new();
            let mut types = Vec::new();
            for ty in std::mem::take(&mut pack.types) {
                let source = match &ty {
                    Type::Parameter { at, .. } if sources.contains(at) => Some(*at),
                    _ => None,
                };
                let supplied = source.and_then(|source| {
                    self.bindings
                        .iter()
                        .find(|(at, _)| *at == source)
                        .map(|(_, given)| *given)
                });
                if let Some(given) = supplied
                    && (given.forwarded_from.is_none() || !given.tuple_sources.is_empty())
                {
                    types.extend(given.types.iter().cloned());
                    for source in &given.tuple_sources {
                        if !unresolved.contains(source) {
                            unresolved.push(*source);
                        }
                    }
                } else {
                    if let Some(source) = source
                        && !unresolved.contains(&source)
                    {
                        unresolved.push(source);
                    }
                    types.push(ty);
                }
            }
            pack.keys = (0..types.len())
                .map(crate::hir::PackKey::Position)
                .collect();
            pack.types = types;
            pack.forwarded_from = unresolved.first().copied();
            pack.tuple_sources = unresolved;
            return;
        }
        let Some(source) = pack.forwarded_from else {
            return;
        };
        if let Some((_, given)) = self.bindings.iter().find(|(at, _)| *at == source) {
            if let (Some(receiving), Some(original)) = (&mut pack.reflected, &given.reflected) {
                // The original fixed inputs and the reflected suffix are
                // one sequence. GEN-17 takes the suffix left by this template.
                let mut inputs = original.prefix.iter().cloned().chain(
                    given
                        .keys
                        .iter()
                        .cloned()
                        .zip(given.types.iter().cloned())
                        .map(|(key, ty)| (Some(key), ty)),
                );
                receiving.prefix = inputs.by_ref().take(receiving.fixed).collect();
                (pack.keys, pack.types) = inputs.filter_map(|(key, ty)| Some((key?, ty))).unzip();
            } else {
                pack.keys.extend(given.keys.iter().cloned());
                pack.types.extend(given.types.iter().cloned());
            }
            pack.forwarded_from = given.forwarded_from;
        }
    }

    /// Specialize a written type, including its GEN-17 member types.
    pub(crate) fn project(
        &self,
        ty: &Type,
        map: &[(Location, &GenericArgument)],
    ) -> Result<Type, MemberFault> {
        let fault = RefCell::new(None);
        let member = |at, name: &str, key: &Constant| {
            let (_, pack) = self.bindings.iter().find(|(declared, _)| *declared == at)?;
            let fault_here = match &key.value {
                ConstantValue::Str(_) | ConstantValue::Int(_) => {
                    if let Some(ty) = pack.member(&key.value) {
                        return Some(ty.clone());
                    }
                    if pack.forwarded_from.is_some() {
                        return None;
                    }
                    MemberFault::Missing {
                        pack: name.to_string(),
                        key: key.clone(),
                    }
                }
                ConstantValue::Unknown
                | ConstantValue::Unevaluated
                | ConstantValue::Parameter { .. }
                | ConstantValue::Named { .. }
                | ConstantValue::Constructed { .. }
                | ConstantValue::Unary(..)
                | ConstantValue::Binary(..) => return None,
                _ => MemberFault::KeyKind { key: key.clone() },
            };
            fault.borrow_mut().get_or_insert(fault_here);
            Some(Type::Unknown)
        };
        let untold_sequence = Cell::new(false);
        let sequence = |at| {
            let (_, pack) = self.bindings.iter().find(|(declared, _)| *declared == at)?;
            let types = pack.types.clone();
            if let Some(source) = pack.forwarded_from
                && !types
                    .iter()
                    .any(|ty| matches!(ty, Type::Parameter { at, .. } if *at == source))
            {
                // A reflected callback whose suffix is not yet supplied
                // has no tuple arity. Keep the entire type unresolved.
                untold_sequence.set(true);
                return Some(Vec::new());
            }
            Some(types)
        };
        // Only a tuple expansion spends the sequence's combined weight;
        // a lookup of one reflected member keeps its existing bound.
        let expands = ty.any_part(
            &|part| {
                matches!(part, Type::Tuple(elements) if elements.iter()
            .any(|element| matches!(element, Type::Parameter { at, .. }
                if self.bindings.iter().any(|(bound, _)| bound == at))))
            },
            &|_| false,
        );
        let bound = if expands {
            self.heaviest.max(self.sequence_weight)
        } else {
            self.heaviest
        };
        let ty = substitute_with_packs(ty, map, &member, &sequence, bound);
        fault.into_inner().map_or(
            Ok(if untold_sequence.get() {
                Type::Unknown
            } else {
                ty
            }),
            Err,
        )
    }

    /// A known key must exist when a declaration writes its member type.
    /// Requirements evaluate their own member lookups conditionally.
    pub(super) fn written_member_fault(
        &self,
        unit: &Module,
        declaration: usize,
        map: &[(Location, &GenericArgument)],
    ) -> Option<MemberFault> {
        if self.bindings.is_empty() {
            return None;
        }
        let declared = &unit.declarations[declaration];
        let contains = |outer: hgl_source::Span, inner: hgl_source::Span| {
            outer.start <= inner.start && inner.end <= outer.end
        };
        let span = declared.syntax.span();
        let requires = child(&declared.syntax, hgl_parser::SyntaxKind::RequiresClause);
        let in_body = |at: Location| {
            at.file == declared.at.file
                && contains(span, at.span)
                && !requires
                    .as_ref()
                    .is_some_and(|condition| contains(condition.span(), at.span))
        };
        let written = unit
            .written
            .iter()
            .filter(|written| in_body(written.at))
            .map(|written| &written.ty);
        // GEN-8 writes the call's type arguments in the separate call table.
        let call_types = unit
            .call_arguments
            .iter()
            .filter(|call| in_body(call.at))
            .flat_map(|call| call.arguments.iter())
            .filter_map(|(_, argument)| match argument {
                GenericArgument::Type(ty) => Some(ty),
                GenericArgument::Constant(_) | GenericArgument::NoResult => None,
            });
        written
            .chain(call_types)
            .filter(|ty| ty.any_part(&|part| matches!(part, Type::PackMember { .. }), &|_| false))
            .find_map(|ty| self.project(ty, map).err())
    }
}

/// A compile-time pack member lookup failed (GEN-17).
#[derive(Debug)]
pub(crate) enum MemberFault {
    /// The known pack has no member under this key.
    Missing {
        /// The pack's declared name.
        pack: String,
        /// The requested key.
        key: Constant,
    },
    /// The constant has neither of the permitted key types.
    KeyKind {
        /// The constant given as a key.
        key: Constant,
    },
}

/// The named pack declared for one of this declaration's callbacks (FN-8).
pub(super) fn dependent_slot(module: &Module, signature: &Signature) -> Option<usize> {
    signature
        .parameters
        .iter()
        .position(|parameter| module.dependent_packs.contains_key(&parameter.at))
}

/// Whether a declaration binds packs from its static function arguments.
pub(super) fn reflects(module: &Module, signature: &Signature) -> bool {
    dependent_slot(module, signature).is_some()
        || signature.parameters.iter().any(|parameter| {
            matches!(&parameter.ty, Type::Function(template) if template.named_pack.is_some())
        })
}

/// The packs and callback specializations an operator's call supplies.
pub(super) struct OperatorPacks {
    pub(super) packs: Vec<TypePackArgument>,
    /// Each supplied callback's own typed pack, by written argument position.
    pub(super) callbacks: Vec<(usize, TypePackArgument)>,
}

/// A mismatch of an operator's public dependent call.
pub(super) struct OperatorFault {
    pub(super) callback: usize,
    pub(super) supplied: Vec<usize>,
    pub(super) fault: Fault,
}

/// FN-8 and GEN-17: add pack keys to an operator or candidate after its
/// ordinary inputs and compile-time options have been resolved.
pub(super) fn operator_packs(
    program: &Program,
    (module, signature): (usize, &Signature),
    generics: &[GenericArgument],
    given: &[Given],
    slots: &[Option<usize>],
) -> Result<OperatorPacks, OperatorFault> {
    let declared = &program.modules[module];
    let dependent = dependent_slot(declared, signature);
    let metadata =
        dependent.map(|index| &declared.dependent_packs[&signature.parameters[index].at]);
    let mut result = OperatorPacks {
        packs: Vec::new(),
        callbacks: Vec::new(),
    };
    for (index, parameter) in signature.parameters.iter().enumerate() {
        let Type::Function(template) = &parameter.ty else {
            continue;
        };
        let matching = metadata.filter(|pack| pack.function == parameter.at);
        if template.named_pack.is_none() && matching.is_none() {
            continue;
        }
        let Some(callback) = slots.iter().position(|slot| *slot == Some(index)) else {
            continue;
        };
        let value = &given[callback].value;
        let mut reflected = reflect(template, value);
        if let Some(metadata) = matching
            && let Some(dependent) = dependent
        {
            let supplied: Vec<_> = slots
                .iter()
                .enumerate()
                .filter_map(|(index, slot)| (*slot == Some(dependent)).then_some(index))
                .collect();
            let arguments: Vec<_> = supplied
                .iter()
                .map(|&index| (given[index].key.name(), &given[index].value))
                .collect();
            let without: Vec<_> = metadata
                .without
                .iter()
                .filter_map(|at| {
                    let index = signature
                        .generics
                        .iter()
                        .position(|generic| generic.at == *at)?;
                    let GenericArgument::Constant(value) = generics.get(index)? else {
                        return None;
                    };
                    Some(exclusion_names(&value.value))
                })
                .flatten()
                .collect();
            let bound = bind(program, template, value, &arguments, &without).map_err(|fault| {
                OperatorFault {
                    callback,
                    supplied,
                    fault,
                }
            })?;
            reflected = bound.reflected;
            if let Some(pack) = bound.own_named {
                result.callbacks.push((callback, pack));
            }
            if let Some(generic) = type_pack(&signature.parameters[dependent], &signature.generics)
            {
                result
                    .packs
                    .push(TypePackArgument::keyed(generic, bound.captured));
            }
        }
        if let Some(reflects) = &template.named_pack
            && let Some(index) = signature
                .generics
                .iter()
                .position(|generic| generic.at == reflects.at)
        {
            let bounds = reflected_bounds(reflects, &signature.generics[index]);
            let count = reflected.len() as u64;
            if count < bounds.0 || bounds.1.is_some_and(|maximum| count > maximum) {
                return Err(OperatorFault {
                    callback,
                    supplied: Vec::new(),
                    fault: Fault::Cardinality {
                        parameter: index,
                        count,
                        bounds,
                        excess: None,
                    },
                });
            }
            result
                .packs
                .push(reflected_argument(index, template, value, reflected));
        }
    }
    Ok(result)
}

/// The bounds from both the declared generic and the reflecting function type.
fn reflected_bounds(
    reflects: &crate::ReflectedPack,
    generic: &GenericParameter,
) -> (u64, Option<u64>) {
    let (minimum, maximum) = reflects.bounds;
    match generic.kind {
        crate::GenericKind::Pack {
            minimum: lower,
            maximum: upper,
        } => (minimum.max(lower), maximum.into_iter().chain(upper).min()),
        _ => reflects.bounds,
    }
}

impl Typer<'_> {
    /// Read callback identity and dependent keys before fitting arguments
    /// whose written types themselves use a pack member (GEN-17).
    pub(super) fn read_dependent_call(
        &mut self,
        (name, list): (&SyntaxToken, &SyntaxNode),
        (module, signature, dependent): (usize, &Signature, Option<usize>),
        (slots, map): (&[Slot<'_>], &[(Location, &GenericArgument)]),
        (assigned, values): (&mut [Assigned], &mut [Option<Expr>]),
    ) -> Vec<TypePackArgument> {
        let program = self.program;
        let declared = &program.modules[module];
        // The computed entries can still fill this callback's inputs.
        // Its written reference can be read against its template.
        let dependent = dependent
            .filter(|_| !self.deferred_computed_spread(list))
            .map(|slot| {
                let parameter = &signature.parameters[slot];
                (slot, &declared.dependent_packs[&parameter.at])
            });
        let mut packs = Vec::new();
        for (index, parameter) in signature.parameters.iter().enumerate() {
            let Type::Function(template) = &parameter.ty else {
                continue;
            };
            let for_this = dependent.filter(|(_, pack)| pack.function == parameter.at);
            if template.named_pack.is_none() && for_this.is_none() {
                continue;
            }
            let Some(position) = assigned.iter().position(|entry| entry.slot == Some(index)) else {
                continue;
            };
            let Type::Function(template) = crate::declared::substitute_within(&parameter.ty, map)
            else {
                continue;
            };
            let mut supplied = Vec::new();
            let mut excluded = Vec::new();
            if let Some((slot, declaration)) = for_this {
                let mark = self.mark();
                supplied.extend(
                    assigned
                        .iter()
                        .enumerate()
                        .filter(|(_, entry)| entry.slot == Some(slot))
                        .map(|(position, entry)| {
                            let value = match &entry.forwarded {
                                Some((_, value)) => value.clone(),
                                None => {
                                    self.argument(
                                        Some(slot),
                                        &entry.value,
                                        Some((&slots[slot], false)),
                                        Takes::Parameters,
                                    )
                                    .value
                                }
                            };
                            (position, value)
                        }),
                );
                self.take_back(mark, false);
                excluded = pack_exclusions(declaration, map);
            }
            let arguments: Vec<_> = supplied
                .iter()
                .map(|(offset, value)| (assigned[*offset].name_text(), value))
                .collect();
            let mut callback = self.function_argument_with_inputs(
                &assigned[position].value,
                &template,
                &CallbackInputs {
                    supplied: for_this.is_some().then_some(&arguments),
                    excluded: &excluded,
                },
            );
            let mut reflected = reflect(&template, &callback);
            if let Some((slot, _)) = for_this {
                match bind(program, &template, &callback, &arguments, &excluded) {
                    Ok(bound) => {
                        reflected = bound.reflected;
                        if let Some(slot) =
                            type_pack(&signature.parameters[slot], &signature.generics)
                        {
                            packs.push(TypePackArgument::keyed(slot, bound.captured.clone()));
                        }
                        for ((offset, _), (key, _)) in supplied.iter().zip(bound.captured) {
                            if let Some((_, recorded)) = &mut assigned[*offset].pack {
                                *recorded = key;
                            }
                        }
                        let own_named = bound.own_named;
                        specialize_named_pack(&mut callback, own_named);
                    }
                    Err(Fault::Deferred) => {}
                    Err(fault) => {
                        let sites: Vec<_> = supplied
                            .iter()
                            .map(|(offset, _)| {
                                let entry = &assigned[*offset];
                                let at = Location::new(
                                    self.file,
                                    entry
                                        .name
                                        .as_ref()
                                        .map_or_else(|| entry.value.span(), HasSpan::span),
                                );
                                (at, entry.name_text())
                            })
                            .collect();
                        self.dependent_fault((name, list), &callback, &sites, &fault)
                    }
                }
            }
            if let Some(reflects) = &template.named_pack
                && let Some(slot) = signature
                    .generics
                    .iter()
                    .position(|generic| generic.at == reflects.at)
            {
                let bounds = reflected_bounds(reflects, &signature.generics[slot]);
                let mut reflected = reflected_argument(slot, &template, &callback, reflected);
                if let Type::Function(actual) = &callback.ty {
                    reflected.forwarded_from = actual.named_pack.as_ref().map(|pack| pack.at);
                }
                if reflected.forwarded_from.is_none() {
                    self.check_pack_count(
                        list,
                        &reflects.name,
                        bounds,
                        &vec![callback.at; reflected.types.len()],
                    );
                }
                packs.push(reflected);
            }
            values[position] = Some(callback);
        }
        packs.sort_by_key(|pack| pack.parameter);
        packs
    }

    /// Check reflected bounds and written member types now that the
    /// caller has supplied the pack's keys (GEN-10, GEN-17).
    pub(super) fn check_forwarded_packs(
        &mut self,
        at: Location,
        (module, declaration): (usize, usize),
        arguments: &[GenericArgument],
        packs: &[TypePackArgument],
    ) {
        let unit = &self.program.modules[module];
        let Detail::Callable(signature) = &unit.details[declaration] else {
            return;
        };
        let before = self.diagnostics.len();
        for parameter in &signature.parameters {
            let Type::Function(template) = &parameter.ty else {
                continue;
            };
            let Some(reflects) = &template.named_pack else {
                continue;
            };
            for pack in packs.iter().filter(|pack| pack.forwarded_from.is_none()) {
                let generic = &signature.generics[pack.parameter];
                if generic.at == reflects.at {
                    self.check_pack_count_at(
                        at,
                        &reflects.name,
                        reflected_bounds(reflects, generic),
                        pack.types.len() as u64,
                    );
                }
            }
        }
        if self.diagnostics.len() == before {
            let map: Vec<_> = signature
                .generics
                .iter()
                .map(|generic| generic.at)
                .zip(arguments)
                .collect();
            let reflected = PackTypes::new(&signature.generics, packs);
            if let Some(fault) = reflected.written_member_fault(unit, declaration, &map) {
                self.pack_member_fault(at, &fault);
            }
        }
    }

    /// Report a failed lookup after resolving a pack member's key (GEN-17).
    pub(super) fn pack_member_fault(&mut self, at: Location, fault: &MemberFault) {
        let (code, message) = match fault {
            MemberFault::Missing { pack, key } => (
                codes::PACK_NAME,
                format!("pack `{pack}` has no member under `{key}`"),
            ),
            MemberFault::KeyKind { key } => (
                codes::TYPE_MISMATCH,
                format!("`{key}` must be a compile-time `str` or `i64` pack key"),
            ),
        };
        self.diagnostics
            .emit(error(Category::Type, code, at, message, "GEN-17"));
    }

    /// Report a failure in the operator's public dependent call shape.
    pub(super) fn operator_pack_fault(
        &mut self,
        call: (&SyntaxToken, &SyntaxNode),
        given: &[Given],
        fault: &OperatorFault,
    ) {
        let sites: Vec<_> = fault
            .supplied
            .iter()
            .map(|&index| (given[index].value.at, given[index].key.name()))
            .collect();
        self.dependent_fault(call, &given[fault.callback].value, &sites, &fault.fault);
    }

    /// Report callback matching problems at their original argument.
    fn dependent_fault(
        &mut self,
        (name, list): (&SyntaxToken, &SyntaxNode),
        callback: &Expr,
        supplied: &[(Location, Option<&str>)],
        fault: &Fault,
    ) {
        let call = Location::new(self.file, name.span());
        let closing = Location::new(
            self.file,
            hgl_source::Span::new(list.span().end - 1, list.span().end),
        );
        let at = |index: usize| supplied[index].0;
        let given_name = |index: usize| supplied[index].1.unwrap_or("argument");
        let parameter = |index| match &callback.ty {
            Type::Function(actual) => actual
                .parameters
                .get(index)
                .and_then(crate::FunctionParameter::name)
                .unwrap_or("argument"),
            _ => "argument",
        };
        let report = match *fault {
            Fault::Deferred => return,
            Fault::Order(index)
                if self.diagnostics.as_slice().iter().any(|reported| {
                    reported.primary == at(index) && reported.code.as_str() == codes::ARGUMENT_ORDER
                }) =>
            {
                return;
            }
            Fault::Order(index) => error(
                Category::Type,
                codes::ARGUMENT_ORDER,
                at(index),
                "an argument with no name follows a named one".into(),
                "EXPR-2",
            ),
            Fault::Unexpected(index) => error(
                Category::Type,
                codes::ARGUMENT_UNEXPECTED,
                at(index),
                "the callback has no parameter for this positional argument".into(),
                "FN-8",
            ),
            Fault::Unknown(index) => error(
                Category::Name,
                codes::ARGUMENT_UNKNOWN,
                at(index),
                format!(
                    "the callback has no available parameter `{}`",
                    given_name(index)
                ),
                "FN-8",
            ),
            Fault::Duplicate(index) => error(
                Category::Name,
                codes::ARGUMENT_DUPLICATE,
                at(index),
                format!("`{}` is given twice", given_name(index)),
                "EXPR-2",
            ),
            Fault::Missing(index) => error(
                Category::Type,
                codes::ARGUMENT_MISSING,
                call,
                format!(
                    "the callback is given no `{}`, a parameter with no default",
                    parameter(index)
                ),
                "FN-8",
            ),
            Fault::Cardinality {
                parameter: index,
                count,
                bounds,
                excess,
            } => error(
                Category::Type,
                codes::PACK_CARDINALITY,
                excess.map_or(closing, at),
                format!(
                    "the callback's pack `{}` takes {} but is given {count}",
                    parameter(index),
                    if let Some(upper) = bounds.1 {
                        format!("from {} through {upper} arguments", bounds.0)
                    } else {
                        format!("at least {} arguments", bounds.0)
                    }
                ),
                "GEN-10",
            ),
        };
        self.diagnostics.emit(report);
    }
}

/// The parameter names a dependent pack leaves for the enclosing declaration.
pub(super) fn pack_exclusions<'a>(
    dependent: &crate::program::DependentPack,
    map: &[(Location, &'a GenericArgument)],
) -> Vec<&'a str> {
    dependent
        .without
        .iter()
        .filter_map(|at| map.iter().find(|(declared, _)| declared == at))
        .filter_map(|(_, argument)| match argument {
            GenericArgument::Constant(constant) => Some(&constant.value),
            _ => None,
        })
        .flat_map(exclusion_names)
        .collect()
}

/// The names supplied by one constant in a `without` clause (FN-8).
pub(super) fn exclusion_names(value: &ConstantValue) -> Vec<&str> {
    match value {
        ConstantValue::Str(name) => vec![name.as_str()],
        ConstantValue::StringSet(names) => names.iter().map(String::as_str).collect(),
        _ => Vec::new(),
    }
}

/// Install the callback's own resolved named pack without changing its identity.
pub(super) fn specialize_named_pack(callback: &mut Expr, pack: Option<TypePackArgument>) {
    let Some(pack) = pack else { return };
    let ExprKind::StaticCallable(callee) = &mut callback.kind else {
        return;
    };
    let (Callee::Function { type_packs, .. } | Callee::Candidate { type_packs, .. }) =
        callee.as_mut()
    else {
        return;
    };
    if let Some(given) = type_packs
        .iter_mut()
        .find(|given| given.parameter == pack.parameter)
    {
        *given = pack;
    }
}

/// The static callback and the two packs that its matching determines.
#[derive(Debug, PartialEq)]
pub(super) struct Bound<'a> {
    /// The selected callback, with its existing specialization and identity.
    pub(super) callee: &'a Callee,
    /// Ps, in declaration order followed by the positions of its own pack.
    pub(super) reflected: Vec<(PackKey, Type)>,
    /// As, in the order the arguments were written, with their original types.
    pub(super) captured: Vec<(PackKey, Type)>,
    /// The entries that belong to the callback's own generic named pack.
    pub(super) own_named: Option<TypePackArgument>,
}

/// A problem to report at the argument or call responsible for it.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Fault {
    /// The callback or its function type is still open.
    Deferred,
    /// A positional argument follows a named one, at this argument index.
    Order(usize),
    /// No positional slot takes this argument index.
    Unexpected(usize),
    /// No parameter takes this named argument index.
    Unknown(usize),
    /// This argument index gives the same fixed parameter twice.
    Duplicate(usize),
    /// No argument gives this required parameter index.
    Missing(usize),
    /// A pack of the callback has the wrong number of arguments.
    Cardinality {
        /// The callback's pack parameter.
        parameter: usize,
        /// The number of arguments it was given.
        count: u64,
        /// Its inclusive lower and upper bounds.
        bounds: (u64, Option<u64>),
        /// The first excess argument, or the call's closing parenthesis.
        excess: Option<usize>,
    },
}

/// A fixed slot that participates in argument matching.
struct Ordinary<'a> {
    index: usize,
    name: &'a str,
    required: bool,
    configuration: bool,
    filled: bool,
}

/// Reflect the ordinary suffix of an unapplied callback (GEN-17). Its
/// own packs contribute types when a dependent call supplies arguments.
pub(super) fn reflect(template: &FunctionType, callback: &Expr) -> Vec<(PackKey, Type)> {
    let Type::Function(actual) = &callback.ty else {
        return Vec::new();
    };
    actual
        .ordinary_parameters()
        .skip(template.parameters.len())
        .filter(|input| {
            input
                .declared()
                .is_none_or(|parameter| parameter.pack.is_none())
        })
        .filter_map(|input| Some((PackKey::Name(input.name()?.to_string()), input.ty().clone())))
        .collect()
}

/// Keep the original inputs excluded from a reflection for later forwarding.
fn reflected_argument(
    parameter: usize,
    template: &FunctionType,
    callback: &Expr,
    entries: Vec<(PackKey, Type)>,
) -> TypePackArgument {
    let mut pack = TypePackArgument::keyed(parameter, entries);
    if let Type::Function(actual) = &callback.ty {
        let fixed = template.parameters.len();
        let prefix = actual
            .ordinary_parameters()
            .take(fixed)
            .map(|input| {
                (
                    input.name().map(|name| PackKey::Name(name.to_string())),
                    input.ty().clone(),
                )
            })
            .collect();
        pack.reflected = Some(ReflectedInputs { fixed, prefix });
    }
    pack
}

/// Match the compile-time arguments of a dependent call (FN-8). Input
/// names and argument indices refer to the arguments left by the outer
/// call. The generic values of `without` have already been read as names.
pub(super) fn bind<'a>(
    program: &Program,
    template: &FunctionType,
    callback: &'a Expr,
    supplied: &[(Option<&str>, &Expr)],
    without: &[&str],
) -> Result<Bound<'a>, Fault> {
    let (ExprKind::StaticCallable(callee), Type::Function(actual)) = (&callback.kind, &callback.ty)
    else {
        return Err(Fault::Deferred);
    };
    if matches!(
        **callee,
        Callee::Parameter { .. } | Callee::Required { .. } | Callee::Unread
    ) || actual.named_pack.is_some()
    {
        return Err(Fault::Deferred);
    }
    let fixed = template.parameters.len();
    let mut reflected = Vec::new();
    let mut ordinary = Vec::new();
    let mut positional = None;
    let mut named = None;
    let mut position = 0;
    for (index, input) in actual.parameters.iter().enumerate() {
        let name = input.name().ok_or(Fault::Deferred)?;
        let parameter = input.declared();
        let configuration = parameter.is_some_and(|parameter| parameter.configuration);
        let in_pack = !configuration && position >= fixed;
        position += usize::from(!configuration);
        let excluded = without.contains(&name);
        match parameter.and_then(|parameter| parameter.pack) {
            Some(Pack::Positional) => {
                if !excluded && let Some(parameter) = parameter {
                    positional = Some((index, parameter));
                }
            }
            Some(Pack::Named) => {
                if !excluded && let Some(parameter) = parameter {
                    named = Some((index, parameter));
                }
            }
            None => {
                if in_pack {
                    reflected.push((PackKey::Name(name.to_string()), input.ty().clone()));
                }
                if !excluded {
                    // UNIT-8: nullable parameters can be omitted without a
                    // default. FN-15: a var parameter must always be given.
                    let required = parameter.is_some_and(|parameter| parameter.var)
                        || !parameter.is_some_and(|parameter| parameter.has_default)
                            && !matches!(input.ty(), Type::Nullable(_));
                    ordinary.push(Ordinary {
                        index,
                        name,
                        required,
                        configuration,
                        filled: false,
                    });
                }
            }
        }
    }

    let mut captured = Vec::with_capacity(supplied.len());
    let mut positions = Vec::new();
    let mut names = Vec::new();
    let mut named_seen = false;
    for (index, (name, argument)) in supplied.iter().enumerate() {
        let key = if let Some(name) = name {
            named_seen = true;
            if let Some(slot) = ordinary.iter_mut().find(|slot| slot.name == *name) {
                if slot.configuration {
                    return Err(Fault::Unknown(index));
                }
                if slot.filled {
                    return Err(Fault::Duplicate(index));
                }
                slot.filled = true;
                PackKey::Name(slot.name.to_string())
            } else {
                let excluded = without.contains(name)
                    && actual.parameters.iter().any(|input| {
                        input.name() == Some(name)
                            && input
                                .declared()
                                .is_none_or(|parameter| parameter.pack.is_none())
                    });
                if named.is_none() || excluded {
                    return Err(Fault::Unknown(index));
                }
                if names.iter().any(|(previous, _)| previous == name) {
                    return Err(Fault::Duplicate(index));
                }
                names.push((*name, index));
                PackKey::Name((*name).to_string())
            }
        } else {
            if named_seen {
                return Err(Fault::Order(index));
            }
            if let Some(slot) = ordinary
                .iter_mut()
                .find(|slot| !slot.filled && !slot.configuration)
            {
                slot.filled = true;
                PackKey::Name(slot.name.to_string())
            } else if positional.is_some() {
                let position = positions.len();
                positions.push(index);
                PackKey::Position(position)
            } else {
                return Err(Fault::Unexpected(index));
            }
        };
        captured.push((key, argument.ty.clone()));
    }
    if let Some(slot) = ordinary.iter().find(|slot| slot.required && !slot.filled) {
        return Err(Fault::Missing(slot.index));
    }

    let generics = callee
        .declaration()
        .and_then(|(module, declaration)| program.modules.get(module)?.details.get(declaration))
        .and_then(|detail| match detail {
            Detail::Callable(signature) => Some(signature.generics.as_slice()),
            _ => None,
        })
        .unwrap_or_default();

    if let Some((index, parameter)) = positional {
        let written = type_pack(parameter, generics).and_then(|generic| {
            callee
                .type_packs()
                .iter()
                .find(|pack| pack.parameter == generic)
        });
        let bounds = pack_bounds(parameter, generics, written);
        let count = positions.len() as u64;
        if count < bounds.0 || bounds.1.is_some_and(|maximum| count > maximum) {
            return Err(Fault::Cardinality {
                parameter: index,
                count,
                bounds,
                excess: bounds
                    .1
                    .and_then(|maximum| usize::try_from(maximum).ok())
                    .and_then(|maximum| positions.get(maximum))
                    .copied(),
            });
        }
        reflected.extend((0..positions.len()).map(|position| {
            let ty = written.map_or(&parameter.ty, |pack| &pack.types[position]);
            (PackKey::Position(position), ty.clone())
        }));
    }
    let mut own_named = None;
    if let Some((index, parameter)) = named {
        let bounds = pack_bounds(parameter, generics, None);
        let count = names.len() as u64;
        if count < bounds.0 || bounds.1.is_some_and(|maximum| count > maximum) {
            return Err(Fault::Cardinality {
                parameter: index,
                count,
                bounds,
                excess: bounds
                    .1
                    .and_then(|maximum| usize::try_from(maximum).ok())
                    .and_then(|maximum| names.get(maximum))
                    .map(|(_, index)| *index),
            });
        }
        // FN-8: its own named pack takes the types the arguments already
        // have and reflects the keys in the order they are written.
        let entries = names.into_iter().map(|(_, index)| captured[index].clone());
        if let Some(generic) = type_pack(parameter, generics) {
            own_named = Some(TypePackArgument::keyed(generic, entries.clone().collect()));
        }
        reflected.extend(entries);
    }
    Ok(Bound {
        callee,
        reflected,
        captured,
        own_named,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hir::{Stmt, ValueCategory};
    use crate::{ParsedFile, Scalar, analyse};
    use hgl_diagnostics::Diagnostics;
    use hgl_parser::{Entry, Limits, parse};
    use hgl_source::SourceMap;

    fn analyse_callback(source: &str) -> Program {
        let mut sources = SourceMap::new();
        let mut diagnostics = Diagnostics::new();
        let file = sources.add("callback.hgl", source).unwrap();
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
        assert!(diagnostics.as_slice().is_empty(), "{diagnostics:?}");
        program
    }

    fn callback(program: &Program) -> (&FunctionType, &Expr) {
        let module = program.module("m").unwrap();
        let call = *module.scope["use_it"].first().unwrap();
        let [
            Stmt::Return {
                value: Some(value), ..
            },
        ] = module.bodies[call].as_ref().unwrap().statements.as_slice()
        else {
            panic!("use_it does not return the call");
        };
        let ExprKind::Call {
            callee, arguments, ..
        } = &value.kind
        else {
            panic!("use_it does not return a call");
        };
        let (callee_module, declaration) = callee.declaration().unwrap();
        let Detail::Callable(signature) = &program.modules[callee_module].details[declaration]
        else {
            panic!("inspect is not callable");
        };
        let Type::Function(template) = &signature.parameters[0].ty else {
            panic!("inspect has no function parameter");
        };
        (template, &arguments[0].value)
    }

    fn value(near: &Expr, ty: Scalar) -> Expr {
        Expr {
            kind: ExprKind::Unread,
            at: near.at,
            ty: Type::Scalar(ty),
            category: ValueCategory::Value,
        }
    }

    fn entry(name: &str, ty: Scalar) -> (PackKey, Type) {
        (PackKey::Name(name.to_string()), Type::Scalar(ty))
    }

    #[test]
    fn fixed_slots_match_as_a_call_and_keep_source_types_and_order() {
        // GEN-17: the fixed template prefix does not belong to Ps, which
        // still keeps the defaulted slot. FN-8: As keeps the arguments'
        // types, with declaration names even for positional arguments.
        let program = analyse_callback(
            "module m
const fn inspect<...Ps>(f: const fn(prefix: i64, ...{Ps}) -> i64) -> i64 => 1
const fn target(prefix: i64, key: str, price: f64, weight: f64 = 1.0) -> i64 => 1
const fn use_it() -> i64 => inspect(target)",
        );
        let (template, callback) = callback(&program);
        let integer = value(callback, Scalar::I64);
        let flag = value(callback, Scalar::Bool);
        let bound = bind(
            &program,
            template,
            callback,
            &[
                (None, &integer),
                (Some("price"), &flag),
                (Some("key"), &integer),
            ],
            &[],
        )
        .unwrap();
        let ExprKind::StaticCallable(callee) = &callback.kind else {
            panic!("expected callback");
        };
        assert!(std::ptr::eq(bound.callee, callee.as_ref()));
        assert_eq!(
            bound.reflected,
            [
                entry("key", Scalar::Str),
                entry("price", Scalar::F64),
                entry("weight", Scalar::F64)
            ]
        );
        assert_eq!(
            bound.captured,
            [
                entry("prefix", Scalar::I64),
                entry("price", Scalar::Bool),
                entry("key", Scalar::I64)
            ]
        );
    }

    #[test]
    fn exclusions_leave_matching_but_stay_in_reflection() {
        let program = analyse_callback(
            "module m
const fn inspect<...Ps>(f: const fn(...{Ps}) -> i64) -> i64 => 1
const fn target(index: i64, key: str, price: f64, weight: f64 = 1.0) -> i64 => 1
const fn use_it() -> i64 => inspect(target)",
        );
        let (template, callback) = callback(&program);
        let integer = value(callback, Scalar::I64);
        let exclusions = ["key", "index", "absent"];
        let matched = bind(
            &program,
            template,
            callback,
            &[(None, &integer)],
            &exclusions,
        )
        .unwrap();
        assert_eq!(
            matched.reflected,
            [
                entry("index", Scalar::I64),
                entry("key", Scalar::Str),
                entry("price", Scalar::F64),
                entry("weight", Scalar::F64)
            ]
        );
        assert_eq!(matched.captured, [entry("price", Scalar::I64)]);
        assert_eq!(
            bind(&program, template, callback, &[], &exclusions),
            Err(Fault::Missing(2))
        );
        assert_eq!(
            bind(
                &program,
                template,
                callback,
                &[(Some("key"), &integer)],
                &exclusions
            ),
            Err(Fault::Unknown(0))
        );
        assert_eq!(
            bind(
                &program,
                template,
                callback,
                &[(None, &integer), (Some("price"), &integer)],
                &exclusions
            ),
            Err(Fault::Duplicate(1))
        );
    }

    #[test]
    fn the_callbacks_own_positional_pack_reflects_positions_and_keeps_bounds() {
        // FN-8 and GEN-17: declared element types enter Ps for each
        // actual pack argument; a heterogenous pack takes its written
        // type sequence from the selected callback.
        let program = analyse_callback(
            "module m
const fn inspect<...Ps>(f: const fn(...{Ps}) -> f64) -> i64 => 1
const fn total<...Items>(scale: f64, items: ...Items) -> f64 => scale
const fn use_it() -> i64 => inspect(total<i64, bool>)",
        );
        let (template, callback) = callback(&program);
        let integer = value(callback, Scalar::I64);
        let scale = value(callback, Scalar::F64);
        let matched = bind(
            &program,
            template,
            callback,
            &[(None, &scale), (None, &integer), (None, &integer)],
            &[],
        )
        .unwrap();
        assert_eq!(
            matched.reflected,
            [
                entry("scale", Scalar::F64),
                (PackKey::Position(0), Type::Scalar(Scalar::I64)),
                (PackKey::Position(1), Type::Scalar(Scalar::Bool))
            ]
        );
        assert_eq!(
            matched.captured[2],
            (PackKey::Position(1), Type::Scalar(Scalar::I64))
        );
        assert_eq!(
            bind(&program, template, callback, &[(None, &scale)], &[]),
            Err(Fault::Cardinality {
                parameter: 1,
                count: 0,
                bounds: (2, Some(2)),
                excess: None
            })
        );
        let excluded = bind(&program, template, callback, &[(None, &scale)], &["items"]).unwrap();
        assert_eq!(excluded.reflected, [entry("scale", Scalar::F64)]);
    }

    #[test]
    fn the_callbacks_own_named_pack_reflects_the_keys_it_takes() {
        // FN-8: fixed slots are reflected even when the callback's
        // named pack takes no argument. Captured names extend Ps and As.
        let program = analyse_callback(
            "module m
const fn inspect<...Ps>(f: const fn(...{Ps}) -> i64) -> i64 => 1
const fn target<...Fields>(price: i64, fields: ...{Fields}) -> i64 => price
const fn use_it() -> i64 => inspect(target)",
        );
        let (template, callback) = callback(&program);
        let integer = value(callback, Scalar::I64);
        let matched = bind(&program, template, callback, &[(None, &integer)], &[]).unwrap();
        let [pack] = matched.callee.type_packs() else {
            panic!("expected an empty callback pack for specialization");
        };
        assert_eq!(pack.parameter, 0);
        assert!(pack.types.is_empty());
        assert_eq!(matched.reflected, [entry("price", Scalar::I64)]);
        assert_eq!(matched.captured, [entry("price", Scalar::I64)]);
        let matched = bind(
            &program,
            template,
            callback,
            &[(None, &integer), (Some("extra"), &integer)],
            &[],
        )
        .unwrap();
        assert_eq!(
            matched.reflected,
            [entry("price", Scalar::I64), entry("extra", Scalar::I64)]
        );
        assert_eq!(matched.captured, matched.reflected);
    }
}
