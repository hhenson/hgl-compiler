// Text of a runtime instant or wall clock. Private to libhglrt.
#pragma once

#include <hglrt/values.h>

#include <chrono>
#include <format>
#include <string>

namespace hglrt::detail {

/// A time of day of `microseconds` from midnight (LEX-11). Compact text
/// omits empty seconds and trims fractional zeros, as HGL writes it.
inline std::string clock_text(i64 microseconds, bool compact = true) {
    const i64 minutes = microseconds / 60'000'000;
    const i64 rest = microseconds % 60'000'000;
    std::string text = std::format("{:02}:{:02}", minutes / 60, minutes % 60);
    if (rest != 0 || !compact) {
        text += std::format(":{:02}", rest / 1'000'000);
        if (const i64 fraction = rest % 1'000'000; fraction != 0) {
            std::string digits = std::format("{:06}", fraction);
            if (compact) {
                digits.erase(digits.find_last_not_of('0') + 1);
            }
            text += "." + digits;
        }
    }
    return text;
}

/// The places that write a date and time, each with its own text contract.
enum class InstantFormat { compact, publication, local, failure };

/// A civil date and clock without a zone suffix. Application results and
/// offset diagnostics keep seconds and a full fraction when nonzero.
/// Failure context keeps the chrono evaluation stamp's full precision.
inline std::string instant_text(datetime at, InstantFormat form) {
    if (form == InstantFormat::failure) {
        return std::format("{:%Y-%m-%dT%H:%M:%S}", at);
    }
    const std::chrono::year_month_day day{std::chrono::floor<std::chrono::days>(at)};
    // Modulo finds the time of day without subtracting an instant for the
    // preceding midnight, which can fall outside the clock's range.
    const auto into = at.time_since_epoch() % std::chrono::days{1};
    const auto clock = into < duration::zero() ? into + std::chrono::days{1} : into;
    const std::string calendar = form == InstantFormat::local
        ? std::format("{:04}-{:02}-{:02}", static_cast<int>(day.year()),
                      static_cast<unsigned>(day.month()), static_cast<unsigned>(day.day()))
        : std::format("{:%F}", day);
    return calendar + "T" + clock_text(clock.count(), form == InstantFormat::compact);
}

} // namespace hglrt::detail
