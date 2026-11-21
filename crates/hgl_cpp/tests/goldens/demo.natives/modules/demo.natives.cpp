// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include "demo.natives.h"

// Names this file writes, which no macro of a header above takes.
#undef InvalidPolicy
#undef Rounding
#undef ZeroPolicy
#undef a
#undef b
#undef demo
#undef fail
#undef half_even
#undef nan
#undef natives
#undef natives_are_called
#undef ratio
#undef same
#undef text
#undef value
#undef whole

namespace hgl::demo::natives::m_
{

// ratio(a: f64, b: f64) -> f64 (`const fn`), module demo.natives, line 8
::hgraph::Float ratio([[maybe_unused]] ::hgraph::Float a, [[maybe_unused]] ::hgraph::Float b)
{
    return ::hglrt::div__f64_f64<::hglrt::ZeroPolicy::nan>(a, b);
}

// text(value: i64) -> str (`const fn`), module demo.natives, line 10
::hgraph::Str text([[maybe_unused]] ::hgraph::Int value)
{
    return ::hglrt::convert_i64<::hgraph::Str>(value);
}

// whole(value: f64) -> i64 (`const fn`), module demo.natives, line 12
::hgraph::Int whole([[maybe_unused]] ::hgraph::Float value)
{
    return ::hglrt::convert_f64_Rounding_i64_<::hglrt::InvalidPolicy::fail>(value, ::hglrt::Rounding::half_even, ::std::nullopt);
}

// same(a: str, b: str) -> bool (`const fn`), module demo.natives, line 14
::hgraph::Bool same([[maybe_unused]] const ::hgraph::Str& a, [[maybe_unused]] const ::hgraph::Str& b)
{
    return ::hglrt::eq__T_T<::hgraph::Str>(a, b);
}

}  // namespace hgl::demo::natives::m_

namespace hgl::demo::natives::m_::t_
{

void natives_are_called()
{
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__f64_f64_f64_f64(::hgl::demo::natives::m_::ratio(1.0, 2.0), 0.5, 0.0, 0.0));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__T_T<::hgraph::Str>(::hgl::demo::natives::m_::text(42), ::hgraph::Str{"42"}));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__i64_i64_f64_f64(::hgl::demo::natives::m_::whole(2.5), 2, 0.0, 0.0));
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hgl::demo::natives::m_::same(::hgraph::Str{"a"}, ::hgraph::Str{"a"}));
}

}  // namespace hgl::demo::natives::m_::t_
