// Unit tests of a window's reads (windows.h; the language's TYPE-11, core's
// END-9), on nodes written by hand as the compiler writes them, with hgraph's
// front end for generated code alone, each run in an hgraph graph in
// simulation: a node publishes arrivals to a window, and a reader that the
// window alone wakes keeps what it saw in a list of the test's.
#include "core_nodes.h"

#include <hglrt/windows.h>

#include <doctest/doctest.h>
#include <hgraph/types/generated.h>

#include <chrono>
#include <optional>
#include <string>
#include <vector>

namespace {

using hglrt::datetime;
using hglrt::i64;
using hglrt::list;
using hglrt::testing::Dense;
using namespace std::chrono_literals;

using hglrt_test::start;

/// `fn collect(reading: i64) -> rolling_window<i64, ...> { when { return
/// reading } }`: each reading arrives at the window `W`.
template <class W>
struct Collect {
    static constexpr auto name = "test::collect";

    static void eval(hgraph::In<"reading", hgraph::TS<i64>> reading, hgraph::Out<W> out) {
        hglrt::window_push(out, hglrt::checked_value(reading));
    }
};

/// What a reader saw of a window in one evaluation: the evaluation's time;
/// `valid`, `modified` and `all_valid`; its delta, where it had one; what
/// `removed_value` gave, where a value left; its value read whole, where it
/// was all valid; and `last_modified`.
struct Seen {
    datetime at;
    bool valid;
    bool modified;
    bool all_valid;
    std::optional<i64> delta;
    std::optional<i64> removed;
    std::optional<list<i64>> held;
    std::optional<datetime> last_modified;

    friend bool operator==(const Seen&, const Seen&) = default;
};

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

std::vector<Seen> window_seen;
std::vector<Seen> staged_seen;
std::optional<list<i64>> stopped_window;

/// What `window` shows at `at`. Each read that takes its form from its place
/// is read in both forms (ERR-14), which agree: `ok` of the value or an
/// `err` coded `value.unset_read`, and the value or a failure of that code.
template <class E>
Seen seen_of(datetime at, const E& window) {
    std::optional<i64> delta;
    if (const hglrt::result<i64> checked = hglrt::fn_delta_value_checked(window); checked.has_value()) {
        delta = *checked;
        CHECK(hglrt::fn_delta_value(window) == *checked);
    } else {
        CHECK(checked.error().code == "value.unset_read");
        CHECK(code_raised([&] { (void)hglrt::fn_delta_value(window); }) == "value.unset_read");
    }
    std::optional<i64> removed;
    if (const hglrt::result<i64> left = hglrt::fn_removed_value_checked(window); left.has_value()) {
        removed = *left;
    } else {
        CHECK(left.error().code == "value.unset_read");
    }
    std::optional<list<i64>> held;
    if (const hglrt::result<list<i64>> whole = hglrt::window_value_checked(window); whole.has_value()) {
        held = *whole;
        CHECK(hglrt::window_value(window) == *whole);
    } else {
        CHECK(whole.error().code == "value.unset_read");
        CHECK(code_raised([&] { (void)hglrt::window_value(window); }) == "value.unset_read");
    }
    return Seen{at,
                hglrt::fn_valid(window),
                hglrt::fn_modified(window),
                hglrt::fn_all_valid(window),
                delta,
                removed,
                held,
                hglrt::fn_last_modified(window)};
}

/// `fn watch(readings: rolling_window<i64, ...>) { when modified(readings) {
/// ... } }`: woken by the window alone.
template <class W>
struct Watch {
    static constexpr auto name = "test::watch_window";

    static void eval(hgraph::In<"readings", W, hgraph::InputValidity::Unchecked> readings,
                     hgraph::DateTime now) {
        window_seen.push_back(seen_of(now, readings));
    }
};

/// `fn twice(reading: i64) -> rolling_window<i64, ...> { inject out  when {
/// out = reading  out = reading + 10 } }`: two writes of its window in one
/// evaluation, held and made once, as the compiler writes them.
template <class W>
struct Twice {
    static constexpr auto name = "test::twice";

    static void eval(hgraph::In<"reading", hgraph::TS<i64>> reading, hgraph::Out<W> out) {
        hglrt::WindowArrival<hgraph::Out<W>> arrival{out};
        const auto now = out.evaluation_time();
        const auto before = seen_of(now, out);
        const i64 first = hglrt::checked_value(reading);
        const i64 last = hglrt::add__i64_i64(first, 10);
        arrival.run([&] {
            CHECK(seen_of(now, arrival.observe()) == before);
            arrival.hold(first);
            auto expected = seen_of(now, arrival.observe());
            CHECK(expected.delta == first);
            CHECK(seen_of(now, out) == before);

            arrival.hold(last);
            const auto staged = seen_of(now, arrival.observe());
            expected.delta = last;
            if (expected.held) {
                expected.held->back() = last;
            }
            CHECK(staged == expected);
            CHECK(seen_of(now, arrival.observe()) == staged);
            CHECK(seen_of(now, out) == before);
            staged_seen.push_back(staged);
        });
    }

    static void stop(hgraph::Out<W> out) {
        if (const auto read = hglrt::window_value_checked(out); read.has_value()) {
            stopped_window = *read;
        }
    }
};

/// A node that writes its window and then fails, and takes the failure
/// itself, as a `capture` around it takes it (ERR-8).
template <class W>
struct WrittenThenFailed {
    static constexpr auto name = "test::written_then_failed";

    static void eval(hgraph::In<"reading", hgraph::TS<i64>> reading, hgraph::Out<W> out) {
        hglrt::WindowArrival<hgraph::Out<W>> arrival{out};
        try {
            arrival.run([&] {
                arrival.hold(hglrt::checked_value(reading));
                hglrt::fail_uncoded("a failure after the write");
            });
        } catch (const hglrt::Failure&) {
        }
    }
};

/// Runs `reading` into a window `W` that the node `Writer` writes and
/// `Watch` reads.
template <class W, template <class> class Writer = Collect>
void watched(const Dense<i64>& reading) {
    window_seen.clear();
    staged_seen.clear();
    stopped_window.reset();
    (void)hglrt::testing::run([reading](hgraph::Wiring& wiring, datetime) {
        auto replayed =
            hgraph::wire<hglrt_test::Replay>(wiring, hglrt::testing::replay_items(reading, start));
        hgraph::wire<Watch<W>>(wiring, hgraph::wire<Writer<W>>(wiring, replayed));
    });
}

TEST_CASE("a window of a tick count (TYPE-11, END-9)") {
    watched<hgraph::TSW<i64, 2, 2>>(Dense<i64>{1, 2, 3});
    // END-9: "`valid(x)` of one returns true once it holds a value, and
    // `all_valid(x)` where it holds at least its minimum ... A window of a
    // tick count drops its oldest value only at an arrival"; its delta is
    // the value that arrived, read before it is all valid as after;
    // `removed_value` the oldest an arrival pushes out; and its value is
    // read only where it is all valid (TYPE-11).
    CHECK(window_seen == std::vector<Seen>{
                             {start, true, true, false, 1, std::nullopt, std::nullopt, start},
                             {start + 1us, true, true, true, 2, std::nullopt, list<i64>{1, 2}, start + 1us},
                             {start + 2us, true, true, true, 3, 1, list<i64>{2, 3}, start + 2us},
                         });
}

TEST_CASE("a window of a duration, whose values leave it by age (TYPE-11, END-9)") {
    watched<hgraph::TSWDuration<i64, 3, 1>>(Dense<i64>{1, 2, std::nullopt, std::nullopt, std::nullopt, 3});
    // END-9: "a value is still held when its age is the maximum, and leaves
    // one smallest step later ... whether or not a value arrives then. The
    // window ticks for that exit ... and an exit and an arrival in one cycle
    // are one tick": in a cycle in which a value only leaves it has no
    // delta, and `removed_value` is the value that left. Proposed in END-9,
    // and built: "where a value leaves and none arrives, `last_modified(x)`
    // is the exit's time". "A window emptied by exits stays valid".
    CHECK(window_seen == std::vector<Seen>{
                             {start, true, true, false, 1, std::nullopt, std::nullopt, start},
                             {start + 1us, true, true, true, 2, std::nullopt, list<i64>{1, 2}, start + 1us},
                             {start + 4us, true, true, false, std::nullopt, 1, std::nullopt, start + 4us},
                             {start + 5us, true, true, false, 3, 2, std::nullopt, start + 5us},
                             {start + 9us, true, true, false, std::nullopt, 3, std::nullopt, start + 9us},
                         });
}

TEST_CASE("a window of a duration with a minimum of zero (END-9)") {
    watched<hgraph::TSWDuration<i64, 3, 0>>(Dense<i64>{1, 2});
    // END-9: "A minimum of zero is held by the first value, and an empty
    // window is not all valid".
    CHECK(window_seen == std::vector<Seen>{
                             {start, true, true, true, 1, std::nullopt, list<i64>{1}, start},
                             {start + 1us, true, true, true, 2, std::nullopt, list<i64>{1, 2}, start + 1us},
                             {start + 4us, true, true, true, std::nullopt, 1, list<i64>{2}, start + 4us},
                             {start + 5us, true, true, false, std::nullopt, 2, std::nullopt, start + 5us},
                         });
}

TEST_CASE("two writes of a window in one evaluation are its one arrival (END-5, TYPE-11)") {
    watched<hgraph::TSW<i64, 2, 2>, Twice>(Dense<i64>{1, 2, 3});
    // END-5: "Further writes to the already modified endpoint in the same
    // evaluation update its publication without notifying again"; TYPE-11:
    // "A window takes at most one value a cycle": each cycle's arrival is the
    // last value written, and the third pushes out one value, the oldest.
    CHECK(window_seen ==
          std::vector<Seen>{
              {start, true, true, false, 11, std::nullopt, std::nullopt, start},
              {start + 1us, true, true, true, 12, std::nullopt, list<i64>{11, 12}, start + 1us},
              {start + 2us, true, true, true, 13, 11, list<i64>{12, 13}, start + 2us},
          });
    // TIME-7: each observation sees the staged arrival. Other nodes
    // and stop see only the arrival finally published.
    CHECK(staged_seen == window_seen);
    CHECK(stopped_window == list<i64>{12, 13});
}

TEST_CASE("staged duration window reads preserve the cycle's expiry (TIME-7, END-9)") {
    watched<hgraph::TSWDuration<i64, 3, 1>, Twice>(
        Dense<i64>{1, 2, std::nullopt, std::nullopt, std::nullopt, 3});
    std::vector<Seen> published;
    for (const auto& seen : window_seen) {
        if (seen.delta.has_value()) {
            published.push_back(seen);
        }
    }
    REQUIRE(published.size() == 3);
    CHECK(staged_seen == published);
    CHECK(staged_seen.back().at == start + 5us);
    CHECK(staged_seen.back().removed == 12);
}

TEST_CASE("a window written before a failure holds the arrival (ERR-8)") {
    watched<hgraph::TSW<i64, 2, 1>, WrittenThenFailed>(Dense<i64>{1, 2});
    // ERR-8: "Earlier writes remain visible".
    CHECK(window_seen == std::vector<Seen>{
                             {start, true, true, true, 1, std::nullopt, list<i64>{1}, start},
                             {start + 1us, true, true, true, 2, std::nullopt, list<i64>{1, 2}, start + 1us},
                         });
}

} // namespace
