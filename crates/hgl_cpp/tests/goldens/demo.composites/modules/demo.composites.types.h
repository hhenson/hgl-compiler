// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#pragma once

#include <hglrt/hglrt.h>

namespace hgl::demo::composites::m_
{
struct Point;
struct Line;
struct Book;
template <class T_>
struct Box;
struct Empty;
class Base;
class Shape;
struct Round;
template <class T_>
struct Tagged;
namespace d_
{
struct Point;
struct Line;
struct Book;
template <class T_>
struct Box;
struct Empty;
struct Base;
struct Shape;
struct Round;
template <class T_>
struct Tagged;
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

namespace hgl::demo::composites::m_
{

// enum Side (TYPE-7)
enum class Side
{
    buy,
    sell,
};

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

// abstract struct Base (TYPE-3), module demo.composites, line 39
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

// abstract struct Shape (TYPE-3), module demo.composites, line 43
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

// struct Round (TYPE-3), module demo.composites, line 47
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

// struct Tagged (TYPE-3), module demo.composites, line 51
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
