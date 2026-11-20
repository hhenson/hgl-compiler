// The host of an executable of tests: what the entry unit of one needs, and
// nothing of hgraph's, so that it compiles in a fraction of the time a unit
// of generated node and graph code takes. Generated code includes
// <hglrt/hglrt.h>, which has this too (testing.h).
//
// The compiler writes one executable for each module with tests (TST-1). Its
// entry unit declares each of the module's tests, registers it by the name
// the runner gives it, `module.path::name` (TST-15), and its `main` calls
// `hglrt::testing::main`, which runs the ones named on the command line and
// records what becomes of each in a results file, in the form the runner
// reads from the Python target's driver
// (`crates/hgl_hgraph/support/run_tests.py`):
//
//     EXECUTABLE RESULTS TEST...
//
//     start N            the test is about to run
//     passed N           it ran to its end
//     failed N LINE TEXT it raised, or ran past its time limit: LINE is the
//                        line of the statement of the test that was running
//                        (`statement`), 0 when unknown
//     unloaded N TEXT    no test of the program has that name
//
// where N is the test's position among those named, from 0, and TEXT has
// `\\`, `\n` and `\r` for a backslash, a line feed and a carriage return.
// Each line is written as it happens, so that a test that stops the process
// shows a `start` and nothing after it. The results are a file, and not what
// the process prints, so that nothing a program or hgraph prints can be
// taken for an outcome.
//
// Each TEST is a test's name, and for one held to a time limit (TST-17) its
// name, `@`, and the limit in whole microseconds: `demo.live::beats@5000000`.
// Such a test runs on a thread of its own, and the host waits for it no
// longer than its limit: then the run under way, and each the test starts
// after it, is asked to stop, as its host stops a run in real time, and ends
// as it does at its end; the test goes on to its own end, and has failed
// whatever it then does ("One still running when its limit passes is
// stopped, and has failed"). One that has not ended five seconds after its
// limit cannot be stopped: its `failed` line is written and the process
// ends, with the status 3, and no test after it is run.
#pragma once

#include <string_view>

namespace hglrt::testing {

/// A test's body, as the compiler writes it.
using Body = void (*)();

/// Registers the test `body` under `name`, `module.path::name` (TST-15),
/// for `main` to run: the compiler writes one, at namespace scope, for each
/// test of the module.
class Registration {
  public:
    Registration(std::string_view name, Body body);
};

/// The `main` of an executable of tests: `argv` is the executable, the
/// results file, and each test to run, in the order to run them. Gives 0
/// once the results are written, and 2 where the command line is not that
/// or the results file cannot be written.
int main(int argc, char** argv);

namespace detail {
/// `main`, where a test that has not ended `grace_milliseconds` after its
/// limit is given up on, in place of five seconds: for the library's own
/// tests.
int main_with_grace(int argc, char** argv, long grace_milliseconds);
} // namespace detail

} // namespace hglrt::testing
