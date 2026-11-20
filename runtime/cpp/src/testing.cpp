// The test host's assertions, traces and runs (testing.h, traces.h).
#include "host_state.h"
#include "nested.h"

#include <hglrt/conversions.h>
#include <hglrt/testing.h>

#include <hgraph/runtime/graph.h>
#include <hgraph/runtime/lifecycle_observer.h>

#include <chrono>
#include <cstdint>
#include <format>
#include <mutex>
#include <optional>
#include <stdexcept>
#include <string>

namespace hglrt::testing {

std::atomic<std::uint_least32_t> detail::running_line{0};

// ---- Assertions --------------------------------------------------------------

void check(bool holds) {
    if (!holds) {
        throw AssertionFailed("the assertion does not hold");
    }
}

void statement(std::source_location where) noexcept {
    detail::running_line = where.line();
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

std::string detail::described(const std::exception_ptr& failure) {
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

std::string detail::refused(std::string_view code, const std::exception_ptr& failure) {
    return std::format("the block raised {}, and `{}` was expected", described(failure), code);
}

// ---- Traces --------------------------------------------------------------------

duration smallest_step() noexcept {
    return hgraph::MIN_TD;
}

datetime earliest_start() noexcept {
    return hgraph::MIN_ST;
}

datetime latest_end() noexcept {
    return datetime{std::chrono::sys_days{std::chrono::year{2262} / std::chrono::April / 11}} -
           smallest_step();
}

void detail::unordered_keys() {
    fail_uncoded("the keys of a timed trace must strictly increase");
}

void detail::before_start() {
    fail_uncoded("a timed trace has a key before the run's start");
}

void detail::nothing_held() {
    throw std::logic_error(
        "hglrt: a cell written whole is compared with an eval whose entry kept no value its "
        "result held (hglrt::testing::hold)");
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

std::string detail::key_text(const TraceKey& key) {
    if (const duration* after = std::get_if<duration>(&key)) {
        return scalar_text(*after) + " from the start";
    }
    return scalar_text(std::get<datetime>(key));
}

namespace {

/// A time of day of `microseconds` from midnight: `09:30`, `09:30:05`,
/// `09:30:05.25`, as the language writes one (LEX-11).
std::string clock_text(std::int64_t microseconds) {
    const std::int64_t minutes = microseconds / 60'000'000;
    const std::int64_t rest = microseconds % 60'000'000;
    std::string text = std::format("{:02}:{:02}", minutes / 60, minutes % 60);
    if (rest != 0) {
        text += std::format(":{:02}", rest / 1'000'000);
        if (const std::int64_t fraction = rest % 1'000'000; fraction != 0) {
            std::string digits = std::format("{:06}", fraction);
            digits.erase(digits.find_last_not_of('0') + 1);
            text += "." + digits;
        }
    }
    return text;
}

/// A day and the microseconds into it of the count `microseconds` from the
/// epoch, as the language writes a date and a time: `2026-09-03T09:30`.
std::string day_and_clock_text(std::int64_t microseconds) {
    const std::int64_t day = 86'400'000'000;
    std::int64_t days = microseconds / day;
    std::int64_t into = microseconds % day;
    if (into < 0) {
        into += day;
        --days;
    }
    const std::chrono::year_month_day calendar{std::chrono::sys_days{std::chrono::days{days}}};
    return std::format("{:%F}T{}", calendar, clock_text(into));
}

} // namespace

std::string detail::scalar_text(i64 value) {
    return hglrt::detail::number_text(value);
}

std::string detail::scalar_text(f64 value) {
    return hglrt::detail::number_text(value);
}

std::string detail::scalar_text(bool value) {
    return value ? "true" : "false";
}

std::string detail::scalar_text(const str& value) {
    std::string text = "\"";
    for (const char character : value) {
        const auto code = static_cast<unsigned char>(character);
        switch (character) {
        case '"':
            text += "\\\"";
            break;
        case '\\':
            text += "\\\\";
            break;
        case '\n':
            text += "\\n";
            break;
        case '\t':
            text += "\\t";
            break;
        case '\r':
            text += "\\r";
            break;
        default:
            if (code < 0x20 || code == 0x7f) {
                text += std::format("\\u{{{:x}}}", code);
            } else {
                text += character;
            }
        }
    }
    return text + "\"";
}

std::string detail::scalar_text(const bytes& value) {
    std::string text = "bytes(";
    for (std::size_t at = 0; at < value.data.size(); ++at) {
        text += std::format("{}{}", at == 0 ? "" : ", ", static_cast<unsigned char>(value.data[at]));
    }
    return text + ")";
}

std::string detail::scalar_text(date value) {
    return std::format("@{:%F}", value);
}

std::string detail::scalar_text(time value) {
    return "@" + clock_text(value.microseconds);
}

std::string detail::scalar_text(datetime value) {
    return "@" + day_and_clock_text(value.time_since_epoch().count()) + "Z";
}

std::string detail::scalar_text(duration value) {
    // In the largest unit that holds it whole (LEX-11: `d`, `h`, `m`, `s`,
    // `ms`, `us`).
    const std::int64_t count = value.count();
    if (count == 0) {
        return "0s";
    }
    constexpr std::pair<std::int64_t, std::string_view> units[] = {
        {86'400'000'000, "d"}, {3'600'000'000, "h"}, {60'000'000, "m"}, {1'000'000, "s"}, {1'000, "ms"}};
    for (const auto& [size, unit] : units) {
        if (count % size == 0) {
            return std::format("{}{}", count / size, unit);
        }
    }
    return std::format("{}us", count);
}

std::string detail::scalar_text(const civil_datetime& value) {
    return "@" + day_and_clock_text(value.epoch_microseconds());
}

std::string detail::scalar_text(const zoned_datetime& value) {
    return "@" + hgraph::format_zoned_datetime(value);
}

std::string detail::scalar_text(const tenor& value) {
    return hglrt::detail::tenor_text(value);
}

// ---- Runs ----------------------------------------------------------------------

namespace {

/// The runs of the test under way, which its host stops once its limit has
/// passed (TST-17): the executor of the run under way, whether runs are
/// watched (a run made while they are has `StopAtStart` observe it), and
/// whether they are to stop.
struct Runs {
    std::mutex mutex;
    std::optional<hgraph::GraphExecutorView> under_way;
    std::atomic<bool> watched{false};
    std::atomic<bool> stopping{false};
};

Runs& runs() {
    static Runs kept;
    return kept;
}

/// Asks the run under way to stop, where runs are to stop.
void stop_if_stopping() noexcept {
    Runs& all = runs();
    const std::lock_guard lock(all.mutex);
    if (all.stopping && all.under_way.has_value()) {
        all.under_way->request_stop();
    }
}

/// Asks a run to stop as it starts, where runs are to stop. It stands in for
/// a stop that holds across the start of `run()`: hgraph 0.9.12 clears
/// `stop_requested` as `run()` begins (`run_storage` in its executor), so a
/// stop asked for before then runs every cycle, in simulation and in real
/// time alike.
class StopAtStart final : public hgraph::LifecycleObserver {
  public:
    void on_after_start_graph(const hgraph::GraphView&) override { stop_if_stopping(); }
};

/// The run of `executor` as the one under way while it lives.
class UnderWay {
  public:
    explicit UnderWay(hgraph::GraphExecutorView executor) {
        const std::lock_guard lock(runs().mutex);
        runs().under_way.emplace(std::move(executor));
    }
    ~UnderWay() {
        const std::lock_guard lock(runs().mutex);
        runs().under_way.reset();
    }
    UnderWay(const UnderWay&) = delete;
    UnderWay& operator=(const UnderWay&) = delete;
};

} // namespace

void detail::runs_watched(bool watched) noexcept {
    runs().watched = watched;
}

void detail::stop_runs() noexcept {
    runs().stopping = true;
    stop_if_stopping();
}

void detail::stops_withdrawn() noexcept {
    runs().stopping = false;
}

std::pair<datetime, std::optional<datetime>> resolved(const Bounds& bounds, Mode mode) {
    if (bounds.end.has_value() && bounds.length.has_value()) {
        fail(codes::end_and_length, "a run's configuration gives both an `end` and a `length`");
    }
    const datetime unstated = mode == Mode::simulation
                                  ? earliest_start()
                                  : std::chrono::floor<duration>(hgraph::engine_clock::now());
    const datetime start = bounds.start.value_or(unstated);
    // CRUN-3: "a run's start is no earlier than the first [`earliest_start`]
    // and its end no later than the second [`latest_end`]". CRUN-3 reserves
    // the epoch itself for `never`. Neither rule names the failure of a run
    // outside them: each fails with no code.
    if (start < earliest_start()) {
        fail_uncoded("the run's start is before the earliest evaluation time");
    }
    const auto past_latest_end = [] { fail_uncoded("the run's end is after the latest end"); };
    std::optional<datetime> end = bounds.end;
    if (bounds.length.has_value()) {
        // A length that puts the end after the latest end, one too long for
        // any instant among them, fails before the sum is taken. The start
        // is after the epoch, so `latest_end() - start` does not overflow.
        if (*bounds.length > latest_end() - start) {
            past_latest_end();
        }
        end = add__datetime_duration(start, *bounds.length);
    }
    if (end.has_value() && *end > latest_end()) {
        past_latest_end();
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

Run run(const Entry& entry, const Bounds& bounds, Mode mode) {
    const auto [start, given_end] = resolved(bounds, mode);
    hgraph::Wiring wiring;
    entry(wiring, start);
    // CRUN-3: "`forever` ... after every [evaluation time]: no cycle runs at
    // either": a run with no end ends at the latest end, which hgraph's own
    // last time is later than.
    const datetime end = given_end.value_or(latest_end());
    if (end <= start) {
        // hgraph has no run that ends where it starts, so none is made.
        return Run(start);
    }
    hgraph::GraphExecutorBuilder builder;
    builder.graph_builder(std::move(wiring).finish())
        .mode(mode == Mode::simulation ? hgraph::GraphExecutorMode::Simulation
                                       : hgraph::GraphExecutorMode::RealTime)
        .start_time(start)
        .end_time(end);
    if (runs().watched) {
        // One for the process: an executor keeps its observers for as long
        // as it lives, and a run's is kept for its store.
        static StopAtStart stopper;
        builder.add_lifecycle_observer(&stopper);
    }
    hgraph::GraphExecutorValue executor = builder.make_executor();
    {
        const UnderWay registered(executor.view());
        executor.view().run();
    }
    return Run(start, std::move(executor));
}

} // namespace hglrt::testing
