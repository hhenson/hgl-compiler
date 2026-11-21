// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include <hglrt/host.h>

// Names this file writes, which no macro of a header above takes.
#undef a_designated_input_is_followed_through_its_rebinding
#undef a_designated_position_is_followed_through_its_rebinding
#undef a_designated_position_outside_the_list_fails
#undef a_reader_observes_the_input_designated
#undef a_ref_parameter_is_present_and_wakes_once
#undef a_relay_keeps_what_it_sampled
#undef an_eval_records_a_ref_result_as_a_reader_does
#undef demo
#undef references

namespace hgl::demo::references::m_::t_
{
void a_reader_observes_the_input_designated();
void an_eval_records_a_ref_result_as_a_reader_does();
void a_ref_parameter_is_present_and_wakes_once();
void a_relay_keeps_what_it_sampled();
void a_designated_input_is_followed_through_its_rebinding();
void a_designated_position_is_followed_through_its_rebinding();
void a_designated_position_outside_the_list_fails();
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
}  // namespace

int main(int argc, char **argv)
{
    return ::hglrt::testing::main(argc, argv);
}
