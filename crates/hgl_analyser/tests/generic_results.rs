//! What a call resolves a result that is a type parameter and nothing
//! more to, the `V` of core's `get<V>(store: GlobalState, const key: str)
//! -> V`, where the call writes a nullable type for it or its place
//! expects one (GEN-2, GEN-8, core's CAP-6). Each test cites its rule,
//! and uses the core the compiler is built with.
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

/// CAP-6: "`get<V>(global_state, key)` is the value under `key`, of the
/// type the call writes or its place expects". GEN-2: a call resolves a
/// generic of its callee "then from the type its place expects of the
/// result". So `let saved: i64? = get(global_state, key)` gives core's
/// candidate an `i64?`, as core's `a_stored_null_is_distinct_from_an_absent_key`
/// reads a key that `put` gave an `i64?`, and so does a function's
/// result that is an `i64?`, in a hook and in value code given the
/// store.
#[test]
fn a_store_read_into_a_nullable_place_is_nullable() {
    let checked = module(concat!(
        "fn hooked(value: i64?) -> i64? {\n",
        "    inject global_state\n",
        "    when {\n",
        "        put(global_state, \"k\", value)\n",
        "        let saved: i64? = get(global_state, \"k\")\n",
        "        let whole: list<i64>? = get(global_state, \"whole\")\n",
        "        let listed: list<i64?> = get(global_state, \"listed\")\n",
        "        return get(global_state, \"k\")\n",
        "    }\n",
        "}\n",
        "const fn valued(store: GlobalState) -> i64? => get(store, \"k\")\n",
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(
        called(&checked, body(&checked, "hooked")),
        [
            "put<i64?>: _",
            "get<i64?>: i64?",
            "get<list<i64>?>: list<i64>?",
            "get<list<i64?>>: list<i64?>",
            "get<i64?>: i64?",
        ]
    );
    assert_eq!(
        called(&checked, body(&checked, "valued")),
        ["get<i64?>: i64?"]
    );
}

/// GEN-2: "A call resolves a generic of its callee from what it writes
/// after the name (GEN-8), then from the type its place expects of the
/// result". GEN-8: "the generic arguments the call writes are the
/// operator's" (`convert<f64>(count)` "supplies T"), and GEN-5: "Match
/// candidates against the requested output before inputs". So a
/// function's or a candidate's result that is its type parameter and
/// nothing more is of the nullable type the call writes or its place
/// expects, as core's `get` is (CAP-6). ERR-12 as above.
#[test]
fn a_result_that_is_a_type_parameter_takes_a_nullable_type_whole() {
    let checked = module(concat!(
        "const fn made<V>(store: GlobalState) -> V => get(store, \"k\")\n",
        "operator fetch<V, S>(store: S) -> V\n",
        "impl const fn fetch<V>(store: GlobalState) -> V => get(store, \"k\")\n",
        "const fn uses(store: GlobalState) -> i64 {\n",
        "    let made_placed: i64? = made(store)\n",
        "    let made_written = made<i64?>(store)\n",
        "    let fetched_placed: i64? = fetch(store)\n",
        "    let fetched_written = fetch<i64?>(store)\n",
        "    let plain: i64 = fetched_written\n",
        "    return plain\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:10:22 type/nullable.required_presence ERR-12"]
    );
    assert_eq!(locals(&checked, "uses")[4], "fetched_written: i64?");
    assert_eq!(
        called(&checked, body(&checked, "uses")),
        [
            "made<i64?>: i64?",
            "made<i64?>: i64?",
            "fetch<i64?>: i64?",
            "fetch<i64?>: i64?",
        ]
    );
}

/// GEN-2: "The first of these that says what a parameter is decides it,
/// and what follows is checked against that". The place says what a
/// result that is its type parameter is before the arguments do: `T` is
/// an `i64?` for `same(a, b)` returned from a function whose result is
/// one, each `i64` a value of it (TYPE-14: "A T is a value where a `T?`
/// is expected"), and a `list<i64>` is then no `list<i64?>` for
/// `first(xs)`. TYPE-14: a result of another form is no null, so the
/// `?` a place expects is the place's: `repeat(3)` given to a
/// `list<str>?` is a `list<str>`.
#[test]
fn the_place_decides_a_result_that_is_a_type_parameter_before_the_arguments() {
    let checked = module(concat!(
        "const fn same<T>(left: T, right: T) -> T => left\n",
        "const fn first<T>(values: list<T>) -> T => values[0]\n",
        "const fn repeat<T>(n: i64) -> list<T> => []\n",
        "const fn uses(a: i64, b: i64, xs: list<i64>) -> i64? {\n",
        "    let names: list<str>? = repeat(3)\n",
        "    let head: i64? = first(xs)\n",
        "    return same(a, b)\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), ["m.hgl:7:28 type/type.mismatch EXPR-5"]);
    let calls = called(&checked, body(&checked, "uses"));
    assert_eq!(calls[0], "repeat<str>: list<str>");
    assert_eq!(calls[2], "same<i64?>: i64?");
}
