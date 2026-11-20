//! Writes the run's store where a program sets it before its nodes run,
//! and an application (UNIT-14, RUN-1 to RUN-3): its run statement, as
//! the function its executable runs. Part of the transpiler, the last
//! stage; a family of the dispatch point (`forms`) for `inject
//! global_state` in a graph.
//!
//! | HGL | C++ |
//! | --- | --- |
//! | `inject global_state` in a graph (FN-9) | `const ::hglrt::GlobalState global_state{wiring_.global_state()};`: the store of the run whose graph is being built, which hgraph's `Wiring` holds and carries onto the graph it builds |
//! | `put(global_state, "limit", limit)`, `get`, `has` in a graph (FN-9) | the library's natives, as a node's hook calls them (`capabilities.h`), called as `compose` runs: once, in the order written, before any node of the graph starts |
//! | `application demo.app` (UNIT-14) | `::hglrt::application::Output application_()` in the namespace of its module's unit, and an entry unit, `applications/demo.app.cpp`, whose `main` is the library's host of it (`assemble`) |
//! | `inject global_state` and each `put` before the run statement (UNIT-14) | the function's first statements, on a store of its own, `::hglrt::application::Store store_;`: `const ::hglrt::GlobalState global_state{store_.view()};` and the natives' calls, which the run gives its graph before the entry is wired |
//! | `eval(counted(upto: 3), config)`, `run(...)` (RUN-1, RUN-3) | `::hglrt::application::run<T>(store_, entry, bounds_, mode)`: the entry wires the call and core's `record` of its result under `"run.output"` (CRUN-8), as an eval's does; `run` of an entry with no result. `eval` is `Mode::simulation`, `run` `Mode::real_time` |
//! | The call's arguments and the configuration, core's `RunConfig` (RUN-1) | `const ::hglrt::testing::Bounds bounds_ = ...;`, as an eval's configuration is written (`lifecycle`), after the store; with none written, `RunConfig()`'s, `Bounds{}`. Where more than one of the arguments and the configuration may fail, or one runs statements, each argument is held before it, in the order written (EXPR-2), as an eval's are |
//!
//! The library's `run` gives the run's output as text, and its host
//! (`hglrt/application_host.h`) writes it for the driver (`hgl_cli`).

use hgl_analyser::hir::{Body, ExprKind, Stmt};
use hgl_lower::instances::run_result;

use super::Emitter;
use super::calls::{changes, may_fail};
use super::forms::Family;
use super::graphs::WIRING;
use super::names::{self, identifier};
use super::nodes::{Context, Kind};
use super::statements::Scope;
use super::values::INDENT;
use crate::runtime;

/// The name of the function an application's run statement is written
/// as, in its module's namespace. No HGL name gives it
/// (`names::identifier`).
pub(crate) const APPLICATION: &str = "application_";

/// The name of the capability `inject global_state` binds (TIME-8).
const STORE: &str = "global_state";

/// The name of the store an application's function sets (UNIT-14), of a
/// form no HGL name gives.
const SET: &str = "store_";

impl Emitter<'_> {
    /// Writes the module's application (UNIT-14): what sets its run's
    /// store and its run statement, as the function [`APPLICATION`].
    ///
    /// UNIT-14: "Between its declarations and its run statement it may
    /// set the run's store: `inject global_state`, and then calls of
    /// `put` on it. Each is evaluated once, in the order written, before
    /// the run's graph is built". RUN-1: "its run statement is `run(call,
    /// config)` or `eval(call, config)`, where the call is of an entry
    /// ... and the configuration is a value of core's `RunConfig`, or
    /// `RunConfig()` where none is written. The entry's result, if any,
    /// is the run's output". RUN-3: "The word of a run statement is its
    /// mode."
    pub(super) fn application(&mut self) {
        let program = self.program;
        let declared = &program.modules[self.module];
        let Some(run) = &declared.run_body else {
            // The analyser gave the statement no reading: the program was
            // rejected, and nothing is written of it.
            if let Some(at) = declared
                .run
                .as_ref()
                .map(|run| run.at)
                .or_else(|| declared.application_at())
            {
                self.unsupported(at, "this application's run statement");
            }
            return;
        };
        let library = runtime::NAMESPACE;
        let testing = names::testing("");
        let testing = testing.trim_end_matches("::");
        let inner = INDENT.repeat(2);

        // UNIT-14: the store, set first, in the order written, by value
        // code "read as a constant's is ... with no parameter in scope".
        let mut text = format!("{INDENT}::{library}::application::Store {SET};\n");
        if let Some(store) = &declared.store_body {
            let mut setting = Scope::cold(program, store);
            for statement in &store.statements {
                match statement {
                    Stmt::Inject { local, .. } => text.push_str(&format!(
                        "{INDENT}[[maybe_unused]] const ::{library}::GlobalState {}{{{SET}.view()}};\n",
                        identifier(&store.locals[*local].name)
                    )),
                    _ => self.statement(&mut setting, statement, 1, &mut text),
                }
            }
        }

        // EXPR-2: the call's arguments in the order written, then the
        // configuration, as an eval's (`tests`): where more than one of
        // them may fail, or one runs statements, each argument is held
        // first, in that order, before the configuration.
        let none = Body {
            locals: Vec::new(),
            statements: Vec::new(),
        };
        let mut scope = Scope::cold(program, &none);
        let arguments = match &run.call.kind {
            ExprKind::Call { arguments, .. } => arguments.as_slice(),
            _ => &[],
        };
        let config = run.config.as_ref();
        let held = arguments
            .iter()
            .map(|argument| &argument.value)
            .chain(config)
            .filter(|part| may_fail(part))
            .count()
            > 1
            || arguments.iter().any(|argument| changes(&argument.value))
            || config.is_some_and(changes);
        // Each held, after what it needs run before it.
        let hold = |text: &mut String, scope: &mut Scope, line: String| {
            for before in std::mem::take(&mut scope.before) {
                for line in before.lines() {
                    text.push_str(&format!("{INDENT}{line}\n"));
                }
            }
            text.push_str(&format!("{INDENT}{line}\n"));
        };
        let mut given: Vec<(usize, String)> = Vec::new();
        if held {
            for (position, argument) in arguments.iter().enumerate() {
                let written = self.expression(&mut scope, &argument.value);
                let name = names::temporary("argument", self.temporary());
                hold(
                    &mut text,
                    &mut scope,
                    format!("const auto {name} = {written};"),
                );
                given.push((position, name));
            }
        }
        // RUN-1, CRUN-2: the configuration's bounds, as an eval's are
        // (`lifecycle`).
        let bounds = match config {
            Some(config) => self.run_bounds(&mut scope, config),
            None => format!("{testing}::Bounds{{}}"),
        };
        hold(
            &mut text,
            &mut scope,
            format!("const {testing}::Bounds bounds_ = {bounds};"),
        );

        // The entry: the call wired, and core's `record` of its result
        // under the run's key (CRUN-8).
        let mut entry = String::new();
        let mut called = Scope::cold(program, &none);
        let mut context = Context::new(Kind::Entry, 0);
        let mut target = String::new();
        self.in_context(&mut context, |emitter| {
            target = emitter.temporal_call(&mut called, &run.call, &given);
        });
        for before in std::mem::take(&mut called.before) {
            for line in before.lines() {
                entry.push_str(&format!("{inner}{line}\n"));
            }
        }
        let mode = if run.real_time {
            "real_time"
        } else {
            "simulation"
        };
        let ran = match run_result(program, run) {
            Some(result) => {
                let Some(record) = self.core_instance("record", &result, run.call.at) else {
                    return;
                };
                let Some(written) = self.written_type(&result, run.call.at) else {
                    return;
                };
                entry.push_str(&format!(
                    "{inner}{record}({WIRING}, {target}, ::hgraph::Str{{{testing}::run_output}});\n"
                ));
                format!("::{library}::application::run<{written}>")
            }
            None => {
                entry.push_str(&format!("{inner}{target};\n"));
                format!("::{library}::application::run")
            }
        };
        let line = run_line(declared).unwrap_or_else(|| self.line_of(run.at));
        let word = if run.real_time { "run" } else { "eval" };
        self.piece.text = format!(
            "// The run statement of application {}, line {line}: `{word}` (UNIT-14, RUN-1, RUN-3).\n\
             ::{library}::application::Output {APPLICATION}()\n{{\n{text}\
             {INDENT}return {ran}({SET}, [&](::hgraph::Wiring& {WIRING}, [[maybe_unused]] ::hgraph::DateTime start_) {{\n\
             {entry}{INDENT}}}, bounds_, {testing}::Mode::{mode});\n}}\n",
            declared.path
        );
    }
}

/// The line, counted from 1, the run statement of the application
/// `declared` starts on, from the text of its file, which may hold no
/// declaration to count it from.
fn run_line(declared: &hgl_analyser::Module) -> Option<usize> {
    let run = declared.run.as_ref()?;
    let root = run.syntax.ancestors().last()?;
    let text = root.text().to_string();
    let before = (run.at.span.start as usize).min(text.len());
    Some(
        text.as_bytes()[..before]
            .iter()
            .filter(|byte| **byte == b'\n')
            .count()
            + 1,
    )
}

/// The store's family: `inject global_state` where a graph is built.
pub(super) struct Apps;

impl Family for Apps {
    // FN-9: "A graph injects `global_state` and no other, and uses it
    // where the graph is built: each call on it is evaluated once then,
    // in the order written, before any node of that graph starts". CRUN-7:
    // "A graph that injects the store puts values there as it is built".
    // It is the store of the wiring the graph is built on, which hgraph
    // carries onto the graph built; each call on it is the library's
    // native, called as `compose` runs.
    fn statement(
        &self,
        emitter: &mut Emitter<'_>,
        scope: &mut Scope<'_>,
        statement: &Stmt,
        depth: usize,
        text: &mut String,
    ) -> bool {
        let Stmt::Inject { local, .. } = statement else {
            return false;
        };
        let name = &scope.body.locals[*local].name;
        if name != STORE || emitter.kind() != Some(Kind::Graph) {
            return false;
        }
        text.push_str(&format!(
            "{}[[maybe_unused]] const ::{}::GlobalState {}{{{WIRING}.global_state()}};\n",
            INDENT.repeat(depth),
            runtime::NAMESPACE,
            identifier(name)
        ));
        true
    }
}
