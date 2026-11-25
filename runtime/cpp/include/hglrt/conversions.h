// Core's native conversions (`hgl/native/scalar_operators.hgl`, core's
// scalar contract OPS-4 and OPS-6 to OPS-8): `convert<T>(value)` of the
// scalars, those that cannot fail and those that can.
//
// Core's candidates of `convert` that take one type give different ones
// (OPS-4): `convert(value: i64) -> f64` and `convert(value: i64) -> str` are
// both named `convert_i64` by the emitter's scheme. Each such name is one
// function template here whose first argument is the type converted to,
// `convert_i64<f64>`, the `T` of the call `convert<T>(value)`; where only one
// candidate has a name, the type it converts to is that argument's default.
// A conversion that can fail takes `invalid` (OPS-8) as the template
// argument after it, as a division takes `zero`: under `.error`, the
// default, it gives `result<T, error>`, a failure an `err` carrying the code
// `convert.invalid`, and under `.fail` the plain `T`, a failure raised with
// that code.
#pragma once

#include <hglrt/failure.h>
#include <hglrt/values.h>

#include <chrono>
#include <concepts>
#include <cstddef>
#include <expected>
#include <optional>
#include <source_location>
#include <string_view>
#include <type_traits>
#include <utility>

namespace hglrt {

/// The members of core's enum `Rounding` (OPS-8), in its order: how an `f64`
/// is rounded to an `i64`.
enum class Rounding { truncate, floor, ceiling, half_even, half_up, half_down };

/// The names of core's `Rounding` (values.h, `EnumNames`).
constexpr EnumNames<Rounding, 6> hgl_enum(Rounding) noexcept {
    return {"core::Rounding",
            {{Rounding::truncate, "truncate"},
             {Rounding::floor, "floor"},
             {Rounding::ceiling, "ceiling"},
             {Rounding::half_even, "half_even"},
             {Rounding::half_up, "half_up"},
             {Rounding::half_down, "half_down"}}};
}

/// The members of core's enum `InvalidPolicy` (OPS-8), in its order: what a
/// conversion that fails does.
enum class InvalidPolicy { error, fail };

/// The names of core's `InvalidPolicy` (values.h, `EnumNames`).
constexpr EnumNames<InvalidPolicy, 2> hgl_enum(InvalidPolicy) noexcept {
    return {"core::InvalidPolicy", {{InvalidPolicy::error, "error"}, {InvalidPolicy::fail, "fail"}}};
}

namespace detail {
/// What a conversion that can fail gives under `invalid`: under `.error`,
/// `result<T, error>`, and under `.fail` a `T`.
template <class T, InvalidPolicy invalid>
using converted = std::conditional_t<invalid == InvalidPolicy::error, result<T>, T>;

/// A conversion can borrow its diagnostic text from static character data
/// only. Constructing it at compile time prevents a view of a temporary.
class conversion_reason {
  public:
    template <std::size_t N>
    consteval explicit conversion_reason(const char (&literal)[N]) : text_{literal} {}

    constexpr std::string_view text() const { return text_; }

  private:
    std::string_view text_;
};

/// A conversion that can fail, made: its value, or what says why there is
/// none.
template <class T>
using made = std::expected<T, conversion_reason>;

/// The error of a conversion that failed for the reason `why` (OPS-8: "a
/// failure is an Err carrying error code `convert.invalid`").
error invalid_conversion(std::string_view why);

/// Fails a conversion for the reason `why`, with the code `convert.invalid`
/// (OPS-8: "`.fail`: it returns T, and a failure is an escaping HGL error
/// with that code").
[[noreturn]] void fail_conversion(std::string_view why, std::source_location where);

/// What the conversion that made `value` gives under `invalid`.
template <class T, InvalidPolicy invalid>
converted<T, invalid> outcome(made<T> value, std::source_location where) {
    if (!value.has_value()) {
        if constexpr (invalid == InvalidPolicy::error) {
            return std::unexpected(invalid_conversion(value.error().text()));
        } else {
            fail_conversion(value.error().text(), where);
        }
    }
    return std::move(*value);
}

/// OPS-6, `i64` to `str`: "Its decimal digits, after a `-` where it is below
/// zero".
str number_text(i64 value);

/// OPS-6, `f64` to `str`: "The shortest decimal text that reads back as the
/// same `f64`, always with a `.` or an exponent, after a `-` where its sign
/// is negative ... It has an exponent where the number is 1e16 or more, or
/// below 1e-4 and not zero, in size ... A NaN is `nan`, and the infinities
/// are `inf` and `-inf`". The exponent is written as the language writes a
/// literal's: `1e21`, `1.5e-7`.
str number_text(f64 value);

/// OPS-7, OPS-8: the `i64` that `rounding` selects for `value`, or `nan`
/// for a NaN where it is given; "an `f64` that is NaN, an infinity, or
/// outside `i64` once rounded" is none.
made<i64> whole_of(f64 value, Rounding rounding, std::optional<i64> nan);

/// OPS-6, `str` to `i64`: "The number the text writes: an integer literal,
/// after one optional `-` ... A number outside `i64` fails". The literal is
/// the language's (LEX-22, LEX-23): decimal digits, `0x` and hexadecimal
/// ones, or `0b` and binary ones, one `_` between two digits at most.
made<i64> integer_of(std::string_view text);

/// OPS-6, `str` to `f64`: "The `f64` nearest the number the text writes, a
/// tie to the even one: a float or an integer literal, after one optional
/// `-` ... One that rounds to an infinity fails ... `"nan"` is a NaN, and
/// `"inf"` and `"-inf"` are the infinities".
made<f64> real_of(std::string_view text);

/// OPS-6, `str` to `bool`: "True for `"true"` and false for `"false"`".
made<bool> flag_of(std::string_view text);

/// OPS-6, OPS-7, `bytes` to `str`: the bytes, where they are well-formed
/// UTF-8.
made<str> decoded(const bytes& value);
} // namespace detail

// ---- Conversions that cannot fail (OPS-6) ----------------------------------

/// TYPE-18, FMT-5: a type's canonical diagnostic spelling.
template <class To = str>
    requires std::same_as<To, str>
To convert_Type(const type_value& value) {
    return value.spelling;
}

/// `convert(value: i64) -> f64`, `-> bool`, `-> str`: the nearest `f64`, a
/// tie to the even one; false for zero and true otherwise; its decimal
/// digits.
template <class To>
To convert_i64(i64 value) {
    if constexpr (std::same_as<To, f64>) {
        return static_cast<f64>(value);
    } else if constexpr (std::same_as<To, bool>) {
        return value != 0;
    } else if constexpr (std::same_as<To, str>) {
        return detail::number_text(value);
    } else {
        static_assert(false, "OPS-4: an i64 converts to an f64, a bool or a str");
    }
}

/// `convert(value: f64) -> bool`, `-> str`: false for either zero and true
/// otherwise, a NaN among them; its shortest text (`detail::number_text`). To an
/// `i64` is `convert_f64_Rounding_i64_`, which can fail.
template <class To>
To convert_f64(f64 value) {
    if constexpr (std::same_as<To, bool>) {
        return value != 0.0;
    } else if constexpr (std::same_as<To, str>) {
        return detail::number_text(value);
    } else {
        static_assert(false,
                      "OPS-4: an f64 converts to a bool or a str here, and to an i64 with its options");
    }
}

/// `convert(value: bool) -> i64`, `-> f64`, `-> str`: 0 or 1; 0.0 or 1.0;
/// `false` or `true`.
template <class To>
To convert_bool(bool value) {
    if constexpr (std::same_as<To, i64>) {
        return value ? 1 : 0;
    } else if constexpr (std::same_as<To, f64>) {
        return value ? 1.0 : 0.0;
    } else if constexpr (std::same_as<To, str>) {
        return value ? str{"true"} : str{"false"};
    } else {
        static_assert(false, "OPS-4: a bool converts to an i64, an f64 or a str");
    }
}

/// `convert(value: date) -> datetime`: "The start of that date, in UTC". The
/// one type it converts to is its template argument's default: the emitter
/// gives none to a candidate whose name no other has.
template <class To = datetime>
To convert_date(date value) {
    static_assert(std::same_as<To, datetime>, "OPS-4: a date converts to a datetime");
    return datetime{std::chrono::sys_days{value}};
}

/// `convert(value: datetime) -> date`: "The date of the instant, in UTC".
template <class To = date>
To convert_datetime(datetime value) {
    static_assert(std::same_as<To, date>, "OPS-4: a datetime converts to a date");
    return date{std::chrono::floor<std::chrono::days>(value)};
}

// ---- Conversions that can fail (OPS-7, OPS-8) ------------------------------

/// `convert<invalid>(value: f64, rounding: Rounding = .truncate, nan: i64? =
/// null)`: the integer `rounding` selects (OPS-8: `.truncate` toward zero,
/// `.floor`, `.ceiling`, and `.half_even`, `.half_up`, `.half_down` to the
/// nearest with a tie to the even integer, away from zero, toward zero),
/// and `nan`, where it is given, for a NaN. NaN otherwise, an infinity, and
/// an integer outside `i64` are invalid.
template <InvalidPolicy invalid = InvalidPolicy::error>
detail::converted<i64, invalid>
convert_f64_Rounding_i64_(f64 value, Rounding rounding = Rounding::truncate,
                          std::optional<i64> nan = std::nullopt,
                          std::source_location where = std::source_location::current()) {
    return detail::outcome<i64, invalid>(detail::whole_of(value, rounding, nan), where);
}

/// `convert(value: str) -> bytes`, which cannot fail, its UTF-8; and
/// `convert<invalid>(value: str)` to `i64`, `f64` or `bool`, which can: the
/// number an integer literal writes; the `f64` nearest the number a float or
/// an integer literal writes, or a NaN or an infinity named; `true` or
/// `false` (OPS-6). The text is all of it: "nothing stands before or after
/// it, a space among them".
template <class To, InvalidPolicy invalid = InvalidPolicy::error>
auto convert_str(const str& value, std::source_location where = std::source_location::current()) {
    if constexpr (std::same_as<To, bytes>) {
        static_assert(invalid == InvalidPolicy::error,
                      "OPS-7: text to bytes cannot fail, and takes no `invalid`");
        return bytes{value};
    } else if constexpr (std::same_as<To, i64>) {
        return detail::outcome<i64, invalid>(detail::integer_of(value), where);
    } else if constexpr (std::same_as<To, f64>) {
        return detail::outcome<f64, invalid>(detail::real_of(value), where);
    } else if constexpr (std::same_as<To, bool>) {
        return detail::outcome<bool, invalid>(detail::flag_of(value), where);
    } else {
        static_assert(false, "OPS-4: a str converts to an i64, an f64, a bool or bytes");
    }
}

/// `convert<invalid>(value: bytes) -> str`: the text the bytes are in UTF-8;
/// bytes that are not well-formed UTF-8 are invalid (OPS-7).
template <class To = str, InvalidPolicy invalid = InvalidPolicy::error>
detail::converted<str, invalid> convert_bytes(const bytes& value,
                                              std::source_location where = std::source_location::current()) {
    static_assert(std::same_as<To, str>, "OPS-4: bytes convert to a str");
    return detail::outcome<str, invalid>(detail::decoded(value), where);
}

/// TYPE-7, OPS-6: an enum converts to the member's name or signed number.
template <class To, class E>
    requires std::is_enum_v<E>
To convert_E(E value) {
    static_assert(std::same_as<To, str> || std::same_as<To, i64>);
    if constexpr (named_enum<E>) {
        const auto enumeration = hgl_enum(value);
        for (std::size_t index = 0; index < enumeration.numbers.size(); ++index) {
            const auto& [member, name] = enumeration.members[index];
            if (member == value) {
                if constexpr (std::same_as<To, str>) {
                    return str{name};
                } else {
                    return enumeration.numbers[index];
                }
            }
        }
    }
    // Every HGL enum value is one of its members (TYPE-7).
    std::unreachable();
}

/// TYPE-7, OPS-7, OPS-8: a number with no member fails under `invalid`.
template <class E, InvalidPolicy invalid>
    requires std::is_enum_v<E>
detail::converted<E, invalid>
convert_i64(i64 value, std::source_location where = std::source_location::current()) {
    if constexpr (named_enum<E>) {
        const auto enumeration = hgl_enum(E{});
        for (std::size_t index = 0; index < enumeration.numbers.size(); ++index) {
            if (enumeration.numbers[index] == value) {
                return enumeration.members[index].first;
            }
        }
    }
    return detail::outcome<E, invalid>(
        std::unexpected(detail::conversion_reason{"no enum member has that number"}), where);
}

} // namespace hglrt
