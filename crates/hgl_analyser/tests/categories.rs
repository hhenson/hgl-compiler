//! What the analyser records on the typed program of each expression,
//! a connection or an ordinary value (FN-3), and of each call, whether
//! it is lifted (FN-6), for the stages after it. Each test cites its
//! rule.

mod common;

use common::{Checked, body, module};
use hgl_analyser::hir::{Body, Callee, ExprKind, ValueCategory, walk_statements};

/// No lines.
const NONE: [&str; 0] = [];

/// Each expression of `body`, an outer one before those it holds, as
/// its source text and what the analyser recorded of it: `connection`,
/// `value` or `unsettled`, and `lifted` after a call that is.
fn recorded(checked: &Checked, body: &Body) -> Vec<String> {
    let mut found = Vec::new();
    walk_statements(&body.statements, &mut |expression| {
        let text = checked
            .sources
            .file(expression.at.file)
            .slice(expression.at.span);
        let category = match expression.category {
            ValueCategory::Connection => "connection",
            ValueCategory::Value => "value",
            ValueCategory::Unsettled => "unsettled",
        };
        let lifted = if expression.is_lifted() {
            " lifted"
        } else {
            ""
        };
        found.push(format!("{text}: {category}{lifted}"));
    });
    found
}

/// The body of the instance that the first call of a function in the
/// body of `caller` asks for.
fn instance_called_by(checked: &Checked, caller: &str) -> Body {
    let mut call = None;
    walk_statements(&body(checked, caller).statements, &mut |expression| {
        if let ExprKind::Call {
            callee:
                Callee::Function {
                    module,
                    declaration,
                    generics,
                    requirements,
                    ..
                },
            ..
        } = &expression.kind
        {
            call.get_or_insert((
                *module,
                *declaration,
                generics.clone(),
                requirements.clone(),
            ));
        }
    });
    let (module, declaration, generics, requirements) = call.expect("the caller calls a function");
    let (_, instance) = checked
        .program
        .instance(module, declaration, &generics, &requirements)
        .expect("the function has a body");
    instance
}

/// FN-3: "There a temporal parameter, and the result of a temporal or
/// lifted call, is a connection: the argument of a call, the operand of
/// a symbol, a condition, a bound local, or what is returned." The
/// generics chapter: "Boolean forms `&&`, `||`, and `!` bind `and_`,
/// `or_`, and `not_` respectively", and FN-6: "A selected value helper,
/// a `const fn` or a value candidate of an operator, with temporal
/// arguments in composition is lifted ... All-scalar calls execute
/// directly". So `a && b` and `!a` of connections are connections, and
/// so is what a `const fn` given `!a` gives, which is lifted
/// (compiler issue 270).
#[test]
fn what_each_expression_of_a_graph_is_is_recorded() {
    let checked = module(concat!(
        "const fn neg(flag: bool) -> bool => flag\n",
        "fn both(a: bool, b: bool) -> bool => a && b\n",
        "fn negated(a: bool, const k: bool) -> bool => neg(!a) || neg(k)\n",
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(
        recorded(&checked, body(&checked, "both")),
        [
            "a && b: connection lifted",
            "a: connection",
            "b: connection"
        ]
    );
    assert_eq!(
        recorded(&checked, body(&checked, "negated")),
        [
            "neg(!a) || neg(k): connection lifted",
            "neg(!a): connection lifted",
            "!a: connection lifted",
            "a: connection",
            "neg(k): value",
            "k: value",
        ]
    );
}

/// EXPR-6: scalar Boolean forms inside graphs keep their first reading
/// when no flow facts change, including under explicit grouping.
#[test]
fn nested_scalar_boolean_forms_fit_the_reading_budget() {
    let expression = (0..20).fold("value".to_string(), |inside, _| {
        format!("({inside} || false)")
    });
    let checked = module(&format!(
        "fn nested(const value: bool) -> bool => {expression}\n"
    ));
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
}

/// FN-6: a value helper "with temporal arguments in composition is
/// lifted ... All-scalar calls execute directly and invent no source
/// timing". A temporal function's call is the node or the graph itself,
/// and a hook calls value functions as a value function does (FN-3: "In
/// a hook a temporal parameter that is not `ref` (FN-16) is a value of
/// its declared type"), so neither is lifted. In a value function
/// nothing is a connection.
#[test]
fn which_calls_are_lifted_is_recorded() {
    let checked = module(concat!(
        "const fn scale(value: f64, factor: f64 = 2.0) -> f64 => value * factor\n",
        "fn scaled(value: f64) -> f64 => scale(value)\n",
        "fn fixed(const value: f64) -> f64 => scale(value)\n",
        "fn passed(value: f64) -> f64 => scaled(value)\n",
        "fn read(value: f64) -> f64 {\n",
        "    when { return scale(value) }\n",
        "}\n",
        "const fn twice(value: f64) -> f64 => scale(value)\n",
    ));
    assert_eq!(checked.summary(), NONE);
    let lines = |name: &str| recorded(&checked, body(&checked, name));
    assert_eq!(
        lines("scaled"),
        ["scale(value): connection lifted", "value: connection"]
    );
    assert_eq!(lines("fixed"), ["scale(value): value", "value: value"]);
    assert_eq!(
        lines("passed"),
        ["scaled(value): connection", "value: connection"]
    );
    assert_eq!(lines("read"), ["scale(value): value", "value: value"]);
    assert_eq!(lines("twice"), ["scale(value): value", "value: value"]);
}

/// GEN-16: an instance's body has "each operation the declaration
/// requires as the call of the candidate that answers it", and GEN-7:
/// in the generic body which candidate that is, a temporal or a value
/// one (GEN-5), is not known, so what `add_(k, k)` gives is not settled
/// there. The instance records it: core's value candidate of `add_` for
/// two `i64` given two ordinary values is called while the graph is
/// built, and given a connection is lifted (FN-6). EXPR-3:
/// "Initialization fixes a binding's value/connection category", so
/// `s` is an ordinary value in the instance.
#[test]
fn an_instance_records_what_its_candidates_give() {
    let checked = module(concat!(
        "fn total<T>(const k: T, x: T) -> T requires add_(T, T) -> T {\n",
        "    let s = add_(k, k)\n",
        "    return add_(s, x)\n",
        "}\n",
        "fn used(x: i64) -> i64 => total(2, x)\n",
    ));
    assert_eq!(checked.summary(), NONE);
    let generic = body(&checked, "total");
    assert_eq!(
        generic
            .locals
            .iter()
            .map(|local| local.category)
            .collect::<Vec<_>>(),
        [
            ValueCategory::Value,
            ValueCategory::Connection,
            ValueCategory::Unsettled
        ]
    );
    assert_eq!(
        recorded(&checked, generic),
        [
            "add_(k, k): unsettled",
            "k: value",
            "k: value",
            "add_(s, x): connection",
            "s: unsettled",
            "x: connection",
        ]
    );
    let instance = instance_called_by(&checked, "used");
    assert_eq!(instance.locals[2].category, ValueCategory::Value);
    assert_eq!(
        recorded(&checked, &instance),
        [
            "add_(k, k): value",
            "k: value",
            "k: value",
            "add_(s, x): connection lifted",
            "s: value",
            "x: connection",
        ]
    );
}

/// EXPR-3: "Initialization fixes a binding's value/connection
/// category", and a block's value is the one it ends in (CTRL-10). A
/// local that the value of another declares is settled first in the
/// instance, and the other with it: `s` is what `t` is, an ordinary
/// value once core's value candidate of `add_` answers the requirement.
#[test]
fn an_instance_settles_a_local_that_a_value_declares() {
    let checked = module(concat!(
        "fn total<T>(const k: T, x: T) -> T requires add_(T, T) -> T {\n",
        "    let s = {\n",
        "        let t = add_(k, k)\n",
        "        t\n",
        "    }\n",
        "    return add_(s, x)\n",
        "}\n",
        "fn used(x: i64) -> i64 => total(2, x)\n",
    ));
    assert_eq!(checked.summary(), NONE);
    let instance = instance_called_by(&checked, "used");
    let categories: Vec<(&str, ValueCategory)> = instance
        .locals
        .iter()
        .map(|local| (local.name.as_str(), local.category))
        .collect();
    assert_eq!(
        categories,
        [
            ("k", ValueCategory::Value),
            ("x", ValueCategory::Connection),
            ("t", ValueCategory::Value),
            ("s", ValueCategory::Value),
        ]
    );
    let calls: Vec<String> = recorded(&checked, &instance)
        .into_iter()
        .filter(|line| line.starts_with("add_"))
        .collect();
    assert_eq!(
        calls,
        ["add_(k, k): value", "add_(s, x): connection lifted"]
    );
}
