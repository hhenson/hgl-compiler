// Core's native capability operations of the global store (capabilities.h).
#include <hglrt/capabilities.h>

#include <format>

namespace hglrt {

bool has_GlobalState_str(hgraph::GlobalStateView store, std::string_view key) {
    return store.contains(key);
}

void detail::store_missing(std::string_view key, std::source_location where) {
    fail_uncoded(std::format("the global store holds nothing under '{}'", key), where);
}

void detail::store_type(std::string_view key, std::source_location where) {
    fail_uncoded(std::format("the global store holds a value of another type under '{}'", key), where);
}

} // namespace hglrt
