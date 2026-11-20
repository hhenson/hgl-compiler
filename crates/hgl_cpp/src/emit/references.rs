//! The `ref` mode of a node's parameters and result (FN-16, TIME-10;
//! core's END-10): the family of the dispatch point (`forms`) that writes
//! them, and what a node's struct and wiring function take of them. Part
//! of the transpiler, the last stage.
//!
//! Not written yet, and reported: a `ref` parameter or result.

use hgl_analyser::hir::Body;
use hgl_analyser::{Phase, Signature, Type};
use hgl_diagnostics::Location;

use super::Emitter;
use super::forms::Family;

/// The references' family.
pub(super) struct References;

impl Family for References {}

/// What a node adds for the ordinary inputs its `ref` result designates
/// (FN-16): none yet.
#[derive(Default)]
pub(super) struct Designations;

impl Designations {
    /// The inputs the node requires valid itself, of `valid`, those its
    /// handlers ask together (TIME-4).
    pub(super) fn validity(&self, valid: Vec<usize>) -> Vec<usize> {
        valid
    }

    /// The parameters of the node's `eval` that follow a designation.
    pub(super) fn parameters(&self) -> Vec<String> {
        Vec::new()
    }

    /// The members of the node's state that keep a designation.
    pub(super) fn members(&self) -> String {
        String::new()
    }

    /// True where the node keeps a designation in its state.
    pub(super) fn holds(&self) -> bool {
        false
    }
}

impl Emitter<'_> {
    /// True where the `ref` parameters and result of `signature`, a
    /// temporal function's of `phase` (a source's where `source`), are
    /// written; each that is not is reported, at `at` for the result.
    pub(super) fn references_written(
        &mut self,
        signature: &Signature,
        _phase: Phase,
        _source: bool,
        at: Location,
    ) -> bool {
        if signature.reference {
            self.unsupported(at, "a `ref` result");
            return false;
        }
        if let Some(parameter) = signature
            .parameters
            .iter()
            .find(|parameter| parameter.reference)
        {
            self.unsupported(parameter.at, "a `ref` parameter");
            return false;
        }
        true
    }

    /// hgraph's schema of a node's temporal parameter or result of `ty`,
    /// marked `ref` where `reference` (FN-16); one not written is
    /// reported at `at`.
    pub(super) fn endpoint_schema(
        &mut self,
        ty: &Type,
        _reference: bool,
        at: Location,
    ) -> Option<String> {
        self.temporal_schema(ty, at)
    }

    /// The ordinary inputs the `ref` result of the node of `signature`,
    /// whose body is `body` and which injects `injected`, designates
    /// (FN-16). `None` where one is not written yet, which is reported.
    pub(super) fn designated_inputs(
        &mut self,
        _signature: &Signature,
        _body: &Body,
        _injected: &[usize],
    ) -> Option<Designations> {
        Some(Designations)
    }

    /// What a node's `eval` starts with for `designated`: none yet.
    pub(super) fn designation_prologue(
        &mut self,
        _designated: &Designations,
        _body: &Body,
        _inputs: &[usize],
        _active: &[usize],
    ) -> String {
        String::new()
    }

    /// The ports the wiring function of the node of `signature` gives a
    /// second time, after its parameters: none yet.
    pub(super) fn designated_ports(&self, _signature: &Signature, _body: &Body) -> Vec<String> {
        Vec::new()
    }

    /// `called`, what a wiring function gives of the node of `signature`,
    /// whose result is of `schema`, as the connection a caller is given.
    pub(super) fn result_port(
        &self,
        _signature: &Signature,
        _schema: Option<&str>,
        called: String,
    ) -> String {
        called
    }
}
