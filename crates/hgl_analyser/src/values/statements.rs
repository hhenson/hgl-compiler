//! A function's body and its statements: `let`, `var`, an assignment to
//! a local or to a place inside a value, and `return` (FN-7, EXPR-3).
//! Part of the typing pass ([`super`]); `if`, the loops and `switch` are
//! read in [`super::control`].

use hgl_diagnostics::{Category, Diagnostic, Location};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode};

use super::Typer;
use super::candidates::Given;
use super::control::{Outcome, ValueUse};
use super::fit::open;
use super::graph_instance::GraphReturnCheck;
use super::results::{captured_binding, captured_output};
use crate::hir::{
    BinaryOperator, Body, ErrorPart, Expr, ExprKind, Hook, Local, Stmt, ValueCategory,
};
use crate::program::{DeclarationKind, LocalKind, RUN_CONFIG};
use crate::read::is_type;
use crate::syntax::{child, tokens, ungrouped};
use crate::types::{NamedKind, Type};
use crate::{codes, error, local_error};

/// FN-7: a function with a required result can reach its end with none.
pub(super) fn missing_return(at: Location, name: &str, result: &Type) -> Diagnostic {
    error(
        Category::Type,
        codes::RETURN_MISSING,
        at,
        format!("{name} declares a result of `{result}`, and can reach its end with no value"),
        "FN-7",
    )
}

impl<'a> Typer<'a> {
    /// The body, or `None` for a function with none written.
    ///
    /// FN-7: "A body that ends in an expression returns that expression,
    /// as an arrow body does: `{ value }` and `=> value` are one body.
    /// In a function that declares no result the expression is a
    /// statement and its value is dropped ... A `const fn` that declares
    /// a result ends every path in `return value` or in its body's
    /// value: one that can reach its end with no value rejects."
    /// The body of a graph: composition (FN-2, FN-3).
    ///
    /// "There a temporal parameter, and the result of a temporal or
    /// lifted call, is a connection: the argument of a call, the
    /// operand of a symbol, a condition, a bound local, or what is
    /// returned." A `const` parameter is an ordinary value, known while
    /// the graph is built.
    ///
    /// Read so far: locals, calls and symbols, and what is returned. A
    /// conditional and a loop in composition are for a later part
    /// (CTRL-1 to CTRL-8), and are not read.
    pub(super) fn graph(mut self) -> (Option<Body>, Option<GraphReturnCheck>) {
        self.graph = true;
        for (local, parameter) in self.signature.parameters.iter().enumerate() {
            self.locals[local].category =
                if parameter.constant || matches!(parameter.ty, Type::Function(_)) {
                    ValueCategory::Value
                } else {
                    ValueCategory::Connection
                };
        }
        let body = self.function_body();
        // CTRL-14: retain the source flow result for a graph where the
        // continuation would be needed if a return becomes temporal.
        let check_return = body
            .as_ref()
            .filter(|_| {
                self.signature
                    .result
                    .as_ref()
                    .is_some_and(|result| !open(result))
                    && !self.returned
                    && !self.unsure
                    && !self.must_return
            })
            .map(|body| {
                self.returning_statement(body.statements.last())
                    .map_or(GraphReturnCheck::Fallthrough, |switches| {
                        GraphReturnCheck::TailSwitches(switches.to_vec())
                    })
            });
        (body, check_return)
    }

    pub(super) fn body(mut self) -> Option<Body> {
        self.function_body()
    }

    fn function_body(&mut self) -> Option<Body> {
        let syntax = &self.declaration.syntax;
        let result = self.signature.result.clone();
        let at = self.declaration.at;
        let name = format!("`{}`", self.declaration.name);
        let statements = self.check_body(syntax, result.as_ref(), at, &name)?;
        Some(Body {
            locals: std::mem::take(&mut self.locals),
            statements,
        })
    }

    /// Check a declaration's or an anonymous function's body (FN-7).
    pub(super) fn check_body(
        &mut self,
        syntax: &SyntaxNode,
        result: Option<&Type>,
        at: Location,
        name: &str,
    ) -> Option<Vec<Stmt>> {
        let statements = if let Some(block) = child(syntax, SyntaxKind::Block) {
            let (mut statements, last) = self.block_head(&block);
            if let Some(last) = last {
                statements.push(self.final_expression(&last, result));
            }
            statements
        } else {
            let arrow = child(syntax, SyntaxKind::ArrowBody)?;
            match arrow.children().next() {
                Some(value) => vec![self.final_expression(&value, result)],
                None => Vec::new(),
            }
        };
        // FN-7: "A `const fn` that declares a result ends every path in
        // `return value` or in its body's value". CTRL-14: a graph that
        // returns a value in a branch selected at runtime does too:
        // "the continuation returns a value too, and one that can reach
        // its end with none rejects."
        if let Some(result) = result
            && !open(result)
            && !self.returned
            && !self.unsure
            && (self.anonymous.is_some() || !self.graph || self.must_return)
        {
            self.diagnostics.emit(missing_return(at, name, result));
        }
        Some(statements)
    }

    /// The expression a body ends in: what the function returns, when
    /// it declares a result, and a statement when it declares none
    /// (FN-7).
    fn final_expression(&mut self, node: &SyntaxNode, result: Option<&Type>) -> Stmt {
        let at = self.at(node);
        let Some(result) = result else {
            let (value, _) = self.flow(node, None, ValueUse::Discarded);
            return Stmt::Expr(value);
        };
        let value_use = if self
            .anonymous
            .as_ref()
            .is_some_and(|scope| scope.infer_result)
        {
            ValueUse::Inferred
        } else {
            ValueUse::Required
        };
        let (value, outcome) = self.flow(node, Some(result), value_use);
        match outcome {
            Outcome::Value(_) => {
                let value = self.fit_return(value, result);
                self.returned = true;
                Stmt::Return {
                    value: Some(value),
                    at,
                }
            }
            // CTRL-10: an `if` with no `else` whose branch returns is
            // null where its condition does not hold. That is a value
            // of a result written `T?`, and is what the function then
            // returns.
            Outcome::Null if matches!(result, Type::Nullable(_)) => {
                let mut value = value;
                value.ty = result.clone();
                self.returned = true;
                Stmt::Return {
                    value: Some(value),
                    at,
                }
            }
            // Every path through it has returned, or it has no value
            // and the function ends with none. Null alone is no value
            // of a result that is not nullable.
            Outcome::Returns | Outcome::Nothing | Outcome::Null => Stmt::Expr(value),
        }
    }

    // ---- Statements ----

    /// Read a statement list. A `let` pack may contribute several entry
    /// declarations, while ordinary statements contribute one (FN-19).
    pub(super) fn read_statements(
        &mut self,
        nodes: impl IntoIterator<Item = SyntaxNode>,
    ) -> Vec<Stmt> {
        let mut statements = Vec::new();
        for node in nodes {
            self.append_statement(&node, &mut statements);
        }
        statements
    }

    /// Read one statement into its containing list.
    pub(super) fn append_statement(&mut self, node: &SyntaxNode, statements: &mut Vec<Stmt>) {
        if !self.bind_let_pack(node, statements) {
            statements.push(self.statement(node));
        }
    }

    /// A statement of a block. One read where no expression is, is read
    /// once, and has its own allowance of rereads
    /// (`Typer::begin_construct`).
    pub(super) fn statement(&mut self, node: &SyntaxNode) -> Stmt {
        let outer = self.begin_construct(node);
        let statement = self.statement_read(node);
        self.end_construct(outer);
        statement
    }

    /// [`Typer::statement`], within its construct.
    fn statement_read(&mut self, node: &SyntaxNode) -> Stmt {
        match node.kind() {
            SyntaxKind::LocalDecl => self.local(node),
            // FN-9: "A graph injects `global_state` and no other, and
            // uses it where the graph is built". An `inject` of any
            // other capability has made the body a node's (FN-2).
            SyntaxKind::InjectDecl if self.graph && !self.testing => {
                let mut bound = self.inject(node);
                if bound.is_empty() {
                    self.unread(node)
                } else {
                    bound.swap_remove(0)
                }
            }
            // TST-17: an integration test "may inject `run_config`,
            // before its other statements". Where that has no place, and
            // any other capability in a test, the phase pass reports.
            SyntaxKind::InjectDecl if self.testing && self.integration => {
                let mut bound = self.inject(node);
                match bound.iter().position(|statement| {
                    matches!(statement, Stmt::Inject { local, .. }
                        if self.locals[*local].name == RUN_CONFIG)
                }) {
                    Some(position) => bound.swap_remove(position),
                    None => self.unread(node),
                }
            }
            SyntaxKind::AssignStmt => self.assignment(node),
            // An `assert` is a statement of a test's own (TST-3).
            SyntaxKind::AssertStmt if self.testing => self.assertion(node),
            // TST-3: a test "cannot itself declare state/cache, inject,
            // lifecycle hooks, when, yield, or return". The phase pass
            // reports a `return` there, and it is not read.
            SyntaxKind::ReturnStmt if self.testing && self.anonymous.is_none() => self.unread(node),
            // A `yield` is a statement of a source's own (SOURCE-1). In
            // a hook it is reported by the phase pass, and not read.
            SyntaxKind::YieldStmt if self.generating && self.anonymous.is_none() => {
                // SOURCE-4: "A valued return and yield inside a value
                // expression reject."
                if self.valued > 0 {
                    self.diagnostics.emit(error(
                        Category::Phase,
                        codes::YIELD_IN_VALUE,
                        self.at(node),
                        "a `yield` is a statement of the source, and this one stands inside \
                         a block or an `if` whose value is used"
                            .to_string(),
                        "SOURCE-4",
                    ));
                }
                self.yield_statement(node)
            }
            // SOURCE-4: "A bare return or body completion finishes the
            // source ... A valued return ... reject[s]": a source writes
            // its output with `yield` (SOURCE-1). The value is read for
            // what it reports itself.
            SyntaxKind::ReturnStmt if self.generating && self.anonymous.is_none() => {
                let value = node
                    .children()
                    .next()
                    .map(|value| self.expression(&value, Some(&Type::Unknown)));
                if let Some(value) = &value {
                    self.diagnostics.emit(error(
                        Category::Phase,
                        codes::SOURCE_RETURN_VALUE,
                        value.at,
                        "a source writes its output with `yield`: its `return` takes no value"
                            .to_string(),
                        "SOURCE-4",
                    ));
                }
                self.returned = true;
                Stmt::Return {
                    value,
                    at: self.at(node),
                }
            }
            SyntaxKind::ReturnStmt => {
                let value = node.children().next().map(|value| self.returned(&value));
                let at = self.at(node);
                // FN-7: "A value return yields an ordinary value." One
                // with nothing written yields none.
                // In a temporal function a `return` with no value "ends
                // the evaluation and publishes nothing itself".
                if let (None, Some(result)) = (&value, self.result_type())
                    && !open(result)
                    && (self.anonymous.is_some() || self.hook.is_none())
                {
                    self.diagnostics.emit(error(
                        Category::Type,
                        codes::TYPE_MISMATCH,
                        at,
                        format!("this returns no value, and the function's result is `{result}`"),
                        "FN-7",
                    ));
                }
                // "`return value` [rejects] in a function that declares
                // no result."
                if let (Some(value), None) = (&value, self.result_type()) {
                    self.diagnostics.emit(error(
                        Category::Type,
                        codes::TYPE_MISMATCH,
                        value.at,
                        "this returns a value, and the function declares no result".to_string(),
                        "FN-7",
                    ));
                }
                // TIME-7: "a `return` with a value stands in neither
                // `start` nor `stop`."
                if let (Some(value), Some(hook @ (Hook::Start | Hook::Stop))) = (&value, self.hook)
                    && self.anonymous.is_none()
                {
                    self.out_lifecycle(
                        value.at,
                        format!(
                            "a `return` with a value publishes, and `{}` publishes nothing",
                            hook.as_str()
                        ),
                    );
                }
                // CTRL-14: "A `return value` in a branch of a temporal
                // `if` makes the graph's result that value while the
                // branch is selected and the continuation's otherwise".
                if value.is_some() && self.temporal.is_some() {
                    self.must_return = true;
                }
                // Preserve the existing check on a return from a dynamic
                // graph loop until CTRL-15's sequence check lands (#373).
                if self.dynamic.is_some() {
                    self.diagnostics.emit(child_loop_return(at));
                }
                // CTRL-5: "Return targets the enclosing HGL function."
                self.returned = true;
                Stmt::Return { value, at }
            }
            // A block or an `if` on a line of its own: its value, when
            // it has one, is dropped.
            SyntaxKind::ExprStmt => match node.children().next() {
                Some(value) => Stmt::Expr(self.flow(&value, None, ValueUse::Discarded).0),
                None => Stmt::Unread { at: self.at(node) },
            },
            SyntaxKind::WhileStmt => self.while_loop(node),
            SyntaxKind::ForStmt => self.for_loop(node),
            SyntaxKind::SwitchStmt => self.switch(node),
            _ => self.unread(node),
        }
    }

    /// A statement this pass does not read yet.
    pub(super) fn unread(&mut self, node: &SyntaxNode) -> Stmt {
        self.forget();
        self.unsure = true;
        Stmt::Unread { at: self.at(node) }
    }

    /// FN-7: "A value return yields an ordinary value." EXPR-5: the
    /// result type is the type expected of it.
    fn returned(&mut self, node: &SyntaxNode) -> Expr {
        let result = self.result_type().cloned();
        // FN-16: a `return` to a `ref` result designates what it names.
        if let Some(result) = &result
            && self.anonymous.is_none()
            && self.signature.reference
            && self.hook.is_some()
        {
            return self.designation(node, result);
        }
        // TIME-7: "A runtime return publishes to its result."
        if let Some(result) = &result
            && self.anonymous.is_none()
            && (self.hook.is_some() || self.generating)
        {
            return self.published(node, result, "the function's result");
        }
        if let Some(result) = &result {
            let value = self.expression(node, Some(result));
            self.fit_return(value, result)
        } else {
            // The return statement itself reports a value written in
            // a function with no result (FN-7).
            self.expression_in_flow(node, None, ValueUse::Discarded)
        }
    }

    fn fit_return(&mut self, value: Expr, result: &Type) -> Expr {
        let place = "the function's result";
        // A graph may already admit the provisional connection, while
        // a nullable result still decides how it reads the selected call.
        if self.graph && matches!(result, Type::Nullable(_)) {
            self.record_result_read(&value, Some((result, place)));
        }
        // A projected field can be known nullable before its inferred
        // constructor arguments decide the payload. The graph instance
        // fits that result once the payload and category settle (EXPR-5).
        if self.graph
            && self.hook.is_none()
            && matches!(&value.ty, Type::Nullable(payload) if **payload == Type::Unknown)
            && self.category(&value) == ValueCategory::Unsettled
        {
            return value;
        }
        self.fit_input(value, result, place)
    }

    /// EXPR-3: "Initialization fixes a binding's value/connection
    /// category and type; an annotation checks that category rather
    /// than changing it ... An uninitialized typed var supplies no value
    /// or connection."
    pub(super) fn local(&mut self, node: &SyntaxNode) -> Stmt {
        let Some(name) = tokens(node).find(|token| token.kind() == SyntaxKind::Ident) else {
            return self.unread(node);
        };
        // `state` and `cache` are a node's (TIME-5, TIME-6): each is
        // assigned as a `var` is.
        let kind = match tokens(node).next().map(|token| token.kind()) {
            Some(SyntaxKind::KwVar) => LocalKind::Var,
            Some(SyntaxKind::KwState) => LocalKind::State,
            Some(SyntaxKind::KwCache) => LocalKind::Cache,
            _ => LocalKind::Let,
        };
        // The annotation, read by the pass before this one.
        let annotation = node
            .children()
            .find(|part| is_type(part.kind()))
            .and_then(|part| {
                let ty = self.written_type_at(part.span().start)?;
                Some(self.project_pack_pass_type(&ty, self.at(&part)))
            });
        // The first value: the child that is no type.
        let binds_child = annotation.is_none() && kind == LocalKind::Let && self.reads_children();
        let selections_before = self.selecting.borrow().len();
        let value = node
            .children()
            .find(|part| !is_type(part.kind()))
            .map(|value| {
                let before = self.binding_child;
                if binds_child {
                    self.binding_child = Some(self.at(&ungrouped(value.clone())));
                }
                let value = self.expression(&value, annotation.as_ref());
                self.binding_child = before;
                match &annotation {
                    Some(annotation) => self.fit(value, annotation, "the type written for it"),
                    None => value,
                }
            });
        // ERR-14: an inferred `let` captures the selected endpoint.
        // Its initializer evaluates the receiver and index, and every
        // read of the payload is represented at the consuming use.
        let child = binds_child
            .then(|| self.child_origin(value.as_ref()?))
            .flatten();
        let value = value.map(|value| {
            if child.is_some() {
                let (ty, at) = (Type::Endpoint(Box::new(value.ty.clone())), value.at);
                Expr {
                    kind: ExprKind::ChildEndpointOf(Box::new(value)),
                    ty,
                    at,
                    category: ValueCategory::Unsettled,
                }
            } else {
                value
            }
        });
        let ty = match (&annotation, &value) {
            (Some(annotation), _) => annotation.clone(),
            (None, Some(value)) => value.ty.clone(),
            (None, None) => Type::Unknown,
        };
        let at = Location::new(self.file, name.span());
        // CTRL-11: "A `var` whose type is written `T?` is null where it
        // is declared with no value ... it is assigned from there".
        let assigned = value.is_some() || matches!(ty, Type::Nullable(_));
        let local = self.declare(name.text(), at, kind, ty, assigned);
        if annotation.is_none()
            && !self.signature.generics.is_empty()
            && self.instance.is_empty()
            && let Some(projected) = value
                .as_ref()
                .and_then(|value| self.provisional_type(value, selections_before))
        {
            self.provisional_locals.insert(local, projected);
        }
        if let Some(origin) = child {
            self.bound_children.insert(local, origin);
        }
        // TIME-8: a local a capability is bound to is that capability.
        if let Some(capability) = value.as_ref().and_then(|value| self.capability_of(value)) {
            self.aliases.insert(local, capability);
        }
        // ERR-7: "What it gives is bound with `let`".
        if let Some(call) = value
            .as_ref()
            .and_then(|value| captured_binding(kind, value))
        {
            let output = captured_output(self.program, call);
            self.captures.insert(local, output);
        }
        // EXPR-3: "Initialization fixes a binding's value/connection
        // category".
        self.locals[local].category = match &value {
            Some(value) if self.graph => self.category(value),
            // A `var` of a graph declared with no value: its first
            // assignment says what it is.
            None if self.graph => ValueCategory::Unsettled,
            _ => ValueCategory::Value,
        };
        Stmt::Declare { local, value }
    }

    /// EXPR-3: "`let` is immutable; `var` may be assigned within the
    /// same category/type ... A compound assignment reads its existing
    /// target."
    fn assignment(&mut self, node: &SyntaxNode) -> Stmt {
        let mut parts = node.children();
        let (Some(place), Some(value)) = (parts.next(), parts.next()) else {
            return self.unread(node);
        };
        // NATIVE-6: "None is assigned." A property of a native type is
        // read-only, a capability's among them (TIME-8).
        if self.assigned_property(&place) {
            return self.unread(node);
        }
        let local = match place.kind() {
            SyntaxKind::NameExpr => self.local_of(&place),
            // EXPR-3: "An assignment may name a place inside a value".
            SyntaxKind::MemberExpr | SyntaxKind::IndexExpr => {
                return self.place_assignment(node, &place, &value);
            }
            _ => None,
        };
        let Some(local) = local else {
            return self.unread(node);
        };
        let operator = tokens(node).find_map(|token| compound(token.kind()));
        // Where the assignment's operator is written: `+=`.
        let operator_at = tokens(node)
            .find(|token| compound(token.kind()).is_some())
            .map_or(self.at(node), |token| {
                Location::new(self.file, token.span())
            });
        let at = self.at(node);
        let target = self.locals[local].ty.clone();
        // TIME-7: "`inject out` exposes that same result for reads,
        // assignment, and admitted piecewise mutation ... `out` is read
        // in a `when` and in `stop`, and written only in a `when`".
        let is_out = Some(local) == self.out;
        if is_out && let Some(hook @ (Hook::Start | Hook::Stop)) = self.hook {
            self.out_lifecycle(
                self.at(&place),
                format!(
                    "`out` is written only in a `when`, and this is `{}`",
                    hook.as_str()
                ),
            );
        }
        if !is_out
            && !matches!(
                self.locals[local].kind,
                LocalKind::Var | LocalKind::State | LocalKind::Cache
            )
        {
            self.diagnostics.emit(
                error(
                    Category::Name,
                    codes::BINDING_IMMUTABLE,
                    self.at(&place),
                    format!(
                        "`{}` is {} and cannot be assigned: only a `var` can",
                        self.locals[local].name,
                        self.locals[local].kind.describe()
                    ),
                    "EXPR-3",
                )
                .with_related(self.locals[local].at, "declared here"),
            );
            // The value is still read, for what it reports itself.
            let value = self.expression(&value, Some(&target));
            return Stmt::Assign {
                local,
                operator,
                value,
                at,
            };
        }
        let value = match operator {
            // FN-16, proposed: "`out = items[idx]` designates as `return
            // items[idx]` does".
            None if is_out && self.signature.reference => self.designation(&value, &target),
            // TIME-7: an assignment to `out` publishes to the result.
            None if is_out => self.published(&value, &target, "the node's result"),
            None => {
                let value = self.expression(&value, Some(&target));
                self.fit(value, &target, "the type of the `var`")
            }
            Some(operator) => {
                // `name += value` assigns what `name + value` is: the
                // target is the left operand, and is read.
                let left = self.read_local(local, &place);
                let left = Given::read(0, place.clone(), left);
                let right = self.operand(1, value.clone());
                let identity = self.symbol_operator(operator.identity(), operator_at);
                // What the sum is given to says what it is to be.
                let (kind, result) = self.symbol(
                    (operator, operator_at),
                    identity,
                    (left, right),
                    (Vec::new(), false, false),
                    Some(&target),
                );
                // What the operator gives is given to the `var`, in the
                // form the place takes (ERR-15).
                let given = Expr {
                    kind,
                    ty: result,
                    at: self.at(&value),
                    category: ValueCategory::Unsettled,
                };
                let mut given = self.fit(given, &target, "the type of the `var`");
                given.at = at;
                given
            }
        };
        self.assigned[local] = true;
        // Keep the existing checks for dynamic graph loops until the
        // compiler can reject their sequence under CTRL-15 (#373).
        if self.dynamic.is_some_and(|outside| local < outside) {
            self.diagnostics
                .emit(child_loop_write(&self.locals[local], self.at(&place)));
        } else if self.graph {
            let given = (self.category(&value), value.at);
            self.assigned_category(local, given, false);
        }
        // TIME-8: a `var` a capability is assigned to is that
        // capability from there.
        if operator.is_none()
            && let Some(capability) = self.capability_of(&value)
        {
            self.aliases.insert(local, capability);
        }
        Stmt::Assign {
            local,
            operator,
            value,
            at,
        }
    }

    /// `point.x = 1`, `row[0] += 1`, `book["a"] = 1`: an assignment to
    /// a place inside a value.
    ///
    /// EXPR-3: "An assignment may name a place inside a value, at any
    /// depth ... The place's root is a `var`, a `state`, a `cache`, or,
    /// in a `when`, `out` (TIME-7). A `let`, a parameter, and an
    /// iteration binding are read-only all the way down. The value is
    /// of the field's type; of a list's element type, under an `i64`
    /// index; of a dictionary's value type, under a key of its key
    /// type; or of a tuple's position, under a constant index ... A set
    /// has no place to assign. A compound assignment reads the place
    /// first".
    ///
    /// The place is read as the expression it is: what a field or an
    /// element is said to be where it is read is what it takes where
    /// it is assigned, and a set, which has no element to read, has
    /// none to assign.
    ///
    /// EXPR-15: "In a graph, an assignment into a place of a `var` is
    /// copy on write ... `point.x = price` rebinds `point` to a new
    /// connection, which is `point` with its `x` from `price` ... The
    /// place, the type of its value, and a compound assignment are as
    /// EXPR-3 has them." So it is read as any other, and is then an
    /// assignment to the `var`: held to the `var`'s category, and to
    /// what a loop or a branch selected at runtime admits (CTRL-14,
    /// CTRL-15).
    fn place_assignment(
        &mut self,
        node: &SyntaxNode,
        place: &SyntaxNode,
        value: &SyntaxNode,
    ) -> Stmt {
        // "place = IDENTIFIER, { index_suffix | member_suffix }": the
        // root is the name the chain begins with.
        let mut root = place.clone();
        while matches!(root.kind(), SyntaxKind::MemberExpr | SyntaxKind::IndexExpr) {
            let Some(base) = root.children().next() else {
                return self.unread(node);
            };
            root = base;
        }
        let local = (root.kind() == SyntaxKind::NameExpr)
            .then(|| self.local_of(&root))
            .flatten();
        let Some(local) = local else {
            return self.unread(node);
        };
        let at = self.at(node);
        let operator = tokens(node).find_map(|token| compound(token.kind()));
        let operator_at = tokens(node)
            .find(|token| compound(token.kind()).is_some())
            .map_or(at, |token| Location::new(self.file, token.span()));
        // TIME-7: "`out` is read in a `when` and in `stop`, and written
        // only in a `when`". In `start` reading the place reports it.
        let is_out = Some(local) == self.out;
        if is_out && self.hook == Some(Hook::Stop) {
            self.out_lifecycle(
                self.at(place),
                "`out` is written only in a `when`, and this is `stop`".to_string(),
            );
        }
        // FN-16: "A `ref` result designates one endpoint whole: an
        // assignment to a part of its `out`, `out.bid = x` or `out[0] =
        // x`, rejects. Proposed: that assignment rejects with the code
        // `binding.immutable`." The place is named and not read: `out`
        // of a `ref` result is an endpoint, and a read of it is
        // `ref.read`, which is not reported again here (ERR-9). `stop`
        // and `start` write no `out` at all (`out.lifecycle`).
        if is_out && self.signature.reference && self.hook == Some(Hook::When) {
            self.diagnostics.emit(
                error(
                    Category::Name,
                    codes::BINDING_IMMUTABLE,
                    self.at(place),
                    "`out` of a `ref` result designates one endpoint whole, and no part of it \
                     is assigned"
                        .to_string(),
                    "FN-16",
                )
                .with_note("`out = items[idx]` or `return items[idx]` designates an endpoint"),
            );
            let before = self.naming.replace(self.at(&root));
            let target = self.expression(place, None);
            self.naming = before;
            // The value is still read, for what it reports itself.
            let value = self.expression(value, Some(&target.ty));
            return Stmt::AssignPlace {
                place: target,
                operator,
                value,
                at,
            };
        }
        let writable = is_out
            || matches!(
                self.locals[local].kind,
                LocalKind::Var | LocalKind::State | LocalKind::Cache
            );
        let mut target = self.expression(place, None);
        // A hook reads `out` of an atomic as its whole V (`read_local`);
        // as the root of a place it is the atomic it is declared, a part
        // that publishes whole (TYPE-9).
        if is_out {
            root_as_declared(&mut target, &self.locals);
        }
        // NATIVE-6: "None is assigned." TYPE-16: a delta that is a type
        // of its own "has properties, none is assigned". Each is
        // read-only wherever the value is reached from:
        // `items[0].size`.
        if let Some((what, rule)) = Self::read_only_inside(&target) {
            self.diagnostics.emit(error(
                Category::Name,
                codes::BINDING_IMMUTABLE,
                self.at(place),
                format!("this is a property of {what}, and a property is read and not assigned"),
                rule,
            ));
            let value = self.expression(value, Some(&target.ty));
            return Stmt::AssignPlace {
                place: target,
                operator,
                value,
                at,
            };
        }
        if !writable {
            self.diagnostics.emit(
                error(
                    Category::Name,
                    codes::BINDING_IMMUTABLE,
                    self.at(place),
                    format!(
                        "`{}` is {} and is read-only all the way down: no field or element \
                         of it is assigned",
                        self.locals[local].name,
                        self.locals[local].kind.describe()
                    ),
                    "EXPR-3",
                )
                .with_related(self.locals[local].at, "declared here")
                .with_note("copy it to a `var`, and assign into that"),
            );
            // The value is still read, for what it reports itself.
            let value = self.expression(value, Some(&target.ty));
            return Stmt::AssignPlace {
                place: target,
                operator,
                value,
                at,
            };
        }
        let value = match operator {
            None => {
                let value = self.expression(value, Some(&target.ty));
                self.fit(value, &target.ty, "the type of the place")
            }
            Some(operator) => {
                // `place += value` assigns what `place + value` is: the
                // place is the left operand, and has been read.
                let left = Given::read(0, place.clone(), target.clone());
                let right = self.operand(1, value.clone());
                let identity = self.symbol_operator(operator.identity(), operator_at);
                let (kind, result) = self.symbol(
                    (operator, operator_at),
                    identity,
                    (left, right),
                    (Vec::new(), false, false),
                    Some(&target.ty),
                );
                let given = Expr {
                    kind,
                    ty: result,
                    at: self.at(value),
                    category: ValueCategory::Unsettled,
                };
                let mut given = self.fit(given, &target.ty, "the type of the place");
                given.at = at;
                given
            }
        };
        // EXPR-15: in a graph this assignment rebinds the `var`.
        // The dynamic-loop check is retained until the compiler checks
        // the sequence as CTRL-15 requires (#373).
        if self.graph && self.dynamic.is_some_and(|outside| local < outside) {
            self.diagnostics
                .emit(child_loop_write(&self.locals[local], self.at(place)));
        } else if self.graph {
            // What the `var` becomes turns on the value and on every
            // index of the place: `row[index] = 3.0` with `index` a
            // connection selects the element as the graph runs.
            let given = place_assignment_category(&target, &value, |part| self.category(part));
            self.assigned_category(local, given, true);
        }
        Stmt::AssignPlace {
            place: target,
            operator,
            value,
            at,
        }
    }

    /// What a place reaches into that takes no assignment, and the rule
    /// that says so: a native value, whose properties are read-only
    /// (NATIVE-6), or a delta that is a type of its own (TYPE-16). The
    /// place is a property of one or something inside one, however the
    /// value is reached: `items[0].size`, with `items` a list of a
    /// native type, is one. ERR-4: "`.context` is a read-only nullable
    /// record": an error's context and its parts are read-only too.
    pub(super) fn read_only_inside(place: &Expr) -> Option<(String, &'static str)> {
        if let ExprKind::ErrorPart { part, .. } = &place.kind {
            return match part {
                ErrorPart::Context => Some(("`error`".to_string(), "ERR-4")),
                ErrorPart::Node | ErrorPart::Phase | ErrorPart::EvaluationTime => {
                    Some(("an error's context".to_string(), "ERR-4"))
                }
                ErrorPart::Message | ErrorPart::Code => None,
            };
        }
        let (ExprKind::Field { base, .. } | ExprKind::Index { base, .. }) = &place.kind else {
            return None;
        };
        match &base.ty {
            Type::Named {
                kind: NamedKind::Native,
                ..
            } => Some((format!("`{}`, a native type", base.ty), "NATIVE-6")),
            Type::Delta(_) => Some((format!("`{}`, a delta", base.ty), "TYPE-16")),
            _ => Self::read_only_inside(base),
        }
    }

    /// Reports an assignment whose place is a property of a native
    /// type: `clock.evaluation_time = value` (NATIVE-6, TIME-8). True
    /// when it is one.
    ///
    /// Places inside a value are not read yet, so this looks at the one
    /// form it can judge without reading: `name.property`, where the
    /// name is a local of a native type, or a capability, which is of
    /// one whether or not core has declared it.
    fn assigned_property(&mut self, place: &SyntaxNode) -> bool {
        if place.kind() != SyntaxKind::MemberExpr {
            return false;
        }
        // A place begins with a name: `place = IDENTIFIER, { index_suffix
        // | member_suffix }`.
        let Some(base) = place.children().next() else {
            return false;
        };
        let Some(local) = (base.kind() == SyntaxKind::NameExpr)
            .then(|| self.local_of(&base))
            .flatten()
        else {
            return false;
        };
        let native = match &self.locals[local].ty {
            Type::Named {
                module,
                declaration,
                ..
            } => {
                self.program.modules[*module].declarations[*declaration].kind
                    == DeclarationKind::NativeType
            }
            _ => false,
        };
        if !native && !self.is_capability(local) {
            return false;
        }
        let property = tokens(place).last();
        self.diagnostics.emit(
            error(
                Category::Name,
                codes::BINDING_IMMUTABLE,
                self.at(place),
                format!(
                    "`{}.{}` is a property of a native type, and a property is read and \
                     not assigned",
                    self.locals[local].name,
                    property.as_ref().map_or("", |name| name.text())
                ),
                "NATIVE-6",
            )
            .with_related(self.locals[local].at, "declared here"),
        );
        true
    }
}

/// The existing CTRL-15 restriction on writing an outer binding from a child.
pub(super) fn child_loop_write(binding: &Local, at: Location) -> Diagnostic {
    local_error(
        Category::Phase,
        codes::local::LOOP_CARRIED,
        at,
        format!(
            "`{}` is declared outside this loop, which makes one child for each member: \
             nothing is carried from one child to the next",
            binding.name
        ),
        "CTRL-15",
    )
    .with_related(binding.at, "declared here")
}

/// The existing CTRL-15 restriction on returning from a child loop.
pub(super) fn child_loop_return(at: Location) -> Diagnostic {
    local_error(
        Category::Phase,
        codes::local::LOOP_CARRIED,
        at,
        "this loop makes one child for each member, and a child does not return from the graph"
            .to_string(),
        "CTRL-15",
    )
}

/// The place `place` with its root, a local, of the type the local is
/// declared with, and not of the type a hook reads it as: an atomic's
/// V (TYPE-9), or the type under a `?` that marks omission (TYPE-14).
fn root_as_declared(place: &mut Expr, locals: &[Local]) {
    if let ExprKind::Local(local) = place.kind {
        place.ty = locals[local].ty.clone();
        return;
    }
    if let ExprKind::Field { base, .. } | ExprKind::Index { base, .. } = &mut place.kind {
        root_as_declared(base, locals);
    }
}

/// The value and all indices can turn a place assignment into a connection.
pub(super) fn place_assignment_category(
    place: &Expr,
    value: &Expr,
    category: impl Fn(&Expr) -> ValueCategory,
) -> (ValueCategory, Location) {
    use ValueCategory::{Connection, Unsettled, Value};
    let mut parts = vec![value];
    place_indices(place, &mut parts);
    let mut given = (Value, value.at);
    for part in parts {
        match category(part) {
            Connection => return (Connection, part.at),
            Unsettled if given.0 == Value => given = (Unsettled, part.at),
            _ => {}
        }
    }
    given
}

/// Adds to `indices` each index of the place `place`, from the
/// outermost in: the `i` and the `j` of `rows[i].cells[j]`.
fn place_indices<'e>(place: &'e Expr, indices: &mut Vec<&'e Expr>) {
    match &place.kind {
        ExprKind::Index { base, index } => {
            indices.push(index);
            place_indices(base, indices);
        }
        ExprKind::Field { base, .. } => place_indices(base, indices),
        _ => {}
    }
}

/// The operator of a compound assignment: `+` for `+=`. `None` for `=`
/// and for a token that is no assignment.
fn compound(kind: SyntaxKind) -> Option<BinaryOperator> {
    Some(match kind {
        SyntaxKind::PlusEq => BinaryOperator::Add,
        SyntaxKind::MinusEq => BinaryOperator::Subtract,
        SyntaxKind::StarEq => BinaryOperator::Multiply,
        SyntaxKind::SlashEq => BinaryOperator::Divide,
        SyntaxKind::SlashSlashEq => BinaryOperator::FloorDivide,
        SyntaxKind::PercentEq => BinaryOperator::Modulo,
        _ => return None,
    })
}
