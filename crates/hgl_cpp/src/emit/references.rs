//! The `ref` mode of a node's parameters and result (FN-16, TIME-10;
//! core's END-10): the family of the dispatch point (`forms`) that writes
//! a designation, and what a node's struct and wiring function take of
//! the mode (`nodes`). Part of the transpiler, the last stage.
//!
//! A reference is a temporal value whose payload designates an endpoint
//! (TIME-10), and hgraph's `REF` input and output are the engine's form
//! of it: a node's `REF` input given an ordinary connection holds a
//! reference to it, and an ordinary input given a `REF` output observes
//! what that designates, as hgraph binds them.
//!
//! | HGL | C++ |
//! | --- | --- |
//! | `ref low: i64`, a `ref` parameter | hgraph's `REF` input of the parameter's temporal form, `::hgraph::In<"low", ::hgraph::REF<::hgraph::TS<::hgraph::Int>>>`, active and required as any input is: active where a handler subscribes to it, which wakes the node where it is given a designation, and valid where a reference is present (TIME-10); its wiring function takes a connection of T |
//! | `-> ref i64`, a `ref` result | hgraph's `REF` output, `::hgraph::Out<::hgraph::REF<...>>`; the wiring function gives it as a connection of T, `.as<::hgraph::TS<...>>()`, to which an ordinary reader is bound as hgraph binds a reader of a `REF` |
//! | `return low` of a whole `ref` parameter | `out_.set(::hglrt::designation(low));`: what it designates as the `return` runs, which the result keeps until the node writes it again |
//! | `return price` of an input that is no `ref` parameter | the input is given to the node again, as a `REF` input it is active on (`designated_00_`); the node keeps which it designates in its state (`designates_`), publishes what that `REF` holds, and, where it ticks while the result designates it, publishes it again and runs no handler for that wake |
//! | `out = ...` of a `ref` result | as the `return`, and the evaluation goes on |
//! | `invalidate(out)` of a `ref` result that follows an input | the result follows none, then is invalidated |
//!
//! Not written yet, and reported: a designation of a child of an input,
//! `return items[idx]`; a `ref` result of a source; and a designation of
//! an input that is no `ref` parameter in a node that injects a scheduler
//! or an alarm.

use hgl_analyser::hir::{Body, EndpointOperation, Expr, ExprKind, Hook, Stmt};
use hgl_analyser::{Phase, Signature, Type};
use hgl_diagnostics::Location;
use hgl_lower::nodes::{
    Designated, Timer, designated, designates, designations, is_out, is_reference,
};

use super::Emitter;
use super::forms::Family;
use super::names::{self, identifier};
use super::nodes::{Kind, OUT, STATE};
use super::statements::Scope;
use super::values::INDENT;
use crate::runtime;

/// The name of the node's state member, and of the local bound to it in
/// `eval`, that says which input the result follows: its place among
/// those [`followed`] gives, or -1 for none. A name no HGL name gives
/// (`names`).
const DESIGNATES: &str = "designates_";

/// The references' family: the statements of a hook that designate.
pub(super) struct References;

impl Family for References {
    fn statement(
        &self,
        emitter: &mut Emitter<'_>,
        scope: &mut Scope<'_>,
        statement: &Stmt,
        depth: usize,
        text: &mut String,
    ) -> bool {
        if emitter.kind() != Some(Kind::Hook(Hook::When)) {
            return false;
        }
        let Some(signature) = emitter
            .temporal
            .as_ref()
            .and_then(|context| context.signature.clone())
            .filter(|signature| signature.reference)
        else {
            return false;
        };
        let body = scope.body;
        let indent = INDENT.repeat(depth);
        match statement {
            // FN-16: "A `return` to a `ref` result is given an expression
            // that names an endpoint"; it publishes, and leaves the
            // evaluation (FN-7).
            Stmt::Return {
                value: Some(value), ..
            } if designates(value) => {
                if let Some(written) = emitter.designation(&signature, body, value, depth) {
                    text.push_str(&format!("{written}{indent}return;\n"));
                }
                true
            }
            // FN-16: "`out = items[idx]` designates as `return items[idx]`
            // does"; the evaluation goes on (TIME-7).
            Stmt::Assign {
                local,
                operator: None,
                value,
                ..
            } if is_out(&body.locals, *local) && designates(value) => {
                if let Some(written) = emitter.designation(&signature, body, value, depth) {
                    text.push_str(&written);
                }
                true
            }
            // END-4: `invalidate(out)` leaves the result designating
            // nothing, so it follows no input after it.
            Stmt::Expr(
                value @ Expr {
                    kind:
                        ExprKind::Endpoint {
                            operation: EndpointOperation::Invalidate,
                            inputs: Some(inputs),
                        },
                    ..
                },
            ) if matches!(inputs.as_slice(), [only] if is_out(&body.locals, *only))
                && !followed(&signature, body).is_empty() =>
            {
                let written = emitter.expression(scope, value);
                text.push_str(&format!("{indent}{DESIGNATES} = -1;\n{indent}{written};\n"));
                true
            }
            _ => false,
        }
    }
}

/// The inputs that are no `ref` parameter which the `ref` result of the
/// node of `signature`, whose body is `body`, designates whole, each once,
/// in the order first designated: the node follows each (FN-16: "Readers
/// follow a later rebinding of that endpoint without waking the node for
/// it"). None for a node whose result is not `ref`.
fn followed(signature: &Signature, body: &Body) -> Vec<usize> {
    if !signature.reference {
        return Vec::new();
    }
    let mut found: Vec<usize> = Vec::new();
    for (local, how, _) in designations(&body.locals, Some(signature), &body.statements) {
        let reference = is_reference(&body.locals, Some(signature), local);
        if how == Designated::Whole && !reference && !found.contains(&local) {
            found.push(local);
        }
    }
    found
}

/// The C++ name of the `REF` input through which a node follows the
/// input `local`, of its locals.
fn follower(local: usize) -> String {
    names::temporary("designated", local)
}

/// What a node adds for the inputs that are no `ref` parameter which its
/// `ref` result designates (FN-16): each is given to it a second time, as
/// a `REF` input it is active on, and its state keeps which one the
/// result follows.
#[derive(Default)]
pub(super) struct Designations {
    /// Each input followed, in the order of [`followed`]: its local, and
    /// the `eval` parameter it is followed through.
    followed: Vec<(usize, String)>,
}

impl Designations {
    /// The inputs the node requires valid itself, of `valid`, those its
    /// handlers ask together (TIME-4). A node that follows an input is
    /// evaluated where that input's `REF` ticks, whatever else is valid,
    /// so none is the node's own: each handler is guarded by its own.
    pub(super) fn validity(&self, valid: Vec<usize>) -> Vec<usize> {
        if self.followed.is_empty() {
            valid
        } else {
            Vec::new()
        }
    }

    /// The parameters of the node's `eval` that follow a designation,
    /// given after the node's own.
    pub(super) fn parameters(&self) -> Vec<String> {
        self.followed
            .iter()
            .map(|(_, parameter)| parameter.clone())
            .collect()
    }

    /// The members of the node's state that keep a designation.
    pub(super) fn members(&self) -> String {
        if self.followed.is_empty() {
            return String::new();
        }
        format!(
            "{INDENT}{INDENT}// FN-16: the input the `ref` result follows, by its place among\n\
             {INDENT}{INDENT}// those it designates, or -1 for none.\n\
             {INDENT}{INDENT}::hgraph::Int {DESIGNATES}{{-1}};\n"
        )
    }

    /// True where the node keeps a designation in its state.
    pub(super) fn holds(&self) -> bool {
        !self.followed.is_empty()
    }
}

impl Emitter<'_> {
    /// True where the `ref` parameters and result of `signature`, a
    /// temporal function's of `phase` (a source's where `source`), are
    /// written; each that is not is reported, at `at` for the result.
    pub(super) fn references_written(
        &mut self,
        signature: &Signature,
        phase: Phase,
        source: bool,
        at: Location,
    ) -> bool {
        let marked = signature
            .parameters
            .iter()
            .find(|parameter| parameter.reference);
        // FN-16: "A value function, a graph, and an operator mark none",
        // which the analyser rejects (`ref.position`).
        if phase == Phase::Graph {
            if signature.reference {
                self.unsupported(at, "a `ref` result of a graph");
                return false;
            }
            if let Some(parameter) = marked {
                self.unsupported(parameter.at, "a `ref` parameter of a graph");
                return false;
            }
        }
        // SOURCE-1: a source names no temporal input, which a `ref`
        // result would designate; the analyser rejects its `yield`.
        if source && signature.reference {
            self.unsupported(at, "a `ref` result of a source");
            return false;
        }
        true
    }

    /// hgraph's schema of a node's temporal parameter or result of `ty`:
    /// its temporal form, and where `reference` (FN-16) hgraph's `REF` of
    /// it, `::hgraph::REF<::hgraph::TS<::hgraph::Int>>`. One whose type
    /// is not written is reported at `at`.
    pub(super) fn endpoint_schema(
        &mut self,
        ty: &Type,
        reference: bool,
        at: Location,
    ) -> Option<String> {
        let schema = self.temporal_schema(ty, at)?;
        Some(if reference {
            format!("::hgraph::REF<{schema}>")
        } else {
            schema
        })
    }

    /// The inputs that are no `ref` parameter which the `ref` result of
    /// the node of `signature`, whose body is `body` and which injects
    /// `injected`, designates (FN-16), as [`Designations`]. `None` where
    /// one is not written yet, which is reported.
    pub(super) fn designated_inputs(
        &mut self,
        signature: &Signature,
        body: &Body,
        injected: &[usize],
    ) -> Option<Designations> {
        if !signature.reference {
            return Some(Designations::default());
        }
        for (local, how, at) in designations(&body.locals, Some(signature), &body.statements) {
            if how == Designated::Position {
                // The list's own form first, so that a list not written
                // yet is reported as that.
                self.temporal_schema(&body.locals[local].ty, at)?;
                self.unsupported(at, "a designation of a position of a list");
                return None;
            }
        }
        let followed = followed(signature, body);
        if !followed.is_empty()
            && let Some(&timer) = injected
                .iter()
                .find(|&&local| Timer::of(&body.locals[local].name).is_some())
        {
            // What wakes the node beside its inputs is the timers' to
            // say: a follower's own wake runs no handler.
            self.unsupported(
                body.locals[timer].at,
                "a designation of an input in a node that injects a scheduler or an alarm",
            );
            return None;
        }
        let mut following = Vec::new();
        for local in followed {
            let declared = &body.locals[local];
            let schema = self.temporal_schema(&declared.ty, declared.at)?;
            // A label no HGL name is: hgraph names the node's inputs by
            // their labels.
            let label = names::string(&format!("designated:{}", declared.name));
            following.push((
                local,
                format!(
                    "::hgraph::In<{label}, ::hgraph::REF<{schema}>, ::hgraph::InputValidity::Unchecked> {}",
                    follower(local)
                ),
            ));
        }
        Some(Designations {
            followed: following,
        })
    }

    /// What a node's `eval` starts with where it follows the inputs of
    /// `designated`: the designation its state keeps, bound by reference;
    /// for the input it follows, where that input's `REF` ticks, what it
    /// designates now published (FN-16: "Readers follow a later rebinding
    /// of that endpoint"); and, where none of the node's own inputs woke
    /// it, the evaluation left before any handler ("without waking the
    /// node for it"). `body` is the node's, `inputs` its temporal inputs,
    /// `active` those a handler subscribes to.
    pub(super) fn designation_prologue(
        &self,
        designated: &Designations,
        body: &Body,
        inputs: &[usize],
        active: &[usize],
    ) -> String {
        if designated.followed.is_empty() {
            return String::new();
        }
        let indent = INDENT.repeat(2);
        let inner = INDENT.repeat(3);
        let mut text = format!(
            "{indent}// FN-16: the input the `ref` result follows, kept in the node's state.\n\
             {indent}::hgraph::Int& {DESIGNATES} = {STATE}.modify().{DESIGNATES};\n\
             {indent}// FN-16: \"Readers follow a later rebinding of that endpoint without\n\
             {indent}// waking the node for it\".\n"
        );
        for (place, (local, _)) in designated.followed.iter().enumerate() {
            let view = follower(*local);
            text.push_str(&format!(
                "{indent}if ({DESIGNATES} == {place} && {view}.modified())\n{indent}{{\n\
                 {inner}{OUT}.set(::{}::designation({view}));\n{indent}}}\n",
                runtime::NAMESPACE
            ));
        }
        // An input wakes the node where it is active and publishes, or is
        // invalidated (END-4: "The transition to invalid notifies active
        // subscribers"). Where the body changes an input's activity
        // (END-5), each input is asked whether it is active.
        let mut changes = false;
        hgl_analyser::hir::walk_statements(&body.statements, &mut |value| {
            changes |= matches!(
                value.kind,
                ExprKind::Endpoint {
                    operation: EndpointOperation::Activate | EndpointOperation::Passivate,
                    ..
                }
            );
        });
        let woken: Vec<String> = inputs
            .iter()
            .filter(|local| changes || active.contains(local))
            .map(|&local| {
                let input = identifier(&body.locals[local].name);
                if changes {
                    format!("(({input}.modified() || {input}.invalidated()) && {input}.active())")
                } else {
                    format!("{input}.modified() || {input}.invalidated()")
                }
            })
            .collect();
        let woken = if woken.is_empty() {
            "false".to_string()
        } else {
            woken.join(" || ")
        };
        text.push_str(&format!(
            "{indent}if (!({woken}))\n{indent}{{\n{inner}return;\n{indent}}}\n"
        ));
        text
    }

    /// The ports the wiring function of the node of `signature`, whose
    /// body is `body`, gives a second time, after its parameters: each
    /// input it follows, named as the wiring function's parameter is.
    pub(super) fn designated_ports(&self, signature: &Signature, body: &Body) -> Vec<String> {
        followed(signature, body)
            .into_iter()
            .map(|local| identifier(&body.locals[local].name))
            .collect()
    }

    /// `called`, what a wiring function gives of the node of `signature`,
    /// whose result is of `schema`, as the connection a caller is given:
    /// for a `ref` result, a connection of T (FN-16: "a `ref` result is a
    /// connection of T to whoever reads it"), which hgraph binds an
    /// ordinary reader of to what the result designates.
    pub(super) fn result_port(
        &self,
        signature: &Signature,
        schema: Option<&str>,
        called: String,
    ) -> String {
        match schema {
            Some(schema) if signature.reference => format!("{called}.as<{schema}>()"),
            _ => called,
        }
    }

    /// The statements, `depth` levels in, that make the result of the node
    /// of `signature`, whose body is `body`, designate `value`, what a
    /// `return` or an `out =` names (FN-16). `None` where it is not
    /// written yet, which is reported.
    fn designation(
        &mut self,
        signature: &Signature,
        body: &Body,
        value: &Expr,
        depth: usize,
    ) -> Option<String> {
        let indent = INDENT.repeat(depth);
        let following = followed(signature, body);
        let published = |view: &str| {
            format!(
                "{indent}{OUT}.set(::{}::designation({view}));\n",
                runtime::NAMESPACE
            )
        };
        match designated(&body.locals, Some(signature), value) {
            Some((local, Designated::Whole)) => {
                if is_reference(&body.locals, Some(signature), local) {
                    // FN-16: "Returning a whole `ref` parameter samples its
                    // current designation when the `return` executes ...
                    // the result keeps the sampled designation until the
                    // node writes its result again": nothing is followed.
                    let mut text = String::new();
                    if !following.is_empty() {
                        text.push_str(&format!("{indent}{DESIGNATES} = -1;\n"));
                    }
                    text.push_str(&published(&identifier(&body.locals[local].name)));
                    return Some(text);
                }
                // FN-16: "Returning an ordinary input ... designates the
                // logical endpoint named by the path", which the node
                // follows.
                let Some(place) = following.iter().position(|&followed| followed == local) else {
                    self.unsupported(value.at, "this designation");
                    return None;
                };
                Some(format!(
                    "{indent}{DESIGNATES} = {place};\n{}",
                    published(&follower(local))
                ))
            }
            Some((_, Designated::Position)) => {
                self.unsupported(value.at, "a designation of a position of a list");
                None
            }
            None => {
                self.unsupported(value.at, "a designation of this child");
                None
            }
        }
    }
}
