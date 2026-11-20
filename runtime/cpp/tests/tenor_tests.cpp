// Unit tests of core's native tenor operations (tenors.h), against core's
// tenor cases (`tests/implementation/tenors.hgl`) and the edges of TEN-1 to
// TEN-9. Core's calendar candidates (`hgl/core/calendar.hgl`), which are
// HGL over these natives, are written out below as the compiler is to write
// them, so that business days are counted on calendars as core declares
// them.
#include <hglrt/tenors.h>
#include <hglrt/traces.h>

#include <doctest/doctest.h>

#include <chrono>
#include <limits>
#include <optional>
#include <string>

namespace {

using hglrt::CmpResult;
using hglrt::date;
using hglrt::i64;
using hglrt::tenor;
using std::chrono::year;

constexpr i64 most = std::numeric_limits<i64>::max();
constexpr i64 least = std::numeric_limits<i64>::min();

/// The code of the HGL failure `call` raises, or none where it raises none.
template <class Call>
std::optional<std::string> code_raised(Call&& call) {
    try {
        call();
    } catch (const hglrt::Failure& failure) {
        return failure.error().code.value_or("(no code)");
    }
    return std::nullopt;
}

tenor months(i64 count, i64 days = 0) {
    return tenor::months_days(count, days);
}
tenor days(i64 count) {
    return tenor::months_days(0, count);
}
tenor business(i64 count) {
    return tenor::business_days(count);
}

/// The text of `value` (TEN-9).
std::string text(const tenor& value) {
    return hglrt::convert_tenor(value);
}

// Core's calendar candidates (`hgl/core/calendar.hgl`), as the compiler is
// to write them from their HGL.

/// `struct Weekdays {}` and its `business_day`: `!weekend(day)`.
struct Weekdays {};
bool business_day(Weekdays, date day) {
    return !hglrt::fn_weekend(day);
}

/// core's case's `struct Closed { day: date }`: one day closed beside the
/// weekend.
struct Closed {
    date day;
};
bool business_day(Closed calendar, date day) {
    return day != calendar.day && business_day(Weekdays{}, day);
}

/// core's case's `struct Only { day: date }`: one business day.
struct Only {
    date day;
};
bool business_day(Only calendar, date day) {
    return day == calendar.day;
}

template <class C>
date on_or_after(date day, C calendar) {
    date at = day;
    while (!business_day(calendar, at)) {
        at = hglrt::fn_moved(at, days(1));
    }
    return at;
}

template <class C>
date on_or_before(date day, C calendar) {
    date at = day;
    while (!business_day(calendar, at)) {
        at = hglrt::fn_moved(at, days(-1));
    }
    return at;
}

template <class C>
date business_days_on(date day, i64 count, C calendar) {
    date at = on_or_after(day, calendar);
    for (i64 left = count; left > 0;) {
        at = hglrt::fn_moved(at, days(1));
        if (business_day(calendar, at)) {
            --left;
        }
    }
    return at;
}

template <class C>
date business_days_back(date day, i64 count, C calendar) {
    date at = on_or_before(day, calendar);
    for (i64 left = count; left > 0;) {
        at = hglrt::fn_moved(at, days(-1));
        if (business_day(calendar, at)) {
            --left;
        }
    }
    return at;
}

/// `day + by` on `calendar` (`moved_on`).
template <class C = Weekdays>
date plus(date day, const tenor& by, C calendar = {}) {
    if (const std::optional<i64> count = hglrt::fn_business_days(by)) {
        return *count < 0 ? business_days_back(day, -*count, calendar)
                          : business_days_on(day, *count, calendar);
    }
    return hglrt::fn_moved(day, by);
}

/// `day - by` on `calendar` (`moved_back`).
template <class C = Weekdays>
date minus(date day, const tenor& by, C calendar = {}) {
    if (const std::optional<i64> count = hglrt::fn_business_days(by)) {
        return *count < 0 ? business_days_on(day, -*count, calendar)
                          : business_days_back(day, *count, calendar);
    }
    return hglrt::fn_moved(day, hglrt::neg__tenor(by));
}

/// Core's `Rolling` and its `roll` (TEN-8).
enum class Rolling { following, modified_following, preceding, modified_preceding };

template <class C>
date roll(Rolling rule, C calendar, date day) {
    if (rule == Rolling::following) {
        return on_or_after(day, calendar);
    }
    if (rule == Rolling::preceding) {
        return on_or_before(day, calendar);
    }
    if (rule == Rolling::modified_following) {
        const date after = on_or_after(day, calendar);
        return hglrt::fn_same_month(after, day) ? after : on_or_before(day, calendar);
    }
    const date before = on_or_before(day, calendar);
    return hglrt::fn_same_month(before, day) ? before : on_or_after(day, calendar);
}

} // namespace

TEST_CASE("a tenor is its counts, and its arithmetic is count by count (TYPE-17, TEN-2)") {
    // core's case a_tenor_is_its_counts.
    CHECK(months(12) == months(12));
    CHECK(days(7) == days(7));
    CHECK(months(1) != days(30));
    CHECK(business(0) != days(0));
    CHECK(hglrt::add__tenor_tenor(months(6), months(6)) == months(12));
    CHECK(hglrt::sub__tenor_tenor(months(18), months(6)) == months(12));
    CHECK(hglrt::mul__tenor_i64(months(18), 2) == months(36));
    CHECK(hglrt::neg__tenor(months(1, 2)) == hglrt::mul__tenor_i64(months(1, 2), -1));
    CHECK(hglrt::add__tenor_tenor(hglrt::sub__tenor_tenor(months(1), days(1)), days(1)) == months(1));
    CHECK(hglrt::add__tenor_tenor(business(2), business(3)) == business(5));
    CHECK(hglrt::mul__tenor_i64(business(2), 3) == business(6));
    // A scaling by zero keeps the form.
    CHECK(hglrt::mul__tenor_i64(business(4), 0) == business(0));
    CHECK(hglrt::mul__tenor_i64(months(4, 2), 0) == days(0));
    CHECK(hglrt::neg__tenor(business(-3)) == business(3));
    CHECK(hglrt::sub__tenor_tenor(days(0), months(1, 2)) == months(-1, -2));
}

TEST_CASE("the two forms have no sum, and a count stays in i64 (TEN-2)") {
    // core's case the_two_forms_have_no_sum.
    CHECK(code_raised([] { (void)hglrt::add__tenor_tenor(business(2), months(1)); }) == "tenor.mixed_units");
    CHECK(code_raised([] { (void)hglrt::add__tenor_tenor(days(1), business(0)); }) == "tenor.mixed_units");
    CHECK(code_raised([] { (void)hglrt::sub__tenor_tenor(business(0), days(0)); }) == "tenor.mixed_units");
    // "A count outside the i64 range is an HGL error with code
    // `numeric.out_of_range`", each count of either form.
    CHECK(hglrt::add__tenor_tenor(months(most - 1), months(1)) == months(most));
    CHECK(code_raised([] { (void)hglrt::add__tenor_tenor(months(most), months(1)); }) ==
          "numeric.out_of_range");
    CHECK(code_raised([] { (void)hglrt::add__tenor_tenor(days(least), days(-1)); }) ==
          "numeric.out_of_range");
    CHECK(code_raised([] { (void)hglrt::sub__tenor_tenor(business(least), business(1)); }) ==
          "numeric.out_of_range");
    CHECK(code_raised([] { (void)hglrt::neg__tenor(months(least)); }) == "numeric.out_of_range");
    CHECK(code_raised([] { (void)hglrt::neg__tenor(months(0, least)); }) == "numeric.out_of_range");
    CHECK(code_raised([] { (void)hglrt::neg__tenor(business(least)); }) == "numeric.out_of_range");
    CHECK(code_raised([] { (void)hglrt::mul__tenor_i64(days(most / 2 + 1), 2); }) == "numeric.out_of_range");
    CHECK(code_raised([] { (void)hglrt::mul__tenor_i64(business(least), -1); }) == "numeric.out_of_range");
    CHECK(hglrt::neg__tenor(months(most, most)) == months(least + 1, least + 1));
}

TEST_CASE("the order is count by count, and two that differ both ways have none (TEN-3)") {
    // core's case the_order_is_count_by_count.
    CHECK(hglrt::lt__tenor_tenor(months(1), months(2)));
    CHECK(hglrt::lt__tenor_tenor(months(1), months(1, 1)));
    CHECK(hglrt::le__tenor_tenor(months(1), months(1)));
    CHECK(hglrt::lt__tenor_tenor(business(2), business(3)));
    CHECK_FALSE(hglrt::lt__tenor_tenor(months(1), days(45)));
    CHECK_FALSE(hglrt::lt__tenor_tenor(days(45), months(1)));
    CHECK_FALSE(hglrt::ge__tenor_tenor(months(1), days(45)));
    CHECK_FALSE(hglrt::lt__tenor_tenor(business(2), months(1)));
    CHECK(hglrt::compare__tenor_tenor(months(12), months(12)) == CmpResult::equivalent);
    CHECK(hglrt::compare__tenor_tenor(months(1), months(2)) == CmpResult::less);
    CHECK(hglrt::compare__tenor_tenor(business(3), business(2)) == CmpResult::greater);
    CHECK(hglrt::compare__tenor_tenor(months(1), days(45)) == CmpResult::unordered);
    CHECK(hglrt::compare__tenor_tenor(business(2), months(1)) == CmpResult::unordered);
    // Two zeros of different forms are unordered too, and none of the four
    // holds of an unordered pair.
    CHECK(hglrt::compare__tenor_tenor(business(0), days(0)) == CmpResult::unordered);
    CHECK_FALSE(hglrt::le__tenor_tenor(business(0), days(0)));
    CHECK_FALSE(hglrt::ge__tenor_tenor(business(0), days(0)));
    CHECK_FALSE(hglrt::gt__tenor_tenor(days(0), business(0)));
    // Below zero: each count less is less, and a count less with another
    // greater is unordered.
    CHECK(hglrt::lt__tenor_tenor(months(-1), months(0, 1)));
    CHECK(hglrt::gt__tenor_tenor(business(-1), business(-2)));
    CHECK(hglrt::compare__tenor_tenor(months(1, -1), days(0)) == CmpResult::unordered);
    CHECK(hglrt::ge__tenor_tenor(months(most, most), months(least, least)));
}

TEST_CASE("a tenor's text has its largest units (TEN-9)") {
    // core's case a_tenors_text_has_its_largest_units.
    CHECK(text(days(13)) == "1W6D");
    CHECK(text(months(18)) == "1Y6M");
    CHECK(text(months(18, 17)) == "1Y6M2W3D");
    CHECK(text(days(0)) == "0D");
    CHECK(text(business(0)) == "0B");
    CHECK(text(business(10)) == "10B");
    CHECK(text(hglrt::neg__tenor(days(13))) == "-1W6D");
    CHECK(text(months(1, -1)) == "1M-1D");
    // Each unit pair has its own sign, and a whole one has no smaller unit.
    CHECK(text(months(-18)) == "-1Y6M");
    CHECK(text(months(24, 14)) == "2Y2W");
    CHECK(text(months(-1, 7)) == "-1M1W");
    CHECK(text(business(-2)) == "-2B");
    CHECK(text(business(least)) == "-9223372036854775808B");
    CHECK(text(days(least)) == "-1317624576693539401W1D");
    CHECK(text(months(most)) == "768614336404564650Y7M");
    CHECK(hglrt::convert_tenor<hglrt::str>(months(3)) == "3M");
    CHECK(hglrt::testing::text_of(months(1, 2)) == "1M2D");
    CHECK(hglrt::type_name<tenor>() == "tenor");
}

TEST_CASE("the parts core's calendar candidates are written with (TEN-1, TEN-5, TEN-8)") {
    CHECK(hglrt::fn_business_days(business(3)) == 3);
    CHECK(hglrt::fn_business_days(business(0)) == 0);
    CHECK(hglrt::fn_business_days(business(-2)) == -2);
    CHECK_FALSE(hglrt::fn_business_days(days(3)).has_value());
    CHECK_FALSE(hglrt::fn_business_days(days(0)).has_value());
    // 2026-01-02 is a Friday; 0001-01-01, in the proleptic Gregorian
    // calendar, a Monday; 9999-12-31 a Friday.
    CHECK_FALSE(hglrt::fn_weekend(year{2026} / 1 / 2));
    CHECK(hglrt::fn_weekend(year{2026} / 1 / 3));
    CHECK(hglrt::fn_weekend(year{2026} / 1 / 4));
    CHECK_FALSE(hglrt::fn_weekend(year{2026} / 1 / 5));
    CHECK_FALSE(hglrt::fn_weekend(year{1} / 1 / 1));
    CHECK(hglrt::fn_weekend(year{1} / 1 / 6));
    CHECK_FALSE(hglrt::fn_weekend(year{9999} / 12 / 31));
    CHECK(hglrt::fn_same_month(year{2026} / 1 / 1, year{2026} / 1 / 31));
    CHECK_FALSE(hglrt::fn_same_month(year{2026} / 1 / 31, year{2026} / 2 / 1));
    CHECK_FALSE(hglrt::fn_same_month(year{2025} / 1 / 15, year{2026} / 1 / 15));
}

TEST_CASE("a date takes the months first, to the last day of a shorter month, and then the days (TEN-4)") {
    // core's case months_end_at_the_months_last_day.
    CHECK(plus(year{2026} / 1 / 31, months(1)) == year{2026} / 2 / 28);
    CHECK(plus(year{2026} / 1 / 31, months(3)) == year{2026} / 4 / 30);
    CHECK(plus(year{2026} / 1 / 31, months(18)) == year{2027} / 7 / 31);
    CHECK(plus(year{2024} / 2 / 29, months(12)) == year{2025} / 2 / 28);
    CHECK(minus(year{2026} / 1 / 31, days(14)) == year{2026} / 1 / 17);
    CHECK(minus(year{2026} / 3 / 31, months(1)) == year{2026} / 2 / 28);
    CHECK(plus(year{2026} / 1 / 31, months(1, 1)) == year{2026} / 3 / 1);
    CHECK(plus(year{2026} / 1 / 31, hglrt::mul__tenor_i64(months(1), 2)) == year{2026} / 3 / 31);
    CHECK(plus(plus(year{2026} / 1 / 31, months(1)), months(1)) == year{2026} / 3 / 28);
    CHECK(plus(year{2026} / 1 / 31, months(-1)) == year{2025} / 12 / 31);
    // A leap year's February, and a move back across years.
    CHECK(plus(year{2024} / 1 / 31, months(1)) == year{2024} / 2 / 29);
    CHECK(plus(year{2026} / 3 / 15, months(-27)) == year{2023} / 12 / 15);
    CHECK(plus(year{2026} / 3 / 1, days(-1)) == year{2026} / 2 / 28);
    // The ends of the date range: a result, or the date the months give,
    // outside it is out of range.
    CHECK(plus(year{9999} / 12 / 30, days(1)) == year{9999} / 12 / 31);
    CHECK(code_raised([] { (void)hglrt::fn_moved(year{9999} / 12 / 31, days(1)); }) ==
          "numeric.out_of_range");
    CHECK(code_raised([] { (void)hglrt::fn_moved(year{1} / 1 / 1, days(-1)); }) == "numeric.out_of_range");
    CHECK(code_raised([] { (void)hglrt::fn_moved(year{9999} / 12 / 1, months(1)); }) ==
          "numeric.out_of_range");
    CHECK(code_raised([] { (void)hglrt::fn_moved(year{1} / 1 / 31, months(-1)); }) == "numeric.out_of_range");
    CHECK(code_raised([] { (void)hglrt::fn_moved(year{2026} / 1 / 1, months(most)); }) ==
          "numeric.out_of_range");
    CHECK(code_raised([] { (void)hglrt::fn_moved(year{2026} / 1 / 1, months(least)); }) ==
          "numeric.out_of_range");
    CHECK(code_raised([] { (void)hglrt::fn_moved(year{2026} / 1 / 1, days(most)); }) ==
          "numeric.out_of_range");
    CHECK(code_raised([] { (void)hglrt::fn_moved(year{2026} / 1 / 1, days(least)); }) ==
          "numeric.out_of_range");
    CHECK(code_raised([] { (void)hglrt::fn_moved(year{9999} / 12 / 31, months(1, -40)); }) ==
          "numeric.out_of_range");
    CHECK(hglrt::fn_moved(year{1} / 1 / 1, months(119'987, 30)) == year{9999} / 12 / 31);
    // A tenor of business days has neither months nor days.
    CHECK(hglrt::fn_moved(year{2026} / 1 / 3, business(5)) == year{2026} / 1 / 3);
}

TEST_CASE("a count of business days starts from a business day (TEN-5, TEN-6)") {
    // core's case business_days_start_from_a_business_day; 2026-01-03 is
    // a Saturday.
    CHECK(plus(year{2026} / 1 / 2, business(1)) == year{2026} / 1 / 5);
    CHECK(plus(year{2026} / 1 / 5, business(0)) == year{2026} / 1 / 5);
    CHECK(plus(year{2026} / 1 / 5, business(5)) == year{2026} / 1 / 12);
    CHECK(minus(year{2026} / 1 / 5, business(1)) == year{2026} / 1 / 2);
    CHECK(plus(year{2026} / 1 / 3, business(0)) == year{2026} / 1 / 5);
    CHECK(minus(year{2026} / 1 / 3, business(0)) == year{2026} / 1 / 2);
    CHECK(plus(year{2026} / 1 / 3, business(1)) == year{2026} / 1 / 6);
    CHECK(plus(year{2026} / 1 / 3, business(2)) == year{2026} / 1 / 7);
    CHECK(minus(year{2026} / 1 / 3, business(1)) == year{2026} / 1 / 1);
    CHECK(plus(year{2026} / 1 / 5, hglrt::mul__tenor_i64(business(1), -1)) == year{2026} / 1 / 2);
    // core's case a_calendar_says_what_a_business_day_is.
    const Closed closed{year{2026} / 1 / 5};
    CHECK(plus(year{2026} / 1 / 2, business(1), closed) == year{2026} / 1 / 6);
    CHECK(minus(year{2026} / 1 / 6, business(1), closed) == year{2026} / 1 / 2);
    CHECK(plus(year{2026} / 1 / 5, business(0), closed) == year{2026} / 1 / 6);
    CHECK(plus(year{2026} / 1 / 2, business(1), Weekdays{}) == year{2026} / 1 / 5);
    // The tenors' text example: a trade at the weekend, settled in two days,
    // with the Monday closed.
    CHECK(plus(year{2026} / 1 / 2, business(2), closed) == year{2026} / 1 / 7);
}

TEST_CASE("a rule rolls a moved date to a business day (TEN-7, TEN-8)") {
    // core's case a_rule_rolls_to_a_business_day; 2026-01-31 is a Saturday
    // and 2026-02-01 a Sunday.
    const Weekdays weekdays;
    CHECK(roll(Rolling::following, weekdays, plus(year{2026} / 1 / 31, days(0))) == year{2026} / 2 / 2);
    CHECK(roll(Rolling::modified_following, weekdays, plus(year{2026} / 1 / 31, days(0))) ==
          year{2026} / 1 / 30);
    CHECK(roll(Rolling::preceding, weekdays, plus(year{2026} / 1 / 31, days(0))) == year{2026} / 1 / 30);
    CHECK(roll(Rolling::modified_preceding, weekdays, plus(year{2026} / 2 / 1, days(0))) ==
          year{2026} / 2 / 2);
    CHECK(roll(Rolling::modified_following, weekdays, plus(year{2025} / 10 / 31, months(3))) ==
          year{2026} / 1 / 30);
    CHECK(roll(Rolling::following, weekdays, minus(year{2026} / 5 / 1, months(3))) == year{2026} / 2 / 2);
    const Closed closed{year{2026} / 2 / 2};
    CHECK(roll(Rolling::following, closed, plus(year{2026} / 1 / 31, days(0), closed)) == year{2026} / 2 / 3);
    // A rule looks only the way it needs: this calendar has no business day
    // before its one, and none after it.
    const Only only{year{2026} / 1 / 5};
    CHECK(roll(Rolling::following, only, year{2026} / 1 / 3) == year{2026} / 1 / 5);
    CHECK(roll(Rolling::preceding, only, year{2026} / 1 / 7) == year{2026} / 1 / 5);
    CHECK(roll(Rolling::modified_following, only, year{2026} / 1 / 3) == year{2026} / 1 / 5);
    CHECK(roll(Rolling::modified_preceding, only, year{2026} / 1 / 7) == year{2026} / 1 / 5);
    // The contract's example: to the 30th of April, closed, rolled back
    // where the next business day is in May.
    const Closed april{year{2026} / 4 / 30};
    CHECK(roll(Rolling::modified_following, april, plus(year{2026} / 1 / 30, months(3), april)) ==
          year{2026} / 4 / 29);
}
