// The host of an executable of tests (host.h).
#include "host_state.h"

#include <hglrt/host.h>

#include <charconv>
#include <chrono>
#include <cmath>
#include <condition_variable>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <exception>
#include <format>
#include <functional>
#include <map>
#include <memory>
#include <mutex>
#include <optional>
#include <string>
#include <string_view>
#include <system_error>
#include <thread>
#include <vector>

namespace hglrt::testing {

namespace {

/// The registered tests, by name.
std::map<std::string, Body, std::less<>>& registered() {
    static std::map<std::string, Body, std::less<>> tests;
    return tests;
}

/// `text` as one line of the results file: a backslash, a line feed and a
/// carriage return as `\\`, `\n` and `\r`, as the runner reads them, and
/// every other character as it is, a NUL and the other control characters
/// among them: the runner ends a line at a line feed alone.
std::string escaped(std::string_view text) {
    std::string out;
    out.reserve(text.size());
    for (const char character : text) {
        switch (character) {
        case '\\':
            out += "\\\\";
            break;
        case '\n':
            out += "\\n";
            break;
        case '\r':
            out += "\\r";
            break;
        default:
            out += character;
        }
    }
    return out;
}

/// A test named on the command line: its name, and its time limit where it
/// is held to one.
struct Named {
    std::string_view name;
    std::optional<std::chrono::microseconds> limit;
};

/// The test `argument` names, `NAME` or `NAME@MICROSECONDS`; none where its
/// limit is no count of microseconds.
std::optional<Named> named(std::string_view argument) {
    const std::size_t mark = argument.rfind('@');
    if (mark == std::string_view::npos) {
        return Named{argument, std::nullopt};
    }
    const std::string_view written = argument.substr(mark + 1);
    std::int64_t microseconds = 0;
    const auto [end, failure] =
        std::from_chars(written.data(), written.data() + written.size(), microseconds);
    if (written.empty() || failure != std::errc{} || end != written.data() + written.size() ||
        microseconds < 0) {
        return std::nullopt;
    }
    return Named{argument.substr(0, mark), std::chrono::microseconds{microseconds}};
}

/// A time limit as a person reads it, as the compiler's driver writes one:
/// `1m`, `90s`, `0.5s`.
std::string seconds(std::chrono::microseconds limit) {
    const double whole = static_cast<double>(limit.count()) / 1e6;
    if (whole >= 60.0 && std::fmod(whole, 60.0) == 0.0) {
        return std::format("{}m", static_cast<std::int64_t>(whole / 60.0));
    }
    return std::format("{}s", whole);
}

/// What became of a test: none where it passed, and otherwise the line of
/// its statement that was running and what it failed with. `given_up` where
/// it was still running after its limit and its grace, and could not be
/// stopped.
struct Outcome {
    std::optional<std::pair<std::uint_least32_t, std::string>> failed;
    bool given_up = false;
};

/// Runs `body` on the thread of the host, as a test with no limit is.
Outcome unbounded(Body body) {
    detail::running_line = 0;
    try {
        body();
    } catch (...) {
        return Outcome{std::pair{detail::running_line.load(), detail::described(std::current_exception())}};
    }
    return Outcome{};
}

/// Waits on `ended` with `lock` until `done()` holds or `limit` has passed,
/// whichever is first, and gives whether `done()` holds. A limit too long for
/// the steady clock to count from now is one that cannot pass: the wait is
/// then for `done()` alone.
template <class Done>
bool waited(std::condition_variable& ended, std::unique_lock<std::mutex>& lock,
            std::chrono::microseconds limit, Done done) {
    using clock = std::chrono::steady_clock;
    const clock::time_point now = clock::now();
    // The longest wait the clock can count from now, in whole microseconds.
    const auto room = std::chrono::floor<std::chrono::microseconds>(clock::time_point::max() - now);
    if (limit >= room) {
        ended.wait(lock, done);
        return true;
    }
    return ended.wait_until(lock, now + std::chrono::duration_cast<clock::duration>(limit), done);
}

/// Runs `body` on a thread of its own, and waits for it no longer than
/// `limit` (TST-17): then the runs of the test are asked to stop, and the
/// test is given `grace` more to end. It has failed whatever it then does.
Outcome bounded(Body body, std::chrono::microseconds limit, std::chrono::milliseconds grace) {
    // What the test's thread and the host share; the thread keeps it alive
    // where it is left running.
    struct Shared {
        std::mutex mutex;
        std::condition_variable ended;
        bool done = false;
        std::exception_ptr failure;
    };
    const auto shared = std::make_shared<Shared>();
    detail::running_line = 0;
    detail::runs_watched(true);
    std::thread test([shared, body] {
        std::exception_ptr failure;
        try {
            body();
        } catch (...) {
            failure = std::current_exception();
        }
        {
            const std::lock_guard lock(shared->mutex);
            shared->failure = failure;
            shared->done = true;
        }
        shared->ended.notify_all();
    });
    std::unique_lock lock(shared->mutex);
    if (waited(shared->ended, lock, limit, [&] { return shared->done; })) {
        lock.unlock();
        test.join();
        detail::runs_watched(false);
        if (shared->failure == nullptr) {
            return Outcome{};
        }
        return Outcome{std::pair{detail::running_line.load(), detail::described(shared->failure)}};
    }
    // The statement running when the limit passed.
    const std::uint_least32_t line = detail::running_line.load();
    const std::string said = "timed out: it was still running after its limit of " + seconds(limit);
    lock.unlock();
    detail::stop_runs();
    lock.lock();
    const bool done = shared->ended.wait_for(lock, grace, [&] { return shared->done; });
    lock.unlock();
    if (!done) {
        // The thread cannot be ended, and what it holds may be the next
        // test's too.
        test.detach();
        return Outcome{std::pair{line, said}, true};
    }
    test.join();
    detail::runs_watched(false);
    detail::stops_withdrawn();
    return Outcome{std::pair{line, said}};
}

} // namespace

Registration::Registration(std::string_view name, Body body) {
    registered().emplace(std::string{name}, body);
}

int main(int argc, char** argv) {
    return detail::main_with_grace(argc, argv, 5'000);
}

int detail::main_with_grace(int argc, char** argv, long grace_milliseconds) {
    if (argc < 2) {
        std::fprintf(stderr, "usage: %s RESULTS TEST...\n", argc > 0 ? argv[0] : "tests");
        return 2;
    }
    std::vector<Named> tests;
    for (int position = 2; position < argc; ++position) {
        const std::optional<Named> test = named(argv[position]);
        if (!test.has_value()) {
            std::fprintf(stderr, "%s: the time limit of %s is no count of microseconds\n", argv[0],
                         argv[position]);
            return 2;
        }
        tests.push_back(*test);
    }
    std::FILE* results = std::fopen(argv[1], "w");
    if (results == nullptr) {
        std::fprintf(stderr, "%s: the results file %s cannot be written\n", argv[0], argv[1]);
        return 2;
    }
    bool written = true;
    // Each line is written whole, a NUL in it among the rest, and flushed as
    // it is written, so that what was recorded is there however the process
    // ends.
    const auto say = [&](const std::string& line) {
        written = std::fwrite(line.data(), 1, line.size(), results) == line.size() &&
                  std::fputc('\n', results) != EOF && std::fflush(results) == 0 && written;
    };
    for (std::size_t number = 0; number < tests.size(); ++number) {
        const Named& test = tests[number];
        say(std::format("start {}", number));
        const auto found = registered().find(test.name);
        if (found == registered().end()) {
            say(std::format("unloaded {} {}", number,
                            escaped(std::format("the program has no test `{}`", test.name))));
            continue;
        }
        const Outcome outcome =
            test.limit.has_value()
                ? bounded(found->second, *test.limit, std::chrono::milliseconds{grace_milliseconds})
                : unbounded(found->second);
        if (!outcome.failed.has_value()) {
            say(std::format("passed {}", number));
            continue;
        }
        say(std::format("failed {} {} {}", number, outcome.failed->first, escaped(outcome.failed->second)));
        if (outcome.given_up) {
            std::fclose(results);
            std::_Exit(3);
        }
    }
    return std::fclose(results) == 0 && written ? 0 : 2;
}

} // namespace hglrt::testing
