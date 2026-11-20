// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include <hglrt/host.h>

// Names this file writes, which no macro of a header above takes.
#undef adds_one
#undef an_outputless_target_runs
#undef demo
#undef handlers_run_in_order
#undef sources_and_constants
#undef temporal
#undef the_clock_is_read

namespace hgl::demo::temporal::m_::t_
{
void adds_one();
void handlers_run_in_order();
void sources_and_constants();
void the_clock_is_read();
void an_outputless_target_runs();
}  // namespace hgl::demo::temporal::m_::t_

namespace
{
// Each test, by its identity, `module.path::name` (TST-15).
const ::hglrt::testing::Registration test_0{"demo.temporal::adds_one", &::hgl::demo::temporal::m_::t_::adds_one};
const ::hglrt::testing::Registration test_1{"demo.temporal::handlers_run_in_order", &::hgl::demo::temporal::m_::t_::handlers_run_in_order};
const ::hglrt::testing::Registration test_2{"demo.temporal::sources_and_constants", &::hgl::demo::temporal::m_::t_::sources_and_constants};
const ::hglrt::testing::Registration test_3{"demo.temporal::the_clock_is_read", &::hgl::demo::temporal::m_::t_::the_clock_is_read};
const ::hglrt::testing::Registration test_4{"demo.temporal::an_outputless_target_runs", &::hgl::demo::temporal::m_::t_::an_outputless_target_runs};
}  // namespace

int main(int argc, char **argv)
{
    return ::hglrt::testing::main(argc, argv);
}
