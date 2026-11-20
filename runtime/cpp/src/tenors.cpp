// Core's native tenor operations (tenors.h).
#include "numbers.h"

#include <hglrt/tenors.h>

#include <algorithm>
#include <chrono>
#include <cstdint>
#include <string>
#include <string_view>

namespace hglrt {

namespace {

using detail::i128;

/// Fails a sum or a difference of two tenors of different forms (TEN-2).
void same_form(const tenor& lhs, const tenor& rhs, std::string_view what, std::source_location where) {
    if (lhs.is_business() != rhs.is_business()) {
        fail(codes::mixed_units,
             "a tenor of business days and one of months and days have no " + std::string{what}, where);
    }
}

[[noreturn]] void count_out_of_range(std::source_location where) {
    fail(codes::out_of_range, "a count of a tenor is outside the signed 64-bit range", where);
}

i64 summed(i64 lhs, i64 rhs, std::source_location where) {
    i64 out = 0;
    if (__builtin_add_overflow(lhs, rhs, &out)) {
        count_out_of_range(where);
    }
    return out;
}

i64 subtracted(i64 lhs, i64 rhs, std::source_location where) {
    i64 out = 0;
    if (__builtin_sub_overflow(lhs, rhs, &out)) {
        count_out_of_range(where);
    }
    return out;
}

i64 scaled(i64 lhs, i64 rhs, std::source_location where) {
    i64 out = 0;
    if (__builtin_mul_overflow(lhs, rhs, &out)) {
        count_out_of_range(where);
    }
    return out;
}

/// The text of one unit pair of a tenor (TEN-9): `count` in `larger` units
/// of `each` of the `smaller`, those of either that are not zero, after a
/// `-` where it is below zero; nothing for a count of zero.
std::string units(i64 count, std::uint64_t each, char larger, char smaller) {
    if (count == 0) {
        return {};
    }
    const auto bits = static_cast<std::uint64_t>(count);
    const std::uint64_t magnitude = count < 0 ? 0 - bits : bits;
    std::string text = count < 0 ? "-" : "";
    if (const std::uint64_t high = magnitude / each; high != 0) {
        text += std::to_string(high) + larger;
    }
    if (const std::uint64_t low = magnitude % each; low != 0) {
        text += std::to_string(low) + smaller;
    }
    return text;
}

[[noreturn]] void date_out_of_range(std::source_location where) {
    fail(codes::out_of_range, "a date result is outside the years 1 to 9999", where);
}

} // namespace

tenor add__tenor_tenor(const tenor& lhs, const tenor& rhs, std::source_location where) {
    same_form(lhs, rhs, "sum", where);
    if (lhs.is_business()) {
        return tenor::business_days(summed(lhs.business(), rhs.business(), where));
    }
    return tenor::months_days(summed(lhs.months(), rhs.months(), where),
                              summed(lhs.days(), rhs.days(), where));
}

tenor sub__tenor_tenor(const tenor& lhs, const tenor& rhs, std::source_location where) {
    same_form(lhs, rhs, "difference", where);
    if (lhs.is_business()) {
        return tenor::business_days(subtracted(lhs.business(), rhs.business(), where));
    }
    return tenor::months_days(subtracted(lhs.months(), rhs.months(), where),
                              subtracted(lhs.days(), rhs.days(), where));
}

tenor neg__tenor(const tenor& value, std::source_location where) {
    if (value.is_business()) {
        return tenor::business_days(subtracted(0, value.business(), where));
    }
    return tenor::months_days(subtracted(0, value.months(), where), subtracted(0, value.days(), where));
}

tenor mul__tenor_i64(const tenor& lhs, i64 rhs, std::source_location where) {
    if (lhs.is_business()) {
        return tenor::business_days(scaled(lhs.business(), rhs, where));
    }
    return tenor::months_days(scaled(lhs.months(), rhs, where), scaled(lhs.days(), rhs, where));
}

CmpResult compare__tenor_tenor(const tenor& lhs, const tenor& rhs) {
    if (lhs.is_business() != rhs.is_business()) {
        return CmpResult::unordered;
    }
    bool less = false;
    bool greater = false;
    const auto count = [&](i64 left, i64 right) {
        less = less || left < right;
        greater = greater || left > right;
    };
    if (lhs.is_business()) {
        count(lhs.business(), rhs.business());
    } else {
        count(lhs.months(), rhs.months());
        count(lhs.days(), rhs.days());
    }
    if (less && greater) {
        return CmpResult::unordered;
    }
    return less ? CmpResult::less : greater ? CmpResult::greater : CmpResult::equivalent;
}

bool lt__tenor_tenor(const tenor& lhs, const tenor& rhs) {
    return compare__tenor_tenor(lhs, rhs) == CmpResult::less;
}

bool le__tenor_tenor(const tenor& lhs, const tenor& rhs) {
    const CmpResult outcome = compare__tenor_tenor(lhs, rhs);
    return outcome == CmpResult::less || outcome == CmpResult::equivalent;
}

bool gt__tenor_tenor(const tenor& lhs, const tenor& rhs) {
    return compare__tenor_tenor(lhs, rhs) == CmpResult::greater;
}

bool ge__tenor_tenor(const tenor& lhs, const tenor& rhs) {
    const CmpResult outcome = compare__tenor_tenor(lhs, rhs);
    return outcome == CmpResult::greater || outcome == CmpResult::equivalent;
}

str detail::tenor_text(const tenor& value) {
    if (value.is_business()) {
        return std::to_string(value.business()) + "B";
    }
    std::string text = units(value.months(), 12, 'Y', 'M') + units(value.days(), 7, 'W', 'D');
    return text.empty() ? "0D" : text;
}

std::optional<i64> fn_business_days(const tenor& value) {
    if (value.is_business()) {
        return value.business();
    }
    return std::nullopt;
}

date fn_moved(date day, const tenor& by, std::source_location where) {
    using namespace std::chrono;
    // The months first, counted from the year 0 so that a floor division
    // gives the year: 128 bits hold any count of them.
    const i128 month_index = static_cast<i128>(static_cast<int>(day.year())) * 12 +
                             (static_cast<unsigned>(day.month()) - 1) + by.months();
    i128 year_number = month_index / 12;
    i128 month_number = month_index % 12;
    if (month_number < 0) {
        month_number += 12;
        --year_number;
    }
    if (year_number < 1 || year_number > 9999) {
        date_out_of_range(where);
    }
    const year_month whole{year{static_cast<int>(year_number)},
                           month{static_cast<unsigned>(month_number) + 1}};
    // The same day of that month, or its last where it has no such day.
    const unsigned last_day = static_cast<unsigned>((whole / last).day());
    const sys_days start{whole / std::min(static_cast<unsigned>(day.day()), last_day)};
    // Then the days.
    constexpr i128 first_day = sys_days{year{1} / January / 1}.time_since_epoch().count();
    constexpr i128 latest_day = sys_days{year{9999} / December / 31}.time_since_epoch().count();
    const i128 moved = static_cast<i128>(start.time_since_epoch().count()) + by.days();
    if (moved < first_day || moved > latest_day) {
        date_out_of_range(where);
    }
    return year_month_day{sys_days{days{static_cast<days::rep>(moved)}}};
}

bool fn_weekend(date day) {
    const std::chrono::weekday of{std::chrono::sys_days{day}};
    return of == std::chrono::Saturday || of == std::chrono::Sunday;
}

bool fn_same_month(date lhs, date rhs) {
    return lhs.year() == rhs.year() && lhs.month() == rhs.month();
}

} // namespace hglrt
