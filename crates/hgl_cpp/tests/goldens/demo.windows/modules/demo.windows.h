// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#pragma once

#include <hglrt/hglrt.h>

// Names this file writes, which no macro of a header above takes.
#undef arrival
#undef checked_total
#undef collect
#undef collect_any
#undef collect_recent
#undef collect_spanned
#undef demo
#undef departure
#undef early_total
#undef lifted_total
#undef observe_arrival
#undef observe_checked
#undef observe_departure
#undef observe_readiness
#undef observe_recent_arrival
#undef observe_recent_departure
#undef observe_recent_readiness
#undef observe_recent_validity
#undef observe_spanned
#undef observe_total
#undef observe_validity
#undef outcome
#undef probe
#undef read_or_code
#undef readiness
#undef reading
#undef readings
#undef ready_total
#undef recent_arrival
#undef recent_departure
#undef recent_readiness
#undef recent_validity
#undef spanned_readiness
#undef total
#undef total_at_tick
#undef total_when_ready
#undef total_when_valid
#undef validity
#undef value_or_code
#undef windows

namespace hgl::demo::windows::m_
{

// total(readings: list<i64>) -> i64 (`const fn`), module demo.windows, line 10
::hgraph::Int total(const ::hglrt::list<::hgraph::Int>& readings);

// value_or_code(outcome: result<i64, error>) -> str (`const fn`), module demo.windows, line 17
::hgraph::Str value_or_code(const ::hglrt::result<::hgraph::Int>& outcome);

// read_or_code(outcome: result<list<i64>, error>) -> str (`const fn`), module demo.windows, line 25
::hgraph::Str read_or_code(const ::hglrt::result<::hglrt::list<::hgraph::Int>>& outcome);

namespace w_
{
// collect(reading: i64) -> rolling_window<i64, 3, 2> (node), module demo.windows, line 33
::hgraph::Port<::hgraph::TSW<::hgraph::Int, 3, 2>> collect(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading);
}  // namespace w_

namespace w_
{
// collect_recent(reading: i64) -> rolling_window<i64, 3us, 1us> (node), module demo.windows, line 38
::hgraph::Port<::hgraph::TSWDuration<::hgraph::Int, 3, 1>> collect_recent(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading);
}  // namespace w_

namespace w_
{
// collect_any(reading: i64) -> rolling_window<i64, 3us, 0s> (node), module demo.windows, line 43
::hgraph::Port<::hgraph::TSWDuration<::hgraph::Int, 3, 0>> collect_any(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading);
}  // namespace w_

namespace w_
{
// validity(probe: bool, readings: rolling_window<i64, 3, 2>) -> bool (node), module demo.windows, line 47
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> validity(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::TSW<::hgraph::Int, 3, 2>> readings);
}  // namespace w_

namespace w_
{
// readiness(probe: bool, readings: rolling_window<i64, 3, 2>) -> bool (node), module demo.windows, line 51
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> readiness(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::TSW<::hgraph::Int, 3, 2>> readings);
}  // namespace w_

namespace w_
{
// observe_validity(probe: bool, reading: i64) -> bool (graph), module demo.windows, line 55
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> observe_validity(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading);
}  // namespace w_

namespace w_
{
// observe_readiness(probe: bool, reading: i64) -> bool (graph), module demo.windows, line 57
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> observe_readiness(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading);
}  // namespace w_

namespace w_
{
// total_when_ready(readings: rolling_window<i64, 3, 2>) -> i64 (node), module demo.windows, line 66
::hgraph::Port<::hgraph::TS<::hgraph::Int>> total_when_ready(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSW<::hgraph::Int, 3, 2>> readings);
}  // namespace w_

namespace w_
{
// total_when_valid(readings: rolling_window<i64, 3, 2>) -> i64 (node), module demo.windows, line 70
::hgraph::Port<::hgraph::TS<::hgraph::Int>> total_when_valid(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSW<::hgraph::Int, 3, 2>> readings);
}  // namespace w_

namespace w_
{
// ready_total(reading: i64) -> i64 (graph), module demo.windows, line 74
::hgraph::Port<::hgraph::TS<::hgraph::Int>> ready_total(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading);
}  // namespace w_

namespace w_
{
// early_total(reading: i64) -> i64 (graph), module demo.windows, line 76
::hgraph::Port<::hgraph::TS<::hgraph::Int>> early_total(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading);
}  // namespace w_

namespace w_
{
// checked_total(readings: rolling_window<i64, 3, 2>) -> str (node), module demo.windows, line 87
::hgraph::Port<::hgraph::TS<::hgraph::Str>> checked_total(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSW<::hgraph::Int, 3, 2>> readings);
}  // namespace w_

namespace w_
{
// observe_checked(reading: i64) -> str (graph), module demo.windows, line 94
::hgraph::Port<::hgraph::TS<::hgraph::Str>> observe_checked(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading);
}  // namespace w_

namespace w_
{
// lifted_total(reading: i64) -> i64 (graph), module demo.windows, line 102
::hgraph::Port<::hgraph::TS<::hgraph::Int>> lifted_total(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading);
}  // namespace w_

namespace w_
{
// arrival(readings: rolling_window<i64, 3, 2>) -> i64 (node), module demo.windows, line 109
::hgraph::Port<::hgraph::TS<::hgraph::Int>> arrival(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSW<::hgraph::Int, 3, 2>> readings);
}  // namespace w_

namespace w_
{
// departure(readings: rolling_window<i64, 3, 2>) -> str (node), module demo.windows, line 113
::hgraph::Port<::hgraph::TS<::hgraph::Str>> departure(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSW<::hgraph::Int, 3, 2>> readings);
}  // namespace w_

namespace w_
{
// observe_arrival(reading: i64) -> i64 (graph), module demo.windows, line 117
::hgraph::Port<::hgraph::TS<::hgraph::Int>> observe_arrival(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading);
}  // namespace w_

namespace w_
{
// observe_departure(reading: i64) -> str (graph), module demo.windows, line 119
::hgraph::Port<::hgraph::TS<::hgraph::Str>> observe_departure(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading);
}  // namespace w_

namespace w_
{
// recent_readiness(readings: rolling_window<i64, 3us, 1us>) -> bool (node), module demo.windows, line 129
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> recent_readiness(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSWDuration<::hgraph::Int, 3, 1>> readings);
}  // namespace w_

namespace w_
{
// recent_arrival(readings: rolling_window<i64, 3us, 1us>) -> str (node), module demo.windows, line 133
::hgraph::Port<::hgraph::TS<::hgraph::Str>> recent_arrival(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSWDuration<::hgraph::Int, 3, 1>> readings);
}  // namespace w_

namespace w_
{
// recent_departure(readings: rolling_window<i64, 3us, 1us>) -> str (node), module demo.windows, line 137
::hgraph::Port<::hgraph::TS<::hgraph::Str>> recent_departure(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSWDuration<::hgraph::Int, 3, 1>> readings);
}  // namespace w_

namespace w_
{
// observe_recent_readiness(reading: i64) -> bool (graph), module demo.windows, line 141
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> observe_recent_readiness(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading);
}  // namespace w_

namespace w_
{
// observe_recent_arrival(reading: i64) -> str (graph), module demo.windows, line 143
::hgraph::Port<::hgraph::TS<::hgraph::Str>> observe_recent_arrival(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading);
}  // namespace w_

namespace w_
{
// observe_recent_departure(reading: i64) -> str (graph), module demo.windows, line 145
::hgraph::Port<::hgraph::TS<::hgraph::Str>> observe_recent_departure(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading);
}  // namespace w_

namespace w_
{
// collect_spanned(reading: i64) -> rolling_window<i64, 4us, 2us> (node), module demo.windows, line 163
::hgraph::Port<::hgraph::TSWDuration<::hgraph::Int, 4, 2>> collect_spanned(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading);
}  // namespace w_

namespace w_
{
// spanned_readiness(probe: bool, readings: rolling_window<i64, 4us, 2us>) -> bool (node), module demo.windows, line 167
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> spanned_readiness(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::TSWDuration<::hgraph::Int, 4, 2>> readings);
}  // namespace w_

namespace w_
{
// recent_validity(readings: rolling_window<i64, 3us, 1us>) -> bool (node), module demo.windows, line 171
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> recent_validity(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSWDuration<::hgraph::Int, 3, 1>> readings);
}  // namespace w_

namespace w_
{
// observe_spanned(probe: bool, reading: i64) -> bool (graph), module demo.windows, line 175
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> observe_spanned(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading);
}  // namespace w_

namespace w_
{
// observe_recent_validity(reading: i64) -> bool (graph), module demo.windows, line 177
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> observe_recent_validity(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading);
}  // namespace w_

namespace w_
{
// total_at_tick(readings: rolling_window<i64, 3us, 0s>) -> i64 (node), module demo.windows, line 189
::hgraph::Port<::hgraph::TS<::hgraph::Int>> total_at_tick(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSWDuration<::hgraph::Int, 3, 0>> readings);
}  // namespace w_

namespace w_
{
// observe_total(reading: i64) -> i64 (graph), module demo.windows, line 196
::hgraph::Port<::hgraph::TS<::hgraph::Int>> observe_total(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading);
}  // namespace w_

}  // namespace hgl::demo::windows::m_
