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
            | Callee::DeferredPackCall
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
                Argument::NoResult => "none".to_string(),
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

/// GEN-2, GEN-8 and CAP-6: a nullable place leaves the store's value
/// type open when the call writes no type.
#[test]
fn a_nullable_store_read_requires_a_written_value_type() {
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
    assert_eq!(
        checked.summary(),
        ["m.hgl:6:27 type/generic.unresolved GEN-8"],
        "{}",
        checked.report
    );
    assert!(
        checked
            .report
            .contains("write the type argument, `get<i64?>(...)` or `get<i64>(...)`"),
        "{}",
        checked.report
    );
}

/// TIME-8 and CAP-6: the written read type is the key's type even
/// where the read itself is used as a nullable value.
#[test]
fn an_explicit_store_read_checks_the_type_of_the_key() {
    let checked = module(concat!(
        "fn hooked(value: i64?) -> i64? {\n",
        "    inject global_state\n",
        "    when {\n",
        "        put(global_state, \"k\", value)\n",
        "        let saved: i64? = get<i64>(global_state, \"k\")\n",
        "        return saved\n",
        "    }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:6:27 injectable/store.key_type TIME-8"],
        "{}",
        checked.report
    );
}

/// GEN-2 and GEN-8: a local, return or argument's nullable type leaves
/// the bare result type open where the call's arguments say nothing.
#[test]
fn a_nullable_place_alone_does_not_decide_a_bare_result_type() {
    let checked = module(concat!(
        "const fn choose<T>() -> T {\n",
        "    let values: list<T> = []\n",
        "    return values[0]\n",
        "}\n",
        "const fn into_local<T>() -> T? {\n",
        "    let item: T? = choose()\n",
        "    return item\n",
        "}\n",
        "const fn into_return() -> i64? { return choose() }\n",
        "const fn take_optional(value: i64?) -> i64? => value\n",
        "const fn into_argument() -> i64? => take_optional(choose())\n",
        "const fn plain() -> i64 => choose()\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:7:20 type/generic.unresolved GEN-8",
            "m.hgl:10:41 type/generic.unresolved GEN-8",
            "m.hgl:12:51 type/generic.unresolved GEN-8",
        ],
        "{}",
        checked.report
    );
    assert_eq!(
        called(&checked, body(&checked, "plain")),
        ["choose<i64>: i64"]
    );
}

/// GEN-5: an invalid input still leaves no candidate to select, even
/// with the result type supplied by a plain place.
#[test]
fn a_nullable_store_read_with_an_invalid_key_has_no_candidate() {
    let checked = module("const fn read(store: GlobalState) -> i64? => get(store, 1)\n");
    assert_eq!(
        checked.summary(),
        ["m.hgl:2:46 operator/operator.no_candidate GEN-5"],
        "{}",
        checked.report
    );
}

/// GEN-2, GEN-8 and TIME-7: a node's nullable result supplies the
/// same whole-value place, including the value of a block or branch.
#[test]
fn a_nullable_publication_checks_an_unresolved_store_read() {
    let checked = module(concat!(
        "const fn save(store: GlobalState, const key: str, value: i64) -> i64 {\n",
        "    put(store, key, value)\n",
        "    return value\n",
        "}\n",
        "fn absent(value: i64) -> i64? {\n",
        "    inject global_state\n",
        "    when { return get(global_state, \"absent\") }\n",
        "}\n",
        "fn visible(value: i64) -> i64? {\n",
        "    inject global_state\n",
        "    when {\n",
        "        put(global_state, \"visible\", value)\n",
        "        return get(global_state, \"visible\")\n",
        "    }\n",
        "}\n",
        "fn hidden(value: i64) -> i64? {\n",
        "    inject global_state\n",
        "    when {\n",
        "        let saved = save(global_state, \"hidden\", value)\n",
        "        return (get(global_state, \"hidden\"))\n",
        "    }\n",
        "}\n",
        "fn nested(value: i64) -> i64? {\n",
        "    inject global_state\n",
        "    when {\n",
        "        return { if value > 0 { get(global_state, \"branch\") } else { null } }\n",
        "    }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:8:19 type/generic.unresolved GEN-8",
            "m.hgl:14:16 type/generic.unresolved GEN-8",
            "m.hgl:21:17 type/generic.unresolved GEN-8",
            "m.hgl:27:33 type/generic.unresolved GEN-8",
        ],
        "{}",
        checked.report
    );
}

/// TIME-7 and TYPE-16: a call whose declared result is a delta can
/// still infer its parameter from a nullable result's delta place.
#[test]
fn a_nullable_publication_can_infer_the_type_of_a_delta() {
    let checked = module(concat!(
        "const fn changed<V>() -> delta<V?> => delta<V?>()\n",
        "fn changed_node(value: i64) -> i64? {\n",
        "    when { return changed() }\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
    assert_eq!(
        called(&checked, body(&checked, "changed_node")),
        ["changed<i64>: delta<i64?>"]
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

/// TIME-7 and TST-5: assignment, yield and trace cells use the
/// nullable publication type before considering its delta.
#[test]
fn nullable_publications_do_not_infer_a_bare_success() {
    let checked = module(concat!(
        "const fn choose<T>() -> T {\n",
        "    let empty: list<T> = []\n",
        "    return empty[0]\n",
        "}\n",
        "const fn fetch<T>(value: i64) -> result<T, error> =>\n",
        "    err(error(message: \"none\", code: \"sample.none\"))\n",
        "operator get_value<O>(value: i64) -> O\n",
        "impl const fn get_value<T>(value: i64) -> result<T, error> => fetch<T>(value)\n",
        "fn assigned(value: i64) -> i64? {\n",
        "    inject out\n",
        "    when { out = choose() }\n",
        "}\n",
        "fn yielded(const values: list<i64>) -> i64? {\n",
        "    for value in values { yield 1us: fetch(value) }\n",
        "}\n",
        "fn selected(value: i64) -> i64? {\n",
        "    when { return get_value(value) }\n",
        "}\n",
        "fn relay(value: i64?) -> i64? => value\n",
        "test a { assert eval relay(value: [null]) == [choose()] }\n",
        "test b { assert eval relay(value: [null]) == [fetch(1)] }\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:12:18 type/generic.unresolved GEN-8",
            "m.hgl:15:38 type/generic.unresolved GEN-8",
            "m.hgl:18:19 type/generic.unresolved GEN-8",
            "m.hgl:21:47 type/generic.unresolved GEN-8",
            "m.hgl:22:47 type/generic.unresolved GEN-8",
        ],
        "{}",
        checked.report
    );
}

/// GEN-5: a candidate needing the contextual success still counts
/// when the call is ambiguous, with either concrete result shape.
#[test]
fn a_bare_fallible_candidate_does_not_hide_an_ambiguity() {
    for (concrete, condition) in [("i64?", ""), ("i64", ""), ("i64?", " requires T == i64")] {
        let source = format!(
            "operator convert_value<O>(value: i64) -> O\n\
             impl const fn convert_value<T>(value: i64) -> result<T, error>{condition} =>\n\
                 err(error(message: \"none\", code: \"sample.none\"))\n\
             impl const fn convert_value(value: i64) -> {concrete} => value\n\
             const fn read() -> i64? => convert_value(1)\n"
        );
        let checked = module(&source);
        assert_eq!(
            checked.summary(),
            ["m.hgl:6:28 operator/operator.ambiguous GEN-5"],
            "{}",
            checked.report
        );
    }
}

/// GEN-2 and GEN-3: an equality can decide the bare result of a
/// candidate read into a nullable place.
#[test]
fn an_equality_can_resolve_a_fallible_candidates_bare_result() {
    let checked = module(concat!(
        "operator convert_value<O>(value: i64) -> O\n",
        "impl const fn convert_value<T>(value: i64) -> result<T, error> requires T == i64 =>\n",
        "    err(error(message: \"none\", code: \"sample.none\"))\n",
        "const fn read() -> i64? => convert_value(1)\n",
        "const fn written() -> i64? => convert_value<i64>(1)\n",
    ));
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
    assert_eq!(
        called(&checked, body(&checked, "read")),
        ["convert_value<i64>: result<i64, error>"]
    );
}

/// GEN-2, proposed: a membership in several types permits them and
/// supplies no single type for a bare result in a nullable place.
#[test]
fn a_type_membership_leaves_a_nullable_bare_result_unresolved() {
    let checked = module(concat!(
        "operator convert_value<O>(value: i64) -> O\n",
        "impl const fn convert_value<T>(value: i64) -> result<T, error> requires T in {i64, f64} =>\n",
        "    err(error(message: \"none\", code: \"sample.none\"))\n",
        "const fn read() -> i64? => convert_value(1)\n",
        "const fn written() -> i64? => convert_value<i64>(1)\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:5:28 type/generic.unresolved GEN-8"],
        "{}",
        checked.report
    );
}

/// CAP-6 and FN-16: the ordinary faults remain primary when a nullable
/// store read would also have a bare result to resolve.
#[test]
fn a_nullable_store_read_preserves_the_calls_own_diagnostic() {
    let checked = module(concat!(
        "const fn from_runtime_key(store: GlobalState, key: str) -> i64? => get(store, key)\n",
        "const fn from_reserved_key(store: GlobalState) -> i64? => get(store, \"run.output\")\n",
        "fn from_ref_result(value: i64) -> ref i64? {\n",
        "    inject global_state\n",
        "    when { return get(global_state, \"absent\") }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:2:79 type/argument.constant FN-1",
            "m.hgl:3:70 injectable/store.reserved_key TIME-8",
            "m.hgl:6:19 type/type.mismatch FN-16",
        ],
        "{}",
        checked.report
    );
}

/// GEN-8: the diagnostic's type argument occupies the same position
/// as the one that checks successfully in the public operator.
#[test]
fn a_nullable_operator_result_names_its_public_type_argument() {
    let checked = module(concat!(
        "operator pick_from<K, O>(key: K) -> O\n",
        "impl const fn pick_from<T>(key: str) -> T {\n",
        "    let empty: list<T> = []\n",
        "    return empty[0]\n",
        "}\n",
        "const fn unresolved() -> i64? => pick_from(\"a\")\n",
        "const fn written() -> i64? => pick_from<str, i64?>(\"a\")\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:7:34 type/generic.unresolved GEN-8"],
        "{}",
        checked.report
    );
    assert!(
        checked.report.contains("leaves `O` unresolved"),
        "{}",
        checked.report
    );
    assert!(
        checked.report.contains("`pick_from<str, i64?>(...)`"),
        "{}",
        checked.report
    );
}

/// ERR-9: a rejected nullable bare success does not cascade into a
/// mismatch, while a known incompatible success still reports one.
#[test]
fn an_unresolved_fallible_call_does_not_cascade_into_a_type_mismatch() {
    let checked = module(concat!(
        "const fn fetch<T>(store: GlobalState, const key: str) -> result<T, error> =>\n",
        "    get<T>(store, key)\n",
        "fn read(value: i64) -> i64? {\n",
        "    inject global_state\n",
        "    when {\n",
        "        let saved: i64? = fetch(global_state, \"absent\")\n",
        "        return saved\n",
        "    }\n",
        "}\n",
        "const fn incompatible(store: GlobalState) -> i64? {\n",
        "    let saved: i64? = fetch<str>(store, \"a\")\n",
        "    return saved\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:7:27 type/generic.unresolved GEN-8",
            "m.hgl:12:23 type/type.mismatch EXPR-5",
        ],
        "{}",
        checked.report
    );
}

/// GEN-2 and TIME-7: a symbol's fallible bare result is unresolved in a
/// nullable value, whether bound locally or published as a node result.
#[test]
fn a_symbol_rejects_an_unresolved_bare_result_in_a_nullable_place() {
    let checked = module(concat!(
        "struct Room { size: i64 }\n",
        "impl const fn add_<T>(room: Room, label: str) -> result<T, error> =>\n",
        "    err(error(message: \"none\", code: \"sample.none\"))\n",
        "const fn local(value: i64) -> i64? {\n",
        "    let saved: i64? = Room(size: value) + \"a\"\n",
        "    return saved\n",
        "}\n",
        "fn returned(value: i64) -> i64? {\n",
        "    when { return Room(size: value) + \"a\" }\n",
        "}\n",
        "fn assigned(value: i64) -> i64? {\n",
        "    inject out\n",
        "    when { out = Room(size: value) + \"a\" }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:6:41 type/generic.unresolved GEN-8",
            "m.hgl:10:37 type/generic.unresolved GEN-8",
            "m.hgl:14:36 type/generic.unresolved GEN-8",
        ],
        "{}",
        checked.report
    );
}
