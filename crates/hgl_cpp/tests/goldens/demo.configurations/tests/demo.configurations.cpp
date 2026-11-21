// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include <hglrt/host.h>

// Names this file writes, which no macro of a header above takes.
#undef configurations
#undef configurations_that_differ_are_two_nodes
#undef demo
#undef lifted_calls_and_branches_are_given_any_value
#undef nodes_are_given_any_value
#undef sources_are_given_a_set_and_a_dictionary

namespace hgl::demo::configurations::m_::t_
{
void sources_are_given_a_set_and_a_dictionary();
void nodes_are_given_any_value();
void configurations_that_differ_are_two_nodes();
void lifted_calls_and_branches_are_given_any_value();
}  // namespace hgl::demo::configurations::m_::t_

namespace
{
// Each test, by its identity, `module.path::name` (TST-15).
const ::hglrt::testing::Registration test_0{"demo.configurations::sources_are_given_a_set_and_a_dictionary", &::hgl::demo::configurations::m_::t_::sources_are_given_a_set_and_a_dictionary};
const ::hglrt::testing::Registration test_1{"demo.configurations::nodes_are_given_any_value", &::hgl::demo::configurations::m_::t_::nodes_are_given_any_value};
const ::hglrt::testing::Registration test_2{"demo.configurations::configurations_that_differ_are_two_nodes", &::hgl::demo::configurations::m_::t_::configurations_that_differ_are_two_nodes};
const ::hglrt::testing::Registration test_3{"demo.configurations::lifted_calls_and_branches_are_given_any_value", &::hgl::demo::configurations::m_::t_::lifted_calls_and_branches_are_given_any_value};
}  // namespace

int main(int argc, char **argv)
{
    return ::hglrt::testing::main(argc, argv);
}
