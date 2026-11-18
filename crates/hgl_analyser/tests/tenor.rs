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
