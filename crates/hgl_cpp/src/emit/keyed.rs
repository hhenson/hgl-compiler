//! The temporal form of a set and of a dictionary: the family of the
//! dispatch point (`forms`) that writes a temporal value of `set<K>` and
//! of `dict<K, V>` (TYPE-9: "dictionaries to keyed children, and sets to
//! membership"). Part of the transpiler, the last stage.
//!
//! | HGL | C++ |
//! | --- | --- |
//! | A temporal value of `set<K>`, of `dict<K, V>` | hgraph's `TSS<K>`, `TSD<K, V'>`, `V'` the form of `V` |
//! | A set or dictionary used as a constant connection (FN-3) | `::hglrt::constant<T, Schema>` publishes its whole membership or entries once |
//! | A read of one whole in a hook (TIME-1) | `::hglrt::keyed_value(view)`: an `::hglrt::set` or `::hglrt::dict`, which fails with `value.unset_read` where it is not valid |
//! | `return value`, `out = value`, of a whole value or a delta (TIME-7) | `::hglrt::keyed_publish(view, value);`: hgraph's whole write of the one, its write of a delta of the other |
//! | `delta_value(view)` (END-3, TYPE-16, ERR-14) | `::hglrt::fn_delta_value(view)`, or `fn_delta_value_checked`: an `::hglrt::set_delta`, `::hglrt::dict_delta` |
//! | `all_valid(book)` (TIME-9) | `::hglrt::fn_all_valid(book)` |
//! | `book[key]` read in a hook, as a value or into a `result` (TIME-1, EXPR-7, ERR-14) | `::hglrt::keyed_entry_value(book, key)`, `keyed_entry_result`: an entry the dictionary does not hold, or that is not valid, fails with `value.unset_read` or gives an `err` of it |
//! | `valid(book[key])`, `modified`, `last_modified`, `delta_value` of an entry (TIME-9) | `::hglrt::keyed_entry_valid(book, key)` and the like: an entry the dictionary does not hold is not valid |
//! | `invalidate(out)` (END-4) | the endpoints' `::hglrt::fn_invalidate(out_)`, which the library writes for these forms |
//! | A cell of a trace of one (TST-5) | `::hglrt::testing::Whole<T>{value}` for a value written whole; the delta itself for a delta |
//!
//! A set's element and a dictionary's key are a scalar, which hgraph
//! holds as a value of its own; a dictionary's value is a scalar, a set,
//! a dictionary or a tuple of such positions, whose form the library
//! converts (TIME-1). This family reports unsupported temporal forms
//! and direct payload reads of nested entries (`book["a"]["b"]`). Observations of
//! nested entries (END-8) use the `endpoints` family's view of the complete
//! path. A key's child as a graph connection (EXPR-7) is the `references`
//! family's, a designation of the entry.

use hgl_analyser::hir::{EndpointOperation, Expr, ExprKind, Node, Stmt, walk_statement_nodes};
use hgl_analyser::{Scalar, Type};
use hgl_diagnostics::Location;
use hgl_lower::nodes::Handler;

use super::Emitter;
use super::forms::Family;
use super::graphs::WIRING;
use super::nodes::{Kind, OUT, Role};
use super::statements::Scope;
use super::types::cpp_type;
use super::values::INDENT;
use crate::runtime;

/// The family of sets and dictionaries.
pub(super) struct Keyed;

/// The C++ type of `ty` where it is a set's element or a dictionary's key
/// this family writes: a scalar, but `timezone`, `zoned_time` and `any`
/// (TYPE-14: "A set's element and a dictionary's key are never null"),
/// which hgraph holds as a value of its own.
fn key(emitter: &Emitter, ty: &Type) -> Option<String> {
    match ty {
        Type::Scalar(Scalar::Timezone | Scalar::ZonedTime | Scalar::Any) => None,
        Type::Scalar(_) => cpp_type(emitter.program, ty),
        _ => None,
    }
}

/// True where `ty` is a dictionary's value whose form the library
/// converts (`keyed_form`): a scalar's, a set's, a dictionary's, and a
/// tuple's of such positions, whose form is the `structs` family's (the
/// dispatch point gives it).
fn entry_form(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Scalar(scalar) if !matches!(scalar, Scalar::Timezone | Scalar::ZonedTime | Scalar::Any)
    ) || matches!(ty, Type::Set(_) | Type::Dict { .. })
        || matches!(ty, Type::Tuple(positions) if !positions.is_empty() && positions.iter().all(entry_form))
}

/// hgraph's schema of a temporal value of `ty`, where it is a set or a
/// dictionary this family writes.
fn schema(emitter: &mut Emitter, ty: &Type) -> Option<String> {
    match ty {
        Type::Set(element) => Some(format!("::hgraph::TSS<{}>", key(emitter, element)?)),
        Type::Dict { key: keys, value } if entry_form(value) => {
            let keys = key(emitter, keys)?;
            let value = emitter.temporal_schema_of(value)?;
            Some(format!("::hgraph::TSD<{keys}, {value}>"))
        }
        _ => None,
    }
}

/// How many keyed writes a path can make before it finishes or reaches
/// the next statement. We only need to distinguish zero, one and many.
#[derive(Clone, Copy)]
struct Writes {
    continuing: Option<usize>,
    maximum: usize,
}

impl Writes {
    const NONE: Self = Self {
        continuing: Some(0),
        maximum: 0,
    };

    fn followed_by(self, next: Self) -> Self {
        let Some(prior) = self.continuing else {
            return self;
        };
        Self {
            continuing: next.continuing.map(|count| (prior + count).min(2)),
            maximum: self.maximum.max((prior + next.maximum).min(2)),
        }
    }

    fn alternative(self, other: Self) -> Self {
        Self {
            continuing: self.continuing.max(other.continuing),
            maximum: self.maximum.max(other.maximum),
        }
    }
}

fn expression_writes(value: &Expr, out: usize) -> Writes {
    match &value.kind {
        ExprKind::Block { statements, value } => statement_writes(statements, out).followed_by(
            value
                .as_deref()
                .map_or(Writes::NONE, |v| expression_writes(v, out)),
        ),
        ExprKind::If {
            condition,
            then,
            otherwise,
            ..
        } => expression_writes(condition, out).followed_by(
            expression_writes(then, out).alternative(
                otherwise
                    .as_deref()
                    .map_or(Writes::NONE, |v| expression_writes(v, out)),
            ),
        ),
        _ => Writes::NONE,
    }
}

fn statement_writes(statements: &[Stmt], out: usize) -> Writes {
    statements.iter().fold(Writes::NONE, |writes, statement| {
        let next = match statement {
            Stmt::Assign { local, value, .. } if *local == out => expression_writes(value, out)
                .followed_by(Writes {
                    continuing: Some(1),
                    maximum: 1,
                }),
            Stmt::Return { value, .. } => Writes {
                continuing: None,
                maximum: usize::from(value.is_some()),
            },
            Stmt::Expr(value)
            | Stmt::Assign { value, .. }
            | Stmt::Declare {
                value: Some(value), ..
            } => expression_writes(value, out),
            Stmt::For { body, .. } | Stmt::While { body, .. } => {
                let iteration = statement_writes(body, out);
                let repeats = iteration.continuing.unwrap_or(0) > 0;
                Writes {
                    continuing: Some(usize::from(repeats) * 2),
                    maximum: if repeats { 2 } else { iteration.maximum },
                }
            }
            Stmt::Switch { cases, default, .. } => cases.iter().fold(
                default
                    .as_deref()
                    .map_or(Writes::NONE, |body| statement_writes(body, out)),
                |paths, case| paths.alternative(statement_writes(&case.body, out)),
            ),
            _ => Writes::NONE,
        };
        writes.followed_by(next)
    })
}

// Keyed deltas can cancel inside a keyed child. A tuple child has a
// different delta representation and keeps its existing publication path.
fn staged_shape(ty: &Type) -> bool {
    match ty {
        Type::Dict { value, .. } => staged_shape(value),
        Type::Set(_) | Type::Scalar(_) => true,
        _ => false,
    }
}

fn stages_writes(selected: &[(&Handler<'_>, Location)], out: usize) -> bool {
    let mut eligible = true;
    let mut writes = Writes::NONE;
    for (handler, _) in selected {
        let mut check = |node| match node {
            Node::Expression(Expr {
                kind:
                    ExprKind::Endpoint {
                        operation,
                        inputs: Some(inputs),
                    },
                ..
            }) if inputs.contains(&out) => {
                eligible &= matches!(
                    operation,
                    EndpointOperation::Valid
                        | EndpointOperation::AllValid
                        | EndpointOperation::Modified
                        | EndpointOperation::LastModified
                        | EndpointOperation::DeltaValue
                );
            }
            Node::Expression(Expr {
                kind: ExprKind::Local(local) | ExprKind::EndpointOf(local),
                ..
            }) if *local == out => eligible = false,
            _ => {}
        };
        walk_statement_nodes(handler.body, &mut check);
        for condition in &handler.conditions {
            condition.walk_nodes(&mut check);
        }
        let when = statement_writes(handler.body, out);
        writes = writes.followed_by(when.alternative(Writes::NONE));
    }
    eligible && writes.maximum >= 2
}

impl Family for Keyed {
    fn schema(&self, emitter: &mut Emitter<'_>, ty: &Type) -> Option<String> {
        schema(emitter, ty)
    }

    // FN-3: the ordinary collection publishes once in its keyed form.
    fn constant(&self, emitter: &mut Emitter<'_>, ty: &Type, value: &str) -> Option<String> {
        let schema = schema(emitter, ty)?;
        let written = cpp_type(emitter.program, ty)?;
        Some(format!(
            "::{}::constant<{written}, {schema}>({WIRING}, {value})",
            runtime::NAMESPACE
        ))
    }

    // TIME-1: "A dictionary and an unsized list hold what they hold: a
    // read of one whole requires its `valid` and no more", as a set's does.
    fn read(&self, emitter: &mut Emitter<'_>, ty: &Type, view: &str) -> Option<String> {
        schema(emitter, ty)?;
        Some(format!("::{}::keyed_value({view})", runtime::NAMESPACE))
    }

    // TIME-7: "a complete value of the result's type T, published whole,
    // or a `delta<T>` (TYPE-16), published as that change": the library
    // tells the two apart by their C++ types.
    fn publish(
        &self,
        emitter: &mut Emitter<'_>,
        ty: &Type,
        view: &str,
        value: &str,
    ) -> Option<String> {
        schema(emitter, ty)?;
        Some(format!(
            "::{}::keyed_publish({view}, {value});",
            runtime::NAMESPACE
        ))
    }

    // END-5 updates one publication through the evaluation; TIME-7
    // leaves a valid keyed result silent if those changes cancel.
    fn evaluation(
        &self,
        emitter: &mut Emitter<'_>,
        ty: &Type,
        selected: &[(&Handler<'_>, Location)],
        handlers: &str,
    ) -> Option<String> {
        schema(emitter, ty)?;
        let context = emitter.temporal.as_ref()?;
        let out = context.roles.iter().position(|role| *role == Role::Out)?;
        if !staged_shape(ty)
            || context
                .signature
                .as_ref()
                .is_some_and(|signature| signature.reference)
            || !stages_writes(selected, out)
        {
            return None;
        }
        let indent = INDENT.repeat(2);
        let inner: String = handlers
            .lines()
            .map(|line| format!("{INDENT}{line}\n"))
            .collect();
        let library = runtime::NAMESPACE;
        Some(format!(
            "{indent}::{library}::KeyedEvaluation<decltype({OUT})> keyed_evaluation_{{{OUT}}};\n\
             {indent}keyed_evaluation_.run([&](auto& {OUT}) {{\n{inner}{indent}}});\n"
        ))
    }

    // END-3, TYPE-16: a set's delta "with the properties `added` and
    // `removed`", a dictionary's with `updated` and `removed`; ERR-14 has
    // the read take its form from its place.
    fn delta(
        &self,
        emitter: &mut Emitter<'_>,
        ty: &Type,
        view: &str,
        checked: bool,
    ) -> Option<String> {
        schema(emitter, ty)?;
        let native = if checked {
            "fn_delta_value_checked"
        } else {
            "fn_delta_value"
        };
        Some(format!("::{}::{native}({view})", runtime::NAMESPACE))
    }

    // TST-5: a cell is "written as a `return` is": a set or a dictionary
    // whole, or a delta of one.
    fn cell(
        &self,
        emitter: &mut Emitter<'_>,
        scope: &mut Scope<'_>,
        ty: &Type,
        value: &Expr,
    ) -> Option<String> {
        schema(emitter, ty)?;
        let written = emitter.expression(scope, value);
        if !value.ty.same_type(ty) {
            return Some(written);
        }
        let whole = cpp_type(emitter.program, ty)?;
        Some(format!(
            "{}<{whole}>{{{written}}}",
            super::names::testing("Whole")
        ))
    }

    // TIME-9: "`all_valid` is true where every one is, by its own
    // `valid`, one level deep": of a dictionary in a hook. TIME-1: an
    // entry of a dictionary input, `book[key]`, is an endpoint whether or
    // not it is valid, read checked when it runs (EXPR-7).
    fn expression(
        &self,
        emitter: &mut Emitter<'_>,
        scope: &mut Scope<'_>,
        value: &Expr,
    ) -> Option<String> {
        if !matches!(emitter.kind(), Some(Kind::Hook(_))) {
            return None;
        }
        let library = runtime::NAMESPACE;
        match &value.kind {
            ExprKind::Endpoint {
                operation: EndpointOperation::AllValid,
                inputs: Some(inputs),
            } => {
                let [local] = inputs.as_slice() else {
                    return None;
                };
                let ty = match emitter.role(*local) {
                    Role::Out => emitter.temporal.as_ref()?.result.clone()?,
                    Role::Input => scope.body.locals[*local].ty.clone(),
                    _ => return None,
                };
                if !matches!(ty, Type::Dict { .. }) || schema(emitter, &ty).is_none() {
                    return None;
                }
                let view = emitter.view(&scope.body.locals, *local)?;
                Some(format!("::{library}::fn_all_valid({view})"))
            }
            // END-1 to END-3, END-6 of an entry (TIME-9).
            ExprKind::ChildEndpoint { operation, child } if !nested_dictionary_entry(child) => {
                let (view, key) = match entry_of(emitter, scope, child)? {
                    Ok(entry) => entry,
                    Err(reported) => return Some(reported),
                };
                let native = match operation {
                    EndpointOperation::Valid => "keyed_entry_valid",
                    EndpointOperation::Modified => "keyed_entry_modified",
                    EndpointOperation::LastModified => "keyed_entry_last_modified",
                    EndpointOperation::DeltaValue => "keyed_entry_delta",
                    _ => {
                        return Some(emitter.missing(
                            value.at,
                            &format!("`{}` of a dictionary's entry", operation.name()),
                        ));
                    }
                };
                scope.acted = true;
                Some(format!("::{library}::{native}({view}, {key})"))
            }
            // ERR-14: the delta of an entry where its place takes a
            // `result`.
            ExprKind::CheckedDelta(read) => {
                let ExprKind::ChildEndpoint {
                    operation: EndpointOperation::DeltaValue,
                    child,
                } = &read.kind
                else {
                    return None;
                };
                if nested_dictionary_entry(child) {
                    return None;
                }
                let (view, key) = match entry_of(emitter, scope, child)? {
                    Ok(entry) => entry,
                    Err(reported) => return Some(reported),
                };
                scope.acted = true;
                Some(format!(
                    "::{library}::keyed_entry_delta_checked({view}, {key})"
                ))
            }
            // TIME-1: "a child that is not valid fails it
            // (`value.unset_read`), or gives an `err` where its place takes
            // a `result`": a read named when compiled is checked too.
            ExprKind::CheckedChild { read, result } => {
                let (view, key) = match entry_of(emitter, scope, read)? {
                    Ok(entry) => entry,
                    Err(reported) => return Some(reported),
                };
                scope.read = true;
                scope.acted = true;
                let native = if *result {
                    "keyed_entry_result"
                } else {
                    "keyed_entry_value"
                };
                Some(format!("::{library}::{native}({view}, {key})"))
            }
            ExprKind::Index { .. } => {
                let (view, key) = match entry_of(emitter, scope, value)? {
                    Ok(entry) => entry,
                    Err(reported) => return Some(reported),
                };
                scope.read = true;
                scope.acted = true;
                Some(format!("::{library}::keyed_entry_value({view}, {key})"))
            }
            _ => None,
        }
    }
}

/// An observation of an entry in a nested dictionary uses the endpoint
/// family's recursive view, which also represents a missing parent (END-8).
fn nested_dictionary_entry(child: &Expr) -> bool {
    matches!(
        &child.kind,
        ExprKind::Index { base, .. }
            if matches!(base.ty, Type::Dict { .. }) && matches!(base.kind, ExprKind::Index { .. })
    )
}

/// The view of the dictionary input and the C++ of the key, where `read`
/// is the entry `book[key]` of a temporal input of a dictionary this
/// family writes; `None` where it is not. An entry of an entry,
/// `book[a][b]`, is reported, and its stand-in given as the error.
fn entry_of(
    emitter: &mut Emitter,
    scope: &mut Scope,
    read: &Expr,
) -> Option<Result<(String, String), String>> {
    let ExprKind::Index { base, index } = &read.kind else {
        return None;
    };
    let (local, nested) = root(base)?;
    if emitter.role(local) != Role::Input {
        return None;
    }
    let declared = &scope.body.locals[local].ty;
    if !matches!(declared, Type::Dict { .. }) || schema(emitter, declared).is_none() {
        return None;
    }
    if nested {
        return Some(Err(
            emitter.missing(read.at, "an entry of a dictionary's entry")
        ));
    }
    let view = emitter.view(&scope.body.locals, local)?;
    let key = emitter.expression(scope, index);
    Some(Ok((view, key)))
}

/// The local a chain of indices is rooted at, and whether `base` is
/// itself an index of it.
fn root(base: &Expr) -> Option<(usize, bool)> {
    match &base.kind {
        ExprKind::Local(local) => Some((*local, false)),
        ExprKind::Index { base, .. } => root(base).map(|(local, _)| (local, true)),
        _ => None,
    }
}
