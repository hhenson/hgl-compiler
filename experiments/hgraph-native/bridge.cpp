#include "native.hpp"
#include <cstdio>
#include <cstdlib>
#include <exception>
using namespace bench;

// All ABI calls run synchronously. A failed benchmark aborts with an infrastructure
// diagnostic. No C++ exception may unwind across the Rust caller.
template <class F> static auto guarded(F f) noexcept {
    try {
        return f();
    } catch (const std::exception &e) {
        std::fprintf(stderr, "bridge: %s\n", e.what());
    } catch (...) {
        std::fputs("unknown bridge failure\n", stderr);
    }
    std::abort();
}
extern "C" {
unsigned hg_source(Plan *p, Input input) noexcept {
    return guarded([&] { return p->source(input); });
}
unsigned hg_node(Plan *p, unsigned graph, const Type *inputs, std::size_t count, const Type *out,
                 bool state, Hook eval, Hook start, Hook stop, unsigned arg) noexcept {
    return guarded([&] { return p->node(graph, {inputs, count}, out, state, eval, start, stop, arg); });
}
void hg_edge(Plan *p, unsigned g, unsigned a, unsigned b, unsigned slot) noexcept {
    guarded([&] { p->edge(g, a, b, slot); });
}
unsigned hg_map(Plan *p, ChildPort key, ChildPort value) noexcept {
    return guarded([&] { return p->map(key, value); });
}
void hg_keys(Plan *p, unsigned a, unsigned b) noexcept {
    guarded([&] { p->keys(a, b); });
}
void hg_record(Plan *p, unsigned a, unsigned lane) noexcept {
    guarded([&] { p->record(a, lane); });
}
std::int64_t hg_read(Frame *f, unsigned slot) noexcept {
    return guarded([&] { return f->integer(slot); });
}
const char *hg_text(Frame *f, unsigned slot, std::size_t *size) noexcept {
    return guarded([&] {
        auto v = f->text(slot);
        *size = v.size();
        return v.data();
    });
}
std::int64_t hg_state(Frame *f) noexcept {
    return guarded([&] { return f->state(); });
}
void hg_set_state(Frame *f, std::int64_t v) noexcept {
    guarded([&] { f->set_state(v); });
}
std::int64_t hg_get(Frame *f, const char *s, std::size_t n) noexcept {
    return guarded([&] { return f->get({s, n}); });
}
void hg_put(Frame *f, const char *s, std::size_t n, std::int64_t v) noexcept {
    guarded([&] { f->put({s, n}, v); });
}
void hg_publish(Frame *f, std::int64_t v) noexcept {
    guarded([&] { f->publish(v); });
}
void hg_update(Frame *f, const char *s, std::size_t n, std::int64_t v) noexcept {
    guarded([&] { f->update({s, n}, v); });
}
void hg_remove(Frame *f, const char *s, std::size_t n) noexcept {
    guarded([&] { f->remove({s, n}); });
}
int hg_run(int n, const char *const *args, Wire wire) noexcept {
    return guarded([&] { return run(n, args, wire); });
}
}
