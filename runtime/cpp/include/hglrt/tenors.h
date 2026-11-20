// Core's native tenor operations (`hgl/native/tenors.hgl`, core's tenor
// contract TEN-1 to TEN-9): a tenor's arithmetic, order and text, and the
// parts core's own calendar candidates (`hgl/core/calendar.hgl`) are
// written with. Each is named as scalar.h says.
//
// A tenor is hgraph's `Tenor` (TYPE-17; hgraph's RFC 0051): "a count of
// months and a count of days, or ... a count of business days; each count
// is a signed 64-bit integer". hgraph keeps its counts, its form, its
// equality and its hash, and nothing more; its arithmetic, order, text and
// what it does to a date are these, written from core's contract.
#pragma once

#include <hglrt/failure.h>
#include <hglrt/scalar.h>
#include <hglrt/values.h>

#include <concepts>
#include <optional>
#include <source_location>

namespace hglrt {

// ---- A tenor's arithmetic (TEN-2) ----------------------------------------
//
// TEN-2: "A sum, a difference, a negation, and a scaling apply to each count.
// The operands of a sum or a difference are of one form: a tenor of business
// days with one of months and days is an HGL error with code
// `tenor.mixed_units`. A count outside the i64 range is an HGL error with
// code `numeric.out_of_range`."

/// `add_(lhs: tenor, rhs: tenor) -> tenor`.
tenor add__tenor_tenor(const tenor& lhs, const tenor& rhs,
                       std::source_location where = std::source_location::current());

/// `sub_(lhs: tenor, rhs: tenor) -> tenor`.
tenor sub__tenor_tenor(const tenor& lhs, const tenor& rhs,
                       std::source_location where = std::source_location::current());

/// `neg_(value: tenor) -> tenor`.
tenor neg__tenor(const tenor& value, std::source_location where = std::source_location::current());

/// `mul_(lhs: tenor, rhs: i64) -> tenor`. Core's HGL candidate takes the
/// operands the other way round.
tenor mul__tenor_i64(const tenor& lhs, i64 rhs, std::source_location where = std::source_location::current());

// ---- A tenor's order (TEN-3) ---------------------------------------------
//
// TEN-3: "The order is count by count. One tenor is less than or equal to
// another when they are of one form and no count of it is greater. `<=>`
// gives `equivalent` for two equal tenors; `less` or `greater` where each
// count that differs differs that way; and `unordered` otherwise, two
// tenors of different forms included. `<`, `<=`, `>`, and `>=` are false
// for an unordered pair."

/// `compare_(lhs: tenor, rhs: tenor) -> CmpResult`.
CmpResult compare__tenor_tenor(const tenor& lhs, const tenor& rhs);

/// `lt_(lhs: tenor, rhs: tenor) -> bool`.
bool lt__tenor_tenor(const tenor& lhs, const tenor& rhs);

/// `le_(lhs: tenor, rhs: tenor) -> bool`.
bool le__tenor_tenor(const tenor& lhs, const tenor& rhs);

/// `gt_(lhs: tenor, rhs: tenor) -> bool`.
bool gt__tenor_tenor(const tenor& lhs, const tenor& rhs);

/// `ge_(lhs: tenor, rhs: tenor) -> bool`.
bool ge__tenor_tenor(const tenor& lhs, const tenor& rhs);

// ---- A tenor's text (TEN-9) ----------------------------------------------

namespace detail {
/// TEN-9: "a tenor's text, which uses the largest units: its months as `Y`
/// and `M`, its days as `W` and `D`, or its business days as `B`. A unit
/// whose count is zero is not written; a tenor of no months and no days is
/// `0D`. A count below zero has `-` before its units": `1Y6M`, `1M-1D`,
/// `-2B`.
str tenor_text(const tenor& value);
} // namespace detail

/// `convert(value: tenor) -> str` (OPS-6, TEN-9). It is a template of the
/// type converted to, as conversions.h has `convert`, whose one argument is
/// `str` and is given where it is not written.
template <class To = str>
To convert_tenor(const tenor& value) {
    static_assert(std::same_as<To, str>, "OPS-4: a tenor converts to a str");
    return detail::tenor_text(value);
}

// ---- The parts core's calendar candidates are written with ---------------

/// `business_days(value: tenor) -> i64?` (TEN-1): the count of a tenor of
/// business days, and null for one of months and days.
std::optional<i64> fn_business_days(const tenor& value);

/// `moved(day: date, by: tenor) -> date` (TEN-4): "A date moved by a tenor of
/// months and days takes the months first: the same day of the month that
/// many months on, or the last day of that month where it has no such day.
/// Then it takes the days. A count below zero moves back ... A result outside
/// the `date` range is an HGL error with code `numeric.out_of_range`", and so
/// is the date the months give, as the reference provider has it. Core calls
/// it with a tenor of months and days alone: one of business days, which
/// has neither, leaves the date as it is.
date fn_moved(date day, const tenor& by, std::source_location where = std::source_location::current());

/// `weekend(day: date) -> bool` (TEN-5): true for a Saturday and for a
/// Sunday.
bool fn_weekend(date day);

/// `same_month(lhs: date, rhs: date) -> bool` (TEN-8): true where the two
/// dates are in one month of one year.
bool fn_same_month(date lhs, date rhs);

} // namespace hglrt
