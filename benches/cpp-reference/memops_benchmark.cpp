#include <algorithm>
#include <atomic>
#include <barrier>
#include <chrono>
#include <cstddef>
#include <cstdlib>
#include <cstring>
#include <iostream>
#include <memory>
#include <stdexcept>
#include <string>
#include <thread>
#include <vector>

#ifdef __linux__
#include <pthread.h>
#include <sched.h>
#endif

namespace {
using clock_type = std::chrono::steady_clock;
using buffer = std::unique_ptr<unsigned char, decltype(&std::free)>;

buffer allocate(std::size_t bytes) {
  void *memory = nullptr;
  if (posix_memalign(&memory, 64, bytes) != 0)
    throw std::bad_alloc();
  return {static_cast<unsigned char *>(memory), &std::free};
}

bool pin_thread(unsigned cpu) {
#ifdef __linux__
  if (cpu >= CPU_SETSIZE)
    return false;
  cpu_set_t set;
  CPU_ZERO(&set);
  CPU_SET(cpu, &set);
  return pthread_setaffinity_np(pthread_self(), sizeof(set), &set) == 0;
#else
  (void)cpu;
  return true;
#endif
}

std::vector<unsigned> parse_cpus(const char *text) {
  std::vector<unsigned> cpus;
  if (std::strcmp(text, "-") == 0)
    return cpus;
  const std::string input(text);
  std::size_t begin = 0;
  while (begin < input.size()) {
    const auto comma = input.find(',', begin);
    cpus.push_back(
        static_cast<unsigned>(std::stoul(input.substr(begin, comma - begin))));
    if (comma == std::string::npos)
      break;
    begin = comma + 1;
  }
  return cpus;
}

template <class Function>
double parallel_run(std::size_t bytes, unsigned workers,
                    const std::vector<unsigned> &cpus, Function &&function) {
  std::barrier ready(static_cast<std::ptrdiff_t>(workers + 1));
  std::barrier start(static_cast<std::ptrdiff_t>(workers + 1));
  std::atomic<bool> affinity_failed = false;
  std::vector<std::thread> threads;
  threads.reserve(workers);
  for (unsigned worker = 0; worker < workers; ++worker) {
    threads.emplace_back([&, worker] {
      if (!cpus.empty() && !pin_thread(cpus[worker])) {
        affinity_failed.store(true, std::memory_order_relaxed);
      }
      const auto begin = bytes * worker / workers;
      const auto end = bytes * (worker + 1) / workers;
      ready.arrive_and_wait();
      start.arrive_and_wait();
      if (!affinity_failed.load(std::memory_order_relaxed))
        function(begin, end);
    });
  }
  ready.arrive_and_wait();
  const auto before = clock_type::now();
  start.arrive_and_wait();
  for (auto &thread : threads)
    thread.join();
  if (affinity_failed.load(std::memory_order_relaxed)) {
    throw std::runtime_error("pthread_setaffinity_np failed");
  }
  return std::chrono::duration<double>(clock_type::now() - before).count();
}

double median(std::vector<double> values) {
  std::sort(values.begin(), values.end());
  return values[values.size() / 2];
}
} // namespace

int main(int argc, char **argv) {
  if (argc != 4) {
    std::cerr << "usage: memops_benchmark BYTES WORKERS CPU_LIST_OR_-\n";
    return 2;
  }
  try {
    const auto bytes = static_cast<std::size_t>(std::stoull(argv[1]));
    const auto workers = static_cast<unsigned>(std::stoul(argv[2]));
    const auto cpus = parse_cpus(argv[3]);
    if (bytes == 0 || workers == 0 ||
        (!cpus.empty() && cpus.size() != workers)) {
      throw std::invalid_argument("invalid byte, worker, or CPU-list count");
    }
#ifndef __linux__
    if (!cpus.empty())
      throw std::invalid_argument("affinity requires Linux");
#endif
    auto source_owner = allocate(bytes);
    auto destination_owner = allocate(bytes);
    auto *const source = source_owner.get();
    auto *const destination = destination_owner.get();

    parallel_run(bytes, workers, cpus, [&](std::size_t begin, std::size_t end) {
      for (auto index = begin; index < end; ++index) {
        source[index] = static_cast<unsigned char>(index * 131U + 17U);
        destination[index] = 0;
      }
    });

    constexpr unsigned repetitions = 9;
    std::vector<double> memcpy_times;
    std::vector<double> memset_times;
    memcpy_times.reserve(repetitions);
    memset_times.reserve(repetitions);
    for (unsigned repetition = 0; repetition < repetitions; ++repetition) {
      memcpy_times.push_back(parallel_run(
          bytes, workers, cpus, [&](std::size_t begin, std::size_t end) {
            std::memcpy(destination + begin, source + begin, end - begin);
          }));
      memset_times.push_back(parallel_run(
          bytes, workers, cpus, [&](std::size_t begin, std::size_t end) {
            std::memset(destination + begin, 0x5a, end - begin);
          }));
    }

    // Leave an observed copy result after the measured passes.
    parallel_run(bytes, workers, cpus, [&](std::size_t begin, std::size_t end) {
      std::memcpy(destination + begin, source + begin, end - begin);
    });
    std::size_t checksum = 0;
    for (std::size_t index = 0; index < bytes; index += 4096) {
      checksum += destination[index];
    }

    const auto memcpy_seconds = median(memcpy_times);
    const auto memset_seconds = median(memset_times);
    const auto payload = static_cast<double>(bytes) / 1.0e9;
    std::cout
        << "workers,memcpy_payload_gbps,memcpy_read_write_gbps,memset_gbps,"
           "checksum\n"
        << workers << ',' << payload / memcpy_seconds << ','
        << 2.0 * payload / memcpy_seconds << ',' << payload / memset_seconds
        << ',' << checksum << '\n';
  } catch (const std::exception &error) {
    std::cerr << "memops_benchmark: " << error.what() << '\n';
    return 1;
  }
}
