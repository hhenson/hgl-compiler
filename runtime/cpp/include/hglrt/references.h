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

namespace detail {

/// Select a logical dictionary entry through a structural input. A
/// forwarding slot without a binding has to be observed until it binds.
/// A node's held key also follows later changes to a forwarding binding.
template <class Out>
void select_entry(hgraph::TSDInputView& entries, const hgraph::ValueView& wanted,
                  i64& followed_slot, const Out& out, bool follow_rebinding = false) {
    entries.base().make_key_active(wanted);
    const std::size_t slot = entries.find_slot(wanted);
    if (followed_slot > 0 && static_cast<std::size_t>(followed_slot - 1) != slot) {
        const auto previous = static_cast<std::size_t>(followed_slot - 1);
        if (entries.slot_occupied(previous)) {
            entries.at_slot(previous).make_passive();
        }
        followed_slot = 0;
    }
    hgraph::TimeSeriesReference reference = hgraph::TimeSeriesReference::empty();
    if (slot != hgraph::TS_DATA_NO_CHILD_ID) {
        hgraph::TSInputView entry = entries.at_slot(slot);
        reference = entry.reference();
        const bool watch_binding = [&] {
            if (!follow_rebinding && !reference.is_empty()) {
                return false;
            }
            const auto position = entry.bound_output();
            return position.bound() && position.forwarding();
        }();
        if (watch_binding) {
            entry.make_active();
            followed_slot = static_cast<i64>(slot) + 1;
        } else if (followed_slot > 0) {
            entry.make_passive();
            followed_slot = 0;
        }
    }
    if (!out.valid() || !(out.value().template checked_as<hgraph::TimeSeriesReference>() == reference)) {
        out.set(reference);
    }
}

} // namespace detail

/// `dictionary[key]` in a graph, by a node of its own that `key` and the
/// dictionary's membership wake: its result `out` is made to designate the
/// entry `key` names (EXPR-7: "Temporal dictionary key selection may give
/// a reference, a designation of the keyed child (TIME-10)"), and an
/// ordinary reader of it observes that entry, ticking with the value it
/// holds where the key comes to name another. Where the dictionary holds no
/// entry for the key the result designates nothing, and its reader is not
/// valid (TIME-1: "A child of a collection ... is an endpoint whether or
/// not it is valid"), until one is added. A result given the designation
/// it holds is not published again (TIME-10).
///
/// It is hgraph's own form of a dictionary's entry by key: the dictionary
/// observed for its membership (`InputActivity::Structural`) and for the
/// key's, the entry's reference read from the input, and an entry whose
/// output forwards to one not bound yet observed until it is, its slot
/// plus one kept in `pending` (0 for none).
template <class Dictionary, class Key, class Out>
void select_entry(Dictionary& dictionary, const Key& key, i64& pending, const Out& out) {
    detail::select_entry(dictionary, key.base().value(), pending, out);
}

/// The logical key held by a node's `ref` result and the forwarding slot
/// it observes, if any (FN-16).
struct KeyDesignation {
    hgraph::Value key;
    i64 followed_slot{};
};

/// Refresh the selected key after a dictionary membership or binding
/// change. A missing key can receive a binding without running user code.
template <class Dictionary, class Out>
void refresh_key_designation(Dictionary& dictionary, KeyDesignation& held, const Out& out) {
    detail::select_entry(dictionary, held.key.view(), held.followed_slot, out, true);
}

/// A `return` or `out =` chooses a key once, then follows that logical
/// entry independently of any subsequent change to the key expression.
template <class Dictionary, class Out>
void designate_key(Dictionary& dictionary, const typename Dictionary::key_type& key,
                   KeyDesignation& held, const Out& out) {
    held.key = hgraph::Value{key};
    refresh_key_designation(dictionary, held, out);
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
