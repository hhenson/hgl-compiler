// The text of a value as HGL writes it, for application output and failures.
#pragma once

#include <hglrt/values.h>

#include <string>
#include <tuple>
#include <type_traits>
#include <variant>

namespace hglrt {

namespace detail {
template <class T>
struct is_variant : std::false_type {};
template <class... T>
struct is_variant<std::variant<T...>> : std::true_type {};

/// The text of a scalar, as the language writes a literal of it.
std::string scalar_text(i64 value);
std::string scalar_text(f64 value);
std::string scalar_text(bool value);
std::string scalar_text(const str& value);
std::string scalar_text(const bytes& value);
std::string scalar_text(date value);
std::string scalar_text(time value);
std::string scalar_text(datetime value);
std::string scalar_text(duration value);
std::string scalar_text(const civil_datetime& value);
std::string scalar_text(const zoned_datetime& value);
std::string scalar_text(const tenor& value);
} // namespace detail

/// `value` as the language writes it, for a failure or an application result: a
/// scalar as its literal, `null`, a tuple `(1, 2.0)`, a list `[1, 2]`, a
/// list's delta by its properties, an enum's member as `Type.member`
/// (TYPE-7), from what its `hgl_enum` names (values.h).
template <class T>
std::string text_of(const T& value) {
    if constexpr (hglrt::detail::is_optional<T>::value) {
        return value.has_value() ? text_of(*value) : std::string{"null"};
    } else if constexpr (hglrt::detail::is_tuple<T>::value) {
        std::string parts;
        std::apply(
            [&](const auto&... part) { ((parts += (parts.empty() ? "" : ", ") + text_of(part)), ...); },
            value);
        return "(" + parts + ")";
    } else if constexpr (hglrt::detail::is_list<T>::value || hglrt::detail::is_fixed_list<T>::value) {
        std::string parts;
        for (const auto& element : value) {
            parts += (parts.empty() ? "" : ", ") + text_of(element);
        }
        return "[" + parts + "]";
    } else if constexpr (hglrt::detail::is_list_delta<T>::value) {
        std::string updated;
        for (const auto& [position, change] : value.updated) {
            updated += (updated.empty() ? "" : ", ") + text_of(position) + ": " + text_of(change);
        }
        std::string removed;
        for (const i64 position : value.removed) {
            removed += (removed.empty() ? "" : ", ") + text_of(position);
        }
        std::string text = "delta<" + type_name<typename T::list_type>() + ">(updated: [" + updated +
                           "], removed: [" + removed + "]";
        if (value.size_modified.has_value()) {
            text += ", size_modified: " + text_of(*value.size_modified);
        }
        return text + ")";
    } else if constexpr (hglrt::detail::is_nullable_delta<T>::value) {
        // TYPE-16: `delta<i64?>(value: 7)`, and `delta<i64?>()` a published
        // null.
        return type_name<T>() + "(" + (value.value.has_value() ? "value: " + text_of(*value.value) : "") +
               ")";
    } else if constexpr (detail::is_variant<T>::value) {
        return std::visit([](const auto& held) { return text_of(held); }, value);
    } else if constexpr (requires { value_form<T>::text(value); }) {
        return value_form<T>::text(value);
    } else if constexpr (named_enum<T>) {
        return hglrt::detail::member_text(value);
    } else if constexpr (requires { detail::scalar_text(value); }) {
        return detail::scalar_text(value);
    } else {
        return "a value of " + type_name<T>();
    }
}

} // namespace hglrt
