// Core's native scalar operators of text (text.h).
#include <hglrt/failure.h>
#include <hglrt/text.h>

#include <cstddef>
#include <stdexcept>

namespace hglrt {

str add__str_str(const str& lhs, const str& rhs) {
    str out;
    out.reserve(lhs.size() + rhs.size());
    out += lhs;
    out += rhs;
    return out;
}

str mul__str_i64(const str& lhs, i64 rhs, std::source_location where) {
    if (rhs < 0) {
        fail(codes::out_of_range, "text is repeated a count below zero", where);
    }
    if (lhs.empty() || rhs == 0) {
        return str{};
    }
    const auto count = static_cast<std::size_t>(rhs);
    if (count > str{}.max_size() / lhs.size()) {
        throw std::length_error("hglrt: text repeated beyond what a string holds");
    }
    str out;
    out.reserve(lhs.size() * count);
    for (std::size_t copy = 0; copy < count; ++copy) {
        out += lhs;
    }
    return out;
}

} // namespace hglrt
