//! The analyser against the rules on named operator requirements: what
//! a declaration requires of its generic parameters is callable in its
//! body, and is tested where a call selects it. Each test cites its
//! rule.
//!
//! A finding is written `file:line:column category/code rule`.

mod common;

use common::{Checked, body, locals, module};
use hgl_analyser::Detail;
use hgl_analyser::hir::{Answer, Body, Callee, Expr, ExprKind, Stmt, ValueCategory};

/// No lines.
const NONE: [&str; 0] = [];

/// GEN-3 and GEN-7: a generic declaration must support each type admitted
/// by all of its conjoined memberships and equalities, in either order.
#[test]
fn ordinary_finite_requirements_intersect() {
    for (condition, accepted) in [
        ("T in {i64, f64} && T in {i64, str}", true),
        ("T in {i64, str} && T in {i64, f64}", true),
        ("T in {str, i64} && T == i64", true),
        ("i64 == T && T in {str, i64}", true),
        ("T in {str, i64} && T in {str, f64}", false),
        ("T == i64 && T == f64", false),
        ("T == f64 && T == i64", false),
    ] {
        let checked = module(&format!(
            "const fn difference<T>(x: T, y: T) -> T requires {condition} => x - y\n"
        ));
        if accepted {
            assert_eq!(checked.summary(), NONE, "{condition}\n{}", checked.report);
        } else {
            assert_eq!(checked.findings.len(), 1, "{condition}\n{}", checked.report);
            assert_eq!(checked.findings[0].code, "operator/operator.no_candidate");
        }
    }
}

/// GEN-3, GEN-7 and GEN-17: carry finite bounds across positive equality
/// chains. A function and an operator can each use the resulting type.
#[test]
fn transitive_ordinary_and_reflected_bounds_prove_calls() {
    let definitions = concat!(
        "const left: str = \"left\"\nconst right: str = \"right\"\n",
        "const fn need_float<X>(x: X) -> X requires X == f64 => x\n",
        "operator pick<X>(x: X) -> X\nimpl const fn pick(x: f64) -> f64 => x\n",
        "const fn proof<T, U, ...P>(f: const fn(...{P}) -> i64, x: T, u: U, y: P[left]) -> f64\n",
    );
    for condition in [
        "T in {f64, i64} && T == U && U == P[left] && U in {str, f64}",
        "U in {str, f64} && P[left] == U && U == T && T in {f64, i64}",
        "T in {f64, i64} && T == P[left] && P[left] == P[right] && P[right] in {str, f64}",
        "P[right] in {str, f64} && P[right] == P[left] && P[left] == T && T in {f64, i64}",
    ] {
        // The wrapper returns a fixed type so its two proof obligations can
        // use different inputs without adding a conversion between them.
        let source = format!(
            "{definitions}    requires {condition} {{\n    let v: T = need_float<T>(x)\n    let w: P[left] = pick(y)\n    return 2.0\n}}\n"
        );
        let checked = module(&source);
        assert_eq!(checked.summary(), NONE, "{condition}\n{}", checked.report);
    }
}

/// GEN-7 and GEN-17: the same-type pairs have candidates. Independent
/// reflected keys also admit mixed pairs, for which no candidate exists.
#[test]
fn explicitly_equal_keys_share_one_trial_type() {
    let definitions = concat!(
        "const a: str = \"a\"\nconst b: str = \"b\"\n",
        "operator pair<X, Y>(x: X, y: Y) -> i64\n",
        "impl const fn pair(x: i64, y: i64) -> i64 => 1\n",
        "impl const fn pair(x: f64, y: f64) -> i64 => 2\n",
        "const fn proof<...P>(cb: const fn(...{P}) -> i64, x: P[a], y: P[b]) -> i64\n",
        "    requires P[a] in {i64, f64} && P[b] in {i64, f64}",
    );
    for (condition, accepted) in [
        (" && P[a] == P[b]", true),
        (" && P[b] == P[a]", true),
        (" && (P[a] == P[b] || P[a] == i64)", false),
        ("", false),
    ] {
        let checked = module(&format!("{definitions}{condition} => pair(x, y)\n"));
        if accepted {
            assert_eq!(checked.summary(), NONE, "{condition}\n{}", checked.report);
        } else {
            assert_eq!(checked.findings.len(), 1, "{condition}\n{}", checked.report);
            assert_eq!(checked.findings[0].code, "operator/operator.no_candidate");
        }
    }
}

/// GEN-3, GEN-7 and GEN-17: substituting a finite key value can join
/// two previously separate equality classes. A different possible key
/// still allows independent types for the fixed keys.
#[test]
fn a_trial_joins_bounds_that_name_the_same_key() {
    let declarations = concat!(
        "const a: str = \"spot\"\nconst b: str = \"rate\"\n",
        "operator pair<X, Y>(x: X, y: Y) -> i64\n",
        "impl const fn pair(x: i64, y: i64) -> i64 => 1\n",
        "impl const fn pair(x: f64, y: f64) -> i64 => 2\n",
        "const fn proof<...P, const k: str>(cb: const fn(...{P}) -> i64, x: P[a], y: P[b]) -> i64\n",
    );
    for (condition, accepted) in [
        (
            "P[a] in {i64, f64} && P[k] == P[b] && P[k] in {i64, f64} && k in {\"spot\"}",
            true,
        ),
        (
            "k in {\"spot\"} && P[k] in {i64, f64} && P[k] == P[b] && P[a] in {i64, f64}",
            true,
        ),
        (
            "P[a] in {i64, f64} && P[k] == P[b] && P[k] in {i64, f64} && k in {\"spot\", \"other\"}",
            false,
        ),
    ] {
        let checked = module(&format!(
            "{declarations}    requires {condition} => pair(x, y)\n"
        ));
        if accepted {
            assert_eq!(checked.summary(), NONE, "{condition}\n{}", checked.report);
        } else {
            assert_eq!(checked.findings.len(), 1, "{condition}\n{}", checked.report);
            assert_eq!(checked.findings[0].code, "operator/operator.no_candidate");
        }
    }
}

/// GEN-3, GEN-7 and GEN-17: a reflected member's stated type can
/// authorize arithmetic. Every alternative must admit it.
#[test]
fn a_reflected_members_type_proves_arithmetic() {
    let definitions = concat!(
        "const key: str = \"item\"\n",
        "const fn total<...Ps>(f: const fn(...{Ps}) -> i64, ",
        "a: Ps[key], b: Ps[key]) -> Ps[key]\n",
    );
    for condition in [
        "Ps[key] == i64",
        "Ps[key] in {i64, f64}",
        "each k in arg_keys(Ps) { Ps[k] == i64 }",
    ] {
        let checked = module(&format!("{definitions}    requires {condition} => a + b\n"));
        assert_eq!(checked.summary(), NONE, "{condition}\n{}", checked.report);
    }
    for condition in [
        "Ps[key] in {i64, bool}",
        "Ps[key] != str",
        "any k in arg_keys(Ps) { Ps[k] == i64 }",
    ] {
        let checked = module(&format!("{definitions}    requires {condition} => a + b\n"));
        assert_eq!(checked.findings.len(), 1, "{condition}\n{}", checked.report);
        assert_eq!(checked.findings[0].code, "operator/operator.no_candidate");
    }
    let symbolic = module(concat!(
        "const fn total<const key: str, ...Ps>(f: const fn(...{Ps}) -> i64, ",
        "a: Ps[key], b: Ps[key]) -> Ps[key]\n",
        "    requires Ps[key] in {i64, f64} => a + b\n",
    ));
    assert_eq!(symbolic.summary(), NONE, "{}", symbolic.report);
}

/// GEN-7 and GEN-17: a member's equality and finite membership cooperate
/// whether the finite types are stated for the member or for its type.
#[test]
fn reflected_member_conjuncts_prove_the_same_call_in_either_order() {
    let definitions = concat!(
        "const key: str = \"item\"\n",
        "const fn subtract<T, ...Ps>(f: const fn(...{Ps}) -> i64, ",
        "t: T, a: Ps[key]) -> Ps[key]\n",
    );
    for condition in [
        "Ps[key] in {i64, f64}",
        "Ps[key] in {i64, f64} && Ps[key] == T",
        "Ps[key] == T && Ps[key] in {i64, f64}",
        "T in {i64, f64} && Ps[key] == T",
        "Ps[key] == T && T in {i64, f64}",
    ] {
        let checked = module(&format!("{definitions}    requires {condition} => a - a\n"));
        assert_eq!(checked.summary(), NONE, "{condition}\n{}", checked.report);
    }
}

/// GEN-3, GEN-7 and GEN-17: admit precisely the ordinary trials that can
/// share a type with this reflected member. Both call forms need evidence.
#[test]
fn reflected_members_and_parameters_keep_only_their_common_types() {
    let declarations = concat!(
        "const key: str = \"item\"\n",
        "const fn only_float<X>(x: X) -> X requires X == f64 => x\n",
        "operator pick<X>(x: X) -> X\n",
        "impl const fn pick(x: f64) -> f64 => x\n",
        "const fn caller<T, ...Ps>(f: const fn(...{Ps}) -> i64, t: T, x: Ps[key]) -> ",
    );
    for (result, call, rejected) in [
        (
            "Ps[key]",
            "only_float<Ps[key]>(x)",
            "constraint/requires.unmet",
        ),
        ("Ps[key]", "pick(x)", "operator/operator.no_candidate"),
        ("T", "only_float<T>(t)", "constraint/requires.unmet"),
        ("T", "pick(t)", "operator/operator.no_candidate"),
    ] {
        for (condition, admitted) in [
            (
                "T in {i64, f64} && Ps[key] in {str, f64} && Ps[key] == T",
                true,
            ),
            (
                "T == Ps[key] && Ps[key] in {f64, str} && T in {f64, i64}",
                true,
            ),
            (
                "each k in arg_keys(Ps) { Ps[k] in {str, f64} } && Ps[key] == T && T in {i64, f64}",
                true,
            ),
            (
                "T in {str, f64} && Ps[key] in {str, f64} && Ps[key] == T",
                false,
            ),
            (
                "T in {i64, bool} && Ps[key] in {str, f64} && Ps[key] == T",
                false,
            ),
            ("T in {i64, f64} && Ps[key] in {str, f64}", false),
        ] {
            let checked = module(&format!(
                "{declarations}{result}\n    requires {condition} => {call}\n"
            ));
            if admitted {
                assert_eq!(checked.summary(), NONE, "{condition}\n{}", checked.report);
            } else {
                assert_eq!(checked.findings.len(), 1, "{condition}\n{}", checked.report);
                assert_eq!(
                    checked.findings[0].code, rejected,
                    "{condition}\n{}",
                    checked.report
                );
            }
        }
    }
}

/// GEN-3 and GEN-7: two parameters equated to one member have to agree,
/// even when only one parameter reaches the callee. Distinct keys do not.
#[test]
fn parameters_bounded_through_the_same_member_constrain_each_other() {
    let declarations = concat!(
        "const key: str = \"spot\"\n",
        "const other: str = \"rate\"\n",
        "const fn float<X>(x: X) -> X requires X == f64 => x\n",
        "operator pick<X>(x: X) -> X\n",
        "impl const fn pick(x: f64) -> f64 => x\n",
        "const fn caller<T, U, ...Ps>(f: const fn(...{Ps}) -> i64, x: T, y: U) -> T\n",
    );
    for (call, rejected) in [
        ("float<T>(x)", "constraint/requires.unmet"),
        ("pick(x)", "operator/operator.no_candidate"),
    ] {
        for (condition, admitted) in [
            (
                "T in {i64, f64} && U in {str, f64} && T == Ps[key] && U == Ps[key]",
                true,
            ),
            (
                "Ps[key] == U && Ps[key] == T && U in {str, f64} && T in {i64, f64}",
                true,
            ),
            (
                "T in {i64, f64} && U in {i64, f64} && T == Ps[key] && Ps[key] == U",
                false,
            ),
            (
                "T in {i64, f64} && U in {str, f64} && T == Ps[key] && U == Ps[other]",
                false,
            ),
        ] {
            let checked = module(&format!(
                "{declarations}    requires {condition} => {call}\n"
            ));
            if admitted {
                assert_eq!(checked.summary(), NONE, "{condition}\n{}", checked.report);
            } else {
                assert_eq!(checked.findings.len(), 1, "{condition}\n{}", checked.report);
                assert_eq!(
                    checked.findings[0].code, rejected,
                    "{condition}\n{}",
                    checked.report
                );
            }
        }
    }
}

/// GEN-7 and GEN-10: universal bounds alone do not require the pack to
/// contain a key. An unrelated member cannot invalidate a proof about T.
#[test]
fn only_linked_named_members_constrain_an_ordinary_trial() {
    let definitions = concat!(
        "const key: str = \"item\"\n",
        "const fn float<X>(x: X) -> X requires X == f64 => x\n",
        "const fn caller<T, ...Ps>(f: const fn(...{Ps}) -> i64, t: T) -> T\n",
    );
    let empty_pack = module(&format!(
        "{definitions}    requires T in {{i64, f64}} && each k in arg_keys(Ps) {{ Ps[k] == T && Ps[k] in {{str, f64}} }} => float<T>(t)\n"
    ));
    assert_eq!(empty_pack.findings.len(), 1, "{}", empty_pack.report);
    assert_eq!(empty_pack.findings[0].code, "constraint/requires.unmet");
    let unrelated = module(&format!(
        "{definitions}    requires T in {{f64}} && Ps[key] == bool && Ps[key] == str => float<T>(t)\n"
    ));
    assert_eq!(unrelated.summary(), NONE, "{}", unrelated.report);
}

/// GEN-3, GEN-7 and GEN-10: positive constraints on the same reflected key
/// admit their intersection in any conjunction order, for both generic calls
/// and operators, while retaining rejections when a failing type remains or
/// the intersection is empty.
#[test]
fn reflected_member_same_key_constraints_intersect_in_any_order() {
    let helper = concat!(
        "const k: str = \"item\"\n",
        "const other: str = \"other\"\n",
        "const fn num_only<T>(value: T) -> T requires T in {i64, f64} => value\n",
        "const fn pair_only<T>(value: T) -> T requires T == list<i64, 2> => value\n",
    );
    let fn_decl =
        "const fn check<T, ...Ps>(f: const fn(...{Ps}) -> i64, t: T, x: Ps[k]) -> Ps[k]\n";

    // Positive: finite same-key intersections prove both callee requires and operators.
    for condition in [
        "Ps[k] in {str, f64} && Ps[k] in {f64}",
        "Ps[k] in {f64} && Ps[k] in {str, f64}",
        "each a in arg_keys(Ps) { Ps[a] in {str, f64} } && Ps[k] in {f64}",
        "Ps[k] in {f64} && each a in arg_keys(Ps) { Ps[a] in {str, f64} }",
        "each a in arg_keys(Ps) { Ps[a] in {i64, f64} } && Ps[k] in {str, f64}",
        "Ps[k] in {str, f64} && each a in arg_keys(Ps) { Ps[a] in {i64, f64} }",
        "each a in arg_keys(Ps) { Ps[a] in {str, f64} } && each b in arg_keys(Ps) { Ps[b] in {bool, f64} }",
        "each a in arg_keys(Ps) { Ps[a] in {str, f64} && Ps[a] in {bool, f64} }",
        "Ps[k] in {str, bool, f64} && Ps[k] in {str, i64, f64} && Ps[k] in {bool, i64, f64}",
        "Ps[k] in {str, f64} && Ps[k] == f64",
        "Ps[k] == f64 && Ps[k] in {str, f64}",
        "Ps[k] == T && Ps[k] in {str, f64} && Ps[k] in {i64, f64}",
        "Ps[k] in {list<T, 2>, f64} && Ps[k] in {str, f64}",
        "Ps[k] in {str, f64} && Ps[k] in {list<T, 2>, f64}",
    ] {
        let checked_fn = module(&format!(
            "{helper}{fn_decl}    requires {condition} => num_only<Ps[k]>(x)\n"
        ));
        assert_eq!(
            checked_fn.summary(),
            NONE,
            "fn {condition}\n{}",
            checked_fn.report
        );
        let checked_op = module(&format!(
            "{helper}{fn_decl}    requires {condition} => x - x\n"
        ));
        assert_eq!(
            checked_op.summary(),
            NONE,
            "op {condition}\n{}",
            checked_op.report
        );
    }

    // Positive: symbolic key parameter combines universal and explicit bounds in either order.
    for condition in [
        "each a in arg_keys(Ps) { Ps[a] in {str, f64} } && Ps[key] in {f64}",
        "Ps[key] in {f64} && each a in arg_keys(Ps) { Ps[a] in {str, f64} }",
    ] {
        let checked = module(&format!(
            "{helper}const fn sym<const key: str, ...Ps>(f: const fn(...{{Ps}}) -> i64, \
             x: Ps[key]) -> Ps[key]\n    requires {condition} => num_only<Ps[key]>(x)\n"
        ));
        assert_eq!(checked.summary(), NONE, "{condition}\n{}", checked.report);
    }

    // Positive: open size/type pattern narrows a concrete set in either order.
    for condition in [
        "Ps[k] == list<i64, N> && Ps[k] in {list<i64, 2>, str}",
        "Ps[k] in {list<i64, 2>, str} && Ps[k] == list<i64, N>",
        "Ps[k] == list<T, 2> && Ps[k] in {list<i64, 2>, str}",
        "Ps[k] in {list<i64, 2>, str} && Ps[k] == list<T, 2>",
    ] {
        let checked = module(&format!(
            "{helper}const fn sized<T, const N: i64, ...Ps>(f: const fn(...{{Ps}}) -> i64, \
             t: T, x: Ps[k]) -> Ps[k]\n    requires {condition} => pair_only<Ps[k]>(x)\n"
        ));
        assert_eq!(checked.summary(), NONE, "{condition}\n{}", checked.report);
    }

    // Negative: intersection still admits a failing type, is empty/contradictory,
    // mentions an unconstrained type parameter, names a different key, or stands
    // inside disjunction/negation/existential.
    for condition in [
        "Ps[k] in {str, i64, f64} && Ps[k] in {str, f64}",
        "Ps[k] in {str, f64} && Ps[k] in {str, i64, f64}",
        "each a in arg_keys(Ps) { Ps[a] in {str, i64, f64} } && Ps[k] in {str, f64}",
        "Ps[k] in {str, f64} && each a in arg_keys(Ps) { Ps[a] in {str, i64, f64} }",
        "Ps[k] in {str} && Ps[k] in {f64}",
        "Ps[k] in {f64} && Ps[k] in {str}",
        "Ps[k] == str && Ps[k] in {f64}",
        "Ps[k] in {f64} && Ps[k] == str",
        "Ps[k] == i64 && Ps[k] in {str}",
        "Ps[k] == f64 && Ps[k] == str",
        "Ps[k] == str && Ps[k] == f64",
        "Ps[k] in {T, f64} && Ps[k] in {str, f64}",
        "Ps[k] in {str, f64} && Ps[k] in {T, f64}",
        "Ps[k] in {str, f64} && Ps[other] in {f64}",
        "Ps[k] in {str, f64} && (Ps[k] in {f64} || Ps[k] in {bool})",
        "Ps[k] in {str, f64} && !(Ps[k] == str)",
        "Ps[k] in {str, f64} && any a in arg_keys(Ps) { Ps[a] == f64 }",
    ] {
        let checked_fn = module(&format!(
            "{helper}{fn_decl}    requires {condition} => num_only<Ps[k]>(x)\n"
        ));
        assert_eq!(
            checked_fn.findings.len(),
            1,
            "fn {condition}\n{}",
            checked_fn.report
        );
        assert_eq!(
            checked_fn.findings[0].code, "constraint/requires.unmet",
            "fn {condition}\n{}",
            checked_fn.report
        );
        let checked_op = module(&format!(
            "{helper}{fn_decl}    requires {condition} => x - x\n"
        ));
        assert_eq!(
            checked_op.findings.len(),
            1,
            "op {condition}\n{}",
            checked_op.report
        );
        assert_eq!(
            checked_op.findings[0].code, "operator/operator.no_candidate",
            "op {condition}\n{}",
            checked_op.report
        );
    }

    // Open bounds with different type or constant variables stay conservative
    // in both conjunction orders: never admit one order and reject the other.
    let open_helper = concat!(
        "const k: str = \"item\"\n",
        "struct Pair { a: i64\n b: i64 }\n",
        "operator same_type_op<A, B>(a: A, b: B) -> i64\n",
        "impl const fn same_type_op<X>(a: X, b: X) -> i64 => 1\n",
        "const fn struct_only<X>(value: X) -> X requires X in struct => value\n",
    );
    let open_decl = concat!(
        "const fn check<T, U, ...Ps>(f: const fn(...{Ps}) -> i64, ",
        "t: T, u: U, x: Ps[k]) -> Ps[k]\n",
    );
    for condition in [
        "T in struct && Ps[k] == T && Ps[k] == U",
        "T in struct && Ps[k] == U && Ps[k] == T",
        "T in struct && Ps[k] in {T, Pair} && Ps[k] in {U, Pair}",
        "T in struct && Ps[k] in {U, Pair} && Ps[k] in {T, Pair}",
    ] {
        let checked = module(&format!(
            "{open_helper}{open_decl}    requires {condition} => struct_only<Ps[k]>(x)\n"
        ));
        assert_eq!(
            checked.findings.len(),
            1,
            "open struct {condition}\n{}",
            checked.report
        );
        assert_eq!(
            checked.findings[0].code, "constraint/requires.unmet",
            "open struct {condition}\n{}",
            checked.report
        );
    }
    let open_pair_decl = concat!(
        "const fn check_pair<T, U, ...Ps>(f: const fn(...{Ps}) -> i64, ",
        "t: T, u: U, x: Ps[k]) -> i64\n",
    );
    for condition in [
        "Ps[k] == T && Ps[k] == U",
        "Ps[k] == U && Ps[k] == T",
        "Ps[k] in {T, i64} && Ps[k] in {U, i64}",
        "Ps[k] in {U, i64} && Ps[k] in {T, i64}",
    ] {
        let checked = module(&format!(
            "{open_helper}{open_pair_decl}    requires {condition} => same_type_op(x, t)\n"
        ));
        assert_eq!(
            checked.findings.len(),
            1,
            "open op {condition}\n{}",
            checked.report
        );
        assert_eq!(
            checked.findings[0].code, "operator/operator.no_candidate",
            "open op {condition}\n{}",
            checked.report
        );
    }
    let open_list_decl = concat!(
        "const fn check_list<T, U, ...Ps>(f: const fn(...{Ps}) -> i64, ",
        "lt: list<T, 2>, u: U, x: Ps[k]) -> i64\n",
    );
    for condition in [
        "Ps[k] == list<T, 2> && Ps[k] == list<U, 2>",
        "Ps[k] == list<U, 2> && Ps[k] == list<T, 2>",
        "Ps[k] in {list<T, 2>, Pair} && Ps[k] in {list<U, 2>, Pair}",
        "Ps[k] in {list<U, 2>, Pair} && Ps[k] in {list<T, 2>, Pair}",
    ] {
        let checked = module(&format!(
            "{open_helper}{open_list_decl}    requires {condition} => same_type_op(x, lt)\n"
        ));
        assert_eq!(
            checked.findings.len(),
            1,
            "open list {condition}\n{}",
            checked.report
        );
        assert_eq!(
            checked.findings[0].code, "operator/operator.no_candidate",
            "open list {condition}\n{}",
            checked.report
        );
    }
    for condition in [
        "Ps[k] == list<i64, N> && Ps[k] == list<i64, M>",
        "Ps[k] == list<i64, M> && Ps[k] == list<i64, N>",
    ] {
        let checked = module(&format!(
            "{open_helper}const fn check_sizes<const N: i64, const M: i64, ...Ps>(\
             f: const fn(...{{Ps}}) -> i64, ln: list<i64, N>, x: Ps[k]) -> i64\n\
             requires {condition} => same_type_op(x, ln)\n"
        ));
        assert_eq!(
            checked.findings.len(),
            1,
            "open sizes {condition}\n{}",
            checked.report
        );
        assert_eq!(
            checked.findings[0].code, "operator/operator.no_candidate",
            "open sizes {condition}\n{}",
            checked.report
        );
    }

    // Positive controls: same-variable open subset or open bounds alongside a concrete set.
    for condition in [
        "Ps[k] == list<T, 2> && Ps[k] in {list<T, 2>, Pair}",
        "Ps[k] in {list<T, 2>, Pair} && Ps[k] == list<T, 2>",
    ] {
        let checked = module(&format!(
            "{open_helper}{open_list_decl}    requires {condition} => same_type_op(x, lt)\n"
        ));
        assert_eq!(
            checked.summary(),
            NONE,
            "same-var list subset {condition}\n{}",
            checked.report
        );
    }
    for condition in [
        "Ps[k] == list<i64, N> && Ps[k] in {list<i64, N>, Pair}",
        "Ps[k] in {list<i64, N>, Pair} && Ps[k] == list<i64, N>",
    ] {
        let checked = module(&format!(
            "{open_helper}const fn check_sizes<const N: i64, ...Ps>(\
             f: const fn(...{{Ps}}) -> i64, ln: list<i64, N>, x: Ps[k]) -> i64\n\
             requires {condition} => same_type_op(x, ln)\n"
        ));
        assert_eq!(
            checked.summary(),
            NONE,
            "same-var size subset {condition}\n{}",
            checked.report
        );
    }
    for condition in [
        "Ps[k] == T && Ps[k] == T",
        "Ps[k] == T && Ps[k] in {T, str}",
        "Ps[k] in {T, str} && Ps[k] == T",
        "each a in arg_keys(Ps) { Ps[a] in {T, str} } && Ps[k] == T",
        "Ps[k] == T && each a in arg_keys(Ps) { Ps[a] in {T, str} }",
    ] {
        let checked = module(&format!(
            "{open_helper}{open_pair_decl}    requires {condition} => same_type_op(x, t)\n"
        ));
        assert_eq!(
            checked.summary(),
            NONE,
            "same-var open subset {condition}\n{}",
            checked.report
        );
    }
    for condition in [
        "Ps[k] == T && Ps[k] == U && Ps[k] in {i64, f64}",
        "Ps[k] == U && Ps[k] == T && Ps[k] in {i64, f64}",
        "Ps[k] in {i64, f64} && Ps[k] == T && Ps[k] == U",
    ] {
        let checked = module(&format!(
            "{helper}{open_decl}    requires {condition} => num_only<Ps[k]>(x)\n"
        ));
        assert_eq!(
            checked.summary(),
            NONE,
            "open + concrete {condition}\n{}",
            checked.report
        );
    }
}

/// GEN-7, TYPE-9, TYPE-16: an unresolved type that can normalize when its
/// generic parameter is supplied (`delta<T>`, `atomic<T>`, nested lists,
/// nullable wrappers, open function types, open list extents) cannot be
/// discarded as disjoint when intersecting same-key reflected bounds.
/// Genuine structural disjointness still narrows in either conjunction order.
#[test]
fn reflected_member_intersection_keeps_normalizing_open_types() {
    let base = concat!(
        "const k: str = \"size\"\n",
        "const fn allowed<X>(v: X) -> X requires X == str => v\n",
        "const fn allowed_i64_or_str<X>(v: X) -> X requires X in {i64, str} => v\n",
        "const fn allowed_i64<X>(v: X) -> X requires X == i64 => v\n",
        "operator str_op<X>(v: X) -> i64\n",
        "impl const fn str_op(v: str) -> i64 => 1\n",
        "const fn sig_i64(size: i64) -> i64 => 1\n",
        "const fn sig_str(size: str) -> i64 => 1\n",
        "const fn sig_list(size: list<i64>) -> i64 => 1\n",
        "const fn sig_pair(size: list<i64, 2>) -> i64 => 1\n",
        "const fn sig_opt(size: i64?) -> i64 => 1\n",
        "const fn sig_delta(size: delta<delta<list<i64>>?>) -> i64 => 1\n",
        "const fn sig_tuple(size: tuple<i64, str>) -> i64 => 1\n",
    );
    let decl = concat!(
        "const fn demonstration<T, ...P>(f: const fn(...{P}) -> i64, ",
        "t: T, v: P[k]) -> P[k]\n",
    );
    let op_decl = concat!(
        "const fn op_demo<T, ...P>(f: const fn(...{P}) -> i64, ",
        "t: T, v: P[k]) -> i64\n",
    );

    // Unsound narrowing regressions: both conjunction orders must reject at
    // declaration AND with a concrete invocation where the open type
    // normalizes into the concrete set (for both callee requires and operators).
    let nested_delta_call =
        "test actual { let v = demonstration(sig_delta, 2, delta<delta<list<i64>>?>()) }\n";
    let nested_delta_op =
        "test actual { let v = op_demo(sig_delta, 2, delta<delta<list<i64>>?>()) }\n";
    let rejecting_with_calls = [
        (
            "P[k] in {delta<delta<list<T>>?>, str} && P[k] in {delta<delta<list<i64>>?>, str}",
            nested_delta_call,
            nested_delta_op,
        ),
        (
            "P[k] in {delta<delta<list<i64>>?>, str} && P[k] in {delta<delta<list<T>>?>, str}",
            nested_delta_call,
            nested_delta_op,
        ),
        (
            "P[k] in {delta<T>, str} && P[k] in {i64, str}",
            "test actual { assert demonstration(sig_i64, 2, 3) == 3 }\n",
            "test actual { assert op_demo(sig_i64, 2, 3) == 1 }\n",
        ),
        (
            "P[k] in {i64, str} && P[k] in {delta<T>, str}",
            "test actual { assert demonstration(sig_i64, 2, 3) == 3 }\n",
            "test actual { assert op_demo(sig_i64, 2, 3) == 1 }\n",
        ),
        (
            "each a in arg_keys(P) { P[a] in {delta<T>, str} } && P[k] in {i64, str}",
            "test actual { assert demonstration(sig_i64, 2, 3) == 3 }\n",
            "test actual { assert op_demo(sig_i64, 2, 3) == 1 }\n",
        ),
        (
            "P[k] in {i64, str} && each a in arg_keys(P) { P[a] in {delta<T>, str} }",
            "test actual { assert demonstration(sig_i64, 2, 3) == 3 }\n",
            "test actual { assert op_demo(sig_i64, 2, 3) == 1 }\n",
        ),
        (
            "P[k] == delta<T> && P[k] in {i64, str}",
            "test actual { assert demonstration(sig_i64, 2, 3) == 3 }\n",
            "test actual { assert op_demo(sig_i64, 2, 3) == 1 }\n",
        ),
        (
            "P[k] in {i64, str} && P[k] == delta<T>",
            "test actual { assert demonstration(sig_i64, 2, 3) == 3 }\n",
            "test actual { assert op_demo(sig_i64, 2, 3) == 1 }\n",
        ),
        (
            "P[k] in {atomic<T>, str} && P[k] in {i64, str}",
            "test actual { assert demonstration(sig_i64, 2, 3) == 3 }\n",
            "test actual { assert op_demo(sig_i64, 2, 3) == 1 }\n",
        ),
        (
            "P[k] in {i64, str} && P[k] in {atomic<T>, str}",
            "test actual { assert demonstration(sig_i64, 2, 3) == 3 }\n",
            "test actual { assert op_demo(sig_i64, 2, 3) == 1 }\n",
        ),
        (
            "P[k] in {list<delta<T>>, str} && P[k] in {list<i64>, str}",
            "test actual { assert demonstration(sig_list, 2, [3]) == [3] }\n",
            "test actual { assert op_demo(sig_list, 2, [3]) == 1 }\n",
        ),
        (
            "P[k] in {list<i64>, str} && P[k] in {list<delta<T>>, str}",
            "test actual { assert demonstration(sig_list, 2, [3]) == [3] }\n",
            "test actual { assert op_demo(sig_list, 2, [3]) == 1 }\n",
        ),
        (
            "P[k] in {list<atomic<T>>, str} && P[k] in {list<i64>, str}",
            "test actual { assert demonstration(sig_list, 2, [3]) == [3] }\n",
            "test actual { assert op_demo(sig_list, 2, [3]) == 1 }\n",
        ),
        (
            "P[k] in {list<i64>, str} && P[k] in {list<atomic<T>>, str}",
            "test actual { assert demonstration(sig_list, 2, [3]) == [3] }\n",
            "test actual { assert op_demo(sig_list, 2, [3]) == 1 }\n",
        ),
        (
            "P[k] in {list<delta<T>, 2>, str} && P[k] in {list<i64, 2>, str}",
            "test actual { assert demonstration(sig_pair, 2, [3, 4]) == [3, 4] }\n",
            "test actual { assert op_demo(sig_pair, 2, [3, 4]) == 1 }\n",
        ),
        (
            "P[k] in {list<i64, 2>, str} && P[k] in {list<delta<T>, 2>, str}",
            "test actual { assert demonstration(sig_pair, 2, [3, 4]) == [3, 4] }\n",
            "test actual { assert op_demo(sig_pair, 2, [3, 4]) == 1 }\n",
        ),
        (
            "P[k] in {delta<T>?, str} && P[k] in {i64?, str}",
            "test actual { assert demonstration(sig_opt, 2, 3) == 3 }\n",
            "test actual { assert op_demo(sig_opt, 2, 3) == 1 }\n",
        ),
        (
            "P[k] in {i64?, str} && P[k] in {delta<T>?, str}",
            "test actual { assert demonstration(sig_opt, 2, 3) == 3 }\n",
            "test actual { assert op_demo(sig_opt, 2, 3) == 1 }\n",
        ),
        (
            "P[k] in {atomic<T?>, str} && P[k] in {i64?, str}",
            "test actual { assert demonstration(sig_opt, 2, 3) == 3 }\n",
            "test actual { assert op_demo(sig_opt, 2, 3) == 1 }\n",
        ),
        (
            "P[k] in {i64?, str} && P[k] in {atomic<T?>, str}",
            "test actual { assert demonstration(sig_opt, 2, 3) == 3 }\n",
            "test actual { assert op_demo(sig_opt, 2, 3) == 1 }\n",
        ),
        (
            "P[k] in {list<atomic<T?>>, str} && P[k] in {list<i64?>, str}",
            "const fn sig_opt_list(size: list<i64?>) -> i64 => 1\n\
             test actual { assert demonstration(sig_opt_list, 2, [3]) == [3] }\n",
            "const fn sig_opt_list(size: list<i64?>) -> i64 => 1\n\
             test actual { assert op_demo(sig_opt_list, 2, [3]) == 1 }\n",
        ),
        (
            "P[k] in {list<i64?>, str} && P[k] in {list<atomic<T?>>, str}",
            "const fn sig_opt_list(size: list<i64?>) -> i64 => 1\n\
             test actual { assert demonstration(sig_opt_list, 2, [3]) == [3] }\n",
            "const fn sig_opt_list(size: list<i64?>) -> i64 => 1\n\
             test actual { assert op_demo(sig_opt_list, 2, [3]) == 1 }\n",
        ),
        (
            "P[k] in {atomic<delta<T>?>, str} && P[k] in {i64?, str}",
            "test actual { assert demonstration(sig_opt, 2, 3) == 3 }\n",
            "test actual { assert op_demo(sig_opt, 2, 3) == 1 }\n",
        ),
        (
            "P[k] in {i64?, str} && P[k] in {atomic<delta<T>?>, str}",
            "test actual { assert demonstration(sig_opt, 2, 3) == 3 }\n",
            "test actual { assert op_demo(sig_opt, 2, 3) == 1 }\n",
        ),
        (
            "P[k] in {tuple<delta<T>, str>, str} && P[k] in {tuple<i64, str>, str}",
            "test actual { assert demonstration(sig_tuple, 2, (3, \"a\")) == (3, \"a\") }\n",
            "test actual { assert op_demo(sig_tuple, 2, (3, \"a\")) == 1 }\n",
        ),
        (
            "P[k] in {tuple<i64, str>, str} && P[k] in {tuple<delta<T>, str>, str}",
            "test actual { assert demonstration(sig_tuple, 2, (3, \"a\")) == (3, \"a\") }\n",
            "test actual { assert op_demo(sig_tuple, 2, (3, \"a\")) == 1 }\n",
        ),
    ];
    for (condition, fn_call, op_call) in rejecting_with_calls {
        for (label, suffix) in [("decl", ""), ("call", fn_call)] {
            let checked = module(&format!(
                "{base}{decl}    requires {condition} => allowed<P[k]>(v)\n{suffix}"
            ));
            assert_eq!(
                checked.findings.len(),
                1,
                "fn {label} {condition}\n{}",
                checked.report
            );
            assert_eq!(
                checked.findings[0].code, "constraint/requires.unmet",
                "fn {label} {condition}\n{}",
                checked.report
            );
        }
        for (label, suffix) in [("decl", ""), ("call", op_call)] {
            let checked = module(&format!(
                "{base}{op_decl}    requires {condition} => str_op(v)\n{suffix}"
            ));
            assert_eq!(
                checked.findings.len(),
                1,
                "op {label} {condition}\n{}",
                checked.report
            );
            assert_eq!(
                checked.findings[0].code, "operator/operator.no_candidate",
                "op {label} {condition}\n{}",
                checked.report
            );
        }
    }

    // Declaration rejections for additional normalizing/open shapes:
    // `delta<T?>`, `delta<delta<T>>`, `atomic<delta<T>>`, nested `dict`/`result`,
    // tuple pack expansion `tuple<Us>`, and open list size vs unbounded `list<i64>`.
    for condition in [
        "P[k] in {delta<T?>, str} && P[k] in {i64, str}",
        "P[k] in {i64, str} && P[k] in {delta<T?>, str}",
        "P[k] in {atomic<T?>, str} && P[k] in {atomic<list<i64>?>, str}",
        "P[k] in {atomic<list<i64>?>, str} && P[k] in {atomic<T?>, str}",
        "P[k] in {delta<delta<T>>, str} && P[k] in {i64, str}",
        "P[k] in {i64, str} && P[k] in {delta<delta<T>>, str}",
        "P[k] in {atomic<delta<T>>, str} && P[k] in {i64, str}",
        "P[k] in {i64, str} && P[k] in {atomic<delta<T>>, str}",
        "P[k] in {dict<str, delta<T>>, str} && P[k] in {dict<str, i64>, str}",
        "P[k] in {dict<str, i64>, str} && P[k] in {dict<str, delta<T>>, str}",
        "P[k] in {dict<str, atomic<T>>, str} && P[k] in {dict<str, i64>, str}",
        "P[k] in {dict<str, i64>, str} && P[k] in {dict<str, atomic<T>>, str}",
        "P[k] in {result<delta<T>, error>, str} && P[k] in {result<i64, error>, str}",
        "P[k] in {result<i64, error>, str} && P[k] in {result<delta<T>, error>, str}",
    ] {
        let checked = module(&format!(
            "{base}{decl}    requires {condition} => allowed<P[k]>(v)\n"
        ));
        assert_eq!(
            checked.findings.len(),
            1,
            "extra decl {condition}\n{}",
            checked.report
        );
        assert_eq!(
            checked.findings[0].code, "constraint/requires.unmet",
            "extra decl {condition}\n{}",
            checked.report
        );
    }
    for condition in [
        "P[k] in {tuple<Us>, str} && P[k] in {tuple<i64, str>, str}",
        "P[k] in {tuple<i64, str>, str} && P[k] in {tuple<Us>, str}",
    ] {
        let checked = module(&format!(
            "{base}const fn pack_tuple<...Us, ...P>(f: const fn(...{{P}}) -> i64, \
             u: tuple<Us>, v: P[k]) -> P[k]\n    requires {condition} => allowed<P[k]>(v)\n\
             test actual {{ assert pack_tuple(sig_tuple, (2, \"a\"), (3, \"b\")) == (3, \"b\") }}\n"
        ));
        assert_eq!(
            checked.findings.len(),
            1,
            "pack tuple {condition}\n{}",
            checked.report
        );
        assert_eq!(
            checked.findings[0].code, "constraint/requires.unmet",
            "pack tuple {condition}\n{}",
            checked.report
        );
    }
    for condition in [
        "P[k] in {list<i64, N>, str} && P[k] in {list<i64>, str}",
        "P[k] in {list<i64>, str} && P[k] in {list<i64, N>, str}",
    ] {
        let checked = module(&format!(
            "{base}const fn open_extent<N, ...P>(f: const fn(...{{P}}) -> i64, \
             x: list<i64, N>, v: P[k]) -> P[k]\n    requires {condition} => allowed<P[k]>(v)\n"
        ));
        assert_eq!(
            checked.findings.len(),
            1,
            "open extent {condition}\n{}",
            checked.report
        );
        assert_eq!(
            checked.findings[0].code, "constraint/requires.unmet",
            "open extent {condition}\n{}",
            checked.report
        );
    }

    // Positive controls: when the callee handles every type admitted by the
    // intersection, both declaration and concrete calls succeed in either order.
    for condition in [
        "P[k] in {delta<T>, str} && P[k] in {i64, str}",
        "P[k] in {i64, str} && P[k] in {delta<T>, str}",
        "P[k] in {atomic<T>, str} && P[k] in {i64, str}",
        "P[k] in {i64, str} && P[k] in {atomic<T>, str}",
        "P[k] == delta<T> && P[k] in {i64, str}",
        "P[k] in {i64, str} && P[k] == delta<T>",
    ] {
        let checked = module(&format!(
            "{base}{decl}    requires {condition} => allowed_i64_or_str<P[k]>(v)\n\
             test ok {{\n\
                 assert demonstration(sig_i64, 2, 3) == 3\n\
                 assert demonstration(sig_str, \"a\", \"b\") == \"b\"\n\
             }}\n"
        ));
        assert_eq!(
            checked.summary(),
            NONE,
            "either {condition}\n{}",
            checked.report
        );
    }
    for condition in [
        "P[k] in {delta<T>, str} && P[k] == i64",
        "P[k] == i64 && P[k] in {delta<T>, str}",
        "P[k] in {atomic<T>, str} && P[k] == i64",
        "P[k] == i64 && P[k] in {atomic<T>, str}",
    ] {
        let checked = module(&format!(
            "{base}{decl}    requires {condition} => allowed_i64<P[k]>(v)\n\
             test ok {{ assert demonstration(sig_i64, 2, 3) == 3 }}\n"
        ));
        assert_eq!(
            checked.summary(),
            NONE,
            "exact i64 {condition}\n{}",
            checked.report
        );
    }

    // Real disjoint controls: outer constructors that never normalize into the
    // competing concrete type still narrow to `str` in both orders (for both
    // declaration and concrete invocation, and for both callee requires and operators).
    for condition in [
        "P[k] in {list<delta<T>>, str} && P[k] in {i64, str}",
        "P[k] in {i64, str} && P[k] in {list<delta<T>>, str}",
        "P[k] in {list<T>, str} && P[k] in {delta<delta<list<i64>>?>, str}",
        "P[k] in {delta<delta<list<i64>>?>, str} && P[k] in {list<T>, str}",
        "P[k] in {list<atomic<T>>, str} && P[k] in {i64, str}",
        "P[k] in {i64, str} && P[k] in {list<atomic<T>>, str}",
        "P[k] in {delta<list<T>>, str} && P[k] in {i64, str}",
        "P[k] in {i64, str} && P[k] in {delta<list<T>>, str}",
        "P[k] in {atomic<list<T>>, str} && P[k] in {i64, str}",
        "P[k] in {i64, str} && P[k] in {atomic<list<T>>, str}",
        "P[k] in {delta<T>?, str} && P[k] in {i64, str}",
        "P[k] in {i64, str} && P[k] in {delta<T>?, str}",
        "P[k] in {atomic<T?>, str} && P[k] in {i64, str}",
        "P[k] in {i64, str} && P[k] in {atomic<T?>, str}",
        "P[k] in {list<atomic<T?>>, str} && P[k] in {list<i64>, str}",
        "P[k] in {list<i64>, str} && P[k] in {list<atomic<T?>>, str}",
        "P[k] in {atomic<delta<T>?>, str} && P[k] in {i64, str}",
        "P[k] in {i64, str} && P[k] in {atomic<delta<T>?>, str}",
        "P[k] in {atomic<T>, str} && P[k] in {list<i64>, str}",
        "P[k] in {list<i64>, str} && P[k] in {atomic<T>, str}",
        "P[k] in {atomic<T?>, str} && P[k] in {list<i64>?, str}",
        "P[k] in {list<i64>?, str} && P[k] in {atomic<T?>, str}",
        "P[k] in {atomic<T?>, str} && P[k] in {atomic<list<i64>>, str}",
        "P[k] in {atomic<list<i64>>, str} && P[k] in {atomic<T?>, str}",
        "P[k] in {list<delta<T>, 3>, str} && P[k] in {list<i64, 2>, str}",
        "P[k] in {list<i64, 2>, str} && P[k] in {list<delta<T>, 3>, str}",
        "T in {f64, bool} && P[k] in {delta<T>, str} && P[k] in {i64, str}",
        "T in {f64, bool} && P[k] in {i64, str} && P[k] in {delta<T>, str}",
    ] {
        let checked_fn = module(&format!(
            "{base}{decl}    requires {condition} => allowed<P[k]>(v)\n\
             test ok {{ assert demonstration(sig_str, 1.5, \"ok\") == \"ok\" }}\n"
        ));
        assert_eq!(
            checked_fn.summary(),
            NONE,
            "disjoint fn {condition}\n{}",
            checked_fn.report
        );
        let checked_op = module(&format!(
            "{base}{op_decl}    requires {condition} => str_op(v)\n\
             test ok {{ assert op_demo(sig_str, 1.5, \"ok\") == 1 }}\n"
        ));
        assert_eq!(
            checked_op.summary(),
            NONE,
            "disjoint op {condition}\n{}",
            checked_op.report
        );
    }
}

/// GEN-7: a concrete reflected result must recheck its consumers, including
/// a second operator and a local initialized with the first call.
#[test]
fn reflected_results_recheck_consumers_at_the_calls_that_make_them() {
    for body in [
        "=> a - a - \"x\"",
        "=> (a + a) * \"x\"",
        "{ let d = a - a\n    return d - \"x\"\n}",
    ] {
        let checked = module(&format!(
            r#"const item_key: str = "item"
const fn chain<...Ps>(f: const fn(...{{Ps}}) -> i64, a: Ps[item_key]) -> Ps[item_key]
    requires Ps[item_key] in {{i64, f64}} {body}
const fn num(item: i64) -> i64 => item
test check_chain {{ assert chain(num, 3) == 0 }}
"#
        ));
        assert_eq!(checked.findings.len(), 1, "{body}\n{}", checked.report);
        assert_eq!(checked.findings[0].code, "operator/operator.no_candidate");
        assert!(
            checked.report.contains("no `i64` and `str`")
                && checked
                    .report
                    .contains("this call makes the instance `chain<[i64]>`"),
            "{body}\n{}",
            checked.report
        );
    }
}

/// GEN-7: diagnostics distinguish missing evidence, insufficient evidence,
/// and reaching the analyser's limit while trying stated alternatives.
#[test]
fn reflected_member_proof_reports_its_failure_reason() {
    let declaration = concat!(
        "const key: str = \"item\"\n",
        "const fn subtract<...Ps>(f: const fn(...{Ps}) -> i64, a: Ps[key]) -> Ps[key]\n",
    );
    let missing = module(&format!(
        "{declaration}    requires Ps[key] != str => a - a\n"
    ));
    assert_eq!(missing.findings[0].code, "operator/operator.no_candidate");
    assert!(
        missing.report.contains("`requires Ps[k] in {i64, f64}`"),
        "{}",
        missing.report
    );

    let equality = module(&format!(
        "{declaration}    requires Ps[key] == str => a - a\n"
    ));
    assert_eq!(equality.findings[0].code, "operator/operator.no_candidate");
    assert!(
        equality.report.contains(r#"`Ps["item"]` = `str`"#),
        "{}",
        equality.report
    );
    assert!(
        !equality.report.contains("a membership authorises"),
        "{}",
        equality.report
    );

    let keys: Vec<_> = (0..8).map(|index| format!("k{index}")).collect();
    let declarations = keys
        .iter()
        .map(|key| format!("const {key}: str = \"{key}\"\n"))
        .collect::<String>();
    let members: Vec<_> = keys.iter().map(|key| format!("Ps[{key}]")).collect();
    let requirements = members
        .iter()
        .map(|member| format!("{member} in {{i64, f64, bool, str}}"))
        .collect::<Vec<_>>()
        .join(" && ");
    let exhausted = module(&format!(
        "{declarations}const fn subtract<...Ps>(f: const fn(...{{Ps}}) -> i64, \
         a: tuple<{}>) -> i64 requires {requirements} => a - a\n",
        members.join(", ")
    ));
    assert_eq!(exhausted.findings[0].code, "operator/operator.no_candidate");
    assert!(
        exhausted.report.contains(&format!(
            "limit of {} combinations",
            hgl_analyser::INSTANCES
        )),
        "{}",
        exhausted.report
    );
}

/// GEN-7 and GEN-17: two reflected keys need independent trial types.
#[test]
fn a_reflected_pack_keeps_distinct_memberships_for_its_keys() {
    let checked = module(concat!(
        "const left: str = \"left\"\n",
        "const right: str = \"right\"\n",
        "operator match_pair<T, U>(a: T, b: U) -> i64\n",
        "impl const fn match_pair(a: i64, b: i64) -> i64 => a + b\n",
        "impl const fn match_pair(a: f64, b: f64) -> i64 => 0\n",
        "const fn broken<...Ps>(f: const fn(...{Ps}) -> i64, a: Ps[left], b: Ps[right]) -> i64\n",
        "    requires Ps[left] in {i64, f64} && Ps[right] in {i64, f64} => match_pair(a, b)\n",
    ));
    assert_eq!(checked.findings.len(), 1, "{}", checked.report);
    assert_eq!(checked.findings[0].code, "operator/operator.no_candidate");
    assert!(
        checked.report.contains(r#"`Ps["left"]` = `i64`"#),
        "{}",
        checked.report
    );
}

#[test]
fn a_type_inequality_tests_the_resolved_argument() {
    // GEN-3: `T != f64` tests a known type as `!(T == f64)` and binds nothing.
    let declaration = "const fn non_float<T>(value: T) -> T requires T != f64 => value\n";
    let accepted = module(&format!(
        "{declaration}const fn good() -> i64 => non_float(7)\n"
    ));
    assert_eq!(accepted.summary(), NONE);

    let rejected = module(&format!(
        "{declaration}const fn bad() -> f64 => non_float(1.0)\n"
    ));
    assert!(
        rejected
            .findings
            .iter()
            .any(|finding| finding.code == "constraint/requires.unmet"),
        "{}",
        rejected.report
    );
}

/// What each statement of `name` that holds a call calls: the name of
/// the operator a requirement names, `candidate` for a candidate
/// selected, and `none` for a call with no callee.
fn callees(checked: &Checked, name: &str) -> Vec<String> {
    fn of(checked: &Checked, value: &Expr, found: &mut Vec<String>) {
        if let ExprKind::Call { callee, .. } = &value.kind {
            found.push(match callee {
                Callee::Required {
                    operator: (module, declaration),
                    ..
                } => checked.program.modules[*module].declarations[*declaration]
                    .name
                    .clone(),
                Callee::Candidate { .. } => "candidate".to_string(),
                Callee::Function { .. } => "function".to_string(),
                Callee::Parameter { .. }
                | Callee::Anonymous(_)
                | Callee::DeferredOperator(_)
                | Callee::DeferredPackCall
                | Callee::DeferredValue
                | Callee::Unread => "none".to_string(),
            });
        }
    }
    let mut found = Vec::new();
    for statement in &body(checked, name).statements {
        match statement {
            Stmt::Declare {
                value: Some(value), ..
            }
            | Stmt::Return {
                value: Some(value), ..
            }
            | Stmt::Expr(value) => of(checked, value, &mut found),
            Stmt::Hook { body, .. } => {
                for statement in body {
                    if let Stmt::Return {
                        value: Some(value), ..
                    } = statement
                    {
                        of(checked, value, &mut found);
                    }
                }
            }
            _ => {}
        }
    }
    found
}

/// GEN-7: "A generic body is checked under its stated requirements".
/// GEN-16: "a named operator requirement tests callability with the
/// supplied types. A result arrow constrains that operator's result
/// type". GEN-15: requirements joined by `&&` all hold. The generics
/// chapter: "A symbol is the call of its identity", so `a + b` is the
/// `add_(a, b)` a requirement names.
#[test]
fn a_generic_body_calls_what_its_requirements_state() {
    let checked = module(concat!(
        "const fn twice<T>(a: T) -> T requires add_(T, T) -> T => add_(a, a)\n",
        "const fn sum<T>(a: T, b: T) -> T requires add_(T, T) -> T => a + b\n",
        "const fn difference<T>(a: T, b: T) -> T\n",
        "    requires sub_(T, T) -> T && (neg_(T) -> T) {\n",
        "    let negated = -b\n",
        "    let whole = sub_(a, b)\n",
        "    return a - negated\n",
        "}\n",
        // A requirement with operands of two types, and of a type that
        // holds the parameter.
        "const fn scaled<T>(a: T, by: f64) -> list<T>\n",
        "    requires mul_(T, f64) -> T && add_(list<T>, list<T>) -> list<T> {\n",
        "    let one = mul_(a, by)\n",
        "    let ones: list<T> = [one]\n",
        // A sequence written out has no type of its own: it is read as
        // the type the requirement supplies in its place.
        "    let pair = [one] + [one]\n",
        "    let joined = add_(rhs: [a], lhs: ones)\n",
        "    return ones + ones\n",
        "}\n",
        // EXPR-2: "Named arguments use the selected public callable's
        // parameter names": the operator's, `lhs` and `rhs`.
        "const fn named<T>(a: T, b: T) -> T requires sub_(T, T) -> T {\n",
        "    let both = sub_(lhs: a, rhs: b)\n",
        "    let mixed = sub_(a, rhs: b)\n",
        "    return sub_(rhs: b, lhs: a)\n",
        "}\n",
        // The parameter may stand in the result alone.
        "operator make<O>() -> O\n",
        "const fn build<T>() -> T requires make() -> T => make()\n",
        // GEN-8: the call may write the operator's generic arguments.
        "const fn built<T>() -> T requires make() -> T => make<T>()\n",
        // With no arrow the requirement says only that the call is
        // admitted: what it gives is not known.
        "const fn loose<T>(a: T) requires add_(T, T) {\n",
        "    let unknown = add_(a, a)\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(callees(&checked, "twice"), ["add_"]);
    assert_eq!(callees(&checked, "sum"), ["add_"]);
    assert_eq!(callees(&checked, "difference"), ["neg_", "sub_", "sub_"]);
    assert_eq!(
        callees(&checked, "scaled"),
        ["mul_", "add_", "add_", "add_"]
    );
    assert_eq!(callees(&checked, "named"), ["sub_", "sub_", "sub_"]);
    assert_eq!(callees(&checked, "build"), ["make"]);
    assert_eq!(callees(&checked, "built"), ["make"]);
    // Each argument is recorded with the operator's parameter it is
    // given to: `sub_(rhs: b, lhs: a)` gives `b` to the second.
    let Some(Stmt::Return {
        value: Some(value), ..
    }) = body(&checked, "named").statements.last()
    else {
        panic!("a return with a value");
    };
    let ExprKind::Call { arguments, .. } = &value.kind else {
        panic!("a call: {:?}", value.kind);
    };
    let given: Vec<Option<usize>> = arguments
        .iter()
        .map(|argument| argument.parameter)
        .collect();
    assert_eq!(given, [Some(1), Some(0)]);
    assert_eq!(
        locals(&checked, "difference")[2..],
        ["negated: T", "whole: T"]
    );
    assert_eq!(
        locals(&checked, "scaled")[2..],
        [
            "one: T",
            "ones: list<T>",
            "pair: list<T>",
            "joined: list<T>"
        ]
    );
    assert_eq!(locals(&checked, "loose")[1..], ["unknown: _"]);
}

/// GEN-7: "choosing a specialization does not authorize unrelated
/// operations". A requirement is the call it names, supplied with the
/// types it names, and no other; one under a disjunction or a negation
/// does not hold for sure (GEN-15). GEN-16: the call is of the type
/// the requirement's arrow states, which is then what is given where
/// another type is expected (EXPR-5).
#[test]
fn a_requirement_is_the_call_it_names_and_no_other() {
    let checked = module(concat!(
        // No requirement.
        "const fn bare<T>(a: T) -> T => add_(a, a)\n",
        // Another operator's.
        "const fn other<T>(a: T) -> T requires sub_(T, T) -> T => add_(a, a)\n",
        // Other types.
        "const fn mixed<T>(a: T, n: i64) -> T requires add_(T, T) -> T => add_(a, n)\n",
        // One of two that may hold.
        "const fn either<T>(a: T) -> T\n",
        "    requires add_(T, T) -> T || sub_(T, T) -> T => add_(a, a)\n",
        // The arrow's type is the call's.
        "const fn wrong<T>(a: T) -> i64 requires add_(T, T) -> T => add_(a, a)\n",
        // Generic arguments the requirement does not agree with.
        "operator make<O>() -> O\n",
        "const fn unlike<T, U>() -> T requires make() -> T => make<U>()\n",
        // Generic arguments that agree in one part and not in another:
        // `make2<T>()` asks for a `tuple<T, _>`.
        "operator make2<A, B>() -> tuple<A, B>\n",
        "const fn parts<T, U>() requires make2() -> tuple<U, T> {\n",
        "    let agreed = make2<U>()\n",
        "    let pair = make2<T>()\n",
        "}\n",
        // A sequence that is not of the type supplied.
        "const fn lists<T>(a: T) -> list<T>\n",
        "    requires add_(list<T>, list<T>) -> list<T> => [a] + [1]\n",
        // A name the operator does not declare is a candidate's option,
        // which no requirement speaks for.
        "const fn option<T>(a: T) -> T\n",
        "    requires div_(T, T) -> T => div_(a, a, zero: .fail)\n",
        // A symbol is its call: `+`, `<` and a minus need theirs stated.
        "const fn plus<T>(a: T, b: T) -> T => a + b\n",
        "const fn less<T>(a: T, b: T) -> bool => a < b\n",
        "const fn minus<T>(a: T) -> T => -a\n",
        // OPS-2: \"`==` and `!=` compare two values of one type\": core's
        // candidate takes any, and so a generic parameter.
        "const fn same<T>(a: T, b: T) -> bool => a == b\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:2:32 operator/operator.no_candidate GEN-5",
            "m.hgl:3:58 operator/operator.no_candidate GEN-5",
            "m.hgl:4:66 operator/operator.no_candidate GEN-5",
            "m.hgl:6:52 operator/operator.no_candidate GEN-5",
            "m.hgl:7:60 type/type.mismatch EXPR-5",
            "m.hgl:9:54 operator/operator.no_candidate GEN-5",
            "m.hgl:13:16 operator/operator.no_candidate GEN-5",
            "m.hgl:16:55 operator/operator.no_candidate OPS-1",
            "m.hgl:18:33 operator/operator.no_candidate GEN-5",
            "m.hgl:19:40 operator/operator.no_candidate OPS-1",
            "m.hgl:20:43 operator/operator.no_candidate OPS-2",
            "m.hgl:21:33 operator/operator.no_candidate OPS-1",
        ]
    );
}

/// GEN-5: "Match candidates against the requested output before
/// inputs". Two requirements may name one operator with the same types
/// and different results (GEN-16: "A result arrow constrains that
/// operator's result type"): the call is the one that gives what its
/// place expects, whichever is written first.
#[test]
fn a_requirement_is_chosen_by_the_result_asked_for() {
    let checked = module(concat!(
        "operator measure<V, O>(value: V) -> O\n",
        "const fn wide<T>(a: T) -> f64\n",
        "    requires measure(T) -> i64 && measure(T) -> f64 => measure(a)\n",
        "const fn narrow<T>(a: T) -> i64\n",
        "    requires measure(T) -> i64 && measure(T) -> f64 => measure(a)\n",
        "const fn either<T>(a: T) requires measure(T) -> i64 && measure(T) -> f64 {\n",
        "    let real: f64 = measure(a)\n",
        // Nothing is asked for: the first stated.
        "    let first = measure(a)\n",
        "}\n",
        // Neither gives a `str`: the first stands, and does not fit.
        "const fn neither<T>(a: T) -> str\n",
        "    requires measure(T) -> i64 && measure(T) -> f64 => measure(a)\n",
    ));
    assert_eq!(checked.summary(), ["m.hgl:12:56 type/type.mismatch EXPR-5"]);
    assert_eq!(locals(&checked, "either")[1..], ["real: f64", "first: i64"]);
}

/// GEN-16: "a named operator requirement tests callability with the
/// supplied types. A result arrow constrains that operator's result
/// type". GEN-5: "Match candidates against the requested output before
/// inputs, then check their conditions." A candidate that requires an
/// operation is admitted for the types that have it.
#[test]
fn a_candidates_requirement_is_tested_with_the_calls_types() {
    let checked = module(concat!(
        "operator total<T>(a: T, b: T) -> T\n",
        "impl const fn total<T>(a: T, b: T) -> T requires add_(T, T) -> T => add_(a, b)\n",
        "operator spread<T>(a: T, b: T) -> T\n",
        "impl const fn spread<T>(a: T, b: T) -> T\n",
        "    requires sub_(T, T) -> T && neg_(T) -> T => sub_(a, neg_(b))\n",
        // The arrow constrains the result: an `i64` less an `i64` is no
        // `duration`, and a `datetime` less a `datetime` is one.
        "operator apart<T>(a: T, b: T) -> duration\n",
        "impl const fn apart<T>(a: T, b: T) -> duration\n",
        "    requires sub_(T, T) -> duration => sub_(a, b)\n",
        "const fn uses(x: f64, n: i64, flag: bool, text: str, at: datetime) {\n",
        "    let real = total(x, x)\n",
        "    let whole = total(n, n)\n",
        "    let words = total(text, text)\n",
        "    let both = spread(n, n)\n",
        "    let gap = apart(at, at)\n",
        // A `bool` is not added, a `str` is not subtracted, and two
        // `i64` are not a `duration` apart.
        "    let bad1 = total(flag, flag)\n",
        "    let bad2 = spread(text, text)\n",
        "    let bad3 = apart(n, n)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:16:16 operator/operator.no_candidate GEN-5",
            "m.hgl:17:16 operator/operator.no_candidate GEN-5",
            "m.hgl:18:16 operator/operator.no_candidate GEN-5",
        ]
    );
    assert_eq!(
        locals(&checked, "uses")[5..10],
        [
            "real: f64",
            "whole: i64",
            "words: str",
            "both: i64",
            "gap: duration",
        ]
    );
}

/// GEN-8: an operator's generic value parameter "takes part in the
/// output requested ... and the default where none of them says." A
/// requirement is tested as the call it names would select: with the
/// operator's default in the output requested, and with what the
/// operator requires of its result.
#[test]
fn a_requirement_is_tested_as_its_call_would_select() {
    let checked = module(concat!(
        "operator window<T, const size: i64 = 3>(value: T) -> list<T, size>\n",
        "impl const fn window(value: f64) -> list<f64, 3> => [value, value, value]\n",
        "impl const fn window(value: f64) -> list<f64, 5> =>\n",
        "    [value, value, value, value, value]\n",
        // With no arrow the default says which list: three.
        "operator framed<T>(a: T) -> T\n",
        "impl const fn framed<T>(a: T) -> T requires window(T) => a\n",
        // The arrow says five.
        "operator wide<T>(a: T) -> T\n",
        "impl const fn wide<T>(a: T) -> T requires window(T) -> list<T, 5> => a\n",
        // No candidate gives four.
        "operator odd<T>(a: T) -> T\n",
        "impl const fn odd<T>(a: T) -> T requires window(T) -> list<T, 4> => a\n",
        // TYPE-14: an arrow that is optional still says which list.
        "operator maybe<T>(a: T) -> T\n",
        "impl const fn maybe<T>(a: T) -> T requires window(T) -> list<T, 4>? => a\n",
        // GEN-13: a default the call would take is of its parameter's
        // type as the types resolve it: `2` is no `str`.
        "operator choose<T, const fallback: T = 2>(value: T) -> T\n",
        "impl const fn choose(value: i64) -> i64 => value\n",
        "impl const fn choose(value: str) -> str => value\n",
        "operator chosen<T>(a: T) -> T\n",
        "impl const fn chosen<T>(a: T) -> T requires choose(T) -> T => a\n",
        // GEN-2: an abstract struct widens what an earlier operand
        // decided.
        "abstract struct Shape { id: i64 }\n",
        "struct Round : Shape { radius: f64 }\n",
        "operator same<T, const mode: i64 = 0>(left: T, right: T) -> T\n",
        "impl const fn same(left: Shape, right: Shape) -> Shape => left\n",
        "operator paired<A, B>(a: A, b: B) -> A\n",
        "impl const fn paired<A, B>(a: A, b: B) -> A requires same(A, B) => a\n",
        // The arrow is held to as well as the default: `count` gives an
        // `i64` whatever its `mode`, and so no `str`.
        "operator count<T, const mode: i64 = 0>(value: T) -> i64\n",
        "impl const fn count(value: bool) -> i64 => 1\n",
        "operator texted<T>(a: T) -> T\n",
        "impl const fn texted<T>(a: T) -> T requires count(T) -> str => a\n",
        "operator counted<T>(a: T) -> T\n",
        "impl const fn counted<T>(a: T) -> T requires count(T) -> i64 => a\n",
        "const fn uses(x: f64, text: str, round: Round, shape: Shape) {\n",
        "    let three = framed(x)\n",
        "    let five = wide(x)\n",
        "    let four = odd(x)\n",
        "    let none = maybe(x)\n",
        "    let whole = chosen(1)\n",
        "    let words = chosen(text)\n",
        "    let widened = paired(round, shape)\n",
        "    let no = texted(true)\n",
        "    let yes = counted(true)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:34:16 operator/operator.no_candidate GEN-5",
            "m.hgl:35:16 operator/operator.no_candidate GEN-5",
            "m.hgl:37:17 operator/operator.no_candidate GEN-5",
            "m.hgl:39:14 operator/operator.no_candidate GEN-5",
        ]
    );
    assert_eq!(
        locals(&checked, "uses")[4..],
        [
            "three: f64",
            "five: f64",
            "four: _",
            "none: _",
            "whole: i64",
            "words: _",
            "widened: Round",
            "no: _",
            "yes: bool",
        ]
    );
}

/// GEN-8: an operator's generic value parameter takes "the default
/// where none of them says", and so takes part in what a call requests.
/// GEN-7: a requirement is the call it names as that call asks: one
/// that gives another output than the call requests is not it.
#[test]
fn a_required_call_requests_what_its_defaults_say() {
    let checked = module(concat!(
        "operator window<T, const size: i64 = 3>(value: T) -> list<T, size>\n",
        // The place says five, and so does the requirement.
        "const fn five<T>(value: T) -> list<T, 5>\n",
        "    requires window(T) -> list<T, 5> => window(value)\n",
        // Nothing says the size: the default, three.
        "const fn three<T>(value: T) requires window(T) -> list<T, 3> {\n",
        "    let got = window(value)\n",
        "    let written = window<T, 3>(value)\n",
        "}\n",
        // The requirement is of five, and the call asks for three.
        "const fn other<T>(value: T) requires window(T) -> list<T, 5> {\n",
        "    let got = window(value)\n",
        "    let asked: list<T, 5> = window(value)\n",
        "}\n",
        // With no arrow the requirement is of the bare call, which asks
        // for the default: three, and not five.
        "const fn bare<T>(value: T) requires window(T) {\n",
        "    let got = window(value)\n",
        "    let wide = window<T, 5>(value)\n",
        "}\n",
        // The same holds where a requirement is evidence for a
        // candidate's: `framed` asks for the bare call.
        "operator framed<T>(a: T) -> T\n",
        "impl const fn framed<T>(a: T) -> T requires window(T) => a\n",
        "const fn by_three<T>(value: T) -> T requires window(T) -> list<T, 3> => framed(value)\n",
        "const fn by_bare<T>(value: T) -> T requires window(T) => framed(value)\n",
        "const fn by_five<T>(value: T) -> T requires window(T) -> list<T, 5> => framed(value)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:10:15 operator/operator.no_candidate GEN-5",
            "m.hgl:15:16 operator/operator.no_candidate GEN-5",
            "m.hgl:21:72 operator/operator.no_candidate GEN-5",
        ]
    );
    assert_eq!(
        locals(&checked, "three")[1..],
        ["got: list<T, 3>", "written: list<T, 3>"]
    );
    assert_eq!(
        locals(&checked, "other")[1..],
        ["got: _", "asked: list<T, 5>"]
    );
    assert_eq!(
        locals(&checked, "bare")[1..],
        ["got: list<T, 3>", "wide: _"]
    );
}

/// GEN-16: "a named operator requirement tests callability with the
/// supplied types." Core's OPS-9 has its requirements name value
/// operations, "Addition requires a value `add_(T, T) -> T`", which a
/// node's hook calls (FN-3). So a requirement is tested against the
/// value candidates whatever declares it, and what a graph states meets
/// what a node's candidate asks (GEN-7).
#[test]
fn a_requirement_names_a_value_operation() {
    let checked = module(concat!(
        "operator tick<V>(value: V) -> V\n",
        // A temporal candidate alone for an `i64`; a value one for an
        // `f64`.
        "impl fn tick(value: i64) -> i64 {\n",
        "    when { return value }\n",
        "}\n",
        "impl const fn tick(value: f64) -> f64 => value\n",
        "operator pass<T>(a: T) -> T\n",
        "impl fn pass<T>(a: T) -> T requires tick(T) -> T => tick(a)\n",
        "fn uses(n: i64, x: f64) -> f64 {\n",
        "    let whole = pass(n)\n",
        "    return pass(x)\n",
        "}\n",
        "operator held<T>(a: T) -> T\n",
        "impl fn held<T>(a: T) -> T requires tick(T) -> T {\n",
        "    when { return tick(a) }\n",
        "}\n",
        "fn stated<T>(a: T) -> T requires tick(T) -> T => held(a)\n",
        "fn unstated<T>(a: T) -> T => held(a)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:10:17 operator/operator.no_candidate GEN-5",
            "m.hgl:18:30 operator/operator.no_candidate GEN-5",
        ]
    );
}

/// GEN-7: a generic body's call of a candidate that requires an
/// operation of its parameter is admitted where the body's own
/// requirements state that operation, and rejected where they do not.
/// GEN-15: "negation binds before conjunction".
#[test]
fn a_requirement_is_met_by_the_callers_own() {
    let checked = module(concat!(
        "operator total<T>(a: T, b: T) -> T\n",
        "impl const fn total<T>(a: T, b: T) -> T requires add_(T, T) -> T => add_(a, b)\n",
        "const fn stated<T>(a: T) -> T requires add_(T, T) -> T => total(a, a)\n",
        "const fn unstated<T>(a: T) -> T => total(a, a)\n",
        // A requirement nothing states is still met by a candidate that
        // takes any one type: core's `eq_<T>`.
        "operator alike<T>(a: T, b: T) -> bool\n",
        "impl const fn alike<T>(a: T, b: T) -> bool requires eq_(T, T) -> bool => a == b\n",
        "const fn compared<T>(a: T, b: T) -> bool => alike(a, b)\n",
        // Two candidates told apart by a requirement and its negation.
        "operator describe<T>(a: T) -> str\n",
        "impl const fn describe<T>(a: T) -> str requires add_(T, T) -> T => \"added\"\n",
        "impl const fn describe<T>(a: T) -> str requires !add_(T, T) -> T => \"kept\"\n",
        "const fn uses(n: i64, flag: bool) {\n",
        "    let added = describe(n)\n",
        "    let kept = describe(flag)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:5:36 operator/operator.no_candidate GEN-5"]
    );
    assert_eq!(callees(&checked, "stated"), ["none"]);
    assert_eq!(callees(&checked, "compared"), ["none"]);
    assert_eq!(callees(&checked, "uses"), ["candidate", "candidate"]);
}

/// FN-18 and GEN-16: a named requirement supplies the ordinary operands;
/// a brace entry is type-checked in its own configuration slot.
#[test]
fn a_required_operator_accepts_well_typed_configuration() {
    let checked = module(concat!(
        "operator pick<T>{fallback: T? = null}(value: T) -> i64\n",
        "impl const fn pick{fallback: i64?}(value: i64) -> i64 => value\n",
        "impl const fn pick{fallback: bool?}(value: bool) -> i64 => 0\n",
        "const fn configured<T>(value: T) -> i64 requires pick(T) -> i64 =>\n",
        "    pick{fallback: value}(value)\n",
        "const fn bad<T>(value: T) -> i64 requires pick(T) -> i64 =>\n",
        "    pick{fallback: 1}(value)\n",
        "const fn one() -> i64 => configured(1)\n",
        "const fn zero() -> i64 => configured(true)\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:8:20 type/type.mismatch EXPR-5"],
        "{}",
        checked.report
    );
    assert_eq!(callees(&checked, "configured"), ["pick"]);
}

/// GEN-7, FN-3: "Any body calls an operator; in a runtime body or a
/// value function the call selects a value candidate." A node's hook
/// and a graph's body are checked under their declaration's
/// requirements as a value function's is. FN-6: in a graph the
/// operation is lifted where it is given a connection.
#[test]
fn a_node_and_a_graph_are_checked_under_their_requirements() {
    let checked = module(concat!(
        "fn node_sum<T>(lhs: T, rhs: T) -> T requires add_(T, T) -> T {\n",
        "    when { return add_(lhs, rhs) }\n",
        "}\n",
        "fn graph_sum<T>(lhs: T, rhs: T) -> T requires add_(T, T) -> T {\n",
        "    let sum = lhs + rhs\n",
        "    return sum\n",
        "}\n",
        "fn graph_bare<T>(lhs: T, rhs: T) -> T => add_(lhs, rhs)\n",
        // FN-3: a connection given to a `const` parameter of the
        // operation required is rejected, as at any call.
        "operator freeze<T>(const value: T) -> T\n",
        "fn frozen<T>(x: T) -> T requires freeze(T) -> T => freeze(x)\n",
        // Given no connection, what a required operation gives may be
        // a connection or a value: a temporal candidate or a value one
        // (GEN-5). It is not settled here, and is not judged where a
        // constant is required.
        "operator source<O>() -> O\n",
        "operator width<V>(of: V) -> i64\n",
        "fn sized(value: f64, const size: i64) -> f64 => value\n",
        "fn sourced<T>(price: f64, const sample: T) -> f64\n",
        "    requires source() -> T && width(T) -> i64 {\n",
        "    let made = source()\n",
        "    let wide = width(sample)\n",
        "    return sized(price, wide)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:9:42 operator/operator.no_candidate GEN-5",
            "m.hgl:11:59 phase/connection.as_constant FN-3",
        ]
    );
    let unsettled: Vec<(&str, ValueCategory)> = body(&checked, "sourced").locals[2..]
        .iter()
        .map(|local| (local.name.as_str(), local.category))
        .collect();
    assert_eq!(
        unsettled,
        [
            ("made", ValueCategory::Unsettled),
            ("wide", ValueCategory::Unsettled),
        ]
    );
    assert_eq!(callees(&checked, "node_sum"), ["add_"]);
    assert_eq!(callees(&checked, "graph_sum"), ["add_"]);
    let sum = &body(&checked, "graph_sum").locals[2];
    assert_eq!(
        (sum.name.as_str(), sum.category),
        ("sum", ValueCategory::Connection)
    );
}

/// An answer as a test reads it: the candidate as `name(types)`, with
/// what answers its own requirements after it in brackets; `stated N`
/// for the enclosing declaration's requirement N; `unread`.
fn answer(checked: &Checked, answer: &Answer) -> String {
    match answer {
        Answer::Candidate {
            module,
            declaration,
            requirements,
            ..
        } => {
            let declared = &checked.program.modules[*module];
            let Some(Detail::Callable(signature)) = declared.details.get(*declaration) else {
                panic!("a candidate has a signature");
            };
            let types: Vec<String> = signature
                .parameters
                .iter()
                .map(|parameter| parameter.ty.to_string())
                .collect();
            let name = &declared.declarations[*declaration].name;
            let own: Vec<String> = requirements
                .iter()
                .map(|own| self::answer(checked, own))
                .collect();
            if own.is_empty() {
                format!("{name}({})", types.join(", "))
            } else {
                format!("{name}({}) [{}]", types.join(", "), own.join(", "))
            }
        }
        Answer::Stated(index) => format!("stated {index}"),
        Answer::Unread => "unread".to_string(),
    }
}

/// What answers the requirements of the callee of each call in `name`,
/// in the order the calls are met; and for a call of what the
/// declaration requires, `requirement N`.
fn answers(checked: &Checked, name: &str) -> Vec<String> {
    answers_in(checked, body(checked, name))
}

fn answers_in(checked: &Checked, body: &Body) -> Vec<String> {
    let mut found = Vec::new();
    hgl_analyser::hir::walk_statements(&body.statements, &mut |value| {
        let ExprKind::Call { callee, .. } = &value.kind else {
            return;
        };
        match callee {
            Callee::Function { requirements, .. } | Callee::Candidate { requirements, .. } => {
                let each: Vec<String> = requirements
                    .iter()
                    .map(|one| answer(checked, one))
                    .collect();
                found.push(each.join(", "));
            }
            Callee::Required { requirement, .. } => {
                found.push(format!("requirement {requirement}"))
            }
            Callee::Parameter { .. }
            | Callee::Anonymous(_)
            | Callee::DeferredOperator(_)
            | Callee::DeferredPackCall
            | Callee::DeferredValue
            | Callee::Unread => found.push("none".to_string()),
        }
    });
    found
}

/// GEN-16: "a named operator requirement tests callability with the
/// supplied types ... it is tested against the operator's value
/// candidates, and holds where one most-specific value candidate takes
/// the types supplied (GEN-5)". A call of a declaration records that
/// candidate for each requirement the declaration states, so that what
/// its body requires is known for that call. GEN-15: requirements
/// joined by `&&` all hold, and each has its answer, in the order
/// written.
#[test]
fn a_call_records_what_answers_each_requirement() {
    let checked = module(concat!(
        "operator fuse<L, R, O>(lhs: L, rhs: R) -> O\n",
        "impl const fn fuse(lhs: i64, rhs: i64) -> i64 => lhs\n",
        "impl const fn fuse(lhs: str, rhs: str) -> str => lhs\n",
        "const fn twice<T>(value: T) -> T requires fuse(T, T) -> T => fuse(value, value)\n",
        "const fn whole() -> i64 => twice(1)\n",
        "const fn text() -> str => twice(\"a\")\n",
        // Two requirements, and the body's calls of each.
        "const fn both<T>(value: T) -> T requires neg_(T) -> T && (fuse(T, T) -> T) =>\n",
        "    fuse(neg_(value), value)\n",
        "const fn uses() -> i64 => both(1)\n",
        // A function that states none has none answered.
        "const fn plain(value: i64) -> i64 => value\n",
        "const fn calls() -> i64 => plain(1)\n",
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(answers(&checked, "twice"), ["requirement 0"]);
    assert_eq!(answers(&checked, "whole"), ["fuse(i64, i64)"]);
    assert_eq!(answers(&checked, "text"), ["fuse(str, str)"]);
    // The call of `fuse` is met before its argument's call of `neg_`.
    assert_eq!(
        answers(&checked, "both"),
        ["requirement 1", "requirement 0"]
    );
    assert_eq!(answers(&checked, "uses"), ["neg_(i64), fuse(i64, i64)"]);
    assert_eq!(answers(&checked, "calls"), [""]);
}

/// GEN-7: "A generic body is checked under its stated requirements".
/// Where a call's types name a generic parameter of the declaration it
/// is written in, what that declaration states is the answer: which
/// candidate that is, is known where the declaration is called in turn.
/// A candidate that takes any one type answers for a parameter with
/// nothing stated.
#[test]
fn a_call_in_a_generic_body_is_answered_by_what_the_body_states() {
    let checked = module(concat!(
        "operator fuse<L, R, O>(lhs: L, rhs: R) -> O\n",
        "impl const fn fuse(lhs: i64, rhs: i64) -> i64 => lhs\n",
        "const fn twice<T>(value: T) -> T requires fuse(T, T) -> T => fuse(value, value)\n",
        "const fn outer<U>(value: U) -> U requires neg_(U) -> U && fuse(U, U) -> U =>\n",
        "    twice(value)\n",
        "const fn top() -> i64 => outer(2)\n",
        // Nothing stated, and a candidate for any one type.
        "operator show<T>(value: T) -> str\n",
        "impl const fn show<T>(value: T) -> str => \"\"\n",
        "const fn shown<T>(value: T) -> str requires show(T) -> str => show(value)\n",
        "const fn whatever<U>(value: U) -> str => shown(value)\n",
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(answers(&checked, "outer"), ["stated 1"]);
    assert_eq!(answers(&checked, "top"), ["neg_(i64), fuse(i64, i64)"]);
    assert_eq!(answers(&checked, "whatever"), ["show(T)"]);
}

/// GEN-16: the candidate that answers may state a requirement of its
/// own, which is answered with it: a `fuse` of two lists is the
/// candidate for lists, with the `fuse` of their elements.
#[test]
fn a_candidate_that_answers_has_its_own_requirements_answered() {
    let checked = module(concat!(
        "operator fuse<L, R, O>(lhs: L, rhs: R) -> O\n",
        "impl const fn fuse(lhs: i64, rhs: i64) -> i64 => lhs\n",
        "impl const fn fuse<T>(lhs: list<T>, rhs: list<T>) -> list<T>\n",
        "    requires fuse(T, T) -> T => lhs\n",
        "const fn twice<T>(value: T) -> T requires fuse(T, T) -> T => fuse(value, value)\n",
        "const fn lists() -> list<i64> => twice([1])\n",
        "const fn nested() -> list<list<i64>> => twice([[1]])\n",
        // A call of the operator itself selects the candidate, and
        // records what answers that candidate's requirement.
        "const fn direct(a: list<i64>) -> list<i64> => fuse(a, a)\n",
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(
        answers(&checked, "lists"),
        ["fuse(list<T>, list<T>) [fuse(i64, i64)]"]
    );
    assert_eq!(
        answers(&checked, "nested"),
        ["fuse(list<T>, list<T>) [fuse(list<T>, list<T>) [fuse(i64, i64)]]"]
    );
    assert_eq!(answers(&checked, "direct"), ["fuse(i64, i64)"]);
}

/// GEN-16: "A requirement names a value operation, whatever declares
/// it ... So what a graph requires is what a node or a value function
/// it calls may require." Core's OPS-9: the temporal candidate of `+`
/// under `{invalid: .identity}` requires a value `add_(T, T) -> T`. A
/// graph that states the same answers it, and a call of that graph
/// selects the value candidate.
#[test]
fn a_graph_answers_a_temporal_candidates_requirement() {
    let checked = module(concat!(
        "fn identity_sum<T>(lhs: T, rhs: T) -> T\n",
        "    requires add_(T, T) -> T => lhs +{invalid: .identity} rhs\n",
        "fn wired(a: f64, b: f64) -> f64 => identity_sum(a, b)\n",
        "fn texts(a: str, b: str) -> str => identity_sum(a, b)\n",
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(answers(&checked, "identity_sum"), ["none"]);
    for (caller, expected) in [("wired", "add_(f64, f64)"), ("texts", "add_(str, str)")] {
        let Some(Stmt::Return {
            value: Some(value), ..
        }) = body(&checked, caller).statements.last()
        else {
            panic!("a return with a call");
        };
        let ExprKind::Call {
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
        else {
            panic!("a call of identity_sum");
        };
        assert_eq!(answers(&checked, caller), [expected]);
        let (_, instance) = checked
            .program
            .instance(*module, *declaration, generics, requirements)
            .expect("identity_sum has a body");
        assert_eq!(answers_in(&checked, &instance), [expected]);
    }
}

/// GEN-3: "A call of a function whose `requires` does not hold with
/// the generic arguments the call resolves is rejected". The condition
/// is a membership of a type, a comparison of a generic value
/// parameter, given or left to its default, or a named operator
/// requirement (GEN-16), and it is reported at the call. Nothing
/// answers the requirement of such a call.
#[test]
fn a_call_that_does_not_meet_its_callees_requires_is_rejected() {
    let checked = module(concat!(
        "enum Mode { fast, slow }\n",
        "const fn twice<T>(value: T) -> T requires add_(T, T) -> T => value + value\n",
        "const fn numeric<T>(value: T) -> T requires T in {i64, f64} => value\n",
        "const fn exact<T>(value: T) -> T requires T == i64 => value\n",
        "const fn pick<const mode: Mode = .fast>(value: i64) -> i64\n",
        "    requires mode == .fast => value\n",
        "const fn eager<const mode: Mode = .slow>(value: i64) -> i64\n",
        "    requires mode == .fast => value\n",
        // Each holds.
        "const fn fine() -> i64 =>\n",
        "    twice(2) + numeric(3) + exact(4) + pick(5) + pick(6, mode: .fast) + eager(7, mode: .fast)\n",
        "const fn real() -> f64 => numeric(1.5)\n",
        // No value `add_` takes two `bool`.
        "const fn neither() -> bool => twice(true)\n",
        "const fn text() -> str => numeric(\"x\")\n",
        "const fn inexact() -> f64 => exact(1.5)\n",
        "const fn slow() -> i64 => pick(1, mode: .slow)\n",
        // The default is the value where the call gives none.
        "const fn defaulted() -> i64 => eager(1)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:13:31 constraint/requires.unmet GEN-3",
            "m.hgl:14:27 constraint/requires.unmet GEN-3",
            "m.hgl:15:30 constraint/requires.unmet GEN-3",
            "m.hgl:16:27 constraint/requires.unmet GEN-3",
            "m.hgl:17:32 constraint/requires.unmet GEN-3",
        ]
    );
    assert_eq!(answers(&checked, "neither"), ["unread"]);
}

/// GEN-3, GEN-10 and GEN-16: `each` checks every captured type, including
/// a named pack's types, and holds over an empty capture. A named
/// requirement asks for a candidate for the current type.
#[test]
fn a_call_checks_each_type_its_pack_requires() {
    let checked = module(concat!(
        "const fn all_i64<...Ts>(items: ...Ts) -> i64 requires each T in types(Ts) { T == i64 } => 0\n",
        "const fn named_i64<...Ts>(items: ...{Ts}) -> i64 requires each T in types(Ts) { T == i64 } => 0\n",
        "operator allowed<T>(x: T) -> T\n",
        "impl const fn allowed(x: i64) -> i64 => x\n",
        "const fn needs_judgment<...Ts>(items: ...Ts) -> i64\n",
        "    requires each T in types(Ts) { T == i64 && allowed(T) -> T } => 0\n",
        "const fn good() -> i64 => all_i64(1, 2)\n",
        "const fn empty() -> i64 => all_i64()\n",
        "const fn good_named() -> i64 => named_i64(an: 1, other: 2)\n",
        "const fn mixed() -> i64 => all_i64(1, true)\n",
        "const fn mixed_named() -> i64 => named_i64(an: 1, other: true)\n",
        "const fn callable() -> i64 => needs_judgment(1)\n",
        "const fn false_member() -> i64 => needs_judgment(1, \"no\")\n",
        "const fn empty_condition() -> i64 => needs_judgment()\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:11:28 constraint/requires.unmet GEN-3",
            "m.hgl:12:34 constraint/requires.unmet GEN-3",
            "m.hgl:14:35 constraint/requires.unmet GEN-3",
        ],
        "{}",
        checked.report
    );
    assert!(
        checked
            .report
            .contains("the `requires` of `needs_judgment` does not hold for this call")
    );
}

/// GEN-7 and GEN-10: a generic caller's stated memberships prove a
/// requirement on a fixed argument or on a member of the captured pack.
#[test]
fn a_generic_body_proves_requirements_on_calls_with_packs() {
    let checked = module(concat!(
        "const fn numeric<T, ...Ts>(value: T, rest: ...Ts) -> i64\n",
        "    requires T in {i64, f64} && each R in types(Ts) { R in {i64, f64} } => 1\n",
        "const fn fixed<U>(value: U) -> i64 requires U in {i64, f64} => numeric(value, 1)\n",
        "const fn packed<U>(value: U) -> i64 requires U in {i64, f64} => numeric(1, value)\n",
        "const fn invalid<U>(value: U) -> i64 requires U in {i64, str} => numeric(1, value)\n",
    ));
    let reports = checked.summary();
    assert_eq!(reports.len(), 1, "{}", checked.report);
    assert!(reports[0].starts_with("m.hgl:6:"), "{}", checked.report);
    assert!(
        reports[0].contains("constraint/requires.unmet GEN-7"),
        "{}",
        checked.report
    );
    assert!(checked.report.contains("`U` = `str`"), "{}", checked.report);
}

/// GEN-3: "and so is an application of a struct whose `requires` does
/// not hold with its arguments": where the struct is constructed, its
/// arguments written or found from its fields (GEN-2), and where a type
/// that applies it is written, at any depth.
#[test]
fn an_application_that_does_not_meet_a_structs_requires_is_rejected() {
    let checked = module(concat!(
        "struct Sized<T> requires T in {i64, f64} { value: T }\n",
        "struct Summed<T> requires add_(T, T) -> T { value: T }\n",
        "const fn made() -> Sized<i64> => Sized(value: 1)\n",
        "const fn written() -> Sized<f64> => Sized<f64>(value: 1.5)\n",
        "const fn summed() -> Summed<i64> => Summed(value: 1)\n",
        "const fn found() -> bool {\n",
        "    let held = Sized(value: \"x\")\n",
        "    return true\n",
        "}\n",
        "const fn given() -> bool {\n",
        "    let held = Sized<str>(value: \"x\")\n",
        "    return true\n",
        "}\n",
        "const fn taken(held: Sized<str>) -> bool => true\n",
        "const fn nested(held: list<Sized<bool>>) -> bool => true\n",
        // A named operator requirement is told where the struct is
        // constructed.
        "const fn unsummed() -> bool {\n",
        "    let held = Summed(value: true)\n",
        "    return true\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:8:16 constraint/requires.unmet GEN-3",
            "m.hgl:12:16 constraint/requires.unmet GEN-3",
            "m.hgl:15:22 constraint/requires.unmet GEN-3",
            "m.hgl:16:23 constraint/requires.unmet GEN-3",
            "m.hgl:18:16 constraint/requires.unmet GEN-3",
        ]
    );
}

/// GEN-7: "A generic body is checked under its stated requirements". A
/// call there whose types name the body's own generic parameter meets a
/// named operator requirement where the body states it, and does not
/// where it states none. A membership the body does not state is not
/// known to hold either, and the call rejects where the body is declared
/// (GEN-7: "It is judged once, where it is declared, by what its
/// `requires` states"; hhenson/hgl_spec#144).
#[test]
fn a_call_in_a_generic_body_meets_what_the_body_states() {
    let checked = module(concat!(
        "const fn twice<T>(value: T) -> T requires add_(T, T) -> T => value + value\n",
        "const fn numeric<T>(value: T) -> T requires T in {i64, f64} => value\n",
        "const fn stated<U>(value: U) -> U requires add_(U, U) -> U => twice(value)\n",
        "const fn unstated<U>(value: U) -> U => twice(value)\n",
        "const fn member<U>(value: U) -> U => numeric(value)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:5:40 constraint/requires.unmet GEN-3",
            "m.hgl:6:38 constraint/requires.unmet GEN-7",
        ]
    );
}

/// GEN-8: "A call may supply its generic arguments as a generic
/// constructor does ... The value a call gives a generic value
/// parameter is a compile-time constant: a literal, an enum member
/// however it is written (TYPE-7), `Zero.nan` or `.nan`, a constant
/// (UNIT-13) ... or a constant expression of those". GEN-3: a
/// `requires` is held to the value however it is written and wherever
/// it is given: after the callee's name or by name, in a call, a
/// construction and a type.
#[test]
fn a_requires_is_held_to_a_generic_value_however_it_is_written() {
    let checked = module(concat!(
        "enum Mode { fast, slow }\n",
        "const SLOW: Mode = .slow\n",
        "const OFF: bool = false\n",
        "const fn pick<const enabled: bool>(value: i64) -> i64 requires enabled == true => value\n",
        "const fn moded<const mode: Mode = .fast>(value: i64) -> i64\n",
        "    requires mode == .fast => value\n",
        "const fn sized<const size: i64>(value: i64) -> i64 requires size in {2, 3} => value\n",
        "const fn named<const label: str>(value: i64) -> i64 requires label == \"a\" => value\n",
        "const fn real<const scale: f64>(value: i64) -> i64 requires scale == 1.5 => value\n",
        "struct Flag<const on: bool> requires on == true { value: i64 }\n",
        "struct Moded<const mode: Mode> requires mode == .fast { value: i64 }\n",
        // Line 13: each of these does not hold.
        "const fn a() -> i64 => pick<false>(1)\n",
        "const fn b() -> i64 => pick(1, enabled: false)\n",
        "const fn c() -> i64 => moded<.slow>(1)\n",
        "const fn d() -> i64 => moded<Mode.slow>(1)\n",
        "const fn e() -> i64 => moded<SLOW>(1)\n",
        "const fn f() -> i64 => moded(1, mode: SLOW)\n",
        "const fn g() -> i64 => sized<4>(1)\n",
        "const fn h() -> i64 => sized<1 + 3>(1)\n",
        "const fn i() -> i64 => named<\"b\">(1)\n",
        "const fn j() -> i64 => real<2.5>(1)\n",
        "const fn k() -> i64 => real(1, scale: 2.5)\n",
        "const fn l() -> i64 => pick<OFF>(1)\n",
        "const fn m(held: Flag<false>) -> i64 => 1\n",
        "const fn n(held: Moded<.slow>) -> i64 => 1\n",
        "const fn o(held: Moded<Mode.slow>) -> i64 => 1\n",
        "const fn p(held: list<Flag<false>>) -> i64 => 1\n",
        "const fn q(held: Moded<SLOW>) -> i64 => 1\n",
        "const fn r() -> i64 {\n",
        "    let held = Flag<false>(value: 1)\n",
        "    return 1\n",
        "}\n",
        "const fn s() -> i64 {\n",
        "    let held = Moded<SLOW>(value: 1)\n",
        "    return 1\n",
        "}\n",
        // Each of these holds.
        "const fn fine(x: Flag<true>, y: Moded<.fast>, z: Moded<Mode.fast>) -> i64 =>\n",
        "    pick<true>(1) + moded<.fast>(1) + moded<Mode.fast>(1) + moded(1) + sized<2>(1)\n",
        "    + named<\"a\">(1) + real<1.5>(1) + Flag<true>(value: 1).value\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:13:24 constraint/requires.unmet GEN-3",
            "m.hgl:14:24 constraint/requires.unmet GEN-3",
            "m.hgl:15:24 constraint/requires.unmet GEN-3",
            "m.hgl:16:24 constraint/requires.unmet GEN-3",
            "m.hgl:17:24 constraint/requires.unmet GEN-3",
            "m.hgl:18:24 constraint/requires.unmet GEN-3",
            "m.hgl:19:24 constraint/requires.unmet GEN-3",
            "m.hgl:20:24 constraint/requires.unmet GEN-3",
            "m.hgl:21:24 constraint/requires.unmet GEN-3",
            "m.hgl:22:24 constraint/requires.unmet GEN-3",
            "m.hgl:23:24 constraint/requires.unmet GEN-3",
            "m.hgl:24:24 constraint/requires.unmet GEN-3",
            "m.hgl:25:18 constraint/requires.unmet GEN-3",
            "m.hgl:26:18 constraint/requires.unmet GEN-3",
            "m.hgl:27:18 constraint/requires.unmet GEN-3",
            "m.hgl:28:18 constraint/requires.unmet GEN-3",
            "m.hgl:29:18 constraint/requires.unmet GEN-3",
            "m.hgl:31:16 constraint/requires.unmet GEN-3",
            "m.hgl:35:16 constraint/requires.unmet GEN-3",
        ]
    );
}

/// GEN-3: a function, and a struct, with no generic parameter may
/// state a `requires` of types it names itself, and a call, a
/// construction and a written type are held to it.
#[test]
fn a_requires_with_no_generic_parameter_is_held_too() {
    let checked = module(concat!(
        "const fn blocked(value: i64) -> i64 requires add_(bool, bool) -> bool => value\n",
        "const fn open(value: i64) -> i64 requires add_(i64, i64) -> i64 => value + value\n",
        "fn node_blocked(value: i64) -> i64 requires add_(bool, bool) -> bool {\n",
        "    when { return value }\n",
        "}\n",
        "struct Shut requires add_(bool, bool) -> bool { value: i64 }\n",
        "struct Ajar requires add_(i64, i64) -> i64 { value: i64 }\n",
        // Line 9.
        "const fn a() -> i64 => blocked(1)\n",
        "const fn b() -> i64 => open(1)\n",
        "fn c(value: i64) -> i64 => node_blocked(value)\n",
        "const fn d() -> i64 => Shut(value: 1).value\n",
        "const fn e() -> i64 => Ajar(value: 1).value\n",
        "const fn f(held: Shut, other: Ajar) -> i64 => 1\n",
        "test g {\n",
        "    let seen = eval node_blocked(value: [1])\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:9:24 constraint/requires.unmet GEN-3",
            "m.hgl:11:28 constraint/requires.unmet GEN-3",
            "m.hgl:12:24 constraint/requires.unmet GEN-3",
            "m.hgl:14:18 constraint/requires.unmet GEN-3",
            "m.hgl:16:21 constraint/requires.unmet GEN-3",
        ]
    );
}

/// GEN-3, GEN-16: a struct's named operator requirement is told
/// wherever a type that applies it is written: in a function's
/// signature, a struct's field, an operator's and a native function's
/// parameters. GEN-7: where its argument is a generic parameter of the
/// declaration that writes it, it holds where that declaration states
/// it.
#[test]
fn a_structs_operator_requirement_is_told_where_its_type_is_written() {
    let checked = module(concat!(
        "struct Summed<T> requires add_(T, T) -> T { value: T }\n",
        // Each of these holds.
        "const fn whole(held: Summed<i64>) -> i64 => 1\n",
        "const fn stated<U>(held: Summed<U>) -> i64 requires add_(U, U) -> U => 1\n",
        "struct Stated<U> requires add_(U, U) -> U { held: Summed<U> }\n",
        // Line 6: each of these does not.
        "const fn flags(held: Summed<bool>) -> i64 => 1\n",
        "const fn unstated<U>(held: Summed<U>) -> i64 => 1\n",
        "struct Holder { held: Summed<bool> }\n",
        "struct Unstated<U> { held: Summed<U> }\n",
        "operator through(held: Summed<bool>) -> bool\n",
        "native const fn outside(held: Summed<bool>) -> bool\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:6:22 constraint/requires.unmet GEN-3",
            "m.hgl:7:28 constraint/requires.unmet GEN-3",
            "m.hgl:8:23 constraint/requires.unmet GEN-3",
            "m.hgl:9:28 constraint/requires.unmet GEN-3",
            "m.hgl:10:24 constraint/requires.unmet GEN-3",
            "m.hgl:11:31 constraint/requires.unmet GEN-3",
        ]
    );
}

/// GEN-2: "a generic value parameter that nothing says anything of
/// takes its default". GEN-3: a `requires` is held to a default too,
/// where the default names a constant (UNIT-13) or is an expression of
/// constants: for a call, a construction and a written type, each of
/// which leaves the parameter out.
#[test]
fn a_requires_is_held_to_a_default_that_names_a_constant() {
    let checked = module(concat!(
        "enum Mode { fast, slow }\n",
        "const SLOW: Mode = .slow\n",
        "const OFF: bool = false\n",
        "const ON: bool = true\n",
        "const TWO: i64 = 2\n",
        "const fn pick<const enabled: bool = OFF>(value: i64) -> i64\n",
        "    requires enabled == true => value\n",
        "const fn keep<const enabled: bool = ON>(value: i64) -> i64\n",
        "    requires enabled == true => value\n",
        "const fn moded<const mode: Mode = SLOW>(value: i64) -> i64\n",
        "    requires mode == .fast => value\n",
        "const fn plus<const size: i64 = TWO + 1>(value: i64) -> i64 requires size == 2 => value\n",
        "struct Flag<const on: bool = OFF> requires on == true { value: i64 }\n",
        "struct Lit<const on: bool = ON> requires on == true { value: i64 }\n",
        "struct Wide<const size: i64 = TWO + 1> requires size == 2 { value: i64 }\n",
        "struct Exact<const size: i64> requires size == 2 { value: i64 }\n",
        // Line 18: each of these does not hold.
        "const fn a() -> i64 => pick(1)\n",
        "const fn b() -> i64 => moded(1)\n",
        "const fn c() -> i64 => plus(1)\n",
        "const fn d() -> i64 => plus<TWO + 2>(1)\n",
        "const fn e() -> i64 => plus(1, size: TWO * 2)\n",
        "const fn f() -> i64 => Flag(value: 1).value\n",
        "const fn g() -> i64 => Wide(value: 1).value\n",
        "const fn h() -> i64 => Exact<TWO + 1>(value: 1).value\n",
        "const fn i(held: Flag) -> i64 => 1\n",
        "const fn j(held: Wide) -> i64 => 1\n",
        "const fn k(held: Exact<TWO + 1>) -> i64 => 1\n",
        "struct Holder { held: Flag }\n",
        // Each of these holds.
        "const fn fine(held: Lit, exact: Exact<TWO>, sum: Exact<1 + 1>) -> i64 =>\n",
        "    keep(1) + pick(1, enabled: true) + pick<ON>(1) + plus<TWO>(1) + plus(1, size: TWO)\n",
        "    + Lit(value: 1).value + Flag<ON>(value: 1).value\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:18:24 constraint/requires.unmet GEN-3",
            "m.hgl:19:24 constraint/requires.unmet GEN-3",
            "m.hgl:20:24 constraint/requires.unmet GEN-3",
            "m.hgl:21:24 constraint/requires.unmet GEN-3",
            "m.hgl:22:24 constraint/requires.unmet GEN-3",
            "m.hgl:23:24 constraint/requires.unmet GEN-3",
            "m.hgl:24:24 constraint/requires.unmet GEN-3",
            "m.hgl:25:24 constraint/requires.unmet GEN-3",
            "m.hgl:26:18 constraint/requires.unmet GEN-3",
            "m.hgl:27:18 constraint/requires.unmet GEN-3",
            "m.hgl:28:18 constraint/requires.unmet GEN-3",
            "m.hgl:29:23 constraint/requires.unmet GEN-3",
        ]
    );
}

/// GEN-2: "A struct application supplies all or none". A type that
/// writes a struct's generic value is held to what it writes, however
/// that is spelled, and never to the parameter's default; one that
/// writes none is held to the default. Each application in a type is
/// read for itself.
#[test]
fn a_written_argument_of_a_type_is_not_taken_for_the_default() {
    let checked = module(concat!(
        "enum Mode { fast, slow }\n",
        "const SLOW: Mode = .slow\n",
        "const FAST: Mode = .fast\n",
        "const TWO: i64 = 2\n",
        "struct Pick<const mode: Mode = SLOW> requires mode == .fast { value: i64 }\n",
        "struct Wide<const size: i64 = TWO + 1> requires size == 2 { value: i64 }\n",
        // Each of these holds: what is written is what counts.
        "const fn a(held: Pick<Mode.fast>) -> i64 => 1\n",
        "const fn b(held: Pick<.fast>) -> i64 => 1\n",
        "const fn c(held: Pick<FAST>) -> i64 => 1\n",
        "const fn d(held: Wide<TWO>) -> i64 => 1\n",
        "const fn e(held: tuple<Pick<FAST>, Wide<2>>) -> i64 => 1\n",
        // Line 13: each of these does not.
        "const fn f(held: Pick) -> i64 => 1\n",
        "const fn g(held: Pick<Mode.slow>) -> i64 => 1\n",
        "const fn h(held: Pick<SLOW>) -> i64 => 1\n",
        "const fn i(held: Wide) -> i64 => 1\n",
        "const fn j(held: Wide<TWO + 1>) -> i64 => 1\n",
        "const fn k(held: tuple<Pick<FAST>, Pick>) -> i64 => 1\n",
        "const fn l(held: tuple<Pick, Pick<FAST>>) -> i64 => 1\n",
        // TYPE-7: an enum's member "however it is written", its
        // constructor among the ways.
        "const fn m(held: Pick<Mode(\"slow\")>) -> i64 => 1\n",
        "const fn n(held: Pick<Mode(1)>) -> i64 => 1\n",
        "const fn o(held: Pick<(Mode.slow)>) -> i64 => 1\n",
        "const fn p(held: list<Pick<Mode(\"slow\")>>) -> i64 => 1\n",
        "const fn q() -> i64 => Pick<Mode(\"slow\")>(value: 1).value\n",
        "const fn fine(held: Pick<Mode(\"fast\")>, other: Pick<Mode(0)>) -> i64 =>\n",
        "    Pick<Mode(\"fast\")>(value: 1).value\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:13:18 constraint/requires.unmet GEN-3",
            "m.hgl:14:18 constraint/requires.unmet GEN-3",
            "m.hgl:15:18 constraint/requires.unmet GEN-3",
            "m.hgl:16:18 constraint/requires.unmet GEN-3",
            "m.hgl:17:18 constraint/requires.unmet GEN-3",
            "m.hgl:18:18 constraint/requires.unmet GEN-3",
            "m.hgl:19:18 constraint/requires.unmet GEN-3",
            "m.hgl:20:18 constraint/requires.unmet GEN-3",
            "m.hgl:21:18 constraint/requires.unmet GEN-3",
            "m.hgl:22:18 constraint/requires.unmet GEN-3",
            "m.hgl:23:18 constraint/requires.unmet GEN-3",
            "m.hgl:24:24 constraint/requires.unmet GEN-3",
        ]
    );
}

/// UNIT-13: a constant "names one ordinary value". TYPE-7: an enum's
/// member is one "however it is written". A constant of an enum is the
/// member its expression names, read where the constant is declared:
/// `Mode.slow`, `Mode("slow")`, `Mode(1)`, or another constant, which
/// is followed. GEN-3: a `requires` is held to it wherever the constant
/// is given.
#[test]
fn a_requires_is_held_to_a_constant_however_its_value_is_written() {
    let checked = module(concat!(
        "enum Mode { fast, slow }\n",
        "const A: Mode = Mode.slow\n",
        "const B: Mode = Mode(\"slow\")\n",
        "const C: Mode = Mode(1)\n",
        "const D: Mode = A\n",
        "const E: Mode = (Mode.slow)\n",
        "const OFF: bool = false\n",
        "const STILL_OFF: bool = OFF\n",
        "const F1: Mode = Mode.fast\n",
        "const F2: Mode = Mode(\"fast\")\n",
        "const F3: Mode = F1\n",
        "const fn pick<const mode: Mode = .fast>(value: i64) -> i64\n",
        "    requires mode == .fast => value\n",
        "const fn flag<const on: bool>(value: i64) -> i64 requires on == true => value\n",
        "struct Pick<const mode: Mode = A> requires mode == .fast { value: i64 }\n",
        // Line 17: each of these does not hold.
        "const fn a() -> i64 => pick<A>(1)\n",
        "const fn b() -> i64 => pick<B>(1)\n",
        "const fn c() -> i64 => pick<C>(1)\n",
        "const fn d() -> i64 => pick<D>(1)\n",
        "const fn e() -> i64 => pick(1, mode: E)\n",
        "const fn f() -> i64 => flag<STILL_OFF>(1)\n",
        "const fn g(held: Pick<B>) -> i64 => 1\n",
        "const fn h(held: Pick) -> i64 => 1\n",
        // Each of these holds.
        "const fn fine(held: Pick<F2>) -> i64 => pick<F1>(1) + pick<F2>(1) + pick(1, mode: F3)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:17:24 constraint/requires.unmet GEN-3",
            "m.hgl:18:24 constraint/requires.unmet GEN-3",
            "m.hgl:19:24 constraint/requires.unmet GEN-3",
            "m.hgl:20:24 constraint/requires.unmet GEN-3",
            "m.hgl:21:24 constraint/requires.unmet GEN-3",
            "m.hgl:22:24 constraint/requires.unmet GEN-3",
            "m.hgl:23:18 constraint/requires.unmet GEN-3",
            "m.hgl:24:18 constraint/requires.unmet GEN-3",
        ]
    );
}
