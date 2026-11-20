//! What a node's hooks ask, as the language says it: what each `when`
//! subscribes to and requires, what the node's handlers ask together,
//! what wakes it beside its inputs, and which of its locals are its
//! inputs. Part of lowering: a transpiler writes each handler's guards
//! with these (TIME-3, TIME-4, TIME-9).

use hgl_analyser::hir::{EndpointOperation, Expr, ExprKind, Header, Local, Stmt};
use hgl_analyser::{LocalKind, Signature};
use hgl_diagnostics::Location;

/// What wakes a node beside its inputs (TIME-9): the scheduler or the
/// alarm it injects.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Timer {
    /// `inject scheduler`: `scheduled()` is due where a wake it was
    /// asked for is due.
    Scheduler,
    /// `inject alarm`: `scheduled()` "holds where the node is evaluated
    /// and no temporal input has ticked".
    Alarm,
}

impl Timer {
    /// The timer an `inject` of the capability `injected` gives a node:
    /// `scheduler` and `alarm` (core's CAP-4, CAP-5), and `None` for any
    /// other capability.
    pub fn of(injected: &str) -> Option<Timer> {
        match injected {
            "scheduler" => Some(Timer::Scheduler),
            "alarm" => Some(Timer::Alarm),
            _ => None,
        }
    }
}

/// One `when` of a node: what its header asks, and its body.
pub struct Handler<'a> {
    /// The inputs whose tick wakes it (TIME-3), as local positions.
    pub subscribed: Vec<usize>,
    /// The inputs it requires to be valid.
    pub required: Vec<usize>,
    /// Predicates checked when the node runs, including explicit modification guards.
    pub conditions: Vec<&'a Expr>,
    /// True where its header has `scheduled()`, alone or as a top-level
    /// conjunct: a wake by the node's scheduler or alarm runs it
    /// (TIME-3).
    pub scheduled: bool,
    /// True where its header writes a `modified` selector. One that
    /// writes none "subscribes to every temporal input".
    pub selects: bool,
    /// Where its header has `modified([])` beside a `modified` selector
    /// that names inputs, each a top-level conjunct: the specification
    /// does not say what the two ask together, and the target reports it.
    pub beside: Option<Location>,
    /// True where its header has `modified([])` alone or as a top-level
    /// conjunct: the selector, which a wake of the scheduler or the
    /// alarm runs, and in its body `modified([])` is true (TIME-3).
    pub empty: bool,
    /// Its statements.
    pub body: &'a [Stmt],
}

impl<'a> Handler<'a> {
    /// A handler as the analyser read its header (`hir::Header`):
    /// what it subscribes to and requires, and the conjuncts of its
    /// condition that are checked when it runs.
    pub fn of(header: &'a Header, condition: Option<&'a Expr>, body: &'a [Stmt]) -> Handler<'a> {
        let conjuncts = condition.map(Expr::conjuncts).unwrap_or_default();
        let empty = conjuncts.iter().any(|conjunct| {
            matches!(&conjunct.kind, ExprKind::Endpoint {
                operation: EndpointOperation::Modified,
                inputs: Some(inputs),
            } if inputs.is_empty())
        });
        Handler {
            subscribed: header.subscribed.clone(),
            required: header.required.clone(),
            conditions: header
                .conditions
                .iter()
                .filter_map(|&position| conjuncts.get(position).copied())
                .collect(),
            scheduled: header.scheduled,
            selects: header.selects,
            beside: header.beside,
            empty,
            body,
        }
    }
}

/// The inputs among `inputs` that a handler among `handlers` subscribes
/// to. TIME-4: "Aggregate subscriptions cover every handler."
pub fn subscribed_inputs(inputs: &[usize], handlers: &[Handler]) -> Vec<usize> {
    inputs
        .iter()
        .copied()
        .filter(|input| {
            handlers
                .iter()
                .any(|handler| handler.subscribed.contains(input))
        })
        .collect()
}

/// The inputs among `inputs` that every handler among `handlers`
/// requires to be valid. TIME-4: "Node-wide validity includes only
/// requirements common to executable handlers; other guards stay
/// per-handler."
pub fn required_inputs(inputs: &[usize], handlers: &[Handler]) -> Vec<usize> {
    inputs
        .iter()
        .copied()
        .filter(|input| {
            handlers
                .iter()
                .all(|handler| handler.required.contains(input))
        })
        .collect()
}

/// True when `local`, among a function's `locals`, is a temporal input
/// of it: a parameter of its `signature` that is not `const` (FN-3). A
/// test or a module's store, which have no signature, has none.
pub fn is_input(locals: &[Local], signature: Option<&Signature>, local: usize) -> bool {
    let declared = &locals[local];
    declared.kind == LocalKind::Parameter
        && signature.is_some_and(|signature| {
            signature
                .parameters
                .iter()
                .any(|parameter| parameter.name == declared.name && !parameter.constant)
        })
}

/// True when `local`, among a function's `locals`, is a temporal input
/// of it that is marked `ref` (FN-16): an endpoint whose value is not
/// read.
pub fn is_reference(locals: &[Local], signature: Option<&Signature>, local: usize) -> bool {
    let declared = &locals[local];
    declared.kind == LocalKind::Parameter
        && signature.is_some_and(|signature| {
            signature.parameters.iter().any(|parameter| {
                parameter.name == declared.name && !parameter.constant && parameter.reference
            })
        })
}

/// True when `local`, among a node's `locals`, is its `inject out`
/// (TIME-7).
pub fn is_out(locals: &[Local], local: usize) -> bool {
    let declared = &locals[local];
    declared.kind == LocalKind::Inject && declared.name == "out"
}
