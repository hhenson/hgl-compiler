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
