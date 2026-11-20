// Core's native scalar operators of the temporal types (temporal.h).
#include "numbers.h"

#include <hglrt/temporal.h>

#include <chrono>
#include <cmath>
#include <cstddef>
#include <memory>
#include <optional>
#include <stdexcept>

namespace hglrt {

namespace {

using detail::number_of;
using detail::u128;
using detail::width_of;

using std::chrono::sys_days;

// TYPE-2's instants, the years 1 to 9999: the first and the last
// microsecond, counted from the epoch. A civil datetime's local clock is
// counted so too.
constexpr i64 first_instant =
    std::chrono::duration_cast<duration>(sys_days{std::chrono::year{1} / 1 / 1}.time_since_epoch()).count();
constexpr i64 last_instant =
    std::chrono::duration_cast<duration>(sys_days{std::chrono::year{10000} / 1 / 1}.time_since_epoch())
        .count() -
    1;

[[noreturn]] void duration_out_of_range(std::source_location where) {
    fail(codes::out_of_range, "a duration result is outside the signed 64-bit range of microseconds", where);
}

[[noreturn]] void instant_out_of_range(std::source_location where) {
    fail(codes::out_of_range, "an instant result is outside the years 1 to 9999", where);
}

/// The microsecond `at` moved by `by` microseconds, forward or `back`, where
/// it is one of the years 1 to 9999.
i64 moved(i64 at, i64 by, bool back, std::source_location where) {
    const std::optional<i64> out = back ? detail::checked_sub(at, by) : detail::checked_add(at, by);
    if (!out.has_value() || *out < first_instant || *out > last_instant) {
        instant_out_of_range(where);
    }
    return *out;
}

/// `computed`, a count of microseconds made with checked arithmetic, as a
/// duration where it is one, and otherwise a failure at `where`.
duration in_range(std::optional<i64> computed, std::source_location where) {
    if (!computed.has_value()) {
        duration_out_of_range(where);
    }
    return duration{*computed};
}

/// The duration from the microsecond `rhs` to the microsecond `lhs`,
/// checked.
duration between(i64 lhs, i64 rhs, std::source_location where) {
    return in_range(detail::checked_sub(lhs, rhs), where);
}

/// The microsecond the date `day` starts at, on its own clock.
i64 start_of(date day) {
    return std::chrono::duration_cast<duration>(sys_days{day}.time_since_epoch()).count();
}

/// The magnitude of a duration's count: 2^63 for the least.
u128 magnitude_of(duration value) {
    return number_of(value.count()).magnitude.mantissa;
}

/// The duration of sign `negative` and magnitude `magnitude` microseconds,
/// where there is one.
duration signed_duration(bool negative, u128 magnitude, std::source_location where) {
    if (!detail::fits_i64(negative, magnitude)) {
        duration_out_of_range(where);
    }
    return duration{detail::signed_of(negative, magnitude)};
}

/// hgraph's time-zone provider for the process, made the first time it is
/// asked for, as hgraph's codecs make theirs.
const hgraph::TimeZoneProvider& zones() {
    static const std::shared_ptr<const hgraph::TimeZoneProvider> provider = hgraph::make_time_zone_provider();
    return *provider;
}

/// The instant `instant` in the zone of `like`. hgraph refuses a zoned
/// datetime whose wall clock is outside the years 1 to 9999 with an
/// `std::overflow_error`.
zoned_datetime in_zone(i64 instant, const zoned_datetime& like, std::source_location where) {
    try {
        return hgraph::at_zone(datetime{duration{instant}}, like.zone(), zones());
    } catch (const std::overflow_error&) {
        fail(codes::out_of_range, "the wall clock of a zoned result is outside the years 1 to 9999", where);
    }
}

} // namespace

// ---- Durations -------------------------------------------------------------

duration neg__duration(duration value, std::source_location where) {
    return in_range(detail::checked_neg(value.count()), where);
}

duration add__duration_duration(duration lhs, duration rhs, std::source_location where) {
    return in_range(detail::checked_add(lhs.count(), rhs.count()), where);
}

duration sub__duration_duration(duration lhs, duration rhs, std::source_location where) {
    return between(lhs.count(), rhs.count(), where);
}

duration mul__duration_i64(duration lhs, i64 rhs, std::source_location where) {
    return in_range(detail::checked_mul(lhs.count(), rhs), where);
}

duration mul__duration_f64(duration lhs, f64 rhs, std::source_location where) {
    if (!std::isfinite(rhs)) {
        fail(codes::out_of_range, "a duration multiplied by an infinity or a NaN", where);
    }
    // The product of the count and the factor's mantissa is exact in 117
    // bits; the factor's power of two then shifts it, rounded once.
    const detail::Number factor = number_of(rhs);
    const bool negative = (lhs.count() < 0) != factor.negative;
    const u128 product = magnitude_of(lhs) * factor.magnitude.mantissa;
    if (product == 0) {
        return duration::zero();
    }
    const int exponent = factor.magnitude.exponent;
    if (exponent >= 0) {
        if (width_of(product) + exponent > 64) {
            duration_out_of_range(where);
        }
        return signed_duration(negative, product << exponent, where);
    }
    return signed_duration(negative, detail::rounded_shift(product, static_cast<std::size_t>(-exponent)),
                           where);
}

duration detail::duration_quotient(duration lhs, i64 rhs, std::source_location where) {
    const bool negative = (lhs.count() < 0) != (rhs < 0);
    return signed_duration(negative, rounded_quotient(magnitude_of(lhs), number_of(rhs).magnitude.mantissa),
                           where);
}

duration detail::duration_quotient(duration lhs, f64 rhs, std::source_location where) {
    if (!std::isfinite(rhs)) {
        fail(codes::out_of_range, "a duration divided by an infinity or a NaN", where);
    }
    const Number divisor = number_of(rhs);
    const bool negative = (lhs.count() < 0) != divisor.negative;
    const u128 count = magnitude_of(lhs);
    const u128 mantissa = divisor.magnitude.mantissa;
    const int exponent = divisor.magnitude.exponent;
    if (count == 0) {
        return duration::zero();
    }
    // The quotient is `count * 2^-exponent / mantissa`, rounded once.
    if (exponent <= 0) {
        const int shift = -exponent;
        // It is more than 2^(width(count) - 1 + shift - width(mantissa)):
        // at that power or above, beyond every duration; below it the
        // shifted count fits in 118 bits.
        if (width_of(count) - 1 + shift - width_of(mantissa) >= 64) {
            duration_out_of_range(where);
        }
        return signed_duration(negative, rounded_quotient(count << shift, mantissa), where);
    }
    // A divisor more than twice the count gives a quotient below one half.
    if (width_of(mantissa) + exponent > width_of(count) + 1) {
        return duration::zero();
    }
    return signed_duration(negative, rounded_quotient(count, mantissa << exponent), where);
}

// ---- Instants --------------------------------------------------------------

datetime add__datetime_duration(datetime lhs, duration rhs, std::source_location where) {
    return datetime{duration{moved(lhs.time_since_epoch().count(), rhs.count(), false, where)}};
}

datetime sub__datetime_duration(datetime lhs, duration rhs, std::source_location where) {
    return datetime{duration{moved(lhs.time_since_epoch().count(), rhs.count(), true, where)}};
}

duration sub__datetime_datetime(datetime lhs, datetime rhs, std::source_location where) {
    return between(lhs.time_since_epoch().count(), rhs.time_since_epoch().count(), where);
}

civil_datetime add__civil_datetime_duration(civil_datetime lhs, duration rhs, std::source_location where) {
    return civil_datetime::from_epoch_microseconds(
        moved(lhs.epoch_microseconds(), rhs.count(), false, where));
}

civil_datetime sub__civil_datetime_duration(civil_datetime lhs, duration rhs, std::source_location where) {
    return civil_datetime::from_epoch_microseconds(moved(lhs.epoch_microseconds(), rhs.count(), true, where));
}

duration sub__civil_datetime_civil_datetime(civil_datetime lhs, civil_datetime rhs,
                                            std::source_location where) {
    return between(lhs.epoch_microseconds(), rhs.epoch_microseconds(), where);
}

civil_datetime add__date_duration(date lhs, duration rhs, std::source_location where) {
    return civil_datetime::from_epoch_microseconds(moved(start_of(lhs), rhs.count(), false, where));
}

civil_datetime sub__date_duration(date lhs, duration rhs, std::source_location where) {
    return civil_datetime::from_epoch_microseconds(moved(start_of(lhs), rhs.count(), true, where));
}

duration sub__date_date(date lhs, date rhs) {
    // Two dates of any years a `year_month_day` holds are less than 2^25
    // days apart, which is less than 2^62 microseconds.
    return duration{start_of(lhs) - start_of(rhs)};
}

civil_datetime add__date_time(date lhs, time rhs) {
    return civil_datetime{lhs, rhs};
}

zoned_datetime add__zoned_datetime_duration(const zoned_datetime& lhs, duration rhs,
                                            std::source_location where) {
    return in_zone(moved(lhs.instant().time_since_epoch().count(), rhs.count(), false, where), lhs, where);
}

zoned_datetime sub__zoned_datetime_duration(const zoned_datetime& lhs, duration rhs,
                                            std::source_location where) {
    return in_zone(moved(lhs.instant().time_since_epoch().count(), rhs.count(), true, where), lhs, where);
}

bool eq__zoned_datetime_zoned_datetime_bool(const zoned_datetime& lhs, const zoned_datetime& rhs, bool utc) {
    // One instant in one zone has one offset, so the two are compared by
    // their instants and their zones' names.
    return lhs.instant() == rhs.instant() && (utc || lhs.zone() == rhs.zone());
}

bool ne__zoned_datetime_zoned_datetime_bool(const zoned_datetime& lhs, const zoned_datetime& rhs, bool utc) {
    return !eq__zoned_datetime_zoned_datetime_bool(lhs, rhs, utc);
}

} // namespace hglrt
