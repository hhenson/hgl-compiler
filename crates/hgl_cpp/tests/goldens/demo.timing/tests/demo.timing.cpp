// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include <hglrt/host.h>

// Names this file writes, which no macro of a header above takes.
#undef a_resumed_source_keeps_its_locals_until_it_returns
#undef a_run_configuration_is_given_in_braces
#undef a_scheduler_wakes_a_node_with_no_input
#undef a_source_keeps_its_cache_and_state
#undef a_source_reads_its_clock_when_it_runs
#undef a_wake_before_now_is_dropped
#undef a_wake_can_coincide_with_an_input
#undef a_zero_duration_outside_start_fails
#undef absolute_targets_before_at_and_after_the_start
#undef an_alarm_wakes_a_node
#undef an_empty_selection_is_true_where_it_is_selected
#undef demo
#undef scheduled_is_read_as_a_value
#undef the_wake_delivered_is_pending_no_longer
#undef timing

namespace hgl::demo::timing::m_::t_
{
void a_scheduler_wakes_a_node_with_no_input();
void a_wake_can_coincide_with_an_input();
void an_empty_selection_is_true_where_it_is_selected();
void scheduled_is_read_as_a_value();
void the_wake_delivered_is_pending_no_longer();
void a_wake_before_now_is_dropped();
void a_zero_duration_outside_start_fails();
void an_alarm_wakes_a_node();
void a_source_reads_its_clock_when_it_runs();
void absolute_targets_before_at_and_after_the_start();
void a_resumed_source_keeps_its_locals_until_it_returns();
void a_source_keeps_its_cache_and_state();
void a_run_configuration_is_given_in_braces();
}  // namespace hgl::demo::timing::m_::t_

namespace
{
// Each test, by its identity, `module.path::name` (TST-15).
const ::hglrt::testing::Registration test_0{"demo.timing::a_scheduler_wakes_a_node_with_no_input", &::hgl::demo::timing::m_::t_::a_scheduler_wakes_a_node_with_no_input};
const ::hglrt::testing::Registration test_1{"demo.timing::a_wake_can_coincide_with_an_input", &::hgl::demo::timing::m_::t_::a_wake_can_coincide_with_an_input};
const ::hglrt::testing::Registration test_2{"demo.timing::an_empty_selection_is_true_where_it_is_selected", &::hgl::demo::timing::m_::t_::an_empty_selection_is_true_where_it_is_selected};
const ::hglrt::testing::Registration test_3{"demo.timing::scheduled_is_read_as_a_value", &::hgl::demo::timing::m_::t_::scheduled_is_read_as_a_value};
const ::hglrt::testing::Registration test_4{"demo.timing::the_wake_delivered_is_pending_no_longer", &::hgl::demo::timing::m_::t_::the_wake_delivered_is_pending_no_longer};
const ::hglrt::testing::Registration test_5{"demo.timing::a_wake_before_now_is_dropped", &::hgl::demo::timing::m_::t_::a_wake_before_now_is_dropped};
const ::hglrt::testing::Registration test_6{"demo.timing::a_zero_duration_outside_start_fails", &::hgl::demo::timing::m_::t_::a_zero_duration_outside_start_fails};
const ::hglrt::testing::Registration test_7{"demo.timing::an_alarm_wakes_a_node", &::hgl::demo::timing::m_::t_::an_alarm_wakes_a_node};
const ::hglrt::testing::Registration test_8{"demo.timing::a_source_reads_its_clock_when_it_runs", &::hgl::demo::timing::m_::t_::a_source_reads_its_clock_when_it_runs};
const ::hglrt::testing::Registration test_9{"demo.timing::absolute_targets_before_at_and_after_the_start", &::hgl::demo::timing::m_::t_::absolute_targets_before_at_and_after_the_start};
const ::hglrt::testing::Registration test_10{"demo.timing::a_resumed_source_keeps_its_locals_until_it_returns", &::hgl::demo::timing::m_::t_::a_resumed_source_keeps_its_locals_until_it_returns};
const ::hglrt::testing::Registration test_11{"demo.timing::a_source_keeps_its_cache_and_state", &::hgl::demo::timing::m_::t_::a_source_keeps_its_cache_and_state};
const ::hglrt::testing::Registration test_12{"demo.timing::a_run_configuration_is_given_in_braces", &::hgl::demo::timing::m_::t_::a_run_configuration_is_given_in_braces};
}  // namespace

int main(int argc, char **argv)
{
    return ::hglrt::testing::main(argc, argv);
}
