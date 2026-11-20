// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include <hglrt/hglrt.h>

namespace hgl::demo::natives::m_::t_
{
void natives_are_called();
}  // namespace hgl::demo::natives::m_::t_

namespace
{
// Each test, by its identity, `module.path::name` (TST-15).
const ::hglrt::testing::Registration test_0{"demo.natives::natives_are_called", &::hgl::demo::natives::m_::t_::natives_are_called};
}  // namespace

int main(int argc, char **argv)
{
    return ::hglrt::testing::main(argc, argv);
}
