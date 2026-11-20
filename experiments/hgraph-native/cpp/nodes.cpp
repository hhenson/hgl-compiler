#include "nodes.hpp"
#include <array>
using bench::Frame;
namespace cases {
constexpr int body_bias = 0;
unsigned wire_offset(unsigned base) { return base; }
void count(Frame *f, unsigned) {
    auto next = f->state() + 1;
    f->set_state(next);
    f->publish(next);
}
void relay(Frame *f, unsigned offset) { f->publish(f->integer(0) + offset + body_bias); }
void priced(Frame *f, unsigned) {
    auto key = f->text(0);
    auto price = f->integer(1);
    if (price < 0)
        f->remove(key);
    else
        f->update(key, price);
}
void start(Frame *f, unsigned) { f->put("started", f->get("started") + 1); }
void child(Frame *f, unsigned) {
    auto seen = f->state() + 1;
    f->set_state(seen);
    if (f->text(0) == "a")
        f->put("a", seen);
}
void stop(Frame *f, unsigned) { f->put("stopped", f->get("stopped") + 1); }
void read_store(Frame *f, unsigned lane) {
    constexpr std::array names = {"a", "started", "stopped"};
    f->publish(f->get(names.at(lane % 3)) + lane / 3 + body_bias);
}
} // namespace cases
