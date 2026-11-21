// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#pragma once

#include <hglrt/hglrt.h>
#include "type-demo-store.h"

// Names this file writes, which no macro of a header above takes.
#undef Quote
#undef based
#undef bid_plus
#undef booked
#undef changed
#undef counted_members
#undef demo
#undef extra
#undef paired
#undef quoted
#undef quotes
#undef read_at_build
#undef seeded
#undef store
#undef ticks
#undef value
#undef values
#undef x

namespace hgl::demo::store::m_
{

namespace w_
{
// quoted(value: i64) -> i64 (node), module demo.store, line 14
::hgraph::Port<::hgraph::TS<::hgraph::Int>> quoted(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// paired(value: i64) -> i64 (node), module demo.store, line 23
::hgraph::Port<::hgraph::TS<::hgraph::Int>> paired(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// booked(value: i64) -> i64 (node), module demo.store, line 31
::hgraph::Port<::hgraph::TS<::hgraph::Int>> booked(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// counted_members(value: i64) -> i64 (node), module demo.store, line 43
::hgraph::Port<::hgraph::TS<::hgraph::Int>> counted_members(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// changed(value: i64) -> i64 (node), module demo.store, line 54
::hgraph::Port<::hgraph::TS<::hgraph::Int>> changed(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// bid_plus(x: i64, const extra: i64) -> i64 (node), module demo.store, line 65
::hgraph::Port<::hgraph::TS<::hgraph::Int>> bid_plus(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x, ::hgraph::Int extra);
}  // namespace w_

namespace w_
{
// seeded(x: i64) -> i64 (graph), module demo.store, line 69
::hgraph::Port<::hgraph::TS<::hgraph::Int>> seeded(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x);
}  // namespace w_

namespace w_
{
// read_at_build(const values: list<i64>) -> i64 (graph), module demo.store, line 78
::hgraph::Port<::hgraph::TS<::hgraph::Int>> read_at_build(::hgraph::Wiring& wiring_, const ::hglrt::list<::hgraph::Int>& values);
}  // namespace w_

namespace w_
{
// ticks(const values: list<i64>) -> i64 (source), module demo.store, line 85
::hgraph::Port<::hgraph::TS<::hgraph::Int>> ticks(::hgraph::Wiring& wiring_, const ::hglrt::list<::hgraph::Int>& values);
}  // namespace w_

namespace w_
{
// based(value: i64) -> Quote (node), module demo.store, line 91
::hgraph::Port<::hgraph::NativeTSB<::hgl::demo::store::m_::Quote>> based(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// quotes(const values: list<i64>) -> Quote (graph), module demo.store, line 99
::hgraph::Port<::hgraph::NativeTSB<::hgl::demo::store::m_::Quote>> quotes(::hgraph::Wiring& wiring_, const ::hglrt::list<::hgraph::Int>& values);
}  // namespace w_

}  // namespace hgl::demo::store::m_
