// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include <hglrt/host.h>

// Names this file writes, which no macro of a header above takes.
#undef a_narrowed_read
#undef a_nested_struct_delta_is_recorded
#undef a_nested_struct_field_is_written_and_read_by_its_parts
#undef a_published_null_has_a_delta
#undef a_struct_delta_keeps_the_final_write_to_each_field
#undef a_struct_delta_reads_the_field_that_changed
#undef a_struct_is_all_valid_once_every_field_is
#undef a_struct_is_valid_once_one_field_is
#undef a_struct_that_holds_one_with_an_optional_field_is_read_whole
#undef a_struct_with_an_optional_field_is_all_valid_and_read_whole
#undef a_tuple_is_valid_once_one_position_is_and_all_valid_once_both_are
#undef a_tuple_position_is_read_and_projected
#undef a_whole_struct_is_a_trace_cell
#undef a_whole_tuple_is_a_trace_cell
#undef an_atomic_input_is_read_whole
#undef an_atomic_quote_wakes_its_bid_projection
#undef changing_a_sibling_field_does_not_publish_a_bid
#undef demo
#undef last_modified_follows_the_last_publication
#undef publication_establishes_validity
#undef structs

namespace hgl::demo::structs::m_::t_
{
void changing_a_sibling_field_does_not_publish_a_bid();
void an_atomic_quote_wakes_its_bid_projection();
void a_struct_delta_keeps_the_final_write_to_each_field();
void a_struct_is_valid_once_one_field_is();
void a_struct_is_all_valid_once_every_field_is();
void a_whole_struct_is_a_trace_cell();
void a_tuple_is_valid_once_one_position_is_and_all_valid_once_both_are();
void a_struct_with_an_optional_field_is_all_valid_and_read_whole();
void a_whole_tuple_is_a_trace_cell();
void a_nested_struct_field_is_written_and_read_by_its_parts();
void a_nested_struct_delta_is_recorded();
void a_struct_delta_reads_the_field_that_changed();
void publication_establishes_validity();
void a_published_null_has_a_delta();
void a_narrowed_read();
void last_modified_follows_the_last_publication();
void an_atomic_input_is_read_whole();
void a_tuple_position_is_read_and_projected();
void a_struct_that_holds_one_with_an_optional_field_is_read_whole();
}  // namespace hgl::demo::structs::m_::t_

namespace
{
// Each test, by its identity, `module.path::name` (TST-15).
const ::hglrt::testing::Registration test_0{"demo.structs::changing_a_sibling_field_does_not_publish_a_bid", &::hgl::demo::structs::m_::t_::changing_a_sibling_field_does_not_publish_a_bid};
const ::hglrt::testing::Registration test_1{"demo.structs::an_atomic_quote_wakes_its_bid_projection", &::hgl::demo::structs::m_::t_::an_atomic_quote_wakes_its_bid_projection};
const ::hglrt::testing::Registration test_2{"demo.structs::a_struct_delta_keeps_the_final_write_to_each_field", &::hgl::demo::structs::m_::t_::a_struct_delta_keeps_the_final_write_to_each_field};
const ::hglrt::testing::Registration test_3{"demo.structs::a_struct_is_valid_once_one_field_is", &::hgl::demo::structs::m_::t_::a_struct_is_valid_once_one_field_is};
const ::hglrt::testing::Registration test_4{"demo.structs::a_struct_is_all_valid_once_every_field_is", &::hgl::demo::structs::m_::t_::a_struct_is_all_valid_once_every_field_is};
const ::hglrt::testing::Registration test_5{"demo.structs::a_whole_struct_is_a_trace_cell", &::hgl::demo::structs::m_::t_::a_whole_struct_is_a_trace_cell};
const ::hglrt::testing::Registration test_6{"demo.structs::a_tuple_is_valid_once_one_position_is_and_all_valid_once_both_are", &::hgl::demo::structs::m_::t_::a_tuple_is_valid_once_one_position_is_and_all_valid_once_both_are};
const ::hglrt::testing::Registration test_7{"demo.structs::a_struct_with_an_optional_field_is_all_valid_and_read_whole", &::hgl::demo::structs::m_::t_::a_struct_with_an_optional_field_is_all_valid_and_read_whole};
const ::hglrt::testing::Registration test_8{"demo.structs::a_whole_tuple_is_a_trace_cell", &::hgl::demo::structs::m_::t_::a_whole_tuple_is_a_trace_cell};
const ::hglrt::testing::Registration test_9{"demo.structs::a_nested_struct_field_is_written_and_read_by_its_parts", &::hgl::demo::structs::m_::t_::a_nested_struct_field_is_written_and_read_by_its_parts};
const ::hglrt::testing::Registration test_10{"demo.structs::a_nested_struct_delta_is_recorded", &::hgl::demo::structs::m_::t_::a_nested_struct_delta_is_recorded};
const ::hglrt::testing::Registration test_11{"demo.structs::a_struct_delta_reads_the_field_that_changed", &::hgl::demo::structs::m_::t_::a_struct_delta_reads_the_field_that_changed};
const ::hglrt::testing::Registration test_12{"demo.structs::publication_establishes_validity", &::hgl::demo::structs::m_::t_::publication_establishes_validity};
const ::hglrt::testing::Registration test_13{"demo.structs::a_published_null_has_a_delta", &::hgl::demo::structs::m_::t_::a_published_null_has_a_delta};
const ::hglrt::testing::Registration test_14{"demo.structs::a_narrowed_read", &::hgl::demo::structs::m_::t_::a_narrowed_read};
const ::hglrt::testing::Registration test_15{"demo.structs::last_modified_follows_the_last_publication", &::hgl::demo::structs::m_::t_::last_modified_follows_the_last_publication};
const ::hglrt::testing::Registration test_16{"demo.structs::an_atomic_input_is_read_whole", &::hgl::demo::structs::m_::t_::an_atomic_input_is_read_whole};
const ::hglrt::testing::Registration test_17{"demo.structs::a_tuple_position_is_read_and_projected", &::hgl::demo::structs::m_::t_::a_tuple_position_is_read_and_projected};
const ::hglrt::testing::Registration test_18{"demo.structs::a_struct_that_holds_one_with_an_optional_field_is_read_whole", &::hgl::demo::structs::m_::t_::a_struct_that_holds_one_with_an_optional_field_is_read_whole};
}  // namespace

int main(int argc, char **argv)
{
    return ::hglrt::testing::main(argc, argv);
}
