// Core's operations on an endpoint (`hgl/native/endpoints.hgl`; the
// language's TIME-9, core's END-1 to END-10): `valid`, `all_valid`,
// `modified`, `delta_value`, `last_modified`, `invalidate`, `is_active`,
// `activate` and `passivate`.
//
// TIME-9: "`endpoint<T>` is the type of the endpoint of a temporal value of
// T, for a function that takes the endpoint and not its payload ... A call
// gives an `endpoint<T>` parameter a temporal input of the enclosing node by
// its name, a child of one ... or `out`". The generated code gives each of
// these natives the hgraph view the node's hook has for that endpoint, as it
// is: an input `hgraph::In<"name", S>`, its result `hgraph::Out<S>`, or a
// child of either, whose hgraph schema `S` is the view's `schema`. Each is a
// function template over the view's type, named as the compiler names an
// exported native (`fn_` and its name).
//
// What an endpoint publishes in a cycle, `delta<T>` (TYPE-16), is read from
// hgraph's delta by `endpoint_read`, the one place a temporal form says how
// it is read: a scalar's is written here, and each construct lane adds its
// form's (see `endpoint_read` below).
#pragma once

#include <hglrt/checked.h>
#include <hglrt/values.h>

#include <hgraph/types/generated.h>

#include <optional>
#include <source_location>
#include <type_traits>

namespace hglrt {

/// The hgraph schema of the endpoint view `E`: `TS<i64>` for an
/// `In<"price", TS<i64>>` and for an `Out<TS<i64>>`.
template <class E>
using schema_of = typename std::remove_cvref_t<E>::schema;

/// How an endpoint of the hgraph schema `Schema` is read where hgraph's own
/// view does not say it as HGL has it: the extension point of the temporal
/// forms. A construct lane gives its form a specialization of this, beside
/// its form's other code, with
///
///   - `delta_type`, the C++ type of `delta<T>` for the form's T, which is
///     `delta_t<T>` (values.h), and a static `delta(view)` that gives it
///     from what the view published this cycle (END-3, TYPE-16), for
///     `delta_value`; and,
///   - for a collection or a window, a static `all_valid(view)` (END-7,
///     END-9), for `all_valid`.
///
/// `valid`, `modified` and `last_modified` are the view's own for every
/// form (hgraph: "True when this view or at least one structural child has
/// a current value", END-7's `valid`). A form that has no specialization is
/// one the target does not write yet, and a call of `delta_value` or
/// `all_valid` of it does not compile.
template <class Schema>
struct endpoint_read;

/// A scalar (END-3: "For a scalar it is the value published"), whose delta
/// is its value: `delta<i64>` is `i64`.
template <class T>
struct endpoint_read<hgraph::TS<T>> {
    using delta_type = T;

    /// The value the endpoint holds: an input's typed value, and an output's
    /// as its erased value gives it.
    template <class E>
    static T delta(const E& endpoint) {
        if constexpr (std::is_same_v<std::remove_cvref_t<decltype(endpoint.value())>, T>) {
            return endpoint.value();
        } else {
            return endpoint.value().template checked_as<T>();
        }
    }
};

/// A nullable scalar (END-3: "For a nullable scalar it is the optional that
/// type gives it, which holds the value published or the null: a published
/// null is `delta<i64?>()` for an `i64?` endpoint, and is no absent
/// delta"): hgraph's `TS<Optional<T>>`, whose published null is a value
/// that holds nothing.
template <class T>
struct endpoint_read<hgraph::TS<hgraph::Optional<T>>> {
    using delta_type = std::optional<T>;

    template <class E>
    static std::optional<T> delta(const E& endpoint) {
        const hgraph::OptionalView held = endpoint.value().as_optional();
        if (!held.has_value()) {
            return std::nullopt;
        }
        return held.value().template checked_as<T>();
    }
};

/// `valid<T>(input: endpoint<T>) -> bool` (END-1, END-7, END-9, END-10):
/// whether the endpoint holds a payload, of a collection whether any child
/// does, of a window whether it holds a value, and of a `ref` parameter
/// whether it has been given a designation. False before the first
/// publication and after an invalidation (END-4).
template <class E>
bool fn_valid(const E& endpoint) {
    return endpoint.valid();
}

/// `all_valid<T>(input: endpoint<T>) -> bool` (END-7, END-9): whether every
/// element, field or entry of a collection is valid, one level deep, and
/// whether a window holds at least its minimum, as the form's
/// `endpoint_read` says. TIME-9: "`all_valid` takes only a collection or a
/// window; another endpoint is rejected (`endpoint.argument`)", so a scalar
/// has none.
template <class E>
    requires requires(const E& endpoint) { endpoint_read<schema_of<E>>::all_valid(endpoint); }
bool fn_all_valid(const E& endpoint) {
    return endpoint_read<schema_of<E>>::all_valid(endpoint);
}

/// `modified<T>(input: endpoint<T>) -> bool` (END-2): whether the endpoint
/// published in the current cycle, an equal value included. END-2: "An
/// input is not modified without being valid", and END-4: "An explicit
/// `modified(input)` guard is false on that transition", the cycle it is
/// invalidated in, so it is false for an endpoint that is not valid.
template <class E>
bool fn_modified(const E& endpoint) {
    return endpoint.valid() && endpoint.modified();
}

/// `delta_value<T>(input: endpoint<T>) -> delta<T>` (END-3): what the
/// endpoint published in the current cycle, as the form's `endpoint_read`
/// gives it. END-3: "The checker must prove `x` is valid and modified at
/// the call"; an endpoint that is not valid has no payload to read, which
/// fails the read with `value.unset_read` (TIME-1).
template <class E>
    requires requires(const E& endpoint) { endpoint_read<schema_of<E>>::delta(endpoint); }
typename endpoint_read<schema_of<E>>::delta_type
fn_delta_value(const E& endpoint, std::source_location where = std::source_location::current()) {
    if (!endpoint.valid()) {
        unset_read("the delta of an endpoint that is not valid is read", where);
    }
    return endpoint_read<schema_of<E>>::delta(endpoint);
}

/// `last_modified<T>(input: endpoint<T>) -> datetime?` (END-6): the
/// evaluation time of the publication the endpoint retains, an equal one's
/// own time included, kept through silence; "null while the endpoint is
/// invalid, including before its first publication and after
/// invalidation". Of a `ref` parameter, when it was last given a
/// designation (END-10).
template <class E>
std::optional<datetime> fn_last_modified(const E& endpoint) {
    if (!endpoint.valid()) {
        return std::nullopt;
    }
    return endpoint.last_modified_time();
}

/// `invalidate<T>(input: endpoint<T>)` (END-4): clears the validity,
/// modification, payload and delta of the node's own result, `out`, or of a
/// child of it. "Within a producer evaluation the last action determines
/// the state": a publication after it in the same evaluation leaves the
/// endpoint valid and modified. It takes an output's view: an input's
/// value is its producer's to clear.
template <class E>
    requires requires(const E& endpoint) { endpoint.begin_mutation(endpoint.evaluation_time()); }
void fn_invalidate(const E& endpoint) {
    static_cast<void>(endpoint.begin_mutation(endpoint.evaluation_time()).invalidate());
}

/// `is_active<T>(input: endpoint<T>) -> bool` (END-5, TIME-9): the current
/// activity of the node's direct input, whether a publication of it wakes
/// the node, read while it is not valid too.
template <class E>
    requires requires(const E& input) { input.active(); }
bool fn_is_active(const E& input) {
    return input.active();
}

/// `activate<T>(input: endpoint<T>)` (END-5, TIME-9): makes the node's
/// direct input active, so that each publication of it wakes the node.
template <class E>
    requires requires(E& input) { input.make_active(); }
void fn_activate(E& input) {
    input.make_active();
}

/// `passivate<T>(input: endpoint<T>)` (END-5, TIME-3: "Passivating an input
/// removes it as a wake source"): makes the node's direct input passive. It
/// is read as it was: "Active and passive inputs read the same validity,
/// payload and delta".
template <class E>
    requires requires(E& input) { input.make_passive(); }
void fn_passivate(E& input) {
    input.make_passive();
}

} // namespace hglrt
