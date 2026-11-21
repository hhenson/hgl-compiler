// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include <hglrt/application_host.h>

// Names this file writes, which no macro of a header above takes.
#undef demo
#undef quotes
#undef store

namespace hgl::demo::store::quotes::m_
{
::hglrt::application::Output application_();
}  // namespace hgl::demo::store::quotes::m_

// The run statement of `demo.store.quotes` is `eval`: a simulation (RUN-3).
int main(int argc, char **argv)
{
    ::hglrt::application::main(argc, argv, false, &::hgl::demo::store::quotes::m_::application_);
}
