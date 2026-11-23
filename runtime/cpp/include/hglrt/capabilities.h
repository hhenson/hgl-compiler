// Core's capabilities (`hgl/native/capabilities.hgl`; core's CAP-1 to
// CAP-7, the language's TIME-8): the native types `Logger`, `Clock`,
// `Scheduler`, `Alarm`, `GlobalState` and `RunConfig`, and the native
// operations on them.
//
// CAP-1: "Each capability but `out` has a native type of core's module ...
// Core constructs none in HGL. A property is read as an ordinary value and is
// not assigned." Each type here is made from the hgraph view a node's hook
// is given for it, which the generated code declares as a parameter of the
// hook and makes the type from at the top of the hook, once:
//
//   | HGL         | made from                                       |
//   | ----------- | ----------------------------------------------- |
//   | Logger      | `hgraph::LoggerView`                            |
//   | Clock       | `hgraph::EvaluationClockView`                   |
//   | Scheduler   | `hgraph::NodeScheduler` and the hook (`Hook`)   |
//   | Alarm       | `hgraph::NodeScheduler` and the hook (`Hook`)   |
//   | GlobalState | `hgraph::GlobalStateView`, of a node's hook or  |
//   |             | of a graph's `Wiring::global_state()` (FN-9)    |
//   | RunConfig   | its fields, where an application or an          |
//   |             | integration test injects it (RUN-6)             |
//
// Each property core declares is a `const` member function of its name,
// `clock.evaluation_time()`. A capability is not retained (TIME-8): each
// type borrows its view, and lives no longer than the hook it is made in.
// The operations are named as the compiler names a native: an exported one
// `fn_` and its name (`fn_log`), a candidate of an operator its name and the
// types of its parameters (`schedule_Alarm_duration`).
#pragma once

#include <hglrt/failure.h>
#include <hglrt/values.h>

#include <hgraph/runtime/global_state.h>
#include <hgraph/types/generated.h>
#include <hgraph/types/value/value.h>

#include <cstddef>
#include <optional>
#include <source_location>
#include <string>
#include <string_view>

namespace hglrt {

/// The hook a capability is used in (TIME-5): what may be asked of a
/// scheduler or an alarm depends on it (TIME-8: "A wake asked for with a
/// `duration` takes one more than zero, but in `start`, which takes zero
/// too").
enum class Hook { start, evaluation, stop };

// ---- The logger (CAP-2) ------------------------------------------------------

/// Core's `LogLevel`, its members in core's order (CAP-2: "`debug`, `info`,
/// `warning`, or `error`").
enum class LogLevel { debug, info, warning, error };

/// The names of core's `LogLevel` (values.h, `EnumNames`).
constexpr EnumNames<LogLevel, 4> hgl_enum(LogLevel) noexcept {
    return {"core::LogLevel",
            {{LogLevel::debug, "debug"},
             {LogLevel::info, "info"},
             {LogLevel::warning, "warning"},
             {LogLevel::error, "error"}}};
}

/// `Logger` (CAP-2), what `inject logger` binds: the run's logger, as the
/// hook's `hgraph::LoggerView` gives it.
class Logger {
  public:
    explicit Logger(hgraph::LoggerView view) noexcept : view_(view) {}

    /// `logger.level` (CAP-2: "the logger's current level"): the least level
    /// a record must be at to be written. hgraph's trace level is below
    /// `debug`, and is `debug`; its levels above `error`, and a logger that
    /// writes nothing, are `error`.
    [[nodiscard]] LogLevel level() const noexcept;

    /// The hgraph view it is made from.
    [[nodiscard]] const hgraph::LoggerView& view() const noexcept { return view_; }

  private:
    hgraph::LoggerView view_;
};

/// `log(logger: Logger, level: LogLevel, message: str)` (CAP-2): "writes one
/// record of `message` at `level`", through hgraph's logger, as it is: a
/// brace in it is text. A record below the logger's level is dropped.
void fn_log(const Logger& logger, LogLevel level, std::string_view message);

// ---- The clock (CAP-3) -------------------------------------------------------

/// `Clock` (CAP-3), what `inject clock` binds: the engine's clock, as the
/// hook's `hgraph::EvaluationClockView` gives it. "It has no operation."
class Clock {
  public:
    explicit Clock(hgraph::EvaluationClockView view) noexcept : view_(view) {}

    /// `clock.evaluation_time`: the time of the current cycle. In `start`
    /// it is the run's start time, and in `stop` the last cycle's.
    [[nodiscard]] datetime evaluation_time() const noexcept { return view_.evaluation_time(); }
    /// `clock.now`: the engine's estimate of the wall clock's time.
    [[nodiscard]] datetime now() const noexcept { return view_.now(); }
    /// `clock.cycle_time`: the time spent in the current cycle so far.
    [[nodiscard]] duration cycle_time() const noexcept { return view_.cycle_time(); }
    /// `clock.next_cycle_evaluation_time`: the earliest time a cycle after
    /// this one could have.
    [[nodiscard]] datetime next_cycle_evaluation_time() const noexcept {
        return view_.next_cycle_evaluation_time();
    }

    /// The hgraph view it is made from.
    [[nodiscard]] const hgraph::EvaluationClockView& view() const noexcept { return view_; }

  private:
    hgraph::EvaluationClockView view_;
};

// ---- The scheduler (CAP-4) ---------------------------------------------------

/// The dropped datetime wakes of one node in a run (TIME-8, CAP-4, CAP-5).
class DroppedWakes {
    friend struct DroppedWakeLog;

  private:
    /// Warn on the first drop and count any further drops.
    void record(const hgraph::NodeView& node) const;
    /// At stop, report the count of drops after the initial warning.
    void finish(const hgraph::NodeView& node) const;

    // The count is diagnostic state. Updating it needs no hgraph mutation.
    mutable std::size_t count_{0};
};

/// Borrowed access to a node's drop count. The state is read only to warn
/// of an actual drop or report the count at stop.
struct DroppedWakeLog {
    const hgraph::NodeView* node{nullptr};
    const DroppedWakes& (*read)(const hgraph::NodeView&){nullptr};

    template <class State, DroppedWakes State::*field>
    [[nodiscard]] static DroppedWakeLog bind(const hgraph::NodeView& node) noexcept {
        return {&node, [](const hgraph::NodeView& owner) -> const DroppedWakes& {
                    return hgraph::State<State>{owner.state()}.ref().*field;
                }};
    }

    /// Record a drop where the timer is bound to the node's state.
    void record() const noexcept;
    /// Report any further drops at this node's stop.
    void finish() const noexcept;
};

/// Report later drops on leaving a user-written stop hook, including drops
/// requested by that hook before an early return.
class DroppedWakeReport {
  public:
    explicit DroppedWakeReport(DroppedWakeLog log) noexcept : log_(log) {}
    DroppedWakeReport(const DroppedWakeReport&) = delete;
    DroppedWakeReport& operator=(const DroppedWakeReport&) = delete;
    ~DroppedWakeReport() { log_.finish(); }

  private:
    DroppedWakeLog log_;
};

/// `Scheduler` (CAP-4), what `inject scheduler` binds: the node's own
/// scheduler, hgraph's `NodeScheduler`, in the hook `hook`.
///
/// CAP-4 has a wake pending until it is delivered: `is_scheduled` is "True
/// while a wake is pending". hgraph's scheduler has it so: in the
/// evaluation a wake brings about, that wake is due and no longer pending,
/// so `is_scheduled`, `next_scheduled_time` and `un_schedule` with no tag
/// see only the wakes to come, and `is_scheduled_now` stays true for the
/// rest of that evaluation after `un_schedule` or `reset` in it (TIME-9).
///
/// hgraph marks the node for the earliest wake asked of it, and a wake
/// cancelled (`un_schedule`, `reset`) or replaced under its tag leaves the
/// mark: the node is evaluated then with no wake due and, but where an
/// input ticks then too, no input modified. That evaluation is no wake
/// (TIME-3: "A wake by the scheduler or alarm wakes the handlers ..."), so
/// the generated code runs no handler in it.
class Scheduler {
  public:
    Scheduler(hgraph::NodeScheduler view, Hook hook, DroppedWakeLog drop_log = {});

    /// `scheduler.next_scheduled_time` (CAP-4): the time of the earliest
    /// pending wake, and null with none.
    [[nodiscard]] std::optional<datetime> next_scheduled_time() const noexcept;
    /// `scheduler.is_scheduled` (CAP-4): whether a wake is pending.
    [[nodiscard]] bool is_scheduled() const noexcept { return view_.is_scheduled(); }
    /// `scheduler.is_scheduled_now` (CAP-4): "whether a wake asked of the
    /// scheduler is due in the current evaluation, whether or not an input
    /// has ticked in it", which the language's `scheduled()` is (TIME-9). In
    /// `start` and `stop`, which are no evaluation, whether a wake is due at
    /// the hook's time, as hgraph has it: in `start`, one asked for with
    /// `0s` is.
    [[nodiscard]] bool is_scheduled_now() const noexcept { return view_.is_scheduled_now(); }

    /// The hgraph scheduler it is made from.
    [[nodiscard]] const hgraph::NodeScheduler& view() const noexcept { return view_; }
    /// The hook it is used in.
    [[nodiscard]] Hook hook() const noexcept { return hook_; }

    /// Record a stale dated request unless it is to be handled by the wall clock.
    [[nodiscard]] bool record_drop(bool on_wall_clock) const;

  private:
    hgraph::NodeScheduler view_;
    Hook hook_;
    DroppedWakeLog drop_log_;
};

// The wall-clock option, `on_wall_clock: true`, is hgraph's own, as CAP-4
// has it: in a run in real time the request "is measured from ... the later
// of the evaluation time and `clock.now`", and "A wake of the option that is
// already due where it is asked for ... is not dropped: it comes in the next
// cycle, at the later of that time and the evaluation time plus the engine's
// smallest step, and in `start` at that time"; "In simulation the option
// changes nothing".

/// `schedule(capability: Scheduler, at: duration, tag: str? = null,
/// on_wall_clock: bool = false)` (CAP-4): asks for a wake `at` after the
/// current evaluation time, or with the wall-clock option in real time
/// after the later of it and `clock.now`. TIME-8: "A wake asked for with a
/// `duration` takes one more than zero, but in `start`, which takes zero too
/// ... A duration that is not admitted, a negative one in `start` included,
/// ... fails the call as the program runs, with
/// `schedule.non_positive_duration`". A wake
/// asked for under a tag replaces the pending wake of that tag. A tag is
/// any text, the empty one included: null is no tag (CAP-4: "a `str?` that
/// is null when omitted"), and hgraph keeps no wake under the empty tag, so
/// each is given to hgraph after a mark of its own.
void schedule_Scheduler_duration_str__bool(const Scheduler& capability, duration at,
                                           const std::optional<str>& tag = std::nullopt,
                                           bool on_wall_clock = false,
                                           std::source_location where = std::source_location::current());

/// `schedule(capability: Scheduler, at: datetime, tag: str? = null,
/// on_wall_clock: bool = false)` (CAP-4): asks for a wake at `at`. In graph
/// time, an earlier wake or one at the current time outside `start` is
/// dropped and logged (TIME-8). An equal time in `start` requests the node's
/// first evaluation. Real-time wall-clock requests are handled by hgraph.
void schedule_Scheduler_datetime_str__bool(const Scheduler& capability, datetime at,
                                           const std::optional<str>& tag = std::nullopt,
                                           bool on_wall_clock = false);

/// `un_schedule(scheduler: Scheduler, tag: str? = null)` (CAP-4): "cancel a
/// pending wake": with no tag the earliest, with one the wake asked for
/// under it, where there is one.
void fn_un_schedule(const Scheduler& scheduler, const std::optional<str>& tag = std::nullopt);

/// `reset(scheduler: Scheduler)` (CAP-4): cancels every pending wake.
void fn_reset(const Scheduler& scheduler);

// ---- The alarm (CAP-5) -------------------------------------------------------

/// `Alarm` (CAP-5), what `inject alarm` binds: "a cheaper scheduler: best
/// effort, with nothing tracked, moved, or cancelled", in the hook `hook`.
/// It has no property. "`schedule(alarm, at)` asks for a wake ...; asking
/// again keeps the earliest": its one wake is kept under a tag of its own
/// in the node's own scheduler, hgraph's `NodeScheduler`, which arms the
/// node for it again after an evaluation of the node at an input's tick
/// before the wake is due (hgraph's `SingleShotScheduler` only marks the
/// node, and that evaluation takes the mark away). An alarm made in an
/// evaluation takes the wake due then, which is delivered and pending no
/// longer, as the scheduler takes it: a wake asked for in that evaluation
/// is kept whatever its time.
class Alarm {
  public:
    Alarm(hgraph::NodeScheduler view, Hook hook, DroppedWakeLog drop_log = {});

    /// The hgraph scheduler it is made from.
    [[nodiscard]] const hgraph::NodeScheduler& view() const noexcept { return view_; }
    /// The hook it is used in.
    [[nodiscard]] Hook hook() const noexcept { return hook_; }

    /// Record a stale dated request.
    void record_drop() const;

  private:
    hgraph::NodeScheduler view_;
    Hook hook_;
    DroppedWakeLog drop_log_;
};

/// `schedule(capability: Alarm, at: duration)` (CAP-5): asks for a wake `at`
/// after the current evaluation time. "What may be asked of it is what
/// CAP-4 says may be asked of the scheduler, alike: a `duration` more than
/// zero, or zero in `start`"; another fails with
/// `schedule.non_positive_duration` (TIME-8).
void schedule_Alarm_duration(const Alarm& capability, duration at,
                             std::source_location where = std::source_location::current());

/// `schedule(capability: Alarm, at: datetime)` (CAP-5): asks for a wake at
/// `at`; "a `datetime` before the current evaluation time is dropped", and
/// one at it as the scheduler drops one. A wake still pending at `at` or
/// before it is kept, and one later is replaced: "asking again keeps the
/// earliest".
void schedule_Alarm_datetime(const Alarm& capability, datetime at);

// ---- The global store (CAP-6) ------------------------------------------------

/// `GlobalState` (CAP-6), what `inject global_state` binds: the run's store,
/// "a run-wide store of ordinary values under string keys", as hgraph's
/// `GlobalStateView` gives it to a node's hook and, where a graph is built,
/// to its wiring (FN-9). It has no property.
class GlobalState {
  public:
    explicit GlobalState(hgraph::GlobalStateView view) noexcept : view_(view) {}

    /// The hgraph view it is made from.
    [[nodiscard]] const hgraph::GlobalStateView& view() const noexcept { return view_; }

  private:
    hgraph::GlobalStateView view_;
};

namespace detail {
/// What the store holds for a value of `V` (CAP-6: "a run-wide store of
/// ordinary values under string keys"): the value itself, in a type of the
/// library's own, which hgraph keeps whole as an atomic, one for each `V`.
/// hgraph's own schema of a value is no use here: a struct's is a bundle,
/// which hgraph keeps field by field and gives back as no `V`, and a tuple,
/// a set, a dictionary and a delta have none. So every ordinary value is
/// held alike, and `get` gives back exactly the type that was put.
template <class V>
struct Stored {
    V value{};
};

/// Registers `Stored<V>` with hgraph, the first time one is put.
template <class V>
void store_registered() {
    static const hgraph::ValueTypeMetaData* const registered =
        hgraph::TypeRegistry::instance().register_scalar<Stored<V>>();
    (void)registered;
}

/// The error of a read of the store under `key`, which holds nothing.
error store_missing(std::string_view key);
/// The error of a read or write under `key`, which holds another type.
error store_type(std::string_view key);
} // namespace detail

// CAP-6: "`get<V>(global_state, key)` is the value under `key`, of the type
// the call writes or its place expects; an absent key gives `value.missing_key`.
// `has(global_state, key)` is whether the key holds a value.
// `put(global_state, key, value)` puts one there. A key keeps the type of its
// first value: a `put` of another type is rejected where the two are in one
// program, and fails otherwise. Reading or writing the store wakes no node."

/// `has(store: GlobalState, const key: str) -> bool`: whether `key` holds a
/// value (CAP-6).
bool has_GlobalState_str(const GlobalState& store, std::string_view key);

/// `get<V>(store: GlobalState, const key: str) -> result<V, error>`: a
/// copy of the value or an error (CAP-6). A held null is a success.
template <class V>
result<V> get_store_result(const GlobalState& store, std::string_view key) {
    if (!store.view().contains(key)) {
        return std::unexpected(detail::store_missing(key));
    }
    const detail::Stored<V>* held = store.view().get(key).template try_as<detail::Stored<V>>();
    if (held == nullptr) {
        return std::unexpected(detail::store_type(key));
    }
    return result<V>{std::in_place, held->value};
}

/// The value of a store result where the runtime's own caller requires
/// it (CAP-6, ERR-15).
template <class V>
V get_GlobalState_str(const GlobalState& store, std::string_view key,
                      std::source_location where = std::source_location::current()) {
    return require_result(get_store_result<V>(store, key), where);
}

/// `put<V>(store: GlobalState, const key: str, value: V)`: puts a copy of
/// `value` under `key`, which keeps the type of its first value (CAP-6).
template <class V>
void put_GlobalState_str_V(const GlobalState& store, std::string_view key, const V& value,
                           std::source_location where = std::source_location::current()) {
    if (store.view().contains(key) && store.view().get(key).template try_as<detail::Stored<V>>() == nullptr) {
        raise(detail::store_type(key), where);
    }
    detail::store_registered<V>();
    store.view().set(key, hgraph::Value{detail::Stored<V>{value}});
}

// ---- The run's configuration (CAP-7) -----------------------------------------

/// `RunConfig` (CAP-7), what `inject run_config` binds in an application and
/// in an integration test (the language's RUN-6): the configuration of the run
/// as it is supplied, each field null where it is left out, and not as the run
/// resolves it (`testing::resolved`). It is a value, made where it is injected
/// from what supplies it, and has no operation.
class RunConfig {
  public:
    /// The fields, in the order core declares them.
    struct Fields {
        std::optional<datetime> start{};
        std::optional<datetime> end{};
        std::optional<duration> length{};
    };

    /// The configuration that has `fields`.
    explicit RunConfig(Fields fields) noexcept : fields_(fields) {}

    /// `run_config.start`: the start supplied, null where none is.
    [[nodiscard]] std::optional<datetime> start() const noexcept { return fields_.start; }
    /// `run_config.end`: the end supplied, null where none is.
    [[nodiscard]] std::optional<datetime> end() const noexcept { return fields_.end; }
    /// `run_config.length`: the length supplied, null where none is.
    [[nodiscard]] std::optional<duration> length() const noexcept { return fields_.length; }

  private:
    Fields fields_;
};

} // namespace hglrt
