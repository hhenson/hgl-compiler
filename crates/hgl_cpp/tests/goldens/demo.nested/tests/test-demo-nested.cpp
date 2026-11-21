// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include <hglrt/host.h>

// Names this file writes, which no macro of a header above takes.
#undef a_child_of_a_list_of_tuples_is_wired
#undef a_dictionary_of_tuples_is_published_by_a_delta
#undef a_dictionary_of_tuples_is_relayed_whole
#undef a_dictionary_of_tuples_replays_its_deltas
#undef a_fixed_list_of_tuples_is_published_by_a_delta
#undef a_fixed_list_of_tuples_is_relayed_whole
#undef a_key_child_of_a_dictionary_of_tuples_is_wired
#undef a_list_of_tuples_is_published_by_a_delta
#undef a_list_of_tuples_is_relayed_whole
#undef a_list_of_tuples_replays_its_deltas
#undef a_struct_with_a_list_field_is_a_configuration
#undef a_struct_with_a_list_field_is_published_by_a_delta
#undef a_struct_with_a_list_field_is_relayed_whole
#undef a_struct_with_a_list_field_is_stored
#undef a_struct_with_a_list_field_replays_its_cells
#undef a_tuple_of_a_list_is_published_by_a_delta
#undef a_tuple_of_a_list_is_relayed_whole
#undef a_tuple_of_a_list_replays_its_cells
#undef demo
#undef nested

namespace hgl::demo::nested::m_::t_
{
void a_list_of_tuples_is_relayed_whole();
void a_list_of_tuples_is_published_by_a_delta();
void a_list_of_tuples_replays_its_deltas();
void a_fixed_list_of_tuples_is_relayed_whole();
void a_fixed_list_of_tuples_is_published_by_a_delta();
void a_tuple_of_a_list_is_relayed_whole();
void a_tuple_of_a_list_is_published_by_a_delta();
void a_tuple_of_a_list_replays_its_cells();
void a_dictionary_of_tuples_is_relayed_whole();
void a_dictionary_of_tuples_is_published_by_a_delta();
void a_dictionary_of_tuples_replays_its_deltas();
void a_struct_with_a_list_field_is_relayed_whole();
void a_struct_with_a_list_field_is_published_by_a_delta();
void a_struct_with_a_list_field_replays_its_cells();
void a_child_of_a_list_of_tuples_is_wired();
void a_key_child_of_a_dictionary_of_tuples_is_wired();
void a_struct_with_a_list_field_is_a_configuration();
void a_struct_with_a_list_field_is_stored();
}  // namespace hgl::demo::nested::m_::t_

namespace
{
// Each test, by its identity, `module.path::name` (TST-15).
const ::hglrt::testing::Registration test_0{"demo.nested::a_list_of_tuples_is_relayed_whole", &::hgl::demo::nested::m_::t_::a_list_of_tuples_is_relayed_whole};
const ::hglrt::testing::Registration test_1{"demo.nested::a_list_of_tuples_is_published_by_a_delta", &::hgl::demo::nested::m_::t_::a_list_of_tuples_is_published_by_a_delta};
const ::hglrt::testing::Registration test_2{"demo.nested::a_list_of_tuples_replays_its_deltas", &::hgl::demo::nested::m_::t_::a_list_of_tuples_replays_its_deltas};
const ::hglrt::testing::Registration test_3{"demo.nested::a_fixed_list_of_tuples_is_relayed_whole", &::hgl::demo::nested::m_::t_::a_fixed_list_of_tuples_is_relayed_whole};
const ::hglrt::testing::Registration test_4{"demo.nested::a_fixed_list_of_tuples_is_published_by_a_delta", &::hgl::demo::nested::m_::t_::a_fixed_list_of_tuples_is_published_by_a_delta};
const ::hglrt::testing::Registration test_5{"demo.nested::a_tuple_of_a_list_is_relayed_whole", &::hgl::demo::nested::m_::t_::a_tuple_of_a_list_is_relayed_whole};
const ::hglrt::testing::Registration test_6{"demo.nested::a_tuple_of_a_list_is_published_by_a_delta", &::hgl::demo::nested::m_::t_::a_tuple_of_a_list_is_published_by_a_delta};
const ::hglrt::testing::Registration test_7{"demo.nested::a_tuple_of_a_list_replays_its_cells", &::hgl::demo::nested::m_::t_::a_tuple_of_a_list_replays_its_cells};
const ::hglrt::testing::Registration test_8{"demo.nested::a_dictionary_of_tuples_is_relayed_whole", &::hgl::demo::nested::m_::t_::a_dictionary_of_tuples_is_relayed_whole};
const ::hglrt::testing::Registration test_9{"demo.nested::a_dictionary_of_tuples_is_published_by_a_delta", &::hgl::demo::nested::m_::t_::a_dictionary_of_tuples_is_published_by_a_delta};
const ::hglrt::testing::Registration test_10{"demo.nested::a_dictionary_of_tuples_replays_its_deltas", &::hgl::demo::nested::m_::t_::a_dictionary_of_tuples_replays_its_deltas};
const ::hglrt::testing::Registration test_11{"demo.nested::a_struct_with_a_list_field_is_relayed_whole", &::hgl::demo::nested::m_::t_::a_struct_with_a_list_field_is_relayed_whole};
const ::hglrt::testing::Registration test_12{"demo.nested::a_struct_with_a_list_field_is_published_by_a_delta", &::hgl::demo::nested::m_::t_::a_struct_with_a_list_field_is_published_by_a_delta};
const ::hglrt::testing::Registration test_13{"demo.nested::a_struct_with_a_list_field_replays_its_cells", &::hgl::demo::nested::m_::t_::a_struct_with_a_list_field_replays_its_cells};
const ::hglrt::testing::Registration test_14{"demo.nested::a_child_of_a_list_of_tuples_is_wired", &::hgl::demo::nested::m_::t_::a_child_of_a_list_of_tuples_is_wired};
const ::hglrt::testing::Registration test_15{"demo.nested::a_key_child_of_a_dictionary_of_tuples_is_wired", &::hgl::demo::nested::m_::t_::a_key_child_of_a_dictionary_of_tuples_is_wired};
const ::hglrt::testing::Registration test_16{"demo.nested::a_struct_with_a_list_field_is_a_configuration", &::hgl::demo::nested::m_::t_::a_struct_with_a_list_field_is_a_configuration};
const ::hglrt::testing::Registration test_17{"demo.nested::a_struct_with_a_list_field_is_stored", &::hgl::demo::nested::m_::t_::a_struct_with_a_list_field_is_stored};
}  // namespace

int main(int argc, char **argv)
{
    return ::hglrt::testing::main(argc, argv);
}
