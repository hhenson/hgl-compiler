// Unit tests of constants and of a source's timing (sources.h).
#include "core_nodes.h"

#include <doctest/doctest.h>

#include <chrono>
#include <cmath>
#include <limits>
#include <optional>
#include <string>
#include <tuple>
#include <vector>

namespace {

using hglrt::datetime;
using hglrt::i64;
using hglrt::Yielded;
using namespace std::chrono_literals;

/// A scheduler that keeps the wakes asked of it.
struct Wakes {
    mutable std::vector<datetime> asked;
    void schedule(datetime when) const { asked.push_back(when); }
};

/// A source's body: the yields it gives, one at each step.
struct Body {
    std::vector<Yielded<i64>> yields;
    std::size_t next = 0;

    std::optional<Yielded<i64>> operator()() {
        if (next >= yields.size()) {
            return std::nullopt;
        }
        return yields[next++];
    }
};

const datetime start = hglrt::testing::earliest_start();

/// The code of the HGL failure `call` raises, or none.
template <class Call>
std::optional<std::string> code_raised(Call&& call) {
    try {
        call();
    } catch (const hglrt::Failure& failure) {
        return failure.error().code.value_or("(no code)");
    }
    return std::nullopt;
}

/// A node with an `i64` input whose result is the constant 5, wired as
/// `fn five(value: i64) -> i64 => add(value, 5)` would wire its `5`.
struct Added {
    static constexpr auto name = "test::added";

    static void eval(hgraph::In<"lhs", hgraph::TS<i64>> lhs, hgraph::In<"rhs", hgraph::TS<i64>> rhs,
                     hgraph::Out<hgraph::TS<i64>> out) {
        out.set(hglrt::add__i64_i64(lhs.value(), rhs.value()));
    }
};

/// `1.0 / value`, as `1.0 / x` of a connection `x` is: a zero divisor gives
/// the infinity of its sign (NUM-3).
struct Reciprocal {
    static constexpr auto name = "test::reciprocal";

    static void eval(hgraph::In<"value", hgraph::TS<hglrt::f64>> value,
                     hgraph::Out<hgraph::TS<hglrt::f64>> out) {
        out.set(hglrt::div__f64_f64(1.0, value.value()));
    }
};

/// The one value the run `ran` recorded under `key`.
template <class T>
T recorded_once(const hglrt::testing::Run& ran, std::string_view key) {
    const auto entries = ran.get<hglrt::testing::Recorded<T>>(key);
    REQUIRE(entries.size() == 1);
    return std::get<1>(entries[0]);
}

} // namespace

TEST_CASE("a source publishes a current target and suspends at one to come (SOURCE-2, SOURCE-3)") {
    hglrt::Source<i64> source;
    Wakes wakes;
    Body body{{{start, 1}, {start + 5us, 2}, {2us, 3}}};
    std::vector<i64> published;
    const auto publish = [&](i64 value) { published.push_back(value); };

    // "The first evaluation begins the body": a current target publishes
    // now, and a future one keeps its payload and asks for a wake.
    hglrt::resume(source, start, wakes, body, publish);
    CHECK(published == std::vector<i64>{1});
    CHECK(wakes.asked == std::vector<datetime>{start + 5us});

    // An evaluation at another time is not the wake.
    hglrt::resume(source, start + 1us, wakes, body, publish);
    CHECK(published == std::vector<i64>{1});

    // "On wake it publishes and resumes after yield": a duration is from the
    // evaluation it is given in.
    hglrt::resume(source, start + 5us, wakes, body, publish);
    CHECK(published == std::vector<i64>{1, 2});
    CHECK(wakes.asked == std::vector<datetime>{start + 5us, start + 7us});

    // The body completes after the last publication (SOURCE-4).
    hglrt::resume(source, start + 7us, wakes, body, publish);
    CHECK(published == std::vector<i64>{1, 2, 3});
    CHECK(wakes.asked.size() == 2);
}

TEST_CASE("a past target is skipped and still orders the next (SOURCE-3)") {
    hglrt::Source<i64> source;
    Wakes wakes;
    std::vector<i64> published;
    const auto publish = [&](i64 value) { published.push_back(value); };

    Body later{{{start - 1us, 1}, {start, 2}}};
    hglrt::resume(source, start, wakes, later, publish);
    CHECK(published == std::vector<i64>{2});

    hglrt::Source<i64> again;
    Body earlier{{{start - 1us, 1}, {start - 2us, 2}}};
    CHECK(code_raised([&] { hglrt::resume(again, start, wakes, earlier, publish); }) ==
          "yield.non_increasing_time");
}

TEST_CASE("a target that does not increase, and a negative duration, fail (SOURCE-2, SOURCE-3)") {
    Wakes wakes;
    std::vector<i64> published;
    const auto publish = [&](i64 value) { published.push_back(value); };

    hglrt::Source<i64> repeated;
    Body same_time{{{start, 1}, {start, 2}}};
    CHECK(code_raised([&] { hglrt::resume(repeated, start, wakes, same_time, publish); }) ==
          "yield.non_increasing_time");
    // The publication made before the failure stands (ERR-8).
    CHECK(published == std::vector<i64>{1});

    hglrt::Source<i64> negative;
    Body backwards{{{-1us, 1}}};
    CHECK(code_raised([&] { hglrt::resume(negative, start, wakes, backwards, publish); }) ==
          "yield.negative_duration");
}

TEST_CASE("a constant publishes once, as its run starts (FN-3)") {
    const hglrt::testing::Entry entry = [](hgraph::Wiring& wiring, datetime) {
        auto value = hgraph::wire<hglrt_test::Replay>(
            wiring, hglrt::testing::Recorded<i64>{{start, 1}, {start + 2us, 2}});
        auto added = hgraph::wire<Added>(wiring, value, hglrt::constant(wiring, i64{5}));
        hgraph::wire<hglrt_test::Record>(wiring, added, hglrt::str{hglrt::testing::run_output});
    };
    const hglrt::testing::Recording<i64> recording = hglrt::testing::evaluate<i64>(entry);
    CHECK(recording.entries == hglrt::testing::Recorded<i64>{{start, 6}, {start + 2us, 7}});

    // The constant alone is valid from the first cycle and ticks there only.
    const hglrt::testing::Entry alone = [](hgraph::Wiring& wiring, datetime) {
        hgraph::wire<hglrt_test::Record>(wiring, hglrt::constant(wiring, i64{9}),
                                         hglrt::str{hglrt::testing::run_output});
    };
    CHECK(hglrt::testing::evaluate<i64>(alone).entries == hglrt::testing::Recorded<i64>{{start, 9}});
}

TEST_CASE("constants equal under `==` but of different values publish each its own (FN-3, NUM-2, NUM-3)") {
    using hglrt::f64;
    using hglrt::list;
    // `0.0 == -0.0`, and NUM-2 keeps the sign of a zero: `1.0 / x` of each
    // is the infinity of its sign, whichever is wired first.
    for (const bool plus_first : {true, false}) {
        const auto ran = hglrt::testing::run([plus_first](hgraph::Wiring& wiring, datetime) {
            std::optional<hgraph::Port<hgraph::TS<f64>>> plus;
            std::optional<hgraph::Port<hgraph::TS<f64>>> minus;
            if (plus_first) {
                plus = hglrt::constant(wiring, 0.0);
                minus = hglrt::constant(wiring, -0.0);
            } else {
                minus = hglrt::constant(wiring, -0.0);
                plus = hglrt::constant(wiring, 0.0);
            }
            hgraph::wire<hglrt_test::RecordOf<f64>>(wiring, hgraph::wire<Reciprocal>(wiring, *plus),
                                                    hglrt::str{"plus"});
            hgraph::wire<hglrt_test::RecordOf<f64>>(wiring, hgraph::wire<Reciprocal>(wiring, *minus),
                                                    hglrt::str{"minus"});
        });
        CHECK(recorded_once<f64>(ran, "plus") == std::numeric_limits<f64>::infinity());
        CHECK(recorded_once<f64>(ran, "minus") == -std::numeric_limits<f64>::infinity());
    }

    // So too where the zero is inside a value: a list's element and a
    // tuple's position.
    using Pair = std::tuple<i64, f64>;
    const auto ran = hglrt::testing::run([](hgraph::Wiring& wiring, datetime) {
        hgraph::wire<hglrt_test::RecordOf<list<f64>>>(wiring, hglrt::constant(wiring, list<f64>{0.0}),
                                                      hglrt::str{"plus"});
        hgraph::wire<hglrt_test::RecordOf<list<f64>>>(wiring, hglrt::constant(wiring, list<f64>{-0.0}),
                                                      hglrt::str{"minus"});
        hgraph::wire<hglrt_test::RecordOf<list<Pair>>>(wiring, hglrt::constant(wiring, list<Pair>{{1, 0.0}}),
                                                       hglrt::str{"pair plus"});
        hgraph::wire<hglrt_test::RecordOf<list<Pair>>>(wiring, hglrt::constant(wiring, list<Pair>{{1, -0.0}}),
                                                       hglrt::str{"pair minus"});
    });
    CHECK_FALSE(std::signbit(recorded_once<list<f64>>(ran, "plus").at(0)));
    CHECK(std::signbit(recorded_once<list<f64>>(ran, "minus").at(0)));
    CHECK_FALSE(std::signbit(std::get<1>(recorded_once<list<Pair>>(ran, "pair plus").at(0))));
    CHECK(std::signbit(std::get<1>(recorded_once<list<Pair>>(ran, "pair minus").at(0))));
}
