//! The analyser against the rules on a capability's type, a native
//! type's properties and the type of an endpoint. Each test cites its
//! rule.
//!
//! A finding is written `file:line:column category/code rule`.
//!
//! Core declares the capability types (TIME-8). Each test here gives a
//! `module core` of its own, which has only what the test reads, so that
//! what is checked does not turn on which core the compiler is built
//! with.

mod common;

use common::{Checked, check, locals};

/// No lines.
const NONE: [&str; 0] = [];

/// A core with the capability types, one operation on a logger, and
/// the operator `+` stands for.
/// `log` is native and so private to core (UNIT-3); `log_info` is the
/// helper other modules call.
const CORE: &str = concat!(
    "module core\n",
    "export enum LogLevel { debug, info, warning, error }\n",
    "export native type Logger {\n",
    "    level: LogLevel\n",
    "}\n",
    "export native type Clock {\n",
    "    evaluation_time: datetime\n",
    "    now: datetime\n",
    "    cycle_time: duration\n",
    "}\n",
    "export native type Scheduler {\n",
    "    next_scheduled_time: datetime?\n",
    "    is_scheduled: bool\n",
    "}\n",
    "export native type Alarm\n",
    "export native type GlobalState\n",
    "native const fn log(logger: Logger, level: LogLevel, message: str)\n",
    // `+`, as core declares it.
    "export operator add_<L, R, O>(lhs: L, rhs: R) -> O\n",
    "export const fn log_info(logger: Logger, message: str) {\n",
    "    log(logger, .info, message)\n",
    "}\n",
);

/// Checks `core.hgl`, the core above, with one file, `m.hgl`, of module
/// `m`. The `module` line is added, so the text given starts on line 2.
fn module(text: &str) -> Checked {
    check(&[("core.hgl", CORE), ("m.hgl", &format!("module m\n{text}"))])
}

/// TIME-8: "`inject` binds each of the others to a local of its
/// capability's type, which core declares and names as the capability
/// with a capital: `Logger`, `Clock`, `Scheduler`, `Alarm`,
/// `GlobalState` ... Binding one to a `let` and giving one to a
/// parameter of its type are uses. Its properties are read-only,
/// `clock.evaluation_time`". FN-9: a value function "is given one as an
/// argument", its parameter of the capability's type. NATIVE-6:
/// "`value.property` reads one as an ordinary value."
#[test]
fn a_capability_has_the_type_core_declares() {
    let checked = module(concat!(
        "const fn stamped(clock: Clock, value: f64) -> tuple<datetime, f64> =>\n",
        "    (clock.evaluation_time, value)\n",
        "fn stamp(value: f64) -> tuple<datetime, f64> {\n",
        "    inject out, clock, logger, scheduler, global_state\n",
        "    when {\n",
        "        log_info(logger, \"tick\")\n",
        "        let bound = clock\n",
        "        let at = bound.now\n",
        "        let level = logger.level\n",
        "        let next = scheduler.next_scheduled_time\n",
        "        return stamped(clock, value)\n",
        "    }\n",
        "}\n",
        "fn poll() -> i64 {\n",
        "    inject alarm\n",
        "    when scheduled() { return 1 }\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(
        locals(&checked, "stamp"),
        [
            "value: f64",
            // `out` is the node's result (TIME-7), and has no type of
            // its own.
            "out: tuple<datetime, f64>",
            "clock: Clock",
            "logger: Logger",
            "scheduler: Scheduler",
            "global_state: GlobalState",
            "bound: Clock",
            "at: datetime",
            "level: LogLevel",
            "next: datetime?",
        ]
    );
    assert_eq!(locals(&checked, "poll"), ["alarm: Alarm"]);
}

/// TIME-8 and GEN-3 admit a capability type listed in the type set of an
/// operator declaration's membership. Other constraints and nested types
/// still reject it.
#[test]
fn a_capability_type_can_be_listed_in_an_operator_type_membership() {
    let checked = module(concat!(
        "operator wake<C, W>(capability: C, at: W)\n",
        "    requires C in {Scheduler, Alarm} && W in {duration, datetime}\n",
        "operator nested<C>(value: C) requires C in {list<Alarm>}\n",
        "operator equal<C>(value: C) requires C == Alarm\n",
        "operator ordinary<C>(value: C)\n",
        "impl const fn ordinary<C>(value: C) requires C in {Scheduler} {}\n",
        "const fn helper<C>(value: C) requires C in {Alarm} {}\n",
        "const ONE: i64 = 1\n",
        "operator mixed<C>(value: C) requires C in {Alarm, 1}\n",
        "operator named<C>(value: C) requires C in {ONE, Alarm}\n",
        "operator constant<const N: i64>(value: i64) requires N in {Alarm}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:4:50 injectable/capability.as_value TIME-8",
            "m.hgl:5:43 injectable/capability.as_value TIME-8",
            "m.hgl:7:52 injectable/capability.as_value TIME-8",
            "m.hgl:8:45 injectable/capability.as_value TIME-8",
            "m.hgl:10:44 injectable/capability.as_value TIME-8",
            "m.hgl:11:44 type/type.not_a_type UNIT-10",
            "m.hgl:11:49 injectable/capability.as_value TIME-8",
            "m.hgl:12:60 injectable/capability.as_value TIME-8",
        ]
    );
}

/// TIME-8: "A capability type is written only as the type of a
/// parameter of a `const fn` or of a native value function. A capability
/// is used within the node that injects it and is not retained: it is
/// not ... what a function returns". EXPR-5: a `Logger` where a `Clock`
/// is taken does not fit.
#[test]
fn a_capability_type_is_a_value_functions_parameter() {
    let checked = module(concat!(
        "const fn stamped(clock: Clock, value: f64) -> f64 => value\n",
        "native const fn raw(clock: Clock) -> datetime\n",
        // The type of a result, and the parameter returned.
        "const fn back(clock: Clock) -> Clock => clock\n",
        // A parameter of a temporal function, of a native one, a field,
        // a part of another type and a local's annotation.
        "fn takes(clock: Clock, value: f64) -> f64 => value\n",
        "native fn fed(logger: Logger, value: f64) -> f64\n",
        "struct Holder { clock: Clock }\n",
        "const fn many(clocks: list<Clock>) -> i64 => 0\n",
        "fn keeps(value: f64) -> f64 {\n",
        "    inject clock, logger\n",
        "    when {\n",
        "        let named: Clock = clock\n",
        "        return stamped(logger, value)\n",
        "    }\n",
        "}\n",
        // A value function retains none either.
        "const fn listed(clock: Clock) -> i64 {\n",
        "    let kept = [clock]\n",
        "    return 0\n",
        "}\n",
        // The value of a block, and of an `if`, goes where the block
        // or the `if` does: returned, and into a list.
        "const fn leaked(clock: Clock) -> any {\n",
        "    clock\n",
        "}\n",
        "const fn chosen(clock: Clock, flag: bool) -> i64 {\n",
        "    let kept = [if flag { clock } else { (clock) }]\n",
        "    let bound = if flag { clock } else { clock }\n",
        "    if flag { clock }\n",
        "    return 0\n",
        "}\n",
        // FN-7: "In a function that declares no result the expression
        // is a statement": nothing is returned, and so nothing kept.
        "const fn discarded(clock: Clock) {\n",
        "    clock\n",
        "}\n",
        "const fn dropped(clock: Clock) => clock\n",
        // A parameter of a capability's type captured by an anonymous
        // function is retained by it.
        "const fn closed(clock: Clock) -> i64 {\n",
        "    let reader = fn() => clock.now\n",
        "    return 0\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:4:32 injectable/capability.as_value TIME-8",
            "m.hgl:4:41 injectable/capability.as_value TIME-8",
            "m.hgl:5:17 injectable/capability.as_value TIME-8",
            "m.hgl:6:23 injectable/capability.as_value TIME-8",
            "m.hgl:7:24 injectable/capability.as_value TIME-8",
            "m.hgl:8:28 injectable/capability.as_value TIME-8",
            "m.hgl:12:20 injectable/capability.as_value TIME-8",
            "m.hgl:13:24 type/type.mismatch EXPR-5",
            "m.hgl:17:17 injectable/capability.as_value TIME-8",
            "m.hgl:21:5 injectable/capability.as_value TIME-8",
            "m.hgl:24:27 injectable/capability.as_value TIME-8",
            "m.hgl:24:43 injectable/capability.as_value TIME-8",
            "m.hgl:34:26 injectable/capability.as_value TIME-8",
        ]
    );
}

/// NATIVE-6: "A native type may declare read-only properties, each a
/// name and an ordinary value type, written as a struct's typed fields
/// are. `value.property` reads one as an ordinary value." Two of one
/// name reject, as two fields of a struct do (TYPE-4). TYPE-12: "An
/// ordinary value type is written with no `rolling_window`, `atomic`, or
/// `signal` at any depth".
#[test]
fn a_native_types_properties_are_ordinary_values() {
    let checked = check(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "native type Handle {\n",
            "    size: i64\n",
            "    label: str?\n",
            "    time: duration\n",
            "    size: f64\n",
            "    feed: rolling_window<f64, 5>\n",
            "}\n",
            "native type Opaque\n",
            "const fn read(handle: Handle, opaque: Opaque) -> i64 {\n",
            "    let label = handle.label\n",
            "    let span = handle.time\n",
            "    let missing = handle.width\n",
            "    let nothing = opaque.size\n",
            // "None is assigned, and a native type is not constructed
            // in HGL: a call of its name rejects."
            "    handle.size = 2\n",
            "    let made = Handle(size: 1)\n",
            "    return handle.size\n",
            "}\n",
        ),
    )]);
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:6:5 type/field.duplicate NATIVE-6",
            "m.hgl:7:11 type/type.not_ordinary NATIVE-6",
            "m.hgl:13:26 name/member.unknown NATIVE-6",
            "m.hgl:14:26 name/member.unknown NATIVE-6",
            "m.hgl:15:5 name/binding.immutable NATIVE-6",
            "m.hgl:16:16 type/call.not_callable NATIVE-6",
        ]
    );
    assert_eq!(
        locals(&checked, "read"),
        [
            "handle: Handle",
            "opaque: Opaque",
            "label: str?",
            "span: duration",
            "missing: _",
            "nothing: _",
            "made: _",
        ]
    );
}

/// TYPE-12: "`endpoint<T>` is the endpoint of a temporal value of T, for
/// a function that takes the endpoint and not its payload (TIME-9). It
/// is the whole type of a parameter of a native value function, and of
/// nothing else." FN-1 makes that parameter the exception to a value
/// function's ordinary value types.
#[test]
fn endpoint_is_the_type_of_a_native_value_functions_parameter() {
    let checked = check(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "native const fn peek<T>(input: endpoint<T>) -> bool\n",
            "operator probe<T>(input: T) -> bool\n",
            "native impl const fn probe(input: endpoint<i64>) -> bool\n",
            // Not the whole type, a result, and with `?`.
            "native const fn many<T>(inputs: list<endpoint<T>>) -> bool\n",
            "native const fn gives<T>(input: endpoint<T>) -> endpoint<T>\n",
            "native const fn maybe<T>(input: endpoint<T>?) -> bool\n",
            // A native temporal function, a value function that is not
            // native, a graph and a field.
            "native fn node(input: endpoint<f64>) -> bool\n",
            "const fn plain(input: endpoint<f64>) -> bool => true\n",
            "fn graph(input: endpoint<f64>) -> f64 => 1.0\n",
            "struct Holder { input: endpoint<f64> }\n",
        ),
    )]);
    let endpoints: Vec<String> = checked
        .summary()
        .into_iter()
        .filter(|finding| finding.contains("endpoint.position"))
        .collect();
    assert_eq!(
        endpoints,
        [
            "m.hgl:5:38 type/endpoint.position TYPE-12",
            "m.hgl:6:49 type/endpoint.position TYPE-12",
            "m.hgl:7:33 type/endpoint.position TYPE-12",
            "m.hgl:8:23 type/endpoint.position TYPE-12",
            "m.hgl:9:23 type/endpoint.position TYPE-12",
            "m.hgl:10:17 type/endpoint.position TYPE-12",
            "m.hgl:11:24 type/endpoint.position TYPE-12",
        ]
    );
}

/// TIME-8: "Binding one to a `let` and giving one to a parameter of its
/// type are uses." A parameter written as a generic parameter or `any`
/// is of no one type, and takes no capability; nor is a generic
/// argument a capability type (owner's decision of 2026-11-16 on
/// hhenson/hgl_spec#73, question 5).
#[test]
fn a_capability_is_given_only_to_a_parameter_of_its_type() {
    let checked = module(concat!(
        "const fn stamped(clock: Clock, value: f64) -> f64 => value\n",
        "const fn boxed<T>(value: T) -> list<T> => [value]\n",
        "const fn shown(value: any) -> i64 => 0\n",
        "operator kept<V>(value: V) -> i64\n",
        "impl const fn kept<V>(value: V) -> i64 => 0\n",
        "const fn maybe<T>(value: T?) -> i64 => 0\n",
        "const fn perhaps(value: any?) -> i64 => 0\n",
        "impl const fn add_<V>(lhs: V, rhs: V) -> i64 => 0\n",
        "fn uses(value: f64) -> f64 {\n",
        "    inject clock\n",
        "    when {\n",
        // A parameter of the capability's type.
        "        let stamp = stamped(clock, value)\n",
        // A generic parameter, with the argument inferred and written,
        // through a local the capability is bound to, of an operator's
        // candidate, and `any`.
        "        let held = boxed(clock)\n",
        "        let typed = boxed<Clock>(clock)\n",
        "        let bound = clock\n",
        "        let again = boxed(bound)\n",
        "        let count = kept(clock)\n",
        "        let text = shown(clock)\n",
        // With `?`, which takes what the type under it takes.
        "        let one = maybe(clock)\n",
        "        let two = perhaps(clock)\n",
        // A symbol calls the candidate it selects.
        "        let sum = clock + clock\n",
        // What is no capability is given to either.
        "        let fine = boxed(value)\n",
        "        return value\n",
        "    }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:14:26 injectable/capability.as_value TIME-8",
            "m.hgl:15:27 injectable/capability.as_value TIME-8",
            "m.hgl:15:34 injectable/capability.as_value TIME-8",
            "m.hgl:17:27 injectable/capability.as_value TIME-8",
            "m.hgl:18:26 injectable/capability.as_value TIME-8",
            "m.hgl:19:26 injectable/capability.as_value TIME-8",
            "m.hgl:20:25 injectable/capability.as_value TIME-8",
            "m.hgl:21:27 injectable/capability.as_value TIME-8",
            "m.hgl:22:19 injectable/capability.as_value TIME-8",
            "m.hgl:22:27 injectable/capability.as_value TIME-8",
        ]
    );
}

/// TIME-8: a capability's "properties are read-only". NATIVE-6: "None is
/// assigned." A capability is of a native type whether or not core has
/// declared it.
#[test]
fn a_capabilitys_property_is_not_assigned() {
    let typed = module(concat!(
        "fn stamps(value: f64) -> f64 {\n",
        "    inject clock\n",
        "    when {\n",
        "        clock.evaluation_time = clock.now\n",
        "        return value\n",
        "    }\n",
        "}\n",
    ));
    assert_eq!(
        typed.summary(),
        ["m.hgl:5:9 name/binding.immutable NATIVE-6"]
    );
    // With the core the compiler is built with, which declares no
    // capability type yet.
    let untyped = check(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "fn stamps(value: f64) -> f64 {\n",
            "    inject clock\n",
            "    when {\n",
            "        clock.evaluation_time = clock.now\n",
            "        return value\n",
            "    }\n",
            "}\n",
        ),
    )]);
    assert_eq!(
        untyped.summary(),
        ["m.hgl:5:9 name/binding.immutable NATIVE-6"]
    );
}

/// NATIVE-1: "A native function that is no candidate is private unless
/// exported, as a function is (UNIT-3). `export` is written on its
/// declaration, the form with no body, and nowhere else." UNIT-3:
/// "Ordinary functions and types are private unless exported, a native
/// function and a native type among them".
#[test]
fn an_exported_native_function_is_another_modules_to_call() {
    let checked = check(&[
        (
            "lib.hgl",
            concat!(
                "module lib\n",
                "export native type Handle {\n",
                "    size: i64\n",
                "}\n",
                "export native const fn describe(value: i64) -> str\n",
                "native const fn hidden(value: i64) -> str\n",
                "native type Private\n",
            ),
        ),
        (
            "lib_provider.hgl",
            concat!(
                "module lib part provider\n",
                // The part does not repeat `export`.
                "native const fn describe(value: i64) -> str {}\n",
                "native const fn hidden(value: i64) -> str {}\n",
            ),
        ),
        (
            "app.hgl",
            concat!(
                "module app\n",
                "use lib::{describe, hidden, Handle, Private}\n",
                "const fn shown(handle: Handle) -> str => describe(handle.size)\n",
            ),
        ),
    ]);
    assert_eq!(
        checked.summary(),
        [
            "app.hgl:2:21 module/import.private UNIT-2",
            "app.hgl:2:37 module/import.private UNIT-2",
        ]
    );
}

/// Core's capability contract, with the core the compiler is built
/// with. CAP-1: "Each capability but `out` has a native type of core's
/// module, named as the capability with a capital". CAP-2:
/// "`log(logger, level, message)` writes one record ... `log_debug`,
/// `log_info`, `log_warning`, and `log_error` each call `log` at their
/// level. `logger.level` is the logger's current level." CAP-3: a
/// clock's four properties. CAP-4: a scheduler's three properties,
/// `un_schedule` and `reset`. CAP-5: an alarm "has no property".
#[test]
fn the_supplied_core_declares_the_capabilities() {
    let checked = check(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "fn uses(value: f64) -> f64 {\n",
            "    inject clock, logger, scheduler, global_state\n",
            "    when {\n",
            "        log(logger, .warning, \"direct\")\n",
            "        log_debug(logger, \"debug\")\n",
            "        log_info(logger, \"info\")\n",
            "        log_warning(logger, \"warning\")\n",
            "        log_error(logger, \"error\")\n",
            "        let level = logger.level\n",
            "        let at = clock.evaluation_time\n",
            "        let now = clock.now\n",
            "        let spent = clock.cycle_time\n",
            "        let next = clock.next_cycle_evaluation_time\n",
            "        let pending = scheduler.is_scheduled\n",
            "        let now_due: bool = scheduler.is_scheduled_now\n",
            "        let due = scheduler.next_scheduled_time\n",
            "        un_schedule(scheduler)\n",
            "        un_schedule(scheduler, tag: \"poll\")\n",
            "        reset(scheduler)\n",
            "        return value\n",
            "    }\n",
            "}\n",
            "fn alarmed(value: f64) -> f64 {\n",
            "    inject alarm\n",
            "    when {\n",
            "        let none = alarm.is_scheduled\n",
            "        log(alarm, .info, \"text\")\n",
            "        return value\n",
            "    }\n",
            "}\n",
        ),
    )]);
    assert!(checked.core_supplied);
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:27:26 name/member.unknown NATIVE-6",
            "m.hgl:28:13 type/type.mismatch EXPR-5",
        ]
    );
    assert_eq!(checked.prelude_names("m"), [""; 0]);
    assert_eq!(
        locals(&checked, "uses"),
        [
            "value: f64",
            "clock: Clock",
            "logger: Logger",
            "scheduler: Scheduler",
            "global_state: GlobalState",
            "level: LogLevel",
            "at: datetime",
            "now: datetime",
            "spent: duration",
            "next: datetime",
            "pending: bool",
            "now_due: bool",
            "due: datetime?",
        ]
    );
}

/// Core's scheduling and global store, with the core the compiler is
/// built with. CAP-4: "`schedule(scheduler, at)` asks for a wake: `at`
/// is a `duration` or a `datetime`. It takes a `tag` ... and
/// `on_wall_clock`". CAP-5: "`schedule(alarm, at)` ... It has no
/// property, no tag, no wall clock". CAP-6: "`get<V>(global_state, key)`
/// is the value under `key`, of the type the call writes or its place
/// expects ... `has(global_state, key)` ... `put(global_state, key,
/// value)`", each key "fixed when the graph is built (the language's
/// FN-1)". ERR-6: `get` on a dictionary "yields contextual absence".
#[test]
fn the_supplied_core_declares_scheduling_and_the_store() {
    let checked = check(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "fn counted(const every: duration = 1s, const name: str = \"count\") -> i64 {\n",
            "    inject scheduler, global_state\n",
            "    start {\n",
            "        if !has(global_state, \"count\") { put(global_state, \"count\", 0) }\n",
            "        schedule(scheduler, every, tag: \"count\")\n",
            "    }\n",
            "    when scheduled() {\n",
            "        schedule(scheduler, at: every, on_wall_clock: true)\n",
            "        schedule(scheduler, @2026-11-16T09:00Z)\n",
            "        put(global_state, name, get<i64>(global_state, name) + 1)\n",
            "        let total: i64 = get(global_state, \"total\")\n",
            "        return get<i64>(global_state, \"count\")\n",
            "    }\n",
            "}\n",
            "fn woken(value: f64, const every: duration = 1s) -> f64 {\n",
            "    inject alarm\n",
            "    start { schedule(alarm, every) }\n",
            "    when {\n",
            "        schedule(alarm, @2026-11-16T09:00Z)\n",
            "        return value\n",
            "    }\n",
            "}\n",
            "const fn lookup(book: dict<str, i64>, key: str) -> i64 {\n",
            "    let price = get(book, key)\n",
            "    if price == null { return 0 }\n",
            "    return price\n",
            "}\n",
            // A tag on an alarm, unsupported schedule argument types, a
            // key that is no string, and a key from the evaluation.
            "fn wrong(value: str) -> str {\n",
            "    inject alarm, global_state\n",
            "    start { schedule(alarm, 1s, tag: \"poll\") }\n",
            "    when {\n",
            "        schedule(alarm, 5)\n",
            "        put(global_state, 7, 1)\n",
            "        put(global_state, value, 1)\n",
            "        return value\n",
            "    }\n",
            "}\n",
            // The run's own key, in each of the store's operations. A
            // key that only starts as it does is a program's.
            "fn taken(value: i64) -> i64 {\n",
            "    inject global_state\n",
            "    start { put(global_state, \"run.output\", 0) }\n",
            "    when {\n",
            "        if has(global_state, \"run.output\") { return get<i64>(global_state, \"run.output\") }\n",
            "        put(global_state, \"run.output.mine\", value)\n",
            "        return labelled(global_state, \"run.output\", value)\n",
            "    }\n",
            "}\n",
            // A function of the program that is given the store: its
            // own `const` parameter is no key of the store.
            "const fn labelled(store: GlobalState, const label: str, value: i64) -> i64 => value\n",
            "const fn wrong_capability(clock: Clock) { schedule(clock, 1s) }\n",
        ),
    )]);
    assert!(checked.core_supplied);
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:31:13 operator/operator.no_candidate GEN-5",
            "m.hgl:33:9 operator/operator.no_candidate GEN-5",
            "m.hgl:34:9 operator/operator.no_candidate GEN-5",
            "m.hgl:35:27 type/argument.constant FN-1",
            // The global store: "The key `\"run.output\"` is the run's
            // own ... a program that names it rejects".
            "m.hgl:41:31 injectable/store.reserved_key TIME-8",
            "m.hgl:43:30 injectable/store.reserved_key TIME-8",
            "m.hgl:43:76 injectable/store.reserved_key TIME-8",
            "m.hgl:49:43 operator/operator.no_candidate GEN-5",
        ]
    );
    assert_eq!(checked.prelude_names("m"), [""; 0]);
    assert!(locals(&checked, "counted").contains(&"total: i64".to_string()));
    assert!(locals(&checked, "lookup").contains(&"price: result<i64, error>".to_string()));
}

/// TIME-8 and CAP-6 apply to names resolved through ordinary const arguments.
#[test]
fn store_key_checks_follow_resolved_const_arguments() {
    let checked = check(&[(
        "m.hgl",
        r#"module m
fn save(const key: str) {
    inject global_state
    put(global_state, key, 7)
}
fn forward(const name: str = "run.output") { save(key: name) }
fn different(const key: str) {
    inject global_state
    put(global_state, key, 7)
    let text: str = get(global_state, key)
}
test ordinary { eval forward(name: "run.output.mine") }
test reserved { eval forward() }
test conflict { eval different(key: "app.key") }
"#,
    )]);
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:10:21 injectable/store.key_type TIME-8",
            "m.hgl:13:17 injectable/store.reserved_key TIME-8",
        ],
        "{}",
        checked.report,
    );
}

/// A generic store use binds its payload type in each instance (CAP-6, TIME-8).
#[test]
fn store_key_types_are_compared_across_instances() {
    let checked = check(&[(
        "m.hgl",
        r#"module m
fn save<V>(const key: str, const value: V) {
    inject global_state
    put(global_state, key, value)
}
fn twice(const key: str) {
    save(key: key, value: 7)
    save(key: "other", value: "allowed")
    save(key: key, value: "wrong")
}
test conflict { eval twice(key: "shared") }
"#,
    )]);
    assert_eq!(
        checked.summary(),
        ["m.hgl:9:5 injectable/store.key_type TIME-8"],
        "{}",
        checked.report,
    );
}

/// A helper may compute its actual key in recursion. A key written
/// directly in a store operation is still checked at that operation.
#[test]
fn store_key_checks_distinguish_a_recursive_value_from_a_written_key() {
    let checked = check(&[(
        "m.hgl",
        r#"module m
const fn recursive_put(store: GlobalState, depth: i64, const key: str) {
    if depth == 0 { put(store, key, 7) }
    else { recursive_put(store, depth - 1, key + ".output") }
}
fn indirect(const prefix: str) {
    inject global_state
    when {}
    stop { recursive_put(global_state, 1, prefix) }
}
test cooked_prefix { eval indirect(prefix: "run.output") }
const fn direct_put(store: GlobalState, depth: i64, const key: str) {
    if depth == 0 { put(store, "run.output", 7) }
    else { direct_put(store, depth - 1, key + ".output") }
}
const fn alpha(store: GlobalState, depth: i64, const key: str) {
    if depth == 0 { put(store, key, 7) }
    else { beta(store, depth - 1, key + ".output") }
}
const fn beta(store: GlobalState, depth: i64, const forwarded_key: str) {
    alpha(store, depth, forwarded_key)
}
fn mutual(const start_key: str) {
    inject global_state
    when {}
    stop { alpha(global_state, 1, start_key) }
}
test indirect_cycle { eval mutual(start_key: "run.output") }
"#,
    )]);
    assert_eq!(
        checked.summary(),
        ["m.hgl:13:32 injectable/store.reserved_key TIME-8"],
        "{}",
        checked.report,
    );
}

/// TIME-8: fixedness is per route. One helper is reached from a cycle that
/// changes its key, "the call that uses it fails ... as it is evaluated",
/// and from a node with a key the text fixes, which "rejects" in source
/// at the call that fixes it (GEN-7, CAP-6). A key a computation gives
/// is left to the graph's build.
#[test]
fn store_key_checks_are_per_route_through_a_helper() {
    let checked = check(&[(
        "m.hgl",
        r#"module m
const fn supply(prefix: str) -> str => prefix
const fn save(store: GlobalState, const key: str) { put(store, key, 7) }
const fn recur(store: GlobalState, depth: i64, const key: str) {
    if depth == 0 { save(store, key) }
    else { recur(store, depth - 1, key + ".output") }
}
fn routes(const computed: str) {
    inject global_state
    when {}
    stop {
        recur(global_state, 1, "run")
        recur(global_state, 1, "run.output")
        save(global_state, computed)
        save(global_state, "user.direct")
        save(global_state, "run.output")
    }
}
test t { eval routes(computed: supply("run.output")) }
"#,
    )]);
    assert_eq!(
        checked.summary(),
        ["m.hgl:16:9 injectable/store.reserved_key TIME-8"],
        "{}",
        checked.report,
    );
}

/// TIME-8: a key two `const` parameters exchange on the way round is the
/// run's, though another check reads the parameter for each instance of
/// the cycle: the instances give the helper no key of their own.
#[test]
fn an_instance_of_a_cycle_gives_its_helper_no_changed_key() {
    let checked = check(&[(
        "m.hgl",
        r#"module m
const fn save(store: GlobalState, const key: str) { put(store, key, 7) }
const fn swap(store: GlobalState, depth: i64, const key: str, const next: str) -> str {
    if depth == 0 {
        save(store, key)
        return ""
    }
    let label = format(key)
    return swap(store, depth - 1, next, key) + label
}
fn hook(const depth: i64) {
    inject global_state
    when {}
    stop { let text = swap(global_state, depth, "run.output", "user.next") }
}
test t { eval hook(depth: 1) }
"#,
    )]);
    assert_eq!(
        checked.summary(),
        Vec::<String>::new(),
        "{}",
        checked.report
    );
}

/// TIME-8: "Where a function calls itself, directly or through other
/// functions, and the key can differ between the calls on the way round",
/// the key is the run's. A candidate that calls its own operator closes
/// such a cycle, and a key two `const` parameters exchange in a function
/// that reads neither is "exchanged on the way round". Neither is checked
/// in source, and no instance is made for each key the cycle reaches.
#[test]
fn cycles_through_an_operator_and_exchanges_are_the_runs() {
    let checked = check(&[(
        "m.hgl",
        r#"module m
operator step<S, T>(store: S, depth: i64, const key: str, value: T) requires S in {GlobalState}
impl const fn step<T>(store: GlobalState, depth: i64, const key: str, value: T) requires T in {i64, f64} {
    if depth == 0 { put(store, key, value) }
    else { step(store, depth - 1, key + ".output", value) }
}
const fn sb(store: GlobalState, depth: i64, const p: str, const q: str) {
    if depth == 0 { sa(store, 0, p) }
    else { sb(store, depth - 1, q, p) }
}
const fn sa(store: GlobalState, depth: i64, const a: str) {
    put(store, a, 1)
    if depth > 0 { sb(store, depth - 1, a, a) }
}
fn hook(const key: str, const depth: i64) {
    inject global_state
    when {}
    stop {
        step(global_state, depth, key, 1)
        sb(global_state, depth, "user.x", "run.output")
    }
}
test first { eval hook(key: "run", depth: 0) }
test grows { eval hook(key: "user", depth: 2) }
test unused { eval hook(key: "run.output", depth: 1) }
"#,
    )]);
    assert_eq!(
        checked.summary(),
        Vec::<String>::new(),
        "{}",
        checked.report
    );
}

/// The #885 route, with a key the text fixes.
const SAME_KEY_ROUTES: &str = r#"module m
const fn sa(store: GlobalState, depth: i64, flag: bool, const k: str) {
    put(store, k, 1)
    if depth > 0 { sb(store, depth - 1, flag, k, k) }
}
const fn sb(store: GlobalState, depth: i64, flag: bool, const p: str, const q: str) {
    if flag { sa(store, depth, flag, p) } else { sa(store, depth, flag, q) }
}
fn node(const key: str) {
    inject global_state
    when {}
    stop { sa(global_state, 1, true, key) }
}
test src {
    eval node(key: "run.output")
}
"#;

/// The #885 route, with a key the text fixes.
const SELECTED_KEY_ROUTE: &str = r#"module m
operator w<S, T>(store: S, const key: str, value: T) requires S in {GlobalState}
impl const fn w(store: GlobalState, const key: str, value: i64) { relay(store, key + "put", 1.5) }
impl const fn w(store: GlobalState, const key: str, value: f64) { put(store, key, value) }
const fn relay<T>(store: GlobalState, const key: str, value: T) requires T in {i64, f64} { w(store, key, value) }
fn node(const key: str) {
    inject global_state
    when {}
    stop { w(global_state, key, 1) }
}
test src {
    eval node(key: "run.out")
}
"#;

/// TIME-8, CAP-6: this route fixes its one key in source (GEN-7).
#[test]
fn one_key_forwarded_to_two_const_slots_stays_fixed() {
    let checked = check(&[("m.hgl", SAME_KEY_ROUTES)]);
    assert_eq!(
        checked.summary(),
        ["m.hgl:15:5 injectable/store.reserved_key TIME-8"],
        "{}",
        checked.report
    );
}

/// TIME-8, CAP-6: this route fixes its one key in source (GEN-7).
#[test]
fn a_selected_store_candidate_does_not_close_another_instances_cycle() {
    let checked = check(&[("m.hgl", SELECTED_KEY_ROUTE)]);
    assert_eq!(
        checked.summary(),
        ["m.hgl:12:5 injectable/store.reserved_key TIME-8"],
        "{}",
        checked.report
    );
}

/// TIME-8, CAP-6: the same routes admit a nonreserved effective key.
#[test]
fn fixed_store_routes_admit_their_nonreserved_key() {
    for source in [
        SAME_KEY_ROUTES.replace("\"run.output\"", "\"user.key\""),
        SELECTED_KEY_ROUTE.replace("\"run.out\"", "\"run.output\""),
    ] {
        let checked = check(&[("m.hgl", &source)]);
        assert_eq!(
            checked.summary(),
            Vec::<String>::new(),
            "{}",
            checked.report
        );
    }
}

/// TIME-8, CAP-6 and UNIT-13: two stable reads before a swapping cycle
/// still supply one fixed key; a helper need not receive a bare local.
#[test]
fn stable_store_entry_expressions_share_their_key() {
    for (declarations, parameter, argument, key) in [
        (
            "const reserved: str = \"run.output\"",
            "const k: str",
            "\"unused\"",
            "reserved",
        ),
        ("", "const k: str", "\"run.out\"", "k + \"put\""),
        (
            "struct Keys { name: str }",
            "const k: Keys",
            "Keys(name: \"run.output\")",
            "k.name",
        ),
        (
            "",
            "const k: list<str, 2>",
            "[\"unused\", \"run.output\"]",
            "k[1]",
        ),
        (
            "",
            "const k: tuple<str, str>",
            "(\"unused\", \"run.output\")",
            "k[1]",
        ),
    ] {
        let source = format!(
            r#"module m
{declarations}
const fn rotate(s: GlobalState, n: i64, const left: str, const right: str) {{
    put(s, left, n)
    if n > 0 {{ rotate(s, n - 1, right, left) }}
}}
fn node({parameter}) {{
    inject global_state
    when {{}}
    stop {{ rotate(global_state, 2, {key}, {key}) }}
}}
test source {{ eval node({argument}) }}
"#
        );
        let checked = check(&[("m.hgl", &source)]);
        assert_eq!(checked.summary().len(), 1, "key {key}: {}", checked.report);
        assert!(
            checked.summary()[0].ends_with("injectable/store.reserved_key TIME-8"),
            "{}",
            checked.report
        );
        let benign = source
            .replace("run.output", "user.key")
            .replace("run.out", "user.");
        let checked = check(&[("m.hgl", &benign)]);
        assert!(checked.summary().is_empty(), "{}", checked.report);
    }
}

/// TIME-8: different fields, indexes or expressions remain different ports;
/// swapping them defers the reserved-key check to the reached operation.
#[test]
fn distinct_store_entry_expressions_keep_operation_checks() {
    for (declarations, parameter, argument, left, right) in [
        (
            "struct Keys {\nleft: str\nright: str\n}",
            "const k: Keys",
            "Keys(left: \"run.output\", right: \"safe\")",
            "k.left",
            "k.right",
        ),
        (
            "",
            "const k: list<str, 2>",
            "[\"run.output\", \"safe\"]",
            "k[0]",
            "k[1]",
        ),
        (
            "",
            "const k: tuple<str, str>",
            "(\"run.output\", \"safe\")",
            "k[0]",
            "k[1]",
        ),
        (
            "",
            "const k: str",
            "\"run.out\"",
            "k + \"put\"",
            "k + \"safe\"",
        ),
    ] {
        let source = format!(
            r#"module m
{declarations}
const fn rotate(s: GlobalState, n: i64, const left: str, const right: str) {{
    if n == 0 {{ put(s, left, n) }}
    else {{ rotate(s, n - 1, right, left) }}
}}
fn node({parameter}) {{
    inject global_state
    when {{}}
    stop {{ rotate(global_state, 1, {left}, {right}) }}
}}
test source {{ eval node({argument}) }}
"#
        );
        let checked = check(&[("m.hgl", &source)]);
        assert!(
            checked.summary().is_empty(),
            "{left} / {right}: {}",
            checked.report
        );
    }
}

/// TIME-8, CAP-6: a helper computes the key at graph build, not in source.
#[test]
fn fixed_store_routes_leave_computed_keys_to_the_build() {
    for (source, literal) in [
        (SAME_KEY_ROUTES, "run.output"),
        (SELECTED_KEY_ROUTE, "run.out"),
    ] {
        let source = source
            .replacen(
                "module m\n",
                "module m\nconst fn supply_key(key: str) -> str => key\n",
                1,
            )
            .replace(
                &format!("key: \"{literal}\""),
                &format!("key: supply_key(\"{literal}\")"),
            );
        let checked = check(&[("m.hgl", &source)]);
        assert_eq!(
            checked.summary(),
            Vec::<String>::new(),
            "{}",
            checked.report
        );
    }
}

/// TIME-8, CAP-6: an unrelated forwarding graph must not erase a fixed-key
/// proof. Core's explicitly supplied documentation contains this map form.
#[test]
fn unrelated_forwarding_graph_does_not_hide_fixed_store_routes() {
    let unrelated = r#"
fn line_total(price: i64, quantity: i64) -> i64 => price * quantity
fn totals(prices: list<i64, 2>, quantities: list<i64, 2>) -> list<i64, 2> =>
    map{join: .outer}(line_total, prices, quantities)
test unrelated_map {
    assert eval totals(prices: [[2, 3]], quantities: [[5, 7]]) == [[10, 21]]
}
"#;
    for (source, line) in [(SAME_KEY_ROUTES, 15), (SELECTED_KEY_ROUTE, 12)] {
        let source = format!("{source}{unrelated}");
        let checked = check(&[("m.hgl", &source)]);
        assert_eq!(
            checked.summary(),
            [format!(
                "m.hgl:{line}:5 injectable/store.reserved_key TIME-8"
            )],
            "{}",
            checked.report
        );
    }
}

/// TIME-8: observers in both functions do not make two exchanged keys one.
/// The effective key at depth one is safe, so source must leave the cycle to
/// its runtime operation rather than reject the first key.
#[test]
fn two_readers_exchanging_keys_remain_dynamic() {
    let checked = check(&[(
        "m.hgl",
        r#"module m
const fn first(store: GlobalState, depth: i64, const key: str, const next: str) {
    if depth == 0 { put(store, key, 7) }
    else { second(store, depth - 1, next, key) }
}
const fn second(store: GlobalState, depth: i64, const key: str, const next: str) {
    if depth == 0 { put(store, key, 7) }
    else { first(store, depth - 1, next, key) }
}
fn node(const key: str, const next: str) {
    inject global_state
    when {}
    stop { first(global_state, 1, key, next) }
}
test t { eval node(key: "run.output", next: "user.next") }
"#,
    )]);
    assert_eq!(
        checked.summary(),
        Vec::<String>::new(),
        "{}",
        checked.report
    );
}

/// TIME-8, CAP-6: an uncalled exported helper has no fixed entry route.
/// Reaching the same changing cycle must preserve the declaration's admission.
#[test]
fn exported_changing_store_cycle_keeps_runtime_keys_without_an_entry() {
    let helper = r#"module m
export const fn helper(store: GlobalState, depth: i64, const key: str) {
    put(store, key, 1)
    if depth > 0 { helper(store, depth - 1, "run.output") }
}
"#;
    let caller = r#"
fn node() {
    inject global_state
    when {}
    stop { helper(global_state, 1, "safe") }
}
test source { eval node() }
"#;
    for source in [helper.to_owned(), format!("{helper}{caller}")] {
        let checked = check(&[("m.hgl", &source)]);
        assert_eq!(
            checked.summary(),
            Vec::<String>::new(),
            "{}",
            checked.report
        );
    }
}

/// TIME-8, CAP-6: selecting one member does not inspect its coarse-cycle peers.
#[test]
fn unvisited_members_of_a_reached_store_cycle_keep_runtime_keys() {
    let source = r#"module m
operator w<S,T>(store:S, const key:str, value:T) requires S in {GlobalState}
impl const fn w(store:GlobalState, const key:str, value:i64) {
    put(store, key, value)
    relay(store, key + "put", 1.5)
    w(store, "run.output", 1)
}
impl const fn w(store:GlobalState, const key:str, value:f64) { put(store,key,value) }
const fn relay<T>(store:GlobalState, const key:str, value:T) requires T in {i64,f64} { w(store,key,value) }
fn node(const key:str) {
    inject global_state
    when {}
    stop { relay(global_state,key,1.5) }
}
test source { eval node(key:"safe") }
"#;
    for source in [
        source.to_owned(),
        source.replace("relay(global_state,key,1.5)", "relay(global_state,key,1)"),
    ] {
        let checked = check(&[("m.hgl", &source)]);
        assert_eq!(
            checked.summary(),
            Vec::<String>::new(),
            "{}",
            checked.report
        );
    }
}

/// TIME-8, CAP-6: unrelated entry routes cannot change a fixed call's stage.
#[test]
fn an_unused_specialization_does_not_delay_a_fixed_store_key() {
    let source = r#"module m
const fn computed(s: str) -> str => s
operator dispatch_k<S,T>(s: S, depth: i64, const label: str, tag: T) requires S in {GlobalState}
impl const fn dispatch_k(s: GlobalState, depth: i64, const label: str, tag: f64) { put(s, label, tag) }
impl const fn dispatch_k(s: GlobalState, depth: i64, const label: str, tag: i64) {
    put(s, label, tag)
    if depth > 0 { via(s, depth - 1, label + "put", tag) }
}
const fn via<T>(s: GlobalState, depth: i64, const label: str, tag: T) requires T in {f64, i64} {
    dispatch_k(s, depth, label, tag)
}
fn stable(const label: str, const d: i64) {
    inject global_state
    start { let guard = 1 // d }
    when {}
    stop { via(global_state, 2, label, 3.5) }
}
test fixed { eval stable("run.output", 1) }
"#;
    let unused = r#"fn moving(const label: str, const d: i64, const n: i64) {
    inject global_state
    start { let guard = 1 // d }
    when {}
    stop { via(global_state, n, label, 7) }
}
"#;
    for source in [source.to_owned(), format!("{source}{unused}")] {
        let checked = check(&[("m.hgl", &source)]);
        assert_eq!(
            checked
                .summary()
                .iter()
                .filter(|s| s.contains("store.reserved_key"))
                .count(),
            1,
            "{}",
            checked.report
        );
    }
}

/// TIME-8, CAP-6: unrelated entry routes cannot change a fixed call's stage.
#[test]
fn an_unused_distinct_alias_entry_does_not_delay_a_fixed_store_key() {
    let source = r#"module m
const fn copied(s: GlobalState, n: i64, flag: bool, const key: str) {
    put(s, key, n)
    if n > 0 { choose_key(s, n - 1, flag, key, key) }
}
const fn choose_key(s: GlobalState, n: i64, flag: bool, const left: str, const right: str) {
    if flag { copied(s, n, flag, right) } else { copied(s, n, flag, left) }
}
fn sample(const key: str, const divisor: i64, const flag: bool) {
    inject global_state
    start { let guard = 1 // divisor }
    when {}
    stop { copied(global_state, 3, flag, key) }
}
const fn computed(s: str) -> str => s
test fixed { eval sample("run.output", 1, false) }
"#;
    let unused = r#"fn other(const p: str, const q: str) {
    inject global_state
    when {}
    stop { choose_key(global_state, 1, false, p, q) }
}
"#;
    for source in [source.to_owned(), format!("{source}{unused}")] {
        let checked = check(&[("m.hgl", &source)]);
        assert_eq!(
            checked
                .summary()
                .iter()
                .filter(|s| s.contains("store.reserved_key"))
                .count(),
            1,
            "{}",
            checked.report
        );
    }
}
