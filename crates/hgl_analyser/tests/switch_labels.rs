//! The analyser against the rules on a `switch`'s labels: CTRL-6,
//! CTRL-12, CTRL-15 and UNIT-13. Each test cites its rule.
//!
//! A finding is written `file:line:column category/code rule`.

mod common;

use common::{Checked, check};

/// No lines.
const NONE: [&str; 0] = [];

/// Checks one file, `m.hgl`, of module `m`. The `module` line is added,
/// so the text given starts on line 2.
fn module(text: &str) -> Checked {
    check(&[("m.hgl", &format!("module m\n{text}"))])
}

/// CTRL-6: "Switch labels are typed source constants, compared by
/// resolved value. Duplicate values reject, including differently
/// spelled equal enum members." UNIT-13: "The constant has that one
/// value wherever it is read". A constant, of this module or another,
/// is its value, and so is a constant expression of integers.
#[test]
fn labels_are_compared_by_their_values() {
    let checked = check(&[
        (
            "m.hgl",
            concat!(
                "module m\n",
                "use n::{FIVE}\n",
                "enum Side { buy, sell }\n",
                "const ONE: i64 = 1\n",
                "const TWO: i64 = ONE + 1\n",
                "const BUY: Side = Side.buy\n",
                "const ALSO_BUY: Side = BUY\n",
                "const fn by_constant(route: i64) -> i64 {\n",
                "    switch route {\n",
                "        case ONE: return 10\n",
                "        case 1: return 100\n",
                "        default: return 0\n",
                "    }\n",
                "}\n",
                "fn in_a_graph(route: i64, a: i64) -> i64 {\n",
                "    switch route {\n",
                "        case TWO: return a\n",
                "        case 1 + 1: return a * 2\n",
                "        default: return a\n",
                "    }\n",
                "}\n",
                "const fn by_member(side: Side) -> i64 {\n",
                "    switch side {\n",
                "        case ALSO_BUY: return 10\n",
                "        case .buy: return 100\n",
                "        default: return 0\n",
                "    }\n",
                "}\n",
                "const fn imported(route: i64) -> i64 {\n",
                "    switch route {\n",
                "        case FIVE: return 1\n",
                "        case 5: return 2\n",
                "        default: return 0\n",
                "    }\n",
                "}\n",
                "const fn apart(route: i64) -> i64 {\n",
                "    switch route {\n",
                "        case ONE: return 1\n",
                "        case TWO: return 2\n",
                "        case -1: return 3\n",
                "        default: return 0\n",
                "    }\n",
                "}\n",
            ),
        ),
        ("n.hgl", "module n\nexport const FIVE: i64 = 5\n"),
    ]);
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:11:14 type/switch.duplicate_label CTRL-6",
            "m.hgl:18:14 type/switch.duplicate_label CTRL-6",
            "m.hgl:25:14 type/switch.duplicate_label CTRL-6",
            "m.hgl:32:14 type/switch.duplicate_label CTRL-6",
        ]
    );
}

/// CTRL-6: "Enum coverage can establish exhaustiveness", and CTRL-11:
/// a `switch` that covers its enum leaves no value out. A constant that
/// names a member covers it, so the function reaches no end with no
/// value.
#[test]
fn a_constant_label_covers_its_member() {
    let checked = module(concat!(
        "enum Side { buy, sell }\n",
        "const BUY: Side = Side.buy\n",
        "const fn quoted(side: Side) -> str {\n",
        "    switch side {\n",
        "        case BUY: return \"bid\"\n",
        "        case .sell: return \"ask\"\n",
        "    }\n",
        "}\n",
        "fn routed(side: Side, a: i64, b: i64) -> i64 {\n",
        "    switch side {\n",
        "        case BUY: return a\n",
        "        case .sell: return b\n",
        "    }\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE);
}

/// UNIT-13: a constant is "a compile-time constant where a generic value
/// argument (GEN-8) or a `switch` label (CTRL-12) requires one", and
/// GEN-8 says what one is: "a literal, an enum member however it is
/// written (TYPE-7) ... a constant (UNIT-13), a generic value parameter
/// in scope, or a constant expression of those. Any other rejects." A
/// `let`, a `const` parameter and a call are none; a connection is
/// `connection.as_constant` (FN-3).
#[test]
fn a_label_is_a_compile_time_constant() {
    let checked = module(concat!(
        "const fn doubled(value: i64) -> i64 => value * 2\n",
        "const fn by_let(route: i64) -> i64 {\n",
        "    let n = 1\n",
        "    switch route {\n",
        "        case n: return 1\n",
        "        default: return 0\n",
        "    }\n",
        "}\n",
        "fn by_parameter(route: i64, const n: i64) -> i64 {\n",
        "    switch route {\n",
        "        case n: return route\n",
        "        default: return route\n",
        "    }\n",
        "}\n",
        "const fn by_call(route: i64) -> i64 {\n",
        "    switch route {\n",
        "        case doubled(1): return 1\n",
        "        default: return 0\n",
        "    }\n",
        "}\n",
        "fn by_connection(route: i64, other: i64) -> i64 {\n",
        "    switch route {\n",
        "        case other: return route\n",
        "        default: return route\n",
        "    }\n",
        "}\n",
        "const fn by_generic<const k: i64>(route: i64) -> i64 {\n",
        "    switch route {\n",
        "        case k: return 1\n",
        "        case k + 1: return 2\n",
        "        case -1: return 3\n",
        "        default: return 0\n",
        "    }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:6:14 local:type/switch.label_constant CTRL-12",
            "m.hgl:12:14 local:type/switch.label_constant CTRL-15",
            "m.hgl:18:14 local:type/switch.label_constant CTRL-12",
            "m.hgl:24:14 phase/connection.as_constant FN-3",
        ]
    );
}

/// UNIT-13: "The constant has that one value wherever it is read". A
/// constant named many times over, each doubling the one before, is
/// read once each: checking it takes no time to speak of.
#[test]
fn a_constant_named_many_times_is_read_once() {
    let mut text = String::from("const C0: i64 = 1\n");
    for level in 1..=40 {
        text.push_str(&format!(
            "const C{level}: i64 = C{} + C{}\n",
            level - 1,
            level - 1
        ));
    }
    text.push_str(concat!(
        "const fn deep(route: i64) -> i64 {\n",
        "    switch route {\n",
        "        case C40: return 1\n",
        "        case 1099511627776: return 2\n",
        "        default: return 0\n",
        "    }\n",
        "}\n",
    ));
    let checked = module(&text);
    assert_eq!(
        checked.summary(),
        ["m.hgl:46:14 type/switch.duplicate_label CTRL-6"]
    );
}
