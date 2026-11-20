// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include <hglrt/host.h>

namespace hgl::demo::composites::m_::t_
{
void structs_are_values();
void fields_default_and_nest();
void a_struct_holding_nan_is_not_equal_to_itself();
void generic_and_abstract_structs();
void arguments_are_evaluated_in_the_order_written();
void sets_have_no_order();
void dictionaries_read_add_and_loop();
void a_missing_key_fails();
void deltas_are_values();
void enums_zones_and_nullables();
void a_var_place_is_taken_where_the_call_is_made();
}  // namespace hgl::demo::composites::m_::t_

namespace
{
// Each test, by its identity, `module.path::name` (TST-15).
const ::hglrt::testing::Registration test_0{"demo.composites::structs_are_values", &::hgl::demo::composites::m_::t_::structs_are_values};
const ::hglrt::testing::Registration test_1{"demo.composites::fields_default_and_nest", &::hgl::demo::composites::m_::t_::fields_default_and_nest};
const ::hglrt::testing::Registration test_2{"demo.composites::a_struct_holding_nan_is_not_equal_to_itself", &::hgl::demo::composites::m_::t_::a_struct_holding_nan_is_not_equal_to_itself};
const ::hglrt::testing::Registration test_3{"demo.composites::generic_and_abstract_structs", &::hgl::demo::composites::m_::t_::generic_and_abstract_structs};
const ::hglrt::testing::Registration test_4{"demo.composites::arguments_are_evaluated_in_the_order_written", &::hgl::demo::composites::m_::t_::arguments_are_evaluated_in_the_order_written};
const ::hglrt::testing::Registration test_5{"demo.composites::sets_have_no_order", &::hgl::demo::composites::m_::t_::sets_have_no_order};
const ::hglrt::testing::Registration test_6{"demo.composites::dictionaries_read_add_and_loop", &::hgl::demo::composites::m_::t_::dictionaries_read_add_and_loop};
const ::hglrt::testing::Registration test_7{"demo.composites::a_missing_key_fails", &::hgl::demo::composites::m_::t_::a_missing_key_fails};
const ::hglrt::testing::Registration test_8{"demo.composites::deltas_are_values", &::hgl::demo::composites::m_::t_::deltas_are_values};
const ::hglrt::testing::Registration test_9{"demo.composites::enums_zones_and_nullables", &::hgl::demo::composites::m_::t_::enums_zones_and_nullables};
const ::hglrt::testing::Registration test_10{"demo.composites::a_var_place_is_taken_where_the_call_is_made", &::hgl::demo::composites::m_::t_::a_var_place_is_taken_where_the_call_is_made};
}  // namespace

int main(int argc, char **argv)
{
    return ::hglrt::testing::main(argc, argv);
}
