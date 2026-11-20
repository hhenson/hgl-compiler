//! Writes a named test (TST-1 to TST-4) as a C++ function of the
//! module's test namespace, which the module's entry unit runs when its
//! command line names it. Part of the transpiler, the last stage.
//!
//! Each statement of the test is told to the runtime library's test
//! host before it runs, `::hglrt::testing::statement()`, which takes the
//! line it is called on, so that a failure names the statement that was
//! running; `assert condition` is `::hglrt::testing::check(condition)`
//! (TST-4). A test's statements are written as a value function's are;
//! an eval, and every other form, is not written yet.

use hgl_analyser::hir::{Body, Local, Stmt};
use hgl_diagnostics::Location;

use super::Emitter;
use super::names::{self, identifier};
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
            self.test_statement(body, statement, &mut text);
        }
        text.push_str("}\n");
        self.piece.text = text;
    }

    /// Writes `statement` of a test's `body` into `text`.
    fn test_statement(&mut self, body: &Body, statement: &Stmt, text: &mut String) {
        match statement {
            // TST-4: "An `assert` takes a `bool`".
            Stmt::Assert { condition, .. } => {
                let condition = self.expression(body, condition);
                let check = names::testing("check");
                text.push_str(&format!("{INDENT}{check}({condition});\n"));
            }
            Stmt::Return { at, .. } => self.unsupported(*at, "a `return` in a test"),
            other => self.statement(body, other, 1, text),
        }
    }
}

/// Where `statement` is written: a declaration where the name it
/// declares is, `locals` being the body's.
fn statement_at(statement: &Stmt, locals: &[Local]) -> Location {
    match statement {
        Stmt::Declare { local, .. } => locals[*local].at,
        Stmt::Assign { at, .. }
        | Stmt::AssignPlace { at, .. }
        | Stmt::Inject { at, .. }
        | Stmt::Hook { at, .. }
        | Stmt::Return { at, .. }
        | Stmt::Assert { at, .. }
        | Stmt::AssertRaises { at, .. }
        | Stmt::Yield { at, .. }
        | Stmt::For { at, .. }
        | Stmt::While { at, .. }
        | Stmt::Switch { at, .. }
        | Stmt::Unread { at } => *at,
        Stmt::Expr(value) => value.at,
    }
}
