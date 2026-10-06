#pragma once
// Parallel "variants" stage: eight parameter sets become eight independent
// tasks, each scanning the same read-only clean table. Import and preparation
// stay sequential. The driver constructs this pool outside the measured loop.
#include <atomic>
#include <condition_variable>
#include <exception>
#include <mutex>
#include <thread>
#include "workload.hpp"

class BatchPool {
    std::mutex mutex;
    std::condition_variable wake, finished;
    std::vector<std::thread> threads;
    bool stop = false;
    unsigned generation = 0, active = 0;
    std::atomic<std::size_t> next{0};
    const workflow::Table* source = nullptr;
    const std::vector<workflow::Parameters>* parameters = nullptr;
    std::vector<workflow::Table>* outputs = nullptr;
    std::exception_ptr failure;

public:
    // Reuse these workers across the warmup and every measured iteration.
    explicit BatchPool(unsigned count) {
        for (unsigned i = 0; i < count; ++i) {
            threads.emplace_back([this] {
                unsigned seen = 0;
                std::unique_lock lock(mutex);
                for (;;) {
                    wake.wait(lock, [&] {
                        return stop || generation != seen;
                    });
                    if (stop) {
                        return;
                    }
                    seen = generation;
                    lock.unlock();
                    try {
                        // Claim a parameter set, not a range of source rows.
                        // Each task writes only its own slot in the result vector.
                        for (auto i = next.fetch_add(1); i < parameters->size();
                            i = next.fetch_add(1)) {
                            (*outputs)[i] = workflow::Variant(*source, (*parameters)[i]);
                        }
                    } catch (...) {
                        std::lock_guard guard(mutex);
                        failure = std::current_exception();
                    }
                    lock.lock();
                    if (--active == 0) {
                        finished.notify_one();
                    }
                }
            });
        }
    }

    ~BatchPool() {
        {
            std::lock_guard lock(mutex);
            stop = true;
        }
        wake.notify_all();
        for (auto& thread : threads) {
            thread.join();
        }
    }

    std::vector<workflow::Table> Run(
        const workflow::Table& clean, const std::vector<workflow::Parameters>& p) {
        // "variants" measures allocation, dispatch, all eight Variant calls,
        // and this wait. Retain every output until the whole batch has finished.
        std::vector<workflow::Table> results(p.size());
        std::unique_lock lock(mutex);
        source = &clean;
        parameters = &p;
        outputs = &results;
        next = 0;
        active = threads.size();
        failure = nullptr;
        ++generation;
        wake.notify_all();
        finished.wait(lock, [&] {
            return active == 0;
        });
        if (failure) {
            std::rethrow_exception(failure);
        }

        return results;
    }
};
