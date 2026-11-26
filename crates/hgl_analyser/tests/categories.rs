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

/// EXPR-6: a nullable guard does not make a nested scalar operand
/// depend on it when that operand uses different bindings.
#[test]
fn nested_nullable_guards_fit_the_reading_budget() {
    let parameters = (0..24)
        .map(|index| format!("const a{index}: i64?"))
        .collect::<Vec<_>>()
        .join(", ");
    for mixed in [false, true] {
        let expression = (0..24).rev().fold("true".to_string(), |inside, index| {
            if mixed && index % 2 == 1 {
                format!("(a{index} == null || {inside})")
            } else {
                format!("(a{index} != null && {inside})")
            }
        });
        let checked = module(&format!(
            "fn nested({parameters}) -> bool => {expression}\n"
        ));
        assert_eq!(checked.summary(), NONE, "{}", checked.report);
    }
}

/// EXPR-6 and ERR-12: each nested level uses the binding its guard refines.
/// Probing the graph's categories must not recursively replay scalar flow.
#[test]
fn nested_scalar_guarded_reads_fit_the_reading_budget() {
    // Match the analyser stack used by `hgl`; these 24 levels contain
    // two connectives apiece, beyond the default Rust test thread's stack.
    std::thread::Builder::new()
        .stack_size(hgl_analyser::stack_size(hgl_parser::Limits::default()))
        .spawn(|| {
            let parameters = (0..24)
                .map(|index| format!("const a{index}: i64?"))
                .collect::<Vec<_>>()
                .join(", ");
            for shape in 0..3 {
                let mut expression = if shape == 1 {
                    (0..24)
                        .map(|index| format!("positive(a{index})"))
                        .collect::<Vec<_>>()
                        .join(" && ")
                } else {
                    "true".to_string()
                };
                for index in (0..24).rev() {
                    expression = match shape {
                        1 => format!("a{index} != null && ({expression})"),
                        2 if index % 2 == 1 => {
                            format!("a{index} == null || (positive(a{index}) && ({expression}))")
                        }
                        _ => format!("a{index} != null && (positive(a{index}) && ({expression}))"),
                    };
                }
                let checked = module(&format!(
                    "const fn positive(value: i64) -> bool => value > 0\n\
                     fn nested({parameters}) -> bool => {expression}\n"
                ));
                assert_eq!(checked.summary(), NONE, "{}", checked.report);
            }
        })
        .expect("the analysis thread starts")
        .join()
        .expect("nested guarded reads check");
}

/// FN-3, FN-6 and EXPR-6: an outer connection retains its category while a
/// scalar subtree refines its own binding; value code keeps short-circuit flow.
#[test]
fn scalar_guarded_subtrees_keep_their_flow_inside_temporal_logic() {
    let checked = module(concat!(
        "const fn positive(value: i64) -> bool => value > 0\n",
        "const fn as_value(value: i64?) -> bool => value != null && positive(value)\n",
        "fn as_scalar(const value: i64?) -> bool => value != null && positive(value)\n",
        "fn mixed(tick: bool, const value: i64?) -> bool => \
         tick && (value != null && positive(value))\n",
    ));
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
    assert!(recorded(&checked, body(&checked, "mixed"))[0].ends_with(": connection lifted"));
    assert!(recorded(&checked, body(&checked, "as_scalar"))[0].ends_with(": value"));
}

/// EXPR-6, ERR-3/12 and FN-6: a scalar negation keeps its operand's
/// category while an outer connective learns its short-circuit facts.
#[test]
fn negated_scalar_calls_keep_short_circuit_presence_facts() {
    for expression in [
        "a == null || !positive(a)",
        "a != null && !positive(a)",
        "a == null || !!positive(a)",
        "!(a != null && !positive(a))",
        "a == null || !(positive(a) && positive(a))",
    ] {
        for (phase, parameter) in [("const fn", "a: i64?"), ("fn", "const a: i64?")] {
            let checked = module(&format!(
                "const fn positive<T>(value: T) -> bool requires T in {{i64}} => value > 0\n\
                 {phase} guarded({parameter}) -> bool => {expression}\n"
            ));
            assert_eq!(checked.summary(), NONE, "{expression}: {}", checked.report);
            assert!(
                recorded(&checked, body(&checked, "guarded"))[0].ends_with(": value"),
                "{expression}: scalar arguments remain values"
            );
        }
        let checked = module(&format!(
            "const fn positive<T>(value: T) -> bool requires T in {{i64}} => value > 0\n\
             fn mixed(tick: bool, const a: i64?) -> bool => tick || ({expression})\n"
        ));
        assert_eq!(checked.summary(), NONE, "{expression}: {}", checked.report);
        assert!(recorded(&checked, body(&checked, "mixed"))[0].ends_with(": connection lifted"));
    }
}

/// ERR-12 and FN-3/6: scalar facts do not protect the wrong binding,
/// the wrong branch, an earlier read, or a separate temporal argument.
#[test]
fn negated_calls_still_require_their_own_presence_fact() {
    for (parameters, expression) in [
        ("const a: i64?", "a == null && !positive(a)"),
        ("const a: i64?", "a != null || !positive(a)"),
        ("const a: i64?", "!positive(a) || a == null"),
        ("const a: i64?, const b: i64?", "a == null || !positive(b)"),
        ("a: i64?", "a == null || !positive(a)"),
    ] {
        let checked = module(&format!(
            "const fn positive<T>(value: T) -> bool requires T in {{i64}} => value > 0\n\
             fn guarded({parameters}) -> bool => {expression}\n"
        ));
        assert!(
            checked.findings.iter().any(|finding| {
                finding.code == "constraint/requires.unmet" && finding.rule == "GEN-3"
            }),
            "{expression}: {}",
            checked.report
        );
    }
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
