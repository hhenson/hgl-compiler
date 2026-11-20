// The host of an application's executable (the language's UNIT-14, RUN-1 to
// RUN-4): what the entry unit of one needs, and nothing of hgraph's, as
// host.h is for an executable of tests. Generated code includes
// <hglrt/hglrt.h>, which has this too (application.h).
//
// The compiler writes one executable for each application. Its module's unit
// holds the function its run statement is written as (application.h), and its
// entry unit's `main` calls `hglrt::application::main` with that function,
// which runs it once and records what became of the run in a results file, in
// the form the runner reads from the Python target's driver
// (`crates/hgl_hgraph/support/run_application.py`):
//
//     EXECUTABLE RESULTS
//
//     start                the run is about to start
//     published TIME TEXT  one publication of the run's output, in time
//                          order: its instant and what was published
//     stopped              the run ended, and every publication of its
//                          output is above
//     stopped none         the run ended, and its entry has no result
//     host                 the host asked the run to stop, before `stopped`
//     failed TEXT          the run failed, and has no output (core's CRUN-6)
//
// where TEXT has `\\`, `\n` and `\r` for a backslash, a line feed and a
// carriage return, as host.h writes it. A `start` with nothing after it is a
// run that was under way when the process ended. The results are a file, and
// not what the process prints, so that nothing a program or hgraph prints can
// be taken for an outcome.
//
// RUN-3: a run in real time runs "until the configuration's end or until its
// host stops it". The host asks for a stop in one of two ways: an interrupt
// or a request to end, of this process (SIGINT, SIGTERM), or a line `stop` on
// standard input, which what started the process writes for a request that
// reached it alone; standard input closed is such a request too, as what
// started the run is gone. A run in real time that is asked to stop is
// stopped through hgraph's own control, the executor's stop, and ends as it
// does at its end: every node that started is stopped (CRUN-6), and its output
// is what it recorded up to then. A simulation waits for nothing: asked to
// stop, the process ends at once, `failed interrupted`, with the status 130,
// and so does a second interrupt of a run in real time.
#pragma once

#include <iosfwd>
#include <optional>
#include <string>
#include <vector>

namespace hglrt::application {

/// One publication of a run's output (core's CRUN-8), as the host writes it:
/// its instant, in UTC, `1970-01-01T00:00:01.000001Z`, and what was
/// published, as the language writes a value (traces.h, `text_of`).
struct Publication {
    std::string at;
    std::string value;
};

/// A run's output (RUN-1: "The entry's result, if any, is the run's output"):
/// each publication of the entry's result, in time order; none for an entry
/// with no result.
using Output = std::optional<std::vector<Publication>>;

/// An application's run statement, as the compiler writes it: it sets the
/// run's store, runs the entry once (CRUN-1) and gives the run's output. A
/// failure of the run escapes it.
using Body = Output (*)();

/// The `main` of an application's executable, `EXECUTABLE RESULTS`: runs
/// `body` once, in real time where `real_time` (the run statement's word is
/// `run`) and otherwise in simulation (`eval`), and writes what became of the
/// run to RESULTS, as above. It ends the process: with 0 once the run has
/// ended, 1 where it failed, 130 where it was interrupted, and 2 where the
/// command line is not that or the results cannot be written. No static
/// object is destroyed as it ends: a thread may still be reading standard
/// input.
[[noreturn]] void main(int argc, char** argv, bool real_time, Body body);

namespace detail {
/// `main`, which gives the status in place of ending the process: the
/// host's requests to stop are read from `requests` (none where it is null),
/// and the process's interrupts are taken only where `signals`. For the
/// library's own tests.
int main_with(int argc, char** argv, bool real_time, Body body, std::istream* requests, bool signals);
} // namespace detail

} // namespace hglrt::application
