//! The temporal form of a struct, a tuple and an atomic struct: the
//! family of the dispatch point (`forms`) that writes them, and what a
//! struct the value code writes (`composites`) tells hgraph so that it is
//! one. Part of the transpiler, the last stage.
//!
//! TYPE-9: "Temporalization recursively maps tuples/structs to child
//! fields ... `atomic<V>` stops expansion at a complete ordinary V".
//!
//! | HGL | C++ |
//! | --- | --- |
//! | `struct Quote { bid: i64, ask: i64 }` | beside the aggregate `composites` writes, `hgraph::native_bundle<Quote>`, its nominal schema and its members, and `hglrt::type_named` of it and of its delta, its HGL name |
//! | A temporal value of `Quote` | hgraph's `NativeTSB<Quote>`, a nominal bundle whose whole value is the aggregate, with a child endpoint for each field; where a field is optional (TYPE-4), hgraph's `NominalTSB` of `Quote` with each field's own form, an optional one `TS<Optional<T>>` |
//! | A temporal value of `tuple<i64, str>` | hgraph's `UnNamedTSB` of a field for each position, named by it: `"0"`, `"1"` |
//! | A temporal value of `atomic<Quote>` | `TS<Quote>`: one endpoint whose value is the whole struct |
//! | A read of one whole in a hook (TIME-1) | `::hglrt::whole_value(view)`, checked when it runs: "requires `all_valid` of it", an optional child that is not valid read as null |
//! | `quote.bid`, `pair[0]` read in a hook | the read of that child's own view, `::hglrt::child<0>(quote)`, as its form reads it |
//! | `valid(quote.bid)`, `modified`, `all_valid`, `delta_value`, `last_modified` of a child (TIME-9, END-8) | the library's natives given the child's view |
//! | `all_valid(quote)` (TIME-9) | `::hglrt::fn_all_valid(quote)`: every field valid by its own `valid`, an optional one aside |
//! | `return value`, `out = value` (TIME-7) | `::hglrt::publish(out_, value);`: a whole value publishes each field, a delta each field it holds |
//! | `out.bid = value` in a `when` (TIME-7: "admitted piecewise mutation") | the publication of the field's child, as its form publishes |
//! | `delta_value(quote)` (END-3, TYPE-16) | `::hglrt::fn_delta_value(quote)`: each field that published in the cycle, with its delta |
//! | `update(quote).bid` in a graph (EXPR-7) | `::hglrt::field_port<0>(port)`, the child's own connection |
//! | `whole(quote).bid` of an atomic struct in a graph | `::hglrt::atomic_field<0, S>(wiring_, port)`, a node that publishes the field at each whole publication |
//! | A cell of a trace of one (TST-5) | a `hglrt::testing::Cell<T>`: its delta, or its whole value (`Whole<T>`); a trace of an atomic struct one of `hglrt::atomic_trace<S>`, whose cell, the whole struct, is its delta |
//! | A list field, `counts: list<i64>`, a list position (TYPE-9) | the list's own form, the `lists` family's `TSL`, asked of the dispatch point; the struct's nominal bundle holds the field as the value hgraph holds that `TSL` as, `::hglrt::list_value<::hgraph::Int>`; read and published through the list's own form (endpoints.h, `whole` and `publisher`) |
//!
//! An atomic struct that holds an optional field, at any depth, is one
//! endpoint of the whole struct as any other is: hgraph maps the C++
//! optional of the field to its `Optional`, a null to a published null.
//!
//! Not this family's, and reported: an abstract struct or a struct with
//! generic value arguments as a temporal value, a struct with no field,
//! a tuple with an atomic position, a struct or a tuple
//! with a part whose form is not a scalar's, an enum's, a nullable's, a
//! list's or another of this family's (a set, a dictionary, a window), a
//! constant connection of one (FN-3), and a read of a part of `out`.

use hgl_analyser::hir::{EndpointOperation, Expr, ExprKind, Hook, Stmt, Value};
use hgl_analyser::{
    Argument, DeclarationKind, Detail, GenericKind, NamedKind, Program, Scalar, Type,
};
use hgl_lower::types::{fields_of, fixed_list_size, inherited};

use super::Emitter;
use super::forms::Family;
use super::graphs::WIRING;
use super::names;
use super::nodes::{Kind, Role};
use super::statements::Scope;
use super::types::{cpp_type, cpp_type_in};
use super::values::INDENT;
use crate::runtime;

/// The structs' family: structs, tuples and atomic structs.
pub(super) struct Structs;

/// The struct `(module, declaration)` that `ty` names, where it is one
/// hgraph holds as a nominal bundle: concrete, with a supported schema
/// for each field and generic type argument ([`bundle_field`]).
pub(super) fn bundled(program: &Program, ty: &Type) -> Option<(usize, usize)> {
    let Type::Named {
        module,
        declaration,
        kind: NamedKind::Struct,
        ..
    } = ty
    else {
        return None;
    };
    bundles(program, ty, 0, &mut Vec::new()).then_some((*module, *declaration))
}

/// Check the fields of this particular application, giving each field
/// the struct's type arguments before looking up its schema (TYPE-3,
/// TYPE-4). `depth` bounds the structs it holds (TYPE-6).
fn bundles(program: &Program, ty: &Type, depth: usize, checked: &mut Vec<(Type, usize)>) -> bool {
    let Type::Named {
        module,
        declaration,
        arguments,
        kind: NamedKind::Struct,
        ..
    } = ty
    else {
        return false;
    };
    // An application can occur both as an argument and as a field. Reuse
    // a successful check with the same or a smaller remaining depth bound.
    if checked
        .iter()
        .any(|(known, at)| depth <= *at && known == ty)
    {
        return true;
    }
    if depth > 32 || inherited(program, *module, *declaration) || cpp_type(program, ty).is_none() {
        return false;
    }
    let supported = arguments.iter().all(|argument| {
        matches!(argument, Argument::Type(ty) if bundle_field(program, ty, depth + 1, checked).is_some())
    }) && fields_of(program, ty).is_some_and(|fields| {
        !fields.is_empty()
            && fields
                .iter()
                .all(|(_, field)| bundle_field(program, field, depth, checked).is_some())
    });
    if supported {
        checked.push((ty.clone(), depth));
    }
    supported
}

/// The type of a field of type `ty` in a struct's nominal schema, as
/// hgraph holds it (`hgraph::NominalBundle`): a scalar or an enum as its
/// C++ type, a nullable of one as hgraph's `Optional`, and a struct, or
/// an atomic one, as that struct's own schema. `None` for any other.
fn bundle_field(
    program: &Program,
    ty: &Type,
    depth: usize,
    checked: &mut Vec<(Type, usize)>,
) -> Option<String> {
    match ty {
        Type::Scalar(Scalar::Any | Scalar::Timezone | Scalar::ZonedTime) => None,
        Type::Scalar(_) => cpp_type(program, ty),
        Type::Named {
            kind: NamedKind::Struct,
            ..
        } => {
            if !bundles(program, ty, depth + 1, checked) {
                return None;
            }
            Some(format!(
                "::hgraph::native_bundle<{}>::schema",
                cpp_type(program, ty)?
            ))
        }
        Type::Named { .. } if enumeration(program, ty) => cpp_type(program, ty),
        Type::Nullable(inner)
            if matches!(**inner, Type::Scalar(_)) || enumeration(program, inner) =>
        {
            let payload = bundle_field(program, inner, depth, checked)?;
            Some(format!("::hgraph::Optional<{payload}>"))
        }
        Type::Atomic(payload) if matches!(**payload, Type::Named { .. }) => {
            bundle_field(program, payload, depth, checked)
        }
        // A list field, whose temporal form is the `lists` family's `TSL`:
        // the value schema hgraph holds that `TSL` as, of the element's
        // (structs.h, `list_value`), which the bundle's field must be.
        Type::List { element, size } => {
            let element = bundle_field(program, element, depth, checked)?;
            match fixed_list_size(size.as_ref()) {
                None => Some(format!("::{}::list_value<{element}>", runtime::NAMESPACE)),
                Some(size) => match size.value {
                    hgl_analyser::ConstantValue::Int(count) if count > 0 => Some(format!(
                        "::{}::list_value<{element}, {count}>",
                        runtime::NAMESPACE
                    )),
                    _ => None,
                },
            }
        }
        _ => None,
    }
}

/// True where `ty` is an enum with a member.
fn enumeration(program: &Program, ty: &Type) -> bool {
    let Type::Named {
        module,
        declaration,
        ..
    } = ty
    else {
        return false;
    };
    let declared = &program.modules[*module];
    declared.declarations[*declaration].kind == DeclarationKind::Enum
        && matches!(declared.details.get(*declaration), Some(Detail::Enum(read)) if !read.members.is_empty())
}

/// The form of a part of a struct or a tuple, a field or a position, of
/// type `ty`: hgraph's schema of it, where it is one whose value the
/// library reads and publishes whole or part by part (structs.h): a
/// scalar's or an enum's `TS<T>`, a nullable's `TS<Optional<T>>`, a
/// struct's, a tuple's or an atomic struct's of this family, and a list's
/// `TSL`, the `lists` family's, read and published through its own form
/// (TYPE-9; endpoints.h, `whole` and `publisher`). Asked of the dispatch
/// point, and checked against what the library takes.
fn part_schema(emitter: &mut Emitter, ty: &Type) -> Option<String> {
    let schema = emitter.temporal_schema_of(ty)?;
    let program = emitter.program;
    let mine = matches!(
        ty,
        Type::Tuple(_)
            | Type::Named {
                kind: NamedKind::Struct,
                ..
            }
            | Type::Atomic(_)
    );
    let whole = cpp_type(program, ty).map(|written| format!("::hgraph::TS<{written}>"));
    let nullable = match ty {
        Type::Nullable(inner) => cpp_type(program, inner)
            .map(|written| format!("::hgraph::TS<::hgraph::Optional<{written}>>")),
        _ => None,
    };
    let list = schema.starts_with("::hgraph::TSL<");
    (mine || list || Some(&schema) == whole.as_ref() || Some(&schema) == nullable.as_ref())
        .then_some(schema)
}

/// The C++ type of a temporal form of this family, and which it is.
enum Form {
    /// A struct, `NativeTSB<S>` or `NominalTSB<S, ...>`.
    Struct(String),
    /// A tuple, `UnNamedTSB<...>`.
    Tuple(String),
    /// An atomic struct, `TS<S>`.
    Atomic(String),
}

/// The form of `ty` where this family writes it: the C++ type of its
/// value and which form it is; its types noted where they are another
/// module's.
fn form(emitter: &mut Emitter, ty: &Type) -> Option<Form> {
    let program = emitter.program;
    match ty {
        Type::Named { .. } => {
            bundled(program, ty)?;
            Some(Form::Struct(emitter.named_type(ty)?))
        }
        Type::Tuple(positions) if !positions.is_empty() => {
            Some(Form::Tuple(emitter.named_type(ty)?))
        }
        Type::Atomic(payload) => {
            bundled(program, payload)?;
            Some(Form::Atomic(emitter.named_type(payload)?))
        }
        _ => None,
    }
}

impl Family for Structs {
    fn schema(&self, emitter: &mut Emitter<'_>, ty: &Type) -> Option<String> {
        match form(emitter, ty)? {
            Form::Struct(written) => {
                let program = emitter.program;
                let fields = fields_of(program, ty)?;
                let mut parts = Vec::new();
                // Whether every field's form is the one hgraph's own mapping
                // of the aggregate gives it (`NativeTSB`): a `TS` of the
                // field's C++ type, or a struct's `NativeTSB`.
                let mut native = true;
                for (member, field) in &fields {
                    let schema = part_schema(emitter, field)?;
                    let whole = cpp_type(program, field)
                        .is_some_and(|written| schema == format!("::hgraph::TS<{written}>"));
                    native &= !matches!(field, Type::Atomic(_))
                        && (whole || schema.starts_with("::hgraph::NativeTSB<"));
                    parts.push(format!(
                        "::hgraph::Field<{}, {schema}>",
                        names::string(&member.name)
                    ));
                }
                // TYPE-9: each field a child of its own form: hgraph's
                // `NativeTSB` where each is as its mapping has it, and
                // otherwise, an optional field or an atomic one, at any
                // depth, `NominalTSB` of each field's own form.
                if !native {
                    Some(format!(
                        "::hgraph::NominalTSB<{written}, {}>",
                        parts.join(", ")
                    ))
                } else {
                    Some(format!("::hgraph::NativeTSB<{written}>"))
                }
            }
            Form::Tuple(_) => {
                let Type::Tuple(positions) = ty else {
                    return None;
                };
                // An atomic position's C++ value is its payload's, whose
                // delta the library's tuple delta would take for the
                // position's, which is the payload itself (TYPE-16).
                if positions.iter().any(|part| matches!(part, Type::Atomic(_))) {
                    return None;
                }
                let mut parts = Vec::new();
                for (position, part) in positions.iter().enumerate() {
                    let schema = part_schema(emitter, part)?;
                    parts.push(format!("::hgraph::Field<\"{position}\", {schema}>"));
                }
                Some(format!("::hgraph::UnNamedTSB<{}>", parts.join(", ")))
            }
            // TYPE-9: "`atomic<V>` stops expansion at a complete ordinary
            // V": one endpoint of the whole struct.
            Form::Atomic(written) => Some(format!("::hgraph::TS<{written}>")),
        }
    }

    // TST-5, TST-16: a trace of an atomic struct holds whole structs, each
    // its publication's delta (TYPE-16: "`atomic<V>`: V"), which the
    // struct's own C++ type, whose delta is field by field, does not say:
    // the trace is one of `hglrt::atomic_trace<S>`.
    fn payload(&self, emitter: &mut Emitter<'_>, ty: &Type) -> Option<String> {
        match form(emitter, ty)? {
            Form::Atomic(written) => {
                Some(format!("::{}::atomic_trace<{written}>", runtime::NAMESPACE))
            }
            Form::Struct(_) | Form::Tuple(_) => None,
        }
    }

    // TIME-1: "A read of a struct, a tuple or a fixed list whole, `return
    // quote`, requires `all_valid` of it", checked again when it runs.
    fn read(&self, emitter: &mut Emitter<'_>, ty: &Type, view: &str) -> Option<String> {
        let native = match form(emitter, ty)? {
            Form::Struct(_) | Form::Tuple(_) => "whole_value",
            Form::Atomic(_) => "checked_value",
        };
        Some(format!("::{}::{native}({view})", runtime::NAMESPACE))
    }

    // TIME-7: "take a complete value of the result's type T, published
    // whole, or a `delta<T>` (TYPE-16), published as that change"; "A
    // result of `atomic<V>` takes a V".
    fn publish(
        &self,
        emitter: &mut Emitter<'_>,
        ty: &Type,
        view: &str,
        value: &str,
    ) -> Option<String> {
        Some(match form(emitter, ty)? {
            Form::Struct(_) | Form::Tuple(_) => {
                format!("::{}::publish({view}, {value});", runtime::NAMESPACE)
            }
            Form::Atomic(_) => format!("{view}.set({value});"),
        })
    }

    // END-3, TYPE-16: a struct's delta names each field that changed, a
    // tuple's each position, and an atomic's is the whole value; ERR-14
    // has the read take its form from its place.
    fn delta(
        &self,
        emitter: &mut Emitter<'_>,
        ty: &Type,
        view: &str,
        checked: bool,
    ) -> Option<String> {
        form(emitter, ty)?;
        let native = if checked {
            "fn_delta_value_checked"
        } else {
            "fn_delta_value"
        };
        Some(format!("::{}::{native}({view})", runtime::NAMESPACE))
    }

    // TST-5: a cell is "written as a `return` is (TIME-7) ... a T,
    // published whole, or a `delta<T>`": for a struct and a tuple, whose
    // delta is no T, a `Cell<T>` of either.
    fn cell(
        &self,
        emitter: &mut Emitter<'_>,
        scope: &mut Scope<'_>,
        ty: &Type,
        value: &Expr,
    ) -> Option<String> {
        let testing = names::testing("");
        let testing = testing.trim_end_matches("::");
        let written = match form(emitter, ty)? {
            Form::Struct(written) | Form::Tuple(written) => written,
            // An atomic struct's cell is the whole struct, its delta.
            Form::Atomic(written) => {
                let made = emitter.expression(scope, value);
                return Some(format!(
                    "{testing}::Cell<::{}::atomic_trace<{written}>>{{{made}}}",
                    runtime::NAMESPACE
                ));
            }
        };
        let made = emitter.expression(scope, value);
        if value.ty == *ty {
            Some(format!(
                "{testing}::Cell<{written}>{{{testing}::Whole<{written}>{{{made}}}}}"
            ))
        } else {
            Some(format!("{testing}::Cell<{written}>{{{made}}}"))
        }
    }

    fn expression(
        &self,
        emitter: &mut Emitter<'_>,
        scope: &mut Scope<'_>,
        value: &Expr,
    ) -> Option<String> {
        let hook = matches!(emitter.kind(), Some(Kind::Hook(_) | Kind::Lifted));
        match &value.kind {
            // TYPE-9: an atomic struct's input holds the whole struct, which
            // the analyser types its read as.
            ExprKind::Local(local) if hook && emitter.role(*local) == Role::Input => {
                let declared = &scope.body.locals[*local];
                let Type::Atomic(payload) = &declared.ty else {
                    return None;
                };
                if **payload != value.ty {
                    return None;
                }
                let declared = declared.ty.clone();
                let view = names::identifier(&scope.body.locals[*local].name);
                scope.read = true;
                scope.acted = true;
                Some(emitter.temporal_read(&declared, &view, value.at))
            }
            // TIME-1: "In a hook, a read of a child's value ... requires
            // that child's admission", which the analyser has proved; it
            // is read as the child's own form reads it.
            ExprKind::Field { .. } | ExprKind::Index { .. } if hook => {
                let (view, out) = child_view(emitter, scope, value)?;
                if out {
                    return Some(emitter.missing(value.at, "a read of a part of `out`"));
                }
                scope.read = true;
                scope.acted = true;
                Some(emitter.temporal_read(&value.ty, &view, value.at))
            }
            // TIME-9: an operation on a child is given the child's view.
            ExprKind::ChildEndpoint { operation, child } if hook => {
                let (view, _) = child_view(emitter, scope, child)?;
                scope.acted = true;
                Some(child_operation(emitter, *operation, &view, child, false))
            }
            ExprKind::ChildEndpointOf(child) if hook => {
                let (view, _) = child_view(emitter, scope, child)?;
                Some(view)
            }
            // ERR-14: a child's `delta_value` where its place takes a
            // `result`.
            ExprKind::CheckedDelta(read) if hook => {
                let ExprKind::ChildEndpoint {
                    operation: EndpointOperation::DeltaValue,
                    child,
                } = &read.kind
                else {
                    return None;
                };
                let (view, _) = child_view(emitter, scope, child)?;
                scope.acted = true;
                Some(child_operation(
                    emitter,
                    EndpointOperation::DeltaValue,
                    &view,
                    child,
                    true,
                ))
            }
            // TIME-9: "`all_valid` is true where every one is, by its own
            // `valid`, one level deep, an optional child aside".
            ExprKind::Endpoint {
                operation: EndpointOperation::AllValid,
                inputs: Some(inputs),
            } if hook && !inputs.is_empty() => {
                let locals = &scope.body.locals;
                let mut tested = Vec::new();
                for &local in inputs {
                    let ty = endpoint_type(emitter, locals, local)?;
                    if matches!(form(emitter, &ty)?, Form::Atomic(_)) {
                        return None;
                    }
                    let view = emitter.view(locals, local)?;
                    tested.push(format!("::{}::fn_all_valid({view})", runtime::NAMESPACE));
                }
                Some(if tested.len() == 1 {
                    tested.remove(0)
                } else {
                    format!("({})", tested.join(" && "))
                })
            }
            _ => None,
        }
    }

    // TIME-7: `out = value`'s "admitted piecewise mutation": a field of
    // the result assigned in a `when` publishes that child.
    fn statement(
        &self,
        emitter: &mut Emitter<'_>,
        scope: &mut Scope<'_>,
        statement: &Stmt,
        depth: usize,
        text: &mut String,
    ) -> bool {
        let Stmt::AssignPlace {
            place,
            operator,
            value,
            at,
        } = statement
        else {
            return false;
        };
        let Some((view, true)) = child_view(emitter, scope, place) else {
            return false;
        };
        if operator.is_some() || emitter.kind() != Some(Kind::Hook(Hook::When)) {
            emitter.unsupported(*at, "this assignment to a part of `out`");
            return true;
        }
        let written = emitter.expression(scope, value);
        let published = emitter.temporal_publish(&place.ty, &view, &written, *at);
        text.push_str(&format!("{}{published}\n", INDENT.repeat(depth)));
        true
    }

    // EXPR-7, TYPE-9: a field of a struct's connection, or a position of a
    // tuple's, is that child's own connection; a field of an atomic
    // struct's is a node that reads the whole value.
    fn wiring(
        &self,
        emitter: &mut Emitter<'_>,
        scope: &mut Scope<'_>,
        value: &Expr,
    ) -> Option<String> {
        let (base, index) = match &value.kind {
            ExprKind::Field { base, member } => (base, *member),
            ExprKind::Index { base, index } if matches!(base.ty, Type::Tuple(_)) => {
                let ExprKind::Literal(Value::Int(position)) = index.kind else {
                    return None;
                };
                (base, usize::try_from(position).ok()?)
            }
            _ => return None,
        };
        let atomic = match form(emitter, &base.ty)? {
            Form::Struct(_) | Form::Tuple(_) => false,
            Form::Atomic(_) => true,
        };
        let port = emitter.wiring_form(scope, base);
        if atomic {
            let schema = emitter.temporal_schema(&value.ty, value.at)?;
            return Some(format!(
                "::{}::atomic_field<{index}, {schema}>({WIRING}, {port})",
                runtime::NAMESPACE
            ));
        }
        Some(format!(
            "::{}::field_port<{index}>({port})",
            runtime::NAMESPACE
        ))
    }
}

/// The type of the endpoint `local` of `locals`: a temporal input's own,
/// or the node's result for `out`.
fn endpoint_type(
    emitter: &Emitter,
    locals: &[hgl_analyser::hir::Local],
    local: usize,
) -> Option<Type> {
    match emitter.role(local) {
        Role::Input => Some(locals[local].ty.clone()),
        Role::Out => emitter
            .temporal
            .as_ref()
            .and_then(|context| context.result.clone()),
        _ => None,
    }
}

/// The view of `value`, a temporal input, `out`, or a field of a struct
/// or a position of a tuple of either, at any depth (TIME-1: "A child of
/// a collection ... is an endpoint whether or not it is valid"), and
/// whether it is `out` or a part of it. `None` for anything else: a
/// child of another collection is its own family's.
pub(super) fn child_view(
    emitter: &mut Emitter,
    scope: &Scope,
    value: &Expr,
) -> Option<(String, bool)> {
    match &value.kind {
        ExprKind::Local(local) => {
            let view = emitter.view(&scope.body.locals, *local)?;
            Some((view, emitter.role(*local) == Role::Out))
        }
        ExprKind::Field { base, member } => {
            let base_type = whole_type(emitter, scope, base)?;
            if !matches!(form(emitter, &base_type)?, Form::Struct(_)) {
                return None;
            }
            let (view, out) = child_view(emitter, scope, base)?;
            Some((
                format!("::{}::child<{member}>({view})", runtime::NAMESPACE),
                out,
            ))
        }
        ExprKind::Index { base, index } => {
            let base_type = whole_type(emitter, scope, base)?;
            if !matches!(form(emitter, &base_type)?, Form::Tuple(_)) {
                return None;
            }
            let ExprKind::Literal(Value::Int(position)) = index.kind else {
                return None;
            };
            let (view, out) = child_view(emitter, scope, base)?;
            Some((
                format!("::{}::child<{position}>({view})", runtime::NAMESPACE),
                out,
            ))
        }
        _ => None,
    }
}

/// The type of the endpoint `value` is, where it names one: an input's
/// declared type, the node's result for `out`, and a child's own.
fn whole_type(emitter: &Emitter, scope: &Scope, value: &Expr) -> Option<Type> {
    match &value.kind {
        ExprKind::Local(local) => endpoint_type(emitter, &scope.body.locals, *local),
        _ => Some(value.ty.clone()),
    }
}

/// The operation `operation` on the child of view `view`, `child` as the
/// analyser read it (TIME-9, END-8); its `delta_value` in the form where
/// its place takes a `result` where `checked` (ERR-14).
fn child_operation(
    emitter: &mut Emitter,
    operation: EndpointOperation,
    view: &str,
    child: &Expr,
    checked: bool,
) -> String {
    let native = |name: &str| format!("::{}::{name}({view})", runtime::NAMESPACE);
    match operation {
        EndpointOperation::Valid => native("fn_valid"),
        EndpointOperation::Modified => native("fn_modified"),
        EndpointOperation::LastModified => native("fn_last_modified"),
        EndpointOperation::AllValid => native("fn_all_valid"),
        EndpointOperation::DeltaValue => emitter.temporal_delta(&child.ty, view, checked, child.at),
        _ => emitter.missing(child.at, &format!("`{}` of a field", operation.name())),
    }
}

impl Emitter<'_> {
    /// What the struct `index` of the module tells hgraph and the library
    /// beside its definition (`composites`), where hgraph holds it as a
    /// nominal bundle: `hgraph::native_bundle` of it, its nominal schema
    /// (its module's path and its name) with each field's type, and its
    /// members; and `hglrt::type_named` of it and of its delta, by which
    /// the library names it, a list of it among them (values.h).
    pub(super) fn bundle_traits(
        &self,
        index: usize,
        qualified: &str,
        delta: &str,
        specialized: &str,
        parameters: &[String],
    ) -> String {
        let program = self.program;
        let declared = &program.modules[self.module];
        let Some(Detail::Struct(read)) = declared.details.get(index) else {
            return String::new();
        };
        if inherited(program, self.module, index)
            || read.members.is_empty()
            || read
                .generics
                .iter()
                .any(|parameter| !matches!(parameter.kind, GenericKind::Plain | GenericKind::Type))
        {
            return String::new();
        }
        let generic = !parameters.is_empty();
        let ns = runtime::NAMESPACE;
        let schema_of = |ty: &str| format!("::{ns}::bundle_value<{ty}>");
        let mut fields = Vec::new();
        let mut checked = Vec::new();
        for member in &read.members {
            let field = if generic {
                cpp_type_in(program, &member.ty, true).map(|ty| schema_of(&ty))
            } else {
                bundle_field(program, &member.ty, 0, &mut checked)
            };
            let Some(field) = field else {
                return String::new();
            };
            fields.push(format!(
                "::hgraph::Field<{}, {field}>",
                names::string(&member.name)
            ));
        }
        let declaration = &declared.declarations[index];
        let name = &declaration.name;
        let hgl_name = declared.qualified_declaration_name(name, declaration.test_owner);
        let namespace = hgl_name
            .rsplit_once("::")
            .map_or(&*declared.path, |(scope, _)| scope);
        let arguments = parameters
            .iter()
            .map(|parameter| schema_of(parameter))
            .collect::<Vec<_>>()
            .join(", ");
        let schema = format!(
            "::hgraph::NominalBundle<{}, {}, false, ::hgraph::BundleParents<>, ::hgraph::BundleArguments<{arguments}>, {}>",
            names::string(namespace),
            names::string(name),
            fields.join(", ")
        );
        let mut text = format!(
            "// TYPE-9: `{name}` as hgraph's nominal bundle, its fields as `fields` names them.\n\
             {specialized}struct hgraph::native_bundle<{qualified}>\n{{\n\
             {INDENT}using schema = {schema};\n\
             {INDENT}static constexpr auto members = ::{ns}::fields<{qualified}>::members;\n}};\n"
        );
        let field_names: Vec<String> = read
            .members
            .iter()
            .map(|member| names::string(&member.name))
            .collect();
        for (of, is_delta) in [(qualified, false), (delta, true)] {
            let name = if generic {
                let applied = format!(
                    "::{ns}::applied_struct_name<{}>({})",
                    parameters.join(", "),
                    names::string(&hgl_name),
                );
                let name = if is_delta {
                    format!("\"delta<\" + {applied} + \">\"")
                } else {
                    applied
                };
                format!("static ::std::string name() {{ return {name}; }}")
            } else {
                let name = if is_delta {
                    format!("delta<{hgl_name}>")
                } else {
                    hgl_name.clone()
                };
                format!(
                    "static constexpr ::std::string_view name = {};",
                    names::string(&name)
                )
            };
            text.push_str(&format!(
                "{specialized}struct {ns}::type_named<{of}>\n{{\n\
                 {INDENT}{name}\n\
                 {INDENT}static constexpr ::std::array<::std::string_view, {}> fields{{{}}};\n\
                 {INDENT}static constexpr bool delta = {is_delta};\n}};\n",
                field_names.len(),
                field_names.join(", ")
            ));
        }
        text
    }
}
