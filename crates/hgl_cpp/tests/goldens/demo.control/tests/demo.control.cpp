// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include <hglrt/host.h>

namespace hgl::demo::control::m_::t_
{
void a_conditional_child_restarts_only_after_leaving_its_branch();
void a_branch_runs_once_with_the_values_held_when_it_starts();
void an_outer_producer_keeps_its_lifetime_across_branches();
void an_early_return_skips_the_continuation();
void a_switch_forwards_the_selected_connection();
void a_probe_reads_a_selected_producer_that_stayed_quiet();
void an_outputless_branch_runs_only_while_selected();
void an_else_if_chain_selects_in_order();
void a_branch_reads_ordinary_values();
void a_var_keeps_its_binding_where_the_branch_does_not_assign_it();
void a_var_with_no_value_is_given_one_by_each_branch();
void a_switch_returns_from_a_case_or_goes_on();
void a_lifted_condition_selects();
void a_return_under_a_wiring_time_if_has_its_continuation();
void a_let_of_a_returning_if_returns_or_goes_on();
void a_switch_with_no_default_runs_a_case_each_value_has();
void a_branch_in_an_unrolled_loop_is_configured_for_its_pass();
void a_wiring_time_if_composes_one_branch();
}  // namespace hgl::demo::control::m_::t_

namespace
{
// Each test, by its identity, `module.path::name` (TST-15).
const ::hglrt::testing::Registration test_0{"demo.control::a_conditional_child_restarts_only_after_leaving_its_branch", &::hgl::demo::control::m_::t_::a_conditional_child_restarts_only_after_leaving_its_branch};
const ::hglrt::testing::Registration test_1{"demo.control::a_branch_runs_once_with_the_values_held_when_it_starts", &::hgl::demo::control::m_::t_::a_branch_runs_once_with_the_values_held_when_it_starts};
const ::hglrt::testing::Registration test_2{"demo.control::an_outer_producer_keeps_its_lifetime_across_branches", &::hgl::demo::control::m_::t_::an_outer_producer_keeps_its_lifetime_across_branches};
const ::hglrt::testing::Registration test_3{"demo.control::an_early_return_skips_the_continuation", &::hgl::demo::control::m_::t_::an_early_return_skips_the_continuation};
const ::hglrt::testing::Registration test_4{"demo.control::a_switch_forwards_the_selected_connection", &::hgl::demo::control::m_::t_::a_switch_forwards_the_selected_connection};
const ::hglrt::testing::Registration test_5{"demo.control::a_probe_reads_a_selected_producer_that_stayed_quiet", &::hgl::demo::control::m_::t_::a_probe_reads_a_selected_producer_that_stayed_quiet};
const ::hglrt::testing::Registration test_6{"demo.control::an_outputless_branch_runs_only_while_selected", &::hgl::demo::control::m_::t_::an_outputless_branch_runs_only_while_selected};
const ::hglrt::testing::Registration test_7{"demo.control::an_else_if_chain_selects_in_order", &::hgl::demo::control::m_::t_::an_else_if_chain_selects_in_order};
const ::hglrt::testing::Registration test_8{"demo.control::a_branch_reads_ordinary_values", &::hgl::demo::control::m_::t_::a_branch_reads_ordinary_values};
const ::hglrt::testing::Registration test_9{"demo.control::a_var_keeps_its_binding_where_the_branch_does_not_assign_it", &::hgl::demo::control::m_::t_::a_var_keeps_its_binding_where_the_branch_does_not_assign_it};
const ::hglrt::testing::Registration test_10{"demo.control::a_var_with_no_value_is_given_one_by_each_branch", &::hgl::demo::control::m_::t_::a_var_with_no_value_is_given_one_by_each_branch};
const ::hglrt::testing::Registration test_11{"demo.control::a_switch_returns_from_a_case_or_goes_on", &::hgl::demo::control::m_::t_::a_switch_returns_from_a_case_or_goes_on};
const ::hglrt::testing::Registration test_12{"demo.control::a_lifted_condition_selects", &::hgl::demo::control::m_::t_::a_lifted_condition_selects};
const ::hglrt::testing::Registration test_13{"demo.control::a_return_under_a_wiring_time_if_has_its_continuation", &::hgl::demo::control::m_::t_::a_return_under_a_wiring_time_if_has_its_continuation};
const ::hglrt::testing::Registration test_14{"demo.control::a_let_of_a_returning_if_returns_or_goes_on", &::hgl::demo::control::m_::t_::a_let_of_a_returning_if_returns_or_goes_on};
const ::hglrt::testing::Registration test_15{"demo.control::a_switch_with_no_default_runs_a_case_each_value_has", &::hgl::demo::control::m_::t_::a_switch_with_no_default_runs_a_case_each_value_has};
const ::hglrt::testing::Registration test_16{"demo.control::a_branch_in_an_unrolled_loop_is_configured_for_its_pass", &::hgl::demo::control::m_::t_::a_branch_in_an_unrolled_loop_is_configured_for_its_pass};
const ::hglrt::testing::Registration test_17{"demo.control::a_wiring_time_if_composes_one_branch", &::hgl::demo::control::m_::t_::a_wiring_time_if_composes_one_branch};
}  // namespace

int main(int argc, char **argv)
{
    return ::hglrt::testing::main(argc, argv);
}
