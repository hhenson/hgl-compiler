// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#pragma once

#include <hglrt/hglrt.h>

// Names this file writes, which no macro of a header above takes.
#undef apps
#undef bounded
#undef demo
#undef every
#undef kept
#undef limit
#undef limited
#undef quiet
#undef seeded
#undef ticks
#undef value
#undef values

namespace hgl::demo::apps::m_
{

namespace w_
{
// ticks(const values: list<i64>, const every: duration = ...) -> i64 (source), module demo.apps, line 8
::hgraph::Port<::hgraph::TS<::hgraph::Int>> ticks(::hgraph::Wiring& wiring_, const ::hglrt::list<::hgraph::Int>& values, ::hgraph::TimeDelta every);
}  // namespace w_

namespace w_
{
// limited(value: i64) -> i64 (node), module demo.apps, line 15
::hgraph::Port<::hgraph::TS<::hgraph::Int>> limited(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// seeded(const limit: i64, const values: list<i64>) -> i64 (graph), module demo.apps, line 24
::hgraph::Port<::hgraph::TS<::hgraph::Int>> seeded(::hgraph::Wiring& wiring_, ::hgraph::Int limit, const ::hglrt::list<::hgraph::Int>& values);
}  // namespace w_

namespace w_
{
// bounded(const values: list<i64>) -> i64 (graph), module demo.apps, line 32
::hgraph::Port<::hgraph::TS<::hgraph::Int>> bounded(::hgraph::Wiring& wiring_, const ::hglrt::list<::hgraph::Int>& values);
}  // namespace w_

namespace w_
{
// kept(value: i64) (node), module demo.apps, line 37
void kept(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// quiet(const values: list<i64>) (graph), module demo.apps, line 43
void quiet(::hgraph::Wiring& wiring_, const ::hglrt::list<::hgraph::Int>& values);
}  // namespace w_

}  // namespace hgl::demo::apps::m_
