//! Candidate-local literal readings with operator options (GEN-2, GEN-5, GEN-6).

mod common;

use common::check;

const DECLARATIONS: &str = concat!(
    "module m\n",
    "operator n2<L, C, const on: bool = true>(items: C, zero: L? = null) -> L\n",
    "impl const fn n2<V, const on: bool = true>(items: list<V>, zero: V? = null) -> V requires on => items[0]\n",
    "operator n1<L, C>(items: C, zero: L? = null) -> L\n",
    "impl const fn n1<V>(items: list<V>, zero: V? = null) -> V => items[0]\n",
    "operator plus<T>(a: T, b: T) -> T properties<f64> { associative }\n",
    "impl const fn plus(a: f64, b: f64) -> f64 => a + b\n",
    "impl const fn plus(a: i64, b: i64) -> i64 => a + b\n",
    "const fn fsub(a: f64, b: f64) -> f64 => a - b\n",
    "operator tc<L, R, C, const on: bool = true>(f: const fn(L, R) -> L, items: C, zero: L? = null) -> L\n",
    "impl const fn tc<V, const on: bool = true>(f: const fn(V, V) -> V, items: list<V>, zero: V? = null) -> V requires on => f(items[0], items[1])\n",
    "operator t0<L, R, C>(f: const fn(L, R) -> L, items: C, zero: L? = null) -> L\n",
    "impl const fn t0<V>(f: const fn(V, V) -> V, items: list<V>, zero: V? = null) -> V => f(items[0], items[1])\n",
    "operator red<L, R, C, const law: bool = associative(f)>(f: const fn(L, R) -> L, items: C, zero: L? = null) -> L\n",
    "impl const fn red<V, const law: bool = associative(f)>(f: const fn(V, V) -> V, items: list<V>, zero: V? = null) -> V requires law => f(items[0], items[1])\n",
    "impl const fn red<L, R, const law: bool = associative(f)>(f: const fn(L, R) -> L, items: list<R>, zero: L) -> L requires !law => f(f(zero, items[0]), items[1])\n",
    "operator o1<L, R, C>(f: const fn(L, R) -> L, items: C, zero: L? = null) -> L\n",
    "impl const fn o1<L, R>(f: const fn(L, R) -> L, items: list<R>, zero: L) -> L => f(f(zero, items[0]), items[1])\n",
);

#[test]
fn a_literal_seed_takes_the_candidate_type_with_or_without_an_option() {
    // GEN-2, GEN-6: items decide V before zero, which is a widened literal.
    for operator in ["n1", "n2", "n2{on: true}"] {
        for seed in ["1", "-1"] {
            let call = format!("{operator}([1.5, 2.0], {seed})");
            let checked = check(&[(
                "m.hgl",
                &format!("{DECLARATIONS}test observed {{ assert {call} == 1.5 }}\n"),
            )]);
            assert!(!checked.rejected, "{call}: {}", checked.report);
        }
    }
}

#[test]
fn function_arguments_keep_candidate_local_literal_widening() {
    // GEN-2, GEN-6, FN-17: callbacks and options do not fix the seed as i64.
    for call in [
        "t0(plus, [1.5, 2.0], 1)",
        "t0(fn(a, b) => a + b, [1.5, 2.0], 1)",
        "tc(plus, [1.5, 2.0], 1)",
        "tc(fn(a, b) => a + b, [1.5, 2.0], 1)",
        "tc{on: true}(plus, [1.5, 2.0], 1)",
        "red(plus, [1.5, 2.0], 1)",
    ] {
        let checked = check(&[(
            "m.hgl",
            &format!("{DECLARATIONS}test observed {{ assert {call} == 3.5 }}\n"),
        )]);
        assert!(!checked.rejected, "{call}: {}", checked.report);
    }
}

#[test]
fn a_named_function_still_decides_its_types_before_a_later_literal() {
    // GEN-2, FN-17: d12/o1 is an existing success, not another fix to make.
    for call in ["o1(fsub, [1.5, 2.0], 10)", "red(fsub, [1.5, 2.0], 10)"] {
        let checked = check(&[(
            "m.hgl",
            &format!("{DECLARATIONS}test observed {{ assert {call} == 6.5 }}\n"),
        )]);
        assert!(!checked.rejected, "{call}: {}", checked.report);
    }
}

#[test]
fn the_same_literal_reading_is_used_in_a_graph() {
    // GEN-2, GEN-6: graph selection gives the literal its candidate's type too.
    for operator in ["n1", "n2", "n2{on: true}"] {
        let checked = check(&[(
            "m.hgl",
            &format!(
                "{DECLARATIONS}fn graph_value(items: list<f64>) -> f64 {{ let selected = {operator}(items, 1)\n return selected }}\n"
            ),
        )]);
        assert!(!checked.rejected, "{operator}: {}", checked.report);
    }
}

#[test]
fn widening_does_not_override_written_types_or_turn_a_variable_into_a_literal() {
    // GEN-2, GEN-5, GEN-6: only literals widen; explicit requests and gates hold.
    for body in [
        "test rejected { assert n2<i64>([1.5, 2.0], 1) == 1 }",
        "test rejected { assert n2{on: false}([1.5, 2.0], 1) == 1.5 }",
        "const fn rejected(items: list<f64>, seed: i64) -> f64 => n2(items, seed)",
    ] {
        let checked = check(&[("m.hgl", &format!("{DECLARATIONS}{body}\n"))]);
        assert!(
            checked
                .findings
                .iter()
                .any(|finding| finding.code == "operator/operator.no_candidate"),
            "{}",
            checked.report
        );
    }
}

#[test]
fn an_output_default_keeps_its_shape_without_fixing_a_literal_element_type() {
    // GEN-2, GEN-6, GEN-8: size still selects the result, while the
    // integer literal takes the chosen candidate's f64 parameter type.
    let source = concat!(
        "module m\n",
        "operator window<T, const size: i64 = 3>(value: T) -> list<T, size>\n",
        "impl const fn window(value: f64) -> list<f64, 3> => [value, value, value]\n",
        "impl const fn window(value: f64) -> list<f64, 5> => [value, value, value, value, value]\n",
        "test defaults { let three = window(1)\n assert three == [1.0, 1.0, 1.0]\n",
        "let five = window{size: 5}(1)\n assert five == [1.0, 1.0, 1.0, 1.0, 1.0] }\n",
    );
    let checked = check(&[("m.hgl", source)]);
    assert!(!checked.rejected, "{}", checked.report);
}

#[test]
fn dependent_generic_defaults_keep_their_call_diagnostics() {
    // GEN-13: a default is checked at the type the call resolves,
    // including when the selected candidate never reads that option.
    for reads_default in [false, true] {
        for (ty, argument, setup) in [
            ("i64", "1", ""),
            ("f64", "1.0", ""),
            ("i64", "typed", "let typed: i64 = 1\n"),
        ] {
            let candidate = if reads_default {
                format!(
                    "impl const fn choose<const fallback: {ty}>(value: {ty}) -> {ty} => fallback\n"
                )
            } else {
                format!("impl const fn choose(value: {ty}) -> {ty} => value\n")
            };
            let source = format!(
                "module m\noperator choose<T, const fallback: T = \"x\">(value: T) -> T\n{candidate}test observed {{ {setup}let picked = choose({argument}) }}\n"
            );
            let checked = check(&[("m.hgl", &source)]);
            assert_eq!(
                checked.findings.len(),
                1,
                "{reads_default}/{argument}: {}",
                checked.report
            );
            let finding = &checked.findings[0];
            assert_eq!(finding.code, "type/type.mismatch", "{}", checked.report);
            assert_eq!(finding.rule, "GEN-13", "{}", checked.report);
            assert_eq!(
                finding.message,
                format!(
                    "this leaves `fallback` to its default, a `str`, and `fallback` is `{ty}` here"
                )
            );
        }
    }
}

#[test]
fn public_generic_defaults_and_explicit_overrides_keep_their_checks() {
    // GEN-13, EXPR-5: public default checks keep the literal reading;
    // an integer default can also fit the selected float candidate,
    // and explicit values do not take the invalid declared default.
    let checked = check(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "operator floating<T, const fallback: T = 1>(value: T) -> T\n",
            "impl const fn floating(value: f64) -> f64 => value\n",
            "operator overridden<T, const fallback: T = \"x\">(value: T) -> T\n",
            "impl const fn overridden(value: i64) -> i64 => value\n",
            "test observed { let widened = floating(1)\n",
            "let named = overridden{fallback: 7}(1)\n",
            "let positional = overridden<i64, 7>(1) }\n",
        ),
    )]);
    assert!(!checked.rejected, "{}", checked.report);
}

#[test]
fn options_preserve_every_literal_form_and_its_diagnostics() {
    // GEN-5, GEN-6: an unrelated option neither requests a preliminary
    // result type nor changes which candidate reads the arguments.
    let scalar =
        "operator op<L@O>(value: L) -> L\nimpl const fn op@C(value: f64) -> f64@R => value\n";
    let integer =
        "operator op<L@O>(value: L) -> L\nimpl const fn op@C(value: i64) -> i64@R => value\n";
    let list = "operator op<L@O>(items: list<L>) -> L\nimpl const fn op@C(items: list<f64>) -> f64@R => items[0] + 0.5\n";
    let callback = "operator op<L@O>(zero: L, f: const fn(L, L) -> L) -> L\nimpl const fn op@C(zero: f64, f: const fn(f64, f64) -> f64) -> f64@R => f(zero, zero)\n";
    let seeded = "operator op<L, C@O>(items: C, zero: L? = null) -> L\nimpl const fn op<V@O>(items: list<V>, zero: V? = null) -> V@R => items[0]\n";
    let plus = "operator plus<T>(a: T, b: T) -> T\nimpl const fn plus(a: f64, b: f64) -> f64 => a + b\nimpl const fn plus(a: i64, b: i64) -> i64 => a + b\n";
    for (declarations, call, admitted) in [
        (scalar, "op(3)", true),
        (scalar, "op(-3)", true),
        (scalar, "op((3))", true),
        (scalar, "op(1.5)", true),
        (scalar, "op(value: 3)", true),
        (integer, "op(1.5)", false),
        (list, "op([1, 2])", true),
        (list, "op([1.0, 2.0])", true),
        (list, "op(items: [1, 2])", true),
        (callback, "op(3, plus)", true),
        (callback, "op(3, add_)", true),
        (callback, "op(3, fn(a, b) => a + b)", true),
        (callback, "op(f: add_, zero: 3)", true),
        (seeded, "op([1.5, 2.0], 1)", true),
        (seeded, "op([1, 2], 0.5)", false),
    ] {
        let mut outcomes = Vec::new();
        for mode in 0..3 {
            let option = if mode == 0 {
                ""
            } else {
                ", const on: bool = true"
            };
            let generic = if mode == 0 {
                ""
            } else {
                "<const on: bool = true>"
            };
            let requires = if mode == 0 { "" } else { " requires on" };
            let declarations = declarations
                .replace("@O", option)
                .replace("@C", generic)
                .replace("@R", requires);
            let call = if mode == 2 {
                call.replace("op(", "op{on: true}(")
            } else {
                call.to_string()
            };
            let source =
                format!("module m\n{plus}{declarations}test observed {{ let picked = {call} }}\n");
            let checked = check(&[("m.hgl", &source)]);
            assert_eq!(
                !checked.rejected, admitted,
                "mode {mode}/{call}: {}",
                checked.report
            );
            outcomes.push(
                checked
                    .findings
                    .into_iter()
                    .map(|f| (f.code, f.rule, f.message))
                    .collect::<Vec<_>>(),
            );
        }
        assert_eq!(outcomes[0], outcomes[1], "{call}");
        // A written option is listed among the supplied arguments;
        // account for that entry while preserving the failure itself.
        let explicit: Vec<_> = outcomes[0]
            .iter()
            .map(|(code, rule, message)| {
                (
                    code.clone(),
                    *rule,
                    message.replace(" takes ", " takes `on: bool`, "),
                )
            })
            .collect();
        assert_eq!(explicit, outcomes[2], "{call}");
    }
}

#[test]
fn literal_types_still_constrain_public_result_requirements() {
    // GEN-3, GEN-4: result membership still reads complete public inference.
    let declarations = "module m\noperator rs<T, O>(value: T) -> O requires O in {T}\nimpl const fn rs(value: i64) -> f64 => 0.5\nconst fn accept(value: f64) -> f64 => value\n";
    for body in [
        "test observed { let picked = rs(1) }",
        "test observed { let picked = rs(-1) }",
        "const fn observed() -> f64 => rs(1)",
        "const fn observed<U>(other: U) -> f64 => rs(1)",
        "test observed { let picked = accept(rs(1)) }",
    ] {
        let checked = check(&[("m.hgl", &format!("{declarations}{body}\n"))]);
        assert!(
            checked
                .findings
                .iter()
                .any(|f| f.code == "operator/operator.no_candidate"),
            "{}",
            checked.report
        );
    }
}

#[test]
fn a_reflected_pack_requirement_keeps_the_literals_public_type() {
    // GEN-3, GEN-17: T remains i64 while the function reflects its named pack.
    let checked = check(&[(
        "m.hgl",
        r#"module m
const fn scaled(price: f64) -> f64 => price
operator rr<T, ...Ps, ...As>(seed: T, f: const fn(...{Ps}) -> f64, args: ...{As for f}) -> i64
    requires T in {i64} && each arg in arg_keys(As) { As[arg] == Ps[arg] }
impl const fn rr<...Ps, ...As>(seed: i64, f: const fn(...{Ps}) -> f64, args: ...{As for f}) -> i64
    requires each arg in arg_keys(As) { As[arg] == Ps[arg] } => 7
test observed { assert rr(1, scaled, price: 2.0) == 7 }
"#,
    )]);
    assert!(!checked.rejected, "{}", checked.report);
}

#[test]
fn a_float_default_does_not_erase_the_public_integer_literal_type() {
    // GEN-13: the full public inference still checks a dependent default.
    let checked = check(&[(
        "m.hgl",
        r#"module m
operator choose<T, const fallback: T = 2.5>(value: T) -> T
impl const fn choose(value: i64) -> i64 => value
test observed { let picked = choose(1) }
"#,
    )]);
    assert_eq!(checked.findings.len(), 1, "{}", checked.report);
    assert_eq!(checked.findings[0].code, "type/type.mismatch");
    assert_eq!(checked.findings[0].rule, "GEN-13");
}

#[test]
fn typed_values_and_type_requirements_keep_their_fixed_requests() {
    // GEN-2, GEN-8: a typed value is not a contextual literal.
    let checked = check(&[(
        "m.hgl",
        r#"module m
operator fixed<T, const on: bool = true>(value: T) -> T
impl const fn fixed(value: i64) -> f64 => 0.5
const fn observed(value: i64) { let picked = fixed(value) }
"#,
    )]);
    assert_eq!(checked.findings.len(), 1, "{}", checked.report);
    assert_eq!(checked.findings[0].code, "operator/operator.no_candidate");
    assert!(
        checked.findings[0].message.ends_with("and gives `i64`"),
        "{}",
        checked.report
    );
}
