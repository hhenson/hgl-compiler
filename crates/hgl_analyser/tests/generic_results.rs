//! Generic arguments of store reads and the form selected by their
//! use (GEN-2, GEN-8, ERR-15, CAP-6).
//!
//! A finding is written `file:line:column category/code rule`.

mod common;

use common::{Checked, body, locals, module};
use hgl_analyser::Argument;
use hgl_analyser::hir::{Body, Callee, Expr, ExprKind, walk_statements};

/// No findings.
const NONE: [&str; 0] = [];

/// Each call in `body`, in the order the calls are met: the function or
/// the candidate called, by its name, with its generic arguments in
/// angle brackets and the type of the call after a colon. A call with no
/// callee selected is `none`.
fn called(checked: &Checked, body: &Body) -> Vec<String> {
    let mut found = Vec::new();
    walk_statements(&body.statements, &mut |value: &Expr| {
        let ExprKind::Call { callee, .. } = &value.kind else {
            return;
        };
        let (module, declaration, generics) = match callee {
            Callee::Function {
                module,
                declaration,
                generics,
                ..
            }
            | Callee::Candidate {
                module,
                declaration,
                generics,
                ..
            } => (*module, *declaration, generics),
            Callee::Required { .. }
            | Callee::Parameter { .. }
            | Callee::Anonymous(_)
            | Callee::DeferredOperator(_)
            | Callee::DeferredValue
            | Callee::Unread => {
                found.push("none".to_string());
                return;
            }
        };
        let given: Vec<String> = generics
            .iter()
            .map(|argument| match argument {
                Argument::Type(ty) => ty.to_string(),
                Argument::Constant(constant) => constant.text.clone(),
            })
            .collect();
        found.push(format!(
            "{}<{}>: {}",
            checked.program.modules[module].declarations[declaration].name,
            given.join(", "),
            value.ty
        ));
    });
    found
}

/// CAP-6 and ERR-15: store reads give a result. For an explicit nullable
/// V (GEN-8), the call still gives `result<V, error>`, and a place of
/// type V takes the success's value. A later read of that nullable
/// value as a plain value checks presence (ERR-12).
#[test]
fn a_store_read_with_a_nullable_type_argument_gives_a_result() {
    let checked = module(concat!(
        "fn hooked(value: i64?) -> i64 {\n",
        "    inject global_state\n",
        "    when {\n",
        "        put(global_state, \"k\", value)\n",
        "        let other = get<i64?>(global_state, \"k\")\n",
        "        let whole = get<list<i64>?>(global_state, \"whole\")\n",
        "        let listed = get<list<i64?>>(global_state, \"listed\")\n",
        "        let paired = get<tuple<i64?, str>>(global_state, \"paired\")\n",
        "        let maybe: i64? = other\n",
        "        let plain: i64 = maybe\n",
        "        return plain\n",
        "    }\n",
        "}\n",
        "const fn valued(store: GlobalState) -> i64 {\n",
        "    let other: i64? = get<i64?>(store, \"k\")\n",
        "    if other == null { return 0 }\n",
        "    return other\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(
        locals(&checked, "hooked")[2..6],
        [
            "other: result<i64?, error>",
            "whole: result<list<i64>?, error>",
            "listed: result<list<i64?>, error>",
            "paired: result<tuple<i64?, str>, error>",
        ]
    );
    assert_eq!(
        called(&checked, body(&checked, "hooked")),
        [
            "put<i64?>: _",
            "get<i64?>: result<i64?, error>",
            "get<list<i64>?>: result<list<i64>?, error>",
            "get<list<i64?>>: result<list<i64?>, error>",
            "get<tuple<i64?, str>>: result<tuple<i64?, str>, error>",
        ]
    );
    assert_eq!(locals(&checked, "valued")[1], "other: i64?");
    assert_eq!(
        called(&checked, body(&checked, "valued"))[0],
        "get<i64?>: result<i64?, error>"
    );
}

/// GEN-8: "the generic arguments the call writes are the operator's"
/// (`convert<f64>(count)` "supplies T"), and GEN-2: "What the call
/// writes ... is not widened". So a program's operator whose generic
/// candidate's result is its type parameter is given the nullable type
/// the call writes, as core's `get` is (CAP-6), and so is a `const fn`.
/// ERR-12 as above.
#[test]
fn a_nullable_type_written_for_a_result_that_is_a_type_parameter_is_taken_whole() {
    let checked = module(concat!(
        "const fn made<V>(store: GlobalState) -> V => get(store, \"k\")\n",
        "operator fetch<V, S>(store: S) -> V\n",
        "impl const fn fetch<V>(store: GlobalState) -> V => get(store, \"k\")\n",
        "const fn uses(store: GlobalState) -> i64 {\n",
        "    let made_written = made<i64?>(store)\n",
        "    let fetched_written = fetch<i64?>(store)\n",
        "    let plain: i64 = fetched_written\n",
        "    return plain\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(
        locals(&checked, "uses")[1..3],
        ["made_written: i64?", "fetched_written: i64?"]
    );
    assert_eq!(
        called(&checked, body(&checked, "uses")),
        ["made<i64?>: i64?", "fetch<i64?>: i64?"]
    );
}

/// GEN-2 and ERR-15: a nullable place can take the nullable form of
/// a fallible store read without an explicit type argument.
#[test]
fn a_nullable_place_infers_the_value_type_of_a_store_read() {
    let checked = module(concat!(
        "fn hooked(value: i64) -> i64? {\n",
        "    inject global_state\n",
        "    when {\n",
        "        put(global_state, \"k\", value)\n",
        "        let saved: i64? = get(global_state, \"k\")\n",
        "        return saved\n",
        "    }\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(
        called(&checked, body(&checked, "hooked")),
        ["put<i64>: _", "get<i64>: result<i64, error>"]
    );
}

/// Ordinary calls of a generic function whose result is its type
/// parameter, in a nullable place, with arguments that say the parameter
/// is not nullable: a binding, a function's result and a parameter, and
/// a function under `requires T in {i64, f64}` (GEN-2, GEN-3, TYPE-14).
#[test]
fn a_nullable_place_accepts_a_result_its_arguments_say_is_not_nullable() {
    let checked = module(concat!(
        "const fn first<T>(values: list<T>) -> T => values[0]\n",
        "const fn larger<T>(a: T, b: T) -> T\n",
        "    requires T in {i64, f64} {\n",
        "    if a < b { return b }\n",
        "    return a\n",
        "}\n",
        "const fn into_let(xs: list<i64>) -> i64 {\n",
        "    let head: i64? = first(xs)\n",
        "    if head == null { return 0 }\n",
        "    return head\n",
        "}\n",
        "const fn into_return(xs: list<i64>) -> i64? => first(xs)\n",
        "const fn into_return_member(a: i64, b: i64) -> i64? => larger(a, b)\n",
        "const fn takes(value: i64?) -> bool => value == null\n",
        "const fn into_argument(xs: list<i64>) -> bool => takes(first(xs))\n",
        "const fn written(xs: list<i64>) -> i64? => first<i64>(xs)\n",
        "const fn repeat<T>(n: i64) -> list<T> => []\n",
        "const fn listed() -> list<str>? => repeat(3)\n",
    ));
    assert_eq!(checked.summary(), NONE);
}
