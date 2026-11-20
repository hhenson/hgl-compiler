// The run of an application (the language's UNIT-14, RUN-1 to RUN-4; core's
// CRUN-1 to CRUN-8): what the function an application's run statement is
// written as calls. Its host, the executable's `main`, is application_host.h.
//
// An application's run is a run as a test's eval is (RUN-4: "An application
// and a test's eval are run under this one contract"): the compiler writes its
// entry, which sets the run's store with what the application puts there
// before its run statement, wires the entry's call, and core's `record` of its
// result under `"run.output"`; `run` runs it with the test host's run
// (testing.h), in simulation or in real time, and gives what it recorded as
// the host writes it.
//
//     ::hglrt::application::Output application_()
//     {
//         return ::hglrt::application::run<::hgraph::Int>([&](::hgraph::Wiring& wiring_, ...) {
//             const ::hglrt::GlobalState global_state{wiring_.global_state()};
//             ::hglrt::put_GlobalState_str_V<::hgraph::Int>(global_state, ::hgraph::Str{"limit"}, 3);
//             record(wiring_, w_::counted(wiring_), ::hgraph::Str{::hglrt::testing::run_output});
//         }, ::hglrt::application::bounds(config), ::hglrt::testing::Mode::simulation);
//     }
#pragma once

#include <hglrt/application_host.h>
#include <hglrt/testing.h>
#include <hglrt/traces.h>
#include <hglrt/values.h>

#include <optional>
#include <string>
#include <vector>

namespace hglrt::application {

namespace detail {
/// `at` as the host writes a publication's instant: in UTC, its date and its
/// time of day to the second, `1970-01-01T00:00:01Z`, and the microseconds
/// where it has any, `1970-01-01T00:00:01.000001Z`.
std::string instant_text(datetime at);
} // namespace detail

/// The bounds `config`, core's `RunConfig` as the compiler writes it (a
/// struct of `start`, `end` and `length`), gives a run (RUN-1: "the
/// configuration is a value of core's `RunConfig`"; CRUN-2). What the bounds
/// mean, `run.end_and_length` among it, is the run's (testing.h, `resolved`).
template <class Config>
testing::Bounds bounds(const Config& config) {
    return testing::Bounds{config.start, config.end, config.length};
}

/// The run of an application whose entry's result is a T, wired by `entry`
/// (CRUN-1: "It builds the entry's graph, starts it, evaluates zero or more
/// cycles, and stops it, once each"), within `bounds`, in `mode` (RUN-3), and
/// its output (CRUN-8: "the value under that key once the run has stopped"),
/// each publication as the host writes it. A run that runs no cycle, its end
/// at or before its start, has an empty output; one that stops with nothing
/// under the run's key fails, as an eval does (CEVAL-6). A failure of the run
/// escapes.
template <class T>
Output run(const testing::Entry& entry, const testing::Bounds& bounds, testing::Mode mode) {
    const testing::Run ran = testing::run(entry, bounds, mode);
    std::vector<Publication> published;
    if (!ran.built()) {
        return published;
    }
    for (const auto& [at, value] : testing::recording<T>(ran).entries) {
        published.push_back(Publication{detail::instant_text(at), testing::text_of(value)});
    }
    return published;
}

/// The run of an application whose entry has no result (CRUN-8: "An entry
/// with no result has no output"), as `run<T>` runs one that has.
inline Output run(const testing::Entry& entry, const testing::Bounds& bounds, testing::Mode mode) {
    (void)testing::run(entry, bounds, mode);
    return std::nullopt;
}

} // namespace hglrt::application
