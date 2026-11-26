//! Control flow in a value function, a hook and a graph: blocks, `if`,
//! `while`, `for` and `switch`, and what is assigned on each path
//! (CTRL-5, CTRL-10 to CTRL-15). Part of the typing pass ([`super`]).

use hgl_diagnostics::{Category, Diagnostic, Location};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};

use super::expressions::{ExpressionFit, takes_its_type};
use super::fit::{is_integer_literal, not_null, nullable, open};
use super::symbols::is_null;
use super::{BOOL, Fact, Typer};
use crate::constant::Fault;
use crate::declared::agree;
use crate::hir::{self, Expr, ExprKind, IfInference, LocalId, Stmt, ValueCategory};
use crate::program::{LocalKind, Program};
use crate::syntax::{child, tokens, ungrouped};
use crate::types::{Constant, ConstantValue, Detail, Scalar, Type};
use crate::{codes, error};

/// CTRL-13: a graph's no-else `if` has its branch's type where it is a
/// connection and a nullable branch type where it is an ordinary value.
/// A deferred category leaves the type open until instance selection.
pub(super) fn no_else_type(ty: Type, condition: ValueCategory, branch: ValueCategory) -> Type {
    use ValueCategory::{Connection, Unsettled, Value};
    match (condition, branch) {
        (Connection, _) | (_, Connection) => ty,
        (Value, Value) => nullable(ty),
        (Unsettled, _) | (_, Unsettled) => Type::Unknown,
    }
}

/// CTRL-15 and FN-3: a graph tests its `while` condition when it is
/// built, so the condition cannot be a connection.
pub(super) fn while_connection(condition: &Expr, category: ValueCategory) -> Option<Diagnostic> {
    (category == ValueCategory::Connection).then(|| {
        error(
            Category::Phase,
            codes::CONNECTION_AS_CONSTANT,
            condition.at,
            "the condition of a `while` is tested while the graph is built, and this is \
             a connection"
                .to_string(),
            "FN-3",
        )
    })
}

pub(super) const RANGE_LOWER: &str = "the lower bound of this range";
pub(super) const RANGE_UPPER: &str = "the upper bound of this range";

/// EXPR-16: composition evaluates a range bound while building the graph.
pub(super) fn range_connection(
    bound: &Expr,
    name: &str,
    category: ValueCategory,
) -> Option<Diagnostic> {
    (category == ValueCategory::Connection).then(|| {
        error(
            Category::Phase,
            codes::CONNECTION_AS_CONSTANT,
            bound.at,
            format!("{name} is evaluated where the graph is built, and this is a connection"),
            "EXPR-16",
        )
    })
}

pub(super) fn comprehension_extent_error(at: Location, composition: bool) -> Diagnostic {
    let message = if composition {
        "a list comprehension in composition takes a range with constant bounds or a fixed-size list connection"
    } else {
        "this comprehension needs a constant number of entries for its fixed-size list"
    };
    error(
        Category::Type,
        codes::LIST_SIZE,
        at,
        message.to_string(),
        "EXPR-17",
    )
}

/// The number of iterations of a fixed-size list (EXPR-17).
pub(super) fn fixed_list_extent(ty: &Type) -> Option<&Constant> {
    let Type::List {
        size: Some(size), ..
    } = ty
    else {
        return None;
    };
    (!matches!(size.value, ConstantValue::Unbounded)).then_some(size)
}

/// Show a symbolic range extent as an expression instead of a range type.
fn extent_text(node: &SyntaxNode, value: &ConstantValue) -> String {
    let mut bounds = node.children();
    if let ConstantValue::RangeLength(lower, _) = value
        && let (Some(first), Some(last)) = (bounds.next(), bounds.next())
    {
        if matches!(**lower, ConstantValue::Int(0)) {
            return last.text().to_string();
        }
        let (first, last) = (first.text(), last.text());
        return format!("if ({last}) > ({first}) {{ ({last}) - ({first}) }} else {{ 0 }}");
    }
    node.text().to_string()
}

/// What a `for` binds, and whether it makes one child for each member,
/// by what it loops over (CTRL-15).
///
/// "`for item in items` over an ordinary list, set, or dictionary
/// composes its body once for each element, with ordinary values bound
/// ... Over a connection of `list<T, N>` it composes its body `N` times,
/// `item` a connection of `T` ... Over a connection of `list<T>`,
/// `set<T>`, or `dict<K, V>` it makes one child for each member
/// (CTRL-7): `item` is a connection of `T` for a list and an ordinary
/// `T`, the member itself, for a set; `key` is an ordinary `K` and
/// `value` a connection of `V` for a dictionary."
pub(super) fn bound_by_loop(
    over: ValueCategory,
    ty: &Type,
    names: usize,
) -> (Vec<ValueCategory>, bool) {
    use ValueCategory::{Connection, Unsettled, Value};
    match (over, ty) {
        (Connection, Type::List { size, .. }) => {
            // `list<T, unbounded>` grows as `list<T>` does.
            let fixed = size
                .as_ref()
                .is_some_and(|size| !matches!(size.value, ConstantValue::Unbounded));
            (vec![Connection; names], !fixed)
        }
        (Connection, Type::Set(_)) => (vec![Value; names], true),
        (Connection, Type::Dict { .. }) if names == 2 => (vec![Value, Connection], true),
        // What is looped over is not settled: neither is what is bound.
        (Unsettled, _) => (vec![Unsettled; names], false),
        _ => (vec![Value; names], false),
    }
}

/// CTRL-12 and CTRL-15: a `for` reads these collection types.
pub(super) fn iterates(ty: &Type) -> bool {
    matches!(ty, Type::List { .. } | Type::Set(_) | Type::Dict { .. })
}

/// The types of the loop bindings in order, for the supported shape.
pub(super) fn iteration_types(ty: &Type, names: usize) -> Option<Vec<&Type>> {
    match (ty, names) {
        (Type::List { element, .. } | Type::Set(element), 1) => Some(vec![element]),
        (Type::Dict { key, value }, 2) => Some(vec![key, value]),
        _ => None,
    }
}

/// How a flow expression's result is used.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ValueUse {
    /// A statement or the tail of a function with no result (FN-7).
    Discarded,
    Required,
    /// The inferred tail of a callback can give no result (FN-17).
    Inferred,
}

impl ValueUse {
    pub(super) fn is_used(self) -> bool {
        self != Self::Discarded
    }
}

/// What a block or an `if` comes to.
///
/// `#[derive(Clone)]` asks the compiler to write the code that copies
/// one.
#[derive(Clone)]
pub(super) enum Outcome {
    /// It has a value of this type.
    Value(Type),
    /// It has none: its last statement is no expression.
    Nothing,
    /// Every path through it returns from the function.
    Returns,
    /// It is null and nothing else: an `if` with no `else` whose branch
    /// returns (CTRL-10: "as if its `else` were null").
    Null,
}

/// One branch of an `if`, as it was read.
struct Branch {
    /// The branch: a block, or the `if` of an `else if`.
    value: Expr,
    /// What it comes to.
    outcome: Outcome,
    /// What is assigned at its end.
    assigned: Vec<bool>,
    /// True when every path through it has returned.
    returned: bool,
}

/// The value of a `switch` label, for telling two alike (CTRL-6).
use hir::LabelValue as Label;

// ---- Control flow in a value function ----

impl<'a> Typer<'a> {
    /// The statements of a block before its value, read in order, and
    /// the expression that is its value: its last statement, when that
    /// is an expression (CTRL-10).
    pub(super) fn block_head(&mut self, block: &SyntaxNode) -> (Vec<Stmt>, Option<SyntaxNode>) {
        let parts: Vec<SyntaxNode> = block.children().collect();
        let last = parts
            .last()
            .filter(|last| last.kind() == SyntaxKind::ExprStmt)
            .and_then(|last| last.children().next());
        let before = if last.is_some() {
            parts.len() - 1
        } else {
            parts.len()
        };
        let statements = self.read_statements(parts[..before].iter().cloned());
        (statements, last)
    }

    /// An expression that may be a block or an `if`, with what it comes to.
    pub(super) fn flow(
        &mut self,
        node: &SyntaxNode,
        expected: Option<&Type>,
        value_use: ValueUse,
    ) -> (Expr, Outcome) {
        self.flow_with_fit(node, expected, value_use, ExpressionFit::Value)
    }

    pub(super) fn flow_with_fit(
        &mut self,
        node: &SyntaxNode,
        expected: Option<&Type>,
        value_use: ValueUse,
        fit: ExpressionFit,
    ) -> (Expr, Outcome) {
        // "Parentheses select grouping" (EXPR-1).
        let node = &ungrouped(node.clone());
        // SOURCE-4: a block or an `if` whose value is used is a value
        // expression, and a `yield` inside it is reported.
        let valued =
            value_use.is_used() && matches!(node.kind(), SyntaxKind::Block | SyntaxKind::IfExpr);
        self.valued += u32::from(valued);
        let read = match node.kind() {
            SyntaxKind::Block => self.block(node, expected, value_use, fit),
            SyntaxKind::IfExpr => self.conditional(node, expected, value_use, fit),
            _ => {
                let value = self.expression_with_fit(node, expected, value_use, fit);
                let outcome = if value_use == ValueUse::Inferred
                    && value.ty == Type::Unknown
                    && let ExprKind::Call { callee, .. } = &value.kind
                    && self.call_has_no_result(callee)
                {
                    Outcome::Nothing
                } else {
                    Outcome::Value(value.ty.clone())
                };
                (value, outcome)
            }
        };
        self.valued -= u32::from(valued);
        read
    }

    /// CTRL-10: "A block whose last statement is an expression has that
    /// expression's value and type; any other block has none."
    ///
    /// Where the place says what it expects, the value is checked
    /// against that here, where it is written; the block is then of the
    /// type expected and is not reported again.
    fn block(
        &mut self,
        node: &SyntaxNode,
        expected: Option<&Type>,
        value_use: ValueUse,
        fit: ExpressionFit,
    ) -> (Expr, Outcome) {
        let at = self.at(node);
        // ERR-12: what a null test proves inside the block holds to its
        // end.
        let frame = self.present.len();
        let (expression, outcome) = self.block_read(node, expected, value_use, at, fit);
        self.present.truncate(frame);
        (expression, outcome)
    }

    /// The block `node`, read: see [`Typer::block`].
    fn block_read(
        &mut self,
        node: &SyntaxNode,
        expected: Option<&Type>,
        value_use: ValueUse,
        at: Location,
        fit: ExpressionFit,
    ) -> (Expr, Outcome) {
        let (statements, last) = self.block_head(node);
        let (value, outcome) = match last {
            // CTRL-5: what follows a `return` is not reached. It is read
            // for what it reports itself, and is not the block's value.
            Some(last) if self.returned => {
                let (value, _) = self.flow(&last, Some(&Type::Unknown), ValueUse::Discarded);
                (Some(Box::new(value)), Outcome::Returns)
            }
            Some(last) => {
                let (value, outcome) = self.flow_with_fit(&last, expected, value_use, fit);
                match (outcome, expected) {
                    (Outcome::Value(_), Some(expected)) if !open(expected) => {
                        let reported = self.diagnostics.len();
                        // A branch of a graph's `if` or `switch` gives
                        // its connection across a reference as the
                        // place it stands in does (TIME-10).
                        let pattern_type = fit.input_type(&value.ty, expected);
                        let value = self.fit_input(
                            value,
                            pattern_type.as_ref().unwrap_or(expected),
                            "the value of this block",
                        );
                        let ty = if self.diagnostics.len() == reported {
                            value.ty.clone()
                        } else {
                            expected.clone()
                        };
                        (Some(Box::new(value)), Outcome::Value(ty))
                    }
                    (outcome, _) => (Some(Box::new(value)), outcome),
                }
            }
            None => (None, Outcome::Nothing),
        };
        // CTRL-5: a block that has returned on every path comes to
        // that, whatever follows the `return`.
        let outcome = if self.returned {
            Outcome::Returns
        } else {
            outcome
        };
        let ty = match &outcome {
            Outcome::Value(ty) => ty.clone(),
            Outcome::Nothing | Outcome::Returns | Outcome::Null => Type::Unknown,
        };
        let kind = ExprKind::Block { statements, value };
        (
            Expr {
                kind,
                ty,
                at,
                category: ValueCategory::Unsettled,
            },
            outcome,
        )
    }

    /// CTRL-10: "In a value function the condition of an `if` or a
    /// `while` is a `bool`: a `bool?` there is read, and a null fails the
    /// read (ERR-12); a number, or any other type, rejects." CTRL-13: in
    /// composition "A `bool?` there is read as ERR-12 says". ERR-15: "a
    /// condition ... expect[s] the T": a `result<bool, error>` there is
    /// its `bool`, a failure raised. For `result<bool?, error>`, the
    /// condition takes the `bool?` and then reads it.
    pub(super) fn condition(&mut self, node: &SyntaxNode) -> Expr {
        let value = self.expression(node, Some(&BOOL));
        self.fit_condition(value)
    }

    /// Checks the condition's type and takes the reads required by CTRL-13.
    pub(super) fn fit_condition(&mut self, value: Expr) -> Expr {
        if let Some(taken) = self.taken(&BOOL, &value.ty) {
            return self.take(value, taken);
        }
        if !open(&value.ty) && value.ty != BOOL {
            self.diagnostics.emit(error(
                Category::Type,
                codes::TYPE_MISMATCH,
                value.at,
                format!("this is `{}`, and a condition is a `bool`", value.ty),
                self.conditional_rule(),
            ));
        }
        value
    }

    /// The rule that says what an `if` is where this one stands:
    /// CTRL-13 in a graph, CTRL-10 in a value function and a hook.
    fn conditional_rule(&self) -> &'static str {
        if self.graph { "CTRL-13" } else { "CTRL-10" }
    }

    /// The rule that says what a loop and a `switch` are where this one
    /// stands: CTRL-15 in a graph, CTRL-12 in a value function and a
    /// hook.
    fn loop_rule(&self) -> &'static str {
        if self.graph { "CTRL-15" } else { "CTRL-12" }
    }

    /// One branch of an `if`, read from what is assigned before the
    /// `if`.
    fn branch(
        &mut self,
        node: &SyntaxNode,
        expected: Option<&Type>,
        value_use: ValueUse,
        (before, before_returned, proved): (&[bool], bool, &[Fact]),
        fit: ExpressionFit,
    ) -> Branch {
        self.assigned = restored(before, self.locals.len(), true);
        self.returned = before_returned;
        // ERR-12: what the condition proves holds in the branch, and no
        // further.
        let frame = self.present.len();
        self.present.extend_from_slice(proved);
        let (value, outcome) = self.flow_with_fit(node, expected, value_use, fit);
        self.present.truncate(frame);
        Branch {
            value,
            outcome,
            assigned: self.assigned.clone(),
            returned: self.returned,
        }
    }

    /// `if condition { ... } else { ... }`.
    ///
    /// CTRL-10: "An `if` whose value is used has the one type its two
    /// branches' values have, as the entries of a sequence have one
    /// (EXPR-9): a null branch makes it `T?`, and two types reject.
    /// With no `else` it is as if its `else` were null ... Where the
    /// place expects a type, each branch's value is checked against it.
    /// A branch that returns from the function gives the `if` no value
    /// and takes no part in its type (CTRL-5)."
    ///
    /// CTRL-11: a `var` "is assigned after an `if` when every branch
    /// that does not return assigns it, so not by an `if` with no
    /// `else`".
    fn conditional(
        &mut self,
        node: &SyntaxNode,
        expected: Option<&Type>,
        value_use: ValueUse,
        fit: ExpressionFit,
    ) -> (Expr, Outcome) {
        let at = self.at(node);
        let mut parts = node.children();
        let (Some(condition), Some(then)) = (parts.next(), parts.next()) else {
            self.forget();
            self.unsure = true;
            let unread = Expr {
                kind: ExprKind::Unread,
                ty: Type::Unknown,
                at,
                category: ValueCategory::Unsettled,
            };
            return (unread, Outcome::Value(Type::Unknown));
        };
        let otherwise = parts.next();
        let guard = ungrouped(condition.clone());
        let selected = if self.lexical_default_dependencies {
            None
        } else if guard.kind() == SyntaxKind::PackKeyMembership {
            let (kind, _) = self.pack_key_membership(&guard);
            match kind {
                ExprKind::Literal(hir::Value::Bool(selected)) => Some(selected),
                kind => {
                    // An unresolved guard leaves both alternatives to its instance.
                    self.unsure = true;
                    for assignment in then
                        .descendants()
                        .chain(otherwise.iter().flat_map(SyntaxNode::descendants))
                        .filter(|part| part.kind() == SyntaxKind::AssignStmt)
                    {
                        if let Some(place) = assignment.children().next().map(ungrouped)
                            && place.kind() == SyntaxKind::NameExpr
                            && let Some(local) = self.local_of(&place)
                        {
                            // Recheck the continuation with actual assignments
                            // once the branch is known (CTRL-11, CTRL-16).
                            self.assigned[local] = true;
                        }
                    }
                    let ty = expected.cloned().unwrap_or(Type::Unknown);
                    let outcome = if value_use.is_used() {
                        Outcome::Value(ty.clone())
                    } else {
                        Outcome::Nothing
                    };
                    return (
                        Expr {
                            kind,
                            ty,
                            at,
                            category: ValueCategory::Unsettled,
                        },
                        outcome,
                    );
                }
            }
        } else {
            self.active_pack_condition(&condition)
        };
        if let Some(selected) = selected {
            // CTRL-16: a constant condition in a concrete pack pass
            // composes only the selected branch.
            if selected {
                self.report_pack_branch(&then);
                return self.flow_with_fit(&then, expected, value_use, fit);
            }
            if let Some(otherwise) = otherwise {
                self.report_pack_branch(&otherwise);
                return self.flow_with_fit(&otherwise, expected, value_use, fit);
            }
            let (kind, outcome) = if value_use.is_used() {
                (ExprKind::Literal(hir::Value::Null), Outcome::Null)
            } else {
                (
                    ExprKind::Block {
                        statements: Vec::new(),
                        value: None,
                    },
                    Outcome::Nothing,
                )
            };
            return (
                Expr {
                    kind,
                    ty: Type::Unknown,
                    at,
                    category: ValueCategory::Value,
                },
                outcome,
            );
        }
        let condition = self.condition(&condition);
        // CTRL-1: "A wiring-time condition chooses which branch to
        // compose. A temporal condition in composition selects a child
        // graph at runtime."
        let selects = if self.graph {
            self.category(&condition)
        } else {
            ValueCategory::Value
        };
        // A branch selected at runtime: a `var` before it is declared
        // outside (CTRL-14). The nearest such `if` says which are: one
        // inside another has its own, and a `var` of the outer branch
        // is declared before the inner.
        let outer = self.temporal;
        if selects == ValueCategory::Connection {
            self.temporal = Some(self.locals.len());
        }
        // ERR-12: the bindings the condition proves are not null, where
        // it holds and where it does not.
        let (when_true, when_false) = self.null_tests(&condition);
        let before = self.assigned.clone();
        let before_returned = self.returned;
        let (in_then, in_else) = (
            (before.as_slice(), before_returned, when_true.as_slice()),
            (before.as_slice(), before_returned, when_false.as_slice()),
        );
        // The place says what a branch's value is, when it says. With
        // nothing said, a branch that is `null` or `.member` takes its
        // type from the other branch, which is then read first.
        let told = expected.is_some_and(|expected| !open(expected));
        let from_other = |first: &Outcome, second: &SyntaxNode| match first {
            Outcome::Value(ty) if !open(ty) => Some(if branch_is_null(second) {
                nullable(ty.clone())
            } else {
                ty.clone()
            }),
            _ => None,
        };
        let by_place = |branch: &SyntaxNode| branch_takes_its_type(branch);
        let (mut then, mut other) = match &otherwise {
            None => (self.branch(&then, expected, value_use, in_then, fit), None),
            Some(otherwise) if !told && by_place(&then) && !by_place(otherwise) => {
                let other = self.branch(otherwise, expected, value_use, in_else, fit);
                let hint = from_other(&other.outcome, &then);
                let then = self.branch(&then, hint.as_ref().or(expected), value_use, in_then, fit);
                (then, Some(other))
            }
            Some(otherwise) => {
                let then = self.branch(&then, expected, value_use, in_then, fit);
                let hint = if !told && by_place(otherwise) {
                    from_other(&then.outcome, otherwise)
                } else {
                    None
                };
                let other = self.branch(
                    otherwise,
                    hint.as_ref().or(expected),
                    value_use,
                    in_else,
                    fit,
                );
                (then, Some(other))
            }
        };
        self.temporal = outer;
        // ERR-12: "including after a terminating null branch". Where
        // one branch returns, what follows the `if` is reached only
        // through the other, and what that one's side of the condition
        // proves holds to the end of the block the `if` stands in.
        let else_returned = other
            .as_ref()
            .map_or(before_returned, |other| other.returned);
        if then.returned && !else_returned {
            self.present.extend_from_slice(&when_false);
        } else if else_returned && !then.returned && other.is_some() {
            self.present.extend_from_slice(&when_true);
        }

        // What is assigned after: by every branch that does not return.
        let count = self.locals.len();
        match &other {
            Some(other) => {
                self.assigned = joined(
                    (&then.assigned, then.returned),
                    (&other.assigned, other.returned),
                    count,
                );
                self.returned = then.returned && other.returned;
            }
            None => {
                self.assigned = joined(
                    (&then.assigned, then.returned),
                    (&before, before_returned),
                    count,
                );
                self.returned = then.returned && before_returned;
            }
        }

        // CTRL-14: a final conditional can supply a continuation only
        // through both arms. Reuse each arm's ordinary flow result;
        // any arm still waiting for a switch must prove its coverage.
        if self.graph {
            self.returning_tails.remove(&at);
            if !self.returned
                && let Some(other) = &other
            {
                let covered = |branch: &Branch| {
                    if branch.returned {
                        Some(&[][..])
                    } else {
                        self.returning_expression(&branch.value)
                    }
                };
                if let (Some(then), Some(other)) = (covered(&then), covered(other)) {
                    let switches = then.iter().chain(other).copied().collect();
                    self.returning_tails.insert(at, switches);
                }
            }
        }

        let outcome = match (
            then.outcome.clone(),
            other.as_ref().map(|other| other.outcome.clone()),
        ) {
            (Outcome::Returns, Some(Outcome::Returns)) => Outcome::Returns,
            (Outcome::Returns, Some(outcome)) | (outcome, Some(Outcome::Returns)) => outcome,
            (Outcome::Value(_), Some(Outcome::Value(_))) => {
                let ty = match (expected, &mut other) {
                    (Some(expected), _) if told => expected.clone(),
                    // CTRL-10 gives the one type to "an `if` whose value
                    // is used". One whose value is dropped has branches
                    // of any types.
                    _ if !value_use.is_used() => Type::Unknown,
                    (_, Some(other)) => self.one_type(&mut then.value, &mut other.value),
                    (_, None) => then.value.ty.clone(),
                };
                Outcome::Value(ty)
            }
            // CTRL-13: with no `else`, "under a temporal condition it is
            // a connection of `T` that never ticks while the condition
            // is false (CTRL-3): `T`, not `T?`. Under a wiring-time
            // condition a branch that is an ordinary value makes it a
            // `T?`, as in a value function, and a branch that is a
            // connection makes it a connection of `T` that never ticks
            // where the branch is not composed." So a branch that is a
            // connection gives a connection of `T` whatever the
            // condition is. Otherwise, where the condition or the
            // branch is not settled, neither is the type.
            (Outcome::Value(ty), None) if self.graph => {
                Outcome::Value(no_else_type(ty, selects, self.category(&then.value)))
            }
            // "With no `else` it is as if its `else` were null: its
            // value is a `T?`".
            (Outcome::Value(ty), None) => Outcome::Value(nullable(ty)),
            // The same holds where the branch written returns: the `if`
            // is then null and nothing else.
            (Outcome::Returns | Outcome::Null, None) => Outcome::Null,
            // A branch that is null alone is a null branch: it "makes
            // it `T?`".
            (Outcome::Null, Some(Outcome::Null)) => Outcome::Null,
            (Outcome::Null, Some(Outcome::Value(ty)))
            | (Outcome::Value(ty), Some(Outcome::Null)) => Outcome::Value(nullable(ty)),
            _ => Outcome::Nothing,
        };
        let ty = match &outcome {
            Outcome::Value(ty) => ty.clone(),
            Outcome::Nothing | Outcome::Returns | Outcome::Null => Type::Unknown,
        };
        // CTRL-10: record the surviving branch from the same outcomes
        // that decide the source's type. A dead tail is no branch value.
        let inference = match (
            value_use.is_used(),
            &then.outcome,
            other.as_ref().map(|other| &other.outcome),
        ) {
            (true, Outcome::Value(_), Some(Outcome::Value(_))) if !told => IfInference::Both,
            (true, Outcome::Value(_), Some(Outcome::Returns)) => IfInference::Then,
            (true, Outcome::Returns, Some(Outcome::Value(_))) => IfInference::Otherwise,
            _ => IfInference::None,
        };
        let kind = ExprKind::If {
            condition: Box::new(condition),
            then: Box::new(then.value),
            otherwise: other.map(|other| Box::new(other.value)),
            inference,
        };
        (
            Expr {
                kind,
                ty,
                at,
                category: ValueCategory::Unsettled,
            },
            outcome,
        )
    }

    /// The one type the values of two branches have, with nothing
    /// expected of them: the type of both; the wider of a `T` and a
    /// `T?`; an `f64` for an integer literal and an `f64`, the literal
    /// widened (EXPR-5). Two types are reported at the second branch.
    pub(super) fn one_type(&mut self, then: &mut Expr, other: &mut Expr) -> Type {
        let (first, second) = (then.ty.clone(), other.ty.clone());
        if open(&first) || open(&second) {
            return Type::Unknown;
        }
        if agree(&first, &second) || self.admits(&first, &second) {
            return first;
        }
        if self.admits(&second, &first) {
            return second;
        }
        let float = Type::Scalar(Scalar::F64);
        if second == float
            && let Some(widened) = widened_block(then)
        {
            *then = widened;
            return float;
        }
        if first == float
            && let Some(widened) = widened_block(other)
        {
            *other = widened;
            return float;
        }
        self.diagnostics.emit(error(
            Category::Type,
            codes::TYPE_MISMATCH,
            other.at,
            format!("this branch is `{second}`, and the other is `{first}`"),
            self.conditional_rule(),
        ));
        first
    }

    /// The body of a loop. CTRL-11: a `var` is "never [assigned] by a
    /// loop's body, which may run no times"; nor does a `return` in it
    /// end every path.
    pub(super) fn loop_body(&mut self, block: Option<&SyntaxNode>) -> Vec<Stmt> {
        let before = self.assigned.clone();
        let before_returned = self.returned;
        // ERR-12: a body may run no times, so what a null test proves
        // inside it is proved of nothing after the loop.
        let frame = self.present.len();
        let statements =
            block.map_or_else(Vec::new, |block| self.read_statements(block.children()));
        self.present.truncate(frame);
        self.assigned = restored(&before, self.locals.len(), true);
        self.returned = before_returned;
        statements
    }

    /// CTRL-12: "`while condition { ... }` repeats its body while the
    /// condition holds; the form with no condition is a source's
    /// (SOURCE-4) and rejects there."
    pub(super) fn while_loop(&mut self, node: &SyntaxNode) -> Stmt {
        let at = self.at(node);
        let condition = node
            .children()
            .find(|part| part.kind() != SyntaxKind::Block);
        let condition = match condition {
            Some(condition) => Some(self.condition(&condition)),
            // CTRL-15: "the form with no condition is a source's".
            None if self.generating => None,
            None => {
                let keyword = tokens(node)
                    .next()
                    .map_or(at, |keyword| Location::new(self.file, keyword.span()));
                self.diagnostics.emit(error(
                    Category::Phase,
                    codes::WHILE_CONDITION,
                    keyword,
                    format!(
                        "a `while` with no condition is for a source that yields: in a {} it \
                         has one",
                        if self.graph {
                            "graph"
                        } else {
                            "value function"
                        }
                    ),
                    self.loop_rule(),
                ));
                None
            }
        };
        // CTRL-15: "The condition of a `while` is an ordinary `bool`,
        // tested while the graph is built: a connection there rejects
        // (FN-3)".
        if self.graph
            && let Some(condition) = &condition
            && let Some(report) = while_connection(condition, self.category(condition))
        {
            self.diagnostics.emit(report);
        }
        // ERR-12: the body is reached only where the condition holds,
        // so what it proves is not null is not null there.
        let frame = self.present.len();
        if let Some(condition) = &condition {
            let (when_true, _) = self.null_tests(condition);
            self.present.extend(when_true);
        }
        let body = self.loop_body(child(node, SyntaxKind::Block).as_ref());
        self.present.truncate(frame);
        Stmt::While {
            condition,
            body,
            at,
        }
    }

    /// CTRL-12: "`for item in items` runs its body for each element of a
    /// list, in index order, or of a set, and `for key, value in
    /// entries` for each key and value of a dictionary. One name over a
    /// dictionary, two over a list or a set, and any other type after
    /// `in` reject; a nullable list, set, or dictionary there is read".
    pub(super) fn for_loop(&mut self, node: &SyntaxNode) -> Stmt {
        let at = self.at(node);
        // The pattern's names: the identifiers directly inside the
        // statement, but the `in` that ends them (LEX-10).
        let mut names: Vec<SyntaxToken> = tokens(node)
            .filter(|token| token.kind() == SyntaxKind::Ident)
            .collect();
        if names.last().is_some_and(|token| token.text() == "in") {
            names.pop();
        }
        let Some(sequence) = node
            .children()
            .find(|part| part.kind() != SyntaxKind::Block)
        else {
            return self.unread(node);
        };
        let sequence = self.loop_iterable(&sequence);
        let types = self.loop_bindings(&sequence, names.len());
        // CTRL-15: what the loop binds, and whether it makes one child
        // for each member.
        let over = if self.graph {
            self.category(&sequence)
        } else {
            ValueCategory::Value
        };
        let (categories, children) = bound_by_loop(over, &sequence.ty, names.len());
        let outside = self.locals.len();
        let names: Vec<LocalId> = names
            .iter()
            .zip(types)
            .map(|(name, ty)| {
                let at = Location::new(self.file, name.span());
                self.declare(name.text(), at, LocalKind::Iteration, ty, true)
            })
            .collect();
        for (local, category) in names.iter().zip(categories) {
            self.locals[*local].category = category;
        }
        // Each such loop says which `var`s are outside it: one declared
        // in an enclosing loop's body is outside the loop nested there.
        // A loop that makes no children keeps the boundary of the one
        // it stands in.
        let outer = self.dynamic;
        if children {
            self.dynamic = Some(outside);
        }
        let range_scope = self.enter_range_bindings(&sequence, &names);
        let body = self.loop_body(child(node, SyntaxKind::Block).as_ref());
        self.unrolled_range_bindings.truncate(range_scope);
        self.dynamic = outer;
        Stmt::For {
            names,
            sequence,
            body,
            at,
        }
    }

    /// CTRL-16: a graph range with fixed bounds gives each pass a
    /// constant name. Track it only while reading that pass's body.
    fn enter_range_bindings(&mut self, sequence: &Expr, names: &[LocalId]) -> usize {
        let scope = self.unrolled_range_bindings.len();
        if self.graph
            && let ExprKind::Range { lower, upper } = &sequence.kind
            && self.is_fixed(lower, &self.unrolled_range_bindings)
            && self.is_fixed(upper, &self.unrolled_range_bindings)
        {
            self.unrolled_range_bindings.extend_from_slice(names);
        }
        scope
    }

    /// An iterable has the ordinary collection rules (CTRL-12), or is a
    /// range whose bounds are ordinary `i64` values (EXPR-16).
    pub(super) fn loop_iterable(&mut self, node: &SyntaxNode) -> Expr {
        if node.kind() != SyntaxKind::RangeExpr {
            if let Some(pack) = self.homogeneous_pack_iterable(node) {
                return pack;
            }
            let value = self.expression(node, None);
            return self.received(value, iterates);
        }
        let mut bounds = node.children();
        let (Some(lower), Some(upper)) = (bounds.next(), bounds.next()) else {
            return Expr {
                kind: ExprKind::Unread,
                ty: Type::Unknown,
                at: self.at(node),
                category: ValueCategory::Unsettled,
            };
        };
        let mut bound = |node: &SyntaxNode, name: &str| {
            let expected = Type::Scalar(Scalar::I64);
            let value = self.expression(node, Some(&expected));
            let value = self.fit(value, &expected, name);
            if self.graph
                && let Some(report) = range_connection(&value, name, self.category(&value))
            {
                self.diagnostics.emit(report);
            }
            Box::new(value)
        };
        let lower = bound(&lower, RANGE_LOWER);
        let upper = bound(&upper, RANGE_UPPER);
        Expr {
            kind: ExprKind::Range { lower, upper },
            ty: Type::List {
                element: Box::new(Type::Scalar(Scalar::I64)),
                size: None,
            },
            at: self.at(node),
            category: ValueCategory::Value,
        }
    }

    /// Check the list type and the scope of each entry (EXPR-17).
    pub(super) fn list_comprehension(
        &mut self,
        node: &SyntaxNode,
        expected: Option<&Type>,
        fit: ExpressionFit,
    ) -> (ExprKind, Type) {
        let mut parts = node.children();
        let (Some(sequence_node), Some(entry), Some(name)) = (
            parts.next(),
            parts.next(),
            tokens(node).find(|token| token.kind() == SyntaxKind::Ident),
        ) else {
            return (ExprKind::Unread, Type::Unknown);
        };
        let before_sequence = self.diagnostics.len();
        let mut sequence = self.loop_iterable(&sequence_node);
        let binding_ty = self.loop_bindings(&sequence, 1).remove(0);
        let sequence_reported = self.diagnostics.len() != before_sequence;
        let over = if self.graph {
            self.category(&sequence)
        } else {
            ValueCategory::Value
        };
        let (item_category, _) = bound_by_loop(over, &sequence.ty, 1);
        let binding = self.declare(
            name.text(),
            Location::new(self.file, name.span()),
            LocalKind::Iteration,
            binding_ty,
            true,
        );
        self.locals[binding].category = item_category[0];
        let (element, expected_size) = match not_null(expected) {
            Some(Type::List { element, size }) => (Some(&**element), size.as_ref()),
            _ => (None, None),
        };
        let fixed = expected_size.filter(|size| !matches!(size.value, ConstantValue::Unbounded));
        // It is possible to make no entries. What happens in an entry
        // cannot prove a binding assigned, present, or returned after it.
        let before = self.assigned.clone();
        let returned = self.returned;
        let present = self.present.len();
        let range_scope = self.enter_range_bindings(&sequence, &[binding]);
        let value = self.expression_with_fit(&entry, element, ValueUse::Required, fit);
        self.unrolled_range_bindings.truncate(range_scope);
        let value = match element {
            Some(element) => {
                let pattern_type = fit.input_type(&value.ty, element);
                self.fit_input(
                    value,
                    pattern_type.as_ref().unwrap_or(element),
                    "this list entry",
                )
            }
            None => value,
        };
        self.present.truncate(present);
        self.assigned = restored(&before, self.locals.len(), true);
        self.returned = returned;
        let size = if sequence_reported {
            fixed.cloned()
        } else if self.graph || fixed.is_some() {
            self.comprehension_size(&sequence_node, &sequence, over, fixed)
        } else {
            None
        };
        // EXPR-17: an ordinary range result has the same list shape as
        // its entries written out. A temporal result uses the pass count.
        let range_entry =
            (self.graph && fixed.is_none() && matches!(sequence.kind, ExprKind::Range { .. }))
                .then(|| self.category(&value));
        let pending_range =
            range_entry == Some(ValueCategory::Unsettled) && !sequence_reported && size.is_some();
        if pending_range && let Type::List { size: extent, .. } = &mut sequence.ty {
            // Keep the pass count on the range until the result's
            // category settles. The range's type is projected as usual.
            *extent = size.clone();
        }
        let ty = if pending_range
            || self.graph && !sequence_reported && size.is_none() && open(&sequence.ty)
        {
            // The fixed list's size and the type of the binding can
            // still depend on instance selection.
            Type::Unknown
        } else {
            Type::List {
                element: Box::new(element.cloned().unwrap_or_else(|| value.ty.clone())),
                size: if range_entry == Some(ValueCategory::Value) {
                    expected_size.cloned()
                } else {
                    size
                },
            }
        };
        (
            ExprKind::ListComprehension {
                composition: self.graph,
                binding,
                sequence: Box::new(sequence),
                entry: Box::new(value),
                ordinary_size: expected_size.filter(|_| pending_range).cloned(),
            },
            ty,
        )
    }

    /// A fixed comprehension's size is the count of its passes, known
    /// before composition or wherever a fixed list is expected (EXPR-17).
    fn comprehension_size(
        &mut self,
        node: &SyntaxNode,
        sequence: &Expr,
        over: ValueCategory,
        expected: Option<&Constant>,
    ) -> Option<Constant> {
        let size = if matches!(sequence.kind, ExprKind::Range { .. }) {
            let mut bounds = node.children();
            let mut read = || {
                // GEN-7: a forwarded body is checked again with the
                // instance's sizes, including those used in range bounds.
                self.given_constant(&bounds.next()?, Some(&Type::Scalar(Scalar::I64)))
                    .ok()
            };
            match (read(), read()) {
                (Some(lower), Some(upper))
                    if crate::constant::worked_out(&lower)
                        && crate::constant::worked_out(&upper) =>
                {
                    Some(ConstantValue::range_length(lower, upper))
                }
                _ => None,
            }
        } else if (!self.graph || over != ValueCategory::Value)
            && let Some(size) = fixed_list_extent(&sequence.ty)
        {
            if crate::constant::worked_out(&size.value) {
                return Some(Constant {
                    text: size.value.key().unwrap_or_else(|| size.text.clone()),
                    value: size.value.clone(),
                });
            }
            None
        } else if !self.graph
            && let ExprKind::Sequence(entries) = &sequence.kind
            && matches!(sequence.ty, Type::List { .. })
        {
            i64::try_from(entries.len()).ok().map(ConstantValue::Int)
        } else {
            None
        };
        let Some(value) = size.filter(crate::constant::worked_out) else {
            if !open(&sequence.ty) {
                self.diagnostics
                    .emit(comprehension_extent_error(sequence.at, self.graph));
            }
            return expected.cloned();
        };
        Some(Constant {
            text: value.key().unwrap_or_else(|| extent_text(node, &value)),
            value,
        })
    }

    /// The source check for a `for` sequence and its binding types.
    pub(super) fn loop_bindings(&mut self, sequence: &Expr, count: usize) -> Vec<Type> {
        let ty = &sequence.ty;
        if let Some(types) = iteration_types(ty, count) {
            return types.into_iter().cloned().collect();
        }
        if !open(ty) {
            let message = if matches!(sequence.kind, ExprKind::Range { .. }) {
                "a range gives one iteration binding".to_string()
            } else if count == 1 {
                format!("this is `{ty}`, and `for` with one name loops over a list or a set")
            } else {
                format!("this is `{ty}`, and `for` with two names loops over a dictionary")
            };
            self.diagnostics.emit(error(
                Category::Type,
                codes::TYPE_MISMATCH,
                sequence.at,
                message,
                self.loop_rule(),
            ));
        }
        vec![Type::Unknown; count]
    }

    /// How many members the enum `ty` has; `None` for a type that is no
    /// enum.
    pub(super) fn enum_members(&self, ty: &Type) -> Option<usize> {
        let Type::Named {
            module,
            declaration,
            ..
        } = ty
        else {
            return None;
        };
        match self.program.modules[*module].details.get(*declaration) {
            Some(Detail::Enum(read)) => Some(read.members.len()),
            _ => None,
        }
    }

    /// CTRL-12 and CTRL-15: the selector chooses the type of every label.
    pub(super) fn switch_type(&mut self, selector: &Expr) -> (Type, Option<usize>) {
        let members = self.enum_members(&selector.ty);
        if open(&selector.ty)
            || members.is_some()
            || matches!(
                selector.ty,
                Type::Scalar(Scalar::Bool | Scalar::I64 | Scalar::Str)
            )
        {
            return (selector.ty.clone(), members);
        }
        self.diagnostics.emit(error(
            Category::Type,
            codes::TYPE_MISMATCH,
            selector.at,
            format!(
                "this is `{}`, and a `switch` selects on a `bool`, `i64`, `str` or enum value",
                selector.ty
            ),
            self.loop_rule(),
        ));
        (Type::Unknown, None)
    }

    /// CTRL-12: "A `switch` there selects on a `bool`, `i64`, `str`, or
    /// enum value. Each label is a constant of the selector's type."
    /// CTRL-6: "Duplicate values reject, including differently spelled
    /// equal enum members. There is no fallthrough ... Enum coverage
    /// can establish exhaustiveness". CTRL-11: a `var` is assigned
    /// "after a `switch` when every case that does not return assigns
    /// it and the cases leave no value out, with a `default` or by
    /// covering an enum".
    pub(super) fn switch(&mut self, node: &SyntaxNode) -> Stmt {
        let at = self.at(node);
        let Some(selector) = node.children().find(|part| {
            !matches!(
                part.kind(),
                SyntaxKind::SwitchCase | SyntaxKind::SwitchDefault
            )
        }) else {
            return self.unread(node);
        };
        let selector = self.expression(&selector, None);
        // CTRL-15: "a `switch` over an ordinary value composes the one
        // case selected, and over a connection each case is a child
        // graph selected at runtime ... a case's assignments and returns
        // follow CTRL-14."
        let outer = self.temporal;
        if self.graph && self.category(&selector) == ValueCategory::Connection {
            self.temporal = Some(self.locals.len());
        }
        let (of, members) = self.switch_type(&selector);
        let before = self.assigned.clone();
        let before_returned = self.returned;
        // What is assigned at the end of each case, and whether it has
        // returned.
        let mut ends: Vec<(Vec<bool>, bool)> = Vec::new();
        let mut seen: Vec<(ConstantValue, Location)> = Vec::new();
        let mut cases = Vec::new();
        let mut default = None;
        for part in node.children() {
            match part.kind() {
                SyntaxKind::SwitchCase => {
                    let mut inside = part.children();
                    let Some(written) = inside.next() else {
                        continue;
                    };
                    self.assigned = restored(&before, self.locals.len(), false);
                    self.returned = before_returned;
                    let label = self.expression(&written, Some(&of));
                    let label = self.fit(label, &of, "the selector");
                    // CTRL-6: "Switch labels are typed source constants,
                    // compared by resolved value": each is read as a
                    // value of the selector's type (`crate::constant`).
                    let read = self.read_constant(&written, Some(&of));
                    // FN-3: "Where a constant is required, a `switch`
                    // label ... a connection rejects."
                    if self.graph && self.category(&label) == ValueCategory::Connection {
                        self.diagnostics.emit(error(
                            Category::Phase,
                            codes::CONNECTION_AS_CONSTANT,
                            label.at,
                            "a `switch` label is a constant, and this is a connection".to_string(),
                            "FN-3",
                        ));
                    } else if matches!(read, Err(Fault::NotConstant)) {
                        // UNIT-13: a constant is "a compile-time constant
                        // where ... a `switch` label (CTRL-12) requires
                        // one". CTRL-12: "Each label is a constant of
                        // the selector's type". GEN-8 says what such a
                        // constant is: "a literal, an enum member however
                        // it is written (TYPE-7) ... a constant (UNIT-13),
                        // a generic value parameter in scope, or a
                        // constant expression of those". One that is a
                        // constant whose value this pass does not work
                        // out, `-{shifted: true} 1` of a candidate of the
                        // program's own, is a label: whether it repeats
                        // another is left unjudged (CTRL-6, "compared by
                        // resolved value").
                        self.diagnostics.emit(error(
                            Category::Type,
                            codes::SWITCH_LABEL_CONSTANT,
                            label.at,
                            "a `switch` label is a compile-time constant, and this is known \
                             only when the program runs"
                                .to_string(),
                            "CTRL-12",
                        ));
                    }
                    let constant = read.ok();
                    let case_type = if of == Type::Unknown { &label.ty } else { &of };
                    let value = constant
                        .as_ref()
                        .and_then(|constant| self::label(self.program, constant, case_type));
                    if let Some(constant) = constant {
                        match seen
                            .iter()
                            // CTRL-6: "compared by resolved value", as the
                            // selector's `==` compares them when the
                            // program runs (`ConstantValue::equals`).
                            .find(|(other, _)| other.equals(&constant) == Some(true))
                        {
                            Some((_, first)) => self
                                .diagnostics
                                .emit(duplicate_switch_label(label.at, *first)),
                            None => seen.push((constant, label.at)),
                        }
                    }
                    // ERR-12: what a null test proves in one case is
                    // proved of no other, and of nothing after the
                    // `switch`: another case may be the one selected.
                    let frame = self.present.len();
                    let body = self.read_statements(inside);
                    self.present.truncate(frame);
                    ends.push((self.assigned.clone(), self.returned));
                    cases.push(hir::Case { label, value, body });
                }
                SyntaxKind::SwitchDefault => {
                    self.assigned = restored(&before, self.locals.len(), false);
                    self.returned = before_returned;
                    let frame = self.present.len();
                    let body = self.read_statements(part.children());
                    self.present.truncate(frame);
                    ends.push((self.assigned.clone(), self.returned));
                    default = Some(body);
                }
                _ => {}
            }
        }
        let covered = seen
            .iter()
            .filter(|(label, _)| matches!(label, ConstantValue::Member(_)))
            .count();
        let exhaustive = default.is_some() || members.is_some_and(|count| covered == count);
        let cases_return = !ends.is_empty() && ends.iter().all(|(_, returned)| *returned);
        if self.graph {
            if open(&selector.ty) && default.is_none() && cases_return {
                self.returning_tails.insert(at, vec![at]);
            } else {
                self.returning_tails.remove(&at);
            }
        }
        let count = self.locals.len();
        if exhaustive && !ends.is_empty() {
            self.assigned = (0..count)
                .map(|local| {
                    // A local a later case declares is not assigned
                    // at the end of an earlier one.
                    ends.iter().all(|(assigned, returned)| {
                        *returned || assigned.get(local).copied().unwrap_or(false)
                    })
                })
                .collect();
            self.returned = cases_return;
        } else {
            self.assigned = restored(&before, count, false);
            self.returned = before_returned;
        }
        self.temporal = outer;
        Stmt::Switch {
            selector,
            cases,
            default,
            at,
        }
    }

    /// The proof of a graph's continuation goes through its tail only.
    pub(super) fn returning_statement(&self, tail: Option<&Stmt>) -> Option<&[Location]> {
        match tail? {
            Stmt::Expr(value) => self.returning_expression(value),
            Stmt::Switch { at, .. } => self.returning_tails.get(at).map(Vec::as_slice),
            _ => None,
        }
    }

    fn returning_expression(&self, tail: &Expr) -> Option<&[Location]> {
        if let ExprKind::Block { statements, value } = &tail.kind {
            match value {
                Some(value) => self.returning_expression(value),
                None => self.returning_statement(statements.last()),
            }
        } else {
            self.returning_tails.get(&tail.at).map(Vec::as_slice)
        }
    }
}

/// What was assigned at `before`, for as many locals as there are now.
/// One declared since is `since`: assigned, when its block has ended
/// and it is in scope no more; not assigned, when it is a `switch`
/// case's, which is in scope after the `switch` (UNIT-12) on paths that
/// did not declare it.
fn restored(before: &[bool], count: usize, since: bool) -> Vec<bool> {
    let mut assigned = before.to_vec();
    assigned.resize(count, since);
    assigned
}

/// What is assigned where two paths meet: a local both assign, a path
/// that has returned counting as assigning all (CTRL-5: "Definite
/// assignment ignores paths that have already returned").
pub(super) fn joined(first: (&[bool], bool), second: (&[bool], bool), count: usize) -> Vec<bool> {
    let on = |(assigned, returned): (&[bool], bool), local: usize| {
        returned || assigned.get(local).copied().unwrap_or(true)
    };
    (0..count)
        .map(|local| on(first, local) && on(second, local))
        .collect()
}

/// The expression that is the value of a branch written as a block: its
/// last statement, when that is an expression.
pub(super) fn value_of_branch(branch: &SyntaxNode) -> Option<SyntaxNode> {
    if branch.kind() != SyntaxKind::Block {
        return None;
    }
    branch
        .children()
        .last()
        .filter(|last| last.kind() == SyntaxKind::ExprStmt)
        .and_then(|last| last.children().next())
}

/// True when a branch's value takes its type from its place: `null` or
/// `.member`, as the value of a block, of a branch of the `if` of an
/// `else if`, or in parentheses.
pub(super) fn branch_takes_its_type(branch: &SyntaxNode) -> bool {
    let branch = &ungrouped(branch.clone());
    match branch.kind() {
        SyntaxKind::Block => {
            value_of_branch(branch).is_some_and(|value| branch_takes_its_type(&value))
        }
        // The condition is first; the branches follow it.
        SyntaxKind::IfExpr => branch
            .children()
            .skip(1)
            .any(|inner| branch_takes_its_type(&inner)),
        _ => takes_its_type(branch),
    }
}

/// True when a branch's value, found as [`branch_takes_its_type`] finds
/// it, is `null`.
pub(super) fn branch_is_null(branch: &SyntaxNode) -> bool {
    let branch = &ungrouped(branch.clone());
    match branch.kind() {
        SyntaxKind::Block => value_of_branch(branch).is_some_and(|value| branch_is_null(&value)),
        SyntaxKind::IfExpr => branch
            .children()
            .skip(1)
            .any(|inner| branch_is_null(&inner)),
        _ => is_null(branch),
    }
}

/// The block `branch` with its value, an integer literal, widened to an
/// `f64` (EXPR-5); `None` when its value is no integer literal.
fn widened_block(branch: &Expr) -> Option<Expr> {
    let ExprKind::Block {
        statements,
        value: Some(value),
    } = &branch.kind
    else {
        return None;
    };
    if !is_integer_literal(value) {
        return None;
    }
    let float = Type::Scalar(Scalar::F64);
    let widened = Expr {
        kind: ExprKind::Widen(value.clone()),
        ty: float.clone(),
        at: value.at,
        category: ValueCategory::Unsettled,
    };
    Some(Expr {
        kind: ExprKind::Block {
            statements: statements.clone(),
            value: Some(Box::new(widened)),
        },
        ty: float,
        at: branch.at,
        category: ValueCategory::Unsettled,
    })
}

/// CTRL-6: a later switch label repeats the first label's resolved value.
pub(super) fn duplicate_switch_label(label: Location, first: Location) -> Diagnostic {
    error(
        Category::Type,
        codes::SWITCH_DUPLICATE_LABEL,
        label,
        "this label has the value of an earlier one".to_string(),
        "CTRL-6",
    )
    .with_related(first, "first here")
}

/// A constant as the stages after the analyser hold a `switch` label's
/// value (`hir::Case::value`) and a key a child is read by: of the
/// types a label may be (CTRL-12), and a float. A member of an enum is
/// by its position among the members of `ty`, the selector's enum.
/// `None` for one of another type, or not valued.
pub(super) fn label(program: &Program, value: &ConstantValue, ty: &Type) -> Option<Label> {
    Some(match value {
        ConstantValue::Int(value) => Label::Int(*value),
        ConstantValue::Bool(value) => Label::Bool(*value),
        ConstantValue::Str(value) => Label::Str(value.clone()),
        ConstantValue::Float(value) => return Label::float(*value),
        ConstantValue::Member(named) => {
            let Some(Type::Named {
                module,
                declaration,
                ..
            }) = not_null(Some(ty))
            else {
                return None;
            };
            let Some(Detail::Enum(read)) = program.modules[*module].details.get(*declaration)
            else {
                return None;
            };
            Label::Member(
                read.members
                    .iter()
                    .position(|member| member.name == named.name)?,
            )
        }
        _ => return None,
    })
}
