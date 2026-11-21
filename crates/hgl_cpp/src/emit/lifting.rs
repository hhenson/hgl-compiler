//! Lifting and capture: a value function called with connections, which
//! is wired as a node of its own (FN-6, FN-11), and `capture(callable,
//! args...)`, a temporal error boundary around a call (ERR-7, ERR-8); with
//! them the temporal form of an `error` and of a `result`, which a
//! capture's `.error` and a call that returns its failure as data
//! publish, and a `?` in a `when`, which fails the node with the error it
//! keeps (ERR-5). Part of the transpiler, the last stage; a family of the
//! dispatch point (`forms`).
//!
//! | HGL | C++ |
//! | --- | --- |
//! | A lifted call, `scaled(value, factor: 2)` (FN-6) | a node of its own, a struct `lifted_00_` in the graph's struct, wired with `::hgraph::wire<lifted_00_>(wiring_, value, ::hgraph::Int{2})`: each argument that is a connection an input, and each other configuration, an `hgraph::Scalar`; its `eval` calls the function and publishes what it gives, each time it is evaluated; one that gives nothing is a sink |
//! | An input that must be valid (FN-11) | `hgraph::In<"value", S>`, which hgraph evaluates the node only once valid, and read checked (TIME-1) |
//! | An input whose parameter has a default, or admits null (FN-11) | `hgraph::In<"factor", S, InputValidity::Unchecked>` under a name of its own, `input_00_`, and the value bound to the parameter's name at the top of `eval`: `::hglrt::fn_valid(input_00_) ? T{read} : T{default}`; T the parameter's `T?` where the input is a connection of T (TYPE-14) |
//! | A struct, a tuple, a fixed list or a window given to one (FN-11, FN-12) | the helper is not called where it is valid and not all valid: `if (!::hglrt::fn_all_valid(view)) return;` |
//! | `eval scale(value: [1, _, 3])` of a value function (TST-8) | the same node, given the replays of the traces as its inputs, at the scope of the test's namespace ahead of the test |
//! | `let seen = capture(attempt, value)` (ERR-7) | a graph of its own for the call, a struct `captured_00_` in the graph's struct, wired by hgraph's `try_except_` with canonical errors, `::hglrt::capture<captured_00_>(wiring_, value)` (`<hglrt/capture.h>`) |
//! | `seen.value`, `seen.error` (ERR-7) | `::hglrt::captured_value(seen)`, `::hglrt::captured_error(seen)` |
//! | A temporal value of `error`, of `result<T, error>` | `TS<::hglrt::error>`, which is hgraph's `CanonicalError`; `TS<::hglrt::result<T>>`, a `std::expected` registered with hgraph as an atomic (`<hglrt/capture.h>`) |
//! | A cell of a trace of either (TST-5) | the value, `::hglrt::result<::hgraph::Int>{...}`, compared by its own equality, which excludes an error's provenance |
//! | `value?` in a `when` (ERR-5, ERR-8) | the result held before the statement, and the node failed with its error as it is, `::hglrt::raise(held.error())`, which keeps the context a retained error has |
//! | `failure.context`, and its `.node`, `.phase`, `.evaluation_time` (ERR-4) | `(failure).context`, a `std::optional` of hgraph's `ErrorContext`; its members of those names |
//!
//! Not written yet, and reported: a lifted call or a capture given an
//! argument of a pack (FN-8), a capture of a call of an operator's
//! candidate, and a capture given an `f64` configuration, which hgraph
//! would take for another given `-0.0` for `0.0` (hgraph issue 1866).

use hgl_analyser::hir::{
    Argument, Body, Callee, ErrorPart, Expr, ExprKind, Hook, Local, Stmt, ValueCategory,
};
use hgl_analyser::{DeclarationKind, Detail, LocalKind, Role as Phase, Type};
use hgl_lower::calls::{LeftOut, left_out};
use hgl_lower::types::read_whole_all_valid;

use super::Emitter;
use super::calls::{changes, may_fail};
use super::forms::Family;
use super::graphs::WIRING;
use super::names::{self, identifier};
use super::nodes::{Context, Kind, OUT, Role};
use super::statements::Scope;
use super::values::INDENT;
use crate::runtime;

/// An argument of a lifted call that the caller has written already: an
/// eval's replay of a trace (TST-8), or what a capture's graph is given
/// (ERR-7).
pub(super) struct Given {
    /// The argument's position among the call's arguments.
    pub(super) position: usize,
    /// Its C++: the port of a connection, or the value of configuration.
    pub(super) written: String,
    /// For a connection, the type of its temporal value; `None` for
    /// configuration, which is a value of the argument's own type.
    pub(super) input: Option<Type>,
}

impl Emitter<'_> {
    /// The lifted call `value` (FN-6: "A selected value helper ... with
    /// temporal arguments in composition is lifted: temporal arguments
    /// become inputs; scalar arguments and omitted defaults bind as
    /// configuration"), wired: a node written for it in the context's
    /// `lifted`, given each connection as an input and each other argument
    /// as configuration, in the order they are written, then each
    /// parameter the call leaves out as configuration, in the order of the
    /// parameters (EXPR-2). `given` are the arguments written already, by
    /// their positions; `defaults` the values already held for some of the
    /// parameters left out, by their positions among the parameters (an
    /// eval's, which holds them before its run's configuration); `result`
    /// the type of what the call gives, `None` for a function with no
    /// result.
    ///
    /// Its `eval` calls the function with the payload of each input and
    /// the value of each configuration, and publishes what it gives:
    /// "Each admitted evaluation publishes its result, including an equal
    /// result. An outputless helper lifts to a sink."
    pub(super) fn lifted(
        &mut self,
        scope: &mut Scope,
        value: &Expr,
        given: &[Given],
        defaults: &[(usize, String)],
        result: Option<&Type>,
    ) -> String {
        let ExprKind::Call {
            callee,
            arguments,
            form,
            ..
        } = &value.kind
        else {
            return self.missing(value.at, "this lifted call");
        };
        let program = self.program;
        let Some((module, declaration)) = callee.declaration() else {
            return self.missing(value.at, "this lifted call");
        };
        let Some(Detail::Callable(signature)) = program.modules[module].details.get(declaration)
        else {
            return self.missing(value.at, "this lifted call");
        };
        let mut locals: Vec<Local> = Vec::new();
        let mut roles: Vec<Role> = Vec::new();
        let mut parameters: Vec<String> = Vec::new();
        let mut wired: Vec<String> = vec![WIRING.to_string()];
        let mut synthetic: Vec<Argument> = Vec::new();
        // For each argument, the name of its scalar where it is one.
        let mut labels: Vec<Option<String>> = Vec::new();
        // What `eval` does before it calls the function: FN-11's
        // admission, and the values the inputs that need not be valid
        // bind.
        let mut prelude = String::new();
        let indent = INDENT.repeat(2);
        let mut unique = false;
        for (position, argument) in arguments.iter().enumerate() {
            let Some(index) = argument
                .parameter
                .filter(|&index| index < signature.parameters.len())
            else {
                return self.missing(argument.value.at, "an argument given to a pack");
            };
            let parameter = &signature.parameters[index];
            if locals.iter().any(|local| local.name == parameter.name) {
                return self.missing(argument.value.at, "this lifted call");
            }
            let at = argument.value.at;
            let name = identifier(&parameter.name);
            let label = names::string(&parameter.name);
            let written = given.iter().find(|given| given.position == position);
            let input = match written {
                Some(given) => given.input.clone(),
                None => argument
                    .value
                    .is_connection()
                    .then(|| argument.value.ty.clone()),
            };
            // The type of the body's local, and the type the helper is
            // given: for an input, what a read of it gives, which is not
            // the input's own type for every form (a window's `list<V>`,
            // TYPE-11, `forms::Family::read_type`).
            let (ty, read) = match input {
                Some(ty) => {
                    let Some(schema) = self.temporal_schema(&ty, at) else {
                        return "0".to_string();
                    };
                    let mut read = self.read_type(&ty);
                    // FN-11: "A struct, a tuple, a fixed list or a window
                    // is supplied only where it is `all_valid` as well".
                    let whole = read_whole_all_valid(program, &ty)
                        || matches!(ty, Type::RollingWindow { .. });
                    let all_valid = format!("::{}::fn_all_valid", runtime::NAMESPACE);
                    let valid = format!("::{}::fn_valid", runtime::NAMESPACE);
                    // An input the body reads is its view, of the input's
                    // type; a fallback's binding is a value of the read's.
                    let mut local = ty.clone();
                    match left_out(program, (module, declaration), index, parameter) {
                        // FN-11: "Every other input must satisfy ordinary
                        // `valid(input)`": hgraph evaluates the node only
                        // once it is.
                        None => {
                            parameters.push(format!("::hgraph::In<{label}, {schema}> {name}"));
                            roles.push(Role::Input);
                            if whole {
                                prelude.push_str(&format!(
                                    "{indent}if (!{all_valid}({name}))\n{indent}{{\n{indent}{INDENT}return;\n{indent}}}\n"
                                ));
                            }
                        }
                        // FN-11: "A lifted argument with a declared default
                        // does not require a valid input: bind the default
                        // when that input is invalid. Otherwise, an argument
                        // whose value contract admits null does not require
                        // a valid input: bind null when it is invalid ... A
                        // valid value, including a valid null, is supplied
                        // as published." Its input is not required valid,
                        // and is as active as any other: "Fallback changes
                        // admission/binding, not activation".
                        Some(fallback) => {
                            let view = names::temporary("input", self.temporary());
                            parameters.push(format!(
                                "::hgraph::In<{label}, {schema}, ::hgraph::InputValidity::Unchecked> {view}"
                            ));
                            roles.push(Role::Plain);
                            // TYPE-14: "A T is a value where a `T?` is
                            // expected": a connection of T given to a
                            // parameter written `T?` binds a `T?`, the value
                            // read or its fallback.
                            let nullable =
                                matches!(parameter.ty.omission_removed(), Type::Nullable(_));
                            if nullable && !matches!(read, Type::Nullable(_)) {
                                read = Type::Nullable(Box::new(read));
                            }
                            local = read.clone();
                            let Some(cpp) = self.written_type(&read, at) else {
                                return "0".to_string();
                            };
                            let value = self.temporal_read(&ty, &view, at);
                            let fallback = match fallback {
                                LeftOut::Default(default) => {
                                    // The analyser keeps a default as a
                                    // value with no locals of its own, so a
                                    // block in it that runs statements
                                    // cannot be written (`calls`).
                                    if changes(default) {
                                        return self
                                            .missing(default.at, "a default that runs statements");
                                    }
                                    // UNIT-7: a default is read with no
                                    // local in scope.
                                    let cold = Body {
                                        locals: Vec::new(),
                                        statements: Vec::new(),
                                    };
                                    let mut defaults = Scope::cold(program, &cold);
                                    let written = self.expression(&mut defaults, default);
                                    if !defaults.before.is_empty() {
                                        return self.missing(
                                            default.at,
                                            "a default that propagates a failure",
                                        );
                                    }
                                    written
                                }
                                LeftOut::Null => "::std::nullopt".to_string(),
                            };
                            // "one that is valid and not all valid binds no
                            // fallback, its default included, and the
                            // helper is not invoked".
                            if whole {
                                prelude.push_str(&format!(
                                    "{indent}if ({valid}({view}) && !{all_valid}({view}))\n{indent}{{\n{indent}{INDENT}return;\n{indent}}}\n"
                                ));
                            }
                            prelude.push_str(&format!(
                                "{indent}[[maybe_unused]] const {cpp} {name} = {valid}({view}) ? {cpp}{{{value}}} : {cpp}{{{fallback}}};\n"
                            ));
                        }
                    }
                    wired.push(match written {
                        Some(given) => given.written.clone(),
                        None => self.wiring_form(scope, &argument.value),
                    });
                    labels.push(None);
                    (local, read)
                }
                None => {
                    let ty = argument.value.ty.clone();
                    let Some(cpp) = self.written_type(&ty, at) else {
                        return "0".to_string();
                    };
                    // FN-1: as hgraph holds a configuration
                    // (`configuration`).
                    let held = super::configuration::held(program, &ty, &cpp);
                    parameters.push(format!("::hgraph::Scalar<{label}, {held}> {name}"));
                    roles.push(Role::Config);
                    // The configuration is given as the type its scalar
                    // is, a literal among it.
                    let configured = match written {
                        Some(given) => given.written.clone(),
                        None => self.expression(scope, &argument.value),
                    };
                    unique |= super::nodes::holds_f64(program, &ty, 0);
                    wired.push(super::configuration::given(
                        program,
                        &ty,
                        &cpp,
                        &format!("static_cast<{cpp}>({configured})"),
                    ));
                    labels.push(Some(parameter.name.clone()));
                    (ty.clone(), ty)
                }
            };
            synthetic.push(Argument {
                parameter: argument.parameter,
                value: Expr {
                    kind: ExprKind::Local(locals.len()),
                    ty: read,
                    at,
                    category: ValueCategory::Value,
                },
            });
            locals.push(Local {
                name: parameter.name.clone(),
                at,
                kind: LocalKind::Parameter,
                ty,
                category: ValueCategory::Value,
            });
        }
        // FN-6: "scalar arguments and omitted defaults bind as
        // configuration": each parameter the call leaves out is given its
        // default where the graph is built, after the arguments written and
        // in the order of the parameters (EXPR-2), as an `hgraph::Scalar`.
        // One written `T?` with no default is given null (UNIT-8), which the
        // call of the function in the node writes itself; so is a default
        // of a `T?` that cannot fail, which is the same value wherever it
        // is evaluated: hgraph holds no `std::optional` as configuration.
        let mut omitted_failing = 0;
        for (index, parameter) in signature.parameters.iter().enumerate() {
            if arguments
                .iter()
                .any(|argument| argument.parameter == Some(index))
            {
                continue;
            }
            if parameter.pack.is_some() {
                return self.missing(value.at, "a lifted call that leaves out a pack");
            }
            let (configured, ty) = match left_out(program, (module, declaration), index, parameter)
            {
                Some(LeftOut::Null) => continue,
                Some(LeftOut::Default(default)) if matches!(default.ty, Type::Nullable(_)) => {
                    if may_fail(default) {
                        return self.missing(
                            default.at,
                            "a lifted call that leaves out a `T?` whose default may fail",
                        );
                    }
                    continue;
                }
                Some(LeftOut::Default(default)) => {
                    let configured = match defaults.iter().find(|(at, _)| *at == index) {
                        Some((_, held)) => held.clone(),
                        None => {
                            if changes(default) {
                                return self.missing(default.at, "a default that runs statements");
                            }
                            let cold = Body {
                                locals: Vec::new(),
                                statements: Vec::new(),
                            };
                            let mut defaults = Scope::cold(program, &cold);
                            let written = self.expression(&mut defaults, default);
                            if !defaults.before.is_empty() {
                                return self
                                    .missing(default.at, "a default that propagates a failure");
                            }
                            omitted_failing += usize::from(may_fail(default));
                            written
                        }
                    };
                    (configured, default.ty.clone())
                }
                None => {
                    return self.missing(
                        value.at,
                        &format!("a call that leaves out `{}`", parameter.name),
                    );
                }
            };
            let Some(cpp) = self.written_type(&ty, value.at) else {
                return "0".to_string();
            };
            let label = names::string(&parameter.name);
            let name = identifier(&parameter.name);
            let held = super::configuration::held(program, &ty, &cpp);
            parameters.push(format!("::hgraph::Scalar<{label}, {held}> {name}"));
            roles.push(Role::Config);
            unique |= super::nodes::holds_f64(program, &ty, 0);
            wired.push(super::configuration::given(
                program,
                &ty,
                &cpp,
                &format!("static_cast<{cpp}>({configured})"),
            ));
            labels.push(Some(parameter.name.clone()));
            synthetic.push(Argument {
                parameter: Some(index),
                value: Expr {
                    kind: ExprKind::Local(locals.len()),
                    ty: ty.clone(),
                    at: value.at,
                    category: ValueCategory::Value,
                },
            });
            locals.push(Local {
                name: parameter.name.clone(),
                at: value.at,
                kind: LocalKind::Parameter,
                ty,
                category: ValueCategory::Value,
            });
        }
        let result = result.filter(|_| signature.result.is_some());
        let schema = match result {
            Some(result) => match self.temporal_schema(result, value.at) {
                Some(schema) => {
                    parameters.push(format!("::hgraph::Out<{schema}> {OUT}"));
                    Some(schema)
                }
                None => return "0".to_string(),
            },
            None => None,
        };
        let call = Expr {
            kind: ExprKind::Call {
                callee: callee.clone(),
                arguments: synthetic,
                form: *form,
                lifted: false,
            },
            ty: result.cloned().unwrap_or_else(|| value.ty.clone()),
            at: value.at,
            category: ValueCategory::Value,
        };
        let statement = if result.is_some() {
            Stmt::Return {
                value: Some(call),
                at: value.at,
            }
        } else {
            Stmt::Expr(call)
        };
        let body = Body {
            locals,
            statements: vec![statement],
        };
        let mut context = Context::new(Kind::Lifted, roles.len());
        context.roles = roles;
        context.result = result.cloned();
        let mut lifted_scope = Scope::cold(program, &body);
        let mut statements = prelude;
        self.in_context(&mut context, |emitter| {
            emitter.statements(&mut lifted_scope, &body.statements, 2, &mut statements);
        });
        // A node of a graph is a struct in the graph's own, named as the
        // piece's temporaries are. One of an eval is written at the scope
        // of the test's namespace, ahead of the test, where every test of
        // the module writes its own: it is named by where its call is.
        let name = if self.kind() == Some(Kind::Entry) {
            format!("lifted_0{}_{}_", value.at.file.index(), value.at.span.start)
        } else {
            names::temporary("lifted", self.temporary())
        };
        let callee_name = &program.modules[module].declarations[declaration].name;
        let graph = &program.modules[self.module].path;
        let line = self.line_of(value.at);
        let text = format!(
            "// The call of `{callee_name}` on line {line}, lifted (FN-6).\n\
             struct {name}\n{{\n\
             {INDENT}static constexpr auto name = {};\n\
             {INDENT}static void eval({})\n{INDENT}{{\n{statements}{INDENT}}}\n}};\n",
            names::string(&format!("{graph}:{callee_name}")),
            parameters
                .iter()
                .map(|parameter| format!("[[maybe_unused]] {parameter}"))
                .collect::<Vec<_>>()
                .join(", ")
        );
        if let Some(context) = self.temporal.as_mut() {
            context.lifted.push(text);
        }
        // EXPR-2: the arguments in the order written, each held first
        // where more than one may fail or one runs statements. What the
        // caller wrote is a port or a value already.
        let unwritten = |position: usize| !given.iter().any(|given| given.position == position);
        let failing = arguments
            .iter()
            .enumerate()
            .filter(|(position, argument)| unwritten(*position) && may_fail(&argument.value))
            .count()
            + omitted_failing;
        let mut steps = String::new();
        if failing > 1
            || arguments
                .iter()
                .enumerate()
                .any(|(position, argument)| unwritten(position) && changes(&argument.value))
        {
            for written in wired.iter_mut().skip(1) {
                let held = names::temporary("argument", self.temporary());
                steps.push_str(&format!("const auto {held} = {written}; "));
                *written = held;
            }
        }
        let held = |wiring: String| {
            if steps.is_empty() {
                wiring
            } else {
                format!("[&] {{ {steps}return {wiring}; }}()")
            }
        };
        if unique {
            let arguments: Vec<(Option<String>, String)> =
                labels.into_iter().zip(wired.into_iter().skip(1)).collect();
            return held(super::nodes::unique(&name, schema.as_deref(), &arguments));
        }
        held(format!("::hgraph::wire<{name}>({})", wired.join(", ")))
    }

    /// `capture(callable, args...)` of the call `call` (ERR-7: "In
    /// composition, `capture(callable, args...)` creates a temporal error
    /// boundary"), wired: a graph written for the call, a struct
    /// `captured_00_` in the context's `lifted`, which hgraph's
    /// `try_except_` evaluates as a child graph with canonical errors
    /// (`::hglrt::capture`). Each argument that is a connection is a port
    /// of the child graph, and each other an `hgraph::Scalar` of it, in
    /// the order written; the child calls the callable with them.
    ///
    /// ERR-7: "Capturing a node covers its evaluation; capturing a graph
    /// covers failures that leave that subgraph. Each evaluation failure
    /// ticks its canonical error; before failure the stream is invalid,
    /// and later success leaves its retained error valid but unmodified."
    /// ERR-8: "Start/stop failures escape": hgraph's `try_except_` gives
    /// an evaluation's failure to its error output and lets a start's and
    /// a stop's through.
    fn capture(&mut self, scope: &mut Scope, value: &Expr, call: &Expr) -> String {
        let ExprKind::Call {
            callee, arguments, ..
        } = &call.kind
        else {
            return self.missing(value.at, "a `capture` of this");
        };
        let program = self.program;
        let Some((module, declaration)) = callee.declaration() else {
            return self.missing(value.at, "a `capture` of this");
        };
        let Some(Detail::Callable(signature)) = program.modules[module].details.get(declaration)
        else {
            return self.missing(value.at, "a `capture` of this");
        };
        let lifted = call.is_lifted();
        let temporal = matches!(
            program.modules[module].declarations[declaration].kind,
            DeclarationKind::Function(Phase::Temporal)
        );
        if !lifted && !temporal {
            return self.missing(value.at, "a `capture` of a call of this");
        }
        // The child graph's parameters, and what the boundary is given
        // for each, in the order the arguments are written.
        let mut parameters = vec![format!("::hgraph::Wiring& {WIRING}")];
        let mut outer: Vec<String> = vec![WIRING.to_string()];
        let mut given: Vec<Given> = Vec::new();
        for (position, argument) in arguments.iter().enumerate() {
            let Some(index) = argument
                .parameter
                .filter(|&index| index < signature.parameters.len())
            else {
                return self.missing(argument.value.at, "an argument given to a pack");
            };
            let parameter = &signature.parameters[index];
            let name = names::temporary("input", self.temporary());
            if argument.value.is_connection() {
                // TYPE-14: a connection of T given to a temporal parameter
                // written `T?` is widened where the boundary is given it
                // (`graphs::widened_from`); a lifted call's input binds it.
                let ty = match super::graphs::widened_from(&argument.value.ty, &parameter.ty) {
                    Some(_) if !lifted => parameter.ty.clone(),
                    _ => argument.value.ty.clone(),
                };
                let Some(schema) = self.temporal_schema(&ty, argument.value.at) else {
                    return "0".to_string();
                };
                parameters.push(format!("[[maybe_unused]] ::hgraph::Port<{schema}> {name}"));
                outer.push(self.connection(scope, &argument.value, &ty));
                given.push(Given {
                    position,
                    written: name,
                    input: Some(ty),
                });
            } else {
                // FN-3: a value where a temporal parameter expects a
                // connection is a constant, wired outside the boundary.
                if !parameter.constant && !lifted {
                    let port = self.connection(scope, &argument.value, &parameter.ty);
                    let Some(schema) = self.temporal_schema(&parameter.ty, argument.value.at)
                    else {
                        return "0".to_string();
                    };
                    parameters.push(format!("[[maybe_unused]] ::hgraph::Port<{schema}> {name}"));
                    outer.push(port);
                    given.push(Given {
                        position,
                        written: name,
                        input: Some(parameter.ty.clone()),
                    });
                    continue;
                }
                let ty = argument.value.ty.clone();
                // hgraph issue 1866: hgraph takes two boundaries of one
                // graph, given the same connections, for one where their
                // configurations are equal under `==`, as `0.0` and `-0.0`
                // are, and `try_except_` has no wiring as a node of its own.
                if super::nodes::holds_f64(program, &ty, 0) {
                    return self.missing(
                        argument.value.at,
                        "a `capture` given an `f64` configuration (hgraph issue 1866)",
                    );
                }
                let Some(cpp) = self.written_type(&ty, argument.value.at) else {
                    return "0".to_string();
                };
                let label = names::string(&parameter.name);
                // FN-1: as hgraph holds a configuration (`configuration`).
                let held = super::configuration::held(program, &ty, &cpp);
                parameters.push(format!(
                    "[[maybe_unused]] ::hgraph::Scalar<{label}, {held}> {name}"
                ));
                let configured = self.expression(scope, &argument.value);
                outer.push(super::configuration::given(
                    program,
                    &ty,
                    &cpp,
                    &format!("static_cast<{cpp}>({configured})"),
                ));
                given.push(Given {
                    position,
                    written: super::configuration::read(program, &ty, &name),
                    input: None,
                });
            }
        }
        let gives = signature.result.is_some();
        let result = if gives {
            match self.temporal_schema(&call.ty, call.at) {
                Some(schema) => format!("::hgraph::Port<{schema}>"),
                None => return "0".to_string(),
            }
        } else {
            "void".to_string()
        };
        // The call the child graph makes, of its own ports and scalars.
        let inner = if lifted {
            let result = gives.then(|| call.ty.clone());
            self.lifted(scope, call, &given, &[], result.as_ref())
        } else {
            let ports: Vec<(usize, String)> = given
                .iter()
                .map(|given| (given.position, given.written.clone()))
                .collect();
            self.temporal_call(scope, call, &ports)
        };
        let statement = if gives {
            format!("return {inner};")
        } else {
            format!("{inner};")
        };
        let name = names::temporary("captured", self.temporary());
        let callee_name = &program.modules[module].declarations[declaration].name;
        let graph = &program.modules[self.module].path;
        let line = self.line_of(value.at);
        let text = format!(
            "// The `capture` of `{callee_name}` on line {line} (ERR-7): the graph hgraph's\n\
             // `try_except_` evaluates as its child.\n\
             struct {name}\n{{\n\
             {INDENT}static constexpr auto name = {};\n\
             {INDENT}static {result} compose({})\n{INDENT}{{\n{INDENT}{INDENT}{statement}\n{INDENT}}}\n}};\n",
            names::string(&format!("{graph}:capture({callee_name})")),
            parameters.join(", ")
        );
        if let Some(context) = self.temporal.as_mut() {
            context.lifted.push(text);
        }
        // EXPR-2: the arguments in the order written, each held first
        // where more than one may fail or one runs statements.
        let failing = arguments
            .iter()
            .filter(|argument| may_fail(&argument.value))
            .count();
        let captured = format!("::{}::capture<{name}>", runtime::NAMESPACE);
        if failing > 1 || arguments.iter().any(|argument| changes(&argument.value)) {
            let mut steps = String::new();
            for written in outer.iter_mut().skip(1) {
                let held = names::temporary("argument", self.temporary());
                steps.push_str(&format!("const auto {held} = {written}; "));
                *written = held;
            }
            return format!(
                "[&] {{ {steps}return {captured}({}); }}()",
                outer.join(", ")
            );
        }
        format!("{captured}({})", outer.join(", "))
    }

    /// `seen.value` or `seen.error` (ERR-7: "What it gives is bound with
    /// `let` and read through two connections"), of `capture`, a capture
    /// or a local bound to one: the port of the boundary's field.
    fn captured(&mut self, scope: &mut Scope, capture: &Expr, error: bool) -> String {
        let boundary = match &capture.kind {
            ExprKind::Local(local) if self.role(*local) == Role::Connection => {
                identifier(&scope.body.locals[*local].name)
            }
            ExprKind::Capture(call) => self.capture(scope, capture, call),
            _ => return self.missing(capture.at, "a read of a capture"),
        };
        let field = if error {
            "captured_error"
        } else {
            "captured_value"
        };
        format!("::{}::{field}({boundary})", runtime::NAMESPACE)
    }

    /// `operand?` in a `when` (ERR-5: "In when, only `result<T,error>` may
    /// propagate to node failure. No success output is written; completed
    /// effects stand"): the result held under a name of its own before
    /// the statement, and the node failed with its error where it is a
    /// failure; the expression is the success's payload. The error is
    /// raised as it is, so that a retained one keeps its context (ERR-8:
    /// "A consumer that propagates a retained error with `?` makes a new
    /// failure event, whose error keeps its first provenance"). Only where
    /// it is the first thing its statement evaluates, as a value
    /// function's `?` is (`expressions`).
    fn raised(
        &mut self,
        scope: &mut Scope,
        operand: &Expr,
        at: hgl_diagnostics::Location,
    ) -> String {
        if scope.conditional > 0 || scope.acted {
            return self.missing(at, "a `?` here");
        }
        if scope.read {
            return self.missing(at, "a `?` after a read in its statement");
        }
        let written = self.expression(scope, operand);
        let held = names::temporary("propagated", self.temporary());
        scope.before.push(format!("const auto {held} = {written};"));
        scope.before.push(format!(
            "if (!{held}.has_value())\n{{\n{INDENT}::{}::raise({held}.error());\n}}",
            runtime::NAMESPACE
        ));
        format!("(*{held})")
    }
}

/// The C++ type of `ty` where it is a type whose temporal form this
/// family writes: an `error`, and a `result` of a value and an error
/// (ERR-2, ERR-4), each an atomic of hgraph's.
fn atomic(emitter: &Emitter, ty: &Type) -> Option<String> {
    match ty {
        Type::Error | Type::Result { .. } => super::types::cpp_type(emitter.program, ty),
        _ => None,
    }
}

/// The lifting family: lifted calls, captures, the temporal forms of
/// `error` and `result`, and `?` in a `when`.
pub(super) struct Lifting;

impl Family for Lifting {
    fn schema(&self, emitter: &mut Emitter<'_>, ty: &Type) -> Option<String> {
        atomic(emitter, ty).map(|written| format!("::hgraph::TS<{written}>"))
    }

    // TIME-1: the read is checked when it runs, and fails with
    // `value.unset_read` where it is not admitted.
    fn read(&self, emitter: &mut Emitter<'_>, ty: &Type, view: &str) -> Option<String> {
        atomic(emitter, ty)?;
        Some(format!("::{}::checked_value({view})", runtime::NAMESPACE))
    }

    // ERR-2: "Temporal outcomes publish atomically, including repeats":
    // the whole value, whose delta is itself.
    fn publish(
        &self,
        emitter: &mut Emitter<'_>,
        ty: &Type,
        view: &str,
        value: &str,
    ) -> Option<String> {
        atomic(emitter, ty)?;
        Some(format!("{view}.set({value});"))
    }

    // END-3: an atomic's delta is the value published, as a scalar's is.
    fn delta(
        &self,
        emitter: &mut Emitter<'_>,
        ty: &Type,
        view: &str,
        checked: bool,
    ) -> Option<String> {
        atomic(emitter, ty)?;
        let native = if checked {
            "fn_delta_value_checked"
        } else {
            "fn_delta_value"
        };
        Some(format!("::{}::{native}({view})", runtime::NAMESPACE))
    }

    // FN-3: a value wired as a constant, one publication when the graph
    // starts.
    fn constant(&self, emitter: &mut Emitter<'_>, ty: &Type, value: &str) -> Option<String> {
        let written = atomic(emitter, ty)?;
        Some(format!(
            "::{}::constant<{written}>({WIRING}, {value})",
            runtime::NAMESPACE
        ))
    }

    // TST-5: a cell is "written as a `return` is": an `error` and a
    // `result` publish whole, and a cell is the value, `ok(2)` or
    // `err(error(message: "m"))`, compared by its own equality, which
    // excludes an error's provenance (ERR-2, ERR-4).
    fn cell(
        &self,
        emitter: &mut Emitter<'_>,
        scope: &mut Scope<'_>,
        ty: &Type,
        value: &Expr,
    ) -> Option<String> {
        let written = atomic(emitter, ty)?;
        let value = emitter.expression(scope, value);
        Some(format!("{written}{{{value}}}"))
    }

    // ERR-7: "What it gives is bound with `let`": the boundary's port,
    // whose type is the library's.
    fn statement(
        &self,
        emitter: &mut Emitter<'_>,
        scope: &mut Scope<'_>,
        statement: &Stmt,
        depth: usize,
        text: &mut String,
    ) -> bool {
        let Stmt::Declare {
            local,
            value:
                Some(
                    value @ Expr {
                        kind: ExprKind::Capture(call),
                        ..
                    },
                ),
        } = statement
        else {
            return false;
        };
        if emitter.kind() != Some(Kind::Graph) {
            return false;
        }
        let indent = INDENT.repeat(depth);
        let name = identifier(&scope.body.locals[*local].name);
        let boundary = emitter.capture(scope, value, call);
        text.push_str(&format!(
            "{indent}[[maybe_unused]] const auto {name} = {boundary};\n"
        ));
        true
    }

    fn expression(
        &self,
        emitter: &mut Emitter<'_>,
        scope: &mut Scope<'_>,
        value: &Expr,
    ) -> Option<String> {
        match &value.kind {
            ExprKind::Propagate(operand) if emitter.kind() == Some(Kind::Hook(Hook::When)) => {
                Some(emitter.raised(scope, operand, value.at))
            }
            // ERR-4: "`.context` is a read-only nullable record of `.node:
            // str`, `.phase: str`, one of `"evaluate"`, `"start"`, and
            // `"stop"`, and `.evaluation_time: datetime`; a constructed
            // error has none". hgraph gives it where the error first fails
            // a node, and keeps it as it propagates.
            ExprKind::ErrorPart { base, part } => {
                let member = match part {
                    ErrorPart::Context => "context",
                    ErrorPart::Node => "node",
                    ErrorPart::Phase => "phase",
                    ErrorPart::EvaluationTime => "evaluation_time",
                    ErrorPart::Message | ErrorPart::Code => return None,
                };
                let base = emitter.expression(scope, base);
                Some(format!("({base}).{member}"))
            }
            _ => None,
        }
    }

    fn wiring(
        &self,
        emitter: &mut Emitter<'_>,
        scope: &mut Scope<'_>,
        value: &Expr,
    ) -> Option<String> {
        match &value.kind {
            ExprKind::Call { .. } if value.is_lifted() => {
                let result = value.ty.clone();
                let gives = gives(emitter, value);
                Some(emitter.lifted(scope, value, &[], &[], gives.then_some(&result)))
            }
            ExprKind::Capture(call) => Some(emitter.capture(scope, value, call)),
            ExprKind::Captured { capture, error } => Some(emitter.captured(scope, capture, *error)),
            _ => None,
        }
    }
}

/// True where the call `value` gives something: its function has a
/// result.
fn gives(emitter: &Emitter, value: &Expr) -> bool {
    let ExprKind::Call { callee, .. } = &value.kind else {
        return false;
    };
    let declared = match callee {
        Callee::Function {
            module,
            declaration,
            ..
        }
        | Callee::Candidate {
            module,
            declaration,
            ..
        } => emitter.program.modules[*module].details.get(*declaration),
        _ => None,
    };
    matches!(declared, Some(Detail::Callable(signature)) if signature.result.is_some())
}
