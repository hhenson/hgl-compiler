//! Writes the anonymous function behind a checked static argument
//! (FN-14, FN-17). Its identity selects code at compile time and only
//! its ordinary arguments and captures reach the helper it calls.

use hgl_analyser::hir::{AnonymousFunction, Argument, Expr, ExprKind, ValueCategory};
use hgl_analyser::{Parameter, Signature};

use super::Emitter;
use super::names;
use super::statements::Scope;

impl Emitter<'_> {
    /// Give an invoked function a private C++ helper in the current
    /// piece. Each invocation binds its lexical captures as extra
    /// ordinary parameters, without a runtime callable object.
    pub(super) fn anonymous_helper(
        &mut self,
        function: &AnonymousFunction,
        signature: &Signature,
    ) -> Option<String> {
        let name = names::temporary("anonymous", self.anonymous_number);
        self.anonymous_number += 1;
        // The anonymous body owns its locals and flow. The outer node,
        // graph, or test resumes its own emission after the helper.
        let outer = self.temporal.take();
        let control = std::mem::take(&mut self.control);
        let definition = if function.template.value {
            self.anonymous_value(function, signature, &name)
        } else {
            self.graph_definition(&name, signature, &function.body, function.at)
        };
        self.temporal = outer;
        self.control = control;
        self.ahead
            .push(format!("namespace {{\n{}}}\n", definition?));
        if function.template.value {
            Some(name)
        } else {
            Some(format!("{name}::compose"))
        }
    }

    /// A private C++ function with value return and failure handling.
    fn anonymous_value(
        &mut self,
        function: &AnonymousFunction,
        signature: &Signature,
        name: &str,
    ) -> Option<String> {
        let (_, head) = self.head(name, signature, function.at)?;
        let mut text = format!("{head}\n{{\n");
        self.value_body(
            &function.body,
            signature.result.as_ref(),
            function.at,
            &mut text,
        )?;
        text.push_str("}\n");
        Some(text)
    }
}

/// The helper's parameters are the explicit inputs followed by those
/// supplied from the lexical scope. A graph's scalar capture is taken
/// as a value and a temporal capture as its connection (FN-3, FN-14).
pub(super) fn anonymous_signature(function: &AnonymousFunction) -> Signature {
    let inputs = function.template.parameters.len();
    let parameters = function
        .body
        .locals
        .iter()
        .take(inputs + function.captures.len())
        .enumerate()
        .map(|(index, local)| Parameter {
            name: local.name.clone(),
            at: local.at,
            constant: !function.template.value
                && index >= inputs
                && local.category == ValueCategory::Value,
            var: false,
            reference: false,
            ty: local.ty.clone(),
            pack: None,
            pack_bounds: (0, None),
            has_default: false,
        })
        .collect();
    Signature {
        generics: Vec::new(),
        parameters,
        result: function.template.result.clone(),
        reference: function.template.reference,
    }
}

/// The captures as expressions of the caller. A generated lifted node
/// already supplies its own held captures, in the same parameter slots.
pub(super) fn anonymous_arguments(
    scope: &Scope,
    function: &AnonymousFunction,
    arguments: &[Argument],
) -> Option<Vec<Argument>> {
    let mut bound = arguments.to_vec();
    for (index, &capture) in function.captures.iter().enumerate() {
        let parameter = Some(function.template.parameters.len() + index);
        if arguments
            .iter()
            .any(|argument| argument.parameter == parameter)
        {
            continue;
        }
        let local = scope.body.locals.get(capture)?;
        bound.push(Argument {
            parameter,
            pack: None,
            value: Expr {
                kind: ExprKind::Local(capture),
                at: function.at,
                category: local.category,
                ty: local.ty.clone(),
            },
        });
    }
    Some(bound)
}
