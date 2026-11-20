// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include <hglrt/host.h>

namespace hgl::demo::calc::m_::t_
{
void sums();
void overflows();
}  // namespace hgl::demo::calc::m_::t_

namespace
{
// Each test by its identity, `module.path::name` (TST-15).
const ::hglrt::TestCase cases[] = {
    {"demo.calc::sums", &::hgl::demo::calc::m_::t_::sums},
    {"demo.calc::overflows", &::hgl::demo::calc::m_::t_::overflows},
};
}  // namespace

int main(int argc, char **argv)
{
    return ::hglrt::run_tests(cases, argc, argv);
}
