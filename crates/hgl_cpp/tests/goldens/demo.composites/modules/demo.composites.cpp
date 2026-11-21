// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include "demo.composites.h"

// Names this file writes, which no macro of a header above takes.
#undef Base
#undef Book
#undef Box
#undef Empty
#undef Filled
#undef Flags
#undef Holder
#undef Line
#undef Memo
#undef Noted
#undef Outcome
#undef Pair
#undef Point
#undef Reading
#undef Round
#undef Shape
#undef Shelf
#undef Side
#undef Sided
#undef Stocked
#undef Tagged
#undef ZeroPolicy
#undef a_dictionarys_place_comes_before_its_key
#undef a_failing_value_adds_no_key
#undef a_missing_key_fails
#undef a_nullable_delta_is_no_nullable
#undef a_struct_holding_nan_is_not_equal_to_itself
#undef a_var_place_is_taken_where_the_call_is_made
#undef absent
#undef an_empty_default_of_an_optional_collection_is_the_collection
#undef arguments_are_evaluated_in_the_order_written
#undef arrived
#undef as_base
#undef as_shapes
#undef at
#undef base
#undef base_id
#undef bits_are_held_as_they_were_read
#undef book
#undef booked
#undef books
#undef bump
#undef bump_after_growth
#undef bumped
#undef buy
#undef by
#undef change
#undef changed
#undef cleared
#undef composites
#undef count
#undef counted
#undef counts
#undef deltas_are_values
#undef demo
#undef dictionaries_read_add_and_loop
#undef doubled_all
#undef empty
#undef end
#undef enums_zones_and_nullables
#undef failing
#undef fields_default_and_nest
#undef first
#undef first_price
#undef first_x
#undef found
#undef generic_and_abstract_structs
#undef held
#undef holder
#undef holds
#undef id
#undef ieee
#undef is_one
#undef item
#undef items
#undef joined
#undef kept
#undef key
#undef level
#undef lhs
#undef line
#undef listed
#undef listed_in_order
#undef london
#undef longer
#undef look
#undef made
#undef maybes
#undef member
#undef members
#undef more
#undef moved
#undef moved_in
#undef name
#undef named
#undef names
#undef next
#undef none
#undef note
#undef noted
#undef nudged
#undef number
#undef odd
#undef on
#undef opening
#undef origin
#undef pair_change
#undef point
#undef points
#undef price_of
#undef prices
#undef published_null
#undef read_before
#undef reads
#undef rhs
#undef round
#undef rounds
#undef rows
#undef same
#undef seen
#undef sell
#undef sets_have_no_order
#undef seven
#undef shape
#undef shelf
#undef side_named
#undef side_of
#undef size
#undef std
#undef stretched
#undef structs_are_values
#undef sum
#undef tag
#undef tag_count
#undef tags
#undef total
#undef unchanged
#undef value
#undef wanted
#undef word
#undef words
#undef written_order
#undef x
#undef y

namespace hgl::demo::composites::m_
{

// held(holder: Holder<list<i64, 2>>) -> list<i64, 2> (`const fn`), module demo.composites, line 86
::std::array<::hgraph::Int, 2> held([[maybe_unused]] const ::hgl::demo::composites::m_::Holder<::std::array<::hgraph::Int, 2>>& holder)
{
    return (holder).value();
}

// look(book: dict<str, delta<i64?>>, key: str) -> delta<i64?>? (`const fn`), module demo.composites, line 122
::std::optional<::hglrt::nullable_delta<::hgraph::Int, ::hgraph::Int>> look([[maybe_unused]] const ::hglrt::dict<::hgraph::Str, ::hglrt::nullable_delta<::hgraph::Int, ::hgraph::Int>>& book, [[maybe_unused]] const ::hgraph::Str& key)
{
    return ::hglrt::result_or_null(::hglrt::get_dict_result<::hgraph::Str, ::hglrt::nullable_delta<::hgraph::Int, ::hgraph::Int>>(book, key));
}

// published_null() -> delta<Reading> (`const fn`), module demo.composites, line 124
::hgl::demo::composites::m_::d_::Reading published_null()
{
    return ::hgl::demo::composites::m_::d_::Reading{::hglrt::nullable_delta<::hgraph::Int, ::hgraph::Int>{}};
}

// unchanged() -> delta<Reading> (`const fn`), module demo.composites, line 126
::hgl::demo::composites::m_::d_::Reading unchanged()
{
    return ::hgl::demo::composites::m_::d_::Reading{::std::nullopt};
}

namespace c_
{
// add_(lhs: Point, rhs: Point) -> Point (candidate `impl const fn`), module demo.composites, line 129
::hgl::demo::composites::m_::Point add_1_([[maybe_unused]] ::hgl::demo::composites::m_::Point lhs, [[maybe_unused]] ::hgl::demo::composites::m_::Point rhs)
{
    return [&]() -> ::hgl::demo::composites::m_::Point { const ::hgraph::Float field_00_ = ::hglrt::add__f64_f64((lhs).x, (rhs).x); const ::hgraph::Float field_01_ = ::hglrt::add__f64_f64((lhs).y, (rhs).y); return ::hgl::demo::composites::m_::Point{field_00_, field_01_}; }();
}
}  // namespace c_

// origin() -> Point (`const fn`), module demo.composites, line 131
::hgl::demo::composites::m_::Point origin()
{
    return ::hgl::demo::composites::m_::Point{0.0, 0.0};
}

// moved(point: Point, by: f64) -> Point (`const fn`), module demo.composites, line 134
::hgl::demo::composites::m_::Point moved([[maybe_unused]] ::hgl::demo::composites::m_::Point point, [[maybe_unused]] ::hgraph::Float by)
{
    [[maybe_unused]] ::hgl::demo::composites::m_::Point next = point;
    next.x = ::hglrt::add__f64_f64(next.x, by);
    next.y = ::hglrt::add__f64_f64((next).y, by);
    return next;
}

// stretched(line: Line, by: f64) -> Line (`const fn`), module demo.composites, line 141
::hgl::demo::composites::m_::Line stretched([[maybe_unused]] const ::hgl::demo::composites::m_::Line& line, [[maybe_unused]] ::hgraph::Float by)
{
    [[maybe_unused]] ::hgl::demo::composites::m_::Line next = line;
    next.end.x = ::hglrt::add__f64_f64(next.end.x, by);
    next.class_ = 4;
    return next;
}

// points() -> list<Point> (`const fn`), module demo.composites, line 148
::hglrt::list<::hgl::demo::composites::m_::Point> points()
{
    return ::hglrt::list<::hgl::demo::composites::m_::Point>{::hgl::demo::composites::m_::Point{1.0, 2.0}, ::hgl::demo::composites::m_::Point{3.0, 0.0}};
}

// first_x() -> f64 (`const fn`), module demo.composites, line 151
::hgraph::Float first_x()
{
    return (::hglrt::item(::hgl::demo::composites::m_::points(), 0)).x;
}

// named(shape: Shape) -> str (`const fn`), module demo.composites, line 153
::hgraph::Str named([[maybe_unused]] const ::hgl::demo::composites::m_::Shape& shape)
{
    return (shape).name();
}

// same(lhs: Shape, rhs: Shape) -> bool (`const fn`), module demo.composites, line 155
::hgraph::Bool same([[maybe_unused]] const ::hgl::demo::composites::m_::Shape& lhs, [[maybe_unused]] const ::hgl::demo::composites::m_::Shape& rhs)
{
    return ::hglrt::eq__T_T<::hgl::demo::composites::m_::Shape>(lhs, rhs);
}

// as_base(shape: Shape) -> Base (`const fn`), module demo.composites, line 157
::hgl::demo::composites::m_::Base as_base([[maybe_unused]] const ::hgl::demo::composites::m_::Shape& shape)
{
    return shape;
}

// base_id(base: Base) -> i64 (`const fn`), module demo.composites, line 159
::hgraph::Int base_id([[maybe_unused]] const ::hgl::demo::composites::m_::Base& base)
{
    return (base).id();
}

// written_order() -> Point (`const fn`), module demo.composites, line 163
::hgl::demo::composites::m_::Point written_order()
{
    [[maybe_unused]] ::hgraph::Float at = 1.0;
    return [&]() -> ::hgl::demo::composites::m_::Point { const ::hgraph::Float field_00_ = [&]() -> ::hgraph::Float {
    at = 2.0;
    return at;
}(); const ::hgraph::Float field_01_ = at; return ::hgl::demo::composites::m_::Point{field_01_, field_00_}; }();
}

// read_before() -> Point (`const fn`), module demo.composites, line 171
::hgl::demo::composites::m_::Point read_before()
{
    [[maybe_unused]] ::hgraph::Float at = 1.0;
    return [&]() -> ::hgl::demo::composites::m_::Point { const ::hgraph::Float field_00_ = at; const ::hgraph::Float field_01_ = [&]() -> ::hgraph::Float {
    at = 5.0;
    return at;
}(); return ::hgl::demo::composites::m_::Point{field_00_, field_01_}; }();
}

// counted(words: list<str>) -> dict<str, i64> (`const fn`), module demo.composites, line 181
::hglrt::dict<::hgraph::Str, ::hgraph::Int> counted([[maybe_unused]] const ::hglrt::list<::hgraph::Str>& words)
{
    [[maybe_unused]] ::hglrt::dict<::hgraph::Str, ::hgraph::Int> counts = ::hglrt::dict<::hgraph::Str, ::hgraph::Int>{};
    for (const ::hgraph::Str& word : words)
    {
        if ((!(::hglrt::result_or_null(::hglrt::get_dict_result<::hgraph::Str, ::hgraph::Int>(counts, word))).has_value()))
        {
            ::hglrt::entry(counts, word) = 1;
        }
        else
        {
            const ::hgraph::Str key_00_ = word;
            static_cast<void>(::hglrt::entry_at(counts, key_00_));
            ::hglrt::entry_at(counts, key_00_) = ::hglrt::add__i64_i64(::hglrt::entry_at(counts, key_00_), 1);
        }
    }
    return counts;
}

// bumped(book: dict<str, i64>, key: str) -> dict<str, i64> (`const fn`), module demo.composites, line 193
::hglrt::dict<::hgraph::Str, ::hgraph::Int> bumped([[maybe_unused]] const ::hglrt::dict<::hgraph::Str, ::hgraph::Int>& book, [[maybe_unused]] const ::hgraph::Str& key)
{
    [[maybe_unused]] ::hglrt::dict<::hgraph::Str, ::hgraph::Int> changed = book;
    const ::hgraph::Str key_00_ = key;
    static_cast<void>(::hglrt::entry_at(changed, key_00_));
    ::hglrt::entry_at(changed, key_00_) = ::hglrt::add__i64_i64(::hglrt::entry_at(changed, key_00_), 1);
    return changed;
}

// total(book: dict<str, i64>) -> i64 (`const fn`), module demo.composites, line 200
::hgraph::Int total([[maybe_unused]] const ::hglrt::dict<::hgraph::Str, ::hgraph::Int>& book)
{
    [[maybe_unused]] ::hgraph::Int sum = 0;
    for ([[maybe_unused]] const auto& [key, value] : book)
    {
        sum = ::hglrt::add__i64_i64(sum, value);
    }
    return sum;
}

// size(members: set<f64>) -> i64 (`const fn`), module demo.composites, line 208
::hgraph::Int size([[maybe_unused]] const ::hglrt::set<::hgraph::Float>& members)
{
    [[maybe_unused]] ::hgraph::Int count = 0;
    for ([[maybe_unused]] const ::hgraph::Float& member : members)
    {
        count = ::hglrt::add__i64_i64(count, 1);
    }
    return count;
}

// holds(members: set<str>, wanted: str) -> bool (`const fn`), module demo.composites, line 216
::hgraph::Bool holds([[maybe_unused]] const ::hglrt::set<::hgraph::Str>& members, [[maybe_unused]] const ::hgraph::Str& wanted)
{
    for ([[maybe_unused]] const ::hgraph::Str& member : members)
    {
        if (::hglrt::eq__T_T<::hgraph::Str>(member, wanted))
        {
            return true;
        }
    }
    return false;
}

// tags() -> set<str> (`const fn`), module demo.composites, line 226
::hglrt::set<::hgraph::Str> tags()
{
    return ::hglrt::set<::hgraph::Str>{::hgraph::Str{"a"}, ::hgraph::Str{"b"}};
}

// tag_count() -> i64 (`const fn`), module demo.composites, line 228
::hgraph::Int tag_count()
{
    [[maybe_unused]] ::hgraph::Int count = 0;
    const ::hglrt::set<::hgraph::Str> items_00_ = ::hgl::demo::composites::m_::tags();
    for ([[maybe_unused]] const ::hgraph::Str& tag : items_00_)
    {
        count = ::hglrt::add__i64_i64(count, 1);
    }
    return count;
}

// doubled_all(book: dict<str, i64>) -> dict<str, i64> (`const fn`), module demo.composites, line 238
::hglrt::dict<::hgraph::Str, ::hgraph::Int> doubled_all([[maybe_unused]] const ::hglrt::dict<::hgraph::Str, ::hgraph::Int>& book)
{
    [[maybe_unused]] ::hglrt::dict<::hgraph::Str, ::hgraph::Int> changed = book;
    const ::hglrt::dict<::hgraph::Str, ::hgraph::Int> items_00_ = changed;
    for ([[maybe_unused]] const auto& [key, value] : items_00_)
    {
        const ::hgraph::Str key_01_ = key;
        static_cast<void>(::hglrt::entry(changed, key_01_));
        ::hglrt::entry(changed, key_01_) = ::hglrt::mul__i64_i64(value, 2);
        ::hglrt::entry(changed, ::hgraph::Str{"extra"}) = 0;
    }
    return changed;
}

// price_of(book: Book, key: str) -> i64 (`const fn`), module demo.composites, line 248
::hgraph::Int price_of([[maybe_unused]] const ::hgl::demo::composites::m_::Book& book, [[maybe_unused]] const ::hgraph::Str& key)
{
    return ::hglrt::entry_at((book).prices, key);
}

// first_price(books: list<Book>) -> i64 (`const fn`), module demo.composites, line 250
::hgraph::Int first_price([[maybe_unused]] const ::hglrt::list<::hgl::demo::composites::m_::Book>& books)
{
    return ::hglrt::entry_at((::hglrt::item(books, 0)).prices, ::hgraph::Str{"a"});
}

// moved_in(book: dict<str, Point>) -> dict<str, Point> (`const fn`), module demo.composites, line 253
::hglrt::dict<::hgraph::Str, ::hgl::demo::composites::m_::Point> moved_in([[maybe_unused]] const ::hglrt::dict<::hgraph::Str, ::hgl::demo::composites::m_::Point>& book)
{
    [[maybe_unused]] ::hglrt::dict<::hgraph::Str, ::hgl::demo::composites::m_::Point> changed = book;
    ::hglrt::entry_at(changed, ::hgraph::Str{"a"}).x = 9.0;
    return changed;
}

// arrived(change: delta<set<str>>) -> set<str> (`const fn`), module demo.composites, line 260
::hglrt::set<::hgraph::Str> arrived([[maybe_unused]] const ::hglrt::set_delta<::hgraph::Str>& change)
{
    return (change).added;
}

// joined(name: str) -> delta<set<str>> (`const fn`), module demo.composites, line 262
::hglrt::set_delta<::hgraph::Str> joined([[maybe_unused]] const ::hgraph::Str& name)
{
    return ::hglrt::set_delta<::hgraph::Str>{::hglrt::set<::hgraph::Str>{name}, ::hglrt::set<::hgraph::Str>{}};
}

// cleared() -> delta<f64?> (`const fn`), module demo.composites, line 264
::hglrt::nullable_delta<::hgraph::Float, ::hgraph::Float> cleared()
{
    return ::hglrt::nullable_delta<::hgraph::Float, ::hgraph::Float>{};
}

// seven() -> delta<i64?> (`const fn`), module demo.composites, line 266
::hglrt::nullable_delta<::hgraph::Int, ::hgraph::Int> seven()
{
    return ::hglrt::nullable_delta<::hgraph::Int, ::hgraph::Int>{7};
}

// booked() -> delta<dict<str, i64>> (`const fn`), module demo.composites, line 268
::hglrt::dict_delta<::hgraph::Str, ::hgraph::Int> booked()
{
    return ::hglrt::dict_delta<::hgraph::Str, ::hgraph::Int>{::hglrt::dict_of<::hgraph::Str, ::hgraph::Int>({{::hgraph::Str{"a"}, 1}}), ::hglrt::set<::hgraph::Str>{::hgraph::Str{"b"}}};
}

// listed() -> delta<list<i64>> (`const fn`), module demo.composites, line 270
::hglrt::list_delta<::hglrt::list<::hgraph::Int>> listed()
{
    return ::hglrt::list_delta_of<::hglrt::list<::hgraph::Int>>(::hglrt::dict_of<::hgraph::Int, ::hgraph::Int>({{0, 4}}), ::hglrt::set<::hgraph::Int>{1}, ::std::nullopt);
}

// nudged() -> delta<Point> (`const fn`), module demo.composites, line 272
::hgl::demo::composites::m_::d_::Point nudged()
{
    return ::hgl::demo::composites::m_::d_::Point{1.0, ::std::nullopt};
}

// listed_in_order() -> delta<list<i64>> (`const fn`), module demo.composites, line 276
::hglrt::list_delta<::hglrt::list<::hgraph::Int>> listed_in_order()
{
    [[maybe_unused]] ::hgraph::Int at = 0;
    return [&]() -> ::hglrt::list_delta<::hglrt::list<::hgraph::Int>> { const ::hglrt::set<::hgraph::Int> field_00_ = ::hglrt::set<::hgraph::Int>{at}; const ::hglrt::dict<::hgraph::Int, ::hgraph::Int> field_01_ = ::hglrt::dict_of<::hgraph::Int, ::hgraph::Int>({{[&]() -> ::hgraph::Int {
    at = 2;
    return at;
}(), 9}}); const ::std::optional<::hgraph::Int> field_02_ = ::std::nullopt; return ::hglrt::list_delta_of<::hglrt::list<::hgraph::Int>>(field_01_, field_00_, field_02_); }();
}

// pair_change() -> tuple<f64?, i64?> (`const fn`), module demo.composites, line 284
::std::tuple<::std::optional<::hgraph::Float>, ::std::optional<::hgraph::Int>> pair_change()
{
    return ::std::tuple<::std::optional<::hgraph::Float>, ::std::optional<::hgraph::Int>>{1.0, ::std::nullopt};
}

// side_of(number: i64) -> Side (`const fn`), module demo.composites, line 287
::hgl::demo::composites::m_::Side side_of([[maybe_unused]] ::hgraph::Int number)
{
    return ::hglrt::enum_member<::hgl::demo::composites::m_::Side, ::hgraph::Int>(number, {{0, ::hgl::demo::composites::m_::Side::buy}, {5, ::hgl::demo::composites::m_::Side::sell}});
}

// side_named(name: str) -> Side (`const fn`), module demo.composites, line 289
::hgl::demo::composites::m_::Side side_named([[maybe_unused]] const ::hgraph::Str& name)
{
    return ::hglrt::enum_member<::hgl::demo::composites::m_::Side, ::hgraph::Str>(name, {{"buy", ::hgl::demo::composites::m_::Side::buy}, {"sell", ::hgl::demo::composites::m_::Side::sell}});
}

// london() -> timezone (`const fn`), module demo.composites, line 292
::hgraph::ZoneId london()
{
    return ::hglrt::zone_literal("Europe/London");
}

// opening() -> zoned_time (`const fn`), module demo.composites, line 294
::hgraph::ZonedTime opening()
{
    return ::hglrt::zoned_time_literal(::hgraph::time_of_day(9, 30, 0, 0), "Europe/London");
}

// is_one(value: i64?) -> bool (`const fn`), module demo.composites, line 297
::hgraph::Bool is_one([[maybe_unused]] ::std::optional<::hgraph::Int> value)
{
    return (value == 1);
}

// bump(count: i64, by: i64) (`const fn`), module demo.composites, line 301
void bump([[maybe_unused]] ::hgraph::Int& count, [[maybe_unused]] ::hgraph::Int by)
{
    count = by;
}

// bump_after_growth() -> list<i64> (`const fn`), module demo.composites, line 305
::hglrt::list<::hgraph::Int> bump_after_growth()
{
    [[maybe_unused]] ::hglrt::list<::hgraph::Int> items = ::hglrt::list<::hgraph::Int>{10};
    [&] { const ::hgraph::Int index_00_ = 0; const ::hgraph::Int argument_01_ = [&]() -> ::hgraph::Int {
    items = ::hglrt::list<::hgraph::Int>{20, 30};
    return 1;
}(); return ::hgl::demo::composites::m_::bump(::hglrt::item(items, index_00_), argument_01_); }();
    return items;
}

// as_shapes(rounds: list<Round>) -> list<Shape> (`const fn`), module demo.composites, line 315
::hglrt::list<::hgl::demo::composites::m_::Shape> as_shapes([[maybe_unused]] const ::hglrt::list<::hgl::demo::composites::m_::Round>& rounds)
{
    return ::hglrt::parent_list<::hgl::demo::composites::m_::Shape>(rounds);
}

// failing() -> i64 (`const fn`), module demo.composites, line 414
::hgraph::Int failing()
{
    return ::hglrt::item(::hglrt::list<::hgraph::Int>{1}, 5);
}

}  // namespace hgl::demo::composites::m_

namespace hgl::demo::composites::m_::t_
{

void structs_are_values()
{
    ::hglrt::testing::statement();
    [[maybe_unused]] const ::hgl::demo::composites::m_::Point point = ::hgl::demo::composites::m_::Point{1.0, 2.0};
    ::hglrt::testing::statement();
    ::hglrt::testing::check((::hglrt::eq__f64_f64_f64_f64((point).x, 1.0, 0.0, 0.0) && ::hglrt::eq__f64_f64_f64_f64((point).y, 2.0, 0.0, 0.0)));
    ::hglrt::testing::statement();
    ::hglrt::testing::check([&] { const ::hgl::demo::composites::m_::Point argument_00_ = ::hgl::demo::composites::m_::origin(); const ::hgl::demo::composites::m_::Point argument_01_ = ::hgl::demo::composites::m_::Point{0.0, 0.0}; return ::hglrt::eq__T_T<::hgl::demo::composites::m_::Point>(argument_00_, argument_01_); }());
    ::hglrt::testing::statement();
    ::hglrt::testing::check([&] { const ::hgl::demo::composites::m_::Point argument_02_ = ::hgl::demo::composites::m_::moved(point, 1.5); const ::hgl::demo::composites::m_::Point argument_03_ = ::hgl::demo::composites::m_::Point{2.5, 3.5}; return ::hglrt::eq__T_T<::hgl::demo::composites::m_::Point>(argument_02_, argument_03_); }());
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__T_T<::hgl::demo::composites::m_::Point>(point, ::hgl::demo::composites::m_::Point{1.0, 2.0}));
    ::hglrt::testing::statement();
    ::hglrt::testing::check([&] { const ::hgl::demo::composites::m_::Point argument_04_ = ::hgl::demo::composites::m_::c_::add_1_(point, ::hgl::demo::composites::m_::Point{3.0, 0.0}); const ::hgl::demo::composites::m_::Point argument_05_ = ::hgl::demo::composites::m_::Point{4.0, 2.0}; return ::hglrt::eq__T_T<::hgl::demo::composites::m_::Point>(argument_04_, argument_05_); }());
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__f64_f64_f64_f64(::hgl::demo::composites::m_::first_x(), 1.0, 0.0, 0.0));
    ::hglrt::testing::statement();
    ::hglrt::testing::check([&] { const ::hgl::demo::composites::m_::Empty argument_06_ = ::hgl::demo::composites::m_::Empty{}; const ::hgl::demo::composites::m_::Empty argument_07_ = ::hgl::demo::composites::m_::Empty{}; return ::hglrt::eq__T_T<::hgl::demo::composites::m_::Empty>(argument_06_, argument_07_); }());
    ::hglrt::testing::statement();
    ::hglrt::testing::check([&] { const ::hgl::demo::composites::m_::Outcome argument_08_ = ::hgl::demo::composites::m_::Outcome{::std::expected<::hgraph::Int, ::hgraph::Str>{1}}; const ::hgl::demo::composites::m_::Outcome argument_09_ = ::hgl::demo::composites::m_::Outcome{::std::expected<::hgraph::Int, ::hgraph::Str>{1}}; return ::hglrt::eq__T_T<::hgl::demo::composites::m_::Outcome>(argument_08_, argument_09_); }());
    ::hglrt::testing::statement();
    ::hglrt::testing::check([&] { const ::hgl::demo::composites::m_::Outcome argument_010_ = ::hgl::demo::composites::m_::Outcome{::std::expected<::hgraph::Int, ::hgraph::Str>{::std::unexpect, ::hgraph::Str{"no"}}}; const ::hgl::demo::composites::m_::Outcome argument_011_ = ::hgl::demo::composites::m_::Outcome{::std::expected<::hgraph::Int, ::hgraph::Str>{1}}; return ::hglrt::ne__T_T<::hgl::demo::composites::m_::Outcome>(argument_010_, argument_011_); }());
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__i64_i64_f64_f64((::hgl::demo::composites::m_::Sided<::hgl::demo::composites::m_::Side::sell>{2}).count, 2, 0.0, 0.0));
}

void fields_default_and_nest()
{
    ::hglrt::testing::statement();
    [[maybe_unused]] const ::hgl::demo::composites::m_::Line line = [&]() -> ::hgl::demo::composites::m_::Line { const ::hgl::demo::composites::m_::Point field_00_ = ::hgl::demo::composites::m_::origin(); const ::hgl::demo::composites::m_::Point field_01_ = ::hgl::demo::composites::m_::Point{1.0, 0.0}; const ::hgraph::Int field_02_ = 3; const ::std::optional<::hgraph::Str> field_03_ = ::std::nullopt; return ::hgl::demo::composites::m_::Line{field_00_, field_01_, field_02_, field_03_}; }();
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__T_T<::hgl::demo::composites::m_::Point>((line).end, ::hgl::demo::composites::m_::Point{1.0, 0.0}));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__i64_i64_f64_f64((line).class_, 3, 0.0, 0.0));
    ::hglrt::testing::statement();
    ::hglrt::testing::check((!((line).note).has_value()));
    ::hglrt::testing::statement();
    [[maybe_unused]] const ::hgl::demo::composites::m_::Line longer = ::hgl::demo::composites::m_::stretched(line, 2.0);
    ::hglrt::testing::statement();
    ::hglrt::testing::check((::hglrt::eq__f64_f64_f64_f64(((longer).end).x, 3.0, 0.0, 0.0) && ::hglrt::eq__i64_i64_f64_f64((longer).class_, 4, 0.0, 0.0)));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__f64_f64_f64_f64(((line).end).x, 1.0, 0.0, 0.0));
    ::hglrt::testing::statement();
    ::hglrt::testing::check((([&]() -> ::hgl::demo::composites::m_::Line { const ::hgl::demo::composites::m_::Point field_04_ = ::hgl::demo::composites::m_::origin(); const ::hgraph::Str field_05_ = ::hgraph::Str{"n"}; const ::hgl::demo::composites::m_::Point field_06_ = ::hgl::demo::composites::m_::Point{1.0, 0.0}; const ::hgraph::Int field_07_ = 3; return ::hgl::demo::composites::m_::Line{field_04_, field_06_, field_07_, field_05_}; }()).note).has_value());
}

void a_struct_holding_nan_is_not_equal_to_itself()
{
    ::hglrt::testing::statement();
    [[maybe_unused]] const ::hgl::demo::composites::m_::Point odd = ::hgl::demo::composites::m_::Point{::hglrt::div__f64_f64<::hglrt::ZeroPolicy::ieee>(0.0, 0.0), 0.0};
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::ne__T_T<::hgl::demo::composites::m_::Point>(odd, odd));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::ne__T_T<::hglrt::list<::hgl::demo::composites::m_::Point>>(::hglrt::list<::hgl::demo::composites::m_::Point>{odd}, ::hglrt::list<::hgl::demo::composites::m_::Point>{odd}));
}

void generic_and_abstract_structs()
{
    ::hglrt::testing::statement();
    ::hglrt::testing::check([&] { const ::hgl::demo::composites::m_::Box<::hgraph::Int> argument_00_ = ::hgl::demo::composites::m_::Box<::hgraph::Int>{3, 0}; const ::hgl::demo::composites::m_::Box<::hgraph::Int> argument_01_ = ::hgl::demo::composites::m_::Box<::hgraph::Int>{3, 0}; return ::hglrt::eq__T_T<::hgl::demo::composites::m_::Box<::hgraph::Int>>(argument_00_, argument_01_); }());
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__i64_i64_f64_f64((::hgl::demo::composites::m_::Box<::hgraph::Str>{::hgraph::Str{"a"}, 0}).count, 0, 0.0, 0.0));
    ::hglrt::testing::statement();
    [[maybe_unused]] const ::hgl::demo::composites::m_::Round round = ::hgl::demo::composites::m_::Round{1, ::hgraph::Str{"shape"}, 2.0};
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__T_T<::hgraph::Str>(::hgl::demo::composites::m_::named(round), ::hgraph::Str{"shape"}));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hgl::demo::composites::m_::same(round, ::hgl::demo::composites::m_::Round{1, ::hgraph::Str{"shape"}, 2.0}));
    ::hglrt::testing::statement();
    ::hglrt::testing::check((!::hgl::demo::composites::m_::same(round, ::hgl::demo::composites::m_::Round{2, ::hgraph::Str{"shape"}, 2.0})));
    ::hglrt::testing::statement();
    ::hglrt::testing::check((![&] { const ::hgl::demo::composites::m_::Tagged<::hgraph::Int> argument_02_ = ::hgl::demo::composites::m_::Tagged<::hgraph::Int>{1, ::hgraph::Str{"shape"}, 1}; const ::hgl::demo::composites::m_::Tagged<::hgraph::Float> argument_03_ = ::hgl::demo::composites::m_::Tagged<::hgraph::Float>{1, ::hgraph::Str{"shape"}, 1.0}; return ::hgl::demo::composites::m_::same(argument_02_, argument_03_); }()));
    ::hglrt::testing::statement();
    ::hglrt::testing::check((!::hgl::demo::composites::m_::same(round, ::hgl::demo::composites::m_::Tagged<::hgraph::Float>{1, ::hgraph::Str{"shape"}, 2.0})));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__i64_i64_f64_f64(::hgl::demo::composites::m_::base_id(::hgl::demo::composites::m_::as_base(round)), 1, 0.0, 0.0));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__i64_i64_f64_f64(::hgl::demo::composites::m_::base_id(round), 1, 0.0, 0.0));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__T_T<::hgraph::Str>((::hglrt::item(::hgl::demo::composites::m_::as_shapes(::hglrt::list<::hgl::demo::composites::m_::Round>{round}), 0)).name(), ::hgraph::Str{"shape"}));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__T_T<::std::array<::hgraph::Int, 2>>(::hgl::demo::composites::m_::held(::hgl::demo::composites::m_::Pair{::std::array<::hgraph::Int, 2>{1, 2}}), ::std::array<::hgraph::Int, 2>{1, 2}));
}

void arguments_are_evaluated_in_the_order_written()
{
    ::hglrt::testing::statement();
    ::hglrt::testing::check([&] { const ::hgl::demo::composites::m_::Point argument_00_ = ::hgl::demo::composites::m_::written_order(); const ::hgl::demo::composites::m_::Point argument_01_ = ::hgl::demo::composites::m_::Point{2.0, 2.0}; return ::hglrt::eq__T_T<::hgl::demo::composites::m_::Point>(argument_00_, argument_01_); }());
    ::hglrt::testing::statement();
    ::hglrt::testing::check([&] { const ::hgl::demo::composites::m_::Point argument_02_ = ::hgl::demo::composites::m_::read_before(); const ::hgl::demo::composites::m_::Point argument_03_ = ::hgl::demo::composites::m_::Point{1.0, 5.0}; return ::hglrt::eq__T_T<::hgl::demo::composites::m_::Point>(argument_02_, argument_03_); }());
}

void sets_have_no_order()
{
    ::hglrt::testing::statement();
    [[maybe_unused]] const ::hglrt::set<::hgraph::Int> made = ::hglrt::set<::hgraph::Int>{3, 1, 2};
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__T_T<::hglrt::set<::hgraph::Int>>(made, ::hglrt::set<::hgraph::Int>{1, 2, 3}));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::ne__T_T<::hglrt::set<::hgraph::Int>>(made, ::hglrt::set<::hgraph::Int>{1, 2}));
    ::hglrt::testing::statement();
    [[maybe_unused]] const ::hglrt::set<::hgraph::Str> empty = ::hglrt::set<::hgraph::Str>{};
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__T_T<::hglrt::set<::hgraph::Str>>(empty, ::hglrt::set<::hgraph::Str>{}));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__i64_i64_f64_f64(::hgl::demo::composites::m_::size(::hglrt::set<::hgraph::Float>{0.0, ::hglrt::neg__f64(0.0), 1.0}), 2, 0.0, 0.0));
    ::hglrt::testing::statement();
    [[maybe_unused]] const ::hglrt::set<::hgl::demo::composites::m_::Point> books = ::hglrt::set<::hgl::demo::composites::m_::Point>{::hgl::demo::composites::m_::Point{1.0, 0.0}, ::hgl::demo::composites::m_::Point{1.0, 0.0}};
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__T_T<::hglrt::set<::hgl::demo::composites::m_::Point>>(books, ::hglrt::set<::hgl::demo::composites::m_::Point>{::hgl::demo::composites::m_::Point{1.0, 0.0}}));
    ::hglrt::testing::statement();
    ::hglrt::testing::check((::hgl::demo::composites::m_::holds(::hglrt::set<::hgraph::Str>{::hgraph::Str{"a"}, ::hgraph::Str{"b"}}, ::hgraph::Str{"b"}) && (!::hgl::demo::composites::m_::holds(::hglrt::set<::hgraph::Str>{::hgraph::Str{"a"}}, ::hgraph::Str{"b"}))));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__i64_i64_f64_f64(::hgl::demo::composites::m_::tag_count(), 2, 0.0, 0.0));
}

void dictionaries_read_add_and_loop()
{
    ::hglrt::testing::statement();
    [[maybe_unused]] const ::hglrt::dict<::hgraph::Str, ::hgraph::Int> book = ::hglrt::dict_of<::hgraph::Str, ::hgraph::Int>({{::hgraph::Str{"a"}, 1}, {::hgraph::Str{"b"}, 2}});
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__i64_i64_f64_f64(::hglrt::entry_at(book, ::hgraph::Str{"a"}), 1, 0.0, 0.0));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__T_T<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>(book, ::hglrt::dict_of<::hgraph::Str, ::hgraph::Int>({{::hgraph::Str{"b"}, 2}, {::hgraph::Str{"a"}, 1}})));
    ::hglrt::testing::statement();
    ::hglrt::testing::check((!(::hglrt::result_or_null(::hglrt::get_dict_result<::hgraph::Str, ::hgraph::Int>(book, ::hgraph::Str{"c"}))).has_value()));
    ::hglrt::testing::statement();
    [[maybe_unused]] const ::std::optional<::hgraph::Int> found = ::hglrt::result_or_null(::hglrt::get_dict_result<::hgraph::Str, ::hgraph::Int>(book, ::hgraph::Str{"b"}));
    ::hglrt::testing::statement();
    ::hglrt::testing::check((found == 2));
    ::hglrt::testing::statement();
    [[maybe_unused]] const ::hglrt::dict<::hgraph::Str, ::std::optional<::hgraph::Int>> maybes = ::hglrt::dict_of<::hgraph::Str, ::std::optional<::hgraph::Int>>({{::hgraph::Str{"a"}, ::std::nullopt}, {::hgraph::Str{"b"}, 2}});
    ::hglrt::testing::statement();
    ::hglrt::testing::check(((!(::hglrt::entry_at(maybes, ::hgraph::Str{"a"})).has_value()) && (::hglrt::entry_at(maybes, ::hgraph::Str{"b"}) == 2)));
    ::hglrt::testing::statement();
    ::hglrt::testing::raises("value.missing_key", [&] {
        [[maybe_unused]] const ::std::optional<::hgraph::Int> absent = ::hglrt::entry_at(maybes, ::hgraph::Str{"c"});
    });
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__i64_i64_f64_f64(::hgl::demo::composites::m_::total(book), 3, 0.0, 0.0));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__T_T<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>(::hgl::demo::composites::m_::counted(::hglrt::list<::hgraph::Str>{::hgraph::Str{"x"}, ::hgraph::Str{"y"}, ::hgraph::Str{"x"}}), ::hglrt::dict_of<::hgraph::Str, ::hgraph::Int>({{::hgraph::Str{"x"}, 2}, {::hgraph::Str{"y"}, 1}})));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__T_T<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>(::hgl::demo::composites::m_::bumped(book, ::hgraph::Str{"b"}), ::hglrt::dict_of<::hgraph::Str, ::hgraph::Int>({{::hgraph::Str{"a"}, 1}, {::hgraph::Str{"b"}, 3}})));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__T_T<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>(::hgl::demo::composites::m_::doubled_all(::hglrt::dict_of<::hgraph::Str, ::hgraph::Int>({{::hgraph::Str{"a"}, 1}})), ::hglrt::dict_of<::hgraph::Str, ::hgraph::Int>({{::hgraph::Str{"a"}, 2}, {::hgraph::Str{"extra"}, 0}})));
    ::hglrt::testing::statement();
    [[maybe_unused]] const ::hgl::demo::composites::m_::Book shelf = ::hgl::demo::composites::m_::Book{::hgraph::Str{"o"}, ::hglrt::set<::hgraph::Str>{}, ::hglrt::dict_of<::hgraph::Str, ::hgraph::Int>({{::hgraph::Str{"a"}, 5}}), ::std::nullopt, ::hglrt::list<::hgl::demo::composites::m_::Point>{}};
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__i64_i64_f64_f64(::hgl::demo::composites::m_::price_of(shelf, ::hgraph::Str{"a"}), 5, 0.0, 0.0));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__i64_i64_f64_f64(::hgl::demo::composites::m_::first_price(::hglrt::list<::hgl::demo::composites::m_::Book>{shelf}), 5, 0.0, 0.0));
    ::hglrt::testing::statement();
    ::hglrt::testing::check([&] { const ::hglrt::dict<::hgraph::Str, ::hgl::demo::composites::m_::Point> argument_00_ = ::hgl::demo::composites::m_::moved_in(::hglrt::dict_of<::hgraph::Str, ::hgl::demo::composites::m_::Point>({{::hgraph::Str{"a"}, ::hgl::demo::composites::m_::Point{1.0, 0.0}}})); const ::hglrt::dict<::hgraph::Str, ::hgl::demo::composites::m_::Point> argument_01_ = ::hglrt::dict_of<::hgraph::Str, ::hgl::demo::composites::m_::Point>({{::hgraph::Str{"a"}, ::hgl::demo::composites::m_::Point{9.0, 0.0}}}); return ::hglrt::eq__T_T<::hglrt::dict<::hgraph::Str, ::hgl::demo::composites::m_::Point>>(argument_00_, argument_01_); }());
    ::hglrt::testing::statement();
    [[maybe_unused]] const ::hglrt::dict<::hgraph::Str, ::hgraph::Float> odd = ::hglrt::dict_of<::hgraph::Str, ::hgraph::Float>({{::hgraph::Str{"n"}, ::hglrt::div__f64_f64<::hglrt::ZeroPolicy::ieee>(0.0, 0.0)}});
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::ne__T_T<::hglrt::dict<::hgraph::Str, ::hgraph::Float>>(odd, odd));
}

void a_missing_key_fails()
{
    ::hglrt::testing::statement();
    [[maybe_unused]] const ::hglrt::dict<::hgraph::Str, ::hgraph::Int> book = ::hglrt::dict_of<::hgraph::Str, ::hgraph::Int>({{::hgraph::Str{"a"}, 1}});
    ::hglrt::testing::statement();
    ::hglrt::testing::raises("value.missing_key", [&] {
        [[maybe_unused]] const ::hgraph::Int none = ::hglrt::entry_at(book, ::hgraph::Str{"b"});
    });
    ::hglrt::testing::statement();
    ::hglrt::testing::raises("value.missing_key", [&] {
        [[maybe_unused]] const ::hglrt::dict<::hgraph::Str, ::hgraph::Int> more = ::hgl::demo::composites::m_::bumped(book, ::hgraph::Str{"b"});
    });
}

void a_failing_value_adds_no_key()
{
    ::hglrt::testing::statement();
    [[maybe_unused]] ::hglrt::dict<::hgraph::Str, ::hgraph::Int> book = ::hglrt::dict_of<::hgraph::Str, ::hgraph::Int>({{::hgraph::Str{"a"}, 1}});
    ::hglrt::testing::statement();
    ::hglrt::testing::raises("value.index_out_of_range", [&] {
        static_cast<void>(::hglrt::entry(book, ::hgraph::Str{"b"}));
        ::hglrt::entry(book, ::hgraph::Str{"b"}) = ::hgl::demo::composites::m_::failing();
    });
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__T_T<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>(book, ::hglrt::dict_of<::hgraph::Str, ::hgraph::Int>({{::hgraph::Str{"a"}, 1}})));
    ::hglrt::testing::statement();
    [[maybe_unused]] ::hglrt::list<::hglrt::dict<::hgraph::Str, ::hgraph::Int>> rows = ::hglrt::list<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>{::hglrt::dict_of<::hgraph::Str, ::hgraph::Int>({{::hgraph::Str{"a"}, 1}})};
    ::hglrt::testing::statement();
    ::hglrt::testing::raises("value.index_out_of_range", [&] {
        static_cast<void>(::hglrt::entry(::hglrt::item(rows, 0), ::hgraph::Str{"b"}));
        ::hglrt::entry(::hglrt::item(rows, 0), ::hgraph::Str{"b"}) = [&]() -> ::hgraph::Int {
    [[maybe_unused]] const ::hgraph::Int none = 0;
    return ::hgl::demo::composites::m_::failing();
}();
    });
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__T_T<::hglrt::list<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>>(rows, ::hglrt::list<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>{::hglrt::dict_of<::hgraph::Str, ::hgraph::Int>({{::hgraph::Str{"a"}, 1}})}));
}

void a_dictionarys_place_comes_before_its_key()
{
    ::hglrt::testing::statement();
    [[maybe_unused]] ::hglrt::list<::hglrt::dict<::hgraph::Str, ::hgraph::Int>> rows = ::hglrt::list<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>{::hglrt::dict_of<::hgraph::Str, ::hgraph::Int>({{::hgraph::Str{"a"}, 1}})};
    ::hglrt::testing::statement();
    [[maybe_unused]] ::hgraph::Int seen = 0;
    ::hglrt::testing::statement();
    ::hglrt::testing::raises("value.index_out_of_range", [&] {
        static_cast<void>(::hglrt::item(rows, 5));
        const ::hgraph::Str key_00_ = [&]() -> ::hgraph::Str {
            seen = 1;
            return ::hgraph::Str{"b"};
        }();
        static_cast<void>(::hglrt::entry(::hglrt::item(rows, 5), key_00_));
        ::hglrt::entry(::hglrt::item(rows, 5), key_00_) = 1;
    });
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__i64_i64_f64_f64(seen, 0, 0.0, 0.0));
    ::hglrt::testing::statement();
    [[maybe_unused]] const ::hglrt::dict<::hgraph::Str, ::hgraph::Str> names = ::hglrt::dict_of<::hgraph::Str, ::hgraph::Str>({{::hgraph::Str{"a"}, ::hgraph::Str{"b"}}});
    ::hglrt::testing::statement();
    ::hglrt::testing::raises("value.index_out_of_range", [&] {
        static_cast<void>(::hglrt::item(rows, 5));
        const ::hgraph::Str key_01_ = ::hglrt::entry_at(names, ::hgraph::Str{"missing"});
        static_cast<void>(::hglrt::entry(::hglrt::item(rows, 5), key_01_));
        ::hglrt::entry(::hglrt::item(rows, 5), key_01_) = 1;
    });
    ::hglrt::testing::statement();
    static_cast<void>(::hglrt::item(rows, 0));
    const ::hgraph::Str key_02_ = ::hglrt::entry_at(names, ::hgraph::Str{"a"});
    static_cast<void>(::hglrt::entry(::hglrt::item(rows, 0), key_02_));
    ::hglrt::entry(::hglrt::item(rows, 0), key_02_) = 2;
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__T_T<::hglrt::list<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>>(rows, ::hglrt::list<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>{::hglrt::dict_of<::hgraph::Str, ::hgraph::Int>({{::hgraph::Str{"a"}, 1}, {::hgraph::Str{"b"}, 2}})}));
}

void deltas_are_values()
{
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__T_T<::hglrt::set<::hgraph::Str>>(::hgl::demo::composites::m_::arrived(::hgl::demo::composites::m_::joined(::hgraph::Str{"a"})), ::hglrt::set<::hgraph::Str>{::hgraph::Str{"a"}}));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__T_T<::hglrt::set<::hgraph::Str>>((::hgl::demo::composites::m_::joined(::hgraph::Str{"a"})).removed, ::hglrt::set<::hgraph::Str>{}));
    ::hglrt::testing::statement();
    ::hglrt::testing::check((!((::hgl::demo::composites::m_::cleared()).value).has_value()));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(((::hgl::demo::composites::m_::seven()).value == 7));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__T_T<::hglrt::dict<::hgraph::Str, ::hgraph::Int>>((::hgl::demo::composites::m_::booked()).updated, ::hglrt::dict_of<::hgraph::Str, ::hgraph::Int>({{::hgraph::Str{"a"}, 1}})));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__T_T<::hglrt::set<::hgraph::Str>>((::hgl::demo::composites::m_::booked()).removed, ::hglrt::set<::hgraph::Str>{::hgraph::Str{"b"}}));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__T_T<::hglrt::dict<::hgraph::Int, ::hgraph::Int>>(::hglrt::updated_of(::hgl::demo::composites::m_::listed()), ::hglrt::dict_of<::hgraph::Int, ::hgraph::Int>({{0, 4}})));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__T_T<::hglrt::set<::hgraph::Int>>(::hglrt::removed_of(::hgl::demo::composites::m_::listed()), ::hglrt::set<::hgraph::Int>{1}));
    ::hglrt::testing::statement();
    ::hglrt::testing::check((!((::hgl::demo::composites::m_::listed()).size_modified).has_value()));
    ::hglrt::testing::statement();
    ::hglrt::testing::check((::hglrt::eq__T_T<::hglrt::set<::hgraph::Int>>(::hglrt::removed_of(::hgl::demo::composites::m_::listed_in_order()), ::hglrt::set<::hgraph::Int>{0}) && ::hglrt::eq__T_T<::hglrt::dict<::hgraph::Int, ::hgraph::Int>>(::hglrt::updated_of(::hgl::demo::composites::m_::listed_in_order()), ::hglrt::dict_of<::hgraph::Int, ::hgraph::Int>({{2, 9}}))));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(((::hgl::demo::composites::m_::nudged()).x == 1.0));
    ::hglrt::testing::statement();
    ::hglrt::testing::check((!((::hgl::demo::composites::m_::nudged()).y).has_value()));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__T_T<::std::tuple<::std::optional<::hgraph::Float>, ::std::optional<::hgraph::Int>>>(::hgl::demo::composites::m_::pair_change(), ::std::tuple<::std::optional<::hgraph::Float>, ::std::optional<::hgraph::Int>>{1.0, ::std::nullopt}));
}

void enums_zones_and_nullables()
{
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__T_T<::hgl::demo::composites::m_::Side>(::hgl::demo::composites::m_::side_of(5), ::hgl::demo::composites::m_::Side::sell));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__T_T<::hgl::demo::composites::m_::Side>(::hgl::demo::composites::m_::side_named(::hgraph::Str{"buy"}), ::hgl::demo::composites::m_::Side::buy));
    ::hglrt::testing::statement();
    ::hglrt::testing::check([&] { const ::hgraph::ZoneId argument_00_ = ::hgl::demo::composites::m_::london(); const ::hgraph::ZoneId argument_01_ = ::hglrt::zone_literal("Europe/London"); return ::hglrt::eq__T_T<::hgraph::ZoneId>(argument_00_, argument_01_); }());
    ::hglrt::testing::statement();
    ::hglrt::testing::check([&] { const ::hgraph::ZonedTime argument_02_ = ::hgl::demo::composites::m_::opening(); const ::hgraph::ZonedTime argument_03_ = ::hglrt::zoned_time_literal(::hgraph::time_of_day(9, 30, 0, 0), "Europe/London"); return ::hglrt::eq__T_T<::hgraph::ZonedTime>(argument_02_, argument_03_); }());
    ::hglrt::testing::statement();
    ::hglrt::testing::check([&] { const ::hgraph::ZonedTime argument_04_ = ::hgl::demo::composites::m_::opening(); const ::hgraph::ZonedTime argument_05_ = ::hglrt::zoned_time_literal(::hgraph::time_of_day(9, 30, 0, 0), "UTC"); return ::hglrt::ne__T_T<::hgraph::ZonedTime>(argument_04_, argument_05_); }());
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hgl::demo::composites::m_::is_one(1));
    ::hglrt::testing::statement();
    [[maybe_unused]] ::hgl::demo::composites::m_::Noted noted = ::hgl::demo::composites::m_::Memo{::hgraph::Str{"a"}};
    ::hglrt::testing::statement();
    ::hglrt::testing::check([&] { const ::hgraph::Str left_06_ = [&]() -> ::hgraph::Str {
    noted = ::hgl::demo::composites::m_::Memo{::hgraph::Str{"b"}};
    return ::hgraph::Str{"b"};
}(); const ::std::optional<::hgraph::Str> right_07_ = (noted).note(); return left_06_ == right_07_; }());
    ::hglrt::testing::statement();
    ::hglrt::testing::check((!::hgl::demo::composites::m_::is_one(2)));
    ::hglrt::testing::statement();
    ::hglrt::testing::check((!::hgl::demo::composites::m_::is_one(::std::nullopt)));
}

void bits_are_held_as_they_were_read()
{
    ::hglrt::testing::statement();
    [[maybe_unused]] ::hglrt::list<::hgraph::Bool> on = ::hglrt::list<::hgraph::Bool>{true, false};
    ::hglrt::testing::statement();
    [[maybe_unused]] const ::hgl::demo::composites::m_::Flags made = [&]() -> ::hgl::demo::composites::m_::Flags { const ::hgraph::Bool field_00_ = ::hglrt::item(on, 1); const ::hglrt::list<::hgraph::Bool> field_01_ = [&]() -> ::hglrt::list<::hgraph::Bool> {
    ::hglrt::item(on, 1) = true;
    return on;
}(); return ::hgl::demo::composites::m_::Flags{field_01_, field_00_}; }();
    ::hglrt::testing::statement();
    ::hglrt::testing::check(((!(made).first) && ::hglrt::eq__T_T<::hglrt::list<::hgraph::Bool>>((made).on, ::hglrt::list<::hgraph::Bool>{true, true})));
    ::hglrt::testing::statement();
    [[maybe_unused]] const ::hglrt::set<::hgraph::Bool> members = ::hglrt::set<::hgraph::Bool>{true, false, true};
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__T_T<::hglrt::set<::hgraph::Bool>>(members, ::hglrt::set<::hgraph::Bool>{false, true}));
}

void a_var_place_is_taken_where_the_call_is_made()
{
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__T_T<::hglrt::list<::hgraph::Int>>(::hgl::demo::composites::m_::bump_after_growth(), ::hglrt::list<::hgraph::Int>{1, 30}));
    ::hglrt::testing::statement();
    [[maybe_unused]] ::hglrt::list<::hglrt::list<::hgraph::Int>> rows = ::hglrt::list<::hglrt::list<::hgraph::Int>>{::hglrt::list<::hgraph::Int>{1}};
    ::hglrt::testing::statement();
    [[maybe_unused]] ::hgraph::Int seen = 0;
    ::hglrt::testing::statement();
    ::hglrt::testing::raises("value.index_out_of_range", [&] {
        [&] { const ::hgraph::Int index_00_ = 5; static_cast<void>(::hglrt::item(rows, index_00_)); const ::hgraph::Int index_01_ = [&]() -> ::hgraph::Int {
    seen = 1;
    return 0;
}(); const ::hgraph::Int argument_02_ = 1; return ::hgl::demo::composites::m_::bump(::hglrt::item(::hglrt::item(rows, index_00_), index_01_), argument_02_); }();
    });
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__i64_i64_f64_f64(seen, 0, 0.0, 0.0));
}

void a_nullable_delta_is_no_nullable()
{
    ::hglrt::testing::statement();
    [[maybe_unused]] const ::hglrt::dict<::hgraph::Str, ::hglrt::nullable_delta<::hgraph::Int, ::hgraph::Int>> book = ::hglrt::dict_of<::hgraph::Str, ::hglrt::nullable_delta<::hgraph::Int, ::hgraph::Int>>({{::hgraph::Str{"a"}, ::hglrt::nullable_delta<::hgraph::Int, ::hgraph::Int>{}}});
    ::hglrt::testing::statement();
    ::hglrt::testing::check((::hglrt::result_or_null(::hglrt::get_dict_result<::hgraph::Str, ::hglrt::nullable_delta<::hgraph::Int, ::hgraph::Int>>(book, ::hgraph::Str{"a"}))).has_value());
    ::hglrt::testing::statement();
    ::hglrt::testing::check([&] { const ::std::optional<::hglrt::nullable_delta<::hgraph::Int, ::hgraph::Int>> left_00_ = ::hgl::demo::composites::m_::look(book, ::hgraph::Str{"a"}); const ::hglrt::nullable_delta<::hgraph::Int, ::hgraph::Int> right_01_ = ::hglrt::nullable_delta<::hgraph::Int, ::hgraph::Int>{}; return left_00_ == right_01_; }());
    ::hglrt::testing::statement();
    ::hglrt::testing::check((!(::hgl::demo::composites::m_::look(book, ::hgraph::Str{"missing"})).has_value()));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(((::hgl::demo::composites::m_::published_null()).level).has_value());
    ::hglrt::testing::statement();
    ::hglrt::testing::check((!((::hgl::demo::composites::m_::unchanged()).level).has_value()));
    ::hglrt::testing::statement();
    ::hglrt::testing::check([&] { const ::std::optional<::hglrt::nullable_delta<::hgraph::Int, ::hgraph::Int>> left_02_ = (::hgl::demo::composites::m_::published_null()).level; const ::hglrt::nullable_delta<::hgraph::Int, ::hgraph::Int> right_03_ = ::hglrt::nullable_delta<::hgraph::Int, ::hgraph::Int>{}; return left_02_ == right_03_; }());
    ::hglrt::testing::statement();
    ::hglrt::testing::check([&] { const ::std::optional<::hglrt::nullable_delta<::hgraph::Int, ::hgraph::Int>> left_04_ = (::hgl::demo::composites::m_::unchanged()).level; const ::hglrt::nullable_delta<::hgraph::Int, ::hgraph::Int> right_05_ = ::hglrt::nullable_delta<::hgraph::Int, ::hgraph::Int>{}; return left_04_ != right_05_; }());
    ::hglrt::testing::statement();
    ::hglrt::testing::check([&] { const ::std::optional<::hglrt::nullable_delta<::hgraph::Int, ::hgraph::Int>> left_06_ = (::hgl::demo::composites::m_::d_::Reading{::hglrt::nullable_delta<::hgraph::Int, ::hgraph::Int>{2}}).level; const ::hglrt::nullable_delta<::hgraph::Int, ::hgraph::Int> right_07_ = ::hglrt::nullable_delta<::hgraph::Int, ::hgraph::Int>{2}; return left_06_ == right_07_; }());
    ::hglrt::testing::statement();
    [[maybe_unused]] ::hgraph::Int reads = 0;
    ::hglrt::testing::statement();
    ::hglrt::testing::check([&] { const ::std::optional<::hglrt::nullable_delta<::hgraph::Int, ::hgraph::Int>> left_08_ = [&]() -> ::std::optional<::hglrt::nullable_delta<::hgraph::Int, ::hgraph::Int>> {
    reads = ::hglrt::add__i64_i64(reads, 1);
    return (::hgl::demo::composites::m_::published_null()).level;
}(); const ::hglrt::nullable_delta<::hgraph::Int, ::hgraph::Int> right_09_ = ::hglrt::nullable_delta<::hgraph::Int, ::hgraph::Int>{}; return left_08_ == right_09_; }());
    ::hglrt::testing::statement();
    ::hglrt::testing::check([&] { const ::std::optional<::hglrt::nullable_delta<::hgraph::Int, ::hgraph::Int>> left_010_ = (::hgl::demo::composites::m_::unchanged()).level; const ::hglrt::nullable_delta<::hgraph::Int, ::hgraph::Int> right_011_ = [&]() -> ::hglrt::nullable_delta<::hgraph::Int, ::hgraph::Int> {
    reads = ::hglrt::add__i64_i64(reads, 1);
    return ::hglrt::nullable_delta<::hgraph::Int, ::hgraph::Int>{};
}(); return left_010_ != right_011_; }());
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__i64_i64_f64_f64(reads, 2, 0.0, 0.0));
}

void an_empty_default_of_an_optional_collection_is_the_collection()
{
    ::hglrt::testing::statement();
    ::hglrt::testing::check(((((::hgl::demo::composites::m_::Stocked{::hglrt::list<::hgraph::Int>{}, ::hglrt::set<::hgraph::Str>{}, ::hglrt::dict<::hgraph::Str, ::hgraph::Int>{}, ::hglrt::list<::hgraph::Int>{}}).items).has_value() && ((::hgl::demo::composites::m_::Stocked{::hglrt::list<::hgraph::Int>{}, ::hglrt::set<::hgraph::Str>{}, ::hglrt::dict<::hgraph::Str, ::hgraph::Int>{}, ::hglrt::list<::hgraph::Int>{}}).tags).has_value()) && ((::hgl::demo::composites::m_::Stocked{::hglrt::list<::hgraph::Int>{}, ::hglrt::set<::hgraph::Str>{}, ::hglrt::dict<::hgraph::Str, ::hgraph::Int>{}, ::hglrt::list<::hgraph::Int>{}}).book).has_value()));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(((::hgl::demo::composites::m_::Stocked{::hglrt::list<::hgraph::Int>{}, ::hglrt::set<::hgraph::Str>{}, ::hglrt::dict<::hgraph::Str, ::hgraph::Int>{}, ::hglrt::list<::hgraph::Int>{}}).kept).has_value());
    ::hglrt::testing::statement();
    ::hglrt::testing::check(((::hgl::demo::composites::m_::Shelf<::hgraph::Int>{::hglrt::list<::hgraph::Int>{}}).items).has_value());
    ::hglrt::testing::statement();
    ::hglrt::testing::check(((::hgl::demo::composites::m_::Filled{::hglrt::list<::hgraph::Int>{}}).held).has_value());
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__i64_i64_f64_f64((::hgl::demo::composites::m_::std{2}).x, 2, 0.0, 0.0));
}

}  // namespace hgl::demo::composites::m_::t_
