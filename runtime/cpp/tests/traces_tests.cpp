// Unit tests of traces (traces.h) of nullables, tuples and lists of scalars:
// their cells written whole and as deltas, their comparison with an eval's
// recording, where two first differ, what one written out replays and gives
// a `const fn`, and the text of a value.
#include <hglrt/testing.h>

#include <doctest/doctest.h>

#include <array>
#include <chrono>
#include <limits>
#include <optional>
#include <stdexcept>
#include <string>
#include <tuple>

namespace {

using hglrt::datetime;
using hglrt::i64;
using hglrt::list;
using hglrt::list_delta;
using hglrt::testing::Dense;
using hglrt::testing::Recorded;
using hglrt::testing::Recording;
using hglrt::testing::Timed;
using hglrt::testing::Whole;
using namespace std::chrono_literals;

const datetime start = hglrt::testing::earliest_start();

using Maybe = std::optional<i64>;
using Pair = std::tuple<i64, i64>;
using PairDelta = hglrt::delta_t<Pair>;
using Values = list<i64>;
using ValuesDelta = hglrt::delta_t<Values>;
using Flags = std::array<bool, 2>;
using FlagsDelta = hglrt::delta_t<Flags>;

/// What `difference` says of two traces, or `(the same)`.
template <class Left, class Right>
std::string said(const Left& left, const Right& right) {
    return hglrt::testing::difference(left, right).value_or("(the same)");
}

} // namespace

TEST_CASE("a delta is T for a scalar and a nullable, and a value of its own otherwise (TYPE-16)") {
    static_assert(std::is_same_v<hglrt::delta_t<i64>, i64>);
    static_assert(std::is_same_v<hglrt::delta_t<Maybe>, Maybe>);
    static_assert(std::is_same_v<PairDelta, std::tuple<Maybe, Maybe>>);
    static_assert(
        std::is_same_v<hglrt::delta_t<std::tuple<Maybe, i64>>, std::tuple<std::optional<Maybe>, Maybe>>);
    static_assert(std::is_same_v<ValuesDelta, list_delta<Values>>);
    static_assert(!std::is_same_v<FlagsDelta, hglrt::delta_t<list<bool>>>);
    static_assert(hglrt::testing::publishes_whole<Maybe>);
    static_assert(!hglrt::testing::publishes_whole<Pair>);
    CHECK(hglrt::type_name<ValuesDelta>() == "delta<list<i64>>");
    CHECK(hglrt::type_name<FlagsDelta>() == "delta<list<bool, 2>>");
    CHECK(hglrt::type_name<PairDelta>() == "tuple<i64?, i64?>");
    // Two list deltas are equal where they name the same changes, and hash
    // alike then.
    const ValuesDelta one{.updated = {{0, 1}, {1, 2}}, .removed = {3}, .size_modified = 2};
    const ValuesDelta again{.updated = {{1, 2}, {0, 1}}, .removed = {3}, .size_modified = 2};
    CHECK(one == again);
    CHECK(hglrt::hash_of(one) == hglrt::hash_of(again));
    CHECK_FALSE(one == ValuesDelta{.updated = {{0, 1}, {1, 2}}, .removed = {3}});
    // NUM-5 holds inside a value: a delta that holds a NaN is not equal to
    // itself.
    const list_delta<list<double>> odd{.updated = {{0, std::numeric_limits<double>::quiet_NaN()}}};
    CHECK_FALSE(hglrt::eq__T_T(odd, odd));
}

TEST_CASE("a trace of a nullable has published nulls apart from no publication (TST-5, TST-16)") {
    // core's case a_null_publication_has_a_tuple_in_the_recording:
    // `[_, null, 7, _, null]`.
    const Recording<Maybe> recorded{start,
                                    {{start + 1us, Maybe{}}, {start + 2us, 7}, {start + 4us, Maybe{}}}};
    const Dense<Maybe> written{std::nullopt, Maybe{}, Maybe{7}, std::nullopt, Maybe{}};
    CHECK(hglrt::testing::same(recorded, written));
    CHECK(hglrt::testing::same(written, Timed<Maybe>{{{1us, Maybe{}}, {2us, 7}, {4us, Maybe{}}}}));
    CHECK_FALSE(hglrt::testing::same(recorded, Dense<Maybe>{std::nullopt, std::nullopt, Maybe{7}}));
    CHECK(said(recorded, Dense<Maybe>{std::nullopt, Maybe{}, Maybe{7}, std::nullopt, Maybe{8}}) ==
          "the traces differ at 4us from the start: the eval has null, and the trace written out has 8");
    // What a `const fn` is given, and a replay: a tuple for each publication.
    CHECK(hglrt::testing::publications(recorded) == recorded.entries);
    CHECK(hglrt::testing::replay_items(written, start) == recorded.entries);
    CHECK(hglrt::testing::publications(written) == recorded.entries);
}

TEST_CASE("a tuple's cells are its delta or a whole value (TST-5, TYPE-16)") {
    // core's set_one_part: position 0 on the second cycle, 1 on the fourth.
    const Recording<Pair> recorded{
        start,
        {{start + 1us, PairDelta{3, std::nullopt}}, {start + 3us, PairDelta{std::nullopt, 8}}},
        hglrt::testing::Held<Pair>{{start + 1us, std::nullopt}, {start + 3us, Pair{3, 8}}}};
    // Deltas are compared with the deltas published.
    CHECK(hglrt::testing::same(recorded, Dense<Pair>{std::nullopt, PairDelta{3, std::nullopt}, std::nullopt,
                                                     PairDelta{std::nullopt, 8}}));
    // A whole value with the value the result held: none at the second
    // cycle, where a part was not valid, and (3, 8) at the fourth.
    CHECK(hglrt::testing::same(
        recorded, Dense<Pair>{std::nullopt, PairDelta{3, std::nullopt}, std::nullopt, Whole<Pair>{{3, 8}}}));
    CHECK_FALSE(hglrt::testing::same(
        recorded, Dense<Pair>{std::nullopt, Whole<Pair>{{3, 0}}, std::nullopt, Whole<Pair>{{3, 8}}}));
    CHECK(said(recorded,
               Dense<Pair>{std::nullopt, PairDelta{3, std::nullopt}, std::nullopt, Whole<Pair>{{3, 9}}}) ==
          "the traces differ at 3us from the start: the eval has (null, 8), holding (3, 8), and the trace "
          "written out has (3, 9)");
    // A whole cell meets a recording that kept no value its result held: the
    // generated entry lacks what it needs, which is no HGL failure.
    const Recording<Pair> bare{start, recorded.entries};
    CHECK_THROWS_AS((void)hglrt::testing::same(bare, Dense<Pair>{std::nullopt, Whole<Pair>{{3, 0}}}),
                    std::logic_error);
    // Two cells written out are the same where both are whole or both
    // deltas, and equal.
    CHECK(hglrt::testing::same(Dense<Pair>{Whole<Pair>{{1, 2}}}, Timed<Pair>{{{0us, Whole<Pair>{{1, 2}}}}}));
    CHECK_FALSE(hglrt::testing::same(Dense<Pair>{Whole<Pair>{{1, 2}}}, Dense<Pair>{PairDelta{1, 2}}));
    // A whole cell replays as each of its positions.
    CHECK(hglrt::testing::replay_items(Dense<Pair>{Whole<Pair>{{1, 2}}, PairDelta{std::nullopt, 3}}, start) ==
          Recorded<PairDelta>{{start, PairDelta{1, 2}}, {start + 1us, PairDelta{std::nullopt, 3}}});
    // A nullable position written null in a whole value publishes a null.
    using Mixed = std::tuple<Maybe, i64>;
    CHECK(std::get<1>(hglrt::testing::replay_items(Dense<Mixed>{Whole<Mixed>{{Maybe{}, 4}}}, start)[0]) ==
          hglrt::delta_t<Mixed>{Maybe{}, 4});
}

TEST_CASE("a list's cells are its delta or a whole value, which replaces what it held (TST-5, TST-9)") {
    const ValuesDelta grown{.updated = {{0, 1}, {1, 2}}, .size_modified = 2};
    const ValuesDelta changed{.updated = {{1, 5}}};
    const ValuesDelta shrunk{.removed = {1}, .size_modified = 1};
    const Recording<Values> recorded{start,
                                     {{start, grown}, {start + 1us, changed}, {start + 2us, shrunk}},
                                     hglrt::testing::Held<Values>{{start, Values{1, 2}},
                                                                  {start + 1us, Values{1, 5}},
                                                                  {start + 2us, Values{1}}}};
    CHECK(hglrt::testing::same(recorded,
                               Dense<Values>{Whole<Values>{{1, 2}}, changed, Whole<Values>{Values{1}}}));
    CHECK(hglrt::testing::same(recorded, Dense<Values>{grown, Whole<Values>{{1, 5}}, shrunk}));
    CHECK(said(recorded, Dense<Values>{grown, changed, Whole<Values>{{1, 2}}}) ==
          "the traces differ at 2us from the start: the eval has delta<list<i64>>(updated: [], removed: [1], "
          "size_modified: 1), holding [1], and the trace written out has [1, 2]");
    // Replayed, a whole list sets each position, removes the tail the input
    // held past it, and gives the new size where it moves; a delta's size
    // is followed too.
    const Recorded<ValuesDelta> items = hglrt::testing::replay_items(
        Dense<Values>{Whole<Values>{{1, 2, 3}}, Whole<Values>{{4}}, ValuesDelta{.updated = {{2, 9}}},
                      Whole<Values>{{}}, Whole<Values>{{}}},
        start);
    REQUIRE(items.size() == 5);
    CHECK(std::get<1>(items[0]) == ValuesDelta{.updated = {{0, 1}, {1, 2}, {2, 3}}, .size_modified = 3});
    CHECK(std::get<1>(items[1]) == ValuesDelta{.updated = {{0, 4}}, .removed = {1, 2}, .size_modified = 1});
    CHECK(std::get<1>(items[2]) == ValuesDelta{.updated = {{2, 9}}});
    CHECK(std::get<1>(items[3]) == ValuesDelta{.removed = {0, 1, 2}, .size_modified = 0});
    CHECK(std::get<1>(items[4]) == ValuesDelta{});
    // A negative size taken from the size, never below none.
    CHECK(hglrt::testing::detail::size_after(3, ValuesDelta{.size_modified = -1}) == 2);
    CHECK(hglrt::testing::detail::size_after(3, ValuesDelta{.size_modified = -5}) == 0);
    CHECK(hglrt::testing::detail::size_after(2, ValuesDelta{.updated = {{4, 1}}}) == 5);
    CHECK(hglrt::testing::detail::size_after(
              2, ValuesDelta{.size_modified = std::numeric_limits<i64>::min()}) == 0);
}

TEST_CASE("a fixed list's whole cells name each position (TST-5, TYPE-16)") {
    // core's a_window_is_valid_before_its_minimum_and_all_valid_at_it:
    // `[[false, false], [true, false], [true, false], [true, true]]`.
    const auto both = [](bool first, bool second) {
        return FlagsDelta{.updated = {{0, first}, {1, second}}};
    };
    const Recording<Flags> recorded{start,
                                    {{start, both(false, false)},
                                     {start + 1us, both(true, false)},
                                     {start + 2us, both(true, false)},
                                     {start + 3us, both(true, true)}},
                                    hglrt::testing::Held<Flags>{{start, Flags{false, false}},
                                                                {start + 1us, Flags{true, false}},
                                                                {start + 2us, Flags{true, false}},
                                                                {start + 3us, Flags{true, true}}}};
    const Dense<Flags> written{Whole<Flags>{{false, false}}, Whole<Flags>{{true, false}},
                               Whole<Flags>{{true, false}}, Whole<Flags>{{true, true}}};
    CHECK(hglrt::testing::same(recorded, written));
    CHECK(hglrt::testing::replay_items(written, start) == recorded.entries);
    CHECK(said(recorded, Dense<Flags>{Whole<Flags>{{false, false}}}) ==
          "the traces differ at 1us from the start: the eval has delta<list<bool, 2>>(updated: [0: true, 1: "
          "false], "
          "removed: []), holding [true, false], and the trace written out has nothing");
}

TEST_CASE("where two traces first differ, and what each has there (TST-4)") {
    const Recording<i64> recorded{start, {{start, 7}, {start + 2us, 9}}};
    CHECK(said(recorded, Dense<i64>{7, std::nullopt, 9}) == "(the same)");
    CHECK(said(recorded, Dense<i64>{7, std::nullopt, 8}) ==
          "the traces differ at 2us from the start: the eval has 9, and the trace written out has 8");
    CHECK(said(recorded, Dense<i64>{7, 9}) ==
          "the traces differ at 1us from the start: the eval has nothing, and the trace written out has 9");
    CHECK(said(recorded, Dense<i64>{7}) ==
          "the traces differ at 2us from the start: the eval has 9, and the trace written out has nothing");
    CHECK(said(Dense<i64>{7}, Dense<i64>{8}) ==
          "the traces differ at 0s from the start: the left one has 7, and the right one has 8");
    CHECK(said(Timed<i64>{{{start + 1s, 1}}}, Timed<i64>{{{start + 2s, 1}}}) ==
          "the traces differ at @1970-01-01T00:00:01.000001Z: the left one has 1, and the right one has "
          "nothing");
    // As an assertion.
    CHECK_NOTHROW(hglrt::testing::expect_same(recorded, Dense<i64>{7, std::nullopt, 9}));
    try {
        hglrt::testing::expect_same(recorded, Dense<i64>{7, std::nullopt, 8});
        FAIL("the assertion passed");
    } catch (const hglrt::testing::AssertionFailed& failure) {
        CHECK(std::string{failure.what()} ==
              "the traces differ at 2us from the start: the eval has 9, and the trace written out has 8");
    }
    CHECK_NOTHROW(hglrt::testing::expect_different(recorded, Dense<i64>{7}));
    CHECK_THROWS_WITH_AS(hglrt::testing::expect_different(recorded, Dense<i64>{7, std::nullopt, 9}),
                         "the two traces are the same", hglrt::testing::AssertionFailed);
}

TEST_CASE("what a `const fn` is given for a trace written out (TST-16)") {
    // A dense position and a duration key are after the earliest start, and
    // an instant is itself.
    CHECK(hglrt::testing::publications(Dense<i64>{7, std::nullopt, 9}) ==
          Recorded<i64>{{start, 7}, {start + 2us, 9}});
    const datetime opening = std::chrono::sys_days{std::chrono::year{2026} / 1 / 5} + 9h + 30min;
    CHECK(hglrt::testing::publications(Timed<i64>{{{opening, 1}, {opening + 1us, 2}}}) ==
          Recorded<i64>{{opening, 1}, {opening + 1us, 2}});
    CHECK(hglrt::testing::publications(Timed<i64>{{{0s, 1}, {1s, 2}}}) ==
          Recorded<i64>{{start, 1}, {start + 1s, 2}});
    // A whole cell is the delta it publishes.
    CHECK(hglrt::testing::publications(Dense<Values>{Whole<Values>{{4}}}) ==
          Recorded<ValuesDelta>{{start, ValuesDelta{.updated = {{0, 4}}, .size_modified = 1}}});
}

TEST_CASE("the text of a value, as the language writes it") {
    using hglrt::testing::text_of;
    using namespace std::chrono;
    CHECK(text_of(i64{-3}) == "-3");
    CHECK(text_of(0.1 + 0.2) == "0.30000000000000004");
    CHECK(text_of(1e21) == "1e21");
    CHECK(text_of(true) == "true");
    CHECK(text_of(hglrt::str{"a \"b\"\\\n\x01é"}) == "\"a \\\"b\\\"\\\\\\n\\u{1}é\"");
    CHECK(text_of(hglrt::bytes{"h\xff"}) == "bytes(104, 255)");
    CHECK(text_of(Maybe{}) == "null");
    CHECK(text_of(Maybe{7}) == "7");
    CHECK(text_of(Pair{1, 2}) == "(1, 2)");
    CHECK(text_of(Values{1, 2}) == "[1, 2]");
    CHECK(text_of(Values{}) == "[]");
    CHECK(text_of(Flags{true, false}) == "[true, false]");
    CHECK(text_of(ValuesDelta{.updated = {{0, 4}}, .removed = {1}, .size_modified = 1}) ==
          "delta<list<i64>>(updated: [0: 4], removed: [1], size_modified: 1)");
    CHECK(text_of(hglrt::CmpResult::greater) == "2");
    CHECK(text_of(hglrt::date{year{2026} / 9 / 3}) == "@2026-09-03");
    CHECK(text_of(hgraph::time_of_day(9, 30)) == "@09:30");
    CHECK(text_of(hgraph::time_of_day(9, 30, 5, 250'000)) == "@09:30:05.25");
    CHECK(text_of(datetime{sys_days{year{2026} / 9 / 3} + 9h + 30min}) == "@2026-09-03T09:30Z");
    CHECK(text_of(datetime{sys_days{year{1969} / 12 / 31} + 23h + 59min + 59s + 1us}) ==
          "@1969-12-31T23:59:59.000001Z");
    CHECK(text_of(hglrt::civil_datetime{year{2026} / 9 / 3, 9, 30}) == "@2026-09-03T09:30");
    CHECK(text_of(hglrt::duration{0}) == "0s");
    CHECK(text_of(hglrt::duration{90s}) == "90s");
    CHECK(text_of(hglrt::duration{-5min}) == "-5m");
    CHECK(text_of(hglrt::duration{48h}) == "2d");
    CHECK(text_of(hglrt::duration{1500ms}) == "1500ms");
    CHECK(text_of(hglrt::duration{7us}) == "7us");
    const auto zones = hgraph::make_time_zone_provider();
    const hglrt::zoned_datetime london =
        hgraph::at_zone(sys_days{year{2026} / 9 / 3} + 8h + 30min, hgraph::ZoneId{"Europe/London"}, *zones);
    CHECK(text_of(london).starts_with("@2026-09-03T09:30"));
    CHECK(text_of(london).ends_with("[Europe/London]"));
}
