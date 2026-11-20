// The test host (testing.h).
#include "nested.h"

#include <hglrt/testing.h>

#include <hgraph/runtime/graph.h>

#include <cstdint>
#include <cstdio>
#include <format>
#include <functional>
#include <map>
#include <string>

namespace hglrt::testing {

namespace {

/// The line of the last statement marked in the running test (`statement`),
/// 0 before the first. Tests run one at a time, on the thread of `main`.
thread_local std::uint_least32_t running_line = 0;

/// The registered tests, by name.
std::map<std::string, Body, std::less<>>& registered() {
    static std::map<std::string, Body, std::less<>> tests;
    return tests;
}

/// `text` as one line of the results file: a backslash, a line feed and a
/// carriage return as `\\`, `\n` and `\r`, as the runner reads them.
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

/// What became of a test, as the line under it says (TST-14): an
/// assertion's failure as it says, and any other failure as `describe`
/// gives it.
std::string described(const std::exception_ptr& failure) {
    try {
        std::rethrow_exception(failure);
    } catch (const AssertionFailed& assertion) {
        return assertion.what();
    } catch (const std::exception& other) {
        return describe(other);
    } catch (...) {
        return "an exception that is no std::exception";
    }
}

} // namespace

// ---- Assertions --------------------------------------------------------------

void check(bool holds) {
    if (!holds) {
        throw AssertionFailed("the assertion does not hold");
    }
}

void statement(std::source_location where) noexcept {
    running_line = where.line();
}

std::optional<std::string> detail::escaping_code(const std::exception_ptr& failure) {
    try {
        std::rethrow_exception(failure);
    } catch (const std::exception& escaped) {
        std::optional<std::string> code;
        hglrt::detail::each_nested(escaped, [&](const std::exception& part) {
            if (dynamic_cast<const AssertionFailed*>(&part) != nullptr) {
                return true;
            }
            if (const auto* carried = dynamic_cast<const hgraph::ExecutionFailure*>(&part)) {
                code = carried->error().code;
                return true;
            }
            return false;
        });
        return code;
    } catch (...) {
        return std::nullopt;
    }
}

std::string detail::refused(std::string_view code, const std::exception_ptr& failure) {
    return std::format("the block raised {}, and `{}` was expected", described(failure), code);
}

// ---- Traces --------------------------------------------------------------------

duration smallest_step() noexcept {
    return hgraph::MIN_TD;
}

void detail::unordered_keys() {
    fail_uncoded("the keys of a timed trace must strictly increase");
}

void detail::before_start() {
    fail_uncoded("a timed trace has a key before the run's start");
}

TraceKey detail::relative_key(const TraceKey& key, const std::optional<datetime>& start) {
    if (const datetime* instant = std::get_if<datetime>(&key); instant != nullptr && start.has_value()) {
        return *instant - *start;
    }
    return key;
}

bool detail::before(const TraceKey& earlier, const TraceKey& later) {
    if (earlier.index() != later.index()) {
        fail_uncoded("a timed trace with durations and instants is read against no run");
    }
    return earlier < later;
}

// ---- Runs ----------------------------------------------------------------------

datetime earliest_start() noexcept {
    return hgraph::MIN_ST;
}

std::pair<datetime, std::optional<datetime>> resolved(const Bounds& bounds) {
    if (bounds.end.has_value() && bounds.length.has_value()) {
        fail(codes::end_and_length, "a run's configuration gives both an `end` and a `length`");
    }
    const datetime start = bounds.start.value_or(earliest_start());
    // CRUN-3 reserves the epoch itself for `never`.
    if (start < earliest_start()) {
        fail_uncoded("the run's start is before the earliest evaluation time");
    }
    std::optional<datetime> end = bounds.end;
    if (bounds.length.has_value()) {
        end = add__datetime_duration(start, *bounds.length);
    }
    return {start, end};
}

Run::Run(datetime start) : start_(start) {}

Run::Run(datetime start, hgraph::GraphExecutorValue executor)
    : start_(start), executor_(std::move(executor)) {}

bool Run::has(std::string_view key) const {
    return executor_.has_value() && store().contains(key);
}

hgraph::GlobalStateView Run::store() const {
    return executor_->view().graph().global_state();
}

Run run(const Entry& entry, const Bounds& bounds) {
    const auto [start, end] = resolved(bounds);
    hgraph::Wiring wiring;
    entry(wiring, start);
    if (end.has_value() && *end <= start) {
        // hgraph has no run that ends where it starts, so none is made.
        return Run(start);
    }
    hgraph::GraphExecutorBuilder builder;
    builder.graph_builder(std::move(wiring).finish())
        .mode(hgraph::GraphExecutorMode::Simulation)
        .start_time(start);
    if (end.has_value()) {
        builder.end_time(*end);
    }
    hgraph::GraphExecutorValue executor = builder.make_executor();
    executor.view().run();
    return Run(start, std::move(executor));
}

// ---- The host ------------------------------------------------------------------

Registration::Registration(std::string_view name, Body body) {
    registered().emplace(std::string{name}, body);
}

int main(int argc, char** argv) {
    if (argc < 2) {
        std::fprintf(stderr, "usage: %s RESULTS NAME...\n", argc > 0 ? argv[0] : "tests");
        return 2;
    }
    std::FILE* results = std::fopen(argv[1], "w");
    if (results == nullptr) {
        std::fprintf(stderr, "%s: the results file %s cannot be written\n", argv[0], argv[1]);
        return 2;
    }
    bool written = true;
    // Each line is flushed as it is written, so that what was recorded is
    // there however the process ends.
    const auto say = [&](const std::string& line) {
        written = std::fputs(line.c_str(), results) >= 0 && std::fputc('\n', results) != EOF &&
                  std::fflush(results) == 0 && written;
    };
    for (int position = 2; position < argc; ++position) {
        const int number = position - 2;
        const std::string_view name = argv[position];
        say(std::format("start {}", number));
        const auto found = registered().find(name);
        if (found == registered().end()) {
            say(std::format("unloaded {} {}", number,
                            escaped(std::format("the program has no test `{}`", name))));
            continue;
        }
        running_line = 0;
        std::exception_ptr failure;
        try {
            found->second();
        } catch (...) {
            failure = std::current_exception();
        }
        if (failure == nullptr) {
            say(std::format("passed {}", number));
        } else {
            say(std::format("failed {} {} {}", number, running_line, escaped(described(failure))));
        }
    }
    return std::fclose(results) == 0 && written ? 0 : 2;
}

} // namespace hglrt::testing
