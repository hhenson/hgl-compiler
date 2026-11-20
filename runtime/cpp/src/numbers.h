// Numbers as exact quantities: an `i64` or a finite `f64` as a sign and an
// integer times a power of two, so that arithmetic on them can be made
// exact before it is rounded once (NUM-2, NUM-5, OPS-10); and the checked
// `i64` arithmetic every count of the library is made with. Private to
// libhglrt.
#pragma once

#include <hglrt/values.h>

#include <bit>
#include <cmath>
#include <cstddef>
#include <cstdint>
#include <limits>
#include <optional>

namespace hglrt::detail {

// ---- Checked i64 arithmetic ------------------------------------------------
//
// The sum, difference, product and negation of `i64`, each none where the
// exact result is outside the signed 64-bit range (NUM-2: "An
// unrepresentable i64 result produces an HGL error with code
// `numeric.out_of_range`; it neither wraps nor silently promotes"): the
// integers, the durations' microseconds and the tenors' counts are each
// checked through these. GCC's and clang's overflow builtins are used here
// and nowhere else, so that a compiler without them changes this file
// alone.

/// `lhs + rhs`, where it is an `i64`.
inline std::optional<i64> checked_add(i64 lhs, i64 rhs) noexcept {
    i64 out = 0;
    if (__builtin_add_overflow(lhs, rhs, &out)) {
        return std::nullopt;
    }
    return out;
}

/// `lhs - rhs`, where it is an `i64`.
inline std::optional<i64> checked_sub(i64 lhs, i64 rhs) noexcept {
    i64 out = 0;
    if (__builtin_sub_overflow(lhs, rhs, &out)) {
        return std::nullopt;
    }
    return out;
}

/// `lhs * rhs`, where it is an `i64`.
inline std::optional<i64> checked_mul(i64 lhs, i64 rhs) noexcept {
    i64 out = 0;
    if (__builtin_mul_overflow(lhs, rhs, &out)) {
        return std::nullopt;
    }
    return out;
}

/// `-value`, where it is an `i64`: that of the least is not.
inline std::optional<i64> checked_neg(i64 value) noexcept {
    if (value == std::numeric_limits<i64>::min()) {
        return std::nullopt;
    }
    return -value;
}

// ---- Exact quantities --------------------------------------------------------

// An unsigned 128-bit integer, GCC's: the product of two 64-bit integers.
// `__extension__` keeps `-Wpedantic` quiet about a type ISO C++ lacks.
__extension__ typedef unsigned __int128 u128;
__extension__ typedef __int128 i128;

/// A finite number's magnitude, `mantissa` times two to the `exponent`.
struct Magnitude {
    std::uint64_t mantissa = 0;
    int exponent = 0;
};

/// A finite number: its sign and magnitude.
struct Number {
    bool negative = false;
    Magnitude magnitude;
};

/// `value` exactly. The least i64's magnitude, 2^63, is a 64-bit unsigned
/// integer.
inline Number number_of(i64 value) {
    const auto bits = static_cast<std::uint64_t>(value);
    return Number{value < 0, Magnitude{value < 0 ? 0 - bits : bits, 0}};
}

/// `value`, which is finite, exactly: zero has a mantissa of zero.
inline Number number_of(f64 value) {
    if (value == 0.0) {
        return Number{std::signbit(value), Magnitude{}};
    }
    int exponent = 0;
    // `fraction` is in [0.5, 1) and has at most 53 significant bits, so it
    // times 2^53 is an integer, a subnormal's as well.
    const f64 fraction = std::frexp(std::fabs(value), &exponent);
    const auto mantissa = static_cast<std::uint64_t>(std::ldexp(fraction, 53));
    return Number{value < 0.0, Magnitude{mantissa, exponent - 53}};
}

/// The bits `value` takes: none for zero.
inline int width_of(u128 value) {
    const auto high = static_cast<std::uint64_t>(value >> 64);
    if (high != 0) {
        return 64 + std::bit_width(high);
    }
    return std::bit_width(static_cast<std::uint64_t>(value));
}

/// `numerator / denominator`, `denominator` not zero, rounded to the
/// nearest integer, a tie to the even one (OPS-10).
inline u128 rounded_quotient(u128 numerator, u128 denominator) {
    u128 whole = numerator / denominator;
    const u128 left = numerator % denominator;
    // `left` against half the denominator, without halving it.
    const u128 twice = left * 2;
    if (twice > denominator || (twice == denominator && (whole & 1) != 0)) {
        ++whole;
    }
    return whole;
}

/// `value` divided by two to the `shift`, rounded to the nearest integer, a
/// tie to the even one.
inline u128 rounded_shift(u128 value, std::size_t shift) {
    if (shift == 0) {
        return value;
    }
    if (shift >= 128) {
        // `value` is below 2^128, so the quotient is below one half.
        return 0;
    }
    const u128 whole = value >> shift;
    const u128 left = value & ((u128{1} << shift) - 1);
    const u128 half = u128{1} << (shift - 1);
    if (left > half || (left == half && (whole & 1) != 0)) {
        return whole + 1;
    }
    return whole;
}

/// Whether an `i64` has the sign `negative` and the magnitude `magnitude`:
/// the magnitude of the least is 2^63.
inline bool fits_i64(bool negative, u128 magnitude) {
    const u128 most = (u128{1} << 63) - (negative ? 0 : 1);
    return magnitude <= most;
}

/// The `i64` of sign `negative` and magnitude `magnitude`, which
/// `fits_i64`.
inline i64 signed_of(bool negative, u128 magnitude) {
    const auto bits = static_cast<std::uint64_t>(magnitude);
    return static_cast<i64>(negative ? 0 - bits : bits);
}

} // namespace hglrt::detail
