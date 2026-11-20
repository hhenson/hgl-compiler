// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include <hglrt/host.h>

// Names this file writes, which no macro of a header above takes.
#undef a_list_fails_before_its_index
#undef a_place_fails_at_its_list_first
#undef a_place_is_checked_before_its_value
#undef a_switch_compares_by_value
#undef a_type_declared_after_its_use
#undef a_var_parameter_is_its_place
#undef an_overflow_fails_with_its_code
#undef bits_are_values
#undef conditions_choose
#undef defaults_and_nulls
#undef demo
#undef evaluation_follows_the_order_written
#undef generics_by_instance
#undef loops_run
#undef null_tests_of_any_value
#undef results_propagate
#undef tuples_and_constants
#undef values

namespace hgl::demo::values::m_::t_
{
void conditions_choose();
void a_switch_compares_by_value();
void loops_run();
void defaults_and_nulls();
void generics_by_instance();
void tuples_and_constants();
void null_tests_of_any_value();
void bits_are_values();
void results_propagate();
void evaluation_follows_the_order_written();
void a_type_declared_after_its_use();
void a_var_parameter_is_its_place();
void an_overflow_fails_with_its_code();
void a_place_is_checked_before_its_value();
void a_place_fails_at_its_list_first();
void a_list_fails_before_its_index();
}  // namespace hgl::demo::values::m_::t_

namespace
{
// Each test, by its identity, `module.path::name` (TST-15).
const ::hglrt::testing::Registration test_0{"demo.values::conditions_choose", &::hgl::demo::values::m_::t_::conditions_choose};
const ::hglrt::testing::Registration test_1{"demo.values::a_switch_compares_by_value", &::hgl::demo::values::m_::t_::a_switch_compares_by_value};
const ::hglrt::testing::Registration test_2{"demo.values::loops_run", &::hgl::demo::values::m_::t_::loops_run};
const ::hglrt::testing::Registration test_3{"demo.values::defaults_and_nulls", &::hgl::demo::values::m_::t_::defaults_and_nulls};
const ::hglrt::testing::Registration test_4{"demo.values::generics_by_instance", &::hgl::demo::values::m_::t_::generics_by_instance};
const ::hglrt::testing::Registration test_5{"demo.values::tuples_and_constants", &::hgl::demo::values::m_::t_::tuples_and_constants};
const ::hglrt::testing::Registration test_6{"demo.values::null_tests_of_any_value", &::hgl::demo::values::m_::t_::null_tests_of_any_value};
const ::hglrt::testing::Registration test_7{"demo.values::bits_are_values", &::hgl::demo::values::m_::t_::bits_are_values};
const ::hglrt::testing::Registration test_8{"demo.values::results_propagate", &::hgl::demo::values::m_::t_::results_propagate};
const ::hglrt::testing::Registration test_9{"demo.values::evaluation_follows_the_order_written", &::hgl::demo::values::m_::t_::evaluation_follows_the_order_written};
const ::hglrt::testing::Registration test_10{"demo.values::a_type_declared_after_its_use", &::hgl::demo::values::m_::t_::a_type_declared_after_its_use};
const ::hglrt::testing::Registration test_11{"demo.values::a_var_parameter_is_its_place", &::hgl::demo::values::m_::t_::a_var_parameter_is_its_place};
const ::hglrt::testing::Registration test_12{"demo.values::an_overflow_fails_with_its_code", &::hgl::demo::values::m_::t_::an_overflow_fails_with_its_code};
const ::hglrt::testing::Registration test_13{"demo.values::a_place_is_checked_before_its_value", &::hgl::demo::values::m_::t_::a_place_is_checked_before_its_value};
const ::hglrt::testing::Registration test_14{"demo.values::a_place_fails_at_its_list_first", &::hgl::demo::values::m_::t_::a_place_fails_at_its_list_first};
const ::hglrt::testing::Registration test_15{"demo.values::a_list_fails_before_its_index", &::hgl::demo::values::m_::t_::a_list_fails_before_its_index};
}  // namespace

int main(int argc, char **argv)
{
    return ::hglrt::testing::main(argc, argv);
}
