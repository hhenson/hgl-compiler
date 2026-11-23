//! The analyser against the rules on named operator requirements: what
//! a declaration requires of its generic parameters is callable in its
//! body, and is tested where a call selects it. Each test cites its
//! rule.
//!
//! A finding is written `file:line:column category/code rule`.

mod common;

use common::{Checked, body, locals, module};
use hgl_analyser::Detail;
use hgl_analyser::hir::{Answer, Body, Callee, Expr, ExprKind, Stmt, ValueCategory};

/// No lines.
const NONE: [&str; 0] = [];

#[test]
fn a_type_inequality_tests_the_resolved_argument() {
    // GEN-3: `T != f64` tests a known type as `!(T == f64)` and binds nothing.
    let declaration = "const fn non_float<T>(value: T) -> T requires T != f64 => value\n";
    let accepted = module(&format!(
        "{declaration}const fn good() -> i64 => non_float(7)\n"
    ));
    assert_eq!(accepted.summary(), NONE);

    let rejected = module(&format!(
        "{declaration}const fn bad() -> f64 => non_float(1.0)\n"
    ));
    assert!(
        rejected
            .findings
            .iter()
            .any(|finding| finding.code == "constraint/requires.unmet"),
        "{}",
        rejected.report
    );
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
                    ..
                } => checked.program.modules[*module].declarations[*declaration]
                    .name
                    .clone(),
                Callee::Candidate { .. } => "candidate".to_string(),
                Callee::Function { .. } => "function".to_string(),
                Callee::Parameter { .. }
                | Callee::Anonymous(_)
                | Callee::DeferredOperator(_)
                | Callee::DeferredPackCall
                | Callee::DeferredValue
                | Callee::Unread => "none".to_string(),
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
        // The parameter may stand in the result alone.
        "operator make<O>() -> O\n",
        "const fn build<T>() -> T requires make() -> T => make()\n",
        // GEN-8: the call may write the operator's generic arguments.
        "const fn built<T>() -> T requires make() -> T => make<T>()\n",
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
    assert_eq!(callees(&checked, "build"), ["make"]);
    assert_eq!(callees(&checked, "built"), ["make"]);
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
        // Generic arguments the requirement does not agree with.
        "operator make<O>() -> O\n",
        "const fn unlike<T, U>() -> T requires make() -> T => make<U>()\n",
        // Generic arguments that agree in one part and not in another:
        // `make2<T>()` asks for a `tuple<T, _>`.
        "operator make2<A, B>() -> tuple<A, B>\n",
        "const fn parts<T, U>() requires make2() -> tuple<U, T> {\n",
        "    let agreed = make2<U>()\n",
        "    let pair = make2<T>()\n",
        "}\n",
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
            "m.hgl:9:54 operator/operator.no_candidate GEN-5",
            "m.hgl:13:16 operator/operator.no_candidate GEN-5",
            "m.hgl:16:55 operator/operator.no_candidate OPS-1",
            "m.hgl:18:33 operator/operator.no_candidate GEN-5",
            "m.hgl:19:40 operator/operator.no_candidate OPS-1",
            "m.hgl:20:43 operator/operator.no_candidate OPS-2",
            "m.hgl:21:33 operator/operator.no_candidate OPS-1",
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

/// GEN-8: an operator's generic value parameter "takes part in the
/// output requested ... and the default where none of them says." A
/// requirement is tested as the call it names would select: with the
/// operator's default in the output requested, and with what the
/// operator requires of its result.
#[test]
fn a_requirement_is_tested_as_its_call_would_select() {
    let checked = module(concat!(
        "operator window<T, const size: i64 = 3>(value: T) -> list<T, size>\n",
        "impl const fn window(value: f64) -> list<f64, 3> => [value, value, value]\n",
        "impl const fn window(value: f64) -> list<f64, 5> =>\n",
        "    [value, value, value, value, value]\n",
        // With no arrow the default says which list: three.
        "operator framed<T>(a: T) -> T\n",
        "impl const fn framed<T>(a: T) -> T requires window(T) => a\n",
        // The arrow says five.
        "operator wide<T>(a: T) -> T\n",
        "impl const fn wide<T>(a: T) -> T requires window(T) -> list<T, 5> => a\n",
        // No candidate gives four.
        "operator odd<T>(a: T) -> T\n",
        "impl const fn odd<T>(a: T) -> T requires window(T) -> list<T, 4> => a\n",
        // TYPE-14: an arrow that is optional still says which list.
        "operator maybe<T>(a: T) -> T\n",
        "impl const fn maybe<T>(a: T) -> T requires window(T) -> list<T, 4>? => a\n",
        // GEN-13: a default the call would take is of its parameter's
        // type as the types resolve it: `2` is no `str`.
        "operator choose<T, const fallback: T = 2>(value: T) -> T\n",
        "impl const fn choose(value: i64) -> i64 => value\n",
        "impl const fn choose(value: str) -> str => value\n",
        "operator chosen<T>(a: T) -> T\n",
        "impl const fn chosen<T>(a: T) -> T requires choose(T) -> T => a\n",
        // GEN-2: an abstract struct widens what an earlier operand
        // decided.
        "abstract struct Shape { id: i64 }\n",
        "struct Round : Shape { radius: f64 }\n",
        "operator same<T, const mode: i64 = 0>(left: T, right: T) -> T\n",
        "impl const fn same(left: Shape, right: Shape) -> Shape => left\n",
        "operator paired<A, B>(a: A, b: B) -> A\n",
        "impl const fn paired<A, B>(a: A, b: B) -> A requires same(A, B) => a\n",
        // The arrow is held to as well as the default: `count` gives an
        // `i64` whatever its `mode`, and so no `str`.
        "operator count<T, const mode: i64 = 0>(value: T) -> i64\n",
        "impl const fn count(value: bool) -> i64 => 1\n",
        "operator texted<T>(a: T) -> T\n",
        "impl const fn texted<T>(a: T) -> T requires count(T) -> str => a\n",
        "operator counted<T>(a: T) -> T\n",
        "impl const fn counted<T>(a: T) -> T requires count(T) -> i64 => a\n",
        "const fn uses(x: f64, text: str, round: Round, shape: Shape) {\n",
        "    let three = framed(x)\n",
        "    let five = wide(x)\n",
        "    let four = odd(x)\n",
        "    let none = maybe(x)\n",
        "    let whole = chosen(1)\n",
        "    let words = chosen(text)\n",
        "    let widened = paired(round, shape)\n",
        "    let no = texted(true)\n",
        "    let yes = counted(true)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:34:16 operator/operator.no_candidate GEN-5",
            "m.hgl:35:16 operator/operator.no_candidate GEN-5",
            "m.hgl:37:17 operator/operator.no_candidate GEN-5",
            "m.hgl:39:14 operator/operator.no_candidate GEN-5",
        ]
    );
    assert_eq!(
        locals(&checked, "uses")[4..],
        [
            "three: f64",
            "five: f64",
            "four: _",
            "none: _",
            "whole: i64",
            "words: _",
            "widened: Round",
            "no: _",
            "yes: bool",
        ]
    );
}

/// GEN-8: an operator's generic value parameter takes "the default
/// where none of them says", and so takes part in what a call requests.
/// GEN-7: a requirement is the call it names as that call asks: one
/// that gives another output than the call requests is not it.
#[test]
fn a_required_call_requests_what_its_defaults_say() {
    let checked = module(concat!(
        "operator window<T, const size: i64 = 3>(value: T) -> list<T, size>\n",
        // The place says five, and so does the requirement.
        "const fn five<T>(value: T) -> list<T, 5>\n",
        "    requires window(T) -> list<T, 5> => window(value)\n",
        // Nothing says the size: the default, three.
        "const fn three<T>(value: T) requires window(T) -> list<T, 3> {\n",
        "    let got = window(value)\n",
        "    let written = window<T, 3>(value)\n",
        "}\n",
        // The requirement is of five, and the call asks for three.
        "const fn other<T>(value: T) requires window(T) -> list<T, 5> {\n",
        "    let got = window(value)\n",
        "    let asked: list<T, 5> = window(value)\n",
        "}\n",
        // With no arrow the requirement is of the bare call, which asks
        // for the default: three, and not five.
        "const fn bare<T>(value: T) requires window(T) {\n",
        "    let got = window(value)\n",
        "    let wide = window<T, 5>(value)\n",
        "}\n",
        // The same holds where a requirement is evidence for a
        // candidate's: `framed` asks for the bare call.
        "operator framed<T>(a: T) -> T\n",
        "impl const fn framed<T>(a: T) -> T requires window(T) => a\n",
        "const fn by_three<T>(value: T) -> T requires window(T) -> list<T, 3> => framed(value)\n",
        "const fn by_bare<T>(value: T) -> T requires window(T) => framed(value)\n",
        "const fn by_five<T>(value: T) -> T requires window(T) -> list<T, 5> => framed(value)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:10:15 operator/operator.no_candidate GEN-5",
            "m.hgl:15:16 operator/operator.no_candidate GEN-5",
            "m.hgl:21:72 operator/operator.no_candidate GEN-5",
        ]
    );
    assert_eq!(
        locals(&checked, "three")[1..],
        ["got: list<T, 3>", "written: list<T, 3>"]
    );
    assert_eq!(
        locals(&checked, "other")[1..],
        ["got: _", "asked: list<T, 5>"]
    );
    assert_eq!(
        locals(&checked, "bare")[1..],
        ["got: list<T, 3>", "wide: _"]
    );
}

/// GEN-16: "a named operator requirement tests callability with the
/// supplied types." Core's OPS-9 has its requirements name value
/// operations, "Addition requires a value `add_(T, T) -> T`", which a
/// node's hook calls (FN-3). So a requirement is tested against the
/// value candidates whatever declares it, and what a graph states meets
/// what a node's candidate asks (GEN-7).
#[test]
fn a_requirement_names_a_value_operation() {
    let checked = module(concat!(
        "operator tick<V>(value: V) -> V\n",
        // A temporal candidate alone for an `i64`; a value one for an
        // `f64`.
        "impl fn tick(value: i64) -> i64 {\n",
        "    when { return value }\n",
        "}\n",
        "impl const fn tick(value: f64) -> f64 => value\n",
        "operator pass<T>(a: T) -> T\n",
        "impl fn pass<T>(a: T) -> T requires tick(T) -> T => tick(a)\n",
        "fn uses(n: i64, x: f64) -> f64 {\n",
        "    let whole = pass(n)\n",
        "    return pass(x)\n",
        "}\n",
        "operator held<T>(a: T) -> T\n",
        "impl fn held<T>(a: T) -> T requires tick(T) -> T {\n",
        "    when { return tick(a) }\n",
        "}\n",
        "fn stated<T>(a: T) -> T requires tick(T) -> T => held(a)\n",
        "fn unstated<T>(a: T) -> T => held(a)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:10:17 operator/operator.no_candidate GEN-5",
            "m.hgl:18:30 operator/operator.no_candidate GEN-5",
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
    assert_eq!(callees(&checked, "stated"), ["none"]);
    assert_eq!(callees(&checked, "compared"), ["none"]);
    assert_eq!(callees(&checked, "uses"), ["candidate", "candidate"]);
}

/// FN-18 and GEN-16: a named requirement supplies the ordinary operands;
/// a brace entry is type-checked in its own configuration slot.
#[test]
fn a_required_operator_accepts_well_typed_configuration() {
    let checked = module(concat!(
        "operator pick<T>{fallback: T? = null}(value: T) -> i64\n",
        "impl const fn pick{fallback: i64?}(value: i64) -> i64 => value\n",
        "impl const fn pick{fallback: bool?}(value: bool) -> i64 => 0\n",
        "const fn configured<T>(value: T) -> i64 requires pick(T) -> i64 =>\n",
        "    pick{fallback: value}(value)\n",
        "const fn bad<T>(value: T) -> i64 requires pick(T) -> i64 =>\n",
        "    pick{fallback: 1}(value)\n",
        "const fn one() -> i64 => configured(1)\n",
        "const fn zero() -> i64 => configured(true)\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:8:20 type/type.mismatch EXPR-5"],
        "{}",
        checked.report
    );
    assert_eq!(callees(&checked, "configured"), ["pick"]);
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
        // FN-3: a connection given to a `const` parameter of the
        // operation required is rejected, as at any call.
        "operator freeze<T>(const value: T) -> T\n",
        "fn frozen<T>(x: T) -> T requires freeze(T) -> T => freeze(x)\n",
        // Given no connection, what a required operation gives may be
        // a connection or a value: a temporal candidate or a value one
        // (GEN-5). It is not settled here, and is not judged where a
        // constant is required.
        "operator source<O>() -> O\n",
        "operator width<V>(of: V) -> i64\n",
        "fn sized(value: f64, const size: i64) -> f64 => value\n",
        "fn sourced<T>(price: f64, const sample: T) -> f64\n",
        "    requires source() -> T && width(T) -> i64 {\n",
        "    let made = source()\n",
        "    let wide = width(sample)\n",
        "    return sized(price, wide)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:9:42 operator/operator.no_candidate GEN-5",
            "m.hgl:11:59 phase/connection.as_constant FN-3",
        ]
    );
    let unsettled: Vec<(&str, ValueCategory)> = body(&checked, "sourced").locals[2..]
        .iter()
        .map(|local| (local.name.as_str(), local.category))
        .collect();
    assert_eq!(
        unsettled,
        [
            ("made", ValueCategory::Unsettled),
            ("wide", ValueCategory::Unsettled),
        ]
    );
    assert_eq!(callees(&checked, "node_sum"), ["add_"]);
    assert_eq!(callees(&checked, "graph_sum"), ["add_"]);
    let sum = &body(&checked, "graph_sum").locals[2];
    assert_eq!(
        (sum.name.as_str(), sum.category),
        ("sum", ValueCategory::Connection)
    );
}

/// An answer as a test reads it: the candidate as `name(types)`, with
/// what answers its own requirements after it in brackets; `stated N`
/// for the enclosing declaration's requirement N; `unread`.
fn answer(checked: &Checked, answer: &Answer) -> String {
    match answer {
        Answer::Candidate {
            module,
            declaration,
            requirements,
            ..
        } => {
            let declared = &checked.program.modules[*module];
            let Some(Detail::Callable(signature)) = declared.details.get(*declaration) else {
                panic!("a candidate has a signature");
            };
            let types: Vec<String> = signature
                .parameters
                .iter()
                .map(|parameter| parameter.ty.to_string())
                .collect();
            let name = &declared.declarations[*declaration].name;
            let own: Vec<String> = requirements
                .iter()
                .map(|own| self::answer(checked, own))
                .collect();
            if own.is_empty() {
                format!("{name}({})", types.join(", "))
            } else {
                format!("{name}({}) [{}]", types.join(", "), own.join(", "))
            }
        }
        Answer::Stated(index) => format!("stated {index}"),
        Answer::Unread => "unread".to_string(),
    }
}

/// What answers the requirements of the callee of each call in `name`,
/// in the order the calls are met; and for a call of what the
/// declaration requires, `requirement N`.
fn answers(checked: &Checked, name: &str) -> Vec<String> {
    answers_in(checked, body(checked, name))
}

fn answers_in(checked: &Checked, body: &Body) -> Vec<String> {
    let mut found = Vec::new();
    hgl_analyser::hir::walk_statements(&body.statements, &mut |value| {
        let ExprKind::Call { callee, .. } = &value.kind else {
            return;
        };
        match callee {
            Callee::Function { requirements, .. } | Callee::Candidate { requirements, .. } => {
                let each: Vec<String> = requirements
                    .iter()
                    .map(|one| answer(checked, one))
                    .collect();
                found.push(each.join(", "));
            }
            Callee::Required { requirement, .. } => {
                found.push(format!("requirement {requirement}"))
            }
            Callee::Parameter { .. }
            | Callee::Anonymous(_)
            | Callee::DeferredOperator(_)
            | Callee::DeferredPackCall
            | Callee::DeferredValue
            | Callee::Unread => found.push("none".to_string()),
        }
    });
    found
}

/// GEN-16: "a named operator requirement tests callability with the
/// supplied types ... it is tested against the operator's value
/// candidates, and holds where one most-specific value candidate takes
/// the types supplied (GEN-5)". A call of a declaration records that
/// candidate for each requirement the declaration states, so that what
/// its body requires is known for that call. GEN-15: requirements
/// joined by `&&` all hold, and each has its answer, in the order
/// written.
#[test]
fn a_call_records_what_answers_each_requirement() {
    let checked = module(concat!(
        "operator join<L, R, O>(lhs: L, rhs: R) -> O\n",
        "impl const fn join(lhs: i64, rhs: i64) -> i64 => lhs\n",
        "impl const fn join(lhs: str, rhs: str) -> str => lhs\n",
        "const fn twice<T>(value: T) -> T requires join(T, T) -> T => join(value, value)\n",
        "const fn whole() -> i64 => twice(1)\n",
        "const fn text() -> str => twice(\"a\")\n",
        // Two requirements, and the body's calls of each.
        "const fn both<T>(value: T) -> T requires neg_(T) -> T && (join(T, T) -> T) =>\n",
        "    join(neg_(value), value)\n",
        "const fn uses() -> i64 => both(1)\n",
        // A function that states none has none answered.
        "const fn plain(value: i64) -> i64 => value\n",
        "const fn calls() -> i64 => plain(1)\n",
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(answers(&checked, "twice"), ["requirement 0"]);
    assert_eq!(answers(&checked, "whole"), ["join(i64, i64)"]);
    assert_eq!(answers(&checked, "text"), ["join(str, str)"]);
    // The call of `join` is met before its argument's call of `neg_`.
    assert_eq!(
        answers(&checked, "both"),
        ["requirement 1", "requirement 0"]
    );
    assert_eq!(answers(&checked, "uses"), ["neg_(i64), join(i64, i64)"]);
    assert_eq!(answers(&checked, "calls"), [""]);
}

/// GEN-7: "A generic body is checked under its stated requirements".
/// Where a call's types name a generic parameter of the declaration it
/// is written in, what that declaration states is the answer: which
/// candidate that is, is known where the declaration is called in turn.
/// A candidate that takes any one type answers for a parameter with
/// nothing stated.
#[test]
fn a_call_in_a_generic_body_is_answered_by_what_the_body_states() {
    let checked = module(concat!(
        "operator join<L, R, O>(lhs: L, rhs: R) -> O\n",
        "impl const fn join(lhs: i64, rhs: i64) -> i64 => lhs\n",
        "const fn twice<T>(value: T) -> T requires join(T, T) -> T => join(value, value)\n",
        "const fn outer<U>(value: U) -> U requires neg_(U) -> U && join(U, U) -> U =>\n",
        "    twice(value)\n",
        "const fn top() -> i64 => outer(2)\n",
        // Nothing stated, and a candidate for any one type.
        "operator show<T>(value: T) -> str\n",
        "impl const fn show<T>(value: T) -> str => \"\"\n",
        "const fn shown<T>(value: T) -> str requires show(T) -> str => show(value)\n",
        "const fn whatever<U>(value: U) -> str => shown(value)\n",
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(answers(&checked, "outer"), ["stated 1"]);
    assert_eq!(answers(&checked, "top"), ["neg_(i64), join(i64, i64)"]);
    assert_eq!(answers(&checked, "whatever"), ["show(T)"]);
}

/// GEN-16: the candidate that answers may state a requirement of its
/// own, which is answered with it: a `join` of two lists is the
/// candidate for lists, with the `join` of their elements.
#[test]
fn a_candidate_that_answers_has_its_own_requirements_answered() {
    let checked = module(concat!(
        "operator join<L, R, O>(lhs: L, rhs: R) -> O\n",
        "impl const fn join(lhs: i64, rhs: i64) -> i64 => lhs\n",
        "impl const fn join<T>(lhs: list<T>, rhs: list<T>) -> list<T>\n",
        "    requires join(T, T) -> T => lhs\n",
        "const fn twice<T>(value: T) -> T requires join(T, T) -> T => join(value, value)\n",
        "const fn lists() -> list<i64> => twice([1])\n",
        "const fn nested() -> list<list<i64>> => twice([[1]])\n",
        // A call of the operator itself selects the candidate, and
        // records what answers that candidate's requirement.
        "const fn direct(a: list<i64>) -> list<i64> => join(a, a)\n",
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(
        answers(&checked, "lists"),
        ["join(list<T>, list<T>) [join(i64, i64)]"]
    );
    assert_eq!(
        answers(&checked, "nested"),
        ["join(list<T>, list<T>) [join(list<T>, list<T>) [join(i64, i64)]]"]
    );
    assert_eq!(answers(&checked, "direct"), ["join(i64, i64)"]);
}

/// GEN-16: "A requirement names a value operation, whatever declares
/// it ... So what a graph requires is what a node or a value function
/// it calls may require." Core's OPS-9: the temporal candidate of `+`
/// under `{invalid: .identity}` requires a value `add_(T, T) -> T`. A
/// graph that states the same answers it, and a call of that graph
/// selects the value candidate.
#[test]
fn a_graph_answers_a_temporal_candidates_requirement() {
    let checked = module(concat!(
        "fn identity_sum<T>(lhs: T, rhs: T) -> T\n",
        "    requires add_(T, T) -> T => lhs +{invalid: .identity} rhs\n",
        "fn wired(a: f64, b: f64) -> f64 => identity_sum(a, b)\n",
        "fn texts(a: str, b: str) -> str => identity_sum(a, b)\n",
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(answers(&checked, "identity_sum"), ["none"]);
    for (caller, expected) in [("wired", "add_(f64, f64)"), ("texts", "add_(str, str)")] {
        let Some(Stmt::Return {
            value: Some(value), ..
        }) = body(&checked, caller).statements.last()
        else {
            panic!("a return with a call");
        };
        let ExprKind::Call {
            callee:
                Callee::Function {
                    module,
                    declaration,
                    generics,
                    requirements,
                    ..
                },
            ..
        } = &value.kind
        else {
            panic!("a call of identity_sum");
        };
        assert_eq!(answers(&checked, caller), [expected]);
        let (_, instance) = checked
            .program
            .instance(*module, *declaration, generics, requirements)
            .expect("identity_sum has a body");
        assert_eq!(answers_in(&checked, &instance), [expected]);
    }
}

/// GEN-3: "A call of a function whose `requires` does not hold with
/// the generic arguments the call resolves is rejected". The condition
/// is a membership of a type, a comparison of a generic value
/// parameter, given or left to its default, or a named operator
/// requirement (GEN-16), and it is reported at the call. Nothing
/// answers the requirement of such a call.
#[test]
fn a_call_that_does_not_meet_its_callees_requires_is_rejected() {
    let checked = module(concat!(
        "enum Mode { fast, slow }\n",
        "const fn twice<T>(value: T) -> T requires add_(T, T) -> T => value + value\n",
        "const fn numeric<T>(value: T) -> T requires T in {i64, f64} => value\n",
        "const fn exact<T>(value: T) -> T requires T == i64 => value\n",
        "const fn pick<const mode: Mode = .fast>(value: i64) -> i64\n",
        "    requires mode == .fast => value\n",
        "const fn eager<const mode: Mode = .slow>(value: i64) -> i64\n",
        "    requires mode == .fast => value\n",
        // Each holds.
        "const fn fine() -> i64 =>\n",
        "    twice(2) + numeric(3) + exact(4) + pick(5) + pick(6, mode: .fast) + eager(7, mode: .fast)\n",
        "const fn real() -> f64 => numeric(1.5)\n",
        // No value `add_` takes two `bool`.
        "const fn neither() -> bool => twice(true)\n",
        "const fn text() -> str => numeric(\"x\")\n",
        "const fn inexact() -> f64 => exact(1.5)\n",
        "const fn slow() -> i64 => pick(1, mode: .slow)\n",
        // The default is the value where the call gives none.
        "const fn defaulted() -> i64 => eager(1)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:13:31 constraint/requires.unmet GEN-3",
            "m.hgl:14:27 constraint/requires.unmet GEN-3",
            "m.hgl:15:30 constraint/requires.unmet GEN-3",
            "m.hgl:16:27 constraint/requires.unmet GEN-3",
            "m.hgl:17:32 constraint/requires.unmet GEN-3",
        ]
    );
    assert_eq!(answers(&checked, "neither"), ["unread"]);
}

/// GEN-3 and GEN-10: `each` checks every captured type, including a
/// named pack's types, and holds over an empty capture. A failing type
/// settles the verdict even when another test is still untold.
#[test]
fn a_call_checks_each_type_its_pack_requires() {
    let checked = module(concat!(
        "const fn all_i64<...Ts>(items: ...Ts) -> i64 requires each T in types(Ts) { T == i64 } => 0\n",
        "const fn named_i64<...Ts>(items: ...{Ts}) -> i64 requires each T in types(Ts) { T == i64 } => 0\n",
        "operator allowed<T>(x: T) -> T\n",
        "impl const fn allowed(x: i64) -> i64 => x\n",
        "const fn needs_judgment<...Ts>(items: ...Ts) -> i64\n",
        "    requires each T in types(Ts) { T == i64 && allowed(T) -> T } => 0\n",
        "const fn good() -> i64 => all_i64(1, 2)\n",
        "const fn empty() -> i64 => all_i64()\n",
        "const fn good_named() -> i64 => named_i64(an: 1, other: 2)\n",
        "const fn mixed() -> i64 => all_i64(1, true)\n",
        "const fn mixed_named() -> i64 => named_i64(an: 1, other: true)\n",
        "const fn untold() -> i64 => needs_judgment(1)\n",
        "const fn false_over_untold() -> i64 => needs_judgment(1, \"no\")\n",
        "const fn empty_opaque() -> i64 => needs_judgment()\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:11:28 constraint/requires.unmet GEN-3",
            "m.hgl:12:34 constraint/requires.unmet GEN-3",
            "m.hgl:13:29 constraint/requires.unmet GEN-3",
            "m.hgl:14:40 constraint/requires.unmet GEN-3",
        ],
        "{}",
        checked.report
    );
    assert!(
        checked
            .report
            .contains("the `requires` of `needs_judgment` is not known to hold for this call")
    );
    assert!(
        checked
            .report
            .contains("the `requires` of `needs_judgment` does not hold for this call")
    );
}

/// GEN-7 and GEN-10: a generic caller's stated memberships prove a
/// requirement on a fixed argument or on a member of the captured pack.
#[test]
fn a_generic_body_proves_requirements_on_calls_with_packs() {
    let checked = module(concat!(
        "const fn numeric<T, ...Ts>(value: T, rest: ...Ts) -> i64\n",
        "    requires T in {i64, f64} && each R in types(Ts) { R in {i64, f64} } => 1\n",
        "const fn fixed<U>(value: U) -> i64 requires U in {i64, f64} => numeric(value, 1)\n",
        "const fn packed<U>(value: U) -> i64 requires U in {i64, f64} => numeric(1, value)\n",
        "const fn invalid<U>(value: U) -> i64 requires U in {i64, str} => numeric(1, value)\n",
    ));
    let reports = checked.summary();
    assert_eq!(reports.len(), 1, "{}", checked.report);
    assert!(reports[0].starts_with("m.hgl:6:"), "{}", checked.report);
    assert!(
        reports[0].contains("constraint/requires.unmet GEN-7"),
        "{}",
        checked.report
    );
    assert!(checked.report.contains("`U` = `str`"), "{}", checked.report);
}

/// GEN-3: "and so is an application of a struct whose `requires` does
/// not hold with its arguments": where the struct is constructed, its
/// arguments written or found from its fields (GEN-2), and where a type
/// that applies it is written, at any depth.
#[test]
fn an_application_that_does_not_meet_a_structs_requires_is_rejected() {
    let checked = module(concat!(
        "struct Sized<T> requires T in {i64, f64} { value: T }\n",
        "struct Summed<T> requires add_(T, T) -> T { value: T }\n",
        "const fn made() -> Sized<i64> => Sized(value: 1)\n",
        "const fn written() -> Sized<f64> => Sized<f64>(value: 1.5)\n",
        "const fn summed() -> Summed<i64> => Summed(value: 1)\n",
        "const fn found() -> bool {\n",
        "    let held = Sized(value: \"x\")\n",
        "    return true\n",
        "}\n",
        "const fn given() -> bool {\n",
        "    let held = Sized<str>(value: \"x\")\n",
        "    return true\n",
        "}\n",
        "const fn taken(held: Sized<str>) -> bool => true\n",
        "const fn nested(held: list<Sized<bool>>) -> bool => true\n",
        // A named operator requirement is told where the struct is
        // constructed.
        "const fn unsummed() -> bool {\n",
        "    let held = Summed(value: true)\n",
        "    return true\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:8:16 constraint/requires.unmet GEN-3",
            "m.hgl:12:16 constraint/requires.unmet GEN-3",
            "m.hgl:15:22 constraint/requires.unmet GEN-3",
            "m.hgl:16:23 constraint/requires.unmet GEN-3",
            "m.hgl:18:16 constraint/requires.unmet GEN-3",
        ]
    );
}

/// GEN-7: "A generic body is checked under its stated requirements". A
/// call there whose types name the body's own generic parameter meets a
/// named operator requirement where the body states it, and does not
/// where it states none. A membership the body does not state is not
/// known to hold either, and the call rejects where the body is declared
/// (GEN-7: "It is judged once, where it is declared, by what its
/// `requires` states"; hhenson/hgl_spec#144).
#[test]
fn a_call_in_a_generic_body_meets_what_the_body_states() {
    let checked = module(concat!(
        "const fn twice<T>(value: T) -> T requires add_(T, T) -> T => value + value\n",
        "const fn numeric<T>(value: T) -> T requires T in {i64, f64} => value\n",
        "const fn stated<U>(value: U) -> U requires add_(U, U) -> U => twice(value)\n",
        "const fn unstated<U>(value: U) -> U => twice(value)\n",
        "const fn member<U>(value: U) -> U => numeric(value)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:5:40 constraint/requires.unmet GEN-3",
            "m.hgl:6:38 constraint/requires.unmet GEN-7",
        ]
    );
}

/// GEN-8: "A call may supply its generic arguments as a generic
/// constructor does ... The value a call gives a generic value
/// parameter is a compile-time constant: a literal, an enum member
/// however it is written (TYPE-7), `Zero.nan` or `.nan`, a constant
/// (UNIT-13) ... or a constant expression of those". GEN-3: a
/// `requires` is held to the value however it is written and wherever
/// it is given: after the callee's name or by name, in a call, a
/// construction and a type.
#[test]
fn a_requires_is_held_to_a_generic_value_however_it_is_written() {
    let checked = module(concat!(
        "enum Mode { fast, slow }\n",
        "const SLOW: Mode = .slow\n",
        "const OFF: bool = false\n",
        "const fn pick<const enabled: bool>(value: i64) -> i64 requires enabled == true => value\n",
        "const fn moded<const mode: Mode = .fast>(value: i64) -> i64\n",
        "    requires mode == .fast => value\n",
        "const fn sized<const size: i64>(value: i64) -> i64 requires size in {2, 3} => value\n",
        "const fn named<const label: str>(value: i64) -> i64 requires label == \"a\" => value\n",
        "const fn real<const scale: f64>(value: i64) -> i64 requires scale == 1.5 => value\n",
        "struct Flag<const on: bool> requires on == true { value: i64 }\n",
        "struct Moded<const mode: Mode> requires mode == .fast { value: i64 }\n",
        // Line 13: each of these does not hold.
        "const fn a() -> i64 => pick<false>(1)\n",
        "const fn b() -> i64 => pick(1, enabled: false)\n",
        "const fn c() -> i64 => moded<.slow>(1)\n",
        "const fn d() -> i64 => moded<Mode.slow>(1)\n",
        "const fn e() -> i64 => moded<SLOW>(1)\n",
        "const fn f() -> i64 => moded(1, mode: SLOW)\n",
        "const fn g() -> i64 => sized<4>(1)\n",
        "const fn h() -> i64 => sized<1 + 3>(1)\n",
        "const fn i() -> i64 => named<\"b\">(1)\n",
        "const fn j() -> i64 => real<2.5>(1)\n",
        "const fn k() -> i64 => real(1, scale: 2.5)\n",
        "const fn l() -> i64 => pick<OFF>(1)\n",
        "const fn m(held: Flag<false>) -> i64 => 1\n",
        "const fn n(held: Moded<.slow>) -> i64 => 1\n",
        "const fn o(held: Moded<Mode.slow>) -> i64 => 1\n",
        "const fn p(held: list<Flag<false>>) -> i64 => 1\n",
        "const fn q(held: Moded<SLOW>) -> i64 => 1\n",
        "const fn r() -> i64 {\n",
        "    let held = Flag<false>(value: 1)\n",
        "    return 1\n",
        "}\n",
        "const fn s() -> i64 {\n",
        "    let held = Moded<SLOW>(value: 1)\n",
        "    return 1\n",
        "}\n",
        // Each of these holds.
        "const fn fine(x: Flag<true>, y: Moded<.fast>, z: Moded<Mode.fast>) -> i64 =>\n",
        "    pick<true>(1) + moded<.fast>(1) + moded<Mode.fast>(1) + moded(1) + sized<2>(1)\n",
        "    + named<\"a\">(1) + real<1.5>(1) + Flag<true>(value: 1).value\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:13:24 constraint/requires.unmet GEN-3",
            "m.hgl:14:24 constraint/requires.unmet GEN-3",
            "m.hgl:15:24 constraint/requires.unmet GEN-3",
            "m.hgl:16:24 constraint/requires.unmet GEN-3",
            "m.hgl:17:24 constraint/requires.unmet GEN-3",
            "m.hgl:18:24 constraint/requires.unmet GEN-3",
            "m.hgl:19:24 constraint/requires.unmet GEN-3",
            "m.hgl:20:24 constraint/requires.unmet GEN-3",
            "m.hgl:21:24 constraint/requires.unmet GEN-3",
            "m.hgl:22:24 constraint/requires.unmet GEN-3",
            "m.hgl:23:24 constraint/requires.unmet GEN-3",
            "m.hgl:24:24 constraint/requires.unmet GEN-3",
            "m.hgl:25:18 constraint/requires.unmet GEN-3",
            "m.hgl:26:18 constraint/requires.unmet GEN-3",
            "m.hgl:27:18 constraint/requires.unmet GEN-3",
            "m.hgl:28:18 constraint/requires.unmet GEN-3",
            "m.hgl:29:18 constraint/requires.unmet GEN-3",
            "m.hgl:31:16 constraint/requires.unmet GEN-3",
            "m.hgl:35:16 constraint/requires.unmet GEN-3",
        ]
    );
}

/// GEN-3: a function, and a struct, with no generic parameter may
/// state a `requires` of types it names itself, and a call, a
/// construction and a written type are held to it.
#[test]
fn a_requires_with_no_generic_parameter_is_held_too() {
    let checked = module(concat!(
        "const fn blocked(value: i64) -> i64 requires add_(bool, bool) -> bool => value\n",
        "const fn open(value: i64) -> i64 requires add_(i64, i64) -> i64 => value + value\n",
        "fn node_blocked(value: i64) -> i64 requires add_(bool, bool) -> bool {\n",
        "    when { return value }\n",
        "}\n",
        "struct Shut requires add_(bool, bool) -> bool { value: i64 }\n",
        "struct Ajar requires add_(i64, i64) -> i64 { value: i64 }\n",
        // Line 9.
        "const fn a() -> i64 => blocked(1)\n",
        "const fn b() -> i64 => open(1)\n",
        "fn c(value: i64) -> i64 => node_blocked(value)\n",
        "const fn d() -> i64 => Shut(value: 1).value\n",
        "const fn e() -> i64 => Ajar(value: 1).value\n",
        "const fn f(held: Shut, other: Ajar) -> i64 => 1\n",
        "test g {\n",
        "    let seen = eval node_blocked(value: [1])\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:9:24 constraint/requires.unmet GEN-3",
            "m.hgl:11:28 constraint/requires.unmet GEN-3",
            "m.hgl:12:24 constraint/requires.unmet GEN-3",
            "m.hgl:14:18 constraint/requires.unmet GEN-3",
            "m.hgl:16:21 constraint/requires.unmet GEN-3",
        ]
    );
}

/// GEN-3, GEN-16: a struct's named operator requirement is told
/// wherever a type that applies it is written: in a function's
/// signature, a struct's field, an operator's and a native function's
/// parameters. GEN-7: where its argument is a generic parameter of the
/// declaration that writes it, it holds where that declaration states
/// it.
#[test]
fn a_structs_operator_requirement_is_told_where_its_type_is_written() {
    let checked = module(concat!(
        "struct Summed<T> requires add_(T, T) -> T { value: T }\n",
        // Each of these holds.
        "const fn whole(held: Summed<i64>) -> i64 => 1\n",
        "const fn stated<U>(held: Summed<U>) -> i64 requires add_(U, U) -> U => 1\n",
        "struct Stated<U> requires add_(U, U) -> U { held: Summed<U> }\n",
        // Line 6: each of these does not.
        "const fn flags(held: Summed<bool>) -> i64 => 1\n",
        "const fn unstated<U>(held: Summed<U>) -> i64 => 1\n",
        "struct Holder { held: Summed<bool> }\n",
        "struct Unstated<U> { held: Summed<U> }\n",
        "operator through(held: Summed<bool>) -> bool\n",
        "native const fn outside(held: Summed<bool>) -> bool\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:6:22 constraint/requires.unmet GEN-3",
            "m.hgl:7:28 constraint/requires.unmet GEN-3",
            "m.hgl:8:23 constraint/requires.unmet GEN-3",
            "m.hgl:9:28 constraint/requires.unmet GEN-3",
            "m.hgl:10:24 constraint/requires.unmet GEN-3",
            "m.hgl:11:31 constraint/requires.unmet GEN-3",
        ]
    );
}

/// GEN-2: "a generic value parameter that nothing says anything of
/// takes its default". GEN-3: a `requires` is held to a default too,
/// where the default names a constant (UNIT-13) or is an expression of
/// constants: for a call, a construction and a written type, each of
/// which leaves the parameter out.
#[test]
fn a_requires_is_held_to_a_default_that_names_a_constant() {
    let checked = module(concat!(
        "enum Mode { fast, slow }\n",
        "const SLOW: Mode = .slow\n",
        "const OFF: bool = false\n",
        "const ON: bool = true\n",
        "const TWO: i64 = 2\n",
        "const fn pick<const enabled: bool = OFF>(value: i64) -> i64\n",
        "    requires enabled == true => value\n",
        "const fn keep<const enabled: bool = ON>(value: i64) -> i64\n",
        "    requires enabled == true => value\n",
        "const fn moded<const mode: Mode = SLOW>(value: i64) -> i64\n",
        "    requires mode == .fast => value\n",
        "const fn plus<const size: i64 = TWO + 1>(value: i64) -> i64 requires size == 2 => value\n",
        "struct Flag<const on: bool = OFF> requires on == true { value: i64 }\n",
        "struct Lit<const on: bool = ON> requires on == true { value: i64 }\n",
        "struct Wide<const size: i64 = TWO + 1> requires size == 2 { value: i64 }\n",
        "struct Exact<const size: i64> requires size == 2 { value: i64 }\n",
        // Line 18: each of these does not hold.
        "const fn a() -> i64 => pick(1)\n",
        "const fn b() -> i64 => moded(1)\n",
        "const fn c() -> i64 => plus(1)\n",
        "const fn d() -> i64 => plus<TWO + 2>(1)\n",
        "const fn e() -> i64 => plus(1, size: TWO * 2)\n",
        "const fn f() -> i64 => Flag(value: 1).value\n",
        "const fn g() -> i64 => Wide(value: 1).value\n",
        "const fn h() -> i64 => Exact<TWO + 1>(value: 1).value\n",
        "const fn i(held: Flag) -> i64 => 1\n",
        "const fn j(held: Wide) -> i64 => 1\n",
        "const fn k(held: Exact<TWO + 1>) -> i64 => 1\n",
        "struct Holder { held: Flag }\n",
        // Each of these holds.
        "const fn fine(held: Lit, exact: Exact<TWO>, sum: Exact<1 + 1>) -> i64 =>\n",
        "    keep(1) + pick(1, enabled: true) + pick<ON>(1) + plus<TWO>(1) + plus(1, size: TWO)\n",
        "    + Lit(value: 1).value + Flag<ON>(value: 1).value\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:18:24 constraint/requires.unmet GEN-3",
            "m.hgl:19:24 constraint/requires.unmet GEN-3",
            "m.hgl:20:24 constraint/requires.unmet GEN-3",
            "m.hgl:21:24 constraint/requires.unmet GEN-3",
            "m.hgl:22:24 constraint/requires.unmet GEN-3",
            "m.hgl:23:24 constraint/requires.unmet GEN-3",
            "m.hgl:24:24 constraint/requires.unmet GEN-3",
            "m.hgl:25:24 constraint/requires.unmet GEN-3",
            "m.hgl:26:18 constraint/requires.unmet GEN-3",
            "m.hgl:27:18 constraint/requires.unmet GEN-3",
            "m.hgl:28:18 constraint/requires.unmet GEN-3",
            "m.hgl:29:23 constraint/requires.unmet GEN-3",
        ]
    );
}

/// GEN-2: "A struct application supplies all or none". A type that
/// writes a struct's generic value is held to what it writes, however
/// that is spelled, and never to the parameter's default; one that
/// writes none is held to the default. Each application in a type is
/// read for itself.
#[test]
fn a_written_argument_of_a_type_is_not_taken_for_the_default() {
    let checked = module(concat!(
        "enum Mode { fast, slow }\n",
        "const SLOW: Mode = .slow\n",
        "const FAST: Mode = .fast\n",
        "const TWO: i64 = 2\n",
        "struct Pick<const mode: Mode = SLOW> requires mode == .fast { value: i64 }\n",
        "struct Wide<const size: i64 = TWO + 1> requires size == 2 { value: i64 }\n",
        // Each of these holds: what is written is what counts.
        "const fn a(held: Pick<Mode.fast>) -> i64 => 1\n",
        "const fn b(held: Pick<.fast>) -> i64 => 1\n",
        "const fn c(held: Pick<FAST>) -> i64 => 1\n",
        "const fn d(held: Wide<TWO>) -> i64 => 1\n",
        "const fn e(held: tuple<Pick<FAST>, Wide<2>>) -> i64 => 1\n",
        // Line 13: each of these does not.
        "const fn f(held: Pick) -> i64 => 1\n",
        "const fn g(held: Pick<Mode.slow>) -> i64 => 1\n",
        "const fn h(held: Pick<SLOW>) -> i64 => 1\n",
        "const fn i(held: Wide) -> i64 => 1\n",
        "const fn j(held: Wide<TWO + 1>) -> i64 => 1\n",
        "const fn k(held: tuple<Pick<FAST>, Pick>) -> i64 => 1\n",
        "const fn l(held: tuple<Pick, Pick<FAST>>) -> i64 => 1\n",
        // TYPE-7: an enum's member "however it is written", its
        // constructor among the ways.
        "const fn m(held: Pick<Mode(\"slow\")>) -> i64 => 1\n",
        "const fn n(held: Pick<Mode(1)>) -> i64 => 1\n",
        "const fn o(held: Pick<(Mode.slow)>) -> i64 => 1\n",
        "const fn p(held: list<Pick<Mode(\"slow\")>>) -> i64 => 1\n",
        "const fn q() -> i64 => Pick<Mode(\"slow\")>(value: 1).value\n",
        "const fn fine(held: Pick<Mode(\"fast\")>, other: Pick<Mode(0)>) -> i64 =>\n",
        "    Pick<Mode(\"fast\")>(value: 1).value\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:13:18 constraint/requires.unmet GEN-3",
            "m.hgl:14:18 constraint/requires.unmet GEN-3",
            "m.hgl:15:18 constraint/requires.unmet GEN-3",
            "m.hgl:16:18 constraint/requires.unmet GEN-3",
            "m.hgl:17:18 constraint/requires.unmet GEN-3",
            "m.hgl:18:18 constraint/requires.unmet GEN-3",
            "m.hgl:19:18 constraint/requires.unmet GEN-3",
            "m.hgl:20:18 constraint/requires.unmet GEN-3",
            "m.hgl:21:18 constraint/requires.unmet GEN-3",
            "m.hgl:22:18 constraint/requires.unmet GEN-3",
            "m.hgl:23:18 constraint/requires.unmet GEN-3",
            "m.hgl:24:24 constraint/requires.unmet GEN-3",
        ]
    );
}

/// UNIT-13: a constant "names one ordinary value". TYPE-7: an enum's
/// member is one "however it is written". A constant of an enum is the
/// member its expression names, read where the constant is declared:
/// `Mode.slow`, `Mode("slow")`, `Mode(1)`, or another constant, which
/// is followed. GEN-3: a `requires` is held to it wherever the constant
/// is given.
#[test]
fn a_requires_is_held_to_a_constant_however_its_value_is_written() {
    let checked = module(concat!(
        "enum Mode { fast, slow }\n",
        "const A: Mode = Mode.slow\n",
        "const B: Mode = Mode(\"slow\")\n",
        "const C: Mode = Mode(1)\n",
        "const D: Mode = A\n",
        "const E: Mode = (Mode.slow)\n",
        "const OFF: bool = false\n",
        "const STILL_OFF: bool = OFF\n",
        "const F1: Mode = Mode.fast\n",
        "const F2: Mode = Mode(\"fast\")\n",
        "const F3: Mode = F1\n",
        "const fn pick<const mode: Mode = .fast>(value: i64) -> i64\n",
        "    requires mode == .fast => value\n",
        "const fn flag<const on: bool>(value: i64) -> i64 requires on == true => value\n",
        "struct Pick<const mode: Mode = A> requires mode == .fast { value: i64 }\n",
        // Line 17: each of these does not hold.
        "const fn a() -> i64 => pick<A>(1)\n",
        "const fn b() -> i64 => pick<B>(1)\n",
        "const fn c() -> i64 => pick<C>(1)\n",
        "const fn d() -> i64 => pick<D>(1)\n",
        "const fn e() -> i64 => pick(1, mode: E)\n",
        "const fn f() -> i64 => flag<STILL_OFF>(1)\n",
        "const fn g(held: Pick<B>) -> i64 => 1\n",
        "const fn h(held: Pick) -> i64 => 1\n",
        // Each of these holds.
        "const fn fine(held: Pick<F2>) -> i64 => pick<F1>(1) + pick<F2>(1) + pick(1, mode: F3)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:17:24 constraint/requires.unmet GEN-3",
            "m.hgl:18:24 constraint/requires.unmet GEN-3",
            "m.hgl:19:24 constraint/requires.unmet GEN-3",
            "m.hgl:20:24 constraint/requires.unmet GEN-3",
            "m.hgl:21:24 constraint/requires.unmet GEN-3",
            "m.hgl:22:24 constraint/requires.unmet GEN-3",
            "m.hgl:23:18 constraint/requires.unmet GEN-3",
            "m.hgl:24:18 constraint/requires.unmet GEN-3",
        ]
    );
}
