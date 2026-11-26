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
fn a_typed_input_keeps_its_binding_beside_a_contextual_argument() {
    // GEN-2, GEN-6: the typed first input fixes candidate admission,
    // although the second literal can widen under that candidate.
    for option in ["", ", const on: bool = true"] {
        for (candidate, admitted) in [("i64", true), ("f64", false)] {
            let source = format!(
                "module m\noperator mixed<T, U{option}>(value: T, other: U) -> bool\nimpl const fn mixed(value: {candidate}, other: f64) -> bool => true\nconst fn observed(value: i64) {{ let picked = mixed(value, 1) }}\n"
            );
            let checked = check(&[("m.hgl", &source)]);
            assert_eq!(
                !checked.rejected, admitted,
                "{option}/{candidate}: {}",
                checked.report
            );
            if !admitted {
                assert_eq!(checked.findings.len(), 1, "{}", checked.report);
                assert_eq!(checked.findings[0].code, "operator/operator.no_candidate");
            }
        }
    }
}

/// GEN-7 admits generic-dependent literals. The unrelated fully concrete
/// call inside a generic declaration retains its existing request.
#[test]
fn generic_literal_calls_are_admitted_without_changing_concrete_body_calls() {
    for (index, body) in [
        "const fn observed<V>(items: list<V>, seed: V) -> V requires V in {i64, f64} => op(items, seed) + op(items, 1)",
        "fn observed<V>(items: list<V>, seed: V) -> V requires V in {i64, f64} => op(items, seed) + op(items, 1)",
        "const fn observed<V>(unused: V) { let picked = op([1.5, 2.0], 1) }",
    ].into_iter().enumerate() {
        for option in [false, true] {
            let generic = if option {
                ", const on: bool = true"
            } else {
                ""
            };
            let requires = if option { " requires on" } else { "" };
            let source = format!(
                "module m\noperator op<L, C{generic}>(items: C, zero: L? = null) -> L\nimpl const fn op<V{generic}>(items: list<V>, zero: V? = null) -> V{requires} => items[0]\n{body}\n"
            );
            let checked = check(&[("m.hgl", &source)]);
            assert_eq!(
                checked.rejected, option && index == 2,
                "{body}/{option}: {}",
                checked.report
            );
            if option && index == 2 {
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
    }
}

/// UNIT-8, GEN-2: inherited configuration defaults take the fitted
/// argument's type, including the payload of a nullable parameter.
#[test]
fn inherited_configuration_defaults_keep_the_fitted_literal_type() {
    let checked = check(&[(
        "m.hgl",
        r#"module m
operator seed<T>() -> list<T>
impl const fn seed() -> list<i64> => [5]
impl const fn seed() -> list<f64> => [2.5]
operator plain<T>{initial: list<T> = seed<T>()}(item: T) -> i64
    requires seed() -> list<T>
impl const fn plain{initial: list<i64>}(item: i64?) -> i64 => initial[0]
operator widened<T, const on: bool = true>{initial: list<T> = seed<T>()}(item: T) -> f64
    requires seed() -> list<T>
impl const fn widened<const on: bool = true>{initial: list<f64>}(item: f64?) -> f64 requires on => initial[0]
test observed { assert plain(1) == 5
    assert widened(1) == 2.5 }
"#,
    )]);
    assert!(!checked.rejected, "{}", checked.report);
}

/// GEN-2, GEN-5: a typed input fixes its generic in an output request,
/// even beside a literal flag. GEN-6 declaration validation is separate.
#[test]
fn typed_inputs_keep_result_requests_with_every_flag_spelling() {
    for call in [
        "fixed(value, true)",
        "fixed(value, flag)",
        "fixed(flag: true, value: value)",
        "fixed(value, !flag)",
        "fixed{on: true}(value, true)",
        "fixed{on: true}(value, flag)",
    ] {
        let source = format!(
            "module m\noperator fixed<T, const on: bool = true>(value: T, flag: bool) -> T\nimpl const fn fixed(value: i64, flag: bool) -> f64 => 2.5\ntest observed {{ let value: i64 = 3\nlet flag: bool = true\nlet picked = {call} }}\n"
        );
        let checked = check(&[("m.hgl", &source)]);
        assert_eq!(checked.findings.len(), 2, "{call}: {}", checked.report);
        assert_eq!(checked.findings[0].code, "operator/impl.result");
        assert_eq!(checked.findings[1].code, "operator/operator.no_candidate");
        assert!(
            checked.findings[1].message.ends_with("and gives `i64`"),
            "{}",
            checked.report
        );
    }
}

/// GEN-2, GEN-5, GEN-6: writing one generic leaves a literal open for
/// candidate fitting, while a typed argument still fixes its own type.
#[test]
fn a_partial_written_generic_keeps_typed_requests_and_literal_widening() {
    let checked = check(&[(
        "m.hgl",
        r#"module m
operator fx2<U, T>(flag: U, value: T) -> T
impl const fn fx2(flag: bool, value: i64) -> f64 => 2.5
test observed {
    let value: i64 = 3
    let picked = fx2<bool>(true, value)
}
"#,
    )]);
    assert_eq!(checked.findings.len(), 2, "{}", checked.report);
    assert_eq!(checked.findings[0].code, "operator/impl.result");
    assert_eq!(checked.findings[1].code, "operator/operator.no_candidate");
    let checked = check(&[(
        "m.hgl",
        r#"module m
operator same2<U, T>(flag: U, value: T) -> T
impl const fn same2(flag: bool, value: f64) -> f64 => value + 0.5
test observed { assert same2<bool>(true, 1) == 1.5 }
"#,
    )]);
    assert!(!checked.rejected, "{}", checked.report);
}

/// GEN-5, EXPR-9 and WRITE-7: report exactly the ambiguous literal in
/// a successful call. A typed input, sole viable reading or acknowledged
/// warning needs no report.
#[test]
fn a_viable_alternate_set_warns_at_the_selected_literal() {
    let checked = common::module(concat!(
        "operator pick<T>(x: T) -> i64\n",
        "impl const fn pick(x: list<i64, 2>) -> i64 => 1\n",
        "impl const fn pick(x: set<i64>) -> i64 => 2\n",
        "operator nested<T>(x: T) -> i64\n",
        "impl const fn nested(x: list<list<i64, 2>, 1>) -> i64 => 1\n",
        "impl const fn nested(x: list<set<i64>, 1>) -> i64 => 2\n",
        "const fn mixed() -> i64 => pick([1,2]) + nested([[3,4]])\n",
        "const fn specified() -> i64 => pick<set<i64>>([1,2])\n",
        "const fn only_set() -> i64 => pick([1,2,3])\n",
        "@suppress_warning(\"operator.sequence_reading\")\n",
        "const fn suppressed() -> i64 => pick([1,2])\n",
        "const fn only_list() -> i64 => pick([8,8])\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:8:33 warning/operator.sequence_reading GEN-5",
            "m.hgl:8:50 warning/operator.sequence_reading GEN-5",
        ],
        "{}",
        checked.report
    );
    assert!(
        checked.report.contains("let members: set<i64> = [3,4]"),
        "{}",
        checked.report
    );
}

/// GEN-5: after choosing the bracket reading, parameter and result type fits
/// settle the integer literal's precision before candidate conditions run.
#[test]
fn exact_integer_fits_count_within_the_selected_sequence_reading() {
    let operator = "operator pick<const on:bool>(xs:any, n:any)->i64\n";
    let exact = "impl const fn pick<const on:bool>(xs:list<i64>, n:i64)->i64 requires on => 1\n";
    let list = "impl const fn pick<const on:bool>(xs:list<i64>, n:f64)->i64 => 2\n";
    let set = "impl const fn pick<const on:bool>(xs:set<i64>, n:f64)->i64 => 3\n";
    let calls = concat!(
        "const fn inline()->i64 => pick{on:false}([3,8],11)\n",
        "const fn stored(xs:list<i64>)->i64 => pick{on:false}(xs,11)\n",
        "const fn floated()->i64 => pick{on:false}([3,8],11.0)\n",
        "const fn enabled()->i64 => pick{on:true}([3,8],11)\n",
    );
    for implementations in [[exact, list, set], [set, list, exact]] {
        let checked = common::module(&[operator, &implementations.concat(), calls].concat());
        assert_eq!(
            checked.summary(),
            [
                "m.hgl:6:27 operator/operator.no_candidate GEN-5",
                "m.hgl:7:39 operator/operator.no_candidate GEN-5",
                "m.hgl:8:43 warning/operator.sequence_reading GEN-5",
                "m.hgl:9:42 warning/operator.sequence_reading GEN-5",
            ],
            "{}",
            checked.report
        );
    }
}

/// GEN-5: an exact fit in a discarded set reading cannot block the widened
/// list. Its condition can also leave the list as the only admitted reading.
#[test]
fn a_disabled_set_type_fit_does_not_set_the_lists_precision() {
    let operator = "operator pick<const on:bool>(xs:any, n:any)->i64\n";
    let exact = "impl const fn pick<const on:bool>(xs:set<i64>, n:i64)->i64 requires on => 1\n";
    let list = "impl const fn pick<const on:bool>(xs:list<i64>, n:f64)->i64 => 2\n";
    let set = "impl const fn pick<const on:bool>(xs:set<i64>, n:f64)->i64 => 3\n";
    let call = "const fn chosen()->i64 => pick{on:false}([3,8],11)\n";
    let both = common::module(&[operator, exact, set, list, call].concat());
    assert_eq!(
        both.summary(),
        ["m.hgl:6:42 warning/operator.sequence_reading GEN-5"],
        "{}",
        both.report
    );
    let only = common::module(&[operator, exact, list, call].concat());
    assert!(only.findings.is_empty(), "{}", only.report);
}

/// GEN-5: an unresolved list tie cannot fall through to the set candidate.
/// Opposite choices at two independent arguments have no preferred ordering.
#[test]
fn tied_list_candidates_and_crossed_arguments_stay_ambiguous() {
    let checked = common::module(concat!(
        "operator tied<X,A,B>(x:X,a:A,b:B)->i64\n",
        "impl const fn tied(x:list<i64,2>,a:i64,b:any)->i64=>1\n",
        "impl const fn tied(x:list<i64,2>,a:any,b:i64)->i64=>2\n",
        "impl const fn tied(x:set<i64>,a:i64,b:i64)->i64=>3\n",
        "const fn first()->i64 => tied([8,13],1,2)\n",
        "operator crossed<A,B>(x:A,y:B)->i64\n",
        "impl const fn crossed(x:list<i64>,y:set<i64>)->i64=>1\n",
        "impl const fn crossed(x:set<i64>,y:list<i64>)->i64=>2\n",
        "const fn second()->i64 => crossed([1,2],[3,4])\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:6:26 operator/operator.ambiguous GEN-5",
            "m.hgl:10:27 operator/operator.ambiguous GEN-5",
        ],
        "{}",
        checked.report
    );
}

/// EXPR-9: constant string members must be distinct in a set. A
/// repeated member can still be read as a list without a choice warning.
#[test]
fn an_invalid_set_reading_does_not_warn_or_reject_a_valid_list() {
    let checked = common::module(concat!(
        "operator pick<T>(x:T)->i64\n",
        "impl const fn pick(x:list<str>)->i64 => 1\n",
        "impl const fn pick(x:set<str>)->i64 => 2\n",
        "const fn repeated()->i64 => pick([\"x\",\"x\"])\n",
        "const fn invalid()->i64 => pick<set<str>>([\"x\",\"x\"])\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:6:48 type/set.duplicate_member EXPR-9",],
        "{}",
        checked.report
    );
}

/// GEN-5: compare the complete combinations before numeric precision or
/// ordinary specificity. Either declaration order yields the same outcome.
#[test]
fn all_list_readings_dominate_crossed_ones_as_a_group() {
    let public = "operator mixed<A,B,C>(a:A,b:B,c:C)->i64\n";
    let first = "impl const fn mixed(a:list<i64>,b:set<i64>,c:i64)->i64 => 1\n";
    let second = "impl const fn mixed(a:set<i64>,b:list<i64>,c:f64)->i64 => 2\n";
    let both = "impl const fn mixed(a:list<i64>,b:list<i64>,c:f64)->i64 => 3\n";
    let call = "const fn value()->i64 => mixed([1],[2],7)\n";
    for (left, right) in [(first, second), (second, first)] {
        let checked = common::module(&[public, left, right, call].concat());
        assert_eq!(
            checked.summary(),
            ["m.hgl:5:26 operator/operator.ambiguous GEN-5"],
            "{}",
            checked.report
        );
        let checked = common::module(&[public, left, right, both, call].concat());
        assert_eq!(
            checked.summary(),
            [
                "m.hgl:6:32 warning/operator.sequence_reading GEN-5",
                "m.hgl:6:36 warning/operator.sequence_reading GEN-5",
            ],
            "{}",
            checked.report
        );
    }
}

/// ERR-15 selects the fewest result reads before GEN-5. A list
/// interpretation requiring an extra payload read cannot trigger WRITE-7.
#[test]
fn a_whole_result_precedes_a_competing_list_reading() {
    let checked = common::module(concat!(
        "operator pick<A,B>(xs:A,b:B)->i64\n",
        "impl const fn pick(xs:list<i64,1>,b:i64)->i64 => 1\n",
        "impl const fn pick(xs:set<i64>,b:result<i64,error>)->i64 => 2\n",
        "const fn kept(input:result<i64,error>)->i64 => pick([5],input)\n",
    ));
    assert!(checked.findings.is_empty(), "{}", checked.report);
}

/// GEN-5, EXPR-9: a dictionary's key and value each supply context to
/// their own literal. The containing dictionary has no list/set choice.
#[test]
fn dictionary_keys_and_values_warn_at_the_inner_literal() {
    let checked = common::module(concat!(
        "operator table<T>(xs:T)->i64\n",
        "impl const fn table(xs:dict<str,list<i64>>)->i64 => 1\n",
        "impl const fn table(xs:dict<str,set<i64>>)->i64 => 2\n",
        "operator keyed<T>(xs:T)->i64\n",
        "impl const fn keyed(xs:dict<list<i64>,str>)->i64 => 1\n",
        "impl const fn keyed(xs:dict<set<i64>,str>)->i64 => 2\n",
        "const fn item()->i64 => table([\"a\": [1,2]])\n",
        "const fn key()->i64 => keyed([[3,4]: \"b\"])\n",
        "const fn wanted()->i64 => table<dict<str,set<i64>>>([\"a\": [1,2]])\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:8:37 warning/operator.sequence_reading GEN-5",
            "m.hgl:9:31 warning/operator.sequence_reading GEN-5",
        ],
        "{}",
        checked.report
    );
}

/// EXPR-9, GEN-5: the public tuple supplies a list context for the
/// first slot. Its `any` slot still admits two collection readings.
#[test]
fn public_input_context_only_fixes_the_literals_it_types() {
    let checked = common::module(concat!(
        "operator pick<T>(xs:T)->i64\n",
        "impl const fn pick(xs:tuple<list<i64>,list<i64>>)->i64=>1\n",
        "impl const fn pick(xs:tuple<list<i64>,set<i64>>)->i64=>2\n",
        "const fn result_value()->i64 => pick<tuple<list<i64>,any>>(([1],[2]))\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:5:65 warning/operator.sequence_reading GEN-5"],
        "{}",
        checked.report
    );
}

/// ERR-2 supplies the success or failure payload's type from the
/// candidate. Compare the literals of both tagged arguments together.
#[test]
fn tagged_payload_choices_follow_the_candidate_result_type() {
    let checked = common::module(concat!(
        "operator pick<A,B>(x:A,y:B)->i64\n",
        "impl const fn pick(x:result<list<i64>,error>,y:result<bool,set<i64>>)->i64=>1\n",
        "impl const fn pick(x:result<set<i64>,error>,y:result<bool,list<i64>>)->i64=>2\n",
        "const fn crossed()->i64=>pick(ok([1]),err([2]))\n",
        "const fn selected()->i64=>pick<result<set<i64>,error>>(ok([1]),err([2]))\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:5:26 operator/operator.ambiguous GEN-5"],
        "{}",
        checked.report
    );
    let checked = common::module(concat!(
        "operator pick<A,B>(x:A,y:B)->i64\n",
        "impl const fn pick(x:result<list<i64>,error>,y:result<bool,set<i64>>)->i64=>1\n",
        "impl const fn pick(x:result<set<i64>,error>,y:result<bool,list<i64>>)->i64=>2\n",
        "impl const fn pick(x:result<list<i64>,error>,y:result<bool,list<i64>>)->i64=>3\n",
        "const fn both()->i64=>pick(ok([1]),err([2]))\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:6:31 warning/operator.sequence_reading GEN-5",
            "m.hgl:6:40 warning/operator.sequence_reading GEN-5",
        ],
        "{}",
        checked.report
    );
}

/// EXPR-9: brace collections have a fixed outer interpretation, but
/// context-dependent bracketed keys, values and members stay eligible.
#[test]
fn brace_collections_compare_only_their_nested_bracketed_literals() {
    let checked = common::module(concat!(
        "operator pick<T>(v:T)->i64\n",
        "impl const fn pick(v:dict<list<i64>,set<i64>>)->i64=>1\n",
        "impl const fn pick(v:dict<set<i64>,list<i64>>)->i64=>2\n",
        "const fn crossed()->i64=>pick({[1]:[2]})\n",
        "operator bags<T>(v:T)->i64\n",
        "impl const fn bags(v:set<list<i64,2>>)->i64=>1\n",
        "impl const fn bags(v:set<set<i64>>)->i64=>2\n",
        "const fn inner()->i64=>bags({[1,2],})\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:5:26 operator/operator.ambiguous GEN-5",
            "m.hgl:9:30 warning/operator.sequence_reading GEN-5",
        ],
        "{}",
        checked.report
    );
}

/// EXPR-17: the comprehension's result supplies its element context.
#[test]
fn a_comprehension_compares_its_entry_and_leaves_the_iterable_alone() {
    let checked = common::module(concat!(
        "operator pick<T>(v:T)->i64\n",
        "impl const fn pick(v:result<list<list<i64>>,error>)->i64=>1\n",
        "impl const fn pick(v:result<list<set<i64>>,error>)->i64=>2\n",
        "const fn selected()->i64=>pick(ok([for j in [3,4]: [j,5]]))\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:5:52 warning/operator.sequence_reading GEN-5"],
        "{}",
        checked.report
    );
}
