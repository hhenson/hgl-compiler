// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include "demo.lifting.h"
#include "core.h"

namespace hgl::demo::lifting::m_
{

// scaled(value: i64, factor: i64 = ..., offset: i64 = ...) -> i64 (`const fn`), module demo.lifting, line 8
::hgraph::Int scaled([[maybe_unused]] ::hgraph::Int value, [[maybe_unused]] ::hgraph::Int factor, [[maybe_unused]] ::hgraph::Int offset)
{
    return ::hglrt::add__i64_i64(::hglrt::mul__i64_i64(value, factor), offset);
}

namespace n_
{
// scaled_by(value: i64, factor: i64) -> i64 (graph), module demo.lifting, line 13
struct scaled_by
{
    // The call of `scaled` on line 14, lifted (FN-6).
    struct lifted_01_
    {
        static constexpr auto name = "demo.lifting:scaled";
        static void eval([[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::In<"factor", ::hgraph::TS<::hgraph::Int>, ::hgraph::InputValidity::Unchecked> input_00_, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
        {
            [[maybe_unused]] const ::hgraph::Int factor = ::hglrt::fn_valid(input_00_) ? ::hgraph::Int{::hglrt::checked_value(input_00_)} : ::hgraph::Int{2};
            out_.set(::hgl::demo::lifting::m_::scaled(::hglrt::checked_value(value), factor, 0));
            return;
        }
    };

    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> factor)
    {
        return ::hgraph::wire<lifted_01_>(wiring_, value, factor);
    }
};
}  // namespace n_
namespace w_
{
// scaled_by(value: i64, factor: i64) -> i64 (graph), module demo.lifting, line 13
::hgraph::Port<::hgraph::TS<::hgraph::Int>> scaled_by(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> factor)
{
    return ::hgl::demo::lifting::m_::n_::scaled_by::compose(wiring_, value, factor);
}
}  // namespace w_

// check(value: i64) (`const fn`), module demo.lifting, line 19
void check([[maybe_unused]] ::hgraph::Int value)
{
    [[maybe_unused]] const ::hgraph::Int quotient = ::hglrt::floordiv__i64_i64<::hglrt::ZeroPolicy::fail>(1, value);
}

namespace n_
{
// checked(value: i64) (graph), module demo.lifting, line 23
struct checked
{
    // The call of `check` on line 24, lifted (FN-6).
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
}  // namespace n_
namespace w_
{
// checked(value: i64) (graph), module demo.lifting, line 23
void checked(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    ::hgl::demo::lifting::m_::n_::checked::compose(wiring_, value);
}
}  // namespace w_

namespace n_
{
// describe(failure: error, sample: i64) -> str (node), module demo.lifting, line 28
struct describe
{
    static constexpr auto name = "demo.lifting::describe";

    static void eval([[maybe_unused]] ::hgraph::In<"failure", ::hgraph::TS<::hglrt::error>, ::hgraph::InputActivity::Passive, ::hgraph::InputValidity::Unchecked> failure, [[maybe_unused]] ::hgraph::In<"sample", ::hgraph::TS<::hgraph::Int>, ::hgraph::InputValidity::Unchecked> sample, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Str>> out_)
    {
        // when, line 29 (TIME-3)
        if (::hglrt::fn_modified(sample))
        {
            if (::hglrt::fn_valid(failure))
            {
                [[maybe_unused]] const std::optional<::hgraph::Str> code = (::hglrt::checked_value(failure)).code;
                if ((!(code).has_value()))
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
}  // namespace n_
namespace w_
{
// describe(failure: error, sample: i64) -> str (node), module demo.lifting, line 28
::hgraph::Port<::hgraph::TS<::hgraph::Str>> describe(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hglrt::error>> failure, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> sample)
{
    return ::hgraph::wire<::hgl::demo::lifting::m_::n_::describe>(wiring_, failure, sample);
}
}  // namespace w_

namespace n_
{
// checked_errors(value: i64) -> str (graph), module demo.lifting, line 40
struct checked_errors
{
    // The call of `check` on line 41, lifted (FN-6).
    struct lifted_01_
    {
        static constexpr auto name = "demo.lifting:check";
        static void eval([[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>> value)
        {
            ::hgl::demo::lifting::m_::check(::hglrt::checked_value(value));
        }
    };

    // The `capture` of `check` on line 41 (ERR-7): the graph hgraph's
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
}  // namespace n_
namespace w_
{
// checked_errors(value: i64) -> str (graph), module demo.lifting, line 40
::hgraph::Port<::hgraph::TS<::hgraph::Str>> checked_errors(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgl::demo::lifting::m_::n_::checked_errors::compose(wiring_, value);
}
}  // namespace w_

// attempt(value: i64) -> result<i64, error> (`const fn`), module demo.lifting, line 47
::hglrt::result<::hgraph::Int> attempt([[maybe_unused]] ::hgraph::Int value)
{
    if (::hglrt::lt__T_T<::hgraph::Int>(value, 0))
    {
        return ::hglrt::result<::hgraph::Int>{std::unexpect, ::hglrt::make_error(::hgraph::Str{"negative input"}, ::hgraph::Str{"numeric.out_of_range"})};
    }
    return ::hglrt::result<::hgraph::Int>{::hglrt::mul__i64_i64(value, 10)};
}

namespace n_
{
// raised(value: i64) -> i64 (node), module demo.lifting, line 52
struct raised
{
    static constexpr auto name = "demo.lifting::raised";

    static void eval([[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 53 (TIME-3)
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
}  // namespace n_
namespace w_
{
// raised(value: i64) -> i64 (node), module demo.lifting, line 52
::hgraph::Port<::hgraph::TS<::hgraph::Int>> raised(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgraph::wire<::hgl::demo::lifting::m_::n_::raised>(wiring_, value);
}
}  // namespace w_

namespace n_
{
// returned(value: i64) -> result<i64, error> (node), module demo.lifting, line 56
struct returned
{
    static constexpr auto name = "demo.lifting::returned";

    static void eval([[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hglrt::result<::hgraph::Int>>> out_)
    {
        // when, line 57 (TIME-3)
        {
            out_.set(::hgl::demo::lifting::m_::attempt(::hglrt::checked_value(value)));
            return;
        }
    }
};
}  // namespace n_
namespace w_
{
// returned(value: i64) -> result<i64, error> (node), module demo.lifting, line 56
::hgraph::Port<::hgraph::TS<::hglrt::result<::hgraph::Int>>> returned(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgraph::wire<::hgl::demo::lifting::m_::n_::returned>(wiring_, value);
}
}  // namespace w_

namespace n_
{
// outcome(taken: result<i64, error>) -> str (node), module demo.lifting, line 60
struct outcome
{
    static constexpr auto name = "demo.lifting::outcome";

    static void eval([[maybe_unused]] ::hgraph::In<"taken", ::hgraph::TS<::hglrt::result<::hgraph::Int>>> taken, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Str>> out_)
    {
        // when, line 61 (TIME-3)
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
}  // namespace n_
namespace w_
{
// outcome(taken: result<i64, error>) -> str (node), module demo.lifting, line 60
::hgraph::Port<::hgraph::TS<::hgraph::Str>> outcome(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hglrt::result<::hgraph::Int>>> taken)
{
    return ::hgraph::wire<::hgl::demo::lifting::m_::n_::outcome>(wiring_, taken);
}
}  // namespace w_

namespace n_
{
// outcomes(value: i64) -> str (graph), module demo.lifting, line 67
struct outcomes
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Str>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
    {
        return ::hgl::demo::lifting::m_::w_::outcome(wiring_, ::hgl::demo::lifting::m_::w_::returned(wiring_, value));
    }
};
}  // namespace n_
namespace w_
{
// outcomes(value: i64) -> str (graph), module demo.lifting, line 67
::hgraph::Port<::hgraph::TS<::hgraph::Str>> outcomes(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgl::demo::lifting::m_::n_::outcomes::compose(wiring_, value);
}
}  // namespace w_

namespace n_
{
// shifted(value: i64, const by: i64) -> i64 (node), module demo.lifting, line 72
struct shifted
{
    static constexpr auto name = "demo.lifting::shifted";

    static void eval([[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::Scalar<"by", ::hgraph::Int> by, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 73 (TIME-3)
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
}  // namespace n_
namespace w_
{
// shifted(value: i64, const by: i64) -> i64 (node), module demo.lifting, line 72
::hgraph::Port<::hgraph::TS<::hgraph::Int>> shifted(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value, ::hgraph::Int by)
{
    return ::hgraph::wire<::hgl::demo::lifting::m_::n_::shifted>(wiring_, value, by);
}
}  // namespace w_

namespace n_
{
// shifted_values(value: i64) -> i64 (graph), module demo.lifting, line 76
struct shifted_values
{
    // The `capture` of `shifted` on line 77 (ERR-7): the graph hgraph's
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
}  // namespace n_
namespace w_
{
// shifted_values(value: i64) -> i64 (graph), module demo.lifting, line 76
::hgraph::Port<::hgraph::TS<::hgraph::Int>> shifted_values(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgl::demo::lifting::m_::n_::shifted_values::compose(wiring_, value);
}
}  // namespace w_

namespace n_
{
// guarded(value: i64) -> i64 (graph), module demo.lifting, line 81
struct guarded
{
    static ::hgraph::Port<::hgraph::TS<::hgraph::Int>> compose(::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
    {
        return ::hgl::demo::lifting::m_::w_::raised(wiring_, value);
    }
};
}  // namespace n_
namespace w_
{
// guarded(value: i64) -> i64 (graph), module demo.lifting, line 81
::hgraph::Port<::hgraph::TS<::hgraph::Int>> guarded(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgl::demo::lifting::m_::n_::guarded::compose(wiring_, value);
}
}  // namespace w_

namespace n_
{
// guarded_values(value: i64) -> i64 (graph), module demo.lifting, line 85
struct guarded_values
{
    // The `capture` of `guarded` on line 86 (ERR-7): the graph hgraph's
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
}  // namespace n_
namespace w_
{
// guarded_values(value: i64) -> i64 (graph), module demo.lifting, line 85
::hgraph::Port<::hgraph::TS<::hgraph::Int>> guarded_values(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgl::demo::lifting::m_::n_::guarded_values::compose(wiring_, value);
}
}  // namespace w_

namespace n_
{
// write_then_fail(value: i64) -> i64 (node), module demo.lifting, line 91
struct write_then_fail
{
    static constexpr auto name = "demo.lifting::write_then_fail";

    static void eval([[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        // when, line 93 (TIME-3)
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
}  // namespace n_
namespace w_
{
// write_then_fail(value: i64) -> i64 (node), module demo.lifting, line 91
::hgraph::Port<::hgraph::TS<::hgraph::Int>> write_then_fail(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgraph::wire<::hgl::demo::lifting::m_::n_::write_then_fail>(wiring_, value);
}
}  // namespace w_

namespace n_
{
// written_values(value: i64) -> i64 (graph), module demo.lifting, line 100
struct written_values
{
    // The `capture` of `write_then_fail` on line 101 (ERR-7): the graph hgraph's
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
}  // namespace n_
namespace w_
{
// written_values(value: i64) -> i64 (graph), module demo.lifting, line 100
::hgraph::Port<::hgraph::TS<::hgraph::Int>> written_values(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value)
{
    return ::hgl::demo::lifting::m_::n_::written_values::compose(wiring_, value);
}
}  // namespace w_

namespace n_
{
// raise_again(first: error, repeat: bool) -> error (node), module demo.lifting, line 107
struct raise_again
{
    static constexpr auto name = "demo.lifting::raise_again";

    static void eval([[maybe_unused]] ::hgraph::In<"first", ::hgraph::TS<::hglrt::error>> first, [[maybe_unused]] ::hgraph::In<"repeat", ::hgraph::TS<::hgraph::Bool>, ::hgraph::InputValidity::Unchecked> repeat, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hglrt::error>> out_)
    {
        // when, line 108 (TIME-3)
        if (::hglrt::fn_modified(first))
        {
            out_.set(::hglrt::checked_value(first));
            return;
        }
        // when, line 109 (TIME-3)
        if (::hglrt::fn_modified(repeat))
        {
            [[maybe_unused]] const ::hglrt::result<::hglrt::error> failure = ::hglrt::result<::hglrt::error>{std::unexpect, ::hglrt::checked_value(first)};
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
}  // namespace n_
namespace w_
{
// raise_again(first: error, repeat: bool) -> error (node), module demo.lifting, line 107
::hgraph::Port<::hgraph::TS<::hglrt::error>> raise_again(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hglrt::error>> first, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> repeat)
{
    return ::hgraph::wire<::hgl::demo::lifting::m_::n_::raise_again>(wiring_, first, repeat);
}
}  // namespace w_

namespace n_
{
// handled(value: i64, repeat: bool) -> error (graph), module demo.lifting, line 115
struct handled
{
    // The `capture` of `raised` on line 116 (ERR-7): the graph hgraph's
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
}  // namespace n_
namespace w_
{
// handled(value: i64, repeat: bool) -> error (graph), module demo.lifting, line 115
::hgraph::Port<::hgraph::TS<::hglrt::error>> handled(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> repeat)
{
    return ::hgl::demo::lifting::m_::n_::handled::compose(wiring_, value, repeat);
}
}  // namespace w_

namespace n_
{
// kept_time(first: error, repeated: error) -> datetime (node), module demo.lifting, line 120
struct kept_time
{
    static constexpr auto name = "demo.lifting::kept_time";

    static void eval([[maybe_unused]] ::hgraph::In<"first", ::hgraph::TS<::hglrt::error>, ::hgraph::InputActivity::Passive> first, [[maybe_unused]] ::hgraph::In<"repeated", ::hgraph::TS<::hglrt::error>> repeated, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::DateTime>> out_)
    {
        // when, line 121 (TIME-3)
        if (::hglrt::fn_modified(repeated))
        {
            [[maybe_unused]] const std::optional<::hgraph::ErrorContext> original = (::hglrt::checked_value(first)).context;
            [[maybe_unused]] const std::optional<::hgraph::ErrorContext> again = (::hglrt::checked_value(repeated)).context;
            if (((!(original).has_value()) || (!(again).has_value())))
            {
                out_.set(::hgl::core::m_::never());
                return;
            }
            if (([&] { const auto argument_00_ = ((*original)).node; const auto argument_01_ = ((*again)).node; return ::hglrt::ne__T_T<::hgraph::Str>(argument_00_, argument_01_); }() || [&] { const auto argument_02_ = ((*original)).phase; const auto argument_03_ = ((*again)).phase; return ::hglrt::ne__T_T<::hgraph::Str>(argument_02_, argument_03_); }()))
            {
                out_.set(::hgl::core::m_::never());
                return;
            }
            out_.set(((*again)).evaluation_time);
            return;
        }
    }
};
}  // namespace n_
namespace w_
{
// kept_time(first: error, repeated: error) -> datetime (node), module demo.lifting, line 120
::hgraph::Port<::hgraph::TS<::hgraph::DateTime>> kept_time(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hglrt::error>> first, ::hgraph::Port<::hgraph::TS<::hglrt::error>> repeated)
{
    return ::hgraph::wire<::hgl::demo::lifting::m_::n_::kept_time>(wiring_, first, repeated);
}
}  // namespace w_

namespace n_
{
// provenance(value: i64, repeat: bool) -> datetime (graph), module demo.lifting, line 130
struct provenance
{
    // The `capture` of `handled` on line 131 (ERR-7): the graph hgraph's
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
}  // namespace n_
namespace w_
{
// provenance(value: i64, repeat: bool) -> datetime (graph), module demo.lifting, line 130
::hgraph::Port<::hgraph::TS<::hgraph::DateTime>> provenance(::hgraph::Wiring& wiring_, ::hgraph::Port<::hgraph::TS<::hgraph::Int>> value, ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> repeat)
{
    return ::hgl::demo::lifting::m_::n_::provenance::compose(wiring_, value, repeat);
}
}  // namespace w_

}  // namespace hgl::demo::lifting::m_

namespace hgl::demo::lifting::m_::t_
{

// The call of `scaled` on line 137, lifted (FN-6).
struct lifted_00_3862_
{
    static constexpr auto name = "demo.lifting:scaled";
    static void eval([[maybe_unused]] ::hgraph::In<"value", ::hgraph::TS<::hgraph::Int>> value, [[maybe_unused]] ::hgraph::In<"factor", ::hgraph::TS<::hgraph::Int>, ::hgraph::InputValidity::Unchecked> input_09_, [[maybe_unused]] ::hgraph::In<"offset", ::hgraph::TS<::hgraph::Int>, ::hgraph::InputValidity::Unchecked> input_010_, [[maybe_unused]] ::hgraph::Out<::hgraph::TS<::hgraph::Int>> out_)
    {
        [[maybe_unused]] const ::hgraph::Int factor = ::hglrt::fn_valid(input_09_) ? ::hgraph::Int{::hglrt::checked_value(input_09_)} : ::hgraph::Int{2};
        [[maybe_unused]] const ::hgraph::Int offset = ::hglrt::fn_valid(input_010_) ? ::hgraph::Int{::hglrt::checked_value(input_010_)} : ::hgraph::Int{0};
        out_.set(::hgl::demo::lifting::m_::scaled(::hglrt::checked_value(value), factor, offset));
        return;
    }
};

void a_default_binds_until_its_input_publishes()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_04_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, std::nullopt, 3}, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{std::nullopt, 4, std::nullopt}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, [&] { const auto argument_02_ = input_00_; const auto argument_03_ = input_01_; return ::hgl::demo::lifting::m_::w_::scaled_by(wiring_, argument_02_, argument_03_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_05_ = ::hglrt::testing::Dense<::hgraph::Int>{2, 4, 12};
        ::hglrt::testing::expect_same(left_04_, right_05_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_011_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_06_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, 2}, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_07_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{std::nullopt, 3}, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_08_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgraph::wire<lifted_00_3862_>(wiring_, input_06_, input_07_, input_08_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_012_ = ::hglrt::testing::Dense<::hgraph::Int>{3, 7};
        ::hglrt::testing::expect_same(left_011_, right_012_);
    }();
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
        const auto right_02_ = ::hglrt::testing::Dense<::hgraph::Int>{15, std::nullopt, 25};
        ::hglrt::testing::expect_same(left_01_, right_02_);
    }();
    ::hglrt::testing::statement();
    [&] {
        const auto left_04_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_03_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{1, ::hglrt::neg__i64(1), 2}, start_));
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::lifting::m_::w_::guarded_values(wiring_, input_03_), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_05_ = ::hglrt::testing::Dense<::hgraph::Int>{10, std::nullopt, 20};
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
        const auto left_04_ = ::hglrt::testing::evaluate<::hgraph::DateTime>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        const ::hgraph::Port<::hgraph::TS<::hgraph::Int>> input_00_ = ::hgl::core::m_::w_::i_::replay::of_i64(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Int>{8, ::hglrt::neg__i64(1)}, start_));
        const ::hgraph::Port<::hgraph::TS<::hgraph::Bool>> input_01_ = ::hgl::core::m_::w_::i_::replay::of_bool(wiring_, ::hglrt::testing::replay_items(::hglrt::testing::Dense<::hgraph::Bool>{std::nullopt, std::nullopt, std::nullopt, true}, start_));
        ::hgl::core::m_::w_::i_::record::of_datetime(wiring_, [&] { const auto argument_02_ = input_00_; const auto argument_03_ = input_01_; return ::hgl::demo::lifting::m_::w_::provenance(wiring_, argument_02_, argument_03_); }(), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_05_ = ::hglrt::testing::Dense<::hgraph::DateTime>{std::nullopt, std::nullopt, std::nullopt, first};
        ::hglrt::testing::expect_same(left_04_, right_05_);
    }();
}

}  // namespace hgl::demo::lifting::m_::t_
