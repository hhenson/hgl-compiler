// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include "demo.lifting.h"
#include "core.h"

// Names this file writes, which no macro of a header above takes.
#undef ZeroPolicy
#undef a
#undef a_capture_keeps_the_child_live
#undef a_connection_of_i64_is_one_of_i64_nullable
#undef a_default_binds_until_its_input_publishes
#undef a_failure_and_a_returned_error_are_told_apart
#undef a_raised_error_keeps_its_provenance
#undef a_sink_fails_and_a_capture_of_it_publishes_the_error
#undef again
#undef an_earlier_write_stands
#undef an_evals_configuration_is_evaluated_once
#undef attempt
#undef b
#undef boundary
#undef branch_widened
#undef by
#undef captured_shown
#undef check
#undef checked
#undef checked_errors
#undef code
#undef command
#undef core
#undef count
#undef demo
#undef describe
#undef earliest_start
#undef error
#undef errors_of
#undef factor
#undef fail
#undef failure
#undef first
#undef flag
#undef guarded
#undef guarded_values
#undef handled
#undef inner
#undef kept_time
#undef lifting
#undef message
#undef multiplied
#undef never
#undef offset
#undef or_zero
#undef or_zero_of
#undef original
#undef outcome
#undef outcomes
#undef outer
#undef produced
#undef provenance
#undef quotient
#undef raise_again
#undef raised
#undef record
#undef relayed
#undef repeat
#undef repeated
#undef replay
#undef results_and_errors_are_traces
#undef returned
#undef sample
#undef scaled
#undef scaled_by
#undef shifted
#undef shifted_values
#undef shown
#undef shown_five
#undef shown_of
#undef taken
#undef tenfold
#undef v
#undef validity
#undef value
#undef widened_node_result
#undef widened_result
#undef widened_validity
#undef write_then_fail
#undef written_values
#undef x
#undef y

namespace hgl::demo::lifting::m_
{

// scaled(value: i64, factor: i64 = ..., offset: i64 = ...) -> i64 (`const fn`), module demo.lifting, line 8
::hgraph::Int scaled([[maybe_unused]] ::hgraph::Int value, [[maybe_unused]] ::hgraph::Int factor, [[maybe_unused]] ::hgraph::Int offset)
{
    return ::hglrt::add__i64_i64(::hglrt::mul__i64_i64(value, factor), offset);
}

// multiplied(value: i64, const factor: i64) -> i64 (`const fn`), module demo.lifting, line 12
::hgraph::Int multiplied([[maybe_unused]] ::hgraph::Int value, [[maybe_unused]] ::hgraph::Int factor)
{
    return ::hglrt::mul__i64_i64(value, factor);
}

namespace n_::scaled_by
{
// scaled_by(value: i64, factor: i64) -> i64 (graph), module demo.lifting, line 16
struct impl_
{
    // The call of `scaled` on line 17, lifted (FN-6).
    struct lifted_01_
    {
        static constexpr auto name = "demo.lifting:scaled";
        static void eval([[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::In<"factor", ::hgraph::TS<::hgraph::Int>, ::hgraph::InputValidity::Unchecked> input_00_, [[maybe_unused]] ::hgraph::Scalar<"offset", ::hgraph::Int> offset, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
        {
            [[maybe_unused]] const ::hgraph::Int factor = ::hglrt::fn_valid(input_00_) ? ::hgraph::Int{::hglrt::checked_value(input_00_)} : ::hgraph::Int{2};
            out_.set(::hgl::demo::lifting::m_::scaled(::hglrt::checked_value(value), factor, offset.value()));
            return;
        }
    };

    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> factor)
    {
        return ::hgraph::wire<lifted_01_>(wiring_, value, factor, static_cast<::hgraph::Int>(0));
    }
};
}  // namespace n_::scaled_by
namespace w_
{
// scaled_by(value: i64, factor: i64) -> i64 (graph), module demo.lifting, line 16
::hgraph::Port<::hgraph::TS<::hgraph::Int>> scaled_by(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> factor)
{
    return ::hgl::demo::lifting::m_::n_::scaled_by::impl_::compose(wiring_, value, factor);
}
}  // namespace w_

// check(value: i64) (`const fn`), module demo.lifting, line 22
void check([[maybe_unused]] ::hgraph::Int value)
{
    [[maybe_unused]] const ::hgraph::Int quotient = ::hglrt::floordiv__i64_i64<::hglrt::ZeroPolicy::fail>(1, value);
}

namespace n_::checked
{
// checked(value: i64) (graph), module demo.lifting, line 26
struct impl_
{
    // The call of `check` on line 27, lifted (FN-6).
    struct lifted_00_
    {
        static constexpr auto name = "demo.lifting:check";
        static void eval([[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>> value)
        {
            ::hgl::demo::lifting::m_::check(::hglrt::checked_value(value));
        }
    };

    static void compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
    {
        ::hgraph::wire<lifted_00_>(wiring_, value);
    }
};
}  // namespace n_::checked
namespace w_
{
// checked(value: i64) (graph), module demo.lifting, line 26
void checked(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    ::hgl::demo::lifting::m_::n_::checked::impl_::compose(wiring_, value);
}
}  // namespace w_

namespace n_::describe
{
// describe(failure: error, sample: i64) -> str (node), module demo.lifting, line 31
struct impl_
{
    static constexpr auto name = "demo.lifting::describe";

    static void eval([[maybe_unused]] ::hgraph::In<"failure", ::hgraph::TS<::hglrt::error>, ::hgraph::InputActivity::Passive, ::hgraph::InputValidity::Unchecked> failure, [[maybe_unused]] ::hgraph::In<"sample", ::hgraph::TS<::hgraph::Int>, ::hgraph::InputValidity::Unchecked> sample, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Str>> out_)
    {
        // when, line 32 (TIME-3)
        if (::hglrt::fn_modified(sample))
        {
            if (::hglrt::fn_valid(failure))
            {
                [[maybe_unused]] const ::std::optional<::hgraph::Str> code = (::hglrt::checked_value(failure)).code;
                if ((!code.has_value()))
                {
                    out_.set(::hgraph::Str{"uncoded"});
                    return;
                }
                if (::hglrt::fn_modified(failure))
                {
                    out_.set((*code));
                    return;
                }
                out_.set(::hgraph::Str{"retained"});
                return;
            }
            out_.set(::hgraph::Str{"clear"});
            return;
        }
    }
};
}  // namespace n_::describe
namespace w_
{
// describe(failure: error, sample: i64) -> str (node), module demo.lifting, line 31
::hgraph::Port<::hgraph::TS<::hgraph::Str>> describe(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hglrt::error>> failure, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> sample)
{
    return ::hgraph::wire<::hgl::demo::lifting::m_::n_::describe::impl_>(wiring_, failure, sample);
}
}  // namespace w_

namespace n_::checked_errors
{
// checked_errors(value: i64) -> str (graph), module demo.lifting, line 43
struct impl_
{
    // The call of `check` on line 44, lifted (FN-6).
    struct lifted_01_
    {
        static constexpr auto name = "demo.lifting:check";
        static void eval([[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>> value)
        {
            ::hgl::demo::lifting::m_::check(::hglrt::checked_value(value));
        }
    };

    // The `capture` of `check` on line 44 (ERR-7): the graph hgraph's
    // `try_except_` evaluates as its child.
    struct captured_02_
    {
        static constexpr auto name = "demo.lifting:capture(check)";
        static void compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_)
        {
            ::hgraph::wire<lifted_01_>(wiring_, input_00_);
        }
    };

    static ::hgraph::Port<::hgraph::TS<::hgraph::Str>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
    {
        [[maybe_unused]] const auto boundary = ::hglrt::capture<captured_02_>(wiring_, value);
        return ::hgl::demo::lifting::m_::w_::describe(wiring_, ::hglrt::captured_error(boundary), value);
    }
};
}  // namespace n_::checked_errors
namespace w_
{
// checked_errors(value: i64) -> str (graph), module demo.lifting, line 43
::hgraph::Port<::hgraph::TS<::hgraph::Str>> checked_errors(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgl::demo::lifting::m_::n_::checked_errors::impl_::compose(wiring_, value);
}
}  // namespace w_

// attempt(value: i64) -> result<i64, error> (`const fn`), module demo.lifting, line 50
::hglrt::result<::hgraph::Int> attempt([[maybe_unused]] ::hgraph::Int value)
{
    if (::hglrt::lt__T_T<::hgraph::Int>(value, 0))
    {
        return ::hglrt::result<::hgraph::Int>{::std::unexpect, ::hglrt::make_error(::hgraph::Str{"negative input"}, ::hgraph::Str{"numeric.out_of_range"})};
    }
    return ::hglrt::result<::hgraph::Int>{::hglrt::mul__i64_i64(value, 10)};
}

namespace n_::raised
{
// raised(value: i64) -> i64 (node), module demo.lifting, line 55
struct impl_
{
    static constexpr auto name = "demo.lifting::raised";

    static void eval([[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 56 (TIME-3)
        {
            const auto propagated_00_ = ::hgl::demo::lifting::m_::attempt(::hglrt::checked_value(value));
            if (!propagated_00_.has_value())
            {
                ::hglrt::raise(propagated_00_.error());
            }
            out_.set((*propagated_00_));
            return;
        }
    }
};
}  // namespace n_::raised
namespace w_
{
// raised(value: i64) -> i64 (node), module demo.lifting, line 55
::hgraph::Port<::hgraph::TS<::hgraph::Int>> raised(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgraph::wire<::hgl::demo::lifting::m_::n_::raised::impl_>(wiring_, value);
}
}  // namespace w_

namespace n_::returned
{
// returned(value: i64) -> result<i64, error> (node), module demo.lifting, line 59
struct impl_
{
    static constexpr auto name = "demo.lifting::returned";

    static void eval([[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hglrt::result<::hgraph::Int>>> out_)
    {
        // when, line 60 (TIME-3)
        {
            out_.set(::hgl::demo::lifting::m_::attempt(::hglrt::checked_value(value)));
            return;
        }
    }
};
}  // namespace n_::returned
namespace w_
{
// returned(value: i64) -> result<i64, error> (node), module demo.lifting, line 59
::hgraph::Port<::hgraph::TS<::hglrt::result<::hgraph::Int>>> returned(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgraph::wire<::hgl::demo::lifting::m_::n_::returned::impl_>(wiring_, value);
}
}  // namespace w_

namespace n_::outcome
{
// outcome(taken: result<i64, error>) -> str (node), module demo.lifting, line 63
struct impl_
{
    static constexpr auto name = "demo.lifting::outcome";

    static void eval([[maybe_unused]] ::hgraph::In<"taken", ::hgraph::TS<::hglrt::result<::hgraph::Int>>> taken, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Str>> out_)
    {
        // when, line 64 (TIME-3)
        {
            if ((!(::hglrt::checked_value(taken)).has_value()))
            {
                out_.set(((::hglrt::checked_value(taken)).error()).message);
                return;
            }
            out_.set(::hgraph::Str{"ok"});
            return;
        }
    }
};
}  // namespace n_::outcome
namespace w_
{
// outcome(taken: result<i64, error>) -> str (node), module demo.lifting, line 63
::hgraph::Port<::hgraph::TS<::hgraph::Str>> outcome(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hglrt::result<::hgraph::Int>>> taken)
{
    return ::hgraph::wire<::hgl::demo::lifting::m_::n_::outcome::impl_>(wiring_, taken);
}
}  // namespace w_

namespace n_::outcomes
{
// outcomes(value: i64) -> str (graph), module demo.lifting, line 70
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Str>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
    {
        return ::hgl::demo::lifting::m_::w_::outcome(wiring_, ::hgl::demo::lifting::m_::w_::returned(wiring_, value));
    }
};
}  // namespace n_::outcomes
namespace w_
{
// outcomes(value: i64) -> str (graph), module demo.lifting, line 70
::hgraph::Port<::hgraph::TS<::hgraph::Str>> outcomes(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgl::demo::lifting::m_::n_::outcomes::impl_::compose(wiring_, value);
}
}  // namespace w_

namespace n_::shifted
{
// shifted(value: i64, const by: i64) -> i64 (node), module demo.lifting, line 75
struct impl_
{
    static constexpr auto name = "demo.lifting::shifted";

    static void eval([[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::Scalar<"by", ::hgraph::Int> by, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 76 (TIME-3)
        {
            const auto propagated_00_ = ::hgl::demo::lifting::m_::attempt(::hglrt::checked_value(value));
            if (!propagated_00_.has_value())
            {
                ::hglrt::raise(propagated_00_.error());
            }
            out_.set(::hglrt::add__i64_i64((*propagated_00_), by.value()));
            return;
        }
    }
};
}  // namespace n_::shifted
namespace w_
{
// shifted(value: i64, const by: i64) -> i64 (node), module demo.lifting, line 75
::hgraph::Port<::hgraph::TS<::hgraph::Int>> shifted(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value, ::hgraph::Int by)
{
    return ::hgraph::wire<::hgl::demo::lifting::m_::n_::shifted::impl_>(wiring_, value, by);
}
}  // namespace w_

namespace n_::shifted_values
{
// shifted_values(value: i64) -> i64 (graph), module demo.lifting, line 79
struct impl_
{
    // The `capture` of `shifted` on line 80 (ERR-7): the graph hgraph's
    // `try_except_` evaluates as its child.
    struct captured_02_
    {
        static constexpr auto name = "demo.lifting:capture(shifted)";
        static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_, [[maybe_unused]] ::hgraph::Scalar<"by", ::hgraph::Int> input_01_)
        {
            return ::hgl::demo::lifting::m_::w_::shifted(wiring_, input_00_, input_01_.value());
        }
    };

    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
    {
        [[maybe_unused]] const auto boundary = ::hglrt::capture<captured_02_>(wiring_, value, static_cast<::hgraph::Int>(5));
        return ::hglrt::captured_value(boundary);
    }
};
}  // namespace n_::shifted_values
namespace w_
{
// shifted_values(value: i64) -> i64 (graph), module demo.lifting, line 79
::hgraph::Port<::hgraph::TS<::hgraph::Int>> shifted_values(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgl::demo::lifting::m_::n_::shifted_values::impl_::compose(wiring_, value);
}
}  // namespace w_

namespace n_::guarded
{
// guarded(value: i64) -> i64 (graph), module demo.lifting, line 84
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
    {
        return ::hgl::demo::lifting::m_::w_::raised(wiring_, value);
    }
};
}  // namespace n_::guarded
namespace w_
{
// guarded(value: i64) -> i64 (graph), module demo.lifting, line 84
::hgraph::Port<::hgraph::TS<::hgraph::Int>> guarded(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgl::demo::lifting::m_::n_::guarded::impl_::compose(wiring_, value);
}
}  // namespace w_

namespace n_::guarded_values
{
// guarded_values(value: i64) -> i64 (graph), module demo.lifting, line 88
struct impl_
{
    // The `capture` of `guarded` on line 89 (ERR-7): the graph hgraph's
    // `try_except_` evaluates as its child.
    struct captured_01_
    {
        static constexpr auto name = "demo.lifting:capture(guarded)";
        static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_)
        {
            return ::hgl::demo::lifting::m_::w_::guarded(wiring_, input_00_);
        }
    };

    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
    {
        return ::hglrt::captured_value(::hglrt::capture<captured_01_>(wiring_, value));
    }
};
}  // namespace n_::guarded_values
namespace w_
{
// guarded_values(value: i64) -> i64 (graph), module demo.lifting, line 88
::hgraph::Port<::hgraph::TS<::hgraph::Int>> guarded_values(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgl::demo::lifting::m_::n_::guarded_values::impl_::compose(wiring_, value);
}
}  // namespace w_

namespace n_::write_then_fail
{
// write_then_fail(value: i64) -> i64 (node), module demo.lifting, line 94
struct impl_
{
    static constexpr auto name = "demo.lifting::write_then_fail";

    static void eval([[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 96 (TIME-3)
        {
            out_.set(::hglrt::checked_value(value));
            const auto propagated_00_ = ::hgl::demo::lifting::m_::attempt(::hglrt::checked_value(value));
            if (!propagated_00_.has_value())
            {
                ::hglrt::raise(propagated_00_.error());
            }
            [[maybe_unused]] const ::hgraph::Int tenfold = (*propagated_00_);
            out_.set(tenfold);
        }
    }
};
}  // namespace n_::write_then_fail
namespace w_
{
// write_then_fail(value: i64) -> i64 (node), module demo.lifting, line 94
::hgraph::Port<::hgraph::TS<::hgraph::Int>> write_then_fail(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgraph::wire<::hgl::demo::lifting::m_::n_::write_then_fail::impl_>(wiring_, value);
}
}  // namespace w_

namespace n_::written_values
{
// written_values(value: i64) -> i64 (graph), module demo.lifting, line 103
struct impl_
{
    // The `capture` of `write_then_fail` on line 104 (ERR-7): the graph hgraph's
    // `try_except_` evaluates as its child.
    struct captured_01_
    {
        static constexpr auto name = "demo.lifting:capture(write_then_fail)";
        static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_)
        {
            return ::hgl::demo::lifting::m_::w_::write_then_fail(wiring_, input_00_);
        }
    };

    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
    {
        [[maybe_unused]] const auto boundary = ::hglrt::capture<captured_01_>(wiring_, value);
        return ::hglrt::captured_value(boundary);
    }
};
}  // namespace n_::written_values
namespace w_
{
// written_values(value: i64) -> i64 (graph), module demo.lifting, line 103
::hgraph::Port<::hgraph::TS<::hgraph::Int>> written_values(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgl::demo::lifting::m_::n_::written_values::impl_::compose(wiring_, value);
}
}  // namespace w_

namespace n_::raise_again
{
// raise_again(first: error, repeat: bool) -> error (node), module demo.lifting, line 110
struct impl_
{
    static constexpr auto name = "demo.lifting::raise_again";

    static void eval([[maybe_unused]] ::hgraph::In<"first", ::hgraph::TS<::hglrt::error>> first, [[maybe_unused]] ::hgraph::In<"repeat", ::hgraph::TS<::hgraph::Bool>, ::hgraph::InputValidity::Unchecked> repeat, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hglrt::error>> out_)
    {
        // when, line 111 (TIME-3)
        if (::hglrt::fn_modified(first))
        {
            out_.set(::hglrt::checked_value(first));
            return;
        }
        // when, line 112 (TIME-3)
        if (::hglrt::fn_modified(repeat))
        {
            [[maybe_unused]] const ::hglrt::result<::hglrt::error> failure = ::hglrt::result<::hglrt::error>{::std::unexpect, ::hglrt::checked_value(first)};
            const auto propagated_00_ = failure;
            if (!propagated_00_.has_value())
            {
                ::hglrt::raise(propagated_00_.error());
            }
            out_.set((*propagated_00_));
            return;
        }
    }
};
}  // namespace n_::raise_again
namespace w_
{
// raise_again(first: error, repeat: bool) -> error (node), module demo.lifting, line 110
::hgraph::Port<::hgraph::TS<::hglrt::error>> raise_again(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hglrt::error>> first, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> repeat)
{
    return ::hgraph::wire<::hgl::demo::lifting::m_::n_::raise_again::impl_>(wiring_, first, repeat);
}
}  // namespace w_

namespace n_::handled
{
// handled(value: i64, repeat: bool) -> error (graph), module demo.lifting, line 118
struct impl_
{
    // The `capture` of `raised` on line 119 (ERR-7): the graph hgraph's
    // `try_except_` evaluates as its child.
    struct captured_01_
    {
        static constexpr auto name = "demo.lifting:capture(raised)";
        static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_)
        {
            return ::hgl::demo::lifting::m_::w_::raised(wiring_, input_00_);
        }
    };

    static ::hgraph::Port<::hgraph::TS<::hglrt::error>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> repeat)
    {
        [[maybe_unused]] const auto inner = ::hglrt::capture<captured_01_>(wiring_, value);
        return ::hgl::demo::lifting::m_::w_::raise_again(wiring_, ::hglrt::captured_error(inner), repeat);
    }
};
}  // namespace n_::handled
namespace w_
{
// handled(value: i64, repeat: bool) -> error (graph), module demo.lifting, line 118
::hgraph::Port<::hgraph::TS<::hglrt::error>> handled(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> repeat)
{
    return ::hgl::demo::lifting::m_::n_::handled::impl_::compose(wiring_, value, repeat);
}
}  // namespace w_

namespace n_::kept_time
{
// kept_time(first: error, repeated: error) -> datetime (node), module demo.lifting, line 123
struct impl_
{
    static constexpr auto name = "demo.lifting::kept_time";

    static void eval([[maybe_unused]] ::hgraph::In<"first", ::hgraph::TS<::hglrt::error>, ::hgraph::InputActivity::Passive> first, [[maybe_unused]] ::hgraph::In<"repeated", ::hgraph::TS<::hglrt::error>> repeated, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::DateTime>> out_)
    {
        // when, line 124 (TIME-3)
        if (::hglrt::fn_modified(repeated))
        {
            [[maybe_unused]] const ::std::optional<::hgraph::ErrorContext> original = (::hglrt::checked_value(first)).context;
            [[maybe_unused]] const ::std::optional<::hgraph::ErrorContext> again = (::hglrt::checked_value(repeated)).context;
            if (((!original.has_value()) || (!again.has_value())))
            {
                out_.set(::hgl::core::m_::never());
                return;
            }
            if (([&] { const ::hgraph::Str argument_00_ = ((*original)).node; const ::hgraph::Str argument_01_ = ((*again)).node; return ::hglrt::ne__T_T<::hgraph::Str>(argument_00_, argument_01_); }() || [&] { const ::hgraph::Str argument_02_ = ((*original)).phase; const ::hgraph::Str argument_03_ = ((*again)).phase; return ::hglrt::ne__T_T<::hgraph::Str>(argument_02_, argument_03_); }()))
            {
                out_.set(::hgl::core::m_::never());
                return;
            }
            out_.set(((*again)).evaluation_time);
            return;
        }
    }
};
}  // namespace n_::kept_time
namespace w_
{
// kept_time(first: error, repeated: error) -> datetime (node), module demo.lifting, line 123
::hgraph::Port<::hgraph::TS<::hgraph::DateTime>> kept_time(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hglrt::error>> first, ::hgraph::Port<::hgraph::TS<::hglrt::error>> repeated)
{
    return ::hgraph::wire<::hgl::demo::lifting::m_::n_::kept_time::impl_>(wiring_, first, repeated);
}
}  // namespace w_

namespace n_::provenance
{
// provenance(value: i64, repeat: bool) -> datetime (graph), module demo.lifting, line 133
struct impl_
{
    // The `capture` of `handled` on line 134 (ERR-7): the graph hgraph's
    // `try_except_` evaluates as its child.
    struct captured_02_
    {
        static constexpr auto name = "demo.lifting:capture(handled)";
        static ::hgraph::Port<::hgraph::TS<::hglrt::error>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_01_)
        {
            return ::hgl::demo::lifting::m_::w_::handled(wiring_, input_00_, input_01_);
        }
    };

    static ::hgraph::Port<::hgraph::TS<::hgraph::DateTime>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> repeat)
    {
        [[maybe_unused]] const auto outer = ::hglrt::capture<captured_02_>(wiring_, value, repeat);
        return [&] { const auto argument_03_ = ::hglrt::captured_value(outer); const auto argument_04_ = ::hglrt::captured_error(outer); return ::hgl::demo::lifting::m_::w_::kept_time(wiring_, argument_03_, argument_04_); }();
    }
};
}  // namespace n_::provenance
namespace w_
{
// provenance(value: i64, repeat: bool) -> datetime (graph), module demo.lifting, line 133
::hgraph::Port<::hgraph::TS<::hgraph::DateTime>> provenance(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> repeat)
{
    return ::hgl::demo::lifting::m_::n_::provenance::impl_::compose(wiring_, value, repeat);
}
}  // namespace w_

namespace n_::errors_of
{
// errors_of(value: i64) -> error (graph), module demo.lifting, line 179
struct impl_
{
    // The `capture` of `raised` on line 180 (ERR-7): the graph hgraph's
    // `try_except_` evaluates as its child.
    struct captured_01_
    {
        static constexpr auto name = "demo.lifting:capture(raised)";
        static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_)
        {
            return ::hgl::demo::lifting::m_::w_::raised(wiring_, input_00_);
        }
    };

    static ::hgraph::Port<::hgraph::TS<::hglrt::error>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
    {
        [[maybe_unused]] const auto boundary = ::hglrt::capture<captured_01_>(wiring_, value);
        return ::hglrt::captured_error(boundary);
    }
};
}  // namespace n_::errors_of
namespace w_
{
// errors_of(value: i64) -> error (graph), module demo.lifting, line 179
::hgraph::Port<::hgraph::TS<::hglrt::error>> errors_of(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgl::demo::lifting::m_::n_::errors_of::impl_::compose(wiring_, value);
}
}  // namespace w_

// or_zero(a: i64, b: i64?) -> i64 (`const fn`), module demo.lifting, line 194
::hgraph::Int or_zero([[maybe_unused]] ::hgraph::Int a, [[maybe_unused]] ::std::optional<::hgraph::Int> b)
{
    [[maybe_unused]] const ::std::optional<::hgraph::Int> x = b;
    if ((!x.has_value()))
    {
        return a;
    }
    return ::hglrt::add__i64_i64(a, (*x));
}

namespace n_::or_zero_of
{
// or_zero_of(a: i64, b: i64) -> i64 (graph), module demo.lifting, line 200
struct impl_
{
    // The call of `or_zero` on line 200, lifted (FN-6).
    struct lifted_01_
    {
        static constexpr auto name = "demo.lifting:or_zero";
        static void eval([[maybe_unused]] ::hgraph::In<"a", ::hgraph::TS<::hgraph::Int>> a, [[maybe_unused]] ::hgraph::In<"b", ::hgraph::TS<::hgraph::Int>, ::hgraph::InputValidity::Unchecked> input_00_, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
        {
            [[maybe_unused]] const ::std::optional<::hgraph::Int> b = ::hglrt::fn_valid(input_00_) ? ::std::optional<::hgraph::Int>{::hglrt::checked_value(input_00_)} : ::std::optional<::hgraph::Int>{::std::nullopt};
            out_.set(::hgl::demo::lifting::m_::or_zero(::hglrt::checked_value(a), b));
            return;
        }
    };

    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> a, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> b)
    {
        return ::hgraph::wire<lifted_01_>(wiring_, a, b);
    }
};
}  // namespace n_::or_zero_of
namespace w_
{
// or_zero_of(a: i64, b: i64) -> i64 (graph), module demo.lifting, line 200
::hgraph::Port<::hgraph::TS<::hgraph::Int>> or_zero_of(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> a, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> b)
{
    return ::hgl::demo::lifting::m_::n_::or_zero_of::impl_::compose(wiring_, a, b);
}
}  // namespace w_

namespace n_::shown
{
// shown(x: i64?) -> str (node), module demo.lifting, line 202
struct impl_
{
    static constexpr auto name = "demo.lifting::shown";

    static void eval([[maybe_unused]] ::hgraph::In<"x", ::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> x, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Str>> out_)
    {
        // when, line 203 (TIME-3)
        {
            [[maybe_unused]] const ::std::optional<::hgraph::Int> v = ::hglrt::nullable_value(x);
            if ((!v.has_value()))
            {
                out_.set(::hgraph::Str{"null"});
                return;
            }
            out_.set(::hglrt::convert_i64<::hgraph::Str>((*v)));
            return;
        }
    }
};
}  // namespace n_::shown
namespace w_
{
// shown(x: i64?) -> str (node), module demo.lifting, line 202
::hgraph::Port<::hgraph::TS<::hgraph::Str>> shown(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> x)
{
    return ::hgraph::wire<::hgl::demo::lifting::m_::n_::shown::impl_>(wiring_, x);
}
}  // namespace w_

namespace n_::relayed
{
// relayed(x: i64) -> i64 (node), module demo.lifting, line 210
struct impl_
{
    static constexpr auto name = "demo.lifting::relayed";

    static void eval([[maybe_unused]] ::hgraph::In<"x", ::hgraph::TS<::hgraph::Int>> x, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 211 (TIME-3)
        {
            out_.set(::hglrt::checked_value(x));
            return;
        }
    }
};
}  // namespace n_::relayed
namespace w_
{
// relayed(x: i64) -> i64 (node), module demo.lifting, line 210
::hgraph::Port<::hgraph::TS<::hgraph::Int>> relayed(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x)
{
    return ::hgraph::wire<::hgl::demo::lifting::m_::n_::relayed::impl_>(wiring_, x);
}
}  // namespace w_

namespace n_::shown_of
{
// shown_of(x: i64) -> str (graph), module demo.lifting, line 214
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Str>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x)
    {
        return ::hgl::demo::lifting::m_::w_::shown(wiring_, (::hgl::demo::lifting::m_::w_::relayed(wiring_, x)).as<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>>());
    }
};
}  // namespace n_::shown_of
namespace w_
{
// shown_of(x: i64) -> str (graph), module demo.lifting, line 214
::hgraph::Port<::hgraph::TS<::hgraph::Str>> shown_of(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x)
{
    return ::hgl::demo::lifting::m_::n_::shown_of::impl_::compose(wiring_, x);
}
}  // namespace w_

namespace n_::shown_five
{
// shown_five(flag: bool) -> str (graph), module demo.lifting, line 216
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Str>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> flag)
    {
        return ::hgl::demo::lifting::m_::w_::shown(wiring_, (::hglrt::constant<::hgraph::Int>(wiring_, 5)).as<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>>());
    }
};
}  // namespace n_::shown_five
namespace w_
{
// shown_five(flag: bool) -> str (graph), module demo.lifting, line 216
::hgraph::Port<::hgraph::TS<::hgraph::Str>> shown_five(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> flag)
{
    return ::hgl::demo::lifting::m_::n_::shown_five::impl_::compose(wiring_, flag);
}
}  // namespace w_

namespace n_::widened_result
{
// widened_result(x: i64) -> i64? (graph), module demo.lifting, line 218
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x)
    {
        return (x).as<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>>();
    }
};
}  // namespace n_::widened_result
namespace w_
{
// widened_result(x: i64) -> i64? (graph), module demo.lifting, line 218
::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> widened_result(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x)
{
    return ::hgl::demo::lifting::m_::n_::widened_result::impl_::compose(wiring_, x);
}
}  // namespace w_

namespace n_::widened_node_result
{
// widened_node_result(x: i64) -> i64? (graph), module demo.lifting, line 220
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x)
    {
        [[maybe_unused]] const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> y = ::hgl::demo::lifting::m_::w_::relayed(wiring_, x);
        return (y).as<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>>();
    }
};
}  // namespace n_::widened_node_result
namespace w_
{
// widened_node_result(x: i64) -> i64? (graph), module demo.lifting, line 220
::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> widened_node_result(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x)
{
    return ::hgl::demo::lifting::m_::n_::widened_node_result::impl_::compose(wiring_, x);
}
}  // namespace w_

namespace n_::produced
{
// produced(command: i64) -> i64 (node), module demo.lifting, line 225
struct impl_
{
    static constexpr auto name = "demo.lifting::produced";

    static void eval([[maybe_unused]] ::hgraph::In<"command", ::hgraph::TS<::hgraph::Int>> command, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 227 (TIME-3)
        {
            if (::hglrt::ne__i64_i64_f64_f64(::hglrt::checked_value(command), 0, 0.0, 0.0))
            {
                out_.set(::hglrt::checked_value(command));
                return;
            }
            ::hglrt::fn_invalidate(out_);
        }
    }
};
}  // namespace n_::produced
namespace w_
{
// produced(command: i64) -> i64 (node), module demo.lifting, line 225
::hgraph::Port<::hgraph::TS<::hgraph::Int>> produced(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> command)
{
    return ::hgraph::wire<::hgl::demo::lifting::m_::n_::produced::impl_>(wiring_, command);
}
}  // namespace w_

namespace n_::validity
{
// validity(x: i64?) -> bool (node), module demo.lifting, line 233
struct impl_
{
    static constexpr auto name = "demo.lifting::validity";

    static void eval([[maybe_unused]] ::hgraph::In<"x", ::hgraph::TS<::hgraph::Optional<::hgraph::Int>>, ::hgraph::InputValidity::Unchecked> x, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Bool>> out_)
    {
        // when, line 234 (TIME-3)
        {
            out_.set(::hglrt::fn_valid(x));
            return;
        }
    }
};
}  // namespace n_::validity
namespace w_
{
// validity(x: i64?) -> bool (node), module demo.lifting, line 233
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> validity(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> x)
{
    return ::hgraph::wire<::hgl::demo::lifting::m_::n_::validity::impl_>(wiring_, x);
}
}  // namespace w_

namespace n_::widened_validity
{
// widened_validity(command: i64) -> bool (graph), module demo.lifting, line 237
struct impl_
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> command)
    {
        return ::hgl::demo::lifting::m_::w_::validity(wiring_, (::hgl::demo::lifting::m_::w_::produced(wiring_, command)).as<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>>());
    }
};
}  // namespace n_::widened_validity
namespace w_
{
// widened_validity(command: i64) -> bool (graph), module demo.lifting, line 237
::hgraph::Port<::hgraph::TS<::hgraph::Bool>> widened_validity(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> command)
{
    return ::hgl::demo::lifting::m_::n_::widened_validity::impl_::compose(wiring_, command);
}
}  // namespace w_

namespace n_::branch_widened
{
// branch_widened(flag: bool, x: i64, y: i64) -> i64? (graph), module demo.lifting, line 239
struct impl_
{
    // The `if` on line 240 over a connection (CTRL-2, CTRL-13 to CTRL-15): a child
    // graph for each branch, of which hgraph's `switch_` runs the one selected.
    struct switch_00_
    {
        // The branch selected while the condition is `true`.
        struct branch_00_
        {
            static constexpr auto name = "demo.lifting:if on line 240";
            static ::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> compose([[maybe_unused]] ::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> y)
            {
                return (x).as<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>>();
            }
        };
    
        // The branch selected while the condition is `false`.
        struct branch_01_
        {
            static constexpr auto name = "demo.lifting:if on line 240";
            static ::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> compose([[maybe_unused]] ::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> y)
            {
                return (y).as<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>>();
            }
        };
    
        static ::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> wire_(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> key_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> y)
        {
            ::std::vector<::hgraph::SwitchBranchGraph> branches_;
            branches_.push_back(::hgraph::SwitchBranchGraph{::hgraph::Value{static_cast<::hgraph::Bool>(true)}, ::hgraph::compile_subgraph<branch_00_>(wiring_)});
            branches_.push_back(::hgraph::SwitchBranchGraph{::hgraph::Value{static_cast<::hgraph::Bool>(false)}, ::hgraph::compile_subgraph<branch_01_>(wiring_)});
            const ::std::array<::hgraph::WiringPortRef, 2> inputs_{x.erased(), y.erased()};
            return ::hgraph::Port<void>{wiring_, ::hgraph::compiled::switch_(wiring_, key_.erased(), inputs_, ::std::move(branches_))}.as<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>>();
        }
    };

    static ::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> flag, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> y)
    {
        return switch_00_::wire_(wiring_, flag, x, y);
    }
};
}  // namespace n_::branch_widened
namespace w_
{
// branch_widened(flag: bool, x: i64, y: i64) -> i64? (graph), module demo.lifting, line 239
::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> branch_widened(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> flag, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> y)
{
    return ::hgl::demo::lifting::m_::n_::branch_widened::impl_::compose(wiring_, flag, x, y);
}
}  // namespace w_

namespace n_::captured_shown
{
// captured_shown(x: i64) -> str (graph), module demo.lifting, line 244
struct impl_
{
    // The `capture` of `shown` on line 245 (ERR-7): the graph hgraph's
    // `try_except_` evaluates as its child.
    struct captured_01_
    {
        static constexpr auto name = "demo.lifting:capture(shown)";
        static ::hgraph::Port<::hgraph::TS<::hgraph::Str>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> input_00_)
        {
            return ::hgl::demo::lifting::m_::w_::shown(wiring_, input_00_);
        }
    };

    static ::hgraph::Port<::hgraph::TS<::hgraph::Str>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x)
    {
        [[maybe_unused]] const auto boundary = ::hglrt::capture<captured_01_>(wiring_, (x).as<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>>());
        return ::hglrt::captured_value(boundary);
    }
};
}  // namespace n_::captured_shown
namespace w_
{
// captured_shown(x: i64) -> str (graph), module demo.lifting, line 244
::hgraph::Port<::hgraph::TS<::hgraph::Str>> captured_shown(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> x)
{
    return ::hgl::demo::lifting::m_::n_::captured_shown::impl_::compose(wiring_, x);
}
}  // namespace w_

}  // namespace hgl::demo::lifting::m_

namespace hgl::demo::lifting::m_::t_
{

// The call of `scaled` on line 140, lifted (FN-6).
struct lifted_00_4003_
{
    static constexpr auto name = "demo.lifting:scaled";
    static void eval([[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::In<"factor", ::hgraph::TS<::hgraph::Int>, ::hgraph::InputValidity::Unchecked> input_014_, [[maybe_unused]] ::hgraph::In<"offset", ::hgraph::TS<::hgraph::Int>, ::hgraph::InputValidity::Unchecked> input_015_, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        [[maybe_unused]] const ::hgraph::Int factor = ::hglrt::fn_valid(input_014_) ? ::hgraph::Int{::hglrt::checked_value(input_014_)} : ::hgraph::Int{2};
        [[maybe_unused]] const ::hgraph::Int offset = ::hglrt::fn_valid(input_015_) ? ::hgraph::Int{::hglrt::checked_value(input_015_)} : ::hgraph::Int{0};
        out_.set(::hgl::demo::lifting::m_::scaled(::hglrt::checked_value(value), factor, offset));
        return;
    }
};

// The call of `scaled` on line 141, lifted (FN-6).
struct lifted_00_4080_
{
    static constexpr auto name = "demo.lifting:scaled";
    static void eval([[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::Scalar<"factor", ::hgraph::Int> factor, [[maybe_unused]] ::hgraph::Scalar<"offset", ::hgraph::Int> offset, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        out_.set(::hgl::demo::lifting::m_::scaled(::hglrt::checked_value(value), factor.value(), offset.value()));
        return;
    }
};

void a_default_binds_until_its_input_publishes()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_06_ = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Int>{1, ::std::nullopt, 3};
        const auto trace_02_ = ::hglrt::testing::Dense<::hgraph::Int>{::std::nullopt, 4, ::std::nullopt};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_02_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_04_ = input_01_; const auto argument_05_ = input_03_; return ::hgl::demo::lifting::m_::w_::scaled_by(wiring_, argument_04_, argument_05_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_07_ = ::hglrt::testing::Dense<::hgraph::Int>{2, 4, 12};
        ::hglrt::testing::expect_same(left_06_, right_07_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_016_ = [&]() {
        const auto trace_08_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 2};
        const auto trace_010_ = ::hglrt::testing::Dense<::hgraph::Int>{::std::nullopt, 3};
        const auto trace_012_ = ::hglrt::testing::Dense<::hgraph::Int>{1};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_09_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_08_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_011_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_010_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_013_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_012_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgraph::wire<lifted_00_4003_>(wiring_, input_09_, input_011_, input_013_), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_017_ = ::hglrt::testing::Dense<::hgraph::Int>{3, 7};
        ::hglrt::testing::expect_same(left_016_, right_017_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_019_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_018_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 2}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgraph::wire<lifted_00_4080_>(wiring_, input_018_, static_cast<::hgraph::Int>(2), static_cast<::hgraph::Int>(0)), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_020_ = ::hglrt::testing::Dense<::hgraph::Int>{2, 4};
        ::hglrt::testing::expect_same(left_019_, right_020_);
    }();
}

// The call of `multiplied` on line 146, lifted (FN-6).
struct lifted_00_4197_
{
    static constexpr auto name = "demo.lifting:multiplied";
    static void eval([[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::Scalar<"factor", ::hgraph::Int> factor, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        out_.set(::hgl::demo::lifting::m_::multiplied(::hglrt::checked_value(value), factor.value()));
        return;
    }
};

void an_evals_configuration_is_evaluated_once()
{
    ::hglrt::testing::statement();
    [[maybe_unused]] ::hgraph::Int count = 0;
    ::hglrt::testing::statement();
    [&] {
        const auto left_03_ = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 2};
        const ::hgraph::Int argument_02_ = [&]() -> ::hgraph::Int {
    count = ::hglrt::add__i64_i64(count, 1);
    return ::hglrt::add__i64_i64(count, 2);
}();
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgraph::wire<lifted_00_4197_>(wiring_, input_01_, static_cast<::hgraph::Int>(argument_02_)), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_04_ = ::hglrt::testing::Dense<::hgraph::Int>{3, 6};
        ::hglrt::testing::expect_same(left_03_, right_04_);
    }();
    ::hglrt::testing::statement();
    ::hglrt::testing::check(::hglrt::eq__i64_i64_f64_f64(count, 1, 0.0, 0.0));
}

void a_sink_fails_and_a_capture_of_it_publishes_the_error()
{
    ::hglrt::testing::statement();
    static_cast<void>(::hglrt::testing::run([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 2}, start_));
        ::hgl::demo::lifting::m_::w_::checked(wiring_, input_00_);
    }));
    ::hglrt::testing::statement();
    ::hglrt::testing::raises("numeric.division_by_zero", [&] {
        static_cast<void>(::hglrt::testing::run([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 0}, start_));
        ::hgl::demo::lifting::m_::w_::checked(wiring_, input_01_);
    }));
    });
    ::hglrt::testing::statement();
    [&] {
        const auto left_03_ = ::hglrt::testing::evaluate<::hgraph::Str>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_02_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 0, 2}, start_));
        ::hgl::core::m_::w_::i_::record::of_str(wiring_, ::hgl::demo::lifting::m_::w_::checked_errors(wiring_, input_02_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_04_ = ::hglrt::testing::Dense<::hgraph::Str>{::hgraph::Str{"clear"}, ::hgraph::Str{"numeric.division_by_zero"}, ::hgraph::Str{"retained"}};
        ::hglrt::testing::expect_same(left_03_, right_04_);
    }();
}

void a_failure_and_a_returned_error_are_told_apart()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgraph::Str>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, ::hglrt::neg__i64(1)}, start_));
        ::hgl::core::m_::w_::i_::record::of_str(wiring_, ::hgl::demo::lifting::m_::w_::outcomes(wiring_, input_00_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgraph::Str>{::hgraph::Str{"ok"}, ::hgraph::Str{"negative input"}};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
    ::hglrt::testing::statement();
    ::hglrt::testing::raises("numeric.out_of_range", [&] {
        static_cast<void>(::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, ::hglrt::neg__i64(1)}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::lifting::m_::w_::raised(wiring_, input_03_), ::hgraph::Str{::hglrt::testing::run_output});
    }));
    });
}

void a_capture_keeps_the_child_live()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, ::hglrt::neg__i64(1), 2}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::lifting::m_::w_::shifted_values(wiring_, input_00_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgraph::Int>{15, ::std::nullopt, 25};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_04_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, ::hglrt::neg__i64(1), 2}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::lifting::m_::w_::guarded_values(wiring_, input_03_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_05_ = ::hglrt::testing::Dense<::hgraph::Int>{10, ::std::nullopt, 20};
        ::hglrt::testing::expect_same(left_04_, right_05_);
    }();
}

void an_earlier_write_stands()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, ::hglrt::neg__i64(2), 3}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::lifting::m_::w_::written_values(wiring_, input_00_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hgraph::Int>{10, ::hglrt::neg__i64(2), 30};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
}

void results_and_errors_are_traces()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_01_ = ::hglrt::testing::evaluate<::hglrt::result<::hgraph::Int>>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, ::hglrt::neg__i64(1)}, start_));
        ::hgl::core::m_::w_::i_::record::of_1result_1i64_1_1error_1_(wiring_, ::hgl::demo::lifting::m_::w_::returned(wiring_, input_00_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_02_ = ::hglrt::testing::Dense<::hglrt::result<::hgraph::Int>>{::hglrt::result<::hgraph::Int>{::hglrt::result<::hgraph::Int>{10}}, ::hglrt::result<::hgraph::Int>{::hglrt::result<::hgraph::Int>{::std::unexpect, ::hglrt::make_error(::hgraph::Str{"negative input"}, ::hgraph::Str{"numeric.out_of_range"})}}};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_04_ = ::hglrt::testing::evaluate<::hglrt::error>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, ::hglrt::neg__i64(1)}, start_));
        ::hgl::core::m_::w_::i_::record::of_error(wiring_, ::hgl::demo::lifting::m_::w_::errors_of(wiring_, input_03_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_05_ = ::hglrt::testing::Dense<::hglrt::error>{::std::nullopt, ::hglrt::error{::hglrt::make_error(::hgraph::Str{"negative input"}, ::hgraph::Str{"numeric.out_of_range"})}};
        ::hglrt::testing::expect_same(left_04_, right_05_);
    }();
}

void a_connection_of_i64_is_one_of_i64_nullable()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_06_ = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Int>{1, ::std::nullopt, 2};
        const auto trace_02_ = ::hglrt::testing::Dense<::hgraph::Int>{::std::nullopt, 10, ::std::nullopt};
        return ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_02_, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_04_ = input_01_; const auto argument_05_ = input_03_; return ::hgl::demo::lifting::m_::w_::or_zero_of(wiring_, argument_04_, argument_05_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_07_ = ::hglrt::testing::Dense<::hgraph::Int>{1, 11, 12};
        ::hglrt::testing::expect_same(left_06_, right_07_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_09_ = ::hglrt::testing::evaluate<::hgraph::Str>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_08_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 2}, start_));
        ::hgl::core::m_::w_::i_::record::of_str(wiring_, ::hgl::demo::lifting::m_::w_::shown_of(wiring_, input_08_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_010_ = ::hglrt::testing::Dense<::hgraph::Str>{::hgraph::Str{"1"}, ::hgraph::Str{"2"}};
        ::hglrt::testing::expect_same(left_09_, right_010_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_012_ = ::hglrt::testing::evaluate<::hgraph::Str>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_011_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Bool>{true}, start_));
        ::hgl::core::m_::w_::i_::record::of_str(wiring_, ::hgl::demo::lifting::m_::w_::shown_five(wiring_, input_011_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_013_ = ::hglrt::testing::Dense<::hgraph::Str>{::hgraph::Str{"5"}};
        ::hglrt::testing::expect_same(left_012_, right_013_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_017_ = ::hglrt::testing::evaluate<::std::optional<::hgraph::Int>>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_014_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 2}, start_));
        struct held_015_
        {
            static void eval(::hgraph::In<"result", ::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> result_, ::hgraph::DateTime now_)
            {
                if (!::hglrt::fn_modified(result_))
                {
                    return;
                }
                ::std::optional<::std::optional<::hgraph::Int>> whole_;
                try
                {
                    whole_.emplace(::hglrt::nullable_value(result_));
                }
                catch (const ::hglrt::Failure&)
                {
                }
                ::hglrt::testing::hold<::std::optional<::hgraph::Int>>(now_, ::std::move(whole_));
            }
        };
        const auto result_016_ = ::hgl::demo::lifting::m_::w_::widened_result(wiring_, input_014_);
        ::hgraph::wire<held_015_>(wiring_, result_016_);
        ::hgl::core::m_::w_::i_::record::of_1i64_1_(wiring_, result_016_, ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_018_ = ::hglrt::testing::Dense<::std::optional<::hgraph::Int>>{::std::optional<::hgraph::Int>{1}, ::std::optional<::hgraph::Int>{2}};
        ::hglrt::testing::expect_same(left_017_, right_018_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_022_ = ::hglrt::testing::evaluate<::std::optional<::hgraph::Int>>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_019_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 2}, start_));
        struct held_020_
        {
            static void eval(::hgraph::In<"result", ::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> result_, ::hgraph::DateTime now_)
            {
                if (!::hglrt::fn_modified(result_))
                {
                    return;
                }
                ::std::optional<::std::optional<::hgraph::Int>> whole_;
                try
                {
                    whole_.emplace(::hglrt::nullable_value(result_));
                }
                catch (const ::hglrt::Failure&)
                {
                }
                ::hglrt::testing::hold<::std::optional<::hgraph::Int>>(now_, ::std::move(whole_));
            }
        };
        const auto result_021_ = ::hgl::demo::lifting::m_::w_::widened_node_result(wiring_, input_019_);
        ::hgraph::wire<held_020_>(wiring_, result_021_);
        ::hgl::core::m_::w_::i_::record::of_1i64_1_(wiring_, result_021_, ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_023_ = ::hglrt::testing::Dense<::std::optional<::hgraph::Int>>{::std::optional<::hgraph::Int>{1}, ::std::optional<::hgraph::Int>{2}};
        ::hglrt::testing::expect_same(left_022_, right_023_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_025_ = ::hglrt::testing::evaluate<::hgraph::Bool>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_024_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 0, 2}, start_));
        ::hgl::core::m_::w_::i_::record::of_bool(wiring_, ::hgl::demo::lifting::m_::w_::widened_validity(wiring_, input_024_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_026_ = ::hglrt::testing::Dense<::hgraph::Bool>{true, false, true};
        ::hglrt::testing::expect_same(left_025_, right_026_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_038_ = [&]() {
        const auto trace_027_ = ::hglrt::testing::Dense<::hgraph::Bool>{true, false};
        const auto trace_029_ = ::hglrt::testing::Dense<::hgraph::Int>{1, ::std::nullopt};
        const auto trace_031_ = ::hglrt::testing::Dense<::hgraph::Int>{2, ::std::nullopt};
        return ::hglrt::testing::evaluate<::std::optional<::hgraph::Int>>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_028_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(trace_027_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_030_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_029_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_032_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_031_, start_));
        struct held_036_
        {
            static void eval(::hgraph::In<"result", ::hgraph::TS<::hgraph::Optional<::hgraph::Int>>> result_, ::hgraph::DateTime now_)
            {
                if (!::hglrt::fn_modified(result_))
                {
                    return;
                }
                ::std::optional<::std::optional<::hgraph::Int>> whole_;
                try
                {
                    whole_.emplace(::hglrt::nullable_value(result_));
                }
                catch (const ::hglrt::Failure&)
                {
                }
                ::hglrt::testing::hold<::std::optional<::hgraph::Int>>(now_, ::std::move(whole_));
            }
        };
        const auto result_037_ = [&] { const auto argument_033_ = input_028_; const auto argument_034_ = input_030_; const auto argument_035_ = input_032_; return ::hgl::demo::lifting::m_::w_::branch_widened(wiring_, argument_033_, argument_034_, argument_035_); }();
        ::hgraph::wire<held_036_>(wiring_, result_037_);
        ::hgl::core::m_::w_::i_::record::of_1i64_1_(wiring_, result_037_, ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_039_ = ::hglrt::testing::Dense<::std::optional<::hgraph::Int>>{::std::optional<::hgraph::Int>{1}, ::std::optional<::hgraph::Int>{2}};
        ::hglrt::testing::expect_same(left_038_, right_039_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_041_ = ::hglrt::testing::evaluate<::hgraph::Str>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_040_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{3}, start_));
        ::hgl::core::m_::w_::i_::record::of_str(wiring_, ::hgl::demo::lifting::m_::w_::captured_shown(wiring_, input_040_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_042_ = ::hglrt::testing::Dense<::hgraph::Str>{::hgraph::Str{"3"}};
        ::hglrt::testing::expect_same(left_041_, right_042_);
    }();
}

void a_raised_error_keeps_its_provenance()
{
    ::hglrt::testing::statement();
    [[maybe_unused]] const ::hgraph::DateTime first = ::hglrt::add__datetime_duration(::hgl::core::m_::earliest_start(), ::hgraph::TimeDelta{1});
    ::hglrt::testing::statement();
    [&] {
        const auto left_06_ = [&]() {
        const auto trace_00_ = ::hglrt::testing::Dense<::hgraph::Int>{8, ::hglrt::neg__i64(1)};
        const auto trace_02_ = ::hglrt::testing::Dense<::hgraph::Bool>{::std::nullopt, ::std::nullopt, ::std::nullopt, true};
        return ::hglrt::testing::evaluate<::hgraph::DateTime>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(trace_00_, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(trace_02_, start_));
        ::hgl::core::m_::w_::i_::record::of_datetime(wiring_, [&] { const auto argument_04_ = input_01_; const auto argument_05_ = input_03_; return ::hgl::demo::lifting::m_::w_::provenance(wiring_, argument_04_, argument_05_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_07_ = ::hglrt::testing::Dense<::hgraph::DateTime>{::std::nullopt, ::std::nullopt, ::std::nullopt, first};
        ::hglrt::testing::expect_same(left_06_, right_07_);
    }();
}

}  // namespace hgl::demo::lifting::m_::t_
