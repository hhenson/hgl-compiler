//! The analyser against TYPE-17, the tenor, and its literal (LEX-28).
//! Each test cites the rule.
//!
//! A finding is written `file:line:column category/code rule`.

mod common;

use common::{Checked, check};

/// Checks one file, `m.hgl`, of module `m`. The `module` line is added,
/// so the text given starts on line 2.
fn module(text: &str) -> Checked {
    check(&[("m.hgl", &format!("module m\n{text}"))])
}

/// The locals of the function `name` of module `m`, each as `name: type`.
fn locals(checked: &Checked, name: &str) -> Vec<String> {
    let module = checked.program.module("m").expect("module m exists");
    let index = module
        .declarations
        .iter()
        .position(|declaration| declaration.name == name)
        .expect("the declaration exists");
    module.bodies[index]
        .as_ref()
        .expect("it has a body")
        .locals
        .iter()
        .map(|local| format!("{}: {}", local.name, local.ty))
        .collect()
}

/// TYPE-17: "A `tenor` is a rule for moving a `date`, and no elapsed
/// time." LEX-28: "Case tells a tenor from a duration: `3M` is three
/// months and `3m` three minutes." A tenor literal is a `tenor`, the
/// type is written `tenor`, and neither is the other's.
#[test]
fn a_tenor_literal_is_a_tenor_and_no_duration() {
    let checked = module(concat!(
        "struct Leg {\n",
        "    lag: tenor = 2B\n",
        "    length: tenor\n",
        "}\n",
        "const fn rules(given: tenor) -> tenor {\n",
        "    let quarter = 3M\n",
        "    let spot: tenor = 2B\n",
        "    let leg = Leg(length: 1Y6M)\n",
        "    let kept: list<tenor> = [1W, given, leg.lag]\n",
        "    return quarter\n",
        "}\n",
        // Line 13: a tenor where a duration is expected, and a duration
        // where a tenor is.
        "const fn mixed() -> i64 {\n",
        "    let wait: duration = 3M\n",
        "    let months: tenor = 3m\n",
        "    return 1\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:14:26 type/type.mismatch EXPR-5",
            "m.hgl:15:25 type/type.mismatch EXPR-5",
        ]
    );
    assert_eq!(
        locals(&checked, "rules"),
        [
            "given: tenor",
            "quarter: tenor",
            "spot: tenor",
            "leg: Leg",
            "kept: list<tenor>",
        ]
    );
}

/// Core's TEN-1: "The symbol operators admit the operand types of the
/// first table below. Every other pairing is a source rejection,
/// `operator.no_candidate`. Scaling commutes ...; a date is the left
/// operand." The pairings are core's candidates, found as any
/// operator's are. TEN-5, TEN-7: `calendar` and `rolling_rule` are
/// options of `+` and `-`, each an argument of the candidate that takes
/// it (EXPR-10): a calendar is a value `business_day` has a candidate
/// for, and a rolling rule one `roll` has.
#[test]
fn a_tenors_operators_and_options_are_cores_candidates() {
    let checked = module(concat!(
        "struct Closed {\n",
        "    day: date\n",
        "}\n",
        "impl const fn business_day(calendar: Closed, day: date) -> bool => day != calendar.day\n",
        "const fn found(day: date, closed: Closed) -> i64 {\n",
        "    let moved = day + 3M\n",
        "    let back = day - 2B\n",
        "    let both = 6M + 6M\n",
        "    let less = 1Y6M - 6M\n",
        "    let scaled = 2 * 1Y6M\n",
        "    let other = 1Y6M * 2\n",
        "    let turned = -(1M)\n",
        "    let ordered = 1M < 2M\n",
        "    let outcome = 1M <=> 45D\n",
        "    let same = 1Y == 12M\n",
        "    let text = convert<str>(13D)\n",
        "    let settled = day +{calendar: closed} 2B\n",
        "    let rolled = day +{rolling_rule: Rolling.following} 3M\n",
        "    let each = day -{rolling_rule: Rolling.preceding, calendar: closed} 3M\n",
        "    return 1\n",
        "}\n",
        // Line 23: a tenor on the left of a date, an instant that is no
        // date, a division, a calendar that is none, and an option no
        // candidate takes.
        "const fn refused(day: date, at: datetime) -> i64 {\n",
        "    let turned = 3M + day\n",
        "    let instant = at + 3M\n",
        "    let halved = 3M / 2\n",
        "    let counted = day +{calendar: 5} 2B\n",
        "    let named = day +{holidays: 5} 2B\n",
        "    return 1\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:24:21 operator/operator.no_candidate OPS-1",
            "m.hgl:25:22 operator/operator.no_candidate OPS-1",
            "m.hgl:26:21 operator/operator.no_candidate OPS-1",
            "m.hgl:27:23 operator/operator.no_candidate OPS-1",
            "m.hgl:28:23 name/argument.unknown EXPR-10",
        ]
    );
    assert_eq!(
        locals(&checked, "found")[2..],
        [
            "moved: date",
            "back: date",
            "both: tenor",
            "less: tenor",
            "scaled: tenor",
            "other: tenor",
            "turned: tenor",
            "ordered: bool",
            "outcome: CmpResult",
            "same: bool",
            "text: str",
            "settled: date",
            "rolled: date",
            "each: date",
        ]
    );
}
