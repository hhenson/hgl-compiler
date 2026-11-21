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

TEST_CASE("a key is told apart by == (OPS-2, NUM-5): the two zeros are one, a NaN is no key's") {
    const f64 nan = std::numeric_limits<f64>::quiet_NaN();
    hglrt::set<f64> made{0.0, -0.0, nan};
    CHECK(made.size() == 2);
    CHECK(made.contains(0.0));
    CHECK(made.contains(-0.0));
    CHECK(hglrt::key_hash_of(0.0) == hglrt::key_hash_of(-0.0));
    // A NaN is equal to no value, itself included: no read finds it, a
    // second is a member of its own, and a set that holds one is not equal
    // to itself, as `==` has it inside a value.
    CHECK_FALSE(made.contains(nan));
    made.insert(nan);
    CHECK(made.size() == 3);
    CHECK_FALSE(made == made);
    // A key that holds an f64 at any depth.
    hglrt::set<std::tuple<f64, i64>> tuples{{-0.0, 2}, {0.0, 2}};
    CHECK(tuples.size() == 1);
    CHECK_FALSE(tuples.contains({nan, 2}));
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
}

TEST_CASE("a dictionary is made of its entries in the order written (EXPR-9)") {
    const auto book = hglrt::dict_of<str, i64>({{"a", 1}, {"b", 2}});
    CHECK(book.size() == 2);
    CHECK(hglrt::entry_at(book, "a") == 1);
    CHECK(hglrt::entry_at(book, "b") == 2);
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
    hglrt::entry(book, "c") = hglrt::entry_at(book, "c");
    CHECK(hglrt::entry_at(book, "c") == hglrt::list<i64>{2, 3});
    // Naming the place adds nothing: a value that fails leaves the
    // dictionary as it was.
    const auto size = book.size();
    static_cast<void>(hglrt::entry(book, "never"));
    CHECK_THROWS_AS(hglrt::entry(book, "never") = hglrt::entry_at(book, "missing"), hglrt::Failure);
    CHECK(book.size() == size);
    CHECK_FALSE(book.contains("never"));
}

TEST_CASE("get gives the value of a key, or null (ERR-6)") {
    const hglrt::dict<str, i64> book{{"a", 0}};
    CHECK(hglrt::get_dict_K__T__K(book, "a") == std::optional<i64>{0});
    CHECK(hglrt::get_dict_K__T__K(book, "b") == std::nullopt);
    // TYPE-14: `T?` of an `i64?` is an `i64?`, and a null held is null.
    const hglrt::dict<str, std::optional<i64>> nullable{{"a", std::nullopt}, {"b", 2}};
    static_assert(std::is_same_v<decltype(hglrt::get_dict_K__T__K(nullable, "a")), std::optional<i64>>);
    CHECK(hglrt::get_dict_K__T__K(nullable, "a") == std::nullopt);
    CHECK(hglrt::get_dict_K__T__K(nullable, "b") == std::optional<i64>{2});
    CHECK(hglrt::get_dict_K__T__K(nullable, "c") == std::nullopt);
    // TYPE-16: a nullable's delta is no nullable. A published null held is
    // a value, which a missing key is not.
    using Published = hglrt::delta_t<std::optional<i64>>;
    const hglrt::dict<str, Published> published{{"a", Published{}}, {"b", 2}};
    static_assert(
        std::is_same_v<decltype(hglrt::get_dict_K__T__K(published, "a")), std::optional<Published>>);
    CHECK(hglrt::get_dict_K__T__K(published, "a").has_value());
    CHECK(hglrt::get_dict_K__T__K(published, "a") == Published{});
    CHECK(hglrt::get_dict_K__T__K(published, "b") == Published{2});
    CHECK_FALSE(hglrt::get_dict_K__T__K(published, "c").has_value());
    // A `delta<i64?>?` compared with a `delta<i64?>`: as the value it is.
    CHECK_FALSE(std::optional<Published>{} == Published{});
    CHECK(std::optional<Published>{Published{}} == Published{});
    // A `delta<i64?>` as a key: a published null and 0 are two.
    const hglrt::set<Published> keys{Published{}, Published{0}, Published{}};
    CHECK(keys.size() == 2);
    CHECK(hglrt::type_name<Published>() == "delta<i64?>");
    CHECK(hglrt::type_name<hglrt::nullable_delta<hglrt::list<i64>, hglrt::list<i64>>>() ==
          "delta<atomic<list<i64>>?>");
}

TEST_CASE("a struct is a key through its fields") {
    const f64 nan = std::numeric_limits<f64>::quiet_NaN();
    const Point odd{nan, 0.0};
    CHECK_FALSE(odd == odd);
    hglrt::set<Point> points{Point{1.0, -0.0}, Point{1.0, 0.0}};
    CHECK(points.size() == 1);
    CHECK(points.contains(Point{1.0, 0.0}));
    // A struct that holds a NaN is equal to no struct: no read finds it.
    points.insert(odd);
    CHECK(points.size() == 2);
    CHECK_FALSE(points.contains(odd));
}

namespace {
/// A struct with a field of a `list<bool>`, whose elements libc++ gives as
/// references to their bits.
struct Bits {
    hglrt::list<bool> on;

    friend bool operator==(const Bits&, const Bits&) = default;
};
} // namespace

template <>
struct hglrt::fields<Bits> {
    static constexpr auto members = std::tuple{&Bits::on};
};

TEST_CASE("a list<bool> is a key, a member and a field (TYPE-14)") {
    const hglrt::list<bool> flags{true, false};
    hglrt::set<hglrt::list<bool>> members{flags, hglrt::list<bool>{true, false}, hglrt::list<bool>{false}};
    CHECK(members.size() == 2);
    CHECK(members.contains(flags));
    const hglrt::dict<hglrt::list<bool>, i64> keyed{{flags, 1}};
    CHECK(hglrt::entry_at(keyed, hglrt::list<bool>{true, false}) == 1);
    hglrt::set<Bits> fields{Bits{flags}, Bits{flags}, Bits{{false}}};
    CHECK(fields.size() == 2);
    CHECK(hglrt::key_hash_of(Bits{flags}) == hglrt::key_hash_of(Bits{{true, false}}));
    CHECK(hglrt::hash_of(flags) == hglrt::hash_of(hglrt::list<bool>{true, false}));
    const hglrt::set<bool> truths{true, false, true};
    CHECK(truths.size() == 2);
}

TEST_CASE("a result is a key whatever its failure's type (ERR-2)") {
    using failing = std::expected<i64, str>;
    hglrt::set<failing> results{failing{1}, failing{std::unexpect, "no"}, failing{std::unexpect, "no"}};
    CHECK(results.size() == 2);
    CHECK(results.contains(failing{std::unexpect, "no"}));
    hglrt::set<hglrt::result<i64>> errors{hglrt::result<i64>{std::unexpect, hglrt::make_error("a", "x")},
                                          hglrt::result<i64>{2}};
    CHECK(errors.size() == 2);
    CHECK(errors.contains(hglrt::result<i64>{std::unexpect, hglrt::make_error("a", "x")}));
    CHECK(hglrt::key_hash_of(hglrt::make_error("a")) == hglrt::key_hash_of(hglrt::make_error("a")));
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
    CHECK(hglrt::key_hash_of(round) == hglrt::key_hash_of(Shape{Round{1, 2.0}}));
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
