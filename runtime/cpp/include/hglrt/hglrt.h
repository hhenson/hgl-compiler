// libhglrt, the C++ runtime library of the HGL compiler's C++ target: the one
// header the generated code includes.
//
// The library holds what the generated code calls and hgraph does not have:
// core's native declarations written in C++ (scalar.h, temporal.h, text.h,
// conversions.h, tenors.h, lists.h, endpoints.h, capabilities.h), HGL's failures
// (failure.h), the reads checked when they run (checked.h), constants and
// sources (sources.h) with the coroutine type of a source's body
// (generator.h), the test host (testing.h), and the run of an application
// (application.h). It is built on
// hgraph's runtime and wiring libraries alone: nothing of hgraph's standard
// library is used (docs/cpp-target.md).
#pragma once

#include <hglrt/application.h>
#include <hglrt/capabilities.h>
#include <hglrt/checked.h>
#include <hglrt/conversions.h>
#include <hglrt/endpoints.h>
#include <hglrt/failure.h>
#include <hglrt/generator.h>
#include <hglrt/lists.h>
#include <hglrt/scalar.h>
#include <hglrt/sources.h>
#include <hglrt/temporal.h>
#include <hglrt/tenors.h>
#include <hglrt/testing.h>
#include <hglrt/text.h>
#include <hglrt/values.h>
