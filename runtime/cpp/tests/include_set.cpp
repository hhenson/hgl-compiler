// The include set of libhglrt's public headers. It is preprocessed and not
// compiled (CMakeLists.txt, `hglrt_include_set`): the test
// `hglrt_takes_no_stdlib` reads every header it brings in for one of
// hgraph's standard library or of Arrow (cmake/check_no_stdlib.cmake).
#include <hglrt/hglrt.h>
#include <hglrt/host.h>
