//! The analyser against the rules on a node's hooks. Each test cites
//! its rule.
//!
//! A finding is written `file:line:column category/code rule`.

mod common;

use common::{Checked, check};
use hgl_analyser::hir::{Body, EndpointOperation, ExprKind, Hook, Stmt};

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

/// FN-3: "In a hook a temporal parameter is a value of its declared
/// type, read as in a value function". FN-2: a node's body is its
/// declarations and then its hooks. TIME-9: "The operations on an
/// endpoint are intrinsics of the language ... each takes a temporal
/// input of the enclosing function by its name, or `out`". TIME-7: "`out`
/// is read in a `when` and in `stop`, and written only in a `when`".
/// FN-7: a `return` with no value "ends the evaluation and publishes
/// nothing itself".
#[test]
fn a_hook_is_read_as_value_code() {
    let checked = module(concat!(
        "fn counted(value: i64, const step: i64) -> i64 {\n",
        "    state seen: i64 = 0\n",
        "    cache scale: i64 = step * 2\n",
        "    inject out, scheduler\n",
        "    start { }\n",
        "    when modified() && valid(value) {\n",
        "        seen += value * scale\n",
        "        if seen > 100 {\n",
        "            invalidate(out)\n",
        "            return\n",
        "        }\n",
        "        let published = delta_value(value)\n",
        "        let when_last = last_modified(value)\n",
        "        out = seen\n",
        "    }\n",
        "    when scheduled() && valid([]) { return seen }\n",
        "    stop { let last = out }\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE);
    let body = body(&checked, "counted");
    let locals: Vec<String> = body
        .locals
        .iter()
        .map(|local| format!("{}: {}", local.name, local.ty))
        .collect();
    assert_eq!(
        locals,
        [
            "value: i64",
            "step: i64",
            "seen: i64",
            "scale: i64",
            "out: i64",
            "scheduler: _",
            "published: i64",
            "when_last: datetime?",
            "last: i64",
        ]
    );
    // Its declarations and then its hooks.
    let shape: Vec<String> = body
        .statements
        .iter()
        .map(|statement| match statement {
            Stmt::Declare { local, .. } => format!("declare {}", body.locals[*local].name),
            Stmt::Inject { local, .. } => format!("inject {}", body.locals[*local].name),
            Stmt::Hook { hook, .. } => hook.as_str().to_string(),
            other => panic!("a declaration or a hook: {other:?}"),
        })
        .collect();
    assert_eq!(
        shape,
        [
            "declare seen",
            "declare scale",
            "inject out",
            "inject scheduler",
            "start",
            "when",
            "when",
            "stop",
        ]
    );
    // `valid([])`: the endpoints named, here none.
    let Stmt::Hook {
        hook: Hook::When,
        condition: Some(condition),
        ..
    } = &body.statements[6]
    else {
        panic!("a `when` with a condition");
    };
    let (_, _, right) = condition.kind.binary().expect("`&&`");
    assert_eq!(
        right.kind,
        ExprKind::Endpoint {
            operation: EndpointOperation::Valid,
            inputs: Some(Vec::new()),
        }
    );
}

/// TIME-9: an endpoint operation "takes a temporal input of the
/// enclosing function by its name, or `out`". TIME-7: "`start` neither
/// reads nor writes it, and a `return` with a value stands in neither
/// `start` nor `stop`." FN-3: "A runtime hook can call value helpers but
/// cannot silently construct a temporal child call."
#[test]
fn a_hook_rejects_what_its_phase_has_not() {
    let checked = module(concat!(
        "fn doubled(value: i64) -> i64 => value\n",
        "fn bad(value: i64, other: f64, const step: i64) -> i64 {\n",
        "    state seen: i64 = 0\n",
        "    inject out\n",
        "    start {\n",
        "        let first = out\n",
        "        return 1\n",
        "    }\n",
        // A state, a `const` parameter and an expression are no
        // endpoints.
        "    when valid(seen) && modified(step) {\n",
        "        let late = delta_value(value + 1)\n",
        // A sequence is a selector's form: `delta_value` takes one
        // endpoint by its name.
        "        let both = delta_value([value, other])\n",
        "        let none = last_modified([])\n",
        "        return other\n",
        "    }\n",
        // A `yield` in an anonymous function is that function's: the
        // node around it is no source, and is read all the same.
        "    when {\n",
        "        let helper = fn() { yield 0s: 1 }\n",
        "        return doubled(value)\n",
        "    }\n",
        "    stop {\n",
        "        out = 2\n",
        "        invalidate(out)\n",
        "        let last = valid(out)\n",
        "    }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:7:21 phase/out.lifecycle TIME-7",
            "m.hgl:8:16 phase/out.lifecycle TIME-7",
            "m.hgl:10:16 phase/endpoint.argument TIME-9",
            "m.hgl:10:34 phase/endpoint.argument TIME-9",
            "m.hgl:11:32 phase/endpoint.argument TIME-9",
            "m.hgl:12:32 phase/endpoint.argument TIME-9",
            "m.hgl:13:34 phase/endpoint.argument TIME-9",
            "m.hgl:14:16 type/type.mismatch EXPR-5",
            "m.hgl:18:16 function-kind/call.role FN-3",
            "m.hgl:21:9 phase/out.lifecycle TIME-7",
            "m.hgl:22:9 phase/out.lifecycle TIME-7",
        ]
    );
}

/// TIME-3: "`valid()` requires all temporal inputs to be valid;
/// `valid(a, b)` requires both named inputs, and similarly for more
/// inputs. Omitting either selector uses the same default as its
/// zero-argument form. `valid([])` requires no inputs to be valid. A
/// selector sets
/// subscription or admission when it stands alone in the header or
/// appears as a top-level conjunct. Selectors under negation or
/// disjunction leave the default in place." TIME-4: "A payload read
/// under manual admission must be dominated by a successful guard. A
/// guard is a test of `valid(input)`, on the input by its name, that
/// holds where the read stands: in the branch it holds for, in the
/// right operand of the `&&` it guards, and after a branch that
/// returns".
#[test]
fn a_payload_is_read_where_its_input_is_admitted_or_guarded() {
    let checked = module(concat!(
        // The temporal chapter's `manual_forward`, with a second input.
        "fn manual(value: i64, other: i64) -> i64 {\n",
        "    inject out\n",
        "    when modified() && valid([]) {\n",
        "        if valid(value) { return value }\n",
        "        if !valid(other) {\n",
        "            invalidate(out)\n",
        "            return\n",
        "        }\n",
        "        return other\n",
        "    }\n",
        "}\n",
        // The default: every input is valid.
        "fn defaulted(value: i64, other: i64) -> i64 {\n",
        "    when modified(value) { return value + other }\n",
        "}\n",
        // A selector that names its inputs admits those.
        "fn named(a: i64, b: i64, c: i64) -> i64 {\n",
        "    when valid(a, b) && a > 0 { return a + b }\n",
        "}\n",
        // Under a negation the default stays in place.
        "fn negated(value: i64, flag: bool) -> i64 {\n",
        "    when !valid(flag) || value > 0 { return value }\n",
        "}\n",
        // A top-level selector admits its input for the whole header,
        // whatever order the conjuncts are written in.
        "fn ordered(value: i64) -> i64 {\n",
        "    when value > 0 && valid(value) { return value }\n",
        "}\n",
        // A name a declaration binds is a call of it, and no selector:
        // the default stays in place.
        "const fn ready() -> bool => true\n",
        "fn helped(value: i64) -> i64 {\n",
        "    when ready() { return value }\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE);
}

/// TIME-9: a selector is the language's `valid`, the name nothing
/// binds. Where a module declares the name, a call of it is a call of
/// that declaration: it sets no admission, and the default stands.
/// (TIME-9 also has the name not taken by a declaration, which is not
/// reported yet: only the reads are looked at here.)
#[test]
fn a_declared_name_is_a_call_and_sets_no_admission() {
    let checked = module(concat!(
        "const fn valid() -> bool => true\n",
        "fn helped(value: i64) -> i64 {\n",
        "    when valid() { return value }\n",
        "}\n",
    ));
    let unguarded: Vec<String> = checked
        .summary()
        .into_iter()
        .filter(|line| line.contains("payload.unguarded"))
        .collect();
    assert_eq!(unguarded, NONE);
}

/// TIME-4: an unguarded payload read under manual admission rejects. "A
/// saved bool proves nothing." TIME-5: "State initializers cannot read
/// current temporal inputs."
#[test]
fn an_unguarded_payload_read_is_rejected() {
    let checked = module(concat!(
        "fn unguarded(value: i64) -> i64 {\n",
        "    when valid([]) { return value + 1 }\n",
        "}\n",
        // `b` is not among those the header admits.
        "fn partial(a: i64, b: i64) -> i64 {\n",
        "    when valid(a) { return a + b }\n",
        "}\n",
        "fn saved(value: i64) -> i64 {\n",
        "    when valid([]) {\n",
        "        let ok = valid(value)\n",
        "        if ok { return value }\n",
        "        return 0\n",
        "    }\n",
        "}\n",
        // The header's own condition is read under its admission.
        "fn header(value: i64) -> i64 {\n",
        "    when valid([]) && value > 0 { return 1 }\n",
        "}\n",
        "fn seeded(value: i64, const first: i64) -> i64 {\n",
        "    state kept: i64 = first\n",
        "    state seen: i64 = value\n",
        "    when { return seen + kept }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:29 phase/payload.unguarded TIME-4",
            "m.hgl:6:32 phase/payload.unguarded TIME-4",
            "m.hgl:11:24 phase/payload.unguarded TIME-4",
            "m.hgl:16:23 phase/payload.unguarded TIME-4",
            "m.hgl:20:23 phase/state.input_read TIME-5",
        ]
    );
}

/// TIME-3: "`modified(a, b)` subscribes to both named inputs, and
/// similarly for more inputs ... `valid(a, b)` requires both named
/// inputs, and similarly for more inputs ... `valid([])` requires no
/// inputs to be valid." TIME-9: an endpoint operation takes a temporal
/// input "by its name". So a sequence that names inputs is no form of a
/// selector. TIME-4: an input the selector does not name needs its
/// guard, and a guard names several as a header does.
#[test]
fn a_selector_names_its_inputs_each_by_its_name() {
    let checked = module(concat!(
        "fn three(a: i64, b: i64, c: i64) -> i64 {\n",
        "    when valid(a, b, c) { return a + b + c }\n",
        "    when valid(a, b) {\n",
        // `c` is not among those named.
        "        return a + c\n",
        "    }\n",
        "    when valid([]) {\n",
        "        if valid(a, b) { return a + b }\n",
        "    }\n",
        // A subscription to the inputs named: validity is the default,
        // every input.
        "    when modified(a, b) { return a + c }\n",
        // Forms the chapter does not have: a sequence that names
        // inputs.
        "    when valid([a, b]) { return a }\n",
        "    when modified([a]) { return a }\n",
        // A constant, and a name given twice, among those named.
        "    when valid(a, 1) { return a }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:5:20 phase/payload.unguarded TIME-4",
            "m.hgl:11:16 phase/endpoint.argument TIME-9",
            "m.hgl:12:19 phase/endpoint.argument TIME-9",
            "m.hgl:13:19 phase/endpoint.argument TIME-9",
        ]
    );
}

/// TIME-3: "`modified([])` subscribes to no input: it stands only in a
/// node that injects `scheduler` or `alarm` and schedules a wake on it
/// in `start`, which is then what wakes the handler."
#[test]
fn an_empty_subscription_needs_what_wakes_it() {
    let checked = module(concat!(
        // The chapter's `sampled`.
        "fn sampled(a: f64, const every: duration = 1s) -> f64 {\n",
        "    inject scheduler\n",
        "    start { schedule(scheduler, every) }\n",
        "    when modified([]) && valid([]) {\n",
        "        schedule(scheduler, every)\n",
        "        if valid(a) { return a }\n",
        "    }\n",
        "}\n",
        // In a branch of `start`, and in parentheses, which only group
        // (EXPR-1).
        "fn alarmed(a: f64) -> f64 {\n",
        "    inject alarm\n",
        "    start {\n",
        "        if true { schedule((alarm), 1s) }\n",
        "    }\n",
        "    when modified([]) { return a }\n",
        "}\n",
        // Through a local the scheduler is bound to (TIME-8).
        "fn renamed(a: f64) -> f64 {\n",
        "    inject scheduler\n",
        "    start {\n",
        "        let timer = (scheduler)\n",
        "        let again = timer\n",
        "        schedule(again, 1s)\n",
        "    }\n",
        "    when modified([]) { return a }\n",
        "}\n",
        // Nothing injected that wakes it.
        "fn dormant(a: f64) -> f64 {\n",
        "    when modified([]) && valid([]) { return 0.0 }\n",
        "}\n",
        // Injected, and no `start`.
        "fn unstarted(a: f64) -> f64 {\n",
        "    inject scheduler\n",
        "    when modified([]) { return a }\n",
        "}\n",
        // A `start` that schedules nothing, and a wake asked for only
        // in the handler it would wake.
        "fn idle(a: f64) -> f64 {\n",
        "    inject scheduler, logger\n",
        "    start { log_info(logger, \"up\") }\n",
        "    when modified([]) {\n",
        "        schedule(scheduler, 1s)\n",
        "        return a\n",
        "    }\n",
        "}\n",
        // A wake on what is no capability of the node.
        "fn misled(a: f64, const other: duration = 1s) -> f64 {\n",
        "    inject alarm\n",
        "    start { schedule(other, 1s) }\n",
        "    when modified([]) { return a }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:27:10 phase/modified.no_driver TIME-3",
            "m.hgl:31:10 phase/modified.no_driver TIME-3",
            "m.hgl:36:10 phase/modified.no_driver TIME-3",
            "m.hgl:44:10 phase/modified.no_driver TIME-3",
        ]
    );
}

/// TIME-8: "A capability is used within the node that injects it and is
/// not retained: it is not the value of a `state` or a `cache`, an entry
/// of a tuple, list, set, dictionary, or struct, what a function
/// returns, what a node publishes, or a capture of an anonymous function
/// (FN-14). Binding one to a `let` and giving one to a parameter of its
/// type are uses." TIME-7: `out` is the node's result, read as a value.
#[test]
fn a_capability_is_used_and_not_retained() {
    let checked = module(concat!(
        "struct Kept { value: i64 }\n",
        "const fn stamped(at: datetime, value: i64) -> i64 => value\n",
        "fn uses(value: i64) -> i64 {\n",
        "    state held: i64 = 0\n",
        "    inject clock, logger, scheduler, out\n",
        "    start { schedule(scheduler, 0s) }\n",
        "    when {\n",
        // Uses: a property, an argument, a `let`.
        "        log_info(logger, \"tick\")\n",
        "        let now = (clock).evaluation_time\n",
        "        let last = stamped(clock.evaluation_time, out)\n",
        "        let bound = clock\n",
        "        let count = get<i64>(logger, \"count\")\n",
        // Retained: in a tuple, a list, a struct, a `state`, and what
        // is published.
        "        let pair = (scheduler, value)\n",
        "        let listed = [clock]\n",
        "        let entry = Kept(value: logger)\n",
        "        held = clock\n",
        "        out = scheduler\n",
        // A local a capability is bound to is that capability: the
        // pinned core gives none a type, and it is one all the same.
        "        let again = bound\n",
        "        let stored = [again]\n",
        "        var moving = if value > 0 { clock } else { bound }\n",
        "        let moved = (moving, 1)\n",
        "        return clock\n",
        "    }\n",
        "}\n",
        // One node injects `alarm` or `scheduler`, not both.
        "fn both(value: i64) -> i64 {\n",
        "    inject scheduler, alarm\n",
        "    when { return value }\n",
        "}\n",
    ));
    let capability: Vec<String> = checked
        .summary()
        .into_iter()
        .filter(|finding| finding.contains("injectable/"))
        .collect();
    assert_eq!(
        capability,
        [
            "m.hgl:14:21 injectable/capability.as_value TIME-8",
            "m.hgl:15:23 injectable/capability.as_value TIME-8",
            "m.hgl:16:33 injectable/capability.as_value TIME-8",
            "m.hgl:17:16 injectable/capability.as_value TIME-8",
            "m.hgl:18:15 injectable/capability.as_value TIME-8",
            "m.hgl:20:23 injectable/capability.as_value TIME-8",
            "m.hgl:22:22 injectable/capability.as_value TIME-8",
            "m.hgl:23:16 injectable/capability.as_value TIME-8",
            "m.hgl:27:23 injectable/inject.alarm_and_scheduler TIME-8",
        ]
    );
}
