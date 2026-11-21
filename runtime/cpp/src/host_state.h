// What the host of a test executable (host.cpp) and the runs of its tests
// (testing.cpp) share: the line of the statement of the test running, and
// the stop of a test's runs once its time limit has passed (TST-17); and what
// the host of an application's executable (application.cpp) shares with
// them: the stop of its run, as its host stops a run in real time (RUN-3),
// and how a failure and a results line are written. Private to libhglrt.
#pragma once

#include <atomic>
#include <cstdio>
#include <cstdint>
#include <exception>
#include <string>
#include <string_view>

namespace hglrt::testing::detail {

/// The line of the last statement marked in the running test (`statement`),
/// 0 before the first. Tests run one at a time; the host reads it from
/// another thread where the test is held to a time limit.
extern std::atomic<std::uint_least32_t> running_line;

/// Has each run made from here on watched, so that `stop_runs` stops it
/// even as it starts, or not: the host watches the runs of a test held to a
/// time limit, and no other's, as a run watched has hgraph call its
/// observer at each node's evaluation.
void runs_watched(bool watched) noexcept;

/// Asks the run under way to stop, and each run started after it until
/// `stops_withdrawn`, as its host stops a run in real time: each then ends
/// as it does at its end.
void stop_runs() noexcept;

/// Withdraws `stop_runs`, once the test whose runs it stopped has ended.
void stops_withdrawn() noexcept;

/// What became of a test that raised `failure`, as a person reads it
/// (TST-14): an assertion's failure as it says, and any other failure as
/// `describe` gives it.
std::string described(const std::exception_ptr& failure);

/// `text` as one line of a results file: a backslash, a line feed and a
/// carriage return as `\\`, `\n` and `\r`, as the runner reads them, and every
/// other character as it is, a NUL and the other control characters among
/// them: the runner ends a line at a line feed alone. The test host's and an
/// application's host's (application.cpp) results are written so.
std::string escaped(std::string_view text);

/// Writes a full line, including any NUL in it, and flushes the result.
inline bool write_results_line(std::FILE* results, std::string_view line) {
    return std::fwrite(line.data(), 1, line.size(), results) == line.size() &&
           std::fputc('\n', results) != EOF && std::fflush(results) == 0;
}

} // namespace hglrt::testing::detail
