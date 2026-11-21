// Unit tests of the coroutine type of a source's body (generator.h), and of
// a body written with it driven by `resume` (sources.h).
#include "core_nodes.h"

#include <hglrt/failure.h>
#include <hglrt/generator.h>
#include <hglrt/sources.h>
#include <hglrt/testing.h>

#include <doctest/doctest.h>

#include <chrono>
#include <iterator>
#include <memory>
#include <optional>
#include <ranges>
#include <stdexcept>
#include <string>
#include <type_traits>
#include <utility>
#include <vector>

namespace {

using hglrt::datetime;
using hglrt::generator;
using hglrt::i64;
using hglrt::Yielded;
using namespace std::chrono_literals;

// The shape of `std::generator<T>`: a move-only view whose iterator is an
// input iterator giving `T&&`.
static_assert(std::ranges::input_range<generator<i64>>);
static_assert(std::ranges::view<generator<i64>>);
static_assert(std::input_iterator<generator<i64>::iterator>);
static_assert(!std::forward_iterator<generator<i64>::iterator>);
static_assert(std::same_as<std::iter_reference_t<generator<i64>::iterator>, i64&&>);
static_assert(std::same_as<std::ranges::sentinel_t<generator<i64>>, std::default_sentinel_t>);
static_assert(std::move_constructible<generator<i64>> && !std::copy_constructible<generator<i64>>);
static_assert(std::movable<generator<i64>::iterator> && !std::copyable<generator<i64>::iterator>);

/// Every element of `values`, moved out in order.
template <class T>
std::vector<T> drain(generator<T> values) {
    std::vector<T> out;
    for (T&& value : values) {
        out.push_back(std::move(value));
    }
    return out;
}

/// A body with yields in a loop and in both arms of a branch, and one after
/// the loop: 0, 10, 2, 30, 4, then -1.
generator<i64> evens_and_tens(i64 count) {
    for (i64 i = 0; i < count; ++i) {
        if (i % 2 == 0) {
            co_yield i;
        } else {
            co_yield i * 10;
        }
    }
    co_yield -1;
}

TEST_CASE("a body gives its yields in order, from loops and branches") {
    CHECK(drain(evens_and_tens(5)) == std::vector<i64>{0, 10, 2, 30, 4, -1});
    CHECK(drain(evens_and_tens(0)) == std::vector<i64>{-1});
}

TEST_CASE("a body that yields nothing is empty") {
    auto nothing = []() -> generator<i64> { co_return; };
    generator<i64> values = nothing();
    CHECK(values.begin() == std::default_sentinel);
}

TEST_CASE("a body does not run until begin, and runs one yield at a time") {
    std::vector<std::string> ran;
    auto body = [](std::vector<std::string>& ran) -> generator<i64> {
        ran.emplace_back("first");
        co_yield 1;
        ran.emplace_back("second");
        co_yield 2;
        ran.emplace_back("end");
    };
    generator<i64> values = body(ran);
    CHECK(ran.empty());
    auto at = values.begin();
    CHECK(ran == std::vector<std::string>{"first"});
    CHECK(*at == 1);
    ++at;
    CHECK(ran == std::vector<std::string>{"first", "second"});
    CHECK(*at == 2);
    at++;
    CHECK(at == std::default_sentinel);
    CHECK(ran == std::vector<std::string>{"first", "second", "end"});
    // At the end, a step does nothing.
    ++at;
    CHECK(at == std::default_sentinel);
    CHECK(ran.size() == 3);
}

TEST_CASE("a yielded lvalue is a copy, and the body's variable is kept") {
    auto body = []() -> generator<std::string> {
        std::string text = "a text long enough not to be stored inside the string";
        co_yield text;
        co_yield text;
        co_yield std::move(text);
    };
    const std::vector<std::string> taken = drain(body());
    REQUIRE(taken.size() == 3);
    CHECK(taken[0] == "a text long enough not to be stored inside the string");
    CHECK(taken[1] == taken[0]);
    CHECK(taken[2] == taken[0]);
}

TEST_CASE("a yielded rvalue is given where it is, and a move-only one can be moved out") {
    auto body = []() -> generator<std::unique_ptr<i64>> {
        co_yield std::make_unique<i64>(3);
        auto kept = std::make_unique<i64>(4);
        co_yield std::move(kept);
        CHECK(kept == nullptr);
    };
    std::vector<std::unique_ptr<i64>> taken = drain(body());
    REQUIRE(taken.size() == 2);
    CHECK(*taken[0] == 3);
    CHECK(*taken[1] == 4);
}

TEST_CASE("an exception the body lets out is thrown from the call that resumed it") {
    auto at_once = []() -> generator<i64> {
        hglrt::fail("numeric.out_of_range", "before any yield");
        co_return;
    };
    generator<i64> first = at_once();
    try {
        (void)first.begin();
        FAIL("begin did not throw");
    } catch (const hglrt::Failure& failure) {
        CHECK(failure.error().code == "numeric.out_of_range");
        CHECK(failure.error().message == "before any yield");
    }

    auto later = []() -> generator<i64> {
        co_yield 1;
        throw std::runtime_error("after the first yield");
    };
    generator<i64> second = later();
    auto at = second.begin();
    CHECK(*at == 1);
    CHECK_THROWS_WITH_AS(++at, "after the first yield", std::runtime_error);
    // The body is at its end, and is not resumed again.
    CHECK(at == std::default_sentinel);
    CHECK_NOTHROW(++at);
}

/// Counts the locals of a body that are alive.
struct Alive {
    int& count;
    explicit Alive(int& count) : count(count) { ++count; }
    Alive(const Alive&) = delete;
    Alive& operator=(const Alive&) = delete;
    ~Alive() { --count; }
};

generator<i64> counted(int& alive) {
    Alive outer{alive};
    for (i64 i = 0;; ++i) {
        Alive inner{alive};
        co_yield i;
    }
}

TEST_CASE("destroying a generator mid-way destroys the locals in its frame") {
    int alive = 0;
    {
        generator<i64> values = counted(alive);
        CHECK(alive == 0);
        auto at = values.begin();
        CHECK(alive == 2);
        ++at;
        ++at;
        CHECK(*at == 2);
        CHECK(alive == 2);
    }
    CHECK(alive == 0);
    {
        generator<i64> never_started = counted(alive);
    }
    CHECK(alive == 0);
}

TEST_CASE("a generator moved from leaves the frame with the one moved to") {
    int alive = 0;
    {
        generator<i64> first = counted(alive);
        auto at = first.begin();
        CHECK(*at == 0);
        generator<i64> second = std::move(first);
        CHECK(alive == 2);
        ++at;
        CHECK(*at == 1);
        generator<i64> third = counted(alive);
        (void)third.begin();
        CHECK(alive == 4);
        // Assigning destroys the frame the generator assigned to had.
        third = std::move(second);
        CHECK(alive == 2);
        ++at;
        CHECK(*at == 2);
    }
    CHECK(alive == 0);
}

// ---- A body written with it, driven by `resume` (SOURCE-1 to SOURCE-3) ----

/// A scheduler that keeps the wakes asked of it.
struct Wakes {
    mutable std::vector<datetime> asked;
    void schedule(datetime when) const { asked.push_back(when); }
};

/// The step `resume` takes, over a body: each call runs it on to its next
/// `yield`, as a source's node does with the body in its state.
template <class T>
struct Stepped {
    generator<Yielded<T>> body;
    std::optional<typename generator<Yielded<T>>::iterator> at{};

    std::optional<Yielded<T>> operator()() {
        if (!at.has_value()) {
            at.emplace(body.begin());
        } else {
            ++*at;
        }
        if (*at == std::default_sentinel) {
            return std::nullopt;
        }
        return std::move(**at);
    }
};

/// `fn ticks(n: i64) -> i64 { var i = 0  while i < n { yield 2us * i: i  i = i + 1 } }`,
/// its durations from each evaluation.
generator<Yielded<i64>> ticks(i64 n) {
    for (i64 i = 0; i < n; ++i) {
        co_yield Yielded<i64>{hglrt::duration{2 * i}, i};
    }
}

TEST_CASE("a source's body as a generator, driven by resume (SOURCE-3)") {
    using hglrt_test::start;
    Wakes wakes;
    hglrt::Source<i64> source;
    Stepped<i64> body{ticks(3)};
    using Published = std::vector<std::pair<datetime, i64>>;
    Published published;
    datetime now = start;
    auto publish = [&](i64 value) { published.emplace_back(now, value); };

    // "a current target publishes now; a future target retains its payload
    // and suspends": 0 at once, 1 two microseconds on.
    hglrt::resume(source, now, wakes, body, publish);
    const Published first{{start, 0}};
    CHECK(published == first);
    REQUIRE(wakes.asked == std::vector<datetime>{start + 2us});
    // "On wake it publishes and resumes after yield": 2 is four
    // microseconds after that wake.
    now = start + 2us;
    hglrt::resume(source, now, wakes, body, publish);
    const Published second{{start, 0}, {start + 2us, 1}};
    CHECK(published == second);
    REQUIRE(wakes.asked.back() == start + 6us);
    now = start + 6us;
    hglrt::resume(source, now, wakes, body, publish);
    const Published third{{start, 0}, {start + 2us, 1}, {start + 6us, 2}};
    CHECK(published == third);
    // "body completion finishes the source" (SOURCE-4).
    CHECK(body.at.has_value());
    CHECK(*body.at == std::default_sentinel);
    CHECK(wakes.asked.size() == 2);
}

} // namespace
