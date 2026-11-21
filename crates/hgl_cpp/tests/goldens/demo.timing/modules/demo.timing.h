// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#pragma once

#include <hglrt/hglrt.h>

// Names this file writes, which no macro of a header above takes.
#undef absolute_yields
#undef alarm_or_input
#undef alarmed
#undef at
#undef cancel_next
#undef cause_of_wake
#undef counter
#undef counting_from
#undef delay
#undef demo
#undef empty_elsewhere
#undef empty_selected
#undef every
#undef finish_early
#undef finish_or_resume
#undef first
#undef limit
#undef opening_at
#undef pending
#undef repeated
#undef scheduled_on_input
#undef second
#undef shortened
#undef ticker
#undef time_before_each_delay
#undef times
#undef timing
#undef value
#undef wake_after
#undef wake_in_the_past

namespace hgl::demo::timing::m_
{

namespace w_
{
// ticker(const every: duration, const times: i64) -> i64 (node), module demo.timing, line 10
::hgraph::Port<::hgraph::TS<::hgraph::Int>> ticker(::hgraph::Wiring& wiring_, ::hgraph::TimeDelta every, ::hgraph::Int times);
}  // namespace w_

namespace w_
{
// cause_of_wake(value: i64) -> str (node), module demo.timing, line 23
::hgraph::Port<::hgraph::TS<::hgraph::Str>> cause_of_wake(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// empty_selected(value: i64) -> bool (node), module demo.timing, line 37
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> empty_selected(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// empty_elsewhere(value: i64) -> bool (node), module demo.timing, line 44
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> empty_elsewhere(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// scheduled_on_input(value: i64) -> bool (node), module demo.timing, line 54
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> scheduled_on_input(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// pending() -> bool (node), module demo.timing, line 62
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> pending(::hgraph::Wiring& wiring_);
}  // namespace w_

namespace w_
{
// cancel_next() -> bool (node), module demo.timing, line 72
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> cancel_next(::hgraph::Wiring& wiring_);
}  // namespace w_

namespace w_
{
// wake_in_the_past(value: i64) -> str (node), module demo.timing, line 88
::hgraph::Port<::hgraph::TS<::hgraph::Str>> wake_in_the_past(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// wake_after(delay: duration) -> bool (node), module demo.timing, line 99
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> wake_after(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::TimeDelta>> delay);
}  // namespace w_

namespace w_
{
// alarm_or_input(value: i64) -> str (node), module demo.timing, line 108
::hgraph::Port<::hgraph::TS<::hgraph::Str>> alarm_or_input(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// alarmed() -> i64 (node), module demo.timing, line 117
::hgraph::Port<::hgraph::TS<::hgraph::Int>> alarmed(::hgraph::Wiring& wiring_);
}  // namespace w_

namespace w_
{
// time_before_each_delay() -> datetime (source), module demo.timing, line 129
::hgraph::Port<::hgraph::TS<::hgraph::DateTime>> time_before_each_delay(::hgraph::Wiring& wiring_);
}  // namespace w_

namespace w_
{
// absolute_yields(const first: datetime, const second: datetime) -> i64 (source), module demo.timing, line 138
::hgraph::Port<::hgraph::TS<::hgraph::Int>> absolute_yields(::hgraph::Wiring& wiring_, ::hgraph::DateTime first, ::hgraph::DateTime second);
}  // namespace w_

// opening_at(at: datetime) -> datetime (`const fn`), module demo.timing, line 144
::hgraph::DateTime opening_at(::hgraph::DateTime at);

// shortened(at: datetime) -> bool (`const fn`), module demo.timing, line 147
::hgraph::Bool shortened(::hgraph::DateTime at);

namespace w_
{
// finish_or_resume(const finish_early: bool) -> i64 (source), module demo.timing, line 150
::hgraph::Port<::hgraph::TS<::hgraph::Int>> finish_or_resume(::hgraph::Wiring& wiring_, ::hgraph::Bool finish_early);
}  // namespace w_

namespace w_
{
// counter(const limit: i64) -> i64 (source), module demo.timing, line 161
::hgraph::Port<::hgraph::TS<::hgraph::Int>> counter(::hgraph::Wiring& wiring_, ::hgraph::Int limit);
}  // namespace w_

namespace w_
{
// counting_from(const first: i64) -> i64 (source), module demo.timing, line 172
::hgraph::Port<::hgraph::TS<::hgraph::Int>> counting_from(::hgraph::Wiring& wiring_, ::hgraph::Int first);
}  // namespace w_

namespace w_::i_::repeated
{
// repeated(const value: i64, const times: i64) -> i64 (source), the instance `i64`, module demo.timing, line 182
::hgraph::Port<::hgraph::TS<::hgraph::Int>> of_i64(::hgraph::Wiring& wiring_, ::hgraph::Int value, ::hgraph::Int times);
}  // namespace w_::i_::repeated

}  // namespace hgl::demo::timing::m_
