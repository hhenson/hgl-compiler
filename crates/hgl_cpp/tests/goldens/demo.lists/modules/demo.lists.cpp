// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include "demo.lists.h"
#include "core.h"

// Names this file writes, which no macro of a header above takes.
#undef a_list_input_replays_its_cells
#undef at
#undef changes
#undef changes_of
#undef core
#undef count
#undef demo
#undef first
#undef fixed_list_children
#undef high
#undef lists
#undef low
#undef made
#undef next
#undef observe
#undef observe_parts
#undef only_first
#undef only_second
#undef pair
#undef part_validity
#undef parts
#undef probe
#undef probe_pair
#undef published_whole_and_by_delta
#undef read_slot
#undef reading
#undef readings
#undef record
#undef relay
#undef replay
#undef running
#undef set_part
#undef set_slot
#undef slot
#undef slot_of
#undef span
#undef span_of
#undef sum
#undef total
#undef tuple_positions
#undef value
#undef values

namespace hgl::demo::lists::m_
{

namespace n_
{
// readings(count: i64) -> list<i64> (node), module demo.lists, line 10
struct readings
{
    static constexpr auto name = "demo.lists::readings";

    static void eval([[maybe_unused]] ::hgraph::In<"count", ::hgraph::TS<::hgraph::Int>> count, [[maybe_unused]] ::hgraph::Out<::hgraph::TSL<::hgraph::TS<::hgraph::Int>>> out_)
    {
        // when, line 11 (TIME-3)
        {
            if (::hglrt::eq__i64_i64_f64_f64(::hglrt::checked_value(count), 0, 0.0, 0.0))
            {
                ::hglrt::publish(out_, ::hglrt::list<::hgraph::Int>{});
                return;
            }
            if (::hglrt::lt__T_T<::hgraph::Int>(::hglrt::checked_value(count), 0))
            {
                ::hglrt::publish(out_, ::hglrt::list_delta_of<::hglrt::list<::hgraph::Int>>(::hglrt::dict<::hgraph::Int, ::hgraph::Int>{}, ::hglrt::set<::hgraph::Int>{}, ::hglrt::checked_value(count)));
                return;
            }
            [[maybe_unused]] ::hglrt::list<::hgraph::Int> made = ::hglrt::list<::hgraph::Int>{};
            [[maybe_unused]] ::hgraph::Int next = 0;
            while (::hglrt::lt__T_T<::hgraph::Int>(next, ::hglrt::checked_value(count)))
            {
                ::hglrt::fn_append<::hgraph::Int>(made, ::hglrt::add__i64_i64(next, 1));
                next = ::hglrt::add__i64_i64(next, 1);
            }
            ::hglrt::publish(out_, made);
            return;
        }
    }
};
}  // namespace n_
namespace w_
{
// readings(count: i64) -> list<i64> (node), module demo.lists, line 10
::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>>> readings(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> count)
{
    return ::hgraph::wire<::hgl::demo::lists::m_::n_::readings>(wiring_, count);
}
}  // namespace w_

// sum(values: list<i64>) -> i64 (`const fn`), module demo.lists, line 24
::hgraph::Int sum([[maybe_unused]] const ::hglrt::list<::hgraph::Int>& values)
{
    [[maybe_unused]] ::hgraph::Int running = 0;
    for (const ::hgraph::Int& value : values)
    {
        running = ::hglrt::add__i64_i64(running, value);
    }
    return running;
}

namespace n_
{
// total(count: i64) -> i64 (graph), module demo.lists, line 31
struct total
{
    // The call of `sum` on line 31, lifted (FN-6).
    struct lifted_00_
    {
        static constexpr auto name = "demo.lists:sum";
        static void eval([[maybe_unused]] ::hgraph::In<"values", ::hgraph::TSL<::hgraph::TS<::hgraph::Int>>> values, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
        {
            out_.set(::hgl::demo::lists::m_::sum(::hglrt::checked_whole(values)));
            return;
        }
    };

    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> count)
    {
        return ::hgraph::wire<lifted_00_>(wiring_, ::hgl::demo::lists::m_::w_::readings(wiring_, count));
    }
};
}  // namespace n_
namespace w_
{
// total(count: i64) -> i64 (graph), module demo.lists, line 31
::hgraph::Port<::hgraph::TS<::hgraph::Int>> total(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> count)
{
    return ::hgl::demo::lists::m_::n_::total::compose(wiring_, count);
}
}  // namespace w_

namespace n_
{
// changes(values: list<i64>) -> list<i64> (node), module demo.lists, line 34
struct changes
{
    static constexpr auto name = "demo.lists::changes";

    static void eval([[maybe_unused]] ::hgraph::In<"values", ::hgraph::TSL<::hgraph::TS<::hgraph::Int>>> values, [[maybe_unused]] ::hgraph::Out<::hgraph::TSL<::hgraph::TS<::hgraph::Int>>> out_)
    {
        // when, line 35 (TIME-3)
        {
            ::hglrt::publish(out_, ::hglrt::fn_delta_value(values));
            return;
        }
    }
};
}  // namespace n_
namespace w_
{
// changes(values: list<i64>) -> list<i64> (node), module demo.lists, line 34
::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>>> changes(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>>> values)
{
    return ::hgraph::wire<::hgl::demo::lists::m_::n_::changes>(wiring_, values);
}
}  // namespace w_

namespace n_
{
// changes_of(count: i64) -> list<i64> (graph), module demo.lists, line 38
struct changes_of
{
    static ::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> count)
    {
        return ::hgl::demo::lists::m_::w_::changes(wiring_, ::hgl::demo::lists::m_::w_::readings(wiring_, count));
    }
};
}  // namespace n_
namespace w_
{
// changes_of(count: i64) -> list<i64> (graph), module demo.lists, line 38
::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>>> changes_of(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> count)
{
    return ::hgl::demo::lists::m_::n_::changes_of::compose(wiring_, count);
}
}  // namespace w_

namespace n_
{
// set_slot(slot: i64, reading: i64) -> list<i64, 2> (node), module demo.lists, line 41
struct set_slot
{
    static constexpr auto name = "demo.lists::set_slot";

    static void eval([[maybe_unused]] ::hgraph::In<"slot", ::hgraph::TS<::hgraph::Int>> slot, [[maybe_unused]] ::hgraph::In<"reading", ::hgraph::TS<::hgraph::Int>> reading, [[maybe_unused]] ::hgraph::Out<::hgraph::TSL<::hgraph::TS<::hgraph::Int>, 2>> out_)
    {
        // when, line 42 (TIME-3)
        {
            ::hglrt::publish(out_, ::hglrt::list_delta_of<std::array<::hgraph::Int, 2>>(::hglrt::dict_of<::hgraph::Int, ::hgraph::Int>({{::hglrt::checked_value(slot), ::hglrt::checked_value(reading)}}), ::hglrt::set<::hgraph::Int>{}, std::nullopt));
            return;
        }
    }
};
}  // namespace n_
namespace w_
{
// set_slot(slot: i64, reading: i64) -> list<i64, 2> (node), module demo.lists, line 41
::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>, 2>> set_slot(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> slot, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading)
{
    return ::hgraph::wire<::hgl::demo::lists::m_::n_::set_slot>(wiring_, slot, reading);
}
}  // namespace w_

namespace n_
{
// probe_pair(probe: bool, pair: list<i64, 2>, slot: i64) -> list<bool, 3> (node), module demo.lists, line 46
struct probe_pair
{
    static constexpr auto name = "demo.lists::probe_pair";

    static void eval([[maybe_unused]] ::hgraph::In<"probe", ::hgraph::TS<::hgraph::Bool>, ::hgraph::InputValidity::Unchecked> probe, [[maybe_unused]] ::hgraph::In<"pair", ::hgraph::TSL<::hgraph::TS<::hgraph::Int>, 2>, ::hgraph::InputActivity::Passive, ::hgraph::InputValidity::Unchecked> pair, [[maybe_unused]] ::hgraph::In<"slot", ::hgraph::TS<::hgraph::Int>, ::hgraph::InputActivity::Passive> slot, [[maybe_unused]] ::hgraph::Out<::hgraph::TSL<::hgraph::TS<::hgraph::Bool>, 3>> out_)
    {
        // when, line 47 (TIME-3)
        if (::hglrt::fn_modified(probe))
        {
            ::hglrt::publish(out_, std::array<::hgraph::Bool, 3>{::hglrt::fn_valid(pair), ::hglrt::fn_all_valid(pair), ::hglrt::fn_valid(::hglrt::child(pair, ::hglrt::checked_value(slot)))});
            return;
        }
    }
};
}  // namespace n_
namespace w_
{
// probe_pair(probe: bool, pair: list<i64, 2>, slot: i64) -> list<bool, 3> (node), module demo.lists, line 46
::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Bool>, 3>> probe_pair(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>, 2>> pair, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> slot)
{
    return ::hgraph::wire<::hgl::demo::lists::m_::n_::probe_pair>(wiring_, probe, pair, slot);
}
}  // namespace w_

namespace n_
{
// observe(probe: bool, slot: i64, reading: i64, at: i64) -> list<bool, 3> (graph), module demo.lists, line 52
struct observe
{
    static ::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Bool>, 3>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> slot, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> at)
    {
        return ::hgl::demo::lists::m_::w_::probe_pair(wiring_, probe, ::hgl::demo::lists::m_::w_::set_slot(wiring_, slot, reading), at);
    }
};
}  // namespace n_
namespace w_
{
// observe(probe: bool, slot: i64, reading: i64, at: i64) -> list<bool, 3> (graph), module demo.lists, line 52
::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Bool>, 3>> observe(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> slot, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> at)
{
    return ::hgl::demo::lists::m_::n_::observe::compose(wiring_, probe, slot, reading, at);
}
}  // namespace w_

namespace n_
{
// read_slot(pair: list<i64, 2>, slot: i64) -> i64 (node), module demo.lists, line 56
struct read_slot
{
    static constexpr auto name = "demo.lists::read_slot";

    static void eval([[maybe_unused]] ::hgraph::In<"pair", ::hgraph::TSL<::hgraph::TS<::hgraph::Int>, 2>> pair, [[maybe_unused]] ::hgraph::In<"slot", ::hgraph::TS<::hgraph::Int>> slot, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 57 (TIME-3)
        {
            out_.set(::hglrt::checked_whole(::hglrt::child(pair, ::hglrt::checked_value(slot))));
            return;
        }
    }
};
}  // namespace n_
namespace w_
{
// read_slot(pair: list<i64, 2>, slot: i64) -> i64 (node), module demo.lists, line 56
::hgraph::Port<::hgraph::TS<::hgraph::Int>> read_slot(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>, 2>> pair, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> slot)
{
    return ::hgraph::wire<::hgl::demo::lists::m_::n_::read_slot>(wiring_, pair, slot);
}
}  // namespace w_

namespace n_
{
// slot_of(slot: i64, reading: i64, at: i64) -> i64 (graph), module demo.lists, line 60
struct slot_of
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> slot, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> at)
    {
        return ::hgl::demo::lists::m_::w_::read_slot(wiring_, ::hgl::demo::lists::m_::w_::set_slot(wiring_, slot, reading), at);
    }
};
}  // namespace n_
namespace w_
{
// slot_of(slot: i64, reading: i64, at: i64) -> i64 (graph), module demo.lists, line 60
::hgraph::Port<::hgraph::TS<::hgraph::Int>> slot_of(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> slot, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> at)
{
    return ::hgl::demo::lists::m_::n_::slot_of::compose(wiring_, slot, reading, at);
}
}  // namespace w_

// span(low: i64, high: i64) -> i64 (`const fn`), module demo.lists, line 62
::hgraph::Int span([[maybe_unused]] ::hgraph::Int low, [[maybe_unused]] ::hgraph::Int high)
{
    return ::hglrt::sub__i64_i64(high, low);
}

namespace n_
{
// span_of(slot: i64, reading: i64) -> i64 (graph), module demo.lists, line 65
struct span_of
{
    // The call of `span` on line 67, lifted (FN-6).
    struct lifted_00_
    {
        static constexpr auto name = "demo.lists:span";
        static void eval([[maybe_unused]] ::hgraph::In<"low", ::hgraph::TS<::hgraph::Int>> low, [[maybe_unused]] ::hgraph::In<"high", ::hgraph::TS<::hgraph::Int>> high, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
        {
            out_.set(::hgl::demo::lists::m_::span(::hglrt::checked_value(low), ::hglrt::checked_value(high)));
            return;
        }
    };

    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> slot, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading)
    {
        [[maybe_unused]] const ::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>, 2>> pair = ::hgl::demo::lists::m_::w_::set_slot(wiring_, slot, reading);
        return [&] { const auto argument_01_ = ::hglrt::element_port(pair, 0); const auto argument_02_ = ::hglrt::element_port(pair, 1); return ::hgraph::wire<lifted_00_>(wiring_, argument_01_, argument_02_); }();
    }
};
}  // namespace n_
namespace w_
{
// span_of(slot: i64, reading: i64) -> i64 (graph), module demo.lists, line 65
::hgraph::Port<::hgraph::TS<::hgraph::Int>> span_of(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> slot, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading)
{
    return ::hgl::demo::lists::m_::n_::span_of::compose(wiring_, slot, reading);
}
}  // namespace w_

namespace n_
{
// set_part(first: bool, value: i64) -> tuple<i64, i64> (node), module demo.lists, line 71
struct set_part
{
    static constexpr auto name = "demo.lists::set_part";

    static void eval([[maybe_unused]] ::hgraph::In<"first", ::hgraph::TS<::hgraph::Bool>> first, [[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::Out<::hgraph::UnNamedTSB<::hgraph::Field<"_0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"_1", ::hgraph::TS<::hgraph::Int>>>> out_)
    {
        // when, line 72 (TIME-3)
        {
            [[maybe_unused]] const std::tuple<std::optional<::hgraph::Int>, std::optional<::hgraph::Int>> only_first = std::tuple<std::optional<::hgraph::Int>, std::optional<::hgraph::Int>>{::hglrt::checked_value(value), std::nullopt};
            [[maybe_unused]] const std::tuple<std::optional<::hgraph::Int>, std::optional<::hgraph::Int>> only_second = std::tuple<std::optional<::hgraph::Int>, std::optional<::hgraph::Int>>{std::nullopt, ::hglrt::checked_value(value)};
            if (::hglrt::checked_value(first))
            {
                ::hglrt::publish(out_, only_first);
                return;
            }
            ::hglrt::publish(out_, only_second);
            return;
        }
    }
};
}  // namespace n_
namespace w_
{
// set_part(first: bool, value: i64) -> tuple<i64, i64> (node), module demo.lists, line 71
::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"_0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"_1", ::hgraph::TS<::hgraph::Int>>>> set_part(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> first, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgraph::wire<::hgl::demo::lists::m_::n_::set_part>(wiring_, first, value);
}
}  // namespace w_

namespace n_
{
// part_validity(probe: bool, parts: tuple<i64, i64>) -> list<bool, 2> (node), module demo.lists, line 80
struct part_validity
{
    static constexpr auto name = "demo.lists::part_validity";

    static void eval([[maybe_unused]] ::hgraph::In<"probe", ::hgraph::TS<::hgraph::Bool>, ::hgraph::InputValidity::Unchecked> probe, [[maybe_unused]] ::hgraph::In<"parts", ::hgraph::UnNamedTSB<::hgraph::Field<"_0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"_1", ::hgraph::TS<::hgraph::Int>>>, ::hgraph::InputActivity::Passive, ::hgraph::InputValidity::Unchecked> parts, [[maybe_unused]] ::hgraph::Out<::hgraph::TSL<::hgraph::TS<::hgraph::Bool>, 2>> out_)
    {
        // when, line 81 (TIME-3)
        if (::hglrt::fn_modified(probe))
        {
            ::hglrt::publish(out_, std::array<::hgraph::Bool, 2>{::hglrt::fn_valid(parts), ::hglrt::fn_all_valid(parts)});
            return;
        }
    }
};
}  // namespace n_
namespace w_
{
// part_validity(probe: bool, parts: tuple<i64, i64>) -> list<bool, 2> (node), module demo.lists, line 80
::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Bool>, 2>> part_validity(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"_0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"_1", ::hgraph::TS<::hgraph::Int>>>> parts)
{
    return ::hgraph::wire<::hgl::demo::lists::m_::n_::part_validity>(wiring_, probe, parts);
}
}  // namespace w_

namespace n_
{
// observe_parts(probe: bool, first: bool, value: i64) -> list<bool, 2> (graph), module demo.lists, line 84
struct observe_parts
{
    static ::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Bool>, 2>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> first, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
    {
        return ::hgl::demo::lists::m_::w_::part_validity(wiring_, probe, ::hgl::demo::lists::m_::w_::set_part(wiring_, first, value));
    }
};
}  // namespace n_
namespace w_
{
// observe_parts(probe: bool, first: bool, value: i64) -> list<bool, 2> (graph), module demo.lists, line 84
::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Bool>, 2>> observe_parts(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> first, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgl::demo::lists::m_::n_::observe_parts::compose(wiring_, probe, first, value);
}
}  // namespace w_

namespace n_
{
// relay(values: list<i64>) -> list<i64> (node), module demo.lists, line 88
struct relay
{
    static constexpr auto name = "demo.lists::relay";

    static void eval([[maybe_unused]] ::hgraph::In<"values", ::hgraph::TSL<::hgraph::TS<::hgraph::Int>>> values, [[maybe_unused]] ::hgraph::Out<::hgraph::TSL<::hgraph::TS<::hgraph::Int>>> out_)
    {
        // when, line 89 (TIME-3)
        {
            ::hglrt::publish(out_, ::hglrt::checked_whole(values));
            return;
        }
    }
};
}  // namespace n_
namespace w_
{
// relay(values: list<i64>) -> list<i64> (node), module demo.lists, line 88
::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>>> relay(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>>> values)
{
    return ::hgraph::wire<::hgl::demo::lists::m_::n_::relay>(wiring_, values);
}
}  // namespace w_

}  // namespace hgl::demo::lists::m_

namespace hgl::demo::lists::m_::t_
{

void published_whole_and_by_delta()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{0, 0, 3, ::hglrt::neg__i64(1), ::hglrt::neg__i64(5)}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::lists::m_::w_::total(wiring_, input_00_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgraph::Int>{0, std::nullopt, 6, 3, 0};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_05_ = ::hglrt::testing::evaluate<::hglrt::list<::hgraph::Int>>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{2, 1}, start_));
        const auto result_04_ = ::hgl::demo::lists::m_::w_::changes_of(wiring_, input_03_);
        ::hgl::core::m_::w_::i_::record::of_1list_1i64_1_(wiring_, result_04_, ::hgraph::Str{::hglrt::testing::run_output});
        ::hglrt::testing::hold_result(wiring_, result_04_);
    });
        const auto right_06_ = ::hglrt::testing::Dense<::hglrt::list<::hgraph::Int>>{::hglrt::testing::Cell<::hglrt::list<::hgraph::Int>>{std::in_place_index<0>, ::hglrt::list_delta_of<::hglrt::list<::hgraph::Int>>(::hglrt::dict_of<::hgraph::Int, ::hgraph::Int>({{0, 1}, {1, 2}}), ::hglrt::set<::hgraph::Int>{}, 2)}, ::hglrt::testing::Cell<::hglrt::list<::hgraph::Int>>{std::in_place_index<0>, ::hglrt::list_delta_of<::hglrt::list<::hgraph::Int>>(::hglrt::dict_of<::hgraph::Int, ::hgraph::Int>({{0, 1}}), ::hglrt::set<::hgraph::Int>{1}, 1)}};
        ::hglrt::testing::expect_same(left_05_, right_06_);
    }();
}

void fixed_list_children()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_09_ = ::hglrt::testing::evaluate<std::array<::hgraph::Bool, 3>>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Bool>{true, true, true}, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{std::nullopt, 0, 1}, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_02_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{std::nullopt, 3, 8}, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 1, 1}, start_));
        const auto result_08_ = [&] { const auto argument_04_ = input_00_; const auto argument_05_ = input_01_; const auto argument_06_ = input_02_; const auto argument_07_ = input_03_; return ::hgl::demo::lists::m_::w_::observe(wiring_, argument_04_, argument_05_, argument_06_, argument_07_); }();
        ::hgl::core::m_::w_::i_::record::of_1list_1bool_1_13_1_(wiring_, result_08_, ::hgraph::Str{::hglrt::testing::run_output});
        ::hglrt::testing::hold_result(wiring_, result_08_);
    });
        const auto right_010_ = ::hglrt::testing::Dense<std::array<::hgraph::Bool, 3>>{::hglrt::testing::Cell<std::array<::hgraph::Bool, 3>>{::hglrt::testing::Whole<std::array<::hgraph::Bool, 3>>{std::array<::hgraph::Bool, 3>{false, false, false}}}, ::hglrt::testing::Cell<std::array<::hgraph::Bool, 3>>{::hglrt::testing::Whole<std::array<::hgraph::Bool, 3>>{std::array<::hgraph::Bool, 3>{true, false, false}}}, ::hglrt::testing::Cell<std::array<::hgraph::Bool, 3>>{::hglrt::testing::Whole<std::array<::hgraph::Bool, 3>>{std::array<::hgraph::Bool, 3>{true, true, true}}}};
        ::hglrt::testing::expect_same(left_09_, right_010_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_015_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_011_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{0, 1, std::nullopt, 0}, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_012_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{3, 8, std::nullopt, 4}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_013_ = input_011_; const auto argument_014_ = input_012_; return ::hgl::demo::lists::m_::w_::span_of(wiring_, argument_013_, argument_014_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_016_ = ::hglrt::testing::Dense<::hgraph::Int>{std::nullopt, 5, std::nullopt, 4};
        ::hglrt::testing::expect_same(left_015_, right_016_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_023_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_017_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{0}, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_018_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{3}, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_019_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{0}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_020_ = input_017_; const auto argument_021_ = input_018_; const auto argument_022_ = input_019_; return ::hgl::demo::lists::m_::w_::slot_of(wiring_, argument_020_, argument_021_, argument_022_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_024_ = ::hglrt::testing::Dense<::hgraph::Int>{3};
        ::hglrt::testing::expect_same(left_023_, right_024_);
    }();
    ::hglrt::testing::statement();
    ::hglrt::testing::raises("value.unset_read", [&] {
        static_cast<void>(::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_025_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{0, std::nullopt}, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_026_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{3, std::nullopt}, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_027_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{0, 1}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_028_ = input_025_; const auto argument_029_ = input_026_; const auto argument_030_ = input_027_; return ::hgl::demo::lists::m_::w_::slot_of(wiring_, argument_028_, argument_029_, argument_030_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    }));
    });
    ::hglrt::testing::statement();
    ::hglrt::testing::raises("value.index_out_of_range", [&] {
        static_cast<void>(::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_031_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{0}, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_032_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{3}, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_033_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{2}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_034_ = input_031_; const auto argument_035_ = input_032_; const auto argument_036_ = input_033_; return ::hgl::demo::lists::m_::w_::slot_of(wiring_, argument_034_, argument_035_, argument_036_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    }));
    });
}

void tuple_positions()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_07_ = ::hglrt::testing::evaluate<std::array<::hgraph::Bool, 2>>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Bool>{true, true, true}, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Bool>{std::nullopt, true, false}, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_02_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{std::nullopt, 3, 8}, start_));
        const auto result_06_ = [&] { const auto argument_03_ = input_00_; const auto argument_04_ = input_01_; const auto argument_05_ = input_02_; return ::hgl::demo::lists::m_::w_::observe_parts(wiring_, argument_03_, argument_04_, argument_05_); }();
        ::hgl::core::m_::w_::i_::record::of_1list_1bool_1_12_1_(wiring_, result_06_, ::hgraph::Str{::hglrt::testing::run_output});
        ::hglrt::testing::hold_result(wiring_, result_06_);
    });
        const auto right_08_ = ::hglrt::testing::Dense<std::array<::hgraph::Bool, 2>>{::hglrt::testing::Cell<std::array<::hgraph::Bool, 2>>{::hglrt::testing::Whole<std::array<::hgraph::Bool, 2>>{std::array<::hgraph::Bool, 2>{false, false}}}, ::hglrt::testing::Cell<std::array<::hgraph::Bool, 2>>{::hglrt::testing::Whole<std::array<::hgraph::Bool, 2>>{std::array<::hgraph::Bool, 2>{true, false}}}, ::hglrt::testing::Cell<std::array<::hgraph::Bool, 2>>{::hglrt::testing::Whole<std::array<::hgraph::Bool, 2>>{std::array<::hgraph::Bool, 2>{true, true}}}};
        ::hglrt::testing::expect_same(left_07_, right_08_);
    }();
}

void a_list_input_replays_its_cells()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_02_ = ::hglrt::testing::evaluate<::hglrt::list<::hgraph::Int>>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_1list_1i64_1_(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hglrt::list<::hgraph::Int>>{::hglrt::testing::Cell<::hglrt::list<::hgraph::Int>>{::hglrt::testing::Whole<::hglrt::list<::hgraph::Int>>{::hglrt::list<::hgraph::Int>{1, 2}}}, ::hglrt::testing::Cell<::hglrt::list<::hgraph::Int>>{std::in_place_index<0>, ::hglrt::list_delta_of<::hglrt::list<::hgraph::Int>>(::hglrt::dict_of<::hgraph::Int, ::hgraph::Int>({{2, 3}}), ::hglrt::set<::hgraph::Int>{}, std::nullopt)}, ::hglrt::testing::Cell<::hglrt::list<::hgraph::Int>>{::hglrt::testing::Whole<::hglrt::list<::hgraph::Int>>{::hglrt::list<::hgraph::Int>{5}}}}, start_));
        const auto result_01_ = ::hgl::demo::lists::m_::w_::relay(wiring_, input_00_);
        ::hgl::core::m_::w_::i_::record::of_1list_1i64_1_(wiring_, result_01_, ::hgraph::Str{::hglrt::testing::run_output});
        ::hglrt::testing::hold_result(wiring_, result_01_);
    });
        const auto right_03_ = ::hglrt::testing::Dense<::hglrt::list<::hgraph::Int>>{::hglrt::testing::Cell<::hglrt::list<::hgraph::Int>>{::hglrt::testing::Whole<::hglrt::list<::hgraph::Int>>{::hglrt::list<::hgraph::Int>{1, 2}}}, ::hglrt::testing::Cell<::hglrt::list<::hgraph::Int>>{::hglrt::testing::Whole<::hglrt::list<::hgraph::Int>>{::hglrt::list<::hgraph::Int>{1, 2, 3}}}, ::hglrt::testing::Cell<::hglrt::list<::hgraph::Int>>{std::in_place_index<0>, ::hglrt::list_delta_of<::hglrt::list<::hgraph::Int>>(::hglrt::dict_of<::hgraph::Int, ::hgraph::Int>({{0, 5}}), ::hglrt::set<::hgraph::Int>{1, 2}, 1)}};
        ::hglrt::testing::expect_same(left_02_, right_03_);
    }();
}

}  // namespace hgl::demo::lists::m_::t_
