// Core's capabilities and the native operations on them (capabilities.h).
#include <hglrt/capabilities.h>
#include <hglrt/temporal.h>

#include <format>
#include <optional>
#include <string>

namespace hglrt {

namespace {

/// hgraph's levels, on spdlog's scale, of core's `LogLevel` members, in
/// core's order.
constexpr int spdlog_levels[] = {1, 2, 3, 4};

/// The tag hgraph keeps a wake asked for under `tag` with: none for none,
/// and otherwise the tag after a mark, so that the empty tag is one too
/// (hgraph keeps no wake under the empty text).
std::optional<std::string> engine_tag(const std::optional<str>& tag) {
    if (!tag.has_value()) {
        return std::nullopt;
    }
    return "#" + *tag;
}

/// Fails unless `at` is a duration a wake may be asked for with in `hook`
/// (TIME-8, CAP-4, CAP-5).
void check_delay(duration at, Hook hook, std::source_location where) {
    if (at < duration::zero()) {
        fail(codes::non_positive_duration, "a wake is asked for with a duration below zero", where);
    }
    if (at == duration::zero() && hook != Hook::start) {
        fail(codes::non_positive_duration,
             "a wake is asked for with a duration of zero, which only `start` takes", where);
    }
}

/// The tag an alarm's one wake is kept under in its node's scheduler: no
/// tag of a scheduler's is the same (`engine_tag`), and a node injects one
/// or the other (TIME-8).
const std::string alarm_tag = "alarm";

/// Whether a wake at `at`, asked for in `hook` at the evaluation time
/// `now`, is dropped: one before `now`, and in an evaluation one at it,
/// which no cycle can still run at (CRUN-3).
bool dropped(datetime at, datetime now, Hook hook) {
    return hook == Hook::start ? at < now : at <= now;
}

/// Borrow the run's logger with this node's provenance, as a LoggerView
/// injection does. A timer only needs it to issue a drop diagnostic.
hgraph::LoggerView logger_of(const hgraph::NodeView& node) {
    const auto graph = node.graph();
    return hgraph::LoggerView{graph.logger(), node.pointer(), graph.logger_ops()};
}

} // namespace

// ---- The logger ----------------------------------------------------------------

LogLevel Logger::level() const noexcept {
    const int level = view_.effective_level();
    if (level <= spdlog_levels[0]) {
        return LogLevel::debug;
    }
    if (level == spdlog_levels[1]) {
        return LogLevel::info;
    }
    if (level == spdlog_levels[2]) {
        return LogLevel::warning;
    }
    return LogLevel::error;
}

void fn_log(const Logger& logger, LogLevel level, std::string_view message) {
    logger.view().log(spdlog_levels[static_cast<int>(level)], message);
}

// ---- The scheduler -------------------------------------------------------------

void DroppedWakes::record(const hgraph::NodeView& node) {
    if (count_++ == 0) {
        logger_of(node).warn("Dropped wake requested at or before the evaluation time");
    }
}

void DroppedWakes::finish(const hgraph::NodeView& node) const {
    if (count_ > 1) {
        logger_of(node).info("Additional dropped wakes totaled {}", count_ - 1);
    }
}

void DroppedWakeLog::record() const noexcept {
    if (apply != nullptr) {
        try {
            apply(*node, Operation::record);
        } catch (...) {
            // A dropped wake is never a failure (TIME-8).
        }
    }
}

void DroppedWakeLog::finish() const noexcept {
    if (apply != nullptr) {
        try {
            apply(*node, Operation::finish);
        } catch (...) {
            // Preserve any failure already raised by the stop hook.
        }
    }
}

Scheduler::Scheduler(hgraph::NodeScheduler view, Hook hook, DroppedWakeLog drop_log)
    : view_(view), hook_(hook), drop_log_(drop_log) {}

bool Scheduler::record_drop(bool on_wall_clock) const {
    if (on_wall_clock && (drop_log_.node == nullptr || drop_log_.node->evaluation_clock().supports_wall_clock())) {
        return false; // Let hgraph handle real-time requests or an unbound view.
    }
    drop_log_.record();
    return true;
}

std::optional<datetime> Scheduler::next_scheduled_time() const noexcept {
    if (!view_.is_scheduled()) {
        return std::nullopt;
    }
    return view_.next_scheduled_time();
}

void schedule_Scheduler_duration_str__bool(const Scheduler& capability, duration at,
                                           const std::optional<str>& tag, bool on_wall_clock,
                                           std::source_location where) {
    check_delay(at, capability.hook(), where);
    const datetime when = add__datetime_duration(capability.view().scheduling_time(on_wall_clock), at, where);
    capability.view().schedule(when, engine_tag(tag), on_wall_clock);
}

void schedule_Scheduler_datetime_str__bool(const Scheduler& capability, datetime at,
                                           const std::optional<str>& tag, bool on_wall_clock) {
    if (dropped(at, capability.view().now(), capability.hook()) && capability.record_drop(on_wall_clock)) {
        return;
    }
    capability.view().schedule(at, engine_tag(tag), on_wall_clock);
}

void fn_un_schedule(const Scheduler& scheduler, const std::optional<str>& tag) {
    if (tag.has_value()) {
        scheduler.view().un_schedule(*engine_tag(tag));
    } else {
        scheduler.view().un_schedule();
    }
}

void fn_reset(const Scheduler& scheduler) {
    scheduler.view().reset();
}

// ---- The alarm -----------------------------------------------------------------

void schedule_Alarm_duration(const Alarm& capability, duration at, std::source_location where) {
    check_delay(at, capability.hook(), where);
    schedule_Alarm_datetime(capability, add__datetime_duration(capability.view().now(), at, where));
}

Alarm::Alarm(hgraph::NodeScheduler view, Hook hook, DroppedWakeLog drop_log)
    : view_(view), hook_(hook), drop_log_(drop_log) {
    if (hook_ != Hook::evaluation) {
        return;
    }
    // hgraph keeps the wake it delivers until the node has run.
    if (view_.tag_is_scheduled_now(alarm_tag)) {
        view_.un_schedule(alarm_tag);
    }
}

void Alarm::record_drop() const {
    drop_log_.record();
}

void schedule_Alarm_datetime(const Alarm& capability, datetime at) {
    // hgraph's scheduler drops a time before its evaluation time, and keeps
    // one at it in `start` alone.
    if (dropped(at, capability.view().now(), capability.hook())) {
        capability.record_drop();
        return;
    }
    // CAP-5: "asking again keeps the earliest".
    const hgraph::NodeScheduler& scheduler = capability.view();
    if (scheduler.has_tag(alarm_tag) && scheduler.tag_time(alarm_tag) <= at) {
        return;
    }
    scheduler.schedule(at, alarm_tag);
}

// ---- The global store ----------------------------------------------------------

bool has_GlobalState_str(const GlobalState& store, std::string_view key) {
    return store.view().contains(key);
}

error detail::store_missing(std::string_view key) {
    return make_error(std::format("the global store holds nothing under '{}'", key), std::string{codes::missing_key});
}

error detail::store_type(std::string_view key) {
    return make_error(std::format("the global store holds a value of another type under '{}'", key));
}

} // namespace hglrt
