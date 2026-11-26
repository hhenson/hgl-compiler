// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include <hglrt/host.h>

// Names this file writes, which no macro of a header above takes.
#undef a_dictionary_of_sets
#undef a_dictionary_relayed_whole_publishes_each_entry
#undef a_dictionary_ticks_only_where_it_changes
#undef a_lifted_call_reads_the_dictionary_whole
#undef a_lifted_call_reads_the_set_whole
#undef a_set_of_floats
#undef a_set_relayed_whole_publishes_its_change
#undef a_set_ticks_only_where_it_changes
#undef an_entry_of_a_dictionary_is_read_checked
#undef demo
#undef ignored_removals_establish_only_missing_dictionary_validity
#undef keyed
#undef two_writes_of_a_set_are_one_change

namespace hgl::demo::keyed::m_::t_
{
void a_set_ticks_only_where_it_changes();
void two_writes_of_a_set_are_one_change();
void a_lifted_call_reads_the_set_whole();
void a_set_relayed_whole_publishes_its_change();
void a_set_of_floats();
void a_dictionary_ticks_only_where_it_changes();
void ignored_removals_establish_only_missing_dictionary_validity();
void a_lifted_call_reads_the_dictionary_whole();
void a_dictionary_relayed_whole_publishes_each_entry();
void a_dictionary_of_sets();
void an_entry_of_a_dictionary_is_read_checked();
}  // namespace hgl::demo::keyed::m_::t_

namespace
{
// Each test, by its identity, `module.path::name` (TST-15).
const ::hglrt::testing::Registration test_0{"demo.keyed::a_set_ticks_only_where_it_changes", &::hgl::demo::keyed::m_::t_::a_set_ticks_only_where_it_changes};
const ::hglrt::testing::Registration test_1{"demo.keyed::two_writes_of_a_set_are_one_change", &::hgl::demo::keyed::m_::t_::two_writes_of_a_set_are_one_change};
const ::hglrt::testing::Registration test_2{"demo.keyed::a_lifted_call_reads_the_set_whole", &::hgl::demo::keyed::m_::t_::a_lifted_call_reads_the_set_whole};
const ::hglrt::testing::Registration test_3{"demo.keyed::a_set_relayed_whole_publishes_its_change", &::hgl::demo::keyed::m_::t_::a_set_relayed_whole_publishes_its_change};
const ::hglrt::testing::Registration test_4{"demo.keyed::a_set_of_floats", &::hgl::demo::keyed::m_::t_::a_set_of_floats};
const ::hglrt::testing::Registration test_5{"demo.keyed::a_dictionary_ticks_only_where_it_changes", &::hgl::demo::keyed::m_::t_::a_dictionary_ticks_only_where_it_changes};
const ::hglrt::testing::Registration test_6{"demo.keyed::ignored_removals_establish_only_missing_dictionary_validity", &::hgl::demo::keyed::m_::t_::ignored_removals_establish_only_missing_dictionary_validity};
const ::hglrt::testing::Registration test_7{"demo.keyed::a_lifted_call_reads_the_dictionary_whole", &::hgl::demo::keyed::m_::t_::a_lifted_call_reads_the_dictionary_whole};
const ::hglrt::testing::Registration test_8{"demo.keyed::a_dictionary_relayed_whole_publishes_each_entry", &::hgl::demo::keyed::m_::t_::a_dictionary_relayed_whole_publishes_each_entry};
const ::hglrt::testing::Registration test_9{"demo.keyed::a_dictionary_of_sets", &::hgl::demo::keyed::m_::t_::a_dictionary_of_sets};
const ::hglrt::testing::Registration test_10{"demo.keyed::an_entry_of_a_dictionary_is_read_checked", &::hgl::demo::keyed::m_::t_::an_entry_of_a_dictionary_is_read_checked};
}  // namespace

int main(int argc, char **argv)
{
    return ::hglrt::testing::main(argc, argv);
}
