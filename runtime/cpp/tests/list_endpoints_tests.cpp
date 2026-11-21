// Unit tests of the temporal forms of a list and of a tuple
// (list_endpoints.h), on nodes written by hand as the compiler writes them,
// with hgraph's front end for generated code alone, each run in an hgraph
// graph in simulation. Each node keeps what it saw in a list of the test's,
// which the test then reads.
#include "core_nodes.h"

#include <hglrt/list_endpoints.h>
#include <hglrt/structs.h>

#include <doctest/doctest.h>
#include <hgraph/types/generated.h>

#include <array>
#include <chrono>
#include <cmath>
#include <optional>
#include <string>
#include <tuple>
#include <vector>

namespace {

using hglrt::datetime;
using hglrt::f64;
using hglrt::i64;
using hglrt::list;
using hglrt::list_delta;
using hglrt::testing::Dense;
using namespace std::chrono_literals;

using Growing = hgraph::TSL<hgraph::TS<i64>>;
using Pair = hgraph::TSL<hgraph::TS<i64>, 2>;

const datetime start = hglrt::testing::earliest_start();

/// The connection core's `replay` makes of `trace`.
hgraph::Port<hgraph::TS<i64>> replayed(hgraph::Wiring& wiring, const Dense<i64>& trace) {
    return hgraph::wire<hglrt_test::Replay>(wiring, hglrt::testing::replay_items(trace, start));
}

/// The code of the HGL failure `call` raises, as `assert raises` reads it
/// (`""` for one with none), or nothing where it raises none.
template <class Call>
std::optional<std::string> code_raised(Call&& call) {
    try {
        call();
    } catch (...) {
        return hglrt::testing::detail::escaping_code(std::current_exception()).value_or("");
    }
    return std::nullopt;
}

// ---- A growing list: TIME-7, TIME-9, TYPE-16 ------------------------------

/// What a reader saw of a growing list at a wake.
struct ListSeen {
    datetime at;
    bool valid;
    bool all_valid;
    std::optional<list_delta<list<i64>>> delta;

    friend bool operator==(const ListSeen&, const ListSeen&) = default;
};

std::vector<ListSeen> list_seen;

/// core's `write_readings` of collection_ticks.hgl and its neighbours: 0
/// writes a delta that names no change, 1 publishes `[1, 2]` whole, 2
/// updates position 3, 3 removes position 2, 4 shrinks by one, 5 publishes
/// `[]` whole, 6 sets the size to 1, and any other step invalidates the list.
struct WriteReadings {
    static constexpr auto name = "test::write_readings";

    static void eval(hgraph::In<"step", hgraph::TS<i64>> step, hgraph::Out<Growing> out_) {
        switch (step.value()) {
        case 0:
            hglrt::publish(out_, list_delta<list<i64>>{});
            break;
        case 1:
            hglrt::publish(out_, list<i64>{1, 2});
            break;
        case 2:
            hglrt::publish(out_, list_delta<list<i64>>{.updated = {{3, 5}}});
            break;
        case 3:
            hglrt::publish(out_, list_delta<list<i64>>{.removed = {2}});
            break;
        case 4:
            hglrt::publish(out_, list_delta<list<i64>>{.size_modified = -1});
            break;
        case 5:
            hglrt::publish(out_, list<i64>{});
            break;
        case 6:
            hglrt::publish(out_, list_delta<list<i64>>{.size_modified = 1});
            break;
        default:
            hglrt::fn_invalidate(out_);
        }
    }
};

/// `fn readings_valid(readings: list<i64>) -> bool { when valid([]) { ... } }`.
struct ReadingsSeen {
    static constexpr auto name = "test::readings_seen";

    static void eval(hgraph::In<"readings", Growing, hgraph::InputValidity::Unchecked> readings,
                     hgraph::DateTime now) {
        list_seen.push_back(ListSeen{
            now, hglrt::fn_valid(readings), hglrt::fn_all_valid(readings),
            hglrt::fn_modified(readings) ? std::optional{hglrt::fn_delta_value(readings)} : std::nullopt});
    }
};

std::vector<ListSeen> seen_of_steps(const Dense<i64>& steps) {
    list_seen.clear();
    (void)hglrt::testing::run([&](hgraph::Wiring& wiring, datetime) {
        hgraph::wire<ReadingsSeen>(wiring, hgraph::wire<WriteReadings>(wiring, replayed(wiring, steps)));
    });
    return list_seen;
}

TEST_CASE("a growing list's empty publications tick only where they make it valid (TIME-7)") {
    const list_delta<list<i64>> none{};
    // TIME-7: "Where a `list<V>` ... is not valid, never published or
    // invalidated, it makes the value valid and empty, and ticks. Where it is
    // valid the delta changes nothing, and is no tick"; and the reader still
    // sees the list as it is at its next wake, by the invalidation.
    CHECK(seen_of_steps(Dense<i64>{0, 0, 9, 0}) == std::vector<ListSeen>{
                                                       {start, true, true, none},
                                                       {start + 2us, false, false, std::nullopt},
                                                       {start + 3us, true, true, none},
                                                   });
    // "an empty list ... given to a valid empty one, the value is unchanged,
    // and it is no tick ... Each ... position of a list that it gives is
    // published, the same value again included".
    const list_delta<list<i64>> both{.updated = {{0, 1}, {1, 2}}, .size_modified = 2};
    CHECK(seen_of_steps(Dense<i64>{5, 5, 1, 1, 5, 5}) ==
          std::vector<ListSeen>{
              {start, true, true, none},
              {start + 2us, true, true, both},
              {start + 3us, true, true, list_delta<list<i64>>{.updated = {{0, 1}, {1, 2}}}},
              {start + 4us, true, true, list_delta<list<i64>>{.removed = {0, 1}, .size_modified = 0}},
          });
}

/// `[1, 2, 3]` published whole, then each position removed, then position 0
/// updated alone (TYPE-16).
struct EmptiedPositions {
    static constexpr auto name = "test::emptied_positions";

    static void eval(hgraph::In<"step", hgraph::TS<i64>> step, hgraph::Out<Growing> out_) {
        switch (step.value()) {
        case 0:
            hglrt::publish(out_, list<i64>{1, 2, 3});
            break;
        case 1:
            hglrt::publish(out_, list_delta<list<i64>>{.removed = {0, 1, 2}});
            break;
        default:
            hglrt::publish(out_, list_delta<list<i64>>{.updated = {{0, 7}}});
        }
    }
};

TEST_CASE("a growing list whose every position lost its value keeps its size (TYPE-16)") {
    list_seen.clear();
    (void)hglrt::testing::run([&](hgraph::Wiring& wiring, datetime) {
        hgraph::wire<ReadingsSeen>(
            wiring, hgraph::wire<EmptiedPositions>(wiring, replayed(wiring, Dense<i64>{0, 1, 2})));
    });
    // TYPE-16: "`removed`, the positions that lost their value ... and
    // `size_modified`, set only when the size moves": the positions stay,
    // not valid, and an update of one moves no size. END-4 has an
    // invalidation empty the list; a removal does not.
    CHECK(list_seen == std::vector<ListSeen>{
                           {start, true, true,
                            list_delta<list<i64>>{.updated = {{0, 1}, {1, 2}, {2, 3}}, .size_modified = 3}},
                           {start + 1us, false, false, std::nullopt},
                           {start + 2us, true, false, list_delta<list<i64>>{.updated = {{0, 7}}}},
                       });
}

TEST_CASE("a growing list's delta extends, leaves gaps, shrinks and ignores a gap's removal (TYPE-16)") {
    CHECK(seen_of_steps(Dense<i64>{1, 2, 3, 4, 6}) ==
          std::vector<ListSeen>{
              {start, true, true, list_delta<list<i64>>{.updated = {{0, 1}, {1, 2}}, .size_modified = 2}},
              // "`updated: [3: 5]` on `[1, 2]` gives four positions, the
              // third not valid".
              {start + 1us, true, false, list_delta<list<i64>>{.updated = {{3, 5}}, .size_modified = 4}},
              // The removal of the gap names no change: no tick.
              // "`size_modified: -1` drops its last position", named in
              // `removed`.
              {start + 3us, true, false, list_delta<list<i64>>{.removed = {3}, .size_modified = 3}},
              {start + 4us, true, true, list_delta<list<i64>>{.removed = {1, 2}, .size_modified = 1}},
          });
}

std::vector<std::optional<list<i64>>> totals_read;
std::vector<std::optional<std::vector<double>>> f64_read;

/// What a lifted `sum_readings(readings)` is given (FN-12): the list read
/// whole, or the failure of the read.
struct ReadWhole {
    static constexpr auto name = "test::read_whole";

    static void eval(hgraph::In<"readings", Growing> readings) {
        const hglrt::result<list<i64>> read = hglrt::checked_whole_result(readings);
        totals_read.push_back(read.has_value() ? std::optional{*read} : std::nullopt);
        CHECK(code_raised([&] { (void)hglrt::checked_whole(readings); }) ==
              (read.has_value() ? std::nullopt : std::optional<std::string>{"value.unset_read"}));
    }
};

struct Floats {
    static constexpr auto name = "test::floats";
    static void eval(hgraph::In<"step", hgraph::TS<i64>> step,
                     hgraph::Out<hgraph::TSL<hgraph::TS<f64>>> out_) {
        if (step.value() == 0) {
            hglrt::publish(out_, list<f64>{1.0, 2.0});
        } else {
            hglrt::publish(out_, list_delta<list<f64>>{.updated = {{3, 5.0}}});
        }
    }
};

struct ReadFloats {
    static constexpr auto name = "test::read_floats";
    static void eval(hgraph::In<"values", hgraph::TSL<hgraph::TS<f64>>> values) {
        const list<f64> read = hglrt::checked_whole(values);
        f64_read.push_back(std::vector<double>(read.begin(), read.end()));
    }
};

TEST_CASE("a growing list read whole fails at a gap of an i64, and gives NaN at one of an f64 (TIME-1)") {
    totals_read.clear();
    (void)hglrt::testing::run([&](hgraph::Wiring& wiring, datetime) {
        hgraph::wire<ReadWhole>(
            wiring, hgraph::wire<WriteReadings>(wiring, replayed(wiring, Dense<i64>{1, 2, 4, 4})));
    });
    CHECK(totals_read == std::vector<std::optional<list<i64>>>{list<i64>{1, 2}, std::nullopt, std::nullopt,
                                                               list<i64>{1, 2}});

    f64_read.clear();
    (void)hglrt::testing::run([&](hgraph::Wiring& wiring, datetime) {
        hgraph::wire<ReadFloats>(wiring, hgraph::wire<Floats>(wiring, replayed(wiring, Dense<i64>{0, 1})));
    });
    REQUIRE(f64_read.size() == 2);
    CHECK(*f64_read[0] == std::vector<double>{1.0, 2.0});
    REQUIRE(f64_read[1]->size() == 4);
    CHECK(std::isnan((*f64_read[1])[2]));
    CHECK((*f64_read[1])[3] == 5.0);
}

/// A delta that a list cannot take fails the write that publishes it
/// (TYPE-16): a fixed list's position past its bounds, with the code of an
/// index outside a list (EXPR-3), and an `updated` entry at or past the size
/// written with it, with none.
struct Refused {
    static constexpr auto name = "test::refused";

    static void eval(hgraph::In<"step", hgraph::TS<i64>> step, hgraph::Out<Growing> out_) {
        if (step.value() == 0) {
            hglrt::publish(out_, list<i64>{1, 2, 3});
        } else {
            hglrt::publish(out_, list_delta<list<i64>>{.updated = {{step.value(), 9}}, .size_modified = 2});
        }
    }
};

struct FixedRefused {
    static constexpr auto name = "test::fixed_refused";

    static void eval(hgraph::In<"slot", hgraph::TS<i64>> slot, hgraph::Out<Pair> out_) {
        hglrt::publish(out_, list_delta<std::array<i64, 2>>{.updated = {{slot.value(), 3}}});
    }
};

/// A reader of a connection of the schema `S`, so that what publishes it
/// is evaluated.
template <class S>
struct Sink {
    static constexpr auto name = "test::sink";
    static void eval(hgraph::In<"input", S, hgraph::InputValidity::Unchecked>) {}
};

TEST_CASE("a delta a list cannot take fails its publication (TYPE-16, EXPR-3)") {
    CHECK(code_raised([] {
              (void)hglrt::testing::run([](hgraph::Wiring& wiring, datetime) {
                  hgraph::wire<Sink<Growing>>(
                      wiring, hgraph::wire<Refused>(wiring, replayed(wiring, Dense<i64>{0, 1})));
              });
          }) == std::nullopt);
    CHECK(code_raised([] {
              (void)hglrt::testing::run([](hgraph::Wiring& wiring, datetime) {
                  hgraph::wire<Sink<Growing>>(
                      wiring, hgraph::wire<Refused>(wiring, replayed(wiring, Dense<i64>{0, 2})));
              });
          }) == "");
    CHECK(code_raised([] {
              (void)hglrt::testing::run([](hgraph::Wiring& wiring, datetime) {
                  hgraph::wire<Sink<Pair>>(
                      wiring, hgraph::wire<FixedRefused>(wiring, replayed(wiring, Dense<i64>{0, 2})));
              });
          }) == "value.index_out_of_range");
}

// ---- A fixed list and a tuple: TIME-9, END-7, END-8 -----------------------

/// What a reader saw of a fixed list at a wake.
struct PairSeen {
    bool valid;
    bool all_valid;
    std::optional<list_delta<std::array<i64, 2>>> delta;
    std::optional<std::array<i64, 2>> whole;
    std::optional<i64> second;

    friend bool operator==(const PairSeen&, const PairSeen&) = default;
};

std::vector<PairSeen> pair_seen;

/// `fn set_one_position(slot: i64, reading: i64) -> list<i64, 2> { when {
/// return delta<list<i64, 2>>(updated: [slot: reading]) } }`.
struct SetOnePosition {
    static constexpr auto name = "test::set_one_position";

    static void eval(hgraph::In<"slot", hgraph::TS<i64>> slot, hgraph::In<"reading", hgraph::TS<i64>> reading,
                     hgraph::Out<Pair> out_) {
        hglrt::publish(out_, list_delta<std::array<i64, 2>>{
                                 .updated = {{hglrt::checked_value(slot), hglrt::checked_value(reading)}}});
    }
};

/// A probe of the pair at each wake: `valid`, `all_valid`, `delta_value`,
/// the whole value and `pair[1]` in the form that takes a `result`.
struct PairProbe {
    static constexpr auto name = "test::pair_probe";

    static void eval(hgraph::In<"probe", hgraph::TS<i64>, hgraph::InputValidity::Unchecked>,
                     hgraph::In<"pair", Pair, hgraph::InputValidity::Unchecked> pair) {
        const hglrt::result<std::array<i64, 2>> whole = hglrt::checked_whole_result(pair);
        const hglrt::result<i64> second = hglrt::checked_whole_result(hglrt::child(pair, 1));
        pair_seen.push_back(
            PairSeen{hglrt::fn_valid(pair), hglrt::fn_all_valid(pair),
                     hglrt::fn_modified(pair) ? std::optional{hglrt::fn_delta_value(pair)} : std::nullopt,
                     whole.has_value() ? std::optional{*whole} : std::nullopt,
                     second.has_value() ? std::optional{*second} : std::nullopt});
        CHECK(second.has_value() == hglrt::fn_valid(hglrt::child(pair, 1)));
    }
};

TEST_CASE("a fixed list is valid once a position is, all valid once both are (TIME-9, END-7)") {
    pair_seen.clear();
    (void)hglrt::testing::run([](hgraph::Wiring& wiring, datetime) {
        const auto pair = hgraph::wire<SetOnePosition>(
            wiring, replayed(wiring, Dense<i64>{std::nullopt, 0, std::nullopt, 1}),
            replayed(wiring, Dense<i64>{std::nullopt, 3, std::nullopt, 8}));
        hgraph::wire<PairProbe>(wiring, replayed(wiring, Dense<i64>{1, 1, 1, 1}), pair);
    });
    using D = list_delta<std::array<i64, 2>>;
    CHECK(pair_seen == std::vector<PairSeen>{
                           {false, false, std::nullopt, std::nullopt, std::nullopt},
                           {true, false, D{.updated = {{0, 3}}}, std::nullopt, std::nullopt},
                           {true, false, std::nullopt, std::nullopt, std::nullopt},
                           {true, true, D{.updated = {{1, 8}}}, std::array<i64, 2>{3, 8}, 8},
                       });
}

struct OutOfBounds {
    static constexpr auto name = "test::out_of_bounds";
    static void eval(hgraph::In<"pair", Pair> pair, hgraph::In<"values", Growing> values) {
        CHECK(code_raised([&] { (void)hglrt::child(pair, 2); }) == "value.index_out_of_range");
        CHECK(code_raised([&] { (void)hglrt::child(values, -1); }) == "value.index_out_of_range");
        CHECK(!hglrt::fn_valid(hglrt::child(values, 7)));
        CHECK(code_raised([&] { (void)hglrt::checked_whole(hglrt::child(values, 7)); }) ==
              "value.unset_read");
    }
};

TEST_CASE(
    "a child read at an index outside a fixed list fails (EXPR-3), past a growing one's end is not valid") {
    (void)hglrt::testing::run([](hgraph::Wiring& wiring, datetime) {
        const auto pair = hgraph::wire<SetOnePosition>(wiring, replayed(wiring, Dense<i64>{0}),
                                                       replayed(wiring, Dense<i64>{3}));
        const auto values = hgraph::wire<WriteReadings>(wiring, replayed(wiring, Dense<i64>{1}));
        hgraph::wire<OutOfBounds>(wiring, pair, values);
    });
}

std::vector<i64> spans;

/// `fn span_of_pair(slot: i64, reading: i64) -> i64 { let pair =
/// set_one_position(slot, reading); return span(pair[0], pair[1]) }`, its
/// lifted `span` a node of two inputs each a child of the pair.
struct Span {
    static constexpr auto name = "test::span";
    static void eval(hgraph::In<"low", hgraph::TS<i64>> low, hgraph::In<"high", hgraph::TS<i64>> high) {
        spans.push_back(hglrt::checked_value(high) - hglrt::checked_value(low));
    }
};

TEST_CASE("a child of a fixed list is wired as a connection of its own (TIME-9, FN-11)") {
    spans.clear();
    (void)hglrt::testing::run([](hgraph::Wiring& wiring, datetime) {
        const hgraph::Port<Pair> pair =
            hgraph::wire<SetOnePosition>(wiring, replayed(wiring, Dense<i64>{0, 1, std::nullopt, 0}),
                                         replayed(wiring, Dense<i64>{3, 8, std::nullopt, 4}));
        hgraph::wire<Span>(wiring, hglrt::element_port(pair, 0), hglrt::element_port(pair, 1));
    });
    CHECK(spans == std::vector<i64>{5, 4});
    CHECK(code_raised([] {
              (void)hglrt::testing::run([](hgraph::Wiring& wiring, datetime) {
                  const hgraph::Port<Pair> pair = hgraph::wire<SetOnePosition>(
                      wiring, replayed(wiring, Dense<i64>{0}), replayed(wiring, Dense<i64>{3}));
                  (void)hglrt::element_port(pair, 2);
              });
          }) == "value.index_out_of_range");
}

/// What a reader of a growing list's position saw: each publication of it.
std::vector<std::pair<datetime, i64>> positions_seen;

struct PositionReader {
    static constexpr auto name = "test::position_reader";
    static void eval(hgraph::In<"position", hgraph::TS<i64>, hgraph::InputValidity::Unchecked> position,
                     hgraph::DateTime now) {
        if (hglrt::fn_modified(position)) {
            positions_seen.emplace_back(now, hglrt::checked_value(position));
        }
    }
};

TEST_CASE("a position of a growing list is wired before the list holds it (TIME-1, TIME-9)") {
    positions_seen.clear();
    // [], [1, 2], [1], [1, 2], then position 3 updated: the reader of
    // position 1 sees its publications, and nothing while the list does not
    // reach it.
    (void)hglrt::testing::run([](hgraph::Wiring& wiring, datetime) {
        const hgraph::Port<Growing> values =
            hgraph::wire<WriteReadings>(wiring, replayed(wiring, Dense<i64>{5, 1, 4, 1, 2}));
        hgraph::wire<PositionReader>(wiring, hglrt::element_port(values, 1));
    });
    CHECK(positions_seen == std::vector<std::pair<datetime, i64>>{{start + 1us, 2}, {start + 3us, 2}});
    CHECK(code_raised([] {
              (void)hglrt::testing::run([](hgraph::Wiring& wiring, datetime) {
                  const hgraph::Port<Growing> values =
                      hgraph::wire<WriteReadings>(wiring, replayed(wiring, Dense<i64>{1}));
                  (void)hglrt::element_port(values, -1);
              });
          }) == "value.index_out_of_range");
}

struct Yields {
    static constexpr auto name = "test::yields";
    static void eval(hgraph::In<"step", hgraph::TS<i64>> step, hgraph::Out<Growing> out_) {
        if (step.value() == 0) {
            hglrt::publish(out_, list<i64>{4});
        } else {
            hglrt::publish(out_, list_delta<list<i64>>{.updated = {{1, 5}}});
        }
    }
};

std::vector<list<bool>> flags_read;

/// A growing `list<bool>`, a `std::vector<bool>`, whose element is a bit:
/// published whole and by delta, and read whole, on every standard library
/// (an element of one is a proxy on libc++).
struct Flags {
    static constexpr auto name = "test::flags";
    static void eval(hgraph::In<"step", hgraph::TS<i64>> step,
                     hgraph::Out<hgraph::TSL<hgraph::TS<bool>>> out_) {
        if (step.value() == 0) {
            const list<bool> whole{true, false};
            hglrt::publish(out_, whole);
        } else {
            hglrt::publish(out_, list_delta<list<bool>>{.updated = {{1, true}, {2, false}}});
        }
    }
};

struct ReadFlags {
    static constexpr auto name = "test::read_flags";
    static void eval(hgraph::In<"flags", hgraph::TSL<hgraph::TS<bool>>> flags) {
        flags_read.push_back(hglrt::checked_whole(flags));
    }
};

TEST_CASE("a growing list of bool is published and read whole (TIME-7, TIME-1)") {
    flags_read.clear();
    (void)hglrt::testing::run([](hgraph::Wiring& wiring, datetime) {
        hgraph::wire<ReadFlags>(wiring, hgraph::wire<Flags>(wiring, replayed(wiring, Dense<i64>{0, 1})));
    });
    CHECK(flags_read == std::vector<list<bool>>{list<bool>{true, false}, list<bool>{true, true, false}});
}

TEST_CASE("a trace of a list of lists replays a whole cell from what each position held (TST-9)") {
    using Rows = list<list<i64>>;
    using Inner = list_delta<list<i64>>;
    using Cell = hglrt::testing::Cell<Rows>;
    const auto items = hglrt::testing::replay_items(
        Dense<Rows>{Cell{hglrt::testing::Whole<Rows>{Rows{{1}, {2, 3}}}},
                    Cell{list_delta<Rows>{.updated = {{1, Inner{.updated = {{2, 9}}}}}}},
                    Cell{hglrt::testing::Whole<Rows>{Rows{{4}}}}},
        start);
    REQUIRE(items.size() == 3);
    CHECK(std::get<1>(items[0]) ==
          list_delta<Rows>{.updated = {{0, Inner{.updated = {{0, 1}}, .size_modified = 1}},
                                       {1, Inner{.updated = {{0, 2}, {1, 3}}, .size_modified = 2}}},
                           .size_modified = 2});
    // The third cell is published from what each position then held: the
    // first row keeps its size, and the second, which grew to three, goes.
    CHECK(std::get<1>(items[2]) ==
          list_delta<Rows>{.updated = {{0, Inner{.updated = {{0, 4}}}}}, .removed = {1}, .size_modified = 1});
}

TEST_CASE("a source's yield publishes a whole list or a delta of one (SOURCE-1, TIME-7)") {
    totals_read.clear();
    (void)hglrt::testing::run([](hgraph::Wiring& wiring, datetime) {
        hgraph::wire<ReadWhole>(wiring, hgraph::wire<Yields>(wiring, replayed(wiring, Dense<i64>{0, 1})));
    });
    CHECK(totals_read == std::vector<std::optional<list<i64>>>{list<i64>{4}, list<i64>{4, 5}});
}

using PairDeltas = hglrt::testing::Recorded<list_delta<std::array<i64, 2>>>;

/// core's `record` at `list<i64, 2>`: each delta, put in the store at stop.
struct RecordPair {
    static constexpr auto name = "test::record_pair";

    static void start(hgraph::State<PairDeltas> state) { state.set(PairDeltas{}); }

    static void eval(hgraph::In<"input", Pair> input, hgraph::State<PairDeltas> state,
                     hgraph::EvaluationClockView clock) {
        if (hglrt::fn_modified(input)) {
            hglrt::fn_append(state.modify(),
                             std::tuple{clock.evaluation_time(), hglrt::fn_delta_value(input)});
        }
    }

    static void stop(hgraph::State<PairDeltas> state, hgraph::GlobalStateView global_state) {
        hglrt::put_GlobalState_str_V(hglrt::GlobalState{global_state}, hglrt::str{hglrt::testing::run_output},
                                     state.ref());
    }
};

/// The node an eval's entry wires for a result whose delta is no T (the
/// compiler's `tests`): at each publication, the value the result holds
/// read whole, or none where that read fails.
struct HoldPair {
    static constexpr auto name = "test::hold_pair";

    static void eval(hgraph::In<"result", Pair> result, hgraph::DateTime now) {
        if (!hglrt::fn_modified(result)) {
            return;
        }
        std::optional<std::array<i64, 2>> whole;
        try {
            whole.emplace(hglrt::checked_whole(result));
        } catch (const hglrt::Failure&) {
        }
        hglrt::testing::hold<std::array<i64, 2>>(now, std::move(whole));
    }
};

TEST_CASE("an eval keeps the value a list result holds for a cell written whole (TST-5)") {
    const auto recorded = hglrt::testing::evaluate<std::array<i64, 2>>([](hgraph::Wiring& wiring, datetime) {
        const hgraph::Port<Pair> pair = hgraph::wire<SetOnePosition>(
            wiring, replayed(wiring, Dense<i64>{0, 1}), replayed(wiring, Dense<i64>{3, 8}));
        hgraph::wire<RecordPair>(wiring, pair);
        hgraph::wire<HoldPair>(wiring, pair);
    });
    using D = list_delta<std::array<i64, 2>>;
    CHECK(recorded.entries ==
          PairDeltas{{start, D{.updated = {{0, 3}}}}, {start + 1us, D{.updated = {{1, 8}}}}});
    REQUIRE(recorded.held.has_value());
    CHECK(*recorded.held == hglrt::testing::Held<std::array<i64, 2>>{
                                {start, std::nullopt}, {start + 1us, std::array<i64, 2>{3, 8}}});
    // A cell written whole is compared with the value held (TST-5).
    using Cell = hglrt::testing::Cell<std::array<i64, 2>>;
    CHECK(hglrt::testing::same(
        recorded, Dense<std::array<i64, 2>>{Cell{D{.updated = {{0, 3}}}},
                                            Cell{hglrt::testing::Whole<std::array<i64, 2>>{{3, 8}}}}));
}

// ---- A list of tuples: TYPE-9, each part through its own form -----------

using Named = std::tuple<i64, hglrt::str>;
using NamedTS = hgraph::UnNamedTSB<hgraph::Field<"0", hgraph::TS<i64>>, hgraph::Field<"1", hgraph::TS<hglrt::str>>>;
using Names = hgraph::TSL<NamedTS>;

std::vector<std::optional<list<Named>>> names_read;
std::vector<list_delta<list<Named>>> names_changed;

/// `fn names(step: i64) -> list<tuple<i64, str>>`: a list published whole,
/// then a delta that adds a tuple, then one that renames the first.
struct NamesWritten {
    static constexpr auto name = "test::names_written";
    static void eval(hgraph::In<"step", hgraph::TS<i64>> step, hgraph::Out<Names> out_) {
        if (step.value() == 0) {
            hglrt::publish(out_, list<Named>{Named{1, "a"}});
        } else if (step.value() == 1) {
            hglrt::publish(out_, list_delta<list<Named>>{.updated = {{1, {2, hglrt::str{"b"}}}}});
        } else {
            hglrt::publish(out_, list_delta<list<Named>>{.updated = {{0, {std::nullopt, hglrt::str{"c"}}}}});
        }
    }
};

struct NamesRead {
    static constexpr auto name = "test::names_read";
    static void eval(hgraph::In<"names", Names> names) {
        const hglrt::result<list<Named>> read = hglrt::checked_whole_result(names);
        names_read.push_back(read.has_value() ? std::optional{*read} : std::nullopt);
        names_changed.push_back(hglrt::fn_delta_value(names));
    }
};

TEST_CASE("a list of tuples is published, read and read as its delta through the tuple's form (TYPE-9)") {
    names_read.clear();
    names_changed.clear();
    (void)hglrt::testing::run([](hgraph::Wiring& wiring, datetime) {
        hgraph::wire<NamesRead>(wiring, hgraph::wire<NamesWritten>(wiring, replayed(wiring, Dense<i64>{0, 1, 2})));
    });
    CHECK(names_read == std::vector<std::optional<list<Named>>>{list<Named>{Named{1, "a"}},
                                                                list<Named>{Named{1, "a"}, Named{2, "b"}},
                                                                list<Named>{Named{1, "c"}, Named{2, "b"}}});
    using D = list_delta<list<Named>>;
    CHECK(names_changed == std::vector<D>{D{.updated = {{0, {1, hglrt::str{"a"}}}}, .size_modified = 1},
                                          D{.updated = {{1, {2, hglrt::str{"b"}}}}, .size_modified = 2},
                                          D{.updated = {{0, {std::nullopt, hglrt::str{"c"}}}}}});
}

/// TST-9: a cell written whole publishes its change from what the cells
/// before it left, each part followed by its own publisher: a list of tuples
/// gives each position published and the tail it drops; a tuple's list does
/// as a list does, and moves its size only where it moves (TYPE-16).
TEST_CASE("a whole cell publishes each part as its change from what it held (TST-9, TYPE-16)") {
    using hglrt::testing::detail::Publisher;
    // A cell of `value` written whole, published by `publisher`.
    const auto whole = [](auto& publisher, const auto& value) {
        using V = std::remove_cvref_t<decltype(value)>;
        return publisher(hglrt::testing::Cell<V>{hglrt::testing::Whole<V>{value}});
    };
    Publisher<list<Named>> names;
    static_cast<void>(whole(names, list<Named>{Named{1, "a"}, Named{2, "b"}}));
    CHECK(whole(names, list<Named>{Named{1, "a"}}) ==
          list_delta<list<Named>>{.updated = {{0, {1, hglrt::str{"a"}}}}, .removed = {1}, .size_modified = 1});
    using Tagged = std::tuple<list<i64>, bool>;
    Publisher<Tagged> tagged;
    static_cast<void>(whole(tagged, Tagged{list<i64>{1, 2}, true}));
    const auto shrunk = whole(tagged, Tagged{list<i64>{3}, false});
    CHECK(std::get<0>(shrunk) == list_delta<list<i64>>{.updated = {{0, 3}}, .removed = {1}, .size_modified = 1});
    CHECK(std::get<1>(shrunk) == std::optional<bool>{false});
    // A list given its own size moves none; a delta followed moves it.
    CHECK(std::get<0>(whole(tagged, Tagged{list<i64>{4}, false})) == list_delta<list<i64>>{.updated = {{0, 4}}});
    static_cast<void>(tagged(hglrt::testing::Cell<Tagged>{
        std::in_place_index<0>, hglrt::delta_t<Tagged>{list_delta<list<i64>>{.updated = {{1, 5}}}, std::nullopt}}));
    CHECK(std::get<0>(whole(tagged, Tagged{list<i64>{}, false})) ==
          list_delta<list<i64>>{.removed = {0, 1}, .size_modified = 0});
}

} // namespace
