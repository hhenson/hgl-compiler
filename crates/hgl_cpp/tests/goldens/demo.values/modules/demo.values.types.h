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

// enum Level (TYPE-7)
enum class Level
{
    low,
    high,
};

}  // namespace hgl::demo::values::m_
