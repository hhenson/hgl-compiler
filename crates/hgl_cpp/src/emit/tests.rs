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
//! | `eval target(value: [1, _, 3])` (TST-5, CEVAL-1) | `::hglrt::testing::evaluate<T>(entry)`: the entry wires core's `replay` of each trace written to the input it is given, the target, and core's `record` of its result under `"run.output"`, and, for a result whose delta is no T, a node `held_00_` that keeps the result read whole at each publication (`::hglrt::testing::hold`); `run target(...)` in an integration test runs in real time (TST-17); an eval of a target with no result is `::hglrt::testing::run(entry)` (TST-8) |
//! | `[7, _, 9]`, `[0s: 7, 2s: 9]` (TST-6, TST-7) | `::hglrt::testing::Dense<T>{7, std::nullopt, 9}`, `::hglrt::testing::Timed<T>{...}`, each cell as its type's family writes it |
//! | `assert left == right` of two traces (TST-4) | `::hglrt::testing::expect_same(left, right)`, the left one evaluated first; `!=` is `expect_different`; either as a value is `::hglrt::testing::same` |
//! | A trace given to a `const fn` (TST-16) | `::hglrt::testing::publications(trace)`, its publications as a list |
//! | `let seen = eval target(...)`, a `let` of a trace | `const auto seen = ...;`: the recording, or the trace written out, as the test host has it |
//!
//! An eval's configuration, and an integration test's `inject
//! run_config`, are the lifecycle's (`lifecycle`). An eval of a
//! value function is its lifted node's (`lifting`), given the replays as
//! its inputs and each default the call leaves out as configuration, and
//! written ahead of the test.

use hgl_analyser::hir::{Cell, Expr, ExprKind, Stmt};
use hgl_analyser::{Argument as GenericArgument, Type};
use hgl_lower::calls::{LeftOut, left_out, runtime_arguments};
use hgl_lower::instances::{core_function, run_wiring, trace_payload};

use super::Emitter;
use super::calls::{changes, may_fail};
use super::forms::Family;
use super::graphs::WIRING;
use super::lifecycle::config_held;
use super::lifting::Given;
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
        // What the test's evals wire that a function cannot hold, ahead
        // of it: its statements' lines move down by as many lines.
        let mut ahead: String = std::mem::take(&mut self.ahead)
            .into_iter()
            .map(|node| node + "\n")
            .collect();
        // A node written ahead sets the HGL lines of its statements; the
        // test's own are the unit's again after it.
        let nodes = ahead.clone();
        super::restore_after(&mut ahead, &nodes);
        let moved = ahead.matches('\n').count();
        for (line, _) in &mut self.piece.statements {
            *line += moved;
        }
        self.piece.text = ahead + &text;
    }

    /// `value`, an `eval target(...)` (TST-5, TST-8; core's CEVAL-1): a
    /// run of the entry made for it, and the trace it recorded.
    fn eval(&mut self, scope: &mut Scope, value: &Expr) -> String {
        let ExprKind::Eval {
            call,
            config,
            inherited,
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
        let Some(wiring) = run_wiring(program, call, &value.ty) else {
            return self.missing(call.at, "an eval of this");
        };
        let library = names::testing("");
        let library = library.trim_end_matches("::");
        // EXPR-13: the configuration, written before the call (RUN-6),
        // and then the call's arguments in the order written (EXPR-2).
        // Where more than one of them may fail, or one runs statements,
        // each is held first, in that order, before the run that reads
        // them; a trace's cells are read where the entry is wired
        // otherwise.
        // The defaults of the `const` parameters the call leaves out, which
        // are evaluated after the arguments written (EXPR-2); of a value
        // function, which is lifted, of every parameter it leaves out,
        // which binds as configuration (FN-6).
        let left: Vec<&Expr> = callee
            .declaration()
            .and_then(|(module, declaration)| {
                match program.modules[module].details.get(declaration) {
                    Some(hgl_analyser::Detail::Callable(signature)) => Some(
                        signature
                            .parameters
                            .iter()
                            .enumerate()
                            .filter(|(index, parameter)| {
                                (parameter.constant || !temporal)
                                    && !arguments
                                        .iter()
                                        .any(|argument| argument.parameter == Some(*index))
                            })
                            .filter_map(|(index, parameter)| {
                                match left_out(program, (module, declaration), index, parameter) {
                                    Some(LeftOut::Default(default)) => Some(default),
                                    _ => None,
                                }
                            })
                            .collect(),
                    ),
                    _ => None,
                }
            })
            .unwrap_or_default();
        let (config_failing, config_changes) = config_held(config);
        let held = arguments
            .iter()
            .map(|argument| &argument.value)
            .chain(left.iter().copied())
            .filter(|part| may_fail(part))
            .count()
            + config_failing
            > 1
            || arguments.iter().any(|argument| changes(&argument.value))
            || config_changes;
        let mut steps = String::new();
        let mut text = format!(
            "[&](::hgraph::Wiring& {WIRING}, [[maybe_unused]] ::hgraph::DateTime start_) {{\n"
        );
        let indent = INDENT.repeat(2);
        // RUN-6: the run's configuration gives its bounds (CRUN-2): the
        // eval's entries, and in an integration test that injects
        // `run_config` the injected configuration's for the fields they
        // do not name. An integration test's `run` gives its mode.
        let inherited = inherited.map(|local| identifier(&scope.body.locals[local].name));
        let configured_run = !config.is_empty() || inherited.is_some();
        let mut bounds = if configured_run {
            self.run_bounds(scope, config, inherited.as_deref())
        } else {
            "{}".to_string()
        };
        if held && configured_run {
            let name = names::temporary("bounds", self.temporary());
            steps.push_str(&format!("{indent}const auto {name} = {bounds};\n"));
            bounds = name;
        }
        // CEVAL-1: "core's `replay` wired to each temporal input of the
        // target", each given the trace written for it (CEVAL-2).
        let mut given: Vec<(usize, String)> = Vec::new();
        // What a lifted target is given (`lifting`): the replays, and the
        // configuration held.
        let mut replayed: Vec<Given> = Vec::new();
        let mut context = Context::new(Kind::Entry, scope.body.locals.len());
        for (position, argument) in runtime_arguments(arguments) {
            // A trace that is no trace written out, a `let` of one or an
            // eval's, is replayed from what it holds: not written yet.
            if matches!(argument.value.ty, Type::Trace { .. })
                && !matches!(argument.value.kind, ExprKind::Trace(_))
            {
                return self.missing(
                    argument.value.at,
                    "a trace given to an eval that is no trace written out",
                );
            }
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
                    // Held as its HGL type, so that a later part does not
                    // change it through a proxy (`graphs::temporal_call`).
                    let Some(ty) = self.written_type(&argument.value.ty, argument.value.at) else {
                        return "0".to_string();
                    };
                    steps.push_str(&format!("{indent}const {ty} {name} = {written};\n"));
                    // A lifted target is given it as configuration, so that
                    // it is evaluated once (EXPR-2).
                    replayed.push(Given {
                        position,
                        written: name.clone(),
                        input: None,
                    });
                    given.push((position, name));
                }
                continue;
            }
            let Some((_, ty)) = wiring.inputs.iter().find(|(given, _)| *given == position) else {
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
            given.push((position, port.clone()));
            replayed.push(Given {
                position,
                written: port,
                input: Some(ty),
            });
        }
        // EXPR-2: "the arguments written, then the defaults of those left
        // out": where the parts are held, each `const` parameter's default
        // is held too, in the order of the parameters.
        let mut defaults: Vec<(usize, String)> = Vec::new();
        if held && let Some((module, declaration)) = callee.declaration() {
            let parameters = match program.modules[module].details.get(declaration) {
                Some(hgl_analyser::Detail::Callable(signature)) => signature.parameters.clone(),
                _ => Vec::new(),
            };
            for (index, parameter) in parameters.iter().enumerate() {
                let written = arguments
                    .iter()
                    .any(|argument| argument.parameter == Some(index));
                if written || !(parameter.constant || !temporal) {
                    continue;
                }
                let Some(LeftOut::Default(default)) =
                    left_out(program, (module, declaration), index, parameter)
                else {
                    continue;
                };
                // A value function's default that cannot fail is the same
                // value wherever it is evaluated: its lifted node binds it
                // where it is wired (`lifting`).
                if !temporal && !may_fail(default) {
                    continue;
                }
                if changes(default) {
                    return self.missing(default.at, "a default that runs statements");
                }
                let cold = hgl_analyser::hir::Body {
                    locals: Vec::new(),
                    statements: Vec::new(),
                };
                let mut cold_scope = Scope::cold(program, &cold);
                let written = self.expression(&mut cold_scope, default);
                if !cold_scope.before.is_empty() {
                    return self.missing(default.at, "a default that propagates a failure");
                }
                let name = names::temporary("argument", self.temporary());
                let Some(ty) = self.written_type(&default.ty, default.at) else {
                    return "0".to_string();
                };
                steps.push_str(&format!("{indent}const {ty} {name} = {written};\n"));
                defaults.push((index, name));
            }
        }
        let mut target = String::new();
        self.in_context(&mut context, |emitter| {
            target = if temporal {
                emitter.temporal_call_held(scope, call, &given, &defaults)
            } else {
                // TST-8: "A selected value helper uses ordinary lifting":
                // a lifted node, given the replays as its inputs, which is
                // written ahead of the test.
                emitter.lifted(scope, call, &replayed, &defaults, wiring.result.as_ref())
            };
        });
        // A node written already, where the eval is written again, is
        // not written twice.
        for node in context.lifted {
            if !self.ahead.contains(&node) {
                self.ahead.push(node);
            }
        }
        let configured = match (configured_run, *real_time) {
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
                let Some(written) = self.payload_type(result, call.at) else {
                    return "0".to_string();
                };
                // TST-5: "one written as a T [is compared] with the value
                // the result holds once its cycle has published". Where T's
                // delta is no T, a node of the entry's own keeps that value
                // at each publication (`hglrt::testing::hold`): the result
                // read whole as its form reads it, or none where that read
                // fails. A result whose form no family writes is reported
                // where the target is written. A window's trace holds its
                // arrivals and its read is a list (`Family::read_type`): it
                // has no cell written whole.
                if Type::delta(result.clone()) != *result
                    && self.read_type(result) == *result
                    && let Some(schema) = self.temporal_schema_of(result)
                {
                    let read = self.temporal_read(result, "result_", call.at);
                    let holder = names::temporary("held", self.temporary());
                    let port = names::temporary("result", self.temporary());
                    let inner = INDENT.repeat(3);
                    let body = INDENT.repeat(4);
                    text.push_str(&format!(
                        "{indent}struct {holder}\n{indent}{{\n\
                         {inner}static void eval(::hgraph::In<\"result\", {schema}> result_, ::hgraph::DateTime now_)\n{inner}{{\n\
                         {body}if (!::{runtime}::fn_modified(result_))\n{body}{{\n{body}{INDENT}return;\n{body}}}\n\
                         {body}::std::optional<{written}> whole_;\n\
                         {body}try\n{body}{{\n{body}{INDENT}whole_.emplace({read});\n{body}}}\n\
                         {body}catch (const ::{runtime}::Failure&)\n{body}{{\n{body}}}\n\
                         {body}{library}::hold<{written}>(now_, ::std::move(whole_));\n\
                         {inner}}}\n{indent}}};\n\
                         {indent}const auto {port} = {target};\n\
                         {indent}::hgraph::wire<{holder}>({WIRING}, {port});\n",
                        runtime = crate::runtime::NAMESPACE,
                    ));
                    target = port;
                }
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
    pub(super) fn core_instance(
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
        let Some(written) = self.payload_type(ty, value.at) else {
            return "0".to_string();
        };
        // TST-6: a dense trace; TST-7: a timed one, each cell with its key.
        if cells.iter().all(|cell| cell.time.is_none()) {
            let mut written_cells = Vec::new();
            for Cell { value: cell, .. } in cells {
                written_cells.push(match cell {
                    Some(cell) => self.trace_cell(scope, ty, cell),
                    None => "::std::nullopt".to_string(),
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
            && let Some(declared) = scope.body.locals.get(*local)
            && matches!(declared.ty, Type::Trace { .. })
        {
            let name = identifier(&declared.name);
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
