//! Writes core's growing ordered reduction (RED-5) in the C++ target.
//! The selected HGL callable becomes the graph of one guarded fold step.

use hgl_analyser::LocalKind;
use hgl_analyser::hir::{Body, Callee, Expr, ExprKind, Local, ValueCategory};
use hgl_lower::calls::OrderedReduction;

use super::Emitter;
use super::anonymous::anonymous_arguments;
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
        let mut given: Vec<Given> = [accumulator, member]
            .into_iter()
            .zip(["accumulator_", "member_"])
            .enumerate()
            .map(|(position, (parameter, written))| Given {
                position,
                written: written.to_string(),
                input: Some(parameter.value.ty.clone()),
            })
            .collect();
        let mut capture_types = Vec::new();
        let mut capture_values = Vec::new();
        let mut combiner = std::borrow::Cow::Borrowed(&reduction.combiner);
        if let Callee::Anonymous(function) = callee
            && !function.captures.is_empty()
        {
            // FN-14: read captures in the enclosing scope. The tuple is
            // only wiring-time data. Its ports stay connections in the
            // callback and are bound by the compiled switch and reduce.
            let Some(bound) = anonymous_arguments(scope, function, parameters) else {
                return self.missing(at, "an ordered reduction with no enclosing capture");
            };
            for (index, capture) in bound[parameters.len()..].iter().enumerate() {
                let value = &capture.value;
                let local = &function.body.locals[parameters.len() + index];
                let (ty, connection) = if reduction.combiner.is_lifted() {
                    (&value.ty, value.is_connection())
                } else {
                    (&local.ty, local.category != ValueCategory::Value)
                };
                let (written_type, written, input) = if connection {
                    let Some(schema) = self.temporal_schema(ty, value.at) else {
                        return "0".to_string();
                    };
                    (
                        format!("::hgraph::Port<{schema}>"),
                        self.connection(scope, value, ty),
                        Some(ty.clone()),
                    )
                } else {
                    let Some(cpp) = self.written_type(ty, value.at) else {
                        return "0".to_string();
                    };
                    (cpp, self.expression(scope, value), None)
                };
                capture_types.push(written_type);
                capture_values.push(written);
                given.push(Given {
                    position: parameters.len() + index,
                    written: format!("::std::get<{index}>(captures_.value())"),
                    input,
                });
            }
            let ExprKind::Call { arguments, .. } = &mut combiner.to_mut().kind else {
                unreachable!("the ordered combiner is a call");
            };
            *arguments = bound;
        }
        let call = if combiner.is_lifted() {
            self.lifted(&mut callback_scope, &combiner, &given, &[], Some(&value.ty))
        } else {
            let given: Vec<_> = given
                .into_iter()
                .map(|given| (given.position, given.written))
                .collect();
            self.temporal_call_in_form(
                &mut callback_scope,
                &combiner,
                HeldInputs {
                    given: &given,
                    ..HeldInputs::default()
                },
                ResultForm::Whole,
            )
        };
        let (capture_definition, capture_parameter, capture_argument) = if capture_types.is_empty()
        {
            (String::new(), String::new(), String::new())
        } else {
            (
                format!(
                    "{INDENT}using captures_type_ = ::std::tuple<{}>;\n",
                    capture_types.join(", ")
                ),
                format!(
                    ",\n{INDENT}{INDENT}::hgraph::Scalar<\"captures\", captures_type_> captures_"
                ),
                format!(", {name}::captures_type_{{{}}}", capture_values.join(", ")),
            )
        };
        let definition = format!(
            "// RED-5: the callback for one valid member of the ordered fold.\n\
             struct {name}\n{{\n\
             {capture_definition}{INDENT}static ::hgraph::Port<{accumulator_schema}> compose(\n\
             {INDENT}{INDENT}::hgraph::Wiring& {WIRING}, ::hgraph::Port<{accumulator_schema}> accumulator_,\n\
             {INDENT}{INDENT}::hgraph::Port<{member_schema}> member_{capture_parameter})\n\
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
            "[&] {{ {steps}return ::{runtime}::ordered_list_reduce<{name}>({WIRING}, items_, zero_{capture_argument}); }}()"
        )
    }
}
