// A window, `rolling_window<V, Max, Min>` (the language's TYPE-11, core's
// END-9): hgraph's `TSW` of a tick count and `TSWDuration` of a duration,
// read as HGL reads one.
//
// hgraph's window is HGL's in what it holds and when it ticks, as 0.9.13
// has it: it holds its arrivals oldest first; one of a tick count drops its
// oldest value only at an arrival; in one of a duration a value is held
// while its age is at most the maximum, and leaves one smallest step later,
// in a cycle of its own, a value arriving then or not, where the window
// ticks (hgraph's runtime wakes the window's owner's output for each exit);
// `valid` holds once it has held a value, emptied by exits too; `all_valid`
// holds at its minimum; its delta is the value that arrived, and none in a
// cycle in which a value only left; it keeps the value that left in the
// cycle, `removed_value`; and `last_modified` at an exit is the exit's time.
//
// One read differs, and is guarded here: TYPE-11 has "A read of [a window's
// value] before the window is `all_valid` fails with `value.unset_read`, or
// gives an `err` of an `error` with that code where its place takes a
// `result<list<V>, error>`", where hgraph reads the values held whatever
// the minimum. `window_value` and `window_value_checked` stand for that
// read, and `tests/hgraph_window_read.cpp` shows hgraph's own.
#pragma once

#include <hglrt/checked.h>
#include <hglrt/endpoints.h>
#include <hglrt/failure.h>
#include <hglrt/values.h>

#include <hgraph/types/generated.h>

#include <cstddef>
#include <cstdint>
#include <expected>
#include <source_location>
#include <string>
#include <type_traits>

namespace hglrt {

namespace detail {
/// What a window of `V` is read as, for its tick count's form and its
/// duration's alike (END-3, END-9).
template <class V>
struct window_read {
    /// END-9: "`delta_value(x)` of a window is the value that arrived in
    /// the cycle (the language's TYPE-16)", a V.
    using delta_type = V;

    /// The value that arrived in the cycle: hgraph's delta of a window is
    /// that value.
    template <class E>
    static V delta(const E& window) {
        return window.delta_value().template checked_as<V>();
    }

    /// END-9: "In a cycle in which a value only leaves a window of a
    /// duration, the window ticks and has no delta": hgraph's delta of a
    /// window holds nothing there.
    template <class E>
    static bool has_delta(const E& window) {
        return window.delta_value().has_value();
    }

    /// END-9: "`all_valid(x)` where it holds at least its minimum: of a
    /// tick count, that many values; of a duration, values whose arrival
    /// times span at least the minimum ... A minimum of zero is held by the
    /// first value, and an empty window is not all valid": hgraph's own.
    template <class E>
    static bool all_valid(const E& window) {
        return window.all_valid();
    }
};

/// Whether the hgraph schema `S` is a window's.
template <class S>
struct is_window : std::false_type {};
template <class V, std::size_t Max, std::size_t Min>
struct is_window<hgraph::TSW<V, Max, Min>> : std::true_type {};
template <class V, std::int64_t Max, std::int64_t Min>
struct is_window<hgraph::TSWDuration<V, Max, Min>> : std::true_type {};

/// The values the window `window` holds, oldest first, whatever its
/// minimum.
template <class E>
list<typename schema_of<E>::value_type> held(const E& window) {
    using V = typename schema_of<E>::value_type;
    const hgraph::TSWDataView data = window.data_view();
    list<V> values;
    values.reserve(data.size());
    for (std::size_t index = 0; index < data.size(); ++index) {
        values.push_back(data.at(index).template checked_as<V>());
    }
    return values;
}
} // namespace detail

/// A window of a tick count, hgraph's `TSW` (END-9).
template <class V, std::size_t Max, std::size_t Min>
struct endpoint_read<hgraph::TSW<V, Max, Min>> : detail::window_read<V> {};

/// A window of a duration, hgraph's `TSWDuration` (END-9).
template <class V, std::int64_t Max, std::int64_t Min>
struct endpoint_read<hgraph::TSWDuration<V, Max, Min>> : detail::window_read<V> {};

/// An hgraph view of a window: an input, an output, or a child of either.
template <class E>
concept window_view = detail::is_window<schema_of<E>>::value;

/// The V of the window view `E`.
template <window_view E>
using window_value_t = typename schema_of<E>::value_type;

/// `return value` or `out = value` to a window result (TIME-7: "A result of
/// `rolling_window<V, Max, Min>` takes a V, which arrives, and nothing
/// else"; END-9: "A node publishes a V to a window, which arrives"): `value`
/// arrives at the window, which drops its oldest value where it is a full
/// window of a tick count.
template <window_view E>
void window_push(const E& window, const window_value_t<E>& value) {
    window.push(value);
}

/// A read of a window's value in a hook or for a lifted helper (TYPE-11,
/// END-9): "a `list<V>` of the values held, oldest first. Before the window
/// is all valid that read fails with `value.unset_read`". hgraph reads the
/// values held at any size, which this guards.
template <window_view E>
list<window_value_t<E>> window_value(const E& window,
                                     std::source_location where = std::source_location::current()) {
    if (!fn_all_valid(window)) {
        unset_read("a window that is not all valid is read", where);
    }
    return detail::held(window);
}

/// The same read where its place expects `result<list<V>, error>` (TYPE-11,
/// ERR-14): `ok` of the values held, oldest first, or, before the window is
/// all valid, "an `err` of an `error` with that code".
template <window_view E>
result<list<window_value_t<E>>> window_value_checked(const E& window) {
    if (!fn_all_valid(window)) {
        return std::unexpected(
            make_error("a window that is not all valid is read", std::string{codes::unset_read}));
    }
    return detail::held(window);
}

/// `removed_value<V, Max, Min>(input: endpoint<rolling_window<V, Max, Min>>)`
/// where its place expects `result<V, error>`, the only place it is read in
/// (END-9: "`removed_value(x)` is the value that left the window in the
/// cycle: in one of a tick count, the oldest value an arrival pushes out; in
/// one of a duration, the value that leaves by age, a value arriving then or
/// not. It is read only where its place expects a `result<V, error>`, and
/// gives `ok` of that value, or an `err` coded `value.unset_read` where none
/// left"): hgraph's `removed_value` of the window, which it keeps for the
/// cycle.
template <window_view E>
result<window_value_t<E>> fn_removed_value_checked(const E& window) {
    const hgraph::TSWDataView data = window.data_view();
    const hgraph::DateTime now = window.evaluation_time();
    if (!data.has_removed_value(now)) {
        return std::unexpected(
            make_error("no value left the window in the cycle", std::string{codes::unset_read}));
    }
    return data.removed_value(now).template checked_as<window_value_t<E>>();
}

} // namespace hglrt
