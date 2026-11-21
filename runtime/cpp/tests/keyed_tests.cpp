// Unit tests of the temporal form of a set and of a dictionary (keyed.h), on
// nodes written by hand as the compiler writes them, with hgraph's front end
// for generated code alone, each run in an hgraph graph in simulation. A
// writer publishes what its command says, and a reader, which requires
// nothing valid, keeps what it saw in each evaluation in a list of the
// test's.
#include "core_nodes.h"

#include <hglrt/keyed.h>

#include <doctest/doctest.h>
#include <hgraph/types/generated.h>

#include <chrono>
#include <optional>
#include <string>
#include <utility>
#include <vector>

namespace {

using hglrt::datetime;
using hglrt::i64;
using hglrt::str;
using hglrt::testing::Dense;
using namespace std::chrono_literals;

using Members = hglrt::set<i64>;
using MembersDelta = hglrt::set_delta<i64>;
using Book = hglrt::dict<str, i64>;
using BookDelta = hglrt::dict_delta<str, i64>;
using MembersSchema = hgraph::TSS<i64>;
using BookSchema = hgraph::TSD<str, hgraph::TS<i64>>;

using hglrt_test::start;

using hglrt_test::replayed;

/// What a reader saw in one evaluation: its time, `valid`, `modified`, the
/// value read whole where it is valid, and the delta where it has one.
template <class Value, class Delta>
struct Seen {
    datetime at;
    bool valid;
    bool modified;
    std::optional<Value> value;
    std::optional<Delta> delta;

    friend bool operator==(const Seen&, const Seen&) = default;
};

template <class E>
auto seen_of(datetime at, const E& endpoint) {
    using Form = hglrt::keyed_form<hglrt::schema_of<E>>;
    using Delta = hglrt::endpoint_delta_t<E>;
    std::optional<typename Form::type> value;
    if (hglrt::fn_valid(endpoint)) {
        value = hglrt::keyed_value(endpoint);
    }
    std::optional<Delta> delta;
    if (const hglrt::result<Delta> checked = hglrt::fn_delta_value_checked(endpoint); checked.has_value()) {
        delta = *checked;
    }
    return Seen<typename Form::type, Delta>{at, hglrt::fn_valid(endpoint), hglrt::fn_modified(endpoint),
                                            std::move(value), std::move(delta)};
}

// ---- A set -----------------------------------------------------------------

std::vector<Seen<Members, MembersDelta>> members_seen;

/// `fn write_members(command: i64) -> set<i64>`: 0 a delta that names no
/// change, 1 `[1]` whole, 2 `[1, 2]` whole, 3 the removal of 9, 4
/// `invalidate(out)`, 5 two deltas, the addition of 2 and 3 and then the
/// removal of 1 and 2, 6 `[]` whole.
struct WriteMembers {
    static constexpr auto name = "test::write_members";

    static void eval(hgraph::In<"command", hgraph::TS<i64>> command, hgraph::Out<MembersSchema> out) {
        switch (command.value()) {
        case 0:
            hglrt::keyed_publish(out, MembersDelta{});
            break;
        case 1:
            hglrt::keyed_publish(out, Members{1});
            break;
        case 2:
            hglrt::keyed_publish(out, Members{1, 2});
            break;
        case 3:
            hglrt::keyed_publish(out, MembersDelta{{}, {9}});
            break;
        case 4:
            hglrt::fn_invalidate(out);
            break;
        case 5:
            hglrt::keyed_publish(out, MembersDelta{{2, 3}, {}});
            hglrt::keyed_publish(out, MembersDelta{{}, {1, 2}});
            break;
        case 6:
            hglrt::keyed_publish(out, Members{});
            break;
        default:
            break;
        }
    }
};

struct ReadMembers {
    static constexpr auto name = "test::read_members";

    static void eval(hgraph::In<"members", MembersSchema, hgraph::InputValidity::Unchecked> members,
                     hgraph::DateTime now) {
        members_seen.push_back(seen_of(now, members));
    }
};

std::vector<Seen<Members, MembersDelta>> watch_members(const Dense<i64>& commands) {
    members_seen.clear();
    (void)hglrt::testing::run([&](hgraph::Wiring& wiring, datetime) {
        hgraph::wire<ReadMembers>(wiring, hgraph::wire<WriteMembers>(wiring, replayed(wiring, commands)));
    });
    return members_seen;
}

TEST_CASE("a set's delta that names no change ticks only where it makes the set valid (TIME-7, END-4)") {
    // TIME-7: "Where a `list<V>`, a dictionary or a set is not valid ... it
    // makes the value valid and empty, and ticks. Where it is valid the delta
    // changes nothing, and is no tick". TIME-3: "An active input's transition
    // to invalid wakes the node", and END-4 empties it.
    CHECK(watch_members(Dense<i64>{0, 0, 4, 0}) ==
          std::vector<Seen<Members, MembersDelta>>{
              {start, true, true, Members{}, MembersDelta{}},
              {start + 2us, false, false, std::nullopt, std::nullopt},
              {start + 3us, true, true, Members{}, MembersDelta{}},
          });
}

TEST_CASE("a set given the members it holds, or a removal of what it does not hold, is no tick (TIME-7)") {
    CHECK(watch_members(Dense<i64>{1, 1, 3, 2, 2}) ==
          std::vector<Seen<Members, MembersDelta>>{
              {start, true, true, Members{1}, MembersDelta{{1}, {}}},
              {start + 3us, true, true, Members{1, 2}, MembersDelta{{2}, {}}},
          });
}

TEST_CASE("a set given whole replaces its members, and an empty one empties it (TIME-7)") {
    CHECK(watch_members(Dense<i64>{2, 1, 6, 6}) ==
          std::vector<Seen<Members, MembersDelta>>{
              {start, true, true, Members{1, 2}, MembersDelta{{1, 2}, {}}},
              {start + 1us, true, true, Members{1}, MembersDelta{{}, {2}}},
              {start + 2us, true, true, Members{}, MembersDelta{{}, {1}}},
          });
}

TEST_CASE("two deltas of a set in one evaluation are one change (TIME-2, END-5)") {
    CHECK(watch_members(Dense<i64>{1, 5}) ==
          std::vector<Seen<Members, MembersDelta>>{
              {start, true, true, Members{1}, MembersDelta{{1}, {}}},
              {start + 1us, true, true, Members{3}, MembersDelta{{3}, {1}}},
          });
}

// ---- A dictionary ----------------------------------------------------------

std::vector<Seen<Book, BookDelta>> book_seen;
std::vector<bool> book_all_valid;

/// `fn write_book(command: i64) -> dict<str, i64>`: 0 a delta that names no
/// change, 1 `A` updated to 7, 2 to 9, 3 the removal of `B`, 4
/// `invalidate(out)`, 5 `[]` whole, 6 `["A": 7]` whole, 7 `["bid": 10, "ask":
/// 12]` whole, 8 `["bid": 11]` whole, 9 the removal of `A`.
struct WriteBook {
    static constexpr auto name = "test::write_book";

    static void eval(hgraph::In<"command", hgraph::TS<i64>> command, hgraph::Out<BookSchema> out) {
        switch (command.value()) {
        case 0:
            hglrt::keyed_publish(out, BookDelta{});
            break;
        case 1:
            hglrt::keyed_publish(out, BookDelta{{{"A", 7}}, {}});
            break;
        case 2:
            hglrt::keyed_publish(out, BookDelta{{{"A", 9}}, {}});
            break;
        case 3:
            hglrt::keyed_publish(out, BookDelta{{}, {"B"}});
            break;
        case 4:
            hglrt::fn_invalidate(out);
            break;
        case 5:
            hglrt::keyed_publish(out, Book{});
            break;
        case 6:
            hglrt::keyed_publish(out, Book{{"A", 7}});
            break;
        case 7:
            hglrt::keyed_publish(out, Book{{"bid", 10}, {"ask", 12}});
            break;
        case 8:
            hglrt::keyed_publish(out, Book{{"bid", 11}});
            break;
        case 9:
            hglrt::keyed_publish(out, BookDelta{{}, {"A"}});
            break;
        default:
            break;
        }
    }
};

struct ReadBook {
    static constexpr auto name = "test::read_book";

    static void eval(hgraph::In<"book", BookSchema, hgraph::InputValidity::Unchecked> book,
                     hgraph::DateTime now) {
        book_seen.push_back(seen_of(now, book));
        book_all_valid.push_back(hglrt::fn_all_valid(book));
    }
};

std::vector<Seen<Book, BookDelta>> watch_book(const Dense<i64>& commands) {
    book_seen.clear();
    book_all_valid.clear();
    (void)hglrt::testing::run([&](hgraph::Wiring& wiring, datetime) {
        hgraph::wire<ReadBook>(wiring, hgraph::wire<WriteBook>(wiring, replayed(wiring, commands)));
    });
    return book_seen;
}

TEST_CASE("a dictionary's delta that names no change ticks only where it makes it valid (TIME-7, TIME-9)") {
    CHECK(watch_book(Dense<i64>{0, 0, 4, 0}) == std::vector<Seen<Book, BookDelta>>{
                                                    {start, true, true, Book{}, BookDelta{}},
                                                    {start + 2us, false, false, std::nullopt, std::nullopt},
                                                    {start + 3us, true, true, Book{}, BookDelta{}},
                                                });
    // TIME-9: "It is true of a valid empty collection. `all_valid` of a
    // collection that is not valid is false."
    CHECK(book_all_valid == std::vector<bool>{true, false, true});
}

TEST_CASE("a removal of a key the dictionary does not hold is no tick, and a removed key is gone (TYPE-16)") {
    CHECK(watch_book(Dense<i64>{1, 3, 2, 9, 1}) ==
          std::vector<Seen<Book, BookDelta>>{
              {start, true, true, Book{{"A", 7}}, BookDelta{{{"A", 7}}, {}}},
              {start + 2us, true, true, Book{{"A", 9}}, BookDelta{{{"A", 9}}, {}}},
              {start + 3us, true, true, Book{}, BookDelta{{}, {"A"}}},
              {start + 4us, true, true, Book{{"A", 7}}, BookDelta{{{"A", 7}}, {}}},
          });
}

TEST_CASE("a dictionary given whole publishes each entry, and an empty one given to an empty one is none "
          "(TIME-7)") {
    CHECK(watch_book(Dense<i64>{5, 5, 6, 6, 7, 8}) ==
          std::vector<Seen<Book, BookDelta>>{
              {start, true, true, Book{}, BookDelta{}},
              {start + 2us, true, true, Book{{"A", 7}}, BookDelta{{{"A", 7}}, {}}},
              {start + 3us, true, true, Book{{"A", 7}}, BookDelta{{{"A", 7}}, {}}},
              {start + 4us, true, true, Book{{"bid", 10}, {"ask", 12}},
               BookDelta{{{"bid", 10}, {"ask", 12}}, {"A"}}},
              {start + 5us, true, true, Book{{"bid", 11}}, BookDelta{{{"bid", 11}}, {"ask"}}},
          });
}

TEST_CASE("an invalidated dictionary is empty and not valid, and a whole one makes it valid (END-4)") {
    CHECK(watch_book(Dense<i64>{1, 4, 5}) ==
          std::vector<Seen<Book, BookDelta>>{
              {start, true, true, Book{{"A", 7}}, BookDelta{{{"A", 7}}, {}}},
              {start + 1us, false, false, std::nullopt, std::nullopt},
              {start + 2us, true, true, Book{}, BookDelta{}},
          });
}

/// What a reader saw of one entry of a dictionary: `valid`, `modified`,
/// `last_modified`, its delta where it has one, its value read into a
/// `result`, and the code of the failure of its plain read.
struct EntrySeen {
    bool valid;
    bool modified;
    std::optional<datetime> last_modified;
    std::optional<i64> delta;
    std::optional<i64> value;
    std::optional<std::string> failed;

    friend bool operator==(const EntrySeen&, const EntrySeen&) = default;
};

std::vector<std::pair<EntrySeen, EntrySeen>> entries_seen;

EntrySeen entry_seen(const hgraph::In<"book", BookSchema, hgraph::InputValidity::Unchecked>& book,
                     const str& key) {
    EntrySeen seen{hglrt::keyed_entry_valid(book, key),
                   hglrt::keyed_entry_modified(book, key),
                   hglrt::keyed_entry_last_modified(book, key),
                   std::nullopt,
                   std::nullopt,
                   std::nullopt};
    if (const auto delta = hglrt::keyed_entry_delta_checked(book, key); delta.has_value()) {
        seen.delta = *delta;
        CHECK(hglrt::keyed_entry_delta(book, key) == *delta);
    } else {
        CHECK(delta.error().code == "value.unset_read");
    }
    if (const auto value = hglrt::keyed_entry_result(book, key); value.has_value()) {
        seen.value = *value;
        CHECK(hglrt::keyed_entry_value(book, key) == *value);
    } else {
        try {
            (void)hglrt::keyed_entry_value(book, key);
        } catch (const hglrt::Failure& failure) {
            seen.failed = failure.error().code;
        }
    }
    return seen;
}

/// Reads the entries `A` and `Z` of its book in each evaluation.
struct ReadEntries {
    static constexpr auto name = "test::read_entries";

    static void eval(hgraph::In<"book", BookSchema, hgraph::InputValidity::Unchecked> book) {
        entries_seen.emplace_back(entry_seen(book, "A"), entry_seen(book, "Z"));
    }
};

TEST_CASE("an entry of a dictionary is an endpoint whether or not the dictionary holds it (TIME-1, TIME-9)") {
    entries_seen.clear();
    (void)hglrt::testing::run([](hgraph::Wiring& wiring, datetime) {
        hgraph::wire<ReadEntries>(wiring,
                                  hgraph::wire<WriteBook>(wiring, replayed(wiring, Dense<i64>{1, 5, 6})));
    });
    const EntrySeen absent{false, false, std::nullopt, std::nullopt, std::nullopt, "value.unset_read"};
    // The whole `[]` removes `A`: its entry is then one the dictionary does
    // not hold, as `Z` always is.
    CHECK(entries_seen == std::vector<std::pair<EntrySeen, EntrySeen>>{
                              {{true, true, start, 7, 7, std::nullopt}, absent},
                              {absent, absent},
                              {{true, true, start + 2us, 7, 7, std::nullopt}, absent},
                          });
}

std::vector<std::optional<std::string>> early_codes;

/// Reads its set whole in each evaluation, whether or not it is valid, and
/// keeps the code of the failure where the read fails.
struct ReadEarly {
    static constexpr auto name = "test::read_early";

    static void eval(hgraph::In<"probe", hgraph::TS<i64>, hgraph::InputValidity::Unchecked>,
                     hgraph::In<"members", MembersSchema, hgraph::InputValidity::Unchecked> members) {
        try {
            (void)hglrt::keyed_value(members);
            early_codes.emplace_back(std::nullopt);
        } catch (const hglrt::Failure& failure) {
            early_codes.push_back(failure.error().code);
        }
    }
};

TEST_CASE("a set that is not valid fails a read of it whole (TIME-1, ERR-14)") {
    early_codes.clear();
    (void)hglrt::testing::run([](hgraph::Wiring& wiring, datetime) {
        hgraph::wire<ReadEarly>(
            wiring, replayed(wiring, Dense<i64>{0, 0}),
            hgraph::wire<WriteMembers>(wiring, replayed(wiring, Dense<i64>{std::nullopt, 1})));
    });
    CHECK(early_codes == std::vector<std::optional<std::string>>{"value.unset_read", std::nullopt});
}

// ---- Traces ----------------------------------------------------------------

TEST_CASE("a set's and a dictionary's whole trace cells publish their change from the cells before (TST-9)") {
    using SetCell = hglrt::testing::Cell<Members>;
    using Whole = hglrt::testing::Whole<Members>;
    const auto set_items = hglrt::testing::replay_items(
        Dense<Members>{SetCell{Whole{{1, 2}}}, SetCell{Whole{{1, 2}}}, SetCell{MembersDelta{{3}, {1}}},
                       std::nullopt, SetCell{Whole{{4}}}},
        start);
    REQUIRE(set_items.size() == 4);
    CHECK(std::get<1>(set_items[0]) == MembersDelta{{1, 2}, {}});
    // A set given the members it holds names no change (TIME-7).
    CHECK(std::get<1>(set_items[1]) == MembersDelta{});
    CHECK(std::get<1>(set_items[2]) == MembersDelta{{3}, {1}});
    CHECK(std::get<1>(set_items[3]) == MembersDelta{{4}, {2, 3}});

    using BookCell = hglrt::testing::Cell<Book>;
    using WholeBook = hglrt::testing::Whole<Book>;
    const auto book_items = hglrt::testing::replay_items(
        Dense<Book>{BookCell{WholeBook{{{"bid", 10}, {"ask", 12}}}}, BookCell{WholeBook{{{"bid", 11}}}}},
        start);
    REQUIRE(book_items.size() == 2);
    CHECK(std::get<1>(book_items[0]) == BookDelta{{{"bid", 10}, {"ask", 12}}, {}});
    CHECK(std::get<1>(book_items[1]) == BookDelta{{{"bid", 11}}, {"ask"}});
}

TEST_CASE("a set, a dictionary and their deltas have their HGL names and text") {
    CHECK(hglrt::type_name<Members>() == "set<i64>");
    CHECK(hglrt::type_name<hglrt::list<std::tuple<datetime, BookDelta>>>() ==
          "list<tuple<datetime, delta<dict<str, i64>>>>");
    CHECK(hglrt::testing::text_of(Members{2, 1}) == "[1, 2]");
    CHECK(hglrt::testing::text_of(Book{{"b", 2}, {"a", 1}}) == "[\"a\": 1, \"b\": 2]");
    CHECK(hglrt::testing::text_of(MembersDelta{{3}, {1}}) == "delta<set<i64>>(added: [3], removed: [1])");
    CHECK(hglrt::testing::text_of(BookDelta{{{"a", 1}}, {"b"}}) ==
          "delta<dict<str, i64>>(updated: [\"a\": 1], removed: [\"b\"])");
}

} // namespace
