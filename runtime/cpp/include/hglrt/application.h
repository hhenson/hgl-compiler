// The run of an application (the language's UNIT-14, RUN-1 to RUN-4; core's
// CRUN-1 to CRUN-8): what the function an application's run statement is
// written as calls. Its host, the executable's `main`, is application_host.h.
//
// An application's run is a run as a test's eval is (RUN-4: "An application
// and a test's eval are run under this one contract"). The compiler writes
// what the application puts in the run's store before its run statement as
// statements on a `Store` of its own, evaluated first, in the order written
// (UNIT-14); then the entry's arguments and its configuration, as an eval's;
// and the entry, which wires the entry's call and core's `record` of its
// result under `"run.output"`. `run` gives the run's graph that store before
// the entry is wired (CRUN-7: "Before its graph is built the store holds what
// the run's application put there"), runs it with the test host's run
// (testing.h), in simulation or in real time, and gives what it recorded as
// the host writes it.
//
//     ::hglrt::application::Output application_()
//     {
//         ::hglrt::application::Store store_;
//         const ::hglrt::GlobalState global_state{store_.view()};
//         ::hglrt::put_GlobalState_str_V<::hgraph::Int>(global_state, ::hgraph::Str{"limit"}, 3);
//         const ::hglrt::testing::Bounds bounds_ = ::hglrt::testing::Bounds{.length = 10s};
//         return ::hglrt::application::run<::hgraph::Int>(store_, [&](::hgraph::Wiring& wiring_, ...) {
//             record(wiring_, w_::counted(wiring_), ::hgraph::Str{::hglrt::testing::run_output});
//         }, bounds_, ::hglrt::testing::Mode::simulation);
//     }
#pragma once

#include <hglrt/application_host.h>
#include <hglrt/testing.h>
#include <hglrt/traces.h>
#include <hglrt/values.h>

#include <hgraph/runtime/global_state.h>

#include <optional>
#include <string>
#include <vector>

namespace hglrt::application {

/// The run's store as an application sets it before its run statement
/// (UNIT-14: "`inject global_state`, and then calls of `put` on it. Each is
/// evaluated once, in the order written, before the run's graph is built"):
/// hgraph's store, which `inject global_state` there is made from, as a
/// node's is from its view.
using Store = hgraph::GlobalState;

namespace detail {
/// `at` as the host writes a publication's instant: in UTC, its date and its
/// time of day to the second, `1970-01-01T00:00:01Z`, and the microseconds
/// where it has any, `1970-01-01T00:00:01.000001Z`.
std::string instant_text(datetime at);

/// `entry`, which first gives the store of the graph it wires what `store`
/// holds (CRUN-7). Both are kept by reference, and outlive the run.
testing::Entry seeded(Store& store, const testing::Entry& entry);
} // namespace detail

/// The run of an application whose entry's result is a T, wired by `entry`
/// on a graph whose store holds what `store` does (CRUN-1: "It builds the
/// entry's graph, starts it, evaluates zero or more cycles, and stops it,
/// once each"), within `bounds`, in `mode` (RUN-3), and its output (CRUN-8: "the value under that key once
/// the run has stopped"), each publication as the host writes it. A run that runs no cycle, its end at or
/// before its start, has an empty output; one that stops with nothing under the run's key fails, as an eval
/// does (CEVAL-6). A failure of the run escapes.
template <class T>
Output run(Store& store, const testing::Entry& entry, const testing::Bounds& bounds, testing::Mode mode) {
    const testing::Run ran = testing::run(detail::seeded(store, entry), bounds, mode);
    std::vector<Publication> published;
    for (const auto& [at, value] : testing::recording<T>(ran).entries) {
        published.push_back(Publication{detail::instant_text(at), testing::text_of(value)});
    }
    return published;
}

/// The run of an application whose entry has no result (CRUN-8: "An entry
/// with no result has no output"), as `run<T>` runs one that has.
inline Output run(Store& store, const testing::Entry& entry, const testing::Bounds& bounds,
                  testing::Mode mode) {
    (void)testing::run(detail::seeded(store, entry), bounds, mode);
    return std::nullopt;
}

} // namespace hglrt::application
