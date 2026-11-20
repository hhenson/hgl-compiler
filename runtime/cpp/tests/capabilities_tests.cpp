// Unit tests of the global store's natives (capabilities.h) and of the list
// natives (lists.h).
#include <hglrt/capabilities.h>
#include <hglrt/lists.h>

#include <doctest/doctest.h>

#include <tuple>

TEST_CASE("the global store keeps a copy of each value under its key (CAP-6)") {
    hgraph::GlobalState state;
    const hgraph::GlobalStateView store = state.view();
    CHECK_FALSE(hglrt::has_GlobalState_str(store, "limit"));
    hglrt::put_GlobalState_str_V(store, "limit", hglrt::i64{3});
    CHECK(hglrt::has_GlobalState_str(store, "limit"));
    CHECK(hglrt::get_GlobalState_str<hglrt::i64>(store, "limit") == 3);
    hglrt::put_GlobalState_str_V(store, "limit", hglrt::i64{4});
    CHECK(hglrt::get_GlobalState_str<hglrt::i64>(store, "limit") == 4);

    hglrt::list<std::tuple<hglrt::datetime, hglrt::i64>> entries{{hglrt::datetime{}, 1}};
    hglrt::put_GlobalState_str_V(store, "entries", entries);
    entries.emplace_back(hglrt::datetime{}, 2);
    CHECK(hglrt::get_GlobalState_str<decltype(entries)>(store, "entries").size() == 1);
}

TEST_CASE("an absent key, and a value of another type, fail with no code (CAP-6)") {
    hgraph::GlobalState state;
    const hgraph::GlobalStateView store = state.view();
    try {
        (void)hglrt::get_GlobalState_str<hglrt::i64>(store, "absent");
        FAIL("nothing was raised");
    } catch (const hglrt::Failure& failure) {
        CHECK_FALSE(failure.error().code.has_value());
        CHECK(failure.error().message == "the global store holds nothing under 'absent'");
    }
    hglrt::put_GlobalState_str_V(store, "limit", hglrt::i64{3});
    try {
        (void)hglrt::get_GlobalState_str<hglrt::str>(store, "limit");
        FAIL("nothing was raised");
    } catch (const hglrt::Failure& failure) {
        CHECK(failure.error().message == "the global store holds a value of another type under 'limit'");
    }
    // "A key keeps the type of its first value".
    CHECK_THROWS_AS(hglrt::put_GlobalState_str_V(store, "limit", hglrt::str{"three"}), hglrt::Failure);
    CHECK(hglrt::get_GlobalState_str<hglrt::i64>(store, "limit") == 3);
}

TEST_CASE("append and extend grow the place they are given (LIST-1 to LIST-3)") {
    hglrt::list<hglrt::i64> values{1, 2};
    const hglrt::list<hglrt::i64> before = values;
    hglrt::fn_append(values, 3);
    CHECK(values == hglrt::list<hglrt::i64>{1, 2, 3});
    CHECK(before == hglrt::list<hglrt::i64>{1, 2});
    hglrt::fn_extend(values, hglrt::list<hglrt::i64>{});
    CHECK(values.size() == 3);
    hglrt::fn_extend(values, hglrt::list<hglrt::i64>{4, 5});
    CHECK(values == hglrt::list<hglrt::i64>{1, 2, 3, 4, 5});
    hglrt::fn_extend(values, values);
    CHECK(values == hglrt::list<hglrt::i64>{1, 2, 3, 4, 5, 1, 2, 3, 4, 5});
}
