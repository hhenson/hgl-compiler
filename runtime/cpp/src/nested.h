// A failure and the failures nested in it, as hgraph nests a node's failure
// inside an error of its own (`std::throw_with_nested`). Private to libhglrt.
#pragma once

#include <exception>

namespace hglrt::detail {

/// Calls `visit` with `failure` and then with each failure nested in it,
/// outermost first, until `visit` gives true; gives whether one did. A
/// nested failure that is no `std::exception` ends the walk.
template <class Visit>
bool each_nested(const std::exception& failure, Visit&& visit) {
    if (visit(failure)) {
        return true;
    }
    const auto* nested = dynamic_cast<const std::nested_exception*>(&failure);
    if (nested == nullptr || nested->nested_ptr() == nullptr) {
        return false;
    }
    try {
        std::rethrow_exception(nested->nested_ptr());
    } catch (const std::exception& inner) {
        return each_nested(inner, visit);
    } catch (...) {
        return false;
    }
}

} // namespace hglrt::detail
