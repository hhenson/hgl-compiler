// Unit tests of the executable test host (host.h), its results and limits.
#include "core_nodes.h"

#include <doctest/doctest.h>

#include <chrono>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <limits>
#include <source_location>
#include <string>
#include <sys/wait.h>
#include <thread>
#include <tuple>
#include <unistd.h>
#include <utility>
#include <vector>

namespace {

using hglrt::i64;
using hglrt::testing::Dense;
using hglrt::testing::Mode;
using namespace std::chrono_literals;

// Tests registered with the host, as the compiler registers a module's.

void passes() {
    hglrt::testing::statement();
    hglrt::testing::check(true);
}

std::uint_least32_t failing_line = 0;

void fails_an_assertion() {
    hglrt::testing::statement();
    hglrt::testing::check(true);
    failing_line = std::source_location::current().line() + 1;
    hglrt::testing::statement();
    hglrt::testing::check(false);
}

void raises_out_of_range() {
    hglrt::testing::statement();
    (void)hglrt::add__i64_i64(std::numeric_limits<i64>::max(), 1);
}

/// The text of a failure whose message holds a NUL and the other control
/// characters (LEX-24 admits `\u{0}` in a string).
const std::string controls("before \0\x01\x0b\x0c\x1b\x1f\x7f\n\r after", 22);

void raises_with_controls() {
    hglrt::testing::statement();
    hglrt::raise(hglrt::make_error("said " + controls, "user.code"));
}

// Tests held to a time limit (TST-17).

/// Where the statement of `runs_past_its_limit` that is running at its
/// limit is: the test runs in a process of its own, and this one is told.
const std::source_location limited_at = std::source_location::current();

/// A run in real time that has no end, which its host stops at the test's
/// limit: it then ends as it does at its end, with what it recorded.
void runs_past_its_limit() {
    hglrt::testing::statement();
    hglrt::testing::statement(limited_at);
    const auto ran = hglrt::testing::evaluate<i64>(hglrt_test::entry_of<hglrt_test::Counted>(Dense<i64>{7}),
                                                   {}, Mode::real_time);
    hglrt::testing::statement();
    hglrt::testing::check(ran.entries.size() == 1 && std::get<1>(ran.entries[0]) == 1);
}

/// A run started after the test's limit has passed, which is stopped as it
/// starts.
void starts_a_run_after_its_limit() {
    hglrt::testing::statement();
    std::this_thread::sleep_for(200ms);
    hglrt::testing::statement();
    (void)hglrt::testing::evaluate<i64>(hglrt_test::entry_of<hglrt_test::Counted>(Dense<i64>{7}), {},
                                        Mode::real_time);
}

/// A test that cannot be stopped.
void never_ends() {
    hglrt::testing::statement();
    while (true) {
        std::this_thread::sleep_for(10ms);
    }
}

/// A run in real time that ends of itself within the test's limit.
void runs_within_its_limit() {
    hglrt::testing::statement();
    const auto ran = hglrt::testing::evaluate<i64>(
        hglrt_test::entry_of<hglrt_test::Counted>(Dense<i64>{7, 7}), {.length = 500ms}, Mode::real_time);
    hglrt::testing::check(ran.entries.size() == 2 && std::get<1>(ran.entries[1]) == 2);
}

const hglrt::testing::Registration registered_passes{"host.cases::passes", &passes};
const hglrt::testing::Registration registered_fails{"host.cases::fails_an_assertion", &fails_an_assertion};
const hglrt::testing::Registration registered_raises{"host.cases::raises_out_of_range", &raises_out_of_range};
const hglrt::testing::Registration registered_controls{"host.cases::raises_with_controls",
                                                       &raises_with_controls};
const hglrt::testing::Registration registered_past{"host.cases::runs_past_its_limit", &runs_past_its_limit};
const hglrt::testing::Registration registered_after{"host.cases::starts_a_run_after_its_limit",
                                                    &starts_a_run_after_its_limit};
const hglrt::testing::Registration registered_never{"host.cases::never_ends", &never_ends};
const hglrt::testing::Registration registered_within{"host.cases::runs_within_its_limit",
                                                     &runs_within_its_limit};

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

/// Runs the host on the tests `names` in a process of its own, which gives
/// up on a test `grace_milliseconds` after its limit: what status it ended
/// with, and the results it wrote. A test that hangs ends that process, and
/// not this one.
std::pair<int, std::vector<std::string>> hosted(std::vector<std::string> names, long grace_milliseconds) {
    static int runs = 0;
    const std::filesystem::path results =
        "hglrt-host-" + std::to_string(::getpid()) + "-" + std::to_string(runs++) + ".txt";
    std::fflush(nullptr);
    const pid_t child = ::fork();
    REQUIRE(child >= 0);
    if (child == 0) {
        std::string program = "tests";
        std::string file = results.string();
        std::vector<char*> argv{program.data(), file.data()};
        for (std::string& name : names) {
            argv.push_back(name.data());
        }
        std::_Exit(hglrt::testing::detail::main_with_grace(static_cast<int>(argv.size()), argv.data(),
                                                           grace_milliseconds));
    }
    int status = 0;
    REQUIRE(::waitpid(child, &status, 0) == child);
    return {WIFEXITED(status) ? WEXITSTATUS(status) : -1, lines_of(results)};
}

} // namespace

TEST_CASE("the host runs the tests named and writes what became of each") {
    // In the directory the tests run in: their build's.
    const std::filesystem::path results = "hglrt-host-results.txt";
    std::string program = "tests";
    std::string file = results.string();
    std::vector<std::string> names{"host.cases::passes", "host.cases::fails_an_assertion",
                                   "host.cases::raises_out_of_range", "host.cases::absent"};
    std::vector<char*> argv{program.data(), file.data()};
    for (std::string& name : names) {
        argv.push_back(name.data());
    }
    REQUIRE(hglrt::testing::main(static_cast<int>(argv.size()), argv.data()) == 0);

    const std::vector<std::string> lines = lines_of(results);
    REQUIRE(lines.size() == 8);
    CHECK(lines[0] == "start 0");
    CHECK(lines[1] == "passed 0");
    CHECK(lines[2] == "start 1");
    CHECK(lines[3] == "failed 1 " + std::to_string(failing_line) + " the assertion does not hold");
    CHECK(lines[4] == "start 2");
    CHECK(lines[5].starts_with("failed 2 "));
    CHECK(lines[5].ends_with(" numeric.out_of_range: an i64 result is outside the signed 64-bit range"));
    CHECK(lines[6] == "start 3");
    CHECK(lines[7] == "unloaded 3 the program has no test `host.cases::absent`");

    // A command line with no results file is refused, and so is a time
    // limit that is no count of microseconds.
    std::vector<char*> bare{program.data()};
    CHECK(hglrt::testing::main(1, bare.data()) == 2);
    std::string unlimited = "host.cases::passes@soon";
    std::vector<char*> malformed{program.data(), file.data(), unlimited.data()};
    CHECK(hglrt::testing::main(3, malformed.data()) == 2);
    CHECK_FALSE(std::filesystem::exists(results));
}

TEST_CASE("a failure's text is one results line, whatever characters it holds") {
    const std::filesystem::path results = "hglrt-host-controls.txt";
    std::string program = "tests";
    std::string file = results.string();
    std::string name = "host.cases::raises_with_controls";
    std::vector<char*> argv{program.data(), file.data(), name.data()};
    REQUIRE(hglrt::testing::main(static_cast<int>(argv.size()), argv.data()) == 0);
    const std::vector<std::string> lines = lines_of(results);
    REQUIRE(lines.size() == 2);
    // A line feed and a carriage return are escaped, and every other
    // character is written as it is: a NUL ends no line.
    const std::string written("before \0\x01\x0b\x0c\x1b\x1f\x7f\\n\\r after", 24);
    CHECK(lines[1].starts_with("failed 0 "));
    CHECK(lines[1].ends_with(" user.code: said " + written));
}

TEST_CASE("a time limit too long for the host's clock is one that cannot pass (TST-17)") {
    const std::filesystem::path results = "hglrt-host-limits.txt";
    std::string program = "tests";
    std::string file = results.string();
    // 2^63 - 1 microseconds, about 317 years, and about 32 years.
    std::vector<std::string> names{"host.cases::passes@9223372036854775807",
                                   "host.cases::passes@10000000000000000",
                                   "host.cases::passes@1000000000000000"};
    std::vector<char*> argv{program.data(), file.data()};
    for (std::string& name : names) {
        argv.push_back(name.data());
    }
    REQUIRE(hglrt::testing::detail::main_with_grace(static_cast<int>(argv.size()), argv.data(), 200) == 0);
    const std::vector<std::string> lines = lines_of(results);
    REQUIRE(lines.size() == 6);
    for (std::size_t test = 0; test < 3; ++test) {
        CHECK(lines[2 * test + 1] == "passed " + std::to_string(test));
    }
}

TEST_CASE("a test past its time limit has its runs stopped, and has failed (TST-17)") {
    // The run in real time is stopped at the limit of 100ms and ends as at
    // its end; the test goes on to its end, and has failed; the next test
    // is not stopped.
    const auto [status, lines] =
        hosted({"host.cases::runs_past_its_limit@100000", "host.cases::runs_within_its_limit",
                "host.cases::runs_within_its_limit@5000000"},
               2'000);
    CHECK(status == 0);
    REQUIRE(lines.size() == 6);
    CHECK(lines[0] == "start 0");
    CHECK(lines[1] == "failed 0 " + std::to_string(limited_at.line()) +
                          " timed out: it was still running after its limit of 100ms");
    CHECK(lines[2] == "start 1");
    CHECK(lines[3] == "passed 1");
    CHECK(lines[4] == "start 2");
    CHECK(lines[5] == "passed 2");
}

TEST_CASE("a run a test starts after its limit is stopped as it starts (TST-17)") {
    const auto [status, lines] =
        hosted({"host.cases::starts_a_run_after_its_limit@50000", "host.cases::passes"}, 2'000);
    CHECK(status == 0);
    REQUIRE(lines.size() == 4);
    CHECK(lines[1].starts_with("failed 0 "));
    CHECK(lines[1].ends_with(" timed out: it was still running after its limit of 50ms"));
    CHECK(lines[3] == "passed 1");
}

TEST_CASE("a test that cannot be stopped ends the process at its limit and grace (TST-17)") {
    const auto [status, lines] = hosted({"host.cases::never_ends@50000", "host.cases::passes"}, 200);
    CHECK(status == 3);
    REQUIRE(lines.size() == 2);
    CHECK(lines[0] == "start 0");
    CHECK(lines[1].starts_with("failed 0 "));
    CHECK(lines[1].ends_with(" timed out: it was still running after its limit of 50ms"));
}
