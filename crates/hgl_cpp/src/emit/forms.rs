//! The dispatch point of the temporal forms: where the emitter looks up
//! how a form is written, and the one list of the families that write
//! them. Part of the transpiler, the last stage.
//!
//! A *family* is a part of the target that writes some forms: the
//! scalars' temporal form (`scalars`), the endpoints' reads and natives
//! (`endpoints`), a node's hooks (`nodes`), a source's body (`sources`),
//! a graph's wiring (`graphs`), a test's evals (`tests`). Each is a
//! [`Family`], whose methods each answer one lookup, and say nothing
//! (`None`, `false`) of a form that is not the family's:
//!
//! | Lookup | What it answers |
//! | --- | --- |
//! | a type's temporal form | [`Family::schema`], hgraph's schema of a temporal value of the type; [`Family::read`], a hook's read of its payload; [`Family::publish`], a publication of a value of it; [`Family::delta`], its `delta_value`; [`Family::constant`], a value of it wired as a connection (FN-3) |
//! | a type's trace cell | [`Family::cell`], one cell of a trace written out (TST-5) |
//! | a statement | [`Family::statement`], a statement of a hook, a source's body or a graph |
//! | an expression | [`Family::expression`], an expression of a hook, a source's body, a test or value code |
//! | a wiring form | [`Family::wiring`], an expression of a graph that is a connection, as the `hgraph::Port` it gives |
//!
//! [`FAMILIES`] lists them, and each lookup asks them in that order: the
//! first that answers writes the form. A construct lane adds a family by
//! adding its file under `emit/` (its `mod` line in `emit.rs`) and one
//! entry in [`FAMILIES`], ahead of the spine's, so that it answers for
//! its forms first. A form no family answers for is reported, `the cpp
//! target does not write ... yet`, and is never written wrongly: a type
//! lookup reports it here, and a statement or an expression of a hook
//! falls to the value code's writer (`statements`, `expressions`), which
//! reports each form it does not write.

use hgl_analyser::Type;
use hgl_analyser::hir::{Expr, Stmt};
use hgl_diagnostics::Location;

use super::Emitter;
use super::statements::Scope;

/// A part of the target that writes some forms. Each method answers one
/// lookup of the dispatch point, and gives `None` (or `false`) for a
/// form that is not the family's; one that is the family's and that it
/// cannot write is reported by the family and given a stand-in, as
/// `Emitter::missing` gives one.
pub(super) trait Family: Sync {
    /// hgraph's schema of a temporal value of `ty`, qualified from the
    /// global namespace: `::hgraph::TS<::hgraph::Int>` for an `i64`.
    fn schema(&self, _emitter: &mut Emitter<'_>, _ty: &Type) -> Option<String> {
        None
    }

    /// The read of the payload of the endpoint `view`, a temporal value of
    /// `ty`, in a hook (TIME-1): checked when it runs.
    fn read(&self, _emitter: &mut Emitter<'_>, _ty: &Type, _view: &str) -> Option<String> {
        None
    }

    /// The statement that publishes `value`, the C++ of a value of `ty`
    /// or of a `delta<T>` of it, to the output view `view` (TIME-7).
    fn publish(
        &self,
        _emitter: &mut Emitter<'_>,
        _ty: &Type,
        _view: &str,
        _value: &str,
    ) -> Option<String> {
        None
    }

    /// `delta_value` of the endpoint `view`, a temporal value of `ty`
    /// (END-3): where `checked`, the read whose place takes a `result`,
    /// and otherwise the plain read (ERR-14).
    fn delta(
        &self,
        _emitter: &mut Emitter<'_>,
        _ty: &Type,
        _view: &str,
        _checked: bool,
    ) -> Option<String> {
        None
    }

    /// `value`, the C++ of an ordinary value of `ty`, wired as a constant
    /// connection on the wiring `wiring_` (FN-3).
    fn constant(&self, _emitter: &mut Emitter<'_>, _ty: &Type, _value: &str) -> Option<String> {
        None
    }

    /// One cell of a trace of `ty` written out, whose value is `value`
    /// (TST-5: "written as a `return` is").
    fn cell(
        &self,
        _emitter: &mut Emitter<'_>,
        _scope: &mut Scope<'_>,
        _ty: &Type,
        _value: &Expr,
    ) -> Option<String> {
        None
    }

    /// Writes `statement`, `depth` levels in, into `text`, where it is
    /// the family's: true where it did.
    fn statement(
        &self,
        _emitter: &mut Emitter<'_>,
        _scope: &mut Scope<'_>,
        _statement: &Stmt,
        _depth: usize,
        _text: &mut String,
    ) -> bool {
        false
    }

    /// `value` as a C++ expression, where it is the family's.
    fn expression(
        &self,
        _emitter: &mut Emitter<'_>,
        _scope: &mut Scope<'_>,
        _value: &Expr,
    ) -> Option<String> {
        None
    }

    /// `value`, an expression of a graph that is a connection, as the
    /// C++ of the `hgraph::Port` it gives, where it is the family's.
    fn wiring(
        &self,
        _emitter: &mut Emitter<'_>,
        _scope: &mut Scope<'_>,
        _value: &Expr,
    ) -> Option<String> {
        None
    }
}

/// Every family, in the order each lookup asks them. A construct lane
/// puts its own ahead of the spine's.
static FAMILIES: &[&dyn Family] = &[
    &super::references::References,
    &super::scalars::Scalars,
    &super::tests::Tests,
    &super::sources::Sources,
    &super::nodes::Nodes,
    &super::graphs::Graphs,
    &super::endpoints::Endpoints,
];

impl Emitter<'_> {
    /// hgraph's schema of a temporal value of `ty` ([`Family::schema`]);
    /// one no family writes is reported at `at`.
    pub(super) fn temporal_schema(&mut self, ty: &Type, at: Location) -> Option<String> {
        let found = FAMILIES.iter().find_map(|family| family.schema(self, ty));
        if found.is_none() {
            self.unsupported(at, &format!("a temporal value of `{ty}`"));
        }
        found
    }

    /// The read of the payload of `view`, of `ty`, in a hook
    /// ([`Family::read`]); one no family writes is reported at `at`.
    pub(super) fn temporal_read(&mut self, ty: &Type, view: &str, at: Location) -> String {
        match FAMILIES
            .iter()
            .find_map(|family| family.read(self, ty, view))
        {
            Some(read) => read,
            None => self.missing(at, &format!("a read of a temporal value of `{ty}`")),
        }
    }

    /// The publication of `value` to `view`, of `ty` ([`Family::publish`]);
    /// one no family writes is reported at `at`.
    pub(super) fn temporal_publish(
        &mut self,
        ty: &Type,
        view: &str,
        value: &str,
        at: Location,
    ) -> String {
        match FAMILIES
            .iter()
            .find_map(|family| family.publish(self, ty, view, value))
        {
            Some(publish) => publish,
            None => {
                self.unsupported(at, &format!("a publication of `{ty}`"));
                String::new()
            }
        }
    }

    /// `delta_value` of `view`, of `ty` ([`Family::delta`]); one no
    /// family writes is reported at `at`.
    pub(super) fn temporal_delta(
        &mut self,
        ty: &Type,
        view: &str,
        checked: bool,
        at: Location,
    ) -> String {
        match FAMILIES
            .iter()
            .find_map(|family| family.delta(self, ty, view, checked))
        {
            Some(delta) => delta,
            None => self.missing(at, &format!("the delta of `{ty}`")),
        }
    }

    /// `value` of `ty` wired as a constant connection
    /// ([`Family::constant`]); one no family writes is reported at `at`.
    pub(super) fn temporal_constant(&mut self, ty: &Type, value: &str, at: Location) -> String {
        match FAMILIES
            .iter()
            .find_map(|family| family.constant(self, ty, value))
        {
            Some(constant) => constant,
            None => self.missing(at, &format!("a constant connection of `{ty}`")),
        }
    }

    /// One cell of a trace of `ty` ([`Family::cell`]); one no family
    /// writes is reported at `value`.
    pub(super) fn trace_cell(&mut self, scope: &mut Scope, ty: &Type, value: &Expr) -> String {
        match FAMILIES
            .iter()
            .find_map(|family| family.cell(self, scope, ty, value))
        {
            Some(cell) => cell,
            None => self.missing(value.at, &format!("a trace of `{ty}`")),
        }
    }

    /// Writes `statement` where a family writes it ([`Family::statement`]):
    /// true where one did. The value code's writer writes the rest.
    pub(super) fn form_statement(
        &mut self,
        scope: &mut Scope,
        statement: &Stmt,
        depth: usize,
        text: &mut String,
    ) -> bool {
        FAMILIES
            .iter()
            .any(|family| family.statement(self, scope, statement, depth, text))
    }

    /// `value` where a family writes it ([`Family::expression`]). The
    /// value code's writer writes the rest.
    pub(super) fn form_expression(&mut self, scope: &mut Scope, value: &Expr) -> Option<String> {
        FAMILIES
            .iter()
            .find_map(|family| family.expression(self, scope, value))
    }

    /// `value`, a connection of a graph, as the port it gives
    /// ([`Family::wiring`]); one no family writes is reported.
    pub(super) fn wiring_form(&mut self, scope: &mut Scope, value: &Expr) -> String {
        match FAMILIES
            .iter()
            .find_map(|family| family.wiring(self, scope, value))
        {
            Some(port) => port,
            None => self.missing(value.at, unwired(value)),
        }
    }
}

/// What a connection no family wires is reported as: its form, so that
/// the case list says which lane's it is.
fn unwired(value: &Expr) -> &'static str {
    use hgl_analyser::hir::ExprKind;
    match &value.kind {
        ExprKind::Capture(_) => "a `capture`",
        ExprKind::Captured { .. } => "a read of a capture",
        ExprKind::If { .. } => "an `if` that gives a connection",
        ExprKind::Block { .. } => "a block that gives a connection",
        ExprKind::Field { .. } => "a field of a connection",
        ExprKind::Index { .. } => "a child of a connection",
        ExprKind::Widen(_) => "a connection of `i64` taken as `f64`",
        ExprKind::Call { .. } => "this call of connections",
        ExprKind::Construct { .. } | ExprKind::Tuple(_) | ExprKind::Sequence(_) => {
            "a value made of connections"
        }
        _ => "this connection",
    }
}
