// Unit tests of timing.h: whether an input ticked in a node's evaluation
// (TIME-3, TIME-9), on nodes written by hand with hgraph's front end for
// generated code alone, run in an hgraph graph in simulation; and a
// capability lent to a source's body (SOURCE-1, TIME-8).
#include <hglrt/hglrt.h>

#include <doctest/doctest.h>
#include <hgraph/types/generated.h>

#include <chrono>
#include <stdexcept>
#include <vector>

namespace {

using hglrt::datetime;
using hglrt::i64;
using namespace std::chrono_literals;

const datetime start = hglrt::testing::earliest_start();

/// A source of commands: 7 at the start and 0 a microsecond later.
struct Commands {
    static constexpr auto name = "test::commands";
    static constexpr bool schedule_on_start = true;

    static void eval(hgraph::DateTime now, hgraph::NodeScheduler scheduler,
                     hgraph::Out<hgraph::TS<i64>> out) {
        out.set(now == start ? i64{7} : i64{0});
        if (now == start) {
            scheduler.schedule(start + 1us);
        }
    }
};

/// Publishes each command but 0, and loses its value at a 0.
struct Produce {
    static constexpr auto name = "test::produce";

    static void eval(hgraph::In<"command", hgraph::TS<i64>> command, hgraph::DateTime now,
                     hgraph::Out<hgraph::TS<i64>> out) {
        if (command.value() == 0) {
            static_cast<void>(out.begin_mutation(now).invalidate());
        } else {
            out.set(command.value());
        }
    }
};

/// A source that publishes 1 two microseconds after the start, and nothing
/// before.
struct Later {
    static constexpr auto name = "test::later";
    static constexpr bool schedule_on_start = true;

    static void eval(hgraph::DateTime now, hgraph::NodeScheduler scheduler,
                     hgraph::Out<hgraph::TS<i64>> out) {
        if (now == start) {
            scheduler.schedule(start + 2us);
            return;
        }
        out.set(i64{1});
    }
};

/// What `Watch` saw in one evaluation: its time, and whether each input
/// ticked.
struct Seen {
    datetime at;
    bool input;
    bool other;

    friend bool operator==(const Seen&, const Seen&) = default;
};

std::vector<Seen> seen;

/// Keeps, at each evaluation, whether each of its inputs ticked.
struct Watch {
    static constexpr auto name = "test::watch";

    static void eval(hgraph::In<"input", hgraph::TS<i64>, hgraph::InputValidity::Unchecked> input,
                     hgraph::In<"other", hgraph::TS<i64>, hgraph::InputValidity::Unchecked> other,
                     hgraph::DateTime now) {
        seen.push_back(Seen{now, hglrt::ticked(input), hglrt::ticked(other)});
    }
};

TEST_CASE("an input ticks where it publishes and where it loses its value (TIME-9, END-4)") {
    seen.clear();
    (void)hglrt::testing::run(
        [](hgraph::Wiring& wiring, datetime) {
            hgraph::wire<Watch>(wiring, hgraph::wire<Produce>(wiring, hgraph::wire<Commands>(wiring)),
                                hgraph::wire<Later>(wiring));
        },
        {});
    // At the start the input publishes; a microsecond later it loses its
    // value, which hgraph's own `modified` does not count; at two the other
    // input publishes, and the first does not tick.
    CHECK(seen ==
          std::vector<Seen>{{start, true, false}, {start + 1us, true, false}, {start + 2us, false, true}});
}

TEST_CASE("a capability is lent to a source's body for one resumption (SOURCE-1, TIME-8)") {
    hglrt::Lent<i64> slot;
    CHECK_THROWS_AS((void)slot.get(), std::logic_error);
    {
        const hglrt::Lending<i64> lent{slot, 5};
        CHECK(slot.get() == 5);
    }
    CHECK_THROWS_AS((void)slot.get(), std::logic_error);
    // It is taken back where the resumption fails.
    try {
        const hglrt::Lending<i64> lent{slot, 6};
        CHECK(slot.get() == 6);
        throw std::runtime_error("the body failed");
    } catch (const std::runtime_error&) {
    }
    CHECK_THROWS_AS((void)slot.get(), std::logic_error);
}

} // namespace
