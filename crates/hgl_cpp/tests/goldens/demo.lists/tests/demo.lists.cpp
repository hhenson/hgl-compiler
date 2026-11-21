// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include <hglrt/host.h>

// Names this file writes, which no macro of a header above takes.
#undef a_growing_list_child_is_wired
#undef a_list_input_replays_its_cells
#undef a_list_of_lists_is_replayed
#undef children_named_in_a_hook
#undef demo
#undef fixed_list_children
#undef lists
#undef published_whole_and_by_delta
#undef tuple_positions

namespace hgl::demo::lists::m_::t_
{
void published_whole_and_by_delta();
void fixed_list_children();
void children_named_in_a_hook();
void tuple_positions();
void a_list_input_replays_its_cells();
void a_growing_list_child_is_wired();
void a_list_of_lists_is_replayed();
}  // namespace hgl::demo::lists::m_::t_

namespace
{
// Each test, by its identity, `module.path::name` (TST-15).
const ::hglrt::testing::Registration test_0{"demo.lists::published_whole_and_by_delta", &::hgl::demo::lists::m_::t_::published_whole_and_by_delta};
const ::hglrt::testing::Registration test_1{"demo.lists::fixed_list_children", &::hgl::demo::lists::m_::t_::fixed_list_children};
const ::hglrt::testing::Registration test_2{"demo.lists::children_named_in_a_hook", &::hgl::demo::lists::m_::t_::children_named_in_a_hook};
const ::hglrt::testing::Registration test_3{"demo.lists::tuple_positions", &::hgl::demo::lists::m_::t_::tuple_positions};
const ::hglrt::testing::Registration test_4{"demo.lists::a_list_input_replays_its_cells", &::hgl::demo::lists::m_::t_::a_list_input_replays_its_cells};
const ::hglrt::testing::Registration test_5{"demo.lists::a_growing_list_child_is_wired", &::hgl::demo::lists::m_::t_::a_growing_list_child_is_wired};
const ::hglrt::testing::Registration test_6{"demo.lists::a_list_of_lists_is_replayed", &::hgl::demo::lists::m_::t_::a_list_of_lists_is_replayed};
}  // namespace

int main(int argc, char **argv)
{
    return ::hglrt::testing::main(argc, argv);
}
