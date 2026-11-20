// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#pragma once

#include <hglrt/hglrt.h>

namespace hgl::demo::control::m_
{

namespace w_
{
// count_publications(value: i64) -> i64 (node), module demo.control, line 8
::hgraph::Port<::hgraph::TS<::hgraph::Int>> count_publications(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// halved(divisor: f64) -> f64 (node), module demo.control, line 16
::hgraph::Port<::hgraph::TS<::hgraph::Float>> halved(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Float>> divisor);
}  // namespace w_

namespace w_
{
// overflow_on_large(value: i64) (node), module demo.control, line 20
void overflow_on_large(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// sample_on_probe(watched: i64, probe: bool) -> i64 (node), module demo.control, line 24
::hgraph::Port<::hgraph::TS<::hgraph::Int>> sample_on_probe(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> watched, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe);
}  // namespace w_

namespace w_
{
// count_inside_branch(enabled: bool, value: i64) -> i64 (graph), module demo.control, line 32
::hgraph::Port<::hgraph::TS<::hgraph::Int>> count_inside_branch(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> enabled, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// count_outside_branch(enabled: bool, value: i64) -> i64 (graph), module demo.control, line 52
::hgraph::Port<::hgraph::TS<::hgraph::Int>> count_outside_branch(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> enabled, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// bypass_ratio(bypass: bool, divisor: f64) -> f64 (graph), module demo.control, line 65
::hgraph::Port<::hgraph::TS<::hgraph::Float>> bypass_ratio(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> bypass, ::hgraph::Port<::hgraph::TS<::hgraph::Float>> divisor);
}  // namespace w_

namespace w_
{
// switch_input(route: str, left: i64, right: i64, fallback: i64) -> i64 (graph), module demo.control, line 78
::hgraph::Port<::hgraph::TS<::hgraph::Int>> switch_input(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Str>> route, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> left, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> right, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> fallback);
}  // namespace w_

namespace w_
{
// sample_selection(use_left: bool, left: i64, right: i64, probe: bool) -> i64 (graph), module demo.control, line 98
::hgraph::Port<::hgraph::TS<::hgraph::Int>> sample_selection(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> use_left, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> left, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> right, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe);
}  // namespace w_

namespace w_
{
// guarded(open: bool, value: i64) -> i64 (graph), module demo.control, line 113
::hgraph::Port<::hgraph::TS<::hgraph::Int>> guarded(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> open, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// three_way(a: bool, b: bool, x: i64, y: i64, z: i64) -> i64 (graph), module demo.control, line 126
::hgraph::Port<::hgraph::TS<::hgraph::Int>> three_way(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> a, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> b, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> y, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> z);
}  // namespace w_

namespace w_
{
// scaled(open: bool, value: i64, const factor: i64) -> i64 (graph), module demo.control, line 140
::hgraph::Port<::hgraph::TS<::hgraph::Int>> scaled(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> open, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value, ::hgraph::Int factor);
}  // namespace w_

namespace w_
{
// adjusted(rising: bool, price: i64) -> i64 (graph), module demo.control, line 152
::hgraph::Port<::hgraph::TS<::hgraph::Int>> adjusted(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> rising, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> price);
}  // namespace w_

namespace w_
{
// either(rising: bool, price: i64) -> i64 (graph), module demo.control, line 165
::hgraph::Port<::hgraph::TS<::hgraph::Int>> either(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> rising, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> price);
}  // namespace w_

namespace w_
{
// routed(route: i64, value: i64) -> i64 (graph), module demo.control, line 176
::hgraph::Port<::hgraph::TS<::hgraph::Int>> routed(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> route, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// large(value: i64) -> i64 (graph), module demo.control, line 190
::hgraph::Port<::hgraph::TS<::hgraph::Int>> large(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// chosen_by_config(const wide: bool, open: bool, value: i64) -> i64 (graph), module demo.control, line 200
::hgraph::Port<::hgraph::TS<::hgraph::Int>> chosen_by_config(::hgraph::Wiring& wiring_, ::hgraph::Bool wide, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> open, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// early(open: bool, value: i64) -> i64 (graph), module demo.control, line 216
::hgraph::Port<::hgraph::TS<::hgraph::Int>> early(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> open, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// unmatched(route: str, value: i64) -> i64 (graph), module demo.control, line 226
::hgraph::Port<::hgraph::TS<::hgraph::Int>> unmatched(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Str>> route, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// gated_product(open: bool, price: i64, const factors: list<i64>) -> i64 (graph), module demo.control, line 241
::hgraph::Port<::hgraph::TS<::hgraph::Int>> gated_product(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> open, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> price, const ::hglrt::list<::hgraph::Int>& factors);
}  // namespace w_

namespace w_
{
// sized(price: i64, const wide: bool) -> i64 (graph), module demo.control, line 256
::hgraph::Port<::hgraph::TS<::hgraph::Int>> sized(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> price, ::hgraph::Bool wide);
}  // namespace w_

namespace w_
{
// gated_by_config(price: i64, const open: bool) -> i64 (graph), module demo.control, line 260
::hgraph::Port<::hgraph::TS<::hgraph::Int>> gated_by_config(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> price, ::hgraph::Bool open);
}  // namespace w_

namespace w_
{
// mixed(price: i64, const wide: bool) -> i64 (graph), module demo.control, line 265
::hgraph::Port<::hgraph::TS<::hgraph::Int>> mixed(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> price, ::hgraph::Bool wide);
}  // namespace w_

}  // namespace hgl::demo::control::m_
