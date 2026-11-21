// Unit tests of the reads checked when they run (checked.h).
#include <hglrt/checked.h>

#include <doctest/doctest.h>

#include <array>
#include <limits>
#include <string>
#include <type_traits>
#include <utility>
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

/// A list made for a read, which ends with the statement it is made in, as
/// a call's result does: `names()[0]`.
hglrt::list<hglrt::str> names() {
    return {"a name long enough that its text is on the heap", "b"};
}

// `item` of a list that ends with its statement gives the element itself,
// and not a reference into the list: a `const` reference the C++ target
// binds to it (a `let`) keeps it. Of a list that outlives the read it gives
// a reference, which an assignment writes through.
using hglrt::i64;
using hglrt::item;
using hglrt::list;
using hglrt::str;
static_assert(std::is_same_v<decltype(item(std::declval<list<str>>(), i64{0})), str>);
static_assert(std::is_same_v<decltype(item(std::declval<const list<str>>(), i64{0})), str>);
static_assert(std::is_same_v<decltype(item(std::declval<std::array<str, 2>>(), i64{0})), str>);
static_assert(std::is_same_v<decltype(item(std::declval<const std::array<str, 2>>(), i64{0})), str>);
static_assert(std::is_same_v<decltype(item(std::declval<const list<str>&>(), i64{0})), const str&>);
static_assert(std::is_same_v<decltype(item(std::declval<list<str>&>(), i64{0})), str&>);
static_assert(std::is_same_v<decltype(item(std::declval<const std::array<str, 2>&>(), i64{0})), const str&>);
static_assert(std::is_same_v<decltype(item(std::declval<std::array<str, 2>&>(), i64{0})), str&>);
// A `list<bool>` is a `std::vector<bool>`, which holds bits: a read gives
// the `bool` itself on every standard library, whatever proxy its vector's
// `operator[]` gives, and a place the vector's own reference to its bit.
static_assert(std::is_same_v<decltype(item(std::declval<const list<bool>&>(), i64{0})), bool>);
static_assert(std::is_same_v<decltype(item(std::declval<list<bool>>(), i64{0})), bool>);
static_assert(std::is_same_v<decltype(item(std::declval<const list<bool>>(), i64{0})), bool>);
static_assert(
    std::is_same_v<decltype(item(std::declval<list<bool>&>(), i64{0})), std::vector<bool>::reference>);

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

TEST_CASE(
    "an index outside a list fails where it is evaluated, coded value.index_out_of_range (EXPR-3, EXPR-7)") {
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
            CHECK(failure.error().code == "value.index_out_of_range");
            CHECK(failure.error().message ==
                  "the index " + std::to_string(outside) + " is outside a list of 3");
        }
    }
    // An assignment's index is checked as a read's is.
    const auto code_of = [](auto&& call) -> std::string {
        try {
            call();
        } catch (const hglrt::Failure& failure) {
            return failure.error().code.value_or("(no code)");
        }
        return "(nothing raised)";
    };
    CHECK(code_of([&] { hglrt::item(values, 3) = 1; }) == "value.index_out_of_range");
    CHECK(code_of([&] { hglrt::item(values, std::numeric_limits<hglrt::i64>::min()) = 1; }) ==
          "value.index_out_of_range");
    CHECK(values == hglrt::list<hglrt::i64>{10, 21, 30});
    // A fixed list, `list<T, N>`, is an ordinary list too.
    std::array<hglrt::i64, 2> fixed{1, 2};
    hglrt::item(fixed, 1) = 5;
    CHECK(fixed[1] == 5);
    CHECK(code_of([&] { (void)hglrt::item(std::as_const(fixed), 2); }) == "value.index_out_of_range");
    CHECK(code_of([&] { hglrt::item(fixed, -1) = 0; }) == "value.index_out_of_range");
    const std::vector<hglrt::str> empty;
    CHECK(code_of([&] { (void)hglrt::item(empty, 0); }) == "value.index_out_of_range");
}

TEST_CASE("an element of a list that ends with its statement outlives it (EXPR-7)") {
    // `let first = names()[0]`: under the address sanitizer a reference
    // into the list would be read after the list is freed.
    const auto& first = hglrt::item(names(), 0);
    CHECK(first == "a name long enough that its text is on the heap");
    const auto& last =
        hglrt::item(std::array<hglrt::str, 2>{"x", "a second name long enough to be on the heap"}, 1);
    CHECK(last == "a second name long enough to be on the heap");
    // A list of bool's element, read and written.
    hglrt::list<bool> flags{true, false};
    const auto& read = hglrt::item(std::as_const(flags), 0);
    CHECK(read);
    hglrt::item(flags, 1) = true;
    CHECK(flags == hglrt::list<bool>{true, true});
    hglrt::item(flags, 0) = false;
    CHECK(read);
    CHECK_FALSE(hglrt::item(hglrt::list<bool>{false}, 0));
    const hglrt::list<bool> fixed{true};
    CHECK(hglrt::item(std::move(fixed), 0));
    // Its index is checked as any other's.
    try {
        (void)hglrt::item(names(), 2);
        FAIL("nothing was raised");
    } catch (const hglrt::Failure& failure) {
        CHECK(failure.error().code == "value.index_out_of_range");
    }
}
