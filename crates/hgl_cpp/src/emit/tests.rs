//! Writes a named test (TST-1 to TST-4) as a C++ function of the
//! module's test namespace, which the module's entry unit runs when its
//! command line names it. Part of the transpiler, the last stage.
//!
//! Each statement of the test is told to the runtime library's test
//! host before it runs, `::hglrt::testing::statement()`, which takes the
//! line it is called on, so that a failure names the statement that was
//! running. A test's statements are written as a value function's are
//! (`statements`), and two are a test's own: `assert condition` is
//! `::hglrt::testing::check(condition)` (TST-4), and `assert
//! raises("code") { ... }` is `::hglrt::testing::raises("code", ...)`
//! of the block (TST-10). An eval is not written yet.

use super::Emitter;
use super::names::{self, identifier};
use super::statements::{Scope, statement_at};
use super::values::INDENT;

impl Emitter<'_> {
    /// Writes named test `index` of the module.
    pub(super) fn test(&mut self, index: usize) {
        let declared = &self.program.modules[self.module];
        let test = &declared.tests[index];
        let Some(Some(body)) = declared.test_bodies.get(index) else {
            self.unsupported(test.at, "a test with no block");
            return;
        };
        let mut scope = Scope::cold(self.program, body);
        scope.testing = true;
        let mut text = format!("void {}()\n{{\n", identifier(&test.name));
        for statement in &body.statements {
            // The line the statement starts on, counted from 0 in the
            // piece's text.
            let line = text.matches('\n').count();
            self.piece
                .statements
                .push((line, statement_at(statement, &body.locals)));
            let marked = names::testing("statement");
            text.push_str(&format!("{INDENT}{marked}();\n"));
            if let hgl_analyser::hir::Stmt::Return { at, .. } = statement {
                self.unsupported(*at, "a `return` in a test");
                continue;
            }
            self.statement(&mut scope, statement, 1, &mut text);
        }
        text.push_str("}\n");
        self.piece.text = text;
    }
}
