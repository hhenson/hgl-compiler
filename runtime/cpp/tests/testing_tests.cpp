// Unit tests of assertions, eval and run bounds (testing.h).
#include "core_nodes.h"

#include <doctest/doctest.h>

#include <chrono>
#include <exception>
#include <limits>
#include <optional>
#include <string>

namespace {

using hglrt::datetime;
using hglrt::i64;
using hglrt::testing::Dense;
using hglrt::testing::Mode;
using hglrt::testing::Timed;
using hglrt_test::code_raised;
using hglrt_test::start;
using namespace std::chrono_literals;

} // namespace

TEST_CASE("assert raises takes a failure of its code alone (TST-10, TST-11)") {
    hglrt::testing::raises("numeric.out_of_range",
                           [] { (void)hglrt::add__i64_i64(std::numeric_limits<i64>::max(), 1); });
    // Normal completion fails the test.
    try {
        hglrt::testing::raises("numeric.out_of_range", [] {});
        FAIL("the assertion passed");
    } catch (const hglrt::testing::AssertionFailed& failure) {
        CHECK(std::string{failure.what()} ==
              "the block raised nothing, and `numeric.out_of_range` was expected");
    }
    // A different code fails it, and says what was raised.
    try {
        hglrt::testing::raises("value.unset_read", [] { hglrt::fail("convert.invalid", "not a number"); });
        FAIL("the assertion passed");
    } catch (const hglrt::testing::AssertionFailed& failure) {
        CHECK(std::string{failure.what()} ==
              "the block raised convert.invalid: not a number, and `value.unset_read` was expected");
    }
    // A failure with no code satisfies no coded expectation.
    CHECK_THROWS_AS(hglrt::testing::raises("value.unset_read", [] { hglrt::fail_uncoded("uncoded"); }),
                    hglrt::testing::AssertionFailed);
    // "A failed nested assertion cannot satisfy an enclosing raises", nor can
    // one an inner `raises` refused.
    CHECK_THROWS_AS(hglrt::testing::raises("convert.invalid", [] { hglrt::testing::check(false); }),
                    hglrt::testing::AssertionFailed);
    CHECK_THROWS_AS(hglrt::testing::raises("convert.invalid",
                                           [] {
                                               hglrt::testing::raises("value.unset_read", [] {
                                                   hglrt::fail("convert.invalid", "inner");
                                               });
                                           }),
                    hglrt::testing::AssertionFailed);
}

TEST_CASE("a run's bounds (CRUN-2, CRUN-3)") {
    const auto [unstated, none] = hglrt::testing::resolved({});
    CHECK(unstated == start);
    CHECK_FALSE(none.has_value());
    CHECK(hglrt::testing::resolved({.length = 3us}).second == start + 3us);
    CHECK(hglrt::testing::resolved({.start = start + 1s, .length = 3us}).second == start + 1s + 3us);
    CHECK(code_raised([] { (void)hglrt::testing::resolved({.end = start + 1s, .length = 1s}); }) ==
          "run.end_and_length");
    // CRUN-3 reserves the epoch itself for `never`.
    CHECK(code_raised([] { (void)hglrt::testing::resolved({.start = datetime{}}); }) == "(no code)");
}

TEST_CASE("a run's end is no later than the latest end, and no cycle is at or after forever (CRUN-3)") {
    using hglrt::testing::evaluate;
    using hglrt::testing::resolved;
    using hglrt_test::Counted;
    using hglrt_test::entry_of;
    namespace c = std::chrono;
    // "`forever`, `2262-04-11T00:00:00Z` ... the latest end, `latest_end`,
    // one smallest step before `forever`".
    const datetime forever{c::sys_days{c::year{2262} / 4 / 11}};
    const datetime latest = hglrt::testing::latest_end();
    CHECK(latest == forever - 1us);
    CHECK(resolved({.end = latest}).second == latest);
    CHECK(resolved({.start = latest - 1s, .length = 1s}).second == latest);
    CHECK(hglrt::testing::same(evaluate<i64>(entry_of<Counted>(Dense<i64>{7}), {.end = latest}),
                               Dense<i64>{1}));

    // An end after it fails as a start before the earliest start does, with
    // no code: one given, and one a length gives, a length so long that no
    // instant holds the end among them.
    const i64 most = std::numeric_limits<i64>::max();
    for (const hglrt::testing::Bounds& past :
         {hglrt::testing::Bounds{.end = latest + 1us}, hglrt::testing::Bounds{.end = forever + c::years{100}},
          hglrt::testing::Bounds{.start = latest, .length = 1us},
          hglrt::testing::Bounds{.start = forever + c::years{1}, .length = 1s},
          hglrt::testing::Bounds{.start = datetime{c::sys_days{c::year{2300} / 1 / 1}}, .length = 1s},
          hglrt::testing::Bounds{.length = c::days{1'000'000}},
          hglrt::testing::Bounds{.length = hglrt::duration{most}},
          hglrt::testing::Bounds{.start = latest, .length = hglrt::duration{most}}}) {
        CHECK(code_raised([&] { (void)resolved(past); }) == "(no code)");
        CHECK(code_raised([&] { (void)evaluate<i64>(entry_of<Counted>(Dense<i64>{7}), past); }) ==
              "(no code)");
    }

    // A run with no end ends at the latest end: a start at it or after it,
    // after forever and after hgraph's own last time among them, runs no
    // cycle, and a publication timed after forever is not made.
    for (const datetime late : {latest, forever + c::years{1}, datetime{c::sys_days{c::year{2300} / 1 / 1}},
                                datetime{c::sys_days{c::year{9999} / 12 / 31}}}) {
        const auto ran = evaluate<i64>(entry_of<Counted>(Dense<i64>{7}), {.start = late});
        CHECK(ran.start == late);
        CHECK(ran.entries.empty());
    }
    const auto ran =
        evaluate<i64>(entry_of<Counted>(Timed<i64>{{{0s, 7}, {latest - 1us, 7}, {forever + 1s, 7}}}));
    CHECK(hglrt::testing::same(ran, Timed<i64>{{{0s, 1}, {latest - 1us, 2}}}));
}

TEST_CASE("an eval runs its entry in simulation and reads its recording (CEVAL-1, CEVAL-4)") {
    using hglrt_test::Counted;
    using hglrt_test::entry_of;
    // core's conformance case replay_preserves_equal_ticks.
    const auto counted =
        hglrt::testing::evaluate<i64>(entry_of<Counted>(Dense<i64>{7, std::nullopt, 7, 7, std::nullopt}));
    CHECK(hglrt::testing::same(counted, Dense<i64>{1, std::nullopt, 2, 3}));
    // RUN-4 and CEVAL-5: each run has fresh state and a fresh recording.
    CHECK(hglrt::testing::same(hglrt::testing::evaluate<i64>(entry_of<Counted>(Dense<i64>{})), Dense<i64>{}));
    CHECK(
        hglrt::testing::same(hglrt::testing::evaluate<i64>(entry_of<Counted>(Dense<i64>{7, std::nullopt, 7})),
                             Dense<i64>{1, std::nullopt, 2}));
    // CRUN-2: `end` is exclusive.
    const auto bounded =
        hglrt::testing::evaluate<i64>(entry_of<Counted>(Dense<i64>{7, 7, 7, 7}), {.length = 2us});
    CHECK(hglrt::testing::same(bounded, Dense<i64>{1, 2}));
    // A run whose end is its start runs no cycle, and records nothing.
    const auto empty = hglrt::testing::evaluate<i64>(entry_of<Counted>(Dense<i64>{7}), {.length = 0us});
    CHECK(empty.entries.empty());
}

TEST_CASE("a failure of a run escapes the eval, with its code (CEVAL-6, ERR-8)") {
    using hglrt_test::Doubled;
    using hglrt_test::entry_of;
    const i64 large = std::numeric_limits<i64>::max() / 2 + 1;
    hglrt::testing::raises("numeric.out_of_range", [&] {
        (void)hglrt::testing::evaluate<i64>(entry_of<Doubled>(Dense<i64>{1, large}));
    });
    try {
        (void)hglrt::testing::evaluate<i64>(entry_of<Doubled>(Dense<i64>{1, large}));
        FAIL("the run did not fail");
    } catch (const std::exception& failure) {
        const std::optional<hglrt::error> carried = hglrt::carried_error(failure);
        REQUIRE(carried.has_value());
        REQUIRE(carried->context.has_value());
        CHECK(carried->context->phase == "evaluate");
        CHECK(carried->context->node.starts_with("test::doubled"));
        CHECK(carried->context->evaluation_time == start + 1us);
    }
    // A run that records nothing under "run.output" fails the eval.
    const hglrt::testing::Entry unrecorded = [](hgraph::Wiring& wiring, datetime) {
        (void)hglrt::constant(wiring, i64{1});
    };
    CHECK(code_raised([&] { (void)hglrt::testing::evaluate<i64>(unrecorded); }) == "(no code)");
}

TEST_CASE("a run in real time starts at the host clock's present (CRUN-2, RUN-3)") {
    const datetime before = std::chrono::floor<hglrt::duration>(hgraph::engine_clock::now());
    const auto [present, none] = hglrt::testing::resolved({}, Mode::real_time);
    CHECK(present >= before);
    CHECK_FALSE(none.has_value());
    CHECK(hglrt::testing::resolved({.length = 1s}, Mode::real_time).second.has_value());
    // A start given is kept.
    CHECK(hglrt::testing::resolved({.start = start + 1s}, Mode::real_time).first == start + 1s);
}
