// Core's SET-1 dictionary key set and MAP-5 lifecycle key joins.
#pragma once

#include <hglrt/collections.h>

#include <hgraph/types/generated.h>

#include <cstddef>
#include <span>

namespace hglrt {

/// MAP-5: select the keys of every split input, any split input, or the first.
enum class MapKeyJoin { Inner, Outer, Left };

/// MAP-5: derive lifecycle keys from the split ports in callback parameter order.
hgraph::WiringPortRef joined_map_keys(
    hgraph::Wiring& wiring,
    std::span<const hgraph::WiringPortRef> inputs,
    std::span<const std::size_t> split,
    MapKeyJoin join
);

namespace key_sets_detail {
/// The membership port of a dictionary, with the full wiring front end
/// confined to the runtime's implementation.
hgraph::WiringPortRef dictionary_keys(const hgraph::WiringPortRef& dictionary);
} // namespace key_sets_detail

/// SET-1: the keys of an ordinary dictionary, independent of its values.
template <class K, class V>
set<K> keys_dict_K__V_(const dict<K, V>& dictionary) {
    set<K> result;
    result.reserve(dictionary.size());
    for (const auto& entry : dictionary) {
        result.insert(entry.first);
    }
    return result;
}

/// SET-1: hgraph's TSD key-set port observes membership, not value updates.
template <class K, class V>
hgraph::Port<hgraph::TSS<K>> wire_keys_dict_K__V_(
    hgraph::Wiring& wiring,
    hgraph::Port<hgraph::TSD<K, V>> dictionary
) {
    return hgraph::Port<hgraph::TSS<K>>{
        wiring, key_sets_detail::dictionary_keys(dictionary.erased())
    };
}

} // namespace hglrt
