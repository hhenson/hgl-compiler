// libhglrt, the C++ runtime library of the HGL compiler's C++ target: the one
// header the generated code includes.
//
// The library holds what the generated code calls and hgraph does not have:
// core's native declarations written in C++ (scalar.h, temporal.h, text.h,
// conversions.h, tenors.h, lists.h, endpoints.h, capabilities.h, output.h), HGL's failures
// (failure.h), a capture and the temporal values of errors and results
// (capture.h), sets, dictionaries and what a struct tells the library
// (collections.h), the temporal forms of a nullable scalar (nullables.h), of
// a struct and a tuple (structs.h), of a set and a dictionary (keyed.h) and of
// a list (list_endpoints.h),
// the reads checked when they run (checked.h), constants and sources
// (sources.h) with the coroutine type of a source's body (generator.h), what
// tells a node's wakes apart and what a source's body is lent (timing.h),
// windows (windows.h), what a `ref` parameter designates (references.h), the
// branches control flow over a connection wires where the language gives
// none (control.h), the test host (testing.h), and the run of an application
// (application.h). It is built on hgraph's runtime and wiring libraries
// alone: nothing of hgraph's standard library is used (docs/cpp-target.md).
#pragma once

#include <hglrt/application.h>
#include <hglrt/capabilities.h>
#include <hglrt/capture.h>
#include <hglrt/checked.h>
#include <hglrt/collections.h>
#include <hglrt/control.h>
#include <hglrt/conversions.h>
#include <hglrt/endpoints.h>
#include <hglrt/failure.h>
#include <hglrt/generator.h>
#include <hglrt/keyed.h>
#include <hglrt/key_sets.h>
#include <hglrt/list_endpoints.h>
#include <hglrt/lists.h>
#include <hglrt/nullables.h>
#include <hglrt/output.h>
#include <hglrt/references.h>
#include <hglrt/scalar.h>
#include <hglrt/sources.h>
#include <hglrt/structs.h>
#include <hglrt/temporal.h>
#include <hglrt/tenors.h>
#include <hglrt/testing.h>
#include <hglrt/text.h>
#include <hglrt/timing.h>
#include <hglrt/values.h>
#include <hglrt/windows.h>
