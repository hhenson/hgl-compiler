// Core's SET-1 dictionary key set in value code and temporal composition.
#pragma once

#include <hglrt/collections.h>

#include <hgraph/types/generated.h>

namespace hglrt {

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
