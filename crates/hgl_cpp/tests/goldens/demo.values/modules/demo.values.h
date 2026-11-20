// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#pragma once

#include <hglrt/hglrt.h>

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

// index_once() -> tuple<list<i64>, i64> (`const fn`), module demo.values, line 133
std::tuple<::hglrt::list<::hgraph::Int>, ::hgraph::Int> index_once();

// held_text() -> str (`const fn`), module demo.values, line 145
::hgraph::Str held_text();

// made_error() -> str (`const fn`), module demo.values, line 154
::hgraph::Str made_error();

// same_level(level: Level) -> Level (`const fn`), module demo.values, line 164
::hgl::demo::values::m_::Level same_level(::hgl::demo::values::m_::Level level);

// bump(count: i64, by: i64) (`const fn`), module demo.values, line 169
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
