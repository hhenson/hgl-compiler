// Core's native scalar operators (`hgl/native/scalar_operators.hgl`), the ones
// core's scalar cases reach: checked `i64` arithmetic, `f64` arithmetic and
// division, an instant moved by a duration, numeric equality with its
// tolerances, order, and the Boolean forms.
//
// Each is a function named as the Python target names the candidate it is:
// the operator's name, then the type of each parameter, `add__i64_i64` for
// `native impl const fn add_(lhs: i64, rhs: i64) -> i64`. A candidate with a
// generic type parameter is a function template, `lt__T_T<T>`, which the
// generated code calls with the type it instances, and one with a generic
// value parameter, the zero policy of a division, takes it as a template
// argument. One that can fail takes the place it is called from last, which
// its failure carries (failure.h); the generated code leaves it to its
// default.
#pragma once

#include <hglrt/failure.h>
#include <hglrt/values.h>

#include <concepts>
#include <source_location>
#include <type_traits>

namespace hglrt {

// ---- i64 arithmetic (NUM-1, NUM-2) ---------------------------------------
//
// NUM-2: "An unrepresentable i64 result produces an HGL error with code
// `numeric.out_of_range`; it neither wraps nor silently promotes."

/// `add_(lhs: i64, rhs: i64) -> i64`: the sum, checked (NUM-2).
i64 add__i64_i64(i64 lhs, i64 rhs, std::source_location where = std::source_location::current());

/// `sub_(lhs: i64, rhs: i64) -> i64`: the difference, checked (NUM-2).
i64 sub__i64_i64(i64 lhs, i64 rhs, std::source_location where = std::source_location::current());

/// `mul_(lhs: i64, rhs: i64) -> i64`: the product, checked (NUM-2).
i64 mul__i64_i64(i64 lhs, i64 rhs, std::source_location where = std::source_location::current());

/// `neg_(value: i64) -> i64`: the negation, checked: that of the least `i64`
/// is out of range (NUM-1, NUM-2).
i64 neg__i64(i64 value, std::source_location where = std::source_location::current());

// ---- Division and its zero policy (NUM-3, NUM-9, NUM-12) -----------------

/// The members of core's enum `ZeroPolicy` (NUM-3), which a division's
/// `zero` names. A call gives `zero` as a compile-time constant (NUM-9), and
/// the generated code passes the member it names as the template argument.
enum class ZeroPolicy { ieee, nan, error, fail };

namespace detail {
/// What a division whose plain result is `T` gives under `zero`: under
/// `.error`, `result<T, error>` (NUM-9), and otherwise `T`.
template <class T, ZeroPolicy zero>
using divided = std::conditional_t<zero == ZeroPolicy::error, result<T>, T>;

/// The error of a zero divisor under `.error` (NUM-9: "either signed zero
/// divisor produces `err(...)` carrying error code
/// `numeric.division_by_zero`").
error division_by_zero();

/// `lhs / rhs` of two `f64` under `zero`, which is not `.error`.
f64 divide(f64 lhs, f64 rhs, ZeroPolicy zero, std::source_location where);

/// `lhs / rhs` of two `i64`, `rhs` not zero: the `f64` nearest the exact
/// quotient.
f64 quotient(i64 lhs, i64 rhs);

/// `lhs // rhs` of two `i64`, `rhs` not zero, checked (NUM-1, NUM-2).
i64 floor_quotient(i64 lhs, i64 rhs, std::source_location where);

/// `lhs % rhs` of two `i64`, `rhs` not zero (NUM-1).
i64 floor_remainder(i64 lhs, i64 rhs);

/// Fails with `numeric.division_by_zero` (NUM-3: "The `fail` policy makes a
/// zero divisor an HGL error with code `numeric.division_by_zero`").
[[noreturn]] void fail_division_by_zero(std::source_location where);
} // namespace detail

/// `div_<zero>(lhs: f64, rhs: f64)`: true division (NUM-1). A zero divisor
/// gives what `zero` says (NUM-3): under `.ieee`, the default, "a finite
/// nonzero numerator divided by signed zero produces signed infinity; zero
/// divided by zero produces NaN"; under `.nan` a NaN; under `.fail` a failure
/// coded `numeric.division_by_zero`; and under `.error` an `err` coded so,
/// every other divisor giving `ok` of the quotient (NUM-9).
template <ZeroPolicy zero = ZeroPolicy::ieee>
detail::divided<f64, zero> div__f64_f64(f64 lhs, f64 rhs,
                                        std::source_location where = std::source_location::current()) {
    if constexpr (zero == ZeroPolicy::error) {
        if (rhs == 0.0) {
            return std::unexpected(detail::division_by_zero());
        }
        return detail::divide(lhs, rhs, ZeroPolicy::fail, where);
    } else {
        return detail::divide(lhs, rhs, zero, where);
    }
}

/// `div_<zero>(lhs: i64, rhs: i64)`: true division of two `i64`, an `f64`
/// (NUM-1: "`/` defaults to f64"), the binary64 nearest the exact quotient
/// (NUM-2). A zero divisor is as for `div__f64_f64`.
template <ZeroPolicy zero = ZeroPolicy::ieee>
detail::divided<f64, zero> div__i64_i64(i64 lhs, i64 rhs,
                                        std::source_location where = std::source_location::current()) {
    if (rhs == 0) {
        if constexpr (zero == ZeroPolicy::error) {
            return std::unexpected(detail::division_by_zero());
        } else {
            return detail::divide(static_cast<f64>(lhs), 0.0, zero, where);
        }
    }
    return detail::quotient(lhs, rhs);
}

/// `floordiv_<zero>(lhs: i64, rhs: i64)`: the quotient rounded toward
/// negative infinity (NUM-1), checked (NUM-2: the least `i64` by `-1` is out
/// of range). It admits `.fail`, the default, and `.error` (NUM-12: its
/// result holds no infinity or NaN).
template <ZeroPolicy zero = ZeroPolicy::fail>
detail::divided<i64, zero> floordiv__i64_i64(i64 lhs, i64 rhs,
                                             std::source_location where = std::source_location::current()) {
    static_assert(zero == ZeroPolicy::fail || zero == ZeroPolicy::error,
                  "NUM-12: an i64 floor division admits only .fail and .error");
    if (rhs == 0) {
        if constexpr (zero == ZeroPolicy::error) {
            return std::unexpected(detail::division_by_zero());
        } else {
            detail::fail_division_by_zero(where);
        }
    }
    return detail::floor_quotient(lhs, rhs, where);
}

/// `mod_<zero>(lhs: i64, rhs: i64)`: the remainder that has the divisor's
/// sign when it is not zero (NUM-1). It admits `.fail`, the default, and
/// `.error` (NUM-12).
template <ZeroPolicy zero = ZeroPolicy::fail>
detail::divided<i64, zero> mod__i64_i64(i64 lhs, i64 rhs,
                                        std::source_location where = std::source_location::current()) {
    static_assert(zero == ZeroPolicy::fail || zero == ZeroPolicy::error,
                  "NUM-12: an i64 modulo admits only .fail and .error");
    if (rhs == 0) {
        if constexpr (zero == ZeroPolicy::error) {
            return std::unexpected(detail::division_by_zero());
        } else {
            detail::fail_division_by_zero(where);
        }
    }
    return detail::floor_remainder(lhs, rhs);
}

// ---- f64 arithmetic (NUM-1, NUM-2) ---------------------------------------
//
// NUM-2: "Floating arithmetic rounds to binary64 and preserves IEEE special
// values, including signed zero and overflow infinity." None fails.

/// `add_(lhs: f64, rhs: f64) -> f64`: the IEEE sum.
f64 add__f64_f64(f64 lhs, f64 rhs);

/// `sub_(lhs: f64, rhs: f64) -> f64`: the IEEE difference.
f64 sub__f64_f64(f64 lhs, f64 rhs);

/// `mul_(lhs: f64, rhs: f64) -> f64`: the IEEE product.
f64 mul__f64_f64(f64 lhs, f64 rhs);

/// `neg_(value: f64) -> f64`: the IEEE negation, which keeps NaN and turns
/// the sign of a zero (NUM-1).
f64 neg__f64(f64 value);

// ---- Instants (OPS-1, OPS-10) --------------------------------------------

/// `add_(lhs: datetime, rhs: duration) -> datetime`: the instant `rhs` after
/// `lhs`, exact to the microsecond and checked: one outside the years 1 to
/// 9999 (TYPE-2) is out of range (OPS-10: a result "outside its type's range
/// is an HGL error with code `numeric.out_of_range`").
datetime add__datetime_duration(datetime lhs, duration rhs,
                                std::source_location where = std::source_location::current());

// ---- Numeric equality (NUM-2, NUM-4, NUM-5, OPS-2) -----------------------
//
// NUM-4: "Plain numeric `==` is exact and `!=` is its complement. Configured
// `==` accepts absolute (`abs`) and relative (`rel`) tolerance, each
// defaulting to zero", and for finite operands closeness is
// `abs(a - b) <= max(abs, rel * max(abs(a), abs(b)))`, decided here exactly.
// A tolerance that is not finite and nonnegative fails, with no code.
// NUM-5: NaN is close to nothing, an infinity only to the same infinity, and
// the two zeros are equal. NUM-2: an `i64` is never rounded to an `f64` to be
// compared with one.

/// `eq_(lhs: i64, rhs: i64, abs: f64 = 0.0, rel: f64 = 0.0) -> bool`.
bool eq__i64_i64_f64_f64(i64 lhs, i64 rhs, f64 abs = 0.0, f64 rel = 0.0,
                         std::source_location where = std::source_location::current());

/// `ne_(lhs: i64, rhs: i64, abs: f64 = 0.0, rel: f64 = 0.0) -> bool`: the
/// complement of `eq_` under the same options.
bool ne__i64_i64_f64_f64(i64 lhs, i64 rhs, f64 abs = 0.0, f64 rel = 0.0,
                         std::source_location where = std::source_location::current());

/// `eq_(lhs: f64, rhs: f64, abs: f64 = 0.0, rel: f64 = 0.0) -> bool`.
bool eq__f64_f64_f64_f64(f64 lhs, f64 rhs, f64 abs = 0.0, f64 rel = 0.0,
                         std::source_location where = std::source_location::current());

/// `ne_(lhs: f64, rhs: f64, abs: f64 = 0.0, rel: f64 = 0.0) -> bool`.
bool ne__f64_f64_f64_f64(f64 lhs, f64 rhs, f64 abs = 0.0, f64 rel = 0.0,
                         std::source_location where = std::source_location::current());

/// `eq_(lhs: i64, rhs: f64, abs: f64 = 0.0, rel: f64 = 0.0) -> bool`.
bool eq__i64_f64_f64_f64(i64 lhs, f64 rhs, f64 abs = 0.0, f64 rel = 0.0,
                         std::source_location where = std::source_location::current());

/// `ne_(lhs: i64, rhs: f64, abs: f64 = 0.0, rel: f64 = 0.0) -> bool`.
bool ne__i64_f64_f64_f64(i64 lhs, f64 rhs, f64 abs = 0.0, f64 rel = 0.0,
                         std::source_location where = std::source_location::current());

// ---- Equality and order of one type (OPS-2, NUM-6) -----------------------

/// `eq_<T>(lhs: T, rhs: T) -> bool`: the language's equality of two values
/// of one type (OPS-2), which is C++'s `==` of what the target writes: a
/// NaN is equal to nothing, a list is equal position by position, and a
/// struct the compiler writes is equal field by field.
template <std::equality_comparable T>
bool eq__T_T(const T& lhs, const T& rhs) {
    return lhs == rhs;
}

/// `ne_<T>(lhs: T, rhs: T) -> bool`: the complement of `eq_<T>`.
template <std::equality_comparable T>
bool ne__T_T(const T& lhs, const T& rhs) {
    return !(lhs == rhs);
}

/// The types core's order candidates admit (`requires T in {i64, f64, str,
/// bool, date, time, datetime, civil_datetime, duration}`), those of them
/// this library has yet. `str` is ordered by its bytes, which is the order of
/// its code points; `false` is before `true`.
template <class T>
concept ordered = std::same_as<T, i64> || std::same_as<T, f64> || std::same_as<T, str> ||
                  std::same_as<T, bool> || std::same_as<T, hgraph::Date> || std::same_as<T, hgraph::Time> ||
                  std::same_as<T, datetime> || std::same_as<T, duration>;

/// `lt_<T>(lhs: T, rhs: T) -> bool` (OPS-2): false where either is a NaN
/// (NUM-6: an unordered pair is no `less`).
template <ordered T>
bool lt__T_T(const T& lhs, const T& rhs) {
    return lhs < rhs;
}

/// `le_<T>(lhs: T, rhs: T) -> bool` (OPS-2, NUM-6).
template <ordered T>
bool le__T_T(const T& lhs, const T& rhs) {
    return lhs <= rhs;
}

/// `gt_<T>(lhs: T, rhs: T) -> bool` (OPS-2, NUM-6).
template <ordered T>
bool gt__T_T(const T& lhs, const T& rhs) {
    return lhs > rhs;
}

/// `ge_<T>(lhs: T, rhs: T) -> bool` (OPS-2, NUM-6).
template <ordered T>
bool ge__T_T(const T& lhs, const T& rhs) {
    return lhs >= rhs;
}

// ---- Boolean forms (OPS-5) -----------------------------------------------
//
// The compiler writes `&&` and `||` so that the right operand is evaluated
// only where it decides the result; these take operands already evaluated.

/// `and_(lhs: bool, rhs: bool) -> bool`.
bool and__bool_bool(bool lhs, bool rhs);

/// `or_(lhs: bool, rhs: bool) -> bool`.
bool or__bool_bool(bool lhs, bool rhs);

/// `not_(value: bool) -> bool`.
bool not__bool(bool value);

} // namespace hglrt
