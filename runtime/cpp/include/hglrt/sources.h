// What makes a connection with no input: a constant wired where a connection
// is expected (FN-3), and a source, a body with `yield` (SOURCE-1 to
// SOURCE-4), which core's `replay` is.
//
// A constant is a node of this library. A source is a node the compiler
// writes, whose body is a C++23 coroutine kept in the node's state
// (docs/cpp-target.md); `resume` takes the step that runs it on to its next
// `yield`, and drives it under the timing SOURCE-2 and SOURCE-3 give.
#pragma once

#include <hglrt/endpoints.h>
#include <hglrt/failure.h>
#include <hglrt/scalar.h>
#include <hglrt/temporal.h>
#include <hglrt/values.h>

#include <hgraph/types/generated.h>
#include <hgraph/types/metadata/type_realization.h>

#include <optional>
#include <source_location>
#include <span>
#include <type_traits>
#include <typeinfo>
#include <utility>
#include <variant>

namespace hglrt {

// ---- Constants (FN-3) ----------------------------------------------------

/// The node of a constant connection of `T` in its temporal `Schema`:
/// it publishes its value once, in the first cycle of its run.
template <class T, class Schema = hgraph::TS<T>>
struct Constant {
    static constexpr auto name = "hgl.const";
    static constexpr bool schedule_on_start = true;

    static constexpr bool atomic = std::is_same_v<Schema, hgraph::TS<T>>;
    // A fixed list is stored whole in a configuration (FN-1).
    using Storage = std::conditional_t<atomic, T, configured<T>>;

    static void eval(hgraph::Scalar<"value", Storage> value, hgraph::Out<Schema> out) {
        if constexpr (atomic) {
            out.set(value.value());
        } else {
            publish_as_form(out, value.value().held, std::source_location::current());
        }
    }
};

/// FN-3: "An ordinary value of type T is accepted where composition expects
/// a connection of T, and is wired as a constant, one publication when the
/// graph starts." Each constant is a node of its own, which hgraph does not
/// intern (`Wiring::add_unique_node`): hgraph takes two nodes for one where
/// their scalar configurations are equal under `==`, which holds of `0.0`
/// and `-0.0`, and of a list or a tuple that holds either, though NUM-2 keeps
/// them apart ("preserves IEEE special values, including signed zero"), so
/// that one constant would publish the other's value. A constant publishes
/// once, at the start: two of one value give each reader the same
/// publication in the same cycle, and no reader can tell two nodes from one.
template <class T, class Schema = hgraph::TS<T>>
hgraph::Port<Schema> constant(hgraph::Wiring& wiring, const T& value) {
    using Node = Constant<T, Schema>;
    using Signature = hgraph::StaticNodeSignature<Node>;
    // The node's scalar configuration, as `hgraph::wire` makes it.
    hgraph::Value scalars{
        hgraph::value_type_for_wiring(Signature::scalar_schema(), wiring.realization_options())};
    {
        auto fields = scalars.as_bundle().begin_mutation();
        if constexpr (Node::atomic) {
            fields["value"].set(value);
        } else {
            fields["value"].set(typename Node::Storage{value});
        }
    }
    hgraph::NodeBuilder builder;
    builder.implementation<Node>();
    return hgraph::Port<Schema>{wiring, wiring.add_unique_node(typeid(Node), std::move(builder),
                                                               std::span<const hgraph::WiringPortRef>{},
                                                               std::move(scalars))};
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
        // SOURCE-3: "A past absolute target is skipped". This driver skips it
        // itself: hgraph's `NodeScheduler` drops a wake asked for at a time
        // that is not after the evaluation time, with nothing said, and the
        // source would then wait for it for ever (hgraph's own skip, issue
        // 1848, is its Python generator's).
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
