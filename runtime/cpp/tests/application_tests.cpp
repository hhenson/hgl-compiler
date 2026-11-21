// Unit tests of an application's run and its host (application.h,
// application_host.h): the run's store set before its graph is built, its
// output as the host writes it, a failure, and the host's stop of a run in
// simulation and in real time. Each host runs in a process of its own, as an
// application's executable does.
#include "core_nodes.h"

#include <doctest/doctest.h>

#include <chrono>
#include <csignal>
#include <cstdio>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <limits>
#include <optional>
#include <sstream>
#include <string>
#include <sys/wait.h>
#include <thread>
#include <unistd.h>
#include <utility>
#include <vector>

namespace {

using hglrt::datetime;
using hglrt::i64;
using hglrt::application::Output;
using hglrt::testing::Dense;
using hglrt::testing::Mode;
using namespace std::chrono_literals;

using hglrt_test::start;

/// `fn limited(value: i64) -> i64 { inject global_state; when { if value <=
/// get<i64>(global_state, "limit") { return value } } }`
struct Limited {
    static constexpr auto name = "test::limited";

    static void eval(hgraph::In<"value", hgraph::TS<i64>> value, hgraph::Out<hgraph::TS<i64>> out,
                     hgraph::GlobalStateView store) {
        const hglrt::GlobalState global_state{store};
        if (value.value() <= hglrt::get_GlobalState_str<i64>(global_state, "limit")) {
            out.set(value.value());
        }
    }
};

/// The entry of `limited` over `trace`.
hglrt::testing::Entry limited_entry(Dense<i64> trace) {
    return [trace](hgraph::Wiring& wiring, datetime at) {
        auto replayed = hgraph::wire<hglrt_test::Replay>(wiring, hglrt::testing::replay_items(trace, at));
        hgraph::wire<hglrt_test::Record>(wiring, hgraph::wire<Limited>(wiring, replayed),
                                         hglrt::str{hglrt::testing::run_output});
    };
}

/// A store as an application sets it before its run (UNIT-14): `put(
/// global_state, "limit", limit)`.
hglrt::application::Store limit_of(i64 limit) {
    hglrt::application::Store store;
    hglrt::put_GlobalState_str_V<i64>(hglrt::GlobalState{store.view()}, "limit", limit);
    return store;
}

/// An entry with nothing to set in the store.
hglrt::application::Store& nothing_stored() {
    static hglrt::application::Store none;
    return none;
}

/// The lines of the results file `results`, which is then removed.
std::vector<std::string> lines_of(const std::filesystem::path& results) {
    std::ifstream written(results);
    std::vector<std::string> lines;
    for (std::string line; std::getline(written, line);) {
        lines.push_back(line);
    }
    written.close();
    std::filesystem::remove(results);
    return lines;
}

/// The pipe's end that a host's standard input is read from, which a test
/// writes to and may close: -1 once it is closed.
struct Requests {
    int written = -1;

    /// Writes `text` to it.
    void say(std::string_view text) const {
        REQUIRE(::write(written, text.data(), text.size()) == static_cast<ssize_t>(text.size()));
    }

    /// Closes it: the host reads that as a request to stop.
    void close() {
        if (written >= 0) {
            ::close(written);
            written = -1;
        }
    }
};

/// Runs the host of an application whose run statement is `body`, in a
/// process of its own whose standard input is a pipe, which `then` is given
/// with the process. The pipe is closed once the process has ended, where
/// `then` did not close it. What status the process ended with, and the
/// results it wrote.
template <class Then>
std::pair<int, std::vector<std::string>> hosted(bool real_time, hglrt::application::Body body, Then then) {
    static int runs = 0;
    const std::filesystem::path results =
        "hglrt-application-" + std::to_string(::getpid()) + "-" + std::to_string(runs++) + ".txt";
    int pipe[2];
    REQUIRE(::pipe(pipe) == 0);
    std::fflush(nullptr);
    const pid_t child = ::fork();
    REQUIRE(child >= 0);
    if (child == 0) {
        ::close(pipe[1]);
        ::dup2(pipe[0], STDIN_FILENO);
        ::close(pipe[0]);
        std::string program = "application";
        std::string file = results.string();
        std::vector<char*> argv{program.data(), file.data()};
        hglrt::application::main(static_cast<int>(argv.size()), argv.data(), real_time, body);
    }
    ::close(pipe[0]);
    Requests requests{pipe[1]};
    then(child, requests);
    int status = 0;
    REQUIRE(::waitpid(child, &status, 0) == child);
    requests.close();
    return {WIFEXITED(status) ? WEXITSTATUS(status) : -1, lines_of(results)};
}

/// Waits until the run has started: the time a host takes to start one.
void started() {
    std::this_thread::sleep_for(300ms);
}

/// Asks nothing of the run.
void nothing(pid_t, Requests&) {}

Output seeded() {
    hglrt::application::Store store = limit_of(2);
    return hglrt::application::run<i64>(store, limited_entry(Dense<i64>{1, 2, 3, 4}), {}, Mode::simulation);
}

Output silent() {
    hglrt::application::Store store = limit_of(2);
    return hglrt::application::run(store, limited_entry(Dense<i64>{1}), {}, Mode::simulation);
}

Output failing() {
    return hglrt::application::run<i64>(
        nothing_stored(),
        hglrt_test::entry_of<hglrt_test::Doubled>(Dense<i64>{1, std::numeric_limits<i64>::max()}), {},
        Mode::simulation);
}

Output both_bounds() {
    hglrt::application::Store store = limit_of(2);
    return hglrt::application::run<i64>(store, limited_entry(Dense<i64>{1}),
                                        {.end = start + 1s, .length = 1s}, Mode::simulation);
}

Output endless() {
    return hglrt::application::run<i64>(
        nothing_stored(), hglrt_test::entry_of<hglrt_test::Counted>(Dense<i64>{7}), {}, Mode::real_time);
}

Output bounded_in_real_time() {
    return hglrt::application::run<i64>(nothing_stored(),
                                        hglrt_test::entry_of<hglrt_test::Counted>(Dense<i64>{7, 7}),
                                        {.length = 200ms}, Mode::real_time);
}

/// A node whose `stop` takes a second.
struct SlowToStop {
    static constexpr auto name = "test::slow_to_stop";

    static void eval(hgraph::In<"value", hgraph::TS<i64>>) {}

    static void stop() { std::this_thread::sleep_for(1s); }
};

Output endless_and_slow_to_stop() {
    hglrt::testing::Entry entry = [](hgraph::Wiring& wiring, datetime at) {
        hglrt_test::entry_of<hglrt_test::Counted>(Dense<i64>{7})(wiring, at);
        hgraph::wire<SlowToStop>(wiring, hglrt::constant(wiring, i64{1}));
    };
    return hglrt::application::run<i64>(nothing_stored(), entry, {}, Mode::real_time);
}

Output endless_simulation() {
    hglrt::testing::Entry entry = [](hgraph::Wiring& wiring, datetime at) {
        hglrt_test::entry_of<hglrt_test::Counted>(Dense<i64>{7})(wiring, at);
        std::this_thread::sleep_for(60s);
    };
    return hglrt::application::run<i64>(nothing_stored(), entry, {}, Mode::simulation);
}

} // namespace

TEST_CASE("an instant is written as the host writes a publication's") {
    using hglrt::application::detail::instant_text;
    CHECK(instant_text(start) == "1970-01-01T00:00:00.000001Z");
    CHECK(instant_text(start + 1s - 1us) == "1970-01-01T00:00:01Z");
    CHECK(instant_text(datetime{std::chrono::sys_days{std::chrono::year{2026} / 11 / 21}} + 9h + 30min +
                       250ms) == "2026-11-21T09:30:00.250000Z");
}

TEST_CASE("an application's run is set its store before its graph is built, and gives its output (UNIT-14, "
          "CRUN-7, CRUN-8)") {
    const Output output = seeded();
    REQUIRE(output.has_value());
    REQUIRE(output->size() == 2);
    CHECK((*output)[0].at == "1970-01-01T00:00:00.000001Z");
    CHECK((*output)[0].value == "1");
    CHECK((*output)[1].at == "1970-01-01T00:00:00.000002Z");
    CHECK((*output)[1].value == "2");
    CHECK_FALSE(silent().has_value());
    // A run that runs no cycle has an empty output.
    hglrt::application::Store store = limit_of(2);
    const Output none =
        hglrt::application::run<i64>(store, limited_entry(Dense<i64>{1}), {.length = 0us}, Mode::simulation);
    REQUIRE(none.has_value());
    CHECK(none->empty());
}

TEST_CASE("the host writes the run's output, or that it has none") {
    const auto [status, lines] = hosted(false, &seeded, nothing);
    CHECK(status == 0);
    CHECK(lines == std::vector<std::string>{"start", "published 1970-01-01T00:00:00.000001Z 1",
                                            "published 1970-01-01T00:00:00.000002Z 2", "stopped"});
    const auto [silent_status, silent_lines] = hosted(false, &silent, nothing);
    CHECK(silent_status == 0);
    CHECK(silent_lines == std::vector<std::string>{"start", "stopped none"});
}

TEST_CASE("a run that fails has no output, and the host says why (CRUN-6, CRUN-2)") {
    const auto [status, lines] = hosted(false, &failing, nothing);
    CHECK(status == 1);
    REQUIRE(lines.size() == 2);
    CHECK(lines[1].starts_with("failed numeric.out_of_range: "));
    const auto [bounds_status, bounds_lines] = hosted(false, &both_bounds, nothing);
    CHECK(bounds_status == 1);
    REQUIRE(bounds_lines.size() == 2);
    CHECK(bounds_lines[1].starts_with("failed run.end_and_length: "));
}

TEST_CASE("the host stops a run in real time as it does at its end (RUN-3, CRUN-6)") {
    // Each run counts the one tick of its replay, at its start: the host
    // clock's present.
    const auto counted_once = [](const std::vector<std::string>& lines) {
        return lines.size() == 4 && lines[0] == "start" && lines[1] == "host" &&
               lines[2].starts_with("published ") && lines[2].ends_with("Z 1") && lines[3] == "stopped";
    };
    // Asked down standard input.
    const auto [status, lines] = hosted(true, &endless, [](pid_t, Requests& requests) {
        started();
        requests.say("stop\n");
    });
    CHECK(status == 0);
    CHECK(counted_once(lines));
    // Interrupted, and then asked down standard input too, as an interrupt
    // from a terminal reaches the host and what started it alike.
    const auto [interrupted_status, interrupted_lines] =
        hosted(true, &endless, [](pid_t child, Requests& requests) {
            started();
            REQUIRE(::kill(child, SIGINT) == 0);
            requests.say("stop\n");
        });
    CHECK(interrupted_status == 0);
    CHECK(counted_once(interrupted_lines));
    // Asked down standard input, and then interrupted: one interrupt is no
    // second.
    const auto [asked_status, asked_lines] = hosted(true, &endless, [](pid_t child, Requests& requests) {
        started();
        requests.say("stop\n");
        std::this_thread::sleep_for(50ms);
        REQUIRE(::kill(child, SIGINT) == 0);
    });
    CHECK(asked_status == 0);
    CHECK(counted_once(asked_lines));
    // Standard input closed is a request to stop: here before the run has
    // started, which is not lost as it starts.
    const auto [closed_status, closed_lines] =
        hosted(true, &endless, [](pid_t, Requests& requests) { requests.close(); });
    CHECK(closed_status == 0);
    REQUIRE(closed_lines.size() >= 3);
    CHECK(closed_lines[1] == "host");
    CHECK(closed_lines.back() == "stopped");
    // A second interrupt ends the process at once, here as its nodes stop.
    const auto [twice_status, twice_lines] =
        hosted(true, &endless_and_slow_to_stop, [](pid_t child, Requests&) {
            started();
            REQUIRE(::kill(child, SIGINT) == 0);
            std::this_thread::sleep_for(100ms);
            REQUIRE(::kill(child, SIGTERM) == 0);
        });
    CHECK(twice_status == 130);
    REQUIRE_FALSE(twice_lines.empty());
    CHECK(twice_lines.back() == "failed interrupted");
    // One with an end ends there, with nothing asked.
    const auto [bounded_status, bounded_lines] = hosted(true, &bounded_in_real_time, nothing);
    CHECK(bounded_status == 0);
    REQUIRE(bounded_lines.size() == 4);
    CHECK(bounded_lines[1].ends_with("Z 1"));
    CHECK(bounded_lines[2].ends_with("Z 2"));
    CHECK(bounded_lines[3] == "stopped");
}

TEST_CASE("a simulation asked to stop ends at once, with no output (RUN-3)") {
    const auto [status, lines] = hosted(false, &endless_simulation, [](pid_t child, Requests&) {
        started();
        REQUIRE(::kill(child, SIGINT) == 0);
    });
    CHECK(status == 130);
    CHECK(lines == std::vector<std::string>{"start", "failed interrupted"});
}
