// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#pragma once

#include <hglrt/hglrt.h>

// Names this file writes, which no macro of a header above takes.
#undef a
#undef b
#undef changed_key
#undef choose
#undef chooses
#undef chosen
#undef demo
#undef entry_a
#undef flag
#undef follow
#undef handed
#undef handed_doubled
#undef handed_if_valid
#undef handed_on
#undef handed_on_if_valid
#undef handed_or_low
#undef high
#undef high_slot
#undef holds_one
#undef holds_withdrawn
#undef idx
#undef item
#undef items
#undef key
#undef low
#undef low_slot
#undef on
#undef pick
#undef pick_list
#undef picked
#undef present
#undef present_item
#undef price
#undef probe
#undef reading
#undef references
#undef route
#undef routed
#undef sample_picked
#undef sampled
#undef sel
#undef selected
#undef selected_present
#undef set_item
#undef slot
#undef take
#undef target
#undef withdrawn
#undef write_entry

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

namespace w_
{
// route(idx: i64, items: list<i64, 2>) -> ref i64 (node), module demo.references, line 95
::hgraph::Port<::hgraph::TS<::hgraph::Int>> route(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> idx, ::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>, 2>> items);
}  // namespace w_

namespace w_
{
// set_item(slot: i64, reading: i64) -> list<i64, 2> (node), module demo.references, line 99
::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>, 2>> set_item(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> slot, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading);
}  // namespace w_

namespace w_
{
// pick_list(flag: bool, ref low: list<i64, 2>, ref high: list<i64, 2>) -> ref list<i64, 2> (node), module demo.references, line 103
::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>, 2>> pick_list(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> flag, ::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>, 2>> low, ::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>, 2>> high);
}  // namespace w_

namespace w_
{
// routed(idx: i64, flag: bool, low_slot: i64, low: i64, high_slot: i64, high: i64) -> i64 (graph), module demo.references, line 110
::hgraph::Port<::hgraph::TS<::hgraph::Int>> routed(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> idx, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> flag, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> low_slot, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> low, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> high_slot, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> high);
}  // namespace w_

namespace w_
{
// write_entry(changed_key: str, price: i64?) -> dict<str, i64> (node), module demo.references, line 136
::hgraph::Port<::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>> write_entry(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Str>> changed_key, ::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> price);
}  // namespace w_

namespace w_
{
// selected(key: str, changed_key: str, price: i64?) -> i64 (graph), module demo.references, line 143
::hgraph::Port<::hgraph::TS<::hgraph::Int>> selected(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Str>> key, ::hgraph::Port<::hgraph::TS<::hgraph::Str>> changed_key, ::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> price);
}  // namespace w_

namespace w_
{
// present_item(probe: bool, item: i64) -> bool (node), module demo.references, line 154
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> present_item(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> item);
}  // namespace w_

namespace w_
{
// selected_present(probe: bool, key: str, changed_key: str, price: i64?) -> bool (graph), module demo.references, line 158
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> selected_present(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::TS<::hgraph::Str>> key, ::hgraph::Port<::hgraph::TS<::hgraph::Str>> changed_key, ::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> price);
}  // namespace w_

namespace w_
{
// entry_a(changed_key: str, price: i64?) -> i64 (graph), module demo.references, line 171
::hgraph::Port<::hgraph::TS<::hgraph::Int>> entry_a(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Str>> changed_key, ::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> price);
}  // namespace w_

namespace w_
{
// withdrawn(on: bool, ref low: i64) -> ref i64 (node), module demo.references, line 182
::hgraph::Port<::hgraph::TS<::hgraph::Int>> withdrawn(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> on, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> low);
}  // namespace w_

namespace w_
{
// handed(take: bool, ref target: i64) -> ref i64 (node), module demo.references, line 190
::hgraph::Port<::hgraph::TS<::hgraph::Int>> handed(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> take, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> target);
}  // namespace w_

namespace w_
{
// handed_on(take: bool, on: bool, low: i64) -> i64 (graph), module demo.references, line 194
::hgraph::Port<::hgraph::TS<::hgraph::Int>> handed_on(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> take, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> on, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> low);
}  // namespace w_

namespace w_
{
// handed_if_valid(take: bool, ref target: i64) -> ref i64 (node), module demo.references, line 204
::hgraph::Port<::hgraph::TS<::hgraph::Int>> handed_if_valid(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> take, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> target);
}  // namespace w_

namespace w_
{
// handed_on_if_valid(take: bool, on: bool, low: i64) -> i64 (graph), module demo.references, line 208
::hgraph::Port<::hgraph::TS<::hgraph::Int>> handed_on_if_valid(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> take, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> on, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> low);
}  // namespace w_

namespace w_
{
// holds_one(take: bool, ref target: i64) -> bool (node), module demo.references, line 219
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> holds_one(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> take, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> target);
}  // namespace w_

namespace w_
{
// holds_withdrawn(take: bool, on: bool, low: i64) -> bool (graph), module demo.references, line 223
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> holds_withdrawn(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> take, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> on, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> low);
}  // namespace w_

namespace w_
{
// handed_doubled(take: bool, on: bool, low: i64) -> i64 (graph), module demo.references, line 235
::hgraph::Port<::hgraph::TS<::hgraph::Int>> handed_doubled(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> take, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> on, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> low);
}  // namespace w_

namespace w_
{
// handed_or_low(chooses: bool, take: bool, on: bool, low: i64) -> i64 (graph), module demo.references, line 245
::hgraph::Port<::hgraph::TS<::hgraph::Int>> handed_or_low(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> chooses, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> take, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> on, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> low);
}  // namespace w_

namespace w_::i_::sampled
{
// sampled(take: bool, ref target: i64) -> ref i64 (node), the instance `i64`, module demo.references, line 51
::hgraph::Port<::hgraph::TS<::hgraph::Int>> of_i64(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> take, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> target);
}  // namespace w_::i_::sampled

}  // namespace hgl::demo::references::m_
