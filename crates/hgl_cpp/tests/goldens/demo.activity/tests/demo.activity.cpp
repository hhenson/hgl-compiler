// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include <hglrt/host.h>

namespace hgl::demo::activity::m_::t_
{
void a_passive_input_wakes_no_node();
void an_enum_is_published();
void activity_belongs_to_the_binding();
void an_enum_is_a_constant_connection();
}  // namespace hgl::demo::activity::m_::t_

namespace
{
// Each test, by its identity, `module.path::name` (TST-15).
const ::hglrt::testing::Registration test_0{"demo.activity::a_passive_input_wakes_no_node", &::hgl::demo::activity::m_::t_::a_passive_input_wakes_no_node};
const ::hglrt::testing::Registration test_1{"demo.activity::an_enum_is_published", &::hgl::demo::activity::m_::t_::an_enum_is_published};
const ::hglrt::testing::Registration test_2{"demo.activity::activity_belongs_to_the_binding", &::hgl::demo::activity::m_::t_::activity_belongs_to_the_binding};
const ::hglrt::testing::Registration test_3{"demo.activity::an_enum_is_a_constant_connection", &::hgl::demo::activity::m_::t_::an_enum_is_a_constant_connection};
}  // namespace

int main(int argc, char **argv)
{
    return ::hglrt::testing::main(argc, argv);
}
