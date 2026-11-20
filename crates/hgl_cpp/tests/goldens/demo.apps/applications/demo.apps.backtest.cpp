// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include <hglrt/application_host.h>

// Names this file writes, which no macro of a header above takes.
#undef apps
#undef backtest
#undef demo

namespace hgl::demo::apps::backtest::m_
{
::hglrt::application::Output application_();
}  // namespace hgl::demo::apps::backtest::m_

// The run statement of `demo.apps.backtest` is `eval`: a simulation (RUN-3).
int main(int argc, char **argv)
{
    ::hglrt::application::main(argc, argv, false, &::hgl::demo::apps::backtest::m_::application_);
}
