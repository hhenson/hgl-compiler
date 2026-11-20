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

const datetime start = hglrt::testing::earliest_start();

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

/// Runs `reading` into a window `W` that `Watch` reads.
template <class W>
void watched(const Dense<i64>& reading) {
    window_seen.clear();
    (void)hglrt::testing::run([reading](hgraph::Wiring& wiring, datetime) {
        auto replayed =
            hgraph::wire<hglrt_test::Replay>(wiring, hglrt::testing::replay_items(reading, start));
        hgraph::wire<Watch<W>>(wiring, hgraph::wire<Collect<W>>(wiring, replayed));
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

} // namespace
