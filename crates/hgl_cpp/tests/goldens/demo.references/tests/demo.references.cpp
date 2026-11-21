// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include <hglrt/host.h>

// Names this file writes, which no macro of a header above takes.
#undef a_branch_given_an_empty_reference_is_silent
#undef a_designated_input_is_followed_through_its_rebinding
#undef a_designated_position_is_followed_through_its_rebinding
#undef a_designated_position_outside_the_list_fails
#undef a_header_that_asks_valid_runs_no_handler_for_an_empty_reference
#undef a_lifted_call_of_an_empty_reference_is_silent
#undef a_reader_observes_the_input_designated
#undef a_ref_parameter_is_present_and_wakes_once
#undef a_relay_keeps_what_it_sampled
#undef a_selected_entry_is_followed_through_its_key
#undef a_selected_entry_the_dictionary_lacks_is_not_valid
#undef an_empty_reference_is_handed_on
#undef an_entry_at_a_value_key_is_followed
#undef an_eval_records_a_ref_result_as_a_reader_does
#undef demo
#undef references
#undef valid_of_a_ref_parameter_that_holds_no_reference_is_false

namespace hgl::demo::references::m_::t_
{
void a_reader_observes_the_input_designated();
void an_eval_records_a_ref_result_as_a_reader_does();
void a_ref_parameter_is_present_and_wakes_once();
void a_relay_keeps_what_it_sampled();
void a_designated_input_is_followed_through_its_rebinding();
void a_designated_position_is_followed_through_its_rebinding();
void a_designated_position_outside_the_list_fails();
void a_selected_entry_is_followed_through_its_key();
void a_selected_entry_the_dictionary_lacks_is_not_valid();
void an_entry_at_a_value_key_is_followed();
void an_empty_reference_is_handed_on();
void a_header_that_asks_valid_runs_no_handler_for_an_empty_reference();
void valid_of_a_ref_parameter_that_holds_no_reference_is_false();
void a_lifted_call_of_an_empty_reference_is_silent();
void a_branch_given_an_empty_reference_is_silent();
}  // namespace hgl::demo::references::m_::t_

namespace
{
// Each test, by its identity, `module.path::name` (TST-15).
const ::hglrt::testing::Registration test_0{"demo.references::a_reader_observes_the_input_designated", &::hgl::demo::references::m_::t_::a_reader_observes_the_input_designated};
const ::hglrt::testing::Registration test_1{"demo.references::an_eval_records_a_ref_result_as_a_reader_does", &::hgl::demo::references::m_::t_::an_eval_records_a_ref_result_as_a_reader_does};
const ::hglrt::testing::Registration test_2{"demo.references::a_ref_parameter_is_present_and_wakes_once", &::hgl::demo::references::m_::t_::a_ref_parameter_is_present_and_wakes_once};
const ::hglrt::testing::Registration test_3{"demo.references::a_relay_keeps_what_it_sampled", &::hgl::demo::references::m_::t_::a_relay_keeps_what_it_sampled};
const ::hglrt::testing::Registration test_4{"demo.references::a_designated_input_is_followed_through_its_rebinding", &::hgl::demo::references::m_::t_::a_designated_input_is_followed_through_its_rebinding};
const ::hglrt::testing::Registration test_5{"demo.references::a_designated_position_is_followed_through_its_rebinding", &::hgl::demo::references::m_::t_::a_designated_position_is_followed_through_its_rebinding};
const ::hglrt::testing::Registration test_6{"demo.references::a_designated_position_outside_the_list_fails", &::hgl::demo::references::m_::t_::a_designated_position_outside_the_list_fails};
const ::hglrt::testing::Registration test_7{"demo.references::a_selected_entry_is_followed_through_its_key", &::hgl::demo::references::m_::t_::a_selected_entry_is_followed_through_its_key};
const ::hglrt::testing::Registration test_8{"demo.references::a_selected_entry_the_dictionary_lacks_is_not_valid", &::hgl::demo::references::m_::t_::a_selected_entry_the_dictionary_lacks_is_not_valid};
const ::hglrt::testing::Registration test_9{"demo.references::an_entry_at_a_value_key_is_followed", &::hgl::demo::references::m_::t_::an_entry_at_a_value_key_is_followed};
const ::hglrt::testing::Registration test_10{"demo.references::an_empty_reference_is_handed_on", &::hgl::demo::references::m_::t_::an_empty_reference_is_handed_on};
const ::hglrt::testing::Registration test_11{"demo.references::a_header_that_asks_valid_runs_no_handler_for_an_empty_reference", &::hgl::demo::references::m_::t_::a_header_that_asks_valid_runs_no_handler_for_an_empty_reference};
const ::hglrt::testing::Registration test_12{"demo.references::valid_of_a_ref_parameter_that_holds_no_reference_is_false", &::hgl::demo::references::m_::t_::valid_of_a_ref_parameter_that_holds_no_reference_is_false};
const ::hglrt::testing::Registration test_13{"demo.references::a_lifted_call_of_an_empty_reference_is_silent", &::hgl::demo::references::m_::t_::a_lifted_call_of_an_empty_reference_is_silent};
const ::hglrt::testing::Registration test_14{"demo.references::a_branch_given_an_empty_reference_is_silent", &::hgl::demo::references::m_::t_::a_branch_given_an_empty_reference_is_silent};
}  // namespace

int main(int argc, char **argv)
{
    return ::hglrt::testing::main(argc, argv);
}
