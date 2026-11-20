// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#pragma once

#include <hglrt/hglrt.h>

// Names this file writes, which no macro of a header above takes.
#undef a
#undef add_one
#undef b
#undef count_publications
#undef counted_ticks
#undef demo
#undef first_of
#undef noted
#undef scaled
#undef stamped
#undef temporal
#undef ticks
#undef value
#undef values

namespace hgl::demo::temporal::m_
{

namespace w_
{
// add_one(value: i64) -> i64 (graph), module demo.temporal, line 7
::hgraph::Port<::hgraph::TS<::hgraph::Int>> add_one(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// scaled(value: f64) -> f64 (graph), module demo.temporal, line 12
::hgraph::Port<::hgraph::TS<::hgraph::Float>> scaled(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Float>> value);
}  // namespace w_

namespace w_
{
// count_publications(value: i64) -> i64 (node), module demo.temporal, line 17
::hgraph::Port<::hgraph::TS<::hgraph::Int>> count_publications(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// first_of(a: i64, b: i64) -> i64 (node), module demo.temporal, line 27
::hgraph::Port<::hgraph::TS<::hgraph::Int>> first_of(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> a, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> b);
}  // namespace w_

namespace w_
{
// ticks(const values: list<i64>) -> i64 (source), module demo.temporal, line 33
::hgraph::Port<::hgraph::TS<::hgraph::Int>> ticks(::hgraph::Wiring& wiring_, const ::hglrt::list<::hgraph::Int>& values);
}  // namespace w_

namespace w_
{
// counted_ticks(const values: list<i64>) -> i64 (graph), module demo.temporal, line 40
::hgraph::Port<::hgraph::TS<::hgraph::Int>> counted_ticks(::hgraph::Wiring& wiring_, const ::hglrt::list<::hgraph::Int>& values);
}  // namespace w_

namespace w_
{
// stamped(value: i64) -> datetime (node), module demo.temporal, line 46
::hgraph::Port<::hgraph::TS<::hgraph::DateTime>> stamped(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// noted(value: i64) (node), module demo.temporal, line 52
void noted(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

}  // namespace hgl::demo::temporal::m_
