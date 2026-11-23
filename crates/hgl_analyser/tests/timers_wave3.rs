//! What the analyser holds a node's scheduler and alarm to: `scheduled()`
//! in `start` and `stop` (TIME-9), and the duration a wake is asked for
//! with (TIME-8, core's CAP-4 and CAP-5).

mod common;

use common::{check, module};

/// TIME-9: "A node that injects neither rejects it (`inject.no_scheduler`),
/// and so do `start` and `stop` (`scheduled.lifecycle`)." Wherever it
/// stands in the hook, under a negation or a condition, with a scheduler
/// or an alarm; and in a node that injects neither, both rules reject
/// it. In a `when` it is the selector, and accepted.
#[test]
fn scheduled_in_start_or_stop_rejects() {
    let checked = module(concat!(
        "fn started(value: i64) -> bool {\n",
        "    state seen: bool = false\n",
        "    inject scheduler\n",
        "    start {\n",
        "        seen = scheduled()\n",
        "        schedule(scheduler, 1s)\n",
        "    }\n",
        "    when { return seen }\n",
        "}\n",
        "fn stopped(value: i64) -> bool {\n",
        "    state seen: bool = false\n",
        "    inject alarm\n",
        "    start { schedule(alarm, 1s) }\n",
        "    when scheduled() { return seen }\n",
        "    stop {\n",
        "        if !scheduled() { seen = true }\n",
        "    }\n",
        "}\n",
        "fn neither(value: i64) -> bool {\n",
        "    state seen: bool = false\n",
        "    inject clock\n",
        "    start { seen = scheduled() && true }\n",
        "    when { return seen }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:6:16 phase/scheduled.lifecycle TIME-9",
            "m.hgl:17:13 phase/scheduled.lifecycle TIME-9",
            "m.hgl:23:20 phase/scheduled.lifecycle TIME-9",
            "m.hgl:23:20 injectable/inject.no_scheduler TIME-9",
        ]
    );
}

/// TIME-8: "A wake asked for with a `duration` takes one more than zero,
/// but in `start`, which takes zero too ... A duration that is not
/// admitted, a negative one in `start` included, rejects where the
/// compiler sees its value, a literal or a constant (UNIT-13)". In a
/// `when` and in `stop` zero and less reject, a literal, a negated one,
/// a constant and a constant expression of durations, in parentheses
/// too; a tag beside it changes nothing. In `start` zero is admitted and
/// less rejects. Core's CAP-5: what may be asked of the alarm is what may
/// be asked of the scheduler, alike.
#[test]
fn a_duration_the_compiler_sees_is_held_to_its_hook() {
    let checked = module(concat!(
        "const ZERO: duration = 0s\n",
        "const BACK: duration = 1s - 2s\n",
        "fn in_when(value: i64) -> i64 {\n",
        "    inject scheduler\n",
        "    when modified(value) {\n",
        "        schedule(scheduler, 0s)\n",
        "        schedule(scheduler, -1s)\n",
        "        schedule(scheduler, ZERO)\n",
        "        schedule(scheduler, BACK, tag: \"t\")\n",
        "        schedule(scheduler, (0s))\n",
        "        schedule(scheduler, 1us)\n",
        "        return value\n",
        "    }\n",
        "}\n",
        "fn alarm_in_when(value: i64) -> i64 {\n",
        "    inject alarm\n",
        "    when modified(value) {\n",
        "        schedule(alarm, 0s)\n",
        "        schedule(alarm, 1ms)\n",
        "        return value\n",
        "    }\n",
        "}\n",
        "fn in_start(value: i64) -> i64 {\n",
        "    inject alarm\n",
        "    start {\n",
        "        schedule(alarm, 0s)\n",
        "        schedule(alarm, ZERO)\n",
        "        schedule(alarm, -1s)\n",
        "        schedule(alarm, BACK)\n",
        "    }\n",
        "    when { return value }\n",
        "}\n",
        "fn scheduler_in_start(value: i64) -> i64 {\n",
        "    inject scheduler\n",
        "    start {\n",
        "        schedule(scheduler, 0s, tag: \"first\")\n",
        "        schedule(scheduler, -1us)\n",
        "    }\n",
        "    when { return value }\n",
        "}\n",
        "fn in_stop(value: i64) -> i64 {\n",
        "    inject scheduler\n",
        "    when { return value }\n",
        "    stop { schedule(scheduler, 0s) }\n",
        "}\n",
        // Nested in the hook: in a branch and in a loop.
        "fn nested(value: i64) -> i64 {\n",
        "    inject alarm\n",
        "    when modified(value) {\n",
        "        if value > 0 { schedule(alarm, 0us) }\n",
        "        for step in [1, 2] { schedule(alarm, -2ms) }\n",
        "        return value\n",
        "    }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:7:29 type/schedule.duration_bounds TIME-8",
            "m.hgl:8:29 type/schedule.duration_bounds TIME-8",
            "m.hgl:9:29 type/schedule.duration_bounds TIME-8",
            "m.hgl:10:29 type/schedule.duration_bounds TIME-8",
            // The value inside the parentheses.
            "m.hgl:11:30 type/schedule.duration_bounds TIME-8",
            "m.hgl:19:25 type/schedule.duration_bounds TIME-8",
            "m.hgl:29:25 type/schedule.duration_bounds TIME-8",
            "m.hgl:30:25 type/schedule.duration_bounds TIME-8",
            "m.hgl:38:29 type/schedule.duration_bounds TIME-8",
            "m.hgl:45:32 type/schedule.duration_bounds TIME-8",
            "m.hgl:50:40 type/schedule.duration_bounds TIME-8",
            "m.hgl:51:46 type/schedule.duration_bounds TIME-8",
        ]
    );
}

/// TIME-8: a duration whose value the compiler does not see "fails the
/// call as the program runs", and is accepted: an input, a `let`, a
/// `const` parameter, whatever its default, and a generic value
/// parameter. So is a wake at a `datetime`, which is dropped where it is
/// before the evaluation time. A value function given the scheduler is
/// held to what no hook admits, a negative duration; whether a zero there
/// is in `start` the rule does not say, and it is accepted for the
/// target to report. Only Core's `schedule` has this timing contract.
#[test]
fn a_duration_known_only_as_the_program_runs_is_accepted() {
    let checked = module(concat!(
        "fn at_run_time(value: i64, delay: duration, const every: duration = 0s) -> i64 {\n",
        "    inject scheduler\n",
        "    start { schedule(scheduler, every) }\n",
        "    when modified(value) {\n",
        "        let none = 0s\n",
        "        schedule(scheduler, delay)\n",
        "        schedule(scheduler, every)\n",
        "        schedule(scheduler, none)\n",
        "        schedule(scheduler, never)\n",
        "        return value\n",
        "    }\n",
        "}\n",
        "fn generic<const D: duration>(value: i64) -> i64 {\n",
        "    inject alarm\n",
        "    when modified(value) {\n",
        "        schedule(alarm, D)\n",
        "        return value\n",
        "    }\n",
        "}\n",
        "const fn helper(scheduler: Scheduler) {\n",
        "    schedule(scheduler, 0s)\n",
        "    schedule(scheduler, -1s)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:23:25 type/schedule.duration_bounds TIME-8"]
    );

    let local = check(&[
        ("core.hgl", "module core\n"),
        (
            "m.hgl",
            concat!(
                "module m\n",
                "struct Ticket { n: i64 }\n",
                "operator schedule(capability: Ticket, at: duration)\n",
                "impl const fn schedule(capability: Ticket, at: duration) {}\n",
                "const fn locally(value: i64) {\n",
                "    schedule(Ticket(n: value), -1s)\n",
                "}\n",
            ),
        ),
    ]);
    assert!(local.summary().is_empty(), "{}", local.report);
}
