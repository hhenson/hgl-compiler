// The reads that are checked when they run, which the generated code calls
// where the compiler cannot prove the read sound: a payload that may not be
// there (TIME-1, ERR-14) and a position that may be outside a list (EXPR-3,
// EXPR-7).
#pragma once

#include <hglrt/failure.h>
#include <hglrt/values.h>

#include <array>
#include <cstddef>
#include <source_location>
#include <string>
#include <type_traits>
#include <utility>
#include <vector>

namespace hglrt {

/// Fails a read that needs a value which is not there, with the code
/// `value.unset_read` (TIME-1, ERR-14; the catalogue: "Executed operation
/// requires an unset ordinary payload, a collection's child that is not
/// valid, or a struct, a tuple or a fixed list read whole that is not all
/// valid"). `message` says what was read.
[[noreturn]] void unset_read(std::string message = "a part that has not published is read",
                             std::source_location where = std::source_location::current());

/// The value of the endpoint `input`, a typed hgraph input view, read where
/// the read is checked when it runs (ERR-14): "a child that is not valid
/// fails the read with `value.unset_read`".
template <class Input>
auto checked_value(const Input& input, std::source_location where = std::source_location::current())
    -> std::remove_cvref_t<decltype(input.value())> {
    if (!input.valid()) {
        unset_read("the child read is not valid", where);
    }
    return input.value();
}

/// The same read where its place takes `result<T, error>` (ERR-14): "it gives
/// `ok` of the child's value, or, for a child that is not valid, `err` of an
/// `error` whose code is `value.unset_read`".
template <class Input>
auto checked_result(const Input& input) -> result<std::remove_cvref_t<decltype(input.value())>> {
    if (!input.valid()) {
        return std::unexpected(make_error("the child read is not valid", std::string{codes::unset_read}));
    }
    return input.value();
}

namespace detail {
/// Fails unless `index` is a position of an ordinary list of `size`
/// elements, unsized or fixed (EXPR-3: "An index outside a list, below zero
/// or at or past its size, fails where it is evaluated, with code
/// `value.index_out_of_range`"; EXPR-7: "An index outside an ordinary list
/// fails the read with code `value.index_out_of_range`, as it fails an
/// assignment"). `item` alone calls it: a tuple's index is a constant within
/// its length, and a temporal collection's child is read through
/// `checked_value`.
void check_index(i64 index, std::size_t size, std::source_location where);

/// What `item` of a list of `T` that outlives the read gives: a `const`
/// reference to the element, and for a `list<bool>` the `bool` itself.
template <class T>
using element_read = std::conditional_t<std::is_same_v<T, bool>, bool, const T&>;
} // namespace detail

/// `values[index]` (EXPR-7): the element of a list at an `i64` position. An
/// index outside the list, a negative one among them, fails with
/// `value.index_out_of_range` (EXPR-3).
///
/// Of a list that outlives the read, a reference to the element; of one that
/// ends with the read's statement (`names()[0]`), the element itself, moved
/// out of the list, so that nothing refers into a list that is gone: a
/// `const` reference bound to it, which a `let` is (docs/cpp-target.md),
/// keeps it. A `list<bool>` is a `std::vector<bool>`, which holds bits and so
/// has no `bool` to refer to, and whose `operator[]` gives a proxy of the
/// standard library's own (a `bool` in libstdc++, a class in libc++): a read
/// of one gives the `bool` itself, converted from it, on every library, and
/// a place the vector's reference to its bit, which refers into the vector.
template <class T>
detail::element_read<T> item(const std::vector<T>& values, i64 index,
                             std::source_location where = std::source_location::current()) {
    detail::check_index(index, values.size(), where);
    if constexpr (std::is_same_v<T, bool>) {
        return static_cast<bool>(values[static_cast<std::size_t>(index)]);
    } else {
        return values[static_cast<std::size_t>(index)];
    }
}

/// `values[index]` as a place, which an assignment writes (EXPR-3, FN-15):
/// `T&`, and for a `list<bool>` the vector's `reference` to the bit.
template <class T>
typename std::vector<T>::reference item(std::vector<T>& values, i64 index,
                                        std::source_location where = std::source_location::current()) {
    detail::check_index(index, values.size(), where);
    return values[static_cast<std::size_t>(index)];
}

/// `values[index]` of a list that ends with the read's statement: the
/// element, moved out of it (a `list<bool>`'s converted to a `bool`).
template <class T>
T item(std::vector<T>&& values, i64 index, std::source_location where = std::source_location::current()) {
    detail::check_index(index, values.size(), where);
    return static_cast<T>(std::move(values[static_cast<std::size_t>(index)]));
}

/// `values[index]` of a `const` list that ends with the read's statement:
/// a copy of the element (a `list<bool>`'s converted to a `bool`).
template <class T>
T item(const std::vector<T>&& values, i64 index,
       std::source_location where = std::source_location::current()) {
    detail::check_index(index, values.size(), where);
    return static_cast<T>(values[static_cast<std::size_t>(index)]);
}

/// `values[index]` of a fixed list, `list<T, N>` (TYPE-4, EXPR-3).
template <class T, std::size_t N>
const T& item(const std::array<T, N>& values, i64 index,
              std::source_location where = std::source_location::current()) {
    detail::check_index(index, N, where);
    return values[static_cast<std::size_t>(index)];
}

/// `values[index]` of a fixed list as a place.
template <class T, std::size_t N>
T& item(std::array<T, N>& values, i64 index, std::source_location where = std::source_location::current()) {
    detail::check_index(index, N, where);
    return values[static_cast<std::size_t>(index)];
}

/// `values[index]` of a fixed list that ends with the read's statement: the
/// element, moved out of it.
template <class T, std::size_t N>
T item(std::array<T, N>&& values, i64 index, std::source_location where = std::source_location::current()) {
    detail::check_index(index, N, where);
    return std::move(values[static_cast<std::size_t>(index)]);
}

/// `values[index]` of a `const` fixed list that ends with the read's
/// statement: a copy of the element.
template <class T, std::size_t N>
T item(const std::array<T, N>&& values, i64 index,
       std::source_location where = std::source_location::current()) {
    detail::check_index(index, N, where);
    return values[static_cast<std::size_t>(index)];
}

} // namespace hglrt
