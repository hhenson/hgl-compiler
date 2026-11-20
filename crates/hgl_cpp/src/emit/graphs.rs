//! Writes temporal functions whose body is a graph's (FN-2: typed
//! connections and no payload read) as functions over hgraph's
//! `Wiring&` and typed ports, and an application's run statement
//! (UNIT-14, RUN-1) as its entry. Part of the transpiler, the last
//! stage.
//!
//! None is written yet: each is reported where it is written.

use super::Emitter;

impl Emitter<'_> {
    /// Writes temporal function `index` of the module, whose body is a
    /// graph's.
    pub(super) fn graph(&mut self, index: usize) {
        let at = self.program.modules[self.module].declarations[index].at;
        self.unsupported(at, "a graph");
    }

    /// Writes the module's run statement, for an application (RUN-1).
    pub(super) fn application(&mut self) {
        let declared = &self.program.modules[self.module];
        let at = declared
            .run_body
            .as_ref()
            .map(|run| run.at)
            .or_else(|| declared.run.as_ref().map(|run| run.at))
            .or_else(|| declared.application_at());
        match at {
            Some(at) => self.unsupported(at, "an application's run statement"),
            None => {
                // An application with no run statement to read was
                // rejected; there is nothing to write.
            }
        }
    }
}
