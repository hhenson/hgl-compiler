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

use common::{Checked, check};

/// No lines.
const NONE: [&str; 0] = [];

/// A core with the capability types and one operation on a logger.
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
    "export const fn log_info(logger: Logger, message: str) {\n",
    "    log(logger, .info, message)\n",
    "}\n",
);

/// Checks `core.hgl`, the core above, with one file, `m.hgl`, of module
/// `m`. The `module` line is added, so the text given starts on line 2.
fn module(text: &str) -> Checked {
    check(&[("core.hgl", CORE), ("m.hgl", &format!("module m\n{text}"))])
}

/// The locals of the function `name` of module `m`, as `local: type`.
fn locals(checked: &Checked, name: &str) -> Vec<String> {
    let module = checked.program.module("m").expect("module m exists");
    let index = module
        .declarations
        .iter()
        .position(|declaration| declaration.name == name)
        .expect("the declaration exists");
    module.bodies[index]
        .as_ref()
        .expect("it has a body")
        .locals
        .iter()
        .map(|local| format!("{}: {}", local.name, local.ty))
        .collect()
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
        ]
    );
}

/// NATIVE-6: "A native type may declare read-only properties, each a
/// name and an ordinary value type, written as a struct's typed fields
/// are. `value.property` reads one as an ordinary value." Two of one
/// name reject, as two fields of a struct do (TYPE-4). TYPE-12: "An
/// ordinary value type is written with no `rolling`, `ref`, `atomic`,
/// or `signal` at any depth".
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
            "    feed: ref<f64>\n",
            "}\n",
            "native type Opaque\n",
            "const fn read(handle: Handle, opaque: Opaque) -> i64 {\n",
            "    let label = handle.label\n",
            "    let span = handle.time\n",
            "    let missing = handle.width\n",
            "    let nothing = opaque.size\n",
            "    return handle.size\n",
            "}\n",
        ),
    )]);
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:6:5 type/field.duplicate NATIVE-6",
            "m.hgl:7:11 type/type.not_ordinary TYPE-12",
            "m.hgl:13:26 name/member.unknown NATIVE-6",
            "m.hgl:14:26 name/member.unknown NATIVE-6",
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
