// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#pragma once

#include <hglrt/hglrt.h>

// Names this file writes, which no macro of a header above takes.
#undef a
#undef b
#undef calc
#undef demo
#undef total
#undef twice
#undef value

namespace hgl::demo::calc::m_
{

// total(a: i64, b: i64) -> i64 (`const fn`), module demo.calc, line 4
::hgraph::Int total(::hgraph::Int a, ::hgraph::Int b);

// twice(value: i64) -> i64 (`const fn`), module demo.calc, line 6
::hgraph::Int twice(::hgraph::Int value);

}  // namespace hgl::demo::calc::m_
