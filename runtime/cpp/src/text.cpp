// Core's native scalar operators of text (text.h).
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

str mul__str_i64(const str& lhs, i64 rhs) {
    if (rhs < 0) {
        throw std::domain_error("hglrt: core's contract gives no rule for text repeated a count below zero");
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
