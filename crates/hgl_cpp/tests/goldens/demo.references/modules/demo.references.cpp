// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include "demo.references.h"
#include "core.h"

// Names this file writes, which no macro of a header above takes.
#undef a
#undef a_designated_input_is_followed_through_its_rebinding
#undef a_designated_position_is_followed_through_its_rebinding
#undef a_designated_position_outside_the_list_fails
#undef a_reader_observes_the_input_designated
#undef a_ref_parameter_is_present_and_wakes_once
#undef a_relay_keeps_what_it_sampled
#undef a_selected_entry_is_followed_through_its_key
#undef a_selected_entry_the_dictionary_lacks_is_not_valid
#undef an_entry_at_a_value_key_is_followed
#undef an_eval_records_a_ref_result_as_a_reader_does
#undef b
#undef changed_key
#undef choose
#undef chosen
#undef core
#undef demo
#undef entry_a
#undef flag
#undef follow
#undef high
#undef high_slot
#undef idx
#undef item
#undef items
#undef key
#undef low
#undef low_slot
#undef modified
#undef pick
#undef pick_list
#undef picked
#undef present
#undef present_item
#undef price
#undef probe
#undef reading
#undef record
#undef references
#undef replay
#undef route
#undef routed
#undef sample_picked
#undef sampled
#undef sel
#undef selected
#undef selected_present
#undef set_item
#undef slot
#undef take
#undef target
#undef wakes
#undef write_entry

namespace hgl::demo::references::m_
{

namespace n_::pick
{
// pick(flag: bool, ref low: i64, ref high: i64) -> ref i64 (node), module demo.references, line 9
struct impl_
{
    static constexpr auto name = "demo.references::pick";

    static void eval([[maybe_unused]] ::hgraph::In<"flag", ::hgraph::TS<::hgraph::Bool>> flag, [[maybe_unused]] ::hgraph::In<"low", ::hgraph::REF<::hgraph::TS<::hgraph::Int>>, ::hgraph::InputActivity::Passive> low, [[maybe_unused]] ::hgraph::In<"high", ::hgraph::REF<::hgraph::TS<::hgraph::Int>>, ::hgraph::InputActivity::Passive> high, [[maybe_unused]] ::hgraph::Out<::hgraph::REF<::hgraph::TS<::hgraph::Int>>> out_)
    {
        // when, line 10 (TIME-3)
        if (::hglrt::fn_modified(flag))
        {
            if (::hglrt::checked_value(flag))
            {
                out_.set(::hglrt::designation(high));
                return;
            }
            out_.set(::hglrt::designation(low));
            return;
        }
    }
};
}  // namespace n_::pick
namespace w_
{
// pick(flag: bool, ref low: i64, ref high: i64) -> ref i64 (node), module demo.references, line 9
::hgraph::Port<::hgraph::TS<::hgraph::Int>> pick(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> flag, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> low, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> high)
{
    return ::hgraph::wire<::hgl::demo::references::m_::n_::pick::impl_>(wiring_, flag, low, high).as<::hgraph::TS<::hgraph::Int>>();
}
}  // namespace w_

namespace n_::follow
{
// follow(item: i64) -> i64 (node), module demo.references, line 17
struct impl_
{
    static constexpr auto name = "demo.references::follow";

    static void eval([[maybe_unused]] ::hgraph::In<"item", ::hgraph::TS<::hgraph::Int>> item, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 18 (TIME-3)
        {
            out_.set(::hglrt::checked_value(item));
            return;
        }
    }
};
}  // namespace n_::follow
namespace w_
{
// follow(item: i64) -> i64 (node), module demo.references, line 17
::hgraph::Port<::hgraph::TS<::hgraph::Int>> follow(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> item)
{
    return ::hgraph::wire<::hgl::demo::references::m_::n_::follow::impl_>(wiring_, item);
}
}  // namespace w_

namespace n_::picked
{
// picked(flag: bool, low: i64, high: i64) -> i64 (graph), module demo.references, line 21
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> flag, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> low, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> high)
    {
        return ::hgl::demo::references::m_::w_::follow(wiring_, ::hgl::demo::references::m_::w_::pick(wiring_, flag, low, high));
    }
};
}  // namespace n_::picked
namespace w_
{
// picked(flag: bool, low: i64, high: i64) -> i64 (graph), module demo.references, line 21
::hgraph::Port<::hgraph::TS<::hgraph::Int>> picked(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> flag, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> low, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> high)
{
    return ::hgl::demo::references::m_::n_::picked::impl_::compose(wiring_, flag, low, high);
}
}  // namespace w_

namespace n_::present
{
// present(probe: bool, ref target: i64) -> i64 (node), module demo.references, line 37
struct impl_
{
    static constexpr auto name = "demo.references::present";

    // The node's `state` and `cache` (TIME-6).
    struct state_type_
    {
        ::hgraph::Int wakes{};
    };

    static void start([[maybe_unused]] ::hgraph::State<state_type_> state_)
    {
        state_.set(state_type_{});
        state_type_& kept_ = state_.modify();
        [[maybe_unused]] auto& wakes = kept_.wakes;
        wakes = 0;
    }

    static void eval([[maybe_unused]] ::hgraph::In<"probe", ::hgraph::TS<::hgraph::Bool>> probe, [[maybe_unused]] ::hgraph::In<"target", ::hgraph::REF<::hgraph::TS<::hgraph::Int>>, ::hgraph::InputValidity::Unchecked> target, [[maybe_unused]] ::hgraph::State<state_type_> state_, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        state_type_& kept_ = state_.modify();
        [[maybe_unused]] auto& wakes = kept_.wakes;
        // when, line 39 (TIME-3)
        if (::hglrt::fn_valid(target) && ::hglrt::fn_modified(target))
        {
            wakes = ::hglrt::add__i64_i64(wakes, 1);
        }
        // when, line 40 (TIME-3)
        if (::hglrt::fn_modified(probe))
        {
            if (::hglrt::fn_valid(target))
            {
                out_.set(wakes);
                return;
            }
            out_.set(::hglrt::neg__i64(1));
            return;
        }
    }
};
}  // namespace n_::present
}  // namespace hgl::demo::references::m_

// The state of the node, as hgraph holds it (TIME-6).
template <>
struct hgraph::scalar_descriptor<::hgl::demo::references::m_::n_::present::impl_::state_type_>
{
    [[nodiscard]] static constexpr bool is_concrete() noexcept { return true; }
    [[nodiscard]] static const ValueTypeMetaData* value_meta()
    {
        return TypeRegistry::instance().register_scalar<::hgl::demo::references::m_::n_::present::impl_::state_type_>("hgl:demo.references::present.state");
    }
};

namespace hgl::demo::references::m_
{
namespace w_
{
// present(probe: bool, ref target: i64) -> i64 (node), module demo.references, line 37
::hgraph::Port<::hgraph::TS<::hgraph::Int>> present(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> target)
{
    return ::hgraph::wire<::hgl::demo::references::m_::n_::present::impl_>(wiring_, probe, target);
}
}  // namespace w_

namespace n_::sample_picked
{
// sample_picked(take: bool, flag: bool, low: i64, high: i64) -> i64 (graph), module demo.references, line 55
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> take, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> flag, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> low, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> high)
    {
        return ::hgl::demo::references::m_::w_::follow(wiring_, ::hgl::demo::references::m_::w_::i_::sampled::of_i64(wiring_, take, ::hgl::demo::references::m_::w_::pick(wiring_, flag, low, high)));
    }
};
}  // namespace n_::sample_picked
namespace w_
{
// sample_picked(take: bool, flag: bool, low: i64, high: i64) -> i64 (graph), module demo.references, line 55
::hgraph::Port<::hgraph::TS<::hgraph::Int>> sample_picked(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> take, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> flag, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> low, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> high)
{
    return ::hgl::demo::references::m_::n_::sample_picked::impl_::compose(wiring_, take, flag, low, high);
}
}  // namespace w_

namespace n_::choose
{
// choose(flag: i64, low: i64, high: i64) -> ref i64 (node), module demo.references, line 70
struct impl_
{
    static constexpr auto name = "demo.references::choose";

    // The node's `state` and `cache` (TIME-6).
    struct state_type_
    {
        // FN-16: the input or position the `ref` result follows, by its
        // place among those it designates, or -1 for none.
        ::hgraph::Int designates_{-1};
    };

    static void start([[maybe_unused]] ::hgraph::State<state_type_> state_)
    {
        state_.set(state_type_{});
    }

    static void eval([[maybe_unused]] ::hgraph::In<"flag", ::hgraph::TS<::hgraph::Int>, ::hgraph::InputValidity::Unchecked> flag, [[maybe_unused]] ::hgraph::In<"low", ::hgraph::TS<::hgraph::Int>, ::hgraph::InputActivity::Passive, ::hgraph::InputValidity::Unchecked> low, [[maybe_unused]] ::hgraph::In<"high", ::hgraph::TS<::hgraph::Int>, ::hgraph::InputActivity::Passive, ::hgraph::InputValidity::Unchecked> high, [[maybe_unused]] ::hgraph::In<"designated:low", ::hgraph::REF<::hgraph::TS<::hgraph::Int>>, ::hgraph::InputValidity::Unchecked> designated_01_, [[maybe_unused]] ::hgraph::In<"designated:high", ::hgraph::REF<::hgraph::TS<::hgraph::Int>>, ::hgraph::InputValidity::Unchecked> designated_02_, [[maybe_unused]] ::hgraph::State<state_type_> state_, [[maybe_unused]] ::hgraph::Out<::hgraph::REF<::hgraph::TS<::hgraph::Int>>> out_)
    {
        // FN-16: the input the `ref` result follows, kept in the node's state.
        ::hgraph::Int& designates_ = state_.modify().designates_;
        // FN-16: "Readers follow a later rebinding of that endpoint without
        // waking the node for it".
        if (designates_ == 0 && designated_01_.modified())
        {
            out_.set(::hglrt::designation(designated_01_));
        }
        if (designates_ == 1 && designated_02_.modified())
        {
            out_.set(::hglrt::designation(designated_02_));
        }
        if (!(flag.modified() || flag.invalidated()))
        {
            return;
        }
        // when, line 72 (TIME-3)
        if (::hglrt::fn_valid(flag) && ::hglrt::fn_valid(low) && ::hglrt::fn_valid(high) && ::hglrt::fn_modified(flag))
        {
            if (::hglrt::eq__i64_i64_f64_f64(::hglrt::checked_value(flag), 1, 0.0, 0.0))
            {
                designates_ = 0;
                out_.set(::hglrt::designation(designated_01_));
            }
            if (::hglrt::eq__i64_i64_f64_f64(::hglrt::checked_value(flag), 2, 0.0, 0.0))
            {
                designates_ = 1;
                out_.set(::hglrt::designation(designated_02_));
                return;
            }
            if (::hglrt::eq__i64_i64_f64_f64(::hglrt::checked_value(flag), 0, 0.0, 0.0))
            {
                designates_ = -1;
                ::hglrt::fn_invalidate(out_);
            }
        }
    }
};
}  // namespace n_::choose
}  // namespace hgl::demo::references::m_

// The state of the node, as hgraph holds it (TIME-6).
template <>
struct hgraph::scalar_descriptor<::hgl::demo::references::m_::n_::choose::impl_::state_type_>
{
    [[nodiscard]] static constexpr bool is_concrete() noexcept { return true; }
    [[nodiscard]] static const ValueTypeMetaData* value_meta()
    {
        return TypeRegistry::instance().register_scalar<::hgl::demo::references::m_::n_::choose::impl_::state_type_>("hgl:demo.references::choose.state");
    }
};

namespace hgl::demo::references::m_
{
namespace w_
{
// choose(flag: i64, low: i64, high: i64) -> ref i64 (node), module demo.references, line 70
::hgraph::Port<::hgraph::TS<::hgraph::Int>> choose(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> flag, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> low, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> high)
{
    return ::hgraph::wire<::hgl::demo::references::m_::n_::choose::impl_>(wiring_, flag, low, high, low, high).as<::hgraph::TS<::hgraph::Int>>();
}
}  // namespace w_

namespace n_::chosen
{
// chosen(flag: i64, sel: bool, a: i64, b: i64, high: i64) -> i64 (graph), module demo.references, line 79
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> flag, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> sel, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> a, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> b, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> high)
    {
        return ::hgl::demo::references::m_::w_::follow(wiring_, ::hgl::demo::references::m_::w_::choose(wiring_, flag, ::hgl::demo::references::m_::w_::pick(wiring_, sel, a, b), high));
    }
};
}  // namespace n_::chosen
namespace w_
{
// chosen(flag: i64, sel: bool, a: i64, b: i64, high: i64) -> i64 (graph), module demo.references, line 79
::hgraph::Port<::hgraph::TS<::hgraph::Int>> chosen(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> flag, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> sel, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> a, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> b, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> high)
{
    return ::hgl::demo::references::m_::n_::chosen::impl_::compose(wiring_, flag, sel, a, b, high);
}
}  // namespace w_

namespace n_::route
{
// route(idx: i64, items: list<i64, 2>) -> ref i64 (node), module demo.references, line 95
struct impl_
{
    static constexpr auto name = "demo.references::route";

    // The node's `state` and `cache` (TIME-6).
    struct state_type_
    {
        // FN-16: the input or position the `ref` result follows, by its
        // place among those it designates, or -1 for none.
        ::hgraph::Int designates_{-1};
    };

    static void start([[maybe_unused]] ::hgraph::State<state_type_> state_)
    {
        state_.set(state_type_{});
    }

    static void eval([[maybe_unused]] ::hgraph::In<"idx", ::hgraph::TS<::hgraph::Int>, ::hgraph::InputValidity::Unchecked> idx, [[maybe_unused]] ::hgraph::In<"items", ::hgraph::TSL<::hgraph::TS<::hgraph::Int>, 2>, ::hgraph::InputActivity::Passive, ::hgraph::InputValidity::Unchecked> items, [[maybe_unused]] ::hgraph::In<"designated:items[0]", ::hgraph::REF<::hgraph::TS<::hgraph::Int>>, ::hgraph::InputValidity::Unchecked> designated_01_0_, [[maybe_unused]] ::hgraph::In<"designated:items[1]", ::hgraph::REF<::hgraph::TS<::hgraph::Int>>, ::hgraph::InputValidity::Unchecked> designated_01_1_, [[maybe_unused]] ::hgraph::State<state_type_> state_, [[maybe_unused]] ::hgraph::Out<::hgraph::REF<::hgraph::TS<::hgraph::Int>>> out_)
    {
        // FN-16: the input the `ref` result follows, kept in the node's state.
        ::hgraph::Int& designates_ = state_.modify().designates_;
        // FN-16: "Readers follow a later rebinding of that endpoint without
        // waking the node for it".
        if (designates_ == 0 && designated_01_0_.modified())
        {
            out_.set(::hglrt::designation(designated_01_0_));
        }
        if (designates_ == 1 && designated_01_1_.modified())
        {
            out_.set(::hglrt::designation(designated_01_1_));
        }
        if (!(idx.modified() || idx.invalidated()))
        {
            return;
        }
        // when, line 96 (TIME-3)
        if (::hglrt::fn_valid(idx) && ::hglrt::fn_valid(items) && ::hglrt::fn_modified(idx))
        {
            const ::hgraph::Int position_00_ = ::hglrt::designated_position(::hglrt::checked_value(idx), 2);
            designates_ = 0 + position_00_;
            out_.set(::hglrt::designation_of(position_00_, designated_01_0_, designated_01_1_));
            return;
        }
    }
};
}  // namespace n_::route
}  // namespace hgl::demo::references::m_

// The state of the node, as hgraph holds it (TIME-6).
template <>
struct hgraph::scalar_descriptor<::hgl::demo::references::m_::n_::route::impl_::state_type_>
{
    [[nodiscard]] static constexpr bool is_concrete() noexcept { return true; }
    [[nodiscard]] static const ValueTypeMetaData* value_meta()
    {
        return TypeRegistry::instance().register_scalar<::hgl::demo::references::m_::n_::route::impl_::state_type_>("hgl:demo.references::route.state");
    }
};

namespace hgl::demo::references::m_
{
namespace w_
{
// route(idx: i64, items: list<i64, 2>) -> ref i64 (node), module demo.references, line 95
::hgraph::Port<::hgraph::TS<::hgraph::Int>> route(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> idx, ::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>, 2>> items)
{
    return ::hgraph::wire<::hgl::demo::references::m_::n_::route::impl_>(wiring_, idx, items, ::hglrt::element_port(items, 0), ::hglrt::element_port(items, 1)).as<::hgraph::TS<::hgraph::Int>>();
}
}  // namespace w_

namespace n_::set_item
{
// set_item(slot: i64, reading: i64) -> list<i64, 2> (node), module demo.references, line 99
struct impl_
{
    static constexpr auto name = "demo.references::set_item";

    static void eval([[maybe_unused]] ::hgraph::In<"slot", ::hgraph::TS<::hgraph::Int>> slot, [[maybe_unused]] ::hgraph::In<"reading", ::hgraph::TS<::hgraph::Int>> reading, [[maybe_unused]] ::hgraph::Out<::hgraph::TSL<::hgraph::TS<::hgraph::Int>, 2>> out_)
    {
        // when, line 100 (TIME-3)
        {
            ::hglrt::publish(out_, ::hglrt::list_delta_of<::std::array<::hgraph::Int, 2>>(::hglrt::dict_of<::hgraph::Int, ::hgraph::Int>({{::hglrt::checked_value(slot), ::hglrt::checked_value(reading)}}), ::hglrt::set<::hgraph::Int>{}, ::std::nullopt));
            return;
        }
    }
};
}  // namespace n_::set_item
namespace w_
{
// set_item(slot: i64, reading: i64) -> list<i64, 2> (node), module demo.references, line 99
::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>, 2>> set_item(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> slot, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading)
{
    return ::hgraph::wire<::hgl::demo::references::m_::n_::set_item::impl_>(wiring_, slot, reading);
}
}  // namespace w_

namespace n_::pick_list
{
// pick_list(flag: bool, ref low: list<i64, 2>, ref high: list<i64, 2>) -> ref list<i64, 2> (node), module demo.references, line 103
struct impl_
{
    static constexpr auto name = "demo.references::pick_list";

    static void eval([[maybe_unused]] ::hgraph::In<"flag", ::hgraph::TS<::hgraph::Bool>> flag, [[maybe_unused]] ::hgraph::In<"low", ::hgraph::REF<::hgraph::TSL<::hgraph::TS<::hgraph::Int>, 2>>, ::hgraph::InputActivity::Passive> low, [[maybe_unused]] ::hgraph::In<"high", ::hgraph::REF<::hgraph::TSL<::hgraph::TS<::hgraph::Int>, 2>>, ::hgraph::InputActivity::Passive> high, [[maybe_unused]] ::hgraph::Out<::hgraph::REF<::hgraph::TSL<::hgraph::TS<::hgraph::Int>, 2>>> out_)
    {
        // when, line 104 (TIME-3)
        if (::hglrt::fn_modified(flag))
        {
            if (::hglrt::checked_value(flag))
            {
                out_.set(::hglrt::designation(high));
                return;
            }
            out_.set(::hglrt::designation(low));
            return;
        }
    }
};
}  // namespace n_::pick_list
namespace w_
{
// pick_list(flag: bool, ref low: list<i64, 2>, ref high: list<i64, 2>) -> ref list<i64, 2> (node), module demo.references, line 103
::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>, 2>> pick_list(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> flag, ::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>, 2>> low, ::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>, 2>> high)
{
    return ::hgraph::wire<::hgl::demo::references::m_::n_::pick_list::impl_>(wiring_, flag, low, high).as<::hgraph::TSL<::hgraph::TS<::hgraph::Int>, 2>>();
}
}  // namespace w_

namespace n_::routed
{
// routed(idx: i64, flag: bool, low_slot: i64, low: i64, high_slot: i64, high: i64) -> i64 (graph), module demo.references, line 110
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> idx, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> flag, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> low_slot, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> low, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> high_slot, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> high)
    {
        return ::hgl::demo::references::m_::w_::follow(wiring_, ::hgl::demo::references::m_::w_::route(wiring_, idx, [&] { const auto argument_00_ = flag; const auto argument_01_ = ::hgl::demo::references::m_::w_::set_item(wiring_, low_slot, low); const auto argument_02_ = ::hgl::demo::references::m_::w_::set_item(wiring_, high_slot, high); return ::hgl::demo::references::m_::w_::pick_list(wiring_, argument_00_, argument_01_, argument_02_); }()));
    }
};
}  // namespace n_::routed
namespace w_
{
// routed(idx: i64, flag: bool, low_slot: i64, low: i64, high_slot: i64, high: i64) -> i64 (graph), module demo.references, line 110
::hgraph::Port<::hgraph::TS<::hgraph::Int>> routed(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> idx, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> flag, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> low_slot, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> low, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> high_slot, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> high)
{
    return ::hgl::demo::references::m_::n_::routed::impl_::compose(wiring_, idx, flag, low_slot, low, high_slot, high);
}
}  // namespace w_

namespace n_::write_entry
{
// write_entry(changed_key: str, price: i64?) -> dict<str, i64> (node), module demo.references, line 136
struct impl_
{
    static constexpr auto name = "demo.references::write_entry";

    static void eval([[maybe_unused]] ::hgraph::In<"changed_key", ::hgraph::TS<::hgraph::Str>> changed_key, [[maybe_unused]] ::hgraph::In<"price", ::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> price, [[maybe_unused]] ::hgraph::Out<::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>> out_)
    {
        // when, line 137 (TIME-3)
        {
            if ((!::hglrt::nullable_value(price).has_value()))
            {
                ::hglrt::keyed_publish(out_, ::hglrt::dict_delta<::hgraph::Str, ::hgraph::Int>{::hglrt::dict<::hgraph::Str, ::hgraph::Int>{}, ::hglrt::set<::hgraph::Str>{::hglrt::checked_value(changed_key)}});
                return;
            }
            ::hglrt::keyed_publish(out_, ::hglrt::dict_delta<::hgraph::Str, ::hgraph::Int>{::hglrt::dict_of<::hgraph::Str, ::hgraph::Int>({{::hglrt::checked_value(changed_key), ::hglrt::nullable_payload(price)}}), ::hglrt::set<::hgraph::Str>{}});
            return;
        }
    }
};
}  // namespace n_::write_entry
namespace w_
{
// write_entry(changed_key: str, price: i64?) -> dict<str, i64> (node), module demo.references, line 136
::hgraph::Port<::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>> write_entry(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Str>> changed_key, ::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> price)
{
    return ::hgraph::wire<::hgl::demo::references::m_::n_::write_entry::impl_>(wiring_, changed_key, price);
}
}  // namespace w_

namespace n_::selected
{
// selected(key: str, changed_key: str, price: i64?) -> i64 (graph), module demo.references, line 143
struct impl_
{
    // The entry of a dictionary its key selects, on line 144: a
    // designation of it (EXPR-7, TIME-10).
    struct selected_00_
    {
        static constexpr auto name = "demo.references:entry";
        static void start(::hgraph::State<::hgraph::Int> state_) { state_.set(::hgraph::Int{0}); }
        static void eval(::hgraph::In<"dictionary", ::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>, ::hgraph::InputActivity::Structural, ::hgraph::InputValidity::Unchecked> dictionary, ::hgraph::In<"key", ::hgraph::TS<::hgraph::Str>> key, ::hgraph::State<::hgraph::Int> state_, ::hgraph::Out<::hgraph::REF<::hgraph::TS<::hgraph::Int>>> out_)
        {
            ::hglrt::select_entry(dictionary, key, state_.modify(), out_);
        }
    };

    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Str>> key, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Str>> changed_key, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> price)
    {
        return ::hgl::demo::references::m_::w_::follow(wiring_, ::hgraph::wire<selected_00_>(wiring_, ::hgl::demo::references::m_::w_::write_entry(wiring_, changed_key, price), key).as<::hgraph::TS<::hgraph::Int>>());
    }
};
}  // namespace n_::selected
namespace w_
{
// selected(key: str, changed_key: str, price: i64?) -> i64 (graph), module demo.references, line 143
::hgraph::Port<::hgraph::TS<::hgraph::Int>> selected(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Str>> key, ::hgraph::Port<::hgraph::TS<::hgraph::Str>> changed_key, ::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> price)
{
    return ::hgl::demo::references::m_::n_::selected::impl_::compose(wiring_, key, changed_key, price);
}
}  // namespace w_

namespace n_::present_item
{
// present_item(probe: bool, item: i64) -> bool (node), module demo.references, line 154
struct impl_
{
    static constexpr auto name = "demo.references::present_item";

    static void eval([[maybe_unused]] ::hgraph::In<"probe", ::hgraph::TS<::hgraph::Bool>> probe, [[maybe_unused]] ::hgraph::In<"item", ::hgraph::TS<::hgraph::Int>, ::hgraph::InputActivity::Passive, ::hgraph::InputValidity::Unchecked> item, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Bool>> out_)
    {
        // when, line 155 (TIME-3)
        if (::hglrt::fn_modified(probe))
        {
            out_.set(::hglrt::fn_valid(item));
            return;
        }
    }
};
}  // namespace n_::present_item
namespace w_
{
// present_item(probe: bool, item: i64) -> bool (node), module demo.references, line 154
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> present_item(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> item)
{
    return ::hgraph::wire<::hgl::demo::references::m_::n_::present_item::impl_>(wiring_, probe, item);
}
}  // namespace w_

namespace n_::selected_present
{
// selected_present(probe: bool, key: str, changed_key: str, price: i64?) -> bool (graph), module demo.references, line 158
struct impl_
{
    // The entry of a dictionary its key selects, on line 159: a
    // designation of it (EXPR-7, TIME-10).
    struct selected_00_
    {
        static constexpr auto name = "demo.references:entry";
        static void start(::hgraph::State<::hgraph::Int> state_) { state_.set(::hgraph::Int{0}); }
        static void eval(::hgraph::In<"dictionary", ::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>, ::hgraph::InputActivity::Structural, ::hgraph::InputValidity::Unchecked> dictionary, ::hgraph::In<"key", ::hgraph::TS<::hgraph::Str>> key, ::hgraph::State<::hgraph::Int> state_, ::hgraph::Out<::hgraph::REF<::hgraph::TS<::hgraph::Int>>> out_)
        {
            ::hglrt::select_entry(dictionary, key, state_.modify(), out_);
        }
    };

    static ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Str>> key, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Str>> changed_key, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> price)
    {
        return ::hgl::demo::references::m_::w_::present_item(wiring_, probe, ::hgraph::wire<selected_00_>(wiring_, ::hgl::demo::references::m_::w_::write_entry(wiring_, changed_key, price), key).as<::hgraph::TS<::hgraph::Int>>());
    }
};
}  // namespace n_::selected_present
namespace w_
{
// selected_present(probe: bool, key: str, changed_key: str, price: i64?) -> bool (graph), module demo.references, line 158
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> selected_present(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::TS<::hgraph::Str>> key, ::hgraph::Port<::hgraph::TS<::hgraph::Str>> changed_key, ::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> price)
{
    return ::hgl::demo::references::m_::n_::selected_present::impl_::compose(wiring_, probe, key, changed_key, price);
}
}  // namespace w_

namespace n_::entry_a
{
// entry_a(changed_key: str, price: i64?) -> i64 (graph), module demo.references, line 171
struct impl_
{
    // The entry of a dictionary its key selects, on line 171: a
    // designation of it (EXPR-7, TIME-10).
    struct selected_00_
    {
        static constexpr auto name = "demo.references:entry";
        static void start(::hgraph::State<::hgraph::Int> state_) { state_.set(::hgraph::Int{0}); }
        static void eval(::hgraph::In<"dictionary", ::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>, ::hgraph::InputActivity::Structural, ::hgraph::InputValidity::Unchecked> dictionary, ::hgraph::In<"key", ::hgraph::TS<::hgraph::Str>> key, ::hgraph::State<::hgraph::Int> state_, ::hgraph::Out<::hgraph::REF<::hgraph::TS<::hgraph::Int>>> out_)
        {
            ::hglrt::select_entry(dictionary, key, state_.modify(), out_);
        }
    };

    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Str>> changed_key, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> price)
    {
        return ::hgl::demo::references::m_::w_::follow(wiring_, ::hgraph::wire<selected_00_>(wiring_, ::hgl::demo::references::m_::w_::write_entry(wiring_, changed_key, price), ::hglrt::constant<::hgraph::Str>(wiring_, ::hgraph::Str{"A"})).as<::hgraph::TS<::hgraph::Int>>());
    }
};
}  // namespace n_::entry_a
namespace w_
{
// entry_a(changed_key: str, price: i64?) -> i64 (graph), module demo.references, line 171
::hgraph::Port<::hgraph::TS<::hgraph::Int>> entry_a(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Str>> changed_key, ::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> price)
{
    return ::hgl::demo::references::m_::n_::entry_a::impl_::compose(wiring_, changed_key, price);
}
}  // namespace w_

namespace n_::i_::sampled::of_i64
{
// sampled(take: bool, ref target: i64) -> ref i64 (node), the instance `i64`, module demo.references, line 51
struct impl_
{
    static constexpr auto name = "demo.references::sampled[i64]";

    static void eval([[maybe_unused]] ::hgraph::In<"take", ::hgraph::TS<::hgraph::Bool>> take, [[maybe_unused]] ::hgraph::In<"target", ::hgraph::REF<::hgraph::TS<::hgraph::Int>>, ::hgraph::InputActivity::Passive> target, [[maybe_unused]] ::hgraph::Out<::hgraph::REF<::hgraph::TS<::hgraph::Int>>> out_)
    {
        // when, line 52 (TIME-3)
        if (::hglrt::fn_modified(take))
        {
            out_.set(::hglrt::designation(target));
            return;
        }
    }
};
}  // namespace n_::i_::sampled::of_i64
namespace w_::i_::sampled
{
// sampled(take: bool, ref target: i64) -> ref i64 (node), the instance `i64`, module demo.references, line 51
::hgraph::Port<::hgraph::TS<::hgraph::Int>> of_i64(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> take, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> target)
{
    return ::hgraph::wire<::hgl::demo::references::m_::n_::i_::sampled::of_i64::impl_>(wiring_, take, target).as<::hgraph::TS<::hgraph::Int>>();
}
}  // namespace w_::i_::sampled

}  // namespace hgl::demo::references::m_

namespace hgl::demo::references::m_::t_
{

void a_reader_observes_the_input_designated()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_09_ = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Bool>{false, ::std::nullopt, true, ::std::nullopt};
        const auto trace_02_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 2, 3, ::std::nullopt};
        const auto trace_04_ = ::hglrt::testing::Dense<::hgraph::Int>{10, ::std::nullopt, ::std::nullopt, 11};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_02_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_05_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_04_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_06_ = input_01_; const auto argument_07_ = input_03_; const auto argument_08_ = input_05_; return ::hgl::demo::references::m_::w_::picked(wiring_, argument_06_, argument_07_, argument_08_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_010_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 2, 10, 11};
        ::hglrt::testing::expect_same(left_09_, right_010_);
    }();
}

void an_eval_records_a_ref_result_as_a_reader_does()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_09_ = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Bool>{true, ::std::nullopt};
        const auto trace_02_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 2};
        const auto trace_04_ = ::hglrt::testing::Dense<::hgraph::Int>{3, ::std::nullopt};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_02_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_05_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_04_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_06_ = input_01_; const auto argument_07_ = input_03_; const auto argument_08_ = input_05_; return ::hgl::demo::references::m_::w_::pick(wiring_, argument_06_, argument_07_, argument_08_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_010_ = ::hglrt::testing::Dense<::hgraph::Int>{3, ::std::nullopt};
        ::hglrt::testing::expect_same(left_09_, right_010_);
    }();
}

void a_ref_parameter_is_present_and_wakes_once()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_06_ = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Bool>{true, true, true};
        const auto trace_02_ = ::hglrt::testing::Dense<::hgraph::Int>{::std::nullopt, 7, 8};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_02_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_04_ = input_01_; const auto argument_05_ = input_03_; return ::hgl::demo::references::m_::w_::present(wiring_, argument_04_, argument_05_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_07_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 1, 1};
        ::hglrt::testing::expect_same(left_06_, right_07_);
    }();
}

void a_relay_keeps_what_it_sampled()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_012_ = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Bool>{true, ::std::nullopt, ::std::nullopt, true};
        const auto trace_02_ = ::hglrt::testing::Dense<::hgraph::Bool>{false, true, ::std::nullopt, ::std::nullopt};
        const auto trace_04_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 2, 3, ::std::nullopt};
        const auto trace_06_ = ::hglrt::testing::Dense<::hgraph::Int>{10, ::std::nullopt, 11, ::std::nullopt};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(trace_02_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_05_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_04_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_07_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_06_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_08_ = input_01_; const auto argument_09_ = input_03_; const auto argument_010_ = input_05_; const auto argument_011_ = input_07_; return ::hgl::demo::references::m_::w_::sample_picked(wiring_, argument_08_, argument_09_, argument_010_, argument_011_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_013_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 2, 3, 11};
        ::hglrt::testing::expect_same(left_012_, right_013_);
    }();
}

void a_designated_input_is_followed_through_its_rebinding()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_015_ = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Int>{1, ::std::nullopt, ::std::nullopt, ::std::nullopt, 2, ::std::nullopt, 0, ::std::nullopt, 1};
        const auto trace_02_ = ::hglrt::testing::Dense<::hgraph::Bool>{false, ::std::nullopt, true, ::std::nullopt, ::std::nullopt, ::std::nullopt, ::std::nullopt, false, ::std::nullopt};
        const auto trace_04_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 2, ::std::nullopt, 3, ::std::nullopt, ::std::nullopt, ::std::nullopt, 4, ::std::nullopt};
        const auto trace_06_ = ::hglrt::testing::Dense<::hgraph::Int>{10, ::std::nullopt, ::std::nullopt, 11, ::std::nullopt, ::std::nullopt, ::std::nullopt, ::std::nullopt, ::std::nullopt};
        const auto trace_08_ = ::hglrt::testing::Dense<::hgraph::Int>{100, ::std::nullopt, ::std::nullopt, ::std::nullopt, ::std::nullopt, 101, ::std::nullopt, 102, ::std::nullopt};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(trace_02_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_05_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_04_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_07_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_06_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_09_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_08_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_010_ = input_01_; const auto argument_011_ = input_03_; const auto argument_012_ = input_05_; const auto argument_013_ = input_07_; const auto argument_014_ = input_09_; return ::hgl::demo::references::m_::w_::chosen(wiring_, argument_010_, argument_011_, argument_012_, argument_013_, argument_014_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_016_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 2, 10, 11, 100, 101, ::std::nullopt, ::std::nullopt, 4};
        ::hglrt::testing::expect_same(left_015_, right_016_);
    }();
}

void a_designated_position_is_followed_through_its_rebinding()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_018_ = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Int>{1, ::std::nullopt, ::std::nullopt, 0, ::std::nullopt};
        const auto trace_02_ = ::hglrt::testing::Dense<::hgraph::Bool>{false, ::std::nullopt, true, ::std::nullopt, ::std::nullopt};
        const auto trace_04_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 0, ::std::nullopt, ::std::nullopt, 0};
        const auto trace_06_ = ::hglrt::testing::Dense<::hgraph::Int>{10, 11, ::std::nullopt, ::std::nullopt, 12};
        const auto trace_08_ = ::hglrt::testing::Dense<::hgraph::Int>{1, ::std::nullopt, 0, ::std::nullopt, ::std::nullopt};
        const auto trace_010_ = ::hglrt::testing::Dense<::hgraph::Int>{20, ::std::nullopt, 21, ::std::nullopt, ::std::nullopt};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(trace_02_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_05_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_04_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_07_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_06_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_09_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_08_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_011_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_010_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_012_ = input_01_; const auto argument_013_ = input_03_; const auto argument_014_ = input_05_; const auto argument_015_ = input_07_; const auto argument_016_ = input_09_; const auto argument_017_ = input_011_; return ::hgl::demo::references::m_::w_::routed(wiring_, argument_012_, argument_013_, argument_014_, argument_015_, argument_016_, argument_017_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_019_ = ::hglrt::testing::Dense<::hgraph::Int>{10, ::std::nullopt, 20, 21, ::std::nullopt};
        ::hglrt::testing::expect_same(left_018_, right_019_);
    }();
}

void a_designated_position_outside_the_list_fails()
{
    ::hglrt::testing::statement();
    ::hglrt::testing::raises("value.index_out_of_range", [&] {
        static_cast<void>([&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Int>{2};
        const auto trace_02_ = ::hglrt::testing::Dense<::std::array<::hgraph::Int, 2>>{::hglrt::testing::Cell<::std::array<::hgraph::Int, 2>>{::hglrt::testing::Whole<::std::array<::hgraph::Int, 2>>{::std::array<::hgraph::Int, 2>{1, 2}}}};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        const ::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>, 2>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_1list_1i64_1_12_1_(wiring_, ::hglrt::testing::replay_items(trace_02_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_04_ = input_01_; const auto argument_05_ = input_03_; return ::hgl::demo::references::m_::w_::route(wiring_, argument_04_, argument_05_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }());
    });
}

void a_selected_entry_is_followed_through_its_key()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_09_ = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Str>{::hgraph::Str{"A"}, ::std::nullopt, ::hgraph::Str{"B"}, ::std::nullopt, ::std::nullopt, ::std::nullopt, ::hgraph::Str{"C"}, ::std::nullopt};
        const auto trace_02_ = ::hglrt::testing::Dense<::hgraph::Str>{::hgraph::Str{"A"}, ::hgraph::Str{"B"}, ::std::nullopt, ::hgraph::Str{"A"}, ::hgraph::Str{"B"}, ::hgraph::Str{"B"}, ::std::nullopt, ::hgraph::Str{"C"}};
        const auto trace_04_ = ::hglrt::testing::Dense<::std::optional<::hgraph::Int>>{::std::optional<::hgraph::Int>{7}, ::std::optional<::hgraph::Int>{20}, ::std::nullopt, ::std::optional<::hgraph::Int>{8}, ::std::optional<::hgraph::Int>{::std::nullopt}, ::std::optional<::hgraph::Int>{22}, ::std::nullopt, ::std::optional<::hgraph::Int>{30}};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Str>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_str(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Str>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_str(wiring_, ::hglrt::testing::replay_items(trace_02_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> input_05_ = ::hgl::core::m_::w_::i_::replay::of_1i64_1_(wiring_, ::hglrt::testing::replay_items(trace_04_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_06_ = input_01_; const auto argument_07_ = input_03_; const auto argument_08_ = input_05_; return ::hgl::demo::references::m_::w_::selected(wiring_, argument_06_, argument_07_, argument_08_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_010_ = ::hglrt::testing::Dense<::hgraph::Int>{7, ::std::nullopt, 20, ::std::nullopt, ::std::nullopt, 22, ::std::nullopt, 30};
        ::hglrt::testing::expect_same(left_09_, right_010_);
    }();
}

void a_selected_entry_the_dictionary_lacks_is_not_valid()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_012_ = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Bool>{true, true, true, true};
        const auto trace_02_ = ::hglrt::testing::Dense<::hgraph::Str>{::hgraph::Str{"A"}, ::std::nullopt, ::hgraph::Str{"C"}, ::std::nullopt};
        const auto trace_04_ = ::hglrt::testing::Dense<::hgraph::Str>{::hgraph::Str{"A"}, ::hgraph::Str{"A"}, ::std::nullopt, ::hgraph::Str{"C"}};
        const auto trace_06_ = ::hglrt::testing::Dense<::std::optional<::hgraph::Int>>{::std::optional<::hgraph::Int>{1}, ::std::optional<::hgraph::Int>{::std::nullopt}, ::std::nullopt, ::std::optional<::hgraph::Int>{3}};
        return ::hglrt::testing::evaluate<::hgraph::Bool>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Str>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_str(wiring_, ::hglrt::testing::replay_items(trace_02_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Str>> input_05_ = ::hgl::core::m_::w_::i_::replay::of_str(wiring_, ::hglrt::testing::replay_items(trace_04_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> input_07_ = ::hgl::core::m_::w_::i_::replay::of_1i64_1_(wiring_, ::hglrt::testing::replay_items(trace_06_, start_));
        ::hgl::core::m_::w_::i_::record::of_bool(wiring_, [&] { const auto argument_08_ = input_01_; const auto argument_09_ = input_03_; const auto argument_010_ = input_05_; const auto argument_011_ = input_07_; return ::hgl::demo::references::m_::w_::selected_present(wiring_, argument_08_, argument_09_, argument_010_, argument_011_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_013_ = ::hglrt::testing::Dense<::hgraph::Bool>{true, false, false, true};
        ::hglrt::testing::expect_same(left_012_, right_013_);
    }();
}

void an_entry_at_a_value_key_is_followed()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_06_ = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Str>{::hgraph::Str{"B"}, ::hgraph::Str{"A"}, ::hgraph::Str{"A"}};
        const auto trace_02_ = ::hglrt::testing::Dense<::std::optional<::hgraph::Int>>{::std::optional<::hgraph::Int>{1}, ::std::optional<::hgraph::Int>{2}, ::std::optional<::hgraph::Int>{3}};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Str>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_str(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_1i64_1_(wiring_, ::hglrt::testing::replay_items(trace_02_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_04_ = input_01_; const auto argument_05_ = input_03_; return ::hgl::demo::references::m_::w_::entry_a(wiring_, argument_04_, argument_05_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_07_ = ::hglrt::testing::Dense<::hgraph::Int>{::std::nullopt, 2, 3};
        ::hglrt::testing::expect_same(left_06_, right_07_);
    }();
}

}  // namespace hgl::demo::references::m_::t_
