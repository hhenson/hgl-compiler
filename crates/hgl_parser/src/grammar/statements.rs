//! Blocks and statements: the expressions-and-bindings,
//! control-flow-and-sources, temporal-model and tests-and-eval chapters.

use crate::grammar::can_start_expression;
use crate::kind::SyntaxKind::{self, *};
use crate::parser::{Bracket, Choice, Context, Parser, Verdict};

/// True for `=` and the compound assignment operators.
fn is_assignment_operator(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        Eq | PlusEq | MinusEq | StarEq | SlashEq | SlashSlashEq | PercentEq
    )
}

impl Parser<'_> {
    // ---- Blocks ----

    /// `block = "{", [ NL ], [ statement, { NL, statement }, [ NL ] ], "}" ;`
    ///
    /// The cursor is on the `{`.
    pub(crate) fn block(&mut self) {
        if !self.enter() {
            return;
        }
        self.start(Block);
        self.bump();
        // LEX-4: braces delimit a block; indentation means nothing.
        // LEX-6: one statement to a line.
        self.nested(Context::BLOCK, |parser| {
            parser.lines(
                "a statement",
                "LEX-6",
                |_| false,
                |parser| parser.statement(),
            );
            parser.close_brace();
        });
        self.finish();
        self.leave();
    }

    /// A block that is the body of the declaration or statement before it.
    pub(crate) fn body_block(&mut self) {
        if self.at(LBrace) {
            self.header_brace();
            self.block();
        } else {
            self.expected("`{`");
        }
    }

    /// Reports the `{` under the cursor when it is not on the line of the
    /// header it belongs to. LEX-5: "A body's opening brace stays with its
    /// header."
    pub(crate) fn header_brace(&mut self) {
        if self.broken() {
            self.error(
                "a body's opening brace stays on its header's line",
                "LEX-5",
                None,
            );
        }
    }

    // ---- Statements ----

    /// ```text
    /// statement = local_decl | assignment | return_statement | expression
    ///           | state_decl | cache_decl | inject_decl | lifecycle_block
    ///           | when_statement | for_statement | while_statement
    ///           | yield_statement | switch_statement | assert_statement ;
    /// ```
    ///
    /// Parses nothing when no statement starts at the cursor.
    pub(crate) fn statement(&mut self) {
        match self.current() {
            KwLet | KwVar => self.local_decl(),
            KwReturn => {
                // `return_statement = "return", [ expression ] ;`
                self.start(ReturnStmt);
                self.bump();
                if !(self.nl() || matches!(self.current(), RBrace | Eof | Semicolon)) {
                    self.expression();
                }
                self.finish();
            }
            KwState => self.stored_decl(StateDecl),
            KwCache => self.stored_decl(CacheDecl),
            KwInject => self.inject_decl(),
            KwStart | KwStop => {
                // `lifecycle_block = ( "start" | "stop" ), block ;`
                self.start(LifecycleBlock);
                self.bump();
                self.body_block();
                self.finish();
            }
            KwWhen => self.conditional(WhenStmt),
            KwWhile => self.conditional(WhileStmt),
            KwFor => self.for_stmt(),
            KwYield => {
                // `yield_statement = "yield", expression, ":", expression ;`
                self.start(YieldStmt);
                self.bump();
                if self.goes_on("an expression") {
                    self.expression();
                }
                if self.eat(Colon) {
                    self.expression();
                } else {
                    self.expected("`:`");
                }
                self.finish();
            }
            KwAssert => self.assert_stmt(),
            Ident => self.name_statement(),
            // `.member` begins an expression and no statement.
            kind if can_start_expression(kind) && kind != Dot => self.expr_stmt(),
            _ => {}
        }
    }

    /// ```text
    /// local_decl = ( "let" | "var" ), IDENTIFIER, [ ":", type ], "=", expression
    ///            | "var", IDENTIFIER, ":", type ;
    /// ```
    fn local_decl(&mut self) {
        let variable = self.at(KwVar);
        self.start(LocalDecl);
        self.bump();
        self.expect_name("a name");
        let typed = self.eat(Colon);
        if typed {
            self.type_();
        }
        if self.at_assign() && !self.nl() {
            self.eat_assign();
            self.expression();
        } else if !(variable && typed) {
            self.expected("`=`");
        }
        self.finish();
    }

    /// ```text
    /// state_decl = "state", IDENTIFIER, [ ":", value_type ], "=", expression ;
    /// cache_decl = "cache", IDENTIFIER, [ ":", value_type ], "=", expression ;
    /// ```
    fn stored_decl(&mut self, kind: SyntaxKind) {
        self.start(kind);
        self.bump();
        self.expect_name("a name");
        if self.eat(Colon) {
            self.type_();
        }
        if self.at_assign() && !self.nl() {
            self.eat_assign();
            self.expression();
        } else {
            self.expected("`=`");
        }
        self.finish();
    }

    /// `inject_decl = "inject", IDENTIFIER, { ",", IDENTIFIER }, [ "," ] ;`
    pub(crate) fn inject_decl(&mut self) {
        self.start(InjectDecl);
        self.bump();
        self.expect_name("a name");
        while self.eat(Comma) {
            // LEX-6: "An inject list may continue after a comma when the
            // next line begins with a name."
            if self.at(Ident) && !self.cut() {
                self.bump();
            } else {
                break;
            }
        }
        self.finish();
    }

    /// ```text
    /// when_statement = "when", [ expression ], block ;
    /// while_statement = "while", [ expression ], block ;
    /// ```
    ///
    /// CTRL-9: after `when` or `while`, the first `{` outside parentheses,
    /// brackets and operator configuration opens the body, and what
    /// stands before it is the condition. So `when { ready } { step() }`
    /// has the body `{ ready }`, and a second statement on its line.
    fn conditional(&mut self, kind: SyntaxKind) {
        self.start(kind);
        self.bump();
        if !self.at(LBrace) && self.goes_on("a condition or `{`") {
            self.subject();
        }
        self.body_block();
        self.finish();
    }

    /// The expression between `if`, `for ... in`, `switch`, `when` or
    /// `while` and the `{` of its body.
    ///
    /// CTRL-9: "the first `{` outside parentheses, brackets, and operator
    /// configuration (EXPR-10) opens the body ... An expression there
    /// that contains any other brace is parenthesized." So a `{` that
    /// would begin a block is not read as one here: it is left for the
    /// body. `if ({ ready }) { step() }` has a block for its condition;
    /// `if { ready } { step() }` has no condition.
    pub(crate) fn subject(&mut self) {
        let context = Context {
            no_brace: true,
            ..self.context
        };
        self.nested(context, |parser| parser.expression());
    }

    /// ```text
    /// for_statement = "for", iteration_pattern, "in", expression, block ;
    /// iteration_pattern = IDENTIFIER, [ ",", IDENTIFIER ] ;
    /// ```
    fn for_stmt(&mut self) {
        self.start(ForStmt);
        self.bump();
        self.expect_name("a name");
        if self.eat(Comma) {
            self.expect_name("a name");
        }
        // LEX-10: `in` is a name with a meaning only here.
        if self.expect_word("in") && self.goes_on("an expression") {
            // CTRL-9: the first `{` opens the body.
            self.subject();
        }
        self.body_block();
        self.finish();
    }

    /// ```text
    /// assert_statement = "assert", ( expression | raises_assertion ) ;
    /// raises_assertion = "raises", "(", expression, ")", block ;
    /// ```
    fn assert_stmt(&mut self) {
        self.start(AssertStmt);
        self.bump();
        if !self.goes_on("an expression") {
            self.finish();
            return;
        }
        // LEX-10: `raises` is a name with a meaning only here, and only
        // in this form. `assert raises(x) == 1` calls a function.
        let raises = self.at_word("raises")
            && self.choose(Choice::Raises, |parser| {
                let errors = parser.errors;
                parser.bump();
                if !parser.eat(LParen) {
                    return Verdict::No;
                }
                parser.nested(Context::LIST, |parser| {
                    parser.expression();
                    parser.close(RParen);
                });
                if parser.errors == errors && parser.at(LBrace) && !parser.nl() {
                    Verdict::Yes
                } else {
                    Verdict::No
                }
            }) == Verdict::Yes;
        if raises {
            self.start(RaisesAssertion);
            self.bump();
            // LEX-27: `raises(...)` is a call.
            self.attached(Bracket::Call);
            self.bump();
            self.nested(Context::LIST, |parser| {
                parser.expression();
                parser.close(RParen);
            });
            self.body_block();
            self.finish();
        } else {
            self.expression();
        }
        self.finish();
    }

    /// A statement that starts with a name: a `switch`, an assignment, or
    /// an expression.
    fn name_statement(&mut self) {
        // LEX-10: `switch` is a name with a meaning only at the start of
        // a statement of this form. `switch = 1` assigns to a variable.
        if self.at_word("switch") {
            let switch = self.choose(Choice::Switch, |parser| {
                let errors = parser.errors;
                parser.bump();
                if parser.nl() {
                    return Verdict::No;
                }
                parser.expression();
                if parser.errors == errors && parser.at(LBrace) && !parser.nl() {
                    Verdict::Yes
                } else {
                    Verdict::No
                }
            });
            if switch == Verdict::Yes {
                self.switch_stmt();
                return;
            }
        }
        // An assignment and an expression begin alike, `a.b[0] = 1` and
        // `a.b[0](1)`, so the place is read and taken back to see whether
        // an assignment operator follows it.
        let assignment = self.choose(Choice::Assignment, |parser| {
            let errors = parser.errors;
            parser.place();
            let assigns = is_assignment_operator(parser.current()) && !parser.nl();
            if parser.errors == errors && assigns {
                Verdict::Yes
            } else {
                Verdict::No
            }
        });
        if assignment == Verdict::Yes {
            // `assignment = place, assignment_operator, expression ;`
            self.start(AssignStmt);
            self.place();
            self.bump();
            self.expression();
            self.finish();
        } else {
            self.expr_stmt();
        }
    }

    /// `place = IDENTIFIER, { index_suffix | member_suffix } ;`
    ///
    /// Built from the same nodes as the expression it also is.
    fn place(&mut self) {
        let mark = self.mark();
        self.leaf(NameExpr);
        loop {
            match self.current() {
                // LEX-27: as in an expression.
                LBracket if self.suffix_at(0) => {
                    self.attached(Bracket::Index);
                    self.index_suffix();
                    self.wrap(mark, IndexExpr);
                }
                Dot if !self.nl() => {
                    self.member_suffix();
                    self.wrap(mark, MemberExpr);
                }
                _ => break,
            }
        }
    }

    /// An expression as a statement.
    fn expr_stmt(&mut self) {
        self.start(ExprStmt);
        self.expression();
        self.finish();
    }

    // ---- Switch ----

    /// ```text
    /// switch_statement = "switch", expression,
    ///     "{", [ NL ], { switch_case, [ NL ] }, [ switch_default, [ NL ] ], "}" ;
    /// switch_case = "case", const_expression, ":", [ NL ], [ statement, { NL, statement } ] ;
    /// switch_default = "default", ":", [ NL ], [ statement, { NL, statement } ] ;
    /// ```
    fn switch_stmt(&mut self) {
        self.start(SwitchStmt);
        self.bump();
        // CTRL-9: the first `{` opens the cases.
        self.subject();
        // A switch body holds statements, switches among them, without
        // going through `block`; `enter` bounds that nesting here.
        if !self.at(LBrace) || !self.enter() {
            self.expected("`{`");
            self.finish();
            return;
        }
        self.header_brace();
        self.bump();
        self.nested(Context::BLOCK, |parser| {
            let mut default_seen = false;
            loop {
                if parser.at(RBrace) || parser.at(Eof) {
                    break;
                }
                if !parser.at_switch_label() {
                    parser.begin_entry();
                    parser.expected("`case` or `default`");
                    parser.skip_line(true);
                    continue;
                }
                if default_seen {
                    // The grammar puts `default` last.
                    parser.begin_entry();
                    parser.expected("`}`");
                }
                if parser.at_word("case") {
                    parser.start(SwitchCase);
                    parser.bump();
                    if parser.goes_on("a value") {
                        parser.expression();
                    }
                } else {
                    default_seen = true;
                    parser.start(SwitchDefault);
                    parser.bump();
                }
                if !parser.eat(Colon) {
                    parser.expected("`:`");
                }
                // A label's statements run to the next label or the `}`.
                // A label needs no line of its own.
                parser.lines(
                    "a statement",
                    "LEX-6",
                    |parser| parser.at_switch_label(),
                    |parser| parser.statement(),
                );
                parser.finish();
            }
            parser.close_brace();
        });
        self.leave();
        self.finish();
    }

    /// True at `case`, or at `default` followed by `:`.
    ///
    /// LEX-10: both are names with a meaning only where a statement of a
    /// switch body starts.
    fn at_switch_label(&self) -> bool {
        self.at_word("case") || (self.at_word("default") && self.nth(1) == Colon)
    }
}
