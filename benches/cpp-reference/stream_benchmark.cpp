#include <algorithm>
#include <atomic>
#include <barrier>
#include <chrono>
#include <cstddef>
#include <cstdlib>
#include <cstring>
#include <iostream>
#include <memory>
#include <numeric>
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
using aligned_buffer = std::unique_ptr<double, decltype(&std::free)>;

aligned_buffer allocate_aligned(std::size_t count) {
    void* memory = nullptr;
    if (posix_memalign(&memory, 64, count * sizeof(double)) != 0) {
        throw std::bad_alloc();
    }
    return {static_cast<double*>(memory), &std::free};
}

bool pin_thread(unsigned cpu) {
#ifdef __linux__
    if (cpu >= CPU_SETSIZE) {
        return false;
    }
    cpu_set_t set;
    CPU_ZERO(&set);
    CPU_SET(cpu, &set);
    return pthread_setaffinity_np(pthread_self(), sizeof(set), &set) == 0;
#else
    (void)cpu;
    return true;
#endif
}

std::vector<unsigned> parse_cpu_list(const char* text) {
    std::vector<unsigned> cpus;
    if (std::strcmp(text, "-") == 0) {
        return cpus;
    }

    const std::string input(text);
    std::size_t begin = 0;
    while (begin < input.size()) {
        const auto comma = input.find(',', begin);
        cpus.push_back(
            static_cast<unsigned>(std::stoul(input.substr(begin, comma - begin))));
        if (comma == std::string::npos) {
            break;
        }
        begin = comma + 1;
    }
    return cpus;
}

template <class Function>
double parallel_run(std::size_t count, unsigned workers,
                    const std::vector<unsigned>& cpus, Function&& function) {
    std::barrier ready(static_cast<std::ptrdiff_t>(workers + 1));
    std::barrier start(static_cast<std::ptrdiff_t>(workers + 1));
    std::atomic<bool> affinity_failed = false;
    std::vector<std::thread> threads;
    threads.reserve(workers);

    for (unsigned worker = 0; worker < workers; ++worker) {
        threads.emplace_back([&, worker] {
            if (!cpus.empty()) {
                if (!pin_thread(cpus[worker])) {
                    affinity_failed.store(true, std::memory_order_relaxed);
                }
            }
            const auto begin = count * worker / workers;
            const auto end = count * (worker + 1) / workers;
            ready.arrive_and_wait();
            start.arrive_and_wait();
            if (affinity_failed.load(std::memory_order_relaxed)) {
                return;
            }
            function(begin, end);
        });
    }

    // Exclude thread construction, but include release synchronization and joining.
    ready.arrive_and_wait();
    const auto before = clock_type::now();
    start.arrive_and_wait();
    for (auto& thread : threads) {
        thread.join();
    }
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

int main(int argc, char** argv) {
    if (argc != 4) {
        std::cerr << "usage: stream_benchmark ELEMENTS WORKERS CPU_LIST_OR_-\n";
        return 2;
    }

    try {
        const auto count = static_cast<std::size_t>(std::stoull(argv[1]));
        const auto workers = static_cast<unsigned>(std::stoul(argv[2]));
        const auto cpus = parse_cpu_list(argv[3]);
        if (count == 0 || workers == 0 || (!cpus.empty() && cpus.size() != workers)) {
            throw std::invalid_argument("invalid element, worker, or CPU-list count");
        }
#ifndef __linux__
        if (!cpus.empty()) {
            throw std::invalid_argument("CPU affinity is supported only on Linux");
        }
#endif

        auto a_owner = allocate_aligned(count);
        auto b_owner = allocate_aligned(count);
        auto c_owner = allocate_aligned(count);
        auto* const a = a_owner.get();
        auto* const b = b_owner.get();
        auto* const c = c_owner.get();

        parallel_run(count, workers, cpus, [&](std::size_t begin, std::size_t end) {
            for (auto index = begin; index < end; ++index) {
                a[index] = 1.0;
                b[index] = 2.0;
                c[index] = 0.0;
            }
        });

        constexpr double scalar = 3.0;
        constexpr unsigned repetitions = 9;
        std::vector<double> copy_times;
        std::vector<double> scale_times;
        std::vector<double> add_times;
        std::vector<double> triad_times;
        copy_times.reserve(repetitions);
        scale_times.reserve(repetitions);
        add_times.reserve(repetitions);
        triad_times.reserve(repetitions);

        for (unsigned repetition = 0; repetition < repetitions; ++repetition) {
            copy_times.push_back(parallel_run(
                count, workers, cpus, [&](std::size_t begin, std::size_t end) {
                    for (auto index = begin; index < end; ++index) {
                        c[index] = a[index];
                    }
                }));
            scale_times.push_back(parallel_run(
                count, workers, cpus, [&](std::size_t begin, std::size_t end) {
                    for (auto index = begin; index < end; ++index) {
                        b[index] = scalar * c[index];
                    }
                }));
            add_times.push_back(parallel_run(
                count, workers, cpus, [&](std::size_t begin, std::size_t end) {
                    for (auto index = begin; index < end; ++index) {
                        c[index] = a[index] + b[index];
                    }
                }));
            triad_times.push_back(parallel_run(
                count, workers, cpus, [&](std::size_t begin, std::size_t end) {
                    for (auto index = begin; index < end; ++index) {
                        a[index] = b[index] + scalar * c[index];
                    }
                }));
        }

        const auto gbps = [count](double streams, double seconds) {
            return streams * static_cast<double>(count * sizeof(double)) / seconds /
                   1.0e9;
        };
        const double checksum = std::accumulate(a, a + count, 0.0);
        std::cout << "workers,copy_gbps,scale_gbps,add_gbps,triad_gbps,checksum\n"
                  << workers << ',' << gbps(2.0, median(copy_times)) << ','
                  << gbps(2.0, median(scale_times)) << ','
                  << gbps(3.0, median(add_times)) << ','
                  << gbps(3.0, median(triad_times)) << ',' << checksum << '\n';
    } catch (const std::exception& error) {
        std::cerr << "stream_benchmark: " << error.what() << '\n';
        return 1;
    }
}
