#pragma once
#include <cstddef>
#include <cstdint>
#include <span>
#include <string_view>

namespace bench {
enum class Type : unsigned { integer, text, flag, dictionary };
enum class Input : unsigned { count, relay, key, price, probe };
struct Frame {
    const void *node;
    std::int64_t time;
    std::int64_t integer(unsigned slot) const;
    std::string_view text(unsigned slot) const;
    std::int64_t state() const;
    void set_state(std::int64_t value) const;
    std::int64_t get(std::string_view key) const;
    void put(std::string_view key, std::int64_t value) const;
    void publish(std::int64_t value) const;
    void update(std::string_view key, std::int64_t value) const;
    void remove(std::string_view key) const;
};
struct ChildPort {
    unsigned node, slot;
};
using Hook = void (*)(Frame *, unsigned);
class Plan {
  public:
    struct Data;
    Data &data;
    unsigned source(Input input);
    unsigned node(unsigned graph, std::span<const Type> inputs, const Type *output, bool state,
                  Hook eval, Hook start = nullptr, Hook stop = nullptr, unsigned argument = 0);
    void edge(unsigned graph, unsigned from, unsigned to, unsigned slot);
    unsigned map(ChildPort key, ChildPort value);
    void keys(unsigned from, unsigned to);
    void record(unsigned from, unsigned lane);
};
using Wire = void (*)(Plan *, bool keyed, unsigned offset);
int run(int argc, const char *const *argv, Wire wire);
} // namespace bench
