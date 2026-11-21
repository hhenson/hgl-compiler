// Unit tests of what the run's store holds (capabilities.h, CAP-6: "a
// run-wide store of ordinary values under string keys"): every ordinary
// value, put and got back as itself, a struct among them, which hgraph's own
// schema of it, a bundle, would not give back; and a tuple, a set, a
// dictionary and a delta, which hgraph has no schema of its own for.
#include <hglrt/collections.h>
#include <hglrt/hglrt.h>
#include <hglrt/keyed.h>
#include <hglrt/structs.h>

#include <doctest/doctest.h>
#include <hgraph/runtime/global_state.h>
#include <hgraph/types/generated.h>

#include <cmath>
#include <optional>
#include <string>
#include <tuple>

namespace hglrt_store_test {

using hglrt::i64;

/// `struct Quote { bid: i64, ask: i64 }`, written as the compiler writes a
/// struct: an aggregate, `hglrt::fields` of it, and `hgraph::native_bundle`
/// of it, which makes its schema with hgraph a bundle.
struct Quote {
    i64 bid;
    i64 ask;
    friend bool operator==(const Quote&, const Quote&) = default;
};

} // namespace hglrt_store_test

namespace st = hglrt_store_test;

template <>
struct hglrt::fields<st::Quote> {
    static constexpr auto members = std::tuple{&st::Quote::bid, &st::Quote::ask};
};

template <>
struct hgraph::native_bundle<st::Quote> {
    using schema = NominalBundle<"test.store", "Quote", false, BundleParents<>, BundleArguments<>,
                                 Field<"bid", Int>, Field<"ask", Int>>;
    static constexpr auto members = ::hglrt::fields<st::Quote>::members;
};

namespace {

using hglrt::i64;

/// Puts `value` under `key` of a store of its own and gets it back.
template <class V>
V put_and_got(const V& value) {
    hgraph::GlobalState state;
    const hglrt::GlobalState store{state.view()};
    hglrt::put_GlobalState_str_V(store, "key", value);
    return hglrt::get_GlobalState_str<V>(store, "key");
}

} // namespace

TEST_CASE("a struct put in the store is got back as itself (CAP-6)") {
    CHECK(put_and_got(st::Quote{1, 2}) == st::Quote{1, 2});
    // A key keeps the type of its first value.
    hgraph::GlobalState state;
    const hglrt::GlobalState store{state.view()};
    hglrt::put_GlobalState_str_V(store, "quote", st::Quote{1, 2});
    CHECK_THROWS_AS((void)hglrt::get_GlobalState_str<i64>(store, "quote"), hglrt::Failure);
    CHECK_THROWS_AS(hglrt::put_GlobalState_str_V(store, "quote", i64{3}), hglrt::Failure);
    CHECK(hglrt::get_GlobalState_str<st::Quote>(store, "quote") == st::Quote{1, 2});
}

TEST_CASE("a tuple, a set, a dictionary and a delta put in the store are got back as themselves (CAP-6)") {
    using pair = std::tuple<i64, hglrt::str>;
    CHECK(put_and_got(pair{1, "x"}) == pair{1, "x"});
    CHECK(put_and_got(hglrt::set<i64>{1, 2}) == hglrt::set<i64>{1, 2});
    hglrt::dict<hglrt::str, i64> book;
    book.emplace("a", 1);
    CHECK(put_and_got(book) == book);
    hglrt::set_delta<i64> change;
    change.added.insert(3);
    CHECK(put_and_got(change).added == change.added);
    // A nullable's null is a value the key holds.
    CHECK_FALSE(put_and_got(std::optional<i64>{}).has_value());
}

TEST_CASE("a value in the store is a copy, and keeps what it was put with (CAP-6)") {
    hgraph::GlobalState state;
    const hglrt::GlobalState store{state.view()};
    hglrt::set<i64> members{1};
    hglrt::put_GlobalState_str_V(store, "members", members);
    members.insert(2);
    CHECK(hglrt::get_GlobalState_str<hglrt::set<i64>>(store, "members") == hglrt::set<i64>{1});
    // Both zeros are kept as they are.
    hglrt::put_GlobalState_str_V(store, "zero", -0.0);
    CHECK(std::signbit(hglrt::get_GlobalState_str<hglrt::f64>(store, "zero")));
}
