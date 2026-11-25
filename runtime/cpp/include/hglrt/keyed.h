// The temporal form of a set and of a dictionary (TYPE-9: "dictionaries to
// keyed children, and sets to membership"): hgraph's `TSS<K>` and
// `TSD<K, V>`, each entry of a dictionary a child of its value's own form.
//
// What the generated code calls of them is here: the read of one whole
// (TIME-1), what it published in the cycle as HGL's `delta<T>` (END-3,
// TYPE-16), `all_valid` of a dictionary (TIME-9), a dictionary's entry read
// and given to an endpoint operation in a hook (TIME-1, EXPR-7), a
// publication whole or as a delta (TIME-7), and the invalidation of one
// (END-4). A publication is
// hgraph's own: a value given whole is hgraph's whole write of it
// (`apply_current_value`), and a delta is hgraph's write of that change
// (`apply_delta`), each converted to hgraph's value first. Under TIME-7,
// a set given the members it holds,
// and an empty dictionary given to a valid empty one, is no tick; each entry
// of a dictionary given whole is published, the same value again included; a
// delta that names no change, or whose every removal names what is not held,
// ticks only where it makes the value valid; and two deltas written in one
// evaluation are one change. Two keys a delta names are one where hgraph's
// set or dictionary takes them as one (TYPE-16).
//
// A set's element and a dictionary's key are a type hgraph holds as a value
// of its own (a scalar, an enum), and a dictionary's value is one whose form
// this header converts: a scalar's (`TS<T>`), a set's or a dictionary's. The
// order of a temporal set's or dictionary's members is hgraph's; the
// `hglrt::set` and `hglrt::dict` they are read into promise none (CTRL-12).
//
// A trace's cell written whole of a set or a dictionary is published as its
// change from what the trace's earlier cells left (TST-9), as hgraph would
// publish it: the members added and removed, and for a dictionary each entry
// it gives and the keys it leaves out (`testing::detail::Publisher`).
#pragma once

#include <hglrt/checked.h>
#include <hglrt/collections.h>
#include <hglrt/endpoints.h>
#include <hglrt/structs.h>
#include <hglrt/traces.h>
#include <hglrt/values.h>

#include <hgraph/types/generated.h>
#include <hgraph/types/time_series/ts_delta.h>
#include <hgraph/types/value/value_builder.h>

#include <algorithm>
#include <optional>
#include <source_location>
#include <string>
#include <tuple>
#include <type_traits>
#include <utility>
#include <vector>

namespace hglrt {

/// What an HGL value of the temporal form `Schema` is, and how it goes to
/// and from hgraph: `type`, the C++ type of the value; `read(endpoint, where)`, the
/// value a valid endpoint of the form holds, whole; `whole(value)`, hgraph's
/// value of it, which hgraph publishes whole; and `change(delta)`, hgraph's
/// canonical delta of a `delta_t<type>`, which hgraph publishes as that
/// change. Written for a scalar's form, a set's and a dictionary's; a
/// dictionary reads and writes each entry through its value's.
template <class Schema>
struct keyed_form;

namespace keyed_detail {
/// The failure or result error of a tuple entry that cannot be read whole.
inline constexpr char incomplete_tuple_read[] =
    "an entry of a dictionary that is not all valid is read whole";

/// hgraph's binding of the value schema `meta`, through which hgraph builds
/// a value of it.
inline hgraph::ValueTypeRef binding_of(const hgraph::ValueTypeMetaData* meta) {
    return hgraph::ValuePlanFactory::instance().type_for(meta);
}

/// The time-series schema hgraph has for `Schema`.
template <class Schema>
const hgraph::TSValueTypeMetaData* ts_meta() {
    return hgraph::schema_descriptor<Schema>::ts_meta();
}

/// `element`, a member or a key as an hgraph view gives it, as a `K`: an
/// input's typed view gives a `K` (or vector<bool>'s proxy for Boolean
/// membership), and an output's an erased value (TYPE-9, TIME-1).
template <class K, class Element>
K key_of(const Element& element) {
    if constexpr (std::is_same_v<std::remove_cvref_t<Element>, K> ||
                  (std::is_same_v<K, bool> &&
                   std::is_same_v<std::remove_cvref_t<Element>, std::vector<bool>::reference>)) {
        return element;
    } else {
        return element.template checked_as<K>();
    }
}

/// hgraph's set of `members`, of the element type `K`.
template <class K, class Members>
hgraph::Value set_value(const Members& members) {
    hgraph::SetBuilder built{binding_of(hgraph::scalar_descriptor<K>::value_meta())};
    for (const K& member : members) {
        static_cast<void>(built.insert(member));
    }
    return built.build();
}
} // namespace keyed_detail

/// A scalar's form, `TS<T>`, as a dictionary's entry: a value published
/// whole, whose delta is itself (TYPE-16).
template <class T>
struct keyed_form<hgraph::TS<T>> {
    using type = T;

    template <class E>
    static T read(const E& endpoint, std::source_location) {
        return endpoint_read<hgraph::TS<T>>::delta(endpoint);
    }

    static hgraph::Value whole(const T& value) { return hgraph::Value{value}; }
    static hgraph::Value change(const T& value) { return hgraph::Value{value}; }
};

/// A set's form, `TSS<K>` (TYPE-9: "sets to membership").
template <class K>
struct keyed_form<hgraph::TSS<K>> {
    using type = set<K>;

    template <class E>
    static set<K> read(const E& endpoint, std::source_location) {
        set<K> held;
        for (const auto& member : endpoint.values()) {
            held.insert(keyed_detail::key_of<K>(member));
        }
        return held;
    }

    static hgraph::Value whole(const set<K>& value) { return keyed_detail::set_value<K>(value); }

    /// hgraph's `Bundle{added, removed}` of `delta`.
    static hgraph::Value change(const set_delta<K>& delta) {
        hgraph::BundleBuilder built{
            keyed_detail::binding_of(keyed_detail::ts_meta<hgraph::TSS<K>>()->delta_value_schema)};
        built.set("added", keyed_detail::set_value<K>(delta.added));
        built.set("removed", keyed_detail::set_value<K>(delta.removed));
        return built.build();
    }
};

/// A dictionary's form, `TSD<K, V>` (TYPE-9: "dictionaries to keyed
/// children"), each entry a child of the value's form `V`.
template <class K, class V>
struct keyed_form<hgraph::TSD<K, V>> {
    using value_type = typename keyed_form<V>::type;
    using type = dict<K, value_type>;

    /// The entries that are valid, each read whole: "A dictionary ... holds
    /// what it holds" (TIME-1).
    template <class E>
    static std::optional<type> held(const E& endpoint) {
        type held;
        for (const auto& [key, child] : detail::viewed(endpoint).valid_items()) {
            if (!fn_valid(child)) {
                continue;
            }
            auto entry = hglrt::whole<V>::read(child);
            if (!entry.has_value()) {
                return std::nullopt;
            }
            held.insert_or_assign(keyed_detail::key_of<K>(key), std::move(*entry));
        }
        return held;
    }

    template <class E>
    static type read(const E& endpoint, std::source_location where) {
        auto entries = held(endpoint);
        if (!entries.has_value()) {
            unset_read(keyed_detail::incomplete_tuple_read, where);
        }
        return std::move(*entries);
    }

    /// hgraph's map of `value`, each entry hgraph's value of its own form.
    static hgraph::Value whole(const type& value) {
        const auto* schema = keyed_detail::ts_meta<V>();
        hgraph::MapBuilder built{keyed_detail::binding_of(hgraph::scalar_descriptor<K>::value_meta()),
                                 keyed_detail::binding_of(schema->value_schema)};
        for (const auto& [key, entry] : value) {
            const hgraph::Value written_key{key};
            const hgraph::Value written = keyed_form<V>::whole(entry);
            built.set_item(written_key.view(), written.view());
        }
        return built.build();
    }

    /// hgraph's `Bundle{removed, modified}` of `delta`, each entry updated
    /// as its own form's delta.
    static hgraph::Value change(const dict_delta<K, value_type>& delta) {
        const auto* schema = keyed_detail::ts_meta<V>();
        hgraph::MapBuilder modified{keyed_detail::binding_of(hgraph::scalar_descriptor<K>::value_meta()),
                                    keyed_detail::binding_of(schema->delta_value_schema)};
        for (const auto& [key, entry] : delta.updated) {
            const hgraph::Value written_key{key};
            const hgraph::Value written = keyed_form<V>::change(entry);
            modified.set_item(written_key.view(), written.view());
        }
        hgraph::BundleBuilder built{
            keyed_detail::binding_of(keyed_detail::ts_meta<hgraph::TSD<K, V>>()->delta_value_schema)};
        built.set("removed", keyed_detail::set_value<K>(delta.removed));
        built.set("modified", modified.build());
        return built.build();
    }
};

/// A set read whole, as the part of a collection or through a bound child.
template <class K>
struct schema_value<hgraph::TSS<K>> {
    using type = set<K>;
};
template <class K>
struct whole<hgraph::TSS<K>> {
    template <class E>
    static std::optional<set<K>> read(const E& endpoint) {
        if (!fn_valid(endpoint)) {
            return std::nullopt;
        }
        return keyed_form<hgraph::TSS<K>>::read(detail::viewed(endpoint),
                                                  std::source_location::current());
    }
};

/// A dictionary read whole. A wholly unset entry is omitted, and a valid
/// entry that cannot be read whole fails the read (TIME-1, ERR-14).
template <class K, class V>
struct schema_value<hgraph::TSD<K, V>> {
    using type = typename keyed_form<hgraph::TSD<K, V>>::type;
};
template <class K, class V>
struct whole<hgraph::TSD<K, V>> {
    template <class E>
    static std::optional<schema_value_t<hgraph::TSD<K, V>>> read(const E& endpoint) {
        if (!fn_valid(endpoint)) {
            return std::nullopt;
        }
        return keyed_form<hgraph::TSD<K, V>>::held(endpoint);
    }
};

/// A tuple's form, `UnNamedTSB` (structs.h), as a dictionary's entry: a
/// value of a position for each position. An entry is read whole as a tuple
/// is (TIME-1: "So does one of a child that is a struct, a tuple or a fixed
/// list ... requires `all_valid`"), checked when it runs: one that is not all
/// valid fails the read (`value.unset_read`). It is written as hgraph's
/// bundle of its positions, each as its own form writes it, and its delta as
/// hgraph's bundle of the positions it holds (TYPE-16: "null leaves the
/// position ... as it was").
template <class... F>
struct keyed_form<hgraph::UnNamedTSB<F...>> {
    using schema = hgraph::UnNamedTSB<F...>;
    using type = std::tuple<typename keyed_form<typename F::schema>::type...>;

    template <class E>
    static type read(const E& endpoint, std::source_location where) {
        std::optional<schema_value_t<schema>> held = ::hglrt::whole<schema>::read(endpoint);
        if (!held.has_value()) {
            unset_read(keyed_detail::incomplete_tuple_read, where);
        }
        return std::move(*held);
    }

    static hgraph::Value whole(const type& value) {
        hgraph::BundleBuilder built{keyed_detail::binding_of(keyed_detail::ts_meta<schema>()->value_schema)};
        [&]<std::size_t... I>(std::index_sequence<I...>) {
            (
                [&] {
                    const hgraph::Value written =
                        keyed_form<typename std::tuple_element_t<I, std::tuple<F...>>::schema>::whole(
                            std::get<I>(value));
                    built.set(I, written.view());
                }(),
                ...);
        }(std::index_sequence_for<F...>{});
        return built.build();
    }

    static hgraph::Value change(const delta_t<type>& delta) {
        hgraph::BundleBuilder built{
            keyed_detail::binding_of(keyed_detail::ts_meta<schema>()->delta_value_schema)};
        [&]<std::size_t... I>(std::index_sequence<I...>) {
            (
                [&] {
                    if (const auto& part = std::get<I>(delta); part.has_value()) {
                        const hgraph::Value written =
                            keyed_form<typename std::tuple_element_t<I, std::tuple<F...>>::schema>::change(*part);
                        built.set(I, written.view());
                    }
                }(),
                ...);
        }(std::index_sequence_for<F...>{});
        return built.build();
    }
};

/// A set's endpoint (END-3, TYPE-16): its delta is "`added` and `removed`,
/// each a `set<K>`", what hgraph's set added and removed in the cycle.
template <class K>
struct endpoint_read<hgraph::TSS<K>> {
    using delta_type = set_delta<K>;

    template <class E>
    static set_delta<K> delta(const E& endpoint) {
        set_delta<K> change;
        for (const auto& member : detail::viewed(endpoint).added()) {
            change.added.insert(keyed_detail::key_of<K>(member));
        }
        for (const auto& member : detail::viewed(endpoint).removed()) {
            change.removed.insert(keyed_detail::key_of<K>(member));
        }
        return change;
    }
};

/// A dictionary's endpoint (END-3, END-7, TYPE-16): its delta is "`updated`,
/// a `dict<K, delta<V>>` of each key added or changed with its delta, and
/// `removed`, a `set<K>`".
template <class K, class V>
struct endpoint_read<hgraph::TSD<K, V>> {
    using value_type = typename keyed_form<V>::type;
    using delta_type = dict_delta<K, value_type>;

    template <class E>
    static delta_type delta(const E& endpoint) {
        delta_type change;
        for (const auto& [key, child] : detail::viewed(endpoint).modified_items()) {
            change.updated.insert_or_assign(keyed_detail::key_of<K>(key), endpoint_read<V>::delta(child));
        }
        for (const auto& [key, child] : detail::viewed(endpoint).removed_items()) {
            change.removed.insert(keyed_detail::key_of<K>(key));
        }
        return change;
    }

    /// TIME-9: "`all_valid` is true where every one is, by its own `valid`,
    /// one level deep, an optional child aside ... It is true of a valid
    /// empty collection. `all_valid` of a collection that is not valid is
    /// false."
    template <class E>
    static bool all_valid(const E& endpoint) {
        if (!endpoint.valid()) {
            return false;
        }
        for (const auto& child : detail::viewed(endpoint).values()) {
            if (!fn_valid(child)) {
                return false;
            }
        }
        return true;
    }
};

/// The value of a set's or a dictionary's endpoint, read whole in a hook
/// (TIME-1): "A dictionary and an unsized list hold what they hold: a read of
/// one whole requires its `valid` and no more", and so does a set's. One that
/// is not valid fails the read with `value.unset_read` (ERR-14).
template <class E>
auto keyed_value(const E& endpoint, std::source_location where = std::source_location::current()) ->
    typename keyed_form<schema_of<E>>::type {
    if (!endpoint.valid()) {
        unset_read("a set or a dictionary that is not valid is read whole", where);
    }
    return keyed_form<schema_of<E>>::read(endpoint, where);
}

/// `return value`, `out = value` of a set's or a dictionary's result
/// (TIME-7): a value given whole, "published as its change from what the
/// result holds", through hgraph's whole write.
template <class E>
void keyed_publish(const E& out, const typename keyed_form<schema_of<E>>::type& value) {
    hgraph::apply_current_value(out.base(), keyed_form<schema_of<E>>::whole(value).view());
}

/// `return delta`, `out = delta` (TIME-7): a `delta<T>`, "published as that
/// change", through hgraph's write of a delta.
template <class E>
void keyed_publish(const E& out, const endpoint_delta_t<E>& delta) {
    hgraph::apply_delta(out.base(), keyed_form<schema_of<E>>::change(delta).view());
}

namespace keyed_detail {
/// Reuse keyed publication wherever a generic publisher holds this form.
template <class Schema>
struct KeyedPublisher {
    template <class O>
    static void whole(const O& out, const schema_value_t<Schema>& value, std::source_location) {
        keyed_publish(out, value);
    }

    template <class O>
    static void change(const O& out, const delta_t<schema_value_t<Schema>>& value, std::source_location) {
        keyed_publish(out, value);
    }
};
} // namespace keyed_detail

/// FN-3: a constant set publishes whole through its membership port.
template <class K>
struct publisher<hgraph::TSS<K>> : keyed_detail::KeyedPublisher<hgraph::TSS<K>> {};

/// FN-3: a constant dictionary publishes whole through its keyed port.
template <class K, class V>
struct publisher<hgraph::TSD<K, V>> : keyed_detail::KeyedPublisher<hgraph::TSD<K, V>> {};

namespace keyed_detail {
/// Observe the live children of a keyed map through a dictionary of publications.
template <class K, class V>
struct PublishedMapEntries {
    using Schema = hgraph::TSD<K, V>;
    static constexpr auto name = "hgl.map_published_entries";

    static void eval(hgraph::In<"children", Schema, hgraph::InputValidity::Unchecked> children,
                     hgraph::Out<Schema> out) {
        const auto& endpoint = out.base();
        auto result = endpoint.as_dict();
        auto mutation = result.begin_mutation(endpoint.evaluation_time());
        const auto remove = [&](const hgraph::ValueView& key) {
            // TIME-7: removing a key the result never held cannot wake a reader.
            if (result.contains(key)) {
                static_cast<void>(mutation.erase(key));
            }
        };
        for (const auto& key : children.removed_keys()) {
            remove(key);
        }
        for (const auto& [key, child] : children.modified_items()) {
            const auto& source = child.base();
            if (!source.valid()) {
                // MAP-10: an invalidated child keeps its lifecycle key but
                // withdraws its entry from the result.
                remove(key);
                continue;
            }
            if (source.schema()->kind == hgraph::TSTypeKind::TS) {
                mutation.set(key, source.value());
                continue;
            }
            const bool published = result.contains(key);
            hgraph::TSOutputView entry{endpoint.output(), mutation.at(key), endpoint.evaluation_time()};
            if (published) {
                hgraph::forward_delta(entry, source);
            } else {
                hgraph::reconcile_current_state(entry, source, {
                    .scope = hgraph::TSCurrentReconcileScope::Sample,
                    .sample_all = true,
                    .membership = true,
                });
            }
        }
        if (children.valid() && !out.valid()) {
            mutation.touch();
        }
    }
};
} // namespace keyed_detail

/// MAP-10, SET-1: expose the keys and ticks of published map entries.
template <class K, class V>
hgraph::Port<hgraph::TSD<K, V>> published_map_entries(
    hgraph::Wiring& wiring, hgraph::Port<hgraph::TSD<K, V>> children
) {
    return hgraph::wire<keyed_detail::PublishedMapEntries<K, V>>(wiring, children)
        .template as<hgraph::TSD<K, V>>();
}

// ---- A dictionary's entry (TIME-1, TIME-9, EXPR-7) -------------------------
//
// TIME-1: "A child of a collection (TIME-9), an element, a field or an
// entry, is an endpoint whether or not it is valid". hgraph has a child of a
// dictionary only for a key it holds: the entry of a key it does not hold is
// one that is not valid, not modified and has no delta. Each read of one
// below is checked when it runs, whatever the compiler proved of it.

namespace keyed_detail {
/// The key type of the dictionary endpoint `E`.
template <class E>
using key_of_t = typename std::remove_cvref_t<E>::key_type;

/// The form of an entry of the dictionary endpoint `E`.
template <class E>
using entry_schema_t = typename std::remove_cvref_t<E>::value_schema;

/// The entry of `key` of `book`, where it holds the key: hgraph's view of
/// the child, read and never made (an output's `at` would add the key).
template <class E>
auto entry_view(const E& book, const key_of_t<E>& key) -> std::optional<decltype(book.at_slot(0))> {
    if (!book.contains(key)) {
        return std::nullopt;
    }
    return book.at_slot(book.find_slot(key));
}
} // namespace keyed_detail

/// The logical entry at `key`, including where it has no current view
/// (TIME-1). A read of a bound entry looks it up when used.
template <class E, class K>
auto keyed_child(const E& book, const K& key) {
    using V = decltype(detail::viewed(book).at_slot(0));
    if (detail::present(book)) {
        auto entry = keyed_detail::entry_view(detail::viewed(book), key);
        if (entry.has_value()) {
            return Child<V>{std::move(*entry)};
        }
    }
    return Child<V>{};
}

/// `valid(book[key])` (END-1, TIME-9): whether the dictionary holds the key
/// and its entry is valid.
template <class E>
bool keyed_entry_valid(const E& book, const keyed_detail::key_of_t<E>& key) {
    const auto entry = keyed_detail::entry_view(book, key);
    return entry.has_value() && fn_valid(*entry);
}

/// `modified(book[key])` (END-2): whether the entry published in the cycle.
template <class E>
bool keyed_entry_modified(const E& book, const keyed_detail::key_of_t<E>& key) {
    const auto entry = keyed_detail::entry_view(book, key);
    return entry.has_value() && fn_modified(*entry);
}

/// `last_modified(book[key])` (END-6): null where the entry is not valid.
template <class E>
std::optional<datetime> keyed_entry_last_modified(const E& book, const keyed_detail::key_of_t<E>& key) {
    const auto entry = keyed_detail::entry_view(book, key);
    if (!entry.has_value()) {
        return std::nullopt;
    }
    return fn_last_modified(*entry);
}

/// `delta_value(book[key])` (END-3, ERR-14) where its place is no `result`:
/// the entry's delta, which fails with `value.unset_read` where it has none,
/// a key the dictionary does not hold among them.
template <class E>
auto keyed_entry_delta(const E& book, const keyed_detail::key_of_t<E>& key,
                       std::source_location where = std::source_location::current()) ->
    typename endpoint_read<keyed_detail::entry_schema_t<E>>::delta_type {
    const auto entry = keyed_detail::entry_view(book, key);
    if (!entry.has_value()) {
        unset_read("the delta of an entry the dictionary does not hold is read", where);
    }
    return fn_delta_value(*entry, where);
}

/// `delta_value(book[key])` where its place takes a `result` (ERR-14): `err`
/// coded `value.unset_read` where the entry has no delta.
template <class E>
auto keyed_entry_delta_checked(const E& book, const keyed_detail::key_of_t<E>& key)
    -> result<typename endpoint_read<keyed_detail::entry_schema_t<E>>::delta_type> {
    const auto entry = keyed_detail::entry_view(book, key);
    if (!entry.has_value()) {
        return detail::unset_result("the delta of an entry the dictionary does not hold is read");
    }
    return fn_delta_value_checked(*entry);
}

/// `book[key]` read in a hook (TIME-1, EXPR-7): the entry's value, whole as
/// its form reads it. "A child that is not valid fails it
/// (`value.unset_read`)": one the dictionary does not hold among them.
template <class E>
auto keyed_entry_value(const E& book, const keyed_detail::key_of_t<E>& key,
                       std::source_location where = std::source_location::current()) ->
    typename keyed_form<keyed_detail::entry_schema_t<E>>::type {
    const auto entry = keyed_detail::entry_view(book, key);
    if (!entry.has_value() || !fn_valid(*entry)) {
        unset_read("an entry that is not valid is read", where);
    }
    return keyed_form<keyed_detail::entry_schema_t<E>>::read(*entry, where);
}

/// `book[key]` where its place takes a `result` (ERR-14): "it gives `ok` of
/// the child's value, or, for a child that is not valid, `err` of an `error`
/// whose code is `value.unset_read`". A tuple that is valid but cannot be
/// read whole gives the same error (TIME-1).
template <class E>
auto keyed_entry_result(const E& book, const keyed_detail::key_of_t<E>& key,
                        std::source_location = std::source_location::current())
    -> result<typename keyed_form<keyed_detail::entry_schema_t<E>>::type> {
    const auto entry = keyed_detail::entry_view(book, key);
    if (!entry.has_value() || !fn_valid(*entry)) {
        return detail::unset_result("an entry that is not valid is read");
    }
    auto held = whole<keyed_detail::entry_schema_t<E>>::read(*entry);
    if (!held.has_value()) {
        return detail::unset_result(keyed_detail::incomplete_tuple_read);
    }
    return std::move(*held);
}

/// `invalidate(out)` of a set's result (END-4): clears its validity and its
/// members.
template <class K>
void fn_invalidate(const hgraph::Out<hgraph::TSS<K>>& out) {
    static_cast<void>(out.base().begin_mutation(out.evaluation_time()).invalidate());
}

/// `invalidate(out)` of a dictionary's result (END-4): clears its validity
/// and its entries.
template <class K, class V>
void fn_invalidate(const hgraph::Out<hgraph::TSD<K, V>>& out) {
    static_cast<void>(out.base().begin_mutation(out.evaluation_time()).invalidate());
}

namespace keyed_detail {
/// The members of `members` as the language writes each, in the order of
/// their text: a set promises no order, and a failure says the same thing
/// each time.
template <class Members, class Text>
std::string sorted_text(const Members& members, Text text) {
    std::vector<std::string> parts;
    for (const auto& member : members) {
        parts.push_back(text(member));
    }
    std::ranges::sort(parts);
    std::string joined;
    for (const std::string& part : parts) {
        joined += (joined.empty() ? "" : ", ") + part;
    }
    return "[" + joined + "]";
}

/// A set's members as the language writes them.
template <class K>
std::string set_text(const set<K>& members) {
    return sorted_text(members, [](const K& member) { return testing::text_of(member); });
}

/// A dictionary's entries as the language writes them.
template <class K, class V>
std::string dict_text(const dict<K, V>& entries) {
    return sorted_text(entries, [](const auto& entry) {
        return testing::text_of(entry.first) + ": " + testing::text_of(entry.second);
    });
}
} // namespace keyed_detail

/// A set's name and text (values.h, traces.h): `set<i64>`, `[1, 2]`.
template <class K>
struct value_form<set<K>> {
    static std::string name() { return "set<" + type_name<K>() + ">"; }
    static std::string text(const set<K>& value) { return keyed_detail::set_text(value); }
};

/// A dictionary's: `dict<str, i64>`, `["a": 1]`.
template <class K, class V>
struct value_form<dict<K, V>> {
    static std::string name() { return "dict<" + type_name<K>() + ", " + type_name<V>() + ">"; }
    static std::string text(const dict<K, V>& value) { return keyed_detail::dict_text(value); }
};

/// A set's delta's: `delta<set<i64>>`, by its properties.
template <class K>
struct value_form<set_delta<K>> {
    static std::string name() { return "delta<" + type_name<set<K>>() + ">"; }
    static std::string text(const set_delta<K>& value) {
        return name() + "(added: " + keyed_detail::set_text(value.added) +
               ", removed: " + keyed_detail::set_text(value.removed) + ")";
    }
};

/// A dictionary's delta's: `delta<dict<str, i64>>`, by its properties.
template <class K, class V>
struct value_form<dict_delta<K, V>> {
    static std::string name() { return "delta<" + type_name<dict<K, V>>() + ">"; }
    static std::string text(const dict_delta<K, V>& value) {
        return name() + "(updated: " + keyed_detail::dict_text(value.updated) +
               ", removed: " + keyed_detail::set_text(value.removed) + ")";
    }
};

} // namespace hglrt

namespace hglrt::testing::detail {

/// What each cell of a trace of a set publishes (TST-9): a delta as it is,
/// and a set written whole as its change from what the earlier cells left,
/// "a T whole, which replaces what the input held": the members it adds and
/// those it leaves out. A set given the members it holds is then a delta
/// that names no change, which is no tick (TIME-7).
template <class K>
class Publisher<set<K>> {
  public:
    set_delta<K> operator()(const Cell<set<K>>& cell) {
        set_delta<K> delta;
        if (cell.index() == 0) {
            delta = std::get<0>(cell);
            for (const K& member : delta.removed) {
                held_.erase(member);
            }
            held_.insert(delta.added.begin(), delta.added.end());
            return delta;
        }
        const set<K>& whole = std::get<1>(cell).value;
        for (const K& member : whole) {
            if (!held_.contains(member)) {
                delta.added.insert(member);
            }
        }
        for (const K& member : held_) {
            if (!whole.contains(member)) {
                delta.removed.insert(member);
            }
        }
        held_ = whole;
        return delta;
    }

  private:
    set<K> held_{};
};

/// What each cell of a trace of a dictionary publishes (TST-9): a delta as
/// it is, and a dictionary written whole as each entry it gives, "the same
/// value again included" (TIME-7), and the removal of each key it leaves out.
/// Each entry is published as its own value's cell would be, after the
/// entry's earlier ones: whole where its value publishes whole, and as its
/// change from what it held otherwise, a set's members among them.
template <class K, class V>
class Publisher<dict<K, V>> {
  public:
    dict_delta<K, V> operator()(const Cell<dict<K, V>>& cell) {
        dict_delta<K, V> delta;
        if (cell.index() == 0) {
            delta = std::get<0>(cell);
            for (const K& key : delta.removed) {
                entries_.erase(key);
            }
            for (const auto& [key, change] : delta.updated) {
                static_cast<void>(entries_[key](Cell<V>{change}));
            }
            return delta;
        }
        const dict<K, V>& whole = std::get<1>(cell).value;
        for (const auto& [key, entry] : whole) {
            if constexpr (publishes_whole<V>) {
                delta.updated.insert_or_assign(key, entries_[key](entry));
            } else {
                delta.updated.insert_or_assign(key, entries_[key](Cell<V>{Whole<V>{entry}}));
            }
        }
        for (auto at = entries_.begin(); at != entries_.end();) {
            if (whole.contains(at->first)) {
                ++at;
            } else {
                delta.removed.insert(at->first);
                at = entries_.erase(at);
            }
        }
        return delta;
    }

  private:
    /// What each key held publishes from, by key.
    dict<K, Publisher<V>> entries_{};
};

} // namespace hglrt::testing::detail
