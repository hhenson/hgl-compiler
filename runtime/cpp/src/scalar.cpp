// Core's native scalar operators of numbers and of the ordered types
// (scalar.h).
#include "numbers.h"

#include <hglrt/scalar.h>

#include <array>
#include <bit>
#include <cmath>
#include <compare>
#include <cstddef>
#include <cstdint>
#include <limits>
#include <stdexcept>
#include <string>

namespace hglrt {

namespace {

using detail::Magnitude;
using detail::Number;
using detail::number_of;
using detail::u128;

constexpr i64 least_i64 = std::numeric_limits<i64>::min();

[[noreturn]] void out_of_range(std::source_location where) {
    fail(codes::out_of_range, "an i64 result is outside the signed 64-bit range", where);
}

// ---- Exact arithmetic for numeric closeness (NUM-4, NUM-5) ---------------
//
// Closeness is `abs(a - b) <= max(abs, rel * max(abs(a), abs(b)))` of the
// operands' exact values. An f64 is an integer times a power of two, and so
// is an i64; the difference of two of them and the product of a tolerance
// and one of them are too, with more bits than any built-in type holds: the
// difference of `1e308` and `1e-308` has about 2,100. They are held here as
// such integers, in `Wide`, and compared exactly. NUM-5: "The finite test
// must not admit an incorrect result through overflow of intermediate
// subtraction or scaling."

/// An unsigned integer of 4,096 bits, little-endian in 64-bit digits.
/// Every value it holds here is far below its limit (see `at_most`).
class Wide {
  public:
    static constexpr std::size_t digits = 64;

    /// `value` times two to the `shift`.
    static Wide shifted(u128 value, std::size_t shift) {
        Wide out;
        const std::size_t word = shift / 64;
        const unsigned bit = static_cast<unsigned>(shift % 64);
        if (word + 2 >= digits) {
            throw std::logic_error("hglrt: an exact quantity outside its bounds");
        }
        const auto low = static_cast<std::uint64_t>(value);
        const auto high = static_cast<std::uint64_t>(value >> 64);
        if (bit == 0) {
            out.digits_[word] = low;
            out.digits_[word + 1] = high;
        } else {
            out.digits_[word] = low << bit;
            out.digits_[word + 1] = (low >> (64 - bit)) | (high << bit);
            out.digits_[word + 2] = high >> (64 - bit);
        }
        return out;
    }

    [[nodiscard]] std::size_t bit_width() const {
        for (std::size_t index = digits; index-- > 0;) {
            if (digits_[index] != 0) {
                return index * 64 + static_cast<std::size_t>(std::bit_width(digits_[index]));
            }
        }
        return 0;
    }

    /// Multiplies by two to the `shift`, which keeps it within bounds.
    void shift_left(std::size_t shift) {
        if (bit_width() + shift >= digits * 64) {
            throw std::logic_error("hglrt: an exact quantity outside its bounds");
        }
        const std::size_t word = shift / 64;
        const unsigned bit = static_cast<unsigned>(shift % 64);
        for (std::size_t index = digits; index-- > 0;) {
            std::uint64_t moved = 0;
            if (index >= word) {
                moved = digits_[index - word] << bit;
                if (bit != 0 && index > word) {
                    moved |= digits_[index - word - 1] >> (64 - bit);
                }
            }
            digits_[index] = moved;
        }
    }

    Wide& operator+=(const Wide& other) {
        std::uint64_t carry = 0;
        for (std::size_t index = 0; index < digits; ++index) {
            const u128 sum = static_cast<u128>(digits_[index]) + other.digits_[index] + carry;
            digits_[index] = static_cast<std::uint64_t>(sum);
            carry = static_cast<std::uint64_t>(sum >> 64);
        }
        return *this;
    }

    /// Subtracts `other`, which is not greater.
    Wide& operator-=(const Wide& other) {
        std::uint64_t borrow = 0;
        for (std::size_t index = 0; index < digits; ++index) {
            const std::uint64_t subtrahend = other.digits_[index] + borrow;
            // A borrow out where the subtrahend wrapped or exceeds the digit.
            const bool out = (borrow != 0 && subtrahend == 0) || digits_[index] < subtrahend;
            digits_[index] -= subtrahend;
            borrow = out ? 1 : 0;
        }
        return *this;
    }

    friend std::strong_ordering operator<=>(const Wide& lhs, const Wide& rhs) {
        for (std::size_t index = digits; index-- > 0;) {
            if (lhs.digits_[index] != rhs.digits_[index]) {
                return lhs.digits_[index] <=> rhs.digits_[index];
            }
        }
        return std::strong_ordering::equal;
    }

  private:
    std::array<std::uint64_t, digits> digits_{};
};

/// A nonnegative exact quantity, `value` times two to the `exponent`.
struct Exact {
    Wide value;
    int exponent = 0;
};

/// `|lhs - rhs|`, exactly.
Exact distance(const Number& lhs, const Number& rhs) {
    const int lowest = std::min(lhs.magnitude.exponent, rhs.magnitude.exponent);
    Wide left =
        Wide::shifted(lhs.magnitude.mantissa, static_cast<std::size_t>(lhs.magnitude.exponent - lowest));
    Wide right =
        Wide::shifted(rhs.magnitude.mantissa, static_cast<std::size_t>(rhs.magnitude.exponent - lowest));
    if (lhs.negative != rhs.negative) {
        left += right;
        return Exact{left, lowest};
    }
    if (left >= right) {
        left -= right;
        return Exact{left, lowest};
    }
    right -= left;
    return Exact{right, lowest};
}

/// `lhs * rhs`, exactly.
Exact product(const Magnitude& lhs, const Magnitude& rhs) {
    return Exact{Wide::shifted(static_cast<u128>(lhs.mantissa) * rhs.mantissa, 0),
                 lhs.exponent + rhs.exponent};
}

/// Whether `lhs <= rhs`. Each is below 2^2049 and above 2^-2149 where not
/// zero, so their highest bits are compared first, and where those are at
/// one place the two aligned take fewer than 3,200 bits.
bool at_most(Exact lhs, Exact rhs) {
    const std::size_t left_width = lhs.value.bit_width();
    const std::size_t right_width = rhs.value.bit_width();
    if (left_width == 0) {
        return true;
    }
    if (right_width == 0) {
        return false;
    }
    const long left_top = static_cast<long>(left_width) + lhs.exponent;
    const long right_top = static_cast<long>(right_width) + rhs.exponent;
    if (left_top != right_top) {
        return left_top < right_top;
    }
    if (lhs.exponent > rhs.exponent) {
        lhs.value.shift_left(static_cast<std::size_t>(lhs.exponent - rhs.exponent));
    } else {
        rhs.value.shift_left(static_cast<std::size_t>(rhs.exponent - lhs.exponent));
    }
    return lhs.value <= rhs.value;
}

/// NUM-4: a negative or non-finite tolerance fails with `numeric.invalid_tolerance`.
void check_tolerances(f64 abs, f64 rel, std::source_location where) {
    if (!std::isfinite(abs) || abs < 0.0) {
        fail(codes::invalid_tolerance, "the tolerance `abs` is not finite and nonnegative", where);
    }
    if (!std::isfinite(rel) || rel < 0.0) {
        fail(codes::invalid_tolerance, "the tolerance `rel` is not finite and nonnegative", where);
    }
}

/// Whether the finite `lhs` and `rhs` are close under the tolerances `abs`
/// and `rel` (NUM-4), exactly: `abs(lhs - rhs)` is at most `abs`, or at most
/// `rel` times the magnitude of either, which is at most `rel` times the
/// greater.
bool close(const Number& lhs, const Number& rhs, f64 abs, f64 rel) {
    const Exact apart = distance(lhs, rhs);
    const Magnitude absolute = number_of(abs).magnitude;
    const Magnitude relative = number_of(rel).magnitude;
    return at_most(apart, Exact{Wide::shifted(absolute.mantissa, 0), absolute.exponent}) ||
           at_most(apart, product(relative, lhs.magnitude)) ||
           at_most(apart, product(relative, rhs.magnitude));
}

/// How the `i64` `lhs` compares with the `f64` `rhs` by their exact values
/// (NUM-2: "Mixed numeric comparison must not first round an i64 operand to
/// f64"); unordered for a NaN.
std::partial_ordering compare_exactly(i64 lhs, f64 rhs) {
    if (std::isnan(rhs)) {
        return std::partial_ordering::unordered;
    }
    if (rhs >= detail::i64_upper_bound) {
        return std::partial_ordering::less;
    }
    if (rhs < -detail::i64_upper_bound) {
        return std::partial_ordering::greater;
    }
    // `rhs` is now in [-2^63, 2^63), so its whole part is an i64.
    const f64 whole = std::trunc(rhs);
    const auto whole_i64 = static_cast<i64>(whole);
    if (lhs != whole_i64) {
        return lhs < whole_i64 ? std::partial_ordering::less : std::partial_ordering::greater;
    }
    const f64 fraction = rhs - whole;
    if (fraction > 0.0) {
        return std::partial_ordering::less;
    }
    if (fraction < 0.0) {
        return std::partial_ordering::greater;
    }
    return std::partial_ordering::equivalent;
}

} // namespace

// ---- i64 arithmetic ------------------------------------------------------

i64 add__i64_i64(i64 lhs, i64 rhs, std::source_location where) {
    return detail::in_range(detail::checked_add(lhs, rhs), out_of_range, where);
}

i64 sub__i64_i64(i64 lhs, i64 rhs, std::source_location where) {
    return detail::in_range(detail::checked_sub(lhs, rhs), out_of_range, where);
}

i64 mul__i64_i64(i64 lhs, i64 rhs, std::source_location where) {
    return detail::in_range(detail::checked_mul(lhs, rhs), out_of_range, where);
}

i64 neg__i64(i64 value, std::source_location where) {
    return detail::in_range(detail::checked_neg(value), out_of_range, where);
}

// ---- Division --------------------------------------------------------------

error detail::division_by_zero() {
    return make_error("a division by zero", std::string{codes::division_by_zero});
}

void detail::fail_division_by_zero(std::source_location where) {
    raise(division_by_zero(), where);
}

f64 detail::divide(f64 lhs, f64 rhs, ZeroPolicy zero, std::source_location where) {
    if (rhs != 0.0) {
        return lhs / rhs;
    }
    switch (zero) {
    case ZeroPolicy::fail:
        fail_division_by_zero(where);
    case ZeroPolicy::ieee:
        // NUM-3: "zero divided by zero produces NaN"; the sign of an
        // infinity "combines both operand signs".
        if (lhs == 0.0 || std::isnan(lhs)) {
            return std::numeric_limits<f64>::quiet_NaN();
        }
        return std::copysign(std::numeric_limits<f64>::infinity(),
                             std::copysign(1.0, lhs) * std::copysign(1.0, rhs));
    case ZeroPolicy::nan:
        return std::numeric_limits<f64>::quiet_NaN();
    case ZeroPolicy::error:
        break;
    }
    // A division under `.error` gives a result, which its candidate makes.
    throw std::logic_error("hglrt: a division under .error has no plain quotient");
}

f64 detail::quotient(i64 lhs, i64 rhs) {
    const bool negative = (lhs < 0) != (rhs < 0);
    const std::uint64_t numerator = number_of(lhs).magnitude.mantissa;
    const std::uint64_t denominator = number_of(rhs).magnitude.mantissa;
    if (numerator == 0) {
        return negative ? -0.0 : 0.0;
    }
    // The quotient scaled by 2^scale has 55 or 56 bits before the point:
    // two or three more than an f64 holds. With the remainder's being
    // nonzero folded into the last of them, converting it rounds once, to
    // the nearest f64 of the exact quotient, ties to even.
    const int scale = 55 - std::bit_width(numerator) + std::bit_width(denominator);
    const u128 scaled = static_cast<u128>(numerator) << (scale > 0 ? scale : 0);
    const u128 divisor = static_cast<u128>(denominator) << (scale < 0 ? -scale : 0);
    const u128 whole = scaled / divisor;
    const u128 sticky = scaled % divisor != 0 ? 1 : 0;
    const f64 magnitude = std::ldexp(static_cast<f64>(whole | sticky), -scale);
    return negative ? -magnitude : magnitude;
}

i64 detail::floor_quotient(i64 lhs, i64 rhs, std::source_location where) {
    if (lhs == least_i64 && rhs == -1) {
        out_of_range(where);
    }
    i64 whole = lhs / rhs;
    if (lhs % rhs != 0 && ((lhs < 0) != (rhs < 0))) {
        --whole;
    }
    return whole;
}

i64 detail::floor_remainder(i64 lhs, i64 rhs) {
    // C++'s `%` of the least i64 by -1 is undefined; the remainder is 0.
    if (rhs == -1) {
        return 0;
    }
    i64 remainder = lhs % rhs;
    if (remainder != 0 && ((remainder < 0) != (rhs < 0))) {
        remainder += rhs;
    }
    return remainder;
}

f64 detail::floor_quotient(f64 lhs, f64 rhs) {
    // NUM-1: "Where an f64 operand is an infinity or NaN and the divisor is
    // not zero, `//` is the floor of the IEEE quotient": an infinity or a NaN
    // is its own floor, and a finite one here is a zero, a finite numerator
    // over an infinity, whose floor keeps its sign.
    if (!std::isfinite(lhs) || !std::isfinite(rhs)) {
        const f64 quotient = lhs / rhs;
        if (!std::isfinite(quotient)) {
            return quotient;
        }
        return std::copysign(std::floor(quotient), quotient);
    }
    // The floor of the exact quotient of two finite numbers, as CPython's
    // `float_floor_div` computes it:
    // the remainder `fmod` gives is exact, so `lhs - remainder` is a
    // multiple of `rhs`, and the quotient of the two is the floor or one
    // above it.
    f64 remainder = std::fmod(lhs, rhs);
    f64 whole = (lhs - remainder) / rhs;
    if (remainder != 0.0 && ((rhs < 0.0) != (remainder < 0.0))) {
        whole -= 1.0;
    }
    if (whole == 0.0) {
        return std::copysign(0.0, lhs / rhs);
    }
    f64 floored = std::floor(whole);
    if (whole - floored > 0.5) {
        floored += 1.0;
    }
    return floored;
}

f64 detail::floor_remainder(f64 lhs, f64 rhs) {
    // NUM-1: "`%` what IEEE `fmod` gives: a remainder by an infinity is the
    // numerator as it is, whatever its sign"; of an infinite numerator, and
    // with a NaN operand, it is a NaN.
    if (!std::isfinite(lhs) || !std::isfinite(rhs)) {
        if (std::isnan(lhs) || std::isnan(rhs) || std::isinf(lhs)) {
            return std::numeric_limits<f64>::quiet_NaN();
        }
        return lhs;
    }
    // "modulo has the divisor's sign when nonzero. So do `//` and `%` of
    // finite f64 operands": a zero remainder is the zero of the divisor's
    // sign, as CPython's `float_rem` has it.
    f64 remainder = std::fmod(lhs, rhs);
    if (remainder == 0.0) {
        return std::copysign(0.0, rhs);
    }
    if ((rhs < 0.0) != (remainder < 0.0)) {
        remainder += rhs;
    }
    return remainder;
}

// ---- f64 arithmetic --------------------------------------------------------

f64 add__f64_f64(f64 lhs, f64 rhs) {
    return lhs + rhs;
}

f64 sub__f64_f64(f64 lhs, f64 rhs) {
    return lhs - rhs;
}

f64 mul__f64_f64(f64 lhs, f64 rhs) {
    return lhs * rhs;
}

f64 neg__f64(f64 value) {
    return -value;
}

// ---- Mixed arithmetic ------------------------------------------------------

f64 add__i64_f64(i64 lhs, f64 rhs) {
    return static_cast<f64>(lhs) + rhs;
}

f64 sub__i64_f64(i64 lhs, f64 rhs) {
    return static_cast<f64>(lhs) - rhs;
}

f64 sub__f64_i64(f64 lhs, i64 rhs) {
    return lhs - static_cast<f64>(rhs);
}

f64 mul__i64_f64(i64 lhs, f64 rhs) {
    return static_cast<f64>(lhs) * rhs;
}

// ---- Numeric equality ------------------------------------------------------

bool eq__i64_i64_f64_f64(i64 lhs, i64 rhs, f64 abs, f64 rel, std::source_location where) {
    check_tolerances(abs, rel, where);
    if (abs == 0.0 && rel == 0.0) {
        return lhs == rhs;
    }
    return close(number_of(lhs), number_of(rhs), abs, rel);
}

bool ne__i64_i64_f64_f64(i64 lhs, i64 rhs, f64 abs, f64 rel, std::source_location where) {
    return !eq__i64_i64_f64_f64(lhs, rhs, abs, rel, where);
}

bool eq__f64_f64_f64_f64(f64 lhs, f64 rhs, f64 abs, f64 rel, std::source_location where) {
    check_tolerances(abs, rel, where);
    // IEEE `==` has NaN equal to nothing, an infinity equal to itself alone,
    // and the two zeros equal (NUM-5).
    if (abs == 0.0 && rel == 0.0) {
        return lhs == rhs;
    }
    // NUM-5: "Apply these cases before the finite closeness rule."
    if (std::isnan(lhs) || std::isnan(rhs)) {
        return false;
    }
    if (std::isinf(lhs) || std::isinf(rhs)) {
        return lhs == rhs;
    }
    return close(number_of(lhs), number_of(rhs), abs, rel);
}

bool ne__f64_f64_f64_f64(f64 lhs, f64 rhs, f64 abs, f64 rel, std::source_location where) {
    return !eq__f64_f64_f64_f64(lhs, rhs, abs, rel, where);
}

bool eq__i64_f64_f64_f64(i64 lhs, f64 rhs, f64 abs, f64 rel, std::source_location where) {
    check_tolerances(abs, rel, where);
    if (abs == 0.0 && rel == 0.0) {
        return compare_exactly(lhs, rhs) == std::partial_ordering::equivalent;
    }
    // An i64 is close to no NaN and to no infinity (NUM-5).
    if (!std::isfinite(rhs)) {
        return false;
    }
    return close(number_of(lhs), number_of(rhs), abs, rel);
}

bool ne__i64_f64_f64_f64(i64 lhs, f64 rhs, f64 abs, f64 rel, std::source_location where) {
    return !eq__i64_f64_f64_f64(lhs, rhs, abs, rel, where);
}

// ---- Order and comparison of an i64 and an f64 --------------------------------

namespace {

/// What `<=>` gives for the order `order` (NUM-6).
CmpResult outcome_of(std::partial_ordering order) {
    if (order == std::partial_ordering::less) {
        return CmpResult::less;
    }
    if (order == std::partial_ordering::greater) {
        return CmpResult::greater;
    }
    if (order == std::partial_ordering::equivalent) {
        return CmpResult::equivalent;
    }
    return CmpResult::unordered;
}

/// How the `f64` `lhs` compares with the `i64` `rhs`, exactly.
std::partial_ordering compare_exactly(f64 lhs, i64 rhs) {
    return 0 <=> compare_exactly(rhs, lhs);
}

} // namespace

bool lt__i64_f64(i64 lhs, f64 rhs) {
    return compare_exactly(lhs, rhs) < 0;
}

bool lt__f64_i64(f64 lhs, i64 rhs) {
    return compare_exactly(lhs, rhs) < 0;
}

bool le__i64_f64(i64 lhs, f64 rhs) {
    return compare_exactly(lhs, rhs) <= 0;
}

bool le__f64_i64(f64 lhs, i64 rhs) {
    return compare_exactly(lhs, rhs) <= 0;
}

bool gt__i64_f64(i64 lhs, f64 rhs) {
    return compare_exactly(lhs, rhs) > 0;
}

bool gt__f64_i64(f64 lhs, i64 rhs) {
    return compare_exactly(lhs, rhs) > 0;
}

bool ge__i64_f64(i64 lhs, f64 rhs) {
    return compare_exactly(lhs, rhs) >= 0;
}

bool ge__f64_i64(f64 lhs, i64 rhs) {
    return compare_exactly(lhs, rhs) >= 0;
}

CmpResult compare__i64_f64(i64 lhs, f64 rhs) {
    return outcome_of(compare_exactly(lhs, rhs));
}

CmpResult compare__f64_i64(f64 lhs, i64 rhs) {
    return outcome_of(compare_exactly(lhs, rhs));
}

// ---- Boolean forms ---------------------------------------------------------

bool and__bool_bool(bool lhs, bool rhs) {
    return lhs && rhs;
}

bool or__bool_bool(bool lhs, bool rhs) {
    return lhs || rhs;
}

bool not__bool(bool value) {
    return !value;
}

} // namespace hglrt
