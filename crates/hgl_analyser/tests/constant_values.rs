//! The analyser against what a compile-time constant is: one value of
//! the type its place gives it, however it is written, compared with
//! another by that type's own equality (GEN-8, UNIT-13, EXPR-5, CTRL-6,
//! TYPE-2, TYPE-7, TYPE-17; the owner's decision on
//! hhenson/hgl-compiler#211). Each line of that issue's two tables is
//! here twice: the program that must be rejected, and the one beside it
//! that must not.
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

/// The owner's first point (hhenson/hgl-compiler#211): "if a constant
/// once coerced to its expected type and converted to it's value is the
/// same then it is the same". EXPR-5: "An integer literal is an `f64`
/// where one is expected", so `FBox<1>` is `FBox<1.0>`; an `i64`
/// constant is not converted by its place, and is no `f64`.
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

/// The owner's second point: "we should still trace back to the express
/// way the it was written": a constant keeps how it is written beside
/// its value, and a message shows it so.
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

/// TYPE-2: a `datetime` is a "UTC-instant", and two are equal as
/// instants (the owner's decision on hhenson/hgl-compiler#211). A
/// `zoned_datetime` is an instant in a named zone: "Same-instant zoned
/// values need not be equal; zone aliases are distinct names".
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
/// instance, the constant the call gives it, a list or a tuple among
/// them: each element as the value it is of the element's type (EXPR-5:
/// the `1` of a `tuple<f64, str>` is `1.0`).
#[test]
fn a_composite_constant_is_its_elements_in_an_instance() {
    let checked = module(concat!(
        "const fn values<const X: list<i64>>() -> list<i64> => X\n",
        "const fn pair<const P: tuple<f64, str>>() -> tuple<f64, str> => P\n",
        "const fn listed() -> list<i64> => values<[1, 0x2]>()\n",
        "const fn paired() -> tuple<f64, str> => pair<(1, \"\\u{61}\")>()\n",
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
/// of that `T?`, and of no place of a `T`.
#[test]
fn a_nullable_constant_is_a_value_of_its_nullable_type() {
    let shared = concat!(
        "const A: i64? = 1\n",
        "struct Box<const X: i64?> { v: i64 }\n",
        "struct IBox<const X: i64> { v: i64 }\n",
    );
    expect(
        &format!("{shared}const fn a(x: Box<A>) -> Box<1> => x\n"),
        &[],
    );
    expect(
        &format!("{shared}const fn a(x: IBox<A>) -> i64 => 1\n"),
        &["5 type.mismatch"],
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
             const fn d(b: Box<.x>) -> i64 => 1\n"
        ),
        &["5 type.mismatch", "6 enum.context_type"],
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
/// `?` too (TYPE-7, TYPE-10).
#[test]
fn a_constructed_members_enum_and_a_named_lists_size_count() {
    let shared = concat!(
        "enum A { x }\n",
        "enum B { x }\n",
        "struct Box<const X: any> { v: i64 }\n",
        "struct Sized<const X: list<i64, 3>?> { v: i64 }\n",
        "const TWO: list<i64, 2>? = [1, 2]\n",
        "const THREE: list<i64, 3>? = [1, 2, 3]\n",
    );
    expect(
        &format!(
            "{shared}const fn f<const N: i64>(b: Box<A(N)>) -> Box<A(N)> => b\n\
             const fn g(s: Sized<THREE>) -> i64 => 1\n"
        ),
        &[],
    );
    expect(
        &format!(
            "{shared}const fn f<const N: i64>(b: Box<A(N)>) -> Box<B(N)> => b\n\
             const fn g(s: Sized<TWO>) -> i64 => 1\n"
        ),
        &["8 type.mismatch", "9 type.mismatch"],
    );
}

/// A list or a tuple whose elements' types are not known keeps its
/// shape: `[]` is no `i64` (EXPR-5). An equality of two types is no
/// constant `false`: OPS-2, "`==` and `!=` compare two values of one
/// type", and two enums' members are of two (TYPE-7). An implied
/// member, alone or in a list or a tuple, is of the enum of the value
/// beside it, and is no member of none.
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
             const fn i(x: Flag<([A.x] == [.x])>, y: Flag<((.y, 1) != (A.x, 1))>) -> Flag<true> => y\n"
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
             const fn p(x: Flag<([A.x] == [.z])>) -> i64 => 1\n"
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
