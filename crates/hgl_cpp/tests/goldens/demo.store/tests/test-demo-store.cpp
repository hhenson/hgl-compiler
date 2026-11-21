// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include <hglrt/host.h>

// Names this file writes, which no macro of a header above takes.
#undef a_struct_is_got_back_in_a_node
#undef a_struct_is_got_back_where_a_graph_is_built
#undef a_tuple_a_dictionary_a_set_and_a_delta_are_got_back
#undef demo
#undef store

namespace hgl::demo::store::m_::t_
{
void a_struct_is_got_back_in_a_node();
void a_tuple_a_dictionary_a_set_and_a_delta_are_got_back();
void a_struct_is_got_back_where_a_graph_is_built();
}  // namespace hgl::demo::store::m_::t_

namespace
{
// Each test, by its identity, `module.path::name` (TST-15).
const ::hglrt::testing::Registration test_0{"demo.store::a_struct_is_got_back_in_a_node", &::hgl::demo::store::m_::t_::a_struct_is_got_back_in_a_node};
const ::hglrt::testing::Registration test_1{"demo.store::a_tuple_a_dictionary_a_set_and_a_delta_are_got_back", &::hgl::demo::store::m_::t_::a_tuple_a_dictionary_a_set_and_a_delta_are_got_back};
const ::hglrt::testing::Registration test_2{"demo.store::a_struct_is_got_back_where_a_graph_is_built", &::hgl::demo::store::m_::t_::a_struct_is_got_back_where_a_graph_is_built};
}  // namespace

int main(int argc, char **argv)
{
    return ::hglrt::testing::main(argc, argv);
}
