// What makes a connection with no input: a constant wired where a connection
// is expected (FN-3), and a source, a body with `yield` (SOURCE-1 to
// SOURCE-4), which core's `replay` is.
//
// A constant is a node of this library. A source is a node the compiler
// writes, whose body is a C++23 coroutine kept in the node's state
// (docs/cpp-target.md); `resume` takes the step that runs it on to its next
// `yield`, and drives it under the timing SOURCE-2 and SOURCE-3 give.
#pragma once

#include <hglrt/failure.h>
#include <hglrt/scalar.h>
#include <hglrt/values.h>

#include <hgraph/types/graph_wiring.h>
#include <hgraph/types/static_node.h>

#include <optional>
#include <source_location>
#include <utility>
#include <variant>

namespace hglrt {

// ---- Constants (FN-3) ----------------------------------------------------

/// The node of a constant connection of `T`: it publishes its value once,
/// in the first cycle of its run, and then nothing.
template <class T>
struct Constant {
    static constexpr auto name = "hgl.const";
    static constexpr bool schedule_on_start = true;

    static void eval(hgraph::Scalar<"value", T> value, hgraph::Out<hgraph::TS<T>> out) {
        out.set(value.value());
    }
};

/// FN-3: "An ordinary value of type T is accepted where composition expects
/// a connection of T, and is wired as a constant, one publication when the
/// graph starts." Two constants of one type and value are one node, as
/// hgraph interns identical nodes.
template <class T>
hgraph::Port<hgraph::TS<T>> constant(hgraph::Wiring& wiring, const T& value) {
    return hgraph::wire<Constant<T>>(wiring, value);
}

// ---- Sources (SOURCE-1 to SOURCE-4) --------------------------------------

/// The time a `yield` gives (SOURCE-2): "A duration is relative to current
/// evaluation time; a datetime is absolute."
using YieldTime = std::variant<duration, datetime>;

/// What a step of a source's body gives: its `yield`'s time and payload,
/// each evaluated once and the time first (SOURCE-2).
template <class T>
struct Yielded {
    YieldTime time;
    T value;
};

/// A source between two evaluations of its node: the payload it keeps for a
/// target still to come, that target, and the target of its last `yield`,
/// whatever became of it. It is part of the node's state, and starts empty.
template <class T>
struct Source {
    std::optional<T> pending{};
    datetime target{};
    std::optional<datetime> previous{};
};

/// Resumes the source `source` in its node's evaluation at `now`, and
/// publishes what is due then through `publish`.
///
/// `step()` runs the body on to its next `yield`, giving a `Yielded<T>`, or
/// nothing where the body completes (SOURCE-4: "A bare return or body
/// completion finishes the source"). `scheduler` is the node's scheduler,
/// which `resume` asks for the wake of a target still to come.
///
/// SOURCE-2: "Negative duration fails. Resolve duration addition with
/// checked arithmetic." SOURCE-3: "Targets strictly increase across the
/// invocation, including skipped past targets. The first has no
/// predecessor. A past absolute target is skipped; a current target
/// publishes now; a future target retains its payload and suspends. On wake
/// it publishes and resumes after yield." A publication is made before the
/// statements after its `yield` run, so it stands where they fail (ERR-8).
template <class T, class Scheduler, class Step, class Publish>
void resume(Source<T>& source, datetime now, const Scheduler& scheduler, Step&& step, Publish&& publish,
            std::source_location where = std::source_location::current()) {
    if (source.pending.has_value()) {
        if (source.target != now) {
            return;
        }
        T due = std::move(*source.pending);
        source.pending.reset();
        publish(std::move(due));
    }
    while (true) {
        std::optional<Yielded<T>> next = step();
        if (!next.has_value()) {
            return;
        }
        datetime target{};
        if (const duration* after = std::get_if<duration>(&next->time)) {
            if (*after < duration::zero()) {
                fail(codes::negative_duration, "a `yield` after a negative duration", where);
            }
            target = add__datetime_duration(now, *after, where);
        } else {
            target = std::get<datetime>(next->time);
        }
        if (source.previous.has_value() && target <= *source.previous) {
            fail(codes::non_increasing_time, "a `yield` at a time that is not after the one before it",
                 where);
        }
        source.previous = target;
        if (target < now) {
            continue;
        }
        if (target == now) {
            publish(std::move(next->value));
            continue;
        }
        source.pending = std::move(next->value);
        source.target = target;
        scheduler.schedule(target);
        return;
    }
}

} // namespace hglrt
