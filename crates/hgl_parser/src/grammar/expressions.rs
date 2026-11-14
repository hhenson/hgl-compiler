//! Expressions: the expressions-and-bindings chapter.

use crate::grammar::can_start_expression;
use crate::kind::SyntaxKind::{self, *};
use crate::parser::{Bracket, Choice, Context, Parser, Verdict};

/// CTRL-9: "An expression there that contains any other brace is
/// parenthesized." Said where a brace stands bare between `if`, `for`,
/// `switch`, `when` or `while` and the `{` of its body.
const BRACE_BEFORE_BODY: &str =
    "an expression with a brace in it is parenthesized before the `{` of a body";

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

/// The message for FN-10: "Parenthesize an anonymous function before
/// calling it or applying an operation to the function itself."
///
/// A function expression may stand bare as a whole value: an argument, the
/// right side of `=`, a returned value. As the operand of an operator, or
/// the thing called, indexed or reached into, it is parenthesized. The
/// expression functions below say whether what they parsed is a bare
/// function expression, so that whoever applies something to it can
/// report it.
const BARE_FN: &str = "an anonymous function is parenthesized before it is called or operated on";

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
    ///
    /// True when what was parsed is a bare function expression (FN-10).
    pub(crate) fn binary_expr(&mut self, lowest: u8) -> bool {
        // LEX-5: an expression asked for here is required, and nothing
        // required is taken across a blank line.
        if !self.goes_on("an expression") || !self.enter() {
            return false;
        }
        let mark = self.mark();
        let mut bare_fn = self.unary_expr();
        loop {
            let operator = self.current();
            let Some(tier) = binary_tier(operator) else {
                break;
            };
            // GEN-14: inside generic arguments `>` closes the list.
            if tier < lowest || (operator == Gt && self.context.no_gt) {
                break;
            }
            // LEX-5, LEX-14: a binary operator at the start of a line
            // continues the line before, `-` among them. Only a blank
            // line stops it.
            if self.nl() {
                break;
            }
            if bare_fn {
                // The left operand: `fn(x) { ... } + 1`.
                self.error(BARE_FN, "FN-10", None);
                bare_fn = false;
            }
            self.bump();
            self.operator_config();
            let operand = self.position;
            if self.binary_expr(tier + 1) {
                // The right operand: `1 + fn(x) => x`.
                self.error_behind(operand, BARE_FN, "FN-10");
            }
            // Wrapping from the same mark each time nests to the left:
            // `a - b - c` is `(a - b) - c`.
            self.wrap(mark, BinaryExpr);
        }
        self.leave();
        bare_fn
    }

    /// `unary_expr = ( "-" | "!" ), [ operator_configuration ], unary_expr | postfix_expr ;`
    ///
    /// True when what was parsed is a bare function expression (FN-10).
    fn unary_expr(&mut self) -> bool {
        if !matches!(self.current(), Minus | Bang) {
            return self.postfix_expr();
        }
        if !self.enter() {
            return false;
        }
        self.start(UnaryExpr);
        self.bump();
        // The operand is required, so one line break before it is ignored
        // (LEX-5): `-` alone is no expression.
        if self.goes_on("an expression") {
            self.operator_config();
            if self.goes_on("an expression") {
                let operand = self.position;
                if self.unary_expr() {
                    // The operand of a prefix operator: `!fn() { ... }`.
                    self.error_behind(operand, BARE_FN, "FN-10");
                }
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
        if !self.at(LBrace) || self.cut() {
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
    /// True when what was parsed is a bare function expression (FN-10).
    fn postfix_expr(&mut self) -> bool {
        let mark = self.mark();
        let start = self.position;
        let mut bare_fn = self.primary_expr();
        if self.position == start {
            // No operand, so nothing for a suffix to apply to.
            return false;
        }
        loop {
            let kind = match self.current() {
                // LEX-27: a call's `(` and an index's `[` touch what they
                // apply to. On a later line they begin a statement of
                // their own (LEX-5); apart on the same line they are
                // reported, and read.
                LParen if self.suffix_at(0) => CallExpr,
                LBracket if self.suffix_at(0) => IndexExpr,
                // LEX-5: `.` and `?` cannot begin a statement, so each may
                // start a line.
                Dot if !self.nl() => MemberExpr,
                // TYPE-15: `?` after an expression propagates a result;
                // after a type it makes the type nullable.
                Question if !self.nl() => TryExpr,
                _ => break,
            };
            if bare_fn {
                // The thing called: `fn(x) { ... }(7)`.
                self.error(BARE_FN, "FN-10", None);
                bare_fn = false;
            }
            match kind {
                CallExpr => {
                    self.attached(Bracket::Call);
                    self.arg_list(ArgMode::Call);
                }
                IndexExpr => {
                    self.attached(Bracket::Index);
                    self.index_suffix();
                }
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

    /// ```text
    /// primary_expr = literal | IDENTIFIER | qualified_name | "(", expression, ")"
    ///              | tuple_literal | sequence_literal | typed_constructor
    ///              | delta_constructor | type_call | function_expr
    ///              | if_expression | eval_expression | block ;
    /// literal = INTEGER_LITERAL | FLOAT_LITERAL | STRING_LITERAL | TEMPORAL_LITERAL
    ///         | "true" | "false" | "null" ;
    /// ```
    ///
    /// True when what was parsed is a function expression (FN-10).
    fn primary_expr(&mut self) -> bool {
        match self.current() {
            // A duration is one of the forms of `TEMPORAL_LITERAL`.
            Int | Float | Str | Duration | Temporal | KwTrue | KwFalse | KwNull => {
                self.leaf(Literal);
            }
            Ident => self.name_expr(),
            // `delta` is a reserved word. In an expression it begins a
            // delta constructor and nothing else.
            KwDelta => self.delta_constructor(),
            // TYPE-7: "Where a position expects the enum's type,
            // `.member` names one too". After an operand a `.` reaches
            // into it; here, where an operand is due, it begins one.
            Dot => {
                self.start(ImpliedMember);
                self.bump();
                self.expect_member_name();
                self.finish();
            }
            // LEX-10: `str(value)` is a conversion, though `str` is
            // reserved where a name is expected, and `error(...)` a
            // constructor. Each is a call, so its `(` touches it (LEX-27).
            KwStr | KwBytes | KwAny | KwError if self.nth(1) == LParen && self.suffix_at(1) => {
                self.start(TypeCall);
                self.bump();
                self.attached(Bracket::Call);
                self.arg_list(ArgMode::Call);
                self.finish();
            }
            LParen => self.paren_expr(),
            LBracket => self.sequence_expr(),
            KwFn => {
                self.function_expr();
                return true;
            }
            KwIf => self.if_expr(),
            KwEval => {
                self.start(EvalExpr);
                self.bump();
                // LEX-27: `eval(...)` is a call too.
                if self.at(LParen) && self.suffix_at(0) {
                    self.attached(Bracket::Call);
                    self.arg_list(ArgMode::Eval);
                } else {
                    self.expected("`(`");
                }
                self.finish();
            }
            // CTRL-9: before the body of an `if`, `for`, `switch`, `when`
            // or `while`, the first `{` opens the body. It is left for
            // the statement to take.
            LBrace if self.context.no_brace => {
                self.expected_with("an expression", Some("CTRL-9"), Some(BRACE_BEFORE_BODY))
            }
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
    /// * `a<b>(c)` and `f(a<b, c>(d))`: both read. `Both`, and the
    ///   analyser confirms or re-reads.
    /// * `a < b > (c)`: the `(` stands apart from the `>`, so it is no
    ///   call's (LEX-27), and only the comparison reads. `No`.
    fn probe_constructor(&mut self) -> Verdict {
        let start = self.position;
        let snapshot = self.snapshot();
        let first_report = self.report_count();
        let errors = self.errors;

        // The constructor reading: generic arguments that parse, then `(`.
        self.name();
        // LEX-27: a constructor's `<` touches its name and its `(` touches
        // the `>`. With a gap at either the tokens are no constructor,
        // where they read as comparisons: `a < b > (c)`. Where they do
        // not, the constructor was meant and `constructor` reports the
        // gap.
        let mut attached = self.adjacent_at(0);
        let shape = self.generic_args_unchecked();
        if self.errors != errors || !shape.closed || !self.at(LParen) || self.nl() {
            return Verdict::No;
        }
        attached &= self.adjacent_at(0);
        self.arg_list(ArgMode::Call);
        let end = self.position;

        // The comparison reading of the same tokens, up to `end`. With one
        // generic argument it is one expression, `a<b>(c)` as `a < b >
        // (c)`. With more it is one expression for each, `a < b` and
        // `c > (d)`, which
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
        let comparisons = self.position >= end && !self.errors_between(first_report, end);
        match (attached, comparisons) {
            (true, true) => Verdict::Both,
            (false, true) => Verdict::No,
            (_, false) => Verdict::Yes,
        }
    }

    /// Parses `name<...>(...)` at the cursor, which `probe_constructor`
    /// has found there.
    fn constructor(&mut self, verdict: Verdict) {
        let mark = self.mark();
        self.name();
        self.generic_args();
        let kind = if verdict == Verdict::Both {
            UndecidedConstructor
        } else {
            TypedConstructor
        };
        if self.at(LParen) && !self.nl() {
            self.attached(Bracket::Call);
            self.arg_list(ArgMode::Call);
        } else {
            self.expected("`(`");
        }
        self.wrap(mark, kind);
    }

    /// `delta_constructor = "delta", "<", type, ">", "(", [ delta_arguments ], ")" ;`
    ///
    /// The cursor is on `delta`. The word is reserved, so nothing here
    /// reads two ways: no name is called `delta`, and what follows it
    /// is its one type and its arguments.
    fn delta_constructor(&mut self) {
        self.start(DeltaConstructor);
        self.bump();
        if self.at(Lt) && !self.nl() {
            // LEX-27: `delta<...>`, not `delta <...>`.
            self.attached(Bracket::Generics);
            // One type, where a typed constructor has a list of
            // arguments. It is kept in the same kind of node.
            self.start(GenericArgs);
            self.bump();
            self.nested(Context::ANGLE, |parser| parser.type_());
            self.close_angle();
            self.finish();
        } else {
            self.expected("`<`");
        }
        if self.at(LParen) && !self.nl() {
            // LEX-27: a constructor's `(` touches what it follows.
            self.attached(Bracket::Call);
            self.arg_list(ArgMode::Delta);
        } else {
            self.expected("`(`");
        }
        self.finish();
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
    /// With an arrow body the function takes in everything that follows
    /// it (FN-10); with a block body it ends at the `}`.
    fn function_expr(&mut self) {
        // CTRL-9 again: a block body is a brace in a condition.
        let in_condition = self.context.no_brace;
        self.start(FnExpr);
        self.bump();
        if self.at(LParen) && !self.cut() {
            // LEX-27: `fn(x)`, as `fn f(x)`.
            self.attached(Bracket::Parameters);
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
        if self.at(Arrow) && !self.nl() {
            self.return_type();
        }
        if self.at(FatArrow) && !self.nl() {
            self.arrow_body();
        } else if self.at(LBrace) && !self.nl() {
            if in_condition {
                self.braces_in_condition();
            }
            self.block();
        } else {
            self.expected("`=>` or `{`");
        }
        self.finish();
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
                if parser.goes_on("a condition") {
                    // CTRL-9: the first `{` opens the body.
                    parser.subject();
                }
                parser.body_block();
                // LEX-5: the matching `else` may start a line.
                if !parser.at(KwElse) || parser.nl() {
                    break;
                }
                parser.bump();
                if !(parser.at(KwIf) && !parser.cut()) {
                    parser.body_block();
                    break;
                }
            }
            for _ in 0..open {
                parser.finish();
            }
        });
    }

    /// CTRL-9: "An expression there that contains any other brace is
    /// parenthesized." Reported at an `if` or a block-bodied function
    /// that stands bare before the body of an `if`, `for`, `switch`,
    /// `when` or `while`.
    fn braces_in_condition(&mut self) {
        self.error(BRACE_BEFORE_BODY, "CTRL-9", None);
    }
}
