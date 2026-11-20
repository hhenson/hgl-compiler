// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#pragma once

#include <hglrt/hglrt.h>

// Names this file writes, which no macro of a header above takes.
#undef at
#undef changes
#undef changes_of
#undef count
#undef demo
#undef first
#undef high
#undef lists
#undef low
#undef observe
#undef observe_parts
#undef pair
#undef part_validity
#undef parts
#undef probe
#undef probe_pair
#undef read_slot
#undef reading
#undef readings
#undef relay
#undef set_part
#undef set_slot
#undef slot
#undef slot_of
#undef span
#undef span_of
#undef sum
#undef total
#undef value
#undef values

namespace hgl::demo::lists::m_
{

namespace w_
{
// readings(count: i64) -> list<i64> (node), module demo.lists, line 10
::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>>> readings(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> count);
}  // namespace w_

// sum(values: list<i64>) -> i64 (`const fn`), module demo.lists, line 24
::hgraph::Int sum(const ::hglrt::list<::hgraph::Int>& values);

namespace w_
{
// total(count: i64) -> i64 (graph), module demo.lists, line 31
::hgraph::Port<::hgraph::TS<::hgraph::Int>> total(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> count);
}  // namespace w_

namespace w_
{
// changes(values: list<i64>) -> list<i64> (node), module demo.lists, line 34
::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>>> changes(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>>> values);
}  // namespace w_

namespace w_
{
// changes_of(count: i64) -> list<i64> (graph), module demo.lists, line 38
::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>>> changes_of(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> count);
}  // namespace w_

namespace w_
{
// set_slot(slot: i64, reading: i64) -> list<i64, 2> (node), module demo.lists, line 41
::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>, 2>> set_slot(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> slot, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading);
}  // namespace w_

namespace w_
{
// probe_pair(probe: bool, pair: list<i64, 2>, slot: i64) -> list<bool, 3> (node), module demo.lists, line 46
::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Bool>, 3>> probe_pair(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>, 2>> pair, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> slot);
}  // namespace w_

namespace w_
{
// observe(probe: bool, slot: i64, reading: i64, at: i64) -> list<bool, 3> (graph), module demo.lists, line 52
::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Bool>, 3>> observe(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> slot, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> at);
}  // namespace w_

namespace w_
{
// read_slot(pair: list<i64, 2>, slot: i64) -> i64 (node), module demo.lists, line 56
::hgraph::Port<::hgraph::TS<::hgraph::Int>> read_slot(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>, 2>> pair, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> slot);
}  // namespace w_

namespace w_
{
// slot_of(slot: i64, reading: i64, at: i64) -> i64 (graph), module demo.lists, line 60
::hgraph::Port<::hgraph::TS<::hgraph::Int>> slot_of(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> slot, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> at);
}  // namespace w_

// span(low: i64, high: i64) -> i64 (`const fn`), module demo.lists, line 62
::hgraph::Int span(::hgraph::Int low, ::hgraph::Int high);

namespace w_
{
// span_of(slot: i64, reading: i64) -> i64 (graph), module demo.lists, line 65
::hgraph::Port<::hgraph::TS<::hgraph::Int>> span_of(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> slot, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading);
}  // namespace w_

namespace w_
{
// set_part(first: bool, value: i64) -> tuple<i64, i64> (node), module demo.lists, line 71
::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"_0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"_1", ::hgraph::TS<::hgraph::Int>>>> set_part(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> first, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// part_validity(probe: bool, parts: tuple<i64, i64>) -> list<bool, 2> (node), module demo.lists, line 80
::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Bool>, 2>> part_validity(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"_0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"_1", ::hgraph::TS<::hgraph::Int>>>> parts);
}  // namespace w_

namespace w_
{
// observe_parts(probe: bool, first: bool, value: i64) -> list<bool, 2> (graph), module demo.lists, line 84
::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Bool>, 2>> observe_parts(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> first, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// relay(values: list<i64>) -> list<i64> (node), module demo.lists, line 88
::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>>> relay(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>>> values);
}  // namespace w_

}  // namespace hgl::demo::lists::m_
