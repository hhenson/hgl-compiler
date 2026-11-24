//! GEN-7 and EXPR-5: generic operator literals take the selected instance's
//! type, and GEN-7 checks each use under that type with ordinary diagnostics.

mod common;
use common::check;

const DECLARATIONS: &str = r#"module m
operator n1<L, C>(items: C, zero: L? = null) -> L
impl const fn n1<V>(items: list<V>, zero: V? = null) -> V
    requires V in {i64, f64} => if zero == null { items[0] } else { zero + items[0] }
operator n2<L, C, const on: bool = true>(items: C, zero: L? = null) -> L
impl const fn n2<V, const on: bool = true>(items: list<V>, zero: V? = null) -> V
    requires V in {i64, f64} && on => if zero == null { items[0] } else { zero + items[0] }
"#;

#[test]
fn literals_are_admitted_in_functions_nodes_and_graphs() {
    // GEN-3, GEN-7: the float literal requires only the f64 membership;
    // an integer literal also fits each member of {i64, f64} (EXPR-5).
    for op in ["n1", "n2"] {
        for literal in ["1", "-1", "1.0", "-1.0"] {
            let members = if literal.contains('.') {
                "f64"
            } else {
                "i64, f64"
            };
            let value = format!("{op}(items, seed) + {op}(items, {literal})");
            for (prefix, body) in [
                ("const fn", format!("=> {value}")),
                ("fn", format!("{{ when {{ return {value} }} }}")),
                ("fn", format!("=> {value}")),
            ] {
                let source = format!(
                    "{DECLARATIONS}{prefix} observed<V>(items: list<V>, seed: V) -> V requires V in {{{members}}} {body}\n"
                );
                let checked = check(&[("m.hgl", &source)]);
                assert!(
                    !checked.rejected,
                    "{op}/{literal}/{body}: {}",
                    checked.report
                );
            }
        }
    }
}

#[test]
fn a_literal_still_needs_the_declared_membership() {
    // GEN-7: no candidate is known to admit unconstrained V here.
    for op in ["n1", "n2"] {
        let source =
            format!("{DECLARATIONS}const fn lit<V>(items: list<V>) -> V => {op}(items, 1)\n");
        let checked = check(&[("m.hgl", &source)]);
        assert!(checked.rejected, "{}", checked.report);
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
fn every_member_still_needs_a_candidate_for_a_float_literal() {
    // GEN-3: f64 is not an i64 argument; the i64 member lacks a candidate.
    for op in ["n1", "n2"] {
        let source = format!(
            "{DECLARATIONS}const fn lit<V>(items: list<V>) -> V requires V in {{i64, f64}} => {op}(items, 1.0)\n"
        );
        let checked = check(&[("m.hgl", &source)]);
        assert!(checked.rejected, "{}", checked.report);
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
fn selected_membership_results_recheck_aliases() {
    // GEN-7, EXPR-5: the f64 instance cannot return its selected result
    // as i64. The report continues to name the concrete instance.
    for op in ["n1", "n2"] {
        let source = format!(
            "{DECLARATIONS}const fn as_int<V>(items: list<V>) -> i64 requires V in {{i64, f64}} {{\n    let picked = {op}(items, 1)\n    let alias = picked\n    return alias\n}}\ntest first {{ as_int([1.5, 2.0]) }}\ntest second {{ as_int([2.5, 3.0]) }}\n"
        );
        let checked = check(&[("m.hgl", &source)]);
        assert!(checked.rejected, "{}", checked.report);
        assert!(
            checked
                .findings
                .iter()
                .all(|f| f.code == "type/type.mismatch"),
            "{}",
            checked.report
        );
        assert!(checked.report.contains("as_int<f64>"), "{}", checked.report);
    }
}

#[test]
fn a_written_result_is_not_erased_by_a_literal() {
    // GEN-5, GEN-8: the written i64 is still requested from each member.
    for op in ["n1", "n2"] {
        let source = format!(
            "{DECLARATIONS}const fn fixed<V>(items: list<V>) -> i64 requires V in {{i64, f64}} => {op}<i64>(items, 1)\n"
        );
        let checked = check(&[("m.hgl", &source)]);
        assert!(checked.rejected, "{}", checked.report);
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

/// GEN-7, EXPR-5: temporal instances also recheck a consumer of the
/// selected literal call's result, preserving the ordinary mismatch code.
#[test]
fn node_and_graph_instances_recheck_the_selected_literal_result() {
    for op in ["n1", "n2"] {
        let statements = format!("let picked = {op}(items, 1)\nlet alias = picked\nreturn alias");
        for body in [
            format!("{{ when {{ {statements} }} }}"),
            format!("{{ {statements} }}"),
        ] {
            let source = format!(
                "{DECLARATIONS}fn as_int<V>(items: list<V>) -> i64 requires V in {{i64, f64}} {body}\ntest invoke {{ let invalid = eval as_int(items: [[1.5, 2.0]]) }}\n"
            );
            let checked = check(&[("m.hgl", &source)]);
            assert!(checked.rejected, "{body}: {}", checked.report);
            assert!(
                checked
                    .findings
                    .iter()
                    .all(|f| f.code == "type/type.mismatch"),
                "{}",
                checked.report
            );
            assert!(checked.report.contains("as_int<f64>"), "{}", checked.report);
        }
    }
}
