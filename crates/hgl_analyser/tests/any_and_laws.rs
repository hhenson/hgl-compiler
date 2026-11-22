//! Existential pack constraints and compile-time operator laws (GEN-3,
//! GEN-9, GEN-10, GEN-18, UNIT-7, UNIT-8).

mod common;

use common::check;

#[test]
fn any_accepts_a_matching_member_and_rejects_empty_or_nonmatching_packs() {
    // GEN-10: an existential over none is false; a later match suffices.
    let declarations = "module m\nconst fn present<...Ts>(xs: ...Ts) -> i64\n    requires any T in types(Ts) { T == i64 } => 1\n";
    let accepted = check(&[(
        "m.hgl",
        &format!("{declarations}const fn good() -> i64 => present(\"x\", 7)\n"),
    )]);
    assert!(!accepted.rejected, "{}", accepted.report);
    for arguments in ["", "\"x\", true"] {
        let rejected = check(&[(
            "m.hgl",
            &format!("{declarations}const fn bad() -> i64 => present({arguments})\n"),
        )]);
        assert!(
            rejected
                .findings
                .iter()
                .any(|finding| finding.code == "constraint/requires.unmet"),
            "{}",
            rejected.report
        );
    }
}

#[test]
fn any_binds_the_first_matching_type_and_preserves_explicit_bindings() {
    // GEN-3, GEN-10: failed trials cannot bind U; the first success does.
    let declarations = concat!(
        "module m\n",
        "native const fn first<U, ...Ts>(xs: ...Ts) -> U\n",
        "    requires any T in types(Ts) { T == list<U> }\n",
    );
    for body in [
        "const fn good() -> i64 { let selected = first(true, [1], [\"x\"])\n return selected }",
        "const fn good() -> str => first<str>([1], [\"x\"])",
    ] {
        let accepted = check(&[("m.hgl", &format!("{declarations}{body}\n"))]);
        assert!(!accepted.rejected, "{}", accepted.report);
    }
    let rejected = check(&[(
        "m.hgl",
        &format!(
            "{declarations}const fn bad() -> str {{ let selected = first([1], [\"x\"])\n return selected }}\n"
        ),
    )]);
    assert!(rejected.rejected, "the first split list fixes U as i64");
}

#[test]
fn a_grouped_pack_view_preserves_existential_binding_inference() {
    // GEN-3, GEN-10, EXPR-1: inference and checking accept the same grouped view.
    for view in ["types(Ts)", "(types(Ts))", "((types((Ts))))"] {
        let source = format!(
            "module m\nnative const fn first<U, ...Ts>(xs: ...Ts) -> U requires any T in {view} {{ T == list<U> }}\nconst fn good() -> i64 {{ let selected = first(true, [1], [\"later\"])\n return selected }}\n"
        );
        let accepted = check(&[("m.hgl", &source)]);
        assert!(!accepted.rejected, "{view}: {}", accepted.report);
    }
}

#[test]
fn any_infers_a_generic_list_length_from_either_equality_operand() {
    // GEN-3, GEN-10: a const generic nested in a type is unresolved
    // data too; the concrete pack member is the known side in either order.
    for equality in ["T == list<i64, N>", "list<i64, N> == T"] {
        let declarations = format!(
            "module m\nconst fn arity<const N: i64, ...Ts>(xs: ...Ts) -> i64 requires any T in types(Ts) {{ {equality} }} => N\n"
        );
        let accepted = check(&[(
            "m.hgl",
            &format!("{declarations}const fn good(pair: list<i64, 2>) -> i64 => arity(pair)\n"),
        )]);
        assert!(!accepted.rejected, "{equality}: {}", accepted.report);
        let rejected = check(&[(
            "m.hgl",
            &format!("{declarations}const fn bad(pair: list<i64, 2>) -> i64 => arity<3>(pair)\n"),
        )]);
        assert!(
            rejected
                .findings
                .iter()
                .any(|finding| finding.code == "constraint/requires.unmet"),
            "{equality}: {}",
            rejected.report
        );
    }
}

#[test]
fn an_unmatched_binding_any_rejects_before_reporting_an_unresolved_result() {
    // GEN-3, GEN-10: a known shape mismatch settles the existential,
    // even though its missing binding also leaves the result unknown.
    let declarations = concat!(
        "module m\n",
        "native const fn first<U, ...Ts>(xs: ...Ts) -> U requires any T in types(Ts) { T == list<U> }\n",
        "operator inspect<T>(x: T) -> i64\n",
        "impl const fn inspect<T>(x: T) -> i64 => 1\n",
    );
    for body in [
        "const fn bad() -> i64 { let selected = first(true, 1)\n return 1 }",
        "const fn bad() -> i64 { let selected = first()\n return 1 }",
        "const fn bad() -> i64 => inspect(first(true, 1))",
    ] {
        let rejected = check(&[("m.hgl", &format!("{declarations}{body}\n"))]);
        assert!(
            rejected
                .findings
                .iter()
                .any(|finding| finding.code == "constraint/requires.unmet"),
            "{}",
            rejected.report
        );
        assert!(
            !rejected
                .findings
                .iter()
                .any(|finding| finding.code == "type/generic.unresolved"),
            "{}",
            rejected.report
        );
        assert!(
            !rejected.report.contains("cannot judge"),
            "{}",
            rejected.report
        );
    }
}

#[test]
fn the_keyed_example_reports_an_unmatched_existential_at_the_call() {
    // GEN-10's keyed illustration: neither scalar argument can be dict<K, V>.
    let declarations = concat!(
        "module m\n",
        "const fn keyed<K, R, ...Ps, ...As>(f: const fn(...{Ps}) -> R, args: ...{As for f}) -> dict<K, R>\n",
        " requires any arg in arg_keys(As) { As[arg] == dict<K, Ps[arg]> }\n",
        " && each arg in arg_keys(As) { As[arg] in {Ps[arg], dict<K, Ps[arg]>} } => []\n",
        "const fn scaled(price: f64, factor: f64) -> f64 => price * factor\n",
    );
    let rejected = check(&[(
        "m.hgl",
        &format!(
            "{declarations}const fn bad() -> i64 {{ let selected = keyed(scaled, 1.0, 2.0)\n return 1 }}\n"
        ),
    )]);
    assert!(
        rejected
            .findings
            .iter()
            .any(|finding| finding.code == "constraint/requires.unmet"),
        "{}",
        rejected.report
    );
    assert!(
        !rejected
            .findings
            .iter()
            .any(|finding| finding.code == "type/generic.unresolved"),
        "{}",
        rejected.report
    );
    assert!(
        !rejected.report.contains("cannot judge"),
        "{}",
        rejected.report
    );
}

#[test]
fn a_bool_generic_is_a_requirement_term_in_functions_and_candidates() {
    // GEN-3: bare values compose by !, && and ||, and false rejects a call.
    let declarations =
        "module m\nconst fn gated<const A: bool, const B: bool>() -> i64 requires A && !B => 1\n";
    let accepted = check(&[(
        "m.hgl",
        &format!("{declarations}const fn good() -> i64 => gated<true, false>()\n"),
    )]);
    assert!(!accepted.rejected, "{}", accepted.report);
    for arguments in ["false, false", "true, true"] {
        let rejected = check(&[(
            "m.hgl",
            &format!("{declarations}const fn bad() -> i64 => gated<{arguments}>()\n"),
        )]);
        assert!(
            rejected
                .findings
                .iter()
                .any(|finding| finding.code == "constraint/requires.unmet"),
            "{}",
            rejected.report
        );
    }
    let rejected = check(&[(
        "m.hgl",
        "module m\nconst fn bad<const N: i64>() -> i64 requires N => 1\n",
    )]);
    assert!(
        rejected
            .findings
            .iter()
            .any(|finding| finding.rule == "GEN-3"),
        "{}",
        rejected.report
    );
}

const LAWS: &str = concat!(
    "module m\n",
    "operator plus<T>(a: T, b: T) -> T properties<i64> { associative, commutative }\n",
    "impl const fn plus(a: i64, b: i64) -> i64 => a + b\n",
    "impl const fn plus(a: f64, b: f64) -> f64 => a + b\n",
    "const fn ordinary(a: i64, b: i64) -> i64 => a + b\n",
    "operator fold<V, const law: bool = associative(f)>(f: const fn(V, V) -> V, a: V, b: V) -> V\n",
    "impl const fn fold<V, const law: bool = associative(f)>(f: const fn(V, V) -> V, a: V, b: V) -> V requires law => f(a, b)\n",
);

#[test]
fn a_law_default_queries_the_public_operator_and_allows_a_caller_override() {
    // GEN-9, GEN-18: metadata belongs to the operator and concrete domain.
    for call in [
        "fold(plus, 1, 2)",
        "fold{law: true}(ordinary, 1, 2)",
        "fold{law: true}(fn(a, b) => a + b, 1, 2)",
    ] {
        let accepted = check(&[(
            "m.hgl",
            &format!("{LAWS}const fn good() -> i64 => {call}\n"),
        )]);
        assert!(!accepted.rejected, "{call}: {}", accepted.report);
    }
    for (ty, call) in [
        ("i64", "fold(ordinary, 1, 2)"),
        ("i64", "fold(fn(a, b) => a + b, 1, 2)"),
        ("i64", "fold{law: false}(plus, 1, 2)"),
        ("f64", "fold(plus, 1.0, 2.0)"),
    ] {
        let rejected = check(&[(
            "m.hgl",
            &format!("{LAWS}const fn bad() -> {ty} => {call}\n"),
        )]);
        assert!(
            rejected
                .findings
                .iter()
                .any(|finding| finding.code == "operator/operator.no_candidate"),
            "{call}: {}",
            rejected.report
        );
    }
}

#[test]
fn a_direct_law_requirement_uses_the_function_given_at_the_call() {
    // GEN-18: both law predicates are constraint calls of a formal function.
    let declarations = format!(
        "{LAWS}const fn both<V>(f: const fn(V, V) -> V, a: V, b: V) -> V requires associative(f) && commutative(f) => f(a, b)\n"
    );
    let accepted = check(&[(
        "m.hgl",
        &format!("{declarations}const fn good() -> i64 => both(plus, 1, 2)\n"),
    )]);
    assert!(!accepted.rejected, "{}", accepted.report);
    let rejected = check(&[(
        "m.hgl",
        &format!("{declarations}const fn bad() -> i64 => both(ordinary, 1, 2)\n"),
    )]);
    assert!(
        rejected
            .findings
            .iter()
            .any(|finding| finding.code == "constraint/requires.unmet"),
        "{}",
        rejected.report
    );
}

#[test]
fn a_law_default_is_a_bool_and_other_defaults_cannot_read_formal_arguments() {
    // GEN-18, UNIT-7, UNIT-8: only the specified law default has this scope.
    let rejected = check(&[(
        "m.hgl",
        "module m\nconst fn bad<const law: i64 = associative(f)>(f: const fn(i64, i64) -> i64) -> i64 => law\n",
    )]);
    assert!(
        rejected
            .findings
            .iter()
            .any(|finding| finding.rule == "GEN-18"),
        "{}",
        rejected.report
    );
    let rejected = check(&[(
        "m.hgl",
        "module m\nconst fn bad<const n: i64 = value>(value: i64) -> i64 => n\n",
    )]);
    assert!(rejected.rejected, "ordinary defaults still exclude formals");
}

#[test]
fn law_queries_and_defaults_follow_a_forwarded_function_identity() {
    // GEN-18, FN-17: the given operator identity follows a callback through
    // a generic body; a named function or an anonymous one has no law.
    let declarations = format!(
        "{LAWS}const fn forward<V>(f: const fn(V, V) -> V, a: V, b: V) -> V requires associative(f) => fold(f, a, b)\n"
    );
    let accepted = check(&[(
        "m.hgl",
        &format!("{declarations}const fn good() -> i64 => forward(plus, 1, 2)\n"),
    )]);
    assert!(!accepted.rejected, "{}", accepted.report);
    let rejected = check(&[(
        "m.hgl",
        &format!("{declarations}const fn bad() -> i64 => forward(ordinary, 1, 2)\n"),
    )]);
    assert!(
        rejected.rejected,
        "a forwarded ordinary function has no law"
    );
}

#[test]
fn keyed_any_binds_the_first_split_key_and_each_checks_the_rest() {
    // GEN-10, GEN-17: the specification's keyed fragment resolves K
    // from the first split argument, and rejects a different key later.
    let declarations = concat!(
        "module m\n",
        "fn scaled(price: f64, factor: f64) -> f64 => price * factor\n",
        "operator keyed<K, R, ...Ps, ...As>(f: fn(...{Ps}) -> R, args: ...{As for f}) -> dict<K, R>\n",
        "native impl fn keyed<K, R, ...Ps, ...As>(f: fn(...{Ps}) -> R, args: ...{As for f}) -> dict<K, R>\n",
        "    requires any arg in arg_keys(As) { As[arg] == dict<K, Ps[arg]> }\n",
        "          && each arg in arg_keys(As) { As[arg] in {Ps[arg], dict<K, Ps[arg]>} }\n",
    );
    let accepted = check(&[(
        "m.hgl",
        &format!(
            "{declarations}fn good(prices: dict<str, f64>, two: f64) -> dict<str, f64> {{\n let selected = keyed(scaled, prices, two)\n return selected\n}}\n"
        ),
    )]);
    assert!(!accepted.rejected, "{}", accepted.report);
    for (parameters, arguments) in [
        ("one: f64, two: f64", "one, two"),
        (
            "prices: dict<str, f64>, by_id: dict<i64, f64>",
            "prices, by_id",
        ),
    ] {
        let rejected = check(&[(
            "m.hgl",
            &format!(
                "{declarations}fn bad({parameters}) -> dict<str, f64> => keyed(scaled, {arguments})\n"
            ),
        )]);
        assert!(
            rejected
                .findings
                .iter()
                .any(|finding| finding.code == "operator/operator.no_candidate"),
            "{}",
            rejected.report
        );
    }
}

#[test]
fn a_negated_or_disjunctive_any_only_tests_supplied_bindings() {
    // GEN-3, GEN-10: only a positive conjunct can infer one generic.
    for condition in [
        "!any T in types(Ts) { T == list<U> }",
        "any T in types(Ts) { T == list<U> } || U == str",
    ] {
        let source = format!(
            "module m\nnative const fn selected<U, ...Ts>(xs: ...Ts) -> U requires {condition}\nconst fn bad() -> i64 {{\n let value = selected([1])\n return value\n}}\n"
        );
        let rejected = check(&[("m.hgl", &source)]);
        assert!(
            rejected
                .findings
                .iter()
                .any(|finding| finding.code == "type/generic.unresolved"),
            "{}",
            rejected.report
        );
    }
}

#[test]
fn a_law_dependent_wrapper_is_judged_at_its_declaration() {
    // GEN-7, GEN-18; maintainer's #367 ruling, cases 1 through 4.
    let missing = check(&[(
        "m.hgl",
        &format!(
            "{LAWS}const fn forward<V>(f: const fn(V, V) -> V, a: V, b: V) -> V => fold(f, a, b)\n"
        ),
    )]);
    assert!(
        missing
            .findings
            .iter()
            .any(|finding| finding.code == "operator/operator.no_candidate"),
        "{}",
        missing.report
    );
    let exhaustive = format!(
        "{LAWS}impl const fn fold<V, const law: bool = associative(f)>(f: const fn(V, V) -> V, a: V, b: V) -> V requires !law => f(b, a)\nconst fn forward<V>(f: const fn(V, V) -> V, a: V, b: V) -> V => fold(f, a, b)\nconst fn yes() -> i64 => forward(plus, 1, 2)\nconst fn no() -> i64 => forward(ordinary, 1, 2)\n"
    );
    let rejected = check(&[("m.hgl", &exhaustive)]);
    assert!(
        rejected
            .findings
            .iter()
            .any(|finding| finding.code == "operator/operator.no_candidate"),
        "{}",
        rejected.report
    );
}

#[test]
fn quantifier_types_are_bound_inside_nested_type_expressions() {
    // GEN-10: the bound type is a type wherever the predicate writes it.
    let accepted = check(&[(
        "m.hgl",
        "module m\nconst fn nested<...Ts>(xs: ...Ts) -> i64 requires any T in types(Ts) { list<T> == list<i64> } => 1\nconst fn good() -> i64 => nested(true, 2)\n",
    )]);
    assert!(!accepted.rejected, "{}", accepted.report);
    let rejected = check(&[(
        "m.hgl",
        "module m\noperator bad<T>(a: T, b: T) -> T properties<T> { associative }\n",
    )]);
    assert!(
        rejected
            .findings
            .iter()
            .any(|finding| finding.rule == "GEN-9"),
        "{}",
        rejected.report
    );
}

#[test]
fn a_bool_requirement_authorizes_forwarding_that_option() {
    // GEN-3, GEN-6, GEN-7: a bare bool requirement binds true in the body.
    let source = format!(
        "{LAWS}const fn stated<V, const enabled: bool>(f: const fn(V, V) -> V, a: V, b: V) -> V requires enabled => fold{{law: enabled}}(f, a, b)\nconst fn good() -> i64 => stated{{enabled: true}}(ordinary, 1, 2)\n"
    );
    let accepted = check(&[("m.hgl", &source)]);
    assert!(!accepted.rejected, "{}", accepted.report);
}

#[test]
fn an_existential_can_test_an_inequality_without_binding() {
    // GEN-3, GEN-10, GEN-17: the pinned constraint grammar includes !=.
    let accepted = check(&[(
        "m.hgl",
        "module m\nconst fn different<...Ts>(xs: ...Ts) -> i64 requires any T in types(Ts) { T != i64 } => 1\nconst fn good() -> i64 => different(1, true)\n",
    )]);
    assert!(!accepted.rejected, "{}", accepted.report);
}

#[test]
fn an_inequality_requirement_reads_a_named_constant_as_a_value() {
    // GEN-3, GEN-17, UNIT-13: != uses the same constant operands as ==.
    for condition in ["N != LIMIT", "LIMIT != N"] {
        let declarations = format!(
            "module m\nconst LIMIT: i64 = 2\nconst fn gated<const N: i64>() -> i64 requires {condition} => 1\n"
        );
        let accepted = check(&[(
            "m.hgl",
            &format!("{declarations}const fn good() -> i64 => gated<3>()\n"),
        )]);
        assert!(!accepted.rejected, "{}", accepted.report);
        let rejected = check(&[(
            "m.hgl",
            &format!("{declarations}const fn bad() -> i64 => gated<2>()\n"),
        )]);
        assert!(
            rejected
                .findings
                .iter()
                .any(|finding| finding.code == "constraint/requires.unmet"),
            "{}",
            rejected.report
        );
    }
}

#[test]
fn law_queries_and_bare_terms_reject_runtime_formal_values() {
    // GEN-3, GEN-18: only a function formal supplies a law default;
    // a bare bool term names a generic value, not a runtime parameter.
    for source in [
        "module m\nconst fn bad<const law: bool = associative(value)>(value: i64) -> i64 => value\n",
        "module m\nconst fn bad(value: bool) -> bool requires value => value\n",
    ] {
        let rejected = check(&[("m.hgl", source)]);
        assert!(
            rejected
                .findings
                .iter()
                .any(|finding| finding.code == "type/type.mismatch"),
            "{}",
            rejected.report
        );
    }
}

#[test]
fn a_law_requirement_rejects_a_non_function_operand_at_its_declaration() {
    // GEN-18: the operand is a formal parameter that takes a function,
    // just as in a law default; no call is needed to reject the declaration.
    for law in ["associative", "commutative"] {
        for declaration in [
            format!("const fn bad(a: i64) -> i64 requires {law}(a) => a"),
            format!("const fn bad<V>(a: V) -> V requires {law}(V) => a"),
        ] {
            let rejected = check(&[("m.hgl", &format!("module m\n{declaration}\n"))]);
            assert!(
                rejected.findings.iter().any(|finding| finding.rule == "GEN-18" && finding.code == "type/type.mismatch"),
                "{}",
                rejected.report
            );
        }
    }
}

#[test]
fn a_user_operator_named_after_a_law_keeps_its_named_requirement() {
    // GEN-16, UNIT-10: an operator in scope keeps ordinary name resolution.
    let accepted = check(&[(
        "m.hgl",
        "module m\noperator associative<T>(x: T) -> bool\nimpl const fn associative(x: i64) -> bool => true\nconst fn bare<V>(x: V) -> bool requires associative(V) => associative(x)\nconst fn arrow<V>(x: V) -> bool requires associative(V) -> bool => associative(x)\n",
    )]);
    assert!(!accepted.rejected, "{}", accepted.report);
}

#[test]
fn an_imported_operator_named_after_a_law_keeps_callability_at_calls() {
    // GEN-16, UNIT-2, UNIT-10: a named requirement asks whether the
    // operator is callable; it does not evaluate that operator's result.
    for law in ["associative", "commutative"] {
        for requirement in [format!("{law}(V)"), format!("{law}(V) -> bool")] {
            let operator = format!(
                "module ops\nexport operator {law}<T>(x: T) -> bool\nimpl const fn {law}(x: i64) -> bool => false\n"
            );
            let functions = format!(
                "module m\nuse ops::{{{law}}}\nconst fn through<V>(x: V) -> bool requires {requirement} => {law}(x)\n"
            );
            let accepted = check(&[
                ("ops.hgl", &operator),
                (
                    "m.hgl",
                    &format!("{functions}const fn good() -> bool => through(1)\n"),
                ),
            ]);
            assert!(!accepted.rejected, "{requirement}: {}", accepted.report);
            let rejected = check(&[
                ("ops.hgl", &operator),
                (
                    "m.hgl",
                    &format!("{functions}const fn bad() -> bool => through(\"no candidate\")\n"),
                ),
            ]);
            assert!(
                rejected
                    .findings
                    .iter()
                    .any(|finding| finding.code == "constraint/requires.unmet"),
                "{requirement}: {}",
                rejected.report
            );
        }
    }
}

#[test]
fn law_branches_require_stated_constraints_or_an_explicit_bool_membership() {
    // GEN-7, GEN-18: corrected ruling in merged specification #224.
    let alternatives = format!(
        "{LAWS}impl const fn fold<V, const law: bool = associative(f)>(f: const fn(V, V) -> V, a: V, b: V) -> V requires !law => f(b, a)\n"
    );
    let source = format!(
        "{alternatives}const fn positive<V>(f: const fn(V, V) -> V, a: V, b: V) -> V requires associative(f) => fold(f, a, b)\nconst fn negative<V>(f: const fn(V, V) -> V, a: V, b: V) -> V requires !associative(f) => fold(f, a, b)\nconst fn both<V, const law: bool = associative(f)>(f: const fn(V, V) -> V, a: V, b: V) -> V requires law in {{true, false}} => fold{{law: law}}(f, a, b)\nconst fn yes() -> i64 => positive(plus, 1, 2)\nconst fn no() -> i64 => negative(ordinary, 1, 2)\nconst fn yes_member() -> i64 => both(plus, 1, 2)\nconst fn no_member() -> i64 => both(ordinary, 1, 2)\n"
    );
    let accepted = check(&[("m.hgl", &source)]);
    assert!(!accepted.rejected, "{}", accepted.report);
    let rejected = check(&[(
        "m.hgl",
        &format!(
            "{alternatives}const fn bare(f: const fn(i64, i64) -> i64) -> i64 => fold(f, 1, 2)\n"
        ),
    )]);
    assert!(
        rejected
            .findings
            .iter()
            .any(|finding| finding.code == "operator/operator.no_candidate"),
        "{}",
        rejected.report
    );
}

#[test]
fn parentheses_inside_a_negated_law_preserve_the_guarantee() {
    // GEN-7, GEN-15, GEN-18: !(law(f)) and !law(f) state the same bound.
    let source = format!(
        "{LAWS}impl const fn fold<V, const law: bool = associative(f)>(f: const fn(V, V) -> V, a: V, b: V) -> V requires !law => f(b, a)\nconst fn grouped<V>(f: const fn(V, V) -> V, a: V, b: V) -> V requires !((associative(f))) => fold(f, a, b)\nconst fn good() -> i64 => grouped(ordinary, 1, 2)\n"
    );
    let accepted = check(&[("m.hgl", &source)]);
    assert!(!accepted.rejected, "{}", accepted.report);
}

#[test]
fn parentheses_inside_a_negated_bool_preserve_the_forwarded_binding() {
    // GEN-3, GEN-7, GEN-15: grouping preserves the false bound of !enabled.
    for term in ["!enabled", "!(enabled)", "!((enabled))"] {
        let declarations = format!(
            "{LAWS}impl const fn fold<V, const law: bool = associative(f)>(f: const fn(V, V) -> V, a: V, b: V) -> V requires !law => f(b, a)\nconst fn stated<V, const enabled: bool>(f: const fn(V, V) -> V, a: V, b: V) -> V requires {term} => fold{{law: enabled}}(f, a, b)\n"
        );
        let accepted = check(&[(
            "m.hgl",
            &format!(
                "{declarations}const fn good() -> i64 => stated{{enabled: false}}(ordinary, 1, 2)\n"
            ),
        )]);
        assert!(!accepted.rejected, "{term}: {}", accepted.report);
        let rejected = check(&[(
            "m.hgl",
            &format!(
                "{declarations}const fn bad() -> i64 => stated{{enabled: true}}(ordinary, 1, 2)\n"
            ),
        )]);
        assert!(
            rejected
                .findings
                .iter()
                .any(|finding| finding.code == "constraint/requires.unmet"),
            "{term}: {}",
            rejected.report
        );
    }
}
