// Unit tests of the reads checked when they run (checked.h).
#include <hglrt/checked.h>

#include <doctest/doctest.h>

#include <array>
#include <string>
#include <vector>

namespace {

/// An endpoint as a typed hgraph input view shows one: valid or not, and its
/// value.
struct Endpoint {
    bool is_valid = false;
    hglrt::i64 held = 0;

    [[nodiscard]] bool valid() const { return is_valid; }
    [[nodiscard]] hglrt::i64 value() const { return held; }
};

} // namespace

TEST_CASE("a read of what is not there fails with value.unset_read (TIME-1, ERR-14)") {
    try {
        hglrt::unset_read();
        FAIL("nothing was raised");
    } catch (const hglrt::Failure& failure) {
        CHECK(failure.error().code == "value.unset_read");
        CHECK(failure.error().message == "a part that has not published is read");
    }
    CHECK(hglrt::checked_value(Endpoint{true, 4}) == 4);
    try {
        (void)hglrt::checked_value(Endpoint{false, 4});
        FAIL("nothing was raised");
    } catch (const hglrt::Failure& failure) {
        CHECK(failure.error().code == "value.unset_read");
        CHECK(failure.error().message == "the child read is not valid");
    }
}

TEST_CASE("a checked read where its place takes a result gives one (ERR-14)") {
    const hglrt::result<hglrt::i64> held = hglrt::checked_result(Endpoint{true, 5});
    REQUIRE(held.has_value());
    CHECK(*held == 5);
    const hglrt::result<hglrt::i64> none = hglrt::checked_result(Endpoint{false, 5});
    REQUIRE_FALSE(none.has_value());
    CHECK(none.error().code == "value.unset_read");
    CHECK_FALSE(none.error().context.has_value());
}

TEST_CASE("an index outside a list fails where it is evaluated, with no code (EXPR-3)") {
    hglrt::list<hglrt::i64> values{10, 20, 30};
    CHECK(hglrt::item(values, 0) == 10);
    CHECK(hglrt::item(values, 2) == 30);
    hglrt::item(values, 1) = 21;
    CHECK(values == hglrt::list<hglrt::i64>{10, 21, 30});
    for (const hglrt::i64 outside : {hglrt::i64{-1}, hglrt::i64{3}}) {
        try {
            (void)hglrt::item(values, outside);
            FAIL("nothing was raised");
        } catch (const hglrt::Failure& failure) {
            CHECK_FALSE(failure.error().code.has_value());
            CHECK(failure.error().message ==
                  "the index " + std::to_string(outside) + " is outside a sequence of 3");
        }
    }
    std::array<hglrt::i64, 2> fixed{1, 2};
    hglrt::item(fixed, 1) = 5;
    CHECK(fixed[1] == 5);
    CHECK_THROWS_AS((void)hglrt::item(fixed, 2), hglrt::Failure);
    const std::vector<hglrt::str> empty;
    CHECK_THROWS_AS((void)hglrt::item(empty, 0), hglrt::Failure);
}
