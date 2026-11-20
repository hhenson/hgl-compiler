// Core's native scalar operators of text (`hgl/native/scalar_operators.hgl`,
// core's scalar contract OPS-1): concatenation and repetition of `str`.
// Their order and equality are the generic candidates' (scalar.h), and the
// conversions of text and bytes are in conversions.h. Each is named as
// scalar.h says.
#pragma once

#include <hglrt/values.h>

#include <source_location>

namespace hglrt {

/// `add_(lhs: str, rhs: str) -> str`: `lhs` and then `rhs` (OPS-1).
str add__str_str(const str& lhs, const str& rhs);

/// `mul_(lhs: str, rhs: i64) -> str` (OPS-1): "A `str` and an `i64` give
/// the text repeated that many times, the empty text for a count of zero; a
/// count below zero is an HGL error with code `numeric.out_of_range`." Core's
/// HGL candidate takes the operands the other way round. A count whose text
/// no string can hold is an internal fault of the library, an
/// `std::length_error`, and no HGL failure (ERR-1).
str mul__str_i64(const str& lhs, i64 rhs, std::source_location where = std::source_location::current());

} // namespace hglrt
