// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#pragma once

#include <hglrt/hglrt.h>
#include "demo.configurations.types.h"

// Names this file writes, which no macro of a header above takes.
#undef Holder
#undef Noted
#undef a
#undef above
#undef b
#undef book
#undef booked
#undef both
#undef branch
#undef by
#undef checked
#undef configurations
#undef demo
#undef differ
#undef fallback
#undef first
#undef flag
#undef from_book
#undef from_members
#undef h
#undef holders
#undef key
#undef looked
#undef member_of
#undef members
#undef noted_bid
#undef nudged
#undef probe
#undef q
#undef shifted
#undef signs
#undef summed
#undef values
#undef x

namespace hgl::demo::configurations::m_
{

namespace w_
{
// from_members(const members: set<i64>) -> i64 (source), module demo.configurations, line 19
::hgraph::Port<::hgraph::TS<::hgraph::Int>> from_members(::hgraph::Wiring& wiring_, const ::hglrt::set<::hgraph::Int>& members);
}  // namespace w_

namespace w_
{
// from_book(const book: dict<str, i64>) -> i64 (source), module demo.configurations, line 25
::hgraph::Port<::hgraph::TS<::hgraph::Int>> from_book(::hgraph::Wiring& wiring_, const ::hglrt::dict<::hgraph::Str, ::hgraph::Int>& book);
}  // namespace w_

namespace w_
{
// shifted(x: i64, const by: tuple<i64, i64>) -> i64 (node), module demo.configurations, line 34
::hgraph::Port<::hgraph::TS<::hgraph::Int>> shifted(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x, const std::tuple<::hgraph::Int, ::hgraph::Int>& by);
}  // namespace w_

namespace w_
{
// first(probe: bool, const h: Holder) -> i64 (node), module demo.configurations, line 38
::hgraph::Port<::hgraph::TS<::hgraph::Int>> first(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, const ::hgl::demo::configurations::m_::Holder& h);
}  // namespace w_

namespace w_
{
// noted_bid(x: i64, const q: Noted) -> i64 (node), module demo.configurations, line 42
::hgraph::Port<::hgraph::TS<::hgraph::Int>> noted_bid(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x, const ::hgl::demo::configurations::m_::Noted& q);
}  // namespace w_

namespace w_
{
// nudged(x: i64, const by: i64?) -> i64 (node), module demo.configurations, line 49
::hgraph::Port<::hgraph::TS<::hgraph::Int>> nudged(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x, std::optional<::hgraph::Int> by);
}  // namespace w_

namespace w_
{
// both(x: i64) -> i64 (graph), module demo.configurations, line 57
::hgraph::Port<::hgraph::TS<::hgraph::Int>> both(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x);
}  // namespace w_

namespace w_
{
// holders(probe: bool) -> i64 (graph), module demo.configurations, line 61
::hgraph::Port<::hgraph::TS<::hgraph::Int>> holders(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe);
}  // namespace w_

namespace w_
{
// above(x: f64, const by: tuple<f64, f64>) -> bool (node), module demo.configurations, line 67
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> above(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Float>> x, const std::tuple<::hgraph::Float, ::hgraph::Float>& by);
}  // namespace w_

namespace w_
{
// differ(a: bool, b: bool) -> bool (node), module demo.configurations, line 71
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> differ(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> a, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> b);
}  // namespace w_

namespace w_
{
// signs(x: f64) -> bool (graph), module demo.configurations, line 75
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> signs(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Float>> x);
}  // namespace w_

// member_of(x: i64, members: set<i64>) -> bool (`const fn`), module demo.configurations, line 78
::hgraph::Bool member_of(::hgraph::Int x, const ::hglrt::set<::hgraph::Int>& members);

namespace w_
{
// checked(x: i64) -> bool (graph), module demo.configurations, line 85
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> checked(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x);
}  // namespace w_

// looked(x: str, book: dict<str, i64>) -> i64 (`const fn`), module demo.configurations, line 87
::hgraph::Int looked(const ::hgraph::Str& x, const ::hglrt::dict<::hgraph::Str, ::hgraph::Int>& book);

namespace w_
{
// booked(x: str) -> i64 (graph), module demo.configurations, line 93
::hgraph::Port<::hgraph::TS<::hgraph::Int>> booked(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Str>> x);
}  // namespace w_

namespace w_
{
// summed(key: str, const members: set<i64>) -> i64 (node), module demo.configurations, line 96
::hgraph::Port<::hgraph::TS<::hgraph::Int>> summed(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Str>> key, const ::hglrt::set<::hgraph::Int>& members);
}  // namespace w_

namespace w_
{
// fallback(key: str, const values: list<i64>) -> i64 (node), module demo.configurations, line 104
::hgraph::Port<::hgraph::TS<::hgraph::Int>> fallback(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Str>> key, const ::hglrt::list<::hgraph::Int>& values);
}  // namespace w_

namespace w_
{
// branch(flag: bool, key: str, const members: set<i64>) -> i64 (graph), module demo.configurations, line 108
::hgraph::Port<::hgraph::TS<::hgraph::Int>> branch(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> flag, ::hgraph::Port<::hgraph::TS<::hgraph::Str>> key, const ::hglrt::set<::hgraph::Int>& members);
}  // namespace w_

}  // namespace hgl::demo::configurations::m_
