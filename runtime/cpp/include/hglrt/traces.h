// Traces (TST-4 to TST-7, TST-16, CEVAL-2, CEVAL-4): an eval's recording and
// a trace written out, dense or timed; whether two are the same, and where
// they first differ; what one written out replays to an input and gives a
// `const fn`; and the text of their cells.
//
// A cell is one cycle's publication (TST-5), "written as a `return` is
// (TIME-7) ... a T, published whole, or a `delta<T>`". Where a T publishes
// whole, as a scalar and a nullable scalar do, "the two are one", and a
// cell is a T (a nullable's `std::optional`, where a published null is an
// optional that holds nothing). Where T's delta is a value of another type,
// a tuple's or a list's (values.h), a cell is either, `Cell<T>`: a
// `delta_t<T>`, "compared with the delta its cycle published", or a
// `Whole<T>`, compared "with the value the result holds once its cycle has
// published". An eval's recording holds each publication's delta, as core's
// `record` keeps it; the value the result held is kept beside it where the
// eval's entry gives it (testing.h, `hold`).
#pragma once

#include <hglrt/failure.h>
#include <hglrt/scalar.h>
#include <hglrt/temporal.h>
#include <hglrt/values.h>

#include <algorithm>
#include <cstddef>
#include <optional>
#include <stdexcept>
#include <string>
#include <tuple>
#include <type_traits>
#include <utility>
#include <variant>
#include <vector>

namespace hglrt::testing {

/// The smallest step of time (CRUN-3): a microsecond, hgraph's `MIN_TD`.
duration smallest_step() noexcept;

/// The earliest start (CRUN-3): "one smallest step after
/// `1970-01-01T00:00:00Z`", hgraph's `MIN_ST`, which a simulation of hgraph
/// runs from.
datetime earliest_start() noexcept;

// ---- Cells (TST-5) -------------------------------------------------------

/// A cell written as a whole value of T, where T's delta is no T.
template <class T>
struct Whole {
    T value;
};

/// Whether a T publishes whole: its delta is a T.
template <class T>
inline constexpr bool publishes_whole = std::is_same_v<delta_t<T>, T>;

/// A cell of a trace of a T: a T where a T publishes whole, and otherwise a
/// delta of it or a whole value.
template <class T>
using Cell = std::conditional_t<publishes_whole<T>, T, std::variant<delta_t<T>, Whole<T>>>;

// ---- Traces (TST-6, TST-7, CEVAL-4) --------------------------------------

/// A trace of a T written out dense, `[7, _, 9]` (TST-6): cell i is the
/// publication i smallest steps after the run's start, and an empty cell,
/// `_`, is none.
template <class T>
using Dense = std::vector<std::optional<Cell<T>>>;

/// The key of a timed trace's entry (TST-7): a duration from the run's start,
/// or an instant.
using TraceKey = std::variant<duration, datetime>;

/// A trace of a T written out timed, `[0s: 7, 2s: 9]` (TST-7): its
/// publications in the order written, each with its key.
template <class T>
struct Timed {
    std::vector<std::pair<TraceKey, Cell<T>>> entries;
};

/// Publications as core's `record` keeps them, and as `replay` takes its
/// items (CEVAL-2, CEVAL-4, TST-16): each one's instant and what it
/// published, in time order. A `Recorded<delta_t<T>>` is what a `const fn`
/// is given for a trace of a T.
template <class T>
using Recorded = list<std::tuple<datetime, T>>;

/// The value a result held once each of its publications was made, by the
/// instant of each: none where it held no whole value, a part of it not
/// valid.
template <class T>
using Held = list<std::tuple<datetime, std::optional<T>>>;

/// The trace of an eval of a target whose result is a T (CEVAL-4): the
/// instant its run started at, which its publications are read against;
/// what its run recorded; and, where its entry gave them, the values the
/// result held.
template <class T>
struct Recording {
    datetime start{};
    Recorded<delta_t<T>> entries{};
    std::optional<Held<T>> held{};
};

// ---- The text of a value ---------------------------------------------------

namespace detail {
template <class T>
struct is_whole : std::false_type {};
template <class T>
struct is_whole<Whole<T>> : std::true_type {};

template <class T>
struct is_variant : std::false_type {};
template <class... T>
struct is_variant<std::variant<T...>> : std::true_type {};

/// The T of a cell type `C`, a `Cell<T>`.
template <class C>
struct value_of_cell {
    using type = C;
};
template <class D, class T>
struct value_of_cell<std::variant<D, Whole<T>>> {
    using type = T;
};

/// The text of a scalar, as the language writes a literal of it.
std::string scalar_text(i64 value);
std::string scalar_text(f64 value);
std::string scalar_text(bool value);
std::string scalar_text(const str& value);
std::string scalar_text(const bytes& value);
std::string scalar_text(date value);
std::string scalar_text(time value);
std::string scalar_text(datetime value);
std::string scalar_text(duration value);
std::string scalar_text(const civil_datetime& value);
std::string scalar_text(const zoned_datetime& value);
} // namespace detail

/// `value` as the language writes it, for a person to read in a failure: a
/// scalar as its literal, `null`, a tuple `(1, 2.0)`, a list `[1, 2]`, a
/// list's delta by its properties. An enum the compiler writes is written
/// as its `hgl_text` gives it, found by argument-dependent lookup, and
/// otherwise as its number.
template <class T>
std::string text_of(const T& value) {
    if constexpr (hglrt::detail::is_optional<T>::value) {
        return value.has_value() ? text_of(*value) : std::string{"null"};
    } else if constexpr (hglrt::detail::is_tuple<T>::value) {
        std::string parts;
        std::apply(
            [&](const auto&... part) { ((parts += (parts.empty() ? "" : ", ") + text_of(part)), ...); },
            value);
        return "(" + parts + ")";
    } else if constexpr (hglrt::detail::is_list<T>::value || hglrt::detail::is_fixed_list<T>::value) {
        std::string parts;
        for (const auto& element : value) {
            parts += (parts.empty() ? "" : ", ") + text_of(element);
        }
        return "[" + parts + "]";
    } else if constexpr (hglrt::detail::is_list_delta<T>::value) {
        std::string updated;
        for (const auto& [position, change] : value.updated) {
            updated += (updated.empty() ? "" : ", ") + text_of(position) + ": " + text_of(change);
        }
        std::string removed;
        for (const i64 position : value.removed) {
            removed += (removed.empty() ? "" : ", ") + text_of(position);
        }
        std::string text = "delta<" + type_name<typename T::list_type>() + ">(updated: [" + updated +
                           "], removed: [" + removed + "]";
        if (value.size_modified.has_value()) {
            text += ", size_modified: " + text_of(*value.size_modified);
        }
        return text + ")";
    } else if constexpr (detail::is_whole<T>::value) {
        return text_of(value.value);
    } else if constexpr (detail::is_variant<T>::value) {
        return std::visit([](const auto& held) { return text_of(held); }, value);
    } else if constexpr (requires { hgl_text(value); }) {
        return hgl_text(value);
    } else if constexpr (std::is_enum_v<T>) {
        return text_of(static_cast<i64>(value));
    } else if constexpr (requires { detail::scalar_text(value); }) {
        return detail::scalar_text(value);
    } else {
        return "a value of " + type_name<T>();
    }
}

// ---- Comparison (TST-4) ----------------------------------------------------

namespace detail {
/// Fails a timed trace whose keys do not strictly increase (TST-7). The
/// catalogue's `trace.key_order` is a source rejection; a key known only as
/// the program runs fails with no code.
[[noreturn]] void unordered_keys();

/// Fails a trace that would publish before its run's start (CEVAL-2).
[[noreturn]] void before_start();

/// Fails the comparison of a cell written whole with a recording that
/// holds no value of the result: an entry that gave none is an internal
/// fault of the generated code, and no HGL failure.
[[noreturn]] void nothing_held();

/// The key `key` of a timed trace read against a run that started at
/// `start`, where there is one: a duration from the start.
TraceKey relative_key(const TraceKey& key, const std::optional<datetime>& start);

/// Whether `earlier` is before `later`; fails where the two cannot be
/// ordered, a duration and an instant read against no run.
bool before(const TraceKey& earlier, const TraceKey& later);

/// The text of a key: a duration from the run's start, or an instant.
std::string key_text(const TraceKey& key);

/// One publication of a trace of a T, as two are compared: its key, and the
/// cell written for it, or, in a recording, the delta it published and the
/// value the result then held, where the run gave it.
template <class T>
struct Seen {
    TraceKey key;
    const Cell<T>* written = nullptr;
    const delta_t<T>* published = nullptr;
    /// Whether the run gave the values its result held.
    bool holding = false;
    /// The value the result held, where it was given one for this instant;
    /// none within where it held no whole value.
    const std::optional<T>* held = nullptr;
};

/// A trace as its publications, each with its key: a duration from the
/// run's start where there is one to read it against. A dense trace's empty
/// cells are no publication.
template <class C>
std::vector<Seen<typename value_of_cell<C>::type>> relative(const std::vector<std::optional<C>>& trace,
                                                            const std::optional<datetime>&) {
    std::vector<Seen<typename value_of_cell<C>::type>> out;
    for (std::size_t position = 0; position < trace.size(); ++position) {
        if (trace[position].has_value()) {
            out.push_back(
                {.key = smallest_step() * static_cast<i64>(position), .written = &*trace[position]});
        }
    }
    return out;
}

template <class T>
std::vector<Seen<T>> relative(const Timed<T>& trace, const std::optional<datetime>& start) {
    std::vector<Seen<T>> out;
    for (const auto& [key, cell] : trace.entries) {
        TraceKey after = relative_key(key, start);
        if (!out.empty() && !before(out.back().key, after)) {
            unordered_keys();
        }
        out.push_back({.key = std::move(after), .written = &cell});
    }
    return out;
}

template <class T>
std::vector<Seen<T>> relative(const Recording<T>& trace, const std::optional<datetime>&) {
    std::vector<Seen<T>> out;
    // The values held are in time order, as the publications are: each is
    // found walking the two together.
    std::size_t next_held = 0;
    for (const auto& [at, published] : trace.entries) {
        Seen<T> seen{.key = at - trace.start, .published = &published, .holding = trace.held.has_value()};
        if (trace.held.has_value()) {
            const Held<T>& held = *trace.held;
            while (next_held < held.size() && std::get<0>(held[next_held]) < at) {
                ++next_held;
            }
            if (next_held < held.size() && std::get<0>(held[next_held]) == at) {
                seen.held = &std::get<1>(held[next_held]);
            }
        }
        out.push_back(seen);
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

/// The T a trace is of.
template <class Trace>
struct trace_of;
template <class C>
struct trace_of<std::vector<std::optional<C>>> {
    using type = typename value_of_cell<C>::type;
};
template <class T>
struct trace_of<Timed<T>> {
    using type = T;
};
template <class T>
struct trace_of<Recording<T>> {
    using type = T;
};

/// What names a trace in a failure.
template <class Trace>
std::string name_of(const Trace&) {
    return "the trace written out";
}
template <class T>
std::string name_of(const Recording<T>&) {
    return "the eval";
}

/// Whether two cells written out are the same: a whole value as a whole
/// value, and a delta as a delta.
template <class T>
bool same_written(const Cell<T>& left, const Cell<T>& right) {
    if constexpr (publishes_whole<T>) {
        return eq__T_T(left, right);
    } else {
        if (left.index() != right.index()) {
            return false;
        }
        if (left.index() == 0) {
            return eq__T_T(std::get<0>(left), std::get<0>(right));
        }
        return eq__T_T(std::get<1>(left).value, std::get<1>(right).value);
    }
}

/// Whether the cell written out `cell` is the publication `seen` of a
/// recording (TST-5).
template <class T>
bool matches(const Cell<T>& cell, const Seen<T>& seen) {
    if constexpr (publishes_whole<T>) {
        return eq__T_T(cell, *seen.published);
    } else {
        if (cell.index() == 0) {
            return eq__T_T(std::get<0>(cell), *seen.published);
        }
        if (!seen.holding) {
            nothing_held();
        }
        return seen.held != nullptr && seen.held->has_value() &&
               eq__T_T(std::get<1>(cell).value, **seen.held);
    }
}

/// Whether two publications are the same, keys apart.
template <class T>
bool same_cell(const Seen<T>& left, const Seen<T>& right) {
    if (left.written != nullptr && right.written != nullptr) {
        return same_written<T>(*left.written, *right.written);
    }
    if (left.published != nullptr && right.published != nullptr) {
        return eq__T_T(*left.published, *right.published);
    }
    return left.written != nullptr ? matches(*left.written, right) : matches(*right.written, left);
}

/// The text of a publication.
template <class T>
std::string seen_text(const Seen<T>& seen) {
    if (seen.written != nullptr) {
        return text_of(*seen.written);
    }
    std::string text = text_of(*seen.published);
    if constexpr (!publishes_whole<T>) {
        if (seen.held != nullptr) {
            text += ", holding " + text_of(*seen.held);
        }
    }
    return text;
}
} // namespace detail

/// Where two traces of one type first differ (TST-4: "compare presence,
/// exact present data, and shape/length ... A mismatch identifies the first
/// differing cycle/time and expected/observed data"): none where they are
/// the same. They are the same where they have the same publications, each
/// as long after its run's start as the other's, and equal by the language's
/// equality (OPS-2: a NaN is equal to nothing). "Trailing placeholders have
/// no effect" (TST-6). A timed trace's instants are read against the run the
/// other trace is of, where it is one.
template <class Left, class Right>
std::optional<std::string> difference(const Left& left, const Right& right) {
    using T = typename detail::trace_of<Left>::type;
    static_assert(std::is_same_v<T, typename detail::trace_of<Right>::type>,
                  "hglrt::testing::difference: two traces of one type");
    std::optional<datetime> start = detail::start_of(left);
    if (!start.has_value()) {
        start = detail::start_of(right);
    }
    const auto ours = detail::relative(left, start);
    const auto theirs = detail::relative(right, start);
    std::string ours_name = detail::name_of(left);
    std::string theirs_name = detail::name_of(right);
    if (ours_name == theirs_name) {
        ours_name = "the left one";
        theirs_name = "the right one";
    }
    const auto differ = [&](const TraceKey& key, const std::string& ours_text,
                            const std::string& theirs_text) {
        return "the traces differ at " + detail::key_text(key) + ": " + ours_name + " has " + ours_text +
               ", and " + theirs_name + " has " + theirs_text;
    };
    const std::size_t both = std::min(ours.size(), theirs.size());
    for (std::size_t index = 0; index < both; ++index) {
        const auto& mine = ours[index];
        const auto& other = theirs[index];
        if (mine.key != other.key) {
            // The one that publishes first has a publication the other has
            // not.
            if (detail::before(mine.key, other.key)) {
                return differ(mine.key, detail::seen_text(mine), "nothing");
            }
            return differ(other.key, "nothing", detail::seen_text(other));
        }
        if (!detail::same_cell(mine, other)) {
            return differ(mine.key, detail::seen_text(mine), detail::seen_text(other));
        }
    }
    if (ours.size() > both) {
        return differ(ours[both].key, detail::seen_text(ours[both]), "nothing");
    }
    if (theirs.size() > both) {
        return differ(theirs[both].key, "nothing", detail::seen_text(theirs[both]));
    }
    return std::nullopt;
}

/// Whether two traces are the same (TST-4), as `difference` has it.
template <class Left, class Right>
bool same(const Left& left, const Right& right) {
    return !difference(left, right).has_value();
}

// ---- What a trace written out publishes (CEVAL-2, TST-9, TST-16) ---------

namespace detail {
/// How many positions a list of `size` holds once `delta` is published to it
/// (TYPE-16): the size it gives, a negative one taken from the size, or,
/// with none, enough to hold each position it updates.
template <class L>
std::size_t size_after(std::size_t size, const list_delta<L>& delta) {
    if (delta.size_modified.has_value()) {
        const i64 written = *delta.size_modified;
        if (written < 0) {
            const auto dropped = static_cast<std::size_t>(-(written + 1)) + 1;
            return dropped >= size ? 0 : size - dropped;
        }
        return static_cast<std::size_t>(written);
    }
    if (delta.updated.empty()) {
        return size;
    }
    return std::max(size, static_cast<std::size_t>(delta.updated.rbegin()->first) + 1);
}

/// The delta that publishing `value` whole records, to a value that held
/// `size` positions, where it is a list (TST-9: "a T whole, which replaces
/// what the input held"; TST-16: "A T published whole is held as its delta
/// too"): a tuple's, each position; a fixed list's, each position; an
/// unsized list's, each position, the tail past them removed, and the new
/// size where it moves (TYPE-16: "on a shrink names the tail positions it
/// drops in `removed`"). Each part publishes whole.
template <class T>
delta_t<T> whole_delta(const T& value, std::size_t size) {
    if constexpr (hglrt::detail::is_tuple<T>::value) {
        return std::apply(
            [](const auto&... part) {
                static_assert((publishes_whole<std::remove_cvref_t<decltype(part)>> && ...),
                              "hglrt: a whole tuple cell of parts that publish whole");
                return delta_t<T>{part...};
            },
            value);
    } else {
        using Element = typename T::value_type;
        static_assert(publishes_whole<Element>, "hglrt: a whole list cell of elements that publish whole");
        delta_t<T> delta;
        for (std::size_t position = 0; position < value.size(); ++position) {
            delta.updated.emplace(static_cast<i64>(position), value[position]);
        }
        if constexpr (hglrt::detail::is_list<T>::value) {
            for (std::size_t position = value.size(); position < size; ++position) {
                delta.removed.insert(static_cast<i64>(position));
            }
            if (value.size() != size) {
                delta.size_modified = static_cast<i64>(value.size());
            }
        }
        return delta;
    }
}

/// What each cell of a trace of a T publishes, in order, a whole one as the
/// delta that replaces what the value held: the size of a list is followed
/// through each cell, a delta's among them.
template <class T>
class Publisher {
  public:
    delta_t<T> operator()(const Cell<T>& cell) {
        if constexpr (publishes_whole<T>) {
            return cell;
        } else {
            delta_t<T> delta =
                cell.index() == 0 ? std::get<0>(cell) : whole_delta(std::get<1>(cell).value, size_);
            if constexpr (hglrt::detail::is_list_delta<delta_t<T>>::value) {
                size_ = size_after(size_, delta);
            }
            return delta;
        }
    }

  private:
    std::size_t size_ = 0;
};
} // namespace detail

/// The items core's `replay` is given for the dense trace `trace` of a run
/// that starts at `start` (CEVAL-2): "dense position i is the start plus i
/// smallest steps. A dense placeholder locates any later entries and adds no
/// replay entry or run bound." A cell written whole is given as the delta
/// that replaces what the input held (TST-9). (TIME-7 has a whole value
/// published tick even where it changes nothing, an empty list given an
/// input that holds none: the delta then names no change, and core's
/// `replay` is given no more than the delta.)
template <class C>
Recorded<delta_t<typename detail::value_of_cell<C>::type>>
replay_items(const std::vector<std::optional<C>>& trace, datetime start) {
    using T = typename detail::value_of_cell<C>::type;
    Recorded<delta_t<T>> items;
    detail::Publisher<T> publish;
    for (const auto& seen : detail::relative(trace, start)) {
        items.emplace_back(add__datetime_duration(start, std::get<duration>(seen.key)),
                           publish(*seen.written));
    }
    return items;
}

/// The items `replay` is given for the timed trace `trace` (CEVAL-2, TST-7):
/// each entry at its instant, a duration key from the start. Its keys
/// strictly increase, and none is before the start.
template <class T>
Recorded<delta_t<T>> replay_items(const Timed<T>& trace, datetime start) {
    Recorded<delta_t<T>> items;
    detail::Publisher<T> publish;
    for (const auto& seen : detail::relative(trace, start)) {
        const duration after = std::get<duration>(seen.key);
        if (after < duration::zero()) {
            detail::before_start();
        }
        items.emplace_back(add__datetime_duration(start, after), publish(*seen.written));
    }
    return items;
}

/// What a `const fn` is given for a trace (TST-16): "for a result type T a
/// `list<tuple<datetime, delta<T>>>`, one tuple for each publication, in
/// time order". An eval's trace gives what its run recorded. A trace
/// written out has no run: a dense position, and a duration key, is that
/// long after the earliest start, and an instant key is that instant; a
/// cell written whole is the delta it publishes.
template <class T>
Recorded<delta_t<T>> publications(const Recording<T>& trace) {
    return trace.entries;
}

template <class Trace>
Recorded<delta_t<typename detail::trace_of<Trace>::type>> publications(const Trace& trace) {
    using T = typename detail::trace_of<Trace>::type;
    Recorded<delta_t<T>> items;
    detail::Publisher<T> publish;
    for (const auto& seen : detail::relative(trace, std::nullopt)) {
        const datetime at = std::holds_alternative<duration>(seen.key)
                                ? add__datetime_duration(earliest_start(), std::get<duration>(seen.key))
                                : std::get<datetime>(seen.key);
        items.emplace_back(at, publish(*seen.written));
    }
    return items;
}

} // namespace hglrt::testing
