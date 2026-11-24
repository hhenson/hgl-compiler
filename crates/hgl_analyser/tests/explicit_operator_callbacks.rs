//! FN-17 and MAP-2: fixed callback inputs behind a public `any` input.

mod common;

use common::module;

const PICK: &str = "operator pick<R>(value: any) -> R
impl fn pick(value: i64) -> i64 => value + 1
";

#[test]
fn a_written_any_callback_supplies_its_fixed_input_to_map() {
    // FN-17, MAP-2: the callback's scalar slot precedes fitting the list.
    for argument in ["values", "value: values"] {
        let checked = module(&format!(
            "{PICK}fn mapped(values: list<i64, 2>) -> list<i64, 2> => map(pick<i64>, {argument})"
        ));
        assert!(checked.findings.is_empty(), "{}", checked.report);
    }
}

#[test]
fn a_written_result_can_resolve_the_candidates_input_generic() {
    // FN-17, GEN-8: the result binds T before its fixed input is projected.
    let checked = module(
        "operator identity<R>(value: any) -> R
impl fn identity<T>(value: T) -> T => value
fn mapped(values: list<i64, 2>) -> list<i64, 2> => map(identity<i64>, values)",
    );
    assert!(checked.findings.is_empty(), "{}", checked.report);
}

#[test]
fn ordinary_any_forwarding_keeps_the_supplied_type() {
    // FN-17: typed operands still select between candidates with one result.
    let checked = module(
        "operator choose<R>(value: any) -> R
impl fn choose(value: i64) -> i64 => value + 1
impl fn choose(value: f64) -> i64 => 7
fn forward<R, ...Ps, ...As>(f: fn(...{Ps}) -> R, args: ...{As for f}) -> R => f(...args)
fn ints(value: i64) -> i64 => forward(choose<i64>, value)
fn floats(value: f64) -> i64 => forward(choose<i64>, value)",
    );
    assert!(checked.findings.is_empty(), "{}", checked.report);
}

#[test]
fn a_projected_callback_does_not_project_an_ordinary_forwarded_argument() {
    // EXPR-5, FN-8: the real list cannot be passed to the scalar slot.
    let checked = module(&format!(
        "{PICK}fn forward<R, ...Ps, ...As>(f: fn(...{{Ps}}) -> R, args: ...{{As for f}}) -> R => f(...args)
fn bad(values: list<i64, 2>) -> i64 => forward(pick<i64>, values)"
    ));
    assert_eq!(checked.findings.len(), 1, "{}", checked.report);
    assert_eq!(checked.findings[0].code, "type/type.mismatch");
    assert!(checked.report.contains("this call makes the instance"));
}

#[test]
fn a_projected_callback_still_checks_maps_element_contract() {
    // GEN-3, MAP-2: projection supplies no conversion of a typed list member.
    let checked = module(&format!(
        "{PICK}fn bad(values: list<f64, 2>) -> list<i64, 2> => map(pick<i64>, values)"
    ));
    assert_eq!(checked.findings.len(), 1, "{}", checked.report);
    assert_eq!(checked.findings[0].code, "operator/operator.no_candidate");
}

#[test]
fn a_written_result_still_constrains_the_callback() {
    // FN-17: a callback returning i64 is not one returning f64.
    let checked = module(&format!(
        "{PICK}fn bad(values: list<i64, 2>) -> list<f64, 2> => map(pick<f64>, values)"
    ));
    assert_eq!(checked.findings.len(), 1, "{}", checked.report);
    assert_eq!(checked.findings[0].code, "type/type.mismatch");
}

#[test]
fn a_callback_with_no_input_evidence_is_not_guessed() {
    // FN-17: without supplied types, even a sole candidate does not select.
    let checked = module(&format!(
        "{PICK}fn inspect<R, ...Ps>(f: fn(...{{Ps}}) -> R) -> i64 => 1
fn bad() -> i64 => inspect(pick<i64>)"
    ));
    assert_eq!(checked.findings.len(), 1, "{}", checked.report);
    assert_eq!(checked.findings[0].code, "type/generic.unresolved");
}

#[test]
fn projected_inputs_keep_the_templates_phase_preference() {
    // FN-17, GEN-5: a temporal receiving template prefers that role.
    let checked = module(
        "operator phased<R>(value: any) -> R
impl const fn phased(value: i64) -> i64 => value + 100
impl fn phased(value: i64) -> i64 => value + 1
fn mapped(values: list<i64, 2>) -> list<i64, 2> => map(phased<i64>, values)",
    );
    assert!(checked.findings.is_empty(), "{}", checked.report);
}

#[test]
fn projected_inputs_keep_concrete_over_generic_specificity() {
    // GEN-5: a concrete input still outranks a type variable.
    let checked = module(
        "operator preferred<R>(value: any) -> R
impl fn preferred<T>(value: T) -> T => value
impl fn preferred(value: i64) -> i64 => value + 10
fn mapped(values: list<i64, 2>) -> list<i64, 2> => map(preferred<i64>, values)",
    );
    assert!(checked.findings.is_empty(), "{}", checked.report);
}
