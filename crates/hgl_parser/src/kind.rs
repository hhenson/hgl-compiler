//! Syntax kinds: what each token and each node of the syntax tree is.
//!
//! A syntax tree holds two sorts of element. A *token* is a leaf: one of the
//! lexer's tokens, with its text. A *node* is an interior element that groups
//! tokens and smaller nodes, such as a call or a function declaration. The
//! tree library (`rowan`) stores one number for both, so one enum,
//! [`SyntaxKind`], names them all: first every token kind, then every node
//! kind.

use hgl_lexer::TokenKind;

/// Declares [`SyntaxKind`] and its tables from one list.
///
/// The same "write the list once" technique as `hgl_lexer`'s token table
/// (the C++ "X-macro"). The token names are spelled exactly as the lexer's
/// [`TokenKind`] variants, which lets the macro write the conversion from
/// one to the other. That conversion is a `match` with no catch-all arm, so
/// a token kind added to the lexer and not listed here is a compile error,
/// not a silent mismatch.
///
/// Reading the pattern: `$( ... ),*` repeats for every comma-separated
/// entry; `$(#[$doc:meta])*` captures the doc comments written before a
/// node name so they can be put back on the generated variant.
macro_rules! define_syntax_kinds {
    (
        tokens { $( $token:ident ),* $(,)? }
        nodes { $( $(#[$doc:meta])* $node:ident ),* $(,)? }
    ) => {
        /// What a token or node of the syntax tree is.
        ///
        /// `#[repr(u16)]` stores a kind as a 16-bit number, counted from 0
        /// in the order written, which is the form `rowan` keeps.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[repr(u16)]
        pub enum SyntaxKind {
            $(
                #[doc = concat!("The token `TokenKind::", stringify!($token), "`.")]
                $token,
            )*
            /// The end of the input. The parser sees it after the last
            /// token; it never appears in a tree.
            Eof,
            $( $(#[$doc])* $node, )*
        }

        impl SyntaxKind {
            /// Every kind, in declaration order: `ALL[n]` is the kind whose
            /// number is `n`.
            pub const ALL: &'static [SyntaxKind] = &[
                $( SyntaxKind::$token, )*
                SyntaxKind::Eof,
                $( SyntaxKind::$node, )*
            ];
        }

        // `From` is Rust's standard conversion trait: this makes
        // `SyntaxKind::from(token_kind)` and `token_kind.into()` work.
        impl From<TokenKind> for SyntaxKind {
            fn from(kind: TokenKind) -> SyntaxKind {
                match kind {
                    $( TokenKind::$token => SyntaxKind::$token, )*
                }
            }
        }
    };
}

define_syntax_kinds! {
    // The lexer's token kinds, in its order.
    tokens {
        Whitespace, Newline, LineComment, BlockComment, DocComment, Ident,
        Placeholder, Int, Float, Str, Duration, Temporal,
        KwModule, KwPart, KwUse, KwAs, KwExport, KwAbstract, KwImpl,
        KwInstantiate, KwOperator, KwFn, KwStruct, KwEnum, KwRequires, KwIs,
        KwLet, KwVar, KwState, KwCache, KwInject, KwReturn, KwIf, KwElse,
        KwStart, KwWhen, KwStop, KwFor, KwWhile, KwYield, KwTest, KwAssert,
        KwEval, KwTrue, KwFalse, KwNull, KwBool, KwI64, KwF64, KwStr,
        KwBytes, KwAny, KwDate, KwTime, KwDatetime, KwDuration,
        KwCivilDatetime, KwZonedDatetime, KwZonedTime, KwTimezone,
        LParen, RParen, LBracket, RBracket, LBrace, RBrace, Comma,
        Semicolon, Colon, ColonColon, Dot, Ellipsis, At, Question, Bang,
        Plus, Minus, Star, Slash, SlashSlash, SlashSlashEq, Percent,
        PercentEq, AmpAmp, PipePipe, Eq, PlusEq, MinusEq, StarEq, SlashEq,
        EqEq, BangEq, Lt, LtEq, Gt, GtEq, Spaceship, Arrow, FatArrow,
        Unknown,
    }
    // The nodes. Each is named after the grammar production it holds; the
    // example in a doc comment shows the source text one covers.
    nodes {
        // ---- Roots ----
        /// A whole source file: the module line and the items after it.
        SourceFile,
        /// Declarations or statements with no module line: the root when
        /// the parser is asked for less than a file.
        Fragment,

        // ---- Units and names (units-and-names.md) ----
        /// `module pricing.curves part bootstrap`
        ModuleDecl,
        /// `pricing.curves`
        ModulePath,
        /// `use pricing.curves::{curve, bump}` or `use pricing.curves as pc`
        UseDecl,
        /// `{curve, bump}` in a `use` declaration
        ImportSet,
        /// `pc::curve`
        QualifiedName,

        // ---- Functions (functions-and-phases.md) ----
        /// `export const fn scale(value: f64) -> f64 => value * 2.0`
        FnDecl,
        /// `@suppress_warning("node.no_activation")` before a function
        WarningAnnotation,
        /// `(value: f64, const factor: f64 = 2.0)`, of a declaration or of
        /// a function expression
        ParamList,
        /// `const factor: f64 = 2.0` or `items: ...T`
        Param,
        /// `{T}` after `...` in a parameter
        PackType,
        /// `-> f64`
        ReturnType,
        /// `=> value * 2.0`
        ArrowBody,
        /// `<T, ...Rest, const N: i64>` in a declaration
        GenericParams,
        /// `T`, `...Rest` or `const N: i64`
        GenericParam,

        // ---- Constraints (generics-and-operators.md) ----
        /// `requires T in {i64, f64}`
        RequiresClause,
        /// `a && b` or `a || b` between constraints
        ConstraintBinary,
        /// `!constraint`
        ConstraintNot,
        /// `(constraint)`
        ConstraintParen,
        /// `T in {i64, f64}`, `N == 3` or `T is Shape`
        ConstraintRelation,
        /// `comparable(T)` or `converts(A, B) -> C`
        ConstraintCall,
        /// `each T in Ts { numeric(T) }`
        ConstraintEach,
        /// `{i64, f64}` as a constraint operand
        ConstraintSet,

        // ---- Structures and enumerations (types-and-values.md) ----
        /// `export abstract struct Quote<T> : Base { bid: T }`
        StructDecl,
        /// `: Base, Other` after a struct's name
        StructBases,
        /// `bid: f64 = 0.0` or `kind = "quote"`
        StructMember,
        /// `export enum Side { buy, sell }`
        EnumDecl,
        /// `buy` or `buy = 1`
        EnumMember,

        // ---- Operators (generics-and-operators.md) ----
        /// `operator add<T>(left: T, right: T) -> T`, with any `requires`
        /// and `properties` clauses
        OperatorDecl,
        /// `properties<i64> { associative, identity = 0 }`
        OperatorProperties,
        /// `associative` or `identity = 0`
        OperatorProperty,
        /// `instantiate add<i64>, add<f64>`
        InstantiateDecl,
        /// `add<i64, _>`
        Instantiation,

        // ---- Native boundaries (native-boundaries.md) ----
        /// `export native type Handle`
        NativeTypeDecl,
        /// `native const fn describe(value: i64) -> str`, with any contract
        /// body
        NativeFnDecl,
        /// `{ inject out; start; when; }`
        NativeContractBody,
        /// `when;`
        NativeHook,

        // ---- Tests (tests-and-eval.md) ----
        /// `test name { ... }`
        TestDecl,
        /// `test { ... }`: an unnamed context holding helpers and tests
        TestContext,

        // ---- Types (types-and-values.md) ----
        /// `i64`, `str`, `datetime` and the other reserved type names
        ScalarType,
        /// `Quote`, `pc::Curve` or `Box<i64>`
        NamedType,
        /// `<i64, 3>` after a name
        GenericArgs,
        /// `tuple<i64, str>`
        TupleType,
        /// `list<f64>` or `list<f64, 3>`
        ListType,
        /// `unbounded` as a list size
        UnboundedSize,
        /// `set<str>`
        SetType,
        /// `dict<str, f64>`
        DictType,
        /// `rolling<f64, 5m>` or `rolling<f64, 5, 2>`
        RollingType,
        /// `ref<f64>`
        RefType,
        /// `signal`
        SignalType,
        /// `atomic<Quote>`
        AtomicType,
        /// `result<f64, error>`
        ResultType,
        /// `error`
        ErrorType,
        /// `delta<list<f64>>`
        DeltaType,
        /// `f64?`: the type before the `?`, made nullable
        NullableType,

        // ---- Expressions (expressions-and-bindings.md) ----
        /// `42`, `1.5`, `"text"`, `1h30m`, `@2026-09-03`, `true`, `null`
        Literal,
        /// `price` or `pc::curve`
        NameExpr,
        /// `(a + b)`
        ParenExpr,
        /// `(a, b)` or `(a,)`
        TupleExpr,
        /// `[a, b]`, `[a: b]` or `[]`
        SequenceExpr,
        /// `a: b` in a sequence
        SequencePair,
        /// `_` where a value is left out: in a sequence or an `instantiate`
        /// argument
        Hole,
        /// `Box<i64>(value: 7)`
        TypedConstructor,
        /// A typed constructor that also reads as comparisons, as in
        /// `a < b > (c)`. The parser guesses the constructor and the
        /// analyser confirms it (GEN-14).
        UndecidedConstructor,
        /// `delta<list<f64>>(added: values)`
        DeltaConstructor,
        /// `str(value)`, `bytes(value)` or `any(value)`
        TypeCall,
        /// `fn(x) => x + 1` or `fn(x: i64) -> i64 { return x }`
        FnExpr,
        /// `x` or `x: i64` in a function expression's parameters
        LambdaParam,
        /// `if a { b } else { c }`
        IfExpr,
        /// `eval(doubled, value: [1.0, _, 3.0])`
        EvalExpr,
        /// `{ statements }`
        Block,
        /// `f(a, b: c)`: the callee, then the argument list
        CallExpr,
        /// `items[0]`
        IndexExpr,
        /// `quote.bid`
        MemberExpr,
        /// `value?`
        TryExpr,
        /// `-a` or `!a`
        UnaryExpr,
        /// `a + b`: left operand, operator, any configuration, right operand
        BinaryExpr,
        /// `{zero: error}` directly after an operator
        OperatorConfig,
        /// `zero: error` in an operator configuration
        ConfigArg,
        /// `(a, b: c)` after a callee
        ArgList,
        /// `a` or `b: c` in an argument list
        Arg,

        // ---- Statements (control-flow-and-sources.md, temporal-model.md) ----
        /// `let x = 1`, `var y: i64 = 2` or `var z: i64`
        LocalDecl,
        /// `x += 1` or `quote.bid = 2.0`
        AssignStmt,
        /// `return` or `return value`
        ReturnStmt,
        /// An expression used as a statement
        ExprStmt,
        /// `state count: i64 = 0`
        StateDecl,
        /// `cache last: f64 = 0.0`
        CacheDecl,
        /// `inject out, clock`
        InjectDecl,
        /// `start { ... }` or `stop { ... }`
        LifecycleBlock,
        /// `when { ... }` or `when modified(a) { ... }`
        WhenStmt,
        /// `for value in values { ... }`
        ForStmt,
        /// `while { ... }` or `while running { ... }`
        WhileStmt,
        /// `yield 1s: 7`
        YieldStmt,
        /// `switch side { case Side.buy: ... default: ... }`
        SwitchStmt,
        /// `case Side.buy:` and the statements under it
        SwitchCase,
        /// `default:` and the statements under it
        SwitchDefault,
        /// `assert a == b`
        AssertStmt,
        /// `raises("value.missing_key") { ... }` after `assert`
        RaisesAssertion,

        // ---- Recovery ----
        /// Tokens the parser could not place. Always accompanied by a
        /// diagnostic.
        Error,
    }
}

impl SyntaxKind {
    /// The number `rowan` stores for this kind.
    pub fn to_raw(self) -> u16 {
        // `as` converts between number types; for an enum it gives the
        // variant's number.
        self as u16
    }

    /// The kind stored as `raw`. A number no kind has gives
    /// [`SyntaxKind::Error`]; the parser never produces one.
    pub fn from_raw(raw: u16) -> SyntaxKind {
        SyntaxKind::ALL
            .get(usize::from(raw))
            .copied()
            .unwrap_or(SyntaxKind::Error)
    }

    /// True for a token the grammar never mentions: whitespace, line
    /// breaks, comments, and characters the lexer could not read.
    ///
    /// A line break can end a statement (LEX-5), but it is not written as a
    /// token of any production; the parser looks for one between tokens.
    pub fn is_trivia(self) -> bool {
        matches!(
            self,
            SyntaxKind::Whitespace
                | SyntaxKind::Newline
                | SyntaxKind::LineComment
                | SyntaxKind::BlockComment
                | SyntaxKind::DocComment
                | SyntaxKind::Unknown
        )
    }

    /// True for a kind that names a token, false for a node (and for
    /// [`SyntaxKind::Eof`], which is neither).
    pub fn is_token(self) -> bool {
        self.to_raw() < SyntaxKind::Eof.to_raw()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kind_is_stored_as_its_place_in_the_table() {
        for (index, kind) in SyntaxKind::ALL.iter().enumerate() {
            assert_eq!(usize::from(kind.to_raw()), index, "{kind:?}");
            assert_eq!(SyntaxKind::from_raw(kind.to_raw()), *kind);
        }
        assert_eq!(SyntaxKind::from_raw(u16::MAX), SyntaxKind::Error);
    }

    #[test]
    fn a_token_keeps_its_name() {
        // The conversion is by name, so the debug text is the same.
        for (_, kind) in TokenKind::KEYWORDS.iter().chain(TokenKind::PUNCTUATION) {
            let converted = SyntaxKind::from(*kind);
            assert_eq!(format!("{converted:?}"), format!("{kind:?}"));
            assert!(converted.is_token());
        }
        assert!(!SyntaxKind::Eof.is_token());
        assert!(!SyntaxKind::Block.is_token());
    }
}
