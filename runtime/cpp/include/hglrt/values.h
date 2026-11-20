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
//
// An enum is a C++ enumeration of its members (TYPE-7), and names itself
// through `hgl_enum`, which the compiler writes beside it (`EnumNames`): the
// library names it, prints its members and registers it with hgraph, an
// atomic, through that alone.
//
// What a temporal value publishes in one cycle, its `delta<T>` (TYPE-16), is
// an ordinary value too: `delta_t<T>`. It is T itself for a scalar and for
// a nullable one, a tuple of optionals for a tuple, and `list_delta` for a
// list. A set's, a dictionary's and a struct's are the collections' and the
// structs' work (docs/cpp-target.md, "Not decided here").
#pragma once

#include <hgraph/types/metadata/type_registry.h>
#include <hgraph/types/primitive_types.h>
#include <hgraph/types/static_schema.h>
#include <hgraph/types/temporal.h>
#include <hgraph/util/date_time.h>

#include <array>
#include <concepts>
#include <cstddef>
#include <cstdint>
#include <functional>
#include <map>
#include <optional>
#include <set>
#include <string>
#include <string_view>
#include <tuple>
#include <type_traits>
#include <utility>
#include <vector>

namespace hglrt {

/// `i64` (TYPE-1): a signed 64-bit integer.
using i64 = hgraph::Int;
/// `f64` (TYPE-1): an IEEE 754 binary64 number.
using f64 = hgraph::Float;
/// `str` (TYPE-1): UTF-8 text.
using str = hgraph::Str;
/// `bytes` (TYPE-1): a sequence of octets.
using bytes = hgraph::Bytes;
/// `date` (TYPE-2): a Gregorian date of the years 1 to 9999.
using date = hgraph::Date;
/// `time` (TYPE-2): a time of day, in microseconds from midnight.
using time = hgraph::Time;
/// `datetime` (TYPE-2): an instant, in microseconds from the epoch in UTC.
using datetime = hgraph::DateTime;
/// `duration` (TYPE-2): a signed count of microseconds.
using duration = hgraph::TimeDelta;
/// `civil_datetime` (TYPE-2): a local date and time, in no zone.
using civil_datetime = hgraph::CivilDateTime;
/// `zoned_datetime` (TYPE-2): an instant, the zone it is resolved in, and
/// that zone's offset at it.
using zoned_datetime = hgraph::ZonedDateTime;
/// `tenor` (TYPE-17): a count of months and a count of days, or a count of
/// business days, hgraph's `Tenor` (tenors.h).
using tenor = hgraph::Tenor;

/// `list<T>` (TYPE-4): a `std::vector<T>` with a hash, so that hgraph can
/// hold it as configuration (see the head of this file). It is a vector in
/// every other way, and a function that takes a `std::vector<T>` takes it.
template <class T>
struct list : std::vector<T> {
    using std::vector<T>::vector;
};

/// What names an enum of HGL (TYPE-7): its type, by its name qualified with
/// its module's path as a test's is (TST-15), `demo.trade::Side` or
/// `core::CmpResult`; and each member, its value and its name, in the order
/// declared. An enum `E` gives it through a function `hgl_enum(E)`, which
/// takes any value of `E` and is found by argument-dependent lookup: the
/// compiler writes one beside each enum of a program, in its namespace,
///
///     constexpr ::hglrt::EnumNames<Side, 2> hgl_enum(Side) noexcept {
///         return {"demo.trade::Side", {{Side::buy, "buy"}, {Side::sell, "sell"}}};
///     }
///
/// and it is all an enum needs to be a value of the library: its HGL name
/// (`type_name`), its text (TYPE-7: "`Type.member` names one", testing.h's
/// `text_of`), and its schema, with which hgraph holds it, a list of it and a
/// recording of it.
template <class E, std::size_t N>
struct EnumNames {
    /// The enum's name, qualified with its module's path.
    std::string_view type;
    /// Each member and its name, as HGL writes it.
    std::pair<E, std::string_view> members[N];
};

/// An enumeration the library can name: one whose `hgl_enum` gives its
/// `EnumNames`.
template <class E>
concept named_enum = std::is_enum_v<E> && requires(E value) {
    { hgl_enum(value).type } -> std::convertible_to<std::string_view>;
};

template <class L>
struct list_delta;

/// `delta<T>` (TYPE-16): what a temporal value of T publishes in one cycle.
/// "For most T it is another type the language has: `delta<f64>` is `f64`,
/// and `delta<tuple<f64, i64>>` is `tuple<f64?, i64?>` ... A nullable's
/// delta is an optional: it holds what was published, a null among them"; a
/// list's is a type of its own, `list_delta`.
template <class T>
struct delta_of {
    using type = T;
};

/// `delta_of`'s type.
template <class T>
using delta_t = typename delta_of<T>::type;

template <class T>
struct delta_of<std::optional<T>> {
    using type = std::optional<delta_t<T>>;
};

/// A tuple's delta: each position's delta, or null where the position does
/// not change ("In a tuple's position or a struct's field, null leaves the
/// position or the field as it was").
template <class... T>
struct delta_of<std::tuple<T...>> {
    using type = std::tuple<std::optional<delta_t<T>>...>;
};

template <class T>
struct delta_of<list<T>> {
    using type = list_delta<list<T>>;
};

template <class T, std::size_t N>
struct delta_of<std::array<T, N>> {
    using type = list_delta<std::array<T, N>>;
};

/// `delta<list<V>>` and `delta<list<V, N>>`, `L` (TYPE-16): "`updated`, each
/// changed position with its delta; `removed`, the positions that lost their
/// value, so a fixed list can say a position became invalid; and
/// `size_modified`, set only when the size moves up or down, to the new
/// size". It keeps its list's type whole in its own: the delta of a
/// `list<i64>` is no delta of a `list<i64, 2>`. Two are equal where they name
/// the same changes.
template <class L>
struct list_delta {
    /// The list it is the delta of.
    using list_type = L;

    /// Each position updated, with its delta, by position.
    std::map<i64, delta_t<typename L::value_type>> updated{};
    /// Each position that lost its value.
    std::set<i64> removed{};
    /// The list's new size, where it moved; a negative one written shrinks
    /// the list by that count.
    std::optional<i64> size_modified{};

    friend bool operator==(const list_delta&, const list_delta&) = default;
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

/// A fixed list, `list<T, N>` (TYPE-4), is a `std::array<T, N>`.
template <class T>
struct is_fixed_list : std::false_type {};
template <class T, std::size_t N>
struct is_fixed_list<std::array<T, N>> : std::true_type {};

template <class T>
struct is_list_delta : std::false_type {};
template <class L>
struct is_list_delta<list_delta<L>> : std::true_type {};

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
    } else if constexpr (detail::is_list<T>::value || detail::is_fixed_list<T>::value) {
        std::size_t seed = value.size();
        for (const auto& element : value) {
            seed = detail::combined(seed, hash_of(element));
        }
        return seed;
    } else if constexpr (detail::is_list_delta<T>::value) {
        std::size_t seed = detail::combined(value.updated.size(), value.removed.size());
        for (const auto& [position, change] : value.updated) {
            seed = detail::combined(detail::combined(seed, hash_of(position)), hash_of(change));
        }
        for (const i64 position : value.removed) {
            seed = detail::combined(seed, hash_of(position));
        }
        return detail::combined(seed, hash_of(value.size_modified));
    } else {
        return std::hash<T>{}(value);
    }
}

/// What a header of this library says of a value type it declares that the
/// chains of `type_name` and of `testing::text_of` (traces.h) do not know, a
/// set's or a dictionary's among them (keyed.h): a specialization gives
/// `name()`, the HGL name of the type, and `text(value)`, a value of it as the
/// language writes it. The primary says nothing.
template <class T>
struct value_form {};

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
    } else if constexpr (std::is_same_v<T, bytes>) {
        return "bytes";
    } else if constexpr (std::is_same_v<T, date>) {
        return "date";
    } else if constexpr (std::is_same_v<T, time>) {
        return "time";
    } else if constexpr (std::is_same_v<T, datetime>) {
        return "datetime";
    } else if constexpr (std::is_same_v<T, duration>) {
        return "duration";
    } else if constexpr (std::is_same_v<T, civil_datetime>) {
        return "civil_datetime";
    } else if constexpr (std::is_same_v<T, zoned_datetime>) {
        return "zoned_datetime";
    } else if constexpr (std::is_same_v<T, tenor>) {
        return "tenor";
    } else if constexpr (detail::is_list<T>::value) {
        return "list<" + type_name<typename T::value_type>() + ">";
    } else if constexpr (detail::is_fixed_list<T>::value) {
        return "list<" + type_name<typename T::value_type>() + ", " + std::to_string(std::tuple_size_v<T>) +
               ">";
    } else if constexpr (detail::is_list_delta<T>::value) {
        return "delta<" + type_name<typename T::list_type>() + ">";
    } else if constexpr (detail::is_optional<T>::value) {
        return type_name<typename T::value_type>() + "?";
    } else if constexpr (detail::is_tuple<T>::value) {
        return []<std::size_t... I>(std::index_sequence<I...>) {
            std::string parts;
            ((parts += (I == 0 ? "" : ", ") + type_name<std::tuple_element_t<I, T>>()), ...);
            return "tuple<" + parts + ">";
        }(std::make_index_sequence<std::tuple_size_v<T>>{});
    } else if constexpr (named_enum<T>) {
        return std::string{hgl_enum(T{}).type};
    } else if constexpr (requires { value_form<T>::name(); }) {
        return value_form<T>::name();
    } else if constexpr (requires { T::hgl_name; }) {
        // A type the compiler writes (a struct, a node's state) names itself.
        return std::string{T::hgl_name};
    } else {
        static_assert(false, "hglrt::type_name: no HGL name for this type");
    }
}

} // namespace hglrt

namespace hglrt::detail {
/// The member `value` of an enum as the language names one (TYPE-7:
/// "`Type.member` names one"): `Side.buy`, its type's name without its
/// module's path. A value that is no member, which no program makes, is
/// written as a call of the enum's name with its number would make it.
template <named_enum E>
std::string member_text(E value) {
    const auto names = hgl_enum(value);
    const std::string_view type = names.type.substr(names.type.rfind(':') + 1);
    for (const auto& [member, name] : names.members) {
        if (member == value) {
            return std::string{type} + "." + std::string{name};
        }
    }
    return std::string{type} + "(" + std::to_string(static_cast<std::underlying_type_t<E>>(value)) + ")";
}
} // namespace hglrt::detail

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

/// An enum is registered with hgraph as an atomic, under `hgl.` and its HGL
/// name, the first time hgraph asks for its schema: its members' order is
/// the enumeration's, and its hash and equality the enumeration's own.
template <hglrt::named_enum E>
struct hgraph::scalar_descriptor<E> {
    [[nodiscard]] static constexpr bool is_concrete() noexcept { return true; }
    [[nodiscard]] static const ValueTypeMetaData* value_meta() {
        static const std::string name = "hgl." + hglrt::type_name<E>();
        return TypeRegistry::instance().register_scalar<E>(name);
    }
};
