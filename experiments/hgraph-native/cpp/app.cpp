#include "nodes.hpp"
#include <array>
using namespace bench;

static void wire(Plan *p, bool keyed, unsigned base) {
    constexpr unsigned root = 0, child_graph = 1;
    constexpr unsigned wiring_bias = 0;
    const unsigned offset = cases::wire_offset(base) + wiring_bias;
    constexpr Type i = Type::integer, s = Type::text, b = Type::flag, d = Type::dictionary;
    if (!keyed) {
        auto a = p->source(Input::count);
        auto r = p->source(Input::relay);
        auto counter = p->node(root, std::array{i}, &i, true, cases::count);
        auto relay = p->node(root, std::array{i}, &i, false, cases::relay, nullptr, nullptr, offset);
        p->edge(root, a, counter, 0);
        p->edge(root, r, relay, 0);
        p->record(counter, 0);
        p->record(relay, 1);
        return;
    }
    auto key = p->source(Input::key);
    auto price = p->source(Input::price);
    auto probe = p->source(Input::probe);
    auto prices = p->node(root, std::array{s, i}, &d, false, cases::priced);
    p->edge(root, key, prices, 0);
    p->edge(root, price, prices, 1);
    auto child =
        p->node(child_graph, std::array{s, i}, nullptr, true, cases::child, cases::start, cases::stop);
    auto map = p->map({child, 0}, {child, 1});
    p->edge(root, prices, map, 0);
    p->keys(prices, map);
    for (unsigned lane = 0; lane < 3; ++lane) {
        auto read = p->node(root, std::array{b}, &i, false, cases::read_store, nullptr, nullptr,
                            lane + 3 * offset);
        p->edge(root, probe, read, 0);
        p->record(read, lane);
    }
}
int main(int argc, char **argv) { return run(argc, argv, wire); }
