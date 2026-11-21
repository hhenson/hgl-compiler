// The temporal forms of a list and of a tuple (list_endpoints.h).
#include <hglrt/list_endpoints.h>

#include <utility>

namespace hglrt {

void detail::refused_delta(std::string message, std::source_location where) {
    fail_uncoded(std::move(message), where);
}

} // namespace hglrt
