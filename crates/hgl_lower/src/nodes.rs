//! What a node's hooks ask, as the language says it: what each `when`
//! subscribes to and requires, what the node's handlers ask together,
//! what wakes it beside its inputs, which of its locals are its inputs,
//! and which inputs a `ref` result designates and how. Part of
//! lowering: a transpiler writes each handler's guards and each
//! designation with these (TIME-3, TIME-4, TIME-9, FN-16).

use hgl_analyser::hir::{EndpointOperation, Expr, ExprKind, Header, Local, Stmt};
use hgl_analyser::{LocalKind, NamedKind, Signature, Type};
use hgl_diagnostics::Location;

use crate::types::fixed_list_size;

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
    /// does not say what the two ask together, and a target reports it.
    pub beside: Option<Location>,
    /// True where its header has `modified([])` alone or as a top-level
    /// conjunct: the selector, which a wake of the scheduler or the
    /// alarm runs, and in its body `modified([])` is true (TIME-3).
    pub empty: bool,
    /// True where its header has a validity selector, `valid(a)`,
    /// `valid()` or `valid([])`, alone or as a top-level conjunct: what it
    /// requires is what those name. With none, "Omitting the validity
    /// selector requires all temporal inputs to be valid" (TIME-3).
    pub asks_valid: bool,
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
        // A selector is an endpoint operation the analyser typed the
        // conjunct as (TIME-3: "A selector sets subscription or admission
        // when it stands alone in the header or appears as a top-level
        // conjunct").
        let asks_valid = conjuncts.iter().any(|conjunct| {
            matches!(
                &conjunct.kind,
                ExprKind::Endpoint {
                    operation: EndpointOperation::Valid,
                    ..
                }
            )
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
            asks_valid,
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

/// Each designation of a temporal input that the node whose statements
/// are `statements` makes, in the order written: what a `return` to its
/// `ref` result, or an `out =` of it, designates (FN-16: "A parameter
/// that is not `ref` may be designated too"), how, and where. `locals`
/// and `signature` are the node's.
pub fn designations(
    locals: &[Local],
    signature: Option<&Signature>,
    statements: &[Stmt],
) -> Vec<(usize, Designated, Location)> {
    use hgl_analyser::hir::{Node, walk_statement_nodes};
    let mut found: Vec<(usize, Designated, Location)> = Vec::new();
    walk_statement_nodes(statements, &mut |node| {
        let value = match node {
            Node::Statement(Stmt::Return {
                value: Some(value), ..
            }) => value,
            Node::Statement(Stmt::Assign { local, value, .. }) if is_out(locals, *local) => value,
            _ => return,
        };
        if let Some((local, how)) = designated(locals, signature, value) {
            found.push((local, how, value.at));
        }
    });
    found
}

/// True where `value` is a designation, what a `return` to a `ref` result
/// or `out =` of one is given (FN-16): the endpoint of a temporal input
/// of the node, or of a child of one, which the analyser reads as such
/// only there.
pub fn designates(value: &Expr) -> bool {
    matches!(
        value.kind,
        ExprKind::EndpointOf(_) | ExprKind::ChildEndpointOf(_)
    )
}

/// How a node designates a temporal input (FN-16: "A parameter that is
/// not `ref` may be designated too"): whole, by a position of a fixed list,
/// by a dictionary key, or by a struct field path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Designated {
    /// `return value`
    Whole,
    /// `return items[idx]`, of a `list<T, N>`.
    Position,
    /// `return dictionary[key]`.
    Key,
    /// `return pair.left` or `return outer.inner.left`, of an ordinary struct
    /// input (field indices from outermost to innermost).
    Field(Vec<usize>),
}

/// What `value`, a designation (`designates`), designates of a temporal
/// input of the node whose locals are `locals` and whose signature is
/// `signature`: the input, and how. `None` for a child of a `ref`
/// parameter, and for a child of another shape.
pub fn designated(
    locals: &[Local],
    signature: Option<&Signature>,
    value: &Expr,
) -> Option<(usize, Designated)> {
    match &value.kind {
        ExprKind::EndpointOf(local) if is_input(locals, signature, *local) => {
            Some((*local, Designated::Whole))
        }
        ExprKind::ChildEndpointOf(child) => match &child.kind {
            ExprKind::Index { base, .. } => match (&base.kind, &base.ty) {
                (ExprKind::Local(local), Type::List { size, .. })
                    if fixed_list_size(size.as_ref()).is_some()
                        && is_input(locals, signature, *local)
                        && !is_reference(locals, signature, *local) =>
                {
                    Some((*local, Designated::Position))
                }
                (ExprKind::Local(local), Type::Dict { .. })
                    if is_input(locals, signature, *local)
                        && !is_reference(locals, signature, *local) =>
                {
                    Some((*local, Designated::Key))
                }
                _ => None,
            },
            ExprKind::Field { .. } => {
                let mut path = Vec::new();
                let mut current = child.as_ref();
                while let ExprKind::Field { base, member } = &current.kind {
                    if !matches!(
                        base.ty,
                        Type::Named {
                            kind: NamedKind::Struct,
                            ..
                        }
                    ) {
                        return None;
                    }
                    path.push(*member);
                    current = base.as_ref();
                }
                match current.kind {
                    ExprKind::Local(local)
                        if !path.is_empty()
                            && is_input(locals, signature, local)
                            && !is_reference(locals, signature, local) =>
                    {
                        path.reverse();
                        Some((local, Designated::Field(path)))
                    }
                    _ => None,
                }
            }
            _ => None,
        },
        _ => None,
    }
}
