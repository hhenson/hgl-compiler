//! Writes what a hook reads of its endpoints and its capabilities: the
//! payload of a temporal input, core's operations on an endpoint, and
//! the properties of a capability. Part of the transpiler, the last
//! stage; a family of the dispatch point (`forms`).
//!
//! | HGL | C++ |
//! | --- | --- |
//! | `price`, a temporal input read in a hook (TIME-1) | its temporal form's checked read of the view, `::hglrt::checked_value(price)` for a scalar |
//! | `key`, a `const` parameter read in a hook | `key.value()`, the node's `hgraph::Scalar` |
//! | `valid(a)`, `valid(a, b)`, `valid()`, `valid([])` (END-1) | `::hglrt::fn_valid(a)`, each of them, every temporal input, `true` |
//! | `modified(a)`, `modified(a, b)`, `modified()` (END-2) | `::hglrt::fn_modified(a)`, any of them, any temporal input |
//! | `delta_value(a)` (END-3, ERR-14) | the plain read, or the read whose place takes a `result` where the analyser recorded it so ([`ExprKind::CheckedDelta`]): `::hglrt::fn_delta_value(a)`, `fn_delta_value_checked(a)` for a scalar |
//! | `last_modified(a)` (END-6) | `::hglrt::fn_last_modified(a)` |
//! | `invalidate(out)` (END-4) | `::hglrt::fn_invalidate(out_)` |
//! | `is_active(a)`, an `endpoint<T>` argument of a native (TIME-9, TYPE-12) | `::hglrt::fn_is_active(a)`: the view itself, which the natives take as it is |
//! | `clock.evaluation_time`, a property of a capability (CAP-1) | `clock.evaluation_time()`: the library's capability, made at the top of the hook |
//!
//! `is_active`, `activate` and `passivate` of an input are the `activity`
//! family's, `scheduled()` and `modified([])` read as a value in a
//! handler the `scheduling` family's, and `all_valid` of a window the
//! `windows` family's. Any other `all_valid`, and `modified([])` read as a
//! value elsewhere, are not written yet, and are reported.

use hgl_analyser::hir::{EndpointOperation, Expr, ExprKind, Local, Stmt, Value};
use hgl_analyser::{DeclarationKind, Detail, NamedKind, Program, Type};
use hgl_lower::calls::runtime_arguments;

use super::Emitter;
use super::forms::Family;
use super::names::{identifier, temporary};
use super::nodes::{Kind, Role};
use super::statements::Scope;
use super::values::INDENT;
use crate::runtime;

/// The capability `inject name` binds (TIME-8), as the hook's hgraph
/// view of it and the library's type made from that (`capabilities.h`).
/// The borrowed logger and clock views supply CAP-2 and CAP-3.
/// `None` for `scheduler` and `alarm`, which the timers' work writes, and
/// for any other name.
pub(super) fn capability(name: &str) -> Option<(&'static str, &'static str)> {
    Some(match name {
        "clock" => ("::hgraph::EvaluationClockView", "::hglrt::Clock"),
        "logger" => ("::hgraph::LoggerView", "::hglrt::Logger"),
        "global_state" => ("::hgraph::GlobalStateView", "::hglrt::GlobalState"),
        _ => return None,
    })
}

/// The C++ type of the native type `declaration` of `module`, where it
/// is one of core's capabilities (CAP-1): the library's own,
/// `::hglrt::Clock`. `None` for any other.
pub(super) fn capability_type(
    program: &Program,
    module: usize,
    declaration: usize,
) -> Option<String> {
    let declared = &program.modules[module].declarations[declaration];
    let core = Some(module) == program.core();
    let known = [
        "Logger",
        "Clock",
        "Scheduler",
        "Alarm",
        "GlobalState",
        "RunConfig",
    ]
    .contains(&declared.name.as_str());
    (core && known && declared.kind == DeclarationKind::NativeType)
        .then(|| format!("::{}::{}", runtime::NAMESPACE, declared.name))
}

/// The endpoints' family.
pub(super) struct Endpoints;

impl Emitter<'_> {
    /// The view chosen by an ERR-14 read or metadata operation. At a
    /// bound endpoint the arguments have already been captured.
    pub(super) fn read_endpoint_view(
        &mut self,
        scope: &mut Scope,
        endpoint: &Expr,
    ) -> Option<String> {
        let at = self.source_location(endpoint.at);
        self.endpoint_view(scope, endpoint, &mut None, &at)
    }

    /// Walk an endpoint path. At a binding every variable position or
    /// key is evaluated immediately and captured by value. The view is
    /// resolved at use with that use's location for a bounds failure.
    fn endpoint_view(
        &mut self,
        scope: &mut Scope,
        endpoint: &Expr,
        captures: &mut Option<Vec<String>>,
        where_: &str,
    ) -> Option<String> {
        let lib = runtime::NAMESPACE;
        match &endpoint.kind {
            ExprKind::Local(local) => self.view(&scope.body.locals, *local),
            ExprKind::BoundChild(local) => {
                let name = identifier(&scope.body.locals[*local].name);
                Some(format!("{name}({where_})"))
            }
            ExprKind::Field { base, member }
                if matches!(
                    base.ty,
                    Type::Named {
                        kind: NamedKind::Struct,
                        ..
                    }
                ) =>
            {
                let parent = self.endpoint_view(scope, base, captures, where_)?;
                Some(format!("::{lib}::child<{member}>({parent})"))
            }
            ExprKind::Index { base, index } => {
                let parent = self.endpoint_view(scope, base, captures, where_)?;
                if matches!(base.ty, Type::Tuple(_)) {
                    let ExprKind::Literal(Value::Int(position)) = index.kind else {
                        return None;
                    };
                    return Some(format!("::{lib}::child<{position}>({parent})"));
                }
                let index_code = match base.ty {
                    Type::List { .. } | Type::Dict { .. } => self.expression(scope, index),
                    _ => return None,
                };
                let path = |parent: &str, position: &str| match base.ty {
                    Type::List { .. } => format!("::{lib}::child({parent}, {position}, {where_})"),
                    Type::Dict { .. } => format!("::{lib}::keyed_child({parent}, {position})"),
                    _ => unreachable!("an index into a list or dictionary"),
                };
                if let Some(captures) = captures {
                    let held = temporary("selector", self.temporary());
                    scope
                        .before
                        .push(format!("const auto {held} = {index_code};"));
                    captures.push(held.clone());
                    return Some(path(&parent, &held));
                }
                // EXPR-2: choose the parent before the next index when
                // evaluation order is observable.
                if self.held_read(base, index) {
                    let held = temporary("parent", self.temporary());
                    let selected = temporary("selector", self.temporary());
                    let view = path(&held, &selected);
                    return Some(format!(
                        "[&]() {{ const auto& {held} = {parent}; const auto {selected} = {index_code}; return {view}; }}()"
                    ));
                }
                Some(path(&parent, &index_code))
            }
            _ => None,
        }
    }
}

/// True where a child is a previously bound endpoint, or derives from
/// one through another projection. Existing direct reads use their families.
fn bound_child(value: &Expr) -> bool {
    matches!(value.place_root().kind, ExprKind::BoundChild(_))
}

impl Family for Endpoints {
    fn expression(
        &self,
        emitter: &mut Emitter<'_>,
        scope: &mut Scope<'_>,
        value: &Expr,
    ) -> Option<String> {
        let locals = &scope.body.locals;
        let lib = runtime::NAMESPACE;
        match &value.kind {
            ExprKind::BoundChild(_) | ExprKind::Field { .. } | ExprKind::Index { .. }
                if bound_child(value) =>
            {
                let view = emitter.read_endpoint_view(scope, value)?;
                let at = emitter.source_location(value.at);
                scope.read = true;
                scope.acted = true;
                Some(format!(
                    "::{lib}::read_bound_child(::{lib}::checked_whole_result({view}), {at})"
                ))
            }
            ExprKind::ChildEndpointOf(child) if bound_child(child) => {
                let view = emitter.read_endpoint_view(scope, child)?;
                scope.acted = true;
                Some(view)
            }
            ExprKind::ChildEndpoint { operation, child } => {
                let view = emitter.read_endpoint_view(scope, child)?;
                scope.acted = true;
                let native = operation.name();
                if *operation == EndpointOperation::DeltaValue {
                    let at = emitter.source_location(value.at);
                    if bound_child(child) {
                        Some(format!(
                            "::{lib}::read_bound_child(::{lib}::fn_delta_value_checked({view}), {at})"
                        ))
                    } else {
                        Some(format!("::{lib}::fn_delta_value({view}, {at})"))
                    }
                } else {
                    Some(format!("::{lib}::fn_{native}({view})"))
                }
            }
            ExprKind::CheckedChild { read, result } => {
                let view = emitter.read_endpoint_view(scope, read)?;
                scope.read = true;
                scope.acted = true;
                if *result {
                    Some(format!("::{lib}::checked_whole_result({view})"))
                } else {
                    let at = emitter.source_location(value.at);
                    if bound_child(read) {
                        Some(format!(
                            "::{lib}::read_bound_child(::{lib}::checked_whole_result({view}), {at})"
                        ))
                    } else {
                        Some(format!("::{lib}::checked_whole({view}, {at})"))
                    }
                }
            }
            ExprKind::CheckedWhole(read) => {
                let view = emitter.read_endpoint_view(scope, read)?;
                scope.read = true;
                scope.acted = true;
                Some(format!("::{lib}::checked_whole_result({view})"))
            }
            ExprKind::Local(local) => match emitter.role(*local) {
                // TIME-1: a payload read, as the input's temporal form
                // reads it.
                Role::Input => {
                    scope.read = true;
                    scope.acted = true;
                    let declared = &locals[*local];
                    let view = identifier(&declared.name);
                    if declared.ty != value.ty {
                        return Some(
                            emitter.missing(value.at, "a read of an input narrowed by a test"),
                        );
                    }
                    Some(emitter.temporal_read(&declared.ty, &view, value.at))
                }
                // FN-1: the value as the node's `hgraph::Scalar` holds it
                // (`configuration`); ERR-12: "a binding a test has proved is
                // not null is read as its payload's type there".
                Role::Config => {
                    scope.read = true;
                    let declared = &locals[*local];
                    let read = super::configuration::read(
                        emitter.program,
                        &declared.ty,
                        &identifier(&declared.name),
                    );
                    Some(match &declared.ty {
                        Type::Nullable(payload) if **payload == value.ty => format!("(*{read})"),
                        _ => read,
                    })
                }
                Role::Out => Some(emitter.missing(value.at, "a read of `out`")),
                Role::Plain | Role::Connection => None,
            },
            ExprKind::EndpointOf(local) => match emitter.view(locals, *local) {
                Some(view) => Some(view),
                None => Some(emitter.missing(value.at, "this endpoint")),
            },
            ExprKind::Endpoint { operation, inputs } => Some(endpoint(
                emitter,
                scope,
                value,
                *operation,
                inputs.as_deref(),
            )),
            // ERR-14: the analyser records where the place of a
            // `delta_value` takes a `result`.
            ExprKind::CheckedDelta(read) => match &read.kind {
                ExprKind::ChildEndpoint { child, .. } => {
                    let view = emitter.read_endpoint_view(scope, child)?;
                    scope.acted = true;
                    Some(format!("::{lib}::fn_delta_value_checked({view})"))
                }
                ExprKind::Endpoint {
                    operation: EndpointOperation::DeltaValue,
                    inputs: Some(inputs),
                } if inputs.len() == 1 => {
                    scope.acted = true;
                    Some(emitter.delta_value(locals, inputs[0], true, read.at))
                }
                _ => Some(emitter.missing(value.at, "this read of a delta into a `result`")),
            },
            // CAP-1: "A property is read as an ordinary value": a member
            // function of the library's capability, of the property's
            // name.
            ExprKind::Field { base, member } => {
                let Type::Named {
                    module,
                    declaration,
                    ..
                } = &base.ty
                else {
                    return None;
                };
                capability_type(emitter.program, *module, *declaration)?;
                let Some(Detail::Native(native)) =
                    emitter.program.modules[*module].details.get(*declaration)
                else {
                    return None;
                };
                // The library names each member as core names the
                // property, and every HGL name is written through
                // `identifier`: core's are plain, and kept as they are.
                let property = identifier(&native.properties.get(*member)?.name);
                let base = emitter.expression(scope, base);
                scope.acted = true;
                Some(format!("({base}).{property}()"))
            }
            // TIME-9: a native that takes an `endpoint<T>` is given the
            // view itself, and is a template over the view's type: it is
            // called with no template argument.
            ExprKind::Call {
                callee, arguments, ..
            } => {
                let (module, declaration) = callee.declaration()?;
                let program = emitter.program;
                let declared = &program.modules[module].declarations[declaration];
                if !matches!(declared.kind, DeclarationKind::NativeFunction(_)) {
                    return None;
                }
                let Some(Detail::Callable(signature)) =
                    program.modules[module].details.get(declaration)
                else {
                    return None;
                };
                if !signature
                    .parameters
                    .iter()
                    .any(|parameter| matches!(parameter.ty, Type::Endpoint(_)))
                {
                    return None;
                }
                let name = super::names::native_function(&declared.name);
                if Some(module) != program.core() || !(emitter.provides)(&name) {
                    return Some(emitter.missing(
                        value.at,
                        &format!("the native function `{}`", declared.name),
                    ));
                }
                let mut given = vec![String::new(); signature.parameters.len()];
                for (_, argument) in runtime_arguments(arguments) {
                    let Some(index) = argument.parameter.filter(|&index| index < given.len())
                    else {
                        return Some(emitter.missing(argument.value.at, "this argument"));
                    };
                    given[index] = emitter.expression(scope, &argument.value);
                }
                let given: Vec<_> = given
                    .into_iter()
                    .zip(&signature.parameters)
                    .filter(|(_, parameter)| !matches!(parameter.ty, Type::Function(_)))
                    .map(|(argument, _)| argument)
                    .collect();
                if given.iter().any(String::is_empty) {
                    return Some(emitter.missing(value.at, "a call that leaves out an endpoint"));
                }
                scope.acted = true;
                Some(format!(
                    "::{}::{name}({})",
                    runtime::NAMESPACE,
                    given.join(", ")
                ))
            }
            _ => None,
        }
    }

    // END-4: `invalidate(out)` stands as a statement of its own.
    fn statement(
        &self,
        emitter: &mut Emitter<'_>,
        scope: &mut Scope<'_>,
        statement: &Stmt,
        depth: usize,
        text: &mut String,
    ) -> bool {
        if let Stmt::Declare {
            local,
            value:
                Some(Expr {
                    kind: ExprKind::ChildEndpointOf(child),
                    ..
                }),
        } = statement
            && matches!(scope.body.locals[*local].ty, Type::Endpoint(_))
        {
            let name = identifier(&scope.body.locals[*local].name);
            let mut captures = Some(Vec::new());
            let at = temporary("read_at", emitter.temporary());
            if let Some(view) = emitter.endpoint_view(scope, child, &mut captures, &at) {
                let captured = captures.expect("a binding records its selectors");
                let captures = if captured.is_empty() {
                    "&".to_string()
                } else {
                    format!("&, {}", captured.join(", "))
                };
                let indent = INDENT.repeat(depth);
                text.push_str(&format!(
                    "{indent}const auto {name} = [{captures}]([[maybe_unused]] ::std::source_location {at}) {{ return {view}; }};\n"
                ));
            } else {
                emitter.unsupported(child.at, "this child endpoint binding");
            }
            return true;
        }
        let Stmt::Expr(
            value @ Expr {
                kind:
                    ExprKind::Endpoint {
                        operation: EndpointOperation::Invalidate,
                        ..
                    },
                ..
            },
        ) = statement
        else {
            return false;
        };
        let written = emitter.expression(scope, value);
        text.push_str(&format!("{}{written};\n", INDENT.repeat(depth)));
        true
    }
}

/// The operation `operation` on `inputs`, `value` as the analyser read
/// it (TIME-9; core's END-1 to END-10), in a hook.
fn endpoint(
    emitter: &mut Emitter<'_>,
    scope: &mut Scope<'_>,
    value: &Expr,
    operation: EndpointOperation,
    inputs: Option<&[usize]>,
) -> String {
    if !matches!(emitter.kind(), Some(Kind::Hook(_) | Kind::Lifted)) {
        return emitter.missing(value.at, &format!("`{}` here", operation.name()));
    }
    let locals = &scope.body.locals;
    // With no argument, every temporal input (TIME-3).
    let every: Vec<usize> = (0..locals.len())
        .filter(|&local| emitter.role(local) == Role::Input)
        .collect();
    let named: &[usize] = inputs.unwrap_or(&every);
    let views: Option<Vec<String>> = named
        .iter()
        .map(|&local| emitter.view(locals, local))
        .collect();
    let Some(views) = views else {
        return emitter.missing(value.at, &format!("`{}` of this", operation.name()));
    };
    let native = |name: &str, view: &str| format!("::{}::{name}({view})", runtime::NAMESPACE);
    match operation {
        // END-1: "valid(x)" of each named; `valid([])` requires none.
        EndpointOperation::Valid => match views.as_slice() {
            [] => "true".to_string(),
            [one] => native("fn_valid", one),
            several => format!(
                "({})",
                several
                    .iter()
                    .map(|view| native("fn_valid", view))
                    .collect::<Vec<_>>()
                    .join(" && ")
            ),
        },
        // END-2, TIME-3: "`modified(a, b)` ... requires a publication on
        // at least one of them". `modified([])` read as a value is the
        // timers' work (TIME-3).
        EndpointOperation::Modified => match views.as_slice() {
            [] => emitter.missing(value.at, "`modified([])` as a value"),
            [one] => native("fn_modified", one),
            several => format!(
                "({})",
                several
                    .iter()
                    .map(|view| native("fn_modified", view))
                    .collect::<Vec<_>>()
                    .join(" || ")
            ),
        },
        EndpointOperation::DeltaValue if named.len() == 1 => {
            scope.acted = true;
            emitter.delta_value(locals, named[0], false, value.at)
        }
        EndpointOperation::LastModified if views.len() == 1 => {
            native("fn_last_modified", &views[0])
        }
        EndpointOperation::Invalidate
            if views.len() == 1 && emitter.role(named[0]) == Role::Out =>
        {
            scope.acted = true;
            emitter
                .invalidate_result()
                .unwrap_or_else(|| native("fn_invalidate", &views[0]))
        }
        _ => emitter.missing(value.at, &format!("`{}` here", operation.name())),
    }
}

impl Emitter<'_> {
    /// `delta_value` of `local`, of `locals` (END-3), in the form its
    /// place takes: the plain read, or, where `checked`, the read whose
    /// place takes `result<delta<T>, error>` (ERR-14), as the analyser
    /// records it with [`ExprKind::CheckedDelta`]. This is the one place
    /// the form is chosen; the endpoint's temporal form writes each.
    pub(super) fn delta_value(
        &mut self,
        locals: &[Local],
        local: usize,
        checked: bool,
        at: hgl_diagnostics::Location,
    ) -> String {
        let Some(view) = self.view(locals, local) else {
            return self.missing(at, "`delta_value` of this");
        };
        let ty = match self.role(local) {
            Role::Out => match self
                .temporal
                .as_ref()
                .and_then(|context| context.result.clone())
            {
                Some(result) => result,
                None => return self.missing(at, "`delta_value` of this"),
            },
            _ => locals[local].ty.clone(),
        };
        if let Some(form) = self.input_reader(local).cloned() {
            let underlying = super::lifting::reader_operand(&form);
            let value = self.temporal_read(&underlying.ty, &view, at);
            let value = self.apply_reader_chain(&form, &value);
            let delta = Type::delta(ty);
            let Some(written) = self.written_type(&delta, at) else {
                return "0".to_string();
            };
            let library = runtime::NAMESPACE;
            let missing = "\"the delta of an endpoint that has none in the cycle is read\"";
            let held = format!("{written}{{{value}}}");
            return if checked {
                format!(
                    "[&]() -> ::{library}::result<{written}> {{ if (!::{library}::has_delta({view})) return ::std::unexpected(::{library}::make_error({missing}, ::std::string{{::{library}::codes::unset_read}})); return {held}; }}()"
                )
            } else {
                format!(
                    "[&]() -> {written} {{ if (!::{library}::has_delta({view})) ::{library}::unset_read({missing}); return {held}; }}()"
                )
            };
        }
        self.temporal_delta(&ty, &view, checked, at)
    }
}
