// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#pragma once

#include <hglrt/hglrt.h>

// Names this file writes, which no macro of a header above takes.
#undef Mode
#undef activity
#undef demo
#undef ignore
#undef listen

namespace hgl::demo::activity::m_
{

// enum Mode (TYPE-7)
enum class Mode
{
    listen,
    ignore,
};
constexpr ::hglrt::EnumNames<Mode, 2> hgl_enum(Mode) noexcept
{
    return {"demo.activity::Mode", {{Mode::listen, "listen"}, {Mode::ignore, "ignore"}}};
}

}  // namespace hgl::demo::activity::m_
