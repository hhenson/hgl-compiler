// Core's native list operations (`hgl/native/lists.hgl`, core's list
// contract LIST-1 to LIST-3).
//
// Each takes its list as a `var` parameter (FN-15), which the target writes
// as a reference to the place: the list grows where it is, and a value taken
// from the place before the call is a copy of its own, as it was (LIST-3).
#pragma once

#include <type_traits>
#include <utility>
#include <vector>

namespace hglrt {

/// `append<V>(var values: list<V>, item: V)` (LIST-1): gives the place
/// `values` the list it held with `item` after its last entry.
template <class V>
void fn_append(std::vector<V>& values, std::type_identity_t<V> item) {
    values.push_back(std::move(item));
}

/// `extend<V>(var values: list<V>, items: list<V>)` (LIST-2): gives the place
/// `values` the list it held with each entry of `items` after its last
/// entry, in the order `items` has them. An empty `items` leaves it as it
/// was. `items` may be the place itself, whose entries are then taken as
/// they were before the call.
template <class V>
void fn_extend(std::vector<V>& values, const std::type_identity_t<std::vector<V>>& items) {
    if (&items == &values) {
        const std::vector<V> before = items;
        values.insert(values.end(), before.begin(), before.end());
        return;
    }
    values.insert(values.end(), items.begin(), items.end());
}

} // namespace hglrt
