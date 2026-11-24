// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include "mod-demo-apps.h"
#include "mod-core.h"

// Names this file writes, which no macro of a header above takes.
#undef apps
#undef core
#undef demo
#undef record
#undef seeded
#undef the_store_is_set_where_the_graph_is_built

namespace hgl::demo::apps::m_::t_
{

void the_store_is_set_where_the_graph_is_built()
{
    ::hglrt::testing::statement();
    [&] {
        const auto left_00_ = ::hglrt::testing::evaluate<::hgraph::Int>([&](::hgraph::Wiring& wiring_, [[maybe_unused]] ::hgraph::DateTime start_) {
        ::hgl::core::m_::w_::i_::record::of_i64(wiring_, ::hgl::demo::apps::m_::w_::seeded(wiring_, 2, ::hglrt::list<::hgraph::Int>{1, 2, 3}), ::hgraph::Str{::hglrt::testing::run_output});
    });
        const auto right_01_ = ::hglrt::testing::Timed<::hgraph::Int>{{{::hglrt::testing::TraceKey{::hgraph::TimeDelta{2000000}}, 1}, {::hglrt::testing::TraceKey{::hgraph::TimeDelta{4000000}}, 2}}};
        ::hglrt::testing::expect_same(left_00_, right_01_);
    }();
}

}  // namespace hgl::demo::apps::m_::t_
