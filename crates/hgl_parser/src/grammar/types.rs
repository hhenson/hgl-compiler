//! Types: the types-and-values chapter.

use crate::grammar::is_scalar_type;
use crate::kind::SyntaxKind::{self, *};
use crate::parser::{Bracket, Context, Parser};

use super::expressions::ADDITIVE;

/// The node for a built-in type that takes arguments in angle brackets,
/// by the reserved word it is written with.
fn built_in(word: SyntaxKind) -> Option<SyntaxKind> {
    Some(match word {
        KwTuple => TupleType,
        KwList => ListType,
        KwSet => SetType,
        KwDict => DictType,
        KwRolling => RollingType,
        KwRef => RefType,
        KwAtomic => AtomicType,
        KwResult => ResultType,
        KwDelta => DeltaType,
        // `return None` leaves the function; the `Some(...)` around the
        // match is not reached.
        _ => return None,
    })
}

impl Parser<'_> {
    /// `type = type_base, [ "?" ] ;`
    ///
    /// The grammar's `value_type` is parsed as `type` as well: which types
    /// are value types is a question of meaning, for the analyser.
    pub(crate) fn type_(&mut self) {
        // LEX-5: a type is always something required, and nothing required
        // is taken across a blank line.
        if !self.goes_on("a type") || !self.enter() {
            return;
        }
        let mark = self.mark();
        // TYPE-14: `T?` is a T or null. TYPE-15: the `?` covers the whole
        // type before it, so it is taken after the base, generic arguments
        // included; `list<f64?>` has its `?` taken by the inner call.
        if self.type_base() && self.eat(Question) {
            self.wrap(mark, NullableType);
        }
        self.leave();
    }

    /// ```text
    /// type_base = scalar_type | named_type | tuple_type | list_type | set_type
    ///           | dict_type | rolling_type | ref_type | "signal"
    ///           | "atomic", "<", value_type, ">" | result_type | "error" | delta_type ;
    /// ```
    ///
    /// False, with a report, when no type starts at the cursor.
    fn type_base(&mut self) -> bool {
        let kind = self.current();
        if is_scalar_type(kind) {
            self.leaf(ScalarType);
            return true;
        }
        // The names of the built-in types are reserved words.
        if kind == KwSignal {
            self.leaf(SignalType);
            return true;
        }
        if let Some(node) = built_in(kind) {
            self.built_in_type(node);
            return true;
        }
        if kind != Ident {
            self.expected("a type");
            return false;
        }
        // LEX-10: `error` is a name, not a reserved word. Alone in a
        // type position it is the built-in type.
        let qualified = self.nth(1) == ColonColon && !self.nl_at(1);
        if !qualified && self.at_word("error") {
            self.leaf(ErrorType);
            return true;
        }
        self.named_type();
        true
    }

    /// `named_type = ( IDENTIFIER | qualified_name ), [ generic_arguments ] ;`
    ///
    /// The cursor is on the identifier.
    pub(crate) fn named_type(&mut self) {
        self.start(NamedType);
        self.name();
        // GEN-14: in a type position, `<` after a name always opens
        // generic arguments.
        if self.at(Lt) && !self.nl() {
            self.generic_args();
        }
        self.finish();
    }

    /// ```text
    /// tuple_type = "tuple", "<", type, { ",", type }, ">" ;
    /// list_type = "list", "<", type, [ ",", size_expression ], ">" ;
    /// size_expression = additive_expr | "unbounded" ;
    /// set_type = "set", "<", value_type, ">" ;
    /// dict_type = "dict", "<", value_type, ",", type, ">" ;
    /// rolling_type = "rolling", "<", value_type, ",", const_expression,
    ///     [ ",", const_expression ], ">" ;
    /// ref_type = "ref", "<", type, ">" ;
    /// result_type = "result", "<", value_type, ",", value_type, ">" ;
    /// delta_type = "delta", "<", type, ">" ;
    /// ```
    ///
    /// The cursor is on the reserved word.
    fn built_in_type(&mut self, kind: SyntaxKind) {
        self.start(kind);
        self.bump();
        if !self.at(Lt) || self.nl() {
            self.expected("`<`");
            self.finish();
            return;
        }
        // LEX-27: `list<i64>`, not `list <i64>`.
        self.attached(Bracket::Generics);
        self.bump();
        // LEX-5: line breaks do not count inside angle brackets. GEN-14:
        // `>` closes the list there and is no comparison.
        self.nested(Context::ANGLE, |parser| match kind {
            TupleType => {
                parser.type_();
                while parser.eat(Comma) {
                    parser.type_();
                }
            }
            ListType => {
                parser.type_();
                if parser.eat(Comma) {
                    if parser.at_word("unbounded") && matches!(parser.nth(1), Gt | GtEq) {
                        parser.leaf(UnboundedSize);
                    } else {
                        parser.binary_expr(ADDITIVE);
                    }
                }
            }
            DictType | ResultType => {
                parser.type_();
                parser.expect(Comma);
                parser.type_();
            }
            RollingType => {
                parser.type_();
                parser.expect(Comma);
                parser.expression();
                if parser.eat(Comma) {
                    parser.expression();
                }
            }
            // `set`, `ref`, `atomic` and `delta` take one type.
            _ => parser.type_(),
        });
        self.close_angle();
        self.finish();
    }
}
