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
        if !module.declarations.iter().any(|declaration| {
            declaration.test_owner.is_some() && declaration.kind.candidate().is_some()
        }) {
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
