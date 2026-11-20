// Unit tests of core's native scalar operators (scalar.h), against the
// observations core's numeric contract lists (`spec/numeric-operators.md`).
#include <hglrt/scalar.h>

#include <doctest/doctest.h>

#include <chrono>
#include <cmath>
#include <limits>
#include <optional>
#include <string>

namespace {

using hglrt::f64;
using hglrt::i64;
using hglrt::ZeroPolicy;

constexpr i64 most = std::numeric_limits<i64>::max();
constexpr i64 least = std::numeric_limits<i64>::min();
constexpr f64 infinity = std::numeric_limits<f64>::infinity();
const f64 not_a_number = std::numeric_limits<f64>::quiet_NaN();

/// The code of the HGL failure `call` raises, or none where it raises none.
template <class Call>
std::optional<std::string> code_raised(Call&& call) {
    try {
        call();
    } catch (const hglrt::Failure& failure) {
        return failure.error().code.value_or("(no code)");
    }
    return std::nullopt;
}

} // namespace

TEST_CASE("i64 arithmetic is checked (NUM-2)") {
    CHECK(hglrt::add__i64_i64(2, 3) == 5);
    CHECK(hglrt::sub__i64_i64(2, 3) == -1);
    CHECK(hglrt::mul__i64_i64(-4, 3) == -12);
    CHECK(hglrt::neg__i64(7) == -7);
    CHECK(hglrt::add__i64_i64(most - 1, 1) == most);
    // "Maximum i64 plus one; negation of minimum i64": numeric.out_of_range.
    CHECK(code_raised([] { (void)hglrt::add__i64_i64(most, 1); }) == "numeric.out_of_range");
    CHECK(code_raised([] { (void)hglrt::neg__i64(least); }) == "numeric.out_of_range");
    CHECK(code_raised([] { (void)hglrt::sub__i64_i64(least, 1); }) == "numeric.out_of_range");
    CHECK(code_raised([] { (void)hglrt::mul__i64_i64(most / 2 + 1, 2); }) == "numeric.out_of_range");
    CHECK(code_raised([] { (void)hglrt::mul__i64_i64(least, -1); }) == "numeric.out_of_range");
}

TEST_CASE("a failure carries where it was raised") {
    const int line = __LINE__ + 2;
    try {
        (void)hglrt::add__i64_i64(most, 1);
        FAIL("nothing was raised");
    } catch (const hglrt::Failure& failure) {
        CHECK(failure.where().line() == static_cast<unsigned>(line));
        CHECK(std::string{failure.where().file_name()}.ends_with("scalar_tests.cpp"));
        CHECK(failure.error().message == "an i64 result is outside the signed 64-bit range");
    }
}

TEST_CASE("i64 floor division and modulo (NUM-1, NUM-12)") {
    // "`-7 // 3`; `-7 % 3`": `-3`; `2`.
    CHECK(hglrt::floordiv__i64_i64(-7, 3) == -3);
    CHECK(hglrt::mod__i64_i64(-7, 3) == 2);
    CHECK(hglrt::floordiv__i64_i64(7, -2) == -4);
    CHECK(hglrt::mod__i64_i64(7, -2) == -1);
    CHECK(hglrt::floordiv__i64_i64(6, 3) == 2);
    CHECK(hglrt::mod__i64_i64(6, -3) == 0);
    CHECK(hglrt::mod__i64_i64(least, -1) == 0);
    CHECK(code_raised([] { (void)hglrt::floordiv__i64_i64(least, -1); }) == "numeric.out_of_range");
    // "`7 // 0`; `7 % 0`": numeric.division_by_zero.
    CHECK(code_raised([] { (void)hglrt::floordiv__i64_i64(7, 0); }) == "numeric.division_by_zero");
    CHECK(code_raised([] { (void)hglrt::mod__i64_i64(7, 0); }) == "numeric.division_by_zero");
    // "`7 //{zero: .error} 2`; `7 //{zero: .error} 0`": `ok(3)`; an Err.
    const hglrt::result<i64> three = hglrt::floordiv__i64_i64<ZeroPolicy::error>(7, 2);
    REQUIRE(three.has_value());
    CHECK(*three == 3);
    const hglrt::result<i64> none = hglrt::floordiv__i64_i64<ZeroPolicy::error>(7, 0);
    REQUIRE_FALSE(none.has_value());
    CHECK(none.error().code == "numeric.division_by_zero");
    CHECK(hglrt::mod__i64_i64<ZeroPolicy::error>(7, 0).error().code == "numeric.division_by_zero");
}

TEST_CASE("f64 division under each zero policy (NUM-3, NUM-9)") {
    // "`1.0 / 0.0`; `1.0 / -0.0`; `0.0 / 0.0`": +inf; -inf; NaN.
    CHECK(hglrt::div__f64_f64(1.0, 0.0) == infinity);
    CHECK(hglrt::div__f64_f64(1.0, -0.0) == -infinity);
    CHECK(hglrt::div__f64_f64(-1.0, -0.0) == infinity);
    CHECK(std::isnan(hglrt::div__f64_f64(0.0, 0.0)));
    CHECK(std::isnan(hglrt::div__f64_f64(not_a_number, 0.0)));
    CHECK(hglrt::div__f64_f64(7.0, 2.0) == 3.5);
    // "`1.0 /{zero: .not_a_number} 0.0`": NaN.
    CHECK(std::isnan(hglrt::div__f64_f64<ZeroPolicy::nan>(1.0, 0.0)));
    // "`1.0 /{zero: .fail} 0.0`": numeric.division_by_zero.
    CHECK(code_raised([] { (void)hglrt::div__f64_f64<ZeroPolicy::fail>(1.0, 0.0); }) ==
          "numeric.division_by_zero");
    // "`6.0 /{zero: .error} 2.0`": `ok(3.0)`; a zero divisor of either sign an Err.
    CHECK(*hglrt::div__f64_f64<ZeroPolicy::error>(6.0, 2.0) == 3.0);
    CHECK(hglrt::div__f64_f64<ZeroPolicy::error>(1.0, 0.0).error().code == "numeric.division_by_zero");
    CHECK(hglrt::div__f64_f64<ZeroPolicy::error>(0.0, -0.0).error().code == "numeric.division_by_zero");
    // "NaN divided by a nonzero value with `zero: .error`": Ok containing NaN.
    CHECK(std::isnan(*hglrt::div__f64_f64<ZeroPolicy::error>(not_a_number, 2.0)));
    CHECK(hglrt::div__f64_f64<ZeroPolicy::error>(not_a_number, 0.0).error().code ==
          "numeric.division_by_zero");
}

TEST_CASE("i64 true division is the nearest f64 of the exact quotient (NUM-1, NUM-2)") {
    // "`7 / 2`": 3.5.
    CHECK(hglrt::div__i64_i64(7, 2) == 3.5);
    CHECK(hglrt::div__i64_i64(-7, 2) == -3.5);
    CHECK(hglrt::div__i64_i64(1, 3) == 0x1.5555555555555p-2);
    CHECK(hglrt::div__i64_i64(9007199254740993, 1) == 9007199254740992.0);
    CHECK(hglrt::div__i64_i64(9007199254740993, 3) == 0x1.5555555555556p+51);
    // Quotients that rounding each operand first would give one step off.
    CHECK(hglrt::div__i64_i64(-1491621378785028714, -1086441597939901421) == 0x1.5f7921b357bd3p+0);
    CHECK(hglrt::div__i64_i64(5161945549082021012, -5298123596289569600) == -0x1.f2d70c0ce6109p-1);
    CHECK(hglrt::div__i64_i64(7992424660898182485, 361623101239487594) == 0x1.619fe05b81482p+4);
    CHECK(hglrt::div__i64_i64(least, -1) == 9223372036854775808.0);
    CHECK(hglrt::div__i64_i64(1, least) == -0x1p-63);
    CHECK(std::signbit(hglrt::div__i64_i64(0, -5)));
    CHECK(hglrt::div__i64_i64(3, 0) == infinity);
    CHECK(std::isnan(hglrt::div__i64_i64(0, 0)));
    CHECK(hglrt::div__i64_i64<ZeroPolicy::error>(3, 0).error().code == "numeric.division_by_zero");
}

TEST_CASE("f64 arithmetic keeps IEEE special values (NUM-2)") {
    CHECK(hglrt::add__f64_f64(0.1, 0.2) == 0.1 + 0.2);
    CHECK(hglrt::sub__f64_f64(1.0, 3.0) == -2.0);
    CHECK(hglrt::mul__f64_f64(1e308, 10.0) == infinity);
    CHECK(std::signbit(hglrt::neg__f64(0.0)));
    CHECK(std::isnan(hglrt::neg__f64(not_a_number)));
}

TEST_CASE("an instant moved by a duration stays in the years 1 to 9999 (OPS-10)") {
    using namespace std::chrono;
    const hglrt::datetime last = sys_days{year{9999} / 12 / 31} + hours{24} - microseconds{1};
    CHECK(hglrt::add__datetime_duration(last - microseconds{5}, microseconds{5}) == last);
    CHECK(code_raised([&] { (void)hglrt::add__datetime_duration(last, microseconds{1}); }) ==
          "numeric.out_of_range");
    const hglrt::datetime first = sys_days{year{1} / 1 / 1};
    CHECK(code_raised([&] { (void)hglrt::add__datetime_duration(first, -microseconds{1}); }) ==
          "numeric.out_of_range");
    CHECK(code_raised([] {
              (void)hglrt::add__datetime_duration(hglrt::datetime{microseconds{1}},
                                                  microseconds{std::numeric_limits<i64>::max()});
          }) == "numeric.out_of_range");
}

TEST_CASE("plain numeric equality is exact (NUM-2, NUM-4, NUM-5)") {
    CHECK(hglrt::eq__i64_i64_f64_f64(3, 3));
    CHECK(hglrt::ne__i64_i64_f64_f64(3, 4));
    // "`1.0 == 1.005`": false.
    CHECK_FALSE(hglrt::eq__f64_f64_f64_f64(1.0, 1.005));
    // "NaN compared with itself by `==` and `!=`": false; true.
    CHECK_FALSE(hglrt::eq__f64_f64_f64_f64(not_a_number, not_a_number));
    CHECK(hglrt::ne__f64_f64_f64_f64(not_a_number, not_a_number));
    // "opposite signed zeros are equal".
    CHECK(hglrt::eq__f64_f64_f64_f64(0.0, -0.0));
    CHECK(hglrt::eq__f64_f64_f64_f64(infinity, infinity));
    CHECK_FALSE(hglrt::eq__f64_f64_f64_f64(infinity, -infinity));
    // "`9007199254740993 == 9007199254740992.0`": false.
    CHECK_FALSE(hglrt::eq__i64_f64_f64_f64(9007199254740993, 9007199254740992.0));
    CHECK(hglrt::eq__i64_f64_f64_f64(9007199254740992, 9007199254740992.0));
    CHECK(hglrt::eq__i64_f64_f64_f64(least, -9223372036854775808.0));
    CHECK_FALSE(hglrt::eq__i64_f64_f64_f64(most, 9223372036854775808.0));
    CHECK_FALSE(hglrt::eq__i64_f64_f64_f64(3, 3.5));
    CHECK(hglrt::eq__i64_f64_f64_f64(-3, -3.0));
    CHECK_FALSE(hglrt::eq__i64_f64_f64_f64(0, not_a_number));
    CHECK(hglrt::ne__i64_f64_f64_f64(0, not_a_number));
}

TEST_CASE("numeric equality within tolerances is exact (NUM-4, NUM-5)") {
    // "`1.0 =={abs: 0.01} 1.005`": true.
    CHECK(hglrt::eq__f64_f64_f64_f64(1.0, 1.005, 0.01));
    // "Relative-only tolerance comparing a nonzero value to zero, with rel
    // less than one": false.
    CHECK_FALSE(hglrt::eq__f64_f64_f64_f64(1.0, 0.0, 0.0, 0.5));
    // "Same signed infinities compared with a tolerance": true.
    CHECK(hglrt::eq__f64_f64_f64_f64(infinity, infinity, 1.0));
    CHECK_FALSE(hglrt::eq__f64_f64_f64_f64(infinity, -infinity, 1.0));
    CHECK_FALSE(hglrt::eq__f64_f64_f64_f64(not_a_number, not_a_number, 1.0, 1.0));
    // "`1e308 =={rel: 1.9} -1e308`": false, despite overflow in a naive
    // difference or threshold.
    CHECK_FALSE(hglrt::eq__f64_f64_f64_f64(1e308, -1e308, 0.0, 1.9));
    CHECK(hglrt::eq__f64_f64_f64_f64(1e308, -1e308, 0.0, 2.0));
    // "Tolerance 1 applied to 0/0.75, 0.75/1.5, and 0/1.5": true; true; false.
    CHECK(hglrt::eq__f64_f64_f64_f64(0.0, 0.75, 1.0));
    CHECK(hglrt::eq__f64_f64_f64_f64(0.75, 1.5, 1.0));
    CHECK_FALSE(hglrt::eq__f64_f64_f64_f64(0.0, 1.5, 1.0));
    // At the bound, the exact difference decides: this one is a little more
    // than the f64 nearest it, which a rounded difference would admit.
    CHECK(hglrt::eq__f64_f64_f64_f64(0.5, 0.75, 0.25));
    CHECK_FALSE(
        hglrt::eq__f64_f64_f64_f64(0x1.e759ff97b7508p-3, 0x1.1d5535cb1b7dcp-11, 0x1.e63caa61ec350p-3));
    CHECK(hglrt::eq__f64_f64_f64_f64(0x1.e759ff97b7508p-3, 0x1.1d5535cb1b7dcp-11, 0x1.e63caa61ec351p-3));
    CHECK(hglrt::eq__f64_f64_f64_f64(0x1p-1074, -0x1p-1074, 0x1p-1073));
    CHECK_FALSE(hglrt::eq__f64_f64_f64_f64(0x1p-1074, -0x1p-1074, 0.0, 0.5));
    CHECK(hglrt::eq__f64_f64_f64_f64(0x1p-1074, -0x1p-1074, 0.0, 2.0));
    CHECK(hglrt::eq__f64_f64_f64_f64(1e300, 1e-300, 1e300));
    CHECK_FALSE(hglrt::eq__f64_f64_f64_f64(1e300, -1e-300, 1e300));
    // An i64 against an i64 or an f64 is not rounded first.
    CHECK(hglrt::eq__i64_i64_f64_f64(most, most - 2, 2.0));
    CHECK_FALSE(hglrt::eq__i64_i64_f64_f64(most, most - 3, 2.0));
    CHECK(hglrt::eq__i64_i64_f64_f64(least, most, 0.0, 2.0));
    CHECK_FALSE(hglrt::eq__i64_i64_f64_f64(least, most, 0.0, 1.0));
    CHECK(hglrt::eq__i64_f64_f64_f64(9007199254740993, 9007199254740992.0, 1.0));
    CHECK_FALSE(hglrt::eq__i64_f64_f64_f64(9007199254740993, 9007199254740992.0, 0.5));
    CHECK_FALSE(hglrt::eq__i64_f64_f64_f64(0, infinity, 1.0));
    CHECK(hglrt::ne__i64_f64_f64_f64(0, 1.5, 1.0));
}

TEST_CASE("a tolerance that is not finite and nonnegative fails, with no code (NUM-4)") {
    CHECK(code_raised([] { (void)hglrt::eq__f64_f64_f64_f64(1.0, 1.0, -1.0); }) == "(no code)");
    CHECK(code_raised([] { (void)hglrt::eq__f64_f64_f64_f64(1.0, 1.0, 0.0, infinity); }) == "(no code)");
    CHECK(code_raised([] { (void)hglrt::ne__i64_i64_f64_f64(1, 1, not_a_number); }) == "(no code)");
    CHECK(code_raised([] { (void)hglrt::eq__i64_f64_f64_f64(1, 1.0, 0.0, -0.5); }) == "(no code)");
    CHECK(hglrt::eq__f64_f64_f64_f64(1.0, 1.0, -0.0));
}

TEST_CASE("equality and order of one type (OPS-2, NUM-6)") {
    CHECK(hglrt::eq__T_T<hglrt::str>("a", "a"));
    CHECK(hglrt::ne__T_T<bool>(true, false));
    CHECK_FALSE(hglrt::eq__T_T<f64>(not_a_number, not_a_number));
    CHECK(hglrt::eq__T_T<hglrt::list<f64>>({1.0, 2.0}, {1.0, 2.0}));
    CHECK_FALSE(hglrt::eq__T_T<hglrt::list<f64>>({1.0, not_a_number}, {1.0, not_a_number}));
    CHECK(hglrt::lt__T_T<i64>(-1, 0));
    CHECK(hglrt::le__T_T<i64>(0, 0));
    CHECK(hglrt::gt__T_T<f64>(1.0, -infinity));
    CHECK(hglrt::ge__T_T<f64>(-0.0, 0.0));
    // A NaN is unordered: no `<`, `<=`, `>` or `>=` holds.
    CHECK_FALSE(hglrt::lt__T_T<f64>(not_a_number, 1.0));
    CHECK_FALSE(hglrt::le__T_T<f64>(not_a_number, not_a_number));
    CHECK_FALSE(hglrt::gt__T_T<f64>(1.0, not_a_number));
    CHECK_FALSE(hglrt::ge__T_T<f64>(not_a_number, 1.0));
    // Text is ordered by its code points: `Z` before `a` before `é`.
    CHECK(hglrt::lt__T_T<hglrt::str>("Z", "a"));
    CHECK(hglrt::lt__T_T<hglrt::str>("z", "é"));
    CHECK(hglrt::lt__T_T<hglrt::str>("ab", "abc"));
    CHECK(hglrt::lt__T_T<bool>(false, true));
    CHECK(hglrt::lt__T_T<hglrt::duration>(hglrt::duration{-1}, hglrt::duration{0}));
}

TEST_CASE("Boolean forms (OPS-5)") {
    CHECK(hglrt::and__bool_bool(true, true));
    CHECK_FALSE(hglrt::and__bool_bool(true, false));
    CHECK(hglrt::or__bool_bool(false, true));
    CHECK_FALSE(hglrt::or__bool_bool(false, false));
    CHECK(hglrt::not__bool(false));
    CHECK_FALSE(hglrt::not__bool(true));
}
