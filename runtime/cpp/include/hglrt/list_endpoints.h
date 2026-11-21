// The temporal form of a list (TYPE-9, TYPE-10): what the generated code
// calls to read one, to read and wire a child of one, to
// publish to one, and to read what one published in a cycle as HGL's delta
// (TYPE-16, TIME-1, TIME-7, TIME-9; core's END-3, END-7, END-8).
//
// A growing `list<V>` is hgraph's `TSL<S>`, of the size `unbounded_tsl_size`,
// a child for each position, S the temporal form of V; a `list<V, N>` is
// `TSL<S, N>`. A tuple's form is structs.h's.
//
// hgraph 0.9.13's `TSL` gives what TIME-7, TIME-9 and TYPE-16 say of a list,
// and this library builds on it with no adaptation: an empty list published
// first, or a delta that names no change published to a list that is not
// valid, makes it valid and ticks; an empty list given to a valid empty one,
// a resize to the size it has, and the removal of a position that holds no
// value are no tick; a list whose every position lost its value is not
// valid; an invalidation empties a growing list; and its delta names each
// position updated, each that lost its value (the tail a shrink drops among
// them), and the new size where it moved (hgraph issues 1822 and 1827, fixed
// in 0.9.12 and 0.9.13).
//
// A list's value is `schema_value<S>`, an `hglrt::list` or a `std::array`;
// it is read whole through `whole<S>`, published through `publisher<S>`, and
// read as its delta through `endpoint_read<S>` (endpoints.h), and each of its
// elements through the element's own form's (TYPE-9): a scalar's or a
// nullable's (endpoints.h), a list's (here), a struct's or a tuple's
// (structs.h).
#pragma once

#include <hglrt/checked.h>
#include <hglrt/endpoints.h>
#include <hglrt/failure.h>
#include <hglrt/testing.h>
#include <hglrt/values.h>

#include <hgraph/types/generated.h>

#include <algorithm>
#include <array>
#include <cstddef>
#include <format>
#include <limits>
#include <optional>
#include <source_location>
#include <string>
#include <type_traits>
#include <utility>
#include <vector>

namespace hglrt {

// ---- The shapes of the forms ------------------------------------------------

namespace detail {

/// What a list's schema `S` is: its element's schema, whether it grows, and
/// a fixed list's size.
template <class S>
struct list_schema : std::false_type {};
template <class C, auto N>
struct list_schema<hgraph::TSL<C, N>> : std::true_type {
    static_assert(std::is_integral_v<decltype(N)>, "hglrt: a list's size is a number");
    using element = C;
    /// TYPE-10: "`list<T>` denotes a growing shape", hgraph's `TSL` of
    /// `unbounded_tsl_size`.
    static constexpr bool growing = static_cast<std::size_t>(N) == hgraph::unbounded_tsl_size;
    static constexpr std::size_t size = growing ? 0 : static_cast<std::size_t>(N);
};

} // namespace detail

/// A growing list's value is an `hglrt::list` of its element's, and a fixed
/// one's a `std::array` (TYPE-10).
template <class C, auto N>
struct schema_value<hgraph::TSL<C, N>> {
    using type =
        std::conditional_t<detail::list_schema<hgraph::TSL<C, N>>::growing, list<schema_value_t<C>>,
                           std::array<schema_value_t<C>, detail::list_schema<hgraph::TSL<C, N>>::size>>;
};

// ---- Children (TIME-1, TIME-9) --------------------------------------------

/// A child of a temporal list, an element, named in a hook: `parts[1]`,
/// `values[i]` (TIME-1: "A child of a collection ... is an
/// endpoint whether or not it is valid"). It holds the typed view of the
/// child, `V`, where the collection has the child, and nothing where a
/// growing list does not reach it: a position past its end, which is a child
/// that is not valid. Core's operations on an endpoint take it as they take a
/// view (endpoints.h), and so does a read of it.
template <class V>
class Child {
  public:
    /// The child's hgraph schema.
    using schema = typename V::schema;

    /// A position a growing list does not hold.
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

/// `values[index]`, the child at `index` of the temporal list `values`, a
/// view or a child of one (TIME-1, TIME-9): an endpoint whether or not it is
/// valid. A position past a growing list's end is a child that is not valid,
/// as one between its positions that holds no value is (TYPE-16). An index
/// below zero, or past a fixed list's size, is outside the list, and fails
/// with `value.index_out_of_range` (EXPR-3: "An index outside a list, below
/// zero or at or past its size, fails where it is evaluated").
template <class E>
    requires detail::list_schema<schema_of<E>>::value
auto child(const E& values, i64 index, std::source_location where = std::source_location::current()) {
    using Shape = detail::list_schema<schema_of<E>>;
    using View = std::remove_cvref_t<decltype(std::declval<const detail::viewed_t<E>&>()[std::size_t{}])>;
    const std::size_t held = detail::present(values) ? detail::viewed(values).size() : 0;
    if constexpr (!Shape::growing) {
        detail::check_index(index, Shape::size, where);
    } else if (index < 0) {
        detail::check_index(index, held, where);
    }
    if (static_cast<std::uint64_t>(index) >= held) {
        return Child<View>{};
    }
    return Child<View>{detail::viewed(values)[static_cast<std::size_t>(index)]};
}

// ---- Reading whole (TIME-1, FN-12) -----------------------------------------

namespace detail {

/// What a position of a growing list of `V` that holds no value reads as in
/// the whole list (TIME-1's table): NaN for an `f64`, null for a `V?`, and
/// none for any other.
template <class V>
std::optional<V> hole() {
    if constexpr (std::is_same_v<V, f64>) {
        return std::numeric_limits<f64>::quiet_NaN();
    } else if constexpr (is_optional<V>::value) {
        return V{};
    } else {
        return std::nullopt;
    }
}

/// The value of the child `child`, of the schema `C`, in a whole fixed list:
/// null for an optional child that is not valid, and otherwise its
/// own whole read.
template <class C, class V>
std::optional<schema_value_t<C>> part(const V& child) {
    if constexpr (nullable_schema<C>::value) {
        if (!child.valid()) {
            return schema_value_t<C>{};
        }
    }
    return whole<C>::read(child);
}

} // namespace detail

template <class C, auto N>
struct whole<hgraph::TSL<C, N>> {
    using Shape = detail::list_schema<hgraph::TSL<C, N>>;
    using V = schema_value_t<C>;

    template <class E>
    static std::optional<schema_value_t<hgraph::TSL<C, N>>> read(const E& endpoint) {
        if constexpr (Shape::growing) {
            if (!endpoint.valid()) {
                return std::nullopt;
            }
            const auto& view = detail::viewed(endpoint);
            list<V> out;
            out.reserve(view.size());
            for (std::size_t index = 0; index < view.size(); ++index) {
                const auto element = view[index];
                std::optional<V> read = element.valid() ? whole<C>::read(element) : detail::hole<V>();
                if (!read.has_value()) {
                    return std::nullopt;
                }
                out.push_back(std::move(*read));
            }
            return out;
        } else {
            if (!endpoint_read<hgraph::TSL<C, N>>::all_valid(endpoint)) {
                return std::nullopt;
            }
            const auto& view = detail::viewed(endpoint);
            std::array<V, Shape::size> out{};
            for (std::size_t index = 0; index < Shape::size; ++index) {
                std::optional<V> read = detail::part<C>(view[index]);
                if (!read.has_value()) {
                    return std::nullopt;
                }
                out[index] = std::move(*read);
            }
            return out;
        }
    }
};

/// The value of the endpoint `endpoint` read whole where the read is checked
/// when it runs (TIME-1, ERR-14): a read of a temporal input in a hook, of a
/// child named at an index known only then, and what a lifted helper is
/// given (FN-12). One that may not be read whole fails with
/// `value.unset_read`.
template <class E>
schema_value_t<schema_of<E>> checked_whole(const E& endpoint,
                                           std::source_location where = std::source_location::current()) {
    std::optional<schema_value_t<schema_of<E>>> read = whole<schema_of<E>>::read(endpoint);
    if (!read.has_value()) {
        unset_read("a value read whole that may not be read whole there", where);
    }
    return std::move(*read);
}

/// The same read where its place takes `result<T, error>` (ERR-14): `ok` of
/// the value, or `err` of an `error` whose code is `value.unset_read`.
template <class E>
result<schema_value_t<schema_of<E>>> checked_whole_result(const E& endpoint) {
    std::optional<schema_value_t<schema_of<E>>> read = whole<schema_of<E>>::read(endpoint);
    if (!read.has_value()) {
        return std::unexpected(make_error("a value read whole that may not be read whole there",
                                          std::string{codes::unset_read}));
    }
    return std::move(*read);
}

// ---- Validity and deltas (TIME-9, TYPE-16; core's END-3, END-7) ------------

/// A temporal list. `valid` is hgraph's own, as TIME-9 has it: "`valid` of a
/// collection is true where any element ... is valid", and "A published empty
/// list ... is valid".
template <class C, auto N>
struct endpoint_read<hgraph::TSL<C, N>> {
    using Shape = detail::list_schema<hgraph::TSL<C, N>>;
    using delta_type = delta_t<schema_value_t<hgraph::TSL<C, N>>>;

    /// END-7, TIME-9: "`all_valid` is true where every one is, by its own
    /// `valid`, one level deep, an optional child aside ... It is true of a
    /// valid empty collection. `all_valid` of a collection that is not valid
    /// is false."
    template <class E>
    static bool all_valid(const E& endpoint) {
        if (!endpoint.valid()) {
            return false;
        }
        if constexpr (detail::nullable_schema<C>::value) {
            return true;
        } else {
            const auto& view = detail::viewed(endpoint);
            for (std::size_t index = 0; index < view.size(); ++index) {
                if (!view[index].valid()) {
                    return false;
                }
            }
            return true;
        }
    }

    /// END-3, TYPE-16: "`updated`, each changed position with its delta;
    /// `removed`, the positions that lost their value ... and
    /// `size_modified`, set only when the size moves up or down, to the new
    /// size ... The delta read gives the new size where it moves, never a
    /// negative one, and on a shrink names the tail positions it drops in
    /// `removed`": hgraph's own delta of a `TSL`, each position's delta read
    /// as its form reads one.
    template <class E>
    static delta_type delta(const E& endpoint) {
        const auto& view = detail::viewed(endpoint);
        delta_type change;
        for (const auto& [index, element] : view.modified_items()) {
            static_cast<void>(element);
            change.updated.emplace(static_cast<i64>(index), endpoint_read<C>::delta(view[index]));
        }
        for (const std::size_t index : view.removed_positions()) {
            change.removed.insert(static_cast<i64>(index));
        }
        if constexpr (Shape::growing) {
            if (const std::optional<std::size_t> size = view.size_modified(); size.has_value()) {
                change.size_modified = static_cast<i64>(*size);
            }
        }
        return change;
    }
};

// ---- Publishing (TIME-7, TYPE-16) ------------------------------------------

namespace detail {

/// END-4: a child of an output loses its value.
template <class O>
void invalidated(const O& child) {
    static_cast<void>(child.begin_mutation(child.evaluation_time()).invalidate());
}

/// Fails the publication of a list's delta that its list cannot take (TYPE-16:
/// "the write that publishes it fails"), with no code: the catalogue names
/// none for it.
[[noreturn]] void refused_delta(std::string message, std::source_location where);

} // namespace detail

template <class C, auto N>
struct publisher<hgraph::TSL<C, N>> {
    using Shape = detail::list_schema<hgraph::TSL<C, N>>;
    using L = schema_value_t<hgraph::TSL<C, N>>;

    /// TIME-7: "A collection published whole is published as its change from
    /// what the result holds. Each ... position of a list that it gives is
    /// published, the same value again included, as an equal scalar is ... a
    /// position that it leaves out is removed. Where that leaves nothing to
    /// publish ... an empty list ... given to a valid empty one, the value is
    /// unchanged, and it is no tick. Where a `list<V>` ... is not valid, an
    /// empty one given whole makes it valid, and ticks": a growing list is
    /// given the size of the value, and each position its element.
    template <class O>
    static void whole(const O& out, const L& value, std::source_location where) {
        if constexpr (Shape::growing) {
            out.resize(value.size());
        }
        for (std::size_t index = 0; index < value.size(); ++index) {
            publish_part(out[index], value[index], where);
        }
    }

    /// TIME-7: "a `delta<T>` (TYPE-16), published as that change". TYPE-16:
    /// "A delta written with a smaller `size_modified` and no `removed`
    /// shrinks the list. A negative `size_modified` written reduces the list
    /// by that count ... a count more than the list holds leaves it empty,
    /// which is no failure. An `updated` entry written at the position after a
    /// `list<V>`'s last, with no `size_modified`, extends the list to hold it.
    /// One written further past it extends the list as well, and the positions
    /// between hold no value and are not valid ... A `list<V, N>` keeps its
    /// bounds: ... one in a delta made as the program runs fails the write
    /// that publishes it, as an index outside a list does (EXPR-3). An
    /// `updated` entry at or past the size that a `size_modified` written in
    /// the same delta gives contradicts it: ... the write that publishes it
    /// fails ... [A `removed` that names] a position of a list that holds no
    /// value, never set or already removed [is ignored]". A delta that names
    /// no change gives a growing list the size it has, which makes one that is
    /// not valid valid and empty, and ticks, and is no tick of a valid one
    /// (TIME-7); a fixed list's sets no child, and is never a tick.
    template <class O>
    static void change(const O& out, const delta_t<L>& delta, std::source_location where) {
        // EXPR-3: a position below zero is outside every list.
        for (const auto& [index, element] : delta.updated) {
            static_cast<void>(element);
            if (index < 0) {
                detail::check_index(index, out.size(), where);
            }
        }
        for (const i64 index : delta.removed) {
            if (index < 0) {
                detail::check_index(index, out.size(), where);
            }
        }
        std::size_t size = 0;
        if constexpr (Shape::growing) {
            // The size the list holds: a list whose every position lost its
            // value keeps its positions, not valid (TYPE-16), and one
            // invalidated holds none, as hgraph empties it (END-4).
            const std::size_t held = out.size();
            size = held;
            if (delta.size_modified.has_value()) {
                const i64 written = *delta.size_modified;
                if (written < 0) {
                    const std::uint64_t dropped = static_cast<std::uint64_t>(-(written + 1)) + 1;
                    size = dropped >= held ? 0 : held - static_cast<std::size_t>(dropped);
                } else {
                    size = static_cast<std::size_t>(written);
                }
                if (!delta.updated.empty() &&
                    static_cast<std::uint64_t>(delta.updated.rbegin()->first) >= size) {
                    detail::refused_delta(
                        std::format("the delta updates the position {} and gives the list a size of {}",
                                    delta.updated.rbegin()->first, size),
                        where);
                }
            } else if (!delta.updated.empty()) {
                size = std::max(held, static_cast<std::size_t>(delta.updated.rbegin()->first) + 1);
            }
            out.resize(size);
        } else {
            size = Shape::size;
            for (const auto& [index, element] : delta.updated) {
                static_cast<void>(element);
                detail::check_index(index, size, where);
            }
            for (const i64 index : delta.removed) {
                detail::check_index(index, size, where);
            }
            if (delta.size_modified.has_value()) {
                const i64 written = *delta.size_modified;
                const bool kept = written >= 0 && static_cast<std::uint64_t>(written) == size;
                if (!kept) {
                    detail::refused_delta(
                        std::format("a list of {} keeps its size, which the delta moves", size), where);
                }
            }
        }
        for (const i64 index : delta.removed) {
            if (static_cast<std::uint64_t>(index) < size) {
                detail::invalidated(out[static_cast<std::size_t>(index)]);
            }
        }
        for (const auto& [index, element] : delta.updated) {
            change_part(out[static_cast<std::size_t>(index)], element, where);
        }
    }

  private:
    template <class P, class V>
    static void publish_part(const P& part, const V& value, std::source_location where) {
        publisher<C>::whole(part, value, where);
    }
    template <class P, class D>
    static void change_part(const P& part, const D& delta, std::source_location where) {
        publisher<C>::change(part, delta, where);
    }
};

/// `return value` or `out = value` to the list output `out` (TIME-7): a
/// complete value of the result's type T, published whole, or a `delta<T>`,
/// published as that change, each as the output's form publishes
/// (`publisher`). A struct's and a tuple's `publish` is structs.h's.
template <class O, class V>
    requires detail::list_schema<schema_of<O>>::value
void publish(const O& out, const V& value, std::source_location where = std::source_location::current()) {
    publish_as_form(out, value, where);
}

// ---- Wiring a child (TIME-1, TIME-9) ---------------------------------------

/// The connection of the element `index` of the temporal list `values`, a
/// connection of a graph: `pair[0]` (TIME-9: "a child of one, `parts[1]`").
/// It is the projection hgraph's own `tsl_element` makes, of any source a
/// port is, written here so that generated code needs no more of hgraph's
/// wiring than `generated.h` gives. An index below zero, or past a fixed
/// list's size, is outside the list (EXPR-3).
template <class C, auto N>
hgraph::Port<C> element_port(const hgraph::Port<hgraph::TSL<C, N>>& values, i64 index,
                             std::source_location where = std::source_location::current()) {
    using Shape = detail::list_schema<hgraph::TSL<C, N>>;
    if constexpr (!Shape::growing) {
        detail::check_index(index, Shape::size, where);
    } else if (index < 0) {
        detail::check_index(index, 0, where);
    }
    const auto position = static_cast<std::size_t>(index);
    const hgraph::WiringPortRef list = values.erased();
    const hgraph::TSValueTypeMetaData* schema = hgraph::schema_descriptor<C>::ts_meta();
    const auto projected = [&](std::vector<std::size_t> path) {
        path.push_back(position);
        return path;
    };
    hgraph::WiringPortRef element;
    switch (list.source_kind()) {
    case hgraph::WiringPortRef::SourceKind::Peered:
        element = hgraph::WiringPortRef::peered_source(list.peered_node(), projected(list.peered_path()),
                                                       schema, list.peered_output_kind());
        break;
    case hgraph::WiringPortRef::SourceKind::Structural:
        element = list.structural_children().at(position);
        break;
    case hgraph::WiringPortRef::SourceKind::Boundary:
        element = list.projected_boundary_source(projected(list.boundary_path()), schema);
        break;
    case hgraph::WiringPortRef::SourceKind::Delayed:
        element = list.projected_delayed_source(projected(list.delayed_path()), schema);
        break;
    case hgraph::WiringPortRef::SourceKind::Null:
        element = hgraph::WiringPortRef::null_source(schema);
        break;
    case hgraph::WiringPortRef::SourceKind::Unbound:
        fail_uncoded("a child of a connection that is not bound", where);
    }
    return hgraph::Port<C>{values.wiring(), std::move(element)};
}

} // namespace hglrt
