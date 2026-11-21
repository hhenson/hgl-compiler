//! Omitted configuration defaults of a selected operator candidate (GEN-6, UNIT-8).

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
