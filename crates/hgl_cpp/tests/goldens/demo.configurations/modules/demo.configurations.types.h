// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#pragma once

#include <hglrt/hglrt.h>

// Names this file writes, which no macro of a header above takes.
#undef Holder
#undef Noted
#undef Tagged
#undef bid
#undef configurations
#undef demo
#undef members
#undef n
#undef note
#undef tags
#undef value
#undef xs

namespace hgl::demo::configurations::m_
{

struct Holder;
struct Tagged;
struct Noted;
namespace d_
{
struct Holder;
struct Tagged;
struct Noted;
}  // namespace d_
}  // namespace hgl::demo::configurations::m_

template <>
struct hglrt::delta_of<::hgl::demo::configurations::m_::Holder>
{
    using type = ::hgl::demo::configurations::m_::d_::Holder;
};
template <>
struct hglrt::delta_of<::hgl::demo::configurations::m_::Tagged>
{
    using type = ::hgl::demo::configurations::m_::d_::Tagged;
};
template <>
struct hglrt::delta_of<::hgl::demo::configurations::m_::Noted>
{
    using type = ::hgl::demo::configurations::m_::d_::Noted;
};

namespace hgl::demo::configurations::m_
{

// struct Holder (TYPE-3), module demo.configurations, line 8
struct Holder
{
    ::hglrt::list<::hgraph::Int> xs;
    ::hgraph::Int n;

    friend bool operator==(const struct Holder&, const struct Holder&) = default;
};

namespace d_
{
// delta<Holder> (TYPE-16)
struct Holder
{
    ::std::optional<::hglrt::list_delta<::hglrt::list<::hgraph::Int>>> xs;
    ::std::optional<::hgraph::Int> n;

    friend bool operator==(const struct Holder&, const struct Holder&) = default;
};
}  // namespace d_

// struct Tagged (TYPE-3), module demo.configurations, line 13
struct Tagged
{
    ::hglrt::set<::hgraph::Int> tags;
    ::hgraph::Int n;

    friend bool operator==(const struct Tagged&, const struct Tagged&) = default;
};

namespace d_
{
// delta<Tagged> (TYPE-16)
struct Tagged
{
    ::std::optional<::hglrt::set_delta<::hgraph::Int>> tags;
    ::std::optional<::hgraph::Int> n;

    friend bool operator==(const struct Tagged&, const struct Tagged&) = default;
};
}  // namespace d_

// struct Noted (TYPE-3), module demo.configurations, line 18
struct Noted
{
    ::hgraph::Int bid;
    ::std::optional<::hgraph::Str> note;

    friend bool operator==(const struct Noted&, const struct Noted&) = default;
};

namespace d_
{
// delta<Noted> (TYPE-16)
struct Noted
{
    ::std::optional<::hgraph::Int> bid;
    ::std::optional<::hglrt::nullable_delta<::hgraph::Str, ::hgraph::Str>> note;

    friend bool operator==(const struct Noted&, const struct Noted&) = default;
};
}  // namespace d_

}  // namespace hgl::demo::configurations::m_

template <>
struct hglrt::fields<::hgl::demo::configurations::m_::Holder>
{
    static constexpr auto members = ::std::tuple{&::hgl::demo::configurations::m_::Holder::xs, &::hgl::demo::configurations::m_::Holder::n};
};
template <>
struct hglrt::fields<::hgl::demo::configurations::m_::d_::Holder>
{
    static constexpr auto members = ::std::tuple{&::hgl::demo::configurations::m_::d_::Holder::xs, &::hgl::demo::configurations::m_::d_::Holder::n};
};
template <>
struct std::hash<::hgl::demo::configurations::m_::Holder>
{
    ::std::size_t operator()(const ::hgl::demo::configurations::m_::Holder& value) const { return ::hglrt::key_hash_of(value); }
};
template <>
struct std::hash<::hgl::demo::configurations::m_::d_::Holder>
{
    ::std::size_t operator()(const ::hgl::demo::configurations::m_::d_::Holder& value) const { return ::hglrt::key_hash_of(value); }
};
// TYPE-9: `Holder` as hgraph's nominal bundle, its fields as `fields` names them.
template <>
struct hgraph::native_bundle<::hgl::demo::configurations::m_::Holder>
{
    using schema = ::hgraph::NominalBundle<"demo.configurations", "Holder", false, ::hgraph::BundleParents<>, ::hgraph::BundleArguments<>, ::hgraph::Field<"xs", ::hglrt::list_value<::hgraph::Int>>, ::hgraph::Field<"n", ::hgraph::Int>>;
    static constexpr auto members = ::hglrt::fields<::hgl::demo::configurations::m_::Holder>::members;
};
template <>
struct hglrt::type_named<::hgl::demo::configurations::m_::Holder>
{
    static constexpr ::std::string_view name = "demo.configurations::Holder";
    static constexpr ::std::array<::std::string_view, 2> fields{"xs", "n"};
    static constexpr bool delta = false;
};
template <>
struct hglrt::type_named<::hgl::demo::configurations::m_::d_::Holder>
{
    static constexpr ::std::string_view name = "delta<demo.configurations::Holder>";
    static constexpr ::std::array<::std::string_view, 2> fields{"xs", "n"};
    static constexpr bool delta = true;
};
template <>
struct hglrt::fields<::hgl::demo::configurations::m_::Tagged>
{
    static constexpr auto members = ::std::tuple{&::hgl::demo::configurations::m_::Tagged::tags, &::hgl::demo::configurations::m_::Tagged::n};
};
template <>
struct hglrt::fields<::hgl::demo::configurations::m_::d_::Tagged>
{
    static constexpr auto members = ::std::tuple{&::hgl::demo::configurations::m_::d_::Tagged::tags, &::hgl::demo::configurations::m_::d_::Tagged::n};
};
template <>
struct std::hash<::hgl::demo::configurations::m_::Tagged>
{
    ::std::size_t operator()(const ::hgl::demo::configurations::m_::Tagged& value) const { return ::hglrt::key_hash_of(value); }
};
template <>
struct std::hash<::hgl::demo::configurations::m_::d_::Tagged>
{
    ::std::size_t operator()(const ::hgl::demo::configurations::m_::d_::Tagged& value) const { return ::hglrt::key_hash_of(value); }
};
template <>
struct hglrt::fields<::hgl::demo::configurations::m_::Noted>
{
    static constexpr auto members = ::std::tuple{&::hgl::demo::configurations::m_::Noted::bid, &::hgl::demo::configurations::m_::Noted::note};
};
template <>
struct hglrt::fields<::hgl::demo::configurations::m_::d_::Noted>
{
    static constexpr auto members = ::std::tuple{&::hgl::demo::configurations::m_::d_::Noted::bid, &::hgl::demo::configurations::m_::d_::Noted::note};
};
template <>
struct std::hash<::hgl::demo::configurations::m_::Noted>
{
    ::std::size_t operator()(const ::hgl::demo::configurations::m_::Noted& value) const { return ::hglrt::key_hash_of(value); }
};
template <>
struct std::hash<::hgl::demo::configurations::m_::d_::Noted>
{
    ::std::size_t operator()(const ::hgl::demo::configurations::m_::d_::Noted& value) const { return ::hglrt::key_hash_of(value); }
};
// TYPE-9: `Noted` as hgraph's nominal bundle, its fields as `fields` names them.
template <>
struct hgraph::native_bundle<::hgl::demo::configurations::m_::Noted>
{
    using schema = ::hgraph::NominalBundle<"demo.configurations", "Noted", false, ::hgraph::BundleParents<>, ::hgraph::BundleArguments<>, ::hgraph::Field<"bid", ::hgraph::Int>, ::hgraph::Field<"note", ::hgraph::Optional<::hgraph::Str>>>;
    static constexpr auto members = ::hglrt::fields<::hgl::demo::configurations::m_::Noted>::members;
};
template <>
struct hglrt::type_named<::hgl::demo::configurations::m_::Noted>
{
    static constexpr ::std::string_view name = "demo.configurations::Noted";
    static constexpr ::std::array<::std::string_view, 2> fields{"bid", "note"};
    static constexpr bool delta = false;
};
template <>
struct hglrt::type_named<::hgl::demo::configurations::m_::d_::Noted>
{
    static constexpr ::std::string_view name = "delta<demo.configurations::Noted>";
    static constexpr ::std::array<::std::string_view, 2> fields{"bid", "note"};
    static constexpr bool delta = true;
};
