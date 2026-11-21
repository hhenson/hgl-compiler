// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#pragma once

#include <hglrt/hglrt.h>
#include "demo.nested.types.h"

// Names this file writes, which no macro of a header above takes.
#undef Basket
#undef basket
#undef book
#undef demo
#undef edit_book
#undef fill
#undef grow_pairs
#undef nested
#undef pairs
#undef relay_basket
#undef relay_book
#undef relay_fixed_pairs
#undef relay_pairs
#undef relay_tagged
#undef retag
#undef set_fixed_pair
#undef slot
#undef step
#undef summed
#undef tagged
#undef total
#undef value

namespace hgl::demo::nested::m_
{

namespace w_
{
// relay_pairs(pairs: list<tuple<i64, str>>) -> list<tuple<i64, str>> (node), module demo.nested, line 9
::hgraph::Port<::hgraph::TSL<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>>> relay_pairs(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSL<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>>> pairs);
}  // namespace w_

namespace w_
{
// grow_pairs(step: i64) -> list<tuple<i64, str>> (node), module demo.nested, line 13
::hgraph::Port<::hgraph::TSL<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>>> grow_pairs(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> step);
}  // namespace w_

namespace w_
{
// relay_fixed_pairs(pairs: list<tuple<i64, str>, 2>) -> list<tuple<i64, str>, 2> (node), module demo.nested, line 45
::hgraph::Port<::hgraph::TSL<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>, 2>> relay_fixed_pairs(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSL<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>, 2>> pairs);
}  // namespace w_

namespace w_
{
// set_fixed_pair(slot: i64, value: i64) -> list<tuple<i64, str>, 2> (node), module demo.nested, line 49
::hgraph::Port<::hgraph::TSL<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>, 2>> set_fixed_pair(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> slot, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// relay_tagged(tagged: tuple<list<i64>, bool>) -> tuple<list<i64>, bool> (node), module demo.nested, line 64
::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TSL<::hgraph::TS<::hgraph::Int>>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Bool>>>> relay_tagged(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TSL<::hgraph::TS<::hgraph::Int>>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Bool>>>> tagged);
}  // namespace w_

namespace w_
{
// retag(step: i64) -> tuple<list<i64>, bool> (node), module demo.nested, line 68
::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TSL<::hgraph::TS<::hgraph::Int>>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Bool>>>> retag(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> step);
}  // namespace w_

namespace w_
{
// relay_book(book: dict<str, tuple<i64, str>>) -> dict<str, tuple<i64, str>> (node), module demo.nested, line 91
::hgraph::Port<::hgraph::TSD<::hgraph::Str, ::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>>> relay_book(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TSD<::hgraph::Str, ::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>>> book);
}  // namespace w_

namespace w_
{
// edit_book(step: i64) -> dict<str, tuple<i64, str>> (node), module demo.nested, line 95
::hgraph::Port<::hgraph::TSD<::hgraph::Str, ::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>>> edit_book(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> step);
}  // namespace w_

namespace w_
{
// relay_basket(basket: Basket) -> Basket (node), module demo.nested, line 131
::hgraph::Port<::hgraph::NominalTSB<::hgl::demo::nested::m_::Basket, ::hgraph::Field<"name", ::hgraph::TS<::hgraph::Str>>, ::hgraph::Field<"counts", ::hgraph::TSL<::hgraph::TS<::hgraph::Int>>>>> relay_basket(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::NominalTSB<::hgl::demo::nested::m_::Basket, ::hgraph::Field<"name", ::hgraph::TS<::hgraph::Str>>, ::hgraph::Field<"counts", ::hgraph::TSL<::hgraph::TS<::hgraph::Int>>>>> basket);
}  // namespace w_

namespace w_
{
// fill(step: i64) -> Basket (node), module demo.nested, line 135
::hgraph::Port<::hgraph::NominalTSB<::hgl::demo::nested::m_::Basket, ::hgraph::Field<"name", ::hgraph::TS<::hgraph::Str>>, ::hgraph::Field<"counts", ::hgraph::TSL<::hgraph::TS<::hgraph::Int>>>>> fill(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> step);
}  // namespace w_

// total(basket: Basket) -> i64 (`const fn`), module demo.nested, line 143
::hgraph::Int total(const ::hgl::demo::nested::m_::Basket& basket);

namespace w_
{
// summed(basket: Basket) -> i64 (node), module demo.nested, line 151
::hgraph::Port<::hgraph::TS<::hgraph::Int>> summed(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::NominalTSB<::hgl::demo::nested::m_::Basket, ::hgraph::Field<"name", ::hgraph::TS<::hgraph::Str>>, ::hgraph::Field<"counts", ::hgraph::TSL<::hgraph::TS<::hgraph::Int>>>>> basket);
}  // namespace w_

}  // namespace hgl::demo::nested::m_
