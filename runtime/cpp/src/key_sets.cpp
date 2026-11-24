// Core's SET-1 dictionary key-set projection, implemented by hgraph wiring.

#include <hglrt/key_sets.h>

#include <hgraph/types/subgraph_wiring.h>

namespace hglrt::key_sets_detail {

hgraph::WiringPortRef dictionary_keys(const hgraph::WiringPortRef& dictionary) {
    return hgraph::subgraph_wiring_detail::tsd_key_set_ref(dictionary);
}

} // namespace hglrt::key_sets_detail
