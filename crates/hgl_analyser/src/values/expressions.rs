//! An expression, typed: a literal, a name and the read of a local, a
//! tuple, a sequence and its pairs, and what a condition proves of a
//! nullable binding (EXPR-3, EXPR-5, EXPR-9, ERR-12). Part of the typing
//! pass ([`super`]); [`Typer::expression`] hands every other form to the
//! module that reads it.

use hgl_diagnostics::Category;
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode};

use super::control::{Outcome, ValueUse, branch_is_null, branch_takes_its_type};
use super::fit::{
    accepts_float, fitted_input_pattern, hook_payload_type, is_integer_literal, not_null, nullable,
    open, whole,
};
use super::node::RefRead;
use super::results::proof_binding;
use super::{Fact, Typer, children};
use crate::hir::{
    BinaryOperator, EndpointOperation, Expr, ExprKind, Hook, LocalId, UnaryOperator, Value,
    ValueCategory,
};
use crate::program::{Binding, LocalKind};
use crate::syntax::{child, name_token, tokens, ungrouped};
use crate::types::{Argument, ConstantValue, Detail, GenericKind, Scalar, Type};
use crate::{codes, error};

#[derive(Clone, Copy)]
pub(super) enum ExpressionFit {
    Value,
    InputPattern,
}

impl ExpressionFit {
    pub(super) fn input_type(self, supplied: &Expr, expected: &Type) -> Option<Type> {
        match self {
            Self::Value => None,
            Self::InputPattern => fitted_input_pattern(supplied, expected),
        }
    }
}

impl<'a> Typer<'a> {
    // ---- Expressions ----

    /// The expression `node`, typed. `expected` is the type its place
    /// expects, which gives a literal or a sequence its type (EXPR-5);
    /// whether the result fits is for [`Typer::fit`].
    ///
    /// What is read inside it, a block's statements among them, is part
    /// of the construct around it, which may read it again
    /// ([`Typer::begin_construct`]): so it is counted while it is read.
    pub(super) fn expression(&mut self, node: &SyntaxNode, expected: Option<&Type>) -> Expr {
        self.expression_in_flow(node, expected, ValueUse::Required)
    }

    /// Read an expression whose control-flow context may discard its result.
    pub(super) fn expression_in_flow(
        &mut self,
        node: &SyntaxNode,
        expected: Option<&Type>,
        value_use: ValueUse,
    ) -> Expr {
        self.expression_with_fit(node, expected, value_use, ExpressionFit::Value)
    }

    /// Check a public operator input pattern through its written literal
    /// parts and through block and branch values. The candidate still
    /// reads the operand in its own context.
    pub(super) fn expression_for_input_pattern(&mut self, node: &SyntaxNode, ty: &Type) -> Expr {
        self.read_literal_part(node, ty, ExpressionFit::InputPattern)
    }

    pub(super) fn expression_with_fit(
        &mut self,
        node: &SyntaxNode,
        expected: Option<&Type>,
        value_use: ValueUse,
        fit: ExpressionFit,
    ) -> Expr {
        self.expressing += 1;
        let reported = self.diagnostics.len();
        let mut value = self.expression_read(node, expected, value_use, fit);
        // A call with a settled type already has a result. Only an
        // unknown type needs the callee's signature to distinguish absence.
        if value_use == ValueUse::Required
            && value.ty == Type::Unknown
            && self.diagnostics.len() == reported
            && let ExprKind::Call { callee, .. } = &value.kind
            && self.call_has_no_result(callee)
        {
            let rule = if callee.generics().contains(&Argument::NoResult) {
                "GEN-1"
            } else {
                "FN-7"
            };
            self.diagnostics.emit(error(
                Category::Type,
                codes::TYPE_MISMATCH,
                value.at,
                "this call gives no result and cannot be used as a value".to_string(),
                rule,
            ));
        }
        self.expressing -= 1;
        if self
            .unsettled_results
            .is_some_and(|calls| calls.contains(&value.at))
        {
            value.ty = Type::Unknown;
        }
        match expected {
            Some(expected) => self.endpoint_read_form(value, expected),
            None => value,
        }
    }

    /// [`Typer::expression`], counted.
    fn expression_read(
        &mut self,
        node: &SyntaxNode,
        expected: Option<&Type>,
        value_use: ValueUse,
        fit: ExpressionFit,
    ) -> Expr {
        // TIME-9: a place that expects an endpoint is given one, and no
        // value: the argument of an `endpoint<T>` parameter.
        if let Some(Type::Endpoint(_)) = expected {
            return self.endpoint_given(node);
        }
        // TYPE-16: "A `delta<V>` is a value where a `delta<V?>` is
        // expected". A form that takes its type from its place is read
        // as the `delta<V>`: its entries and its elements are held to
        // that type, a list's delta being a dictionary. The place may
        // be one that can be left out, a `delta<V?>?`, as a struct's
        // delta has for a field. A block and an `if` keep the place's
        // own type, so that a branch may give the whole `delta<V?>`,
        // and each value in them is read here in its turn. `null` is
        // no `delta<V>`: it is left to say so, or to leave the place
        // out.
        if let Some(Type::Delta(of)) = not_null(expected)
            && let Type::Nullable(payload) = &**of
            // A literal, a tuple, a sequence and a symbol before an
            // operand; and `.member`, `ok(value)` and `err(value)`,
            // which have no type but their place's (ERR-2).
            && (matches!(
                node.kind(),
                SyntaxKind::Literal
                    | SyntaxKind::TupleExpr
                    | SyntaxKind::SequenceExpr
                    | SyntaxKind::UnaryExpr
            ) || takes_its_type(node))
            && !written_null(node)
        {
            let lifted = Type::delta((**payload).clone());
            return self.read_literal_part(node, &lifted, fit);
        }
        let at = self.at(node);
        let (kind, ty) = match node.kind() {
            SyntaxKind::Literal => self.literal(node, expected),
            SyntaxKind::TypeOfExpr => {
                let (ty, _) = self.type_query_operand(node);
                (
                    ExprKind::Literal(Value::Type(Box::new(ty))),
                    Type::TypeValue,
                )
            }
            SyntaxKind::NameExpr => return self.name(node),
            // "Parentheses select grouping" (EXPR-1): the expression
            // inside, where it stands.
            SyntaxKind::ParenExpr => {
                if let Some(inner) = node.children().next() {
                    return self.expression_with_fit(&inner, expected, value_use, fit);
                }
                (ExprKind::Unread, Type::Unknown)
            }
            SyntaxKind::TupleExpr => self.tuple(node, expected, fit),
            SyntaxKind::SequenceExpr => self.sequence(node, expected, fit),
            SyntaxKind::ListComprehension => self.list_comprehension(node, expected, fit),
            SyntaxKind::PackComprehension => {
                self.check_pack_comprehension(node);
                (ExprKind::Unread, Type::Unknown)
            }
            SyntaxKind::PackKeyMembership => self.pack_key_membership(node),
            SyntaxKind::UnaryExpr => self.unary(node, expected),
            SyntaxKind::BinaryExpr => self.binary(node, expected),
            SyntaxKind::CallExpr => self.call(node, expected, value_use.is_used(), fit),
            SyntaxKind::TypedConstructor => self.typed_constructor(node, expected),
            SyntaxKind::DeltaConstructor => self.delta_constructor(node),
            SyntaxKind::UndecidedConstructor => {
                // GEN-14: the resolver decided what the node is. One
                // that is comparisons is for a later part.
                let constructor = self
                    .module
                    .decisions
                    .iter()
                    .any(|decision| decision.at == at && decision.constructor == Some(true));
                if constructor {
                    self.typed_constructor(node, expected)
                } else {
                    (ExprKind::Unread, Type::Unknown)
                }
            }
            SyntaxKind::MemberExpr => self.member(node),
            SyntaxKind::TypeCall => self.type_call(node),
            // The body has its own scope and result (FN-7, FN-14).
            // The function itself has no general callable type.
            SyntaxKind::FnExpr => {
                let parameters = self.anonymous_parameters(node);
                let result = child(node, SyntaxKind::ReturnType)
                    .and_then(|annotation| self.anonymous_type(&annotation))
                    .unwrap_or(Type::Unknown);
                self.anonymous_body(node, parameters, &result);
                (ExprKind::Unread, Type::Unknown)
            }
            SyntaxKind::ImpliedMember => self.implied_member(node, expected),
            SyntaxKind::IndexExpr => self.index(node),
            // TST-3: "Eval traces are test-local values." An `eval`
            // anywhere but in a named test is not read.
            SyntaxKind::EvalExpr if self.testing => self.eval(node),
            // TST-17: "beside `eval` it writes `run call` (RUN-3),
            // whose value is the run's output as an eval's is."
            SyntaxKind::RunExpr if self.testing => self.eval(node),
            // A block or an `if` whose value is used.
            SyntaxKind::Block | SyntaxKind::IfExpr => {
                let (value, outcome) = self.flow_with_fit(node, expected, value_use, fit);
                // CTRL-10: "A block whose last statement is an
                // expression has that expression's value and type; any
                // other block has none."
                if value_use == ValueUse::Required && matches!(outcome, Outcome::Nothing) {
                    self.diagnostics.emit(error(
                        Category::Type,
                        codes::TYPE_MISMATCH,
                        at,
                        "this has no value: a block's value is its last statement, when that \
                         is an expression"
                            .to_string(),
                        "CTRL-10",
                    ));
                }
                // TYPE-14: what is null alone is of the type its place
                // expects, and rejects where nothing gives it one.
                if matches!(outcome, Outcome::Null) {
                    let mut value = value;
                    match expected {
                        Some(expected) if !open(expected) => {
                            value.ty = nullable(expected.clone());
                        }
                        Some(_) => {}
                        None => self.diagnostics.emit(error(
                            Category::Type,
                            codes::NULL_CONTEXT,
                            at,
                            "this is null when its condition does not hold, and nothing here \
                             gives that a type: write the type"
                                .to_string(),
                            "TYPE-14",
                        )),
                    }
                    return value;
                }
                return value;
            }
            _ => (ExprKind::Unread, Type::Unknown),
        };
        if matches!(kind, ExprKind::Unread) && node.kind() != SyntaxKind::FnExpr {
            self.forget();
        }
        if self.graph {
            self.constants_of(&kind);
        }
        let value = Expr {
            kind,
            ty,
            at,
            category: ValueCategory::Unsettled,
        };
        // TIME-1: a read in a hook of a temporal input's child.
        if matches!(node.kind(), SyntaxKind::IndexExpr | SyntaxKind::MemberExpr) {
            return self.child_read(node, value);
        }
        value
    }

    /// TYPE-18: check a cold `type(e)` operand without runtime effects,
    /// preserving operand diagnostics while discarding local mutations.
    pub(super) fn type_query_operand(
        &mut self,
        query: &SyntaxNode,
    ) -> (Type, crate::constant::QueryType) {
        let at = self.at(query);
        if let Some(known) = self.constants.remembered_type(at) {
            let ty = match &known {
                crate::constant::QueryType::Resolved(resolved) => resolved.clone(),
                crate::constant::QueryType::Reported
                | crate::constant::QueryType::InstanceDependent(_)
                | crate::constant::QueryType::Unresolved => Type::Unknown,
            };
            return (ty, known);
        }
        let mark = self.mark();
        // A cold block may assign an existing local, including
        // its graph category or capability alias (TYPE-18).
        let locals = self.locals.clone();
        let aliases = self.aliases.clone();
        let returns = (self.must_return, self.returning_tails.clone());
        let lifecycle = (self.hook, self.initializing, self.caching);
        let querying = self.type_query;
        let enclosing_selection = std::mem::take(&mut self.nested_query_selects);
        self.type_query = true;
        self.hook = None;
        self.initializing = false;
        self.caching = false;
        let operand = query
            .children()
            .next()
            .map(|node| self.expression(&node, None));
        let provisional_result = operand
            .as_ref()
            .and_then(|value| self.provisional_type(value, mark.selecting))
            .is_some();
        let ty = operand.map_or(Type::Unknown, |value| value.ty);
        let new_errors = self.diagnostics.as_slice()[mark.diagnostics..]
            .iter()
            .any(|report| report.severity == hgl_diagnostics::Severity::Error);
        // The outer descriptor can contain this query as a nominal argument.
        // Retain its need for selection without adding its calls to runtime HIR.
        let selects = provisional_result || self.nested_query_selects;
        self.nested_query_selects = enclosing_selection || (querying && selects);
        // TYPE-18 evaluates nothing of the operand. Its local
        // effects and deferred calls do not belong to the body.
        self.restore_read(mark, false);
        self.locals = locals;
        self.aliases = aliases;
        (self.must_return, self.returning_tails) = returns;
        (self.hook, self.initializing, self.caching) = lifecycle;
        self.type_query = querying;
        let has_unknown = ty.any_part(&|part| matches!(part, Type::Unknown), &|_| false);
        let result = if new_errors {
            crate::constant::QueryType::Reported
        } else if !querying
            && selects
            && self.instance.is_empty()
            && self.pack_inputs.is_none()
            && !self.signature.generics.is_empty()
        {
            // GEN-7, TYPE-18: an open generic body's provisional operator
            // result cannot settle a Type-valued argument or its requirements.
            // Reread the cold operand with the concrete body and its locals.
            crate::constant::QueryType::InstanceDependent(
                self.signature
                    .generics
                    .iter()
                    .map(super::type_queries::query_binding)
                    .collect(),
            )
        } else if has_unknown {
            if self.diagnostics.has_errors() {
                crate::constant::QueryType::Reported
            } else if !self.signature.generics.is_empty()
                && self.instance.is_empty()
                && self.pack_inputs.is_none()
            {
                // TYPE-18, GEN-8: the operand of a cold query in a generic
                // body may select its result only in the concrete instance.
                // It still supplies a Type-valued constant here. Preserve the
                // partial operand type; the instance rereads the lexical query.
                crate::constant::QueryType::Resolved(ty.clone())
            } else {
                crate::constant::QueryType::Unresolved
            }
        } else if crate::constant::type_has_deferred_query(&ty) {
            crate::constant::QueryType::Unresolved
        } else {
            crate::constant::QueryType::Resolved(ty.clone())
        };
        let previous = self.constants.remember_type(at, result.clone());
        self.type_query_locations.push((at, previous));
        let value_ty = match &result {
            crate::constant::QueryType::Resolved(resolved) => resolved.clone(),
            crate::constant::QueryType::Reported
            | crate::constant::QueryType::InstanceDependent(_)
            | crate::constant::QueryType::Unresolved => Type::Unknown,
        };
        (value_ty, result)
    }

    /// Pre-evaluates every outermost `type(...)` query inside `node` in the
    /// current lexical scope so `read_constant` reads their checked types.
    pub(super) fn remember_type_queries_in(&mut self, node: &SyntaxNode) {
        let mut queries = Vec::new();
        super::type_queries::outermost_type_queries(node, &mut queries);
        for query in &queries {
            self.type_query_operand(query);
        }
    }

    /// What a form this pass does not read assigns is not followed, so
    /// every `var` may have a value after it.
    pub(super) fn forget(&mut self) {
        self.assigned.fill(true);
    }

    /// A literal, with the type its form gives it. `null` has the type
    /// its place expects, which is a nullable one (TYPE-14).
    fn literal(&mut self, node: &SyntaxNode, expected: Option<&Type>) -> (ExprKind, Type) {
        let Some(token) = tokens(node).next() else {
            return (ExprKind::Unread, Type::Unknown);
        };
        // A literal the lexer or `forms` has reported has no value
        // here, and no type: it is reported once (ERR-9). Its value is
        // read as every constant's is (`crate::constant::literal`).
        let read = match token.kind() {
            SyntaxKind::KwNull => {
                return match expected {
                    // TYPE-14: "`T?` denotes an ordinary value that is
                    // either a T or null."
                    Some(expected @ Type::Nullable(_)) => {
                        (ExprKind::Literal(Value::Null), expected.clone())
                    }
                    // "It does not make null an admitted value of
                    // unqualified T."
                    Some(expected) if !open(expected) => {
                        let report = error(
                            Category::Type,
                            codes::TYPE_MISMATCH,
                            self.at(node),
                            format!("`null` is no value of `{expected}`: write `{expected}?`"),
                            "TYPE-14",
                        );
                        // TYPE-16: null in a delta's place is nothing
                        // published, and a published null is the delta
                        // that holds one.
                        let optional = matches!(expected, Type::Delta(of)
                            if matches!(**of, Type::Nullable(_)));
                        self.diagnostics.emit(if optional {
                            report.with_note(format!("a published null is `{expected}()`"))
                        } else {
                            report
                        });
                        (ExprKind::Literal(Value::Null), Type::Unknown)
                    }
                    // What the place expects is not known.
                    Some(_) => (ExprKind::Literal(Value::Null), Type::Unknown),
                    // "`null` has the nullable type its place expects,
                    // and rejects where nothing gives it one."
                    None => {
                        self.diagnostics.emit(error(
                            Category::Type,
                            codes::NULL_CONTEXT,
                            self.at(node),
                            "`null` has the type its place expects, and nothing here gives \
                             one: write the type"
                                .to_string(),
                            "TYPE-14",
                        ));
                        (ExprKind::Literal(Value::Null), Type::Unknown)
                    }
                };
            }
            _ => crate::constant::literal(&token),
        };
        match read {
            Some((value, scalar)) => (ExprKind::Literal(value), Type::Scalar(scalar)),
            None => (ExprKind::Unread, Type::Unknown),
        }
    }

    /// The local a name is, when it names a parameter or a local this
    /// pass has read.
    pub(super) fn local_of(&self, node: &SyntaxNode) -> Option<LocalId> {
        let name = name_token(node)?;
        match self.bindings.get(&(self.file, name.span().start))? {
            // FN-14 and ERR-9: a forbidden capture was already reported
            // by the resolver, so it supplies no type to this use.
            Binding::Local { at, kind } => self.declared.get(at).copied().filter(|&local| {
                !self.is_capture(local) || (kind.capture_allowed() && !self.is_capability(local))
            }),
            Binding::Declarations { .. } => None,
        }
    }

    /// A name: a parameter, a local, a constant of a module, or a
    /// generic value parameter. Any other, a function or a type or a
    /// name left for the prelude, is for the pass that reads calls and
    /// members.
    fn name(&mut self, node: &SyntaxNode) -> Expr {
        let at = self.at(node);
        let unread = Expr {
            kind: ExprKind::Unread,
            ty: Type::Unknown,
            at,
            category: ValueCategory::Unsettled,
        };
        if let Some(local) = self.local_of(node) {
            if self.names_pack(local) {
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::TYPE_MISMATCH,
                    at,
                    format!(
                        "`{}` is a pack and cannot be read as an ordinary value",
                        self.locals[local].name
                    ),
                    "FN-19",
                ));
                return unread;
            }
            if self.reject_function_value(local, at, None) {
                return unread;
            }
            return self.read_local(local, node);
        }
        let Some(name) = name_token(node) else {
            return unread;
        };
        // UNIT-13: "A constant is read by its name wherever an ordinary
        // value is." Its type is the one it is written with.
        if let Some((module, &[declaration])) = self.declarations(&name)
            && let Some(Detail::Constant(ty)) =
                self.program.modules[module].details.get(declaration)
        {
            return Expr {
                kind: ExprKind::Constant {
                    module,
                    declaration,
                },
                ty: ty.clone(),
                at,
                category: ValueCategory::Unsettled,
            };
        }
        let Some(Binding::Local { at: declared, kind }) =
            self.bindings.get(&(self.file, name.span().start))
        else {
            return unread;
        };
        if *kind == LocalKind::PackIterationKey {
            // CTRL-16: a computed pass binds its key as a constant, also
            // when an anonymous body inside the pass reads the key.
            let literal = self
                .bound_pack_key(*declared)
                .and_then(|value| match value {
                    ConstantValue::Int(position) => Some((Value::Int(*position), Scalar::I64)),
                    ConstantValue::Str(name) => Some((Value::Str(name.clone()), Scalar::Str)),
                    _ => None,
                });
            return literal.map_or(unread, |(value, scalar)| Expr {
                kind: ExprKind::Literal(value),
                ty: Type::Scalar(scalar),
                at,
                category: ValueCategory::Unsettled,
            });
        }
        if *kind != LocalKind::GenericParameter {
            return unread;
        }
        let Some(generic) = self
            .signature
            .generics
            .iter()
            .find(|generic| generic.at == *declared)
        else {
            return unread;
        };
        let GenericKind::Value(ty) = &generic.kind else {
            return unread;
        };
        // GEN-7, GEN-13 and TYPE-10: a signature's list size is read as
        // an integer constant. Other value parameters use their declared type.
        let ty = if generic.inferred_list_size {
            Type::Scalar(Scalar::I64)
        } else {
            ty.clone()
        };
        Expr {
            kind: self
                .instance
                .iter()
                .find_map(|(parameter, argument)| {
                    // A contextual argument reread in an instance can
                    // use a value the instance has already supplied.
                    if parameter == declared
                        && let Argument::Constant(given) = argument
                    {
                        self.program.constant(&given.value, &ty, at)
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| ExprKind::GenericValue {
                    name: name.text().to_string(),
                    at: *declared,
                }),
            ty,
            at,
            category: ValueCategory::Unsettled,
        }
    }

    /// A read of the local `local`, written at `node`. EXPR-3: "every
    /// path reaching a read must assign it."
    pub(super) fn read_local(&mut self, local: LocalId, node: &SyntaxNode) -> Expr {
        let at = self.at(node);
        // TYPE-18 obtains a declared type without evaluating the local.
        if !self.type_query && !self.assigned[local] {
            self.diagnostics.emit(
                error(
                    Category::Name,
                    codes::BINDING_UNASSIGNED,
                    at,
                    format!(
                        "`{}` is read before it is given a value",
                        self.locals[local].name
                    ),
                    "EXPR-3",
                )
                .with_related(self.locals[local].at, "declared here"),
            );
            // One report for one `var`.
            self.assigned[local] = true;
        }
        if self.bound_children.contains_key(&local)
            && let Type::Endpoint(payload) = &self.locals[local].ty
        {
            let value = self.child_read(
                node,
                Expr {
                    kind: ExprKind::BoundChild(local),
                    ty: (**payload).clone(),
                    at,
                    category: ValueCategory::Unsettled,
                },
            );
            // ERR-12 refines the value read by this alias. Where the
            // alias names its endpoint, ERR-14 still takes the locator.
            if self.naming != Some(at)
                && self.binding_child != Some(at)
                && self.present.contains(&Fact::Present(local))
                && let Type::Nullable(inner) = &value.ty
            {
                return Expr {
                    ty: (**inner).clone(),
                    kind: ExprKind::PresentChild(Box::new(value)),
                    at,
                    category: ValueCategory::Unsettled,
                };
            }
            return value;
        }
        if self.is_temporal_input(local)
            && (self.hook.is_some() || self.initializing || self.caching)
        {
            // TIME-1: where a child is read, `parts[1]`, the read is the
            // child's, judged where the child is (`child_read`); and a
            // child named as an endpoint, `valid(parts[1])`, is not read.
            let child = self.reads_children() && self.continues_chain(node, &self.locals[local].ty);
            if !child && self.naming != Some(at) {
                // FN-16: "A `ref` parameter and its children ... are
                // endpoints, not values". A child of one is reported where
                // it is read (`child_read`).
                if self.is_reference(local) && self.reads_children() {
                    self.ref_read(local, None, RefRead::Value, at);
                } else {
                    self.payload_read(local, at);
                }
            }
        }
        // FN-16, proposed: "a read of its value rejects", of `out` of a
        // `ref` result, a part of it among them.
        if Some(local) == self.out
            && self.signature.reference
            && self.reads_children()
            && self.naming != Some(at)
        {
            self.ref_read(local, None, RefRead::Value, at);
        }
        self.signal_read(local, at, None);
        // TIME-7: "`out` is read in a `when` and in `stop` ... `start`
        // neither reads nor writes it". Nor does a `state`'s or a
        // `cache`'s initializer, which gives it its first value on
        // every start (TIME-6), before the node has published.
        if Some(local) == self.out {
            if self.hook == Some(Hook::Start) {
                self.out_lifecycle(
                    at,
                    "`out` is neither read nor written in `start`".to_string(),
                );
            } else if self.initializing || self.caching {
                self.out_lifecycle(
                    at,
                    "`out` is read in a `when` and in `stop`, and a `state` or a `cache` is \
                     given its first value on every start, before the node has published"
                        .to_string(),
                );
            }
        }
        // TIME-3: the node's scheduler or alarm used in `start` is what
        // may wake a handler that subscribes to no input. "Any use in
        // `start` counts; what a function given the capability does
        // with it is not followed."
        if self.hook == Some(Hook::Start)
            && self.capability(local).is_some_and(|capability| {
                let injected = &self.locals[capability];
                injected.kind == LocalKind::Inject
                    && matches!(injected.name.as_str(), "scheduler" | "alarm")
            })
        {
            self.driven = true;
        }
        // TIME-8: "A capability is used within the node that injects it
        // and is not retained". An injected capability other than `out`,
        // which is the node's result, and a parameter of a capability's
        // type are both one.
        if self.is_capability(local)
            && let Some(how) = self.retained(node)
        {
            self.diagnostics.emit(
                error(
                    Category::Injectable,
                    codes::CAPABILITY_AS_VALUE,
                    at,
                    format!(
                        "`{}` is a capability, and is not retained: this is {how}",
                        self.locals[local].name
                    ),
                    "TIME-8",
                )
                .with_note(
                    "a capability is bound to a `let`, given to a parameter of its type, or \
                     read through a property, `clock.evaluation_time`",
                ),
            );
        }
        // ERR-12: "Refine that same binding to T on paths proving a
        // non-null T? value".
        let present = self.present.contains(&Fact::Present(local));
        let ty = match &self.locals[local].ty {
            Type::Nullable(inner) if present => inner.as_ref().clone(),
            ty => ty.clone(),
        };
        if let Some((_, value)) = self
            .const_inputs
            .iter()
            .find(|(parameter, _)| *parameter == self.locals[local].at)
        {
            return Expr {
                at,
                ty,
                ..value.clone()
            };
        }
        let kind = if present && !whole(&ty) && self.deferred_presence(local) {
            ExprKind::DeferredPresent(local)
        } else {
            ExprKind::Local(local)
        };
        let endpoint = self.is_temporal_input(local) || Some(local) == self.out;
        let ty = if self.hook.is_some() && endpoint {
            hook_payload_type(&ty)
        } else {
            ty
        };
        Expr {
            kind,
            ty,
            at,
            category: ValueCategory::Unsettled,
        }
    }

    /// An ordinary binding may still be unknown where a generic
    /// graph is read. Keep its null test until this instance is known.
    fn deferred_presence(&self, local: LocalId) -> bool {
        self.graph && self.locals[local].category != ValueCategory::Connection
    }

    /// The bindings the condition `condition` proves are not null: those
    /// where it holds, and those where it does not.
    ///
    /// ERR-12: "An immutable nullable parameter or local may be tested
    /// directly against null." A `var` may be given a null after its
    /// test, and so is proved nothing of.
    pub(super) fn null_tests(&self, condition: &Expr) -> (Vec<Fact>, Vec<Fact>) {
        // ERR-12, ERR-3: "An immutable ... binding, a `let`, a
        // parameter, or an iteration binding".
        let immutable = |local: LocalId| {
            matches!(
                self.locals[local].kind,
                LocalKind::Parameter | LocalKind::Let | LocalKind::Iteration
            )
        };
        let payload = |local: LocalId| match &self.locals[local].ty {
            Type::Endpoint(payload) if self.bound_children.contains_key(&local) => payload,
            ty => ty,
        };
        let refinable = |local: LocalId| {
            immutable(local)
                && (matches!(payload(local), Type::Nullable(_))
                    || matches!(payload(local), Type::Unknown) && self.deferred_presence(local))
        };
        // ERR-3: "In composition a test of a connection is an ordinary
        // lifted call (FN-6): it gives a connection of `bool` and no
        // fact."
        // A binding written `result<T, E>?` that a null test has proved
        // present is read as its result (ERR-12), and is tested as one.
        // The proof may stand before the condition, or be the left
        // operand of the connective the test is the right operand of:
        // `outcome != null && is_ok(outcome)`.
        let tagged = |local: LocalId, known: &[Fact]| {
            let binding = &self.locals[local];
            let present = Fact::Present(local);
            let present = self.present.contains(&present) || known.contains(&present);
            let ty = match payload(local) {
                Type::Nullable(inner) if present => inner,
                ty => ty,
            };
            // A deferred projection retains this lexical fact and checks
            // that its read settled to an ordinary result binding.
            immutable(local)
                && match ty {
                    Type::Result { .. } => {
                        binding.category == ValueCategory::Value || self.deferred_presence(local)
                    }
                    Type::Unknown => self.deferred_presence(local),
                    _ => false,
                }
        };
        null_tests(
            condition,
            &Proofs {
                refinable: &refinable,
                tagged: &tagged,
                inputs: &|| self.temporal_inputs(),
                child_guard: &|operation, child| self.child_guard_facts(operation, child),
                packed: &|entry| self.pack_endpoint_selection(entry),
            },
            &[],
        )
    }

    /// The temporal inputs of the function being read: its parameters
    /// that are not `const` (FN-1). A parameter's local has its
    /// position among the parameters.
    pub(super) fn temporal_inputs(&self) -> Vec<LocalId> {
        (0..self.signature.parameters.len())
            .filter(|&local| self.is_temporal_input(local))
            .collect()
    }

    /// True when `local` is a temporal input of a temporal function.
    pub(super) fn is_temporal_input(&self, local: LocalId) -> bool {
        self.signature
            .parameters
            .get(local)
            .is_some_and(|parameter| {
                !parameter.constant && !matches!(parameter.ty, Type::Function(_))
            })
    }

    /// Read a literal component with the fit of the enclosing operand.
    pub(super) fn read_literal_part(
        &mut self,
        node: &SyntaxNode,
        ty: &Type,
        fit: ExpressionFit,
    ) -> Expr {
        self.expression_with_fit(node, Some(ty), ValueUse::Required, fit)
    }

    /// Fit a literal component after it has been read.
    pub(super) fn fit_literal_part(
        &mut self,
        value: Expr,
        expected: &Type,
        place: &str,
        fit: ExpressionFit,
    ) -> Expr {
        let pattern_type = fit.input_type(&value, expected);
        self.fit(value, pattern_type.as_ref().unwrap_or(expected), place)
    }

    /// `[key: value, ...]` (EXPR-9): "`[a: b]` has one pair entry, not a
    /// standalone pair value ... The use site and expected type
    /// constrain element types, keys, ordering, uniqueness, and
    /// placeholder admission; syntax alone does not select a
    /// dictionary, timed trace, or sparse delta."
    ///
    /// So pairs are a dictionary where one is expected: each key is
    /// read as its key type and each value as its value type. Where
    /// nothing is expected, nothing says what they are
    /// (`sequence.context_type`); where another type that is known is
    /// expected, the place reports that this is none of it.
    fn pairs(
        &mut self,
        node: &SyntaxNode,
        items: &[SyntaxNode],
        expected: Option<&Type>,
        unexpected: bool,
        fit: ExpressionFit,
    ) -> (ExprKind, Type) {
        let sides = match expected {
            Some(Type::Dict { key, value }) => Some(((**key).clone(), (**value).clone())),
            _ => None,
        };
        if sides.is_none() && unexpected {
            self.diagnostics.emit(
                error(
                    Category::Type,
                    codes::SEQUENCE_CONTEXT,
                    self.at(node),
                    "nothing here says what these pairs make: write the type".to_string(),
                    "EXPR-9",
                )
                .with_note("pairs are a dictionary where one is expected: `let book: dict<str, f64> = [\"a\": 1.0]`"),
            );
        }
        let unknown = Type::Unknown;
        let (key_type, value_type) = match &sides {
            Some((key, value)) => (key, value),
            None => (&unknown, &unknown),
        };
        let entries: Vec<(Expr, Expr)> = items
            .iter()
            .filter(|item| item.kind() == SyntaxKind::SequencePair)
            .filter_map(|pair| {
                let mut parts = pair.children();
                let (key, value) = (parts.next()?, parts.next()?);
                let key = self.read_literal_part(&key, key_type, fit);
                let value = self.read_literal_part(&value, value_type, fit);
                Some(if sides.is_some() {
                    let key_place = "a dictionary's key";
                    let value_place = "a dictionary's value";
                    (
                        self.fit_literal_part(key, key_type, key_place, fit),
                        self.fit_literal_part(value, value_type, value_place, fit),
                    )
                } else {
                    (key, value)
                })
            })
            .collect();
        if let Some(ty @ Type::Dict { .. }) = expected {
            self.check_collection_constants(
                items.iter().filter_map(|pair| pair.children().next()),
                ty,
            );
        }
        let ty = match (expected, &sides) {
            (Some(expected), Some(_)) => expected.clone(),
            // A place whose type is known and is no dictionary is given
            // a dictionary of what is not known, which it reports as
            // none of what it expects.
            (Some(expected), None) if !open(expected) => Type::Dict {
                key: Box::new(Type::Unknown),
                value: Box::new(Type::Unknown),
            },
            _ => Type::Unknown,
        };
        (ExprKind::Pairs(entries), ty)
    }

    /// `(a, b)`. An expected tuple of as many elements gives each its
    /// expected type.
    fn tuple(
        &mut self,
        node: &SyntaxNode,
        expected: Option<&Type>,
        fit: ExpressionFit,
    ) -> (ExprKind, Type) {
        let items: Vec<SyntaxNode> = node.children().collect();
        // True when nothing is expected of the tuple, which is not the
        // same as what is expected not being known.
        let unexpected = expected.is_none();
        let expected = match not_null(expected) {
            Some(Type::Tuple(expected)) if expected.len() == items.len() => Some(expected),
            _ => None,
        };
        let elements: Vec<Expr> = items
            .iter()
            .enumerate()
            .map(|(index, item)| match expected {
                Some(expected) => {
                    let ty = &expected[index];
                    let element = self.read_literal_part(item, ty, fit);
                    self.fit_literal_part(element, ty, "this element of the tuple", fit)
                }
                // Nothing is expected of the tuple, and so of its
                // elements: `(null, 1)` gives its `null` no type.
                None if unexpected => self.expression(item, None),
                // What the place expects is not known, or is no tuple
                // of this length: what an element is expected to be is
                // not known either, and nothing is reported of that.
                None => self.expression(item, Some(&Type::Unknown)),
            })
            .collect();
        // An element that does not fit has been reported: the tuple is
        // of the type expected, and is not reported again.
        let ty = match expected {
            Some(expected) => Type::Tuple(expected.clone()),
            None => Type::Tuple(elements.iter().map(|element| element.ty.clone()).collect()),
        };
        (ExprKind::Tuple(elements), ty)
    }

    /// `[a, b]`: a sequence of values (EXPR-9). "The use site and
    /// expected type constrain element types": an expected list or set
    /// gives the elements their type. With none, the values are a list
    /// of the one type they have, `f64` when some are `i64` and some
    /// `f64` (NUM-1).
    ///
    /// Pairs and `_` are for the passes that read dictionaries and test
    /// traces.
    fn sequence(
        &mut self,
        node: &SyntaxNode,
        expected: Option<&Type>,
        fit: ExpressionFit,
    ) -> (ExprKind, Type) {
        // True when nothing is expected of the sequence, which is not
        // the same as what is expected not being known.
        let unexpected = expected.is_none();
        let items: Vec<SyntaxNode> = node.children().collect();
        // TST-5: where a trace is expected, the sequence is one
        // written out.
        if let Some(Type::Trace { cell, .. }) = not_null(expected) {
            return self.trace(&items, cell);
        }
        // EXPR-8: "Harness `_` and timed cells exist only in test
        // traces; outside them a sequence is ordinary data and a
        // placeholder is not a value." Where what is expected is not
        // known, it may be a trace, and nothing is reported (ERR-9).
        let holes: Vec<SyntaxNode> = items
            .iter()
            .flat_map(|item| match item.kind() {
                SyntaxKind::Hole => vec![item.clone()],
                SyntaxKind::SequencePair => item
                    .children()
                    .filter(|part| part.kind() == SyntaxKind::Hole)
                    .collect(),
                _ => Vec::new(),
            })
            .collect();
        if !holes.is_empty() {
            if !expected.is_some_and(open) {
                for hole in &holes {
                    self.placeholder(
                        hole,
                        "`_` is no value: it stands for no publication, in a trace of a test",
                        "EXPR-8",
                    );
                }
            }
            return (ExprKind::Unread, Type::Unknown);
        }
        // A sequence is no `null`: of a `set<i64>?` expected, the set.
        let expected = not_null(expected);
        // EXPR-9: "Entries are either all values or all colon-separated
        // pairs", which the parser holds a sequence to. Pairs, and no
        // entries where a dictionary is expected, are read as one.
        let pairs = items
            .iter()
            .any(|item| item.kind() == SyntaxKind::SequencePair);
        if pairs || (items.is_empty() && matches!(expected, Some(Type::Dict { .. }))) {
            return self.pairs(node, &items, expected, unexpected, fit);
        }
        let element = match expected {
            Some(Type::List { element, .. }) | Some(Type::Set(element)) => Some(&**element),
            _ => None,
        };
        if let (Some(element), Some(expected)) = (element, expected) {
            let elements: Vec<Expr> = items
                .iter()
                .map(|item| {
                    let value = self.read_literal_part(item, element, fit);
                    self.fit_literal_part(value, element, "an element", fit)
                })
                .collect();
            if matches!(expected, Type::Set(_)) {
                self.check_collection_constants(items.iter().cloned(), expected);
            }
            // TYPE-10: "Size participates in identity." A list of a
            // size that is known has that many entries.
            if let Type::List {
                size: Some(size), ..
            } = expected
                && let ConstantValue::Int(size) = size.value
                && i64::try_from(elements.len()) != Ok(size)
            {
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::TYPE_MISMATCH,
                    self.at(node),
                    format!(
                        "this has {} {}, and a `{expected}` has {size}",
                        elements.len(),
                        if elements.len() == 1 {
                            "entry"
                        } else {
                            "entries"
                        }
                    ),
                    "TYPE-10",
                ));
            }
            return (ExprKind::Sequence(elements), expected.clone());
        }
        // From here the place gives the entries no type. Where it
        // expects a type that is known and holds no sequence, an `i64`,
        // the sequence is no value of it. It is then a list of what is
        // not known, which the place reports as no `i64`, and its
        // entries are not judged as well. A dictionary's entries are for
        // the pass that reads dictionaries.
        let misplaced = expected
            .is_some_and(|expected| !open(expected) && !matches!(expected, Type::Dict { .. }));
        let unjudged = if misplaced {
            Type::List {
                element: Box::new(Type::Unknown),
                size: None,
            }
        } else {
            Type::Unknown
        };
        // An entry that takes its type from its place, `null`, says
        // nothing of what the sequence holds: the others are read
        // first, and it is then read as an element of their type.
        // Where what the place expects is not known, what an entry is
        // expected to be is not known either: the `[]` of `[[]]` is
        // then not reported.
        let of_entry = if unexpected {
            None
        } else {
            Some(&Type::Unknown)
        };
        // A block among the entries may assign what another reads
        // (CTRL-10), and an entry that takes its type from its place is
        // read after the others. With a block among them the others
        // are read first only to learn their type, that reading taken
        // back, and every entry is then read where it is written.
        let ordered = items.iter().any(holds_block) && self.may_reread();
        let mut read: Vec<Option<Expr>> = items
            .iter()
            .map(|item| {
                if branch_takes_its_type(item) {
                    return None;
                }
                if !ordered {
                    return Some(self.expression(item, of_entry));
                }
                let mark = self.mark();
                let entry = self.expression(item, of_entry);
                self.take_back(mark, false);
                Some(entry)
            })
            .collect();
        let known: Vec<&Type> = read.iter().flatten().map(|entry| &entry.ty).collect();
        let has_null = items.iter().any(branch_is_null);
        // GEN-7, EXPR-9: an unfinished entry can change the sequence's
        // inferred type, even when another entry supplies a type now.
        // Leave a hole for the instance's join rather than fixing the
        // types of this list and of every place that infers from it.
        let pending = self.graph && !misplaced && known.iter().any(|ty| !whole(ty));
        // EXPR-9: "Entries of two types reject, and so do entries that
        // give no type: `[]`, `[null]`." Where what the place expects
        // is not known, neither is the sequence, and nothing is
        // reported.
        let Some(mut of) = sequence_element(&known, has_null) else {
            if unexpected {
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::SEQUENCE_CONTEXT,
                    self.at(node),
                    "nothing here says what this sequence holds: write its type".to_string(),
                    "EXPR-9",
                ));
            }
            let elements = items
                .iter()
                .map(|item| self.expression(item, Some(&Type::Unknown)))
                .collect();
            return (ExprKind::Sequence(elements), unjudged);
        };
        let mut rejected = false;
        if pending && unexpected && known.first().is_some_and(|ty| whole(ty)) {
            // Once the first entry supplies its type, a pending entry
            // cannot repair a conflict among the complete ones. A float
            // can widen integer literals but cannot convert i64 values.
            let known: Vec<&Type> = known.into_iter().filter(|ty| whole(ty)).collect();
            if let Some(joined) = sequence_element(&known, has_null) {
                let before = self.diagnostics.len();
                for entry in read.iter().flatten().filter(|entry| whole(&entry.ty)) {
                    self.fit(entry.clone(), &joined, "an element, as the first is,");
                }
                rejected = self.diagnostics.len() != before;
                if rejected {
                    // Keep the explicit error on the source entry. There
                    // is no pending join to recheck for this sequence.
                    of = joined;
                }
            }
        }
        // Where what the place expects is not known, it may be what
        // gives entries of two types their one type: a `list<Base>` for
        // an `A` and a `B` that inherit from it. Such entries are not
        // judged here, and the sequence has no type. Where the place
        // is known and holds no sequence, the place reports it.
        if !unexpected
            && !pending
            && read.iter().flatten().any(|entry| {
                !(self.admits(&of, &entry.ty) || is_integer_literal(entry) && accepts_float(&of))
            })
        {
            let elements = items
                .iter()
                .enumerate()
                .map(|(position, item)| match read[position].take() {
                    Some(entry) if !ordered => entry,
                    _ => self.expression(item, Some(&Type::Unknown)),
                })
                .collect();
            return (ExprKind::Sequence(elements), unjudged);
        }
        let elements: Vec<Expr> = items
            .iter()
            .enumerate()
            .map(|(position, item)| {
                let entry = match read[position].take() {
                    Some(entry) if !ordered => entry,
                    _ => self.expression(item, Some(&of)),
                };
                if pending {
                    entry
                } else {
                    self.fit(entry, &of, "an element, as the first is,")
                }
            })
            .collect();
        let element = match (pending && !rejected, has_null) {
            (false, _) => of,
            (true, false) => Type::Unknown,
            (true, true) => Type::Nullable(Box::new(Type::Unknown)),
        };
        let ty = Type::List {
            element: Box::new(element),
            size: None,
        };
        (ExprKind::Sequence(elements), ty)
    }
}

/// EXPR-9: the first entry that supplies a type fixes the element type,
/// with numeric widening and the nullable form for a literal null.
pub(super) fn sequence_element(known: &[&Type], has_null: bool) -> Option<Type> {
    let first = *known.first()?;
    let numbers = |ty: &Type| matches!(ty, Type::Scalar(Scalar::I64 | Scalar::F64));
    let ty = if known.iter().all(|ty| numbers(ty))
        && known.iter().any(|ty| **ty == Type::Scalar(Scalar::F64))
    {
        Type::Scalar(Scalar::F64)
    } else {
        first.clone()
    };
    Some(if has_null && !matches!(ty, Type::Nullable(_)) {
        Type::Nullable(Box::new(ty))
    } else {
        ty
    })
}

/// What [`null_tests`] asks of the function a condition is read in.
struct Proofs<'p> {
    /// Which bindings a null test proves anything of.
    refinable: &'p dyn Fn(LocalId) -> bool,
    /// Which bindings a tag test proves anything of, given what is
    /// known.
    tagged: &'p dyn Fn(LocalId, &[Fact]) -> bool,
    /// Every temporal input, for a `valid()` written with no argument.
    inputs: &'p dyn Fn() -> Vec<LocalId>,
    /// The admission facts from a guard on a child (TIME-1, TIME-4).
    child_guard: &'p dyn Fn(EndpointOperation, &Expr) -> Vec<Fact>,
    /// Which direct pack entry a selector names (FN-19).
    packed: &'p dyn Fn(&Expr) -> Option<crate::hir::PackEndpointSelection>,
}

/// What a condition proves: where it holds, and where it does not.
/// `refinable` says which bindings a null test proves anything of, and
/// `inputs` gives every temporal input, for a `valid()` written with no
/// argument.
///
/// ERR-12: a binding "tested directly against null": `value != null`,
/// `value == null`, with the `null` on either side. TIME-4: "A guard is
/// a test of `valid(input)` or of `modified(input)`, on the input by its
/// name". ERR-3: "Track
/// negation, short-circuit paths". A test of anything else, a field or
/// a call, proves nothing.
fn null_tests(condition: &Expr, proofs: &Proofs, assumed: &[Fact]) -> (Vec<Fact>, Vec<Fact>) {
    let Proofs {
        refinable,
        tagged,
        inputs,
        child_guard,
        packed,
    } = proofs;
    let both = |first: Vec<Fact>, second: Vec<Fact>| -> Vec<Fact> {
        first
            .into_iter()
            .filter(|fact| second.contains(fact))
            .collect()
    };
    let either = |mut first: Vec<Fact>, second: Vec<Fact>| -> Vec<Fact> {
        first.extend(second);
        first
    };
    match &condition.kind {
        // `valid(value)`, `valid([a, b])`, `valid()`: every endpoint
        // named is valid where it holds. Where it does not, one of them
        // is not, and nothing is proved of any.
        // TIME-9: `all_valid` is true "where every one is" valid, and
        // so the collection with them, as `valid` proves it; and
        // TIME-1: "`all_valid(parts)` admits every child".
        ExprKind::Endpoint {
            operation: operation @ (EndpointOperation::Valid | EndpointOperation::AllValid),
            inputs: named,
        } => {
            let valid = match named {
                Some(named) => named.clone(),
                None => inputs(),
            };
            let mut facts: Vec<Fact> = valid.iter().copied().map(Fact::Valid).collect();
            if *operation == EndpointOperation::AllValid {
                facts.extend(valid.into_iter().map(children::all_valid_fact));
            }
            (facts, Vec::new())
        }
        // TIME-4: "A guard is a test of `valid(input)` or of
        // `modified(input)`, on the input by its name ... An input is
        // not modified without being valid, so `modified` of it guards
        // what `valid` of it does; `modified(a, b)`, which holds where
        // either is, guards neither." Nor does `modified()`, of every
        // input, or `modified([])`, of none.
        ExprKind::Endpoint {
            operation: EndpointOperation::Modified,
            inputs: Some(named),
        } => match named.as_slice() {
            [input] => (vec![Fact::Valid(*input)], Vec::new()),
            _ => (Vec::new(), Vec::new()),
        },
        // The same direct-input guards keep the key of each selected
        // temporal pack entry (FN-19, TIME-4).
        ExprKind::PackedEndpoint { operation, inputs }
            if matches!(
                operation,
                EndpointOperation::Valid | EndpointOperation::AllValid
            ) || (*operation == EndpointOperation::Modified && inputs.len() == 1) =>
        {
            let mut facts = Vec::new();
            for input in inputs {
                if let ExprKind::EndpointOf(local) = input.kind {
                    facts.push(Fact::Valid(local));
                    if *operation == EndpointOperation::AllValid {
                        facts.push(children::all_valid_fact(local));
                    }
                } else if let Some(entry) = packed(input) {
                    facts.extend(children::pack_guard_facts(*operation, entry));
                }
            }
            (facts, Vec::new())
        }
        // TIME-4: "A child's guard is a test of `all_valid` of its
        // input, or of `valid` or `modified` of the child as the read
        // names it, `valid(parts[1])` for `parts[1]`". `modified` of it
        // proves what `valid` of it does. One known only when the
        // program runs proves nothing here.
        ExprKind::ChildEndpoint {
            operation:
                operation @ (EndpointOperation::Valid
                | EndpointOperation::AllValid
                | EndpointOperation::Modified),
            child,
        } => (child_guard(*operation, child), Vec::new()),
        // ERR-3: a binding "gains its tag fact from a direct
        // `is_ok`/`is_err` guard on it by its name: where `is_ok` holds
        // it is a success and where it does not a failure, and `is_err`
        // the reverse."
        ExprKind::TagTest { success, value } => {
            let Some(local) = proof_binding(value).filter(|local| tagged(*local, assumed)) else {
                return (Vec::new(), Vec::new());
            };
            let (success_where, failure_where) =
                (vec![Fact::Success(local)], vec![Fact::Failure(local)]);
            if *success {
                (success_where, failure_where)
            } else {
                (failure_where, success_where)
            }
        }
        ExprKind::Unary {
            operator: UnaryOperator::Not,
            operand,
        } => {
            let (when_true, when_false) = null_tests(operand, proofs, assumed);
            (when_false, when_true)
        }
        ExprKind::Binary {
            operator,
            left,
            right,
        } => match operator {
            BinaryOperator::Equal | BinaryOperator::NotEqual => {
                let tested = match (&left.kind, &right.kind) {
                    (_, ExprKind::Literal(Value::Null)) => left,
                    (ExprKind::Literal(Value::Null), _) => right,
                    _ => return (Vec::new(), Vec::new()),
                };
                let Some(local) = proof_binding(tested).filter(|local| refinable(*local)) else {
                    return (Vec::new(), Vec::new());
                };
                match operator {
                    BinaryOperator::NotEqual => (vec![Fact::Present(local)], Vec::new()),
                    _ => (Vec::new(), vec![Fact::Present(local)]),
                }
            }
            // Both hold where `&&` does; one or other fails where it
            // does not.
            BinaryOperator::And => {
                // The right operand is evaluated where the left
                // holds: what the left proves is known there.
                let (left_true, left_false) = null_tests(left, proofs, assumed);
                let known = [assumed, left_true.as_slice()].concat();
                let (right_true, right_false) = null_tests(right, proofs, &known);
                (either(left_true, right_true), both(left_false, right_false))
            }
            BinaryOperator::Or => {
                // The right operand is evaluated where the left does
                // not hold.
                let (left_true, left_false) = null_tests(left, proofs, assumed);
                let known = [assumed, left_false.as_slice()].concat();
                let (right_true, right_false) = null_tests(right, proofs, &known);
                (both(left_true, right_true), either(left_false, right_false))
            }
            _ => (Vec::new(), Vec::new()),
        },
        _ => (Vec::new(), Vec::new()),
    }
}

/// True when an expression holds a block, at any depth: reading it may
/// then declare a local, assign one, or return (CTRL-10).
pub(super) fn holds_block(node: &SyntaxNode) -> bool {
    node.descendants()
        .any(|part| part.kind() == SyntaxKind::Block)
}

/// CTRL-10: a block consisting only of literal reads has no locals,
/// assignment, return or captures to replay. Other block arguments keep
/// their source-ordered reading when a call is probed or inferred.
pub(super) fn has_ordered_block(node: &SyntaxNode) -> bool {
    node.descendants()
        .filter(|part| part.kind() == SyntaxKind::Block)
        .any(|block| {
            block.children().any(|stmt| {
                stmt.kind() != SyntaxKind::ExprStmt
                    || !stmt
                        .children()
                        .next()
                        .is_some_and(|expr| ungrouped(expr).kind() == SyntaxKind::Literal)
            })
        })
}

/// True for the literal `null`, in parentheses or not.
pub(super) fn written_null(node: &SyntaxNode) -> bool {
    let node = &ungrouped(node.clone());
    match node.kind() {
        SyntaxKind::Literal => tokens(node).any(|token| token.kind() == SyntaxKind::KwNull),
        _ => false,
    }
}

/// True when an expression takes its type from its place and has none
/// of its own: `null`, `.member`, and `ok(value)` or `err(value)`,
/// whose "type arguments come from the complete `result<T,E>` its place
/// expects" (ERR-2), in parentheses or not.
pub(super) fn takes_its_type(node: &SyntaxNode) -> bool {
    let node = &ungrouped(node.clone());
    match node.kind() {
        SyntaxKind::ImpliedMember => true,
        SyntaxKind::Literal => tokens(node).any(|token| token.kind() == SyntaxKind::KwNull),
        SyntaxKind::CallExpr => result_constructor(node).is_some(),
        _ => false,
    }
}

/// ERR-13: an unqualified `ok` or `err` call is always a result constructor.
pub(super) fn result_constructor(call: &SyntaxNode) -> Option<bool> {
    let callee = call.children().next()?;
    if callee.kind() != SyntaxKind::NameExpr || child(&callee, SyntaxKind::QualifiedName).is_some()
    {
        return None;
    }
    match name_token(&callee)?.text() {
        "ok" => Some(true),
        "err" => Some(false),
        _ => None,
    }
}
