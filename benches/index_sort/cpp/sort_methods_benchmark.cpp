// Physical, index, and key sort of a GD table on each of its seven columns, after
// the upstream "three sort methods for table" benchmark described at
// https://www.sweclockers.com/forum/post/21314807.
//
// Unlike the six-case benchmark, every column is set, the table has 10,000 rows
// (or INDEX_SORT_ROWS), and every column is sorted. Each GD iteration clears the
// target, appends the source rows, and sorts, like the upstream lambdas. A fourth
// method, not upstream, sorts row numbers with std::sort and a typed comparator
// from ../../cpp-reference/gd_table_row_iterator.h, then materializes a new
// table with the rows in that order.
// The soa_rows cases use standard vectors, std::string and std::optional cells.
// They cache typed keys through a const row iterator, sort the key/index pairs,
// and gather directly from the source into reusable output columns.
//
// The key sort is GD's updated tag_sort_key from forum post 21315112. GD has not
// published it yet (upstream main is still a5b1e3e), so the maintained
// gd_table_key_sort.h adapts it to GD's public row accessors. Replace it with
// sort(uColumn, true, gd::table::tag_sort_key{}) once GD publishes it.

#include <benchmark/benchmark.h>

#include <algorithm>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <iostream>
#include <numeric>
#include <optional>
#include <random>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

#include "gd_table_column-buffer.h"
#include "gd_table_row_iterator.h"
#include "gd_table_key_sort.h"
#include "soa_table_row_iterator.h"
#include "soa_table_key_sort.h"

namespace {

using gd::table::dto::table;

constexpr std::int64_t kRowCount = 10'000;
// Above this, GD's O(n^2) selection sort is not registered: at 10^7 rows it would
// take about a day per column.
constexpr std::int64_t kPhysicalRowLimit = 100'000;

// INDEX_SORT_ROWS, or kRowCount when it is not set.
std::int64_t RowCount() {
    static const std::int64_t iRowCount = [] {
        const char* pszRows = std::getenv("INDEX_SORT_ROWS");
        return pszRows != nullptr ? std::stoll(pszRows) : kRowCount;
    }();
    return iRowCount;
}

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
    tableNew.reserve(static_cast<std::uint64_t>(RowCount()));
    for (std::int64_t i = 0; i < RowCount(); ++i) {
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

using SoaTable = soa_rows::Table<std::uint64_t, std::string, double,
                                std::optional<double>, std::optional<std::int32_t>,
                                std::optional<std::uint32_t>, std::string>;

const SoaTable& SoaSourceTable() {
    // Convert outside timing so the SoA case uses exactly the GD fixture values.
    static const SoaTable source = [] {
        SoaTable result;
        const auto& gdSource = SourceTable();
        result.reserve(gdSource.size());
        for (std::uint64_t row = 0; row < gdSource.size(); ++row) {
            result.push_back({gdSource.cell_get_variant_view(row, 0u).as_uint64(),
                              std::string(gdSource.cell_get_variant_view(row, 1u).as_string_view()),
                              gdSource.cell_get_variant_view(row, 2u).as_double(),
                              gdSource.cell_get_variant_view(row, 3u).as_double(),
                              gdSource.cell_get_variant_view(row, 4u).as_int32(),
                              gdSource.cell_get_variant_view(row, 5u).as_uint32(),
                              std::string(gdSource.cell_get_variant_view(row, 6u).as_string_view())});
        }
        return result;
    }();
    return source;
}

template <std::size_t Column>
void SortSoaTable(benchmark::State& state) {
    const auto& source = SoaSourceTable();
    SoaTable target;
    target.reserve(source.size());
    soa_rows::KeySortWorkspace<Column, SoaTable> workspace(source.size());
    for (auto _ : state) {
        workspace.sort_into(source, target);
        benchmark::DoNotOptimize(target);
        benchmark::ClobberMemory();
    }
    if (!std::is_sorted(target.begin(), target.end(), soa_rows::Less<Column>{})) {
        std::cerr << "SoA column " << Column << " is not sorted\n";
        std::abort();
    }
    state.SetItemsProcessed(state.iterations() * RowCount());
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
            table_key_sort::Sort(tableTarget, uColumn);
        } else {
            tableTarget = SortIndexMaterialize(tableTarget, uColumn);
        }
        benchmark::DoNotOptimize(tableTarget.size());
        benchmark::ClobberMemory();
    }

    RequireSorted(tableTarget, uColumn);
    state.SetItemsProcessed(state.iterations() * RowCount());
}

const bool bRegistered = [] {
    // Upstream order: for each column, sort table 01 (physical), 02 (index), 03 (key);
    // then std::sort over an index into a new table, which is not upstream.
    for (int iColumn = 0; iColumn < 7; ++iColumn) {
        if (RowCount() <= kPhysicalRowLimit) {
            benchmark::RegisterBenchmark("physical", SortTable<Method::physical>)->Arg(iColumn)->Unit(benchmark::kMicrosecond);
        }
        benchmark::RegisterBenchmark("index", SortTable<Method::index>)->Arg(iColumn)->Unit(benchmark::kMicrosecond);
        benchmark::RegisterBenchmark("key", SortTable<Method::key>)->Arg(iColumn)->Unit(benchmark::kMicrosecond);
        benchmark::RegisterBenchmark("std_sort", SortTable<Method::std_sort>)->Arg(iColumn)->Unit(benchmark::kMicrosecond);
    }
    []<std::size_t... Columns>(std::index_sequence<Columns...>) {
        (benchmark::RegisterBenchmark("soa_rows", SortSoaTable<Columns>)->Arg(Columns)->Unit(benchmark::kMicrosecond), ...);
    }(std::make_index_sequence<7>{});
    return true;
}();

}  // namespace

BENCHMARK_MAIN();
