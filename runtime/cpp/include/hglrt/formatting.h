// Core's native value formatting (FMT-4, FMT-5, FMT-7, FMT-9).
// The emitter supplies literal text, typed arguments and compiled specifiers.
#pragma once

#include <hglrt/values.h>

#include <fmt/compile.h>

#include <cstddef>
#include <ctime>
#include <optional>
#include <string_view>

namespace hglrt {

/// A value's calendar and clock fields, with an independent duration sign.
struct FormatClock {
    std::tm calendar{};
    std::uint64_t microseconds = 0;
    bool negative = false;
};

/// Read the wall clock once at the formatting call (FMT-9).
datetime format_wall_time();

/// Form the fields used by chrono directives, without changing precision.
FormatClock format_clock(date value);
FormatClock format_clock(time value);
FormatClock format_clock(datetime value);
FormatClock format_clock(duration value);

/// Append a single compile-time checked directive (FMT-4).
void format_directive(str& text, FormatClock& value, char directive, bool total, unsigned precision);

/// ISO calendar text and the duration literal used with no specifier (FMT-5).
str format_default(date value);
str format_default(time value);
str format_default(datetime value);
str format_default(duration value);

/// The checked fill, minimum width and alignment of a text field (FMT-4).
struct TextPadding {
    std::string_view fill;
    std::size_t width;
    char align;
};

/// Append a plain text field with width and precision in UTF-8 code points (FMT-4).
void format_text(str& output, std::string_view value, TextPadding padding,
                 std::optional<std::size_t> precision);

/// Append FMT-5 float text with the field's FMT-4 sign and padding.
void format_float_text(str& output, f64 value, TextPadding padding,
                       std::optional<char> sign, bool zero);

/// Write an HGL string literal for `{:?}` and compound members (FMT-4, FMT-5).
str format_quoted(std::string_view value);

/// Write a list or set in exactly the order of traversal (FMT-5).
template <class Collection, class Text>
str format_collection(const Collection& values, Text render) {
    str text{"["};
    std::string_view separator;
    for (const auto& value : values) {
        text += separator;
        text += render(value);
        separator = ", ";
    }
    return text + ']';
}

/// Write dictionary entries in exactly the order of traversal (FMT-5).
template <class Dictionary, class KeyText, class ValueText>
str format_dictionary(const Dictionary& values, KeyText key_text, ValueText value_text) {
    str text{"["};
    std::string_view separator;
    for (const auto& [key, value] : values) {
        text += separator;
        text += key_text(key);
        text += ": ";
        text += value_text(value);
        separator = ", ";
    }
    return text + ']';
}

} // namespace hglrt
