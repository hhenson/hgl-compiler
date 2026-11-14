//! Expressions: the expressions-and-bindings chapter.

use crate::grammar::can_start_expression;
use crate::kind::SyntaxKind::{self, *};
use crate::parser::{Choice, Context, Parser, Verdict};

/// The precedence tier of `+` and `-`: where the grammar's `additive_expr`
/// starts. A list size and a constraint operand are `additive_expr`.
pub(crate) const ADDITIVE: u8 = 5;

/// The precedence tier of a binary operator, 1 binding loosest; `None` for
/// a token that is not one.
///
/// EXPR-1 gives the order: `||`, `&&`, equality, comparison, additive,
/// multiplicative. EXPR-11 puts `<=>` with the comparisons.
fn binary_tier(kind: SyntaxKind) -> Option<u8> {
    Some(match kind {
        PipePipe => 1,
        AmpAmp => 2,
        EqEq | BangEq => 3,
        Lt | LtEq | Gt | GtEq | Spaceship => 4,
        Plus | Minus => ADDITIVE,
        Star | Slash | SlashSlash | Percent => 6,
        _ => return None,
    })
}

/// What an argument list belongs to, which decides the forms its
/// arguments may take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArgMode {
    /// A call or a constructor: positional and named arguments.
    Call,
    /// `eval(...)`: the first argument is the expression under test.
    Eval,
    /// A delta constructor: every argument is named.
    Delta,
}

impl Parser<'_> {
    /// `expression = or_expr ;`
    ///
    /// Parses nothing, with a report, when no expression starts at the
    /// cursor.
    pub(crate) fn expression(&mut self) {
        self.binary_expr(1);
    }

    /// The six binary levels of the grammar, `or_expr` down to
    /// `multiplicative_expr`, in one function:
    ///
    /// ```text
    /// or_expr = and_expr, { "||", [ operator_configuration ], and_expr } ;
    /// ...
    /// multiplicative_expr = unary_expr,
    ///     { ( "*" | "/" | "//" | "%" ), [ operator_configuration ], unary_expr } ;
    /// ```
    ///
    /// The levels differ only in their operators, so the operators carry a
    /// tier number and this function parses "an expression of operators at
    /// tier `lowest` or tighter". Its right operand is the same thing one
    /// tier up, which makes each operator left-associative (EXPR-1).
    pub(crate) fn binary_expr(&mut self, lowest: u8) {
        if !self.enter() {
            return;
        }
        let mark = self.mark();
        let mut bare_fn = self.unary_expr();
        loop {
            let operator = self.current();
            // LEX-5, LEX-14: no line-break check. A binary operator at the
            // start of a line continues the line before.
            let Some(tier) = binary_tier(operator) else {
                break;
            };
            // GEN-14: inside generic arguments `>` closes the list.
            if tier < lowest || (operator == Gt && self.context.no_gt) {
                break;
            }
            if bare_fn {
                self.fn_needs_parentheses();
                bare_fn = false;
            }
            self.bump();
            self.continue_line();
            self.operator_config();
            // A configuration is part of its operator (LEX-14), so the
            // operand may stand on the line after it as well.
            self.continue_line();
            self.binary_expr(tier + 1);
            // Wrapping from the same mark each time nests to the left:
            // `a - b - c` is `(a - b) - c`.
            self.wrap(mark, BinaryExpr);
        }
        self.leave();
    }

    /// `unary_expr = ( "-" | "!" ), [ operator_configuration ], unary_expr | postfix_expr ;`
    ///
    /// True when what was parsed is an anonymous function with a block
    /// body and nothing applied to it (see `fn_needs_parentheses`).
    fn unary_expr(&mut self) -> bool {
        if !matches!(self.current(), Minus | Bang) {
            return self.postfix_expr();
        }
        if !self.enter() {
            return false;
        }
        self.start(UnaryExpr);
        self.bump();
        // LEX-5: `!` does not continue a line; a prefix `-` is no binary
        // operator and does not either. So its configuration and its
        // operand both stand on its line.
        if self.on_line("an expression") {
            self.operator_config();
            if self.on_line("an expression") {
                self.unary_expr();
            }
        }
        self.finish();
        self.leave();
        false
    }

    /// ```text
    /// operator_configuration = "{", configuration_argument, { ",", configuration_argument }, [ "," ], "}" ;
    /// configuration_argument = member_name, ":", expression ;
    /// ```
    ///
    /// EXPR-10: a brace of named entries directly after an operator is its
    /// configuration, not a block that is the operand. The cursor is just
    /// after the operator; nothing is parsed when no configuration is
    /// there.
    fn operator_config(&mut self) {
        if !self.at(LBrace) {
            return;
        }
        let named = self.nth_member_name(1) && self.nth(2) == Colon;
        // `a /{} b`: braces with nothing in them and an operand after
        // them. An empty block as the operand, `a / {}`, has none.
        let empty = self.nth(1) == RBrace && can_start_expression(self.nth(2)) && !self.nl_at(2);
        if !(named || empty) {
            return;
        }
        self.start(OperatorConfig);
        if empty {
            self.error(
                "an operator configuration has at least one entry",
                "EXPR-10",
                None,
            );
        }
        self.bump();
        // LEX-5: line breaks do not count inside a configuration.
        self.nested(Context::LIST, |parser| {
            parser.list(RBrace, "a configuration entry", true, |parser| {
                if !(parser.at_member_name() && parser.nth(1) == Colon) {
                    return;
                }
                parser.start(ConfigArg);
                parser.bump();
                parser.bump();
                parser.expression();
                parser.finish();
            });
        });
        self.finish();
    }

    /// ```text
    /// postfix_expr = primary_expr, { call_suffix | index_suffix | member_suffix | "?" } ;
    /// call_suffix = "(", [ arguments ], ")" ;
    /// index_suffix = "[", expression, "]" ;
    /// member_suffix = ".", member_name ;
    /// ```
    ///
    /// Answers as `unary_expr` does.
    fn postfix_expr(&mut self) -> bool {
        let mark = self.mark();
        let start = self.position;
        let mut bare_fn = self.primary_expr();
        if self.position == start {
            // No operand, so nothing for a suffix to apply to.
            return false;
        }
        loop {
            // LEX-5: a suffix does not reach across a line break. `(` at
            // the start of a line begins a new statement.
            if self.nl() {
                break;
            }
            let kind = match self.current() {
                LParen => CallExpr,
                LBracket => IndexExpr,
                Dot => MemberExpr,
                // TYPE-15: `?` after an expression propagates a result;
                // after a type it makes the type nullable.
                Question => TryExpr,
                _ => break,
            };
            if bare_fn {
                self.fn_needs_parentheses();
                bare_fn = false;
            }
            match kind {
                CallExpr => self.arg_list(ArgMode::Call),
                IndexExpr => self.index_suffix(),
                MemberExpr => self.member_suffix(),
                _ => self.bump(),
            }
            self.wrap(mark, kind);
        }
        bare_fn
    }

    /// `index_suffix = "[", expression, "]" ;` with the cursor on the `[`.
    pub(crate) fn index_suffix(&mut self) {
        self.bump();
        self.nested(Context::PLAIN, |parser| {
            parser.expression();
            parser.close(RBracket);
        });
    }

    /// `member_suffix = ".", member_name ;` with the cursor on the `.`.
    pub(crate) fn member_suffix(&mut self) {
        self.bump();
        // LEX-9: `time` may name a member.
        self.expect_member_name();
    }

    /// FN-10: "Parenthesize an anonymous function before calling it or
    /// applying an operation to the function itself." With an arrow body
    /// the question does not arise, since the body takes in whatever
    /// follows. After a block body it does: `fn(x) { ... }(7)`.
    fn fn_needs_parentheses(&mut self) {
        self.error(
            "an anonymous function is parenthesized before it is called or operated on",
            "FN-10",
            None,
        );
    }

    /// ```text
    /// primary_expr = literal | IDENTIFIER | qualified_name | "(", expression, ")"
    ///              | tuple_literal | sequence_literal | typed_constructor
    ///              | delta_constructor | type_call | function_expr
    ///              | if_expression | eval_expression | block ;
    /// literal = INTEGER_LITERAL | FLOAT_LITERAL | STRING_LITERAL | TEMPORAL_LITERAL
    ///         | "true" | "false" | "null" ;
    /// ```
    ///
    /// Answers as `unary_expr` does.
    fn primary_expr(&mut self) -> bool {
        match self.current() {
            // A duration is one of the forms of `TEMPORAL_LITERAL`.
            Int | Float | Str | Duration | Temporal | KwTrue | KwFalse | KwNull => {
                self.leaf(Literal);
            }
            Ident => self.name_expr(),
            // LEX-10: `str(value)` is a conversion, though `str` is
            // reserved where a name is expected.
            KwStr | KwBytes | KwAny if self.nth(1) == LParen && !self.nl_at(1) => {
                self.start(TypeCall);
                self.bump();
                self.arg_list(ArgMode::Call);
                self.finish();
            }
            LParen => self.paren_expr(),
            LBracket => self.sequence_expr(),
            KwFn => return self.function_expr(),
            KwIf => self.if_expr(),
            KwEval => {
                self.start(EvalExpr);
                self.bump();
                if self.at(LParen) && !self.nl() {
                    self.arg_list(ArgMode::Eval);
                } else {
                    self.expected("`(`");
                }
                self.finish();
            }
            // CTRL-9: in the condition of a `when` or `while`, the first
            // `{` opens the body. It is left for the statement to take.
            LBrace if self.context.no_brace => self.expected_with(
                "an expression",
                Some("CTRL-9"),
                Some("a condition that contains a brace is parenthesized"),
            ),
            LBrace => self.block(),
            _ => self.expected("an expression"),
        }
        false
    }

    // ---- Names and constructors ----

    /// `IDENTIFIER | qualified_name`, where
    /// `qualified_name = IDENTIFIER, "::", import_name ;`
    ///
    /// The cursor is on the identifier. A plain name is left as a bare
    /// token; a qualified one gets a node.
    pub(crate) fn name(&mut self) {
        if self.nth(1) == ColonColon && !self.nl_at(1) {
            self.start(QualifiedName);
            self.bump();
            self.bump();
            self.expect_name("a name");
            self.finish();
        } else {
            self.bump();
        }
    }

    /// A name in an expression, and what `<` after it begins:
    ///
    /// ```text
    /// typed_constructor = ( IDENTIFIER | qualified_name ), generic_arguments, "(", [ arguments ], ")" ;
    /// delta_constructor = "delta", "<", type, ">", "(", [ delta_arguments ], ")" ;
    /// ```
    fn name_expr(&mut self) {
        // Where the name ends: after one token, or three when qualified.
        let qualified = self.nth(1) == ColonColon && self.nth(2) == Ident && !self.nl_at(1);
        let after = if qualified { 3 } else { 1 };
        if self.nth(after) == Lt && !self.nl_at(after) && self.no_generic_at != Some(self.position)
        {
            let verdict = self.choose(Choice::Constructor, |parser| parser.probe_constructor());
            if verdict != Verdict::No {
                self.constructor(verdict);
                return;
            }
        }
        self.start(NameExpr);
        self.name();
        self.finish();
    }

    /// Answers whether `name<...>(...)` starts at the cursor, and whether
    /// the same tokens also read as comparisons.
    ///
    /// GEN-14 decides by what the name is: after the name of a generic
    /// type, `<` opens generic arguments; after any other, it compares.
    /// Only the analyser knows what the name is. The parser can say which
    /// readings are possible:
    ///
    /// * `Box<i64>(7)`: `i64` is no expression, so only the constructor
    ///   reads. `Yes`.
    /// * `a < b`: no `>` and `(` follow, so only the comparison reads.
    ///   `No`.
    /// * `a < b > (c)` and `f(a < b, c > (d))`: both read. `Both`, and
    ///   the analyser confirms or re-reads.
    fn probe_constructor(&mut self) -> Verdict {
        let start = self.position;
        let snapshot = self.snapshot();
        let first_report = self.report_count();
        let errors = self.errors;

        // The constructor reading: generic arguments that parse, then `(`.
        self.name();
        let shape = self.generic_args();
        if self.errors != errors || !shape.closed || !self.at(LParen) || self.nl() {
            return Verdict::No;
        }
        self.arg_list(ArgMode::Call);
        let end = self.position;

        // The comparison reading of the same tokens, up to `end`. With one
        // generic argument it is one expression, `a < b > (c)`. With more
        // it is one expression for each, `a < b` and `c > (d)`, which
        // needs commas to be separators where the name stands.
        let trailing_comma = shape.commas.len() == shape.count;
        if trailing_comma || (shape.count > 1 && !self.context.comma_list) {
            return Verdict::Yes;
        }
        self.rollback(&snapshot);
        // The name itself is read as a plain name this time.
        self.no_generic_at = Some(start);
        for comma in &shape.commas {
            self.expression();
            if self.position != *comma || self.errors != errors {
                return Verdict::Yes;
            }
            self.bump();
        }
        self.expression();
        // Errors past `end` are in what follows, under either reading.
        if self.position >= end && !self.errors_between(first_report, end) {
            Verdict::Both
        } else {
            Verdict::Yes
        }
    }

    /// Parses `name<...>(...)` at the cursor, which `probe_constructor`
    /// has found there.
    fn constructor(&mut self, verdict: Verdict) {
        let mark = self.mark();
        // LEX-10: `delta` is a name with a meaning only here and in types.
        let delta = self.at_word("delta") && self.nth(1) == Lt;
        self.name();
        let shape = self.generic_args();
        let (kind, mode) = if verdict == Verdict::Both {
            (UndecidedConstructor, ArgMode::Call)
        } else if delta && shape.count == 1 && shape.all_types {
            (DeltaConstructor, ArgMode::Delta)
        } else {
            (TypedConstructor, ArgMode::Call)
        };
        if self.at(LParen) && !self.nl() {
            self.arg_list(mode);
        } else {
            self.expected("`(`");
        }
        self.wrap(mark, kind);
    }

    // ---- Arguments ----

    /// ```text
    /// arguments = argument, { ",", argument }, [ "," ] ;
    /// argument = [ member_name, ":" ], expression ;
    /// eval_expression = "eval", "(", expression, { ",", argument }, [ "," ], ")" ;
    /// delta_arguments = delta_argument, { ",", delta_argument }, [ "," ] ;
    /// delta_argument = member_name, ":", expression ;
    /// ```
    ///
    /// The cursor is on the `(`.
    pub(crate) fn arg_list(&mut self, mode: ArgMode) {
        self.start(ArgList);
        self.bump();
        // LEX-5: line breaks do not count inside parentheses.
        self.nested(Context::LIST, |parser| {
            if mode == ArgMode::Eval && parser.at(RParen) {
                parser.expected("an expression");
            }
            let mut index = 0usize;
            parser.list(RParen, "an argument", true, |parser| {
                // LEX-9: `time` may label an argument. The first argument
                // of `eval` is an expression and has no label.
                let labelled = parser.at_member_name()
                    && parser.nth(1) == Colon
                    && !(mode == ArgMode::Eval && index == 0);
                if !labelled && !can_start_expression(parser.current()) {
                    return;
                }
                parser.start(Arg);
                if labelled {
                    parser.bump();
                    parser.bump();
                } else if mode == ArgMode::Delta {
                    parser.expected("a named argument");
                }
                parser.expression();
                parser.finish();
                index += 1;
            });
        });
        self.finish();
    }

    // ---- Parentheses and brackets ----

    /// ```text
    /// "(", expression, ")"
    /// tuple_literal = "(", expression, ",", [ expression, { ",", expression }, [ "," ] ], ")" ;
    /// ```
    ///
    /// EXPR-1: parentheses group; a tuple has a comma. `(a)` is `a`,
    /// `(a,)` a tuple of one.
    fn paren_expr(&mut self) {
        let mark = self.mark();
        self.bump();
        // LEX-5: line breaks do not count inside parentheses.
        let kind = self.nested(Context::LIST, |parser| {
            parser.expression();
            if parser.eat(Comma) {
                parser.list(RParen, "an expression", true, |parser| {
                    if can_start_expression(parser.current()) {
                        parser.expression();
                    }
                });
                TupleExpr
            } else {
                parser.close(RParen);
                ParenExpr
            }
        });
        self.wrap(mark, kind);
    }

    /// ```text
    /// sequence_literal = "[", [ sequence_entries ], "]" ;
    /// sequence_entries = sequence_values | sequence_pairs ;
    /// sequence_pair = sequence_item, ":", sequence_item ;
    /// sequence_item = expression | "_" ;
    /// ```
    ///
    /// EXPR-9: the entries are all values or all pairs. The first entry of
    /// the other form is reported; the tree keeps every entry as written.
    fn sequence_expr(&mut self) {
        self.start(SequenceExpr);
        self.bump();
        // LEX-5: line breaks do not count inside brackets.
        self.nested(Context::LIST, |parser| {
            // `Some(true)` once the first entry was a pair, `Some(false)`
            // once it was a value.
            let mut pairs: Option<bool> = None;
            let mut reported = false;
            parser.list(RBracket, "a value", true, |parser| {
                if !(parser.at(Placeholder) || can_start_expression(parser.current())) {
                    return;
                }
                let first_token = parser.position;
                let mark = parser.mark();
                parser.sequence_item();
                let pair = parser.eat(Colon);
                if pair {
                    parser.sequence_item();
                    parser.wrap(mark, SequencePair);
                }
                if pairs.is_none() {
                    pairs = Some(pair);
                } else if pairs != Some(pair) && !reported {
                    reported = true;
                    parser.error_behind(
                        first_token,
                        "a sequence holds values or pairs, not both",
                        "EXPR-9",
                    );
                }
            });
        });
        self.finish();
    }

    /// `sequence_item = expression | "_" ;`
    fn sequence_item(&mut self) {
        if self.at(Placeholder) {
            self.leaf(Hole);
        } else {
            self.expression();
        }
    }

    // ---- Functions and conditionals ----

    /// ```text
    /// function_expr = "fn", "(", [ lambda_parameters ], ")", [ "->", type ],
    ///     ( "=>", expression | block ) ;
    /// lambda_parameter = IDENTIFIER, [ ":", type ] ;
    /// ```
    ///
    /// True when the body is a block.
    fn function_expr(&mut self) -> bool {
        // CTRL-9 again: a block body is a brace in a condition.
        let in_condition = self.context.no_brace;
        self.start(FnExpr);
        self.bump();
        if self.at(LParen) && !self.nl() {
            self.start(ParamList);
            self.bump();
            self.nested(Context::LIST, |parser| {
                parser.list(RParen, "a parameter", true, |parser| {
                    if !parser.at(Ident) {
                        return;
                    }
                    parser.start(LambdaParam);
                    parser.bump();
                    if parser.eat(Colon) {
                        parser.type_();
                    }
                    parser.finish();
                });
            });
            self.finish();
        } else {
            self.expected("`(`");
        }
        if self.at(Arrow) {
            self.return_type();
        }
        let block = if self.at(FatArrow) && !self.nl() {
            self.arrow_body();
            false
        } else if self.at(LBrace) && !self.nl() {
            if in_condition {
                self.braces_in_condition();
            }
            self.block();
            true
        } else {
            self.expected("`=>` or `{`");
            false
        };
        self.finish();
        block
    }

    /// `if_expression = "if", expression, block, [ "else", ( block | if_expression ) ] ;`
    fn if_expr(&mut self) {
        if self.context.no_brace {
            self.braces_in_condition();
        }
        // The braces of an `if` are its own, whatever encloses it.
        let context = Context {
            no_brace: false,
            ..self.context
        };
        self.nested(context, |parser| {
            // An `else if` is an `if` inside the `else` of the one before.
            // A long ladder of them is ordinary code, so it is parsed in a
            // loop, not by this function calling itself, and does not
            // count towards the nesting limit.
            let mut open = 0usize;
            loop {
                parser.start(IfExpr);
                open += 1;
                parser.bump();
                if parser.on_line("a condition") {
                    parser.expression();
                }
                parser.body_block();
                // LEX-5: the matching `else` may start a line.
                if !parser.at(KwElse) {
                    break;
                }
                parser.bump();
                if !(parser.at(KwIf) && !parser.nl()) {
                    parser.body_block();
                    break;
                }
            }
            for _ in 0..open {
                parser.finish();
            }
        });
    }

    /// CTRL-9: "A condition that contains any other brace is
    /// parenthesized." Reported at an `if` or a block-bodied function
    /// that stands bare in the condition of a `when` or `while`.
    fn braces_in_condition(&mut self) {
        self.error(
            "a condition that contains a brace is parenthesized",
            "CTRL-9",
            None,
        );
    }
}
