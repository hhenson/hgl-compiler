// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include "demo.store.quotes.h"
#include "core.h"
#include "demo.store.h"

// Names this file writes, which no macro of a header above takes.
#undef GlobalState
#undef Quote
#undef core
#undef demo
#undef global_state
#undef quotes
#undef record
#undef store

namespace hgl::demo::store::quotes::m_
{

// The run statement of application demo.store.quotes, line 8: `eval` (UNIT-14, RUN-1, RUN-3).
::hglrt::application::Output application_()
{
    ::hglrt::application::Store store_;
    [[maybe_unused]] const ::hglrt::GlobalState global_state{store_.view()};
    ::hglrt::put_GlobalState_str_V<::hgl::demo::store::m_::Quote>(global_state, ::hgraph::Str{"base"}, ::hgl::demo::store::m_::Quote{10, 11});
    const ::hglrt::testing::Bounds bounds_ = ::hglrt::testing::Bounds{};
    return ::hglrt::application::run<::hgl::demo::store::m_::Quote>(store_, [&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        ::hgl::core::m_::w_::i_::record::of_1demo_1store_1_1Quote_(wiring_, ::hgl::demo::store::m_::w_::quotes(wiring_, ::hglrt::list<::hgraph::Int>{1, 2}), ::hgraph::Str{::hglrt::testing::run_output});
    }, bounds_, ::hglrt::testing::Mode::simulation);
}

}  // namespace hgl::demo::store::quotes::m_
