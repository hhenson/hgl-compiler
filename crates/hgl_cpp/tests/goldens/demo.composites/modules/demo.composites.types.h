// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#pragma once

#include <hglrt/hglrt.h>

// Names this file writes, which no macro of a header above takes.
#undef Base
#undef Book
#undef Box
#undef Empty
#undef Flags
#undef Holder
#undef Line
#undef Memo
#undef Noted
#undef Outcome
#undef Pair
#undef Point
#undef Round
#undef Shape
#undef Side
#undef Sided
#undef Sized
#undef Tagged
#undef buy
#undef composites
#undef count
#undef demo
#undef end
#undef first
#undef id
#undef kept
#undef lhs
#undef members
#undef n
#undef name
#undef note
#undef on
#undef owner
#undef points
#undef prices
#undef radius
#undef rhs
#undef sell
#undef side
#undef start
#undef tag
#undef tags
#undef value
#undef x
#undef y

namespace hgl::demo::composites::m_
{

// enum Side (TYPE-7)
enum class Side
{
    buy,
    sell,
};
constexpr ::hglrt::EnumNames<Side, 2> hgl_enum(Side) noexcept
{
    return {"demo.composites::Side", {{Side::buy, "buy"}, {Side::sell, "sell"}}};
}

struct Point;
struct Line;
struct Book;
template <class T_>
struct Box;
struct Empty;
template <::hgl::demo::composites::m_::Side side>
struct Sided;
struct Outcome;
struct Flags;
class Base;
class Shape;
struct Round;
template <class T_>
struct Tagged;
class Noted;
struct Memo;
template <class T_>
class Holder;
template <::hgraph::Int n>
class Sized;
struct Pair;
namespace d_
{
struct Point;
struct Line;
struct Book;
template <class T_>
struct Box;
struct Empty;
template <::hgl::demo::composites::m_::Side side>
struct Sided;
struct Outcome;
struct Flags;
struct Base;
struct Shape;
struct Round;
template <class T_>
struct Tagged;
struct Noted;
struct Memo;
template <class T_>
struct Holder;
template <::hgraph::Int n>
struct Sized;
struct Pair;
}  // namespace d_
}  // namespace hgl::demo::composites::m_

template <>
struct hglrt::delta_of<::hgl::demo::composites::m_::Point>
{
    using type = ::hgl::demo::composites::m_::d_::Point;
};
template <>
struct hglrt::delta_of<::hgl::demo::composites::m_::Line>
{
    using type = ::hgl::demo::composites::m_::d_::Line;
};
template <>
struct hglrt::delta_of<::hgl::demo::composites::m_::Book>
{
    using type = ::hgl::demo::composites::m_::d_::Book;
};
template <class T_>
struct hglrt::delta_of<::hgl::demo::composites::m_::Box<T_>>
{
    using type = ::hgl::demo::composites::m_::d_::Box<T_>;
};
template <>
struct hglrt::delta_of<::hgl::demo::composites::m_::Empty>
{
    using type = ::hgl::demo::composites::m_::d_::Empty;
};
template <::hgl::demo::composites::m_::Side side>
struct hglrt::delta_of<::hgl::demo::composites::m_::Sided<side>>
{
    using type = ::hgl::demo::composites::m_::d_::Sided<side>;
};
template <>
struct hglrt::delta_of<::hgl::demo::composites::m_::Outcome>
{
    using type = ::hgl::demo::composites::m_::d_::Outcome;
};
template <>
struct hglrt::delta_of<::hgl::demo::composites::m_::Flags>
{
    using type = ::hgl::demo::composites::m_::d_::Flags;
};
template <>
struct hglrt::delta_of<::hgl::demo::composites::m_::Base>
{
    using type = ::hgl::demo::composites::m_::d_::Base;
};
template <>
struct hglrt::delta_of<::hgl::demo::composites::m_::Shape>
{
    using type = ::hgl::demo::composites::m_::d_::Shape;
};
template <>
struct hglrt::delta_of<::hgl::demo::composites::m_::Round>
{
    using type = ::hgl::demo::composites::m_::d_::Round;
};
template <>
struct hglrt::parents<::hgl::demo::composites::m_::Round>
{
    using type = std::tuple<::hgl::demo::composites::m_::Shape, ::hgl::demo::composites::m_::Base>;
};
template <class T_>
struct hglrt::delta_of<::hgl::demo::composites::m_::Tagged<T_>>
{
    using type = ::hgl::demo::composites::m_::d_::Tagged<T_>;
};
template <class T_>
struct hglrt::parents<::hgl::demo::composites::m_::Tagged<T_>>
{
    using type = std::tuple<::hgl::demo::composites::m_::Shape, ::hgl::demo::composites::m_::Base>;
};
template <>
struct hglrt::delta_of<::hgl::demo::composites::m_::Noted>
{
    using type = ::hgl::demo::composites::m_::d_::Noted;
};
template <>
struct hglrt::delta_of<::hgl::demo::composites::m_::Memo>
{
    using type = ::hgl::demo::composites::m_::d_::Memo;
};
template <>
struct hglrt::parents<::hgl::demo::composites::m_::Memo>
{
    using type = std::tuple<::hgl::demo::composites::m_::Noted>;
};
template <class T_>
struct hglrt::delta_of<::hgl::demo::composites::m_::Holder<T_>>
{
    using type = ::hgl::demo::composites::m_::d_::Holder<T_>;
};
template <::hgraph::Int n>
struct hglrt::delta_of<::hgl::demo::composites::m_::Sized<n>>
{
    using type = ::hgl::demo::composites::m_::d_::Sized<n>;
};
template <>
struct hglrt::delta_of<::hgl::demo::composites::m_::Pair>
{
    using type = ::hgl::demo::composites::m_::d_::Pair;
};
template <>
struct hglrt::parents<::hgl::demo::composites::m_::Pair>
{
    using type = std::tuple<::hgl::demo::composites::m_::Sized<2>, ::hgl::demo::composites::m_::Holder<std::array<::hgraph::Int, 2>>>;
};

namespace hgl::demo::composites::m_
{

// struct Point (TYPE-3), module demo.composites, line 17
struct Point
{
    ::hgraph::Float x;
    ::hgraph::Float y;

    friend bool operator==(const Point&, const Point&) = default;
};

namespace d_
{
// delta<Point> (TYPE-16)
struct Point
{
    std::optional<::hgraph::Float> x;
    std::optional<::hgraph::Float> y;

    friend bool operator==(const Point&, const Point&) = default;
};
}  // namespace d_

// struct Line (TYPE-3), module demo.composites, line 10
struct Line
{
    ::hgl::demo::composites::m_::Point start;
    ::hgl::demo::composites::m_::Point end;
    ::hgraph::Int class_;
    std::optional<::hgraph::Str> note;

    friend bool operator==(const Line&, const Line&) = default;
};

namespace d_
{
// delta<Line> (TYPE-16)
struct Line
{
    std::optional<::hgl::demo::composites::m_::d_::Point> start;
    std::optional<::hgl::demo::composites::m_::d_::Point> end;
    std::optional<::hgraph::Int> class_;
    std::optional<std::optional<::hgraph::Str>> note;

    friend bool operator==(const Line&, const Line&) = default;
};
}  // namespace d_

// struct Book (TYPE-3), module demo.composites, line 22
struct Book
{
    ::hgraph::Str owner;
    ::hglrt::set<::hgraph::Str> tags;
    ::hglrt::dict<::hgraph::Str, ::hgraph::Int> prices;
    std::optional<::hgl::demo::composites::m_::Side> side;
    ::hglrt::list<::hgl::demo::composites::m_::Point> points;

    friend bool operator==(const Book&, const Book&) = default;
};

namespace d_
{
// delta<Book> (TYPE-16)
struct Book
{
    std::optional<::hgraph::Str> owner;
    std::optional<::hglrt::set_delta<::hgraph::Str>> tags;
    std::optional<::hglrt::dict_delta<::hgraph::Str, ::hgraph::Int>> prices;
    std::optional<std::optional<::hgl::demo::composites::m_::Side>> side;
    std::optional<::hglrt::list_delta<::hglrt::list<::hgl::demo::composites::m_::Point>>> points;

    friend bool operator==(const Book&, const Book&) = default;
};
}  // namespace d_

// struct Box (TYPE-3), module demo.composites, line 30
template <class T_>
struct Box
{
    T_ value;
    ::hgraph::Int count;

    friend bool operator==(const Box&, const Box&) = default;
};

namespace d_
{
// delta<Box> (TYPE-16)
template <class T_>
struct Box
{
    std::optional<::hglrt::delta_t<T_>> value;
    std::optional<::hgraph::Int> count;

    friend bool operator==(const Box&, const Box&) = default;
};
}  // namespace d_

// struct Empty (TYPE-3), module demo.composites, line 35
struct Empty
{
    friend bool operator==(const Empty&, const Empty&) = default;
};

namespace d_
{
// delta<Empty> (TYPE-16)
struct Empty
{
    friend bool operator==(const Empty&, const Empty&) = default;
};
}  // namespace d_

// struct Sided (TYPE-3), module demo.composites, line 38
template <::hgl::demo::composites::m_::Side side>
struct Sided
{
    ::hgraph::Int count;

    friend bool operator==(const Sided&, const Sided&) = default;
};

namespace d_
{
// delta<Sided> (TYPE-16)
template <::hgl::demo::composites::m_::Side side>
struct Sided
{
    std::optional<::hgraph::Int> count;

    friend bool operator==(const Sided&, const Sided&) = default;
};
}  // namespace d_

// struct Outcome (TYPE-3), module demo.composites, line 43
struct Outcome
{
    std::expected<::hgraph::Int, ::hgraph::Str> kept;

    friend bool operator==(const Outcome&, const Outcome&) = default;
};

namespace d_
{
// delta<Outcome> (TYPE-16)
struct Outcome
{
    std::optional<std::expected<::hgraph::Int, ::hgraph::Str>> kept;

    friend bool operator==(const Outcome&, const Outcome&) = default;
};
}  // namespace d_

// struct Flags (TYPE-3), module demo.composites, line 47
struct Flags
{
    ::hglrt::list<::hgraph::Bool> on;
    ::hgraph::Bool first;

    friend bool operator==(const Flags&, const Flags&) = default;
};

namespace d_
{
// delta<Flags> (TYPE-16)
struct Flags
{
    std::optional<::hglrt::list_delta<::hglrt::list<::hgraph::Bool>>> on;
    std::optional<::hgraph::Bool> first;

    friend bool operator==(const Flags&, const Flags&) = default;
};
}  // namespace d_

// abstract struct Base (TYPE-3), module demo.composites, line 54
class Base
{
  public:
    struct of_ : ::hglrt::held_value
    {
        [[nodiscard]] virtual const ::hgraph::Int& id() const = 0;
    };
    template <class S>
    struct as_ final : ::hglrt::held_as<of_, S>
    {
        using ::hglrt::held_as<of_, S>::held_as;
        [[nodiscard]] const ::hgraph::Int& id() const override { return this->held_.id; }
    };

    Base() = default;
    template <class S>
        requires ::hglrt::inherits<S, Base>
    Base(S value) : held_{std::make_shared<const as_<S>>(std::move(value))}
    {
    }

    [[nodiscard]] const ::hgraph::Int& id() const { return held_->id(); }
    friend bool operator==(const Base& lhs, const Base& rhs) { return lhs.held_->equal_(*rhs.held_); }

    std::shared_ptr<const of_> held_;
};

namespace d_
{
// delta<Base> (TYPE-16)
struct Base
{
    std::optional<::hgraph::Int> id;

    friend bool operator==(const Base&, const Base&) = default;
};
}  // namespace d_

// abstract struct Shape (TYPE-3), module demo.composites, line 58
class Shape
{
  public:
    struct of_ : ::hglrt::held_value
    {
        [[nodiscard]] virtual const ::hgraph::Int& id() const = 0;
        [[nodiscard]] virtual const ::hgraph::Str& name() const = 0;
        [[nodiscard]] virtual ::hgl::demo::composites::m_::Base as_0_() const = 0;
    };
    template <class S>
    struct as_ final : ::hglrt::held_as<of_, S>
    {
        using ::hglrt::held_as<of_, S>::held_as;
        [[nodiscard]] const ::hgraph::Int& id() const override { return this->held_.id; }
        [[nodiscard]] const ::hgraph::Str& name() const override { return this->held_.name; }
        [[nodiscard]] ::hgl::demo::composites::m_::Base as_0_() const override { return ::hgl::demo::composites::m_::Base(this->held_); }
    };

    Shape() = default;
    template <class S>
        requires ::hglrt::inherits<S, Shape>
    Shape(S value) : held_{std::make_shared<const as_<S>>(std::move(value))}
    {
    }

    [[nodiscard]] const ::hgraph::Int& id() const { return held_->id(); }
    [[nodiscard]] const ::hgraph::Str& name() const { return held_->name(); }
    operator ::hgl::demo::composites::m_::Base() const { return held_->as_0_(); }
    friend bool operator==(const Shape& lhs, const Shape& rhs) { return lhs.held_->equal_(*rhs.held_); }

    std::shared_ptr<const of_> held_;
};

namespace d_
{
// delta<Shape> (TYPE-16)
struct Shape
{
    std::optional<::hgraph::Int> id;
    std::optional<::hgraph::Str> name;

    friend bool operator==(const Shape&, const Shape&) = default;
};
}  // namespace d_

// struct Round (TYPE-3), module demo.composites, line 62
struct Round
{
    ::hgraph::Int id;
    ::hgraph::Str name;
    ::hgraph::Float radius;

    friend bool operator==(const Round&, const Round&) = default;
};

namespace d_
{
// delta<Round> (TYPE-16)
struct Round
{
    std::optional<::hgraph::Int> id;
    std::optional<::hgraph::Str> name;
    std::optional<::hgraph::Float> radius;

    friend bool operator==(const Round&, const Round&) = default;
};
}  // namespace d_

// struct Tagged (TYPE-3), module demo.composites, line 66
template <class T_>
struct Tagged
{
    ::hgraph::Int id;
    ::hgraph::Str name;
    T_ tag;

    friend bool operator==(const Tagged&, const Tagged&) = default;
};

namespace d_
{
// delta<Tagged> (TYPE-16)
template <class T_>
struct Tagged
{
    std::optional<::hgraph::Int> id;
    std::optional<::hgraph::Str> name;
    std::optional<::hglrt::delta_t<T_>> tag;

    friend bool operator==(const Tagged&, const Tagged&) = default;
};
}  // namespace d_

// abstract struct Noted (TYPE-3), module demo.composites, line 71
class Noted
{
  public:
    struct of_ : ::hglrt::held_value
    {
        [[nodiscard]] virtual const std::optional<::hgraph::Str>& note() const = 0;
    };
    template <class S>
    struct as_ final : ::hglrt::held_as<of_, S>
    {
        using ::hglrt::held_as<of_, S>::held_as;
        [[nodiscard]] const std::optional<::hgraph::Str>& note() const override { return this->held_.note; }
    };

    Noted() = default;
    template <class S>
        requires ::hglrt::inherits<S, Noted>
    Noted(S value) : held_{std::make_shared<const as_<S>>(std::move(value))}
    {
    }

    [[nodiscard]] const std::optional<::hgraph::Str>& note() const { return held_->note(); }
    friend bool operator==(const Noted& lhs, const Noted& rhs) { return lhs.held_->equal_(*rhs.held_); }

    std::shared_ptr<const of_> held_;
};

namespace d_
{
// delta<Noted> (TYPE-16)
struct Noted
{
    std::optional<std::optional<::hgraph::Str>> note;

    friend bool operator==(const Noted&, const Noted&) = default;
};
}  // namespace d_

// struct Memo (TYPE-3), module demo.composites, line 75
struct Memo
{
    std::optional<::hgraph::Str> note;

    friend bool operator==(const Memo&, const Memo&) = default;
};

namespace d_
{
// delta<Memo> (TYPE-16)
struct Memo
{
    std::optional<std::optional<::hgraph::Str>> note;

    friend bool operator==(const Memo&, const Memo&) = default;
};
}  // namespace d_

// abstract struct Holder (TYPE-3), module demo.composites, line 78
template <class T_>
class Holder
{
  public:
    struct of_ : ::hglrt::held_value
    {
        [[nodiscard]] virtual const T_& value() const = 0;
    };
    template <class S>
    struct as_ final : ::hglrt::held_as<of_, S>
    {
        using ::hglrt::held_as<of_, S>::held_as;
        [[nodiscard]] const T_& value() const override { return this->held_.value; }
    };

    Holder() = default;
    template <class S>
        requires ::hglrt::inherits<S, Holder>
    Holder(S value) : held_{std::make_shared<const as_<S>>(std::move(value))}
    {
    }

    [[nodiscard]] const T_& value() const { return held_->value(); }
    friend bool operator==(const Holder& lhs, const Holder& rhs) { return lhs.held_->equal_(*rhs.held_); }

    std::shared_ptr<const of_> held_;
};

namespace d_
{
// delta<Holder> (TYPE-16)
template <class T_>
struct Holder
{
    std::optional<::hglrt::delta_t<T_>> value;

    friend bool operator==(const Holder&, const Holder&) = default;
};
}  // namespace d_

// abstract struct Sized (TYPE-3), module demo.composites, line 82
template <::hgraph::Int n>
class Sized
{
  public:
    struct of_ : ::hglrt::held_value
    {
        [[nodiscard]] virtual const std::array<::hgraph::Int, n>& value() const = 0;
        [[nodiscard]] virtual ::hgl::demo::composites::m_::Holder<std::array<::hgraph::Int, n>> as_0_() const = 0;
    };
    template <class S>
    struct as_ final : ::hglrt::held_as<of_, S>
    {
        using ::hglrt::held_as<of_, S>::held_as;
        [[nodiscard]] const std::array<::hgraph::Int, n>& value() const override { return this->held_.value; }
        [[nodiscard]] ::hgl::demo::composites::m_::Holder<std::array<::hgraph::Int, n>> as_0_() const override { return ::hgl::demo::composites::m_::Holder<std::array<::hgraph::Int, n>>(this->held_); }
    };

    Sized() = default;
    template <class S>
        requires ::hglrt::inherits<S, Sized>
    Sized(S value) : held_{std::make_shared<const as_<S>>(std::move(value))}
    {
    }

    [[nodiscard]] const std::array<::hgraph::Int, n>& value() const { return held_->value(); }
    operator ::hgl::demo::composites::m_::Holder<std::array<::hgraph::Int, n>>() const { return held_->as_0_(); }
    friend bool operator==(const Sized& lhs, const Sized& rhs) { return lhs.held_->equal_(*rhs.held_); }

    std::shared_ptr<const of_> held_;
};

namespace d_
{
// delta<Sized> (TYPE-16)
template <::hgraph::Int n>
struct Sized
{
    std::optional<::hglrt::list_delta<std::array<::hgraph::Int, n>>> value;

    friend bool operator==(const Sized&, const Sized&) = default;
};
}  // namespace d_

// struct Pair (TYPE-3), module demo.composites, line 84
struct Pair
{
    std::array<::hgraph::Int, 2> value;

    friend bool operator==(const Pair&, const Pair&) = default;
};

namespace d_
{
// delta<Pair> (TYPE-16)
struct Pair
{
    std::optional<::hglrt::list_delta<std::array<::hgraph::Int, 2>>> value;

    friend bool operator==(const Pair&, const Pair&) = default;
};
}  // namespace d_

}  // namespace hgl::demo::composites::m_

template <>
struct hglrt::fields<::hgl::demo::composites::m_::Point>
{
    static constexpr auto members = std::tuple{&::hgl::demo::composites::m_::Point::x, &::hgl::demo::composites::m_::Point::y};
};
template <>
struct hglrt::fields<::hgl::demo::composites::m_::d_::Point>
{
    static constexpr auto members = std::tuple{&::hgl::demo::composites::m_::d_::Point::x, &::hgl::demo::composites::m_::d_::Point::y};
};
template <>
struct std::hash<::hgl::demo::composites::m_::Point>
{
    std::size_t operator()(const ::hgl::demo::composites::m_::Point& value) const { return ::hglrt::key_hash_of(value); }
};
template <>
struct std::hash<::hgl::demo::composites::m_::d_::Point>
{
    std::size_t operator()(const ::hgl::demo::composites::m_::d_::Point& value) const { return ::hglrt::key_hash_of(value); }
};
// TYPE-9: `Point` as hgraph's nominal bundle, its fields as `fields` names them.
template <>
struct hgraph::native_bundle<::hgl::demo::composites::m_::Point>
{
    using schema = ::hgraph::NominalBundle<"demo.composites", "Point", false, ::hgraph::BundleParents<>, ::hgraph::BundleArguments<>, ::hgraph::Field<"x", ::hgraph::Float>, ::hgraph::Field<"y", ::hgraph::Float>>;
    static constexpr auto members = ::hglrt::fields<::hgl::demo::composites::m_::Point>::members;
};
template <>
struct hglrt::type_named<::hgl::demo::composites::m_::Point>
{
    static constexpr std::string_view name = "demo.composites::Point";
    static constexpr std::array<std::string_view, 2> fields{"x", "y"};
    static constexpr bool delta = false;
};
template <>
struct hglrt::type_named<::hgl::demo::composites::m_::d_::Point>
{
    static constexpr std::string_view name = "delta<demo.composites::Point>";
    static constexpr std::array<std::string_view, 2> fields{"x", "y"};
    static constexpr bool delta = true;
};
template <>
struct hglrt::fields<::hgl::demo::composites::m_::Line>
{
    static constexpr auto members = std::tuple{&::hgl::demo::composites::m_::Line::start, &::hgl::demo::composites::m_::Line::end, &::hgl::demo::composites::m_::Line::class_, &::hgl::demo::composites::m_::Line::note};
};
template <>
struct hglrt::fields<::hgl::demo::composites::m_::d_::Line>
{
    static constexpr auto members = std::tuple{&::hgl::demo::composites::m_::d_::Line::start, &::hgl::demo::composites::m_::d_::Line::end, &::hgl::demo::composites::m_::d_::Line::class_, &::hgl::demo::composites::m_::d_::Line::note};
};
template <>
struct std::hash<::hgl::demo::composites::m_::Line>
{
    std::size_t operator()(const ::hgl::demo::composites::m_::Line& value) const { return ::hglrt::key_hash_of(value); }
};
template <>
struct std::hash<::hgl::demo::composites::m_::d_::Line>
{
    std::size_t operator()(const ::hgl::demo::composites::m_::d_::Line& value) const { return ::hglrt::key_hash_of(value); }
};
// TYPE-9: `Line` as hgraph's nominal bundle, its fields as `fields` names them.
template <>
struct hgraph::native_bundle<::hgl::demo::composites::m_::Line>
{
    using schema = ::hgraph::NominalBundle<"demo.composites", "Line", false, ::hgraph::BundleParents<>, ::hgraph::BundleArguments<>, ::hgraph::Field<"start", ::hgraph::native_bundle<::hgl::demo::composites::m_::Point>::schema>, ::hgraph::Field<"end", ::hgraph::native_bundle<::hgl::demo::composites::m_::Point>::schema>, ::hgraph::Field<"class", ::hgraph::Int>, ::hgraph::Field<"note", ::hgraph::Optional<::hgraph::Str>>>;
    static constexpr auto members = ::hglrt::fields<::hgl::demo::composites::m_::Line>::members;
};
template <>
struct hglrt::type_named<::hgl::demo::composites::m_::Line>
{
    static constexpr std::string_view name = "demo.composites::Line";
    static constexpr std::array<std::string_view, 4> fields{"start", "end", "class", "note"};
    static constexpr bool delta = false;
};
template <>
struct hglrt::type_named<::hgl::demo::composites::m_::d_::Line>
{
    static constexpr std::string_view name = "delta<demo.composites::Line>";
    static constexpr std::array<std::string_view, 4> fields{"start", "end", "class", "note"};
    static constexpr bool delta = true;
};
template <>
struct hglrt::fields<::hgl::demo::composites::m_::Book>
{
    static constexpr auto members = std::tuple{&::hgl::demo::composites::m_::Book::owner, &::hgl::demo::composites::m_::Book::tags, &::hgl::demo::composites::m_::Book::prices, &::hgl::demo::composites::m_::Book::side, &::hgl::demo::composites::m_::Book::points};
};
template <>
struct hglrt::fields<::hgl::demo::composites::m_::d_::Book>
{
    static constexpr auto members = std::tuple{&::hgl::demo::composites::m_::d_::Book::owner, &::hgl::demo::composites::m_::d_::Book::tags, &::hgl::demo::composites::m_::d_::Book::prices, &::hgl::demo::composites::m_::d_::Book::side, &::hgl::demo::composites::m_::d_::Book::points};
};
template <>
struct std::hash<::hgl::demo::composites::m_::Book>
{
    std::size_t operator()(const ::hgl::demo::composites::m_::Book& value) const { return ::hglrt::key_hash_of(value); }
};
template <>
struct std::hash<::hgl::demo::composites::m_::d_::Book>
{
    std::size_t operator()(const ::hgl::demo::composites::m_::d_::Book& value) const { return ::hglrt::key_hash_of(value); }
};
template <class T_>
struct hglrt::fields<::hgl::demo::composites::m_::Box<T_>>
{
    static constexpr auto members = std::tuple{&::hgl::demo::composites::m_::Box<T_>::value, &::hgl::demo::composites::m_::Box<T_>::count};
};
template <class T_>
struct hglrt::fields<::hgl::demo::composites::m_::d_::Box<T_>>
{
    static constexpr auto members = std::tuple{&::hgl::demo::composites::m_::d_::Box<T_>::value, &::hgl::demo::composites::m_::d_::Box<T_>::count};
};
template <class T_>
struct std::hash<::hgl::demo::composites::m_::Box<T_>>
{
    std::size_t operator()(const ::hgl::demo::composites::m_::Box<T_>& value) const { return ::hglrt::key_hash_of(value); }
};
template <class T_>
struct std::hash<::hgl::demo::composites::m_::d_::Box<T_>>
{
    std::size_t operator()(const ::hgl::demo::composites::m_::d_::Box<T_>& value) const { return ::hglrt::key_hash_of(value); }
};
template <>
struct hglrt::fields<::hgl::demo::composites::m_::Empty>
{
    static constexpr auto members = std::tuple{};
};
template <>
struct hglrt::fields<::hgl::demo::composites::m_::d_::Empty>
{
    static constexpr auto members = std::tuple{};
};
template <>
struct std::hash<::hgl::demo::composites::m_::Empty>
{
    std::size_t operator()(const ::hgl::demo::composites::m_::Empty& value) const { return ::hglrt::key_hash_of(value); }
};
template <>
struct std::hash<::hgl::demo::composites::m_::d_::Empty>
{
    std::size_t operator()(const ::hgl::demo::composites::m_::d_::Empty& value) const { return ::hglrt::key_hash_of(value); }
};
template <::hgl::demo::composites::m_::Side side>
struct hglrt::fields<::hgl::demo::composites::m_::Sided<side>>
{
    static constexpr auto members = std::tuple{&::hgl::demo::composites::m_::Sided<side>::count};
};
template <::hgl::demo::composites::m_::Side side>
struct hglrt::fields<::hgl::demo::composites::m_::d_::Sided<side>>
{
    static constexpr auto members = std::tuple{&::hgl::demo::composites::m_::d_::Sided<side>::count};
};
template <::hgl::demo::composites::m_::Side side>
struct std::hash<::hgl::demo::composites::m_::Sided<side>>
{
    std::size_t operator()(const ::hgl::demo::composites::m_::Sided<side>& value) const { return ::hglrt::key_hash_of(value); }
};
template <::hgl::demo::composites::m_::Side side>
struct std::hash<::hgl::demo::composites::m_::d_::Sided<side>>
{
    std::size_t operator()(const ::hgl::demo::composites::m_::d_::Sided<side>& value) const { return ::hglrt::key_hash_of(value); }
};
template <>
struct hglrt::fields<::hgl::demo::composites::m_::Outcome>
{
    static constexpr auto members = std::tuple{&::hgl::demo::composites::m_::Outcome::kept};
};
template <>
struct hglrt::fields<::hgl::demo::composites::m_::d_::Outcome>
{
    static constexpr auto members = std::tuple{&::hgl::demo::composites::m_::d_::Outcome::kept};
};
template <>
struct std::hash<::hgl::demo::composites::m_::Outcome>
{
    std::size_t operator()(const ::hgl::demo::composites::m_::Outcome& value) const { return ::hglrt::key_hash_of(value); }
};
template <>
struct std::hash<::hgl::demo::composites::m_::d_::Outcome>
{
    std::size_t operator()(const ::hgl::demo::composites::m_::d_::Outcome& value) const { return ::hglrt::key_hash_of(value); }
};
template <>
struct hglrt::fields<::hgl::demo::composites::m_::Flags>
{
    static constexpr auto members = std::tuple{&::hgl::demo::composites::m_::Flags::on, &::hgl::demo::composites::m_::Flags::first};
};
template <>
struct hglrt::fields<::hgl::demo::composites::m_::d_::Flags>
{
    static constexpr auto members = std::tuple{&::hgl::demo::composites::m_::d_::Flags::on, &::hgl::demo::composites::m_::d_::Flags::first};
};
template <>
struct std::hash<::hgl::demo::composites::m_::Flags>
{
    std::size_t operator()(const ::hgl::demo::composites::m_::Flags& value) const { return ::hglrt::key_hash_of(value); }
};
template <>
struct std::hash<::hgl::demo::composites::m_::d_::Flags>
{
    std::size_t operator()(const ::hgl::demo::composites::m_::d_::Flags& value) const { return ::hglrt::key_hash_of(value); }
};
// TYPE-9: `Flags` as hgraph's nominal bundle, its fields as `fields` names them.
template <>
struct hgraph::native_bundle<::hgl::demo::composites::m_::Flags>
{
    using schema = ::hgraph::NominalBundle<"demo.composites", "Flags", false, ::hgraph::BundleParents<>, ::hgraph::BundleArguments<>, ::hgraph::Field<"on", ::hglrt::list_value<::hgraph::Bool>>, ::hgraph::Field<"first", ::hgraph::Bool>>;
    static constexpr auto members = ::hglrt::fields<::hgl::demo::composites::m_::Flags>::members;
};
template <>
struct hglrt::type_named<::hgl::demo::composites::m_::Flags>
{
    static constexpr std::string_view name = "demo.composites::Flags";
    static constexpr std::array<std::string_view, 2> fields{"on", "first"};
    static constexpr bool delta = false;
};
template <>
struct hglrt::type_named<::hgl::demo::composites::m_::d_::Flags>
{
    static constexpr std::string_view name = "delta<demo.composites::Flags>";
    static constexpr std::array<std::string_view, 2> fields{"on", "first"};
    static constexpr bool delta = true;
};
template <>
struct hglrt::fields<::hgl::demo::composites::m_::d_::Base>
{
    static constexpr auto members = std::tuple{&::hgl::demo::composites::m_::d_::Base::id};
};
template <>
struct std::hash<::hgl::demo::composites::m_::Base>
{
    std::size_t operator()(const ::hgl::demo::composites::m_::Base& value) const { return ::hglrt::key_hash_of(value); }
};
template <>
struct std::hash<::hgl::demo::composites::m_::d_::Base>
{
    std::size_t operator()(const ::hgl::demo::composites::m_::d_::Base& value) const { return ::hglrt::key_hash_of(value); }
};
template <>
struct hglrt::fields<::hgl::demo::composites::m_::d_::Shape>
{
    static constexpr auto members = std::tuple{&::hgl::demo::composites::m_::d_::Shape::id, &::hgl::demo::composites::m_::d_::Shape::name};
};
template <>
struct std::hash<::hgl::demo::composites::m_::Shape>
{
    std::size_t operator()(const ::hgl::demo::composites::m_::Shape& value) const { return ::hglrt::key_hash_of(value); }
};
template <>
struct std::hash<::hgl::demo::composites::m_::d_::Shape>
{
    std::size_t operator()(const ::hgl::demo::composites::m_::d_::Shape& value) const { return ::hglrt::key_hash_of(value); }
};
template <>
struct hglrt::fields<::hgl::demo::composites::m_::Round>
{
    static constexpr auto members = std::tuple{&::hgl::demo::composites::m_::Round::id, &::hgl::demo::composites::m_::Round::name, &::hgl::demo::composites::m_::Round::radius};
};
template <>
struct hglrt::fields<::hgl::demo::composites::m_::d_::Round>
{
    static constexpr auto members = std::tuple{&::hgl::demo::composites::m_::d_::Round::id, &::hgl::demo::composites::m_::d_::Round::name, &::hgl::demo::composites::m_::d_::Round::radius};
};
template <>
struct std::hash<::hgl::demo::composites::m_::Round>
{
    std::size_t operator()(const ::hgl::demo::composites::m_::Round& value) const { return ::hglrt::key_hash_of(value); }
};
template <>
struct std::hash<::hgl::demo::composites::m_::d_::Round>
{
    std::size_t operator()(const ::hgl::demo::composites::m_::d_::Round& value) const { return ::hglrt::key_hash_of(value); }
};
// TYPE-9: `Round` as hgraph's nominal bundle, its fields as `fields` names them.
template <>
struct hgraph::native_bundle<::hgl::demo::composites::m_::Round>
{
    using schema = ::hgraph::NominalBundle<"demo.composites", "Round", false, ::hgraph::BundleParents<>, ::hgraph::BundleArguments<>, ::hgraph::Field<"id", ::hgraph::Int>, ::hgraph::Field<"name", ::hgraph::Str>, ::hgraph::Field<"radius", ::hgraph::Float>>;
    static constexpr auto members = ::hglrt::fields<::hgl::demo::composites::m_::Round>::members;
};
template <>
struct hglrt::type_named<::hgl::demo::composites::m_::Round>
{
    static constexpr std::string_view name = "demo.composites::Round";
    static constexpr std::array<std::string_view, 3> fields{"id", "name", "radius"};
    static constexpr bool delta = false;
};
template <>
struct hglrt::type_named<::hgl::demo::composites::m_::d_::Round>
{
    static constexpr std::string_view name = "delta<demo.composites::Round>";
    static constexpr std::array<std::string_view, 3> fields{"id", "name", "radius"};
    static constexpr bool delta = true;
};
template <class T_>
struct hglrt::fields<::hgl::demo::composites::m_::Tagged<T_>>
{
    static constexpr auto members = std::tuple{&::hgl::demo::composites::m_::Tagged<T_>::id, &::hgl::demo::composites::m_::Tagged<T_>::name, &::hgl::demo::composites::m_::Tagged<T_>::tag};
};
template <class T_>
struct hglrt::fields<::hgl::demo::composites::m_::d_::Tagged<T_>>
{
    static constexpr auto members = std::tuple{&::hgl::demo::composites::m_::d_::Tagged<T_>::id, &::hgl::demo::composites::m_::d_::Tagged<T_>::name, &::hgl::demo::composites::m_::d_::Tagged<T_>::tag};
};
template <class T_>
struct std::hash<::hgl::demo::composites::m_::Tagged<T_>>
{
    std::size_t operator()(const ::hgl::demo::composites::m_::Tagged<T_>& value) const { return ::hglrt::key_hash_of(value); }
};
template <class T_>
struct std::hash<::hgl::demo::composites::m_::d_::Tagged<T_>>
{
    std::size_t operator()(const ::hgl::demo::composites::m_::d_::Tagged<T_>& value) const { return ::hglrt::key_hash_of(value); }
};
template <>
struct hglrt::fields<::hgl::demo::composites::m_::d_::Noted>
{
    static constexpr auto members = std::tuple{&::hgl::demo::composites::m_::d_::Noted::note};
};
template <>
struct std::hash<::hgl::demo::composites::m_::Noted>
{
    std::size_t operator()(const ::hgl::demo::composites::m_::Noted& value) const { return ::hglrt::key_hash_of(value); }
};
template <>
struct std::hash<::hgl::demo::composites::m_::d_::Noted>
{
    std::size_t operator()(const ::hgl::demo::composites::m_::d_::Noted& value) const { return ::hglrt::key_hash_of(value); }
};
template <>
struct hglrt::fields<::hgl::demo::composites::m_::Memo>
{
    static constexpr auto members = std::tuple{&::hgl::demo::composites::m_::Memo::note};
};
template <>
struct hglrt::fields<::hgl::demo::composites::m_::d_::Memo>
{
    static constexpr auto members = std::tuple{&::hgl::demo::composites::m_::d_::Memo::note};
};
template <>
struct std::hash<::hgl::demo::composites::m_::Memo>
{
    std::size_t operator()(const ::hgl::demo::composites::m_::Memo& value) const { return ::hglrt::key_hash_of(value); }
};
template <>
struct std::hash<::hgl::demo::composites::m_::d_::Memo>
{
    std::size_t operator()(const ::hgl::demo::composites::m_::d_::Memo& value) const { return ::hglrt::key_hash_of(value); }
};
// TYPE-9: `Memo` as hgraph's nominal bundle, its fields as `fields` names them.
template <>
struct hgraph::native_bundle<::hgl::demo::composites::m_::Memo>
{
    using schema = ::hgraph::NominalBundle<"demo.composites", "Memo", false, ::hgraph::BundleParents<>, ::hgraph::BundleArguments<>, ::hgraph::Field<"note", ::hgraph::Optional<::hgraph::Str>>>;
    static constexpr auto members = ::hglrt::fields<::hgl::demo::composites::m_::Memo>::members;
};
template <>
struct hglrt::type_named<::hgl::demo::composites::m_::Memo>
{
    static constexpr std::string_view name = "demo.composites::Memo";
    static constexpr std::array<std::string_view, 1> fields{"note"};
    static constexpr bool delta = false;
};
template <>
struct hglrt::type_named<::hgl::demo::composites::m_::d_::Memo>
{
    static constexpr std::string_view name = "delta<demo.composites::Memo>";
    static constexpr std::array<std::string_view, 1> fields{"note"};
    static constexpr bool delta = true;
};
template <class T_>
struct hglrt::fields<::hgl::demo::composites::m_::d_::Holder<T_>>
{
    static constexpr auto members = std::tuple{&::hgl::demo::composites::m_::d_::Holder<T_>::value};
};
template <class T_>
struct std::hash<::hgl::demo::composites::m_::Holder<T_>>
{
    std::size_t operator()(const ::hgl::demo::composites::m_::Holder<T_>& value) const { return ::hglrt::key_hash_of(value); }
};
template <class T_>
struct std::hash<::hgl::demo::composites::m_::d_::Holder<T_>>
{
    std::size_t operator()(const ::hgl::demo::composites::m_::d_::Holder<T_>& value) const { return ::hglrt::key_hash_of(value); }
};
template <::hgraph::Int n>
struct hglrt::fields<::hgl::demo::composites::m_::d_::Sized<n>>
{
    static constexpr auto members = std::tuple{&::hgl::demo::composites::m_::d_::Sized<n>::value};
};
template <::hgraph::Int n>
struct std::hash<::hgl::demo::composites::m_::Sized<n>>
{
    std::size_t operator()(const ::hgl::demo::composites::m_::Sized<n>& value) const { return ::hglrt::key_hash_of(value); }
};
template <::hgraph::Int n>
struct std::hash<::hgl::demo::composites::m_::d_::Sized<n>>
{
    std::size_t operator()(const ::hgl::demo::composites::m_::d_::Sized<n>& value) const { return ::hglrt::key_hash_of(value); }
};
template <>
struct hglrt::fields<::hgl::demo::composites::m_::Pair>
{
    static constexpr auto members = std::tuple{&::hgl::demo::composites::m_::Pair::value};
};
template <>
struct hglrt::fields<::hgl::demo::composites::m_::d_::Pair>
{
    static constexpr auto members = std::tuple{&::hgl::demo::composites::m_::d_::Pair::value};
};
template <>
struct std::hash<::hgl::demo::composites::m_::Pair>
{
    std::size_t operator()(const ::hgl::demo::composites::m_::Pair& value) const { return ::hglrt::key_hash_of(value); }
};
template <>
struct std::hash<::hgl::demo::composites::m_::d_::Pair>
{
    std::size_t operator()(const ::hgl::demo::composites::m_::d_::Pair& value) const { return ::hglrt::key_hash_of(value); }
};
// TYPE-9: `Pair` as hgraph's nominal bundle, its fields as `fields` names them.
template <>
struct hgraph::native_bundle<::hgl::demo::composites::m_::Pair>
{
    using schema = ::hgraph::NominalBundle<"demo.composites", "Pair", false, ::hgraph::BundleParents<>, ::hgraph::BundleArguments<>, ::hgraph::Field<"value", ::hglrt::list_value<::hgraph::Int, 2>>>;
    static constexpr auto members = ::hglrt::fields<::hgl::demo::composites::m_::Pair>::members;
};
template <>
struct hglrt::type_named<::hgl::demo::composites::m_::Pair>
{
    static constexpr std::string_view name = "demo.composites::Pair";
    static constexpr std::array<std::string_view, 1> fields{"value"};
    static constexpr bool delta = false;
};
template <>
struct hglrt::type_named<::hgl::demo::composites::m_::d_::Pair>
{
    static constexpr std::string_view name = "delta<demo.composites::Pair>";
    static constexpr std::array<std::string_view, 1> fields{"value"};
    static constexpr bool delta = true;
};
