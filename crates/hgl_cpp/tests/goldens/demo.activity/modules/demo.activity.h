// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#pragma once

#include <hglrt/hglrt.h>

// Names this file writes, which no macro of a header above takes.
#undef Mode
#undef activity
#undef bindings
#undef data
#undef demo
#undef driving
#undef gated
#undef ignore
#undef listen
#undef listening
#undef mode
#undef mode_of
#undef same_twice
#undef sampled
#undef value

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

namespace w_
{
// gated(mode: Mode, data: i64) -> i64 (node), module demo.activity, line 13
::hgraph::Port<::hgraph::TS<::hgraph::Int>> gated(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgl::demo::activity::m_::Mode>> mode, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> data);
}  // namespace w_

namespace w_
{
// mode_of(value: i64) -> Mode (node), module demo.activity, line 23
::hgraph::Port<::hgraph::TS<::hgl::demo::activity::m_::Mode>> mode_of(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// bindings(driving: i64, sampled: i64) -> bool (node), module demo.activity, line 32
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> bindings(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> driving, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> sampled);
}  // namespace w_

namespace w_
{
// same_twice(value: i64) -> bool (graph), module demo.activity, line 36
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> same_twice(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value);
}  // namespace w_

namespace w_
{
// listening(data: i64) -> i64 (graph), module demo.activity, line 39
::hgraph::Port<::hgraph::TS<::hgraph::Int>> listening(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> data);
}  // namespace w_

}  // namespace hgl::demo::activity::m_
