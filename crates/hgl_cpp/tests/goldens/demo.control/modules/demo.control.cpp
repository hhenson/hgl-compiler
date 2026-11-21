// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include "demo.control.h"
#include "core.h"

// Names this file writes, which no macro of a header above takes.
#undef ZeroPolicy
#undef a
#undef a_branch_in_an_unrolled_loop_is_configured_for_its_pass
#undef a_branch_reads_ordinary_values
#undef a_branch_runs_once_with_the_values_held_when_it_starts
#undef a_conditional_child_restarts_only_after_leaving_its_branch
#undef a_let_of_a_returning_if_returns_or_goes_on
#undef a_lifted_condition_selects
#undef a_probe_reads_a_selected_producer_that_stayed_quiet
#undef a_return_under_a_wiring_time_if_has_its_continuation
#undef a_switch_forwards_the_selected_connection
#undef a_switch_returns_from_a_case_or_goes_on
#undef a_switch_with_no_default_runs_a_case_each_value_has
#undef a_var_keeps_its_binding_where_the_branch_does_not_assign_it
#undef a_var_with_no_value_is_given_one_by_each_branch
#undef a_wiring_time_if_composes_one_branch
#undef adjusted
#undef an_early_return_skips_the_continuation
#undef an_else_if_chain_selects_in_order
#undef an_outer_producer_keeps_its_lifetime_across_branches
#undef an_outputless_branch_runs_only_while_selected
#undef b
#undef bypass
#undef bypass_ratio
#undef checked
#undef chosen
#undef chosen_by_config
#undef control
#undef core
#undef count
#undef count_inside_branch
#undef count_outside_branch
#undef count_publications
#undef counted
#undef demo
#undef divisor
#undef early
#undef either
#undef enabled
#undef factor
#undef factors
#undef fallback
#undef gated_by_config
#undef gated_product
#undef guarded
#undef halved
#undef ieee
#undef large
#undef left
#undef lhs
#undef mixed
#undef never
#undef offset
#undef open
#undef out
#undef overflow_on_large
#undef price
#undef probe
#undef record
#undef replay
#undef rhs
#undef right
#undef rising
#undef route
#undef routed
#undef running
#undef sample_on_probe
#undef sample_selection
#undef scaled
#undef selected
#undef sized
#undef start
#undef switch_input
#undef three_way
#undef unmatched
#undef use_left
#undef v
#undef value
#undef watched
#undef wide
#undef x
#undef y
#undef z

namespace hgl::demo::control::m_
{

namespace n_::count_publications
{
// count_publications(value: i64) -> i64 (node), module demo.control, line 8
struct impl_
{
    static constexpr auto name = "demo.control::count_publications";

    // The node's `state` and `cache` (TIME-6).
    struct state_type_
    {
        ::hgraph::Int count{};
    };

    static void start([[maybe_unused]] ::hgraph::State<state_type_> state_)
    {
        state_.set(state_type_{});
        state_type_& kept_ = state_.modify();
        [[maybe_unused]] auto& count = kept_.count;
        count = 0;
    }

    static void eval([[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::State<state_type_> state_, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        state_type_& kept_ = state_.modify();
        [[maybe_unused]] auto& count = kept_.count;
        // when, line 10 (TIME-3)
        {
            count = ::hglrt::add__i64_i64(count, 1);
            out_.set(count);
            return;
        }
    }
};
}  // namespace n_::count_publications
}  // namespace hgl::demo::control::m_

// The state of the node, as hgraph holds it (TIME-6).
template <>
struct hgraph::scalar_descriptor<::hgl::demo::control::m_::n_::count_publications::impl_::state_type_>
{
    [[nodiscard]] static constexpr bool is_concrete() noexcept { return true; }
    [[nodiscard]] static const ValueTypeMetaData* value_meta()
    {
        return TypeRegistry::instance().register_scalar<::hgl::demo::control::m_::n_::count_publications::impl_::state_type_>("hgl:demo.control::count_publications.state");
    }
};

namespace hgl::demo::control::m_
{
namespace w_
{
// count_publications(value: i64) -> i64 (node), module demo.control, line 8
::hgraph::Port<::hgraph::TS<::hgraph::Int>> count_publications(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgraph::wire<::hgl::demo::control::m_::n_::count_publications::impl_>(wiring_, value);
}
}  // namespace w_

namespace n_::halved
{
// halved(divisor: f64) -> f64 (node), module demo.control, line 16
struct impl_
{
    static constexpr auto name = "demo.control::halved";

    static void eval([[maybe_unused]] ::hgraph::In<"divisor", ::hgraph::TS<::hgraph::Float>> divisor, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Float>> out_)
    {
        // when, line 17 (TIME-3)
        {
            out_.set(::hglrt::div__f64_f64<::hglrt::ZeroPolicy::ieee>(::hglrt::checked_value(divisor), 2.0));
            return;
        }
    }
};
}  // namespace n_::halved
namespace w_
{
// halved(divisor: f64) -> f64 (node), module demo.control, line 16
::hgraph::Port<::hgraph::TS<::hgraph::Float>> halved(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Float>> divisor)
{
    return ::hgraph::wire<::hgl::demo::control::m_::n_::halved::impl_>(wiring_, divisor);
}
}  // namespace w_

namespace n_::overflow_on_large
{
// overflow_on_large(value: i64) (node), module demo.control, line 20
struct impl_
{
    static constexpr auto name = "demo.control::overflow_on_large";

    static void eval([[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>> value)
    {
        // when, line 21 (TIME-3)
        {
            [[maybe_unused]] const ::hgraph::Int checked = ::hglrt::mul__i64_i64(::hglrt::checked_value(value), 4611686018427387904);
        }
    }
};
}  // namespace n_::overflow_on_large
namespace w_
{
// overflow_on_large(value: i64) (node), module demo.control, line 20
void overflow_on_large(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    ::hgraph::wire<::hgl::demo::control::m_::n_::overflow_on_large::impl_>(wiring_, value);
}
}  // namespace w_

namespace n_::sample_on_probe
{
// sample_on_probe(watched: i64, probe: bool) -> i64 (node), module demo.control, line 24
struct impl_
{
    static constexpr auto name = "demo.control::sample_on_probe";

    static void eval([[maybe_unused]] ::hgraph::In<"watched", ::hgraph::TS<::hgraph::Int>, ::hgraph::InputActivity::Passive> watched, [[maybe_unused]] ::hgraph::In<"probe", ::hgraph::TS<::hgraph::Bool>> probe, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 25 (TIME-3)
        if (::hglrt::fn_modified(probe))
        {
            out_.set(::hglrt::checked_value(watched));
            return;
        }
    }
};
}  // namespace n_::sample_on_probe
namespace w_
{
// sample_on_probe(watched: i64, probe: bool) -> i64 (node), module demo.control, line 24
::hgraph::Port<::hgraph::TS<::hgraph::Int>> sample_on_probe(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> watched, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe)
{
    return ::hgraph::wire<::hgl::demo::control::m_::n_::sample_on_probe::impl_>(wiring_, watched, probe);
}
}  // namespace w_

namespace n_::count_inside_branch
{
// count_inside_branch(enabled: bool, value: i64) -> i64 (graph), module demo.control, line 32
struct impl_
{
    // The `if` on line 33 over a connection (CTRL-2, CTRL-13 to CTRL-15): a child
    // graph for each branch, of which hgraph's `switch_` runs the one selected.
    struct switch_00_
    {
        // The branch selected while the condition is `true`.
        struct branch_00_
        {
            static constexpr auto name = "demo.control:if on line 33";
            static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose([[maybe_unused]] ::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
            {
                return ::hgl::demo::control::m_::w_::count_publications(wiring_, value);
            }
        };
    
        // The branch selected while the condition is `false`.
        struct branch_01_
        {
            static constexpr auto name = "demo.control:if on line 33";
            static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose([[maybe_unused]] ::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
            {
                return ::hglrt::never<::hgraph::TS<::hgraph::Int>>(wiring_);
            }
        };
    
        static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> wire_(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> key_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
        {
            ::std::vector<::hgraph::SwitchBranchGraph> branches_;
            branches_.push_back(::hgraph::SwitchBranchGraph{::hgraph::Value{static_cast<::hgraph::Bool>(true)}, ::hgraph::compile_subgraph<branch_00_>(wiring_)});
            branches_.push_back(::hgraph::SwitchBranchGraph{::hgraph::Value{static_cast<::hgraph::Bool>(false)}, ::hgraph::compile_subgraph<branch_01_>(wiring_)});
            const ::std::array<::hgraph::WiringPortRef, 1> inputs_{value.erased()};
            return ::hgraph::Port<void>{wiring_, ::hgraph::compiled::switch_(wiring_, key_.erased(), inputs_, ::std::move(branches_))}.as<::hgraph::TS<::hgraph::Int>>();
        }
    };

    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> enabled, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
    {
        return switch_00_::wire_(wiring_, enabled, value);
    }
};
}  // namespace n_::count_inside_branch
namespace w_
{
// count_inside_branch(enabled: bool, value: i64) -> i64 (graph), module demo.control, line 32
::hgraph::Port<::hgraph::TS<::hgraph::Int>> count_inside_branch(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> enabled, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgl::demo::control::m_::n_::count_inside_branch::impl_::compose(wiring_, enabled, value);
}
}  // namespace w_

namespace n_::count_outside_branch
{
// count_outside_branch(enabled: bool, value: i64) -> i64 (graph), module demo.control, line 52
struct impl_
{
    // The `if` on line 54 over a connection (CTRL-2, CTRL-13 to CTRL-15): a child
    // graph for each branch, of which hgraph's `switch_` runs the one selected.
    struct switch_00_
    {
        // The branch selected while the condition is `true`.
        struct branch_00_
        {
            static constexpr auto name = "demo.control:if on line 54";
            static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose([[maybe_unused]] ::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> count)
            {
                return count;
            }
        };
    
        // The branch selected while the condition is `false`.
        struct branch_01_
        {
            static constexpr auto name = "demo.control:if on line 54";
            static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose([[maybe_unused]] ::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> count)
            {
                return ::hglrt::never<::hgraph::TS<::hgraph::Int>>(wiring_);
            }
        };
    
        static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> wire_(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> key_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> count)
        {
            ::std::vector<::hgraph::SwitchBranchGraph> branches_;
            branches_.push_back(::hgraph::SwitchBranchGraph{::hgraph::Value{static_cast<::hgraph::Bool>(true)}, ::hgraph::compile_subgraph<branch_00_>(wiring_)});
            branches_.push_back(::hgraph::SwitchBranchGraph{::hgraph::Value{static_cast<::hgraph::Bool>(false)}, ::hgraph::compile_subgraph<branch_01_>(wiring_)});
            const ::std::array<::hgraph::WiringPortRef, 1> inputs_{count.erased()};
            return ::hgraph::Port<void>{wiring_, ::hgraph::compiled::switch_(wiring_, key_.erased(), inputs_, ::std::move(branches_))}.as<::hgraph::TS<::hgraph::Int>>();
        }
    };

    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> enabled, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
    {
        [[maybe_unused]] const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> count = ::hgl::demo::control::m_::w_::count_publications(wiring_, value);
        return switch_00_::wire_(wiring_, enabled, count);
    }
};
}  // namespace n_::count_outside_branch
namespace w_
{
// count_outside_branch(enabled: bool, value: i64) -> i64 (graph), module demo.control, line 52
::hgraph::Port<::hgraph::TS<::hgraph::Int>> count_outside_branch(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> enabled, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgl::demo::control::m_::n_::count_outside_branch::impl_::compose(wiring_, enabled, value);
}
}  // namespace w_

namespace n_::bypass_ratio
{
// bypass_ratio(bypass: bool, divisor: f64) -> f64 (graph), module demo.control, line 65
struct impl_
{
    // The `if` on line 66 over a connection (CTRL-2, CTRL-13 to CTRL-15): a child
    // graph for each branch, of which hgraph's `switch_` runs the one selected.
    struct switch_00_
    {
        // The branch selected while the condition is `true`.
        struct branch_00_
        {
            static constexpr auto name = "demo.control:if on line 66";
            static ::hgraph::Port<::hgraph::TS<::hgraph::Float>> compose([[maybe_unused]] ::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Float>> divisor)
            {
                return divisor;
            }
        };
    
        // The branch selected while the condition is `false`.
        struct branch_01_
        {
            static constexpr auto name = "demo.control:if on line 66";
            static ::hgraph::Port<::hgraph::TS<::hgraph::Float>> compose([[maybe_unused]] ::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Float>> divisor)
            {
                return ::hgl::demo::control::m_::w_::halved(wiring_, divisor);
            }
        };
    
        static ::hgraph::Port<::hgraph::TS<::hgraph::Float>> wire_(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> key_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Float>> divisor)
        {
            ::std::vector<::hgraph::SwitchBranchGraph> branches_;
            branches_.push_back(::hgraph::SwitchBranchGraph{::hgraph::Value{static_cast<::hgraph::Bool>(true)}, ::hgraph::compile_subgraph<branch_00_>(wiring_)});
            branches_.push_back(::hgraph::SwitchBranchGraph{::hgraph::Value{static_cast<::hgraph::Bool>(false)}, ::hgraph::compile_subgraph<branch_01_>(wiring_)});
            const ::std::array<::hgraph::WiringPortRef, 1> inputs_{divisor.erased()};
            return ::hgraph::Port<void>{wiring_, ::hgraph::compiled::switch_(wiring_, key_.erased(), inputs_, ::std::move(branches_))}.as<::hgraph::TS<::hgraph::Float>>();
        }
    };

    static ::hgraph::Port<::hgraph::TS<::hgraph::Float>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> bypass, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Float>> divisor)
    {
        return switch_00_::wire_(wiring_, bypass, divisor);
    }
};
}  // namespace n_::bypass_ratio
namespace w_
{
// bypass_ratio(bypass: bool, divisor: f64) -> f64 (graph), module demo.control, line 65
::hgraph::Port<::hgraph::TS<::hgraph::Float>> bypass_ratio(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> bypass, ::hgraph::Port<::hgraph::TS<::hgraph::Float>> divisor)
{
    return ::hgl::demo::control::m_::n_::bypass_ratio::impl_::compose(wiring_, bypass, divisor);
}
}  // namespace w_

namespace n_::switch_input
{
// switch_input(route: str, left: i64, right: i64, fallback: i64) -> i64 (graph), module demo.control, line 78
struct impl_
{
    // The `switch` on line 80 over a connection (CTRL-2, CTRL-13 to CTRL-15): a child
    // graph for each branch, of which hgraph's `switch_` runs the one selected.
    struct switch_00_
    {
        // The branch of the `case` on line 81.
        struct branch_00_
        {
            static constexpr auto name = "demo.control:switch on line 80";
            static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose([[maybe_unused]] ::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> left, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> right, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> fallback, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> selected)
            {
                selected = left;
                return selected;
            }
        };
    
        // The branch of the `case` on line 82.
        struct branch_01_
        {
            static constexpr auto name = "demo.control:switch on line 80";
            static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose([[maybe_unused]] ::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> left, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> right, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> fallback, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> selected)
            {
                selected = right;
                return selected;
            }
        };
    
        // The branch of `default`, which a value no case has selects.
        struct branch_02_
        {
            static constexpr auto name = "demo.control:switch on line 80";
            static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose([[maybe_unused]] ::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> left, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> right, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> fallback, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> selected)
            {
                selected = fallback;
                return selected;
            }
        };
    
        static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> wire_(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Str>> key_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> left, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> right, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> fallback, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> selected)
        {
            ::std::vector<::hgraph::SwitchBranchGraph> branches_;
            branches_.push_back(::hgraph::SwitchBranchGraph{::hgraph::Value{static_cast<::hgraph::Str>(::hgraph::Str{"left"})}, ::hgraph::compile_subgraph<branch_00_>(wiring_)});
            branches_.push_back(::hgraph::SwitchBranchGraph{::hgraph::Value{static_cast<::hgraph::Str>(::hgraph::Str{"right"})}, ::hgraph::compile_subgraph<branch_01_>(wiring_)});
            const ::std::array<::hgraph::WiringPortRef, 4> inputs_{left.erased(), right.erased(), fallback.erased(), selected.erased()};
            return ::hgraph::Port<void>{wiring_, ::hgraph::compiled::switch_(wiring_, key_.erased(), inputs_, ::std::move(branches_), ::hgraph::SwitchBranchGraph{{}, ::hgraph::compile_subgraph<branch_02_>(wiring_)})}.as<::hgraph::TS<::hgraph::Int>>();
        }
    };

    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Str>> route, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> left, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> right, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> fallback)
    {
        [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> selected = fallback;
        selected = switch_00_::wire_(wiring_, route, left, right, fallback, selected);
        return selected;
    }
};
}  // namespace n_::switch_input
namespace w_
{
// switch_input(route: str, left: i64, right: i64, fallback: i64) -> i64 (graph), module demo.control, line 78
::hgraph::Port<::hgraph::TS<::hgraph::Int>> switch_input(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Str>> route, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> left, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> right, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> fallback)
{
    return ::hgl::demo::control::m_::n_::switch_input::impl_::compose(wiring_, route, left, right, fallback);
}
}  // namespace w_

namespace n_::sample_selection
{
// sample_selection(use_left: bool, left: i64, right: i64, probe: bool) -> i64 (graph), module demo.control, line 98
struct impl_
{
    // The `if` on line 99 over a connection (CTRL-2, CTRL-13 to CTRL-15): a child
    // graph for each branch, of which hgraph's `switch_` runs the one selected.
    struct switch_00_
    {
        // The branch selected while the condition is `true`.
        struct branch_00_
        {
            static constexpr auto name = "demo.control:if on line 99";
            static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose([[maybe_unused]] ::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> left, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> right)
            {
                return left;
            }
        };
    
        // The branch selected while the condition is `false`.
        struct branch_01_
        {
            static constexpr auto name = "demo.control:if on line 99";
            static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose([[maybe_unused]] ::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> left, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> right)
            {
                return right;
            }
        };
    
        static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> wire_(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> key_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> left, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> right)
        {
            ::std::vector<::hgraph::SwitchBranchGraph> branches_;
            branches_.push_back(::hgraph::SwitchBranchGraph{::hgraph::Value{static_cast<::hgraph::Bool>(true)}, ::hgraph::compile_subgraph<branch_00_>(wiring_)});
            branches_.push_back(::hgraph::SwitchBranchGraph{::hgraph::Value{static_cast<::hgraph::Bool>(false)}, ::hgraph::compile_subgraph<branch_01_>(wiring_)});
            const ::std::array<::hgraph::WiringPortRef, 2> inputs_{left.erased(), right.erased()};
            return ::hgraph::Port<void>{wiring_, ::hgraph::compiled::switch_(wiring_, key_.erased(), inputs_, ::std::move(branches_))}.as<::hgraph::TS<::hgraph::Int>>();
        }
    };

    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> use_left, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> left, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> right, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe)
    {
        [[maybe_unused]] const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> selected = switch_00_::wire_(wiring_, use_left, left, right);
        return ::hgl::demo::control::m_::w_::sample_on_probe(wiring_, selected, probe);
    }
};
}  // namespace n_::sample_selection
namespace w_
{
// sample_selection(use_left: bool, left: i64, right: i64, probe: bool) -> i64 (graph), module demo.control, line 98
::hgraph::Port<::hgraph::TS<::hgraph::Int>> sample_selection(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> use_left, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> left, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> right, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> probe)
{
    return ::hgl::demo::control::m_::n_::sample_selection::impl_::compose(wiring_, use_left, left, right, probe);
}
}  // namespace w_

namespace n_::guarded
{
// guarded(open: bool, value: i64) -> i64 (graph), module demo.control, line 113
struct impl_
{
    // The `if` on line 114 over a connection (CTRL-2, CTRL-13 to CTRL-15): a child
    // graph for each branch, of which hgraph's `switch_` runs the one selected.
    struct switch_00_
    {
        // The branch selected while the condition is `true`.
        struct branch_00_
        {
            static constexpr auto name = "demo.control:if on line 114";
            static void compose([[maybe_unused]] ::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
            {
                ::hgl::demo::control::m_::w_::overflow_on_large(wiring_, value);
            }
        };
    
        // The branch selected while the condition is `false`.
        struct branch_01_
        {
            static constexpr auto name = "demo.control:if on line 114";
            static void compose([[maybe_unused]] ::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
            {
            }
        };
    
        static void wire_(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> key_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
        {
            ::std::vector<::hgraph::SwitchBranchGraph> branches_;
            branches_.push_back(::hgraph::SwitchBranchGraph{::hgraph::Value{static_cast<::hgraph::Bool>(true)}, ::hgraph::compile_subgraph<branch_00_>(wiring_)});
            branches_.push_back(::hgraph::SwitchBranchGraph{::hgraph::Value{static_cast<::hgraph::Bool>(false)}, ::hgraph::compile_subgraph<branch_01_>(wiring_)});
            const ::std::array<::hgraph::WiringPortRef, 1> inputs_{value.erased()};
            ::hgraph::compiled::switch_sink_(wiring_, key_.erased(), inputs_, ::std::move(branches_));
        }
    };

    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> open, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
    {
        switch_00_::wire_(wiring_, open, value);
        return value;
    }
};
}  // namespace n_::guarded
namespace w_
{
// guarded(open: bool, value: i64) -> i64 (graph), module demo.control, line 113
::hgraph::Port<::hgraph::TS<::hgraph::Int>> guarded(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> open, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgl::demo::control::m_::n_::guarded::impl_::compose(wiring_, open, value);
}
}  // namespace w_

namespace n_::three_way
{
// three_way(a: bool, b: bool, x: i64, y: i64, z: i64) -> i64 (graph), module demo.control, line 126
struct impl_
{
    // The `if` on line 127 over a connection (CTRL-2, CTRL-13 to CTRL-15): a child
    // graph for each branch, of which hgraph's `switch_` runs the one selected.
    struct switch_01_
    {
        // The branch selected while the condition is `true`.
        struct branch_00_
        {
            static constexpr auto name = "demo.control:if on line 127";
            static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose([[maybe_unused]] ::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> y, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> z)
            {
                return y;
            }
        };
    
        // The branch selected while the condition is `false`.
        struct branch_01_
        {
            static constexpr auto name = "demo.control:if on line 127";
            static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose([[maybe_unused]] ::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> y, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> z)
            {
                return z;
            }
        };
    
        static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> wire_(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> key_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> y, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> z)
        {
            ::std::vector<::hgraph::SwitchBranchGraph> branches_;
            branches_.push_back(::hgraph::SwitchBranchGraph{::hgraph::Value{static_cast<::hgraph::Bool>(true)}, ::hgraph::compile_subgraph<branch_00_>(wiring_)});
            branches_.push_back(::hgraph::SwitchBranchGraph{::hgraph::Value{static_cast<::hgraph::Bool>(false)}, ::hgraph::compile_subgraph<branch_01_>(wiring_)});
            const ::std::array<::hgraph::WiringPortRef, 2> inputs_{y.erased(), z.erased()};
            return ::hgraph::Port<void>{wiring_, ::hgraph::compiled::switch_(wiring_, key_.erased(), inputs_, ::std::move(branches_))}.as<::hgraph::TS<::hgraph::Int>>();
        }
    };

    // The `if` on line 127 over a connection (CTRL-2, CTRL-13 to CTRL-15): a child
    // graph for each branch, of which hgraph's `switch_` runs the one selected.
    struct switch_00_
    {
        // The branch selected while the condition is `true`.
        struct branch_00_
        {
            static constexpr auto name = "demo.control:if on line 127";
            static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose([[maybe_unused]] ::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> b, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> y, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> z)
            {
                return x;
            }
        };
    
        // The branch selected while the condition is `false`.
        struct branch_01_
        {
            static constexpr auto name = "demo.control:if on line 127";
            static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose([[maybe_unused]] ::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> b, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> y, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> z)
            {
                return switch_01_::wire_(wiring_, b, y, z);
            }
        };
    
        static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> wire_(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> key_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> b, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> y, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> z)
        {
            ::std::vector<::hgraph::SwitchBranchGraph> branches_;
            branches_.push_back(::hgraph::SwitchBranchGraph{::hgraph::Value{static_cast<::hgraph::Bool>(true)}, ::hgraph::compile_subgraph<branch_00_>(wiring_)});
            branches_.push_back(::hgraph::SwitchBranchGraph{::hgraph::Value{static_cast<::hgraph::Bool>(false)}, ::hgraph::compile_subgraph<branch_01_>(wiring_)});
            const ::std::array<::hgraph::WiringPortRef, 4> inputs_{b.erased(), x.erased(), y.erased(), z.erased()};
            return ::hgraph::Port<void>{wiring_, ::hgraph::compiled::switch_(wiring_, key_.erased(), inputs_, ::std::move(branches_))}.as<::hgraph::TS<::hgraph::Int>>();
        }
    };

    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> a, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> b, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> y, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> z)
    {
        return switch_00_::wire_(wiring_, a, b, x, y, z);
    }
};
}  // namespace n_::three_way
namespace w_
{
// three_way(a: bool, b: bool, x: i64, y: i64, z: i64) -> i64 (graph), module demo.control, line 126
::hgraph::Port<::hgraph::TS<::hgraph::Int>> three_way(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> a, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> b, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> y, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> z)
{
    return ::hgl::demo::control::m_::n_::three_way::impl_::compose(wiring_, a, b, x, y, z);
}
}  // namespace w_

namespace n_::scaled
{
// scaled(open: bool, value: i64, const factor: i64) -> i64 (graph), module demo.control, line 140
struct impl_
{
    // The call of `mul_` on line 142, lifted (FN-6).
    struct lifted_01_
    {
        static constexpr auto name = "demo.control:mul_";
        static void eval([[maybe_unused]] ::hgraph::In<"lhs", ::hgraph::TS<::hgraph::Int>> lhs, [[maybe_unused]] ::hgraph::Scalar<"rhs", ::hgraph::Int> rhs, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
        {
            out_.set(::hglrt::mul__i64_i64(::hglrt::checked_value(lhs), rhs.value()));
            return;
        }
    };

    // The call of `add_` on line 142, lifted (FN-6).
    struct lifted_02_
    {
        static constexpr auto name = "demo.control:add_";
        static void eval([[maybe_unused]] ::hgraph::In<"lhs", ::hgraph::TS<::hgraph::Int>> lhs, [[maybe_unused]] ::hgraph::Scalar<"rhs", ::hgraph::Int> rhs, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
        {
            out_.set(::hglrt::add__i64_i64(::hglrt::checked_value(lhs), rhs.value()));
            return;
        }
    };

    // The `if` on line 142 over a connection (CTRL-2, CTRL-13 to CTRL-15): a child
    // graph for each branch, of which hgraph's `switch_` runs the one selected.
    struct switch_00_
    {
        // The branch selected while the condition is `true`.
        struct branch_00_
        {
            static constexpr auto name = "demo.control:if on line 142";
            static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose([[maybe_unused]] ::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::Scalar<"factor", ::hgraph::Int> factor, [[maybe_unused]] ::hgraph::Scalar<"offset", ::hgraph::Int> offset)
            {
                return ::hgraph::wire<lifted_02_>(wiring_, ::hgraph::wire<lifted_01_>(wiring_, value, static_cast<::hgraph::Int>(factor.value())), static_cast<::hgraph::Int>(offset.value()));
            }
        };
    
        // The branch selected while the condition is `false`.
        struct branch_01_
        {
            static constexpr auto name = "demo.control:if on line 142";
            static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose([[maybe_unused]] ::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::Scalar<"factor", ::hgraph::Int> factor, [[maybe_unused]] ::hgraph::Scalar<"offset", ::hgraph::Int> offset)
            {
                return ::hglrt::never<::hgraph::TS<::hgraph::Int>>(wiring_);
            }
        };
    
        static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> wire_(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> key_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::Int factor, [[maybe_unused]] ::hgraph::Int offset)
        {
            ::std::vector<::hgraph::SwitchBranchGraph> branches_;
            branches_.push_back(::hgraph::SwitchBranchGraph{::hgraph::Value{static_cast<::hgraph::Bool>(true)}, ::hgraph::compile_subgraph<branch_00_>(wiring_, factor, offset)});
            branches_.push_back(::hgraph::SwitchBranchGraph{::hgraph::Value{static_cast<::hgraph::Bool>(false)}, ::hgraph::compile_subgraph<branch_01_>(wiring_, factor, offset)});
            const ::std::array<::hgraph::WiringPortRef, 1> inputs_{value.erased()};
            return ::hgraph::Port<void>{wiring_, ::hgraph::compiled::switch_(wiring_, key_.erased(), inputs_, ::std::move(branches_))}.as<::hgraph::TS<::hgraph::Int>>();
        }
    };

    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> open, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::Int factor)
    {
        [[maybe_unused]] const ::hgraph::Int offset = ::hglrt::add__i64_i64(factor, 1);
        return switch_00_::wire_(wiring_, open, value, factor, offset);
    }
};
}  // namespace n_::scaled
namespace w_
{
// scaled(open: bool, value: i64, const factor: i64) -> i64 (graph), module demo.control, line 140
::hgraph::Port<::hgraph::TS<::hgraph::Int>> scaled(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> open, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value, ::hgraph::Int factor)
{
    return ::hgl::demo::control::m_::n_::scaled::impl_::compose(wiring_, open, value, factor);
}
}  // namespace w_

namespace n_::adjusted
{
// adjusted(rising: bool, price: i64) -> i64 (graph), module demo.control, line 152
struct impl_
{
    // The `if` on line 154 over a connection (CTRL-2, CTRL-13 to CTRL-15): a child
    // graph for each branch, of which hgraph's `switch_` runs the one selected.
    struct switch_00_
    {
        // The branch selected while the condition is `true`.
        struct branch_00_
        {
            static constexpr auto name = "demo.control:if on line 154";
            static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose([[maybe_unused]] ::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> price, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> chosen)
            {
                chosen = ::hgl::demo::control::m_::w_::count_publications(wiring_, price);
                return chosen;
            }
        };
    
        // The branch selected while the condition is `false`.
        struct branch_01_
        {
            static constexpr auto name = "demo.control:if on line 154";
            static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose([[maybe_unused]] ::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> price, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> chosen)
            {
                return chosen;
            }
        };
    
        static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> wire_(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> key_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> price, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> chosen)
        {
            ::std::vector<::hgraph::SwitchBranchGraph> branches_;
            branches_.push_back(::hgraph::SwitchBranchGraph{::hgraph::Value{static_cast<::hgraph::Bool>(true)}, ::hgraph::compile_subgraph<branch_00_>(wiring_)});
            branches_.push_back(::hgraph::SwitchBranchGraph{::hgraph::Value{static_cast<::hgraph::Bool>(false)}, ::hgraph::compile_subgraph<branch_01_>(wiring_)});
            const ::std::array<::hgraph::WiringPortRef, 2> inputs_{price.erased(), chosen.erased()};
            return ::hgraph::Port<void>{wiring_, ::hgraph::compiled::switch_(wiring_, key_.erased(), inputs_, ::std::move(branches_))}.as<::hgraph::TS<::hgraph::Int>>();
        }
    };

    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> rising, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> price)
    {
        [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> chosen = price;
        chosen = switch_00_::wire_(wiring_, rising, price, chosen);
        return chosen;
    }
};
}  // namespace n_::adjusted
namespace w_
{
// adjusted(rising: bool, price: i64) -> i64 (graph), module demo.control, line 152
::hgraph::Port<::hgraph::TS<::hgraph::Int>> adjusted(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> rising, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> price)
{
    return ::hgl::demo::control::m_::n_::adjusted::impl_::compose(wiring_, rising, price);
}
}  // namespace w_

namespace n_::either
{
// either(rising: bool, price: i64) -> i64 (graph), module demo.control, line 165
struct impl_
{
    // The call of `add_` on line 167, lifted (FN-6).
    struct lifted_01_
    {
        static constexpr auto name = "demo.control:add_";
        static void eval([[maybe_unused]] ::hgraph::In<"lhs", ::hgraph::TS<::hgraph::Int>> lhs, [[maybe_unused]] ::hgraph::Scalar<"rhs", ::hgraph::Int> rhs, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
        {
            out_.set(::hglrt::add__i64_i64(::hglrt::checked_value(lhs), rhs.value()));
            return;
        }
    };

    // The call of `sub_` on line 167, lifted (FN-6).
    struct lifted_02_
    {
        static constexpr auto name = "demo.control:sub_";
        static void eval([[maybe_unused]] ::hgraph::In<"lhs", ::hgraph::TS<::hgraph::Int>> lhs, [[maybe_unused]] ::hgraph::Scalar<"rhs", ::hgraph::Int> rhs, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
        {
            out_.set(::hglrt::sub__i64_i64(::hglrt::checked_value(lhs), rhs.value()));
            return;
        }
    };

    // The `if` on line 167 over a connection (CTRL-2, CTRL-13 to CTRL-15): a child
    // graph for each branch, of which hgraph's `switch_` runs the one selected.
    struct switch_00_
    {
        // The branch selected while the condition is `true`.
        struct branch_00_
        {
            static constexpr auto name = "demo.control:if on line 167";
            static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose([[maybe_unused]] ::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> price)
            {
                [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> chosen{};
                chosen = ::hgraph::wire<lifted_01_>(wiring_, price, static_cast<::hgraph::Int>(1));
                return chosen;
            }
        };
    
        // The branch selected while the condition is `false`.
        struct branch_01_
        {
            static constexpr auto name = "demo.control:if on line 167";
            static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose([[maybe_unused]] ::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> price)
            {
                [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> chosen{};
                chosen = ::hgraph::wire<lifted_02_>(wiring_, price, static_cast<::hgraph::Int>(1));
                return chosen;
            }
        };
    
        static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> wire_(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> key_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> price)
        {
            ::std::vector<::hgraph::SwitchBranchGraph> branches_;
            branches_.push_back(::hgraph::SwitchBranchGraph{::hgraph::Value{static_cast<::hgraph::Bool>(true)}, ::hgraph::compile_subgraph<branch_00_>(wiring_)});
            branches_.push_back(::hgraph::SwitchBranchGraph{::hgraph::Value{static_cast<::hgraph::Bool>(false)}, ::hgraph::compile_subgraph<branch_01_>(wiring_)});
            const ::std::array<::hgraph::WiringPortRef, 1> inputs_{price.erased()};
            return ::hgraph::Port<void>{wiring_, ::hgraph::compiled::switch_(wiring_, key_.erased(), inputs_, ::std::move(branches_))}.as<::hgraph::TS<::hgraph::Int>>();
        }
    };

    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> rising, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> price)
    {
        [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> chosen{};
        chosen = switch_00_::wire_(wiring_, rising, price);
        return chosen;
    }
};
}  // namespace n_::either
namespace w_
{
// either(rising: bool, price: i64) -> i64 (graph), module demo.control, line 165
::hgraph::Port<::hgraph::TS<::hgraph::Int>> either(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> rising, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> price)
{
    return ::hgl::demo::control::m_::n_::either::impl_::compose(wiring_, rising, price);
}
}  // namespace w_

namespace n_::routed
{
// routed(route: i64, value: i64) -> i64 (graph), module demo.control, line 176
struct impl_
{
    // The call of `add_` on line 182, lifted (FN-6).
    struct lifted_01_
    {
        static constexpr auto name = "demo.control:add_";
        static void eval([[maybe_unused]] ::hgraph::In<"lhs", ::hgraph::TS<::hgraph::Int>> lhs, [[maybe_unused]] ::hgraph::Scalar<"rhs", ::hgraph::Int> rhs, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
        {
            out_.set(::hglrt::add__i64_i64(::hglrt::checked_value(lhs), rhs.value()));
            return;
        }
    };

    // The `switch` on line 177 over a connection (CTRL-2, CTRL-13 to CTRL-15): a child
    // graph for each branch, of which hgraph's `switch_` runs the one selected.
    struct switch_00_
    {
        // The branch of the `case` on line 178.
        struct branch_00_
        {
            static constexpr auto name = "demo.control:switch on line 177";
            static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose([[maybe_unused]] ::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
            {
                return value;
            }
        };
    
        // The branch of the `case` on line 179.
        struct branch_01_
        {
            static constexpr auto name = "demo.control:switch on line 177";
            static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose([[maybe_unused]] ::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
            {
                return ::hgl::demo::control::m_::w_::count_publications(wiring_, value);
            }
        };
    
        // The branch of `default`, which a value no case has selects.
        struct branch_02_
        {
            static constexpr auto name = "demo.control:switch on line 177";
            static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose([[maybe_unused]] ::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
            {
                {
                }
                return ::hgraph::wire<lifted_01_>(wiring_, value, static_cast<::hgraph::Int>(100));
            }
        };
    
        static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> wire_(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> key_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
        {
            ::std::vector<::hgraph::SwitchBranchGraph> branches_;
            branches_.push_back(::hgraph::SwitchBranchGraph{::hgraph::Value{static_cast<::hgraph::Int>(1)}, ::hgraph::compile_subgraph<branch_00_>(wiring_)});
            branches_.push_back(::hgraph::SwitchBranchGraph{::hgraph::Value{static_cast<::hgraph::Int>(2)}, ::hgraph::compile_subgraph<branch_01_>(wiring_)});
            const ::std::array<::hgraph::WiringPortRef, 1> inputs_{value.erased()};
            return ::hgraph::Port<void>{wiring_, ::hgraph::compiled::switch_(wiring_, key_.erased(), inputs_, ::std::move(branches_), ::hgraph::SwitchBranchGraph{{}, ::hgraph::compile_subgraph<branch_02_>(wiring_)})}.as<::hgraph::TS<::hgraph::Int>>();
        }
    };

    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> route, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
    {
        return switch_00_::wire_(wiring_, route, value);
    }
};
}  // namespace n_::routed
namespace w_
{
// routed(route: i64, value: i64) -> i64 (graph), module demo.control, line 176
::hgraph::Port<::hgraph::TS<::hgraph::Int>> routed(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> route, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgl::demo::control::m_::n_::routed::impl_::compose(wiring_, route, value);
}
}  // namespace w_

namespace n_::large
{
// large(value: i64) -> i64 (graph), module demo.control, line 190
struct impl_
{
    // The `if` on line 191 over a connection (CTRL-2, CTRL-13 to CTRL-15): a child
    // graph for each branch, of which hgraph's `switch_` runs the one selected.
    struct switch_00_
    {
        // The branch selected while the condition is `true`.
        struct branch_00_
        {
            static constexpr auto name = "demo.control:if on line 191";
            static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose([[maybe_unused]] ::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
            {
                return ::hgl::demo::control::m_::w_::count_publications(wiring_, value);
            }
        };
    
        // The branch selected while the condition is `false`.
        struct branch_01_
        {
            static constexpr auto name = "demo.control:if on line 191";
            static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose([[maybe_unused]] ::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
            {
                return value;
            }
        };
    
        static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> wire_(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> key_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
        {
            ::std::vector<::hgraph::SwitchBranchGraph> branches_;
            branches_.push_back(::hgraph::SwitchBranchGraph{::hgraph::Value{static_cast<::hgraph::Bool>(true)}, ::hgraph::compile_subgraph<branch_00_>(wiring_)});
            branches_.push_back(::hgraph::SwitchBranchGraph{::hgraph::Value{static_cast<::hgraph::Bool>(false)}, ::hgraph::compile_subgraph<branch_01_>(wiring_)});
            const ::std::array<::hgraph::WiringPortRef, 1> inputs_{value.erased()};
            return ::hgraph::Port<void>{wiring_, ::hgraph::compiled::switch_(wiring_, key_.erased(), inputs_, ::std::move(branches_))}.as<::hgraph::TS<::hgraph::Int>>();
        }
    };

    // The call of `gt_` on line 191, lifted (FN-6).
    struct lifted_01_
    {
        static constexpr auto name = "demo.control:gt_";
        static void eval([[maybe_unused]] ::hgraph::In<"lhs", ::hgraph::TS<::hgraph::Int>> lhs, [[maybe_unused]] ::hgraph::Scalar<"rhs", ::hgraph::Int> rhs, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Bool>> out_)
        {
            out_.set(::hglrt::gt__T_T<::hgraph::Int>(::hglrt::checked_value(lhs), rhs.value()));
            return;
        }
    };

    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
    {
        return switch_00_::wire_(wiring_, ::hgraph::wire<lifted_01_>(wiring_, value, static_cast<::hgraph::Int>(10)), value);
    }
};
}  // namespace n_::large
namespace w_
{
// large(value: i64) -> i64 (graph), module demo.control, line 190
::hgraph::Port<::hgraph::TS<::hgraph::Int>> large(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgl::demo::control::m_::n_::large::impl_::compose(wiring_, value);
}
}  // namespace w_

namespace n_::chosen_by_config
{
// chosen_by_config(const wide: bool, open: bool, value: i64) -> i64 (graph), module demo.control, line 200
struct impl_
{
    // The call of `add_` on line 204, lifted (FN-6).
    struct lifted_01_
    {
        static constexpr auto name = "demo.control:add_";
        static void eval([[maybe_unused]] ::hgraph::In<"lhs", ::hgraph::TS<::hgraph::Int>> lhs, [[maybe_unused]] ::hgraph::Scalar<"rhs", ::hgraph::Int> rhs, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
        {
            out_.set(::hglrt::add__i64_i64(::hglrt::checked_value(lhs), rhs.value()));
            return;
        }
    };

    // The `if` on line 202 over a connection (CTRL-2, CTRL-13 to CTRL-15): a child
    // graph for each branch, of which hgraph's `switch_` runs the one selected.
    struct switch_00_
    {
        // The branch selected while the condition is `true`.
        struct branch_00_
        {
            static constexpr auto name = "demo.control:if on line 202";
            static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose([[maybe_unused]] ::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
            {
                return value;
            }
        };
    
        // The branch selected while the condition is `false`.
        struct branch_01_
        {
            static constexpr auto name = "demo.control:if on line 202";
            static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose([[maybe_unused]] ::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
            {
                return ::hgraph::wire<lifted_01_>(wiring_, value, static_cast<::hgraph::Int>(1));
            }
        };
    
        static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> wire_(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> key_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
        {
            ::std::vector<::hgraph::SwitchBranchGraph> branches_;
            branches_.push_back(::hgraph::SwitchBranchGraph{::hgraph::Value{static_cast<::hgraph::Bool>(true)}, ::hgraph::compile_subgraph<branch_00_>(wiring_)});
            branches_.push_back(::hgraph::SwitchBranchGraph{::hgraph::Value{static_cast<::hgraph::Bool>(false)}, ::hgraph::compile_subgraph<branch_01_>(wiring_)});
            const ::std::array<::hgraph::WiringPortRef, 1> inputs_{value.erased()};
            return ::hgraph::Port<void>{wiring_, ::hgraph::compiled::switch_(wiring_, key_.erased(), inputs_, ::std::move(branches_))}.as<::hgraph::TS<::hgraph::Int>>();
        }
    };

    // The call of `add_` on line 204, lifted (FN-6).
    struct lifted_02_
    {
        static constexpr auto name = "demo.control:add_";
        static void eval([[maybe_unused]] ::hgraph::In<"lhs", ::hgraph::TS<::hgraph::Int>> lhs, [[maybe_unused]] ::hgraph::Scalar<"rhs", ::hgraph::Int> rhs, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
        {
            out_.set(::hglrt::add__i64_i64(::hglrt::checked_value(lhs), rhs.value()));
            return;
        }
    };

    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Bool wide, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> open, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
    {
        if (wide)
        {
            return switch_00_::wire_(wiring_, open, value);
        }
        else
        {
            return ::hgraph::wire<lifted_02_>(wiring_, value, static_cast<::hgraph::Int>(1));
        }
    }
};
}  // namespace n_::chosen_by_config
namespace w_
{
// chosen_by_config(const wide: bool, open: bool, value: i64) -> i64 (graph), module demo.control, line 200
::hgraph::Port<::hgraph::TS<::hgraph::Int>> chosen_by_config(::hgraph::Wiring& wiring_, ::hgraph::Bool wide, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> open, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgl::demo::control::m_::n_::chosen_by_config::impl_::compose(wiring_, wide, open, value);
}
}  // namespace w_

namespace n_::early
{
// early(open: bool, value: i64) -> i64 (graph), module demo.control, line 216
struct impl_
{
    // The call of `add_` on line 217, lifted (FN-6).
    struct lifted_01_
    {
        static constexpr auto name = "demo.control:add_";
        static void eval([[maybe_unused]] ::hgraph::In<"lhs", ::hgraph::TS<::hgraph::Int>> lhs, [[maybe_unused]] ::hgraph::Scalar<"rhs", ::hgraph::Int> rhs, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
        {
            out_.set(::hglrt::add__i64_i64(::hglrt::checked_value(lhs), rhs.value()));
            return;
        }
    };

    // The call of `mul_` on line 218, lifted (FN-6).
    struct lifted_02_
    {
        static constexpr auto name = "demo.control:mul_";
        static void eval([[maybe_unused]] ::hgraph::In<"lhs", ::hgraph::TS<::hgraph::Int>> lhs, [[maybe_unused]] ::hgraph::Scalar<"rhs", ::hgraph::Int> rhs, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
        {
            out_.set(::hglrt::mul__i64_i64(::hglrt::checked_value(lhs), rhs.value()));
            return;
        }
    };

    // The `if` on line 217 over a connection (CTRL-2, CTRL-13 to CTRL-15): a child
    // graph for each branch, of which hgraph's `switch_` runs the one selected.
    struct switch_00_
    {
        // The branch selected while the condition is `true`.
        struct branch_00_
        {
            static constexpr auto name = "demo.control:if on line 217";
            static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose([[maybe_unused]] ::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
            {
                return value;
            }
        };
    
        // The branch selected while the condition is `false`.
        struct branch_01_
        {
            static constexpr auto name = "demo.control:if on line 217";
            static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose([[maybe_unused]] ::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
            {
                [[maybe_unused]] const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> v = ::hgraph::wire<lifted_01_>(wiring_, value, static_cast<::hgraph::Int>(5));
                return ::hgraph::wire<lifted_02_>(wiring_, v, static_cast<::hgraph::Int>(2));
            }
        };
    
        static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> wire_(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> key_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
        {
            ::std::vector<::hgraph::SwitchBranchGraph> branches_;
            branches_.push_back(::hgraph::SwitchBranchGraph{::hgraph::Value{static_cast<::hgraph::Bool>(true)}, ::hgraph::compile_subgraph<branch_00_>(wiring_)});
            branches_.push_back(::hgraph::SwitchBranchGraph{::hgraph::Value{static_cast<::hgraph::Bool>(false)}, ::hgraph::compile_subgraph<branch_01_>(wiring_)});
            const ::std::array<::hgraph::WiringPortRef, 1> inputs_{value.erased()};
            return ::hgraph::Port<void>{wiring_, ::hgraph::compiled::switch_(wiring_, key_.erased(), inputs_, ::std::move(branches_))}.as<::hgraph::TS<::hgraph::Int>>();
        }
    };

    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> open, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
    {
        return switch_00_::wire_(wiring_, open, value);
    }
};
}  // namespace n_::early
namespace w_
{
// early(open: bool, value: i64) -> i64 (graph), module demo.control, line 216
::hgraph::Port<::hgraph::TS<::hgraph::Int>> early(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> open, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgl::demo::control::m_::n_::early::impl_::compose(wiring_, open, value);
}
}  // namespace w_

namespace n_::unmatched
{
// unmatched(route: str, value: i64) -> i64 (graph), module demo.control, line 226
struct impl_
{
    // The call of `add_` on line 229, lifted (FN-6).
    struct lifted_01_
    {
        static constexpr auto name = "demo.control:add_";
        static void eval([[maybe_unused]] ::hgraph::In<"lhs", ::hgraph::TS<::hgraph::Int>> lhs, [[maybe_unused]] ::hgraph::Scalar<"rhs", ::hgraph::Int> rhs, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
        {
            out_.set(::hglrt::add__i64_i64(::hglrt::checked_value(lhs), rhs.value()));
            return;
        }
    };

    // The call of `add_` on line 230, lifted (FN-6).
    struct lifted_02_
    {
        static constexpr auto name = "demo.control:add_";
        static void eval([[maybe_unused]] ::hgraph::In<"lhs", ::hgraph::TS<::hgraph::Int>> lhs, [[maybe_unused]] ::hgraph::Scalar<"rhs", ::hgraph::Int> rhs, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
        {
            out_.set(::hglrt::add__i64_i64(::hglrt::checked_value(lhs), rhs.value()));
            return;
        }
    };

    // The `switch` on line 228 over a connection (CTRL-2, CTRL-13 to CTRL-15): a child
    // graph for each branch, of which hgraph's `switch_` runs the one selected.
    struct switch_00_
    {
        // The branch of the `case` on line 229.
        struct branch_00_
        {
            static constexpr auto name = "demo.control:switch on line 228";
            static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose([[maybe_unused]] ::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> out)
            {
                out = ::hgraph::wire<lifted_01_>(wiring_, value, static_cast<::hgraph::Int>(1));
                return out;
            }
        };
    
        // The branch of the `case` on line 230.
        struct branch_01_
        {
            static constexpr auto name = "demo.control:switch on line 228";
            static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose([[maybe_unused]] ::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> out)
            {
                out = ::hgraph::wire<lifted_02_>(wiring_, value, static_cast<::hgraph::Int>(2));
                return out;
            }
        };
    
        // The branch a value no case has selects: it fails (CTRL-6).
        struct branch_02_
        {
            static constexpr auto name = "demo.control:switch on line 228";
            static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose([[maybe_unused]] ::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Str>> key_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> out)
            {
                return ::hglrt::unmatched<::hgraph::Str, ::hgraph::TS<::hgraph::Int>>(wiring_, key_);
            }
        };
    
        static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> wire_(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Str>> key_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> out)
        {
            ::std::vector<::hgraph::SwitchBranchGraph> branches_;
            branches_.push_back(::hgraph::SwitchBranchGraph{::hgraph::Value{static_cast<::hgraph::Str>(::hgraph::Str{"a"})}, ::hgraph::compile_subgraph<branch_00_>(wiring_)});
            branches_.push_back(::hgraph::SwitchBranchGraph{::hgraph::Value{static_cast<::hgraph::Str>(::hgraph::Str{"b"})}, ::hgraph::compile_subgraph<branch_01_>(wiring_)});
            const ::std::array<::hgraph::WiringPortRef, 2> inputs_{value.erased(), out.erased()};
            return ::hgraph::Port<void>{wiring_, ::hgraph::compiled::switch_(wiring_, key_.erased(), inputs_, ::std::move(branches_), ::hgraph::SwitchBranchGraph{{}, ::hgraph::compile_subgraph<branch_02_>(wiring_), true})}.as<::hgraph::TS<::hgraph::Int>>();
        }
    };

    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Str>> route, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
    {
        [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> out = value;
        out = switch_00_::wire_(wiring_, route, value, out);
        return out;
    }
};
}  // namespace n_::unmatched
namespace w_
{
// unmatched(route: str, value: i64) -> i64 (graph), module demo.control, line 226
::hgraph::Port<::hgraph::TS<::hgraph::Int>> unmatched(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Str>> route, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgl::demo::control::m_::n_::unmatched::impl_::compose(wiring_, route, value);
}
}  // namespace w_

namespace n_::gated_product
{
// gated_product(open: bool, price: i64, const factors: list<i64>) -> i64 (graph), module demo.control, line 241
struct impl_
{
    // The call of `mul_` on line 244, lifted (FN-6).
    struct lifted_01_
    {
        static constexpr auto name = "demo.control:mul_";
        static void eval([[maybe_unused]] ::hgraph::In<"lhs", ::hgraph::TS<::hgraph::Int>> lhs, [[maybe_unused]] ::hgraph::Scalar<"rhs", ::hgraph::Int> rhs, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
        {
            out_.set(::hglrt::mul__i64_i64(::hglrt::checked_value(lhs), rhs.value()));
            return;
        }
    };

    // The `if` on line 244 over a connection (CTRL-2, CTRL-13 to CTRL-15): a child
    // graph for each branch, of which hgraph's `switch_` runs the one selected.
    struct switch_00_
    {
        // The branch selected while the condition is `true`.
        struct branch_00_
        {
            static constexpr auto name = "demo.control:if on line 244";
            static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose([[maybe_unused]] ::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> running, [[maybe_unused]] ::hgraph::Scalar<"factor", ::hgraph::Int> factor)
            {
                running = ::hgraph::wire<lifted_01_>(wiring_, running, static_cast<::hgraph::Int>(factor.value()));
                return running;
            }
        };
    
        // The branch selected while the condition is `false`.
        struct branch_01_
        {
            static constexpr auto name = "demo.control:if on line 244";
            static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose([[maybe_unused]] ::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> running, [[maybe_unused]] ::hgraph::Scalar<"factor", ::hgraph::Int> factor)
            {
                return running;
            }
        };
    
        static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> wire_(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> key_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> running, [[maybe_unused]] ::hgraph::Int factor)
        {
            ::std::vector<::hgraph::SwitchBranchGraph> branches_;
            branches_.push_back(::hgraph::SwitchBranchGraph{::hgraph::Value{static_cast<::hgraph::Bool>(true)}, ::hgraph::compile_subgraph<branch_00_>(wiring_, factor)});
            branches_.push_back(::hgraph::SwitchBranchGraph{::hgraph::Value{static_cast<::hgraph::Bool>(false)}, ::hgraph::compile_subgraph<branch_01_>(wiring_, factor)});
            const ::std::array<::hgraph::WiringPortRef, 1> inputs_{running.erased()};
            return ::hgraph::Port<void>{wiring_, ::hgraph::compiled::switch_(wiring_, key_.erased(), inputs_, ::std::move(branches_))}.as<::hgraph::TS<::hgraph::Int>>();
        }
    };

    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> open, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> price, [[maybe_unused]] const ::hglrt::list<::hgraph::Int>& factors)
    {
        [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> running = price;
        for (const ::hgraph::Int& factor : factors)
        {
            running = switch_00_::wire_(wiring_, open, running, factor);
        }
        return running;
    }
};
}  // namespace n_::gated_product
namespace w_
{
// gated_product(open: bool, price: i64, const factors: list<i64>) -> i64 (graph), module demo.control, line 241
::hgraph::Port<::hgraph::TS<::hgraph::Int>> gated_product(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> open, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> price, const ::hglrt::list<::hgraph::Int>& factors)
{
    return ::hgl::demo::control::m_::n_::gated_product::impl_::compose(wiring_, open, price, factors);
}
}  // namespace w_

namespace n_::sized
{
// sized(price: i64, const wide: bool) -> i64 (graph), module demo.control, line 256
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> price, [[maybe_unused]] ::hgraph::Bool wide)
    {
        return [&]() -> ::hgraph::Port<::hgraph::TS<::hgraph::Int>> {
    if (wide)
    {
        return ::hgl::demo::control::m_::w_::count_publications(wiring_, price);
    }
    else
    {
        return price;
    }
}();
    }
};
}  // namespace n_::sized
namespace w_
{
// sized(price: i64, const wide: bool) -> i64 (graph), module demo.control, line 256
::hgraph::Port<::hgraph::TS<::hgraph::Int>> sized(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> price, ::hgraph::Bool wide)
{
    return ::hgl::demo::control::m_::n_::sized::impl_::compose(wiring_, price, wide);
}
}  // namespace w_

namespace n_::gated_by_config
{
// gated_by_config(price: i64, const open: bool) -> i64 (graph), module demo.control, line 260
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> price, [[maybe_unused]] ::hgraph::Bool open)
    {
        [[maybe_unused]] const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> chosen = [&]() -> ::hgraph::Port<::hgraph::TS<::hgraph::Int>> {
    if (open)
    {
        return price;
    }
    else
    {
        return ::hglrt::never<::hgraph::TS<::hgraph::Int>>(wiring_);
    }
}();
        return chosen;
    }
};
}  // namespace n_::gated_by_config
namespace w_
{
// gated_by_config(price: i64, const open: bool) -> i64 (graph), module demo.control, line 260
::hgraph::Port<::hgraph::TS<::hgraph::Int>> gated_by_config(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> price, ::hgraph::Bool open)
{
    return ::hgl::demo::control::m_::n_::gated_by_config::impl_::compose(wiring_, price, open);
}
}  // namespace w_

namespace n_::mixed
{
// mixed(price: i64, const wide: bool) -> i64 (graph), module demo.control, line 265
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> price, [[maybe_unused]] ::hgraph::Bool wide)
    {
        return [&]() -> ::hgraph::Port<::hgraph::TS<::hgraph::Int>> {
    if (wide)
    {
        [[maybe_unused]] const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> counted = ::hgl::demo::control::m_::w_::count_publications(wiring_, price);
        return counted;
    }
    else
    {
        return ::hglrt::constant<::hgraph::Int>(wiring_, 5);
    }
}();
    }
};
}  // namespace n_::mixed
namespace w_
{
// mixed(price: i64, const wide: bool) -> i64 (graph), module demo.control, line 265
::hgraph::Port<::hgraph::TS<::hgraph::Int>> mixed(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> price, ::hgraph::Bool wide)
{
    return ::hgl::demo::control::m_::n_::mixed::impl_::compose(wiring_, price, wide);
}
}  // namespace w_

}  // namespace hgl::demo::control::m_

namespace hgl::demo::control::m_::t_
{

void a_conditional_child_restarts_only_after_leaving_its_branch()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_06_ = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Bool>{true, true, false, ::std::nullopt, true, ::std::nullopt};
        const auto trace_02_ = ::hglrt::testing::Dense<::hgraph::Int>{10, 20, 30, 40, 50, 60};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_02_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_04_ = input_01_; const auto argument_05_ = input_03_; return ::hgl::demo::control::m_::w_::count_inside_branch(wiring_, argument_04_, argument_05_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_07_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 2, ::std::nullopt, ::std::nullopt, 1, 2};
        ::hglrt::testing::expect_same(left_06_, right_07_);
    }();
}

void a_branch_runs_once_with_the_values_held_when_it_starts()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_06_ = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Bool>{false, true, false, true};
        const auto trace_02_ = ::hglrt::testing::Dense<::hgraph::Int>{10, ::std::nullopt, ::std::nullopt, ::std::nullopt};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_02_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_04_ = input_01_; const auto argument_05_ = input_03_; return ::hgl::demo::control::m_::w_::count_inside_branch(wiring_, argument_04_, argument_05_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_07_ = ::hglrt::testing::Dense<::hgraph::Int>{::std::nullopt, 1, ::std::nullopt, 1};
        ::hglrt::testing::expect_same(left_06_, right_07_);
    }();
}

void an_outer_producer_keeps_its_lifetime_across_branches()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_06_ = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Bool>{true, true, false, ::std::nullopt, true, ::std::nullopt};
        const auto trace_02_ = ::hglrt::testing::Dense<::hgraph::Int>{10, 20, 30, 40, 50, 60};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_02_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_04_ = input_01_; const auto argument_05_ = input_03_; return ::hgl::demo::control::m_::w_::count_outside_branch(wiring_, argument_04_, argument_05_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_07_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 2, ::std::nullopt, ::std::nullopt, 5, 6};
        ::hglrt::testing::expect_same(left_06_, right_07_);
    }();
}

void an_early_return_skips_the_continuation()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_06_ = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Bool>{true, false, true, false};
        const auto trace_02_ = ::hglrt::testing::Dense<::hgraph::Float>{0.0, 3.0, 0.0, 2.0};
        return ::hglrt::testing::evaluate<::hgraph::Float>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Float>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_f64(wiring_, ::hglrt::testing::replay_items(trace_02_, start_));
        ::hgl::core::m_::w_::i_::record::of_f64(wiring_, [&] { const auto argument_04_ = input_01_; const auto argument_05_ = input_03_; return ::hgl::demo::control::m_::w_::bypass_ratio(wiring_, argument_04_, argument_05_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_07_ = ::hglrt::testing::Dense<::hgraph::Float>{0.0, 1.5, 0.0, 1.0};
        ::hglrt::testing::expect_same(left_06_, right_07_);
    }();
}

void a_switch_forwards_the_selected_connection()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_012_ = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Str>{::hgraph::Str{"left"}, ::hgraph::Str{"right"}, ::std::nullopt, ::hgraph::Str{"other"}, ::std::nullopt, ::hgraph::Str{"left"}};
        const auto trace_02_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 2, 3, 4, 5, 6};
        const auto trace_04_ = ::hglrt::testing::Dense<::hgraph::Int>{10, 20, ::std::nullopt, 40, 50, 60};
        const auto trace_06_ = ::hglrt::testing::Dense<::hgraph::Int>{100, 200, 300, 400, 500, 600};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Str>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_str(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_02_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_05_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_04_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_07_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_06_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_08_ = input_01_; const auto argument_09_ = input_03_; const auto argument_010_ = input_05_; const auto argument_011_ = input_07_; return ::hgl::demo::control::m_::w_::switch_input(wiring_, argument_08_, argument_09_, argument_010_, argument_011_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_013_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 20, ::std::nullopt, 400, 500, 6};
        ::hglrt::testing::expect_same(left_012_, right_013_);
    }();
}

void a_probe_reads_a_selected_producer_that_stayed_quiet()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_012_ = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Bool>{true, false, ::std::nullopt, ::std::nullopt, true, ::std::nullopt};
        const auto trace_02_ = ::hglrt::testing::Dense<::hgraph::Int>{7, ::std::nullopt, ::std::nullopt, 9, ::std::nullopt, ::std::nullopt};
        const auto trace_04_ = ::hglrt::testing::Dense<::hgraph::Int>{20, ::std::nullopt, ::std::nullopt, ::std::nullopt, ::std::nullopt, ::std::nullopt};
        const auto trace_06_ = ::hglrt::testing::Dense<::hgraph::Bool>{true, ::std::nullopt, true, ::std::nullopt, ::std::nullopt, true};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_02_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_05_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_04_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_07_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(trace_06_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_08_ = input_01_; const auto argument_09_ = input_03_; const auto argument_010_ = input_05_; const auto argument_011_ = input_07_; return ::hgl::demo::control::m_::w_::sample_selection(wiring_, argument_08_, argument_09_, argument_010_, argument_011_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_013_ = ::hglrt::testing::Dense<::hgraph::Int>{7, ::std::nullopt, 20, ::std::nullopt, ::std::nullopt, 9};
        ::hglrt::testing::expect_same(left_012_, right_013_);
    }();
}

void an_outputless_branch_runs_only_while_selected()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_06_ = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Bool>{false, false, true};
        const auto trace_02_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 5, 0};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_02_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_04_ = input_01_; const auto argument_05_ = input_03_; return ::hgl::demo::control::m_::w_::guarded(wiring_, argument_04_, argument_05_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_07_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 5, 0};
        ::hglrt::testing::expect_same(left_06_, right_07_);
    }();
    ::hglrt::testing::statement();
    ::hglrt::testing::raises("numeric.out_of_range", [&] {
        [&] {
            const auto left_014_ = [&]() {
        const auto trace_08_ = ::hglrt::testing::Dense<::hgraph::Bool>{false, true};
        const auto trace_010_ = ::hglrt::testing::Dense<::hgraph::Int>{5, 5};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_09_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(trace_08_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_011_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_010_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_012_ = input_09_; const auto argument_013_ = input_011_; return ::hgl::demo::control::m_::w_::guarded(wiring_, argument_012_, argument_013_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
            const auto right_015_ = ::hglrt::testing::Dense<::hgraph::Int>{5, 5};
            ::hglrt::testing::expect_same(left_014_, right_015_);
        }();
    });
}

void an_else_if_chain_selects_in_order()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_015_ = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Bool>{true, false, false, true};
        const auto trace_02_ = ::hglrt::testing::Dense<::hgraph::Bool>{true, true, false, ::std::nullopt};
        const auto trace_04_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 2, 3, 4};
        const auto trace_06_ = ::hglrt::testing::Dense<::hgraph::Int>{10, 20, 30, 40};
        const auto trace_08_ = ::hglrt::testing::Dense<::hgraph::Int>{100, 200, 300, 400};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(trace_02_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_05_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_04_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_07_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_06_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_09_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_08_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_010_ = input_01_; const auto argument_011_ = input_03_; const auto argument_012_ = input_05_; const auto argument_013_ = input_07_; const auto argument_014_ = input_09_; return ::hgl::demo::control::m_::w_::three_way(wiring_, argument_010_, argument_011_, argument_012_, argument_013_, argument_014_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_016_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 20, 300, 4};
        ::hglrt::testing::expect_same(left_015_, right_016_);
    }();
}

void a_branch_reads_ordinary_values()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_08_ = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Bool>{true, true, false, true};
        const auto trace_02_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 2, 3, 4};
        const ::hgraph::Int argument_04_ = 3;
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_02_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_05_ = input_01_; const auto argument_06_ = input_03_; const ::hgraph::Int argument_07_ = argument_04_; return ::hgl::demo::control::m_::w_::scaled(wiring_, argument_05_, argument_06_, argument_07_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_09_ = ::hglrt::testing::Dense<::hgraph::Int>{7, 10, ::std::nullopt, 16};
        ::hglrt::testing::expect_same(left_08_, right_09_);
    }();
}

void a_var_keeps_its_binding_where_the_branch_does_not_assign_it()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_06_ = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Bool>{false, true, ::std::nullopt, false, true};
        const auto trace_02_ = ::hglrt::testing::Dense<::hgraph::Int>{10, 20, 30, 40, 50};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_02_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_04_ = input_01_; const auto argument_05_ = input_03_; return ::hgl::demo::control::m_::w_::adjusted(wiring_, argument_04_, argument_05_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_07_ = ::hglrt::testing::Dense<::hgraph::Int>{10, 1, 2, 40, 1};
        ::hglrt::testing::expect_same(left_06_, right_07_);
    }();
}

void a_var_with_no_value_is_given_one_by_each_branch()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_06_ = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Bool>{true, false, ::std::nullopt};
        const auto trace_02_ = ::hglrt::testing::Dense<::hgraph::Int>{10, 20, 30};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_02_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_04_ = input_01_; const auto argument_05_ = input_03_; return ::hgl::demo::control::m_::w_::either(wiring_, argument_04_, argument_05_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_07_ = ::hglrt::testing::Dense<::hgraph::Int>{11, 19, 29};
        ::hglrt::testing::expect_same(left_06_, right_07_);
    }();
}

void a_switch_returns_from_a_case_or_goes_on()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_06_ = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 2, ::std::nullopt, 3, 2};
        const auto trace_02_ = ::hglrt::testing::Dense<::hgraph::Int>{5, 6, 7, 8, 9};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_02_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_04_ = input_01_; const auto argument_05_ = input_03_; return ::hgl::demo::control::m_::w_::routed(wiring_, argument_04_, argument_05_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_07_ = ::hglrt::testing::Dense<::hgraph::Int>{5, 1, 2, 108, 1};
        ::hglrt::testing::expect_same(left_06_, right_07_);
    }();
}

void a_lifted_condition_selects()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 20, 30, 2, 40}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::control::m_::w_::large(wiring_, input_00_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 1, 2, 2, 1};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
}

void a_return_under_a_wiring_time_if_has_its_continuation()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_08_ = [&]() {
        const ::hgraph::Bool argument_00_ = true;
        const auto trace_01_ = ::hglrt::testing::Dense<::hgraph::Bool>{true, false, true};
        const auto trace_03_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 2, 3};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_02_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(trace_01_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_04_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_03_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const ::hgraph::Bool argument_05_ = argument_00_; const auto argument_06_ = input_02_; const auto argument_07_ = input_04_; return ::hgl::demo::control::m_::w_::chosen_by_config(wiring_, argument_05_, argument_06_, argument_07_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_09_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 3, 3};
        ::hglrt::testing::expect_same(left_08_, right_09_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_018_ = [&]() {
        const ::hgraph::Bool argument_010_ = false;
        const auto trace_011_ = ::hglrt::testing::Dense<::hgraph::Bool>{true, false, true};
        const auto trace_013_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 2, 3};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_012_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(trace_011_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_014_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_013_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const ::hgraph::Bool argument_015_ = argument_010_; const auto argument_016_ = input_012_; const auto argument_017_ = input_014_; return ::hgl::demo::control::m_::w_::chosen_by_config(wiring_, argument_015_, argument_016_, argument_017_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_019_ = ::hglrt::testing::Dense<::hgraph::Int>{2, 3, 4};
        ::hglrt::testing::expect_same(left_018_, right_019_);
    }();
}

void a_let_of_a_returning_if_returns_or_goes_on()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_06_ = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Bool>{true, false, ::std::nullopt, true};
        const auto trace_02_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 2, 3, 4};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_02_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_04_ = input_01_; const auto argument_05_ = input_03_; return ::hgl::demo::control::m_::w_::early(wiring_, argument_04_, argument_05_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_07_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 14, 16, 4};
        ::hglrt::testing::expect_same(left_06_, right_07_);
    }();
}

void a_switch_with_no_default_runs_a_case_each_value_has()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_06_ = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Str>{::hgraph::Str{"a"}, ::hgraph::Str{"b"}, ::std::nullopt};
        const auto trace_02_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 2, 3};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Str>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_str(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_02_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_04_ = input_01_; const auto argument_05_ = input_03_; return ::hgl::demo::control::m_::w_::unmatched(wiring_, argument_04_, argument_05_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_07_ = ::hglrt::testing::Dense<::hgraph::Int>{2, 4, 5};
        ::hglrt::testing::expect_same(left_06_, right_07_);
    }();
}

void a_branch_in_an_unrolled_loop_is_configured_for_its_pass()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_08_ = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Bool>{true, false, true};
        const auto trace_02_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 2, 3};
        const ::hglrt::list<::hgraph::Int> argument_04_ = ::hglrt::list<::hgraph::Int>{2, 5};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_02_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_05_ = input_01_; const auto argument_06_ = input_03_; const ::hglrt::list<::hgraph::Int> argument_07_ = argument_04_; return ::hgl::demo::control::m_::w_::gated_product(wiring_, argument_05_, argument_06_, argument_07_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_09_ = ::hglrt::testing::Dense<::hgraph::Int>{10, 2, 30};
        ::hglrt::testing::expect_same(left_08_, right_09_);
    }();
}

void a_wiring_time_if_composes_one_branch()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{10, 20, ::std::nullopt, 30}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::control::m_::w_::sized(wiring_, input_00_, true), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 2, ::std::nullopt, 3};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_04_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{10, 20, ::std::nullopt, 30}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::control::m_::w_::sized(wiring_, input_03_, false), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_05_ = ::hglrt::testing::Dense<::hgraph::Int>{10, 20, ::std::nullopt, 30};
        ::hglrt::testing::expect_same(left_04_, right_05_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_07_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_06_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{10, 20}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::control::m_::w_::gated_by_config(wiring_, input_06_, true), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_08_ = ::hglrt::testing::Dense<::hgraph::Int>{10, 20};
        ::hglrt::testing::expect_same(left_07_, right_08_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_010_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_09_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{10, 20}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::control::m_::w_::gated_by_config(wiring_, input_09_, false), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_011_ = ::hglrt::testing::Dense<::hgraph::Int>{::std::nullopt, ::std::nullopt};
        ::hglrt::testing::expect_same(left_010_, right_011_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_013_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_012_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{10, 20}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::control::m_::w_::mixed(wiring_, input_012_, true), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_014_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 2};
        ::hglrt::testing::expect_same(left_013_, right_014_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_016_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_015_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{10, 20}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::control::m_::w_::mixed(wiring_, input_015_, false), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_017_ = ::hglrt::testing::Dense<::hgraph::Int>{5, ::std::nullopt};
        ::hglrt::testing::expect_same(left_016_, right_017_);
    }();
}

}  // namespace hgl::demo::control::m_::t_
