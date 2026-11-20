// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#pragma once

#include <hglrt/hglrt.h>

// Names this file writes, which no macro of a header above takes.
#undef Level
#undef Side
#undef a
#undef at
#undef b
#undef bump
#undef bumped
#undef bumped_at
#undef buy
#undef by
#undef checked
#undef code_first
#undef count
#undef demo
#undef divisor
#undef doubled
#undef evens
#undef factor
#undef first
#undef first_row_total
#undef from_a_made_row
#undef grown
#undef held_text
#undef high
#undef hold
#undef in_order
#undef index_first
#undef index_once
#undef indices_in_order
#undef larger
#undef level
#undef limit
#undef list_first
#undef low
#undef made_error
#undef made_rows
#undef never_null
#undef null_is_null
#undef offset
#undef one
#undef opposite
#undef pair
#undef pick
#undef placed
#undef placed_at
#undef present
#undef read_first
#undef replaced_first
#undef same_level
#undef scaled
#undef sell
#undef side
#undef sign
#undef step
#undef swapped
#undef text_first
#undef triangle
#undef tuple_first
#undef value
#undef values

namespace hgl::demo::values::m_
{

// enum Side (TYPE-7)
enum class Side
{
    buy,
    sell,
    hold,
};

// enum Level (TYPE-7)
enum class Level
{
    low,
    high,
};

// const limit: i64 (UNIT-13), module demo.values, line 9
const ::hgraph::Int& limit();

// const step: duration (UNIT-13), module demo.values, line 10
const ::hgraph::TimeDelta& step();

// sign(value: i64) -> i64 (`const fn`), module demo.values, line 13
::hgraph::Int sign(::hgraph::Int value);

// larger(a: i64, b: i64) -> i64 (`const fn`), module demo.values, line 22
::hgraph::Int larger(::hgraph::Int a, ::hgraph::Int b);

// opposite(side: Side) -> Side (`const fn`), module demo.values, line 25
::hgl::demo::values::m_::Side opposite(::hgl::demo::values::m_::Side side);

// triangle(count: i64) -> i64 (`const fn`), module demo.values, line 37
::hgraph::Int triangle(::hgraph::Int count);

// evens(values: list<i64>) -> list<i64> (`const fn`), module demo.values, line 48
::hglrt::list<::hgraph::Int> evens(const ::hglrt::list<::hgraph::Int>& values);

// bumped(values: list<i64>, at: i64) -> list<i64> (`const fn`), module demo.values, line 59
::hglrt::list<::hgraph::Int> bumped(const ::hglrt::list<::hgraph::Int>& values, ::hgraph::Int at);

// scaled(value: i64, factor: i64 = ..., offset: i64? = ...) -> i64 (`const fn`), module demo.values, line 66
::hgraph::Int scaled(::hgraph::Int value, ::hgraph::Int factor, std::optional<::hgraph::Int> offset);

// swapped(pair: tuple<i64, str>) -> tuple<str, i64> (`const fn`), module demo.values, line 77
std::tuple<::hgraph::Str, ::hgraph::Int> swapped(const std::tuple<::hgraph::Int, ::hgraph::Str>& pair);

// checked(value: i64) -> result<i64, error> (`const fn`), module demo.values, line 80
::hglrt::result<::hgraph::Int> checked(::hgraph::Int value);

// doubled(value: i64) -> result<i64, error> (`const fn`), module demo.values, line 87
::hglrt::result<::hgraph::Int> doubled(::hgraph::Int value);

// pick(a: i64, b: i64) -> i64 (`const fn`), module demo.values, line 95
::hgraph::Int pick(::hgraph::Int a, ::hgraph::Int b);

// in_order() -> i64 (`const fn`), module demo.values, line 97
::hgraph::Int in_order();

// placed() -> list<i64> (`const fn`), module demo.values, line 105
::hglrt::list<::hgraph::Int> placed();

// read_first() -> i64 (`const fn`), module demo.values, line 115
::hgraph::Int read_first();

// list_first() -> i64 (`const fn`), module demo.values, line 123
::hgraph::Int list_first();

// replaced_first() -> list<i64> (`const fn`), module demo.values, line 133
::hglrt::list<::hgraph::Int> replaced_first();

// placed_at(divisor: i64) -> list<list<i64>> (`const fn`), module demo.values, line 144
::hglrt::list<::hglrt::list<::hgraph::Int>> placed_at(::hgraph::Int divisor);

// bumped_at(divisor: i64) -> list<list<i64>> (`const fn`), module demo.values, line 150
::hglrt::list<::hglrt::list<::hgraph::Int>> bumped_at(::hgraph::Int divisor);

// indices_in_order() -> list<list<i64>> (`const fn`), module demo.values, line 158
::hglrt::list<::hglrt::list<::hgraph::Int>> indices_in_order();

// present(a: i64?) -> bool (`const fn`), module demo.values, line 170
::hgraph::Bool present(std::optional<::hgraph::Int> a);

// never_null(a: i64) -> bool (`const fn`), module demo.values, line 177
::hgraph::Bool never_null(::hgraph::Int a);

// null_is_null() -> bool (`const fn`), module demo.values, line 179
::hgraph::Bool null_is_null();

// index_first() -> list<i64> (`const fn`), module demo.values, line 182
::hglrt::list<::hgraph::Int> index_first();

// index_once() -> tuple<list<i64>, i64> (`const fn`), module demo.values, line 193
std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Int> index_once();

// held_text() -> str (`const fn`), module demo.values, line 205
::hgraph::Str held_text();

// tuple_first() -> tuple<i64, i64> (`const fn`), module demo.values, line 216
std::tuple<::hgraph::Int, ::hgraph::Int> tuple_first();

// text_first() -> tuple<str, i64> (`const fn`), module demo.values, line 224
std::tuple<::hgraph::Str, ::hgraph::Int> text_first();

// from_a_made_row(one: i64) -> i64 (`const fn`), module demo.values, line 234
::hgraph::Int from_a_made_row(::hgraph::Int one);

// made_rows() -> list<list<i64>> (`const fn`), module demo.values, line 237
::hglrt::list<::hglrt::list<::hgraph::Int>> made_rows();

// first_row_total() -> i64 (`const fn`), module demo.values, line 239
::hgraph::Int first_row_total();

// grown(by: i64) -> list<i64> (`const fn`), module demo.values, line 248
::hglrt::list<::hgraph::Int> grown(::hgraph::Int by);

// made_error() -> str (`const fn`), module demo.values, line 250
::hgraph::Str made_error();

// code_first() -> str (`const fn`), module demo.values, line 260
::hgraph::Str code_first();

// same_level(level: Level) -> Level (`const fn`), module demo.values, line 270
::hgl::demo::values::m_::Level same_level(::hgl::demo::values::m_::Level level);

// bump(count: i64, by: i64) (`const fn`), module demo.values, line 275
void bump(::hgraph::Int& count, ::hgraph::Int by);

namespace i_::first
{
// first<T>(values: list<T>) -> T (`const fn`), the instance `i64`, module demo.values, line 74
::hgraph::Int of_i64(const ::hglrt::list<::hgraph::Int>& values);
}  // namespace i_::first

namespace i_::first
{
// first<T>(values: list<T>) -> T (`const fn`), the instance `str`, module demo.values, line 74
::hgraph::Str of_str(const ::hglrt::list<::hgraph::Str>& values);
}  // namespace i_::first

}  // namespace hgl::demo::values::m_
