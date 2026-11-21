// Unit tests of core's native scalar operators of the temporal types
// (temporal.h), against the observations core's scalar contract lists
// (`spec/scalar-operators.md`, OPS-10) and the ends of each type's range.
#include <hglrt/temporal.h>

#include <doctest/doctest.h>

#include <chrono>
#include <cmath>
#include <limits>
#include <optional>
#include <string>

namespace {

using hglrt::duration;
using hglrt::f64;
using hglrt::i64;
using hglrt::ZeroPolicy;
using std::chrono::days;
using std::chrono::hours;
using std::chrono::microseconds;
using std::chrono::sys_days;
using std::chrono::year;
using std::chrono::year_month_day;
using namespace std::chrono_literals;

constexpr i64 most = std::numeric_limits<i64>::max();
constexpr i64 least = std::numeric_limits<i64>::min();
constexpr f64 infinity = std::numeric_limits<f64>::infinity();
const f64 not_a_number = std::numeric_limits<f64>::quiet_NaN();

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

/// The first and the last instant of the years 1 to 9999 (TYPE-2).
const hglrt::datetime first_instant = sys_days{year{1} / 1 / 1};
const hglrt::datetime last_instant = sys_days{year{9999} / 12 / 31} + hours{24} - microseconds{1};

/// The civil datetime of the date `day` at `hour` and `minute`.
hglrt::civil_datetime civil(year_month_day day, int hour = 0, int minute = 0) {
    return hglrt::civil_datetime{day, hour, minute};
}

} // namespace

TEST_CASE("a duration's arithmetic is exact and checked (OPS-1, OPS-10)") {
    CHECK(hglrt::add__duration_duration(5min, 30s) == 330s);
    CHECK(hglrt::sub__duration_duration(5min, 30s) == 270s);
    CHECK(hglrt::neg__duration(5min) == -5min);
    CHECK(hglrt::mul__duration_i64(5min, 2) == 10min);
    CHECK(hglrt::mul__duration_i64(5min, -2) == -10min);
    const duration longest{most};
    const duration shortest{least};
    CHECK(hglrt::add__duration_duration(longest - 1us, 1us) == longest);
    CHECK(code_raised([&] { (void)hglrt::add__duration_duration(longest, 1us); }) == "numeric.out_of_range");
    CHECK(code_raised([&] { (void)hglrt::sub__duration_duration(shortest, 1us); }) == "numeric.out_of_range");
    CHECK(code_raised([&] { (void)hglrt::neg__duration(shortest); }) == "numeric.out_of_range");
    CHECK(hglrt::neg__duration(longest) == shortest + 1us);
    CHECK(code_raised([&] { (void)hglrt::mul__duration_i64(longest, 2); }) == "numeric.out_of_range");
    CHECK(code_raised([&] { (void)hglrt::mul__duration_i64(shortest, -1); }) == "numeric.out_of_range");
}

TEST_CASE("a duration times an f64 is rounded to the microsecond, a tie to even (OPS-10)") {
    // core's case reverse_scaling: `mul_(10s, 0.5) == 5s`.
    CHECK(hglrt::mul__duration_f64(10s, 0.5) == 5s);
    // "`1us * 0.5`; `3us * 0.5`": `0us`; `2us`.
    CHECK(hglrt::mul__duration_f64(1us, 0.5) == 0us);
    CHECK(hglrt::mul__duration_f64(3us, 0.5) == 2us);
    CHECK(hglrt::mul__duration_f64(5us, 0.5) == 2us);
    CHECK(hglrt::mul__duration_f64(-3us, 0.5) == -2us);
    CHECK(hglrt::mul__duration_f64(3us, -0.5) == -2us);
    CHECK(hglrt::mul__duration_f64(-5us, -0.5) == 2us);
    // Not a tie: the exact product of 0.1, a little more than a tenth.
    CHECK(hglrt::mul__duration_f64(5us, 0.1) == 1us);
    CHECK(hglrt::mul__duration_f64(1s, 1.5) == 1500ms);
    CHECK(hglrt::mul__duration_f64(1us, 0x1p-1074) == 0us);
    CHECK(hglrt::mul__duration_f64(1us, -0.0) == 0us);
    CHECK(hglrt::mul__duration_f64(duration{least}, 1.0) == duration{least});
    CHECK(hglrt::mul__duration_f64(duration{1}, 0x1p62) == duration{i64{1} << 62});
    // "A duration times an infinity or a NaN": numeric.out_of_range; so is a
    // product outside the range.
    CHECK(code_raised([] { (void)hglrt::mul__duration_f64(1s, infinity); }) == "numeric.out_of_range");
    CHECK(code_raised([] { (void)hglrt::mul__duration_f64(0s, not_a_number); }) == "numeric.out_of_range");
    CHECK(code_raised([] { (void)hglrt::mul__duration_f64(duration{least}, -1.0); }) ==
          "numeric.out_of_range");
    CHECK(code_raised([] { (void)hglrt::mul__duration_f64(duration{1}, 0x1p63); }) == "numeric.out_of_range");
    CHECK(code_raised([] { (void)hglrt::mul__duration_f64(1s, 1e300); }) == "numeric.out_of_range");
}

TEST_CASE("a duration divided by a number is rounded to the microsecond (OPS-10, NUM-12)") {
    // "`5m / 3`": `100s`, exactly; "`1us / 3`; `2us / 3`": `0us`; `1us`.
    CHECK(hglrt::div__duration_i64(5min, 3) == 100s);
    CHECK(hglrt::div__duration_i64(1us, 3) == 0us);
    CHECK(hglrt::div__duration_i64(2us, 3) == 1us);
    CHECK(hglrt::div__duration_i64(-2us, 3) == -1us);
    // A tie goes to the even one.
    CHECK(hglrt::div__duration_i64(1us, 2) == 0us);
    CHECK(hglrt::div__duration_i64(3us, 2) == 2us);
    CHECK(hglrt::div__duration_i64(-3us, -2) == 2us);
    CHECK(hglrt::div__duration_i64(duration{least}, 1) == duration{least});
    CHECK(code_raised([] { (void)hglrt::div__duration_i64(duration{least}, -1); }) == "numeric.out_of_range");
    CHECK(hglrt::div__duration_f64(5s, 2.0) == 2500ms);
    CHECK(hglrt::div__duration_f64(3us, 2.0) == 2us);
    CHECK(hglrt::div__duration_f64(1us, 3.0) == 0us);
    CHECK(hglrt::div__duration_f64(1us, 1e300) == 0us);
    CHECK(hglrt::div__duration_f64(-3us, 0.5) == -6us);
    CHECK(hglrt::div__duration_f64(1us, 0x1p-62) == duration{i64{1} << 62});
    CHECK(code_raised([] { (void)hglrt::div__duration_f64(1us, 0x1p-63); }) == "numeric.out_of_range");
    CHECK(code_raised([] { (void)hglrt::div__duration_f64(1s, 0x1p-1074); }) == "numeric.out_of_range");
    // "So is a `duration` multiplied or divided by a NaN or an infinity."
    CHECK(code_raised([] { (void)hglrt::div__duration_f64(1s, infinity); }) == "numeric.out_of_range");
    CHECK(code_raised([] { (void)hglrt::div__duration_f64(1s, not_a_number); }) == "numeric.out_of_range");
    // "`5m / 0`": numeric.division_by_zero, and `.error` an Err of it; a
    // divisor that is not zero gives `ok`, whatever then fails.
    CHECK(code_raised([] { (void)hglrt::div__duration_i64(5min, 0); }) == "numeric.division_by_zero");
    CHECK(code_raised([] { (void)hglrt::div__duration_f64(5min, -0.0); }) == "numeric.division_by_zero");
    CHECK(hglrt::div__duration_i64<ZeroPolicy::error>(5min, 0).error().code == "numeric.division_by_zero");
    CHECK(hglrt::div__duration_f64<ZeroPolicy::error>(5min, 0.0).error().code == "numeric.division_by_zero");
    CHECK(*hglrt::div__duration_i64<ZeroPolicy::error>(5min, 3) == 100s);
    CHECK(code_raised([] { (void)hglrt::div__duration_f64<ZeroPolicy::error>(5min, infinity); }) ==
          "numeric.out_of_range");
}

TEST_CASE("two durations divided (OPS-1, NUM-12)") {
    // `5m / 2m` is an f64; "`5m / 0s`": positive infinity.
    CHECK(hglrt::div__duration_duration(5min, 2min) == 2.5);
    CHECK(hglrt::div__duration_duration(1us, 3us) == 1.0 / 3.0);
    CHECK(hglrt::div__duration_duration(5min, 0s) == infinity);
    CHECK(hglrt::div__duration_duration(-5min, 0s) == -infinity);
    CHECK(std::isnan(hglrt::div__duration_duration(0s, 0s)));
    CHECK(std::isnan(hglrt::div__duration_duration<ZeroPolicy::nan>(5min, 0s)));
    CHECK(code_raised([] { (void)hglrt::div__duration_duration<ZeroPolicy::fail>(5min, 0s); }) ==
          "numeric.division_by_zero");
    CHECK(hglrt::div__duration_duration<ZeroPolicy::error>(5min, 0s).error().code ==
          "numeric.division_by_zero");
    // `//` is the whole count, toward negative infinity; "`5m // 0s`":
    // numeric.division_by_zero.
    CHECK(hglrt::floordiv__duration_duration(5min, 2min) == 2);
    CHECK(hglrt::floordiv__duration_duration(-5min, 2min) == -3);
    CHECK(code_raised([] { (void)hglrt::floordiv__duration_duration(5min, 0s); }) ==
          "numeric.division_by_zero");
    CHECK(hglrt::floordiv__duration_duration<ZeroPolicy::error>(5min, 0s).error().code ==
          "numeric.division_by_zero");
    CHECK(code_raised([] { (void)hglrt::floordiv__duration_duration(duration{least}, -1us); }) ==
          "numeric.out_of_range");
}

TEST_CASE("an instant moved by a duration stays in the years 1 to 9999 (OPS-10)") {
    // core's case reverse_instant_addition.
    const hglrt::datetime instant = sys_days{year{2026} / 9 / 3} + 9h + 30min;
    CHECK(hglrt::add__datetime_duration(instant, 5min) == instant + 5min);
    CHECK(hglrt::sub__datetime_duration(instant, 5min) == instant - 5min);
    CHECK(hglrt::add__datetime_duration(last_instant - microseconds{5}, microseconds{5}) == last_instant);
    CHECK(hglrt::sub__datetime_duration(first_instant + 1us, 1us) == first_instant);
    // "`@9999-12-31T23:59:59Z + 1d`": numeric.out_of_range.
    CHECK(code_raised([] {
              (void)hglrt::add__datetime_duration(sys_days{year{9999} / 12 / 31} + 23h + 59min + 59s, 24h);
          }) == "numeric.out_of_range");
    CHECK(code_raised([] { (void)hglrt::add__datetime_duration(last_instant, microseconds{1}); }) ==
          "numeric.out_of_range");
    CHECK(code_raised([] { (void)hglrt::add__datetime_duration(first_instant, -microseconds{1}); }) ==
          "numeric.out_of_range");
    CHECK(code_raised([] { (void)hglrt::sub__datetime_duration(first_instant, microseconds{1}); }) ==
          "numeric.out_of_range");
    CHECK(code_raised([] {
              (void)hglrt::add__datetime_duration(hglrt::datetime{microseconds{1}}, duration{most});
          }) == "numeric.out_of_range");
    CHECK(code_raised([] { (void)hglrt::sub__datetime_duration(first_instant, duration{least}); }) ==
          "numeric.out_of_range");
    // Two instants: the duration from one to the other.
    CHECK(hglrt::sub__datetime_datetime(instant + 90s, instant) == 90s);
    CHECK(hglrt::sub__datetime_datetime(instant, instant + 90s) == -90s);
    CHECK(hglrt::sub__datetime_datetime(last_instant, first_instant) == last_instant - first_instant);
    CHECK(code_raised([] {
              (void)hglrt::sub__datetime_datetime(hglrt::datetime{duration{most}}, hglrt::datetime{-1us});
          }) == "numeric.out_of_range");
}

TEST_CASE("a date and a duration give a civil datetime (OPS-1, OPS-10)") {
    const year_month_day day = year{2026} / 9 / 3;
    // "`@2026-09-03 + 2d`": `@2026-09-05T00:00`; "`@2026-09-03 + 36h`":
    // `@2026-09-04T12:00`; "`@2026-09-03 - 1h`": `@2026-09-02T23:00`.
    CHECK(hglrt::add__date_duration(day, 48h) == civil(year{2026} / 9 / 5));
    CHECK(hglrt::add__date_duration(day, 36h) == civil(year{2026} / 9 / 4, 12));
    CHECK(hglrt::sub__date_duration(day, 1h) == civil(year{2026} / 9 / 2, 23));
    CHECK(hglrt::add__date_duration(day, 0us) == civil(day));
    CHECK(hglrt::add__date_duration(year{9999} / 12 / 31, 24h - 1us) ==
          hglrt::civil_datetime{year{9999} / 12 / 31, hgraph::time_of_day(23, 59, 59, 999'999)});
    CHECK(code_raised([] { (void)hglrt::add__date_duration(year{9999} / 12 / 31, 24h); }) ==
          "numeric.out_of_range");
    CHECK(code_raised([] { (void)hglrt::sub__date_duration(year{1} / 1 / 1, 1us); }) ==
          "numeric.out_of_range");
    CHECK(code_raised([] { (void)hglrt::add__date_duration(year{2026} / 9 / 3, duration{most}); }) ==
          "numeric.out_of_range");
    // `@2026-09-04 - @2026-09-03` is a duration, whole days.
    CHECK(hglrt::sub__date_date(year{2026} / 9 / 4, year{2026} / 9 / 3) == 24h);
    CHECK(hglrt::sub__date_date(year{2026} / 3 / 1, year{2026} / 2 / 1) == days{28});
    CHECK(hglrt::sub__date_date(year{1} / 1 / 1, year{9999} / 12 / 31) == -days{3'652'058});
    // core's case reverse_date_time_addition: `@2026-09-03 + @09:30`.
    CHECK(hglrt::add__date_time(day, hgraph::time_of_day(9, 30)) == civil(day, 9, 30));
    CHECK(hglrt::add__date_time(year{9999} / 12 / 31, hgraph::time_of_day(23, 59, 59, 999'999)) ==
          hglrt::add__date_duration(year{9999} / 12 / 31, 24h - 1us));
}

TEST_CASE("a civil datetime is moved on its own clock, in the years 1 to 9999 (OPS-10)") {
    const hglrt::civil_datetime morning = civil(year{2026} / 9 / 3, 9, 30);
    CHECK(hglrt::add__civil_datetime_duration(morning, 5min) == civil(year{2026} / 9 / 3, 9, 35));
    CHECK(hglrt::sub__civil_datetime_duration(morning, 10h) == civil(year{2026} / 9 / 2, 23, 30));
    CHECK(hglrt::sub__civil_datetime_civil_datetime(civil(year{2026} / 9 / 4), morning) == 14h + 30min);
    CHECK(hglrt::sub__civil_datetime_civil_datetime(morning, civil(year{2026} / 9 / 4)) == -(14h + 30min));
    const hglrt::civil_datetime latest = hglrt::add__date_duration(year{9999} / 12 / 31, 24h - 1us);
    CHECK(code_raised([&] { (void)hglrt::add__civil_datetime_duration(latest, 1us); }) ==
          "numeric.out_of_range");
    CHECK(code_raised([] { (void)hglrt::sub__civil_datetime_duration(civil(year{1} / 1 / 1), 1us); }) ==
          "numeric.out_of_range");
    CHECK(code_raised([&] { (void)hglrt::sub__civil_datetime_duration(latest, duration{least}); }) ==
          "numeric.out_of_range");
}

TEST_CASE("a duration moves a zoned datetime's instant, in its zone (OPS-10, TYPE-2)") {
    const auto zones = hgraph::make_time_zone_provider();
    const hgraph::ZoneId london{"Europe/London"};
    const hgraph::ZoneId utc{"UTC"};
    const auto at = [&](hglrt::datetime instant, hgraph::ZoneId zone) {
        return hgraph::at_zone(instant, zone, *zones);
    };
    // London's clocks go forward an hour at 01:00 UTC on 29 March 2026.
    // "`@2026-03-28T12:00+00:00[Europe/London] + 1d`":
    // `@2026-03-29T13:00+01:00[Europe/London]`, 86,400 seconds later.
    const hglrt::zoned_datetime before = at(sys_days{year{2026} / 3 / 28} + 12h, london);
    const hglrt::zoned_datetime after = hglrt::add__zoned_datetime_duration(before, 24h);
    CHECK(after.instant() == before.instant() + 24h);
    CHECK(after.zone() == london);
    CHECK(after.offset_seconds() == 3600);
    CHECK(after.civil() == civil(year{2026} / 3 / 29, 13));
    // "`@2026-03-29T00:30+00:00[Europe/London] + 1h`":
    // `@2026-03-29T02:30+01:00[Europe/London]`.
    const hglrt::zoned_datetime half_past = at(sys_days{year{2026} / 3 / 29} + 30min, london);
    CHECK(hglrt::add__zoned_datetime_duration(half_past, 1h).civil() == civil(year{2026} / 3 / 29, 2, 30));
    CHECK(hglrt::sub__zoned_datetime_duration(after, 24h) == before);
    // core's case reverse_instant_addition:
    // `@2026-09-03T09:30+01:00[Europe/London] + 5m`.
    const hglrt::zoned_datetime summer = at(sys_days{year{2026} / 9 / 3} + 8h + 30min, london);
    CHECK(hglrt::add__zoned_datetime_duration(summer, 5min).civil() == civil(year{2026} / 9 / 3, 9, 35));
    // The instant stays in the years 1 to 9999, and so does its wall clock.
    CHECK(code_raised([&] { (void)hglrt::add__zoned_datetime_duration(at(last_instant, utc), 1us); }) ==
          "numeric.out_of_range");
    const hglrt::zoned_datetime tokyo = at(last_instant - 10h, hgraph::ZoneId{"Asia/Tokyo"});
    CHECK(code_raised([&] { (void)hglrt::add__zoned_datetime_duration(tokyo, 2h); }) ==
          "numeric.out_of_range");
}

TEST_CASE("zoned datetimes are equal by instant and zone, or as UTC by instant (OPS-2, TYPE-2)") {
    const auto zones = hgraph::make_time_zone_provider();
    const hglrt::datetime instant = sys_days{year{2026} / 9 / 3} + 8h + 30min;
    const hglrt::zoned_datetime london = hgraph::at_zone(instant, hgraph::ZoneId{"Europe/London"}, *zones);
    const hglrt::zoned_datetime utc = hgraph::at_zone(instant, hgraph::ZoneId{"UTC"}, *zones);
    // "`... [Europe/London] == ... [UTC]`": false; "`=={utc: true}`": true.
    CHECK_FALSE(hglrt::eq__zoned_datetime_zoned_datetime_bool(london, utc));
    CHECK(hglrt::ne__zoned_datetime_zoned_datetime_bool(london, utc));
    CHECK(hglrt::eq__zoned_datetime_zoned_datetime_bool(london, utc, true));
    CHECK_FALSE(hglrt::ne__zoned_datetime_zoned_datetime_bool(london, utc, true));
    CHECK(hglrt::eq__zoned_datetime_zoned_datetime_bool(london, london));
    // Two names of one zone are two zones.
    const hglrt::zoned_datetime alias = hgraph::at_zone(instant, hgraph::ZoneId{"GB"}, *zones);
    CHECK_FALSE(hglrt::eq__zoned_datetime_zoned_datetime_bool(london, alias));
    CHECK(hglrt::eq__zoned_datetime_zoned_datetime_bool(london, alias, true));
    const hglrt::zoned_datetime later = hglrt::add__zoned_datetime_duration(london, 1us);
    CHECK_FALSE(hglrt::eq__zoned_datetime_zoned_datetime_bool(london, later, true));
}

namespace {
/// The message of the HGL failure `call` raises, and whether it has a code.
template <class Call>
std::optional<std::string> uncoded_message(Call&& call) {
    try {
        call();
    } catch (const hglrt::Failure& failure) {
        CHECK_FALSE(failure.error().code.has_value());
        return failure.error().message;
    }
    return std::nullopt;
}
} // namespace

TEST_CASE("a zone literal is checked by the provider's catalogue, and its offset by the zone (LEX-12)") {
    const std::string absent = "the zone `Nowhere/Atlantis` is not in the run provider's zone catalogue";
    CHECK(uncoded_message([] { (void)hglrt::zone_literal("Nowhere/Atlantis"); }) == absent);
    CHECK(uncoded_message([] {
              (void)hglrt::zoned_time_literal(hgraph::time_of_day(9, 30), "Nowhere/Atlantis");
          }) == absent);
    const hglrt::datetime instant = sys_days{year{2026} / 9 / 3} + 8h + 30min;
    CHECK(uncoded_message([&] { (void)hglrt::zoned_datetime_literal(instant, 3600, "Nowhere/Atlantis"); }) ==
          absent);
    // `@2026-09-03T09:30+01:00[Europe/London]` is the zone's own offset;
    // `@2026-09-03T08:30+00:00[Europe/London]` is not.
    const hglrt::zoned_datetime london = hglrt::zoned_datetime_literal(instant, 3600, "Europe/London");
    CHECK(london ==
          hgraph::at_zone(instant, hgraph::ZoneId{"Europe/London"}, *hgraph::make_time_zone_provider()));
    CHECK(uncoded_message([&] { (void)hglrt::zoned_datetime_literal(instant, 0, "Europe/London"); }) ==
          "the offset of `2026-09-03T08:30:00` is not the offset of the zone `Europe/London` at that time");
    CHECK(uncoded_message([&] { (void)hglrt::zoned_datetime_literal(instant + 7us, 0, "Europe/London"); }) ==
          "the offset of `2026-09-03T08:30:00.000007` is not the offset of the zone `Europe/London` at that "
          "time");
    CHECK(hglrt::zone_literal("Europe/London") == hgraph::ZoneId{"Europe/London"});
    // An alias the catalogue has is a zone of its own name.
    CHECK(hglrt::zone_literal("GB") == hgraph::ZoneId{"GB"});
    CHECK(hglrt::zoned_time_literal(hgraph::time_of_day(9, 30), "UTC").zone() == hgraph::ZoneId{"UTC"});
}
