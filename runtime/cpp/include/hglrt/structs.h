// The temporal form of a struct and of a tuple (TYPE-9: "Temporalization
// recursively maps tuples/structs to child fields"), and of an atomic struct
// (TYPE-9: "`atomic<V>` stops expansion at a complete ordinary V").
//
// A struct the compiler writes is a plain aggregate (collections.h); beside
// it the compiler writes `hgraph::native_bundle<S>`, its nominal schema and
// its members, so that hgraph's `NativeTSB<S>` is its temporal form: a
// nominal bundle whose whole value is the struct, a child endpoint for each
// field. A tuple is hgraph's structural bundle, `UnNamedTSB`, a child for
// each position, named by its position. An atomic struct is `TS<S>`, one
// endpoint whose value is the whole struct.
//
// What the generated code reads of these and publishes to them is here: a
// child's view (`child`) and a child's port in a graph (`field_port`), the
// whole value read checked (`whole_value`), a publication of a whole value
// or of a delta (`publish`), a field of an atomic struct's connection
// (`atomic_field`), and each form's `endpoint_read` (endpoints.h): its delta
// (TYPE-16), `all_valid` (TIME-9) and the whole value it holds, where it
// can be read whole (`held`).
#pragma once

#include <hglrt/checked.h>
#include <hglrt/collections.h>
#include <hglrt/endpoints.h>
#include <hglrt/failure.h>
#include <hglrt/nullables.h>
#include <hglrt/traces.h>
#include <hglrt/values.h>

#include <hgraph/types/generated.h>
#include <hgraph/types/native_bundle.h>

#include <cstddef>
#include <optional>
#include <source_location>
#include <stdexcept>
#include <string>
#include <tuple>
#include <type_traits>
#include <utility>
#include <vector>

namespace hglrt {

namespace detail {

/// The fields of a bundle schema `S`: a struct's `NominalTSB` or a tuple's
/// `UnNamedTSB`, each a `hgraph::Field` of its name and its schema.
template <class S>
struct bundle_schema : std::false_type {};
template <class T, class... F>
struct bundle_schema<hgraph::NominalTSB<T, F...>> : std::true_type {
    using fields = std::tuple<F...>;
};
template <class... F>
struct bundle_schema<hgraph::UnNamedTSB<F...>> : std::true_type {
    using fields = std::tuple<F...>;
};

/// True where `S` is a struct's or a tuple's schema.
template <class S>
concept bundle = bundle_schema<S>::value;

/// The `I`-th field of the bundle schema `S`.
template <std::size_t I, class S>
using field_of = std::tuple_element_t<I, typename bundle_schema<S>::fields>;

/// How many fields the bundle schema `S` has.
template <class S>
inline constexpr std::size_t field_count = std::tuple_size_v<typename bundle_schema<S>::fields>;

/// The members of the struct `T` and of its delta, `delta_t<T>`, each a
/// pointer to a member in the order of its fields (collections.h).
template <class T>
inline constexpr auto& members_of = fields<T>::members;
template <class T>
inline constexpr auto& delta_members_of = fields<delta_t<T>>::members;

/// Whether `all_valid` asks the child endpoint `view` (TIME-9: "one level
/// deep, an optional child aside: one whose type is `T?` ... is not
/// asked"): whether it is valid, or true for an optional one.
template <class E>
bool asked_valid(const E& view) {
    return nullable_schema<schema_of<E>>::value || view.valid();
}

/// Whether the child endpoint `view` can be read whole: a struct or a tuple
/// whose every part can, a nullable always (null where it is not valid), and
/// any other as its own form reads it whole (`whole`).
template <class E>
bool readable(const E& view);

/// The whole value of the child endpoint `view`, which can be read so.
template <class E>
auto value_of(const E& view);

/// Publishes `value`, a whole value or a delta, to the child output `view`.
template <class E, class V>
void publish_part(const E& view, const V& value);

} // namespace detail

/// A struct's value is its aggregate, and a tuple's a `std::tuple` of its
/// positions' values (TYPE-9).
template <class T, class... F>
struct schema_value<hgraph::NominalTSB<T, F...>> {
    using type = T;
};
template <class... F>
struct schema_value<hgraph::UnNamedTSB<F...>> {
    using type = std::tuple<schema_value_t<typename F::schema>...>;
};

/// The view of the `I`-th field of `view`, an input or an output of a
/// struct or a tuple (TIME-1: "A child of a collection ... a field ... is an
/// endpoint whether or not it is valid"): hgraph's view of that child, of
/// the field's schema, which the natives of endpoints.h take as they take
/// any endpoint.
template <std::size_t I, class E>
auto child(const E& view) {
    using S = schema_of<E>;
    static_assert(detail::bundle<S>, "hglrt::child: a field of a struct or a position of a tuple");
    return view.template field<detail::field_of<I, S>::name_sv>();
}

/// The connection of the `I`-th field of `port`, a connection of a struct or
/// a tuple (EXPR-7, TYPE-9: "Temporalization recursively maps tuples/structs
/// to child fields"): the child's own output, which ticks where that child
/// publishes and not where a sibling does. It is the projection hgraph's
/// own makes of a bundle's field (`subgraph_wiring_detail::tsb_field_ref`,
/// which only its heavier front end, `graph_wiring.h`, holds), for each kind
/// of source a port may have.
template <std::size_t I, class S>
hgraph::Port<typename detail::field_of<I, S>::schema> field_port(const hgraph::Port<S>& port) {
    using Field = typename detail::field_of<I, S>::schema;
    const hgraph::WiringPortRef whole = port.erased();
    const hgraph::TSValueTypeMetaData* schema = hgraph::schema_descriptor<Field>::ts_meta();
    const auto within = [](std::vector<std::size_t> path) {
        path.push_back(I);
        return path;
    };
    switch (whole.source_kind()) {
    case hgraph::WiringPortRef::SourceKind::Peered:
        return hgraph::Port<Field>{port.wiring(),
                                   hgraph::WiringPortRef::peered_source(whole.peered_node(), within(whole.peered_path()),
                                                                        schema, whole.peered_output_kind())};
    case hgraph::WiringPortRef::SourceKind::Structural:
        return hgraph::Port<Field>{port.wiring(), whole.structural_children()[I]};
    case hgraph::WiringPortRef::SourceKind::Boundary:
        return hgraph::Port<Field>{port.wiring(),
                                   whole.projected_boundary_source(within(whole.boundary_path()), schema)};
    case hgraph::WiringPortRef::SourceKind::Delayed:
        return hgraph::Port<Field>{port.wiring(),
                                   whole.projected_delayed_source(within(whole.delayed_path()), schema)};
    case hgraph::WiringPortRef::SourceKind::Null:
        return hgraph::Port<Field>{port.wiring(), hgraph::WiringPortRef::null_source(schema)};
    case hgraph::WiringPortRef::SourceKind::Unbound:
        break;
    }
    throw std::logic_error("hglrt::field_port: the connection is not bound");
}

/// The endpoint of a struct, `NativeTSB<T>`: hgraph's nominal bundle whose
/// whole value is the aggregate `T`.
template <class T, class... F>
struct endpoint_read<hgraph::NominalTSB<T, F...>> {
    using schema = hgraph::NominalTSB<T, F...>;
    using value_type = T;
    using delta_type = delta_t<T>;

    /// TIME-9: "`all_valid` is true where every one is, by its own `valid`,
    /// one level deep ... It is true of a valid empty collection.
    /// `all_valid` of a collection that is not valid is false."
    template <class E>
    static bool all_valid(const E& view) {
        return view.valid() && [&]<std::size_t... I>(std::index_sequence<I...>) {
            return (detail::asked_valid(child<I>(view)) && ...);
        }(std::index_sequence_for<F...>{});
    }

    /// Whether the struct can be read whole: it is valid, and every field
    /// can be, a struct's whole, an optional one always (null where it is
    /// not valid), and any other's by its `valid`.
    template <class E>
    static bool complete(const E& view) {
        return view.valid() && [&]<std::size_t... I>(std::index_sequence<I...>) {
            return (detail::readable(child<I>(view)) && ...);
        }(std::index_sequence_for<F...>{});
    }

    /// The struct, read from its fields, which can each be read whole.
    template <class E>
    static T value(const E& view) {
        T whole{};
        [&]<std::size_t... I>(std::index_sequence<I...>) {
            ((whole.*std::get<I>(detail::members_of<T>) = detail::value_of(child<I>(view))), ...);
        }(std::index_sequence_for<F...>{});
        return whole;
    }

    /// TYPE-16: a struct's delta has "a property for each field ... of type
    /// `delta<F>?`: null where the field did not change": each field that
    /// published in the cycle, with its delta.
    template <class E>
    static delta_type delta(const E& view) {
        delta_type change{};
        [&]<std::size_t... I>(std::index_sequence<I...>) {
            (
                [&] {
                    const auto part = child<I>(view);
                    if (fn_modified(part)) {
                        change.*std::get<I>(detail::delta_members_of<T>) =
                            endpoint_read<schema_of<decltype(part)>>::delta(part);
                    }
                }(),
                ...);
        }(std::index_sequence_for<F...>{});
        return change;
    }

    /// The whole value the struct holds, where it can be read whole, and
    /// none where a part of it is not valid.
    template <class E>
    static std::optional<T> held(const E& view) {
        if (!complete(view)) {
            return std::nullopt;
        }
        return value(view);
    }
};

/// The endpoint of a tuple, hgraph's `UnNamedTSB` of a field for each
/// position.
template <class... F>
struct endpoint_read<hgraph::UnNamedTSB<F...>> {
    using schema = hgraph::UnNamedTSB<F...>;
    using value_type = schema_value_t<schema>;
    using delta_type = delta_t<value_type>;

    /// TIME-9, as a struct's: every position valid by its own `valid`.
    template <class E>
    static bool all_valid(const E& view) {
        return view.valid() && [&]<std::size_t... I>(std::index_sequence<I...>) {
            return (detail::asked_valid(child<I>(view)) && ...);
        }(std::index_sequence_for<F...>{});
    }

    /// Whether the tuple can be read whole: every position can.
    template <class E>
    static bool complete(const E& view) {
        return view.valid() && [&]<std::size_t... I>(std::index_sequence<I...>) {
            return (detail::readable(child<I>(view)) && ...);
        }(std::index_sequence_for<F...>{});
    }

    /// The tuple, read from its positions, which can each be read whole.
    template <class E>
    static value_type value(const E& view) {
        return [&]<std::size_t... I>(std::index_sequence<I...>) {
            return value_type{detail::value_of(child<I>(view))...};
        }(std::index_sequence_for<F...>{});
    }

    /// TYPE-16: "`delta<tuple<f64, i64>>` is `tuple<f64?, i64?>`": each
    /// position that published in the cycle, with its delta, and null for
    /// each other.
    template <class E>
    static delta_type delta(const E& view) {
        return [&]<std::size_t... I>(std::index_sequence<I...>) {
            return delta_type{[&]() -> std::tuple_element_t<I, delta_type> {
                const auto part = child<I>(view);
                if (!fn_modified(part)) {
                    return std::nullopt;
                }
                return endpoint_read<schema_of<decltype(part)>>::delta(part);
            }()...};
        }(std::index_sequence_for<F...>{});
    }

    /// The whole value the tuple holds, or none where a part is not valid.
    template <class E>
    static std::optional<value_type> held(const E& view) {
        if (!complete(view)) {
            return std::nullopt;
        }
        return value(view);
    }
};

/// The whole value of `view`, a struct's or a tuple's endpoint, read in a
/// hook (TIME-1): "A read of a struct, a tuple or a fixed list whole ...
/// requires `all_valid` of it", checked when it runs. One that cannot be read
/// whole, a part of it not valid, fails with `value.unset_read`.
template <class E>
auto whole_value(const E& view, std::source_location where = std::source_location::current()) {
    using Read = endpoint_read<schema_of<E>>;
    if (!Read::complete(view)) {
        unset_read("a struct or a tuple read whole is not all valid", where);
    }
    return Read::value(view);
}

/// `return value` and `out = value` of a struct or a tuple (TIME-7): "take
/// a complete value of the result's type T, published whole, or a
/// `delta<T>` (TYPE-16), published as that change". A whole value publishes
/// every field, the same value again included; a delta publishes each field
/// it holds, and leaves each field that is null as it was ("In a tuple's
/// position or a struct's field, null leaves the position or the field as
/// it was"). Each field publishes as its own form does.
template <class E, class V>
    requires detail::bundle<schema_of<E>>
void publish(const E& out, const V& value) {
    using S = schema_of<E>;
    using Read = endpoint_read<S>;
    using Whole = typename Read::value_type;
    constexpr std::size_t count = detail::field_count<S>;
    if constexpr (std::is_same_v<V, Whole>) {
        [&]<std::size_t... I>(std::index_sequence<I...>) {
            if constexpr (detail::is_tuple<Whole>::value) {
                (detail::publish_part(child<I>(out), std::get<I>(value)), ...);
            } else {
                (detail::publish_part(child<I>(out), value.*std::get<I>(detail::members_of<Whole>)), ...);
            }
        }(std::make_index_sequence<count>{});
    } else {
        static_assert(std::is_same_v<V, typename Read::delta_type>,
                      "hglrt::publish: a whole value of the result's type, or its delta");
        [&]<std::size_t... I>(std::index_sequence<I...>) {
            (
                [&] {
                    const auto& part = [&]() -> const auto& {
                        if constexpr (detail::is_tuple<Whole>::value) {
                            return std::get<I>(value);
                        } else {
                            return value.*std::get<I>(detail::delta_members_of<Whole>);
                        }
                    }();
                    if (part.has_value()) {
                        detail::publish_part(child<I>(out), *part);
                    }
                }(),
                ...);
        }(std::make_index_sequence<count>{});
    }
}

/// A struct or a tuple read whole as a part of another form, a list's
/// element or a dictionary's entry (TIME-1: "So does one of a child that is
/// a struct, a tuple or a fixed list ... requires `all_valid`"): its value
/// where it can be read whole, and none otherwise.
template <class S>
    requires detail::bundle<S>
struct whole<S> {
    template <class E>
    static std::optional<schema_value_t<S>> read(const E& view) {
        return endpoint_read<S>::held(view);
    }
};

/// A struct or a tuple published as a part of another form, as `publish`
/// publishes one (TIME-7).
template <class S>
    requires detail::bundle<S>
struct publisher<S> {
    template <class O>
    static void whole(const O& out, const schema_value_t<S>& value, std::source_location) {
        hglrt::publish(out, value);
    }
    template <class O>
    static void change(const O& out, const delta_t<schema_value_t<S>>& delta, std::source_location) {
        hglrt::publish(out, delta);
    }
};

namespace detail {

template <class E>
bool readable(const E& view) {
    using S = schema_of<E>;
    if constexpr (bundle<S>) {
        return endpoint_read<S>::complete(view);
    } else if constexpr (nullable_schema<S>::value) {
        return true;
    } else {
        return ::hglrt::whole<S>::read(view).has_value();
    }
}

template <class E>
auto value_of(const E& view) {
    using S = schema_of<E>;
    if constexpr (bundle<S>) {
        return endpoint_read<S>::value(view);
    } else if constexpr (nullable_schema<S>::value) {
        // TIME-1: "In the value it reads, an optional child (TIME-9) that is
        // not valid is null".
        return view.valid() ? endpoint_read<S>::delta(view) : endpoint_delta_t<E>{};
    } else {
        // A scalar's, an enum's and a list's whole value, as its form reads
        // it (`whole`), which `readable` has said it can.
        return *::hglrt::whole<S>::read(view);
    }
}

template <class E, class V>
void publish_part(const E& view, const V& value) {
    if constexpr (bundle<schema_of<E>>) {
        hglrt::publish(view, value);
    } else {
        publish_as_form(view, value, std::source_location::current());
    }
}

} // namespace detail

/// The node of a field of an atomic struct's connection: at each
/// publication of the whole struct it publishes the field, as its form
/// publishes a whole value (TYPE-9: "`atomic<V>` stops expansion at a
/// complete ordinary V", so the struct has no child, and the field follows
/// "each whole-struct publication", the same value again included).
template <class S, std::size_t I, class Field>
struct AtomicField {
    static constexpr auto name = "hgl.atomic_field";

    static void eval(hgraph::In<"whole", hgraph::TS<S>> whole, hgraph::Out<Field> out) {
        const S value = checked_value(whole);
        detail::publish_part(out, value.*std::get<I>(detail::members_of<S>));
    }
};

/// The connection of the `I`-th field of `whole`, a connection of an
/// atomic struct (EXPR-7), whose form is `Field`.
template <std::size_t I, class Field, class S>
hgraph::Port<Field> atomic_field(hgraph::Wiring& wiring, const hgraph::Port<hgraph::TS<S>>& whole) {
    return hgraph::wire<AtomicField<S, I, Field>>(wiring, whole);
}

} // namespace hglrt

namespace hglrt::testing {

} // namespace hglrt::testing

namespace hglrt {

/// What a trace of an atomic struct `S`, `atomic<S>`, is a trace of (TST-5,
/// TST-16): its value is the whole struct, and so is its delta (TYPE-16:
/// "`atomic<V>`: V: an atomic publishes whole"), which the struct's own
/// delta, field by field, is not. A trace's cell of it is `S`, what core's
/// `record` of `atomic<S>` keeps and its `replay` is given.
template <class S>
struct atomic_trace {
    S value;

    friend bool operator==(const atomic_trace&, const atomic_trace&) = default;
};

template <class S>
struct delta_of<atomic_trace<S>> {
    using type = S;
};

} // namespace hglrt

namespace hglrt::testing {

/// A whole value of an atomic struct publishes the struct (TST-9).
template <class S>
struct whole_publication<atomic_trace<S>> {
    static S delta(const atomic_trace<S>& value) { return value.value; }
};

/// The recording of an eval of an atomic struct `S` as a recording of `S`,
/// to compare with a trace whose cells are `S` (TST-4, TST-5): each whole
/// publication is the delta of every field (TST-9), and the value the result
/// then held is the struct published.
template <class S>
Recording<S> as_struct_recording(const Recording<atomic_trace<S>>& atomic) {
    Recording<S> out{atomic.start, {}, Held<S>{}};
    for (const auto& [at, value] : atomic.entries) {
        out.entries.emplace_back(at, whole_publication<S>::delta(value));
        out.held->emplace_back(at, value);
    }
    return out;
}

/// Whether the trace type `Trace` is of an atomic struct's trace.
template <class Trace>
inline constexpr bool atomic_traced = false;
template <class S>
inline constexpr bool atomic_traced<Recording<atomic_trace<S>>> = true;

/// Where an eval of an atomic struct, which a `let` may hold, and a trace of
/// the struct's whole values first differ (TST-4): the recording is read as
/// one of the struct (`as_struct_recording`). Found by argument-dependent
/// lookup from `expect_same` and `same`.
template <class S, class Right>
    requires(!atomic_traced<Right>)
std::optional<std::string> difference(const Recording<atomic_trace<S>>& left, const Right& right) {
    return difference(as_struct_recording(left), right);
}

/// The same, the eval on the right.
template <class Left, class S>
    requires(!atomic_traced<Left>)
std::optional<std::string> difference(const Left& left, const Recording<atomic_trace<S>>& right) {
    return difference(left, as_struct_recording(right));
}

namespace detail {
/// The delta of type `D` that publishing the part `part` whole records:
/// the part itself where `D` is its own type, an atomic field's payload, and
/// otherwise its whole value's delta as a part (`whole_part_delta`).
template <class D, class P>
D whole_part(const P& part) {
    if constexpr (std::is_same_v<D, P>) {
        return part;
    } else {
        return whole_part_delta(part);
    }
}
} // namespace detail

/// What publishing a whole struct records (TST-9: "a T whole, which
/// replaces what the input held"; TST-16: "A T published whole is held as
/// its delta too"): its delta with every field, each as its whole value
/// records it. Which that is is the delta's property's to say: an atomic
/// field, whose C++ value is its payload's, publishes whole, and its
/// property holds the payload (TYPE-16: "`atomic<V>`: V").
template <class T>
    requires requires { hgraph::native_bundle<T>::members; }
struct whole_publication<T> {
    static delta_t<T> delta(const T& value) {
        delta_t<T> change{};
        constexpr std::size_t count = std::tuple_size_v<std::remove_cvref_t<decltype(fields<T>::members)>>;
        [&]<std::size_t... I>(std::index_sequence<I...>) {
            (
                [&] {
                    auto& property = change.*std::get<I>(hglrt::detail::delta_members_of<T>);
                    property = detail::whole_part<typename std::remove_cvref_t<decltype(property)>::value_type>(
                        value.*std::get<I>(hglrt::detail::members_of<T>));
                }(),
                ...);
        }(std::make_index_sequence<count>{});
        return change;
    }
};

} // namespace hglrt::testing

namespace hglrt {

/// The text of a value of an atomic struct is the struct's.
template <class S>
struct value_form<atomic_trace<S>> {
    static std::string text(const atomic_trace<S>& value) { return testing::text_of(value.value); }
};

/// The name of a struct and of its delta, the compiler's `type_named`, and
/// their text, as the language writes them (TYPE-5, TYPE-16): `Quote(bid: 1,
/// ask: 2)`, and `delta<Quote>(bid: 1)`, a property that is null left out
/// ("A property left out is ... null for a struct's field").
template <class T>
    requires requires { type_named<T>::name; }
struct value_form<T> {
    static std::string name() { return std::string{type_named<T>::name}; }

    static std::string text(const T& value)
        requires requires {
            type_named<T>::fields;
            type_named<T>::delta;
            fields<T>::members;
        }
    {
        std::string parts;
        const auto& members = fields<T>::members;
        [&]<std::size_t... I>(std::index_sequence<I...>) {
            (
                [&] {
                    const auto& field = value.*std::get<I>(members);
                    if constexpr (type_named<T>::delta) {
                        if (!field.has_value()) {
                            return;
                        }
                        parts += (parts.empty() ? "" : ", ") + std::string{type_named<T>::fields[I]} + ": " +
                                 testing::text_of(*field);
                    } else {
                        parts += (parts.empty() ? "" : ", ") + std::string{type_named<T>::fields[I]} + ": " +
                                 testing::text_of(field);
                    }
                }(),
                ...);
        }(std::make_index_sequence<std::tuple_size_v<std::remove_cvref_t<decltype(members)>>>{});
        return name() + "(" + parts + ")";
    }
};

} // namespace hglrt

namespace hglrt {

/// The value schema of a list field in a struct's nominal bundle (TYPE-9):
/// the one hgraph holds the field's temporal form, the `lists` family's
/// `TSL` of the element's form, as, which a bundle's held field must be
/// (hgraph's `TypeRegistry::tsb`): its list of the element's value schema
/// `E`, of `N` positions where it is fixed. Only a schema: the struct's
/// value is read and published field by field (`endpoint_read`, `publish`),
/// a list field as its own form does.
template <class E, std::size_t N = 0>
struct list_value {};

} // namespace hglrt

template <class E, std::size_t N>
struct hgraph::scalar_descriptor<hglrt::list_value<E, N>> {
    [[nodiscard]] static constexpr bool is_concrete() noexcept { return scalar_descriptor<E>::is_concrete(); }
    [[nodiscard]] static const ValueTypeMetaData* value_meta() {
        if constexpr (N == 0) {
            return TypeRegistry::instance().list(scalar_descriptor<E>::value_meta());
        } else {
            return TypeRegistry::instance().fixed_list(scalar_descriptor<E>::value_meta(), N);
        }
    }
};
