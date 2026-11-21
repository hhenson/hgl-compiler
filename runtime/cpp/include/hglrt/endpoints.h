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
// form's (see `endpoint_read` below). How a form is read whole and published
// where it is a part of another (TYPE-9), `whole` and `publisher`, is here
// too, for each form's header to specialize. `delta_value` takes its form from its
// place (ERR-14): `fn_delta_value` is the plain read, and
// `fn_delta_value_checked` the read where the place takes
// `result<delta<T>, error>`, which is `hglrt::result` (failure.h), a
// `std::expected` of the delta and an HGL `error`.
#pragma once

#include <hglrt/checked.h>
#include <hglrt/failure.h>
#include <hglrt/values.h>

#include <hgraph/types/generated.h>

#include <cstddef>
#include <expected>
#include <format>
#include <optional>
#include <source_location>
#include <stdexcept>
#include <string>
#include <string_view>
#include <type_traits>
#include <utility>
#include <vector>

namespace hglrt {

/// The hgraph schema of the endpoint view `E`: `TS<i64>` for an
/// `In<"price", TS<i64>>` and for an `Out<TS<i64>>`.
template <class E>
using schema_of = typename std::remove_cvref_t<E>::schema;

namespace detail {

/// The child at `position`, as a connection with its own `schema` (TYPE-9,
/// TIME-9). hgraph's equivalent projection lives in its full wiring front
/// end; this needs only the generated-code interface.
inline hgraph::WiringPortRef projected_child(const hgraph::WiringPortRef& whole, std::size_t position,
                                            const hgraph::TSValueTypeMetaData* schema,
                                            const char* unbound) {
    const auto project = [position](std::vector<std::size_t> path) {
        path.push_back(position);
        return path;
    };
    switch (whole.source_kind()) {
    case hgraph::WiringPortRef::SourceKind::Peered:
        return hgraph::WiringPortRef::peered_source(whole.peered_node(), project(whole.peered_path()),
                                                  schema, whole.peered_output_kind());
    case hgraph::WiringPortRef::SourceKind::Structural:
        return whole.structural_children().at(position);
    case hgraph::WiringPortRef::SourceKind::Boundary:
        return whole.projected_boundary_source(project(whole.boundary_path()), schema);
    case hgraph::WiringPortRef::SourceKind::Delayed:
        return whole.projected_delayed_source(project(whole.delayed_path()), schema);
    case hgraph::WiringPortRef::SourceKind::Null:
        return hgraph::WiringPortRef::null_source(schema);
    case hgraph::WiringPortRef::SourceKind::Unbound:
        break;
    }
    // ERR-1: an unbound connection is an internal fault.
    throw std::logic_error(unbound);
}

} // namespace detail

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
///     END-9), for `all_valid`; and,
///   - for a form whose endpoint may tick with no delta, a static
///     `has_delta(view)`, whether it has one in the cycle. ERR-14: "Only a
///     window of a duration has a tick with no delta, one in which a value
///     only leaves it". A form without it has a delta wherever the endpoint
///     is modified.
///
/// A window's specialization may hold more that its own natives read, what
/// left it in the cycle for `removed_value` among them; those natives are
/// the windows work's, beside these, and named as these are
/// (`fn_removed_value_checked`).
///
/// A form may specialize `valid(view)` when hgraph's own validity includes
/// a collection that has no valid children (END-7). A form that has no specialization is
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
/// that holds nothing. Its delta is the nullable's (`nullable_delta`).
template <class T>
struct endpoint_read<hgraph::TS<hgraph::Optional<T>>> {
    using delta_type = delta_t<std::optional<T>>;

    template <class E>
    static delta_type delta(const E& endpoint) {
        const hgraph::OptionalView held = endpoint.value().as_optional();
        if (!held.has_value()) {
            return delta_type{};
        }
        return delta_type{held.value().template checked_as<T>()};
    }
};

// ---- A form as a part of another (TYPE-9, TIME-1, TIME-7) ------------------
//
// TYPE-9: "Temporalization recursively maps tuples/structs to child fields,
// lists to child positions, dictionaries to keyed children". A form made of
// parts, a list's elements, a struct's fields, a tuple's positions, reads and
// publishes each part through the part's own form, whichever header writes
// it, by these three traits, which each form specializes beside its other
// code: a scalar's and a nullable's here, a list's in list_endpoints.h, a
// struct's and a tuple's in structs.h. A part whose form has none of them is
// one the target does not write, and its whole does not compile.

namespace detail {
/// Whether `S` is the schema of a nullable scalar, `TS<Optional<T>>`, whose
/// child is not asked by `all_valid` (TIME-9: "an optional child aside: one
/// whose type is `T?` ... is not asked") and reads null where it is not valid
/// (TYPE-15).
template <class S>
struct nullable_schema : std::false_type {};
template <class T>
struct nullable_schema<hgraph::TS<hgraph::Optional<T>>> : std::true_type {};
} // namespace detail

/// The HGL value a temporal value of the hgraph schema `S` holds whole: T for
/// a scalar `TS<T>`, `std::optional<T>` for a nullable one, and each other
/// form's as its header says.
template <class S>
struct schema_value;

/// `schema_value`'s type.
template <class S>
using schema_value_t = typename schema_value<S>::type;

template <class T>
struct schema_value<hgraph::TS<T>> {
    using type = T;
};

template <class T>
struct schema_value<hgraph::TS<hgraph::Optional<T>>> {
    using type = std::optional<T>;
};

/// How a temporal value of the schema `S` is read whole: a static
/// `read(view)` gives its value, a `schema_value_t<S>`, or nothing where it
/// may not be read whole (TIME-1): a scalar's needs it valid, and a struct, a
/// tuple or a fixed list "requires `all_valid` of it".
template <class S>
struct whole;

template <class T>
struct whole<hgraph::TS<T>> {
    template <class E>
    static std::optional<T> read(const E& endpoint) {
        if (!endpoint.valid()) {
            return std::nullopt;
        }
        return endpoint_read<hgraph::TS<T>>::delta(endpoint);
    }
};

template <class T>
struct whole<hgraph::TS<hgraph::Optional<T>>> {
    template <class E>
    static std::optional<std::optional<T>> read(const E& endpoint) {
        if (!endpoint.valid()) {
            return std::nullopt;
        }
        // Its delta (`nullable_delta`) holds the value it holds.
        return endpoint_read<hgraph::TS<hgraph::Optional<T>>>::delta(endpoint).value;
    }
};

/// How a value is published to an output of the schema `S` (TIME-7): a static
/// `whole(out, value, where)` publishes a complete value, a
/// `schema_value_t<S>`, and `change(out, delta, where)` a delta of it, a
/// `delta_t<schema_value_t<S>>`, which fails where its form cannot take it.
template <class S>
struct publisher;

template <class T>
struct publisher<hgraph::TS<T>> {
    template <class O>
    static void whole(const O& out, const T& value, std::source_location) {
        out.set(value);
    }
    template <class O>
    static void change(const O& out, const T& value, std::source_location where) {
        whole(out, value, where);
    }
};

/// A nullable scalar publishes whole: a value, or a null (TYPE-15: "`return
/// null` for an `i64?` result a null, as `return delta<i64?>()` does"); its
/// delta (`nullable_delta`) holds the same.
template <class T>
struct publisher<hgraph::TS<hgraph::Optional<T>>> {
    template <class O>
    static void whole(const O& out, const std::optional<T>& value, std::source_location) {
        if (value.has_value()) {
            out.set(*value);
        } else {
            out.set(hgraph::Null);
        }
    }
    template <class O>
    static void change(const O& out, const delta_t<std::optional<T>>& delta, std::source_location where) {
        whole(out, delta.value, where);
    }
};

/// Publishes `value` to the output `out` as its form does (TIME-7): a
/// complete value of its type whole, and a delta of it as that change.
template <class O, class V>
void publish_as_form(const O& out, const V& value, std::source_location where) {
    using S = schema_of<O>;
    using T = schema_value_t<S>;
    using Given = std::remove_cvref_t<V>;
    if constexpr (std::is_same_v<Given, T>) {
        publisher<S>::whole(out, value, where);
    } else if constexpr (std::is_same_v<Given, delta_t<T>>) {
        publisher<S>::change(out, value, where);
    } else if constexpr (std::is_convertible_v<Given, T>) {
        publisher<S>::whole(out, T(value), where);
    } else {
        static_assert(false, "hglrt: neither a value of the output's type nor its delta");
    }
}

/// `valid<T>(input: endpoint<T>) -> bool` (END-1, END-7, END-9, END-10):
/// whether the endpoint holds a payload, of a collection whether any child
/// does, of a window whether it holds a value, and of a `ref` parameter
/// whether it has been given a designation. False before the first
/// publication and after an invalidation (END-4).
template <class E>
bool fn_valid(const E& endpoint) {
    if constexpr (requires { endpoint_read<schema_of<E>>::valid(endpoint); }) {
        return endpoint_read<schema_of<E>>::valid(endpoint);
    } else {
        return endpoint.valid();
    }
}

/// A logical child of a collection, whether or not it has a view (TIME-1).
/// A missing position or key is an invalid endpoint. Core operations take
/// a child with its own schema as they would take a view.
template <class V>
class Child {
  public:
    /// The child's hgraph schema.
    using schema = typename V::schema;

    /// A child the collection does not hold.
    Child() = default;

    /// The child that `view` is.
    explicit Child(V view) : view_(std::move(view)) {}

    /// Whether the collection has the child.
    [[nodiscard]] bool present() const noexcept { return view_.has_value(); }

    /// The child's view; only where it is `present`.
    [[nodiscard]] const V& view() const { return *view_; }

    /// END-7's `valid` of the child: false where the collection has none.
    [[nodiscard]] bool valid() const { return view_.has_value() && view_->valid(); }

    /// Whether the child published in the cycle.
    [[nodiscard]] bool modified() const { return view_.has_value() && view_->modified(); }

    /// The time of the publication the child retains; only where it is valid.
    [[nodiscard]] hgraph::DateTime last_modified_time() const { return view_->last_modified_time(); }

    /// The child's own value as its view reads it; only where it is valid.
    [[nodiscard]] decltype(auto) value() const { return view_->value(); }

  private:
    std::optional<V> view_{};
};

/// The plain read of a held endpoint raises at the consuming use
/// (ERR-14). hgraph 0.9.23 keeps the canonical error but discards the
/// exception subclass during node evaluation. Carry the HGL location in
/// this read's message so it survives that boundary.
template <class T>
T read_bound_child(result<T> read, std::source_location where) {
    if (read.has_value()) {
        return std::move(*read);
    }
    error failure = std::move(read).error();
    if (std::string_view{where.file_name()}.ends_with(".hgl")) {
        failure.message += std::format(", raised at {}:{}", where.file_name(), where.line());
    }
    throw hgraph::ExecutionFailure{std::move(failure)};
}

namespace detail {

template <class E>
struct is_child : std::false_type {};
template <class V>
struct is_child<Child<V>> : std::true_type {};

/// Whether the endpoint `endpoint`, a view or a `Child`, is there to read: a
/// view always is.
template <class E>
bool present(const E& endpoint) {
    if constexpr (is_child<E>::value) {
        return endpoint.present();
    } else {
        static_cast<void>(endpoint);
        return true;
    }
}

/// The view of the endpoint `endpoint`: a `Child`'s, or the view itself.
template <class E>
decltype(auto) viewed(const E& endpoint) {
    if constexpr (is_child<E>::value) {
        return endpoint.view();
    } else {
        return (endpoint);
    }
}

/// The view of an endpoint `E`, unwrapped from a `Child`.
template <class E>
using viewed_t = std::remove_cvref_t<decltype(viewed(std::declval<const E&>()))>;

} // namespace detail

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
/// invalidated in, so it is false for an endpoint that is not valid, as
/// hgraph's own is for a scalar.
template <class E>
bool fn_modified(const E& endpoint) {
    return fn_valid(endpoint) && endpoint.modified();
}

/// The C++ type of the delta of the endpoint view `E`, `delta<T>`.
template <class E>
using endpoint_delta_t = typename endpoint_read<schema_of<E>>::delta_type;

/// Whether the endpoint `endpoint` has a delta in the current cycle: it is
/// modified, and its form says it has one (`endpoint_read::has_delta`).
template <class E>
bool has_delta(const E& endpoint) {
    if (!fn_modified(endpoint)) {
        return false;
    }
    if constexpr (requires { endpoint_read<schema_of<E>>::has_delta(endpoint); }) {
        return endpoint_read<schema_of<E>>::has_delta(endpoint);
    } else {
        return true;
    }
}

/// `delta_value<T>(input: endpoint<T>) -> delta<T>` (END-3), read where its
/// place is no `result`: what the endpoint published in the current cycle,
/// as the form's `endpoint_read` gives it. ERR-14: "elsewhere it gives the
/// delta, and the read fails with that code where there is none", the code
/// `value.unset_read`: where the endpoint is not modified in the cycle, not
/// valid among it, or ticks with no delta. END-3: "The checker must prove
/// `x` is valid and modified at the call".
template <class E>
    requires requires(const E& endpoint) { endpoint_read<schema_of<E>>::delta(endpoint); }
endpoint_delta_t<E> fn_delta_value(const E& endpoint,
                                   std::source_location where = std::source_location::current()) {
    if (!has_delta(endpoint)) {
        unset_read("the delta of an endpoint that has none in the cycle is read", where);
    }
    return endpoint_read<schema_of<E>>::delta(endpoint);
}

/// `delta_value<T>(input: endpoint<T>)` where its place expects
/// `result<delta<T>, error>` (ERR-14): "`ok` of the delta, or `err` coded
/// `value.unset_read` where the endpoint has no delta in the cycle". For
/// every form but a window of a duration it is `ok` wherever the endpoint
/// is modified, so core's `record` reads every delta so.
template <class E>
    requires requires(const E& endpoint) { endpoint_read<schema_of<E>>::delta(endpoint); }
result<endpoint_delta_t<E>> fn_delta_value_checked(const E& endpoint) {
    if (!has_delta(endpoint)) {
        return detail::unset_result("the delta of an endpoint that has none in the cycle is read");
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
    if (!fn_valid(endpoint)) {
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
