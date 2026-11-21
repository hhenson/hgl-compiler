// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include "demo.nested.h"
#include "core.h"

// Names this file writes, which no macro of a header above takes.
#undef Basket
#undef a_child_of_a_list_of_tuples_is_wired
#undef a_dictionary_of_tuples_is_published_by_a_delta
#undef a_dictionary_of_tuples_is_relayed_whole
#undef a_dictionary_of_tuples_replays_its_deltas
#undef a_fixed_list_of_tuples_is_published_by_a_delta
#undef a_fixed_list_of_tuples_is_relayed_whole
#undef a_key_child_of_a_dictionary_of_tuples_is_wired
#undef a_list_of_tuples_is_published_by_a_delta
#undef a_list_of_tuples_is_relayed_whole
#undef a_list_of_tuples_replays_its_deltas
#undef a_struct_with_a_list_field_is_a_configuration
#undef a_struct_with_a_list_field_is_published_by_a_delta
#undef a_struct_with_a_list_field_is_relayed_whole
#undef a_struct_with_a_list_field_replays_its_cells
#undef a_tuple_of_a_list_is_published_by_a_delta
#undef a_tuple_of_a_list_is_relayed_whole
#undef a_tuple_of_a_list_replays_its_cells
#undef basket
#undef baskets
#undef book
#undef core
#undef count
#undef counted
#undef counted_all
#undef counts
#undef demo
#undef edit_book
#undef entry_x
#undef fill
#undef first_pair
#undef grow_pairs
#undef key
#undef longer
#undef name
#undef nested
#undef pairs
#undef record
#undef relay_basket
#undef relay_book
#undef relay_fixed_pairs
#undef relay_pairs
#undef relay_tagged
#undef renamed
#undef replay
#undef retag
#undef second_relayed
#undef set_fixed_pair
#undef slot
#undef start
#undef step
#undef sum
#undef summed
#undef tagged
#undef tick
#undef total
#undef value

namespace hgl::demo::nested::m_
{

namespace n_::relay_pairs
{
// relay_pairs(pairs: list<tuple<i64, str>>) -> list<tuple<i64, str>> (node), module demo.nested, line 11
struct impl_
{
    static constexpr auto name = "demo.nested::relay_pairs";

    static void eval([[maybe_unused]] ::hgraph::In<"pairs", ::hgraph::TSL<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>>> pairs, [[maybe_unused]] ::hgraph::Out<::hgraph::TSL<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>>> out_)
    {
        // when, line 12 (TIME-3)
        {
            ::hglrt::publish(out_, ::hglrt::checked_whole(pairs));
            return;
        }
    }
};
}  // namespace n_::relay_pairs
namespace w_
{
// relay_pairs(pairs: list<tuple<i64, str>>) -> list<tuple<i64, str>> (node), module demo.nested, line 11
::hgraph::Port<::hgraph::TSL<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>>> relay_pairs(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSL<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>>> pairs)
{
    return ::hgraph::wire<::hgl::demo::nested::m_::n_::relay_pairs::impl_>(wiring_, pairs);
}
}  // namespace w_

namespace n_::grow_pairs
{
// grow_pairs(step: i64) -> list<tuple<i64, str>> (node), module demo.nested, line 15
struct impl_
{
    static constexpr auto name = "demo.nested::grow_pairs";

    static void eval([[maybe_unused]] ::hgraph::In<"step", ::hgraph::TS<::hgraph::Int>> step, [[maybe_unused]] ::hgraph::Out<::hgraph::TSL<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>>> out_)
    {
        // when, line 16 (TIME-3)
        {
            if (::hglrt::eq__i64_i64_f64_f64(::hglrt::checked_value(step), 0, 0.0, 0.0))
            {
                ::hglrt::publish(out_, ::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>{::std::tuple<::hgraph::Int, ::hgraph::Str>{1, ::hgraph::Str{"a"}}});
                return;
            }
            if (::hglrt::eq__i64_i64_f64_f64(::hglrt::checked_value(step), 1, 0.0, 0.0))
            {
                ::hglrt::publish(out_, ::hglrt::list_delta_of<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>(::hglrt::dict_of<::hgraph::Int, ::std::tuple<::std::optional<::hgraph::Int>, ::std::optional<::hgraph::Str>>>({{1, ::std::tuple<::std::optional<::hgraph::Int>, ::std::optional<::hgraph::Str>>{2, ::hgraph::Str{"b"}}}}), ::hglrt::set<::hgraph::Int>{}, ::std::nullopt));
                return;
            }
            [[maybe_unused]] const ::std::tuple<::std::optional<::hgraph::Int>, ::std::optional<::hgraph::Str>> renamed = ::std::tuple<::std::optional<::hgraph::Int>, ::std::optional<::hgraph::Str>>{::std::nullopt, ::hgraph::Str{"c"}};
            ::hglrt::publish(out_, ::hglrt::list_delta_of<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>(::hglrt::dict_of<::hgraph::Int, ::std::tuple<::std::optional<::hgraph::Int>, ::std::optional<::hgraph::Str>>>({{0, renamed}}), ::hglrt::set<::hgraph::Int>{}, ::std::nullopt));
            return;
        }
    }
};
}  // namespace n_::grow_pairs
namespace w_
{
// grow_pairs(step: i64) -> list<tuple<i64, str>> (node), module demo.nested, line 15
::hgraph::Port<::hgraph::TSL<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>>> grow_pairs(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> step)
{
    return ::hgraph::wire<::hgl::demo::nested::m_::n_::grow_pairs::impl_>(wiring_, step);
}
}  // namespace w_

namespace n_::relay_fixed_pairs
{
// relay_fixed_pairs(pairs: list<tuple<i64, str>, 2>) -> list<tuple<i64, str>, 2> (node), module demo.nested, line 47
struct impl_
{
    static constexpr auto name = "demo.nested::relay_fixed_pairs";

    static void eval([[maybe_unused]] ::hgraph::In<"pairs", ::hgraph::TSL<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>, 2>> pairs, [[maybe_unused]] ::hgraph::Out<::hgraph::TSL<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>, 2>> out_)
    {
        // when, line 48 (TIME-3)
        if (::hglrt::fn_all_valid(pairs))
        {
            ::hglrt::publish(out_, ::hglrt::checked_whole(pairs));
            return;
        }
    }
};
}  // namespace n_::relay_fixed_pairs
namespace w_
{
// relay_fixed_pairs(pairs: list<tuple<i64, str>, 2>) -> list<tuple<i64, str>, 2> (node), module demo.nested, line 47
::hgraph::Port<::hgraph::TSL<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>, 2>> relay_fixed_pairs(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSL<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>, 2>> pairs)
{
    return ::hgraph::wire<::hgl::demo::nested::m_::n_::relay_fixed_pairs::impl_>(wiring_, pairs);
}
}  // namespace w_

namespace n_::set_fixed_pair
{
// set_fixed_pair(slot: i64, value: i64) -> list<tuple<i64, str>, 2> (node), module demo.nested, line 51
struct impl_
{
    static constexpr auto name = "demo.nested::set_fixed_pair";

    static void eval([[maybe_unused]] ::hgraph::In<"slot", ::hgraph::TS<::hgraph::Int>> slot, [[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::Out<::hgraph::TSL<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>, 2>> out_)
    {
        // when, line 52 (TIME-3)
        {
            ::hglrt::publish(out_, ::hglrt::list_delta_of<::std::array<::std::tuple<::hgraph::Int, ::hgraph::Str>, 2>>(::hglrt::dict_of<::hgraph::Int, ::std::tuple<::std::optional<::hgraph::Int>, ::std::optional<::hgraph::Str>>>({{::hglrt::checked_value(slot), ::std::tuple<::std::optional<::hgraph::Int>, ::std::optional<::hgraph::Str>>{::hglrt::checked_value(value), ::hgraph::Str{"x"}}}}), ::hglrt::set<::hgraph::Int>{}, ::std::nullopt));
            return;
        }
    }
};
}  // namespace n_::set_fixed_pair
namespace w_
{
// set_fixed_pair(slot: i64, value: i64) -> list<tuple<i64, str>, 2> (node), module demo.nested, line 51
::hgraph::Port<::hgraph::TSL<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>, 2>> set_fixed_pair(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> slot, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgraph::wire<::hgl::demo::nested::m_::n_::set_fixed_pair::impl_>(wiring_, slot, value);
}
}  // namespace w_

namespace n_::relay_tagged
{
// relay_tagged(tagged: tuple<list<i64>, bool>) -> tuple<list<i64>, bool> (node), module demo.nested, line 66
struct impl_
{
    static constexpr auto name = "demo.nested::relay_tagged";

    static void eval([[maybe_unused]] ::hgraph::In<"tagged", ::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TSL<::hgraph::TS<::hgraph::Int>>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Bool>>>> tagged, [[maybe_unused]] ::hgraph::Out<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TSL<::hgraph::TS<::hgraph::Int>>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Bool>>>> out_)
    {
        // when, line 67 (TIME-3)
        if (::hglrt::fn_all_valid(tagged))
        {
            ::hglrt::publish(out_, ::hglrt::whole_value(tagged));
            return;
        }
    }
};
}  // namespace n_::relay_tagged
namespace w_
{
// relay_tagged(tagged: tuple<list<i64>, bool>) -> tuple<list<i64>, bool> (node), module demo.nested, line 66
::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TSL<::hgraph::TS<::hgraph::Int>>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Bool>>>> relay_tagged(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TSL<::hgraph::TS<::hgraph::Int>>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Bool>>>> tagged)
{
    return ::hgraph::wire<::hgl::demo::nested::m_::n_::relay_tagged::impl_>(wiring_, tagged);
}
}  // namespace w_

namespace n_::retag
{
// retag(step: i64) -> tuple<list<i64>, bool> (node), module demo.nested, line 70
struct impl_
{
    static constexpr auto name = "demo.nested::retag";

    static void eval([[maybe_unused]] ::hgraph::In<"step", ::hgraph::TS<::hgraph::Int>> step, [[maybe_unused]] ::hgraph::Out<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TSL<::hgraph::TS<::hgraph::Int>>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Bool>>>> out_)
    {
        // when, line 71 (TIME-3)
        {
            if (::hglrt::eq__i64_i64_f64_f64(::hglrt::checked_value(step), 0, 0.0, 0.0))
            {
                ::hglrt::publish(out_, ::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>{::hglrt::list<::hgraph::Int>{1, 2}, true});
                return;
            }
            [[maybe_unused]] const ::std::tuple<::std::optional<::hglrt::list_delta<::hglrt::list<::hgraph::Int>>>, ::std::optional<::hgraph::Bool>> longer = ::std::tuple<::std::optional<::hglrt::list_delta<::hglrt::list<::hgraph::Int>>>, ::std::optional<::hgraph::Bool>>{::hglrt::list_delta_of<::hglrt::list<::hgraph::Int>>(::hglrt::dict_of<::hgraph::Int, ::hgraph::Int>({{2, 7}}), ::hglrt::set<::hgraph::Int>{}, ::std::nullopt), ::std::nullopt};
            if (::hglrt::eq__i64_i64_f64_f64(::hglrt::checked_value(step), 1, 0.0, 0.0))
            {
                ::hglrt::publish(out_, longer);
                return;
            }
            ::hglrt::publish(out_, ::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>{::hglrt::list<::hgraph::Int>{}, false});
            return;
        }
    }
};
}  // namespace n_::retag
namespace w_
{
// retag(step: i64) -> tuple<list<i64>, bool> (node), module demo.nested, line 70
::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TSL<::hgraph::TS<::hgraph::Int>>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Bool>>>> retag(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> step)
{
    return ::hgraph::wire<::hgl::demo::nested::m_::n_::retag::impl_>(wiring_, step);
}
}  // namespace w_

namespace n_::relay_book
{
// relay_book(book: dict<str, tuple<i64, str>>) -> dict<str, tuple<i64, str>> (node), module demo.nested, line 93
struct impl_
{
    static constexpr auto name = "demo.nested::relay_book";

    static void eval([[maybe_unused]] ::hgraph::In<"book", ::hgraph::TSD<::hgraph::Str, ::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>>> book, [[maybe_unused]] ::hgraph::Out<::hgraph::TSD<::hgraph::Str, ::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>>> out_)
    {
        // when, line 94 (TIME-3)
        {
            ::hglrt::keyed_publish(out_, ::hglrt::keyed_value(book));
            return;
        }
    }
};
}  // namespace n_::relay_book
namespace w_
{
// relay_book(book: dict<str, tuple<i64, str>>) -> dict<str, tuple<i64, str>> (node), module demo.nested, line 93
::hgraph::Port<::hgraph::TSD<::hgraph::Str, ::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>>> relay_book(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSD<::hgraph::Str, ::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>>> book)
{
    return ::hgraph::wire<::hgl::demo::nested::m_::n_::relay_book::impl_>(wiring_, book);
}
}  // namespace w_

namespace n_::edit_book
{
// edit_book(step: i64) -> dict<str, tuple<i64, str>> (node), module demo.nested, line 97
struct impl_
{
    static constexpr auto name = "demo.nested::edit_book";

    static void eval([[maybe_unused]] ::hgraph::In<"step", ::hgraph::TS<::hgraph::Int>> step, [[maybe_unused]] ::hgraph::Out<::hgraph::TSD<::hgraph::Str, ::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>>> out_)
    {
        // when, line 98 (TIME-3)
        {
            if (::hglrt::eq__i64_i64_f64_f64(::hglrt::checked_value(step), 0, 0.0, 0.0))
            {
                ::hglrt::keyed_publish(out_, ::hglrt::dict_of<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>({{::hgraph::Str{"x"}, ::std::tuple<::hgraph::Int, ::hgraph::Str>{1, ::hgraph::Str{"a"}}}, {::hgraph::Str{"y"}, ::std::tuple<::hgraph::Int, ::hgraph::Str>{2, ::hgraph::Str{"b"}}}}));
                return;
            }
            [[maybe_unused]] const ::std::tuple<::std::optional<::hgraph::Int>, ::std::optional<::hgraph::Str>> renamed = ::std::tuple<::std::optional<::hgraph::Int>, ::std::optional<::hgraph::Str>>{::std::nullopt, ::hgraph::Str{"c"}};
            if (::hglrt::eq__i64_i64_f64_f64(::hglrt::checked_value(step), 1, 0.0, 0.0))
            {
                ::hglrt::keyed_publish(out_, ::hglrt::dict_delta<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>{::hglrt::dict_of<::hgraph::Str, ::std::tuple<::std::optional<::hgraph::Int>, ::std::optional<::hgraph::Str>>>({{::hgraph::Str{"x"}, renamed}}), ::hglrt::set<::hgraph::Str>{}});
                return;
            }
            ::hglrt::keyed_publish(out_, ::hglrt::dict_delta<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>{::hglrt::dict<::hgraph::Str, ::std::tuple<::std::optional<::hgraph::Int>, ::std::optional<::hgraph::Str>>>{}, ::hglrt::set<::hgraph::Str>{::hgraph::Str{"y"}}});
            return;
        }
    }
};
}  // namespace n_::edit_book
namespace w_
{
// edit_book(step: i64) -> dict<str, tuple<i64, str>> (node), module demo.nested, line 97
::hgraph::Port<::hgraph::TSD<::hgraph::Str, ::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>>> edit_book(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> step)
{
    return ::hgraph::wire<::hgl::demo::nested::m_::n_::edit_book::impl_>(wiring_, step);
}
}  // namespace w_

namespace n_::relay_basket
{
// relay_basket(basket: Basket) -> Basket (node), module demo.nested, line 133
struct impl_
{
    static constexpr auto name = "demo.nested::relay_basket";

    static void eval([[maybe_unused]] ::hgraph::In<"basket", ::hgraph::NominalTSB<::hgl::demo::nested::m_::Basket, ::hgraph::Field<"name", ::hgraph::TS<::hgraph::Str>>, ::hgraph::Field<"counts", ::hgraph::TSL<::hgraph::TS<::hgraph::Int>>>>> basket, [[maybe_unused]] ::hgraph::Out<::hgraph::NominalTSB<::hgl::demo::nested::m_::Basket, ::hgraph::Field<"name", ::hgraph::TS<::hgraph::Str>>, ::hgraph::Field<"counts", ::hgraph::TSL<::hgraph::TS<::hgraph::Int>>>>> out_)
    {
        // when, line 134 (TIME-3)
        if (::hglrt::fn_all_valid(basket))
        {
            ::hglrt::publish(out_, ::hglrt::whole_value(basket));
            return;
        }
    }
};
}  // namespace n_::relay_basket
namespace w_
{
// relay_basket(basket: Basket) -> Basket (node), module demo.nested, line 133
::hgraph::Port<::hgraph::NominalTSB<::hgl::demo::nested::m_::Basket, ::hgraph::Field<"name", ::hgraph::TS<::hgraph::Str>>, ::hgraph::Field<"counts", ::hgraph::TSL<::hgraph::TS<::hgraph::Int>>>>> relay_basket(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::NominalTSB<::hgl::demo::nested::m_::Basket, ::hgraph::Field<"name", ::hgraph::TS<::hgraph::Str>>, ::hgraph::Field<"counts", ::hgraph::TSL<::hgraph::TS<::hgraph::Int>>>>> basket)
{
    return ::hgraph::wire<::hgl::demo::nested::m_::n_::relay_basket::impl_>(wiring_, basket);
}
}  // namespace w_

namespace n_::fill
{
// fill(step: i64) -> Basket (node), module demo.nested, line 137
struct impl_
{
    static constexpr auto name = "demo.nested::fill";

    static void eval([[maybe_unused]] ::hgraph::In<"step", ::hgraph::TS<::hgraph::Int>> step, [[maybe_unused]] ::hgraph::Out<::hgraph::NominalTSB<::hgl::demo::nested::m_::Basket, ::hgraph::Field<"name", ::hgraph::TS<::hgraph::Str>>, ::hgraph::Field<"counts", ::hgraph::TSL<::hgraph::TS<::hgraph::Int>>>>> out_)
    {
        // when, line 138 (TIME-3)
        {
            if (::hglrt::eq__i64_i64_f64_f64(::hglrt::checked_value(step), 0, 0.0, 0.0))
            {
                ::hglrt::publish(out_, ::hgl::demo::nested::m_::Basket{::hgraph::Str{"b"}, ::hglrt::list<::hgraph::Int>{1, 2}});
                return;
            }
            if (::hglrt::eq__i64_i64_f64_f64(::hglrt::checked_value(step), 1, 0.0, 0.0))
            {
                ::hglrt::publish(out_, ::hgl::demo::nested::m_::d_::Basket{::std::nullopt, ::hglrt::list_delta_of<::hglrt::list<::hgraph::Int>>(::hglrt::dict_of<::hgraph::Int, ::hgraph::Int>({{2, 3}}), ::hglrt::set<::hgraph::Int>{}, ::std::nullopt)});
                return;
            }
            ::hglrt::publish(out_, ::hgl::demo::nested::m_::d_::Basket{::std::nullopt, ::hglrt::list_delta_of<::hglrt::list<::hgraph::Int>>(::hglrt::dict<::hgraph::Int, ::hgraph::Int>{}, ::hglrt::set<::hgraph::Int>{}, 0)});
            return;
        }
    }
};
}  // namespace n_::fill
namespace w_
{
// fill(step: i64) -> Basket (node), module demo.nested, line 137
::hgraph::Port<::hgraph::NominalTSB<::hgl::demo::nested::m_::Basket, ::hgraph::Field<"name", ::hgraph::TS<::hgraph::Str>>, ::hgraph::Field<"counts", ::hgraph::TSL<::hgraph::TS<::hgraph::Int>>>>> fill(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> step)
{
    return ::hgraph::wire<::hgl::demo::nested::m_::n_::fill::impl_>(wiring_, step);
}
}  // namespace w_

// total(basket: Basket) -> i64 (`const fn`), module demo.nested, line 145
::hgraph::Int total([[maybe_unused]] const ::hgl::demo::nested::m_::Basket& basket)
{
    [[maybe_unused]] ::hgraph::Int sum = 0;
    for (const ::hgraph::Int& count : (basket).counts)
    {
        sum = ::hglrt::add__i64_i64(sum, count);
    }
    return sum;
}

namespace n_::summed
{
// summed(basket: Basket) -> i64 (node), module demo.nested, line 153
struct impl_
{
    static constexpr auto name = "demo.nested::summed";

    static void eval([[maybe_unused]] ::hgraph::In<"basket", ::hgraph::NominalTSB<::hgl::demo::nested::m_::Basket, ::hgraph::Field<"name", ::hgraph::TS<::hgraph::Str>>, ::hgraph::Field<"counts", ::hgraph::TSL<::hgraph::TS<::hgraph::Int>>>>> basket, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 154 (TIME-3)
        if (::hglrt::fn_all_valid(basket))
        {
            out_.set(::hgl::demo::nested::m_::total(::hglrt::whole_value(basket)));
            return;
        }
    }
};
}  // namespace n_::summed
namespace w_
{
// summed(basket: Basket) -> i64 (node), module demo.nested, line 153
::hgraph::Port<::hgraph::TS<::hgraph::Int>> summed(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::NominalTSB<::hgl::demo::nested::m_::Basket, ::hgraph::Field<"name", ::hgraph::TS<::hgraph::Str>>, ::hgraph::Field<"counts", ::hgraph::TSL<::hgraph::TS<::hgraph::Int>>>>> basket)
{
    return ::hgraph::wire<::hgl::demo::nested::m_::n_::summed::impl_>(wiring_, basket);
}
}  // namespace w_

namespace n_::first_pair
{
// first_pair(pairs: list<tuple<i64, str>>) -> tuple<i64, str> (graph), module demo.nested, line 181
struct impl_
{
    static ::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TSL<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>>> pairs)
    {
        return ::hglrt::element_port(pairs, 0);
    }
};
}  // namespace n_::first_pair
namespace w_
{
// first_pair(pairs: list<tuple<i64, str>>) -> tuple<i64, str> (graph), module demo.nested, line 181
::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>> first_pair(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSL<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>>> pairs)
{
    return ::hgl::demo::nested::m_::n_::first_pair::impl_::compose(wiring_, pairs);
}
}  // namespace w_

namespace n_::second_relayed
{
// second_relayed(pairs: list<tuple<i64, str>>) -> tuple<i64, str> (graph), module demo.nested, line 183
struct impl_
{
    static ::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TSL<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>>> pairs)
    {
        return ::hglrt::element_port(::hgl::demo::nested::m_::w_::relay_pairs(wiring_, pairs), 1);
    }
};
}  // namespace n_::second_relayed
namespace w_
{
// second_relayed(pairs: list<tuple<i64, str>>) -> tuple<i64, str> (graph), module demo.nested, line 183
::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>> second_relayed(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSL<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>>> pairs)
{
    return ::hgl::demo::nested::m_::n_::second_relayed::impl_::compose(wiring_, pairs);
}
}  // namespace w_

namespace n_::entry_x
{
// entry_x(book: dict<str, tuple<i64, str>>) -> tuple<i64, str> (graph), module demo.nested, line 192
struct impl_
{
    // The entry of a dictionary its key selects, on line 192: a
    // designation of it (EXPR-7, TIME-10).
    struct selected_00_
    {
        static constexpr auto name = "demo.nested:entry";
        static void start(::hgraph::State<::hgraph::Int> state_) { state_.set(::hgraph::Int{0}); }
        static void eval(::hgraph::In<"dictionary", ::hgraph::TSD<::hgraph::Str, ::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>>, ::hgraph::InputActivity::Structural, ::hgraph::InputValidity::Unchecked> dictionary, ::hgraph::In<"key", ::hgraph::TS<::hgraph::Str>> key, ::hgraph::State<::hgraph::Int> state_, ::hgraph::Out<::hgraph::REF<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>>> out_)
        {
            ::hglrt::select_entry(dictionary, key, state_.modify(), out_);
        }
    };

    static ::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TSD<::hgraph::Str, ::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>>> book)
    {
        return ::hgraph::wire<selected_00_>(wiring_, ::hgl::demo::nested::m_::w_::relay_book(wiring_, book), ::hglrt::constant<::hgraph::Str>(wiring_, ::hgraph::Str{"x"})).as<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>>();
    }
};
}  // namespace n_::entry_x
namespace w_
{
// entry_x(book: dict<str, tuple<i64, str>>) -> tuple<i64, str> (graph), module demo.nested, line 192
::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>> entry_x(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSD<::hgraph::Str, ::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>>> book)
{
    return ::hgl::demo::nested::m_::n_::entry_x::impl_::compose(wiring_, book);
}
}  // namespace w_

namespace n_::counted
{
// counted(const basket: Basket, tick: i64) -> i64 (node), module demo.nested, line 200
struct impl_
{
    static constexpr auto name = "demo.nested::counted";

    static void eval([[maybe_unused]] ::hgraph::Scalar<"basket", ::hglrt::configured<::hgl::demo::nested::m_::Basket>> basket, [[maybe_unused]] ::hgraph::In<"tick", ::hgraph::TS<::hgraph::Int>> tick, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 201 (TIME-3)
        {
            out_.set(::hglrt::add__i64_i64(::hglrt::checked_value(tick), ::hgl::demo::nested::m_::total(basket.value().held)));
            return;
        }
    }
};
}  // namespace n_::counted
namespace w_
{
// counted(const basket: Basket, tick: i64) -> i64 (node), module demo.nested, line 200
::hgraph::Port<::hgraph::TS<::hgraph::Int>> counted(::hgraph::Wiring& wiring_, const ::hgl::demo::nested::m_::Basket& basket, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> tick)
{
    return ::hgraph::wire<::hgl::demo::nested::m_::n_::counted::impl_>(wiring_, ::hglrt::configured<::hgl::demo::nested::m_::Basket>{basket}, tick);
}
}  // namespace w_

namespace n_::counted_all
{
// counted_all(const baskets: list<Basket>, tick: i64) -> i64 (node), module demo.nested, line 204
struct impl_
{
    static constexpr auto name = "demo.nested::counted_all";

    static void eval([[maybe_unused]] ::hgraph::Scalar<"baskets", ::hglrt::list<::hgl::demo::nested::m_::Basket>> baskets, [[maybe_unused]] ::hgraph::In<"tick", ::hgraph::TS<::hgraph::Int>> tick, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 205 (TIME-3)
        {
            [[maybe_unused]] ::hgraph::Int sum = ::hglrt::checked_value(tick);
            for (const ::hgl::demo::nested::m_::Basket& basket : baskets.value())
            {
                sum = ::hglrt::add__i64_i64(sum, ::hgl::demo::nested::m_::total(basket));
            }
            out_.set(sum);
            return;
        }
    }
};
}  // namespace n_::counted_all
namespace w_
{
// counted_all(const baskets: list<Basket>, tick: i64) -> i64 (node), module demo.nested, line 204
::hgraph::Port<::hgraph::TS<::hgraph::Int>> counted_all(::hgraph::Wiring& wiring_, const ::hglrt::list<::hgl::demo::nested::m_::Basket>& baskets, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> tick)
{
    return ::hgraph::wire<::hgl::demo::nested::m_::n_::counted_all::impl_>(wiring_, baskets, tick);
}
}  // namespace w_

}  // namespace hgl::demo::nested::m_

namespace hgl::demo::nested::m_::t_
{

void a_list_of_tuples_is_relayed_whole()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_03_ = ::hglrt::testing::evaluate<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TSL<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_1list_1tuple_1i64_1_1str_1_1_(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::testing::Cell<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::testing::Whole<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>{::std::tuple<::hgraph::Int, ::hgraph::Str>{1, ::hgraph::Str{"a"}}, ::std::tuple<::hgraph::Int, ::hgraph::Str>{2, ::hgraph::Str{"b"}}}}}, ::hglrt::testing::Cell<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::testing::Whole<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>{::std::tuple<::hgraph::Int, ::hgraph::Str>{3, ::hgraph::Str{"c"}}}}}}, start_));
        struct held_01_
        {
            static void eval(::hgraph::In<"result", ::hgraph::TSL<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>>> result_, ::hgraph::DateTime now_)
            {
                if (!::hglrt::fn_modified(result_))
                {
                    return;
                }
                ::std::optional<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>> whole_;
                try
                {
                    whole_.emplace(::hglrt::checked_whole(result_));
                }
                catch (const ::hglrt::Failure&)
                {
                }
                ::hglrt::testing::hold<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>(now_, ::std::move(whole_));
            }
        };
        const auto result_02_ = ::hgl::demo::nested::m_::w_::relay_pairs(wiring_, input_00_);
        ::hgraph::wire<held_01_>(wiring_, result_02_);
        ::hgl::core::m_::w_::i_::record::of_1list_1tuple_1i64_1_1str_1_1_(wiring_, result_02_, ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_04_ = ::hglrt::testing::Dense<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::testing::Cell<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::testing::Whole<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>{::std::tuple<::hgraph::Int, ::hgraph::Str>{1, ::hgraph::Str{"a"}}, ::std::tuple<::hgraph::Int, ::hgraph::Str>{2, ::hgraph::Str{"b"}}}}}, ::hglrt::testing::Cell<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::testing::Whole<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>{::std::tuple<::hgraph::Int, ::hgraph::Str>{3, ::hgraph::Str{"c"}}}}}};
        ::hglrt::testing::expect_same(left_03_, right_04_);
    }();
}

void a_list_of_tuples_is_published_by_a_delta()
{
    ::hglrt::testing::statement();
    [[maybe_unused]] const ::std::tuple<::std::optional<::hgraph::Int>, ::std::optional<::hgraph::Str>> renamed = ::std::tuple<::std::optional<::hgraph::Int>, ::std::optional<::hgraph::Str>>{::std::nullopt, ::hgraph::Str{"c"}};
    ::hglrt::testing::statement();
    [&] {
        const auto left_03_ = ::hglrt::testing::evaluate<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{0, 1, 2}, start_));
        struct held_01_
        {
            static void eval(::hgraph::In<"result", ::hgraph::TSL<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>>> result_, ::hgraph::DateTime now_)
            {
                if (!::hglrt::fn_modified(result_))
                {
                    return;
                }
                ::std::optional<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>> whole_;
                try
                {
                    whole_.emplace(::hglrt::checked_whole(result_));
                }
                catch (const ::hglrt::Failure&)
                {
                }
                ::hglrt::testing::hold<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>(now_, ::std::move(whole_));
            }
        };
        const auto result_02_ = ::hgl::demo::nested::m_::w_::grow_pairs(wiring_, input_00_);
        ::hgraph::wire<held_01_>(wiring_, result_02_);
        ::hgl::core::m_::w_::i_::record::of_1list_1tuple_1i64_1_1str_1_1_(wiring_, result_02_, ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_04_ = ::hglrt::testing::Dense<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::testing::Cell<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::testing::Whole<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>{::std::tuple<::hgraph::Int, ::hgraph::Str>{1, ::hgraph::Str{"a"}}}}}, ::hglrt::testing::Cell<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::std::in_place_index<0>, ::hglrt::list_delta_of<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>(::hglrt::dict_of<::hgraph::Int, ::std::tuple<::std::optional<::hgraph::Int>, ::std::optional<::hgraph::Str>>>({{1, ::std::tuple<::std::optional<::hgraph::Int>, ::std::optional<::hgraph::Str>>{2, ::hgraph::Str{"b"}}}}), ::hglrt::set<::hgraph::Int>{}, 2)}, ::hglrt::testing::Cell<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::std::in_place_index<0>, ::hglrt::list_delta_of<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>(::hglrt::dict_of<::hgraph::Int, ::std::tuple<::std::optional<::hgraph::Int>, ::std::optional<::hgraph::Str>>>({{0, renamed}}), ::hglrt::set<::hgraph::Int>{}, ::std::nullopt)}};
        ::hglrt::testing::expect_same(left_03_, right_04_);
    }();
}

void a_list_of_tuples_replays_its_deltas()
{
    ::hglrt::testing::statement();
    [[maybe_unused]] const ::std::tuple<::std::optional<::hgraph::Int>, ::std::optional<::hgraph::Str>> renamed = ::std::tuple<::std::optional<::hgraph::Int>, ::std::optional<::hgraph::Str>>{::std::nullopt, ::hgraph::Str{"c"}};
    ::hglrt::testing::statement();
    [&] {
        const auto left_03_ = ::hglrt::testing::evaluate<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TSL<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_1list_1tuple_1i64_1_1str_1_1_(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::testing::Cell<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::testing::Whole<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>{::std::tuple<::hgraph::Int, ::hgraph::Str>{1, ::hgraph::Str{"a"}}, ::std::tuple<::hgraph::Int, ::hgraph::Str>{2, ::hgraph::Str{"b"}}}}}, ::hglrt::testing::Cell<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::std::in_place_index<0>, ::hglrt::list_delta_of<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>(::hglrt::dict_of<::hgraph::Int, ::std::tuple<::std::optional<::hgraph::Int>, ::std::optional<::hgraph::Str>>>({{0, renamed}}), ::hglrt::set<::hgraph::Int>{}, ::std::nullopt)}, ::hglrt::testing::Cell<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::testing::Whole<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>{::std::tuple<::hgraph::Int, ::hgraph::Str>{5, ::hgraph::Str{"e"}}}}}}, start_));
        struct held_01_
        {
            static void eval(::hgraph::In<"result", ::hgraph::TSL<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>>> result_, ::hgraph::DateTime now_)
            {
                if (!::hglrt::fn_modified(result_))
                {
                    return;
                }
                ::std::optional<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>> whole_;
                try
                {
                    whole_.emplace(::hglrt::checked_whole(result_));
                }
                catch (const ::hglrt::Failure&)
                {
                }
                ::hglrt::testing::hold<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>(now_, ::std::move(whole_));
            }
        };
        const auto result_02_ = ::hgl::demo::nested::m_::w_::relay_pairs(wiring_, input_00_);
        ::hgraph::wire<held_01_>(wiring_, result_02_);
        ::hgl::core::m_::w_::i_::record::of_1list_1tuple_1i64_1_1str_1_1_(wiring_, result_02_, ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_04_ = ::hglrt::testing::Dense<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::testing::Cell<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::testing::Whole<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>{::std::tuple<::hgraph::Int, ::hgraph::Str>{1, ::hgraph::Str{"a"}}, ::std::tuple<::hgraph::Int, ::hgraph::Str>{2, ::hgraph::Str{"b"}}}}}, ::hglrt::testing::Cell<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::testing::Whole<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>{::std::tuple<::hgraph::Int, ::hgraph::Str>{1, ::hgraph::Str{"c"}}, ::std::tuple<::hgraph::Int, ::hgraph::Str>{2, ::hgraph::Str{"b"}}}}}, ::hglrt::testing::Cell<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::testing::Whole<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>{::std::tuple<::hgraph::Int, ::hgraph::Str>{5, ::hgraph::Str{"e"}}}}}};
        ::hglrt::testing::expect_same(left_03_, right_04_);
    }();
}

void a_fixed_list_of_tuples_is_relayed_whole()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_03_ = ::hglrt::testing::evaluate<::std::array<::std::tuple<::hgraph::Int, ::hgraph::Str>, 2>>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TSL<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>, 2>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_1list_1tuple_1i64_1_1str_1_1_12_1_(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::std::array<::std::tuple<::hgraph::Int, ::hgraph::Str>, 2>>{::hglrt::testing::Cell<::std::array<::std::tuple<::hgraph::Int, ::hgraph::Str>, 2>>{::hglrt::testing::Whole<::std::array<::std::tuple<::hgraph::Int, ::hgraph::Str>, 2>>{::std::array<::std::tuple<::hgraph::Int, ::hgraph::Str>, 2>{::std::tuple<::hgraph::Int, ::hgraph::Str>{1, ::hgraph::Str{"a"}}, ::std::tuple<::hgraph::Int, ::hgraph::Str>{2, ::hgraph::Str{"b"}}}}}}, start_));
        struct held_01_
        {
            static void eval(::hgraph::In<"result", ::hgraph::TSL<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>, 2>> result_, ::hgraph::DateTime now_)
            {
                if (!::hglrt::fn_modified(result_))
                {
                    return;
                }
                ::std::optional<::std::array<::std::tuple<::hgraph::Int, ::hgraph::Str>, 2>> whole_;
                try
                {
                    whole_.emplace(::hglrt::checked_whole(result_));
                }
                catch (const ::hglrt::Failure&)
                {
                }
                ::hglrt::testing::hold<::std::array<::std::tuple<::hgraph::Int, ::hgraph::Str>, 2>>(now_, ::std::move(whole_));
            }
        };
        const auto result_02_ = ::hgl::demo::nested::m_::w_::relay_fixed_pairs(wiring_, input_00_);
        ::hgraph::wire<held_01_>(wiring_, result_02_);
        ::hgl::core::m_::w_::i_::record::of_1list_1tuple_1i64_1_1str_1_1_12_1_(wiring_, result_02_, ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_04_ = ::hglrt::testing::Dense<::std::array<::std::tuple<::hgraph::Int, ::hgraph::Str>, 2>>{::hglrt::testing::Cell<::std::array<::std::tuple<::hgraph::Int, ::hgraph::Str>, 2>>{::hglrt::testing::Whole<::std::array<::std::tuple<::hgraph::Int, ::hgraph::Str>, 2>>{::std::array<::std::tuple<::hgraph::Int, ::hgraph::Str>, 2>{::std::tuple<::hgraph::Int, ::hgraph::Str>{1, ::hgraph::Str{"a"}}, ::std::tuple<::hgraph::Int, ::hgraph::Str>{2, ::hgraph::Str{"b"}}}}}};
        ::hglrt::testing::expect_same(left_03_, right_04_);
    }();
}

void a_fixed_list_of_tuples_is_published_by_a_delta()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_08_ = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Int>{0, 1};
        const auto trace_02_ = ::hglrt::testing::Dense<::hgraph::Int>{5, 6};
        return ::hglrt::testing::evaluate<::std::array<::std::tuple<::hgraph::Int, ::hgraph::Str>, 2>>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_02_, start_));
        struct held_06_
        {
            static void eval(::hgraph::In<"result", ::hgraph::TSL<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>, 2>> result_, ::hgraph::DateTime now_)
            {
                if (!::hglrt::fn_modified(result_))
                {
                    return;
                }
                ::std::optional<::std::array<::std::tuple<::hgraph::Int, ::hgraph::Str>, 2>> whole_;
                try
                {
                    whole_.emplace(::hglrt::checked_whole(result_));
                }
                catch (const ::hglrt::Failure&)
                {
                }
                ::hglrt::testing::hold<::std::array<::std::tuple<::hgraph::Int, ::hgraph::Str>, 2>>(now_, ::std::move(whole_));
            }
        };
        const auto result_07_ = [&] { const auto argument_04_ = input_01_; const auto argument_05_ = input_03_; return ::hgl::demo::nested::m_::w_::set_fixed_pair(wiring_, argument_04_, argument_05_); }();
        ::hgraph::wire<held_06_>(wiring_, result_07_);
        ::hgl::core::m_::w_::i_::record::of_1list_1tuple_1i64_1_1str_1_1_12_1_(wiring_, result_07_, ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_09_ = ::hglrt::testing::Dense<::std::array<::std::tuple<::hgraph::Int, ::hgraph::Str>, 2>>{::hglrt::testing::Cell<::std::array<::std::tuple<::hgraph::Int, ::hgraph::Str>, 2>>{::std::in_place_index<0>, ::hglrt::list_delta_of<::std::array<::std::tuple<::hgraph::Int, ::hgraph::Str>, 2>>(::hglrt::dict_of<::hgraph::Int, ::std::tuple<::std::optional<::hgraph::Int>, ::std::optional<::hgraph::Str>>>({{0, ::std::tuple<::std::optional<::hgraph::Int>, ::std::optional<::hgraph::Str>>{5, ::hgraph::Str{"x"}}}}), ::hglrt::set<::hgraph::Int>{}, ::std::nullopt)}, ::hglrt::testing::Cell<::std::array<::std::tuple<::hgraph::Int, ::hgraph::Str>, 2>>{::hglrt::testing::Whole<::std::array<::std::tuple<::hgraph::Int, ::hgraph::Str>, 2>>{::std::array<::std::tuple<::hgraph::Int, ::hgraph::Str>, 2>{::std::tuple<::hgraph::Int, ::hgraph::Str>{5, ::hgraph::Str{"x"}}, ::std::tuple<::hgraph::Int, ::hgraph::Str>{6, ::hgraph::Str{"x"}}}}}};
        ::hglrt::testing::expect_same(left_08_, right_09_);
    }();
}

void a_tuple_of_a_list_is_relayed_whole()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_03_ = ::hglrt::testing::evaluate<::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TSL<::hgraph::TS<::hgraph::Int>>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Bool>>>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_1tuple_1list_1i64_1_1_1bool_1_(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>>{::hglrt::testing::Cell<::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>>{::hglrt::testing::Whole<::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>>{::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>{::hglrt::list<::hgraph::Int>{1, 2}, true}}}, ::hglrt::testing::Cell<::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>>{::hglrt::testing::Whole<::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>>{::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>{::hglrt::list<::hgraph::Int>{3}, false}}}}, start_));
        struct held_01_
        {
            static void eval(::hgraph::In<"result", ::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TSL<::hgraph::TS<::hgraph::Int>>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Bool>>>> result_, ::hgraph::DateTime now_)
            {
                if (!::hglrt::fn_modified(result_))
                {
                    return;
                }
                ::std::optional<::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>> whole_;
                try
                {
                    whole_.emplace(::hglrt::whole_value(result_));
                }
                catch (const ::hglrt::Failure&)
                {
                }
                ::hglrt::testing::hold<::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>>(now_, ::std::move(whole_));
            }
        };
        const auto result_02_ = ::hgl::demo::nested::m_::w_::relay_tagged(wiring_, input_00_);
        ::hgraph::wire<held_01_>(wiring_, result_02_);
        ::hgl::core::m_::w_::i_::record::of_1tuple_1list_1i64_1_1_1bool_1_(wiring_, result_02_, ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_04_ = ::hglrt::testing::Dense<::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>>{::hglrt::testing::Cell<::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>>{::hglrt::testing::Whole<::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>>{::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>{::hglrt::list<::hgraph::Int>{1, 2}, true}}}, ::hglrt::testing::Cell<::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>>{::hglrt::testing::Whole<::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>>{::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>{::hglrt::list<::hgraph::Int>{3}, false}}}};
        ::hglrt::testing::expect_same(left_03_, right_04_);
    }();
}

void a_tuple_of_a_list_is_published_by_a_delta()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_03_ = ::hglrt::testing::evaluate<::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{0, 1, 2}, start_));
        struct held_01_
        {
            static void eval(::hgraph::In<"result", ::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TSL<::hgraph::TS<::hgraph::Int>>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Bool>>>> result_, ::hgraph::DateTime now_)
            {
                if (!::hglrt::fn_modified(result_))
                {
                    return;
                }
                ::std::optional<::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>> whole_;
                try
                {
                    whole_.emplace(::hglrt::whole_value(result_));
                }
                catch (const ::hglrt::Failure&)
                {
                }
                ::hglrt::testing::hold<::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>>(now_, ::std::move(whole_));
            }
        };
        const auto result_02_ = ::hgl::demo::nested::m_::w_::retag(wiring_, input_00_);
        ::hgraph::wire<held_01_>(wiring_, result_02_);
        ::hgl::core::m_::w_::i_::record::of_1tuple_1list_1i64_1_1_1bool_1_(wiring_, result_02_, ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_04_ = ::hglrt::testing::Dense<::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>>{::hglrt::testing::Cell<::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>>{::hglrt::testing::Whole<::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>>{::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>{::hglrt::list<::hgraph::Int>{1, 2}, true}}}, ::hglrt::testing::Cell<::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>>{::hglrt::testing::Whole<::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>>{::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>{::hglrt::list<::hgraph::Int>{1, 2, 7}, true}}}, ::hglrt::testing::Cell<::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>>{::hglrt::testing::Whole<::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>>{::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>{::hglrt::list<::hgraph::Int>{}, false}}}};
        ::hglrt::testing::expect_same(left_03_, right_04_);
    }();
}

void a_tuple_of_a_list_replays_its_cells()
{
    ::hglrt::testing::statement();
    [[maybe_unused]] const ::std::tuple<::std::optional<::hglrt::list_delta<::hglrt::list<::hgraph::Int>>>, ::std::optional<::hgraph::Bool>> longer = ::std::tuple<::std::optional<::hglrt::list_delta<::hglrt::list<::hgraph::Int>>>, ::std::optional<::hgraph::Bool>>{::hglrt::list_delta_of<::hglrt::list<::hgraph::Int>>(::hglrt::dict_of<::hgraph::Int, ::hgraph::Int>({{1, 9}}), ::hglrt::set<::hgraph::Int>{}, ::std::nullopt), ::std::nullopt};
    ::hglrt::testing::statement();
    [&] {
        const auto left_03_ = ::hglrt::testing::evaluate<::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TSL<::hgraph::TS<::hgraph::Int>>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Bool>>>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_1tuple_1list_1i64_1_1_1bool_1_(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>>{::hglrt::testing::Cell<::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>>{::hglrt::testing::Whole<::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>>{::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>{::hglrt::list<::hgraph::Int>{1, 2, 3}, true}}}, ::hglrt::testing::Cell<::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>>{longer}, ::hglrt::testing::Cell<::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>>{::hglrt::testing::Whole<::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>>{::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>{::hglrt::list<::hgraph::Int>{}, false}}}}, start_));
        struct held_01_
        {
            static void eval(::hgraph::In<"result", ::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TSL<::hgraph::TS<::hgraph::Int>>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Bool>>>> result_, ::hgraph::DateTime now_)
            {
                if (!::hglrt::fn_modified(result_))
                {
                    return;
                }
                ::std::optional<::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>> whole_;
                try
                {
                    whole_.emplace(::hglrt::whole_value(result_));
                }
                catch (const ::hglrt::Failure&)
                {
                }
                ::hglrt::testing::hold<::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>>(now_, ::std::move(whole_));
            }
        };
        const auto result_02_ = ::hgl::demo::nested::m_::w_::relay_tagged(wiring_, input_00_);
        ::hgraph::wire<held_01_>(wiring_, result_02_);
        ::hgl::core::m_::w_::i_::record::of_1tuple_1list_1i64_1_1_1bool_1_(wiring_, result_02_, ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_04_ = ::hglrt::testing::Dense<::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>>{::hglrt::testing::Cell<::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>>{::hglrt::testing::Whole<::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>>{::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>{::hglrt::list<::hgraph::Int>{1, 2, 3}, true}}}, ::hglrt::testing::Cell<::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>>{::hglrt::testing::Whole<::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>>{::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>{::hglrt::list<::hgraph::Int>{1, 9, 3}, true}}}, ::hglrt::testing::Cell<::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>>{::hglrt::testing::Whole<::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>>{::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Bool>{::hglrt::list<::hgraph::Int>{}, false}}}};
        ::hglrt::testing::expect_same(left_03_, right_04_);
    }();
}

void a_dictionary_of_tuples_is_relayed_whole()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_03_ = ::hglrt::testing::evaluate<::hglrt::dict<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TSD<::hgraph::Str, ::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_1dict_1str_1_1tuple_1i64_1_1str_1_1_(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hglrt::dict<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::testing::Whole<::hglrt::dict<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::dict_of<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>({{::hgraph::Str{"x"}, ::std::tuple<::hgraph::Int, ::hgraph::Str>{1, ::hgraph::Str{"a"}}}})}, ::hglrt::testing::Whole<::hglrt::dict<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::dict_of<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>({{::hgraph::Str{"x"}, ::std::tuple<::hgraph::Int, ::hgraph::Str>{1, ::hgraph::Str{"a"}}}, {::hgraph::Str{"y"}, ::std::tuple<::hgraph::Int, ::hgraph::Str>{2, ::hgraph::Str{"b"}}}})}, ::hglrt::testing::Whole<::hglrt::dict<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::dict_of<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>({{::hgraph::Str{"y"}, ::std::tuple<::hgraph::Int, ::hgraph::Str>{3, ::hgraph::Str{"c"}}}})}}, start_));
        struct held_01_
        {
            static void eval(::hgraph::In<"result", ::hgraph::TSD<::hgraph::Str, ::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>>> result_, ::hgraph::DateTime now_)
            {
                if (!::hglrt::fn_modified(result_))
                {
                    return;
                }
                ::std::optional<::hglrt::dict<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>> whole_;
                try
                {
                    whole_.emplace(::hglrt::keyed_value(result_));
                }
                catch (const ::hglrt::Failure&)
                {
                }
                ::hglrt::testing::hold<::hglrt::dict<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>>(now_, ::std::move(whole_));
            }
        };
        const auto result_02_ = ::hgl::demo::nested::m_::w_::relay_book(wiring_, input_00_);
        ::hgraph::wire<held_01_>(wiring_, result_02_);
        ::hgl::core::m_::w_::i_::record::of_1dict_1str_1_1tuple_1i64_1_1str_1_1_(wiring_, result_02_, ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_04_ = ::hglrt::testing::Dense<::hglrt::dict<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::testing::Whole<::hglrt::dict<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::dict_of<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>({{::hgraph::Str{"x"}, ::std::tuple<::hgraph::Int, ::hgraph::Str>{1, ::hgraph::Str{"a"}}}})}, ::hglrt::testing::Whole<::hglrt::dict<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::dict_of<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>({{::hgraph::Str{"x"}, ::std::tuple<::hgraph::Int, ::hgraph::Str>{1, ::hgraph::Str{"a"}}}, {::hgraph::Str{"y"}, ::std::tuple<::hgraph::Int, ::hgraph::Str>{2, ::hgraph::Str{"b"}}}})}, ::hglrt::testing::Whole<::hglrt::dict<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::dict_of<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>({{::hgraph::Str{"y"}, ::std::tuple<::hgraph::Int, ::hgraph::Str>{3, ::hgraph::Str{"c"}}}})}};
        ::hglrt::testing::expect_same(left_03_, right_04_);
    }();
}

void a_dictionary_of_tuples_is_published_by_a_delta()
{
    ::hglrt::testing::statement();
    [[maybe_unused]] const ::std::tuple<::std::optional<::hgraph::Int>, ::std::optional<::hgraph::Str>> renamed = ::std::tuple<::std::optional<::hgraph::Int>, ::std::optional<::hgraph::Str>>{::std::nullopt, ::hgraph::Str{"c"}};
    ::hglrt::testing::statement();
    [&] {
        const auto left_03_ = ::hglrt::testing::evaluate<::hglrt::dict<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{0, 1, 2}, start_));
        struct held_01_
        {
            static void eval(::hgraph::In<"result", ::hgraph::TSD<::hgraph::Str, ::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>>> result_, ::hgraph::DateTime now_)
            {
                if (!::hglrt::fn_modified(result_))
                {
                    return;
                }
                ::std::optional<::hglrt::dict<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>> whole_;
                try
                {
                    whole_.emplace(::hglrt::keyed_value(result_));
                }
                catch (const ::hglrt::Failure&)
                {
                }
                ::hglrt::testing::hold<::hglrt::dict<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>>(now_, ::std::move(whole_));
            }
        };
        const auto result_02_ = ::hgl::demo::nested::m_::w_::edit_book(wiring_, input_00_);
        ::hgraph::wire<held_01_>(wiring_, result_02_);
        ::hgl::core::m_::w_::i_::record::of_1dict_1str_1_1tuple_1i64_1_1str_1_1_(wiring_, result_02_, ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_04_ = ::hglrt::testing::Dense<::hglrt::dict<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::testing::Whole<::hglrt::dict<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::dict_of<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>({{::hgraph::Str{"x"}, ::std::tuple<::hgraph::Int, ::hgraph::Str>{1, ::hgraph::Str{"a"}}}, {::hgraph::Str{"y"}, ::std::tuple<::hgraph::Int, ::hgraph::Str>{2, ::hgraph::Str{"b"}}}})}, ::hglrt::dict_delta<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>{::hglrt::dict_of<::hgraph::Str, ::std::tuple<::std::optional<::hgraph::Int>, ::std::optional<::hgraph::Str>>>({{::hgraph::Str{"x"}, renamed}}), ::hglrt::set<::hgraph::Str>{}}, ::hglrt::testing::Whole<::hglrt::dict<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::dict_of<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>({{::hgraph::Str{"x"}, ::std::tuple<::hgraph::Int, ::hgraph::Str>{1, ::hgraph::Str{"c"}}}})}};
        ::hglrt::testing::expect_same(left_03_, right_04_);
    }();
}

void a_dictionary_of_tuples_replays_its_deltas()
{
    ::hglrt::testing::statement();
    [[maybe_unused]] const ::std::tuple<::std::optional<::hgraph::Int>, ::std::optional<::hgraph::Str>> renamed = ::std::tuple<::std::optional<::hgraph::Int>, ::std::optional<::hgraph::Str>>{::std::nullopt, ::hgraph::Str{"c"}};
    ::hglrt::testing::statement();
    [&] {
        const auto left_03_ = ::hglrt::testing::evaluate<::hglrt::dict<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TSD<::hgraph::Str, ::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_1dict_1str_1_1tuple_1i64_1_1str_1_1_(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hglrt::dict<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::testing::Whole<::hglrt::dict<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::dict_of<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>({{::hgraph::Str{"x"}, ::std::tuple<::hgraph::Int, ::hgraph::Str>{1, ::hgraph::Str{"a"}}}, {::hgraph::Str{"y"}, ::std::tuple<::hgraph::Int, ::hgraph::Str>{2, ::hgraph::Str{"b"}}}})}, ::hglrt::dict_delta<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>{::hglrt::dict_of<::hgraph::Str, ::std::tuple<::std::optional<::hgraph::Int>, ::std::optional<::hgraph::Str>>>({{::hgraph::Str{"x"}, renamed}}), ::hglrt::set<::hgraph::Str>{::hgraph::Str{"y"}}}}, start_));
        struct held_01_
        {
            static void eval(::hgraph::In<"result", ::hgraph::TSD<::hgraph::Str, ::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>>> result_, ::hgraph::DateTime now_)
            {
                if (!::hglrt::fn_modified(result_))
                {
                    return;
                }
                ::std::optional<::hglrt::dict<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>> whole_;
                try
                {
                    whole_.emplace(::hglrt::keyed_value(result_));
                }
                catch (const ::hglrt::Failure&)
                {
                }
                ::hglrt::testing::hold<::hglrt::dict<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>>(now_, ::std::move(whole_));
            }
        };
        const auto result_02_ = ::hgl::demo::nested::m_::w_::relay_book(wiring_, input_00_);
        ::hgraph::wire<held_01_>(wiring_, result_02_);
        ::hgl::core::m_::w_::i_::record::of_1dict_1str_1_1tuple_1i64_1_1str_1_1_(wiring_, result_02_, ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_04_ = ::hglrt::testing::Dense<::hglrt::dict<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::testing::Whole<::hglrt::dict<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::dict_of<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>({{::hgraph::Str{"x"}, ::std::tuple<::hgraph::Int, ::hgraph::Str>{1, ::hgraph::Str{"a"}}}, {::hgraph::Str{"y"}, ::std::tuple<::hgraph::Int, ::hgraph::Str>{2, ::hgraph::Str{"b"}}}})}, ::hglrt::testing::Whole<::hglrt::dict<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::dict_of<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>({{::hgraph::Str{"x"}, ::std::tuple<::hgraph::Int, ::hgraph::Str>{1, ::hgraph::Str{"c"}}}})}};
        ::hglrt::testing::expect_same(left_03_, right_04_);
    }();
}

void a_struct_with_a_list_field_is_relayed_whole()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_03_ = ::hglrt::testing::evaluate<::hgl::demo::nested::m_::Basket>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::NominalTSB<::hgl::demo::nested::m_::Basket, ::hgraph::Field<"name", ::hgraph::TS<::hgraph::Str>>, ::hgraph::Field<"counts", ::hgraph::TSL<::hgraph::TS<::hgraph::Int>>>>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_1demo_1nested_1_1Basket_(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgl::demo::nested::m_::Basket>{::hglrt::testing::Cell<::hgl::demo::nested::m_::Basket>{::hglrt::testing::Whole<::hgl::demo::nested::m_::Basket>{::hgl::demo::nested::m_::Basket{::hgraph::Str{"b"}, ::hglrt::list<::hgraph::Int>{1, 2}}}}, ::hglrt::testing::Cell<::hgl::demo::nested::m_::Basket>{::hglrt::testing::Whole<::hgl::demo::nested::m_::Basket>{::hgl::demo::nested::m_::Basket{::hgraph::Str{"c"}, ::hglrt::list<::hgraph::Int>{}}}}}, start_));
        struct held_01_
        {
            static void eval(::hgraph::In<"result", ::hgraph::NominalTSB<::hgl::demo::nested::m_::Basket, ::hgraph::Field<"name", ::hgraph::TS<::hgraph::Str>>, ::hgraph::Field<"counts", ::hgraph::TSL<::hgraph::TS<::hgraph::Int>>>>> result_, ::hgraph::DateTime now_)
            {
                if (!::hglrt::fn_modified(result_))
                {
                    return;
                }
                ::std::optional<::hgl::demo::nested::m_::Basket> whole_;
                try
                {
                    whole_.emplace(::hglrt::whole_value(result_));
                }
                catch (const ::hglrt::Failure&)
                {
                }
                ::hglrt::testing::hold<::hgl::demo::nested::m_::Basket>(now_, ::std::move(whole_));
            }
        };
        const auto result_02_ = ::hgl::demo::nested::m_::w_::relay_basket(wiring_, input_00_);
        ::hgraph::wire<held_01_>(wiring_, result_02_);
        ::hgl::core::m_::w_::i_::record::of_1demo_1nested_1_1Basket_(wiring_, result_02_, ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_04_ = ::hglrt::testing::Dense<::hgl::demo::nested::m_::Basket>{::hglrt::testing::Cell<::hgl::demo::nested::m_::Basket>{::hglrt::testing::Whole<::hgl::demo::nested::m_::Basket>{::hgl::demo::nested::m_::Basket{::hgraph::Str{"b"}, ::hglrt::list<::hgraph::Int>{1, 2}}}}, ::hglrt::testing::Cell<::hgl::demo::nested::m_::Basket>{::hglrt::testing::Whole<::hgl::demo::nested::m_::Basket>{::hgl::demo::nested::m_::Basket{::hgraph::Str{"c"}, ::hglrt::list<::hgraph::Int>{}}}}};
        ::hglrt::testing::expect_same(left_03_, right_04_);
    }();
}

void a_struct_with_a_list_field_is_published_by_a_delta()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_03_ = ::hglrt::testing::evaluate<::hgl::demo::nested::m_::Basket>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{0, 1, 2}, start_));
        struct held_01_
        {
            static void eval(::hgraph::In<"result", ::hgraph::NominalTSB<::hgl::demo::nested::m_::Basket, ::hgraph::Field<"name", ::hgraph::TS<::hgraph::Str>>, ::hgraph::Field<"counts", ::hgraph::TSL<::hgraph::TS<::hgraph::Int>>>>> result_, ::hgraph::DateTime now_)
            {
                if (!::hglrt::fn_modified(result_))
                {
                    return;
                }
                ::std::optional<::hgl::demo::nested::m_::Basket> whole_;
                try
                {
                    whole_.emplace(::hglrt::whole_value(result_));
                }
                catch (const ::hglrt::Failure&)
                {
                }
                ::hglrt::testing::hold<::hgl::demo::nested::m_::Basket>(now_, ::std::move(whole_));
            }
        };
        const auto result_02_ = ::hgl::demo::nested::m_::w_::fill(wiring_, input_00_);
        ::hgraph::wire<held_01_>(wiring_, result_02_);
        ::hgl::core::m_::w_::i_::record::of_1demo_1nested_1_1Basket_(wiring_, result_02_, ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_04_ = ::hglrt::testing::Dense<::hgl::demo::nested::m_::Basket>{::hglrt::testing::Cell<::hgl::demo::nested::m_::Basket>{::hglrt::testing::Whole<::hgl::demo::nested::m_::Basket>{::hgl::demo::nested::m_::Basket{::hgraph::Str{"b"}, ::hglrt::list<::hgraph::Int>{1, 2}}}}, ::hglrt::testing::Cell<::hgl::demo::nested::m_::Basket>{::hgl::demo::nested::m_::d_::Basket{::std::nullopt, ::hglrt::list_delta_of<::hglrt::list<::hgraph::Int>>(::hglrt::dict_of<::hgraph::Int, ::hgraph::Int>({{2, 3}}), ::hglrt::set<::hgraph::Int>{}, 3)}}, ::hglrt::testing::Cell<::hgl::demo::nested::m_::Basket>{::hglrt::testing::Whole<::hgl::demo::nested::m_::Basket>{::hgl::demo::nested::m_::Basket{::hgraph::Str{"b"}, ::hglrt::list<::hgraph::Int>{}}}}};
        ::hglrt::testing::expect_same(left_03_, right_04_);
    }();
}

void a_struct_with_a_list_field_replays_its_cells()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::NominalTSB<::hgl::demo::nested::m_::Basket, ::hgraph::Field<"name", ::hgraph::TS<::hgraph::Str>>, ::hgraph::Field<"counts", ::hgraph::TSL<::hgraph::TS<::hgraph::Int>>>>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_1demo_1nested_1_1Basket_(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgl::demo::nested::m_::Basket>{::hglrt::testing::Cell<::hgl::demo::nested::m_::Basket>{::hglrt::testing::Whole<::hgl::demo::nested::m_::Basket>{::hgl::demo::nested::m_::Basket{::hgraph::Str{"b"}, ::hglrt::list<::hgraph::Int>{1, 2}}}}, ::hglrt::testing::Cell<::hgl::demo::nested::m_::Basket>{::hgl::demo::nested::m_::d_::Basket{::std::nullopt, ::hglrt::list_delta_of<::hglrt::list<::hgraph::Int>>(::hglrt::dict_of<::hgraph::Int, ::hgraph::Int>({{0, 5}}), ::hglrt::set<::hgraph::Int>{}, ::std::nullopt)}}, ::hglrt::testing::Cell<::hgl::demo::nested::m_::Basket>{::hglrt::testing::Whole<::hgl::demo::nested::m_::Basket>{::hgl::demo::nested::m_::Basket{::hgraph::Str{"b"}, ::hglrt::list<::hgraph::Int>{4}}}}}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::nested::m_::w_::summed(wiring_, input_00_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgraph::Int>{3, 7, 4};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
}

void a_child_of_a_list_of_tuples_is_wired()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_03_ = ::hglrt::testing::evaluate<::std::tuple<::hgraph::Int, ::hgraph::Str>>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TSL<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_1list_1tuple_1i64_1_1str_1_1_(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::testing::Cell<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::testing::Whole<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>{::std::tuple<::hgraph::Int, ::hgraph::Str>{1, ::hgraph::Str{"a"}}}}}, ::hglrt::testing::Cell<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::testing::Whole<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>{::std::tuple<::hgraph::Int, ::hgraph::Str>{1, ::hgraph::Str{"a"}}, ::std::tuple<::hgraph::Int, ::hgraph::Str>{2, ::hgraph::Str{"b"}}}}}, ::hglrt::testing::Cell<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::testing::Whole<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>{::std::tuple<::hgraph::Int, ::hgraph::Str>{3, ::hgraph::Str{"c"}}}}}}, start_));
        struct held_01_
        {
            static void eval(::hgraph::In<"result", ::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>> result_, ::hgraph::DateTime now_)
            {
                if (!::hglrt::fn_modified(result_))
                {
                    return;
                }
                ::std::optional<::std::tuple<::hgraph::Int, ::hgraph::Str>> whole_;
                try
                {
                    whole_.emplace(::hglrt::whole_value(result_));
                }
                catch (const ::hglrt::Failure&)
                {
                }
                ::hglrt::testing::hold<::std::tuple<::hgraph::Int, ::hgraph::Str>>(now_, ::std::move(whole_));
            }
        };
        const auto result_02_ = ::hgl::demo::nested::m_::w_::first_pair(wiring_, input_00_);
        ::hgraph::wire<held_01_>(wiring_, result_02_);
        ::hgl::core::m_::w_::i_::record::of_1tuple_1i64_1_1str_1_(wiring_, result_02_, ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_04_ = ::hglrt::testing::Dense<::std::tuple<::hgraph::Int, ::hgraph::Str>>{::hglrt::testing::Cell<::std::tuple<::hgraph::Int, ::hgraph::Str>>{::hglrt::testing::Whole<::std::tuple<::hgraph::Int, ::hgraph::Str>>{::std::tuple<::hgraph::Int, ::hgraph::Str>{1, ::hgraph::Str{"a"}}}}, ::hglrt::testing::Cell<::std::tuple<::hgraph::Int, ::hgraph::Str>>{::hglrt::testing::Whole<::std::tuple<::hgraph::Int, ::hgraph::Str>>{::std::tuple<::hgraph::Int, ::hgraph::Str>{1, ::hgraph::Str{"a"}}}}, ::hglrt::testing::Cell<::std::tuple<::hgraph::Int, ::hgraph::Str>>{::hglrt::testing::Whole<::std::tuple<::hgraph::Int, ::hgraph::Str>>{::std::tuple<::hgraph::Int, ::hgraph::Str>{3, ::hgraph::Str{"c"}}}}};
        ::hglrt::testing::expect_same(left_03_, right_04_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_08_ = ::hglrt::testing::evaluate<::std::tuple<::hgraph::Int, ::hgraph::Str>>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TSL<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>>> input_05_ = ::hgl::core::m_::w_::i_::replay::of_1list_1tuple_1i64_1_1str_1_1_(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::testing::Cell<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::testing::Whole<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>{::std::tuple<::hgraph::Int, ::hgraph::Str>{1, ::hgraph::Str{"a"}}}}}, ::hglrt::testing::Cell<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::testing::Whole<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>{::std::tuple<::hgraph::Int, ::hgraph::Str>{1, ::hgraph::Str{"a"}}, ::std::tuple<::hgraph::Int, ::hgraph::Str>{2, ::hgraph::Str{"b"}}}}}, ::hglrt::testing::Cell<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::testing::Whole<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>{::std::tuple<::hgraph::Int, ::hgraph::Str>{3, ::hgraph::Str{"c"}}}}}, ::hglrt::testing::Cell<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::testing::Whole<::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Str>>{::std::tuple<::hgraph::Int, ::hgraph::Str>{4, ::hgraph::Str{"d"}}, ::std::tuple<::hgraph::Int, ::hgraph::Str>{5, ::hgraph::Str{"e"}}}}}}, start_));
        struct held_06_
        {
            static void eval(::hgraph::In<"result", ::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>> result_, ::hgraph::DateTime now_)
            {
                if (!::hglrt::fn_modified(result_))
                {
                    return;
                }
                ::std::optional<::std::tuple<::hgraph::Int, ::hgraph::Str>> whole_;
                try
                {
                    whole_.emplace(::hglrt::whole_value(result_));
                }
                catch (const ::hglrt::Failure&)
                {
                }
                ::hglrt::testing::hold<::std::tuple<::hgraph::Int, ::hgraph::Str>>(now_, ::std::move(whole_));
            }
        };
        const auto result_07_ = ::hgl::demo::nested::m_::w_::second_relayed(wiring_, input_05_);
        ::hgraph::wire<held_06_>(wiring_, result_07_);
        ::hgl::core::m_::w_::i_::record::of_1tuple_1i64_1_1str_1_(wiring_, result_07_, ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_09_ = ::hglrt::testing::Dense<::std::tuple<::hgraph::Int, ::hgraph::Str>>{::std::nullopt, ::hglrt::testing::Cell<::std::tuple<::hgraph::Int, ::hgraph::Str>>{::hglrt::testing::Whole<::std::tuple<::hgraph::Int, ::hgraph::Str>>{::std::tuple<::hgraph::Int, ::hgraph::Str>{2, ::hgraph::Str{"b"}}}}, ::std::nullopt, ::hglrt::testing::Cell<::std::tuple<::hgraph::Int, ::hgraph::Str>>{::hglrt::testing::Whole<::std::tuple<::hgraph::Int, ::hgraph::Str>>{::std::tuple<::hgraph::Int, ::hgraph::Str>{5, ::hgraph::Str{"e"}}}}};
        ::hglrt::testing::expect_same(left_08_, right_09_);
    }();
}

void a_key_child_of_a_dictionary_of_tuples_is_wired()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_03_ = ::hglrt::testing::evaluate<::std::tuple<::hgraph::Int, ::hgraph::Str>>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TSD<::hgraph::Str, ::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_1dict_1str_1_1tuple_1i64_1_1str_1_1_(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hglrt::dict<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::testing::Whole<::hglrt::dict<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::dict_of<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>({{::hgraph::Str{"x"}, ::std::tuple<::hgraph::Int, ::hgraph::Str>{1, ::hgraph::Str{"a"}}}})}, ::hglrt::testing::Whole<::hglrt::dict<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::dict_of<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>({{::hgraph::Str{"y"}, ::std::tuple<::hgraph::Int, ::hgraph::Str>{2, ::hgraph::Str{"b"}}}})}, ::hglrt::testing::Whole<::hglrt::dict<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>>{::hglrt::dict_of<::hgraph::Str, ::std::tuple<::hgraph::Int, ::hgraph::Str>>({{::hgraph::Str{"x"}, ::std::tuple<::hgraph::Int, ::hgraph::Str>{3, ::hgraph::Str{"c"}}}})}}, start_));
        struct held_01_
        {
            static void eval(::hgraph::In<"result", ::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>> result_, ::hgraph::DateTime now_)
            {
                if (!::hglrt::fn_modified(result_))
                {
                    return;
                }
                ::std::optional<::std::tuple<::hgraph::Int, ::hgraph::Str>> whole_;
                try
                {
                    whole_.emplace(::hglrt::whole_value(result_));
                }
                catch (const ::hglrt::Failure&)
                {
                }
                ::hglrt::testing::hold<::std::tuple<::hgraph::Int, ::hgraph::Str>>(now_, ::std::move(whole_));
            }
        };
        const auto result_02_ = ::hgl::demo::nested::m_::w_::entry_x(wiring_, input_00_);
        ::hgraph::wire<held_01_>(wiring_, result_02_);
        ::hgl::core::m_::w_::i_::record::of_1tuple_1i64_1_1str_1_(wiring_, result_02_, ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_04_ = ::hglrt::testing::Dense<::std::tuple<::hgraph::Int, ::hgraph::Str>>{::hglrt::testing::Cell<::std::tuple<::hgraph::Int, ::hgraph::Str>>{::hglrt::testing::Whole<::std::tuple<::hgraph::Int, ::hgraph::Str>>{::std::tuple<::hgraph::Int, ::hgraph::Str>{1, ::hgraph::Str{"a"}}}}, ::std::nullopt, ::hglrt::testing::Cell<::std::tuple<::hgraph::Int, ::hgraph::Str>>{::hglrt::testing::Whole<::std::tuple<::hgraph::Int, ::hgraph::Str>>{::std::tuple<::hgraph::Int, ::hgraph::Str>{3, ::hgraph::Str{"c"}}}}};
        ::hglrt::testing::expect_same(left_03_, right_04_);
    }();
}

void a_struct_with_a_list_field_is_a_configuration()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_05_ = [&]() {
        const ::hgl::demo::nested::m_::Basket argument_00_ = ::hgl::demo::nested::m_::Basket{::hgraph::Str{"b"}, ::hglrt::list<::hgraph::Int>{1, 2, 3}};
        const auto trace_01_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 2};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_02_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_01_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const ::hgl::demo::nested::m_::Basket argument_03_ = argument_00_; const auto argument_04_ = input_02_; return ::hgl::demo::nested::m_::w_::counted(wiring_, argument_03_, argument_04_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_06_ = ::hglrt::testing::Dense<::hgraph::Int>{7, 8};
        ::hglrt::testing::expect_same(left_05_, right_06_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_012_ = [&]() {
        const ::hglrt::list<::hgl::demo::nested::m_::Basket> argument_07_ = ::hglrt::list<::hgl::demo::nested::m_::Basket>{::hgl::demo::nested::m_::Basket{::hgraph::Str{"b"}, ::hglrt::list<::hgraph::Int>{1}}, ::hgl::demo::nested::m_::Basket{::hgraph::Str{"c"}, ::hglrt::list<::hgraph::Int>{4}}};
        const auto trace_08_ = ::hglrt::testing::Dense<::hgraph::Int>{1};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_09_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_08_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const ::hglrt::list<::hgl::demo::nested::m_::Basket> argument_010_ = argument_07_; const auto argument_011_ = input_09_; return ::hgl::demo::nested::m_::w_::counted_all(wiring_, argument_010_, argument_011_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_013_ = ::hglrt::testing::Dense<::hgraph::Int>{6};
        ::hglrt::testing::expect_same(left_012_, right_013_);
    }();
}

}  // namespace hgl::demo::nested::m_::t_
