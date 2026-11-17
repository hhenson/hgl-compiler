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
//! candidate that is depends on the types. So is one that reads a
//! generic value parameter as a value (GEN-13), and one that calls a
//! function written by instance with its own generic parameters, which
//! are known only in an instance of it.
//!
//! An application's run statement is not written yet, and is not read
//! here.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use hgl_analyser::hir::{Answer, Body, Callee, Expr, ExprKind, Stmt, walk_statements};
use hgl_analyser::{Argument as GenericArgument, Detail, Program, Role, Signature};

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
    /// found once, and the analyser gives up a type that grows past
    /// its bound, which is then not whole.
    pub(crate) fn of(program: &Program) -> Instances {
        let mut instances = Instances::default();
        for (index, module) in program.modules.iter().enumerate() {
            for (declaration, body) in module.bodies.iter().enumerate() {
                let value = module.declarations[declaration].kind.role() == Some(Role::Value);
                if let (Some(body), true) = (body, value)
                    && generic(program, index, declaration)
                    && requires(body)
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
            // What is worked out with no body around it: a constant's
            // value, and a default (UNIT-13, UNIT-8).
            let defaults = module.defaults.iter().flatten().flatten();
            for value in module.constants.iter().flatten().chain(defaults) {
                waiting.extend(instances.asked_in(program, value));
            }
        }
        while let Some(next) = waiting.pop_front() {
            let Some(key) = key(program, (next.module, next.declaration), &next.generics) else {
                continue;
            };
            let found = instances.declared(next.module, next.declaration);
            if found.iter().any(|instance| instance.key == key) {
                continue;
            }
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
            waiting.extend(instances.asked(program, &body.statements));
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

    /// The instance the call `value` asks for, when it is the call of a
    /// declaration that is written by instance.
    fn asks(&self, program: &Program, value: &Expr) -> Option<Asked> {
        let ExprKind::Call { callee, .. } = &value.kind else {
            return None;
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
            return None;
        };
        self.instanced(program, *module, *declaration)
            .then(|| Asked {
                module: *module,
                declaration: *declaration,
                generics: generics.clone(),
                requirements: requirements.clone(),
            })
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
