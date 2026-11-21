// HGL's composite values beside the list: sets and dictionaries, what a
// struct the compiler writes tells this library, a value of an abstract
// struct, the identity of a key, and the deltas of a set, a dictionary and a
// list as values (TYPE-3 to TYPE-6, TYPE-14, TYPE-16, EXPR-3, ERR-6, CTRL-12;
// core's OPS-2).
//
// An ordinary set and an ordinary dictionary promise no order of iteration
// (CTRL-12), so each is a hash container: `set<K>` is an
// `std::unordered_set`, `dict<K, V>` an `std::unordered_map`, each with this
// library's hash and identity of a key. Two sets are equal where they have the
// same members, and two dictionaries where they have the same keys and each
// value is equal as `==` of its type has it (OPS-2), whatever the order.
//
// A key is told apart from another by `==` of its type (OPS-2), as a set's
// members and a dictionary's keys are: the two zeros are one key, as `==`
// has them, and a key that holds a NaN is equal to no key, itself included
// (NUM-5), so it is found by no read. `key_hash` hashes a key alike for keys
// that are `==`. The pinned text says no more of a key that holds a NaN.
//
// A struct the compiler writes is a plain aggregate of its fields, a parent's
// first, with HGL's equality, `==` of each field (OPS-2). It tells this
// library its fields through `fields<S>::members`, a tuple of pointers to its
// members in order, from which its hash and its identity as a key are made.
// A value of an abstract struct holds any struct that inherits from it
// (TYPE-3); the compiler writes its class over `held_value` and `held_as`.
#pragma once

#include <hglrt/checked.h>
#include <hglrt/failure.h>
#include <hglrt/values.h>

#include <cstddef>
#include <cstdint>
#include <expected>
#include <functional>
#include <map>
#include <memory>
#include <optional>
#include <set>
#include <source_location>
#include <string>
#include <string_view>
#include <tuple>
#include <type_traits>
#include <typeinfo>
#include <unordered_map>
#include <unordered_set>
#include <utility>

/// Marks a function that gives a reference into what its first argument
/// holds, and never into a key it is given: GCC would otherwise take a key
/// made for the call for what the reference may name (`-Wdangling-reference`).
#if defined(__GNUC__) && !defined(__clang__)
#define HGLRT_NO_DANGLING [[gnu::no_dangling]]
#else
#define HGLRT_NO_DANGLING
#endif

namespace hglrt {

namespace codes {
/// ERR-6: "Strict ordinary dictionary lookup has no member".
inline constexpr std::string_view missing_key = "value.missing_key";
} // namespace codes

/// What a struct `S` the compiler writes tells the library: `members`, a
/// `std::tuple` of a pointer to each of its members, in the order the struct
/// has them (a parent's fields first). The compiler specializes it for each
/// struct and each struct's delta.
template <class S>
struct fields;

/// A struct the compiler writes, or another type that names its members as one
/// does (`fields`).
template <class S>
concept written_struct = requires { fields<S>::members; };

/// `T?` (TYPE-14): `std::optional<T>`, and `T` itself where it is a nullable
/// already, as the suffix makes no nullable of a nullable.
template <class T>
using nullable_t = std::conditional_t<detail::is_optional<T>::value, T, std::optional<T>>;

/// A value of an abstract struct (TYPE-3): a class the compiler writes, which
/// holds the struct it is through `held_`, a pointer to a `held_value`.
template <class A>
concept abstract_value = requires(const A& value) { value.held_->hash_(); };

template <class T>
std::size_t key_hash_of(const T& value);

/// The hash of a key, alike for keys that are `==`.
struct key_hash {
    template <class T>
    std::size_t operator()(const T& value) const {
        return key_hash_of(value);
    }
};

/// Whether two keys are one: `==` of their type (OPS-2, NUM-5).
struct key_equal {
    template <class T>
    bool operator()(const T& lhs, const T& rhs) const {
        return lhs == rhs;
    }
};

/// `set<K>` (TYPE-14, TYPE-16): its members, each once, in no order (CTRL-12).
/// Two sets are equal where they have the same members (OPS-2).
template <class K>
struct set : std::unordered_set<K, key_hash, key_equal> {
    using std::unordered_set<K, key_hash, key_equal>::unordered_set;

    friend bool operator==(const set& lhs, const set& rhs) {
        if (lhs.size() != rhs.size()) {
            return false;
        }
        for (const K& member : lhs) {
            if (!rhs.contains(member)) {
                return false;
            }
        }
        return true;
    }
};

/// `dict<K, V>` (TYPE-14, TYPE-16): a value for each of its keys, in no order
/// (CTRL-12). Two are equal where they have the same keys and each value is
/// equal, as `==` of `V` has it (OPS-2): one that holds a NaN is not equal to
/// itself.
template <class K, class V>
struct dict : std::unordered_map<K, V, key_hash, key_equal> {
    using std::unordered_map<K, V, key_hash, key_equal>::unordered_map;

    friend bool operator==(const dict& lhs, const dict& rhs) {
        if (lhs.size() != rhs.size()) {
            return false;
        }
        for (const auto& [key, value] : lhs) {
            const auto found = rhs.find(key);
            if (found == rhs.end() || !(value == found->second)) {
                return false;
            }
        }
        return true;
    }
};

/// The dictionary `[key: value, ...]` (EXPR-9) of `entries`, each in the order
/// written, whose keys the compiler knows are distinct.
template <class K, class V>
dict<K, V> dict_of(std::initializer_list<std::pair<K, V>> entries) {
    dict<K, V> made;
    made.reserve(entries.size());
    for (const auto& [key, value] : entries) {
        made.emplace(key, value);
    }
    return made;
}

namespace detail {
/// Fails a strict read of a dictionary that has no member of the key read,
/// with the code `value.missing_key` (ERR-6).
[[noreturn]] void missing_key(std::source_location where);
} // namespace detail

/// `book[key]` read (ERR-6, EXPR-7): "Ordinary/atomic dictionary brackets and
/// `at` are strict point-in-time reads; absence fails", with
/// `value.missing_key`.
template <class K, class V>
HGLRT_NO_DANGLING const V& entry_at(const dict<K, V>& book, const std::type_identity_t<K>& key,
                                    std::source_location where = std::source_location::current()) {
    const auto found = book.find(key);
    if (found == book.end()) {
        detail::missing_key(where);
    }
    return found->second;
}

/// `book[key]` as a place that a compound assignment reads first, or that is
/// part of a larger place (EXPR-3): the key must be there.
template <class K, class V>
HGLRT_NO_DANGLING V& entry_at(dict<K, V>& book, const std::type_identity_t<K>& key,
                              std::source_location where = std::source_location::current()) {
    const auto found = book.find(key);
    if (found == book.end()) {
        detail::missing_key(where);
    }
    return found->second;
}

/// `book[key]` as the place an assignment writes whole (EXPR-3): "A key the
/// dictionary has not is added". The key is added, or its value replaced,
/// where the value is assigned, and not before: naming the place changes
/// nothing, so a value that fails leaves the dictionary as it was. It holds
/// its dictionary by reference and its key as a value of its own, and lives
/// only in the statement that assigns it.
template <class K, class V>
class entry_place {
  public:
    entry_place(dict<K, V>& book, K key) : book_(book), key_(std::move(key)) {}

    /// Gives the key `value`, adding the key where the dictionary has not.
    entry_place& operator=(V value) {
        book_.insert_or_assign(key_, std::move(value));
        return *this;
    }

  private:
    dict<K, V>& book_;
    K key_;
};

/// The place `book[key]` an assignment writes whole (`entry_place`).
template <class K, class V>
entry_place<K, V> entry(dict<K, V>& book, const std::type_identity_t<K>& key) {
    return {book, key};
}

/// `get<K, T>(store: dict<K, T>, key: K) -> T?` (core's capabilities.hgl,
/// ERR-6): the value the dictionary has for the key, or null where it has
/// none. A `T` that is a nullable already gives itself: `T?` of an `i64?`
/// is an `i64?` (TYPE-14), and a null held is null.
template <class K, class T>
nullable_t<T> get_dict_K__T__K(const dict<K, T>& store, const std::type_identity_t<K>& key) {
    const auto found = store.find(key);
    if (found == store.end()) {
        return std::nullopt;
    }
    return found->second;
}

/// `delta<set<K>>` (TYPE-16): "with the properties `added` and `removed`, each
/// a `set<K>`".
template <class K>
struct set_delta {
    /// The set's delta's set.
    using set_type = set<K>;
    set<K> added{};
    set<K> removed{};

    friend bool operator==(const set_delta&, const set_delta&) = default;
};

/// `delta<dict<K, V>>` (TYPE-16): "with the properties `updated`, a
/// `dict<K, delta<V>>` of each key added or changed with its delta, and
/// `removed`, a `set<K>`".
template <class K, class V>
struct dict_delta {
    /// The dictionary it is the delta of.
    using dict_type = dict<K, V>;
    dict<K, delta_t<V>> updated{};
    set<K> removed{};

    friend bool operator==(const dict_delta&, const dict_delta&) = default;
};

template <class K>
struct delta_of<set<K>> {
    using type = set_delta<K>;
};

template <class K, class V>
struct delta_of<dict<K, V>> {
    using type = dict_delta<K, V>;
};

template <class K>
struct fields<set_delta<K>> {
    static constexpr auto members = std::tuple{&set_delta<K>::added, &set_delta<K>::removed};
};

template <class K, class V>
struct fields<dict_delta<K, V>> {
    static constexpr auto members = std::tuple{&dict_delta<K, V>::updated, &dict_delta<K, V>::removed};
};

/// `change.updated` of a list's delta (TYPE-16): "a `dict<i64, delta<V>>` of
/// each changed position with its delta".
template <class L>
dict<i64, delta_t<typename L::value_type>> updated_of(const list_delta<L>& change) {
    return {change.updated.begin(), change.updated.end()};
}

/// `change.removed` of a list's delta (TYPE-16): "a `set<i64>` of the
/// positions that lost their value".
template <class L>
set<i64> removed_of(const list_delta<L>& change) {
    return {change.removed.begin(), change.removed.end()};
}

/// `delta<list<V>>(updated: ..., removed: ..., size_modified: ...)`, the
/// list's type `L` (TYPE-16): a property left out is empty, or null for
/// `size_modified`.
template <class L>
list_delta<L> list_delta_of(const dict<i64, delta_t<typename L::value_type>>& updated,
                            const set<i64>& removed, std::optional<i64> size_modified) {
    list_delta<L> made;
    made.updated.insert(updated.begin(), updated.end());
    made.removed.insert(removed.begin(), removed.end());
    made.size_modified = size_modified;
    return made;
}

/// The part of a value of an abstract struct (TYPE-3) that every one has,
/// whatever struct it holds: HGL's `==` of two, and a hash alike where that
/// holds. Its names, and those of `held_as`, end with
/// `_`, which no name of a field the compiler writes does: the class of the
/// abstract struct gives each field a function of the field's name.
struct held_value {
    held_value() = default;
    held_value(const held_value&) = delete;
    held_value& operator=(const held_value&) = delete;
    virtual ~held_value() = default;

    /// `==` (OPS-2): "Two structs are equal when they are of one struct with
    /// the same generic arguments ... and each field is equal".
    [[nodiscard]] virtual bool equal_(const held_value& other) const = 0;
    /// The hash of the struct held, alike where `equal_` holds.
    [[nodiscard]] virtual std::size_t hash_() const = 0;
};

/// The struct `S` held where a value of the abstract struct whose
/// `Interface` this is, a `held_value`, is expected. The compiler's class
/// of the abstract struct derives from it to read each field it has.
template <class Interface, class S>
struct held_as : Interface {
    explicit held_as(S held) : held_(std::move(held)) {}

    /// The struct held.
    S held_;

    [[nodiscard]] bool equal_(const held_value& other) const final {
        const auto* of = dynamic_cast<const held_as*>(&other);
        return of != nullptr && held_ == of->held_;
    }
    [[nodiscard]] std::size_t hash_() const final {
        return detail::combined(typeid(S).hash_code(), key_hash_of(held_));
    }
};

/// The structs `S` inherits from, at any depth (TYPE-3): `type` is a
/// `std::tuple` of them, each with the arguments `S` gives it. The compiler
/// specializes it for each struct with a parent.
template <class S>
struct parents {
    using type = std::tuple<>;
};

namespace detail {
template <class A, class Tuple>
struct listed : std::false_type {};
template <class A, class... P>
struct listed<A, std::tuple<P...>> : std::bool_constant<(std::is_same_v<A, P> || ...)> {};
} // namespace detail

/// True where the struct `S` is a value where the abstract struct `A` is
/// expected: it inherits from it (TYPE-3, "through any number of parents").
template <class S, class A>
concept inherits = detail::listed<A, typename parents<std::remove_cvref_t<S>>::type>::value;

/// `convert<list<B>>(values)` (core's OPS-6): the list of the abstract struct
/// `B` that holds the same elements.
template <class B, class A>
list<B> parent_list(const std::vector<A>& values) {
    list<B> made;
    made.reserve(values.size());
    for (const A& value : values) {
        made.push_back(B(value));
    }
    return made;
}

/// `Kind(value)` (TYPE-7): the member of the enum `E` whose number, or whose
/// name, is `value`, from `members`, each a member with its number or name. A
/// value that is none fails where it is evaluated; the catalogue has no code
/// for that failure.
template <class E, class Value>
E enum_member(const Value& value, std::initializer_list<std::pair<Value, E>> members,
              std::source_location where = std::source_location::current()) {
    for (const auto& [named, member] : members) {
        if (named == value) {
            return member;
        }
    }
    if constexpr (std::is_same_v<Value, i64>) {
        fail_uncoded("no member of the enum is numbered " + std::to_string(value), where);
    } else {
        fail_uncoded("no member of the enum is named \"" + std::string{value} + "\"", where);
    }
}

namespace detail {
template <class T>
struct is_set : std::false_type {};
template <class K>
struct is_set<set<K>> : std::true_type {};

template <class T>
struct is_dict : std::false_type {};
template <class K, class V>
struct is_dict<dict<K, V>> : std::true_type {};

template <class T>
struct is_expected : std::false_type {};
template <class T, class E>
struct is_expected<std::expected<T, E>> : std::true_type {};

/// `hash` mixed so that a sum of several, which does not hang on their
/// order, keeps their bits apart (splitmix64's finalizer).
inline std::size_t mixed(std::uint64_t hash) {
    hash = (hash ^ (hash >> 30U)) * 0xbf58476d1ce4e5b9ULL;
    hash = (hash ^ (hash >> 27U)) * 0x94d049bb133111ebULL;
    return static_cast<std::size_t>(hash ^ (hash >> 31U));
}
} // namespace detail

/// The hash of a key, alike for keys that are `==`: one for both zeros, and a
/// set's and a dictionary's whatever the order of their members.
template <class T>
std::size_t key_hash_of(const T& value) {
    if constexpr (std::is_floating_point_v<T>) {
        // `-0.0 == 0.0`: the two hash alike. A NaN is equal to nothing.
        return std::hash<T>{}(value == 0 ? T{0} : value);
    } else if constexpr (detail::is_optional<T>::value) {
        return value.has_value() ? detail::combined(1, key_hash_of(*value)) : 0;
    } else if constexpr (detail::is_tuple<T>::value) {
        std::size_t seed = std::tuple_size_v<T>;
        std::apply([&](const auto&... part) { ((seed = detail::combined(seed, key_hash_of(part))), ...); },
                   value);
        return seed;
    } else if constexpr (detail::is_list<T>::value || detail::is_fixed_list<T>::value) {
        std::size_t seed = value.size();
        // Each element as the list's value type: libc++ gives an element
        // of a `const std::vector<bool>` as a reference to its bit, which this
        // binds as a `bool`.
        for (const typename T::value_type& element : value) {
            seed = detail::combined(seed, key_hash_of(element));
        }
        return seed;
    } else if constexpr (detail::is_set<T>::value) {
        std::size_t sum = 0;
        for (const auto& member : value) {
            sum += detail::mixed(key_hash_of(member));
        }
        return detail::combined(value.size(), sum);
    } else if constexpr (detail::is_dict<T>::value) {
        std::size_t sum = 0;
        for (const auto& [key, held] : value) {
            sum += detail::mixed(detail::combined(key_hash_of(key), key_hash_of(held)));
        }
        return detail::combined(value.size(), sum);
    } else if constexpr (detail::is_list_delta<T>::value) {
        std::size_t seed = detail::combined(value.updated.size(), value.removed.size());
        for (const auto& [position, change] : value.updated) {
            seed = detail::combined(detail::combined(seed, key_hash_of(position)), key_hash_of(change));
        }
        for (const i64 position : value.removed) {
            seed = detail::combined(seed, key_hash_of(position));
        }
        return detail::combined(seed, key_hash_of(value.size_modified));
    } else if constexpr (written_struct<T>) {
        std::size_t seed = std::tuple_size_v<std::remove_cvref_t<decltype(fields<T>::members)>>;
        std::apply(
            [&](const auto... member) { ((seed = detail::combined(seed, key_hash_of(value.*member))), ...); },
            fields<T>::members);
        return seed;
    } else if constexpr (abstract_value<T>) {
        return value.held_->hash_();
    } else if constexpr (detail::is_expected<T>::value) {
        if (value.has_value()) {
            if constexpr (std::is_void_v<typename T::value_type>) {
                return 1;
            } else {
                return detail::combined(1, key_hash_of(*value));
            }
        }
        if constexpr (std::is_same_v<typename T::error_type, error>) {
            // ERR-4: an `error`'s equality excludes its context; equal
            // errors have one message.
            return detail::combined(2, std::hash<std::string>{}(value.error().message));
        } else {
            return detail::combined(2, key_hash_of(value.error()));
        }
    } else if constexpr (std::is_same_v<T, error>) {
        return std::hash<std::string>{}(value.message);
    } else {
        return std::hash<T>{}(value);
    }
}

} // namespace hglrt

template <class K>
struct std::hash<hglrt::set<K>> {
    std::size_t operator()(const hglrt::set<K>& value) const { return hglrt::key_hash_of(value); }
};

template <class K, class V>
struct std::hash<hglrt::dict<K, V>> {
    std::size_t operator()(const hglrt::dict<K, V>& value) const { return hglrt::key_hash_of(value); }
};

template <class K>
struct std::hash<hglrt::set_delta<K>> {
    std::size_t operator()(const hglrt::set_delta<K>& value) const { return hglrt::key_hash_of(value); }
};

template <class K, class V>
struct std::hash<hglrt::dict_delta<K, V>> {
    std::size_t operator()(const hglrt::dict_delta<K, V>& value) const { return hglrt::key_hash_of(value); }
};
