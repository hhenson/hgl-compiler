// Unit tests of what a `ref` parameter designates (references.h), on nodes
// written by hand as the compiler writes them from their HGL, with hgraph's
// front end for generated code alone, each run in an hgraph graph in
// simulation. As core's END-10 has it, they check what an ordinary reader
// observes and what a node sees of its `ref` parameter, and nothing of how
// a reference is carried between them.
#include "core_nodes.h"

#include <hglrt/references.h>

#include <doctest/doctest.h>
#include <hgraph/types/generated.h>

#include <chrono>
#include <optional>
#include <string>
#include <vector>

namespace {

using hglrt::datetime;
using hglrt::i64;
using hglrt::testing::Dense;
using namespace std::chrono_literals;

using hglrt_test::start;

using hglrt_test::replayed;

/// What a node saw of its `ref` parameter in one evaluation.
struct Seen {
    datetime at;
    bool valid;
    bool designates;

    friend bool operator==(const Seen&, const Seen&) = default;
};

std::vector<Seen> present_seen;

/// `fn present(probe: i64, ref target: i64) { when modified(probe) { ... } }`:
/// what its `ref` parameter is, in each cycle the probe ticks.
struct Present {
    static constexpr auto name = "test::present";

    static void eval(hgraph::In<"probe", hgraph::TS<i64>> probe,
                     hgraph::In<"target", hgraph::REF<hgraph::TS<i64>>, hgraph::InputActivity::Passive,
                                hgraph::InputValidity::Unchecked>
                         target,
                     hgraph::DateTime now) {
        static_cast<void>(probe);
        present_seen.push_back(Seen{now, hglrt::fn_valid(target), !hglrt::designation(target).is_empty()});
    }
};

TEST_CASE("a `ref` parameter designates the connection it is given, whatever that holds (TIME-10)") {
    present_seen.clear();
    (void)hglrt::testing::run([](hgraph::Wiring& wiring, datetime) {
        hgraph::wire<Present>(wiring, replayed(wiring, Dense<i64>{1, 1, 1}),
                              replayed(wiring, Dense<i64>{std::nullopt, std::nullopt, 7}));
    });
    // TIME-10: "a connection given to a `ref` parameter becomes the
    // designation of that connection's endpoint", and "`valid` of a `ref`
    // parameter is whether a reference is present, whatever the endpoint
    // it designates holds".
    CHECK(present_seen == std::vector<Seen>{
                              {start, true, true},
                              {start + 1us, true, true},
                              {start + 2us, true, true},
                          });
}

/// `fn sample(take: i64, ref target: i64) -> ref i64 { when modified(take) {
/// return target } }`.
struct Sample {
    static constexpr auto name = "test::sample";

    static void eval(hgraph::In<"take", hgraph::TS<i64>> take,
                     hgraph::In<"target", hgraph::REF<hgraph::TS<i64>>, hgraph::InputActivity::Passive,
                                hgraph::InputValidity::Unchecked>
                         target,
                     hgraph::Out<hgraph::REF<hgraph::TS<i64>>> out) {
        static_cast<void>(take);
        out.set(hglrt::designation(target));
    }
};

/// `fn none(take: i64) -> ref i64`, whose result designates nothing in each
/// cycle `take` ticks.
struct None {
    static constexpr auto name = "test::none";

    static void eval(hgraph::In<"take", hgraph::TS<i64>> take,
                     hgraph::Out<hgraph::REF<hgraph::TS<i64>>> out) {
        static_cast<void>(take);
        out.set(hgraph::TimeSeriesReference::empty());
    }
};

TEST_CASE("an ordinary reader of a `ref` result observes the endpoint it designates (FN-16, TIME-10)") {
    const hglrt::testing::Recording<i64> recorded =
        hglrt::testing::evaluate<i64>([](hgraph::Wiring& wiring, datetime) {
            auto designated = replayed(wiring, Dense<i64>{std::nullopt, 5, std::nullopt, 6});
            auto sampled = hgraph::wire<Sample>(wiring, replayed(wiring, Dense<i64>{1}), designated);
            hgraph::wire<hglrt_test::Record>(wiring, sampled.as<hgraph::TS<i64>>(),
                                             hglrt::str{hglrt::testing::run_output});
        });
    // TIME-10: the reader "observes the endpoint designated as if wired to
    // it: it is valid where that endpoint is, and ticks where that endpoint
    // publishes".
    CHECK(hglrt::testing::same(recorded, Dense<i64>{std::nullopt, 5, std::nullopt, 6}));
}

TEST_CASE("a `ref` parameter given a result that designates nothing designates nothing (FN-16)") {
    present_seen.clear();
    (void)hglrt::testing::run([](hgraph::Wiring& wiring, datetime) {
        auto nothing = hgraph::wire<None>(wiring, replayed(wiring, Dense<i64>{1}));
        hgraph::wire<Present>(wiring, replayed(wiring, Dense<i64>{1, 1}), nothing.as<hgraph::TS<i64>>());
    });
    CHECK(present_seen.size() == 2);
    for (const Seen& seen : present_seen) {
        CHECK_FALSE(seen.designates);
    }
}

/// `fn route(idx: i64, items: list<i64, 2>) -> ref i64 { when modified(idx)
/// { return items[idx] } }`, each position followed through a `REF` input
/// of its own.
struct Route {
    static constexpr auto name = "test::route";

    static void eval(hgraph::In<"idx", hgraph::TS<i64>> idx,
                     hgraph::In<"first", hgraph::REF<hgraph::TS<i64>>, hgraph::InputActivity::Passive,
                                hgraph::InputValidity::Unchecked>
                         first,
                     hgraph::In<"second", hgraph::REF<hgraph::TS<i64>>, hgraph::InputActivity::Passive,
                                hgraph::InputValidity::Unchecked>
                         second,
                     hgraph::Out<hgraph::REF<hgraph::TS<i64>>> out) {
        const i64 position = hglrt::designated_position(idx.value(), 2);
        out.set(hglrt::designation_of(position, first, second));
    }
};

TEST_CASE("a position designated names its own input, and one outside the list fails (FN-16, EXPR-3)") {
    const hglrt::testing::Recording<i64> recorded =
        hglrt::testing::evaluate<i64>([](hgraph::Wiring& wiring, datetime) {
            auto routed = hgraph::wire<Route>(wiring, replayed(wiring, Dense<i64>{1, std::nullopt, 0}),
                                              replayed(wiring, Dense<i64>{10, 11, std::nullopt}),
                                              replayed(wiring, Dense<i64>{20, 21, std::nullopt}));
            hgraph::wire<hglrt_test::Record>(wiring, routed.as<hgraph::TS<i64>>(),
                                             hglrt::str{hglrt::testing::run_output});
        });
    // The reader observes the second position, then the first, which it
    // ticks with the value it holds (TIME-10).
    CHECK(hglrt::testing::same(recorded, Dense<i64>{20, 21, 11}));
    CHECK(hglrt::designated_position(1, 2) == 1);
    for (const i64 outside : {i64{2}, i64{-1}}) {
        std::optional<std::string> code;
        try {
            (void)hglrt::designated_position(outside, 2);
        } catch (const hglrt::Failure& failure) {
            code = failure.error().code;
        }
        CHECK(code == "value.index_out_of_range");
    }
}

using BookSchema = hgraph::TSD<hglrt::str, hgraph::TS<i64>>;
using BookDelta = hglrt::dict_delta<hglrt::str, i64>;

/// `fn write_book(command: i64) -> dict<str, i64>`: 1 `A: 7`, 2 `B: 20`, 3
/// `A: 8`, 4 the removal of B, 5 `B: 22`, 6 `C: 30`.
struct WriteBook {
    static constexpr auto name = "test::write_book";

    static void eval(hgraph::In<"command", hgraph::TS<i64>> command, hgraph::Out<BookSchema> out) {
        switch (command.value()) {
        case 1:
            hglrt::keyed_publish(out, BookDelta{{{"A", 7}}, {}});
            break;
        case 2:
            hglrt::keyed_publish(out, BookDelta{{{"B", 20}}, {}});
            break;
        case 3:
            hglrt::keyed_publish(out, BookDelta{{{"A", 8}}, {}});
            break;
        case 4:
            hglrt::keyed_publish(out, BookDelta{{}, {"B"}});
            break;
        case 5:
            hglrt::keyed_publish(out, BookDelta{{{"B", 22}}, {}});
            break;
        case 6:
            hglrt::keyed_publish(out, BookDelta{{{"C", 30}}, {}});
            break;
        default:
            break;
        }
    }
};

/// `fn key_of(command: i64) -> str`: 1 `"A"`, 2 `"B"`, 3 `"C"`.
struct KeyOf {
    static constexpr auto name = "test::key_of";

    static void eval(hgraph::In<"command", hgraph::TS<i64>> command,
                     hgraph::Out<hgraph::TS<hglrt::str>> out) {
        out.set(hglrt::str(1, static_cast<char>('A' + command.value() - 1)));
    }
};

/// `book[key]` of a graph, as the compiler writes it.
struct Selected {
    static constexpr auto name = "test::selected";

    static void start(hgraph::State<i64> state) { state.set(i64{0}); }

    static void eval(hgraph::In<"dictionary", BookSchema, hgraph::InputActivity::Structural,
                                hgraph::InputValidity::Unchecked>
                         dictionary,
                     hgraph::In<"key", hgraph::TS<hglrt::str>> key, hgraph::State<i64> state,
                     hgraph::Out<hgraph::REF<hgraph::TS<i64>>> out) {
        hglrt::select_entry(dictionary, key, state.modify(), out);
    }
};

TEST_CASE("an entry selected by key is what its reader observes (EXPR-7, TIME-10, TIME-1)") {
    const hglrt::testing::Recording<i64> recorded =
        hglrt::testing::evaluate<i64>([](hgraph::Wiring& wiring, datetime) {
            auto book = hgraph::wire<WriteBook>(
                wiring, replayed(wiring, Dense<i64>{1, 2, std::nullopt, 3, 4, 5, std::nullopt, 6}));
            auto key = hgraph::wire<KeyOf>(
                wiring, replayed(wiring, Dense<i64>{1, std::nullopt, 2, std::nullopt, std::nullopt,
                                                    std::nullopt, 3, std::nullopt}));
            auto selected = hgraph::wire<Selected>(wiring, book, key);
            hgraph::wire<hglrt_test::Record>(wiring, selected.as<hgraph::TS<i64>>(),
                                             hglrt::str{hglrt::testing::run_output});
        });
    // A: 7 read; B: 20 silent; the key names B, which holds 20; A: 8
    // silent; B removed, and the reader not valid; B: 22 added, read; the
    // key names C, which the book lacks; C: 30 added, read.
    CHECK(hglrt::testing::same(
        recorded, Dense<i64>{7, std::nullopt, 20, std::nullopt, std::nullopt, 22, std::nullopt, 30}));
}

} // namespace
