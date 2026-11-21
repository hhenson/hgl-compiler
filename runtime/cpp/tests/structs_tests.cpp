// Unit tests of the temporal forms of a struct, a tuple, an atomic struct and
// a nullable scalar (structs.h, nullables.h), on nodes written by hand as the
// compiler writes them, with hgraph's front end for generated code alone,
// each run in an hgraph graph in simulation. The struct is written as the
// compiler writes one: an aggregate, its delta, `hglrt::fields` of each, and
// `hgraph::native_bundle` of it.
#include "core_nodes.h"

#include <hglrt/nullables.h>
#include <hglrt/structs.h>

#include <doctest/doctest.h>
#include <hgraph/types/generated.h>

#include <chrono>
#include <cstddef>
#include <functional>
#include <optional>
#include <string>
#include <string_view>
#include <tuple>
#include <vector>

namespace hglrt_structs_test {

using hglrt::i64;

/// `struct Quote { bid: i64, ask: i64 }`.
struct Quote {
    i64 bid;
    i64 ask;

    friend bool operator==(const Quote&, const Quote&) = default;
};

namespace d_ {
/// `delta<Quote>`.
struct Quote {
    std::optional<i64> bid;
    std::optional<i64> ask;

    friend bool operator==(const Quote&, const Quote&) = default;
};
} // namespace d_

/// `struct Book { top: Quote, size: i64 }`.
struct Book {
    Quote top;
    i64 size;

    friend bool operator==(const Book&, const Book&) = default;
};

namespace d_ {
/// `delta<Book>`.
struct Book {
    std::optional<d_::Quote> top;
    std::optional<i64> size;

    friend bool operator==(const Book&, const Book&) = default;
};
} // namespace d_

/// `struct Envelope { quote: atomic<Quote>, size: i64 }`: an atomic field
/// holds its payload, and its delta's property holds the payload too.
struct Envelope {
    Quote quote;
    i64 size;

    friend bool operator==(const Envelope&, const Envelope&) = default;
};

namespace d_ {
/// `delta<Envelope>`.
struct Envelope {
    std::optional<hglrt_structs_test::Quote> quote;
    std::optional<i64> size;

    friend bool operator==(const Envelope&, const Envelope&) = default;
};
} // namespace d_

} // namespace hglrt_structs_test

namespace t = hglrt_structs_test;

template <>
struct hglrt::delta_of<t::Envelope> {
    using type = t::d_::Envelope;
};
template <>
struct hglrt::fields<t::Envelope> {
    static constexpr auto members = std::tuple{&t::Envelope::quote, &t::Envelope::size};
};
template <>
struct hglrt::fields<t::d_::Envelope> {
    static constexpr auto members = std::tuple{&t::d_::Envelope::quote, &t::d_::Envelope::size};
};

template <>
struct hglrt::delta_of<t::Quote> {
    using type = t::d_::Quote;
};
template <>
struct hglrt::delta_of<t::Book> {
    using type = t::d_::Book;
};
template <>
struct hglrt::fields<t::Quote> {
    static constexpr auto members = std::tuple{&t::Quote::bid, &t::Quote::ask};
};
template <>
struct hglrt::fields<t::d_::Quote> {
    static constexpr auto members = std::tuple{&t::d_::Quote::bid, &t::d_::Quote::ask};
};
template <>
struct hglrt::fields<t::Book> {
    static constexpr auto members = std::tuple{&t::Book::top, &t::Book::size};
};
template <>
struct hglrt::fields<t::d_::Book> {
    static constexpr auto members = std::tuple{&t::d_::Book::top, &t::d_::Book::size};
};
template <>
struct std::hash<t::Quote> {
    std::size_t operator()(const t::Quote& value) const { return ::hglrt::key_hash_of(value); }
};
template <>
struct std::hash<t::d_::Quote> {
    std::size_t operator()(const t::d_::Quote& value) const { return ::hglrt::key_hash_of(value); }
};
template <>
struct hgraph::native_bundle<t::Quote> {
    using schema = NominalBundle<"test.structs", "Quote", false, BundleParents<>, BundleArguments<>,
                                 Field<"bid", Int>, Field<"ask", Int>>;
    static constexpr auto members = ::hglrt::fields<t::Quote>::members;
};
template <>
struct hgraph::native_bundle<t::Book> {
    using schema = NominalBundle<"test.structs", "Book", false, BundleParents<>, BundleArguments<>,
                                 Field<"top", native_bundle<t::Quote>::schema>, Field<"size", Int>>;
    static constexpr auto members = ::hglrt::fields<t::Book>::members;
};
template <>
struct hgraph::native_bundle<t::Envelope> {
    using schema = NominalBundle<"test.structs", "Envelope", false, BundleParents<>, BundleArguments<>,
                                 Field<"quote", native_bundle<t::Quote>::schema>, Field<"size", Int>>;
    static constexpr auto members = ::hglrt::fields<t::Envelope>::members;
};
template <>
struct hglrt::type_named<t::Quote> {
    static constexpr std::string_view name = "test.structs::Quote";
};
template <>
struct hglrt::type_named<t::d_::Quote> {
    static constexpr std::string_view name = "delta<test.structs::Quote>";
};

namespace {

using hglrt::datetime;
using hglrt::i64;
using hglrt::testing::Dense;
using namespace std::chrono_literals;
using QuoteTS = hgraph::NativeTSB<t::Quote>;
using BookTS = hgraph::NativeTSB<t::Book>;
using PairTS = hgraph::UnNamedTSB<hgraph::Field<"0", hgraph::TS<i64>>, hgraph::Field<"1", hgraph::TS<i64>>>;

const datetime start = hglrt::testing::earliest_start();

/// The connection core's `replay` makes of `trace`.
hgraph::Port<hgraph::TS<i64>> replayed(hgraph::Wiring& wiring, const Dense<i64>& trace) {
    return hgraph::wire<hglrt_test::Replay>(wiring, hglrt::testing::replay_items(trace, start));
}

/// `fn update_quote(is_bid: i64, price: i64) -> Quote { when { if is_bid ==
/// 1 { return delta<Quote>(bid: price) } ... } }`: 1 sets the bid, 2 the
/// ask, 3 the whole quote, and 4 writes the bid three times, the ask once.
struct UpdateQuote {
    static constexpr auto name = "test::update_quote";

    static void eval(hgraph::In<"which", hgraph::TS<i64>> which, hgraph::In<"price", hgraph::TS<i64>> price,
                     hgraph::Out<QuoteTS> out) {
        const i64 at = hglrt::checked_value(price);
        switch (hglrt::checked_value(which)) {
        case 1:
            hglrt::publish(out, t::d_::Quote{.bid = at, .ask = std::nullopt});
            break;
        case 2:
            hglrt::publish(out, t::d_::Quote{.bid = std::nullopt, .ask = at});
            break;
        case 3:
            hglrt::publish(out, t::Quote{at, at + 2});
            break;
        default:
            hglrt::publish(out, t::d_::Quote{.bid = 0, .ask = std::nullopt});
            hglrt::publish(out, t::d_::Quote{.bid = std::nullopt, .ask = at + 2});
            hglrt::child<0>(out).set(at);
            break;
        }
    }
};

/// What a reader saw of a quote in one evaluation.
struct QuoteSeen {
    datetime at;
    bool valid;
    bool all_valid;
    bool modified;
    std::optional<t::d_::Quote> delta;
    std::optional<t::Quote> whole;
    bool bid_modified;

    friend bool operator==(const QuoteSeen&, const QuoteSeen&) = default;
};

std::vector<QuoteSeen> quote_seen;

/// A reader of a quote and of its bid's endpoint.
struct WatchQuote {
    static constexpr auto name = "test::watch_quote";

    static void eval(hgraph::In<"quote", QuoteTS, hgraph::InputValidity::Unchecked> quote, hgraph::DateTime now) {
        std::optional<t::d_::Quote> delta;
        if (hglrt::fn_modified(quote)) {
            delta = hglrt::fn_delta_value(quote);
        }
        std::optional<t::Quote> whole;
        if (hglrt::fn_all_valid(quote)) {
            whole = hglrt::whole_value(quote);
        }
        quote_seen.push_back(QuoteSeen{now, hglrt::fn_valid(quote), hglrt::fn_all_valid(quote),
                                       hglrt::fn_modified(quote), delta, whole,
                                       hglrt::fn_modified(hglrt::child<0>(quote))});
    }
};

TEST_CASE("a struct's fields publish on their own, and the struct's delta names them (TIME-9, TYPE-16)") {
    quote_seen.clear();
    (void)hglrt::testing::run([](hgraph::Wiring& wiring, datetime) {
        hgraph::wire<WatchQuote>(wiring, hgraph::wire<UpdateQuote>(wiring, replayed(wiring, Dense<i64>{1, 2, 1, 3, 4}),
                                                                    replayed(wiring, Dense<i64>{10, 12, 11, 20, 30})));
    });
    const t::d_::Quote bid_only{10, std::nullopt};
    // TIME-9: "`valid` of a collection is true where any element, field or
    // entry is valid"; "`all_valid` is true where every one is".
    CHECK(quote_seen == std::vector<QuoteSeen>{
                            {start, true, false, true, bid_only, std::nullopt, true},
                            {start + 1us, true, true, true, t::d_::Quote{std::nullopt, 12}, t::Quote{10, 12}, false},
                            {start + 2us, true, true, true, t::d_::Quote{11, std::nullopt}, t::Quote{11, 12}, true},
                            {start + 3us, true, true, true, t::d_::Quote{20, 22}, t::Quote{20, 22}, true},
                            // TIME-2: the last write to a field in an
                            // evaluation is the one published.
                            {start + 4us, true, true, true, t::d_::Quote{30, 32}, t::Quote{30, 32}, true},
                        });
}

/// A reader of a quote read whole.
struct ReadQuote {
    static constexpr auto name = "test::read_quote";

    static void eval(hgraph::In<"quote", QuoteTS> quote) { (void)hglrt::whole_value(quote); }
};

TEST_CASE("a whole read of a struct that is not all valid fails with value.unset_read (TIME-1)") {
    std::optional<std::string> code;
    try {
        (void)hglrt::testing::run([](hgraph::Wiring& wiring, datetime) {
            hgraph::wire<ReadQuote>(wiring, hgraph::wire<UpdateQuote>(wiring, replayed(wiring, Dense<i64>{1}),
                                                                    replayed(wiring, Dense<i64>{10})));
        });
    } catch (const std::exception& failure) {
        const std::optional<hglrt::error> carried = hglrt::carried_error(failure);
        code = carried.has_value() ? carried->code : std::nullopt;
    }
    CHECK(code == "value.unset_read");
}

std::vector<std::pair<datetime, i64>> bid_ticks;

/// A reader of an `i64` connection, which keeps each value it is given.
struct KeepTicks {
    static constexpr auto name = "test::keep_ticks";

    static void eval(hgraph::In<"value", hgraph::TS<i64>> value, hgraph::DateTime now) {
        bid_ticks.emplace_back(now, hglrt::checked_value(value));
    }
};

TEST_CASE("a field's connection ticks where the field publishes, and not where a sibling does (EXPR-7)") {
    bid_ticks.clear();
    (void)hglrt::testing::run([](hgraph::Wiring& wiring, datetime) {
        const auto quote = hgraph::wire<UpdateQuote>(wiring, replayed(wiring, Dense<i64>{1, 2, 1}),
                                                     replayed(wiring, Dense<i64>{10, 13, 11}));
        hgraph::wire<KeepTicks>(wiring, hglrt::field_port<0>(quote));
    });
    CHECK(bid_ticks == std::vector<std::pair<datetime, i64>>{{start, 10}, {start + 2us, 11}});
}

/// `fn whole_quote(bid: i64, ask: i64) -> atomic<Quote>`.
struct WholeQuote {
    static constexpr auto name = "test::whole_quote";

    static void eval(hgraph::In<"bid", hgraph::TS<i64>> bid, hgraph::In<"ask", hgraph::TS<i64>> ask,
                     hgraph::Out<hgraph::TS<t::Quote>> out) {
        out.set(t::Quote{hglrt::checked_value(bid), hglrt::checked_value(ask)});
    }
};

TEST_CASE("a field of an atomic struct follows each whole publication (TYPE-9, EXPR-7)") {
    bid_ticks.clear();
    (void)hglrt::testing::run([](hgraph::Wiring& wiring, datetime) {
        const auto quote = hgraph::wire<WholeQuote>(wiring, replayed(wiring, Dense<i64>{10, std::nullopt, 11}),
                                                    replayed(wiring, Dense<i64>{12, 13, std::nullopt}));
        hgraph::wire<KeepTicks>(wiring, hglrt::atomic_field<0, hgraph::TS<i64>>(wiring, quote));
    });
    CHECK(bid_ticks == std::vector<std::pair<datetime, i64>>{{start, 10}, {start + 1us, 10}, {start + 2us, 11}});
}

/// `fn make_book(size: i64) -> Book`: the whole book, then its size alone.
struct MakeBook {
    static constexpr auto name = "test::make_book";

    static void eval(hgraph::In<"size", hgraph::TS<i64>> size, hgraph::Out<BookTS> out) {
        const i64 at = hglrt::checked_value(size);
        if (at == 1) {
            hglrt::publish(out, t::Book{t::Quote{1, 2}, at});
        } else {
            hglrt::publish(out, t::d_::Book{.top = t::d_::Quote{std::nullopt, at}, .size = at});
        }
    }
};

std::vector<std::pair<t::d_::Book, t::Book>> books_seen;

struct WatchBook {
    static constexpr auto name = "test::watch_book";

    static void eval(hgraph::In<"book", BookTS> book) {
        books_seen.emplace_back(hglrt::fn_delta_value(book), hglrt::whole_value(book));
    }
};

TEST_CASE("a struct that holds a struct publishes and reads it field by field (TYPE-9, TYPE-16)") {
    books_seen.clear();
    (void)hglrt::testing::run([](hgraph::Wiring& wiring, datetime) {
        hgraph::wire<WatchBook>(wiring, hgraph::wire<MakeBook>(wiring, replayed(wiring, Dense<i64>{1, 5})));
    });
    CHECK(books_seen ==
          std::vector<std::pair<t::d_::Book, t::Book>>{
              {t::d_::Book{t::d_::Quote{1, 2}, 1}, t::Book{t::Quote{1, 2}, 1}},
              {t::d_::Book{t::d_::Quote{std::nullopt, 5}, 5}, t::Book{t::Quote{1, 5}, 5}},
          });
    // TST-9: a whole cell is published as the delta of every field: a
    // struct's field as its own delta, an atomic field as its payload, and
    // a tuple's struct and tuple positions as their deltas.
    CHECK(hglrt::testing::detail::Publisher<t::Book>{}.replace(t::Book{t::Quote{1, 2}, 3}) ==
          t::d_::Book{t::d_::Quote{1, 2}, 3});
    CHECK(hglrt::testing::detail::Publisher<t::Envelope>{}.replace(t::Envelope{t::Quote{1, 2}, 3}) ==
          t::d_::Envelope{t::Quote{1, 2}, 3});
    using Nested = std::tuple<t::Quote, std::tuple<i64, i64>>;
    CHECK(hglrt::testing::detail::Publisher<Nested>{}.replace(Nested{t::Quote{1, 2}, {3, 4}}) ==
          hglrt::delta_t<Nested>{t::d_::Quote{1, 2}, std::tuple<std::optional<i64>, std::optional<i64>>{3, 4}});
}

/// `fn set_one_part(first: i64, value: i64) -> tuple<i64, i64>`, which
/// publishes a delta of one position.
struct SetOnePart {
    static constexpr auto name = "test::set_one_part";

    static void eval(hgraph::In<"first", hgraph::TS<i64>> first, hgraph::In<"value", hgraph::TS<i64>> value,
                     hgraph::Out<PairTS> out) {
        const i64 at = hglrt::checked_value(value);
        if (hglrt::checked_value(first) == 1) {
            hglrt::publish(out, std::tuple<std::optional<i64>, std::optional<i64>>{at, std::nullopt});
        } else {
            hglrt::publish(out, std::tuple<std::optional<i64>, std::optional<i64>>{std::nullopt, at});
        }
    }
};

std::vector<std::tuple<bool, bool, std::optional<std::tuple<i64, i64>>>> pairs_seen;

struct WatchPair {
    static constexpr auto name = "test::watch_pair";

    static void eval(hgraph::In<"pair", PairTS> pair) {
        pairs_seen.emplace_back(hglrt::fn_valid(pair), hglrt::fn_all_valid(pair),
                                hglrt::endpoint_read<PairTS>::held(pair));
    }
};

TEST_CASE("a tuple is valid once a position is, and all valid once both are (TIME-9)") {
    pairs_seen.clear();
    (void)hglrt::testing::run([](hgraph::Wiring& wiring, datetime) {
        hgraph::wire<WatchPair>(wiring, hgraph::wire<SetOnePart>(wiring, replayed(wiring, Dense<i64>{1, 2, 1}),
                                                                  replayed(wiring, Dense<i64>{3, 8, 4})));
    });
    CHECK(pairs_seen == std::vector<std::tuple<bool, bool, std::optional<std::tuple<i64, i64>>>>{
                            {true, false, std::nullopt},
                            {true, true, std::tuple<i64, i64>{3, 8}},
                            {true, true, std::tuple<i64, i64>{4, 8}},
                        });
}

std::vector<std::optional<i64>> nullables_seen;

/// `fn maybe(command: i64) -> i64? { when { if command == 0 { return null }
/// return command } }`.
struct Maybe {
    static constexpr auto name = "test::maybe";

    static void eval(hgraph::In<"command", hgraph::TS<i64>> command,
                     hgraph::Out<hgraph::TS<hgraph::Optional<i64>>> out) {
        const i64 at = hglrt::checked_value(command);
        hglrt::publish_nullable(out, at == 0 ? std::optional<i64>{} : std::optional<i64>{at});
    }
};

struct WatchMaybe {
    static constexpr auto name = "test::watch_maybe";

    static void eval(hgraph::In<"maybe", hgraph::TS<hgraph::Optional<i64>>> maybe) {
        nullables_seen.push_back(hglrt::nullable_value(maybe));
        if (nullables_seen.back().has_value()) {
            CHECK(hglrt::nullable_payload(maybe) == *nullables_seen.back());
        }
    }
};

TEST_CASE("a nullable's published null is a value, read as an optional that holds nothing (TYPE-15)") {
    nullables_seen.clear();
    (void)hglrt::testing::run([](hgraph::Wiring& wiring, datetime) {
        hgraph::wire<WatchMaybe>(wiring, hgraph::wire<Maybe>(wiring, replayed(wiring, Dense<i64>{0, 7, 0})));
    });
    CHECK(nullables_seen == std::vector<std::optional<i64>>{std::nullopt, 7, std::nullopt});
    CHECK(hglrt::type_name<std::optional<i64>>() == "i64?");
    CHECK(hglrt::type_name<t::d_::Quote>() == "delta<test.structs::Quote>");
}

} // namespace
