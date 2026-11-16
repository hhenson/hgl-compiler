//! The analyser against the rules on results and errors: the
//! constructors, the tag tests and the facts they give, an error's
//! parts, and propagation. Each test cites its rule.
//!
//! A finding is written `file:line:column category/code rule`.

mod common;

use common::{Checked, check};
use hgl_analyser::hir::{Body, ExprKind, Stmt};

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

/// The locals of `name`, as `local: type`.
fn locals(checked: &Checked, name: &str) -> Vec<String> {
    body(checked, name)
        .locals
        .iter()
        .map(|local| format!("{}: {}", local.name, local.ty))
        .collect()
}

/// The chapter's `examples.results`, less its module line.
const RESULTS: &str = concat!(
    "const fn checked(value: i64) -> result<i64, error> {\n",
    "    if value < 0 {\n",
    "        return err(error(message: \"negative price\", code: \"price.negative\"))\n",
    "    }\n",
    "    return ok(value)\n",
    "}\n",
    "const fn doubled(value: i64) -> result<i64, error> {\n",
    "    let price = checked(value)?\n",
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
    "    when { return checked(value)? }\n",
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
        "`?` gives the T of a success"
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
            "m.hgl:31:43 name/member.unknown ERR-3",
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

/// ERR-5: "Success yields T. In a value function returning
/// `result<U,E>`, failure returns the same E as Err. In when, only
/// `result<T,error>` may propagate to node failure ... Composition,
/// lifecycle, and generator uses reject (`result.propagation_phase`),
/// and an operand or an enclosing result of another type rejects
/// (`result.propagation_type`) ... It does not unwrap null (ERR-12)."
#[test]
fn propagation_needs_a_result_and_a_place_to_leave_by() {
    let checked = module(concat!(
        "const fn checked(value: i64) -> result<i64, error> => ok(value)\n",
        "const fn texted(value: i64) -> result<i64, str> => ok(value)\n",
        // The same E, whatever the T.
        "const fn doubled(value: i64) -> result<f64, error> {\n",
        "    let price = checked(value)?\n",
        "    return ok(2.0)\n",
        "}\n",
        // An enclosing result of another E, and no result at all.
        "const fn crossed(value: i64) -> result<i64, error> {\n",
        "    return ok(texted(value)?)\n",
        "}\n",
        "const fn plain(value: i64) -> i64 {\n",
        "    return checked(value)?\n",
        "}\n",
        // What is no result, a nullable included.
        "const fn numbers(value: i64, maybe: i64?) -> result<i64, error> {\n",
        "    let one = value?\n",
        "    let two = maybe?\n",
        "    return ok(0)\n",
        "}\n",
        "fn node(value: i64) -> i64 {\n",
        "    inject out\n",
        "    start { let early = checked(1)? }\n",
        "    when {\n",
        "        let texts = texted(value)?\n",
        "        return checked(value)?\n",
        "    }\n",
        "    stop { let late = checked(2)? }\n",
        "}\n",
        "fn graph(value: result<i64, error>) -> i64 => value?\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:9:28 type/result.propagation_type ERR-5",
            "m.hgl:12:26 type/result.propagation_type ERR-5",
            "m.hgl:15:20 type/result.propagation_type ERR-5",
            "m.hgl:16:20 type/result.propagation_type ERR-5",
            "m.hgl:21:35 phase/result.propagation_phase ERR-5",
            "m.hgl:23:34 type/result.propagation_type ERR-5",
            "m.hgl:26:33 phase/result.propagation_phase ERR-5",
            "m.hgl:28:52 phase/result.propagation_phase ERR-5",
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
    "    when { return checked(value)? }\n",
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

/// ERR-7: "In a value function or a hook it rejects (`capture.phase`)
/// ... outputless calls expose only `.error`, a connection of `error`,
/// and `.value` rejects there (`capture.output_field`). Capture of
/// wiring-time scalar work rejects (`capture.phase`)."
#[test]
fn a_capture_covers_a_temporal_call_in_a_graph() {
    let checked = module(concat!(
        "const fn checked(value: i64) -> result<i64, error> => ok(value)\n",
        "fn required(value: i64) -> i64 {\n",
        "    when { return checked(value)? }\n",
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
        // No callable: nothing, an expression, and a local.
        "    let nothing = capture()\n",
        "    let unnamed = capture(1 + 2, value)\n",
        "    let local = capture(value, value)\n",
        "    failures(named.error)\n",
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
            "m.hgl:29:16 phase/capture.phase ERR-7",
            "m.hgl:34:20 phase/capture.phase ERR-7",
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
