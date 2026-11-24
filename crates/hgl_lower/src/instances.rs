//! The instances of generic declarations a program asks for. Part of
//! lowering: a transpiler writes each before it writes the modules.
//!
//! A node or a graph is written with the types of its inputs, and a
//! node's code calls the function chosen for those types. So a temporal
//! function with generic parameters, and a temporal candidate of an
//! operator with them, is written once for each set of generic arguments
//! it is called with (GEN-2):
//!
//! ```text
//! fn identity_sum<T>(lhs: T, rhs: T) -> T
//!     requires add_(T, T) -> T => lhs +{invalid: .identity} rhs
//! ```
//!
//! called with two `f64` and with two `str` is two graphs, whose
//! [`Instance::suffix`]es are `f64` and `str`.
//!
//! This reads the typed program for those calls before any module is
//! written, since a call in one module asks for an instance that is
//! written in the declaration's module. It starts from every body that
//! is not itself written by instance, and from every test. The
//! analyser gives the body of each instance
//! ([`Program::instance`]), whose own calls ask for more: the graph
//! above asks for core's candidate of `+` for `f64`, which core's
//! OPS-9 gives a body of its own.
//!
//! A value function and a value candidate with generic parameters is
//! written by instance too, as the target's values are typed
//! ([`Instances::typed`]): which candidate answers a requirement it
//! states (GEN-16) or a call a membership it states authorises (GEN-3,
//! GEN-7), and what a generic value parameter is (GEN-13), each instance
//! says. For tenor date movement the instances call the selected
//! calendar and rolling operations (TEN-5, TEN-6, TEN-7, TEN-8).
//!
//! An eval asks for instances too: its run is wired with core's
//! `replay` for each input it gives a trace and core's `record` for
//! its target's result (core's CEVAL-1), each written for the type it
//! is of.
//!
//! An application's run statement asks as an eval does: for what its
//! call and its configuration name, and for core's `record` of its
//! entry's result (core's CRUN-8). So do the statements it sets its
//! run's store with (UNIT-14).

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt::Write;

use hgl_analyser::hir::{
    Answer, Argument, Body, Callee, Expr, ExprKind, Local, Node, Run, Stmt, TypePackArgument,
    ValueCategory, walk_statement_nodes_at, walk_statements,
};
use hgl_analyser::{
    Argument as GenericArgument, DeclarationKind, Detail, GenericKind, LocalKind, Pack, Program,
    Role, Signature, Type,
};
use hgl_diagnostics::Location;

use crate::calls::{DefaultValue, LeftOut, left_out, runtime_arguments};
use crate::canonical::{canonical, canonical_argument};
use crate::packs::{Captures, Slot};

/// A declaration specialized for one set of generic and function
/// arguments (GEN-2, FN-17).
pub struct Instance {
    /// Each generic argument as text no other is written as
    /// (`canonical_argument`): what tells one instance from another.
    key: Vec<String>,
    /// What follows the declaration's own name in the name a target
    /// gives the instance: the generic arguments, each character that is
    /// no letter or digit as `_`, and a number where two instances would
    /// otherwise have one name.
    pub suffix: String,
    /// Its signature, with the generic arguments in place.
    pub signature: Signature,
    /// Its body, with them in place, and each operation the declaration
    /// requires as the call of the candidate that answers it (GEN-16).
    pub body: Body,
    /// The original slot and key of each runtime parameter, when
    /// lowering changed the parameter list.
    slots: Option<Vec<Slot>>,
    /// Where an expression still needs a pack as a whole.
    pub pack_use: Option<Location>,
}

impl Instance {
    /// The concrete parameter an analysed argument supplies.
    pub fn parameter(&self, argument: &Argument) -> Option<usize> {
        if matches!(argument.value.kind, ExprKind::StaticCallable(_)) {
            return None;
        }
        match &self.slots {
            Some(slots) => slots.iter().position(|slot| slot.takes(argument)),
            None => argument.parameter.filter(|_| argument.pack.is_none()),
        }
    }

    /// Maps a runtime parameter index back to the declaration's
    /// parameter, which owns its default (UNIT-8).
    pub fn original_parameter(&self, index: usize) -> usize {
        self.slots
            .as_ref()
            .map_or(index, |slots| slots[index].parameter)
    }
}

/// Runtime slots for a native value call with a positional pack (FN-8).
/// A native declaration has no HGL body to specialize.
pub struct NativePack {
    /// The concrete runtime parameters in call order.
    pub signature: Signature,
    slots: Vec<Slot>,
}

impl NativePack {
    /// Expand a native value candidate taking only a positional pack.
    pub fn of(program: &Program, callee: &Callee, arguments: &[Argument]) -> Option<Self> {
        let (module, declaration) = callee.declaration()?;
        let owner = &program.modules[module];
        if !matches!(
            owner.declarations[declaration].kind,
            DeclarationKind::NativeCandidate(Role::Value, _)
        ) {
            return None;
        }
        let Detail::Callable(signature) = &owner.details[declaration] else {
            return None;
        };
        let [parameter] = signature.parameters.as_slice() else {
            return None;
        };
        if parameter.pack != Some(Pack::Positional) {
            return None;
        }

        let captures = Captures::of(program, callee, signature, arguments);
        let mut signature = signature.clone();
        let mut body = Body {
            locals: vec![Local {
                at: parameter.at,
                name: parameter.name.clone(),
                kind: LocalKind::Parameter,
                ty: parameter.ty.clone(),
                category: ValueCategory::Value,
            }],
            statements: Vec::new(),
        };
        let (slots, _) = captures.expand(&mut signature, &mut body);
        Some(Self {
            signature,
            slots: slots?,
        })
    }

    /// The concrete slot of the supplied argument.
    pub fn parameter(&self, argument: &Argument) -> Option<usize> {
        self.slots.iter().position(|slot| slot.takes(argument))
    }
}

/// The instances of each generic temporal declaration of a program, in
/// the order they are first asked for.
#[derive(Default)]
pub struct Instances {
    /// By the declaration: the position of its module in the program,
    /// and its position among that module's declarations.
    declared: BTreeMap<(usize, usize), Vec<Instance>>,
    /// The value functions and value candidates that are written by
    /// instance: each with generic parameters and a body.
    requiring: BTreeSet<(usize, usize)>,
    callable_limits: CallableLimits,
    /// The exact instances that exceeded the target's growth limit.
    growing: BTreeSet<(usize, usize, Vec<String>)>,
}

/// The target's resource limits for instancing static function arguments.
#[derive(Default)]
pub struct CallableLimits {
    /// Copied targets, statements, expressions and locals allowed in one request.
    pub size: usize,
    /// Nested nodes allowed on one path through the specialized body.
    pub depth: usize,
    /// Deeper requests for the same declaration allowed on one request chain.
    pub growth: usize,
}

/// Which limit prevented the target from writing an instance.
pub enum CallableLimit {
    /// The request would copy too many nodes to specialize its body.
    Size,
    /// The resulting body would nest too deeply to copy or emit safely.
    Depth,
    /// The same declaration was requested too often with a deeper argument.
    Growth,
}

/// One call's request for an instance.
struct Asked {
    /// The position of the declaration's module in the program.
    module: usize,
    /// The declaration's position in that module.
    declaration: usize,
    /// What each of its generic parameters stands for at the call.
    generics: Vec<GenericArgument>,
    /// The concrete type sequences of the call's generic packs.
    type_packs: Vec<TypePackArgument>,
    /// Ordinary const values needed by a compile-time check in the body.
    constants: Vec<hgl_analyser::ConstInput>,
    /// Which arguments the call captures in its parameter packs.
    captures: Captures,
    /// What answers each requirement it states there.
    requirements: Vec<Answer>,
    /// Its static callable targets, by the argument's original slot.
    functions: Vec<(usize, Callee)>,
    /// The anonymous target depth of each of those arguments.
    depths: Vec<(usize, usize)>,
}

/// A completed request on the path to another instance.
struct Request {
    declaration: (usize, usize),
    depths: Vec<(usize, usize)>,
    /// Index of the preceding callable request in the search's request list.
    parent: Option<usize>,
}

/// What an eval's run is wired with (core's CEVAL-1: "core's `replay`
/// wired to each temporal input of the target, and the target's result
/// as the entry's").
pub struct Wiring {
    /// Each replay, by the argument's position in the call and the type
    /// of its input. Several replays can supply one pack (FN-8).
    pub inputs: Vec<(usize, Type)>,
    /// The type of the target's result, where it has one.
    pub result: Option<Type>,
}

/// The endpoint payload for a trace of publications (TST-5, TYPE-16).
/// Replays and the lifted node receiving those replays use the same type.
pub fn trace_payload(trace: &Type) -> Option<Type> {
    trace.trace_payload()
}

/// What the run of the eval that takes `call` is wired with; `eval` is
/// the eval's own type, a trace of what the target publishes. `None`
/// where `call` is no call of a function.
///
/// A target with no generic parameter says the type of each input and
/// of its result: they are its own. A generic one's are read back from
/// its traces, since a trace's cell is the delta of what is published
/// (the language's TST-5): a scalar is its own delta, and a set's and a
/// nullable's delta name the type they are of. A delta does not say
/// that a part of it was an `atomic<V>`, whose delta is V.
pub fn run_wiring(program: &Program, call: &Expr, eval: &Type) -> Option<Wiring> {
    let ExprKind::Call {
        callee:
            callee @ Callee::Function {
                module,
                declaration,
                generics,
                requirements,
                ..
            },
        arguments,
        ..
    } = &call.kind
    else {
        return None;
    };
    let Some(Detail::Callable(signature)) = program.modules[*module].details.get(*declaration)
    else {
        return None;
    };
    // A generic target's parameters as this call takes them (GEN-2).
    let captures = Captures::of(program, callee, signature, arguments);
    let instance = if generics.is_empty() {
        None
    } else {
        program.instance_with_inputs(
            (*module, *declaration),
            generics,
            requirements,
            hgl_analyser::InstanceInputs {
                functions: &functions_of(arguments),
                type_packs: callee.type_packs(),
                pack_inputs: Some(&captures.inputs()),
                ..hgl_analyser::InstanceInputs::default()
            },
        )
    };
    // Where the target's own types are known, its inputs and result are
    // of them; otherwise they are read back from the traces.
    let known = signature.generics.is_empty() || instance.is_some();
    let parameters = instance
        .as_ref()
        .map_or(&signature.parameters, |(taken, _)| &taken.parameters);
    let inputs = runtime_arguments(arguments)
        .filter_map(|(position, argument)| {
            let parameter = parameters.get(argument.parameter?)?;
            // Only an argument that is a trace is replayed.
            let payload = trace_payload(&argument.value.ty)?;
            if let Some(ty) = captures.ty(argument) {
                return Some((position, ty.clone()));
            }
            // TYPE-16: a window's delta is "V: the value that arrived",
            // so a trace of one is a trace of V: the window is the
            // parameter's.
            let windowed = matches!(parameter.ty, Type::RollingWindow { .. });
            Some((
                position,
                if known || windowed {
                    parameter.ty.clone()
                } else {
                    payload
                },
            ))
        })
        .collect();
    // The call's own type is the result as this call has it.
    let windowed = matches!(call.ty, Type::RollingWindow { .. });
    let result = match &signature.result {
        None => None,
        Some(_) if known || windowed => Some(call.ty.clone()),
        Some(_) => trace_payload(eval),
    };
    Some(Wiring { inputs, result })
}

/// The type of what the entry of the run statement `run` publishes,
/// which the run records (core's CRUN-8): "The run wires `record` to
/// the entry's result". `None` for an entry with no result, and where
/// the statement's call is no call of a function.
pub fn run_result(program: &Program, run: &Run) -> Option<Type> {
    let ExprKind::Call {
        callee:
            Callee::Function {
                module,
                declaration,
                ..
            },
        ..
    } = &run.call.kind
    else {
        return None;
    };
    let Some(Detail::Callable(signature)) = program.modules[*module].details.get(*declaration)
    else {
        return None;
    };
    // The call's own type is the result as this call has it: with the
    // generic arguments of the call in place of the entry's generic
    // parameters.
    signature.result.as_ref().map(|_| run.call.ty.clone())
}

/// The temporal function `name` of core's module: `replay`, `record`.
pub fn core_function(program: &Program, name: &str) -> Option<(usize, usize)> {
    let core = program.core()?;
    let declaration = program.modules[core]
        .declarations
        .iter()
        .position(|declared| {
            declared.name == name && declared.kind == DeclarationKind::Function(Role::Temporal)
        })?;
    Some((core, declaration))
}

/// True for a temporal function, or a temporal candidate of an
/// operator, that has generic parameters and a body.
fn generic_temporal(program: &Program, module: usize, declaration: usize) -> bool {
    let Some(declared) = program.modules.get(module) else {
        return false;
    };
    let temporal = declared
        .declarations
        .get(declaration)
        .is_some_and(|declaration| declaration.kind.role() == Some(Role::Temporal));
    temporal && generic(program, module, declaration)
}

/// True for a declaration with generic parameters and a body.
fn generic(program: &Program, module: usize, declaration: usize) -> bool {
    let Some(declared) = program.modules.get(module) else {
        return false;
    };
    let generic = matches!(
        declared.details.get(declaration),
        Some(Detail::Callable(signature)) if !signature.generics.is_empty() || signature.parameters.iter().any(|parameter| parameter.pack.is_some() || matches!(parameter.ty, Type::Function(_)))
    );
    generic && matches!(declared.bodies.get(declaration), Some(Some(_)))
}

/// Read the static callable arguments by the original parameter index.
fn function_arguments(arguments: &[Argument]) -> impl Iterator<Item = (usize, &Callee)> + Clone {
    arguments.iter().filter_map(|argument| {
        let ExprKind::StaticCallable(target) = &argument.value.kind else {
            return None;
        };
        Some((argument.parameter?, target.as_ref()))
    })
}

fn functions_of(arguments: &[Argument]) -> Vec<(usize, Callee)> {
    let mut functions: Vec<_> = function_arguments(arguments)
        .map(|(slot, target)| (slot, target.clone()))
        .collect();
    functions.sort_by_key(|(slot, _)| *slot);
    functions
}

fn called_or_supplied(value: &Expr) -> Option<&Callee> {
    match &value.kind {
        ExprKind::Call { callee, .. } => Some(callee),
        ExprKind::StaticCallable(callee) => Some(callee),
        _ => None,
    }
}

struct CopyBudget {
    remaining: usize,
    depth_limit: usize,
}

impl CopyBudget {
    fn spend(&mut self, nodes: usize, depth: usize) -> Result<(), CallableLimit> {
        if depth > self.depth_limit {
            return Err(CallableLimit::Depth);
        }
        self.remaining = self
            .remaining
            .checked_sub(nodes)
            .ok_or(CallableLimit::Size)?;
        Ok(())
    }
}

struct CallableShape {
    /// Deepest HIR node or callee, counting from the whole body's root.
    deepest: usize,
    /// The anonymous callee levels, which measure repeated request growth.
    anonymous: usize,
}

struct ArgumentCost {
    nodes: usize,
    height: usize,
}

/// Spend the cost of one callee and its owned body. A named callee owns
/// its identity and an anonymous callee owns the nodes of its body.
fn callable_shape(
    target: &Callee,
    depth: usize,
    budget: &mut CopyBudget,
    arguments: &[(usize, ArgumentCost)],
) -> Result<CallableShape, CallableLimit> {
    if let Callee::Parameter { parameter, .. } = target
        && let Some((_, cost)) = arguments.iter().find(|(slot, _)| slot == parameter)
    {
        let deepest = depth + cost.height - 1;
        budget.spend(cost.nodes, deepest)?;
        // In the forecast only, the argument's anonymous nesting was
        // already measured. Its structural depth grows at this use site.
        return Ok(CallableShape {
            deepest,
            anonymous: 0,
        });
    }
    budget.spend(1, depth)?;
    if let Callee::Anonymous(function) = target {
        let mut shape = body_shape(&function.body, depth + 1, budget, arguments)?;
        shape.anonymous += 1;
        return Ok(shape);
    }
    Ok(CallableShape {
        deepest: depth,
        anonymous: 0,
    })
}

/// Walk an owned body. When `arguments` is given, forecast each copy the
/// instancing pass makes at a call or forwarding use of a static parameter.
fn body_shape(
    body: &Body,
    depth: usize,
    budget: &mut CopyBudget,
    arguments: &[(usize, ArgumentCost)],
) -> Result<CallableShape, CallableLimit> {
    let mut shape = CallableShape {
        deepest: depth - 1,
        anonymous: 0,
    };
    budget.spend(body.locals.len(), shape.deepest)?;
    walk_statement_nodes_at(&body.statements, depth, &mut |node, depth| {
        budget.spend(1, depth)?;
        shape.deepest = shape.deepest.max(depth);
        if let Node::Expression(expression) = node
            && let Some(callee) = called_or_supplied(expression)
        {
            let nested = callable_shape(callee, depth + 1, budget, arguments)?;
            shape.deepest = shape.deepest.max(nested.deepest);
            shape.anonymous = shape.anonymous.max(nested.anonymous);
        }
        Ok(())
    })?;
    Ok(shape)
}

/// Check the borrowed arguments and the body that their instance would
/// own. Neither a wide copy nor a deeply nested copy may be constructed
/// first and checked only when it makes its next request.
fn request_depths(
    program: &Program,
    (module, declaration): (usize, usize),
    arguments: &[Argument],
    limits: &CallableLimits,
) -> Result<Vec<(usize, usize)>, CallableLimit> {
    let mut budget = CopyBudget {
        remaining: limits.size,
        depth_limit: limits.depth,
    };
    let mut costs = Vec::new();
    let mut depths = Vec::new();
    for (slot, target) in function_arguments(arguments) {
        let before = budget.remaining;
        let shape = callable_shape(target, 1, &mut budget, &[])?;
        depths.push((slot, shape.anonymous));
        costs.push((
            slot,
            ArgumentCost {
                nodes: before - budget.remaining,
                height: shape.deepest,
            },
        ));
    }
    if !costs.is_empty()
        && let Some(body) = &program.modules[module].bodies[declaration]
    {
        body_shape(body, 1, &mut budget, &costs)?;
    }
    Ok(depths)
}

fn exceeds_growth(
    declaration: (usize, usize),
    depths: &[(usize, usize)],
    requests: &[Request],
    mut parent: Option<usize>,
    mut remaining: usize,
) -> bool {
    if depths.iter().all(|(_, depth)| *depth == 0) {
        return false;
    }
    while let Some(index) = parent {
        let above = &requests[index];
        if above.declaration == declaration
            && depths.iter().any(|(slot, depth)| {
                above.depths.iter().any(|(previous_slot, previous_depth)| {
                    slot == previous_slot && depth > previous_depth
                })
            })
        {
            if remaining == 0 {
                return true;
            }
            remaining -= 1;
        }
        parent = above.parent;
    }
    false
}

/// Identify the candidate that answers a callable's requirement, with
/// constants keyed by their value (GEN-13, GEN-16).
fn answer_key(program: &Program, answer: &Answer) -> Option<String> {
    let Answer::Candidate {
        operator,
        module,
        declaration,
        generics,
        requirements,
    } = answer
    else {
        return matches!(answer, Answer::Unread).then(|| "unread".to_string());
    };
    let generics = generics
        .iter()
        .map(|argument| canonical_argument(program, argument))
        .collect::<Option<Vec<_>>>()?;
    let requirements = requirements
        .iter()
        .map(|answer| answer_key(program, answer))
        .collect::<Option<Vec<_>>>()?;
    Some(format!(
        "{operator:?}:{module}:{declaration}:{generics:?}:{requirements:?}"
    ))
}

/// Identify a concrete function or operator candidate and whether its key
/// holds an anonymous body's const inputs (GEN-13, GEN-16, FN-17).
fn function_key(program: &Program, target: &Callee) -> Option<(String, bool)> {
    if let Callee::Anonymous(function) = target {
        let types = function
            .body
            .locals
            .iter()
            .take(function.template.parameters.len() + function.captures.len())
            .map(|local| Some((canonical(program, &local.ty)?, local.category)))
            .collect::<Option<Vec<_>>>()?;
        let result = match &function.template.result {
            Some(result) => Some(canonical(program, result)?),
            None => None,
        };
        let generic_arguments = function
            .specialization
            .generics
            .iter()
            .map(|argument| canonical_argument(program, argument))
            .collect::<Option<Vec<_>>>()?;
        let type_packs = function
            .specialization
            .type_packs
            .iter()
            .map(|pack| {
                let types = pack
                    .types
                    .iter()
                    .map(|ty| canonical(program, trace_payload(ty).as_ref().unwrap_or(ty)))
                    .collect::<Option<Vec<_>>>()?;
                let prefix = pack
                    .reflected
                    .iter()
                    .flat_map(|inputs| &inputs.prefix)
                    .map(|(key, ty)| {
                        Some((
                            key,
                            canonical(program, trace_payload(ty).as_ref().unwrap_or(ty))?,
                        ))
                    })
                    .collect::<Option<Vec<_>>>()?;
                Some((pack.parameter, &pack.keys, types, prefix))
            })
            .collect::<Option<Vec<_>>>()?;
        let pack_inputs = function
            .specialization
            .pack_inputs
            .iter()
            .map(|entry| Some((entry.parameter, &entry.key, canonical(program, &entry.ty)?)))
            .collect::<Option<Vec<_>>>()?;
        // The same source lambda may close over different static
        // functions when its enclosing function is specialized.
        let mut targets = Vec::new();
        walk_statements(&function.body.statements, &mut |expression| {
            if let Some(target) = called_or_supplied(expression)
                && !matches!(target, Callee::Unread)
            {
                targets.push(function_key(program, target));
            }
        });
        // A nested key is already encoded. Quoting it again would escape
        // the escapes at every level. Its byte length frames it as-is.
        let mut encoded = String::new();
        let mut has_const = !function.specialization.const_inputs.is_empty();
        for target in targets {
            let (target, target_has_const) = target?;
            has_const |= target_has_const;
            write!(encoded, "{}#{target}", target.len()).expect("writing to a string succeeds");
        }
        for (slot, key) in &function.specialization.const_inputs {
            write!(encoded, "const[{slot}]:{}#{key}", key.len())
                .expect("writing to a string succeeds");
        }
        let key = format!(
            "anonymous:{}:{}:{}:{types:?}:{result:?}:{generic_arguments:?}:{type_packs:?}:{pack_inputs:?}:{encoded}",
            function.at.file.index(),
            function.at.span.start,
            function.template.value,
        );
        return Some((key, has_const));
    }
    let declared = target.declaration()?;
    let Detail::Callable(signature) = &program.modules[declared.0].details[declared.1] else {
        return None;
    };
    let arguments = generic_key(program, signature, target.generics(), target.type_packs())?;
    let requirements = match target {
        Callee::Function { requirements, .. } | Callee::Candidate { requirements, .. } => {
            requirements
                .iter()
                .map(|answer| answer_key(program, answer))
                .collect::<Option<Vec<_>>>()?
        }
        _ => return None,
    };
    Some((
        format!("{declared:?}:{arguments:?}:{requirements:?}"),
        false,
    ))
}

/// Marks a function key with const text that should stay out of its C++ name.
const CONST_FUNCTION_PREFIX: &str = "function with const inputs[";

/// An instance that uses a test's candidate has different evidence from
/// the same call with another test's candidate or with the module's (TST-1).
fn uses_test_candidate(program: &Program, answer: &Answer) -> bool {
    match answer {
        Answer::Candidate {
            module,
            declaration,
            requirements,
            ..
        } => {
            program.modules[*module].declarations[*declaration]
                .test_owner
                .is_some()
                || requirements
                    .iter()
                    .any(|answer| uses_test_candidate(program, answer))
        }
        _ => false,
    }
}

/// The generic arguments of a call of `declaration` of `module` as an
/// instance's key. `None` where they are not whole: one of them names
/// a generic parameter or is not known, or the call does not say what
/// each generic parameter of the declaration is.
fn key<'a>(
    program: &Program,
    (module, declaration): (usize, usize),
    generics: &[GenericArgument],
    type_packs: &[TypePackArgument],
    captures: &Captures,
    functions: impl Iterator<Item = (usize, &'a Callee)> + Clone,
    (requirements, constants): (&[Answer], &[hgl_analyser::ConstInput]),
) -> Option<Vec<String>> {
    let Some(Detail::Callable(signature)) = program.modules.get(module)?.details.get(declaration)
    else {
        return None;
    };
    let mut key = generic_key(program, signature, generics, type_packs)?;
    if signature
        .parameters
        .iter()
        .any(|parameter| parameter.pack.is_some())
    {
        key.push("arguments".to_string());
        key.extend(captures.key(program)?);
    }
    for (slot, parameter) in signature.parameters.iter().enumerate() {
        if !matches!(parameter.ty, Type::Function(_)) {
            continue;
        }
        let (_, target) = functions.clone().find(|(index, _)| *index == slot)?;
        let (target, has_const) = function_key(program, target)?;
        if has_const {
            key.push(format!("{CONST_FUNCTION_PREFIX}{slot}]:{target}"));
        } else {
            key.push(format!("function[{slot}]:{target}"));
        }
    }
    if requirements
        .iter()
        .any(|answer| uses_test_candidate(program, answer))
    {
        key.push("test requirements".to_string());
        key.extend(
            requirements
                .iter()
                .map(|answer| answer_key(program, answer))
                .collect::<Option<Vec<_>>>()?,
        );
    }
    key.extend(
        constants
            .iter()
            .map(|input| format!("const[{}]:{}", input.slot, input.key)),
    );
    Some(key)
}

/// The canonical identity of the type, pack and constant arguments.
fn generic_key(
    program: &Program,
    signature: &Signature,
    generics: &[GenericArgument],
    type_packs: &[TypePackArgument],
) -> Option<Vec<String>> {
    if generics.len() != signature.generics.len() {
        return None;
    }
    let mut key = Vec::new();
    for (index, (generic, argument)) in signature.generics.iter().zip(generics).enumerate() {
        if matches!(generic.kind, GenericKind::Pack { .. }) {
            let pack = type_packs.iter().find(|pack| pack.parameter == index)?;
            let types = pack
                .types
                .iter()
                .map(|ty| canonical(program, trace_payload(ty).as_ref().unwrap_or(ty)))
                .collect::<Option<Vec<_>>>()?;
            key.push(format!("pack[{}]", types.join(", ")));
        } else {
            key.push(canonical_argument(program, argument)?);
        }
    }
    Some(key)
}

impl Instances {
    /// The instances `program` asks for.
    ///
    /// A body that is written by instance asks with its own generic
    /// parameters, which says nothing until it is an instance itself:
    /// so the search starts from every other body, and from the tests,
    /// and goes on through each instance it finds. A call whose
    /// arguments are not whole asks for nothing. It ends: an instance is
    /// found once, the analyser gives up a type that grows past its
    /// bound, which is then not whole, and no more instances of one
    /// declaration are made from one call than the analyser follows
    /// ([`hgl_analyser::INSTANCES`]). Lowering also applies the target's
    /// limits to the size, structural depth and growth of static function arguments
    /// (FN-17). The target reports a call whose instance was declined.
    ///
    /// The target's values are typed, so every value function and value
    /// candidate with generic parameters is written by instance, one for
    /// each set of generic arguments it is called with (GEN-2), as a
    /// temporal one is.
    pub fn typed(program: &Program, callable_limits: CallableLimits) -> Instances {
        let mut instances = Instances {
            callable_limits,
            ..Instances::default()
        };
        for (index, module) in program.modules.iter().enumerate() {
            for (declaration, body) in module.bodies.iter().enumerate() {
                let value = module.declarations[declaration].kind.role() == Some(Role::Value);
                if body.is_some() && value && generic(program, index, declaration) {
                    instances.requiring.insert((index, declaration));
                }
            }
        }
        let mut waiting: VecDeque<Asked> = VecDeque::new();
        for (index, module) in program.modules.iter().enumerate() {
            for (declaration, body) in module.bodies.iter().enumerate() {
                // A body that is written by instance asks through its
                // instances. Every other body is written as it is, a
                // value function with generic parameters among them,
                // and a call in it may name whole arguments: the
                // `twice(1)` of `const fn pair<T>(value: T) -> i64`.
                let own = instances.instanced(program, index, declaration);
                if let (Some(body), false) = (body, own) {
                    waiting.extend(instances.asked(program, &body.statements));
                }
            }
            for body in module.test_bodies.iter().flatten() {
                waiting.extend(instances.asked(program, &body.statements));
            }
            // UNIT-14: an application's run statement, which names its
            // entry and is recorded by core's `record`.
            if let Some(run) = &module.run_body {
                waiting.extend(instances.asked_in(program, &run.call));
                let record = core_function(program, "record");
                if let (Some((module, declaration)), Some(result)) =
                    (record, run_result(program, run))
                {
                    waiting.push_back(Asked {
                        module,
                        declaration,
                        generics: vec![GenericArgument::Type(result)],
                        type_packs: Vec::new(),
                        constants: Vec::new(),
                        captures: Captures::default(),
                        requirements: Vec::new(),
                        functions: Vec::new(),
                        depths: Vec::new(),
                    });
                }
            }
            // UNIT-14: the statements an application sets its run's
            // store with, whose values may call what is written by
            // instance.
            if let Some(body) = &module.store_body {
                waiting.extend(instances.asked(program, &body.statements));
            }
            // What is worked out with no body around it: a constant's
            // value, and a default (UNIT-13, UNIT-8).
            let defaults = module.defaults.iter().flatten().flatten();
            for value in module.constants.iter().flatten().chain(defaults) {
                waiting.extend(instances.asked_in(program, &value.value));
            }
        }
        // Each call asked from where the search starts is a root, and
        // what an instance asks for is asked from that instance's root:
        // how many instances of each declaration each root has made.
        let mut waiting: VecDeque<(Asked, usize, Option<usize>)> = waiting
            .into_iter()
            .enumerate()
            .map(|(root, asked)| (asked, root, None))
            .collect();
        let mut made: BTreeMap<(usize, usize, usize), usize> = BTreeMap::new();
        let mut requests = Vec::new();
        while let Some((next, root, parent)) = waiting.pop_front() {
            let declaration = (next.module, next.declaration);
            let Some(key) = key(
                program,
                declaration,
                &next.generics,
                &next.type_packs,
                &next.captures,
                next.functions.iter().map(|(slot, target)| (*slot, target)),
                (&next.requirements, &next.constants),
            ) else {
                continue;
            };
            let found = instances.declared(next.module, next.declaration);
            if found.iter().any(|instance| instance.key == key) {
                continue;
            }
            if exceeds_growth(
                declaration,
                &next.depths,
                &requests,
                parent,
                instances.callable_limits.growth,
            ) {
                instances
                    .growing
                    .insert((next.module, next.declaration, key));
                continue;
            }
            // No rule says a program whose instances never end is
            // rejected or accepted: none is made past the analyser's
            // bound, so that writing the program ends.
            let count = made
                .entry((root, next.module, next.declaration))
                .or_insert(0);
            if *count >= hgl_analyser::INSTANCES {
                continue;
            }
            *count += 1;
            let Some((mut signature, mut body)) = program.instance_with_inputs(
                (next.module, next.declaration),
                &next.generics,
                &next.requirements,
                hgl_analyser::InstanceInputs {
                    functions: &next.functions,
                    type_packs: &next.type_packs,
                    pack_inputs: Some(&next.captures.inputs()),
                    constants: &next.constants,
                },
            ) else {
                continue;
            };
            let (slots, pack_use) = next.captures.expand(&mut signature, &mut body);
            // The arguments as part of a name.
            let mut plain: String = key[..key.len() - next.constants.len()]
                .iter()
                .map(|part| {
                    if part.starts_with(CONST_FUNCTION_PREFIX) {
                        part.split_once(':')
                            .map_or(part.as_str(), |(binding, _)| binding)
                    } else {
                        part.as_str()
                    }
                })
                .collect::<Vec<_>>()
                .join("_")
                .chars()
                .map(|character| {
                    if character.is_ascii_alphanumeric() {
                        character
                    } else {
                        '_'
                    }
                })
                .collect();
            if !next.constants.is_empty() {
                // The suffix distinguishes const instances without embedding
                // the text of a potentially long pattern in an identifier.
                plain.push_str("_const");
            }
            // Two sets of arguments may differ only in characters that
            // are no letter or digit: a number then tells them apart.
            let mut suffix = plain.clone();
            let mut number = found.len();
            while found.iter().any(|instance| instance.suffix == suffix) {
                suffix = format!("{plain}_{number}");
                number += 1;
            }
            let parent = if next.depths.is_empty() {
                parent
            } else {
                let index = requests.len();
                requests.push(Request {
                    declaration,
                    depths: next.depths,
                    parent,
                });
                Some(index)
            };
            waiting.extend(
                instances
                    .asked(program, &body.statements)
                    .into_iter()
                    .map(|asked| (asked, root, parent)),
            );
            instances
                .declared
                .entry((next.module, next.declaration))
                .or_default()
                .push(Instance {
                    key,
                    suffix,
                    signature,
                    body,
                    slots,
                    pack_use,
                });
        }
        instances
    }

    /// The instances `value` asks for: the one the call of a
    /// declaration that is written by instance names, and for an eval
    /// the ones its run is wired with, core's `replay` for each input
    /// it gives a trace and core's `record` for its target's result
    /// (core's CEVAL-1).
    fn asks(
        &self,
        program: &Program,
        value: &Expr,
        expanding: &mut Vec<(usize, usize, usize)>,
    ) -> Vec<Asked> {
        if let ExprKind::Eval { call, .. } = &value.kind {
            let Some(wiring) = run_wiring(program, call, &value.ty) else {
                return Vec::new();
            };
            let of = |function: Option<(usize, usize)>, ty: &Type| {
                function.map(|(module, declaration)| Asked {
                    module,
                    declaration,
                    generics: vec![GenericArgument::Type(ty.clone())],
                    type_packs: Vec::new(),
                    constants: Vec::new(),
                    captures: Captures::default(),
                    requirements: Vec::new(),
                    functions: Vec::new(),
                    depths: Vec::new(),
                })
            };
            let replay = core_function(program, "replay");
            let record = core_function(program, "record");
            return wiring
                .inputs
                .iter()
                .filter_map(|(_, ty)| of(replay, ty))
                .chain(wiring.result.as_ref().and_then(|ty| of(record, ty)))
                .collect();
        }
        let ExprKind::Call {
            callee, arguments, ..
        } = &value.kind
        else {
            return Vec::new();
        };
        if let Callee::Anonymous(function) = callee {
            let mut found = Vec::new();
            walk_statements(&function.body.statements, &mut |value| {
                found.extend(self.asks(program, value, expanding));
            });
            return found;
        }
        let (Callee::Function {
            module,
            declaration,
            generics,
            requirements,
            ..
        }
        | Callee::Candidate {
            module,
            declaration,
            generics,
            requirements,
            ..
        }) = callee
        else {
            return Vec::new();
        };
        let Some(Detail::Callable(signature)) = program.modules[*module].details.get(*declaration)
        else {
            return Vec::new();
        };
        let mut found = Vec::new();
        // Check the borrowed arguments before cloning them into a request.
        if self.instanced(program, *module, *declaration)
            && let Ok(depths) = request_depths(
                program,
                (*module, *declaration),
                arguments,
                &self.callable_limits,
            )
            && let Ok(constants) = program.call_const_inputs(callee, arguments)
        {
            found.push(Asked {
                module: *module,
                declaration: *declaration,
                generics: generics.clone(),
                type_packs: callee.type_packs().to_vec(),
                constants,
                captures: Captures::of(program, callee, signature, arguments),
                requirements: requirements.clone(),
                functions: functions_of(arguments),
                depths,
            });
        }
        // The declaration's open default was already visited. Here it may
        // ask for a concrete instance once the operator's arguments are placed.
        if let Callee::Candidate {
            operator_generics, ..
        } = callee
            && !operator_generics.is_empty()
        {
            for (index, parameter) in signature.parameters.iter().enumerate() {
                let key = (*module, *declaration, index);
                if !parameter.configuration
                    || arguments
                        .iter()
                        .any(|argument| argument.parameter == Some(index))
                    || expanding.contains(&key)
                {
                    continue;
                }
                if let Some(LeftOut::Default(DefaultValue::Specialized(default))) =
                    left_out(program, callee, index, parameter)
                {
                    expanding.push(key);
                    default
                        .value
                        .walk(&mut |value| found.extend(self.asks(program, value, expanding)));
                    expanding.pop();
                }
            }
        }
        found
    }

    /// The instances the calls in `statements` ask for, in the order
    /// the calls are met.
    fn asked(&self, program: &Program, statements: &[Stmt]) -> Vec<Asked> {
        let mut found = Vec::new();
        let mut expanding = Vec::new();
        walk_statements(statements, &mut |value| {
            found.extend(self.asks(program, value, &mut expanding));
        });
        found
    }

    /// The instances the calls in `expression` ask for, in the order
    /// the calls are met.
    fn asked_in(&self, program: &Program, expression: &Expr) -> Vec<Asked> {
        let mut found = Vec::new();
        let mut expanding = Vec::new();
        expression.walk(&mut |value| found.extend(self.asks(program, value, &mut expanding)));
        found
    }

    /// True for a declaration that is written by instance: a function or
    /// a candidate with generic parameters and a body.
    pub fn instanced(&self, program: &Program, module: usize, declaration: usize) -> bool {
        generic_temporal(program, module, declaration)
            || self.requiring.contains(&(module, declaration))
            || program.needs_const_inputs((module, declaration))
    }

    /// The instances of `declaration` of `module`, in the order they
    /// were first asked for.
    pub fn declared(&self, module: usize, declaration: usize) -> &[Instance] {
        self.declared
            .get(&(module, declaration))
            .map_or(&[], Vec::as_slice)
    }

    /// The instance of `declaration` of `module` for the generic
    /// arguments `generics`. `None` where they are not whole, or the
    /// declaration has no body to write.
    pub fn find(
        &self,
        program: &Program,
        (module, declaration): (usize, usize),
        generics: &[GenericArgument],
    ) -> Option<&Instance> {
        let key = key(
            program,
            (module, declaration),
            generics,
            &[],
            &Captures::default(),
            std::iter::empty(),
            (&[], &[]),
        )?;
        self.declared(module, declaration)
            .iter()
            .find(|instance| instance.key == key)
    }

    /// The concrete instance this analysed call requests, including any
    /// captured argument types and keys (FN-8, GEN-10).
    pub fn find_call(
        &self,
        program: &Program,
        callee: &Callee,
        arguments: &[Argument],
    ) -> Option<&Instance> {
        let (module, declaration) = callee.declaration()?;
        request_depths(
            program,
            (module, declaration),
            arguments,
            &self.callable_limits,
        )
        .ok()?;
        let key = call_key(program, callee, arguments)?;
        self.declared(module, declaration)
            .iter()
            .find(|instance| instance.key == key)
    }

    /// The reason this missing instance exceeded the target's limits.
    pub fn callable_limit(
        &self,
        program: &Program,
        callee: &Callee,
        arguments: &[Argument],
    ) -> Option<CallableLimit> {
        let (module, declaration) = callee.declaration()?;
        if let Err(limit) = request_depths(
            program,
            (module, declaration),
            arguments,
            &self.callable_limits,
        ) {
            return Some(limit);
        }
        if self.growing.is_empty() {
            return None;
        }
        let key = call_key(program, callee, arguments)?;
        self.growing
            .contains(&(module, declaration, key))
            .then_some(CallableLimit::Growth)
    }
}

fn call_key(program: &Program, callee: &Callee, arguments: &[Argument]) -> Option<Vec<String>> {
    let (module, declaration) = callee.declaration()?;
    let Detail::Callable(signature) = program.modules[module].details.get(declaration)? else {
        return None;
    };
    let captures = Captures::of(program, callee, signature, arguments);
    let constants = program.call_const_inputs(callee, arguments).ok()?;
    let requirements = match callee {
        Callee::Function { requirements, .. } | Callee::Candidate { requirements, .. } => {
            requirements.as_slice()
        }
        _ => &[],
    };
    key(
        program,
        (module, declaration),
        callee.generics(),
        callee.type_packs(),
        &captures,
        function_arguments(arguments),
        (requirements, &constants),
    )
}
