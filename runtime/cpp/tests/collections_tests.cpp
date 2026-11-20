// Unit tests of sets, dictionaries, structs, abstract structs and their deltas
// as values (collections.h).
#include <hglrt/collections.h>

#include <doctest/doctest.h>

#include <limits>
#include <memory>
#include <optional>
#include <string>
#include <tuple>
#include <utility>

namespace {

using hglrt::f64;
using hglrt::i64;
using hglrt::str;

/// A struct as the compiler writes one: an aggregate with HGL's equality.
struct Point {
    f64 x;
    f64 y;

    friend bool operator==(const Point&, const Point&) = default;
};

/// An abstract struct with one field, as the compiler writes one.
class Shape {
  public:
    struct of_ : hglrt::held_value {
        [[nodiscard]] virtual const i64& id() const = 0;
    };
    template <class S>
    struct as_ final : hglrt::held_as<of_, S> {
        using hglrt::held_as<of_, S>::held_as;
        [[nodiscard]] const i64& id() const override { return this->held_.id; }
    };

    Shape() = default;
    template <class S>
        requires hglrt::inherits<S, Shape>
    Shape(S value) : held_{std::make_shared<const as_<S>>(std::move(value))} {}

    [[nodiscard]] const i64& id() const { return held_->id(); }
    friend bool operator==(const Shape& lhs, const Shape& rhs) { return lhs.held_->equal_(*rhs.held_); }

    std::shared_ptr<const of_> held_;
};

/// Two structs that inherit from `Shape`.
struct Round {
    i64 id;
    f64 radius;

    friend bool operator==(const Round&, const Round&) = default;
};
struct Square {
    i64 id;
    f64 side;

    friend bool operator==(const Square&, const Square&) = default;
};

} // namespace

template <>
struct hglrt::fields<Point> {
    static constexpr auto members = std::tuple{&Point::x, &Point::y};
};
template <>
struct hglrt::fields<Round> {
    static constexpr auto members = std::tuple{&Round::id, &Round::radius};
};
template <>
struct hglrt::fields<Square> {
    static constexpr auto members = std::tuple{&Square::id, &Square::side};
};
template <>
struct hglrt::parents<Round> {
    using type = std::tuple<Shape>;
};
template <>
struct hglrt::parents<Square> {
    using type = std::tuple<Shape>;
};

TEST_CASE("a set holds each member once and is equal whatever the order (TYPE-14, OPS-2)") {
    const hglrt::set<i64> made{3, 1, 2, 1};
    CHECK(made.size() == 3);
    CHECK(made == hglrt::set<i64>{1, 2, 3});
    CHECK(made != hglrt::set<i64>{1, 2});
    CHECK(made != hglrt::set<i64>{1, 2, 4});
    CHECK(hglrt::set<str>{} == hglrt::set<str>{});
}

TEST_CASE("a key of f64: the two zeros are one, and so is every NaN") {
    const f64 nan = std::numeric_limits<f64>::quiet_NaN();
    hglrt::set<f64> made{0.0, -0.0, nan, -nan};
    CHECK(made.size() == 2);
    CHECK(made.contains(0.0));
    CHECK(made.contains(nan));
    // NUM-5 holds of `==`, and not of a key's identity.
    CHECK_FALSE(nan == nan);
    CHECK(hglrt::same(nan, nan));
    CHECK(hglrt::key_hash_of(0.0) == hglrt::key_hash_of(-0.0));
    // A set's members are told apart by their identity, so a set that holds
    // a NaN is the same set as itself.
    CHECK(made == made);
    // A key that holds an f64 at any depth.
    hglrt::set<std::tuple<f64, i64>> tuples{{nan, 1}, {nan, 1}, {-0.0, 2}, {0.0, 2}};
    CHECK(tuples.size() == 2);
    hglrt::set<hglrt::list<f64>> lists{hglrt::list<f64>{nan}, hglrt::list<f64>{nan}};
    CHECK(lists.size() == 1);
}

TEST_CASE("a dictionary is equal where its keys are and each value is equal (OPS-2)") {
    const hglrt::dict<str, i64> book{{"a", 1}, {"b", 2}};
    CHECK(book == hglrt::dict<str, i64>{{"b", 2}, {"a", 1}});
    CHECK(book != hglrt::dict<str, i64>{{"a", 1}, {"b", 3}});
    CHECK(book != hglrt::dict<str, i64>{{"a", 1}, {"c", 2}});
    // NUM-5 holds inside a value: a dictionary that holds a NaN is not equal
    // to itself.
    const hglrt::dict<i64, f64> odd{{1, std::numeric_limits<f64>::quiet_NaN()}};
    CHECK_FALSE(odd == odd);
    CHECK(hglrt::same(odd, odd));
}

TEST_CASE("a dictionary written twice for a key has the value written last") {
    const auto book = hglrt::dict_of<str, i64>({{"a", 1}, {"b", 2}, {"a", 3}});
    CHECK(book.size() == 2);
    CHECK(hglrt::entry_at(book, "a") == 3);
}

TEST_CASE("a strict read of a key a dictionary has not fails with value.missing_key (ERR-6)") {
    hglrt::dict<str, i64> book{{"a", 1}};
    CHECK(hglrt::entry_at(book, "a") == 1);
    try {
        (void)hglrt::entry_at(std::as_const(book), "b");
        FAIL("nothing was raised");
    } catch (const hglrt::Failure& failure) {
        CHECK(failure.error().code == "value.missing_key");
    }
    // A compound assignment reads the place first (EXPR-3).
    CHECK_THROWS_AS(hglrt::entry_at(book, "b") += 1, hglrt::Failure);
    CHECK(book.size() == 1);
    // A present zero is not absent.
    hglrt::entry(book, "z") = 0;
    CHECK(hglrt::entry_at(book, "z") == 0);
}

TEST_CASE("an assignment adds a key the dictionary has not (EXPR-3)") {
    hglrt::dict<str, hglrt::list<i64>> book{{"a", {1}}};
    hglrt::entry(book, "b") = hglrt::list<i64>{2, 3};
    CHECK(book.size() == 2);
    // A value held by reference outlives the growth of its dictionary, which
    // keeps each entry where it is.
    const hglrt::list<i64>& held = hglrt::entry_at(book, "a");
    for (i64 key = 0; key < 1000; ++key) {
        hglrt::entry(book, std::to_string(key)) = hglrt::list<i64>{key};
    }
    CHECK(held == hglrt::list<i64>{1});
    // An entry assigned from another of the same dictionary.
    hglrt::entry(book, "c") = hglrt::entry_at(book, "b");
    CHECK(hglrt::entry_at(book, "c") == hglrt::list<i64>{2, 3});
}

TEST_CASE("get gives the value of a key, or null (ERR-6)") {
    const hglrt::dict<str, i64> book{{"a", 0}};
    CHECK(hglrt::get_dict_K__T__K(book, "a") == std::optional<i64>{0});
    CHECK(hglrt::get_dict_K__T__K(book, "b") == std::nullopt);
}

TEST_CASE("a struct is a key through its fields") {
    const f64 nan = std::numeric_limits<f64>::quiet_NaN();
    const Point odd{nan, 0.0};
    CHECK_FALSE(odd == odd);
    hglrt::set<Point> points{odd, odd, Point{1.0, -0.0}, Point{1.0, 0.0}};
    CHECK(points.size() == 2);
    CHECK(points.contains(Point{1.0, 0.0}));
}

TEST_CASE("the deltas of a set, a dictionary and a list are values (TYPE-16)") {
    const hglrt::set_delta<i64> change{{1, 2}, {3}};
    CHECK(change == hglrt::set_delta<i64>{{2, 1}, {3}});
    CHECK(change != hglrt::set_delta<i64>{{1, 2}, {}});
    hglrt::set<hglrt::set_delta<i64>> changes{change, change};
    CHECK(changes.size() == 1);
    static_assert(std::is_same_v<hglrt::delta_t<hglrt::set<i64>>, hglrt::set_delta<i64>>);
    static_assert(std::is_same_v<hglrt::delta_t<hglrt::dict<str, hglrt::list<i64>>>,
                                 hglrt::dict_delta<str, hglrt::list<i64>>>);
    const hglrt::dict_delta<str, i64> booked{{{"a", 1}}, {"b"}};
    CHECK(booked.updated.size() == 1);
    CHECK(booked == hglrt::dict_delta<str, i64>{{{"a", 1}}, {"b"}});
    const auto listed = hglrt::list_delta_of<hglrt::list<i64>>(hglrt::dict<i64, i64>{{0, 4}},
                                                               hglrt::set<i64>{1}, std::nullopt);
    CHECK(listed.updated == std::map<i64, i64>{{0, 4}});
    CHECK(hglrt::updated_of(listed) == hglrt::dict<i64, i64>{{0, 4}});
    CHECK(hglrt::removed_of(listed) == hglrt::set<i64>{1});
    CHECK(listed.size_modified == std::nullopt);
}

TEST_CASE("a value of an abstract struct holds a struct that inherits from it (TYPE-3, OPS-2)") {
    const Shape round = Round{1, 2.0};
    const Shape square = Square{1, 2.0};
    CHECK(round.id() == 1);
    CHECK(round == Shape{Round{1, 2.0}});
    // One of one struct is no value of another, whatever their fields.
    CHECK_FALSE(round == square);
    CHECK(hglrt::same(round, Shape{Round{1, 2.0}}));
    CHECK_FALSE(hglrt::same(round, square));
    hglrt::set<Shape> shapes{round, square, Shape{Round{1, 2.0}}};
    CHECK(shapes.size() == 2);
    // A copy holds the same struct.
    const Shape copied = round;
    CHECK(copied == round);
    const auto converted = hglrt::parent_list<Shape>(hglrt::list<Round>{Round{3, 1.0}});
    CHECK(converted.size() == 1);
    CHECK(converted[0].id() == 3);
    static_assert(!std::is_constructible_v<Shape, Point>);
}

TEST_CASE("an enum's member is made from its number or its name (TYPE-7)") {
    enum class Side { buy, sell };
    CHECK(hglrt::enum_member<Side>(i64{1}, {{0, Side::buy}, {1, Side::sell}}) == Side::sell);
    CHECK(hglrt::enum_member<Side>(str{"buy"}, {{"buy", Side::buy}, {"sell", Side::sell}}) == Side::buy);
    CHECK_THROWS_AS(hglrt::enum_member<Side>(i64{2}, {{0, Side::buy}, {1, Side::sell}}), hglrt::Failure);
    CHECK_THROWS_AS(hglrt::enum_member<Side>(str{"BUY"}, {{"buy", Side::buy}}), hglrt::Failure);
}
