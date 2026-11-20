// HGL's values as the C++ the target writes, and as hgraph holds them.
//
// A scalar of HGL is hgraph's own type (TYPE-1, TYPE-2): the aliases below
// name them as HGL does. A list is `hglrt::list<T>`, which is a
// `std::vector<T>` that hgraph can hash, and a tuple is a `std::tuple`.
// hgraph holds a list as one opaque value (an atomic): it stores, copies
// and compares it and never looks inside. A list it holds as configuration,
// a source's items, takes part in hgraph's interning of identical nodes,
// which hashes it; hgraph hashes a value through `std::hash`, which the
// standard library gives no `std::vector`, and which no program may give
// it for one. So a list is a type of this library until hgraph hashes a
// value through a trait of its own.
#pragma once

#include <hgraph/types/metadata/type_registry.h>
#include <hgraph/types/primitive_types.h>
#include <hgraph/types/static_schema.h>
#include <hgraph/util/date_time.h>

#include <cstddef>
#include <cstdint>
#include <functional>
#include <optional>
#include <string>
#include <tuple>
#include <type_traits>
#include <vector>

namespace hglrt {

/// `i64` (TYPE-1): a signed 64-bit integer.
using i64 = hgraph::Int;
/// `f64` (TYPE-1): an IEEE 754 binary64 number.
using f64 = hgraph::Float;
/// `str` (TYPE-1): UTF-8 text.
using str = hgraph::Str;
/// `datetime` (TYPE-2): an instant, in microseconds from the epoch in UTC.
using datetime = hgraph::DateTime;
/// `duration` (TYPE-2): a signed count of microseconds.
using duration = hgraph::TimeDelta;

/// `list<T>` (TYPE-4): a `std::vector<T>` with a hash, so that hgraph can
/// hold it as configuration (see the head of this file). It is a vector in
/// every other way, and a function that takes a `std::vector<T>` takes it.
template <class T>
struct list : std::vector<T> {
    using std::vector<T>::vector;
};

/// The hash of `value`, part by part: what `std::hash` gives a list (TYPE-8:
/// equal values hash alike). A tuple and an optional are hashed through
/// their parts, as the standard library hashes neither a tuple nor an
/// optional of every type.
template <class T>
std::size_t hash_of(const T& value);

namespace detail {

/// Mixes the hash `next` into `seed`, as `boost::hash_combine` does.
inline std::size_t combined(std::size_t seed, std::size_t next) {
    return seed ^ (next + 0x9e3779b97f4a7c15ULL + (seed << 6) + (seed >> 2));
}

template <class T>
struct is_tuple : std::false_type {};
template <class... T>
struct is_tuple<std::tuple<T...>> : std::true_type {};

template <class T>
struct is_optional : std::false_type {};
template <class T>
struct is_optional<std::optional<T>> : std::true_type {};

template <class T>
struct is_list : std::false_type {};
template <class T>
struct is_list<list<T>> : std::true_type {};

} // namespace detail

template <class T>
std::size_t hash_of(const T& value) {
    if constexpr (detail::is_tuple<T>::value) {
        std::size_t seed = std::tuple_size_v<T>;
        std::apply([&](const auto&... part) { ((seed = detail::combined(seed, hash_of(part))), ...); },
                   value);
        return seed;
    } else if constexpr (detail::is_optional<T>::value) {
        return value.has_value() ? detail::combined(1, hash_of(*value)) : 0;
    } else if constexpr (detail::is_list<T>::value) {
        std::size_t seed = value.size();
        for (const auto& element : value) {
            seed = detail::combined(seed, hash_of(element));
        }
        return seed;
    } else {
        return std::hash<T>{}(value);
    }
}

/// The name of the HGL type that the C++ type `T` holds, as HGL writes it
/// (`i64`, `list<tuple<datetime, f64>>`, `str?`): the name a list is
/// registered with hgraph under.
template <class T>
std::string type_name() {
    if constexpr (std::is_same_v<T, i64>) {
        return "i64";
    } else if constexpr (std::is_same_v<T, f64>) {
        return "f64";
    } else if constexpr (std::is_same_v<T, bool>) {
        return "bool";
    } else if constexpr (std::is_same_v<T, str>) {
        return "str";
    } else if constexpr (std::is_same_v<T, datetime>) {
        return "datetime";
    } else if constexpr (std::is_same_v<T, duration>) {
        return "duration";
    } else if constexpr (detail::is_list<T>::value) {
        return "list<" + type_name<typename T::value_type>() + ">";
    } else if constexpr (detail::is_optional<T>::value) {
        return type_name<typename T::value_type>() + "?";
    } else if constexpr (detail::is_tuple<T>::value) {
        return []<std::size_t... I>(std::index_sequence<I...>) {
            std::string parts;
            ((parts += (I == 0 ? "" : ", ") + type_name<std::tuple_element_t<I, T>>()), ...);
            return "tuple<" + parts + ">";
        }(std::make_index_sequence<std::tuple_size_v<T>>{});
    } else if constexpr (requires { T::hgl_name; }) {
        // A type the compiler writes (a struct, a node's state) names itself.
        return std::string{T::hgl_name};
    } else {
        static_assert(false, "hglrt::type_name: no HGL name for this type");
    }
}

} // namespace hglrt

template <class T>
struct std::hash<hglrt::list<T>> {
    std::size_t operator()(const hglrt::list<T>& value) const { return hglrt::hash_of(value); }
};

/// A list is registered with hgraph as an atomic, under `hgl.` and its HGL
/// type's name, the first time hgraph asks for its schema.
template <class T>
struct hgraph::scalar_descriptor<hglrt::list<T>> {
    [[nodiscard]] static constexpr bool is_concrete() noexcept { return true; }
    [[nodiscard]] static const ValueTypeMetaData* value_meta() {
        static const std::string name = "hgl." + hglrt::type_name<hglrt::list<T>>();
        return TypeRegistry::instance().register_scalar<hglrt::list<T>>(name);
    }
};
