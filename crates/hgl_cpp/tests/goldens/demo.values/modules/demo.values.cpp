// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include "demo.values.h"

// Names this file writes, which no macro of a header above takes.
#undef Level
#undef Side
#undef ZeroPolicy
#undef a
#undef a_list_fails_before_its_index
#undef a_place_fails_at_its_list_first
#undef a_place_is_checked_before_its_value
#undef a_switch_compares_by_value
#undef a_type_declared_after_its_use
#undef a_var_parameter_is_its_place
#undef an_overflow_fails_with_its_code
#undef assigned
#undef at
#undef b
#undef big
#undef bit_kept
#undef bits_are_values
#undef bump
#undef bumped
#undef bumped_at
#undef buy
#undef by
#undef changed
#undef checked
#undef checked_value
#undef code_first
#undef conditions_choose
#undef count
#undef defaults_and_nulls
#undef demo
#undef divisor
#undef doubled
#undef element
#undef entry
#undef error
#undef evaluation_follows_the_order_written
#undef evens
#undef factor
#undef fail
#undef first
#undef first_bit_as_it_was
#undef first_flag
#undef first_row_total
#undef flag
#undef flags
#undef flipped
#undef from_a_made_row
#undef generics_by_instance
#undef get
#undef given
#undef grown
#undef held
#undef held_text
#undef high
#undef hold
#undef i
#undef in_order
#undef index_first
#undef index_once
#undef indices_in_order
#undef item
#undef items
#undef kept
#undef larger
#undef level
#undef limit
#undef list_first
#undef loops_run
#undef made
#undef made_error
#undef made_rows
#undef message
#undef never_null
#undef null_is_null
#undef null_tests_of_any_value
#undef offset
#undef one
#undef opposite
#undef pair
#undef pick
#undef pick_flag
#undef picked
#undef placed
#undef placed_at
#undef present
#undef read_first
#undef replaced
#undef replaced_first
#undef results_propagate
#undef rows
#undef same_level
#undef scaled
#undef sell
#undef set_bits
#undef side
#undef sign
#undef step
#undef swapped
#undef text
#undef text_first
#undef total
#undef triangle
#undef tuple_first
#undef tuples_and_constants
#undef value
#undef values
#undef words
#undef x
#undef zero

namespace hgl::demo::values::m_
{

// const limit: i64 (UNIT-13), module demo.values, line 9
const ::hgraph::Int& limit()
{
    static const ::hgraph::Int value = 10;
    return value;
}

// const step: duration (UNIT-13), module demo.values, line 10
const ::hgraph::TimeDelta& step()
{
    static const ::hgraph::TimeDelta value = ::hgraph::TimeDelta{5000000};
    return value;
}

// sign(value: i64) -> i64 (`const fn`), module demo.values, line 13
::hgraph::Int sign([[maybe_unused]] ::hgraph::Int value)
{
    if (::hglrt::lt__T_T<::hgraph::Int>(value, 0))
    {
        return ::hglrt::neg__i64(1);
    }
    else if (::hglrt::eq__i64_i64_f64_f64(value, 0, 0.0, 0.0))
    {
        return 0;
    }
    return 1;
}

// larger(a: i64, b: i64) -> i64 (`const fn`), module demo.values, line 22
::hgraph::Int larger([[maybe_unused]] ::hgraph::Int a, [[maybe_unused]] ::hgraph::Int b)
{
    return (::hglrt::lt__T_T<::hgraph::Int>(a, b) ? ::hgraph::Int(b) : ::hgraph::Int(a));
}

// opposite(side: Side) -> Side (`const fn`), module demo.values, line 25
::hgl::demo::values::m_::Side opposite([[maybe_unused]] ::hgl::demo::values::m_::Side side)
{
    {
        const ::hgl::demo::values::m_::Side selected_00_ = side;
        if (selected_00_ == ::hgl::demo::values::m_::Side::buy)
        {
            return ::hgl::demo::values::m_::Side::sell;
        }
        else if (selected_00_ == ::hgl::demo::values::m_::Side::sell)
        {
            return ::hgl::demo::values::m_::Side::buy;
        }
        else
        {
            return side;
        }
    }
}

// triangle(count: i64) -> i64 (`const fn`), module demo.values, line 37
::hgraph::Int triangle([[maybe_unused]] ::hgraph::Int count)
{
    [[maybe_unused]] ::hgraph::Int total = 0;
    [[maybe_unused]] ::hgraph::Int at = 1;
    while (::hglrt::le__T_T<::hgraph::Int>(at, count))
    {
        total = ::hglrt::add__i64_i64(total, at);
        at = ::hglrt::add__i64_i64(at, 1);
    }
    return total;
}

// evens(values: list<i64>) -> list<i64> (`const fn`), module demo.values, line 48
::hglrt::list<::hgraph::Int> evens([[maybe_unused]] const ::hglrt::list<::hgraph::Int>& values)
{
    [[maybe_unused]] ::hglrt::list<::hgraph::Int> kept = ::hglrt::list<::hgraph::Int>{};
    for (const ::hgraph::Int& value : values)
    {
        if (::hglrt::eq__i64_i64_f64_f64(::hglrt::mod__i64_i64<::hglrt::ZeroPolicy::fail>(value, 2), 0, 0.0, 0.0))
        {
            ::hglrt::fn_append<::hgraph::Int>(kept, value);
        }
    }
    return kept;
}

// bumped(values: list<i64>, at: i64) -> list<i64> (`const fn`), module demo.values, line 59
::hglrt::list<::hgraph::Int> bumped([[maybe_unused]] const ::hglrt::list<::hgraph::Int>& values, [[maybe_unused]] ::hgraph::Int at)
{
    [[maybe_unused]] ::hglrt::list<::hgraph::Int> changed = values;
    const ::hgraph::Int index_00_ = at;
    static_cast<void>(::hglrt::item(changed, index_00_));
    ::hglrt::item(changed, index_00_) = ::hglrt::add__i64_i64(::hglrt::item(changed, index_00_), 1);
    return changed;
}

// scaled(value: i64, factor: i64 = ..., offset: i64? = ...) -> i64 (`const fn`), module demo.values, line 66
::hgraph::Int scaled([[maybe_unused]] ::hgraph::Int value, [[maybe_unused]] ::hgraph::Int factor, [[maybe_unused]] ::std::optional<::hgraph::Int> offset)
{
    if ((!offset.has_value()))
    {
        return ::hglrt::mul__i64_i64(value, factor);
    }
    return ::hglrt::add__i64_i64(::hglrt::mul__i64_i64(value, factor), (*offset));
}

// swapped(pair: tuple<i64, str>) -> tuple<str, i64> (`const fn`), module demo.values, line 77
::std::tuple<::hgraph::Str, ::hgraph::Int> swapped([[maybe_unused]] const ::std::tuple<::hgraph::Int, ::hgraph::Str>& pair)
{
    return ::std::tuple<::hgraph::Str, ::hgraph::Int>{::std::get<1>(pair), ::std::get<0>(pair)};
}

// checked(value: i64) -> result<i64, error> (`const fn`), module demo.values, line 80
::hglrt::result<::hgraph::Int> checked([[maybe_unused]] ::hgraph::Int value)
{
    if (::hglrt::gt__T_T<::hgraph::Int>(value, ::hgl::demo::values::m_::limit()))
    {
        return ::hglrt::result<::hgraph::Int>{::std::unexpect, ::hglrt::make_error(::hgraph::Str{"too large"}, ::hgraph::Str{"demo.too_large"})};
    }
    return ::hglrt::result<::hgraph::Int>{value};
}

// doubled(value: i64) -> result<i64, error> (`const fn`), module demo.values, line 87
::hglrt::result<::hgraph::Int> doubled([[maybe_unused]] ::hgraph::Int value)
{
    try
    {
        [[maybe_unused]] const ::hgraph::Int checked_value = ::hglrt::propagate(::hgl::demo::values::m_::checked(value));
        return ::hglrt::result<::hgraph::Int>{::hglrt::mul__i64_i64(checked_value, 2)};
    }
    catch (const ::hglrt::return_error<::hglrt::error>& failure_)
    {
        return ::std::unexpected(failure_.value);
    }
}

// pick(a: i64, b: i64) -> i64 (`const fn`), module demo.values, line 95
::hgraph::Int pick([[maybe_unused]] ::hgraph::Int a, [[maybe_unused]] ::hgraph::Int b)
{
    return a;
}

// in_order() -> i64 (`const fn`), module demo.values, line 97
::hgraph::Int in_order()
{
    [[maybe_unused]] ::hgraph::Int x = 1;
    return [&] { const ::hgraph::Int argument_00_ = x; const ::hgraph::Int argument_01_ = [&]() -> ::hgraph::Int {
    x = 2;
    return 0;
}(); return ::hgl::demo::values::m_::pick(argument_00_, argument_01_); }();
}

// placed() -> list<i64> (`const fn`), module demo.values, line 105
::hglrt::list<::hgraph::Int> placed()
{
    [[maybe_unused]] ::hglrt::list<::hgraph::Int> items = ::hglrt::list<::hgraph::Int>{10, 20};
    [[maybe_unused]] ::hgraph::Int at = 0;
    const ::hgraph::Int index_00_ = at;
    static_cast<void>(::hglrt::item(items, index_00_));
    ::hglrt::item(items, index_00_) = [&] { const ::hgraph::Int argument_01_ = ::hglrt::item(items, index_00_); const ::hgraph::Int argument_02_ = [&]() -> ::hgraph::Int {
    at = 1;
    return 5;
}(); return ::hglrt::add__i64_i64(argument_01_, argument_02_); }();
    return items;
}

// read_first() -> i64 (`const fn`), module demo.values, line 115
::hgraph::Int read_first()
{
    [[maybe_unused]] ::hglrt::list<::hgraph::Int> values = ::hglrt::list<::hgraph::Int>{1, 2};
    return [&]() -> ::hgraph::Int { const auto list_00_ = values; const auto index_01_ = [&]() -> ::hgraph::Int {
    values = ::hglrt::list<::hgraph::Int>{5, 6};
    return 0;
}(); return ::hglrt::item(list_00_, index_01_); }();
}

// list_first() -> i64 (`const fn`), module demo.values, line 123
::hgraph::Int list_first()
{
    [[maybe_unused]] ::hgraph::Int at = 0;
    [[maybe_unused]] const ::hgraph::Int picked = [&]() -> ::hgraph::Int { const auto list_00_ = [&]() -> ::hglrt::list<::hgraph::Int> {
    at = 1;
    return ::hglrt::list<::hgraph::Int>{10, 20};
}(); const auto index_01_ = at; return ::hglrt::item(list_00_, index_01_); }();
    return picked;
}

// replaced_first() -> list<i64> (`const fn`), module demo.values, line 133
::hglrt::list<::hgraph::Int> replaced_first()
{
    [[maybe_unused]] ::hglrt::list<::hgraph::Int> items = ::hglrt::list<::hgraph::Int>{10};
    static_cast<void>(::hglrt::item(items, 2));
    ::hglrt::item(items, 2) = [&]() -> ::hgraph::Int {
    items = ::hglrt::list<::hgraph::Int>{10, 20, 30};
    return 5;
}();
    return items;
}

// placed_at(divisor: i64) -> list<list<i64>> (`const fn`), module demo.values, line 144
::hglrt::list<::hglrt::list<::hgraph::Int>> placed_at([[maybe_unused]] ::hgraph::Int divisor)
{
    [[maybe_unused]] ::hglrt::list<::hglrt::list<::hgraph::Int>> rows = ::hglrt::list<::hglrt::list<::hgraph::Int>>{::hglrt::list<::hgraph::Int>{10}};
    static_cast<void>(::hglrt::item(rows, 2));
    const ::hgraph::Int index_00_ = ::hglrt::floordiv__i64_i64<::hglrt::ZeroPolicy::fail>(1, divisor);
    static_cast<void>(::hglrt::item(::hglrt::item(rows, 2), index_00_));
    ::hglrt::item(::hglrt::item(rows, 2), index_00_) = 0;
    return rows;
}

// bumped_at(divisor: i64) -> list<list<i64>> (`const fn`), module demo.values, line 150
::hglrt::list<::hglrt::list<::hgraph::Int>> bumped_at([[maybe_unused]] ::hgraph::Int divisor)
{
    [[maybe_unused]] ::hglrt::list<::hglrt::list<::hgraph::Int>> rows = ::hglrt::list<::hglrt::list<::hgraph::Int>>{::hglrt::list<::hgraph::Int>{10}};
    ::hgl::demo::values::m_::bump([&]() -> ::hgraph::Int& { auto& list_00_ = ::hglrt::item(rows, 2); const auto index_01_ = ::hglrt::floordiv__i64_i64<::hglrt::ZeroPolicy::fail>(1, divisor); return ::hglrt::item(list_00_, index_01_); }(), 1);
    return rows;
}

// indices_in_order() -> list<list<i64>> (`const fn`), module demo.values, line 158
::hglrt::list<::hglrt::list<::hgraph::Int>> indices_in_order()
{
    [[maybe_unused]] ::hglrt::list<::hglrt::list<::hgraph::Int>> rows = ::hglrt::list<::hglrt::list<::hgraph::Int>>{::hglrt::list<::hgraph::Int>{1, 2}, ::hglrt::list<::hgraph::Int>{3, 4}};
    [[maybe_unused]] ::hgraph::Int i = 0;
    const ::hgraph::Int index_00_ = i;
    static_cast<void>(::hglrt::item(rows, index_00_));
    const ::hgraph::Int index_01_ = [&]() -> ::hgraph::Int {
        i = 1;
        return 0;
    }();
    static_cast<void>(::hglrt::item(::hglrt::item(rows, index_00_), index_01_));
    ::hglrt::item(::hglrt::item(rows, index_00_), index_01_) = 5;
    return rows;
}

// present(a: i64?) -> bool (`const fn`), module demo.values, line 170
::hgraph::Bool present([[maybe_unused]] ::std::optional<::hgraph::Int> a)
{
    if ((!a.has_value()))
    {
        return false;
    }
    return a.has_value();
}

// never_null(a: i64) -> bool (`const fn`), module demo.values, line 177
::hgraph::Bool never_null([[maybe_unused]] ::hgraph::Int a)
{
    return (static_cast<void>(a), false);
}

// null_is_null() -> bool (`const fn`), module demo.values, line 179
::hgraph::Bool null_is_null()
{
    return true;
}

// index_first() -> list<i64> (`const fn`), module demo.values, line 182
::hglrt::list<::hgraph::Int> index_first()
{
    [[maybe_unused]] ::hglrt::list<::hgraph::Int> items = ::hglrt::list<::hgraph::Int>{10, 20};
    [[maybe_unused]] ::hgraph::Int at = 0;
    const ::hgraph::Int index_00_ = [&]() -> ::hgraph::Int {
        at = 1;
        return at;
    }();
    static_cast<void>(::hglrt::item(items, index_00_));
    ::hglrt::item(items, index_00_) = at;
    return items;
}

// index_once() -> tuple<list<i64>, i64> (`const fn`), module demo.values, line 193
::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Int> index_once()
{
    [[maybe_unused]] ::hglrt::list<::hgraph::Int> items = ::hglrt::list<::hgraph::Int>{10, 20, 30};
    [[maybe_unused]] ::hgraph::Int at = 0;
    const ::hgraph::Int index_00_ = [&]() -> ::hgraph::Int {
        at = ::hglrt::add__i64_i64(at, 1);
        return at;
    }();
    static_cast<void>(::hglrt::item(items, index_00_));
    ::hglrt::item(items, index_00_) = [&] { const ::hgraph::Int argument_01_ = ::hglrt::item(items, index_00_); const ::hgraph::Int argument_02_ = 5; return ::hglrt::add__i64_i64(argument_01_, argument_02_); }();
    return ::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Int>{items, at};
}

// held_text() -> str (`const fn`), module demo.values, line 205
::hgraph::Str held_text()
{
    [[maybe_unused]] const ::hglrt::list<::hglrt::list<::hgraph::Str>> rows = ::hglrt::list<::hglrt::list<::hgraph::Str>>{::hglrt::list<::hgraph::Str>{::hgraph::Str{"text"}}};
    [[maybe_unused]] const ::hgraph::Str held = [&]() -> ::hgraph::Str { const auto list_02_ = [&]() -> ::hglrt::list<::hgraph::Str> { const auto list_00_ = rows; const auto index_01_ = [&]() -> ::hgraph::Int {
    [[maybe_unused]] const ::hgraph::Int zero = 0;
    return zero;
}(); return ::hglrt::item(list_00_, index_01_); }(); const auto index_03_ = 0; return ::hglrt::item(list_02_, index_03_); }();
    return held;
}

// tuple_first() -> tuple<i64, i64> (`const fn`), module demo.values, line 216
::std::tuple<::hgraph::Int, ::hgraph::Int> tuple_first()
{
    [[maybe_unused]] ::hgraph::Int x = 1;
    return [&]() -> ::std::tuple<::hgraph::Int, ::hgraph::Int> { const ::hgraph::Int entry_00_ = x; const ::hgraph::Int entry_01_ = [&]() -> ::hgraph::Int {
    x = 2;
    return 0;
}(); return ::std::tuple<::hgraph::Int, ::hgraph::Int>{entry_00_, entry_01_}; }();
}

// text_first() -> tuple<str, i64> (`const fn`), module demo.values, line 224
::std::tuple<::hgraph::Str, ::hgraph::Int> text_first()
{
    [[maybe_unused]] ::hglrt::list<::hgraph::Str> words = ::hglrt::list<::hgraph::Str>{::hgraph::Str{"before"}};
    return [&]() -> ::std::tuple<::hgraph::Str, ::hgraph::Int> { const ::hgraph::Str entry_00_ = ::hglrt::item(words, 0); const ::hgraph::Int entry_01_ = [&]() -> ::hgraph::Int {
    words = ::hglrt::list<::hgraph::Str>{::hgraph::Str{"after"}, ::hgraph::Str{"more"}};
    return 0;
}(); return ::std::tuple<::hgraph::Str, ::hgraph::Int>{entry_00_, entry_01_}; }();
}

// from_a_made_row(one: i64) -> i64 (`const fn`), module demo.values, line 234
::hgraph::Int from_a_made_row([[maybe_unused]] ::hgraph::Int one)
{
    return [&]() -> ::hgraph::Int { const auto list_00_ = ::hglrt::item(::hglrt::list<::hglrt::list<::hgraph::Int>>{::hglrt::list<::hgraph::Int>{7}}, 0); const auto index_01_ = ::hglrt::floordiv__i64_i64<::hglrt::ZeroPolicy::fail>(0, one); return ::hglrt::item(list_00_, index_01_); }();
}

// made_rows() -> list<list<i64>> (`const fn`), module demo.values, line 237
::hglrt::list<::hglrt::list<::hgraph::Int>> made_rows()
{
    return ::hglrt::list<::hglrt::list<::hgraph::Int>>{::hglrt::list<::hgraph::Int>{1, 2}, ::hglrt::list<::hgraph::Int>{3}};
}

// first_row_total() -> i64 (`const fn`), module demo.values, line 239
::hgraph::Int first_row_total()
{
    [[maybe_unused]] ::hgraph::Int total = 0;
    const ::hglrt::list<::hgraph::Int> items_00_ = ::hglrt::item(::hgl::demo::values::m_::made_rows(), 0);
    for (const ::hgraph::Int& entry : items_00_)
    {
        total = ::hglrt::add__i64_i64(total, entry);
    }
    return total;
}

// grown(by: i64) -> list<i64> (`const fn`), module demo.values, line 248
::hglrt::list<::hgraph::Int> grown([[maybe_unused]] ::hgraph::Int by)
{
    return ::hglrt::list<::hgraph::Int>{::hglrt::add__i64_i64(by, 9223372036854775807)};
}

// made_error() -> str (`const fn`), module demo.values, line 250
::hgraph::Str made_error()
{
    [[maybe_unused]] ::hgraph::Str message = ::hgraph::Str{"before"};
    [[maybe_unused]] const ::hglrt::error made = [&] { const auto message_00_ = message; const auto code_01_ = [&]() -> ::hgraph::Str {
    message = ::hgraph::Str{"after"};
    return ::hgraph::Str{"code"};
}(); return ::hglrt::make_error(message_00_, code_01_); }();
    return (made).message;
}

// code_first() -> str (`const fn`), module demo.values, line 260
::hgraph::Str code_first()
{
    [[maybe_unused]] ::hgraph::Str text = ::hgraph::Str{"before"};
    [[maybe_unused]] const ::hglrt::error made = [&] { const auto code_01_ = [&]() -> ::hgraph::Str {
    text = ::hgraph::Str{"after"};
    return ::hgraph::Str{"code"};
}(); const auto message_00_ = text; return ::hglrt::make_error(message_00_, code_01_); }();
    return (made).message;
}

// same_level(level: Level) -> Level (`const fn`), module demo.values, line 270
::hgl::demo::values::m_::Level same_level([[maybe_unused]] ::hgl::demo::values::m_::Level level)
{
    return level;
}

// flipped(flags: list<bool>) -> list<bool> (`const fn`), module demo.values, line 276
::hglrt::list<::hgraph::Bool> flipped([[maybe_unused]] const ::hglrt::list<::hgraph::Bool>& flags)
{
    [[maybe_unused]] ::hglrt::list<::hgraph::Bool> changed = flags;
    static_cast<void>(::hglrt::item(changed, 0));
    ::hglrt::item(changed, 0) = (!::hglrt::item(changed, 0));
    return changed;
}

// first_flag(flags: list<bool>) -> bool (`const fn`), module demo.values, line 282
::hgraph::Bool first_flag([[maybe_unused]] const ::hglrt::list<::hgraph::Bool>& flags)
{
    return ::hglrt::item(flags, 0);
}

// pick_flag(a: bool, b: bool) -> bool (`const fn`), module demo.values, line 284
::hgraph::Bool pick_flag([[maybe_unused]] ::hgraph::Bool a, [[maybe_unused]] ::hgraph::Bool b)
{
    return a;
}

// first_bit_as_it_was() -> bool (`const fn`), module demo.values, line 286
::hgraph::Bool first_bit_as_it_was()
{
    [[maybe_unused]] ::hglrt::list<::hgraph::Bool> flags = ::hglrt::list<::hgraph::Bool>{false};
    return [&] { const ::hgraph::Bool argument_00_ = ::hglrt::item(flags, 0); const ::hgraph::Bool argument_01_ = [&]() -> ::hgraph::Bool {
    ::hglrt::item(flags, 0) = true;
    return false;
}(); return ::hgl::demo::values::m_::pick_flag(argument_00_, argument_01_); }();
}

// bit_kept() -> bool (`const fn`), module demo.values, line 294
::hgraph::Bool bit_kept()
{
    [[maybe_unused]] ::hglrt::list<::hgraph::Bool> flags = ::hglrt::list<::hgraph::Bool>{false};
    [[maybe_unused]] const ::hgraph::Bool kept = ::hglrt::item(flags, 0);
    ::hglrt::item(flags, 0) = true;
    return kept;
}

// set_bits(flags: list<bool>) -> i64 (`const fn`), module demo.values, line 301
::hgraph::Int set_bits([[maybe_unused]] const ::hglrt::list<::hgraph::Bool>& flags)
{
    [[maybe_unused]] ::hgraph::Int count = 0;
    for (const ::hgraph::Bool& flag : flags)
    {
        if (flag)
        {
            count = ::hglrt::add__i64_i64(count, 1);
        }
    }
    return count;
}

// bump(count: i64, by: i64) (`const fn`), module demo.values, line 312
void bump([[maybe_unused]] ::hgraph::Int& count, [[maybe_unused]] ::hgraph::Int by)
{
    count = ::hglrt::add__i64_i64(count, by);
}

namespace i_::first
{
// first<T>(values: list<T>) -> T (`const fn`), the instance `i64`, module demo.values, line 74
::hgraph::Int of_i64([[maybe_unused]] const ::hglrt::list<::hgraph::Int>& values)
{
    return ::hglrt::item(values, 0);
}
}  // namespace i_::first

namespace i_::first
{
// first<T>(values: list<T>) -> T (`const fn`), the instance `str`, module demo.values, line 74
::hgraph::Str of_str([[maybe_unused]] const ::hglrt::list<::hgraph::Str>& values)
{
    return ::hglrt::item(values, 0);
}
}  // namespace i_::first

namespace i_::first
{
// first<T>(values: list<T>) -> T (`const fn`), the instance `bool`, module demo.values, line 74
::hgraph::Bool of_bool([[maybe_unused]] const ::hglrt::list<::hgraph::Bool>& values)
{
    return ::hglrt::item(values, 0);
}
}  // namespace i_::first

}  // namespace hgl::demo::values::m_

namespace hgl::demo::values::m_::t_
{

void conditions_choose()
{
    ::hglrt::testing::statement();
    ::hglrt::testing::check((([&] { const ::hgraph::Int argument_00_ = ::hgl::demo::values::m_::sign(::hglrt::neg__i64(5)); const ::hgraph::Int argument_01_ = ::hglrt::neg__i64(1); const ::hgraph::Float argument_02_ = 0.0; const ::hgraph::Float argument_03_ = 0.0; return ::hglrt::eq__i64_i64_f64_f64(argument_00_, argument_01_, argument_02_, argument_03_); }() && ::hglrt::eq__i64_i64_f64_f64(::hgl::demo::values::m_::sign(0), 0, 0.0, 0.0)) && ::hglrt::eq__i64_i64_f64_f64(::hgl::demo::values::m_::sign(7), 1, 0.0, 0.0)));
    ::hglrt::testing::statement();
    ::hglrt::testing::check((::hglrt::eq__i64_i64_f64_f64(::hgl::demo::values::m_::larger(2, 9), 9, 0.0, 0.0) && ::hglrt::eq__i64_i64_f64_f64(::hgl::demo::values::m_::larger(9, 2), 9, 0.0, 0.0)));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__i64_i64_f64_f64([&] { const ::hgraph::Int argument_04_ = ::hglrt::item(::hglrt::list<::hgraph::Int>{7}, 0); const ::hgraph::Int argument_05_ = ::hgl::demo::values::m_::sign(1); return ::hgl::demo::values::m_::larger(argument_04_, argument_05_); }(), 7, 0.0, 0.0));
}

void a_switch_compares_by_value()
{
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__T_T<::hgl::demo::values::m_::Side>(::hgl::demo::values::m_::opposite(::hgl::demo::values::m_::Side::buy), ::hgl::demo::values::m_::Side::sell));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__T_T<::hgl::demo::values::m_::Side>(::hgl::demo::values::m_::opposite(::hgl::demo::values::m_::Side::hold), ::hgl::demo::values::m_::Side::hold));
}

void loops_run()
{
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__i64_i64_f64_f64(::hgl::demo::values::m_::triangle(4), 10, 0.0, 0.0));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__T_T<::hglrt::list<::hgraph::Int>>(::hgl::demo::values::m_::evens(::hglrt::list<::hgraph::Int>{1, 2, 3, 4}), ::hglrt::list<::hgraph::Int>{2, 4}));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__T_T<::hglrt::list<::hgraph::Int>>(::hgl::demo::values::m_::bumped(::hglrt::list<::hgraph::Int>{1, 2, 3}, 1), ::hglrt::list<::hgraph::Int>{1, 3, 3}));
}

void defaults_and_nulls()
{
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__i64_i64_f64_f64(::hgl::demo::values::m_::scaled(3, 2, ::std::nullopt), 6, 0.0, 0.0));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__i64_i64_f64_f64(::hgl::demo::values::m_::scaled(3, 3, ::std::nullopt), 9, 0.0, 0.0));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__i64_i64_f64_f64(::hgl::demo::values::m_::scaled(3, 2, 1), 7, 0.0, 0.0));
}

void generics_by_instance()
{
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__i64_i64_f64_f64(::hgl::demo::values::m_::i_::first::of_i64(::hglrt::list<::hgraph::Int>{4, 5}), 4, 0.0, 0.0));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__T_T<::hgraph::Str>(::hgl::demo::values::m_::i_::first::of_str(::hglrt::list<::hgraph::Str>{::hgraph::Str{"a"}, ::hgraph::Str{"b"}}), ::hgraph::Str{"a"}));
}

void tuples_and_constants()
{
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__T_T<::std::tuple<::hgraph::Str, ::hgraph::Int>>(::hgl::demo::values::m_::swapped(::std::tuple<::hgraph::Int, ::hgraph::Str>{1, ::hgraph::Str{"one"}}), ::std::tuple<::hgraph::Str, ::hgraph::Int>{::hgraph::Str{"one"}, 1}));
    ::hglrt::testing::statement();
    ::hglrt::testing::check((::hglrt::eq__i64_i64_f64_f64(::hgl::demo::values::m_::limit(), 10, 0.0, 0.0) && ::hglrt::eq__T_T<::hgraph::TimeDelta>(::hgl::demo::values::m_::step(), ::hgraph::TimeDelta{5000000})));
}

void null_tests_of_any_value()
{
    ::hglrt::testing::statement();
    ::hglrt::testing::check((::hgl::demo::values::m_::present(3) && (!::hgl::demo::values::m_::present(::std::nullopt))));
    ::hglrt::testing::statement();
    ::hglrt::testing::check((!::hgl::demo::values::m_::never_null(1)));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hgl::demo::values::m_::null_is_null());
}

void bits_are_values()
{
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__T_T<::hglrt::list<::hgraph::Bool>>(::hgl::demo::values::m_::flipped(::hglrt::list<::hgraph::Bool>{true, false}), ::hglrt::list<::hgraph::Bool>{false, false}));
    ::hglrt::testing::statement();
    ::hglrt::testing::check((::hgl::demo::values::m_::first_flag(::hglrt::list<::hgraph::Bool>{true, false}) && ::hglrt::eq__T_T<::hgraph::Bool>(::hgl::demo::values::m_::i_::first::of_bool(::hglrt::list<::hgraph::Bool>{false, true}), false)));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(((!::hgl::demo::values::m_::first_bit_as_it_was()) && (!::hgl::demo::values::m_::bit_kept())));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__i64_i64_f64_f64(::hgl::demo::values::m_::set_bits(::hglrt::list<::hgraph::Bool>{true, false, true}), 2, 0.0, 0.0));
}

void results_propagate()
{
    ::hglrt::testing::statement();
    ::hglrt::testing::check((::hgl::demo::values::m_::doubled(3)).has_value());
    ::hglrt::testing::statement();
    ::hglrt::testing::check((!(::hgl::demo::values::m_::doubled(11)).has_value()));
}

void evaluation_follows_the_order_written()
{
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__i64_i64_f64_f64(::hgl::demo::values::m_::in_order(), 1, 0.0, 0.0));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__T_T<::hglrt::list<::hgraph::Int>>(::hgl::demo::values::m_::placed(), ::hglrt::list<::hgraph::Int>{15, 20}));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__i64_i64_f64_f64(::hgl::demo::values::m_::read_first(), 1, 0.0, 0.0));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__i64_i64_f64_f64(::hgl::demo::values::m_::list_first(), 20, 0.0, 0.0));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__T_T<::hglrt::list<::hgraph::Int>>(::hgl::demo::values::m_::index_first(), ::hglrt::list<::hgraph::Int>{10, 1}));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__T_T<::hglrt::list<::hglrt::list<::hgraph::Int>>>(::hgl::demo::values::m_::indices_in_order(), ::hglrt::list<::hglrt::list<::hgraph::Int>>{::hglrt::list<::hgraph::Int>{5, 2}, ::hglrt::list<::hgraph::Int>{3, 4}}));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__T_T<::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Int>>(::hgl::demo::values::m_::index_once(), ::std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Int>{::hglrt::list<::hgraph::Int>{10, 25, 30}, 1}));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__T_T<::hgraph::Str>(::hgl::demo::values::m_::held_text(), ::hgraph::Str{"text"}));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__T_T<::std::tuple<::hgraph::Int, ::hgraph::Int>>(::hgl::demo::values::m_::tuple_first(), ::std::tuple<::hgraph::Int, ::hgraph::Int>{1, 0}));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__T_T<::std::tuple<::hgraph::Str, ::hgraph::Int>>(::hgl::demo::values::m_::text_first(), ::std::tuple<::hgraph::Str, ::hgraph::Int>{::hgraph::Str{"before"}, 0}));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__i64_i64_f64_f64(::hgl::demo::values::m_::from_a_made_row(1), 7, 0.0, 0.0));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__i64_i64_f64_f64(::hgl::demo::values::m_::first_row_total(), 3, 0.0, 0.0));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__T_T<::hgraph::Str>(::hgl::demo::values::m_::made_error(), ::hgraph::Str{"before"}));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__T_T<::hgraph::Str>(::hgl::demo::values::m_::code_first(), ::hgraph::Str{"after"}));
}

void a_type_declared_after_its_use()
{
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__T_T<::hgl::demo::values::m_::Level>(::hgl::demo::values::m_::same_level(::hgl::demo::values::m_::Level::high), ::hgl::demo::values::m_::Level::high));
}

void a_var_parameter_is_its_place()
{
    ::hglrt::testing::statement();
    [[maybe_unused]] ::hgraph::Int count = 1;
    ::hglrt::testing::statement();
    ::hgl::demo::values::m_::bump(count, 2);
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__i64_i64_f64_f64(count, 3, 0.0, 0.0));
}

void an_overflow_fails_with_its_code()
{
    ::hglrt::testing::statement();
    ::hglrt::testing::raises("numeric.out_of_range", [&] {
        [[maybe_unused]] const ::hgraph::Int big = ::hglrt::add__i64_i64(::hgl::demo::values::m_::triangle(10), 9223372036854775807);
    });
}

void a_place_is_checked_before_its_value()
{
    ::hglrt::testing::statement();
    ::hglrt::testing::raises("value.index_out_of_range", [&] {
        [[maybe_unused]] const ::hglrt::list<::hgraph::Int> replaced = ::hgl::demo::values::m_::replaced_first();
    });
}

void a_place_fails_at_its_list_first()
{
    ::hglrt::testing::statement();
    ::hglrt::testing::raises("value.index_out_of_range", [&] {
        [[maybe_unused]] const ::hglrt::list<::hglrt::list<::hgraph::Int>> assigned = ::hgl::demo::values::m_::placed_at(0);
    });
    ::hglrt::testing::statement();
    ::hglrt::testing::raises("value.index_out_of_range", [&] {
        [[maybe_unused]] const ::hglrt::list<::hglrt::list<::hgraph::Int>> given = ::hgl::demo::values::m_::bumped_at(0);
    });
}

void a_list_fails_before_its_index()
{
    ::hglrt::testing::statement();
    [[maybe_unused]] const ::hgraph::Int zero = 0;
    ::hglrt::testing::statement();
    ::hglrt::testing::raises("numeric.out_of_range", [&] {
        [[maybe_unused]] const ::hgraph::Int element = [&]() -> ::hgraph::Int { const auto list_00_ = ::hgl::demo::values::m_::grown(1); const auto index_01_ = ::hglrt::floordiv__i64_i64<::hglrt::ZeroPolicy::fail>(1, zero); return ::hglrt::item(list_00_, index_01_); }();
    });
}

}  // namespace hgl::demo::values::m_::t_
