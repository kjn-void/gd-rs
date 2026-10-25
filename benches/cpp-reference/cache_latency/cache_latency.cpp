// Standalone C++20 experiment: intermittent L1 misses served by L2.
// Build on Linux (run both lines):
// clang-format off
// line_bytes=$(getconf LEVEL1_DCACHE_LINESIZE)
// g++ -std=c++20 -O3 -flto -DL1_CACHE_BYTES=$line_bytes cache_latency.cpp perf_control.cpp -o cache-latency
// clang-format on
//
// Run: taskset -c 3 ./cache-latency work1 80 0.5
//
// Arguments: kernel, cold-ring KiB, seconds, optional permutation seed.

#include <algorithm>
#include <array>
#include <atomic>
#include <bit>
#include <chrono>
#include <cstddef>
#include <cstdint>
#include <iomanip>
#include <iostream>
#include <numeric>
#include <random>
#include <stdexcept>
#include <string>
#include <vector>

#include "perf_control.h"

// These Linux kernel macros are not exported in userspace headers. Supply the
// detected target line size at build time; alignas requires a constant
// expression.
#if defined(L1_CACHE_BYTES)
constexpr std::size_t CACHE_LINE_SIZE = L1_CACHE_BYTES;
#elif defined(L1_CACHE_SHIFT)
constexpr std::size_t CACHE_LINE_SIZE = std::size_t{1} << L1_CACHE_SHIFT;
#else
#error \
    "Define L1_CACHE_BYTES or L1_CACHE_SHIFT from the target's Linux cache information"
#endif

static_assert(std::has_single_bit(CACHE_LINE_SIZE));
static_assert(CACHE_LINE_SIZE > sizeof(std::uint64_t));
static_assert(CACHE_LINE_SIZE <= 1024);

#if defined(L1_CACHE_BYTES) && defined(L1_CACHE_SHIFT)
static_assert(CACHE_LINE_SIZE == (std::size_t{1} << L1_CACHE_SHIFT));
#endif

struct alignas(CACHE_LINE_SIZE) Node {
  std::uint64_t next = 0;
  std::array<std::byte, CACHE_LINE_SIZE - sizeof(std::uint64_t)> padding{};
};

static_assert(sizeof(Node) == CACHE_LINE_SIZE);

struct State {
  std::uint64_t a = 0x123456789abcdef0ULL;
  std::uint64_t b = 0xfedcba9876543210ULL;
  std::uint64_t c = 0x3141592653589793ULL;
  std::uint64_t d = 0x2718281828459045ULL;
  std::uint64_t cold_index = 0;
  std::uint64_t hot_index = 0;
  std::uint64_t loaded_sum = 0;

  bool operator==(const State&) const = default;
};

// One load in four visits the large ring; the other three visit a 1 KiB ring.
// Shuffled links defeat stride prefetching. The cold chain cannot issue its
// next load until the previous cold load returns, but the hot chain and
// arithmetic can proceed independently. There is no inline assembly, software
// prefetch, explicit SIMD, or hardware fence in this computation.
template <unsigned kRounds>
[[gnu::noinline, gnu::noclone]]
State RunMixed(const Node* cold, const Node* hot, std::uint64_t iterations) {
  const State initial;

  auto a = initial.a;
  auto b = initial.b;
  auto c = initial.c;
  auto d = initial.d;

  std::uint64_t cold_index = 0;
  std::uint64_t hot_index = 0;
  std::uint64_t loaded_sum = 0;

  for (std::uint64_t i = 0; i < iterations; ++i) {
    std::uint64_t next;

    if ((i & 3) == 0) {
      cold_index = cold[cold_index].next;
      next = cold_index;
    } else {
      hot_index = hot[hot_index].next;
      next = hot_index;
    }

    loaded_sum += next;

    // Four independent integer recurrences, unrelated to the loaded value.
    for (unsigned round = 0; round < kRounds; ++round) {
      a = std::rotl(a, 13) + 131;
      b = std::rotl(b, 17) + 313;
      c = std::rotl(c, 29) + 571;
      d = std::rotl(d, 37) + 877;
    }
  }

  return {a, b, c, d, cold_index, hot_index, loaded_sum};
}

// Latency calibration: every load depends on the previous load. There is no
// independent work to cover the cold ring's load-to-use latency.
[[gnu::noinline, gnu::noclone]]
State RunSerial(const Node* cold, const Node* /*hot*/,
                std::uint64_t iterations) {
  std::uint64_t index = 0;
  std::uint64_t loaded_sum = 0;

  for (std::uint64_t i = 0; i < iterations; ++i) {
    index = cold[index].next;
    loaded_sum += index;
  }

  State result;
  result.cold_index = index;
  result.loaded_sum = loaded_sum;

  return result;
}

using Kernel = State (*)(const Node*, const Node*, std::uint64_t);

struct Entry {
  const char* name;
  Kernel run;
  unsigned rounds;
  bool serial;
};

// The mixed kernels repeat cold -> hot -> hot -> hot, adding each loaded index
// to the checksum. The number in the name selects arithmetic rounds per
// iteration. Each round updates four independent uint64_t states (a, b, c, d),
// with one rotation and one addition per state. These updates do not depend on
// the loaded index, so OoO execution can overlap them with a pending load.
//
// work0: Ring traversal and checksum only; no independent arithmetic.
// work1: 1 round per iteration: 4 rotations and 4 additions.
// work2: 2 rounds per iteration: 8 rotations and 8 additions.
// work4: 4 rounds per iteration: 16 rotations and 16 additions.
//
// serial: Follow only the cold ring, once per iteration, and update the
// checksum. Every next address depends on the previous load. There are no
// hot-ring steps or independent arithmetic rounds, exposing the chain's
// load-to-use latency.
constexpr Entry kEntries[] = {
    {"work0", RunMixed<0>, 0, false}, {"work1", RunMixed<1>, 1, false},
    {"work2", RunMixed<2>, 2, false}, {"work4", RunMixed<4>, 4, false},
    {"serial", RunSerial, 0, true},
};

std::vector<Node> MakeRing(std::size_t node_count, std::uint64_t seed) {
  std::vector<Node> nodes(node_count);
  std::vector<std::size_t> order(node_count);

  std::iota(order.begin(), order.end(), 0);

  std::mt19937_64 random(seed);
  std::shuffle(order.begin(), order.end(), random);

  for (std::size_t i = 0; i < node_count; ++i) {
    nodes[order[i]].next = order[(i + 1) % node_count];
  }

  // Verify one cycle containing every cache line, and touch all lines.
  std::vector<bool> seen(node_count);
  std::size_t index = 0;

  for (std::size_t i = 0; i < node_count; ++i) {
    if (index >= node_count || seen[index]) {
      throw std::runtime_error("Bad ring");
    }

    seen[index] = true;
    index = nodes[index].next;
  }

  if (index != 0) {
    throw std::runtime_error("Ring does not close");
  }

  return nodes;
}

State Reference(const Entry& entry, const Node* cold, const Node* hot,
                std::uint64_t iterations) {
  State state;

  for (std::uint64_t i = 0; i < iterations; ++i) {
    if (entry.serial || (i & 3) == 0) {
      state.cold_index = cold[state.cold_index].next;
      state.loaded_sum += state.cold_index;
    } else {
      state.hot_index = hot[state.hot_index].next;
      state.loaded_sum += state.hot_index;
    }

    for (unsigned round = 0; round < entry.rounds; ++round) {
      state.a = std::rotl(state.a, 13) + 131;
      state.b = std::rotl(state.b, 17) + 313;
      state.c = std::rotl(state.c, 29) + 571;
      state.d = std::rotl(state.d, 37) + 877;
    }
  }

  return state;
}

void Verify() {
  for (auto kib : {8, 72, 80, 96}) {
    for (auto seed : {70274, 982451653}) {
      const auto cold = MakeRing(kib * 1024 / sizeof(Node), seed);
      const auto hot = MakeRing(1024 / sizeof(Node), 12345);

      for (const auto& entry : kEntries) {
        for (auto iterations : {0, 1, 2, 3, 4, 17, 65537}) {
          const auto actual = entry.run(cold.data(), hot.data(), iterations);
          const auto expected =
              Reference(entry, cold.data(), hot.data(), iterations);

          if (!(actual == expected)) {
            throw std::runtime_error("Verification failed");
          }
        }
      }
    }
  }
}

std::uint64_t Digest(const State& state) {
  return state.a ^ state.b ^ state.c ^ state.d ^ state.cold_index ^
         state.hot_index ^ state.loaded_sum;
}

volatile std::uint64_t g_observed;

int main(int argc, char** argv) {
  try {
    if (argc == 2 && std::string(argv[1]) == "--verify") {
      Verify();

      std::cout << "All kernels, sizes, and seeds verified\n";

      return 0;
    }

    if (argc != 4 && argc != 5) {
      std::cerr << "Usage: " << argv[0]
                << " work0|work1|work2|work4|serial cold_KiB seconds [seed]\n";

      return 2;
    }

    const std::string name = argv[1];
    const Entry* entry = nullptr;

    for (const auto& candidate : kEntries) {
      if (name == candidate.name) {
        entry = &candidate;
      }
    }

    const auto kib = std::stoull(argv[2]);
    const auto duration = std::stod(argv[3]);
    const auto seed = argc == 5 ? std::stoull(argv[4]) : 70274ULL;

    if (entry == nullptr || kib < 1 || kib > 1024 || !(duration > 0)) {
      throw std::runtime_error("Invalid arguments");
    }

    const auto cold = MakeRing(kib * 1024 / sizeof(Node), seed);
    const auto hot = MakeRing(1024 / sizeof(Node), 12345);

    constexpr std::uint64_t kBlockIterations = 65536;
    const auto expected =
        Reference(*entry, cold.data(), hot.data(), kBlockIterations);

    auto seconds = [](auto start) {
      return std::chrono::duration<double>(std::chrono::steady_clock::now() -
                                           start)
          .count();
    };

    State last;
    const auto warm = std::chrono::steady_clock::now();

    do {
      // Compiler-only barrier between whole calls; no CPU fence is emitted.
      std::atomic_signal_fence(std::memory_order_acq_rel);
      last = entry->run(cold.data(), hot.data(), kBlockIterations);
      g_observed = Digest(last);
    } while (seconds(warm) < 0.10);

    PerfControl perf;
    perf.Start();

    std::uint64_t calls = 0;
    std::uint64_t checksum = 0;
    const auto start = std::chrono::steady_clock::now();

    do {
      std::atomic_signal_fence(std::memory_order_acq_rel);
      last = entry->run(cold.data(), hot.data(), kBlockIterations);
      checksum += Digest(last);
      ++calls;
    } while (seconds(start) < duration);

    const auto elapsed = seconds(start);
    g_observed = checksum;

    perf.Stop();

    if (!(last == expected) || checksum != Digest(expected) * calls) {
      throw std::runtime_error("Wrong result");
    }

    const auto iterations = calls * kBlockIterations;

    std::cout << std::setprecision(12) << "{\"kernel\":\"" << name
              << "\",\"cold_kib\":" << kib
              << ",\"hot_kib\":1,\"cache_line_size\":" << CACHE_LINE_SIZE
              << ",\"seed\":" << seed << ",\"calls\":" << calls
              << ",\"iterations\":" << iterations << ",\"elapsed\":" << elapsed
              << ",\"ns_per_iteration\":" << elapsed * 1e9 / iterations
              << ",\"checksum\":" << checksum << "}\n";

    return 0;
  } catch (const std::exception& error) {
    std::cerr << error.what() << '\n';

    return 1;
  }
}
