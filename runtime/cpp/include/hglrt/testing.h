// The test host: what an executable of a module's generated tests needs.
//
// The compiler writes one executable for each module with tests (TST-1). Its
// tests register by the name the runner gives them, `module.path::name`
// (TST-15), and its `main` calls `hglrt::testing::main`, which runs the ones
// named on the command line and records what becomes of each in a results
// file, in the form the runner reads from the Python target's driver
// (`crates/hgl_hgraph/support/run_tests.py`):
//
//     EXECUTABLE RESULTS NAME...
//
//     start N            the test is about to run
//     passed N           it ran to its end
//     failed N LINE TEXT it raised: LINE is the line of the statement of the
//                        test that was running (`statement`), 0 when unknown
//     unloaded N TEXT    no test of the program has that name
//
// where N is the test's position among the names, from 0, and TEXT has `\\`,
// `\n` and `\r` for a backslash, a line feed and a carriage return. Each line
// is written as it happens, so that a test that stops the process shows a
// `start` and nothing after it. The results are a file, and not what the
// process prints, so that nothing a program or hgraph prints can be taken for
// an outcome.
//
// A test's eval is a run (CEVAL-1): the compiler writes the entry, which
// wires core's `replay` to each temporal input, the target, and core's
// `record` to its result under `"run.output"`; `evaluate` builds the entry's
// graph, runs it with hgraph's executor in simulation, and reads the
// recording from the run's store once it has stopped.
#pragma once

#include <hglrt/capabilities.h>
#include <hglrt/failure.h>
#include <hglrt/scalar.h>
#include <hglrt/values.h>

#include <hgraph/runtime/executor.h>
#include <hgraph/runtime/global_state.h>
#include <hgraph/types/graph_wiring.h>

#include <cstddef>
#include <exception>
#include <functional>
#include <optional>
#include <source_location>
#include <stdexcept>
#include <string>
#include <string_view>
#include <tuple>
#include <type_traits>
#include <utility>
#include <variant>
#include <vector>

namespace hglrt::testing {

// ---- Assertions (TST-2, TST-10, TST-11) ----------------------------------

/// The failure of an assertion of a test: a test failure (ERR-1), which no
/// `assert raises` takes for the failure it expects (TST-11).
class AssertionFailed : public std::runtime_error {
  public:
    using std::runtime_error::runtime_error;
};

/// `assert condition` (TST-2): fails the test where `holds` is false.
void check(bool holds);

/// Marks the statement of the running test that starts at `where`: the line
/// the host gives for a failure is that of the last statement marked. The
/// compiler marks each statement of a test before it runs.
void statement(std::source_location where = std::source_location::current()) noexcept;

namespace detail {
/// The execution code of what escaped an `assert raises` block, where it is
/// an HGL failure with one, and none where it has none or it is, or is
/// raised in, an assertion's failure (TST-11: "A failed nested assertion
/// cannot satisfy an enclosing raises").
std::optional<std::string> escaping_code(const std::exception_ptr& failure);

/// What an `assert raises` that expected `code` says of `failure`, which it
/// does not take.
std::string refused(std::string_view code, const std::exception_ptr& failure);
} // namespace detail

/// `assert raises(code) { block }` (TST-10): "A matching escaping error
/// passes ...; then continue after the assertion. ... a different code ...
/// fails the test", and so does "Normal completion". A failure it does not
/// take is let out inside the assertion's own, which says what it was.
template <class Block>
void raises(std::string_view code, Block&& block) {
    try {
        std::forward<Block>(block)();
    } catch (const AssertionFailed&) {
        throw;
    } catch (...) {
        const std::exception_ptr failure = std::current_exception();
        if (detail::escaping_code(failure) == code) {
            return;
        }
        std::throw_with_nested(AssertionFailed(detail::refused(code, failure)));
    }
    throw AssertionFailed("the block raised nothing, and `" + std::string{code} + "` was expected");
}

// ---- Traces (TST-4 to TST-7, CEVAL-2, CEVAL-4) ---------------------------

/// A trace written out dense, `[7, _, 9]` (TST-6): cell i is the publication
/// i smallest steps after the run's start, and an empty cell, `_`, is none.
/// A cell of a nullable `T?` that publishes null is a present
/// `std::optional<T>` that holds nothing.
template <class T>
using Dense = std::vector<std::optional<T>>;

/// The key of a timed trace's entry (TST-7): a duration from the run's start,
/// or an instant.
using TraceKey = std::variant<duration, datetime>;

/// A trace written out timed, `[0s: 7, 2s: 9]` (TST-7): its publications in
/// the order written, each with its key.
template <class T>
struct Timed {
    std::vector<std::pair<TraceKey, T>> entries;
};

/// A recording as core's `record` keeps it, and as `replay` takes its items
/// (CEVAL-2, CEVAL-4): each publication's instant and delta, in time order.
/// The delta of a scalar is its value.
template <class T>
using Recorded = list<std::tuple<datetime, T>>;

/// The trace of an eval (CEVAL-4): what its run recorded, and the instant
/// the run started at, which its publications are read against.
template <class T>
struct Recording {
    datetime start{};
    Recorded<T> entries{};
};

/// The smallest step of time (CRUN-3): a microsecond, hgraph's `MIN_TD`.
duration smallest_step() noexcept;

namespace detail {
/// Fails a timed trace whose keys do not strictly increase (TST-7). The
/// catalogue's `trace.key_order` is a source rejection; a key known only as
/// the program runs fails with no code.
[[noreturn]] void unordered_keys();

/// Fails a trace that would publish before its run's start (CEVAL-2).
[[noreturn]] void before_start();

/// The key `key` of a timed trace read against a run that started at
/// `start`, where there is one: a duration from the start.
TraceKey relative_key(const TraceKey& key, const std::optional<datetime>& start);

/// Whether `earlier` is before `later`; fails where the two cannot be
/// ordered, a duration and an instant read against no run.
bool before(const TraceKey& earlier, const TraceKey& later);

/// A trace as its publications, each with its key: a duration from the
/// run's start where there is one to read it against. A dense trace's empty
/// cells are no publication.
template <class T>
std::vector<std::pair<TraceKey, const T*>> relative(const Dense<T>& trace, const std::optional<datetime>&) {
    std::vector<std::pair<TraceKey, const T*>> out;
    for (std::size_t position = 0; position < trace.size(); ++position) {
        if (trace[position].has_value()) {
            out.emplace_back(smallest_step() * static_cast<i64>(position), &*trace[position]);
        }
    }
    return out;
}

template <class T>
std::vector<std::pair<TraceKey, const T*>> relative(const Timed<T>& trace,
                                                    const std::optional<datetime>& start) {
    std::vector<std::pair<TraceKey, const T*>> out;
    for (const auto& [key, value] : trace.entries) {
        TraceKey after = relative_key(key, start);
        if (!out.empty() && !before(out.back().first, after)) {
            unordered_keys();
        }
        out.emplace_back(std::move(after), &value);
    }
    return out;
}

template <class T>
std::vector<std::pair<TraceKey, const T*>> relative(const Recording<T>& trace,
                                                    const std::optional<datetime>&) {
    std::vector<std::pair<TraceKey, const T*>> out;
    for (const auto& [at, value] : trace.entries) {
        out.emplace_back(at - trace.start, &value);
    }
    return out;
}

template <class T>
std::optional<datetime> start_of(const Recording<T>& trace) {
    return trace.start;
}
template <class Trace>
std::optional<datetime> start_of(const Trace&) {
    return std::nullopt;
}

template <class Trace>
struct cell_of;
template <class T>
struct cell_of<Dense<T>> {
    using type = T;
};
template <class T>
struct cell_of<Timed<T>> {
    using type = T;
};
template <class T>
struct cell_of<Recording<T>> {
    using type = T;
};
} // namespace detail

/// Whether two traces are the same (TST-4): they have the same publications,
/// each as long after its run's start as the other's, and equal by the
/// language's equality (OPS-2: a NaN is equal to nothing). "Trailing
/// placeholders have no effect" (TST-6). A timed trace's instants are read
/// against the run the other trace is of, where it is one.
template <class Left, class Right>
bool same(const Left& left, const Right& right) {
    using T = typename detail::cell_of<Left>::type;
    static_assert(std::is_same_v<T, typename detail::cell_of<Right>::type>,
                  "hglrt::testing::same: two traces of one type");
    std::optional<datetime> start = detail::start_of(left);
    if (!start.has_value()) {
        start = detail::start_of(right);
    }
    const auto ours = detail::relative(left, start);
    const auto theirs = detail::relative(right, start);
    if (ours.size() != theirs.size()) {
        return false;
    }
    for (std::size_t index = 0; index < ours.size(); ++index) {
        if (ours[index].first != theirs[index].first ||
            !eq__T_T(*ours[index].second, *theirs[index].second)) {
            return false;
        }
    }
    return true;
}

/// The items core's `replay` is given for the dense trace `trace` of a run
/// that starts at `start` (CEVAL-2): "dense position i is the start plus i
/// smallest steps. A dense placeholder locates any later entries and adds no
/// replay entry or run bound."
template <class T>
Recorded<T> replay_items(const Dense<T>& trace, datetime start) {
    Recorded<T> items;
    for (const auto& [key, value] : detail::relative(trace, start)) {
        items.emplace_back(add__datetime_duration(start, std::get<duration>(key)), *value);
    }
    return items;
}

/// The items `replay` is given for the timed trace `trace` (CEVAL-2, TST-7):
/// each entry at its instant, a duration key from the start. Its keys
/// strictly increase, and none is before the start.
template <class T>
Recorded<T> replay_items(const Timed<T>& trace, datetime start) {
    Recorded<T> items;
    for (const auto& [key, value] : detail::relative(trace, start)) {
        const duration after = std::get<duration>(key);
        if (after < duration::zero()) {
            detail::before_start();
        }
        items.emplace_back(add__datetime_duration(start, after), *value);
    }
    return items;
}

// ---- Runs (CRUN-1 to CRUN-8, CEVAL-1, CEVAL-6) ---------------------------

/// The key a run's recording is put under (CRUN-8).
inline constexpr std::string_view run_output = "run.output";

/// The earliest start (CRUN-3): "one smallest step after
/// `1970-01-01T00:00:00Z`", hgraph's `MIN_ST`, which a simulation of hgraph
/// runs from.
datetime earliest_start() noexcept;

/// The bounds a run's configuration, core's `RunConfig`, gives it (CRUN-2):
/// each null where the configuration gives none.
struct Bounds {
    std::optional<datetime> start{};
    std::optional<datetime> end{};
    std::optional<duration> length{};
};

/// The start a run in simulation has, and the end its bounds give it, none
/// where they give none (CRUN-2: "A null `start` is the earliest start in
/// simulation ... `length` gives the end counted from the start the run has
/// ... A `RunConfig` with both an `end` and a `length` is refused before the
/// run's graph is built, an HGL error with code `run.end_and_length`").
/// A start before the earliest start fails.
std::pair<datetime, std::optional<datetime>> resolved(const Bounds& bounds);

/// What wires a run's entry: given the run's graph and the instant it
/// starts at, it wires the entry's nodes, core's `record` among them.
using Entry = std::function<void(hgraph::Wiring& wiring, datetime start)>;

/// A run that has stopped (CRUN-1): its start, and its store (CRUN-7), which
/// holds what its nodes put there.
class Run {
  public:
    /// A run from `start` that ran no cycle, because its end was its start
    /// or before it: its graph was not built, and its store is empty.
    explicit Run(datetime start);

    /// A run from `start` that its executor `executor` has run.
    Run(datetime start, hgraph::GraphExecutorValue executor);

    /// The instant the run started at.
    [[nodiscard]] datetime start() const noexcept { return start_; }

    /// Whether its graph was built and run: false for a run with no cycle.
    [[nodiscard]] bool built() const noexcept { return executor_.has_value(); }

    /// Whether `key` holds a value in the run's store.
    [[nodiscard]] bool has(std::string_view key) const;

    /// A copy of the value under `key` in the run's store, of the type `V`;
    /// one that is absent or of another type fails (CAP-6).
    template <class V>
    [[nodiscard]] V get(std::string_view key) const {
        if (!executor_.has_value()) {
            hglrt::detail::store_missing(key, std::source_location::current());
        }
        return get_GlobalState_str<V>(store(), key);
    }

  private:
    [[nodiscard]] hgraph::GlobalStateView store() const;

    datetime start_;
    std::optional<hgraph::GraphExecutorValue> executor_;
};

/// Runs the entry `entry` in simulation within `bounds` (CRUN-1: "It builds
/// the entry's graph, starts it, evaluates zero or more cycles, and stops it,
/// once each"; CRUN-2: "`start` is inclusive and `end` exclusive"). A run
/// whose end is not after its start runs no cycle, and its graph is not
/// built; its entry is still wired, so that what the entry checks as it is
/// wired, the traces it replays, is checked. A failure of the run escapes,
/// as hgraph gives it.
Run run(const Entry& entry, const Bounds& bounds = {});

/// The recording that the run `ran` put under `"run.output"` (CEVAL-4).
/// CEVAL-6: "A failed run or a missing recording makes eval fail."
template <class T>
Recording<T> recording(const Run& ran) {
    if (!ran.has(run_output)) {
        fail_uncoded("the run stopped with no recording");
    }
    return Recording<T>{ran.start(), ran.get<Recorded<T>>(run_output)};
}

/// `eval(...)` (CEVAL-1): the run of the entry `entry` within `bounds`, and
/// the trace it recorded.
template <class T>
Recording<T> evaluate(const Entry& entry, const Bounds& bounds = {}) {
    const Run ran = run(entry, bounds);
    if (!ran.built()) {
        // CEVAL-4: the recording starts empty, and no cycle added to it.
        return Recording<T>{ran.start(), {}};
    }
    return recording<T>(ran);
}

// ---- The host ------------------------------------------------------------

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
/// results file, and the name of each test to run, in the order to run
/// them. Gives 0 once the results are written, and 2 where the command
/// line is not that or the results file cannot be written.
int main(int argc, char** argv);

} // namespace hglrt::testing
