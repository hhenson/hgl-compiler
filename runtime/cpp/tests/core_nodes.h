// Core's `replay` and `record` at `i64`, and two nodes of a test program,
// written by hand as the compiler is to write them from their HGL, so that
// the unit tests can run what an eval runs:
//
//     fn replay<T>(const items: list<tuple<datetime, delta<T>>>) -> T {
//         for item in items { yield item[0]: item[1] }
//     }
//
//     fn record<T>(input: T, const key: str) {
//         cache entries: list<tuple<datetime, delta<T>>> = []
//         inject clock, global_state
//         when valid(input) && modified(input) {
//             append(entries, (clock.evaluation_time, delta_value(input)))
//         }
//         stop { put(global_state, key, entries) }
//     }
#pragma once

#include <hglrt/hglrt.h>

#include <hgraph/runtime/evaluation_clock.h>
#include <hgraph/runtime/node_scheduler.h>

#include <cstddef>
#include <optional>
#include <string_view>
#include <tuple>

namespace hglrt_test {

using hglrt::i64;
using hglrt::list;
using hglrt::testing::Recorded;

/// The state of `replay` at `i64`: where its `for` is, and its source.
struct ReplayState {
    static constexpr std::string_view hgl_name = "test::replay.state";
    std::size_t position = 0;
    hglrt::Source<i64> source{};
};

/// `replay<i64>`: a source whose body is a step that resumes after its last
/// `yield`.
struct Replay {
    static constexpr auto name = "core::replay";
    static constexpr bool schedule_on_start = true;

    static void start(hgraph::State<ReplayState> state) { state.set(ReplayState{}); }

    static void eval(hgraph::Scalar<"items", Recorded<i64>> items, hgraph::State<ReplayState> state,
                     hgraph::DateTime now, hgraph::NodeScheduler scheduler,
                     hgraph::Out<hgraph::TS<i64>> out) {
        ReplayState& kept = state.modify();
        const Recorded<i64>& all = items.value();
        hglrt::resume(
            kept.source, now, scheduler,
            [&]() -> std::optional<hglrt::Yielded<i64>> {
                if (kept.position >= all.size()) {
                    return std::nullopt;
                }
                const auto& [at, value] = all[kept.position++];
                return hglrt::Yielded<i64>{at, value};
            },
            [&](i64 value) { out.set(value); });
    }
};

/// The cache of `record` at `i64`.
struct RecordState {
    static constexpr std::string_view hgl_name = "test::record.state";
    Recorded<i64> entries{};
};

/// `record<i64>`.
struct Record {
    static constexpr auto name = "core::record";

    static void start(hgraph::State<RecordState> state) { state.set(RecordState{}); }

    static void eval(hgraph::In<"input", hgraph::TS<i64>> input, hgraph::Scalar<"key", hglrt::str>,
                     hgraph::State<RecordState> state, hgraph::EvaluationClockView clock) {
        if (input.valid() && input.modified()) {
            hglrt::fn_append(state.modify().entries, std::tuple{clock.evaluation_time(), input.value()});
        }
    }

    static void stop(hgraph::Scalar<"key", hglrt::str> key, hgraph::State<RecordState> state,
                     hgraph::GlobalStateView global_state) {
        hglrt::put_GlobalState_str_V(global_state, key.value(), state.ref().entries);
    }
};

/// `fn counted(value: i64) -> i64 { state count: i64 = 0; when { count += 1; return count } }`
struct Counted {
    static constexpr auto name = "test::counted";

    static void start(hgraph::State<i64> state) { state.set(i64{0}); }

    static void eval(hgraph::In<"value", hgraph::TS<i64>>, hgraph::State<i64> state,
                     hgraph::Out<hgraph::TS<i64>> out) {
        i64& count = state.modify();
        count = hglrt::add__i64_i64(count, 1);
        out.set(count);
    }
};

/// `fn doubled(value: i64) -> i64 { when { return value * 2 } }`: fails
/// where the product is out of range.
struct Doubled {
    static constexpr auto name = "test::doubled";

    static void eval(hgraph::In<"value", hgraph::TS<i64>> value, hgraph::Out<hgraph::TS<i64>> out) {
        out.set(hglrt::mul__i64_i64(value.value(), 2));
    }
};

/// The entry of `eval(target(value: trace))`, for a node `Target` of one
/// `i64` input and an `i64` result: `replay` of the trace, the target, and
/// `record` under `"run.output"` (CEVAL-1).
template <class Target, class Trace>
hglrt::testing::Entry entry_of(Trace trace) {
    return [trace](hgraph::Wiring& wiring, hglrt::datetime start) {
        auto replayed = hgraph::wire<Replay>(wiring, hglrt::testing::replay_items(trace, start));
        auto result = hgraph::wire<Target>(wiring, replayed);
        hgraph::wire<Record>(wiring, result, hglrt::str{hglrt::testing::run_output});
    };
}

} // namespace hglrt_test

/// The states are registered with hgraph by the names they give.
template <>
struct hgraph::scalar_descriptor<hglrt_test::ReplayState> {
    [[nodiscard]] static constexpr bool is_concrete() noexcept { return true; }
    [[nodiscard]] static const ValueTypeMetaData* value_meta() {
        return TypeRegistry::instance().register_scalar<hglrt_test::ReplayState>(
            hglrt_test::ReplayState::hgl_name);
    }
};

template <>
struct hgraph::scalar_descriptor<hglrt_test::RecordState> {
    [[nodiscard]] static constexpr bool is_concrete() noexcept { return true; }
    [[nodiscard]] static const ValueTypeMetaData* value_meta() {
        return TypeRegistry::instance().register_scalar<hglrt_test::RecordState>(
            hglrt_test::RecordState::hgl_name);
    }
};
