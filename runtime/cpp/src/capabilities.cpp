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

/// Whether a wake at `at`, asked for in `hook` at the evaluation time
/// `now`, is dropped: one before `now`, and in an evaluation one at it,
/// which no cycle can still run at (CRUN-3).
bool dropped(datetime at, datetime now, Hook hook) {
    return hook == Hook::start ? at < now : at <= now;
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

Scheduler::Scheduler(hgraph::NodeScheduler view, Hook hook) : view_(view), hook_(hook) {
    if (hook_ != Hook::evaluation) {
        return;
    }
    // hgraph's `un_schedule()` with no tag takes its earliest wake, and that
    // wake's tag with it.
    while (view_.is_scheduled_now()) {
        view_.un_schedule();
        due_now_ = true;
    }
}

std::optional<datetime> Scheduler::next_scheduled_time() const noexcept {
    if (!view_.is_scheduled()) {
        return std::nullopt;
    }
    return view_.next_scheduled_time();
}

bool Scheduler::is_scheduled_now() const noexcept {
    return hook_ == Hook::evaluation ? due_now_ : view_.is_scheduled_now();
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
    // hgraph keeps a wall-clock wake already past for the next cycle: CAP-4
    // drops one before the evaluation time on either clock.
    if (at < capability.view().now()) {
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

void schedule_Alarm_datetime(const Alarm& capability, datetime at) {
    // hgraph's graph refuses a time before its evaluation time, and keeps
    // one at it for a cycle already under way.
    if (dropped(at, capability.view().now(), capability.hook())) {
        return;
    }
    capability.view().schedule(at);
}

// ---- The global store ----------------------------------------------------------

bool has_GlobalState_str(const GlobalState& store, std::string_view key) {
    return store.view().contains(key);
}

void detail::store_missing(std::string_view key, std::source_location where) {
    fail_uncoded(std::format("the global store holds nothing under '{}'", key), where);
}

void detail::store_type(std::string_view key, std::source_location where) {
    fail_uncoded(std::format("the global store holds a value of another type under '{}'", key), where);
}

} // namespace hglrt
