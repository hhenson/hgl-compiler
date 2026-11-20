//! Writes the run's store where a program sets it before its nodes run,
//! and an application (UNIT-14, RUN-1 to RUN-3): its run statement, as
//! the function its executable runs. Part of the transpiler, the last
//! stage; a family of the dispatch point (`forms`) for `inject
//! global_state` in a graph and in an application.
//!
//! | HGL | C++ |
//! | --- | --- |
//! | `inject global_state` in a graph (FN-9) | `const ::hglrt::GlobalState global_state{wiring_.global_state()};`: the store of the run whose graph is being built, which hgraph's `Wiring` holds and carries onto the graph it builds |
//! | `put(global_state, "limit", limit)`, `get`, `has` in a graph (FN-9) | the library's natives, as a node's hook calls them (`capabilities.h`), called as `compose` runs: once, in the order written, before any node of the graph starts |
//! | `application demo.app` (UNIT-14) | `::hglrt::application::Output application_()` in the namespace of its module's unit, and an entry unit, `applications/demo.app.cpp`, whose `main` is the library's host of it (`assemble`) |
//! | `inject global_state` and each `put` before the run statement (UNIT-14) | the first statements of the run's entry, before its call is wired: the store of the run's graph, before the graph is built |
//! | `eval(counted(upto: 3), config)`, `run(...)` (RUN-1, RUN-3) | `::hglrt::application::run<T>(entry, bounds_, mode)`: the entry wires the call and core's `record` of its result under `"run.output"` (CRUN-8), as an eval's does; `run` of an entry with no result. `eval` is `Mode::simulation`, `run` `Mode::real_time` |
//! | The configuration, core's `RunConfig` (RUN-1) | `const ::hglrt::testing::Bounds bounds_ = ::hglrt::application::bounds(config);`, the value as the value code writes it; with none written, `RunConfig()`'s, `Bounds{}` |
//!
//! The library's `run` gives the run's output as text, and its host
//! (`hglrt/application_host.h`) writes it for the driver (`hgl_cli`).

use hgl_analyser::hir::{Body, Stmt};
use hgl_lower::instances::run_result;

use super::Emitter;
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

        // The configuration, a value as any other (RUN-1), worked out
        // before the run, whose bounds it gives (CRUN-2).
        let none = Body {
            locals: Vec::new(),
            statements: Vec::new(),
        };
        let mut scope = Scope::cold(program, &none);
        let bounds = match &run.config {
            Some(config) => {
                let written = self.expression(&mut scope, config);
                format!("::{library}::application::bounds({written})")
            }
            None => format!("{testing}::Bounds{{}}"),
        };
        let mut text = String::new();
        for before in std::mem::take(&mut scope.before) {
            for line in before.lines() {
                text.push_str(&format!("{INDENT}{line}\n"));
            }
        }
        text.push_str(&format!(
            "{INDENT}const {testing}::Bounds bounds_ = {bounds};\n"
        ));

        // The entry: the store set first, then the call wired, and core's
        // `record` of its result under the run's key (CRUN-8).
        let mut entry = String::new();
        if let Some(store) = &declared.store_body {
            let mut setting = Scope::cold(program, store);
            let mut context = Context::new(Kind::Entry, store.locals.len());
            self.in_context(&mut context, |emitter| {
                emitter.statements(&mut setting, &store.statements, 2, &mut entry);
            });
        }
        let mut called = Scope::cold(program, &none);
        let mut context = Context::new(Kind::Entry, 0);
        let mut target = String::new();
        self.in_context(&mut context, |emitter| {
            target = emitter.temporal_call(&mut called, &run.call, &[]);
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
             {INDENT}return {ran}([&](::hgraph::Wiring& {WIRING}, [[maybe_unused]] ::hgraph::DateTime start_) {{\n\
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

/// The store's family: `inject global_state` where a graph is built and
/// where an application sets its run's store.
pub(super) struct Apps;

impl Family for Apps {
    // FN-9: "A graph injects `global_state` and no other, and uses it
    // where the graph is built: each call on it is evaluated once then,
    // in the order written, before any node of that graph starts". UNIT-14:
    // an application's are "evaluated once, in the order written, before
    // the run's graph is built". CRUN-7: "A graph that injects the store
    // puts values there as it is built". Each is the store of the wiring
    // the graph is built on, which hgraph carries onto the graph built.
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
        if name != STORE || !matches!(emitter.kind(), Some(Kind::Graph | Kind::Entry)) {
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
