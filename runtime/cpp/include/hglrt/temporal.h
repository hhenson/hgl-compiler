// Core's native scalar operators of the temporal types
// (`hgl/native/scalar_operators.hgl`, core's scalar contract OPS-1, OPS-2
// and OPS-10): a duration's arithmetic, an instant moved by a duration, the
// duration between two instants, a date and a time combined, and two zoned
// datetimes compared. Each is named as scalar.h says.
//
// OPS-10: "A `duration` result is the exact result rounded to the nearest
// duration the type holds, a tie to the even one ... A `duration` or an
// instant result outside its type's range is an HGL error with code
// `numeric.out_of_range`, as an `i64` result is (NUM-2): it neither wraps nor
// saturates. So is a `duration` multiplied or divided by a NaN or an
// infinity." A duration holds any signed 64-bit count of microseconds; an
// instant, a `datetime` or a `civil_datetime`, one of the years 1 to 9999
// (TYPE-2), as does the wall clock of a `zoned_datetime`.
#pragma once

#include <hglrt/failure.h>
#include <hglrt/scalar.h>
#include <hglrt/values.h>

#include <cstdint>
#include <source_location>
#include <string_view>

namespace hglrt {

// ---- Durations (OPS-1, OPS-10) -------------------------------------------

/// `neg_(value: duration) -> duration`: the negation, checked: that of the
/// least duration is out of range.
duration neg__duration(duration value, std::source_location where = std::source_location::current());

/// `add_(lhs: duration, rhs: duration) -> duration`: the sum, checked.
duration add__duration_duration(duration lhs, duration rhs,
                                std::source_location where = std::source_location::current());

/// `sub_(lhs: duration, rhs: duration) -> duration`: the difference,
/// checked.
duration sub__duration_duration(duration lhs, duration rhs,
                                std::source_location where = std::source_location::current());

/// `mul_(lhs: duration, rhs: i64) -> duration`: the product, checked. Core's
/// HGL candidate takes the operands the other way round.
duration mul__duration_i64(duration lhs, i64 rhs,
                           std::source_location where = std::source_location::current());

/// `mul_(lhs: duration, rhs: f64) -> duration`: the exact product rounded to
/// the microsecond, a tie to the even one (`3us * 0.5` is `2us`); an infinity
/// or a NaN factor, or a product outside the range, is out of range.
duration mul__duration_f64(duration lhs, f64 rhs,
                           std::source_location where = std::source_location::current());

namespace detail {
/// `lhs / rhs`, `rhs` not zero: the exact quotient rounded to the
/// microsecond, a tie to the even one, checked.
duration duration_quotient(duration lhs, i64 rhs, std::source_location where);

/// `lhs / rhs`, `rhs` not zero: as above; an infinity or a NaN divisor is
/// out of range.
duration duration_quotient(duration lhs, f64 rhs, std::source_location where);
} // namespace detail

/// `div_<zero>(lhs: duration, rhs: i64)`: the quotient rounded to the
/// microsecond (`5m / 3` is `100s`, `2us / 3` is `1us`). It admits `.fail`,
/// the default, and `.error` (NUM-12: the result holds no infinity or NaN).
template <ZeroPolicy zero = ZeroPolicy::fail>
detail::divided<duration, zero>
div__duration_i64(duration lhs, i64 rhs, std::source_location where = std::source_location::current()) {
    static_assert(zero == ZeroPolicy::fail || zero == ZeroPolicy::error,
                  "NUM-12: a duration divided by a number admits only .fail and .error");
    if (rhs == 0) {
        if constexpr (zero == ZeroPolicy::error) {
            return std::unexpected(detail::division_by_zero());
        } else {
            detail::fail_division_by_zero(where);
        }
    }
    return detail::duration_quotient(lhs, rhs, where);
}

/// `div_<zero>(lhs: duration, rhs: f64)`: as `div__duration_i64`; a zero of
/// either sign is a zero divisor, and an infinity or a NaN is out of range.
template <ZeroPolicy zero = ZeroPolicy::fail>
detail::divided<duration, zero>
div__duration_f64(duration lhs, f64 rhs, std::source_location where = std::source_location::current()) {
    static_assert(zero == ZeroPolicy::fail || zero == ZeroPolicy::error,
                  "NUM-12: a duration divided by a number admits only .fail and .error");
    if (rhs == 0.0) {
        if constexpr (zero == ZeroPolicy::error) {
            return std::unexpected(detail::division_by_zero());
        } else {
            detail::fail_division_by_zero(where);
        }
    }
    return detail::duration_quotient(lhs, rhs, where);
}

/// `div_<zero>(lhs: duration, rhs: duration)`: the ratio of the two counts,
/// the `f64` nearest it. It admits every policy, and defaults to `.ieee`, as
/// a quotient of numbers does: `5m / 0s` is positive infinity, `0s / 0s` a
/// NaN.
template <ZeroPolicy zero = ZeroPolicy::ieee>
detail::divided<f64, zero>
div__duration_duration(duration lhs, duration rhs,
                       std::source_location where = std::source_location::current()) {
    return div__i64_i64<zero>(lhs.count(), rhs.count(), where);
}

/// `floordiv_<zero>(lhs: duration, rhs: duration)`: how many whole `rhs`
/// there are in `lhs`, rounded toward negative infinity (NUM-1), an `i64`,
/// checked. It admits `.fail`, the default, and `.error` (NUM-12).
template <ZeroPolicy zero = ZeroPolicy::fail>
detail::divided<i64, zero>
floordiv__duration_duration(duration lhs, duration rhs,
                            std::source_location where = std::source_location::current()) {
    return floordiv__i64_i64<zero>(lhs.count(), rhs.count(), where);
}

// ---- Instants (OPS-1, OPS-10) --------------------------------------------

/// `add_(lhs: datetime, rhs: duration) -> datetime`: the instant `rhs` after
/// `lhs`, exact to the microsecond and checked.
datetime add__datetime_duration(datetime lhs, duration rhs,
                                std::source_location where = std::source_location::current());

/// `sub_(lhs: datetime, rhs: duration) -> datetime`: the instant `rhs`
/// before `lhs`, checked.
datetime sub__datetime_duration(datetime lhs, duration rhs,
                                std::source_location where = std::source_location::current());

/// `sub_(lhs: datetime, rhs: datetime) -> duration`: the time from `rhs` to
/// `lhs`, checked.
duration sub__datetime_datetime(datetime lhs, datetime rhs,
                                std::source_location where = std::source_location::current());

/// `add_(lhs: civil_datetime, rhs: duration) -> civil_datetime`: the local
/// time `rhs` after `lhs`, checked.
civil_datetime add__civil_datetime_duration(civil_datetime lhs, duration rhs,
                                            std::source_location where = std::source_location::current());

/// `sub_(lhs: civil_datetime, rhs: duration) -> civil_datetime`: the local
/// time `rhs` before `lhs`, checked.
civil_datetime sub__civil_datetime_duration(civil_datetime lhs, duration rhs,
                                            std::source_location where = std::source_location::current());

/// `sub_(lhs: civil_datetime, rhs: civil_datetime) -> duration`: the time
/// from `rhs` to `lhs` on one clock, checked.
duration sub__civil_datetime_civil_datetime(civil_datetime lhs, civil_datetime rhs,
                                            std::source_location where = std::source_location::current());

/// `add_(lhs: date, rhs: duration) -> civil_datetime`: OPS-10, "A `date`
/// with a `duration` added or taken away is the `civil_datetime` that far
/// from the start of the date, whatever the duration": `@2026-09-03 + 36h` is
/// `@2026-09-04T12:00`.
civil_datetime add__date_duration(date lhs, duration rhs,
                                  std::source_location where = std::source_location::current());

/// `sub_(lhs: date, rhs: duration) -> civil_datetime`: the local time `rhs`
/// before the start of `lhs`.
civil_datetime sub__date_duration(date lhs, duration rhs,
                                  std::source_location where = std::source_location::current());

/// `sub_(lhs: date, rhs: date) -> duration`: the time from the start of
/// `rhs` to the start of `lhs`, whole days.
duration sub__date_date(date lhs, date rhs);

/// `add_(lhs: date, rhs: time) -> civil_datetime`: that time on that date
/// (OPS-1). Core's HGL candidate takes the operands the other way round.
civil_datetime add__date_time(date lhs, time rhs);

/// `add_(lhs: zoned_datetime, rhs: duration) -> zoned_datetime`: OPS-10, "the
/// instant that far away, in the same zone: its wall clock and offset are the
/// zone's at that instant". The offset is the one hgraph's time-zone
/// provider for the process gives (`make_time_zone_provider`), as hgraph's
/// own codecs read one: a value function is given no run's store to find a
/// provider of the run's in. A wall clock outside the years 1 to 9999 is out
/// of range.
zoned_datetime add__zoned_datetime_duration(const zoned_datetime& lhs, duration rhs,
                                            std::source_location where = std::source_location::current());

/// `sub_(lhs: zoned_datetime, rhs: duration) -> zoned_datetime`: the
/// instant `rhs` before, in the same zone.
zoned_datetime sub__zoned_datetime_duration(const zoned_datetime& lhs, duration rhs,
                                            std::source_location where = std::source_location::current());

// ---- Zones (LEX-12) ---------------------------------------------------------
//
// LEX-12: "The selected run provider checks exact catalog membership and
// offset consistency." The provider is hgraph's time-zone provider for the
// process, as `add__zoned_datetime_duration` reads one. A literal that fails
// its check fails where it is evaluated, with no code: the catalogue has
// none for it.

/// `@[Europe/London]`, a `timezone` literal: the zone `name`, where the
/// provider's catalogue has it.
hgraph::ZoneId zone_literal(std::string_view name,
                            std::source_location where = std::source_location::current());

/// `@09:30[Europe/London]`, a `zoned_time` literal: the time `at` in the zone
/// `name`, checked as `zone_literal` checks it. It is on no date, so it has
/// no offset to check.
hgraph::ZonedTime zoned_time_literal(time at, std::string_view name,
                                     std::source_location where = std::source_location::current());

/// `@2026-09-03T09:30+01:00[Europe/London]`, a `zoned_datetime` literal: the
/// instant `at`, written `offset_seconds` east of UTC, in the zone `name`,
/// checked as `zone_literal` checks it. The offset written is the zone's own
/// at the instant, or the literal fails: "A time a change of offset repeats
/// is one instant at each of its two offsets, and a time it skips is at
/// none".
zoned_datetime zoned_datetime_literal(datetime at, std::int32_t offset_seconds, std::string_view name,
                                      std::source_location where = std::source_location::current());

/// `eq_(lhs: zoned_datetime, rhs: zoned_datetime, utc: bool = false) -> bool`
/// (OPS-2, TYPE-2): "equal when their instants and their zone names both
/// are; compared as UTC, they are equal when their instants are". Two
/// names of one zone are two zones.
bool eq__zoned_datetime_zoned_datetime_bool(const zoned_datetime& lhs, const zoned_datetime& rhs,
                                            bool utc = false);

/// `ne_(lhs: zoned_datetime, rhs: zoned_datetime, utc: bool = false) -> bool`:
/// the complement of `eq_` under the same option.
bool ne__zoned_datetime_zoned_datetime_bool(const zoned_datetime& lhs, const zoned_datetime& rhs,
                                            bool utc = false);

} // namespace hglrt
