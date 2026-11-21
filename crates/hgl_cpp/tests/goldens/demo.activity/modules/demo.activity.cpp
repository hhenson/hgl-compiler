// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include "demo.activity.h"
#include "core.h"

// Names this file writes, which no macro of a header above takes.
#undef Mode
#undef a_passive_input_wakes_no_node
#undef activity
#undef activity_belongs_to_the_binding
#undef an_enum_is_a_constant_connection
#undef an_enum_is_published
#undef bindings
#undef core
#undef data
#undef demo
#undef driving
#undef gated
#undef ignore
#undef listen
#undef listening
#undef mode
#undef mode_of
#undef record
#undef replay
#undef same_twice
#undef sampled
#undef value

namespace hgl::demo::activity::m_
{

namespace n_::gated
{
// gated(mode: Mode, data: i64) -> i64 (node), module demo.activity, line 13
struct impl_
{
    static constexpr auto name = "demo.activity::gated";

    static void eval([[maybe_unused]] ::hgraph::In<"mode", ::hgraph::TS<::hgl::demo::activity::m_::Mode>> mode, [[maybe_unused]] ::hgraph::In<"data", ::hgraph::TS<::hgraph::Int>> data, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 14 (TIME-3)
        if (::hglrt::fn_modified(mode))
        {
            if (::hglrt::eq__T_T<::hgl::demo::activity::m_::Mode>(::hglrt::checked_value(mode), ::hgl::demo::activity::m_::Mode::listen))
            {
                ::hglrt::fn_activate(data);
            }
            else
            {
                ::hglrt::fn_passivate(data);
            }
        }
        // when, line 18 (TIME-3)
        if (::hglrt::fn_modified(data) && ::hglrt::fn_is_active(data))
        {
            out_.set(::hglrt::checked_value(data));
            return;
        }
        // when, line 19 (TIME-3)
        if (::hglrt::fn_modified(data))
        {
            out_.set(::hglrt::neg__i64(::hglrt::checked_value(data)));
            return;
        }
    }
};
}  // namespace n_::gated
namespace w_
{
// gated(mode: Mode, data: i64) -> i64 (node), module demo.activity, line 13
::hgraph::Port<::hgraph::TS<::hgraph::Int>> gated(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgl::demo::activity::m_::Mode>> mode, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> data)
{
    return ::hgraph::wire<::hgl::demo::activity::m_::n_::gated::impl_>(wiring_, mode, data);
}
}  // namespace w_

namespace n_::mode_of
{
// mode_of(value: i64) -> Mode (node), module demo.activity, line 23
struct impl_
{
    static constexpr auto name = "demo.activity::mode_of";

    static void eval([[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgl::demo::activity::m_::Mode>> out_)
    {
        // when, line 24 (TIME-3)
        {
            if (::hglrt::gt__T_T<::hgraph::Int>(::hglrt::checked_value(value), 0))
            {
                out_.set(::hgl::demo::activity::m_::Mode::listen);
                return;
            }
            out_.set(::hgl::demo::activity::m_::Mode::ignore);
            return;
        }
    }
};
}  // namespace n_::mode_of
namespace w_
{
// mode_of(value: i64) -> Mode (node), module demo.activity, line 23
::hgraph::Port<::hgraph::TS<::hgl::demo::activity::m_::Mode>> mode_of(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgraph::wire<::hgl::demo::activity::m_::n_::mode_of::impl_>(wiring_, value);
}
}  // namespace w_

namespace n_::bindings
{
// bindings(driving: i64, sampled: i64) -> bool (node), module demo.activity, line 32
struct impl_
{
    static constexpr auto name = "demo.activity::bindings";

    static void eval([[maybe_unused]] ::hgraph::In<"driving", ::hgraph::TS<::hgraph::Int>> driving, [[maybe_unused]] ::hgraph::In<"sampled", ::hgraph::TS<::hgraph::Int>, ::hgraph::InputActivity::Passive> sampled, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Bool>> out_)
    {
        // when, line 33 (TIME-3)
        if (::hglrt::fn_modified(driving))
        {
            out_.set((::hglrt::fn_is_active(driving) && (!::hglrt::fn_is_active(sampled))));
            return;
        }
    }
};
}  // namespace n_::bindings
namespace w_
{
// bindings(driving: i64, sampled: i64) -> bool (node), module demo.activity, line 32
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> bindings(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> driving, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> sampled)
{
    return ::hgraph::wire<::hgl::demo::activity::m_::n_::bindings::impl_>(wiring_, driving, sampled);
}
}  // namespace w_

namespace n_::same_twice
{
// same_twice(value: i64) -> bool (graph), module demo.activity, line 36
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
    {
        return ::hgl::demo::activity::m_::w_::bindings(wiring_, value, value);
    }
};
}  // namespace n_::same_twice
namespace w_
{
// same_twice(value: i64) -> bool (graph), module demo.activity, line 36
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> same_twice(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgl::demo::activity::m_::n_::same_twice::impl_::compose(wiring_, value);
}
}  // namespace w_

namespace n_::listening
{
// listening(data: i64) -> i64 (graph), module demo.activity, line 39
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> data)
    {
        return ::hgl::demo::activity::m_::w_::gated(wiring_, ::hglrt::constant<::hgl::demo::activity::m_::Mode>(wiring_, ::hgl::demo::activity::m_::Mode::listen), data);
    }
};
}  // namespace n_::listening
namespace w_
{
// listening(data: i64) -> i64 (graph), module demo.activity, line 39
::hgraph::Port<::hgraph::TS<::hgraph::Int>> listening(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> data)
{
    return ::hgl::demo::activity::m_::n_::listening::impl_::compose(wiring_, data);
}
}  // namespace w_

}  // namespace hgl::demo::activity::m_

namespace hgl::demo::activity::m_::t_
{

void a_passive_input_wakes_no_node()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_06_ = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgl::demo::activity::m_::Mode>{::hgl::demo::activity::m_::Mode::listen, ::hgl::demo::activity::m_::Mode::ignore, ::std::nullopt, ::hgl::demo::activity::m_::Mode::ignore, ::hgl::demo::activity::m_::Mode::listen, ::std::nullopt};
        const auto trace_02_ = ::hglrt::testing::Dense<::hgraph::Int>{1, ::std::nullopt, 2, 3, ::std::nullopt, 4};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgl::demo::activity::m_::Mode>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_1demo_1activity_1_1Mode_(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_02_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_04_ = input_01_; const auto argument_05_ = input_03_; return ::hgl::demo::activity::m_::w_::gated(wiring_, argument_04_, argument_05_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_07_ = ::hglrt::testing::Dense<::hgraph::Int>{1, ::std::nullopt, ::std::nullopt, ::hglrt::neg__i64(3), ::std::nullopt, 4};
        ::hglrt::testing::expect_same(left_06_, right_07_);
    }();
}

void an_enum_is_published()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgl::demo::activity::m_::Mode>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, ::hglrt::neg__i64(1), 2}, start_));
        ::hgl::core::m_::w_::i_::record::of_1demo_1activity_1_1Mode_(wiring_, ::hgl::demo::activity::m_::w_::mode_of(wiring_, input_00_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgl::demo::activity::m_::Mode>{::hgl::demo::activity::m_::Mode::listen, ::hgl::demo::activity::m_::Mode::ignore, ::hgl::demo::activity::m_::Mode::listen};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
}

void activity_belongs_to_the_binding()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgraph::Bool>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{7, ::std::nullopt, 7}, start_));
        ::hgl::core::m_::w_::i_::record::of_bool(wiring_, ::hgl::demo::activity::m_::w_::same_twice(wiring_, input_00_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgraph::Bool>{true, ::std::nullopt, true};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
}

void an_enum_is_a_constant_connection()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 2}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::activity::m_::w_::listening(wiring_, input_00_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 2};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
}

}  // namespace hgl::demo::activity::m_::t_
