// Exact workload counterpart to benches/text_workflow/driver.rs.
#include "gd_table_column-buffer.h"
#include <algorithm>
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
    std::uint64_t id;
    std::string region, message, output;
    std::uint64_t score;
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

std::string Message(std::size_t row, std::size_t length) {
    std::ostringstream text;
    text << std::setfill('0') << std::setw(8) << row % 100'000'000
         << (row % 3 == 0 ? "-error-" : "-event-");
    auto result = text.str(); result.resize(length, 'x'); return result;
}

Table Fixture(std::size_t rows, unsigned length) {
    Table table(static_cast<unsigned>(rows));
    // GD's length-bearing buffers need room for a terminator as well as text.
    using Column = std::tuple<std::string_view, unsigned, std::string_view>;
    table.column_add(std::vector<Column>{{"uint64", 0, "id"}, {"string", 9, "region"},
        {"string", length + 1, "message"}, {"string", length + 4, "output"},
        {"uint64", 0, "score"}}, gd::table::tag_type_name{});
    const auto prepared = table.prepare();
    if (!prepared.first) throw std::runtime_error(prepared.second);
    for (std::size_t row = 0; row < rows; ++row) {
        const auto text = Message(row, length);
        table.row_add({static_cast<std::uint64_t>(row),
            std::string_view(row % 4 == 0 ? "north" : "south"),
            std::string_view(text), std::string_view(text), static_cast<std::uint64_t>(row % 100)});
    }
    return table;
}

std::uint64_t Number(const Table& table, std::size_t row, unsigned column) {
    std::uint64_t value;
    // memcpy works even when GD's AoS string slots leave this cell unaligned.
    std::memcpy(&value, table.cell_get(row, column), sizeof(value)); return value;
}

std::string_view Text(const Table& table, std::size_t row, unsigned column) {
    return table.cell_get_variant_view(row, column).as_string_view();
}

StringTable FixtureStrings(std::size_t rows, unsigned length) {
    StringTable table;
    table.reserve(rows);
    for (std::size_t row = 0; row < rows; ++row) {
        auto message = Message(row, length);
        table.push_back({row, row % 4 == 0 ? "north" : "south", message, message, row % 100});
    }
    return table;
}

std::uint64_t Number(const StringTable& table, std::size_t row, unsigned column) {
    return column == 0 ? table[row].id : table[row].score;
}

std::string_view Text(const StringTable& table, std::size_t row, unsigned column) {
    if (column == 1) return table[row].region;
    return column == 2 ? table[row].message : table[row].output;
}

template<class TableType>
std::vector<std::uint64_t> Selected(const TableType& table, std::size_t begin, std::size_t end) {
    std::vector<std::uint64_t> result;
    for (auto row = begin; row < end; ++row) {
        if (Text(table, row, 1) == "north" && Number(table, row, 4) >= 20
            && Text(table, row, 2).find("error") != std::string_view::npos) result.push_back(row);
    }
    return result;
}

void TransformRange(Table& table, std::size_t begin, std::size_t end, unsigned length) {
    std::string scratch; scratch.reserve(length + 3);
    for (auto row = begin; row < end; ++row) {
        scratch.assign(Text(table, row, 2));
        for (auto& ch : scratch) if (ch >= 'a' && ch <= 'z') ch -= 'a' - 'A';
        scratch += "|ok";
        table.cell_set(row, 3U, gd::variant_view(std::string_view(scratch)));
    }
}

void TransformRange(StringTable& table, std::size_t begin, std::size_t end, unsigned) {
    for (auto row = begin; row < end; ++row) {
        auto& output = table[row].output;
        output.assign(table[row].message);
        for (auto& ch : output) if (ch >= 'a' && ch <= 'z') ch -= 'a' - 'A';
        output += "|ok";
    }
}

template<class TableType>
void Transform(TableType& table, unsigned length, unsigned workers, Pool& pool) {
    pool.run([&](unsigned worker) { TransformRange(table, table.size() * worker / workers, table.size() * (worker + 1) / workers, length); });
}

template<class TableType>
std::vector<std::vector<std::uint64_t>> Filter(const TableType& table, unsigned workers, Pool& pool) {
    std::vector<std::vector<std::uint64_t>> parts(workers);
    pool.run([&](unsigned worker) { parts[worker] = Selected(table, table.size() * worker / workers, table.size() * (worker + 1) / workers); });
    return parts;
}

std::vector<Table> Pipeline(const Table& source, unsigned length, unsigned workers, Pool& pool) {
    std::vector<Table> parts(workers);
    pool.run([&](unsigned worker) {
        const auto rows = Selected(source, source.size() * worker / workers, source.size() * (worker + 1) / workers);
        Table target(source, gd::table::tag_columns{});
        target.row_add(rows.size());
        // This schema has inline fixed-capacity text and no references or sidecars.
        // Gather copies whole GD rows through its public row-buffer API.
        for (std::size_t row = 0; row < rows.size(); ++row)
            std::memcpy(target.row_get(row), source.row_get(rows[row]), source.size_row());
        TransformRange(target, 0, target.size(), length);
        parts[worker] = std::move(target);
    });
    return parts;
}

std::vector<StringTable> Pipeline(const StringTable& source, unsigned length, unsigned workers, Pool& pool) {
    std::vector<StringTable> parts(workers);
    pool.run([&](unsigned worker) {
        const auto rows = Selected(source, source.size() * worker / workers, source.size() * (worker + 1) / workers);
        auto& target = parts[worker];
        target.reserve(rows.size());
        for (auto row : rows) target.push_back(source[row]);
        TransformRange(target, 0, target.size(), length);
    });
    return parts;
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
std::pair<std::size_t, std::uint64_t> Digest(const std::vector<const TableType*>& tables) {
    std::uint64_t hash = 14'695'981'039'346'656'037ULL;
    std::size_t count = 0;
    for (const auto* table : tables) {
        for (std::size_t row = 0; row < table->size(); ++row) {
            ++count;
            for (unsigned column = 0; column < 5; ++column) {
                if (column == 0 || column == 4) hash = HashNumber(hash, Number(*table, row, column));
                else { const auto text = Text(*table, row, column); hash = Hash(HashNumber(hash, text.size()), text.data(), text.size()); }
            }
        }
    }
    return {count, hash};
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
    const std::string_view stage = argv[5];
    const auto samples = std::stoul(argv[6]);
    const auto sampleMs = std::stoul(argv[7]);
    if (!rows || rows > 100'000'000 || length < 16 || length > 4096 || !workers || workers > 64 || !samples || !sampleMs) throw std::runtime_error("invalid arguments");
    Pool pool(workers);
    const auto buildStart = Clock::now();
    auto table = [&] {
        if constexpr (std::is_same_v<TableType, Table>) return Fixture(rows, length);
        else return FixtureStrings(rows, length);
    }();
    const auto buildNs = std::chrono::duration_cast<std::chrono::nanoseconds>(Clock::now() - buildStart).count();
    std::pair<std::size_t, std::uint64_t> verification;
    if (stage == "filter") {
        auto parts = Filter(table, workers, pool);
        verification = {0, 14'695'981'039'346'656'037ULL};
        for (const auto& part : parts) for (auto id : part) { ++verification.first; verification.second = HashNumber(verification.second, id); }
    } else if (stage == "transform") {
        Transform(table, length, workers, pool); verification = Digest<TableType>({&table});
    } else if (stage == "pipeline") {
        const auto parts = Pipeline(table, length, workers, pool);
        std::vector<const TableType*> tables;
        for (const auto& part : parts) tables.push_back(&part);
        verification = Digest(tables);
    } else throw std::runtime_error("invalid operation");
    if (std::string_view(argv[8]) == "verify") {
        std::cout << "{\"count\":" << verification.first << ",\"digest\":\"" << verification.second << "\"}\n"; return 0;
    }
    const auto operation = [&] {
        if (stage == "filter") { const auto result = Filter(table, workers, pool); Consume(result); }
        else if (stage == "transform") { Transform(table, length, workers, pool); Consume(table); }
        else { const auto result = Pipeline(table, length, workers, pool); Consume(result); }
    };
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
    std::cout << "],\"iterations\":" << iterations << ",\"build_ns\":\"" << buildNs << "\",\"count\":" << verification.first << ",\"digest\":\"" << verification.second << "\"}\n";
    return 0;
}

int main(int argc, char** argv) {
    try {
        if (argc != 9) throw std::runtime_error("gd|std rows length workers operation samples sample_ms verify");
        if (std::string_view(argv[1]) == "gd") return Run<Table>(argv);
        if (std::string_view(argv[1]) == "std") return Run<StringTable>(argv);
        throw std::runtime_error("invalid representation");
    } catch (const std::exception& error) { std::cerr << error.what() << '\n'; return 1; }
}
