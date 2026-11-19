//! Composition as the hgraph target writes it: a graph's `if`, `switch`
//! and `for` over connections as child graphs that hgraph's `switch_`
//! and `map_` wire (CTRL-14, CTRL-15), and a capture of a failure as a
//! graph of its own (ERR-7). Part of the transpiler, the last stage:
//! `emit` writes a module with them.

use super::*;

impl Emitter<'_> {
    /// CTRL-10: "A block whose last statement is an expression has that
    /// expression's value and type". A `let`, a `var`, an assignment to
    /// a name or a `return` whose value is a block with statements in
    /// it, or an `if` with such a block for a branch, in a value
    /// function and a node's hooks, written where `statement` is one:
    /// says whether it was. Python has no expression with statements in
    /// it, so the statements are written in their order and the value
    /// is given after them, in each branch of Python's `if`. "With no
    /// `else` it is as if its `else` were null", and "A branch that
    /// returns from the function gives the `if` no value" (CTRL-5).
    pub(super) fn valued(&mut self, function: &Function, statement: &Stmt, depth: usize) -> bool {
        // A value that has statements in it, at any depth of blocks and
        // `if`s; any other is written as an expression.
        fn has_statements(value: &Expr) -> bool {
            match &value.kind {
                ExprKind::Block { statements, value } => {
                    !statements.is_empty() || value.as_deref().is_some_and(has_statements)
                }
                ExprKind::If {
                    then, otherwise, ..
                } => has_statements(then) || otherwise.as_deref().is_some_and(has_statements),
                _ => false,
            }
        }
        let value = match statement {
            Stmt::Declare {
                value: Some(value), ..
            }
            | Stmt::Assign {
                operator: None,
                value,
                ..
            }
            | Stmt::Return {
                value: Some(value), ..
            } if has_statements(value) => value,
            _ => return false,
        };
        // The statement again, with `value` in place of its own.
        let given = |value: Expr| -> Stmt {
            let mut again = statement.clone();
            match &mut again {
                Stmt::Declare { value: place, .. } | Stmt::Return { value: place, .. } => {
                    *place = Some(value)
                }
                Stmt::Assign { value: place, .. } => *place = value,
                _ => {}
            }
            again
        };
        match &value.kind {
            ExprKind::Block {
                statements,
                value: Some(inner),
            } => {
                let mut all = statements.clone();
                all.push(given((**inner).clone()));
                self.statements(function, &all, depth);
            }
            ExprKind::If {
                condition,
                then,
                otherwise,
            } => {
                let condition = self.expression(function, condition);
                self.line(depth, &format!("if {condition}:"));
                // A branch's statements, then the statement given its
                // value: a block's last expression, or the branch
                // itself; null where it has none.
                let null = Expr {
                    kind: ExprKind::Literal(Value::Null),
                    ty: value.ty.clone(),
                    at: value.at,
                };
                for (position, branch) in [Some(&**then), otherwise.as_deref()]
                    .into_iter()
                    .enumerate()
                {
                    if position == 1 {
                        self.line(depth, "else:");
                    }
                    let mut all = Vec::new();
                    match branch.map(|branch| &branch.kind) {
                        Some(ExprKind::Block { statements, value }) => {
                            all.extend(statements.iter().cloned());
                            if !always_returns(statements) {
                                let value = value.as_deref().cloned().unwrap_or(null.clone());
                                all.push(given(value));
                            }
                        }
                        Some(_) => all.extend(branch.cloned().map(given)),
                        None => all.push(given(null.clone())),
                    }
                    self.block(function, &all, depth + 1);
                }
            }
            // A block with no value gives none: the analyser holds such
            // a value to no place.
            _ => self.unsupported(value.at, "this block as a value"),
        }
        true
    }

    /// An `if` or a `switch` of a graph (CTRL-13 to CTRL-15), written
    /// where `statement` is one: says whether it was, and whether the
    /// statements after it, `rest`, were written with it.
    ///
    /// Over an ordinary value it is decided where the graph is built,
    /// and is Python's own `if`: "a `switch` over an ordinary value
    /// composes the one case selected". Over a connection "each case is
    /// a child graph selected at runtime": hgraph's `switch_`, given a
    /// graph for each branch (`Emitter::switched`).
    ///
    /// CTRL-14: "A `return value` in a branch of a temporal `if` makes
    /// the graph's result that value while the branch is selected and
    /// the continuation's otherwise": the rest of the block is the
    /// other branch's graph. "After the `if` the `var` is one
    /// connection: the branch's binding while that branch is selected,
    /// and the binding it had before for a branch that does not assign
    /// it": each branch's graph gives the `var`, and what `switch_`
    /// gives is assigned to it.
    pub(super) fn branched(
        &mut self,
        function: &Function,
        statement: &Stmt,
        rest: &[Stmt],
        depth: usize,
    ) -> Branched {
        // The statements and the value of a branch: a block's, or an
        // `else if`, which is the value of its `else`.
        fn parts(branch: &Expr) -> (&[Stmt], Option<&Expr>) {
            match &branch.kind {
                ExprKind::Block { statements, value } => (statements, value.as_deref()),
                _ => (&[], Some(branch)),
            }
        }
        match statement {
            // An `if` that stands as a statement, over a connection.
            Stmt::Expr(Expr {
                kind:
                    ExprKind::If {
                        condition,
                        then,
                        otherwise,
                    },
                at,
                ..
            }) if self.connects(function, condition) => {
                // Each branch's statements; an `else if` is the one
                // statement of the `else`.
                let then_owned = alternative_statements(Some(then));
                let then_statements = then_owned.as_slice();
                let other_owned = alternative_statements(otherwise.as_deref());
                let other_statements = other_owned.as_slice();
                if may_return(then_statements) || may_return(other_statements) {
                    if self.unrolled_return(*at) {
                        return Branched::Ended;
                    }
                    // A path that does not return goes on into the rest
                    // of the block: each branch's graph has it after
                    // its own statements.
                    let on = |statements: &[Stmt]| continued(statements, rest);
                    let Some(result) = self.current_result(function) else {
                        self.unsupported(*at, "a `return` in an `if` of a graph with no result");
                        return Branched::Ended;
                    };
                    let branches = [
                        Branch {
                            key: "True".to_string(),
                            statements: on(then_statements),
                            tail: Tail::Returns,
                        },
                        Branch {
                            key: "False".to_string(),
                            statements: on(other_statements),
                            tail: Tail::Returns,
                        },
                    ];
                    let call =
                        self.switched(function, condition, &branches, Some(&result), *at, depth);
                    self.line(depth, &format!("return {call}"));
                    return Branched::Ended;
                }
                let branches = [("True", then_statements), ("False", other_statements)];
                self.assigned_in_branches(function, condition, (&branches, false), *at, depth);
                Branched::Written
            }
            // An `if` over an ordinary value that stands as a statement,
            // with a returning `if` or `switch` in a branch: what follows
            // it is written in each branch, so that the nested one is
            // lowered with its other path.
            Stmt::Expr(Expr {
                kind:
                    ExprKind::If {
                        condition,
                        then,
                        otherwise,
                    },
                ..
            }) if !rest.is_empty()
                && (nested_return(&alternative_statements(Some(then)))
                    || nested_return(&alternative_statements(otherwise.as_deref()))) =>
            {
                let condition = self.expression(function, condition);
                self.line(depth, &format!("if {condition}:"));
                let then = continued(&alternative_statements(Some(then)), rest);
                self.scoped(|emitter| emitter.block(function, &then, depth + 1));
                self.line(depth, "else:");
                let other = continued(&alternative_statements(otherwise.as_deref()), rest);
                self.block(function, &other, depth + 1);
                Branched::Ended
            }
            // An `if` whose value the graph returns, or a `let` or a
            // `var` is given.
            Stmt::Return {
                value:
                    Some(
                        whole @ Expr {
                            kind:
                                ExprKind::If {
                                    condition,
                                    then,
                                    otherwise,
                                },
                            at,
                            ..
                        },
                    ),
                ..
            }
            | Stmt::Declare {
                value:
                    Some(
                        whole @ Expr {
                            kind:
                                ExprKind::If {
                                    condition,
                                    then,
                                    otherwise,
                                },
                            at,
                            ..
                        },
                    ),
                ..
            } => {
                // What the value is given to: the graph's result, or a
                // name.
                let declared = match statement {
                    Stmt::Declare { local, .. } => Some(*local),
                    _ => None,
                };
                let target = declared.map(|local| self.place(function, local));
                let given = |value: String| match &target {
                    Some(target) => format!("{target} = {value}"),
                    None => format!("return {value}"),
                };
                let (then_statements, then_value) = parts(then);
                let other = otherwise.as_deref().map(parts);
                let branch_statements = [
                    Some(then_statements),
                    other.map(|(statements, _)| statements),
                ];
                if self.connects(function, condition) {
                    // A `return` in a branch whose value a `let` or a
                    // `var` is given, in its statements or in its value,
                    // leaves the graph, not the branch's graph (CTRL-5:
                    // "Return targets the enclosing HGL function").
                    let returning = |value: Option<&Expr>| {
                        value.is_some_and(|value| may_return(&[Stmt::Expr(value.clone())]))
                    };
                    if let Some(local) = declared
                        && (branch_statements
                            .iter()
                            .flatten()
                            .any(|statements| may_return(statements))
                            || returning(then_value)
                            || other.is_some_and(|(_, value)| returning(value)))
                    {
                        return self.returned_value(
                            function,
                            (condition, local, *at),
                            [Some((then_statements, then_value)), other],
                            rest,
                            depth,
                        );
                    }
                    // CTRL-14: "After the `if` the `var` is one
                    // connection: the branch's binding while that branch
                    // is selected". `switch_` gives one connection, the
                    // `if`'s value: a `var` of what encloses the `if`
                    // that a branch assigns is not written here. Where
                    // the graph returns the value nothing reads the
                    // `var` after the branch, which reads its own.
                    if declared.is_some()
                        && !assigned_outside(std::slice::from_ref(&Stmt::Expr(whole.clone())))
                            .is_empty()
                    {
                        self.unsupported(
                            *at,
                            "an `if` over a connection that gives a `let` or a `var` its value and assigns a `var` declared before it",
                        );
                        return Branched::Written;
                    }
                    let tail = |value: Option<&Expr>| match value {
                        Some(value) => Tail::Value(Box::new(value.clone())),
                        None => Tail::Nothing,
                    };
                    let branches = [
                        Branch {
                            key: "True".to_string(),
                            statements: then_statements.to_vec(),
                            tail: tail(then_value),
                        },
                        // CTRL-13: "With no `else`, under a temporal
                        // condition it is a connection of `T` that
                        // never ticks while the condition is false".
                        Branch {
                            key: "False".to_string(),
                            statements: other
                                .map_or(Vec::new(), |(statements, _)| statements.to_vec()),
                            tail: other.map_or(Tail::Nothing, |(_, value)| tail(value)),
                        },
                    ];
                    // What a `return` gives is the graph's result, or the
                    // branch's it stands in: each branch's graph gives
                    // that, null where it is nullable (TYPE-14).
                    let result = match declared {
                        Some(_) => Some(whole.ty.clone()),
                        None => self.current_result(function),
                    }
                    .unwrap_or_else(|| whole.ty.clone());
                    let call =
                        self.switched(function, condition, &branches, Some(&result), *at, depth);
                    self.line(depth, &given(call));
                    if let Some(local) = declared {
                        self.bind(function, local);
                    }
                    return Branched::Written;
                }
                // A wiring-time condition: the one branch is composed.
                // "a branch that is a connection makes it a connection
                // of `T` that never ticks where the branch is not
                // composed", and an ordinary value a `T?`.
                let connection = then_value.is_some_and(|value| self.connects(function, value))
                    || other.is_some_and(|(_, value)| {
                        value.is_some_and(|value| self.connects(function, value))
                    });
                let absent = if connection {
                    match self.annotation(&whole.ty, true, *at) {
                        Some(annotation) => format!("_hg.nothing({annotation})"),
                        None => "None".to_string(),
                    }
                } else {
                    "None".to_string()
                };
                // Each branch is its statements and then what it gives,
                // as a statement of its own: a `return` of its value, or
                // the name's declaration, so that an `if` over a
                // connection among them, or as the value, is lowered with
                // what follows it. Where one of them returns, what
                // follows the `let` is written in each branch too.
                // A branch's value that is itself an `if` with a `return`
                // in it needs what follows too.
                let returning_value = |value: Option<&Expr>| {
                    value.is_some_and(|value| may_return(&[Stmt::Expr(value.clone())]))
                };
                let ends = declared.is_some()
                    && (branch_statements
                        .iter()
                        .flatten()
                        .any(|statements| nested_return(statements))
                        || returning_value(then_value)
                        || other.is_some_and(|(_, value)| returning_value(value)));
                let gives = |value: Option<&Expr>| -> Option<Stmt> {
                    let value = value?.clone();
                    let at = value.at;
                    Some(match declared {
                        Some(local) => Stmt::Declare {
                            local,
                            value: Some(value),
                        },
                        None => Stmt::Return {
                            value: Some(value),
                            at,
                        },
                    })
                };
                let condition = self.expression(function, condition);
                self.line(depth, &format!("if {condition}:"));
                let before = self.bound.clone();
                let mut binds = Vec::new();
                let mut reaches = Vec::new();
                let alternatives = [
                    Some((then_statements, then_value)),
                    other.or(Some((&[][..], None))),
                ];
                for (position, (statements, value)) in
                    alternatives.into_iter().flatten().enumerate()
                {
                    if position == 1 {
                        self.line(depth, "else:");
                    }
                    self.bound = before.clone();
                    let mut all = statements.to_vec();
                    if let Some(given) = gives(value) {
                        all.push(given);
                        if ends {
                            all = continued(&all, rest);
                        }
                        self.statements(function, &all, depth + 1);
                    } else if let (true, Some(target), Some(local)) =
                        (ends && nested_return(statements), &target, declared)
                    {
                        // A branch with no value whose statements return
                        // under a nested `if` or `switch`: the name is
                        // given the absent value first, and what follows
                        // comes after the statements, so that the nested
                        // one has it as its other path. Nothing in the
                        // branch reads the name it is the value of.
                        if !always_returns(statements) {
                            self.line(depth + 1, &format!("{target} = {absent}"));
                            self.bind(function, local);
                        }
                        all = continued(statements, rest);
                        self.statements(function, &all, depth + 1);
                    } else {
                        // A branch with no value: its statements, the
                        // absent value, and then what follows.
                        self.statements(function, statements, depth + 1);
                        if !always_returns(statements) {
                            match &target {
                                Some(target) => {
                                    self.line(depth + 1, &format!("{target} = {absent}"))
                                }
                                // An absent ordinary value returned is a
                                // null, published as the `return` of one
                                // is (TYPE-14).
                                None if !connection => {
                                    let null = Stmt::Return {
                                        value: Some(Expr {
                                            kind: ExprKind::Literal(Value::Null),
                                            ty: whole.ty.clone(),
                                            at: *at,
                                        }),
                                        at: *at,
                                    };
                                    self.statement(function, &null, depth + 1);
                                }
                                None => self.line(depth + 1, &format!("return {absent}")),
                            }
                            if let Some(local) = declared {
                                self.bind(function, local);
                            }
                            if ends {
                                self.statements(function, rest, depth + 1);
                                all = continued(statements, rest);
                            }
                        }
                    }
                    reaches.push(!always_returns(&all));
                    binds.push(std::mem::take(&mut self.bound));
                }
                if ends {
                    return Branched::Ended;
                }
                // Python's `if`: what both branches bind is bound after,
                // less a branch that returns, which does not reach the
                // next statement.
                let reached = binds
                    .into_iter()
                    .zip(reaches)
                    .filter_map(|(bound, reaches)| reaches.then_some(bound));
                self.bound = joined(reached).unwrap_or(before);
                if let Some(local) = declared {
                    self.bind(function, local);
                }
                Branched::Written
            }
            // A `switch` over a connection.
            Stmt::Switch {
                selector,
                cases,
                default,
                at,
            } if self.connects(function, selector) => {
                if self.labels_repeat(cases) {
                    return Branched::Ended;
                }
                let mut branches: Vec<(String, &[Stmt])> = Vec::new();
                for case in cases {
                    let label = self.expression(function, &case.label);
                    branches.push((label, &case.body));
                }
                if let Some(default) = default {
                    // CTRL-6: "An unmatched selector uses default".
                    branches.push(("_hg.DEFAULT".to_string(), default));
                }
                // CTRL-6: "An unmatched selector ... fails": with no
                // `default`, `switch_`'s own is a graph that fails as
                // a node's `switch` does (`Tail::Unmatched`).
                let unmatched = default.is_none();
                if branches
                    .iter()
                    .any(|(_, statements)| may_return(statements))
                {
                    if self.unrolled_return(*at) {
                        return Branched::Ended;
                    }
                    let Some(result) = self.current_result(function) else {
                        self.unsupported(*at, "a `return` in a `switch` of a graph with no result");
                        return Branched::Ended;
                    };
                    // A case that can reach its end goes on into the rest
                    // of the block, as an `if`'s branch does. With no
                    // `default`, an unmatched selector fails (CTRL-6).
                    let mut branches: Vec<Branch> = branches
                        .into_iter()
                        .map(|(key, statements)| Branch {
                            key,
                            statements: continued(statements, rest),
                            tail: Tail::Returns,
                        })
                        .collect();
                    if unmatched {
                        branches.push(Branch {
                            key: "_hg.DEFAULT".to_string(),
                            statements: Vec::new(),
                            tail: Tail::Unmatched,
                        });
                    }
                    let call =
                        self.switched(function, selector, &branches, Some(&result), *at, depth);
                    self.line(depth, &format!("return {call}"));
                    return Branched::Ended;
                }
                let branches: Vec<(&str, &[Stmt])> = branches
                    .iter()
                    .map(|(key, statements)| (key.as_str(), *statements))
                    .collect();
                self.assigned_in_branches(function, selector, (&branches, unmatched), *at, depth);
                Branched::Written
            }
            // CTRL-15: "a `switch` over an ordinary value composes the
            // one case selected": the selector is read where the graph
            // is built, and each case is an `if` on it (`Emitter::switch`).
            // A case with a returning `if` or `switch` in it has what
            // follows the `switch` after its own statements, as a
            // wiring-time `if`'s branch does.
            Stmt::Switch {
                selector,
                cases,
                default,
                ..
            } => {
                let returns = cases.iter().any(|case| nested_return(&case.body))
                    || default.as_deref().is_some_and(nested_return);
                let ends = returns && !rest.is_empty();
                let on = |statements: &[Stmt]| {
                    if ends {
                        continued(statements, rest)
                    } else {
                        statements.to_vec()
                    }
                };
                let cases: Vec<Case> = cases
                    .iter()
                    .map(|case| Case {
                        label: case.label.clone(),
                        value: case.value.clone(),
                        body: on(&case.body),
                    })
                    .collect();
                let default = default.as_deref().map(on);
                if !self.switch(function, selector, &cases, default.as_deref(), depth) {
                    return Branched::Ended;
                }
                if ends {
                    Branched::Ended
                } else {
                    Branched::Written
                }
            }
            _ => Branched::No,
        }
    }

    /// What a `return` written now gives: the result of the innermost
    /// branch graph of a `switch_` that encloses it, or the graph's.
    pub(super) fn current_result(&self, function: &Function) -> Option<Type> {
        self.results.last().cloned().or_else(|| {
            function
                .signature
                .and_then(|signature| signature.result.clone())
        })
    }

    /// `written`, what a `return` of a graph gives, as a connection of
    /// the `T?` the graph, or the branch graph it stands in, gives.
    /// TYPE-14: "a T is a value where a T? is expected". hgraph's
    /// `switch_` takes branches of one type, and a connection of `T` is
    /// hgraph's `TS[T]`, not `TS[T | None]`: the provider's `nullable`
    /// gives the one for the other, and a connection that is one
    /// already as it is. FN-3: an ordinary value is "wired as a
    /// constant, one publication when the graph starts", and a null is
    /// published as hgraph's `Null` (TYPE-15). A `T?` of any other `T`
    /// is reported.
    pub(super) fn normalized(
        &mut self,
        function: &Function,
        written: String,
        connection: bool,
        at: Location,
    ) -> String {
        let scalar = match self.current_result(function) {
            Some(Type::Nullable(inner)) if whole_scalar(&inner) => {
                self.annotation(&inner, false, at)
            }
            _ => None,
        };
        let Some(scalar) = scalar else {
            self.unsupported(at, "a nullable result of a graph of this type");
            return written;
        };
        self.natives = true;
        self.hgraph = true;
        // A graph's own result may be either: what a graph gives is
        // taken as it is, and a branch graph that is given it makes it
        // one.
        if connection && self.results.is_empty() {
            written
        } else if connection {
            format!("_core.nullable({written}, {scalar})")
        } else {
            format!("_hg.const(_core.published({written}), _hg.TS[{scalar} | None])")
        }
    }

    /// The case among `cases` whose label has the value of an earlier
    /// one's. CTRL-6: "Duplicate values reject". The analyser rejects
    /// those it can tell apart, and gives each case the value it
    /// resolved its label to (`Case::value`), which is taken here as it
    /// is: the two stages do not decide it twice. A label it could not
    /// resolve, a generic value parameter, is in an instance the literal
    /// or the member the call gives it (GEN-13), which is read as it is.
    fn repeated_label(cases: &[Case]) -> Option<Location> {
        use hgl_analyser::hir::LabelValue;
        let mut seen: Vec<LabelValue> = Vec::new();
        for case in cases {
            let value = case.value.clone().or_else(|| match &case.label.kind {
                ExprKind::Literal(Value::Int(value)) => Some(LabelValue::Int(*value)),
                ExprKind::Literal(Value::Bool(value)) => Some(LabelValue::Bool(*value)),
                ExprKind::Literal(Value::Str(value)) => Some(LabelValue::Str(value.clone())),
                ExprKind::EnumMember { member, .. } => Some(LabelValue::Member(*member)),
                _ => None,
            });
            let Some(value) = value else {
                continue;
            };
            if seen.contains(&value) {
                return Some(case.label.at);
            }
            seen.push(value);
        }
        None
    }

    /// Reports a `switch` two of whose labels have one value (see
    /// [`Emitter::repeated_label`]), and says whether it did: hgraph's
    /// `switch_` would keep the last of them, and Python's `if` the
    /// first.
    fn labels_repeat(&mut self, cases: &[Case]) -> bool {
        match Self::repeated_label(cases) {
            Some(at) => {
                self.unsupported(
                    at,
                    "a `switch` two of whose labels have one value in an instance",
                );
                true
            }
            None => false,
        }
    }

    /// Reports a `return` under a branch selected at runtime where it
    /// stands in a loop unrolled where the graph is built, and says
    /// whether it did. CTRL-14: such a `return` "makes the graph's
    /// result that value while the branch is selected and the
    /// continuation's otherwise", and the continuation of a pass is the
    /// passes after it and what follows the loop, which this target does
    /// not write as a branch's graph.
    fn unrolled_return(&mut self, at: Location) -> bool {
        if self.unrolling > 0 {
            self.unsupported(
                at,
                "a `return` under a branch selected at runtime in a loop unrolled where the graph is built",
            );
        }
        self.unrolling > 0
    }

    /// `let name = if condition { ... } else { ... }`, or a `var`, over
    /// a connection, where a branch returns from the graph in its
    /// statements or in its value. `given` is the condition, the name
    /// and where the `if` is written; `branches` each branch's
    /// statements and value, the second `None` where there is no
    /// `else`.
    ///
    /// CTRL-14: "A `return value` in a branch of a temporal `if` makes
    /// the graph's result that value while the branch is selected and
    /// the continuation's otherwise". The name is the branch's: each
    /// branch's graph has its statements, then the name given the
    /// branch's value, and then what follows, `rest`, which returns.
    /// The graph returns what `switch_` gives. A branch with no value
    /// that can reach its end gives the name a connection that never
    /// ticks (CTRL-13), which is not written here.
    fn returned_value(
        &mut self,
        function: &Function,
        (condition, local, at): (&Expr, usize, Location),
        branches: [Option<(&[Stmt], Option<&Expr>)>; 2],
        rest: &[Stmt],
        depth: usize,
    ) -> Branched {
        if self.unrolled_return(at) {
            return Branched::Ended;
        }
        let Some(result) = self.current_result(function) else {
            self.unsupported(at, "a `return` in an `if` of a graph with no result");
            return Branched::Ended;
        };
        let mut written = Vec::new();
        for (key, branch) in ["True", "False"].into_iter().zip(branches) {
            let (statements, value) = branch.unwrap_or((&[], None));
            let statements = match value {
                _ if always_returns(statements) => statements.to_vec(),
                Some(value) => {
                    let mut all = statements.to_vec();
                    all.push(Stmt::Declare {
                        local,
                        value: Some(value.clone()),
                    });
                    continued(&all, rest)
                }
                None => {
                    self.unsupported(
                        at,
                        "a `return` in an `if` over a connection that gives a `let` or a `var` its value, with a branch that has no value",
                    );
                    return Branched::Ended;
                }
            };
            written.push(Branch {
                key: key.to_string(),
                statements,
                tail: Tail::Returns,
            });
        }
        let call = self.switched(function, condition, &written, Some(&result), at, depth);
        self.line(depth, &format!("return {call}"));
        Branched::Ended
    }

    /// An `if` or a `switch` over the connection `key` whose branches
    /// assign a `var` (CTRL-14): each branch's graph gives the `var`
    /// as the branch leaves it, and what hgraph's `switch_` gives is
    /// assigned to it. One `var` is written; branches that assign
    /// several are reported.
    fn assigned_in_branches(
        &mut self,
        function: &Function,
        key: &Expr,
        (branches, unmatched): (&[(&str, &[Stmt])], bool),
        at: Location,
        depth: usize,
    ) {
        // A `switch` with no `default` fails where no case matches.
        let failing = unmatched.then(|| Branch {
            key: "_hg.DEFAULT".to_string(),
            statements: Vec::new(),
            tail: Tail::Unmatched,
        });
        // The `var`s declared outside the branches that one assigns, by
        // name or by a place inside them.
        let mut assigned: Vec<usize> = Vec::new();
        for (_, statements) in branches {
            for local in assigned_outside(statements) {
                if !assigned.contains(&local) {
                    assigned.push(local);
                }
            }
        }
        // CTRL-3: "An outputless if has no synthetic result; an omitted
        // false branch performs no conditional work": branches that
        // assign nothing are graphs with no output, and `switch_` gives
        // nothing.
        if assigned.is_empty() {
            let branches: Vec<Branch> = branches
                .iter()
                .map(|(key, statements)| Branch {
                    key: (*key).to_string(),
                    statements: statements.to_vec(),
                    tail: Tail::Outputless,
                })
                .chain(failing)
                .collect();
            let call = self.switched(function, key, &branches, None, at, depth);
            self.line(depth, &call);
            return;
        }
        let [var] = assigned.as_slice() else {
            self.unsupported(
                at,
                "an `if` or a `switch` over a connection whose branches assign more than one `var`",
            );
            return;
        };
        let result = function.locals[*var].ty.clone();
        let branches: Vec<Branch> = branches
            .iter()
            .map(|(key, statements)| Branch {
                key: (*key).to_string(),
                statements: statements.to_vec(),
                tail: Tail::Local(*var),
            })
            .chain(failing)
            .collect();
        let call = self.switched(function, key, &branches, Some(&result), at, depth);
        let target = self.place(function, *var);
        self.line(depth, &format!("{target} = {call}"));
        self.bind(function, *var);
    }

    /// hgraph's `switch_` over the connection `key`, with a graph for
    /// each branch, written before the statement at `depth`. Gives the
    /// call. `result` is what each branch's graph gives, `None` for
    /// branches with no output (CTRL-3).
    ///
    /// A child graph is given what it reads: every branch's graph takes
    /// the connections any branch reads that are bound outside it, by
    /// their names, and `switch_` is given them. An ordinary value is
    /// read where the graph is built, as Python reads a name of the
    /// function it is written in. hgraph starts a branch's graph where
    /// its key is selected and stops it where another is, and keeps it
    /// while the key is published again as it was (CTRL-2).
    fn switched(
        &mut self,
        function: &Function,
        key: &Expr,
        branches: &[Branch],
        result: Option<&Type>,
        at: Location,
        depth: usize,
    ) -> String {
        use hgl_analyser::hir::walk_statement_nodes;
        self.hgraph = true;
        let number = self.switched;
        self.switched += 1;
        // What each branch's graph takes: every connection any branch
        // names (`taken`).
        let mut read: Vec<usize> = Vec::new();
        let mut declared: Vec<usize> = Vec::new();
        for branch in branches {
            let mut visit = |node| named_in(node, &mut read, &mut declared);
            walk_statement_nodes(&branch.statements, &mut visit);
            match &branch.tail {
                Tail::Value(value) => value.walk_nodes(&mut visit),
                // A branch that does not assign the `var` gives the
                // binding it had before.
                Tail::Local(var) => read.push(*var),
                Tail::Nothing | Tail::Returns | Tail::Outputless | Tail::Unmatched => {}
            }
        }
        let read = self.child_inputs(function, (read, &declared), None, at);
        let annotation = result.map(|result| self.annotation_or_object(result, true, at));
        let returns = annotation
            .as_ref()
            .map_or(String::new(), |annotation| format!(" -> {annotation}"));
        let parameters = self.parameters_of(function, &read, at);
        let mut cases = Vec::new();
        let outside = std::mem::replace(&mut self.bound, read.clone());
        for (position, branch) in branches.iter().enumerate() {
            // A child graph has its parameters bound, and what its own
            // statements bind.
            self.bound = read.clone();
            let graph = format!("_case_{number}_{position}");
            self.line(depth, "@_hg.graph");
            if let Tail::Unmatched = branch.tail {
                // `switch_` gives the selector to a branch whose first
                // parameter is `key`.
                let failing = self.unmatched_node(key, annotation.as_deref(), at);
                let mut taken = vec![format!(
                    "key: {}",
                    self.annotation_or_object(&key.ty, true, at)
                )];
                taken.extend(parameters.iter().cloned());
                self.line(
                    depth,
                    &format!("def {graph}({}){returns}:", taken.join(", ")),
                );
                let call = format!("{failing}(key=key)");
                self.line(
                    depth + 1,
                    &if annotation.is_some() {
                        format!("return {call}")
                    } else {
                        call
                    },
                );
                cases.push(format!("{}: {graph}", branch.key));
                continue;
            }
            self.line(
                depth,
                &format!("def {graph}({}){returns}:", parameters.join(", ")),
            );
            self.renamed(function, &read, depth + 1);
            // What the branch gives is the last of its statements, so
            // that an `if` or a `switch` among them is lowered with it.
            let mut all = branch.statements.clone();
            match &branch.tail {
                Tail::Value(value) => all.push(Stmt::Return {
                    value: Some((**value).clone()),
                    at: value.at,
                }),
                Tail::Local(var) => all.push(Stmt::Return {
                    value: Some(Expr {
                        kind: ExprKind::Local(*var),
                        ty: function.locals[*var].ty.clone(),
                        at,
                    }),
                    at,
                }),
                Tail::Nothing | Tail::Returns | Tail::Outputless | Tail::Unmatched => {}
            }
            self.in_case += 1;
            self.results.extend(result.cloned());
            let lines = self.statements(function, &all, depth + 1);
            if result.is_some() {
                self.results.pop();
            }
            self.in_case -= 1;
            if let (Tail::Nothing, Some(annotation)) = (&branch.tail, &annotation)
                && !always_returns(&all)
            {
                self.line(depth + 1, &format!("return _hg.nothing({annotation})"));
            } else if lines == 0 {
                // CTRL-3: "an omitted false branch performs no
                // conditional work": a graph with nothing in it, as
                // `switch_` needs a graph for each key it is given.
                self.line(depth + 1, "pass");
            }
            cases.push(format!("{}: {graph}", branch.key));
        }
        let key = self.expression(function, key);
        self.bound = outside;
        let mut given = vec![key, format!("{{{}}}", cases.join(", "))];
        given.extend(read.iter().map(|&local| {
            let python = name(&function.locals[local].name);
            format!("{}={python}", taken_name(&python))
        }));
        format!("_hg.switch_({})", given.join(", "))
    }

    /// The node a `switch_`'s failing `default` wires (`Tail::Unmatched`):
    /// it fails with the provider's `unmatched` when it is evaluated, as
    /// the branch is selected, giving what `annotation` names, or
    /// nothing.
    fn unmatched_node(&mut self, key: &Expr, annotation: Option<&str>, at: Location) -> String {
        self.natives = true;
        let selector = self.annotation_or_object(&key.ty, true, at);
        let (decorator, returns) = match annotation {
            Some(annotation) => ("compute_node", format!(" -> {annotation}")),
            None => ("sink_node", String::new()),
        };
        self.lifted_node(
            "unmatched",
            format!("@_hg.{decorator}\ndef "),
            format!("(key: {selector}){returns}:\n    _core.unmatched(key.value)\n"),
        )
    }

    /// What a child graph of a `switch_` or a `map_` takes of the graph
    /// it is written in (`taken`), from what its statements name and
    /// declare, `read` and `declared`, with each window given to it reported
    /// (`windows_given`): `over` is the loop's sequence, where it is a
    /// loop's.
    fn child_inputs(
        &mut self,
        function: &Function,
        (read, declared): (Vec<usize>, &[usize]),
        over: Option<&Type>,
        at: Location,
    ) -> Vec<usize> {
        let taken = self.taken(function, read, declared);
        self.windows_given(function, &taken, over, at);
        taken
    }

    /// What a child graph takes of the graph it is written in, from
    /// what its statements name, `read`, and declare, `declared`
    /// ([`named_in`]): every connection named, read or assigned, at any
    /// depth, that has a value where the child is written
    /// (`Emitter::bound`), less the names the child declares itself.
    /// One the child does not need is harmless; one with no value yet is
    /// never given. A key that hgraph gives as a port is a connection
    /// here (`Emitter::ported`).
    fn taken(&self, function: &Function, mut read: Vec<usize>, declared: &[usize]) -> Vec<usize> {
        read.retain(|local| {
            !declared.contains(local)
                && self.bound.contains(local)
                && (function.locals[*local].category == ValueCategory::Connection
                    || self.ported.contains(local))
        });
        read.sort_unstable();
        read.dedup();
        read
    }

    /// Reports, at `at`, a window given to a child graph: one that a
    /// connection `taken` holds, itself or within (`windows_in`), or
    /// that the loop's sequence `over` holds. hgraph's `switch_` and
    /// `map_` fail as the graph runs where a `TSW` is given to the
    /// graph they make, alone or as the child of a bundle or a
    /// dictionary, even as a reference to one.
    fn windows_given(
        &mut self,
        function: &Function,
        taken: &[usize],
        over: Option<&Type>,
        at: Location,
    ) {
        let program = self.program;
        let given = taken
            .iter()
            .map(|&local| &function.locals[local].ty)
            .chain(over)
            .any(|ty| !windows_in(program, ty).is_empty());
        if given {
            self.unsupported(at, "a window given to the graph of a branch or of a loop");
        }
    }

    /// The parameters of a child graph that takes the connections
    /// `taken`, each by its name, with its annotation.
    fn parameters_of(&mut self, function: &Function, taken: &[usize], at: Location) -> Vec<String> {
        taken
            .iter()
            .map(|&local| {
                let declared = &function.locals[local];
                let of = self.annotation_or_object(&declared.ty, true, at);
                format!("{}: {of}", taken_name(&name(&declared.name)))
            })
            .collect()
    }

    /// The first lines of a child graph that takes the connections
    /// `taken`: each one taken under a name of the target's own
    /// ([`taken_name`]) is given its own name.
    fn renamed(&mut self, function: &Function, taken: &[usize], depth: usize) {
        for &local in taken {
            let python = name(&function.locals[local].name);
            let given = taken_name(&python);
            if given != python {
                self.line(depth, &format!("{python} = {given}"));
            }
        }
    }

    /// CTRL-12: "`for item in items` runs its body for each element of
    /// a list, in index order, or of a set, and `for key, value in
    /// entries` for each key and value of a dictionary." CTRL-8: "During
    /// node evaluation, traversal uses current values ... Lists preserve
    /// index order": in a hook a temporal input is read as its current
    /// value (`port_value`), a list's a tuple in index order.
    ///
    /// Python's `for` reads what it loops over once, where the loop
    /// begins: a `var` the body assigns is not what is looped over.
    pub(super) fn for_loop(
        &mut self,
        function: &Function,
        (names, sequence): (&[usize], &Expr),
        body: &[Stmt],
        at: Location,
        depth: usize,
    ) {
        let over = self.expression(function, sequence);
        // A loop runs over the value its sequence had where the loop
        // began. Its names are references to the entries, and the loop
        // takes a copy only where its own body changes the place the
        // sequence is read from ([`Copies`]).
        let over = if names
            .first()
            .is_some_and(|&first| function.copies.copied(first))
            && !self.connection(function, sequence)
        {
            self.natives = true;
            format!("_core.copied({over})")
        } else {
            over
        };
        let bound: Vec<String> = names
            .iter()
            .map(|&local| self.place(function, local))
            .collect();
        match (&sequence.ty, bound.as_slice()) {
            (Type::List { .. } | Type::Set(_), [item]) => {
                self.line(depth, &format!("for {item} in {over}:"));
            }
            (Type::Dict { .. }, [key, value]) => {
                self.line(depth, &format!("for {key}, {value} in {over}.items():"));
            }
            _ => {
                self.unsupported(at, "this loop");
                return;
            }
        }
        self.block(function, body, depth + 1);
    }

    /// A `for` of a graph over a connection of `list<T>`, `set<T>` or
    /// `dict<K, V>`. CTRL-15: "it makes one child for each member
    /// (CTRL-7): `item` is a connection of `T` for a list and an
    /// ordinary `T`, the member itself, for a set; `key` is an ordinary
    /// `K` and `value` a connection of `V` for a dictionary." CTRL-7:
    /// "Update keeps the child; removal stops it; reinsertion creates
    /// fresh state. Captured temporal connections are bound whole and
    /// add no membership."
    ///
    /// The body is a graph of its own, `_each_N`, and hgraph's `map_`
    /// runs one of it for each key of a `TSD` it is given, keeps it
    /// while the key stays and stops it where the key is removed; a key
    /// added again has a child that starts afresh. A dictionary's `TSD`
    /// is given as it is, and each child has its value's port. A set's
    /// `TSS` is the keys, `__keys__`. A list, which ticks whole here
    /// (`list_annotation`), is made hgraph's `TSD` of its positions by
    /// hgraph's `convert`: a position that changes ticks, one past the
    /// list's end is removed. hgraph gives a child its key as a port,
    /// `__key_arg__`, and so a key or a member is read only as the whole
    /// argument of a temporal parameter (`key_read_as_value`), where a
    /// port is what is taken. The connections the body reads from
    /// outside it are given whole, `_hg.pass_through`.
    pub(super) fn each(
        &mut self,
        function: &Function,
        names: &[usize],
        sequence: &Expr,
        body: &[Stmt],
        at: Location,
        depth: usize,
    ) {
        use hgl_analyser::hir::walk_statement_nodes;
        // The key hgraph gives each child, and the member's own port.
        let (key, member) = match (&sequence.ty, names) {
            (Type::Set(_), [member]) => (Some(*member), None),
            (Type::Dict { .. }, [key, value]) => (Some(*key), Some(*value)),
            (Type::List { .. }, [item]) => (None, Some(*item)),
            _ => {
                self.unsupported(at, "this loop");
                return;
            }
        };
        if let Some(key) = key
            && let Some(read) = self.key_read_as_value(key, body)
        {
            self.unsupported(
                read,
                "a loop's key or member read other than as the argument of a temporal parameter",
            );
            return;
        }
        self.hgraph = true;
        let number = self.looped;
        self.looped += 1;
        // What the child takes of the graph it is written in, less the
        // loop's own names.
        let mut read: Vec<usize> = Vec::new();
        let mut declared: Vec<usize> = names.to_vec();
        walk_statement_nodes(body, &mut |node| named_in(node, &mut read, &mut declared));
        let key = key.filter(|key| read.contains(key));
        let taken = self.child_inputs(function, (read, &declared), Some(&sequence.ty), at);
        // The child's parameters: the key, where the body reads it, the
        // member's port, and what it takes.
        let mut parameters = Vec::new();
        let mut own = Vec::new();
        if let Some(key) = key {
            // hgraph's key is a `TS` of a scalar.
            let ty = &function.locals[key].ty;
            if !whole_scalar(ty) {
                self.unsupported(at, &format!("a loop's key or member of type `{ty}`"));
            }
            let of = self.annotation_or_object(ty, true, at);
            parameters.push(format!("{}: {of}", name(&function.locals[key].name)));
            own.push(key);
        }
        let mut multiplexed = None;
        if let Some(member) = member {
            let child = match &sequence.ty {
                Type::Dict { value, .. } => self.child_annotation(&sequence.ty, value, at),
                // An element of a list that ticks whole publishes whole.
                Type::List { element, .. } if !is_nullable(element) => {
                    self.annotation(element, true, at)
                }
                _ => {
                    self.unsupported(at, &format!("a loop over `{}`", sequence.ty));
                    None
                }
            };
            let child = child.unwrap_or_else(|| "_py.object".to_string());
            let python = taken_name(&name(&function.locals[member].name));
            parameters.push(format!("{python}: {child}"));
            own.push(member);
            multiplexed = Some((python, child));
        }
        parameters.extend(self.parameters_of(function, &taken, at));
        let graph = format!("_each_{number}");
        self.line(depth, "@_hg.graph");
        self.line(depth, &format!("def {graph}({}):", parameters.join(", ")));
        self.renamed(function, &own[key.iter().len()..], depth + 1);
        self.renamed(function, &taken, depth + 1);
        own.extend(&taken);
        // The child has its parameters bound, and its key is a port.
        let outside = std::mem::replace(&mut self.bound, own);
        let unrolling = std::mem::take(&mut self.unrolling);
        let ported = self.ported.len();
        self.ported.extend(key);
        self.block(function, body, depth + 1);
        self.ported.truncate(ported);
        self.unrolling = unrolling;
        self.bound = outside;
        let over = self.expression(function, sequence);
        let mut given = vec![graph];
        match (&sequence.ty, multiplexed) {
            (Type::Set(_), _) => given.push(format!("__keys__={over}")),
            (Type::List { .. }, Some((member, child))) => given.push(format!(
                "{member}=_hg.convert[_hg.TSD[_py.int, {child}]]({over})"
            )),
            (_, Some((member, _))) => given.push(format!("{member}={over}")),
            (_, None) => {}
        }
        let key_arg = key.map_or(String::new(), |key| name(&function.locals[key].name));
        given.push(format!("__key_arg__={}", string(&key_arg)));
        given.extend(taken.iter().map(|&local| {
            let python = name(&function.locals[local].name);
            format!("{}=_hg.pass_through({python})", taken_name(&python))
        }));
        self.line(depth, &format!("_hg.map_({})", given.join(", ")));
    }

    /// Where `body`, the body of a loop that makes one child for each
    /// member, reads `key`, the loop's key or member, other than as the
    /// whole argument of a temporal parameter of a temporal function:
    /// `None` where it reads it only so. hgraph's `map_` gives a child
    /// its key as a port, and a port is what such a parameter takes; an
    /// ordinary value given one is a constant of it (FN-3), which ticks
    /// where the child starts, as the key's port does. Anywhere else the
    /// key would be a value known where the child is built, which
    /// hgraph does not give.
    fn key_read_as_value(&self, key: usize, body: &[Stmt]) -> Option<Location> {
        use hgl_analyser::hir::{Node, walk_statement_nodes};
        let program = self.program;
        let mut reads = Vec::new();
        let mut given = Vec::new();
        walk_statement_nodes(body, &mut |node| match node {
            Node::Expression(Expr {
                kind: ExprKind::Local(local),
                at,
                ..
            }) if *local == key => reads.push(*at),
            Node::Expression(Expr {
                kind:
                    ExprKind::Call {
                        callee:
                            Callee::Function {
                                module,
                                declaration,
                                ..
                            },
                        arguments,
                        ..
                    },
                ..
            }) => {
                let declared = &program.modules[*module];
                let temporal =
                    declared.declarations[*declaration].kind.role() == Some(Role::Temporal);
                let Some(Detail::Callable(signature)) = declared.details.get(*declaration) else {
                    return;
                };
                for argument in arguments {
                    let taken = argument
                        .parameter
                        .and_then(|index| signature.parameters.get(index))
                        .is_some_and(|parameter| {
                            !parameter.constant && !parameter.var && parameter.pack.is_none()
                        });
                    if temporal
                        && taken
                        && matches!(argument.value.kind, ExprKind::Local(local) if local == key)
                    {
                        given.push(argument.value.at);
                    }
                }
            }
            _ => {}
        });
        reads.into_iter().find(|at| !given.contains(at))
    }

    /// CTRL-6: "Switch labels are typed source constants, compared by
    /// resolved value ... There is no fallthrough. An unmatched
    /// selector uses default or fails." CTRL-12: "A `switch` there
    /// selects on a `bool`, `i64`, `str`, or enum value."
    ///
    /// The selector is evaluated once and held under a name of the
    /// target's own, `_selected_N`, N the depth the `switch` is written
    /// at: one nested in a case has another. Each case is an `if` on
    /// it, and what no case matches falls to the `default`, or fails in
    /// the provider.
    ///
    /// In a graph, CTRL-15: "a `switch` over an ordinary value composes
    /// the one case selected". What is bound after is what each case
    /// that reaches its end binds (`joined`); one that matches no case
    /// fails (CTRL-6). False where it is reported and not written
    /// (`labels_repeat`).
    pub(super) fn switch(
        &mut self,
        function: &Function,
        selector: &Expr,
        cases: &[Case],
        default: Option<&[Stmt]>,
        depth: usize,
    ) -> bool {
        if self.labels_repeat(cases) {
            return false;
        }
        let before = self.bound.clone();
        let mut reached = Vec::new();
        let selected = format!("_selected_{depth}");
        let selector = self.expression(function, selector);
        self.line(depth, &format!("{selected} = {selector}"));
        for (index, case) in cases.iter().enumerate() {
            self.bound = before.clone();
            let label = self.expression(function, &case.label);
            let word = if index == 0 { "if" } else { "elif" };
            self.line(depth, &format!("{word} {selected} == {label}:"));
            self.block(function, &case.body, depth + 1);
            if !always_returns(&case.body) {
                reached.push(std::mem::take(&mut self.bound));
            }
        }
        self.bound = before.clone();
        // With no case there is no `if` for an `else` to follow.
        let inner = if cases.is_empty() {
            depth
        } else {
            self.line(depth, "else:");
            depth + 1
        };
        match default {
            Some(default) => {
                self.block(function, default, inner);
                if !always_returns(default) {
                    reached.push(std::mem::take(&mut self.bound));
                }
            }
            None => {
                self.natives = true;
                self.line(inner, &format!("_core.unmatched({selected})"));
            }
        }
        self.bound = joined(reached).unwrap_or(before);
        true
    }

    /// `if condition { ... } else { ... }` standing as a statement.
    pub(super) fn if_statement(
        &mut self,
        function: &Function,
        condition: &Expr,
        then: &Expr,
        otherwise: Option<&Expr>,
        depth: usize,
    ) {
        let condition = self.expression(function, condition);
        self.line(depth, &format!("if {condition}:"));
        let before = self.bound.clone();
        self.branch(function, then, depth + 1);
        let after_then = std::mem::replace(&mut self.bound, before);
        if let Some(otherwise) = otherwise {
            self.line(depth, "else:");
            self.branch(function, otherwise, depth + 1);
        }
        // What is bound after: what both branches bind, less a branch
        // that returns, which does not reach the next statement.
        // Where neither reaches it, the `else`'s bindings stand.
        let ends = |branch: &Expr| always_returns(&alternative_statements(Some(branch)));
        let after_else = std::mem::take(&mut self.bound);
        let reached = [
            (!ends(then)).then_some(after_then),
            (!otherwise.is_some_and(ends)).then(|| after_else.clone()),
        ];
        self.bound = joined(reached.into_iter().flatten()).unwrap_or(after_else);
    }

    /// Runs `write`, and then gives back what was bound before it
    /// (`Emitter::bound`): what it binds is not bound after. A loop's
    /// body may not run, and only the case selected of a `switch` runs.
    pub(super) fn scoped<T>(&mut self, write: impl FnOnce(&mut Self) -> T) -> T {
        let before = self.bound.clone();
        let written = write(self);
        self.bound = before;
        written
    }

    /// Marks `local` as having a value in the graph being written
    /// (`Emitter::bound`).
    pub(super) fn bind(&mut self, function: &Function, local: usize) {
        if function.mode == Mode::Graph && !self.bound.contains(&local) {
            self.bound.push(local);
        }
    }

    /// A branch of an `if` that stands as a statement: the statements
    /// of its block, and its last expression as one more.
    fn branch(&mut self, function: &Function, branch: &Expr, depth: usize) {
        let before = self.text.len();
        // `else if` is the next `if`, as the one statement of the
        // `else`. Each goes through `statements`, so that in a graph an
        // `if` over a connection in the branch is composition
        // (CTRL-13).
        let statements = alternative_statements(Some(branch));
        self.statements(function, &statements, depth);
        if self.text.len() == before {
            self.line(depth, "pass");
        }
    }

    /// `capture(callable, args...)` in a graph (ERR-7): "a temporal
    /// error boundary" around `call`, the call it names. hgraph's
    /// `try_except` gives a bundle of the child's output, `out`, and its
    /// failures, `exception`; for a child with no output it gives the
    /// failures alone, which are put in a bundle of that one field, so
    /// that `.error` reads either the same way.
    ///
    /// ERR-8: "Capture ends the failed pass without rollback or
    /// immediate retry. The child remains live for future ordinary work"
    /// and "Start/stop failures escape": hgraph catches a failure of the
    /// child's evaluation only, and the child is evaluated again on its
    /// next input. The nearest `try_except` catches a failure, once.
    ///
    /// Written here is the capture of a graph of the program, or of a
    /// source, given no wiring-time value: hgraph's `try_except` makes
    /// the child a graph of its own. Any other capture is written where
    /// it is the value of a `let` (`Emitter::captured_call`), and
    /// reported anywhere else; so is one in a test outside an eval, and
    /// one of a lifted operator or native function.
    pub(super) fn capture(&mut self, function: &Function, call: &Expr) -> String {
        let at = call.at;
        let program = self.program;
        // TST-3: a test is a composition block, and a capture there, as
        // a temporal call there, makes a connection Python has no
        // graph to make in.
        if function.mode == Mode::Test {
            self.unsupported(at, "a capture in a test, outside an eval");
            return "None".to_string();
        }
        let Some(CapturedCall {
            callee: (module, declaration),
            generics,
            arguments,
            declared,
            signature,
            bodied,
        }) = captured_call_of(program, call)
        else {
            self.unsupported(at, "this capture");
            return "None".to_string();
        };
        let temporal = declared.kind.role() == Some(Role::Temporal);
        if !bodied || !(temporal || declared.kind.role() == Some(Role::Value)) {
            self.unsupported(at, "a capture of a lifted operator or native function");
            return "None".to_string();
        }
        if !temporal
            || self.is_node((module, declaration))
            || signature
                .parameters
                .iter()
                .any(|parameter| parameter.constant)
        {
            self.unsupported(
                at,
                "a capture of a node, of a lifted call or of a graph given a wiring-time \
                 value, other than as the value of a `let`",
            );
            return "None".to_string();
        }
        let python = self.called_name((module, declaration), generics, at);
        let arguments = self.arguments(
            function,
            (module, declaration),
            generics,
            signature,
            arguments,
        );
        let given = if arguments.is_empty() {
            python
        } else {
            format!("{python}, {arguments}")
        };
        self.attempted(&given, signature.result.is_none())
    }

    /// hgraph's `try_except` given `given`, its callable and then the
    /// callable's arguments (ERR-7). For a callable with no output,
    /// `outputless`, it gives the failures alone, which are put in a
    /// bundle of that one field, so that `.error` reads either the same
    /// way.
    ///
    /// ERR-8: "Start/stop failures escape, including lifecycle work
    /// triggered during evaluation". hgraph's `try_except` captures the
    /// failure of a child's `start` or `stop` that it meets while it
    /// evaluates what it captures, a branch of a `switch_` selected or
    /// left: the provider's `captured` wires beside it what raises such
    /// a failure again. ERR-1: so it does a failure that is no HGL
    /// error, which no capture takes.
    fn attempted(&mut self, given: &str, outputless: bool) -> String {
        self.hgraph = true;
        self.natives = true;
        let attempted = format!("_hg.try_except({given})");
        if outputless {
            format!("_core.captured(_hg.combine(exception={attempted}))")
        } else {
            format!("_core.captured({attempted})")
        }
    }

    /// `let seen = capture(child, value, 2)` in a graph (ERR-7), where
    /// `child` is a node, a value function lifted over a connection
    /// (FN-6), or a graph or a source given a wiring-time value. Writes
    /// what the capture needs before the statement at `depth`, and
    /// gives the capture; `None` where `call` is a graph or a source
    /// given none, which [`Emitter::capture`] writes.
    ///
    /// The capture is of a graph of the target's own, `_capture_0`,
    /// whose inputs are the child's temporal parameters and which calls
    /// the child with each wiring-time value bound; hgraph's
    /// `try_except` makes it a graph of its own. A node called there is
    /// one no call outside the capture shares: hgraph makes one node of
    /// two calls of a node with the same inputs and the same scalars in
    /// one graph, and `try_except` given a node directly would capture
    /// the failures of every call it is made of. And `try_except` takes
    /// no graph with a scalar input. "capturing a graph covers failures
    /// that leave that subgraph": the subgraph is the child's, called as
    /// it is anywhere else.
    ///
    /// EXPR-13: the arguments are evaluated once, in the order written,
    /// where the capture is: before the `let`'s own line. Each
    /// wiring-time value is held under a name of its own, which the
    /// graph reads. A connection that is a local or a literal is given
    /// as it is; any other is held too, in its place in the order. The
    /// defaults of what the call leaves out are evaluated after, in
    /// the graph's call of the child, as a call evaluates them.
    pub(super) fn captured_call(
        &mut self,
        function: &Function,
        call: &Expr,
        depth: usize,
    ) -> Option<String> {
        let at = call.at;
        let program = self.program;
        let CapturedCall {
            callee,
            generics,
            arguments,
            declared,
            signature,
            bodied,
        } = captured_call_of(program, call)?;
        let node = self.is_node(callee);
        // FN-6: "A selected value helper ... with temporal arguments in
        // composition is lifted". ERR-7: a value function given no
        // connection composes nothing, which the analyser rejects
        // (`capture.phase`).
        let lifted = declared.kind.role() == Some(Role::Value)
            && bodied
            && arguments
                .iter()
                .any(|argument| self.connects(function, &argument.value));
        let constants = signature
            .parameters
            .iter()
            .any(|parameter| parameter.constant);
        let temporal = declared.kind.role() == Some(Role::Temporal) && bodied;
        if !(lifted || temporal && (node || constants)) {
            return None;
        }
        if signature
            .parameters
            .iter()
            .any(|parameter| parameter.pack.is_some())
        {
            self.unsupported(at, "a capture of a call of a function with a pack");
            return Some("None".to_string());
        }
        let number = self.captured;
        self.captured += 1;
        let graph = format!("_capture_{number}");
        // Each argument as `(argument, parameter, value, input)`: the
        // position of its parameter, what it is written as, and whether
        // it is an input of the child, a connection, or its
        // configuration.
        let mut written: Vec<(&Argument, usize, String, bool)> = Vec::new();
        // What a node is given inside `_core.Held(...)`; a lifted node
        // holds its configuration itself (`bound`).
        let kept = if lifted {
            vec![false; signature.parameters.len()]
        } else {
            self.kept_configuration(callee, generics, signature)
        };
        let child = if lifted {
            let (values, bound) = self.bound(function, signature, arguments, |emitter, value| {
                emitter.connects(function, value).then(|| value.ty.clone())
            });
            let result = signature.result.as_ref().map(|_| &call.ty);
            let node = self.lifted(
                callee,
                Lifts::Function(generics),
                signature,
                &bound,
                result,
                at,
            );
            // `bound` writes each argument as `name=value`, in the order
            // of `arguments`, a pack's left out.
            let given = arguments.iter().filter_map(|argument| {
                let index = argument.parameter?;
                signature.parameters[index]
                    .pack
                    .is_none()
                    .then_some((argument, index))
            });
            for ((argument, index), value) in given.zip(values) {
                let value = value
                    .split_once('=')
                    .map_or(value.clone(), |(_, value)| value.to_string());
                let input = matches!(bound[index], Some(Bound::Input(_)));
                written.push((argument, index, value, input));
            }
            node
        } else {
            for argument in arguments {
                let Some(index) = argument.parameter else {
                    continue;
                };
                let value = self.expression(function, &argument.value);
                let value = self.held_value(kept[index], value);
                let input = !signature.parameters[index].constant;
                written.push((argument, index, value, input));
            }
            self.called_name(callee, generics, at)
        };
        // The child's graph is given its inputs by name, and the graph
        // of the capture is given them as `try_except` is.
        let mut inputs = Vec::new();
        let mut given = Vec::new();
        let mut called = Vec::new();
        for (argument, index, value, input) in written {
            let python = name(&signature.parameters[index].name);
            let plain = matches!(
                argument.value.kind,
                ExprKind::Local(_) | ExprKind::Literal(_)
            );
            let value = if !input || !plain {
                let held = format!("{graph}_{python}");
                self.line(depth, &format!("{held} = {value}"));
                held
            } else {
                value
            };
            if input {
                // A type the target does not write is reported there.
                let Some(annotation) = self.annotation(&argument.value.ty, true, at) else {
                    return Some("None".to_string());
                };
                inputs.push(format!("{python}: {annotation}"));
                given.push(format!("{python}={value}"));
                called.push(format!("{python}={python}"));
            } else {
                called.push(format!("{python}={value}"));
            }
        }
        // A lifted node binds what its call leaves out itself.
        if !lifted {
            for (index, parameter) in signature.parameters.iter().enumerate() {
                if arguments
                    .iter()
                    .any(|argument| argument.parameter == Some(index))
                {
                    continue;
                }
                if let Some(value) = self.left_out_of_call(callee, generics, index, parameter) {
                    // A node keeps a default of its configuration as it
                    // keeps one given (`kept_configuration`).
                    let value = self.held_value(kept[index], value);
                    called.push(format!("{}={value}", name(&parameter.name)));
                }
            }
        }
        let result = match &signature.result {
            Some(_) => match self.annotation(&call.ty, true, at) {
                Some(result) => result,
                None => return Some("None".to_string()),
            },
            None => "None".to_string(),
        };
        self.hgraph = true;
        self.line(depth, "@_hg.graph");
        self.line(
            depth,
            &format!("def {graph}({}) -> {result}:", inputs.join(", ")),
        );
        self.line(depth + 1, &format!("return {child}({})", called.join(", ")));
        let given = [graph].into_iter().chain(given).collect::<Vec<_>>();
        Some(self.attempted(&given.join(", "), signature.result.is_none()))
    }
}

/// What `Emitter::branched` made of a statement of a graph.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Branched {
    /// It is no `if` and no `switch` of composition: it is written as
    /// any statement.
    No,
    /// It was written, and the statements after it follow.
    Written,
    /// It was written with the statements after it, as a branch.
    Ended,
}

/// One branch of an `if` or a `switch` over a connection: the child
/// graph hgraph's `switch_` runs while its key is selected.
struct Branch {
    /// The key it is selected by, as Python: `True`, a label, or
    /// hgraph's `DEFAULT`.
    key: String,
    /// Its statements.
    statements: Vec<Stmt>,
    /// What it gives.
    tail: Tail,
}

/// What a branch's graph gives after its statements.
enum Tail {
    /// The branch's value.
    Value(Box<Expr>),
    /// The `var` the branches assign, as the branch leaves it.
    Local(usize),
    /// Nothing: a connection that never ticks (CTRL-13).
    Nothing,
    /// Its statements end with a `return` of their own.
    Returns,
    /// Nothing, and the graph has no output: CTRL-3, "An outputless if
    /// has no synthetic result".
    Outputless,
    /// The `default` of a `switch` that writes none: CTRL-6, "An
    /// unmatched selector uses default or fails". The graph fails when
    /// it is selected, as a node's `switch` does (`_core.unmatched`).
    Unmatched,
}

/// What a capture's call calls (ERR-7), as a captured `let` and a
/// captured call of a graph read it: a function of the program or the
/// candidate of an operator the call resolved to.
struct CapturedCall<'p> {
    /// Its module and its declaration.
    callee: (usize, usize),
    /// The call's generic arguments.
    generics: &'p [GenericArgument],
    /// The call's arguments.
    arguments: &'p [Argument],
    /// Its declaration.
    declared: &'p Declaration,
    /// Its signature.
    signature: &'p Signature,
    /// True for a function or a candidate with a body of its own.
    bodied: bool,
}

/// What `call` calls, where it is a call of a function or a candidate
/// with a signature. `None` for any other expression.
fn captured_call_of<'p>(program: &'p Program, call: &'p Expr) -> Option<CapturedCall<'p>> {
    let ExprKind::Call {
        callee, arguments, ..
    } = &call.kind
    else {
        return None;
    };
    let (module, declaration) = callee.declaration()?;
    let declared = &program.modules[module].declarations[declaration];
    let Some(Detail::Callable(signature)) = program.modules[module].details.get(declaration) else {
        return None;
    };
    let bodied = matches!(
        declared.kind,
        DeclarationKind::Implementation(..) | DeclarationKind::Function(_)
    );
    Some(CapturedCall {
        callee: (module, declaration),
        generics: callee.generics(),
        arguments,
        declared,
        signature,
        bodied,
    })
}

/// What is bound after branches (`Emitter::bound`): what each of
/// `reached`, the bindings of a branch that reaches its end, binds.
/// `None` where no branch reaches it.
fn joined(reached: impl IntoIterator<Item = Vec<usize>>) -> Option<Vec<usize>> {
    reached.into_iter().reduce(|mut joined, bound| {
        joined.retain(|local| bound.contains(local));
        joined
    })
}

/// The statements of a branch of an `if` that stands as a statement:
/// none with no `else`, a block's, with the block's last expression (a
/// nested `if`, say) as a statement of its own, and for an `else if`
/// the nested `if` as its one statement.
fn alternative_statements(otherwise: Option<&Expr>) -> Vec<Stmt> {
    match otherwise {
        None => Vec::new(),
        Some(Expr {
            kind: ExprKind::Block { statements, value },
            ..
        }) => {
            let mut all = statements.clone();
            all.extend(value.iter().map(|value| Stmt::Expr((**value).clone())));
            all
        }
        Some(alternative) => vec![Stmt::Expr(alternative.clone())],
    }
}

/// True where some path through `statements` has a `return`, at any
/// depth.
fn may_return(statements: &[Stmt]) -> bool {
    let mut found = false;
    hgl_analyser::hir::walk_statement_nodes(statements, &mut |node| {
        found |= matches!(
            node,
            hgl_analyser::hir::Node::Statement(Stmt::Return { .. })
        );
    });
    found
}

/// True where an `if` or a `switch` among `statements` has a `return`
/// in it: it needs what follows it to lower its other paths.
fn nested_return(statements: &[Stmt]) -> bool {
    statements.iter().any(|statement| {
        matches!(
            statement,
            Stmt::Switch { .. }
                | Stmt::Expr(Expr {
                    kind: ExprKind::If { .. },
                    ..
                })
        ) && may_return(std::slice::from_ref(statement))
    })
}

/// `statements`, and after them `rest` where they can reach their end.
fn continued(statements: &[Stmt], rest: &[Stmt]) -> Vec<Stmt> {
    let mut all = statements.to_vec();
    if !always_returns(statements) {
        all.extend_from_slice(rest);
    }
    all
}

/// The locals `statements` assign, by name or by a place inside them,
/// that they do not declare themselves: the `var`s of what encloses
/// them that they change.
fn assigned_outside(statements: &[Stmt]) -> Vec<usize> {
    use hgl_analyser::hir::{Node, walk_statement_nodes};
    let mut assigned = Vec::new();
    let mut declared = Vec::new();
    walk_statement_nodes(statements, &mut |node| match node {
        Node::Statement(Stmt::Declare { local, .. }) => declared.push(*local),
        Node::Statement(Stmt::For { names, .. }) => declared.extend(names),
        Node::Statement(Stmt::Assign { local, .. }) => assigned.push(*local),
        Node::Statement(Stmt::AssignPlace { place, .. }) => {
            if let ExprKind::Local(local) = place.place_root().kind {
                assigned.push(local);
            }
        }
        _ => {}
    });
    assigned.retain(|local| !declared.contains(local));
    assigned.sort_unstable();
    assigned.dedup();
    assigned
}

/// Adds what `node`, a part of a child graph's statements, names to
/// `read`, and what it declares to `declared`: a local read or
/// assigned, and a `let`, a `var` or a loop's names.
fn named_in(node: hgl_analyser::hir::Node, read: &mut Vec<usize>, declared: &mut Vec<usize>) {
    use hgl_analyser::hir::Node;
    match node {
        Node::Statement(Stmt::Declare { local, .. }) => declared.push(*local),
        Node::Statement(Stmt::For { names, .. }) => declared.extend(names),
        Node::Statement(Stmt::Assign { local, .. }) => read.push(*local),
        Node::Expression(Expr {
            kind: ExprKind::Local(local),
            ..
        }) => read.push(*local),
        _ => {}
    }
}

/// True where every path through `statements` ends in a `return`: the
/// last is one, or an `if` with an `else` or a `switch` with a
/// `default` each branch of which always returns.
fn always_returns(statements: &[Stmt]) -> bool {
    match statements.last() {
        Some(Stmt::Return { .. }) => true,
        Some(Stmt::Expr(Expr {
            kind:
                ExprKind::If {
                    then,
                    otherwise: Some(otherwise),
                    ..
                },
            ..
        })) => {
            always_returns(&alternative_statements(Some(then)))
                && always_returns(&alternative_statements(Some(otherwise)))
        }
        Some(Stmt::Switch {
            cases,
            default: Some(default),
            ..
        }) => cases.iter().all(|case| always_returns(&case.body)) && always_returns(default),
        _ => false,
    }
}
