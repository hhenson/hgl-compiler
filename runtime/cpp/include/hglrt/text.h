// Core's native scalar operators of text (`hgl/native/scalar_operators.hgl`,
// core's scalar contract OPS-1): concatenation and repetition of `str`.
// Their order and equality are the generic candidates' (scalar.h), and the
// conversions of text and bytes are in conversions.h. Each is named as
// scalar.h says.
#pragma once

#include <hglrt/values.h>

namespace hglrt {

/// `add_(lhs: str, rhs: str) -> str`: `lhs` and then `rhs` (OPS-1).
str add__str_str(const str& lhs, const str& rhs);

/// `mul_(lhs: str, rhs: i64) -> str`: `lhs` `rhs` times over, none for a
/// count of zero (OPS-1). Core's HGL candidate takes the operands the other
/// way round. Core's contract gives no rule for a count below zero, and
/// none is made up: such a count is an internal fault of the library, an
/// `std::domain_error`, and so is a count whose text no string can hold, an
/// `std::length_error`; neither is an HGL failure (ERR-1).
str mul__str_i64(const str& lhs, i64 rhs);

} // namespace hglrt
