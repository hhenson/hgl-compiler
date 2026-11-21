//! The analyser against the rules on results and errors: the
//! constructors, the tag tests and the facts they give, an error's
//! parts, and propagation. Each test cites its rule.
//!
//! A finding is written `file:line:column category/code rule`.

mod common;

use common::{body, check, locals, module};
use hgl_analyser::hir::{ExprKind, Stmt};

/// No lines.
const NONE: [&str; 0] = [];

/// The chapter's `examples.results`, less its module line.
const RESULTS: &str = concat!(
    "const fn checked(value: i64) -> result<i64, error> {\n",
    "    if value < 0 {\n",
    "        return err(error(message: \"negative price\", code: \"price.negative\"))\n",
    "    }\n",
    "    return ok(value)\n",
    "}\n",
    "const fn doubled(value: i64) -> result<i64, error> {\n",
    "    let price: i64 = checked(value)\n",
    "    return ok(price * 2)\n",
    "}\n",
    "const fn recovered(value: i64) -> i64 {\n",
    "    let outcome = checked(value)\n",
    "    if is_err(outcome) { return 0 }\n",
    "    return outcome.value\n",
    "}\n",
    "const fn diagnostic(outcome: result<i64, error>) -> str {\n",
    "    if is_ok(outcome) { return \"ok\" }\n",
    "    return outcome.error.message\n",
    "}\n",
    "fn required(value: i64) -> i64 {\n",
    "    when { return checked(value) }\n",
    "}\n",
    "fn tagged(value: i64) -> result<i64, error> {\n",
    "    when { return checked(value) }\n",
    "}\n",
    "fn is_failure(value: result<i64, error>) -> bool => is_err(value)\n",
);

/// ERR-2: "`ok(value)` constructs a success and `err(value)` a failure
/// ... Both type arguments come from the complete `result<T,E>` its
/// place expects". ERR-3: `is_ok` and `is_err` "give a `bool`", and a
/// guard proves the binding's tag. ERR-4: `error(message: text, code:
/// optional_code)`. ERR-5: "Success yields T." ERR-13: each name is the
/// form the chapter defines, and is looked up nowhere.
#[test]
fn the_chapters_results_are_read() {
    let checked = module(RESULTS);
    assert_eq!(checked.summary(), NONE);
    // None of the five names is left for the prelude to bind.
    assert_eq!(checked.prelude_names("m"), NONE);
    assert_eq!(
        locals(&checked, "doubled"),
        ["value: i64", "price: i64"],
        "the place takes a success as its T"
    );
    assert_eq!(
        locals(&checked, "recovered"),
        ["value: i64", "outcome: result<i64, error>"]
    );
    // `return err(error(...))`: a failure that owns an error.
    let Some(Stmt::Expr(first)) = body(&checked, "checked").statements.first() else {
        panic!("`checked` begins with an `if`");
    };
    let ExprKind::If { then, .. } = &first.kind else {
        panic!("`checked` begins with an `if`");
    };
    let ExprKind::Block { statements, .. } = &then.kind else {
        panic!("the branch is a block");
    };
    let Some(Stmt::Return {
        value: Some(returned),
        ..
    }) = statements.first()
    else {
        panic!("the branch returns a value");
    };
    assert_eq!(returned.ty.to_string(), "result<i64, error>");
    let ExprKind::Tagged { success, payload } = &returned.kind else {
        panic!(
            "`err(...)` is a tagged result, and this is {:?}",
            returned.kind
        );
    };
    assert!(!success);
    assert_eq!(payload.ty.to_string(), "error");
    let ExprKind::Error { code, .. } = &payload.kind else {
        panic!("the payload is `error(...)`");
    };
    assert!(code.is_some(), "the code is written");
}

/// ERR-2: "Each takes one argument by position and no generic argument
/// ... `value` is read as that T for `ok` and as that E for `err`.
/// Where the place expects no complete result type the constructor
/// rejects (`result.context_type`), and a payload that is not of its
/// side's type rejects as any value does (EXPR-5)."
#[test]
fn a_constructor_takes_its_types_from_its_place() {
    let checked = module(concat!(
        "const fn made(flag: bool) -> result<f64, str> {\n",
        // An integer where the success is an `f64` is widened; the
        // result may be nullable.
        "    let whole: result<f64, str> = ok(1)\n",
        "    let maybe: result<f64, str>? = err(\"no\")\n",
        // Nothing says the type; another type is expected.
        "    let missing = ok(1)\n",
        "    let number: i64 = ok(1)\n",
        // A payload of the other side's type.
        "    let crossed: result<f64, str> = ok(\"text\")\n",
        "    let swapped: result<f64, str> = err(2.0)\n",
        // One value, by position.
        "    let none: result<f64, str> = ok()\n",
        "    let two: result<f64, str> = ok(1.0, 2.0)\n",
        "    let named: result<f64, str> = err(value: \"no\")\n",
        // No generic argument: the form is read as it is without.
        "    let typed: result<f64, str> = ok<f64>(1.0)\n",
        "    if flag { return ok(1.0) }\n",
        "    return err(\"never\")\n",
        "}\n",
        // An argument of an operator's call is read as the candidate's
        // parameter, and the call is selected and typed: no candidate
        // gives a `str`, and none takes a failure of `i64` (GEN-5).
        "operator consume<V>(outcome: V) -> i64\n",
        "impl const fn consume(outcome: result<i64, str>) -> i64 => 0\n",
        "const fn right() -> i64 => consume(ok(1))\n",
        "const fn wrong() -> str => consume(ok(1))\n",
        "const fn mixed() -> i64 => consume(err(1))\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:5:19 type/result.context_type ERR-2",
            "m.hgl:6:23 type/result.context_type ERR-2",
            "m.hgl:7:40 type/type.mismatch EXPR-5",
            "m.hgl:8:41 type/type.mismatch EXPR-5",
            "m.hgl:9:34 type/argument.missing ERR-2",
            "m.hgl:10:41 type/argument.unexpected ERR-2",
            "m.hgl:11:39 name/argument.unknown ERR-2",
            "m.hgl:12:35 type/generic.argument_count ERR-2",
            "m.hgl:19:28 operator/operator.no_candidate GEN-5",
            "m.hgl:20:28 operator/operator.no_candidate GEN-5",
        ]
    );
    assert_eq!(
        locals(&checked, "made")[1..3],
        ["whole: result<f64, str>", "maybe: result<f64, str>?"]
    );
}

/// ERR-3: "An immutable result binding, a `let`, a parameter, or an
/// iteration binding, gains its tag fact from a direct `is_ok`/`is_err`
/// guard on it by its name: where `is_ok` holds it is a success and
/// where it does not a failure, and `is_err` the reverse ...
/// `.value`/`.error` requires the matching fact, and without it, or on
/// what is no such binding, rejects (`result.required_variant`). Track
/// negation, short-circuit paths, terminating branches, and merges. A
/// saved bool, alias, fresh expression, or mutable var supplies no
/// retained fact."
#[test]
fn a_guard_proves_a_results_tag() {
    let checked = module(concat!(
        "const fn checked(value: i64) -> result<i64, str> => ok(value)\n",
        "const fn reads(outcome: result<i64, str>, other: result<i64, str>) -> i64 {\n",
        // Each branch of a guard, and its negation.
        "    let first = if is_ok(outcome) { outcome.value } else { 0 }\n",
        "    let text = if is_ok(outcome) { \"\" } else { outcome.error }\n",
        "    let second = if !is_err(outcome) { outcome.value } else { 0 }\n",
        // Short circuit: the right operand is read where the left
        // holds.
        "    let both = is_ok(outcome) && outcome.value > 0\n",
        "    let either = is_err(outcome) || outcome.value > 0\n",
        // No guard; the other tag's guard; a guard on another binding.
        "    let bare = outcome.value\n",
        "    let wrong = if is_err(outcome) { outcome.value } else { 0 }\n",
        "    let cross = if is_ok(other) { outcome.value } else { 0 }\n",
        // A saved bool, a fresh expression and a `var` keep no fact.
        "    let saved = is_ok(outcome)\n",
        "    let later = if saved { outcome.value } else { 0 }\n",
        "    let fresh = checked(1).value\n",
        "    var moving = checked(2)\n",
        "    let moved = if is_ok(moving) { moving.value } else { 0 }\n",
        // A branch that returns proves the other tag after it.
        "    if is_err(other) { return 0 }\n",
        "    return other.value\n",
        "}\n",
        // A nullable result proved present is tested as its result, and
        // the two guards compose (ERR-12).
        "const fn maybe(outcome: result<i64, str>?) -> i64 {\n",
        "    if outcome == null { return 0 }\n",
        "    if is_err(outcome) { return 0 }\n",
        "    return outcome.value\n",
        "}\n",
        // In one condition: the right operand of `&&` is read where the
        // left has proved the result present.
        "const fn joined(outcome: result<i64, str>?) -> i64 {\n",
        "    if outcome != null && is_ok(outcome) { return outcome.value }\n",
        "    if outcome == null || is_err(outcome) { return 0 }\n",
        "    return outcome.value\n",
        "}\n",
        // Any other type rejects, and a result has two parts.
        "const fn tests(value: i64, outcome: result<i64, str>) -> bool {\n",
        "    let odd = if is_ok(outcome) { outcome.payload } else { 0 }\n",
        "    return is_ok(value)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:9:24 type/result.required_variant ERR-3",
            "m.hgl:10:46 type/result.required_variant ERR-3",
            "m.hgl:11:43 type/result.required_variant ERR-3",
            "m.hgl:13:36 type/result.required_variant ERR-3",
            "m.hgl:14:28 type/result.required_variant ERR-3",
            "m.hgl:16:43 type/result.required_variant ERR-3",
            "m.hgl:31:35 type/result.propagation_type ERR-5",
            "m.hgl:32:18 type/type.mismatch ERR-3",
        ]
    );
    assert_eq!(
        locals(&checked, "reads")[2..7],
        [
            "first: i64",
            "text: str",
            "second: i64",
            "both: bool",
            "either: bool",
        ]
    );
}

/// ERR-3: "In composition a test of a connection is an ordinary lifted
/// call (FN-6): it gives a connection of `bool` and no fact." A guard on
/// an ordinary value there proves its tag as anywhere.
#[test]
fn a_test_of_a_connection_gives_no_fact() {
    let checked = module(concat!(
        "fn wired(value: result<i64, str>, const fixed: result<i64, str>) -> i64 {\n",
        "    let known = if is_ok(fixed) { fixed.value } else { 0 }\n",
        "    let failed = is_err(value)\n",
        "    return if is_ok(value) { value.value } else { known }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:5:36 type/result.required_variant ERR-3"]
    );
}

/// ERR-4: "`error(message: text, code: optional_code)` owns a required
/// string message and optional string code, each given by its name:
/// `message` is a `str`, and `code` a `str?` that is null when omitted.
/// `.message` is the `str` and `.code` the `str?`."
#[test]
fn an_error_owns_a_message_and_a_code() {
    let checked = module(concat!(
        "const fn made(text: str) -> error {\n",
        "    let plain = error(message: text)\n",
        "    let coded = error(code: \"price.negative\", message: \"negative\")\n",
        "    let absent = error(message: text, code: null)\n",
        "    let message = coded.message\n",
        "    let code = coded.code\n",
        // By position, with no message, twice, a name it has not, and
        // a message that is no string.
        "    let placed = error(text)\n",
        "    let empty = error(code: \"x\")\n",
        "    let twice = error(message: text, message: text)\n",
        "    let extra = error(message: text, detail: text)\n",
        "    let typed = error(message: 1)\n",
        "    let piece = plain.detail\n",
        "    return plain\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:8:18 type/argument.missing ERR-4",
            "m.hgl:8:24 type/argument.unexpected ERR-4",
            "m.hgl:9:17 type/argument.missing ERR-4",
            "m.hgl:10:38 name/argument.duplicate ERR-4",
            "m.hgl:11:38 name/argument.unknown ERR-4",
            "m.hgl:12:32 type/type.mismatch EXPR-5",
            "m.hgl:13:23 name/member.unknown ERR-4",
        ]
    );
    assert_eq!(
        locals(&checked, "made")[1..6],
        [
            "plain: error",
            "coded: error",
            "absent: error",
            "message: str",
            "code: str?",
        ]
    );
}

/// ERR-5 and ERR-15: a place asking for T raises an error in its
/// own phase. A value function that returns a result needs the same
/// failure type, and elsewhere only an `error` can be raised.
#[test]
fn a_result_raises_only_where_its_failure_type_is_admitted() {
    let checked = module(concat!(
        "const fn checked(value: i64) -> result<i64, error> => ok(value)\n",
        "const fn texted(value: i64) -> result<i64, str> => ok(value)\n",
        "const fn doubled(value: i64) -> result<f64, error> {\n",
        "    let price: i64 = checked(value)\n",
        "    return ok(2.0)\n",
        "}\n",
        "const fn crossed(value: i64) -> result<i64, error> {\n",
        "    return ok(texted(value))\n",
        "}\n",
        "const fn plain(value: i64) -> i64 => checked(value)\n",
        "fn node(value: i64) -> i64 {\n",
        "    start { let early: i64 = checked(1) }\n",
        "    when {\n",
        "        let texts: i64 = texted(value)\n",
        "        return checked(value)\n",
        "    }\n",
        "    stop { let late: i64 = checked(2) }\n",
        "}\n",
        "fn graph(value: result<i64, error>) -> i64 => value\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:9:15 type/result.propagation_type ERR-5",
            "m.hgl:15:26 type/result.propagation_type ERR-5",
        ]
    );
    assert_eq!(locals(&checked, "doubled"), ["value: i64", "price: i64"]);
}

/// The chapter's `examples.capture`, less its module line.
const CAPTURE: &str = concat!(
    "const fn checked(value: i64) -> result<i64, error> {\n",
    "    if value < 0 { return err(error(message: \"negative price\")) }\n",
    "    return ok(value)\n",
    "}\n",
    "fn required(value: i64) -> i64 {\n",
    "    when { return checked(value) }\n",
    "}\n",
    "fn forward(value: i64) -> i64 {\n",
    "    when { return value }\n",
    "}\n",
    "fn failed(failure: error, step: i64) -> bool {\n",
    "    when modified(step) && valid(step) { return modified(failure) }\n",
    "}\n",
    "fn held(failure: error, step: i64) -> bool {\n",
    "    when modified(step) && valid(step) { return valid(failure) }\n",
    "}\n",
    "fn captured_value(value: i64) -> i64 {\n",
    "    let seen = capture(required, value)\n",
    "    forward(seen.value)\n",
    "}\n",
    "fn captured_events(value: i64, step: i64) -> bool {\n",
    "    let seen = capture(required, value)\n",
    "    failed(seen.error, step)\n",
    "}\n",
    "fn captured_presence(value: i64, step: i64) -> bool {\n",
    "    let seen = capture(required, value)\n",
    "    held(seen.error, step)\n",
    "}\n",
);

/// ERR-7: "In composition, `capture(callable, args...)` creates a
/// temporal error boundary. It names a callable and gives it the
/// arguments its own call takes, positional or named, read, checked,
/// and selected as that call's are (EXPR-2, FN-4, GEN-5) ... What it
/// gives is bound with `let` and read through two connections: `.value`
/// retains the normal output type; outputless calls expose only
/// `.error`, a connection of `error`".
#[test]
fn a_capture_is_read_through_two_connections() {
    let checked = module(CAPTURE);
    assert_eq!(checked.summary(), NONE);
    assert_eq!(checked.prelude_names("m"), NONE);
    // `let seen = capture(required, value)`: the call it covers is
    // `required(value)`, the name left out of its arguments.
    let captured = body(&checked, "captured_value");
    let Some(Stmt::Declare {
        value: Some(seen), ..
    }) = captured.statements.first()
    else {
        panic!("`captured_value` begins with a `let`");
    };
    let ExprKind::Capture(call) = &seen.kind else {
        panic!("the `let` holds a capture, and this is {:?}", seen.kind);
    };
    let ExprKind::Call { arguments, .. } = &call.kind else {
        panic!("a capture covers a call");
    };
    assert_eq!(arguments.len(), 1, "`required` is given `value` alone");
    assert_eq!(call.ty.to_string(), "i64");
    // `forward(seen.value)`: the call's own output.
    let Some(Stmt::Return {
        value: Some(result),
        ..
    }) = captured.statements.last()
    else {
        panic!("`captured_value` ends in its result");
    };
    let ExprKind::Call { arguments, .. } = &result.kind else {
        panic!("the result is a call of `forward`");
    };
    assert!(matches!(
        arguments[0].value.kind,
        ExprKind::Captured { error: false, .. }
    ));
    assert_eq!(arguments[0].value.ty.to_string(), "i64");
}

/// A capture noted while an argument is tried against a candidate goes
/// when the trial is taken back: the local it was bound to is gone, and
/// its number is the next local's. ERR-7: `.value` is the capture's;
/// EXPR-7: `plain.count` is the struct's field.
#[test]
fn a_capture_in_a_trial_reading_marks_no_later_local() {
    let checked = module(concat!(
        "fn required(value: i64) -> i64 {\n",
        "    when { return value }\n",
        "}\n",
        "struct Counted { count: i64 }\n",
        "const fn choose<A, B>(a: A, b: B) -> A => a\n",
        "fn composed(value: i64) -> i64 {\n",
        "    choose(\n",
        "        {\n",
        "            let plain = Counted(count: 1)\n",
        "            plain.count\n",
        "        },\n",
        "        {\n",
        "            let seen = capture(required, value)\n",
        "            seen.value\n",
        "        },\n",
        "    )\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE);
}

/// ERR-7: "In a value function or a hook it rejects (`capture.phase`)
/// ... outputless calls expose only `.error`, a connection of `error`,
/// and `.value` rejects there (`capture.output_field`). Capture of
/// wiring-time scalar work rejects (`capture.phase`)."
#[test]
fn a_capture_covers_a_temporal_call_in_a_graph() {
    let checked = module(concat!(
        "const fn checked(value: i64) -> result<i64, error> => ok(value)\n",
        "fn required(value: i64) -> i64 {\n",
        "    when { return checked(value) }\n",
        "}\n",
        "fn sink(value: i64) {\n",
        "    when { }\n",
        "}\n",
        "fn failures(failure: error) -> bool {\n",
        "    when { return true }\n",
        "}\n",
        // An outputless call has an `.error` and no `.value`.
        "fn outputless(value: i64) -> bool {\n",
        "    let seen = capture(sink, value)\n",
        "    let wrong = seen.value\n",
        "    let odd = seen.banana\n",
        "    failures(seen.error)\n",
        "}\n",
        "fn composed(const fixed: i64, value: i64) -> bool {\n",
        // A value function given ordinary values is wiring-time work;
        // given a connection it is lifted, and may be captured.
        "    let wired = capture(checked, fixed)\n",
        "    let lifted = capture(checked, value)\n",
        // The call's own arguments, by name, and checked as its are.
        "    let named = capture(required, value: value)\n",
        "    let typed = capture(required, \"text\")\n",
        // No callable: nothing, an expression, a local, and a struct,
        // whose name constructs a value.
        "    let nothing = capture()\n",
        "    let unnamed = capture(1 + 2, value)\n",
        "    let local = capture(value, value)\n",
        "    let built = capture(Kept, value: 1)\n",
        // The call is held to what it is anywhere in a graph: a `const`
        // parameter is given no connection (FN-3).
        "    let limited = capture(bounded, value, value)\n",
        "    failures(named.error)\n",
        "}\n",
        "struct Kept { value: i64 }\n",
        "fn bounded(const limit: i64, value: i64) -> i64 {\n",
        "    when { return value }\n",
        "}\n",
        // A value function, and a hook.
        "const fn valued(value: i64) -> i64 {\n",
        "    let seen = capture(required, value)\n",
        "    return 0\n",
        "}\n",
        "fn hooked(value: i64) -> i64 {\n",
        "    when {\n",
        "        let seen = capture(required, value)\n",
        "        return value\n",
        "    }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:14:22 type/capture.output_field ERR-7",
            "m.hgl:15:20 name/member.unknown ERR-7",
            "m.hgl:19:17 phase/capture.phase ERR-7",
            "m.hgl:22:35 type/type.mismatch EXPR-5",
            "m.hgl:23:19 type/argument.missing ERR-7",
            "m.hgl:24:27 phase/capture.phase ERR-7",
            "m.hgl:25:25 phase/capture.phase ERR-7",
            "m.hgl:26:25 phase/capture.phase ERR-7",
            "m.hgl:27:36 phase/connection.as_constant FN-3",
            "m.hgl:35:16 phase/capture.phase ERR-7",
            "m.hgl:40:20 phase/capture.phase ERR-7",
        ]
    );
}

/// ERR-13: "`ok`, `err`, `is_ok`, `is_err`, and `capture` are contextual
/// names (LEX-10): as the callee of a call each is the form this
/// chapter defines, whatever else is in scope. No declaration, import
/// alias, parameter, or local takes one of them (UNIT-12); a member may
/// (LEX-9)."
#[test]
fn nothing_takes_the_name_of_a_result_form() {
    let checked = check(&[
        ("n.hgl", "module n\nexport const fn one() -> i64 => 1\n"),
        (
            "m.hgl",
            concat!(
                "module m\n",
                "use n as capture\n",
                "const fn is_ok(value: i64) -> i64 => value\n",
                "struct is_err { flag: bool }\n",
                "const fn takes(err: bool) -> bool {\n",
                "    let ok = 1\n",
                "    return err\n",
                "}\n",
                // A member may have the name, and is given by it.
                "struct Outcome {\n",
                "    ok: bool\n",
                "    err: str\n",
                "}\n",
                "const fn reads(outcome: Outcome) -> bool => outcome.ok\n",
                "const fn makes() -> Outcome => Outcome(ok: true, err: \"\")\n",
            ),
        ),
    ]);
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:2:10 name/name.duplicate ERR-13",
            "m.hgl:3:10 name/name.duplicate ERR-13",
            "m.hgl:4:8 name/name.duplicate ERR-13",
            "m.hgl:5:16 name/name.duplicate ERR-13",
            "m.hgl:6:9 name/name.duplicate ERR-13",
        ]
    );
}

/// ERR-5, ERR-12, ERR-15: an inferred local keeps its result. A place
/// that expects the result keeps it too. A place expecting the value or
/// its nullable form records which conversion it requires.
#[test]
fn value_code_records_each_form_of_a_result() {
    use hgl_analyser::hir::{NULL_READ_CODE, Raise, walk_statements};

    let checked = module(concat!(
        "const fn checked(value: i64) -> result<i64, error> => ok(value)\n",
        "const fn forms(value: i64, nullable: i64?) -> result<i64, error> {\n",
        "    let held = checked(value)\n",
        "    let whole: result<i64, error> = held\n",
        "    let raised: i64 = held\n",
        "    let optional: i64? = held\n",
        "    let read: i64 = nullable\n",
        "    return ok(raised + read)\n",
        "}\n",
        "const fn nullable_value(value: i64?) -> result<i64?, error> => ok(value)\n",
        "const fn take_nullable(value: i64?) -> i64? => nullable_value(value)\n",
    ));
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
    assert_eq!(
        locals(&checked, "forms"),
        [
            "value: i64",
            "nullable: i64?",
            "held: result<i64, error>",
            "whole: result<i64, error>",
            "raised: i64",
            "optional: i64?",
            "read: i64",
        ],
    );
    let mut observed = Vec::new();
    walk_statements(
        &body(&checked, "forms").statements,
        &mut |expr| match expr.kind {
            ExprKind::Raised {
                raise: Raise::Return,
                ..
            } => observed.push("value"),
            ExprKind::OrNull { reader: false, .. } => observed.push("nullable"),
            ExprKind::NullRead {
                raise: Raise::Return,
                code: NULL_READ_CODE,
                ..
            } => {
                observed.push("null read");
            }
            _ => {}
        },
    );
    assert_eq!(observed, ["value", "nullable", "null read"]);

    let mut raised = 0;
    walk_statements(&body(&checked, "take_nullable").statements, &mut |expr| {
        if matches!(
            expr.kind,
            ExprKind::Raised {
                raise: Raise::Execution,
                ..
            }
        ) {
            assert_eq!(expr.ty.to_string(), "i64?");
            raised += 1;
        }
    });
    assert_eq!(raised, 1);
}

/// ERR-12 and ERR-15: the lifted call can publish its result unchanged,
/// raise at its own node, or publish null on failure. A held connection's
/// conversions and a nullable connection read happen at the reader.
#[test]
fn composition_records_each_form_of_a_result() {
    use hgl_analyser::hir::{NULL_READ_CODE, Raise, ValueCategory, walk_statements};

    let checked = module(concat!(
        "const fn checked(value: i64) -> result<i64, error> => ok(value)\n",
        "fn forms(value: i64, nullable: i64?) -> i64 {\n",
        "    let whole: result<i64, error> = checked(value)\n",
        "    let raised: i64 = checked(value)\n",
        "    let optional: i64? = checked(value)\n",
        "    let held_raised: i64 = whole\n",
        "    let held_optional: i64? = whole\n",
        "    let read: i64 = nullable\n",
        "    return raised\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
    let mut observed = Vec::new();
    walk_statements(&body(&checked, "forms").statements, &mut |expr| {
        let form = match expr.kind {
            ExprKind::Raised {
                raise: Raise::CallNode,
                ..
            } => "call value",
            ExprKind::Raised {
                raise: Raise::Reader,
                ..
            } => "held value",
            ExprKind::OrNull { reader: false, .. } => "call nullable",
            ExprKind::OrNull { reader: true, .. } => "held nullable",
            ExprKind::NullRead {
                raise: Raise::Reader,
                code: NULL_READ_CODE,
                ..
            } => "null read",
            _ => return,
        };
        assert_eq!(expr.category, ValueCategory::Connection);
        observed.push(form);
    });
    assert_eq!(
        observed,
        [
            "call value",
            "call nullable",
            "held value",
            "held nullable",
            "null read"
        ]
    );
}

/// ERR-15 and ERR-12: an operator's places are independent. An
/// argument can keep its result while another takes the result's
/// value or reads a nullable payload.
#[test]
fn operators_fit_each_argument_to_its_selected_parameter() {
    use hgl_analyser::hir::{Raise, walk_statements};

    let checked = module(concat!(
        "operator take<A, B>(keep: A, read: B) -> i64\n",
        "impl const fn take(keep: result<i64, error>, read: i64) -> i64 => read\n",
        "operator take_items<A, B>(keep: A, read: B) -> i64\n",
        "impl const fn take_items<T>(keep: result<i64, error>, read: list<T>) -> i64 => 1\n",
        "const fn read_result(keep: result<i64, error>, read: result<i64, error>) -> i64 => take(keep, read)\n",
        "const fn read_nullable(keep: result<i64, error>, read: i64?) -> i64 => take(keep: keep, read: read)\n",
        "const fn read_items(keep: result<i64, error>, read: result<list<i64>, error>) -> i64 => take_items(keep, read)\n",
    ));
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
    for (name, nullable) in [
        ("read_result", false),
        ("read_nullable", true),
        ("read_items", false),
    ] {
        let mut calls = 0;
        walk_statements(&body(&checked, name).statements, &mut |expr| {
            if let ExprKind::Call { arguments, .. } = &expr.kind {
                assert!(matches!(arguments[0].value.kind, ExprKind::Local(_)));
                let converted = &arguments[1].value.kind;
                if nullable {
                    assert!(matches!(
                        converted,
                        ExprKind::NullRead {
                            raise: Raise::Execution,
                            ..
                        }
                    ));
                } else {
                    assert!(matches!(
                        converted,
                        ExprKind::Raised {
                            raise: Raise::Execution,
                            ..
                        }
                    ));
                }
                calls += 1;
            }
        });
        assert_eq!(calls, 1, "{name}");
    }
}

/// ERR-15: among admitted candidates, the fewest result reads take
/// precedence. Equal read counts use ordinary candidate ranking.
#[test]
fn operators_rank_admitted_candidates_by_result_reads() {
    use hgl_analyser::hir::walk_statements;

    let checked = module(concat!(
        "operator two<A, B>(x: A, y: B) -> i64\n",
        "impl const fn two(x: result<i64, error>, y: i64) -> i64 => 1\n",
        "impl const fn two(x: i64, y: i64) -> i64 => 3\n",
        "const fn choose(x: result<i64, error>, y: result<i64, error>) -> i64 => two(x, y)\n",
    ));
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
    let mut selected = Vec::new();
    walk_statements(&body(&checked, "choose").statements, &mut |expr| {
        if let ExprKind::Call { arguments, .. } = &expr.kind {
            selected.push((arguments[0].value.ty.clone(), arguments[1].value.ty.clone()));
        }
    });
    assert_eq!(selected.len(), 1);
    assert!(matches!(selected[0].0, hgl_analyser::Type::Result { .. }));
    assert_eq!(
        selected[0].1,
        hgl_analyser::Type::Scalar(hgl_analyser::Scalar::I64)
    );

    let tied = module(concat!(
        "operator two<A, B>(x: A, y: B) -> i64\n",
        "impl const fn two(x: result<i64, error>, y: i64) -> i64 => 1\n",
        "impl const fn two(x: i64, y: result<i64, error>) -> i64 => 2\n",
        "const fn choose(x: result<i64, error>, y: result<i64, error>) -> i64 => two(x, y)\n",
    ));
    let faults = tied.summary();
    assert_eq!(faults.len(), 1, "{}", tied.report);
    assert!(faults[0].contains("operator/operator.ambiguous"));
}

/// ERR-15: a candidate admitted by payload inference competes with a
/// concrete candidate. GEN-5 applies only after comparing read counts.
#[test]
fn an_inferred_payload_candidate_can_read_fewer_results() {
    use hgl_analyser::hir::walk_statements;

    let checked = module(concat!(
        "operator two<A, B>(x: A, y: B) -> i64\n",
        "impl const fn two<P>(x: result<i64, error>, y: P) -> i64 requires P == i64 => 1\n",
        "impl const fn two(x: i64, y: i64) -> i64 => 3\n",
        "const fn choose(x: result<i64, error>, y: result<i64, error>) -> i64 => two(x, y)\n",
    ));
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
    let mut kept_whole = false;
    walk_statements(&body(&checked, "choose").statements, &mut |expr| {
        if let ExprKind::Call { arguments, .. } = &expr.kind {
            kept_whole = matches!(arguments[0].value.ty, hgl_analyser::Type::Result { .. });
        }
    });
    assert!(kept_whole);
}

/// ERR-12, ERR-15: an operand kept whole leaves nullable inference at
/// the first pass that admits a candidate without reading any result.
#[test]
fn a_whole_result_does_not_extend_nullable_fallback() {
    use hgl_analyser::hir::walk_statements;

    let checked = module(concat!(
        "operator choose<A, B, C>(kept: A, used: B, extra: C) -> i64\n",
        "impl const fn choose<A, C>(kept: A, used: i64, extra: C) -> i64 => 1\n",
        "impl const fn choose<C>(kept: result<i64, error>, used: i64, extra: C) -> i64 requires C == i64 => 2\n",
        "const fn chosen(kept: result<i64, error>, used: i64?, extra: i64?) -> i64 => choose(kept, used, extra)\n",
    ));
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
    let mut held = false;
    walk_statements(&body(&checked, "chosen").statements, &mut |expr| {
        if let ExprKind::Call { arguments, .. } = &expr.kind {
            held = matches!(arguments[2].value.ty, hgl_analyser::Type::Nullable(_));
        }
    });
    assert!(held);
}

/// GEN-2, ERR-12 and ERR-15: a generic function sees the argument its
/// parameter can take. An unconstrained parameter keeps the whole type.
#[test]
fn generic_functions_infer_from_the_accepted_argument_form() {
    use hgl_analyser::Argument;
    use hgl_analyser::hir::{Raise, walk_statements};

    let checked = module(concat!(
        "const fn count<T>(items: list<T>) -> i64 => 1\n",
        "const fn accept<T>(item: T) -> i64 => 1\n",
        "const fn from_result(items: result<list<i64>, error>) -> i64 => count(items)\n",
        "const fn from_nullable(items: list<i64>?) -> i64 => count(items)\n",
        "const fn from_both(items: result<list<i64>?, error>) -> i64 => count(items)\n",
        "const fn from_whole(items: result<list<i64>, error>) -> i64 => accept(items)\n",
    ));
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
    for (name, expected, form) in [
        ("from_result", "i64", "raised"),
        ("from_nullable", "i64", "read"),
        ("from_both", "i64", "raised and read"),
        ("from_whole", "result<list<i64>, error>", "whole"),
    ] {
        let mut calls = 0;
        walk_statements(&body(&checked, name).statements, &mut |expr| {
            if let ExprKind::Call {
                callee, arguments, ..
            } = &expr.kind
            {
                let [Argument::Type(ty)] = callee.generics() else {
                    panic!("{name} has one resolved type argument");
                };
                assert_eq!(ty.to_string(), expected);
                let given = match &arguments[0].value.kind {
                    ExprKind::Raised {
                        raise: Raise::Execution,
                        ..
                    } => "raised",
                    ExprKind::NullRead {
                        value,
                        raise: Raise::Execution,
                        ..
                    } => match &value.kind {
                        ExprKind::Raised {
                            raise: Raise::Execution,
                            ..
                        } => "raised and read",
                        _ => "read",
                    },
                    ExprKind::Local(_) => "whole",
                    kind => panic!("unexpected form in {name}: {kind:?}"),
                };
                assert_eq!(given, form);
                calls += 1;
            }
        });
        assert_eq!(calls, 1, "{name}");
    }
}

/// ERR-12, ERR-15: indexing or iterating a fallible nullable collection
/// first takes the success value, then reads the nullable collection.
#[test]
fn indexing_and_iteration_read_nullable_success_payloads() {
    use hgl_analyser::hir::{NULL_READ_CODE, Raise, walk_statements};

    let checked = module(concat!(
        "const fn head(items: result<list<i64>?, error>) -> i64 => items[0]\n",
        "const fn sum(items: result<list<i64>?, error>) -> i64 {\n",
        "    var total = 0\n",
        "    for item in items { total += item }\n",
        "    return total\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
    for name in ["head", "sum"] {
        let mut reads = 0;
        walk_statements(&body(&checked, name).statements, &mut |expr| {
            if let ExprKind::NullRead { value, raise, code } = &expr.kind {
                assert_eq!(*code, NULL_READ_CODE);
                assert_eq!(*raise, Raise::Execution);
                assert!(matches!(
                    value.kind,
                    ExprKind::Raised {
                        raise: Raise::Execution,
                        ..
                    }
                ));
                assert_eq!(expr.ty.to_string(), "list<i64>");
                reads += 1;
            }
        });
        assert_eq!(reads, 1, "{name}");
    }
}

/// ERR-15 with ERR-12 (hhenson/hgl-compiler#345): a non-nullable place
/// takes a result's success value, then reads it. CTRL-10 and CTRL-13
/// require the same two reads for a fallible nullable condition.
#[test]
fn typed_places_and_conditions_read_nullable_success_payloads() {
    use hgl_analyser::hir::{NULL_READ_CODE, Raise, walk_statements};

    let checked = module(concat!(
        "const fn required(flag: result<bool?, error>) -> i64 {\n",
        "    if flag { return 1 }\n",
        "    return 0\n",
        "}\n",
        "fn choose(flag: result<bool?, error>) -> i64 => if flag { 1 } else { 0 }\n",
        "const fn read_value(outcome: result<i64?, error>) -> i64 => outcome\n",
        "const fn checked(value: i64) -> result<i64?, error> => ok(value)\n",
        "fn read_connection(value: i64) -> i64 => checked(value)\n",
    ));
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
    for (name, ty, read_phase, result_phase) in [
        ("required", "bool", Raise::Execution, Raise::Execution),
        ("choose", "bool", Raise::Reader, Raise::Reader),
        ("read_value", "i64", Raise::Execution, Raise::Execution),
        ("read_connection", "i64", Raise::Reader, Raise::CallNode),
    ] {
        let mut reads = 0;
        walk_statements(&body(&checked, name).statements, &mut |expr| {
            if let ExprKind::NullRead { value, raise, code } = &expr.kind {
                assert_eq!(*code, NULL_READ_CODE);
                assert_eq!(*raise, read_phase);
                assert!(
                    matches!(value.kind, ExprKind::Raised { raise, .. } if raise == result_phase)
                );
                assert_eq!(expr.ty.to_string(), ty);
                reads += 1;
            }
        });
        assert_eq!(reads, 1, "{name}");
    }
}

/// ERR-15: when one candidate fits with either operand's payload, the
/// two arrangements are ambiguous. When only one fits, take that one.
#[test]
fn candidate_cannot_choose_between_equally_few_result_reads() {
    let checked = module(concat!(
        "operator read_one<X, Y>(left: X, right: Y) -> i64
",
        "impl const fn read_one<T, U>(left: T, right: U) -> i64
",
        "    requires (T in {result<i64, error>} && U in {i64})
",
        "          || (T in {i64} && U in {result<i64, error>}) => 0
",
        "const fn ambiguous(l: result<i64, error>, r: result<i64, error>) -> i64 => read_one(l, r)
",
        "const fn read_left(l: result<i64, error>) -> i64 => read_one(l, 3)
",
        "const fn read_right(r: result<i64, error>) -> i64 => read_one(3, r)
",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:6:76 operator/operator.ambiguous ERR-15"],
        "{}",
        checked.report,
    );
}

/// ERR-15: a whole result wins over taking its value. Among non-whole
/// arrangements, taking one value wins over taking two.
#[test]
fn candidate_uses_the_arrangement_with_fewest_result_reads() {
    let checked = module(concat!(
        "operator keep<X>(item: X) -> i64
",
        "impl const fn keep<T>(item: T) -> i64 requires T in {i64, result<i64, error>} => 0
",
        "operator prefer<X, Y, Z>(a: X, b: Y, c: Z) -> i64
",
        "impl const fn prefer<T, U, V>(a: T, b: U, c: V) -> i64
",
        "    requires (T in {i64} && U in {result<i64, error>} && V in {result<i64, error>})
",
        "          || (T in {result<i64, error>} && U in {i64} && V in {i64}) => 0
",
        "const fn whole(x: result<i64, error>) -> i64 => keep(x)
",
        "const fn one(a: result<i64, error>, b: result<i64, error>, c: result<i64, error>) -> i64
",
        "    => prefer(a, b, c)
",
    ));
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
    for (name, raised) in [("whole", &[false][..]), ("one", &[true, false, false][..])] {
        let [
            Stmt::Return {
                value: Some(call), ..
            },
        ] = body(&checked, name).statements.as_slice()
        else {
            panic!("expected a return of an operator call");
        };
        let ExprKind::Call { arguments, .. } = &call.kind else {
            panic!("expected an operator call, got {:?}", call.kind);
        };
        let reads: Vec<bool> = arguments
            .iter()
            .map(|arg| matches!(arg.value.kind, ExprKind::Raised { .. }))
            .collect();
        assert_eq!(reads, raised, "{name}");
    }
}
