//! Writes temporal functions whose body is a node's (FN-2: state and
//! hooks that run when the node is evaluated) as hgraph static nodes:
//! a struct with `eval`, `start` and `stop`, and a wiring function that
//! adds it to a graph. Part of the transpiler, the last stage.
//!
//! None is written yet: each is reported where it is declared, and so
//! is a temporal function whose body the runtime library supplies.

use hgl_analyser::Phase;

use super::Emitter;

impl Emitter<'_> {
    /// Writes temporal function `index` of the module: a node, or a
    /// graph (`graphs`), as its body describes (FN-2).
    pub(super) fn temporal_function(&mut self, index: usize) {
        let declared = &self.program.modules[self.module];
        let at = declared.declarations[index].at;
        match declared.phases.get(index).copied().flatten() {
            Some(Phase::Node) => self.unsupported(at, "a node"),
            Some(Phase::Graph) => self.graph(index),
            None => self.unsupported(at, "a temporal function with no HGL body"),
        }
    }
}
