// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#pragma once

#include <hglrt/hglrt.h>

// Names this file writes, which no macro of a header above takes.
#undef Quote
#undef ask
#undef bid
#undef demo
#undef members
#undef store
#undef value

namespace hgl::demo::store::m_
{

struct Quote;
namespace d_
{
struct Quote;
}  // namespace d_
}  // namespace hgl::demo::store::m_

template <>
struct hglrt::delta_of<::hgl::demo::store::m_::Quote>
{
    using type = ::hgl::demo::store::m_::d_::Quote;
};

namespace hgl::demo::store::m_
{

// struct Quote (TYPE-3), module demo.store, line 8
struct Quote
{
    ::hgraph::Int bid;
    ::hgraph::Int ask;

    friend bool operator==(const Quote&, const Quote&) = default;
};

namespace d_
{
// delta<Quote> (TYPE-16)
struct Quote
{
    std::optional<::hgraph::Int> bid;
    std::optional<::hgraph::Int> ask;

    friend bool operator==(const Quote&, const Quote&) = default;
};
}  // namespace d_

}  // namespace hgl::demo::store::m_

template <>
struct hglrt::fields<::hgl::demo::store::m_::Quote>
{
    static constexpr auto members = std::tuple{&::hgl::demo::store::m_::Quote::bid, &::hgl::demo::store::m_::Quote::ask};
};
template <>
struct hglrt::fields<::hgl::demo::store::m_::d_::Quote>
{
    static constexpr auto members = std::tuple{&::hgl::demo::store::m_::d_::Quote::bid, &::hgl::demo::store::m_::d_::Quote::ask};
};
template <>
struct std::hash<::hgl::demo::store::m_::Quote>
{
    std::size_t operator()(const ::hgl::demo::store::m_::Quote& value) const { return ::hglrt::key_hash_of(value); }
};
template <>
struct std::hash<::hgl::demo::store::m_::d_::Quote>
{
    std::size_t operator()(const ::hgl::demo::store::m_::d_::Quote& value) const { return ::hglrt::key_hash_of(value); }
};
// TYPE-9: `Quote` as hgraph's nominal bundle, its fields as `fields` names them.
template <>
struct hgraph::native_bundle<::hgl::demo::store::m_::Quote>
{
    using schema = ::hgraph::NominalBundle<"demo.store", "Quote", false, ::hgraph::BundleParents<>, ::hgraph::BundleArguments<>, ::hgraph::Field<"bid", ::hgraph::Int>, ::hgraph::Field<"ask", ::hgraph::Int>>;
    static constexpr auto members = ::hglrt::fields<::hgl::demo::store::m_::Quote>::members;
};
template <>
struct hglrt::type_named<::hgl::demo::store::m_::Quote>
{
    static constexpr std::string_view name = "demo.store::Quote";
    static constexpr std::array<std::string_view, 2> fields{"bid", "ask"};
    static constexpr bool delta = false;
};
template <>
struct hglrt::type_named<::hgl::demo::store::m_::d_::Quote>
{
    static constexpr std::string_view name = "delta<demo.store::Quote>";
    static constexpr std::array<std::string_view, 2> fields{"bid", "ask"};
    static constexpr bool delta = true;
};
