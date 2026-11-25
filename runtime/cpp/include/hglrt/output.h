// Core's native process output (OUT-1 and OUT-2).
#pragma once

#include <hglrt/values.h>

namespace hglrt {

/// Core's choice of process stream, in declaration order.
enum class Stream { out, err };

/// The names and declaration order of Core's `Stream` members (TYPE-7).
constexpr EnumNames<Stream, 2> hgl_enum(Stream) noexcept {
    return {"core::Stream", {{Stream::out, "out"}, {Stream::err, "err"}}};
}

/// Write exactly the given bytes to a process stream, optionally flushing it.
void fn_write(const str& text, Stream stream, bool flush);

} // namespace hglrt
