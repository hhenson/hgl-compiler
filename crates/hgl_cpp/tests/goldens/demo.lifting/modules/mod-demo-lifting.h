// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#pragma once

#include <hglrt/hglrt.h>

// Names this file writes, which no macro of a header above takes.
#undef a
#undef attempt
#undef b
#undef branch_widened
#undef by
#undef captured_pick
#undef captured_shown
#undef check
#undef checked
#undef checked_errors
#undef command
#undef demo
#undef describe
#undef error
#undef errors_of
#undef factor
#undef failure
#undef first
#undef flag
#undef guarded
#undef guarded_values
#undef handled
#undef kept_time
#undef lifting
#undef low
#undef multiplied
#undef offset
#undef or_zero
#undef or_zero_of
#undef outcome
#undef outcomes
#undef pick
#undef picked
#undef picked_validity
#undef produced
#undef provenance
#undef raise_again
#undef raised
#undef relayed
#undef repeat
#undef repeated
#undef returned
#undef sample
#undef scaled
#undef scaled_by
#undef shifted
#undef shifted_values
#undef shown
#undef shown_five
#undef shown_of
#undef taken
#undef validity
#undef value
#undef widened_node_result
#undef widened_result
#undef widened_validity
#undef write_then_fail
#undef written_values
#undef x
#undef y

namespace hgl::demo::lifting::m_
{

// scaled(value: i64, factor: i64 = ..., offset: i64 = ...) -> i64 (`const fn`), module demo.lifting, line 8
::hgraph::Int scaled(::hgraph::Int value, ::hgraph::Int factor, ::hgraph::Int offset);

// multiplied(value: i64, const factor: i64) -> i64 (`const fn`), module demo.lifting, line 12
::hgraph::Int multiplied(::hgraph::Int value, ::hgraph::Int factor);

namespace w_
{
// scaled_by(value: i64, factor: i64) -> i64 (graph), module demo.lifting, line 16
::hgraph::Port<::hgraph::TS<::hgraph::Int>> scaled_by(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> factor);
}  // namespace w_

// check(value: i64) (`const fn`), module demo.lifting, line 22
void check(::hgraph::Int value);

namespace w_
{
// checked(value: i64) (graph), module demo.lifting, line 26
void checked(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// describe(failure: error, sample: i64) -> str (node), module demo.lifting, line 31
::hgraph::Port<::hgraph::TS<::hgraph::Str>> describe(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hglrt::error>> failure, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> sample);
}  // namespace w_

namespace w_
{
// checked_errors(value: i64) -> str (graph), module demo.lifting, line 43
::hgraph::Port<::hgraph::TS<::hgraph::Str>> checked_errors(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

// attempt(value: i64) -> result<i64, error> (`const fn`), module demo.lifting, line 50
::hglrt::result<::hgraph::Int> attempt(::hgraph::Int value);

namespace w_
{
// raised(value: i64) -> i64 (node), module demo.lifting, line 55
::hgraph::Port<::hgraph::TS<::hgraph::Int>> raised(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// returned(value: i64) -> result<i64, error> (node), module demo.lifting, line 59
::hgraph::Port<::hgraph::TS<::hglrt::result<::hgraph::Int>>> returned(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_
namespace w_
{
// returned(value: i64) -> result<i64, error> (node), module demo.lifting, line 59
::hgraph::Port<::hgraph::TS<::hgraph::Int>> returned_result_value_(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_
namespace w_
{
// returned(value: i64) -> result<i64, error> (node), module demo.lifting, line 59
::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> returned_result_nullable_(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// outcome(taken: result<i64, error>) -> str (node), module demo.lifting, line 63
::hgraph::Port<::hgraph::TS<::hgraph::Str>> outcome(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hglrt::result<::hgraph::Int>>> taken);
}  // namespace w_

namespace w_
{
// outcomes(value: i64) -> str (graph), module demo.lifting, line 70
::hgraph::Port<::hgraph::TS<::hgraph::Str>> outcomes(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// shifted(value: i64, const by: i64) -> i64 (node), module demo.lifting, line 75
::hgraph::Port<::hgraph::TS<::hgraph::Int>> shifted(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value, ::hgraph::Int by);
}  // namespace w_

namespace w_
{
// shifted_values(value: i64) -> i64 (graph), module demo.lifting, line 79
::hgraph::Port<::hgraph::TS<::hgraph::Int>> shifted_values(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// guarded(value: i64) -> i64 (graph), module demo.lifting, line 84
::hgraph::Port<::hgraph::TS<::hgraph::Int>> guarded(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// guarded_values(value: i64) -> i64 (graph), module demo.lifting, line 88
::hgraph::Port<::hgraph::TS<::hgraph::Int>> guarded_values(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// write_then_fail(value: i64) -> i64 (node), module demo.lifting, line 94
::hgraph::Port<::hgraph::TS<::hgraph::Int>> write_then_fail(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// written_values(value: i64) -> i64 (graph), module demo.lifting, line 103
::hgraph::Port<::hgraph::TS<::hgraph::Int>> written_values(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// raise_again(first: error, repeat: bool) -> error (node), module demo.lifting, line 110
::hgraph::Port<::hgraph::TS<::hglrt::error>> raise_again(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hglrt::error>> first, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> repeat);
}  // namespace w_

namespace w_
{
// handled(value: i64, repeat: bool) -> error (graph), module demo.lifting, line 118
::hgraph::Port<::hgraph::TS<::hglrt::error>> handled(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> repeat);
}  // namespace w_

namespace w_
{
// kept_time(first: error, repeated: error) -> datetime (node), module demo.lifting, line 123
::hgraph::Port<::hgraph::TS<::hgraph::DateTime>> kept_time(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hglrt::error>> first, ::hgraph::Port<::hgraph::TS<::hglrt::error>> repeated);
}  // namespace w_

namespace w_
{
// provenance(value: i64, repeat: bool) -> datetime (graph), module demo.lifting, line 133
::hgraph::Port<::hgraph::TS<::hgraph::DateTime>> provenance(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> repeat);
}  // namespace w_

namespace w_
{
// errors_of(value: i64) -> error (graph), module demo.lifting, line 179
::hgraph::Port<::hgraph::TS<::hglrt::error>> errors_of(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

// or_zero(a: i64, b: i64?) -> i64 (`const fn`), module demo.lifting, line 194
::hgraph::Int or_zero(::hgraph::Int a, ::std::optional<::hgraph::Int> b);

namespace w_
{
// or_zero_of(a: i64, b: i64) -> i64 (graph), module demo.lifting, line 200
::hgraph::Port<::hgraph::TS<::hgraph::Int>> or_zero_of(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> a, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> b);
}  // namespace w_

namespace w_
{
// shown(x: i64?) -> str (node), module demo.lifting, line 202
::hgraph::Port<::hgraph::TS<::hgraph::Str>> shown(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> x);
}  // namespace w_

namespace w_
{
// relayed(x: i64) -> i64 (node), module demo.lifting, line 210
::hgraph::Port<::hgraph::TS<::hgraph::Int>> relayed(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x);
}  // namespace w_

namespace w_
{
// shown_of(x: i64) -> str (graph), module demo.lifting, line 214
::hgraph::Port<::hgraph::TS<::hgraph::Str>> shown_of(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x);
}  // namespace w_

namespace w_
{
// shown_five(flag: bool) -> str (graph), module demo.lifting, line 216
::hgraph::Port<::hgraph::TS<::hgraph::Str>> shown_five(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> flag);
}  // namespace w_

namespace w_
{
// widened_result(x: i64) -> i64? (graph), module demo.lifting, line 218
::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> widened_result(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x);
}  // namespace w_

namespace w_
{
// widened_node_result(x: i64) -> i64? (graph), module demo.lifting, line 220
::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> widened_node_result(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x);
}  // namespace w_

namespace w_
{
// produced(command: i64) -> i64 (node), module demo.lifting, line 225
::hgraph::Port<::hgraph::TS<::hgraph::Int>> produced(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> command);
}  // namespace w_

namespace w_
{
// validity(x: i64?) -> bool (node), module demo.lifting, line 233
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> validity(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> x);
}  // namespace w_

namespace w_
{
// widened_validity(command: i64) -> bool (graph), module demo.lifting, line 237
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> widened_validity(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> command);
}  // namespace w_

namespace w_
{
// branch_widened(flag: bool, x: i64, y: i64) -> i64? (graph), module demo.lifting, line 239
::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> branch_widened(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> flag, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> y);
}  // namespace w_

namespace w_
{
// captured_shown(x: i64) -> str (graph), module demo.lifting, line 244
::hgraph::Port<::hgraph::TS<::hgraph::Str>> captured_shown(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x);
}  // namespace w_

namespace w_
{
// pick(flag: bool, ref low: i64?) -> ref i64? (node), module demo.lifting, line 252
::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> pick(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> flag, ::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> low);
}  // namespace w_

namespace w_
{
// picked(flag: bool, low: i64) -> i64? (graph), module demo.lifting, line 256
::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> picked(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> flag, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> low);
}  // namespace w_

namespace w_
{
// picked_validity(flag: bool, command: i64) -> bool (graph), module demo.lifting, line 258
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> picked_validity(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> flag, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> command);
}  // namespace w_

namespace w_
{
// captured_pick(flag: bool, low: i64) -> str (graph), module demo.lifting, line 260
::hgraph::Port<::hgraph::TS<::hgraph::Str>> captured_pick(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> flag, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> low);
}  // namespace w_

}  // namespace hgl::demo::lifting::m_
