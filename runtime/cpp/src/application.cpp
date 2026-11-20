// The host of an application's executable (application_host.h) and the text
// of a run's output (application.h).
#include "host_state.h"

#include <hglrt/application.h>
#include <hglrt/application_host.h>

#include <atomic>
#include <chrono>
#include <csignal>
#include <cstdio>
#include <cstdlib>
#include <exception>
#include <format>
#include <iostream>
#include <istream>
#include <mutex>
#include <string>
#include <thread>

namespace hglrt::application {

std::string detail::instant_text(datetime at) {
    const auto day = std::chrono::floor<std::chrono::days>(at);
    const std::chrono::year_month_day calendar{day};
    const std::chrono::hh_mm_ss<duration> clock{at - day};
    std::string text = std::format("{:%F}T{:02}:{:02}:{:02}", calendar, clock.hours().count(),
                                   clock.minutes().count(), clock.seconds().count());
    if (const auto fraction = clock.subseconds().count(); fraction != 0) {
        text += std::format(".{:06}", fraction);
    }
    return text + "Z";
}

namespace {

/// The status the process ends with where it is interrupted, as a shell
/// gives one an interrupt ended: 128 and the interrupt's number.
constexpr int interrupted_status = 130;

/// Ends the process with `status`, what it printed written out first, and no
/// static object destroyed. Standard input is not flushed: the thread that
/// reads it may hold it, waiting for a line, for as long as the process lives.
[[noreturn]] void end_process(int status) {
    std::fflush(stdout);
    std::fflush(stderr);
    std::_Exit(status);
}

/// What the run's host shares with what asks it to stop, from the threads
/// that watch for a request: the results file, the run's word, whether a
/// stop was asked for and whether by an interrupt, and whether the run's
/// outcome is written. Made once for the process and never destroyed, as a
/// thread may still be reading standard input as the process ends.
struct Host {
    std::mutex mutex;
    std::FILE* results = nullptr;
    bool written = true;
    bool real_time = false;
    bool asked = false;
    bool interrupted = false;
    bool ended = false;

    /// Writes `line` to the results, whole, and flushes it, so that what
    /// was recorded is there however the process ends. Called with `mutex`
    /// held.
    void say(const std::string& line) {
        written = std::fwrite(line.data(), 1, line.size(), results) == line.size() &&
                  std::fputc('\n', results) != EOF && std::fflush(results) == 0 && written;
    }
};

Host& host() {
    static Host* const kept = new Host;
    return *kept;
}

/// How many interrupts or requests to end the process has been sent: its
/// signal handler counts them, and a thread of the host reads them. A plain
/// lock-free atomic, which a signal handler may use.
std::atomic<int> interrupts{0};
static_assert(std::atomic<int>::is_always_lock_free);

/// Whether the host is still watching for interrupts.
std::atomic<bool> watching{false};

/// Counts an interrupt, and takes the next one too: a platform may give the
/// signal its default handling once it has been handled.
void on_interrupt(int signal) {
    interrupts.fetch_add(1);
    std::signal(signal, on_interrupt);
}

/// The host asks the run to stop (RUN-3), by an interrupt where
/// `interrupt`: a run in real time ends as it does at its end, and what it
/// recorded is its output. A simulation waits for nothing, and a second
/// interrupt is the host insisting: the process then ends at once, and the
/// run has no output. Only an interrupt counts towards a second: one from a
/// terminal reaches this process and what started it alike, which then
/// passes its own request on.
void stop_asked(bool interrupt) {
    Host& kept = host();
    const std::lock_guard lock(kept.mutex);
    if (kept.ended) {
        return;
    }
    if (!kept.real_time || (interrupt && kept.interrupted)) {
        kept.say("failed interrupted");
        end_process(interrupted_status);
    }
    kept.interrupted = kept.interrupted || interrupt;
    if (!kept.asked) {
        kept.asked = true;
        kept.say("host");
    }
    // hgraph's own control: the executor of the run under way is asked to
    // stop, and one that starts after this is stopped as it starts.
    testing::detail::stop_runs();
}

/// Reads the host's requests from `requests` until it ends: each line
/// `stop` is one, and so is its end, what started the run being gone.
void read_requests(std::istream* requests) {
    for (std::string line; std::getline(*requests, line);) {
        if (line == "stop" || line == "stop\r") {
            stop_asked(false);
        }
    }
    stop_asked(false);
}

/// Takes each interrupt counted, while the host is watching for them.
void watch_interrupts() {
    int taken = 0;
    while (watching.load()) {
        for (const int sent = interrupts.load(); taken < sent; ++taken) {
            stop_asked(true);
        }
        std::this_thread::sleep_for(std::chrono::milliseconds{10});
    }
}

} // namespace

int detail::main_with(int argc, char** argv, bool real_time, Body body, std::istream* requests,
                      bool signals) {
    if (argc != 2) {
        std::fprintf(stderr, "usage: %s RESULTS\n", argc > 0 ? argv[0] : "application");
        return 2;
    }
    Host& kept = host();
    {
        const std::lock_guard lock(kept.mutex);
        kept.results = std::fopen(argv[1], "w");
        if (kept.results == nullptr) {
            std::fprintf(stderr, "%s: the results file %s cannot be written\n", argv[0], argv[1]);
            return 2;
        }
        kept.written = true;
        kept.real_time = real_time;
        kept.asked = false;
        kept.interrupted = false;
        kept.ended = false;
        kept.say("start");
    }
    // A run in real time is watched, so that a stop asked for before it
    // starts is not lost as it starts (testing.cpp, `StopAtStart`).
    if (real_time) {
        testing::detail::runs_watched(true);
    }
    std::thread interrupts_watched;
    if (signals) {
        std::signal(SIGINT, on_interrupt);
        std::signal(SIGTERM, on_interrupt);
        watching = true;
        interrupts_watched = std::thread(watch_interrupts);
    }
    if (requests != nullptr) {
        // It cannot be joined: it may wait on standard input for as long
        // as the process lives.
        std::thread(read_requests, requests).detach();
    }
    Output output;
    std::exception_ptr failure;
    try {
        output = body();
    } catch (...) {
        failure = std::current_exception();
    }
    int status = 0;
    {
        const std::lock_guard lock(kept.mutex);
        kept.ended = true;
        if (failure != nullptr) {
            // CRUN-6: "A run that failed has no output."
            kept.say("failed " + testing::detail::escaped(testing::detail::described(failure)));
            status = 1;
        } else if (!output.has_value()) {
            kept.say("stopped none");
        } else {
            for (const Publication& published : *output) {
                kept.say(
                    std::format("published {} {}", published.at, testing::detail::escaped(published.value)));
            }
            kept.say("stopped");
        }
        if (std::fclose(kept.results) != 0 || !kept.written) {
            status = 2;
        }
        kept.results = nullptr;
    }
    if (interrupts_watched.joinable()) {
        watching = false;
        interrupts_watched.join();
    }
    if (real_time) {
        testing::detail::runs_watched(false);
        testing::detail::stops_withdrawn();
    }
    return status;
}

void main(int argc, char** argv, bool real_time, Body body) {
    end_process(detail::main_with(argc, argv, real_time, body, &std::cin, true));
}

} // namespace hglrt::application
