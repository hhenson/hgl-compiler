// Unit tests of the scheduler and the alarm (capabilities.h; core's CAP-4,
// CAP-5, the language's TIME-8 and TIME-9), on nodes written by hand as the
// compiler writes them, with hgraph's front end for generated code alone,
// each run in an hgraph graph in simulation. Most are core's scheduling
// cases (`tests/conformance/scheduled_wakes.hgl`). Each node keeps the wakes
// it saw in a list of the test's, which the test then reads.
#include "core_nodes.h"

#include <hglrt/capabilities.h>

#include <doctest/doctest.h>
#include <hgraph/types/generated.h>

#include <chrono>
#include <optional>
#include <string>
#include <vector>

namespace {

using hglrt::datetime;
using hglrt::duration;
using hglrt::Hook;
using hglrt::i64;
using hglrt::testing::Dense;
using namespace std::chrono_literals;

const datetime start = hglrt::testing::earliest_start();
/// The same, named where a node's own `start` hides it.
const datetime run_start = start;

/// What a node saw in one evaluation: its time, and its scheduler's
/// `is_scheduled_now`, `is_scheduled` and `next_scheduled_time`.
struct Woken {
    datetime at;
    bool now;
    bool pending;
    std::optional<datetime> next;

    friend bool operator==(const Woken&, const Woken&) = default;
};

std::vector<Woken> woken;

/// Runs a graph of the node `Node`, with the scalar `case_` and no input,
/// for `length`.
template <class Node>
void run_alone(i64 case_, duration length = 10us) {
    woken.clear();
    (void)hglrt::testing::run(
        [case_](hgraph::Wiring& wiring, datetime) { hgraph::wire<Node>(wiring, case_); }, {.length = length});
}

/// The code of the HGL failure the run of `Node` with `case_` fails with,
/// or none.
template <class Node>
std::optional<std::string> run_fails(i64 case_) {
    try {
        run_alone<Node>(case_);
    } catch (const std::exception& failure) {
        const std::optional<hglrt::error> carried = hglrt::carried_error(failure);
        return carried.has_value() ? carried->code.value_or("(no code)") : "(not HGL's)";
    }
    return std::nullopt;
}

/// The scheduler of a node's hook, as the compiler makes it.
hglrt::Scheduler made(hgraph::NodeScheduler view, Hook hook) {
    return hglrt::Scheduler{view, hook};
}

/// Each case's `start`, of `Wakes` below.
void start_wakes(i64 case_, const hglrt::Scheduler& scheduler) {
    using hglrt::fn_reset;
    using hglrt::fn_un_schedule;
    using hglrt::schedule_Scheduler_duration_str__bool;
    switch (case_) {
    case 0: // core's wake_without_inputs(.keep_both)
    case 1: // ... (.cancel_later)
    case 2: // ... (.reset_all)
        schedule_Scheduler_duration_str__bool(scheduler, 1us, "early");
        schedule_Scheduler_duration_str__bool(scheduler, 3us, "late");
        if (case_ == 1) {
            fn_un_schedule(scheduler, "late");
        }
        if (case_ == 2) {
            fn_reset(scheduler);
        }
        break;
    case 3: // two wakes under the empty tag: the second replaces the first
        schedule_Scheduler_duration_str__bool(scheduler, 3us, "");
        schedule_Scheduler_duration_str__bool(scheduler, 1us, "");
        break;
    case 4: // a wake under the empty tag cancelled by it, one with no tag kept
        schedule_Scheduler_duration_str__bool(scheduler, 4us, "");
        schedule_Scheduler_duration_str__bool(scheduler, 2us);
        fn_un_schedule(scheduler, "");
        break;
    case 5: // a wake at the start
        schedule_Scheduler_duration_str__bool(scheduler, 0s);
        break;
    case 6: // three wakes; the first cancels the next
    case 7: // three wakes; the first resets them all
        schedule_Scheduler_duration_str__bool(scheduler, 1us);
        schedule_Scheduler_duration_str__bool(scheduler, 3us);
        schedule_Scheduler_duration_str__bool(scheduler, 5us);
        break;
    case 8: // a wake at the start's own time, as a datetime, on the wall clock
        hglrt::schedule_Scheduler_datetime_str__bool(scheduler, scheduler.view().now(), std::nullopt, true);
        break;
    case 9: // a negative duration in `start`
        schedule_Scheduler_duration_str__bool(scheduler, -1us);
        break;
    case 10: // a wake at the start whose evaluation asks for one of zero
    case 11: // ... for one before now, and one at it, as datetimes
    case 13: // ... for one before now with the wall-clock option
        schedule_Scheduler_duration_str__bool(scheduler, 0s);
        break;
    case 12: // core's wall_clock_wakes
        schedule_Scheduler_duration_str__bool(scheduler, 1us, "early", true);
        hglrt::schedule_Scheduler_datetime_str__bool(scheduler, scheduler.view().now() + 3us, "late", true);
        break;
    default:
        break;
    }
}

/// `fn wakes(const case_: i64) { inject scheduler; start { ... }
/// when { ... } }`: a node with no input, whose scheduler is asked for
/// wakes in `start` as `start_wakes` says, and which keeps what it sees in
/// each evaluation.
struct Wakes {
    static constexpr auto name = "test::wakes";

    static void start(hgraph::Scalar<"case", i64> case_, hgraph::NodeScheduler view) {
        start_wakes(case_.value(), made(view, Hook::start));
    }

    static void eval(hgraph::Scalar<"case", i64> case_, hgraph::NodeScheduler view, hgraph::DateTime now) {
        const hglrt::Scheduler scheduler = made(view, Hook::evaluation);
        if (case_.value() == 6 && now == run_start + 1us) {
            hglrt::fn_un_schedule(scheduler);
        }
        if (case_.value() == 7 && now == run_start + 1us) {
            hglrt::fn_reset(scheduler);
        }
        if (case_.value() == 10) {
            hglrt::schedule_Scheduler_duration_str__bool(scheduler, 0s);
        }
        if (case_.value() == 11) {
            hglrt::schedule_Scheduler_datetime_str__bool(scheduler, now - 1us);
            hglrt::schedule_Scheduler_datetime_str__bool(scheduler, now, "at now");
        }
        if (case_.value() == 13) {
            hglrt::schedule_Scheduler_datetime_str__bool(scheduler, now - 1us, std::nullopt, true);
        }
        woken.push_back(Woken{now, scheduler.is_scheduled_now(), scheduler.is_scheduled(),
                              scheduler.next_scheduled_time()});
    }
};

TEST_CASE("a reset removes every wake, and cancelling one tag keeps the other (CAP-4)") {
    // core's a_reset_removes_all_wakes_from_an_inputless_node and
    // cancelling_the_later_tag_preserves_the_earlier_wake.
    run_alone<Wakes>(0);
    CHECK(woken == std::vector<Woken>{{start + 1us, true, true, start + 3us},
                                      {start + 3us, true, false, std::nullopt}});
    run_alone<Wakes>(1);
    CHECK(woken == std::vector<Woken>{{start + 1us, true, false, std::nullopt}});
    // hgraph evaluates the node at the time it was armed for, the earliest
    // wake asked for, though the wake was cancelled: no wake is due in that
    // evaluation, which the generated code takes for none (capabilities.h).
    run_alone<Wakes>(2);
    CHECK(woken == std::vector<Woken>{{start + 1us, false, false, std::nullopt}});
}

TEST_CASE("the empty text is a tag as any other is (CAP-4)") {
    // CAP-4: the tag is "a `str?` that is null when omitted"; a wake asked
    // for under a tag replaces the pending wake of that tag.
    run_alone<Wakes>(3);
    CHECK(woken == std::vector<Woken>{{start + 1us, true, false, std::nullopt}});
    run_alone<Wakes>(4);
    CHECK(woken == std::vector<Woken>{{start + 2us, true, false, std::nullopt}});
}

TEST_CASE("a wake being delivered is pending no longer, and stays due (CAP-4, TIME-9)") {
    // "`un_schedule(scheduler)` ... cancel[s] a pending wake": in a wake,
    // the next one.
    run_alone<Wakes>(6);
    CHECK(woken == std::vector<Woken>{{start + 1us, true, true, start + 5us},
                                      {start + 5us, true, false, std::nullopt}});
    // core's a_wake_stays_due_after_a_reset_inside_it: "it stays true for
    // the rest of that evaluation after `un_schedule` or `reset` in it".
    run_alone<Wakes>(7);
    CHECK(woken == std::vector<Woken>{{start + 1us, true, false, std::nullopt}});
}

TEST_CASE("a zero duration in start wakes the node's first evaluation (TIME-8, CAP-4)") {
    // core's a_zero_duration_in_start_wakes_the_first_evaluation.
    run_alone<Wakes>(5);
    CHECK(woken == std::vector<Woken>{{start, true, false, std::nullopt}});
    // A datetime at the start's time asks for a wake then too, and the wall
    // clock is the engine's in simulation.
    run_alone<Wakes>(8);
    CHECK(woken == std::vector<Woken>{{start, true, false, std::nullopt}});
}

TEST_CASE("in simulation the wall-clock option changes nothing (CAP-4)") {
    // core's the_wall_clock_option_changes_nothing_in_simulation: "the
    // duration 1us after the start, and the datetime at 3us from it".
    run_alone<Wakes>(12);
    CHECK(woken == std::vector<Woken>{{start + 1us, true, true, start + 3us},
                                      {start + 3us, true, false, std::nullopt}});
    // A datetime before the evaluation time is dropped, as one asked for
    // without the option.
    run_alone<Wakes>(13);
    CHECK(woken == std::vector<Woken>{{start, true, false, std::nullopt}});
}

/// `fn wall_clock_late() { inject scheduler, clock; start { schedule(
/// scheduler, 0s) } when { ... } }`, run in real time: its first evaluation
/// asks for a wake at a wall-clock time already past, and its second stops
/// the run.
struct WallClockLate {
    static constexpr auto name = "test::wall_clock_late";

    static void start(hgraph::NodeScheduler view) {
        hglrt::schedule_Scheduler_duration_str__bool(made(view, Hook::start), 0s);
    }

    static void eval(hgraph::NodeScheduler view, hgraph::EvaluationClockView clock_view,
                     hgraph::EngineControlView engine, hgraph::DateTime now) {
        const hglrt::Scheduler scheduler = made(view, Hook::evaluation);
        const hglrt::Clock clock{clock_view};
        woken.push_back(Woken{now, scheduler.is_scheduled_now(), false, std::nullopt});
        if (woken.size() == 1) {
            hglrt::schedule_Scheduler_datetime_str__bool(scheduler, clock.now() - 1ms, std::nullopt, true);
        } else {
            engine.request_stop();
        }
    }
};

TEST_CASE("in real time a wall-clock wake already due comes in the next cycle (CAP-4)") {
    // CAP-4: "A wake of the option that is already due where it is asked
    // for ... is not dropped: it comes in the next cycle, at the later of
    // that time and the evaluation time plus the engine's smallest step".
    // The second evaluation stops the run, which its length only bounds.
    woken.clear();
    (void)hglrt::testing::run([](hgraph::Wiring& wiring, datetime) { hgraph::wire<WallClockLate>(wiring); },
                              {.length = 10s}, hglrt::testing::Mode::real_time);
    REQUIRE(woken.size() == 2);
    CHECK(woken[1].now);
    CHECK(woken[1].at > woken[0].at);
}

TEST_CASE("a duration not admitted fails, and a datetime past is dropped (TIME-8, CAP-4)") {
    // core's a_zero_duration_asked_of_the_scheduler_fails: "A duration that
    // is not admitted, a negative one in `start` included, ... fails the call
    // as the program runs, with `schedule.non_positive_duration`".
    CHECK(run_fails<Wakes>(9) == "schedule.non_positive_duration");
    CHECK(run_fails<Wakes>(10) == "schedule.non_positive_duration");
    // core's a_wake_before_now_is_dropped: "there is no wake, and no failure";
    // one at the evaluation time no cycle can still run at.
    CHECK(run_fails<Wakes>(11) == std::nullopt);
    CHECK(woken == std::vector<Woken>{{start, true, false, std::nullopt}});
}

/// `fn alarmed(value: i64, const case_: i64) { inject alarm; start
/// { ... } when { ... } }`, whose input is silent unless its case gives it
/// a trace.
struct Alarmed {
    static constexpr auto name = "test::alarmed";

    static void start(hgraph::Scalar<"case", i64> case_, hgraph::NodeScheduler view) {
        const hglrt::Alarm alarm{view, Hook::start};
        switch (case_.value()) {
        case 0: // asking again keeps the earliest, whichever is first
            hglrt::schedule_Alarm_duration(alarm, 3us);
            hglrt::schedule_Alarm_duration(alarm, 1us);
            break;
        case 1:
            hglrt::schedule_Alarm_duration(alarm, 1us);
            hglrt::schedule_Alarm_datetime(alarm, run_start + 3us);
            break;
        case 2: // a negative duration in `start`
            hglrt::schedule_Alarm_duration(alarm, -1us);
            break;
        case 5: // a wake the input's tick comes before, or lands with
            hglrt::schedule_Alarm_duration(alarm, 3us);
            break;
        case 7: // a wake asked for at each tick of the input, in `eval`
            break;
        default: // a wake at the start
            hglrt::schedule_Alarm_duration(alarm, 0s);
            break;
        }
    }

    static void eval(hgraph::In<"value", hgraph::TS<i64>, hgraph::InputValidity::Unchecked> value,
                     hgraph::Scalar<"case", i64> case_, hgraph::NodeScheduler view, hgraph::DateTime now) {
        const hglrt::Alarm alarm{view, Hook::evaluation};
        if (case_.value() == 7 && value.modified()) {
            hglrt::schedule_Alarm_duration(alarm, 3us);
        }
        if (case_.value() == 3) {
            hglrt::schedule_Alarm_duration(alarm, 0s);
        }
        if (case_.value() == 4) {
            // core's alarm_in_the_past, and one at the evaluation time.
            hglrt::schedule_Alarm_datetime(alarm, now - 1us);
            hglrt::schedule_Alarm_datetime(alarm, now);
        }
        woken.push_back(Woken{now, false, false, std::nullopt});
    }
};

/// Runs a graph of `Alarmed` with `case_`, its input replaying `value`.
void run_alarmed(i64 case_, const Dense<i64>& value = {}) {
    woken.clear();
    (void)hglrt::testing::run(
        [case_, value](hgraph::Wiring& wiring, datetime) {
            auto replayed =
                hgraph::wire<hglrt_test::Replay>(wiring, hglrt::testing::replay_items(value, start));
            hgraph::wire<Alarmed>(wiring, replayed, case_);
        },
        {.length = 10us});
}

TEST_CASE("an alarm keeps the earliest wake asked of it (CAP-5)") {
    run_alarmed(0);
    CHECK(woken == std::vector<Woken>{{start + 1us, false, false, std::nullopt}});
    run_alarmed(1);
    CHECK(woken == std::vector<Woken>{{start + 1us, false, false, std::nullopt}});
    // core's a_zero_duration_in_start_wakes_the_alarm_at_the_first_evaluation.
    run_alarmed(6, Dense<i64>{std::nullopt, 7});
    CHECK(woken ==
          std::vector<Woken>{{start, false, false, std::nullopt}, {start + 1us, false, false, std::nullopt}});
}

TEST_CASE("an alarm is asked what the scheduler is (CAP-5, TIME-8)") {
    hglrt::testing::raises("schedule.non_positive_duration", [] { run_alarmed(2); });
    hglrt::testing::raises("schedule.non_positive_duration", [] { run_alarmed(3); });
    // A datetime before the evaluation time is dropped, which hgraph's graph
    // would refuse, and one at it too.
    run_alarmed(4);
    CHECK(woken == std::vector<Woken>{{start, false, false, std::nullopt}});
}

TEST_CASE("an alarm keeps its wake when an input's tick evaluates the node first (CAP-5)") {
    // CAP-5: "`schedule(alarm, at)` asks for a wake ...; asking again keeps
    // the earliest": an evaluation at an input's tick before the wake is due
    // takes nothing away from it, asked in `start` or in an evaluation.
    run_alarmed(5, Dense<i64>{std::nullopt, 7});
    CHECK(woken == std::vector<Woken>{{start + 1us, false, false, std::nullopt},
                                      {start + 3us, false, false, std::nullopt}});
    run_alarmed(7, Dense<i64>{7});
    CHECK(woken ==
          std::vector<Woken>{{start, false, false, std::nullopt}, {start + 3us, false, false, std::nullopt}});
    // Asked again at the second tick, for 4us: the earliest, 3us, is kept.
    run_alarmed(7, Dense<i64>{7, 8});
    CHECK(woken == std::vector<Woken>{{start, false, false, std::nullopt},
                                      {start + 1us, false, false, std::nullopt},
                                      {start + 3us, false, false, std::nullopt}});
    // A wake that lands with the input's tick is one evaluation, and spent.
    run_alarmed(5, Dense<i64>{7, std::nullopt, std::nullopt, 9});
    CHECK(woken ==
          std::vector<Woken>{{start, false, false, std::nullopt}, {start + 3us, false, false, std::nullopt}});
}

} // namespace
