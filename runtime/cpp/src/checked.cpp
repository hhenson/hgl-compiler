// The reads checked when they run (checked.h).
#include <hglrt/checked.h>

#include <cstdint>
#include <format>
#include <utility>

namespace hglrt {

void unset_read(std::string message, std::source_location where) {
    fail(codes::unset_read, std::move(message), where);
}

void detail::check_index(i64 index, std::size_t size, std::source_location where) {
    if (index < 0 || static_cast<std::uint64_t>(index) >= size) {
        fail(codes::index_out_of_range, std::format("the index {} is outside a list of {}", index, size),
             where);
    }
}

} // namespace hglrt
