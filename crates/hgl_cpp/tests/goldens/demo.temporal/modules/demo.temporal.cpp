// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include "demo.temporal.h"
#include "core.h"

// Names this file writes, which no macro of a header above takes.
#undef Clock
#undef Logger
#undef a
#undef a_held_value_is_as_it_was
#undef a_run_is_bounded
#undef add_one
#undef adds_one
#undef an_outputless_target_runs
#undef any_name_is_written
#undef b
#undef bumped
#undef chosen
#undef clock
#undef compose
#undef core
#undef count
#undef count_publications
#undef counted
#undef counted_ticks
#undef demo
#undef earliest_start
#undef first
#undef first_of
#undef flags
#undef handlers_run_in_order
#undef item
#undef length
#undef lhs
#undef log_debug
#undef logger
#undef name
#undef next
#undef noted
#undef one
#undef opening
#undef pick
#undef record
#undef replay
#undef reset
#undef rhs
#undef scaled
#undef second
#undef seen
#undef sources_and_constants
#undef stamped
#undef start
#undef temporal
#undef the_clock_is_read
#undef ticks
#undef value
#undef values

namespace hgl::demo::temporal::m_
{

namespace n_::add_one
{
// add_one(value: i64) -> i64 (graph), module demo.temporal, line 7
struct impl_
{
    // The call of `add_` on line 8, lifted (FN-6).
    struct lifted_00_
    {
        static constexpr auto name = "demo.temporal:add_";
        static void eval([[maybe_unused]] ::hgraph::In<"lhs", ::hgraph::TS<::hgraph::Int>> lhs, [[maybe_unused]] ::hgraph::Scalar<"rhs", ::hgraph::Int> rhs, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
        {
            out_.set(::hglrt::add__i64_i64(::hglrt::checked_value(lhs), rhs.value()));
            return;
        }
    };

    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
    {
        return ::hgraph::wire<lifted_00_>(wiring_, value, static_cast<::hgraph::Int>(1));
    }
};
}  // namespace n_::add_one
namespace w_
{
// add_one(value: i64) -> i64 (graph), module demo.temporal, line 7
::hgraph::Port<::hgraph::TS<::hgraph::Int>> add_one(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgl::demo::temporal::m_::n_::add_one::impl_::compose(wiring_, value);
}
}  // namespace w_

namespace n_::scaled
{
// scaled(value: f64) -> f64 (graph), module demo.temporal, line 12
struct impl_
{
    // The call of `mul_` on line 13, lifted (FN-6).
    struct lifted_00_
    {
        static constexpr auto name = "demo.temporal:mul_";
        static void eval([[maybe_unused]] ::hgraph::In<"lhs", ::hgraph::TS<::hgraph::Float>> lhs, [[maybe_unused]] ::hgraph::Scalar<"rhs", ::hgraph::Float> rhs, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Float>> out_)
        {
            out_.set(::hglrt::mul__f64_f64(::hglrt::checked_value(lhs), rhs.value()));
            return;
        }
    };

    static ::hgraph::Port<::hgraph::TS<::hgraph::Float>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Float>> value)
    {
        return [&]() {
        // hgraph issue 1866: wired as a node of its own, since hgraph would take
        // two whose configurations hold `0.0` and `-0.0` for one.
        ::hgraph::Value scalars_{::hgraph::value_type_for_wiring(::hgraph::StaticNodeSignature<lifted_00_>::scalar_schema(), wiring_.realization_options())};
        {
            auto configured_ = scalars_.as_bundle().begin_mutation();
            configured_["rhs"].set(static_cast<::hgraph::Float>(2.5));
        }
        ::hgraph::NodeBuilder builder_;
        builder_.implementation<lifted_00_>();
        const ::std::array<::hgraph::WiringPortRef, 1> inputs_{value.erased()};
        return ::hgraph::Port<::hgraph::TS<::hgraph::Float>>{wiring_, wiring_.add_unique_node(typeid(lifted_00_), ::std::move(builder_), ::std::span<const ::hgraph::WiringPortRef>{inputs_}, ::std::move(scalars_))};
    }();
    }
};
}  // namespace n_::scaled
namespace w_
{
// scaled(value: f64) -> f64 (graph), module demo.temporal, line 12
::hgraph::Port<::hgraph::TS<::hgraph::Float>> scaled(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Float>> value)
{
    return ::hgl::demo::temporal::m_::n_::scaled::impl_::compose(wiring_, value);
}
}  // namespace w_

namespace n_::count_publications
{
// count_publications(value: i64) -> i64 (node), module demo.temporal, line 17
struct impl_
{
    static constexpr auto name = "demo.temporal::count_publications";

    // The node's `state` and `cache` (TIME-6).
    struct state_type_
    {
        ::hgraph::Int count{};
    };

    static void start([[maybe_unused]] ::hgraph::State<state_type_> state_)
    {
        state_.set(state_type_{});
        state_type_& kept_ = state_.modify();
        [[maybe_unused]] auto& count = kept_.count;
        count = 0;
    }

    static void eval([[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::State<state_type_> state_, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        state_type_& kept_ = state_.modify();
        [[maybe_unused]] auto& count = kept_.count;
        // when, line 19 (TIME-3)
        {
            count = ::hglrt::add__i64_i64(count, 1);
            out_.set(count);
            return;
        }
    }
};
}  // namespace n_::count_publications
}  // namespace hgl::demo::temporal::m_

// The state of the node, as hgraph holds it (TIME-6).
template <>
struct hgraph::scalar_descriptor<::hgl::demo::temporal::m_::n_::count_publications::impl_::state_type_>
{
    [[nodiscard]] static constexpr bool is_concrete() noexcept { return true; }
    [[nodiscard]] static const ValueTypeMetaData* value_meta()
    {
        return TypeRegistry::instance().register_scalar<::hgl::demo::temporal::m_::n_::count_publications::impl_::state_type_>("hgl:demo.temporal::count_publications.state");
    }
};

namespace hgl::demo::temporal::m_
{
namespace w_
{
// count_publications(value: i64) -> i64 (node), module demo.temporal, line 17
::hgraph::Port<::hgraph::TS<::hgraph::Int>> count_publications(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgraph::wire<::hgl::demo::temporal::m_::n_::count_publications::impl_>(wiring_, value);
}
}  // namespace w_

namespace n_::first_of
{
// first_of(a: i64, b: i64) -> i64 (node), module demo.temporal, line 27
struct impl_
{
    static constexpr auto name = "demo.temporal::first_of";

    static void eval([[maybe_unused]] ::hgraph::In<"a", ::hgraph::TS<::hgraph::Int>, ::hgraph::InputValidity::Unchecked> a, [[maybe_unused]] ::hgraph::In<"b", ::hgraph::TS<::hgraph::Int>, ::hgraph::InputValidity::Unchecked> b, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 28 (TIME-3)
        if (::hglrt::fn_valid(a) && ::hglrt::fn_valid(b) && ::hglrt::fn_modified(a))
        {
            out_.set(::hglrt::fn_delta_value(a));
            return;
        }
        // when, line 29 (TIME-3)
        if (::hglrt::fn_modified(b))
        {
            out_.set(::hglrt::mul__i64_i64(::hglrt::checked_value(b), 10));
            return;
        }
    }
};
}  // namespace n_::first_of
namespace w_
{
// first_of(a: i64, b: i64) -> i64 (node), module demo.temporal, line 27
::hgraph::Port<::hgraph::TS<::hgraph::Int>> first_of(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> a, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> b)
{
    return ::hgraph::wire<::hgl::demo::temporal::m_::n_::first_of::impl_>(wiring_, a, b);
}
}  // namespace w_

namespace n_::ticks
{
// ticks(const values: list<i64>) -> i64 (source), module demo.temporal, line 33
struct impl_
{
    static constexpr auto name = "demo.temporal::ticks";
    // SOURCE-2: "The first evaluation begins the body".
    static constexpr bool schedule_on_start = true;
    using yielded_type_ = ::hglrt::Yielded<::hgraph::Int>;
    using body_type_ = ::hglrt::generator<yielded_type_>;

    // The body (SOURCE-1), given its configuration by value.
    static body_type_ body_([[maybe_unused]] ::hglrt::list<::hgraph::Int> values)
    {
        for (const ::hgraph::Int& value : values)
        {
            co_yield yielded_type_{::hgraph::TimeDelta{1}, value};
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
}  // namespace hgl::demo::temporal::m_

// The state of the node, as hgraph holds it (TIME-6).
template <>
struct hgraph::scalar_descriptor<::hgl::demo::temporal::m_::n_::ticks::impl_::state_type_>
{
    [[nodiscard]] static constexpr bool is_concrete() noexcept { return true; }
    [[nodiscard]] static const ValueTypeMetaData* value_meta()
    {
        return TypeRegistry::instance().register_scalar<::hgl::demo::temporal::m_::n_::ticks::impl_::state_type_>("hgl:demo.temporal::ticks.state");
    }
};

namespace hgl::demo::temporal::m_
{
namespace w_
{
// ticks(const values: list<i64>) -> i64 (source), module demo.temporal, line 33
::hgraph::Port<::hgraph::TS<::hgraph::Int>> ticks(::hgraph::Wiring& wiring_, const ::hglrt::list<::hgraph::Int>& values)
{
    return ::hgraph::wire<::hgl::demo::temporal::m_::n_::ticks::impl_>(wiring_, values);
}
}  // namespace w_

namespace n_::counted_ticks
{
// counted_ticks(const values: list<i64>) -> i64 (graph), module demo.temporal, line 40
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] const ::hglrt::list<::hgraph::Int>& values)
    {
        [[maybe_unused]] const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> counted = ::hgl::demo::temporal::m_::w_::count_publications(wiring_, ::hgl::demo::temporal::m_::w_::ticks(wiring_, values));
        return ::hgl::demo::temporal::m_::w_::first_of(wiring_, counted, ::hglrt::constant<::hgraph::Int>(wiring_, 5));
    }
};
}  // namespace n_::counted_ticks
namespace w_
{
// counted_ticks(const values: list<i64>) -> i64 (graph), module demo.temporal, line 40
::hgraph::Port<::hgraph::TS<::hgraph::Int>> counted_ticks(::hgraph::Wiring& wiring_, const ::hglrt::list<::hgraph::Int>& values)
{
    return ::hgl::demo::temporal::m_::n_::counted_ticks::impl_::compose(wiring_, values);
}
}  // namespace w_

namespace n_::stamped
{
// stamped(value: i64) -> datetime (node), module demo.temporal, line 46
struct impl_
{
    static constexpr auto name = "demo.temporal::stamped";

    static void eval([[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::DateTime>> out_, [[maybe_unused]] ::hgraph::EvaluationClockView capability_01_)
    {
        [[maybe_unused]] const ::hglrt::Clock clock{capability_01_};
        // when, line 48 (TIME-3)
        {
            out_.set((clock).evaluation_time());
            return;
        }
    }
};
}  // namespace n_::stamped
namespace w_
{
// stamped(value: i64) -> datetime (node), module demo.temporal, line 46
::hgraph::Port<::hgraph::TS<::hgraph::DateTime>> stamped(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgraph::wire<::hgl::demo::temporal::m_::n_::stamped::impl_>(wiring_, value);
}
}  // namespace w_

namespace n_::noted
{
// noted(value: i64) (node), module demo.temporal, line 52
struct impl_
{
    static constexpr auto name = "demo.temporal::noted";

    static void eval([[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::LoggerView capability_01_)
    {
        [[maybe_unused]] const ::hglrt::Logger logger{capability_01_};
        // when, line 54 (TIME-3)
        {
            ::hgl::core::m_::log_debug(logger, ::hgraph::Str{"noted"});
        }
    }
};
}  // namespace n_::noted
namespace w_
{
// noted(value: i64) (node), module demo.temporal, line 52
void noted(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    ::hgraph::wire<::hgl::demo::temporal::m_::n_::noted::impl_>(wiring_, value);
}
}  // namespace w_

namespace n_::name
{
// name(value: i64) -> i64 (node), module demo.temporal, line 58
struct impl_
{
    static constexpr auto name = "demo.temporal::name";

    static void eval([[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 59 (TIME-3)
        {
            out_.set(::hglrt::checked_value(value));
            return;
        }
    }
};
}  // namespace n_::name
namespace w_
{
// name(value: i64) -> i64 (node), module demo.temporal, line 58
::hgraph::Port<::hgraph::TS<::hgraph::Int>> name(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgraph::wire<::hgl::demo::temporal::m_::n_::name::impl_>(wiring_, value);
}
}  // namespace w_

namespace n_::compose
{
// compose(value: i64) -> i64 (graph), module demo.temporal, line 62
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
    {
        return ::hgl::demo::temporal::m_::w_::name(wiring_, value);
    }
};
}  // namespace n_::compose
namespace w_
{
// compose(value: i64) -> i64 (graph), module demo.temporal, line 62
::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgl::demo::temporal::m_::n_::compose::impl_::compose(wiring_, value);
}
}  // namespace w_

namespace n_::pick
{
// pick(value: bool, const first: bool, const second: bool) -> bool (node), module demo.temporal, line 68
struct impl_
{
    static constexpr auto name = "demo.temporal::pick";

    static void eval([[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Bool>> value, [[maybe_unused]] ::hgraph::Scalar<"first", ::hgraph::Bool> first, [[maybe_unused]] ::hgraph::Scalar<"second", ::hgraph::Bool> second, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Bool>> out_)
    {
        // when, line 69 (TIME-3)
        {
            out_.set(first.value());
            return;
        }
    }
};
}  // namespace n_::pick
namespace w_
{
// pick(value: bool, const first: bool, const second: bool) -> bool (node), module demo.temporal, line 68
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> pick(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> value, ::hgraph::Bool first, ::hgraph::Bool second)
{
    return ::hgraph::wire<::hgl::demo::temporal::m_::n_::pick::impl_>(wiring_, value, first, second);
}
}  // namespace w_

namespace n_::chosen
{
// chosen(value: bool) -> bool (graph), module demo.temporal, line 72
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> value)
    {
        [[maybe_unused]] ::hglrt::list<::hgraph::Bool> flags = ::hglrt::list<::hgraph::Bool>{false};
        return [&] { const auto argument_00_ = value; const ::hgraph::Bool argument_01_ = ::hglrt::item(flags, 0); const ::hgraph::Bool argument_02_ = [&]() -> ::hgraph::Bool {
    ::hglrt::item(flags, 0) = true;
    return true;
}(); return ::hgl::demo::temporal::m_::w_::pick(wiring_, argument_00_, argument_01_, argument_02_); }();
    }
};
}  // namespace n_::chosen
namespace w_
{
// chosen(value: bool) -> bool (graph), module demo.temporal, line 72
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> chosen(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> value)
{
    return ::hgl::demo::temporal::m_::n_::chosen::impl_::compose(wiring_, value);
}
}  // namespace w_

namespace n_::bumped
{
// bumped(value: i64) -> i64 (node), module demo.temporal, line 81
struct impl_
{
    static constexpr auto name = "demo.temporal::bumped";

    static void eval([[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 82 (TIME-3)
        {
            [[maybe_unused]] const ::hgraph::Int next = [&]() -> ::hgraph::Int {
    [[maybe_unused]] const ::hgraph::Int one = 1;
    return ::hglrt::add__i64_i64(::hglrt::checked_value(value), one);
}();
            out_.set(next);
            return;
        }
    }
};
}  // namespace n_::bumped
namespace w_
{
// bumped(value: i64) -> i64 (node), module demo.temporal, line 81
::hgraph::Port<::hgraph::TS<::hgraph::Int>> bumped(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgraph::wire<::hgl::demo::temporal::m_::n_::bumped::impl_>(wiring_, value);
}
}  // namespace w_

}  // namespace hgl::demo::temporal::m_

namespace hgl::demo::temporal::m_::t_
{

void adds_one()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, ::std::nullopt, 3}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::temporal::m_::w_::add_one(wiring_, input_00_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgraph::Int>{2, ::std::nullopt, 4};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_04_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::temporal::m_::w_::add_one(wiring_, input_03_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_05_ = ::hglrt::testing::Dense<::hgraph::Int>{1};
        ::hglrt::testing::expect_different(left_04_, right_05_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_07_ = ::hglrt::testing::evaluate<::hgraph::Float>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Float>> input_06_ = ::hgl::core::m_::w_::i_::replay::of_f64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Float>{2.0}, start_));
        ::hgl::core::m_::w_::i_::record::of_f64(wiring_, ::hgl::demo::temporal::m_::w_::scaled(wiring_, input_06_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_08_ = ::hglrt::testing::Dense<::hgraph::Float>{5.0};
        ::hglrt::testing::expect_same(left_07_, right_08_);
    }();
}

void handlers_run_in_order()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_06_ = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Int>{1, ::std::nullopt, 3};
        const auto trace_02_ = ::hglrt::testing::Dense<::hgraph::Int>{::std::nullopt, 2, 4};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_02_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_04_ = input_01_; const auto argument_05_ = input_03_; return ::hgl::demo::temporal::m_::w_::first_of(wiring_, argument_04_, argument_05_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_07_ = ::hglrt::testing::Dense<::hgraph::Int>{::std::nullopt, 20, 3};
        ::hglrt::testing::expect_same(left_06_, right_07_);
    }();
}

void sources_and_constants()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_00_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::temporal::m_::w_::counted_ticks(wiring_, ::hglrt::list<::hgraph::Int>{10, 20, 30}), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_01_ = ::hglrt::testing::Dense<::hgraph::Int>{50, 1, 2, 3};
        ::hglrt::testing::expect_same(left_00_, right_01_);
    }();
}

void the_clock_is_read()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgraph::DateTime>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Timed<::hgraph::Int>{{{::hglrt::testing::TraceKey{::hgraph::TimeDelta{0}}, 7}, {::hglrt::testing::TraceKey{::hgraph::TimeDelta{2000000}}, 9}}}, start_));
        ::hgl::core::m_::w_::i_::record::of_datetime(wiring_, ::hgl::demo::temporal::m_::w_::stamped(wiring_, input_00_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Timed<::hgraph::DateTime>{{{::hglrt::testing::TraceKey{::hgraph::TimeDelta{0}}, ::hgl::core::m_::earliest_start()}, {::hglrt::testing::TraceKey{::hgraph::TimeDelta{2000000}}, ::hglrt::add__datetime_duration(::hgl::core::m_::earliest_start(), ::hgraph::TimeDelta{2000000})}}};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
}

void an_outputless_target_runs()
{
    ::hglrt::testing::statement();
    static_cast<void>(::hglrt::testing::run([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 2}, start_));
        ::hgl::demo::temporal::m_::w_::noted(wiring_, input_00_);
    }));
}

void a_run_is_bounded()
{
    ::hglrt::testing::statement();
    [[maybe_unused]] const ::hgraph::DateTime opening = ::hgraph::DateTime{::std::chrono::microseconds{1767605400000000}};
    ::hglrt::testing::statement();
    [[maybe_unused]] const auto seen = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 2, 3};
        const auto bounds_02_ = ::hglrt::testing::Bounds{.start = opening, .length = ::hgraph::TimeDelta{2}};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::temporal::m_::w_::add_one(wiring_, input_01_), ::hgraph::Str{::hglrt::testing::run_output});
    }, bounds_02_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_03_ = seen;
        const auto right_04_ = ::hglrt::testing::Dense<::hgraph::Int>{2, 3};
        ::hglrt::testing::expect_same(left_03_, right_04_);
    }();
}

void any_name_is_written()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{4}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::temporal::m_::w_::compose(wiring_, input_00_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgraph::Int>{4};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
}

void a_held_value_is_as_it_was()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgraph::Bool>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Bool>{true}, start_));
        ::hgl::core::m_::w_::i_::record::of_bool(wiring_, ::hgl::demo::temporal::m_::w_::chosen(wiring_, input_00_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgraph::Bool>{false};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_04_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 2}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::temporal::m_::w_::bumped(wiring_, input_03_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_05_ = ::hglrt::testing::Dense<::hgraph::Int>{2, 3};
        ::hglrt::testing::expect_same(left_04_, right_05_);
    }();
}

}  // namespace hgl::demo::temporal::m_::t_
