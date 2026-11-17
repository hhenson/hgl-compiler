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
//! has no generic parameter of its own, and from every test. The
//! analyser gives the body of each instance
//! ([`Program::instance`]), whose own calls ask for more: the graph
//! above asks for core's candidate of `+` for `f64`, which core's
//! OPS-9 gives a body of its own.
//!
//! A value function with generic parameters is not written this way:
//! Python takes a value of any type. An application's run statement is
//! not written yet, and is not read here.

use std::collections::{BTreeMap, VecDeque};

use hgl_analyser::hir::{Answer, Body, Callee, ExprKind, walk_statements};
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

/// True for a declaration that is written by instance: a temporal
/// function, or a temporal candidate of an operator, that has generic
/// parameters and a body.
pub(crate) fn instanced(program: &Program, module: usize, declaration: usize) -> bool {
    let Some(declared) = program.modules.get(module) else {
        return false;
    };
    let temporal = declared
        .declarations
        .get(declaration)
        .is_some_and(|declaration| declaration.kind.role() == Some(Role::Temporal));
    let generic = matches!(
        declared.details.get(declaration),
        Some(Detail::Callable(signature)) if !signature.generics.is_empty()
    );
    let bodied = matches!(declared.bodies.get(declaration), Some(Some(_)));
    temporal && generic && bodied
}

/// The generic arguments of a call as an instance's key. `None` where
/// one of them is not whole: it names a generic parameter, or is not
/// known.
fn key(program: &Program, generics: &[GenericArgument]) -> Option<Vec<String>> {
    generics
        .iter()
        .map(|argument| canonical_argument(program, argument))
        .collect()
}

/// The instances the calls in `body` ask for, in the order the calls
/// are met.
fn asked(program: &Program, body: &Body) -> Vec<Asked> {
    let mut found = Vec::new();
    walk_statements(&body.statements, &mut |value| {
        let ExprKind::Call { callee, .. } = &value.kind else {
            return;
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
            return;
        };
        if instanced(program, *module, *declaration) {
            found.push(Asked {
                module: *module,
                declaration: *declaration,
                generics: generics.clone(),
                requirements: requirements.clone(),
            });
        }
    });
    found
}

impl Instances {
    /// The instances `program` asks for.
    ///
    /// A generic body asks with its own generic parameters, which says
    /// nothing until that body is an instance itself: so the search
    /// starts from the bodies with none, and from the tests, and goes
    /// on through each instance it finds. It ends: an instance is
    /// found once, and the analyser gives up a type that grows past
    /// its bound, which is then not whole.
    pub(crate) fn of(program: &Program) -> Instances {
        let mut instances = Instances::default();
        let mut waiting: VecDeque<Asked> = VecDeque::new();
        for module in &program.modules {
            for (declaration, body) in module.bodies.iter().enumerate() {
                let generic = matches!(
                    module.details.get(declaration),
                    Some(Detail::Callable(signature)) if !signature.generics.is_empty()
                );
                if let (Some(body), false) = (body, generic) {
                    waiting.extend(asked(program, body));
                }
            }
            for body in module.test_bodies.iter().flatten() {
                waiting.extend(asked(program, body));
            }
        }
        while let Some(next) = waiting.pop_front() {
            let Some(key) = key(program, &next.generics) else {
                continue;
            };
            let declared = instances
                .declared
                .entry((next.module, next.declaration))
                .or_default();
            if declared.iter().any(|instance| instance.key == key) {
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
            let mut suffix: String = key
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
            if declared.iter().any(|instance| instance.suffix == suffix) {
                suffix.push_str(&format!("_{}", declared.len()));
            }
            waiting.extend(asked(program, &body));
            declared.push(Instance {
                key,
                suffix,
                signature,
                body,
            });
        }
        instances
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
        let key = key(program, generics)?;
        self.declared(module, declaration)
            .iter()
            .find(|instance| instance.key == key)
    }
}
