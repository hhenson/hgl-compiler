// The temporal form of a nullable scalar, `T?` (TYPE-14, TYPE-15, TIME-7):
// hgraph's `TS<Optional<T>>`, whose published null is a value that holds
// nothing, valid and modified as any publication is. What the generated
// code reads of one and how it publishes to one are here; its delta is
// endpoints.h's (`endpoint_read<TS<Optional<T>>>`), the optional that a
// nullable's `delta<T?>` is (TYPE-16).
#pragma once

#include <hglrt/checked.h>
#include <hglrt/endpoints.h>
#include <hglrt/failure.h>
#include <hglrt/values.h>

#include <hgraph/types/generated.h>

#include <optional>
#include <source_location>
#include <string>
#include <utility>

namespace hglrt {

/// The payload of the nullable endpoint `input`, `TS<Optional<T>>`, read in
/// a hook (TIME-1): the `std::optional<T>` it holds, which holds nothing for
/// a published null (TYPE-15: "A present null is ordinary data, distinct
/// from an invalid temporal input or silence"). The read is checked when it
/// runs: one of an endpoint that is not valid fails with `value.unset_read`.
template <class E>
endpoint_delta_t<E> nullable_value(const E& input, std::source_location where = std::source_location::current()) {
    if (!input.valid()) {
        unset_read("the nullable read is not valid", where);
    }
    return endpoint_read<schema_of<E>>::delta(input);
}

/// The payload of the nullable endpoint `input` where a test against null
/// has proved it holds a value (ERR-12: "a binding a test has proved is not
/// null is read as its payload's type there"): the T. One that holds
/// nothing, which the analyser has ruled out, fails with no code.
template <class E>
auto nullable_payload(const E& input, std::source_location where = std::source_location::current()) ->
    typename endpoint_delta_t<E>::value_type {
    endpoint_delta_t<E> held = nullable_value(input, where);
    if (!held.has_value()) {
        fail_uncoded("a null is read where a test proved a value", where);
    }
    return *std::move(held);
}

/// `return value` and `out = value` of a nullable result (TIME-7): "`return
/// null` for an `i64?` result a null, as `return delta<i64?>()` does". The
/// value, a T or the optional that is its delta, ticks the output: a held T
/// as that T, and an optional that holds nothing as hgraph's published null.
template <class E>
void publish_nullable(const E& out, const endpoint_delta_t<E>& value) {
    if (value.has_value()) {
        out.set(*value);
    } else {
        out.set(hgraph::Null);
    }
}

} // namespace hglrt

/// A nullable value is registered with hgraph as an atomic, under `hgl.` and
/// its HGL type's name (`i64?`), the first time hgraph asks for its schema,
/// as a list is (values.h): so that hgraph holds one where it holds a value,
/// under a key of the run's store (CAP-6) and in a node's configuration. Its
/// equality and hash are the optional's own. This is the ordinary value
/// `T?`; a temporal one is hgraph's `TS<Optional<T>>` (above).
template <class T>
struct hgraph::scalar_descriptor<std::optional<T>> {
    [[nodiscard]] static constexpr bool is_concrete() noexcept { return true; }
    [[nodiscard]] static const ValueTypeMetaData* value_meta() {
        static const std::string name = "hgl." + hglrt::type_name<std::optional<T>>();
        return TypeRegistry::instance().register_scalar<std::optional<T>>(name);
    }
};
