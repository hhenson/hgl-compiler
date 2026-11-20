// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include "demo.references.h"
#include "core.h"

namespace hgl::demo::references::m_
{

namespace n_
{
// pick(flag: bool, ref low: i64, ref high: i64) -> ref i64 (node), module demo.references, line 9
struct pick
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
}  // namespace n_
namespace w_
{
// pick(flag: bool, ref low: i64, ref high: i64) -> ref i64 (node), module demo.references, line 9
::hgraph::Port<::hgraph::TS<::hgraph::Int>> pick(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> flag, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> low, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> high)
{
    return ::hgraph::wire<::hgl::demo::references::m_::n_::pick>(wiring_, flag, low, high).as<::hgraph::TS<::hgraph::Int>>();
}
}  // namespace w_

namespace n_
{
// follow(item: i64) -> i64 (node), module demo.references, line 17
struct follow
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
}  // namespace n_
namespace w_
{
// follow(item: i64) -> i64 (node), module demo.references, line 17
::hgraph::Port<::hgraph::TS<::hgraph::Int>> follow(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> item)
{
    return ::hgraph::wire<::hgl::demo::references::m_::n_::follow>(wiring_, item);
}
}  // namespace w_

namespace n_
{
// picked(flag: bool, low: i64, high: i64) -> i64 (graph), module demo.references, line 21
struct picked
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> flag, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> low, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> high)
    {
        return ::hgl::demo::references::m_::w_::follow(wiring_, ::hgl::demo::references::m_::w_::pick(wiring_, flag, low, high));
    }
};
}  // namespace n_
namespace w_
{
// picked(flag: bool, low: i64, high: i64) -> i64 (graph), module demo.references, line 21
::hgraph::Port<::hgraph::TS<::hgraph::Int>> picked(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> flag, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> low, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> high)
{
    return ::hgl::demo::references::m_::n_::picked::compose(wiring_, flag, low, high);
}
}  // namespace w_

namespace n_
{
// present(probe: bool, ref target: i64) -> i64 (node), module demo.references, line 37
struct present
{
    static constexpr auto name = "demo.references::present";

    // The node's `state` and `cache` (TIME-6).
    struct state_type
    {
        ::hgraph::Int wakes{};
    };

    static void start([[maybe_unused]] ::hgraph::State<state_type> state_)
    {
        state_.set(state_type{});
        state_type& kept_ = state_.modify();
        [[maybe_unused]] auto& wakes = kept_.wakes;
        wakes = 0;
    }

    static void eval([[maybe_unused]] ::hgraph::In<"probe", ::hgraph::TS<::hgraph::Bool>> probe, [[maybe_unused]] ::hgraph::In<"target", ::hgraph::REF<::hgraph::TS<::hgraph::Int>>, ::hgraph::InputValidity::Unchecked> target, [[maybe_unused]] ::hgraph::State<state_type> state_, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        state_type& kept_ = state_.modify();
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
}  // namespace n_
}  // namespace hgl::demo::references::m_

// The state of the node, as hgraph holds it (TIME-6).
template <>
struct hgraph::scalar_descriptor<::hgl::demo::references::m_::n_::present::state_type>
{
    [[nodiscard]] static constexpr bool is_concrete() noexcept { return true; }
    [[nodiscard]] static const ValueTypeMetaData* value_meta()
    {
        return TypeRegistry::instance().register_scalar<::hgl::demo::references::m_::n_::present::state_type>("hgl:demo.references::present.state");
    }
};

namespace hgl::demo::references::m_
{
namespace w_
{
// present(probe: bool, ref target: i64) -> i64 (node), module demo.references, line 37
::hgraph::Port<::hgraph::TS<::hgraph::Int>> present(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> target)
{
    return ::hgraph::wire<::hgl::demo::references::m_::n_::present>(wiring_, probe, target);
}
}  // namespace w_

namespace n_
{
// sample_picked(take: bool, flag: bool, low: i64, high: i64) -> i64 (graph), module demo.references, line 55
struct sample_picked
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> take, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> flag, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> low, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> high)
    {
        return ::hgl::demo::references::m_::w_::follow(wiring_, ::hgl::demo::references::m_::w_::i_::sampled::of_i64(wiring_, take, ::hgl::demo::references::m_::w_::pick(wiring_, flag, low, high)));
    }
};
}  // namespace n_
namespace w_
{
// sample_picked(take: bool, flag: bool, low: i64, high: i64) -> i64 (graph), module demo.references, line 55
::hgraph::Port<::hgraph::TS<::hgraph::Int>> sample_picked(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> take, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> flag, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> low, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> high)
{
    return ::hgl::demo::references::m_::n_::sample_picked::compose(wiring_, take, flag, low, high);
}
}  // namespace w_

namespace n_
{
// choose(flag: i64, low: i64, high: i64) -> ref i64 (node), module demo.references, line 70
struct choose
{
    static constexpr auto name = "demo.references::choose";

    // The node's `state` and `cache` (TIME-6).
    struct state_type
    {
        // FN-16: the input the `ref` result follows, by its place among
        // those it designates, or -1 for none.
        ::hgraph::Int designates_{-1};
    };

    static void start([[maybe_unused]] ::hgraph::State<state_type> state_)
    {
        state_.set(state_type{});
    }

    static void eval([[maybe_unused]] ::hgraph::In<"flag", ::hgraph::TS<::hgraph::Int>, ::hgraph::InputValidity::Unchecked> flag, [[maybe_unused]] ::hgraph::In<"low", ::hgraph::TS<::hgraph::Int>, ::hgraph::InputActivity::Passive, ::hgraph::InputValidity::Unchecked> low, [[maybe_unused]] ::hgraph::In<"high", ::hgraph::TS<::hgraph::Int>, ::hgraph::InputActivity::Passive, ::hgraph::InputValidity::Unchecked> high, [[maybe_unused]] ::hgraph::In<"designated:low", ::hgraph::REF<::hgraph::TS<::hgraph::Int>>, ::hgraph::InputValidity::Unchecked> designated_01_, [[maybe_unused]] ::hgraph::In<"designated:high", ::hgraph::REF<::hgraph::TS<::hgraph::Int>>, ::hgraph::InputValidity::Unchecked> designated_02_, [[maybe_unused]] ::hgraph::State<state_type> state_, [[maybe_unused]] ::hgraph::Out<::hgraph::REF<::hgraph::TS<::hgraph::Int>>> out_)
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
}  // namespace n_
}  // namespace hgl::demo::references::m_

// The state of the node, as hgraph holds it (TIME-6).
template <>
struct hgraph::scalar_descriptor<::hgl::demo::references::m_::n_::choose::state_type>
{
    [[nodiscard]] static constexpr bool is_concrete() noexcept { return true; }
    [[nodiscard]] static const ValueTypeMetaData* value_meta()
    {
        return TypeRegistry::instance().register_scalar<::hgl::demo::references::m_::n_::choose::state_type>("hgl:demo.references::choose.state");
    }
};

namespace hgl::demo::references::m_
{
namespace w_
{
// choose(flag: i64, low: i64, high: i64) -> ref i64 (node), module demo.references, line 70
::hgraph::Port<::hgraph::TS<::hgraph::Int>> choose(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> flag, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> low, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> high)
{
    return ::hgraph::wire<::hgl::demo::references::m_::n_::choose>(wiring_, flag, low, high, low, high).as<::hgraph::TS<::hgraph::Int>>();
}
}  // namespace w_

namespace n_
{
// chosen(flag: i64, sel: bool, a: i64, b: i64, high: i64) -> i64 (graph), module demo.references, line 79
struct chosen
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> flag, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> sel, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> a, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> b, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> high)
    {
        return ::hgl::demo::references::m_::w_::follow(wiring_, ::hgl::demo::references::m_::w_::choose(wiring_, flag, ::hgl::demo::references::m_::w_::pick(wiring_, sel, a, b), high));
    }
};
}  // namespace n_
namespace w_
{
// chosen(flag: i64, sel: bool, a: i64, b: i64, high: i64) -> i64 (graph), module demo.references, line 79
::hgraph::Port<::hgraph::TS<::hgraph::Int>> chosen(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> flag, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> sel, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> a, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> b, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> high)
{
    return ::hgl::demo::references::m_::n_::chosen::compose(wiring_, flag, sel, a, b, high);
}
}  // namespace w_

namespace n_::i_::sampled
{
// sampled(take: bool, ref target: i64) -> ref i64 (node), the instance `i64`, module demo.references, line 51
struct of_i64
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
}  // namespace n_::i_::sampled
namespace w_::i_::sampled
{
// sampled(take: bool, ref target: i64) -> ref i64 (node), the instance `i64`, module demo.references, line 51
::hgraph::Port<::hgraph::TS<::hgraph::Int>> of_i64(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> take, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> target)
{
    return ::hgraph::wire<::hgl::demo::references::m_::n_::i_::sampled::of_i64>(wiring_, take, target).as<::hgraph::TS<::hgraph::Int>>();
}
}  // namespace w_::i_::sampled

}  // namespace hgl::demo::references::m_

namespace hgl::demo::references::m_::t_
{

void a_reader_observes_the_input_designated()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_06_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Bool>{false, std::nullopt, true, std::nullopt}, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 2, 3, std::nullopt}, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_02_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{10, std::nullopt, std::nullopt, 11}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_03_ = input_00_; const auto argument_04_ = input_01_; const auto argument_05_ = input_02_; return ::hgl::demo::references::m_::w_::picked(wiring_, argument_03_, argument_04_, argument_05_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_07_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 2, 10, 11};
        ::hglrt::testing::expect_same(left_06_, right_07_);
    }();
}

void an_eval_records_a_ref_result_as_a_reader_does()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_06_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Bool>{true, std::nullopt}, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 2}, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_02_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{3, std::nullopt}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_03_ = input_00_; const auto argument_04_ = input_01_; const auto argument_05_ = input_02_; return ::hgl::demo::references::m_::w_::pick(wiring_, argument_03_, argument_04_, argument_05_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_07_ = ::hglrt::testing::Dense<::hgraph::Int>{3, std::nullopt};
        ::hglrt::testing::expect_same(left_06_, right_07_);
    }();
}

void a_ref_parameter_is_present_and_wakes_once()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_04_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Bool>{true, true, true}, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{std::nullopt, 7, 8}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_02_ = input_00_; const auto argument_03_ = input_01_; return ::hgl::demo::references::m_::w_::present(wiring_, argument_02_, argument_03_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_05_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 1, 1};
        ::hglrt::testing::expect_same(left_04_, right_05_);
    }();
}

void a_relay_keeps_what_it_sampled()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_08_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Bool>{true, std::nullopt, std::nullopt, true}, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Bool>{false, true, std::nullopt, std::nullopt}, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_02_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 2, 3, std::nullopt}, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{10, std::nullopt, 11, std::nullopt}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_04_ = input_00_; const auto argument_05_ = input_01_; const auto argument_06_ = input_02_; const auto argument_07_ = input_03_; return ::hgl::demo::references::m_::w_::sample_picked(wiring_, argument_04_, argument_05_, argument_06_, argument_07_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_09_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 2, 3, 11};
        ::hglrt::testing::expect_same(left_08_, right_09_);
    }();
}

void a_designated_input_is_followed_through_its_rebinding()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_010_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, std::nullopt, std::nullopt, std::nullopt, 2, std::nullopt, 0, std::nullopt, 1}, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Bool>{false, std::nullopt, true, std::nullopt, std::nullopt, std::nullopt, std::nullopt, false, std::nullopt}, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_02_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 2, std::nullopt, 3, std::nullopt, std::nullopt, std::nullopt, 4, std::nullopt}, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{10, std::nullopt, std::nullopt, 11, std::nullopt, std::nullopt, std::nullopt, std::nullopt, std::nullopt}, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_04_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{100, std::nullopt, std::nullopt, std::nullopt, std::nullopt, 101, std::nullopt, 102, std::nullopt}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_05_ = input_00_; const auto argument_06_ = input_01_; const auto argument_07_ = input_02_; const auto argument_08_ = input_03_; const auto argument_09_ = input_04_; return ::hgl::demo::references::m_::w_::chosen(wiring_, argument_05_, argument_06_, argument_07_, argument_08_, argument_09_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_011_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 2, 10, 11, 100, 101, std::nullopt, std::nullopt, 4};
        ::hglrt::testing::expect_same(left_010_, right_011_);
    }();
}

}  // namespace hgl::demo::references::m_::t_
