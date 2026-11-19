//! The analyser against the rules on an error's provenance and on a
//! `result` as a temporal value: an error's `.context` and its parts
//! (ERR-4), and the envelopes as input and output types (TYPE-13). Each
//! test cites its rule.
//!
//! A finding is written `file:line:column category/code rule`.

mod common;

use common::{Checked, body, locals, module};
use hgl_analyser::hir::{ErrorPart, ExprKind, walk_statements};

/// No lines.
const NONE: [&str; 0] = [];

/// Each part of an error or of its context that the body of `name`
/// reads, with its type, in the order the walk meets them.
fn parts(checked: &Checked, name: &str) -> Vec<(ErrorPart, String)> {
    let mut found = Vec::new();
    walk_statements(&body(checked, name).statements, &mut |expr| {
        if let ExprKind::ErrorPart { part, .. } = &expr.kind {
            found.push((*part, expr.ty.to_string()));
        }
    });
    found
}

/// ERR-4: "`.context` is a read-only nullable record of `.node: str`,
/// `.phase: str`, one of `"evaluate"`, `"start"`, and `"stop"`, and
/// `.evaluation_time: datetime`; a constructed error has none." ERR-12:
/// a test against null proves an immutable binding of it present, in a
/// value function and in a `when`.
#[test]
fn an_errors_context_is_a_nullable_record_of_three_parts() {
    let checked = module(concat!(
        "const fn origin(failure: error) -> str {\n",
        "    let context = failure.context\n",
        "    if context == null { return \"none\" }\n",
        "    return context.node\n",
        "}\n",
        "const fn timed(failure: error) -> datetime? {\n",
        "    let context = failure.context\n",
        "    if context != null && context.phase == \"evaluate\" {\n",
        "        return context.evaluation_time\n",
        "    }\n",
        "    return null\n",
        "}\n",
        "const fn same(first: error, second: error) -> bool {\n",
        "    let one = first.context\n",
        "    let other = second.context\n",
        "    return one == other\n",
        "}\n",
        "fn phase_of(failure: error) -> str {\n",
        "    when {\n",
        "        let context = failure.context\n",
        "        if context == null { return \"none\" }\n",
        "        return context.phase\n",
        "    }\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(
        locals(&checked, "origin"),
        ["failure: error", "context: error context?"]
    );
    let read = |name| {
        parts(&checked, name)
            .into_iter()
            .map(|(part, ty)| format!("{part:?}: {ty}"))
            .collect::<Vec<_>>()
    };
    assert_eq!(read("origin"), ["Context: error context?", "Node: str"]);
    assert_eq!(
        read("timed"),
        [
            "Context: error context?",
            "Phase: str",
            "EvaluationTime: datetime",
        ]
    );
    assert_eq!(read("phase_of"), ["Context: error context?", "Phase: str"]);
}

/// ERR-4: "Nullable code/context observations require presence
/// checking": a part of a context is read where a test has proved it
/// present, and on no fresh expression. A context has the three parts
/// and no other, each of its type; "`.context` is a read-only nullable
/// record", so neither it nor a part of it is assigned.
#[test]
fn a_context_is_guarded_typed_and_read_only() {
    let checked = module(concat!(
        "const fn unguarded(failure: error) -> str => failure.context.node\n",
        "const fn unproved(failure: error) -> str {\n",
        "    let context = failure.context\n",
        "    return context.phase\n",
        "}\n",
        "const fn mistyped(failure: error) -> i64 {\n",
        "    let context = failure.context\n",
        "    if context == null { return 0 }\n",
        "    let label = context.label\n",
        "    return context.node\n",
        "}\n",
        "const fn assigned(failure: error) -> str {\n",
        "    var copy = failure\n",
        "    copy.context = null\n",
        "    let context = failure.context\n",
        "    if context == null { return \"\" }\n",
        "    var held = [context]\n",
        "    held[0].node = \"elsewhere\"\n",
        "    return held[0].phase\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:2:46 type/nullable.required_presence ERR-4",
            "m.hgl:5:12 type/nullable.required_presence ERR-4",
            "m.hgl:10:25 name/member.unknown ERR-4",
            "m.hgl:11:12 type/type.mismatch EXPR-5",
            "m.hgl:15:5 name/binding.immutable ERR-4",
            "m.hgl:19:5 name/binding.immutable ERR-4",
        ]
    );
}

/// TYPE-13: "Result and error envelopes are ordinary atomic values;
/// their tag/refinement rules govern payload access. Silence,
/// invalidity, null absence, and an error outcome are distinct." Each
/// is its own atomic and its own delta (TYPE-9, TYPE-16), its payloads
/// are ordinary value types, a null is no result, and a nullable result
/// is tested for its tag only where it is proved present (ERR-3,
/// ERR-12).
#[test]
fn the_envelopes_are_ordinary_atomic_values() {
    let checked = module(concat!(
        "fn atom(value: atomic<result<i64, error>>, failure: atomic<error>) -> result<i64, error> {\n",
        "    when {\n",
        "        let change: delta<result<i64, error>> = delta_value(value)\n",
        "        let lost: delta<error> = delta_value(failure)\n",
        "        let held: error = lost\n",
        "        return change\n",
        "    }\n",
        "}\n",
        "const fn proved(value: result<i64, error>?) -> bool {\n",
        "    if value == null { return false }\n",
        "    return is_ok(value)\n",
        "}\n",
        "const fn nulled() -> result<i64, error> => null\n",
        "const fn untested(value: result<i64, error>?) -> bool => is_ok(value)\n",
        "fn windowed(value: result<rolling_window<f64, 5>, error>) -> i64 { when { return 1 } }\n",
        "const fn other(value: result<i64, str>) -> result<i64, error> => value\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:14:44 type/type.mismatch TYPE-14",
            "m.hgl:15:64 type/type.mismatch ERR-3",
            "m.hgl:16:27 type/type.not_ordinary TYPE-13",
            "m.hgl:17:66 type/type.mismatch EXPR-5",
        ]
    );
    assert_eq!(
        locals(&checked, "atom"),
        [
            "value: result<i64, error>",
            "failure: error",
            "change: result<i64, error>",
            "lost: error",
            "held: error",
        ]
    );
}

/// ERR-12: "Refine that same binding to T on paths proving a non-null T?
/// value ... Using it where T is required without that proof rejects
/// with `nullable.required_presence`." EXPR-7: a member and an element
/// are read of the payload, so a nullable's is read where a test
/// against null proves the binding present, in a value function and in
/// a `when`. What is read with no proof is not known, and nothing more
/// is reported of it (ERR-9): `mistyped` reports the presence alone.
#[test]
fn a_nullables_payload_is_read_where_it_is_proved_present() {
    let checked = module(concat!(
        "struct Quote {\n",
        "    bid: f64\n",
        "}\n",
        "const fn message_of(failure: error?) -> str => failure.message\n",
        "const fn bid_of(quote: Quote?) -> f64 => quote.bid\n",
        "const fn value_of(outcome: result<i64, error>?) -> i64 => outcome.value\n",
        "const fn head(items: list<i64>?) -> i64 => items[0]\n",
        "const fn priced(book: dict<str, i64>?) -> i64 => book[\"a\"]\n",
        "const fn mistyped(failure: error?) -> i64 => failure.message\n",
        "const fn guarded(failure: error?, quote: Quote?, items: list<i64>?) -> f64 {\n",
        "    if failure == null || quote == null || items == null { return 0.0 }\n",
        "    let message: str = failure.message\n",
        "    let leading: i64 = items[0]\n",
        "    return quote.bid\n",
        "}\n",
        "fn watched(quote: Quote?) -> f64 {\n",
        "    when {\n",
        "        if quote == null { return 0.0 }\n",
        "        return quote.bid\n",
        "    }\n",
        "}\n",
        "fn unwatched(quote: Quote?) -> f64 {\n",
        "    when { return quote.bid }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:5:48 type/nullable.required_presence ERR-12",
            "m.hgl:6:42 type/nullable.required_presence ERR-12",
            "m.hgl:7:59 type/nullable.required_presence ERR-12",
            "m.hgl:8:44 type/nullable.required_presence ERR-12",
            "m.hgl:9:50 type/nullable.required_presence ERR-12",
            "m.hgl:10:46 type/nullable.required_presence ERR-12",
            "m.hgl:24:19 type/nullable.required_presence ERR-12",
        ]
    );
}
