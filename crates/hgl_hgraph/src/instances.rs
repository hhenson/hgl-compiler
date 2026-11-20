//! The instances of generic temporal declarations a program asks for.
//!
//! An hgraph node or graph is written with the types of its inputs, and
//! a node's code calls the function chosen for those types. So a
//! temporal function with generic parameters, and a temporal candidate
//! of an operator with them, is written once for each set of generic
//! arguments it is called with (GEN-2):
//!
//! ```text
//! fn identity_sum<T>(lhs: T, rhs: T) -> T
//!     requires add_(T, T) -> T => lhs +{invalid: .identity} rhs
//! ```
//!
//! called with two `f64` and with two `str` is two graphs,
//! `_inst_identity_sum__f64` and `_inst_identity_sum__str`.
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
//! A value function with generic parameters is written once where it
//! can be: Python takes a value of any type. It is written by instance
//! too where its body calls an operation the declaration requires, or
//! hands one of its requirements on to what it calls (GEN-16): which
//! candidate that is depends on the types, and where its body has a call
//! a membership it states authorises, whose candidate each instance
//! selects (hhenson/hgl_spec#144). So is one that reads a generic value parameter as a value
//! (GEN-13), and one that calls a
//! function written by instance with its own generic parameters, which
//! are known only in an instance of it.
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

use hgl_analyser::hir::{Answer, Body, Callee, Expr, ExprKind, Run, Stmt, walk_statements};
use hgl_analyser::{
    Argument as GenericArgument, DeclarationKind, Detail, Program, Role, Signature, Type,
};

use crate::emit::canonical_argument;

/// A generic temporal declaration as one set of generic arguments takes
/// it.
pub(crate) struct Instance {
    /// Each generic argument as text no other is written as
    /// (`canonical_argument`): what tells one instance from another.
    key: Vec<String>,
    /// What follows the declaration's own name in the instance's Python
    /// name: the generic arguments, each character that is no letter
    /// or digit as `_`, and a number where two instances would
    /// otherwise have one name.
    pub(crate) suffix: String,
    /// Its signature, with the generic arguments in place.
    pub(crate) signature: Signature,
    /// Its body, with them in place, and each operation the declaration
    /// requires as the call of the candidate that answers it (GEN-16).
    pub(crate) body: Body,
}

/// The instances of each generic temporal declaration of a program, in
/// the order they are first asked for.
#[derive(Default)]
pub(crate) struct Instances {
    /// By the declaration: the position of its module in the program,
    /// and its position among that module's declarations.
    declared: BTreeMap<(usize, usize), Vec<Instance>>,
    /// The value functions and value candidates that are written by
    /// instance: those whose body [`requires`], and those that call
    /// one written by instance with arguments that are not whole.
    requiring: BTreeSet<(usize, usize)>,
    /// The instances asked for and not made, each by its declaration and
    /// its key: those past the [`hgl_analyser::INSTANCES`] of one
    /// declaration that one call makes.
    past: BTreeSet<(usize, usize, Vec<String>)>,
}

/// One call's request for an instance.
struct Asked {
    /// The position of the declaration's module in the program.
    module: usize,
    /// The declaration's position in that module.
    declaration: usize,
    /// What each of its generic parameters stands for at the call.
    generics: Vec<GenericArgument>,
    /// What answers each requirement it states there.
    requirements: Vec<Answer>,
}

/// What an eval's run is wired with (core's CEVAL-1: "core's `replay`
/// wired to each temporal input of the target, and the target's result
/// as the entry's").
pub(crate) struct Wiring {
    /// Each temporal input the eval gives a trace: its parameter's
    /// name, and the type the input is of.
    pub(crate) inputs: Vec<(String, Type)>,
    /// The type of the target's result, where it has one.
    pub(crate) result: Option<Type>,
}

/// The endpoint payload for a trace of publications (TST-5, TYPE-16).
/// Replays and the lifted node receiving those replays use the same type.
pub(crate) fn trace_payload(trace: &Type) -> Option<Type> {
    let Type::Trace { cell, .. } = trace else {
        return None;
    };
    Some(published_type(cell))
}

/// The type whose publication `cell` is the delta of: a delta that is
/// a type of its own names it, a scalar is its own, and a tuple's delta
/// is "`tuple<delta<A>?, delta<B>?>`" (TYPE-16), each position the delta
/// of the tuple's under a `?`.
fn published_type(cell: &Type) -> Type {
    match cell {
        Type::Delta(payload) => payload.as_ref().clone(),
        Type::Tuple(positions) => Type::Tuple(
            positions
                .iter()
                .map(|position| match position {
                    Type::Nullable(changed) => published_type(changed),
                    other => published_type(other),
                })
                .collect(),
        ),
        scalar => scalar.clone(),
    }
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
pub(crate) fn run_wiring(program: &Program, call: &Expr, eval: &Type) -> Option<Wiring> {
    let ExprKind::Call {
        callee:
            Callee::Function {
                module,
                declaration,
                generics,
                requirements,
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
    let instance = if generics.is_empty() {
        None
    } else {
        program.instance(*module, *declaration, generics, requirements)
    };
    // Where the target's own types are known, its inputs and result are
    // of them; otherwise they are read back from the traces.
    let known = signature.generics.is_empty() || instance.is_some();
    let parameters = instance
        .as_ref()
        .map_or(&signature.parameters, |(taken, _)| &taken.parameters);
    let inputs = arguments
        .iter()
        .filter_map(|argument| {
            let parameter = parameters.get(argument.parameter?)?;
            // Only an argument that is a trace is replayed.
            let payload = trace_payload(&argument.value.ty)?;
            // TYPE-16: a window's delta is "V: the value that arrived",
            // so a trace of one is a trace of V: the window is the
            // parameter's.
            let windowed = matches!(parameter.ty, Type::RollingWindow { .. });
            Some((
                parameter.name.clone(),
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
pub(crate) fn run_result(program: &Program, run: &Run) -> Option<Type> {
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
pub(crate) fn core_function(program: &Program, name: &str) -> Option<(usize, usize)> {
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
        Some(Detail::Callable(signature)) if !signature.generics.is_empty()
    );
    generic && matches!(declared.bodies.get(declaration), Some(Some(_)))
}

/// True for a body that one call of its declaration says something
/// of. It needs to know what answers its declaration's requirements
/// (GEN-16): it calls an operation the declaration requires, or one of
/// its calls is answered by a requirement the declaration states,
/// which it so hands on. Or it reads a generic value parameter as a
/// value, which is the constant the call gives (GEN-13).
fn requires(body: &Body) -> bool {
    fn stated(answers: &[Answer]) -> bool {
        answers.iter().any(|answer| match answer {
            Answer::Stated(_) => true,
            Answer::Candidate { requirements, .. } => stated(requirements),
            Answer::Unread => false,
        })
    }
    let mut found = false;
    walk_statements(&body.statements, &mut |value| {
        found |= match &value.kind {
            ExprKind::GenericValue { .. } => true,
            ExprKind::Call { callee, .. } => match callee {
                Callee::Required { .. } => true,
                Callee::Function { requirements, .. } | Callee::Candidate { requirements, .. } => {
                    stated(requirements)
                }
                Callee::Unread => false,
            },
            _ => false,
        };
    });
    found
}

/// The generic arguments of a call of `declaration` of `module` as an
/// instance's key. `None` where they are not whole: one of them names
/// a generic parameter or is not known, or the call does not say what
/// each generic parameter of the declaration is.
fn key(
    program: &Program,
    (module, declaration): (usize, usize),
    generics: &[GenericArgument],
) -> Option<Vec<String>> {
    let Some(Detail::Callable(signature)) = program.modules.get(module)?.details.get(declaration)
    else {
        return None;
    };
    if generics.len() != signature.generics.len() {
        return None;
    }
    generics
        .iter()
        .map(|argument| canonical_argument(program, argument))
        .collect()
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
    /// ([`hgl_analyser::INSTANCES`]): one past them is not made, and a
    /// call of it is reported.
    pub(crate) fn of(program: &Program) -> Instances {
        let mut instances = Instances::default();
        for (index, module) in program.modules.iter().enumerate() {
            for (declaration, body) in module.bodies.iter().enumerate() {
                let value = module.declarations[declaration].kind.role() == Some(Role::Value);
                // A body with a call a membership authorises is written
                // for each instance, which calls the candidate it selects.
                if let (Some(body), true) = (body, value)
                    && generic(program, index, declaration)
                    && (requires(body) || module.selects_per_instance(declaration))
                {
                    instances.requiring.insert((index, declaration));
                }
            }
        }
        // A value function that calls one written by instance with its
        // own generic parameters is written by instance too: only an
        // instance of it says which instance it calls. That makes more
        // of them so, until it makes no more.
        loop {
            let mut more = Vec::new();
            for (index, module) in program.modules.iter().enumerate() {
                for (declaration, body) in module.bodies.iter().enumerate() {
                    let value = module.declarations[declaration].kind.role() == Some(Role::Value);
                    let (Some(body), true) = (body, value) else {
                        continue;
                    };
                    if !generic(program, index, declaration)
                        || instances.requiring.contains(&(index, declaration))
                    {
                        continue;
                    }
                    let open = instances
                        .asked(program, &body.statements)
                        .iter()
                        .any(|asked| {
                            key(program, (asked.module, asked.declaration), &asked.generics)
                                .is_none()
                        });
                    if open {
                        more.push((index, declaration));
                    }
                }
            }
            if more.is_empty() {
                break;
            }
            instances.requiring.extend(more);
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
                if let Some(config) = &run.config {
                    waiting.extend(instances.asked_in(program, config));
                }
                let record = core_function(program, "record");
                if let (Some((module, declaration)), Some(result)) =
                    (record, run_result(program, run))
                {
                    waiting.push_back(Asked {
                        module,
                        declaration,
                        generics: vec![GenericArgument::Type(result)],
                        requirements: Vec::new(),
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
                waiting.extend(instances.asked_in(program, value));
            }
        }
        // Each call asked from where the search starts is a root, and
        // what an instance asks for is asked from that instance's root:
        // how many instances of each declaration each root has made.
        let mut waiting: VecDeque<(Asked, usize)> = waiting
            .into_iter()
            .enumerate()
            .map(|(root, asked)| (asked, root))
            .collect();
        let mut made: BTreeMap<(usize, usize, usize), usize> = BTreeMap::new();
        while let Some((next, root)) = waiting.pop_front() {
            let Some(key) = key(program, (next.module, next.declaration), &next.generics) else {
                continue;
            };
            let found = instances.declared(next.module, next.declaration);
            if found.iter().any(|instance| instance.key == key) {
                continue;
            }
            // No rule says a program whose instances never end is
            // rejected or accepted: none is made past the analyser's
            // bound, so that writing the program ends.
            let count = made
                .entry((root, next.module, next.declaration))
                .or_insert(0);
            if *count >= hgl_analyser::INSTANCES {
                instances.past.insert((next.module, next.declaration, key));
                continue;
            }
            *count += 1;
            let Some((signature, body)) = program.instance(
                next.module,
                next.declaration,
                &next.generics,
                &next.requirements,
            ) else {
                continue;
            };
            // The arguments as part of a Python name.
            let plain: String = key
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
            // Two sets of arguments may differ only in characters that
            // are no letter or digit: a number then tells them apart.
            let mut suffix = plain.clone();
            let mut number = found.len();
            while found.iter().any(|instance| instance.suffix == suffix) {
                suffix = format!("{plain}_{number}");
                number += 1;
            }
            waiting.extend(
                instances
                    .asked(program, &body.statements)
                    .into_iter()
                    .map(|asked| (asked, root)),
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
                });
        }
        instances
    }

    /// The instances `value` asks for: the one the call of a
    /// declaration that is written by instance names, and for an eval
    /// the ones its run is wired with, core's `replay` for each input
    /// it gives a trace and core's `record` for its target's result
    /// (core's CEVAL-1).
    fn asks(&self, program: &Program, value: &Expr) -> Vec<Asked> {
        if let ExprKind::Eval { call, .. } = &value.kind {
            let Some(wiring) = run_wiring(program, call, &value.ty) else {
                return Vec::new();
            };
            let of = |function: Option<(usize, usize)>, ty: &Type| {
                function.map(|(module, declaration)| Asked {
                    module,
                    declaration,
                    generics: vec![GenericArgument::Type(ty.clone())],
                    requirements: Vec::new(),
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
        let ExprKind::Call { callee, .. } = &value.kind else {
            return Vec::new();
        };
        let (Callee::Function {
            module,
            declaration,
            generics,
            requirements,
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
        if !self.instanced(program, *module, *declaration) {
            return Vec::new();
        }
        vec![Asked {
            module: *module,
            declaration: *declaration,
            generics: generics.clone(),
            requirements: requirements.clone(),
        }]
    }

    /// The instances the calls in `statements` ask for, in the order
    /// the calls are met.
    fn asked(&self, program: &Program, statements: &[Stmt]) -> Vec<Asked> {
        let mut found = Vec::new();
        walk_statements(statements, &mut |value| {
            found.extend(self.asks(program, value));
        });
        found
    }

    /// The instances the calls in `expression` ask for, in the order
    /// the calls are met.
    fn asked_in(&self, program: &Program, expression: &Expr) -> Vec<Asked> {
        let mut found = Vec::new();
        expression.walk(&mut |value| found.extend(self.asks(program, value)));
        found
    }

    /// True for a declaration that is written by instance: a temporal
    /// function or candidate with generic parameters and a body, and a
    /// value one whose body needs to know what answers its
    /// requirements, or calls one written by instance with its own
    /// generic parameters.
    pub(crate) fn instanced(&self, program: &Program, module: usize, declaration: usize) -> bool {
        generic_temporal(program, module, declaration)
            || self.requiring.contains(&(module, declaration))
    }

    /// The instances of `declaration` of `module`, in the order they
    /// were first asked for.
    pub(crate) fn declared(&self, module: usize, declaration: usize) -> &[Instance] {
        self.declared
            .get(&(module, declaration))
            .map_or(&[], Vec::as_slice)
    }

    /// True where the instance of `declaration` of `module` for the
    /// generic arguments `generics` was asked for and not made: it is
    /// past the instances of it one call makes ([`hgl_analyser::INSTANCES`]).
    pub(crate) fn past(
        &self,
        program: &Program,
        (module, declaration): (usize, usize),
        generics: &[GenericArgument],
    ) -> bool {
        key(program, (module, declaration), generics)
            .is_some_and(|key| self.past.contains(&(module, declaration, key)))
    }

    /// The instance of `declaration` of `module` for the generic
    /// arguments `generics`. `None` where they are not whole, or the
    /// declaration has no body to write.
    pub(crate) fn find(
        &self,
        program: &Program,
        (module, declaration): (usize, usize),
        generics: &[GenericArgument],
    ) -> Option<&Instance> {
        let key = key(program, (module, declaration), generics)?;
        self.declared(module, declaration)
            .iter()
            .find(|instance| instance.key == key)
    }
}
