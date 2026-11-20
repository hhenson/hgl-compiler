#include "native.hpp"
#include <array>
#include <charconv>
#include <chrono>
#include <cstdio>
#include <cstdlib>
#include <hgraph/lib/testing/runtime_support.h>
#include <hgraph/runtime/map_node.h>
#include <hgraph/runtime/runtime.h>
#include <hgraph/types/static_node.h>
#include <limits>
#include <optional>
#include <stdexcept>

namespace bench {
using namespace hgraph;
using Number = std::int64_t;
static const NodeView &view(const Frame &f) { return *static_cast<const NodeView *>(f.node); }
static DateTime at(const Frame &f) { return DateTime{TimeDelta{f.time}}; }
Number Frame::integer(unsigned slot) const {
    auto root = view(*this).input(at(*this));
    auto bundle = root.as_bundle();
    auto input = bundle[slot];
    return input.value().checked_as<Number>();
}
std::string_view Frame::text(unsigned slot) const {
    auto root = view(*this).input(at(*this));
    auto bundle = root.as_bundle();
    auto input = bundle[slot];
    return input.value().checked_as<std::string>();
}
Number Frame::state() const { return view(*this).state().checked_as<Number>(); }
void Frame::set_state(Number value) const { view(*this).state().begin_mutation().set_scalar(value); }
Number Frame::get(std::string_view key) const {
    auto gs = view(*this).global_state();
    return gs.contains(key) ? gs.get_as<Number>(key) : 0;
}
void Frame::put(std::string_view key, Number value) const {
    view(*this).global_state().set(key, Value{value});
}
void Frame::publish(Number value) const { testing::set_output_value(view(*this), at(*this), value); }
void Frame::update(std::string_view key, Number value) const {
    auto out = view(*this).output(at(*this));
    out.as_dict().begin_mutation(at(*this)).set(Value{std::string(key)}.view(), Value{value}.view());
}
void Frame::remove(std::string_view key) const {
    auto out = view(*this).output(at(*this));
    static_cast<void>(out.as_dict().begin_mutation(at(*this)).erase(Value{std::string(key)}.view()));
}

struct Digest {
    std::uint64_t hash = 14695981039346656037ULL;
    std::size_t count = 0;
    void add(std::size_t step, Number value) {
        hash = (hash ^ step) * 1099511628211ULL;
        hash = (hash ^ static_cast<std::uint64_t>(value)) * 1099511628211ULL;
        ++count;
    }
    bool operator==(const Digest &) const = default;
};
struct Plan::Data {
    std::array<GraphBuilder, 2> graphs;
    std::array<Digest, 3> digests;
    using Trace = std::vector<std::pair<std::size_t, Number>>;
    std::array<Trace, 3> fixture;
    bool check_fixture = false;
    std::size_t cycles;
    std::size_t publications = 0;
};
static auto schema(Type type) -> const TSValueTypeMetaData * {
    auto &r = TypeRegistry::instance();
    switch (type) {
    case Type::integer:
        return r.ts(scalar_descriptor<Number>::value_meta());
    case Type::text:
        return r.ts(scalar_descriptor<std::string>::value_meta());
    case Type::flag:
        return r.ts(scalar_descriptor<bool>::value_meta());
    case Type::dictionary:
        return r.tsd(scalar_descriptor<std::string>::value_meta(), schema(Type::integer));
    }
    throw std::invalid_argument("unknown schema");
}
unsigned Plan::source(Input input) {
    NodeTypeMetaData meta;
    meta.display_name = "replay";
    meta.node_kind = NodeKind::PullSource;
    meta.output_schema = schema(input == Input::key     ? Type::text
                                : input == Input::probe ? Type::flag
                                                        : Type::integer);
    meta.schedule_on_start = true;
    NodeCallbacks hooks;
    hooks.evaluate = [input, &d = data](const NodeView &n, DateTime now) {
        auto step = static_cast<std::size_t>((now - MIN_ST) / MIN_TD);
        if (step + 1 < d.cycles)
            n.graph_value()->schedule_node(n.node_index(), now + MIN_TD);
        if (input == Input::count || input == Input::relay) {
            auto pos = step % 5;
            if (pos == 1 || pos == 4)
                return;
            testing::set_output_value(n, now, Number{input == Input::relay && pos == 3 ? 9 : 7});
        } else if (input == Input::probe) {
            if (step % 2 == 0)
                return;
            testing::set_output_value(n, now, true);
        } else {
            if (step % 2)
                return;
            auto event = (step % 10) / 2;
            if (input == Input::key)
                testing::set_output_value(n, now, std::string(event == 1 ? "b" : "a"));
            else
                testing::set_output_value(n, now, std::array<Number, 5>{1, 7, 2, -1, 5}[event]);
        }
        ++d.publications;
    };
    auto &g = data.graphs[0];
    unsigned index = g.node_count();
    g.add_node(NodeBuilder::native(std::move(meta), std::move(hooks)));
    return index;
}
static auto callback(Hook hook, unsigned argument) -> std::function<void(const NodeView &, DateTime)> {
    if (!hook)
        return {};
    return [hook, argument](const NodeView &n, DateTime now) {
        Frame f{&n, now.time_since_epoch().count()};
        hook(&f, argument);
    };
}
unsigned Plan::node(unsigned graph, std::span<const Type> inputs, const Type *output, bool state,
                    Hook eval, Hook start, Hook stop, unsigned arg) {
    auto &g = data.graphs.at(graph);
    NodeTypeMetaData meta;
    meta.display_name = "case_node";
    meta.node_kind = output ? NodeKind::Compute : NodeKind::Sink;
    meta.output_schema = output ? schema(*output) : nullptr;
    meta.state_schema = state ? scalar_descriptor<Number>::value_meta() : nullptr;
    meta.uses_global_state = true;
    std::vector<std::pair<std::string, const TSValueTypeMetaData *>> fields;
    std::vector<TSEndpointSchema> endpoints;
    for (auto t : inputs) {
        fields.emplace_back("arg" + std::to_string(fields.size()), schema(t));
        endpoints.push_back(TSEndpointSchema::peered(schema(t)));
    }
    meta.input_schema = TypeRegistry::instance().un_named_tsb(fields);
    auto ep = TSEndpointSchema::non_peered(meta.input_schema, std::move(endpoints));
    NodeCallbacks hooks;
    hooks.start = callback(start, arg);
    hooks.evaluate = callback(eval, arg);
    hooks.stop = callback(stop, arg);
    unsigned id = g.node_count();
    g.add_node(NodeBuilder::native(std::move(meta), std::move(hooks), std::move(ep)));
    return id;
}
void Plan::edge(unsigned graph, unsigned from, unsigned to, unsigned slot) {
    data.graphs.at(graph).add_edge({from, {}, to, {slot}});
}
unsigned Plan::map(ChildPort key, ChildPort value) {
    auto *dict = schema(Type::dictionary);
    auto *keys = TypeRegistry::instance().tss(scalar_descriptor<std::string>::value_meta());
    NodeTypeMetaData meta;
    meta.display_name = "each_price";
    meta.node_kind = NodeKind::Sink;
    meta.input_schema = TypeRegistry::instance().un_named_tsb({{"prices", dict}, {"__keys__", keys}});
    MapNodeSpec spec;
    spec.child.graph_builder = std::move(data.graphs[1]);
    spec.child.input_bindings = {{{0}, {key.node, {key.slot}}, schema(Type::text)},
                                 {{1}, {value.node, {value.slot}}, schema(Type::integer)}};
    spec.args = {{MapArgSourceKind::Key, 0}, {MapArgSourceKind::Element, 0}};
    spec.multiplexed_inputs = {0};
    spec.keys_input_index = 1;
    spec.key_output_schema = schema(Type::text);
    auto &g = data.graphs[0];
    unsigned id = g.node_count();
    g.add_node(map_node(std::move(meta), std::move(spec)));
    return id;
}
void Plan::keys(unsigned from, unsigned to) {
    data.graphs[0].add_edge({from, {ts_key_set_path_component}, to, {1}});
}
void Plan::record(unsigned from, unsigned lane) {
    auto &g = data.graphs[0];
    NodeTypeMetaData meta;
    meta.display_name = "record";
    meta.node_kind = NodeKind::Sink;
    meta.input_schema = testing::single_input_schema(*schema(Type::integer));
    NodeCallbacks hooks;
    hooks.evaluate = [lane, &d = data](const NodeView &n, DateTime now) {
        auto root = n.input(now);
        auto bundle = root.as_bundle();
        auto input = bundle[0];
        auto v = input.value().checked_as<Number>();
        auto step = static_cast<std::size_t>((now - MIN_ST) / MIN_TD);
        d.digests.at(lane).add(step, v);
        if (d.check_fixture)
            d.fixture.at(lane).emplace_back(step, v);
    };
    unsigned id = g.node_count();
    auto ep = TSEndpointSchema::non_peered(meta.input_schema,
                                           {TSEndpointSchema::peered(schema(Type::integer))});
    g.add_node(NodeBuilder::native(std::move(meta), std::move(hooks), std::move(ep)));
    edge(0, from, id, 0);
}
// Independent, timestamp-sensitive oracle, including child lifetimes across repetitions.
static auto expected(bool keyed, std::size_t cycles, Number offset) {
    std::array<Digest, 3> out;
    Number count = 0, a = 0, starts = 0, stops = 0;
    bool a_live = false, b_live = false;
    for (std::size_t i = 0; i < cycles; ++i) {
        if (!keyed) {
            auto pos = i % 5;
            if (pos == 1 || pos == 4)
                continue;
            out[0].add(i, ++count);
            out[1].add(i, (pos == 3 ? 9 : 7) + offset);
        } else if (i % 2) {
            out[0].add(i, a + offset);
            out[1].add(i, starts + offset);
            out[2].add(i, stops + offset);
        } else {
            switch (i % 10) {
            case 2:
                if (!b_live) {
                    ++starts;
                    b_live = true;
                }
                break;
            case 6:
                ++stops;
                a_live = false;
                break;
            default:
                if (!a_live) {
                    ++starts;
                    a = 0;
                    a_live = true;
                }
                ++a;
            }
        }
    }
    return out;
}
// Literal publications from the HGL fixture. Omitted instants must stay silent.
static void check_fixture(const Plan::Data &data, bool keyed, Number offset) {
    using Trace = Plan::Data::Trace;
    std::array<Trace, 3> want;
    if (keyed) {
        want = {Trace{{1, 1}, {3, 1}, {5, 2}, {7, 2}, {9, 1}},
                Trace{{1, 1}, {3, 2}, {5, 2}, {7, 2}, {9, 3}},
                Trace{{1, 0}, {3, 0}, {5, 0}, {7, 1}, {9, 1}}};
    } else {
        want = {Trace{{0, 1}, {2, 2}, {3, 3}}, Trace{{0, 7}, {2, 7}, {3, 9}}, Trace{}};
    }
    for (unsigned lane = 0; lane < want.size(); ++lane) {
        if (keyed || lane == 1)
            for (auto &entry : want[lane])
                entry.second += offset;
        if (want[lane] != data.fixture[lane])
            throw std::runtime_error("literal HGL trace mismatch on lane " + std::to_string(lane));
    }
}
int run(int argc, const char *const *argv, Wire wire) {
    try {
        if (argc != 4)
            throw std::invalid_argument("usage: program scalar|keyed repetitions expected-offset");
        const std::string_view name = argv[1];
        const bool keyed = name == "keyed";
        if (!keyed && name != "scalar")
            throw std::invalid_argument("unknown case");
        auto reps = std::stoul(argv[2]);
        if (reps == 0 || reps > 10000000)
            throw std::invalid_argument("repetition count out of range");
        Plan::Data data{};
        data.check_fixture = reps == 1;
        data.cycles = reps * (keyed ? 10 : 5);
        Plan plan{data};
        wire(&plan, keyed, 0);
        GraphExecutorBuilder builder;
        builder.graph_builder(std::move(data.graphs[0]))
            .start_time(MIN_ST)
            .end_time(MIN_ST + MIN_TD * data.cycles);
        auto executor = builder.make_executor();
        auto start = std::chrono::steady_clock::now();
        executor.view().run();
        auto ns = std::chrono::duration_cast<std::chrono::nanoseconds>(std::chrono::steady_clock::now() -
                                                                       start)
                      .count();
        auto offset = std::stoll(argv[3]);
        if (data.check_fixture)
            check_fixture(data, keyed, offset);
        auto oracle = expected(keyed, data.cycles, offset);
        if (data.digests != oracle || data.publications != reps * (keyed ? 15 : 6)) {
            for (int i = 0; i < 3; ++i)
                std::fprintf(stderr, "lane %d: %zu/%zu %llu/%llu\n", i, data.digests[i].count,
                             oracle[i].count, (unsigned long long)data.digests[i].hash,
                             (unsigned long long)oracle[i].hash);
            throw std::runtime_error("publication count or timestamp/value digest differs");
        }
        std::printf("%zu %lld", data.publications, (long long)ns);
        for (auto lane : data.digests)
            std::printf(" %zu %llu", lane.count, (unsigned long long)lane.hash);
        std::puts("");
        return 0;
    } catch (const std::exception &e) {
        std::fprintf(stderr, "%s\n", e.what());
        return 1;
    }
}
} // namespace bench
