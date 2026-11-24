// Unit tests of core's native conversions (conversions.h), against core's
// conversion cases (`tests/core_conformance/conversions.hgl`) and the edges of
// each rule of OPS-6 to OPS-8.
#include <hglrt/conversions.h>
#include <hglrt/formatting.h>

#include <doctest/doctest.h>

#include <chrono>
#include <cmath>
#include <limits>
#include <optional>
#include <string>

namespace {

using hglrt::bytes;
using hglrt::f64;
using hglrt::i64;
using hglrt::InvalidPolicy;
using hglrt::Rounding;
using hglrt::str;

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

/// `whole(value, rounding)` of core's case: `convert<i64>(value, rounding:
/// rounding, invalid: .fail)`.
i64 whole(f64 value, Rounding rounding) {
    return hglrt::convert_f64_Rounding_i64_<InvalidPolicy::fail>(value, rounding);
}

/// Whether the text `text` is no `i64`: `is_err(convert<i64>(text))`.
bool no_integer(const str& text) {
    const hglrt::result<i64> made = hglrt::convert_str<i64>(text);
    return !made.has_value() && made.error().code == "convert.invalid";
}

/// Whether the text `text` is no `f64`.
bool no_number(const str& text) {
    const hglrt::result<f64> made = hglrt::convert_str<f64>(text);
    return !made.has_value() && made.error().code == "convert.invalid";
}

/// `again(text)` of core's case: the text of the `f64` that `text` is.
str again(const str& text) {
    return hglrt::convert_f64<str>(hglrt::convert_str<f64, InvalidPolicy::fail>(text));
}

} // namespace

TEST_CASE("a number is a flag and back, and an i64 an f64 (OPS-6)") {
    // core's case a_number_is_a_flag_and_back.
    CHECK(hglrt::convert_i64<f64>(3) == 3.0);
    CHECK(hglrt::convert_i64<bool>(7));
    CHECK_FALSE(hglrt::convert_i64<bool>(0));
    CHECK(hglrt::convert_i64<bool>(least));
    CHECK(hglrt::convert_f64<bool>(0.5));
    CHECK_FALSE(hglrt::convert_f64<bool>(0.0));
    CHECK_FALSE(hglrt::convert_f64<bool>(-0.0));
    CHECK(hglrt::convert_f64<bool>(not_a_number));
    CHECK(hglrt::convert_f64<bool>(-infinity));
    CHECK(hglrt::convert_bool<i64>(true) == 1);
    CHECK(hglrt::convert_bool<i64>(false) == 0);
    CHECK(hglrt::convert_bool<f64>(true) == 1.0);
    CHECK(hglrt::convert_bool<f64>(false) == 0.0);
    // "The nearest `f64`", a tie to the even one.
    CHECK(hglrt::convert_i64<f64>(9007199254740993) == 9007199254740992.0);
    CHECK(hglrt::convert_i64<f64>(9007199254740995) == 9007199254740996.0);
    CHECK(hglrt::convert_i64<f64>(most) == 0x1p63);
    CHECK(hglrt::convert_i64<f64>(least) == -0x1p63);
}

TEST_CASE("an f64 is rounded to an i64 as asked (OPS-7, OPS-8)") {
    // core's case an_f64_is_rounded_as_asked.
    CHECK(whole(2.9, Rounding::truncate) == 2);
    CHECK(whole(-2.9, Rounding::truncate) == -2);
    CHECK(whole(-2.5, Rounding::floor) == -3);
    CHECK(whole(-2.5, Rounding::ceiling) == -2);
    CHECK(whole(2.5, Rounding::half_even) == 2);
    CHECK(whole(3.5, Rounding::half_even) == 4);
    CHECK(whole(-2.5, Rounding::half_even) == -2);
    CHECK(whole(-3.5, Rounding::half_even) == -4);
    CHECK(whole(2.5, Rounding::half_up) == 3);
    CHECK(whole(-2.5, Rounding::half_up) == -3);
    CHECK(whole(2.5, Rounding::half_down) == 2);
    CHECK(whole(-2.5, Rounding::half_down) == -2);
    CHECK(whole(2.6, Rounding::half_down) == 3);
    CHECK(whole(2.4, Rounding::half_up) == 2);
    // Just below a half is no tie.
    CHECK(whole(0.49999999999999994, Rounding::half_up) == 0);
    CHECK(whole(-0.49999999999999994, Rounding::half_up) == 0);
    CHECK(whole(0.5000000000000001, Rounding::half_down) == 1);
    // A number of 2^52 or more is whole already.
    CHECK(whole(4503599627370497.0, Rounding::half_even) == 4503599627370497);
    CHECK(whole(-0.0, Rounding::ceiling) == 0);
    // `.truncate` where none is given, and a `result` where `invalid` is not.
    CHECK(hglrt::convert_f64_Rounding_i64_<InvalidPolicy::fail>(2.9) == 2);
    const hglrt::result<i64> made = hglrt::convert_f64_Rounding_i64_(2.9);
    REQUIRE(made.has_value());
    CHECK(*made == 2);
    // The ends of i64 once rounded.
    CHECK(*hglrt::convert_f64_Rounding_i64_(-9223372036854775808.0) == least);
    CHECK(*hglrt::convert_f64_Rounding_i64_(9223372036854774784.0) == 9223372036854774784);
}

TEST_CASE("an f64 that is no i64 fails, and `nan` is what a NaN is (OPS-7, OPS-8)") {
    // core's case an_f64_that_is_no_i64_fails.
    CHECK(hglrt::convert_f64_Rounding_i64_(not_a_number).error().code == "convert.invalid");
    CHECK_FALSE(hglrt::convert_f64_Rounding_i64_(infinity).has_value());
    CHECK_FALSE(hglrt::convert_f64_Rounding_i64_(-infinity).has_value());
    CHECK_FALSE(hglrt::convert_f64_Rounding_i64_(9223372036854775808.0).has_value());
    CHECK(hglrt::convert_f64_Rounding_i64_(-9223372036854775808.0).has_value());
    CHECK(hglrt::convert_f64_Rounding_i64_<InvalidPolicy::fail>(not_a_number, Rounding::truncate, -1) == -1);
    CHECK(code_raised([] { (void)hglrt::convert_f64_Rounding_i64_<InvalidPolicy::fail>(not_a_number); }) ==
          "convert.invalid");
    CHECK(code_raised([] { (void)hglrt::convert_f64_Rounding_i64_<InvalidPolicy::fail>(1e19); }) ==
          "convert.invalid");
    // "With `nan: 0` a NaN converts to 0; an infinity still fails."
    CHECK(*hglrt::convert_f64_Rounding_i64_(not_a_number, Rounding::floor, 0) == 0);
    CHECK_FALSE(hglrt::convert_f64_Rounding_i64_(infinity, Rounding::floor, 0).has_value());
    // The f64 below the least i64 is no i64 however it is rounded.
    CHECK_FALSE(hglrt::convert_f64_Rounding_i64_(-9223372036854777856.0, Rounding::ceiling).has_value());
}

TEST_CASE("an integer and a flag are written (OPS-6)") {
    // core's case an_integer_and_a_flag_are_written.
    CHECK(hglrt::convert_i64<str>(42) == "42");
    CHECK(hglrt::convert_i64<str>(-3) == "-3");
    CHECK(hglrt::convert_i64<str>(0) == "0");
    CHECK(hglrt::convert_i64<str>(least) == "-9223372036854775808");
    CHECK(hglrt::convert_i64<str>(most) == "9223372036854775807");
    CHECK(hglrt::convert_bool<str>(true) == "true");
    CHECK(hglrt::convert_bool<str>(false) == "false");
}

TEST_CASE("an f64 is written as the shortest text that reads back (OPS-6)") {
    // core's case an_f64_is_written_as_the_shortest_text_that_reads_back.
    CHECK(hglrt::convert_f64<str>(3.0) == "3.0");
    CHECK(hglrt::convert_f64<str>(0.1) == "0.1");
    CHECK(hglrt::convert_f64<str>(-2.5) == "-2.5");
    CHECK(hglrt::convert_f64<str>(0.1 + 0.2) == "0.30000000000000004");
    CHECK(hglrt::convert_f64<str>(-0.0) == "-0.0");
    CHECK(hglrt::convert_f64<str>(0.0) == "0.0");
    // An exponent from 1e16 up, and below 1e-4.
    CHECK(hglrt::convert_f64<str>(1000000000000000.0) == "1000000000000000.0");
    CHECK(hglrt::convert_f64<str>(9999999999999998.0) == "9999999999999998.0");
    CHECK(hglrt::convert_f64<str>(10000000000000000.0) == "1e16");
    CHECK(hglrt::convert_f64<str>(1e21) == "1e21");
    CHECK(hglrt::convert_f64<str>(1.5e300) == "1.5e300");
    CHECK(hglrt::convert_f64<str>(-1.5e300) == "-1.5e300");
    CHECK(hglrt::convert_f64<str>(0.0001) == "0.0001");
    CHECK(hglrt::convert_f64<str>(0.00001) == "1e-5");
    CHECK(hglrt::convert_f64<str>(1.5e-7) == "1.5e-7");
    CHECK(hglrt::convert_f64<str>(123.456) == "123.456");
    CHECK(hglrt::convert_f64<str>(0.001234) == "0.001234");
    CHECK(hglrt::convert_f64<str>(std::numeric_limits<f64>::max()) == "1.7976931348623157e308");
    CHECK(hglrt::convert_f64<str>(std::numeric_limits<f64>::denorm_min()) == "5e-324");
    CHECK(hglrt::convert_f64<str>(not_a_number) == "nan");
    CHECK(hglrt::convert_f64<str>(infinity) == "inf");
    CHECK(hglrt::convert_f64<str>(-infinity) == "-inf");
}

TEST_CASE("a default f64 takes its sign from the converted text (FMT-4, OPS-6)") {
    const auto format = [](f64 value, hglrt::TextPadding padding,
                           std::optional<char> sign = std::nullopt, bool zero = false) {
        str text;
        hglrt::format_float_text(text, value, padding, sign, zero);
        return text;
    };
    const hglrt::TextPadding right{" ", 8, '>'};
    const hglrt::TextPadding no_width{" ", 0, '>'};
    CHECK(format(3.0, no_width, '+') == "+3.0");
    CHECK(format(3.0, no_width, '-') == "3.0");
    CHECK(format(3.0, no_width, ' ') == " 3.0");
    CHECK(format(3.0, no_width, std::nullopt, true) == "3.0");
    CHECK(format(-0.0, no_width, '+') == "-0.0");
    CHECK(format(1e21, no_width, '+') == "+1e21");
    for (f64 nan : {std::copysign(not_a_number, 1.0), std::copysign(not_a_number, -1.0)}) {
        CHECK(format(nan, right) == "     nan");
        CHECK(format(nan, right, '-') == "     nan");
        CHECK(format(nan, right, '+') == "    +nan");
        CHECK(format(nan, right, ' ') == "     nan");
        CHECK(format(nan, right, std::nullopt, true) == "     nan");
        CHECK(format(nan, {"*", 9, '^'}, '+') == "**+nan***");
        CHECK(format(nan, no_width, '+') == "+nan");
        CHECK(format(nan, no_width, '-') == "nan");
        CHECK(format(nan, no_width, ' ') == " nan");
    }
    CHECK(format(-0.0, right, '+', true) == "-00000.0");
    CHECK(format(-infinity, right, '+', true) == "    -inf");
}

TEST_CASE("text is an i64 where it is an integer literal, after one `-` (OPS-6, OPS-7)") {
    // core's case text_is_read_as_an_integer.
    CHECK(hglrt::convert_str<i64, InvalidPolicy::fail>("42") == 42);
    CHECK(hglrt::convert_str<i64, InvalidPolicy::fail>("-3") == -3);
    CHECK(hglrt::convert_str<i64, InvalidPolicy::fail>("0x1F") == 31);
    CHECK(hglrt::convert_str<i64, InvalidPolicy::fail>("0x1f") == 31);
    CHECK(hglrt::convert_str<i64, InvalidPolicy::fail>("-0b101") == -5);
    CHECK(hglrt::convert_str<i64, InvalidPolicy::fail>("1_000") == 1000);
    CHECK(hglrt::convert_str<i64, InvalidPolicy::fail>("0xFFFF_FFFF") == 4294967295);
    CHECK(hglrt::convert_str<i64, InvalidPolicy::fail>("-9223372036854775808") == least);
    CHECK(hglrt::convert_str<i64, InvalidPolicy::fail>("9223372036854775807") == most);
    CHECK(hglrt::convert_str<i64, InvalidPolicy::fail>("0x7FFF_FFFF_FFFF_FFFF") == most);
    CHECK(hglrt::convert_str<i64, InvalidPolicy::fail>("-0x8000000000000000") == least);
    CHECK(hglrt::convert_str<i64, InvalidPolicy::fail>("007") == 7);
    CHECK(hglrt::convert_str<i64, InvalidPolicy::fail>("-0") == 0);
    CHECK(hglrt::convert_str<i64, InvalidPolicy::fail>("000000000000000000000000000000000001") == 1);
    // Text that is no such literal, and a number outside i64.
    CHECK(no_integer("9223372036854775808"));
    CHECK(no_integer("-9223372036854775809"));
    CHECK(no_integer("0x8000000000000000"));
    CHECK(no_integer("99999999999999999999999999999999999999999999"));
    CHECK(no_integer("three"));
    CHECK(no_integer(""));
    CHECK(no_integer("-"));
    CHECK(no_integer(" 3"));
    CHECK(no_integer("3 "));
    CHECK(no_integer("+3"));
    CHECK(no_integer("--3"));
    CHECK(no_integer("3.0"));
    CHECK(no_integer("1__0"));
    CHECK(no_integer("1_"));
    CHECK(no_integer("_1"));
    CHECK(no_integer("0x"));
    CHECK(no_integer("0x_1"));
    CHECK(no_integer("0X1F"));
    CHECK(no_integer("0b102"));
    CHECK(no_integer("٣"));
    CHECK(code_raised([] { (void)hglrt::convert_str<i64, InvalidPolicy::fail>("three"); }) ==
          "convert.invalid");
}

TEST_CASE("text is an f64 where it is a float or an integer literal, or names one (OPS-6, OPS-7)") {
    // core's case text_is_read_as_a_float.
    CHECK(hglrt::convert_str<f64, InvalidPolicy::fail>("2.5") == 2.5);
    CHECK(hglrt::convert_str<f64, InvalidPolicy::fail>("-1e5") == -100000.0);
    CHECK(hglrt::convert_str<f64, InvalidPolicy::fail>("3") == 3.0);
    CHECK(hglrt::convert_str<f64, InvalidPolicy::fail>("1_0.5E1") == 105.0);
    CHECK(hglrt::convert_str<f64, InvalidPolicy::fail>("1e+2") == 100.0);
    CHECK(hglrt::convert_str<f64, InvalidPolicy::fail>("inf") == infinity);
    CHECK(hglrt::convert_str<f64, InvalidPolicy::fail>("-inf") == -infinity);
    CHECK(std::isnan(hglrt::convert_str<f64, InvalidPolicy::fail>("nan")));
    // The text a number converts to converts back to that number.
    CHECK(again("0.30000000000000004") == "0.30000000000000004");
    CHECK(again("1e21") == "1e21");
    CHECK(again("-0.0") == "-0.0");
    CHECK(again("nan") == "nan");
    CHECK(again("-inf") == "-inf");
    CHECK(again("5e-324") == "5e-324");
    CHECK(again("1.7976931348623157e308") == "1.7976931348623157e308");
    // The nearest f64, a tie to the even one; an integer literal of any
    // base, and of more digits than an f64 holds.
    CHECK(hglrt::convert_str<f64, InvalidPolicy::fail>("9007199254740993") == 9007199254740992.0);
    CHECK(hglrt::convert_str<f64, InvalidPolicy::fail>("9007199254740995") == 9007199254740996.0);
    CHECK(hglrt::convert_str<f64, InvalidPolicy::fail>("0x1F") == 31.0);
    CHECK(hglrt::convert_str<f64, InvalidPolicy::fail>("-0b1_01") == -5.0);
    CHECK(hglrt::convert_str<f64, InvalidPolicy::fail>("0b1" + std::string(52, '0') + "1") ==
          9007199254740992.0);
    CHECK(hglrt::convert_str<f64, InvalidPolicy::fail>("0x20000000000003") == 9007199254740996.0);
    CHECK(hglrt::convert_str<f64, InvalidPolicy::fail>("99999999999999999999") == 1e20);
    // LEX-20: "A nonzero literal that rounds to zero or to a subnormal is
    // accepted".
    CHECK(hglrt::convert_str<f64, InvalidPolicy::fail>("1e-400") == 0.0);
    CHECK(std::signbit(hglrt::convert_str<f64, InvalidPolicy::fail>("-1e-400")));
    CHECK(hglrt::convert_str<f64, InvalidPolicy::fail>("0.000000000000000000000000000000001e-300") == 0.0);
    CHECK(hglrt::convert_str<f64, InvalidPolicy::fail>("4e-324") == 5e-324);
    // Text that is neither, and a number that rounds to an infinity.
    CHECK(no_number("1e999"));
    CHECK(no_number("-1e999"));
    CHECK(no_number("1.7976931348623159e308"));
    CHECK(no_number("100000000000000000000e290"));
    CHECK(no_number("0x" + std::string(300, 'F')));
    CHECK(no_number(".5"));
    CHECK(no_number("5."));
    CHECK(no_number("NaN"));
    CHECK(no_number("+inf"));
    CHECK(no_number("-nan"));
    CHECK(no_number("infinity"));
    CHECK(no_number("2.5 "));
    CHECK(no_number("1e"));
    CHECK(no_number("1e_5"));
    CHECK(no_number("1._5"));
    CHECK(no_number("0x1p3"));
    CHECK(no_number(""));
    CHECK(code_raised([] { (void)hglrt::convert_str<f64, InvalidPolicy::fail>("two"); }) ==
          "convert.invalid");
}

TEST_CASE("text and a bool, text and bytes, a date and a datetime (OPS-6, OPS-7)") {
    using namespace std::chrono;
    // core's case the_other_conversions.
    CHECK(hglrt::convert_str<bool, InvalidPolicy::fail>("true"));
    CHECK_FALSE(hglrt::convert_str<bool, InvalidPolicy::fail>("false"));
    CHECK_FALSE(hglrt::convert_str<bool>("True").has_value());
    CHECK_FALSE(hglrt::convert_str<bool>("").has_value());
    CHECK(code_raised([] { (void)hglrt::convert_str<bool, InvalidPolicy::fail>("yes"); }) ==
          "convert.invalid");
    // UTF-8 both ways.
    const str text = "héllo";
    const bytes encoded = hglrt::convert_str<bytes>(text);
    CHECK(encoded.data.size() == 6);
    CHECK(hglrt::convert_bytes<str, InvalidPolicy::fail>(encoded) == text);
    CHECK(hglrt::convert_bytes<str, InvalidPolicy::fail>(bytes{""}).empty());
    CHECK(hglrt::convert_bytes<str, InvalidPolicy::fail>(bytes{std::string{"a\0b", 3}}).size() == 3);
    CHECK(hglrt::convert_bytes<str, InvalidPolicy::fail>(bytes{"\U0010FFFF"}) == "\U0010FFFF");
    // Bytes that are not well-formed UTF-8: a lone continuation byte, a
    // lead cut short, an overlong form, a surrogate, and beyond U+10FFFF.
    for (const char* malformed : {"\x80", "\xc3", "a\xe2\x82", "\xc0\xaf", "\xe0\x80\xaf", "\xed\xa0\x80",
                                  "\xf4\x90\x80\x80", "\xf5\x80\x80\x80", "\xff"}) {
        CAPTURE(malformed);
        CHECK(hglrt::convert_bytes<str>(bytes{malformed}).error().code == "convert.invalid");
    }
    CHECK(code_raised([] { (void)hglrt::convert_bytes<str, InvalidPolicy::fail>(bytes{"\xff"}); }) ==
          "convert.invalid");
    // "`convert<datetime>(@2026-03-01) == @2026-03-01T00:00:00Z`";
    // "`convert<date>(@2026-03-01T23:59:59Z) == @2026-03-01`".
    CHECK(hglrt::convert_date<hglrt::datetime>(year{2026} / 3 / 1) == sys_days{year{2026} / 3 / 1});
    // A name no other candidate has is called with no template argument.
    CHECK(hglrt::convert_date(year{2026} / 3 / 1) == sys_days{year{2026} / 3 / 1});
    CHECK(hglrt::convert_datetime(sys_days{year{2026} / 3 / 1}) == year{2026} / 3 / 1);
    CHECK(hglrt::convert_bytes(bytes{"ok"}).value() == "ok");
    CHECK(hglrt::convert_datetime<hglrt::date>(sys_days{year{2026} / 3 / 1} + 23h + 59min + 59s) ==
          year{2026} / 3 / 1);
    // Before the epoch, the date is the day the instant is in.
    CHECK(hglrt::convert_datetime<hglrt::date>(sys_days{year{1969} / 12 / 31} + 23h) == year{1969} / 12 / 31);
    CHECK(hglrt::convert_date<hglrt::datetime>(year{1} / 1 / 1) == sys_days{year{1} / 1 / 1});
    CHECK(hglrt::convert_datetime<hglrt::date>(sys_days{year{9999} / 12 / 31} + 24h - 1us) ==
          year{9999} / 12 / 31);
}

TEST_CASE("the members of the conversions' enums are core's, in its order (OPS-8)") {
    CHECK(static_cast<int>(Rounding::truncate) == 0);
    CHECK(static_cast<int>(Rounding::half_down) == 5);
    CHECK(static_cast<int>(InvalidPolicy::error) == 0);
    CHECK(static_cast<int>(InvalidPolicy::fail) == 1);
}
