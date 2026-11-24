//! Writes core's growing ordered reduction (RED-5) in the C++ target.
//! The selected HGL callable becomes the graph of one guarded fold step.

use hgl_analyser::LocalKind;
use hgl_analyser::hir::{Body, Callee, Expr, ExprKind, Local};
use hgl_lower::calls::OrderedReduction;

use super::Emitter;
use super::graphs::{HeldInputs, WIRING, widened_from};
use super::lifting::Given;
use super::names;
use super::nodes::ResultForm;
use super::statements::Scope;
use super::values::INDENT;

impl Emitter<'_> {
    /// Wire the list and seed once, and the selected binary function as a child graph.
    pub(super) fn ordered_reduction(
        &mut self,
        scope: &mut Scope,
        value: &Expr,
        reduction: &OrderedReduction,
        held: HeldInputs<'_>,
    ) -> String {
        let ExprKind::Call { arguments, .. } = &value.kind else {
            return self.missing(value.at, "this ordered reduction");
        };
        let ExprKind::Call {
            callee,
            arguments: parameters,
            ..
        } = &reduction.combiner.kind
        else {
            return self.missing(value.at, "an ordered reduction without a combiner");
        };
        if matches!(callee, Callee::Anonymous(function) if !function.captures.is_empty()) {
            return self.missing(
                reduction.combiner.at,
                "an ordered reduction with a captured callback",
            );
        }
        let [accumulator, member] = parameters.as_slice() else {
            return self.missing(value.at, "an ordered reduction without two combiner inputs");
        };
        let at = reduction.combiner.at;
        let Some(accumulator_schema) = self.temporal_schema(&accumulator.value.ty, at) else {
            return "0".to_string();
        };
        let Some(member_schema) = self.temporal_schema(&member.value.ty, at) else {
            return "0".to_string();
        };
        let callback_body = Body {
            locals: [accumulator, member]
                .into_iter()
                .zip(["accumulator", "member"])
                .map(|(input, name)| Local {
                    at,
                    name: name.to_string(),
                    kind: LocalKind::Parameter,
                    ty: input.value.ty.clone(),
                    category: input.value.category,
                })
                .collect(),
            statements: Vec::new(),
        };
        let mut callback_scope = Scope::cold(self.program, &callback_body);
        let name = names::temporary("combiner", self.temporary());
        let given = [(0, "accumulator_".to_string()), (1, "member_".to_string())];
        let call = if reduction.combiner.is_lifted() {
            let ports: Vec<Given> = given
                .into_iter()
                .zip([accumulator, member])
                .map(|((position, written), parameter)| Given {
                    position,
                    written,
                    input: Some(parameter.value.ty.clone()),
                })
                .collect();
            self.lifted(
                &mut callback_scope,
                &reduction.combiner,
                &ports,
                &[],
                Some(&value.ty),
            )
        } else {
            self.temporal_call_in_form(
                &mut callback_scope,
                &reduction.combiner,
                HeldInputs {
                    given: &given,
                    ..HeldInputs::default()
                },
                ResultForm::Whole,
            )
        };
        let definition = format!(
            "// RED-5: the callback for one valid member of the ordered fold.\n\
             struct {name}\n{{\n\
             {INDENT}static ::hgraph::Port<{accumulator_schema}> compose(\n\
             {INDENT}{INDENT}::hgraph::Wiring& {WIRING}, ::hgraph::Port<{accumulator_schema}> accumulator_,\n\
             {INDENT}{INDENT}::hgraph::Port<{member_schema}> member_)\n\
             {INDENT}{{\n{INDENT}{INDENT}return {call};\n{INDENT}}}\n}};\n"
        );
        let Some(context) = self.temporal.as_mut() else {
            return self.missing(
                value.at,
                "an ordered reduction outside temporal composition",
            );
        };
        context.lifted.push(definition);

        // EXPR-2: keep the source order even when the two inputs are named.
        let [items, zero] = reduction.inputs;
        let mut steps = String::new();
        for (position, argument) in arguments.iter().enumerate() {
            let (bound, expected) = if position == items {
                ("items_", &argument.value.ty)
            } else if position == zero {
                ("zero_", &accumulator.value.ty)
            } else {
                continue;
            };
            let port = held.given.iter().find(|(source, _)| *source == position);
            let input = match port {
                // TYPE-14: a held seed also uses the selected accumulator's nullable schema.
                Some((_, port)) if widened_from(&argument.value.ty, expected).is_some() => {
                    format!("({port}).as<{accumulator_schema}>()")
                }
                Some((_, port)) => port.clone(),
                None => self.connection(scope, &argument.value, expected),
            };
            steps.push_str(&format!("const auto {bound} = {input}; "));
        }
        let runtime = crate::runtime::NAMESPACE;
        format!(
            "[&] {{ {steps}return ::{runtime}::ordered_list_reduce<{name}>({WIRING}, items_, zero_); }}()"
        )
    }
}
