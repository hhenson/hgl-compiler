// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#pragma once

#include <hglrt/hglrt.h>
#include "type-demo-structs.h"

// Names this file writes, which no macro of a header above takes.
#undef Holding
#undef MarketQuote
#undef NotedQuote
#undef Outer
#undef PlainQuote
#undef Wrapped
#undef action
#undef ask
#undef atomic_bid
#undef atomic_bid_ticks
#undef bid
#undef bid_delta
#undef bid_ticks
#undef code
#undef command
#undef demo
#undef doubled
#undef first
#undef first_of
#undef held_count
#undef holding
#undef input
#undef is_bid
#undef make_outer
#undef nested_total
#undef note_is_null
#undef note_or_none
#undef noted_note
#undef nullable_delta_payload
#undef observe_noted_quote
#undef observe_part_validity
#undef observe_quote_all_valid
#undef observe_quote_valid
#undef observe_validity
#undef outer
#undef outer_deltas
#undef outer_ticks
#undef pair
#undef pair_sum
#undef part_validity
#undef parts
#undef price
#undef probe
#undef publish_nullable
#undef quote
#undef quote_all_valid
#undef quote_valid
#undef quoted_total
#undef read_noted_quote
#undef read_outer
#undef read_validity
#undef reading
#undef relay_pair
#undef relay_quote
#undef sample
#undef sample_timestamp
#undef scalar_timestamp
#undef set_one_part
#undef size
#undef spread
#undef spread_of
#undef structs
#undef update_quote
#undef value
#undef watched
#undef whole_noted
#undef whole_plain
#undef whole_quote
#undef whole_wrapped
#undef wrap
#undef wrapped
#undef wrapped_note
#undef wrapped_of
#undef wrapped_sum
#undef write_bid
#undef write_quote_fields

namespace hgl::demo::structs::m_
{

namespace w_
{
// update_quote(is_bid: bool, price: i64) -> MarketQuote (node), module demo.structs, line 14
::hgraph::Port<::hgraph::NativeTSB<::hgl::demo::structs::m_::MarketQuote>> update_quote(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> is_bid, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> price);
}  // namespace w_

namespace w_
{
// bid_ticks(is_bid: bool, price: i64) -> i64 (graph), module demo.structs, line 21
::hgraph::Port<::hgraph::TS<::hgraph::Int>> bid_ticks(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> is_bid, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> price);
}  // namespace w_

namespace w_
{
// whole_quote(bid: i64, ask: i64) -> atomic<MarketQuote> (node), module demo.structs, line 30
::hgraph::Port<::hgraph::TS<::hgl::demo::structs::m_::MarketQuote>> whole_quote(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> bid, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> ask);
}  // namespace w_

namespace w_
{
// atomic_bid_ticks(bid: i64, ask: i64) -> i64 (graph), module demo.structs, line 34
::hgraph::Port<::hgraph::TS<::hgraph::Int>> atomic_bid_ticks(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> bid, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> ask);
}  // namespace w_

namespace w_
{
// write_quote_fields(bid: i64, ask: i64) -> MarketQuote (node), module demo.structs, line 43
::hgraph::Port<::hgraph::NativeTSB<::hgl::demo::structs::m_::MarketQuote>> write_quote_fields(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> bid, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> ask);
}  // namespace w_

namespace w_
{
// quote_valid(probe: bool, quote: MarketQuote) -> bool (node), module demo.structs, line 63
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> quote_valid(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::NativeTSB<::hgl::demo::structs::m_::MarketQuote>> quote);
}  // namespace w_

namespace w_
{
// quote_all_valid(probe: bool, quote: MarketQuote) -> bool (node), module demo.structs, line 67
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> quote_all_valid(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::NativeTSB<::hgl::demo::structs::m_::MarketQuote>> quote);
}  // namespace w_

namespace w_
{
// observe_quote_valid(probe: bool, is_bid: bool, price: i64) -> bool (graph), module demo.structs, line 71
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> observe_quote_valid(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> is_bid, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> price);
}  // namespace w_

namespace w_
{
// observe_quote_all_valid(probe: bool, is_bid: bool, price: i64) -> bool (graph), module demo.structs, line 74
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> observe_quote_all_valid(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> is_bid, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> price);
}  // namespace w_

namespace w_
{
// relay_quote(quote: MarketQuote) -> MarketQuote (node), module demo.structs, line 93
::hgraph::Port<::hgraph::NativeTSB<::hgl::demo::structs::m_::MarketQuote>> relay_quote(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::NativeTSB<::hgl::demo::structs::m_::MarketQuote>> quote);
}  // namespace w_

namespace w_
{
// set_one_part(first: bool, value: i64) -> tuple<i64, i64> (node), module demo.structs, line 111
::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Int>>>> set_one_part(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> first, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// part_validity(probe: bool, parts: tuple<i64, i64>) -> tuple<bool, bool> (node), module demo.structs, line 120
::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Bool>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Bool>>>> part_validity(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Int>>>> parts);
}  // namespace w_

namespace w_
{
// observe_part_validity(probe: bool, first: bool, value: i64) -> tuple<bool, bool> (graph), module demo.structs, line 124
::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Bool>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Bool>>>> observe_part_validity(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> first, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// write_bid(bid: i64) -> NotedQuote (node), module demo.structs, line 140
::hgraph::Port<::hgraph::NominalTSB<::hgl::demo::structs::m_::NotedQuote, ::hgraph::Field<"bid", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"note", ::hgraph::TS<::hgraph::Optional<::hgraph::Str>>>>> write_bid(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> bid);
}  // namespace w_

// note_is_null(quote: NotedQuote) -> bool (`const fn`), module demo.structs, line 144
::hgraph::Bool note_is_null(const ::hgl::demo::structs::m_::NotedQuote& quote);

namespace w_
{
// read_noted_quote(probe: bool, quote: NotedQuote) -> tuple<bool, bool, bool> (node), module demo.structs, line 146
::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Bool>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Bool>>, ::hgraph::Field<"2", ::hgraph::TS<::hgraph::Bool>>>> read_noted_quote(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::NominalTSB<::hgl::demo::structs::m_::NotedQuote, ::hgraph::Field<"bid", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"note", ::hgraph::TS<::hgraph::Optional<::hgraph::Str>>>>> quote);
}  // namespace w_

namespace w_
{
// observe_noted_quote(probe: bool, bid: i64) -> tuple<bool, bool, bool> (graph), module demo.structs, line 152
::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Bool>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Bool>>, ::hgraph::Field<"2", ::hgraph::TS<::hgraph::Bool>>>> observe_noted_quote(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> bid);
}  // namespace w_

namespace w_
{
// relay_pair(pair: tuple<i64, str>) -> tuple<i64, str> (node), module demo.structs, line 162
::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>> relay_pair(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>> pair);
}  // namespace w_

namespace w_
{
// make_outer(size: i64) -> Outer (node), module demo.structs, line 180
::hgraph::Port<::hgraph::NativeTSB<::hgl::demo::structs::m_::Outer>> make_outer(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> size);
}  // namespace w_

namespace w_
{
// read_outer(outer: Outer) -> i64 (node), module demo.structs, line 189
::hgraph::Port<::hgraph::TS<::hgraph::Int>> read_outer(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::NativeTSB<::hgl::demo::structs::m_::Outer>> outer);
}  // namespace w_

namespace w_
{
// outer_ticks(size: i64) -> i64 (graph), module demo.structs, line 197
::hgraph::Port<::hgraph::TS<::hgraph::Int>> outer_ticks(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> size);
}  // namespace w_

namespace w_
{
// outer_deltas(size: i64) -> Outer (node), module demo.structs, line 203
::hgraph::Port<::hgraph::NativeTSB<::hgl::demo::structs::m_::Outer>> outer_deltas(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> size);
}  // namespace w_

namespace w_
{
// bid_delta(quote: NotedQuote) -> i64? (node), module demo.structs, line 217
::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> bid_delta(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::NominalTSB<::hgl::demo::structs::m_::NotedQuote, ::hgraph::Field<"bid", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"note", ::hgraph::TS<::hgraph::Optional<::hgraph::Str>>>>> quote);
}  // namespace w_

namespace w_
{
// publish_nullable(command: i64) -> i64? (node), module demo.structs, line 231
::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> publish_nullable(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> command);
}  // namespace w_

namespace w_
{
// read_validity(probe: bool, watched: i64?) -> bool (node), module demo.structs, line 238
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> read_validity(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe, ::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> watched);
}  // namespace w_

namespace w_
{
// observe_validity(command: i64, probe: bool) -> bool (graph), module demo.structs, line 242
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> observe_validity(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> command, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe);
}  // namespace w_

namespace w_
{
// nullable_delta_payload(input: i64?) -> i64? (node), module demo.structs, line 251
::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> nullable_delta_payload(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> input);
}  // namespace w_

namespace w_
{
// doubled(reading: i64?) -> i64 (node), module demo.structs, line 262
::hgraph::Port<::hgraph::TS<::hgraph::Int>> doubled(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> reading);
}  // namespace w_

namespace w_
{
// action(code: i64) -> i64? (node), module demo.structs, line 273
::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> action(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> code);
}  // namespace w_

namespace w_
{
// sample_timestamp(sample: bool, watched: i64?) -> datetime? (node), module demo.structs, line 282
::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::DateTime>>> sample_timestamp(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> sample, ::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> watched);
}  // namespace w_

namespace w_
{
// scalar_timestamp(code: i64, sample: bool) -> datetime? (graph), module demo.structs, line 286
::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::DateTime>>> scalar_timestamp(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> code, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> sample);
}  // namespace w_

namespace w_
{
// whole_plain(bid: i64) -> atomic<PlainQuote> (node), module demo.structs, line 304
::hgraph::Port<::hgraph::TS<::hgl::demo::structs::m_::PlainQuote>> whole_plain(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> bid);
}  // namespace w_

namespace w_
{
// spread(quote: atomic<PlainQuote>) -> i64 (node), module demo.structs, line 308
::hgraph::Port<::hgraph::TS<::hgraph::Int>> spread(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgl::demo::structs::m_::PlainQuote>> quote);
}  // namespace w_

namespace w_
{
// spread_of(bid: i64) -> i64 (graph), module demo.structs, line 312
::hgraph::Port<::hgraph::TS<::hgraph::Int>> spread_of(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> bid);
}  // namespace w_

namespace w_
{
// pair_sum(pair: tuple<i64, i64>) -> i64 (node), module demo.structs, line 318
::hgraph::Port<::hgraph::TS<::hgraph::Int>> pair_sum(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Int>>>> pair);
}  // namespace w_

namespace w_
{
// first_of(pair: tuple<i64, str>) -> i64 (graph), module demo.structs, line 322
::hgraph::Port<::hgraph::TS<::hgraph::Int>> first_of(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Str>>>> pair);
}  // namespace w_

namespace w_
{
// wrap(bid: i64) -> Wrapped (node), module demo.structs, line 335
::hgraph::Port<::hgraph::NominalTSB<::hgl::demo::structs::m_::Wrapped, ::hgraph::Field<"noted", ::hgraph::NominalTSB<::hgl::demo::structs::m_::NotedQuote, ::hgraph::Field<"bid", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"note", ::hgraph::TS<::hgraph::Optional<::hgraph::Str>>>>>, ::hgraph::Field<"count", ::hgraph::TS<::hgraph::Int>>>> wrap(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> bid);
}  // namespace w_

namespace w_
{
// wrapped_sum(wrapped: Wrapped) -> i64 (node), module demo.structs, line 339
::hgraph::Port<::hgraph::TS<::hgraph::Int>> wrapped_sum(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::NominalTSB<::hgl::demo::structs::m_::Wrapped, ::hgraph::Field<"noted", ::hgraph::NominalTSB<::hgl::demo::structs::m_::NotedQuote, ::hgraph::Field<"bid", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"note", ::hgraph::TS<::hgraph::Optional<::hgraph::Str>>>>>, ::hgraph::Field<"count", ::hgraph::TS<::hgraph::Int>>>> wrapped);
}  // namespace w_

namespace w_
{
// wrapped_of(bid: i64) -> i64 (graph), module demo.structs, line 346
::hgraph::Port<::hgraph::TS<::hgraph::Int>> wrapped_of(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> bid);
}  // namespace w_

namespace w_
{
// atomic_bid(quote: atomic<MarketQuote>) -> i64 (node), module demo.structs, line 362
::hgraph::Port<::hgraph::TS<::hgraph::Int>> atomic_bid(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgl::demo::structs::m_::MarketQuote>> quote);
}  // namespace w_

namespace w_
{
// whole_noted(bid: i64) -> atomic<NotedQuote> (node), module demo.structs, line 377
::hgraph::Port<::hgraph::TS<::hgl::demo::structs::m_::NotedQuote>> whole_noted(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> bid);
}  // namespace w_

namespace w_
{
// note_or_none(quote: atomic<NotedQuote>) -> str (node), module demo.structs, line 384
::hgraph::Port<::hgraph::TS<::hgraph::Str>> note_or_none(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgl::demo::structs::m_::NotedQuote>> quote);
}  // namespace w_

namespace w_
{
// noted_note(bid: i64) -> str? (graph), module demo.structs, line 392
::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Str>>> noted_note(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> bid);
}  // namespace w_

namespace w_
{
// whole_wrapped(bid: i64) -> atomic<Wrapped> (node), module demo.structs, line 394
::hgraph::Port<::hgraph::TS<::hgl::demo::structs::m_::Wrapped>> whole_wrapped(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> bid);
}  // namespace w_

namespace w_
{
// wrapped_note(wrapped: atomic<Wrapped>) -> bool (node), module demo.structs, line 398
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> wrapped_note(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgl::demo::structs::m_::Wrapped>> wrapped);
}  // namespace w_

namespace w_
{
// held_count(holding: Holding) -> i64 (node), module demo.structs, line 409
::hgraph::Port<::hgraph::TS<::hgraph::Int>> held_count(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::NominalTSB<::hgl::demo::structs::m_::Holding, ::hgraph::Field<"quote", ::hgraph::TS<::hgl::demo::structs::m_::NotedQuote>>, ::hgraph::Field<"count", ::hgraph::TS<::hgraph::Int>>>> holding);
}  // namespace w_

namespace w_
{
// quoted_total(pair: tuple<MarketQuote, i64>) -> i64 (node), module demo.structs, line 434
::hgraph::Port<::hgraph::TS<::hgraph::Int>> quoted_total(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::NativeTSB<::hgl::demo::structs::m_::MarketQuote>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Int>>>> pair);
}  // namespace w_

namespace w_
{
// nested_total(pair: tuple<tuple<i64, i64>, i64>) -> i64 (node), module demo.structs, line 438
::hgraph::Port<::hgraph::TS<::hgraph::Int>> nested_total(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::UnNamedTSB<::hgraph::Field<"0", ::hgraph::TS<::hgraph::Int>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Int>>>>, ::hgraph::Field<"1", ::hgraph::TS<::hgraph::Int>>>> pair);
}  // namespace w_

}  // namespace hgl::demo::structs::m_
