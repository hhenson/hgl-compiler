// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#pragma once

#include <hglrt/hglrt.h>

// Names this file writes, which no macro of a header above takes.
#undef Basket
#undef counts
#undef demo
#undef name
#undef nested
#undef value

namespace hgl::demo::nested::m_
{

struct Basket;
namespace d_
{
struct Basket;
}  // namespace d_
}  // namespace hgl::demo::nested::m_

template <>
struct hglrt::delta_of<::hgl::demo::nested::m_::Basket>
{
    using type = ::hgl::demo::nested::m_::d_::Basket;
};

namespace hgl::demo::nested::m_
{

// struct Basket (TYPE-3), module demo.nested, line 127
struct Basket
{
    ::hgraph::Str name;
    ::hglrt::list<::hgraph::Int> counts;

    friend bool operator==(const struct Basket&, const struct Basket&) = default;
};

namespace d_
{
// delta<Basket> (TYPE-16)
struct Basket
{
    ::std::optional<::hgraph::Str> name;
    ::std::optional<::hglrt::list_delta<::hglrt::list<::hgraph::Int>>> counts;

    friend bool operator==(const struct Basket&, const struct Basket&) = default;
};
}  // namespace d_

}  // namespace hgl::demo::nested::m_

template <>
struct hglrt::fields<::hgl::demo::nested::m_::Basket>
{
    static constexpr auto members = ::std::tuple{&::hgl::demo::nested::m_::Basket::name, &::hgl::demo::nested::m_::Basket::counts};
};
template <>
struct hglrt::fields<::hgl::demo::nested::m_::d_::Basket>
{
    static constexpr auto members = ::std::tuple{&::hgl::demo::nested::m_::d_::Basket::name, &::hgl::demo::nested::m_::d_::Basket::counts};
};
template <>
struct std::hash<::hgl::demo::nested::m_::Basket>
{
    ::std::size_t operator()(const ::hgl::demo::nested::m_::Basket& value) const { return ::hglrt::key_hash_of(value); }
};
template <>
struct std::hash<::hgl::demo::nested::m_::d_::Basket>
{
    ::std::size_t operator()(const ::hgl::demo::nested::m_::d_::Basket& value) const { return ::hglrt::key_hash_of(value); }
};
// TYPE-9: `Basket` as hgraph's nominal bundle, its fields as `fields` names them.
template <>
struct hgraph::native_bundle<::hgl::demo::nested::m_::Basket>
{
    using schema = ::hgraph::NominalBundle<"demo.nested", "Basket", false, ::hgraph::BundleParents<>, ::hgraph::BundleArguments<>, ::hgraph::Field<"name", ::hgraph::Str>, ::hgraph::Field<"counts", ::hglrt::list_value<::hgraph::Int>>>;
    static constexpr auto members = ::hglrt::fields<::hgl::demo::nested::m_::Basket>::members;
};
template <>
struct hglrt::type_named<::hgl::demo::nested::m_::Basket>
{
    static constexpr ::std::string_view name = "demo.nested::Basket";
    static constexpr ::std::array<::std::string_view, 2> fields{"name", "counts"};
    static constexpr bool delta = false;
};
template <>
struct hglrt::type_named<::hgl::demo::nested::m_::d_::Basket>
{
    static constexpr ::std::string_view name = "delta<demo.nested::Basket>";
    static constexpr ::std::array<::std::string_view, 2> fields{"name", "counts"};
    static constexpr bool delta = true;
};
