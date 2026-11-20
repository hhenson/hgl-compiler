// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#pragma once

#include <hglrt/hglrt.h>

// Names this file writes, which no macro of a header above takes.
#undef book
#undef book_changes
#undef book_valid
#undef book_wakes
#undef demo
#undef entry_a
#undef entry_at
#undef entry_delta
#undef entry_or_code
#undef groups
#undef key
#undef keyed
#undef member_changes
#undef member_wakes
#undef members
#undef members_valid
#undef price_of_a
#undef priced
#undef read_delta
#undef relay_book
#undef relay_floats
#undef relay_groups
#undef relay_members
#undef step
#undef sum_members
#undef total_of_members
#undef values
#undef write_book
#undef write_members

namespace hgl::demo::keyed::m_
{

namespace w_
{
// write_members(step: i64) -> set<i64> (node), module demo.keyed, line 12
::hgraph::Port<::hgraph::TSS<::hgraph::Int>> write_members(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> step);
}  // namespace w_

namespace w_
{
// members_valid(members: set<i64>) -> bool (node), module demo.keyed, line 28
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> members_valid(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSS<::hgraph::Int>> members);
}  // namespace w_

namespace w_
{
// member_wakes(step: i64) -> bool (graph), module demo.keyed, line 32
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> member_wakes(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> step);
}  // namespace w_

namespace w_
{
// read_delta(members: set<i64>) -> set<i64> (node), module demo.keyed, line 34
::hgraph::Port<::hgraph::TSS<::hgraph::Int>> read_delta(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSS<::hgraph::Int>> members);
}  // namespace w_

namespace w_
{
// member_changes(step: i64) -> set<i64> (graph), module demo.keyed, line 38
::hgraph::Port<::hgraph::TSS<::hgraph::Int>> member_changes(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> step);
}  // namespace w_

// sum_members(members: set<i64>) -> i64 (`const fn`), module demo.keyed, line 40
::hgraph::Int sum_members(const ::hglrt::set<::hgraph::Int>& members);

namespace w_
{
// total_of_members(step: i64) -> i64 (graph), module demo.keyed, line 46
::hgraph::Port<::hgraph::TS<::hgraph::Int>> total_of_members(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> step);
}  // namespace w_

namespace w_
{
// relay_members(members: set<i64>) -> set<i64> (node), module demo.keyed, line 64
::hgraph::Port<::hgraph::TSS<::hgraph::Int>> relay_members(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSS<::hgraph::Int>> members);
}  // namespace w_

namespace w_
{
// relay_floats(values: set<f64>) -> set<f64> (node), module demo.keyed, line 79
::hgraph::Port<::hgraph::TSS<::hgraph::Float>> relay_floats(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSS<::hgraph::Float>> values);
}  // namespace w_

namespace w_
{
// write_book(step: i64) -> dict<str, i64> (node), module demo.keyed, line 90
::hgraph::Port<::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>> write_book(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> step);
}  // namespace w_

namespace w_
{
// book_valid(book: dict<str, i64>) -> bool (node), module demo.keyed, line 103
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> book_valid(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>> book);
}  // namespace w_

namespace w_
{
// book_wakes(step: i64) -> bool (graph), module demo.keyed, line 107
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> book_wakes(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> step);
}  // namespace w_

// price_of_a(book: dict<str, i64>) -> i64 (`const fn`), module demo.keyed, line 109
::hgraph::Int price_of_a(const ::hglrt::dict<::hgraph::Str, ::hgraph::Int>& book);

namespace w_
{
// priced(step: i64) -> i64 (graph), module demo.keyed, line 115
::hgraph::Port<::hgraph::TS<::hgraph::Int>> priced(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> step);
}  // namespace w_

namespace w_
{
// relay_book(book: dict<str, i64>) -> dict<str, i64> (node), module demo.keyed, line 127
::hgraph::Port<::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>> relay_book(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>> book);
}  // namespace w_

namespace w_
{
// book_changes(book: dict<str, i64>) -> dict<str, i64> (node), module demo.keyed, line 131
::hgraph::Port<::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>> book_changes(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>> book);
}  // namespace w_

namespace w_
{
// relay_groups(groups: dict<str, set<i64>>) -> dict<str, set<i64>> (node), module demo.keyed, line 145
::hgraph::Port<::hgraph::TSD<::hgraph::Str, ::hgraph::TSS<::hgraph::Int>>> relay_groups(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSD<::hgraph::Str, ::hgraph::TSS<::hgraph::Int>>> groups);
}  // namespace w_

namespace w_
{
// entry_a(book: dict<str, i64>) -> i64 (node), module demo.keyed, line 159
::hgraph::Port<::hgraph::TS<::hgraph::Int>> entry_a(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>> book);
}  // namespace w_

namespace w_
{
// entry_at(book: dict<str, i64>, key: str) -> i64 (node), module demo.keyed, line 163
::hgraph::Port<::hgraph::TS<::hgraph::Int>> entry_at(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>> book, ::hgraph::Port<::hgraph::TS<::hgraph::Str>> key);
}  // namespace w_

namespace w_
{
// entry_or_code(book: dict<str, i64>, key: str) -> str (node), module demo.keyed, line 167
::hgraph::Port<::hgraph::TS<::hgraph::Str>> entry_or_code(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>> book, ::hgraph::Port<::hgraph::TS<::hgraph::Str>> key);
}  // namespace w_

namespace w_
{
// entry_delta(book: dict<str, i64>) -> i64 (node), module demo.keyed, line 177
::hgraph::Port<::hgraph::TS<::hgraph::Int>> entry_delta(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSD<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>> book);
}  // namespace w_

}  // namespace hgl::demo::keyed::m_
