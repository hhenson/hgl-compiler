//! Configuration of a selected operator candidate (GEN-6, FN-18, UNIT-8).

mod common;

use common::{check, module};

#[test]
fn a_generic_inherited_default_is_held_to_the_selected_parameter() {
    // GEN-6, UNIT-8: a call leaving the configuration out takes the
    // operator's default as a value of the selected parameter's type.
    let checked = module(
        "operator pick<T>{fallback: T = 1}(value: T) -> T
impl const fn pick{fallback: bool}(value: bool) -> bool => fallback
const fn bad() -> bool => pick(false)",
    );
    assert_eq!(
        checked.summary(),
        ["m.hgl:2:32 type/type.mismatch EXPR-5"],
        "{}",
        checked.report
    );
}

#[test]
fn supplying_the_configuration_leaves_the_default_unused() {
    // FN-18, GEN-6: the explicit configuration wins. The unused
    // generic default need not fit the chosen bool candidate.
    let checked = module(
        "operator pick<T>{fallback: T = 1}(value: T) -> T
impl const fn pick{fallback: bool}(value: bool) -> bool => fallback
const fn given() -> bool => pick{fallback: false}(true)",
    );
    assert!(checked.findings.is_empty(), "{}", checked.report);
}

#[test]
fn a_default_on_a_required_operator_is_checked_in_the_instance() {
    // GEN-16, UNIT-8: the candidate only becomes known in the instance.
    let checked = module(
        "operator pick<T>{fallback: T = 1}(value: T) -> T
impl const fn pick{fallback: bool}(value: bool) -> bool => fallback
impl const fn pick{fallback: i64}(value: i64) -> i64 => fallback
const fn forward<T>(value: T) -> T requires pick(T) -> T => pick(value)
const fn given<T>(value: T) -> T requires pick(T) -> T => pick{fallback: value}(value)
const fn from_integer() -> i64 => forward(5)
const fn explicit() -> bool => given(false)
const fn invalid() -> bool => forward(false)",
    );
    assert_eq!(
        checked.summary(),
        ["m.hgl:2:32 type/type.mismatch EXPR-5"],
        "{}",
        checked.report
    );
}

#[test]
fn required_configuration_is_held_to_the_selected_parameter() {
    // FN-18, GEN-16: the public bool and a selected candidate's
    // parameter are both checked when configuration is supplied.
    let checked = module(
        "operator pick<T>{fallback: bool = false}(value: T) -> i64
impl const fn pick{fallback: i64}(value: bool) -> i64 => fallback
impl const fn pick{fallback: bool}(value: i64) -> i64 => if fallback { 1 } else { value }
const fn forward<T>(value: T) -> i64 requires pick(T) -> i64 => pick{fallback: true}(value)
const fn accepted() -> i64 => forward(5)
const fn rejected() -> i64 => forward(false)",
    );
    assert_eq!(
        checked.summary(),
        ["m.hgl:5:80 type/type.mismatch EXPR-5"],
        "{}",
        checked.report
    );
}

#[test]
fn an_inherited_default_reports_its_required_calls_configuration_error() {
    // GEN-16, FN-18: after the operator's requirement is answered,
    // the call inside its default checks the selected configuration type.
    let checked = module(
        "operator make<T>{flag: bool = false}() -> T
impl const fn make{flag: i64}() -> i64 => flag
operator pick<T>{fallback: T = make<T>{flag: true}()}(input: T) -> T requires make() -> T
impl const fn pick{fallback: i64}(input: i64) -> i64 => fallback
const fn first() -> i64 => pick(1)
const fn second() -> i64 => pick(2)",
    );
    assert_eq!(
        checked.summary(),
        ["m.hgl:4:46 type/type.mismatch EXPR-5"],
        "{}",
        checked.report
    );
}

#[test]
fn a_required_configuration_literal_can_widen_but_a_binding_cannot() {
    // FN-18, EXPR-5: the integer literal fits a float place, but an
    // integer binding is never implicitly converted to a float.
    let checked = module(
        "operator choose<T>{fallback: i64 = 0}(value: T) -> f64
impl const fn choose{fallback: f64}(value: bool) -> f64 => fallback
const fn literal<T>(value: T) -> f64 requires choose(T) -> f64 => choose{fallback: 3}(value)
const fn binding<T>(value: T, fallback: i64) -> f64 requires choose(T) -> f64 => choose{fallback: fallback}(value)
const fn accepted() -> f64 => literal(false)
const fn rejected() -> f64 => binding(false, 3)",
    );
    assert_eq!(
        checked.summary(),
        ["m.hgl:5:99 type/type.mismatch EXPR-5"],
        "{}",
        checked.report
    );
}

#[test]
fn public_literal_widening_does_not_preempt_the_selected_parameter() {
    // EXPR-5: the same written literal fits the public float place and
    // the selected candidate's integer place.
    let checked = module(
        "operator pick<T>{fallback: f64 = 0}(value: T) -> i64
impl const fn pick{fallback: i64}(value: bool) -> i64 => fallback
const fn forward<T>(value: T) -> i64 requires pick(T) -> i64 => pick{fallback: 3}(value)
const fn accepted() -> i64 => forward(false)",
    );
    assert!(checked.findings.is_empty(), "{}", checked.report);
}

#[test]
fn a_default_is_checked_when_a_candidate_type_is_specialized() {
    // GEN-7, UNIT-8: the candidate is known in the body, and its
    // configuration type becomes concrete in each instance.
    let checked = module(
        "operator pick<T>{fallback: T = 1}(value: T) -> T
impl const fn pick<T>{fallback: T}(value: T) -> T => fallback
const fn forward<T>(value: T) -> T => pick(value)
const fn from_integer() -> i64 => forward(5)
const fn invalid() -> bool => forward(false)",
    );
    assert_eq!(
        checked.summary(),
        ["m.hgl:2:32 type/type.mismatch EXPR-5"],
        "{}",
        checked.report
    );
}

#[test]
fn a_fixed_public_default_is_also_checked_against_the_candidate() {
    // UNIT-8: the public annotation accepting the default does not
    // make that value fit a different type at the selected slot.
    let checked = module(
        "operator pick{fallback: i64 = 1}(value: bool) -> bool
impl const fn pick{fallback: bool}(value: bool) -> bool => fallback
const fn bad() -> bool => pick(false)
const fn given() -> bool => pick{fallback: false}(true)",
    );
    assert_eq!(
        checked.summary(),
        ["m.hgl:2:31 type/type.mismatch EXPR-5"],
        "{}",
        checked.report
    );
}

#[test]
fn inherited_literals_take_the_selected_configuration_type() {
    // UNIT-8, EXPR-5, TYPE-14: integer literals widen to f64 and null
    // takes the nullable type of its place.
    let checked = module(
        "operator floating<T>{fallback: T = 1}(value: T) -> T
impl const fn floating{fallback: f64}(value: f64) -> f64 => fallback
const fn widened() -> f64 => floating(2.0)

operator optional<T>{fallback: T = null}(value: T) -> T
impl const fn optional{fallback: f64?}(value: f64?) -> f64? => fallback
const fn absent() -> f64? => optional(2.0)",
    );
    assert!(checked.findings.is_empty(), "{}", checked.report);
}

#[test]
fn an_inherited_null_needs_a_nullable_type() {
    // UNIT-8 and TYPE-14: an unresolved type initially accepts null.
    // A selected nonnullable ordinary parameter rejects it.
    let checked = module(
        "operator pick<T>{fallback: T = null}(value: T) -> T
impl const fn pick{fallback: bool}(value: bool) -> bool => fallback
const fn bad() -> bool => pick(false)",
    );
    assert_eq!(
        checked.summary(),
        ["m.hgl:2:32 type/type.mismatch TYPE-14"],
        "{}",
        checked.report
    );
}

#[test]
fn an_inherited_default_keeps_its_operator_scope_and_generics() {
    // UNIT-7, UNIT-8: the value comes from the operator's enclosing
    // module, even for a candidate and caller in other modules.
    let checked = check(&[
        (
            "owner.hgl",
            "module owner
const offset: i64 = 2
const fn from<const n: i64>() -> i64 => n + offset
export operator configured<const amount: i64, T>{fallback: T = from<amount>()}(value: T) -> T",
        ),
        (
            "candidate.hgl",
            "module candidate
use owner::{configured}
impl const fn configured{fallback: i64}(value: i64) -> i64 => fallback",
        ),
        (
            "caller.hgl",
            "module caller
use owner::{configured}
const fn configured_value() -> i64 => configured<8>(1)",
        ),
    ]);
    assert!(checked.findings.is_empty(), "{}", checked.report);
}
