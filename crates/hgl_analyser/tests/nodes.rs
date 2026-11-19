//! The analyser against the rules on a node's hooks. Each test cites
//! its rule.
//!
//! A finding is written `file:line:column category/code rule`.

mod common;

use common::{body, module};
use hgl_analyser::hir::{EndpointOperation, ExprKind, Hook, Stmt};

/// No lines.
const NONE: [&str; 0] = [];

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
            "scheduler: Scheduler",
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
        "        let on = valid(value)\n",
        "        if on { return value }\n",
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

/// TIME-5: "`start` reads no temporal input and gives none to an
/// endpoint operation: it runs before any has a value. `stop` reads a
/// temporal input under manual admission, as a `when` under
/// `valid([])` does: no header stands over it, so each payload read is
/// dominated by a guard (TIME-4)."
#[test]
fn start_reads_no_input_and_stop_reads_one_under_a_guard() {
    let checked = module(concat!(
        "fn last_seen(value: i64, other: i64) -> i64 {\n",
        "    state last: i64 = 0\n",
        "    state seen: bool = false\n",
        "    start {\n",
        // A read, and an endpoint operation.
        "        last = value\n",
        "        seen = valid(value)\n",
        "    }\n",
        "    when { return value }\n",
        "    stop {\n",
        // Under its guard, and after a guard that returns.
        "        if valid(value) { last = value }\n",
        "        if valid(value) && valid(other) { last = value + other }\n",
        // No guard, and the guard of another input.
        "        last = value\n",
        "        if valid(value) { last = other }\n",
        "    }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:6:16 phase/input.lifecycle TIME-5",
            "m.hgl:7:16 phase/input.lifecycle TIME-5",
            "m.hgl:13:16 phase/payload.unguarded TIME-5",
            "m.hgl:14:34 phase/payload.unguarded TIME-5",
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

/// TIME-3: "`modified([])` subscribes to no input: the node's scheduler
/// or alarm is then what wakes the handler. Where the node injects
/// neither, or its `start` does not use the one it injects, nothing
/// wakes it: the node is dormant, and the selector is warned of
/// (`node.no_activation`, ERR-10). Any use in `start` counts; what a
/// function given the capability does with it is not followed. A node
/// that is dormant by design acknowledges the warning." ERR-1: "A
/// warning does not reject valid source."
#[test]
fn a_dormant_node_is_warned_of() {
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
        // Any use in `start` counts: an alarm in a branch, a local the
        // scheduler is bound to, a function it is given to, and with
        // `start` written after the handler.
        "fn alarmed(a: f64) -> f64 {\n",
        "    inject alarm\n",
        "    start {\n",
        "        if true { schedule((alarm), 1s) }\n",
        "    }\n",
        "    when modified([]) { return a }\n",
        "}\n",
        "fn renamed(a: f64) -> f64 {\n",
        "    inject scheduler\n",
        "    start { let timer = scheduler }\n",
        "    when modified([]) { return a }\n",
        "}\n",
        "const fn armed(scheduler: Scheduler) -> i64 => 0\n",
        "fn helped(a: f64) -> f64 {\n",
        "    inject scheduler\n",
        "    when modified([]) { return a }\n",
        "    start { armed(scheduler) }\n",
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
        // A `start` that does not use it: the scheduler is used only in
        // the handler it would wake.
        "fn idle(a: f64) -> f64 {\n",
        "    inject scheduler, logger\n",
        "    start { log_info(logger, \"up\") }\n",
        "    when modified([]) {\n",
        "        schedule(scheduler, 1s)\n",
        "        return a\n",
        "    }\n",
        "}\n",
        // Dormant by design: the chapter's `null_sink`.
        "@suppress_warning(\"node.no_activation\")\n",
        "fn null_sink(a: f64) {\n",
        "    when modified([]) { }\n",
        "}\n",
        // Another warning's name acknowledges nothing of this one.
        "@suppress_warning(\"statement.no_effect\")\n",
        "fn other(a: f64) {\n",
        "    when modified([]) { }\n",
        "}\n",
        // The string's value names the warning, however it is spelled.
        "@suppress_warning(\"node\\u{2e}no_activation\")\n",
        "fn escaped(a: f64) {\n",
        "    when modified([]) { }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:29:10 warning/node.no_activation TIME-3",
            "m.hgl:33:10 warning/node.no_activation TIME-3",
            "m.hgl:38:10 warning/node.no_activation TIME-3",
            "m.hgl:49:10 warning/node.no_activation TIME-3",
        ]
    );
    // Each is accepted: a warning rejects nothing.
    assert!(!checked.rejected);
}

/// TIME-3: "A selector sets subscription or admission when it stands
/// alone in the header or appears as a top-level conjunct. Selectors
/// under negation or disjunction leave the default in place." Where
/// `modified([])` "is a bool, in a body or under a negation or a
/// disjunction, it is true in a cycle where no temporal input was
/// modified": no selector, so the handler subscribes to every input
/// ("A handler with no `modified` selector subscribes to every temporal
/// input"), which wakes it, and the node is not dormant. A selector
/// alone and as a top-level conjunct is warned of (`node.no_activation`).
#[test]
fn an_empty_selection_that_is_no_selector_leaves_the_node_awake() {
    let checked = module(concat!(
        "fn negated(a: f64) -> bool {\n",
        "    when !modified([]) { return true }\n",
        "}\n",
        "fn in_body(a: f64) -> bool {\n",
        "    when { return modified([]) }\n",
        "}\n",
        "fn either(a: f64) -> bool {\n",
        "    when modified([]) || modified(a) { return true }\n",
        "}\n",
        "fn guarded(a: f64) -> bool {\n",
        "    when {\n",
        "        if modified([]) { return false }\n",
        "        return true\n",
        "    }\n",
        "}\n",
        // The control: a selector, alone and as a top-level conjunct.
        "fn alone(a: f64) -> bool {\n",
        "    when modified([]) { return true }\n",
        "}\n",
        "fn conjunct(a: f64) -> bool {\n",
        "    when valid(a) && modified([]) { return true }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:18:10 warning/node.no_activation TIME-3",
            "m.hgl:21:22 warning/node.no_activation TIME-3",
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
        // Retained: in a tuple, a list, a dictionary, a struct, a
        // `state`, and what is published.
        "        let pair = (scheduler, value)\n",
        "        let listed = [clock]\n",
        "        let keyed: dict<str, i64> = [\"at\": (clock)]\n",
        "        let entry = Kept(value: logger)\n",
        "        held = clock\n",
        "        out = scheduler\n",
        // A local a capability is bound to is that capability: the
        // pinned core gives none a type, and it is one all the same.
        "        let again = bound\n",
        "        let stored = [again]\n",
        "        var moving = if value > 0 { clock } else { bound }\n",
        "        let moved = (moving, 1)\n",
        // FN-14 lets an anonymous function use a `let` around it: one
        // a capability is bound to is a capture of the capability.
        "        let reader = fn() => again\n",
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
            "m.hgl:16:45 injectable/capability.as_value TIME-8",
            "m.hgl:17:33 injectable/capability.as_value TIME-8",
            "m.hgl:18:16 injectable/capability.as_value TIME-8",
            "m.hgl:19:15 injectable/capability.as_value TIME-8",
            "m.hgl:21:23 injectable/capability.as_value TIME-8",
            "m.hgl:23:22 injectable/capability.as_value TIME-8",
            "m.hgl:24:30 injectable/capability.as_value TIME-8",
            "m.hgl:25:16 injectable/capability.as_value TIME-8",
            "m.hgl:29:23 injectable/inject.alarm_and_scheduler TIME-8",
        ]
    );
}

/// TIME-9: "`scheduled()` is the handler selector for a wake by the
/// node's scheduler or alarm. In a node that injects `scheduler` it is
/// the scheduler's `is_scheduled_now` ... In a node that injects
/// `alarm` it holds where the node is evaluated and no temporal input
/// has ticked. A node that injects neither rejects it
/// (`inject.no_scheduler`)." Core's CAP-4: `is_scheduled_now` is a
/// `bool` property of the scheduler, read in a body too.
#[test]
fn scheduled_asks_the_scheduler_or_the_alarm_the_node_injects() {
    let checked = module(concat!(
        "fn sampled(price: f64, const every: duration = 1s) -> f64 {\n",
        "    state last: f64 = 0.0\n",
        "    inject scheduler\n",
        "    start { schedule(scheduler, every) }\n",
        "    when modified(price) && valid(price) { last = price }\n",
        "    when scheduled() {\n",
        "        schedule(scheduler, every)\n",
        "        return last\n",
        "    }\n",
        "}\n",
        "fn polled(const every: duration = 1s) -> bool {\n",
        "    inject alarm\n",
        "    start { schedule(alarm, every) }\n",
        "    when scheduled() { return true }\n",
        "}\n",
        // The property the selector is, read as a value.
        "fn due(price: f64) -> bool {\n",
        "    inject scheduler\n",
        "    start { schedule(scheduler, 1s) }\n",
        "    when { return scheduler.is_scheduled_now }\n",
        "}\n",
        // Neither is injected: in a header, and in a body.
        "fn neither(price: f64) -> f64 {\n",
        "    inject clock\n",
        "    when scheduled() { return price }\n",
        "    when {\n",
        "        if !scheduled() { return price }\n",
        "    }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:24:10 injectable/inject.no_scheduler TIME-9",
            "m.hgl:26:13 injectable/inject.no_scheduler TIME-9",
        ]
    );
}

/// TIME-9: "`scheduled()` is the handler selector for a wake by the
/// node's scheduler or alarm ... A node that injects neither rejects it
/// (`inject.no_scheduler`)". SOURCE-1: a source is "a node" that
/// injects "not `out`, `scheduler`, or `alarm`", and a node's `state`
/// initializer is its own. FN-9: "A value function injects none", and
/// TIME-9: it "has no output or schedule of its own"; a graph is no
/// node either: "Constructed nodes own their own output and
/// scheduler". The catalogue has no row for the selector in a value
/// function or a graph, and the code is local.
#[test]
fn scheduled_is_a_selector_of_a_node_alone() {
    let checked = module(concat!(
        "fn in_source() -> bool {\n",
        "    yield 1s: scheduled()\n",
        "}\n",
        "const fn in_value_function() -> bool => scheduled()\n",
        "fn in_graph(value: i64) -> bool => scheduled()\n",
        "fn in_initializer(value: i64) -> bool {\n",
        "    state woken: bool = scheduled()\n",
        "    when { return woken }\n",
        "}\n",
        // The control: a node's hook.
        "fn in_node(value: i64) -> bool {\n",
        "    when { return scheduled() }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:15 injectable/inject.no_scheduler TIME-9",
            "m.hgl:5:41 local:phase/scheduled.outside_node FN-9",
            "m.hgl:6:36 local:phase/scheduled.outside_node FN-9",
            "m.hgl:8:25 injectable/inject.no_scheduler TIME-9",
            "m.hgl:12:19 injectable/inject.no_scheduler TIME-9",
        ]
    );
}

/// EXPR-3: "The place's root is a `var`, a `state`, a `cache`, or, in a
/// `when`, `out` (TIME-7)." TIME-7: "`out` is read in a `when` and in
/// `stop`, and written only in a `when`". TIME-8: a capability is not
/// "an entry of a tuple, list, set, dictionary, or struct".
#[test]
fn a_place_of_a_state_or_of_out_is_assigned() {
    let checked = module(concat!(
        "struct Point {\n",
        "    x: f64\n",
        "    y: f64\n",
        "}\n",
        "fn tracked(price: f64) -> Point {\n",
        "    inject out, clock\n",
        "    state kept: Point = Point(x: 0.0, y: 0.0)\n",
        "    cache seen: list<f64> = [0.0]\n",
        "    start {\n",
        "        kept.x = 1.0\n",
        "        out.x = 1.0\n",
        "    }\n",
        "    when {\n",
        "        kept.x = price\n",
        "        seen[0] += price\n",
        "        out.y = price\n",
        // A parameter of a node is a temporal input: no place of it
        // is assigned. A capability given to an element is retained.
        "        price.x = 1.0\n",
        "        seen[0] = clock\n",
        "        return kept\n",
        "    }\n",
        "    stop {\n",
        "        out.x = 0.0\n",
        "    }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:12:9 phase/out.lifecycle TIME-7",
            "m.hgl:18:9 name/binding.immutable EXPR-3",
            "m.hgl:19:19 injectable/capability.as_value TIME-8",
            "m.hgl:19:19 type/type.mismatch EXPR-5",
            "m.hgl:23:9 phase/out.lifecycle TIME-7",
        ]
    );
}
