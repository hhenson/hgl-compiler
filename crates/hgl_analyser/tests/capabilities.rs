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
    "operator add_<L, R, O>(lhs: L, rhs: R) -> O\n",
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
            // A tag on an alarm, a number for a time, a key that is no
            // string, and a key that is a value of the evaluation.
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
        ]
    );
    assert_eq!(checked.prelude_names("m"), [""; 0]);
    assert!(locals(&checked, "counted").contains(&"total: i64".to_string()));
    assert!(locals(&checked, "lookup").contains(&"price: i64?".to_string()));
}
