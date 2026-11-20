// Unit tests of what a `ref` parameter designates (references.h), on nodes
// written by hand as the compiler writes them from their HGL, with hgraph's
// front end for generated code alone, each run in an hgraph graph in
// simulation. As core's END-10 has it, they check what an ordinary reader
// observes and what a node sees of its `ref` parameter, and nothing of how
// a reference is carried between them.
#include "core_nodes.h"

#include <hglrt/references.h>

#include <doctest/doctest.h>
#include <hgraph/types/generated.h>

#include <chrono>
#include <optional>
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

/// What a node saw of its `ref` parameter in one evaluation.
struct Seen {
    datetime at;
    bool valid;
    bool designates;

    friend bool operator==(const Seen&, const Seen&) = default;
};

std::vector<Seen> present_seen;

/// `fn present(probe: i64, ref target: i64) { when modified(probe) { ... } }`:
/// what its `ref` parameter is, in each cycle the probe ticks.
struct Present {
    static constexpr auto name = "test::present";

    static void eval(hgraph::In<"probe", hgraph::TS<i64>> probe,
                     hgraph::In<"target", hgraph::REF<hgraph::TS<i64>>, hgraph::InputActivity::Passive,
                                hgraph::InputValidity::Unchecked>
                         target,
                     hgraph::DateTime now) {
        static_cast<void>(probe);
        present_seen.push_back(Seen{now, hglrt::fn_valid(target), !hglrt::designation(target).is_empty()});
    }
};

TEST_CASE("a `ref` parameter designates the connection it is given, whatever that holds (TIME-10)") {
    present_seen.clear();
    (void)hglrt::testing::run([](hgraph::Wiring& wiring, datetime) {
        hgraph::wire<Present>(wiring, replayed(wiring, Dense<i64>{1, 1, 1}),
                              replayed(wiring, Dense<i64>{std::nullopt, std::nullopt, 7}));
    });
    // TIME-10: "a connection given to a `ref` parameter becomes the
    // designation of that connection's endpoint", and "`valid` of a `ref`
    // parameter is whether a reference is present, whatever the endpoint
    // it designates holds".
    CHECK(present_seen == std::vector<Seen>{
                              {start, true, true},
                              {start + 1us, true, true},
                              {start + 2us, true, true},
                          });
}

/// `fn sample(take: i64, ref target: i64) -> ref i64 { when modified(take) {
/// return target } }`.
struct Sample {
    static constexpr auto name = "test::sample";

    static void eval(hgraph::In<"take", hgraph::TS<i64>> take,
                     hgraph::In<"target", hgraph::REF<hgraph::TS<i64>>, hgraph::InputActivity::Passive,
                                hgraph::InputValidity::Unchecked>
                         target,
                     hgraph::Out<hgraph::REF<hgraph::TS<i64>>> out) {
        static_cast<void>(take);
        out.set(hglrt::designation(target));
    }
};

/// `fn none(take: i64) -> ref i64`, whose result designates nothing in each
/// cycle `take` ticks.
struct None {
    static constexpr auto name = "test::none";

    static void eval(hgraph::In<"take", hgraph::TS<i64>> take,
                     hgraph::Out<hgraph::REF<hgraph::TS<i64>>> out) {
        static_cast<void>(take);
        out.set(hgraph::TimeSeriesReference::empty());
    }
};

TEST_CASE("an ordinary reader of a `ref` result observes the endpoint it designates (FN-16, TIME-10)") {
    const hglrt::testing::Recording<i64> recorded =
        hglrt::testing::evaluate<i64>([](hgraph::Wiring& wiring, datetime) {
            auto designated = replayed(wiring, Dense<i64>{std::nullopt, 5, std::nullopt, 6});
            auto sampled = hgraph::wire<Sample>(wiring, replayed(wiring, Dense<i64>{1}), designated);
            hgraph::wire<hglrt_test::Record>(wiring, sampled.as<hgraph::TS<i64>>(),
                                             hglrt::str{hglrt::testing::run_output});
        });
    // TIME-10: the reader "observes the endpoint designated as if wired to
    // it: it is valid where that endpoint is, and ticks where that endpoint
    // publishes".
    CHECK(hglrt::testing::same(recorded, Dense<i64>{std::nullopt, 5, std::nullopt, 6}));
}

TEST_CASE("a `ref` parameter given a result that designates nothing designates nothing (FN-16)") {
    present_seen.clear();
    (void)hglrt::testing::run([](hgraph::Wiring& wiring, datetime) {
        auto nothing = hgraph::wire<None>(wiring, replayed(wiring, Dense<i64>{1}));
        hgraph::wire<Present>(wiring, replayed(wiring, Dense<i64>{1, 1}), nothing.as<hgraph::TS<i64>>());
    });
    CHECK(present_seen.size() == 2);
    for (const Seen& seen : present_seen) {
        CHECK_FALSE(seen.designates);
    }
}

} // namespace
