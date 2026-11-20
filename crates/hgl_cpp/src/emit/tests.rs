//! Writes a named test (TST-1 to TST-4) as a C++ function of the
//! module's test namespace, which the module's entry unit runs when its
//! command line names it, and the evals and traces in it. Part of the
//! transpiler, the last stage; a family of the dispatch point (`forms`)
//! for an eval, a trace and their comparison.
//!
//! Each statement of the test is told to the runtime library's test
//! host before it runs, `::hglrt::testing::statement()`, which takes the
//! line it is called on, so that a failure names the statement that was
//! running. A test's statements are written as a value function's are
//! (`statements`), and two are a test's own: `assert condition` is
//! `::hglrt::testing::check(condition)` (TST-4), and `assert
//! raises("code") { ... }` is `::hglrt::testing::raises("code", ...)`
//! of the block (TST-10).
//!
//! | HGL | C++ |
//! | --- | --- |
//! | `eval(target(value: [1, _, 3]))` (TST-5, CEVAL-1) | `::hglrt::testing::evaluate<T>(entry)`: the entry wires core's `replay` of each trace written to the input it is given, the target, and core's `record` of its result under `"run.output"`; `run(...)` in an integration test runs in real time (TST-17); an eval of a target with no result is `::hglrt::testing::run(entry)` (TST-8) |
//! | `[7, _, 9]`, `[0s: 7, 2s: 9]` (TST-6, TST-7) | `::hglrt::testing::Dense<T>{7, std::nullopt, 9}`, `::hglrt::testing::Timed<T>{...}`, each cell as its type's family writes it |
//! | `assert left == right` of two traces (TST-4) | `::hglrt::testing::expect_same(left, right)`, the left one evaluated first; `!=` is `expect_different`; either as a value is `::hglrt::testing::same` |
//! | A trace given to a `const fn` (TST-16) | `::hglrt::testing::publications(trace)`, its publications as a list |
//! | `let seen = eval(...)`, a `let` of a trace | `const auto seen = ...;`: the recording, or the trace written out, as the test host has it |
//!
//! An eval's configuration is the lifecycle's (`lifecycle`); an eval of a
//! value function is not written yet, and is reported.

use hgl_analyser::hir::{Cell, Expr, ExprKind, Stmt};
use hgl_analyser::{Argument as GenericArgument, Type};
use hgl_lower::instances::{core_function, run_wiring, trace_payload};

use super::Emitter;
use super::calls::{changes, may_fail};
use super::forms::Family;
use super::graphs::WIRING;
use super::names::{self, identifier};
use super::nodes::{Context, Kind, Placed};
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

    /// `value`, an `eval(...)` (TST-5, TST-8; core's CEVAL-1): a run of
    /// the entry made for it, and the trace it recorded.
    fn eval(&mut self, scope: &mut Scope, value: &Expr) -> String {
        let ExprKind::Eval {
            call,
            config,
            real_time,
        } = &value.kind
        else {
            return self.missing(value.at, "an eval");
        };
        let program = self.program;
        let ExprKind::Call {
            callee, arguments, ..
        } = &call.kind
        else {
            return self.missing(call.at, "an eval of this");
        };
        let temporal = callee.declaration().is_some_and(|(module, declaration)| {
            program.modules[module].declarations[declaration].kind
                == hgl_analyser::DeclarationKind::Function(hgl_analyser::Role::Temporal)
        });
        if !temporal {
            // TST-5: "a selected value helper uses ordinary lifting".
            return self.missing(call.at, "an eval of a value function");
        }
        let Some(wiring) = run_wiring(program, call, &value.ty) else {
            return self.missing(call.at, "an eval of this");
        };
        let library = names::testing("");
        let library = library.trim_end_matches("::");
        // EXPR-2: the call's arguments in the order written, then the
        // configuration (RUN-1). Where more than one of them may fail, or
        // one runs statements, each is held first, in that order, before
        // the run that reads them; a trace's cells are read where the
        // entry is wired otherwise.
        let held = arguments
            .iter()
            .map(|argument| &argument.value)
            .chain(config.as_deref())
            .filter(|part| may_fail(part))
            .count()
            > 1
            || arguments.iter().any(|argument| changes(&argument.value))
            || config.as_deref().is_some_and(changes);
        let mut steps = String::new();
        let mut text = format!(
            "[&](::hgraph::Wiring& {WIRING}, [[maybe_unused]] ::hgraph::DateTime start_) {{\n"
        );
        let indent = INDENT.repeat(2);
        // CEVAL-1: "core's `replay` wired to each temporal input of the
        // target", each given the trace written for it (CEVAL-2).
        let mut given: Vec<(usize, String)> = Vec::new();
        let mut context = Context::new(Kind::Entry, scope.body.locals.len());
        for (position, argument) in arguments.iter().enumerate() {
            if !matches!(argument.value.kind, ExprKind::Trace(_)) {
                // A `const` parameter's configuration, held as a value; a
                // value given to a temporal one is wired as a constant
                // where the entry is (FN-3).
                let constant = argument
                    .parameter
                    .and_then(|index| callee_parameter(program, callee, index))
                    .is_some_and(|parameter| parameter.constant);
                if held && constant {
                    let mut written = String::new();
                    self.in_context(&mut context, |emitter| {
                        written = emitter.expression(scope, &argument.value);
                    });
                    let name = names::temporary("argument", self.temporary());
                    steps.push_str(&format!("{indent}const auto {name} = {written};\n"));
                    given.push((position, name));
                }
                continue;
            }
            let parameter = argument.parameter.and_then(|index| {
                callee_parameter(program, callee, index).map(|parameter| parameter.name.clone())
            });
            let Some((_, ty)) = wiring
                .inputs
                .iter()
                .find(|(name, _)| Some(name) == parameter.as_ref())
            else {
                return self.missing(argument.value.at, "a trace given to this");
            };
            let ty = ty.clone();
            let Some(replay) = self.core_instance("replay", &ty, argument.value.at) else {
                return "0".to_string();
            };
            let Some(schema) = self.temporal_schema(&ty, argument.value.at) else {
                return "0".to_string();
            };
            let mut trace = String::new();
            self.in_context(&mut context, |emitter| {
                trace = emitter.trace(scope, &argument.value, &ty);
            });
            if held {
                let name = names::temporary("trace", self.temporary());
                steps.push_str(&format!("{indent}const auto {name} = {trace};\n"));
                trace = name;
            }
            let port = names::temporary("input", self.temporary());
            text.push_str(&format!(
                "{indent}const ::hgraph::Port<{schema}> {port} = {replay}({WIRING}, {library}::replay_items({trace}, start_));\n"
            ));
            given.push((position, port));
        }
        let mut target = String::new();
        self.in_context(&mut context, |emitter| {
            target = emitter.temporal_call(scope, call, &given);
        });
        // RUN-1: the run's configuration, core's `RunConfig`, gives its
        // bounds (CRUN-2), and an integration test's `run` its mode.
        let mut bounds = match config {
            Some(config) => self.run_bounds(scope, config),
            None => "{}".to_string(),
        };
        if held && config.is_some() {
            let name = names::temporary("bounds", self.temporary());
            steps.push_str(&format!("{indent}const auto {name} = {bounds};\n"));
            bounds = name;
        }
        let configured = match (config.is_some(), *real_time) {
            (_, true) => format!(", {bounds}, {library}::Mode::real_time"),
            (true, false) => format!(", {bounds}"),
            (false, false) => String::new(),
        };
        // CEVAL-1: "and the target's result as the entry's", recorded by
        // core's `record` under the run's key (CRUN-8).
        let run = match &wiring.result {
            None => {
                text.push_str(&format!("{indent}{target};\n{INDENT}}}"));
                format!("{library}::run({text}{configured})")
            }
            Some(result) => {
                let Some(record) = self.core_instance("record", result, call.at) else {
                    return "0".to_string();
                };
                let Some(written) = self.written_type(result, call.at) else {
                    return "0".to_string();
                };
                text.push_str(&format!(
                    "{indent}{record}({WIRING}, {target}, ::hgraph::Str{{{library}::run_output}});\n{INDENT}}}"
                ));
                format!("{library}::evaluate<{written}>({text}{configured})")
            }
        };
        if steps.is_empty() {
            run
        } else {
            format!("[&]() {{\n{steps}{indent}return {run};\n{INDENT}}}()")
        }
    }

    /// The wiring function of the instance of core's temporal function
    /// `name`, `replay` or `record`, for `ty` (CEVAL-1), which the piece
    /// then reaches. `None` where there is none, which is reported at
    /// `at`.
    fn core_instance(
        &mut self,
        name: &str,
        ty: &Type,
        at: hgl_diagnostics::Location,
    ) -> Option<String> {
        let program = self.program;
        let Some((module, declaration)) = core_function(program, name) else {
            self.unsupported(at, &format!("an eval with no core `{name}`"));
            return None;
        };
        let instances = self.instances;
        let Some(instance) = instances.find(
            program,
            (module, declaration),
            &[GenericArgument::Type(ty.clone())],
        ) else {
            self.unsupported(at, &format!("core's `{name}` of `{ty}`"));
            return None;
        };
        self.piece
            .reach
            .uses
            .insert((module, declaration, Some(instance.suffix.clone())));
        Some(
            Placed::of(program, module, declaration, Some(&instance.suffix))
                .wiring_qualified(program, module),
        )
    }

    /// `value`, a trace written out (TST-5 to TST-7), of `ty`: dense, or
    /// timed, each cell as `ty`'s family writes it.
    fn trace(&mut self, scope: &mut Scope, value: &Expr, ty: &Type) -> String {
        let ExprKind::Trace(cells) = &value.kind else {
            return self.missing(value.at, "this trace");
        };
        let library = names::testing("");
        let library = library.trim_end_matches("::");
        let Some(written) = self.written_type(ty, value.at) else {
            return "0".to_string();
        };
        // TST-6: a dense trace; TST-7: a timed one, each cell with its key.
        if cells.iter().all(|cell| cell.time.is_none()) {
            let mut written_cells = Vec::new();
            for Cell { value: cell, .. } in cells {
                written_cells.push(match cell {
                    Some(cell) => self.trace_cell(scope, ty, cell),
                    None => "std::nullopt".to_string(),
                });
            }
            return format!(
                "{library}::Dense<{written}>{{{}}}",
                written_cells.join(", ")
            );
        }
        let mut entries = Vec::new();
        for cell in cells {
            let (Some(time), Some(published)) = (&cell.time, &cell.value) else {
                return self.missing(value.at, "this timed trace");
            };
            let key = self.expression(scope, time);
            let published = self.trace_cell(scope, ty, published);
            entries.push(format!("{{{library}::TraceKey{{{key}}}, {published}}}"));
        }
        format!("{library}::Timed<{written}>{{{{{}}}}}", entries.join(", "))
    }

    /// One side of a comparison of traces (TST-4): an eval, or a trace
    /// written out, of its own type.
    fn compared(&mut self, scope: &mut Scope, side: &Expr) -> String {
        match &side.kind {
            ExprKind::Eval { .. } => self.eval(scope, side),
            ExprKind::Trace(_) => match trace_payload(&side.ty) {
                Some(ty) => self.trace(scope, side, &ty),
                None => self.missing(side.at, "this trace"),
            },
            // A `let` that holds a trace, read as it is.
            ExprKind::Local(_) => self.expression(scope, side),
            _ => self.missing(side.at, "a comparison of this trace"),
        }
    }
}

/// The parameter `index` of the function `callee` calls.
fn callee_parameter<'p>(
    program: &'p hgl_analyser::Program,
    callee: &hgl_analyser::hir::Callee,
    index: usize,
) -> Option<&'p hgl_analyser::Parameter> {
    let (module, declaration) = callee.declaration()?;
    match program.modules[module].details.get(declaration)? {
        hgl_analyser::Detail::Callable(signature) => signature.parameters.get(index),
        _ => None,
    }
}

/// The tests' family: evals, traces, and their comparison.
pub(super) struct Tests;

impl Family for Tests {
    fn expression(
        &self,
        emitter: &mut Emitter<'_>,
        scope: &mut Scope<'_>,
        value: &Expr,
    ) -> Option<String> {
        match &value.kind {
            ExprKind::Eval { .. } => {
                scope.acted = true;
                Some(emitter.eval(scope, value))
            }
            // TST-4: "Harness comparison is intrinsic", the left trace
            // first (EXPR-2).
            ExprKind::TraceCompare {
                left,
                right,
                negated,
            } => {
                scope.acted = true;
                let left = emitter.compared(scope, left);
                let right = emitter.compared(scope, right);
                let held_left = names::temporary("left", emitter.temporary());
                let held_right = names::temporary("right", emitter.temporary());
                let not = if *negated { "!" } else { "" };
                Some(format!(
                    "[&]() -> bool {{ const auto {held_left} = {left}; const auto {held_right} = {right}; return {not}{}({held_left}, {held_right}); }}()",
                    names::testing("same")
                ))
            }
            ExprKind::Trace(_) => match trace_payload(&value.ty) {
                Some(ty) => Some(emitter.trace(scope, value, &ty)),
                None => Some(emitter.missing(value.at, "this trace")),
            },
            // TST-16: "A trace given to a `const fn` is an ordinary value
            // there ... a `list<tuple<datetime, delta<T>>>`, one tuple for
            // each publication, in time order".
            ExprKind::Publications(trace) => {
                scope.acted = true;
                let trace = emitter.compared(scope, trace);
                Some(format!("{}({trace})", names::testing("publications")))
            }
            _ => None,
        }
    }

    // TST-4: an assertion that two traces are the same says where they
    // first differ.
    fn statement(
        &self,
        emitter: &mut Emitter<'_>,
        scope: &mut Scope<'_>,
        statement: &Stmt,
        depth: usize,
        text: &mut String,
    ) -> bool {
        // TST-16: a `let` of a trace, an eval's or one written out, holds
        // it as the test host has it: a recording, or a trace of cells.
        if let Stmt::Declare {
            local,
            value: Some(value),
        } = statement
            && matches!(scope.body.locals[*local].ty, Type::Trace { .. })
        {
            let name = identifier(&scope.body.locals[*local].name);
            let written = emitter.expression(scope, value);
            text.push_str(&format!(
                "{}[[maybe_unused]] const auto {name} = {written};\n",
                INDENT.repeat(depth)
            ));
            return true;
        }
        let Stmt::Assert {
            condition:
                Expr {
                    kind:
                        ExprKind::TraceCompare {
                            left,
                            right,
                            negated,
                        },
                    ..
                },
            ..
        } = statement
        else {
            return false;
        };
        if !scope.testing {
            return false;
        }
        let indent = INDENT.repeat(depth);
        let inner = INDENT.repeat(depth + 1);
        let left = emitter.compared(scope, left);
        let right = emitter.compared(scope, right);
        let held_left = names::temporary("left", emitter.temporary());
        let held_right = names::temporary("right", emitter.temporary());
        let expect = if *negated {
            names::testing("expect_different")
        } else {
            names::testing("expect_same")
        };
        text.push_str(&format!(
            "{indent}[&] {{\n{inner}const auto {held_left} = {left};\n{inner}const auto {held_right} = {right};\n{inner}{expect}({held_left}, {held_right});\n{indent}}}();\n"
        ));
        true
    }
}
