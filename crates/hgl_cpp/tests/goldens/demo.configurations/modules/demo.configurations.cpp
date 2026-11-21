// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include "demo.configurations.h"
#include "core.h"

// Names this file writes, which no macro of a header above takes.
#undef Holder
#undef Noted
#undef Tagged
#undef ZeroPolicy
#undef a
#undef above
#undef b
#undef bid
#undef book
#undef booked
#undef both
#undef branch
#undef by
#undef checked
#undef configurations
#undef configurations_that_differ_are_two_nodes
#undef core
#undef demo
#undef differ
#undef fallback
#undef first
#undef fixed
#undef flag
#undef found
#undef from_book
#undef from_members
#undef fs
#undef get
#undef h
#undef held_first
#undef held_or
#undef holders
#undef hs
#undef ieee
#undef item
#undef key
#undef lhs
#undef lifted_calls_and_branches_are_given_any_value
#undef looked
#undef m
#undef member
#undef member_of
#undef members
#undef n
#undef nodes_are_given_any_value
#undef note
#undef noted_bid
#undef nudged
#undef pair_or
#undef paired
#undef probe
#undef q
#undef record
#undef replay
#undef reset
#undef rhs
#undef shifted
#undef signs
#undef sources_are_given_a_set_and_a_dictionary
#undef start
#undef summed
#undef t
#undef tagged
#undef tagged_first
#undef tagged_or
#undef ts
#undef value
#undef values
#undef x
#undef xs

namespace hgl::demo::configurations::m_
{

namespace n_::from_members
{
// from_members(const members: set<i64>) -> i64 (source), module demo.configurations, line 24
struct impl_
{
    static constexpr auto name = "demo.configurations::from_members";
    // SOURCE-2: "The first evaluation begins the body".
    static constexpr bool schedule_on_start = true;
    using yielded_type_ = ::hglrt::Yielded<::hgraph::Int>;
    using body_type_ = ::hglrt::generator<yielded_type_>;

    // The body (SOURCE-1), given its configuration by value.
    static body_type_ body_([[maybe_unused]] ::hglrt::set<::hgraph::Int> members)
    {
        for ([[maybe_unused]] const ::hgraph::Int& member : members)
        {
            co_yield yielded_type_{::hgraph::TimeDelta{1000000}, member};
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

    static void start(::hgraph::Scalar<"members", ::hglrt::configured<::hglrt::set<::hgraph::Int>>> members, ::hgraph::State<state_type_> state_)
    {
        state_.set(state_type_{});
        state_type_& kept_ = state_.modify();
        kept_.frame_.emplace(body_(members.value().held));
    }

    // SOURCE-3: what is due now is published, and the node asks for
    // a wake at the next target.
    static void eval([[maybe_unused]] ::hgraph::Scalar<"members", ::hglrt::configured<::hglrt::set<::hgraph::Int>>> members, ::hgraph::State<state_type_> state_, ::hgraph::DateTime now_, ::hgraph::NodeScheduler scheduler_, ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
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
}  // namespace n_::from_members
}  // namespace hgl::demo::configurations::m_

// The state of the node, as hgraph holds it (TIME-6).
template <>
struct hgraph::scalar_descriptor<::hgl::demo::configurations::m_::n_::from_members::impl_::state_type_>
{
    [[nodiscard]] static constexpr bool is_concrete() noexcept { return true; }
    [[nodiscard]] static const ValueTypeMetaData* value_meta()
    {
        return TypeRegistry::instance().register_scalar<::hgl::demo::configurations::m_::n_::from_members::impl_::state_type_>("hgl:demo.configurations::from_members.state");
    }
};

namespace hgl::demo::configurations::m_
{
namespace w_
{
// from_members(const members: set<i64>) -> i64 (source), module demo.configurations, line 24
::hgraph::Port<::hgraph::TS<::hgraph::Int>> from_members(::hgraph::Wiring& wiring_, const ::hglrt::set<::hgraph::Int>& members)
{
    return ::hgraph::wire<::hgl::demo::configurations::m_::n_::from_members::impl_>(wiring_, ::hglrt::configured<::hglrt::set<::hgraph::Int>>{members});
}
}  // namespace w_

namespace n_::from_book
{
// from_book(const book: dict<str, i64>) -> i64 (source), module demo.configurations, line 30
struct impl_
{
    static constexpr auto name = "demo.configurations::from_book";
    // SOURCE-2: "The first evaluation begins the body".
    static constexpr bool schedule_on_start = true;
    using yielded_type_ = ::hglrt::Yielded<::hgraph::Int>;
    using body_type_ = ::hglrt::generator<yielded_type_>;

    // The body (SOURCE-1), given its configuration by value.
    static body_type_ body_([[maybe_unused]] ::hglrt::dict<::hgraph::Str, ::hgraph::Int> book)
    {
        for ([[maybe_unused]] const auto& [key, value] : book)
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

    static void start(::hgraph::Scalar<"book", ::hglrt::configured<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>> book, ::hgraph::State<state_type_> state_)
    {
        state_.set(state_type_{});
        state_type_& kept_ = state_.modify();
        kept_.frame_.emplace(body_(book.value().held));
    }

    // SOURCE-3: what is due now is published, and the node asks for
    // a wake at the next target.
    static void eval([[maybe_unused]] ::hgraph::Scalar<"book", ::hglrt::configured<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>> book, ::hgraph::State<state_type_> state_, ::hgraph::DateTime now_, ::hgraph::NodeScheduler scheduler_, ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
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
}  // namespace n_::from_book
}  // namespace hgl::demo::configurations::m_

// The state of the node, as hgraph holds it (TIME-6).
template <>
struct hgraph::scalar_descriptor<::hgl::demo::configurations::m_::n_::from_book::impl_::state_type_>
{
    [[nodiscard]] static constexpr bool is_concrete() noexcept { return true; }
    [[nodiscard]] static const ValueTypeMetaData* value_meta()
    {
        return TypeRegistry::instance().register_scalar<::hgl::demo::configurations::m_::n_::from_book::impl_::state_type_>("hgl:demo.configurations::from_book.state");
    }
};

namespace hgl::demo::configurations::m_
{
namespace w_
{
// from_book(const book: dict<str, i64>) -> i64 (source), module demo.configurations, line 30
::hgraph::Port<::hgraph::TS<::hgraph::Int>> from_book(::hgraph::Wiring& wiring_, const ::hglrt::dict<::hgraph::Str, ::hgraph::Int>& book)
{
    return ::hgraph::wire<::hgl::demo::configurations::m_::n_::from_book::impl_>(wiring_, ::hglrt::configured<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>{book});
}
}  // namespace w_

namespace n_::shifted
{
// shifted(x: i64, const by: tuple<i64, i64>) -> i64 (node), module demo.configurations, line 39
struct impl_
{
    static constexpr auto name = "demo.configurations::shifted";

    static void eval([[maybe_unused]] ::hgraph::In<"x", ::hgraph::TS<::hgraph::Int>> x, [[maybe_unused]] ::hgraph::Scalar<"by", ::hglrt::configured<::std::tuple<::hgraph::Int, ::hgraph::Int>>> by, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 40 (TIME-3)
        {
            out_.set([&] { const ::hgraph::Int argument_00_ = ::hglrt::add__i64_i64(::hglrt::checked_value(x), ::std::get<0>(by.value().held)); const ::hgraph::Int& argument_01_ = ::std::get<1>(by.value().held); return ::hglrt::add__i64_i64(argument_00_, argument_01_); }());
            return;
        }
    }
};
}  // namespace n_::shifted
namespace w_
{
// shifted(x: i64, const by: tuple<i64, i64>) -> i64 (node), module demo.configurations, line 39
::hgraph::Port<::hgraph::TS<::hgraph::Int>> shifted(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x, const ::std::tuple<::hgraph::Int, ::hgraph::Int>& by)
{
    return ::hgraph::wire<::hgl::demo::configurations::m_::n_::shifted::impl_>(wiring_, x, ::hglrt::configured<::std::tuple<::hgraph::Int, ::hgraph::Int>>{by});
}
}  // namespace w_

namespace n_::first
{
// first(probe: bool, const h: Holder) -> i64 (node), module demo.configurations, line 43
struct impl_
{
    static constexpr auto name = "demo.configurations::first";

    static void eval([[maybe_unused]] ::hgraph::In<"probe", ::hgraph::TS<::hgraph::Bool>> probe, [[maybe_unused]] ::hgraph::Scalar<"h", ::hglrt::configured<::hgl::demo::configurations::m_::Holder>> h, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 44 (TIME-3)
        {
            out_.set(::hglrt::add__i64_i64(::hglrt::item((h.value().held).xs, 0), (h.value().held).n));
            return;
        }
    }
};
}  // namespace n_::first
namespace w_
{
// first(probe: bool, const h: Holder) -> i64 (node), module demo.configurations, line 43
::hgraph::Port<::hgraph::TS<::hgraph::Int>> first(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, const ::hgl::demo::configurations::m_::Holder& h)
{
    return ::hgraph::wire<::hgl::demo::configurations::m_::n_::first::impl_>(wiring_, probe, ::hglrt::configured<::hgl::demo::configurations::m_::Holder>{h});
}
}  // namespace w_

namespace n_::noted_bid
{
// noted_bid(x: i64, const q: Noted) -> i64 (node), module demo.configurations, line 47
struct impl_
{
    static constexpr auto name = "demo.configurations::noted_bid";

    static void eval([[maybe_unused]] ::hgraph::In<"x", ::hgraph::TS<::hgraph::Int>> x, [[maybe_unused]] ::hgraph::Scalar<"q", ::hglrt::configured<::hgl::demo::configurations::m_::Noted>> q, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 48 (TIME-3)
        {
            if ((!((q.value().held).note).has_value()))
            {
                out_.set(::hglrt::add__i64_i64(::hglrt::checked_value(x), (q.value().held).bid));
                return;
            }
            out_.set(::hglrt::neg__i64(1));
            return;
        }
    }
};
}  // namespace n_::noted_bid
namespace w_
{
// noted_bid(x: i64, const q: Noted) -> i64 (node), module demo.configurations, line 47
::hgraph::Port<::hgraph::TS<::hgraph::Int>> noted_bid(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x, const ::hgl::demo::configurations::m_::Noted& q)
{
    return ::hgraph::wire<::hgl::demo::configurations::m_::n_::noted_bid::impl_>(wiring_, x, ::hglrt::configured<::hgl::demo::configurations::m_::Noted>{q});
}
}  // namespace w_

namespace n_::nudged
{
// nudged(x: i64, const by: i64?) -> i64 (node), module demo.configurations, line 54
struct impl_
{
    static constexpr auto name = "demo.configurations::nudged";

    static void eval([[maybe_unused]] ::hgraph::In<"x", ::hgraph::TS<::hgraph::Int>> x, [[maybe_unused]] ::hgraph::Scalar<"by", ::std::optional<::hgraph::Int>> by, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 55 (TIME-3)
        {
            if ((!by.value().has_value()))
            {
                out_.set(::hglrt::checked_value(x));
                return;
            }
            out_.set(::hglrt::add__i64_i64(::hglrt::checked_value(x), (*by.value())));
            return;
        }
    }
};
}  // namespace n_::nudged
namespace w_
{
// nudged(x: i64, const by: i64?) -> i64 (node), module demo.configurations, line 54
::hgraph::Port<::hgraph::TS<::hgraph::Int>> nudged(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x, ::std::optional<::hgraph::Int> by)
{
    return ::hgraph::wire<::hgl::demo::configurations::m_::n_::nudged::impl_>(wiring_, x, by);
}
}  // namespace w_

namespace n_::held_or
{
// held_or(x: i64, const h: Holder?) -> i64 (node), module demo.configurations, line 64
struct impl_
{
    static constexpr auto name = "demo.configurations::held_or";

    static void eval([[maybe_unused]] ::hgraph::In<"x", ::hgraph::TS<::hgraph::Int>> x, [[maybe_unused]] ::hgraph::Scalar<"h", ::std::optional<::hgl::demo::configurations::m_::Holder>> h, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 65 (TIME-3)
        {
            if (h.value().has_value())
            {
                out_.set(::hglrt::add__i64_i64(::hglrt::checked_value(x), ((*h.value())).n));
                return;
            }
            out_.set(::hglrt::checked_value(x));
            return;
        }
    }
};
}  // namespace n_::held_or
namespace w_
{
// held_or(x: i64, const h: Holder?) -> i64 (node), module demo.configurations, line 64
::hgraph::Port<::hgraph::TS<::hgraph::Int>> held_or(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x, const ::std::optional<::hgl::demo::configurations::m_::Holder>& h)
{
    return ::hgraph::wire<::hgl::demo::configurations::m_::n_::held_or::impl_>(wiring_, x, h);
}
}  // namespace w_

namespace n_::tagged_or
{
// tagged_or(x: i64, const t: Tagged?) -> i64 (node), module demo.configurations, line 71
struct impl_
{
    static constexpr auto name = "demo.configurations::tagged_or";

    static void eval([[maybe_unused]] ::hgraph::In<"x", ::hgraph::TS<::hgraph::Int>> x, [[maybe_unused]] ::hgraph::Scalar<"t", ::hglrt::configured<::std::optional<::hgl::demo::configurations::m_::Tagged>>> t, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 72 (TIME-3)
        {
            if (t.value().held.has_value())
            {
                out_.set(::hglrt::add__i64_i64(::hglrt::checked_value(x), ((*t.value().held)).n));
                return;
            }
            out_.set(::hglrt::checked_value(x));
            return;
        }
    }
};
}  // namespace n_::tagged_or
namespace w_
{
// tagged_or(x: i64, const t: Tagged?) -> i64 (node), module demo.configurations, line 71
::hgraph::Port<::hgraph::TS<::hgraph::Int>> tagged_or(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x, const ::std::optional<::hgl::demo::configurations::m_::Tagged>& t)
{
    return ::hgraph::wire<::hgl::demo::configurations::m_::n_::tagged_or::impl_>(wiring_, x, ::hglrt::configured<::std::optional<::hgl::demo::configurations::m_::Tagged>>{t});
}
}  // namespace w_

namespace n_::tagged_first
{
// tagged_first(x: i64, const tagged: list<Tagged>) -> i64 (node), module demo.configurations, line 78
struct impl_
{
    static constexpr auto name = "demo.configurations::tagged_first";

    static void eval([[maybe_unused]] ::hgraph::In<"x", ::hgraph::TS<::hgraph::Int>> x, [[maybe_unused]] ::hgraph::Scalar<"tagged", ::hglrt::configured<::hglrt::list<::hgl::demo::configurations::m_::Tagged>>> tagged, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 79 (TIME-3)
        {
            out_.set(::hglrt::add__i64_i64(::hglrt::checked_value(x), (::hglrt::item(tagged.value().held, 0)).n));
            return;
        }
    }
};
}  // namespace n_::tagged_first
namespace w_
{
// tagged_first(x: i64, const tagged: list<Tagged>) -> i64 (node), module demo.configurations, line 78
::hgraph::Port<::hgraph::TS<::hgraph::Int>> tagged_first(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x, const ::hglrt::list<::hgl::demo::configurations::m_::Tagged>& tagged)
{
    return ::hgraph::wire<::hgl::demo::configurations::m_::n_::tagged_first::impl_>(wiring_, x, ::hglrt::configured<::hglrt::list<::hgl::demo::configurations::m_::Tagged>>{tagged});
}
}  // namespace w_

namespace n_::pair_or
{
// pair_or(x: i64, const t: tuple<i64, i64>?) -> i64 (node), module demo.configurations, line 82
struct impl_
{
    static constexpr auto name = "demo.configurations::pair_or";

    static void eval([[maybe_unused]] ::hgraph::In<"x", ::hgraph::TS<::hgraph::Int>> x, [[maybe_unused]] ::hgraph::Scalar<"t", ::hglrt::configured<::std::optional<::std::tuple<::hgraph::Int, ::hgraph::Int>>>> t, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 83 (TIME-3)
        {
            if ((!t.value().held.has_value()))
            {
                out_.set(::hglrt::checked_value(x));
                return;
            }
            out_.set(::hglrt::add__i64_i64(::hglrt::checked_value(x), ::std::get<0>((*t.value().held))));
            return;
        }
    }
};
}  // namespace n_::pair_or
namespace w_
{
// pair_or(x: i64, const t: tuple<i64, i64>?) -> i64 (node), module demo.configurations, line 82
::hgraph::Port<::hgraph::TS<::hgraph::Int>> pair_or(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x, const ::std::optional<::std::tuple<::hgraph::Int, ::hgraph::Int>>& t)
{
    return ::hgraph::wire<::hgl::demo::configurations::m_::n_::pair_or::impl_>(wiring_, x, ::hglrt::configured<::std::optional<::std::tuple<::hgraph::Int, ::hgraph::Int>>>{t});
}
}  // namespace w_

namespace n_::held_first
{
// held_first(x: i64, const hs: list<Holder>) -> i64 (node), module demo.configurations, line 89
struct impl_
{
    static constexpr auto name = "demo.configurations::held_first";

    static void eval([[maybe_unused]] ::hgraph::In<"x", ::hgraph::TS<::hgraph::Int>> x, [[maybe_unused]] ::hgraph::Scalar<"hs", ::hglrt::list<::hgl::demo::configurations::m_::Holder>> hs, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 90 (TIME-3)
        {
            out_.set(::hglrt::add__i64_i64(::hglrt::checked_value(x), (::hglrt::item(hs.value(), 0)).n));
            return;
        }
    }
};
}  // namespace n_::held_first
namespace w_
{
// held_first(x: i64, const hs: list<Holder>) -> i64 (node), module demo.configurations, line 89
::hgraph::Port<::hgraph::TS<::hgraph::Int>> held_first(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x, const ::hglrt::list<::hgl::demo::configurations::m_::Holder>& hs)
{
    return ::hgraph::wire<::hgl::demo::configurations::m_::n_::held_first::impl_>(wiring_, x, hs);
}
}  // namespace w_

namespace n_::paired
{
// paired(x: i64, const ts: list<tuple<i64, i64>>) -> i64 (node), module demo.configurations, line 93
struct impl_
{
    static constexpr auto name = "demo.configurations::paired";

    static void eval([[maybe_unused]] ::hgraph::In<"x", ::hgraph::TS<::hgraph::Int>> x, [[maybe_unused]] ::hgraph::Scalar<"ts", ::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Int>>> ts, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 94 (TIME-3)
        {
            out_.set(::hglrt::add__i64_i64(::hglrt::checked_value(x), ::std::get<1>(::hglrt::item(ts.value(), 0))));
            return;
        }
    }
};
}  // namespace n_::paired
namespace w_
{
// paired(x: i64, const ts: list<tuple<i64, i64>>) -> i64 (node), module demo.configurations, line 93
::hgraph::Port<::hgraph::TS<::hgraph::Int>> paired(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x, const ::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Int>>& ts)
{
    return ::hgraph::wire<::hgl::demo::configurations::m_::n_::paired::impl_>(wiring_, x, ts);
}
}  // namespace w_

namespace n_::fixed
{
// fixed(x: i64, const fs: list<i64, 2>) -> i64 (node), module demo.configurations, line 97
struct impl_
{
    static constexpr auto name = "demo.configurations::fixed";

    static void eval([[maybe_unused]] ::hgraph::In<"x", ::hgraph::TS<::hgraph::Int>> x, [[maybe_unused]] ::hgraph::Scalar<"fs", ::hglrt::configured<::std::array<::hgraph::Int, 2>>> fs, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 98 (TIME-3)
        {
            out_.set(::hglrt::add__i64_i64(::hglrt::checked_value(x), ::hglrt::item(fs.value().held, 1)));
            return;
        }
    }
};
}  // namespace n_::fixed
namespace w_
{
// fixed(x: i64, const fs: list<i64, 2>) -> i64 (node), module demo.configurations, line 97
::hgraph::Port<::hgraph::TS<::hgraph::Int>> fixed(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x, const ::std::array<::hgraph::Int, 2>& fs)
{
    return ::hgraph::wire<::hgl::demo::configurations::m_::n_::fixed::impl_>(wiring_, x, ::hglrt::configured<::std::array<::hgraph::Int, 2>>{fs});
}
}  // namespace w_

namespace n_::both
{
// both(x: i64) -> i64 (graph), module demo.configurations, line 102
struct impl_
{
    // The call of `add_` on line 103, lifted (FN-6).
    struct lifted_00_
    {
        static constexpr auto name = "demo.configurations:add_";
        static void eval([[maybe_unused]] ::hgraph::In<"lhs", ::hgraph::TS<::hgraph::Int>> lhs, [[maybe_unused]] ::hgraph::In<"rhs", ::hgraph::TS<::hgraph::Int>> rhs, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
        {
            out_.set(::hglrt::add__i64_i64(::hglrt::checked_value(lhs), ::hglrt::checked_value(rhs)));
            return;
        }
    };

    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x)
    {
        return [&] { const auto argument_01_ = ::hgl::demo::configurations::m_::w_::shifted(wiring_, x, ::std::tuple<::hgraph::Int, ::hgraph::Int>{1, 2}); const auto argument_02_ = ::hgl::demo::configurations::m_::w_::shifted(wiring_, x, ::std::tuple<::hgraph::Int, ::hgraph::Int>{10, 20}); return ::hgraph::wire<lifted_00_>(wiring_, argument_01_, argument_02_); }();
    }
};
}  // namespace n_::both
namespace w_
{
// both(x: i64) -> i64 (graph), module demo.configurations, line 102
::hgraph::Port<::hgraph::TS<::hgraph::Int>> both(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x)
{
    return ::hgl::demo::configurations::m_::n_::both::impl_::compose(wiring_, x);
}
}  // namespace w_

namespace n_::holders
{
// holders(probe: bool) -> i64 (graph), module demo.configurations, line 106
struct impl_
{
    // The call of `add_` on line 107, lifted (FN-6).
    struct lifted_00_
    {
        static constexpr auto name = "demo.configurations:add_";
        static void eval([[maybe_unused]] ::hgraph::In<"lhs", ::hgraph::TS<::hgraph::Int>> lhs, [[maybe_unused]] ::hgraph::In<"rhs", ::hgraph::TS<::hgraph::Int>> rhs, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
        {
            out_.set(::hglrt::add__i64_i64(::hglrt::checked_value(lhs), ::hglrt::checked_value(rhs)));
            return;
        }
    };

    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe)
    {
        return [&] { const auto argument_01_ = ::hgl::demo::configurations::m_::w_::first(wiring_, probe, ::hgl::demo::configurations::m_::Holder{::hglrt::list<::hgraph::Int>{5}, 1}); const auto argument_02_ = ::hgl::demo::configurations::m_::w_::first(wiring_, probe, ::hgl::demo::configurations::m_::Holder{::hglrt::list<::hgraph::Int>{50}, 10}); return ::hgraph::wire<lifted_00_>(wiring_, argument_01_, argument_02_); }();
    }
};
}  // namespace n_::holders
namespace w_
{
// holders(probe: bool) -> i64 (graph), module demo.configurations, line 106
::hgraph::Port<::hgraph::TS<::hgraph::Int>> holders(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe)
{
    return ::hgl::demo::configurations::m_::n_::holders::impl_::compose(wiring_, probe);
}
}  // namespace w_

namespace n_::above
{
// above(x: f64, const by: tuple<f64, f64>) -> bool (node), module demo.configurations, line 112
struct impl_
{
    static constexpr auto name = "demo.configurations::above";

    static void eval([[maybe_unused]] ::hgraph::In<"x", ::hgraph::TS<::hgraph::Float>> x, [[maybe_unused]] ::hgraph::Scalar<"by", ::hglrt::configured<::std::tuple<::hgraph::Float, ::hgraph::Float>>> by, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Bool>> out_)
    {
        // when, line 113 (TIME-3)
        {
            out_.set(::hglrt::gt__T_T<::hgraph::Float>(::hglrt::div__f64_f64<::hglrt::ZeroPolicy::ieee>(::hglrt::checked_value(x), ::std::get<0>(by.value().held)), 0.0));
            return;
        }
    }
};
}  // namespace n_::above
namespace w_
{
// above(x: f64, const by: tuple<f64, f64>) -> bool (node), module demo.configurations, line 112
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> above(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Float>> x, const ::std::tuple<::hgraph::Float, ::hgraph::Float>& by)
{
    return [&]() {
        // hgraph issue 1866: wired as a node of its own, since hgraph would take
        // two whose configurations hold `0.0` and `-0.0` for one.
        ::hgraph::Value scalars_{::hgraph::value_type_for_wiring(::hgraph::StaticNodeSignature<::hgl::demo::configurations::m_::n_::above::impl_>::scalar_schema(), wiring_.realization_options())};
        {
            auto configured_ = scalars_.as_bundle().begin_mutation();
            configured_["by"].set(::hglrt::configured<::std::tuple<::hgraph::Float, ::hgraph::Float>>{by});
        }
        ::hgraph::NodeBuilder builder_;
        builder_.implementation<::hgl::demo::configurations::m_::n_::above::impl_>();
        const ::std::array<::hgraph::WiringPortRef, 1> inputs_{x.erased()};
        return ::hgraph::Port<::hgraph::TS<::hgraph::Bool>>{wiring_, wiring_.add_unique_node(typeid(::hgl::demo::configurations::m_::n_::above::impl_), ::std::move(builder_), ::std::span<const ::hgraph::WiringPortRef>{inputs_}, ::std::move(scalars_))};
    }();
}
}  // namespace w_

namespace n_::differ
{
// differ(a: bool, b: bool) -> bool (node), module demo.configurations, line 116
struct impl_
{
    static constexpr auto name = "demo.configurations::differ";

    static void eval([[maybe_unused]] ::hgraph::In<"a", ::hgraph::TS<::hgraph::Bool>> a, [[maybe_unused]] ::hgraph::In<"b", ::hgraph::TS<::hgraph::Bool>> b, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Bool>> out_)
    {
        // when, line 117 (TIME-3)
        {
            out_.set(::hglrt::ne__T_T<::hgraph::Bool>(::hglrt::checked_value(a), ::hglrt::checked_value(b)));
            return;
        }
    }
};
}  // namespace n_::differ
namespace w_
{
// differ(a: bool, b: bool) -> bool (node), module demo.configurations, line 116
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> differ(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> a, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> b)
{
    return ::hgraph::wire<::hgl::demo::configurations::m_::n_::differ::impl_>(wiring_, a, b);
}
}  // namespace w_

namespace n_::signs
{
// signs(x: f64) -> bool (graph), module demo.configurations, line 120
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Float>> x)
    {
        return [&] { const auto argument_00_ = ::hgl::demo::configurations::m_::w_::above(wiring_, x, ::std::tuple<::hgraph::Float, ::hgraph::Float>{0.0, 1.0}); const auto argument_01_ = ::hgl::demo::configurations::m_::w_::above(wiring_, x, ::std::tuple<::hgraph::Float, ::hgraph::Float>{::hglrt::neg__f64(0.0), 1.0}); return ::hgl::demo::configurations::m_::w_::differ(wiring_, argument_00_, argument_01_); }();
    }
};
}  // namespace n_::signs
namespace w_
{
// signs(x: f64) -> bool (graph), module demo.configurations, line 120
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> signs(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Float>> x)
{
    return ::hgl::demo::configurations::m_::n_::signs::impl_::compose(wiring_, x);
}
}  // namespace w_

// member_of(x: i64, members: set<i64>) -> bool (`const fn`), module demo.configurations, line 123
::hgraph::Bool member_of([[maybe_unused]] ::hgraph::Int x, [[maybe_unused]] const ::hglrt::set<::hgraph::Int>& members)
{
    for ([[maybe_unused]] const ::hgraph::Int& m : members)
    {
        if (::hglrt::eq__i64_i64_f64_f64(m, x, 0.0, 0.0))
        {
            return true;
        }
    }
    return false;
}

namespace n_::checked
{
// checked(x: i64) -> bool (graph), module demo.configurations, line 130
struct impl_
{
    // The call of `member_of` on line 130, lifted (FN-6).
    struct lifted_00_
    {
        static constexpr auto name = "demo.configurations:member_of";
        static void eval([[maybe_unused]] ::hgraph::In<"x", ::hgraph::TS<::hgraph::Int>> x, [[maybe_unused]] ::hgraph::Scalar<"members", ::hglrt::configured<::hglrt::set<::hgraph::Int>>> members, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Bool>> out_)
        {
            out_.set(::hgl::demo::configurations::m_::member_of(::hglrt::checked_value(x), members.value().held));
            return;
        }
    };

    static ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x)
    {
        return ::hgraph::wire<lifted_00_>(wiring_, x, ::hglrt::configured<::hglrt::set<::hgraph::Int>>{static_cast<::hglrt::set<::hgraph::Int>>(::hglrt::set<::hgraph::Int>{1, 2})});
    }
};
}  // namespace n_::checked
namespace w_
{
// checked(x: i64) -> bool (graph), module demo.configurations, line 130
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> checked(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x)
{
    return ::hgl::demo::configurations::m_::n_::checked::impl_::compose(wiring_, x);
}
}  // namespace w_

// looked(x: str, book: dict<str, i64>) -> i64 (`const fn`), module demo.configurations, line 132
::hgraph::Int looked([[maybe_unused]] const ::hgraph::Str& x, [[maybe_unused]] const ::hglrt::dict<::hgraph::Str, ::hgraph::Int>& book)
{
    [[maybe_unused]] const ::std::optional<::hgraph::Int> found = ::hglrt::get_dict_K__T__K<::hgraph::Str, ::hgraph::Int>(book, x);
    if ((!found.has_value()))
    {
        return ::hglrt::neg__i64(1);
    }
    return (*found);
}

namespace n_::booked
{
// booked(x: str) -> i64 (graph), module demo.configurations, line 138
struct impl_
{
    // The call of `looked` on line 138, lifted (FN-6).
    struct lifted_00_
    {
        static constexpr auto name = "demo.configurations:looked";
        static void eval([[maybe_unused]] ::hgraph::In<"x", ::hgraph::TS<::hgraph::Str>> x, [[maybe_unused]] ::hgraph::Scalar<"book", ::hglrt::configured<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>> book, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
        {
            out_.set(::hgl::demo::configurations::m_::looked(::hglrt::checked_value(x), book.value().held));
            return;
        }
    };

    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Str>> x)
    {
        return ::hgraph::wire<lifted_00_>(wiring_, x, ::hglrt::configured<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>{static_cast<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>(::hglrt::dict_of<::hgraph::Str, ::hgraph::Int>({{::hgraph::Str{"a"}, 1}}))});
    }
};
}  // namespace n_::booked
namespace w_
{
// booked(x: str) -> i64 (graph), module demo.configurations, line 138
::hgraph::Port<::hgraph::TS<::hgraph::Int>> booked(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Str>> x)
{
    return ::hgl::demo::configurations::m_::n_::booked::impl_::compose(wiring_, x);
}
}  // namespace w_

namespace n_::summed
{
// summed(key: str, const members: set<i64>) -> i64 (node), module demo.configurations, line 141
struct impl_
{
    static constexpr auto name = "demo.configurations::summed";

    static void eval([[maybe_unused]] ::hgraph::In<"key", ::hgraph::TS<::hgraph::Str>> key, [[maybe_unused]] ::hgraph::Scalar<"members", ::hglrt::configured<::hglrt::set<::hgraph::Int>>> members, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 142 (TIME-3)
        {
            [[maybe_unused]] ::hgraph::Int n = 0;
            for ([[maybe_unused]] const ::hgraph::Int& m : members.value().held)
            {
                n = ::hglrt::add__i64_i64(n, m);
            }
            out_.set(n);
            return;
        }
    }
};
}  // namespace n_::summed
namespace w_
{
// summed(key: str, const members: set<i64>) -> i64 (node), module demo.configurations, line 141
::hgraph::Port<::hgraph::TS<::hgraph::Int>> summed(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Str>> key, const ::hglrt::set<::hgraph::Int>& members)
{
    return ::hgraph::wire<::hgl::demo::configurations::m_::n_::summed::impl_>(wiring_, key, ::hglrt::configured<::hglrt::set<::hgraph::Int>>{members});
}
}  // namespace w_

namespace n_::fallback
{
// fallback(key: str, const values: list<i64>) -> i64 (node), module demo.configurations, line 149
struct impl_
{
    static constexpr auto name = "demo.configurations::fallback";

    static void eval([[maybe_unused]] ::hgraph::In<"key", ::hgraph::TS<::hgraph::Str>> key, [[maybe_unused]] ::hgraph::Scalar<"values", ::hglrt::list<::hgraph::Int>> values, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 150 (TIME-3)
        {
            out_.set(::hglrt::item(values.value(), 0));
            return;
        }
    }
};
}  // namespace n_::fallback
namespace w_
{
// fallback(key: str, const values: list<i64>) -> i64 (node), module demo.configurations, line 149
::hgraph::Port<::hgraph::TS<::hgraph::Int>> fallback(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Str>> key, const ::hglrt::list<::hgraph::Int>& values)
{
    return ::hgraph::wire<::hgl::demo::configurations::m_::n_::fallback::impl_>(wiring_, key, values);
}
}  // namespace w_

namespace n_::branch
{
// branch(flag: bool, key: str, const members: set<i64>) -> i64 (graph), module demo.configurations, line 153
struct impl_
{
    // The `if` on line 154 over a connection (CTRL-2, CTRL-13 to CTRL-15): a child
    // graph for each branch, of which hgraph's `switch_` runs the one selected.
    struct switch_00_
    {
        // The branch selected while the condition is `true`.
        struct branch_00_
        {
            static constexpr auto name = "demo.configurations:if on line 154";
            static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose([[maybe_unused]] ::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Str>> key, [[maybe_unused]] ::hgraph::Scalar<"members", ::hglrt::configured<::hglrt::set<::hgraph::Int>>> members)
            {
                return ::hgl::demo::configurations::m_::w_::summed(wiring_, key, members.value().held);
            }
        };
    
        // The branch selected while the condition is `false`.
        struct branch_01_
        {
            static constexpr auto name = "demo.configurations:if on line 154";
            static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose([[maybe_unused]] ::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Str>> key, [[maybe_unused]] ::hgraph::Scalar<"members", ::hglrt::configured<::hglrt::set<::hgraph::Int>>> members)
            {
                return ::hgl::demo::configurations::m_::w_::fallback(wiring_, key, ::hglrt::list<::hgraph::Int>{7});
            }
        };
    
        static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> wire_(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> key_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Str>> key, [[maybe_unused]] const ::hglrt::set<::hgraph::Int>& members)
        {
            ::std::vector<::hgraph::SwitchBranchGraph> branches_;
            branches_.push_back(::hgraph::SwitchBranchGraph{::hgraph::Value{static_cast<::hgraph::Bool>(true)}, ::hgraph::compile_subgraph<branch_00_>(wiring_, ::hglrt::configured<::hglrt::set<::hgraph::Int>>{members})});
            branches_.push_back(::hgraph::SwitchBranchGraph{::hgraph::Value{static_cast<::hgraph::Bool>(false)}, ::hgraph::compile_subgraph<branch_01_>(wiring_, ::hglrt::configured<::hglrt::set<::hgraph::Int>>{members})});
            const ::std::array<::hgraph::WiringPortRef, 1> inputs_{key.erased()};
            return ::hgraph::Port<void>{wiring_, ::hgraph::compiled::switch_(wiring_, key_.erased(), inputs_, ::std::move(branches_))}.as<::hgraph::TS<::hgraph::Int>>();
        }
    };

    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> flag, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Str>> key, [[maybe_unused]] const ::hglrt::set<::hgraph::Int>& members)
    {
        return switch_00_::wire_(wiring_, flag, key, members);
    }
};
}  // namespace n_::branch
namespace w_
{
// branch(flag: bool, key: str, const members: set<i64>) -> i64 (graph), module demo.configurations, line 153
::hgraph::Port<::hgraph::TS<::hgraph::Int>> branch(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> flag, ::hgraph::Port<::hgraph::TS<::hgraph::Str>> key, const ::hglrt::set<::hgraph::Int>& members)
{
    return ::hgl::demo::configurations::m_::n_::branch::impl_::compose(wiring_, flag, key, members);
}
}  // namespace w_

}  // namespace hgl::demo::configurations::m_

namespace hgl::demo::configurations::m_::t_
{

void sources_are_given_a_set_and_a_dictionary()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_00_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::configurations::m_::w_::from_members(wiring_, ::hglrt::set<::hgraph::Int>{7}), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_01_ = ::hglrt::testing::Timed<::hgraph::Int>{{{::hglrt::testing::TraceKey{::hgraph::TimeDelta{1000000}}, 7}}};
        ::hglrt::testing::expect_same(left_00_, right_01_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_02_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::configurations::m_::w_::from_book(wiring_, ::hglrt::dict_of<::hgraph::Str, ::hgraph::Int>({{::hgraph::Str{"a"}, 7}})), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_03_ = ::hglrt::testing::Timed<::hgraph::Int>{{{::hglrt::testing::TraceKey{::hgraph::TimeDelta{1000000}}, 7}}};
        ::hglrt::testing::expect_same(left_02_, right_03_);
    }();
}

void nodes_are_given_any_value()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::configurations::m_::w_::shifted(wiring_, input_00_, ::std::tuple<::hgraph::Int, ::hgraph::Int>{10, 100}), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgraph::Int>{111};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_08_ = [&]() {
        const auto trace_03_ = ::hglrt::testing::Dense<::hgraph::Bool>{true};
        const ::hgl::demo::configurations::m_::Holder argument_05_ = ::hgl::demo::configurations::m_::Holder{::hglrt::list<::hgraph::Int>{5}, 1};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_04_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(trace_03_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_06_ = input_04_; const ::hgl::demo::configurations::m_::Holder argument_07_ = argument_05_; return ::hgl::demo::configurations::m_::w_::first(wiring_, argument_06_, argument_07_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_09_ = ::hglrt::testing::Dense<::hgraph::Int>{6};
        ::hglrt::testing::expect_same(left_08_, right_09_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_015_ = [&]() {
        const auto trace_010_ = ::hglrt::testing::Dense<::hgraph::Int>{1};
        const ::hgl::demo::configurations::m_::Noted argument_012_ = ::hgl::demo::configurations::m_::Noted{10, ::std::nullopt};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_011_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_010_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_013_ = input_011_; const ::hgl::demo::configurations::m_::Noted argument_014_ = argument_012_; return ::hgl::demo::configurations::m_::w_::noted_bid(wiring_, argument_013_, argument_014_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_016_ = ::hglrt::testing::Dense<::hgraph::Int>{11};
        ::hglrt::testing::expect_same(left_015_, right_016_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_018_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_017_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::configurations::m_::w_::nudged(wiring_, input_017_, ::std::nullopt), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_019_ = ::hglrt::testing::Dense<::hgraph::Int>{1};
        ::hglrt::testing::expect_same(left_018_, right_019_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_021_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_020_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::configurations::m_::w_::nudged(wiring_, input_020_, 5), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_022_ = ::hglrt::testing::Dense<::hgraph::Int>{6};
        ::hglrt::testing::expect_same(left_021_, right_022_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_028_ = [&]() {
        const auto trace_023_ = ::hglrt::testing::Dense<::hgraph::Int>{1};
        const ::hgl::demo::configurations::m_::Holder argument_025_ = ::hgl::demo::configurations::m_::Holder{::hglrt::list<::hgraph::Int>{5}, 2};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_024_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_023_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_026_ = input_024_; const ::std::optional<::hgl::demo::configurations::m_::Holder> argument_027_ = argument_025_; return ::hgl::demo::configurations::m_::w_::held_or(wiring_, argument_026_, argument_027_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_029_ = ::hglrt::testing::Dense<::hgraph::Int>{3};
        ::hglrt::testing::expect_same(left_028_, right_029_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_031_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_030_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::configurations::m_::w_::held_or(wiring_, input_030_, ::std::nullopt), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_032_ = ::hglrt::testing::Dense<::hgraph::Int>{1};
        ::hglrt::testing::expect_same(left_031_, right_032_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_034_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_033_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::configurations::m_::w_::pair_or(wiring_, input_033_, ::std::tuple<::hgraph::Int, ::hgraph::Int>{5, 6}), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_035_ = ::hglrt::testing::Dense<::hgraph::Int>{6};
        ::hglrt::testing::expect_same(left_034_, right_035_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_037_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_036_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::configurations::m_::w_::pair_or(wiring_, input_036_, ::std::nullopt), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_038_ = ::hglrt::testing::Dense<::hgraph::Int>{1};
        ::hglrt::testing::expect_same(left_037_, right_038_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_044_ = [&]() {
        const auto trace_039_ = ::hglrt::testing::Dense<::hgraph::Int>{1};
        const ::hglrt::list<::hgl::demo::configurations::m_::Holder> argument_041_ = ::hglrt::list<::hgl::demo::configurations::m_::Holder>{::hgl::demo::configurations::m_::Holder{::hglrt::list<::hgraph::Int>{}, 4}};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_040_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_039_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_042_ = input_040_; const ::hglrt::list<::hgl::demo::configurations::m_::Holder> argument_043_ = argument_041_; return ::hgl::demo::configurations::m_::w_::held_first(wiring_, argument_042_, argument_043_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_045_ = ::hglrt::testing::Dense<::hgraph::Int>{5};
        ::hglrt::testing::expect_same(left_044_, right_045_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_051_ = [&]() {
        const auto trace_046_ = ::hglrt::testing::Dense<::hgraph::Int>{1};
        const ::hgl::demo::configurations::m_::Tagged argument_048_ = ::hgl::demo::configurations::m_::Tagged{::hglrt::set<::hgraph::Int>{1}, 2};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_047_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_046_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_049_ = input_047_; const ::std::optional<::hgl::demo::configurations::m_::Tagged> argument_050_ = argument_048_; return ::hgl::demo::configurations::m_::w_::tagged_or(wiring_, argument_049_, argument_050_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_052_ = ::hglrt::testing::Dense<::hgraph::Int>{3};
        ::hglrt::testing::expect_same(left_051_, right_052_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_054_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_053_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::configurations::m_::w_::tagged_or(wiring_, input_053_, ::std::nullopt), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_055_ = ::hglrt::testing::Dense<::hgraph::Int>{1};
        ::hglrt::testing::expect_same(left_054_, right_055_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_061_ = [&]() {
        const auto trace_056_ = ::hglrt::testing::Dense<::hgraph::Int>{1};
        const ::hglrt::list<::hgl::demo::configurations::m_::Tagged> argument_058_ = ::hglrt::list<::hgl::demo::configurations::m_::Tagged>{::hgl::demo::configurations::m_::Tagged{::hglrt::set<::hgraph::Int>{}, 4}};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_057_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_056_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_059_ = input_057_; const ::hglrt::list<::hgl::demo::configurations::m_::Tagged> argument_060_ = argument_058_; return ::hgl::demo::configurations::m_::w_::tagged_first(wiring_, argument_059_, argument_060_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_062_ = ::hglrt::testing::Dense<::hgraph::Int>{5};
        ::hglrt::testing::expect_same(left_061_, right_062_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_064_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_063_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::configurations::m_::w_::paired(wiring_, input_063_, ::hglrt::list<::std::tuple<::hgraph::Int, ::hgraph::Int>>{::std::tuple<::hgraph::Int, ::hgraph::Int>{5, 6}}), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_065_ = ::hglrt::testing::Dense<::hgraph::Int>{7};
        ::hglrt::testing::expect_same(left_064_, right_065_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_067_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_066_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::configurations::m_::w_::fixed(wiring_, input_066_, ::std::array<::hgraph::Int, 2>{5, 6}), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_068_ = ::hglrt::testing::Dense<::hgraph::Int>{7};
        ::hglrt::testing::expect_same(left_067_, right_068_);
    }();
}

void configurations_that_differ_are_two_nodes()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::configurations::m_::w_::both(wiring_, input_00_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgraph::Int>{35};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_04_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Bool>{true}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::configurations::m_::w_::holders(wiring_, input_03_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_05_ = ::hglrt::testing::Dense<::hgraph::Int>{66};
        ::hglrt::testing::expect_same(left_04_, right_05_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_07_ = ::hglrt::testing::evaluate<::hgraph::Bool>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Float>> input_06_ = ::hgl::core::m_::w_::i_::replay::of_f64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Float>{1.0}, start_));
        ::hgl::core::m_::w_::i_::record::of_bool(wiring_, ::hgl::demo::configurations::m_::w_::signs(wiring_, input_06_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_08_ = ::hglrt::testing::Dense<::hgraph::Bool>{true};
        ::hglrt::testing::expect_same(left_07_, right_08_);
    }();
}

void lifted_calls_and_branches_are_given_any_value()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgraph::Bool>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 3}, start_));
        ::hgl::core::m_::w_::i_::record::of_bool(wiring_, ::hgl::demo::configurations::m_::w_::checked(wiring_, input_00_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgraph::Bool>{true, false};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_04_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Str>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_str(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Str>{::hgraph::Str{"a"}, ::hgraph::Str{"b"}}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::configurations::m_::w_::booked(wiring_, input_03_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_05_ = ::hglrt::testing::Dense<::hgraph::Int>{1, ::hglrt::neg__i64(1)};
        ::hglrt::testing::expect_same(left_04_, right_05_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_014_ = [&]() {
        const auto trace_06_ = ::hglrt::testing::Dense<::hgraph::Bool>{true, false};
        const auto trace_08_ = ::hglrt::testing::Dense<::hgraph::Str>{::hgraph::Str{"a"}, ::hgraph::Str{"b"}};
        const ::hglrt::set<::hgraph::Int> argument_010_ = ::hglrt::set<::hgraph::Int>{1, 2};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_07_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(trace_06_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Str>> input_09_ = ::hgl::core::m_::w_::i_::replay::of_str(wiring_, ::hglrt::testing::replay_items(trace_08_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_011_ = input_07_; const auto argument_012_ = input_09_; const ::hglrt::set<::hgraph::Int> argument_013_ = argument_010_; return ::hgl::demo::configurations::m_::w_::branch(wiring_, argument_011_, argument_012_, argument_013_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_015_ = ::hglrt::testing::Dense<::hgraph::Int>{3, 7};
        ::hglrt::testing::expect_same(left_014_, right_015_);
    }();
}

}  // namespace hgl::demo::configurations::m_::t_
