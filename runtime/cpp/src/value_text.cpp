// Scalar value text for the renderer in value_text.h.
#include "instant_text.h"

#include <hglrt/conversions.h>
#include <hglrt/tenors.h>
#include <hglrt/value_text.h>

#include <cstddef>
#include <cstdint>
#include <format>
#include <string>
#include <string_view>
#include <utility>

namespace hglrt {

std::string detail::scalar_text(i64 value) {
    return hglrt::detail::number_text(value);
}

std::string detail::scalar_text(f64 value) {
    return hglrt::detail::number_text(value);
}

std::string detail::scalar_text(bool value) {
    return value ? "true" : "false";
}

std::string detail::scalar_text(const str& value) {
    std::string text = "\"";
    for (const char character : value) {
        const auto code = static_cast<unsigned char>(character);
        switch (character) {
        case '"':
            text += "\\\"";
            break;
        case '\\':
            text += "\\\\";
            break;
        case '\n':
            text += "\\n";
            break;
        case '\t':
            text += "\\t";
            break;
        case '\r':
            text += "\\r";
            break;
        default:
            if (code < 0x20 || code == 0x7f) {
                text += std::format("\\u{{{:x}}}", code);
            } else {
                text += character;
            }
        }
    }
    return text + "\"";
}

std::string detail::scalar_text(const bytes& value) {
    std::string text = "bytes(";
    for (std::size_t at = 0; at < value.data.size(); ++at) {
        text += std::format("{}{}", at == 0 ? "" : ", ", static_cast<unsigned char>(value.data[at]));
    }
    return text + ")";
}

std::string detail::scalar_text(date value) {
    return std::format("@{:%F}", value);
}

std::string detail::scalar_text(time value) {
    return "@" + hglrt::detail::clock_text(value.microseconds);
}

std::string detail::scalar_text(datetime value) {
    return "@" + hglrt::detail::instant_text(value, hglrt::detail::InstantFormat::compact) + "Z";
}

std::string detail::scalar_text(duration value) {
    // In the largest unit that holds it whole (LEX-11: `d`, `h`, `m`, `s`,
    // `ms`, `us`).
    const std::int64_t count = value.count();
    if (count == 0) {
        return "0s";
    }
    constexpr std::pair<std::int64_t, std::string_view> units[] = {
        {86'400'000'000, "d"}, {3'600'000'000, "h"}, {60'000'000, "m"}, {1'000'000, "s"}, {1'000, "ms"}};
    for (const auto& [size, unit] : units) {
        if (count % size == 0) {
            return std::format("{}{}", count / size, unit);
        }
    }
    return std::format("{}us", count);
}

std::string detail::scalar_text(const civil_datetime& value) {
    const datetime local{duration{value.epoch_microseconds()}};
    return "@" + hglrt::detail::instant_text(local, hglrt::detail::InstantFormat::compact);
}

std::string detail::scalar_text(const zoned_datetime& value) {
    return "@" + hgraph::format_zoned_datetime(value);
}

std::string detail::scalar_text(const tenor& value) {
    return hglrt::detail::tenor_text(value);
}

} // namespace hglrt
