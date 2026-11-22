//! The analyser against the comparison of two zoned datetimes: the
//! language's TYPE-2 and core's OPS-2, with the option `utc` (EXPR-10,
//! EXPR-14). Each test cites the rule.
//!
//! A finding is written `file:line:column category/code rule`.

mod common;

use common::{Checked, body, module};
use hgl_analyser::hir::{Callee, Expr, ExprKind, Stmt};
use hgl_analyser::{Detail, Type};

/// No lines.
const NONE: [&str; 0] = [];

/// The candidate each `let`, `return` and handler's `return` of `name`
/// calls, with its parameter types, and the parameters its arguments
/// are given to in the order they are written:
/// `eq_(zoned_datetime, zoned_datetime, bool) <- lhs, utc, rhs`.
fn selected(checked: &Checked, name: &str) -> Vec<String> {
    fn of(checked: &Checked, value: &Expr) -> String {
        let ExprKind::Call {
            callee:
                Callee::Candidate {
                    module,
                    declaration,
                    ..
                },
            arguments,
            ..
        } = &value.kind
        else {
            return format!("no candidate: {:?}", value.kind);
        };
        let declared = &checked.program.modules[*module].declarations[*declaration];
        let Some(Detail::Callable(signature)) =
            checked.program.modules[*module].details.get(*declaration)
        else {
            return "no signature".to_string();
        };
        let types: Vec<String> = signature
            .parameters
            .iter()
            .map(|parameter| parameter.ty.to_string())
            .collect();
        let given: Vec<&str> = arguments
            .iter()
            .map(|argument| {
                argument
                    .parameter
                    .map_or("?", |index| signature.parameters[index].name.as_str())
            })
            .collect();
        format!(
            "{}({}) <- {}",
            declared.name,
            types.join(", "),
            given.join(", ")
        )
    }
    // A handler's body is a statement of its own, with its statements.
    fn read(checked: &Checked, statements: &[Stmt], found: &mut Vec<String>) {
        for statement in statements {
            match statement {
                Stmt::Declare {
                    value: Some(value), ..
                }
                | Stmt::Return {
                    value: Some(value), ..
                } => found.push(of(checked, value)),
                Stmt::Hook { body, .. } => read(checked, body, found),
                _ => {}
            }
        }
    }
    let mut found = Vec::new();
    read(checked, &body(checked, name).statements, &mut found);
    found
}

/// TYPE-2: "Two `zoned_datetime` values are equal when both their
/// instants and their zone names are ... Compared as UTC, they are equal
/// when their instants are. That comparison is written `a =={utc: true}
/// b`, beside the tolerance options (EXPR-10)." OPS-2: "The option is
/// `utc`, a `bool` that is false unless given, `a =={utc: true} b`,
/// declared by the candidates of `eq_` and `ne_` for two zoned
/// datetimes."
///
/// `==` and `!=` of two zoned datetimes, with the option or without,
/// by symbol or by name, select core's candidate for them, whose `utc`
/// the option is given to: in a value function, in a node's handler and
/// in a graph. EXPR-13: the arguments are kept in the order they are
/// written, the option between the operands. The option may be a
/// parameter's value, and a generic value parameter's.
#[test]
fn a_zoned_comparison_selects_cores_candidate_with_utc() {
    let checked = module(concat!(
        "const fn value(a: zoned_datetime, b: zoned_datetime, flag: bool) {\n",
        "    let plain = a == b\n",
        "    let apart = a != b\n",
        "    let as_utc = a =={utc: true} b\n",
        "    let apart_as_utc = a !={utc: flag} b\n",
        "    let named = eq_(a, b, utc: true)\n",
        "    let named_apart = ne_(utc: false, lhs: a, rhs: b)\n",
        "}\n",
        "const fn generic<const as_utc: bool>(a: zoned_datetime, b: zoned_datetime) -> bool =>\n",
        "    a =={utc: as_utc} b\n",
        "fn hooked(a: zoned_datetime, b: zoned_datetime, flag: bool) -> bool {\n",
        "    when {\n",
        "        let plain = a == b\n",
        "        return a !={utc: flag} b\n",
        "    }\n",
        "}\n",
        "fn wired(a: zoned_datetime, b: zoned_datetime, const as_utc: bool) -> bool {\n",
        "    let plain = a != b\n",
        "    return a =={utc: as_utc} b\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(
        selected(&checked, "value"),
        [
            "eq_(zoned_datetime, zoned_datetime, bool) <- lhs, rhs",
            "ne_(zoned_datetime, zoned_datetime, bool) <- lhs, rhs",
            "eq_(zoned_datetime, zoned_datetime, bool) <- lhs, utc, rhs",
            "ne_(zoned_datetime, zoned_datetime, bool) <- lhs, utc, rhs",
            "eq_(zoned_datetime, zoned_datetime, bool) <- lhs, rhs, utc",
            "ne_(zoned_datetime, zoned_datetime, bool) <- utc, lhs, rhs",
        ]
    );
    assert_eq!(
        selected(&checked, "generic"),
        ["eq_(zoned_datetime, zoned_datetime, bool) <- lhs, utc, rhs"]
    );
    assert_eq!(
        selected(&checked, "hooked"),
        [
            "eq_(zoned_datetime, zoned_datetime, bool) <- lhs, rhs",
            "ne_(zoned_datetime, zoned_datetime, bool) <- lhs, utc, rhs",
        ]
    );
    assert_eq!(
        selected(&checked, "wired"),
        [
            "ne_(zoned_datetime, zoned_datetime, bool) <- lhs, rhs",
            "eq_(zoned_datetime, zoned_datetime, bool) <- lhs, utc, rhs",
        ]
    );
}

/// OPS-2: "The zoned types have no order": `<`, `<=`, `>`, `>=` and
/// `<=>` of two zoned datetimes, or of two zoned times, have no
/// candidate (OPS-1: "Every other pairing is a source rejection,
/// `operator.no_candidate`"), and neither has a `timezone`. OPS-1's
/// table gives a zoned datetime no difference of two.
///
/// EXPR-10: "Reject empty configuration, duplicate options, incompatible
/// types, and an option that no candidate for the operands declares."
/// `utc` is a `bool` (OPS-2), declared by the candidates for two zoned
/// datetimes alone: not for a `datetime`, a `zoned_time` or an
/// ordering; and the tolerances are for numbers (NUM-4). The zoned
/// candidate declares `utc` in its parentheses, so a graph can lift a
/// connection there in either spelling (EXPR-14).
#[test]
fn zoned_datetimes_have_no_order_and_utc_is_theirs_alone() {
    let checked = module(concat!(
        "const fn uses(a: zoned_datetime, b: zoned_datetime, t: zoned_time, zone: timezone, i: datetime) {\n",
        "    let r1 = a < b\n",
        "    let r2 = a <= b\n",
        "    let r3 = a > b\n",
        "    let r4 = a >= b\n",
        "    let r5 = a <=> b\n",
        "    let r6 = t < t\n",
        "    let r7 = zone >= zone\n",
        "    let r8 = compare_(a, b)\n",
        "    let r9 = a - b\n",
        // Line 12: the option.
        "    let o1 = a =={abs: 1.0} b\n",
        "    let o2 = a =={utc: 1} b\n",
        "    let o3 = a =={utc: null} b\n",
        "    let o4 = a =={utc: true, utc: false} b\n",
        "    let o5 = i =={utc: true} i\n",
        "    let o6 = t !={utc: true} t\n",
        "    let o7 = a <{utc: true} b\n",
        "    let o8 = lt_(a, b, utc: true)\n",
        "}\n",
        // Line 21: in a handler and in a graph alike.
        "fn hooked(a: zoned_datetime, b: zoned_datetime) -> bool {\n",
        "    when { return a < b }\n",
        "}\n",
        "fn wired(a: zoned_datetime, b: zoned_datetime, flag: bool) -> bool {\n",
        "    let ordered = a >= b\n",
        "    return a =={utc: flag} b\n",
        "}\n",
        "fn wired_named(a: zoned_datetime, b: zoned_datetime, flag: bool) -> bool => eq_(a, b, utc: flag)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:16 operator/operator.no_candidate OPS-2",
            "m.hgl:4:16 operator/operator.no_candidate OPS-2",
            "m.hgl:5:16 operator/operator.no_candidate OPS-2",
            "m.hgl:6:16 operator/operator.no_candidate OPS-2",
            "m.hgl:7:16 operator/operator.no_candidate OPS-2",
            "m.hgl:8:16 operator/operator.no_candidate OPS-2",
            "m.hgl:9:19 operator/operator.no_candidate OPS-2",
            "m.hgl:10:14 operator/operator.no_candidate GEN-5",
            "m.hgl:11:16 operator/operator.no_candidate OPS-1",
            "m.hgl:12:16 operator/operator.no_candidate OPS-2",
            "m.hgl:13:16 operator/operator.no_candidate OPS-2",
            "m.hgl:14:16 operator/operator.no_candidate OPS-2",
            "m.hgl:15:30 name/argument.duplicate EXPR-10",
            "m.hgl:16:16 operator/operator.no_candidate OPS-2",
            "m.hgl:17:16 operator/operator.no_candidate OPS-2",
            "m.hgl:18:18 name/argument.unknown EXPR-10",
            "m.hgl:19:24 name/argument.unknown EXPR-2",
            "m.hgl:22:21 operator/operator.no_candidate OPS-2",
            "m.hgl:25:21 operator/operator.no_candidate OPS-2",
        ]
    );
}

/// TYPE-2: zoned datetimes are values of one type, so a list, a set, a
/// dictionary and a struct hold them (TYPE-12: "An ordinary value
/// type"), and `==` of two of those is core's generic candidate, which
/// compares the zoned datetimes they hold as their own `==` does
/// (OPS-2). A set's member and a dictionary's key take no `?` (TYPE-14).
#[test]
fn a_value_that_holds_zoned_datetimes_is_compared_as_a_whole() {
    let checked = module(concat!(
        "struct Meeting { at: zoned_datetime }\n",
        "const fn uses(a: zoned_datetime, m: Meeting) {\n",
        "    let listed: list<zoned_datetime> = [a]\n",
        "    let members: set<zoned_datetime> = [a]\n",
        "    let keyed: dict<zoned_datetime, i64> = [a: 1]\n",
        "    let same_list = listed == listed\n",
        "    let same_set = members == members\n",
        "    let same_meeting = m == m\n",
        "}\n",
        // Line 11: a nullable member or key.
        "const fn bad(a: set<zoned_datetime?>, b: dict<zoned_datetime?, i64>) {}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:11:21 type/nullable.key TYPE-14",
            "m.hgl:11:47 type/nullable.key TYPE-14",
        ]
    );
    let compared = selected(&checked, "uses");
    assert_eq!(
        compared[3..],
        [
            "eq_(T, T) <- lhs, rhs",
            "eq_(T, T) <- lhs, rhs",
            "eq_(T, T) <- lhs, rhs",
        ]
    );
    let types: Vec<Type> = body(&checked, "uses").locals[2..5]
        .iter()
        .map(|local| local.ty.clone())
        .collect();
    assert_eq!(
        types.iter().map(ToString::to_string).collect::<Vec<_>>(),
        [
            "list<zoned_datetime>",
            "set<zoned_datetime>",
            "dict<zoned_datetime, i64>"
        ]
    );
}
