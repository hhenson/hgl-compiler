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
#undef Reading
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
#undef look
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
#undef published_null
#undef read_before
#undef rhs
#undef rounds
#undef same
#undef seven
#undef shape
#undef side_named
#undef side_of
#undef size
#undef std
#undef stretched
#undef tag_count
#undef tags
#undef total
#undef unchanged
#undef value
#undef wanted
#undef words
#undef written_order

namespace hgl::demo::composites::m_
{

// held(holder: Holder<list<i64, 2>>) -> list<i64, 2> (`const fn`), module demo.composites, line 86
::std::array<::hgraph::Int, 2> held(const ::hgl::demo::composites::m_::Holder<::std::array<::hgraph::Int, 2>>& holder);

// look(book: dict<str, delta<i64?>>, key: str) -> delta<i64?>? (`const fn`), module demo.composites, line 122
::std::optional<::hglrt::nullable_delta<::hgraph::Int, ::hgraph::Int>> look(const ::hglrt::dict<::hgraph::Str, ::hglrt::nullable_delta<::hgraph::Int, ::hgraph::Int>>& book, const ::hgraph::Str& key);

// published_null() -> delta<Reading> (`const fn`), module demo.composites, line 124
::hgl::demo::composites::m_::d_::Reading published_null();

// unchanged() -> delta<Reading> (`const fn`), module demo.composites, line 126
::hgl::demo::composites::m_::d_::Reading unchanged();

namespace c_
{
// add_(lhs: Point, rhs: Point) -> Point (candidate `impl const fn`), module demo.composites, line 129
::hgl::demo::composites::m_::Point add_1_1Point_1Point_d28_(::hgl::demo::composites::m_::Point lhs, ::hgl::demo::composites::m_::Point rhs);
}  // namespace c_

// origin() -> Point (`const fn`), module demo.composites, line 131
::hgl::demo::composites::m_::Point origin();

// moved(point: Point, by: f64) -> Point (`const fn`), module demo.composites, line 134
::hgl::demo::composites::m_::Point moved(::hgl::demo::composites::m_::Point point, ::hgraph::Float by);

// stretched(line: Line, by: f64) -> Line (`const fn`), module demo.composites, line 141
::hgl::demo::composites::m_::Line stretched(const ::hgl::demo::composites::m_::Line& line, ::hgraph::Float by);

// points() -> list<Point> (`const fn`), module demo.composites, line 148
::hglrt::list<::hgl::demo::composites::m_::Point> points();

// first_x() -> f64 (`const fn`), module demo.composites, line 151
::hgraph::Float first_x();

// named(shape: Shape) -> str (`const fn`), module demo.composites, line 153
::hgraph::Str named(const ::hgl::demo::composites::m_::Shape& shape);

// same(lhs: Shape, rhs: Shape) -> bool (`const fn`), module demo.composites, line 155
::hgraph::Bool same(const ::hgl::demo::composites::m_::Shape& lhs, const ::hgl::demo::composites::m_::Shape& rhs);

// as_base(shape: Shape) -> Base (`const fn`), module demo.composites, line 157
::hgl::demo::composites::m_::Base as_base(const ::hgl::demo::composites::m_::Shape& shape);

// base_id(base: Base) -> i64 (`const fn`), module demo.composites, line 159
::hgraph::Int base_id(const ::hgl::demo::composites::m_::Base& base);

// written_order() -> Point (`const fn`), module demo.composites, line 163
::hgl::demo::composites::m_::Point written_order();

// read_before() -> Point (`const fn`), module demo.composites, line 171
::hgl::demo::composites::m_::Point read_before();

// counted(words: list<str>) -> dict<str, i64> (`const fn`), module demo.composites, line 181
::hglrt::dict<::hgraph::Str, ::hgraph::Int> counted(const ::hglrt::list<::hgraph::Str>& words);

// bumped(book: dict<str, i64>, key: str) -> dict<str, i64> (`const fn`), module demo.composites, line 193
::hglrt::dict<::hgraph::Str, ::hgraph::Int> bumped(const ::hglrt::dict<::hgraph::Str, ::hgraph::Int>& book, const ::hgraph::Str& key);

// total(book: dict<str, i64>) -> i64 (`const fn`), module demo.composites, line 200
::hgraph::Int total(const ::hglrt::dict<::hgraph::Str, ::hgraph::Int>& book);

// size(members: set<f64>) -> i64 (`const fn`), module demo.composites, line 208
::hgraph::Int size(const ::hglrt::set<::hgraph::Float>& members);

// holds(members: set<str>, wanted: str) -> bool (`const fn`), module demo.composites, line 216
::hgraph::Bool holds(const ::hglrt::set<::hgraph::Str>& members, const ::hgraph::Str& wanted);

// tags() -> set<str> (`const fn`), module demo.composites, line 226
::hglrt::set<::hgraph::Str> tags();

// tag_count() -> i64 (`const fn`), module demo.composites, line 228
::hgraph::Int tag_count();

// doubled_all(book: dict<str, i64>) -> dict<str, i64> (`const fn`), module demo.composites, line 238
::hglrt::dict<::hgraph::Str, ::hgraph::Int> doubled_all(const ::hglrt::dict<::hgraph::Str, ::hgraph::Int>& book);

// price_of(book: Book, key: str) -> i64 (`const fn`), module demo.composites, line 248
::hgraph::Int price_of(const ::hgl::demo::composites::m_::Book& book, const ::hgraph::Str& key);

// first_price(books: list<Book>) -> i64 (`const fn`), module demo.composites, line 250
::hgraph::Int first_price(const ::hglrt::list<::hgl::demo::composites::m_::Book>& books);

// moved_in(book: dict<str, Point>) -> dict<str, Point> (`const fn`), module demo.composites, line 253
::hglrt::dict<::hgraph::Str, ::hgl::demo::composites::m_::Point> moved_in(const ::hglrt::dict<::hgraph::Str, ::hgl::demo::composites::m_::Point>& book);

// arrived(change: delta<set<str>>) -> set<str> (`const fn`), module demo.composites, line 260
::hglrt::set<::hgraph::Str> arrived(const ::hglrt::set_delta<::hgraph::Str>& change);

// joined(name: str) -> delta<set<str>> (`const fn`), module demo.composites, line 262
::hglrt::set_delta<::hgraph::Str> joined(const ::hgraph::Str& name);

// cleared() -> delta<f64?> (`const fn`), module demo.composites, line 264
::hglrt::nullable_delta<::hgraph::Float, ::hgraph::Float> cleared();

// seven() -> delta<i64?> (`const fn`), module demo.composites, line 266
::hglrt::nullable_delta<::hgraph::Int, ::hgraph::Int> seven();

// booked() -> delta<dict<str, i64>> (`const fn`), module demo.composites, line 268
::hglrt::dict_delta<::hgraph::Str, ::hgraph::Int> booked();

// listed() -> delta<list<i64>> (`const fn`), module demo.composites, line 270
::hglrt::list_delta<::hglrt::list<::hgraph::Int>> listed();

// nudged() -> delta<Point> (`const fn`), module demo.composites, line 272
::hgl::demo::composites::m_::d_::Point nudged();

// listed_in_order() -> delta<list<i64>> (`const fn`), module demo.composites, line 276
::hglrt::list_delta<::hglrt::list<::hgraph::Int>> listed_in_order();

// pair_change() -> tuple<f64?, i64?> (`const fn`), module demo.composites, line 284
::std::tuple<::std::optional<::hgraph::Float>, ::std::optional<::hgraph::Int>> pair_change();

// side_of(number: i64) -> Side (`const fn`), module demo.composites, line 287
::hgl::demo::composites::m_::Side side_of(::hgraph::Int number);

// side_named(name: str) -> Side (`const fn`), module demo.composites, line 289
::hgl::demo::composites::m_::Side side_named(const ::hgraph::Str& name);

// london() -> timezone (`const fn`), module demo.composites, line 292
::hgraph::ZoneId london();

// opening() -> zoned_time (`const fn`), module demo.composites, line 294
::hgraph::ZonedTime opening();

// is_one(value: i64?) -> bool (`const fn`), module demo.composites, line 297
::hgraph::Bool is_one(::std::optional<::hgraph::Int> value);

// bump(count: i64, by: i64) (`const fn`), module demo.composites, line 301
void bump(::hgraph::Int& count, ::hgraph::Int by);

// bump_after_growth() -> list<i64> (`const fn`), module demo.composites, line 305
::hglrt::list<::hgraph::Int> bump_after_growth();

// as_shapes(rounds: list<Round>) -> list<Shape> (`const fn`), module demo.composites, line 315
::hglrt::list<::hgl::demo::composites::m_::Shape> as_shapes(const ::hglrt::list<::hgl::demo::composites::m_::Round>& rounds);

// failing() -> i64 (`const fn`), module demo.composites, line 414
::hgraph::Int failing();

}  // namespace hgl::demo::composites::m_
