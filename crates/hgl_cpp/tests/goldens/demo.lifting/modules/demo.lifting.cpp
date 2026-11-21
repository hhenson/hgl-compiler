// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include "demo.lifting.h"
#include "core.h"

// Names this file writes, which no macro of a header above takes.
#undef ZeroPolicy
#undef a_capture_keeps_the_child_live
#undef a_default_binds_until_its_input_publishes
#undef a_failure_and_a_returned_error_are_told_apart
#undef a_raised_error_keeps_its_provenance
#undef a_sink_fails_and_a_capture_of_it_publishes_the_error
#undef again
#undef an_earlier_write_stands
#undef an_evals_configuration_is_evaluated_once
#undef attempt
#undef boundary
#undef by
#undef check
#undef checked
#undef checked_errors
#undef code
#undef core
#undef count
#undef demo
#undef describe
#undef earliest_start
#undef error
#undef factor
#undef fail
#undef failure
#undef first
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
#undef original
#undef outcome
#undef outcomes
#undef outer
#undef provenance
#undef quotient
#undef raise_again
#undef raised
#undef record
#undef repeat
#undef repeated
#undef replay
#undef returned
#undef sample
#undef scaled
#undef scaled_by
#undef shifted
#undef shifted_values
#undef taken
#undef tenfold
#undef value
#undef write_then_fail
#undef written_values

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

}  // namespace hgl::demo::lifting::m_

namespace hgl::demo::lifting::m_::t_
{

// The call of `scaled` on line 140, lifted (FN-6).
struct lifted_00_4004_
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
struct lifted_00_4082_
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
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgraph::wire<lifted_00_4004_>(wiring_, input_09_, input_011_, input_013_), ::hgraph::Str{::hglrt::testing::run_output});
    });
    }();
        const auto right_017_ = ::hglrt::testing::Dense<::hgraph::Int>{3, 7};
        ::hglrt::testing::expect_same(left_016_, right_017_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_019_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_018_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 2}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgraph::wire<lifted_00_4082_>(wiring_, input_018_, static_cast<::hgraph::Int>(2), static_cast<::hgraph::Int>(0)), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_020_ = ::hglrt::testing::Dense<::hgraph::Int>{2, 4};
        ::hglrt::testing::expect_same(left_019_, right_020_);
    }();
}

// The call of `multiplied` on line 146, lifted (FN-6).
struct lifted_00_4200_
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
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgraph::wire<lifted_00_4200_>(wiring_, input_01_, static_cast<::hgraph::Int>(argument_02_)), ::hgraph::Str{::hglrt::testing::run_output});
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
