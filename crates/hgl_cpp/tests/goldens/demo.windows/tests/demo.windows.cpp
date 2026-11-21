// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include <hglrt/host.h>

// Names this file writes, which no macro of a header above takes.
#undef a_full_window_of_ticks_pushes_out_its_oldest
#undef a_lifted_helper_waits_for_all_valid
#undef a_read_before_all_valid_fails
#undef a_read_into_a_result_gives_an_error
#undef a_value_leaves_a_window_of_a_duration
#undef a_window_is_recorded_and_replayed
#undef a_window_of_a_duration_falls_back_and_stays_valid
#undef demo
#undef the_last_write_of_a_cycle_arrives
#undef two_handlers_write_one_arrival
#undef valid_before_all_valid
#undef windows

namespace hgl::demo::windows::m_::t_
{
void valid_before_all_valid();
void a_read_before_all_valid_fails();
void a_read_into_a_result_gives_an_error();
void a_lifted_helper_waits_for_all_valid();
void a_full_window_of_ticks_pushes_out_its_oldest();
void a_value_leaves_a_window_of_a_duration();
void a_window_of_a_duration_falls_back_and_stays_valid();
void a_window_is_recorded_and_replayed();
void the_last_write_of_a_cycle_arrives();
void two_handlers_write_one_arrival();
}  // namespace hgl::demo::windows::m_::t_

namespace
{
// Each test, by its identity, `module.path::name` (TST-15).
const ::hglrt::testing::Registration test_0{"demo.windows::valid_before_all_valid", &::hgl::demo::windows::m_::t_::valid_before_all_valid};
const ::hglrt::testing::Registration test_1{"demo.windows::a_read_before_all_valid_fails", &::hgl::demo::windows::m_::t_::a_read_before_all_valid_fails};
const ::hglrt::testing::Registration test_2{"demo.windows::a_read_into_a_result_gives_an_error", &::hgl::demo::windows::m_::t_::a_read_into_a_result_gives_an_error};
const ::hglrt::testing::Registration test_3{"demo.windows::a_lifted_helper_waits_for_all_valid", &::hgl::demo::windows::m_::t_::a_lifted_helper_waits_for_all_valid};
const ::hglrt::testing::Registration test_4{"demo.windows::a_full_window_of_ticks_pushes_out_its_oldest", &::hgl::demo::windows::m_::t_::a_full_window_of_ticks_pushes_out_its_oldest};
const ::hglrt::testing::Registration test_5{"demo.windows::a_value_leaves_a_window_of_a_duration", &::hgl::demo::windows::m_::t_::a_value_leaves_a_window_of_a_duration};
const ::hglrt::testing::Registration test_6{"demo.windows::a_window_of_a_duration_falls_back_and_stays_valid", &::hgl::demo::windows::m_::t_::a_window_of_a_duration_falls_back_and_stays_valid};
const ::hglrt::testing::Registration test_7{"demo.windows::a_window_is_recorded_and_replayed", &::hgl::demo::windows::m_::t_::a_window_is_recorded_and_replayed};
const ::hglrt::testing::Registration test_8{"demo.windows::the_last_write_of_a_cycle_arrives", &::hgl::demo::windows::m_::t_::the_last_write_of_a_cycle_arrives};
const ::hglrt::testing::Registration test_9{"demo.windows::two_handlers_write_one_arrival", &::hgl::demo::windows::m_::t_::two_handlers_write_one_arrival};
}  // namespace

int main(int argc, char **argv)
{
    return ::hglrt::testing::main(argc, argv);
}
