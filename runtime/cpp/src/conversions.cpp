// Core's native conversions (conversions.h).
#include "numbers.h"

#include <hglrt/conversions.h>

#include <algorithm>
#include <array>
#include <charconv>
#include <cmath>
#include <cstddef>
#include <cstdint>
#include <limits>
#include <string>
#include <system_error>

namespace hglrt {

namespace {

using detail::u128;

bool decimal(char character) {
    return character >= '0' && character <= '9';
}

bool hexadecimal(char character) {
    return decimal(character) || (character >= 'a' && character <= 'f') ||
           (character >= 'A' && character <= 'F');
}

bool binary(char character) {
    return character == '0' || character == '1';
}

/// How many characters at the start of `text` are a run of the digits
/// `digit` takes, with one `_` between two digits at most (LEX-22): none
/// where `text` does not start with a digit.
std::size_t digit_run(std::string_view text, bool (*digit)(char)) {
    if (text.empty() || !digit(text.front())) {
        return 0;
    }
    std::size_t at = 1;
    while (at < text.size()) {
        if (digit(text[at])) {
            ++at;
        } else if (text[at] == '_' && at + 1 < text.size() && digit(text[at + 1])) {
            at += 2;
        } else {
            break;
        }
    }
    return at;
}

/// `text` without its `_`, which change no value (LEX-22).
std::string without_separators(std::string_view text) {
    std::string out;
    out.reserve(text.size());
    for (const char character : text) {
        if (character != '_') {
            out += character;
        }
    }
    return out;
}

/// An integer literal's digits, without `_`, and their base.
struct IntegerLiteral {
    std::string digits;
    int base = 10;
};

/// The integer literal that is all of `text` (the language's
/// `INTEGER_LITERAL`, LEX-22, LEX-23), where it is one: decimal digits; `0x`
/// and hexadecimal ones, of either case; or `0b` and binary ones. The prefix
/// letter is lowercase.
std::optional<IntegerLiteral> integer_literal(std::string_view text) {
    int base = 10;
    bool (*digit)(char) = decimal;
    if (text.starts_with("0x")) {
        base = 16;
        digit = hexadecimal;
        text.remove_prefix(2);
    } else if (text.starts_with("0b")) {
        base = 2;
        digit = binary;
        text.remove_prefix(2);
    }
    const std::size_t run = digit_run(text, digit);
    if (run == 0 || run != text.size()) {
        return std::nullopt;
    }
    return IntegerLiteral{without_separators(text), base};
}

/// The float literal that is all of `text` (the language's `FLOAT_LITERAL`:
/// digits, `.` and digits, then an exponent or none; or digits and an
/// exponent), without its `_`, where it is one.
std::optional<std::string> float_literal(std::string_view text) {
    std::size_t at = digit_run(text, decimal);
    if (at == 0) {
        return std::nullopt;
    }
    bool fraction = false;
    bool exponent = false;
    if (at < text.size() && text[at] == '.') {
        const std::size_t run = digit_run(text.substr(at + 1), decimal);
        if (run == 0) {
            return std::nullopt;
        }
        at += 1 + run;
        fraction = true;
    }
    if (at < text.size() && (text[at] == 'e' || text[at] == 'E')) {
        ++at;
        if (at < text.size() && (text[at] == '+' || text[at] == '-')) {
            ++at;
        }
        const std::size_t run = digit_run(text.substr(at), decimal);
        if (run == 0) {
            return std::nullopt;
        }
        at += run;
        exponent = true;
    }
    if (at != text.size() || !(fraction || exponent)) {
        return std::nullopt;
    }
    return without_separators(text);
}

/// The power of ten of the first digit that is not zero of the decimal
/// number `text`, a float literal without its `_`, which is not zero: how
/// `from_chars` is told apart where a number is out of its range, which it
/// is both for one too large and for one that rounds to zero.
long leading_power(std::string_view text) {
    const std::size_t mark = text.find_first_of("eE");
    long power = 0;
    if (mark != std::string_view::npos) {
        std::string_view written = text.substr(mark + 1);
        const bool negative = written.starts_with('-');
        if (written.starts_with('-') || written.starts_with('+')) {
            written.remove_prefix(1);
        }
        // A power beyond a million is beyond every f64 as well.
        for (const char character : written) {
            power = std::min(power * 10 + (character - '0'), 1'000'000L);
        }
        power = negative ? -power : power;
        text = text.substr(0, mark);
    }
    const std::size_t point = std::min(text.find('.'), text.size());
    const std::size_t first = text.find_first_not_of("0.");
    if (first < point) {
        return power + static_cast<long>(point - first) - 1;
    }
    return power - static_cast<long>(first - point);
}

/// The magnitude of `digits`, digits of `base`, saturated beyond 2^64.
u128 magnitude_of(const IntegerLiteral& literal) {
    constexpr u128 beyond = u128{1} << 64;
    u128 magnitude = 0;
    for (const char character : literal.digits) {
        const int value = decimal(character) ? character - '0'
                          : character >= 'a' ? character - 'a' + 10
                                             : character - 'A' + 10;
        magnitude =
            std::min(magnitude * static_cast<unsigned>(literal.base) + static_cast<unsigned>(value), beyond);
    }
    return magnitude;
}

/// The `f64` nearest the integer the literal `literal` writes, a tie to the
/// even one; an infinity where it rounds to one.
f64 nearest_of(const IntegerLiteral& literal) {
    std::string digits = literal.digits;
    std::chars_format format = std::chars_format::general;
    if (literal.base == 2) {
        // Binary digits as hexadecimal ones, four to each from the right.
        std::string hexadecimal_digits;
        const std::size_t lead = digits.size() % 4;
        for (std::size_t at = 0; at < digits.size();) {
            const std::size_t count = at == 0 && lead != 0 ? lead : 4;
            unsigned nibble = 0;
            for (std::size_t bit = 0; bit < count; ++bit) {
                nibble = nibble * 2 + static_cast<unsigned>(digits[at + bit] - '0');
            }
            hexadecimal_digits += "0123456789abcdef"[nibble];
            at += count;
        }
        digits = hexadecimal_digits;
    }
    if (literal.base != 10) {
        format = std::chars_format::hex;
    }
    f64 value = 0.0;
    const auto [end, failure] = std::from_chars(digits.data(), digits.data() + digits.size(), value, format);
    (void)end;
    // An integer is out of the range of an f64 only where it is too large.
    if (failure == std::errc::result_out_of_range) {
        return std::numeric_limits<f64>::infinity();
    }
    return value;
}

/// The `f64` nearest the decimal number `text`, a float literal without its
/// `_`, a tie to the even one: an infinity where it rounds to one, and zero
/// where it rounds to zero (LEX-20: "A nonzero literal that rounds to zero or
/// to a subnormal is accepted").
f64 nearest_of(const std::string& text) {
    f64 value = 0.0;
    const auto [end, failure] =
        std::from_chars(text.data(), text.data() + text.size(), value, std::chars_format::general);
    (void)end;
    if (failure == std::errc::result_out_of_range) {
        return leading_power(text) >= 0 ? std::numeric_limits<f64>::infinity() : 0.0;
    }
    return value;
}

/// `value`, which is finite, rounded to an integer as `rounding` says
/// (OPS-8). A number with a fraction is below 2^52, where the difference of
/// it and its whole part is exact, so a tie is seen as one.
f64 rounded(f64 value, Rounding rounding) {
    switch (rounding) {
    case Rounding::truncate:
        return std::trunc(value);
    case Rounding::floor:
        return std::floor(value);
    case Rounding::ceiling:
        return std::ceil(value);
    case Rounding::half_even:
    case Rounding::half_up:
    case Rounding::half_down:
        break;
    }
    const f64 whole = std::trunc(value);
    const f64 left = std::fabs(value - whole);
    const f64 away = whole + std::copysign(1.0, value);
    switch (rounding) {
    case Rounding::half_even:
        return left > 0.5 || (left == 0.5 && std::fmod(whole, 2.0) != 0.0) ? away : whole;
    case Rounding::half_up:
        return left >= 0.5 ? away : whole;
    default:
        return left > 0.5 ? away : whole;
    }
}

/// Whether `text` is well-formed UTF-8 (the Unicode standard's table 3-7):
/// no overlong form, no surrogate and nothing above U+10FFFF.
bool well_formed(std::string_view text) {
    const auto byte = [&](std::size_t at) { return static_cast<unsigned char>(text[at]); };
    std::size_t at = 0;
    while (at < text.size()) {
        const unsigned char lead = byte(at);
        if (lead < 0x80) {
            ++at;
            continue;
        }
        std::size_t length = 0;
        unsigned char low = 0x80;
        unsigned char high = 0xBF;
        if (lead >= 0xC2 && lead <= 0xDF) {
            length = 2;
        } else if (lead >= 0xE0 && lead <= 0xEF) {
            length = 3;
            low = lead == 0xE0 ? 0xA0 : 0x80;
            high = lead == 0xED ? 0x9F : 0xBF;
        } else if (lead >= 0xF0 && lead <= 0xF4) {
            length = 4;
            low = lead == 0xF0 ? 0x90 : 0x80;
            high = lead == 0xF4 ? 0x8F : 0xBF;
        } else {
            return false;
        }
        if (at + length > text.size()) {
            return false;
        }
        // The second byte has the lead's own bounds; each after it any
        // continuation byte.
        if (byte(at + 1) < low || byte(at + 1) > high) {
            return false;
        }
        for (std::size_t next = 2; next < length; ++next) {
            if (byte(at + next) < 0x80 || byte(at + next) > 0xBF) {
                return false;
            }
        }
        at += length;
    }
    return true;
}

} // namespace

error detail::invalid_conversion(std::string_view why) {
    return make_error(std::string{why}, std::string{codes::invalid_conversion});
}

void detail::fail_conversion(std::string_view why, std::source_location where) {
    fail(codes::invalid_conversion, std::string{why}, where);
}

str detail::number_text(i64 value) {
    return std::to_string(value);
}

str detail::number_text(f64 value) {
    if (std::isnan(value)) {
        return "nan";
    }
    if (std::isinf(value)) {
        return value > 0.0 ? "inf" : "-inf";
    }
    // The shortest digits that read back as `value`, in the form `d.dddde±x`.
    std::array<char, 32> written{};
    const auto [end, failure] =
        std::to_chars(written.data(), written.data() + written.size(), value, std::chars_format::scientific);
    (void)failure;
    const std::string_view form(written.data(), static_cast<std::size_t>(end - written.data()));
    const bool negative = form.starts_with('-');
    const std::size_t mark = form.find('e');
    std::string digits;
    for (const char character : form.substr(negative ? 1 : 0, mark - (negative ? 1 : 0))) {
        if (character != '.') {
            digits += character;
        }
    }
    int power = 0;
    (void)std::from_chars(form.data() + mark + 1 + (form[mark + 1] == '+' ? 1 : 0), form.data() + form.size(),
                          power);
    std::string text = negative ? "-" : "";
    if (power >= 16 || power < -4) {
        text += digits.front();
        if (digits.size() > 1) {
            text += '.';
            text += digits.substr(1);
        }
        text += 'e';
        text += std::to_string(power);
        return text;
    }
    if (power < 0) {
        text += "0.";
        text.append(static_cast<std::size_t>(-power - 1), '0');
        text += digits;
        return text;
    }
    const auto whole = static_cast<std::size_t>(power) + 1;
    if (digits.size() <= whole) {
        text += digits;
        text.append(whole - digits.size(), '0');
        text += ".0";
        return text;
    }
    text += digits.substr(0, whole);
    text += '.';
    text += digits.substr(whole);
    return text;
}

detail::made<i64> detail::whole_of(f64 value, Rounding rounding, std::optional<i64> nan) {
    if (std::isnan(value)) {
        if (nan.has_value()) {
            return *nan;
        }
        return std::unexpected("a NaN is no `i64`");
    }
    if (std::isinf(value)) {
        return std::unexpected("an infinity is no `i64`");
    }
    const f64 whole = rounded(value, rounding);
    // 2^63, the least f64 above every i64; -2^63 is the least i64.
    constexpr f64 beyond = 9223372036854775808.0;
    if (whole < -beyond || whole >= beyond) {
        return std::unexpected("the number is outside `i64` once rounded");
    }
    return static_cast<i64>(whole);
}

detail::made<i64> detail::integer_of(std::string_view text) {
    const bool negative = text.starts_with('-');
    if (negative) {
        text.remove_prefix(1);
    }
    const std::optional<IntegerLiteral> literal = integer_literal(text);
    if (!literal.has_value()) {
        return std::unexpected("the text is no integer literal");
    }
    const u128 magnitude = magnitude_of(*literal);
    if (!fits_i64(negative, magnitude)) {
        return std::unexpected("the number the text writes is outside `i64`");
    }
    return signed_of(negative, magnitude);
}

detail::made<f64> detail::real_of(std::string_view text) {
    if (text == "nan") {
        return std::numeric_limits<f64>::quiet_NaN();
    }
    if (text == "inf") {
        return std::numeric_limits<f64>::infinity();
    }
    if (text == "-inf") {
        return -std::numeric_limits<f64>::infinity();
    }
    const bool negative = text.starts_with('-');
    if (negative) {
        text.remove_prefix(1);
    }
    f64 magnitude = 0.0;
    if (const std::optional<std::string> written = float_literal(text); written.has_value()) {
        magnitude = nearest_of(*written);
    } else if (const std::optional<IntegerLiteral> literal = integer_literal(text); literal.has_value()) {
        magnitude = nearest_of(*literal);
    } else {
        return std::unexpected("the text is no number");
    }
    if (std::isinf(magnitude)) {
        return std::unexpected("the number the text writes rounds to an infinity");
    }
    return negative ? -magnitude : magnitude;
}

detail::made<bool> detail::flag_of(std::string_view text) {
    if (text == "true") {
        return true;
    }
    if (text == "false") {
        return false;
    }
    return std::unexpected("the text is no `bool` literal");
}

detail::made<str> detail::decoded(const bytes& value) {
    if (!well_formed(value.data)) {
        return std::unexpected("the bytes are not well-formed UTF-8");
    }
    return value.data;
}

} // namespace hglrt
