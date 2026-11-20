// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#pragma once

#include <hglrt/hglrt.h>

// Names this file writes, which no macro of a header above takes.
#undef attempt
#undef by
#undef check
#undef checked
#undef checked_errors
#undef demo
#undef describe
#undef error
#undef factor
#undef failure
#undef first
#undef guarded
#undef guarded_values
#undef handled
#undef kept_time
#undef lifting
#undef multiplied
#undef offset
#undef outcome
#undef outcomes
#undef provenance
#undef raise_again
#undef raised
#undef repeat
#undef repeated
#undef returned
#undef sample
#undef scaled
#undef scaled_by
#undef shifted
#undef shifted_values
#undef taken
#undef value
#undef write_then_fail
#undef written_values

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

}  // namespace hgl::demo::lifting::m_
