// Written by the hgl compiler for hgraph's C++ API. Do not edit.
#pragma once

#include <hglrt/hglrt.h>

// Names this file writes, which no macro of a header above takes.
#undef Holding
#undef Inner
#undef MarketQuote
#undef NotedQuote
#undef Outer
#undef PlainQuote
#undef Wrapped
#undef ask
#undef bid
#undef count
#undef demo
#undef high
#undef inner
#undef low
#undef note
#undef noted
#undef quote
#undef size
#undef structs
#undef value

namespace hgl::demo::structs::m_
{

struct MarketQuote;
struct NotedQuote;
struct Inner;
struct Outer;
struct PlainQuote;
struct Wrapped;
struct Holding;
namespace d_
{
struct MarketQuote;
struct NotedQuote;
struct Inner;
struct Outer;
struct PlainQuote;
struct Wrapped;
struct Holding;
}  // namespace d_
}  // namespace hgl::demo::structs::m_

template <>
struct hglrt::delta_of<::hgl::demo::structs::m_::MarketQuote>
{
    using type = ::hgl::demo::structs::m_::d_::MarketQuote;
};
template <>
struct hglrt::delta_of<::hgl::demo::structs::m_::NotedQuote>
{
    using type = ::hgl::demo::structs::m_::d_::NotedQuote;
};
template <>
struct hglrt::delta_of<::hgl::demo::structs::m_::Inner>
{
    using type = ::hgl::demo::structs::m_::d_::Inner;
};
template <>
struct hglrt::delta_of<::hgl::demo::structs::m_::Outer>
{
    using type = ::hgl::demo::structs::m_::d_::Outer;
};
template <>
struct hglrt::delta_of<::hgl::demo::structs::m_::PlainQuote>
{
    using type = ::hgl::demo::structs::m_::d_::PlainQuote;
};
template <>
struct hglrt::delta_of<::hgl::demo::structs::m_::Wrapped>
{
    using type = ::hgl::demo::structs::m_::d_::Wrapped;
};
template <>
struct hglrt::delta_of<::hgl::demo::structs::m_::Holding>
{
    using type = ::hgl::demo::structs::m_::d_::Holding;
};

namespace hgl::demo::structs::m_
{

// struct MarketQuote (TYPE-3), module demo.structs, line 9
struct MarketQuote
{
    ::hgraph::Int bid;
    ::hgraph::Int ask;

    friend bool operator==(const struct MarketQuote&, const struct MarketQuote&) = default;
};

namespace d_
{
// delta<MarketQuote> (TYPE-16)
struct MarketQuote
{
    ::std::optional<::hgraph::Int> bid;
    ::std::optional<::hgraph::Int> ask;

    friend bool operator==(const struct MarketQuote&, const struct MarketQuote&) = default;
};
}  // namespace d_

// struct NotedQuote (TYPE-3), module demo.structs, line 135
struct NotedQuote
{
    ::hgraph::Int bid;
    ::std::optional<::hgraph::Str> note;

    friend bool operator==(const struct NotedQuote&, const struct NotedQuote&) = default;
};

namespace d_
{
// delta<NotedQuote> (TYPE-16)
struct NotedQuote
{
    ::std::optional<::hgraph::Int> bid;
    ::std::optional<::hglrt::nullable_delta<::hgraph::Str, ::hgraph::Str>> note;

    friend bool operator==(const struct NotedQuote&, const struct NotedQuote&) = default;
};
}  // namespace d_

// struct Inner (TYPE-3), module demo.structs, line 170
struct Inner
{
    ::hgraph::Int low;
    ::hgraph::Int high;

    friend bool operator==(const struct Inner&, const struct Inner&) = default;
};

namespace d_
{
// delta<Inner> (TYPE-16)
struct Inner
{
    ::std::optional<::hgraph::Int> low;
    ::std::optional<::hgraph::Int> high;

    friend bool operator==(const struct Inner&, const struct Inner&) = default;
};
}  // namespace d_

// struct Outer (TYPE-3), module demo.structs, line 175
struct Outer
{
    ::hgl::demo::structs::m_::Inner inner;
    ::hgraph::Int size;

    friend bool operator==(const struct Outer&, const struct Outer&) = default;
};

namespace d_
{
// delta<Outer> (TYPE-16)
struct Outer
{
    ::std::optional<::hgl::demo::structs::m_::d_::Inner> inner;
    ::std::optional<::hgraph::Int> size;

    friend bool operator==(const struct Outer&, const struct Outer&) = default;
};
}  // namespace d_

// struct PlainQuote (TYPE-3), module demo.structs, line 299
struct PlainQuote
{
    ::hgraph::Int bid;
    ::hgraph::Int ask;

    friend bool operator==(const struct PlainQuote&, const struct PlainQuote&) = default;
};

namespace d_
{
// delta<PlainQuote> (TYPE-16)
struct PlainQuote
{
    ::std::optional<::hgraph::Int> bid;
    ::std::optional<::hgraph::Int> ask;

    friend bool operator==(const struct PlainQuote&, const struct PlainQuote&) = default;
};
}  // namespace d_

// struct Wrapped (TYPE-3), module demo.structs, line 330
struct Wrapped
{
    ::hgl::demo::structs::m_::NotedQuote noted;
    ::hgraph::Int count;

    friend bool operator==(const struct Wrapped&, const struct Wrapped&) = default;
};

namespace d_
{
// delta<Wrapped> (TYPE-16)
struct Wrapped
{
    ::std::optional<::hgl::demo::structs::m_::d_::NotedQuote> noted;
    ::std::optional<::hgraph::Int> count;

    friend bool operator==(const struct Wrapped&, const struct Wrapped&) = default;
};
}  // namespace d_

// struct Holding (TYPE-3), module demo.structs, line 404
struct Holding
{
    ::hgl::demo::structs::m_::NotedQuote quote;
    ::hgraph::Int count;

    friend bool operator==(const struct Holding&, const struct Holding&) = default;
};

namespace d_
{
// delta<Holding> (TYPE-16)
struct Holding
{
    ::std::optional<::hgl::demo::structs::m_::NotedQuote> quote;
    ::std::optional<::hgraph::Int> count;

    friend bool operator==(const struct Holding&, const struct Holding&) = default;
};
}  // namespace d_

}  // namespace hgl::demo::structs::m_

template <>
struct hglrt::fields<::hgl::demo::structs::m_::MarketQuote>
{
    static constexpr auto members = ::std::tuple{&::hgl::demo::structs::m_::MarketQuote::bid, &::hgl::demo::structs::m_::MarketQuote::ask};
};
template <>
struct hglrt::fields<::hgl::demo::structs::m_::d_::MarketQuote>
{
    static constexpr auto members = ::std::tuple{&::hgl::demo::structs::m_::d_::MarketQuote::bid, &::hgl::demo::structs::m_::d_::MarketQuote::ask};
};
template <>
struct std::hash<::hgl::demo::structs::m_::MarketQuote>
{
    ::std::size_t operator()(const ::hgl::demo::structs::m_::MarketQuote& value) const { return ::hglrt::key_hash_of(value); }
};
template <>
struct std::hash<::hgl::demo::structs::m_::d_::MarketQuote>
{
    ::std::size_t operator()(const ::hgl::demo::structs::m_::d_::MarketQuote& value) const { return ::hglrt::key_hash_of(value); }
};
// TYPE-9: `MarketQuote` as hgraph's nominal bundle, its fields as `fields` names them.
template <>
struct hgraph::native_bundle<::hgl::demo::structs::m_::MarketQuote>
{
    using schema = ::hgraph::NominalBundle<"demo.structs", "MarketQuote", false, ::hgraph::BundleParents<>, ::hgraph::BundleArguments<>, ::hgraph::Field<"bid", ::hgraph::Int>, ::hgraph::Field<"ask", ::hgraph::Int>>;
    static constexpr auto members = ::hglrt::fields<::hgl::demo::structs::m_::MarketQuote>::members;
};
template <>
struct hglrt::type_named<::hgl::demo::structs::m_::MarketQuote>
{
    static constexpr ::std::string_view name = "demo.structs::MarketQuote";
    static constexpr ::std::array<::std::string_view, 2> fields{"bid", "ask"};
    static constexpr bool delta = false;
};
template <>
struct hglrt::type_named<::hgl::demo::structs::m_::d_::MarketQuote>
{
    static constexpr ::std::string_view name = "delta<demo.structs::MarketQuote>";
    static constexpr ::std::array<::std::string_view, 2> fields{"bid", "ask"};
    static constexpr bool delta = true;
};
template <>
struct hglrt::fields<::hgl::demo::structs::m_::NotedQuote>
{
    static constexpr auto members = ::std::tuple{&::hgl::demo::structs::m_::NotedQuote::bid, &::hgl::demo::structs::m_::NotedQuote::note};
};
template <>
struct hglrt::fields<::hgl::demo::structs::m_::d_::NotedQuote>
{
    static constexpr auto members = ::std::tuple{&::hgl::demo::structs::m_::d_::NotedQuote::bid, &::hgl::demo::structs::m_::d_::NotedQuote::note};
};
template <>
struct std::hash<::hgl::demo::structs::m_::NotedQuote>
{
    ::std::size_t operator()(const ::hgl::demo::structs::m_::NotedQuote& value) const { return ::hglrt::key_hash_of(value); }
};
template <>
struct std::hash<::hgl::demo::structs::m_::d_::NotedQuote>
{
    ::std::size_t operator()(const ::hgl::demo::structs::m_::d_::NotedQuote& value) const { return ::hglrt::key_hash_of(value); }
};
// TYPE-9: `NotedQuote` as hgraph's nominal bundle, its fields as `fields` names them.
template <>
struct hgraph::native_bundle<::hgl::demo::structs::m_::NotedQuote>
{
    using schema = ::hgraph::NominalBundle<"demo.structs", "NotedQuote", false, ::hgraph::BundleParents<>, ::hgraph::BundleArguments<>, ::hgraph::Field<"bid", ::hgraph::Int>, ::hgraph::Field<"note", ::hgraph::Optional<::hgraph::Str>>>;
    static constexpr auto members = ::hglrt::fields<::hgl::demo::structs::m_::NotedQuote>::members;
};
template <>
struct hglrt::type_named<::hgl::demo::structs::m_::NotedQuote>
{
    static constexpr ::std::string_view name = "demo.structs::NotedQuote";
    static constexpr ::std::array<::std::string_view, 2> fields{"bid", "note"};
    static constexpr bool delta = false;
};
template <>
struct hglrt::type_named<::hgl::demo::structs::m_::d_::NotedQuote>
{
    static constexpr ::std::string_view name = "delta<demo.structs::NotedQuote>";
    static constexpr ::std::array<::std::string_view, 2> fields{"bid", "note"};
    static constexpr bool delta = true;
};
template <>
struct hglrt::fields<::hgl::demo::structs::m_::Inner>
{
    static constexpr auto members = ::std::tuple{&::hgl::demo::structs::m_::Inner::low, &::hgl::demo::structs::m_::Inner::high};
};
template <>
struct hglrt::fields<::hgl::demo::structs::m_::d_::Inner>
{
    static constexpr auto members = ::std::tuple{&::hgl::demo::structs::m_::d_::Inner::low, &::hgl::demo::structs::m_::d_::Inner::high};
};
template <>
struct std::hash<::hgl::demo::structs::m_::Inner>
{
    ::std::size_t operator()(const ::hgl::demo::structs::m_::Inner& value) const { return ::hglrt::key_hash_of(value); }
};
template <>
struct std::hash<::hgl::demo::structs::m_::d_::Inner>
{
    ::std::size_t operator()(const ::hgl::demo::structs::m_::d_::Inner& value) const { return ::hglrt::key_hash_of(value); }
};
// TYPE-9: `Inner` as hgraph's nominal bundle, its fields as `fields` names them.
template <>
struct hgraph::native_bundle<::hgl::demo::structs::m_::Inner>
{
    using schema = ::hgraph::NominalBundle<"demo.structs", "Inner", false, ::hgraph::BundleParents<>, ::hgraph::BundleArguments<>, ::hgraph::Field<"low", ::hgraph::Int>, ::hgraph::Field<"high", ::hgraph::Int>>;
    static constexpr auto members = ::hglrt::fields<::hgl::demo::structs::m_::Inner>::members;
};
template <>
struct hglrt::type_named<::hgl::demo::structs::m_::Inner>
{
    static constexpr ::std::string_view name = "demo.structs::Inner";
    static constexpr ::std::array<::std::string_view, 2> fields{"low", "high"};
    static constexpr bool delta = false;
};
template <>
struct hglrt::type_named<::hgl::demo::structs::m_::d_::Inner>
{
    static constexpr ::std::string_view name = "delta<demo.structs::Inner>";
    static constexpr ::std::array<::std::string_view, 2> fields{"low", "high"};
    static constexpr bool delta = true;
};
template <>
struct hglrt::fields<::hgl::demo::structs::m_::Outer>
{
    static constexpr auto members = ::std::tuple{&::hgl::demo::structs::m_::Outer::inner, &::hgl::demo::structs::m_::Outer::size};
};
template <>
struct hglrt::fields<::hgl::demo::structs::m_::d_::Outer>
{
    static constexpr auto members = ::std::tuple{&::hgl::demo::structs::m_::d_::Outer::inner, &::hgl::demo::structs::m_::d_::Outer::size};
};
template <>
struct std::hash<::hgl::demo::structs::m_::Outer>
{
    ::std::size_t operator()(const ::hgl::demo::structs::m_::Outer& value) const { return ::hglrt::key_hash_of(value); }
};
template <>
struct std::hash<::hgl::demo::structs::m_::d_::Outer>
{
    ::std::size_t operator()(const ::hgl::demo::structs::m_::d_::Outer& value) const { return ::hglrt::key_hash_of(value); }
};
// TYPE-9: `Outer` as hgraph's nominal bundle, its fields as `fields` names them.
template <>
struct hgraph::native_bundle<::hgl::demo::structs::m_::Outer>
{
    using schema = ::hgraph::NominalBundle<"demo.structs", "Outer", false, ::hgraph::BundleParents<>, ::hgraph::BundleArguments<>, ::hgraph::Field<"inner", ::hgraph::native_bundle<::hgl::demo::structs::m_::Inner>::schema>, ::hgraph::Field<"size", ::hgraph::Int>>;
    static constexpr auto members = ::hglrt::fields<::hgl::demo::structs::m_::Outer>::members;
};
template <>
struct hglrt::type_named<::hgl::demo::structs::m_::Outer>
{
    static constexpr ::std::string_view name = "demo.structs::Outer";
    static constexpr ::std::array<::std::string_view, 2> fields{"inner", "size"};
    static constexpr bool delta = false;
};
template <>
struct hglrt::type_named<::hgl::demo::structs::m_::d_::Outer>
{
    static constexpr ::std::string_view name = "delta<demo.structs::Outer>";
    static constexpr ::std::array<::std::string_view, 2> fields{"inner", "size"};
    static constexpr bool delta = true;
};
template <>
struct hglrt::fields<::hgl::demo::structs::m_::PlainQuote>
{
    static constexpr auto members = ::std::tuple{&::hgl::demo::structs::m_::PlainQuote::bid, &::hgl::demo::structs::m_::PlainQuote::ask};
};
template <>
struct hglrt::fields<::hgl::demo::structs::m_::d_::PlainQuote>
{
    static constexpr auto members = ::std::tuple{&::hgl::demo::structs::m_::d_::PlainQuote::bid, &::hgl::demo::structs::m_::d_::PlainQuote::ask};
};
template <>
struct std::hash<::hgl::demo::structs::m_::PlainQuote>
{
    ::std::size_t operator()(const ::hgl::demo::structs::m_::PlainQuote& value) const { return ::hglrt::key_hash_of(value); }
};
template <>
struct std::hash<::hgl::demo::structs::m_::d_::PlainQuote>
{
    ::std::size_t operator()(const ::hgl::demo::structs::m_::d_::PlainQuote& value) const { return ::hglrt::key_hash_of(value); }
};
// TYPE-9: `PlainQuote` as hgraph's nominal bundle, its fields as `fields` names them.
template <>
struct hgraph::native_bundle<::hgl::demo::structs::m_::PlainQuote>
{
    using schema = ::hgraph::NominalBundle<"demo.structs", "PlainQuote", false, ::hgraph::BundleParents<>, ::hgraph::BundleArguments<>, ::hgraph::Field<"bid", ::hgraph::Int>, ::hgraph::Field<"ask", ::hgraph::Int>>;
    static constexpr auto members = ::hglrt::fields<::hgl::demo::structs::m_::PlainQuote>::members;
};
template <>
struct hglrt::type_named<::hgl::demo::structs::m_::PlainQuote>
{
    static constexpr ::std::string_view name = "demo.structs::PlainQuote";
    static constexpr ::std::array<::std::string_view, 2> fields{"bid", "ask"};
    static constexpr bool delta = false;
};
template <>
struct hglrt::type_named<::hgl::demo::structs::m_::d_::PlainQuote>
{
    static constexpr ::std::string_view name = "delta<demo.structs::PlainQuote>";
    static constexpr ::std::array<::std::string_view, 2> fields{"bid", "ask"};
    static constexpr bool delta = true;
};
template <>
struct hglrt::fields<::hgl::demo::structs::m_::Wrapped>
{
    static constexpr auto members = ::std::tuple{&::hgl::demo::structs::m_::Wrapped::noted, &::hgl::demo::structs::m_::Wrapped::count};
};
template <>
struct hglrt::fields<::hgl::demo::structs::m_::d_::Wrapped>
{
    static constexpr auto members = ::std::tuple{&::hgl::demo::structs::m_::d_::Wrapped::noted, &::hgl::demo::structs::m_::d_::Wrapped::count};
};
template <>
struct std::hash<::hgl::demo::structs::m_::Wrapped>
{
    ::std::size_t operator()(const ::hgl::demo::structs::m_::Wrapped& value) const { return ::hglrt::key_hash_of(value); }
};
template <>
struct std::hash<::hgl::demo::structs::m_::d_::Wrapped>
{
    ::std::size_t operator()(const ::hgl::demo::structs::m_::d_::Wrapped& value) const { return ::hglrt::key_hash_of(value); }
};
// TYPE-9: `Wrapped` as hgraph's nominal bundle, its fields as `fields` names them.
template <>
struct hgraph::native_bundle<::hgl::demo::structs::m_::Wrapped>
{
    using schema = ::hgraph::NominalBundle<"demo.structs", "Wrapped", false, ::hgraph::BundleParents<>, ::hgraph::BundleArguments<>, ::hgraph::Field<"noted", ::hgraph::native_bundle<::hgl::demo::structs::m_::NotedQuote>::schema>, ::hgraph::Field<"count", ::hgraph::Int>>;
    static constexpr auto members = ::hglrt::fields<::hgl::demo::structs::m_::Wrapped>::members;
};
template <>
struct hglrt::type_named<::hgl::demo::structs::m_::Wrapped>
{
    static constexpr ::std::string_view name = "demo.structs::Wrapped";
    static constexpr ::std::array<::std::string_view, 2> fields{"noted", "count"};
    static constexpr bool delta = false;
};
template <>
struct hglrt::type_named<::hgl::demo::structs::m_::d_::Wrapped>
{
    static constexpr ::std::string_view name = "delta<demo.structs::Wrapped>";
    static constexpr ::std::array<::std::string_view, 2> fields{"noted", "count"};
    static constexpr bool delta = true;
};
template <>
struct hglrt::fields<::hgl::demo::structs::m_::Holding>
{
    static constexpr auto members = ::std::tuple{&::hgl::demo::structs::m_::Holding::quote, &::hgl::demo::structs::m_::Holding::count};
};
template <>
struct hglrt::fields<::hgl::demo::structs::m_::d_::Holding>
{
    static constexpr auto members = ::std::tuple{&::hgl::demo::structs::m_::d_::Holding::quote, &::hgl::demo::structs::m_::d_::Holding::count};
};
template <>
struct std::hash<::hgl::demo::structs::m_::Holding>
{
    ::std::size_t operator()(const ::hgl::demo::structs::m_::Holding& value) const { return ::hglrt::key_hash_of(value); }
};
template <>
struct std::hash<::hgl::demo::structs::m_::d_::Holding>
{
    ::std::size_t operator()(const ::hgl::demo::structs::m_::d_::Holding& value) const { return ::hglrt::key_hash_of(value); }
};
// TYPE-9: `Holding` as hgraph's nominal bundle, its fields as `fields` names them.
template <>
struct hgraph::native_bundle<::hgl::demo::structs::m_::Holding>
{
    using schema = ::hgraph::NominalBundle<"demo.structs", "Holding", false, ::hgraph::BundleParents<>, ::hgraph::BundleArguments<>, ::hgraph::Field<"quote", ::hgraph::native_bundle<::hgl::demo::structs::m_::NotedQuote>::schema>, ::hgraph::Field<"count", ::hgraph::Int>>;
    static constexpr auto members = ::hglrt::fields<::hgl::demo::structs::m_::Holding>::members;
};
template <>
struct hglrt::type_named<::hgl::demo::structs::m_::Holding>
{
    static constexpr ::std::string_view name = "demo.structs::Holding";
    static constexpr ::std::array<::std::string_view, 2> fields{"quote", "count"};
    static constexpr bool delta = false;
};
template <>
struct hglrt::type_named<::hgl::demo::structs::m_::d_::Holding>
{
    static constexpr ::std::string_view name = "delta<demo.structs::Holding>";
    static constexpr ::std::array<::std::string_view, 2> fields{"quote", "count"};
    static constexpr bool delta = true;
};
