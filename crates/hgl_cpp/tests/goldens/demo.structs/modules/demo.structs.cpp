// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include "demo.structs.h"
#include "core.h"

// Names this file writes, which no macro of a header above takes.
#undef Inner
#undef MarketQuote
#undef NotedQuote
#undef Outer
#undef PlainQuote
#undef Wrapped
#undef a_narrowed_read
#undef a_nested_struct_delta_is_recorded
#undef a_nested_struct_field_is_written_and_read_by_its_parts
#undef a_published_null_has_a_delta
#undef a_struct_delta_keeps_the_final_write_to_each_field
#undef a_struct_delta_reads_the_field_that_changed
#undef a_struct_is_all_valid_once_every_field_is
#undef a_struct_is_valid_once_one_field_is
#undef a_struct_that_holds_one_with_an_optional_field_is_read_whole
#undef a_struct_with_an_optional_field_is_all_valid_and_read_whole
#undef a_tuple_is_valid_once_one_position_is_and_all_valid_once_both_are
#undef a_tuple_position_is_read_and_projected
#undef a_whole_struct_is_a_trace_cell
#undef a_whole_tuple_is_a_trace_cell
#undef action
#undef an_atomic_input_is_read_whole
#undef an_atomic_quote_wakes_its_bid_projection
#undef an_atomic_struct_is_a_trace_cell
#undef an_atomic_struct_is_replayed_whole
#undef ask
#undef atomic_bid
#undef atomic_bid_ticks
#undef bid
#undef bid_delta
#undef bid_ticks
#undef changing_a_sibling_field_does_not_publish_a_bid
#undef code
#undef command
#undef core
#undef count
#undef demo
#undef doubled
#undef earliest_start
#undef first
#undef first_of
#undef high
#undef inner
#undef input
#undef is_bid
#undef last_modified_follows_the_last_publication
#undef make_outer
#undef note
#undef note_is_null
#undef noted
#undef nullable_delta_payload
#undef observe_noted_quote
#undef observe_part_validity
#undef observe_quote_all_valid
#undef observe_quote_valid
#undef observe_validity
#undef only_first
#undef only_second
#undef outer
#undef outer_deltas
#undef outer_ticks
#undef pair
#undef pair_sum
#undef part_validity
#undef parts
#undef price
#undef probe
#undef publication_establishes_validity
#undef publish_nullable
#undef quote
#undef quote_all_valid
#undef quote_valid
#undef read_noted_quote
#undef read_outer
#undef read_validity
#undef reading
#undef record
#undef relay_pair
#undef relay_quote
#undef replay
#undef sample
#undef sample_timestamp
#undef scalar_timestamp
#undef set_one_part
#undef size
#undef spread
#undef spread_of
#undef structs
#undef t
#undef update_quote
#undef value
#undef watched
#undef whole
#undef whole_plain
#undef whole_quote
#undef wrap
#undef wrapped
#undef wrapped_of
#undef wrapped_sum
#undef write_bid
#undef write_quote_fields

namespace hgl::demo::structs::m_
{

namespace n_::update_quote
{
// update_quote(is_bid: bool, price: i64) -> MarketQuote (node), module demo.structs, line 14
struct impl_
{
    static constexpr auto name = "demo.structs::update_quote";

    static void eval([[maybe_unused]] ::hgraph::In<"is_bid", ::hgraph::TS<::hgraph::Bool>> is_bid, [[maybe_unused]] ::hgraph::In<"price", ::hgraph::TS<::hgraph::Int>> price, [[maybe_unused]] ::hgraph::Out<::hgraph::NativeTSB<::hgl::demo::structs::m_::MarketQuote>> out_)
    {
        // when, line 15 (TIME-3)
        {
            if (::hglrt::checked_value(is_bid))
            {
                ::hglrt::publish(out_, ::hgl::demo::structs::m_::d_::MarketQuote{::hglrt::checked_value(price), std::nullopt});
                return;
            }
            ::hglrt::publish(out_, ::hgl::demo::structs::m_::d_::MarketQuote{std::nullopt, ::hglrt::checked_value(price)});
            return;
        }
    }
};
}  // namespace n_::update_quote
namespace w_
{
// update_quote(is_bid: bool, price: i64) -> MarketQuote (node), module demo.structs, line 14
::hgraph::Port<::hgraph::NativeTSB<::hgl::demo::structs::m_::MarketQuote>> update_quote(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> is_bid, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> price)
{
    return ::hgraph::wire<::hgl::demo::structs::m_::n_::update_quote::impl_>(wiring_, is_bid, price);
}
}  // namespace w_

namespace n_::bid_ticks
{
// bid_ticks(is_bid: bool, price: i64) -> i64 (graph), module demo.structs, line 21
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> is_bid, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> price)
    {
        return ::hglrt::field_port<0>(::hgl::demo::structs::m_::w_::update_quote(wiring_, is_bid, price));
    }
};
}  // namespace n_::bid_ticks
namespace w_
{
// bid_ticks(is_bid: bool, price: i64) -> i64 (graph), module demo.structs, line 21
::hgraph::Port<::hgraph::TS<::hgraph::Int>> bid_ticks(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> is_bid, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> price)
{
    return ::hgl::demo::structs::m_::n_::bid_ticks::impl_::compose(wiring_, is_bid, price);
}
}  // namespace w_

namespace n_::whole_quote
{
// whole_quote(bid: i64, ask: i64) -> atomic<MarketQuote> (node), module demo.structs, line 30
struct impl_
{
    static constexpr auto name = "demo.structs::whole_quote";

    static void eval([[maybe_unused]] ::hgraph::In<"bid", ::hgraph::TS<::hgraph::Int>> bid, [[maybe_unused]] ::hgraph::In<"ask", ::hgraph::TS<::hgraph::Int>> ask, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgl::demo::structs::m_::MarketQuote>> out_)
    {
        // when, line 31 (TIME-3)
        {
            out_.set(::hgl::demo::structs::m_::MarketQuote{::hglrt::checked_value(bid), ::hglrt::checked_value(ask)});
            return;
        }
    }
};
}  // namespace n_::whole_quote
namespace w_
{
// whole_quote(bid: i64, ask: i64) -> atomic<MarketQuote> (node), module demo.structs, line 30
::hgraph::Port<::hgraph::TS<::hgl::demo::structs::m_::MarketQuote>> whole_quote(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> bid, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> ask)
{
    return ::hgraph::wire<::hgl::demo::structs::m_::n_::whole_quote::impl_>(wiring_, bid, ask);
}
}  // namespace w_

namespace n_::atomic_bid_ticks
{
// atomic_bid_ticks(bid: i64, ask: i64) -> i64 (graph), module demo.structs, line 34
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> bid, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> ask)
    {
        return ::hglrt::atomic_field<0, ::hgraph::TS<::hgraph::Int>>(wiring_, ::hgl::demo::structs::m_::w_::whole_quote(wiring_, bid, ask));
    }
};
}  // namespace n_::atomic_bid_ticks
namespace w_
{
// atomic_bid_ticks(bid: i64, ask: i64) -> i64 (graph), module demo.structs, line 34
::hgraph::Port<::hgraph::TS<::hgraph::Int>> atomic_bid_ticks(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> bid, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> ask)
{
    return ::hgl::demo::structs::m_::n_::atomic_bid_ticks::impl_::compose(wiring_, bid, ask);
}
}  // namespace w_

namespace n_::write_quote_fields
{
// write_quote_fields(bid: i64, ask: i64) -> MarketQuote (node), module demo.structs, line 43
struct impl_
{
    static constexpr auto name = "demo.structs::write_quote_fields";

    static void eval([[maybe_unused]] ::hgraph::In<"bid", ::hgraph::TS<::hgraph::Int>> bid, [[maybe_unused]] ::hgraph::In<"ask", ::hgraph::TS<::hgraph::Int>> ask, [[maybe_unused]] ::hgraph::Out<::hgraph::NativeTSB<::hgl::demo::structs::m_::MarketQuote>> out_)
    {
        // when, line 45 (TIME-3)
        {
            ::hglrt::publish(out_, ::hgl::demo::structs::m_::d_::MarketQuote{0, std::nullopt});
            ::hglrt::publish(out_, ::hgl::demo::structs::m_::d_::MarketQuote{std::nullopt, ::hglrt::checked_value(ask)});
            ::hglrt::publish(out_, ::hgl::demo::structs::m_::d_::MarketQuote{::hglrt::checked_value(bid), std::nullopt});
        }
    }
};
}  // namespace n_::write_quote_fields
namespace w_
{
// write_quote_fields(bid: i64, ask: i64) -> MarketQuote (node), module demo.structs, line 43
::hgraph::Port<::hgraph::NativeTSB<::hgl::demo::structs::m_::MarketQuote>> write_quote_fields(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> bid, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> ask)
{
    return ::hgraph::wire<::hgl::demo::structs::m_::n_::write_quote_fields::impl_>(wiring_, bid, ask);
}
}  // namespace w_

namespace n_::quote_valid
{
// quote_valid(probe: bool, quote: MarketQuote) -> bool (node), module demo.structs, line 63
struct impl_
{
    static constexpr auto name = "demo.structs::quote_valid";

    static void eval([[maybe_unused]] ::hgraph::In<"probe", ::hgraph::TS<::hgraph::Bool>, ::hgraph::InputValidity::Unchecked> probe, [[maybe_unused]] ::hgraph::In<"quote", ::hgraph::NativeTSB<::hgl::demo::structs::m_::MarketQuote>, ::hgraph::InputActivity::Passive, ::hgraph::InputValidity::Unchecked> quote, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Bool>> out_)
    {
        // when, line 64 (TIME-3)
        if (::hglrt::fn_modified(probe))
        {
            out_.set(::hglrt::fn_valid(quote));
            return;
        }
    }
};
}  // namespace n_::quote_valid
namespace w_
{
// quote_valid(probe: bool, quote: MarketQuote) -> bool (node), module demo.structs, line 63
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> quote_valid(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::NativeTSB<::hgl::demo::structs::m_::MarketQuote>> quote)
{
    return ::hgraph::wire<::hgl::demo::structs::m_::n_::quote_valid::impl_>(wiring_, probe, quote);
}
}  // namespace w_

namespace n_::quote_all_valid
{
// quote_all_valid(probe: bool, quote: MarketQuote) -> bool (node), module demo.structs, line 67
struct impl_
{
    static constexpr auto name = "demo.structs::quote_all_valid";

    static void eval([[maybe_unused]] ::hgraph::In<"probe", ::hgraph::TS<::hgraph::Bool>, ::hgraph::InputValidity::Unchecked> probe, [[maybe_unused]] ::hgraph::In<"quote", ::hgraph::NativeTSB<::hgl::demo::structs::m_::MarketQuote>, ::hgraph::InputActivity::Passive, ::hgraph::InputValidity::Unchecked> quote, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Bool>> out_)
    {
        // when, line 68 (TIME-3)
        if (::hglrt::fn_modified(probe))
        {
            out_.set(::hglrt::fn_all_valid(quote));
            return;
        }
    }
};
}  // namespace n_::quote_all_valid
namespace w_
{
// quote_all_valid(probe: bool, quote: MarketQuote) -> bool (node), module demo.structs, line 67
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> quote_all_valid(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::NativeTSB<::hgl::demo::structs::m_::MarketQuote>> quote)
{
    return ::hgraph::wire<::hgl::demo::structs::m_::n_::quote_all_valid::impl_>(wiring_, probe, quote);
}
}  // namespace w_

namespace n_::observe_quote_valid
{
// observe_quote_valid(probe: bool, is_bid: bool, price: i64) -> bool (graph), module demo.structs, line 71
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> is_bid, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> price)
    {
        return ::hgl::demo::structs::m_::w_::quote_valid(wiring_, probe, ::hgl::demo::structs::m_::w_::update_quote(wiring_, is_bid, price));
    }
};
}  // namespace n_::observe_quote_valid
namespace w_
{
// observe_quote_valid(probe: bool, is_bid: bool, price: i64) -> bool (graph), module demo.structs, line 71
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> observe_quote_valid(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> is_bid, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> price)
{
    return ::hgl::demo::structs::m_::n_::observe_quote_valid::impl_::compose(wiring_, probe, is_bid, price);
}
}  // namespace w_

namespace n_::observe_quote_all_valid
{
// observe_quote_all_valid(probe: bool, is_bid: bool, price: i64) -> bool (graph), module demo.structs, line 74
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> is_bid, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> price)
    {
        return ::hgl::demo::structs::m_::w_::quote_all_valid(wiring_, probe, ::hgl::demo::structs::m_::w_::update_quote(wiring_, is_bid, price));
    }
};
}  // namespace n_::observe_quote_all_valid
namespace w_
{
// observe_quote_all_valid(probe: bool, is_bid: bool, price: i64) -> bool (graph), module demo.structs, line 74
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> observe_quote_all_valid(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> is_bid, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> price)
{
    return ::hgl::demo::structs::m_::n_::observe_quote_all_valid::impl_::compose(wiring_, probe, is_bid, price);
}
}  // namespace w_

namespace n_::relay_quote
{
// relay_quote(quote: MarketQuote) -> MarketQuote (node), module demo.structs, line 93
struct impl_
{
    static constexpr auto name = "demo.structs::relay_quote";

    static void eval([[maybe_unused]] ::hgraph::In<"quote", ::hgraph::NativeTSB<::hgl::demo::structs::m_::MarketQuote>> quote, [[maybe_unused]] ::hgraph::Out<::hgraph::NativeTSB<::hgl::demo::structs::m_::MarketQuote>> out_)
    {
        // when, line 94 (TIME-3)
        if (::hglrt::fn_all_valid(quote))
        {
            ::hglrt::publish(out_, ::hglrt::whole_value(quote));
            return;
        }
    }
};
}  // namespace n_::relay_quote
namespace w_
{
// relay_quote(quote: MarketQuote) -> MarketQuote (node), module demo.structs, line 93
::hgraph::Port<::hgraph::NativeTSB<::hgl::demo::structs::m_::MarketQuote>> relay_quote(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::NativeTSB<::hgl::demo::structs::m_::MarketQuote>> quote)
{
    return ::hgraph::wire<::hgl::demo::structs::m_::n_::relay_quote::impl_>(wiring_, quote);
}
}  // namespace w_

namespace n_::set_one_part
{
// set_one_part(first: bool, value: i64) -> tuple<i64, i64> (node), module demo.structs, line 111
struct impl_
{
    static constexpr auto name = "demo.structs::set_one_part";

    static void eval([[maybe_unused]] ::hgraph::In<"first", ::hgraph::TS<::hgraph::Bool>> first, [[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::Out<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Int>>>> out_)
    {
        // when, line 112 (TIME-3)
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
}  // namespace n_::set_one_part
namespace w_
{
// set_one_part(first: bool, value: i64) -> tuple<i64, i64> (node), module demo.structs, line 111
::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Int>>>> set_one_part(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> first, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgraph::wire<::hgl::demo::structs::m_::n_::set_one_part::impl_>(wiring_, first, value);
}
}  // namespace w_

namespace n_::part_validity
{
// part_validity(probe: bool, parts: tuple<i64, i64>) -> tuple<bool, bool> (node), module demo.structs, line 120
struct impl_
{
    static constexpr auto name = "demo.structs::part_validity";

    static void eval([[maybe_unused]] ::hgraph::In<"probe", ::hgraph::TS<::hgraph::Bool>, ::hgraph::InputValidity::Unchecked> probe, [[maybe_unused]] ::hgraph::In<"parts", ::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Int>>>, ::hgraph::InputActivity::Passive, ::hgraph::InputValidity::Unchecked> parts, [[maybe_unused]] ::hgraph::Out<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Bool>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Bool>>>> out_)
    {
        // when, line 121 (TIME-3)
        if (::hglrt::fn_modified(probe))
        {
            ::hglrt::publish(out_, std::tuple<::hgraph::Bool, ::hgraph::Bool>{::hglrt::fn_valid(parts), ::hglrt::fn_all_valid(parts)});
            return;
        }
    }
};
}  // namespace n_::part_validity
namespace w_
{
// part_validity(probe: bool, parts: tuple<i64, i64>) -> tuple<bool, bool> (node), module demo.structs, line 120
::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Bool>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Bool>>>> part_validity(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Int>>>> parts)
{
    return ::hgraph::wire<::hgl::demo::structs::m_::n_::part_validity::impl_>(wiring_, probe, parts);
}
}  // namespace w_

namespace n_::observe_part_validity
{
// observe_part_validity(probe: bool, first: bool, value: i64) -> tuple<bool, bool> (graph), module demo.structs, line 124
struct impl_
{
    static ::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Bool>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Bool>>>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> first, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
    {
        return ::hgl::demo::structs::m_::w_::part_validity(wiring_, probe, ::hgl::demo::structs::m_::w_::set_one_part(wiring_, first, value));
    }
};
}  // namespace n_::observe_part_validity
namespace w_
{
// observe_part_validity(probe: bool, first: bool, value: i64) -> tuple<bool, bool> (graph), module demo.structs, line 124
::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Bool>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Bool>>>> observe_part_validity(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> first, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgl::demo::structs::m_::n_::observe_part_validity::impl_::compose(wiring_, probe, first, value);
}
}  // namespace w_

namespace n_::write_bid
{
// write_bid(bid: i64) -> NotedQuote (node), module demo.structs, line 140
struct impl_
{
    static constexpr auto name = "demo.structs::write_bid";

    static void eval([[maybe_unused]] ::hgraph::In<"bid", ::hgraph::TS<::hgraph::Int>> bid, [[maybe_unused]] ::hgraph::Out<::hgraph::NominalTSB<::hgl::demo::structs::m_::NotedQuote, ::hgraph::Field<"bid", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"note", ::hgraph::TS<::hgraph::Optional<::hgraph::Str>>>>> out_)
    {
        // when, line 141 (TIME-3)
        {
            ::hglrt::publish(out_, ::hgl::demo::structs::m_::d_::NotedQuote{::hglrt::checked_value(bid), std::nullopt});
            return;
        }
    }
};
}  // namespace n_::write_bid
namespace w_
{
// write_bid(bid: i64) -> NotedQuote (node), module demo.structs, line 140
::hgraph::Port<::hgraph::NominalTSB<::hgl::demo::structs::m_::NotedQuote, ::hgraph::Field<"bid", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"note", ::hgraph::TS<::hgraph::Optional<::hgraph::Str>>>>> write_bid(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> bid)
{
    return ::hgraph::wire<::hgl::demo::structs::m_::n_::write_bid::impl_>(wiring_, bid);
}
}  // namespace w_

// note_is_null(quote: NotedQuote) -> bool (`const fn`), module demo.structs, line 144
::hgraph::Bool note_is_null([[maybe_unused]] const ::hgl::demo::structs::m_::NotedQuote& quote)
{
    return (!((quote).note).has_value());
}

namespace n_::read_noted_quote
{
// read_noted_quote(probe: bool, quote: NotedQuote) -> tuple<bool, bool, bool> (node), module demo.structs, line 146
struct impl_
{
    static constexpr auto name = "demo.structs::read_noted_quote";

    static void eval([[maybe_unused]] ::hgraph::In<"probe", ::hgraph::TS<::hgraph::Bool>> probe, [[maybe_unused]] ::hgraph::In<"quote", ::hgraph::NominalTSB<::hgl::demo::structs::m_::NotedQuote, ::hgraph::Field<"bid", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"note", ::hgraph::TS<::hgraph::Optional<::hgraph::Str>>>>, ::hgraph::InputActivity::Passive> quote, [[maybe_unused]] ::hgraph::Out<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Bool>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Bool>>, ::hgraph::Field<"2", ::hgraph::TS<::hgraph::Bool>>>> out_)
    {
        // when, line 147 (TIME-3)
        if (::hglrt::fn_modified(probe) && ::hglrt::fn_all_valid(quote))
        {
            ::hglrt::publish(out_, std::tuple<::hgraph::Bool, ::hgraph::Bool, ::hgraph::Bool>{::hglrt::fn_all_valid(quote), ::hgl::demo::structs::m_::note_is_null(::hglrt::whole_value(quote)), ::hglrt::fn_valid(::hglrt::child<1>(quote))});
            return;
        }
    }
};
}  // namespace n_::read_noted_quote
namespace w_
{
// read_noted_quote(probe: bool, quote: NotedQuote) -> tuple<bool, bool, bool> (node), module demo.structs, line 146
::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Bool>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Bool>>, ::hgraph::Field<"2", ::hgraph::TS<::hgraph::Bool>>>> read_noted_quote(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::NominalTSB<::hgl::demo::structs::m_::NotedQuote, ::hgraph::Field<"bid", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"note", ::hgraph::TS<::hgraph::Optional<::hgraph::Str>>>>> quote)
{
    return ::hgraph::wire<::hgl::demo::structs::m_::n_::read_noted_quote::impl_>(wiring_, probe, quote);
}
}  // namespace w_

namespace n_::observe_noted_quote
{
// observe_noted_quote(probe: bool, bid: i64) -> tuple<bool, bool, bool> (graph), module demo.structs, line 152
struct impl_
{
    static ::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Bool>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Bool>>, ::hgraph::Field<"2", ::hgraph::TS<::hgraph::Bool>>>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> bid)
    {
        return ::hgl::demo::structs::m_::w_::read_noted_quote(wiring_, probe, ::hgl::demo::structs::m_::w_::write_bid(wiring_, bid));
    }
};
}  // namespace n_::observe_noted_quote
namespace w_
{
// observe_noted_quote(probe: bool, bid: i64) -> tuple<bool, bool, bool> (graph), module demo.structs, line 152
::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Bool>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Bool>>, ::hgraph::Field<"2", ::hgraph::TS<::hgraph::Bool>>>> observe_noted_quote(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> bid)
{
    return ::hgl::demo::structs::m_::n_::observe_noted_quote::impl_::compose(wiring_, probe, bid);
}
}  // namespace w_

namespace n_::relay_pair
{
// relay_pair(pair: tuple<i64, str>) -> tuple<i64, str> (node), module demo.structs, line 162
struct impl_
{
    static constexpr auto name = "demo.structs::relay_pair";

    static void eval([[maybe_unused]] ::hgraph::In<"pair", ::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>> pair, [[maybe_unused]] ::hgraph::Out<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>> out_)
    {
        // when, line 163 (TIME-3)
        if (::hglrt::fn_all_valid(pair))
        {
            ::hglrt::publish(out_, ::hglrt::whole_value(pair));
            return;
        }
    }
};
}  // namespace n_::relay_pair
namespace w_
{
// relay_pair(pair: tuple<i64, str>) -> tuple<i64, str> (node), module demo.structs, line 162
::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>> relay_pair(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>> pair)
{
    return ::hgraph::wire<::hgl::demo::structs::m_::n_::relay_pair::impl_>(wiring_, pair);
}
}  // namespace w_

namespace n_::make_outer
{
// make_outer(size: i64) -> Outer (node), module demo.structs, line 180
struct impl_
{
    static constexpr auto name = "demo.structs::make_outer";

    static void eval([[maybe_unused]] ::hgraph::In<"size", ::hgraph::TS<::hgraph::Int>> size, [[maybe_unused]] ::hgraph::Out<::hgraph::NativeTSB<::hgl::demo::structs::m_::Outer>> out_)
    {
        // when, line 182 (TIME-3)
        {
            if (::hglrt::eq__i64_i64_f64_f64(::hglrt::checked_value(size), 1, 0.0, 0.0))
            {
                ::hglrt::publish(out_, ::hgl::demo::structs::m_::Outer{::hgl::demo::structs::m_::Inner{1, 2}, ::hglrt::checked_value(size)});
                return;
            }
            ::hglrt::child<1>(out_).set(::hglrt::checked_value(size));
            ::hglrt::child<1>(::hglrt::child<0>(out_)).set(::hglrt::mul__i64_i64(::hglrt::checked_value(size), 10));
        }
    }
};
}  // namespace n_::make_outer
namespace w_
{
// make_outer(size: i64) -> Outer (node), module demo.structs, line 180
::hgraph::Port<::hgraph::NativeTSB<::hgl::demo::structs::m_::Outer>> make_outer(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> size)
{
    return ::hgraph::wire<::hgl::demo::structs::m_::n_::make_outer::impl_>(wiring_, size);
}
}  // namespace w_

namespace n_::read_outer
{
// read_outer(outer: Outer) -> i64 (node), module demo.structs, line 189
struct impl_
{
    static constexpr auto name = "demo.structs::read_outer";

    static void eval([[maybe_unused]] ::hgraph::In<"outer", ::hgraph::NativeTSB<::hgl::demo::structs::m_::Outer>> outer, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 190 (TIME-3)
        if (::hglrt::fn_all_valid(outer) && ::hglrt::fn_all_valid(::hglrt::child<0>(outer)))
        {
            [[maybe_unused]] const ::hgl::demo::structs::m_::Outer& whole = ::hglrt::whole_value(outer);
            if (::hglrt::fn_modified(::hglrt::child<1>(::hglrt::child<0>(outer))))
            {
                out_.set(::hglrt::add__i64_i64(((whole).inner).high, ::hglrt::checked_value(::hglrt::child<1>(outer))));
                return;
            }
            out_.set(::hglrt::checked_value(::hglrt::child<0>(::hglrt::child<0>(outer))));
            return;
        }
    }
};
}  // namespace n_::read_outer
namespace w_
{
// read_outer(outer: Outer) -> i64 (node), module demo.structs, line 189
::hgraph::Port<::hgraph::TS<::hgraph::Int>> read_outer(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::NativeTSB<::hgl::demo::structs::m_::Outer>> outer)
{
    return ::hgraph::wire<::hgl::demo::structs::m_::n_::read_outer::impl_>(wiring_, outer);
}
}  // namespace w_

namespace n_::outer_ticks
{
// outer_ticks(size: i64) -> i64 (graph), module demo.structs, line 197
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> size)
    {
        return ::hgl::demo::structs::m_::w_::read_outer(wiring_, ::hgl::demo::structs::m_::w_::make_outer(wiring_, size));
    }
};
}  // namespace n_::outer_ticks
namespace w_
{
// outer_ticks(size: i64) -> i64 (graph), module demo.structs, line 197
::hgraph::Port<::hgraph::TS<::hgraph::Int>> outer_ticks(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> size)
{
    return ::hgl::demo::structs::m_::n_::outer_ticks::impl_::compose(wiring_, size);
}
}  // namespace w_

namespace n_::outer_deltas
{
// outer_deltas(size: i64) -> Outer (node), module demo.structs, line 203
struct impl_
{
    static constexpr auto name = "demo.structs::outer_deltas";

    static void eval([[maybe_unused]] ::hgraph::In<"size", ::hgraph::TS<::hgraph::Int>> size, [[maybe_unused]] ::hgraph::Out<::hgraph::NativeTSB<::hgl::demo::structs::m_::Outer>> out_)
    {
        // when, line 204 (TIME-3)
        {
            if (::hglrt::eq__i64_i64_f64_f64(::hglrt::checked_value(size), 1, 0.0, 0.0))
            {
                ::hglrt::publish(out_, ::hgl::demo::structs::m_::Outer{::hgl::demo::structs::m_::Inner{1, 2}, ::hglrt::checked_value(size)});
                return;
            }
            ::hglrt::publish(out_, ::hgl::demo::structs::m_::d_::Outer{::hgl::demo::structs::m_::d_::Inner{std::nullopt, ::hglrt::checked_value(size)}, std::nullopt});
            return;
        }
    }
};
}  // namespace n_::outer_deltas
namespace w_
{
// outer_deltas(size: i64) -> Outer (node), module demo.structs, line 203
::hgraph::Port<::hgraph::NativeTSB<::hgl::demo::structs::m_::Outer>> outer_deltas(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> size)
{
    return ::hgraph::wire<::hgl::demo::structs::m_::n_::outer_deltas::impl_>(wiring_, size);
}
}  // namespace w_

namespace n_::bid_delta
{
// bid_delta(quote: NotedQuote) -> i64? (node), module demo.structs, line 217
struct impl_
{
    static constexpr auto name = "demo.structs::bid_delta";

    static void eval([[maybe_unused]] ::hgraph::In<"quote", ::hgraph::NominalTSB<::hgl::demo::structs::m_::NotedQuote, ::hgraph::Field<"bid", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"note", ::hgraph::TS<::hgraph::Optional<::hgraph::Str>>>>> quote, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> out_)
    {
        // when, line 218 (TIME-3)
        {
            ::hglrt::publish_nullable(out_, (::hglrt::fn_delta_value(quote)).bid);
            return;
        }
    }
};
}  // namespace n_::bid_delta
namespace w_
{
// bid_delta(quote: NotedQuote) -> i64? (node), module demo.structs, line 217
::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> bid_delta(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::NominalTSB<::hgl::demo::structs::m_::NotedQuote, ::hgraph::Field<"bid", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"note", ::hgraph::TS<::hgraph::Optional<::hgraph::Str>>>>> quote)
{
    return ::hgraph::wire<::hgl::demo::structs::m_::n_::bid_delta::impl_>(wiring_, quote);
}
}  // namespace w_

namespace n_::publish_nullable
{
// publish_nullable(command: i64) -> i64? (node), module demo.structs, line 231
struct impl_
{
    static constexpr auto name = "demo.structs::publish_nullable";

    static void eval([[maybe_unused]] ::hgraph::In<"command", ::hgraph::TS<::hgraph::Int>> command, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> out_)
    {
        // when, line 232 (TIME-3)
        {
            if (::hglrt::eq__i64_i64_f64_f64(::hglrt::checked_value(command), 0, 0.0, 0.0))
            {
                ::hglrt::publish_nullable(out_, std::nullopt);
                return;
            }
            ::hglrt::publish_nullable(out_, ::hglrt::checked_value(command));
            return;
        }
    }
};
}  // namespace n_::publish_nullable
namespace w_
{
// publish_nullable(command: i64) -> i64? (node), module demo.structs, line 231
::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> publish_nullable(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> command)
{
    return ::hgraph::wire<::hgl::demo::structs::m_::n_::publish_nullable::impl_>(wiring_, command);
}
}  // namespace w_

namespace n_::read_validity
{
// read_validity(probe: bool, watched: i64?) -> bool (node), module demo.structs, line 238
struct impl_
{
    static constexpr auto name = "demo.structs::read_validity";

    static void eval([[maybe_unused]] ::hgraph::In<"probe", ::hgraph::TS<::hgraph::Bool>, ::hgraph::InputValidity::Unchecked> probe, [[maybe_unused]] ::hgraph::In<"watched", ::hgraph::TS<::hgraph::Optional<::hgraph::Int>>, ::hgraph::InputActivity::Passive, ::hgraph::InputValidity::Unchecked> watched, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Bool>> out_)
    {
        // when, line 239 (TIME-3)
        if (::hglrt::fn_modified(probe))
        {
            out_.set(::hglrt::fn_valid(watched));
            return;
        }
    }
};
}  // namespace n_::read_validity
namespace w_
{
// read_validity(probe: bool, watched: i64?) -> bool (node), module demo.structs, line 238
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> read_validity(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> watched)
{
    return ::hgraph::wire<::hgl::demo::structs::m_::n_::read_validity::impl_>(wiring_, probe, watched);
}
}  // namespace w_

namespace n_::observe_validity
{
// observe_validity(command: i64, probe: bool) -> bool (graph), module demo.structs, line 242
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> command, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe)
    {
        return ::hgl::demo::structs::m_::w_::read_validity(wiring_, probe, ::hgl::demo::structs::m_::w_::publish_nullable(wiring_, command));
    }
};
}  // namespace n_::observe_validity
namespace w_
{
// observe_validity(command: i64, probe: bool) -> bool (graph), module demo.structs, line 242
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> observe_validity(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> command, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe)
{
    return ::hgl::demo::structs::m_::n_::observe_validity::impl_::compose(wiring_, command, probe);
}
}  // namespace w_

namespace n_::nullable_delta_payload
{
// nullable_delta_payload(input: i64?) -> i64? (node), module demo.structs, line 251
struct impl_
{
    static constexpr auto name = "demo.structs::nullable_delta_payload";

    static void eval([[maybe_unused]] ::hgraph::In<"input", ::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> input, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> out_)
    {
        // when, line 252 (TIME-3)
        if (::hglrt::fn_modified(input))
        {
            ::hglrt::publish_nullable(out_, (::hglrt::fn_delta_value(input)));
            return;
        }
    }
};
}  // namespace n_::nullable_delta_payload
namespace w_
{
// nullable_delta_payload(input: i64?) -> i64? (node), module demo.structs, line 251
::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> nullable_delta_payload(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> input)
{
    return ::hgraph::wire<::hgl::demo::structs::m_::n_::nullable_delta_payload::impl_>(wiring_, input);
}
}  // namespace w_

namespace n_::doubled
{
// doubled(reading: i64?) -> i64 (node), module demo.structs, line 262
struct impl_
{
    static constexpr auto name = "demo.structs::doubled";

    static void eval([[maybe_unused]] ::hgraph::In<"reading", ::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> reading, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 263 (TIME-3)
        {
            if ((!::hglrt::nullable_value(reading).has_value()))
            {
                out_.set(0);
                return;
            }
            out_.set(::hglrt::mul__i64_i64(::hglrt::nullable_payload(reading), 2));
            return;
        }
    }
};
}  // namespace n_::doubled
namespace w_
{
// doubled(reading: i64?) -> i64 (node), module demo.structs, line 262
::hgraph::Port<::hgraph::TS<::hgraph::Int>> doubled(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> reading)
{
    return ::hgraph::wire<::hgl::demo::structs::m_::n_::doubled::impl_>(wiring_, reading);
}
}  // namespace w_

namespace n_::action
{
// action(code: i64) -> i64? (node), module demo.structs, line 273
struct impl_
{
    static constexpr auto name = "demo.structs::action";

    static void eval([[maybe_unused]] ::hgraph::In<"code", ::hgraph::TS<::hgraph::Int>> code, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> out_)
    {
        // when, line 275 (TIME-3)
        {
            if (::hglrt::eq__i64_i64_f64_f64(::hglrt::checked_value(code), 2, 0.0, 0.0))
            {
                ::hglrt::fn_invalidate(out_);
            }
            else if (::hglrt::eq__i64_i64_f64_f64(::hglrt::checked_value(code), 1, 0.0, 0.0))
            {
                ::hglrt::publish_nullable(out_, std::nullopt);
                return;
            }
            else
            {
                ::hglrt::publish_nullable(out_, 7);
                return;
            }
        }
    }
};
}  // namespace n_::action
namespace w_
{
// action(code: i64) -> i64? (node), module demo.structs, line 273
::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> action(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> code)
{
    return ::hgraph::wire<::hgl::demo::structs::m_::n_::action::impl_>(wiring_, code);
}
}  // namespace w_

namespace n_::sample_timestamp
{
// sample_timestamp(sample: bool, watched: i64?) -> datetime? (node), module demo.structs, line 282
struct impl_
{
    static constexpr auto name = "demo.structs::sample_timestamp";

    static void eval([[maybe_unused]] ::hgraph::In<"sample", ::hgraph::TS<::hgraph::Bool>, ::hgraph::InputValidity::Unchecked> sample, [[maybe_unused]] ::hgraph::In<"watched", ::hgraph::TS<::hgraph::Optional<::hgraph::Int>>, ::hgraph::InputActivity::Passive, ::hgraph::InputValidity::Unchecked> watched, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Optional<::hgraph::DateTime>>> out_)
    {
        // when, line 283 (TIME-3)
        if (::hglrt::fn_modified(sample))
        {
            ::hglrt::publish_nullable(out_, ::hglrt::fn_last_modified(watched));
            return;
        }
    }
};
}  // namespace n_::sample_timestamp
namespace w_
{
// sample_timestamp(sample: bool, watched: i64?) -> datetime? (node), module demo.structs, line 282
::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::DateTime>>> sample_timestamp(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> sample, ::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> watched)
{
    return ::hgraph::wire<::hgl::demo::structs::m_::n_::sample_timestamp::impl_>(wiring_, sample, watched);
}
}  // namespace w_

namespace n_::scalar_timestamp
{
// scalar_timestamp(code: i64, sample: bool) -> datetime? (graph), module demo.structs, line 286
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::DateTime>>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> code, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> sample)
    {
        return ::hgl::demo::structs::m_::w_::sample_timestamp(wiring_, sample, ::hgl::demo::structs::m_::w_::action(wiring_, code));
    }
};
}  // namespace n_::scalar_timestamp
namespace w_
{
// scalar_timestamp(code: i64, sample: bool) -> datetime? (graph), module demo.structs, line 286
::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::DateTime>>> scalar_timestamp(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> code, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> sample)
{
    return ::hgl::demo::structs::m_::n_::scalar_timestamp::impl_::compose(wiring_, code, sample);
}
}  // namespace w_

namespace n_::whole_plain
{
// whole_plain(bid: i64) -> atomic<PlainQuote> (node), module demo.structs, line 304
struct impl_
{
    static constexpr auto name = "demo.structs::whole_plain";

    static void eval([[maybe_unused]] ::hgraph::In<"bid", ::hgraph::TS<::hgraph::Int>> bid, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgl::demo::structs::m_::PlainQuote>> out_)
    {
        // when, line 305 (TIME-3)
        {
            out_.set(::hgl::demo::structs::m_::PlainQuote{::hglrt::checked_value(bid), ::hglrt::add__i64_i64(::hglrt::checked_value(bid), 1)});
            return;
        }
    }
};
}  // namespace n_::whole_plain
namespace w_
{
// whole_plain(bid: i64) -> atomic<PlainQuote> (node), module demo.structs, line 304
::hgraph::Port<::hgraph::TS<::hgl::demo::structs::m_::PlainQuote>> whole_plain(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> bid)
{
    return ::hgraph::wire<::hgl::demo::structs::m_::n_::whole_plain::impl_>(wiring_, bid);
}
}  // namespace w_

namespace n_::spread
{
// spread(quote: atomic<PlainQuote>) -> i64 (node), module demo.structs, line 308
struct impl_
{
    static constexpr auto name = "demo.structs::spread";

    static void eval([[maybe_unused]] ::hgraph::In<"quote", ::hgraph::TS<::hgl::demo::structs::m_::PlainQuote>> quote, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 309 (TIME-3)
        {
            out_.set([&] { const ::hgraph::Int argument_00_ = ::hglrt::sub__i64_i64((::hglrt::checked_value(quote)).ask, (::hglrt::checked_value(quote)).bid); const ::hgraph::Int argument_01_ = (::hglrt::fn_delta_value(quote)).bid; return ::hglrt::add__i64_i64(argument_00_, argument_01_); }());
            return;
        }
    }
};
}  // namespace n_::spread
namespace w_
{
// spread(quote: atomic<PlainQuote>) -> i64 (node), module demo.structs, line 308
::hgraph::Port<::hgraph::TS<::hgraph::Int>> spread(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgl::demo::structs::m_::PlainQuote>> quote)
{
    return ::hgraph::wire<::hgl::demo::structs::m_::n_::spread::impl_>(wiring_, quote);
}
}  // namespace w_

namespace n_::spread_of
{
// spread_of(bid: i64) -> i64 (graph), module demo.structs, line 312
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> bid)
    {
        return ::hgl::demo::structs::m_::w_::spread(wiring_, ::hgl::demo::structs::m_::w_::whole_plain(wiring_, bid));
    }
};
}  // namespace n_::spread_of
namespace w_
{
// spread_of(bid: i64) -> i64 (graph), module demo.structs, line 312
::hgraph::Port<::hgraph::TS<::hgraph::Int>> spread_of(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> bid)
{
    return ::hgl::demo::structs::m_::n_::spread_of::impl_::compose(wiring_, bid);
}
}  // namespace w_

namespace n_::pair_sum
{
// pair_sum(pair: tuple<i64, i64>) -> i64 (node), module demo.structs, line 318
struct impl_
{
    static constexpr auto name = "demo.structs::pair_sum";

    static void eval([[maybe_unused]] ::hgraph::In<"pair", ::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Int>>>> pair, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 319 (TIME-3)
        if (::hglrt::fn_all_valid(pair))
        {
            out_.set([&] { const ::hgraph::Int& argument_00_ = ::hglrt::checked_value(::hglrt::child<0>(pair)); const ::hgraph::Int& argument_01_ = ::hglrt::checked_value(::hglrt::child<1>(pair)); return ::hglrt::add__i64_i64(argument_00_, argument_01_); }());
            return;
        }
    }
};
}  // namespace n_::pair_sum
namespace w_
{
// pair_sum(pair: tuple<i64, i64>) -> i64 (node), module demo.structs, line 318
::hgraph::Port<::hgraph::TS<::hgraph::Int>> pair_sum(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Int>>>> pair)
{
    return ::hgraph::wire<::hgl::demo::structs::m_::n_::pair_sum::impl_>(wiring_, pair);
}
}  // namespace w_

namespace n_::first_of
{
// first_of(pair: tuple<i64, str>) -> i64 (graph), module demo.structs, line 322
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>> pair)
    {
        return ::hglrt::field_port<0>(pair);
    }
};
}  // namespace n_::first_of
namespace w_
{
// first_of(pair: tuple<i64, str>) -> i64 (graph), module demo.structs, line 322
::hgraph::Port<::hgraph::TS<::hgraph::Int>> first_of(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>> pair)
{
    return ::hgl::demo::structs::m_::n_::first_of::impl_::compose(wiring_, pair);
}
}  // namespace w_

namespace n_::wrap
{
// wrap(bid: i64) -> Wrapped (node), module demo.structs, line 335
struct impl_
{
    static constexpr auto name = "demo.structs::wrap";

    static void eval([[maybe_unused]] ::hgraph::In<"bid", ::hgraph::TS<::hgraph::Int>> bid, [[maybe_unused]] ::hgraph::Out<::hgraph::NominalTSB<::hgl::demo::structs::m_::Wrapped, ::hgraph::Field<"noted", ::hgraph::NominalTSB<::hgl::demo::structs::m_::NotedQuote, ::hgraph::Field<"bid", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"note", ::hgraph::TS<::hgraph::Optional<::hgraph::Str>>>>>, ::hgraph::Field<"count", ::hgraph::TS<::hgraph::Int>>>> out_)
    {
        // when, line 336 (TIME-3)
        {
            ::hglrt::publish(out_, ::hgl::demo::structs::m_::d_::Wrapped{::hgl::demo::structs::m_::d_::NotedQuote{::hglrt::checked_value(bid), std::nullopt}, ::hglrt::checked_value(bid)});
            return;
        }
    }
};
}  // namespace n_::wrap
namespace w_
{
// wrap(bid: i64) -> Wrapped (node), module demo.structs, line 335
::hgraph::Port<::hgraph::NominalTSB<::hgl::demo::structs::m_::Wrapped, ::hgraph::Field<"noted", ::hgraph::NominalTSB<::hgl::demo::structs::m_::NotedQuote, ::hgraph::Field<"bid", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"note", ::hgraph::TS<::hgraph::Optional<::hgraph::Str>>>>>, ::hgraph::Field<"count", ::hgraph::TS<::hgraph::Int>>>> wrap(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> bid)
{
    return ::hgraph::wire<::hgl::demo::structs::m_::n_::wrap::impl_>(wiring_, bid);
}
}  // namespace w_

namespace n_::wrapped_sum
{
// wrapped_sum(wrapped: Wrapped) -> i64 (node), module demo.structs, line 339
struct impl_
{
    static constexpr auto name = "demo.structs::wrapped_sum";

    static void eval([[maybe_unused]] ::hgraph::In<"wrapped", ::hgraph::NominalTSB<::hgl::demo::structs::m_::Wrapped, ::hgraph::Field<"noted", ::hgraph::NominalTSB<::hgl::demo::structs::m_::NotedQuote, ::hgraph::Field<"bid", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"note", ::hgraph::TS<::hgraph::Optional<::hgraph::Str>>>>>, ::hgraph::Field<"count", ::hgraph::TS<::hgraph::Int>>>> wrapped, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 340 (TIME-3)
        if (::hglrt::fn_all_valid(wrapped) && ::hglrt::fn_all_valid(::hglrt::child<0>(wrapped)))
        {
            [[maybe_unused]] const ::hgl::demo::structs::m_::Wrapped& whole = ::hglrt::whole_value(wrapped);
            out_.set(::hglrt::add__i64_i64(((whole).noted).bid, (whole).count));
            return;
        }
    }
};
}  // namespace n_::wrapped_sum
namespace w_
{
// wrapped_sum(wrapped: Wrapped) -> i64 (node), module demo.structs, line 339
::hgraph::Port<::hgraph::TS<::hgraph::Int>> wrapped_sum(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::NominalTSB<::hgl::demo::structs::m_::Wrapped, ::hgraph::Field<"noted", ::hgraph::NominalTSB<::hgl::demo::structs::m_::NotedQuote, ::hgraph::Field<"bid", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"note", ::hgraph::TS<::hgraph::Optional<::hgraph::Str>>>>>, ::hgraph::Field<"count", ::hgraph::TS<::hgraph::Int>>>> wrapped)
{
    return ::hgraph::wire<::hgl::demo::structs::m_::n_::wrapped_sum::impl_>(wiring_, wrapped);
}
}  // namespace w_

namespace n_::wrapped_of
{
// wrapped_of(bid: i64) -> i64 (graph), module demo.structs, line 346
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> bid)
    {
        return ::hgl::demo::structs::m_::w_::wrapped_sum(wiring_, ::hgl::demo::structs::m_::w_::wrap(wiring_, bid));
    }
};
}  // namespace n_::wrapped_of
namespace w_
{
// wrapped_of(bid: i64) -> i64 (graph), module demo.structs, line 346
::hgraph::Port<::hgraph::TS<::hgraph::Int>> wrapped_of(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> bid)
{
    return ::hgl::demo::structs::m_::n_::wrapped_of::impl_::compose(wiring_, bid);
}
}  // namespace w_

namespace n_::atomic_bid
{
// atomic_bid(quote: atomic<MarketQuote>) -> i64 (node), module demo.structs, line 362
struct impl_
{
    static constexpr auto name = "demo.structs::atomic_bid";

    static void eval([[maybe_unused]] ::hgraph::In<"quote", ::hgraph::TS<::hgl::demo::structs::m_::MarketQuote>> quote, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 363 (TIME-3)
        {
            out_.set((::hglrt::checked_value(quote)).bid);
            return;
        }
    }
};
}  // namespace n_::atomic_bid
namespace w_
{
// atomic_bid(quote: atomic<MarketQuote>) -> i64 (node), module demo.structs, line 362
::hgraph::Port<::hgraph::TS<::hgraph::Int>> atomic_bid(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgl::demo::structs::m_::MarketQuote>> quote)
{
    return ::hgraph::wire<::hgl::demo::structs::m_::n_::atomic_bid::impl_>(wiring_, quote);
}
}  // namespace w_

}  // namespace hgl::demo::structs::m_

namespace hgl::demo::structs::m_::t_
{

void changing_a_sibling_field_does_not_publish_a_bid()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_06_ = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Bool>{true, false, true};
        const auto trace_02_ = ::hglrt::testing::Dense<::hgraph::Int>{10, 13, 11};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_02_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_04_ = input_01_; const auto argument_05_ = input_03_; return ::hgl::demo::structs::m_::w_::bid_ticks(wiring_, argument_04_, argument_05_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_07_ = ::hglrt::testing::Dense<::hgraph::Int>{10, std::nullopt, 11};
        ::hglrt::testing::expect_same(left_06_, right_07_);
    }();
}

void an_atomic_quote_wakes_its_bid_projection()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_06_ = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Int>{10, std::nullopt, 11};
        const auto trace_02_ = ::hglrt::testing::Dense<::hgraph::Int>{12, 13, std::nullopt};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_02_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_04_ = input_01_; const auto argument_05_ = input_03_; return ::hgl::demo::structs::m_::w_::atomic_bid_ticks(wiring_, argument_04_, argument_05_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_07_ = ::hglrt::testing::Dense<::hgraph::Int>{10, 10, 11};
        ::hglrt::testing::expect_same(left_06_, right_07_);
    }();
}

void a_struct_delta_keeps_the_final_write_to_each_field()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_06_ = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Int>{10, std::nullopt, 11};
        const auto trace_02_ = ::hglrt::testing::Dense<::hgraph::Int>{12, std::nullopt, 14};
        return ::hglrt::testing::evaluate<::hgl::demo::structs::m_::MarketQuote>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_02_, start_));
        const auto held_ = [&] { const auto argument_04_ = input_01_; const auto argument_05_ = input_03_; return ::hgl::demo::structs::m_::w_::write_quote_fields(wiring_, argument_04_, argument_05_); }();
        ::hglrt::testing::keep_held<::hgl::demo::structs::m_::MarketQuote>(wiring_, held_);
        ::hgl::core::m_::w_::i_::record::of_1demo_1structs_1_1MarketQuote_(wiring_, held_, ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_07_ = ::hglrt::testing::Dense<::hgl::demo::structs::m_::MarketQuote>{::hglrt::testing::Cell<::hgl::demo::structs::m_::MarketQuote>{::hgl::demo::structs::m_::d_::MarketQuote{10, 12}}, std::nullopt, ::hglrt::testing::Cell<::hgl::demo::structs::m_::MarketQuote>{::hgl::demo::structs::m_::d_::MarketQuote{11, 14}}};
        ::hglrt::testing::expect_same(left_06_, right_07_);
    }();
}

void a_struct_is_valid_once_one_field_is()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_09_ = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Bool>{true, true, true, true};
        const auto trace_02_ = ::hglrt::testing::Dense<::hgraph::Bool>{std::nullopt, true, std::nullopt, false};
        const auto trace_04_ = ::hglrt::testing::Dense<::hgraph::Int>{std::nullopt, 10, std::nullopt, 12};
        return ::hglrt::testing::evaluate<::hgraph::Bool>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(trace_02_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_05_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_04_, start_));
        ::hgl::core::m_::w_::i_::record::of_bool(wiring_, [&] { const auto argument_06_ = input_01_; const auto argument_07_ = input_03_; const auto argument_08_ = input_05_; return ::hgl::demo::structs::m_::w_::observe_quote_valid(wiring_, argument_06_, argument_07_, argument_08_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_010_ = ::hglrt::testing::Dense<::hgraph::Bool>{false, true, true, true};
        ::hglrt::testing::expect_same(left_09_, right_010_);
    }();
}

void a_struct_is_all_valid_once_every_field_is()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_09_ = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Bool>{true, true, true, true};
        const auto trace_02_ = ::hglrt::testing::Dense<::hgraph::Bool>{std::nullopt, true, std::nullopt, false};
        const auto trace_04_ = ::hglrt::testing::Dense<::hgraph::Int>{std::nullopt, 10, std::nullopt, 12};
        return ::hglrt::testing::evaluate<::hgraph::Bool>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(trace_02_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_05_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_04_, start_));
        ::hgl::core::m_::w_::i_::record::of_bool(wiring_, [&] { const auto argument_06_ = input_01_; const auto argument_07_ = input_03_; const auto argument_08_ = input_05_; return ::hgl::demo::structs::m_::w_::observe_quote_all_valid(wiring_, argument_06_, argument_07_, argument_08_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_010_ = ::hglrt::testing::Dense<::hgraph::Bool>{false, false, false, true};
        ::hglrt::testing::expect_same(left_09_, right_010_);
    }();
}

void a_whole_struct_is_a_trace_cell()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgl::demo::structs::m_::MarketQuote>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::NativeTSB<::hgl::demo::structs::m_::MarketQuote>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_1demo_1structs_1_1MarketQuote_(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgl::demo::structs::m_::MarketQuote>{::hglrt::testing::Cell<::hgl::demo::structs::m_::MarketQuote>{::hglrt::testing::Whole<::hgl::demo::structs::m_::MarketQuote>{::hgl::demo::structs::m_::MarketQuote{10, 12}}}, std::nullopt, ::hglrt::testing::Cell<::hgl::demo::structs::m_::MarketQuote>{::hgl::demo::structs::m_::d_::MarketQuote{std::nullopt, 13}}}, start_));
        const auto held_ = ::hgl::demo::structs::m_::w_::relay_quote(wiring_, input_00_);
        ::hglrt::testing::keep_held<::hgl::demo::structs::m_::MarketQuote>(wiring_, held_);
        ::hgl::core::m_::w_::i_::record::of_1demo_1structs_1_1MarketQuote_(wiring_, held_, ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgl::demo::structs::m_::MarketQuote>{::hglrt::testing::Cell<::hgl::demo::structs::m_::MarketQuote>{::hglrt::testing::Whole<::hgl::demo::structs::m_::MarketQuote>{::hgl::demo::structs::m_::MarketQuote{10, 12}}}, std::nullopt, ::hglrt::testing::Cell<::hgl::demo::structs::m_::MarketQuote>{::hglrt::testing::Whole<::hgl::demo::structs::m_::MarketQuote>{::hgl::demo::structs::m_::MarketQuote{10, 13}}}};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
}

void a_tuple_is_valid_once_one_position_is_and_all_valid_once_both_are()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_09_ = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Bool>{true, true, true, true};
        const auto trace_02_ = ::hglrt::testing::Dense<::hgraph::Bool>{std::nullopt, true, std::nullopt, false};
        const auto trace_04_ = ::hglrt::testing::Dense<::hgraph::Int>{std::nullopt, 3, std::nullopt, 8};
        return ::hglrt::testing::evaluate<std::tuple<::hgraph::Bool, ::hgraph::Bool>>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(trace_02_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_05_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_04_, start_));
        const auto held_ = [&] { const auto argument_06_ = input_01_; const auto argument_07_ = input_03_; const auto argument_08_ = input_05_; return ::hgl::demo::structs::m_::w_::observe_part_validity(wiring_, argument_06_, argument_07_, argument_08_); }();
        ::hglrt::testing::keep_held<std::tuple<::hgraph::Bool, ::hgraph::Bool>>(wiring_, held_);
        ::hgl::core::m_::w_::i_::record::of_1tuple_1bool_1_1bool_1_(wiring_, held_, ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_010_ = ::hglrt::testing::Dense<std::tuple<::hgraph::Bool, ::hgraph::Bool>>{::hglrt::testing::Cell<std::tuple<::hgraph::Bool, ::hgraph::Bool>>{::hglrt::testing::Whole<std::tuple<::hgraph::Bool, ::hgraph::Bool>>{std::tuple<::hgraph::Bool, ::hgraph::Bool>{false, false}}}, ::hglrt::testing::Cell<std::tuple<::hgraph::Bool, ::hgraph::Bool>>{::hglrt::testing::Whole<std::tuple<::hgraph::Bool, ::hgraph::Bool>>{std::tuple<::hgraph::Bool, ::hgraph::Bool>{true, false}}}, ::hglrt::testing::Cell<std::tuple<::hgraph::Bool, ::hgraph::Bool>>{::hglrt::testing::Whole<std::tuple<::hgraph::Bool, ::hgraph::Bool>>{std::tuple<::hgraph::Bool, ::hgraph::Bool>{true, false}}}, ::hglrt::testing::Cell<std::tuple<::hgraph::Bool, ::hgraph::Bool>>{::hglrt::testing::Whole<std::tuple<::hgraph::Bool, ::hgraph::Bool>>{std::tuple<::hgraph::Bool, ::hgraph::Bool>{true, true}}}};
        ::hglrt::testing::expect_same(left_09_, right_010_);
    }();
}

void a_struct_with_an_optional_field_is_all_valid_and_read_whole()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_06_ = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Bool>{true, true};
        const auto trace_02_ = ::hglrt::testing::Dense<::hgraph::Int>{std::nullopt, 5};
        return ::hglrt::testing::evaluate<std::tuple<::hgraph::Bool, ::hgraph::Bool, ::hgraph::Bool>>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_02_, start_));
        const auto held_ = [&] { const auto argument_04_ = input_01_; const auto argument_05_ = input_03_; return ::hgl::demo::structs::m_::w_::observe_noted_quote(wiring_, argument_04_, argument_05_); }();
        ::hglrt::testing::keep_held<std::tuple<::hgraph::Bool, ::hgraph::Bool, ::hgraph::Bool>>(wiring_, held_);
        ::hgl::core::m_::w_::i_::record::of_1tuple_1bool_1_1bool_1_1bool_1_(wiring_, held_, ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_07_ = ::hglrt::testing::Dense<std::tuple<::hgraph::Bool, ::hgraph::Bool, ::hgraph::Bool>>{std::nullopt, ::hglrt::testing::Cell<std::tuple<::hgraph::Bool, ::hgraph::Bool, ::hgraph::Bool>>{::hglrt::testing::Whole<std::tuple<::hgraph::Bool, ::hgraph::Bool, ::hgraph::Bool>>{std::tuple<::hgraph::Bool, ::hgraph::Bool, ::hgraph::Bool>{true, true, false}}}};
        ::hglrt::testing::expect_same(left_06_, right_07_);
    }();
}

void a_whole_tuple_is_a_trace_cell()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<std::tuple<::hgraph::Int, ::hgraph::Str>>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_1tuple_1i64_1_1str_1_(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<std::tuple<::hgraph::Int, ::hgraph::Str>>{::hglrt::testing::Cell<std::tuple<::hgraph::Int, ::hgraph::Str>>{::hglrt::testing::Whole<std::tuple<::hgraph::Int, ::hgraph::Str>>{std::tuple<::hgraph::Int, ::hgraph::Str>{1, ::hgraph::Str{"a"}}}}, std::nullopt, ::hglrt::testing::Cell<std::tuple<::hgraph::Int, ::hgraph::Str>>{::hglrt::testing::Whole<std::tuple<::hgraph::Int, ::hgraph::Str>>{std::tuple<::hgraph::Int, ::hgraph::Str>{2, ::hgraph::Str{"b"}}}}}, start_));
        const auto held_ = ::hgl::demo::structs::m_::w_::relay_pair(wiring_, input_00_);
        ::hglrt::testing::keep_held<std::tuple<::hgraph::Int, ::hgraph::Str>>(wiring_, held_);
        ::hgl::core::m_::w_::i_::record::of_1tuple_1i64_1_1str_1_(wiring_, held_, ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<std::tuple<::hgraph::Int, ::hgraph::Str>>{::hglrt::testing::Cell<std::tuple<::hgraph::Int, ::hgraph::Str>>{::hglrt::testing::Whole<std::tuple<::hgraph::Int, ::hgraph::Str>>{std::tuple<::hgraph::Int, ::hgraph::Str>{1, ::hgraph::Str{"a"}}}}, std::nullopt, ::hglrt::testing::Cell<std::tuple<::hgraph::Int, ::hgraph::Str>>{::hglrt::testing::Whole<std::tuple<::hgraph::Int, ::hgraph::Str>>{std::tuple<::hgraph::Int, ::hgraph::Str>{2, ::hgraph::Str{"b"}}}}};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
}

void a_nested_struct_field_is_written_and_read_by_its_parts()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 3, std::nullopt, 4}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::structs::m_::w_::outer_ticks(wiring_, input_00_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgraph::Int>{3, 33, std::nullopt, 44};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
}

void a_nested_struct_delta_is_recorded()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgl::demo::structs::m_::Outer>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 5}, start_));
        const auto held_ = ::hgl::demo::structs::m_::w_::outer_deltas(wiring_, input_00_);
        ::hglrt::testing::keep_held<::hgl::demo::structs::m_::Outer>(wiring_, held_);
        ::hgl::core::m_::w_::i_::record::of_1demo_1structs_1_1Outer_(wiring_, held_, ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgl::demo::structs::m_::Outer>{::hglrt::testing::Cell<::hgl::demo::structs::m_::Outer>{::hgl::demo::structs::m_::d_::Outer{::hgl::demo::structs::m_::d_::Inner{1, 2}, 1}}, ::hglrt::testing::Cell<::hgl::demo::structs::m_::Outer>{::hglrt::testing::Whole<::hgl::demo::structs::m_::Outer>{::hgl::demo::structs::m_::Outer{::hgl::demo::structs::m_::Inner{1, 5}, 1}}}};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
}

void a_struct_delta_reads_the_field_that_changed()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<std::optional<::hgraph::Int>>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::NominalTSB<::hgl::demo::structs::m_::NotedQuote, ::hgraph::Field<"bid", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"note", ::hgraph::TS<::hgraph::Optional<::hgraph::Str>>>>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_1demo_1structs_1_1NotedQuote_(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgl::demo::structs::m_::NotedQuote>{::hglrt::testing::Cell<::hgl::demo::structs::m_::NotedQuote>{::hglrt::testing::Whole<::hgl::demo::structs::m_::NotedQuote>{::hgl::demo::structs::m_::NotedQuote{1, ::hgraph::Str{"a"}}}}, ::hglrt::testing::Cell<::hgl::demo::structs::m_::NotedQuote>{::hgl::demo::structs::m_::d_::NotedQuote{std::nullopt, ::hgraph::Str{"b"}}}, ::hglrt::testing::Cell<::hgl::demo::structs::m_::NotedQuote>{::hgl::demo::structs::m_::d_::NotedQuote{3, std::nullopt}}}, start_));
        const auto held_ = ::hgl::demo::structs::m_::w_::bid_delta(wiring_, input_00_);
        ::hglrt::testing::keep_held<std::optional<::hgraph::Int>>(wiring_, held_);
        ::hgl::core::m_::w_::i_::record::of_1i64_1_(wiring_, held_, ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<std::optional<::hgraph::Int>>{std::optional<::hgraph::Int>{1}, std::optional<::hgraph::Int>{std::nullopt}, std::optional<::hgraph::Int>{3}};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
}

void publication_establishes_validity()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_06_ = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Int>{std::nullopt, 0, std::nullopt, 7};
        const auto trace_02_ = ::hglrt::testing::Dense<::hgraph::Bool>{true, true, true, true};
        return ::hglrt::testing::evaluate<::hgraph::Bool>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(trace_02_, start_));
        ::hgl::core::m_::w_::i_::record::of_bool(wiring_, [&] { const auto argument_04_ = input_01_; const auto argument_05_ = input_03_; return ::hgl::demo::structs::m_::w_::observe_validity(wiring_, argument_04_, argument_05_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_07_ = ::hglrt::testing::Dense<::hgraph::Bool>{false, true, true, true};
        ::hglrt::testing::expect_same(left_06_, right_07_);
    }();
}

void a_published_null_has_a_delta()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<std::optional<::hgraph::Int>>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_1i64_1_(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<std::optional<::hgraph::Int>>{std::optional<::hgraph::Int>{7}, std::optional<::hgraph::Int>{std::nullopt}, std::nullopt, std::optional<::hgraph::Int>{std::nullopt}, std::optional<::hgraph::Int>{0}}, start_));
        const auto held_ = ::hgl::demo::structs::m_::w_::nullable_delta_payload(wiring_, input_00_);
        ::hglrt::testing::keep_held<std::optional<::hgraph::Int>>(wiring_, held_);
        ::hgl::core::m_::w_::i_::record::of_1i64_1_(wiring_, held_, ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<std::optional<::hgraph::Int>>{std::optional<::hgraph::Int>{7}, std::optional<::hgraph::Int>{std::nullopt}, std::nullopt, std::optional<::hgraph::Int>{std::nullopt}, std::optional<::hgraph::Int>{0}};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
}

void a_narrowed_read()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_1i64_1_(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<std::optional<::hgraph::Int>>{std::optional<::hgraph::Int>{std::nullopt}, std::optional<::hgraph::Int>{2}, std::nullopt, std::optional<::hgraph::Int>{5}}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::structs::m_::w_::doubled(wiring_, input_00_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgraph::Int>{0, 4, std::nullopt, 10};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
}

void last_modified_follows_the_last_publication()
{
    ::hglrt::testing::statement();
    [[maybe_unused]] const ::hgraph::DateTime t = ::hgl::core::m_::earliest_start();
    ::hglrt::testing::statement();
    [&] {
        const auto left_06_ = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Int>{std::nullopt, 0, std::nullopt, 0, 2, 1};
        const auto trace_02_ = ::hglrt::testing::Dense<::hgraph::Bool>{true, true, true, true, true, true};
        return ::hglrt::testing::evaluate<std::optional<::hgraph::DateTime>>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(trace_02_, start_));
        const auto held_ = [&] { const auto argument_04_ = input_01_; const auto argument_05_ = input_03_; return ::hgl::demo::structs::m_::w_::scalar_timestamp(wiring_, argument_04_, argument_05_); }();
        ::hglrt::testing::keep_held<std::optional<::hgraph::DateTime>>(wiring_, held_);
        ::hgl::core::m_::w_::i_::record::of_1datetime_1_(wiring_, held_, ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_07_ = ::hglrt::testing::Dense<std::optional<::hgraph::DateTime>>{std::optional<::hgraph::DateTime>{std::nullopt}, std::optional<::hgraph::DateTime>{::hglrt::add__datetime_duration(t, ::hgraph::TimeDelta{1})}, std::optional<::hgraph::DateTime>{::hglrt::add__datetime_duration(t, ::hgraph::TimeDelta{1})}, std::optional<::hgraph::DateTime>{::hglrt::add__datetime_duration(t, ::hgraph::TimeDelta{3})}, std::optional<::hgraph::DateTime>{std::nullopt}, std::optional<::hgraph::DateTime>{::hglrt::add__datetime_duration(t, ::hgraph::TimeDelta{5})}};
        ::hglrt::testing::expect_same(left_06_, right_07_);
    }();
}

void an_atomic_input_is_read_whole()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 5}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::structs::m_::w_::spread_of(wiring_, input_00_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgraph::Int>{2, 6};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
}

void a_tuple_position_is_read_and_projected()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Int>>>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_1tuple_1i64_1_1i64_1_(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<std::tuple<::hgraph::Int, ::hgraph::Int>>{::hglrt::testing::Cell<std::tuple<::hgraph::Int, ::hgraph::Int>>{::hglrt::testing::Whole<std::tuple<::hgraph::Int, ::hgraph::Int>>{std::tuple<::hgraph::Int, ::hgraph::Int>{1, 2}}}, std::nullopt, ::hglrt::testing::Cell<std::tuple<::hgraph::Int, ::hgraph::Int>>{::hglrt::testing::Whole<std::tuple<::hgraph::Int, ::hgraph::Int>>{std::tuple<::hgraph::Int, ::hgraph::Int>{3, 4}}}}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::structs::m_::w_::pair_sum(wiring_, input_00_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgraph::Int>{3, std::nullopt, 7};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_04_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_1tuple_1i64_1_1str_1_(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<std::tuple<::hgraph::Int, ::hgraph::Str>>{::hglrt::testing::Cell<std::tuple<::hgraph::Int, ::hgraph::Str>>{::hglrt::testing::Whole<std::tuple<::hgraph::Int, ::hgraph::Str>>{std::tuple<::hgraph::Int, ::hgraph::Str>{1, ::hgraph::Str{"a"}}}}, ::hglrt::testing::Cell<std::tuple<::hgraph::Int, ::hgraph::Str>>{::hglrt::testing::Whole<std::tuple<::hgraph::Int, ::hgraph::Str>>{std::tuple<::hgraph::Int, ::hgraph::Str>{2, ::hgraph::Str{"b"}}}}}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::structs::m_::w_::first_of(wiring_, input_03_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_05_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 2};
        ::hglrt::testing::expect_same(left_04_, right_05_);
    }();
}

void a_struct_that_holds_one_with_an_optional_field_is_read_whole()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 2}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::structs::m_::w_::wrapped_of(wiring_, input_00_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgraph::Int>{2, 4};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
}

void an_atomic_struct_is_a_trace_cell()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_06_ = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Int>{10, std::nullopt, 11};
        const auto trace_02_ = ::hglrt::testing::Dense<::hgraph::Int>{12, 13, std::nullopt};
        return ::hglrt::testing::evaluate<::hglrt::atomic_trace<::hgl::demo::structs::m_::MarketQuote>>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_02_, start_));
        const auto held_ = [&] { const auto argument_04_ = input_01_; const auto argument_05_ = input_03_; return ::hgl::demo::structs::m_::w_::whole_quote(wiring_, argument_04_, argument_05_); }();
        ::hglrt::testing::keep_held<::hglrt::atomic_trace<::hgl::demo::structs::m_::MarketQuote>>(wiring_, held_);
        ::hgl::core::m_::w_::i_::record::of_1atomic_1demo_1structs_1_1MarketQuote_1_(wiring_, held_, ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_07_ = ::hglrt::testing::Dense<::hglrt::atomic_trace<::hgl::demo::structs::m_::MarketQuote>>{::hglrt::testing::Cell<::hglrt::atomic_trace<::hgl::demo::structs::m_::MarketQuote>>{::hgl::demo::structs::m_::MarketQuote{10, 12}}, ::hglrt::testing::Cell<::hglrt::atomic_trace<::hgl::demo::structs::m_::MarketQuote>>{::hgl::demo::structs::m_::MarketQuote{10, 13}}, ::hglrt::testing::Cell<::hglrt::atomic_trace<::hgl::demo::structs::m_::MarketQuote>>{::hgl::demo::structs::m_::MarketQuote{11, 13}}};
        ::hglrt::testing::expect_same(left_06_, right_07_);
    }();
}

void an_atomic_struct_is_replayed_whole()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgl::demo::structs::m_::MarketQuote>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_1atomic_1demo_1structs_1_1MarketQuote_1_(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hglrt::atomic_trace<::hgl::demo::structs::m_::MarketQuote>>{::hglrt::testing::Cell<::hglrt::atomic_trace<::hgl::demo::structs::m_::MarketQuote>>{::hgl::demo::structs::m_::MarketQuote{1, 2}}, std::nullopt, ::hglrt::testing::Cell<::hglrt::atomic_trace<::hgl::demo::structs::m_::MarketQuote>>{::hgl::demo::structs::m_::MarketQuote{3, 4}}}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::structs::m_::w_::atomic_bid(wiring_, input_00_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgraph::Int>{1, std::nullopt, 3};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
}

}  // namespace hgl::demo::structs::m_::t_
