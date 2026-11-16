//! The analyser against the rules on named operator requirements: what
//! a declaration requires of its generic parameters is callable in its
//! body, and is tested where a call selects it. Each test cites its
//! rule.
//!
//! A finding is written `file:line:column category/code rule`.

mod common;

use common::{Checked, check};
use hgl_analyser::hir::{Body, Callee, Expr, ExprKind, Stmt};

/// No lines.
const NONE: [&str; 0] = [];

/// Checks one file, `m.hgl`, of module `m`. The `module` line is added,
/// so the text given starts on line 2.
fn module(text: &str) -> Checked {
    check(&[("m.hgl", &format!("module m\n{text}"))])
}

/// The typed body of the function `name` of module `m`.
fn body<'a>(checked: &'a Checked, name: &str) -> &'a Body {
    let module = checked.program.module("m").expect("module m exists");
    let index = module
        .declarations
        .iter()
        .position(|declaration| declaration.name == name)
        .expect("the declaration exists");
    module.bodies[index].as_ref().expect("it has a body")
}

/// The locals of `name` as `local: type`, parameters first.
fn locals(checked: &Checked, name: &str) -> Vec<String> {
    body(checked, name)
        .locals
        .iter()
        .map(|local| format!("{}: {}", local.name, local.ty))
        .collect()
}

/// What each statement of `name` that holds a call calls: the name of
/// the operator a requirement names, `candidate` for a candidate
/// selected, and `none` for a call with no callee.
fn callees(checked: &Checked, name: &str) -> Vec<String> {
    fn of(checked: &Checked, value: &Expr, found: &mut Vec<String>) {
        if let ExprKind::Call { callee, .. } = &value.kind {
            found.push(match callee {
                Callee::Required {
                    operator: (module, declaration),
                } => checked.program.modules[*module].declarations[*declaration]
                    .name
                    .clone(),
                Callee::Candidate { .. } => "candidate".to_string(),
                Callee::Function { .. } => "function".to_string(),
                Callee::Unread => "none".to_string(),
            });
        }
    }
    let mut found = Vec::new();
    for statement in &body(checked, name).statements {
        match statement {
            Stmt::Declare {
                value: Some(value), ..
            }
            | Stmt::Return {
                value: Some(value), ..
            }
            | Stmt::Expr(value) => of(checked, value, &mut found),
            Stmt::Hook { body, .. } => {
                for statement in body {
                    if let Stmt::Return {
                        value: Some(value), ..
                    } = statement
                    {
                        of(checked, value, &mut found);
                    }
                }
            }
            _ => {}
        }
    }
    found
}

/// GEN-7: "A generic body is checked under its stated requirements".
/// GEN-16: "a named operator requirement tests callability with the
/// supplied types. A result arrow constrains that operator's result
/// type". GEN-15: requirements joined by `&&` all hold. The generics
/// chapter: "A symbol is the call of its identity", so `a + b` is the
/// `add_(a, b)` a requirement names.
#[test]
fn a_generic_body_calls_what_its_requirements_state() {
    let checked = module(concat!(
        "const fn twice<T>(a: T) -> T requires add_(T, T) -> T => add_(a, a)\n",
        "const fn sum<T>(a: T, b: T) -> T requires add_(T, T) -> T => a + b\n",
        "const fn difference<T>(a: T, b: T) -> T\n",
        "    requires sub_(T, T) -> T && (neg_(T) -> T) {\n",
        "    let negated = -b\n",
        "    let whole = sub_(a, b)\n",
        "    return a - negated\n",
        "}\n",
        // A requirement with operands of two types, and of a type that
        // holds the parameter.
        "const fn scaled<T>(a: T, by: f64) -> list<T>\n",
        "    requires mul_(T, f64) -> T && add_(list<T>, list<T>) -> list<T> {\n",
        "    let one = mul_(a, by)\n",
        "    let ones: list<T> = [one]\n",
        // A sequence written out has no type of its own: it is read as
        // the type the requirement supplies in its place.
        "    let pair = [one] + [one]\n",
        "    let joined = add_(rhs: [a], lhs: ones)\n",
        "    return ones + ones\n",
        "}\n",
        // EXPR-2: "Named arguments use the selected public callable's
        // parameter names": the operator's, `lhs` and `rhs`.
        "const fn named<T>(a: T, b: T) -> T requires sub_(T, T) -> T {\n",
        "    let both = sub_(lhs: a, rhs: b)\n",
        "    let mixed = sub_(a, rhs: b)\n",
        "    return sub_(rhs: b, lhs: a)\n",
        "}\n",
        // With no arrow the requirement says only that the call is
        // admitted: what it gives is not known.
        "const fn loose<T>(a: T) requires add_(T, T) {\n",
        "    let unknown = add_(a, a)\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(callees(&checked, "twice"), ["add_"]);
    assert_eq!(callees(&checked, "sum"), ["add_"]);
    assert_eq!(callees(&checked, "difference"), ["neg_", "sub_", "sub_"]);
    assert_eq!(
        callees(&checked, "scaled"),
        ["mul_", "add_", "add_", "add_"]
    );
    assert_eq!(callees(&checked, "named"), ["sub_", "sub_", "sub_"]);
    // Each argument is recorded with the operator's parameter it is
    // given to: `sub_(rhs: b, lhs: a)` gives `b` to the second.
    let Some(Stmt::Return {
        value: Some(value), ..
    }) = body(&checked, "named").statements.last()
    else {
        panic!("a return with a value");
    };
    let ExprKind::Call { arguments, .. } = &value.kind else {
        panic!("a call: {:?}", value.kind);
    };
    let given: Vec<Option<usize>> = arguments
        .iter()
        .map(|argument| argument.parameter)
        .collect();
    assert_eq!(given, [Some(1), Some(0)]);
    assert_eq!(
        locals(&checked, "difference")[2..],
        ["negated: T", "whole: T"]
    );
    assert_eq!(
        locals(&checked, "scaled")[2..],
        [
            "one: T",
            "ones: list<T>",
            "pair: list<T>",
            "joined: list<T>"
        ]
    );
    assert_eq!(locals(&checked, "loose")[1..], ["unknown: _"]);
}

/// GEN-7: "choosing a specialization does not authorize unrelated
/// operations". A requirement is the call it names, supplied with the
/// types it names, and no other; one under a disjunction or a negation
/// does not hold for sure (GEN-15). GEN-16: the call is of the type
/// the requirement's arrow states, which is then what is given where
/// another type is expected (EXPR-5).
#[test]
fn a_requirement_is_the_call_it_names_and_no_other() {
    let checked = module(concat!(
        // No requirement.
        "const fn bare<T>(a: T) -> T => add_(a, a)\n",
        // Another operator's.
        "const fn other<T>(a: T) -> T requires sub_(T, T) -> T => add_(a, a)\n",
        // Other types.
        "const fn mixed<T>(a: T, n: i64) -> T requires add_(T, T) -> T => add_(a, n)\n",
        // One of two that may hold.
        "const fn either<T>(a: T) -> T\n",
        "    requires add_(T, T) -> T || sub_(T, T) -> T => add_(a, a)\n",
        // The arrow's type is the call's.
        "const fn wrong<T>(a: T) -> i64 requires add_(T, T) -> T => add_(a, a)\n",
        // A sequence that is not of the type supplied.
        "const fn lists<T>(a: T) -> list<T>\n",
        "    requires add_(list<T>, list<T>) -> list<T> => [a] + [1]\n",
        // A name the operator does not declare is a candidate's option,
        // which no requirement speaks for.
        "const fn option<T>(a: T) -> T\n",
        "    requires div_(T, T) -> T => div_(a, a, zero: .fail)\n",
        // A symbol is its call: `+`, `<` and a minus need theirs stated.
        "const fn plus<T>(a: T, b: T) -> T => a + b\n",
        "const fn less<T>(a: T, b: T) -> bool => a < b\n",
        "const fn minus<T>(a: T) -> T => -a\n",
        // OPS-2: \"`==` and `!=` compare two values of one type\": core's
        // candidate takes any, and so a generic parameter.
        "const fn same<T>(a: T, b: T) -> bool => a == b\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:2:32 operator/operator.no_candidate GEN-5",
            "m.hgl:3:58 operator/operator.no_candidate GEN-5",
            "m.hgl:4:66 operator/operator.no_candidate GEN-5",
            "m.hgl:6:52 operator/operator.no_candidate GEN-5",
            "m.hgl:7:60 type/type.mismatch EXPR-5",
            "m.hgl:9:55 operator/operator.no_candidate OPS-1",
            "m.hgl:11:33 operator/operator.no_candidate GEN-5",
            "m.hgl:12:40 operator/operator.no_candidate OPS-1",
            "m.hgl:13:43 operator/operator.no_candidate OPS-2",
            "m.hgl:14:33 operator/operator.no_candidate OPS-1",
        ]
    );
}

/// GEN-5: "Match candidates against the requested output before
/// inputs". Two requirements may name one operator with the same types
/// and different results (GEN-16: "A result arrow constrains that
/// operator's result type"): the call is the one that gives what its
/// place expects, whichever is written first.
#[test]
fn a_requirement_is_chosen_by_the_result_asked_for() {
    let checked = module(concat!(
        "operator measure<V, O>(value: V) -> O\n",
        "const fn wide<T>(a: T) -> f64\n",
        "    requires measure(T) -> i64 && measure(T) -> f64 => measure(a)\n",
        "const fn narrow<T>(a: T) -> i64\n",
        "    requires measure(T) -> i64 && measure(T) -> f64 => measure(a)\n",
        "const fn either<T>(a: T) requires measure(T) -> i64 && measure(T) -> f64 {\n",
        "    let real: f64 = measure(a)\n",
        // Nothing is asked for: the first stated.
        "    let first = measure(a)\n",
        "}\n",
        // Neither gives a `str`: the first stands, and does not fit.
        "const fn neither<T>(a: T) -> str\n",
        "    requires measure(T) -> i64 && measure(T) -> f64 => measure(a)\n",
    ));
    assert_eq!(checked.summary(), ["m.hgl:12:56 type/type.mismatch EXPR-5"]);
    assert_eq!(locals(&checked, "either")[1..], ["real: f64", "first: i64"]);
}

/// GEN-16: "a named operator requirement tests callability with the
/// supplied types. A result arrow constrains that operator's result
/// type". GEN-5: "Match candidates against the requested output before
/// inputs, then check their conditions." A candidate that requires an
/// operation is admitted for the types that have it.
#[test]
fn a_candidates_requirement_is_tested_with_the_calls_types() {
    let checked = module(concat!(
        "operator total<T>(a: T, b: T) -> T\n",
        "impl const fn total<T>(a: T, b: T) -> T requires add_(T, T) -> T => add_(a, b)\n",
        "operator spread<T>(a: T, b: T) -> T\n",
        "impl const fn spread<T>(a: T, b: T) -> T\n",
        "    requires sub_(T, T) -> T && neg_(T) -> T => sub_(a, neg_(b))\n",
        // The arrow constrains the result: an `i64` less an `i64` is no
        // `duration`, and a `datetime` less a `datetime` is one.
        "operator apart<T>(a: T, b: T) -> duration\n",
        "impl const fn apart<T>(a: T, b: T) -> duration\n",
        "    requires sub_(T, T) -> duration => sub_(a, b)\n",
        "const fn uses(x: f64, n: i64, flag: bool, text: str, at: datetime) {\n",
        "    let real = total(x, x)\n",
        "    let whole = total(n, n)\n",
        "    let words = total(text, text)\n",
        "    let both = spread(n, n)\n",
        "    let gap = apart(at, at)\n",
        // A `bool` is not added, a `str` is not subtracted, and two
        // `i64` are not a `duration` apart.
        "    let bad1 = total(flag, flag)\n",
        "    let bad2 = spread(text, text)\n",
        "    let bad3 = apart(n, n)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:16:16 operator/operator.no_candidate GEN-5",
            "m.hgl:17:16 operator/operator.no_candidate GEN-5",
            "m.hgl:18:16 operator/operator.no_candidate GEN-5",
        ]
    );
    assert_eq!(
        locals(&checked, "uses")[5..10],
        [
            "real: f64",
            "whole: i64",
            "words: str",
            "both: i64",
            "gap: duration",
        ]
    );
}

/// GEN-7: a generic body's call of a candidate that requires an
/// operation of its parameter is admitted where the body's own
/// requirements state that operation, and rejected where they do not.
/// GEN-15: "negation binds before conjunction".
#[test]
fn a_requirement_is_met_by_the_callers_own() {
    let checked = module(concat!(
        "operator total<T>(a: T, b: T) -> T\n",
        "impl const fn total<T>(a: T, b: T) -> T requires add_(T, T) -> T => add_(a, b)\n",
        "const fn stated<T>(a: T) -> T requires add_(T, T) -> T => total(a, a)\n",
        "const fn unstated<T>(a: T) -> T => total(a, a)\n",
        // A requirement nothing states is still met by a candidate that
        // takes any one type: core's `eq_<T>`.
        "operator alike<T>(a: T, b: T) -> bool\n",
        "impl const fn alike<T>(a: T, b: T) -> bool requires eq_(T, T) -> bool => a == b\n",
        "const fn compared<T>(a: T, b: T) -> bool => alike(a, b)\n",
        // Two candidates told apart by a requirement and its negation.
        "operator describe<T>(a: T) -> str\n",
        "impl const fn describe<T>(a: T) -> str requires add_(T, T) -> T => \"added\"\n",
        "impl const fn describe<T>(a: T) -> str requires !add_(T, T) -> T => \"kept\"\n",
        "const fn uses(n: i64, flag: bool) {\n",
        "    let added = describe(n)\n",
        "    let kept = describe(flag)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:5:36 operator/operator.no_candidate GEN-5"]
    );
    assert_eq!(callees(&checked, "stated"), ["candidate"]);
    assert_eq!(callees(&checked, "compared"), ["candidate"]);
    assert_eq!(callees(&checked, "uses"), ["candidate", "candidate"]);
}

/// GEN-7, FN-3: "Any body calls an operator; in a runtime body or a
/// value function the call selects a value candidate." A node's hook
/// and a graph's body are checked under their declaration's
/// requirements as a value function's is. FN-6: in a graph the
/// operation is lifted where it is given a connection.
#[test]
fn a_node_and_a_graph_are_checked_under_their_requirements() {
    let checked = module(concat!(
        "fn node_sum<T>(lhs: T, rhs: T) -> T requires add_(T, T) -> T {\n",
        "    when { return add_(lhs, rhs) }\n",
        "}\n",
        "fn graph_sum<T>(lhs: T, rhs: T) -> T requires add_(T, T) -> T {\n",
        "    let sum = lhs + rhs\n",
        "    return sum\n",
        "}\n",
        "fn graph_bare<T>(lhs: T, rhs: T) -> T => add_(lhs, rhs)\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:9:42 operator/operator.no_candidate GEN-5"]
    );
    assert_eq!(callees(&checked, "node_sum"), ["add_"]);
    assert_eq!(callees(&checked, "graph_sum"), ["add_"]);
    let sum = &body(&checked, "graph_sum").locals[2];
    assert_eq!((sum.name.as_str(), sum.connection), ("sum", true));
}
