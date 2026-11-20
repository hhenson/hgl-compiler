// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#pragma once

#include <hglrt/hglrt.h>
#include "demo.composites.types.h"

// Names this file writes, which no macro of a header above takes.
#undef Base
#undef Book
#undef Holder
#undef Line
#undef Point
#undef Round
#undef Shape
#undef Side
#undef arrived
#undef as_base
#undef as_shapes
#undef base
#undef base_id
#undef book
#undef booked
#undef books
#undef bump
#undef bump_after_growth
#undef bumped
#undef by
#undef change
#undef cleared
#undef composites
#undef count
#undef counted
#undef demo
#undef doubled_all
#undef failing
#undef first_price
#undef first_x
#undef held
#undef holder
#undef holds
#undef is_one
#undef joined
#undef key
#undef lhs
#undef line
#undef listed
#undef listed_in_order
#undef london
#undef members
#undef moved
#undef moved_in
#undef name
#undef named
#undef nudged
#undef number
#undef opening
#undef origin
#undef pair_change
#undef point
#undef points
#undef price_of
#undef read_before
#undef rhs
#undef rounds
#undef same
#undef seven
#undef shape
#undef side_named
#undef side_of
#undef size
#undef stretched
#undef tag_count
#undef tags
#undef total
#undef value
#undef wanted
#undef words
#undef written_order

namespace hgl::demo::composites::m_
{

// held(holder: Holder<list<i64, 2>>) -> list<i64, 2> (`const fn`), module demo.composites, line 79
std::array<::hgraph::Int, 2> held(const ::hgl::demo::composites::m_::Holder<std::array<::hgraph::Int, 2>>& holder);

namespace c_
{
// add_(lhs: Point, rhs: Point) -> Point (candidate `impl const fn`), module demo.composites, line 82
::hgl::demo::composites::m_::Point add_1_(::hgl::demo::composites::m_::Point lhs, ::hgl::demo::composites::m_::Point rhs);
}  // namespace c_

// origin() -> Point (`const fn`), module demo.composites, line 84
::hgl::demo::composites::m_::Point origin();

// moved(point: Point, by: f64) -> Point (`const fn`), module demo.composites, line 87
::hgl::demo::composites::m_::Point moved(::hgl::demo::composites::m_::Point point, ::hgraph::Float by);

// stretched(line: Line, by: f64) -> Line (`const fn`), module demo.composites, line 94
::hgl::demo::composites::m_::Line stretched(const ::hgl::demo::composites::m_::Line& line, ::hgraph::Float by);

// points() -> list<Point> (`const fn`), module demo.composites, line 101
::hglrt::list<::hgl::demo::composites::m_::Point> points();

// first_x() -> f64 (`const fn`), module demo.composites, line 104
::hgraph::Float first_x();

// named(shape: Shape) -> str (`const fn`), module demo.composites, line 106
::hgraph::Str named(const ::hgl::demo::composites::m_::Shape& shape);

// same(lhs: Shape, rhs: Shape) -> bool (`const fn`), module demo.composites, line 108
::hgraph::Bool same(const ::hgl::demo::composites::m_::Shape& lhs, const ::hgl::demo::composites::m_::Shape& rhs);

// as_base(shape: Shape) -> Base (`const fn`), module demo.composites, line 110
::hgl::demo::composites::m_::Base as_base(const ::hgl::demo::composites::m_::Shape& shape);

// base_id(base: Base) -> i64 (`const fn`), module demo.composites, line 112
::hgraph::Int base_id(const ::hgl::demo::composites::m_::Base& base);

// written_order() -> Point (`const fn`), module demo.composites, line 116
::hgl::demo::composites::m_::Point written_order();

// read_before() -> Point (`const fn`), module demo.composites, line 124
::hgl::demo::composites::m_::Point read_before();

// counted(words: list<str>) -> dict<str, i64> (`const fn`), module demo.composites, line 134
::hglrt::dict<::hgraph::Str, ::hgraph::Int> counted(const ::hglrt::list<::hgraph::Str>& words);

// bumped(book: dict<str, i64>, key: str) -> dict<str, i64> (`const fn`), module demo.composites, line 146
::hglrt::dict<::hgraph::Str, ::hgraph::Int> bumped(const ::hglrt::dict<::hgraph::Str, ::hgraph::Int>& book, const ::hgraph::Str& key);

// total(book: dict<str, i64>) -> i64 (`const fn`), module demo.composites, line 153
::hgraph::Int total(const ::hglrt::dict<::hgraph::Str, ::hgraph::Int>& book);

// size(members: set<f64>) -> i64 (`const fn`), module demo.composites, line 161
::hgraph::Int size(const ::hglrt::set<::hgraph::Float>& members);

// holds(members: set<str>, wanted: str) -> bool (`const fn`), module demo.composites, line 169
::hgraph::Bool holds(const ::hglrt::set<::hgraph::Str>& members, const ::hgraph::Str& wanted);

// tags() -> set<str> (`const fn`), module demo.composites, line 179
::hglrt::set<::hgraph::Str> tags();

// tag_count() -> i64 (`const fn`), module demo.composites, line 181
::hgraph::Int tag_count();

// doubled_all(book: dict<str, i64>) -> dict<str, i64> (`const fn`), module demo.composites, line 191
::hglrt::dict<::hgraph::Str, ::hgraph::Int> doubled_all(const ::hglrt::dict<::hgraph::Str, ::hgraph::Int>& book);

// price_of(book: Book, key: str) -> i64 (`const fn`), module demo.composites, line 201
::hgraph::Int price_of(const ::hgl::demo::composites::m_::Book& book, const ::hgraph::Str& key);

// first_price(books: list<Book>) -> i64 (`const fn`), module demo.composites, line 203
::hgraph::Int first_price(const ::hglrt::list<::hgl::demo::composites::m_::Book>& books);

// moved_in(book: dict<str, Point>) -> dict<str, Point> (`const fn`), module demo.composites, line 206
::hglrt::dict<::hgraph::Str, ::hgl::demo::composites::m_::Point> moved_in(const ::hglrt::dict<::hgraph::Str, ::hgl::demo::composites::m_::Point>& book);

// arrived(change: delta<set<str>>) -> set<str> (`const fn`), module demo.composites, line 213
::hglrt::set<::hgraph::Str> arrived(const ::hglrt::set_delta<::hgraph::Str>& change);

// joined(name: str) -> delta<set<str>> (`const fn`), module demo.composites, line 215
::hglrt::set_delta<::hgraph::Str> joined(const ::hgraph::Str& name);

// cleared() -> delta<f64?> (`const fn`), module demo.composites, line 217
std::optional<::hgraph::Float> cleared();

// seven() -> delta<i64?> (`const fn`), module demo.composites, line 219
std::optional<::hgraph::Int> seven();

// booked() -> delta<dict<str, i64>> (`const fn`), module demo.composites, line 221
::hglrt::dict_delta<::hgraph::Str, ::hgraph::Int> booked();

// listed() -> delta<list<i64>> (`const fn`), module demo.composites, line 223
::hglrt::list_delta<::hglrt::list<::hgraph::Int>> listed();

// nudged() -> delta<Point> (`const fn`), module demo.composites, line 225
::hgl::demo::composites::m_::d_::Point nudged();

// listed_in_order() -> delta<list<i64>> (`const fn`), module demo.composites, line 229
::hglrt::list_delta<::hglrt::list<::hgraph::Int>> listed_in_order();

// pair_change() -> tuple<f64?, i64?> (`const fn`), module demo.composites, line 237
std::tuple<std::optional<::hgraph::Float>, std::optional<::hgraph::Int>> pair_change();

// side_of(number: i64) -> Side (`const fn`), module demo.composites, line 240
::hgl::demo::composites::m_::Side side_of(::hgraph::Int number);

// side_named(name: str) -> Side (`const fn`), module demo.composites, line 242
::hgl::demo::composites::m_::Side side_named(const ::hgraph::Str& name);

// london() -> timezone (`const fn`), module demo.composites, line 245
::hgraph::ZoneId london();

// opening() -> zoned_time (`const fn`), module demo.composites, line 247
::hgraph::ZonedTime opening();

// is_one(value: i64?) -> bool (`const fn`), module demo.composites, line 250
::hgraph::Bool is_one(std::optional<::hgraph::Int> value);

// bump(count: i64, by: i64) (`const fn`), module demo.composites, line 254
void bump(::hgraph::Int& count, ::hgraph::Int by);

// bump_after_growth() -> list<i64> (`const fn`), module demo.composites, line 258
::hglrt::list<::hgraph::Int> bump_after_growth();

// as_shapes(rounds: list<Round>) -> list<Shape> (`const fn`), module demo.composites, line 268
::hglrt::list<::hgl::demo::composites::m_::Shape> as_shapes(const ::hglrt::list<::hgl::demo::composites::m_::Round>& rounds);

// failing() -> i64 (`const fn`), module demo.composites, line 362
::hgraph::Int failing();

}  // namespace hgl::demo::composites::m_
