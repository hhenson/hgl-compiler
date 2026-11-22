//! What a test and an application run: a named test's block, its
//! `assert` and `eval`, and the traces it compares; an application's run
//! statement, and the statements that set its run's store (TST-3 to
//! TST-17, RUN-1, UNIT-14). Part of the typing pass ([`super`]).

use hgl_diagnostics::{Category, Location};
use hgl_lexer::literal::decode_string;
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};

use super::expressions::written_null;
use super::fit::open;
use super::{BOOL, Called, Typer};
use crate::declared::names_parameter;
use crate::hir::{
    self, BinaryOperator, Body, Expr, ExprKind, Hook, LocalId, Run, Stmt, Value, ValueCategory,
};
use crate::program::{DeclarationKind, LocalKind, RUN_CONFIG, Role, StoreStatement};
use crate::syntax::{child, name_token, test_statements, tokens, ungrouped};
use crate::types::{ConstantValue, Detail, NamedKind, Scalar, TraceStyle, Type};
use crate::{codes, error};

/// TST-5: an eval gives the publications of its input. A type that
/// still names a generic parameter stands for its cells during inference.
pub(super) fn trace_input_type(ty: Type) -> Type {
    let cell = if names_parameter(&ty) || matches!(ty, Type::Unknown) {
        ty
    } else {
        Type::trace_cell(ty)
    };
    Type::Trace {
        cell: Box::new(cell),
        style: None,
    }
}

/// FN-8 and TST-5: a captured trace supplies a type for the pack element,
/// while its cells supply the publications of that type (TYPE-16).
pub(super) fn trace_pack_type(trace: &Type) -> Option<&Type> {
    let Type::Trace { cell, .. } = trace else {
        return None;
    };
    Some(match &**cell {
        Type::Delta(payload) => payload,
        publication => publication,
    })
}

impl<'a> Typer<'a> {
    /// The block of a named test (TST-3): "A test is a block with
    /// assertions. Outside the call an `eval` or a `run` takes, which is
    /// composition (TST-5, TST-8), its statements are value code, as an
    /// application's are (UNIT-14)". A call of a temporal function
    /// there is `call.role`, an operator selects among its value
    /// candidates, and nothing is lifted. The call an `eval` or a `run`
    /// takes is read as composition is (`Typer::eval`). TST-17: an
    /// integration test "may inject `run_config`, before its other
    /// statements ... Its value code may read `run_config` then".
    pub(super) fn test(mut self, integration: bool) -> Option<Body> {
        child(&self.declaration.syntax, SyntaxKind::Block)?;
        self.graph = false;
        self.testing = true;
        self.value_code = true;
        self.integration = integration;
        let statements = test_statements(&self.declaration.syntax)
            .map(|statement| self.statement(&statement))
            .collect();
        Some(Body {
            locals: self.locals,
            statements,
        })
    }

    /// The statements of an application before its run statement
    /// (UNIT-14): "it may inject `run_config`, giving the run's
    /// configuration its defaults (RUN-6), and set the run's store:
    /// `inject global_state`, and then calls of `put` on it. Each is
    /// evaluated once, in the order written, before the run's graph is
    /// built, and its value is read as a constant's is (UNIT-13), with
    /// no parameter in scope, but that a value given to `put` may read
    /// `run_config` where the application has injected it before that
    /// statement (RUN-6). Any other statement there rejects
    /// (`application.statement`)."
    ///
    /// They are read as the statements of a node's `start` are, which
    /// is where a node uses a capability before its graph runs: with
    /// no parameter and no input, each is value code.
    pub(super) fn store(mut self, written: &[StoreStatement]) -> Option<Body> {
        self.hook = Some(Hook::Start);
        self.applying = true;
        // UNIT-14: "Those statements ... are value code", whose operators
        // select among all candidates (GEN-5).
        self.value_code = true;
        let mut statements = Vec::new();
        for store in written {
            let Some(node) = store.syntax.children().next() else {
                continue;
            };
            if node.kind() == SyntaxKind::InjectDecl {
                // FN-9: "An application injects `global_state`, which it
                // uses only to set the run's store before its run
                // statement (UNIT-14), and `run_config` (RUN-6), and no
                // other."
                for name in tokens(&node).filter(|token| token.kind() == SyntaxKind::Ident) {
                    if name.text() != "global_state" && name.text() != RUN_CONFIG {
                        self.diagnostics.emit(error(
                            Category::Phase,
                            codes::APPLICATION_STATEMENT,
                            Location::new(self.file, name.span()),
                            format!(
                                "an application injects `run_config` and `global_state`, and \
                                 not `{}`",
                                name.text()
                            ),
                            "UNIT-14",
                        ));
                    }
                }
                statements.extend(self.inject(&node));
                continue;
            }
            let reported = self.diagnostics.len();
            let statement = self.statement(&node);
            // A call that is itself rejected is not judged again
            // (ERR-9).
            if self.diagnostics.len() == reported && !self.puts_to_the_store(&statement) {
                self.diagnostics.emit(
                    error(
                        Category::Phase,
                        codes::APPLICATION_STATEMENT,
                        store.at,
                        "an application's statement before its run sets the run's store, and \
                         this is no `put` on it"
                            .to_string(),
                        "UNIT-14",
                    )
                    .with_note("`inject global_state`, and then `put(global_state, key, value)`"),
                );
            } else if self.diagnostics.len() == reported {
                // "its value is read as a constant's is (UNIT-13)": the
                // store is `put`'s own argument, and is read by nothing
                // the call is given. A value that reads the store,
                // `get<i64>(global_state, "limit") + 1`, is no
                // constant's. RUN-6: "Its `put` values may read
                // `run_config` after that `inject`, and no other
                // capability."
                for at in self.reads_of_the_store(&statement) {
                    self.diagnostics.emit(
                        error(
                            Category::Phase,
                            codes::APPLICATION_STATEMENT,
                            at,
                            "what a store statement puts is read as a constant's value is, \
                             and reads no capability but `run_config`: the store is `put`'s \
                             own argument"
                                .to_string(),
                            "UNIT-14",
                        )
                        .with_note(
                            "a constant, a call of a `const fn` on constants, or a property of \
                             `run_config` is such a value",
                        ),
                    );
                }
            }
            statements.push(statement);
        }
        Some(Body {
            locals: self.locals,
            statements,
        })
    }

    /// Where a `put` on the store reads the store again, in what it is
    /// given beside the store itself: each use of an injected
    /// capability but `run_config` inside its key or its value.
    fn reads_of_the_store(&self, statement: &Stmt) -> Vec<Location> {
        let Stmt::Expr(Expr {
            kind: ExprKind::Call { arguments, .. },
            ..
        }) = statement
        else {
            return Vec::new();
        };
        let mut reads = Vec::new();
        for argument in arguments {
            // The store itself: `put`'s first parameter.
            if argument.parameter == Some(0) {
                continue;
            }
            argument.value.walk(&mut |part| {
                if let ExprKind::Local(local) = &part.kind
                    && self.locals[*local].kind == LocalKind::Inject
                    && self.locals[*local].name != RUN_CONFIG
                {
                    reads.push(part.at);
                }
            });
        }
        reads
    }

    /// True for a statement that is a call of core's `put` on the
    /// store the unit injects: `put(global_state, "limit", 3)`.
    fn puts_to_the_store(&self, statement: &Stmt) -> bool {
        let Stmt::Expr(Expr {
            kind: ExprKind::Call {
                callee, arguments, ..
            },
            ..
        }) = statement
        else {
            return false;
        };
        let Some((module, declaration)) = callee.declaration() else {
            return false;
        };
        let put = Some(module) == self.program.core()
            && self.program.modules[module].declarations[declaration].name == "put";
        // The store is `put`'s first parameter, wherever its argument
        // is written: `put(key: "limit", store: global_state, value: 3)`.
        let on_store = arguments
            .iter()
            .find(|argument| argument.parameter == Some(0))
            .is_some_and(|argument| {
                matches!(
                    &argument.value.kind,
                    ExprKind::Local(local)
                        if self.locals[*local].kind == LocalKind::Inject
                            && self.locals[*local].name == "global_state"
                )
            });
        put && on_store
    }

    /// `assert condition`, or `assert raises("code") { ... }` (TST-4,
    /// TST-10). TST-4: "Assert requires a wiring-time bool."
    pub(super) fn assertion(&mut self, node: &SyntaxNode) -> Stmt {
        let at = self.at(node);
        let Some(asserted) = node.children().next() else {
            return self.unread(node);
        };
        if asserted.kind() == SyntaxKind::RaisesAssertion {
            // TST-11: "The block has ordinary lexical test scope". Its
            // code is the phase pass's to check: it is read here as it
            // is written, a string literal between the parentheses.
            let code = asserted
                .children()
                .find(|part| part.kind() == SyntaxKind::Literal)
                .and_then(|literal| tokens(&literal).next())
                .filter(|token| token.kind() == SyntaxKind::Str)
                .and_then(|token| decode_string(token.text()).ok());
            let body = self.loop_body(child(&asserted, SyntaxKind::Block).as_ref());
            return Stmt::AssertRaises { code, body, at };
        }
        let condition = self.expression(&asserted, Some(&BOOL));
        let condition = match self.taken(&BOOL, &condition.ty) {
            Some(form) => self.take(condition, form),
            None => condition,
        };
        // What is not known at all has been reported, or is for a later
        // part: an `eval`, and so a comparison with one (ERR-9). A type
        // known in part is known to be no `bool`: the list of `[]`.
        if !open(&condition.ty) && condition.ty != BOOL {
            self.diagnostics.emit(error(
                Category::Type,
                codes::TYPE_MISMATCH,
                condition.at,
                format!("an `assert` takes a `bool`, and this is `{}`", condition.ty),
                "TST-4",
            ));
        } else if self.category(&condition) == ValueCategory::Connection {
            // "Wiring-time": known when the test's graph is built, and
            // no connection, whose value is known only as the graph
            // runs.
            self.diagnostics.emit(
                error(
                    Category::Phase,
                    codes::CONNECTION_AS_CONSTANT,
                    condition.at,
                    "an `assert` takes a `bool` known when the test is built, and this is a \
                     connection"
                        .to_string(),
                    "TST-4",
                )
                .with_note("what a function publishes is observed with `eval`"),
            );
        }
        Stmt::Assert { condition, at }
    }

    /// True when the expression at `node` is one that gives a trace:
    /// an `eval`, or a local one is bound to, in parentheses or not.
    pub(super) fn gives_trace(&self, node: &SyntaxNode) -> bool {
        let node = &ungrouped(node.clone());
        match node.kind() {
            SyntaxKind::EvalExpr | SyntaxKind::RunExpr => true,
            SyntaxKind::NameExpr => self
                .local_of(node)
                .is_some_and(|local| matches!(self.locals[local].ty, Type::Trace { .. })),
            _ => false,
        }
    }

    /// `left == right` or `left != right` where one side gives a trace
    /// (TST-4). TST-3: a trace is "compared with `==` or `!=` against a
    /// trace written out or another eval". The side that gives a trace
    /// says what the other is: a sequence written out is read as a
    /// trace of the same cells. TST-7: the two are of one style.
    pub(super) fn trace_comparison(
        &mut self,
        operator: BinaryOperator,
        left: &SyntaxNode,
        right: &SyntaxNode,
    ) -> (ExprKind, Type) {
        // The operands are read in the order written (EXPR-13): a block
        // in one may assign what the other reads (CTRL-10). Where only
        // the right one gives a trace, what it is of is learned from a
        // reading that is taken back, and the left is then read as a
        // trace of that before the right is read where it stands.
        let left_leads = self.gives_trace(left);
        let (left, right) = if left_leads {
            let left = self.expression(left, None);
            // An eval of a target with no result has been reported,
            // and what it is compared with is not judged (ERR-9).
            let expected = match &left.ty {
                ty @ Type::Trace { .. } => ty.clone(),
                _ => Type::Unknown,
            };
            let right = self.expression(right, Some(&expected));
            let right = self.fit(right, &expected, "the trace it is compared with");
            (left, right)
        } else {
            let expected = if self.may_reread() {
                let mark = self.mark();
                let probe = self.expression(right, None);
                self.take_back(mark, false);
                match probe.ty {
                    ty @ Type::Trace { .. } => ty,
                    _ => Type::Unknown,
                }
            } else {
                Type::Unknown
            };
            let left = self.expression(left, Some(&expected));
            let right = self.expression(right, None);
            // The right says what the left is to be, now that it is
            // read where it stands.
            let expected = match &right.ty {
                ty @ Type::Trace { .. } => ty.clone(),
                _ => Type::Unknown,
            };
            let left = self.fit(left, &expected, "the trace it is compared with");
            (left, right)
        };
        // TST-7: "Within one eval all inputs and expectation use the
        // same dense/timed style", reported at the later trace.
        if let (
            Type::Trace {
                style: Some(first), ..
            },
            Type::Trace {
                style: Some(later), ..
            },
        ) = (&left.ty, &right.ty)
            && !first.same_notation(*later)
        {
            self.trace_style(right.at, *first, *later);
        }
        let kind = ExprKind::TraceCompare {
            left: Box::new(left),
            right: Box::new(right),
            negated: operator == BinaryOperator::NotEqual,
        };
        (kind, BOOL)
    }

    /// `eval target(...)` in a named test.
    ///
    /// TST-5: "Eval takes a call of a module function, written as any
    /// call is: `eval target(value: [7, _, 9])`. The call binds its
    /// arguments by the function's public positional/named rules, and
    /// its generic arguments as any call does. Temporal/value role
    /// selection follows FN-4 ... An operator is no target: a function
    /// wraps it ... Const parameters receive configuration; temporal
    /// parameters receive contextually typed publication traces. A
    /// trace is an argument of that call and of no call inside it. The
    /// eval's configuration, `eval{length: 4s} target(...)`, is the
    /// run's (RUN-6)."
    ///
    /// TST-3: "An eval of a target that has a result is a trace of that
    /// result's type". TST-8: "an outputless target has no recorder or
    /// comparable result. Its eval stands as a statement and has no
    /// value: binding or comparing it rejects." TST-8 too: "The run is
    /// a simulation (RUN-3) under its configuration (RUN-6): the eval's
    /// own entries and, in an integration test that injects
    /// `run_config`, the injected configuration's for the fields the
    /// eval does not name."
    ///
    /// TST-17: in an integration test `run call` is written beside it,
    /// "whose value is the run's output as an eval's is ... `run` in a
    /// test that is no integration test rejects (`run.position`)." It is
    /// read as an eval is, and is one in real time (RUN-3).
    ///
    /// The configuration is read first, as it is written (EXPR-13), and
    /// then the call, as any call of its function is, with a trace
    /// expected of each temporal parameter.
    pub(super) fn eval(&mut self, node: &SyntaxNode) -> (ExprKind, Type) {
        let at = self.at(node);
        let real_time = node.kind() == SyntaxKind::RunExpr;
        if real_time && !self.integration {
            // "the word": the first token of the form.
            let word = match node.first_token() {
                Some(word) => Location::new(self.file, word.span()),
                None => at,
            };
            self.diagnostics.emit(
                error(
                    Category::Phase,
                    codes::RUN_POSITION,
                    word,
                    "`run` is written in a test that is no integration test".to_string(),
                    "RUN-3",
                )
                .with_note(
                    "a test evaluates in simulation, `eval target(...)`; an `integration_test` \
                     may run in real time",
                ),
            );
        }
        let config = match child(node, SyntaxKind::OperatorConfig) {
            Some(braces) => self.run_configuration(&braces),
            None => Vec::new(),
        };
        let inherited = self.injected_run_config();
        // The parser has reported a form with no call.
        let Some(target) = run_target(node) else {
            return (ExprKind::Unread, Type::Unknown);
        };
        let called = self.call_taken(&target);
        let function = called
            .as_ref()
            .and_then(|called| self.evaluated(&called.name));
        let (Some(called), Some((module, declaration))) = (called.clone(), function) else {
            // A call of a name nothing binds is left for the prelude,
            // and is not judged (ERR-9).
            if !self.unbound(called.as_ref()) {
                self.diagnostics.emit(
                    error(
                        Category::Type,
                        codes::EVAL_TARGET,
                        self.at(&target),
                        "an eval takes a call of a module function or a helper of this test"
                            .to_string(),
                        "TST-5",
                    )
                    .with_note("an operator is evaluated through a function that calls it"),
                );
            }
            // What the call was given is read all the same, with
            // nothing expected of it.
            return match &called {
                Some(called) => self.unread_call(&called.arguments),
                None => (ExprKind::Unread, Type::Unknown),
            };
        };
        // FN-15: a function that declares a `var` parameter "is no
        // target of an `eval`": an eval lifts a value function (TST-8),
        // and that one is not lifted.
        if let Some(Detail::Callable(signature)) =
            self.program.modules[module].details.get(declaration)
            && signature.parameters.iter().any(|parameter| parameter.var)
        {
            self.diagnostics.emit(error(
                Category::Type,
                codes::EVAL_TARGET,
                self.at(&target),
                format!(
                    "`{}` has a `var` parameter, and is no target of an eval",
                    called.name.text()
                ),
                "FN-15",
            ));
            // Nothing more is reported of the call (ERR-9).
            return self.unread_call(&called.arguments);
        }
        let name = called.name.clone();
        // TST-5: the call an eval takes is composition's: "Composition
        // and eval choose the temporal definition if one exists" (FN-4).
        let composing = std::mem::replace(&mut self.graph, true);
        self.evaluating = true;
        let (kind, ty) = self.call_function(
            &called.name,
            &called.arguments,
            (module, declaration),
            None,
            called.generics,
        );
        self.evaluating = false;
        self.graph = composing;
        // The call is held to what any call in a graph is: a `const`
        // parameter is given no connection (FN-3).
        self.constants_of(&kind);
        // TST-7: "Within one eval all inputs and expectation use the
        // same dense/timed style." The first trace that has cells says
        // which, and a later one of the other is reported.
        let mut style: Option<TraceStyle> = None;
        if let ExprKind::Call { arguments, .. } = &kind {
            for argument in arguments {
                let Type::Trace {
                    style: Some(given), ..
                } = &argument.value.ty
                else {
                    continue;
                };
                match style {
                    None => style = Some(*given),
                    Some(first) if !first.same_notation(*given) => {
                        self.trace_style(argument.value.at, first, *given);
                    }
                    Some(_) => {}
                }
            }
        }
        let has_result = matches!(
            self.program.modules[module].details.get(declaration),
            Some(Detail::Callable(signature)) if signature.result.is_some()
        );
        let call = Expr {
            kind,
            ty,
            at,
            category: ValueCategory::Unsettled,
        };
        if !has_result {
            // "Its eval stands as a statement and has no value". One in
            // parentheses is the same statement (EXPR-1).
            let mut holder = node.parent();
            while let Some(parent) = holder
                .as_ref()
                .filter(|parent| parent.kind() == SyntaxKind::ParenExpr)
            {
                holder = parent.parent();
            }
            let statement = holder.is_some_and(|parent| parent.kind() == SyntaxKind::ExprStmt);
            if !statement {
                self.diagnostics.emit(
                    error(
                        Category::Type,
                        codes::EVAL_NO_RESULT,
                        at,
                        format!(
                            "`{}` has no result, so this eval has no value to bind or compare",
                            name.text()
                        ),
                        "TST-8",
                    )
                    .with_note("an eval of a target with no result stands as a statement"),
                );
            }
            return (
                ExprKind::Eval {
                    call: Box::new(call),
                    config,
                    inherited,
                    real_time,
                },
                Type::Unknown,
            );
        }
        // "Each cell is one cycle's publication": the `delta<T>` of the
        // result's type, with a tuple's T named. One not known stays so.
        let cell = if open(&call.ty) {
            call.ty.clone()
        } else {
            Type::trace_cell(call.ty.clone())
        };
        let ty = Type::Trace {
            cell: Box::new(cell),
            style,
        };
        (
            ExprKind::Eval {
                call: Box::new(call),
                config,
                inherited,
                real_time,
            },
            ty,
        )
    }

    /// The local an integration test has bound `run_config` to, where it
    /// injects it (TST-17, RUN-6): "In an integration test that injects
    /// `run_config` ... a field the statement does not name is the
    /// injected configuration's." `None` anywhere else.
    fn injected_run_config(&self) -> Option<LocalId> {
        if !self.integration {
            return None;
        }
        self.locals
            .iter()
            .position(|local| local.kind == LocalKind::Inject && local.name == RUN_CONFIG)
    }

    /// The call an eval or a run statement takes, `target` being the
    /// form's call: `name(arguments)`, or `name<generics>(arguments)`.
    /// GEN-14: `name<...>(...)` the parser left undecided is the call
    /// too, where the resolver decided it is one. `None` for anything
    /// else.
    fn call_taken(&self, target: &SyntaxNode) -> Option<Called<'a>> {
        let target_at = self.at(target);
        let generic = match target.kind() {
            SyntaxKind::TypedConstructor => true,
            SyntaxKind::UndecidedConstructor => self
                .module
                .decisions
                .iter()
                .any(|decision| decision.at == target_at && decision.constructor == Some(true)),
            _ => false,
        };
        let arguments = child(target, SyntaxKind::ArgList)?;
        match target.kind() {
            SyntaxKind::CallExpr => {
                let callee = target
                    .children()
                    .next()
                    .filter(|callee| callee.kind() == SyntaxKind::NameExpr)?;
                Some(Called {
                    name: name_token(&callee)?,
                    arguments,
                    generics: None,
                })
            }
            _ if generic => {
                // As `typed_constructor` finds them: the generic
                // arguments are recorded where the whole name starts.
                let name = name_token(target)?;
                let start = match child(target, SyntaxKind::QualifiedName) {
                    Some(qualified) => qualified.span().start,
                    None => name.span().start,
                };
                Some(Called {
                    name,
                    arguments,
                    generics: self.calls.get(&(self.file, start)).copied(),
                })
            }
            _ => None,
        }
    }

    /// The function `name` stands for where it is evaluated or run: a
    /// module function or a helper of the test. FN-4: "Composition and eval
    /// choose the temporal definition if one exists".
    fn evaluated(&self, name: &SyntaxToken) -> Option<(usize, usize)> {
        let (module, declarations) = self.declarations(name)?;
        let kind_of =
            |declaration: usize| self.program.modules[module].declarations[declaration].kind;
        let of_role = |role: Role| {
            declarations.iter().copied().find(|&declaration| {
                matches!(
                    kind_of(declaration),
                    DeclarationKind::Function(found) | DeclarationKind::NativeFunction(found)
                        if found == role
                )
            })
        };
        of_role(Role::Temporal)
            .or_else(|| of_role(Role::Value))
            .map(|declaration| (module, declaration))
    }

    /// True for a call of a name nothing binds, which is left for the
    /// prelude and not judged (ERR-9).
    fn unbound(&self, called: Option<&Called<'a>>) -> bool {
        called.is_some_and(|called| {
            !self
                .bindings
                .contains_key(&(self.file, called.name.span().start))
        })
    }

    /// An application's run statement (RUN-1): "its run statement is
    /// `run call` or `eval call`, where the call is of an entry, written
    /// as any call is and giving the entry's const arguments. The
    /// statement gives no configuration: the run's is supplied to it
    /// (RUN-6)." "An entry is a temporal function with no temporal
    /// parameters." RUN-6: "a configuration on an application's run
    /// statement" rejects (`run.configuration`), at the brace, and what
    /// it holds is not read (ERR-9).
    ///
    /// UNIT-14: "the arguments of the run statement's call are value
    /// code; the call itself, of the entry, is composition (RUN-1)". The
    /// entry is its temporal definition (`evaluated`), and what it is
    /// given is read as value code is: a `const` parameter takes a
    /// value, and a temporal call there is `call.role`.
    pub(super) fn run_statement(mut self, store: Option<&Body>) -> Option<Run> {
        self.graph = false;
        self.value_code = true;
        // RUN-6: the resolver exposes the application's injection to
        // the entry arguments. Give the same declaration its typed local.
        if let Some(configuration) = store.and_then(|body| {
            body.statements
                .iter()
                .find_map(|statement| match statement {
                    Stmt::Inject { local, .. } if body.locals[*local].name == RUN_CONFIG => {
                        Some(&body.locals[*local])
                    }
                    _ => None,
                })
        }) {
            self.declare(
                RUN_CONFIG,
                configuration.at,
                LocalKind::Inject,
                configuration.ty.clone(),
                true,
            );
        }
        let form = self.declaration.syntax.children().next()?;
        let at = self.at(&form);
        let real_time = form.kind() == SyntaxKind::RunExpr;
        if let Some(braces) = child(&form, SyntaxKind::OperatorConfig) {
            let brace = match braces.first_token() {
                Some(brace) => Location::new(self.file, brace.span()),
                None => self.at(&braces),
            };
            self.diagnostics.emit(
                error(
                    Category::Phase,
                    codes::RUN_CONFIGURATION,
                    brace,
                    "an application's run statement gives no configuration: what starts the \
                     application supplies it"
                        .to_string(),
                    "RUN-6",
                )
                .with_note(
                    "`inject run_config{length: 3s}` before the run statement gives the \
                     configuration defaults",
                ),
            );
        }
        // The parser has reported a run statement with no call.
        let target = run_target(&form)?;
        let called = self.call_taken(&target);
        let function = called
            .as_ref()
            .and_then(|called| self.evaluated(&called.name));
        // "An entry is a temporal function with no temporal
        // parameters." A signature not read is not judged.
        let entry = function.filter(|&(module, declaration)| {
            let module = &self.program.modules[module];
            let temporal = module.declarations[declaration].kind.role() == Some(Role::Temporal);
            let inputs = match module.details.get(declaration) {
                Some(Detail::Callable(signature)) => signature
                    .parameters
                    .iter()
                    .any(|parameter| !parameter.constant),
                _ => false,
            };
            temporal && !inputs
        });
        let (kind, ty) = match (&called, entry) {
            (Some(called), Some(entry)) => {
                let read = self.call_function(
                    &called.name,
                    &called.arguments,
                    entry,
                    None,
                    called.generics,
                );
                // The call is held to what any call in a graph is: a
                // `const` parameter is given no connection (FN-3).
                self.constants_of(&read.0);
                read
            }
            _ => {
                if !self.unbound(called.as_ref()) {
                    self.diagnostics.emit(
                        error(
                            Category::Type,
                            codes::RUN_ENTRY,
                            self.at(&target),
                            "a run statement's call is of an entry, and this is none".to_string(),
                            "RUN-1",
                        )
                        .with_note("an entry is a temporal function with no temporal parameters"),
                    );
                }
                // What the call was given is read all the same, with
                // nothing expected of it.
                match &called {
                    Some(called) => self.unread_call(&called.arguments),
                    None => (ExprKind::Unread, Type::Unknown),
                }
            }
        };
        let call = Expr {
            kind,
            ty,
            at: self.at(&target),
            category: ValueCategory::Unsettled,
        };
        Some(Run {
            real_time,
            locals: self.locals,
            call,
            at,
        })
    }

    /// Reports a trace of one style where the eval it belongs to has
    /// one of the other (TST-7).
    fn trace_style(&mut self, at: Location, first: TraceStyle, later: TraceStyle) {
        let name = |style: TraceStyle| match style {
            TraceStyle::Dense => "dense",
            TraceStyle::Timed(_) => "timed",
        };
        self.diagnostics.emit(
            error(
                Category::Type,
                codes::TRACE_STYLE,
                at,
                format!(
                    "this trace is {}, and the eval's other traces are {}: one eval uses one \
                     style, its expectation included",
                    name(later),
                    name(first)
                ),
                "TST-7",
            )
            .with_note("dense is `[7, _, 9]`, and timed `[0s: 7, 2s: 9]`"),
        );
    }

    /// A trace written out where one is expected: `[7, _, 9]`, dense,
    /// or `[0s: 7, 2s: 9]`, timed. `cell` is what a cell is to be.
    ///
    /// TST-5: "Each cell is one cycle's publication". TST-7: "Timed
    /// entries use duration offsets from run start or absolute datetime
    /// keys. Keys strictly increase; no `_` occurs in timed traces."
    pub(super) fn trace(&mut self, items: &[SyntaxNode], cell: &Type) -> (ExprKind, Type) {
        let timed = items
            .iter()
            .any(|item| item.kind() == SyntaxKind::SequencePair);
        let mut cells = Vec::new();
        // The kind of key a timed trace has, and its last constant key.
        let mut key: Option<Scalar> = None;
        let mut last: Option<i64> = None;
        for item in items {
            let (time, value) = if timed {
                // A value among pairs has been reported by the parser.
                let mut parts = item.children();
                let (Some(time), Some(value)) = (parts.next(), parts.next()) else {
                    continue;
                };
                if item.kind() != SyntaxKind::SequencePair {
                    continue;
                }
                (Some(time), value)
            } else {
                (None, item.clone())
            };
            let time = time.and_then(|time| self.trace_time(&time, &mut key, &mut last));
            let value = if value.kind() == SyntaxKind::Hole {
                if timed {
                    self.placeholder(
                        &value,
                        "a timed trace has no `_`: it lists each publication with its time",
                        "TST-7",
                    );
                }
                None
            } else {
                // TST-5: "For a nullable scalar it is the optional that
                // holds what was published: a cell written as a value
                // is that value, and one written `null` a published
                // null". A cell written `null`, in parentheses or not,
                // is that delta. Any other is read as the delta: a
                // value where one is expected (TYPE-16), or the delta
                // itself, in a block or a branch as outside one.
                let optional = matches!(cell, Type::Delta(of)
                    if matches!(**of, Type::Nullable(_)));
                if optional && written_null(&value) {
                    Some(Expr {
                        kind: ExprKind::Literal(Value::Null),
                        ty: cell.clone(),
                        at: self.at(&value),
                        category: ValueCategory::Unsettled,
                    })
                } else if let Type::Delta(of) = cell
                    && matches!(
                        **of,
                        Type::List { .. }
                            | Type::Set(_)
                            | Type::Dict { .. }
                            | Type::Tuple(_)
                            | Type::Named {
                                kind: NamedKind::Struct,
                                ..
                            }
                    )
                {
                    // TST-5: "Each cell is one cycle's publication,
                    // written as a `return` is (TIME-7) ... a T,
                    // published whole, or a `delta<T>` (TYPE-16),
                    // published as that change. A list, a tuple, a
                    // struct and a dictionary are each written either
                    // way". As a `return` is: "a value that is a T is
                    // published whole", and one that is no T and is a
                    // `delta<T>` as that change. So a tuple written
                    // `(2, null)` of a `tuple<i64, i64?>` is the whole
                    // tuple, its null a published null, and `(null, 3)`
                    // of a `tuple<i64, i64>` its delta.
                    Some(self.published(&value, of, "a cell of the trace"))
                } else {
                    let read = self.expression(&value, Some(cell));
                    Some(self.fit(read, cell, "a cell of the trace"))
                }
            };
            cells.push(hir::Cell { time, value });
        }
        // Where the place does not say what a cell is, the cells do:
        // the one type they have, `f64` when some are `i64` and some
        // `f64` (NUM-1).
        let cell = if open(cell) {
            let known: Vec<&Type> = cells
                .iter()
                .filter_map(|cell| cell.value.as_ref())
                .map(|value| &value.ty)
                .filter(|ty| !open(ty))
                .collect();
            let numbers = |ty: &Type| matches!(ty, Type::Scalar(Scalar::I64 | Scalar::F64));
            match known.first() {
                Some(_)
                    if known.iter().all(|ty| numbers(ty))
                        && known.iter().any(|ty| **ty == Type::Scalar(Scalar::F64)) =>
                {
                    Type::Scalar(Scalar::F64)
                }
                Some(&first) => first.clone(),
                None => cell.clone(),
            }
        } else {
            cell.clone()
        };
        let style = if timed {
            Some(TraceStyle::Timed(key.unwrap_or(Scalar::Duration)))
        } else if items.is_empty() {
            None
        } else {
            Some(TraceStyle::Dense)
        };
        let ty = Type::Trace {
            cell: Box::new(cell),
            style,
        };
        (ExprKind::Trace(cells), ty)
    }

    /// The key of a timed trace's entry: "duration offsets from run
    /// start or absolute datetime keys. Keys strictly increase" (TST-7).
    /// `key` is the kind the trace's keys have so far, and `last` its
    /// last constant key.
    fn trace_time(
        &mut self,
        node: &SyntaxNode,
        key: &mut Option<Scalar>,
        last: &mut Option<i64>,
    ) -> Option<Expr> {
        if node.kind() == SyntaxKind::Hole {
            self.placeholder(
                node,
                "a timed trace has no `_`: each entry has the time it is published at",
                "TST-7",
            );
            return None;
        }
        let time = self.expression(node, None);
        match &time.ty {
            Type::Scalar(scalar @ (Scalar::Duration | Scalar::Datetime)) => match key {
                Some(first) if first != scalar => {
                    self.diagnostics.emit(error(
                        Category::Type,
                        codes::TYPE_MISMATCH,
                        time.at,
                        format!(
                            "this key is a `{}`, and the trace's keys before it are `{}`: a \
                             timed trace's keys are all durations or all datetimes",
                            time.ty,
                            Type::Scalar(*first)
                        ),
                        "TST-7",
                    ));
                }
                Some(_) => {}
                None => *key = Some(*scalar),
            },
            ty if !open(ty) => {
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::TYPE_MISMATCH,
                    time.at,
                    format!(
                        "this is `{ty}`, and a timed trace's key is a `duration` from the \
                         run's start or a `datetime`"
                    ),
                    "TST-7",
                ));
            }
            _ => {}
        }
        // "Keys strictly increase": of the keys that are constants read
        // here, durations.
        if let Ok(ConstantValue::Duration(now)) = self.read_constant(node, None) {
            if last.is_some_and(|before| now <= before) {
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::TRACE_KEY_ORDER,
                    time.at,
                    "this key is not after the one before it: a timed trace's keys \
                         strictly increase"
                        .to_string(),
                    "TST-7",
                ));
            }
            *last = Some(now);
        }
        Some(time)
    }

    /// Reports a `_` where none stands (EXPR-8, TST-7).
    pub(super) fn placeholder(&mut self, hole: &SyntaxNode, message: &str, rule: &'static str) {
        self.diagnostics.emit(error(
            Category::Type,
            codes::TRACE_PLACEHOLDER,
            self.at(hole),
            message.to_string(),
            rule,
        ));
    }
}

/// The call an `eval` or a `run` takes, `form` being the `EvalExpr` or
/// the `RunExpr`: its one node but the configuration (`run_target` in
/// the grammar). `None` where the parser found none.
fn run_target(form: &SyntaxNode) -> Option<SyntaxNode> {
    form.children()
        .find(|part| part.kind() != SyntaxKind::OperatorConfig)
}

/// TST-16: "A trace given to a `const fn` is an ordinary value there:
/// for a result type T a `list<tuple<datetime, delta<T>>>`, one tuple
/// for each publication, in time order."
///
/// A value that is a trace becomes that list. A trace's cell is the
/// `delta<T>` of what it is of (TST-5), a tuple's named by its T
/// (`Type::trace_cell`), whose delta is formed here. Any other value, one
/// that holds a trace among them, is as it was.
pub(super) fn publications(value: hir::Expr) -> hir::Expr {
    let Type::Trace { cell, .. } = &value.ty else {
        return value;
    };
    let ty = Type::List {
        element: Box::new(Type::Tuple(vec![
            Type::Scalar(Scalar::Datetime),
            Type::cell_delta(cell),
        ])),
        size: None,
    };
    let at = value.at;
    hir::Expr {
        kind: ExprKind::Publications(Box::new(value)),
        ty,
        at,
        category: ValueCategory::Unsettled,
    }
}

/// True when `ty` is a trace or holds one at any depth: the
/// `list<trace of i64>` of `[seen]`.
pub(super) fn holds_trace(ty: &Type) -> bool {
    ty.any_part(&|part| matches!(part, Type::Trace { .. }), &|_| false)
}
