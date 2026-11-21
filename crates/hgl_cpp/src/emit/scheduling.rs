//! Writes the timing of a node and of a source: what a scheduler and an
//! alarm add to a node (core's CAP-4, CAP-5; the language's TIME-3,
//! TIME-8, TIME-9), and what the declarations and hooks of a source add
//! to its node (SOURCE-1 to SOURCE-4). Part of the transpiler, the last
//! stage; a family of the dispatch point (`forms`) for `scheduled()` and
//! `modified([])` read as values and for a capability read in a source's
//! body. `nodes` and `sources` ask it for the rest.
//!
//! | HGL | C++ |
//! | --- | --- |
//! | `inject scheduler`, `inject alarm` (TIME-8) | the hook's `::hgraph::NodeScheduler` or `::hgraph::SingleShotScheduler`, and the library's `::hglrt::Scheduler` or `::hglrt::Alarm` made from it at the top of the hook with the hook it is made in, `::hglrt::Hook::evaluation`; in `eval` always, whether or not the handlers name it |
//! | What woke the node (TIME-9) | read once, as `eval` is entered: `timer_woke_`, the scheduler's `is_scheduled_now()`, or, for an alarm, that no input ticked (`::hglrt::ticked`); `input_woke_`, that an input ticked, and is active where the node passivates one |
//! | A handler a wake by the timer runs: `scheduled()` in its header, or no input it subscribes to (TIME-3) | guarded by `timer_woke_` |
//! | Another handler with no `modified` selector, in a node with a timer | guarded by `input_woke_`: "Such a wake alone runs no other handler" |
//! | `scheduled()` read as a `bool` (TIME-9) | `timer_woke_` |
//! | `modified([])` read as a `bool` (TIME-3) | `true` in the body of the handler whose header selects it, and elsewhere that no input published |
//! | `schedule`, `un_schedule`, `reset` (CAP-4, CAP-5) | the library's natives, called as the value code calls any |
//! | A source's `state`, `cache` and `inject` (SOURCE-1) | members of a `slot_type_`, which the node's state keeps on the heap and the body is given by reference: each `state` and `cache`, and each capability the body reads, lent to it for one resumption, `::hglrt::Lent<::hglrt::Clock>` |
//! | A source's `start` | the slot made, its `state` and `cache` given their first values, the body made, then the hook's statements |
//! | A source's `eval` | each capability the body reads lent to it while it runs, `::hglrt::Lending` |
//! | A source's `stop` (SOURCE-1: it "always runs when the node ends, whatever state the generator is in") | the body destroyed, then the hook's statements |
//!
//! hgraph evaluates a node with a scheduler at a wake that was cancelled:
//! no wake is due in that evaluation and, but where an input ticks then
//! too, none ticked, so it runs no handler. Its alarm, which drops a wake
//! still to come where an input's tick evaluates the node first, is best
//! effort as CAP-5 has it (`capabilities.h`).

use hgl_analyser::hir::{Body, EndpointOperation, Expr, ExprKind, Hook, Stmt};
use hgl_analyser::{LocalKind, Signature};
use hgl_lower::nodes::{Handler, Timer};

use super::Emitter;
use super::forms::Family;
use super::names::{self, identifier};
use super::nodes::{Context, Kind, Role, uses};
use super::statements::Scope;
use super::values::INDENT;
use crate::runtime;

/// The name of what says that the node's scheduler or alarm woke it in
/// the evaluation, read as `eval` is entered (TIME-9).
const TIMER_WOKE: &str = "timer_woke_";

/// The name of what says that one of the node's inputs woke it in the
/// evaluation.
const INPUT_WOKE: &str = "input_woke_";

/// The name of what a source's body reaches beside its configuration:
/// the slot, a parameter of the body and a member of the node's state.
const SLOT: &str = "slot_";

/// The timing family: `scheduled()`, `modified([])` read as a value, and
/// a capability read in a source's body.
pub(super) struct Scheduling;

impl Family for Scheduling {
    fn expression(
        &self,
        emitter: &mut Emitter<'_>,
        scope: &mut Scope<'_>,
        value: &Expr,
    ) -> Option<String> {
        match &value.kind {
            // TIME-9: "In a node that injects `scheduler` it is the
            // scheduler's `is_scheduled_now` ... In a node that injects
            // `alarm` it holds where the node is evaluated and no temporal
            // input has ticked": what woke the node, read as it is entered.
            ExprKind::Endpoint {
                operation: EndpointOperation::Scheduled,
                ..
            } => {
                let context = emitter.temporal.as_ref()?;
                (context.kind == Kind::Hook(Hook::When) && context.timer.is_some())
                    .then(|| TIMER_WOKE.to_string())
            }
            // TIME-3: "In that handler's body `modified([])`, as a bool, is
            // true ... Elsewhere ... it is true in a cycle where no temporal
            // input was modified".
            ExprKind::Endpoint {
                operation: EndpointOperation::Modified,
                inputs: Some(inputs),
            } if inputs.is_empty() => {
                let context = emitter.temporal.as_ref()?;
                if context.kind != Kind::Hook(Hook::When) {
                    return None;
                }
                if context.selected_empty {
                    return Some("true".to_string());
                }
                let modified: Vec<String> = (0..scope.body.locals.len())
                    .filter(|&local| emitter.role(local) == Role::Input)
                    .map(|local| {
                        format!(
                            "::{}::fn_modified({})",
                            runtime::NAMESPACE,
                            identifier(&scope.body.locals[local].name)
                        )
                    })
                    .collect();
                Some(match modified.as_slice() {
                    [] => "true".to_string(),
                    _ => format!("!({})", modified.join(" || ")),
                })
            }
            // SOURCE-1, TIME-8: a capability a source's body injects, lent
            // to it for the resumption it is read in.
            ExprKind::Local(local)
                if emitter.kind() == Some(Kind::Source)
                    && scope.body.locals[*local].kind == LocalKind::Inject =>
            {
                scope.read = true;
                Some(format!(
                    "{SLOT}.{}.get()",
                    identifier(&scope.body.locals[*local].name)
                ))
            }
            _ => None,
        }
    }
}

/// The capability `inject name` binds where it is the scheduler or the
/// alarm (TIME-8), as the hook's hgraph view of it and the library's
/// type made from that (`capabilities.h`). `None` for any other name.
pub(super) fn capability(name: &str) -> Option<(&'static str, &'static str)> {
    Some(match Timer::of(name)? {
        Timer::Scheduler => ("::hgraph::NodeScheduler", "::hglrt::Scheduler"),
        Timer::Alarm => ("::hgraph::SingleShotScheduler", "::hglrt::Alarm"),
    })
}

/// What the library's capability `name` is made from beside its view in
/// the hook `hook`: for the scheduler and the alarm, the hook, since what
/// may be asked of them depends on it (TIME-8: "A wake asked for with a
/// `duration` takes one more than zero, but in `start`"); nothing for any
/// other.
pub(super) fn made_in(name: &str, hook: Hook) -> String {
    if Timer::of(name).is_none() {
        return String::new();
    }
    let hook = match hook {
        Hook::Start => "start",
        Hook::When => "evaluation",
        Hook::Stop => "stop",
    };
    format!(", ::{}::Hook::{hook}", runtime::NAMESPACE)
}

/// What a node with a scheduler or an alarm, as `context` says, of
/// `body` and the temporal inputs `inputs`, reads of what woke it as its
/// `eval` is entered, after its capabilities are made: nothing for a node
/// with neither. It is read once, as the Python-writing target reads it:
/// a handler before another may ask for a wake under the tag that is due
/// or passivate the input that ticked, and neither changes what woke the
/// evaluation.
pub(super) fn woken(context: &Context, body: &Body, inputs: &[usize]) -> String {
    let Some(timer) = context.timer else {
        return String::new();
    };
    let library = runtime::NAMESPACE;
    let views: Vec<String> = inputs
        .iter()
        .map(|&local| identifier(&body.locals[local].name))
        .collect();
    // TIME-3: "Passivating an input removes it as a wake source": where
    // the node passivates one, an input woke it where it ticked and is
    // active.
    let mut passivates = false;
    hgl_analyser::hir::walk_statements(&body.statements, &mut |part| {
        passivates |= matches!(
            part.kind,
            ExprKind::Endpoint {
                operation: EndpointOperation::Passivate,
                ..
            }
        );
    });
    let ticked: Vec<String> = views
        .iter()
        .map(|view| format!("::{library}::ticked({view})"))
        .collect();
    let woke: Vec<String> = if passivates {
        views
            .iter()
            .map(|view| {
                format!("(::{library}::ticked({view}) && ::{library}::fn_is_active({view}))")
            })
            .collect()
    } else {
        ticked.clone()
    };
    let input_woke = match woke.as_slice() {
        [] => "false".to_string(),
        _ => woke.join(" || "),
    };
    let timer_woke = match timer {
        // CAP-4: "`is_scheduled_now` is whether a wake asked of the
        // scheduler is due in the current evaluation, whether or not an
        // input has ticked in it". The library's scheduler, made above,
        // keeps that a wake was due.
        Timer::Scheduler => format!("{}.is_scheduled_now()", identifier("scheduler")),
        // TIME-9: "In a node that injects `alarm` it holds where the node
        // is evaluated and no temporal input has ticked."
        Timer::Alarm => match ticked.as_slice() {
            [] => "true".to_string(),
            _ => format!("!({})", ticked.join(" || ")),
        },
    };
    let indent = INDENT.repeat(2);
    format!(
        "{indent}// TIME-9: what woke the node, read as it is entered.\n\
         {indent}[[maybe_unused]] const bool {TIMER_WOKE} = {timer_woke};\n\
         {indent}[[maybe_unused]] const bool {INPUT_WOKE} = {input_woke};\n"
    )
}

/// The guard of what woke the node that `handler` takes beside its own,
/// in a node woken by `timer` beside its inputs (TIME-3): what a wake by
/// the timer runs, "the handlers that subscribe to no input and those
/// whose header has `scheduled()` ... alone or as a top-level conjunct",
/// is guarded by the timer's wake; "Such a wake alone runs no other
/// handler: one with no `modified` selector subscribes to every temporal
/// input, and is run by an input's wake", so that one is guarded by an
/// input's. One with a `modified` selector is guarded by its selector.
/// None in a node with no timer.
pub(super) fn wake_guard(timer: Option<Timer>, handler: &Handler) -> Option<String> {
    timer?;
    if handler.scheduled || handler.subscribed.is_empty() {
        Some(TIMER_WOKE.to_string())
    } else if !handler.selects {
        Some(INPUT_WOKE.to_string())
    } else {
        None
    }
}

/// What a source's declarations and hooks add to its node (SOURCE-1),
/// each a part of what `sources` writes. A source with neither adds
/// nothing but its statements.
#[derive(Default)]
pub(super) struct Hooked {
    /// The statements its body runs, `yield` among them: every one of a
    /// source with no declaration and no hook, and the one block of a
    /// source with them.
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
    /// What the declarations and hooks of the source of `signature` and
    /// `body` add to its node (SOURCE-1): "One with an `inject`, a
    /// `cache`, a `state`, a `start`, or a `stop` has its declarations,
    /// then `start`, then one block that holds every `yield`, then
    /// `stop`". `None` where one is not written yet, which is reported.
    pub(super) fn source_hooks(&mut self, signature: &Signature, body: &Body) -> Option<Hooked> {
        let mut states: Vec<(usize, Option<&Expr>)> = Vec::new();
        let mut injected: Vec<usize> = Vec::new();
        let mut start: Option<&[Stmt]> = None;
        let mut stop: Option<&[Stmt]> = None;
        let mut yielding: Vec<Stmt> = Vec::new();
        for statement in &body.statements {
            match statement {
                Stmt::Declare { local, value }
                    if matches!(
                        body.locals[*local].kind,
                        LocalKind::State | LocalKind::Cache
                    ) =>
                {
                    states.push((*local, value.as_ref()));
                }
                // SOURCE-1: a source "injects `clock`, `logger`, and
                // `global_state`, and not `out`, `scheduler`, or `alarm`",
                // which the analyser rejects.
                Stmt::Inject { local, at } => {
                    let name = &body.locals[*local].name;
                    if super::endpoints::capability(name).is_none() {
                        self.unsupported(*at, &format!("an injected `{name}` in a source"));
                        return None;
                    }
                    injected.push(*local);
                }
                Stmt::Hook {
                    hook: Hook::Start,
                    body: statements,
                    ..
                } => start = Some(statements),
                Stmt::Hook {
                    hook: Hook::Stop,
                    body: statements,
                    ..
                } => stop = Some(statements),
                // SOURCE-1: "A source has no `when`", which the analyser
                // rejects.
                Stmt::Hook { at, .. } => {
                    self.unsupported(*at, "a `when` in a source");
                    return None;
                }
                other => yielding.push(other.clone()),
            }
        }
        if states.is_empty() && injected.is_empty() && start.is_none() && stop.is_none() {
            return Some(Hooked {
                yielding,
                ..Hooked::default()
            });
        }
        let program = self.program;
        let library = runtime::NAMESPACE;
        let two = INDENT.repeat(2);
        // The slot (SOURCE-1: "`cache` and `state` ... are what it keeps
        // from one `yield` to the next"), and each capability the body
        // reads, lent to it (TIME-8: a capability "is not retained").
        let lent: Vec<usize> = injected
            .iter()
            .copied()
            .filter(|&local| uses(&yielding, local))
            .collect();
        let mut members = String::new();
        let mut bound = String::new();
        let mut kept_bound = String::new();
        for (local, _) in &states {
            let declared = &body.locals[*local];
            let ty = self.written_type(&declared.ty, declared.at)?;
            let name = identifier(&declared.name);
            members.push_str(&format!("{two}{ty} {name}{{}};\n"));
            bound.push_str(&format!(
                "{two}[[maybe_unused]] auto& {name} = {SLOT}.{name};\n"
            ));
            kept_bound.push_str(&format!(
                "{two}[[maybe_unused]] auto& {name} = kept_.{SLOT}->{name};\n"
            ));
        }
        let mut eval_parameters = Vec::new();
        let mut eval_before = String::new();
        for &local in &lent {
            let name = &body.locals[local].name;
            let (view, made) = super::endpoints::capability(name)?;
            let member = identifier(name);
            members.push_str(&format!("{two}::{library}::Lent<{made}> {member}{{}};\n"));
            let parameter = names::temporary("capability", local);
            eval_parameters.push(format!("{view} {parameter}"));
            eval_before.push_str(&format!(
                "{two}const ::{library}::Lending<{made}> {}{{kept_.{SLOT}->{member}, {made}{{{parameter}}}}};\n",
                names::temporary("lent", local)
            ));
        }
        let declared = format!(
            "{INDENT}// What the body reaches beside its configuration (SOURCE-1): its\n\
             {INDENT}// `state` and `cache`, kept from one `yield` to the next, and each\n\
             {INDENT}// capability it reads, lent for one resumption (TIME-8).\n\
             {INDENT}struct slot_type_\n{INDENT}{{\n{members}{INDENT}}};\n\n"
        );
        let kept = format!(
            "{two}// What the body reaches, where it does not move.\n\
             {two}std::unique_ptr<slot_type_> {SLOT}{{}};\n"
        );
        // `start` (TIME-5, TIME-6): the slot, the first value of each
        // `state` and `cache`, then the body, which runs nothing until it
        // is first resumed (SOURCE-2: "The first evaluation begins the
        // body"), then the hook's statements, which a `return` may leave.
        let mut first: Vec<Stmt> = states
            .iter()
            .filter_map(|(local, value)| {
                value.map(|value| Stmt::Assign {
                    local: *local,
                    operator: None,
                    value: value.clone(),
                    at: body.locals[*local].at,
                })
            })
            .collect();
        let started: &[Stmt] = start.unwrap_or(&[]);
        let mut start_parameters = Vec::new();
        let mut reached = first.clone();
        reached.extend(started.iter().cloned());
        let made = self.capabilities(
            body,
            &injected,
            &[],
            &reached,
            Hook::Start,
            &mut start_parameters,
        )?;
        let mut start_before =
            format!("{two}kept_.{SLOT} = std::make_unique<slot_type_>();\n{kept_bound}{made}");
        let mut start_after = String::new();
        let mut context = Context::new(Kind::Hook(Hook::Start), body.locals.len());
        for role in context.roles.iter_mut().take(signature.parameters.len()) {
            *role = Role::Config;
        }
        let mut scope = Scope::cold(program, body);
        self.in_context(&mut context, |emitter| {
            for assigned in first.drain(..) {
                emitter.statement(&mut scope, &assigned, 2, &mut start_before);
            }
            emitter.statements(&mut scope, started, 2, &mut start_after);
        });
        // `stop` (SOURCE-1): the body is destroyed first, whatever state
        // it is in, then the hook's statements run.
        let mut stop_parameters = Vec::new();
        let mut stop_after = String::new();
        if let Some(statements) = stop {
            stop_parameters.extend(self.hook_configs(signature, body)?);
            let made = self.capabilities(
                body,
                &injected,
                &[],
                statements,
                Hook::Stop,
                &mut stop_parameters,
            )?;
            stop_after.push_str(&kept_bound);
            stop_after.push_str(&made);
            context.kind = Kind::Hook(Hook::Stop);
            let mut scope = Scope::cold(program, body);
            self.in_context(&mut context, |emitter| {
                emitter.statements(&mut scope, statements, 2, &mut stop_after);
            });
        }
        Some(Hooked {
            yielding,
            taken: vec![format!("[[maybe_unused]] slot_type_& {SLOT}")],
            given: vec![format!("*kept_.{SLOT}")],
            prologue: bound,
            declared,
            kept,
            start_parameters,
            start_before,
            start_after,
            eval_parameters,
            eval_before,
            stop_parameters,
            stop_after,
        })
    }

    /// The `const` parameters of the source of `signature` and `body` as
    /// a hook of its node is given them, each an `hgraph::Scalar`, which
    /// the hook reads as `configuration` holds it (`Role::Config`). `None` where a type
    /// is not written yet, which is reported.
    fn hook_configs(&mut self, signature: &Signature, body: &Body) -> Option<Vec<String>> {
        let mut configs = Vec::new();
        for (local, parameter) in signature.parameters.iter().enumerate() {
            let ty = self.written_type(&parameter.ty, parameter.at)?;
            // FN-1: as hgraph holds a configuration (`configuration`).
            let held = super::configuration::held(self.program, &parameter.ty, &ty);
            configs.push(format!(
                "[[maybe_unused]] ::hgraph::Scalar<{}, {held}> {}",
                names::string(&parameter.name),
                identifier(&body.locals[local].name)
            ));
        }
        Some(configs)
    }
}
