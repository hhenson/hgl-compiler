//! The analyser against the owner's decisions of 2026-11-20, written
//! into the pinned specification and core: the code of a wake's duration
//! the compiler sees (TIME-8), a test of `modified` as a guard (TIME-4,
//! TIME-5), a part of `out` of a `ref` result and a `signal` marked `ref`
//! (FN-16, TYPE-12), and a window read where a `result` is taken (ERR-14,
//! TYPE-11). Each test cites its rule.
//!
//! A finding is written `file:line:column category/code rule`.

mod common;

use common::module;

/// TIME-8: "A duration that is not admitted, a negative one in `start`
/// included, rejects where the compiler sees its value, a literal or a
/// constant (UNIT-13), with `schedule.duration_bounds`, and elsewhere
/// fails the call as the program runs, with
/// `schedule.non_positive_duration`." The catalogue's row: "A wake asked
/// for with a `duration` that is not more than zero, other than a zero in
/// `start`, whose value the compiler sees (TIME-8)". A literal and a
/// constant, of the scheduler and of the alarm, in a `when`, in `start`
/// and in `stop`, and a negative one in a value function given the
/// scheduler; a duration the compiler does not see is accepted.
#[test]
fn a_duration_the_compiler_sees_rejects_with_duration_bounds() {
    let checked = module(concat!(
        "const ZERO: duration = 0s\n",
        "fn in_when(value: i64, delay: duration) -> i64 {\n",
        "    inject scheduler\n",
        "    when modified(value) {\n",
        "        schedule(scheduler, 0s)\n",
        "        schedule(scheduler, ZERO)\n",
        "        schedule(scheduler, delay)\n",
        "        return value\n",
        "    }\n",
        "}\n",
        "fn in_start(value: i64) -> i64 {\n",
        "    inject alarm\n",
        "    start {\n",
        "        schedule(alarm, 0s)\n",
        "        schedule(alarm, -1s)\n",
        "    }\n",
        "    when { return value }\n",
        "    stop { schedule(alarm, ZERO) }\n",
        "}\n",
        "const fn helper(scheduler: Scheduler) {\n",
        "    schedule(scheduler, -1us)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:6:29 type/schedule.duration_bounds TIME-8",
            "m.hgl:7:29 type/schedule.duration_bounds TIME-8",
            "m.hgl:16:25 type/schedule.duration_bounds TIME-8",
            "m.hgl:19:28 type/schedule.duration_bounds TIME-8",
            "m.hgl:22:25 type/schedule.duration_bounds TIME-8",
        ]
    );
}
