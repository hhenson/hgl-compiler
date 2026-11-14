//! Generic parameters and arguments, and `requires` clauses: the
//! generics-and-operators chapter.

use crate::grammar::{can_start_expression, can_start_type};
use crate::kind::SyntaxKind::*;
use crate::parser::{AngleList, Bracket, Choice, Context, Parser, Verdict};

use super::expressions::ADDITIVE;

impl Parser<'_> {
    // ---- Parameters ----

    /// ```text
    /// generic_parameters = "<", generic_parameter, { ",", generic_parameter }, [ "," ], ">" ;
    /// generic_parameter = IDENTIFIER | "...", IDENTIFIER, [ cardinality ]
    ///                   | "const", IDENTIFIER, ":", value_type ;
    /// ```
    ///
    /// The cursor is on the `<`.
    pub(crate) fn generic_params(&mut self) {
        // LEX-27: `fn f<T>`, not `fn f <T>`.
        self.attached(Bracket::Generics);
        self.start(GenericParams);
        self.angle_list("a generic parameter", true, false, |parser| {
            if parser.at(Ellipsis) {
                parser.start(GenericParam);
                parser.bump();
                parser.expect_name("a name");
                // GEN-10: a cardinality follows the pack it bounds.
                if parser.at(LBrace) {
                    parser.cardinality();
                }
                parser.finish();
            } else if parser.at_word("const") && parser.nth(1) == Ident {
                // FN-5: `const` before a name is the modifier.
                parser.start(GenericParam);
                parser.bump();
                parser.bump();
                if parser.eat(Colon) {
                    parser.type_();
                } else {
                    parser.expected("`:`");
                }
                parser.finish();
            } else if parser.at(Ident) {
                parser.leaf(GenericParam);
            }
            false
        });
        self.finish();
    }

    /// `cardinality = "{", INTEGER_LITERAL, [ ":", ( INTEGER_LITERAL | "*" ) ], "}" ;`
    ///
    /// GEN-10: how many arguments a pack may hold. `{n}` is exactly n,
    /// `{n:*}` n or more, `{n:m}` from n to m. The cursor is on the `{`.
    pub(crate) fn cardinality(&mut self) {
        // LEX-27: `...T{2}`, not `...T {2}`.
        self.attached(Bracket::Cardinality);
        self.start(Cardinality);
        self.bump();
        // LEX-5: line breaks do not count inside these braces.
        self.nested(Context::PLAIN, |parser| {
            if !parser.eat(Int) {
                parser.expected("an integer");
            }
            if parser.eat(Colon) && !(parser.eat(Star) || parser.eat(Int)) {
                parser.expected("an integer or `*`");
            }
            parser.close(RBrace);
        });
        self.finish();
    }

    // ---- Arguments ----

    /// ```text
    /// generic_arguments = "<", generic_argument, { ",", generic_argument }, [ "," ], ">" ;
    /// ```
    ///
    /// The cursor is on the `<`. The answer describes the list, for the
    /// caller that has to judge it (GEN-14).
    pub(crate) fn generic_args(&mut self) -> AngleList {
        // LEX-27: `Box<i64>`, not `Box <i64>`.
        self.attached(Bracket::Generics);
        self.generic_args_unchecked()
    }

    /// As [`Parser::generic_args`], without the report of a `<` that
    /// stands apart from its name. The probe that weighs a constructor
    /// against comparisons judges that gap itself.
    pub(crate) fn generic_args_unchecked(&mut self) -> AngleList {
        self.start(GenericArgs);
        let shape = self.angle_list("a type or a value", true, true, |parser| {
            parser.generic_argument()
        });
        self.finish();
        shape
    }

    /// `generic_argument = type | const_expression ;`
    ///
    /// True when the argument was parsed as a type. Parses nothing when
    /// neither can start at the cursor.
    pub(crate) fn generic_argument(&mut self) -> bool {
        let kind = self.current();
        if !(can_start_type(kind) || can_start_expression(kind)) {
            return false;
        }
        let verdict = self.choose(Choice::GenericArgument, |parser| {
            let snapshot = parser.snapshot();
            let errors = parser.errors;
            let ends_argument = |parser: &Self| parser.at(Comma) || parser.at(Gt);

            parser.type_();
            let type_clean = parser.errors == errors;
            let type_end = parser.position;
            if type_clean && ends_argument(parser) {
                return Verdict::Yes;
            }
            parser.rollback(&snapshot);

            parser.expression();
            let expression_clean = parser.errors == errors;
            if expression_clean && ends_argument(parser) {
                // `Size<N + 1>`, and `Size<N >= 3>`, where `N` alone is a
                // type that the list does not end after.
                return Verdict::No;
            }
            // Neither reading ends the argument. A clean type goes first:
            // in `x: list<i64>= value` it leaves the `>=` to be reported
            // as what it is (LEX-17).
            if type_clean {
                Verdict::Yes
            } else if expression_clean || parser.position >= type_end {
                Verdict::No
            } else {
                Verdict::Yes
            }
        });
        if verdict == Verdict::Yes {
            self.type_();
            true
        } else {
            self.expression();
            false
        }
    }

    /// `materialization_argument = type | const_expression | "_" ;`
    ///
    /// GEN-8: in an `instantiate` declaration, `_` keeps a generic
    /// parameter open.
    pub(crate) fn materialization_argument(&mut self) -> bool {
        if self.at(Placeholder) {
            self.leaf(Hole);
            false
        } else {
            self.generic_argument()
        }
    }

    // ---- Constraints ----

    /// `requires_clause = "requires", constraint_expression ;`
    pub(crate) fn requires_clause(&mut self) {
        self.start(RequiresClause);
        self.bump();
        if self.goes_on("a constraint") {
            self.constraint();
        }
        self.finish();
    }

    /// ```text
    /// constraint_expression = constraint_or ;
    /// constraint_or = constraint_and, { "||", constraint_and } ;
    /// constraint_and = constraint_term, { "&&", constraint_term } ;
    /// ```
    ///
    /// GEN-15: negation binds tightest, then conjunction, then
    /// disjunction. Each level calls the tighter one for its operands.
    fn constraint(&mut self) {
        let mark = self.mark();
        self.constraint_and();
        while self.at(PipePipe) && !self.nl() {
            self.bump();
            self.constraint_and();
            self.wrap(mark, ConstraintBinary);
        }
    }

    /// `constraint_and = constraint_term, { "&&", constraint_term } ;`
    fn constraint_and(&mut self) {
        let mark = self.mark();
        self.constraint_term();
        while self.at(AmpAmp) && !self.nl() {
            self.bump();
            self.constraint_term();
            self.wrap(mark, ConstraintBinary);
        }
    }

    /// ```text
    /// constraint_term = "!", constraint_term
    ///                 | "(", constraint_expression, ")"
    ///                 | constraint_relation
    ///                 | constraint_call
    ///                 | "each", IDENTIFIER, "in", constraint_operand, "{", constraint_expression, "}" ;
    /// constraint_relation = constraint_operand, ( "==" | "in" | "is" ), constraint_operand ;
    /// ```
    fn constraint_term(&mut self) {
        if !self.goes_on("a constraint") || !self.enter() {
            return;
        }
        if self.at(Bang) {
            self.start(ConstraintNot);
            self.bump();
            self.constraint_term();
            self.finish();
        } else if self.at_word("each")
            && self.nth(1) == Ident
            && self.nth_word(2, "in")
            && !self.cut_at(1)
            && !self.cut_at(2)
        {
            // LEX-10: `each` and `in` are names with a meaning only here.
            self.start(ConstraintEach);
            self.bump();
            self.bump();
            self.bump();
            self.constraint_operand();
            if self.eat(LBrace) {
                // LEX-5: line breaks do not count inside these braces.
                self.nested(Context::PLAIN, |parser| {
                    parser.constraint();
                    parser.close(RBrace);
                });
            } else {
                self.expected("`{`");
            }
            self.finish();
        } else if self.is_relation() {
            self.start(ConstraintRelation);
            self.constraint_operand();
            self.bump();
            self.constraint_operand();
            self.finish();
        } else if self.at(LParen) {
            self.start(ConstraintParen);
            self.bump();
            self.nested(Context::LIST, |parser| {
                parser.constraint();
                parser.close(RParen);
            });
            self.finish();
        } else if self.at(Ident) {
            self.constraint_call();
        } else {
            self.expected("a constraint");
        }
        self.leave();
    }

    /// True when a relation starts at the cursor: an operand, then `==`,
    /// `in` or `is`.
    ///
    /// A relation, a call and a parenthesised constraint can all begin
    /// with the same tokens (`f(T) == 1`, `f(T)`), so the operand is read
    /// and taken back to see what follows it.
    fn is_relation(&mut self) -> bool {
        let verdict = self.choose(Choice::Relation, |parser| {
            let errors = parser.errors;
            parser.constraint_operand();
            // An operand alone is no constraint, so the constraint is
            // incomplete here and one line break is ignored (LEX-5).
            let relates =
                (parser.at(EqEq) || parser.at(KwIs) || parser.at_word("in")) && !parser.cut();
            if parser.errors == errors && relates {
                Verdict::Yes
            } else {
                Verdict::No
            }
        });
        verdict == Verdict::Yes
    }

    /// ```text
    /// constraint_call = ( IDENTIFIER | qualified_name ),
    ///     "(", [ constraint_operand, { ",", constraint_operand } ], ")", [ "->", type ] ;
    /// ```
    ///
    /// GEN-16: one form for every constraint call, with an optional result
    /// arrow. The cursor is on the identifier.
    fn constraint_call(&mut self) {
        self.start(ConstraintCall);
        self.name();
        // LEX-27: a constraint call is a call.
        if self.at(LParen) && self.suffix_at(0) {
            self.attached(Bracket::Call);
            self.bump();
            self.nested(Context::LIST, |parser| {
                parser.list(RParen, "a type or a value", false, |parser| {
                    parser.constraint_operand();
                });
            });
        } else {
            self.expected("`==`, `in`, `is` or `(`");
        }
        if self.at(Arrow) {
            self.return_type();
        }
        self.finish();
    }

    /// ```text
    /// constraint_operand = type | additive_expr | constraint_set ;
    /// constraint_set = "{", constraint_operand, { ",", constraint_operand }, [ "," ], "}" ;
    /// ```
    ///
    /// Parses nothing, with a report, when no operand starts at the
    /// cursor: every place an operand goes requires one.
    fn constraint_operand(&mut self) {
        // LEX-5: an operand is always required, and nothing required is
        // taken across a blank line.
        if !self.goes_on("a type or a value") {
            return;
        }
        // A set may hold sets; `enter` bounds that nesting.
        if self.at(LBrace) && self.enter() {
            self.start(ConstraintSet);
            self.bump();
            // LEX-5: line breaks do not count inside a constraint set.
            self.nested(Context::PLAIN, |parser| {
                if parser.at(RBrace) {
                    parser.expected("a type or a value");
                }
                parser.list(RBrace, "a type or a value", true, |parser| {
                    parser.constraint_operand();
                });
            });
            self.finish();
            self.leave();
            return;
        }
        let kind = self.current();
        if !(can_start_type(kind) || can_start_expression(kind)) {
            self.expected("a type or a value");
            return;
        }
        // A name is a type and an expression both. It is read as a type
        // unless what follows shows it is the start of a longer
        // expression: `N + 1`, `type(T)`, `sizes[0]`.
        let verdict = self.choose(Choice::Operand, |parser| {
            let errors = parser.errors;
            parser.type_();
            let goes_on = matches!(
                parser.current(),
                LParen | LBracket | Dot | Plus | Minus | Star | Slash | SlashSlash | Percent
            );
            if parser.errors == errors && !goes_on {
                Verdict::Yes
            } else {
                Verdict::No
            }
        });
        if verdict == Verdict::Yes {
            self.type_();
        } else {
            self.binary_expr(ADDITIVE);
        }
    }
}
