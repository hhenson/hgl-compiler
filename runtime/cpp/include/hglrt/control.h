// What a graph's `if` and `switch` over a connection need beside hgraph's
// `compiled::switch_` (CTRL-2, CTRL-3, CTRL-6, CTRL-13 to CTRL-15): the
// branch that never ticks, which an `if` with no `else` selects, and the
// branch that fails, which a `switch` with no `default` selects where no
// case has the selector's value.
//
// The compiler writes each branch of an `if` or a `switch` over a connection
// as a child graph, compiled with hgraph's `compile_subgraph`, and wires
// them with hgraph's `compiled::switch_` (`<hgraph/types/generated.h>`): it
// starts the branch a key selects, stops the one before, and keeps the
// branch while the key is published again as it was. The two below are
// what such a branch wires where the language gives it no statements of
// its own.
#pragma once

#include <hglrt/failure.h>
#include <hglrt/traces.h>

#include <hgraph/types/generated.h>

#include <string>

namespace hglrt {

// ---- A branch that never ticks (CTRL-3, CTRL-13) --------------------------

/// The node of a connection of the schema `S` that never ticks: it has no
/// input and asks for no evaluation, so it is never evaluated and its
/// output is never valid.
template <class S>
struct Never {
    static constexpr auto name = "hgl.never";

    static void eval([[maybe_unused]] hgraph::Out<S> out) {}
};

/// CTRL-3: "A consumed temporal if without else supplies a typed
/// never-ticking branch, not a default scalar." CTRL-13: "With no `else`,
/// under a temporal condition it is a connection of `T` that never ticks
/// while the condition is false". What the branch of no `else` gives: a
/// connection of `S` on `wiring` that never ticks.
template <class S>
hgraph::Port<S> never(hgraph::Wiring& wiring) {
    return hgraph::wire<Never<S>>(wiring);
}

// ---- A selector no case has (CTRL-6) --------------------------------------

namespace detail {
/// Fails, with no code, for the selector `key` of a `switch` that no case
/// has: the catalogue has no code for it.
template <class K>
[[noreturn]] void unmatched(const K& key) {
    fail_uncoded("no case of the `switch` has the value " + testing::text_of(key));
}
} // namespace detail

/// The node of the branch a `switch` with no `default` selects where no
/// case has the selector's value, in a `switch` whose branches give a
/// connection of `S`: given the selector, it fails when it is evaluated,
/// as the branch starts.
template <class K, class S>
struct Unmatched {
    static constexpr auto name = "hgl.unmatched";

    static void eval(hgraph::In<"key", hgraph::TS<K>> key, [[maybe_unused]] hgraph::Out<S> out) {
        detail::unmatched(key.value());
    }
};

/// [`Unmatched`] in a `switch` whose branches give nothing.
template <class K>
struct UnmatchedSink {
    static constexpr auto name = "hgl.unmatched";

    static void eval(hgraph::In<"key", hgraph::TS<K>> key) { detail::unmatched(key.value()); }
};

/// CTRL-6: "An unmatched selector uses default or fails." The branch a
/// `switch` over a connection with no `default` is given, whose branches
/// give a connection of `S`: it fails once it is selected, given the
/// selector `key`, and its connection never ticks.
template <class K, class S>
hgraph::Port<S> unmatched(hgraph::Wiring& wiring, hgraph::Port<hgraph::TS<K>> key) {
    return hgraph::wire<Unmatched<K, S>>(wiring, key);
}

/// [`unmatched`] in a `switch` whose branches give nothing.
template <class K>
void unmatched_sink(hgraph::Wiring& wiring, hgraph::Port<hgraph::TS<K>> key) {
    hgraph::wire<UnmatchedSink<K>>(wiring, key);
}

} // namespace hglrt
