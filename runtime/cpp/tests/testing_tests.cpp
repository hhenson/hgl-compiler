// Unit tests of the test host (testing.h): traces, `assert raises`, runs,
// and the results it writes for the runner.
#include "core_nodes.h"

#include <doctest/doctest.h>

#include <chrono>
#include <cstdint>
#include <filesystem>
#include <fstream>
#include <limits>
#include <optional>
#include <source_location>
#include <string>
#include <vector>

namespace {

using hglrt::datetime;
using hglrt::i64;
using hglrt::testing::Dense;
using hglrt::testing::Recorded;
using hglrt::testing::Recording;
using hglrt::testing::Timed;
using namespace std::chrono_literals;

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

// Tests registered with the host, as the compiler registers a module's.

void passes() {
    hglrt::testing::statement();
    hglrt::testing::check(true);
}

std::uint_least32_t failing_line = 0;

void fails_an_assertion() {
    hglrt::testing::statement();
    hglrt::testing::check(true);
    failing_line = std::source_location::current().line() + 1;
    hglrt::testing::statement();
    hglrt::testing::check(false);
}

void raises_out_of_range() {
    hglrt::testing::statement();
    (void)hglrt::add__i64_i64(std::numeric_limits<i64>::max(), 1);
}

const hglrt::testing::Registration registered_passes{"host.cases::passes", &passes};
const hglrt::testing::Registration registered_fails{"host.cases::fails_an_assertion", &fails_an_assertion};
const hglrt::testing::Registration registered_raises{"host.cases::raises_out_of_range", &raises_out_of_range};

} // namespace

TEST_CASE("a dense trace's cells are publications a smallest step apart (TST-4, TST-6)") {
    const Recording<i64> recorded{start, {{start, 7}, {start + 2us, 9}}};
    CHECK(hglrt::testing::same(recorded, Dense<i64>{7, std::nullopt, 9}));
    // "Trailing placeholders have no effect".
    CHECK(hglrt::testing::same(recorded, Dense<i64>{7, std::nullopt, 9, std::nullopt}));
    CHECK_FALSE(hglrt::testing::same(recorded, Dense<i64>{7, 9}));
    CHECK_FALSE(hglrt::testing::same(recorded, Dense<i64>{7, std::nullopt, 8}));
    CHECK_FALSE(hglrt::testing::same(recorded, Dense<i64>{7}));
    CHECK(hglrt::testing::same(Recording<i64>{start, {}}, Dense<i64>{}));
    CHECK(hglrt::testing::same(Recording<i64>{start, {}}, Dense<i64>{std::nullopt}));
    // Equal by the language's equality: a NaN is equal to nothing.
    const hglrt::f64 not_a_number = std::numeric_limits<hglrt::f64>::quiet_NaN();
    CHECK_FALSE(hglrt::testing::same(Recording<hglrt::f64>{start, {{start, not_a_number}}},
                                     Dense<hglrt::f64>{not_a_number}));
    // A published null is a cell, `_` is none.
    using Maybe = std::optional<i64>;
    const Recording<Maybe> nulls{start, {{start, std::nullopt}, {start + 1us, 3}}};
    CHECK(hglrt::testing::same(nulls, Dense<Maybe>{Maybe{}, Maybe{3}}));
    CHECK_FALSE(hglrt::testing::same(nulls, Dense<Maybe>{std::nullopt, Maybe{3}}));
}

TEST_CASE("a timed trace's keys are durations from the start or instants (TST-7)") {
    const Recording<i64> recorded{start, {{start, 7}, {start + 2s, 9}}};
    CHECK(hglrt::testing::same(recorded, Timed<i64>{{{0s, 7}, {2s, 9}}}));
    CHECK(hglrt::testing::same(recorded, Timed<i64>{{{start, 7}, {start + 2s, 9}}}));
    CHECK(hglrt::testing::same(Timed<i64>{{{0s, 7}, {2s, 9}}}, recorded));
    CHECK_FALSE(hglrt::testing::same(recorded, Timed<i64>{{{0s, 7}, {3s, 9}}}));
    CHECK(hglrt::testing::same(Timed<i64>{{{0us, 7}, {2us, 9}}}, Dense<i64>{7, std::nullopt, 9}));
    // TST-7: keys strictly increase; one known only as the program runs
    // fails with no code.
    CHECK(code_raised([&] { (void)hglrt::testing::same(recorded, Timed<i64>{{{2s, 7}, {2s, 9}}}); }) ==
          "(no code)");
}

TEST_CASE("a trace replayed to an input is its publications' instants (CEVAL-2)") {
    CHECK(hglrt::testing::replay_items(Dense<i64>{7, std::nullopt, 7}, start) ==
          Recorded<i64>{{start, 7}, {start + 2us, 7}});
    CHECK(hglrt::testing::replay_items(Dense<i64>{std::nullopt, std::nullopt}, start).empty());
    CHECK(hglrt::testing::replay_items(Timed<i64>{{{0s, 1}, {start + 1s, 2}}}, start) ==
          Recorded<i64>{{start, 1}, {start + 1s, 2}});
    // "an input replay cannot publish before its run starts".
    CHECK(code_raised([&] { (void)hglrt::testing::replay_items(Timed<i64>{{{start - 1us, 1}}}, start); }) ==
          "(no code)");
}

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
        CHECK(std::string{failure.what()}.starts_with(
            "the block raised convert.invalid: not a number, raised at "));
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

TEST_CASE("an eval runs its entry in simulation and reads its recording (CEVAL-1, CEVAL-4)") {
    using hglrt_test::Counted;
    using hglrt_test::entry_of;
    // core's conformance case replay_preserves_equal_ticks.
    const auto counted =
        hglrt::testing::evaluate<i64>(entry_of<Counted>(Dense<i64>{7, std::nullopt, 7, 7, std::nullopt}));
    CHECK(hglrt::testing::same(counted, Dense<i64>{1, std::nullopt, 2, 3}));
    // runs_are_independent: each run has fresh state and a fresh recording.
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

TEST_CASE("the host runs the tests named and writes what became of each") {
    // In the directory the tests run in: their build's.
    const std::filesystem::path results = "hglrt-host-results.txt";
    std::string program = "tests";
    std::string file = results.string();
    std::vector<std::string> names{"host.cases::passes", "host.cases::fails_an_assertion",
                                   "host.cases::raises_out_of_range", "host.cases::absent"};
    std::vector<char*> argv{program.data(), file.data()};
    for (std::string& name : names) {
        argv.push_back(name.data());
    }
    REQUIRE(hglrt::testing::main(static_cast<int>(argv.size()), argv.data()) == 0);

    std::ifstream written(results);
    std::vector<std::string> lines;
    for (std::string line; std::getline(written, line);) {
        lines.push_back(line);
    }
    std::filesystem::remove(results);
    REQUIRE(lines.size() == 8);
    CHECK(lines[0] == "start 0");
    CHECK(lines[1] == "passed 0");
    CHECK(lines[2] == "start 1");
    CHECK(lines[3] == "failed 1 " + std::to_string(failing_line) + " the assertion does not hold");
    CHECK(lines[4] == "start 2");
    CHECK(lines[5].starts_with("failed 2 "));
    CHECK(lines[5].find(
              " numeric.out_of_range: an i64 result is outside the signed 64-bit range, raised at ") !=
          std::string::npos);
    CHECK(lines[6] == "start 3");
    CHECK(lines[7] == "unloaded 3 the program has no test `host.cases::absent`");

    // A command line with no results file is refused.
    std::vector<char*> bare{program.data()};
    CHECK(hglrt::testing::main(1, bare.data()) == 2);
}
