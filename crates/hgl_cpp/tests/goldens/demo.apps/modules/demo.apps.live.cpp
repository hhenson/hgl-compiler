// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include "demo.apps.live.h"
#include "demo.apps.h"

// Names this file writes, which no macro of a header above takes.
#undef apps
#undef demo
#undef live
#undef quiet

namespace hgl::demo::apps::live::m_
{

// The run statement of application demo.apps.live, line 6: `run` (UNIT-14, RUN-1, RUN-3).
::hglrt::application::Output application_()
{
    ::hglrt::application::Store store_;
    const ::hglrt::testing::Bounds bounds_ = ::hglrt::testing::Bounds{.length = ::hgraph::TimeDelta{200000}};
    return ::hglrt::application::run(store_, [&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        ::hgl::demo::apps::m_::w_::quiet(wiring_, ::hglrt::list<::hgraph::Int>{1, 2});
    }, bounds_, ::hglrt::testing::Mode::real_time);
}

}  // namespace hgl::demo::apps::live::m_
