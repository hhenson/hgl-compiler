// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include "demo.store.h"
#include "core.h"

// Names this file writes, which no macro of a header above takes.
#undef GlobalState
#undef Quote
#undef a_struct_is_got_back_in_a_node
#undef a_struct_is_got_back_where_a_graph_is_built
#undef a_tuple_a_dictionary_a_set_and_a_delta_are_got_back
#undef ask
#undef based
#undef bid
#undef bid_plus
#undef book
#undef booked
#undef by
#undef changed
#undef core
#undef counted_members
#undef demo
#undef extra
#undef found
#undef get
#undef given
#undef global_state
#undef member
#undef members
#undef n
#undef paired
#undef q
#undef quoted
#undef quotes
#undef read_at_build
#undef record
#undef replay
#undef reset
#undef seeded
#undef store
#undef ticks
#undef value
#undef values
#undef x

namespace hgl::demo::store::m_
{

namespace n_::quoted
{
// quoted(value: i64) -> i64 (node), module demo.store, line 14
struct impl_
{
    static constexpr auto name = "demo.store::quoted";

    static void eval([[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_, [[maybe_unused]] ::hgraph::GlobalStateView capability_01_)
    {
        [[maybe_unused]] const ::hglrt::GlobalState global_state{capability_01_};
        // when, line 16 (TIME-3)
        {
            ::hglrt::put_GlobalState_str_V<::hgl::demo::store::m_::Quote>(global_state, ::hgraph::Str{"quote"}, ::hgl::demo::store::m_::Quote{::hglrt::checked_value(value), ::hglrt::add__i64_i64(::hglrt::checked_value(value), 1)});
            out_.set((::hglrt::get_GlobalState_str<::hgl::demo::store::m_::Quote>(global_state, ::hgraph::Str{"quote"})).ask);
            return;
        }
    }
};
}  // namespace n_::quoted
namespace w_
{
// quoted(value: i64) -> i64 (node), module demo.store, line 14
::hgraph::Port<::hgraph::TS<::hgraph::Int>> quoted(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgraph::wire<::hgl::demo::store::m_::n_::quoted::impl_>(wiring_, value);
}
}  // namespace w_

namespace n_::paired
{
// paired(value: i64) -> i64 (node), module demo.store, line 23
struct impl_
{
    static constexpr auto name = "demo.store::paired";

    static void eval([[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_, [[maybe_unused]] ::hgraph::GlobalStateView capability_01_)
    {
        [[maybe_unused]] const ::hglrt::GlobalState global_state{capability_01_};
        // when, line 25 (TIME-3)
        {
            ::hglrt::put_GlobalState_str_V<::std::tuple<::hgraph::Int, ::hgraph::Str>>(global_state, ::hgraph::Str{"pair"}, ::std::tuple<::hgraph::Int, ::hgraph::Str>{::hglrt::checked_value(value), ::hgraph::Str{"x"}});
            out_.set(::std::get<0>(::hglrt::get_GlobalState_str<::std::tuple<::hgraph::Int, ::hgraph::Str>>(global_state, ::hgraph::Str{"pair"})));
            return;
        }
    }
};
}  // namespace n_::paired
namespace w_
{
// paired(value: i64) -> i64 (node), module demo.store, line 23
::hgraph::Port<::hgraph::TS<::hgraph::Int>> paired(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgraph::wire<::hgl::demo::store::m_::n_::paired::impl_>(wiring_, value);
}
}  // namespace w_

namespace n_::booked
{
// booked(value: i64) -> i64 (node), module demo.store, line 31
struct impl_
{
    static constexpr auto name = "demo.store::booked";

    static void eval([[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_, [[maybe_unused]] ::hgraph::GlobalStateView capability_01_)
    {
        [[maybe_unused]] const ::hglrt::GlobalState global_state{capability_01_};
        // when, line 33 (TIME-3)
        {
            [[maybe_unused]] const ::hglrt::dict<::hgraph::Str, ::hgraph::Int> given = ::hglrt::dict_of<::hgraph::Str, ::hgraph::Int>({{::hgraph::Str{"a"}, ::hglrt::checked_value(value)}});
            ::hglrt::put_GlobalState_str_V<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>(global_state, ::hgraph::Str{"book"}, given);
            [[maybe_unused]] const ::hglrt::dict<::hgraph::Str, ::hgraph::Int> book = ::hglrt::get_GlobalState_str<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>(global_state, ::hgraph::Str{"book"});
            [[maybe_unused]] const ::std::optional<::hgraph::Int> found = ::hglrt::get_dict_K__T__K<::hgraph::Str, ::hgraph::Int>(book, ::hgraph::Str{"a"});
            if ((!found.has_value()))
            {
                out_.set(::hglrt::neg__i64(1));
                return;
            }
            out_.set((*found));
            return;
        }
    }
};
}  // namespace n_::booked
namespace w_
{
// booked(value: i64) -> i64 (node), module demo.store, line 31
::hgraph::Port<::hgraph::TS<::hgraph::Int>> booked(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgraph::wire<::hgl::demo::store::m_::n_::booked::impl_>(wiring_, value);
}
}  // namespace w_

namespace n_::counted_members
{
// counted_members(value: i64) -> i64 (node), module demo.store, line 43
struct impl_
{
    static constexpr auto name = "demo.store::counted_members";

    static void eval([[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_, [[maybe_unused]] ::hgraph::GlobalStateView capability_01_)
    {
        [[maybe_unused]] const ::hglrt::GlobalState global_state{capability_01_};
        // when, line 45 (TIME-3)
        {
            [[maybe_unused]] const ::hglrt::set<::hgraph::Int> members = ::hglrt::set<::hgraph::Int>{::hglrt::checked_value(value), ::hglrt::add__i64_i64(::hglrt::checked_value(value), 1)};
            ::hglrt::put_GlobalState_str_V<::hglrt::set<::hgraph::Int>>(global_state, ::hgraph::Str{"members"}, members);
            [[maybe_unused]] ::hgraph::Int n = 0;
            const ::hglrt::set<::hgraph::Int> items_00_ = ::hglrt::get_GlobalState_str<::hglrt::set<::hgraph::Int>>(global_state, ::hgraph::Str{"members"});
            for ([[maybe_unused]] const ::hgraph::Int& member : items_00_)
            {
                n = ::hglrt::add__i64_i64(n, member);
            }
            out_.set(n);
            return;
        }
    }
};
}  // namespace n_::counted_members
namespace w_
{
// counted_members(value: i64) -> i64 (node), module demo.store, line 43
::hgraph::Port<::hgraph::TS<::hgraph::Int>> counted_members(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgraph::wire<::hgl::demo::store::m_::n_::counted_members::impl_>(wiring_, value);
}
}  // namespace w_

namespace n_::changed
{
// changed(value: i64) -> i64 (node), module demo.store, line 54
struct impl_
{
    static constexpr auto name = "demo.store::changed";

    static void eval([[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_, [[maybe_unused]] ::hgraph::GlobalStateView capability_01_)
    {
        [[maybe_unused]] const ::hglrt::GlobalState global_state{capability_01_};
        // when, line 56 (TIME-3)
        {
            ::hglrt::put_GlobalState_str_V<::hglrt::set_delta<::hgraph::Int>>(global_state, ::hgraph::Str{"change"}, ::hglrt::set_delta<::hgraph::Int>{::hglrt::set<::hgraph::Int>{::hglrt::checked_value(value)}, ::hglrt::set<::hgraph::Int>{}});
            [[maybe_unused]] ::hgraph::Int n = 0;
            const ::hglrt::set<::hgraph::Int> items_00_ = (::hglrt::get_GlobalState_str<::hglrt::set_delta<::hgraph::Int>>(global_state, ::hgraph::Str{"change"})).added;
            for ([[maybe_unused]] const ::hgraph::Int& member : items_00_)
            {
                n = ::hglrt::add__i64_i64(n, member);
            }
            out_.set(n);
            return;
        }
    }
};
}  // namespace n_::changed
namespace w_
{
// changed(value: i64) -> i64 (node), module demo.store, line 54
::hgraph::Port<::hgraph::TS<::hgraph::Int>> changed(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgraph::wire<::hgl::demo::store::m_::n_::changed::impl_>(wiring_, value);
}
}  // namespace w_

namespace n_::bid_plus
{
// bid_plus(x: i64, const extra: i64) -> i64 (node), module demo.store, line 65
struct impl_
{
    static constexpr auto name = "demo.store::bid_plus";

    static void eval([[maybe_unused]] ::hgraph::In<"x", ::hgraph::TS<::hgraph::Int>> x, [[maybe_unused]] ::hgraph::Scalar<"extra", ::hgraph::Int> extra, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 66 (TIME-3)
        {
            out_.set(::hglrt::add__i64_i64(::hglrt::checked_value(x), extra.value()));
            return;
        }
    }
};
}  // namespace n_::bid_plus
namespace w_
{
// bid_plus(x: i64, const extra: i64) -> i64 (node), module demo.store, line 65
::hgraph::Port<::hgraph::TS<::hgraph::Int>> bid_plus(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x, ::hgraph::Int extra)
{
    return ::hgraph::wire<::hgl::demo::store::m_::n_::bid_plus::impl_>(wiring_, x, extra);
}
}  // namespace w_

namespace n_::seeded
{
// seeded(x: i64) -> i64 (graph), module demo.store, line 69
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x)
    {
        [[maybe_unused]] const ::hglrt::GlobalState global_state{wiring_.global_state()};
        ::hglrt::put_GlobalState_str_V<::hgl::demo::store::m_::Quote>(global_state, ::hgraph::Str{"q"}, ::hgl::demo::store::m_::Quote{5, 6});
        [[maybe_unused]] const ::hgl::demo::store::m_::Quote q = ::hglrt::get_GlobalState_str<::hgl::demo::store::m_::Quote>(global_state, ::hgraph::Str{"q"});
        return ::hgl::demo::store::m_::w_::bid_plus(wiring_, x, (q).bid);
    }
};
}  // namespace n_::seeded
namespace w_
{
// seeded(x: i64) -> i64 (graph), module demo.store, line 69
::hgraph::Port<::hgraph::TS<::hgraph::Int>> seeded(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x)
{
    return ::hgl::demo::store::m_::n_::seeded::impl_::compose(wiring_, x);
}
}  // namespace w_

namespace n_::read_at_build
{
// read_at_build(const values: list<i64>) -> i64 (graph), module demo.store, line 78
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] const ::hglrt::list<::hgraph::Int>& values)
    {
        [[maybe_unused]] const ::hglrt::GlobalState global_state{wiring_.global_state()};
        [[maybe_unused]] const ::hgraph::Int by = ::hglrt::get_GlobalState_str<::hgraph::Int>(global_state, ::hgraph::Str{"by"});
        return ::hgl::demo::store::m_::w_::bid_plus(wiring_, ::hgl::demo::store::m_::w_::ticks(wiring_, values), by);
    }
};
}  // namespace n_::read_at_build
namespace w_
{
// read_at_build(const values: list<i64>) -> i64 (graph), module demo.store, line 78
::hgraph::Port<::hgraph::TS<::hgraph::Int>> read_at_build(::hgraph::Wiring& wiring_, const ::hglrt::list<::hgraph::Int>& values)
{
    return ::hgl::demo::store::m_::n_::read_at_build::impl_::compose(wiring_, values);
}
}  // namespace w_

namespace n_::ticks
{
// ticks(const values: list<i64>) -> i64 (source), module demo.store, line 85
struct impl_
{
    static constexpr auto name = "demo.store::ticks";
    // SOURCE-2: "The first evaluation begins the body".
    static constexpr bool schedule_on_start = true;
    using yielded_type_ = ::hglrt::Yielded<::hgraph::Int>;
    using body_type_ = ::hglrt::generator<yielded_type_>;

    // The body (SOURCE-1), given its configuration by value.
    static body_type_ body_([[maybe_unused]] ::hglrt::list<::hgraph::Int> values)
    {
        for (const ::hgraph::Int& value : values)
        {
            co_yield yielded_type_{::hgraph::TimeDelta{1000000}, value};
        }
    }

    // The node's state (TIME-6): the body, where it is, and the
    // timing of its yields.
    struct state_type_
    {
        ::hglrt::Source<::hgraph::Int> source_{};
        ::std::optional<body_type_> frame_{};
        ::std::optional<body_type_::iterator> at_{};
    };

    static void start(::hgraph::Scalar<"values", ::hglrt::list<::hgraph::Int>> values, ::hgraph::State<state_type_> state_)
    {
        state_.set(state_type_{});
        state_type_& kept_ = state_.modify();
        kept_.frame_.emplace(body_(values.value()));
    }

    // SOURCE-3: what is due now is published, and the node asks for
    // a wake at the next target.
    static void eval([[maybe_unused]] ::hgraph::Scalar<"values", ::hglrt::list<::hgraph::Int>> values, ::hgraph::State<state_type_> state_, ::hgraph::DateTime now_, ::hgraph::NodeScheduler scheduler_, ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
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
}  // namespace n_::ticks
}  // namespace hgl::demo::store::m_

// The state of the node, as hgraph holds it (TIME-6).
template <>
struct hgraph::scalar_descriptor<::hgl::demo::store::m_::n_::ticks::impl_::state_type_>
{
    [[nodiscard]] static constexpr bool is_concrete() noexcept { return true; }
    [[nodiscard]] static const ValueTypeMetaData* value_meta()
    {
        return TypeRegistry::instance().register_scalar<::hgl::demo::store::m_::n_::ticks::impl_::state_type_>("hgl:demo.store::ticks.state");
    }
};

namespace hgl::demo::store::m_
{
namespace w_
{
// ticks(const values: list<i64>) -> i64 (source), module demo.store, line 85
::hgraph::Port<::hgraph::TS<::hgraph::Int>> ticks(::hgraph::Wiring& wiring_, const ::hglrt::list<::hgraph::Int>& values)
{
    return ::hgraph::wire<::hgl::demo::store::m_::n_::ticks::impl_>(wiring_, values);
}
}  // namespace w_

namespace n_::based
{
// based(value: i64) -> Quote (node), module demo.store, line 91
struct impl_
{
    static constexpr auto name = "demo.store::based";

    static void eval([[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::Out<::hgraph::NativeTSB<::hgl::demo::store::m_::Quote>> out_, [[maybe_unused]] ::hgraph::GlobalStateView capability_01_)
    {
        [[maybe_unused]] const ::hglrt::GlobalState global_state{capability_01_};
        // when, line 93 (TIME-3)
        {
            [[maybe_unused]] const ::hgl::demo::store::m_::Quote q = ::hglrt::get_GlobalState_str<::hgl::demo::store::m_::Quote>(global_state, ::hgraph::Str{"base"});
            ::hglrt::publish(out_, [&]() -> ::hgl::demo::store::m_::Quote { const ::hgraph::Int field_00_ = ::hglrt::add__i64_i64((q).bid, ::hglrt::checked_value(value)); const ::hgraph::Int field_01_ = ::hglrt::add__i64_i64((q).ask, ::hglrt::checked_value(value)); return ::hgl::demo::store::m_::Quote{field_00_, field_01_}; }());
            return;
        }
    }
};
}  // namespace n_::based
namespace w_
{
// based(value: i64) -> Quote (node), module demo.store, line 91
::hgraph::Port<::hgraph::NativeTSB<::hgl::demo::store::m_::Quote>> based(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgraph::wire<::hgl::demo::store::m_::n_::based::impl_>(wiring_, value);
}
}  // namespace w_

namespace n_::quotes
{
// quotes(const values: list<i64>) -> Quote (graph), module demo.store, line 99
struct impl_
{
    static ::hgraph::Port<::hgraph::NativeTSB<::hgl::demo::store::m_::Quote>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] const ::hglrt::list<::hgraph::Int>& values)
    {
        return ::hgl::demo::store::m_::w_::based(wiring_, ::hgl::demo::store::m_::w_::ticks(wiring_, values));
    }
};
}  // namespace n_::quotes
namespace w_
{
// quotes(const values: list<i64>) -> Quote (graph), module demo.store, line 99
::hgraph::Port<::hgraph::NativeTSB<::hgl::demo::store::m_::Quote>> quotes(::hgraph::Wiring& wiring_, const ::hglrt::list<::hgraph::Int>& values)
{
    return ::hgl::demo::store::m_::n_::quotes::impl_::compose(wiring_, values);
}
}  // namespace w_

}  // namespace hgl::demo::store::m_

namespace hgl::demo::store::m_::t_
{

void a_struct_is_got_back_in_a_node()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 2}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::store::m_::w_::quoted(wiring_, input_00_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgraph::Int>{2, 3};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
}

void a_tuple_a_dictionary_a_set_and_a_delta_are_got_back()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 2}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::store::m_::w_::paired(wiring_, input_00_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 2};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_04_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 2}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::store::m_::w_::booked(wiring_, input_03_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_05_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 2};
        ::hglrt::testing::expect_same(left_04_, right_05_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_07_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_06_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 2}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::store::m_::w_::counted_members(wiring_, input_06_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_08_ = ::hglrt::testing::Dense<::hgraph::Int>{3, 5};
        ::hglrt::testing::expect_same(left_07_, right_08_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_010_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_09_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 2}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::store::m_::w_::changed(wiring_, input_09_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_011_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 2};
        ::hglrt::testing::expect_same(left_010_, right_011_);
    }();
}

void a_struct_is_got_back_where_a_graph_is_built()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::store::m_::w_::seeded(wiring_, input_00_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgraph::Int>{6};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
}

}  // namespace hgl::demo::store::m_::t_
