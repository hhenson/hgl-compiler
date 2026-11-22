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
