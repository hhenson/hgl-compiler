//! The category of a reached return in an immediate anonymous call.
//! The call's typed body is discarded after this check (TIME-2, TYPE-10).

use std::collections::HashSet;
use std::ops::Range;
use std::rc::Rc;

use super::super::{Typer, lifting::ImmediateValues};
use super::ImmediateBindings;
use crate::hir::ValueCategory::{Connection, Value as Ordinary};
use crate::hir::{BinaryOperator, Case, Expr, ExprKind, LabelValue, LocalId, Node, Stmt, Value};
use crate::program::LocalKind;
use crate::types::{ConstantValue, Scalar, Type};

// The calls we classify here have value bodies, with no composition unroll.
// Share one limit between all their fixed ranges, including nested loops.
const RANGE_PASSES: u64 = 128;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Return {
    Ordinary,
    Connection,
}

/// Which reached writes can make a later read depend on a connection.
struct Flow<'a, 'p> {
    typer: &'a Typer<'p>,
    bindings: ImmediateBindings,
    enclosing_bindings: usize,
    ranges: Vec<(LocalId, Range<i64>)>,
    range_passes_left: u64,
    live: HashSet<LocalId>,
}

pub(super) fn result_is_live(
    typer: &Typer<'_>,
    body: &[Stmt],
    known: &[(LocalId, Rc<Expr>)],
) -> bool {
    let mut flow = Flow {
        typer,
        bindings: known.to_vec(),
        enclosing_bindings: known.len(),
        ranges: Vec::new(),
        range_passes_left: RANGE_PASSES,
        live: HashSet::new(),
    };
    flow.statements(body, false) == Err(Return::Connection)
}

impl Flow<'_, '_> {
    fn known(&self) -> ImmediateValues<'_> {
        ImmediateValues::new(self.typer.program, &self.typer.instance, &self.bindings)
            .with_ranges(&self.ranges)
    }

    fn reads_live(&self, value: &Expr) -> bool {
        self.typer
            .category_with_live_locals(value, Some(self.known()), Some(&self.live))
            == Connection
    }

    // An error in this walk means control returned from the function.
    fn statements(&mut self, statements: &[Stmt], controlled: bool) -> Result<(), Return> {
        for statement in statements {
            match statement {
                Stmt::Return { value, .. } => {
                    let mut live = controlled;
                    if let Some(value) = value {
                        self.expression(value, controlled)?;
                        live |= self.reads_live(value);
                    }
                    return Err(if live {
                        Return::Connection
                    } else {
                        Return::Ordinary
                    });
                }
                Stmt::Declare {
                    local,
                    value: Some(value),
                }
                | Stmt::Assign { local, value, .. } => {
                    self.expression(value, controlled)?;
                    if controlled || self.reads_live(value) {
                        self.live.insert(*local);
                    } else {
                        self.live.remove(local);
                        if self.bindings.len() > self.enclosing_bindings
                            && matches!(statement, Stmt::Declare { .. })
                            && self.typer.locals[*local].kind == LocalKind::Let
                            && !self.bindings.iter().any(|(id, _)| id == local)
                            && let Some(value) = self.known().read(value)
                        {
                            // This immutable initializer can read the known
                            // iteration name. It belongs only to this pass.
                            self.bindings.push((*local, Rc::new(value)));
                        }
                    }
                }
                Stmt::AssignPlace { place, value, .. } => {
                    self.expression(place, controlled)?;
                    self.expression(value, controlled)?;
                    if let Some(local) = assigned_root(place)
                        && (controlled || self.reads_live(place) || self.reads_live(value))
                    {
                        self.live.insert(local);
                    }
                }
                Stmt::Expr(value) => self.expression(value, controlled)?,
                Stmt::Switch {
                    selector,
                    cases,
                    default,
                    ..
                } => {
                    self.switch(selector, cases, default.as_deref(), controlled)?;
                }
                Stmt::While {
                    condition: Some(condition),
                    body,
                    ..
                } => {
                    self.expression(condition, controlled)?;
                    let known = self.known().condition(condition);
                    if known != Some(false) {
                        self.loop_passes(body, None, known.is_none(), |flow| {
                            controlled || flow.reads_live(condition)
                        })?;
                    }
                }
                Stmt::For {
                    names,
                    sequence,
                    body,
                    ..
                } => {
                    self.expression(sequence, controlled)?;
                    // A fixed-size connection supplies live items, but its
                    // extent does not choose which passes run (CTRL-15). A
                    // range's bounds do choose them and must be known during
                    // composition (EXPR-16).
                    let controlled = controlled
                        || matches!(&sequence.kind, ExprKind::Range { lower, upper }
                            if self.reads_live(lower) || self.reads_live(upper));
                    if let [binding] = names.as_slice()
                        && let Some(range) = self.fixed_range(sequence)
                    {
                        self.range(*binding, sequence, range, body, controlled)?;
                    } else {
                        let passes = self.passes(sequence);
                        self.loop_passes(body, passes, passes.is_none(), |_| controlled)?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn expression(&mut self, value: &Expr, controlled: bool) -> Result<(), Return> {
        value.walk_nodes_at(1, &mut |node, _| {
            let Node::Expression(value) = node else {
                unreachable!("block statements are read by the flow walker");
            };
            match &value.kind {
                ExprKind::Block { statements, value } => {
                    self.statements(statements, controlled)?;
                    self.maybe_expression(value.as_deref(), controlled)?;
                }
                ExprKind::If {
                    condition,
                    then,
                    otherwise,
                    ..
                } => {
                    self.expression(condition, controlled)?;
                    let otherwise = otherwise.as_deref();
                    match self.known().condition(condition) {
                        Some(true) => self.expression(then, controlled)?,
                        Some(false) => self.maybe_expression(otherwise, controlled)?,
                        None => {
                            let controlled = controlled || self.reads_live(condition);
                            self.alternatives([Some(then.as_ref()), otherwise], |flow, arm| {
                                flow.maybe_expression(arm, controlled)
                            })?;
                        }
                    }
                }
                _ => {
                    let Some((operator @ (BinaryOperator::And | BinaryOperator::Or), left, right)) =
                        value.kind.binary()
                    else {
                        return Ok(true);
                    };
                    self.expression(left, controlled)?;
                    match self.known().condition(left) {
                        Some(value) if value == (operator == BinaryOperator::And) => {
                            self.expression(right, controlled)?;
                        }
                        Some(_) => {}
                        None => {
                            let controlled = controlled || self.reads_live(left);
                            self.alternatives([Some(right), None], |flow, value| {
                                flow.maybe_expression(value, controlled)
                            })?;
                        }
                    }
                }
            }
            Ok(false)
        })
    }

    fn maybe_expression(&mut self, value: Option<&Expr>, controlled: bool) -> Result<(), Return> {
        value.map_or(Ok(()), |value| self.expression(value, controlled))
    }

    fn switch(
        &mut self,
        selector: &Expr,
        cases: &[Case],
        default: Option<&[Stmt]>,
        controlled: bool,
    ) -> Result<(), Return> {
        self.expression(selector, controlled)?;
        if let Some(value) = self.known().selector(selector)
            && cases.iter().all(|case| case.value.is_some())
        {
            let body = cases
                .iter()
                .find(|case| case.value.as_ref() == Some(&value))
                .map(|case| case.body.as_slice())
                .or(default)
                .unwrap_or_default();
            return self.statements(body, controlled);
        }
        // CTRL-15: the live selector chooses a case's return or assignment.
        let controlled = controlled || self.reads_live(selector);
        let exhaustive = self.typer.enum_members(&selector.ty) == Some(cases.len());
        let bodies = cases.iter().map(|case| case.body.as_slice());
        let otherwise = default.or_else(|| (!exhaustive).then_some(&[][..]));
        self.alternatives(bodies.chain(otherwise), |flow, body| {
            flow.statements(body, controlled)
        })
    }

    fn fixed_range(&self, sequence: &Expr) -> Option<Range<i64>> {
        let ExprKind::Range { lower, upper } = &sequence.kind else {
            return None;
        };
        let known = self.known();
        let (LabelValue::Int(lower), LabelValue::Int(upper)) =
            (known.selector(lower)?, known.selector(upper)?)
        else {
            return None;
        };
        Some(lower..upper)
    }

    fn range(
        &mut self,
        binding: LocalId,
        sequence: &Expr,
        mut range: Range<i64>,
        body: &[Stmt],
        controlled: bool,
    ) -> Result<(), Return> {
        while self.range_passes_left != 0 {
            let Some(value) = range.next() else {
                return Ok(());
            };
            self.range_passes_left -= 1;
            let enclosing = self.bindings.len();
            self.bindings.push((
                binding,
                Rc::new(Expr {
                    kind: ExprKind::Literal(Value::Int(value)),
                    at: sequence.at,
                    ty: Type::Scalar(Scalar::I64),
                    category: Ordinary,
                }),
            ));
            let checked = self.statements(body, controlled);
            self.bindings.truncate(enclosing);
            checked?;
        }
        if range.is_empty() {
            return Ok(());
        }
        // A remaining pass can see any of these values. Keep the bounds
        // for comparisons, and join writes without enumerating the range.
        let passes = range.start.abs_diff(range.end);
        self.ranges.push((binding, range));
        let checked = self.loop_passes(body, Some(passes), false, |_| controlled);
        self.ranges.pop();
        checked
    }

    fn passes(&self, sequence: &Expr) -> Option<u64> {
        match &sequence.kind {
            ExprKind::Range { .. } => self.fixed_range(sequence).map(|range| {
                if range.is_empty() {
                    0
                } else {
                    range.start.abs_diff(range.end)
                }
            }),
            ExprKind::Sequence(entries) => Some(entries.len() as u64),
            ExprKind::Pairs(entries) => Some(entries.len() as u64),
            _ => super::super::control::fixed_list_extent(&sequence.ty).and_then(|size| {
                if let ConstantValue::Int(count) = size.value {
                    u64::try_from(count).ok()
                } else {
                    None
                }
            }),
        }
    }

    /// A later pass can read a binding written by an earlier pass (CTRL-15).
    /// Join the bindings reaching the next pass until no additional binding
    /// is live, or until a known pass count is exhausted.
    fn loop_passes(
        &mut self,
        body: &[Stmt],
        mut remaining: Option<u64>,
        may_skip: bool,
        controlled: impl Fn(&Self) -> bool,
    ) -> Result<(), Return> {
        while remaining != Some(0) {
            let before = self.live.clone();
            match self.statements(body, controlled(self)) {
                Ok(()) => {}
                Err(Return::Ordinary) if may_skip => {
                    self.live = before;
                    break;
                }
                Err(returned) => return Err(returned),
            }
            remaining = remaining.map(|count| count - 1);
            if remaining == Some(0) {
                break;
            }
            self.live.extend(&before);
            if self.live == before {
                break;
            }
        }
        Ok(())
    }

    /// Every possible arm starts with the same bindings. Only arms that
    /// continue can supply a later read. A live return ends the inquiry.
    fn alternatives<A>(
        &mut self,
        arms: impl IntoIterator<Item = A>,
        read: impl Fn(&mut Self, A) -> Result<(), Return>,
    ) -> Result<(), Return> {
        let before = self.live.clone();
        let mut joined = None;
        for arm in arms {
            self.live.clone_from(&before);
            match read(self, arm) {
                Ok(()) => joined.get_or_insert_with(HashSet::new).extend(&self.live),
                Err(Return::Ordinary) => {}
                Err(Return::Connection) => return Err(Return::Connection),
            }
        }
        self.live = joined.ok_or(Return::Ordinary)?;
        Ok(())
    }
}

fn assigned_root(place: &Expr) -> Option<LocalId> {
    match &place.kind {
        ExprKind::Local(id) => Some(*id),
        ExprKind::Field { base, .. } | ExprKind::Index { base, .. } => assigned_root(base),
        _ => None,
    }
}
