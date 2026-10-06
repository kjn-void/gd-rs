#include <benchmark/benchmark.h>

#include <cstdint>
#include <cstdlib>
#include <limits>
#include <sstream>
#include <stdexcept>
#include <string>
#include <string_view>
#include <vector>

#include "database/gd_database_io.h"
#include "gd_database_sqlite.h"

namespace {

constexpr std::size_t kDefaultRowsPerTable = 1'000'000;
constexpr std::uint64_t kDefaultSchemaSeed = 0x6a09e667f3bcc909ULL;
constexpr std::size_t kTableCount = 3;

enum class NumericType : std::uint8_t { I8, I16, I32, I64, U8, U16, U32, U64, F32, F64 };

struct TableSpec {
    std::string name;
    std::vector<NumericType> columns;
};

void Check(const std::pair<bool, std::string>& result) {
    if (!result.first) {
        throw std::runtime_error(result.second);
    }
}

std::uint64_t NextRandom(std::uint64_t& state) {
    state ^= state << 13U;
    state ^= state >> 7U;
    state ^= state << 17U;
    return state;
}

std::vector<TableSpec> TableSpecs(std::uint64_t seed) {
    std::vector<TableSpec> specs;
    specs.reserve(kTableCount);
    for (std::size_t table = 0; table < kTableCount; ++table) {
        const auto columnCount = 3U + static_cast<std::size_t>(NextRandom(seed) % 3U);
        TableSpec spec{"table_" + std::to_string(table), {}};
        spec.columns.reserve(columnCount);
        for (std::size_t column = 0; column < columnCount; ++column) {
            spec.columns.push_back(static_cast<NumericType>(NextRandom(seed) % 10U));
        }
        specs.push_back(std::move(spec));
    }
    return specs;
}

const char* Declaration(NumericType type) {
    switch (type) {
    case NumericType::I8:
        return "INTEGER_I8";
    case NumericType::I16:
        return "INTEGER_I16";
    case NumericType::I32:
        return "INTEGER_I32";
    case NumericType::I64:
        return "INTEGER_I64";
    case NumericType::U8:
        return "INTEGER_U8";
    case NumericType::U16:
        return "INTEGER_U16";
    case NumericType::U32:
        return "INTEGER_U32";
    case NumericType::U64:
        return "INTEGER_U64";
    case NumericType::F32:
        return "REAL_F32";
    case NumericType::F64:
        return "REAL_F64";
    }
    std::abort();
}

const char* TypeName(NumericType type) {
    switch (type) {
    case NumericType::I8:
        return "I8";
    case NumericType::I16:
        return "I16";
    case NumericType::I32:
        return "I32";
    case NumericType::I64:
        return "I64";
    case NumericType::U8:
        return "U8";
    case NumericType::U16:
        return "U16";
    case NumericType::U32:
        return "U32";
    case NumericType::U64:
        return "U64";
    case NumericType::F32:
        return "F32";
    case NumericType::F64:
        return "F64";
    }
    std::abort();
}

const char* Expression(NumericType type) {
    switch (type) {
    case NumericType::I8:
        return "((value % 255) - 127)";
    case NumericType::I16:
        return "((value % 65535) - 32767)";
    case NumericType::I32:
        return "((value * 48271 % 2000000001) - 1000000000)";
    case NumericType::I64:
        return "((value * 48271) - 24000)";
    case NumericType::U8:
        return "(value % 256)";
    case NumericType::U16:
        return "(value % 65536)";
    case NumericType::U32:
        return "(value * 48271 % 4000000000)";
    case NumericType::U64:
        return "(value * 48271)";
    case NumericType::F32:
        return "CAST((value % 1000003) * 0.5 AS REAL)";
    case NumericType::F64:
        return "CAST(value * 0.125 AS REAL)";
    }
    std::abort();
}

std::uint64_t ByteWidth(NumericType type) {
    switch (type) {
    case NumericType::I8:
    case NumericType::U8:
        return 1;
    case NumericType::I16:
    case NumericType::U16:
        return 2;
    case NumericType::I32:
    case NumericType::U32:
    case NumericType::F32:
        return 4;
    case NumericType::I64:
    case NumericType::U64:
    case NumericType::F64:
        return 8;
    }
    std::abort();
}

std::size_t RowSetting() {
    const char* text = std::getenv("GD_SQLITE_TO_TABLE_ROWS");
    if (text == nullptr) {
        return kDefaultRowsPerTable;
    }
    char* end = nullptr;
    const auto value = std::strtoull(text, &end, 10);
    if (*text == '\0' || *end != '\0' || value == 0 ||
        value > std::numeric_limits<std::size_t>::max()) {
        throw std::runtime_error(std::string("invalid GD_SQLITE_TO_TABLE_ROWS=") + text);
    }
    return static_cast<std::size_t>(value);
}

std::uint64_t SeedSetting() {
    const char* text = std::getenv("GD_SQLITE_TO_TABLE_SEED");
    if (text == nullptr) {
        return kDefaultSchemaSeed;
    }
    char* end = nullptr;
    const bool hexadecimal = text[0] == '0' && (text[1] == 'x' || text[1] == 'X');
    const auto value = std::strtoull(text, &end, hexadecimal ? 16 : 10);
    if (*text == '\0' || *end != '\0') {
        throw std::runtime_error(std::string("invalid GD_SQLITE_TO_TABLE_SEED=") + text);
    }
    return value;
}

std::string SchemaLabel(const std::vector<TableSpec>& specs) {
    std::ostringstream label;
    for (std::size_t table = 0; table < specs.size(); ++table) {
        if (table != 0) {
            label << ';';
        }
        label << specs[table].name << '=';
        for (std::size_t column = 0; column < specs[table].columns.size(); ++column) {
            if (column != 0) {
                label << ',';
            }
            label << TypeName(specs[table].columns[column]);
        }
    }
    return label.str();
}

std::string FixtureSql(const TableSpec& spec, std::size_t rows) {
    std::ostringstream sql;
    sql << "CREATE TABLE " << spec.name << '(';
    for (std::size_t column = 0; column < spec.columns.size(); ++column) {
        if (column != 0) {
            sql << ", ";
        }
        sql << "column_" << column << ' ' << Declaration(spec.columns[column]) << " NOT NULL";
    }
    sql << "); WITH RECURSIVE sequence(value) AS (VALUES(0) UNION ALL "
           "SELECT value + 1 FROM sequence WHERE value + 1 < "
        << rows << ") INSERT INTO " << spec.name << " SELECT ";
    for (std::size_t column = 0; column < spec.columns.size(); ++column) {
        if (column != 0) {
            sql << ", ";
        }
        sql << Expression(spec.columns[column]);
    }
    sql << " FROM sequence;";
    return sql.str();
}

class SqliteFixture {
public:
    SqliteFixture(std::size_t rows, const std::vector<TableSpec>& specs) {
        Check(database.open(":memory:"));
        Check(database.execute("PRAGMA synchronous=OFF; PRAGMA temp_store=MEMORY;"));
        for (const auto& spec : specs) {
            Check(database.execute(FixtureSql(spec, rows)));
        }
    }

    gd::database::sqlite::database database;
};

std::vector<gd::table::dto::table> Materialize(
    SqliteFixture& fixture, const std::vector<TableSpec>& specs, std::size_t rows) {
    std::vector<gd::table::dto::table> tables;
    tables.reserve(specs.size());
    for (const auto& spec : specs) {
        gd::database::sqlite::cursor_i cursor(&fixture.database);
        Check(cursor.open("SELECT * FROM " + spec.name));
        gd::table::dto::table table;
        Check(gd::database::to_table(&cursor, &table));
        if (table.get_row_count() != rows) {
            throw std::runtime_error("GD row count mismatch");
        }
        tables.push_back(std::move(table));
    }
    return tables;
}

void ToTableThreeDeclaredTables(benchmark::State& state) {
    const auto rows = RowSetting();
    const auto seed = SeedSetting();
    const auto specs = TableSpecs(seed);
    SqliteFixture fixture(rows, specs);
    std::uint64_t bytesPerThreeRows = 0;
    std::size_t columnCount = 0;
    for (const auto& spec : specs) {
        columnCount += spec.columns.size();
        for (const auto type : spec.columns) {
            bytesPerThreeRows += ByteWidth(type);
        }
    }

    for (auto _ : state) {
        auto tables = Materialize(fixture, specs, rows);
        benchmark::DoNotOptimize(tables);
    }
    state.SetItemsProcessed(state.iterations() * static_cast<std::int64_t>(rows * kTableCount));
    state.SetBytesProcessed(
        state.iterations() * static_cast<std::int64_t>(rows * bytesPerThreeRows));
    state.counters["columns"] = static_cast<double>(columnCount);
    state.counters["rows_per_table"] = static_cast<double>(rows);
    std::ostringstream label;
    label << "seed=0x" << std::hex << seed << ' ' << SchemaLabel(specs);
    state.SetLabel(label.str());
}

BENCHMARK(ToTableThreeDeclaredTables)->Name("SQLite/ToTable/ThreeTables/GD")->MinTime(10.0);

} // namespace
