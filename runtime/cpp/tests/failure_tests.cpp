// Unit tests of HGL's execution failures (failure.h).
#include <hglrt/failure.h>
#include <hglrt/values.h>

#include <doctest/doctest.h>

#include <chrono>
#include <exception>
#include <stdexcept>
#include <string>

TEST_CASE("an error is equal by its code and message, not its context (ERR-4)") {
    const hglrt::error plain = hglrt::make_error("broken", "value.unset_read");
    CHECK_FALSE(plain.context.has_value());
    hglrt::error placed = plain;
    placed.context =
        hgraph::ErrorContext{"m::f@1", "evaluate", hglrt::datetime{std::chrono::microseconds{2}}};
    CHECK(plain == placed);
    CHECK_FALSE(plain == hglrt::make_error("broken"));
    CHECK_FALSE(plain == hglrt::make_error("other", "value.unset_read"));
}

TEST_CASE("fail raises an ExecutionFailure with its code, message and place (ERR-1)") {
    const unsigned line = __LINE__ + 2;
    try {
        hglrt::fail("numeric.out_of_range", "too far");
    } catch (const hgraph::ExecutionFailure& failure) {
        CHECK(failure.error().code == "numeric.out_of_range");
        CHECK(failure.error().message == "too far");
        CHECK(std::string{failure.what()} == "too far");
        CHECK_FALSE(failure.error().context.has_value());
        const auto* ours = dynamic_cast<const hglrt::Failure*>(&failure);
        REQUIRE(ours != nullptr);
        CHECK(ours->where().line() == line);
    }
    try {
        hglrt::fail_uncoded("no code for this");
    } catch (const hglrt::Failure& failure) {
        CHECK_FALSE(failure.error().code.has_value());
    }
}

TEST_CASE("raise keeps the error's provenance (ERR-5, ERR-8)") {
    hglrt::error retained = hglrt::make_error("first", "convert.invalid");
    retained.context =
        hgraph::ErrorContext{"m::g@4", "evaluate", hglrt::datetime{std::chrono::microseconds{9}}};
    try {
        hglrt::raise(retained);
    } catch (const hglrt::Failure& failure) {
        REQUIRE(failure.error().context.has_value());
        CHECK(failure.error().context->node == "m::g@4");
        CHECK(failure.error() == retained);
    }
}

TEST_CASE("the error a failure nests is found through hgraph's nesting") {
    try {
        try {
            hglrt::fail("value.unset_read", "inner");
        } catch (...) {
            std::throw_with_nested(std::runtime_error("node m::f@3 evaluate failed: inner"));
        }
    } catch (const std::exception& outer) {
        const std::optional<hglrt::error> carried = hglrt::carried_error(outer);
        REQUIRE(carried.has_value());
        CHECK(carried->code == "value.unset_read");
        CHECK(hglrt::raised_at(outer).has_value());
        // Raised here, in no HGL file: the place is not described.
        CHECK(hglrt::describe(outer) == "value.unset_read: inner");
    }
    const std::runtime_error other("not an HGL failure");
    CHECK_FALSE(hglrt::carried_error(other).has_value());
    CHECK(hglrt::describe(other) == "not an HGL failure");
}

/// Fails as the generated code of an HGL statement on line 7 of
/// `demo/places.hgl` fails: defined at the end of this file, which its
/// `#line` renumbers.
void fails_in_places();

TEST_CASE("a failure is described with the HGL file and line it was raised at, and no other place") {
    try {
        fails_in_places();
        FAIL("nothing was raised");
    } catch (const std::exception& failure) {
        CHECK(hglrt::describe(failure) == "value.unset_read: inner, raised at demo/places.hgl:7");
    }
}

TEST_CASE("a failure is described with the context hgraph gave it") {
    hglrt::error given = hglrt::make_error("gone", "numeric.division_by_zero");
    given.context = hgraph::ErrorContext{"m::h@2", "start", hglrt::datetime{std::chrono::microseconds{1}}};
    const hgraph::ExecutionFailure failure{given};
    CHECK(hglrt::describe(failure) ==
          "numeric.division_by_zero: gone (in m::h@2, start, at 1970-01-01T00:00:00.000001)");
}

// The generated code sets the HGL file and line of each statement as this
// does (`#line`): nothing after it in this file is numbered as its own.
void fails_in_places() {
#line 7 "demo/places.hgl"
    hglrt::fail("value.unset_read", "inner");
}
