// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include <hglrt/hglrt.h>

namespace hgl::demo::values::m_::t_
{
void conditions_choose();
void a_switch_compares_by_value();
void loops_run();
void defaults_and_nulls();
void generics_by_instance();
void tuples_and_constants();
void results_propagate();
void evaluation_follows_the_order_written();
void a_var_parameter_is_its_place();
void an_overflow_fails_with_its_code();
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
const ::hglrt::testing::Registration test_6{"demo.values::results_propagate", &::hgl::demo::values::m_::t_::results_propagate};
const ::hglrt::testing::Registration test_7{"demo.values::evaluation_follows_the_order_written", &::hgl::demo::values::m_::t_::evaluation_follows_the_order_written};
const ::hglrt::testing::Registration test_8{"demo.values::a_var_parameter_is_its_place", &::hgl::demo::values::m_::t_::a_var_parameter_is_its_place};
const ::hglrt::testing::Registration test_9{"demo.values::an_overflow_fails_with_its_code", &::hgl::demo::values::m_::t_::an_overflow_fails_with_its_code};
}  // namespace

int main(int argc, char **argv)
{
    return ::hglrt::testing::main(argc, argv);
}
