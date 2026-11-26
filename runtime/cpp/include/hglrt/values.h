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
// an ordinary value too: `delta_t<T>`. It is T itself for a scalar, a
// tuple of optionals for a tuple, `nullable_delta` for a nullable and
// `list_delta` for a list. A set's, a dictionary's and a struct's are the collections' and the
// structs' work (docs/cpp-target.md, "Not decided here").
#pragma once

#include <hgraph/types/metadata/type_registry.h>
#include <hgraph/types/primitive_types.h>
#include <hgraph/types/static_schema.h>
#include <hgraph/types/temporal.h>
#include <hgraph/util/date_time.h>

#include <algorithm>
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
#include <typeinfo>
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

/// `Type` (TYPE-18): its compiled identity and diagnostic spelling.
/// Equality and hashing use identity; names in separate modules may
/// have the same spelling without naming the same type.
struct type_value {
    str identity;
    str spelling;

    friend bool operator==(const type_value& lhs, const type_value& rhs) {
        return lhs.identity == rhs.identity;
    }
};

/// Structural non-type template argument carrying a compiled `Type` identity
/// for generic structs parameterized by `const Kind: Type` (TYPE-18, GEN-13).
template <std::size_t N>
struct type_tag {
    char identity[N]{};

    constexpr type_tag(const char (&text)[N]) noexcept {
        for (std::size_t index = 0; index < N; ++index) {
            identity[index] = text[index];
        }
    }

    friend constexpr bool operator==(const type_tag&, const type_tag&) = default;
};

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
/// (`type_name`), its text (TYPE-7: "`Type.member` names one", `value_text.h`'s
/// `text_of`), and its schema, with which hgraph holds it, a list of it and a
/// recording of it.
template <class E, std::size_t N>
struct EnumNames {
    /// The enum's name, qualified with its module's path.
    std::string_view type;
    /// Each member and its name, as HGL writes it.
    std::pair<E, std::string_view> members[N];
    /// Member numbers in declaration order. The runtime's enums use
    /// ordinary numbering from zero. Generated enums can write other numbers.
    std::array<i64, N> numbers = [] {
        std::array<i64, N> ordinals{};
        for (std::size_t index = 0; index < N; ++index) {
            ordinals[index] = static_cast<i64>(index);
        }
        return ordinals;
    }();
};

/// An empty enum still identifies the type of a port or conversion
/// result (TYPE-7, OPS-7). Its metadata contains zero members.
template <class E>
struct EnumNames<E, 0> {
    std::string_view type;
    std::array<std::pair<E, std::string_view>, 0> members{};
    std::array<i64, 0> numbers{};
};

/// An enumeration the library can name: one whose `hgl_enum` gives its
/// `EnumNames`.
template <class E>
concept named_enum = std::is_enum_v<E> && requires(E value) {
    { hgl_enum(value).type } -> std::convertible_to<std::string_view>;
};

template <class L>
struct list_delta;

template <class T, class D>
struct nullable_delta;

/// `delta<T>` (TYPE-16): what a temporal value of T publishes in one cycle.
/// "For most T it is another type the language has: `delta<f64>` is `f64`,
/// and `delta<tuple<f64, i64>>` is `tuple<f64?, i64?>`"; a nullable's is a
/// type of its own, `nullable_delta`, and a list's, `list_delta`.
template <class T>
struct delta_of {
    using type = T;
};

/// `delta_of`'s type.
template <class T>
using delta_t = typename delta_of<T>::type;

template <class T>
struct delta_of<std::optional<T>> {
    using type = nullable_delta<T, delta_t<T>>;
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

namespace detail {

/// The size of a growing list after publishing `delta` (TYPE-16). An
/// explicit size takes precedence over updates, and a negative size drops
/// up to the number held. The caller checks whether the update is allowed.
template <class L>
std::size_t list_size_after(std::size_t size, const list_delta<L>& delta) {
    if (delta.size_modified.has_value()) {
        const i64 written = *delta.size_modified;
        if (written < 0) {
            const auto dropped = static_cast<std::uint64_t>(-(written + 1)) + 1;
            return dropped >= size ? 0 : size - static_cast<std::size_t>(dropped);
        }
        return static_cast<std::size_t>(written);
    }
    // A position below zero belongs to no list (EXPR-3) and moves no size.
    if (delta.updated.empty() || delta.updated.rbegin()->first < 0) {
        return size;
    }
    return std::max(size, static_cast<std::size_t>(delta.updated.rbegin()->first) + 1);
}

} // namespace detail

/// `delta<T?>` (TYPE-16): "A nullable's ... delta is a type of its own ...
/// Such a delta is no T", and "A nullable's delta is an optional: it holds
/// what was published, a null among them": `value`, the delta `D` of the
/// payload `T` that was published, empty where a null was. `D` is
/// `delta_t<T>` but for an `atomic<T>?`, whose delta is the whole T. It is
/// no `std::optional`, so that a `delta<T?>?` is an optional of one, as a
/// nullable of a nullable never is (TYPE-14), and a `delta<T?>` compared
/// with one is compared as a value. "A `delta<V>` is a value where a
/// `delta<V?>` is expected": a D converts to one that holds it.
template <class T, class D = delta_t<T>>
struct nullable_delta {
    /// The nullable's payload, and its delta.
    using payload_type = T;
    using delta_type = D;

    /// What was published: its delta, or empty for a null.
    std::optional<D> value{};

    constexpr nullable_delta() = default;
    constexpr nullable_delta(D published) : value(std::move(published)) {}
    constexpr explicit nullable_delta(std::optional<D> published) : value(std::move(published)) {}

    friend bool operator==(const nullable_delta&, const nullable_delta&) = default;
};

/// The hash of `value`, part by part: what `std::hash` gives a list (TYPE-8:
/// equal values hash alike). A tuple and an optional are hashed through
/// their parts, as the standard library hashes neither a tuple nor an
/// optional of every type; a type whose `value_form` gives a `hash`, as it
/// says.
template <class T>
std::size_t hash_of(const T& value);

template <class T>
struct value_form;

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

template <class T>
struct is_nullable_delta : std::false_type {};
template <class T, class D>
struct is_nullable_delta<nullable_delta<T, D>> : std::true_type {};

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
    } else if constexpr (detail::is_nullable_delta<T>::value) {
        return detail::combined(2, hash_of(value.value));
    } else if constexpr (detail::is_list<T>::value || detail::is_fixed_list<T>::value) {
        std::size_t seed = value.size();
        // Each element as the list's value type: libc++ gives an element
        // of a `const std::vector<bool>` as a reference to its bit, which this
        // binds as a `bool`.
        for (const typename T::value_type& element : value) {
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
    } else if constexpr (requires { value_form<T>::hash(value); }) {
        return value_form<T>::hash(value);
    } else {
        return std::hash<T>{}(value);
    }
}

/// What a header of this library says of a value type it declares that the
/// chains of `type_name` and of `text_of` (value_text.h) do not know, a
/// set's or a dictionary's (keyed.h) and a struct's (structs.h) among them: a
/// specialization gives `name()`, the HGL name of the type, `text(value)`,
/// a value of it as the language writes it, and `hash(value)`, its hash
/// where the standard library has none (`hash_of`), or any of them. The
/// primary says nothing.
template <class T>
struct value_form {};

/// What the compiler writes beside a struct it writes and its delta, which
/// the library names and writes through `value_form` (structs.h): `name`, the
/// type as HGL writes it (`demo.trade::Quote`, `delta<demo.trade::Quote>`),
/// `fields`, the name of each field in order, and `delta`, whether it is a
/// delta.
template <class T>
struct type_named;

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
    } else if constexpr (detail::is_nullable_delta<T>::value) {
        // A payload whose delta is the whole of it, where its own is not,
        // is an atomic's.
        using P = typename T::payload_type;
        const std::string payload = type_name<P>();
        return "delta<" +
               (std::is_same_v<typename T::delta_type, delta_t<P>> ? payload : "atomic<" + payload + ">") +
               "?>";
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

/// The configuration of a node, a `const` parameter's value (FN-1: "a
/// `const` parameter takes what is fixed when the graph is built. In
/// composition that is any ordinary value"), where hgraph has no scalar of
/// the value's own type to hold it as: a tuple, a fixed list, a set, a
/// dictionary, a struct, a list or a nullable of a struct hgraph does not
/// hold as a bundle, a nullable of a tuple. hgraph holds a node's
/// configuration as `hgraph::Scalar<"name", T>`, and asks of T a scalar of
/// its type registry, with `std::hash` and `==`, for its interning of
/// identical nodes; this holds the value whole, and is registered as an
/// atomic under `hgl.config.` and the name of the value's C++ type (below),
/// with the value's hash (`hash_of`) and equality. Two nodes whose
/// configurations differ are two. The compiler wires opaque configurations
/// that can hold an `f64` separately because hgraph cannot inspect their
/// zero signs (NUM-2). A hook reads the value as `held`.
template <class T>
struct configured {
    T held;

    friend bool operator==(const configured&, const configured&) = default;
};

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

/// Registers the scalar described by `T`. Its descriptor supplies a name
/// once, normally the HGL type name under `hgl.`.
template <class T>
const hgraph::ValueTypeMetaData* named_scalar_meta(
    std::string (*name_of)() = [] { return "hgl." + type_name<T>(); }) {
    static const std::string name = name_of();
    return hgraph::TypeRegistry::instance().register_scalar<T>(name);
}

} // namespace hglrt::detail

template <>
struct std::hash<hglrt::type_value> {
    std::size_t operator()(const hglrt::type_value& value) const {
        return std::hash<hglrt::str>{}(value.identity);
    }
};

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
        return hglrt::detail::named_scalar_meta<hglrt::list<T>>();
    }
};

/// An enum is registered with hgraph as an atomic, under `hgl.` and its HGL
/// name, the first time hgraph asks for its schema: its members' order is
/// the enumeration's, and its hash and equality the enumeration's own.
template <hglrt::named_enum E>
struct hgraph::scalar_descriptor<E> {
    [[nodiscard]] static constexpr bool is_concrete() noexcept { return true; }
    [[nodiscard]] static const ValueTypeMetaData* value_meta() {
        return hglrt::detail::named_scalar_meta<E>();
    }
};

template <class T>
struct std::hash<hglrt::configured<T>> {
    std::size_t operator()(const hglrt::configured<T>& value) const { return hglrt::hash_of(value.held); }
};

/// A configuration that hgraph has no scalar of its value's own type for is
/// registered with hgraph as an atomic, the first time hgraph asks for its
/// schema, under `hgl.config.` and the C++ type's own name, which tells
/// every type apart: not every value type the compiler writes has an HGL
/// name the library can give (a struct hgraph does not hold as a bundle).
template <class T>
struct hgraph::scalar_descriptor<hglrt::configured<T>> {
    [[nodiscard]] static constexpr bool is_concrete() noexcept { return true; }
    [[nodiscard]] static const ValueTypeMetaData* value_meta() {
        return hglrt::detail::named_scalar_meta<hglrt::configured<T>>(
            [] { return std::string{"hgl.config."} + typeid(T).name(); });
    }
};
