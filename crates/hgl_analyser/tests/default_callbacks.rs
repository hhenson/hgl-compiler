//! Omitted defaults reached through a static callable (UNIT-8, FN-17).

mod common;

use common::check;

#[test]
fn invoking_a_bound_callback_can_close_a_default_cycle() {
    let checked = check(&[
        (
            "callback.hgl",
            r#"module callback_cycle
struct Point { x: i64 = apply(read_x) }
const fn apply(f: const fn(i64) -> i64) -> i64 => f(1)
const fn read_x(value: i64) -> i64 => Point().x
"#,
        ),
        (
            "anonymous.hgl",
            r#"module anonymous_cycle
struct Point { x: i64 = apply(fn(n) => Point().x) }
const fn apply(f: const fn(i64) -> i64) -> i64 => f(1)
"#,
        ),
        (
            "relay.hgl",
            r#"module relayed_cycle
struct Point { x: i64 = relay(read_x) }
const fn relay(f: const fn(i64) -> i64) -> i64 => apply(fn(n) => f(n))
const fn apply(f: const fn(i64) -> i64) -> i64 => f(1)
const fn read_x(value: i64) -> i64 => Point().x
"#,
        ),
        (
            "parameter.hgl",
            r#"module parameter_cycle
const fn first(value: i64 = apply(again)) -> i64 => value
const fn apply(f: const fn(i64) -> i64) -> i64 => f(1)
const fn again(value: i64) -> i64 => first()
"#,
        ),
    ]);
    assert_eq!(
        checked.summary(),
        [
            "anonymous.hgl:2:25 name/default.cycle UNIT-8",
            "callback.hgl:2:25 name/default.cycle UNIT-8",
            "parameter.hgl:2:29 name/default.cycle UNIT-8",
            "relay.hgl:2:25 name/default.cycle UNIT-8",
        ],
        "{}",
        checked.report,
    );
    assert!(checked.report.contains("through `relay` and `read_x`"));
}

#[test]
fn only_omitted_arguments_add_the_callbacks_defaults_to_the_cycle() {
    let checked = check(&[
        (
            "omitted.hgl",
            r#"module omitted_callback_argument
struct Point { x: i64 = apply(read_x) }
const fn apply(f: const fn(i64?) -> i64) -> i64 => f()
const fn read_x(value: i64? = Point().x) -> i64 => 17
"#,
        ),
        (
            "supplied.hgl",
            r#"module supplied_callback_argument
struct Point { x: i64 = apply(read_x) }
const fn apply(f: const fn(i64?) -> i64) -> i64 => f(3)
const fn read_x(value: i64? = Point().x) -> i64 => 17
"#,
        ),
    ]);
    assert_eq!(
        checked.summary(),
        [
            "omitted.hgl:2:25 name/default.cycle UNIT-8",
            "omitted.hgl:4:31 name/default.cycle UNIT-8",
        ],
        "{}",
        checked.report,
    );
}

/// FN-18 and UNIT-8: callback argument positions skip configuration defaults.
#[test]
fn callback_arguments_use_ordinary_parameter_slots() {
    let checked = check(&[
        (
            "given.hgl",
            r#"module given_ordinary_slots
struct Point { x: i64 = apply(read_x) }
const fn apply(f: const fn(i64?) -> i64) -> i64 => f(3)
const fn read_x{cfg: i64 = 0}(value: i64? = Point().x) -> i64 => 17
"#,
        ),
        (
            "omitted.hgl",
            r#"module omitted_ordinary_slot
struct Point { x: i64 = apply(read_x) }
const fn apply(f: const fn(i64?) -> i64) -> i64 => f()
const fn read_x{cfg: i64 = 0}(value: i64? = Point().x) -> i64 => 17
"#,
        ),
        (
            "configuration.hgl",
            r#"module omitted_configuration_slot
struct Point { x: i64 = apply(read_x) }
const fn apply(f: const fn(i64?) -> i64) -> i64 => f(3)
const fn read_x{cfg: i64 = Point().x}(value: i64?) -> i64 => 17
"#,
        ),
        (
            "second.hgl",
            r#"module second_ordinary_slot
struct Point { x: i64 = apply(read_x) }
const fn apply(f: const fn(i64?, i64?) -> i64) -> i64 => f(3)
const fn read_x{first: i64 = 0, second: i64 = 1}(a: i64? = Point().x, b: i64? = Point().x) -> i64 => 17
"#,
        ),
        (
            "two.hgl",
            r#"module second_configuration_slot
struct Point { x: i64 = apply(read_x) }
const fn apply(f: const fn(i64?, i64?) -> i64) -> i64 => f(3, 4)
const fn read_x{first: i64 = 0, second: i64 = Point().x}(a: i64?, b: i64?) -> i64 => 17
"#,
        ),
        (
            "both.hgl",
            r#"module both_ordinary_slots
struct Point { x: i64 = apply(read_x) }
const fn apply(f: const fn(i64?, i64?) -> i64) -> i64 => f(3, 4)
const fn read_x{first: i64 = 0, second: i64 = 1}(a: i64? = Point().x, b: i64? = Point().x) -> i64 => 17
"#,
        ),
    ]);
    assert_eq!(
        checked.summary(),
        [
            "configuration.hgl:2:25 name/default.cycle UNIT-8",
            "configuration.hgl:4:28 name/default.cycle UNIT-8",
            "omitted.hgl:2:25 name/default.cycle UNIT-8",
            "omitted.hgl:4:45 name/default.cycle UNIT-8",
            "second.hgl:2:25 name/default.cycle UNIT-8",
            "second.hgl:4:81 name/default.cycle UNIT-8",
            "two.hgl:2:25 name/default.cycle UNIT-8",
            "two.hgl:4:47 name/default.cycle UNIT-8",
        ],
        "{}",
        checked.report,
    );
}

#[test]
fn a_callback_dependency_belongs_to_the_call_that_invokes_it() {
    let checked = check(&[(
        "controls.hgl",
        r#"module callback_controls
struct Point { x: i64 = apply(identity) }
const fn apply(f: const fn(i64) -> i64) -> i64 => f(1)
const fn ignore(f: const fn(i64) -> i64) -> i64 => 17
const fn identity(value: i64) -> i64 => value
const fn read_x(value: i64) -> i64 => Point().x
const fn elsewhere() -> i64 => apply(read_x)

struct Unused { x: i64 = ignore(fn(n) => Unused().x) }

struct Finite { x: i64 = grow(identity, 2) }
const fn grow(f: const fn(i64) -> i64, step: i64) -> i64 =>
    if step == 0 { f(1) } else { grow(fn(n) => f(n + 1), step - 1) }
"#,
    )]);
    assert!(checked.summary().is_empty(), "{}", checked.report);
}
