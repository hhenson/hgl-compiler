// Core's native process output (OUT-1 and OUT-2).
#pragma once

#include <hglrt/values.h>

namespace hglrt {

/// Core's choice of process stream, in declaration order.
enum class Stream { out, err };

/// Write exactly the given bytes to a process stream, optionally flushing it.
void fn_write(const str& text, Stream stream, bool flush);

} // namespace hglrt
