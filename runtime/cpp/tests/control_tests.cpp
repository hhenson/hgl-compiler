// Unit tests of the branches a graph's `if` and `switch` over a connection
// wire beside their own (control.h), each a child graph of hgraph's
// `compiled::switch_`, as the compiler writes them.
#include "core_nodes.h"

#include <doctest/doctest.h>

#include <array>
#include <chrono>
#include <optional>
#include <string>
#include <utility>
#include <vector>

namespace {

using hglrt::i64;
using hglrt::testing::Dense;
using hglrt::testing::Recorded;
using hgraph::Port;
using hgraph::TS;
using namespace std::chrono_literals;

using hglrt_test::start;

/// The branch of `if selected { counted(value) }` while it is selected.
struct Counting {
    static Port<TS<i64>> compose(hgraph::Wiring& wiring, Port<TS<i64>> value) {
        return hgraph::wire<hglrt_test::Counted>(wiring, value);
    }
};

/// The branch of no `else` (CTRL-3).
struct Silent {
    static Port<TS<i64>> compose(hgraph::Wiring& wiring, [[maybe_unused]] Port<TS<i64>> value) {
        return hglrt::never<TS<i64>>(wiring);
    }
};

/// The branch of no `default`, given the selector first.
struct Failing {
    static Port<TS<i64>> compose(hgraph::Wiring& wiring, Port<TS<i64>> key,
                                 [[maybe_unused]] Port<TS<i64>> value) {
        return hglrt::unmatched<i64, TS<i64>>(wiring, key);
    }
};

/// `switch key { case 1: counted(value) ... }` over the replays of `keys`
/// and `values`, with `other` the branch of the key 2 and `fallback` the
/// `switch`'s default, recorded under the run's key.
template <class Other>
hglrt::testing::Entry switched(Dense<i64> keys, Dense<i64> values,
                               std::optional<hgraph::SwitchBranchGraph> (*fallback)(hgraph::Wiring&)) {
    return [keys, values, fallback](hgraph::Wiring& wiring, hglrt::datetime from) {
        auto key = hgraph::wire<hglrt_test::Replay>(wiring, hglrt::testing::replay_items(keys, from));
        auto value = hgraph::wire<hglrt_test::Replay>(wiring, hglrt::testing::replay_items(values, from));
        std::vector<hgraph::SwitchBranchGraph> branches;
        branches.push_back(
            hgraph::SwitchBranchGraph{hgraph::Value{i64{1}}, hgraph::compile_subgraph<Counting>(wiring)});
        branches.push_back(
            hgraph::SwitchBranchGraph{hgraph::Value{i64{2}}, hgraph::compile_subgraph<Other>(wiring)});
        const std::array<hgraph::WiringPortRef, 1> inputs{value.erased()};
        auto result = Port<TS<i64>>{wiring, hgraph::compiled::switch_(wiring, key.erased(), inputs,
                                                                      std::move(branches), fallback(wiring))};
        hgraph::wire<hglrt_test::Record>(wiring, result, hglrt::str{hglrt::testing::run_output});
    };
}

std::optional<hgraph::SwitchBranchGraph> no_default(hgraph::Wiring&) {
    return std::nullopt;
}

std::optional<hgraph::SwitchBranchGraph> failing_default(hgraph::Wiring& wiring) {
    return hgraph::SwitchBranchGraph{{}, hgraph::compile_subgraph<Failing>(wiring), true};
}

} // namespace

TEST_CASE("the branch of no `else` never ticks, and a branch selected again starts fresh (CTRL-2, CTRL-3)") {
    const auto recording = hglrt::testing::evaluate<i64>(
        switched<Silent>(Dense<i64>{1, 2, std::nullopt, 1}, Dense<i64>{10, 20, 30, 40}, &no_default));
    CHECK(recording.entries == Recorded<i64>{{start, 1}, {start + 3us, 1}});
}

TEST_CASE("a selector no case has fails, with no code, once its branch is selected (CTRL-6)") {
    // The selector 3 has no case: the run fails in its cycle, and what
    // was published before stands.
    try {
        (void)hglrt::testing::evaluate<i64>(
            switched<Counting>(Dense<i64>{1, 3}, Dense<i64>{10, 20}, &failing_default));
        FAIL("the run did not fail");
    } catch (const std::exception& failure) {
        const std::optional<hglrt::error> carried = hglrt::carried_error(failure);
        REQUIRE(carried.has_value());
        CHECK_FALSE(carried->code.has_value());
        CHECK(carried->message == "no case of the `switch` has the value 3");
    }
    // A selector each case has never selects it.
    const auto recording = hglrt::testing::evaluate<i64>(
        switched<Counting>(Dense<i64>{1, 2, 1}, Dense<i64>{10, 20, 30}, &failing_default));
    CHECK(recording.entries == Recorded<i64>{{start, 1}, {start + 1us, 1}, {start + 2us, 1}});
}
