//! TST-1 makes the candidates of all named tests visible within their module's test runs.

use std::num::NonZeroUsize;

use crate::program::{Module, Program};

/// The module whose testing run makes its test-declared candidates visible.
///
/// It records candidate visibility only. An operator, type or helper that
/// a named test declares still belongs to that lexical test.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TestCandidateScope(NonZeroUsize);

impl TestCandidateScope {
    pub(crate) fn of(program: &Program, module: &Module) -> Option<Self> {
        if !has_candidates(module) {
            return None;
        }
        let index = program.module_index(&module.path)?;
        NonZeroUsize::new(index + 1).map(Self)
    }

    /// The position of the testing module in `Program::modules`.
    pub fn module(self) -> usize {
        self.0.get() - 1
    }
}

fn has_candidates(module: &Module) -> bool {
    module.declarations.iter().any(|declaration| {
        declaration.test_owner.is_some() && declaration.kind.candidate().is_some()
    })
}

/// Remove roots for test-view specialization after source checking (TST-1).
/// A production command still diagnoses all written bodies. When a module
/// declares test candidates, only its production roots reach the target.
pub(crate) fn omit_test_artifacts(program: &mut Program) {
    for module in &mut program.modules {
        if !has_candidates(module) {
            continue;
        }
        module.test_bodies.fill(None);
        for (index, declaration) in module.declarations.iter().enumerate() {
            if declaration.test_only {
                module.bodies[index] = None;
                module.constants[index] = None;
                module.defaults[index].clear();
                module.selecting[index].clear();
                module.graph_return_checks[index] = None;
            }
        }
    }
}
