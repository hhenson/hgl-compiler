// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include <hglrt/application_host.h>

// Names this file writes, which no macro of a header above takes.
#undef apps
#undef demo
#undef live

namespace hgl::demo::apps::live::m_
{
::hglrt::application::Output application_();
}  // namespace hgl::demo::apps::live::m_

// The run statement of `demo.apps.live` is `run`: in real time (RUN-3).
int main(int argc, char **argv)
{
    ::hglrt::application::main(argc, argv, true, &::hgl::demo::apps::live::m_::application_);
}
