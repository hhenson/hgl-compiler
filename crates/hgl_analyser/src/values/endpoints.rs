//! The endpoint operations, `valid(input)`, `modified(input)` and the
//! rest, and the argument of an `endpoint<T>` parameter: each names a
//! temporal input of the node, or `out`, and does not read it (TIME-9).
//! Part of the typing pass ([`super`]); a child of a collection named as
//! an endpoint is read in [`super::children`].

use hgl_diagnostics::{Category, Location};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};

use super::calls::arguments_of;
use super::fit::{not_null, nullable, reads_cycle, whole};
use super::node::RefRead;
use super::{BOOL, Fact, Typer, children};
use crate::hir::{
    EndpointOperation, Expr, ExprKind, Hook, LocalId, Node, Stmt, ValueCategory,
    walk_statement_nodes,
};
use crate::program::{DeclarationKind, Module, Phase, Role};
use crate::syntax::{child, tokens, ungrouped};
use crate::types::{
    Argument, CallArguments, ConstantValue, Detail, NamedKind, Scalar, Signature, Type,
};
use crate::{codes, error};

impl<'a> Typer<'a> {
    /// True when the node being read injects its scheduler or its
    /// alarm, which `scheduled()` asks (TIME-9). Its declarations are
    /// read from its body, so that one written after the initialiser
    /// being read counts.
    fn injects_a_wake(&self) -> bool {
        let Some(body) = child(&self.declaration.syntax, SyntaxKind::Block) else {
            return false;
        };
        body.children()
            .filter(|statement| statement.kind() == SyntaxKind::InjectDecl)
            .flat_map(|statement| tokens(&statement).collect::<Vec<_>>())
            .any(|name| {
                name.kind() == SyntaxKind::Ident && matches!(name.text(), "scheduler" | "alarm")
            })
    }

    /// Reports `scheduled()`, at `selector`, in a node that injects
    /// neither its scheduler nor its alarm (TIME-9: "A node that injects
    /// neither rejects it (`inject.no_scheduler`)").
    fn no_scheduler(&mut self, selector: Location) {
        let note = if crate::phases::is_source(self.declaration) {
            "a source injects neither: its `yield time: value` sets its next evaluation"
        } else {
            "`inject scheduler` or `inject alarm` gives the node one"
        };
        self.diagnostics.emit(
            error(
                Category::Injectable,
                codes::INJECT_NO_SCHEDULER,
                selector,
                "`scheduled()` asks the node's scheduler or its alarm, and this node injects \
                 neither"
                    .to_string(),
                "TIME-9",
            )
            .with_note(note),
        );
    }

    /// `scheduled()`, at `selector`, where no hook of a node stands.
    ///
    /// TIME-9: "`scheduled()` is the handler selector for a wake by the
    /// node's scheduler or alarm ... A node that injects neither rejects
    /// it (`inject.no_scheduler`)". A source is a node that injects
    /// neither (SOURCE-1: "not `out`, `scheduler`, or `alarm`"), and a
    /// node's `state` and `cache` initialisers are the node's own. FN-9:
    /// "A value function injects none", and TIME-9: it "has no output or
    /// schedule of its own"; nor has a graph, whose "Constructed nodes
    /// own their own output and scheduler", or a test. The catalogue has
    /// no row for those, and the code is local. In the initialiser of a
    /// node that injects one the rule does not place the selector, and
    /// it is left as it is.
    pub(super) fn scheduled_outside_hooks(&mut self, selector: Location) {
        let what = if self.testing {
            "a test"
        } else if self.graph {
            "a graph"
        } else if matches!(
            self.declaration.kind,
            DeclarationKind::Function(Role::Temporal)
                | DeclarationKind::Implementation(Role::Temporal, _)
        ) {
            // A node or a source, outside its hooks.
            if !self.injects_a_wake() {
                self.no_scheduler(selector);
            }
            return;
        } else if self.declaration.kind == DeclarationKind::Constant {
            "a constant"
        } else {
            "a value function"
        };
        self.diagnostics.emit(
            error(
                Category::Phase,
                codes::SCHEDULED_OUTSIDE_NODE,
                selector,
                format!(
                    "`scheduled()` asks a node's scheduler or alarm, and {what} is no node: it has \
                     no schedule of its own"
                ),
                "FN-9",
            )
            .with_note(
                "a node that injects `scheduler` or `alarm` asks it in a hook: `when scheduled() \
                 { ... }`",
            ),
        );
    }

    /// An operation on an endpoint (TIME-9): "Elsewhere each takes a
    /// temporal input of the enclosing function by its name, or `out`".
    /// `valid` and `modified` are also the selectors of a `when` header
    /// (TIME-3): "`modified()` subscribes to all temporal inputs;
    /// `modified(a, b)` subscribes to both named inputs, and similarly
    /// for more inputs ... `valid()` requires all temporal inputs to be
    /// valid; `valid(a, b)` requires both named inputs, and similarly
    /// for more inputs ... `valid([])` requires no inputs to be valid."
    pub(super) fn endpoint(
        &mut self,
        operation: EndpointOperation,
        name: &SyntaxToken,
        list: &SyntaxNode,
    ) -> (ExprKind, Type) {
        if let Some(unread) = self.unread_pack_call(list) {
            return unread;
        }
        let at = Location::new(self.file, name.span());
        self.reject_configuration(name, list);
        let arguments = endpoint_arguments(list);
        // How many it takes: a selector may stand with none, and with
        // as many inputs as it names (TIME-3: "`modified(a, b)`
        // subscribes to both named inputs, and similarly for more
        // inputs").
        let (fewest, most) = match operation {
            EndpointOperation::Scheduled => (0, 0),
            EndpointOperation::Valid | EndpointOperation::Modified => (0, usize::MAX),
            _ => (1, 1),
        };
        if arguments.len() < fewest {
            self.diagnostics.emit(error(
                Category::Type,
                codes::ARGUMENT_MISSING,
                at,
                format!("`{}` is given no endpoint: it takes one", operation.name()),
                "TIME-9",
            ));
        }
        for surplus in arguments.iter().skip(most) {
            self.diagnostics.emit(error(
                Category::Type,
                codes::ARGUMENT_UNEXPECTED,
                self.at(surplus),
                format!(
                    "`{}` takes {}, and this is one more",
                    operation.name(),
                    if most == 0 {
                        "no argument"
                    } else {
                        "one endpoint"
                    }
                ),
                "TIME-9",
            ));
        }
        // TIME-9: `scheduled()` is "a wake asked of the scheduler is due
        // in this evaluation", and `start` and `stop` are no evaluation
        // of the node: "A node that injects neither rejects it
        // (`inject.no_scheduler`), and so do `start` and `stop`
        // (`scheduled.lifecycle`)."
        if operation == EndpointOperation::Scheduled
            && let Some(hook @ (Hook::Start | Hook::Stop)) = self.hook
        {
            // The selector: `scheduled()`, its name to its `)`.
            let selector = list.parent().map_or(at, |call| self.at(&call));
            let hook = if hook == Hook::Start { "start" } else { "stop" };
            self.diagnostics.emit(
                error(
                    Category::Phase,
                    codes::SCHEDULED_LIFECYCLE,
                    selector,
                    format!(
                        "`scheduled()` asks whether a wake is due in the node's evaluation, and \
                         `{hook}` is none"
                    ),
                    "TIME-9",
                )
                .with_note("a `when` asks it: `when scheduled() { ... }`"),
            );
        }
        // The endpoints named, each by its name. `valid([])` and
        // `modified([])` name none (TIME-3); any other sequence is no
        // endpoint, and is reported as one.
        let none = matches!(
            operation,
            EndpointOperation::Valid | EndpointOperation::Modified
        ) && matches!(arguments.as_slice(), [only]
                if only.kind() == SyntaxKind::SequenceExpr && only.children().next().is_none());
        // TIME-9: "`scheduled()` is the handler selector for a wake by
        // the node's scheduler or alarm ... A node that injects neither
        // rejects it". With a scheduler it is the scheduler's
        // `is_scheduled_now`, and with an alarm it holds where no
        // temporal input has ticked: with neither it asks nothing.
        if operation == EndpointOperation::Scheduled && !self.injects_a_wake() {
            // The selector: `scheduled()`, its name to its `)`.
            let selector = list.parent().map_or(at, |call| self.at(&call));
            self.no_scheduler(selector);
        }
        // TIME-9, END-8: "A child of a collection is an endpoint whether
        // or not it is valid: `valid`, `modified`, `delta_value` and
        // `last_modified` take it, and `all_valid` where it is a
        // collection". It is given alone; `is_active`, `activate` and
        // `passivate` "operate on direct temporal inputs".
        if let [only] = arguments.as_slice()
            && matches!(
                operation,
                EndpointOperation::Valid
                    | EndpointOperation::AllValid
                    | EndpointOperation::Modified
                    | EndpointOperation::DeltaValue
                    | EndpointOperation::LastModified
            )
        {
            let reported = self.diagnostics.len();
            if let Some(found) = self.child_endpoint(only) {
                return self.child_operation(operation, at, only, found, reported);
            }
        }
        let inputs: Option<Vec<LocalId>> = if arguments.is_empty() || most == 0 {
            None
        } else if none {
            Some(Vec::new())
        } else {
            Some(
                arguments
                    .iter()
                    .take(most)
                    .filter_map(|entry| self.endpoint_named(operation, entry))
                    .collect(),
            )
        };
        // TIME-5: `start` "gives none to an endpoint operation".
        if self.hook == Some(Hook::Start)
            && let Some(inputs) = &inputs
        {
            let given: Vec<String> = inputs
                .iter()
                .filter(|input| Some(**input) != self.out)
                .map(|input| self.locals[*input].name.clone())
                .collect();
            for input in given {
                self.input_lifecycle(at, &input);
            }
        }
        // TIME-5: every input is passive in stop; neither operation
        // may change its subscription there.
        if self.hook == Some(Hook::Stop)
            && matches!(
                operation,
                EndpointOperation::Activate | EndpointOperation::Passivate
            )
            && let Some(inputs) = &inputs
        {
            for &input in inputs {
                self.diagnostics.emit(error(
                    Category::Phase,
                    codes::INPUT_LIFECYCLE,
                    at,
                    format!(
                        "`{}` cannot change the activity of `{}` in `stop`: every input is passive",
                        operation.name(),
                        self.locals[input].name
                    ),
                    "TIME-5",
                ));
            }
        }
        // TIME-9: "`all_valid` takes only a collection or a window;
        // another endpoint is rejected (`endpoint.argument`)". A
        // collection is "a list, a tuple, a struct and a dictionary", and
        // "A window is valid and all valid as TYPE-11 has it": core's
        // END-9, "`all_valid(x)` where it holds at least its minimum".
        if operation == EndpointOperation::AllValid
            && let Some(inputs) = &inputs
        {
            for (input, given) in inputs.iter().zip(&arguments) {
                let ty = &self.locals[*input].ty;
                let collection = matches!(
                    ty,
                    Type::List { .. }
                        | Type::Tuple(_)
                        | Type::Dict { .. }
                        | Type::Named {
                            kind: NamedKind::Struct,
                            ..
                        }
                        | Type::RollingWindow { .. }
                        | Type::Parameter { .. }
                        | Type::Unknown
                );
                if !collection {
                    self.diagnostics.emit(
                        error(
                            Category::Phase,
                            codes::ENDPOINT_ARGUMENT,
                            self.at(given),
                            format!(
                                "`all_valid` takes a collection or a window, and `{ty}` is neither"
                            ),
                            "TIME-9",
                        )
                        .with_note(
                            "a collection is a list, a tuple, a struct or a dictionary; a window \
                             is a `rolling_window`",
                        ),
                    );
                }
            }
        }
        let writes = operation == EndpointOperation::Invalidate;
        if let (Some(inputs), Some(out)) = (&inputs, self.out)
            && inputs.contains(&out)
        {
            match self.hook {
                Some(Hook::Start) => self.out_lifecycle(
                    at,
                    "`out` is neither read nor written in `start`".to_string(),
                ),
                Some(Hook::Stop) if writes => {
                    self.out_lifecycle(at, "`out` is read in `stop`, and not written".to_string());
                }
                _ => {}
            }
        }
        let ty = match operation {
            EndpointOperation::Valid
            | EndpointOperation::AllValid
            | EndpointOperation::Modified
            | EndpointOperation::Scheduled
            | EndpointOperation::IsActive => BOOL,
            // END-3: "`delta_value(x)` returns what `x` published this
            // cycle, a `delta<T>` for an endpoint of T (the language's
            // TYPE-16). For a scalar it is the value published".
            EndpointOperation::DeltaValue => {
                let input = inputs.as_ref().and_then(|inputs| inputs.first()).copied();
                let of = input.map(|input| self.locals[input].ty.clone());
                // TYPE-16: "`signal`: None". An input that may be left
                // out is its shape under a `?`.
                let none = of
                    .as_ref()
                    .is_some_and(|of| matches!(not_null(Some(of)), Some(Type::Signal)));
                // FN-16: "the endpoint operations take them (TIME-9),
                // `delta_value` aside ... A read of the value or of the
                // `delta_value` of one rejects".
                if let Some(input) = input.filter(|&input| self.designates(input)) {
                    self.ref_read(input, None, RefRead::Delta, at);
                    Type::Unknown
                } else if none {
                    self.diagnostics.emit(
                        error(
                            Category::Shape,
                            codes::DELTA_UNSUPPORTED_SHAPE,
                            at,
                            format!(
                                "`{}` has no delta",
                                of.as_ref().map_or(String::new(), ToString::to_string)
                            ),
                            "TYPE-16",
                        )
                        .with_note("a `signal` has no payload, and publishes no value of its own"),
                    );
                    Type::Unknown
                } else {
                    // END-3, TIME-1, TIME-4: reading a direct input's
                    // delta requires its admission, including in a result
                    // or nullable place. A delta is not a whole payload
                    // read of every child of a collection.
                    if let Some(input) = input.filter(|&input| self.is_temporal_input(input))
                        && self.hook != Some(Hook::Start)
                    {
                        self.delta_read(input, at);
                    }
                    of.map_or(Type::Unknown, Type::delta)
                }
            }
            // END-6: a `datetime?`.
            EndpointOperation::LastModified => nullable(Type::Scalar(Scalar::Datetime)),
            EndpointOperation::Invalidate
            | EndpointOperation::Activate
            | EndpointOperation::Passivate => Type::Unknown,
        };
        (ExprKind::Endpoint { operation, inputs }, ty)
    }

    /// The generic arguments written on an endpoint operation, held to
    /// what the operation is: core declares each with one type, the
    /// type the endpoint holds, `valid<T>(input: endpoint<T>)` (TIME-9).
    pub(super) fn endpoint_generics(
        &mut self,
        name: &SyntaxToken,
        given: Option<&CallArguments>,
        kind: &ExprKind,
    ) {
        let Some(given) = given else {
            return;
        };
        let [(at, argument)] = given.arguments.as_slice() else {
            self.diagnostics.emit(error(
                Category::Type,
                codes::GENERIC_ARGUMENT_COUNT,
                Location::new(self.file, name.span()),
                format!(
                    "`{}` takes 1 generic argument and is given {}",
                    name.text(),
                    given.arguments.len()
                ),
                "GEN-8",
            ));
            return;
        };
        let held = match argument {
            Argument::Type(held) => held,
            Argument::Constant(_) => {
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::GENERIC_ARGUMENT_KIND,
                    *at,
                    format!(
                        "`{}` takes the type its endpoint holds, and this is a value",
                        name.text()
                    ),
                    "GEN-14",
                ));
                return;
            }
        };
        // A child that holds another type is no endpoint of the one
        // written either (EXPR-5).
        if let ExprKind::ChildEndpoint { child, .. } = kind {
            if whole(held) && whole(&child.ty) && child.ty != *held {
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::TYPE_MISMATCH,
                    *at,
                    format!(
                        "`{}<{held}>` takes an endpoint of `{held}`, and this child holds `{}`",
                        name.text(),
                        child.ty
                    ),
                    "EXPR-5",
                ));
            }
            return;
        }
        let ExprKind::Endpoint {
            inputs: Some(inputs),
            ..
        } = kind
        else {
            return;
        };
        // An input that holds another type is no endpoint of the one
        // written (EXPR-5). What is not known has been reported.
        let other: Vec<String> = inputs
            .iter()
            .map(|input| &self.locals[*input])
            .filter(|input| whole(held) && whole(&input.ty) && input.ty != *held)
            .map(|input| format!("`{}` holds `{}`", input.name, input.ty))
            .collect();
        if !other.is_empty() {
            self.diagnostics.emit(error(
                Category::Type,
                codes::TYPE_MISMATCH,
                *at,
                format!(
                    "`{}<{held}>` takes an endpoint of `{held}`, and {}",
                    name.text(),
                    other.join(" and ")
                ),
                "EXPR-5",
            ));
        }
    }

    /// Reports the generic arguments written on `scheduled<...>()`, at
    /// `node` with its name `name`. TIME-9: "`scheduled()` is the
    /// handler selector", and core declares no function of the name: it
    /// has no generic parameter for a call to give one to (GEN-8: "A
    /// call may supply its generic arguments as a generic constructor
    /// does"). The read pass keeps no arguments for a name no
    /// declaration binds, so they are counted as written.
    pub(super) fn scheduled_generics(&mut self, node: &SyntaxNode, name: &SyntaxToken) {
        let given = child(node, SyntaxKind::GenericArgs).map_or(0, |list| {
            list.children()
                .filter(|argument| argument.kind() != SyntaxKind::Error)
                .count()
        });
        if given == 0 {
            return;
        }
        self.diagnostics.emit(
            error(
                Category::Type,
                codes::GENERIC_ARGUMENT_COUNT,
                Location::new(self.file, name.span()),
                format!(
                    "`{}` takes no generic arguments and is given {given}",
                    name.text()
                ),
                "GEN-8",
            )
            .with_note("`scheduled()` is the handler selector, and asks the node's own wake"),
        );
    }

    /// The argument of an `endpoint<T>` parameter (TIME-9): "A call
    /// gives an `endpoint<T>` parameter a temporal input of the
    /// enclosing node by its name, or `out`, and nothing else".
    ///
    /// The input is named and not read: its payload may be invalid, and
    /// the function is given the endpoint to ask. In a graph or a value
    /// function there is no enclosing node, and nothing to give.
    pub(super) fn endpoint_given(&mut self, node: &SyntaxNode) -> Expr {
        let at = self.at(node);
        let named = endpoint_argument(node);
        // TIME-9: "a child of one, `parts[1]`, `quote.bid` or
        // `parts[i]`", in a hook.
        let reported = self.diagnostics.len();
        if self.hook.is_some()
            && let Some(found) = self.child_endpoint(&named)
        {
            if let Some(child) = found {
                self.child_in_start(at, &child);
                let ty = Type::Endpoint(Box::new(child.ty.clone()));
                return Expr {
                    kind: ExprKind::ChildEndpointOf(Box::new(child)),
                    ty,
                    at,
                    category: ValueCategory::Unsettled,
                };
            }
            if self.diagnostics.len() != reported {
                return Expr {
                    kind: ExprKind::Unread,
                    ty: Type::Unknown,
                    at,
                    category: ValueCategory::Unsettled,
                };
            }
        }
        let local = (named.kind() == SyntaxKind::NameExpr)
            .then(|| self.local_of(&named))
            .flatten()
            .filter(|&local| {
                self.hook.is_some() && (self.is_temporal_input(local) || Some(local) == self.out)
            });
        let Some(local) = local else {
            self.diagnostics.emit(
                error(
                    Category::Phase,
                    codes::ENDPOINT_ARGUMENT,
                    at,
                    "an `endpoint<T>` parameter takes a temporal input of the node by its \
                     name, or `out`"
                        .to_string(),
                    "TIME-9",
                )
                .with_note(if self.hook.is_some() {
                    "a state, a `const` parameter and an expression have no endpoint"
                } else {
                    "an endpoint is given in a node's hook: a graph and a value function have \
                     no input of their own to give"
                }),
            );
            return Expr {
                kind: ExprKind::Unread,
                ty: Type::Unknown,
                at,
                category: ValueCategory::Unsettled,
            };
        };
        Expr {
            kind: ExprKind::EndpointOf(local),
            ty: Type::Endpoint(Box::new(self.locals[local].ty.clone())),
            at,
            category: ValueCategory::Unsettled,
        }
    }

    /// The endpoint an argument of an endpoint operation names: a
    /// temporal parameter of the node, or its injected `out`. Any other
    /// is reported (`endpoint.argument`).
    fn endpoint_named(
        &mut self,
        operation: EndpointOperation,
        node: &SyntaxNode,
    ) -> Option<LocalId> {
        let local = match node.kind() {
            SyntaxKind::NameExpr => self.local_of(node),
            _ => None,
        };
        // TIME-9: only direct inputs have a subscription to change.
        let input_only = matches!(
            operation,
            EndpointOperation::Activate | EndpointOperation::Passivate
        );
        // END-4: invalidate clears this node's own output. A direct
        // input, including a ref input, is another node's endpoint.
        let output_only = operation == EndpointOperation::Invalidate;
        match local {
            Some(local)
                if if output_only {
                    Some(local) == self.out
                } else {
                    self.is_temporal_input(local) || (!input_only && Some(local) == self.out)
                } =>
            {
                Some(local)
            }
            _ => {
                self.diagnostics.emit(error(
                    Category::Phase,
                    codes::ENDPOINT_ARGUMENT,
                    self.at(node),
                    if output_only {
                        "`invalidate` takes this node's own `out`".to_string()
                    } else {
                        format!(
                            "`{}` takes a temporal input of `{}` by its name{}",
                            operation.name(),
                            self.declaration.name,
                            if input_only { "" } else { ", or `out`" }
                        )
                    },
                    if output_only { "END-4" } else { "TIME-9" },
                ));
                // This is already an endpoint; rejecting mutation does
                // not read its payload as well, notably for ref (ERR-9).
                if output_only && local.is_some_and(|local| self.is_temporal_input(local)) {
                    return None;
                }
                // Read for what it reports itself. The argument is
                // rejected already, so what it reads of an input is not
                // judged as well (ERR-9).
                let frame = self.present.len();
                let inputs = self.temporal_inputs();
                self.present.extend(inputs.iter().copied().map(Fact::Valid));
                self.present
                    .extend(inputs.into_iter().map(children::all_valid_fact));
                self.expression(node, Some(&Type::Unknown));
                self.present.truncate(frame);
                None
            }
        }
    }
}

impl Typer<'_> {
    /// TYPE-11: a duration window's delta takes a result or nullable
    /// place, because a tick where a value only leaves has no delta.
    /// `removed_value` of any window takes only a result.
    pub(super) fn plain_cycle_reads(&mut self, statements: &[Stmt]) {
        // The checked reads are identified by address. The walk visits
        // the CheckedDelta of either admitted form before its read.
        let mut taken: Vec<*const Expr> = Vec::new();
        let mut reports = Vec::new();
        let rejected = |at, message| {
            error(
                Category::Phase,
                codes::PAYLOAD_UNGUARDED,
                at,
                message,
                "TYPE-11",
            )
        };
        let program = self.program;
        walk_statement_nodes(statements, &mut |node| {
            let Node::Expression(value) = node else {
                return;
            };
            if let ExprKind::CheckedDelta(read) = &value.kind {
                taken.push(&**read);
                return;
            }
            if !reads_cycle(program, value) || taken.contains(&std::ptr::from_ref(value)) {
                return;
            }
            let delta = match &value.kind {
                ExprKind::Endpoint {
                    inputs: Some(inputs),
                    ..
                } => inputs.first().map(|&input| &self.locals[input].ty),
                ExprKind::ChildEndpoint { child, .. } => Some(&child.ty),
                // `removed_value`, of any window.
                _ => {
                    reports.push(
                        rejected(
                            value.at,
                            "this is `removed_value`, which has no value in a cycle in which nothing \
                             leaves the window, and its place takes no `result`"
                                .to_string(),
                        )
                        .with_note("read it where a `result<V, error>` is taken (ERR-14)"),
                    );
                    None
                }
            };
            if let Some(of) = delta
                && duration_window(of.omission_removed())
            {
                reports.push(
                    rejected(
                        value.at,
                        format!(
                            "this is the `delta_value` of a `{of}`, a window of a duration, which \
                             has no delta where a value only leaves it"
                        ),
                    )
                    .with_note("give it a nullable place or a `result<V, error>` place (ERR-14)"),
                );
            }
        });
        for report in reports {
            self.diagnostics.emit(report);
        }
    }
}

/// True for a window of a duration, `rolling_window<f64, 3s, 1s>`, which
/// a value leaves by its age, whether or not one arrives (TYPE-11).
fn duration_window(ty: &Type) -> bool {
    matches!(
        ty,
        Type::RollingWindow { maximum, .. } if matches!(maximum.value, ConstantValue::Duration(_))
    )
}

/// Whether a generic node has a read whose admission or result form
/// depends on an input's shape (GEN-7, TIME-1, ERR-14, TYPE-11). With
/// the instance signature available, check only the affected shapes.
pub(crate) fn node_endpoint_reads(
    module: &Module,
    declaration: usize,
    concrete: Option<&Signature>,
) -> bool {
    if module.phases.get(declaration) != Some(&Some(Phase::Node)) {
        return false;
    }
    let Some(Detail::Callable(signature)) = module.details.get(declaration) else {
        return false;
    };
    if signature.generics.is_empty() {
        return false;
    }
    let Some(Some(body)) = module.bodies.get(declaration) else {
        return false;
    };
    let parameters: Vec<_> = signature
        .generics
        .iter()
        .map(|generic| generic.at)
        .collect();
    let depends_on_shape = |local: LocalId, delta: bool| {
        let Some(parameter) = signature.parameters.get(local) else {
            return false;
        };
        if parameter.constant || parameter.reference {
            return false;
        }
        let source = parameter.ty.omission_removed();
        let unsettled = matches!(source, Type::Parameter { .. })
            || delta
                && matches!(source, Type::RollingWindow { maximum, .. }
                    if maximum.value.names_one_of(&parameters));
        unsettled
            && concrete.is_none_or(|concrete| {
                let ty = concrete.parameters[local].ty.omission_removed();
                if delta {
                    duration_window(ty)
                } else {
                    children::read_whole_all_valid(ty)
                        || matches!(ty, Type::Dict { .. } | Type::List { size: None, .. })
                }
            })
    };
    let mut pending = false;
    walk_statement_nodes(&body.statements, &mut |node| {
        if !pending && let Node::Expression(value) = node {
            pending = match &value.kind {
                ExprKind::Local(local) => depends_on_shape(*local, false),
                ExprKind::Endpoint {
                    operation: EndpointOperation::DeltaValue,
                    inputs: Some(inputs),
                } => inputs
                    .first()
                    .is_some_and(|&local| depends_on_shape(local, true)),
                _ => false,
            };
        }
    });
    pending
}

/// An argument of an endpoint operation or an `endpoint<T>` parameter as
/// every check reads it: EXPR-1, "Parentheses select grouping", so
/// `valid((quote.bid))` names what `valid(quote.bid)` does, and grouping
/// never changes what an argument is, its admission or its guard.
pub(super) fn endpoint_argument(node: &SyntaxNode) -> SyntaxNode {
    ungrouped(node.clone())
}

/// The arguments of the endpoint operation whose list is `list`, each
/// as `endpoint_argument` reads it.
pub(super) fn endpoint_arguments(list: &SyntaxNode) -> Vec<SyntaxNode> {
    arguments_of(list)
        .into_iter()
        .map(|(_, value)| endpoint_argument(&value))
        .collect()
}
