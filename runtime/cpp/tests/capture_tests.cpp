// Unit tests of a capture and of the temporal values of errors and results
// (capture.h), on nodes and child graphs written by hand as the compiler
// writes them, each run in an hgraph graph in simulation. Each observing node
// keeps what it saw in a list of the test's, which the test then reads.
#include "core_nodes.h"

#include <hglrt/capture.h>
#include <hglrt/endpoints.h>

#include <doctest/doctest.h>
#include <hgraph/types/generated.h>

#include <chrono>
#include <optional>
#include <string>
#include <vector>

namespace {

using hglrt::datetime;
using hglrt::i64;
using hglrt::testing::Dense;
using namespace std::chrono_literals;

const datetime start = hglrt::testing::earliest_start();

/// The connection core's `replay` makes of `trace`.
hgraph::Port<hgraph::TS<i64>> replayed(hgraph::Wiring& wiring, const Dense<i64>& trace) {
    return hgraph::wire<hglrt_test::Replay>(wiring, hglrt::testing::replay_items(trace, start));
}

/// `fn attempt(value: i64) -> i64 { state count: i64 = 0; when { count += 1;
/// if value < 0 { fail }; return count } }`: counts its evaluations, and
/// fails on a negative input after it counted it.
struct Attempt {
    static constexpr auto name = "test::attempt";

    static void start(hgraph::State<i64> state) { state.set(i64{0}); }

    static void eval(hgraph::In<"value", hgraph::TS<i64>> value, hgraph::State<i64> state,
                     hgraph::Out<hgraph::TS<i64>> out) {
        i64& count = state.modify();
        count = hglrt::add__i64_i64(count, 1);
        if (value.value() < 0) {
            hglrt::fail(hglrt::codes::out_of_range, "negative input");
        }
        out.set(count);
    }
};

/// The graph the compiler writes for `capture(attempt, value)`.
struct CapturedAttempt {
    static constexpr auto name = "test::capture(attempt)";

    static hgraph::Port<hgraph::TS<i64>> compose(hgraph::Wiring& wiring,
                                                 hgraph::Port<hgraph::TS<i64>> value) {
        return hgraph::wire<Attempt>(wiring, value);
    }
};

/// What an observer saw of a capture in one evaluation.
struct Seen {
    datetime at;
    std::optional<i64> value;
    bool error_valid;
    std::optional<std::string> error_code;

    friend bool operator==(const Seen&, const Seen&) = default;
};

std::vector<Seen> seen;

/// `fn observe(value: i64, failure: error) { when valid([]) { ... } }`: what
/// it is given, whichever woke it.
struct Observe {
    static constexpr auto name = "test::observe";

    static void
    eval(hgraph::In<"value", hgraph::TS<i64>, hgraph::InputValidity::Unchecked> value,
         hgraph::In<"failure", hgraph::TS<hglrt::error>, hgraph::InputValidity::Unchecked> failure,
         hgraph::DateTime now) {
        std::optional<std::string> code;
        if (hglrt::fn_modified(failure)) {
            code = hglrt::checked_value(failure).code.value_or("(no code)");
        }
        seen.push_back(Seen{now, hglrt::fn_modified(value) ? std::optional<i64>{value.value()} : std::nullopt,
                            hglrt::fn_valid(failure), code});
    }
};

TEST_CASE("a capture gives the call's value and ticks its error with each failure (ERR-7, ERR-8)") {
    seen.clear();
    (void)hglrt::testing::run([](hgraph::Wiring& wiring, datetime) {
        const auto boundary =
            hglrt::capture<CapturedAttempt>(wiring, replayed(wiring, Dense<i64>{5, -3, std::nullopt, 7}));
        hgraph::wire<Observe>(wiring, hglrt::captured_value(boundary), hglrt::captured_error(boundary));
    });
    // ERR-7: "before failure the stream is invalid, and later success leaves
    // its retained error valid but unmodified". ERR-8: "The child remains
    // live for future ordinary work": the failing attempt was counted, and
    // the next counts on from it; nothing wakes the child in a cycle with no
    // input.
    CHECK(seen == std::vector<Seen>{
                      {start, 1, false, std::nullopt},
                      {start + 1us, std::nullopt, true, "numeric.out_of_range"},
                      {start + 3us, 3, true, std::nullopt},
                  });
}

std::vector<hglrt::error> errors;

/// Keeps each error it is given.
struct KeepErrors {
    static constexpr auto name = "test::keep_errors";

    static void eval(hgraph::In<"failure", hgraph::TS<hglrt::error>> failure) {
        errors.push_back(hglrt::checked_value(failure));
    }
};

/// `fn check(value: i64) { when { if value == 0 { fail } } }`, a sink.
struct Check {
    static constexpr auto name = "test::check";

    static void eval(hgraph::In<"value", hgraph::TS<i64>> value) {
        if (value.value() == 0) {
            hglrt::fail(hglrt::codes::division_by_zero, "zero");
        }
    }
};

/// The graph the compiler writes for `capture(check, value)`, which gives
/// nothing.
struct CapturedCheck {
    static constexpr auto name = "test::capture(check)";

    static void compose(hgraph::Wiring& wiring, hgraph::Port<hgraph::TS<i64>> value) {
        hgraph::wire<Check>(wiring, value);
    }
};

TEST_CASE("a capture of a call with no result is its error alone, with where it failed (ERR-4, ERR-7)") {
    errors.clear();
    (void)hglrt::testing::run([](hgraph::Wiring& wiring, datetime) {
        const hgraph::Port<hgraph::TS<hglrt::error>> boundary =
            hglrt::capture<CapturedCheck>(wiring, replayed(wiring, Dense<i64>{1, 0, 2, 0}));
        hgraph::wire<KeepErrors>(wiring, hglrt::captured_error(boundary));
    });
    REQUIRE(errors.size() == 2);
    CHECK(errors[0].code == "numeric.division_by_zero");
    // ERR-4: "On first graph failure, attach original node identity, phase,
    // and evaluation time".
    REQUIRE(errors[0].context.has_value());
    CHECK(errors[0].context->node.find("test::check") != std::string::npos);
    CHECK(errors[0].context->phase == "evaluate");
    CHECK(errors[0].context->evaluation_time == start + 1us);
    REQUIRE(errors[1].context.has_value());
    CHECK(errors[1].context->evaluation_time == start + 3us);
}

/// `fn fail_at_start(value: i64) -> i64 { start { fail } when { return value } }`.
struct FailAtStart {
    static constexpr auto name = "test::fail_at_start";

    static void start() { hglrt::fail(hglrt::codes::division_by_zero, "at start"); }

    static void eval(hgraph::In<"value", hgraph::TS<i64>> value, hgraph::Out<hgraph::TS<i64>> out) {
        out.set(value.value());
    }
};

struct CapturedStart {
    static constexpr auto name = "test::capture(fail_at_start)";

    static hgraph::Port<hgraph::TS<i64>> compose(hgraph::Wiring& wiring,
                                                 hgraph::Port<hgraph::TS<i64>> value) {
        return hgraph::wire<FailAtStart>(wiring, value);
    }
};

TEST_CASE("a failure of a start escapes a capture (ERR-8)") {
    std::optional<hglrt::error> escaped;
    try {
        (void)hglrt::testing::run([](hgraph::Wiring& wiring, datetime) {
            const auto boundary = hglrt::capture<CapturedStart>(wiring, replayed(wiring, Dense<i64>{1}));
            hgraph::wire<KeepErrors>(wiring, hglrt::captured_error(boundary));
        });
    } catch (const std::exception& failure) {
        escaped = hglrt::carried_error(failure);
    }
    REQUIRE(escaped.has_value());
    CHECK(escaped->code == "numeric.division_by_zero");
}

std::vector<hglrt::result<i64>> outcomes;

/// `fn outcome(value: i64) -> result<i64, error> { when { ... } }`: an error
/// returned as data, which is no failure.
struct Outcome {
    static constexpr auto name = "test::outcome";

    static void eval(hgraph::In<"value", hgraph::TS<i64>> value,
                     hgraph::Out<hgraph::TS<hglrt::result<i64>>> out) {
        if (value.value() < 0) {
            out.set(hglrt::result<i64>{std::unexpect, hglrt::make_error("negative", "numeric.out_of_range")});
            return;
        }
        out.set(hglrt::result<i64>{value.value()});
    }
};

/// Keeps each result it is given.
struct KeepOutcomes {
    static constexpr auto name = "test::keep_outcomes";

    static void eval(hgraph::In<"outcome", hgraph::TS<hglrt::result<i64>>> outcome) {
        outcomes.push_back(hglrt::checked_value(outcome));
    }
};

TEST_CASE("a result publishes whole, an equal one and an error among them (ERR-2)") {
    outcomes.clear();
    (void)hglrt::testing::run([](hgraph::Wiring& wiring, datetime) {
        hgraph::wire<KeepOutcomes>(wiring,
                                   hgraph::wire<Outcome>(wiring, replayed(wiring, Dense<i64>{1, -1, -1})));
    });
    REQUIRE(outcomes.size() == 3);
    CHECK(outcomes[0] == hglrt::result<i64>{1});
    CHECK_FALSE(outcomes[1].has_value());
    CHECK(outcomes[1] == outcomes[2]);
    CHECK(hgraph::scalar_descriptor<hglrt::result<i64>>::value_meta() != nullptr);
    CHECK(hglrt::detail::held_name<hglrt::result<hglrt::result<i64>>>() ==
          "result<result<i64, error>, error>");
}

} // namespace
