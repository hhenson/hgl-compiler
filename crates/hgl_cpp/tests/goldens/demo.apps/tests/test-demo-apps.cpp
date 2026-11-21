// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include <hglrt/host.h>

// Names this file writes, which no macro of a header above takes.
#undef apps
#undef demo
#undef the_store_is_set_where_the_graph_is_built

namespace hgl::demo::apps::m_::t_
{
void the_store_is_set_where_the_graph_is_built();
}  // namespace hgl::demo::apps::m_::t_

namespace
{
// Each test, by its identity, `module.path::name` (TST-15).
const ::hglrt::testing::Registration test_0{"demo.apps::the_store_is_set_where_the_graph_is_built", &::hgl::demo::apps::m_::t_::the_store_is_set_where_the_graph_is_built};
}  // namespace

int main(int argc, char **argv)
{
    return ::hglrt::testing::main(argc, argv);
}
