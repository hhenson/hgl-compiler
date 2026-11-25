// The test host: what the generated code of a module's tests needs, beside
// what the entry unit of its executable needs (host.h): assertions, traces
// and their comparison, and runs.
//
// A test's eval is a run (CEVAL-1): the compiler writes the entry, which
// wires core's `replay` to each temporal input, the target, and core's
// `record` to its result under `"run.output"`; `evaluate` builds the entry's
// graph, runs it with hgraph's executor, in simulation or, in an integration
// test, in real time (TST-17), and reads the recording from the run's store
// once it has stopped.
#pragma once

#include <hglrt/capabilities.h>
#include <hglrt/failure.h>
#include <hglrt/host.h>
#include <hglrt/scalar.h>
#include <hglrt/temporal.h>
#include <hglrt/traces.h>
#include <hglrt/values.h>

#include <hgraph/runtime/executor.h>
#include <hgraph/runtime/global_state.h>
#include <hgraph/types/generated.h>

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

// ---- Comparing traces (TST-4) ---------------------------------------------

/// `assert left == right` of two traces (TST-4): fails the test where they
/// differ, saying where they first do and what each has there.
template <class Left, class Right>
void expect_same(const Left& left, const Right& right) {
    if (std::optional<std::string> differs = difference(left, right); differs.has_value()) {
        throw AssertionFailed(*differs);
    }
}

/// `assert left != right` of two traces (TST-4): fails the test where they
/// are the same.
template <class Left, class Right>
void expect_different(const Left& left, const Right& right) {
    if (!difference(left, right).has_value()) {
        throw AssertionFailed("the two traces are the same");
    }
}

// ---- Runs (CRUN-1 to CRUN-8, CEVAL-1, CEVAL-6) ---------------------------

using ::hglrt::run_output;

/// How a run keeps time (RUN-3): a simulation's cycles are at the times its
/// nodes ask for, one after the other; a run in real time, an integration
/// test's `run(call, config)` (TST-17), is at the host clock's.
enum class Mode { simulation, real_time };

/// The bounds a run's configuration, core's `RunConfig`, gives it (CRUN-2):
/// each null where the configuration gives none.
struct Bounds {
    std::optional<datetime> start{};
    std::optional<datetime> end{};
    std::optional<duration> length{};
};

/// The start a run of `mode` has, and the end its bounds give it, none
/// where they give none (CRUN-2: "A null `start` is the earliest start in
/// simulation and the host clock's present time in real time. ... `length`
/// gives the end counted from the start the run has ... A `RunConfig` with
/// both an `end` and a `length` is refused before the run's graph is built,
/// an HGL error with code `run.end_and_length`"). CRUN-3: "a run's start is
/// no earlier than the first [`earliest_start`] and its end no later than the
/// second [`latest_end`]": a start before the one and an end, given or from
/// a length, after the other each fail with `run.bounds`.
std::pair<datetime, std::optional<datetime>> resolved(const Bounds& bounds, Mode mode = Mode::simulation);

/// What wires a run's entry: given the run's graph and the instant it
/// starts at, it wires the entry's nodes, core's `record` among them.
using Entry = std::function<void(hgraph::Wiring& wiring, datetime start)>;

/// A run that has stopped (CRUN-1): its start, and its store (CRUN-7), which
/// holds what its nodes put there.
class Run {
  public:
    /// A run from `start` that its executor `executor` has run.
    Run(datetime start, hgraph::GraphExecutorValue executor);

    /// The instant the run started at.
    [[nodiscard]] datetime start() const noexcept { return start_; }

    /// Whether `key` holds a value in the run's store.
    [[nodiscard]] bool has(std::string_view key) const;

    /// A copy of the value under `key` in the run's store, of the type `V`;
    /// one that is absent or of another type fails (CAP-6).
    template <class V>
    [[nodiscard]] V get(std::string_view key) const {
        return get_GlobalState_str<V>(GlobalState{store()}, key);
    }

  private:
    [[nodiscard]] hgraph::GlobalStateView store() const;

    datetime start_;
    hgraph::GraphExecutorValue executor_;
};

/// Runs the entry `entry` within `bounds`, in simulation or in real time
/// (CRUN-1: "It builds the entry's graph, starts it, evaluates zero or more
/// cycles, and stops it, once each"; CRUN-2: "`start` is inclusive and `end`
/// exclusive"). A run with no end ends at the latest end (CRUN-3: "no cycle
/// runs at" `forever`). A run whose end is not after its start runs no
/// cycle. Its graph is built, started and stopped. A run of a test
/// whose time limit has passed (TST-17, host.h) is stopped, at once where it
/// starts after the limit, and ends as it does at its end. A failure of the
/// run escapes, as hgraph gives it.
Run run(const Entry& entry, const Bounds& bounds = {}, Mode mode = Mode::simulation);

/// The recording that the run `ran` put under `"run.output"`, of a result
/// whose type is `T` (CEVAL-4). CEVAL-6: "A failed run or a missing
/// recording makes eval fail."
template <class T>
Recording<T> recording(const Run& ran) {
    if (!ran.has(run_output)) {
        fail_uncoded("the run stopped with no recording");
    }
    return Recording<T>{ran.start(), ran.get<Recorded<delta_t<T>>>(run_output)};
}

namespace detail {
/// Where the values the result of the eval of a `T` under way held are kept,
/// on the thread the eval runs on; none outside an eval.
template <class T>
inline thread_local Held<T>* holding = nullptr;

/// Keeps the values held in `held` while it lives.
template <class T>
class Holding {
  public:
    explicit Holding(Held<T>& held) : before_(holding<T>) { holding<T> = &held; }
    ~Holding() { holding<T> = before_; }
    Holding(const Holding&) = delete;
    Holding& operator=(const Holding&) = delete;

  private:
    Held<T>* before_;
};
} // namespace detail

/// Keeps `value`, the value the result of the eval under way holds once it
/// has published at `at`, or none where a part of it is not valid. The entry
/// of an eval whose result's delta is no T, a tuple's or a list's, wires a
/// node that calls this for each publication of the result, so that a cell
/// written whole is compared with it (TST-5: "one written as a T [is
/// compared] with the value the result holds once its cycle has
/// published").
template <class T>
void hold(datetime at, std::optional<T> value) {
    if (detail::holding<T> != nullptr) {
        detail::holding<T>->emplace_back(at, std::move(value));
    }
}

/// `eval(...)` (CEVAL-1) of a target whose result is a T: the run of the
/// entry `entry` within `bounds`, and the trace it recorded, with the values
/// the result held where the entry gave them (`hold`).
template <class T>
Recording<T> evaluate(const Entry& entry, const Bounds& bounds = {}, Mode mode = Mode::simulation) {
    Held<T> held;
    Recording<T> out;
    {
        const detail::Holding<T> keeping(held);
        const Run ran = run(entry, bounds, mode);
        out = recording<T>(ran);
    }
    if (!held.empty()) {
        out.held = std::move(held);
    }
    return out;
}

} // namespace hglrt::testing
