// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#pragma once

#include <hglrt/hglrt.h>

// Names this file writes, which no macro of a header above takes.
#undef Level
#undef Side
#undef buy
#undef demo
#undef high
#undef hold
#undef low
#undef sell
#undef values

namespace hgl::demo::values::m_
{

// enum Side (TYPE-7)
enum class Side
{
    buy,
    sell,
    hold,
};
constexpr ::hglrt::EnumNames<Side, 3> hgl_enum(Side) noexcept
{
    return {"demo.values::Side", {{Side::buy, "buy"}, {Side::sell, "sell"}, {Side::hold, "hold"}}};
}

// enum Level (TYPE-7)
enum class Level
{
    low,
    high,
};
constexpr ::hglrt::EnumNames<Level, 2> hgl_enum(Level) noexcept
{
    return {"demo.values::Level", {{Level::low, "low"}, {Level::high, "high"}}};
}

}  // namespace hgl::demo::values::m_
