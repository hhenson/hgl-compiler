#pragma once
#include "native.hpp"

namespace cases {
unsigned wire_offset(unsigned base);
void count(bench::Frame *, unsigned);
void relay(bench::Frame *, unsigned);
void priced(bench::Frame *, unsigned);
void start(bench::Frame *, unsigned);
void child(bench::Frame *, unsigned);
void stop(bench::Frame *, unsigned);
void read_store(bench::Frame *, unsigned);
} // namespace cases
