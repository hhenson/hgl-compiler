//! The timing of a node and of a source: a node's scheduler and alarm
//! (core's CAP-4, CAP-5; the language's TIME-3, TIME-8, TIME-9), and a
//! source with declarations or hooks (SOURCE-1). Part of the transpiler,
//! the last stage. `nodes` and `sources` ask it what these add to what
//! they write; none is written yet, and each is reported.

use hgl_analyser::hir::{Body, Hook, Stmt};
use hgl_analyser::{LocalKind, Signature};
use hgl_diagnostics::Location;
use hgl_lower::nodes::{Handler, Timer};

use super::Emitter;
use super::nodes::Context;
use super::statements::statement_at;

/// The capability `inject name` binds where it is the scheduler or the
/// alarm: `None` while neither is written.
pub(super) fn capability(_name: &str) -> Option<(&'static str, &'static str)> {
    None
}

/// What the library's capability `name` is made from beside its view in
/// the hook `hook`: nothing for any written yet.
pub(super) fn made_in(_name: &str, _hook: Hook) -> String {
    String::new()
}

/// What a node with a scheduler or an alarm reads of what woke it, as its
/// `eval` is entered: nothing for a node with neither.
pub(super) fn woken(_context: &Context, _body: &Body, _inputs: &[usize]) -> String {
    String::new()
}

/// The guard of what woke the node that `handler` takes beside its own,
/// in a node woken by `timer` beside its inputs: none with no timer.
pub(super) fn wake_guard(_timer: Option<Timer>, _handler: &Handler) -> Option<String> {
    None
}

/// What a source's declarations and hooks add to its node (SOURCE-1),
/// each a part of what `sources` writes.
#[derive(Default)]
pub(super) struct Hooked {
    /// The statements its body runs, `yield` among them.
    pub(super) yielding: Vec<Stmt>,
    /// The parameters its body takes before its configuration.
    pub(super) taken: Vec<String>,
    /// What `start` gives those parameters.
    pub(super) given: Vec<String>,
    /// What its body runs before its statements.
    pub(super) prologue: String,
    /// What the node's struct declares before its body.
    pub(super) declared: String,
    /// What the node's state holds beside the body.
    pub(super) kept: String,
    /// The parameters of `start` beside the configuration and the state.
    pub(super) start_parameters: Vec<String>,
    /// What `start` runs before it makes the body.
    pub(super) start_before: String,
    /// What `start` runs after it makes the body.
    pub(super) start_after: String,
    /// The parameters of `eval` beside those every source's takes.
    pub(super) eval_parameters: Vec<String>,
    /// What `eval` runs before it resumes the body.
    pub(super) eval_before: String,
    /// The parameters of `stop` beside the state.
    pub(super) stop_parameters: Vec<String>,
    /// What `stop` runs after it destroys the body.
    pub(super) stop_after: String,
}

impl Emitter<'_> {
    /// Whether the node of `signature`, which a timer wakes, is written:
    /// one that is not is reported at `at`. Every one is, while none is
    /// written.
    pub(super) fn timed_node(&mut self, _signature: &Signature, _at: Location) -> bool {
        true
    }

    /// What the declarations and hooks of the source of `signature` and
    /// `body` add to its node: nothing for one that has none, which is
    /// all that is written yet. `None` where one is not written, which is
    /// reported.
    pub(super) fn source_hooks(&mut self, _signature: &Signature, body: &Body) -> Option<Hooked> {
        // SOURCE-1: "One with an `inject`, a `cache`, a `state`, a `start`,
        // or a `stop` has its declarations, then `start`, then one block
        // that holds every `yield`, then `stop`".
        if let Some(statement) = body.statements.iter().find(|statement| {
            matches!(statement, Stmt::Inject { .. } | Stmt::Hook { .. })
                || matches!(statement, Stmt::Declare { local, .. }
                    if matches!(body.locals[*local].kind, LocalKind::State | LocalKind::Cache))
        }) {
            self.unsupported(
                statement_at(statement, &body.locals),
                "a source with a declaration or a hook",
            );
            return None;
        }
        Some(Hooked {
            yielding: body.statements.clone(),
            ..Hooked::default()
        })
    }
}
