#include <benchmark/benchmark.h>

#include <algorithm>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <limits>
#include <stdexcept>
#include <string>
#include <string_view>
#include <tuple>
#include <vector>

#include "gd_table_column-buffer.h"

namespace {

using Column = std::tuple<std::string_view, unsigned, std::string_view>;
using Table = gd::table::table_column_buffer;

constexpr std::size_t kDefaultSourceRows = 1'000'000;
constexpr std::size_t kDefaultCopyPercent = 25;
constexpr std::int64_t kLogicalRowBytes = 1 + 8 + 4 + 2;

template <typename Type> Type Setting(const char* name, Type defaultValue) {
    const auto* text = std::getenv(name);
    if (text == nullptr) {
        return defaultValue;
    }
    char* end = nullptr;
    const auto parsed = std::strtoull(text, &end, 10);
    if (*text == '\0' || *end != '\0' || parsed > std::numeric_limits<Type>::max()) {
        throw std::runtime_error(std::string("invalid ") + name + "=" + text);
    }
    return static_cast<Type>(parsed);
}

struct CopyConfig {
    std::size_t sourceRows;
    std::size_t copiedRows;
    std::size_t copyPercent;
};

CopyConfig GetCopyConfig() {
    const auto sourceRows = Setting("GD_TABLE_COPY_SOURCE_ROWS", kDefaultSourceRows);
    const auto copyPercent = Setting("GD_TABLE_COPY_PERCENT", kDefaultCopyPercent);
    if (sourceRows == 0) {
        throw std::runtime_error("GD_TABLE_COPY_SOURCE_ROWS must be greater than zero");
    }
    if (copyPercent == 0 || copyPercent > 100) {
        throw std::runtime_error("GD_TABLE_COPY_PERCENT must be in 1..=100");
    }
    if (sourceRows > std::numeric_limits<unsigned>::max()) {
        throw std::runtime_error("GD_TABLE_COPY_SOURCE_ROWS exceeds the C++ GD row-capacity type");
    }
    if (sourceRows > std::numeric_limits<std::size_t>::max() / copyPercent) {
        throw std::runtime_error("table-copy row calculation overflowed");
    }
    const auto copiedRows = sourceRows * copyPercent / 100;
    if (copiedRows == 0) {
        throw std::runtime_error("the configured percentage must select at least one row");
    }
    return {sourceRows, copiedRows, copyPercent};
}

Table MakeSource(std::size_t rows) {
    Table table(static_cast<unsigned>(rows));
    table.column_add(std::vector<Column>{{"uint8", 0, "u8_value"}, {"uint64", 0, "u64_value"},
                         {"float", 0, "f32_value"}, {"uint16", 0, "u16_value"}},
        gd::table::tag_type_name{});
    const auto prepared = table.prepare();
    if (!prepared.first) {
        throw std::runtime_error(prepared.second);
    }
    for (std::size_t row = 0; row < rows; ++row) {
        table.row_add({gd::variant_view(static_cast<std::uint8_t>(row)),
            gd::variant_view(static_cast<std::uint64_t>(row) * 48'271U),
            gd::variant_view(static_cast<float>(row % 1'000'003U) * 0.5F),
            gd::variant_view(static_cast<std::uint16_t>(row))});
    }
    return table;
}

std::uint64_t NextRandom(std::uint64_t& state) {
    state ^= state << 13U;
    state ^= state >> 7U;
    state ^= state << 17U;
    return state;
}

std::vector<std::uint64_t> RandomRows(std::size_t sourceRows, std::size_t copiedRows) {
    std::vector<std::uint64_t> rows(sourceRows);
    for (std::size_t row = 0; row < sourceRows; ++row) {
        rows[row] = static_cast<std::uint64_t>(row);
    }
    std::uint64_t state = 0x9e37'79b9'7f4a'7c15ULL;
    for (std::size_t upper = rows.size() - 1; upper > 0; --upper) {
        const auto selected = static_cast<std::size_t>(NextRandom(state) % (upper + 1));
        std::swap(rows[upper], rows[selected]);
    }
    rows.resize(copiedRows);
    std::sort(rows.begin(), rows.end());
    return rows;
}

void VerifyRows(
    const Table& source, const Table& target, const std::vector<std::uint64_t>& sourceRows) {
    if (target.get_row_count() != sourceRows.size()) {
        std::abort();
    }
    for (std::size_t targetRow = 0; targetRow < sourceRows.size(); ++targetRow) {
        for (unsigned column = 0; column < 4; ++column) {
            if (source.cell_get_variant_view(sourceRows[targetRow], column) !=
                target.cell_get_variant_view(targetRow, column)) {
                std::abort();
            }
        }
    }
}

Table CopyRangeRaw(const Table& source, std::uint64_t start, std::uint64_t count) {
    Table target(source, gd::table::tag_columns{});
    target.row_add(count);
    std::memcpy(target.row_get(0), source.row_get(start), count * source.size_row());
    return target;
}

Table CopyRowsRaw(const Table& source, const std::vector<std::uint64_t>& sourceRows) {
    Table target(source, gd::table::tag_columns{});
    target.row_add(sourceRows.size());
    const auto rowBytes = source.size_row();
    for (std::size_t targetRow = 0; targetRow < sourceRows.size(); ++targetRow) {
        std::memcpy(target.row_get(targetRow), source.row_get(sourceRows[targetRow]), rowBytes);
    }
    return target;
}

void SetCopyCounters(benchmark::State& state, const CopyConfig& config, unsigned physicalRowBytes) {
    state.SetItemsProcessed(state.iterations() * static_cast<std::int64_t>(config.copiedRows));
    state.SetBytesProcessed(
        state.iterations() * static_cast<std::int64_t>(config.copiedRows) * kLogicalRowBytes);
    state.counters["source_rows"] = static_cast<double>(config.sourceRows);
    state.counters["selected_rows"] = static_cast<double>(config.copiedRows);
    state.counters["copy_percent"] = static_cast<double>(config.copyPercent);
    state.counters["physical_row_bytes"] = static_cast<double>(physicalRowBytes);
}

void RangeConstructor(benchmark::State& state) {
    const auto config = GetCopyConfig();
    const auto source = MakeSource(config.sourceRows);
    const auto start = (config.sourceRows - config.copiedRows) / 2;
    for (auto _ : state) {
        Table target(source, start, config.copiedRows);
        benchmark::DoNotOptimize(target);
    }
    SetCopyCounters(state, config, source.size_row());
}

BENCHMARK(RangeConstructor)->Name("TableCopy/GD/Range/Constructor");

void RangeRawPayload(benchmark::State& state) {
    const auto config = GetCopyConfig();
    const auto source = MakeSource(config.sourceRows);
    const auto start = (config.sourceRows - config.copiedRows) / 2;
    for (auto _ : state) {
        auto target = CopyRangeRaw(source, start, config.copiedRows);
        benchmark::DoNotOptimize(target);
    }
    SetCopyCounters(state, config, source.size_row());
}

BENCHMARK(RangeRawPayload)->Name("TableCopy/GD/Range/RawPayload");

void RandomConstructor(benchmark::State& state) {
    const auto config = GetCopyConfig();
    const auto source = MakeSource(config.sourceRows);
    const auto sourceRows = RandomRows(config.sourceRows, config.copiedRows);
    for (auto _ : state) {
        Table target(source, sourceRows);
        benchmark::DoNotOptimize(target);
    }
    SetCopyCounters(state, config, source.size_row());
}

BENCHMARK(RandomConstructor)->Name("TableCopy/GD/Random/Constructor");

void RandomRawPayload(benchmark::State& state) {
    const auto config = GetCopyConfig();
    const auto source = MakeSource(config.sourceRows);
    const auto sourceRows = RandomRows(config.sourceRows, config.copiedRows);
    for (auto _ : state) {
        auto target = CopyRowsRaw(source, sourceRows);
        benchmark::DoNotOptimize(target);
    }
    SetCopyCounters(state, config, source.size_row());
}

BENCHMARK(RandomRawPayload)->Name("TableCopy/GD/Random/RawPayload");

struct VerifyFixtures {
    VerifyFixtures() {
        const auto source = MakeSource(256);
        std::vector<std::uint64_t> rangeRows(64);
        for (std::size_t row = 0; row < rangeRows.size(); ++row) {
            rangeRows[row] = row + 96;
        }
        const auto selectedRows = RandomRows(256, 64);
        VerifyRows(source, Table(source, 96, 64), rangeRows);
        VerifyRows(source, CopyRangeRaw(source, 96, 64), rangeRows);
        VerifyRows(source, Table(source, selectedRows), selectedRows);
        VerifyRows(source, CopyRowsRaw(source, selectedRows), selectedRows);
    }
};

const VerifyFixtures kVerifiedFixtures;

} // namespace
