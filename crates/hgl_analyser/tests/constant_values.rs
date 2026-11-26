//! The analyser against what a compile-time constant is. UNIT-13: "A
//! compile-time constant (GEN-8), a literal or a constant among them, is
//! one value of the type its place gives it (EXPR-5) ... Two
//! compile-time constants are one where those values are the same,
//! however each is written" (and CTRL-6, TYPE-2, TYPE-7, TYPE-17). Each
//! line of the two tables of hhenson/hgl-compiler#211, which found where
//! the compiler did not, is here twice: the program that must be
//! rejected, and the one beside it that must not.
//!
//! A finding is written `line code`: the line of `m.hgl`, counted from
//! the `module` line, and the catalogued code.

mod common;

use common::{Checked, check};
use hgl_analyser::hir::{Callee, Expr, ExprKind, Stmt, Value, walk_statements};
use hgl_analyser::{Argument, ConstantValue, Detail, Type};

/// Checks one file, `m.hgl`, of module `m`. The `module` line is added,
/// so the text given starts on line 2.
fn module(text: &str) -> Checked {
    check(&[("m.hgl", &format!("module m\n{text}"))])
}

/// Each finding of `checked` as `line code`.
fn found(checked: &Checked) -> Vec<String> {
    checked
        .summary()
        .iter()
        .map(|finding| {
            let mut parts = finding.split(' ');
            let at = parts.next().unwrap_or_default();
            let code = parts.next().unwrap_or_default();
            let line = at.split(':').nth(1).unwrap_or_default();
            let code = code.rsplit('/').next().unwrap_or_default();
            format!("{line} {code}")
        })
        .collect()
}

/// Asserts what `text`, a module's declarations after `module m`, is
/// rejected for: each finding as `line code`.
fn expect(text: &str, findings: &[&str]) {
    let checked = module(text);
    assert_eq!(found(&checked), findings, "{text}\n{}", checked.report);
}

/// The declarations the cases below share: line 2 to line 15.
const SHARED: &str = concat!(
    "enum Mode { fast, slow }\n",
    "enum Shade { slow, dark }\n",
    "const ONE: i64 = 1\n",
    "const ZERO: i64 = 0\n",
    "const HALF: f64 = 0.5\n",
    "struct Tagged<const t: Mode> { v: i64 }\n",
    "struct IBox<const X: i64> { v: i64 }\n",
    "struct FBox<const X: f64> { v: i64 }\n",
    "struct SBox<const X: str> { v: i64 }\n",
    "struct DBox<const X: duration> { v: i64 }\n",
    "struct TBox<const X: tenor> { v: i64 }\n",
    "const fn real<const scale: f64>(value: i64) -> i64 requires scale == 1.0 => value\n",
    "const fn moded<const mode: Mode>(value: i64) -> i64 requires mode == Mode.fast => value\n",
    "const fn mk<const mode: Mode>() -> Tagged<mode> => Tagged<mode>(v: 1)\n",
);

/// The shared declarations, then `case` from line 16.
fn with_shared(case: &str) -> String {
    format!("{SHARED}{case}")
}

/// Issue 211, "Wrongly accepted", first line. GEN-8: a generic value
/// argument is a compile-time constant, and TYPE-3: "Generic
/// applications are invariant by origin and complete canonical
/// arguments": two applications whose constants differ in value are two
/// types, and a constant of another type than its parameter's is none of
/// its values (EXPR-5, `type.mismatch`).
#[test]
fn a_generic_value_argument_is_compared_by_its_value() {
    expect(
        &with_shared(concat!(
            "const fn a(t: Tagged<.slow>) -> Tagged<.fast> => t\n",
            "const fn b(t: FBox<2.0>) -> FBox<1.0> => t\n",
            "const fn c(t: IBox<\"a\">) -> IBox<2> => t\n",
            "const fn d(t: Tagged<Shade.slow>) -> i64 => 1\n",
        )),
        &[
            "16 type.mismatch",
            "17 type.mismatch",
            "18 type.mismatch",
            "19 type.mismatch",
        ],
    );
    expect(
        &with_shared(concat!(
            "const fn a(t: Tagged<.slow>) -> Tagged<Mode.slow> => t\n",
            "const fn b(t: FBox<1.00>) -> FBox<1.0> => t\n",
            "const fn c(t: IBox<0x2>) -> IBox<2> => t\n",
            "const fn d(t: Tagged<(Mode.slow)>) -> Tagged<Mode(1)> => t\n",
        )),
        &[],
    );
}

/// Issue 211: "A value in parentheses is not read". EXPR-1:
/// "Parentheses select grouping". GEN-3: a call "whose `requires` does
/// not hold with the generic arguments the call resolves is rejected".
#[test]
fn a_value_in_parentheses_is_the_value() {
    let shared = concat!(
        "struct Box<const X: f64> { v: i64 }\n",
        "const fn real<const X: f64>(given: Box<X>) -> i64 requires X == 1.0 => 1\n",
    );
    expect(
        &format!("{shared}const fn r(given: Box<(2.0)>) -> i64 => real(given)\n"),
        &["4 requires.unmet"],
    );
    expect(
        &format!("{shared}const fn r(given: Box<(1.0)>) -> i64 => real(given)\n"),
        &[],
    );
}

/// Issue 211: "An enum member written with its enum". TYPE-7: an enum
/// member is one however it is written; GEN-6: "A call admits the
/// candidate only where that holds for the parameter's value".
#[test]
fn a_member_written_with_its_enum_is_that_member() {
    expect(
        &with_shared("const fn a() -> i64 => moded<.slow>(1)\n"),
        &["16 requires.unmet"],
    );
    expect(
        &with_shared("const fn a() -> i64 => moded<.fast>(1)\n"),
        &[],
    );
}

/// Issue 211: "Tenors are not compared". TYPE-17: "Two tenors are equal
/// when they are of the same form and their counts are equal, however
/// each was written: `1Y` is `12M`".
#[test]
fn tenors_are_compared_by_their_counts() {
    expect(
        &with_shared("const fn a(t: TBox<1Y>) -> TBox<2Y> => t\n"),
        &["16 type.mismatch"],
    );
    expect(
        &with_shared("const fn a(t: TBox<1Y>) -> TBox<12M> => t\n"),
        &[],
    );
}

/// Issue 211: "One duration written two ways". TYPE-16: "A delta that
/// names one member, key or position both as added or updated and as
/// removed is malformed. A constant one is rejected (`delta.overlap`)."
#[test]
fn one_duration_written_two_ways_is_one_key() {
    expect(
        "const fn d() -> delta<set<duration>> => delta<set<duration>>(added: [60s], removed: [1m])\n",
        &["2 delta.overlap"],
    );
    expect(
        "const fn d() -> delta<set<duration>> => delta<set<duration>>(added: [61s], removed: [1m])\n",
        &[],
    );
}

/// Issue 211: "A default of another type or another enum". GEN-13: the
/// default of `const N: V` is a V; TYPE-7: a member of another enum is
/// no value of this one.
#[test]
fn a_default_of_another_type_is_rejected() {
    expect(
        &with_shared(concat!(
            "const fn a<const n: i64 = 1Y>(v: i64) -> i64 => v\n",
            "const fn b<const mode: Mode = Shade.slow>(v: i64) -> i64 => v\n",
            "const fn c<const mode: Mode = Shade(0)>(v: i64) -> i64 => v\n",
            "const fn d<const n: i64 = @2024-01-01>(v: i64) -> i64 => v\n",
        )),
        &[
            "16 type.mismatch",
            "17 type.mismatch",
            "18 type.mismatch",
            "19 type.mismatch",
        ],
    );
    expect(
        &with_shared(concat!(
            "const fn a<const n: i64 = 12>(v: i64) -> i64 => v\n",
            "const fn b<const mode: Mode = Mode.slow>(v: i64) -> i64 => v\n",
            "const fn c<const mode: Mode = Mode(0)>(v: i64) -> i64 => v\n",
            "const fn d<const n: f64 = 1 + 1>(v: i64) -> i64 => v\n",
        )),
        &[],
    );
}

/// Issue 211: "A constant the compiler cannot value is taken as known".
/// GEN-8: "a constant expression of those" is a compile-time constant:
/// `3.0 - 1.0` is 2.0 (NUM-1), and `Mode(ONE)` the member numbered 1
/// (TYPE-7). One this compiler does not work out, a symbol given
/// configuration, is rejected as no constant it has the value of, and
/// is never taken as the same as another.
#[test]
fn a_constant_not_valued_is_not_known() {
    expect(
        &with_shared(concat!(
            "const fn a() -> i64 => real<3.0 - 1.0>(1)\n",
            "const fn b() -> i64 => moded<Mode(ONE)>(1)\n",
            "const fn c() -> i64 => real<-{twice: true} 2.0>(1)\n",
        )),
        &[
            "16 requires.unmet",
            "17 requires.unmet",
            "18 generic.argument_constant",
        ],
    );
    expect(
        &with_shared(concat!(
            "const fn a() -> i64 => real<3.0 - 2.0>(1)\n",
            "const fn b() -> i64 => moded<Mode(ZERO)>(1)\n",
            "const fn c() -> i64 => real<-(-1.0)>(1)\n",
        )),
        &[],
    );
}

/// Issue 211: "An `f64` constant of one module against the same name in
/// another". UNIT-13: "The constant has that one value wherever it is
/// read": `a::C` and `b::C` are two constants, of two values.
#[test]
fn a_constant_of_another_module_is_its_own_value() {
    let files = |use_line: &str| {
        check(&[
            (
                "m.hgl",
                &format!(
                    "module m\nuse n as a\nuse o as b\nstruct FBox<const X: f64> {{ v: i64 }}\n\
                     const fn need<T>(x: T) -> i64 requires T == FBox<a::C> => 1\n{use_line}"
                ),
            ),
            ("n.hgl", "module n\nexport const C: f64 = 1.5\n"),
            ("o.hgl", "module o\nexport const C: f64 = 2.5\n"),
        ])
    };
    let rejected = files("const fn uses(v: FBox<b::C>) -> i64 => need(v)\n");
    assert_eq!(
        found(&rejected),
        ["6 requires.unmet"],
        "{}",
        rejected.report
    );
    let accepted = files("const fn uses(v: FBox<1.5>) -> i64 => need(v)\n");
    assert_eq!(found(&accepted), [""; 0], "{}", accepted.report);
}

/// Issue 211, "Wrongly rejected, or unstable": "A member or a constant
/// named in a default, or after the callee's name, compares unequal".
/// GEN-8: a call gives a generic value parameter by name, or after the
/// callee's name, the same value either way.
#[test]
fn a_member_given_either_way_is_one_value() {
    let need = "const fn need<T>(x: T) -> i64 requires T == Tagged<.slow> => 1\n";
    expect(
        &with_shared(&format!(
            "{need}const fn a() -> i64 => need(mk(mode: .fast))\n\
             const fn b() -> i64 => need(mk<Mode.fast>())\n"
        )),
        &["17 requires.unmet", "18 requires.unmet"],
    );
    expect(
        &with_shared(&format!(
            "{need}const fn a() -> i64 => need(mk(mode: .slow))\n\
             const fn b() -> i64 => need(mk<Mode.slow>())\n\
             const fn c() -> i64 => need(mk(mode: Mode(\"slow\")))\n"
        )),
        &[],
    );
}

/// Issue 211: "A comment inside a constant changes the result". LEX: a
/// comment is no token, and a constant is its value: `- /* c */ 1.0` is
/// `-1.0` wherever it is written.
#[test]
fn a_comment_inside_a_constant_changes_nothing() {
    let need = "const fn need<T>(x: T) -> i64 requires T == FBox<-1.0> => 1\n";
    expect(
        &with_shared(&format!(
            "{need}const fn a(given: FBox<- /* c */ 2.0>) -> i64 => need(given)\n\
             const fn b() -> i64 => need(mk_real(X: - /* c */ 2.0))\n\
             const fn mk_real<const X: f64>() -> FBox<X> => FBox<X>(v: 1)\n"
        )),
        &["17 requires.unmet", "18 requires.unmet"],
    );
    expect(
        &with_shared(&format!(
            "{need}const fn a(given: FBox<- /* c */ 1.0>) -> i64 => need(given)\n\
             const fn b() -> i64 => need(mk_real(X: - /* c */ 1.0))\n\
             const fn mk_real<const X: f64>() -> FBox<X> => FBox<X>(v: 1)\n"
        )),
        &[],
    );
}

/// Issue 211: "One value written two ways": `1.0` and `1.00`, `"\u{61}"`
/// and `"a"` (LEX-2: a string is its characters).
#[test]
fn one_value_written_two_ways_is_one_value() {
    let need = concat!(
        "const fn f<T>(x: T) -> i64 requires T == FBox<1.0> => 1\n",
        "const fn s<T>(x: T) -> i64 requires T == SBox<\"a\"> => 1\n",
    );
    expect(
        &with_shared(&format!(
            "{need}const fn a(given: FBox<1.01>) -> i64 => f(given)\n\
             const fn b(given: SBox<\"\\u{{62}}\">) -> i64 => s(given)\n"
        )),
        &["18 requires.unmet", "19 requires.unmet"],
    );
    expect(
        &with_shared(&format!(
            "{need}const fn a(given: FBox<1.00>) -> i64 => f(given)\n\
             const fn b(given: SBox<\"\\u{{61}}\">) -> i64 => s(given)\n"
        )),
        &[],
    );
}

/// UNIT-13: "A compile-time constant ... is one value of the type its
/// place gives it (EXPR-5): `1` where an `f64` is expected is `1.0`. Two
/// compile-time constants are one where those values are the same".
/// EXPR-5: "An integer literal is an `f64` where one is expected", so
/// `FBox<1>` is `FBox<1.0>`; an `i64` constant is not converted by its
/// place, and is no `f64`.
#[test]
fn an_integer_literal_where_a_float_is_expected_is_the_float() {
    expect(
        &with_shared(concat!(
            "const fn a(given: FBox<1>) -> FBox<1.0> => given\n",
            "const fn b() -> i64 => real<1>(1)\n",
            "const fn c() -> i64 => real(1, scale: 1)\n",
        )),
        &[],
    );
    expect(
        &with_shared(concat!(
            "const fn a(given: FBox<2>) -> FBox<1.0> => given\n",
            "const fn b() -> i64 => real<2>(1)\n",
            "const fn c(given: FBox<ONE>) -> i64 => 1\n",
        )),
        &["16 type.mismatch", "17 requires.unmet", "18 type.mismatch"],
    );
    // One value, so one key: the target writes one instance for both.
    let checked = module(&with_shared(
        "const fn a(x: FBox<1>, y: FBox<1.0>, z: FBox<1.00>) -> i64 => 1\n",
    ));
    let keys: Vec<Option<String>> = parameter_types(&checked, "a")
        .iter()
        .map(|ty| match ty {
            Type::Named { arguments, .. } => match arguments.as_slice() {
                [Argument::Constant(constant)] => constant.value.key(),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(keys, vec![Some("1.0".to_string()); 3]);
}

/// UNIT-13: "How a constant is written is kept with it, for diagnostics
/// and for a formatter, which writes it back as written, and is never
/// compared": a constant keeps how it is written beside its value, and a
/// message shows it so.
#[test]
fn a_constant_keeps_how_it_is_written() {
    let checked = module(&with_shared(
        "const fn a(given: FBox<1.00>) -> FBox<2.0> => given\n",
    ));
    assert_eq!(found(&checked), ["16 type.mismatch"]);
    assert_eq!(
        checked.findings[0].message,
        "this is `FBox<1.00>`, and the function's result is `FBox<2.0>`"
    );
    let types = parameter_types(&checked, "a");
    let Type::Named { arguments, .. } = &types[0] else {
        panic!("{types:?}");
    };
    let [Argument::Constant(constant)] = arguments.as_slice() else {
        panic!("{arguments:?}");
    };
    assert_eq!(constant.text, "1.00");
    assert!(matches!(constant.value, ConstantValue::Float(value) if value == 1.0));
}

/// TYPE-2: a `datetime` is a "UTC-instant", and UNIT-13: two are one
/// constant as one instant, "the `datetime`s `@2026-09-03T09:30Z` and
/// `@2026-09-03T10:30+01:00`, one instant". A `zoned_datetime` is an
/// instant in a named zone: "Same-instant zoned values need not be
/// equal; zone aliases are distinct names".
#[test]
fn a_datetime_is_its_instant_and_a_zoned_one_its_zone_too() {
    let shared = concat!(
        "const fn at<const t: datetime>(v: i64) -> i64 requires t == @2024-01-01T00:00Z => v\n",
        "const fn zoned<const t: zoned_datetime>(v: i64) -> i64\n",
        "    requires t == @2024-01-01T01:00+01:00[Europe/Paris] => v\n",
    );
    expect(
        &format!(
            "{shared}const fn a() -> i64 => at<@2024-01-01T01:00+01:00>(1)\n\
             const fn b() -> i64 => zoned<@2024-01-01T00:00+00:00[Europe/Paris]>(1)\n"
        ),
        &[],
    );
    expect(
        &format!(
            "{shared}const fn a() -> i64 => at<@2024-01-01T01:00Z>(1)\n\
             const fn b() -> i64 => zoned<@2024-01-01T01:00+01:00[Europe/Berlin]>(1)\n"
        ),
        &["5 requires.unmet", "6 requires.unmet"],
    );
}

/// CTRL-6: "Switch labels are typed source constants, compared by
/// resolved value. Duplicate values reject, including differently
/// spelled equal enum members."
#[test]
fn labels_are_compared_by_value_however_written() {
    let switch = |labels: [&str; 2], selector: &str| {
        format!(
            "{SHARED}const SLOW: Mode = Mode(1)\n\
             const fn s(x: {selector}) -> i64 {{\n    switch x {{\n        case {}: return 1\n        \
             case {}: return 2\n        default: return 0\n    }}\n}}\n",
            labels[0], labels[1]
        )
    };
    for (labels, selector) in [
        (["Mode(ONE)", ".slow"], "Mode"),
        (["SLOW", "Mode.slow"], "Mode"),
        (
            ["-9223372036854775808", "-(9223372036854775807) - 1"],
            "i64",
        ),
        (["\"a\"", "\"\\u{61}\""], "str"),
        (["!false", "true"], "bool"),
    ] {
        expect(&switch(labels, selector), &["20 switch.duplicate_label"]);
    }
    for (labels, selector) in [
        (["Mode(ZERO)", ".slow"], "Mode"),
        (["ONE + 1", "1"], "i64"),
        (["\"a\"", "\"\\u{62}\""], "str"),
    ] {
        expect(&switch(labels, selector), &[]);
    }
}

/// Every spelling of one value of a type is that value, and so one
/// constant: `Box<first>` is `Box<second>` for each two (GEN-8, EXPR-5,
/// LEX-2, LEX-7, LEX-22, LEX-23, TYPE-2, TYPE-7, TYPE-17, NUM-1). A
/// spelling of another value is another type.
#[test]
fn every_spelling_of_one_value_is_one_constant() {
    let spellings: [(&str, &[&str], &str); 9] = [
        (
            "i64",
            &[
                "16",
                "0x10",
                "0b1_0000",
                "1_6",
                "(16)",
                "8 + 8",
                "2 * 8",
                "-(-16)",
                "SIXTEEN",
                "33 // 2",
                "-16 % 32",
                "TWICE * 8",
            ],
            "17",
        ),
        (
            "f64",
            &[
                "1.0",
                "1.00",
                "1e0",
                "1",
                "(1.0)",
                "0.5 + 0.5",
                "2.0 / 2.0",
                "-(-1.0)",
                "HALF + HALF",
                "10.0e-1",
                "4 / 4",
            ],
            "1.5",
        ),
        (
            "str",
            &["\"a\"", "\"\\u{61}\"", "(\"a\")", "LETTER", "\"\" + \"a\""],
            "\"b\"",
        ),
        (
            "bool",
            &[
                "true",
                "(true)",
                "!false",
                "YES",
                "1 < 2",
                "\"a\" < \"b\"",
                "false < true",
            ],
            "false",
        ),
        (
            "duration",
            &[
                "60s",
                "1m",
                "60_000ms",
                "30s + 30s",
                "2 * 30s",
                "30s * 2",
                "MINUTE",
            ],
            "61s",
        ),
        (
            "tenor",
            &["1Y", "12M", "(1Y)", "6M + 6M", "2 * 6M", "-(-1Y)"],
            "13M",
        ),
        (
            "Mode",
            &[
                ".slow",
                "Mode.slow",
                "(Mode.slow)",
                "Mode(1)",
                "Mode(\"slow\")",
                "SLOW",
                "Mode(ONE)",
            ],
            ".fast",
        ),
        (
            "datetime",
            &[
                "@2024-01-01T00:00Z",
                "@2024-01-01T00:00:00Z",
                "@2024-01-01T01:00+01:00",
                "@2023-12-31T23:00-01:00",
                "(@2024-01-01T00:00Z)",
            ],
            "@2024-01-01T00:01Z",
        ),
        (
            "date",
            &["@2024-01-01", "(@2024-01-01)", "NEW_YEAR"],
            "@2024-01-02",
        ),
    ];
    let constants = concat!(
        "enum Mode { fast, slow }\n",
        "const ONE: i64 = 1\n",
        "const SIXTEEN: i64 = 16\n",
        "const TWICE: i64 = 2\n",
        "const HALF: f64 = 0.5\n",
        "const LETTER: str = \"a\"\n",
        "const YES: bool = true\n",
        "const MINUTE: duration = 1m\n",
        "const SLOW: Mode = .slow\n",
        "const NEW_YEAR: date = @2024-01-01\n",
    );
    for (ty, written, other) in spellings {
        let mut text = format!("{constants}struct Box<const X: {ty}> {{ v: i64 }}\n");
        for (index, first) in written.iter().enumerate() {
            for second in written {
                text.push_str(&format!(
                    "const fn same_{index}_{}(given: Box<{first}>) -> Box<{second}> => given\n",
                    text.lines().count()
                ));
            }
        }
        expect(&text, &[]);
        let differs = format!(
            "{constants}struct Box<const X: {ty}> {{ v: i64 }}\n\
             const fn other(given: Box<{}>) -> Box<{other}> => given\n",
            written[0]
        );
        expect(&differs, &["13 type.mismatch"]);
    }
}

/// GEN-13: a generic value parameter read as a value is, in an
/// instance, the constant the call gives it, a list, a string set or a tuple among
/// them: each element as the value it is of the element's type (EXPR-5:
/// the `1` of a `tuple<f64, str>` is `1.0`).
#[test]
fn a_composite_constant_is_its_elements_in_an_instance() {
    let checked = module(concat!(
        "const fn values<const X: list<i64>>() -> list<i64> => X\n",
        "const fn pair<const P: tuple<f64, str>>() -> tuple<f64, str> => P\n",
        "const fn listed() -> list<i64> => values<[1, 0x2]>()\n",
        "const fn paired() -> tuple<f64, str> => pair<(1, \"\\u{61}\")>()\n",
        "const excluded: set<str> = [\"key\", \"index\"]\n",
        "const fn names<const N: set<str> = [\"key\", \"index\"]>() -> set<str> => N\n",
        "const fn by_default() -> set<str> => names()\n",
        "const fn by_configuration() -> set<str> => names{N: [\"index\", \"key\"]}()\n",
        "const fn by_name() -> set<str> => names{N: excluded}()\n",
        "const fn empty() -> set<str> => names{N: []}()\n",
    ));
    assert_eq!(found(&checked), [""; 0], "{}", checked.report);
    let returned = |name: &str| -> ExprKind {
        let module = checked.program.module("m").expect("the module exists");
        let index = module
            .declarations
            .iter()
            .position(|declaration| declaration.name == name)
            .expect("the function exists");
        let body = module.bodies[index].as_ref().expect("a body");
        let mut called = None;
        walk_statements(&body.statements, &mut |value: &Expr| {
            if let ExprKind::Call {
                callee:
                    Callee::Function {
                        module,
                        declaration,
                        generics,
                        requirements,
                        ..
                    },
                ..
            } = &value.kind
            {
                called.get_or_insert((
                    *module,
                    *declaration,
                    generics.clone(),
                    requirements.clone(),
                ));
            }
        });
        let (module, declaration, generics, requirements) = called.expect("a call");
        let (_, instance) = checked
            .program
            .instance(module, declaration, &generics, &requirements)
            .expect("the callee has a body");
        let Some(Stmt::Return {
            value: Some(value), ..
        }) = instance.statements.first()
        else {
            panic!("a return with a value");
        };
        value.kind.clone()
    };
    let literals = |kind: ExprKind| -> Vec<ExprKind> {
        match kind {
            ExprKind::Sequence(entries) | ExprKind::Tuple(entries) => {
                entries.into_iter().map(|entry| entry.kind).collect()
            }
            other => panic!("{other:?}"),
        }
    };
    assert_eq!(
        literals(returned("listed")),
        [
            ExprKind::Literal(Value::Int(1)),
            ExprKind::Literal(Value::Int(2))
        ]
    );
    assert_eq!(
        literals(returned("paired")),
        [
            ExprKind::Literal(Value::Float(1.0)),
            ExprKind::Literal(Value::Str("a".to_string()))
        ]
    );
    for name in ["by_default", "by_configuration", "by_name"] {
        assert_eq!(
            literals(returned(name)),
            [
                ExprKind::Literal(Value::Str("index".to_string())),
                ExprKind::Literal(Value::Str("key".to_string())),
            ]
        );
    }
    assert!(literals(returned("empty")).is_empty());
}

/// GEN-8 and UNIT-13: the same members in different written orders
/// give one constant, hence one type (TYPE-3).
#[test]
fn a_string_set_is_identified_by_its_members() {
    let checked = module(concat!(
        "struct Tagged<const N: set<str>> { marker: i64 }\n",
        "const fn same(names: Tagged<[\"key\", \"index\"]>) -> Tagged<[\"index\", \"key\"]> => names\n",
        "const fn different(names: Tagged<[\"key\", \"index\"]>) -> Tagged<[\"key\"]> => names\n",
    ));
    assert_eq!(found(&checked), ["4 type.mismatch"], "{}", checked.report);
}

/// EXPR-9: repeated constant strings reject at the later member,
/// including in a default or an explicit operator configuration.
#[test]
fn a_string_set_rejects_repeated_constant_members() {
    let checked = module(concat!(
        "operator choose<const N: set<str> = [\"default\"]>() -> i64\n",
        "impl const fn choose<const N: set<str>>() -> i64 => 11\n",
        "const fn duplicate_override() -> i64 => choose{N: [\"key\", \"\\u{6b}ey\"]}()\n",
        "const fn duplicate_fallback<const N: set<str> = [\"key\", \"key\"]>() -> i64 => 0\n",
        "const repeated: set<str> = [\"key\", \"key\"]\n",
        "const fn literal_with_runtime(name: str) -> set<str> => [\"key\", name, \"key\"]\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:4:59 type/set.duplicate_member EXPR-9",
            "m.hgl:5:57 type/set.duplicate_member EXPR-9",
            "m.hgl:6:36 type/set.duplicate_member EXPR-9",
            "m.hgl:7:71 type/set.duplicate_member EXPR-9",
        ],
        "{}",
        checked.report
    );
}

/// A list and a tuple constant are their elements (GEN-8: "a constant
/// expression of those"), each read as a value of the element's type,
/// under a `?` too (TYPE-14, EXPR-5): `[1]` where a `list<f64>?` is
/// expected is `[1.0]`. A generic value parameter among the elements is
/// given its value where the declaration is called (GEN-2), and says
/// what it is where an argument's type gives it one.
#[test]
fn a_composite_constant_is_compared_element_by_element() {
    let shared = concat!(
        "struct Box<const X: list<i64>> { v: i64 }\n",
        "struct NBox<const X: list<f64>?> { v: i64 }\n",
        "struct TBox<const X: tuple<f64, i64>?> { v: i64 }\n",
        "const fn mk<const N: i64>() -> Box<[N]> => Box<[N]>(v: 1)\n",
        "const fn take<const N: i64>(given: Box<[N, 1]>) -> i64 requires N == 5 => N\n",
    );
    expect(
        &format!(
            "{shared}const fn a() -> Box<[2]> => mk<2>()\n\
             const fn c(x: NBox<[1]>) -> NBox<[1.0]> => x\n\
             const fn e(x: TBox<(1, 2)>) -> TBox<(1.0, 0x2)> => x\n\
             const fn f() -> i64 => take(Box<[5, 1]>(v: 1))\n"
        ),
        &[],
    );
    expect(
        &format!(
            "{shared}const fn a() -> Box<[3]> => mk<2>()\n\
             const fn c(x: NBox<null>) -> NBox<[1.0]> => x\n\
             const fn e(x: TBox<(1, 2)>) -> TBox<(2, 1)> => x\n\
             const fn f() -> i64 => take(Box<[4, 1]>(v: 1))\n"
        ),
        &[
            "7 type.mismatch",
            "8 type.mismatch",
            "9 type.mismatch",
            "10 requires.unmet",
        ],
    );
}

/// TYPE-7: a member constructed from a generic value parameter,
/// `Side(N)`, is the member the number given names, once it is given
/// one. TYPE-10: "Size participates in identity": a list constant of
/// another length than the size expected is of another type.
#[test]
fn a_constructed_member_and_a_sized_list_are_held_to_their_types() {
    let shared = concat!(
        "enum Side { buy, sell }\n",
        "struct Tagged<const s: Side> { v: i64 }\n",
        "struct Sized<const X: list<i64, 2>> { v: i64 }\n",
        "const fn mk<const N: i64>() -> Tagged<Side(N)> => Tagged<Side(N)>(v: 1)\n",
    );
    expect(
        &format!(
            "{shared}const fn a() -> Tagged<.sell> => mk<1>()\n\
             const fn c(x: Sized<[1, 2]>) -> i64 => 1\n"
        ),
        &[],
    );
    expect(
        &format!(
            "{shared}const fn a() -> Tagged<.buy> => mk<1>()\n\
             const fn c(x: Sized<[1, 2, 3]>) -> i64 => 1\n"
        ),
        &["6 type.mismatch", "7 type.mismatch"],
    );
}

/// NUM-1, NUM-3: `/` of two `i64` is an `f64` already, so where an
/// `f64` is expected its literals are not widened (GEN-5), and the
/// quotient is rounded once. NUM-2: "Mixed numeric comparison must not
/// first round an i64 operand to f64". A constant has the value its
/// expression has when the program runs.
#[test]
fn integer_division_and_mixed_comparison_are_exact() {
    let shared = concat!(
        "struct FBox<const X: f64> { v: i64 }\n",
        "struct BBox<const B: bool> { v: i64 }\n",
        "const QUOTIENT: f64 = 9007199254740993 / 3\n",
    );
    expect(
        &format!(
            "{shared}const fn a(x: FBox<9007199254740993 / 3>) -> FBox<3002399751580331.0> => x\n\
             const fn b(x: BBox<9007199254740993 == 9007199254740992.0>) -> BBox<false> => x\n\
             const fn c(x: FBox<QUOTIENT>) -> FBox<3002399751580331.0> => x\n"
        ),
        &[],
    );
    // `//` and `%` of two `i64` give an `i64`: where an `f64` is
    // expected their literals are `f64`s (GEN-5).
    expect(
        &format!(
            "{shared}const fn d(x: FBox<9007199254740993 // 1>) -> FBox<9007199254740992.0> => x\n\
                  const fn e(x: FBox<-7 % 2>) -> FBox<1.0> => x\n"
        ),
        &[],
    );
    expect(
        &format!(
            "{shared}const fn a(x: FBox<9007199254740993 / 3>) -> FBox<3002399751580330.5> => x\n\
             const fn b(x: BBox<9007199254740993 == 9007199254740992.0>) -> BBox<true> => x\n"
        ),
        &["5 type.mismatch", "6 type.mismatch"],
    );
}

/// TYPE-10, TIME-2 and CTRL-13: a graph's local fixed extent cannot
/// take a connection. Type-only queries and ordinary construction-time
/// values retain their existing behavior.
#[test]
fn a_graph_locals_list_size_cannot_be_a_connection() {
    expect(
        concat!(
            "fn direct(size: i64) -> i64 {\n",
            "    var held: list<i64, size>\n",
            "    return 0\n",
            "}\n",
            "fn computed(size: i64) -> i64 {\n",
            "    let held: list<i64, size + 1> = [1, 2]\n",
            "    return 0\n",
            "}\n",
            "fn nested(size: i64) -> i64 {\n",
            "    var held: tuple<str, list<i64, size>>\n",
            "    return 0\n",
            "}\n",
            "fn aliased(size: i64) -> i64 {\n",
            "    let extent = size + 1\n",
            "    var held: list<i64, (extent)>\n",
            "    return 0\n",
            "}\n",
            "fn branched(size: i64) -> i64 {\n",
            "    var held: list<i64, if false { size } else { 2 }>\n",
            "    return 0\n",
            "}\n",
        ),
        &[
            "3 list.size",
            "7 list.size",
            "11 list.size",
            "16 list.size",
            "20 list.size",
        ],
    );
    expect(
        concat!(
            "const LENGTH: i64 = 2\n",
            "fn sized<const N: i64>(const configured: i64, input: i64) -> i64 {\n",
            "    let fixed: list<i64, LENGTH> = [4, 5]\n",
            "    var generic: list<i64, N>\n",
            "    var ordinary: list<i64, configured>\n",
            "    var typed: list<i64, if type(input) == type(0) { 2 } else { 1 }>\n",
            "    return input + fixed[0]\n",
            "}\n",
        ),
        &[],
    );
}

/// TYPE-10 and TIME-2: an immediate call used as a size also reads its
/// lexical captures. A body that is only formed contributes no read.
#[test]
fn a_local_list_size_checks_immediate_anonymous_captures() {
    expect(
        concat!(
            "fn captured(input: i64) -> i64 {\n",
            "    var held: list<i64, (fn(x: i64) => input + x)(3)>\n",
            "    return input\n",
            "}\n",
            "fn passed(input: i64) -> i64 {\n",
            "    var held: list<i64, (fn(x: i64) => x + 3)(input)>\n",
            "    return input\n",
            "}\n",
            "fn nested(input: i64) -> i64 {\n",
            "    var held: list<i64, (fn(x: i64) => (fn(y: i64) => input + y)(x))(3)>\n",
            "    return input\n",
            "}\n",
            "fn local(input: i64) -> i64 {\n",
            "    let length = (fn(x: i64) => input + x)(3)\n",
            "    var held: list<i64, length>\n",
            "    return input\n",
            "}\n",
            "fn selected(input: i64) -> i64 {\n",
            "    let length = (fn(flag: bool) => if flag { input } else { 3 })(true)\n",
            "    var held: list<i64, length>\n",
            "    return input\n",
            "}\n",
            "fn live_choice(input: bool) -> i64 {\n",
            "    let length = (fn() -> i64 {\n",
            "        if input { return 3 }\n",
            "        return 4\n",
            "    })()\n",
            "    var held: list<i64, length>\n",
            "    return 0\n",
            "}\n",
        ),
        &[
            "3 list.size",
            "7 list.size",
            "11 list.size",
            "16 list.size",
            "21 list.size",
            "29 list.size",
        ],
    );
    expect(
        concat!(
            "fn ordinary(input: i64) -> i64 {\n",
            "    let ordinary_call = (fn(x: i64) => input + x)(3)\n",
            "    let count: i64 = 4\n",
            "    var held: list<i64, (fn(x: i64) => count + x)(3)>\n",
            "    return input\n",
            "}\n",
            "fn unused(input: i64) -> i64 {\n",
            "    var held: list<i64, (fn(x: i64) -> i64 {\n",
            "        fn(y: i64) => (fn(z: i64) => input + z)(y)\n",
            "        return x + 3\n",
            "    })(3)>\n",
            "    return input\n",
            "}\n",
            "fn cold(input: i64) -> i64 {\n",
            "    var held: list<i64, (fn(x: i64) => if type((fn(y: i64) => input + y)(x)) == type(0) { 2 } else { 1 })(3)>\n",
            "    return input\n",
            "}\n",
            "fn cold_branch(input: i64) -> i64 {\n",
            "    var held: list<i64, (fn(x: i64) => if false { input + x } else { 3 })(2)>\n",
            "    return input\n",
            "}\n",
            "fn cold_return(input: i64) -> i64 {\n",
            "    let length = (fn(flag: bool) -> i64 {\n",
            "        if flag { return input }\n",
            "        return 3\n",
            "    })(false)\n",
            "    var held: list<i64, length>\n",
            "    return input\n",
            "}\n",
            "fn cold_local(input: i64) -> i64 {\n",
            "    let length = (fn(x: i64) -> i64 {\n",
            "        let flag = false\n",
            "        let selected = if flag { input + x } else { 3 }\n",
            "        return selected\n",
            "    })(2)\n",
            "    var held: list<i64, length>\n",
            "    return input\n",
            "}\n",
            "fn after_return(input: i64) -> i64 {\n",
            "    let length = (fn(flag: bool) -> i64 {\n",
            "        if flag { return 3 }\n",
            "        return input\n",
            "    })(true)\n",
            "    var held: list<i64, length>\n",
            "    return input\n",
            "}\n",
            "fn graph_result(input: i64) -> i64 {\n",
            "    let output = (fn(x: i64) => input + x)(3)\n",
            "    return output\n",
            "}\n",
        ),
        &[],
    );
}

/// TYPE-10, EXPR-3 and FN-17: the immediate call preserves the category
/// of a reached supplied callback, including through a local binding.
#[test]
fn a_local_list_size_checks_immediately_invoked_supplied_callbacks() {
    expect(
        concat!(
            "fn work(f: const fn(i64) -> i64, input: i64) -> i64 {\n",
            "    let length = (fn(x: i64) => f(x))(3)\n",
            "    var held: list<i64, length>\n",
            "    return input\n",
            "}\n",
            "fn cold(f: const fn(i64) -> i64, input: i64) -> i64 {\n",
            "    var held: list<i64, (fn(x: i64) => if false { f(x) } else { 3 })(2)>\n",
            "    return input\n",
            "}\n",
            "fn live(n: i64) -> i64 => work(fn(x) => x + n, n)\n",
            "fn capture_free(n: i64) -> i64 => work(fn(x) => x + 1, n)\n",
            "fn unused(n: i64) -> i64 => cold(fn(x) => x + n, n)\n",
        ),
        &["11 list.size"],
    );
}

/// CTRL-13, CTRL-15 and TYPE-10: a live selection can choose the result
/// of an immediate call even where each return gives an ordinary value.
#[test]
fn a_local_list_size_checks_the_reached_return_of_an_immediate_call() {
    let inline = |body| {
        format!(
            "fn measured(input: i64, const fixed: i64) -> i64 {{\n\
             var held: list<i64, (fn(k: i64) -> i64 {{ {body} }})(2)>\n\
             return input\n}}\n"
        )
    };
    for body in [
        "switch input { case 0: return 3\ndefault: return 4 }",
        "if input > 0 { return 3 }\nreturn 4",
        "while input > 0 { return 3 }\nreturn 4",
        "switch fixed { case 0: return 3\ndefault: return input }",
        "for i in 0..<input { return 3 }\nreturn 4",
        "for i in input..<4 { return 3 }\nreturn 4",
    ] {
        expect(&inline(body), &["3 list.size"]);
    }
    for body in [
        "switch fixed { case 0: return 3\ndefault: return 4 }",
        "let selector = k\nswitch selector { case 0: return input\ndefault: return 3 }",
        "switch k { case 0: return 3\ndefault: return 4 }\nreturn input",
        "if false { return input }\nreturn 4",
        "while false { return input }\nreturn 4",
        "while true { return 3 }\nreturn input",
        "for i in 0..<0 { return input }\nreturn 4",
        "for i in 0..<fixed { return 3 }\nreturn 4",
        "for i in 0..<k { return 3 }\nreturn input",
    ] {
        expect(&inline(body), &[]);
    }
    expect(
        concat!(
            "fn measured(input: i64) -> i64 {\n",
            "    let width = (fn() -> i64 {\n",
            "        switch input { case 0: return 3\n",
            "                       default: return 4 }\n",
            "    })()\n",
            "    var held: list<i64, width>\n",
            "    return input\n",
            "}\n",
        ),
        &["7 list.size"],
    );
    expect(
        concat!(
            "fn measured(input: i64) -> i64 {\n",
            "    let width = (fn() -> i64 {\n",
            "        for i in 0..<input { return 3 }\n",
            "        return 4\n",
            "    })()\n",
            "    var held: list<i64, width>\n",
            "    return input\n",
            "}\n",
        ),
        &["4 connection.as_constant", "7 list.size"],
    );
}

/// EXPR-3, CTRL-14 and TYPE-10: a reached write can supply a later
/// return, including across passes of a loop or via a live case.
#[test]
fn a_local_list_size_checks_reached_writes_to_the_returned_binding() {
    let inline = |body| {
        format!(
            "fn measured(input: i64, const fixed: i64) -> i64 {{\n\
             var held: list<i64, (fn() -> i64 {{ var n = 3\n{body} }})()>\n\
             return input\n}}\n"
        )
    };
    for body in [
        "n = input + 2\nreturn n",
        "if input > 0 { n = 4 }\nreturn n",
        "switch input { case 0: n = 4\ndefault: n = 5 }\nreturn n",
        "while input > n { n += 1 }\nreturn n",
        "var m = 3\nvar i = 0\nwhile i < 2 { n = m\nm = input\ni += 1 }\nreturn n",
    ] {
        expect(&inline(body), &["3 list.size"]);
    }
    for body in [
        "n = fixed + 2\nreturn n",
        "n = input\nreturn 3",
        "switch fixed { case 0: n = 4\ndefault: n = 5 }\nreturn n",
        "while false { n = input }\nreturn n",
        "var m = 3\nvar i = 0\nwhile i < 2 { n = m\nm = fixed\ni += 1 }\nreturn n",
    ] {
        expect(&inline(body), &[]);
    }
}

/// CTRL-15 and UNIT-13: a module constant or checked arithmetic selects
/// one switch case just as its literal does, including through a local.
#[test]
fn a_local_list_size_uses_the_resolved_value_of_a_switch_selector() {
    let program = |selector: &str, label: &str, argument: &str| {
        format!(
            "const INDEX: i64 = 7\n\
             const SIDE: str = \"west\"\n\
             fn measured(input: i64) -> i64 {{\n\
                 let count = (fn(k: i64) -> i64 {{\n\
                     switch {selector} {{ case {label}: return 3\n\
                                         default: return input }}\n\
                 }})({argument})\n\
                 var held: list<i64, count>\n\
                 return input\n\
             }}\n"
        )
    };
    for (selector, label, argument) in [
        ("INDEX", "7", "0"),
        ("INDEX - 1", "6", "0"),
        ("1 + 2", "3", "0"),
        ("k + INDEX", "9", "1 + 1"),
        ("SIDE", "\"west\"", "0"),
        ("SIDE + \"erly\"", "\"westerly\"", "0"),
    ] {
        expect(&program(selector, label, argument), &[]);
    }
    expect(&program("INDEX - 1", "7", "0"), &["9 list.size"]);

    let aliases = |initializer: &str, selector: &str| {
        format!(
            "const INDEX: i64 = 7\n\
             fn measured(input: i64) -> i64 {{\n\
                 let configured = {initializer}\n\
                 let selected = configured\n\
                 let count = (fn(key: i64) -> i64 {{\n\
                     switch {selector} {{ case 7: return 3\n\
                                         default: return input }}\n\
                 }})(selected)\n\
                 var held: list<i64, count>\n\
                 return input\n\
             }}\n"
        )
    };
    for initializer in ["INDEX", "7", "INDEX + 0"] {
        expect(&aliases(initializer, "key"), &[]);
        expect(&aliases(initializer, "selected"), &[]);
    }
    expect(&aliases("INDEX - 1", "key"), &["10 list.size"]);
    expect(&aliases("input", "key"), &["10 list.size"]);
}

/// TYPE-10 and FN-17: a static callback can still capture a connection.
/// Report the instance that supplies the live callback and keep the
/// capture-free and ordinary named callbacks admissible.
#[test]
fn a_local_list_size_rechecks_static_callback_captures() {
    expect(
        concat!(
            "const fn count(x: i64) -> i64 => x + 1\n",
            "fn relay(f: const fn(i64) -> i64, input: i64) -> i64 {\n",
            "    var held: tuple<str, list<i64, f(2)>>\n",
            "    return input\n",
            "}\n",
            "fn forwarded(f: const fn(i64) -> i64, n: i64) -> i64 => relay(f, n)\n",
            "fn captured(n: i64) -> i64 => forwarded(fn(x) => x + n, n)\n",
            "fn direct(n: i64) -> i64 => relay(fn(x) => x + n, n)\n",
            "fn named(n: i64) -> i64 => relay(count, n)\n",
            "fn capture_free(n: i64) -> i64 => relay(fn(x) => x + 1, n)\n",
        ),
        &["8 list.size", "9 list.size"],
    );
}

/// TYPE-10 and GEN-7: the same extent can take its category from the
/// operator candidate chosen for the concretely supplied type.
#[test]
fn a_local_list_size_rechecks_the_selected_operator_category() {
    expect(
        concat!(
            "operator length_of<T>(x: T) -> i64\n",
            "impl const fn length_of(x: i64) -> i64 => 2\n",
            "impl fn length_of(x: str) -> i64 { when modified(x) { return 3 } }\n",
            "fn relay<T>(const x: T, n: i64) -> i64 requires T in {i64, str} {\n",
            "    var held: list<i64, length_of(x)>\n",
            "    return n\n",
            "}\n",
            "fn dynamic(n: i64) -> i64 => relay(\"q\", n)\n",
            "fn ordinary(n: i64) -> i64 => relay(1, n)\n",
        ),
        &["9 list.size"],
    );
}

/// A constant's type may name a constant, as a list's size: the one
/// reader follows it with the constants it is following, so that one
/// whose type names itself is `constant.cycle` and ends (UNIT-13). A
/// list an operator's option is given is read as a value of the type
/// the candidate declares (GEN-6, EXPR-5): `[1]` of a `list<f64>` is
/// `[1.0]`. OPS-2: "`==` and `!=` compare two values of one type", so
/// `1m == 60` is no constant this compiler works out.
#[test]
fn a_constant_in_a_type_an_option_and_an_equality_are_read_in_context() {
    let checked = module("const A: list<i64, A> = []\n");
    assert!(
        found(&checked).contains(&"2 constant.cycle".to_string()),
        "{}",
        checked.report
    );
    let shared = concat!(
        "const B: list<i64, C> = [1]\n",
        "const C: i64 = 1\n",
        "struct Sized<const X: list<i64, 1>> { v: i64 }\n",
        "struct BBox<const F: bool> { v: i64 }\n",
        "operator pick(value: i64) -> i64\n",
        "impl const fn pick<const X: list<f64> = [2.0]>(value: i64) -> i64 requires X == [1.0] => value\n",
        "const fn s(x: Sized<B>) -> i64 => 1\n",
    );
    expect(
        &format!("{shared}const fn a() -> i64 => pick(1, X: [1])\n"),
        &[],
    );
    expect(
        &format!(
            "{shared}const fn a() -> i64 => pick(1, X: [2])\n\
             const fn b(x: BBox<1m == 60>) -> i64 => 1\n"
        ),
        &["9 operator.no_candidate", "10 generic.argument_constant"],
    );
}

/// TYPE-14: a constant of a `T?` that is no null is a value of a place
/// of that `T?`, and of no place of a `T`. "It does not make null an
/// admitted value of unqualified T": core's `+`, `-`, `<` take no
/// `T?` operand, and an operation on one is worked out to nothing. `==`
/// and `!=` compare two of a `T?`, `null` with a `T` among them.
#[test]
fn a_nullable_constant_is_a_value_of_its_nullable_type() {
    let shared = concat!(
        "const A: i64? = 1\n",
        "struct Box<const X: i64?> { v: i64 }\n",
        "struct IBox<const X: i64> { v: i64 }\n",
        "struct Flag<const X: bool> { v: i64 }\n",
    );
    expect(
        &format!(
            "{shared}const fn a(x: Box<A>) -> Box<1> => x\n\
             const fn b(x: Flag<(A == 1)>, y: Flag<(A != null)>) -> Flag<true> => y\n\
             const fn c(x: Flag<(1 == null)>) -> Flag<false> => x\n\
             const fn d<const N: i64?>(x: Flag<(N == null)>) -> Flag<(N == null)> => x\n\
             const fn e(x: Flag<true>) -> Flag<true> => d<null>(x)\n"
        ),
        &[],
    );
    expect(
        &format!(
            "{shared}const fn a(x: IBox<A>) -> i64 => 1\n\
             const fn b(x: IBox<(1 + A)>) -> IBox<2> => x\n\
             const fn c(x: Box<(-A)>) -> i64 => 1\n\
             const fn d(x: Flag<(A < 2)>) -> i64 => 1\n\
             const fn e<const N: i64?>(x: Flag<(N == null)>) -> Flag<(N == null)> => x\n\
             const fn f(x: Flag<true>) -> Flag<true> => e<1>(x)\n"
        ),
        &[
            "6 type.mismatch",
            "7 generic.argument_constant",
            "8 generic.argument_constant",
            "9 generic.argument_constant",
            "11 type.mismatch",
            "11 type.mismatch",
        ],
    );
}

/// GEN-6: "An option the call omits takes its candidate's default",
/// read as a value of the option's type (EXPR-5): integer literals past
/// 2^53 joined by `+` where an `f64` is expected are `f64` arithmetic.
#[test]
fn a_candidates_default_is_read_as_its_options_type() {
    expect(
        concat!(
            "operator scaled(value: i64) -> i64\n",
            "impl const fn scaled<const X: f64 = 9007199254740993 + 0>(value: i64) -> i64 => value\n",
            "const fn uses() -> i64 => scaled(1)\n",
        ),
        &[],
    );
    // An option the call gives is read as a value of the type the
    // candidate declares, in the same way: `X: 2` of an `f64` is `2.0`.
    let given = |option: &str| {
        format!(
            "operator picked(value: i64) -> i64\n\
             impl const fn picked<const X: f64>(value: i64) -> i64 requires X == 2.0 => value\n\
             const fn uses() -> i64 => picked(1, X: {option})\n"
        )
    };
    expect(&given("2"), &[]);
    expect(&given("3"), &["4 operator.no_candidate"]);
}

/// TYPE-7: "Enum members are distinct typed constants": a member is one
/// of its enum, kept with it where its place's type says nothing of the
/// enum, `any`. There `.member` names none: "With no enum type
/// expected, `.member` rejects".
#[test]
fn a_member_keeps_its_enum() {
    let shared = concat!(
        "enum A { x }\n",
        "enum B { x }\n",
        "struct Box<const X: any> { v: i64 }\n",
    );
    expect(
        &format!("{shared}const fn a(b: Box<A.x>) -> Box<A(0)> => b\n"),
        &[],
    );
    expect(
        &format!(
            "{shared}const fn a(b: Box<A.x>) -> Box<B.x> => b\n\
             const fn d(b: Box<.x>) -> i64 => 1\n\
             const fn e(b: Box<[.x]>) -> i64 => 1\n\
             const fn f(b: Box<(1, .x)>) -> i64 => 1\n"
        ),
        &[
            "5 type.mismatch",
            "6 enum.context_type",
            "7 enum.context_type",
            "8 enum.context_type",
        ],
    );
    let checked = module(&format!(
        "{shared}const fn a(x: Box<A.x>, y: Box<B.x>) -> i64 => 1\n"
    ));
    let keys: Vec<Option<String>> = parameter_types(&checked, "a")
        .iter()
        .map(|ty| match ty {
            Type::Named { arguments, .. } => match arguments.as_slice() {
                [Argument::Constant(constant)] => constant.value.key(),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(
        keys,
        [Some("m::A.x".to_string()), Some("m::B.x".to_string())]
    );
}

/// Siblings of the member's enum and the list's size: a member
/// constructed from a generic value parameter is of its enum too, and a
/// named list constant is held to the size its place expects, under a
/// `?` too, and at any depth (TYPE-7, TYPE-10).
#[test]
fn a_constructed_members_enum_and_a_named_lists_size_count() {
    let shared = concat!(
        "enum A { x }\n",
        "enum B { x }\n",
        "struct Box<const X: any> { v: i64 }\n",
        "struct Sized<const X: list<i64, 3>?> { v: i64 }\n",
        "const TWO: list<i64, 2>? = [1, 2]\n",
        "const THREE: list<i64, 3>? = [1, 2, 3]\n",
        "const NESTED: list<list<i64>> = [[1, 2]]\n",
        "struct Deep<const X: list<list<i64, 3>>> { v: i64 }\n",
        "struct Pairs<const X: list<tuple<i64, list<i64, 2>>>> { v: i64 }\n",
        "const PAIRS: list<tuple<i64, list<i64>>> = [(1, [1, 2])]\n",
    );
    expect(
        &format!(
            "{shared}const fn f<const N: i64>(b: Box<A(N)>) -> Box<A(N)> => b\n\
             const fn g(s: Sized<THREE>) -> i64 => 1\n\
             const fn h(s: Pairs<PAIRS>) -> i64 => 1\n"
        ),
        &[],
    );
    expect(
        &format!(
            "{shared}const fn f<const N: i64>(b: Box<A(N)>) -> Box<B(N)> => b\n\
             const fn g(s: Sized<TWO>) -> i64 => 1\n\
             const fn h(s: Deep<NESTED>) -> i64 => 1\n"
        ),
        &["12 type.mismatch", "13 type.mismatch", "14 type.mismatch"],
    );
}

/// A list or a tuple whose elements' types are not known keeps its
/// shape: `[]` is no `i64` (EXPR-5). An equality of two types is no
/// constant `false`: OPS-2, "`==` and `!=` compare two values of one
/// type", and two enums' members are of two (TYPE-7). An implied
/// member, alone or in a list or a tuple, is of the enum of the value
/// beside it, and is no member of none. A comparison of lists or tuples
/// holding a generic value parameter is worked out once it is given.
#[test]
fn a_collections_shape_and_an_equalitys_operands_count() {
    let shared = concat!(
        "enum A { x, y }\n",
        "enum B { x }\n",
        "struct IBox<const X: i64> { v: i64 }\n",
        "struct LBox<const X: list<i64>> { v: i64 }\n",
        "struct Flag<const X: bool> { v: i64 }\n",
    );
    expect(
        &format!(
            "{shared}const fn f(x: LBox<[]>, y: Flag<(A.x == A.x)>, z: Flag<([A.x] != [A.x])>) -> i64 => 1\n\
             const fn g(x: Flag<(1 == 1.0)>, y: Flag<(A.x != .y)>) -> Flag<true> => x\n\
             const fn h<const M: A>(x: Flag<(M == .x)>) -> i64 => 1\n\
             const fn m<const N: i64>(x: Flag<(N == 1.0)>, y: Flag<(N != null)>) -> i64 => 1\n\
             const fn i(x: Flag<([A.x] == [.x])>, y: Flag<((.y, 1) != (A.x, 1))>) -> Flag<true> => y\n\
             const fn j<const N: i64>(x: Flag<([N] == [1])>) -> Flag<([N] == [1])> => x\n\
             const fn k(x: Flag<true>, y: Flag<false>) -> Flag<false> => j<2>(y)\n\
             const fn l(x: Flag<true>) -> Flag<true> => j<1>(x)\n"
        ),
        &[],
    );
    expect(
        &format!(
            "{shared}const fn f(x: IBox<[]>) -> i64 => 1\n\
             const fn g(x: IBox<[1, \"a\"]>) -> i64 => 1\n\
             const fn h(x: IBox<(1, [])>) -> i64 => 1\n\
             const fn i(x: Flag<(A.x == B.x)>) -> i64 => 1\n\
             const fn j(x: Flag<([A.x] != [B.x])>) -> i64 => 1\n\
             const fn k(x: Flag<(.x == .x)>) -> i64 => 1\n\
             const fn l(x: Flag<(A.x == .z)>) -> i64 => 1\n\
             const fn m(x: Flag<(.y != A.x)>) -> Flag<false> => x\n\
             const fn n(x: Flag<([.x] == [.x])>) -> i64 => 1\n\
             const fn o(x: Flag<([.x] != [])>) -> i64 => 1\n\
             const fn p(x: Flag<([A.x] == [.z])>) -> i64 => 1\n\
             const fn q<const N: i64>(x: Flag<((N, 1) == (1, 1))>) -> Flag<((N, 1) == (1, 1))> => x\n\
             const fn r(x: Flag<true>) -> Flag<true> => q<2>(x)\n\
             const fn s<const N: i64>(x: Flag<(N == \"a\")>) -> i64 => 1\n\
             const fn t<const M: A>(x: Flag<(M == B.x)>) -> i64 => 1\n"
        ),
        &[
            "7 type.mismatch",
            "8 type.mismatch",
            "9 type.mismatch",
            "10 generic.argument_constant",
            "11 generic.argument_constant",
            "12 generic.argument_constant",
            "13 generic.argument_constant",
            "14 type.mismatch",
            "15 generic.argument_constant",
            "16 generic.argument_constant",
            "17 generic.argument_constant",
            "19 type.mismatch",
            "19 type.mismatch",
            "20 generic.argument_constant",
            "21 generic.argument_constant",
        ],
    );
}

/// EXPR-6: "Boolean connectives short-circuit left to right": the
/// right operand of `false &&` and of `true ||` is not needed where it
/// is a parameter not yet given its value, or fails when it is
/// evaluated, `1 // 0 == 0`. "The operands of `&&`, `||`, and `!` are
/// bools", that one too. One otherwise not worked out leaves the
/// connective not worked out: it may be of no type, `1 + "a"`.
#[test]
fn a_boolean_connective_short_circuits() {
    let shared = "struct Flag<const X: bool> { v: i64 }\n";
    expect(
        &format!(
            "{shared}const fn a<const N: bool>(x: Flag<(false && N)>) -> Flag<false> => x\n\
             const fn b<const N: bool>(x: Flag<(true || N)>) -> Flag<true> => x\n\
             const fn c(x: Flag<(false && (1 // 0 == 0))>) -> Flag<false> => x\n"
        ),
        &[],
    );
    expect(
        &format!(
            "{shared}const fn a(x: Flag<(false && (1 + \"a\" == 0))>) -> i64 => 1\n\
             const fn b(x: Flag<(false && 1)>) -> i64 => 1\n\
             const fn c(x: Flag<(false && (1 // 0))>) -> i64 => 1\n\
             const fn d(x: Flag<(true || \"a\")>) -> i64 => 1\n\
             const fn e<const N: i64>(x: Flag<(false && N)>) -> i64 => 1\n"
        ),
        &[
            "3 generic.argument_constant",
            "4 generic.argument_constant",
            "5 generic.argument_constant",
            "6 generic.argument_constant",
            "7 generic.argument_constant",
        ],
    );
}

/// TYPE-7: a construction "takes one argument, by position, that is an
/// `i64` or a `str`": one of another type, a `T?` among them (TYPE-14),
/// constructs no member, and a member's number is no `T?` either.
#[test]
fn an_enum_construction_takes_an_integer_or_a_string() {
    let shared = concat!(
        "enum Mode { fast, slow }\n",
        "const A: i64? = 0\n",
        "const B: i64 = 1\n",
        "struct Box<const X: Mode> { v: i64 }\n",
    );
    expect(
        &format!(
            "{shared}const fn a(x: Box<Mode(B)>, y: Box<Mode(\"slow\")>) -> Box<Mode.slow> => y\n\
             const fn b(x: Box<Mode(1 + 0)>) -> Box<Mode.slow> => x\n\
             const fn c<const N: i64>(x: Box<Mode(N)>) -> Box<Mode(N)> => x\n"
        ),
        &[],
    );
    expect(
        &format!(
            "{shared}const fn a(x: Box<Mode(A)>) -> Box<Mode.fast> => x\n\
             const fn b(x: Box<Mode(1.0)>) -> i64 => 1\n\
             const fn c<const N: i64?>(x: Box<Mode(N)>) -> i64 => 1\n"
        ),
        &[
            "6 generic.argument_constant",
            "7 generic.argument_constant",
            "8 generic.argument_constant",
        ],
    );
    // An enum member's number is an `i64`, and no `i64?`.
    expect(
        &format!("{shared}enum E {{ a = A, b = B }}\n"),
        &["6 enum.value"],
    );
    expect(&format!("{shared}enum E {{ a = B, b = 2 }}\n"), &[]);
}

// ---- The review of #218 ----

/// GEN-7: a generic body "is judged once, where it is declared, by what
/// its `requires` states, and never again for the generic arguments a
/// call of it gives" (hhenson/hgl_spec#144: so is a call in it of another
/// generic function). A call that passes on a generic value parameter of
/// its own, `one_only<Y>`, holds where the body states enough: `requires
/// Y == 1.0` binds `Y` (GEN-3), and a membership whose every member makes
/// the callee's `requires` hold, at any depth, in a value function, a
/// graph, an operator's candidate and one a named operator requirement is
/// answered by, of a type as of a value. A call of the body is then held
/// to the body's own `requires` (GEN-3: "A call of a function whose
/// `requires` does not hold with the generic arguments the call resolves
/// is rejected"). A struct a type the body writes applies is told for
/// each instance. Where the body states nothing, or a membership with a
/// member the callee does not admit, it rejects where it is declared.
/// "Not told" is never "holds": a constant this compiler does not work
/// out is rejected.
#[test]
fn a_requires_is_told_where_a_passed_on_parameter_is_given() {
    let shared = concat!(
        "const fn one_only<const X: f64>(v: f64) -> f64 requires X == 1.0 => v * X\n",
        "const fn outer<const Y: f64>(v: f64) -> f64 requires Y == 1.0 => one_only<Y>(v)\n",
        "const fn outer2<const Z: f64>(v: f64) -> f64 requires Z == 1.0 => outer<Z>(v)\n",
        "fn temporal_outer<const Y: f64>(v: f64) -> f64 requires Y == 1.0 => one_only<Y>(v)\n",
        "const fn int_only<const X: i64>(v: i64) -> i64 requires X == 1 => v * X\n",
        "const fn int_outer<const Y: i64>(v: i64) -> i64 requires Y in {1} => int_only<Y>(v)\n",
        "const fn two() -> f64 => 2.0\n",
        "const C: f64 = two()\n",
        "struct FBox<const X: f64> { v: i64 }\n",
        "const fn not_two<T>(x: T) -> i64 requires !(T == FBox<2.0>) => 1\n",
        "const fn wrap<const X: f64>(fb: FBox<X>) -> i64 requires X in {1.0, 3.0} => not_two(fb)\n",
    );
    expect(
        &format!(
            "{shared}const fn a() -> f64 => outer<1.0>(5.0) + outer2<1.0>(5.0)\n\
             fn b(v: f64) -> f64 => temporal_outer<1.0>(v)\n\
             const fn c() -> i64 => int_outer<1>(5) + wrap(FBox<1.0>(v: 1))\n"
        ),
        &[],
    );
    expect(
        &format!(
            "{shared}const fn a() -> f64 => outer<2.0>(5.0)\n\
             const fn b() -> f64 => outer2<2.0>(5.0)\n\
             fn c(v: f64) -> f64 => temporal_outer<2.0>(v)\n\
             const fn d() -> i64 => int_outer<2>(5)\n\
             const fn e() -> f64 => one_only<C>(1.0)\n\
             const fn f() -> i64 => wrap(FBox<2.0>(v: 1))\n\
             const fn only_two<T>(x: T) -> i64 requires T == FBox<2.0> => 1\n\
             const fn g() -> i64 => only_two(FBox<C>(v: 1))\n\
             operator unwrap<T>(boxed: T) -> i64\n\
             impl const fn unwrap<const X: f64>(boxed: FBox<X>) -> i64 requires X == 2.0 => only_two(boxed)\n\
             const fn h() -> i64 => unwrap(FBox<1.0>(v: 1))\n\
             const fn k() -> i64 => unwrap(FBox<2.0>(v: 1))\n\
             struct Only<T> requires T == FBox<2.0> {{ v: i64 }}\n\
             struct Outer<const X: f64> {{ held: Only<FBox<X>>? }}\n\
             const fn m() -> Outer<1.0> => Outer<1.0>(held: null)\n\
             const fn n() -> Outer<2.0> => Outer<2.0>(held: null)\n\
             const fn relay<T>(x: T) -> i64 requires unwrap(T) -> i64 => unwrap(x)\n\
             const fn o() -> i64 => relay(FBox<1.0>(v: 1))\n\
             const fn p() -> i64 => relay(FBox<2.0>(v: 1))\n"
        ),
        &[
            "13 requires.unmet",
            "14 requires.unmet",
            "15 requires.unmet",
            "16 requires.unmet",
            "17 requires.unmet",
            "18 requires.unmet",
            "20 requires.unmet",
            // No candidate of `unwrap` is for `FBox<1.0>` (GEN-6).
            "23 operator.no_candidate",
            // The result's type and the construction each apply it.
            "27 requires.unmet",
            "27 requires.unmet",
            "30 requires.unmet",
        ],
    );
    // Rejected where they are declared: nothing stated, a member the
    // callee does not admit, in a value function and a graph, and a
    // candidate that states nothing.
    expect(
        &format!(
            "{shared}const fn bare<const Y: f64>(v: f64) -> f64 => one_only<Y>(v)\n\
             const fn wider<const Y: f64>(v: f64) -> f64 requires Y in {{1.0, 2.0}} => one_only<Y>(v)\n\
             fn temporal_bare<const Y: f64>(v: f64) -> f64 => one_only<Y>(v)\n\
             const fn wrapped<const X: f64>(fb: FBox<X>) -> i64 => not_two(fb)\n"
        ),
        &[
            "13 requires.unmet",
            "14 requires.unmet",
            "15 requires.unmet",
            "16 requires.unmet",
        ],
    );
}

/// GEN-13: "Where the instances of one declaration never end when the
/// program is compiled, the program rejects ... an implementation has a
/// bound on the instances of one declaration, and rejects the program
/// where they pass it" (`generic.instance_limit`, proposed). Every body
/// that passes its generic parameters on is followed, each instance once,
/// and no further than this compiler follows: two calls of itself at each
/// level, with new arguments each time, make instances without end. A
/// call in a generic body of another generic function is judged where
/// the body is declared, by what the body states (GEN-7;
/// hhenson/hgl_spec#144): `grow` states nothing of `N` that makes `need`'s
/// `requires` hold, and is rejected there.
#[test]
fn the_instances_a_call_makes_are_followed_within_a_bound() {
    let shared = concat!(
        "const fn need<const N: i64>(v: i64) -> i64 requires N in {1, 2, 3} => v\n",
        "const fn free<const N: i64>(v: i64) -> i64 => free<N * 2>(v) + free<N * 2 + 1>(v)\n",
        "const fn every<const N: i64>(v: i64) -> i64 requires N == N => v\n",
        "const fn spin<const N: i64>(v: i64) -> i64 => every<N>(v) + spin<N * 2>(v) + spin<N * 2 + 1>(v)\n",
    );
    expect(
        &format!("{shared}const fn a() -> i64 => free<1>(1)\n"),
        &["6 generic.instance_limit"],
    );
    // An instance whose arguments grow is followed no further than they
    // may be held: `S + S` doubles a string at each level.
    expect(
        &format!(
            "{shared}const fn same_text<const S: str>(v: i64) -> i64 requires S == S => v\n\
             const fn doubled<const S: str>(v: i64) -> i64 => same_text<S>(v) + doubled<S + S>(v)\n\
             const fn a() -> i64 => doubled<\"x\">(1)\n"
        ),
        &["8 generic.instance_limit"],
    );
    // One instance made from many calls is followed once: four thousand
    // and more calls of `wrap<1>` are one instance of it, whose
    // `requires` holds.
    let calls: String = (0..4100)
        .map(|index| format!("const fn c{index}() -> i64 => wrap<1>(1)\n"))
        .collect();
    expect(
        &format!(
            "{shared}const fn wrap<const N: i64>(v: i64) -> i64 requires N == 1 => need<N>(v)\n{calls}"
        ),
        &[],
    );
    // A `requires` beside a constant not worked out is told by what the
    // body binds: `B` settles it whatever `C` is.
    expect(
        &format!(
            "{shared}const fn two() -> f64 => 2.0\n\
             const C: f64 = two()\n\
             const fn either<const A: f64, const B: f64>(v: i64) -> i64 requires A == 1.0 || B == 2.0 => v\n\
             const fn pass<const X: f64>(v: i64) -> i64 requires X == 2.0 => either<C, X>(v)\n\
             const fn a() -> i64 => pass<2.0>(1)\n\
             const fn b() -> i64 => pass<3.0>(1)\n"
        ),
        &["11 requires.unmet"],
    );
    expect(
        &format!(
            "{shared}const fn grow<const N: i64>(v: i64) -> i64 => need<N>(v) + grow<N * 2>(v) + grow<N * 2 + 1>(v)\n\
             const fn a() -> i64 => grow<1>(1)\n\
             const fn b() -> i64 => spin<1>(1)\n"
        ),
        &[
            "6 requires.unmet",
            "7 generic.instance_limit",
            "8 generic.instance_limit",
        ],
    );
}

/// GEN-3: a `requires` "may use type/category membership, equalities,
/// the membership of a constant in a set of constants": of two generic
/// value parameters too, `N == M` and `N in {M, 5}`, each told by the one
/// comparison. "under negation/disjunction it only tests
/// already-resolved data": a type equality has the same three answers as
/// a constant's: `!(T == FBox<2.0>)` is not told of an `FBox<X>` while
/// `X` is not given, and a body that calls `not_two` so states what
/// tells it, `requires X in {1.0, 3.0}` (GEN-7; see
/// `a_requires_is_told_where_a_passed_on_parameter_is_given`).
#[test]
fn a_requires_compares_two_options_and_a_type_in_three_answers() {
    let shared = concat!(
        "enum Mode { fast, slow }\n",
        "struct FBox<const X: f64> { v: i64 }\n",
        "const fn same_size<const N: i64, const M: i64>(v: i64) -> i64 requires N == M => v\n",
        "const fn modes<const A: Mode, const B: Mode>(v: i64) -> i64 requires A == B => v\n",
        "const fn within<const N: i64, const M: i64>(v: i64) -> i64 requires N in {M, 5} => v\n",
        "const fn not_two<T>(x: T) -> i64 requires !(T == FBox<2.0>) => 1\n",
        "const fn outer<const X: f64>(fb: FBox<X>) -> i64 requires X in {1.0, 3.0} => not_two(fb)\n",
    );
    expect(
        &format!(
            "{shared}const fn a() -> i64 => same_size<1, 1>(1) + modes<.fast, .fast>(1)\n\
             const fn b() -> i64 => within<5, 2>(1) + within<2, 2>(1)\n\
             const fn c() -> i64 => outer(FBox<1.0>(v: 1)) + not_two(FBox<1.0>(v: 1))\n"
        ),
        &[],
    );
    expect(
        &format!(
            "{shared}const fn a() -> i64 => same_size<1, 2>(1)\n\
             const fn b() -> i64 => modes<.fast, .slow>(1)\n\
             const fn c() -> i64 => within<1, 2>(1)\n\
             const fn e() -> i64 => not_two(FBox<2.0>(v: 1))\n"
        ),
        &[
            "9 requires.unmet",
            "10 requires.unmet",
            "11 requires.unmet",
            "12 requires.unmet",
        ],
    );
}

/// UNIT-13: "The same value is not `==` ...: `-0.0` and `0.0` are two
/// constants, though `-0.0 == 0.0`, and any two NaNs, `0.0 / 0.0` among
/// them, are one constant, though `==` holds for no NaN." GEN-3: "with
/// `const k: f64`, `requires k == 0.0` holds where `k` is `-0.0`".
/// NUM-3's true division "defaults to the `ieee` zero policy ... zero
/// divided by zero produces NaN", which is a constant.
#[test]
fn a_constant_is_its_value_and_a_comparison_the_languages() {
    let shared = concat!(
        "struct Scaled<const X: f64> { v: i64 }\n",
        "const fn z<const X: f64>(v: f64) -> f64 requires X == 0.0 => v\n",
    );
    expect(
        &format!(
            "{shared}const fn a(x: Scaled<0.0>) -> Scaled<0.00> => x\n\
             const fn b(x: Scaled<-0.0>) -> Scaled<-(0.0)> => x\n\
             const fn c() -> f64 => z<-0.0>(1.0) + z<0.0>(1.0)\n\
             const NAN: f64 = 0.0 / 0.0\n\
             const fn d(t: Scaled<NAN>) -> Scaled<0.0 / 0.0> => t\n"
        ),
        &[],
    );
    // NUM-5: "NaN is neither equal nor close to any value, including
    // itself", and an ordering of one is false: a comparison the program
    // makes of a NaN constant is the language's.
    expect(
        &format!(
            "{shared}struct Flag<const X: bool> {{ v: i64 }}\n\
             const NAN: f64 = 0.0 / 0.0\n\
             const fn a(x: Flag<(0 == NAN)>, y: Flag<(NAN == NAN)>) -> Flag<false> => x\n\
             const fn b(x: Flag<(NAN < 1.0)>, y: Flag<(1 >= NAN)>) -> Flag<false> => y\n\
             const fn c(x: Flag<(NAN != NAN)>, y: Flag<(0 != NAN)>) -> Flag<true> => y\n\
             const fn d(x: Flag<(0 == NAN)>) -> Flag<true> => x\n"
        ),
        &["9 type.mismatch"],
    );
    expect(
        &format!(
            "{shared}const fn a(x: Scaled<-0.0>) -> Scaled<0.0> => x\n\
             const fn b(x: Scaled<0.0>) -> Scaled<-0.0> => x\n"
        ),
        &["4 type.mismatch", "5 type.mismatch"],
    );
}

/// TYPE-7: "With no enum type expected, `.member` rejects", and a member
/// its enum does not have is no value of it. GEN-13: a generic value
/// parameter's default is a value of the type the call gives the
/// parameter: `.slow`, the default of a `const X: T`, is held to each
/// call's and each construction's `T`.
#[test]
fn an_implied_member_default_is_held_to_the_type_given() {
    let shared = concat!(
        "enum Mode { fast, slow }\n",
        "const fn pick<T, const X: T = .slow>(v: T) -> T => X\n",
        "const fn missing<T, const X: T = .medium>(v: T) -> T => X\n",
        "struct TB<T, const X: T = .slow> { v: T }\n",
    );
    expect(
        &format!(
            "{shared}const fn a() -> Mode => pick(Mode.fast)\n\
             const fn b(v: Mode) -> Mode => TB(v: v).v\n"
        ),
        &[],
    );
    expect(
        &format!(
            "{shared}const fn a() -> i64 => pick(1)\n\
             const fn b() -> Mode => missing(Mode.fast)\n\
             const fn c(v: i64) -> i64 => TB(v: v).v\n"
        ),
        &["6 type.mismatch", "7 type.mismatch", "8 type.mismatch"],
    );
}

/// UNIT-13: "a failure in evaluating it is a required-constant failure
/// (ERR-9)", and ERR-9: "Required constant failures reject where
/// specified": a constant whose `i64` arithmetic leaves the `i64`s
/// (NUM-2) or divides by zero under `.fail` (NUM-12) is rejected where
/// it is declared, and its uses say nothing more. A constant this
/// compiler does not work out is not taken as known where a generic
/// value argument needs it (GEN-8), as the same expression written in
/// place is not. A sum of any length the parser builds a tree for is
/// worked out.
#[test]
fn a_constant_that_fails_is_rejected_where_it_is_declared() {
    let shared = concat!(
        "struct IBox<const X: i64> { v: i64 }\n",
        "struct FBox<const X: f64> { v: i64 }\n",
    );
    let forty = vec!["1"; 40].join(" + ");
    let four_hundred = vec!["1"; 400].join(" + ");
    expect(
        &format!(
            "{shared}const FORTY: i64 = {forty}\n\
             const fn b(t: IBox<FORTY>) -> IBox<40> => t\n\
             const fn c(t: list<i64, FORTY>) -> list<i64, 40> => t\n\
             const MANY: i64 = {four_hundred}\n\
             const fn d(t: IBox<MANY>) -> IBox<400> => t\n"
        ),
        &[],
    );
    // EXPR-6: "Boolean connectives short-circuit left to right": the
    // right operand of `false &&` and of `true ||` is not evaluated, so
    // its failure is none, and the connective is the left operand.
    expect(
        &format!(
            "{shared}struct Flag<const X: bool> {{ v: i64 }}\n\
             const NO: bool = false && (1 // 0 == 0)\n\
             const YES: bool = true || (9223372036854775807 + 1 > 0)\n\
             const fn a(x: Flag<NO>, y: Flag<YES>) -> Flag<false> => x\n\
             const fn b(y: Flag<YES>) -> Flag<true> => y\n"
        ),
        &[],
    );
    let failed = module(&format!(
        "{shared}const BIG: i64 = 9223372036854775807 + 1\n\
         const DIV: i64 = 1 // 0\n\
         const fn a(t: IBox<BIG>) -> IBox<BIG> => t\n\
         const fn b(t: IBox<DIV>) -> i64 => 1\n"
    ));
    assert_eq!(
        failed.summary(),
        [
            "m.hgl:4:18 type/constant.failure UNIT-13",
            "m.hgl:5:18 type/constant.failure UNIT-13",
        ]
    );
    expect(
        &format!(
            "{shared}const LATER: datetime = @2024-01-01T00:00Z + 1s\n\
             struct TBox<const X: datetime> {{ v: i64 }}\n\
             const fn a(t: TBox<LATER>) -> i64 => 1\n"
        ),
        &["6 generic.argument_constant"],
    );
}

/// GEN-8: "A call rejects when it leaves a generic parameter
/// unresolved": a default that names its own parameter gives it no
/// value. A default that names a parameter before its own is of what
/// that one takes, in a call and in a struct written with no arguments,
/// which takes its defaults (GEN-2): `W2` is `W2<1, 1>`.
#[test]
fn a_default_that_names_its_own_parameter_gives_no_value() {
    let shared = concat!(
        "struct IBox<const X: i64> { v: i64 }\n",
        "const fn sized<const N: i64 = N>(v: i64) -> i64 => N + v\n",
        "struct Selfish<const X: i64 = X> { v: i64 }\n",
        "struct W2<const A: i64 = 1, const B: i64 = A> { v: i64 }\n",
        "const fn d<const N: i64 = 1, const M: i64 = N>(v: i64) -> IBox<M> => IBox<M>(v: v)\n",
        "const fn rec<const N: i64, const M: i64 = N>(v: i64) -> i64 => rec<N>(v) + M\n",
    );
    expect(
        &format!(
            "{shared}const fn a(w: W2) -> W2<1, 1> => w\n\
             const fn b() -> i64 => sized<1>(2)\n\
             const fn c() -> i64 {{\n    let one = d(1)\n    let three = d<3>(1)\n    \
             let x: IBox<1> = one\n    let y: IBox<3> = three\n    return 1\n}}\n"
        ),
        &[],
    );
    expect(
        &format!(
            "{shared}const fn a() -> i64 => sized(1)\n\
             const fn b(s: Selfish) -> i64 => 1\n\
             const fn c() -> i64 {{\n    let one = d(1)\n    let x: IBox<2> = one\n    return 1\n}}\n"
        ),
        &[
            "8 generic.unresolved",
            "9 generic.argument_count",
            "12 type.mismatch",
        ],
    );
}

/// The types of the parameters of the function `name` of `m`.
fn parameter_types(checked: &Checked, name: &str) -> Vec<Type> {
    let module = checked.program.module("m").expect("the module exists");
    let index = module
        .declarations
        .iter()
        .position(|declaration| declaration.name == name)
        .expect("the function exists");
    match &module.details[index] {
        Detail::Callable(signature) => signature
            .parameters
            .iter()
            .map(|parameter| parameter.ty.clone())
            .collect(),
        other => panic!("{name} is {other:?}"),
    }
}
