// Unit tests of the logger, the clock, the global store and the run's
// configuration (capabilities.h; core's CAP-2, CAP-3, CAP-6, CAP-7), and of
// the list natives (lists.h). The
// scheduler's and the alarm's are in scheduler_tests.cpp. A capability's
// test runs a node written by hand as the compiler writes it, with hgraph's
// front end for generated code alone, in an hgraph graph in simulation.
#include "core_nodes.h"

#include <hglrt/capabilities.h>
#include <hglrt/lists.h>

#include <doctest/doctest.h>
#include <hgraph/types/generated.h>

#include <chrono>
#include <memory>
#include <optional>
#include <spdlog/logger.h>
#include <string>
#include <string_view>
#include <tuple>
#include <utility>
#include <vector>

namespace {

using hglrt::datetime;
using hglrt::i64;
using hglrt::LogLevel;
using namespace std::chrono_literals;

using hglrt_test::start;

// ---- The logger (CAP-2) ------------------------------------------------------

/// The records a run's logger was given: each one's level, on spdlog's
/// scale, and its text.
std::vector<std::pair<spdlog::level::level_enum, std::string>> records;

/// The level each evaluation of `Logs` read of its logger.
std::vector<LogLevel> levels_read;

/// `fn logs() { inject logger; when { ... } }`, woken once at the start.
/// (hgraph prunes a node that no node with no output depends on: each node
/// of these tests has none.)
struct Logs {
    static constexpr auto name = "test::logs";
    static constexpr bool schedule_on_start = true;

    static void eval(hgraph::LoggerView view) {
        const hglrt::Logger logger{view};
        levels_read.push_back(logger.level());
        hglrt::fn_log(logger, LogLevel::debug, "below the level");
        hglrt::fn_log(logger, LogLevel::info, "a {} is text");
        hglrt::fn_log(logger, LogLevel::warning, "warned");
        hglrt::fn_log(logger, LogLevel::error, "failed");
    }
};

/// Runs `Logs` with a logger at `level`, whose records are kept in
/// `records`.
void run_logs(spdlog::level::level_enum level) {
    records.clear();
    levels_read.clear();
    static const hgraph::LoggerOps kept{
        .emit_impl = [](spdlog::logger&, spdlog::level::level_enum at, std::string_view message,
                        hgraph::NodePtr) { records.emplace_back(at, std::string{message}); },
    };
    auto logger = std::make_shared<spdlog::logger>("test");
    logger->set_level(level);
    hgraph::Wiring wiring;
    hgraph::wire<Logs>(wiring);
    hgraph::GraphExecutorBuilder builder;
    builder.graph_builder(std::move(wiring).finish())
        .start_time(start)
        .end_time(start + 1us)
        .logger(std::move(logger), &kept);
    hgraph::GraphExecutorValue executor = builder.make_executor();
    executor.view().run();
}

TEST_CASE("a log record is written at its level, as it is, where the logger's level admits it (CAP-2)") {
    run_logs(spdlog::level::info);
    CHECK(levels_read == std::vector<LogLevel>{LogLevel::info});
    CHECK(records == std::vector<std::pair<spdlog::level::level_enum, std::string>>{
                         {spdlog::level::info, "a {} is text"},
                         {spdlog::level::warn, "warned"},
                         {spdlog::level::err, "failed"},
                     });
    run_logs(spdlog::level::err);
    CHECK(levels_read == std::vector<LogLevel>{LogLevel::error});
    CHECK(records.size() == 1);
}

TEST_CASE("a logger's level is the least a record is written at (CAP-2)") {
    spdlog::logger logger{"levels"};
    const auto level_at = [&](spdlog::level::level_enum level) {
        logger.set_level(level);
        return hglrt::Logger{hgraph::LoggerView{&logger}}.level();
    };
    CHECK(level_at(spdlog::level::trace) == LogLevel::debug);
    CHECK(level_at(spdlog::level::debug) == LogLevel::debug);
    CHECK(level_at(spdlog::level::info) == LogLevel::info);
    CHECK(level_at(spdlog::level::warn) == LogLevel::warning);
    CHECK(level_at(spdlog::level::err) == LogLevel::error);
    CHECK(level_at(spdlog::level::critical) == LogLevel::error);
    CHECK(level_at(spdlog::level::off) == LogLevel::error);
    // A logger that writes nothing.
    CHECK(hglrt::Logger{hgraph::LoggerView{}}.level() == LogLevel::error);
    hglrt::fn_log(hglrt::Logger{hgraph::LoggerView{}}, LogLevel::error, "written nowhere");
}

// ---- The clock (CAP-3) -------------------------------------------------------

/// What a hook read of its clock: `evaluation_time` and
/// `next_cycle_evaluation_time`, and whether `now` was at the evaluation
/// time or after it and `cycle_time` zero or more.
struct Read {
    datetime evaluation_time;
    datetime next_cycle_evaluation_time;
    bool now_since;
    bool cycle_time_since;

    friend bool operator==(const Read&, const Read&) = default;
};

std::vector<Read> clock_reads;

/// What `clock` reads.
Read read_of(const hglrt::Clock& clock) {
    return Read{clock.evaluation_time(), clock.next_cycle_evaluation_time(),
                clock.now() >= clock.evaluation_time(), clock.cycle_time() >= hglrt::duration::zero()};
}

/// `fn stamp(value: i64) { inject clock; start { ... } when { ... }
/// stop { ... } }`, which reads its clock in each hook.
struct Stamp {
    static constexpr auto name = "test::stamp";

    static void start(hgraph::EvaluationClockView view) {
        clock_reads.push_back(read_of(hglrt::Clock{view}));
    }

    static void eval(hgraph::In<"value", hgraph::TS<i64>>, hgraph::EvaluationClockView view) {
        clock_reads.push_back(read_of(hglrt::Clock{view}));
    }

    static void stop(hgraph::EvaluationClockView view) { clock_reads.push_back(read_of(hglrt::Clock{view})); }
};

TEST_CASE("a clock reads the time of the cycle it is read in (CAP-3)") {
    clock_reads.clear();
    (void)hglrt::testing::run(
        [](hgraph::Wiring& wiring, datetime from) {
            auto replayed = hgraph::wire<hglrt_test::Replay>(
                wiring, hglrt::testing::replay_items(hglrt::testing::Dense<i64>{1, std::nullopt, 3}, from));
            hgraph::wire<Stamp>(wiring, replayed);
        },
        {.start = start + 1s});
    // core's the_clock_reports_the_time_of_each_input: in `start` the run's
    // start time, in `stop` the last cycle's.
    const datetime first = start + 1s;
    CHECK(clock_reads == std::vector<Read>{
                             {first, first + 1us, true, true},
                             {first, first + 1us, true, true},
                             {first + 2us, first + 3us, true, true},
                             {first + 2us, first + 3us, true, true},
                         });
}

// ---- The global store (CAP-6) ------------------------------------------------

/// `fn seen() { inject global_state; when { ... } }`, woken once at
/// the start: it reads the store's `"limit"` and puts it under `"seen"`.
struct Seen {
    static constexpr auto name = "test::seen";
    static constexpr bool schedule_on_start = true;

    static void eval(hgraph::GlobalStateView view) {
        const hglrt::GlobalState store{view};
        hglrt::put_GlobalState_str_V(store, "seen", hglrt::get_GlobalState_str<i64>(store, "limit"));
    }
};

TEST_CASE("a graph puts in the store where it is built, and its nodes read it (CAP-6, FN-9)") {
    const hglrt::testing::Run ran = hglrt::testing::run([](hgraph::Wiring& wiring, datetime) {
        // `fn seeded(const limit: i64) -> i64 { inject global_state; put(
        // global_state, "limit", limit); return seen() }`
        hglrt::put_GlobalState_str_V(hglrt::GlobalState{wiring.global_state()}, "limit", i64{2});
        hgraph::wire<Seen>(wiring);
    });
    CHECK(ran.get<i64>("seen") == 2);
    CHECK(ran.get<i64>("limit") == 2);
}

} // namespace

TEST_CASE("the global store keeps a copy of each value under its key (CAP-6)") {
    hgraph::GlobalState state;
    const hglrt::GlobalState store{state.view()};
    CHECK_FALSE(hglrt::has_GlobalState_str(store, "limit"));
    hglrt::put_GlobalState_str_V(store, "limit", hglrt::i64{3});
    CHECK(hglrt::has_GlobalState_str(store, "limit"));
    CHECK(hglrt::get_GlobalState_str<hglrt::i64>(store, "limit") == 3);
    hglrt::put_GlobalState_str_V(store, "limit", hglrt::i64{4});
    CHECK(hglrt::get_GlobalState_str<hglrt::i64>(store, "limit") == 4);

    hglrt::list<std::tuple<hglrt::datetime, hglrt::i64>> entries{{hglrt::datetime{}, 1}};
    hglrt::put_GlobalState_str_V(store, "entries", entries);
    entries.emplace_back(hglrt::datetime{}, 2);
    CHECK(hglrt::get_GlobalState_str<decltype(entries)>(store, "entries").size() == 1);
}

TEST_CASE("an absent key fails with value.missing_key (CAP-6)") {
    hgraph::GlobalState state;
    const hglrt::GlobalState store{state.view()};
    try {
        (void)hglrt::get_GlobalState_str<hglrt::i64>(store, "absent");
        FAIL("nothing was raised");
    } catch (const hglrt::Failure& failure) {
        CHECK(failure.error().code == "value.missing_key");
        CHECK(failure.error().message == "the global store holds nothing under 'absent'");
    }
    hglrt::put_GlobalState_str_V(store, "limit", hglrt::i64{3});
    try {
        (void)hglrt::get_GlobalState_str<hglrt::str>(store, "limit");
        FAIL("nothing was raised");
    } catch (const hglrt::Failure& failure) {
        CHECK(failure.error().code == "store.key_type");
        CHECK(failure.error().message == "the global store holds a value of another type under 'limit'");
    }
    // "A key keeps the type of its first value".
    hglrt::testing::raises("store.key_type", [&] {
        hglrt::put_GlobalState_str_V(store, "limit", hglrt::str{"three"});
    });
    CHECK(hglrt::get_GlobalState_str<hglrt::i64>(store, "limit") == 3);
}

TEST_CASE("a run's configuration reads each field as supplied, null where it is left out (CAP-7)") {
    const datetime opening = start + 5s;
    const hglrt::RunConfig given{hglrt::RunConfig::Fields{.start = opening, .length = 3s}};
    CHECK(given.start() == std::optional<datetime>{opening});
    CHECK_FALSE(given.end().has_value());
    CHECK(given.length() == std::optional<hglrt::duration>{3s});
    const hglrt::RunConfig none{hglrt::RunConfig::Fields{}};
    CHECK_FALSE(none.start().has_value());
    CHECK_FALSE(none.end().has_value());
    CHECK_FALSE(none.length().has_value());
}

TEST_CASE("append and extend grow the place they are given (LIST-1 to LIST-3)") {
    hglrt::list<hglrt::i64> values{1, 2};
    const hglrt::list<hglrt::i64> before = values;
    hglrt::fn_append(values, 3);
    CHECK(values == hglrt::list<hglrt::i64>{1, 2, 3});
    CHECK(before == hglrt::list<hglrt::i64>{1, 2});
    hglrt::fn_extend(values, hglrt::list<hglrt::i64>{});
    CHECK(values.size() == 3);
    hglrt::fn_extend(values, hglrt::list<hglrt::i64>{4, 5});
    CHECK(values == hglrt::list<hglrt::i64>{1, 2, 3, 4, 5});
    hglrt::fn_extend(values, values);
    CHECK(values == hglrt::list<hglrt::i64>{1, 2, 3, 4, 5, 1, 2, 3, 4, 5});
}
