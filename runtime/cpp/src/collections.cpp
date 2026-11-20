// HGL's sets, dictionaries and structs as values (collections.h).
#include <hglrt/collections.h>

namespace hglrt {

void detail::missing_key(std::source_location where) {
    fail(codes::missing_key, "the dictionary has no member of the key read", where);
}

} // namespace hglrt
