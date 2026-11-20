// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include "demo.calc.h"

// Names this file writes, which no macro of a header above takes.
#undef a
#undef b
#undef calc
#undef demo
#undef doubled
#undef large
#undef overflows
#undef sums
#undef total
#undef twice
#undef value

namespace hgl::demo::calc::m_
{

// total(a: i64, b: i64) -> i64 (`const fn`), module demo.calc, line 4
::hgraph::Int total([[maybe_unused]] ::hgraph::Int a, [[maybe_unused]] ::hgraph::Int b)
{
    return ::hglrt::add__i64_i64(a, b);
}

// twice(value: i64) -> i64 (`const fn`), module demo.calc, line 6
::hgraph::Int twice([[maybe_unused]] ::hgraph::Int value)
{
    [[maybe_unused]] const ::hgraph::Int doubled = ::hgl::demo::calc::m_::total(value, value);
    return doubled;
}

}  // namespace hgl::demo::calc::m_

namespace hgl::demo::calc::m_::t_
{

void sums()
{
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__i64_i64_f64_f64(::hgl::demo::calc::m_::total(2, 3), 5, 0.0, 0.0));
    ::hglrt::testing::statement();
    [[maybe_unused]] const ::hgraph::Int large = ::hgl::demo::calc::m_::twice(21);
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__i64_i64_f64_f64(large, 42, 0.0, 0.0));
}

void overflows()
{
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__i64_i64_f64_f64(::hgl::demo::calc::m_::total(9223372036854775807, 1), 0, 0.0, 0.0));
}

}  // namespace hgl::demo::calc::m_::t_
