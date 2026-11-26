// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include "mod-demo-keyed.h"
#include "mod-core.h"

// Names this file writes, which no macro of a header above takes.
#undef a_dictionary_of_sets
#undef a_dictionary_relayed_whole_publishes_each_entry
#undef a_dictionary_ticks_only_where_it_changes
#undef a_lifted_call_reads_the_dictionary_whole
#undef a_lifted_call_reads_the_set_whole
#undef a_set_of_floats
#undef a_set_relayed_whole_publishes_its_change
#undef a_set_ticks_only_where_it_changes
#undef an_entry_of_a_dictionary_is_read_checked
#undef book
#undef book_changes
#undef book_valid
#undef book_wakes
#undef code
#undef core
#undef demo
#undef entry_a
#undef entry_at
#undef entry_delta
#undef entry_or_code
#undef error
#undef found
#undef groups
#undef ignored_removals_establish_only_missing_dictionary_validity
#undef key
#undef keyed
#undef member
#undef member_changes
#undef member_wakes
#undef members
#undef members_valid
#undef price_of_a
#undef priced
#undef read
#undef read_delta
#undef record
#undef relay_book
#undef relay_floats
#undef relay_groups
#undef relay_members
#undef replay
#undef step
#undef sum_members
#undef total
#undef total_of_members
#undef two_writes_of_a_set_are_one_change
#undef values
#undef write_book
#undef write_members

namespace hgl::demo::keyed::m_
{

namespace n_::write_members
{
// write_members(step: i64) -> set<i64> (node), module demo.keyed, line 12
struct impl_
{
    static constexpr auto name = "demo.keyed::write_members";

    static void eval([[maybe_unused]] ::hgraph::In<"step", ::hgraph::TS<::hgraph::Int>> step, [[maybe_unused]] ::hgraph::Out<::hgraph::TSS<::hgraph::Int>> out_)
    {
        // when, line 14 (TIME-3)
        {
            if (::hglrt::eq__i64_i64_f64_f64(::hglrt::checked_value(step), 0, 0.0, 0.0))
            {
                ::hglrt::keyed_publish(out_, ::hglrt::set_delta<::hgraph::Int>{::hglrt::set<::hgraph::Int>{}, ::hglrt::set<::hgraph::Int>{}});
                return;
            }
            if (::hglrt::eq__i64_i64_f64_f64(::hglrt::checked_value(step), 1, 0.0, 0.0))
            {
                ::hglrt::keyed_publish(out_, ::hglrt::set<::hgraph::Int>{1});
                return;
            }
            if (::hglrt::eq__i64_i64_f64_f64(::hglrt::checked_value(step), 2, 0.0, 0.0))
            {
                ::hglrt::keyed_publish(out_, ::hglrt::set<::hgraph::Int>{1, 2});
                return;
            }
            if (::hglrt::eq__i64_i64_f64_f64(::hglrt::checked_value(step), 3, 0.0, 0.0))
            {
                ::hglrt::keyed_publish(out_, ::hglrt::set_delta<::hgraph::Int>{::hglrt::set<::hgraph::Int>{}, ::hglrt::set<::hgraph::Int>{9}});
                return;
            }
            if (::hglrt::eq__i64_i64_f64_f64(::hglrt::checked_value(step), 5, 0.0, 0.0))
            {
                ::hglrt::keyed_publish(out_, ::hglrt::set_delta<::hgraph::Int>{::hglrt::set<::hgraph::Int>{2, 3}, ::hglrt::set<::hgraph::Int>{}});
                ::hglrt::keyed_publish(out_, ::hglrt::set_delta<::hgraph::Int>{::hglrt::set<::hgraph::Int>{}, ::hglrt::set<::hgraph::Int>{1, 2}});
                return;
            }
            ::hglrt::fn_invalidate(out_);
        }
    }
};
}  // namespace n_::write_members
namespace w_
{
// write_members(step: i64) -> set<i64> (node), module demo.keyed, line 12
::hgraph::Port<::hgraph::TSS<::hgraph::Int>> write_members(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> step)
{
    return ::hgraph::wire<::hgl::demo::keyed::m_::n_::write_members::impl_>(wiring_, step);
}
}  // namespace w_

namespace n_::members_valid
{
// members_valid(members: set<i64>) -> bool (node), module demo.keyed, line 28
struct impl_
{
    static constexpr auto name = "demo.keyed::members_valid";

    static void eval([[maybe_unused]] ::hgraph::In<"members", ::hgraph::TSS<::hgraph::Int>, ::hgraph::InputValidity::Unchecked> members, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Bool>> out_)
    {
        // when, line 29 (TIME-3)
        {
            out_.set(::hglrt::fn_valid(members));
            return;
        }
    }
};
}  // namespace n_::members_valid
namespace w_
{
// members_valid(members: set<i64>) -> bool (node), module demo.keyed, line 28
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> members_valid(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSS<::hgraph::Int>> members)
{
    return ::hgraph::wire<::hgl::demo::keyed::m_::n_::members_valid::impl_>(wiring_, members);
}
}  // namespace w_

namespace n_::member_wakes
{
// member_wakes(step: i64) -> bool (graph), module demo.keyed, line 32
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> step)
    {
        return ::hgl::demo::keyed::m_::w_::members_valid(wiring_, ::hgl::demo::keyed::m_::w_::write_members(wiring_, step));
    }
};
}  // namespace n_::member_wakes
namespace w_
{
// member_wakes(step: i64) -> bool (graph), module demo.keyed, line 32
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> member_wakes(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> step)
{
    return ::hgl::demo::keyed::m_::n_::member_wakes::impl_::compose(wiring_, step);
}
}  // namespace w_

namespace n_::read_delta
{
// read_delta(members: set<i64>) -> set<i64> (node), module demo.keyed, line 34
struct impl_
{
    static constexpr auto name = "demo.keyed::read_delta";

    static void eval([[maybe_unused]] ::hgraph::In<"members", ::hgraph::TSS<::hgraph::Int>> members, [[maybe_unused]] ::hgraph::Out<::hgraph::TSS<::hgraph::Int>> out_)
    {
        // when, line 35 (TIME-3)
        {
            ::hglrt::keyed_publish(out_, ::hglrt::fn_delta_value(members));
            return;
        }
    }
};
}  // namespace n_::read_delta
namespace w_
{
// read_delta(members: set<i64>) -> set<i64> (node), module demo.keyed, line 34
::hgraph::Port<::hgraph::TSS<::hgraph::Int>> read_delta(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSS<::hgraph::Int>> members)
{
    return ::hgraph::wire<::hgl::demo::keyed::m_::n_::read_delta::impl_>(wiring_, members);
}
}  // namespace w_

namespace n_::member_changes
{
// member_changes(step: i64) -> set<i64> (graph), module demo.keyed, line 38
struct impl_
{
    static ::hgraph::Port<::hgraph::TSS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> step)
    {
        return ::hgl::demo::keyed::m_::w_::read_delta(wiring_, ::hgl::demo::keyed::m_::w_::write_members(wiring_, step));
    }
};
}  // namespace n_::member_changes
namespace w_
{
// member_changes(step: i64) -> set<i64> (graph), module demo.keyed, line 38
::hgraph::Port<::hgraph::TSS<::hgraph::Int>> member_changes(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> step)
{
    return ::hgl::demo::keyed::m_::n_::member_changes::impl_::compose(wiring_, step);
}
}  // namespace w_

// sum_members(members: set<i64>) -> i64 (`const fn`), module demo.keyed, line 40
::hgraph::Int sum_members([[maybe_unused]] const ::hglrt::set<::hgraph::Int>& members)
{
    [[maybe_unused]] ::hgraph::Int total = 0;
    for ([[maybe_unused]] const ::hgraph::Int& member : members)
    {
        total = ::hglrt::add__i64_i64(total, member);
    }
    return total;
}

namespace n_::total_of_members
{
// total_of_members(step: i64) -> i64 (graph), module demo.keyed, line 46
struct impl_
{
    // The call of `sum_members` on line 46, lifted (FN-6).
    struct lifted_00_
    {
        static constexpr auto name = "demo.keyed:sum_members";
        static void eval([[maybe_unused]] ::hgraph::In<"members", ::hgraph::TSS<::hgraph::Int>> members, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
        {
            out_.set(::hgl::demo::keyed::m_::sum_members(::hglrt::keyed_value(members)));
            return;
        }
    };

    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> step)
    {
        return ::hgraph::wire<lifted_00_>(wiring_, ::hgl::demo::keyed::m_::w_::write_members(wiring_, step));
    }
};
}  // namespace n_::total_of_members
namespace w_
{
// total_of_members(step: i64) -> i64 (graph), module demo.keyed, line 46
::hgraph::Port<::hgraph::TS<::hgraph::Int>> total_of_members(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> step)
{
    return ::hgl::demo::keyed::m_::n_::total_of_members::impl_::compose(wiring_, step);
}
}  // namespace w_

namespace n_::relay_members
{
// relay_members(members: set<i64>) -> set<i64> (node), module demo.keyed, line 64
struct impl_
{
    static constexpr auto name = "demo.keyed::relay_members";

    static void eval([[maybe_unused]] ::hgraph::In<"members", ::hgraph::TSS<::hgraph::Int>> members, [[maybe_unused]] ::hgraph::Out<::hgraph::TSS<::hgraph::Int>> out_)
    {
        // when, line 65 (TIME-3)
        {
            ::hglrt::keyed_publish(out_, ::hglrt::keyed_value(members));
            return;
        }
    }
};
}  // namespace n_::relay_members
namespace w_
{
// relay_members(members: set<i64>) -> set<i64> (node), module demo.keyed, line 64
::hgraph::Port<::hgraph::TSS<::hgraph::Int>> relay_members(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSS<::hgraph::Int>> members)
{
    return ::hgraph::wire<::hgl::demo::keyed::m_::n_::relay_members::impl_>(wiring_, members);
}
}  // namespace w_

namespace n_::relay_floats
{
// relay_floats(values: set<f64>) -> set<f64> (node), module demo.keyed, line 79
struct impl_
{
    static constexpr auto name = "demo.keyed::relay_floats";

    static void eval([[maybe_unused]] ::hgraph::In<"values", ::hgraph::TSS<::hgraph::Float>> values, [[maybe_unused]] ::hgraph::Out<::hgraph::TSS<::hgraph::Float>> out_)
    {
        // when, line 80 (TIME-3)
        {
            ::hglrt::keyed_publish(out_, ::hglrt::keyed_value(values));
            return;
        }
    }
};
}  // namespace n_::relay_floats
namespace w_
{
// relay_floats(values: set<f64>) -> set<f64> (node), module demo.keyed, line 79
::hgraph::Port<::hgraph::TSS<::hgraph::Float>> relay_floats(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSS<::hgraph::Float>> values)
{
    return ::hgraph::wire<::hgl::demo::keyed::m_::n_::relay_floats::impl_>(wiring_, values);
}
}  // namespace w_

namespace n_::write_book
{
// write_book(step: i64) -> dict<str, i64> (node), module demo.keyed, line 90
struct impl_
{
    static constexpr auto name = "demo.keyed::write_book";

    static void eval([[maybe_unused]] ::hgraph::In<"step", ::hgraph::TS<::hgraph::Int>> step, [[maybe_unused]] ::hgraph::Out<::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>> out_)
    {
        // when, line 92 (TIME-3)
        {
            if (::hglrt::eq__i64_i64_f64_f64(::hglrt::checked_value(step), 0, 0.0, 0.0))
            {
                ::hglrt::keyed_publish(out_, ::hglrt::dict_delta<::hgraph::Str, ::hgraph::Int>{::hglrt::dict<::hgraph::Str, ::hgraph::Int>{}, ::hglrt::set<::hgraph::Str>{}});
                return;
            }
            if (::hglrt::eq__i64_i64_f64_f64(::hglrt::checked_value(step), 1, 0.0, 0.0))
            {
                ::hglrt::keyed_publish(out_, ::hglrt::dict_delta<::hgraph::Str, ::hgraph::Int>{::hglrt::dict_of<::hgraph::Str, ::hgraph::Int>({{::hgraph::Str{"A"}, 7}}), ::hglrt::set<::hgraph::Str>{}});
                return;
            }
            if (::hglrt::eq__i64_i64_f64_f64(::hglrt::checked_value(step), 2, 0.0, 0.0))
            {
                ::hglrt::keyed_publish(out_, ::hglrt::dict_delta<::hgraph::Str, ::hgraph::Int>{::hglrt::dict_of<::hgraph::Str, ::hgraph::Int>({{::hgraph::Str{"A"}, 9}}), ::hglrt::set<::hgraph::Str>{}});
                return;
            }
            if (::hglrt::eq__i64_i64_f64_f64(::hglrt::checked_value(step), 3, 0.0, 0.0))
            {
                ::hglrt::keyed_publish(out_, ::hglrt::dict_delta<::hgraph::Str, ::hgraph::Int>{::hglrt::dict<::hgraph::Str, ::hgraph::Int>{}, ::hglrt::set<::hgraph::Str>{::hgraph::Str{"B"}}});
                return;
            }
            if (::hglrt::eq__i64_i64_f64_f64(::hglrt::checked_value(step), 5, 0.0, 0.0))
            {
                ::hglrt::keyed_publish(out_, ::hglrt::dict<::hgraph::Str, ::hgraph::Int>{});
                return;
            }
            if (::hglrt::eq__i64_i64_f64_f64(::hglrt::checked_value(step), 6, 0.0, 0.0))
            {
                ::hglrt::keyed_publish(out_, ::hglrt::dict_of<::hgraph::Str, ::hgraph::Int>({{::hgraph::Str{"A"}, 7}}));
                return;
            }
            ::hglrt::fn_invalidate(out_);
        }
    }
};
}  // namespace n_::write_book
namespace w_
{
// write_book(step: i64) -> dict<str, i64> (node), module demo.keyed, line 90
::hgraph::Port<::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>> write_book(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> step)
{
    return ::hgraph::wire<::hgl::demo::keyed::m_::n_::write_book::impl_>(wiring_, step);
}
}  // namespace w_

namespace n_::book_valid
{
// book_valid(book: dict<str, i64>) -> bool (node), module demo.keyed, line 103
struct impl_
{
    static constexpr auto name = "demo.keyed::book_valid";

    static void eval([[maybe_unused]] ::hgraph::In<"book", ::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>, ::hgraph::InputValidity::Unchecked> book, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Bool>> out_)
    {
        // when, line 104 (TIME-3)
        {
            out_.set(::hglrt::fn_valid(book));
            return;
        }
    }
};
}  // namespace n_::book_valid
namespace w_
{
// book_valid(book: dict<str, i64>) -> bool (node), module demo.keyed, line 103
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> book_valid(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>> book)
{
    return ::hgraph::wire<::hgl::demo::keyed::m_::n_::book_valid::impl_>(wiring_, book);
}
}  // namespace w_

namespace n_::book_wakes
{
// book_wakes(step: i64) -> bool (graph), module demo.keyed, line 107
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> step)
    {
        return ::hgl::demo::keyed::m_::w_::book_valid(wiring_, ::hgl::demo::keyed::m_::w_::write_book(wiring_, step));
    }
};
}  // namespace n_::book_wakes
namespace w_
{
// book_wakes(step: i64) -> bool (graph), module demo.keyed, line 107
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> book_wakes(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> step)
{
    return ::hgl::demo::keyed::m_::n_::book_wakes::impl_::compose(wiring_, step);
}
}  // namespace w_

// price_of_a(book: dict<str, i64>) -> i64 (`const fn`), module demo.keyed, line 109
::hgraph::Int price_of_a([[maybe_unused]] const ::hglrt::dict<::hgraph::Str, ::hgraph::Int>& book)
{
    [[maybe_unused]] const ::std::optional<::hgraph::Int> found = ::hglrt::result_or_null(::hglrt::get_dict_result<::hgraph::Str, ::hgraph::Int>(book, ::hgraph::Str{"A"}));
    if ((!found.has_value()))
    {
        return ::hglrt::neg__i64(1);
    }
    return (*found);
}

namespace n_::priced
{
// priced(step: i64) -> i64 (graph), module demo.keyed, line 115
struct impl_
{
    // The call of `price_of_a` on line 115, lifted (FN-6).
    struct lifted_00_
    {
        static constexpr auto name = "demo.keyed:price_of_a";
        static void eval([[maybe_unused]] ::hgraph::In<"book", ::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>> book, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
        {
            out_.set(::hgl::demo::keyed::m_::price_of_a(::hglrt::keyed_value(book)));
            return;
        }
    };

    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> step)
    {
        return ::hgraph::wire<lifted_00_>(wiring_, ::hgl::demo::keyed::m_::w_::write_book(wiring_, step));
    }
};
}  // namespace n_::priced
namespace w_
{
// priced(step: i64) -> i64 (graph), module demo.keyed, line 115
::hgraph::Port<::hgraph::TS<::hgraph::Int>> priced(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> step)
{
    return ::hgl::demo::keyed::m_::n_::priced::impl_::compose(wiring_, step);
}
}  // namespace w_

namespace n_::relay_book
{
// relay_book(book: dict<str, i64>) -> dict<str, i64> (node), module demo.keyed, line 137
struct impl_
{
    static constexpr auto name = "demo.keyed::relay_book";

    static void eval([[maybe_unused]] ::hgraph::In<"book", ::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>> book, [[maybe_unused]] ::hgraph::Out<::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>> out_)
    {
        // when, line 138 (TIME-3)
        if (::hglrt::fn_all_valid(book))
        {
            ::hglrt::keyed_publish(out_, ::hglrt::keyed_value(book));
            return;
        }
    }
};
}  // namespace n_::relay_book
namespace w_
{
// relay_book(book: dict<str, i64>) -> dict<str, i64> (node), module demo.keyed, line 137
::hgraph::Port<::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>> relay_book(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>> book)
{
    return ::hgraph::wire<::hgl::demo::keyed::m_::n_::relay_book::impl_>(wiring_, book);
}
}  // namespace w_

namespace n_::book_changes
{
// book_changes(book: dict<str, i64>) -> dict<str, i64> (node), module demo.keyed, line 141
struct impl_
{
    static constexpr auto name = "demo.keyed::book_changes";

    static void eval([[maybe_unused]] ::hgraph::In<"book", ::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>> book, [[maybe_unused]] ::hgraph::Out<::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>> out_)
    {
        // when, line 142 (TIME-3)
        {
            ::hglrt::keyed_publish(out_, ::hglrt::fn_delta_value(book));
            return;
        }
    }
};
}  // namespace n_::book_changes
namespace w_
{
// book_changes(book: dict<str, i64>) -> dict<str, i64> (node), module demo.keyed, line 141
::hgraph::Port<::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>> book_changes(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>> book)
{
    return ::hgraph::wire<::hgl::demo::keyed::m_::n_::book_changes::impl_>(wiring_, book);
}
}  // namespace w_

namespace n_::relay_groups
{
// relay_groups(groups: dict<str, set<i64>>) -> dict<str, set<i64>> (node), module demo.keyed, line 155
struct impl_
{
    static constexpr auto name = "demo.keyed::relay_groups";

    static void eval([[maybe_unused]] ::hgraph::In<"groups", ::hgraph::TSD<::hgraph::Str, ::hgraph::TSS<::hgraph::Int>>> groups, [[maybe_unused]] ::hgraph::Out<::hgraph::TSD<::hgraph::Str, ::hgraph::TSS<::hgraph::Int>>> out_)
    {
        // when, line 156 (TIME-3)
        {
            ::hglrt::keyed_publish(out_, ::hglrt::keyed_value(groups));
            return;
        }
    }
};
}  // namespace n_::relay_groups
namespace w_
{
// relay_groups(groups: dict<str, set<i64>>) -> dict<str, set<i64>> (node), module demo.keyed, line 155
::hgraph::Port<::hgraph::TSD<::hgraph::Str, ::hgraph::TSS<::hgraph::Int>>> relay_groups(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSD<::hgraph::Str, ::hgraph::TSS<::hgraph::Int>>> groups)
{
    return ::hgraph::wire<::hgl::demo::keyed::m_::n_::relay_groups::impl_>(wiring_, groups);
}
}  // namespace w_

namespace n_::entry_a
{
// entry_a(book: dict<str, i64>) -> i64 (node), module demo.keyed, line 169
struct impl_
{
    static constexpr auto name = "demo.keyed::entry_a";

    static void eval([[maybe_unused]] ::hgraph::In<"book", ::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>> book, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 170 (TIME-3)
        {
            if (::hglrt::keyed_entry_valid(book, ::hgraph::Str{"A"}))
            {
                out_.set(::hglrt::keyed_entry_value(book, ::hgraph::Str{"A"}));
                return;
            }
        }
    }
};
}  // namespace n_::entry_a
namespace w_
{
// entry_a(book: dict<str, i64>) -> i64 (node), module demo.keyed, line 169
::hgraph::Port<::hgraph::TS<::hgraph::Int>> entry_a(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>> book)
{
    return ::hgraph::wire<::hgl::demo::keyed::m_::n_::entry_a::impl_>(wiring_, book);
}
}  // namespace w_

namespace n_::entry_at
{
// entry_at(book: dict<str, i64>, key: str) -> i64 (node), module demo.keyed, line 173
struct impl_
{
    static constexpr auto name = "demo.keyed::entry_at";

    static void eval([[maybe_unused]] ::hgraph::In<"book", ::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>> book, [[maybe_unused]] ::hgraph::In<"key", ::hgraph::TS<::hgraph::Str>> key, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 174 (TIME-3)
        {
            out_.set(::hglrt::keyed_entry_value(book, ::hglrt::checked_value(key)));
            return;
        }
    }
};
}  // namespace n_::entry_at
namespace w_
{
// entry_at(book: dict<str, i64>, key: str) -> i64 (node), module demo.keyed, line 173
::hgraph::Port<::hgraph::TS<::hgraph::Int>> entry_at(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>> book, ::hgraph::Port<::hgraph::TS<::hgraph::Str>> key)
{
    return ::hgraph::wire<::hgl::demo::keyed::m_::n_::entry_at::impl_>(wiring_, book, key);
}
}  // namespace w_

namespace n_::entry_or_code
{
// entry_or_code(book: dict<str, i64>, key: str) -> str (node), module demo.keyed, line 177
struct impl_
{
    static constexpr auto name = "demo.keyed::entry_or_code";

    static void eval([[maybe_unused]] ::hgraph::In<"book", ::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>> book, [[maybe_unused]] ::hgraph::In<"key", ::hgraph::TS<::hgraph::Str>> key, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Str>> out_)
    {
        // when, line 178 (TIME-3)
        {
            [[maybe_unused]] const ::hglrt::result<::hgraph::Int> read = ::hglrt::keyed_entry_result(book, ::hglrt::checked_value(key));
            if ((read).has_value())
            {
                out_.set(::hgraph::Str{"ok"});
                return;
            }
            [[maybe_unused]] const ::std::optional<::hgraph::Str> code = ((read).error()).code;
            if ((!code.has_value()))
            {
                out_.set(::hgraph::Str{"no code"});
                return;
            }
            out_.set((*code));
            return;
        }
    }
};
}  // namespace n_::entry_or_code
namespace w_
{
// entry_or_code(book: dict<str, i64>, key: str) -> str (node), module demo.keyed, line 177
::hgraph::Port<::hgraph::TS<::hgraph::Str>> entry_or_code(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>> book, ::hgraph::Port<::hgraph::TS<::hgraph::Str>> key)
{
    return ::hgraph::wire<::hgl::demo::keyed::m_::n_::entry_or_code::impl_>(wiring_, book, key);
}
}  // namespace w_

namespace n_::entry_delta
{
// entry_delta(book: dict<str, i64>) -> i64 (node), module demo.keyed, line 187
struct impl_
{
    static constexpr auto name = "demo.keyed::entry_delta";

    static void eval([[maybe_unused]] ::hgraph::In<"book", ::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>, ::hgraph::InputValidity::Unchecked> book, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 188 (TIME-3)
        if (::hglrt::fn_modified(book))
        {
            if (::hglrt::keyed_entry_modified(book, ::hgraph::Str{"A"}))
            {
                out_.set(::hglrt::keyed_entry_delta(book, ::hgraph::Str{"A"}));
                return;
            }
        }
    }
};
}  // namespace n_::entry_delta
namespace w_
{
// entry_delta(book: dict<str, i64>) -> i64 (node), module demo.keyed, line 187
::hgraph::Port<::hgraph::TS<::hgraph::Int>> entry_delta(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>> book)
{
    return ::hgraph::wire<::hgl::demo::keyed::m_::n_::entry_delta::impl_>(wiring_, book);
}
}  // namespace w_

}  // namespace hgl::demo::keyed::m_

namespace hgl::demo::keyed::m_::t_
{

void a_set_ticks_only_where_it_changes()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgraph::Bool>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{0, 0, 4, 0}, start_));
        ::hgl::core::m_::w_::i_::record::of_bool(wiring_, ::hgl::demo::keyed::m_::w_::member_wakes(wiring_, input_00_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgraph::Bool>{true, ::std::nullopt, false, true};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_04_ = ::hglrt::testing::evaluate<::hgraph::Bool>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 1, 3, 2, 2}, start_));
        ::hgl::core::m_::w_::i_::record::of_bool(wiring_, ::hgl::demo::keyed::m_::w_::member_wakes(wiring_, input_03_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_05_ = ::hglrt::testing::Dense<::hgraph::Bool>{true, ::std::nullopt, ::std::nullopt, true, ::std::nullopt};
        ::hglrt::testing::expect_same(left_04_, right_05_);
    }();
}

void two_writes_of_a_set_are_one_change()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_03_ = ::hglrt::testing::evaluate<::hglrt::set<::hgraph::Int>>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 5}, start_));
        struct held_01_
        {
            static void eval(::hgraph::In<"result", ::hgraph::TSS<::hgraph::Int>> result_, ::hgraph::DateTime now_)
            {
                if (!::hglrt::fn_modified(result_))
                {
                    return;
                }
                ::std::optional<::hglrt::set<::hgraph::Int>> whole_;
                try
                {
                    whole_.emplace(::hglrt::keyed_value(result_));
                }
                catch (const ::hglrt::Failure&)
                {
                }
                ::hglrt::testing::hold<::hglrt::set<::hgraph::Int>>(now_, ::std::move(whole_));
            }
        };
        const auto result_02_ = ::hgl::demo::keyed::m_::w_::member_changes(wiring_, input_00_);
        ::hgraph::wire<held_01_>(wiring_, result_02_);
        ::hgl::core::m_::w_::i_::record::of_1set_1i64_1_(wiring_, result_02_, ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_04_ = ::hglrt::testing::Dense<::hglrt::set<::hgraph::Int>>{::hglrt::set_delta<::hgraph::Int>{::hglrt::set<::hgraph::Int>{1}, ::hglrt::set<::hgraph::Int>{}}, ::hglrt::set_delta<::hgraph::Int>{::hglrt::set<::hgraph::Int>{3}, ::hglrt::set<::hgraph::Int>{1}}};
        ::hglrt::testing::expect_same(left_03_, right_04_);
    }();
}

void a_lifted_call_reads_the_set_whole()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{0, 1, 2, 5}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::keyed::m_::w_::total_of_members(wiring_, input_00_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgraph::Int>{0, 1, 3, 3};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
}

void a_set_relayed_whole_publishes_its_change()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_03_ = ::hglrt::testing::evaluate<::hglrt::set<::hgraph::Int>>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TSS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_1set_1i64_1_(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hglrt::set<::hgraph::Int>>{::hglrt::testing::Whole<::hglrt::set<::hgraph::Int>>{::hglrt::set<::hgraph::Int>{1, 2}}, ::hglrt::testing::Whole<::hglrt::set<::hgraph::Int>>{::hglrt::set<::hgraph::Int>{2, 3}}, ::hglrt::testing::Whole<::hglrt::set<::hgraph::Int>>{::hglrt::set<::hgraph::Int>{2, 3}}, ::hglrt::testing::Whole<::hglrt::set<::hgraph::Int>>{::hglrt::set<::hgraph::Int>{}}}, start_));
        struct held_01_
        {
            static void eval(::hgraph::In<"result", ::hgraph::TSS<::hgraph::Int>> result_, ::hgraph::DateTime now_)
            {
                if (!::hglrt::fn_modified(result_))
                {
                    return;
                }
                ::std::optional<::hglrt::set<::hgraph::Int>> whole_;
                try
                {
                    whole_.emplace(::hglrt::keyed_value(result_));
                }
                catch (const ::hglrt::Failure&)
                {
                }
                ::hglrt::testing::hold<::hglrt::set<::hgraph::Int>>(now_, ::std::move(whole_));
            }
        };
        const auto result_02_ = ::hgl::demo::keyed::m_::w_::relay_members(wiring_, input_00_);
        ::hgraph::wire<held_01_>(wiring_, result_02_);
        ::hgl::core::m_::w_::i_::record::of_1set_1i64_1_(wiring_, result_02_, ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_04_ = ::hglrt::testing::Dense<::hglrt::set<::hgraph::Int>>{::hglrt::set_delta<::hgraph::Int>{::hglrt::set<::hgraph::Int>{1, 2}, ::hglrt::set<::hgraph::Int>{}}, ::hglrt::set_delta<::hgraph::Int>{::hglrt::set<::hgraph::Int>{3}, ::hglrt::set<::hgraph::Int>{1}}, ::std::nullopt, ::hglrt::set_delta<::hgraph::Int>{::hglrt::set<::hgraph::Int>{}, ::hglrt::set<::hgraph::Int>{2, 3}}};
        ::hglrt::testing::expect_same(left_03_, right_04_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_08_ = ::hglrt::testing::evaluate<::hglrt::set<::hgraph::Int>>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TSS<::hgraph::Int>> input_05_ = ::hgl::core::m_::w_::i_::replay::of_1set_1i64_1_(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hglrt::set<::hgraph::Int>>{::hglrt::testing::Whole<::hglrt::set<::hgraph::Int>>{::hglrt::set<::hgraph::Int>{1, 2}}, ::hglrt::set_delta<::hgraph::Int>{::hglrt::set<::hgraph::Int>{3}, ::hglrt::set<::hgraph::Int>{}}, ::hglrt::testing::Whole<::hglrt::set<::hgraph::Int>>{::hglrt::set<::hgraph::Int>{3}}}, start_));
        struct held_06_
        {
            static void eval(::hgraph::In<"result", ::hgraph::TSS<::hgraph::Int>> result_, ::hgraph::DateTime now_)
            {
                if (!::hglrt::fn_modified(result_))
                {
                    return;
                }
                ::std::optional<::hglrt::set<::hgraph::Int>> whole_;
                try
                {
                    whole_.emplace(::hglrt::keyed_value(result_));
                }
                catch (const ::hglrt::Failure&)
                {
                }
                ::hglrt::testing::hold<::hglrt::set<::hgraph::Int>>(now_, ::std::move(whole_));
            }
        };
        const auto result_07_ = ::hgl::demo::keyed::m_::w_::relay_members(wiring_, input_05_);
        ::hgraph::wire<held_06_>(wiring_, result_07_);
        ::hgl::core::m_::w_::i_::record::of_1set_1i64_1_(wiring_, result_07_, ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_09_ = ::hglrt::testing::Dense<::hglrt::set<::hgraph::Int>>{::hglrt::testing::Whole<::hglrt::set<::hgraph::Int>>{::hglrt::set<::hgraph::Int>{1, 2}}, ::hglrt::testing::Whole<::hglrt::set<::hgraph::Int>>{::hglrt::set<::hgraph::Int>{1, 2, 3}}, ::hglrt::testing::Whole<::hglrt::set<::hgraph::Int>>{::hglrt::set<::hgraph::Int>{3}}};
        ::hglrt::testing::expect_same(left_08_, right_09_);
    }();
}

void a_set_of_floats()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_03_ = ::hglrt::testing::evaluate<::hglrt::set<::hgraph::Float>>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TSS<::hgraph::Float>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_1set_1f64_1_(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hglrt::set<::hgraph::Float>>{::hglrt::testing::Whole<::hglrt::set<::hgraph::Float>>{::hglrt::set<::hgraph::Float>{1.0, 2.0}}, ::hglrt::testing::Whole<::hglrt::set<::hgraph::Float>>{::hglrt::set<::hgraph::Float>{2.0}}}, start_));
        struct held_01_
        {
            static void eval(::hgraph::In<"result", ::hgraph::TSS<::hgraph::Float>> result_, ::hgraph::DateTime now_)
            {
                if (!::hglrt::fn_modified(result_))
                {
                    return;
                }
                ::std::optional<::hglrt::set<::hgraph::Float>> whole_;
                try
                {
                    whole_.emplace(::hglrt::keyed_value(result_));
                }
                catch (const ::hglrt::Failure&)
                {
                }
                ::hglrt::testing::hold<::hglrt::set<::hgraph::Float>>(now_, ::std::move(whole_));
            }
        };
        const auto result_02_ = ::hgl::demo::keyed::m_::w_::relay_floats(wiring_, input_00_);
        ::hgraph::wire<held_01_>(wiring_, result_02_);
        ::hgl::core::m_::w_::i_::record::of_1set_1f64_1_(wiring_, result_02_, ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_04_ = ::hglrt::testing::Dense<::hglrt::set<::hgraph::Float>>{::hglrt::testing::Whole<::hglrt::set<::hgraph::Float>>{::hglrt::set<::hgraph::Float>{1.0, 2.0}}, ::hglrt::testing::Whole<::hglrt::set<::hgraph::Float>>{::hglrt::set<::hgraph::Float>{2.0}}};
        ::hglrt::testing::expect_same(left_03_, right_04_);
    }();
}

void a_dictionary_ticks_only_where_it_changes()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgraph::Bool>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{0, 0, 4, 0}, start_));
        ::hgl::core::m_::w_::i_::record::of_bool(wiring_, ::hgl::demo::keyed::m_::w_::book_wakes(wiring_, input_00_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgraph::Bool>{true, ::std::nullopt, false, true};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_04_ = ::hglrt::testing::evaluate<::hgraph::Bool>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 3, 2}, start_));
        ::hgl::core::m_::w_::i_::record::of_bool(wiring_, ::hgl::demo::keyed::m_::w_::book_wakes(wiring_, input_03_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_05_ = ::hglrt::testing::Dense<::hgraph::Bool>{true, ::std::nullopt, true};
        ::hglrt::testing::expect_same(left_04_, right_05_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_07_ = ::hglrt::testing::evaluate<::hgraph::Bool>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_06_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{5, 5, 6, 6}, start_));
        ::hgl::core::m_::w_::i_::record::of_bool(wiring_, ::hgl::demo::keyed::m_::w_::book_wakes(wiring_, input_06_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_08_ = ::hglrt::testing::Dense<::hgraph::Bool>{true, ::std::nullopt, true, true};
        ::hglrt::testing::expect_same(left_07_, right_08_);
    }();
}

void ignored_removals_establish_only_missing_dictionary_validity()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_03_ = ::hglrt::testing::evaluate<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{3, 3}, start_));
        struct held_01_
        {
            static void eval(::hgraph::In<"result", ::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>> result_, ::hgraph::DateTime now_)
            {
                if (!::hglrt::fn_modified(result_))
                {
                    return;
                }
                ::std::optional<::hglrt::dict<::hgraph::Str, ::hgraph::Int>> whole_;
                try
                {
                    whole_.emplace(::hglrt::keyed_value(result_));
                }
                catch (const ::hglrt::Failure&)
                {
                }
                ::hglrt::testing::hold<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>(now_, ::std::move(whole_));
            }
        };
        const auto result_02_ = ::hgl::demo::keyed::m_::w_::write_book(wiring_, input_00_);
        ::hgraph::wire<held_01_>(wiring_, result_02_);
        ::hgl::core::m_::w_::i_::record::of_1dict_1str_1_1i64_1_(wiring_, result_02_, ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_04_ = ::hglrt::testing::Dense<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>{::hglrt::testing::Whole<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>{::hglrt::dict<::hgraph::Str, ::hgraph::Int>{}}, ::std::nullopt};
        ::hglrt::testing::expect_same(left_03_, right_04_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_08_ = ::hglrt::testing::evaluate<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_05_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{3, 1, 3, 4, 3, 3}, start_));
        struct held_06_
        {
            static void eval(::hgraph::In<"result", ::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>> result_, ::hgraph::DateTime now_)
            {
                if (!::hglrt::fn_modified(result_))
                {
                    return;
                }
                ::std::optional<::hglrt::dict<::hgraph::Str, ::hgraph::Int>> whole_;
                try
                {
                    whole_.emplace(::hglrt::keyed_value(result_));
                }
                catch (const ::hglrt::Failure&)
                {
                }
                ::hglrt::testing::hold<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>(now_, ::std::move(whole_));
            }
        };
        const auto result_07_ = ::hgl::demo::keyed::m_::w_::write_book(wiring_, input_05_);
        ::hgraph::wire<held_06_>(wiring_, result_07_);
        ::hgl::core::m_::w_::i_::record::of_1dict_1str_1_1i64_1_(wiring_, result_07_, ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_09_ = ::hglrt::testing::Dense<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>{::hglrt::testing::Whole<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>{::hglrt::dict<::hgraph::Str, ::hgraph::Int>{}}, ::hglrt::testing::Whole<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>{::hglrt::dict_of<::hgraph::Str, ::hgraph::Int>({{::hgraph::Str{"A"}, 7}})}, ::std::nullopt, ::std::nullopt, ::hglrt::testing::Whole<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>{::hglrt::dict<::hgraph::Str, ::hgraph::Int>{}}, ::std::nullopt};
        ::hglrt::testing::expect_same(left_08_, right_09_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_011_ = ::hglrt::testing::evaluate<::hgraph::Bool>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_010_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{3, 3, 1, 3, 4, 3, 3}, start_));
        ::hgl::core::m_::w_::i_::record::of_bool(wiring_, ::hgl::demo::keyed::m_::w_::book_wakes(wiring_, input_010_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_012_ = ::hglrt::testing::Dense<::hgraph::Bool>{true, ::std::nullopt, true, ::std::nullopt, false, true, ::std::nullopt};
        ::hglrt::testing::expect_same(left_011_, right_012_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_016_ = ::hglrt::testing::evaluate<::hglrt::set<::hgraph::Int>>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_013_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{3, 3, 1, 3, 4, 3, 3}, start_));
        struct held_014_
        {
            static void eval(::hgraph::In<"result", ::hgraph::TSS<::hgraph::Int>> result_, ::hgraph::DateTime now_)
            {
                if (!::hglrt::fn_modified(result_))
                {
                    return;
                }
                ::std::optional<::hglrt::set<::hgraph::Int>> whole_;
                try
                {
                    whole_.emplace(::hglrt::keyed_value(result_));
                }
                catch (const ::hglrt::Failure&)
                {
                }
                ::hglrt::testing::hold<::hglrt::set<::hgraph::Int>>(now_, ::std::move(whole_));
            }
        };
        const auto result_015_ = ::hgl::demo::keyed::m_::w_::write_members(wiring_, input_013_);
        ::hgraph::wire<held_014_>(wiring_, result_015_);
        ::hgl::core::m_::w_::i_::record::of_1set_1i64_1_(wiring_, result_015_, ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_017_ = ::hglrt::testing::Dense<::hglrt::set<::hgraph::Int>>{::hglrt::testing::Whole<::hglrt::set<::hgraph::Int>>{::hglrt::set<::hgraph::Int>{}}, ::std::nullopt, ::hglrt::testing::Whole<::hglrt::set<::hgraph::Int>>{::hglrt::set<::hgraph::Int>{1}}, ::std::nullopt, ::std::nullopt, ::hglrt::testing::Whole<::hglrt::set<::hgraph::Int>>{::hglrt::set<::hgraph::Int>{}}, ::std::nullopt};
        ::hglrt::testing::expect_same(left_016_, right_017_);
    }();
}

void a_lifted_call_reads_the_dictionary_whole()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{0, 1, 4, 6}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::keyed::m_::w_::priced(wiring_, input_00_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgraph::Int>{::hglrt::neg__i64(1), 7, ::std::nullopt, 7};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
}

void a_dictionary_relayed_whole_publishes_each_entry()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_03_ = ::hglrt::testing::evaluate<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_1dict_1str_1_1i64_1_(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>{::hglrt::testing::Whole<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>{::hglrt::dict_of<::hgraph::Str, ::hgraph::Int>({{::hgraph::Str{"bid"}, 10}, {::hgraph::Str{"ask"}, 12}})}, ::std::nullopt, ::hglrt::testing::Whole<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>{::hglrt::dict_of<::hgraph::Str, ::hgraph::Int>({{::hgraph::Str{"bid"}, 11}})}}, start_));
        struct held_01_
        {
            static void eval(::hgraph::In<"result", ::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>> result_, ::hgraph::DateTime now_)
            {
                if (!::hglrt::fn_modified(result_))
                {
                    return;
                }
                ::std::optional<::hglrt::dict<::hgraph::Str, ::hgraph::Int>> whole_;
                try
                {
                    whole_.emplace(::hglrt::keyed_value(result_));
                }
                catch (const ::hglrt::Failure&)
                {
                }
                ::hglrt::testing::hold<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>(now_, ::std::move(whole_));
            }
        };
        const auto result_02_ = ::hgl::demo::keyed::m_::w_::relay_book(wiring_, input_00_);
        ::hgraph::wire<held_01_>(wiring_, result_02_);
        ::hgl::core::m_::w_::i_::record::of_1dict_1str_1_1i64_1_(wiring_, result_02_, ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_04_ = ::hglrt::testing::Dense<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>{::hglrt::testing::Whole<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>{::hglrt::dict_of<::hgraph::Str, ::hgraph::Int>({{::hgraph::Str{"bid"}, 10}, {::hgraph::Str{"ask"}, 12}})}, ::std::nullopt, ::hglrt::testing::Whole<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>{::hglrt::dict_of<::hgraph::Str, ::hgraph::Int>({{::hgraph::Str{"bid"}, 11}})}};
        ::hglrt::testing::expect_same(left_03_, right_04_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_08_ = ::hglrt::testing::evaluate<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>> input_05_ = ::hgl::core::m_::w_::i_::replay::of_1dict_1str_1_1i64_1_(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>{::hglrt::testing::Whole<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>{::hglrt::dict_of<::hgraph::Str, ::hgraph::Int>({{::hgraph::Str{"bid"}, 10}})}, ::hglrt::dict_delta<::hgraph::Str, ::hgraph::Int>{::hglrt::dict<::hgraph::Str, ::hgraph::Int>{}, ::hglrt::set<::hgraph::Str>{::hgraph::Str{"bid"}}}, ::hglrt::testing::Whole<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>{::hglrt::dict_of<::hgraph::Str, ::hgraph::Int>({{::hgraph::Str{"ask"}, 3}})}}, start_));
        struct held_06_
        {
            static void eval(::hgraph::In<"result", ::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>> result_, ::hgraph::DateTime now_)
            {
                if (!::hglrt::fn_modified(result_))
                {
                    return;
                }
                ::std::optional<::hglrt::dict<::hgraph::Str, ::hgraph::Int>> whole_;
                try
                {
                    whole_.emplace(::hglrt::keyed_value(result_));
                }
                catch (const ::hglrt::Failure&)
                {
                }
                ::hglrt::testing::hold<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>(now_, ::std::move(whole_));
            }
        };
        const auto result_07_ = ::hgl::demo::keyed::m_::w_::book_changes(wiring_, input_05_);
        ::hgraph::wire<held_06_>(wiring_, result_07_);
        ::hgl::core::m_::w_::i_::record::of_1dict_1str_1_1i64_1_(wiring_, result_07_, ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_09_ = ::hglrt::testing::Dense<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>{::hglrt::dict_delta<::hgraph::Str, ::hgraph::Int>{::hglrt::dict_of<::hgraph::Str, ::hgraph::Int>({{::hgraph::Str{"bid"}, 10}}), ::hglrt::set<::hgraph::Str>{}}, ::hglrt::dict_delta<::hgraph::Str, ::hgraph::Int>{::hglrt::dict<::hgraph::Str, ::hgraph::Int>{}, ::hglrt::set<::hgraph::Str>{::hgraph::Str{"bid"}}}, ::hglrt::dict_delta<::hgraph::Str, ::hgraph::Int>{::hglrt::dict_of<::hgraph::Str, ::hgraph::Int>({{::hgraph::Str{"ask"}, 3}}), ::hglrt::set<::hgraph::Str>{}}};
        ::hglrt::testing::expect_same(left_08_, right_09_);
    }();
}

void a_dictionary_of_sets()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_03_ = ::hglrt::testing::evaluate<::hglrt::dict<::hgraph::Str, ::hglrt::set<::hgraph::Int>>>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TSD<::hgraph::Str, ::hgraph::TSS<::hgraph::Int>>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_1dict_1str_1_1set_1i64_1_1_(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hglrt::dict<::hgraph::Str, ::hglrt::set<::hgraph::Int>>>{::hglrt::testing::Whole<::hglrt::dict<::hgraph::Str, ::hglrt::set<::hgraph::Int>>>{::hglrt::dict_of<::hgraph::Str, ::hglrt::set<::hgraph::Int>>({{::hgraph::Str{"a"}, ::hglrt::set<::hgraph::Int>{1, 2}}})}, ::hglrt::dict_delta<::hgraph::Str, ::hglrt::set<::hgraph::Int>>{::hglrt::dict_of<::hgraph::Str, ::hglrt::set_delta<::hgraph::Int>>({{::hgraph::Str{"a"}, ::hglrt::set_delta<::hgraph::Int>{::hglrt::set<::hgraph::Int>{}, ::hglrt::set<::hgraph::Int>{1}}}}), ::hglrt::set<::hgraph::Str>{}}, ::hglrt::testing::Whole<::hglrt::dict<::hgraph::Str, ::hglrt::set<::hgraph::Int>>>{::hglrt::dict_of<::hgraph::Str, ::hglrt::set<::hgraph::Int>>({{::hgraph::Str{"b"}, ::hglrt::set<::hgraph::Int>{3}}})}}, start_));
        struct held_01_
        {
            static void eval(::hgraph::In<"result", ::hgraph::TSD<::hgraph::Str, ::hgraph::TSS<::hgraph::Int>>> result_, ::hgraph::DateTime now_)
            {
                if (!::hglrt::fn_modified(result_))
                {
                    return;
                }
                ::std::optional<::hglrt::dict<::hgraph::Str, ::hglrt::set<::hgraph::Int>>> whole_;
                try
                {
                    whole_.emplace(::hglrt::keyed_value(result_));
                }
                catch (const ::hglrt::Failure&)
                {
                }
                ::hglrt::testing::hold<::hglrt::dict<::hgraph::Str, ::hglrt::set<::hgraph::Int>>>(now_, ::std::move(whole_));
            }
        };
        const auto result_02_ = ::hgl::demo::keyed::m_::w_::relay_groups(wiring_, input_00_);
        ::hgraph::wire<held_01_>(wiring_, result_02_);
        ::hgl::core::m_::w_::i_::record::of_1dict_1str_1_1set_1i64_1_1_(wiring_, result_02_, ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_04_ = ::hglrt::testing::Dense<::hglrt::dict<::hgraph::Str, ::hglrt::set<::hgraph::Int>>>{::hglrt::testing::Whole<::hglrt::dict<::hgraph::Str, ::hglrt::set<::hgraph::Int>>>{::hglrt::dict_of<::hgraph::Str, ::hglrt::set<::hgraph::Int>>({{::hgraph::Str{"a"}, ::hglrt::set<::hgraph::Int>{1, 2}}})}, ::hglrt::testing::Whole<::hglrt::dict<::hgraph::Str, ::hglrt::set<::hgraph::Int>>>{::hglrt::dict_of<::hgraph::Str, ::hglrt::set<::hgraph::Int>>({{::hgraph::Str{"a"}, ::hglrt::set<::hgraph::Int>{2}}})}, ::hglrt::testing::Whole<::hglrt::dict<::hgraph::Str, ::hglrt::set<::hgraph::Int>>>{::hglrt::dict_of<::hgraph::Str, ::hglrt::set<::hgraph::Int>>({{::hgraph::Str{"b"}, ::hglrt::set<::hgraph::Int>{3}}})}};
        ::hglrt::testing::expect_same(left_03_, right_04_);
    }();
}

void an_entry_of_a_dictionary_is_read_checked()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_1dict_1str_1_1i64_1_(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>{::hglrt::testing::Whole<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>{::hglrt::dict_of<::hgraph::Str, ::hgraph::Int>({{::hgraph::Str{"A"}, 1}})}, ::hglrt::testing::Whole<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>{::hglrt::dict_of<::hgraph::Str, ::hgraph::Int>({{::hgraph::Str{"B"}, 2}})}, ::hglrt::testing::Whole<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>{::hglrt::dict_of<::hgraph::Str, ::hgraph::Int>({{::hgraph::Str{"A"}, 3}, {::hgraph::Str{"B"}, 2}})}}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::keyed::m_::w_::entry_a(wiring_, input_00_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgraph::Int>{1, ::std::nullopt, 3};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_09_ = [&]() {
        const auto trace_03_ = ::hglrt::testing::Dense<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>{::hglrt::testing::Whole<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>{::hglrt::dict_of<::hgraph::Str, ::hgraph::Int>({{::hgraph::Str{"A"}, 1}, {::hgraph::Str{"B"}, 2}})}, ::std::nullopt};
        const auto trace_05_ = ::hglrt::testing::Dense<::hgraph::Str>{::hgraph::Str{"B"}, ::hgraph::Str{"A"}};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>> input_04_ = ::hgl::core::m_::w_::i_::replay::of_1dict_1str_1_1i64_1_(wiring_, ::hglrt::testing::replay_items(trace_03_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Str>> input_06_ = ::hgl::core::m_::w_::i_::replay::of_str(wiring_, ::hglrt::testing::replay_items(trace_05_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_07_ = input_04_; const auto argument_08_ = input_06_; return ::hgl::demo::keyed::m_::w_::entry_at(wiring_, argument_07_, argument_08_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_010_ = ::hglrt::testing::Dense<::hgraph::Int>{2, 1};
        ::hglrt::testing::expect_same(left_09_, right_010_);
    }();
    ::hglrt::testing::statement();
    ::hglrt::testing::raises("value.unset_read", [&] {
        static_cast<void>([&]() {
        const auto trace_011_ = ::hglrt::testing::Dense<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>{::hglrt::testing::Whole<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>{::hglrt::dict_of<::hgraph::Str, ::hgraph::Int>({{::hgraph::Str{"A"}, 1}})}};
        const auto trace_013_ = ::hglrt::testing::Dense<::hgraph::Str>{::hgraph::Str{"C"}};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>> input_012_ = ::hgl::core::m_::w_::i_::replay::of_1dict_1str_1_1i64_1_(wiring_, ::hglrt::testing::replay_items(trace_011_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Str>> input_014_ = ::hgl::core::m_::w_::i_::replay::of_str(wiring_, ::hglrt::testing::replay_items(trace_013_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_015_ = input_012_; const auto argument_016_ = input_014_; return ::hgl::demo::keyed::m_::w_::entry_at(wiring_, argument_015_, argument_016_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }());
    });
    ::hglrt::testing::statement();
    [&] {
        const auto left_023_ = [&]() {
        const auto trace_017_ = ::hglrt::testing::Dense<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>{::hglrt::testing::Whole<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>{::hglrt::dict_of<::hgraph::Str, ::hgraph::Int>({{::hgraph::Str{"A"}, 1}})}, ::std::nullopt};
        const auto trace_019_ = ::hglrt::testing::Dense<::hgraph::Str>{::hgraph::Str{"A"}, ::hgraph::Str{"C"}};
        return ::hglrt::testing::evaluate<::hgraph::Str>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>> input_018_ = ::hgl::core::m_::w_::i_::replay::of_1dict_1str_1_1i64_1_(wiring_, ::hglrt::testing::replay_items(trace_017_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Str>> input_020_ = ::hgl::core::m_::w_::i_::replay::of_str(wiring_, ::hglrt::testing::replay_items(trace_019_, start_));
        ::hgl::core::m_::w_::i_::record::of_str(wiring_, [&] { const auto argument_021_ = input_018_; const auto argument_022_ = input_020_; return ::hgl::demo::keyed::m_::w_::entry_or_code(wiring_, argument_021_, argument_022_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_024_ = ::hglrt::testing::Dense<::hgraph::Str>{::hgraph::Str{"ok"}, ::hgraph::Str{"value.unset_read"}};
        ::hglrt::testing::expect_same(left_023_, right_024_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_026_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>> input_025_ = ::hgl::core::m_::w_::i_::replay::of_1dict_1str_1_1i64_1_(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>{::hglrt::testing::Whole<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>{::hglrt::dict_of<::hgraph::Str, ::hgraph::Int>({{::hgraph::Str{"A"}, 1}})}, ::hglrt::dict_delta<::hgraph::Str, ::hgraph::Int>{::hglrt::dict_of<::hgraph::Str, ::hgraph::Int>({{::hgraph::Str{"B"}, 3}}), ::hglrt::set<::hgraph::Str>{}}, ::hglrt::dict_delta<::hgraph::Str, ::hgraph::Int>{::hglrt::dict_of<::hgraph::Str, ::hgraph::Int>({{::hgraph::Str{"A"}, 4}}), ::hglrt::set<::hgraph::Str>{}}}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::keyed::m_::w_::entry_delta(wiring_, input_025_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_027_ = ::hglrt::testing::Dense<::hgraph::Int>{1, ::std::nullopt, 4};
        ::hglrt::testing::expect_same(left_026_, right_027_);
    }();
}

}  // namespace hgl::demo::keyed::m_::t_
