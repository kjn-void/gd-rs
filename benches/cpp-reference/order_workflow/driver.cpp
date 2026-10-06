// Benchmark harness: setup and SQL verification are outside timing. Standalone
// import/prepare/variants stages isolate work; complete times the full pipeline.
#include <chrono>
#include <iomanip>
#include <iostream>
#include <memory>
#include "pool.hpp"

using namespace workflow;

namespace {
using Statement = std::unique_ptr<sqlite3_stmt, decltype(&sqlite3_finalize)>;

Statement Query(Database& db, const std::string& sql) {
    sqlite3_stmt* raw = nullptr;
    if (sqlite3_prepare_v2(db.get_sqlite3(), sql.c_str(), -1, &raw, nullptr) != SQLITE_OK) {
        throw std::runtime_error(sqlite3_errmsg(db.get_sqlite3()));
    }

    return Statement(raw, sqlite3_finalize);
}

void Require(bool ok, const std::string& message) {
    if (!ok) {
        throw std::runtime_error(message);
    }
}

// Load and validate the eight variant requests once, before any stage timer.
std::vector<Parameters> ReadParameters(Database& db) {
    auto query = Query(
        db, "SELECT region,from_day,to_day,status,minimum,discount_bp FROM parameters ORDER BY id");
    std::vector<Parameters> result;
    int status;
    while ((status = sqlite3_step(query.get())) == SQLITE_ROW) {
        Parameters p{};
        for (int i = 0; i < 6; ++i) {
            p[i] = sqlite3_column_int64(query.get(), i);
        }
        Require(p[0] >= -1 && p[1] <= p[2] && p[3] >= -1 && p[4] >= 0 && p[5] >= 0 && p[5] <= 10000,
            "invalid variant parameters");
        result.push_back(p);
    }
    Require(status == SQLITE_DONE && result.size() == 8, "invalid parameters");

    return result;
}

// Correctness oracle, outside timing: compare each output cell, NULL, and row
// position against fixture.py's independent SQL views, including the row count.
void VerifyTable(Database& db, const Table& table, const std::string& sql) {
    auto query = Query(db, sql);
    Require(sqlite3_column_count(query.get()) == static_cast<int>(table.get_column_count()),
        "wrong output width");
    for (std::uint64_t row = 0; row < table.get_row_count(); ++row) {
        Require(sqlite3_step(query.get()) == SQLITE_ROW, "extra output row: " + sql);
        for (unsigned c = 0; c < table.get_column_count(); ++c) {
            const auto actual = table.cell_get_variant_view(row, c);
            const auto type = sqlite3_column_type(query.get(), c);
            bool equal = type == SQLITE_NULL ? actual.is_null() : !actual.is_null();
            if (equal && type == SQLITE_INTEGER) {
                equal = actual.as_int64() == sqlite3_column_int64(query.get(), c);
            } else if (equal && type == SQLITE_TEXT) {
                const auto* text =
                    reinterpret_cast<const char*>(sqlite3_column_text(query.get(), c));
                const std::string_view expected(text, sqlite3_column_bytes(query.get(), c));
                equal = actual.as_string_view() == expected;
            } else if (type != SQLITE_NULL && type != SQLITE_INTEGER) {
                Require(type == SQLITE_TEXT, "unexpected SQL type");
            }
            Require(equal, "oracle mismatch row " + std::to_string(row) + " column " +
                               std::to_string(c) + ": " + sql);
        }
    }
    Require(sqlite3_step(query.get()) == SQLITE_DONE, "missing output rows: " + sql);
}

// Report deterministic output fingerprints so the runner can compare all three
// implementations. Timed stages do not calculate these verification digests.
std::uint64_t Digest(const Table& table) {
    std::uint64_t hash = 0xcbf29ce484222325ULL;
    const auto byte = [&](std::uint8_t v) {
        hash = (hash ^ v) * 0x100000001b3ULL;
    };
    const auto integer = [&](std::uint64_t v) {
        for (unsigned i = 0; i < 8; ++i) {
            byte(static_cast<std::uint8_t>(v >> (8 * i)));
        }
    };
    integer(table.get_row_count());
    integer(table.get_column_count());
    for (std::uint64_t row = 0; row < table.get_row_count(); ++row) {
        for (unsigned c = 0; c < table.get_column_count(); ++c) {
            const auto value = table.cell_get_variant_view(row, c);
            if (value.is_null()) {
                byte(0);
            } else if (value.is_string()) {
                byte(2);
                const auto text = value.as_string_view();
                integer(text.size());
                for (unsigned char ch : text) {
                    byte(ch);
                }
            } else {
                byte(1);
                integer(static_cast<std::uint64_t>(value.as_int64()));
            }
        }
    }

    return hash;
}

std::string Names(const std::vector<std::string_view>& names) {
    std::string text;
    for (auto name : names) {
        if (!text.empty()) {
            text += ',';
        }
        text += name;
    }

    return text;
}

template <class T> void PrintArray(const std::vector<T>& values) {
    std::cout << '[';
    for (std::size_t i = 0; i < values.size(); ++i) {
        if (i) {
            std::cout << ',';
        }
        std::cout << values[i];
    }
    std::cout << ']';
}

void Verify(Database& db, const std::vector<Parameters>& p, BatchPool& pool, unsigned workers) {
    // Destroy source tables before verifying materialized output, exercising string ownership.
    auto prepared = std::optional<Prepared>([&] {
        const auto input = Load(db);

        return Prepare(input);
    }());
    const auto names = Names(auditNames);
    VerifyTable(
        db, prepared->audit, "SELECT " + names + " FROM expected_audit ORDER BY source_pos");
    VerifyTable(
        db, prepared->clean, "SELECT " + names + " FROM expected_clean ORDER BY source_pos");
    auto outputs = pool.Run(prepared->clean, p);
    const auto cleanHash = Digest(prepared->clean);
    std::vector<std::uint64_t> hashes, counts;
    for (std::size_t i = 0; i < outputs.size(); ++i) {
        VerifyTable(db, outputs[i],
            "SELECT line_id,name,region,day,status,amount_cents FROM expected_variants "
            "WHERE parameter_id=" +
                std::to_string(i) + " ORDER BY source_pos");
        hashes.push_back(Digest(outputs[i]));
        counts.push_back(outputs[i].get_row_count());
    }

    // Ownership check: mutating one variant must not change clean or its peers;

    // destroying clean must not invalidate the independently owned output cells.
    if (outputs[0].get_row_count()) {
        outputs[0].cell_set(0, 1u, View("changed independently"));
        Require(Digest(prepared->clean) == cleanHash, "variant aliases clean table");
        for (std::size_t i = 1; i < outputs.size(); ++i) {
            Require(Digest(outputs[i]) == hashes[i], "variants alias each other");
        }
    }
    prepared.reset();
    for (std::size_t i = 1; i < outputs.size(); ++i) {
        Require(Digest(outputs[i]) == hashes[i], "output did not own its cells");
    }
    std::cout << "{\"implementation\":\"" << implementation << "\",\"verified\":true,\"sqlite\":\""
              << sqlite3_libversion() << "\",\"workers\":" << workers << ",\"counts\":";
    PrintArray(counts);
    std::cout << ",\"digests\":";
    PrintArray(hashes);
    std::cout << "}\n";
}

template <class T> void Escape(const T& value) {
#if defined(__clang__) || defined(__GNUC__)
    asm volatile("" : : "g"(&value) : "memory");
#else
    std::atomic_signal_fence(std::memory_order_seq_cst);
#endif
}

unsigned Number(const char* text) {
    std::size_t end = 0;
    const auto value = std::stoul(text, &end);
    Require(end == std::string_view(text).size() && value > 0 && value <= 100000,
        "invalid positive number");

    return static_cast<unsigned>(value);
}
} // namespace

int main(int argc, char** argv) {
    try {
        Require(argc == 6, "usage: gd_order_workflow DATABASE WORKERS "
                           "verify|import|prepare|variants|complete SAMPLES native|sorted");
        const auto workers = Number(argv[2]), samples = Number(argv[4]);
        Require(workers <= 256, "too many workers");
        const std::string stage = argv[3], index = argv[5];
        Require(index == "native" || index == "sorted", "invalid index mode");

        // Both C++ modes use GD's sorted index. "sorted" selects the alternative
        // index only in the Rust diagnostic; it leaves this C++ workload unchanged.
        // Connection setup, variant parameters, and pool creation are never timed.
        Database db;
        Check(db.open(argv[1]));
        Check(db.execute("PRAGMA query_only=ON; PRAGMA cache_size=-65536;"));
        const auto p = ReadParameters(db);
        BatchPool pool(workers);
        if (stage == "verify") {
            Verify(db, p, pool, workers);

            return 0;
        }

        // Dataset metadata for the report is also read outside timing.
        auto countQuery = Query(db, "SELECT count(*) FROM lines");
        Require(sqlite3_step(countQuery.get()) == SQLITE_ROW, "count query failed");
        const auto rows = sqlite3_column_int64(countQuery.get(), 0);
        countQuery.reset();
        std::optional<Inputs> input;
        std::optional<Prepared> prepared;

        // Isolated "prepare" reuses imported input; isolated "variants" also
        // reuses audit/clean. "complete" recreates all of them on each iteration.
        if (stage == "prepare" || stage == "variants") {
            input.emplace(Load(db));
        }
        if (stage == "variants") {
            prepared.emplace(Prepare(*input));
        }
        std::vector<double> seconds;

        // Iteration zero is a discarded warmup with the same work and lifetimes
        // as the recorded samples. Variant stages wait for the full worker batch.
        for (unsigned iteration = 0; iteration <= samples; ++iteration) {
            const auto start = std::chrono::steady_clock::now();
            if (stage == "import") {
                // Import and destroy the three native input tables.
                const auto result = Load(db);
                Escape(result);
            } else if (stage == "prepare") {
                // Join, validate, materialize audit/clean, then destroy them.
                const auto result = Prepare(*input);
                Escape(result);
            } else if (stage == "variants") {
                // Produce all eight parameterized subsets and destroy outputs.
                const auto result = pool.Run(prepared->clean, p);
                Escape(result);
            } else if (stage == "complete") {
                // Import -> prepare -> eight variants. All input/intermediate/
                // output tables are destroyed when this branch's scope ends.
                const auto loaded = Load(db);
                const auto result = Prepare(loaded);
                const auto outputs = pool.Run(result.clean, p);
                Escape(outputs);
                Escape(result);
                Escape(loaded);
            } else {
                throw std::runtime_error("unknown stage");
            }

            // Branch-local destructors have run: allocation, copying, and
            // destruction are included; JSON reporting below is outside timing.
            const auto elapsed =
                std::chrono::duration<double>(std::chrono::steady_clock::now() - start).count();
            if (iteration) {
                seconds.push_back(elapsed);
            }
        }
        std::cout << std::setprecision(10) << "{\"implementation\":\"" << implementation
                  << "\",\"rows\":" << rows << ",\"workers\":" << workers << ",\"stage\":\""
                  << stage << "\",\"index\":\"" << index << "\",\"sqlite\":\""
                  << sqlite3_libversion() << "\",\"seconds\":";
        PrintArray(seconds);
        std::cout << "}\n";
    } catch (const std::exception& error) {
        std::cerr << error.what() << '\n';

        return 1;
    }
}
