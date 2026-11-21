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
#undef first_of
#undef first_slot
#undef head
#undef high
#undef lists
#undef low
#undef nested
#undef nested_of
#undef nested_valid
#undef observe
#undef observe_parts
#undef ordered_child
#undef ordered_connection
#undef ordered_of
#undef ordered_port
#undef ordered_row
#undef ordered_row_of
#undef pair
#undef part_sum
#undef part_validity
#undef parts
#undef probe
#undef probe_pair
#undef read_slot
#undef reading
#undef readings
#undef relay
#undef second_changed
#undef second_delta
#undef set_part
#undef set_slot
#undef shifted_pair
#undef slot
#undef slot_of
#undef span
#undef span_of
#undef sum
#undef sum_of_parts
#undef total
#undef trigger
#undef value
#undef values
#undef whole_or_part

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
::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Int>>>> set_part(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> first, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// part_validity(probe: bool, parts: tuple<i64, i64>) -> list<bool, 2> (node), module demo.lists, line 80
::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Bool>, 2>> part_validity(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Int>>>> parts);
}  // namespace w_

namespace w_
{
// observe_parts(probe: bool, first: bool, value: i64) -> list<bool, 2> (graph), module demo.lists, line 84
::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Bool>, 2>> observe_parts(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> first, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// first_of(pair: list<i64, 2>) -> i64 (node), module demo.lists, line 88
::hgraph::Port<::hgraph::TS<::hgraph::Int>> first_of(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>, 2>> pair);
}  // namespace w_

namespace w_
{
// first_slot(slot: i64, reading: i64) -> i64 (graph), module demo.lists, line 92
::hgraph::Port<::hgraph::TS<::hgraph::Int>> first_slot(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> slot, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading);
}  // namespace w_

namespace w_
{
// second_delta(pair: list<i64, 2>) -> bool (node), module demo.lists, line 95
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> second_delta(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>, 2>> pair);
}  // namespace w_

namespace w_
{
// second_changed(slot: i64, reading: i64) -> bool (graph), module demo.lists, line 102
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> second_changed(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> slot, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> reading);
}  // namespace w_

namespace w_
{
// whole_or_part(head: bool, value: i64) -> tuple<i64, i64> (node), module demo.lists, line 106
::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Int>>>> whole_or_part(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> head, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// part_sum(parts: tuple<i64, i64>) -> i64 (node), module demo.lists, line 114
::hgraph::Port<::hgraph::TS<::hgraph::Int>> part_sum(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Int>>>> parts);
}  // namespace w_

namespace w_
{
// sum_of_parts(head: bool, value: i64) -> i64 (graph), module demo.lists, line 121
::hgraph::Port<::hgraph::TS<::hgraph::Int>> sum_of_parts(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> head, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// nested(count: i64) -> list<list<i64>> (node), module demo.lists, line 124
::hgraph::Port<::hgraph::TSL<::hgraph::TSL<::hgraph::TS<::hgraph::Int>>>> nested(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> count);
}  // namespace w_

namespace w_
{
// nested_valid(values: list<list<i64>>) -> list<bool, 3> (node), module demo.lists, line 131
::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Bool>, 3>> nested_valid(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSL<::hgraph::TSL<::hgraph::TS<::hgraph::Int>>>> values);
}  // namespace w_

namespace w_
{
// nested_of(count: i64) -> list<bool, 3> (graph), module demo.lists, line 135
::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Bool>, 3>> nested_of(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> count);
}  // namespace w_

namespace w_
{
// ordered_child(values: list<list<i64>>) -> bool (node), module demo.lists, line 139
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> ordered_child(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSL<::hgraph::TSL<::hgraph::TS<::hgraph::Int>>>> values);
}  // namespace w_

namespace w_
{
// ordered_of(count: i64) -> bool (graph), module demo.lists, line 149
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> ordered_of(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> count);
}  // namespace w_

namespace w_
{
// ordered_row(values: list<list<i64>>) -> bool (node), module demo.lists, line 152
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> ordered_row(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSL<::hgraph::TSL<::hgraph::TS<::hgraph::Int>>>> values);
}  // namespace w_

namespace w_
{
// ordered_row_of(count: i64) -> bool (graph), module demo.lists, line 162
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> ordered_row_of(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> count);
}  // namespace w_

namespace w_
{
// shifted_pair(trigger: i64, const first: i64) -> list<i64, 2> (node), module demo.lists, line 166
::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>, 2>> shifted_pair(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> trigger, ::hgraph::Int first);
}  // namespace w_

namespace w_
{
// ordered_port(trigger: i64) -> i64 (graph), module demo.lists, line 170
::hgraph::Port<::hgraph::TS<::hgraph::Int>> ordered_port(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> trigger);
}  // namespace w_

namespace w_
{
// ordered_connection(trigger: i64) -> i64 (graph), module demo.lists, line 180
::hgraph::Port<::hgraph::TS<::hgraph::Int>> ordered_connection(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> trigger);
}  // namespace w_

namespace w_
{
// relay(values: list<i64>) -> list<i64> (node), module demo.lists, line 189
::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>>> relay(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSL<::hgraph::TS<::hgraph::Int>>> values);
}  // namespace w_

}  // namespace hgl::demo::lists::m_
