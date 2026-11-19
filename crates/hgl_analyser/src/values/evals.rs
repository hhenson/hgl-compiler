//! What a test and an application run: a named test's block, its
//! `assert` and `eval`, and the traces it compares; an application's run
//! statement, and the statements that set its run's store (TST-3 to
//! TST-17, RUN-1, UNIT-14). Part of the typing pass ([`super`]).

use hgl_diagnostics::{Category, Location};
use hgl_lexer::literal::decode_string;
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};

use super::calls::arguments_of;
use super::{BOOL, Called, Typer, open, written_null};
use crate::hir::{
    self, BinaryOperator, Body, Expr, ExprKind, Hook, Run, Stmt, Value, ValueCategory,
};
use crate::program::{DeclarationKind, LocalKind, Role, StoreStatement};
use crate::syntax::{child, name_token, tokens, ungrouped};
use crate::types::{ConstantValue, Detail, NamedKind, Scalar, TraceStyle, Type};
use crate::{codes, error};

impl<'a> Typer<'a> {
    /// The block of a named test (TST-3): "A test is a
    /// composition-phase block with assertions." It is read as a
    /// graph's body is, with `assert` among its statements.
    pub(super) fn test(mut self, integration: bool) -> Option<Body> {
        let block = child(&self.declaration.syntax, SyntaxKind::Block)?;
        self.graph = true;
        self.testing = true;
        self.integration = integration;
        let statements = block
            .children()
            .map(|statement| self.statement(&statement))
            .collect();
        Some(Body {
            locals: self.locals,
            statements,
        })
    }

    /// The statements an application sets its run's store with
    /// (UNIT-14): "`inject global_state`, and then calls of `put` on
    /// it. Each is evaluated once, in the order written, before the
    /// run's graph is built, and its value is read as a constant's is
    /// (UNIT-13), with no parameter in scope. Any other statement
    /// there rejects (`application.statement`)."
    ///
    /// They are read as the statements of a node's `start` are, which
    /// is where a node uses a capability before its graph runs: with
    /// no parameter and no input, each is value code.
    pub(super) fn store(mut self, written: &[StoreStatement]) -> Option<Body> {
        self.hook = Some(Hook::Start);
        let mut statements = Vec::new();
        for store in written {
            let Some(node) = store.syntax.children().next() else {
                continue;
            };
            if node.kind() == SyntaxKind::InjectDecl {
                // "An application injects `global_state` and no other"
                // (FN-9).
                for name in tokens(&node).filter(|token| token.kind() == SyntaxKind::Ident) {
                    if name.text() != "global_state" {
                        self.diagnostics.emit(error(
                            Category::Phase,
                            codes::APPLICATION_STATEMENT,
                            Location::new(self.file, name.span()),
                            format!(
                                "an application injects `global_state` alone, to set its \
                                 run's store, and not `{}`",
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
                // constant's.
                for at in self.reads_of_the_store(&statement) {
                    self.diagnostics.emit(
                        error(
                            Category::Phase,
                            codes::APPLICATION_STATEMENT,
                            at,
                            "what a store statement puts is read as a constant's value is, \
                             and reads no capability: the store is `put`'s own argument"
                                .to_string(),
                            "UNIT-14",
                        )
                        .with_note(
                            "a constant, or a call of a `const fn` on constants, is such a value",
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
    /// given beside the store itself: each use of the injected
    /// capability inside its key or its value.
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

    /// `eval(target(...))` in a named test.
    ///
    /// TST-5: "Eval takes a call of a module function, written as any
    /// call is: `eval(target(value: [7, _, 9]))`. The call binds its
    /// arguments by the function's public positional/named rules, and
    /// its generic arguments as any call does. Temporal/value role
    /// selection follows FN-4 ... An operator is no target: a function
    /// wraps it ... Const parameters receive configuration; temporal
    /// parameters receive contextually typed publication traces. A
    /// trace is an argument of that call and of no call inside it. A
    /// second argument is the run's configuration, a `RunConfig`".
    ///
    /// TST-3: "An eval of a target that has a result is a trace of that
    /// result's type". TST-8: "an outputless target has no recorder or
    /// comparable result. Its eval stands as a statement and has no
    /// value: binding or comparing it rejects."
    ///
    /// TST-17: in an integration test `run(call, config)` is written
    /// beside it, "whose value is the run's output as an eval's is ...
    /// in a test that is no integration test it rejects." It is read
    /// as an eval is, and is one in real time (RUN-3).
    ///
    /// The call is read as any call of its function is, with a trace
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
                    "a test evaluates in simulation, `eval(...)`; an `integration_test` may \
                     run in real time",
                ),
            );
        }
        let Some(list) = child(node, SyntaxKind::ArgList) else {
            return (ExprKind::Unread, Type::Unknown);
        };
        let given = arguments_of(&list);
        // The parser has reported an eval given nothing.
        let Some((_, argument)) = given.first().cloned() else {
            return (ExprKind::Unread, Type::Unknown);
        };
        // EXPR-1: "parentheses group", so `(call)` is the call.
        let target = ungrouped(argument.clone());
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
                        self.at(&argument),
                        "an eval's first argument is a call of a function of a module, and this \
                         is none"
                            .to_string(),
                        "TST-5",
                    )
                    .with_note("an operator is evaluated through a function that calls it"),
                );
            }
            // What the call was given is read all the same, with
            // nothing expected of it, and then what follows the call.
            let unread = match &called {
                Some(called) => self.unread_call(&called.arguments),
                None => (ExprKind::Unread, Type::Unknown),
            };
            self.after_call(&given, "an eval", "TST-5");
            return unread;
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
                self.at(&argument),
                format!(
                    "`{}` has a `var` parameter, and is no target of an eval",
                    called.name.text()
                ),
                "FN-15",
            ));
            // Nothing more is reported of the call (ERR-9).
            let unread = self.unread_call(&called.arguments);
            self.after_call(&given, "an eval", "TST-5");
            return unread;
        }
        let name = called.name.clone();
        self.evaluating = true;
        let (kind, ty) = self.call_function(
            &called.name,
            &called.arguments,
            (module, declaration),
            None,
            called.generics,
        );
        self.evaluating = false;
        // The call is held to what any call in a graph is: a `const`
        // parameter is given no connection (FN-3).
        self.constants_of(&kind);
        // EXPR-13: arguments are read in the order written, so the
        // call's before the configuration.
        let config = self.after_call(&given, "an eval", "TST-5");
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
        let call = Expr { kind, ty, at };
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
                    real_time,
                },
                Type::Unknown,
            );
        }
        // "Each cell is one cycle's publication": the `delta<T>` of the
        // result's type. One not known stays so.
        let cell = if open(&call.ty) {
            call.ty.clone()
        } else {
            Type::delta(call.ty.clone())
        };
        let ty = Type::Trace {
            cell: Box::new(cell),
            style,
        };
        (
            ExprKind::Eval {
                call: Box::new(call),
                config,
                real_time,
            },
            ty,
        )
    }

    /// What an eval or a run statement, `what`, is given after its
    /// call, `given` being all it is given: the run's configuration,
    /// and nothing more. "A second argument is the run's configuration"
    /// (TST-5, RUN-1); a third is reported under `rule` and read with
    /// nothing expected of it.
    fn after_call(
        &mut self,
        given: &[(Option<SyntaxToken>, SyntaxNode)],
        what: &str,
        rule: &'static str,
    ) -> Option<Box<Expr>> {
        let config = given.get(1).map(|(_, written)| self.run_config(written));
        for (_, extra) in given.iter().skip(2) {
            self.diagnostics.emit(error(
                Category::Type,
                codes::ARGUMENT_UNEXPECTED,
                self.at(extra),
                format!("{what} takes a call and a run configuration, and nothing more"),
                rule,
            ));
            self.expression(extra, None);
        }
        config
    }

    /// The call an eval or a run statement takes, `target` being its
    /// first argument: `name(arguments)`, or `name<generics>(arguments)`.
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
    /// function of a module, native or not. FN-4: "Composition and eval
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
    /// `run(call, config)` or `eval(call, config)`, where the call is
    /// of an entry, written as any call is and giving the entry's const
    /// arguments, and the configuration is a value of core's
    /// `RunConfig`, or `RunConfig()` where none is written." "An entry
    /// is a temporal function with no temporal parameters."
    ///
    /// The call is read as a graph's call of the entry is: a `const`
    /// parameter takes a constant.
    pub(super) fn run_statement(mut self) -> Option<Run> {
        self.graph = true;
        let form = self.declaration.syntax.children().next()?;
        let at = self.at(&form);
        let real_time = form.kind() == SyntaxKind::RunExpr;
        let list = child(&form, SyntaxKind::ArgList)?;
        let given = arguments_of(&list);
        // The parser has reported a run statement given nothing.
        let (_, argument) = given.first().cloned()?;
        // EXPR-1: "parentheses group", so `(call)` is the call.
        let target = ungrouped(argument.clone());
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
                            self.at(&argument),
                            "a run statement's first argument is a call of an entry, and this \
                             is none"
                                .to_string(),
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
        };
        // EXPR-13: the call is read before the configuration.
        let config = self
            .after_call(&given, "a run statement", "RUN-1")
            .map(|config| *config);
        Some(Run {
            real_time,
            call,
            config,
            at,
        })
    }

    /// The configuration of a run, written as the second argument of
    /// an eval: a value of core's `RunConfig` (RUN-1, core's CRUN-2).
    fn run_config(&mut self, written: &SyntaxNode) -> Box<Expr> {
        let Some(expected) = self.program.run_config_type() else {
            return Box::new(self.expression(written, None));
        };
        let value = self.expression(written, Some(&expected));
        Box::new(self.fit(value, &expected, "the configuration of a run"))
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
                    })
                } else if let Type::Delta(of) = cell
                    && matches!(
                        **of,
                        Type::List { .. }
                            | Type::Set(_)
                            | Type::Dict { .. }
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
                    // way". A cell's type says which it is. A tuple's
                    // delta is a tuple of its positions, and a whole
                    // tuple reads as that.
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

/// TST-16: "A trace given to a `const fn` is an ordinary value there:
/// for a result type T a `list<tuple<datetime, delta<T>>>`, one tuple
/// for each publication, in time order."
///
/// A value that is a trace becomes that list. A trace's cell is already
/// the `delta<T>` of what it is of (TST-5). Any other value, one that
/// holds a trace among them, is as it was.
pub(super) fn publications(value: hir::Expr) -> hir::Expr {
    let Type::Trace { cell, .. } = &value.ty else {
        return value;
    };
    let ty = Type::List {
        element: Box::new(Type::Tuple(vec![
            Type::Scalar(Scalar::Datetime),
            (**cell).clone(),
        ])),
        size: None,
    };
    let at = value.at;
    hir::Expr {
        kind: ExprKind::Publications(Box::new(value)),
        ty,
        at,
    }
}

/// True when `ty` is a trace or holds one at any depth: the
/// `list<trace of i64>` of `[seen]`.
pub(super) fn holds_trace(ty: &Type) -> bool {
    ty.any_part(&|part| matches!(part, Type::Trace { .. }), &|_| false)
}
