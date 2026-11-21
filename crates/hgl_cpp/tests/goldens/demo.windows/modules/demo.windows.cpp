// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include "demo.windows.h"
#include "core.h"

// Names this file writes, which no macro of a header above takes.
#undef a_full_window_of_ticks_pushes_out_its_oldest
#undef a_lifted_helper_waits_for_all_valid
#undef a_read_before_all_valid_fails
#undef a_read_into_a_result_gives_an_error
#undef a_value_leaves_a_window_of_a_duration
#undef a_window_is_recorded_and_replayed
#undef a_window_of_a_duration_falls_back_and_stays_valid
#undef arrival
#undef checked_total
#undef code
#undef collect
#undef collect_any
#undef collect_recent
#undef collect_spanned
#undef core
#undef demo
#undef departure
#undef early_total
#undef error
#undef held
#undef length
#undef lifted_total
#undef observe_arrival
#undef observe_checked
#undef observe_departure
#undef observe_readiness
#undef observe_recent_arrival
#undef observe_recent_departure
#undef observe_recent_readiness
#undef observe_recent_validity
#undef observe_spanned
#undef observe_total
#undef observe_validity
#undef outcome
#undef probe
#undef read_or_code
#undef readiness
#undef reading
#undef readings
#undef ready_total
#undef recent_arrival
#undef recent_departure
#undef recent_readiness
#undef recent_validity
#undef record
#undef replay
#undef spanned_readiness
#undef sum
#undef total
#undef total_at_tick
#undef total_when_ready
#undef total_when_valid
#undef valid_before_all_valid
#undef validity
#undef value_or_code
#undef windows

namespace hgl::demo::windows::m_
{

// total(readings: list<i64>) -> i64 (`const fn`), module demo.windows, line 10
::hgraph::Int total([[maybe_unused]] const ::hglrt::list<::hgraph::Int>& readings)
{
    [[maybe_unused]] ::hgraph::Int sum = 0;
    for (const ::hgraph::Int& reading : readings)
    {
        sum = ::hglrt::add__i64_i64(sum, reading);
    }
    return sum;
}

// value_or_code(outcome: result<i64, error>) -> str (`const fn`), module demo.windows, line 17
::hgraph::Str value_or_code([[maybe_unused]] const ::hglrt::result<::hgraph::Int>& outcome)
{
    if ((outcome).has_value())
    {
        return ::hglrt::convert_i64<::hgraph::Str>((*outcome));
    }
    [[maybe_unused]] const std::optional<::hgraph::Str> code = ((outcome).error()).code;
    if ((!code.has_value()))
    {
        return ::hgraph::Str{"no code"};
    }
    return (*code);
}

// read_or_code(outcome: result<list<i64>, error>) -> str (`const fn`), module demo.windows, line 25
::hgraph::Str read_or_code([[maybe_unused]] const ::hglrt::result<::hglrt::list<::hgraph::Int>>& outcome)
{
    if ((outcome).has_value())
    {
        return ::hglrt::convert_i64<::hgraph::Str>(::hgl::demo::windows::m_::total((*outcome)));
    }
    [[maybe_unused]] const std::optional<::hgraph::Str> code = ((outcome).error()).code;
    if ((!code.has_value()))
    {
        return ::hgraph::Str{"no code"};
    }
    return (*code);
}

namespace n_::collect
{
// collect(reading: i64) -> rolling_window<i64, 3, 2> (node), module demo.windows, line 33
struct impl_
{
    static constexpr auto name = "demo.windows::collect";

    static void eval([[maybe_unused]] ::hgraph::In<"reading", ::hgraph::TS<::hgraph::Int>> reading, [[maybe_unused]] ::hgraph::Out<::hgraph::TSW<::hgraph::Int, 3, 2>> out_)
    {
        // when, line 34 (TIME-3)
        {
            ::hglrt::window_push(out_, ::hglrt::checked_value(reading));
            return;
        }
    }
};
}  // namespace n_::collect
namespace w_
{
// collect(reading: i64) -> rolling_window<i64, 3, 2> (node), module demo.windows, line 33
::hgraph::Port<::hgraph::TSW<::hgraph::Int, 3, 2>> collect(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading)
{
    return ::hgraph::wire<::hgl::demo::windows::m_::n_::collect::impl_>(wiring_, reading);
}
}  // namespace w_

namespace n_::collect_recent
{
// collect_recent(reading: i64) -> rolling_window<i64, 3us, 1us> (node), module demo.windows, line 38
struct impl_
{
    static constexpr auto name = "demo.windows::collect_recent";

    static void eval([[maybe_unused]] ::hgraph::In<"reading", ::hgraph::TS<::hgraph::Int>> reading, [[maybe_unused]] ::hgraph::Out<::hgraph::TSWDuration<::hgraph::Int, 3, 1>> out_)
    {
        // when, line 39 (TIME-3)
        {
            ::hglrt::window_push(out_, ::hglrt::checked_value(reading));
            return;
        }
    }
};
}  // namespace n_::collect_recent
namespace w_
{
// collect_recent(reading: i64) -> rolling_window<i64, 3us, 1us> (node), module demo.windows, line 38
::hgraph::Port<::hgraph::TSWDuration<::hgraph::Int, 3, 1>> collect_recent(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading)
{
    return ::hgraph::wire<::hgl::demo::windows::m_::n_::collect_recent::impl_>(wiring_, reading);
}
}  // namespace w_

namespace n_::collect_any
{
// collect_any(reading: i64) -> rolling_window<i64, 3us, 0s> (node), module demo.windows, line 43
struct impl_
{
    static constexpr auto name = "demo.windows::collect_any";

    static void eval([[maybe_unused]] ::hgraph::In<"reading", ::hgraph::TS<::hgraph::Int>> reading, [[maybe_unused]] ::hgraph::Out<::hgraph::TSWDuration<::hgraph::Int, 3, 0>> out_)
    {
        // when, line 44 (TIME-3)
        {
            ::hglrt::window_push(out_, ::hglrt::checked_value(reading));
            return;
        }
    }
};
}  // namespace n_::collect_any
namespace w_
{
// collect_any(reading: i64) -> rolling_window<i64, 3us, 0s> (node), module demo.windows, line 43
::hgraph::Port<::hgraph::TSWDuration<::hgraph::Int, 3, 0>> collect_any(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading)
{
    return ::hgraph::wire<::hgl::demo::windows::m_::n_::collect_any::impl_>(wiring_, reading);
}
}  // namespace w_

namespace n_::validity
{
// validity(probe: bool, readings: rolling_window<i64, 3, 2>) -> bool (node), module demo.windows, line 47
struct impl_
{
    static constexpr auto name = "demo.windows::validity";

    static void eval([[maybe_unused]] ::hgraph::In<"probe", ::hgraph::TS<::hgraph::Bool>, ::hgraph::InputValidity::Unchecked> probe, [[maybe_unused]] ::hgraph::In<"readings", ::hgraph::TSW<::hgraph::Int, 3, 2>, ::hgraph::InputActivity::Passive, ::hgraph::InputValidity::Unchecked> readings, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Bool>> out_)
    {
        // when, line 48 (TIME-3)
        if (::hglrt::fn_modified(probe))
        {
            out_.set(::hglrt::fn_valid(readings));
            return;
        }
    }
};
}  // namespace n_::validity
namespace w_
{
// validity(probe: bool, readings: rolling_window<i64, 3, 2>) -> bool (node), module demo.windows, line 47
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> validity(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::TSW<::hgraph::Int, 3, 2>> readings)
{
    return ::hgraph::wire<::hgl::demo::windows::m_::n_::validity::impl_>(wiring_, probe, readings);
}
}  // namespace w_

namespace n_::readiness
{
// readiness(probe: bool, readings: rolling_window<i64, 3, 2>) -> bool (node), module demo.windows, line 51
struct impl_
{
    static constexpr auto name = "demo.windows::readiness";

    static void eval([[maybe_unused]] ::hgraph::In<"probe", ::hgraph::TS<::hgraph::Bool>, ::hgraph::InputValidity::Unchecked> probe, [[maybe_unused]] ::hgraph::In<"readings", ::hgraph::TSW<::hgraph::Int, 3, 2>, ::hgraph::InputActivity::Passive, ::hgraph::InputValidity::Unchecked> readings, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Bool>> out_)
    {
        // when, line 52 (TIME-3)
        if (::hglrt::fn_modified(probe))
        {
            out_.set(::hglrt::fn_all_valid(readings));
            return;
        }
    }
};
}  // namespace n_::readiness
namespace w_
{
// readiness(probe: bool, readings: rolling_window<i64, 3, 2>) -> bool (node), module demo.windows, line 51
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> readiness(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::TSW<::hgraph::Int, 3, 2>> readings)
{
    return ::hgraph::wire<::hgl::demo::windows::m_::n_::readiness::impl_>(wiring_, probe, readings);
}
}  // namespace w_

namespace n_::observe_validity
{
// observe_validity(probe: bool, reading: i64) -> bool (graph), module demo.windows, line 55
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading)
    {
        return ::hgl::demo::windows::m_::w_::validity(wiring_, probe, ::hgl::demo::windows::m_::w_::collect(wiring_, reading));
    }
};
}  // namespace n_::observe_validity
namespace w_
{
// observe_validity(probe: bool, reading: i64) -> bool (graph), module demo.windows, line 55
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> observe_validity(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading)
{
    return ::hgl::demo::windows::m_::n_::observe_validity::impl_::compose(wiring_, probe, reading);
}
}  // namespace w_

namespace n_::observe_readiness
{
// observe_readiness(probe: bool, reading: i64) -> bool (graph), module demo.windows, line 57
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading)
    {
        return ::hgl::demo::windows::m_::w_::readiness(wiring_, probe, ::hgl::demo::windows::m_::w_::collect(wiring_, reading));
    }
};
}  // namespace n_::observe_readiness
namespace w_
{
// observe_readiness(probe: bool, reading: i64) -> bool (graph), module demo.windows, line 57
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> observe_readiness(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading)
{
    return ::hgl::demo::windows::m_::n_::observe_readiness::impl_::compose(wiring_, probe, reading);
}
}  // namespace w_

namespace n_::total_when_ready
{
// total_when_ready(readings: rolling_window<i64, 3, 2>) -> i64 (node), module demo.windows, line 66
struct impl_
{
    static constexpr auto name = "demo.windows::total_when_ready";

    static void eval([[maybe_unused]] ::hgraph::In<"readings", ::hgraph::TSW<::hgraph::Int, 3, 2>> readings, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 67 (TIME-3)
        if (::hglrt::fn_all_valid(readings))
        {
            out_.set(::hgl::demo::windows::m_::total(::hglrt::window_value(readings)));
            return;
        }
    }
};
}  // namespace n_::total_when_ready
namespace w_
{
// total_when_ready(readings: rolling_window<i64, 3, 2>) -> i64 (node), module demo.windows, line 66
::hgraph::Port<::hgraph::TS<::hgraph::Int>> total_when_ready(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSW<::hgraph::Int, 3, 2>> readings)
{
    return ::hgraph::wire<::hgl::demo::windows::m_::n_::total_when_ready::impl_>(wiring_, readings);
}
}  // namespace w_

namespace n_::total_when_valid
{
// total_when_valid(readings: rolling_window<i64, 3, 2>) -> i64 (node), module demo.windows, line 70
struct impl_
{
    static constexpr auto name = "demo.windows::total_when_valid";

    static void eval([[maybe_unused]] ::hgraph::In<"readings", ::hgraph::TSW<::hgraph::Int, 3, 2>> readings, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 71 (TIME-3)
        {
            out_.set(::hgl::demo::windows::m_::total(::hglrt::window_value(readings)));
            return;
        }
    }
};
}  // namespace n_::total_when_valid
namespace w_
{
// total_when_valid(readings: rolling_window<i64, 3, 2>) -> i64 (node), module demo.windows, line 70
::hgraph::Port<::hgraph::TS<::hgraph::Int>> total_when_valid(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSW<::hgraph::Int, 3, 2>> readings)
{
    return ::hgraph::wire<::hgl::demo::windows::m_::n_::total_when_valid::impl_>(wiring_, readings);
}
}  // namespace w_

namespace n_::ready_total
{
// ready_total(reading: i64) -> i64 (graph), module demo.windows, line 74
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading)
    {
        return ::hgl::demo::windows::m_::w_::total_when_ready(wiring_, ::hgl::demo::windows::m_::w_::collect(wiring_, reading));
    }
};
}  // namespace n_::ready_total
namespace w_
{
// ready_total(reading: i64) -> i64 (graph), module demo.windows, line 74
::hgraph::Port<::hgraph::TS<::hgraph::Int>> ready_total(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading)
{
    return ::hgl::demo::windows::m_::n_::ready_total::impl_::compose(wiring_, reading);
}
}  // namespace w_

namespace n_::early_total
{
// early_total(reading: i64) -> i64 (graph), module demo.windows, line 76
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading)
    {
        return ::hgl::demo::windows::m_::w_::total_when_valid(wiring_, ::hgl::demo::windows::m_::w_::collect(wiring_, reading));
    }
};
}  // namespace n_::early_total
namespace w_
{
// early_total(reading: i64) -> i64 (graph), module demo.windows, line 76
::hgraph::Port<::hgraph::TS<::hgraph::Int>> early_total(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading)
{
    return ::hgl::demo::windows::m_::n_::early_total::impl_::compose(wiring_, reading);
}
}  // namespace w_

namespace n_::checked_total
{
// checked_total(readings: rolling_window<i64, 3, 2>) -> str (node), module demo.windows, line 87
struct impl_
{
    static constexpr auto name = "demo.windows::checked_total";

    static void eval([[maybe_unused]] ::hgraph::In<"readings", ::hgraph::TSW<::hgraph::Int, 3, 2>> readings, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Str>> out_)
    {
        // when, line 88 (TIME-3)
        {
            [[maybe_unused]] const ::hglrt::result<::hglrt::list<::hgraph::Int>> held = ::hglrt::window_value_checked(readings);
            out_.set(::hgl::demo::windows::m_::read_or_code(held));
            return;
        }
    }
};
}  // namespace n_::checked_total
namespace w_
{
// checked_total(readings: rolling_window<i64, 3, 2>) -> str (node), module demo.windows, line 87
::hgraph::Port<::hgraph::TS<::hgraph::Str>> checked_total(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSW<::hgraph::Int, 3, 2>> readings)
{
    return ::hgraph::wire<::hgl::demo::windows::m_::n_::checked_total::impl_>(wiring_, readings);
}
}  // namespace w_

namespace n_::observe_checked
{
// observe_checked(reading: i64) -> str (graph), module demo.windows, line 94
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Str>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading)
    {
        return ::hgl::demo::windows::m_::w_::checked_total(wiring_, ::hgl::demo::windows::m_::w_::collect(wiring_, reading));
    }
};
}  // namespace n_::observe_checked
namespace w_
{
// observe_checked(reading: i64) -> str (graph), module demo.windows, line 94
::hgraph::Port<::hgraph::TS<::hgraph::Str>> observe_checked(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading)
{
    return ::hgl::demo::windows::m_::n_::observe_checked::impl_::compose(wiring_, reading);
}
}  // namespace w_

namespace n_::lifted_total
{
// lifted_total(reading: i64) -> i64 (graph), module demo.windows, line 102
struct impl_
{
    // The call of `total` on line 102, lifted (FN-6).
    struct lifted_00_
    {
        static constexpr auto name = "demo.windows:total";
        static void eval([[maybe_unused]] ::hgraph::In<"readings", ::hgraph::TSW<::hgraph::Int, 3, 2>> readings, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
        {
            if (!::hglrt::fn_all_valid(readings))
            {
                return;
            }
            out_.set(::hgl::demo::windows::m_::total(::hglrt::window_value(readings)));
            return;
        }
    };

    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading)
    {
        return ::hgraph::wire<lifted_00_>(wiring_, ::hgl::demo::windows::m_::w_::collect(wiring_, reading));
    }
};
}  // namespace n_::lifted_total
namespace w_
{
// lifted_total(reading: i64) -> i64 (graph), module demo.windows, line 102
::hgraph::Port<::hgraph::TS<::hgraph::Int>> lifted_total(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading)
{
    return ::hgl::demo::windows::m_::n_::lifted_total::impl_::compose(wiring_, reading);
}
}  // namespace w_

namespace n_::arrival
{
// arrival(readings: rolling_window<i64, 3, 2>) -> i64 (node), module demo.windows, line 109
struct impl_
{
    static constexpr auto name = "demo.windows::arrival";

    static void eval([[maybe_unused]] ::hgraph::In<"readings", ::hgraph::TSW<::hgraph::Int, 3, 2>> readings, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 110 (TIME-3)
        if (::hglrt::fn_modified(readings))
        {
            out_.set(::hglrt::fn_delta_value(readings));
            return;
        }
    }
};
}  // namespace n_::arrival
namespace w_
{
// arrival(readings: rolling_window<i64, 3, 2>) -> i64 (node), module demo.windows, line 109
::hgraph::Port<::hgraph::TS<::hgraph::Int>> arrival(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSW<::hgraph::Int, 3, 2>> readings)
{
    return ::hgraph::wire<::hgl::demo::windows::m_::n_::arrival::impl_>(wiring_, readings);
}
}  // namespace w_

namespace n_::departure
{
// departure(readings: rolling_window<i64, 3, 2>) -> str (node), module demo.windows, line 113
struct impl_
{
    static constexpr auto name = "demo.windows::departure";

    static void eval([[maybe_unused]] ::hgraph::In<"readings", ::hgraph::TSW<::hgraph::Int, 3, 2>> readings, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Str>> out_)
    {
        // when, line 114 (TIME-3)
        if (::hglrt::fn_modified(readings))
        {
            out_.set(::hgl::demo::windows::m_::value_or_code(::hglrt::fn_removed_value_checked(readings)));
            return;
        }
    }
};
}  // namespace n_::departure
namespace w_
{
// departure(readings: rolling_window<i64, 3, 2>) -> str (node), module demo.windows, line 113
::hgraph::Port<::hgraph::TS<::hgraph::Str>> departure(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSW<::hgraph::Int, 3, 2>> readings)
{
    return ::hgraph::wire<::hgl::demo::windows::m_::n_::departure::impl_>(wiring_, readings);
}
}  // namespace w_

namespace n_::observe_arrival
{
// observe_arrival(reading: i64) -> i64 (graph), module demo.windows, line 117
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading)
    {
        return ::hgl::demo::windows::m_::w_::arrival(wiring_, ::hgl::demo::windows::m_::w_::collect(wiring_, reading));
    }
};
}  // namespace n_::observe_arrival
namespace w_
{
// observe_arrival(reading: i64) -> i64 (graph), module demo.windows, line 117
::hgraph::Port<::hgraph::TS<::hgraph::Int>> observe_arrival(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading)
{
    return ::hgl::demo::windows::m_::n_::observe_arrival::impl_::compose(wiring_, reading);
}
}  // namespace w_

namespace n_::observe_departure
{
// observe_departure(reading: i64) -> str (graph), module demo.windows, line 119
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Str>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading)
    {
        return ::hgl::demo::windows::m_::w_::departure(wiring_, ::hgl::demo::windows::m_::w_::collect(wiring_, reading));
    }
};
}  // namespace n_::observe_departure
namespace w_
{
// observe_departure(reading: i64) -> str (graph), module demo.windows, line 119
::hgraph::Port<::hgraph::TS<::hgraph::Str>> observe_departure(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading)
{
    return ::hgl::demo::windows::m_::n_::observe_departure::impl_::compose(wiring_, reading);
}
}  // namespace w_

namespace n_::recent_readiness
{
// recent_readiness(readings: rolling_window<i64, 3us, 1us>) -> bool (node), module demo.windows, line 129
struct impl_
{
    static constexpr auto name = "demo.windows::recent_readiness";

    static void eval([[maybe_unused]] ::hgraph::In<"readings", ::hgraph::TSWDuration<::hgraph::Int, 3, 1>> readings, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Bool>> out_)
    {
        // when, line 130 (TIME-3)
        if (::hglrt::fn_modified(readings))
        {
            out_.set(::hglrt::fn_all_valid(readings));
            return;
        }
    }
};
}  // namespace n_::recent_readiness
namespace w_
{
// recent_readiness(readings: rolling_window<i64, 3us, 1us>) -> bool (node), module demo.windows, line 129
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> recent_readiness(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSWDuration<::hgraph::Int, 3, 1>> readings)
{
    return ::hgraph::wire<::hgl::demo::windows::m_::n_::recent_readiness::impl_>(wiring_, readings);
}
}  // namespace w_

namespace n_::recent_arrival
{
// recent_arrival(readings: rolling_window<i64, 3us, 1us>) -> str (node), module demo.windows, line 133
struct impl_
{
    static constexpr auto name = "demo.windows::recent_arrival";

    static void eval([[maybe_unused]] ::hgraph::In<"readings", ::hgraph::TSWDuration<::hgraph::Int, 3, 1>> readings, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Str>> out_)
    {
        // when, line 134 (TIME-3)
        if (::hglrt::fn_modified(readings))
        {
            out_.set(::hgl::demo::windows::m_::value_or_code(::hglrt::fn_delta_value_checked(readings)));
            return;
        }
    }
};
}  // namespace n_::recent_arrival
namespace w_
{
// recent_arrival(readings: rolling_window<i64, 3us, 1us>) -> str (node), module demo.windows, line 133
::hgraph::Port<::hgraph::TS<::hgraph::Str>> recent_arrival(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSWDuration<::hgraph::Int, 3, 1>> readings)
{
    return ::hgraph::wire<::hgl::demo::windows::m_::n_::recent_arrival::impl_>(wiring_, readings);
}
}  // namespace w_

namespace n_::recent_departure
{
// recent_departure(readings: rolling_window<i64, 3us, 1us>) -> str (node), module demo.windows, line 137
struct impl_
{
    static constexpr auto name = "demo.windows::recent_departure";

    static void eval([[maybe_unused]] ::hgraph::In<"readings", ::hgraph::TSWDuration<::hgraph::Int, 3, 1>> readings, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Str>> out_)
    {
        // when, line 138 (TIME-3)
        if (::hglrt::fn_modified(readings))
        {
            out_.set(::hgl::demo::windows::m_::value_or_code(::hglrt::fn_removed_value_checked(readings)));
            return;
        }
    }
};
}  // namespace n_::recent_departure
namespace w_
{
// recent_departure(readings: rolling_window<i64, 3us, 1us>) -> str (node), module demo.windows, line 137
::hgraph::Port<::hgraph::TS<::hgraph::Str>> recent_departure(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSWDuration<::hgraph::Int, 3, 1>> readings)
{
    return ::hgraph::wire<::hgl::demo::windows::m_::n_::recent_departure::impl_>(wiring_, readings);
}
}  // namespace w_

namespace n_::observe_recent_readiness
{
// observe_recent_readiness(reading: i64) -> bool (graph), module demo.windows, line 141
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading)
    {
        return ::hgl::demo::windows::m_::w_::recent_readiness(wiring_, ::hgl::demo::windows::m_::w_::collect_recent(wiring_, reading));
    }
};
}  // namespace n_::observe_recent_readiness
namespace w_
{
// observe_recent_readiness(reading: i64) -> bool (graph), module demo.windows, line 141
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> observe_recent_readiness(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading)
{
    return ::hgl::demo::windows::m_::n_::observe_recent_readiness::impl_::compose(wiring_, reading);
}
}  // namespace w_

namespace n_::observe_recent_arrival
{
// observe_recent_arrival(reading: i64) -> str (graph), module demo.windows, line 143
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Str>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading)
    {
        return ::hgl::demo::windows::m_::w_::recent_arrival(wiring_, ::hgl::demo::windows::m_::w_::collect_recent(wiring_, reading));
    }
};
}  // namespace n_::observe_recent_arrival
namespace w_
{
// observe_recent_arrival(reading: i64) -> str (graph), module demo.windows, line 143
::hgraph::Port<::hgraph::TS<::hgraph::Str>> observe_recent_arrival(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading)
{
    return ::hgl::demo::windows::m_::n_::observe_recent_arrival::impl_::compose(wiring_, reading);
}
}  // namespace w_

namespace n_::observe_recent_departure
{
// observe_recent_departure(reading: i64) -> str (graph), module demo.windows, line 145
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Str>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading)
    {
        return ::hgl::demo::windows::m_::w_::recent_departure(wiring_, ::hgl::demo::windows::m_::w_::collect_recent(wiring_, reading));
    }
};
}  // namespace n_::observe_recent_departure
namespace w_
{
// observe_recent_departure(reading: i64) -> str (graph), module demo.windows, line 145
::hgraph::Port<::hgraph::TS<::hgraph::Str>> observe_recent_departure(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading)
{
    return ::hgl::demo::windows::m_::n_::observe_recent_departure::impl_::compose(wiring_, reading);
}
}  // namespace w_

namespace n_::collect_spanned
{
// collect_spanned(reading: i64) -> rolling_window<i64, 4us, 2us> (node), module demo.windows, line 163
struct impl_
{
    static constexpr auto name = "demo.windows::collect_spanned";

    static void eval([[maybe_unused]] ::hgraph::In<"reading", ::hgraph::TS<::hgraph::Int>> reading, [[maybe_unused]] ::hgraph::Out<::hgraph::TSWDuration<::hgraph::Int, 4, 2>> out_)
    {
        // when, line 164 (TIME-3)
        {
            ::hglrt::window_push(out_, ::hglrt::checked_value(reading));
            return;
        }
    }
};
}  // namespace n_::collect_spanned
namespace w_
{
// collect_spanned(reading: i64) -> rolling_window<i64, 4us, 2us> (node), module demo.windows, line 163
::hgraph::Port<::hgraph::TSWDuration<::hgraph::Int, 4, 2>> collect_spanned(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading)
{
    return ::hgraph::wire<::hgl::demo::windows::m_::n_::collect_spanned::impl_>(wiring_, reading);
}
}  // namespace w_

namespace n_::spanned_readiness
{
// spanned_readiness(probe: bool, readings: rolling_window<i64, 4us, 2us>) -> bool (node), module demo.windows, line 167
struct impl_
{
    static constexpr auto name = "demo.windows::spanned_readiness";

    static void eval([[maybe_unused]] ::hgraph::In<"probe", ::hgraph::TS<::hgraph::Bool>, ::hgraph::InputValidity::Unchecked> probe, [[maybe_unused]] ::hgraph::In<"readings", ::hgraph::TSWDuration<::hgraph::Int, 4, 2>, ::hgraph::InputActivity::Passive, ::hgraph::InputValidity::Unchecked> readings, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Bool>> out_)
    {
        // when, line 168 (TIME-3)
        if (::hglrt::fn_modified(probe))
        {
            out_.set(::hglrt::fn_all_valid(readings));
            return;
        }
    }
};
}  // namespace n_::spanned_readiness
namespace w_
{
// spanned_readiness(probe: bool, readings: rolling_window<i64, 4us, 2us>) -> bool (node), module demo.windows, line 167
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> spanned_readiness(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::TSWDuration<::hgraph::Int, 4, 2>> readings)
{
    return ::hgraph::wire<::hgl::demo::windows::m_::n_::spanned_readiness::impl_>(wiring_, probe, readings);
}
}  // namespace w_

namespace n_::recent_validity
{
// recent_validity(readings: rolling_window<i64, 3us, 1us>) -> bool (node), module demo.windows, line 171
struct impl_
{
    static constexpr auto name = "demo.windows::recent_validity";

    static void eval([[maybe_unused]] ::hgraph::In<"readings", ::hgraph::TSWDuration<::hgraph::Int, 3, 1>> readings, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Bool>> out_)
    {
        // when, line 172 (TIME-3)
        if (::hglrt::fn_modified(readings))
        {
            out_.set(::hglrt::fn_valid(readings));
            return;
        }
    }
};
}  // namespace n_::recent_validity
namespace w_
{
// recent_validity(readings: rolling_window<i64, 3us, 1us>) -> bool (node), module demo.windows, line 171
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> recent_validity(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSWDuration<::hgraph::Int, 3, 1>> readings)
{
    return ::hgraph::wire<::hgl::demo::windows::m_::n_::recent_validity::impl_>(wiring_, readings);
}
}  // namespace w_

namespace n_::observe_spanned
{
// observe_spanned(probe: bool, reading: i64) -> bool (graph), module demo.windows, line 175
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading)
    {
        return ::hgl::demo::windows::m_::w_::spanned_readiness(wiring_, probe, ::hgl::demo::windows::m_::w_::collect_spanned(wiring_, reading));
    }
};
}  // namespace n_::observe_spanned
namespace w_
{
// observe_spanned(probe: bool, reading: i64) -> bool (graph), module demo.windows, line 175
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> observe_spanned(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading)
{
    return ::hgl::demo::windows::m_::n_::observe_spanned::impl_::compose(wiring_, probe, reading);
}
}  // namespace w_

namespace n_::observe_recent_validity
{
// observe_recent_validity(reading: i64) -> bool (graph), module demo.windows, line 177
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading)
    {
        return ::hgl::demo::windows::m_::w_::recent_validity(wiring_, ::hgl::demo::windows::m_::w_::collect_recent(wiring_, reading));
    }
};
}  // namespace n_::observe_recent_validity
namespace w_
{
// observe_recent_validity(reading: i64) -> bool (graph), module demo.windows, line 177
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> observe_recent_validity(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading)
{
    return ::hgl::demo::windows::m_::n_::observe_recent_validity::impl_::compose(wiring_, reading);
}
}  // namespace w_

namespace n_::total_at_tick
{
// total_at_tick(readings: rolling_window<i64, 3us, 0s>) -> i64 (node), module demo.windows, line 189
struct impl_
{
    static constexpr auto name = "demo.windows::total_at_tick";

    static void eval([[maybe_unused]] ::hgraph::In<"readings", ::hgraph::TSWDuration<::hgraph::Int, 3, 0>> readings, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 190 (TIME-3)
        if (::hglrt::fn_modified(readings))
        {
            if (::hglrt::fn_all_valid(readings))
            {
                out_.set(::hgl::demo::windows::m_::total(::hglrt::window_value(readings)));
                return;
            }
            out_.set(::hglrt::neg__i64(1));
            return;
        }
    }
};
}  // namespace n_::total_at_tick
namespace w_
{
// total_at_tick(readings: rolling_window<i64, 3us, 0s>) -> i64 (node), module demo.windows, line 189
::hgraph::Port<::hgraph::TS<::hgraph::Int>> total_at_tick(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSWDuration<::hgraph::Int, 3, 0>> readings)
{
    return ::hgraph::wire<::hgl::demo::windows::m_::n_::total_at_tick::impl_>(wiring_, readings);
}
}  // namespace w_

namespace n_::observe_total
{
// observe_total(reading: i64) -> i64 (graph), module demo.windows, line 196
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading)
    {
        return ::hgl::demo::windows::m_::w_::total_at_tick(wiring_, ::hgl::demo::windows::m_::w_::collect_any(wiring_, reading));
    }
};
}  // namespace n_::observe_total
namespace w_
{
// observe_total(reading: i64) -> i64 (graph), module demo.windows, line 196
::hgraph::Port<::hgraph::TS<::hgraph::Int>> observe_total(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading)
{
    return ::hgl::demo::windows::m_::n_::observe_total::impl_::compose(wiring_, reading);
}
}  // namespace w_

}  // namespace hgl::demo::windows::m_

namespace hgl::demo::windows::m_::t_
{

void valid_before_all_valid()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_06_ = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Bool>{true, true, true};
        const auto trace_02_ = ::hglrt::testing::Dense<::hgraph::Int>{std::nullopt, 1, 2};
        return ::hglrt::testing::evaluate<::hgraph::Bool>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_02_, start_));
        ::hgl::core::m_::w_::i_::record::of_bool(wiring_, [&] { const auto argument_04_ = input_01_; const auto argument_05_ = input_03_; return ::hgl::demo::windows::m_::w_::observe_validity(wiring_, argument_04_, argument_05_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_07_ = ::hglrt::testing::Dense<::hgraph::Bool>{false, true, true};
        ::hglrt::testing::expect_same(left_06_, right_07_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_014_ = [&]() {
        const auto trace_08_ = ::hglrt::testing::Dense<::hgraph::Bool>{true, true, true};
        const auto trace_010_ = ::hglrt::testing::Dense<::hgraph::Int>{std::nullopt, 1, 2};
        return ::hglrt::testing::evaluate<::hgraph::Bool>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_09_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(trace_08_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_011_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_010_, start_));
        ::hgl::core::m_::w_::i_::record::of_bool(wiring_, [&] { const auto argument_012_ = input_09_; const auto argument_013_ = input_011_; return ::hgl::demo::windows::m_::w_::observe_readiness(wiring_, argument_012_, argument_013_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_015_ = ::hglrt::testing::Dense<::hgraph::Bool>{false, false, true};
        ::hglrt::testing::expect_same(left_014_, right_015_);
    }();
}

void a_read_before_all_valid_fails()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 2, 3, 4}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::windows::m_::w_::ready_total(wiring_, input_00_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgraph::Int>{std::nullopt, 3, 6, 9};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
    ::hglrt::testing::statement();
    ::hglrt::testing::raises("value.unset_read", [&] {
        static_cast<void>(::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 2}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::windows::m_::w_::early_total(wiring_, input_03_), ::hgraph::Str{::hglrt::testing::run_output});
    }));
    });
}

void a_read_into_a_result_gives_an_error()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgraph::Str>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 2, 3}, start_));
        ::hgl::core::m_::w_::i_::record::of_str(wiring_, ::hgl::demo::windows::m_::w_::observe_checked(wiring_, input_00_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgraph::Str>{::hgraph::Str{"value.unset_read"}, ::hgraph::Str{"3"}, ::hgraph::Str{"6"}};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
}

void a_lifted_helper_waits_for_all_valid()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 2, 3, 4}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::windows::m_::w_::lifted_total(wiring_, input_00_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgraph::Int>{std::nullopt, 3, 6, 9};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
}

void a_full_window_of_ticks_pushes_out_its_oldest()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 2, 3, 4}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::windows::m_::w_::observe_arrival(wiring_, input_00_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 2, 3, 4};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_04_ = ::hglrt::testing::evaluate<::hgraph::Str>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 2, 3, 4}, start_));
        ::hgl::core::m_::w_::i_::record::of_str(wiring_, ::hgl::demo::windows::m_::w_::observe_departure(wiring_, input_03_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_05_ = ::hglrt::testing::Dense<::hgraph::Str>{::hgraph::Str{"value.unset_read"}, ::hgraph::Str{"value.unset_read"}, ::hgraph::Str{"value.unset_read"}, ::hgraph::Str{"1"}};
        ::hglrt::testing::expect_same(left_04_, right_05_);
    }();
}

void a_value_leaves_a_window_of_a_duration()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgraph::Bool>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 2}, start_));
        ::hgl::core::m_::w_::i_::record::of_bool(wiring_, ::hgl::demo::windows::m_::w_::observe_recent_readiness(wiring_, input_00_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgraph::Bool>{false, true, std::nullopt, std::nullopt, false, false};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_04_ = ::hglrt::testing::evaluate<::hgraph::Str>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 2, std::nullopt, std::nullopt, std::nullopt, 3}, start_));
        ::hgl::core::m_::w_::i_::record::of_str(wiring_, ::hgl::demo::windows::m_::w_::observe_recent_arrival(wiring_, input_03_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_05_ = ::hglrt::testing::Dense<::hgraph::Str>{::hgraph::Str{"1"}, ::hgraph::Str{"2"}, std::nullopt, std::nullopt, ::hgraph::Str{"value.unset_read"}, ::hgraph::Str{"3"}, std::nullopt, std::nullopt, std::nullopt, ::hgraph::Str{"value.unset_read"}};
        ::hglrt::testing::expect_same(left_04_, right_05_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_07_ = ::hglrt::testing::evaluate<::hgraph::Str>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_06_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 2, std::nullopt, std::nullopt, std::nullopt, 3}, start_));
        ::hgl::core::m_::w_::i_::record::of_str(wiring_, ::hgl::demo::windows::m_::w_::observe_recent_departure(wiring_, input_06_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_08_ = ::hglrt::testing::Dense<::hgraph::Str>{::hgraph::Str{"value.unset_read"}, ::hgraph::Str{"value.unset_read"}, std::nullopt, std::nullopt, ::hgraph::Str{"1"}, ::hgraph::Str{"2"}, std::nullopt, std::nullopt, std::nullopt, ::hgraph::Str{"3"}};
        ::hglrt::testing::expect_same(left_07_, right_08_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_012_ = [&]() {
        const auto trace_09_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 2, std::nullopt, std::nullopt, std::nullopt, 3};
        const auto bounds_011_ = ::hglrt::testing::Bounds{.length = ::hgraph::TimeDelta{6}};
        return ::hglrt::testing::evaluate<::hgraph::Str>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_010_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_09_, start_));
        ::hgl::core::m_::w_::i_::record::of_str(wiring_, ::hgl::demo::windows::m_::w_::observe_recent_departure(wiring_, input_010_), ::hgraph::Str{::hglrt::testing::run_output});
    }, bounds_011_);
    }();
        const auto right_013_ = ::hglrt::testing::Dense<::hgraph::Str>{::hgraph::Str{"value.unset_read"}, ::hgraph::Str{"value.unset_read"}, std::nullopt, std::nullopt, ::hgraph::Str{"1"}, ::hgraph::Str{"2"}};
        ::hglrt::testing::expect_same(left_012_, right_013_);
    }();
}

void a_window_of_a_duration_falls_back_and_stays_valid()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_06_ = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Bool>{true, true, true, true, true, true};
        const auto trace_02_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 2, 3, std::nullopt, std::nullopt, std::nullopt};
        return ::hglrt::testing::evaluate<::hgraph::Bool>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_02_, start_));
        ::hgl::core::m_::w_::i_::record::of_bool(wiring_, [&] { const auto argument_04_ = input_01_; const auto argument_05_ = input_03_; return ::hgl::demo::windows::m_::w_::observe_spanned(wiring_, argument_04_, argument_05_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_07_ = ::hglrt::testing::Dense<::hgraph::Bool>{false, false, true, true, true, false};
        ::hglrt::testing::expect_same(left_06_, right_07_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_09_ = ::hglrt::testing::evaluate<::hgraph::Bool>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_08_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1}, start_));
        ::hgl::core::m_::w_::i_::record::of_bool(wiring_, ::hgl::demo::windows::m_::w_::observe_recent_validity(wiring_, input_08_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_010_ = ::hglrt::testing::Dense<::hgraph::Bool>{true, std::nullopt, std::nullopt, std::nullopt, true};
        ::hglrt::testing::expect_same(left_09_, right_010_);
    }();
}

void a_window_is_recorded_and_replayed()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 2}, start_));
        ::hgl::core::m_::w_::i_::record::of_1rolling_1window_1i64_1_13us_1_10us_1_(wiring_, ::hgl::demo::windows::m_::w_::collect_any(wiring_, input_00_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 2};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_04_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 2}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::windows::m_::w_::observe_total(wiring_, input_03_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_05_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 3, std::nullopt, std::nullopt, 2, ::hglrt::neg__i64(1)};
        ::hglrt::testing::expect_same(left_04_, right_05_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_07_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TSWDuration<::hgraph::Int, 3, 0>> input_06_ = ::hgl::core::m_::w_::i_::replay::of_1rolling_1window_1i64_1_13us_1_10us_1_(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 2}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::windows::m_::w_::total_at_tick(wiring_, input_06_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_08_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 3, std::nullopt, std::nullopt, 2, ::hglrt::neg__i64(1)};
        ::hglrt::testing::expect_same(left_07_, right_08_);
    }();
}

}  // namespace hgl::demo::windows::m_::t_
