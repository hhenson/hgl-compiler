// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include "demo.apps.h"
#include "core.h"

// Names this file writes, which no macro of a header above takes.
#undef GlobalState
#undef apps
#undef bounded
#undef core
#undef demo
#undef every
#undef global_state
#undef kept
#undef limit
#undef limited
#undef quiet
#undef record
#undef reset
#undef seeded
#undef seen
#undef start
#undef the_store_is_set_where_the_graph_is_built
#undef ticks
#undef value
#undef values

namespace hgl::demo::apps::m_
{

namespace n_::ticks
{
// ticks(const values: list<i64>, const every: duration = ...) -> i64 (source), module demo.apps, line 8
struct impl_
{
    static constexpr auto name = "demo.apps::ticks";
    // SOURCE-2: "The first evaluation begins the body".
    static constexpr bool schedule_on_start = true;
    using yielded_type_ = ::hglrt::Yielded<::hgraph::Int>;
    using body_type_ = ::hglrt::generator<yielded_type_>;

    // The body (SOURCE-1), given its configuration by value.
    static body_type_ body_([[maybe_unused]] ::hglrt::list<::hgraph::Int> values, [[maybe_unused]] ::hgraph::TimeDelta every)
    {
        for (const ::hgraph::Int& value : values)
        {
            co_yield yielded_type_{every, value};
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

    static void start(::hgraph::Scalar<"values", ::hglrt::list<::hgraph::Int>> values, ::hgraph::Scalar<"every", ::hgraph::TimeDelta> every, ::hgraph::State<state_type_> state_)
    {
        state_.set(state_type_{});
        state_type_& kept_ = state_.modify();
        kept_.frame_.emplace(body_(values.value(), every.value()));
    }

    // SOURCE-3: what is due now is published, and the node asks for
    // a wake at the next target.
    static void eval([[maybe_unused]] ::hgraph::Scalar<"values", ::hglrt::list<::hgraph::Int>> values, [[maybe_unused]] ::hgraph::Scalar<"every", ::hgraph::TimeDelta> every, ::hgraph::State<state_type_> state_, ::hgraph::DateTime now_, ::hgraph::NodeScheduler scheduler_, ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
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
}  // namespace hgl::demo::apps::m_

// The state of the node, as hgraph holds it (TIME-6).
template <>
struct hgraph::scalar_descriptor<::hgl::demo::apps::m_::n_::ticks::impl_::state_type_>
{
    [[nodiscard]] static constexpr bool is_concrete() noexcept { return true; }
    [[nodiscard]] static const ValueTypeMetaData* value_meta()
    {
        return TypeRegistry::instance().register_scalar<::hgl::demo::apps::m_::n_::ticks::impl_::state_type_>("hgl:demo.apps::ticks.state");
    }
};

namespace hgl::demo::apps::m_
{
namespace w_
{
// ticks(const values: list<i64>, const every: duration = ...) -> i64 (source), module demo.apps, line 8
::hgraph::Port<::hgraph::TS<::hgraph::Int>> ticks(::hgraph::Wiring& wiring_, const ::hglrt::list<::hgraph::Int>& values, ::hgraph::TimeDelta every)
{
    return ::hgraph::wire<::hgl::demo::apps::m_::n_::ticks::impl_>(wiring_, values, every);
}
}  // namespace w_

namespace n_::limited
{
// limited(value: i64) -> i64 (node), module demo.apps, line 15
struct impl_
{
    static constexpr auto name = "demo.apps::limited";

    static void eval([[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_, [[maybe_unused]] ::hgraph::GlobalStateView capability_01_)
    {
        [[maybe_unused]] const ::hglrt::GlobalState global_state{capability_01_};
        // when, line 17 (TIME-3)
        {
            if (::hglrt::le__T_T<::hgraph::Int>(::hglrt::checked_value(value), ::hglrt::get_GlobalState_str<::hgraph::Int>(global_state, ::hgraph::Str{"limit"})))
            {
                out_.set(::hglrt::checked_value(value));
                return;
            }
        }
    }
};
}  // namespace n_::limited
namespace w_
{
// limited(value: i64) -> i64 (node), module demo.apps, line 15
::hgraph::Port<::hgraph::TS<::hgraph::Int>> limited(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgraph::wire<::hgl::demo::apps::m_::n_::limited::impl_>(wiring_, value);
}
}  // namespace w_

namespace n_::seeded
{
// seeded(const limit: i64, const values: list<i64>) -> i64 (graph), module demo.apps, line 24
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Int limit, [[maybe_unused]] const ::hglrt::list<::hgraph::Int>& values)
    {
        [[maybe_unused]] const ::hglrt::GlobalState global_state{wiring_.global_state()};
        ::hglrt::put_GlobalState_str_V<::hgraph::Int>(global_state, ::hgraph::Str{"limit"}, limit);
        [[maybe_unused]] const ::hgraph::Int seen = ::hglrt::get_GlobalState_str<::hgraph::Int>(global_state, ::hgraph::Str{"limit"});
        return ::hgl::demo::apps::m_::w_::limited(wiring_, ::hgl::demo::apps::m_::w_::ticks(wiring_, values, ::hgl::core::m_::c_::mul_1_(seen, ::hgraph::TimeDelta{1000000})));
    }
};
}  // namespace n_::seeded
namespace w_
{
// seeded(const limit: i64, const values: list<i64>) -> i64 (graph), module demo.apps, line 24
::hgraph::Port<::hgraph::TS<::hgraph::Int>> seeded(::hgraph::Wiring& wiring_, ::hgraph::Int limit, const ::hglrt::list<::hgraph::Int>& values)
{
    return ::hgl::demo::apps::m_::n_::seeded::impl_::compose(wiring_, limit, values);
}
}  // namespace w_

namespace n_::bounded
{
// bounded(const values: list<i64>) -> i64 (graph), module demo.apps, line 32
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] const ::hglrt::list<::hgraph::Int>& values)
    {
        return ::hgl::demo::apps::m_::w_::limited(wiring_, ::hgl::demo::apps::m_::w_::ticks(wiring_, values, ::hgraph::TimeDelta{1000000}));
    }
};
}  // namespace n_::bounded
namespace w_
{
// bounded(const values: list<i64>) -> i64 (graph), module demo.apps, line 32
::hgraph::Port<::hgraph::TS<::hgraph::Int>> bounded(::hgraph::Wiring& wiring_, const ::hglrt::list<::hgraph::Int>& values)
{
    return ::hgl::demo::apps::m_::n_::bounded::impl_::compose(wiring_, values);
}
}  // namespace w_

namespace n_::kept
{
// kept(value: i64) (node), module demo.apps, line 37
struct impl_
{
    static constexpr auto name = "demo.apps::kept";

    static void eval([[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::GlobalStateView capability_01_)
    {
        [[maybe_unused]] const ::hglrt::GlobalState global_state{capability_01_};
        // when, line 39 (TIME-3)
        {
            ::hglrt::put_GlobalState_str_V<::hgraph::Int>(global_state, ::hgraph::Str{"last"}, ::hglrt::checked_value(value));
        }
    }
};
}  // namespace n_::kept
namespace w_
{
// kept(value: i64) (node), module demo.apps, line 37
void kept(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    ::hgraph::wire<::hgl::demo::apps::m_::n_::kept::impl_>(wiring_, value);
}
}  // namespace w_

namespace n_::quiet
{
// quiet(const values: list<i64>) (graph), module demo.apps, line 43
struct impl_
{
    static void compose(::hgraph::Wiring& wiring_, [[maybe_unused]] const ::hglrt::list<::hgraph::Int>& values)
    {
        ::hgl::demo::apps::m_::w_::kept(wiring_, ::hgl::demo::apps::m_::w_::ticks(wiring_, values, ::hgraph::TimeDelta{1000000}));
    }
};
}  // namespace n_::quiet
namespace w_
{
// quiet(const values: list<i64>) (graph), module demo.apps, line 43
void quiet(::hgraph::Wiring& wiring_, const ::hglrt::list<::hgraph::Int>& values)
{
    ::hgl::demo::apps::m_::n_::quiet::impl_::compose(wiring_, values);
}
}  // namespace w_

}  // namespace hgl::demo::apps::m_

namespace hgl::demo::apps::m_::t_
{

void the_store_is_set_where_the_graph_is_built()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_00_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::apps::m_::w_::seeded(wiring_, 2, ::hglrt::list<::hgraph::Int>{1, 2, 3}), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_01_ = ::hglrt::testing::Timed<::hgraph::Int>{{{::hglrt::testing::TraceKey{::hgraph::TimeDelta{2000000}}, 1}, {::hglrt::testing::TraceKey{::hgraph::TimeDelta{4000000}}, 2}}};
        ::hglrt::testing::expect_same(left_00_, right_01_);
    }();
}

}  // namespace hgl::demo::apps::m_::t_
