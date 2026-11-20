// Borrowed callback and graph-builder ABI for the first HGL native slice.
#include <cstdio>
#include <cstdlib>
#include <hgraph/runtime/map_node.h>
#include <hgraph/runtime/runtime.h>
#include <hgraph/types/prepared_graph.h>
#include <hgraph/types/static_node.h>
#include <optional>
#include <variant>

namespace {
using namespace hgraph;
using Number = std::int64_t;
enum class Type : unsigned { Integer, Text, Flag, Dictionary };
struct Event {
  Number step, integer;
  const char *text;
  std::size_t size;
};
struct Port {
  unsigned node, slot;
};
struct CallbackFrame {
  const NodeView &node;
  DateTime time;
};
using Hook = int (*)(const CallbackFrame *);
using Observe = void (*)(void *, Number, Number);
struct Plan {
  PreparedGraph root, child;
  void *context;
  Observe observe;
  Number last{};
  PreparedGraph &graph(bool nested) { return nested ? child : root; }
};
// Unexpected exceptions in individual FFI accessors are infrastructure
// failures. An HGL callback returns a status; the enclosing engine invocation
// reports it.
template <class F> auto guarded(F f) noexcept {
  try {
    return f();
  } catch (const std::exception &e) {
    std::fprintf(stderr, "HGraph bridge: %s\n", e.what());
  } catch (...) {
    std::fputs("unknown HGraph bridge failure\n", stderr);
  }
  std::abort();
}
const TSValueTypeMetaData *schema(Type t) {
  auto &r = TypeRegistry::instance();
  switch (t) {
  case Type::Integer:
    return r.ts(scalar_descriptor<Number>::value_meta());
  case Type::Text:
    return r.ts(scalar_descriptor<std::string>::value_meta());
  case Type::Flag:
    return r.ts(scalar_descriptor<bool>::value_meta());
  case Type::Dictionary:
    return r.tsd(scalar_descriptor<std::string>::value_meta(),
                 schema(Type::Integer));
  }
  throw std::invalid_argument("unknown HGL payload");
}
template <class Read>
auto read_input(const CallbackFrame &f, unsigned i, Read read) {
  auto root = f.node.input(f.time);
  auto bundle = root.as_bundle();
  auto port = bundle[i];
  return read(port);
}
void publish(const NodeView &n, DateTime t, Value v) {
  auto change = n.output(t).begin_mutation(t);
  if (!change.move_value_from(std::move(v)))
    throw std::logic_error("output schema mismatch");
}
auto callback(Hook h) -> std::function<void(const NodeView &, DateTime)> {
  if (!h)
    return {};
  return [h](const NodeView &n, DateTime t) {
    CallbackFrame f{n, t};
    switch (h(&f)) {
    case 0:
      return;
    case 1:
      throw std::runtime_error("numeric.out_of_range");
    case 2:
      throw std::runtime_error("global store has no such key");
    default:
      throw std::logic_error("unexpected Rust callback status");
    }
  };
}
unsigned add(PreparedGraph &g, NodeBuilder n) {
  auto id = g.nodes.size();
  g.nodes.push_back({id, std::move(n)});
  return id;
}
} // namespace
extern "C" {
unsigned hg_replay(Plan *p, Type ty, const Event *events,
                   std::size_t len) noexcept {
  return guarded([&] {
    NodeTypeMetaData m;
    m.display_name = "hgl replay";
    m.node_kind = NodeKind::PullSource;
    m.output_schema = schema(ty);
    m.schedule_on_start = len != 0;
    std::vector<std::pair<Number, Value>> values;
    for (std::size_t i = 0; i < len; ++i) {
      const auto &e = events[i];
      if (e.step < 0 || (i && events[i - 1].step >= e.step) ||
          e.step >= (MAX_ET - MIN_ST) / MIN_TD)
        throw std::out_of_range("replay time");
      p->last = std::max(p->last, e.step);
      switch (ty) {
      case Type::Integer:
        values.emplace_back(e.step, Value{e.integer});
        break;
      case Type::Text:
        values.emplace_back(e.step, Value{std::string(e.text, e.size)});
        break;
      case Type::Flag:
        values.emplace_back(e.step, Value{e.integer != 0});
        break;
      default:
        throw std::invalid_argument("replay schema");
      }
    }
    NodeCallbacks h;
    h.evaluate = [values = std::move(values)](const NodeView &n, DateTime t) {
      auto step = (t - MIN_ST) / MIN_TD;
      auto it =
          std::lower_bound(values.begin(), values.end(), step,
                           [](const auto &e, Number s) { return e.first < s; });
      if (it != values.end() && it->first == step) {
        publish(n, t, it->second);
        ++it;
      }
      if (it != values.end())
        n.graph_value()->schedule_node(n.node_index(),
                                       MIN_ST + MIN_TD * it->first);
    };
    return add(p->root, NodeBuilder::native(std::move(m), std::move(h)));
  });
}
unsigned hg_node(Plan *p, bool nested, const Type *types, std::size_t len,
                 const Type *out, bool state, Hook start, Hook eval,
                 Hook stop) noexcept {
  return guarded([&] {
    NodeTypeMetaData m;
    m.display_name = "HGL node";
    m.node_kind = out ? NodeKind::Compute : NodeKind::Sink;
    m.output_schema = out ? schema(*out) : nullptr;
    m.state_schema = state ? scalar_descriptor<Number>::value_meta() : nullptr;
    m.uses_global_state = true;
    // The lowered Admit operation checks each handler's input mask.
    m.valid_inputs = std::vector<std::size_t>{};
    std::vector<std::pair<std::string, const TSValueTypeMetaData *>> fields;
    std::vector<TSEndpointSchema> slots;
    for (std::size_t i = 0; i < len; ++i) {
      auto *s = schema(types[i]);
      fields.emplace_back("arg" + std::to_string(i), s);
      slots.push_back(TSEndpointSchema::peered(s));
    }
    m.input_schema = TypeRegistry::instance().un_named_tsb(fields);
    auto ep = TSEndpointSchema::non_peered(m.input_schema, std::move(slots));
    NodeCallbacks h;
    h.start = callback(start);
    h.evaluate = callback(eval);
    h.stop = callback(stop);
    return add(p->graph(nested),
               NodeBuilder::native(std::move(m), std::move(h), std::move(ep)));
  });
}
void hg_edge(Plan *p, bool child, unsigned from, unsigned to,
             unsigned slot) noexcept {
  guarded([&] {
    p->graph(child).connections.push_back({PreparedOutput{from}, {to, {slot}}});
  });
}
void hg_each(Plan *p, unsigned dict, Port key, Port value) noexcept {
  guarded([&] {
    const auto *keys = TypeRegistry::instance().tss(
        scalar_descriptor<std::string>::value_meta());
    NodeTypeMetaData m;
    m.display_name = "HGL keyed for";
    m.node_kind = NodeKind::Sink;
    m.input_schema = TypeRegistry::instance().un_named_tsb(
        {{"entries", schema(Type::Dictionary)}, {"__keys__", keys}});
    MapNodeSpec spec;
    auto body = std::exchange(p->child, PreparedGraph{});
    body.connections.push_back({PreparedParentInput{0, {}, schema(Type::Text)},
                                {key.node, {key.slot}}});
    body.connections.push_back(
        {PreparedParentInput{1, {}, schema(Type::Integer)},
         {value.node, {value.slot}}});
    spec.child = body.finish_child(2).child;
    spec.args = {{MapArgSourceKind::Key, 0}, {MapArgSourceKind::Element, 0}};
    spec.multiplexed_inputs = {0};
    spec.keys_input_index = 1;
    spec.key_output_schema = schema(Type::Text);
    auto id = add(p->root, map_node(std::move(m), std::move(spec)));
    p->root.connections.push_back({PreparedOutput{dict}, {id, {0}}});
    p->root.connections.push_back(
        {PreparedOutput{dict, {ts_key_set_path_component}, keys}, {id, {1}}});
  });
}
void hg_record(Plan *p, unsigned from) noexcept {
  guarded([&] {
    auto *ty = schema(Type::Integer);
    NodeTypeMetaData m;
    m.display_name = "HGL record";
    m.node_kind = NodeKind::Sink;
    m.input_schema = TypeRegistry::instance().un_named_tsb({{"value", ty}});
    auto ep = TSEndpointSchema::non_peered(m.input_schema,
                                           {TSEndpointSchema::peered(ty)});
    NodeCallbacks h;
    h.evaluate = [p](const NodeView &n, DateTime t) {
      p->observe(p->context, (t - MIN_ST) / MIN_TD,
                 read_input({n, t}, 0, [](const TSInputView &v) {
                   return v.value().checked_as<Number>();
                 }));
    };
    auto id = add(p->root, NodeBuilder::native(std::move(m), std::move(h),
                                               std::move(ep)));
    p->root.connections.push_back({PreparedOutput{from}, {id, {0}}});
  });
}
bool hg_valid(const CallbackFrame *f, unsigned i) noexcept {
  return guarded([&] {
    return read_input(*f, i, [](const TSInputView &v) { return v.valid(); });
  });
}
bool hg_modified(const CallbackFrame *f, unsigned i) noexcept {
  return guarded([&] {
    return read_input(*f, i, [](const TSInputView &v) { return v.modified(); });
  });
}
Number hg_integer(const CallbackFrame *f, unsigned i) noexcept {
  return guarded([&] {
    return read_input(*f, i, [](const TSInputView &v) {
      return v.value().checked_as<Number>();
    });
  });
}
bool hg_flag(const CallbackFrame *f, unsigned i) noexcept {
  return guarded([&] {
    return read_input(*f, i, [](const TSInputView &v) {
      return v.value().checked_as<bool>();
    });
  });
}
const char *hg_text(const CallbackFrame *f, unsigned i,
                    std::size_t *size) noexcept {
  return guarded([&] {
    return read_input(*f, i, [&](const TSInputView &v) {
      auto value = v.value();
      const auto &s = value.checked_as<std::string>();
      *size = s.size();
      return s.data();
    });
  });
}
Number hg_state(const CallbackFrame *f) noexcept {
  return guarded([&] { return f->node.state().checked_as<Number>(); });
}
void hg_set(const CallbackFrame *f, Number v) noexcept {
  guarded([&] { f->node.state().begin_mutation().set_scalar(v); });
}
bool hg_has(const CallbackFrame *f, const char *s, std::size_t n) noexcept {
  return guarded([&] { return f->node.global_state().contains({s, n}); });
}
Number hg_get(const CallbackFrame *f, const char *s, std::size_t n) noexcept {
  return guarded([&] { return f->node.global_state().get_as<Number>({s, n}); });
}
void hg_put(const CallbackFrame *f, const char *s, std::size_t n,
            Number v) noexcept {
  guarded([&] { f->node.global_state().set({s, n}, Value{v}); });
}
void hg_publish(const CallbackFrame *f, Number v) noexcept {
  guarded([&] { publish(f->node, f->time, Value{v}); });
}
void hg_publish_flag(const CallbackFrame *f, bool v) noexcept {
  guarded([&] { publish(f->node, f->time, Value{v}); });
}
void hg_publish_text(const CallbackFrame *f, const char *s,
                     std::size_t n) noexcept {
  guarded([&] { publish(f->node, f->time, Value{std::string(s, n)}); });
}
void hg_update(const CallbackFrame *f, const char *s, std::size_t n,
               Number v) noexcept {
  guarded([&] {
    auto out = f->node.output(f->time);
    out.as_dict().begin_mutation(f->time).set(Value{std::string(s, n)}.view(),
                                              Value{v}.view());
  });
}
void hg_remove(const CallbackFrame *f, const char *s, std::size_t n) noexcept {
  guarded([&] {
    auto out = f->node.output(f->time);
    static_cast<void>(out.as_dict().begin_mutation(f->time).erase(
        Value{std::string(s, n)}.view()));
  });
}
int hg_run(void (*wire)(Plan *), void *context, Observe observe) noexcept {
  try {
    Plan p{{}, {}, context, observe};
    wire(&p);
    GraphExecutorBuilder b;
    b.graph_builder(p.root.finish().graph_builder)
        .start_time(MIN_ST)
        .end_time(MIN_ST + MIN_TD * (p.last + 1));
    auto executor = b.make_executor();
    executor.view().run();
    return 0;
  } catch (const std::exception &e) {
    std::fprintf(stderr, "%s\n", e.what());
    return 1;
  } catch (...) {
    std::fputs("unknown HGraph execution failure\n", stderr);
    return 1;
  }
}
}

// CRUN-8: format the logical timestamp after the successful run.
extern "C" void hg_print(std::int64_t step, std::int64_t value) noexcept {
  guarded([&] {
    auto t = MIN_ST + MIN_TD * step;
    auto day = std::chrono::floor<std::chrono::days>(t);
    auto date = std::chrono::year_month_day{day};
    auto time = std::chrono::hh_mm_ss{t - day};
    std::printf("%04d-%02u-%02uT%02lld:%02lld:%02lld.%06lldZ  %lld\n",
                int(date.year()), unsigned(date.month()), unsigned(date.day()),
                (long long)time.hours().count(),
                (long long)time.minutes().count(),
                (long long)time.seconds().count(),
                (long long)time.subseconds().count(), (long long)value);
    std::fflush(stdout);
  });
}
