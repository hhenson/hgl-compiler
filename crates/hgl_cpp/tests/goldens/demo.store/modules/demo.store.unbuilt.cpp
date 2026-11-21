// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include "demo.store.unbuilt.h"
#include "core.h"
#include "demo.store.h"

// Names this file writes, which no macro of a header above takes.
#undef core
#undef demo
#undef read_at_build
#undef record
#undef store
#undef unbuilt

namespace hgl::demo::store::unbuilt::m_
{

// The run statement of application demo.store.unbuilt, line 6: `eval` (UNIT-14, RUN-1, RUN-3).
::hglrt::application::Output application_()
{
    ::hglrt::application::Store store_;
    const ::hglrt::testing::Bounds bounds_ = ::hglrt::testing::Bounds{};
    return ::hglrt::application::run<::hgraph::Int>(store_, [&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::store::m_::w_::read_at_build(wiring_, ::hglrt::list<::hgraph::Int>{1, 2}), ::hgraph::Str{::hglrt::testing::run_output});
    }, bounds_, ::hglrt::testing::Mode::simulation);
}

}  // namespace hgl::demo::store::unbuilt::m_
