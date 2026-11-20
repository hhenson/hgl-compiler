// Unit tests of core's operations on an endpoint (endpoints.h), on nodes
// written by hand as the compiler writes them, with hgraph's front end for
// generated code alone, each run in an hgraph graph in simulation. Each node
// keeps what it saw in a list of the test's, which the test then reads.
#include "core_nodes.h"

#include <hglrt/endpoints.h>

#include <doctest/doctest.h>
#include <hgraph/types/generated.h>

#include <chrono>
#include <optional>
#include <string>
#include <type_traits>
#include <utility>
#include <vector>

namespace {

using hglrt::datetime;
using hglrt::i64;
using hglrt::testing::Dense;
using namespace std::chrono_literals;

const datetime start = hglrt::testing::earliest_start();

/// The connection core's `replay` makes of `trace`.
hgraph::Port<hgraph::TS<i64>> replayed(hgraph::Wiring& wiring, const Dense<i64>& trace) {
    return hgraph::wire<hglrt_test::Replay>(wiring, hglrt::testing::replay_items(trace, start));
}

/// What a node saw of an endpoint in one evaluation: the evaluation's time,
/// and `valid`, `modified`, `delta_value` where it has a delta, and
/// `last_modified` of the endpoint.
template <class Delta>
struct Seen {
    datetime at;
    bool valid;
    bool modified;
    std::optional<Delta> delta;
    std::optional<datetime> last_modified;

    friend bool operator==(const Seen&, const Seen&) = default;
};

/// The code of the HGL failure `call` raises, or none.
template <class Call>
std::optional<std::string> code_raised(Call&& call) {
    try {
        call();
    } catch (const hglrt::Failure& failure) {
        return failure.error().code.value_or("(no code)");
    }
    return std::nullopt;
}

/// What `endpoint` shows at `at`. Its delta is read in both forms (ERR-14):
/// where the place takes a `result`, `ok` of the delta or an `err` coded
/// `value.unset_read`, and elsewhere the delta, or a failure of that code.
template <class E>
auto seen_of(datetime at, const E& endpoint) {
    using Delta = hglrt::endpoint_delta_t<E>;
    const hglrt::result<Delta> checked = hglrt::fn_delta_value_checked(endpoint);
    std::optional<Delta> delta;
    if (checked.has_value()) {
        delta = *checked;
        CHECK(hglrt::fn_delta_value(endpoint) == *checked);
    } else {
        CHECK(checked.error().code == "value.unset_read");
        CHECK(code_raised([&] { (void)hglrt::fn_delta_value(endpoint); }) == "value.unset_read");
    }
    return Seen<Delta>{at, hglrt::fn_valid(endpoint), hglrt::fn_modified(endpoint), delta,
                       hglrt::fn_last_modified(endpoint)};
}

std::vector<Seen<i64>> watched_seen;

/// `fn watch(probe: i64, watched: i64) { when valid([]) { ... } }`: reads
/// its input `watched` in each evaluation, whichever input woke it.
struct Watch {
    static constexpr auto name = "test::watch";

    static void eval(hgraph::In<"probe", hgraph::TS<i64>, hgraph::InputValidity::Unchecked>,
                     hgraph::In<"watched", hgraph::TS<i64>, hgraph::InputValidity::Unchecked> watched,
                     hgraph::DateTime now) {
        watched_seen.push_back(seen_of(now, watched));
    }
};

TEST_CASE("a scalar's publication, silence and equal publication (END-1, END-2, END-3, END-6)") {
    watched_seen.clear();
    (void)hglrt::testing::run([](hgraph::Wiring& wiring, datetime) {
        hgraph::wire<Watch>(wiring, replayed(wiring, Dense<i64>{1, 1, 1, 1, 1}),
                            replayed(wiring, Dense<i64>{std::nullopt, 0, std::nullopt, 0, std::nullopt}));
    });
    // END-1: "A scalar publication makes the endpoint valid and retains its
    // payload, including false, zero". END-2: an equal publication is
    // modified; "An idle cycle leaves the endpoint unmodified". END-6:
    // "Silence preserves that time and an equal publication supplies its
    // own time".
    CHECK(watched_seen == std::vector<Seen<i64>>{
                              {start, false, false, std::nullopt, std::nullopt},
                              {start + 1us, true, true, 0, start + 1us},
                              {start + 2us, true, false, std::nullopt, start + 1us},
                              {start + 3us, true, true, 0, start + 3us},
                              {start + 4us, true, false, std::nullopt, start + 3us},
                          });
}

std::vector<Seen<std::optional<i64>>> nullable_seen;
std::vector<std::optional<i64>> nullable_published;

/// `fn publish_nullable(command: i64) -> i64? { when { if command == 0 {
/// return null }; return command } }`, which reads its own result's delta
/// after it publishes (`delta_value(out)`).
struct PublishNullable {
    static constexpr auto name = "test::publish_nullable";

    static void eval(hgraph::In<"command", hgraph::TS<i64>> command,
                     hgraph::Out<hgraph::TS<hgraph::Optional<i64>>> out) {
        if (command.value() == 0) {
            out.set(hgraph::Null);
        } else {
            out.set(command.value());
        }
        nullable_published.push_back(hglrt::fn_delta_value(out));
    }
};

/// `fn read_nullable(watched: i64?) { when { ... } }`.
struct ReadNullable {
    static constexpr auto name = "test::read_nullable";

    static void eval(hgraph::In<"watched", hgraph::TS<hgraph::Optional<i64>>> watched, hgraph::DateTime now) {
        nullable_seen.push_back(seen_of(now, watched));
    }
};

TEST_CASE("a nullable scalar's published null is its delta, and no absent one (END-3)") {
    nullable_seen.clear();
    nullable_published.clear();
    (void)hglrt::testing::run([](hgraph::Wiring& wiring, datetime) {
        hgraph::wire<ReadNullable>(
            wiring, hgraph::wire<PublishNullable>(wiring, replayed(wiring, Dense<i64>{0, 7, 0})));
    });
    const std::optional<i64> null;
    CHECK(nullable_published == std::vector<std::optional<i64>>{null, 7, null});
    // "a published null is `delta<i64?>()` for an `i64?` endpoint, and is no
    // absent delta"; END-6: "A publication with a null payload has a
    // timestamp".
    CHECK(nullable_seen == std::vector<Seen<std::optional<i64>>>{
                               {start, true, true, null, start},
                               {start + 1us, true, true, 7, start + 1us},
                               {start + 2us, true, true, null, start + 2us},
                           });
}

/// What `produce` saw of its own result after both its actions, where it
/// took two.
struct Produced {
    datetime at;
    bool valid;
    bool modified;

    friend bool operator==(const Produced&, const Produced&) = default;
};

std::vector<Produced> produced;
std::vector<Seen<i64>> invalidated_seen;

/// `fn produce(command: i64) -> i64 { inject out; when { ... } }`: 0
/// invalidates its result, 2 publishes 2 and then invalidates it, 3
/// invalidates it and then publishes 3, any other value publishes it.
struct Produce {
    static constexpr auto name = "test::produce";

    static void eval(hgraph::In<"command", hgraph::TS<i64>> command, hgraph::Out<hgraph::TS<i64>> out,
                     hgraph::DateTime now) {
        const i64 value = command.value();
        if (value == 0) {
            hglrt::fn_invalidate(out);
            return;
        }
        if (value == 2) {
            out.set(value);
            hglrt::fn_invalidate(out);
        } else if (value == 3) {
            hglrt::fn_invalidate(out);
            out.set(value);
        } else {
            out.set(value);
            return;
        }
        produced.push_back(Produced{now, hglrt::fn_valid(out), hglrt::fn_modified(out)});
    }
};

/// `fn watch(input: i64) { when valid([]) { ... } }`.
struct WatchAlone {
    static constexpr auto name = "test::watch_alone";

    static void eval(hgraph::In<"input", hgraph::TS<i64>, hgraph::InputValidity::Unchecked> input,
                     hgraph::DateTime now) {
        invalidated_seen.push_back(seen_of(now, input));
    }
};

TEST_CASE("invalidation clears an endpoint and wakes its reader once (END-4, END-6)") {
    produced.clear();
    invalidated_seen.clear();
    (void)hglrt::testing::run([](hgraph::Wiring& wiring, datetime) {
        hgraph::wire<WatchAlone>(
            wiring, hgraph::wire<Produce>(wiring, replayed(wiring, Dense<i64>{7, 0, 0, 7, 2, 3})));
    });
    // END-4: "Within a producer evaluation the last action determines the
    // state: publish then invalidate leaves the endpoint invalid. Invalidate
    // then publish leaves it valid and modified."
    CHECK(produced == std::vector<Produced>{{start + 4us, false, false}, {start + 5us, true, true}});
    // "The transition to invalid notifies active subscribers ... An explicit
    // `modified(input)` guard is false on that transition ... Repeated
    // invalidation while invalid is silent"; END-6: `last_modified` "returns
    // null while the endpoint is invalid".
    CHECK(invalidated_seen == std::vector<Seen<i64>>{
                                  {start, true, true, 7, start},
                                  {start + 1us, false, false, std::nullopt, std::nullopt},
                                  {start + 3us, true, true, 7, start + 3us},
                                  {start + 4us, false, false, std::nullopt, std::nullopt},
                                  {start + 5us, true, true, 3, start + 5us},
                              });
}

/// What `toggle` saw in one evaluation: its time, whether `data` was active
/// as it began, whether it was modified, and whether it was active as it
/// ended.
struct Toggled {
    datetime at;
    bool active_before;
    bool modified;
    bool active_after;

    friend bool operator==(const Toggled&, const Toggled&) = default;
};

std::vector<Toggled> toggled;

/// `fn toggle(probe: i64, data: i64) { when valid([]) { ... } }`: a probe
/// of 1 passivates `data`, one of 2 activates it.
struct Toggle {
    static constexpr auto name = "test::toggle";

    static void eval(hgraph::In<"probe", hgraph::TS<i64>, hgraph::InputValidity::Unchecked> probe,
                     hgraph::In<"data", hgraph::TS<i64>, hgraph::InputValidity::Unchecked> data,
                     hgraph::DateTime now) {
        const bool before = hglrt::fn_is_active(data);
        if (hglrt::fn_modified(probe) && probe.value() == 1) {
            hglrt::fn_passivate(data);
        }
        if (hglrt::fn_modified(probe) && probe.value() == 2) {
            hglrt::fn_activate(data);
        }
        toggled.push_back(Toggled{now, before, hglrt::fn_modified(data), hglrt::fn_is_active(data)});
    }
};

TEST_CASE("a passive input wakes no node and is read as an active one (END-5, TIME-3)") {
    toggled.clear();
    (void)hglrt::testing::run([](hgraph::Wiring& wiring, datetime) {
        hgraph::wire<Toggle>(wiring, replayed(wiring, Dense<i64>{std::nullopt, 1, std::nullopt, 2}),
                             replayed(wiring, Dense<i64>{5, 6, 7, 8, 9}));
    });
    // TIME-3: "Passivating an input removes it as a wake source. If a probe
    // wakes the node in a cycle when passive `data` publishes, `when
    // modified(data)` matches."
    CHECK(toggled == std::vector<Toggled>{
                         {start, true, true, true},
                         {start + 1us, true, true, false},
                         {start + 3us, false, true, true},
                         {start + 4us, true, true, true},
                     });
}

template <class E>
concept takes_all_valid = requires(const E& endpoint) { hglrt::fn_all_valid(endpoint); };
template <class E>
concept takes_delta_value = requires(const E& endpoint) { hglrt::fn_delta_value(endpoint); };
template <class E>
concept takes_activate = requires(E& endpoint) { hglrt::fn_activate(endpoint); };
template <class E>
concept takes_invalidate = requires(const E& endpoint) { hglrt::fn_invalidate(endpoint); };

using Input = hgraph::In<"x", hgraph::TS<i64>>;
using Output = hgraph::Out<hgraph::TS<i64>>;

// TIME-9: "`all_valid` takes only a collection or a window", and a scalar
// has no `endpoint_read::all_valid`.
static_assert(!takes_all_valid<Input>);
static_assert(takes_delta_value<Input> && takes_delta_value<Output>);
// Activity is a direct input's, and invalidation an output's.
static_assert(takes_activate<Input> && !takes_activate<Output>);
static_assert(takes_invalidate<Output> && !takes_invalidate<Input>);
// The result form is the library's `result` of the delta (ERR-14).
static_assert(std::is_same_v<decltype(hglrt::fn_delta_value_checked(std::declval<const Input&>())),
                             hglrt::result<i64>>);

/// The schema of a form whose endpoint may tick with no delta, as a window
/// of a duration does where a value only leaves it.
struct Fading {};

/// A view of a `Fading` endpoint, valid and modified, which holds `held`
/// and has a delta where `arrived`.
struct FadingView {
    using schema = Fading;
    i64 held;
    bool arrived;

    [[nodiscard]] bool valid() const { return true; }
    [[nodiscard]] bool modified() const { return true; }
};

} // namespace

/// How a `Fading` endpoint is read: its delta is what it holds, where one
/// arrived (`has_delta`).
template <>
struct hglrt::endpoint_read<Fading> {
    using delta_type = i64;

    static i64 delta(const FadingView& view) { return view.held; }
    static bool has_delta(const FadingView& view) { return view.arrived; }
};

namespace {

TEST_CASE("a form says where its endpoint ticks with no delta, which both reads see (ERR-14)") {
    CHECK(hglrt::fn_delta_value_checked(FadingView{5, true}) == hglrt::result<i64>{5});
    CHECK(hglrt::fn_delta_value(FadingView{5, true}) == 5);
    const hglrt::result<i64> none = hglrt::fn_delta_value_checked(FadingView{5, false});
    REQUIRE_FALSE(none.has_value());
    CHECK(none.error().code == "value.unset_read");
    CHECK(code_raised([] { (void)hglrt::fn_delta_value(FadingView{5, false}); }) == "value.unset_read");
}

} // namespace
