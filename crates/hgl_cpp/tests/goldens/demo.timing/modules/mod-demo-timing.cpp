// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include "mod-demo-timing.h"
#include "mod-core.h"

// Names this file writes, which no macro of a header above takes.
#undef Alarm
#undef Clock
#undef Logger
#undef Scheduler
#undef a_resumed_source_keeps_its_locals_until_it_returns
#undef a_run_configuration_is_given_in_braces
#undef a_scheduler_wakes_a_node_with_no_input
#undef a_source_keeps_its_cache_and_state
#undef a_source_reads_its_clock_when_it_runs
#undef a_wake_before_now_is_dropped
#undef a_wake_can_coincide_with_an_input
#undef a_zero_duration_outside_start_fails
#undef absolute_targets_before_at_and_after_the_start
#undef absolute_yields
#undef alarm
#undef alarm_asked_in_start
#undef alarm_asked_on_each
#undef alarm_asked_once
#undef alarm_beside_a_passive_input
#undef alarm_or_input
#undef alarmed
#undef an_alarm_beside_a_passive_input
#undef an_alarm_keeps_its_wake_when_an_input_ticks_first
#undef an_alarm_wake_that_lands_with_an_input_is_the_inputs
#undef an_alarm_wakes_a_node
#undef an_empty_selection_is_true_where_it_is_selected
#undef asked
#undef asking_an_alarm_again_keeps_the_earliest
#undef at
#undef cancel_next
#undef cause_of_wake
#undef clock
#undef core
#undef count
#undef counter
#undef counting_from
#undef delay
#undef demo
#undef done
#undef earliest_start
#undef empty_elsewhere
#undef empty_selected
#undef every
#undef finish_early
#undef finish_or_resume
#undef first
#undef get
#undef limit
#undef log_debug
#undef logger
#undef never
#undef next
#undef opening
#undef opening_at
#undef pending
#undef record
#undef repeated
#undef replay
#undef reset
#undef scheduled_is_read_as_a_value
#undef scheduled_on_input
#undef scheduler
#undef second
#undef shortened
#undef the_wake_delivered_is_pending_no_longer
#undef ticked
#undef ticker
#undef time_before_each_delay
#undef times
#undef timing
#undef value
#undef wake_after
#undef wake_in_the_past
#undef woken

namespace hgl::demo::timing::m_
{

namespace n_::ticker
{
// ticker(const every: duration, const times: i64) -> i64 (node), module demo.timing, line 10
struct impl_
{
    static constexpr auto name = "demo.timing::ticker";

    // The node's `state` and `cache` (TIME-6).
    struct state_type_
    {
        ::hgraph::Int count{};
    };

    static void start([[maybe_unused]] ::hgraph::Scalar<"every", ::hgraph::TimeDelta> every, [[maybe_unused]] ::hgraph::Scalar<"times", ::hgraph::Int> times, [[maybe_unused]] ::hgraph::State<state_type_> state_, [[maybe_unused]] ::hgraph::NodeScheduler capability_03_)
    {
        state_.set(state_type_{});
        state_type_& kept_ = state_.modify();
        [[maybe_unused]] auto& count = kept_.count;
        [[maybe_unused]] const ::hglrt::Scheduler scheduler{capability_03_, ::hglrt::Hook::start};
        count = 0;
        ::hglrt::schedule_Scheduler_duration_str__bool(scheduler, every.value(), ::hgraph::Str{"tick"}, false);
    }

    static void eval([[maybe_unused]] ::hgraph::Scalar<"every", ::hgraph::TimeDelta> every, [[maybe_unused]] ::hgraph::Scalar<"times", ::hgraph::Int> times, [[maybe_unused]] ::hgraph::State<state_type_> state_, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_, [[maybe_unused]] ::hgraph::NodeScheduler capability_03_)
    {
        state_type_& kept_ = state_.modify();
        [[maybe_unused]] auto& count = kept_.count;
        [[maybe_unused]] const ::hglrt::Scheduler scheduler{capability_03_, ::hglrt::Hook::evaluation};
        // TIME-9: what woke the node, read as it is entered.
        [[maybe_unused]] const bool timer_woke_ = scheduler.is_scheduled_now();
        [[maybe_unused]] const bool input_woke_ = false;
        // when, line 14 (TIME-3)
        if (timer_woke_)
        {
            count = ::hglrt::add__i64_i64(count, 1);
            if (::hglrt::lt__T_T<::hgraph::Int>(count, times.value()))
            {
                ::hglrt::schedule_Scheduler_duration_str__bool(scheduler, every.value(), ::hgraph::Str{"tick"}, false);
            }
            out_.set(count);
            return;
        }
    }
};
}  // namespace n_::ticker
}  // namespace hgl::demo::timing::m_

// The state of the node, as hgraph holds it (TIME-6).
template <>
struct hgraph::scalar_descriptor<::hgl::demo::timing::m_::n_::ticker::impl_::state_type_>
{
    [[nodiscard]] static constexpr bool is_concrete() noexcept { return true; }
    [[nodiscard]] static const ValueTypeMetaData* value_meta()
    {
        return TypeRegistry::instance().register_scalar<::hgl::demo::timing::m_::n_::ticker::impl_::state_type_>("hgl:demo.timing::ticker.state");
    }
};

namespace hgl::demo::timing::m_
{
namespace w_
{
// ticker(const every: duration, const times: i64) -> i64 (node), module demo.timing, line 10
::hgraph::Port<::hgraph::TS<::hgraph::Int>> ticker(::hgraph::Wiring& wiring_, ::hgraph::TimeDelta every, ::hgraph::Int times)
{
    return ::hgraph::wire<::hgl::demo::timing::m_::n_::ticker::impl_>(wiring_, every, times);
}
}  // namespace w_

namespace n_::cause_of_wake
{
// cause_of_wake(value: i64) -> str (node), module demo.timing, line 23
struct impl_
{
    static constexpr auto name = "demo.timing::cause_of_wake";

    static void start([[maybe_unused]] ::hgraph::NodeScheduler capability_01_)
    {
        [[maybe_unused]] const ::hglrt::Scheduler scheduler{capability_01_, ::hglrt::Hook::start};
        ::hglrt::schedule_Scheduler_duration_str__bool(scheduler, ::hgraph::TimeDelta{2}, ::hgraph::Str{"with input"}, false);
        ::hglrt::schedule_Scheduler_duration_str__bool(scheduler, ::hgraph::TimeDelta{4}, ::hgraph::Str{"alone"}, false);
    }

    static void eval([[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>, ::hgraph::InputValidity::Unchecked> value, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Str>> out_, [[maybe_unused]] ::hgraph::NodeScheduler capability_01_)
    {
        [[maybe_unused]] const ::hglrt::Scheduler scheduler{capability_01_, ::hglrt::Hook::evaluation};
        // TIME-9: what woke the node, read as it is entered.
        [[maybe_unused]] const bool timer_woke_ = scheduler.is_scheduled_now();
        [[maybe_unused]] const bool input_woke_ = ::hglrt::ticked(value);
        // when, line 29 (TIME-3)
        if (timer_woke_)
        {
            if (::hglrt::fn_modified(value))
            {
                out_.set(::hgraph::Str{"input and scheduler"});
                return;
            }
            out_.set(::hgraph::Str{"scheduler"});
            return;
        }
        // when, line 33 (TIME-3)
        if (::hglrt::fn_valid(value) && ::hglrt::fn_modified(value))
        {
            out_.set(::hgraph::Str{"input"});
            return;
        }
    }
};
}  // namespace n_::cause_of_wake
namespace w_
{
// cause_of_wake(value: i64) -> str (node), module demo.timing, line 23
::hgraph::Port<::hgraph::TS<::hgraph::Str>> cause_of_wake(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgraph::wire<::hgl::demo::timing::m_::n_::cause_of_wake::impl_>(wiring_, value);
}
}  // namespace w_

namespace n_::empty_selected
{
// empty_selected(value: i64) -> bool (node), module demo.timing, line 37
struct impl_
{
    static constexpr auto name = "demo.timing::empty_selected";

    static void start([[maybe_unused]] ::hgraph::NodeScheduler capability_01_)
    {
        [[maybe_unused]] const ::hglrt::Scheduler scheduler{capability_01_, ::hglrt::Hook::start};
        ::hglrt::schedule_Scheduler_duration_str__bool(scheduler, ::hgraph::TimeDelta{2}, ::std::nullopt, false);
    }

    static void eval([[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>, ::hgraph::InputActivity::Passive, ::hgraph::InputValidity::Unchecked> value, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Bool>> out_, [[maybe_unused]] ::hgraph::NodeScheduler capability_01_)
    {
        [[maybe_unused]] const ::hglrt::Scheduler scheduler{capability_01_, ::hglrt::Hook::evaluation};
        // TIME-9: what woke the node, read as it is entered.
        [[maybe_unused]] const bool timer_woke_ = scheduler.is_scheduled_now();
        [[maybe_unused]] const bool input_woke_ = ::hglrt::ticked(value);
        // when, line 40 (TIME-3)
        if (timer_woke_)
        {
            out_.set(true);
            return;
        }
    }
};
}  // namespace n_::empty_selected
namespace w_
{
// empty_selected(value: i64) -> bool (node), module demo.timing, line 37
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> empty_selected(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgraph::wire<::hgl::demo::timing::m_::n_::empty_selected::impl_>(wiring_, value);
}
}  // namespace w_

namespace n_::empty_elsewhere
{
// empty_elsewhere(value: i64) -> bool (node), module demo.timing, line 44
struct impl_
{
    static constexpr auto name = "demo.timing::empty_elsewhere";

    static void start([[maybe_unused]] ::hgraph::NodeScheduler capability_01_)
    {
        [[maybe_unused]] const ::hglrt::Scheduler scheduler{capability_01_, ::hglrt::Hook::start};
        ::hglrt::schedule_Scheduler_duration_str__bool(scheduler, ::hgraph::TimeDelta{2}, ::hgraph::Str{"with input"}, false);
        ::hglrt::schedule_Scheduler_duration_str__bool(scheduler, ::hgraph::TimeDelta{4}, ::hgraph::Str{"alone"}, false);
    }

    static void eval([[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>, ::hgraph::InputValidity::Unchecked> value, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Bool>> out_, [[maybe_unused]] ::hgraph::NodeScheduler capability_01_)
    {
        [[maybe_unused]] const ::hglrt::Scheduler scheduler{capability_01_, ::hglrt::Hook::evaluation};
        // TIME-9: what woke the node, read as it is entered.
        [[maybe_unused]] const bool timer_woke_ = scheduler.is_scheduled_now();
        [[maybe_unused]] const bool input_woke_ = ::hglrt::ticked(value);
        // when, line 50 (TIME-3)
        if (timer_woke_)
        {
            out_.set(!(::hglrt::fn_modified(value)));
            return;
        }
    }
};
}  // namespace n_::empty_elsewhere
namespace w_
{
// empty_elsewhere(value: i64) -> bool (node), module demo.timing, line 44
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> empty_elsewhere(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgraph::wire<::hgl::demo::timing::m_::n_::empty_elsewhere::impl_>(wiring_, value);
}
}  // namespace w_

namespace n_::scheduled_on_input
{
// scheduled_on_input(value: i64) -> bool (node), module demo.timing, line 54
struct impl_
{
    static constexpr auto name = "demo.timing::scheduled_on_input";

    static void start([[maybe_unused]] ::hgraph::NodeScheduler capability_01_)
    {
        [[maybe_unused]] const ::hglrt::Scheduler scheduler{capability_01_, ::hglrt::Hook::start};
        ::hglrt::schedule_Scheduler_duration_str__bool(scheduler, ::hgraph::TimeDelta{2}, ::std::nullopt, false);
    }

    static void eval([[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Bool>> out_, [[maybe_unused]] ::hgraph::NodeScheduler capability_01_)
    {
        [[maybe_unused]] const ::hglrt::Scheduler scheduler{capability_01_, ::hglrt::Hook::evaluation};
        // TIME-9: what woke the node, read as it is entered.
        [[maybe_unused]] const bool timer_woke_ = scheduler.is_scheduled_now();
        [[maybe_unused]] const bool input_woke_ = ::hglrt::ticked(value);
        // when, line 57 (TIME-3)
        if (input_woke_)
        {
            out_.set(timer_woke_);
            return;
        }
    }
};
}  // namespace n_::scheduled_on_input
namespace w_
{
// scheduled_on_input(value: i64) -> bool (node), module demo.timing, line 54
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> scheduled_on_input(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgraph::wire<::hgl::demo::timing::m_::n_::scheduled_on_input::impl_>(wiring_, value);
}
}  // namespace w_

namespace n_::pending
{
// pending() -> bool (node), module demo.timing, line 62
struct impl_
{
    static constexpr auto name = "demo.timing::pending";

    static void start([[maybe_unused]] ::hgraph::NodeScheduler capability_00_)
    {
        [[maybe_unused]] const ::hglrt::Scheduler scheduler{capability_00_, ::hglrt::Hook::start};
        ::hglrt::schedule_Scheduler_duration_str__bool(scheduler, ::hgraph::TimeDelta{1}, ::std::nullopt, false);
        ::hglrt::schedule_Scheduler_duration_str__bool(scheduler, ::hgraph::TimeDelta{3}, ::std::nullopt, false);
    }

    static void eval([[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Bool>> out_, [[maybe_unused]] ::hgraph::NodeScheduler capability_00_)
    {
        [[maybe_unused]] const ::hglrt::Scheduler scheduler{capability_00_, ::hglrt::Hook::evaluation};
        // TIME-9: what woke the node, read as it is entered.
        [[maybe_unused]] const bool timer_woke_ = scheduler.is_scheduled_now();
        [[maybe_unused]] const bool input_woke_ = false;
        // when, line 68 (TIME-3)
        if (timer_woke_)
        {
            out_.set((scheduler).is_scheduled());
            return;
        }
    }
};
}  // namespace n_::pending
namespace w_
{
// pending() -> bool (node), module demo.timing, line 62
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> pending(::hgraph::Wiring& wiring_)
{
    return ::hgraph::wire<::hgl::demo::timing::m_::n_::pending::impl_>(wiring_);
}
}  // namespace w_

namespace n_::cancel_next
{
// cancel_next() -> bool (node), module demo.timing, line 72
struct impl_
{
    static constexpr auto name = "demo.timing::cancel_next";

    // The node's `state` and `cache` (TIME-6).
    struct state_type_
    {
        ::hgraph::Int woken{};
    };

    static void start([[maybe_unused]] ::hgraph::State<state_type_> state_, [[maybe_unused]] ::hgraph::NodeScheduler capability_01_)
    {
        state_.set(state_type_{});
        state_type_& kept_ = state_.modify();
        [[maybe_unused]] auto& woken = kept_.woken;
        [[maybe_unused]] const ::hglrt::Scheduler scheduler{capability_01_, ::hglrt::Hook::start};
        woken = 0;
        ::hglrt::schedule_Scheduler_duration_str__bool(scheduler, ::hgraph::TimeDelta{1}, ::std::nullopt, false);
        ::hglrt::schedule_Scheduler_duration_str__bool(scheduler, ::hgraph::TimeDelta{3}, ::std::nullopt, false);
        ::hglrt::schedule_Scheduler_duration_str__bool(scheduler, ::hgraph::TimeDelta{5}, ::std::nullopt, false);
    }

    static void eval([[maybe_unused]] ::hgraph::State<state_type_> state_, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Bool>> out_, [[maybe_unused]] ::hgraph::NodeScheduler capability_01_)
    {
        state_type_& kept_ = state_.modify();
        [[maybe_unused]] auto& woken = kept_.woken;
        [[maybe_unused]] const ::hglrt::Scheduler scheduler{capability_01_, ::hglrt::Hook::evaluation};
        // TIME-9: what woke the node, read as it is entered.
        [[maybe_unused]] const bool timer_woke_ = scheduler.is_scheduled_now();
        [[maybe_unused]] const bool input_woke_ = false;
        // when, line 80 (TIME-3)
        if (timer_woke_)
        {
            woken = ::hglrt::add__i64_i64(woken, 1);
            if (::hglrt::eq__i64_i64_f64_f64(woken, 1, 0.0, 0.0))
            {
                ::hglrt::fn_un_schedule(scheduler, ::std::nullopt);
            }
            out_.set((scheduler).is_scheduled_now());
            return;
        }
    }
};
}  // namespace n_::cancel_next
}  // namespace hgl::demo::timing::m_

// The state of the node, as hgraph holds it (TIME-6).
template <>
struct hgraph::scalar_descriptor<::hgl::demo::timing::m_::n_::cancel_next::impl_::state_type_>
{
    [[nodiscard]] static constexpr bool is_concrete() noexcept { return true; }
    [[nodiscard]] static const ValueTypeMetaData* value_meta()
    {
        return TypeRegistry::instance().register_scalar<::hgl::demo::timing::m_::n_::cancel_next::impl_::state_type_>("hgl:demo.timing::cancel_next.state");
    }
};

namespace hgl::demo::timing::m_
{
namespace w_
{
// cancel_next() -> bool (node), module demo.timing, line 72
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> cancel_next(::hgraph::Wiring& wiring_)
{
    return ::hgraph::wire<::hgl::demo::timing::m_::n_::cancel_next::impl_>(wiring_);
}
}  // namespace w_

namespace n_::wake_in_the_past
{
// wake_in_the_past(value: i64) -> str (node), module demo.timing, line 88
struct impl_
{
    static constexpr auto name = "demo.timing::wake_in_the_past";

    // The node's `state` and `cache` (TIME-6).
    struct state_type_
    {
        ::hglrt::DroppedWakes dropped_wakes_{};
    };

    static void start([[maybe_unused]] ::hgraph::State<state_type_> state_)
    {
        state_.set(state_type_{});
    }

    using signature_args = ::std::tuple<::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>, ::hgraph::InputValidity::Unchecked>, ::hgraph::Out<::hgraph::TS<::hgraph::Str>>, ::hgraph::NodeScheduler, ::hgraph::NodeView, ::hgraph::State<state_type_>>;

    static void eval([[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>, ::hgraph::InputValidity::Unchecked> value, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Str>> out_, [[maybe_unused]] ::hgraph::NodeScheduler capability_01_, [[maybe_unused]] ::hgraph::NodeView wake_node_)
    {
        [[maybe_unused]] const ::hglrt::Scheduler scheduler{capability_01_, ::hglrt::Hook::evaluation, ::hglrt::DroppedWakeLog::bind<state_type_, &state_type_::dropped_wakes_>(wake_node_)};
        // TIME-9: what woke the node, read as it is entered.
        [[maybe_unused]] const bool timer_woke_ = scheduler.is_scheduled_now();
        [[maybe_unused]] const bool input_woke_ = ::hglrt::ticked(value);
        // when, line 90 (TIME-3)
        if (timer_woke_)
        {
            out_.set(::hgraph::Str{"woken"});
            return;
        }
        // when, line 91 (TIME-3)
        if (::hglrt::fn_valid(value) && ::hglrt::fn_modified(value))
        {
            ::hglrt::schedule_Scheduler_datetime_str__bool(scheduler, ::hgl::core::m_::never(), ::std::nullopt, false);
            out_.set(::hgraph::Str{"asked"});
            return;
        }
    }

    static void stop([[maybe_unused]] ::hgraph::NodeView wake_node_)
    {
        ::hglrt::DroppedWakeLog::bind<state_type_, &state_type_::dropped_wakes_>(wake_node_).finish();
    }
};
}  // namespace n_::wake_in_the_past
}  // namespace hgl::demo::timing::m_

// The state of the node, as hgraph holds it (TIME-6).
template <>
struct hgraph::scalar_descriptor<::hgl::demo::timing::m_::n_::wake_in_the_past::impl_::state_type_>
{
    [[nodiscard]] static constexpr bool is_concrete() noexcept { return true; }
    [[nodiscard]] static const ValueTypeMetaData* value_meta()
    {
        return TypeRegistry::instance().register_scalar<::hgl::demo::timing::m_::n_::wake_in_the_past::impl_::state_type_>("hgl:demo.timing::wake_in_the_past.state");
    }
};

namespace hgl::demo::timing::m_
{
namespace w_
{
// wake_in_the_past(value: i64) -> str (node), module demo.timing, line 88
::hgraph::Port<::hgraph::TS<::hgraph::Str>> wake_in_the_past(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgraph::wire<::hgl::demo::timing::m_::n_::wake_in_the_past::impl_>(wiring_, value);
}
}  // namespace w_

namespace n_::wake_after
{
// wake_after(delay: duration) -> bool (node), module demo.timing, line 99
struct impl_
{
    static constexpr auto name = "demo.timing::wake_after";

    static void eval([[maybe_unused]] ::hgraph::In<"delay", ::hgraph::TS<::hgraph::TimeDelta>> delay, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Bool>> out_, [[maybe_unused]] ::hgraph::NodeScheduler capability_01_)
    {
        [[maybe_unused]] const ::hglrt::Scheduler scheduler{capability_01_, ::hglrt::Hook::evaluation};
        // TIME-9: what woke the node, read as it is entered.
        [[maybe_unused]] const bool timer_woke_ = scheduler.is_scheduled_now();
        [[maybe_unused]] const bool input_woke_ = ::hglrt::ticked(delay);
        // when, line 101 (TIME-3)
        if (::hglrt::fn_modified(delay))
        {
            ::hglrt::schedule_Scheduler_duration_str__bool(scheduler, ::hglrt::checked_value(delay), ::std::nullopt, false);
            out_.set(true);
            return;
        }
    }
};
}  // namespace n_::wake_after
namespace w_
{
// wake_after(delay: duration) -> bool (node), module demo.timing, line 99
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> wake_after(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::TimeDelta>> delay)
{
    return ::hgraph::wire<::hgl::demo::timing::m_::n_::wake_after::impl_>(wiring_, delay);
}
}  // namespace w_

namespace n_::alarm_or_input
{
// alarm_or_input(value: i64) -> str (node), module demo.timing, line 108
struct impl_
{
    static constexpr auto name = "demo.timing::alarm_or_input";

    static void start([[maybe_unused]] ::hgraph::NodeScheduler capability_01_)
    {
        [[maybe_unused]] const ::hglrt::Alarm alarm{capability_01_, ::hglrt::Hook::start};
        ::hglrt::schedule_Alarm_duration(alarm, ::hgraph::TimeDelta{2});
    }

    static void eval([[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>, ::hgraph::InputValidity::Unchecked> value, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Str>> out_, [[maybe_unused]] ::hgraph::NodeScheduler capability_01_)
    {
        [[maybe_unused]] const ::hglrt::Alarm alarm{capability_01_, ::hglrt::Hook::evaluation};
        // TIME-9: what woke the node, read as it is entered.
        [[maybe_unused]] const bool timer_woke_ = !(::hglrt::ticked(value));
        [[maybe_unused]] const bool input_woke_ = ::hglrt::ticked(value);
        // when, line 111 (TIME-3)
        if (timer_woke_)
        {
            out_.set(::hgraph::Str{"alarm"});
            return;
        }
        // when, line 112 (TIME-3)
        if (::hglrt::fn_valid(value) && ::hglrt::fn_modified(value))
        {
            out_.set(::hgraph::Str{"input"});
            return;
        }
    }
};
}  // namespace n_::alarm_or_input
namespace w_
{
// alarm_or_input(value: i64) -> str (node), module demo.timing, line 108
::hgraph::Port<::hgraph::TS<::hgraph::Str>> alarm_or_input(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgraph::wire<::hgl::demo::timing::m_::n_::alarm_or_input::impl_>(wiring_, value);
}
}  // namespace w_

namespace n_::alarmed
{
// alarmed() -> i64 (node), module demo.timing, line 117
struct impl_
{
    static constexpr auto name = "demo.timing::alarmed";

    // The node's `state` and `cache` (TIME-6).
    struct state_type_
    {
        ::hgraph::Int count{};
    };

    static void start([[maybe_unused]] ::hgraph::State<state_type_> state_, [[maybe_unused]] ::hgraph::NodeScheduler capability_01_)
    {
        state_.set(state_type_{});
        state_type_& kept_ = state_.modify();
        [[maybe_unused]] auto& count = kept_.count;
        [[maybe_unused]] const ::hglrt::Alarm alarm{capability_01_, ::hglrt::Hook::start};
        count = 0;
        ::hglrt::schedule_Alarm_duration(alarm, ::hgraph::TimeDelta{1});
    }

    static void eval([[maybe_unused]] ::hgraph::State<state_type_> state_, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_, [[maybe_unused]] ::hgraph::NodeScheduler capability_01_)
    {
        state_type_& kept_ = state_.modify();
        [[maybe_unused]] auto& count = kept_.count;
        [[maybe_unused]] const ::hglrt::Alarm alarm{capability_01_, ::hglrt::Hook::evaluation};
        // TIME-9: what woke the node, read as it is entered.
        [[maybe_unused]] const bool timer_woke_ = true;
        [[maybe_unused]] const bool input_woke_ = false;
        // when, line 121 (TIME-3)
        if (timer_woke_)
        {
            count = ::hglrt::add__i64_i64(count, 1);
            if (::hglrt::lt__T_T<::hgraph::Int>(count, 3))
            {
                ::hglrt::schedule_Alarm_duration(alarm, ::hgraph::TimeDelta{2});
            }
            out_.set(count);
            return;
        }
    }
};
}  // namespace n_::alarmed
}  // namespace hgl::demo::timing::m_

// The state of the node, as hgraph holds it (TIME-6).
template <>
struct hgraph::scalar_descriptor<::hgl::demo::timing::m_::n_::alarmed::impl_::state_type_>
{
    [[nodiscard]] static constexpr bool is_concrete() noexcept { return true; }
    [[nodiscard]] static const ValueTypeMetaData* value_meta()
    {
        return TypeRegistry::instance().register_scalar<::hgl::demo::timing::m_::n_::alarmed::impl_::state_type_>("hgl:demo.timing::alarmed.state");
    }
};

namespace hgl::demo::timing::m_
{
namespace w_
{
// alarmed() -> i64 (node), module demo.timing, line 117
::hgraph::Port<::hgraph::TS<::hgraph::Int>> alarmed(::hgraph::Wiring& wiring_)
{
    return ::hgraph::wire<::hgl::demo::timing::m_::n_::alarmed::impl_>(wiring_);
}
}  // namespace w_

namespace n_::alarm_asked_in_start
{
// alarm_asked_in_start(value: i64) -> str (node), module demo.timing, line 131
struct impl_
{
    static constexpr auto name = "demo.timing::alarm_asked_in_start";

    static void start([[maybe_unused]] ::hgraph::NodeScheduler capability_01_)
    {
        [[maybe_unused]] const ::hglrt::Alarm alarm{capability_01_, ::hglrt::Hook::start};
        ::hglrt::schedule_Alarm_duration(alarm, ::hgraph::TimeDelta{3});
    }

    static void eval([[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>, ::hgraph::InputValidity::Unchecked> value, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Str>> out_, [[maybe_unused]] ::hgraph::NodeScheduler capability_01_)
    {
        [[maybe_unused]] const ::hglrt::Alarm alarm{capability_01_, ::hglrt::Hook::evaluation};
        // TIME-9: what woke the node, read as it is entered.
        [[maybe_unused]] const bool timer_woke_ = !(::hglrt::ticked(value));
        [[maybe_unused]] const bool input_woke_ = ::hglrt::ticked(value);
        // when, line 134 (TIME-3)
        if (timer_woke_)
        {
            out_.set(::hgraph::Str{"woke"});
            return;
        }
        // when, line 135 (TIME-3)
        if (::hglrt::fn_valid(value) && ::hglrt::fn_modified(value))
        {
            out_.set(::hgraph::Str{"input"});
            return;
        }
    }
};
}  // namespace n_::alarm_asked_in_start
namespace w_
{
// alarm_asked_in_start(value: i64) -> str (node), module demo.timing, line 131
::hgraph::Port<::hgraph::TS<::hgraph::Str>> alarm_asked_in_start(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgraph::wire<::hgl::demo::timing::m_::n_::alarm_asked_in_start::impl_>(wiring_, value);
}
}  // namespace w_

namespace n_::alarm_asked_once
{
// alarm_asked_once(value: i64) -> str (node), module demo.timing, line 138
struct impl_
{
    static constexpr auto name = "demo.timing::alarm_asked_once";

    // The node's `state` and `cache` (TIME-6).
    struct state_type_
    {
        ::hgraph::Bool asked{};
    };

    static void start([[maybe_unused]] ::hgraph::State<state_type_> state_)
    {
        state_.set(state_type_{});
        state_type_& kept_ = state_.modify();
        [[maybe_unused]] auto& asked = kept_.asked;
        asked = false;
    }

    static void eval([[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>, ::hgraph::InputValidity::Unchecked> value, [[maybe_unused]] ::hgraph::State<state_type_> state_, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Str>> out_, [[maybe_unused]] ::hgraph::NodeScheduler capability_02_)
    {
        state_type_& kept_ = state_.modify();
        [[maybe_unused]] auto& asked = kept_.asked;
        [[maybe_unused]] const ::hglrt::Alarm alarm{capability_02_, ::hglrt::Hook::evaluation};
        // TIME-9: what woke the node, read as it is entered.
        [[maybe_unused]] const bool timer_woke_ = !(::hglrt::ticked(value));
        [[maybe_unused]] const bool input_woke_ = ::hglrt::ticked(value);
        // when, line 141 (TIME-3)
        if (timer_woke_)
        {
            out_.set(::hgraph::Str{"woke"});
            return;
        }
        // when, line 142 (TIME-3)
        if (::hglrt::fn_valid(value) && ::hglrt::fn_modified(value))
        {
            if ((!asked))
            {
                ::hglrt::schedule_Alarm_duration(alarm, ::hgraph::TimeDelta{3});
                asked = true;
            }
            out_.set(::hgraph::Str{"input"});
            return;
        }
    }
};
}  // namespace n_::alarm_asked_once
}  // namespace hgl::demo::timing::m_

// The state of the node, as hgraph holds it (TIME-6).
template <>
struct hgraph::scalar_descriptor<::hgl::demo::timing::m_::n_::alarm_asked_once::impl_::state_type_>
{
    [[nodiscard]] static constexpr bool is_concrete() noexcept { return true; }
    [[nodiscard]] static const ValueTypeMetaData* value_meta()
    {
        return TypeRegistry::instance().register_scalar<::hgl::demo::timing::m_::n_::alarm_asked_once::impl_::state_type_>("hgl:demo.timing::alarm_asked_once.state");
    }
};

namespace hgl::demo::timing::m_
{
namespace w_
{
// alarm_asked_once(value: i64) -> str (node), module demo.timing, line 138
::hgraph::Port<::hgraph::TS<::hgraph::Str>> alarm_asked_once(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgraph::wire<::hgl::demo::timing::m_::n_::alarm_asked_once::impl_>(wiring_, value);
}
}  // namespace w_

namespace n_::alarm_asked_on_each
{
// alarm_asked_on_each(value: i64) -> str (node), module demo.timing, line 151
struct impl_
{
    static constexpr auto name = "demo.timing::alarm_asked_on_each";

    static void eval([[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>, ::hgraph::InputValidity::Unchecked> value, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Str>> out_, [[maybe_unused]] ::hgraph::NodeScheduler capability_01_)
    {
        [[maybe_unused]] const ::hglrt::Alarm alarm{capability_01_, ::hglrt::Hook::evaluation};
        // TIME-9: what woke the node, read as it is entered.
        [[maybe_unused]] const bool timer_woke_ = !(::hglrt::ticked(value));
        [[maybe_unused]] const bool input_woke_ = ::hglrt::ticked(value);
        // when, line 153 (TIME-3)
        if (timer_woke_)
        {
            out_.set(::hgraph::Str{"woke"});
            return;
        }
        // when, line 154 (TIME-3)
        if (::hglrt::fn_valid(value) && ::hglrt::fn_modified(value))
        {
            ::hglrt::schedule_Alarm_duration(alarm, ::hgraph::TimeDelta{3});
            out_.set(::hgraph::Str{"input"});
            return;
        }
    }
};
}  // namespace n_::alarm_asked_on_each
namespace w_
{
// alarm_asked_on_each(value: i64) -> str (node), module demo.timing, line 151
::hgraph::Port<::hgraph::TS<::hgraph::Str>> alarm_asked_on_each(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgraph::wire<::hgl::demo::timing::m_::n_::alarm_asked_on_each::impl_>(wiring_, value);
}
}  // namespace w_

namespace n_::alarm_beside_a_passive_input
{
// alarm_beside_a_passive_input(value: i64) -> i64 (node), module demo.timing, line 161
struct impl_
{
    static constexpr auto name = "demo.timing::alarm_beside_a_passive_input";

    // The node's `state` and `cache` (TIME-6).
    struct state_type_
    {
        ::hgraph::Int woken{};
    };

    static void start([[maybe_unused]] ::hgraph::State<state_type_> state_, [[maybe_unused]] ::hgraph::NodeScheduler capability_02_)
    {
        state_.set(state_type_{});
        state_type_& kept_ = state_.modify();
        [[maybe_unused]] auto& woken = kept_.woken;
        [[maybe_unused]] const ::hglrt::Alarm alarm{capability_02_, ::hglrt::Hook::start};
        woken = 0;
        ::hglrt::schedule_Alarm_duration(alarm, ::hgraph::TimeDelta{3});
    }

    static void eval([[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>, ::hgraph::InputActivity::Passive, ::hgraph::InputValidity::Unchecked> value, [[maybe_unused]] ::hgraph::State<state_type_> state_, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_, [[maybe_unused]] ::hgraph::NodeScheduler capability_02_)
    {
        state_type_& kept_ = state_.modify();
        [[maybe_unused]] auto& woken = kept_.woken;
        [[maybe_unused]] const ::hglrt::Alarm alarm{capability_02_, ::hglrt::Hook::evaluation};
        // TIME-9: what woke the node, read as it is entered.
        [[maybe_unused]] const bool timer_woke_ = !(::hglrt::ticked(value));
        [[maybe_unused]] const bool input_woke_ = ::hglrt::ticked(value);
        // when, line 165 (TIME-3)
        if (timer_woke_)
        {
            woken = ::hglrt::add__i64_i64(woken, 1);
            out_.set(woken);
            return;
        }
    }
};
}  // namespace n_::alarm_beside_a_passive_input
}  // namespace hgl::demo::timing::m_

// The state of the node, as hgraph holds it (TIME-6).
template <>
struct hgraph::scalar_descriptor<::hgl::demo::timing::m_::n_::alarm_beside_a_passive_input::impl_::state_type_>
{
    [[nodiscard]] static constexpr bool is_concrete() noexcept { return true; }
    [[nodiscard]] static const ValueTypeMetaData* value_meta()
    {
        return TypeRegistry::instance().register_scalar<::hgl::demo::timing::m_::n_::alarm_beside_a_passive_input::impl_::state_type_>("hgl:demo.timing::alarm_beside_a_passive_input.state");
    }
};

namespace hgl::demo::timing::m_
{
namespace w_
{
// alarm_beside_a_passive_input(value: i64) -> i64 (node), module demo.timing, line 161
::hgraph::Port<::hgraph::TS<::hgraph::Int>> alarm_beside_a_passive_input(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgraph::wire<::hgl::demo::timing::m_::n_::alarm_beside_a_passive_input::impl_>(wiring_, value);
}
}  // namespace w_

namespace n_::time_before_each_delay
{
// time_before_each_delay() -> datetime (source), module demo.timing, line 172
struct impl_
{
    static constexpr auto name = "demo.timing::time_before_each_delay";
    // SOURCE-2: "The first evaluation begins the body".
    static constexpr bool schedule_on_start = true;
    using yielded_type_ = ::hglrt::Yielded<::hgraph::DateTime>;
    using body_type_ = ::hglrt::generator<yielded_type_>;

    // What the body reaches beside its configuration (SOURCE-1): its
    // `state` and `cache`, kept from one `yield` to the next, and each
    // capability it reads, lent for one resumption (TIME-8).
    struct slot_type_
    {
        ::hglrt::Lent<::hglrt::Clock> clock{};
    };

    // The body (SOURCE-1), given its configuration by value.
    static body_type_ body_([[maybe_unused]] slot_type_& slot_)
    {
        {
            co_yield yielded_type_{::hgraph::TimeDelta{2}, (slot_.clock.get()).evaluation_time()};
            co_yield yielded_type_{::hgraph::TimeDelta{3}, (slot_.clock.get()).evaluation_time()};
        }
    }

    // The node's state (TIME-6): the body, where it is, and the
    // timing of its yields.
    struct state_type_
    {
        ::hglrt::Source<::hgraph::DateTime> source_{};
        // What the body reaches, where it does not move.
        ::std::unique_ptr<slot_type_> slot_{};
        ::std::optional<body_type_> frame_{};
        ::std::optional<body_type_::iterator> at_{};
    };

    static void start(::hgraph::State<state_type_> state_)
    {
        state_.set(state_type_{});
        state_type_& kept_ = state_.modify();
        kept_.slot_ = ::std::make_unique<slot_type_>();
        kept_.frame_.emplace(body_(*kept_.slot_));
    }

    // SOURCE-3: what is due now is published, and the node asks for
    // a wake at the next target.
    static void eval(::hgraph::State<state_type_> state_, ::hgraph::DateTime now_, ::hgraph::NodeScheduler scheduler_, ::hgraph::Out<::hgraph::TS<::hgraph::DateTime>> out_, ::hgraph::EvaluationClockView capability_00_)
    {
        state_type_& kept_ = state_.modify();
        const ::hglrt::Lending<::hglrt::Clock> lent_00_{kept_.slot_->clock, ::hglrt::Clock{capability_00_}};
        ::hglrt::resume(
            kept_.source_, now_, scheduler_,
            [&kept_]() -> ::std::optional<yielded_type_> {
                if (!kept_.at_.has_value())
                {
                    kept_.at_.emplace(kept_.frame_->begin());
                }
                else
                {
                    ++*kept_.at_;
                }
                if (*kept_.at_ == ::std::default_sentinel)
                {
                    return ::std::nullopt;
                }
                return ::std::optional<yielded_type_>{**kept_.at_};
            },
            [&out_](::hgraph::DateTime value_) { out_.set(value_); });
    }

    // SOURCE-4: the body's frame, and the locals in it, end with the run.
    static void stop(::hgraph::State<state_type_> state_)
    {
        state_type_& kept_ = state_.modify();
        kept_.at_.reset();
        kept_.frame_.reset();
    }
};
}  // namespace n_::time_before_each_delay
}  // namespace hgl::demo::timing::m_

// The state of the node, as hgraph holds it (TIME-6).
template <>
struct hgraph::scalar_descriptor<::hgl::demo::timing::m_::n_::time_before_each_delay::impl_::state_type_>
{
    [[nodiscard]] static constexpr bool is_concrete() noexcept { return true; }
    [[nodiscard]] static const ValueTypeMetaData* value_meta()
    {
        return TypeRegistry::instance().register_scalar<::hgl::demo::timing::m_::n_::time_before_each_delay::impl_::state_type_>("hgl:demo.timing::time_before_each_delay.state");
    }
};

namespace hgl::demo::timing::m_
{
namespace w_
{
// time_before_each_delay() -> datetime (source), module demo.timing, line 172
::hgraph::Port<::hgraph::TS<::hgraph::DateTime>> time_before_each_delay(::hgraph::Wiring& wiring_)
{
    return ::hgraph::wire<::hgl::demo::timing::m_::n_::time_before_each_delay::impl_>(wiring_);
}
}  // namespace w_

namespace n_::absolute_yields
{
// absolute_yields(const first: datetime, const second: datetime) -> i64 (source), module demo.timing, line 181
struct impl_
{
    static constexpr auto name = "demo.timing::absolute_yields";
    // SOURCE-2: "The first evaluation begins the body".
    static constexpr bool schedule_on_start = true;
    using yielded_type_ = ::hglrt::Yielded<::hgraph::Int>;
    using body_type_ = ::hglrt::generator<yielded_type_>;

    // The body (SOURCE-1), given its configuration by value.
    static body_type_ body_([[maybe_unused]] ::hgraph::DateTime first, [[maybe_unused]] ::hgraph::DateTime second)
    {
        co_yield yielded_type_{first, 10};
        co_yield yielded_type_{second, 20};
    }

    // The node's state (TIME-6): the body, where it is, and the
    // timing of its yields.
    struct state_type_
    {
        ::hglrt::Source<::hgraph::Int> source_{};
        ::std::optional<body_type_> frame_{};
        ::std::optional<body_type_::iterator> at_{};
    };

    static void start(::hgraph::Scalar<"first", ::hgraph::DateTime> first, ::hgraph::Scalar<"second", ::hgraph::DateTime> second, ::hgraph::State<state_type_> state_)
    {
        state_.set(state_type_{});
        state_type_& kept_ = state_.modify();
        kept_.frame_.emplace(body_(first.value(), second.value()));
    }

    // SOURCE-3: what is due now is published, and the node asks for
    // a wake at the next target.
    static void eval([[maybe_unused]] ::hgraph::Scalar<"first", ::hgraph::DateTime> first, [[maybe_unused]] ::hgraph::Scalar<"second", ::hgraph::DateTime> second, ::hgraph::State<state_type_> state_, ::hgraph::DateTime now_, ::hgraph::NodeScheduler scheduler_, ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        state_type_& kept_ = state_.modify();
        ::hglrt::resume(
            kept_.source_, now_, scheduler_,
            [&kept_]() -> ::std::optional<yielded_type_> {
                if (!kept_.at_.has_value())
                {
                    kept_.at_.emplace(kept_.frame_->begin());
                }
                else
                {
                    ++*kept_.at_;
                }
                if (*kept_.at_ == ::std::default_sentinel)
                {
                    return ::std::nullopt;
                }
                return ::std::optional<yielded_type_>{**kept_.at_};
            },
            [&out_](::hgraph::Int value_) { out_.set(value_); });
    }

    // SOURCE-4: the body's frame, and the locals in it, end with the run.
    static void stop(::hgraph::State<state_type_> state_)
    {
        state_type_& kept_ = state_.modify();
        kept_.at_.reset();
        kept_.frame_.reset();
    }
};
}  // namespace n_::absolute_yields
}  // namespace hgl::demo::timing::m_

// The state of the node, as hgraph holds it (TIME-6).
template <>
struct hgraph::scalar_descriptor<::hgl::demo::timing::m_::n_::absolute_yields::impl_::state_type_>
{
    [[nodiscard]] static constexpr bool is_concrete() noexcept { return true; }
    [[nodiscard]] static const ValueTypeMetaData* value_meta()
    {
        return TypeRegistry::instance().register_scalar<::hgl::demo::timing::m_::n_::absolute_yields::impl_::state_type_>("hgl:demo.timing::absolute_yields.state");
    }
};

namespace hgl::demo::timing::m_
{
namespace w_
{
// absolute_yields(const first: datetime, const second: datetime) -> i64 (source), module demo.timing, line 181
::hgraph::Port<::hgraph::TS<::hgraph::Int>> absolute_yields(::hgraph::Wiring& wiring_, ::hgraph::DateTime first, ::hgraph::DateTime second)
{
    return ::hgraph::wire<::hgl::demo::timing::m_::n_::absolute_yields::impl_>(wiring_, first, second);
}
}  // namespace w_

// opening_at(at: datetime) -> datetime (`const fn`), module demo.timing, line 187
::hgraph::DateTime opening_at([[maybe_unused]] ::hgraph::DateTime at)
{
    return ::hglrt::add__datetime_duration(at, ::hgraph::TimeDelta{0});
}

// shortened(at: datetime) -> bool (`const fn`), module demo.timing, line 190
::hgraph::Bool shortened([[maybe_unused]] ::hgraph::DateTime at)
{
    return ::hglrt::ne__T_T<::hgraph::DateTime>(at, ::hgl::core::m_::never());
}

namespace n_::finish_or_resume
{
// finish_or_resume(const finish_early: bool) -> i64 (source), module demo.timing, line 193
struct impl_
{
    static constexpr auto name = "demo.timing::finish_or_resume";
    // SOURCE-2: "The first evaluation begins the body".
    static constexpr bool schedule_on_start = true;
    using yielded_type_ = ::hglrt::Yielded<::hgraph::Int>;
    using body_type_ = ::hglrt::generator<yielded_type_>;

    // What the body reaches beside its configuration (SOURCE-1): its
    // `state` and `cache`, kept from one `yield` to the next, and each
    // capability it reads, lent for one resumption (TIME-8).
    struct slot_type_
    {
        ::hglrt::Lent<::hglrt::Clock> clock{};
    };

    // The body (SOURCE-1), given its configuration by value.
    static body_type_ body_([[maybe_unused]] slot_type_& slot_, [[maybe_unused]] ::hgraph::Bool finish_early)
    {
        {
            [[maybe_unused]] const ::hgraph::DateTime opening = (slot_.clock.get()).evaluation_time();
            co_yield yielded_type_{::hgraph::TimeDelta{2}, 10};
            if (finish_early)
            {
                co_return;
            }
            co_yield yielded_type_{::hglrt::add__datetime_duration(opening, ::hgraph::TimeDelta{4}), 20};
        }
    }

    // The node's state (TIME-6): the body, where it is, and the
    // timing of its yields.
    struct state_type_
    {
        ::hglrt::Source<::hgraph::Int> source_{};
        // What the body reaches, where it does not move.
        ::std::unique_ptr<slot_type_> slot_{};
        ::std::optional<body_type_> frame_{};
        ::std::optional<body_type_::iterator> at_{};
    };

    static void start(::hgraph::Scalar<"finish_early", ::hgraph::Bool> finish_early, ::hgraph::State<state_type_> state_)
    {
        state_.set(state_type_{});
        state_type_& kept_ = state_.modify();
        kept_.slot_ = ::std::make_unique<slot_type_>();
        kept_.frame_.emplace(body_(*kept_.slot_, finish_early.value()));
    }

    // SOURCE-3: what is due now is published, and the node asks for
    // a wake at the next target.
    static void eval([[maybe_unused]] ::hgraph::Scalar<"finish_early", ::hgraph::Bool> finish_early, ::hgraph::State<state_type_> state_, ::hgraph::DateTime now_, ::hgraph::NodeScheduler scheduler_, ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_, ::hgraph::EvaluationClockView capability_01_)
    {
        state_type_& kept_ = state_.modify();
        const ::hglrt::Lending<::hglrt::Clock> lent_01_{kept_.slot_->clock, ::hglrt::Clock{capability_01_}};
        ::hglrt::resume(
            kept_.source_, now_, scheduler_,
            [&kept_]() -> ::std::optional<yielded_type_> {
                if (!kept_.at_.has_value())
                {
                    kept_.at_.emplace(kept_.frame_->begin());
                }
                else
                {
                    ++*kept_.at_;
                }
                if (*kept_.at_ == ::std::default_sentinel)
                {
                    return ::std::nullopt;
                }
                return ::std::optional<yielded_type_>{**kept_.at_};
            },
            [&out_](::hgraph::Int value_) { out_.set(value_); });
    }

    // SOURCE-4: the body's frame, and the locals in it, end with the run.
    static void stop(::hgraph::State<state_type_> state_)
    {
        state_type_& kept_ = state_.modify();
        kept_.at_.reset();
        kept_.frame_.reset();
    }
};
}  // namespace n_::finish_or_resume
}  // namespace hgl::demo::timing::m_

// The state of the node, as hgraph holds it (TIME-6).
template <>
struct hgraph::scalar_descriptor<::hgl::demo::timing::m_::n_::finish_or_resume::impl_::state_type_>
{
    [[nodiscard]] static constexpr bool is_concrete() noexcept { return true; }
    [[nodiscard]] static const ValueTypeMetaData* value_meta()
    {
        return TypeRegistry::instance().register_scalar<::hgl::demo::timing::m_::n_::finish_or_resume::impl_::state_type_>("hgl:demo.timing::finish_or_resume.state");
    }
};

namespace hgl::demo::timing::m_
{
namespace w_
{
// finish_or_resume(const finish_early: bool) -> i64 (source), module demo.timing, line 193
::hgraph::Port<::hgraph::TS<::hgraph::Int>> finish_or_resume(::hgraph::Wiring& wiring_, ::hgraph::Bool finish_early)
{
    return ::hgraph::wire<::hgl::demo::timing::m_::n_::finish_or_resume::impl_>(wiring_, finish_early);
}
}  // namespace w_

namespace n_::counter
{
// counter(const limit: i64) -> i64 (source), module demo.timing, line 204
struct impl_
{
    static constexpr auto name = "demo.timing::counter";
    // SOURCE-2: "The first evaluation begins the body".
    static constexpr bool schedule_on_start = true;
    using yielded_type_ = ::hglrt::Yielded<::hgraph::Int>;
    using body_type_ = ::hglrt::generator<yielded_type_>;

    // What the body reaches beside its configuration (SOURCE-1): its
    // `state` and `cache`, kept from one `yield` to the next, and each
    // capability it reads, lent for one resumption (TIME-8).
    struct slot_type_
    {
        ::hgraph::Int count{};
    };

    // The body (SOURCE-1), given its configuration by value.
    static body_type_ body_([[maybe_unused]] slot_type_& slot_, [[maybe_unused]] ::hgraph::Int limit)
    {
        [[maybe_unused]] auto& count = slot_.count;
        while (::hglrt::lt__T_T<::hgraph::Int>(count, limit))
        {
            count = ::hglrt::add__i64_i64(count, 1);
            co_yield yielded_type_{::hgraph::TimeDelta{1}, count};
        }
    }

    // The node's state (TIME-6): the body, where it is, and the
    // timing of its yields.
    struct state_type_
    {
        ::hglrt::Source<::hgraph::Int> source_{};
        // What the body reaches, where it does not move.
        ::std::unique_ptr<slot_type_> slot_{};
        ::std::optional<body_type_> frame_{};
        ::std::optional<body_type_::iterator> at_{};
    };

    static void start(::hgraph::Scalar<"limit", ::hgraph::Int> limit, ::hgraph::State<state_type_> state_)
    {
        state_.set(state_type_{});
        state_type_& kept_ = state_.modify();
        kept_.slot_ = ::std::make_unique<slot_type_>();
        [[maybe_unused]] auto& count = kept_.slot_->count;
        count = 0;
        kept_.frame_.emplace(body_(*kept_.slot_, limit.value()));
    }

    // SOURCE-3: what is due now is published, and the node asks for
    // a wake at the next target.
    static void eval([[maybe_unused]] ::hgraph::Scalar<"limit", ::hgraph::Int> limit, ::hgraph::State<state_type_> state_, ::hgraph::DateTime now_, ::hgraph::NodeScheduler scheduler_, ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        state_type_& kept_ = state_.modify();
        ::hglrt::resume(
            kept_.source_, now_, scheduler_,
            [&kept_]() -> ::std::optional<yielded_type_> {
                if (!kept_.at_.has_value())
                {
                    kept_.at_.emplace(kept_.frame_->begin());
                }
                else
                {
                    ++*kept_.at_;
                }
                if (*kept_.at_ == ::std::default_sentinel)
                {
                    return ::std::nullopt;
                }
                return ::std::optional<yielded_type_>{**kept_.at_};
            },
            [&out_](::hgraph::Int value_) { out_.set(value_); });
    }

    // SOURCE-4: the body's frame, and the locals in it, end with the run.
    static void stop(::hgraph::State<state_type_> state_, [[maybe_unused]] ::hgraph::Scalar<"limit", ::hgraph::Int> limit, ::hgraph::LoggerView capability_01_)
    {
        state_type_& kept_ = state_.modify();
        kept_.at_.reset();
        kept_.frame_.reset();
        [[maybe_unused]] auto& count = kept_.slot_->count;
        [[maybe_unused]] const ::hglrt::Logger logger{capability_01_};
        ::hgl::core::m_::log_debug(logger, ::hgraph::Str{"counted"});
    }
};
}  // namespace n_::counter
}  // namespace hgl::demo::timing::m_

// The state of the node, as hgraph holds it (TIME-6).
template <>
struct hgraph::scalar_descriptor<::hgl::demo::timing::m_::n_::counter::impl_::state_type_>
{
    [[nodiscard]] static constexpr bool is_concrete() noexcept { return true; }
    [[nodiscard]] static const ValueTypeMetaData* value_meta()
    {
        return TypeRegistry::instance().register_scalar<::hgl::demo::timing::m_::n_::counter::impl_::state_type_>("hgl:demo.timing::counter.state");
    }
};

namespace hgl::demo::timing::m_
{
namespace w_
{
// counter(const limit: i64) -> i64 (source), module demo.timing, line 204
::hgraph::Port<::hgraph::TS<::hgraph::Int>> counter(::hgraph::Wiring& wiring_, ::hgraph::Int limit)
{
    return ::hgraph::wire<::hgl::demo::timing::m_::n_::counter::impl_>(wiring_, limit);
}
}  // namespace w_

namespace n_::counting_from
{
// counting_from(const first: i64) -> i64 (source), module demo.timing, line 215
struct impl_
{
    static constexpr auto name = "demo.timing::counting_from";
    // SOURCE-2: "The first evaluation begins the body".
    static constexpr bool schedule_on_start = true;
    using yielded_type_ = ::hglrt::Yielded<::hgraph::Int>;
    using body_type_ = ::hglrt::generator<yielded_type_>;

    // What the body reaches beside its configuration (SOURCE-1): its
    // `state` and `cache`, kept from one `yield` to the next, and each
    // capability it reads, lent for one resumption (TIME-8).
    struct slot_type_
    {
        ::hgraph::Int next{};
    };

    // The body (SOURCE-1), given its configuration by value.
    static body_type_ body_([[maybe_unused]] slot_type_& slot_, [[maybe_unused]] ::hgraph::Int first)
    {
        [[maybe_unused]] auto& next = slot_.next;
        while (::hglrt::lt__T_T<::hgraph::Int>(next, ::hglrt::add__i64_i64(first, 3)))
        {
            co_yield yielded_type_{::hgraph::TimeDelta{1}, next};
            next = ::hglrt::add__i64_i64(next, 1);
        }
    }

    // The node's state (TIME-6): the body, where it is, and the
    // timing of its yields.
    struct state_type_
    {
        ::hglrt::Source<::hgraph::Int> source_{};
        // What the body reaches, where it does not move.
        ::std::unique_ptr<slot_type_> slot_{};
        ::std::optional<body_type_> frame_{};
        ::std::optional<body_type_::iterator> at_{};
    };

    static void start(::hgraph::Scalar<"first", ::hgraph::Int> first, ::hgraph::State<state_type_> state_)
    {
        state_.set(state_type_{});
        state_type_& kept_ = state_.modify();
        kept_.slot_ = ::std::make_unique<slot_type_>();
        [[maybe_unused]] auto& next = kept_.slot_->next;
        next = 0;
        kept_.frame_.emplace(body_(*kept_.slot_, first.value()));
        next = first.value();
    }

    // SOURCE-3: what is due now is published, and the node asks for
    // a wake at the next target.
    static void eval([[maybe_unused]] ::hgraph::Scalar<"first", ::hgraph::Int> first, ::hgraph::State<state_type_> state_, ::hgraph::DateTime now_, ::hgraph::NodeScheduler scheduler_, ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        state_type_& kept_ = state_.modify();
        ::hglrt::resume(
            kept_.source_, now_, scheduler_,
            [&kept_]() -> ::std::optional<yielded_type_> {
                if (!kept_.at_.has_value())
                {
                    kept_.at_.emplace(kept_.frame_->begin());
                }
                else
                {
                    ++*kept_.at_;
                }
                if (*kept_.at_ == ::std::default_sentinel)
                {
                    return ::std::nullopt;
                }
                return ::std::optional<yielded_type_>{**kept_.at_};
            },
            [&out_](::hgraph::Int value_) { out_.set(value_); });
    }

    // SOURCE-4: the body's frame, and the locals in it, end with the run.
    static void stop(::hgraph::State<state_type_> state_)
    {
        state_type_& kept_ = state_.modify();
        kept_.at_.reset();
        kept_.frame_.reset();
    }
};
}  // namespace n_::counting_from
}  // namespace hgl::demo::timing::m_

// The state of the node, as hgraph holds it (TIME-6).
template <>
struct hgraph::scalar_descriptor<::hgl::demo::timing::m_::n_::counting_from::impl_::state_type_>
{
    [[nodiscard]] static constexpr bool is_concrete() noexcept { return true; }
    [[nodiscard]] static const ValueTypeMetaData* value_meta()
    {
        return TypeRegistry::instance().register_scalar<::hgl::demo::timing::m_::n_::counting_from::impl_::state_type_>("hgl:demo.timing::counting_from.state");
    }
};

namespace hgl::demo::timing::m_
{
namespace w_
{
// counting_from(const first: i64) -> i64 (source), module demo.timing, line 215
::hgraph::Port<::hgraph::TS<::hgraph::Int>> counting_from(::hgraph::Wiring& wiring_, ::hgraph::Int first)
{
    return ::hgraph::wire<::hgl::demo::timing::m_::n_::counting_from::impl_>(wiring_, first);
}
}  // namespace w_

namespace n_::i_::repeated::of_i64
{
// repeated(const value: i64, const times: i64) -> i64 (source), the instance `i64`, module demo.timing, line 225
struct impl_
{
    static constexpr auto name = "demo.timing::repeated[i64]";
    // SOURCE-2: "The first evaluation begins the body".
    static constexpr bool schedule_on_start = true;
    using yielded_type_ = ::hglrt::Yielded<::hgraph::Int>;
    using body_type_ = ::hglrt::generator<yielded_type_>;

    // What the body reaches beside its configuration (SOURCE-1): its
    // `state` and `cache`, kept from one `yield` to the next, and each
    // capability it reads, lent for one resumption (TIME-8).
    struct slot_type_
    {
        ::hgraph::Int done{};
    };

    // The body (SOURCE-1), given its configuration by value.
    static body_type_ body_([[maybe_unused]] slot_type_& slot_, [[maybe_unused]] ::hgraph::Int value, [[maybe_unused]] ::hgraph::Int times)
    {
        [[maybe_unused]] auto& done = slot_.done;
        while (::hglrt::lt__T_T<::hgraph::Int>(done, times))
        {
            done = ::hglrt::add__i64_i64(done, 1);
            co_yield yielded_type_{::hgraph::TimeDelta{1}, value};
        }
    }

    // The node's state (TIME-6): the body, where it is, and the
    // timing of its yields.
    struct state_type_
    {
        ::hglrt::Source<::hgraph::Int> source_{};
        // What the body reaches, where it does not move.
        ::std::unique_ptr<slot_type_> slot_{};
        ::std::optional<body_type_> frame_{};
        ::std::optional<body_type_::iterator> at_{};
    };

    static void start(::hgraph::Scalar<"value", ::hgraph::Int> value, ::hgraph::Scalar<"times", ::hgraph::Int> times, ::hgraph::State<state_type_> state_)
    {
        state_.set(state_type_{});
        state_type_& kept_ = state_.modify();
        kept_.slot_ = ::std::make_unique<slot_type_>();
        [[maybe_unused]] auto& done = kept_.slot_->done;
        done = 0;
        kept_.frame_.emplace(body_(*kept_.slot_, value.value(), times.value()));
    }

    // SOURCE-3: what is due now is published, and the node asks for
    // a wake at the next target.
    static void eval([[maybe_unused]] ::hgraph::Scalar<"value", ::hgraph::Int> value, [[maybe_unused]] ::hgraph::Scalar<"times", ::hgraph::Int> times, ::hgraph::State<state_type_> state_, ::hgraph::DateTime now_, ::hgraph::NodeScheduler scheduler_, ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        state_type_& kept_ = state_.modify();
        ::hglrt::resume(
            kept_.source_, now_, scheduler_,
            [&kept_]() -> ::std::optional<yielded_type_> {
                if (!kept_.at_.has_value())
                {
                    kept_.at_.emplace(kept_.frame_->begin());
                }
                else
                {
                    ++*kept_.at_;
                }
                if (*kept_.at_ == ::std::default_sentinel)
                {
                    return ::std::nullopt;
                }
                return ::std::optional<yielded_type_>{**kept_.at_};
            },
            [&out_](::hgraph::Int value_) { out_.set(value_); });
    }

    // SOURCE-4: the body's frame, and the locals in it, end with the run.
    static void stop(::hgraph::State<state_type_> state_)
    {
        state_type_& kept_ = state_.modify();
        kept_.at_.reset();
        kept_.frame_.reset();
    }
};
}  // namespace n_::i_::repeated::of_i64
}  // namespace hgl::demo::timing::m_

// The state of the node, as hgraph holds it (TIME-6).
template <>
struct hgraph::scalar_descriptor<::hgl::demo::timing::m_::n_::i_::repeated::of_i64::impl_::state_type_>
{
    [[nodiscard]] static constexpr bool is_concrete() noexcept { return true; }
    [[nodiscard]] static const ValueTypeMetaData* value_meta()
    {
        return TypeRegistry::instance().register_scalar<::hgl::demo::timing::m_::n_::i_::repeated::of_i64::impl_::state_type_>("hgl:demo.timing::repeated[i64].state");
    }
};

namespace hgl::demo::timing::m_
{
namespace w_::i_::repeated
{
// repeated(const value: i64, const times: i64) -> i64 (source), the instance `i64`, module demo.timing, line 225
::hgraph::Port<::hgraph::TS<::hgraph::Int>> of_i64(::hgraph::Wiring& wiring_, ::hgraph::Int value, ::hgraph::Int times)
{
    return ::hgraph::wire<::hgl::demo::timing::m_::n_::i_::repeated::of_i64::impl_>(wiring_, value, times);
}
}  // namespace w_::i_::repeated

}  // namespace hgl::demo::timing::m_

namespace hgl::demo::timing::m_::t_
{

void a_scheduler_wakes_a_node_with_no_input()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_00_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::timing::m_::w_::ticker(wiring_, ::hgraph::TimeDelta{2}, 3), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_01_ = ::hglrt::testing::Dense<::hgraph::Int>{::std::nullopt, ::std::nullopt, 1, ::std::nullopt, 2, ::std::nullopt, 3};
        ::hglrt::testing::expect_same(left_00_, right_01_);
    }();
}

void a_wake_can_coincide_with_an_input()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgraph::Str>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{7, ::std::nullopt, 9}, start_));
        ::hgl::core::m_::w_::i_::record::of_str(wiring_, ::hgl::demo::timing::m_::w_::cause_of_wake(wiring_, input_00_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgraph::Str>{::hgraph::Str{"input"}, ::std::nullopt, ::hgraph::Str{"input and scheduler"}, ::std::nullopt, ::hgraph::Str{"scheduler"}};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
}

void an_empty_selection_is_true_where_it_is_selected()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgraph::Bool>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{7, ::std::nullopt, 9}, start_));
        ::hgl::core::m_::w_::i_::record::of_bool(wiring_, ::hgl::demo::timing::m_::w_::empty_selected(wiring_, input_00_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgraph::Bool>{::std::nullopt, ::std::nullopt, true};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_04_ = ::hglrt::testing::evaluate<::hgraph::Bool>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{7, ::std::nullopt, 9}, start_));
        ::hgl::core::m_::w_::i_::record::of_bool(wiring_, ::hgl::demo::timing::m_::w_::empty_elsewhere(wiring_, input_03_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_05_ = ::hglrt::testing::Dense<::hgraph::Bool>{::std::nullopt, ::std::nullopt, false, ::std::nullopt, true};
        ::hglrt::testing::expect_same(left_04_, right_05_);
    }();
}

void scheduled_is_read_as_a_value()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgraph::Bool>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{7, ::std::nullopt, 9}, start_));
        ::hgl::core::m_::w_::i_::record::of_bool(wiring_, ::hgl::demo::timing::m_::w_::scheduled_on_input(wiring_, input_00_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgraph::Bool>{false, ::std::nullopt, true};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
}

void the_wake_delivered_is_pending_no_longer()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_00_ = ::hglrt::testing::evaluate<::hgraph::Bool>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        ::hgl::core::m_::w_::i_::record::of_bool(wiring_, ::hgl::demo::timing::m_::w_::pending(wiring_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_01_ = ::hglrt::testing::Dense<::hgraph::Bool>{::std::nullopt, true, ::std::nullopt, false};
        ::hglrt::testing::expect_same(left_00_, right_01_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_02_ = ::hglrt::testing::evaluate<::hgraph::Bool>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        ::hgl::core::m_::w_::i_::record::of_bool(wiring_, ::hgl::demo::timing::m_::w_::cancel_next(wiring_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_03_ = ::hglrt::testing::Dense<::hgraph::Bool>{::std::nullopt, true, ::std::nullopt, ::std::nullopt, ::std::nullopt, true};
        ::hglrt::testing::expect_same(left_02_, right_03_);
    }();
}

void a_wake_before_now_is_dropped()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgraph::Str>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{7, ::std::nullopt, 9}, start_));
        ::hgl::core::m_::w_::i_::record::of_str(wiring_, ::hgl::demo::timing::m_::w_::wake_in_the_past(wiring_, input_00_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgraph::Str>{::hgraph::Str{"asked"}, ::std::nullopt, ::hgraph::Str{"asked"}};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
}

void a_zero_duration_outside_start_fails()
{
    ::hglrt::testing::statement();
    ::hglrt::testing::raises("schedule.non_positive_duration", [&] {
        static_cast<void>(::hglrt::testing::evaluate<::hgraph::Bool>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::TimeDelta>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_duration(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::TimeDelta>{::hgraph::TimeDelta{1}, ::hgraph::TimeDelta{0}}, start_));
        ::hgl::core::m_::w_::i_::record::of_bool(wiring_, ::hgl::demo::timing::m_::w_::wake_after(wiring_, input_00_), ::hgraph::Str{::hglrt::testing::run_output});
    }));
    });
}

void an_alarm_wakes_a_node()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgraph::Str>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 2, 3, 4}, start_));
        ::hgl::core::m_::w_::i_::record::of_str(wiring_, ::hgl::demo::timing::m_::w_::alarm_or_input(wiring_, input_00_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgraph::Str>{::hgraph::Str{"input"}, ::hgraph::Str{"input"}, ::hgraph::Str{"input"}, ::hgraph::Str{"input"}};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_03_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::timing::m_::w_::alarmed(wiring_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_04_ = ::hglrt::testing::Dense<::hgraph::Int>{::std::nullopt, 1, ::std::nullopt, 2, ::std::nullopt, 3};
        ::hglrt::testing::expect_same(left_03_, right_04_);
    }();
}

void an_alarm_keeps_its_wake_when_an_input_ticks_first()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgraph::Str>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1}, start_));
        ::hgl::core::m_::w_::i_::record::of_str(wiring_, ::hgl::demo::timing::m_::w_::alarm_asked_in_start(wiring_, input_00_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgraph::Str>{::hgraph::Str{"input"}, ::std::nullopt, ::std::nullopt, ::hgraph::Str{"woke"}};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_04_ = ::hglrt::testing::evaluate<::hgraph::Str>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{::std::nullopt, 1}, start_));
        ::hgl::core::m_::w_::i_::record::of_str(wiring_, ::hgl::demo::timing::m_::w_::alarm_asked_in_start(wiring_, input_03_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_05_ = ::hglrt::testing::Dense<::hgraph::Str>{::std::nullopt, ::hgraph::Str{"input"}, ::std::nullopt, ::hgraph::Str{"woke"}};
        ::hglrt::testing::expect_same(left_04_, right_05_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_07_ = ::hglrt::testing::evaluate<::hgraph::Str>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_06_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{::std::nullopt, ::std::nullopt, ::std::nullopt, ::std::nullopt, 1}, start_));
        ::hgl::core::m_::w_::i_::record::of_str(wiring_, ::hgl::demo::timing::m_::w_::alarm_asked_in_start(wiring_, input_06_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_08_ = ::hglrt::testing::Dense<::hgraph::Str>{::std::nullopt, ::std::nullopt, ::std::nullopt, ::hgraph::Str{"woke"}, ::hgraph::Str{"input"}};
        ::hglrt::testing::expect_same(left_07_, right_08_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_010_ = ::hglrt::testing::evaluate<::hgraph::Str>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_09_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 2}, start_));
        ::hgl::core::m_::w_::i_::record::of_str(wiring_, ::hgl::demo::timing::m_::w_::alarm_asked_once(wiring_, input_09_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_011_ = ::hglrt::testing::Dense<::hgraph::Str>{::hgraph::Str{"input"}, ::hgraph::Str{"input"}, ::std::nullopt, ::hgraph::Str{"woke"}};
        ::hglrt::testing::expect_same(left_010_, right_011_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_013_ = ::hglrt::testing::evaluate<::hgraph::Str>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_012_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1}, start_));
        ::hgl::core::m_::w_::i_::record::of_str(wiring_, ::hgl::demo::timing::m_::w_::alarm_asked_on_each(wiring_, input_012_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_014_ = ::hglrt::testing::Dense<::hgraph::Str>{::hgraph::Str{"input"}, ::std::nullopt, ::std::nullopt, ::hgraph::Str{"woke"}};
        ::hglrt::testing::expect_same(left_013_, right_014_);
    }();
}

void asking_an_alarm_again_keeps_the_earliest()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgraph::Str>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 2}, start_));
        ::hgl::core::m_::w_::i_::record::of_str(wiring_, ::hgl::demo::timing::m_::w_::alarm_asked_on_each(wiring_, input_00_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgraph::Str>{::hgraph::Str{"input"}, ::hgraph::Str{"input"}, ::std::nullopt, ::hgraph::Str{"woke"}};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_04_ = ::hglrt::testing::evaluate<::hgraph::Str>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, ::std::nullopt, ::std::nullopt, ::std::nullopt, 5}, start_));
        ::hgl::core::m_::w_::i_::record::of_str(wiring_, ::hgl::demo::timing::m_::w_::alarm_asked_on_each(wiring_, input_03_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_05_ = ::hglrt::testing::Dense<::hgraph::Str>{::hgraph::Str{"input"}, ::std::nullopt, ::std::nullopt, ::hgraph::Str{"woke"}, ::hgraph::Str{"input"}, ::std::nullopt, ::std::nullopt, ::hgraph::Str{"woke"}};
        ::hglrt::testing::expect_same(left_04_, right_05_);
    }();
}

void an_alarm_beside_a_passive_input()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{::std::nullopt, 7}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::timing::m_::w_::alarm_beside_a_passive_input(wiring_, input_00_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgraph::Int>{::std::nullopt, ::std::nullopt, ::std::nullopt, 1};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
}

void an_alarm_wake_that_lands_with_an_input_is_the_inputs()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgraph::Str>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, ::std::nullopt, ::std::nullopt, 4}, start_));
        ::hgl::core::m_::w_::i_::record::of_str(wiring_, ::hgl::demo::timing::m_::w_::alarm_asked_in_start(wiring_, input_00_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgraph::Str>{::hgraph::Str{"input"}, ::std::nullopt, ::std::nullopt, ::hgraph::Str{"input"}};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
}

void a_source_reads_its_clock_when_it_runs()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_00_ = ::hglrt::testing::evaluate<::hgraph::DateTime>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        ::hgl::core::m_::w_::i_::record::of_datetime(wiring_, ::hgl::demo::timing::m_::w_::time_before_each_delay(wiring_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_01_ = ::hglrt::testing::Dense<::hgraph::DateTime>{::std::nullopt, ::std::nullopt, ::hgl::core::m_::earliest_start(), ::std::nullopt, ::std::nullopt, ::hglrt::add__datetime_duration(::hgl::core::m_::earliest_start(), ::hgraph::TimeDelta{2})};
        ::hglrt::testing::expect_same(left_00_, right_01_);
    }();
}

void absolute_targets_before_at_and_after_the_start()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_04_ = [&]() {
        const ::hgraph::DateTime argument_00_ = ::hgl::core::m_::never();
        const ::hgraph::DateTime argument_01_ = ::hgl::core::m_::earliest_start();
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const ::hgraph::DateTime argument_02_ = argument_00_; const ::hgraph::DateTime argument_03_ = argument_01_; return ::hgl::demo::timing::m_::w_::absolute_yields(wiring_, argument_02_, argument_03_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_05_ = ::hglrt::testing::Dense<::hgraph::Int>{20};
        ::hglrt::testing::expect_same(left_04_, right_05_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_010_ = [&]() {
        const ::hgraph::DateTime argument_06_ = ::hgl::core::m_::earliest_start();
        const ::hgraph::DateTime argument_07_ = ::hglrt::add__datetime_duration(::hgl::core::m_::earliest_start(), ::hgraph::TimeDelta{2});
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const ::hgraph::DateTime argument_08_ = argument_06_; const ::hgraph::DateTime argument_09_ = argument_07_; return ::hgl::demo::timing::m_::w_::absolute_yields(wiring_, argument_08_, argument_09_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_011_ = ::hglrt::testing::Dense<::hgraph::Int>{10, ::std::nullopt, 20};
        ::hglrt::testing::expect_same(left_010_, right_011_);
    }();
    ::hglrt::testing::statement();
    ::hglrt::testing::raises("yield.non_increasing_time", [&] {
        static_cast<void>([&]() {
        const ::hgraph::DateTime argument_012_ = ::hgl::core::m_::never();
        const ::hgraph::DateTime argument_013_ = ::hgl::core::m_::never();
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const ::hgraph::DateTime argument_014_ = argument_012_; const ::hgraph::DateTime argument_015_ = argument_013_; return ::hgl::demo::timing::m_::w_::absolute_yields(wiring_, argument_014_, argument_015_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }());
    });
}

void a_resumed_source_keeps_its_locals_until_it_returns()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_00_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::timing::m_::w_::finish_or_resume(wiring_, false), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_01_ = ::hglrt::testing::Dense<::hgraph::Int>{::std::nullopt, ::std::nullopt, 10, ::std::nullopt, 20};
        ::hglrt::testing::expect_same(left_00_, right_01_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_02_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::timing::m_::w_::finish_or_resume(wiring_, true), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_03_ = ::hglrt::testing::Dense<::hgraph::Int>{::std::nullopt, ::std::nullopt, 10};
        ::hglrt::testing::expect_same(left_02_, right_03_);
    }();
}

void a_source_keeps_its_cache_and_state()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_00_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::timing::m_::w_::counter(wiring_, 3), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_01_ = ::hglrt::testing::Dense<::hgraph::Int>{::std::nullopt, 1, 2, 3};
        ::hglrt::testing::expect_same(left_00_, right_01_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_02_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::timing::m_::w_::counting_from(wiring_, 10), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_03_ = ::hglrt::testing::Dense<::hgraph::Int>{::std::nullopt, 10, 11, 12};
        ::hglrt::testing::expect_same(left_02_, right_03_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_04_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::timing::m_::w_::i_::repeated::of_i64(wiring_, 5, 2), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_05_ = ::hglrt::testing::Dense<::hgraph::Int>{::std::nullopt, 5, 5};
        ::hglrt::testing::expect_same(left_04_, right_05_);
    }();
}

void a_run_configuration_is_given_in_braces()
{
    ::hglrt::testing::statement();
    [[maybe_unused]] const ::hgraph::DateTime opening = ::hgraph::DateTime{::std::chrono::microseconds{1767605400000000}};
    ::hglrt::testing::statement();
    [&] {
        const auto left_00_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::timing::m_::w_::absolute_yields(wiring_, ::hglrt::sub__datetime_duration(opening, ::hgraph::TimeDelta{2}), opening), ::hgraph::Str{::hglrt::testing::run_output});
    }, ::hglrt::testing::Bounds{.start = opening});
        const auto right_01_ = ::hglrt::testing::Dense<::hgraph::Int>{20};
        ::hglrt::testing::expect_same(left_00_, right_01_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_05_ = [&]() {
        const auto bounds_02_ = ::hglrt::testing::Bounds{.start = ::hgl::demo::timing::m_::opening_at(opening)};
        const ::hgraph::DateTime argument_03_ = opening;
        const ::hgraph::DateTime argument_04_ = ::hglrt::add__datetime_duration(opening, ::hgraph::TimeDelta{2});
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::timing::m_::w_::absolute_yields(wiring_, argument_03_, argument_04_), ::hgraph::Str{::hglrt::testing::run_output});
    }, bounds_02_);
    }();
        const auto right_06_ = ::hglrt::testing::Dense<::hgraph::Int>{10, ::std::nullopt, 20};
        ::hglrt::testing::expect_same(left_05_, right_06_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_07_ = ::hglrt::testing::evaluate<::hgraph::DateTime>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        ::hgl::core::m_::w_::i_::record::of_datetime(wiring_, ::hgl::demo::timing::m_::w_::time_before_each_delay(wiring_), ::hgraph::Str{::hglrt::testing::run_output});
    }, ::hglrt::testing::Bounds{.start = opening, .length = (::hgl::demo::timing::m_::shortened(opening) ? ::std::optional<::hgraph::TimeDelta>(::hgraph::TimeDelta{3}) : ::std::optional<::hgraph::TimeDelta>(::hgraph::TimeDelta{1000000}))});
        const auto right_08_ = ::hglrt::testing::Dense<::hgraph::DateTime>{::std::nullopt, ::std::nullopt, opening};
        ::hglrt::testing::expect_same(left_07_, right_08_);
    }();
}

}  // namespace hgl::demo::timing::m_::t_
