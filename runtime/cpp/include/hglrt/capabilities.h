// Core's native capability operations (`hgl/native/capabilities.hgl`), the
// ones of the global store (CAP-6): `get`, `has` and `put`, over hgraph's
// `GlobalStateView`, the store of the run a node is in.
//
// CAP-6: "The global store is a run-wide store of ordinary values under
// string keys ... `get<V>(global_state, key)` is the value under `key`, of
// the type the call writes or its place expects; an absent key fails the
// node. `has(global_state, key)` is whether the key holds a value.
// `put(global_state, key, value)` puts one there. A key keeps the type of its
// first value: a `put` of another type is rejected where the two are in one
// program, and fails otherwise. Reading or writing the store wakes no node."
// The catalogue has no code for either failure, so each has none.
#pragma once

#include <hglrt/failure.h>
#include <hglrt/values.h>

#include <hgraph/runtime/global_state.h>
#include <hgraph/types/value/value.h>

#include <source_location>
#include <string>
#include <string_view>

namespace hglrt {

namespace detail {
/// Fails a read of the store under `key`, which holds nothing.
[[noreturn]] void store_missing(std::string_view key, std::source_location where);
/// Fails a read or a write of the store under `key`, which holds a value of
/// another type.
[[noreturn]] void store_type(std::string_view key, std::source_location where);
} // namespace detail

/// `has(store: GlobalState, const key: str) -> bool`: whether `key` holds a
/// value (CAP-6).
bool has_GlobalState_str(hgraph::GlobalStateView store, std::string_view key);

/// `get<V>(store: GlobalState, const key: str) -> V`: a copy of the value
/// under `key`, which is of the type `V` the call gives (CAP-6). An absent
/// key fails, and so does a value of another type.
template <class V>
V get_GlobalState_str(hgraph::GlobalStateView store, std::string_view key,
                      std::source_location where = std::source_location::current()) {
    if (!store.contains(key)) {
        detail::store_missing(key, where);
    }
    const V* held = store.get(key).template try_as<V>();
    if (held == nullptr) {
        detail::store_type(key, where);
    }
    return *held;
}

/// `put<V>(store: GlobalState, const key: str, value: V)`: puts a copy of
/// `value` under `key`, which keeps the type of its first value (CAP-6).
template <class V>
void put_GlobalState_str_V(hgraph::GlobalStateView store, std::string_view key, const V& value,
                           std::source_location where = std::source_location::current()) {
    if (store.contains(key) && store.get(key).template try_as<V>() == nullptr) {
        detail::store_type(key, where);
    }
    // hgraph holds a value of a type it has a schema for.
    (void)hgraph::scalar_descriptor<V>::value_meta();
    store.set(key, hgraph::Value{value});
}

} // namespace hglrt
