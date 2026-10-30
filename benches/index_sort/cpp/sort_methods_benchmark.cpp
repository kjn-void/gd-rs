// Physical, index, and key sort of a GD table on each of its seven columns, after
// the upstream "three sort methods for table" benchmark described at
// https://www.sweclockers.com/forum/post/21314807.
//
// Unlike the six-case benchmark, every column is set, the table has 10,000 rows,
// and every column is sorted. Each iteration clears the target table, appends
// the source rows, and sorts, exactly like the upstream lambdas. A fourth
// method, not upstream, sorts row numbers with std::sort and a typed comparator
// from ../../cpp-reference/gd_table_row_iterator.h, then materializes a new
// table with the rows in that order.
//
// The key sort is GD's tag_sort_key overload as posted in the forum. GD has not
// published it yet (upstream main is still a5b1e3e), so SortKey() below is that
// function rewritten against GD's public row accessors. Replace it with
// sort(uColumn, true, gd::table::tag_sort_key{}) once GD publishes it.

#include <benchmark/benchmark.h>

#include <algorithm>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <iostream>
#include <numeric>
#include <random>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

#include "gd_table_column-buffer.h"
#include "gd_table_row_iterator.h"

namespace {

using gd::table::dto::table;

constexpr std::int64_t kRowCount = 10'000;

enum class Method { physical, index, key, std_sort };

table MakeEmptyTable() {
    return table(
        (table::eTableFlagNull32 | table::eTableFlagRowStatus),
        {{"uint64", 0, "C1"},
         {"string", 8, "C2"},
         {"double", 0, "C3"},
         {"double", 0, "C4"},
         {"int32", 0, "C5"},
         {"uint32", 0, "C6"},
         {"string", 50, "C7"}},
        gd::table::tag_prepare{});
}

// Same generator, distributions, and draw order as the upstream test.
table MakeSourceTable() {
    std::mt19937_64 generatorRandom(42);
    std::uniform_int_distribution<std::uint64_t> distributionInteger(0, 1'000'000);
    std::uniform_real_distribution<double> distributionDouble(0.0, 1'000.0);
    std::uniform_int_distribution<int> distributionCharacter('a', 'z');

    table tableNew = MakeEmptyTable();
    tableNew.reserve(static_cast<std::uint64_t>(kRowCount));
    for (std::int64_t i = 0; i < kRowCount; ++i) {
        auto uRow = tableNew.row_add_one();

        std::string stringValue(8, ' ');
        std::string stringValue2(25, ' ');
        for (auto& iCharacter : stringValue) {
            iCharacter = static_cast<char>(distributionCharacter(generatorRandom));
        }
        for (auto& iCharacter : stringValue2) {
            iCharacter = static_cast<char>(distributionCharacter(generatorRandom));
        }

        std::string stringValue6 = "01234567890123456789";
        stringValue6 += stringValue2;

        tableNew.cell_set(uRow, 0u, distributionInteger(generatorRandom));
        tableNew.cell_set(uRow, 1u, std::string_view(stringValue));
        tableNew.cell_set(uRow, 2u, distributionDouble(generatorRandom));
        tableNew.cell_set(uRow, 3u, distributionDouble(generatorRandom));
        tableNew.cell_set(uRow, 4u, static_cast<std::int32_t>(distributionInteger(generatorRandom) % 1000));
        tableNew.cell_set(uRow, 5u, static_cast<std::uint32_t>(distributionInteger(generatorRandom) % 1000));
        tableNew.cell_set(uRow, 6u, std::string_view(stringValue6));
    }
    return tableNew;
}

const table& SourceTable() {
    static const table tableSource = MakeSourceTable();
    return tableSource;
}

// GD's posted table_column_buffer::sort(..., tag_sort_key), ascending only: read
// each key once, stable_sort the (key, row) pairs, then gather the rows in order
// into a buffer and copy it back in one block, and the same for row metadata.
void SortKey(table& tableSort, unsigned uColumn) {
    const std::uint64_t uCount = tableSort.size();
    if (uCount < 2) return;

    std::vector<std::pair<gd::variant_view, std::uint64_t>> vectorKey;
    vectorKey.reserve(uCount);
    for (std::uint64_t uRow = 0; uRow < uCount; uRow++) {
        vectorKey.emplace_back(tableSort.cell_get_variant_view(uRow, uColumn), uRow);
    }
    std::stable_sort(vectorKey.begin(), vectorKey.end(),
                     [](const auto& pairLeft, const auto& pairRight) { return pairLeft.first < pairRight.first; });

    // Keys for strings point into the row buffer, so they are not used after this.
    const std::uint64_t uRowSize = tableSort.size_row();
    std::vector<std::uint8_t> vectorData(uCount * uRowSize);
    for (std::uint64_t uPosition = 0; uPosition < uCount; uPosition++) {
        std::memcpy(vectorData.data() + uPosition * uRowSize, tableSort.row_get(vectorKey[uPosition].second), uRowSize);
    }
    std::memcpy(tableSort.row_get(0), vectorData.data(), vectorData.size());

    if (tableSort.is_rowmeta()) {
        const std::uint64_t uMetaSize = tableSort.size_row_meta();
        std::vector<std::uint8_t> vectorMeta(uCount * uMetaSize);
        for (std::uint64_t uPosition = 0; uPosition < uCount; uPosition++) {
            std::memcpy(vectorMeta.data() + uPosition * uMetaSize, tableSort.row_get_meta(vectorKey[uPosition].second), uMetaSize);
        }
        std::memcpy(tableSort.row_get_meta(0), vectorMeta.data(), vectorMeta.size());
    }
}

// Not upstream: std::sort over row numbers, comparing keys read straight from the
// row bytes, then a new table with the rows and their metadata copied in sorted
// order. Returns the new table; tableSort is left unchanged.
table SortIndexMaterialize(const table& tableSort, unsigned uColumn) {
    const std::uint64_t uCount = tableSort.size();
    std::vector<std::uint64_t> vectorIndex(uCount);
    std::iota(vectorIndex.begin(), vectorIndex.end(), std::uint64_t{0});
    table_rows::WithLess(tableSort, uColumn, [&](auto less) {
        std::sort(vectorIndex.begin(), vectorIndex.end(), [&](std::uint64_t uLeft, std::uint64_t uRight) {
            return less.Key(tableSort.row_get(uLeft)) < less.Key(tableSort.row_get(uRight));
        });
    });

    table tableSorted = MakeEmptyTable();
    tableSorted.reserve(uCount);
    tableSorted.set_row_count(uCount);
    const std::uint64_t uRowSize = tableSort.size_row();
    const std::uint64_t uMetaSize = tableSort.is_rowmeta() ? tableSort.size_row_meta() : 0;
    for (std::uint64_t uPosition = 0; uPosition < uCount; uPosition++) {
        std::memcpy(tableSorted.row_get(uPosition), tableSort.row_get(vectorIndex[uPosition]), uRowSize);
        if (uMetaSize != 0) {
            std::memcpy(tableSorted.row_get_meta(uPosition), tableSort.row_get_meta(vectorIndex[uPosition]), uMetaSize);
        }
    }
    return tableSorted;
}

void RequireSorted(const table& tableSorted, unsigned uColumn) {
    for (std::uint64_t uRow = 1; uRow < tableSorted.size(); ++uRow) {
        auto valuePrev = tableSorted.cell_get_variant_view(uRow - 1, uColumn);
        auto valueCurr = tableSorted.cell_get_variant_view(uRow, uColumn);
        if (valueCurr < valuePrev) {
            std::cerr << "column " << uColumn << " is not sorted at row " << uRow << '\n';
            std::abort();
        }
    }
}

// Argument: sort column.
template <Method kMethod>
void SortTable(benchmark::State& state) {
    const auto uColumn = static_cast<unsigned>(state.range(0));
    const table& tableSource = SourceTable();
    table tableTarget = MakeEmptyTable();
    tableTarget.reserve(tableSource.size());

    for (auto _ : state) {
        tableTarget.row_clear();
        tableTarget.append(tableSource);
        if constexpr (kMethod == Method::physical) {
            tableTarget.sort(uColumn, true);
        } else if constexpr (kMethod == Method::index) {
            tableTarget.sort(uColumn, true, gd::table::tag_sort_index{});
        } else if constexpr (kMethod == Method::key) {
            SortKey(tableTarget, uColumn);
        } else {
            tableTarget = SortIndexMaterialize(tableTarget, uColumn);
        }
        benchmark::DoNotOptimize(tableTarget.size());
        benchmark::ClobberMemory();
    }

    RequireSorted(tableTarget, uColumn);
    state.SetItemsProcessed(state.iterations() * kRowCount);
}

const bool bRegistered = [] {
    // Upstream order: for each column, sort table 01 (physical), 02 (index), 03 (key);
    // then std::sort over an index into a new table, which is not upstream.
    for (int iColumn = 0; iColumn < 7; ++iColumn) {
        benchmark::RegisterBenchmark("physical", SortTable<Method::physical>)->Arg(iColumn)->Unit(benchmark::kMicrosecond);
        benchmark::RegisterBenchmark("index", SortTable<Method::index>)->Arg(iColumn)->Unit(benchmark::kMicrosecond);
        benchmark::RegisterBenchmark("key", SortTable<Method::key>)->Arg(iColumn)->Unit(benchmark::kMicrosecond);
        benchmark::RegisterBenchmark("std_sort", SortTable<Method::std_sort>)->Arg(iColumn)->Unit(benchmark::kMicrosecond);
    }
    return true;
}();

}  // namespace

BENCHMARK_MAIN();
