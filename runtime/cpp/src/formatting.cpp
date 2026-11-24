#include <hglrt/formatting.h>

#include <hglrt/conversions.h>

#include <cmath>

#include <fmt/chrono.h>

#include "instant_text.h"

namespace hglrt {
namespace {

using namespace std::chrono;
constexpr std::uint64_t per_second = 1'000'000;
constexpr std::uint64_t per_minute = 60 * per_second;
constexpr std::uint64_t per_hour = 60 * per_minute;
constexpr std::uint64_t per_day = 24 * per_hour;

FormatClock clock_fields(std::uint64_t value, bool negative = false) {
    FormatClock fields;
    fields.microseconds = value;
    fields.negative = negative;
    fields.calendar.tm_hour = static_cast<int>((value / per_hour) % 24);
    fields.calendar.tm_min = static_cast<int>((value / per_minute) % 60);
    fields.calendar.tm_sec = static_cast<int>((value / per_second) % 60);
    return fields;
}

void calendar_fields(FormatClock& fields, sys_days day) {
    // The engine's signed microsecond clock extends beyond chrono::year.
    // Move it by whole Gregorian cycles before reading the month and day.
    constexpr sys_days cycle_start{year{2000} / January / 1};
    constexpr days cycle_days{146097};
    auto offset = day - cycle_start;
    auto cycles = offset / cycle_days;
    auto remainder = offset % cycle_days;
    if (remainder < days::zero()) {
        --cycles;
        remainder += cycle_days;
    }
    const sys_days within = cycle_start + remainder;
    const year_month_day civil{within};
    fields.calendar.tm_year = int(civil.year()) + 400 * static_cast<int>(cycles) - 1900;
    fields.calendar.tm_mon = static_cast<int>(unsigned(civil.month())) - 1;
    fields.calendar.tm_mday = static_cast<int>(unsigned(civil.day()));
    fields.calendar.tm_wday = static_cast<int>(weekday{within}.c_encoding());
    fields.calendar.tm_yday = static_cast<int>((within - sys_days{civil.year() / January / 1}).count());
}

void append_fraction(str& text, std::uint64_t microseconds, unsigned precision) {
    const auto fraction = microseconds % per_second;
    switch (precision) {
    case 3: text += fmt::format(FMT_COMPILE(".{:03}"), fraction / 1000); break;
    case 6: text += fmt::format(FMT_COMPILE(".{:06}"), fraction); break;
    case 9: text += fmt::format(FMT_COMPILE(".{:06}000"), fraction); break;
    default: break;
    }
}

} // namespace

datetime format_wall_time() {
    return std::chrono::floor<duration>(hgraph::engine_clock::now());
}

FormatClock format_clock(date value) {
    FormatClock fields;
    calendar_fields(fields, std::chrono::sys_days{value});
    return fields;
}

FormatClock format_clock(time value) {
    return clock_fields(static_cast<std::uint64_t>(value.microseconds));
}

FormatClock format_clock(datetime value) {
    using namespace std::chrono;
    auto into = value.time_since_epoch() % days{1};
    if (into < duration::zero()) {
        into += days{1};
    }
    FormatClock fields = clock_fields(static_cast<std::uint64_t>(into.count()));
    calendar_fields(fields, floor<days>(value));
    return fields;
}

FormatClock format_clock(duration value) {
    const auto micros = static_cast<std::uint64_t>(value.count());
    const bool negative = value.count() < 0;
    return clock_fields(negative ? std::uint64_t{0} - micros : micros, negative);
}

void format_directive(str& text, FormatClock& value, char directive, bool total, unsigned precision) {
    const auto& calendar = value.calendar;
    // Like the pinned library's duration formatter, write the sign with
    // the first numeric directive. A percent or AM/PM does not consume it.
    if (value.negative && directive != '%' && directive != 'p') {
        text += '-';
        value.negative = false;
    }
    if (total) {
        const auto unit = directive == 'H' ? per_hour : directive == 'M' ? per_minute : per_second;
        text += fmt::format(FMT_COMPILE("{:02}"), value.microseconds / unit);
        if (directive == 'S') {
            append_fraction(text, value.microseconds, precision);
        }
        return;
    }
    switch (directive) {
    case '%': text += '%'; break;
    case 'Y': text += fmt::format(FMT_COMPILE("{:%Y}"), calendar); break;
    case 'y': text += fmt::format(FMT_COMPILE("{:%y}"), calendar); break;
    case 'm': text += fmt::format(FMT_COMPILE("{:%m}"), calendar); break;
    case 'b': text += fmt::format(FMT_COMPILE("{:%b}"), calendar); break;
    case 'B': text += fmt::format(FMT_COMPILE("{:%B}"), calendar); break;
    case 'd': text += fmt::format(FMT_COMPILE("{:%d}"), calendar); break;
    case 'e': text += fmt::format(FMT_COMPILE("{:%e}"), calendar); break;
    case 'a': text += fmt::format(FMT_COMPILE("{:%a}"), calendar); break;
    case 'A': text += fmt::format(FMT_COMPILE("{:%A}"), calendar); break;
    case 'j':
        if (calendar.tm_mday == 0) {
            text += fmt::format(FMT_COMPILE("{}"), value.microseconds / per_day);
        } else {
            text += fmt::format(FMT_COMPILE("{:%j}"), calendar);
        }
        break;
    case 'F': text += fmt::format(FMT_COMPILE("{:%F}"), calendar); break;
    case 'D': text += fmt::format(FMT_COMPILE("{:%D}"), calendar); break;
    case 'H': text += fmt::format(FMT_COMPILE("{:%H}"), calendar); break;
    case 'I': text += fmt::format(FMT_COMPILE("{:%I}"), calendar); break;
    case 'p': text += fmt::format(FMT_COMPILE("{:%p}"), calendar); break;
    case 'M': text += fmt::format(FMT_COMPILE("{:%M}"), calendar); break;
    case 'R': text += fmt::format(FMT_COMPILE("{:%R}"), calendar); break;
    case 'S':
        text += fmt::format(FMT_COMPILE("{:%S}"), calendar);
        append_fraction(text, value.microseconds, precision);
        break;
    case 'T':
        text += fmt::format(FMT_COMPILE("{:%T}"), calendar);
        append_fraction(text, value.microseconds, precision);
        break;
    }
}

str format_default(date value) {
    return fmt::format(FMT_COMPILE("{:%F}"), format_clock(value).calendar);
}

str format_default(time value) {
    return detail::clock_text(value.microseconds, false);
}

str format_default(datetime value) {
    const auto clock = format_clock(value);
    auto text = fmt::format(FMT_COMPILE("{:%FT%T}"), clock.calendar);
    if (clock.microseconds % per_second != 0) {
        append_fraction(text, clock.microseconds, 6);
    }
    return text + 'Z';
}

str format_default(duration value) {
    if (value.count() == 0) {
        return "0s";
    }
    auto clock = format_clock(value);
    auto left = clock.microseconds;
    str text = clock.negative ? "-" : "";
    struct Unit { std::uint64_t size; std::string_view suffix; };
    constexpr Unit units[] = {
        {per_day, "d"}, {per_hour, "h"}, {per_minute, "m"},
        {per_second, "s"}, {1000, "ms"}, {1, "us"},
    };
    for (const auto& [size, suffix] : units) {
        if (left >= size) {
            text += fmt::format(FMT_COMPILE("{}{}"), left / size, suffix);
            left %= size;
        }
    }
    return text;
}

void format_text(str& output, std::string_view value, TextPadding layout,
                 std::optional<std::size_t> precision) {
    std::size_t characters = 0;
    for (std::size_t byte = 0; byte < value.size(); ++byte) {
        // Core strings are valid UTF-8. Only leading bytes start characters.
        if ((static_cast<unsigned char>(value[byte]) & 0xc0) == 0x80) {
            continue;
        }
        if (precision && characters == *precision) {
            value = value.substr(0, byte);
            break;
        }
        ++characters;
    }
    const auto padding = layout.width > characters ? layout.width - characters : 0;
    const auto before = layout.align == '>' ? padding : layout.align == '^' ? padding / 2 : 0;
    for (std::size_t count = 0; count < before; ++count) {
        output.append(layout.fill);
    }
    output.append(value);
    for (std::size_t count = before; count < padding; ++count) {
        output.append(layout.fill);
    }
}

void format_float_text(str& output, f64 value, TextPadding padding,
                       std::optional<char> sign, bool zero) {
    auto text = convert_f64<str>(value);
    if (std::signbit(value)) {
        // The default conversion calls every NaN "nan". A numeric sign
        // or zero flag still uses the scalar formatter's sign bit.
        if ((sign || zero) && !text.starts_with('-')) {
            text.insert(text.begin(), '-');
        }
    } else if (sign == '+' || sign == ' ') {
        text.insert(text.begin(), *sign);
    }

    if (zero && std::isfinite(value) && padding.width > text.size()) {
        const std::size_t sign_width =
            text.starts_with('-') || text.starts_with('+') || text.starts_with(' ') ? 1 : 0;
        output.append(text, 0, sign_width);
        output.append(padding.width - text.size(), '0');
        output.append(text, sign_width);
        return;
    }
    // Numeric zero padding of a non-finite value uses spaces.
    if (!std::isfinite(value) && padding.fill == "0") {
        padding.fill = " ";
    }
    format_text(output, text, padding, std::nullopt);
}

str format_quoted(std::string_view value) {
    str text{"\""};
    for (const unsigned char byte : value) {
        switch (byte) {
        case '"': text += "\\\""; break;
        case '\\': text += "\\\\"; break;
        case '\n': text += "\\n"; break;
        case '\r': text += "\\r"; break;
        case '\t': text += "\\t"; break;
        default:
            if (byte < 0x20 || byte == 0x7f) {
                text += fmt::format(FMT_COMPILE("\\u{{{:x}}}"), static_cast<unsigned>(byte));
            } else {
                text += static_cast<char>(byte);
            }
        }
    }
    return text + '"';
}

} // namespace hglrt
