// Exact counterpart to benches/filter_copy/driver.rs: one source, one target.
#include "gd_table_column-buffer.h"
#include <chrono>
#include <condition_variable>
#include <cstring>
#include <functional>
#include <iomanip>
#include <iostream>
#include <mutex>
#include <sstream>
#include <stdexcept>
#include <thread>
#include <tuple>
#include <type_traits>
#include <vector>

using Table = gd::table::table_column_buffer;
using Clock = std::chrono::steady_clock;
struct StringRow {
    std::uint64_t id, selector, amount;
    std::string name, message;
    StringRow() {} // String members are constructed; numbers are written before use.
    StringRow(std::uint64_t i, std::uint64_t s, std::uint64_t a,
              std::string n, std::string m)
        : id(i), selector(s), amount(a), name(std::move(n)), message(std::move(m)) {}
};
using StringTable = std::vector<StringRow>;

// Persistent workers execute exactly one disjoint row range per task.
class Pool {
    std::mutex mutex;
    std::condition_variable wake, done;
    std::vector<std::thread> threads;
    std::function<void(unsigned)> work;
    unsigned generation = 0, active = 0;
    bool stop = false;
    std::exception_ptr failure;
public:
    explicit Pool(unsigned count) {
        if (count == 1) return;
        for (unsigned i = 0; i < count; ++i) {
            threads.emplace_back([this, i] {
                unsigned seen = 0;
                std::unique_lock lock(mutex);
                for (;;) {
                    wake.wait(lock, [&] { return stop || generation != seen; });
                    if (stop) return;
                    seen = generation;
                    lock.unlock();
                    try { work(i); }
                    catch (...) { std::lock_guard guard(mutex); failure = std::current_exception(); }
                    lock.lock();
                    if (--active == 0) done.notify_one();
                }
            });
        }
    }
    ~Pool() {
        { std::lock_guard guard(mutex); stop = true; }
        wake.notify_all();
        for (auto& thread : threads) thread.join();
    }
    void run(std::function<void(unsigned)> operation) {
        if (threads.empty()) { operation(0); return; }
        std::unique_lock lock(mutex);
        work = std::move(operation); active = threads.size(); failure = nullptr;
        ++generation; wake.notify_all();
        done.wait(lock, [&] { return active == 0; });
        if (failure) std::rethrow_exception(failure);
    }
};


std::uint64_t Selector(std::size_t row) {
    return (row % 100 * 37 + row / 100 * 17) % 100;
}
std::string TextFixture(std::size_t row, unsigned length, std::string_view label) {
    std::ostringstream text;
    text << std::setfill('0') << std::setw(8) << row % 100'000'000 << '-' << label << '-';
    auto result = text.str(); result.resize(length, 'x'); return result;
}
Table Fixture(std::size_t rows, unsigned length) {
    Table table(static_cast<unsigned>(rows));
    using Column = std::tuple<std::string_view, unsigned, std::string_view>;
    // No references, NULL metadata, or row-status sidecars: the complete record,
    // including string lengths and terminators, lives inside this row buffer.
    table.column_add(std::vector<Column>{{"uint64", 0, "id"},
        {"uint64", 0, "selector"}, {"uint64", 0, "amount"},
        {"string", length + 1, "name"}, {"string", length + 1, "message"}},
        gd::table::tag_type_name{});
    const auto prepared = table.prepare();
    if (!prepared.first) throw std::runtime_error(prepared.second);
    for (std::size_t row = 0; row < rows; ++row) {
        const auto name = TextFixture(row, length, "name");
        const auto message = TextFixture(row, length, "text");
        table.row_add({static_cast<std::uint64_t>(row), Selector(row),
            static_cast<std::uint64_t>(row * 13 + 7),
            std::string_view(name), std::string_view(message)});
    }
    return table;
}
StringTable FixtureStrings(std::size_t rows, unsigned length) {
    StringTable table; table.reserve(rows);
    for (std::size_t row = 0; row < rows; ++row)
        table.emplace_back(row, Selector(row), row * 13 + 7,
            TextFixture(row, length, "name"), TextFixture(row, length, "text"));
    return table;
}
std::uint64_t Number(const Table& table, std::size_t row, unsigned column) {
    std::uint64_t result;
    std::memcpy(&result, table.cell_get(row, column), sizeof(result)); return result;
}
std::uint64_t Number(const StringTable& table, std::size_t row, unsigned column) {
    if (column == 0) return table[row].id;
    return column == 1 ? table[row].selector : table[row].amount;
}
std::string_view Text(const Table& table, std::size_t row, unsigned column) {
    return table.cell_get_variant_view(row, column).as_string_view();
}
std::string_view Text(const StringTable& table, std::size_t row, unsigned column) {
    return column == 3 ? table[row].name : table[row].message;
}

template<class TableType>
TableType FilterCopy(const TableType& source, unsigned percent, unsigned workers, Pool& pool) {
    // Temporary row indices are not target tables. Both dispatches join before
    // their successors; table metadata changes only on the coordinating thread.
    std::vector<std::vector<std::size_t>> parts(workers);
    pool.run([&](unsigned worker) {
        const auto begin = source.size() * worker / workers;
        const auto end = source.size() * (worker + 1) / workers;
        auto& selected = parts[worker]; selected.reserve(end - begin);
        for (auto row = begin; row < end; ++row)
            if (Number(source, row, 1) < percent) selected.push_back(row);
    });
    std::vector<std::size_t> offsets(workers + 1, 0);
    for (unsigned worker = 0; worker < workers; ++worker)
        offsets[worker + 1] = offsets[worker] + parts[worker].size();
    if constexpr (std::is_same_v<TableType, Table>) {
        Table target(source, gd::table::tag_columns{});
        // Exact capacity avoids GD's default 50% growth allowance.
        target.row_reserve_add(offsets.back()); target.row_add(offsets.back());
        pool.run([&](unsigned worker) {
            for (std::size_t i = 0; i < parts[worker].size(); ++i)
                std::memcpy(target.row_get(offsets[worker] + i),
                    source.row_get(parts[worker][i]), source.size_row());
        });
        return target;
    } else {
        StringTable target;
        if (workers == 1) {
            target.reserve(offsets.back());
            for (auto row : parts[0]) target.push_back(source[row]);
        } else {
            // Standard vector/string lifetimes are established before dispatch.
            // Workers assign disjoint elements; no vector growth or row mutex.
            target.resize(offsets.back());
            pool.run([&](unsigned worker) {
                for (std::size_t i = 0; i < parts[worker].size(); ++i)
                    target[offsets[worker] + i] = source[parts[worker][i]];
            });
        }
        return target;
    }
}

std::uint64_t Hash(std::uint64_t hash, const void* data, std::size_t length) {
    const auto* bytes = static_cast<const unsigned char*>(data);
    for (std::size_t i = 0; i < length; ++i) hash = (hash ^ bytes[i]) * 1'099'511'628'211ULL;
    return hash;
}

std::uint64_t HashNumber(std::uint64_t hash, std::uint64_t value) {
    unsigned char bytes[8];
    for (unsigned i = 0; i < 8; ++i) bytes[i] = value >> (8 * i);
    return Hash(hash, bytes, 8);
}


template<class TableType>
std::pair<std::size_t, std::uint64_t> Digest(const TableType& table) {
    std::uint64_t hash = 14'695'981'039'346'656'037ULL;
    for (std::size_t row = 0; row < table.size(); ++row) {
        for (unsigned column = 0; column < 5; ++column) {
            if (column < 3) hash = HashNumber(hash, Number(table, row, column));
            else {
                const auto text = Text(table, row, column);
                hash = Hash(HashNumber(hash, text.size()), text.data(), text.size());
            }
        }
    }
    return {table.size(), hash};
}

// Prevent the benchmark result from disappearing under LTO.
template<class T> void Consume(const T& value) {
#if defined(__GNUC__) || defined(__clang__)
    asm volatile("" : : "g"(&value) : "memory");
#else
    static volatile const void* sink; sink = &value;
#endif
}


template<class TableType>
int Run(char** argv) {
    const auto rows = std::stoull(argv[2]);
    const auto length = static_cast<unsigned>(std::stoul(argv[3]));
    const auto workers = static_cast<unsigned>(std::stoul(argv[4]));
    const auto percent = static_cast<unsigned>(std::stoul(argv[5]));
    const auto samples = std::stoul(argv[6]);
    const auto sampleMs = std::stoul(argv[7]);
    if (rows > 100'000'000 || length < 16 || length > 4096 || !workers || workers > 64
        || percent > 100 || !samples || !sampleMs) throw std::runtime_error("invalid arguments");
    Pool pool(workers);
    const auto buildStart = Clock::now();
    auto source = [&] {
        if constexpr (std::is_same_v<TableType, Table>) return Fixture(rows, length);
        else return FixtureStrings(rows, length);
    }();
    const auto buildNs = std::chrono::duration_cast<std::chrono::nanoseconds>(Clock::now() - buildStart).count();
    std::pair<std::size_t, std::uint64_t> verification;
    {
        const auto target = FilterCopy(source, percent, workers, pool);
        verification = Digest(target);
        if (std::string_view(argv[8]) == "verify") {
            source = TableType{};
            if (Digest(target) != verification) throw std::runtime_error("target aliases source");
            std::cout << "{\"count\":" << verification.first << ",\"digest\":\""
                << verification.second << "\",\"independent_target\":true}\n";
            return 0;
        }
    }
    if (std::string_view(argv[8]) != "time") throw std::runtime_error("invalid action");
    const auto operation = [&] { const auto target = FilterCopy(source, percent, workers, pool); Consume(target); };
    std::uint64_t iterations = 1;
    for (;;) {
        const auto start = Clock::now();
        for (std::uint64_t i = 0; i < iterations; ++i) operation();
        if (Clock::now() - start >= std::chrono::milliseconds(sampleMs)) break;
        iterations *= 2;
    }
    std::cout << std::setprecision(12) << "{\"samples_ns\":[";
    for (unsigned sample = 0; sample < samples; ++sample) {
        const auto start = Clock::now();
        for (std::uint64_t i = 0; i < iterations; ++i) operation();
        const auto ns = std::chrono::duration<double, std::nano>(Clock::now() - start).count() / iterations;
        if (sample) std::cout << ',';
        std::cout << ns;
    }
    std::cout << "],\"iterations\":" << iterations << ",\"build_ns\":\"" << buildNs
        << "\",\"count\":" << verification.first << ",\"digest\":\"" << verification.second << "\"}\n";
    return 0;
}
int main(int argc, char** argv) {
    try {
        if (argc != 9) throw std::runtime_error("gd|std rows length workers percent samples sample_ms verify|time");
        if (std::string_view(argv[1]) == "gd") return Run<Table>(argv);
        if (std::string_view(argv[1]) == "std") return Run<StringTable>(argv);
        throw std::runtime_error("invalid layout");
    } catch (const std::exception& error) { std::cerr << error.what() << '\n'; return 1; }
}
