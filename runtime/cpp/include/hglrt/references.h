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

#include <hglrt/checked.h>
#include <hglrt/values.h>

#include <hgraph/types/generated.h>

#include <concepts>
#include <cstddef>
#include <source_location>

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

/// `index`, the position of a fixed list of `size` elements that a `ref`
/// result is made to designate, `return items[idx]` (FN-16: "Returning
/// ... a child path such as `items[idx]` designates that logical
/// endpoint"). An index outside the list fails with
/// `value.index_out_of_range`, as the child at it does (EXPR-3: "An index
/// outside a list, below zero or at or past its size, fails where it is
/// evaluated").
inline i64 designated_position(i64 index, std::size_t size,
                               std::source_location where = std::source_location::current()) {
    detail::check_index(index, size, where);
    return index;
}

/// What the `REF` input among `inputs` at `position` designates
/// (`designation`): each position of a fixed list a `ref` result may
/// designate is followed through a `REF` input of its own, in order, and
/// `position` is one of them, as `designated_position` gives it.
template <class... In>
::hgraph::TimeSeriesReference designation_of(i64 position, const In&... inputs) {
    ::hgraph::TimeSeriesReference found = ::hgraph::TimeSeriesReference::empty();
    i64 at = 0;
    static_cast<void>(((at++ == position ? (found = designation(inputs), true) : false) || ...));
    return found;
}

} // namespace hglrt
