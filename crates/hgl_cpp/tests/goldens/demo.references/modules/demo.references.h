// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#pragma once

#include <hglrt/hglrt.h>

// Names this file writes, which no macro of a header above takes.
#undef a
#undef b
#undef choose
#undef chosen
#undef demo
#undef flag
#undef follow
#undef high
#undef item
#undef low
#undef pick
#undef picked
#undef present
#undef probe
#undef references
#undef sample_picked
#undef sampled
#undef sel
#undef take
#undef target

namespace hgl::demo::references::m_
{

namespace w_
{
// pick(flag: bool, ref low: i64, ref high: i64) -> ref i64 (node), module demo.references, line 9
::hgraph::Port<::hgraph::TS<::hgraph::Int>> pick(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> flag, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> low, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> high);
}  // namespace w_

namespace w_
{
// follow(item: i64) -> i64 (node), module demo.references, line 17
::hgraph::Port<::hgraph::TS<::hgraph::Int>> follow(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> item);
}  // namespace w_

namespace w_
{
// picked(flag: bool, low: i64, high: i64) -> i64 (graph), module demo.references, line 21
::hgraph::Port<::hgraph::TS<::hgraph::Int>> picked(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> flag, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> low, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> high);
}  // namespace w_

namespace w_
{
// present(probe: bool, ref target: i64) -> i64 (node), module demo.references, line 37
::hgraph::Port<::hgraph::TS<::hgraph::Int>> present(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> target);
}  // namespace w_

namespace w_
{
// sample_picked(take: bool, flag: bool, low: i64, high: i64) -> i64 (graph), module demo.references, line 55
::hgraph::Port<::hgraph::TS<::hgraph::Int>> sample_picked(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> take, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> flag, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> low, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> high);
}  // namespace w_

namespace w_
{
// choose(flag: i64, low: i64, high: i64) -> ref i64 (node), module demo.references, line 70
::hgraph::Port<::hgraph::TS<::hgraph::Int>> choose(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> flag, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> low, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> high);
}  // namespace w_

namespace w_
{
// chosen(flag: i64, sel: bool, a: i64, b: i64, high: i64) -> i64 (graph), module demo.references, line 79
::hgraph::Port<::hgraph::TS<::hgraph::Int>> chosen(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> flag, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> sel, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> a, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> b, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> high);
}  // namespace w_

namespace w_::i_::sampled
{
// sampled(take: bool, ref target: i64) -> ref i64 (node), the instance `i64`, module demo.references, line 51
::hgraph::Port<::hgraph::TS<::hgraph::Int>> of_i64(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> take, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> target);
}  // namespace w_::i_::sampled

}  // namespace hgl::demo::references::m_
