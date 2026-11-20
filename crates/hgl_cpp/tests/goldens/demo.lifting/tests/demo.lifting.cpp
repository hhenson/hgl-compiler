// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include <hglrt/host.h>

namespace hgl::demo::lifting::m_::t_
{
void a_default_binds_until_its_input_publishes();
void a_sink_fails_and_a_capture_of_it_publishes_the_error();
void a_failure_and_a_returned_error_are_told_apart();
void a_capture_keeps_the_child_live();
void an_earlier_write_stands();
void a_raised_error_keeps_its_provenance();
}  // namespace hgl::demo::lifting::m_::t_

namespace
{
// Each test, by its identity, `module.path::name` (TST-15).
const ::hglrt::testing::Registration test_0{"demo.lifting::a_default_binds_until_its_input_publishes", &::hgl::demo::lifting::m_::t_::a_default_binds_until_its_input_publishes};
const ::hglrt::testing::Registration test_1{"demo.lifting::a_sink_fails_and_a_capture_of_it_publishes_the_error", &::hgl::demo::lifting::m_::t_::a_sink_fails_and_a_capture_of_it_publishes_the_error};
const ::hglrt::testing::Registration test_2{"demo.lifting::a_failure_and_a_returned_error_are_told_apart", &::hgl::demo::lifting::m_::t_::a_failure_and_a_returned_error_are_told_apart};
const ::hglrt::testing::Registration test_3{"demo.lifting::a_capture_keeps_the_child_live", &::hgl::demo::lifting::m_::t_::a_capture_keeps_the_child_live};
const ::hglrt::testing::Registration test_4{"demo.lifting::an_earlier_write_stands", &::hgl::demo::lifting::m_::t_::an_earlier_write_stands};
const ::hglrt::testing::Registration test_5{"demo.lifting::a_raised_error_keeps_its_provenance", &::hgl::demo::lifting::m_::t_::a_raised_error_keeps_its_provenance};
}  // namespace

int main(int argc, char **argv)
{
    return ::hglrt::testing::main(argc, argv);
}
