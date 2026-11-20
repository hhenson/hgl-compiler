// Core's native scalar operators of numbers and of the ordered types
// (`hgl/native/scalar_operators.hgl`, core's numeric contract NUM-1 to
// NUM-12 and its scalar contract OPS-2, OPS-3, OPS-5): checked `i64`
// arithmetic, `f64` and mixed arithmetic, every division of numbers under
// its zero policy, numeric equality with its tolerances, order and the
// four-way comparison, and the Boolean forms. The other scalar types'
// operators are in temporal.h and text.h, and the conversions in
// conversions.h.
//
// Each is a function named as the C++ target's emitter names the candidate
// it is (crates/hgl_cpp, `names::native_candidate`): the operator's name,
// then the type of each parameter, each after a `_`, `add__i64_i64` for
// `native impl const fn add_(lhs: i64, rhs: i64) -> i64`. A candidate with
// a generic type parameter is a function template, `lt__T_T<T>`, which the
// generated code calls with the type it instances. The candidates of a
// division that differ only in the zero policy they require, and so in
// what they give, are one template whose argument is the policy (NUM-9:
// "`zero` is a generic value parameter of the candidates that declare
// it"). One that can fail takes the place it is called from last, which
// its failure carries (failure.h); the generated code leaves it to its
// default.
#pragma once

#include <hglrt/failure.h>
#include <hglrt/values.h>

#include <concepts>
#include <limits>
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

// ---- Mixed arithmetic (NUM-1, NUM-2) -------------------------------------
//
// NUM-1: "Unconfigured mixed i64/f64 and f64 arithmetic returns f64." The
// `i64` operand is taken as the `f64` nearest it, a tie to the even one,
// and the operation is then the IEEE one of two `f64`, as the reference
// provider has it (`float(lhs) + rhs`). The reverse orders of `add_` and
// `mul_` are core's own HGL candidates, which call these.

/// `add_(lhs: i64, rhs: f64) -> f64`.
f64 add__i64_f64(i64 lhs, f64 rhs);

/// `sub_(lhs: i64, rhs: f64) -> f64`.
f64 sub__i64_f64(i64 lhs, f64 rhs);

/// `sub_(lhs: f64, rhs: i64) -> f64`.
f64 sub__f64_i64(f64 lhs, i64 rhs);

/// `mul_(lhs: i64, rhs: f64) -> f64`.
f64 mul__i64_f64(i64 lhs, f64 rhs);

// ---- Division and its zero policy (NUM-1, NUM-3, NUM-9, NUM-12) ----------

/// The members of core's enum `ZeroPolicy` (NUM-3), in its order, which a
/// division's `zero` names. A call gives `zero` as a compile-time constant
/// (NUM-9), and the generated code passes the member it names as the
/// template argument.
enum class ZeroPolicy { ieee, nan, error, fail };

/// The names of core's `ZeroPolicy` (values.h, `EnumNames`).
constexpr EnumNames<ZeroPolicy, 4> hgl_enum(ZeroPolicy) noexcept {
    return {"core::ZeroPolicy",
            {{ZeroPolicy::ieee, "ieee"},
             {ZeroPolicy::nan, "nan"},
             {ZeroPolicy::error, "error"},
             {ZeroPolicy::fail, "fail"}}};
}

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

/// `lhs // rhs` of two `f64`, `rhs` not zero (NUM-1): the floor of the
/// exact quotient of finite operands, and otherwise "the floor of the IEEE
/// quotient".
f64 floor_quotient(f64 lhs, f64 rhs);

/// `lhs % rhs` of two `f64`, `rhs` not zero (NUM-1): the remainder with the
/// divisor's sign of finite operands, and otherwise "what IEEE `fmod`
/// gives: a remainder by an infinity is the numerator as it is, whatever
/// its sign".
f64 floor_remainder(f64 lhs, f64 rhs);

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

/// `div_<zero>(lhs: i64, rhs: f64)`: true division, the `i64` taken as the
/// `f64` nearest it (see "Mixed arithmetic"), under `zero` as for
/// `div__f64_f64`.
template <ZeroPolicy zero = ZeroPolicy::ieee>
detail::divided<f64, zero> div__i64_f64(i64 lhs, f64 rhs,
                                        std::source_location where = std::source_location::current()) {
    return div__f64_f64<zero>(static_cast<f64>(lhs), rhs, where);
}

/// `div_<zero>(lhs: f64, rhs: i64)`: true division, under `zero` as for
/// `div__f64_f64`; a zero `i64` divisor is a positive zero.
template <ZeroPolicy zero = ZeroPolicy::ieee>
detail::divided<f64, zero> div__f64_i64(f64 lhs, i64 rhs,
                                        std::source_location where = std::source_location::current()) {
    return div__f64_f64<zero>(lhs, static_cast<f64>(rhs), where);
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

/// `floordiv_<zero>(lhs: f64, rhs: f64)`: the floor of the quotient
/// (NUM-1). It admits every policy, and defaults to `.ieee` (NUM-12: "Under
/// `.ieee` a floor division or a modulo with an `f64` operand is what IEEE
/// arithmetic makes of the zero divisor"): a zero divisor gives what it
/// gives `div__f64_f64`, `7.0 // 0.0` positive infinity.
template <ZeroPolicy zero = ZeroPolicy::ieee>
detail::divided<f64, zero> floordiv__f64_f64(f64 lhs, f64 rhs,
                                             std::source_location where = std::source_location::current()) {
    if (rhs == 0.0) {
        if constexpr (zero == ZeroPolicy::error) {
            return std::unexpected(detail::division_by_zero());
        } else {
            return detail::divide(lhs, rhs, zero, where);
        }
    }
    return detail::floor_quotient(lhs, rhs);
}

/// `floordiv_<zero>(lhs: i64, rhs: f64)`: as `floordiv__f64_f64`, the `i64`
/// taken as the `f64` nearest it.
template <ZeroPolicy zero = ZeroPolicy::ieee>
detail::divided<f64, zero> floordiv__i64_f64(i64 lhs, f64 rhs,
                                             std::source_location where = std::source_location::current()) {
    return floordiv__f64_f64<zero>(static_cast<f64>(lhs), rhs, where);
}

/// `floordiv_<zero>(lhs: f64, rhs: i64)`: as `floordiv__f64_f64`.
template <ZeroPolicy zero = ZeroPolicy::ieee>
detail::divided<f64, zero> floordiv__f64_i64(f64 lhs, i64 rhs,
                                             std::source_location where = std::source_location::current()) {
    return floordiv__f64_f64<zero>(lhs, static_cast<f64>(rhs), where);
}

/// `mod_<zero>(lhs: f64, rhs: f64)`: the remainder with the divisor's sign
/// (NUM-1). It admits every policy, and defaults to `.ieee`: a zero divisor
/// gives a NaN under `.ieee` and `.nan`, as IEEE's remainder does
/// (`7.0 % 0.0` is NaN), fails under `.fail`, and is an `err` under
/// `.error`.
template <ZeroPolicy zero = ZeroPolicy::ieee>
detail::divided<f64, zero> mod__f64_f64(f64 lhs, f64 rhs,
                                        std::source_location where = std::source_location::current()) {
    if (rhs == 0.0) {
        if constexpr (zero == ZeroPolicy::error) {
            return std::unexpected(detail::division_by_zero());
        } else {
            if constexpr (zero == ZeroPolicy::fail) {
                detail::fail_division_by_zero(where);
            }
            return std::numeric_limits<f64>::quiet_NaN();
        }
    }
    return detail::floor_remainder(lhs, rhs);
}

/// `mod_<zero>(lhs: i64, rhs: f64)`: as `mod__f64_f64`, the `i64` taken as
/// the `f64` nearest it.
template <ZeroPolicy zero = ZeroPolicy::ieee>
detail::divided<f64, zero> mod__i64_f64(i64 lhs, f64 rhs,
                                        std::source_location where = std::source_location::current()) {
    return mod__f64_f64<zero>(static_cast<f64>(lhs), rhs, where);
}

/// `mod_<zero>(lhs: f64, rhs: i64)`: as `mod__f64_f64`.
template <ZeroPolicy zero = ZeroPolicy::ieee>
detail::divided<f64, zero> mod__f64_i64(f64 lhs, i64 rhs,
                                        std::source_location where = std::source_location::current()) {
    return mod__f64_f64<zero>(lhs, static_cast<f64>(rhs), where);
}

// ---- Numeric equality (NUM-2, NUM-4, NUM-5, OPS-2) -----------------------
//
// NUM-4: "Plain numeric `==` is exact and `!=` is its complement. Configured
// `==` accepts absolute (`abs`) and relative (`rel`) tolerance, each
// defaulting to zero", and for finite operands closeness is
// `abs(a - b) <= max(abs, rel * max(abs(a), abs(b)))`, decided here exactly.
// A tolerance that is not finite and nonnegative fails, with no code.
// NUM-5: NaN is close to nothing, an infinity only to the same infinity, and
// the two zeros are equal. NUM-2: an `i64` is never rounded to an `f64` to be
// compared with one. The order `f64` then `i64` is core's own HGL candidate.

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

// ---- Equality, order and comparison of one type (OPS-2, OPS-3, NUM-6) ----

/// `eq_<T>(lhs: T, rhs: T) -> bool`: the language's equality of two values
/// of one type (OPS-2), which is C++'s `==` of what the target writes: a
/// NaN is equal to nothing, a list or a tuple is equal position by
/// position, and a struct the compiler writes is equal field by field.
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
/// bool, date, time, datetime, civil_datetime, duration}`). `str` is ordered
/// by its bytes, which is the order of its code points; `false` is before
/// `true`; a temporal value by its time.
template <class T>
concept ordered = std::same_as<T, i64> || std::same_as<T, f64> || std::same_as<T, str> ||
                  std::same_as<T, bool> || std::same_as<T, date> || std::same_as<T, time> ||
                  std::same_as<T, datetime> || std::same_as<T, civil_datetime> || std::same_as<T, duration>;

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

/// The members of core's enum `CmpResult` (OPS-3), in its order: what
/// `<=>` gives.
enum class CmpResult { less, equivalent, greater, unordered };

/// The names of core's `CmpResult` (values.h, `EnumNames`).
constexpr EnumNames<CmpResult, 4> hgl_enum(CmpResult) noexcept {
    return {"core::CmpResult",
            {{CmpResult::less, "less"},
             {CmpResult::equivalent, "equivalent"},
             {CmpResult::greater, "greater"},
             {CmpResult::unordered, "unordered"}}};
}

/// `compare_<T>(lhs: T, rhs: T) -> CmpResult` (OPS-3, NUM-6): "A NaN operand
/// yields `unordered`; equivalent numeric values, including opposite signed
/// zeros, yield `equivalent`." Only a number is a NaN, so only a numeric
/// comparison is unordered.
template <ordered T>
CmpResult compare__T_T(const T& lhs, const T& rhs) {
    if (lhs < rhs) {
        return CmpResult::less;
    }
    if (rhs < lhs) {
        return CmpResult::greater;
    }
    return lhs == rhs ? CmpResult::equivalent : CmpResult::unordered;
}

// The order of an `i64` and an `f64` (NUM-2: "Mixed numeric comparison must
// not first round an i64 operand to f64"; NUM-6's table): by their exact
// values, and false, or `unordered`, where the `f64` is a NaN.

/// `lt_(lhs: i64, rhs: f64) -> bool`.
bool lt__i64_f64(i64 lhs, f64 rhs);
/// `lt_(lhs: f64, rhs: i64) -> bool`.
bool lt__f64_i64(f64 lhs, i64 rhs);
/// `le_(lhs: i64, rhs: f64) -> bool`.
bool le__i64_f64(i64 lhs, f64 rhs);
/// `le_(lhs: f64, rhs: i64) -> bool`.
bool le__f64_i64(f64 lhs, i64 rhs);
/// `gt_(lhs: i64, rhs: f64) -> bool`.
bool gt__i64_f64(i64 lhs, f64 rhs);
/// `gt_(lhs: f64, rhs: i64) -> bool`.
bool gt__f64_i64(f64 lhs, i64 rhs);
/// `ge_(lhs: i64, rhs: f64) -> bool`.
bool ge__i64_f64(i64 lhs, f64 rhs);
/// `ge_(lhs: f64, rhs: i64) -> bool`.
bool ge__f64_i64(f64 lhs, i64 rhs);
/// `compare_(lhs: i64, rhs: f64) -> CmpResult`.
CmpResult compare__i64_f64(i64 lhs, f64 rhs);
/// `compare_(lhs: f64, rhs: i64) -> CmpResult`.
CmpResult compare__f64_i64(f64 lhs, i64 rhs);

// ---- Boolean forms (OPS-5) -----------------------------------------------
//
// OPS-5: "A chain of three or more operands joined by `&&` alone is `all_`
// over them, and one joined by `||` alone is `any_`. Core's candidate of
// each for bools is the chain itself, `and_` or `or_` applied left to right,
// short-circuiting as the links do." The compiler writes `&&` and `||` so
// that an operand is evaluated only where it decides the result; these take
// operands already evaluated.

/// `and_(lhs: bool, rhs: bool) -> bool`.
bool and__bool_bool(bool lhs, bool rhs);

/// `or_(lhs: bool, rhs: bool) -> bool`.
bool or__bool_bool(bool lhs, bool rhs);

/// `not_(value: bool) -> bool`.
bool not__bool(bool value);

/// `all_(values: ...bool) -> bool`: whether every one of `values` is true;
/// true of none.
template <std::same_as<bool>... Values>
bool all__bool(Values... values) {
    return (values && ...);
}

/// `any_(values: ...bool) -> bool`: whether one of `values` is true; false
/// of none.
template <std::same_as<bool>... Values>
bool any__bool(Values... values) {
    return (values || ...);
}

} // namespace hglrt
