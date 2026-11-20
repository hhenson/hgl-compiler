// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#pragma once

#include <hglrt/hglrt.h>
#include "core.h"

namespace hgl::demo::natives::m_
{

// ratio(a: f64, b: f64) -> f64 (`const fn`), module demo.natives, line 8
::hgraph::Float ratio(::hgraph::Float a, ::hgraph::Float b);

// text(value: i64) -> str (`const fn`), module demo.natives, line 10
::hgraph::Str text(::hgraph::Int value);

// whole(value: f64) -> i64 (`const fn`), module demo.natives, line 12
::hgraph::Int whole(::hgraph::Float value);

// same(a: str, b: str) -> bool (`const fn`), module demo.natives, line 14
::hgraph::Bool same(const ::hgraph::Str& a, const ::hgraph::Str& b);

}  // namespace hgl::demo::natives::m_
