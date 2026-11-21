// hgraph alone, with no libhglrt and no generated code: what hgraph gives for
// the read of a window's values before the window is all valid, the one read
// of a window in which HGL's text leads hgraph's, and which `windows.h`
// guards (`window_value`, `window_value_checked`).
//
// TYPE-11: "A read of [a window's value] before the window is `all_valid`
// fails with `value.unset_read`, or gives an `err` of an `error` with that
// code where its place takes a `result<list<V>, error>`". hgraph 0.9.23 reads
// the values the window holds whatever its minimum: below it, a window of a
// tick count and one of a duration each give the one value they hold.
//
// The program exits with 0 while hgraph reads so, and with 1, saying what it
// read, once it does not: `windows.h`'s guard then stands for nothing, and is
// to be looked at again.
#include <hgraph/runtime/executor.h>
#include <hgraph/types/generated.h>

#include <chrono>
#include <cstddef>
#include <cstdio>
#include <vector>

namespace {

using hgraph::Int;

/// Publishes 1 to its window at the start, and nothing after.
template <class W>
struct One {
    static constexpr auto name = "hgraph_window_read.one";
    static constexpr bool schedule_on_start = true;

    static void eval(hgraph::Out<W> out) { out.push(Int{1}); }
};

/// What the reader read in its first evaluation, the cycle of the arrival:
/// whether the window was all valid, and the values hgraph gave for it.
struct Read {
    bool read = false;
    bool all_valid = true;
    std::vector<Int> values;
};

Read read;

/// Reads the window's values in its first evaluation, as hgraph gives them.
template <class W>
struct Reader {
    static constexpr auto name = "hgraph_window_read.reader";

    static void eval(hgraph::In<"window", W> window) {
        if (read.read) {
            return;
        }
        read.read = true;
        read.all_valid = window.all_valid();
        for (std::size_t index = 0; index < window.size(); ++index) {
            read.values.push_back(window.at(index));
        }
    }
};

/// Whether hgraph reads the one value a window `W` below its minimum holds.
template <class W>
bool reads_below_the_minimum(const char* window) {
    read = Read{};
    hgraph::Wiring wiring;
    hgraph::wire<Reader<W>>(wiring, hgraph::wire<One<W>>(wiring));
    hgraph::GraphExecutorBuilder builder;
    builder.graph_builder(std::move(wiring).finish())
        .mode(hgraph::GraphExecutorMode::Simulation)
        .start_time(hgraph::MIN_ST)
        .end_time(hgraph::MIN_ST + std::chrono::microseconds(10));
    hgraph::GraphExecutorValue executor = builder.make_executor();
    executor.view().run();
    const bool partial = read.read && !read.all_valid && read.values == std::vector<Int>{1};
    std::printf("%s below its minimum: all_valid %s, %zu value(s) read%s\n", window,
                read.all_valid ? "true" : "false", read.values.size(),
                partial ? ", as hgraph has read it so far" : ": hgraph reads otherwise now");
    return partial;
}

} // namespace

int main() {
    const bool ticks = reads_below_the_minimum<hgraph::TSW<Int, 3, 2>>("a window of 3 ticks, 2 at least,");
    const bool duration =
        reads_below_the_minimum<hgraph::TSWDuration<Int, 3, 1>>("a window of 3us, 1us at least,");
    return ticks && duration ? 0 : 1;
}
