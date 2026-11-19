//! Control flow in a value function, a hook and a graph: blocks, `if`,
//! `while`, `for` and `switch`, and what is assigned on each path
//! (CTRL-5, CTRL-10 to CTRL-15). Part of the typing pass ([`super`]).

use hgl_diagnostics::{Category, Location};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};

use super::expressions::takes_its_type;
use super::fit::{is_integer_literal, not_null, nullable, open};
use super::symbols::is_null;
use super::{BOOL, Fact, Typer};
use crate::constant::Fault;
use crate::declared::agree;
use crate::hir::{self, Expr, ExprKind, LocalId, Stmt, ValueCategory};
use crate::program::{LocalKind, Program};
use crate::syntax::{child, tokens, ungrouped};
use crate::types::{ConstantValue, Detail, Scalar, Type};
use crate::{codes, error, local_error};

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
fn bound_by_loop(over: ValueCategory, ty: &Type, names: usize) -> (Vec<ValueCategory>, bool) {
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
        let statements = parts[..before]
            .iter()
            .map(|statement| self.statement(statement))
            .collect();
        (statements, last)
    }

    /// An expression that may be a block or an `if`, with what it comes
    /// to. Any other expression has its value. `used` is false when the
    /// value is dropped: a statement, or the last expression of a
    /// function that declares no result (FN-7).
    pub(super) fn flow(
        &mut self,
        node: &SyntaxNode,
        expected: Option<&Type>,
        used: bool,
    ) -> (Expr, Outcome) {
        // "Parentheses select grouping" (EXPR-1).
        let node = &ungrouped(node.clone());
        // SOURCE-4: a block or an `if` whose value is used is a value
        // expression, and a `yield` inside it is reported.
        let valued = used && matches!(node.kind(), SyntaxKind::Block | SyntaxKind::IfExpr);
        self.valued += u32::from(valued);
        let read = match node.kind() {
            SyntaxKind::Block => self.block(node, expected, used),
            SyntaxKind::IfExpr => self.conditional(node, expected, used),
            _ => {
                let value = self.expression(node, expected);
                let ty = value.ty.clone();
                (value, Outcome::Value(ty))
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
    fn block(&mut self, node: &SyntaxNode, expected: Option<&Type>, used: bool) -> (Expr, Outcome) {
        let at = self.at(node);
        // ERR-12: what a null test proves inside the block holds to its
        // end.
        let frame = self.present.len();
        let (expression, outcome) = self.block_read(node, expected, used, at);
        self.present.truncate(frame);
        (expression, outcome)
    }

    /// The block `node`, read: see [`Typer::block`].
    fn block_read(
        &mut self,
        node: &SyntaxNode,
        expected: Option<&Type>,
        used: bool,
        at: Location,
    ) -> (Expr, Outcome) {
        let (statements, last) = self.block_head(node);
        let (value, outcome) = match last {
            // CTRL-5: what follows a `return` is not reached. It is read
            // for what it reports itself, and is not the block's value.
            Some(last) if self.returned => {
                let (value, _) = self.flow(&last, Some(&Type::Unknown), false);
                (Some(Box::new(value)), Outcome::Returns)
            }
            Some(last) => {
                let (value, outcome) = self.flow(&last, expected, used);
                match (outcome, expected) {
                    (Outcome::Value(_), Some(expected)) if !open(expected) => {
                        let reported = self.diagnostics.len();
                        // A branch of a graph's `if` or `switch` gives
                        // its connection across a reference as the
                        // place it stands in does (TIME-10).
                        let value = self.fit_input(value, expected, "the value of this block");
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
        (Expr { kind, ty, at }, outcome)
    }

    /// CTRL-10: "In a value function the condition of an `if` or a
    /// `while` is a `bool`: a `bool?`, a number, or any other type
    /// rejects."
    pub(super) fn condition(&mut self, node: &SyntaxNode) -> Expr {
        let value = self.expression(node, Some(&BOOL));
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
        used: bool,
        (before, before_returned, proved): (&[bool], bool, &[Fact]),
    ) -> Branch {
        self.assigned = restored(before, self.locals.len(), true);
        self.returned = before_returned;
        // ERR-12: what the condition proves holds in the branch, and no
        // further.
        let frame = self.present.len();
        self.present.extend_from_slice(proved);
        let (value, outcome) = self.flow(node, expected, used);
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
        used: bool,
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
            };
            return (unread, Outcome::Value(Type::Unknown));
        };
        let otherwise = parts.next();
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
            None => (self.branch(&then, expected, used, in_then), None),
            Some(otherwise) if !told && by_place(&then) && !by_place(otherwise) => {
                let other = self.branch(otherwise, expected, used, in_else);
                let hint = from_other(&other.outcome, &then);
                let then = self.branch(&then, hint.as_ref().or(expected), used, in_then);
                (then, Some(other))
            }
            Some(otherwise) => {
                let then = self.branch(&then, expected, used, in_then);
                let hint = if !told && by_place(otherwise) {
                    from_other(&then.outcome, otherwise)
                } else {
                    None
                };
                let other = self.branch(otherwise, hint.as_ref().or(expected), used, in_else);
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
                    _ if !used => Type::Unknown,
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
                use ValueCategory::{Connection, Unsettled, Value};
                match (selects, self.category(&then.value)) {
                    // A connection of `T` either way: selected at
                    // runtime, or composed where the condition holds.
                    (Connection, _) | (Value | Unsettled, Connection) => Outcome::Value(ty),
                    (Value, Value) => Outcome::Value(nullable(ty)),
                    (Unsettled, Value | Unsettled) | (Value, Unsettled) => {
                        Outcome::Value(Type::Unknown)
                    }
                }
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
        let kind = ExprKind::If {
            condition: Box::new(condition),
            then: Box::new(then.value),
            otherwise: other.map(|other| Box::new(other.value)),
        };
        (Expr { kind, ty, at }, outcome)
    }

    /// The one type the values of two branches have, with nothing
    /// expected of them: the type of both; the wider of a `T` and a
    /// `T?`; an `f64` for an integer literal and an `f64`, the literal
    /// widened (EXPR-5). Two types are reported at the second branch.
    fn one_type(&mut self, then: &mut Expr, other: &mut Expr) -> Type {
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
        let statements = match block {
            Some(block) => block
                .children()
                .map(|statement| self.statement(&statement))
                .collect(),
            None => Vec::new(),
        };
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
        if let Some(condition) = &condition
            && self.graph
            && self.category(condition) == ValueCategory::Connection
        {
            self.diagnostics.emit(error(
                Category::Phase,
                codes::CONNECTION_AS_CONSTANT,
                condition.at,
                "the condition of a `while` is tested while the graph is built, and this is \
                 a connection"
                    .to_string(),
                "FN-3",
            ));
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
    /// `in` reject."
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
        let sequence = self.expression(&sequence, None);
        let types: Vec<Type> = match (&sequence.ty, names.len()) {
            (ty, count) if open(ty) => vec![Type::Unknown; count],
            (Type::List { element, .. } | Type::Set(element), 1) => vec![(**element).clone()],
            (Type::Dict { key, value }, 2) => vec![(**key).clone(), (**value).clone()],
            (ty, count) => {
                let takes = if count == 1 {
                    "`for` with one name loops over a list or a set"
                } else {
                    "`for` with two names loops over a dictionary"
                };
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::TYPE_MISMATCH,
                    sequence.at,
                    format!("this is `{ty}`, and {takes}"),
                    self.loop_rule(),
                ));
                vec![Type::Unknown; count]
            }
        };
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
        let body = self.loop_body(child(node, SyntaxKind::Block).as_ref());
        self.dynamic = outer;
        Stmt::For {
            names,
            sequence,
            body,
            at,
        }
    }

    /// How many members the enum `ty` has; `None` for a type that is no
    /// enum.
    fn enum_members(&self, ty: &Type) -> Option<usize> {
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
        let members = self.enum_members(&selector.ty);
        let selects = open(&selector.ty)
            || members.is_some()
            || matches!(
                selector.ty,
                Type::Scalar(Scalar::Bool | Scalar::I64 | Scalar::Str)
            );
        if !selects {
            self.diagnostics.emit(error(
                Category::Type,
                codes::TYPE_MISMATCH,
                selector.at,
                format!(
                    "this is `{}`, and a `switch` selects on a `bool`, `i64`, `str` or enum \
                     value",
                    selector.ty
                ),
                self.loop_rule(),
            ));
        }
        // What a label is expected to be.
        let of = if selects {
            selector.ty.clone()
        } else {
            Type::Unknown
        };
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
                        self.diagnostics.emit(local_error(
                            Category::Type,
                            codes::local::SWITCH_LABEL_CONSTANT,
                            label.at,
                            "a `switch` label is a compile-time constant, and this is known \
                             only when the program runs"
                                .to_string(),
                            self.loop_rule(),
                        ));
                    }
                    let constant = read.ok();
                    let value = constant
                        .as_ref()
                        .and_then(|constant| self::label(self.program, constant, &of));
                    if let Some(constant) = constant {
                        match seen
                            .iter()
                            // CTRL-6: "compared by resolved value", as the
                            // selector's `==` compares them when the
                            // program runs (`ConstantValue::equals`).
                            .find(|(other, _)| other.equals(&constant) == Some(true))
                        {
                            Some((_, first)) => self.diagnostics.emit(
                                error(
                                    Category::Type,
                                    codes::SWITCH_DUPLICATE_LABEL,
                                    label.at,
                                    "this label has the value of an earlier one".to_string(),
                                    "CTRL-6",
                                )
                                .with_related(*first, "first here"),
                            ),
                            None => seen.push((constant, label.at)),
                        }
                    }
                    // ERR-12: what a null test proves in one case is
                    // proved of no other, and of nothing after the
                    // `switch`: another case may be the one selected.
                    let frame = self.present.len();
                    let body = inside.map(|statement| self.statement(&statement)).collect();
                    self.present.truncate(frame);
                    ends.push((self.assigned.clone(), self.returned));
                    cases.push(hir::Case { label, value, body });
                }
                SyntaxKind::SwitchDefault => {
                    self.assigned = restored(&before, self.locals.len(), false);
                    self.returned = before_returned;
                    let frame = self.present.len();
                    let body = part
                        .children()
                        .map(|statement| self.statement(&statement))
                        .collect();
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
            self.returned = ends.iter().all(|(_, returned)| *returned);
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
    };
    Some(Expr {
        kind: ExprKind::Block {
            statements: statements.clone(),
            value: Some(Box::new(widened)),
        },
        ty: float,
        at: branch.at,
    })
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
