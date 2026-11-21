// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include "demo.lists.h"
#include "core.h"

// Names this file writes, which no macro of a header above takes.
#undef a_list_input_replays_its_cells
#undef at
#undef change
#undef changes
#undef changes_of
#undef children_named_in_a_hook
#undef core
#undef count
#undef demo
#undef first
#undef first_of
#undef first_slot
#undef fixed_list_children
#undef get
#undef head
#undef high
#undef lists
#undef low
#undef made
#undef nested
#undef nested_of
#undef nested_valid
#undef next
#undef observe
#undef observe_parts
#undef only_first
#undef only_second
#undef ordered_child
#undef ordered_connection
#undef ordered_of
#undef ordered_port
#undef ordered_row
#undef ordered_row_of
#undef pair
#undef part_sum
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
#undef second_changed
#undef second_delta
#undef set_part
#undef set_slot
#undef shifted_pair
#undef slot
#undef slot_of
#undef span
#undef span_of
#undef sum
#undef sum_of_parts
#undef total
#undef trigger
#undef tuple_positions
#undef value
#undef values
#undef whole
#undef whole_or_part

namespace hgl::demo::lists::m_
{

namespace n_::readings
{
// readings(count: i64) -> list<i64> (node), module demo.lists, line 10
struct impl_
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
}  // namespace n_::readings
namespace w_
{
// readings(count: i64) -> list<i64> (node), module demo.lists, line 10
::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>>> readings(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> count)
{
    return ::hgraph::wire<::hgl::demo::lists::m_::n_::readings::impl_>(wiring_, count);
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

namespace n_::total
{
// total(count: i64) -> i64 (graph), module demo.lists, line 31
struct impl_
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
}  // namespace n_::total
namespace w_
{
// total(count: i64) -> i64 (graph), module demo.lists, line 31
::hgraph::Port<::hgraph::TS<::hgraph::Int>> total(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> count)
{
    return ::hgl::demo::lists::m_::n_::total::impl_::compose(wiring_, count);
}
}  // namespace w_

namespace n_::changes
{
// changes(values: list<i64>) -> list<i64> (node), module demo.lists, line 34
struct impl_
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
}  // namespace n_::changes
namespace w_
{
// changes(values: list<i64>) -> list<i64> (node), module demo.lists, line 34
::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>>> changes(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>>> values)
{
    return ::hgraph::wire<::hgl::demo::lists::m_::n_::changes::impl_>(wiring_, values);
}
}  // namespace w_

namespace n_::changes_of
{
// changes_of(count: i64) -> list<i64> (graph), module demo.lists, line 38
struct impl_
{
    static ::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> count)
    {
        return ::hgl::demo::lists::m_::w_::changes(wiring_, ::hgl::demo::lists::m_::w_::readings(wiring_, count));
    }
};
}  // namespace n_::changes_of
namespace w_
{
// changes_of(count: i64) -> list<i64> (graph), module demo.lists, line 38
::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>>> changes_of(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> count)
{
    return ::hgl::demo::lists::m_::n_::changes_of::impl_::compose(wiring_, count);
}
}  // namespace w_

namespace n_::set_slot
{
// set_slot(slot: i64, reading: i64) -> list<i64, 2> (node), module demo.lists, line 41
struct impl_
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
}  // namespace n_::set_slot
namespace w_
{
// set_slot(slot: i64, reading: i64) -> list<i64, 2> (node), module demo.lists, line 41
::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>, 2>> set_slot(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> slot, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading)
{
    return ::hgraph::wire<::hgl::demo::lists::m_::n_::set_slot::impl_>(wiring_, slot, reading);
}
}  // namespace w_

namespace n_::probe_pair
{
// probe_pair(probe: bool, pair: list<i64, 2>, slot: i64) -> list<bool, 3> (node), module demo.lists, line 46
struct impl_
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
}  // namespace n_::probe_pair
namespace w_
{
// probe_pair(probe: bool, pair: list<i64, 2>, slot: i64) -> list<bool, 3> (node), module demo.lists, line 46
::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Bool>, 3>> probe_pair(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>, 2>> pair, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> slot)
{
    return ::hgraph::wire<::hgl::demo::lists::m_::n_::probe_pair::impl_>(wiring_, probe, pair, slot);
}
}  // namespace w_

namespace n_::observe
{
// observe(probe: bool, slot: i64, reading: i64, at: i64) -> list<bool, 3> (graph), module demo.lists, line 52
struct impl_
{
    static ::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Bool>, 3>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> slot, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> at)
    {
        return ::hgl::demo::lists::m_::w_::probe_pair(wiring_, probe, ::hgl::demo::lists::m_::w_::set_slot(wiring_, slot, reading), at);
    }
};
}  // namespace n_::observe
namespace w_
{
// observe(probe: bool, slot: i64, reading: i64, at: i64) -> list<bool, 3> (graph), module demo.lists, line 52
::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Bool>, 3>> observe(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> slot, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> at)
{
    return ::hgl::demo::lists::m_::n_::observe::impl_::compose(wiring_, probe, slot, reading, at);
}
}  // namespace w_

namespace n_::read_slot
{
// read_slot(pair: list<i64, 2>, slot: i64) -> i64 (node), module demo.lists, line 56
struct impl_
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
}  // namespace n_::read_slot
namespace w_
{
// read_slot(pair: list<i64, 2>, slot: i64) -> i64 (node), module demo.lists, line 56
::hgraph::Port<::hgraph::TS<::hgraph::Int>> read_slot(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>, 2>> pair, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> slot)
{
    return ::hgraph::wire<::hgl::demo::lists::m_::n_::read_slot::impl_>(wiring_, pair, slot);
}
}  // namespace w_

namespace n_::slot_of
{
// slot_of(slot: i64, reading: i64, at: i64) -> i64 (graph), module demo.lists, line 60
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> slot, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> at)
    {
        return ::hgl::demo::lists::m_::w_::read_slot(wiring_, ::hgl::demo::lists::m_::w_::set_slot(wiring_, slot, reading), at);
    }
};
}  // namespace n_::slot_of
namespace w_
{
// slot_of(slot: i64, reading: i64, at: i64) -> i64 (graph), module demo.lists, line 60
::hgraph::Port<::hgraph::TS<::hgraph::Int>> slot_of(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> slot, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> at)
{
    return ::hgl::demo::lists::m_::n_::slot_of::impl_::compose(wiring_, slot, reading, at);
}
}  // namespace w_

// span(low: i64, high: i64) -> i64 (`const fn`), module demo.lists, line 62
::hgraph::Int span([[maybe_unused]] ::hgraph::Int low, [[maybe_unused]] ::hgraph::Int high)
{
    return ::hglrt::sub__i64_i64(high, low);
}

namespace n_::span_of
{
// span_of(slot: i64, reading: i64) -> i64 (graph), module demo.lists, line 65
struct impl_
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
}  // namespace n_::span_of
namespace w_
{
// span_of(slot: i64, reading: i64) -> i64 (graph), module demo.lists, line 65
::hgraph::Port<::hgraph::TS<::hgraph::Int>> span_of(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> slot, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading)
{
    return ::hgl::demo::lists::m_::n_::span_of::impl_::compose(wiring_, slot, reading);
}
}  // namespace w_

namespace n_::set_part
{
// set_part(first: bool, value: i64) -> tuple<i64, i64> (node), module demo.lists, line 71
struct impl_
{
    static constexpr auto name = "demo.lists::set_part";

    static void eval([[maybe_unused]] ::hgraph::In<"first", ::hgraph::TS<::hgraph::Bool>> first, [[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::Out<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Int>>>> out_)
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
}  // namespace n_::set_part
namespace w_
{
// set_part(first: bool, value: i64) -> tuple<i64, i64> (node), module demo.lists, line 71
::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Int>>>> set_part(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> first, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgraph::wire<::hgl::demo::lists::m_::n_::set_part::impl_>(wiring_, first, value);
}
}  // namespace w_

namespace n_::part_validity
{
// part_validity(probe: bool, parts: tuple<i64, i64>) -> list<bool, 2> (node), module demo.lists, line 80
struct impl_
{
    static constexpr auto name = "demo.lists::part_validity";

    static void eval([[maybe_unused]] ::hgraph::In<"probe", ::hgraph::TS<::hgraph::Bool>, ::hgraph::InputValidity::Unchecked> probe, [[maybe_unused]] ::hgraph::In<"parts", ::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Int>>>, ::hgraph::InputActivity::Passive, ::hgraph::InputValidity::Unchecked> parts, [[maybe_unused]] ::hgraph::Out<::hgraph::TSL<::hgraph::TS<::hgraph::Bool>, 2>> out_)
    {
        // when, line 81 (TIME-3)
        if (::hglrt::fn_modified(probe))
        {
            ::hglrt::publish(out_, std::array<::hgraph::Bool, 2>{::hglrt::fn_valid(parts), ::hglrt::fn_all_valid(parts)});
            return;
        }
    }
};
}  // namespace n_::part_validity
namespace w_
{
// part_validity(probe: bool, parts: tuple<i64, i64>) -> list<bool, 2> (node), module demo.lists, line 80
::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Bool>, 2>> part_validity(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Int>>>> parts)
{
    return ::hgraph::wire<::hgl::demo::lists::m_::n_::part_validity::impl_>(wiring_, probe, parts);
}
}  // namespace w_

namespace n_::observe_parts
{
// observe_parts(probe: bool, first: bool, value: i64) -> list<bool, 2> (graph), module demo.lists, line 84
struct impl_
{
    static ::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Bool>, 2>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> first, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
    {
        return ::hgl::demo::lists::m_::w_::part_validity(wiring_, probe, ::hgl::demo::lists::m_::w_::set_part(wiring_, first, value));
    }
};
}  // namespace n_::observe_parts
namespace w_
{
// observe_parts(probe: bool, first: bool, value: i64) -> list<bool, 2> (graph), module demo.lists, line 84
::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Bool>, 2>> observe_parts(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> first, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgl::demo::lists::m_::n_::observe_parts::impl_::compose(wiring_, probe, first, value);
}
}  // namespace w_

namespace n_::first_of
{
// first_of(pair: list<i64, 2>) -> i64 (node), module demo.lists, line 88
struct impl_
{
    static constexpr auto name = "demo.lists::first_of";

    static void eval([[maybe_unused]] ::hgraph::In<"pair", ::hgraph::TSL<::hgraph::TS<::hgraph::Int>, 2>> pair, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 89 (TIME-3)
        if (::hglrt::fn_valid(::hglrt::child(pair, 0)))
        {
            out_.set(::hglrt::checked_whole(::hglrt::child(pair, 0)));
            return;
        }
    }
};
}  // namespace n_::first_of
namespace w_
{
// first_of(pair: list<i64, 2>) -> i64 (node), module demo.lists, line 88
::hgraph::Port<::hgraph::TS<::hgraph::Int>> first_of(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>, 2>> pair)
{
    return ::hgraph::wire<::hgl::demo::lists::m_::n_::first_of::impl_>(wiring_, pair);
}
}  // namespace w_

namespace n_::first_slot
{
// first_slot(slot: i64, reading: i64) -> i64 (graph), module demo.lists, line 92
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> slot, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading)
    {
        return ::hgl::demo::lists::m_::w_::first_of(wiring_, ::hgl::demo::lists::m_::w_::set_slot(wiring_, slot, reading));
    }
};
}  // namespace n_::first_slot
namespace w_
{
// first_slot(slot: i64, reading: i64) -> i64 (graph), module demo.lists, line 92
::hgraph::Port<::hgraph::TS<::hgraph::Int>> first_slot(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> slot, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading)
{
    return ::hgl::demo::lists::m_::n_::first_slot::impl_::compose(wiring_, slot, reading);
}
}  // namespace w_

namespace n_::second_delta
{
// second_delta(pair: list<i64, 2>) -> bool (node), module demo.lists, line 95
struct impl_
{
    static constexpr auto name = "demo.lists::second_delta";

    static void eval([[maybe_unused]] ::hgraph::In<"pair", ::hgraph::TSL<::hgraph::TS<::hgraph::Int>, 2>> pair, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Bool>> out_)
    {
        // when, line 96 (TIME-3)
        if (::hglrt::fn_valid(::hglrt::child(pair, 1)))
        {
            [[maybe_unused]] const ::hglrt::result<::hgraph::Int> change = ::hglrt::fn_delta_value_checked(::hglrt::child(pair, 1));
            out_.set(((change).has_value() && (::hglrt::fn_last_modified(::hglrt::child(pair, 1))).has_value()));
            return;
        }
    }
};
}  // namespace n_::second_delta
namespace w_
{
// second_delta(pair: list<i64, 2>) -> bool (node), module demo.lists, line 95
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> second_delta(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>, 2>> pair)
{
    return ::hgraph::wire<::hgl::demo::lists::m_::n_::second_delta::impl_>(wiring_, pair);
}
}  // namespace w_

namespace n_::second_changed
{
// second_changed(slot: i64, reading: i64) -> bool (graph), module demo.lists, line 102
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> slot, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading)
    {
        return ::hgl::demo::lists::m_::w_::second_delta(wiring_, ::hgl::demo::lists::m_::w_::set_slot(wiring_, slot, reading));
    }
};
}  // namespace n_::second_changed
namespace w_
{
// second_changed(slot: i64, reading: i64) -> bool (graph), module demo.lists, line 102
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> second_changed(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> slot, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading)
{
    return ::hgl::demo::lists::m_::n_::second_changed::impl_::compose(wiring_, slot, reading);
}
}  // namespace w_

namespace n_::whole_or_part
{
// whole_or_part(head: bool, value: i64) -> tuple<i64, i64> (node), module demo.lists, line 106
struct impl_
{
    static constexpr auto name = "demo.lists::whole_or_part";

    static void eval([[maybe_unused]] ::hgraph::In<"head", ::hgraph::TS<::hgraph::Bool>> head, [[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::Out<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Int>>>> out_)
    {
        // when, line 107 (TIME-3)
        {
            if (::hglrt::checked_value(head))
            {
                ::hglrt::publish(out_, std::tuple<::hgraph::Int, ::hgraph::Int>{::hglrt::checked_value(value), 0});
                return;
            }
            [[maybe_unused]] const std::tuple<std::optional<::hgraph::Int>, std::optional<::hgraph::Int>> only_second = std::tuple<std::optional<::hgraph::Int>, std::optional<::hgraph::Int>>{std::nullopt, ::hglrt::checked_value(value)};
            ::hglrt::publish(out_, only_second);
            return;
        }
    }
};
}  // namespace n_::whole_or_part
namespace w_
{
// whole_or_part(head: bool, value: i64) -> tuple<i64, i64> (node), module demo.lists, line 106
::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Int>>>> whole_or_part(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> head, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgraph::wire<::hgl::demo::lists::m_::n_::whole_or_part::impl_>(wiring_, head, value);
}
}  // namespace w_

namespace n_::part_sum
{
// part_sum(parts: tuple<i64, i64>) -> i64 (node), module demo.lists, line 114
struct impl_
{
    static constexpr auto name = "demo.lists::part_sum";

    static void eval([[maybe_unused]] ::hgraph::In<"parts", ::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Int>>>> parts, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 115 (TIME-3)
        if (::hglrt::fn_all_valid(parts))
        {
            [[maybe_unused]] const std::tuple<::hgraph::Int, ::hgraph::Int>& whole = ::hglrt::whole_value(parts);
            out_.set([&] { const ::hgraph::Int& argument_00_ = ::hglrt::checked_value(::hglrt::child<1>(parts)); const ::hgraph::Int& argument_01_ = std::get<0>(whole); return ::hglrt::add__i64_i64(argument_00_, argument_01_); }());
            return;
        }
    }
};
}  // namespace n_::part_sum
namespace w_
{
// part_sum(parts: tuple<i64, i64>) -> i64 (node), module demo.lists, line 114
::hgraph::Port<::hgraph::TS<::hgraph::Int>> part_sum(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Int>>>> parts)
{
    return ::hgraph::wire<::hgl::demo::lists::m_::n_::part_sum::impl_>(wiring_, parts);
}
}  // namespace w_

namespace n_::sum_of_parts
{
// sum_of_parts(head: bool, value: i64) -> i64 (graph), module demo.lists, line 121
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> head, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
    {
        return ::hgl::demo::lists::m_::w_::part_sum(wiring_, ::hgl::demo::lists::m_::w_::whole_or_part(wiring_, head, value));
    }
};
}  // namespace n_::sum_of_parts
namespace w_
{
// sum_of_parts(head: bool, value: i64) -> i64 (graph), module demo.lists, line 121
::hgraph::Port<::hgraph::TS<::hgraph::Int>> sum_of_parts(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> head, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgl::demo::lists::m_::n_::sum_of_parts::impl_::compose(wiring_, head, value);
}
}  // namespace w_

namespace n_::nested
{
// nested(count: i64) -> list<list<i64>> (node), module demo.lists, line 124
struct impl_
{
    static constexpr auto name = "demo.lists::nested";

    static void eval([[maybe_unused]] ::hgraph::In<"count", ::hgraph::TS<::hgraph::Int>> count, [[maybe_unused]] ::hgraph::Out<::hgraph::TSL<::hgraph::TSL<::hgraph::TS<::hgraph::Int>>>> out_)
    {
        // when, line 125 (TIME-3)
        {
            if (::hglrt::eq__i64_i64_f64_f64(::hglrt::checked_value(count), 0, 0.0, 0.0))
            {
                ::hglrt::publish(out_, ::hglrt::list<::hglrt::list<::hgraph::Int>>{::hglrt::list<::hgraph::Int>{1}, ::hglrt::list<::hgraph::Int>{}});
                return;
            }
            ::hglrt::publish(out_, ::hglrt::list_delta_of<::hglrt::list<::hglrt::list<::hgraph::Int>>>(::hglrt::dict_of<::hgraph::Int, ::hglrt::list_delta<::hglrt::list<::hgraph::Int>>>({{1, ::hglrt::list_delta_of<::hglrt::list<::hgraph::Int>>(::hglrt::dict_of<::hgraph::Int, ::hgraph::Int>({{0, ::hglrt::checked_value(count)}}), ::hglrt::set<::hgraph::Int>{}, std::nullopt)}}), ::hglrt::set<::hgraph::Int>{}, std::nullopt));
            return;
        }
    }
};
}  // namespace n_::nested
namespace w_
{
// nested(count: i64) -> list<list<i64>> (node), module demo.lists, line 124
::hgraph::Port<::hgraph::TSL<::hgraph::TSL<::hgraph::TS<::hgraph::Int>>>> nested(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> count)
{
    return ::hgraph::wire<::hgl::demo::lists::m_::n_::nested::impl_>(wiring_, count);
}
}  // namespace w_

namespace n_::nested_valid
{
// nested_valid(values: list<list<i64>>) -> list<bool, 3> (node), module demo.lists, line 131
struct impl_
{
    static constexpr auto name = "demo.lists::nested_valid";

    static void eval([[maybe_unused]] ::hgraph::In<"values", ::hgraph::TSL<::hgraph::TSL<::hgraph::TS<::hgraph::Int>>>> values, [[maybe_unused]] ::hgraph::Out<::hgraph::TSL<::hgraph::TS<::hgraph::Bool>, 3>> out_)
    {
        // when, line 132 (TIME-3)
        {
            ::hglrt::publish(out_, std::array<::hgraph::Bool, 3>{::hglrt::fn_valid(::hglrt::child(values, 1)), ::hglrt::fn_all_valid(values), ::hglrt::fn_valid(::hglrt::child(::hglrt::child(values, 1), 0))});
            return;
        }
    }
};
}  // namespace n_::nested_valid
namespace w_
{
// nested_valid(values: list<list<i64>>) -> list<bool, 3> (node), module demo.lists, line 131
::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Bool>, 3>> nested_valid(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSL<::hgraph::TSL<::hgraph::TS<::hgraph::Int>>>> values)
{
    return ::hgraph::wire<::hgl::demo::lists::m_::n_::nested_valid::impl_>(wiring_, values);
}
}  // namespace w_

namespace n_::nested_of
{
// nested_of(count: i64) -> list<bool, 3> (graph), module demo.lists, line 135
struct impl_
{
    static ::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Bool>, 3>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> count)
    {
        return ::hgl::demo::lists::m_::w_::nested_valid(wiring_, ::hgl::demo::lists::m_::w_::nested(wiring_, count));
    }
};
}  // namespace n_::nested_of
namespace w_
{
// nested_of(count: i64) -> list<bool, 3> (graph), module demo.lists, line 135
::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Bool>, 3>> nested_of(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> count)
{
    return ::hgl::demo::lists::m_::n_::nested_of::impl_::compose(wiring_, count);
}
}  // namespace w_

namespace n_::ordered_child
{
// ordered_child(values: list<list<i64>>) -> bool (node), module demo.lists, line 139
struct impl_
{
    static constexpr auto name = "demo.lists::ordered_child";

    static void eval([[maybe_unused]] ::hgraph::In<"values", ::hgraph::TSL<::hgraph::TSL<::hgraph::TS<::hgraph::Int>>>> values, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Bool>> out_)
    {
        // when, line 140 (TIME-3)
        {
            [[maybe_unused]] ::hgraph::Int at = 0;
            out_.set(::hglrt::fn_valid([&]() { const auto parent_00_ = ::hglrt::child(values, at); const ::hgraph::Int index_01_ = [&]() -> ::hgraph::Int {
    at = 1;
    return 0;
}(); return ::hglrt::child(parent_00_, index_01_); }()));
            return;
        }
    }
};
}  // namespace n_::ordered_child
namespace w_
{
// ordered_child(values: list<list<i64>>) -> bool (node), module demo.lists, line 139
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> ordered_child(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSL<::hgraph::TSL<::hgraph::TS<::hgraph::Int>>>> values)
{
    return ::hgraph::wire<::hgl::demo::lists::m_::n_::ordered_child::impl_>(wiring_, values);
}
}  // namespace w_

namespace n_::ordered_of
{
// ordered_of(count: i64) -> bool (graph), module demo.lists, line 149
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> count)
    {
        return ::hgl::demo::lists::m_::w_::ordered_child(wiring_, ::hgl::demo::lists::m_::w_::nested(wiring_, count));
    }
};
}  // namespace n_::ordered_of
namespace w_
{
// ordered_of(count: i64) -> bool (graph), module demo.lists, line 149
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> ordered_of(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> count)
{
    return ::hgl::demo::lists::m_::n_::ordered_of::impl_::compose(wiring_, count);
}
}  // namespace w_

namespace n_::ordered_row
{
// ordered_row(values: list<list<i64>>) -> bool (node), module demo.lists, line 152
struct impl_
{
    static constexpr auto name = "demo.lists::ordered_row";

    static void eval([[maybe_unused]] ::hgraph::In<"values", ::hgraph::TSL<::hgraph::TSL<::hgraph::TS<::hgraph::Int>>>> values, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Bool>> out_)
    {
        // when, line 153 (TIME-3)
        {
            [[maybe_unused]] ::hgraph::Int at = 0;
            out_.set(::hglrt::fn_valid([&]() { const auto parent_02_ = [&]() { const auto& parent_00_ = values; const ::hgraph::Int index_01_ = [&]() -> ::hgraph::Int {
    at = 1;
    return 0;
}(); return ::hglrt::child(parent_00_, index_01_); }(); const ::hgraph::Int index_03_ = at; return ::hglrt::child(parent_02_, index_03_); }()));
            return;
        }
    }
};
}  // namespace n_::ordered_row
namespace w_
{
// ordered_row(values: list<list<i64>>) -> bool (node), module demo.lists, line 152
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> ordered_row(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSL<::hgraph::TSL<::hgraph::TS<::hgraph::Int>>>> values)
{
    return ::hgraph::wire<::hgl::demo::lists::m_::n_::ordered_row::impl_>(wiring_, values);
}
}  // namespace w_

namespace n_::ordered_row_of
{
// ordered_row_of(count: i64) -> bool (graph), module demo.lists, line 162
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> count)
    {
        return ::hgl::demo::lists::m_::w_::ordered_row(wiring_, ::hgl::demo::lists::m_::w_::nested(wiring_, count));
    }
};
}  // namespace n_::ordered_row_of
namespace w_
{
// ordered_row_of(count: i64) -> bool (graph), module demo.lists, line 162
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> ordered_row_of(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> count)
{
    return ::hgl::demo::lists::m_::n_::ordered_row_of::impl_::compose(wiring_, count);
}
}  // namespace w_

namespace n_::shifted_pair
{
// shifted_pair(trigger: i64, const first: i64) -> list<i64, 2> (node), module demo.lists, line 166
struct impl_
{
    static constexpr auto name = "demo.lists::shifted_pair";

    static void eval([[maybe_unused]] ::hgraph::In<"trigger", ::hgraph::TS<::hgraph::Int>> trigger, [[maybe_unused]] ::hgraph::Scalar<"first", ::hgraph::Int> first, [[maybe_unused]] ::hgraph::Out<::hgraph::TSL<::hgraph::TS<::hgraph::Int>, 2>> out_)
    {
        // when, line 167 (TIME-3)
        {
            ::hglrt::publish(out_, std::array<::hgraph::Int, 2>{first.value(), ::hglrt::add__i64_i64(first.value(), 10)});
            return;
        }
    }
};
}  // namespace n_::shifted_pair
namespace w_
{
// shifted_pair(trigger: i64, const first: i64) -> list<i64, 2> (node), module demo.lists, line 166
::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>, 2>> shifted_pair(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> trigger, ::hgraph::Int first)
{
    return ::hgraph::wire<::hgl::demo::lists::m_::n_::shifted_pair::impl_>(wiring_, trigger, first);
}
}  // namespace w_

namespace n_::ordered_port
{
// ordered_port(trigger: i64) -> i64 (graph), module demo.lists, line 170
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> trigger)
    {
        [[maybe_unused]] ::hgraph::Int at = 0;
        return [&]() { const auto list_00_ = ::hgl::demo::lists::m_::w_::shifted_pair(wiring_, trigger, at); const ::hgraph::Int index_01_ = [&]() -> ::hgraph::Int {
    at = 1;
    return 0;
}(); return ::hglrt::element_port(list_00_, index_01_); }();
    }
};
}  // namespace n_::ordered_port
namespace w_
{
// ordered_port(trigger: i64) -> i64 (graph), module demo.lists, line 170
::hgraph::Port<::hgraph::TS<::hgraph::Int>> ordered_port(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> trigger)
{
    return ::hgl::demo::lists::m_::n_::ordered_port::impl_::compose(wiring_, trigger);
}
}  // namespace w_

namespace n_::ordered_connection
{
// ordered_connection(trigger: i64) -> i64 (graph), module demo.lists, line 180
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> trigger)
    {
        [[maybe_unused]] ::hgraph::Int at = 0;
        return [&]() { const auto list_02_ = [&] { const auto argument_00_ = trigger; const ::hgraph::Int argument_01_ = [&]() -> ::hgraph::Int {
    at = 1;
    return at;
}(); return ::hgl::demo::lists::m_::w_::shifted_pair(wiring_, argument_00_, argument_01_); }(); const ::hgraph::Int index_03_ = at; return ::hglrt::element_port(list_02_, index_03_); }();
    }
};
}  // namespace n_::ordered_connection
namespace w_
{
// ordered_connection(trigger: i64) -> i64 (graph), module demo.lists, line 180
::hgraph::Port<::hgraph::TS<::hgraph::Int>> ordered_connection(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> trigger)
{
    return ::hgl::demo::lists::m_::n_::ordered_connection::impl_::compose(wiring_, trigger);
}
}  // namespace w_

namespace n_::relay
{
// relay(values: list<i64>) -> list<i64> (node), module demo.lists, line 189
struct impl_
{
    static constexpr auto name = "demo.lists::relay";

    static void eval([[maybe_unused]] ::hgraph::In<"values", ::hgraph::TSL<::hgraph::TS<::hgraph::Int>>> values, [[maybe_unused]] ::hgraph::Out<::hgraph::TSL<::hgraph::TS<::hgraph::Int>>> out_)
    {
        // when, line 190 (TIME-3)
        {
            ::hglrt::publish(out_, ::hglrt::checked_whole(values));
            return;
        }
    }
};
}  // namespace n_::relay
namespace w_
{
// relay(values: list<i64>) -> list<i64> (node), module demo.lists, line 189
::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>>> relay(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>>> values)
{
    return ::hgraph::wire<::hgl::demo::lists::m_::n_::relay::impl_>(wiring_, values);
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
        const auto left_06_ = ::hglrt::testing::evaluate<::hglrt::list<::hgraph::Int>>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{2, 1}, start_));
        struct held_04_
        {
            static void eval(::hgraph::In<"result", ::hgraph::TSL<::hgraph::TS<::hgraph::Int>>> result_, ::hgraph::DateTime now_)
            {
                if (!::hglrt::fn_modified(result_))
                {
                    return;
                }
                std::optional<::hglrt::list<::hgraph::Int>> whole_;
                try
                {
                    whole_.emplace(::hglrt::checked_whole(result_));
                }
                catch (const ::hglrt::Failure&)
                {
                }
                ::hglrt::testing::hold<::hglrt::list<::hgraph::Int>>(now_, std::move(whole_));
            }
        };
        const auto result_05_ = ::hgl::demo::lists::m_::w_::changes_of(wiring_, input_03_);
        ::hgraph::wire<held_04_>(wiring_, result_05_);
        ::hgl::core::m_::w_::i_::record::of_1list_1i64_1_(wiring_, result_05_, ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_07_ = ::hglrt::testing::Dense<::hglrt::list<::hgraph::Int>>{::hglrt::testing::Cell<::hglrt::list<::hgraph::Int>>{std::in_place_index<0>, ::hglrt::list_delta_of<::hglrt::list<::hgraph::Int>>(::hglrt::dict_of<::hgraph::Int, ::hgraph::Int>({{0, 1}, {1, 2}}), ::hglrt::set<::hgraph::Int>{}, 2)}, ::hglrt::testing::Cell<::hglrt::list<::hgraph::Int>>{std::in_place_index<0>, ::hglrt::list_delta_of<::hglrt::list<::hgraph::Int>>(::hglrt::dict_of<::hgraph::Int, ::hgraph::Int>({{0, 1}}), ::hglrt::set<::hgraph::Int>{1}, 1)}};
        ::hglrt::testing::expect_same(left_06_, right_07_);
    }();
}

void fixed_list_children()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_014_ = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Bool>{true, true, true};
        const auto trace_02_ = ::hglrt::testing::Dense<::hgraph::Int>{std::nullopt, 0, 1};
        const auto trace_04_ = ::hglrt::testing::Dense<::hgraph::Int>{std::nullopt, 3, 8};
        const auto trace_06_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 1, 1};
        return ::hglrt::testing::evaluate<std::array<::hgraph::Bool, 3>>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_02_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_05_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_04_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_07_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_06_, start_));
        struct held_012_
        {
            static void eval(::hgraph::In<"result", ::hgraph::TSL<::hgraph::TS<::hgraph::Bool>, 3>> result_, ::hgraph::DateTime now_)
            {
                if (!::hglrt::fn_modified(result_))
                {
                    return;
                }
                std::optional<std::array<::hgraph::Bool, 3>> whole_;
                try
                {
                    whole_.emplace(::hglrt::checked_whole(result_));
                }
                catch (const ::hglrt::Failure&)
                {
                }
                ::hglrt::testing::hold<std::array<::hgraph::Bool, 3>>(now_, std::move(whole_));
            }
        };
        const auto result_013_ = [&] { const auto argument_08_ = input_01_; const auto argument_09_ = input_03_; const auto argument_010_ = input_05_; const auto argument_011_ = input_07_; return ::hgl::demo::lists::m_::w_::observe(wiring_, argument_08_, argument_09_, argument_010_, argument_011_); }();
        ::hgraph::wire<held_012_>(wiring_, result_013_);
        ::hgl::core::m_::w_::i_::record::of_1list_1bool_1_13_1_(wiring_, result_013_, ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_015_ = ::hglrt::testing::Dense<std::array<::hgraph::Bool, 3>>{::hglrt::testing::Cell<std::array<::hgraph::Bool, 3>>{::hglrt::testing::Whole<std::array<::hgraph::Bool, 3>>{std::array<::hgraph::Bool, 3>{false, false, false}}}, ::hglrt::testing::Cell<std::array<::hgraph::Bool, 3>>{::hglrt::testing::Whole<std::array<::hgraph::Bool, 3>>{std::array<::hgraph::Bool, 3>{true, false, false}}}, ::hglrt::testing::Cell<std::array<::hgraph::Bool, 3>>{::hglrt::testing::Whole<std::array<::hgraph::Bool, 3>>{std::array<::hgraph::Bool, 3>{true, true, true}}}};
        ::hglrt::testing::expect_same(left_014_, right_015_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_022_ = [&]() {
        const auto trace_016_ = ::hglrt::testing::Dense<::hgraph::Int>{0, 1, std::nullopt, 0};
        const auto trace_018_ = ::hglrt::testing::Dense<::hgraph::Int>{3, 8, std::nullopt, 4};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_017_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_016_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_019_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_018_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_020_ = input_017_; const auto argument_021_ = input_019_; return ::hgl::demo::lists::m_::w_::span_of(wiring_, argument_020_, argument_021_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_023_ = ::hglrt::testing::Dense<::hgraph::Int>{std::nullopt, 5, std::nullopt, 4};
        ::hglrt::testing::expect_same(left_022_, right_023_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_033_ = [&]() {
        const auto trace_024_ = ::hglrt::testing::Dense<::hgraph::Int>{0};
        const auto trace_026_ = ::hglrt::testing::Dense<::hgraph::Int>{3};
        const auto trace_028_ = ::hglrt::testing::Dense<::hgraph::Int>{0};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_025_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_024_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_027_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_026_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_029_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_028_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_030_ = input_025_; const auto argument_031_ = input_027_; const auto argument_032_ = input_029_; return ::hgl::demo::lists::m_::w_::slot_of(wiring_, argument_030_, argument_031_, argument_032_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_034_ = ::hglrt::testing::Dense<::hgraph::Int>{3};
        ::hglrt::testing::expect_same(left_033_, right_034_);
    }();
    ::hglrt::testing::statement();
    ::hglrt::testing::raises("value.unset_read", [&] {
        static_cast<void>([&]() {
        const auto trace_035_ = ::hglrt::testing::Dense<::hgraph::Int>{0, std::nullopt};
        const auto trace_037_ = ::hglrt::testing::Dense<::hgraph::Int>{3, std::nullopt};
        const auto trace_039_ = ::hglrt::testing::Dense<::hgraph::Int>{0, 1};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_036_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_035_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_038_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_037_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_040_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_039_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_041_ = input_036_; const auto argument_042_ = input_038_; const auto argument_043_ = input_040_; return ::hgl::demo::lists::m_::w_::slot_of(wiring_, argument_041_, argument_042_, argument_043_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }());
    });
    ::hglrt::testing::statement();
    ::hglrt::testing::raises("value.index_out_of_range", [&] {
        static_cast<void>([&]() {
        const auto trace_044_ = ::hglrt::testing::Dense<::hgraph::Int>{0};
        const auto trace_046_ = ::hglrt::testing::Dense<::hgraph::Int>{3};
        const auto trace_048_ = ::hglrt::testing::Dense<::hgraph::Int>{2};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_045_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_044_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_047_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_046_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_049_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_048_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_050_ = input_045_; const auto argument_051_ = input_047_; const auto argument_052_ = input_049_; return ::hgl::demo::lists::m_::w_::slot_of(wiring_, argument_050_, argument_051_, argument_052_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }());
    });
}

void children_named_in_a_hook()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_06_ = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 0};
        const auto trace_02_ = ::hglrt::testing::Dense<::hgraph::Int>{5, 3};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_02_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_04_ = input_01_; const auto argument_05_ = input_03_; return ::hgl::demo::lists::m_::w_::first_slot(wiring_, argument_04_, argument_05_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_07_ = ::hglrt::testing::Dense<::hgraph::Int>{std::nullopt, 3};
        ::hglrt::testing::expect_same(left_06_, right_07_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_014_ = [&]() {
        const auto trace_08_ = ::hglrt::testing::Dense<::hgraph::Int>{0, 1, 0};
        const auto trace_010_ = ::hglrt::testing::Dense<::hgraph::Int>{3, 8, 4};
        return ::hglrt::testing::evaluate<::hgraph::Bool>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_09_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_08_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_011_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_010_, start_));
        ::hgl::core::m_::w_::i_::record::of_bool(wiring_, [&] { const auto argument_012_ = input_09_; const auto argument_013_ = input_011_; return ::hgl::demo::lists::m_::w_::second_changed(wiring_, argument_012_, argument_013_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_015_ = ::hglrt::testing::Dense<::hgraph::Bool>{std::nullopt, true, false};
        ::hglrt::testing::expect_same(left_014_, right_015_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_022_ = [&]() {
        const auto trace_016_ = ::hglrt::testing::Dense<::hgraph::Bool>{true, false};
        const auto trace_018_ = ::hglrt::testing::Dense<::hgraph::Int>{3, 8};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_017_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(trace_016_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_019_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_018_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_020_ = input_017_; const auto argument_021_ = input_019_; return ::hgl::demo::lists::m_::w_::sum_of_parts(wiring_, argument_020_, argument_021_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_023_ = ::hglrt::testing::Dense<::hgraph::Int>{3, 11};
        ::hglrt::testing::expect_same(left_022_, right_023_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_027_ = ::hglrt::testing::evaluate<std::array<::hgraph::Bool, 3>>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_024_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{0, 7}, start_));
        struct held_025_
        {
            static void eval(::hgraph::In<"result", ::hgraph::TSL<::hgraph::TS<::hgraph::Bool>, 3>> result_, ::hgraph::DateTime now_)
            {
                if (!::hglrt::fn_modified(result_))
                {
                    return;
                }
                std::optional<std::array<::hgraph::Bool, 3>> whole_;
                try
                {
                    whole_.emplace(::hglrt::checked_whole(result_));
                }
                catch (const ::hglrt::Failure&)
                {
                }
                ::hglrt::testing::hold<std::array<::hgraph::Bool, 3>>(now_, std::move(whole_));
            }
        };
        const auto result_026_ = ::hgl::demo::lists::m_::w_::nested_of(wiring_, input_024_);
        ::hgraph::wire<held_025_>(wiring_, result_026_);
        ::hgl::core::m_::w_::i_::record::of_1list_1bool_1_13_1_(wiring_, result_026_, ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_028_ = ::hglrt::testing::Dense<std::array<::hgraph::Bool, 3>>{::hglrt::testing::Cell<std::array<::hgraph::Bool, 3>>{::hglrt::testing::Whole<std::array<::hgraph::Bool, 3>>{std::array<::hgraph::Bool, 3>{true, true, false}}}, ::hglrt::testing::Cell<std::array<::hgraph::Bool, 3>>{::hglrt::testing::Whole<std::array<::hgraph::Bool, 3>>{std::array<::hgraph::Bool, 3>{true, true, true}}}};
        ::hglrt::testing::expect_same(left_027_, right_028_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_030_ = ::hglrt::testing::evaluate<::hgraph::Bool>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_029_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{0}, start_));
        ::hgl::core::m_::w_::i_::record::of_bool(wiring_, ::hgl::demo::lists::m_::w_::ordered_of(wiring_, input_029_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_031_ = ::hglrt::testing::Dense<::hgraph::Bool>{true};
        ::hglrt::testing::expect_same(left_030_, right_031_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_033_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_032_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{5}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::lists::m_::w_::ordered_port(wiring_, input_032_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_034_ = ::hglrt::testing::Dense<::hgraph::Int>{0};
        ::hglrt::testing::expect_same(left_033_, right_034_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_036_ = ::hglrt::testing::evaluate<::hgraph::Bool>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_035_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{0}, start_));
        ::hgl::core::m_::w_::i_::record::of_bool(wiring_, ::hgl::demo::lists::m_::w_::ordered_row_of(wiring_, input_035_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_037_ = ::hglrt::testing::Dense<::hgraph::Bool>{false};
        ::hglrt::testing::expect_same(left_036_, right_037_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_039_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_038_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{5}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::lists::m_::w_::ordered_connection(wiring_, input_038_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_040_ = ::hglrt::testing::Dense<::hgraph::Int>{11};
        ::hglrt::testing::expect_same(left_039_, right_040_);
    }();
}

void tuple_positions()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_011_ = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Bool>{true, true, true};
        const auto trace_02_ = ::hglrt::testing::Dense<::hgraph::Bool>{std::nullopt, true, false};
        const auto trace_04_ = ::hglrt::testing::Dense<::hgraph::Int>{std::nullopt, 3, 8};
        return ::hglrt::testing::evaluate<std::array<::hgraph::Bool, 2>>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(trace_02_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_05_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_04_, start_));
        struct held_09_
        {
            static void eval(::hgraph::In<"result", ::hgraph::TSL<::hgraph::TS<::hgraph::Bool>, 2>> result_, ::hgraph::DateTime now_)
            {
                if (!::hglrt::fn_modified(result_))
                {
                    return;
                }
                std::optional<std::array<::hgraph::Bool, 2>> whole_;
                try
                {
                    whole_.emplace(::hglrt::checked_whole(result_));
                }
                catch (const ::hglrt::Failure&)
                {
                }
                ::hglrt::testing::hold<std::array<::hgraph::Bool, 2>>(now_, std::move(whole_));
            }
        };
        const auto result_010_ = [&] { const auto argument_06_ = input_01_; const auto argument_07_ = input_03_; const auto argument_08_ = input_05_; return ::hgl::demo::lists::m_::w_::observe_parts(wiring_, argument_06_, argument_07_, argument_08_); }();
        ::hgraph::wire<held_09_>(wiring_, result_010_);
        ::hgl::core::m_::w_::i_::record::of_1list_1bool_1_12_1_(wiring_, result_010_, ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_012_ = ::hglrt::testing::Dense<std::array<::hgraph::Bool, 2>>{::hglrt::testing::Cell<std::array<::hgraph::Bool, 2>>{::hglrt::testing::Whole<std::array<::hgraph::Bool, 2>>{std::array<::hgraph::Bool, 2>{false, false}}}, ::hglrt::testing::Cell<std::array<::hgraph::Bool, 2>>{::hglrt::testing::Whole<std::array<::hgraph::Bool, 2>>{std::array<::hgraph::Bool, 2>{true, false}}}, ::hglrt::testing::Cell<std::array<::hgraph::Bool, 2>>{::hglrt::testing::Whole<std::array<::hgraph::Bool, 2>>{std::array<::hgraph::Bool, 2>{true, true}}}};
        ::hglrt::testing::expect_same(left_011_, right_012_);
    }();
}

void a_list_input_replays_its_cells()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_03_ = ::hglrt::testing::evaluate<::hglrt::list<::hgraph::Int>>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_1list_1i64_1_(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hglrt::list<::hgraph::Int>>{::hglrt::testing::Cell<::hglrt::list<::hgraph::Int>>{::hglrt::testing::Whole<::hglrt::list<::hgraph::Int>>{::hglrt::list<::hgraph::Int>{1, 2}}}, ::hglrt::testing::Cell<::hglrt::list<::hgraph::Int>>{std::in_place_index<0>, ::hglrt::list_delta_of<::hglrt::list<::hgraph::Int>>(::hglrt::dict_of<::hgraph::Int, ::hgraph::Int>({{2, 3}}), ::hglrt::set<::hgraph::Int>{}, std::nullopt)}, ::hglrt::testing::Cell<::hglrt::list<::hgraph::Int>>{::hglrt::testing::Whole<::hglrt::list<::hgraph::Int>>{::hglrt::list<::hgraph::Int>{5}}}}, start_));
        struct held_01_
        {
            static void eval(::hgraph::In<"result", ::hgraph::TSL<::hgraph::TS<::hgraph::Int>>> result_, ::hgraph::DateTime now_)
            {
                if (!::hglrt::fn_modified(result_))
                {
                    return;
                }
                std::optional<::hglrt::list<::hgraph::Int>> whole_;
                try
                {
                    whole_.emplace(::hglrt::checked_whole(result_));
                }
                catch (const ::hglrt::Failure&)
                {
                }
                ::hglrt::testing::hold<::hglrt::list<::hgraph::Int>>(now_, std::move(whole_));
            }
        };
        const auto result_02_ = ::hgl::demo::lists::m_::w_::relay(wiring_, input_00_);
        ::hgraph::wire<held_01_>(wiring_, result_02_);
        ::hgl::core::m_::w_::i_::record::of_1list_1i64_1_(wiring_, result_02_, ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_04_ = ::hglrt::testing::Dense<::hglrt::list<::hgraph::Int>>{::hglrt::testing::Cell<::hglrt::list<::hgraph::Int>>{::hglrt::testing::Whole<::hglrt::list<::hgraph::Int>>{::hglrt::list<::hgraph::Int>{1, 2}}}, ::hglrt::testing::Cell<::hglrt::list<::hgraph::Int>>{::hglrt::testing::Whole<::hglrt::list<::hgraph::Int>>{::hglrt::list<::hgraph::Int>{1, 2, 3}}}, ::hglrt::testing::Cell<::hglrt::list<::hgraph::Int>>{std::in_place_index<0>, ::hglrt::list_delta_of<::hglrt::list<::hgraph::Int>>(::hglrt::dict_of<::hgraph::Int, ::hgraph::Int>({{0, 5}}), ::hglrt::set<::hgraph::Int>{1, 2}, 1)}};
        ::hglrt::testing::expect_same(left_03_, right_04_);
    }();
}

}  // namespace hgl::demo::lists::m_::t_
