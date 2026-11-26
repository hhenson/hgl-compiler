//! Writes result-bearing keyed map over split dictionaries (MAP-1, MAP-2).
//! hgraph owns child lifetime. The keyed publisher exposes the children's publications (MAP-7, MAP-10).

use hgl_analyser::hir::{Body, Callee, Expr, ExprKind, Local};
use hgl_analyser::{LocalKind, Type};
use hgl_diagnostics::Location;
use hgl_lower::calls::{KeyedMap, KeyedMapInput, KeyedMapProblem};

use super::Emitter;
use super::anonymous::anonymous_arguments;
use super::configuration;
use super::graphs::{HeldInputs, WIRING};
use super::lifting::Given;
use super::names;
use super::nodes::ResultForm;
use super::statements::Scope;
use super::values::INDENT;

enum ParentArgument {
    Port,
    Scalar(String),
}

impl Emitter<'_> {
    /// Wire the split dictionaries and whole arguments to the callback of one child.
    pub(super) fn keyed_map(
        &mut self,
        scope: &mut Scope,
        value: &Expr,
        map: Result<KeyedMap, KeyedMapProblem>,
        held: HeldInputs<'_>,
    ) -> String {
        let map = match map {
            Ok(map) => map,
            Err(KeyedMapProblem::ConstantKey(at)) => {
                return self.missing(at, "a keyed map with a const key parameter");
            }
            Err(KeyedMapProblem::FunctionArgument(at)) => {
                return self.missing(at, "a keyed map passing a function argument to its child");
            }
        };
        let ExprKind::Call { arguments, .. } = &value.kind else {
            return self.missing(value.at, "this keyed map");
        };
        if let Some(keys) = map.keys
            && !matches!(arguments[keys].value.ty, Type::Set(_))
        {
            return self.missing(
                arguments[keys].value.at,
                "a keyed map with a nullable keys connection",
            );
        }

        let mut callback = map.callback;
        let ExprKind::Call {
            callee,
            arguments: bound,
            lifted,
            ..
        } = &mut callback.kind
        else {
            return self.missing(value.at, "a keyed map without a callback");
        };
        if let Callee::Anonymous(function) = callee {
            let Some(with_captures) = anonymous_arguments(scope, function, bound) else {
                return self.missing(function.at, "a keyed map whose callback capture is unbound");
            };
            *bound = with_captures;
        }
        let lifted = *lifted;

        let name = names::temporary("keyed_map", self.temporary());
        let mut parent_arguments: Vec<Option<ParentArgument>> =
            arguments.iter().map(|_| None).collect();
        let mut runtime_parameters = Vec::new();
        let mut scalar_parameters = Vec::new();
        let mut input_ports = Vec::new();
        let mut scalars = Vec::new();
        let mut captures = String::new();
        let mut key_parameter = None;
        let mut split = Vec::new();
        let mut given = Vec::new();
        for (position, argument) in bound.iter().enumerate() {
            let child_name = format!("input_{position}_");
            let parent_name = |source| format!("map_argument_{source}_");
            let ty = &argument.value.ty;
            let at = argument.value.at;
            let binding = map.inputs.get(position).copied();
            let scalar = matches!(binding, Some(KeyedMapInput::Configuration(_)))
                || binding.is_none() && !argument.value.is_connection();
            if scalar {
                let Some((parameter, written, read)) = self.map_scalar(ty, at, &child_name) else {
                    return "0".to_string();
                };
                let parent = match binding {
                    Some(KeyedMapInput::Configuration(source)) => {
                        parent_arguments[source] = Some(ParentArgument::Scalar(written.clone()));
                        parent_name(source)
                    }
                    _ => {
                        let parent = format!("map_capture_{position}_");
                        let expression = self.expression(scope, &argument.value);
                        captures.push_str(&format!("const {written} {parent} = {expression}; "));
                        parent
                    }
                };
                scalars.push(configuration::given(self.program, ty, &written, &parent));
                scalar_parameters.push(parameter);
                given.push(Given {
                    position,
                    written: read,
                    input: None,
                });
                continue;
            }

            let Some(schema) = self.temporal_schema(ty, at) else {
                return "0".to_string();
            };
            let parameter = format!("::hgraph::Port<{schema}> {child_name}");
            match binding {
                Some(KeyedMapInput::Key) => key_parameter = Some(parameter),
                Some(KeyedMapInput::Split(source) | KeyedMapInput::Whole(source)) => {
                    parent_arguments[source] = Some(ParentArgument::Port);
                    if matches!(binding, Some(KeyedMapInput::Split(_))) {
                        split.push(input_ports.len());
                    }
                    runtime_parameters.push(parameter);
                    input_ports.push(format!("{}.erased()", parent_name(source)));
                }
                None => {
                    let parent = format!("map_capture_{position}_");
                    let port = self.connection(scope, &argument.value, ty);
                    captures.push_str(&format!("const auto {parent} = {port}; "));
                    runtime_parameters.push(parameter);
                    input_ports.push(format!("{parent}.erased()"));
                }
                Some(KeyedMapInput::Configuration(_)) => unreachable!(),
            }
            given.push(Given {
                position,
                written: child_name,
                input: Some(ty.clone()),
            });
        }
        if split.is_empty() {
            return self.missing(value.at, "a keyed map without a split dictionary");
        }
        let local_body = Body {
            locals: bound
                .iter()
                .enumerate()
                .map(|(position, arg)| Local {
                    at: arg.value.at,
                    name: format!("input_{position}_"),
                    kind: LocalKind::Parameter,
                    ty: arg.value.ty.clone(),
                    category: arg.value.category,
                })
                .collect(),
            statements: Vec::new(),
        };
        let mut callback_scope = Scope::cold(self.program, &local_body);
        let call = if lifted {
            self.lifted(
                &mut callback_scope,
                &callback,
                &given,
                &[],
                Some(&callback.ty),
            )
        } else {
            let given: Vec<_> = given
                .into_iter()
                .map(|arg| (arg.position, arg.written))
                .collect();
            self.temporal_call_in_form(
                &mut callback_scope,
                &callback,
                HeldInputs {
                    given: &given,
                    ..HeldInputs::default()
                },
                ResultForm::Whole,
            )
        };
        let Some(result) = self.temporal_schema(&value.ty, value.at) else {
            return "0".to_string();
        };
        let Some(child_result) = self.temporal_schema(&callback.ty, callback.at) else {
            return "0".to_string();
        };
        // MAP-8: a lifecycle key can outlive one of its split inputs.
        let call = if map.keys.is_some() || split.len() > 1 {
            format!("::hglrt::keyed_map_child_result({WIRING}, {call})")
        } else {
            call
        };
        let key_policy = if key_parameter.is_some() {
            "First"
        } else {
            "None"
        };
        let mut parameters = vec![format!("::hgraph::Wiring& {WIRING}")];
        parameters.extend(key_parameter);
        parameters.extend(runtime_parameters);
        parameters.extend(scalar_parameters);
        let Some(context) = self.temporal.as_mut() else {
            return self.missing(value.at, "a keyed map outside temporal composition");
        };
        context.lifted.push(format!(
            "// MAP-9: hgraph supplies the key first, whatever its callback slot.\n\
             struct {name}\n{{\n\
             {INDENT}static ::hgraph::Port<{child_result}> compose({})\n\
             {INDENT}{{\n{INDENT}{INDENT}return {call};\n{INDENT}}}\n}};\n",
            parameters.join(", ")
        ));

        // EXPR-2: hold the arguments in their written order before reordering ports.
        if let Some(keys) = map.keys {
            parent_arguments[keys] = Some(ParentArgument::Port);
        }
        // MAP-5: the split ports are already in the callback's parameter order.
        let keys = if let Some(keys) = map.keys {
            format!("map_argument_{keys}_.erased()")
        } else if split.len() == 1 {
            "::std::nullopt".to_string()
        } else {
            let join = if let Some(join) = map.join {
                let written = &arguments[join].value;
                let Some(ty) = self.written_type(&written.ty, written.at) else {
                    return "0".to_string();
                };
                parent_arguments[join] = Some(ParentArgument::Scalar(ty.clone()));
                format!(
                    "map_argument_{join}_ == {ty}::left ? ::hglrt::MapKeyJoin::Left : \
                     map_argument_{join}_ == {ty}::outer ? ::hglrt::MapKeyJoin::Outer : \
                     ::hglrt::MapKeyJoin::Inner"
                )
            } else {
                "::hglrt::MapKeyJoin::Inner".to_string()
            };
            format!("::hglrt::joined_map_keys({WIRING}, inputs_, split_, {join})")
        };
        let mut wiring = String::new();
        for (source, (argument, mode)) in arguments.iter().zip(parent_arguments).enumerate() {
            let Some(mode) = mode else { continue };
            let held = held
                .given
                .iter()
                .find(|(index, _)| *index == source)
                .map(|(_, written)| written);
            let (written_type, expression) = match mode {
                ParentArgument::Port => (
                    "auto".to_string(),
                    match held {
                        Some(written) => written.clone(),
                        None => self.connection(scope, &argument.value, &argument.value.ty),
                    },
                ),
                ParentArgument::Scalar(written_type) => (
                    written_type,
                    match held {
                        Some(written) => written.clone(),
                        None => self.expression(scope, &argument.value),
                    },
                ),
            };
            wiring.push_str(&format!(
                "const {written_type} map_argument_{source}_ = {expression}; "
            ));
        }
        let mut compile = vec![WIRING.to_string()];
        compile.extend(scalars);
        format!(
            "[&] {{ {wiring}{captures}\
             const ::std::array<::hgraph::WiringPortRef, {}> inputs_{{{}}}; \
             constexpr ::std::array<::std::size_t, {}> split_{{{}}}; \
             const ::hgraph::Port<{result}> children_{{{WIRING}, ::hgraph::compiled::map_(\
             {WIRING}, inputs_, split_, ::hgraph::compile_subgraph<{name}>({}), \
             ::hgraph::MapKeyArg::{key_policy}, {keys})}}; \
             return ::hglrt::published_map_entries({WIRING}, children_); }}()",
            input_ports.len(),
            input_ports.join(", "),
            split.len(),
            split
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(", "),
            compile.join(", "),
        )
    }

    fn map_scalar(
        &mut self,
        ty: &Type,
        at: Location,
        name: &str,
    ) -> Option<(String, String, String)> {
        let written = self.written_type(ty, at)?;
        let held = configuration::held(self.program, ty, &written);
        let label = names::string(name);
        let parameter = format!("::hgraph::Scalar<{label}, {held}> {name}");
        let read = configuration::read(self.program, ty, name);
        Some((parameter, written, read))
    }
}
