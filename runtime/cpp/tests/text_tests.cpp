// Unit tests of core's native scalar operators of text (text.h), and of the
// equality and order of text and bytes (scalar.h, OPS-2, TYPE-1).
#include <hglrt/scalar.h>
#include <hglrt/text.h>

#include <doctest/doctest.h>

#include <limits>
#include <stdexcept>
#include <string>

namespace {

using hglrt::bytes;
using hglrt::i64;
using hglrt::str;

} // namespace

TEST_CASE("text is concatenated (OPS-1)") {
    CHECK(hglrt::add__str_str("a", "b") == "ab");
    CHECK(hglrt::add__str_str("", "") == "");
    CHECK(hglrt::add__str_str("hé", "llo") == "héllo");
    // A byte that is no character on its own stays as it is.
    CHECK(hglrt::add__str_str(str{"\xc3"}, str{"\xa9"}) == "é");
    CHECK(hglrt::add__str_str(str{"a\0b", 3}, "c").size() == 4);
}

TEST_CASE("text is repeated a count of times (OPS-1)") {
    // core's case reverse_scaling: `mul_("ab", 2) == "abab"`.
    CHECK(hglrt::mul__str_i64("ab", 2) == "abab");
    CHECK(hglrt::mul__str_i64("ab", 0).empty());
    CHECK(hglrt::mul__str_i64("ab", 1) == "ab");
    CHECK(hglrt::mul__str_i64("", std::numeric_limits<i64>::max()).empty());
    CHECK(hglrt::mul__str_i64("é", 3) == "ééé");
    // Core's contract has no rule for a count below zero, or for text no
    // string can hold: neither is an HGL failure.
    CHECK_THROWS_AS((void)hglrt::mul__str_i64("ab", -1), std::domain_error);
    CHECK_THROWS_AS((void)hglrt::mul__str_i64("ab", std::numeric_limits<i64>::max()), std::length_error);
}

TEST_CASE("text and bytes are equal and ordered by their bytes (OPS-2, TYPE-1)") {
    CHECK(hglrt::eq__T_T<str>("", ""));
    CHECK(hglrt::ne__T_T<str>("e", "é"));
    // Unicode text by its code points: UTF-8's bytes keep their order.
    CHECK(hglrt::lt__T_T<str>("z", "é"));
    CHECK(hglrt::lt__T_T<str>("é", "€"));
    CHECK(hglrt::lt__T_T<str>("€", "\U0001F600"));
    CHECK(hglrt::le__T_T<str>("", ""));
    CHECK(hglrt::gt__T_T<str>("ab", "a"));
    CHECK(hglrt::ge__T_T<str>("b", "abc"));
    // "Bytes admit values 0 through 255; length and ordering use unsigned
    // octets."
    CHECK(hglrt::eq__T_T<bytes>(bytes{"\xff"}, bytes{"\xff"}));
    CHECK(hglrt::ne__T_T<bytes>(bytes{std::string{"a\0", 2}}, bytes{"a"}));
    CHECK(bytes{"\x7f"} < bytes{"\x80"});
}
