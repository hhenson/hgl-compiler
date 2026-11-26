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
#include <string_view>
#include <tuple>
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

// ---- A dictionary of tuples -------------------------------------------------

using TupleEntry = std::tuple<i64, i64>;
using TupleSchema =
    hgraph::UnNamedTSB<hgraph::Field<"0", hgraph::TS<i64>>, hgraph::Field<"1", hgraph::TS<i64>>>;
using TupleBookSchema = hgraph::TSD<str, TupleSchema>;
using TupleBookInput = hgraph::In<"book", TupleBookSchema>;

/// The first step gives only position 0 of the entry. The second completes it.
struct WriteTupleEntry {
    static constexpr auto name = "test::write_tuple_entry";

    static void eval(hgraph::In<"step", hgraph::TS<i64>> step, hgraph::Out<TupleBookSchema> out) {
        using Delta = hglrt::dict_delta<str, TupleEntry>;
        const hglrt::delta_t<TupleEntry> part = step.value() == 0
                                                    ? hglrt::delta_t<TupleEntry>{3, std::nullopt}
                                                    : hglrt::delta_t<TupleEntry>{std::nullopt, 8};
        hglrt::keyed_publish(out, Delta{{{"A", part}}, {}});
    }
};

void read_whole_book_at_hgl_source(const TupleBookInput& book);
void read_tuple_entry_at_hgl_source(const TupleBookInput& book);
hglrt::result<TupleEntry> read_tuple_result_at_hgl_source(const TupleBookInput& book);

std::size_t tuple_book_reads = 0;

struct ReadTupleEntry {
    static constexpr auto name = "test::read_tuple_entry";

    static void eval(hgraph::In<"step", hgraph::TS<i64>> step, TupleBookInput book) {
        ++tuple_book_reads;
        CHECK(hglrt::keyed_entry_valid(book, "A"));
        if (step.value() == 0) {
            const auto check_location = [&](auto read, unsigned line) {
                try {
                    read(book);
                    FAIL("expected an unset read");
                } catch (const hglrt::Failure& failure) {
                    CHECK(failure.error().code == "value.unset_read");
                    CHECK(std::string_view{failure.where().file_name()} == "demo/dictionary_reads.hgl");
                    CHECK(failure.where().line() == line);
                }
            };
            check_location(read_whole_book_at_hgl_source, 12);
            check_location(read_tuple_entry_at_hgl_source, 18);
            const auto result = read_tuple_result_at_hgl_source(book);
            REQUIRE_FALSE(result.has_value());
            CHECK(result.error().code == "value.unset_read");
        } else {
            const auto result = read_tuple_result_at_hgl_source(book);
            REQUIRE(result.has_value());
            CHECK(*result == TupleEntry{3, 8});
        }
    }
};

TEST_CASE("partial dictionary tuples keep the place of a failing read and give err in a result (ERR-14)") {
    tuple_book_reads = 0;
    (void)hglrt::testing::run([](hgraph::Wiring& wiring, datetime) {
        const auto step = replayed(wiring, Dense<i64>{0, 1});
        hgraph::wire<ReadTupleEntry>(wiring, step, hgraph::wire<WriteTupleEntry>(wiring, step));
    });
    CHECK(tuple_book_reads == 2);
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

// ---- Staged KeyedEvaluation observers (TIME-7, TIME-9, END-3) --------------

struct StagedBookStep {
    bool before_all_valid{};
    bool mid_all_valid{};
    bool after_all_valid{};
    bool after_modified{};
    std::optional<BookDelta> mid_delta{};
    std::optional<BookDelta> after_delta{};

    friend bool operator==(const StagedBookStep&, const StagedBookStep&) = default;
};

std::vector<StagedBookStep> staged_book_steps;

struct StagedBookWriter {
    static constexpr auto name = "test::staged_book_writer";

    static void eval(hgraph::In<"step", hgraph::TS<i64>> step, hgraph::Out<BookSchema> out) {
        if (step.value() == 0) {
            // Seed a large dictionary plus one allocated but invalid child slot.
            Book initial;
            for (i64 i = 0; i < 200; ++i) {
                initial.insert_or_assign("k" + std::to_string(i), i);
            }
            hglrt::keyed_publish(out, initial);
            static_cast<void>(out[str{"bad"}]);
            return;
        }
        hglrt::KeyedEvaluation<decltype(out)> staged{out};
        staged.run([&](auto& out_) {
            StagedBookStep seen;
            seen.before_all_valid = hglrt::fn_all_valid(out_);
            if (step.value() == 1) {
                // Touching another key leaves the pre-existing invalid slot unresolved.
                hglrt::keyed_publish(out_, BookDelta{{{"tmp", 1}}, {}});
                seen.mid_all_valid = hglrt::fn_all_valid(out_);
                seen.mid_delta = hglrt::fn_delta_value(out_);
                hglrt::keyed_publish(out_, BookDelta{{}, {"tmp"}});
                seen.after_all_valid = hglrt::fn_all_valid(out_);
                seen.after_modified = hglrt::fn_modified(out_);
                if (auto d = hglrt::fn_delta_value_checked(out_); d.has_value()) {
                    seen.after_delta = *d;
                }
            } else if (step.value() == 2) {
                // Removing the invalid slot flips all_valid to true while committing only that key.
                hglrt::keyed_publish(out_, BookDelta{{}, {"bad"}});
                seen.mid_all_valid = hglrt::fn_all_valid(out_);
                seen.mid_delta = hglrt::fn_delta_value(out_);
                seen.after_all_valid = hglrt::fn_all_valid(out_);
                seen.after_modified = hglrt::fn_modified(out_);
                seen.after_delta = hglrt::fn_delta_value(out_);
            } else if (step.value() == 3) {
                // Sparse churn on an all-valid dictionary stays silent downstream.
                hglrt::keyed_publish(out_, BookDelta{{{"tmp", 9}}, {}});
                seen.mid_all_valid = hglrt::fn_all_valid(out_);
                seen.mid_delta = hglrt::fn_delta_value(out_);
                hglrt::keyed_publish(out_, BookDelta{{}, {"tmp"}});
                seen.after_all_valid = hglrt::fn_all_valid(out_);
                seen.after_modified = hglrt::fn_modified(out_);
                if (auto d = hglrt::fn_delta_value_checked(out_); d.has_value()) {
                    seen.after_delta = *d;
                }
            }
            staged_book_steps.push_back(std::move(seen));
        });
    }
};

std::vector<datetime> staged_book_wakes;

struct StagedBookWatcher {
    static constexpr auto name = "test::staged_book_watcher";

    static void eval(hgraph::In<"book", BookSchema, hgraph::InputValidity::Unchecked>, hgraph::DateTime now) {
        staged_book_wakes.push_back(now);
    }
};

TEST_CASE("KeyedEvaluation staged all_valid and delta_value track pre-existing invalid entries and stay silent on cancellation") {
    staged_book_steps.clear();
    staged_book_wakes.clear();
    (void)hglrt::testing::run([](hgraph::Wiring& wiring, datetime) {
        hgraph::wire<StagedBookWatcher>(
            wiring, hgraph::wire<StagedBookWriter>(wiring, replayed(wiring, Dense<i64>{0, 1, 2, 3})));
    });
    REQUIRE(staged_book_steps.size() == 3);
    CHECK(staged_book_steps[0] == StagedBookStep{
        false, false, false, false, BookDelta{{{"tmp", 1}}, {}}, std::nullopt});
    CHECK(staged_book_steps[1] == StagedBookStep{
        false, true, true, true, BookDelta{{}, {"bad"}}, BookDelta{{}, {"bad"}}});
    CHECK(staged_book_steps[2] == StagedBookStep{
        true, true, true, false, BookDelta{{{"tmp", 9}}, {}}, std::nullopt});
    // Steps 0 (seed) and 2 (remove "bad") wake downstream; cancelling steps 1 and 3 stay silent.
    CHECK(staged_book_wakes == std::vector<datetime>{start, start + 2us});
}

// The generated code sets the HGL file and line at each read.
void read_whole_book_at_hgl_source(const TupleBookInput& book) {
#line 12 "demo/dictionary_reads.hgl"
    (void)hglrt::keyed_value(book);
}

void read_tuple_entry_at_hgl_source(const TupleBookInput& book) {
#line 18 "demo/dictionary_reads.hgl"
    (void)hglrt::keyed_entry_value(book, "A");
}

hglrt::result<TupleEntry> read_tuple_result_at_hgl_source(const TupleBookInput& book) {
#line 23 "demo/dictionary_reads.hgl"
    return hglrt::keyed_entry_result(book, "A");
}

} // namespace
