// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include "demo.calc.h"

namespace hgl::demo::calc::m_
{

::hgraph::Int total([[maybe_unused]] ::hgraph::Int a, [[maybe_unused]] ::hgraph::Int b)
{
    return ::hglrt::add__i64_i64(a, b);
}

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
    ::hglrt::statement(__LINE__);
    ::hglrt::check(::hglrt::eq__i64_i64_f64_f64(::hgl::demo::calc::m_::total(2, 3), 5, 0.0, 0.0));
    ::hglrt::statement(__LINE__);
    [[maybe_unused]] const ::hgraph::Int large = ::hgl::demo::calc::m_::twice(21);
    ::hglrt::statement(__LINE__);
    ::hglrt::check(::hglrt::eq__i64_i64_f64_f64(large, 42, 0.0, 0.0));
}

void overflows()
{
    ::hglrt::statement(__LINE__);
    ::hglrt::check(::hglrt::eq__i64_i64_f64_f64(::hgl::demo::calc::m_::total(9223372036854775807, 1), 0, 0.0, 0.0));
}

}  // namespace hgl::demo::calc::m_::t_
