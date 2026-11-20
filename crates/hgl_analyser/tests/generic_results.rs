//! What a call resolves a result that is a type parameter and nothing
//! more to, the `V` of core's `get<V>(store: GlobalState, const key: str)
//! -> V`, where the call writes a nullable type for it (GEN-2, GEN-8,
//! core's CAP-6), and what a nullable place makes of it, which is not
//! decided (hhenson/hgl_spec#165). Each test cites its rule, and uses the
//! core the compiler is built with.
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
            Callee::Required { .. } | Callee::Unread => {
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

/// CAP-6: "`get<V>(global_state, key)` is the value under `key`, of the
/// type the call writes or its place expects". GEN-2: "A call resolves a
/// generic of its callee from what it writes after the name (GEN-8) ...
/// What the call writes, or its place expects, is not widened". GEN-8:
/// "A call may supply its generic arguments as a generic constructor
/// does". So `get<i64?>(global_state, key)` is an `i64?` and core's
/// candidate is given an `i64?`, as `put` of an `i64?` is, in a hook and
/// in value code given the store, whether the `?` is the whole type's or
/// an element's. ERR-12: an `i64?` given where an `i64` is taken is
/// `nullable.required_presence`.
#[test]
fn a_store_read_written_nullable_is_nullable() {
    let checked = module(concat!(
        "fn hooked(value: i64?) -> i64 {\n",
        "    inject global_state\n",
        "    when {\n",
        "        put(global_state, \"k\", value)\n",
        "        let other = get<i64?>(global_state, \"k\")\n",
        "        let whole = get<list<i64>?>(global_state, \"whole\")\n",
        "        let listed = get<list<i64?>>(global_state, \"listed\")\n",
        "        let paired = get<tuple<i64?, str>>(global_state, \"paired\")\n",
        "        let plain: i64 = other\n",
        "        return plain\n",
        "    }\n",
        "}\n",
        "const fn valued(store: GlobalState) -> i64 {\n",
        "    let other = get<i64?>(store, \"k\")\n",
        "    if other == null { return 0 }\n",
        "    return other\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:10:26 type/nullable.required_presence ERR-12"]
    );
    assert_eq!(
        locals(&checked, "hooked")[2..6],
        [
            "other: i64?",
            "whole: list<i64>?",
            "listed: list<i64?>",
            "paired: tuple<i64?, str>",
        ]
    );
    assert_eq!(
        called(&checked, body(&checked, "hooked")),
        [
            "put<i64?>: _",
            "get<i64?>: i64?",
            "get<list<i64>?>: list<i64>?",
            "get<list<i64?>>: list<i64?>",
            "get<tuple<i64?, str>>: tuple<i64?, str>",
        ]
    );
    assert_eq!(locals(&checked, "valued")[1], "other: i64?");
    assert_eq!(
        called(&checked, body(&checked, "valued"))[0],
        "get<i64?>: i64?"
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
    assert_eq!(
        checked.summary(),
        ["m.hgl:8:22 type/nullable.required_presence ERR-12"]
    );
    assert_eq!(
        locals(&checked, "uses")[1..3],
        ["made_written: i64?", "fetched_written: i64?"]
    );
    assert_eq!(
        called(&checked, body(&checked, "uses")),
        ["made<i64?>: i64?", "fetch<i64?>: i64?"]
    );
}

/// Awaiting hhenson/hgl_spec#165, which asks what a nullable place says
/// of a result that is a type parameter and nothing more. GEN-2 has a
/// call resolve a generic "from the type its place expects of the
/// result", and core's CAP-6 has `get` "of the type the call writes or
/// its place expects"; the text does not say whether an `i64?` place
/// makes the `V` of `get<V>(...) -> V` an `i64?` or an `i64`. This
/// records what is done until it does: the `i64` under the `?`, a `T`
/// being a value of a `T?` (TYPE-14).
#[test]
fn a_store_read_into_a_nullable_place_awaits_hgl_spec_165() {
    let checked = module(concat!(
        "fn hooked(value: i64?) -> i64? {\n",
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
        ["put<i64?>: _", "get<i64>: i64"]
    );
}

/// Ordinary calls of a generic function whose result is its type
/// parameter, in a nullable place, with arguments that say the parameter
/// is not nullable: a binding, a function's result and a parameter, and
/// a function under `requires T in {i64, f64}` (GEN-2, GEN-3, TYPE-14).
/// They are accepted. What a nullable place says of such a result is
/// not decided (hhenson/hgl_spec#165); a change that rejects them is a
/// decision of that issue's, and not of this compiler's.
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
