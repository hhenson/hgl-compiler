// HGL's execution failures (failure.h).
#include "nested.h"

#include <hglrt/failure.h>

#include <chrono>
#include <format>
#include <string_view>
#include <utility>

namespace hglrt {

error make_error(std::string message, std::optional<std::string> code) {
    return error{.code = std::move(code), .message = std::move(message), .context = std::nullopt};
}

Failure::Failure(hglrt::error failure, std::source_location where)
    : hgraph::ExecutionFailure(std::move(failure)), where_(where) {}

void fail(std::string_view code, std::string message, std::source_location where) {
    throw Failure(make_error(std::move(message), std::string{code}), where);
}

void fail_uncoded(std::string message, std::source_location where) {
    throw Failure(make_error(std::move(message)), where);
}

void raise(error failure, std::source_location where) {
    throw Failure(std::move(failure), where);
}

std::optional<error> carried_error(const std::exception& failure) {
    std::optional<error> found;
    detail::each_nested(failure, [&](const std::exception& part) {
        const auto* carried = dynamic_cast<const hgraph::ExecutionFailure*>(&part);
        if (carried == nullptr) {
            return false;
        }
        found = carried->error();
        return true;
    });
    return found;
}

std::optional<std::source_location> raised_at(const std::exception& failure) {
    std::optional<std::source_location> found;
    detail::each_nested(failure, [&](const std::exception& part) {
        const auto* ours = dynamic_cast<const Failure*>(&part);
        if (ours == nullptr) {
            return false;
        }
        found = ours->where();
        return true;
    });
    return found;
}

std::string describe(const std::exception& failure) {
    const std::optional<error> carried = carried_error(failure);
    if (!carried.has_value()) {
        return failure.what();
    }
    std::string text =
        carried->code.has_value() ? *carried->code + ": " + carried->message : carried->message;
    if (carried->context.has_value()) {
        const hgraph::ErrorContext& context = *carried->context;
        text += std::format(" (in {}, {}, at {:%Y-%m-%dT%H:%M:%S})", context.node, context.phase,
                            context.evaluation_time);
    }
    // The place is an HGL file's where the generated code set one with
    // `#line`; one in this library or in a test's body, which the test host
    // names by its statement, is no place of the program's, and is not said.
    if (const std::optional<std::source_location> where = raised_at(failure);
        where.has_value() && std::string_view{where->file_name()}.ends_with(".hgl")) {
        text += std::format(", raised at {}:{}", where->file_name(), where->line());
    }
    return text;
}

} // namespace hglrt
