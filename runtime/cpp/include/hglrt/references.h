// The `ref` mode of a node's parameters and result (the language's FN-16,
// TIME-10; core's END-10): what the generated code of a node calls to read
// a designation.
//
// A `ref` parameter of T is hgraph's `REF<S>` input, and a `ref` result
// hgraph's `REF<S>` output, S the temporal form of T: a reference is a
// temporal value whose payload designates an endpoint (TIME-10). A
// node's `REF` input given an ordinary connection holds a reference to it,
// and an ordinary input given a `REF` output observes what it designates,
// as hgraph binds them. What the generated code reads of a reference is
// what it designates, here, which it publishes to a `ref` result whole;
// it never reads the endpoint designated through it.
#pragma once

#include <hgraph/types/generated.h>

#include <concepts>

namespace hglrt {

/// What the `REF` input `input` designates now: the reference it holds,
/// or the empty reference where it holds none. FN-16: "Returning a whole
/// `ref` parameter samples its current designation when the `return`
/// executes"; TIME-10: "`valid` of a `ref` parameter is whether a
/// reference is present", and one that holds none designates nothing. It
/// is also what a node publishes where its `ref` result designates an
/// ordinary input, read through the `REF` input it is given that input
/// again on.
template <class In>
    requires requires(const In& input) {
        { input.value() } -> std::same_as<::hgraph::TimeSeriesReference>;
    } ::hgraph::TimeSeriesReference
designation(const In& input) {
    return input.valid() ? input.value() : ::hgraph::TimeSeriesReference::empty();
}

} // namespace hglrt
