// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#include <hglrt/application_host.h>

// Names this file writes, which no macro of a header above takes.
#undef demo
#undef failing
#undef store

namespace hgl::demo::store::failing::m_
{
::hglrt::application::Output application_();
}  // namespace hgl::demo::store::failing::m_

// The run statement of `demo.store.failing` is `eval`: a simulation (RUN-3).
int main(int argc, char **argv)
{
    ::hglrt::application::main(argc, argv, false, &::hgl::demo::store::failing::m_::application_);
}
