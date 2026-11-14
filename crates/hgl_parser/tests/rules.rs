//! The rules of the specification that the parser implements, one group
//! of tests for each.
//!
//! Each test names its rule and shows it at work on a small piece of
//! source: the tree that accepted source gives, or the diagnostic that
//! rejected source gets. Trees are written in the one-line form of
//! [`hgl_parser::outline`]: a node is its kind with its children in
//! brackets, a token is its text in quotes.
//!
//! These tests say what this parser does with each rule. They are not a
//! conformance claim: the specification's conformance cases are run by a
//! later stage.

mod common;

use common::run;
use hgl_parser::{Entry, SyntaxKind};

/// Parses `text`, which the grammar accepts, and gives the outline of what
/// the root holds.
fn accepted(entry: Entry, text: &str) -> String {
    let outcome = run(entry, text);
    assert_eq!(outcome.findings, [], "`{text}`\n{}", outcome.report);
    assert!(!outcome.has_error_node(), "`{text}`");
    let outline = outcome.outline();
    // Drop the root's own `Fragment[` and `]`.
    let inside = outline.find('[').expect("a root") + 1;
    outline[inside..outline.len() - 1].to_string()
}

/// [`accepted`] for statements.
fn statements(text: &str) -> String {
    accepted(Entry::Statements, text)
}

/// [`accepted`] for declarations with no `module` line.
fn declarations(text: &str) -> String {
    accepted(Entry::Declarations, text)
}

/// Parses `text`, which the grammar rejects, and gives each diagnostic as
/// `rule line:column: message`, with `-` where no rule is cited.
fn rejected(entry: Entry, text: &str) -> Vec<String> {
    let outcome = run(entry, text);
    assert!(!outcome.findings.is_empty(), "`{text}` should be rejected");
    outcome
        .findings
        .iter()
        .map(|finding| {
            // The grammar has one catalogued code; the documentation
            // comment rules have none, so theirs are local.
            let grammar = finding.code == "syntax.expected_token";
            assert_eq!(finding.catalogued, grammar, "{}", finding.code);
            format!(
                "{} {}: {}",
                finding.rule.unwrap_or("-"),
                finding.at,
                finding.message
            )
        })
        .collect()
}

/// Checks that `text` parses to the same tree as `plain`, which is the
/// same source without the line breaks under test.
fn same_tree(entry: Entry, text: &str, plain: &str) {
    assert_eq!(accepted(entry, text), accepted(entry, plain), "`{text}`");
}

// ---- LEX-4 ----

#[test]
fn braces_delimit_a_block_and_indentation_means_nothing() {
    // LEX-4: "Braces delimit blocks. Indentation is non-semantic."
    let indented = "when {\n    step()\n    other()\n}";
    let ragged = "when {\nstep()\n            other()\n  }";
    assert_eq!(statements(indented), statements(ragged));
    assert_eq!(
        statements(indented),
        "WhenStmt[\"when\" Block[\"{\" \
         ExprStmt[CallExpr[NameExpr[\"step\"] ArgList[\"(\" \")\"]]] \
         ExprStmt[CallExpr[NameExpr[\"other\"] ArgList[\"(\" \")\"]]] \"}\"]]"
    );
}

#[test]
fn a_semicolon_does_not_end_a_statement() {
    // LEX-4: "A semicolon is not an ordinary statement terminator."
    assert_eq!(
        rejected(Entry::Statements, "let a = 1;"),
        ["LEX-4 1:10: `;` does not end a statement"]
    );
    assert_eq!(
        rejected(Entry::Statements, "let a = 1; let b = 2"),
        ["LEX-4 1:10: `;` does not end a statement"]
    );
    // "Native hook markers are a separate declaration form": NATIVE-3
    // below.
}

// ---- LEX-5 ----

#[test]
fn a_line_break_separates_statements_and_declarations() {
    // LEX-5: "A newline separates declarations or statements only where
    // one can end."
    assert_eq!(
        statements("let a = 1\nlet b = 2"),
        "LocalDecl[\"let\" \"a\" \"=\" Literal[\"1\"]] LocalDecl[\"let\" \"b\" \"=\" Literal[\"2\"]]"
    );
    assert_eq!(
        rejected(Entry::Declarations, "fn a() {} fn b() {}"),
        ["LEX-5 1:11: expected a line break, found `fn`"]
    );
    assert_eq!(
        rejected(Entry::Declarations, "struct P { x: f64 y: f64 }"),
        ["LEX-5 1:19: expected a line break, found `y`"]
    );
}

#[test]
fn a_line_break_inside_a_list_is_ignored() {
    // LEX-5: "Ignore it inside parentheses, brackets, and angle-bracketed
    // lists, and inside braces that hold a list rather than statements or
    // members: import sets, constraint sets and `each` constraints,
    // operator property lists, pack types, and operator configuration".
    use Entry::{Declarations, Statements};
    for (entry, text, plain) in [
        (Statements, "f(\n    a,\n    b\n)", "f(a, b)"),
        (Statements, "[\n    a,\n    b\n]", "[a, b]"),
        (Statements, "a[\n    0\n]", "a[0]"),
        (
            Statements,
            "let x: dict<\n    str,\n    f64\n> = y",
            "let x: dict<str, f64> = y",
        ),
        (Statements, "Box<\n    i64\n>(\n    1\n)", "Box<i64>(1)"),
        (
            Declarations,
            "use a.b::{\n    c,\n    d\n}",
            "use a.b::{c, d}",
        ),
        (
            Declarations,
            "fn f<\n    T\n>() requires T in {\n    i64,\n    f64\n} {}",
            "fn f<T>() requires T in {i64, f64} {}",
        ),
        (
            Declarations,
            "fn f<...Ts>() requires each T in Ts {\n    p(T)\n} {}",
            "fn f<...Ts>() requires each T in Ts { p(T) } {}",
        ),
        (
            Declarations,
            "operator add(a: i64) -> i64 properties<\n    i64\n> {\n    associative,\n    identity = 0\n}",
            "operator add(a: i64) -> i64 properties<i64> { associative, identity = 0 }",
        ),
        (
            Declarations,
            "fn f(rest: ...{\n    str\n}) {}",
            "fn f(rest: ...{str}) {}",
        ),
        (
            Statements,
            "a /{\n    zero: error\n} b",
            "a /{zero: error} b",
        ),
    ] {
        same_tree(entry, text, plain);
    }
}

#[test]
fn a_line_break_after_a_token_that_cannot_end_a_line_is_ignored() {
    // LEX-5: "after a comma, binary/assignment operator, `=`, `=>`, `->`,
    // or a type/named-argument colon".
    use Entry::{Declarations, Statements};
    for (entry, text, plain) in [
        (Declarations, "enum E { a,\n    b }", "enum E { a, b }"),
        (
            Declarations,
            "struct S : A,\n    B {}",
            "struct S : A, B {}",
        ),
        (Statements, "let a = b +\n    c", "let a = b + c"),
        (Statements, "let a = b &&\n    c", "let a = b && c"),
        (Statements, "a +=\n    2", "a += 2"),
        (Statements, "let a =\n    1", "let a = 1"),
        (Declarations, "fn f() =>\n    1", "fn f() => 1"),
        (
            Declarations,
            "fn f() ->\n    i64 { return 1 }",
            "fn f() -> i64 { return 1 }",
        ),
        (Statements, "let u:\n    i64 = 1", "let u: i64 = 1"),
        (
            Declarations,
            "struct S { a:\n    i64 }",
            "struct S { a: i64 }",
        ),
    ] {
        same_tree(entry, text, plain);
    }
}

#[test]
fn a_line_break_before_a_token_that_continues_a_line_is_ignored() {
    // LEX-5: "before a binary operator, result arrow, or matching `else`".
    use Entry::{Declarations, Statements};
    for (entry, text, plain) in [
        (Statements, "let a = b\n    + c", "let a = b + c"),
        (Statements, "let a = b\n    - c", "let a = b - c"),
        (Statements, "let a = b\n    == c", "let a = b == c"),
        (
            Declarations,
            "fn f()\n    -> i64 { return 1 }",
            "fn f() -> i64 { return 1 }",
        ),
        (
            Statements,
            "if a { b }\nelse { c }",
            "if a { b } else { c }",
        ),
    ] {
        same_tree(entry, text, plain);
    }
}

#[test]
fn a_bang_does_not_continue_a_line() {
    // LEX-5: "`!` does not continue a line."
    assert_eq!(
        statements("let a = b\n!c"),
        "LocalDecl[\"let\" \"a\" \"=\" NameExpr[\"b\"]] ExprStmt[UnaryExpr[\"!\" NameExpr[\"c\"]]]"
    );
}

#[test]
fn a_call_or_an_index_does_not_cross_a_line_break() {
    // LEX-5: "Postfix calls/indexing do not cross a significant newline."
    assert_eq!(
        statements("let a = b\n(c)"),
        "LocalDecl[\"let\" \"a\" \"=\" NameExpr[\"b\"]] ExprStmt[ParenExpr[\"(\" NameExpr[\"c\"] \")\"]]"
    );
    assert_eq!(
        statements("let a = b\n[c]"),
        "LocalDecl[\"let\" \"a\" \"=\" NameExpr[\"b\"]] ExprStmt[SequenceExpr[\"[\" NameExpr[\"c\"] \"]\"]]"
    );
}

#[test]
fn a_bodys_opening_brace_stays_with_its_header() {
    // LEX-5: "A body's opening brace stays with its header."
    let message = "a body's opening brace stays on its header's line";
    for (entry, text, at) in [
        (Entry::Statements, "when ready\n{ step() }", "2:1"),
        (Entry::Statements, "if a\n{ b }", "2:1"),
        (Entry::Declarations, "fn f()\n{\n}", "2:1"),
        (Entry::Declarations, "struct S\n{\n}", "2:1"),
        (Entry::Declarations, "test t\n{\n}", "2:1"),
    ] {
        assert_eq!(rejected(entry, text), [format!("LEX-5 {at}: {message}")]);
    }
    // The body is parsed all the same, so one mistake is one report.
    let outcome = run(Entry::Statements, "when ready\n{ step() }\nother()");
    assert_eq!(outcome.rules(), ["LEX-5"]);
    assert!(!outcome.has_error_node());
}

#[test]
fn a_line_break_anywhere_else_ends_the_line() {
    // LEX-5 lists where a line break is ignored. This parser ignores one
    // nowhere else, so a line that stops short is incomplete. (LEX-5's
    // first sentence could be read more widely; the reading is reported
    // to the specification's issue tracker.)
    assert_eq!(
        rejected(Entry::Statements, "let\n    a = 1"),
        ["- 1:4: expected a name, found end of line"]
    );
    assert_eq!(
        rejected(Entry::Statements, "let a = -\n    b"),
        ["- 1:10: expected an expression, found end of line"]
    );
    assert_eq!(
        rejected(Entry::Statements, "let a = b\n    .c"),
        ["- 2:5: expected a statement, found `.`"]
    );
    assert_eq!(
        rejected(Entry::Declarations, "fn f() -> i64\n    => 1"),
        ["LEX-5 2:5: `=>` stays on its header's line"]
    );
    assert_eq!(
        rejected(Entry::Declarations, "enum Side {\n    buy\n    , sell\n}"),
        ["- 2:8: expected `,` or `}`, found end of line"]
    );
}

// ---- LEX-6 ----

#[test]
fn two_statements_on_one_line_are_rejected() {
    // LEX-6: "Two ordinary statements on one physical line are rejected."
    assert_eq!(
        rejected(Entry::Statements, "let a = 1 let b = 2"),
        ["LEX-6 1:11: expected a line break, found `let`"]
    );
    // Both statements are in the tree.
    let outcome = run(Entry::Statements, "let a = 1 let b = 2");
    assert!(!outcome.has_error_node());
}

#[test]
fn indentation_does_not_join_statements() {
    // LEX-6: "Inside blocks, indentation alone cannot join statements."
    assert_eq!(
        statements("let a = b\n    c"),
        "LocalDecl[\"let\" \"a\" \"=\" NameExpr[\"b\"]] ExprStmt[NameExpr[\"c\"]]"
    );
}

#[test]
fn an_inject_list_continues_after_a_comma() {
    // LEX-6: "An inject list may continue after a comma when the next
    // line begins with a name."
    assert_eq!(
        statements("inject out,\n    clock"),
        "InjectDecl[\"inject\" \"out\" \",\" \"clock\"]"
    );
    assert_eq!(
        statements("inject out, clock,\nreturn"),
        "InjectDecl[\"inject\" \"out\" \",\" \"clock\" \",\"] ReturnStmt[\"return\"]"
    );
}

// ---- LEX-9 ----

#[test]
fn time_names_a_member_and_labels_an_argument() {
    // LEX-9: "`time` is also admitted as a field/member name and
    // named-argument label."
    assert_eq!(
        statements("quote.time = clock.time"),
        "AssignStmt[MemberExpr[NameExpr[\"quote\"] \".\" \"time\"] \"=\" \
         MemberExpr[NameExpr[\"clock\"] \".\" \"time\"]]"
    );
    assert_eq!(
        statements("f(time: now)"),
        "ExprStmt[CallExpr[NameExpr[\"f\"] ArgList[\"(\" Arg[\"time\" \":\" NameExpr[\"now\"]] \")\"]]]"
    );
    assert_eq!(
        declarations("struct Tick { time: datetime }"),
        "StructDecl[\"struct\" \"Tick\" \"{\" StructMember[\"time\" \":\" ScalarType[\"datetime\"]] \"}\"]"
    );
    // Elsewhere it is the reserved type name.
    assert_eq!(
        rejected(Entry::Statements, "let time = 1"),
        ["- 1:5: expected a name, found `time`"]
    );
}

#[test]
fn const_is_a_modifier_or_a_name_by_position() {
    // LEX-9: "`const` is a modifier only in positions that admit it;
    // elsewhere it is an ordinary identifier, including callable and
    // imported names (FN-5)."
    // FN-5 is the same rule from the functions chapter.
    assert_eq!(
        declarations("const fn const(const: i64, const n: i64 = 3) -> i64 => const"),
        "FnDecl[\"const\" \"fn\" \"const\" ParamList[\"(\" \
         Param[\"const\" \":\" ScalarType[\"i64\"]] \",\" \
         Param[\"const\" \"n\" \":\" ScalarType[\"i64\"] \"=\" Literal[\"3\"]] \")\"] \
         ReturnType[\"->\" ScalarType[\"i64\"]] ArrowBody[\"=>\" NameExpr[\"const\"]]]"
    );
    assert_eq!(
        declarations("use a.b::{const}"),
        "UseDecl[\"use\" ModulePath[\"a\" \".\" \"b\"] \"::\" ImportSet[\"{\" \"const\" \"}\"]]"
    );
    assert_eq!(
        declarations("fn f<const N: i64, const>() {}"),
        "FnDecl[\"fn\" \"f\" GenericParams[\"<\" GenericParam[\"const\" \"N\" \":\" ScalarType[\"i64\"]] \
         \",\" GenericParam[\"const\"] \">\"] ParamList[\"(\" \")\"] Block[\"{\" \"}\"]]"
    );
    assert_eq!(
        statements("const = const(7)"),
        "AssignStmt[NameExpr[\"const\"] \"=\" \
         CallExpr[NameExpr[\"const\"] ArgList[\"(\" Arg[Literal[\"7\"]] \")\"]]]"
    );
}

// ---- LEX-10 ----

#[test]
fn a_contextual_name_is_an_ordinary_name_elsewhere() {
    // LEX-10: "Each acquires its special meaning only in its defined
    // positions. A contextual spelling is not globally reserved."
    assert_eq!(
        statements("let list = 1\nlet delta = list\nlet result = case\nlet in = default"),
        "LocalDecl[\"let\" \"list\" \"=\" Literal[\"1\"]] \
         LocalDecl[\"let\" \"delta\" \"=\" NameExpr[\"list\"]] \
         LocalDecl[\"let\" \"result\" \"=\" NameExpr[\"case\"]] \
         LocalDecl[\"let\" \"in\" \"=\" NameExpr[\"default\"]]"
    );
    // `switch` and `raises` begin a statement form only when the rest of
    // the form follows.
    assert_eq!(
        statements("switch = 3"),
        "AssignStmt[NameExpr[\"switch\"] \"=\" Literal[\"3\"]]"
    );
    assert_eq!(
        statements("assert raises(x) == 1"),
        "AssertStmt[\"assert\" BinaryExpr[CallExpr[NameExpr[\"raises\"] \
         ArgList[\"(\" Arg[NameExpr[\"x\"]] \")\"]] \"==\" Literal[\"1\"]]]"
    );
    // In its position the same spelling has its meaning.
    assert_eq!(
        statements("let x: list<i64> = native"),
        "LocalDecl[\"let\" \"x\" \":\" ListType[\"list\" \"<\" ScalarType[\"i64\"] \">\"] \"=\" NameExpr[\"native\"]]"
    );
}

#[test]
fn str_of_a_value_is_a_conversion() {
    // LEX-10: "`str(value)` is a conversion expression despite `str`
    // being reserved in ordinary name positions."
    assert_eq!(
        statements("str(Side.sell) == \"sell\""),
        "ExprStmt[BinaryExpr[TypeCall[\"str\" ArgList[\"(\" \
         Arg[MemberExpr[NameExpr[\"Side\"] \".\" \"sell\"]] \")\"]] \"==\" Literal[\"\\\"sell\\\"\"]]]"
    );
    assert_eq!(
        statements("bytes(value)"),
        "ExprStmt[TypeCall[\"bytes\" ArgList[\"(\" Arg[NameExpr[\"value\"]] \")\"]]]"
    );
    assert_eq!(
        statements("any(value)"),
        "ExprStmt[TypeCall[\"any\" ArgList[\"(\" Arg[NameExpr[\"value\"]] \")\"]]]"
    );
    assert_eq!(
        rejected(Entry::Statements, "let s = str"),
        ["- 1:9: expected an expression, found `str`"]
    );
}

// ---- LEX-14 ----

#[test]
fn the_full_comparison_continues_a_line_as_any_binary_operator_does() {
    // LEX-14: "Its newline behavior follows LEX-5's binary operator rule.
    // Operator configuration uses ordinary brace/colon tokens."
    assert_eq!(
        statements("let order = left\n    <=> right"),
        "LocalDecl[\"let\" \"order\" \"=\" BinaryExpr[NameExpr[\"left\"] \"<=>\" NameExpr[\"right\"]]]"
    );
    assert_eq!(
        statements("a <=>{nan: last} b"),
        "ExprStmt[BinaryExpr[NameExpr[\"a\"] \"<=>\" \
         OperatorConfig[\"{\" ConfigArg[\"nan\" \":\" NameExpr[\"last\"]] \"}\"] NameExpr[\"b\"]]]"
    );
}

// ---- LEX-17 ----

#[test]
fn gt_eq_does_not_close_generic_arguments() {
    // LEX-17: "`>` directly followed by `=` is `>=` even after generic
    // arguments: write `list<i64> = value`."
    assert_eq!(
        statements("let x: list<i64> = value"),
        "LocalDecl[\"let\" \"x\" \":\" ListType[\"list\" \"<\" ScalarType[\"i64\"] \">\"] \"=\" NameExpr[\"value\"]]"
    );
    assert_eq!(
        rejected(Entry::Statements, "let x: list<i64>= value"),
        ["LEX-17 1:16: `>=` is one token and does not close generic arguments"]
    );
    // The token is not split: the tree holds the `>=` the lexer gave.
    let outcome = run(Entry::Statements, "let x: list<i64>= value\nlet y = 1");
    assert_eq!(outcome.rules(), ["LEX-17"]);
    assert!(outcome.outline().contains("ScalarType[\"i64\"] \">=\"]"));
    // "No token joins `>` to a following `>`": nested lists close with
    // separate tokens.
    assert_eq!(
        statements("let x: dict<str, list<f64>> = y"),
        "LocalDecl[\"let\" \"x\" \":\" DictType[\"dict\" \"<\" ScalarType[\"str\"] \",\" \
         ListType[\"list\" \"<\" ScalarType[\"f64\"] \">\"] \">\"] \"=\" NameExpr[\"y\"]]"
    );
}

// ---- LEX-21 ----

#[test]
fn a_comment_that_holds_a_line_break_counts_as_one() {
    // LEX-21: "A block or documentation comment separates tokens as
    // whitespace does. One that contains a line break also counts as one
    // line break at its position."
    assert_eq!(
        statements("let a = 1 /* one\ntwo */ let b = 2"),
        "LocalDecl[\"let\" \"a\" \"=\" Literal[\"1\"]] LocalDecl[\"let\" \"b\" \"=\" Literal[\"2\"]]"
    );
    assert_eq!(
        rejected(Entry::Statements, "let a = 1 /* note */ let b = 2"),
        ["LEX-6 1:22: expected a line break, found `let`"]
    );
    // So a call does not reach across one (LEX-5).
    assert_eq!(
        statements("let a = b /* one\ntwo */ (c)"),
        "LocalDecl[\"let\" \"a\" \"=\" NameExpr[\"b\"]] ExprStmt[ParenExpr[\"(\" NameExpr[\"c\"] \")\"]]"
    );
    // "A line comment ends before its line break, which remains."
    assert_eq!(
        statements("let a = 1 # note\nlet b = 2"),
        "LocalDecl[\"let\" \"a\" \"=\" Literal[\"1\"]] LocalDecl[\"let\" \"b\" \"=\" Literal[\"2\"]]"
    );
}

// ---- UNIT-1 ----

#[test]
fn a_file_opens_with_one_module_line() {
    // UNIT-1: "A file names one module and optionally one part".
    assert_eq!(
        accepted(Entry::SourceFile, "module pricing.curves part bootstrap\n"),
        "ModuleDecl[\"module\" ModulePath[\"pricing\" \".\" \"curves\"] \"part\" \"bootstrap\"]"
    );
    assert_eq!(
        accepted(
            Entry::SourceFile,
            "\n# a comment first\nmodule a\n\nfn f() {}\n"
        ),
        "ModuleDecl[\"module\" ModulePath[\"a\"]] FnDecl[\"fn\" \"f\" ParamList[\"(\" \")\"] Block[\"{\" \"}\"]]"
    );
    assert_eq!(
        rejected(Entry::SourceFile, "fn f() {}"),
        ["UNIT-1 1:1: expected `module`, found `fn`"]
    );
    assert_eq!(
        rejected(Entry::SourceFile, ""),
        ["UNIT-1 1:1: expected `module`, found end of file"]
    );
    assert_eq!(
        rejected(Entry::SourceFile, "module a\nmodule b"),
        ["UNIT-1 2:1: a file has one `module` line, and it comes first"]
    );
    assert_eq!(
        rejected(Entry::SourceFile, "module a fn f() {}"),
        ["LEX-5 1:10: expected a line break, found `fn`"]
    );
}

// ---- TYPE-14, TYPE-15 ----

#[test]
fn a_question_mark_after_a_type_makes_it_nullable() {
    // TYPE-14: "`T?` denotes an ordinary value that is either a T or
    // null".
    assert_eq!(
        statements("var a: f64?"),
        "LocalDecl[\"var\" \"a\" \":\" NullableType[ScalarType[\"f64\"] \"?\"]]"
    );
    assert_eq!(
        declarations("const fn probe(value: f64? = 2.0) -> f64? => value"),
        "FnDecl[\"const\" \"fn\" \"probe\" ParamList[\"(\" \
         Param[\"value\" \":\" NullableType[ScalarType[\"f64\"] \"?\"] \"=\" Literal[\"2.0\"]] \")\"] \
         ReturnType[\"->\" NullableType[ScalarType[\"f64\"] \"?\"]] ArrowBody[\"=>\" NameExpr[\"value\"]]]"
    );
}

#[test]
fn the_nullable_suffix_covers_the_whole_type_before_it() {
    // TYPE-15: "The suffix applies to the complete preceding type:
    // `list<f64?>` has nullable elements; `list<f64>?` is a nullable list
    // value".
    assert_eq!(
        statements("var a: list<f64?>"),
        "LocalDecl[\"var\" \"a\" \":\" ListType[\"list\" \"<\" NullableType[ScalarType[\"f64\"] \"?\"] \">\"]]"
    );
    assert_eq!(
        statements("var b: list<f64>?"),
        "LocalDecl[\"var\" \"b\" \":\" NullableType[ListType[\"list\" \"<\" ScalarType[\"f64\"] \">\"] \"?\"]]"
    );
    assert_eq!(
        statements("var c: m::Box<i64>?"),
        "LocalDecl[\"var\" \"c\" \":\" NullableType[NamedType[QualifiedName[\"m\" \"::\" \"Box\"] \
         GenericArgs[\"<\" ScalarType[\"i64\"] \">\"]] \"?\"]]"
    );
}

#[test]
fn a_question_mark_after_an_expression_propagates_a_result() {
    // TYPE-15: "A type-position `?` declares nullability;
    // expression-position `?` retains ERR-5's result propagation
    // meaning."
    assert_eq!(
        statements("let c = f(x)?"),
        "LocalDecl[\"let\" \"c\" \"=\" TryExpr[CallExpr[NameExpr[\"f\"] \
         ArgList[\"(\" Arg[NameExpr[\"x\"]] \")\"]] \"?\"]]"
    );
    assert_eq!(
        statements("let d: f64? = (a /{zero: error} b)?"),
        "LocalDecl[\"let\" \"d\" \":\" NullableType[ScalarType[\"f64\"] \"?\"] \"=\" \
         TryExpr[ParenExpr[\"(\" BinaryExpr[NameExpr[\"a\"] \"/\" \
         OperatorConfig[\"{\" ConfigArg[\"zero\" \":\" NameExpr[\"error\"]] \"}\"] NameExpr[\"b\"]] \")\"] \"?\"]]"
    );
}

// ---- EXPR-1, EXPR-11 ----

#[test]
fn operators_bind_by_tier() {
    // EXPR-1: lowest `||`, then `&&`, equality, comparison, additive,
    // multiplicative, unary, postfix.
    assert_eq!(
        statements("a || b && c == d < e + f * -g"),
        "ExprStmt[BinaryExpr[NameExpr[\"a\"] \"||\" BinaryExpr[NameExpr[\"b\"] \"&&\" \
         BinaryExpr[NameExpr[\"c\"] \"==\" BinaryExpr[NameExpr[\"d\"] \"<\" \
         BinaryExpr[NameExpr[\"e\"] \"+\" BinaryExpr[NameExpr[\"f\"] \"*\" \
         UnaryExpr[\"-\" NameExpr[\"g\"]]]]]]]]]"
    );
    // Postfix binds tighter than a prefix operator.
    assert_eq!(
        statements("-a.b(c)[0]?"),
        "ExprStmt[UnaryExpr[\"-\" TryExpr[IndexExpr[CallExpr[MemberExpr[NameExpr[\"a\"] \".\" \"b\"] \
         ArgList[\"(\" Arg[NameExpr[\"c\"]] \")\"]] \"[\" Literal[\"0\"] \"]\"] \"?\"]]]"
    );
}

#[test]
fn binary_operators_group_to_the_left_and_prefix_ones_to_the_right() {
    // EXPR-1: binary operators are left-associative, unary ones
    // right-associative.
    assert_eq!(
        statements("a - b - c"),
        "ExprStmt[BinaryExpr[BinaryExpr[NameExpr[\"a\"] \"-\" NameExpr[\"b\"]] \"-\" NameExpr[\"c\"]]]"
    );
    assert_eq!(
        statements("a // b % c"),
        "ExprStmt[BinaryExpr[BinaryExpr[NameExpr[\"a\"] \"//\" NameExpr[\"b\"]] \"%\" NameExpr[\"c\"]]]"
    );
    assert_eq!(
        statements("!-a"),
        "ExprStmt[UnaryExpr[\"!\" UnaryExpr[\"-\" NameExpr[\"a\"]]]]"
    );
}

#[test]
fn parentheses_group_and_a_tuple_has_a_comma() {
    // EXPR-1: "a tuple has a comma".
    assert_eq!(
        statements("(a + b) * c"),
        "ExprStmt[BinaryExpr[ParenExpr[\"(\" BinaryExpr[NameExpr[\"a\"] \"+\" NameExpr[\"b\"]] \")\"] \
         \"*\" NameExpr[\"c\"]]]"
    );
    assert_eq!(
        statements("(a)"),
        "ExprStmt[ParenExpr[\"(\" NameExpr[\"a\"] \")\"]]"
    );
    assert_eq!(
        statements("(a,)"),
        "ExprStmt[TupleExpr[\"(\" NameExpr[\"a\"] \",\" \")\"]]"
    );
    assert_eq!(
        statements("(a, b)"),
        "ExprStmt[TupleExpr[\"(\" NameExpr[\"a\"] \",\" NameExpr[\"b\"] \")\"]]"
    );
    assert_eq!(
        rejected(Entry::Statements, "()"),
        ["- 1:2: expected an expression, found `)`"]
    );
}

#[test]
fn the_full_comparison_sits_with_the_comparisons() {
    // EXPR-11: `<=>` at the comparison tier: tighter than equality,
    // looser than addition.
    assert_eq!(
        statements("a == b <=> c + 1"),
        "ExprStmt[BinaryExpr[NameExpr[\"a\"] \"==\" BinaryExpr[NameExpr[\"b\"] \"<=>\" \
         BinaryExpr[NameExpr[\"c\"] \"+\" Literal[\"1\"]]]]]"
    );
    assert_eq!(
        statements("1.0 <=> 2.0"),
        "ExprStmt[BinaryExpr[Literal[\"1.0\"] \"<=>\" Literal[\"2.0\"]]]"
    );
}

// ---- EXPR-9 ----

#[test]
fn a_sequence_holds_values_or_pairs() {
    // EXPR-9: "`[a]` has one value entry; `[a: b]` has one pair entry...
    // Empty brackets have no entries."
    assert_eq!(statements("[]"), "ExprStmt[SequenceExpr[\"[\" \"]\"]]");
    assert_eq!(
        statements("[a]"),
        "ExprStmt[SequenceExpr[\"[\" NameExpr[\"a\"] \"]\"]]"
    );
    assert_eq!(
        statements("[a: b, c: d]"),
        "ExprStmt[SequenceExpr[\"[\" SequencePair[NameExpr[\"a\"] \":\" NameExpr[\"b\"]] \",\" \
         SequencePair[NameExpr[\"c\"] \":\" NameExpr[\"d\"]] \"]\"]]"
    );
    assert_eq!(
        statements("[1s: 7]"),
        "ExprStmt[SequenceExpr[\"[\" SequencePair[Literal[\"1s\"] \":\" Literal[\"7\"]] \"]\"]]"
    );
    // `_` leaves an entry out.
    assert_eq!(
        statements("[1.0, _, 3.0,]"),
        "ExprStmt[SequenceExpr[\"[\" Literal[\"1.0\"] \",\" Hole[\"_\"] \",\" Literal[\"3.0\"] \",\" \"]\"]]"
    );
}

#[test]
fn a_sequence_that_mixes_values_and_pairs_is_rejected() {
    // EXPR-9: "Entries are either all values or all colon-separated
    // pairs; mixing these forms is a source error."
    assert_eq!(
        rejected(Entry::Statements, "[a, b: c]"),
        ["EXPR-9 1:5: a sequence holds values or pairs, not both"]
    );
    assert_eq!(
        rejected(Entry::Statements, "[a: b, c, d]"),
        ["EXPR-9 1:8: a sequence holds values or pairs, not both"]
    );
}

// ---- EXPR-10 ----

#[test]
fn named_braces_after_an_operator_are_its_configuration() {
    // EXPR-10: "A symbol operator may carry named configuration
    // immediately after its token: `a /{zero: ieee} b`... A named-entry
    // brace in this position is configuration, not the right operand's
    // block."
    assert_eq!(
        statements("a /{zero: ieee} b"),
        "ExprStmt[BinaryExpr[NameExpr[\"a\"] \"/\" \
         OperatorConfig[\"{\" ConfigArg[\"zero\" \":\" NameExpr[\"ieee\"]] \"}\"] NameExpr[\"b\"]]]"
    );
    assert_eq!(
        statements("a =={abs: 1e-9, rel: 1e-6,} b"),
        "ExprStmt[BinaryExpr[NameExpr[\"a\"] \"==\" OperatorConfig[\"{\" \
         ConfigArg[\"abs\" \":\" Literal[\"1e-9\"]] \",\" ConfigArg[\"rel\" \":\" Literal[\"1e-6\"]] \",\" \"}\"] \
         NameExpr[\"b\"]]]"
    );
    assert_eq!(
        statements("-{wrap: true} a"),
        "ExprStmt[UnaryExpr[\"-\" OperatorConfig[\"{\" ConfigArg[\"wrap\" \":\" Literal[\"true\"]] \"}\"] \
         NameExpr[\"a\"]]]"
    );
    // A brace that holds statements is the operand.
    assert_eq!(
        statements("a / { b }"),
        "ExprStmt[BinaryExpr[NameExpr[\"a\"] \"/\" Block[\"{\" ExprStmt[NameExpr[\"b\"]] \"}\"]]]"
    );
    assert_eq!(
        statements("a / {}"),
        "ExprStmt[BinaryExpr[NameExpr[\"a\"] \"/\" Block[\"{\" \"}\"]]]"
    );
}

#[test]
fn a_configuration_goes_where_its_operator_goes() {
    // LEX-5 with EXPR-10: a binary operator continues a line, with its
    // configuration; a prefix operator does not, with or without one.
    for (text, plain) in [
        ("a /\n    {zero: ieee} b", "a /{zero: ieee} b"),
        ("a /{zero: ieee}\n    b", "a /{zero: ieee} b"),
        ("a\n    /{zero: ieee} b", "a /{zero: ieee} b"),
    ] {
        same_tree(Entry::Statements, text, plain);
    }
    // The braces on the next line are then a statement of their own.
    assert_eq!(
        rejected(Entry::Statements, "let x = !\n{mode: 1} true")[0],
        "- 1:10: expected an expression, found end of line"
    );
    assert_eq!(
        rejected(Entry::Statements, "let x = !{mode: 1}\ntrue"),
        ["- 1:19: expected an expression, found end of line"]
    );
}

#[test]
fn an_empty_operator_configuration_is_rejected() {
    // EXPR-10: "Reject empty configuration".
    assert_eq!(
        rejected(Entry::Statements, "a /{} b"),
        ["EXPR-10 1:4: an operator configuration has at least one entry"]
    );
}

// ---- FN-8 ----

#[test]
fn a_pack_parameter_has_a_positional_and_a_named_form() {
    // FN-8: "Positional packs may be homogeneous or use a declared type
    // pack; named packs retain source names."
    assert_eq!(
        declarations("fn f(first: i64, rest: ...i64, named: ...{str}) {}"),
        "FnDecl[\"fn\" \"f\" ParamList[\"(\" Param[\"first\" \":\" ScalarType[\"i64\"]] \",\" \
         Param[\"rest\" \":\" \"...\" ScalarType[\"i64\"]] \",\" \
         Param[\"named\" \":\" \"...\" PackType[\"{\" ScalarType[\"str\"] \"}\"]] \")\"] Block[\"{\" \"}\"]]"
    );
    assert_eq!(
        declarations("fn f<...Ts>(items: ...Ts) {}"),
        "FnDecl[\"fn\" \"f\" GenericParams[\"<\" GenericParam[\"...\" \"Ts\"] \">\"] \
         ParamList[\"(\" Param[\"items\" \":\" \"...\" NamedType[\"Ts\"]] \")\"] Block[\"{\" \"}\"]]"
    );
    // "Packs have no defaults, cannot be const, and follow all fixed
    // parameters" is the analyser's part of the rule: the forms parse.
    let outcome = run(
        Entry::Declarations,
        "fn f(const rest: ...i64 = 1, last: i64) {}",
    );
    assert_eq!(outcome.findings, []);
}

// ---- FN-10 ----

#[test]
fn an_arrow_body_runs_to_its_enclosing_delimiter_or_line_end() {
    // FN-10: "An arrow body consumes the complete expression up to its
    // enclosing delimiter or significant newline. A comma at the
    // surrounding argument/list level ends the body; a comma in a nested
    // expression does not."
    assert_eq!(
        statements("apply(fn(x) => x + 1, 7)"),
        "ExprStmt[CallExpr[NameExpr[\"apply\"] ArgList[\"(\" Arg[FnExpr[\"fn\" \
         ParamList[\"(\" LambdaParam[\"x\"] \")\"] ArrowBody[\"=>\" BinaryExpr[NameExpr[\"x\"] \"+\" Literal[\"1\"]]]]] \
         \",\" Arg[Literal[\"7\"]] \")\"]]]"
    );
    assert_eq!(
        statements("f(fn(x) => (x, 1), 2)"),
        "ExprStmt[CallExpr[NameExpr[\"f\"] ArgList[\"(\" Arg[FnExpr[\"fn\" \
         ParamList[\"(\" LambdaParam[\"x\"] \")\"] ArrowBody[\"=>\" \
         TupleExpr[\"(\" NameExpr[\"x\"] \",\" Literal[\"1\"] \")\"]]]] \",\" Arg[Literal[\"2\"]] \")\"]]]"
    );
    assert_eq!(
        statements("let f = fn(x) => x + 1\ng()"),
        "LocalDecl[\"let\" \"f\" \"=\" FnExpr[\"fn\" ParamList[\"(\" LambdaParam[\"x\"] \")\"] \
         ArrowBody[\"=>\" BinaryExpr[NameExpr[\"x\"] \"+\" Literal[\"1\"]]]]] \
         ExprStmt[CallExpr[NameExpr[\"g\"] ArgList[\"(\" \")\"]]]"
    );
    // "Operators and calls within that expression belong to the body."
    assert_eq!(
        statements("fn(x) => x(7)"),
        "ExprStmt[FnExpr[\"fn\" ParamList[\"(\" LambdaParam[\"x\"] \")\"] ArrowBody[\"=>\" \
         CallExpr[NameExpr[\"x\"] ArgList[\"(\" Arg[Literal[\"7\"]] \")\"]]]]]"
    );
}

#[test]
fn an_anonymous_function_is_parenthesized_before_it_is_used() {
    // FN-10: "Parenthesize an anonymous function before calling it or
    // applying an operation to the function itself."
    assert_eq!(
        statements("(fn(x) => x + 1)(7)"),
        "ExprStmt[CallExpr[ParenExpr[\"(\" FnExpr[\"fn\" ParamList[\"(\" LambdaParam[\"x\"] \")\"] \
         ArrowBody[\"=>\" BinaryExpr[NameExpr[\"x\"] \"+\" Literal[\"1\"]]]] \")\"] \
         ArgList[\"(\" Arg[Literal[\"7\"]] \")\"]]]"
    );
    let message = "an anonymous function is parenthesized before it is called or operated on";
    // Called, or the left operand: reported at what is applied.
    assert_eq!(
        rejected(Entry::Statements, "fn(x) { return x }(7)"),
        [format!("FN-10 1:19: {message}")]
    );
    assert_eq!(
        rejected(Entry::Statements, "let f = fn(x) { return x } + 1"),
        [format!("FN-10 1:28: {message}")]
    );
    // The right operand, or a prefix operator's: reported at the function.
    assert_eq!(
        rejected(Entry::Statements, "let x = 1 + fn() { return 2 }"),
        [format!("FN-10 1:13: {message}")]
    );
    assert_eq!(
        rejected(Entry::Statements, "let x = 1 + fn(y) => y"),
        [format!("FN-10 1:13: {message}")]
    );
    assert_eq!(
        rejected(Entry::Statements, "let x = !fn() { return true }"),
        [format!("FN-10 1:10: {message}")]
    );
    // Parenthesized, or a whole value, it is accepted.
    for text in [
        "let x = 1 + (fn() { return 2 })",
        "let x = !(fn() { return true })",
        "let f = fn(y) => y",
        "f = fn(y) { return y }",
        "apply(fn(y) => y, with: fn(z) { return z })",
        "return fn(y) => y + 1",
        "[fn(y) => y, fn(z) => z]",
    ] {
        statements(text);
    }
}

// ---- GEN-8 ----

#[test]
fn an_underscore_keeps_an_instantiate_parameter_open() {
    // GEN-8: "`_` there retains a generic parameter".
    assert_eq!(
        declarations("instantiate convert<_, f64>, add<i64, 3>,"),
        "InstantiateDecl[\"instantiate\" \
         Instantiation[\"convert\" GenericArgs[\"<\" Hole[\"_\"] \",\" ScalarType[\"f64\"] \">\"]] \",\" \
         Instantiation[\"add\" GenericArgs[\"<\" ScalarType[\"i64\"] \",\" Literal[\"3\"] \">\"]] \",\"]"
    );
    // Only there: generic arguments elsewhere have no `_`.
    assert_eq!(
        rejected(Entry::Statements, "var x: Box<_>"),
        ["- 1:12: expected a type or a value, found `_`"]
    );
}

// ---- GEN-14 ----

#[test]
fn angle_brackets_after_a_name_in_a_type_are_generic_arguments() {
    // GEN-14: "In a type position... `<` opens generic arguments rather
    // than a comparison."
    assert_eq!(
        statements("var a: Box<Box<i64>>"),
        "LocalDecl[\"var\" \"a\" \":\" NamedType[\"Box\" GenericArgs[\"<\" \
         NamedType[\"Box\" GenericArgs[\"<\" ScalarType[\"i64\"] \">\"]] \">\"]]]"
    );
}

#[test]
fn a_constructor_that_cannot_be_a_comparison_is_a_constructor() {
    // GEN-14, as far as the parser can take it: when the tokens read only
    // as generic arguments and a call, that is what they are.
    assert_eq!(
        statements("Box<i64>(value: 7)"),
        "ExprStmt[TypedConstructor[\"Box\" GenericArgs[\"<\" ScalarType[\"i64\"] \">\"] \
         ArgList[\"(\" Arg[\"value\" \":\" Literal[\"7\"]] \")\"]]]"
    );
    // A named argument cannot stand in parentheses on its own.
    assert_eq!(
        statements("Box<T>(value: 7)"),
        "ExprStmt[TypedConstructor[\"Box\" GenericArgs[\"<\" NamedType[\"T\"] \">\"] \
         ArgList[\"(\" Arg[\"value\" \":\" Literal[\"7\"]] \")\"]]]"
    );
    assert_eq!(
        statements("m::Box<i64>(1)"),
        "ExprStmt[TypedConstructor[QualifiedName[\"m\" \"::\" \"Box\"] GenericArgs[\"<\" ScalarType[\"i64\"] \">\"] \
         ArgList[\"(\" Arg[Literal[\"1\"]] \")\"]]]"
    );
    // Two comparisons need a comma that separates; a statement has none.
    assert_eq!(
        statements("a < b, c > (d)"),
        "ExprStmt[TypedConstructor[\"a\" GenericArgs[\"<\" NamedType[\"b\"] \",\" NamedType[\"c\"] \">\"] \
         ArgList[\"(\" Arg[NameExpr[\"d\"]] \")\"]]]"
    );
}

#[test]
fn a_comparison_that_cannot_be_a_constructor_is_a_comparison() {
    // GEN-14: with no `>` and `(` to close and call, `<` compares.
    assert_eq!(
        statements("a < b"),
        "ExprStmt[BinaryExpr[NameExpr[\"a\"] \"<\" NameExpr[\"b\"]]]"
    );
    assert_eq!(
        statements("a < b > c"),
        "ExprStmt[BinaryExpr[BinaryExpr[NameExpr[\"a\"] \"<\" NameExpr[\"b\"]] \">\" NameExpr[\"c\"]]]"
    );
    assert_eq!(
        statements("a < b && c > d"),
        "ExprStmt[BinaryExpr[BinaryExpr[NameExpr[\"a\"] \"<\" NameExpr[\"b\"]] \"&&\" \
         BinaryExpr[NameExpr[\"c\"] \">\" NameExpr[\"d\"]]]]"
    );
    // Generic arguments open on the name's line (LEX-5): a `<` that
    // starts a line is a binary operator continuing the line before.
    assert_eq!(
        statements("let x = a\n< b > (c)"),
        "LocalDecl[\"let\" \"x\" \"=\" BinaryExpr[BinaryExpr[NameExpr[\"a\"] \"<\" NameExpr[\"b\"]] \">\" \
         ParenExpr[\"(\" NameExpr[\"c\"] \")\"]]]"
    );
}

#[test]
fn each_name_in_a_list_is_judged_on_its_own() {
    // GEN-14: a comparison before a constructor in one list. The failed
    // reading of `a <` as generic arguments does not spoil `c<d>(e)`.
    assert_eq!(
        statements("f(a < b, c<i64>(e))"),
        "ExprStmt[CallExpr[NameExpr[\"f\"] ArgList[\"(\" \
         Arg[BinaryExpr[NameExpr[\"a\"] \"<\" NameExpr[\"b\"]]] \",\" \
         Arg[TypedConstructor[\"c\" GenericArgs[\"<\" ScalarType[\"i64\"] \">\"] \
         ArgList[\"(\" Arg[NameExpr[\"e\"]] \")\"]]] \")\"]]]"
    );
    assert_eq!(
        statements("f(a < b, c < d, e < g)"),
        "ExprStmt[CallExpr[NameExpr[\"f\"] ArgList[\"(\" \
         Arg[BinaryExpr[NameExpr[\"a\"] \"<\" NameExpr[\"b\"]]] \",\" \
         Arg[BinaryExpr[NameExpr[\"c\"] \"<\" NameExpr[\"d\"]]] \",\" \
         Arg[BinaryExpr[NameExpr[\"e\"] \"<\" NameExpr[\"g\"]]] \")\"]]]"
    );
    // Nested generic arguments close one after another.
    assert_eq!(
        statements("Outer<Inner<i64>, f64>(x)"),
        "ExprStmt[TypedConstructor[\"Outer\" GenericArgs[\"<\" \
         NamedType[\"Inner\" GenericArgs[\"<\" ScalarType[\"i64\"] \">\"]] \",\" ScalarType[\"f64\"] \">\"] \
         ArgList[\"(\" Arg[NameExpr[\"x\"]] \")\"]]]"
    );
}

#[test]
fn tokens_that_read_both_ways_are_marked_undecided() {
    // GEN-14 decides by what the name is, which the analyser knows. The
    // parser records its guess, the constructor, in a node of its own
    // (owner decision of 2026-11-13, `docs/architecture.md`).
    assert_eq!(
        statements("a < b > (c)"),
        "ExprStmt[UndecidedConstructor[\"a\" GenericArgs[\"<\" NamedType[\"b\"] \">\"] \
         ArgList[\"(\" Arg[NameExpr[\"c\"]] \")\"]]]"
    );
    assert_eq!(
        statements("f(a < b, c > (d))"),
        "ExprStmt[CallExpr[NameExpr[\"f\"] ArgList[\"(\" Arg[UndecidedConstructor[\"a\" \
         GenericArgs[\"<\" NamedType[\"b\"] \",\" NamedType[\"c\"] \">\"] \
         ArgList[\"(\" Arg[NameExpr[\"d\"]] \")\"]]] \")\"]]]"
    );
    // A generic argument may be any constant expression, so a range
    // check with a parenthesized bound reads both ways as well.
    assert_eq!(
        statements("a < b && c > (d)"),
        "ExprStmt[UndecidedConstructor[\"a\" GenericArgs[\"<\" \
         BinaryExpr[NameExpr[\"b\"] \"&&\" NameExpr[\"c\"]] \">\"] ArgList[\"(\" Arg[NameExpr[\"d\"]] \")\"]]]"
    );
    // The node keeps every token, so the analyser can re-read it.
    let outcome = run(Entry::Statements, "f(a < b, c > (d))");
    let undecided = outcome
        .tree
        .descendants()
        .find(|node| node.kind() == SyntaxKind::UndecidedConstructor)
        .expect("an undecided node");
    assert_eq!(undecided.text().to_string(), "a < b, c > (d)");
}

#[test]
fn a_greater_than_inside_generic_arguments_is_parenthesized() {
    // GEN-14: "At the current generic nesting level, `>` closes the
    // argument list; parenthesize a greater-than expression used as a
    // constant argument."
    assert_eq!(
        statements("var x: Size<(a > b)>"),
        "LocalDecl[\"var\" \"x\" \":\" NamedType[\"Size\" GenericArgs[\"<\" \
         ParenExpr[\"(\" BinaryExpr[NameExpr[\"a\"] \">\" NameExpr[\"b\"]] \")\"] \">\"]]]"
    );
    // Other relations need no parentheses.
    assert_eq!(
        statements("var x: Size<N >= 3, M + 1>"),
        "LocalDecl[\"var\" \"x\" \":\" NamedType[\"Size\" GenericArgs[\"<\" \
         BinaryExpr[NameExpr[\"N\"] \">=\" Literal[\"3\"]] \",\" \
         BinaryExpr[NameExpr[\"M\"] \"+\" Literal[\"1\"]] \">\"]]]"
    );
    // Unparenthesized, the list ends at the first `>`.
    assert_eq!(
        rejected(Entry::Statements, "var x: Size<a > b>"),
        [
            "LEX-6 1:17: expected a line break, found `b`",
            "- 1:19: expected an expression, found end of file"
        ]
    );
}

// ---- GEN-15, GEN-16 ----

#[test]
fn constraints_bind_negation_then_conjunction_then_disjunction() {
    // GEN-15.
    assert_eq!(
        declarations("fn f() requires p(A) || q(A) && !r(A) {}"),
        "FnDecl[\"fn\" \"f\" ParamList[\"(\" \")\"] RequiresClause[\"requires\" \
         ConstraintBinary[ConstraintCall[\"p\" \"(\" NamedType[\"A\"] \")\"] \"||\" \
         ConstraintBinary[ConstraintCall[\"q\" \"(\" NamedType[\"A\"] \")\"] \"&&\" \
         ConstraintNot[\"!\" ConstraintCall[\"r\" \"(\" NamedType[\"A\"] \")\"]]]]] Block[\"{\" \"}\"]]"
    );
    assert_eq!(
        declarations("fn f() requires (p(A) || q(A)) && r(A) {}"),
        "FnDecl[\"fn\" \"f\" ParamList[\"(\" \")\"] RequiresClause[\"requires\" \
         ConstraintBinary[ConstraintParen[\"(\" ConstraintBinary[ConstraintCall[\"p\" \"(\" NamedType[\"A\"] \")\"] \
         \"||\" ConstraintCall[\"q\" \"(\" NamedType[\"A\"] \")\"]] \")\"] \"&&\" \
         ConstraintCall[\"r\" \"(\" NamedType[\"A\"] \")\"]]] Block[\"{\" \"}\"]]"
    );
}

#[test]
fn a_constraint_call_has_one_form_with_an_optional_result() {
    // GEN-16.
    assert_eq!(
        declarations("fn f() requires comparable(T) && m::converts(A, list<B>) -> C {}"),
        "FnDecl[\"fn\" \"f\" ParamList[\"(\" \")\"] RequiresClause[\"requires\" ConstraintBinary[\
         ConstraintCall[\"comparable\" \"(\" NamedType[\"T\"] \")\"] \"&&\" \
         ConstraintCall[QualifiedName[\"m\" \"::\" \"converts\"] \"(\" NamedType[\"A\"] \",\" \
         ListType[\"list\" \"<\" NamedType[\"B\"] \">\"] \")\" ReturnType[\"->\" NamedType[\"C\"]]]]] \
         Block[\"{\" \"}\"]]"
    );
    // Relations and `each` are the other terms of the grammar.
    assert_eq!(
        declarations("fn f() requires type(N) in {i64, f64} && N == 3 && T is Shape {}"),
        "FnDecl[\"fn\" \"f\" ParamList[\"(\" \")\"] RequiresClause[\"requires\" ConstraintBinary[ConstraintBinary[\
         ConstraintRelation[CallExpr[NameExpr[\"type\"] ArgList[\"(\" Arg[NameExpr[\"N\"]] \")\"]] \"in\" \
         ConstraintSet[\"{\" ScalarType[\"i64\"] \",\" ScalarType[\"f64\"] \"}\"]] \"&&\" \
         ConstraintRelation[NamedType[\"N\"] \"==\" Literal[\"3\"]]] \"&&\" \
         ConstraintRelation[NamedType[\"T\"] \"is\" NamedType[\"Shape\"]]]] Block[\"{\" \"}\"]]"
    );
    assert_eq!(
        declarations("fn f<...Ts>() requires each T in Ts { numeric(T) } {}"),
        "FnDecl[\"fn\" \"f\" GenericParams[\"<\" GenericParam[\"...\" \"Ts\"] \">\"] ParamList[\"(\" \")\"] \
         RequiresClause[\"requires\" ConstraintEach[\"each\" \"T\" \"in\" NamedType[\"Ts\"] \"{\" \
         ConstraintCall[\"numeric\" \"(\" NamedType[\"T\"] \")\"] \"}\"]] Block[\"{\" \"}\"]]"
    );
}

#[test]
fn a_constraint_relation_has_an_operand_on_each_side() {
    // The grammar: `constraint_relation = constraint_operand,
    // ( "==" | "in" | "is" ), constraint_operand ;`. A missing operand is
    // not made up for by what follows: here the function's own body.
    assert_eq!(
        rejected(Entry::Declarations, "fn f<T>() requires T == => 1"),
        ["- 1:25: expected a type or a value, found `=>`"]
    );
    assert_eq!(
        rejected(Entry::Declarations, "fn f<T>() requires T in => 1"),
        ["- 1:25: expected a type or a value, found `=>`"]
    );
    assert_eq!(
        rejected(Entry::Declarations, "fn f<T>() requires T is => 1"),
        ["- 1:25: expected a type or a value, found `=>`"]
    );
    assert_eq!(
        rejected(Entry::Declarations, "fn f() requires == 3 {}"),
        ["- 1:17: expected a constraint, found `==`"]
    );
    assert_eq!(
        rejected(Entry::Declarations, "fn f<...Ts>() requires each T in => 1"),
        ["- 1:34: expected a type or a value, found `=>`"]
    );
    assert_eq!(
        rejected(Entry::Declarations, "fn f() requires p(T,) {}"),
        ["- 1:21: expected a type or a value, found `)`"]
    );
}

#[test]
fn a_constraint_follows_the_line_rules_of_an_expression() {
    // LEX-5: `&&`, `||` and a relation's operator continue a line, before
    // and after; `!` does not.
    for (text, plain) in [
        (
            "fn f() requires p(T) &&\n    q(T) {}",
            "fn f() requires p(T) && q(T) {}",
        ),
        (
            "fn f() requires p(T)\n    || q(T) {}",
            "fn f() requires p(T) || q(T) {}",
        ),
        (
            "fn f() requires T in\n    {i64, f64} {}",
            "fn f() requires T in {i64, f64} {}",
        ),
        (
            "fn f() requires T\n    == U {}",
            "fn f() requires T == U {}",
        ),
    ] {
        same_tree(Entry::Declarations, text, plain);
    }
    assert_eq!(
        rejected(Entry::Declarations, "fn f<T>() requires !\np(T) => 1")[0],
        "- 1:21: expected a constraint, found end of line"
    );
    assert_eq!(
        rejected(
            Entry::Declarations,
            "fn f<...Ts>() requires each\nT in Ts { p(T) } {}"
        )[0],
        "- 1:28: expected `==`, `in`, `is` or `(`, found end of line"
    );
}

#[test]
fn only_the_identity_property_has_a_value() {
    // The grammar: `operator_property = "associative" | "commutative"
    // | "identity", "=", const_expression ;`
    assert_eq!(
        declarations(
            "operator add(a: i64) -> i64 properties<i64, f64> { associative, identity = 0, }"
        ),
        "OperatorDecl[\"operator\" \"add\" ParamList[\"(\" Param[\"a\" \":\" ScalarType[\"i64\"]] \")\"] \
         ReturnType[\"->\" ScalarType[\"i64\"]] OperatorProperties[\"properties\" \"<\" ScalarType[\"i64\"] \",\" \
         ScalarType[\"f64\"] \">\" \"{\" OperatorProperty[\"associative\"] \",\" \
         OperatorProperty[\"identity\" \"=\" Literal[\"0\"]] \",\" \"}\"]]"
    );
    let header = "operator add(a: i64) -> i64 properties<i64>";
    assert_eq!(
        rejected(
            Entry::Declarations,
            &format!("{header} {{ associative = 42 }}")
        ),
        ["- 1:59: expected `,` or `}`, found `=`"]
    );
    assert_eq!(
        rejected(
            Entry::Declarations,
            &format!("{header} {{ commutative = true, identity = 0 }}")
        ),
        ["- 1:59: expected `,` or `}`, found `=`"]
    );
    assert_eq!(
        rejected(Entry::Declarations, &format!("{header} {{ identity }}")),
        ["- 1:56: expected `=`, found `}`"]
    );
    assert_eq!(
        rejected(Entry::Declarations, &format!("{header} {{ idempotent }}")),
        ["- 1:47: expected `associative`, `commutative` or `identity`, found `idempotent`"]
    );
}

// ---- CTRL-9 ----

#[test]
fn the_first_brace_after_when_or_while_opens_the_body() {
    // CTRL-9: "After `when` or `while`, the first `{` outside
    // parentheses, brackets, and operator configuration (EXPR-10) opens
    // the body. The expression before it, if any, is the condition."
    // The specification's three illustrations:
    let step = "Block[\"{\" ExprStmt[CallExpr[NameExpr[\"step\"] ArgList[\"(\" \")\"]]] \"}\"]";
    assert_eq!(
        statements("when { step() }"),
        format!("WhenStmt[\"when\" {step}]")
    );
    assert_eq!(
        statements("when ready { step() }"),
        format!("WhenStmt[\"when\" NameExpr[\"ready\"] {step}]")
    );
    assert_eq!(
        statements("when ({ ready }) { step() }"),
        format!(
            "WhenStmt[\"when\" ParenExpr[\"(\" Block[\"{{\" ExprStmt[NameExpr[\"ready\"]] \"}}\"] \")\"] {step}]"
        )
    );
    // `while` is alike.
    assert_eq!(
        statements("while { step() }"),
        format!("WhileStmt[\"while\" {step}]")
    );
    assert_eq!(
        statements("while running { step() }"),
        format!("WhileStmt[\"while\" NameExpr[\"running\"] {step}]")
    );
    // Brackets and operator configuration hold braces of their own.
    assert_eq!(
        statements("when [{ a }][0] { step() }"),
        format!(
            "WhenStmt[\"when\" IndexExpr[SequenceExpr[\"[\" Block[\"{{\" ExprStmt[NameExpr[\"a\"]] \"}}\"] \"]\"] \
             \"[\" Literal[\"0\"] \"]\"] {step}]"
        )
    );
    assert_eq!(
        statements("while a /{zero: error} b { step() }"),
        format!(
            "WhileStmt[\"while\" BinaryExpr[NameExpr[\"a\"] \"/\" \
             OperatorConfig[\"{{\" ConfigArg[\"zero\" \":\" NameExpr[\"error\"]] \"}}\"] NameExpr[\"b\"]] {step}]"
        )
    );
}

#[test]
fn a_brace_in_a_condition_is_parenthesized() {
    // CTRL-9's rejection: `when { ready } { step() }`. "The first block
    // is the body, so a second statement follows on its line (LEX-6)."
    assert_eq!(
        rejected(Entry::Statements, "when { ready } { step() }"),
        ["LEX-6 1:16: expected a line break, found `{`"]
    );
    // "A condition that contains any other brace is parenthesized."
    let message = "a condition that contains a brace is parenthesized";
    assert_eq!(
        rejected(Entry::Statements, "when if a { b } else { c } { step() }"),
        [format!("CTRL-9 1:6: {message}")]
    );
    assert_eq!(
        rejected(Entry::Statements, "when fn() { return 1 } { step() }"),
        [format!("CTRL-9 1:11: {message}")]
    );
    assert_eq!(
        rejected(Entry::Statements, "when a == { b } { c }"),
        ["CTRL-9 1:11: expected an expression, found `{`"]
    );
    // Parenthesized, each is accepted.
    for text in [
        "when (if a { b } else { c }) { step() }",
        "when f(fn() { return 1 }) { step() }",
        "when a == ({ b }) { c }",
    ] {
        statements(text);
    }
}

// ---- NATIVE-3 ----

#[test]
fn a_native_function_has_no_body_or_a_contract_body() {
    // NATIVE-3: "A temporal implementation `{}` denotes a graph;
    // `{ when; }` denotes a node... A missing body is a declaration, not
    // an empty implementation." The parser keeps the three forms apart;
    // what each means is the analyser's part of the rule.
    assert_eq!(
        declarations("native fn accumulate(value: i64) -> i64"),
        "NativeFnDecl[\"native\" \"fn\" \"accumulate\" ParamList[\"(\" Param[\"value\" \":\" ScalarType[\"i64\"]] \")\"] \
         ReturnType[\"->\" ScalarType[\"i64\"]]]"
    );
    assert_eq!(
        declarations("native const fn describe() -> str {}"),
        "NativeFnDecl[\"native\" \"const\" \"fn\" \"describe\" ParamList[\"(\" \")\"] \
         ReturnType[\"->\" ScalarType[\"str\"]] NativeContractBody[\"{\" \"}\"]]"
    );
    assert_eq!(
        declarations(
            "native fn node() -> i64 {\n    inject out\n    start;\n    when;\n    stop;\n}"
        ),
        "NativeFnDecl[\"native\" \"fn\" \"node\" ParamList[\"(\" \")\"] ReturnType[\"->\" ScalarType[\"i64\"]] \
         NativeContractBody[\"{\" InjectDecl[\"inject\" \"out\"] NativeHook[\"start\" \";\"] \
         NativeHook[\"when\" \";\"] NativeHook[\"stop\" \";\"] \"}\"]]"
    );
    // The grammar lets the markers share a line, and `inject` take a `;`.
    assert_eq!(
        declarations("native fn node() -> i64 { inject out, clock; when; }"),
        "NativeFnDecl[\"native\" \"fn\" \"node\" ParamList[\"(\" \")\"] ReturnType[\"->\" ScalarType[\"i64\"]] \
         NativeContractBody[\"{\" InjectDecl[\"inject\" \"out\" \",\" \"clock\"] \";\" NativeHook[\"when\" \";\"] \"}\"]]"
    );
    assert_eq!(
        declarations("export native type Handle\nnative type Other"),
        "NativeTypeDecl[\"export\" \"native\" \"type\" \"Handle\"] NativeTypeDecl[\"native\" \"type\" \"Other\"]"
    );
}

#[test]
fn a_contract_body_holds_no_statements() {
    // NATIVE-3: the body states a contract; the grammar admits `inject`
    // and hook markers in it and nothing else.
    assert_eq!(
        rejected(
            Entry::Declarations,
            "native fn node() -> i64 {\n    let x = 1\n    when;\n}"
        ),
        ["NATIVE-3 2:5: expected `inject`, `start;`, `when;` or `stop;`, found `let`"]
    );
    assert_eq!(
        rejected(
            Entry::Declarations,
            "native fn node() -> i64 {\n    start { }\n}"
        ),
        ["NATIVE-3 2:5: expected `inject`, `start;`, `when;` or `stop;`, found `start`"]
    );
    // A hook marker is its word and its `;`, on one line.
    assert_eq!(
        rejected(
            Entry::Declarations,
            "native fn node() -> i64 {\n    when\n    ;\n}"
        )[0],
        "NATIVE-3 2:5: expected `inject`, `start;`, `when;` or `stop;`, found `when`"
    );
}

// ---- ERR-10 ----

#[test]
fn a_warning_annotation_stands_before_a_named_function() {
    // ERR-10: "At a named function, `@suppress_warning(
    // "node.no_activation")` acknowledges the dormant-node warning for
    // that declaration".
    assert_eq!(
        declarations("@suppress_warning(\"node.no_activation\")\nfn quiet() {}"),
        "FnDecl[WarningAnnotation[\"@\" \"suppress_warning\" \"(\" \"\\\"node.no_activation\\\"\" \")\"] \
         \"fn\" \"quiet\" ParamList[\"(\" \")\"] Block[\"{\" \"}\"]]"
    );
    assert_eq!(
        declarations("@suppress_warning(\"a\")\n@suppress_warning(\"b\")\nexport fn f() {}"),
        "FnDecl[WarningAnnotation[\"@\" \"suppress_warning\" \"(\" \"\\\"a\\\"\" \")\"] \
         WarningAnnotation[\"@\" \"suppress_warning\" \"(\" \"\\\"b\\\"\" \")\"] \
         \"export\" \"fn\" \"f\" ParamList[\"(\" \")\"] Block[\"{\" \"}\"]]"
    );
}

#[test]
fn a_misplaced_warning_annotation_is_rejected() {
    // ERR-10, with the grammar: `{ warning_annotation, NL }` before a
    // function declaration.
    assert_eq!(
        rejected(
            Entry::Declarations,
            "@suppress_warning(\"a\") fn quiet() {}"
        ),
        ["ERR-10 1:24: expected a line break, found `fn`"]
    );
    assert_eq!(
        rejected(Entry::Declarations, "@suppress_warning(\"a\")\nstruct S {}"),
        ["ERR-10 2:1: expected a function declaration, found `struct`"]
    );
    assert_eq!(
        rejected(Entry::Declarations, "@other(\"a\")\nfn f() {}"),
        ["ERR-10 1:2: expected `suppress_warning`, found `other`"]
    );
    // An anonymous function has no declaration to annotate.
    assert_eq!(
        rejected(
            Entry::Statements,
            "@suppress_warning(\"a\")\nlet f = fn() => 1"
        ),
        ["- 1:1: expected a statement, found `@`"]
    );
}

// ---- TST-1 ----

#[test]
fn a_test_context_holds_helpers_types_and_named_tests() {
    // TST-1: "Unnamed contexts contribute private fn/const-fn helpers,
    // unexported struct/enum types, and named tests".
    assert_eq!(
        declarations(
            "test {\n    fn helper() => 1\n    const fn two() => 2\n    struct S {}\n    \
             enum E { a }\n    test named { assert true }\n}"
        ),
        "TestContext[\"test\" \"{\" FnDecl[\"fn\" \"helper\" ParamList[\"(\" \")\"] ArrowBody[\"=>\" Literal[\"1\"]]] \
         FnDecl[\"const\" \"fn\" \"two\" ParamList[\"(\" \")\"] ArrowBody[\"=>\" Literal[\"2\"]]] \
         StructDecl[\"struct\" \"S\" \"{\" \"}\"] EnumDecl[\"enum\" \"E\" \"{\" EnumMember[\"a\"] \"}\"] \
         TestDecl[\"test\" \"named\" Block[\"{\" AssertStmt[\"assert\" Literal[\"true\"]] \"}\"]] \"}\"]"
    );
    assert_eq!(
        declarations("test { fn helper() => 1 }"),
        "TestContext[\"test\" \"{\" FnDecl[\"fn\" \"helper\" ParamList[\"(\" \")\"] ArrowBody[\"=>\" Literal[\"1\"]]] \"}\"]"
    );
}

#[test]
fn a_test_context_admits_nothing_else() {
    // TST-1: "Nested contexts, imports, native declarations... are not
    // admitted here." Exports and operator implementations are the
    // analyser's part of the rule.
    let message = "a test context holds functions, structs, enums and named tests";
    for item in [
        "use a.b::{c}",
        "test { }",
        "native fn n() -> i64",
        "native type Handle",
        "operator add(a: i64) -> i64",
        "instantiate add<i64>",
    ] {
        assert_eq!(
            rejected(Entry::Declarations, &format!("test {{\n    {item}\n}}")),
            [format!("TST-1 2:5: {message}")],
            "{item}"
        );
    }
    // Each is parsed all the same.
    let outcome = run(
        Entry::Declarations,
        "test {\n    use a.b::{c}\n    fn f() {}\n}",
    );
    assert!(!outcome.has_error_node());
}

// ---- SDOC-1 ----

/// The kinds of the nodes of `text` whose first token is a documentation
/// comment: the declarations it documents.
fn documented(entry: Entry, text: &str) -> Vec<SyntaxKind> {
    let outcome = run(entry, text);
    assert_eq!(outcome.findings, [], "`{text}`\n{}", outcome.report);
    outcome
        .tree
        .descendants()
        .filter(|node| {
            node.first_token()
                .is_some_and(|token| token.kind() == SyntaxKind::DocComment)
                // The comment belongs to the innermost node that starts
                // with it, and the root starts with everything.
                && node.kind() != outcome.tree.kind()
        })
        .map(|node| node.kind())
        .collect()
}

#[test]
fn a_documentation_comment_documents_the_declaration_after_it() {
    // SDOC-1: "`/** ... */` immediately before a module, function/native
    // function, operator, struct, field, or test documents that
    // declaration."
    let text = "/** The module. */\nmodule a\n\
                /** A function. */\nexport const fn f() => 1\n\
                /** A native. */\nnative fn n() -> i64\n\
                /** An operator. */\noperator add(a: i64) -> i64\n\
                /** A struct. */\nstruct Quote {\n    /** A field. */\n    bid: f64\n}\n\
                /** A test. */\ntest t { assert true }\n";
    assert_eq!(
        documented(Entry::SourceFile, text),
        [
            SyntaxKind::ModuleDecl,
            SyntaxKind::FnDecl,
            SyntaxKind::NativeFnDecl,
            SyntaxKind::OperatorDecl,
            SyntaxKind::StructDecl,
            SyntaxKind::StructMember,
            SyntaxKind::TestDecl,
        ]
    );
    // "Only whitespace may intervene": blank lines do; an annotation is
    // part of the function.
    assert_eq!(
        documented(Entry::Declarations, "/** Doc. */\n\n\nfn f() {}"),
        [SyntaxKind::FnDecl]
    );
    assert_eq!(
        documented(
            Entry::Declarations,
            "/** Doc. */\n@suppress_warning(\"a\")\nfn f() {}"
        ),
        [SyntaxKind::FnDecl]
    );
}

#[test]
fn a_documentation_comment_that_documents_nothing_is_rejected() {
    // SDOC-1: "Ordinary comments remain comments. Repeated/unattached
    // documentation or a body comment claiming a later declaration is
    // rejected."
    let unattached = "documentation comment documents no declaration";
    assert_eq!(
        rejected(Entry::Declarations, "/** One. */\n/** Two. */\nfn f() {}"),
        ["SDOC-1 1:1: a declaration has one documentation comment"]
    );
    assert_eq!(
        rejected(Entry::Declarations, "/** Doc. */\n# note\nfn f() {}"),
        [format!("SDOC-1 1:1: {unattached}")]
    );
    assert_eq!(
        rejected(
            Entry::Declarations,
            "fn f() {\n    /** In a body. */\n    return 1\n}"
        ),
        [format!("SDOC-1 2:5: {unattached}")]
    );
    assert_eq!(
        rejected(Entry::Declarations, "fn f() {}\n/** At the end. */\n"),
        [format!("SDOC-1 2:1: {unattached}")]
    );
    // The rule lists what can be documented. An enum is not in the list
    // (reported to the specification's issue tracker).
    assert_eq!(
        rejected(Entry::Declarations, "/** Sides. */\nenum Side { buy }"),
        [format!("SDOC-1 1:1: {unattached}")]
    );
    // Its codes are this compiler's: the catalogue has none for it.
    let outcome = run(
        Entry::Declarations,
        "/** One. */\n/** Two. */\nfn f() {}\n/** End. */",
    );
    let codes: Vec<&str> = outcome
        .findings
        .iter()
        .map(|finding| finding.code.as_str())
        .collect();
    assert_eq!(codes, ["doc.repeated", "doc.unattached"]);
}
