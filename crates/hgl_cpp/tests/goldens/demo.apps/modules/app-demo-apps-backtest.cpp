// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include "app-demo-apps-backtest.h"
#include "core.h"
#include "demo.apps.h"

// Names this file writes, which no macro of a header above takes.
#undef GlobalState
#undef apps
#undef backtest
#undef bounded
#undef core
#undef demo
#undef global_state
#undef limit
#undef record
#undef value

namespace hgl::demo::apps::backtest::m_
{

// const limit: i64 (UNIT-13), module demo.apps.backtest, line 5
const ::hgraph::Int& limit()
{
    static const ::hgraph::Int value = 2;
    return value;
}

}  // namespace hgl::demo::apps::backtest::m_

namespace hgl::demo::apps::backtest::m_
{

// The run statement of application demo.apps.backtest, line 12: `eval` (UNIT-14, RUN-1, RUN-3).
::hglrt::application::Output application_()
{
    ::hglrt::application::Store store_;
    [[maybe_unused]] const ::hglrt::GlobalState global_state{store_.view()};
    ::hglrt::put_GlobalState_str_V<::hgraph::Int>(global_state, ::hgraph::Str{"limit"}, ::hgl::demo::apps::backtest::m_::limit());
    const ::hglrt::testing::Bounds bounds_ = ::hglrt::testing::Bounds{};
    return ::hglrt::application::run<::hgraph::Int>(store_, [&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::apps::m_::w_::bounded(wiring_, ::hglrt::list<::hgraph::Int>{1, 2, 3}), ::hgraph::Str{::hglrt::testing::run_output});
    }, bounds_, ::hglrt::testing::Mode::simulation);
}

}  // namespace hgl::demo::apps::backtest::m_
